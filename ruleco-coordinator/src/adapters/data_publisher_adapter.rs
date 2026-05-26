use anyhow::Result;
use ruleco_core::data_message::DataMessage;
use ruleco_core::log_record::LogRecord;
use ruleco_core::protocol_constants::DataMessageType;

use crate::core::ports::LogPublisher;

/// ZMQ PUB socket adapter for publishing data protocol messages.
///
/// Note: ZMQ PUB sockets have a "slow joiner" problem — messages sent
/// immediately after `connect()` may be dropped before the XSUB proxy's
/// subscription handshake completes. Early coordinator log messages could
/// be lost if a data proxy is not yet ready.
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

    pub fn build_log_message(&self, record: &LogRecord) -> DataMessage {
        let content = vec![record.to_json_bytes().unwrap_or_default()];
        DataMessage::new(&self.topic, DataMessageType::Json, content)
    }
}

impl LogPublisher for DataPublisherAdapter {
    fn publish_log(&self, record: &LogRecord) -> Result<(), String> {
        let message = self.build_log_message(record);
        self.socket
            .send_multipart(message.into_frames(), 0)
            .map_err(|e| e.to_string())
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
