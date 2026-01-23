use crate::adapters::{InMemoryDirectoryAdapter, SystemClockAdapter};
use crate::core::parameter_types::AddNodesParams;
use crate::core::ports::message_receiver_port::Identity;
use crate::core::CoordinatorCore;
use jsonrpsee_types::request::Request;
use jsonrpsee_types::Params;
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
    /// Add new nodes.
    AddNodes(Vec<String>),
    /// The coordinator should shut down.
    Shutdown,
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

    /// Handle a JSON-RPC message, which could be a single request or a batch request
    pub fn handle_jsonrpc_message(
        &mut self,
        identity: Identity,
        message: &MessageView,
        content_frame: &[u8],
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        // First, try to parse as a JSON value to determine if it's a batch
        let json_value: serde_json::Value = match serde_json::from_slice(content_frame) {
            Ok(value) => value,
            Err(_) => {
                match message.sender() {
                    Ok(sender_name) => {
                        let error_message = self.create_error_response(
                            sender_name,
                            &Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)),
                            Some(message.header().conversation_id.clone()),
                        )?;
                        return Ok(vec![JsonRpcOutcome::Response(error_message)]);
                    }
                    Err(e) => {
                        eprintln!("Error: Malformed sender name in message, cannot send error response: {:?}", e);
                        return Ok(vec![]);
                    }
                }
            }
        };

        // Check if it's a batch request (array) or single request (object)
        match json_value {
            serde_json::Value::Array(requests) => {
                self.handle_batch_request(identity, message, requests)
            }
            serde_json::Value::Object(_) => {
                // Handle single request by converting back to bytes and parsing as Request
                let request_bytes = serde_json::to_vec(&json_value)?;
                match serde_json::from_slice::<Request>(&request_bytes) {
                    Ok(request) => self.handle_request(identity, message, request),
                    Err(_) => match message.sender() {
                        Ok(sender_name) => {
                            let error_message = self.create_error_response(
                                sender_name,
                                &Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)),
                                Some(message.header().conversation_id.clone()),
                            )?;
                            Ok(vec![JsonRpcOutcome::Response(error_message)])
                        }
                        Err(e) => {
                            eprintln!("Error: Malformed sender name in message, cannot send error response: {:?}", e);
                            Ok(vec![])
                        }
                    },
                }
            }
            _ => {
                // Invalid JSON-RPC message
                match message.sender() {
                    Ok(sender_name) => {
                        let error_message = self.create_error_response(
                            sender_name,
                            &Error::JsonRpc(ErrorObject::from(ErrorCode::InvalidRequest)),
                            Some(message.header().conversation_id.clone()),
                        )?;
                        Ok(vec![JsonRpcOutcome::Response(error_message)])
                    }
                    Err(e) => {
                        eprintln!("Error: Malformed sender name in message, cannot send error response: {:?}", e);
                        Ok(vec![])
                    }
                }
            }
        }
    }

    /// Handle a batch of JSON-RPC requests
    fn handle_batch_request(
        &mut self,
        identity: Identity,
        message: &MessageView,
        requests: Vec<serde_json::Value>,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        if requests.is_empty() {
            // Per JSON-RPC 2.0 spec, an empty batch is an error
            match message.sender() {
                Ok(sender_name) => {
                    let error_message = self.create_error_response(
                        sender_name,
                        &Error::JsonRpc(ErrorObject::from(ErrorCode::InvalidRequest)),
                        Some(message.header().conversation_id.clone()),
                    )?;
                    return Ok(vec![JsonRpcOutcome::Response(error_message)]);
                }
                Err(e) => {
                    eprintln!(
                        "Error: Malformed sender name in message, cannot send error response: {:?}",
                        e
                    );
                    return Ok(vec![]);
                }
            }
        }

        let sender = self.extract_sender(message)?;
        let mut responses = Vec::new();
        let mut all_outcomes = Vec::new(); // Collect all outcomes

        // Process each request in the batch
        for request_value in requests {
            match request_value {
                serde_json::Value::Object(_) => {
                    // Convert the JSON value back to bytes and then parse as Request
                    let request_bytes = serde_json::to_vec(&request_value)?;
                    match serde_json::from_slice::<Request>(&request_bytes) {
                        Ok(request) => {
                            // Clone identity for each request since handle_request takes ownership
                            let outcomes =
                                self.handle_request(identity.clone(), message, request)?;

                            for outcome in outcomes {
                                match outcome {
                                    JsonRpcOutcome::Response(response_message) => {
                                        // Combine the responses from all requests
                                        if let Some(content_frame) =
                                            response_message.content_frame()
                                        {
                                            if let Ok(response_value) =
                                                serde_json::from_slice::<serde_json::Value>(
                                                    content_frame,
                                                )
                                            {
                                                responses.push(response_value);
                                            }
                                        }
                                    }
                                    JsonRpcOutcome::NoAction => {
                                        // No action at all
                                    }
                                    _ => {
                                        all_outcomes.push(outcome);
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            // Invalid request in batch - create an error response for it
                            let error_response = Response::<()>::new(
                                ResponsePayload::Error(ErrorObject::from(
                                    ErrorCode::InvalidRequest,
                                )),
                                Id::Null,
                            );
                            if let Ok(error_value) = serde_json::to_value(&error_response) {
                                responses.push(error_value);
                            }
                        }
                    }
                }
                _ => {
                    // Non-object in batch - create an error response for it
                    let error_response = Response::<()>::new(
                        ResponsePayload::Error(ErrorObject::from(ErrorCode::InvalidRequest)),
                        Id::Null,
                    );
                    if let Ok(error_value) = serde_json::to_value(&error_response) {
                        responses.push(error_value);
                    }
                }
            }
        }

        // If we have responses, create a batch response message
        if !responses.is_empty() {
            let batch_response = serde_json::to_vec(&responses)?;

            let response_message = MessageBuilder::new()
                .receiver(sender.clone())
                .sender(self.name.clone())
                .conversation_id(message.header().conversation_id.clone())
                .message_type(MessageType::Json.into())
                .payload_single(batch_response)
                .build()?;

            all_outcomes.insert(0, JsonRpcOutcome::Response(response_message.to_view()?));
        } else if all_outcomes.is_empty() {
            // No responses and no other outcomes means all were notifications
            all_outcomes.push(JsonRpcOutcome::NoAction);
        }

        Ok(all_outcomes)
    }

    /// Handle a JSON-RPC request.
    pub fn handle_request(
        &mut self,
        identity: Identity,
        message: &MessageView,
        request: Request,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        match request.method_name() {
            // Component methods
            "pong" => self.handle_pong(message, request.id),
            // Extended component
            "shut_down" => self.handle_shut_down(message, request.id()),
            // Coordinator methods
            "sign_in" => self.handle_sign_in(identity, message, request.id()),
            "sign_out" => self.handle_sign_out(identity, message, request.id()),
            "coordinator_sign_in" => {
                self.handle_coordinator_sign_in(identity, message, request.id())
            }
            "coordinator_sign_out" => self.handle_coordinator_sign_out(message, request.id()),
            "add_nodes" => self.handle_add_nodes(message, request.id(), request.params()),
            "send_nodes" => self.handle_send_nodes(message, request.id()),
            "record_components" => self.handle_record_components(message, request.id()),
            "send_local_components" => self.handle_send_local_components(message, request.id()),
            "send_global_components" => self.handle_send_global_components(message, request.id()),
            _ => {
                let sender = self.extract_sender(message)?;
                let error_message = self.create_error_response(
                    sender,
                    &Error::JsonRpc(ErrorObject::from(ErrorCode::MethodNotFound)),
                    Some(message.header().conversation_id.clone()),
                )?;
                Ok(vec![JsonRpcOutcome::Response(error_message)])
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

    /// Create a null JSON-RPC response outcome
    fn create_null_response_outcome(
        &self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        if id == Id::Null {
            return Ok(Vec::<JsonRpcOutcome>::new());
        } else {
            let response_message = self.create_null_response(message, id)?;
            Ok(vec![JsonRpcOutcome::Response(response_message)])
        }
    }

    // Individual method handlers
    fn handle_pong(
        &self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        self.create_null_response_outcome(message, id)
    }

    fn handle_sign_in(
        &mut self,
        identity: Identity,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;
        self.core.handle_sign_in(sender.clone(), identity)?;
        self.create_null_response_outcome(message, id)
    }

    fn handle_sign_out(
        &mut self,
        identity: Identity,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;
        self.core.handle_sign_out(identity, sender.clone())?;
        self.create_null_response_outcome(message, id)
    }

    /// Handle coordinator sign-in
    fn handle_coordinator_sign_in(
        &mut self,
        identity: Identity,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        // Extract sender (should be the coordinator signing in)
        let sender = self.extract_sender(message)?;

        // For coordinator sign-in, we expect the sender to be in format "namespace.COORDINATOR"
        if sender.name() != b"COORDINATOR" {
            let error_message = self.create_error_response(
                sender,
                &Error::duplicate_name(),
                Some(message.header().conversation_id.clone()),
            )?;

            return Ok(vec![JsonRpcOutcome::ResponseToIdentity((
                identity,
                error_message,
            ))]);
        };

        // Create success response
        let response_message = self.create_null_response(message, id)?;

        Ok(vec![JsonRpcOutcome::ResponseToIdentity((
            identity,
            response_message,
        ))])
    }

    fn handle_coordinator_sign_out(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        self.create_null_response_outcome(message, id)
    }

    fn handle_add_nodes(
        &mut self,
        message: &MessageView,
        id: Id,
        params: Params,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let nodes = params.parse::<AddNodesParams>()?;
        let addresses = self.core.handle_add_nodes(nodes);
        let response_outcome = self.create_null_response_outcome(message, id);
        let mut outcomes = match response_outcome {
            Ok(outcomes) => outcomes,
            Err(..) => Vec::new(),
        };
        match addresses {
            Some(addresses) => outcomes.push(JsonRpcOutcome::AddNodes(addresses)),
            None => (),
        };
        Ok(outcomes)
    }

    fn handle_send_nodes(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        self.create_null_response_outcome(message, id)
    }

    fn handle_record_components(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        self.create_null_response_outcome(message, id)
    }

    fn handle_send_local_components(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        self.create_null_response_outcome(message, id)
    }

    fn handle_send_global_components(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        self.create_null_response_outcome(message, id)
    }

    fn handle_shut_down(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        // Create the response message
        let response_message = self.create_null_response(message, id)?;

        Ok(vec![
            JsonRpcOutcome::Response(response_message),
            JsonRpcOutcome::Shutdown,
        ])
    }
}
