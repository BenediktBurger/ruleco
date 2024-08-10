//! # RuLECO
//!
//! `ruleco` is a Rust implementation of the Laboratory Control Protocol (LECO)

const VERSION: u8 = 0; // LECO protocol version

pub mod core {
    use uuid::Uuid;

    /// Create a new conversation id
    pub fn create_conversation_id() -> [u8; 16] {
        //should be a UUIDv7
        //b"conversation_id;"
        let uuid = Uuid::now_v7();
        /*let cid: [u8; 16] = [
            99, 111, 110, 118, 101, 114, 115, 97, 116, 105, 111, 110, 95, 105, 100, 59,
        ];*/
        return uuid.into_bytes();
    }

    /// Different types of content
    pub enum ContentTypes {
        Frames(Vec<Vec<u8>>),
        Frame(Vec<u8>),
        Null,
    }

    /// Describe the full name of a Component with its namespace and name
    ///
    /// # Examples
    ///
    /// ```
    /// use ruleco::core::FullName;
    /// let name_vector = b"namespace_1.name_A".to_vec();
    /// let full_name = FullName::from_vec(&name_vector).unwrap();
    /// assert_eq!(
    ///     full_name,
    ///     FullName {
    ///         namespace: b"namespace_1",
    ///         name: b"name_A",
    /// });
    /// ```
    /// ```
    /// use ruleco::core::FullName;
    /// let full_name = FullName::from_slice(b"namespace_1.name_A").unwrap();
    /// assert_eq!(full_name,
    ///     FullName {
    ///         namespace: b"namespace_1",
    ///         name: b"name_A",
    /// });
    /// ```
    #[derive(PartialEq, Debug)]
    pub struct FullName<'a> {
        pub namespace: &'a [u8],
        pub name: &'a [u8],
    }

    impl<'a> FullName<'a> {
        fn from_split(split: Vec<&'a [u8]>) -> Result<Self, String> {
            match split.len() {
                1 => Ok(Self {
                    namespace: &[],
                    name: split[0],
                }),
                2 => Ok(Self {
                    namespace: split[0],
                    name: split[1],
                }),
                x => Err(format!("Invalid number {x} of elements in name found.")),
            }
        }
        pub fn from_vec(vec: &'a Vec<u8>) -> Result<Self, String> {
            // 46 is value of ASCII "."
            let parts: Vec<&[u8]> = vec.split(|e| *e == 46u8).collect();
            Self::from_split(parts)
        }
        pub fn from_slice(slice: &'a [u8]) -> Result<Self, String> {
            let parts: Vec<&[u8]> = slice.split(|e| *e == 46u8).collect();
            Self::from_split(parts)
        }
    }

    #[cfg(test)]
    mod test {
        use crate::core::FullName;

        #[test]
        fn test_full_name() {
            let full_name = b"abc.def".to_vec();
            assert_eq!(
                FullName::from_vec(&full_name).unwrap(),
                FullName {
                    namespace: b"abc",
                    name: b"def",
                }
            )
        }
        #[test]
        fn test_full_name_without_namespace() {
            let full_name = b"def".to_vec();
            assert_eq!(
                FullName::from_vec(&full_name).unwrap(),
                FullName {
                    namespace: b"",
                    name: b"def",
                }
            )
        }
    }
}

pub mod control_protocol {
    use std::io::BufRead;

    use crate::{
        core::{create_conversation_id, ContentTypes, FullName},
        VERSION,
    };

    pub struct Message {
        pub version: u8,
        pub receiver: Vec<u8>,
        pub sender: Vec<u8>,
        pub header: [u8; 16 + 3 + 1],
        pub payload: Vec<Vec<u8>>,
    }

    impl Message {
        pub fn build(
            receiver: Vec<u8>,
            sender: Vec<u8>,
            conversation_id: Option<&[u8; 16]>,
            message_id: Option<&[u8; 3]>,
            message_type: u8,
            content: ContentTypes,
        ) -> Self {
            let mut header = [0u8; 16 + 3 + 1];
            let cid = match conversation_id {
                None => &create_conversation_id(),
                Some(value) => value,
            };
            header[..16].clone_from_slice(cid);
            header[16..16 + 3].clone_from_slice(message_id.unwrap_or(&[0u8; 3]));
            header[19] = message_type;
            let payload = match content {
                ContentTypes::Frame(frame) => vec![frame],
                ContentTypes::Frames(frames) => frames,
                ContentTypes::Null => vec![vec![]],
            };
            Self {
                version: VERSION,
                receiver,
                sender,
                header,
                payload,
            }
        }

        pub fn receiver(&self) -> FullName {
            FullName::from_vec(&self.receiver).unwrap()
        }
    }

    #[cfg(test)]
    mod tests {}
}

pub mod data_protocol {
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
}
