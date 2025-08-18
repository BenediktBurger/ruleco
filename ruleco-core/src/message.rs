use crate::full_name::{FullName, FullNameError};
use crate::protocol_constants::{MessageType, VERSION};
use std::io;
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

    /// Gets the raw message type value.
    pub fn message_type_raw(&self) -> u8 {
        *self.message_type
    }

    /// Gets the message type, interpreting it according to the standard `MessageType` enum.
    /// Unknown types (e.g., custom user types >127) will be mapped to `MessageType::Undefined`.
    pub fn message_type_enum(&self) -> MessageType {
        self.message_type_raw().into()
    }
}

/// Different types of content
pub enum ContentTypes {
    Frames(Vec<Vec<u8>>),
    Frame(Vec<u8>),
    Null,
}

#[derive(Clone)]
pub struct Message {
    pub frames: Vec<Vec<u8>>,
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
    pub fn receiver(&self) -> Result<FullName, FullNameError> {
        FullName::from_slice(&self.frames[1])
    }
    pub fn sender_frame(&self) -> &Vec<u8> {
        &self.frames[2]
    }
    pub fn sender(&self) -> Result<FullName, FullNameError> {
        FullName::from_slice(&self.frames[2])
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

#[cfg(test)]
mod tests {
    use super::{ContentTypes, Message};
    use crate::protocol_constants::{VERSION, MessageType};

    fn create_message() -> Message {
        Message::build(
            b"N1.receiver".to_vec(),
            b"N1.sender".to_vec(),
            None,
            None,
            MessageType::Json.into(),
            ContentTypes::Frame(b"content".to_vec()),
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
        let receiver = msg.receiver().unwrap();
        assert_eq!(receiver.namespace(), b"N1");
        assert_eq!(receiver.name(), b"receiver");
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
    fn test_header_message_type_enum() {
        let msg = create_message();
        let header = msg.header();
        assert_eq!(header.message_type_enum(), MessageType::Json);

        // Test with a custom type
        let custom_msg = Message::build(
            b"N1.receiver".to_vec(),
            b"N1.sender".to_vec(),
            None,
            None,
            150, // Custom type
            ContentTypes::Frame(b"content".to_vec()),
        );
        let custom_header = custom_msg.header();
        assert_eq!(custom_header.message_type_enum(), MessageType::Undefined);
        assert_eq!(custom_header.message_type_raw(), 150);
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
