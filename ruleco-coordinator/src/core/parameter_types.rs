//! Parameter types for methods

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parameters for add_nodes method
#[derive(Serialize, Deserialize, Debug)]
pub struct AddNodesParams {
    /// Map of namespaces to their coordinator addresses (e.g., "N2" -> "tcp://127.0.0.1:5555")
    pub nodes: HashMap<String, String>,
}

/// Parameters for record_components method
#[derive(Serialize, Deserialize, Debug)]
pub struct RecordComponentsParams {
    pub components: Vec<String>,
}

/// Parameters for remove_expired_addresses method
#[derive(Serialize, Deserialize, Debug)]
pub struct RemoveExpiredAddressesParams {
    /// Expiration time in seconds
    pub expiration_time: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_record_components_params() {
        let json = r#"{"components": ["N1.ComponentA", "N1.ComponentB"]}"#;
        let params: RecordComponentsParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.components.len(), 2);
        assert_eq!(params.components[0], "N1.ComponentA");
        assert_eq!(params.components[1], "N1.ComponentB");
    }

    #[test]
    fn test_deserialize_remove_expired_addresses_params() {
        let json = r#"{"expiration_time": 30.5}"#;
        let params: RemoveExpiredAddressesParams = serde_json::from_str(json).unwrap();

        assert!((params.expiration_time - 30.5).abs() < f64::EPSILON);
    }
}
