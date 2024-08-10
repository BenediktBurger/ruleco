use super::core::{create_conversation_id, ContentTypes};
/// A message in the data protocol
pub struct DataMessage {
    pub topic: Vec<u8>,
    pub header: [u8; 16 + 1],
    pub payload: Vec<Vec<u8>>,
}

impl DataMessage {
    fn new(topic: &str, m_type: u8, content: ContentTypes) -> Self {
        let mut header = [0u8; 17];
        let (one, _two) = header.split_at_mut(16);
        one.copy_from_slice(&create_conversation_id());
        header[16] = m_type;
        let content = match content {
            ContentTypes::Frame(c) => vec![c],
            ContentTypes::Frames(c) => c,
            ContentTypes::Null => vec![vec![]],
        };
        Self {
            topic: topic.as_bytes().to_vec(),
            header: header,
            payload: content,
        }
    }

    fn conversation_id(&self) -> &[u8] {
        &self.header[0..16]
    }

    fn message_type(&self) -> u8 {
        self.header[16]
    }

    fn to_frames(self) -> Vec<Vec<u8>> {
        let header = self.header.to_vec();
        let mut frames: Vec<Vec<u8>> = vec![self.topic, header];
        for frame in self.payload {
            frames.push(frame)
        }
        frames
    }
}

/// A helper to publish some data via the data protocol
///
/// # Examples
///
/// ```
/// use ruleco::data_protocol::DataPublisher;
/// let publisher = DataPublisher::new("pub".to_string(), "localhost", 11100);
/// publisher.send_message("some message".as_bytes().to_vec());
/// ```
pub struct DataPublisher {
    pub name: String,
    socket: zmq::Socket,
}

impl DataPublisher {
    pub fn new(name: String, addr: &str, port: u16) -> Self {
        let ctx = zmq::Context::new();
        let socket = ctx.socket(zmq::PUB).unwrap();
        socket.connect(&format!("tcp://{addr}:{port}")).unwrap();
        let publisher = Self {
            name,
            socket: socket,
        };
        publisher
    }

    /// Send a data message with some content
    pub fn send_message(&self, content: Vec<u8>) {
        let message = DataMessage::new(&self.name, 1, ContentTypes::Frame(content));
        self.socket.send_multipart(message.to_frames(), 0).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_message_type() {
        let dm = DataMessage::new("abc", 5, ContentTypes::Frame(vec![1, 2]));
        assert_eq!(dm.message_type(), 5)
    }

    #[test]
    fn check_conversation_id() {
        let dm = DataMessage::new("abc", 5, ContentTypes::Frame(vec![1, 2]));
        assert!(dm.conversation_id() < &create_conversation_id())
    }
}
