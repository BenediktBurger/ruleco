use crate::actor::{Actor, ActorHandle};
use anyhow::Result;
use ruleco_core::full_name::FullName;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct TestActor {
    actor: Actor,
    state: Arc<Mutex<HashMap<String, Value>>>,
}

pub struct TestActorHandle {
    handle: ActorHandle,
    state: Arc<Mutex<HashMap<String, Value>>>,
}

impl TestActor {
    pub fn connect(address: &str) -> Result<Self> {
        let actor = Actor::connect(address)?;
        let state = Arc::new(Mutex::new(HashMap::new()));

        Ok(Self { actor, state })
    }

    pub fn sign_in(&mut self, name: &str) -> Result<FullName> {
        self.actor.sign_in(name)
    }

    pub fn sign_out(&mut self) -> Result<()> {
        self.actor.sign_out()
    }

    pub fn call(
        &mut self,
        receiver: &FullName,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value> {
        self.actor.call(receiver, method, params)
    }

    pub fn step(&mut self, timeout_ms: i64) -> Result<bool> {
        self.actor.step(timeout_ms)
    }

    pub fn spawn(mut self) -> Result<TestActorHandle> {
        let state = self.state.clone();
        let state_for_get = state.clone();
        let state_for_set = state.clone();

        self.actor
            .register_method(
                "get_test",
                Box::new(move |params| {
                    let params_val = params.unwrap_or(Value::Null);
                    let keys = match params_val.get("parameters") {
                        Some(Value::Array(arr)) => arr
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>(),
                        _ => {
                            return Err(jsonrpsee_types::error::ErrorObject::from(
                                jsonrpsee_types::ErrorCode::InvalidParams,
                            ));
                        }
                    };

                    let state = state_for_get.lock().unwrap();
                    let mut result = serde_json::Map::new();
                    for key in keys {
                        if let Some(val) = state.get(&key) {
                            result.insert(key, val.clone());
                        }
                    }
                    Ok(Value::Object(result))
                }),
            );

        self.actor
            .register_method(
                "set_test",
                Box::new(move |params| {
                    let params_val = params.unwrap_or(Value::Null);
                    let new_params = match params_val.get("parameters") {
                        Some(Value::Object(obj)) => obj,
                        _ => {
                            return Err(jsonrpsee_types::error::ErrorObject::from(
                                jsonrpsee_types::ErrorCode::InvalidParams,
                            ));
                        }
                    };

                    let mut state = state_for_set.lock().unwrap();
                    for (key, val) in new_params {
                        state.insert(key.clone(), val.clone());
                    }
                    Ok(Value::Null)
                }),
            );

        let handle = self.actor.spawn()?;

        Ok(TestActorHandle { handle, state })
    }

    pub fn register_method(
        &mut self,
        name: &str,
        handler: crate::handlers::MethodHandler,
    ) {
        self.actor.register_method(name, handler);
    }

    pub fn full_name(&self) -> Option<&FullName> {
        self.actor.full_name()
    }
}

impl TestActorHandle {
    pub fn call(
        &self,
        receiver: &FullName,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value> {
        self.handle.call(receiver, method, params)
    }

    pub fn get_test(&self, receiver: &FullName, key: &str) -> Result<Value> {
        let params = serde_json::json!({"parameters": [key]});
        let response = self.handle.call(receiver, "get_test", Some(params))?;
        match response.get("result") {
            Some(Value::Object(map)) => Ok(map.get(key).cloned().unwrap_or(Value::Null)),
            _ => anyhow::bail!("Invalid get_test response"),
        }
    }

    pub fn set_test(&self, receiver: &FullName, key: &str, value: Value) -> Result<Value> {
        let params = serde_json::json!({"parameters": {key: value}});
        let response = self.handle.call(receiver, "set_test", Some(params))?;
        match response.get("result") {
            Some(Value::Null) => Ok(Value::Null),
            _ => anyhow::bail!("Invalid set_test response"),
        }
    }

    pub fn get_state_direct(&self, key: &str) -> Option<Value> {
        self.state.lock().unwrap().get(key).cloned()
    }

    pub fn is_running(&self) -> bool {
        self.handle.is_running()
    }

    pub fn stop(&mut self) -> Result<()> {
        self.handle.stop()
    }

    pub fn full_name(&self) -> Option<FullName> {
        self.handle.full_name()
    }

    pub fn sign_out(&mut self) -> Result<()> {
        self.handle.sign_out()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_test_actor_state() {
        let state: Arc<Mutex<HashMap<String, Value>>> =
            Arc::new(Mutex::new(HashMap::new()));

        state.lock().unwrap().insert("temp".to_string(), json!(42.5));

        assert_eq!(
            state.lock().unwrap().get("temp").cloned().unwrap(),
            json!(42.5)
        );
    }
}
