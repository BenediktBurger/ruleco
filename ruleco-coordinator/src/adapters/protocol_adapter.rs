use ruleco_core::message::{MessageError, MessageView};

/// Protocol adapter for parsing ZMQ frames into messages
pub struct ProtocolAdapter;

impl ProtocolAdapter {
    /// Create a new protocol adapter
    pub fn new() -> Self {
        Self
    }

    /// Parse ZMQ frames into a message
    ///
    /// Takes ownership of the frames and creates a zero-copy MessageView.
    pub fn parse_message(frames: Vec<Vec<u8>>) -> Result<MessageView, MessageError> {
        MessageView::new(frames)
    }

    /// Receive a message from a ZMQ socket
    ///
    /// Returns the identity frame and a zero-copy view of the message.
    pub fn receive_message(
        socket: &zmq::Socket,
    ) -> Result<(Vec<u8>, MessageView), Box<dyn std::error::Error>> {
        let identity = socket.recv_bytes(0)?;
        let frames = socket.recv_multipart(0)?;
        let message = Self::parse_message(frames)?;
        Ok((identity, message))
    }
}
