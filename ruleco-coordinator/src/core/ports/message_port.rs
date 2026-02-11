/// Message transport port - abstract frame handling
///
/// This port abstracts the transport mechanism (could be ZeroMQ, TCP, local IPC, etc),
/// dealing with raw frame exchange without any protocol knowledge.
/// Protocol parsing and domain concepts (FullName, MessageView) are handled by the Core.
///
/// The actual transport implementation (sockets, connections, identity management)
/// is completely hidden - the domain layer only sees frame exchange.

/// Source context for received messages
///
/// This enum distinguishes the transport context of a received message,
/// which is essential for correct routing. It captures domain-relevant
/// information without exposing implementation details like socket types.
///
/// # Why This Exists
///
/// Different transport connections have different trust/processing requirements:
/// - Local components need validation (sign-in check, identity verification)
/// - Remote coordinators are already authenticated via their connection
/// - Self messages require special internal processing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identity {
    /// Message from a local component
    ///
    /// The identity uniquely identifies the component within the local namespace.
    /// These messages MUST be validated against the directory (sign-in check).
    Component { identity: Vec<u8> },
    /// Message from a remote coordinator
    ///
    /// The identity identifies the remote coordinator's connection.
    /// These are already authenticated and handled differently during processing.
    Coordinator { identity: Vec<u8> },
    /// Message to/from self (loopback)
    ///
    /// Used for coordinator-to-coordinator communication where this coordinator
    /// is both sender and receiver in the same context.
    SelfTarget,
}

/// Interface for sending and receiving messages
///
/// This is a pure transport port - it deals only with raw frames
/// and identity context, without any knowledge of the protocol.
pub trait MessagePort {
    /// Send frames to a target identity
    ///
    /// The port internally routes to the appropriate transport based on the Identity variant.
    /// This abstracts away the underlying transport mechanism (ROUTER/DEALER sockets, etc).
    ///
    /// # Parameters
    /// * `dest_identity` - The target identity (Component, Coordinator, or SelfTarget)
    /// * `frames` - Raw frames to send (ownership is transferred)
    ///
    /// # Note
    /// Sending to SelfTarget is handled internally by the coordinator and typically
    /// returns an error since self-targeted messages should be processed directly.
    fn send(
        &self,
        dest_identity: &Identity,
        frames: Vec<Vec<u8>>,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Receive a message with timeout
    ///
    /// Polls for a single message from the router socket within the timeout period.
    ///
    /// # Parameters
    /// * `timeout_ms` - Timeout in milliseconds
    ///
    /// # Returns
    /// Some((source_context, received_frames)) if a message is available, None on timeout
    fn recv(
        &self,
        timeout_ms: u64,
    ) -> Result<Option<(Identity, Vec<Vec<u8>>)>, Box<dyn std::error::Error>>;

    /// Receive coordinator sign-in responses
    ///
    /// Polls dealer sockets for coordinator sign-in acknowledgments with a short timeout.
    /// Returns empty vector if no pending coordinator connections or no messages available.
    ///
    /// # Returns
    /// A vector of tuples containing (source_context, received_frames)
    fn recv_coordinator_sign_ins(
        &self,
    ) -> Result<Vec<(Identity, Vec<Vec<u8>>)>, Box<dyn std::error::Error>>;
}

/// Interface for connection lifecycle management
///
/// This port handles transport connection operations, abstracting
/// away the underlying technology (e.g., ZeroMQ, TCP, local IPC).
/// These are transport-layer concerns, not domain logic.
pub trait ConnectionManagementPort: Send + Sync {
    /// Start listening for component connections
    ///
    /// Binds to an address to accept connections from local components.
    /// The specific transport mechanism is abstracted away.
    ///
    /// # Parameters
    /// * `address` - Address to listen on (e.g., "tcp://*:12300")
    fn listen_for_components(&mut self, address: &str) -> Result<(), Box<dyn std::error::Error>>;

    /// Connect to a remote coordinator
    ///
    /// Establishes a connection to another coordinator's listening address.
    ///
    /// # Parameters
    /// * `address` - Address of the remote coordinator's listening endpoint
    ///
    /// # Returns
    /// The connection identity for this remote coordinator
    fn connect_to_coordinator(
        &mut self,
        address: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>>;

    /// Disconnect from a remote coordinator
    ///
    /// Closes the connection to a remote coordinator.
    ///
    /// # Parameters
    /// * `identity` - The connection identity of the remote coordinator to disconnect
    fn disconnect_from_coordinator(
        &mut self,
        identity: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;
}
