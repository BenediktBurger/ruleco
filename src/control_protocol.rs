use std::io;

use crate::{
    core::{create_conversation_id, ContentTypes, FullName},
    VERSION,
};

pub struct Header<'b> {
    pub conversation_id: &'b [u8],
    pub message_id: &'b [u8],
    pub message_type: &'b u8,
}
impl<'b> Header<'b> {
    fn from_frame(frame: &'b Vec<u8>) -> Self {
        Self {
            conversation_id: &frame[..16],
            message_id: &frame[16..16 + 3],
            message_type: &frame[19],
        }
    }
}

#[derive(Clone)]
pub struct Message {
    frames: Vec<Vec<u8>>,
}

impl Message {
    pub fn new(frames: Vec<Vec<u8>>) -> Result<Self, io::Error> {
        if frames.len() < 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not enough frames.",
            ));
        }
        Ok(Self { frames })
    }
    pub fn build(
        receiver: Vec<u8>,
        sender: Vec<u8>,
        conversation_id: Option<&[u8]>,
        message_id: Option<&[u8]>,
        message_type: u8,
        content: ContentTypes,
    ) -> Self {
        let mut header = conversation_id
            .unwrap_or(&create_conversation_id())
            .to_vec();
        header.extend_from_slice(message_id.unwrap_or(&[0, 0, 0]));
        header.push(message_type);
        let mut vec: Vec<Vec<u8>> = vec![vec![VERSION], receiver, sender, header];
        match content {
            ContentTypes::Frame(frame) => vec.push(frame),
            ContentTypes::Frames(frames) => {
                for frame in frames {
                    vec.push(frame)
                }
            }
            ContentTypes::Null => (),
        };
        Self { frames: vec }
    }
    pub fn version(&self) -> Option<&u8> {
        self.frames[0].get(0)
    }
    pub fn receiver_frame(&self) -> &Vec<u8> {
        &self.frames[1]
    }
    pub fn receiver(&self) -> FullName {
        FullName::from_vec(&self.frames[1]).unwrap()
    }
    pub fn sender_frame(&self) -> &Vec<u8> {
        &self.frames[2]
    }
    pub fn sender(&self) -> FullName {
        FullName::from_vec(&self.frames[2]).unwrap()
    }
    pub fn header(&self) -> Header {
        Header::from_frame(&self.frames[3])
    }
    pub fn content_frame(&self) -> Option<&Vec<u8>> {
        self.frames.get(4)
    }
    pub fn payload(&self) -> &[Vec<u8>] {
        &self.frames[4..]
    }
    pub fn to_frames(&self) -> &Vec<Vec<u8>> {
        &self.frames
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    // JSONRPC 2.0 defined errors
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
    ParseError,
    ServerError,
    // LECO errors
    NotSignedIn,
    DuplicateName,
    NodeUnknown,
    ReceiverUnknown,
}

impl Error {
    pub fn code(&self) -> i16 {
        match &self {
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::InternalError => -32603,
            Self::ParseError => -32700,
            Self::ServerError => -32000,
            Self::NotSignedIn => -32090,
            Self::DuplicateName => -32091,
            Self::NodeUnknown => -32092,
            Self::ReceiverUnknown => -32093,
            //_ => -32000,
        }
    }

    pub fn message(&self) -> &str {
        match &self {
            Self::NotSignedIn => "Component not signed in yet!",
            Self::DuplicateName => "The name is already taken.",
            Self::NodeUnknown => "Node is unknown.",
            Self::ReceiverUnknown => "Receiver is not in addresses list.",
            _ => "Server error.",
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{core::FullName, VERSION};

    use super::Message;

    fn create_message() -> Message {
        Message::build(
            b"N1.receiver".to_vec(),
            b"N1.sender".to_vec(),
            None,
            None,
            1,
            crate::core::ContentTypes::Frame(b"content".to_vec()),
        )
    }

    #[test]
    fn test_version() {
        let msg = create_message();
        assert_eq!(*msg.version().unwrap(), VERSION)
    }
    #[test]
    fn test_receiver() {
        let msg = create_message();
        assert_eq!(
            msg.receiver(),
            FullName {
                namespace: b"N1",
                name: b"receiver"
            }
        )
    }
    #[test]
    fn test_header() {
        let msg = create_message();
        let header = msg.header();
        assert_eq!(header.conversation_id.len(), 16);
        assert_eq!(header.message_id, &[0u8; 3]);
        assert_eq!(header.message_type, &1);
    }
    #[test]
    fn test_content() {
        let msg = create_message();
        assert_eq!(*msg.content_frame().unwrap(), b"content".to_vec())
    }
    #[test]
    fn test_payload() {
        let msg = create_message();
        assert_eq!(msg.payload(), vec![b"content".to_vec()])
    }
}

pub mod communicator;
