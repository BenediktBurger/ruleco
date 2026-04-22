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
    bind_address: Option<String>,
    public_address: Option<String>,
    port: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub namespace: String,
    pub timeout_interval: u64,
    pub bind_address: String,
    pub public_address: String,
    pub port: u16,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            namespace: "Default_Namespace".to_string(),
            timeout_interval: 10,
            bind_address: "tcp://*:12300".to_string(),
            public_address: Self::get_public_address("12300"),
            port: 12300,
        }
    }
}

impl CoordinatorConfig {
    pub fn load() -> Self {
        let config_paths = [
            PathBuf::from("ruleco.toml"),
            Self::get_xdg_config_path().unwrap_or_default(),
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
            bind_address: None,
            public_address: None,
            port: None,
        });

        let namespace = match coordinator_settings.namespace {
            Some(ref ns) if ns.is_empty() => Self::get_hostname(),
            Some(ns) => ns,
            None => default.namespace,
        };

        let mut port = coordinator_settings.port.unwrap_or(default.port);

        let bind_address = coordinator_settings
            .bind_address
            .as_ref()
            .map(|s| s.clone())
            .unwrap_or_else(|| format!("tcp://*:{}", port));

        if coordinator_settings.bind_address.is_some() {
            if let Some(addr_port) = bind_address.strip_prefix("tcp://") {
                if let Some(port_str) = addr_port.rsplit(':').next() {
                    if let Ok(parsed_port) = port_str.parse::<u16>() {
                        port = parsed_port;
                    }
                }
            }
        }

        let public_address = match coordinator_settings.public_address {
            Some(addr) if !addr.is_empty() => addr,
            _ => Self::get_public_address(&port.to_string()),
        };

        Self {
            namespace,
            timeout_interval: coordinator_settings
                .timeout_interval
                .unwrap_or(default.timeout_interval),
            bind_address,
            public_address,
            port,
        }
    }

    fn get_hostname() -> String {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown_host".to_string())
    }

    fn get_xdg_config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("ruleco").join("config.toml"))
    }

    /// Get the public IP address of this machine
    /// First tries to resolve the hostname, then falls back to localhost
    fn get_public_address(port: &str) -> String {
        if let Ok(hostname) = hostname::get() {
            if let Some(hostname_str) = hostname.to_str() {
                if let Ok(addrs) = dns_lookup::lookup_host(hostname_str) {
                    if let Some(addr) = addrs.first() {
                        return format!("tcp://{}:{}", addr, port);
                    }
                }
            }
        }

        eprintln!("Warning: Could not determine public IP address from hostname. Using 127.0.0.1 which will only work for local connections. Please configure 'public_address' in ruleco.toml for mutual coordinator sign-in across machines.");
        format!("tcp://127.0.0.1:{}", port)
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
                bind_address: None,
                public_address: None,
                port: None,
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
                bind_address: Some("tcp://*:5555".to_string()),
                public_address: Some("tcp://192.168.1.100:5555".to_string()),
                port: Some(5555),
            }),
        };

        let config = CoordinatorConfig::from_raw(raw);
        assert_eq!(config.namespace, "custom_namespace");
        assert_eq!(config.timeout_interval, 15);
        assert_eq!(config.bind_address, "tcp://*:5555");
        assert_eq!(config.public_address, "tcp://192.168.1.100:5555");
        assert_eq!(config.port, 5555);
    }

    #[test]
    fn test_from_raw_empty() {
        let raw = RawConfig { coordinator: None };

        let config = CoordinatorConfig::from_raw(raw);
        assert_eq!(config.namespace, "Default_Namespace");
        assert_eq!(config.timeout_interval, 10);
        assert_eq!(config.bind_address, "tcp://*:12300");
        assert_eq!(config.port, 12300);
    }

    #[test]
    fn test_from_raw_empty_namespace() {
        let raw = RawConfig {
            coordinator: Some(CoordinatorRawSettings {
                namespace: Some("".to_string()),
                timeout_interval: None,
                bind_address: None,
                public_address: None,
                port: None,
            }),
        };

        let config = CoordinatorConfig::from_raw(raw);
        let hostname = CoordinatorConfig::get_hostname();
        assert_eq!(config.namespace, hostname);
        assert_eq!(config.timeout_interval, 10);
    }

    #[test]
    fn test_from_raw_with_custom_addresses() {
        let raw = RawConfig {
            coordinator: Some(CoordinatorRawSettings {
                namespace: None,
                timeout_interval: None,
                bind_address: Some("tcp://0.0.0.0:9999".to_string()),
                public_address: Some("tcp://10.0.1.5:9999".to_string()),
                port: None,
            }),
        };

        let config = CoordinatorConfig::from_raw(raw);
        assert_eq!(config.bind_address, "tcp://0.0.0.0:9999");
        assert_eq!(config.public_address, "tcp://10.0.1.5:9999");
        assert_eq!(config.port, 9999);
    }

    #[test]
    fn test_get_hostname() {
        let hostname = CoordinatorConfig::get_hostname();
        assert!(!hostname.is_empty());
    }
}
