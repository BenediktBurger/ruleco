use crate::message::create_conversation_id;
use crate::protocol_constants::{DataMessageType, DATA_HEADER_SIZE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataMessage {
    pub topic: Vec<u8>,
    pub header: [u8; DATA_HEADER_SIZE],
    pub payload: Vec<Vec<u8>>,
}

impl DataMessage {
    pub fn new(topic: &str, message_type: DataMessageType, content: Vec<Vec<u8>>) -> Self {
        let conversation_id = create_conversation_id();
        Self::with_conversation_id(topic, conversation_id, message_type, content)
    }

    pub fn with_conversation_id(
        topic: &str,
        conversation_id: [u8; 16],
        message_type: DataMessageType,
        content: Vec<Vec<u8>>,
    ) -> Self {
        let mut header = [0u8; DATA_HEADER_SIZE];
        header[0..16].copy_from_slice(&conversation_id);
        header[16] = u8::from(message_type);
        Self {
            topic: topic.as_bytes().to_vec(),
            header,
            payload: content,
        }
    }

    pub fn into_frames(self) -> Vec<Vec<u8>> {
        let mut frames = vec![self.topic, self.header.to_vec()];
        frames.extend(self.payload);
        frames
    }

    pub fn conversation_id(&self) -> &[u8] {
        &self.header[0..16]
    }

    pub fn message_type(&self) -> DataMessageType {
        self.header[16].into()
    }

    pub fn from_frames(frames: Vec<Vec<u8>>) -> Result<Self, DataMessageError> {
        if frames.len() < 3 {
            return Err(DataMessageError::InvalidFrameCount);
        }
        let topic = frames[0].clone();
        let header_bytes = &frames[1];
        if header_bytes.len() != DATA_HEADER_SIZE {
            return Err(DataMessageError::InvalidHeaderSize);
        }
        let mut header = [0u8; DATA_HEADER_SIZE];
        header.copy_from_slice(header_bytes);
        let payload = frames[2..].to_vec();
        Ok(Self {
            topic,
            header,
            payload,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DataMessageError {
    #[error("invalid frame count")]
    InvalidFrameCount,
    #[error("invalid header size: expected {DATA_HEADER_SIZE} bytes")]
    InvalidHeaderSize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_message_roundtrip() {
        let content = vec![b"hello".to_vec(), b"world".to_vec()];
        let msg = DataMessage::new("N1.Sensor", DataMessageType::Json, content.clone());
        let conv_id = msg.conversation_id().to_vec();
        let msg_type = msg.message_type();

        let frames = msg.into_frames();
        let parsed = DataMessage::from_frames(frames).unwrap();

        assert_eq!(parsed.topic, b"N1.Sensor");
        assert_eq!(parsed.conversation_id(), conv_id.as_slice());
        assert_eq!(parsed.message_type(), msg_type);
        assert_eq!(parsed.payload, content);
    }

    #[test]
    fn test_data_message_spec_vector() {
        let conversation_id: [u8; 16] = [
            0x01, 0x90, 0xa2, 0xb3, 0xc4, 0xd5, 0xe6, 0xf7,
            0xa8, 0xb9, 0xc0, 0xd1, 0xe2, 0xf3, 0xa4, 0xb5,
        ];
        let json_content = r#"["2025-04-24 12:00:00","INFO","recorder","Measurement started"]"#;
        let content = vec![json_content.as_bytes().to_vec()];

        let msg = DataMessage::with_conversation_id(
            "N1.Recorder",
            conversation_id,
            DataMessageType::Json,
            content,
        );

        let frames = msg.into_frames();

        assert_eq!(frames[0], b"N1.Recorder");
        assert_eq!(frames[1].len(), 17);
        assert_eq!(&frames[1][0..16], &conversation_id);
        assert_eq!(frames[1][16], 0x01);
        assert_eq!(frames[2], json_content.as_bytes());
    }

    #[test]
    fn test_from_frames_rejects_oversized_header() {
        let frames = vec![
            b"N1.Sensor".to_vec(),
            vec![0u8; 20],
            b"content".to_vec(),
        ];
        let result = DataMessage::from_frames(frames);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_frames_rejects_undersized_header() {
        let frames = vec![
            b"N1.Sensor".to_vec(),
            vec![0u8; 10],
            b"content".to_vec(),
        ];
        let result = DataMessage::from_frames(frames);
        assert!(result.is_err());
    }
}
