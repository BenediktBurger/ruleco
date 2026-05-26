use std::sync::Mutex;

use ruleco_core::log_record::LogRecord;

use crate::core::ports::LogPublisher;

pub struct SyncPublisher<P: LogPublisher + Send> {
    inner: Mutex<P>,
}

impl<P: LogPublisher + Send> SyncPublisher<P> {
    pub fn new(inner: P) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }
}

impl<P: LogPublisher + Send> LogPublisher for SyncPublisher<P> {
    fn publish_log(&self, record: &LogRecord) -> Result<(), String> {
        self.inner
            .lock()
            .map_err(|e| format!("publisher lock poisoned: {e}"))?
            .publish_log(record)
    }
}

unsafe impl<P: LogPublisher + Send> Sync for SyncPublisher<P> {}
