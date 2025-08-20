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
                // Create a handler instance for error response creation
                let handler = JsonRpcHandler::new(&mut self.core, &self.name);
                match message.sender() {
                    Ok(name) => {
                        let error_message = handler.create_error_response(
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
                        self.zmq_adapter.send_to_local(identity, &error_message)?;
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
                self.zmq_adapter
                    .send_to_local(identity, &response_message)?;
            }
            JsonRpcOutcome::Shutdown(response_message) => {
                self.zmq_adapter
                    .send_to_local(identity, &response_message)?;
                self.running = false;
            }
            JsonRpcOutcome::NoAction => {
                // No response to send
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
        let result = app.handle_self_message(&identity, &message);
        assert!(result.is_ok());
    }
}
