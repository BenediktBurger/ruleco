use std::{thread, time};
use zmq;

fn main() {
    let publisher = DataPublisher::new("pub".to_string(), "localhost", "11100");
    thread::sleep(time::Duration::from_millis(100));
    publisher.send_message("some message".as_bytes().to_vec());
    println!("Successfully finished");
}

fn create_conversation_id() -> [u8; 16] {
    //should be a UUIDv7
    //b"conversation_id;"
    let cid: [u8; 16] = [
        99, 111, 110, 118, 101, 114, 115, 97, 116, 105, 111, 110, 95, 105, 100, 59,
    ];
    return cid;
}

struct DataMessage {
    topic: Vec<u8>,
    header: [u8; 17],
    payload: Vec<Vec<u8>>,
}

impl DataMessage {
    fn new(topic: &str, m_type: u8, content: Vec<u8>) -> Self {
        let mut header = [0u8; 17];
        let (one, _two) = header.split_at_mut(16);
        one.copy_from_slice(&create_conversation_id());
        header[16] = m_type;
        Self {
            topic: topic.as_bytes().to_vec(),
            header: header,
            payload: vec![content],
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

struct DataPublisher {
    name: String,
    socket: zmq::Socket,
}

impl DataPublisher {
    fn new(name: String, addr: &str, port: &str) -> Self {
        let ctx = zmq::Context::new();
        let socket = ctx.socket(zmq::PUB).unwrap();
        socket.connect(&format!("tcp://{addr}:{port}")).unwrap();
        let publisher = Self {
            name,
            socket: socket,
        };
        publisher
    }

    fn send_message(&self, content: Vec<u8>) {
        let message = DataMessage::new(&self.name, 1, content);
        self.socket.send_multipart(message.to_frames(), 0).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_message_type() {
        let dm = DataMessage::new("abc", 5, vec![1, 2]);
        assert_eq!(dm.message_type(), 5)
    }

    #[test]
    fn check_conversation_id() {
        let dm = DataMessage::new("abc", 5, vec![1, 2]);
        assert_eq!(dm.conversation_id(), create_conversation_id())
    }
}
