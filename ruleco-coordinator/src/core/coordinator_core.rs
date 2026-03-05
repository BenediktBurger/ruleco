use crate::core::domain::{ComponentEntry, CoordinatorEntry, RoutingError, RoutingResult};
use crate::core::parameter_types::{AddNodesParams, RecordComponentsParams};
use crate::core::ports::message_port::Identity;
use crate::core::ports::{ClockPort, DirectoryPort, RoutingPort};
use jsonrpsee_types::{ErrorCode, ErrorObject, Request};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use ruleco_core::message::{MessageBuilder, MessageView};
use serde_json::Value;

/// The main coordinator core that implements the domain logic
pub struct CoordinatorCore<D: DirectoryPort, C: ClockPort> {
    /// Our namespace
    namespace: Vec<u8>,
    /// Our address (e.g., "tcp://127.0.0.1:12300")
    address: String,
    /// Directory port for managing components and coordinators
    directory: D,
    /// Clock port for time-related operations
    clock: C,
}

impl<D: DirectoryPort, C: ClockPort> CoordinatorCore<D, C> {
    /// Create a new coordinator core
    pub fn new(namespace: Vec<u8>, address: String, directory: D, clock: C) -> Self {
        Self {
            namespace,
            address,
            directory,
            clock,
        }
    }

    /// Handle a component signing in
    pub fn handle_sign_in(
        &mut self,
        component_name: FullName,
        identity: &[u8],
    ) -> Result<(), Error> {
        let local_identity = identity.to_vec();

        let full_name = if component_name.has_namespace() {
            component_name
        } else {
            FullName::new(self.namespace.clone(), component_name.name().to_vec())
        };

        self.directory
            .register_component(full_name, &local_identity)
    }

    /// Handle a component signing out
    pub fn handle_sign_out(
        &mut self,
        identity: &[u8],
        component_name: FullName,
    ) -> Result<Option<ComponentEntry>, Error> {
        let stored_identity = self.directory.get_component_identity(&component_name)?;
        if &stored_identity != identity {
            return Err(Error::from(ErrorCode::InvalidParams));
        }
        self.directory.deregister_component(component_name)
    }

    /// Handle add_nodes request - return addresses of unknown coordinators
    pub fn handle_add_nodes(&self, nodes: AddNodesParams) -> Option<Vec<std::string::String>> {
        let mut addresses = Vec::<String>::new();
        for (ns, address) in nodes.nodes {
            if ns.as_bytes() == &self.namespace[..] {
                continue;
            }
            let stored_d = self.directory.get_coordinator(ns.as_bytes());
            match stored_d {
                None => {
                    addresses.push(address);
                }
                Some(_) => (),
            }
        }
        if addresses.is_empty() {
            None
        } else {
            Some(addresses)
        }
    }

    /// Get all coordinators including self as (namespace, address) pairs
    pub fn get_all_coordinators_including_self(&self) -> Vec<(String, String)> {
        let mut result = vec![(
            String::from_utf8_lossy(&self.namespace).to_string(),
            self.address.clone(),
        )];
        for coord in self.directory.get_all_coordinators() {
            result.push((
                String::from_utf8_lossy(&coord.namespace).to_string(),
                coord.address.clone(),
            ));
        }
        result
    }

    /// Check for timed out components
    ///
    /// This is domain logic: determining which components haven't been seen
    /// recently. The Core identifies them; the App handles the cleanup.
    pub fn check_timeouts(&mut self, timeout_duration: std::time::Duration) -> Vec<FullName> {
        let now = self.clock.now();
        let mut timed_out_components = Vec::new();

        for component in self.directory.get_all_local_components() {
            if now.duration_since(component.last_seen) > timeout_duration {
                timed_out_components.push(component.name.clone());
            }
        }

        timed_out_components
    }

    /// Remove timed out components from the directory
    pub fn remove_timed_out_components(&mut self, names: &[FullName]) -> Result<(), Error> {
        for name in names {
            let _ = self.directory.deregister_component(name.clone());
        }
        Ok(())
    }

    /// Check if a message contains an error response
    pub fn is_error_response(&self, message: &MessageView) -> bool {
        if let Some(content_frame) = message.content_frame() {
            if let Ok(response_value) = serde_json::from_slice::<Value>(content_frame) {
                return response_value.get("error").is_some();
            }
        }
        false
    }

    /// Handle a successful response to our coordinator sign-in request
    ///
    /// This is called by the App after the remote coordinator accepts our sign_in.
    /// The App provides the address, as it's tracked at the app layer for connection
    /// management (ZMQ-specific concern).
    /// Returns the registered CoordinatorEntry and messages to send (add_nodes + record_components).
    pub fn handle_coordinator_sign_in_success(
        &mut self,
        dealer_identity: &[u8],
        remote_coordinator_name: FullName,
        address: String,
    ) -> Result<(CoordinatorEntry, Vec<MessageView>), Error> {
        let coordinator_entry = CoordinatorEntry {
            namespace: remote_coordinator_name.namespace().to_vec(),
            dealer_identity: dealer_identity.to_vec(),
            address: address.clone(),
            last_seen: self.clock.now(),
        };

        self.directory
            .register_coordinator(coordinator_entry.clone())?;

        let add_nodes_params = AddNodesParams {
            nodes: self
                .get_all_coordinators_including_self()
                .into_iter()
                .collect(),
        };
        let add_nodes_message = MessageBuilder::new()
            .sender(FullName::new(
                self.namespace.clone(),
                b"COORDINATOR".to_vec(),
            ))
            .receiver(remote_coordinator_name.clone())
            .payload_json(&Request::borrowed(
                "add_nodes",
                Some(&serde_json::value::to_raw_value(&add_nodes_params).unwrap()),
                jsonrpsee_types::Id::Number(1),
            ))
            .map_err(|e| Error::custom(-1, format!("Failed to create add_nodes message: {}", e)))?
            .build()
            .map_err(|e| Error::custom(-1, format!("Failed to build message: {}", e)))?
            .to_view()
            .map_err(|e| Error::custom(-1, format!("Failed to create view: {}", e)))?;

        let record_components_message =
            self.create_record_components_message(remote_coordinator_name)?;

        Ok((
            coordinator_entry,
            vec![add_nodes_message, record_components_message],
        ))
    }

    /// Create a record_components message for a remote coordinator
    pub fn create_record_components_message(
        &self,
        remote_coordinator_name: FullName,
    ) -> Result<MessageView, Error> {
        let components: Vec<String> = self
            .directory
            .get_all_local_components()
            .iter()
            .map(|entry| entry.name.to_string())
            .collect();

        let params = RecordComponentsParams { components };

        let message = MessageBuilder::new()
            .sender(FullName::new(
                self.namespace.clone(),
                b"COORDINATOR".to_vec(),
            ))
            .receiver(remote_coordinator_name)
            .payload_json(&Request::borrowed(
                "record_components",
                Some(&serde_json::value::to_raw_value(&params).unwrap()),
                jsonrpsee_types::Id::Number(2),
            ))
            .map_err(|e| {
                Error::custom(
                    -1,
                    format!("Failed to create record_components message: {}", e),
                )
            })?
            .build()
            .map_err(|e| Error::custom(-1, format!("Failed to build message: {}", e)))?
            .to_view()
            .map_err(|e| Error::custom(-1, format!("Failed to create view: {}", e)))?;

        Ok(message)
    }

    /// Remove a coordinator from our network view
    pub fn remove_coordinator(
        &mut self,
        namespace: &[u8],
    ) -> Result<Option<CoordinatorEntry>, Error> {
        self.directory.deregister_coordinator(namespace)
    }

    /// Update last_seen timestamp for a component (heartbeat tracking)
    pub fn update_component_last_seen(&mut self, name: &FullName) -> Result<(), Error> {
        let now = self.clock.now();
        self.directory.update_component_last_seen(name.clone(), now)
    }

    /// Register a coordinator in the directory
    pub fn register_coordinator(&mut self, coordinator: CoordinatorEntry) -> Result<(), Error> {
        self.directory.register_coordinator(coordinator)
    }

    /// Helper to check if a message is a sign_in request
    fn is_sign_in_message(&self, message: &MessageView) -> bool {
        if let Some(content_frame) = message.content_frame() {
            if let Ok(request) = serde_json::from_slice::<Request>(content_frame) {
                return request.method_name() == "sign_in";
            }
        }
        false
    }

    /// Helper to check if a message is a coordinator_sign_in request
    pub fn is_coordinator_sign_in(&self, message: &MessageView) -> bool {
        if let Some(content_frame) = message.content_frame() {
            if let Ok(request) = serde_json::from_slice::<Request>(content_frame) {
                return request.method_name() == "coordinator_sign_in";
            }
        }
        false
    }

    /// Helper to check if a message is a coordinator_sign_out request
    pub fn is_coordinator_sign_out(&self, message: &MessageView) -> bool {
        if let Some(content_frame) = message.content_frame() {
            if let Ok(request) = serde_json::from_slice::<Request>(content_frame) {
                return request.method_name() == "coordinator_sign_out";
            }
        }
        false
    }

    /// Get the coordinator's namespace
    pub fn namespace(&self) -> &[u8] {
        &self.namespace
    }

    /// Get a reference to the namespace (for testing)
    #[cfg(test)]
    pub fn namespace_test(&self) -> &[u8] {
        &self.namespace
    }

    /// Get a reference to the directory (for testing)
    #[cfg(test)]
    pub fn directory(&self) -> &D {
        &self.directory
    }

    /// Get a reference to the directory
    pub fn directory_ref(&self) -> &D {
        &self.directory
    }

    /// Get a mutable reference to the directory
    pub fn directory_mut(&mut self) -> &mut D {
        &mut self.directory
    }

    pub fn clock(&self) -> &C {
        &self.clock
    }

    /// Get the coordinator's address
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Get all local components
    pub fn get_local_components(&self) -> Vec<String> {
        self.directory
            .get_all_local_components()
            .iter()
            .map(|entry| entry.name.to_string())
            .collect()
    }

    /// Check if there are any registered remote coordinators
    pub fn has_remote_coordinators(&self) -> bool {
        self.directory.get_all_coordinators().len() > 0
    }

    /// Get all registered coordinator namespaces (not including self)
    pub fn get_remote_coordinator_namespaces(&self) -> Vec<Vec<u8>> {
        self.directory
            .get_all_coordinators()
            .iter()
            .map(|coord| coord.namespace.clone())
            .collect()
    }

    /// Create record_components messages for all known remote coordinators
    pub fn create_directory_sync_messages(&self) -> Result<Vec<MessageView>, Error> {
        let mut messages = Vec::new();
        let remote_coordinators = self.directory.get_all_coordinators();

        for coordinator in remote_coordinators {
            let remote_name = FullName::new(coordinator.namespace.clone(), b"COORDINATOR".to_vec());
            match self.create_record_components_message(remote_name) {
                Ok(msg) => messages.push(msg),
                Err(e) => eprintln!("Warning: Failed to create directory sync message: {}", e),
            }
        }

        Ok(messages)
    }

    /// Check if a coordinator namespace is registered
    pub fn is_coordinator_registered(&self, namespace: &[u8]) -> bool {
        self.directory.is_coordinator_registered(namespace)
    }

    /// Get the dealer identity for a registered coordinator
    pub fn get_coordinator_dealer_identity(&self, namespace: &[u8]) -> Result<Vec<u8>, Error> {
        self.directory.get_coordinator_dealer_identity(namespace)
    }

    /// Remove remote components for a namespace
    pub fn remove_remote_components(&mut self, namespace: &[u8]) -> Result<(), Error> {
        self.directory.remove_remote_components(namespace)
    }

    /// Check for timed out coordinators
    pub fn check_coordinator_timeouts(
        &mut self,
        timeout_duration: std::time::Duration,
    ) -> Vec<Vec<u8>> {
        let now = self.clock.now();
        let mut timed_out = Vec::new();

        for coordinator in self.directory.get_all_coordinators_mut() {
            if now.duration_since(coordinator.last_seen) > timeout_duration {
                timed_out.push(coordinator.namespace.clone());
            }
        }

        timed_out
    }

    /// Remove timed out coordinators and their components
    pub fn remove_timed_out_coordinators(&mut self, namespaces: &[Vec<u8>]) -> Result<(), Error> {
        for ns in namespaces {
            let _ = self.directory.deregister_coordinator(ns);
            let _ = self.directory.remove_remote_components(ns);
        }
        Ok(())
    }

    /// Update last_seen timestamp for a coordinator (heartbeat tracking)
    pub fn update_coordinator_last_seen(&mut self, namespace: &[u8]) -> Result<(), Error> {
        let now = self.clock.now();
        self.directory.update_coordinator_last_seen(namespace, now)
    }
}

impl<D: DirectoryPort, C: ClockPort> RoutingPort<D, C> for CoordinatorCore<D, C> {
    /// Route a message based on its destination and the identity context of its source
    fn route_message(&self, message: &MessageView, sender_identity: &Identity) -> RoutingResult {
        let sender = match message
            .try_sender(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))
        {
            Ok(sender) => sender,
            Err(error) => {
                return Err(RoutingError::new(
                    error,
                    message.header().conversation_id.clone(),
                ))
            }
        };

        let receiver = match message
            .try_receiver(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))
        {
            Ok(receiver) => receiver,
            Err(error) => {
                return Err(RoutingError::new(
                    error,
                    message.header().conversation_id.clone(),
                ))
            }
        };

        let sender_identity_bytes = match sender_identity {
            Identity::Component { identity } | Identity::Coordinator { identity } => {
                identity.as_slice()
            }
            Identity::SelfTarget => return Ok(Identity::SelfTarget),
        };

        let is_from_local_socket = matches!(sender_identity, Identity::Component { .. });
        let _is_from_remote_socket = matches!(sender_identity, Identity::Coordinator { .. });
        let is_from_local_namespace =
            sender.namespace().is_empty() || sender.namespace() == &self.namespace[..];

        // Step 1: Validate sender registration ONLY for Local socket messages from local namespace
        // Remote coordinators (DEALER) are already authenticated and don't need validation
        // Also bypass for sign_in, coordinator_sign_in, and coordinator_sign_out messages (registration is being established/removed)
        if is_from_local_socket
            && is_from_local_namespace
            && !self.is_sign_in_message(&message)
            && !self.is_coordinator_sign_in(&message)
            && !self.is_coordinator_sign_out(&message)
        {
            if let Ok(stored_identity) = self.directory.get_component_identity(&sender) {
                if stored_identity.as_slice() != sender_identity_bytes {
                    return Err(RoutingError::new(
                        Error::duplicate_name_with_data(serde_json::Value::String(
                            sender.to_string(),
                        )),
                        message.header().conversation_id.clone(),
                    ));
                }
            } else {
                return Err(RoutingError::new(
                    Error::not_signed_in_with_data(sender.to_string().into()),
                    message.header().conversation_id.clone(),
                ));
            }
        }

        // Step 2: Route based on receiver
        if receiver.name() == b"COORDINATOR"
            && (receiver.namespace() == &self.namespace[..] || receiver.namespace().is_empty())
        {
            return Ok(Identity::SelfTarget);
        }

        if receiver.namespace().is_empty() || receiver.namespace() == &self.namespace[..] {
            if let Ok(identity) = self.directory.get_component_identity(&receiver) {
                return Ok(Identity::Component { identity });
            }
            return Err(RoutingError::new(
                Error::receiver_unknown_with_data(receiver.to_string().into()),
                message.header().conversation_id.clone(),
            ));
        }

        if let Ok(dealer_identity) = self
            .directory
            .get_coordinator_dealer_identity(receiver.namespace())
        {
            return Ok(Identity::Coordinator {
                identity: dealer_identity,
            });
        }

        let namespace_str = std::str::from_utf8(receiver.namespace()).unwrap_or("unknown");
        Err(RoutingError::new(
            Error::node_unknown_with_data(Value::String(namespace_str.to_string())),
            message.header().conversation_id.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::in_memory_directory_adapter::InMemoryDirectoryAdapter;
    use std::time::Instant;

    struct MockClock {
        now: Instant,
    }

    impl ClockPort for MockClock {
        fn now(&self) -> Instant {
            self.now
        }
    }

    fn create_test_core() -> CoordinatorCore<InMemoryDirectoryAdapter, MockClock> {
        let directory = InMemoryDirectoryAdapter::new(b"N1".to_vec());
        let clock = MockClock {
            now: Instant::now(),
        };
        CoordinatorCore::new(
            b"N1".to_vec(),
            "tcp://127.0.0.1:12300".to_string(),
            directory,
            clock,
        )
    }

    #[test]
    fn test_create_record_components_message_empty() {
        let core = create_test_core();
        let remote_coordinator = FullName::new(b"N2".to_vec(), b"COORDINATOR".to_vec());

        let message = core
            .create_record_components_message(remote_coordinator.clone())
            .unwrap();

        assert_eq!(message.sender().as_ref().unwrap().namespace(), b"N1");
        assert_eq!(message.sender().as_ref().unwrap().name(), b"COORDINATOR");
        assert_eq!(message.receiver().as_ref().unwrap().namespace(), b"N2");
        assert_eq!(message.receiver().as_ref().unwrap().name(), b"COORDINATOR");

        let content = message.content_frame().unwrap();
        let request: Request = serde_json::from_slice(content).unwrap();
        assert_eq!(request.method_name(), "record_components");
    }

    #[test]
    fn test_create_record_components_message_with_components() {
        let mut core = create_test_core();

        // Sign in two components
        let comp1 = FullName::new(b"N1".to_vec(), b"ComponentA".to_vec());
        let comp2 = FullName::new(b"N1".to_vec(), b"ComponentB".to_vec());
        core.handle_sign_in(comp1.clone(), b"identity1").unwrap();
        core.handle_sign_in(comp2.clone(), b"identity2").unwrap();

        let remote_coordinator = FullName::new(b"N2".to_vec(), b"COORDINATOR".to_vec());
        let message = core
            .create_record_components_message(remote_coordinator)
            .unwrap();

        let content = message.content_frame().unwrap();
        let request: Request = serde_json::from_slice(content).unwrap();
        assert_eq!(request.method_name(), "record_components");

        let params = request.params();
        let params_value: Value = serde_json::from_str(params.as_str().unwrap()).unwrap();
        let components = params_value["components"].as_array().unwrap();
        assert_eq!(components.len(), 2);

        let component_names: Vec<&str> = components.iter().filter_map(|v| v.as_str()).collect();
        assert!(component_names.contains(&"N1.ComponentA"));
        assert!(component_names.contains(&"N1.ComponentB"));
    }
}
