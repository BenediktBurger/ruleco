use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct RawConfig {
    coordinator: Option<CoordinatorRawSettings>,
}

#[derive(Debug, Deserialize)]
struct CoordinatorRawSettings {
    namespace: Option<String>,
    timeout_interval: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub namespace: String,
    pub timeout_interval: u64,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            namespace: "Default_Namespace".to_string(),
            timeout_interval: 10,
        }
    }
}

impl CoordinatorConfig {
    pub fn load() -> Self {
        let config_paths = [
            PathBuf::from("ruleco.toml"),
            Self::get_xdg_config_path(),
        ];

        for path in config_paths {
            if path.exists() {
                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to read config file {:?}: {}", path, e);
                        continue;
                    }
                };

                match toml::from_str::<RawConfig>(&content) {
                    Ok(raw) => return Self::from_raw(raw),
                    Err(e) => {
                        eprintln!("Failed to parse config file {:?}: {}", path, e);
                        continue;
                    }
                }
            }
        }

        Self::default()
    }

    fn from_raw(raw: RawConfig) -> Self {
        let default = Self::default();

        let coordinator_settings = raw.coordinator.unwrap_or(CoordinatorRawSettings {
            namespace: None,
            timeout_interval: None,
        });

        let namespace = match coordinator_settings.namespace {
            Some(ref ns) if ns.is_empty() => Self::get_hostname(),
            Some(ns) => ns,
            None => default.namespace,
        };

        Self {
            namespace,
            timeout_interval: coordinator_settings
                .timeout_interval
                .unwrap_or(default.timeout_interval),
        }
    }

    fn get_hostname() -> String {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown_host".to_string())
    }

    fn get_xdg_config_path() -> PathBuf {
        if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
            PathBuf::from(config_home).join("ruleco").join("config.toml")
        } else {
            let home = std::env::var("HOME")
                .unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home)
                .join(".config")
                .join("ruleco")
                .join("config.toml")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CoordinatorConfig::default();
        assert_eq!(config.namespace, "Default_Namespace");
        assert_eq!(config.timeout_interval, 10);
    }

    #[test]
    fn test_from_raw_partial() {
        let raw = RawConfig {
            coordinator: Some(CoordinatorRawSettings {
                namespace: Some("test_namespace".to_string()),
                timeout_interval: None,
            }),
        };

        let config = CoordinatorConfig::from_raw(raw);
        assert_eq!(config.namespace, "test_namespace");
        assert_eq!(config.timeout_interval, 10);
    }

    #[test]
    fn test_from_raw_complete() {
        let raw = RawConfig {
            coordinator: Some(CoordinatorRawSettings {
                namespace: Some("custom_namespace".to_string()),
                timeout_interval: Some(15),
            }),
        };

        let config = CoordinatorConfig::from_raw(raw);
        assert_eq!(config.namespace, "custom_namespace");
        assert_eq!(config.timeout_interval, 15);
    }

    #[test]
    fn test_from_raw_empty() {
        let raw = RawConfig { coordinator: None };

        let config = CoordinatorConfig::from_raw(raw);
        assert_eq!(config.namespace, "Default_Namespace");
        assert_eq!(config.timeout_interval, 10);
    }

    #[test]
    fn test_from_raw_empty_namespace() {
        let raw = RawConfig {
            coordinator: Some(CoordinatorRawSettings {
                namespace: Some("".to_string()),
                timeout_interval: None,
            }),
        };

        let config = CoordinatorConfig::from_raw(raw);
        let hostname = CoordinatorConfig::get_hostname();
        assert_eq!(config.namespace, hostname);
        assert_eq!(config.timeout_interval, 10);
    }

    #[test]
    fn test_get_hostname() {
        let hostname = CoordinatorConfig::get_hostname();
        assert!(!hostname.is_empty());
        assert_ne!(hostname, "unknown_host");
    }
}