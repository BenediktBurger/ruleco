//! Do some json interpreting
//! Replace later with proper crate, e.g. jsonrpsee
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct Request {
    jsonrpc: String,
    pub id: u16,
    pub method: String,
}
impl Request {
    pub fn build<T: ToString>(id: u16, method: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Response {
    jsonrpc: String,
    pub id: u16,
    pub result: Value, // properly an object
}
impl Response {
    pub fn build(id: u16, result: impl Serialize) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: serde_json::json!(result),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ErrorContent {
    code: i16,
    message: String,
}

#[derive(Serialize, Deserialize)]
pub struct ErrorResponse {
    jsonrpc: String,
    pub id: u16,
    pub error: ErrorContent,
}
impl ErrorResponse {
    pub fn build(id: u16, code: i16, message: &str) -> Self {
        let error = ErrorContent {
            code,
            message: message.to_string(),
        };
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            error,
        }
    }
}

pub fn to_vec(obj: &impl Serialize) -> Vec<u8> {
    serde_json::to_vec(obj).unwrap()
}

pub fn is_sign_in(slice: &[u8]) -> bool {
    match serde_json::from_slice::<Request>(slice) {
        Err(_) => false,
        Ok(request) => request.method == String::from("sign_in"),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_response_null() {
        let response = Response::build(1, None::<u8>);
        let string = serde_json::to_string(&response).unwrap();
        assert_eq!(string, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":null}")
    }

    #[test]
    fn test_response_number() {
        let response = Response::build(1, 123);
        let string = serde_json::to_string(&response).unwrap();
        assert_eq!(string, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":123}")
    }
}
