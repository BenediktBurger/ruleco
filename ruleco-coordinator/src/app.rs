use crate::adapters::{InMemoryDirectoryAdapter, ProtocolAdapter, SystemClockAdapter, ZmqAdapter};
use crate::core::domain::RoutingDecision;
use crate::core::ports::{MessageSenderPort, RoutingPort};
use crate::core::CoordinatorCore;
use jsonrpsee_types::request::Request;
use jsonrpsee_types::{
    response::{Response, ResponsePayload},
    ErrorCode, ErrorObject, Id,
};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use ruleco_core::message::{ConversationId, MessageBuilder, MessageView};
use ruleco_core::protocol_constants::{self, MessageType};
use serde_json::Value;
use std::borrow::Cow;
use std::time::Duration;

/// The main coordinator application that orchestrates the components
pub struct CoordinatorApp {
    /// The core coordinator logic
    core: CoordinatorCore<InMemoryDirectoryAdapter, SystemClockAdapter>,
    /// The ZMQ adapter for message sending
    zmq_adapter: ZmqAdapter,
    /// Our name as a FullName
    name: FullName,
    /// Flag to indicate if the coordinator is running
    running: bool,
}

impl CoordinatorApp {
    /// Create a new coordinator application
    pub fn new(namespace: String, port: Option<u16>) -> Result<Self, Box<dyn std::error::Error>> {
        let port = port.unwrap_or(protocol_constants::DEFAULT_COORDINATOR_PORT);
        let namespace_bytes = namespace.as_bytes().to_vec();
        let directory_adapter = InMemoryDirectoryAdapter::new(namespace_bytes.clone());
        let clock_adapter = SystemClockAdapter::new();
        let mut zmq_adapter = ZmqAdapter::new()?;

        zmq_adapter.bind_router(&format!("tcp://*:{}", &port))?;

        let name = FullName::new(namespace_bytes, b"COORDINATOR".to_vec());

        let core = CoordinatorCore::new(name.to_vec(), directory_adapter, clock_adapter);

        Ok(Self {
            core,
            zmq_adapter,
            name,
            running: false,
        })
    }

    /// Create an error response message
    fn create_error_response(
        &self,
        _recipient_identity: &[u8],
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
            .sender(self.name.clone())
            .conversation_id(conversation_id.unwrap_or_default())
            .message_type(1) // JSON message type
            .payload_single(error_msg)
            .build()?;

        Ok(message.to_view()?)
    }

    /// Create a JSON-RPC response message
    fn create_json_response(
        &self,
        _recipient_identity: &[u8],
        recipient_name: &FullName,
        id: Id,
        result: Value,
        conversation_id: Option<ConversationId>,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let response = Response::new(ResponsePayload::Success(Cow::Borrowed(&result)), id);
        let response_msg = serde_json::to_vec(&response)?;

        let message = MessageBuilder::new()
            .receiver(recipient_name.clone())
            .sender(self.name.clone())
            .conversation_id(conversation_id.unwrap_or_default())
            .message_type(MessageType::Json.into())
            .payload_single(response_msg)
            .build()?;

        Ok(message.to_view()?)
    }

    /// Start the coordinator's main loop
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.running = true;
        println!("Coordinator started");

        while self.running {
            self.check_timeouts();

            // Poll for messages with a timeout
            let mut items = [self.zmq_adapter.router_socket().as_poll_item(zmq::POLLIN)];
            if zmq::poll(&mut items, 100)? > 0 {
                // 100ms timeout
                self.process_message()?;
            }
        }

        println!("Coordinator stopped");
        Ok(())
    }

    /// Process a single message
    fn process_message(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (identity, message) =
            ProtocolAdapter::receive_message(self.zmq_adapter.router_socket())?;

        let decision = self.core.route_message(&message, &identity);

        match decision {
            RoutingDecision::Local { target_identity } => {
                self.zmq_adapter.send_to_local(&target_identity, &message)?;
            }
            RoutingDecision::Remote {
                target_dealer_identity,
            } => {
                self.zmq_adapter
                    .send_to_remote(&target_dealer_identity, &message)?;
            }
            RoutingDecision::SelfTarget => {
                self.handle_self_message(&identity, &message)?;
            }
            RoutingDecision::Error {
                error,
                conversation_id,
            } => {
                match message.sender() {
                    Ok(name) => {
                        let error_message = self.create_error_response(
                            &identity,
                            name, // Pass the FullName directly
                            &error,
                            Some(conversation_id),
                        )?;
                        self.zmq_adapter.send_to_local(&identity, &error_message)?;
                    }

                    Err(e) => {
                        eprintln!("Error: Malformed sender name in message, cannot send error response: {:?}", e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle messages addressed to this coordinator
    fn handle_self_message(
        &mut self,
        identity: &[u8],
        message: &MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let content_frame = match message.content_frame() {
            Some(frame) => frame,
            None => {
                match message.sender() {
                    Ok(sender_name) => {
                        let error_message = self.create_error_response(
                            identity,
                            sender_name,
                            &Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)),
                            Some(message.header().conversation_id.clone()),
                        )?;
                        self.zmq_adapter.send_to_local(identity, &error_message)?;
                    }
                    Err(e) => {
                        eprintln!("Error: Malformed sender name in message, cannot send error response: {:?}", e);
                    }
                }
                return Ok(());
            }
        };

        let request: Request = match serde_json::from_slice(content_frame) {
            Ok(req) => req,
            Err(_) => {
                match message.sender() {
                    Ok(sender_name) => {
                        let error_message = self.create_error_response(
                            identity,
                            sender_name,
                            &Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)),
                            Some(message.header().conversation_id.clone()),
                        )?;
                        self.zmq_adapter.send_to_local(identity, &error_message)?;
                    }
                    Err(e) => {
                        eprintln!("Error: Malformed sender name in message, cannot send error response: {:?}", e);
                    }
                }
                return Ok(());
            }
        };

        // Handle different methods
        match request.method_name() {
            "sign_in" => {
                // Extract sender name from message
                let sender = message
                    .sender()
                    .as_ref()
                    .map_err(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))?;
                self.core
                    .handle_sign_in(sender.clone(), identity.to_vec())?;
                // Send success response
                let response_message = self.create_json_response(
                    identity,
                    sender,
                    request.id(),
                    serde_json::Value::Null,
                    Some(message.header().conversation_id.clone()),
                )?;
                self.zmq_adapter
                    .send_to_local(identity, &response_message)?;
            }
            "sign_out" => {
                // Extract sender name from message
                let sender = message
                    .sender()
                    .as_ref()
                    .map_err(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))?;
                self.core.handle_sign_out(sender.clone())?;
                // Send success response
                let response_message = self.create_json_response(
                    identity,
                    sender,
                    request.id(),
                    serde_json::Value::Null,
                    Some(message.header().conversation_id.clone()),
                )?;
                self.zmq_adapter
                    .send_to_local(identity, &response_message)?;
            }
            "shut_down" => {
                self.running = false;
                // Send success response
                let sender = message
                    .sender()
                    .as_ref()
                    .map_err(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))?;
                let response_message = self.create_json_response(
                    identity,
                    sender,
                    request.id(),
                    serde_json::Value::Null,
                    Some(message.header().conversation_id.clone()),
                )?;
                self.zmq_adapter
                    .send_to_local(identity, &response_message)?;
            }
            _ => {
                let sender = message
                    .sender()
                    .as_ref()
                    .map_err(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))?;
                let error_message = self.create_error_response(
                    identity,
                    sender,
                    &Error::JsonRpc(ErrorObject::from(ErrorCode::MethodNotFound)),
                    Some(message.header().conversation_id.clone()),
                )?;
                self.zmq_adapter.send_to_local(identity, &error_message)?;
            }
        }

        Ok(())
    }

    /// Check for timed out components
    fn check_timeouts(&mut self) {
        let timed_out_components = self.core.check_timeouts(Duration::from_secs(30));
        for component_name in timed_out_components {
            println!(
                "Component {:?} timed out",
                String::from_utf8_lossy(&component_name.to_vec())
            );
        }
    }
}
