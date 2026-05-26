use jsonrpsee_types::{ErrorCode, ErrorObject, Id};
use serde_json::Value;
use std::collections::HashMap;

pub type MethodHandler = Box<dyn Fn(Option<Value>) -> Result<Value, ErrorObject<'static>> + Send>;
pub type ActionHandler = Box<dyn Fn(Vec<Value>) -> Result<Value, ErrorObject<'static>> + Send>;

pub fn dispatch_request(
    method: &str,
    params: Option<Value>,
    id: Id,
    handlers: &HashMap<String, MethodHandler>,
    action_handlers: &HashMap<String, ActionHandler>,
    parameters: &mut HashMap<String, Value>,
) -> Option<Value> {
    match method {
        "pong" => Some(make_null_response(id)),
        "shut_down" => Some(make_null_response(id)),
        "get_parameters" => {
            let result = handle_get_parameters(params, parameters);
            Some(make_result_response(id, result))
        }
        "set_parameters" => {
            let result = handle_set_parameters(params, parameters);
            Some(make_result_response(id, result))
        }
        "call_action" => {
            let result = handle_call_action(params, action_handlers);
            match result {
                Ok(v) => Some(make_result_response(id, Ok(v))),
                Err(e) => Some(make_error_response_value(id, e)),
            }
        }
        _ => {
            if let Some(handler) = handlers.get(method) {
                match handler(params) {
                    Ok(result) => Some(make_result_response(id, Ok(result))),
                    Err(e) => Some(make_error_response_value(id, e)),
                }
            } else {
                Some(make_error_response_value(
                    id,
                    ErrorObject::from(ErrorCode::MethodNotFound),
                ))
            }
        }
    }
}

fn handle_get_parameters(
    params: Option<Value>,
    parameters: &HashMap<String, Value>,
) -> Result<Value, ErrorObject<'static>> {
    let params_val = params.unwrap_or(Value::Null);
    let keys = match params_val.get("parameters") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>(),
        _ => {
            return Err(ErrorObject::from(ErrorCode::InvalidParams));
        }
    };

    let mut result = serde_json::Map::new();
    for key in keys {
        if let Some(val) = parameters.get(&key) {
            result.insert(key, val.clone());
        }
    }
    Ok(Value::Object(result))
}

fn handle_set_parameters(
    params: Option<Value>,
    parameters: &mut HashMap<String, Value>,
) -> Result<Value, ErrorObject<'static>> {
    let params_val = params.unwrap_or(Value::Null);
    let new_params = match params_val.get("parameters") {
        Some(Value::Object(obj)) => obj,
        _ => {
            return Err(ErrorObject::from(ErrorCode::InvalidParams));
        }
    };

    for (key, val) in new_params {
        parameters.insert(key.clone(), val.clone());
    }
    Ok(Value::Null)
}

fn handle_call_action(
    params: Option<Value>,
    action_handlers: &HashMap<String, ActionHandler>,
) -> Result<Value, ErrorObject<'static>> {
    let params_val = params.unwrap_or(Value::Null);
    let action_name = match params_val.get("action").and_then(|v| v.as_str()) {
        Some(name) => name.to_string(),
        None => return Err(ErrorObject::from(ErrorCode::InvalidParams)),
    };

    let args = match params_val.get("args") {
        Some(Value::Array(arr)) => arr.clone(),
        _ => Vec::new(),
    };

    match action_handlers.get(&action_name) {
        Some(handler) => handler(args),
        None => Err(ErrorObject::from(ErrorCode::MethodNotFound)),
    }
}

fn make_null_response(id: Id) -> Value {
    let response = jsonrpsee_types::response::Response::new(
        jsonrpsee_types::response::ResponsePayload::Success(std::borrow::Cow::Borrowed(
            &Value::Null,
        )),
        id,
    );
    serde_json::to_value(response).unwrap()
}

fn make_result_response(id: Id, result: Result<Value, ErrorObject<'static>>) -> Value {
    match result {
        Ok(v) => {
            let response = jsonrpsee_types::response::Response::new(
                jsonrpsee_types::response::ResponsePayload::Success(std::borrow::Cow::Borrowed(
                    &v,
                )),
                id,
            );
            serde_json::to_value(response).unwrap()
        }
        Err(e) => make_error_response_value(id, e),
    }
}

fn make_error_response_value(id: Id, error: ErrorObject<'static>) -> Value {
    let response = jsonrpsee_types::response::Response::<()>::new(
        jsonrpsee_types::response::ResponsePayload::Error(error),
        id,
    );
    serde_json::to_value(response).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_dispatch_pong() {
        let result = dispatch_request(
            "pong",
            None,
            Id::Number(1),
            &HashMap::new(),
            &HashMap::new(),
            &mut HashMap::new(),
        );
        assert!(result.is_some());
        let response = result.unwrap();
        assert!(response.get("result").is_some());
    }

    #[test]
    fn test_dispatch_unknown_method() {
        let result = dispatch_request(
            "unknown_method",
            None,
            Id::Number(1),
            &HashMap::new(),
            &HashMap::new(),
            &mut HashMap::new(),
        );
        assert!(result.is_some());
        let response = result.unwrap();
        assert!(response.get("error").is_some());
        let error = response.get("error").unwrap();
        assert_eq!(error.get("code").unwrap().as_i64().unwrap(), -32601);
    }

    #[test]
    fn test_get_parameters() {
        let mut params = HashMap::new();
        params.insert("temperature".to_string(), json!(42.5));

        let request_params = json!({"parameters": ["temperature"]});
        let result = dispatch_request(
            "get_parameters",
            Some(request_params),
            Id::Number(1),
            &HashMap::new(),
            &HashMap::new(),
            &mut params,
        );
        let response = result.unwrap();
        let result_val = response.get("result").unwrap();
        assert_eq!(result_val.get("temperature").unwrap(), &json!(42.5));
    }

    #[test]
    fn test_set_parameters() {
        let mut params = HashMap::new();
        let request_params = json!({"parameters": {"voltage": 3.3}});
        let result = dispatch_request(
            "set_parameters",
            Some(request_params),
            Id::Number(1),
            &HashMap::new(),
            &HashMap::new(),
            &mut params,
        );
        let response = result.unwrap();
        assert_eq!(response.get("result").unwrap(), &Value::Null);
        assert_eq!(params.get("voltage").unwrap(), &json!(3.3));
    }

    #[test]
    fn test_custom_handler() {
        let mut handlers: HashMap<String, MethodHandler> = HashMap::new();
        handlers.insert(
            "echo".to_string(),
            Box::new(|params| {
                let val = params.unwrap_or(Value::Null);
                Ok(val)
            }),
        );

        let request_params = json!("hello");
        let result = dispatch_request(
            "echo",
            Some(request_params),
            Id::Number(1),
            &handlers,
            &HashMap::new(),
            &mut HashMap::new(),
        );
        let response = result.unwrap();
        assert_eq!(response.get("result").unwrap(), &json!("hello"));
    }
}
