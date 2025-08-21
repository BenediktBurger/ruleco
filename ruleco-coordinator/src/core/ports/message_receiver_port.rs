use ruleco_core::message::MessageView;

/// Interface for receiving messages
///
/// This port abstracts the transport mechanism for receiving messages,
/// whether locally from components or remotely from other coordinators.
pub trait MessageReceiverPort {
    /// Receive a message from a local component, returning the identity and message
    ///
    /// # Returns
    /// A tuple containing the ZMQ identity of the sender and the received message
    fn receive_message_from_local(
        &self,
    ) -> Result<(Vec<u8>, MessageView), Box<dyn std::error::Error>>;

    /// Receive a message from a remote coordinator
    ///
    /// # Parameters
    /// * `dealer_identity` - The ZMQ dealer identity of the remote coordinator
    ///
    /// # Returns
    /// The received message
    fn receive_message_from_remote(
        &self,
        dealer_identity: &[u8],
    ) -> Result<MessageView, Box<dyn std::error::Error>>;

    /// Receive all available messages within a timeout
    ///
    /// # Parameters
    /// * `timeout_ms` - Timeout in milliseconds
    ///
    /// # Returns
    /// A vector of available messages with their source information
    fn receive_messages(
        &self,
        _timeout_ms: u64,
    ) -> Result<Vec<(Identity, MessageView)>, Box<dyn std::error::Error>> {
        // Default implementation that returns an empty vector
        // This should be overridden by implementations that support receiving multiple messages
        Ok(vec![])
    }
}

/// Identity of the sender from which the message came.
pub enum Identity {
    SELF,
    Local { identity: Vec<u8> },
    Remote { identity: Vec<u8> },
}
