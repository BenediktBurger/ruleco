use crate::adapters::{InMemoryDirectoryAdapter, SystemClockAdapter};
use crate::core::domain::CoordinatorEntry;
use crate::core::parameter_types::{AddNodesParams, RemoveExpiredAddressesParams};
use crate::core::ports::clock_port::ClockPort;
use crate::core::ports::message_port::Identity;
use crate::core::ports::DirectoryPort;
use crate::core::CoordinatorCore;
use jsonrpsee_types::request::Request;
use jsonrpsee_types::Params;
use jsonrpsee_types::{
    response::{Response, ResponsePayload},
    ErrorCode, ErrorObject, Id,
};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use ruleco_core::message::{ConversationId, MessageBuilder, MessageView};
use ruleco_core::protocol_constants::MessageType;
use serde_json::Value;
use std::borrow::Cow;
use std::time::Duration;

/// Outcome of handling a JSON-RPC request.
pub enum JsonRpcOutcome {
    /// A response message should be sent back to the client.
    Response(MessageView),
    /// A response message should be sent, but to a specific identity.
    ResponseToIdentity((Identity, MessageView)),
    /// Add new nodes.
    AddNodes(Vec<String>),
    /// The coordinator should shut down.
    Shutdown,
    /// No specific action is required (e.g., for notifications).
    NoAction,
    /// Send directory sync messages to all known remote coordinators.
    DirectorySync(Vec<MessageView>),
}

/// Handler for JSON-RPC requests directed at the Coordinator.
pub struct JsonRpcHandler<'a> {
    /// Reference to the core coordinator logic.
    core: &'a mut CoordinatorCore<InMemoryDirectoryAdapter, SystemClockAdapter>,
    /// Our name as a FullName.
    name: &'a FullName,
}

impl<'a> JsonRpcHandler<'a> {
    /// Create a new JSON-RPC handler.
    pub fn new(
        core: &'a mut CoordinatorCore<InMemoryDirectoryAdapter, SystemClockAdapter>,
        name: &'a FullName,
    ) -> Self {
        Self { core, name }
    }

    /// Handle a JSON-RPC message, which could be a single request or a batch request
    pub fn handle_jsonrpc_message(
        &mut self,
        identity: Identity,
        message: &MessageView,
        content_frame: &[u8],
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        // First, try to parse as a JSON value to determine if it's a batch
        let json_value: serde_json::Value = match serde_json::from_slice(content_frame) {
            Ok(value) => value,
            Err(_) => {
                match message.sender() {
                    Ok(sender_name) => {
                        let error_message = self.create_error_response(
                            sender_name,
                            Id::Null,
                            &Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)),
                            Some(message.header().conversation_id.clone()),
                        )?;
                        return Ok(vec![JsonRpcOutcome::Response(error_message)]);
                    }
                    Err(e) => {
                        eprintln!("Error: Malformed sender name in message, cannot send error response: {:?}", e);
                        return Ok(vec![]);
                    }
                }
            }
        };

        // Check if it's a batch request (array) or single request (object)
        match json_value {
            serde_json::Value::Array(requests) => {
                self.handle_batch_request(identity, message, requests)
            }
            serde_json::Value::Object(_) => {
                // Handle single request by converting back to bytes and parsing as Request
                let request_bytes = serde_json::to_vec(&json_value)?;
                match serde_json::from_slice::<Request>(&request_bytes) {
                    Ok(request) => self.handle_request(identity, message, request),
                    Err(_) => match message.sender() {
                        Ok(sender_name) => {
                            let error_message = self.create_error_response(
                                sender_name,
                                Id::Null,
                                &Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)),
                                Some(message.header().conversation_id.clone()),
                            )?;
                            Ok(vec![JsonRpcOutcome::Response(error_message)])
                        }
                        Err(e) => {
                            eprintln!("Error: Malformed sender name in message, cannot send error response: {:?}", e);
                            Ok(vec![])
                        }
                    },
                }
            }
            _ => {
                // Invalid JSON-RPC message
                match message.sender() {
                    Ok(sender_name) => {
                        let error_message = self.create_error_response(
                            sender_name,
                            Id::Null,
                            &Error::JsonRpc(ErrorObject::from(ErrorCode::InvalidRequest)),
                            Some(message.header().conversation_id.clone()),
                        )?;
                        Ok(vec![JsonRpcOutcome::Response(error_message)])
                    }
                    Err(e) => {
                        eprintln!("Error: Malformed sender name in message, cannot send error response: {:?}", e);
                        return Ok(vec![]);
                    }
                }
            }
        }
    }

    /// Handle a batch of JSON-RPC requests
    fn handle_batch_request(
        &mut self,
        identity: Identity,
        message: &MessageView,
        requests: Vec<serde_json::Value>,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        if requests.is_empty() {
            // Per JSON-RPC 2.0 spec, an empty batch is an error
            match message.sender() {
                Ok(sender_name) => {
                    let error_message = self.create_error_response(
                        sender_name,
                        Id::Null,
                        &Error::JsonRpc(ErrorObject::from(ErrorCode::InvalidRequest)),
                        Some(message.header().conversation_id.clone()),
                    )?;
                    return Ok(vec![JsonRpcOutcome::Response(error_message)]);
                }
                Err(e) => {
                    eprintln!(
                        "Error: Malformed sender name in message, cannot send error response: {:?}",
                        e
                    );
                    return Ok(vec![]);
                }
            }
        }

        let sender = self.extract_sender(message)?;
        let mut responses = Vec::new();
        let mut all_outcomes = Vec::new(); // Collect all outcomes

        // Process each request in the batch
        for request_value in requests {
            match request_value {
                serde_json::Value::Object(_) => {
                    // Convert the JSON value back to bytes and then parse as Request
                    let request_bytes = serde_json::to_vec(&request_value)?;
                    match serde_json::from_slice::<Request>(&request_bytes) {
                        Ok(request) => {
                            // Clone identity for each request since handle_request takes ownership
                            let outcomes =
                                self.handle_request(identity.clone(), message, request)?;

                            for outcome in outcomes {
                                match outcome {
                                    JsonRpcOutcome::Response(response_message) => {
                                        // Combine the responses from all requests
                                        if let Some(content_frame) =
                                            response_message.content_frame()
                                        {
                                            if let Ok(response_value) =
                                                serde_json::from_slice::<serde_json::Value>(
                                                    content_frame,
                                                )
                                            {
                                                responses.push(response_value);
                                            }
                                        }
                                    }
                                    JsonRpcOutcome::NoAction => {
                                        // No action at all
                                    }
                                    _ => {
                                        all_outcomes.push(outcome);
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            // Invalid request in batch - create an error response for it
                            let error_response = Response::<()>::new(
                                ResponsePayload::Error(ErrorObject::from(
                                    ErrorCode::InvalidRequest,
                                )),
                                Id::Null,
                            );
                            if let Ok(error_value) = serde_json::to_value(&error_response) {
                                responses.push(error_value);
                            }
                        }
                    }
                }
                _ => {
                    // Non-object in batch - create an error response for it
                    let error_response = Response::<()>::new(
                        ResponsePayload::Error(ErrorObject::from(ErrorCode::InvalidRequest)),
                        Id::Null,
                    );
                    if let Ok(error_value) = serde_json::to_value(&error_response) {
                        responses.push(error_value);
                    }
                }
            }
        }

        // If we have responses, create a batch response message
        if !responses.is_empty() {
            let batch_response = serde_json::to_vec(&responses)?;

            let response_message = MessageBuilder::new()
                .receiver(sender.clone())
                .sender(self.name.clone())
                .conversation_id(message.header().conversation_id.clone())
                .message_type(MessageType::Json.into())
                .payload_single(batch_response)
                .build()?;

            all_outcomes.insert(0, JsonRpcOutcome::Response(response_message.to_view()?));
        } else if all_outcomes.is_empty() {
            // No responses and no other outcomes means all were notifications
            all_outcomes.push(JsonRpcOutcome::NoAction);
        }

        Ok(all_outcomes)
    }

    /// Handle a JSON-RPC request.
    pub fn handle_request(
        &mut self,
        identity: Identity,
        message: &MessageView,
        request: Request,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        match request.method_name() {
            // Component methods
            "pong" => self.handle_pong(message, request.id()),
            // Extended component
            "shut_down" => self.handle_shut_down(message, request.id()),
            // Coordinator methods
            "sign_in" => self.handle_sign_in(identity, message, request.id()),
            "sign_out" => self.handle_sign_out(identity, message, request.id()),
            "coordinator_sign_in" => {
                self.handle_coordinator_sign_in(identity, message, request.id())
            }
            "coordinator_sign_out" => {
                self.handle_coordinator_sign_out(identity, message, request.id())
            }
            "add_nodes" => self.handle_add_nodes(message, request.id(), request.params()),
            "send_nodes" => self.handle_send_nodes(message, request.id()),
            "record_components" => self.handle_record_components(message, request.id()),
            "send_local_components" => self.handle_send_local_components(message, request.id()),
            "send_global_components" => self.handle_send_global_components(message, request.id()),
            "remove_expired_addresses" => {
                self.handle_remove_expired_addresses(message, request.id(), request.params())
            }
            _ => {
                let sender = self.extract_sender(message)?;
                let error_message = self.create_error_response(
                    sender,
                    request.id(),
                    &Error::JsonRpc(ErrorObject::from(ErrorCode::MethodNotFound)),
                    Some(message.header().conversation_id.clone()),
                )?;
                Ok(vec![JsonRpcOutcome::Response(error_message)])
            }
        }
    }

    /// Extract sender from message with standard error handling
    fn extract_sender<'b>(&self, message: &'b MessageView) -> Result<&'b FullName, Error> {
        message.try_sender(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))
    }

    /// Create a standard null JSON-RPC response
    fn create_null_response(
        &self,
        message: &MessageView,
        id: Id,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;
        self.create_json_response(
            sender,
            id,
            serde_json::Value::Null,
            Some(message.header().conversation_id.clone()),
        )
    }

    /// Create an error response message
    pub fn create_error_response(
        &self,
        recipient_name: &FullName,
        id: Id,
        error: &Error,
        conversation_id: Option<ConversationId>,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let error_object: ErrorObject<'static> = match error {
            Error::Leco(leco_error) => leco_error.clone().into(),
            Error::JsonRpc(json_rpc_error) => json_rpc_error.clone(),
            Error::Custom(code, message) => ErrorObject::owned(*code, message.clone(), None::<()>),
        };

        let error_response = Response::<()>::new(ResponsePayload::Error(error_object), id);
        let error_msg = serde_json::to_vec(&error_response)?;

        let message = MessageBuilder::new()
            .receiver(recipient_name.clone())
            .sender(self.name.clone()) // Use self.name
            .conversation_id(conversation_id.unwrap_or_default())
            .message_type(MessageType::Json.into())
            .payload_single(error_msg)
            .build()?;

        Ok(message.to_view()?)
    }

    /// Create a JSON-RPC response message
    pub fn create_json_response(
        &self,
        recipient_name: &FullName,
        id: Id,
        result: Value,
        conversation_id: Option<ConversationId>,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let response = Response::new(ResponsePayload::Success(Cow::Borrowed(&result)), id);
        let response_msg = serde_json::to_vec(&response)?;

        let message = MessageBuilder::new()
            .receiver(recipient_name.clone())
            .sender(self.name.clone()) // Use self.name
            .conversation_id(conversation_id.unwrap_or_default())
            .message_type(MessageType::Json.into())
            .payload_single(response_msg)
            .build()?;

        Ok(message.to_view()?)
    }

    /// Create a null JSON-RPC response outcome
    fn create_null_response_outcome(
        &self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        if id == Id::Null {
            return Ok(Vec::<JsonRpcOutcome>::new());
        } else {
            let response_message = self.create_null_response(message, id)?;
            Ok(vec![JsonRpcOutcome::Response(response_message)])
        }
    }

    // Individual method handlers
    fn handle_pong(
        &self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        self.create_null_response_outcome(message, id)
    }

    fn handle_sign_in(
        &mut self,
        identity: Identity,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;
        let identity_bytes = match &identity {
            Identity::Component { identity } | Identity::Coordinator { identity } => identity,
            Identity::SelfTarget => return self.create_null_response_outcome(message, id),
        };
        if let Err(e) = self.core.handle_sign_in(sender.clone(), identity_bytes) {
            let error_message = self.create_error_response(
                sender,
                id,
                &e,
                Some(message.header().conversation_id.clone()),
            )?;
            return Ok(vec![JsonRpcOutcome::Response(error_message)]);
        }

        let mut outcomes = self.create_null_response_outcome(message, id)?;

        if self.core.has_remote_coordinators() {
            let sync_messages = self.core.create_directory_sync_messages()?;
            if !sync_messages.is_empty() {
                outcomes.push(JsonRpcOutcome::DirectorySync(sync_messages));
            }
        }

        Ok(outcomes)
    }

    fn handle_sign_out(
        &mut self,
        identity: Identity,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;
        let identity_bytes = match &identity {
            Identity::Component { identity } | Identity::Coordinator { identity } => identity,
            Identity::SelfTarget => return self.create_null_response_outcome(message, id),
        };
        if let Err(e) = self.core.handle_sign_out(identity_bytes, sender.clone()) {
            let error_message = self.create_error_response(
                sender,
                id,
                &e,
                Some(message.header().conversation_id.clone()),
            )?;
            return Ok(vec![JsonRpcOutcome::Response(error_message)]);
        }

        let mut outcomes = self.create_null_response_outcome(message, id)?;

        if self.core.has_remote_coordinators() {
            let sync_messages = self.core.create_directory_sync_messages()?;
            if !sync_messages.is_empty() {
                outcomes.push(JsonRpcOutcome::DirectorySync(sync_messages));
            }
        }

        Ok(outcomes)
    }

    /// Handle coordinator sign-in
    fn handle_coordinator_sign_in(
        &mut self,
        identity: Identity,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        // Extract sender (should be the coordinator signing in)
        let sender = self.extract_sender(message)?;

        // For coordinator sign-in, we expect the sender to be in format "namespace.COORDINATOR"
        if sender.name() != b"COORDINATOR" {
            let error_message = self.create_error_response(
                sender,
                id,
                &Error::JsonRpc(ErrorObject::from(ErrorCode::InvalidParams)),
                Some(message.header().conversation_id.clone()),
            )?;

            return Ok(vec![JsonRpcOutcome::ResponseToIdentity((
                identity,
                error_message,
            ))]);
        };

        // Extract dealer identity from the Identity parameter
        let dealer_identity = match &identity {
            Identity::Coordinator { identity } | Identity::Component { identity } => {
                identity.clone()
            }
            Identity::SelfTarget => {
                let error_message = self.create_error_response(
                    sender,
                    id,
                    &Error::JsonRpc(ErrorObject::from(ErrorCode::InvalidParams)),
                    Some(message.header().conversation_id.clone()),
                )?;

                return Ok(vec![JsonRpcOutcome::ResponseToIdentity((
                    identity,
                    error_message,
                ))]);
            }
        };

        // Create CoordinatorEntry
        let coordinator_entry = CoordinatorEntry {
            namespace: sender.namespace().to_vec(),
            dealer_identity,
            address: String::new(),
            last_seen: self.core.clock().now(),
        };

        // Register the coordinator (will detect duplicates)
        if let Err(e) = self.core.register_coordinator(coordinator_entry) {
            let error_message = self.create_error_response(
                sender,
                id,
                &e,
                Some(message.header().conversation_id.clone()),
            )?;

            return Ok(vec![JsonRpcOutcome::ResponseToIdentity((
                identity,
                error_message,
            ))]);
        }

        // Create success response
        let response_message = self.create_null_response(message, id)?;

        Ok(vec![JsonRpcOutcome::ResponseToIdentity((
            identity,
            response_message,
        ))])
    }

    fn handle_coordinator_sign_out(
        &mut self,
        identity: Identity,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;
        let sender_namespace = sender.namespace();

        if !self.core.is_coordinator_registered(sender_namespace) {
            return Ok(vec![]);
        }

        let dealer_identity = match &identity {
            Identity::Coordinator { identity } | Identity::Component { identity } => {
                identity.clone()
            }
            Identity::SelfTarget => return Ok(vec![]),
        };

        let stored_identity = self
            .core
            .get_coordinator_dealer_identity(sender_namespace)?;
        if stored_identity != dealer_identity {
            let error_message = self.create_error_response(
                sender,
                id,
                &Error::duplicate_name_with_data(serde_json::Value::String(
                    String::from_utf8_lossy(sender_namespace).to_string(),
                )),
                Some(message.header().conversation_id.clone()),
            )?;
            return Ok(vec![JsonRpcOutcome::ResponseToIdentity((
                identity,
                error_message,
            ))]);
        }

        self.core.remove_coordinator(sender_namespace)?;
        if let Err(err) = self.core.remove_remote_components(sender_namespace) {
            eprintln!("Warning: Failed to remove remote components: {}", err);
        }

        self.create_null_response_outcome(message, id)
    }

    fn handle_add_nodes(
        &mut self,
        message: &MessageView,
        id: Id,
        params: Params,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let nodes = params.parse::<AddNodesParams>()?;
        let addresses = self.core.handle_add_nodes(nodes);
        let mut outcomes = self.create_null_response_outcome(message, id)?;
        match addresses {
            Some(addresses) => outcomes.push(JsonRpcOutcome::AddNodes(addresses)),
            None => (),
        };
        Ok(outcomes)
    }

    fn handle_send_nodes(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;
        let nodes_map = serde_json::Map::from_iter(
            self.core
                .get_all_coordinators_including_self()
                .into_iter()
                .map(|(k, v)| (k, serde_json::Value::String(v))),
        );
        let response = self.create_json_response(
            &sender,
            id,
            serde_json::Value::Object(nodes_map),
            Some(message.header().conversation_id.clone()),
        )?;
        Ok(vec![JsonRpcOutcome::Response(response)])
    }

    fn handle_record_components(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        use crate::core::parameter_types::RecordComponentsParams;

        let sender = self.extract_sender(message)?;
        let sender_namespace = sender.namespace().to_vec();

        if let Some(content_frame) = message.content_frame() {
            if let Ok(request) = serde_json::from_slice::<Request>(content_frame) {
                if let Ok(params) = request.params().parse::<RecordComponentsParams>() {
                    let components: Vec<FullName> = params
                        .components
                        .into_iter()
                        .filter_map(|s| FullName::from_str(&s).ok())
                        .collect();
                    self.core
                        .directory_mut()
                        .add_remote_components(sender_namespace, components)?;
                }
            }
        }

        self.create_null_response_outcome(message, id)
    }

    fn handle_send_local_components(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;

        let components = self.core.get_local_components();

        let response_message = self.create_json_response(
            sender,
            id,
            serde_json::to_value(components)?,
            Some(message.header().conversation_id.clone()),
        )?;

        Ok(vec![JsonRpcOutcome::Response(response_message)])
    }

    fn handle_send_global_components(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let sender = self.extract_sender(message)?;

        let mut result_map = serde_json::Map::new();

        // Add local components under our namespace
        let local_components: Vec<String> = self.core.get_local_components();
        let namespace_str = String::from_utf8_lossy(&self.core.namespace()).to_string();
        result_map.insert(
            namespace_str.clone(),
            serde_json::Value::Array(
                local_components
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );

        // Add remote components from other namespaces
        if let Ok(global_components) = self.core.directory_mut().get_all_global_components() {
            for (ns, components) in global_components {
                let ns_str = String::from_utf8_lossy(&ns).to_string();
                if ns_str != namespace_str {
                    let component_strings: Vec<serde_json::Value> = components
                        .into_iter()
                        .map(|c| serde_json::Value::String(c.to_string()))
                        .collect();
                    result_map.insert(ns_str, serde_json::Value::Array(component_strings));
                }
            }
        }

        let response = self.create_json_response(
            &sender,
            id,
            serde_json::Value::Object(result_map),
            Some(message.header().conversation_id.clone()),
        )?;
        Ok(vec![JsonRpcOutcome::Response(response)])
    }

    fn handle_remove_expired_addresses(
        &mut self,
        message: &MessageView,
        id: Id,
        params: Params,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let parsed_params = params.parse::<RemoveExpiredAddressesParams>()?;
        let expiration_duration = Duration::from_secs_f64(parsed_params.expiration_time);

        let timed_out = self.core.check_timeouts(expiration_duration);
        self.core.remove_timed_out_components(&timed_out)?;

        self.create_null_response_outcome(message, id)
    }

    fn handle_shut_down(
        &mut self,
        message: &MessageView,
        id: Id,
    ) -> Result<Vec<JsonRpcOutcome>, Box<dyn std::error::Error>> {
        let response_message = self.create_null_response(message, id)?;

        Ok(vec![
            JsonRpcOutcome::Response(response_message),
            JsonRpcOutcome::Shutdown,
        ])
    }
}

#[cfg(test)]
mod tests {
    use crate::core::parameter_types::AddNodesParams;
    use jsonrpsee_types::request::Request;
    use ruleco_core::{
        full_name::FullName, message::MessageBuilder, protocol_constants::MessageType,
    };

    #[test]
    fn test_handle_add_node() {
        let request_json = r#"{"jsonrpc": "2.0", "method": "add_nodes", "params": {"nodes": {"N1": "N1host:12300", "N2": "wrong_host:-7", "N3": "N3host:12300"}}, "id": 2}"#;
        let message = MessageBuilder::new()
            .receiver(FullName::from_slice(b"test_ns.COORDINATOR").unwrap())
            .sender(FullName::from_slice(b"some_sender").unwrap())
            .message_type(MessageType::Json.into())
            .payload_single(request_json.as_bytes().to_vec())
            .build()
            .unwrap()
            .to_view()
            .unwrap();

        let content_frame = message.content_frame().unwrap();
        let request: Request = serde_json::from_slice(content_frame).unwrap();
        let params = request.params();
        let add_nodes_params = params.parse::<AddNodesParams>().unwrap();

        assert_eq!(add_nodes_params.nodes.len(), 3);
        assert_eq!(
            add_nodes_params.nodes.get("N1"),
            Some(&"N1host:12300".to_string())
        );
    }
}
