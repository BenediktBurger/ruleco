use serde_json::Value;

/// Represents the type of JSON-RPC message
#[derive(Debug, Clone, PartialEq)]
pub enum JsonRpcMessageType {
    /// A JSON-RPC request (has "method" and optionally "params" and "id")
    Request,
    /// A JSON-RPC notification (has "method" and optionally "params" but no "id")
    Notification,
    /// A JSON-RPC success response (has "result" and "id")
    SuccessResponse,
    /// A JSON-RPC error response (has "error" and "id")
    ErrorResponse,
    /// A batch of JSON-RPC messages (an array of requests/responses/notifications)
    Batch(Vec<JsonRpcMessageType>),
    /// Invalid JSON-RPC message
    Invalid,
}

/// Classify a JSON value as a JSON-RPC message type
///
/// This function inspects the structure of a JSON value to determine what type
/// of JSON-RPC message it represents.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use ruleco_core::jsonrpc_utils::{classify_jsonrpc_message, JsonRpcMessageType};
///
/// // Request
/// let request = json!({
///     "method": "subtract",
///     "params": [42, 23],
///     "id": 1
/// });
/// assert_eq!(classify_jsonrpc_message(&request), JsonRpcMessageType::Request);
///
/// // Notification
/// let notification = json!({
///     "method": "update",
///     "params": [1, 2, 3]
/// });
/// assert_eq!(classify_jsonrpc_message(&notification), JsonRpcMessageType::Notification);
///
/// // Success Response
/// let success_response = json!({
///     "result": 19,
///     "id": 1
/// });
/// assert_eq!(classify_jsonrpc_message(&success_response), JsonRpcMessageType::SuccessResponse);
///
/// // Error Response
/// let error_response = json!({
///     "error": {
///         "code": -32601,
///         "message": "Method not found"
///     },
///     "id": 1
/// });
/// assert_eq!(classify_jsonrpc_message(&error_response), JsonRpcMessageType::ErrorResponse);
///
/// // Batch
/// let batch = json!([
///     {"method": "subtract", "params": [42, 23], "id": 1},
///     {"method": "update", "params": [1, 2, 3]}
/// ]);
/// assert_eq!(classify_jsonrpc_message(&batch), JsonRpcMessageType::Batch(vec![
///     JsonRpcMessageType::Request,
///     JsonRpcMessageType::Notification
/// ]));
/// ```
pub fn classify_jsonrpc_message(value: &Value) -> JsonRpcMessageType {
    match value {
        Value::Array(items) => {
            // This is a batch
            let mut types = Vec::with_capacity(items.len());
            for item in items {
                types.push(classify_jsonrpc_message(item));
            }
            JsonRpcMessageType::Batch(types)
        }
        Value::Object(obj) => {
            // Check for response first (has result or error)
            if obj.contains_key("result") {
                if obj.contains_key("id") {
                    JsonRpcMessageType::SuccessResponse
                } else {
                    JsonRpcMessageType::Invalid
                }
            } else if obj.contains_key("error") {
                if obj.contains_key("id") {
                    JsonRpcMessageType::ErrorResponse
                } else {
                    JsonRpcMessageType::Invalid
                }
            } else if obj.contains_key("method") {
                // It's a request or notification
                if obj.contains_key("id") {
                    JsonRpcMessageType::Request
                } else {
                    JsonRpcMessageType::Notification
                }
            } else {
                JsonRpcMessageType::Invalid
            }
        }
        _ => JsonRpcMessageType::Invalid,
    }
}

/// Check if a JSON value is a valid JSON-RPC message
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use ruleco_core::jsonrpc_utils::is_valid_jsonrpc_message;
///
/// let request = json!({
///     "method": "subtract",
///     "params": [42, 23],
///     "id": 1
/// });
/// assert!(is_valid_jsonrpc_message(&request));
///
/// let invalid = json!({
///     "foo": "bar"
/// });
/// assert!(!is_valid_jsonrpc_message(&invalid));
/// ```
pub fn is_valid_jsonrpc_message(value: &Value) -> bool {
    match classify_jsonrpc_message(value) {
        JsonRpcMessageType::Invalid => false,
        JsonRpcMessageType::Batch(types) => {
            // A batch is valid if all items are valid
            types.iter().all(|t| *t != JsonRpcMessageType::Invalid)
        }
        _ => true, // All other types are valid JSON-RPC messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_classify_request() {
        let request = json!({
            "method": "subtract",
            "params": [42, 23],
            "id": 1
        });
        assert_eq!(
            classify_jsonrpc_message(&request),
            JsonRpcMessageType::Request
        );
    }

    #[test]
    fn test_classify_notification() {
        let notification = json!({
            "method": "update",
            "params": [1, 2, 3]
        });
        assert_eq!(
            classify_jsonrpc_message(&notification),
            JsonRpcMessageType::Notification
        );
    }

    #[test]
    fn test_classify_success_response() {
        let success_response = json!({
            "result": 19,
            "id": 1
        });
        assert_eq!(
            classify_jsonrpc_message(&success_response),
            JsonRpcMessageType::SuccessResponse
        );
    }

    #[test]
    fn test_classify_error_response() {
        let error_response = json!({
            "error": {
                "code": -32601,
                "message": "Method not found"
            },
            "id": 1
        });
        assert_eq!(
            classify_jsonrpc_message(&error_response),
            JsonRpcMessageType::ErrorResponse
        );
    }

    #[test]
    fn test_classify_batch() {
        let batch = json!([
            {"method": "subtract", "params": [42, 23], "id": 1},
            {"method": "update", "params": [1, 2, 3]},
            {"result": 19, "id": 2}
        ]);
        assert_eq!(
            classify_jsonrpc_message(&batch),
            JsonRpcMessageType::Batch(vec![
                JsonRpcMessageType::Request,
                JsonRpcMessageType::Notification,
                JsonRpcMessageType::SuccessResponse
            ])
        );
    }

    #[test]
    fn test_classify_invalid() {
        let invalid1 = json!({"foo": "bar"});
        assert_eq!(
            classify_jsonrpc_message(&invalid1),
            JsonRpcMessageType::Invalid
        );

        let invalid2 = json!("not an object or array");
        assert_eq!(
            classify_jsonrpc_message(&invalid2),
            JsonRpcMessageType::Invalid
        );

        // Response without id
        let invalid3 = json!({"result": 19});
        assert_eq!(
            classify_jsonrpc_message(&invalid3),
            JsonRpcMessageType::Invalid
        );

        // Response without id
        let invalid4 = json!({"error": {"code": -32601, "message": "Method not found"}});
        assert_eq!(
            classify_jsonrpc_message(&invalid4),
            JsonRpcMessageType::Invalid
        );
    }

    #[test]
    fn test_is_valid_jsonrpc_message() {
        let request = json!({
            "method": "subtract",
            "params": [42, 23],
            "id": 1
        });
        assert!(is_valid_jsonrpc_message(&request));

        let batch = json!([
            {"method": "subtract", "params": [42, 23], "id": 1},
            {"method": "update", "params": [1, 2, 3]}
        ]);
        assert!(is_valid_jsonrpc_message(&batch));

        let invalid = json!({"foo": "bar"});
        assert!(!is_valid_jsonrpc_message(&invalid));

        // Batch with invalid item
        let invalid_batch = json!([
            {"method": "subtract", "params": [42, 23], "id": 1},
            {"foo": "bar"}
        ]);
        assert!(!is_valid_jsonrpc_message(&invalid_batch));
    }
}
