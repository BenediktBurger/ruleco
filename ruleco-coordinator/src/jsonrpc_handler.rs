use crate::adapters::{InMemoryDirectoryAdapter, SystemClockAdapter};
use crate::core::ports::message_receiver_port::Identity;
use crate::core::CoordinatorCore;
use jsonrpsee_types::request::Request;
use jsonrpsee_types::{
    response::{Response, ResponsePayload},
    ErrorCode, ErrorObject, Id,
};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use ruleco_core::message::{ConversationId, MessageBuilder, MessageView};
use ruleco_core::protocol_constants::MessageType;
use serde_json::Value;
use std::borrow::Cow;

/// Outcome of handling a JSON-RPC request.
pub enum JsonRpcOutcome {
    /// A response message should be sent back to the client.
    Response(MessageView),
    /// A response message should be sent, but to a specific identity.
    ResponseToIdentity((Identity, MessageView)),
    /// The coordinator should shut down.
    Shutdown(MessageView),
    /// No specific action is required (e.g., for notifications).
    NoAction,
}

/// Handler for JSON-RPC requests directed at the Coordinator.
pub struct JsonRpcHandler<'a> {
    /// Reference to the core coordinator logic.
    core: &'a mut CoordinatorCore<InMemoryDirectoryAdapter, SystemClockAdapter>,
    /// Our name as a FullName.
    name: &'a FullName,
}

impl<'a> JsonRpcHandler<'a> {
    /// Create a new JSON-RPC handler.
    pub fn new(
        core: &'a mut CoordinatorCore<InMemoryDirectoryAdapter, SystemClockAdapter>,
        name: &'a FullName,
    ) -> Self {
        Self { core, name }
    }

    /// Handle a JSON-RPC request.
    pub fn handle_request(
        &mut self,
        identity: Identity,
        message: &MessageView,
        request: Request,
    ) -> Result<JsonRpcOutcome, Box<dyn std::error::Error>> {
        match request.method_name() {
            // Component methods
            "pong" => self
                .handle_pong(message, request.id)
                .map(JsonRpcOutcome::Response),
            // Extended component
            "shut_down" => self
                .handle_shut_down(message, request.id())
                .map(JsonRpcOutcome::Shutdown),
            // Coordinator methods
            "sign_in" => self
                .handle_sign_in(identity, message, request.id())
                .map(JsonRpcOutcome::Response),
            "sign_out" => self
                .handle_sign_out(identity, message, request.id())
                .map(JsonRpcOutcome::Response),
            "coordinator_sign_in" => self
                .handle_coordinator_sign_in(identity, message, request.id())
                .map(JsonRpcOutcome::ResponseToIdentity),
            "coordinator_sign_out" => self
                .handle_coordinator_sign_out(message, request.id())
                .map(JsonRpcOutcome::Response),
            "add_nodes" => self
                .handle_add_nodes(message, request.id())
                .map(JsonRpcOutcome::Response),
            "send_nodes" => self
                .handle_send_nodes(message, request.id())
                .map(JsonRpcOutcome::Response),
            "record_components" => self
                .handle_record_components(message, request.id())
                .map(JsonRpcOutcome::Response),
            "send_local_components" => self
                .handle_send_local_components(message, request.id())
                .map(JsonRpcOutcome::Response),
            "send_global_components" => self
                .handle_send_global_components(message, request.id())
                .map(JsonRpcOutcome::Response),
            _ => {
                let sender = self.extract_sender(message)?;
                let error_message = self.create_error_response(
                    sender,
                    &Error::JsonRpc(ErrorObject::from(ErrorCode::MethodNotFound)),
                    Some(message.header().conversation_id.clone()),
                )?;
                Ok(JsonRpcOutcome::Response(error_message))
            }
        }
    }

    /// Extract sender from message with standard error handling
    fn extract_sender<'b>(&self, message: &'b MessageView) -> Result<&'b FullName, Error> {
        message.try_sender(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))
    }

    /// Create a standard null JSON-RPC response
    fn create_null_response(
        &self,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;
        self.create_json_response(
            sender,
            id,
            serde_json::Value::Null,
            Some(message.header().conversation_id.clone()),
        )
    }

    /// Create an error response message
    pub fn create_error_response(
        &self,
        recipient_name: &FullName,
        error: &Error,
        conversation_id: Option<ConversationId>,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let error_object: ErrorObject<'static> = match error {
            Error::Leco(leco_error) => leco_error.clone().into(),
            Error::JsonRpc(json_rpc_error) => json_rpc_error.clone(),
            Error::Custom(code, message) => ErrorObject::owned(*code, message.clone(), None::<()>),
        };

        let error_response = Response::<()>::new(ResponsePayload::Error(error_object), Id::Null);
        let error_msg = serde_json::to_vec(&error_response)?;

        let message = MessageBuilder::new()
            .receiver(recipient_name.clone())
            .sender(self.name.clone()) // Use self.name
            .conversation_id(conversation_id.unwrap_or_default())
            .message_type(1) // JSON message type
            .payload_single(error_msg)
            .build()?;

        Ok(message.to_view()?)
    }

    /// Create a JSON-RPC response message
    pub fn create_json_response(
        &self,
        recipient_name: &FullName,
        id: Id,
        result: Value,
        conversation_id: Option<ConversationId>,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let response = Response::new(ResponsePayload::Success(Cow::Borrowed(&result)), id);
        let response_msg = serde_json::to_vec(&response)?;

        let message = MessageBuilder::new()
            .receiver(recipient_name.clone())
            .sender(self.name.clone()) // Use self.name
            .conversation_id(conversation_id.unwrap_or_default())
            .message_type(MessageType::Json.into())
            .payload_single(response_msg)
            .build()?;

        Ok(message.to_view()?)
    }

    // Individual method handlers
    fn handle_pong(
        &self,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        self.create_null_response(message, id)
    }

    fn handle_sign_in(
        &mut self,
        identity: Identity,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;
        self.core.handle_sign_in(sender.clone(), identity)?;
        self.create_null_response(message, id)
    }

    fn handle_sign_out(
        &mut self,
        identity: Identity,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;
        self.core.handle_sign_out(identity, sender.clone())?;
        self.create_null_response(message, id)
    }

    /// Handle coordinator sign-in
    fn handle_coordinator_sign_in(
        &mut self,
        identity: Identity,
        message: &MessageView,
        id: Id,
    ) -> Result<(Identity, MessageView), Box<dyn std::error::Error>> {
        // Extract sender (should be the coordinator signing in)
        let sender = self.extract_sender(message)?;

        // For coordinator sign-in, we expect the sender to be in format "namespace.COORDINATOR"
        if sender.name() != b"COORDINATOR" {
            return self
                .create_error_response(
                    sender,
                    &Error::JsonRpc(ErrorObject::owned(
                        -32091, // Using duplicate name error code
                        "Invalid coordinator sign-in request".to_string(),
                        None::<()>,
                    )),
                    Some(message.header().conversation_id.clone()),
                )
                .map(|mess| (identity, mess));
        };

        // Return success response
        let message = self.create_null_response(message, id);
        let result = message.map(|mess| (identity, mess));
        result
    }

    fn handle_coordinator_sign_out(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        self.create_null_response(message, id)
    }

    fn handle_add_nodes(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        self.create_null_response(message, id)
    }

    fn handle_send_nodes(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        self.create_null_response(message, id)
    }

    fn handle_record_components(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        self.create_null_response(message, id)
    }

    fn handle_send_local_components(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        self.create_null_response(message, id)
    }

    fn handle_send_global_components(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        self.create_null_response(message, id)
    }

    fn handle_shut_down(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        // Note: Shut down logic will be handled by the CoordinatorApp
        // based on the JsonRpcOutcome::Shutdown variant.
        self.create_null_response(message, id)
    }
}
