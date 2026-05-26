use anyhow::Result;
use ruleco_core::data_message::DataMessage;
use ruleco_core::log_record::LogRecord;
use ruleco_core::protocol_constants::DataMessageType;

pub struct DataPublisherAdapter {
    topic: String,
    socket: zmq::Socket,
    #[allow(dead_code)]
    context: zmq::Context,
}

impl Drop for DataPublisherAdapter {
    fn drop(&mut self) {
        let _ = self.socket.set_linger(0);
    }
}

impl DataPublisherAdapter {
    pub fn new(topic: &str, xsub_addr: &str) -> Result<Self> {
        let context = zmq::Context::new();
        let socket = context.socket(zmq::PUB)?;
        socket.connect(xsub_addr)?;
        Ok(Self {
            topic: topic.to_string(),
            socket,
            context,
        })
    }

    pub fn publish(&self, message: DataMessage) -> Result<()> {
        self.socket.send_multipart(message.into_frames(), 0)?;
        Ok(())
    }

    pub fn build_log_message(&self, record: &LogRecord) -> DataMessage {
        DataMessage::new(&self.topic, DataMessageType::Json, vec![record.to_json_bytes()])
    }

    pub fn publish_log(&self, record: &LogRecord) -> Result<()> {
        let message = self.build_log_message(record);
        self.publish(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruleco_core::log_record::LogLevel;

    #[test]
    fn test_build_log_message_constructs_correct_frames() {
        let record = LogRecord {
            asctime: "2025-04-24 12:00:00".to_string(),
            levelname: LogLevel::Info,
            name: "coordinator".to_string(),
            text: "Test message".to_string(),
        };

        let adapter = DataPublisherAdapter::new("N1.Coordinator", "inproc://test_build_log").unwrap();
        let message = adapter.build_log_message(&record);
        let frames = message.into_frames();

        assert_eq!(frames[0], b"N1.Coordinator");
        assert_eq!(frames[1].len(), 17);
        assert_eq!(frames[1][16], DataMessageType::Json as u8);

        let parsed: serde_json::Value = serde_json::from_slice(&frames[2]).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[0], "2025-04-24 12:00:00");
        assert_eq!(parsed[1], "INFO");
        assert_eq!(parsed[2], "coordinator");
        assert_eq!(parsed[3], "Test message");
    }
}
