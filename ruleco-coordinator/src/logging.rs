use std::cmp::max;
use std::sync::Mutex;

use env_logger::{Builder, Env};
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};

use crate::adapters::data_publisher_adapter::DataPublisherAdapter;
use ruleco_core::log_record::LogRecord;

pub struct LoggingConfig {
    pub stderr_level: LevelFilter,
    pub publish_level: LevelFilter,
    pub data_publisher_addr: Option<String>,
    pub topic: String,
}

pub struct DualLogger {
    stderr_logger: env_logger::Logger,
    publisher: Option<Mutex<DataPublisherAdapter>>,
    stderr_level: LevelFilter,
    publish_level: LevelFilter,
}

impl DualLogger {
    pub fn new(
        stderr_logger: env_logger::Logger,
        publisher: Option<DataPublisherAdapter>,
        stderr_level: LevelFilter,
        publish_level: LevelFilter,
    ) -> Self {
        Self {
            stderr_logger,
            publisher: publisher.map(Mutex::new),
            stderr_level,
            publish_level,
        }
    }

    pub fn should_publish(&self, level: Level) -> bool {
        level <= self.publish_level
    }
}

impl Log for DualLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= max(self.stderr_level, self.publish_level)
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let logged_to_stderr = record.level() <= self.stderr_level;
        if logged_to_stderr {
            self.stderr_logger.log(record);
        }

        if record.level() <= self.publish_level {
            if let Some(publisher) = &self.publisher {
                let log_record = LogRecord::from_log_record(record);
                let guard = publisher.lock().unwrap();
                let message = guard.build_log_message(&log_record);
                if let Err(e) = guard.publish(message) {
                    if !logged_to_stderr {
                        eprintln!("DualLogger: failed to publish log: {e}");
                    }
                }
            }
        }
    }

    fn flush(&self) {
        self.stderr_logger.flush();
    }
}

fn level_filter_to_str(level: LevelFilter) -> &'static str {
    match level {
        LevelFilter::Off => "off",
        LevelFilter::Error => "error",
        LevelFilter::Warn => "warn",
        LevelFilter::Info => "info",
        LevelFilter::Debug => "debug",
        LevelFilter::Trace => "trace",
    }
}

pub fn init_logger(config: &LoggingConfig) -> Result<(), SetLoggerError> {
    let stderr_logger = Builder::from_env(Env::default().default_filter_or(level_filter_to_str(config.stderr_level)))
        .build();

    let publisher = config
        .data_publisher_addr
        .as_ref()
        .and_then(|addr| DataPublisherAdapter::new(&config.topic, addr).ok());

    let dual_logger = DualLogger::new(
        stderr_logger,
        publisher,
        config.stderr_level,
        config.publish_level,
    );

    let max_level = max(config.stderr_level, config.publish_level);
    log::set_max_level(max_level);
    log::set_boxed_logger(Box::new(dual_logger))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dual_logger_no_publisher() {
        let stderr_logger = Builder::new().build();
        let dual_logger = DualLogger::new(stderr_logger, None, LevelFilter::Info, LevelFilter::Warn);
        assert!(dual_logger.enabled(
            &log::Metadata::builder().level(Level::Info).target("test").build()
        ));
    }

    #[test]
    fn test_dual_logger_level_filtering() {
        let stderr_logger = Builder::new().build();
        let dual_logger = DualLogger::new(stderr_logger, None, LevelFilter::Info, LevelFilter::Warn);

        assert!(!dual_logger.should_publish(Level::Info));
        assert!(dual_logger.should_publish(Level::Warn));
        assert!(dual_logger.should_publish(Level::Error));
    }

    #[test]
    fn test_logging_config_defaults() {
        let config = LoggingConfig {
            stderr_level: LevelFilter::Info,
            publish_level: LevelFilter::Warn,
            data_publisher_addr: None,
            topic: "N1.Coordinator".to_string(),
        };

        assert_eq!(config.stderr_level, LevelFilter::Info);
        assert_eq!(config.publish_level, LevelFilter::Warn);
        assert!(config.data_publisher_addr.is_none());
        assert_eq!(config.topic, "N1.Coordinator");
    }
}
