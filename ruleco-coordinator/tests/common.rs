//! Common testing utilities for coordinator integration tests
//!
//! This module provides reusable components for testing the coordinator
//! with real ZMQ sockets, following the control protocol specification.
#![cfg(test)]
#![allow(dead_code)]

use jsonrpsee_types::Id;
use jsonrpsee_types::Request;
use ruleco_coordinator::app::CoordinatorApp;
use ruleco_core::full_name::FullName;
use ruleco_core::message::{ConversationId, MessageBuilder};
use ruleco_core::protocol_constants::MessageType;
use serde_json::Value;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use zmq::{Socket, POLLIN};

/// Base port for test coordinators to avoid conflicts
pub const PORT_BASE: u16 = 15000;

/// Static test namespaces matching the protocol specification examples
pub mod namespaces {
    pub const N1: &str = "N1";
    pub const N2: &str = "N2";
    pub const N3: &str = "N3";
    pub static COORDINATOR_NAME: &str = "COORDINATOR";
}

/// Static test component names
pub mod components {
    pub const CA: &str = "CA";
    pub const CB: &str = "CB";
    pub const CC: &str = "CC";
}

/// Find a free TCP port for testing
pub fn find_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Test coordinator that runs in its own thread
pub struct TestCoordinator {
    /// The namespace of this coordinator
    pub namespace: String,
    /// The port the coordinator is listening on
    pub port: u16,
    /// Handle to the coordinator thread
    pub thread: Option<JoinHandle<Result<(), String>>>,
}

impl TestCoordinator {
    /// Spawn a new coordinator in a background thread
    pub fn spawn(namespace: &str, port: Option<u16>) -> Self {
        Self::spawn_with_timeout(namespace, port, None)
    }

    /// Spawn a new coordinator with a custom timeout in a background thread
    pub fn spawn_with_timeout(
        namespace: &str,
        port: Option<u16>,
        timeout_interval: Option<u64>,
    ) -> Self {
        let port = port.unwrap_or_else(|| find_free_port());
        let namespace = namespace.to_string();

        let thread_namespace = namespace.clone();
        let thread = thread::spawn(move || -> Result<(), String> {
            let mut coordinator =
                CoordinatorApp::new(&thread_namespace, Some(port), timeout_interval)
                    .map_err(|e| format!("Failed to create coordinator: {}", e))?;
            let (_shutdown_tx, shutdown_rx) = crossbeam_channel::bounded::<()>(1);
            coordinator
                .run(shutdown_rx)
                .map_err(|e| format!("Coordinator run failed: {}", e))
        });

        Self {
            namespace,
            port,
            thread: Some(thread),
        }
    }

    /// Gracefully shut down the coordinator using RPC client
    pub fn shutdown(&mut self, client: &TestClient) -> Result<(), Box<dyn std::error::Error>> {
        client.send_shutdown(None)?;

        if let Some(thread) = self.thread.take() {
            let timeout = Duration::from_secs(5);
            join_with_timeout(thread, timeout).map_err(|e| e.into())
        } else {
            Ok(())
        }
    }

    /// Join the thread directly (with timeout for cleanup)
    pub fn join_thread(&mut self, timeout: Duration) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(thread) = self.thread.take() {
            join_with_timeout(thread, timeout)?;
        }
        Ok(())
    }

    /// Check if coordinator is still running
    pub fn is_running(&self) -> bool {
        if let Some(thread) = &self.thread {
            !thread.is_finished()
        } else {
            false
        }
    }

    /// Get the full coordinator name (namespace.COORDINATOR)
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.namespace, namespaces::COORDINATOR_NAME)
    }

    /// Get the coordinator's address string
    pub fn address(&self) -> String {
        format!("tcp://127.0.0.1:{}", self.port)
    }
}

/// Join a thread with a timeout using busy-wait polling
pub fn join_with_timeout<T, E>(
    thread: JoinHandle<Result<T, E>>,
    timeout: Duration,
) -> Result<T, String>
where
    T: Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    let start = Instant::now();

    loop {
        if thread.is_finished() {
            let inner_result = thread
                .join()
                .map_err(|e| format!("Thread panicked: {:?}", e))?;
            return inner_result.map_err(|e| e.to_string());
        }

        if start.elapsed() >= timeout {
            return Err(String::from("Thread did not finish within timeout"));
        }

        thread::sleep(Duration::from_millis(1));
    }
}

impl Drop for TestCoordinator {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = join_with_timeout(thread, Duration::from_millis(500));
        }
    }
}

/// ZMQ DEALER client for testing coordinator communication
pub struct TestClient {
    /// ZMQ DEALER socket
    dealer: Socket,
    /// ZMQ context
    _context: zmq::Context,
    /// The component name (if signed in)
    pub component_name: Option<String>,
    /// The assigned namespace from sign-in
    pub namespace: Option<String>,
    /// Counter for message IDs
    message_id_counter: std::sync::atomic::AtomicU32,
}

impl TestClient {
    /// Create a new DEALER client connected to the coordinator
    pub fn connect(port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let context = zmq::Context::new();
        let dealer = context.socket(zmq::DEALER)?;

        dealer.connect(&format!("tcp://127.0.0.1:{}", port))?;

        Ok(Self {
            dealer,
            _context: context,
            component_name: None,
            namespace: None,
            message_id_counter: std::sync::atomic::AtomicU32::new(1),
        })
    }

    /// Set the component identity for the socket
    pub fn set_identity(&self, identity: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        self.dealer.set_identity(identity)?;
        Ok(())
    }

    /// Wait for the coordinator to be ready to receive messages
    pub fn wait_for_coordinator_ready(
        &self,
        max_retries: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for _ in 0..max_retries {
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    /// Create a sender FullName based on client state
    fn create_sender(&self) -> FullName {
        if let (Some(comp), Some(ns)) = (&self.component_name, &self.namespace) {
            FullName::from_strings(ns, comp).unwrap()
        } else if let Some(comp) = &self.component_name {
            FullName::from_str(comp).unwrap()
        } else {
            FullName::from_slice(b"unknown").unwrap()
        }
    }

    /// Create the coordinator receiver FullName
    fn create_coordinator_receiver() -> FullName {
        FullName::from_slice(b"COORDINATOR").unwrap()
    }

    /// Generate the next message ID (3 bytes)
    fn next_message_id(&self) -> [u8; 3] {
        let counter = self
            .message_id_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst) as u32;
        counter.to_be_bytes()[1..4].try_into().unwrap()
    }

    /// Internal method to send JSON-RPC request with explicit sender
    fn send_jsonrpc_request_with_sender_internal(
        &self,
        method: &str,
        sender: &FullName,
        receiver: Option<&FullName>,
        params: Option<Value>,
        id: Option<u64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Convert Option<u64> to jsonrpsee_types::Id
        let jsonrpsee_id = match id {
            Some(n) => Id::Number(n),
            None => Id::Null,
        };

        // Convert params to RawValue if present
        let params_raw = params.map(|p| {
            serde_json::to_string(&p)
                .map(|s| serde_json::value::RawValue::from_string(s).unwrap())
                .unwrap()
        });

        // Create JSON-RPC request using jsonrpsee_types
        let request = Request::owned(method.to_string(), params_raw, jsonrpsee_id);

        let default_receiver = Self::create_coordinator_receiver();
        let message = MessageBuilder::new()
            .receiver(receiver.unwrap_or_else(|| &default_receiver).clone())
            .sender(sender.clone())
            .conversation_id(ConversationId::new())
            .payload_json(&request)?
            .build()
            .expect("Failed to build JSON-RPC request message");

        let frames = message.to_frames();
        self.dealer.send_multipart(frames, 0)?;

        Ok(())
    }

    /// Send a JSON-RPC request to a recipient (defaults to coordinator)
    pub fn send_jsonrpc_request(
        &self,
        method: &str,
        params: Option<Value>,
        id: Option<u64>,
        receiver: Option<&FullName>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_jsonrpc_request_with_sender_internal(
            method,
            &self.create_sender(),
            receiver,
            params,
            id,
        )
    }

    /// Send a raw message with custom frames
    pub fn send_message(&self, frames: Vec<Vec<u8>>) -> Result<(), Box<dyn std::error::Error>> {
        self.dealer.send_multipart(frames, 0)?;
        Ok(())
    }

    /// Receive a message with timeout
    pub fn receive_message(
        &self,
        timeout_ms: i64,
    ) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        let mut poll_items = vec![self.dealer.as_poll_item(POLLIN)];
        if zmq::poll(&mut poll_items, timeout_ms)? > 0 {
            self.dealer.recv_multipart(0).map_err(|e| e.into())
        } else {
            Err("Timeout waiting for message".into())
        }
    }

    /// Receive a message as a MessageView
    pub fn receive_message_view(
        &self,
        timeout_ms: i64,
    ) -> Result<ruleco_core::message::MessageView, Box<dyn std::error::Error>> {
        let frames = self.receive_message(timeout_ms)?;
        Ok(ruleco_core::message::MessageView::new(frames)?)
    }

    /// Receive a JSON-RPC response
    pub fn receive_jsonrpc_response(
        &self,
        timeout_ms: i64,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let view = self.receive_message_view(timeout_ms)?;
        Ok(view.payload_as_json_value()?)
    }

    /// Sign in to the coordinator
    /// Note: coordinator_namespace should match the coordinator's namespace.
    /// If None, the namespace will be extracted from the coordinator's response sender.
    pub fn sign_in(
        &mut self,
        component_name: &str,
        coordinator_namespace: Option<&str>,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        self.wait_for_coordinator_ready(50)?;

        // Build sender name: coordinator_namespace.component_name (or just component_name if no namespace)
        let sender_name = if let Some(ns) = coordinator_namespace {
            FullName::from_strings(ns, component_name).unwrap()
        } else {
            FullName::from_str(component_name).unwrap()
        };

        // Send with the component name as sender to coordinator
        self.send_jsonrpc_request_with_sender_internal(
            "sign_in",
            &sender_name,
            None,
            None,
            Some(1),
        )?;

        let view = self.receive_message_view(1000)?;
        let response = view.payload_as_json_value()?;

        if response.get("result").is_some() {
            self.component_name = Some(component_name.to_string());

            // Extract namespace from coordinator's response sender
            let sender_fullname = view
                .sender()
                .as_ref()
                .map_err(|e| format!("Invalid sender: {}", e))?
                .clone();
            let namespace_bytes = sender_fullname.namespace();
            if !namespace_bytes.is_empty() {
                self.namespace = Some(
                    std::str::from_utf8(namespace_bytes)
                        .map_err(|e| format!("Invalid UTF-8 in namespace: {}", e))?
                        .to_string(),
                );
            } else if let Some(ns) = coordinator_namespace {
                self.namespace = Some(ns.to_string());
            }
        }

        Ok(response)
    }

    /// Sign out from the coordinator
    pub fn sign_out(&mut self) -> Result<Value, Box<dyn std::error::Error>> {
        if self.component_name.is_none() {
            return Err("Client not signed in".into());
        }

        self.send_jsonrpc_request("sign_out", None, Some(2), None)?;

        let response = self.receive_jsonrpc_response(1000)?;

        self.component_name = None;
        self.namespace = None;

        Ok(response)
    }

    /// Send shut_down request to coordinator (or specific coordinator if provided)
    pub fn send_shutdown(
        &self,
        receiver: Option<&FullName>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_jsonrpc_request("shut_down", None, Some(999), receiver)?;
        Ok(())
    }

    /// Send shut_down and try to receive response
    pub fn shutdown_and_wait(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.send_shutdown(None)?;
        thread::sleep(Duration::from_millis(500));

        if let Err(e) = self.receive_jsonrpc_response(1000) {
            eprintln!("No response to shut_down: {:?}", e);
        }

        Ok(())
    }

    /// Create a minimal content header for testing
    ///
    /// Format: UUIDv7 (16 bytes) + message_id (3 bytes) + message_type (1 byte)
    pub fn create_content_header(&self) -> Vec<u8> {
        let conversation_id = ConversationId::new();
        let message_id = self.next_message_id();

        let mut header = Vec::with_capacity(20);
        header.extend_from_slice(conversation_id.as_bytes());
        header.extend_from_slice(&message_id);
        header.push(MessageType::Json.into());

        header
    }
}

pub fn extract_response(response_frames: &[Vec<u8>]) -> Value {
    let content = &response_frames[4];
    let response: serde_json::Value =
        serde_json::from_slice(content).expect("Failed to parse JSON-RPC response");
    return response;
}

/// Assert that a JSON-RPC response is successful (contains "result")
pub fn assert_success_response(response: &Value) {
    assert!(
        response.get("result").is_some(),
        "Response should contain 'result', got: {:?}",
        response
    );
    assert!(
        response.get("error").is_none(),
        "Response should not contain 'error', got: {:?}",
        response
    );
}

/// Assert that a JSON-RPC response is an error with specific code
pub fn assert_error_response(response: &Value, expected_code: i32) {
    let error = response
        .get("error")
        .expect("Response should contain 'error'");
    let code = error
        .get("code")
        .and_then(|c| c.as_i64())
        .expect("Error should have 'code' field") as i32;

    assert_eq!(
        code, expected_code,
        "Expected error code {}, got {}",
        expected_code, code
    );
}

/// Assert that a JSON-RPC error response contains expected data field
pub fn assert_error_data(response: &Value, expected_data: &str) {
    let error = response
        .get("error")
        .expect("Response should contain 'error'");
    let data = error
        .get("data")
        .and_then(|d| d.as_str())
        .expect("Error should have 'data' field as string");

    assert!(
        data.contains(expected_data),
        "Expected error data to contain '{}', got '{}'",
        expected_data,
        data
    );
}

/// Poll for a condition to be true with timeout
pub fn wait_for_condition<F>(
    mut condition: F,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<(), String>
where
    F: FnMut() -> bool,
{
    let start = Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return Ok(());
        }
        thread::sleep(poll_interval);
    }
    Err(format!("Condition not met within {:?}", timeout))
}

/// Wait for coordinator to be ready by polling with ping
pub fn wait_for_coordinator(
    client: &TestClient,
    max_attempts: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    for attempt in 0..max_attempts {
        let _ = client.send_jsonrpc_request("pong", None, Some(attempt as u64), None);
        if client.receive_jsonrpc_response(100).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(format!("Coordinator not ready after {} attempts", max_attempts).into())
}

/// Assert that JSON-RPC 2.0 requirements are met
pub fn assert_jsonrpc_valid(response: &Value) {
    assert_eq!(
        response.get("jsonrpc"),
        Some(&Value::String("2.0".to_string())),
        "JSON-RPC version should be '2.0'"
    );
    assert!(
        response.get("id").is_some(),
        "Response should have 'id' field"
    );
}

/// Fixture builder for creating unique test data
pub struct FixtureBuilder {
    namespace_counter: u32,
    component_counter: u32,
}

impl FixtureBuilder {
    pub fn new() -> Self {
        Self {
            namespace_counter: 0,
            component_counter: 0,
        }
    }

    /// Generate a unique namespace
    pub fn new_namespace(&mut self) -> String {
        self.namespace_counter += 1;
        format!("test_ns_{}", self.namespace_counter)
    }

    /// Generate a unique component name
    pub fn new_component(&mut self) -> String {
        self.component_counter += 1;
        format!("test_component_{}", self.component_counter)
    }
}

impl Default for FixtureBuilder {
    fn default() -> Self {
        Self::new()
    }
}
