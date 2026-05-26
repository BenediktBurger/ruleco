use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl LogLevel {
    pub fn from_log_level(level: log::Level) -> Self {
        match level {
            log::Level::Trace | log::Level::Debug => LogLevel::Debug,
            log::Level::Info => LogLevel::Info,
            log::Level::Warn => LogLevel::Warning,
            log::Level::Error => LogLevel::Error,
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warning => write!(f, "WARNING"),
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub asctime: String,
    pub levelname: LogLevel,
    pub name: String,
    pub text: String,
}

impl LogRecord {
    pub fn from_log_record(record: &log::Record) -> Self {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = duration.as_secs();
        let days = secs / 86400;
        let time_of_day = secs % 86400;
        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;
        let seconds = time_of_day % 60;

        let (year, month, day) = days_to_date(days);
        let asctime = format!(
            "{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}:{seconds:02}"
        );

        Self {
            asctime,
            levelname: LogLevel::from_log_level(record.level()),
            name: record.target().to_string(),
            text: format!("{}", record.args()),
        }
    }

    pub fn to_json_bytes(&self) -> Vec<u8> {
        let arr = serde_json::json!([
            self.asctime,
            self.levelname.to_string(),
            self.name,
            self.text,
        ]);
        serde_json::to_vec(&arr).unwrap_or_default()
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, LogRecordError> {
        let arr: Vec<serde_json::Value> =
            serde_json::from_slice(bytes).map_err(LogRecordError::Json)?;
        if arr.len() != 4 {
            return Err(LogRecordError::InvalidFormat);
        }
        let asctime = arr[0]
            .as_str()
            .ok_or(LogRecordError::InvalidFormat)?
            .to_string();
        let levelname_str = arr[1]
            .as_str()
            .ok_or(LogRecordError::InvalidFormat)?;
        let levelname = parse_log_level(levelname_str)?;
        let name = arr[2]
            .as_str()
            .ok_or(LogRecordError::InvalidFormat)?
            .to_string();
        let text = arr[3]
            .as_str()
            .ok_or(LogRecordError::InvalidFormat)?
            .to_string();
        Ok(Self {
            asctime,
            levelname,
            name,
            text,
        })
    }
}

fn parse_log_level(s: &str) -> Result<LogLevel, LogRecordError> {
    match s {
        "DEBUG" => Ok(LogLevel::Debug),
        "INFO" => Ok(LogLevel::Info),
        "WARNING" => Ok(LogLevel::Warning),
        "ERROR" => Ok(LogLevel::Error),
        "CRITICAL" => Ok(LogLevel::Critical),
        _ => Err(LogRecordError::InvalidFormat),
    }
}

fn days_to_date(days_since_epoch: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    let mut remaining = days_since_epoch;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }
    let leap = is_leap_year(year);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut month = 1u64;
    for &days_in_month in &month_days {
        if remaining < days_in_month {
            break;
        }
        remaining -= days_in_month;
        month += 1;
    }
    (year, month, remaining + 1)
}

fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[derive(Debug, thiserror::Error)]
pub enum LogRecordError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid format")]
    InvalidFormat,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_record_serialization() {
        let record = LogRecord {
            asctime: "2025-04-24 12:00:00".to_string(),
            levelname: LogLevel::Info,
            name: "recorder".to_string(),
            text: "Measurement started".to_string(),
        };
        let bytes = record.to_json_bytes();
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 4);
        assert_eq!(parsed[0], "2025-04-24 12:00:00");
        assert_eq!(parsed[1], "INFO");
        assert_eq!(parsed[2], "recorder");
        assert_eq!(parsed[3], "Measurement started");
    }

    #[test]
    fn test_log_record_deserialization() {
        let record = LogRecord {
            asctime: "2025-04-24 12:00:00".to_string(),
            levelname: LogLevel::Info,
            name: "recorder".to_string(),
            text: "Measurement started".to_string(),
        };
        let bytes = record.to_json_bytes();
        let parsed = LogRecord::from_json_bytes(&bytes).unwrap();
        assert_eq!(parsed.asctime, record.asctime);
        assert_eq!(parsed.levelname, record.levelname);
        assert_eq!(parsed.name, record.name);
        assert_eq!(parsed.text, record.text);
    }

    #[test]
    fn test_log_level_from_log() {
        assert_eq!(LogLevel::from_log_level(log::Level::Trace), LogLevel::Debug);
        assert_eq!(LogLevel::from_log_level(log::Level::Debug), LogLevel::Debug);
        assert_eq!(LogLevel::from_log_level(log::Level::Info), LogLevel::Info);
        assert_eq!(LogLevel::from_log_level(log::Level::Warn), LogLevel::Warning);
        assert_eq!(LogLevel::from_log_level(log::Level::Error), LogLevel::Error);
    }
}
