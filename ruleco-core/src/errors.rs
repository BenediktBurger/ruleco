use jsonrpsee_types::ErrorCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// LECO (Laboratory Experimental Control Protocol) specific errors
/// These extend the standard JSON-RPC error codes with LECO-specific ones
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LecoError {
    /// Component not signed in yet
    NotSignedIn {
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },

    /// The name is already taken
    DuplicateName {
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },

    /// Node is unknown
    NodeUnknown {
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },

    /// Receiver is not in addresses list
    ReceiverUnknown {
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },

    /// Generic server error with optional data
    #[serde(rename = "server_error")]
    ServerError {
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },
}

impl LecoError {
    /// Get the JSON-RPC error code for this LECO error
    /// Uses codes in the -32000 to -32099 range as recommended by JSON-RPC specification
    pub fn code(&self) -> i32 {
        match self {
            Self::NotSignedIn { .. } => -32090,
            Self::DuplicateName { .. } => -32091,
            Self::NodeUnknown { .. } => -32092,
            Self::ReceiverUnknown { .. } => -32093,
            Self::ServerError { .. } => -32000,
        }
    }

    /// Get the standard error message for this LECO error
    pub fn message(&self) -> &'static str {
        match self {
            Self::NotSignedIn { .. } => "Component not signed in yet!",
            Self::DuplicateName { .. } => "The name is already taken.",
            Self::NodeUnknown { .. } => "Node is unknown.",
            Self::ReceiverUnknown { .. } => "Receiver is not in addresses list.",
            Self::ServerError { .. } => "Server error",
        }
    }

    /// Get additional error data if available
    pub fn data(&self) -> Option<&Value> {
        match self {
            Self::NotSignedIn { data } => data.as_ref(),
            Self::DuplicateName { data } => data.as_ref(),
            Self::NodeUnknown { data } => data.as_ref(),
            Self::ReceiverUnknown { data } => data.as_ref(),
            Self::ServerError { data } => data.as_ref(),
        }
    }

    /// Create a not signed in error with optional data
    pub fn not_signed_in(data: Option<Value>) -> Self {
        Self::NotSignedIn { data }
    }

    /// Create a duplicate name error with optional data
    pub fn duplicate_name(data: Option<Value>) -> Self {
        Self::DuplicateName { data }
    }

    /// Create a node unknown error with optional data
    pub fn node_unknown(data: Option<Value>) -> Self {
        Self::NodeUnknown { data }
    }

    /// Create a receiver unknown error with optional data
    pub fn receiver_unknown(data: Option<Value>) -> Self {
        Self::ReceiverUnknown { data }
    }

    /// Create a server error with optional data
    pub fn server_error(data: Option<Value>) -> Self {
        Self::ServerError { data }
    }
}

impl fmt::Display for LecoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for LecoError {}

// Conversion to jsonrpsee ErrorObject
impl From<LecoError> for jsonrpsee_types::error::ErrorObject<'static> {
    fn from(err: LecoError) -> Self {
        jsonrpsee_types::error::ErrorObject::owned(err.code(), err.message(), err.data().cloned())
    }
}

/// Re-export of jsonrpsee's error types for convenience
pub use jsonrpsee_types::error::ErrorObject;

/// A wrapper for either LECO-specific errors, JSON-RPC errors, or custom errors
#[derive(Debug, Clone)]
pub enum Error {
    /// LECO-specific errors
    Leco(LecoError),

    /// JSON-RPC errors from jsonrpsee
    JsonRpc(ErrorObject<'static>),

    /// Custom errors with code and message
    Custom(i32, String),
}

impl Error {
    pub fn code(&self) -> i32 {
        match self {
            Self::Leco(leco_err) => leco_err.code(),
            Self::JsonRpc(json_rpc_err) => json_rpc_err.code(),
            Self::Custom(code, _) => *code,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Leco(leco_err) => leco_err.message().to_string(),
            Self::JsonRpc(json_rpc_err) => json_rpc_err.message().to_string(),
            Self::Custom(_, message) => message.clone(),
        }
    }

    // Convenience constructors for LECO errors
    pub fn not_signed_in() -> Self {
        Self::Leco(LecoError::not_signed_in(None))
    }

    pub fn not_signed_in_with_data(data: Value) -> Self {
        Self::Leco(LecoError::not_signed_in(Some(data)))
    }

    pub fn duplicate_name() -> Self {
        Self::Leco(LecoError::duplicate_name(None))
    }

    pub fn duplicate_name_with_data(data: Value) -> Self {
        Self::Leco(LecoError::duplicate_name(Some(data)))
    }

    pub fn node_unknown() -> Self {
        Self::Leco(LecoError::node_unknown(None))
    }

    pub fn node_unknown_with_data(data: Value) -> Self {
        Self::Leco(LecoError::node_unknown(Some(data)))
    }

    pub fn receiver_unknown() -> Self {
        Self::Leco(LecoError::receiver_unknown(None))
    }

    pub fn receiver_unknown_with_data(data: Value) -> Self {
        Self::Leco(LecoError::receiver_unknown(Some(data)))
    }

    // Constructor for custom errors
    pub fn custom(code: i32, message: String) -> Self {
        Self::Custom(code, message)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for Error {}

impl From<LecoError> for Error {
    fn from(leco_err: LecoError) -> Self {
        Self::Leco(leco_err)
    }
}

impl From<ErrorObject<'static>> for Error {
    fn from(json_rpc_err: ErrorObject<'static>) -> Self {
        Self::JsonRpc(json_rpc_err)
    }
}

impl From<ErrorCode> for Error {
    fn from(error_code: ErrorCode) -> Self {
        let obj = ErrorObject::from(error_code);
        Self::from(obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_leco_error_codes() {
        assert_eq!(LecoError::not_signed_in(None).code(), -32090);
        assert_eq!(LecoError::duplicate_name(None).code(), -32091);
        assert_eq!(LecoError::node_unknown(None).code(), -32092);
        assert_eq!(LecoError::receiver_unknown(None).code(), -32093);
        assert_eq!(LecoError::server_error(None).code(), -32000);
    }

    #[test]
    fn test_leco_error_messages() {
        assert_eq!(
            LecoError::not_signed_in(None).message(),
            "Component not signed in yet!"
        );
        assert_eq!(
            LecoError::duplicate_name(None).message(),
            "The name is already taken."
        );
        assert_eq!(LecoError::node_unknown(None).message(), "Node is unknown.");
        assert_eq!(
            LecoError::receiver_unknown(None).message(),
            "Receiver is not in addresses list."
        );
        assert_eq!(LecoError::server_error(None).message(), "Server error");
    }

    #[test]
    fn test_leco_error_with_data() {
        let data = json!({"details": "Additional info"});
        let error = LecoError::not_signed_in(Some(data.clone()));
        assert_eq!(error.data(), Some(&data));

        let error = LecoError::duplicate_name(Some(data.clone()));
        assert_eq!(error.data(), Some(&data));

        let error = LecoError::node_unknown(Some(data.clone()));
        assert_eq!(error.data(), Some(&data));

        let error = LecoError::receiver_unknown(Some(data.clone()));
        assert_eq!(error.data(), Some(&data));

        let error = LecoError::server_error(Some(data.clone()));
        assert_eq!(error.data(), Some(&data));
    }

    #[test]
    fn test_leco_error_without_data() {
        let error = LecoError::not_signed_in(None);
        assert_eq!(error.data(), None);

        let error = LecoError::duplicate_name(None);
        assert_eq!(error.data(), None);

        let error = LecoError::node_unknown(None);
        assert_eq!(error.data(), None);

        let error = LecoError::receiver_unknown(None);
        assert_eq!(error.data(), None);

        let error = LecoError::server_error(None);
        assert_eq!(error.data(), None);
    }

    #[test]
    fn test_error_wrapper_constructors() {
        let error = Error::not_signed_in();
        assert_eq!(error.code(), -32090);
        assert_eq!(error.message(), "Component not signed in yet!");

        let error = Error::duplicate_name();
        assert_eq!(error.code(), -32091);
        assert_eq!(error.message(), "The name is already taken.");

        let error = Error::node_unknown();
        assert_eq!(error.code(), -32092);
        assert_eq!(error.message(), "Node is unknown.");

        let error = Error::receiver_unknown();
        assert_eq!(error.code(), -32093);
        assert_eq!(error.message(), "Receiver is not in addresses list.");
    }

    #[test]
    fn test_error_wrapper_with_data_constructors() {
        let data = json!({"component": "test_component"});
        let error = Error::not_signed_in_with_data(data.clone());
        assert_eq!(error.code(), -32090);
        // Check that the underlying LecoError has the data
        match error {
            Error::Leco(leco_err) => assert_eq!(leco_err.data(), Some(&data)),
            _ => panic!("Expected Leco error"),
        }

        let data = json!({"name": "duplicate_test"});
        let error = Error::duplicate_name_with_data(data.clone());
        assert_eq!(error.code(), -32091);
        match error {
            Error::Leco(leco_err) => assert_eq!(leco_err.data(), Some(&data)),
            _ => panic!("Expected Leco error"),
        }
    }

    #[test]
    fn test_custom_error() {
        let error = Error::custom(1000, "Custom error message".to_string());
        assert_eq!(error.code(), 1000);
        assert_eq!(error.message(), "Custom error message");
    }

    #[test]
    fn test_from_leco_error() {
        let leco_error = LecoError::not_signed_in(None);
        let error: Error = leco_error.into();
        assert_eq!(error.code(), -32090);
    }

    #[test]
    fn test_serialization_deserialization() {
        // Test serialization of error with data
        let data = json!({"test": "value"});
        let error = LecoError::not_signed_in(Some(data));

        let serialized = serde_json::to_string(&error).unwrap();
        let deserialized: LecoError = serde_json::from_str(&serialized).unwrap();

        assert_eq!(error, deserialized);

        // Test serialization of error without data
        let error = LecoError::duplicate_name(None);
        let serialized = serde_json::to_string(&error).unwrap();
        let deserialized: LecoError = serde_json::from_str(&serialized).unwrap();

        assert_eq!(error, deserialized);
    }

    #[test]
    fn test_conversion_to_jsonrpsee_error() {
        let leco_error = LecoError::not_signed_in(None);
        let jsonrpsee_error: jsonrpsee_types::error::ErrorObject = leco_error.into();

        assert_eq!(jsonrpsee_error.code(), -32090);
        assert_eq!(jsonrpsee_error.message(), "Component not signed in yet!");

        // Test with data
        let data = json!({"component_id": "test"});
        let leco_error = LecoError::node_unknown(Some(data.clone()));
        let jsonrpsee_error: jsonrpsee_types::error::ErrorObject = leco_error.into();

        assert_eq!(jsonrpsee_error.code(), -32092);
        assert_eq!(jsonrpsee_error.message(), "Node is unknown.");
        // For jsonrpsee ErrorObject, we need to check data differently
        assert!(jsonrpsee_error.data().is_some());
    }
}
