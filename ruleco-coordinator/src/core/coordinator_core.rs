use crate::core::domain::{ComponentEntry, CoordinatorEntry, RoutingError, RoutingResult};
use crate::core::parameter_types::AddNodesParams;
use crate::core::ports::message_port::Identity;
use crate::core::ports::{ClockPort, DirectoryPort, RoutingPort};
use jsonrpsee_types::{ErrorCode, ErrorObject, Request};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use ruleco_core::message::MessageView;
use serde_json::Value;

/// The main coordinator core that implements the domain logic
pub struct CoordinatorCore<D: DirectoryPort, C: ClockPort> {
    /// Our namespace
    namespace: Vec<u8>,
    /// Directory port for managing components and coordinators
    directory: D,
    /// Clock port for time-related operations
    clock: C,
}

impl<D: DirectoryPort, C: ClockPort> CoordinatorCore<D, C> {
    /// Create a new coordinator core
    pub fn new(namespace: Vec<u8>, directory: D, clock: C) -> Self {
        Self {
            namespace,
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
    pub fn handle_coordinator_sign_in_success(
        &mut self,
        dealer_identity: &[u8],
        remote_coordinator_name: FullName,
        address: String,
    ) -> Result<(), Error> {
        let coordinator_entry = CoordinatorEntry {
            namespace: remote_coordinator_name.namespace().to_vec(),
            dealer_identity: dealer_identity.to_vec(),
            address,
        };

        self.directory.register_coordinator(coordinator_entry)
    }

    /// Remove a coordinator from our network view
    pub fn remove_coordinator(
        &mut self,
        namespace: &[u8],
    ) -> Result<Option<CoordinatorEntry>, Error> {
        self.directory.deregister_coordinator(namespace)
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

    /// Get a reference to the namespace (for testing)
    #[cfg(test)]
    pub fn namespace(&self) -> &[u8] {
        &self.namespace
    }

    /// Get a reference to the directory (for testing)
    #[cfg(test)]
    pub fn directory(&self) -> &D {
        &self.directory
    }

    /// Get a mutable reference to the directory (for testing)
    #[cfg(test)]
    pub fn directory_mut(&mut self) -> &mut D {
        &mut self.directory
    }

    pub fn clock(&self) -> &C {
        &self.clock
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
                    Some(message.header().conversation_id.clone()).unwrap_or_default(),
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
                    Some(message.header().conversation_id.clone()).unwrap_or_default(),
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
        if is_from_local_socket && is_from_local_namespace && !self.is_sign_in_message(&message) {
            if let Ok(stored_identity) = self.directory.get_component_identity(&sender) {
                if stored_identity.as_slice() != sender_identity_bytes {
                    return Err(RoutingError::new(
                        Error::duplicate_name_with_data(serde_json::Value::String(
                            sender.to_string(),
                        )),
                        Some(message.header().conversation_id.clone()).unwrap_or_default(),
                    ));
                }
            } else {
                return Err(RoutingError::new(
                    Error::not_signed_in(),
                    Some(message.header().conversation_id.clone()).unwrap_or_default(),
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
                Some(message.header().conversation_id.clone()).unwrap_or_default(),
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

        Err(RoutingError::new(
            Error::node_unknown_with_data(receiver.namespace().into()),
            Some(message.header().conversation_id.clone()).unwrap_or_default(),
        ))
    }
}
