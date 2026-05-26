use std::cmp::max;

use env_logger::{Builder, Env};
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};

use crate::core::ports::LogPublisher;
use ruleco_core::log_record::LogRecord;

pub struct LoggingConfig {
    pub stderr_level: LevelFilter,
    pub publish_level: LevelFilter,
    pub data_publisher_addr: Option<String>,
    pub topic: String,
}

pub struct DualLogger<P: LogPublisher + Sync + 'static> {
    stderr_logger: env_logger::Logger,
    publisher: Option<P>,
    stderr_level: LevelFilter,
    publish_level: LevelFilter,
}

impl<P: LogPublisher + Sync + 'static> DualLogger<P> {
    pub fn new(
        stderr_logger: env_logger::Logger,
        publisher: Option<P>,
        stderr_level: LevelFilter,
        publish_level: LevelFilter,
    ) -> Self {
        Self {
            stderr_logger,
            publisher,
            stderr_level,
            publish_level,
        }
    }

    pub fn should_publish(&self, level: Level) -> bool {
        level <= self.publish_level
    }
}

impl<P: LogPublisher + Sync + 'static> Log for DualLogger<P> {
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
                if let Err(e) = publisher.publish_log(&log_record) {
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

pub fn init_logger<P: LogPublisher + Sync + 'static>(
    config: &LoggingConfig,
    create_publisher: impl FnOnce(&str, &str) -> Result<P, String>,
) -> Result<(), SetLoggerError> {
    let stderr_logger = Builder::from_env(Env::default().default_filter_or(level_filter_to_str(config.stderr_level)))
        .build();

    let publisher = match &config.data_publisher_addr {
        Some(addr) => match create_publisher(&config.topic, addr) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("Warning: failed to create data protocol log publisher ({addr}): {e}");
                eprintln!("Log publishing disabled. Continuing with stderr logging only.");
                None
            }
        },
        None => None,
    };

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
    use std::sync::{Arc, Mutex};

    struct MockPublisher {
        published: Arc<Mutex<Vec<String>>>,
    }

    impl MockPublisher {
        fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
            let published = Arc::new(Mutex::new(Vec::new()));
            let pub_ref = published.clone();
            (Self { published }, pub_ref)
        }
    }

    impl LogPublisher for MockPublisher {
        fn publish_log(&self, record: &LogRecord) -> Result<(), String> {
            let mut guard = self.published.lock().unwrap();
            guard.push(format!("{}|{}|{}", record.levelname, record.name, record.text));
            Ok(())
        }
    }

    unsafe impl Sync for MockPublisher {}

    #[test]
    fn test_dual_logger_no_publisher() {
        let stderr_logger = Builder::new().build();
        let dual_logger: DualLogger<MockPublisher> = DualLogger::new(stderr_logger, None, LevelFilter::Info, LevelFilter::Warn);
        assert!(dual_logger.enabled(
            &log::Metadata::builder().level(Level::Info).target("test").build()
        ));
    }

    #[test]
    fn test_dual_logger_level_filtering() {
        let stderr_logger = Builder::new().build();
        let dual_logger: DualLogger<MockPublisher> = DualLogger::new(stderr_logger, None, LevelFilter::Info, LevelFilter::Warn);

        assert!(!dual_logger.should_publish(Level::Info));
        assert!(dual_logger.should_publish(Level::Warn));
        assert!(dual_logger.should_publish(Level::Error));
    }

    #[test]
    fn test_dual_logger_with_mock_publisher() {
        let (mock, published_ref) = MockPublisher::new();
        let stderr_logger = Builder::new().build();
        let dual_logger = DualLogger::new(stderr_logger, Some(mock), LevelFilter::Info, LevelFilter::Warn);

        assert!(dual_logger.should_publish(Level::Warn));
        assert!(!dual_logger.should_publish(Level::Info));

        let record = log::Record::builder()
            .level(Level::Warn)
            .target("test_module")
            .args(format_args!("test warning"))
            .build();
        dual_logger.log(&record);

        let guard = published_ref.lock().unwrap();
        assert_eq!(guard.len(), 1);
        assert!(guard[0].contains("WARNING"));
        assert!(guard[0].contains("test warning"));
    }

    #[test]
    fn test_dual_logger_publish_level_filters_messages() {
        let (mock, published_ref) = MockPublisher::new();
        let stderr_logger = Builder::new().build();
        let dual_logger = DualLogger::new(stderr_logger, Some(mock), LevelFilter::Trace, LevelFilter::Error);

        let info_record = log::Record::builder()
            .level(Level::Info)
            .target("test")
            .args(format_args!("info msg"))
            .build();
        let error_record = log::Record::builder()
            .level(Level::Error)
            .target("test")
            .args(format_args!("error msg"))
            .build();

        dual_logger.log(&info_record);
        dual_logger.log(&error_record);

        let guard = published_ref.lock().unwrap();
        assert_eq!(guard.len(), 1);
        assert!(guard[0].contains("error msg"));
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
