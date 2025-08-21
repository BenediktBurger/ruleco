use crate::adapters::{InMemoryDirectoryAdapter, SystemClockAdapter, ZmqAdapter};
use crate::core::domain::RoutingDecision;
use crate::core::ports::message_receiver_port::Identity;
use crate::core::ports::{MessageReceiverPort, MessageSenderPort, RoutingPort};
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
pub struct CoordinatorApp<T = ZmqAdapter> {
    /// The core coordinator logic
    core: CoordinatorCore<InMemoryDirectoryAdapter, SystemClockAdapter>,
    /// The adapter for message sending and receiving
    adapter: T,
    /// Our name as a FullName
    name: FullName,
    /// Flag to indicate if the coordinator is running
    running: bool,
}

impl CoordinatorApp<ZmqAdapter> {
    /// Create a new coordinator application
    pub fn new(namespace: String, port: Option<u16>) -> Result<Self, Box<dyn std::error::Error>> {
        let port = port.unwrap_or(protocol_constants::DEFAULT_COORDINATOR_PORT);
        let mut zmq_adapter = ZmqAdapter::new()?;
        zmq_adapter.bind_router(&format!("tcp://*:{}", &port))?;
        Self::new_with_adapter(namespace, zmq_adapter)
    }
}

impl<T> CoordinatorApp<T>
where
    T: MessageSenderPort + MessageReceiverPort,
{
    /// Create a new coordinator application with a specific adapter
    pub fn new_with_adapter(
        namespace: String,
        adapter: T,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let namespace_bytes = namespace.as_bytes().to_vec();
        let directory_adapter = InMemoryDirectoryAdapter::new(namespace_bytes.clone());
        let clock_adapter = SystemClockAdapter::new();
        let name = FullName::new(namespace_bytes, b"COORDINATOR".to_vec());

        let core =
            CoordinatorCore::new(name.namespace().to_vec(), directory_adapter, clock_adapter);

        Ok(Self {
            core,
            adapter,
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
        // Receive all available messages with a timeout
        let messages = self.adapter.receive_messages(100)?;

        for (identity, received_message) in messages {
            self.process_read_message(identity, received_message)?;
        }

        Ok(())
    }

    /// Process a routed message with the given identity and source information
    pub fn process_read_message(
        &mut self,
        identity: Identity,
        message: MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let decision = self.core.route_message(&message, &identity);

        match decision {
            RoutingDecision::Local { target_identity } => {
                self.adapter.send_to_local(&target_identity, message)?;
            }
            RoutingDecision::Remote {
                target_dealer_identity,
            } => {
                self.adapter
                    .send_to_remote(&target_dealer_identity, message)?;
            }
            RoutingDecision::SelfTarget => {
                self.handle_self_message(identity, &message)?;
            }
            RoutingDecision::Error {
                error,
                conversation_id,
            } => {
                self.send_error_response(identity, &message, &error, conversation_id)?;
            }
        }

        Ok(())
    }

    /// Unified method to handle messages addressed to this coordinator
    fn handle_self_message(
        &mut self,
        identity: Identity,
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
                            sender_name,
                            &Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)),
                            Some(message.header().conversation_id.clone()),
                        )?;
                        self.send_to_identity(identity, error_message)?;
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
                self.process_read_message(Identity::SELF, response_message)?;
            }
            JsonRpcOutcome::ResponseToIdentity((identity, response_message)) => {
                self.send_to_identity(identity, response_message)?;
            }
            JsonRpcOutcome::Shutdown(response_message) => {
                self.process_read_message(Identity::SELF, response_message)?;
                self.running = false;
            }
            JsonRpcOutcome::NoAction => {
                // No response to send
            }
        }

        Ok(())
    }

    fn send_to_identity(
        &self,
        identity: Identity,
        message: MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match identity {
            Identity::Remote { identity } => self.adapter.send_to_remote(&identity, message),
            Identity::Local { identity } => self.adapter.send_to_local(&identity, message),
            Identity::SELF => Ok(()), // log
        }
    }

    /// Send an error response for a message
    fn send_error_response(
        &mut self,
        identity: Identity,
        message: &MessageView,
        error: &Error,
        conversation_id: ruleco_core::message::ConversationId,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let handler = JsonRpcHandler::new(&mut self.core, &self.name);
        match message.sender() {
            Ok(name) => {
                let error_message = handler.create_error_response(
                    name, // Pass the FullName directly
                    error,
                    Some(conversation_id),
                )?;
                self.send_to_identity(identity, error_message)?;
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

    /// Get a reference to the core for testing
    #[cfg(test)]
    pub fn core(&mut self) -> &mut CoordinatorCore<InMemoryDirectoryAdapter, SystemClockAdapter> {
        &mut self.core
    }
}

/// Trait for accessing sent messages in tests
#[cfg(test)]
pub trait TestableAdapter {
    fn get_sent_to_local(&self) -> Vec<(Vec<u8>, MessageView)>;
    fn get_sent_to_remote(&self) -> Vec<(Vec<u8>, MessageView)>;
}

#[cfg(test)]
impl TestableAdapter for crate::adapters::MockAdapter {
    fn get_sent_to_local(&self) -> Vec<(Vec<u8>, MessageView)> {
        self.get_sent_to_local()
    }

    fn get_sent_to_remote(&self) -> Vec<(Vec<u8>, MessageView)> {
        self.get_sent_to_remote()
    }
}

// Implementation for ZMQ-specific functionality
impl CoordinatorApp<ZmqAdapter> {
    // This is intentionally left blank for now, but we could add ZMQ-specific methods here if needed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::MockAdapter;
    use ruleco_core::full_name::FullName;
    use ruleco_core::message::MessageBuilder;

    /// Helper function to format message frames for human-readable debug output
    fn format_message_frames(frames: &[Vec<u8>]) -> String {
        frames
            .iter()
            .enumerate()
            .map(|(i, frame)| {
                let string_repr = String::from_utf8_lossy(frame);
                format!("Frame {}: {:?} ({})", i, frame, string_repr)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

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
        let result = app.handle_self_message(Identity::Local { identity }, &message);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_read_message_local_routing() {
        // Create a mock adapter
        let mock_adapter = MockAdapter::new();

        // Create a coordinator app with the mock adapter
        let namespace = "test_namespace".to_string();
        let mut app = CoordinatorApp::new_with_adapter(namespace, mock_adapter)
            .expect("Failed to create CoordinatorApp");

        // Register a local component in the directory
        let component_name = FullName::new(b"test_namespace".to_vec(), b"test_component".to_vec());
        let component_identity = vec![1, 2, 3, 4];
        let sender_name = FullName::new(b"test_namespace".to_vec(), b"sender".to_vec());
        let sender_identity = vec![5, 6, 7, 8];

        // Sign in the component
        app.core()
            .handle_sign_in(
                component_name.clone(),
                Identity::Local {
                    identity: component_identity.clone(),
                },
            )
            .expect("Failed to sign in component");
        app.core()
            .handle_sign_in(
                sender_name.clone(),
                Identity::Local {
                    identity: sender_identity.clone(),
                },
            )
            .expect("Failed to sign in component");

        // Create a message from another component to the registered component
        let message = MessageBuilder::new()
            .sender(sender_name)
            .receiver(component_name)
            .payload_single(b"test_content".to_vec())
            .build()
            .unwrap()
            .to_view()
            .unwrap();

        // Process the message
        let result = app.process_read_message(
            Identity::Local {
                identity: sender_identity,
            },
            message.clone(),
        );
        assert!(result.is_ok());

        // Verify a message was sent to the local component
        let sent_messages = app.adapter.get_sent_to_local();

        // Custom assertion with debug output
        if sent_messages.len() != 1 {
            let formatted_messages: Vec<String> = sent_messages
                .iter()
                .map(|(identity, msg)| {
                    format!(
                        "Identity: {:?}\nFrames:\n{}",
                        identity,
                        format_message_frames(msg.raw_frames())
                    )
                })
                .collect();

            panic!(
                "Expected 1 sent message, but got {}.\nSent messages:\n{}",
                sent_messages.len(),
                formatted_messages.join("\n---\n")
            );
        }

        // Custom assertion for target identity with debug output
        if sent_messages[0].0 != component_identity {
            panic!(
                "Target identity mismatch.\nExpected: {:?}\nGot: {:?}\nMessage frames:\n{}",
                component_identity,
                sent_messages[0].0,
                format_message_frames(sent_messages[0].1.raw_frames())
            );
        }

        // Custom assertion for message content with debug output
        if sent_messages[0].1.raw_frames() != message.raw_frames() {
            panic!(
                "Message content mismatch.\nExpected frames:\n{}\nGot frames:\n{}",
                format_message_frames(message.raw_frames()),
                format_message_frames(sent_messages[0].1.raw_frames())
            );
        }
    }

    #[test]
    fn test_process_read_message_coordinator_sign_in() {
        // Create a coordinator app with a mock adapter
        let mock_adapter = MockAdapter::new();
        let namespace = "test_namespace".to_string();
        let mut app = CoordinatorApp::new_with_adapter(namespace, mock_adapter)
            .expect("Failed to create CoordinatorApp");

        // Create a coordinator sign-in message
        let coordinator_name = FullName::new(b"remote_namespace".to_vec(), b"COORDINATOR".to_vec());
        let request_json = r#"{"jsonrpc":"2.0","method":"coordinator_sign_in","id":1}"#;

        let message = MessageBuilder::new()
            .sender(coordinator_name)
            .receiver(FullName::new(
                b"test_namespace".to_vec(),
                b"COORDINATOR".to_vec(),
            ))
            .payload_single(request_json.as_bytes().to_vec())
            .message_type(ruleco_core::protocol_constants::MessageType::Json.into())
            .build()
            .unwrap()
            .to_view()
            .unwrap();

        // Process the message
        let dealer_identity = vec![1, 2, 3, 4];
        let result = app.process_read_message(
            Identity::Local {
                identity: dealer_identity.clone(),
            },
            message.clone(),
        );
        assert!(result.is_ok());

        // Verify a response was sent to the remote coordinator
        let sent_messages = app.adapter.get_sent_to_local();
        assert_eq!(sent_messages.len(), 1);
        assert_eq!(sent_messages[0].0, dealer_identity); // target dealer identity

        // Verify the response is a JSON-RPC response with null result
        let response_message = &sent_messages[0].1;
        assert_eq!(
            response_message.header().message_type_enum(),
            ruleco_core::protocol_constants::MessageType::Json
        );

        // Custom assertion with debug output using our helper function
        if sent_messages.len() != 1 {
            let formatted_messages: Vec<String> = sent_messages
                .iter()
                .map(|(identity, msg)| {
                    format!(
                        "Identity: {:?}\nFrames:\n{}",
                        identity,
                        format_message_frames(msg.raw_frames())
                    )
                })
                .collect();

            panic!(
                "Expected 1 sent message, but got {}.\nSent messages:\n{}",
                sent_messages.len(),
                formatted_messages.join("\n---\n")
            );
        }

        if sent_messages[0].0 != dealer_identity {
            panic!(
                "Target identity mismatch.\nExpected: {:?}\nGot: {:?}\nMessage frames:\n{}",
                dealer_identity,
                sent_messages[0].0,
                format_message_frames(sent_messages[0].1.raw_frames())
            );
        }

        let response_message = &sent_messages[0].1;
        assert_eq!(
            response_message.header().message_type_enum(),
            ruleco_core::protocol_constants::MessageType::Json
        );
        let content_frame = response_message
            .content_frame()
            .expect("Response should have content");
        let content_str =
            std::str::from_utf8(content_frame).expect("Content should be valid UTF-8");

        // Custom assertion with debug output
        if !content_str.contains("\"result\":null") {
            panic!(
                "Response content does not contain \"result\":null.\nActual content:\n{}\nMessage frames:\n{}",
                content_str,
                format_message_frames(response_message.raw_frames())
            );
        }
    }

    #[test]
    fn test_process_read_message_self_target() {
        // Create a coordinator app with a mock adapter
        let mock_adapter = MockAdapter::new();
        let namespace = "test_namespace".to_string();
        let mut app = CoordinatorApp::new_with_adapter(namespace, mock_adapter)
            .expect("Failed to create CoordinatorApp");

        // Create a message addressed to the coordinator itself
        let sender_name = FullName::new(b"test_namespace".to_vec(), b"sender".to_vec());
        let sender_identity = vec![1, 2, 3, 4];
        let recipient_name = FullName::new(b"test_namespace".to_vec(), b"COORDINATOR".to_vec());
        let request_json = r#"{"jsonrpc":"2.0","method":"some_method","id":1}"#;
        app.core()
            .handle_sign_in(
                sender_name.clone(),
                Identity::Local {
                    identity: sender_identity.clone(),
                },
            )
            .expect("Failed to sign in component");

        let message = MessageBuilder::new()
            .sender(sender_name)
            .receiver(recipient_name)
            .payload_single(request_json.as_bytes().to_vec())
            .message_type(ruleco_core::protocol_constants::MessageType::Json.into())
            .build()
            .unwrap()
            .to_view()
            .unwrap();

        // Process the message
        let result = app.process_read_message(
            Identity::Local {
                identity: sender_identity.clone(),
            },
            message.clone(),
        );
        assert!(result.is_ok());

        // Verify a method not found error response was sent
        let sent_messages = app.adapter.get_sent_to_local();
        assert_eq!(sent_messages.len(), 1);
        assert_eq!(sent_messages[0].0, sender_identity); // target identity

        // Verify the response is a JSON-RPC error response
        let response_message = &sent_messages[0].1;
        assert_eq!(
            response_message.header().message_type_enum(),
            ruleco_core::protocol_constants::MessageType::Json
        );

        // Custom assertion with debug output using our helper function
        if sent_messages.len() != 1 {
            let formatted_messages: Vec<String> = sent_messages
                .iter()
                .map(|(identity, msg)| {
                    format!(
                        "Identity: {:?}\nFrames:\n{}",
                        identity,
                        format_message_frames(msg.raw_frames())
                    )
                })
                .collect();

            panic!(
                "Expected 1 sent message, but got {}.\nSent messages:\n{}",
                sent_messages.len(),
                formatted_messages.join("\n---\n")
            );
        }

        if sent_messages[0].0 != sender_identity {
            panic!(
                "Target identity mismatch.\nExpected: {:?}\nGot: {:?}\nMessage frames:\n{}",
                sender_identity,
                sent_messages[0].0,
                format_message_frames(sent_messages[0].1.raw_frames())
            );
        }

        let response_message = &sent_messages[0].1;
        assert_eq!(
            response_message.header().message_type_enum(),
            ruleco_core::protocol_constants::MessageType::Json
        );
        let content_frame = response_message
            .content_frame()
            .expect("Response should have content");
        let content_str =
            std::str::from_utf8(content_frame).expect("Content should be valid UTF-8");

        // Custom assertions with debug output
        if !content_str.contains("\"error\"") {
            panic!(
                "Response content does not contain \"error\".\nActual content:\n{}\nMessage frames:\n{}",
                content_str,
                format_message_frames(response_message.raw_frames())
            );
        }

        if !content_str.contains("\"code\":-32601") {
            panic!(
                "Response content does not contain \"code\":-32601.\nActual content:\n{}\nMessage frames:\n{}",
                content_str,
                format_message_frames(response_message.raw_frames())
            );
        }
    }
}
