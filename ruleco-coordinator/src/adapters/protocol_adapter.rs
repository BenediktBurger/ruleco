use ruleco_core::message::MessageView;
use zmq;

/// Adapter for ZMQ protocol operations
pub struct ProtocolAdapter;

impl ProtocolAdapter {
    /// Receive a message from a socket, returning the identity and message
    pub fn receive_message(
        socket: &zmq::Socket,
    ) -> Result<(Vec<u8>, MessageView), Box<dyn std::error::Error>> {
        let identity = socket.recv_bytes(0)?;
        let frames = socket.recv_multipart(0)?;
        let message = MessageView::new(frames)?;

        Ok((identity, message))
    }

    /// Receive a message from a dealer socket (no identity part)
    pub fn receive_message_from_dealer(
        socket: &zmq::Socket,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let frames = socket.recv_multipart(0)?;
        if frames.is_empty() {
            return Err("Invalid message format: no parts".into());
        }

        let message = MessageView::new(frames)?;

        Ok(message)
    }
}
