use crate::adapters::{InMemoryDirectoryAdapter, ProtocolAdapter, SystemClockAdapter, ZmqAdapter};
use crate::core::domain::RoutingDecision;
use crate::core::ports::{MessageSenderPort, RoutingPort};
use crate::core::CoordinatorCore;
use crate::jsonrpc_handler::{JsonRpcHandler, JsonRpcOutcome};
use jsonrpsee_types::request::Request;
use jsonrpsee_types::{ErrorCode, ErrorObject};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use ruleco_core::message::MessageView;
use ruleco_core::protocol_constants::{self, MessageType};
use std::time::Duration;
use zmq;

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

    /// Start the coordinator's main loop
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.running = true;
        println!("Coordinator started");

        while self.running {
            self.check_timeouts();

            // Poll for messages with a timeout
            if let Err(e) = self.poll_and_process_messages() {
                eprintln!("Error processing messages: {}", e);
            }
        }

        println!("Coordinator stopped");
        Ok(())
    }

    /// Poll for messages and process them
    fn poll_and_process_messages(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Collect dealer identities first
        let dealer_identities: Vec<Vec<u8>> = self
            .zmq_adapter
            .dealer_sockets_iter()
            .map(|(identity, _)| identity.clone())
            .collect();

        // Create a separate vector for poll items to avoid borrowing conflicts
        let mut poll_items = {
            let router_poll_item = self.zmq_adapter.router_socket().as_poll_item(zmq::POLLIN);
            let dealer_poll_items: Vec<_> = self
                .zmq_adapter
                .dealer_sockets_iter()
                .map(|(_, socket)| socket.as_poll_item(zmq::POLLIN))
                .collect();

            let mut items = vec![router_poll_item];
            items.extend(dealer_poll_items);
            items
        };

        // Poll for messages with a timeout
        if zmq::poll(&mut poll_items[..], 100)? > 0 {
            // Collect indices of readable sockets first
            let mut readable_indices = Vec::new();
            for (i, poll_item) in poll_items.iter().enumerate() {
                if poll_item.is_readable() {
                    readable_indices.push(i);
                }
            }

            // Process messages based on which sockets are readable
            // Router socket is first in poll_items (index 0)
            if readable_indices.contains(&0) {
                self.process_router_message()?;
            }

            // Process dealer messages for readable sockets
            // Dealer sockets start from index 1 in poll_items
            for &index in &readable_indices {
                if index > 0 && index <= dealer_identities.len() {
                    self.process_dealer_message(&dealer_identities[index - 1])?;
                }
            }
        }

        Ok(())
    }

    /// Process a single message from the router socket
    fn process_router_message(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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
                self.handle_self_message(&identity, &message, true)?;
            }
            RoutingDecision::Error {
                error,
                conversation_id,
            } => {
                self.send_error_response(&identity, &message, &error, conversation_id, true)?;
            }
        }

        Ok(())
    }

    /// Process a single message from a dealer socket
    fn process_dealer_message(
        &mut self,
        dealer_identity: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Receive message from dealer socket
        let message = ProtocolAdapter::receive_message_from_dealer(
            self.zmq_adapter.get_dealer_socket(dealer_identity)?,
        )?;

        // For messages from coordinators, we need special handling for coordinator_sign_in
        // since they're allowed to send messages without being fully signed in yet
        let is_coordinator_sign_in = self.is_coordinator_sign_in_message(&message);

        if is_coordinator_sign_in {
            self.handle_coordinator_sign_in(dealer_identity, &message)?;
        } else {
            // Regular routing for other messages from coordinators
            let decision = self.core.route_message(&message, dealer_identity);

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
                    self.handle_self_message(dealer_identity, &message, false)?;
                }
                RoutingDecision::Error {
                    error,
                    conversation_id,
                } => {
                    self.send_error_response(
                        dealer_identity,
                        &message,
                        &error,
                        conversation_id,
                        false,
                    )?;
                }
            }
        }

        Ok(())
    }

    /// Check if a message is a coordinator_sign_in request
    fn is_coordinator_sign_in_message(&self, message: &MessageView) -> bool {
        if let Some(content_frame) = message.content_frame() {
            if let Ok(request) = serde_json::from_slice::<Request>(content_frame) {
                return request.method_name() == "coordinator_sign_in";
            }
        }
        false
    }

    /// Unified method to handle messages addressed to this coordinator
    fn handle_self_message(
        &mut self,
        identity: &[u8],
        message: &MessageView,
        is_from_router: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create a handler instance for request handling and error response creation
        let mut handler = JsonRpcHandler::new(&mut self.core, &self.name);

        let content_frame = match message.content_frame() {
            Some(frame) => frame,
            None => {
                // Just a heartbeat
                return Ok(());
            }
        };

        if message.header().message_type_enum() != MessageType::Json {
            eprintln!(
                "Error: Message of unknown type {} received",
                message.header().message_type_raw()
            );
            return Ok(());
        }

        let request: Request = match serde_json::from_slice(content_frame) {
            Ok(req) => req,
            Err(_) => {
                match message.sender() {
                    Ok(sender_name) => {
                        let error_message = handler.create_error_response(
                            identity,
                            sender_name,
                            &Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)),
                            Some(message.header().conversation_id.clone()),
                        )?;
                        if is_from_router {
                            self.zmq_adapter.send_to_local(identity, &error_message)?;
                        } else {
                            self.zmq_adapter.send_to_remote(identity, &error_message)?;
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: Malformed sender name in message, cannot send error response: {:?}", e);
                    }
                }
                return Ok(());
            }
        };

        let outcome = handler.handle_request(identity, message, request)?;
        match outcome {
            JsonRpcOutcome::Response(response_message) => {
                if is_from_router {
                    self.zmq_adapter
                        .send_to_local(identity, &response_message)?;
                } else {
                    self.zmq_adapter
                        .send_to_remote(identity, &response_message)?;
                }
            }
            JsonRpcOutcome::Shutdown(response_message) => {
                if is_from_router {
                    self.zmq_adapter
                        .send_to_local(identity, &response_message)?;
                } else {
                    self.zmq_adapter
                        .send_to_remote(identity, &response_message)?;
                }
                self.running = false;
            }
            JsonRpcOutcome::NoAction => {
                // No response to send
            }
        }

        Ok(())
    }

    /// Handle a coordinator sign-in message
    fn handle_coordinator_sign_in(
        &mut self,
        dealer_identity: &[u8],
        message: &MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Extract the request from the message
        let content_frame = match message.content_frame() {
            Some(frame) => frame,
            None => {
                eprintln!("Error: coordinator_sign_in message has no content frame");
                return Ok(());
            }
        };

        if message.header().message_type_enum() != MessageType::Json {
            eprintln!(
                "Error: coordinator_sign_in message of unknown type {} received",
                message.header().message_type_raw()
            );
            return Ok(());
        }

        let request: Request = match serde_json::from_slice(content_frame) {
            Ok(req) => req,
            Err(_) => {
                match message.sender() {
                    Ok(sender_name) => {
                        // Create a handler instance for error response creation
                        let handler = JsonRpcHandler::new(&mut self.core, &self.name);
                        let error_message = handler.create_error_response(
                            dealer_identity,
                            &sender_name, // Pass reference instead of moving
                            &Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)),
                            Some(message.header().conversation_id.clone()),
                        )?;
                        self.zmq_adapter
                            .send_to_remote(dealer_identity, &error_message)?;
                    }
                    Err(e) => {
                        eprintln!("Error: Malformed sender name in coordinator_sign_in message, cannot send error response: {:?}", e);
                    }
                }
                return Ok(());
            }
        };

        // Handle the coordinator_sign_in request directly without going through JsonRpcHandler
        // since the handler expects a method in the core that doesn't exist
        if request.method_name() == "coordinator_sign_in" {
            let sender = message
                .try_sender(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))
                .unwrap();

            // For coordinator sign-in, we expect the sender to be in format "namespace.COORDINATOR"
            if sender.name() != b"COORDINATOR" {
                // Create a handler instance for error response creation
                let handler = JsonRpcHandler::new(&mut self.core, &self.name);
                let error_message = handler.create_error_response(
                    dealer_identity,
                    &sender, // Pass reference
                    &Error::JsonRpc(ErrorObject::owned(
                        -32091, // Using custom error code
                        "Invalid coordinator sign-in request".to_string(),
                        None::<()>,
                    )),
                    Some(message.header().conversation_id.clone()),
                )?;
                self.zmq_adapter
                    .send_to_remote(dealer_identity, &error_message)?;
                return Ok(());
            }

            // Get the namespace from the sender
            let namespace = sender.namespace().to_vec();

            // For now, we'll use a placeholder address - in a real implementation,
            // this would come from the connection information
            let address = "unknown".to_string();

            // Create coordinator entry
            let coordinator_entry = crate::core::domain::CoordinatorEntry {
                namespace,
                dealer_identity: dealer_identity.to_vec(),
                address,
            };

            // Add coordinator to our directory
            self.core.add_coordinator(coordinator_entry)?;

            // Create a handler instance for success response
            let handler = JsonRpcHandler::new(&mut self.core, &self.name);
            let response_message = handler.create_json_response(
                dealer_identity,
                &sender, // Pass reference
                request.id().clone(),
                serde_json::Value::Null,
                Some(message.header().conversation_id.clone()),
            )?;
            self.zmq_adapter
                .send_to_remote(dealer_identity, &response_message)?;
        } else {
            // For any other method, we shouldn't be in this handler
            // Create a handler instance for error response creation
            let sender = message
                .try_sender(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))
                .unwrap();
            let handler = JsonRpcHandler::new(&mut self.core, &self.name);
            let error_message = handler.create_error_response(
                dealer_identity,
                &sender, // Pass reference
                &Error::JsonRpc(ErrorObject::from(ErrorCode::MethodNotFound)),
                Some(message.header().conversation_id.clone()),
            )?;
            self.zmq_adapter
                .send_to_remote(dealer_identity, &error_message)?;
        }

        Ok(())
    }

    /// Send an error response for a message
    fn send_error_response(
        &mut self,
        identity: &[u8],
        message: &MessageView,
        error: &Error,
        conversation_id: ruleco_core::message::ConversationId,
        is_from_router: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create a handler instance for error response creation
        let handler = JsonRpcHandler::new(&mut self.core, &self.name);
        match message.sender() {
            Ok(name) => {
                let error_message = handler.create_error_response(
                    identity,
                    name, // Pass the FullName directly
                    error,
                    Some(conversation_id),
                )?;
                if is_from_router {
                    self.zmq_adapter.send_to_local(identity, &error_message)?;
                } else {
                    self.zmq_adapter.send_to_remote(identity, &error_message)?;
                }
            }
            Err(e) => {
                eprintln!(
                    "Error: Malformed sender name in message, cannot send error response: {:?}",
                    e
                );
            }
        }
        Ok(())
    }

    /// Handle messages addressed to this coordinator from another coordinator
    fn handle_self_message_from_coordinator(
        &mut self,
        dealer_identity: &[u8],
        message: &MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // This method is kept for backward compatibility but delegates to the unified handler
        self.handle_self_message(dealer_identity, message, false)
    }

    /// Handle messages addressed to this coordinator from a local component
    fn handle_self_message_from_router(
        &mut self,
        identity: &[u8],
        message: &MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // This method is kept for backward compatibility but delegates to the unified handler
        self.handle_self_message(identity, message, true)
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

#[cfg(test)]
mod tests {
    use super::*;
    use ruleco_core::message::MessageBuilder;

    #[test]
    fn handle_self_message_no_content_frame_returns_ok() {
        // Create a minimal CoordinatorApp instance for testing
        let namespace = "test_namespace".to_string();
        let mut app =
            CoordinatorApp::new(namespace, Some(0)).expect("Failed to create CoordinatorApp");

        let identity = vec![1, 2, 3, 4];

        let sender_name = FullName::new(b"test_namespace".to_vec(), b"sender".to_vec());
        let recipient_name = FullName::from_slice(b"test_ns.COORDINATOR").unwrap(); // Send to self

        let message = MessageBuilder::new()
            .sender(sender_name)
            .receiver(recipient_name)
            .build()
            .unwrap()
            .to_view()
            .unwrap();

        // Call handle_self_message and assert it returns Ok
        let result = app.handle_self_message_from_router(&identity, &message);
        assert!(result.is_ok());
    }
}
