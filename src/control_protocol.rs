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
            message_type: &frame[20],
        }
    }
}

pub struct Message {
    frames: Vec<Vec<u8>>,
}

impl Message {
    pub fn new(frames: Vec<Vec<u8>>) -> Result<Self, &'static str> {
        if frames.len() < 4 {
            return Err("not enough frames!");
        }
        Ok(Self { frames })
    }
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
        let mut vec: Vec<Vec<u8>> = vec![vec![VERSION], receiver, sender, header.to_vec()];
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
    pub fn receiver(&self) -> FullName {
        FullName::from_vec(&self.frames[1]).unwrap()
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
        assert_eq!(msg.receiver(), FullName {namespace: b"N1", name: b"receiver"})
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
