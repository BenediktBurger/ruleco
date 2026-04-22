use crate::adapters::{InMemoryDirectoryAdapter, SystemClockAdapter, ZmqAdapter};
use crate::core::domain::{CoordinatorEntry, RoutingError};
use crate::core::pending_connections::PendingConnections;
use crate::core::ports::clock_port::ClockPort;
use crate::core::ports::message_port::Identity;
use crate::core::ports::routing_port::RoutingPort;
use crate::core::ports::{ConnectionManagementPort, MessagePort};
use crate::core::CoordinatorCore;
use crate::jsonrpc_handler::{JsonRpcHandler, JsonRpcOutcome};
use jsonrpsee_types::{Id, Request};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use ruleco_core::message::{MessageBuilder, MessageView};
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
    /// Our public address (e.g., "tcp://192.168.1.100:12300")
    address: String,
    /// Flag to indicate if the coordinator is running
    running: bool,
    /// Track pending coordinator connections
    pending_connections: PendingConnections,
    /// Timeout interval in seconds for device communication timeout checks
    timeout_interval: u64,
}

impl CoordinatorApp<ZmqAdapter> {
    /// Create a new coordinator application
    pub fn new(
        namespace: &str,
        port: Option<u16>,
        timeout_interval: Option<u64>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let port = port.unwrap_or(protocol_constants::DEFAULT_COORDINATOR_PORT);
        let timeout_interval = timeout_interval.unwrap_or(10);
        let mut zmq_adapter = ZmqAdapter::new()?;
        zmq_adapter.listen_for_components(&format!("tcp://*:{}", &port))?;
        let address = format!("tcp://127.0.0.1:{}", &port);
        Self::new_with_adapter(namespace, address, zmq_adapter, timeout_interval)
    }

    /// Create a new coordinator application with custom bind and public addresses
    pub fn new_with_addresses(
        namespace: &str,
        bind_address: &str,
        public_address: &str,
        timeout_interval: Option<u64>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let timeout_interval = timeout_interval.unwrap_or(10);
        let mut zmq_adapter = ZmqAdapter::new()?;
        zmq_adapter.listen_for_components(bind_address)?;
        Self::new_with_adapter(
            namespace,
            public_address.to_string(),
            zmq_adapter,
            timeout_interval,
        )
    }
}

impl<T> CoordinatorApp<T>
where
    T: MessagePort + ConnectionManagementPort,
{
    /// Create a new coordinator application with a specific adapter
    pub fn new_with_adapter(
        namespace: &str,
        address: String,
        adapter: T,
        timeout_interval: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let namespace_bytes = namespace.as_bytes().to_vec();
        let directory_adapter = InMemoryDirectoryAdapter::new(namespace_bytes.clone());
        let clock_adapter = SystemClockAdapter::new();
        let name = FullName::new(namespace_bytes, b"COORDINATOR".to_vec());

        let core = CoordinatorCore::new(
            name.namespace().to_vec(),
            address.clone(),
            directory_adapter,
            clock_adapter,
        );

        Ok(Self {
            core,
            adapter,
            name,
            address,
            running: false,
            pending_connections: PendingConnections::new(),
            timeout_interval,
        })
    }

    /// Start the coordinator's main loop
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.running = true;
        println!("Coordinator started");

        while self.running {
            let timed_out = self
                .core
                .check_timeouts(Duration::from_secs(self.timeout_interval));
            if !timed_out.is_empty() {
                let _ = self.core.remove_timed_out_components(&timed_out);
            }

            let timeout_duration = Duration::from_secs(self.timeout_interval);
            let timed_out = self
                .pending_connections
                .check_timeouts(timeout_duration, self.core.clock());
            for dealer_identity in timed_out {
                eprintln!("Pending connection timed out");
                let _ = self.adapter.disconnect_from_coordinator(&dealer_identity);
            }

            let timed_out_coordinators = self.core.check_coordinator_timeouts(timeout_duration);
            if !timed_out_coordinators.is_empty() {
                for ns in &timed_out_coordinators {
                    if let Ok(dealer_id) = self.core.get_coordinator_dealer_identity(ns) {
                        let _ = self.adapter.disconnect_from_coordinator(&dealer_id);
                    }
                }
                let _ = self
                    .core
                    .remove_timed_out_coordinators(&timed_out_coordinators);
            }

            if let Err(e) = self.poll_and_process_messages() {
                eprintln!("Error processing messages: {}", e);
            }
        }

        println!("Coordinator stopped - exiting");
        Ok(())
    }

    /// Request the coordinator to stop
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Get the coordinator's public address
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Poll for messages and process them
    fn poll_and_process_messages(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let timeout_ms = 100;

        while let Some((sender_identity, frames)) = self.adapter.recv(timeout_ms)? {
            if let Err(e) = self.process_raw_message(sender_identity, frames) {
                eprintln!("Error processing message: {}", e);
            }
        }

        if !self.pending_connections.is_empty() || self.core.has_remote_coordinators() {
            let coordinator_msgs = self.adapter.recv_coordinator_sign_ins()?;
            for (sender_identity, frames) in coordinator_msgs {
                if let Err(e) = self.process_raw_message(sender_identity, frames) {
                    eprintln!("Error processing message: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Process raw frames received from the adapter
    fn process_raw_message(
        &mut self,
        sender_identity: Identity,
        frames: Vec<Vec<u8>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = match MessageView::new(frames) {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("Failed to parse message: {:?}", e);
                return Ok(());
            }
        };

        self.process_message(sender_identity, message)
    }

    /// Process a parsed message
    fn process_message(
        &mut self,
        sender_identity: Identity,
        message: MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(sender) = message.sender() {
            match &sender_identity {
                Identity::Component { .. } => {
                    if sender.namespace() == self.name.namespace() || sender.namespace().is_empty()
                    {
                        let _ = self.core.update_component_last_seen(&sender);
                    }
                }
                Identity::Coordinator { .. } => {
                    if sender.name() == b"COORDINATOR" {
                        let _ = self.core.update_coordinator_last_seen(sender.namespace());
                    }
                }
                Identity::SelfTarget => {}
            }
        }

        let result = self.core.route_message(&message, &sender_identity);

        match result {
            Ok(target_identity) => match target_identity {
                Identity::SelfTarget => {
                    self.handle_self_message(sender_identity, &message)?;
                }
                _ => {
                    self.adapter
                        .send(&target_identity, message.into_raw_frames())?;
                }
            },
            Err(RoutingError {
                error,
                conversation_id,
            }) => {
                self.send_error_response(sender_identity, &message, &error, conversation_id)?;
            }
        }

        Ok(())
    }

    /// Handle a coordinator sign-in request from another coordinator
    fn handle_coordinator_sign_in_request(
        &mut self,
        sender_identity: Identity,
        coordinator_name: FullName,
        message: &MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dealer_identity = match &sender_identity {
            Identity::Coordinator { identity } | Identity::Component { identity } => {
                identity.clone()
            }
            Identity::SelfTarget => {
                return Ok(());
            }
        };

        let coordinator_entry = CoordinatorEntry {
            namespace: coordinator_name.namespace().to_vec(),
            dealer_identity: dealer_identity.clone(),
            address: String::new(),
            last_seen: self.core.clock().now(),
        };

        if let Err(e) = self.core.register_coordinator(coordinator_entry) {
            let error_response = {
                let handler = JsonRpcHandler::new(&mut self.core, &self.name);
                handler.create_error_response(
                    &coordinator_name,
                    jsonrpsee_types::Id::Null,
                    &e,
                    Some(message.header().conversation_id.clone()),
                )?
            };
            self.adapter
                .send(&sender_identity, error_response.into_raw_frames())?;
            return Ok(());
        }

        let response = {
            let handler = JsonRpcHandler::new(&mut self.core, &self.name);
            handler.create_json_response(
                &coordinator_name,
                jsonrpsee_types::Id::Null,
                serde_json::Value::Null,
                Some(message.header().conversation_id.clone()),
            )?
        };
        self.adapter
            .send(&sender_identity, response.into_raw_frames())?;
        Ok(())
    }

    /// Handle messages addressed to this coordinator
    fn handle_self_message(
        &mut self,
        sender_identity: Identity,
        message: &MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let content_frame = match message.content_frame() {
            Some(frame) => frame,
            None => return Ok(()),
        };

        if message.header().message_type_enum() != MessageType::Json {
            return Ok(());
        }

        let sender_identity_bytes = match &sender_identity {
            Identity::Component { identity } | Identity::Coordinator { identity } => {
                identity.clone()
            }
            Identity::SelfTarget => {
                eprintln!("Self-targeted message from SelfTarget?");
                return Ok(());
            }
        };

        if let Ok(sender_name) = message.sender() {
            if self.core.is_coordinator_sign_in(message) {
                return self.handle_coordinator_sign_in_request(
                    sender_identity,
                    sender_name.clone(),
                    message,
                );
            }

            if let Some(conn_info) = self
                .pending_connections
                .get_pending_connection(&sender_identity_bytes)
            {
                if self.core.is_error_response(message) {
                    eprintln!("Coordinator sign-in failed for {}", conn_info.address);
                    self.pending_connections
                        .complete_connection(&sender_identity_bytes);
                    let _ = self
                        .adapter
                        .disconnect_from_coordinator(&sender_identity_bytes);
                    return Ok(());
                }

                match self.core.handle_coordinator_sign_in_success(
                    &sender_identity_bytes,
                    sender_name.clone(),
                    conn_info.address.clone(),
                ) {
                    Ok((_entry, messages)) => {
                        self.pending_connections
                            .complete_connection(&sender_identity_bytes);
                        for msg in messages {
                            self.adapter.send(
                                &Identity::Coordinator {
                                    identity: sender_identity_bytes.clone(),
                                },
                                msg.into_raw_frames(),
                            )?;
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("Error handling coordinator sign-in success: {}", e);
                        self.pending_connections
                            .complete_connection(&sender_identity_bytes);
                        let _ = self
                            .adapter
                            .disconnect_from_coordinator(&sender_identity_bytes);
                    }
                }
            }
        }

        let outcomes = {
            let mut handler = JsonRpcHandler::new(&mut self.core, &self.name);
            handler.handle_jsonrpc_message(sender_identity.clone(), message, content_frame)?
        };

        for outcome in outcomes {
            match outcome {
                JsonRpcOutcome::Response(response_message) => {
                    self.adapter
                        .send(&sender_identity, response_message.into_raw_frames())?;
                }
                JsonRpcOutcome::ResponseToIdentity((identity, response_message)) => {
                    let frames = response_message.into_raw_frames();
                    match &identity {
                        Identity::SelfTarget => {
                            continue;
                        }
                        _ => {
                            self.adapter.send(&identity, frames)?;
                        }
                    }
                }
                JsonRpcOutcome::Shutdown => {
                    self.running = false;
                }
                JsonRpcOutcome::AddNodes(addresses) => {
                    for address in addresses {
                        self.connect_to_remote_coordinator(address);
                    }
                }
                JsonRpcOutcome::DirectorySync(messages) => {
                    for msg in messages {
                        if let Ok(receiver) = msg.receiver() {
                            let target_identity = Identity::Coordinator {
                                identity: self
                                    .core
                                    .get_coordinator_dealer_identity(receiver.namespace())?,
                            };
                            self.adapter.send(&target_identity, msg.into_raw_frames())?;
                        }
                    }
                }
                JsonRpcOutcome::NoAction => {}
            }
        }

        Ok(())
    }

    fn send_error_response(
        &mut self,
        identity: Identity,
        message: &MessageView,
        error: &Error,
        conversation_id: ruleco_core::message::ConversationId,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match &identity {
            Identity::Component { .. } | Identity::Coordinator { .. } => {}
            Identity::SelfTarget => {
                eprintln!("Cannot send error to self");
                return Ok(());
            }
        };
        let error_message = {
            let handler = JsonRpcHandler::new(&mut self.core, &self.name);
            match message.sender() {
                Ok(name) => {
                    handler.create_error_response(name, Id::Null, error, Some(conversation_id))?
                }
                Err(_) => return Ok(()),
            }
        };
        self.adapter
            .send(&identity, error_message.into_raw_frames())?;
        Ok(())
    }

    fn connect_to_remote_coordinator(&mut self, address: String) {
        let dealer_identity = match self.adapter.connect_to_coordinator(&address) {
            Ok(id) => id,
            Err(err) => {
                eprintln!("Failed to connect to coordinator at {}: {}", address, err);
                return;
            }
        };

        self.pending_connections.add_pending_connection(
            dealer_identity.clone(),
            address.clone(),
            self.core.clock(),
        );

        let request =
            Request::borrowed("coordinator_sign_in", None, jsonrpsee_types::Id::Number(2));
        let receiver_name = match FullName::from_slice(b"COORDINATOR") {
            Ok(name) => name,
            Err(e) => {
                eprintln!("Failed to create remote coordinator name: {}", e);
                return;
            }
        };
        let message = match MessageBuilder::new()
            .receiver(receiver_name)
            .sender(self.name.clone())
            .payload_json(&request)
        {
            Ok(builder) => match builder.build() {
                Ok(built_msg) => match built_msg.to_view() {
                    Ok(view) => view,
                    Err(e) => {
                        eprintln!("Failed to create message view: {}", e);
                        return;
                    }
                },
                Err(e) => {
                    eprintln!("Failed to build message: {}", e);
                    return;
                }
            },
            Err(e) => {
                eprintln!("Failed to add payload to message: {}", e);
                return;
            }
        };

        if let Err(err) = self.adapter.send(
            &Identity::Coordinator {
                identity: dealer_identity.clone(),
            },
            message.into_raw_frames(),
        ) {
            eprintln!("Failed to send coordinator sign-in request: {}", err);
            let _ = self.adapter.disconnect_from_coordinator(&dealer_identity);
            self.pending_connections
                .complete_connection(&dealer_identity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::MockAdapter;

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

    /// Assert that two messages are equal showing message content if they actually differ
    fn assert_messages_are_equal(sent_message: &MessageView, message: &MessageView) {
        if sent_message != message {
            panic!(
                "Message content mismatch.\nExpected frames:\n{}\nGot frames:\n{}",
                format_message_frames(message.raw_frames()),
                format_message_frames(sent_message.raw_frames())
            );
        }
    }

    static NAMESPACE: &str = "test_namespace";
    static REMOTE_NAMESPACE: &str = "remote_namespace";
    static COMPONENT1_IDENTITY: &[u8] = b"com1";
    static COMPONENT2_IDENTITY: &[u8] = b"com2";
    static DEALER_IDENTITY: &[u8] = b"deal";

    fn self_name() -> FullName {
        FullName::from_str(&format!("{}.{}", NAMESPACE, "COORDINATOR")).unwrap()
    }

    fn component1_name() -> FullName {
        FullName::new(NAMESPACE.as_bytes().to_vec(), b"component1".to_vec())
    }

    fn component2_name() -> FullName {
        FullName::new(NAMESPACE.as_bytes().to_vec(), b"component2".to_vec())
    }

    /// Create a default Coordinator app for tests
    ///
    /// Contains already two Components and a Coordinator configured.
    fn create_default_app() -> CoordinatorApp<MockAdapter> {
        let mock_adapter = MockAdapter::new();
        let mut app = CoordinatorApp::new_with_adapter(
            NAMESPACE,
            "tcp://127.0.0.1:12300".to_string(),
            mock_adapter,
            10,
        )
        .expect("Failed to create CoordinatorApp");

        // Sign in the components
        app.core
            .handle_sign_in(component1_name(), COMPONENT1_IDENTITY)
            .expect("Failed to sign in component");
        app.core
            .handle_sign_in(component2_name(), COMPONENT2_IDENTITY)
            .expect("Failed to sign in component");

        app
    }

    #[test]
    fn handle_self_message_no_content_frame_returns_ok() {
        // Create a minimal CoordinatorApp instance for testing
        let namespace = NAMESPACE;
        let mock_adapter = MockAdapter::new();
        let mut app = CoordinatorApp::new_with_adapter(
            namespace,
            "tcp://127.0.0.1:12300".to_string(),
            mock_adapter,
            10,
        )
        .expect("Failed to create CoordinatorApp");
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
        let result = app.handle_self_message(Identity::Component { identity }, &message);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_message_local_routing() {
        let mut app = create_default_app();

        // Create a message from another component to the registered component
        let message = MessageBuilder::new()
            .sender(component1_name())
            .receiver(component2_name())
            .payload_single(b"test_content".to_vec())
            .build()
            .unwrap()
            .to_view()
            .unwrap();

        // Process the message
        let result = app.process_message(
            Identity::Component {
                identity: COMPONENT1_IDENTITY.to_vec(),
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
        if sent_messages[0].0 != COMPONENT2_IDENTITY {
            panic!(
                "Target identity mismatch.\nExpected: {:?}\nGot: {:?}\nMessage frames:\n{}",
                COMPONENT2_IDENTITY,
                sent_messages[0].0,
                format_message_frames(sent_messages[0].1.raw_frames())
            );
        }

        // Custom assertion for message content with debug output
        assert_messages_are_equal(&sent_messages[0].1, &message);
    }

    #[test]
    fn test_process_message_from_remote_to_local() {
        let mut app = create_default_app();
        let message = MessageBuilder::new()
            .sender(
                FullName::from_str(&format!("{}.{}", REMOTE_NAMESPACE, "some_component")).unwrap(),
            )
            .receiver(component1_name())
            .build()
            .unwrap()
            .to_view()
            .unwrap();

        let result = app.process_message(
            Identity::Coordinator {
                identity: DEALER_IDENTITY.to_vec(),
            },
            message.clone(),
        );
        assert!(result.is_ok());

        let sent_messages = app.adapter.get_sent_to_local();
        assert_eq!(sent_messages.len(), 1);
        assert_eq!(sent_messages[0].0, COMPONENT1_IDENTITY.to_vec());

        assert_messages_are_equal(&sent_messages[0].1, &message);
    }

    #[test]
    fn test_process_message_from_local_to_remote() {
        let mut app = create_default_app();

        let coordinator_entry = crate::core::domain::CoordinatorEntry {
            namespace: REMOTE_NAMESPACE.as_bytes().to_vec(),
            dealer_identity: DEALER_IDENTITY.to_vec(),
            address: String::new(),
            last_seen: std::time::Instant::now(),
        };
        app.core
            .register_coordinator(coordinator_entry)
            .expect("Failed to register remote coordinator");

        let message = MessageBuilder::new()
            .sender(component1_name())
            .receiver(
                FullName::from_str(&format!("{}.{}", REMOTE_NAMESPACE, "some_component")).unwrap(),
            )
            .build()
            .unwrap()
            .to_view()
            .unwrap();

        let result = app.process_message(
            Identity::Component {
                identity: COMPONENT1_IDENTITY.to_vec(),
            },
            message.clone(),
        );

        let sent_to_remote = app.adapter.get_sent_to_remote();
        let sent_to_local = app.adapter.get_sent_to_local();
        let all_sent = app.adapter.get_all_sent_messages();

        if !result.is_ok() {
            panic!("process_message failed: {:?}", result);
        }
        if sent_to_remote.len() != 1 {
            panic!(
                "Expected 1 message sent to remote, got {}.\nSent to local: {}\nAll sent: {:?}",
                sent_to_remote.len(),
                sent_to_local.len(),
                all_sent
                    .iter()
                    .map(|(id, _)| format!("{:?}", id))
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(sent_to_remote[0].0, DEALER_IDENTITY.to_vec());

        assert_messages_are_equal(&sent_to_remote[0].1, &message);
    }

    #[test]
    fn test_process_message_coordinator_sign_in() {
        let mut app = create_default_app();

        // Create a coordinator sign-in message
        let coordinator_name =
            FullName::new(b"remote_namespace2".to_vec(), b"COORDINATOR".to_vec());
        let request_json = r#"{"jsonrpc":"2.0","method":"coordinator_sign_in","id":1}"#;

        let message = MessageBuilder::new()
            .sender(coordinator_name)
            .receiver(self_name())
            .payload_single(request_json.as_bytes().to_vec())
            .message_type(ruleco_core::protocol_constants::MessageType::Json.into())
            .build()
            .unwrap()
            .to_view()
            .unwrap();

        // Sign in the coordinator to allow routing
        let dealer_identity = vec![1, 2, 3, 4];
        app.core
            .handle_sign_in(
                FullName::new(b"remote_namespace2".to_vec(), b"COORDINATOR".to_vec()),
                &dealer_identity,
            )
            .expect("Failed to sign in coordinator");

        let result = app.process_message(
            Identity::Coordinator {
                identity: dealer_identity.clone(),
            },
            message,
        );
        assert!(result.is_ok());

        // Verify a response was sent to the remote coordinator
        let sent_messages = app.adapter.get_sent_to_remote();
        assert_eq!(sent_messages.len(), 1);
        assert_eq!(sent_messages[0].0, dealer_identity);

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
        if !content_str.contains(r#""result":null"#) {
            panic!(
                "Response content does not contain \"result\":null.\nActual content:\n{}\nMessage frames:\n{}",
                content_str,
                format_message_frames(response_message.raw_frames())
            );
        }
    }

    #[test]
    fn test_process_message_self_target() {
        let mut app = create_default_app();

        let sender_identity = COMPONENT1_IDENTITY.to_vec();
        let request_json = r#"{"jsonrpc":"2.0","method":"some_method","id":1}"#;

        let message = MessageBuilder::new()
            .sender(component1_name())
            .receiver(self_name())
            .payload_single(request_json.as_bytes().to_vec())
            .message_type(ruleco_core::protocol_constants::MessageType::Json.into())
            .build()
            .unwrap()
            .to_view()
            .unwrap();

        let result = app.process_message(
            Identity::Component {
                identity: sender_identity.clone(),
            },
            message.clone(),
        );
        assert!(result.is_ok());

        // Verify a method not found error response was sent
        let sent_messages = app.adapter.get_sent_to_local();
        assert_eq!(sent_messages.len(), 1);
        assert_eq!(sent_messages[0].0, sender_identity);

        // Verify the response is a JSON-RPC error response
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

        if !content_str.contains(r#""error""#) {
            panic!(
                "Response content does not contain \"error\".\nActual content:\n{}\nMessage frames:\n{}",
                content_str,
                format_message_frames(response_message.raw_frames())
            );
        }

        if !content_str.contains(r#""code":-32601"#) {
            panic!(
                "Response content does not contain \"code\":-32601.\nActual content:\n{}\nMessage frames:\n{}",
                content_str,
                format_message_frames(response_message.raw_frames())
            );
        }
    }
}
