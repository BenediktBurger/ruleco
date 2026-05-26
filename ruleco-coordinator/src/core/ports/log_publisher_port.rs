use ruleco_core::log_record::LogRecord;

pub trait LogPublisher: Send {
    fn publish_log(&self, record: &LogRecord) -> Result<(), String>;
}
