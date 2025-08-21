use ruleco_core::message::MessageView;

/// Interface for sending messages
///
/// This port abstracts the transport mechanism for sending messages,
/// whether locally to components or remotely to other coordinators.
pub trait MessageSenderPort {
    /// Send a message to a local component
    ///
    /// # Parameters
    /// * `identity` - The ZMQ identity of the target local component
    /// * `message` - The message to send
    fn send_to_local(
        &self,
        identity: &[u8],
        message: MessageView,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Send a message to a remote coordinator
    ///
    /// # Parameters
    /// * `dealer_identity` - The ZMQ dealer identity of the target remote coordinator
    /// * `message` - The message to send
    fn send_to_remote(
        &self,
        dealer_identity: &[u8],
        message: MessageView,
    ) -> Result<(), Box<dyn std::error::Error>>;
}
