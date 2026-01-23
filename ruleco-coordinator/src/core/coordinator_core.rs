use crate::core::domain::{ComponentEntry, CoordinatorEntry, RoutingDecision};
use crate::core::parameter_types::AddNodesParams;
use crate::core::pending_connections::PendingConnections;
use crate::core::ports::message_receiver_port::Identity;
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
    /// Track pending connections to remote coordinators
    pending_connections: PendingConnections,
}

impl<D: DirectoryPort, C: ClockPort> CoordinatorCore<D, C> {
    /// Create a new coordinator core
    pub fn new(namespace: Vec<u8>, directory: D, clock: C) -> Self {
        Self {
            namespace,
            directory,
            clock,
            pending_connections: PendingConnections::new(),
        }
    }

    /// Add a pending connection to track
    pub fn add_pending_connection(&mut self, dealer_identity: Vec<u8>, address: String) {
        self.pending_connections.add_pending_connection(dealer_identity, address);
    }

    /// Complete a pending connection and return its information
    pub fn complete_pending_connection(&mut self, dealer_identity: &[u8]) -> Option<crate::core::pending_connections::PendingConnectionInfo> {
        self.pending_connections.complete_connection(dealer_identity)
    }

    /// Get information about a pending connection
    pub fn get_pending_connection(&self, dealer_identity: &[u8]) -> Option<&crate::core::pending_connections::PendingConnectionInfo> {
        self.pending_connections.get_pending_connection(dealer_identity)
    }

    /// Handle a component signing in
    pub fn handle_sign_in(
        &mut self,
        component_name: FullName,
        identity: Identity,
    ) -> Result<(), Error> {
        // If the component name doesn't have a namespace, prepend our namespace to it
        let local_identity = match identity {
            Identity::Local { identity } => Ok(identity),
            _ => Err(Error::from(ErrorCode::InvalidRequest)),
        }?;
        let full_name = if component_name.has_namespace() {
            component_name
        } else {
            FullName::new(self.namespace.clone(), component_name.name().to_vec())
        };

        let component = ComponentEntry {
            name: full_name,
            identity: local_identity,
            last_seen: self.clock.now(),
        };

        self.directory.add_local_component(component)
    }

    /// Handle a component signing out
    pub fn handle_sign_out(
        &mut self,
        identity: Identity,
        component_name: FullName,
    ) -> Result<Option<ComponentEntry>, Error> {
        let stored_component = self.directory.get_local_component(&component_name);
        let local_identity = match identity {
            Identity::Local { identity } => identity,
            _ => return Err(Error::from(ErrorCode::InvalidParams)),
        };
        match stored_component {
            Some(component) => {
                if component.identity == local_identity {
                    self.directory.remove_local_component(component_name)
                } else {
                    return Err(Error::from(ErrorCode::InvalidParams));
                }
            }
            None => return Err(Error::from(ErrorCode::InvalidParams)),
        }
    }

    // Coordinator_sign_in: Just respond with OK
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

    /// Check for timed out components and pending connections
    pub fn check_timeouts(&mut self, timeout_duration: std::time::Duration) -> Vec<FullName> {
        let now = self.clock.now();
        let mut timed_out_components = Vec::new();

        // Collect timed out components
        for component in self.directory.get_all_local_components() {
            if now.duration_since(component.last_seen) > timeout_duration {
                timed_out_components.push(component.name.clone());
            }
        }

        // Remove timed out components
        for name in &timed_out_components {
            let _ = self.directory.remove_local_component(name.clone());
        }

        // Check for timed out pending connections (but don't do anything with them yet)
        let _timed_out_connections = self.pending_connections.check_timeouts(timeout_duration);

        timed_out_components
    }

    /// Check if a message contains an error response
    pub fn is_error_response(&self, message: &MessageView) -> bool {
        if let Some(content_frame) = message.content_frame() {
            if let Ok(response_value) = serde_json::from_slice::<Value>(content_frame) {
                // Check if it's a JSON-RPC response with an error field
                return response_value.get("error").is_some();
            }
        }
        false
    }

    /// Handle a successful response to our coordinator sign-in request
    /// 
    /// This completes the pending connection and adds the coordinator to our directory
    pub fn handle_coordinator_sign_in_success(
        &mut self,
        dealer_identity: &[u8],
        remote_coordinator_name: FullName,
    ) -> Result<(), Error> {
        // Complete the pending connection to get the address
        let pending_info = match self.complete_pending_connection(dealer_identity) {
            Some(info) => info,
            None => {
                // This isn't a pending connection we initiated
                return Err(Error::from(ErrorCode::InvalidParams));
            }
        };

        // Create a coordinator entry
        let coordinator_entry = CoordinatorEntry {
            namespace: remote_coordinator_name.namespace().to_vec(),
            dealer_identity: dealer_identity.to_vec(),
            address: pending_info.address,
        };

        // Add to our directory
        self.directory.add_coordinator(coordinator_entry)
    }

    /// Handle an error response to our coordinator sign-in request
    /// 
    /// This cleans up the pending connection without adding the coordinator to our directory
    pub fn handle_coordinator_sign_in_error(
        &mut self,
        dealer_identity: &[u8],
    ) -> Result<(), Error> {
        // Complete the pending connection to clean it up
        let _pending_info = match self.complete_pending_connection(dealer_identity) {
            Some(info) => info,
            None => {
                // This isn't a pending connection we initiated
                return Err(Error::from(ErrorCode::InvalidParams));
            }
        };

        // No need to add to directory since the connection failed
        Ok(())
    }

    /// Remove a coordinator from our network view
    pub fn remove_coordinator(
        &mut self,
        namespace: &[u8],
    ) -> Result<Option<CoordinatorEntry>, Error> {
        self.directory.remove_coordinator(namespace)
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
}

impl<D: DirectoryPort, C: ClockPort> RoutingPort for CoordinatorCore<D, C> {
    /// Route a message based on its destination and the identity of its source
    fn route_message(&self, message: &MessageView, identity: &Identity) -> RoutingDecision {
        let sender = match message
            .try_sender(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))
        {
            Ok(sender) => sender,
            Err(error) => {
                return RoutingDecision::Error {
                    error,
                    conversation_id: Some(message.header().conversation_id.clone())
                        .unwrap_or_default(),
                }
            }
        };

        let receiver = match message
            .try_receiver(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError)))
        {
            Ok(receiver) => receiver,
            Err(error) => {
                return RoutingDecision::Error {
                    error,
                    conversation_id: Some(message.header().conversation_id.clone())
                        .unwrap_or_default(),
                }
            }
        };

        let is_from_local_namespace =
            sender.namespace().is_empty() || sender.namespace() == &self.namespace[..];

        // Check if sender is signed in (unless it's a sign_in message to coordinator)
        let is_sign_in_to_coordinator =
            (receiver.name() == b"COORDINATOR") && self.is_sign_in_message(&message);

        if is_from_local_namespace && !is_sign_in_to_coordinator {
            match identity {
                Identity::Local { identity } => {
                    // Check if sender is in our local directory with matching identity
                    if let Some(component) = self.directory.get_local_component(&sender) {
                        if component.identity != *identity {
                            return RoutingDecision::Error {
                                error: Error::duplicate_name_with_data(serde_json::Value::String(
                                    sender.to_string(),
                                )),
                                conversation_id: Some(message.header().conversation_id.clone())
                                    .unwrap_or_default(),
                            };
                        }

                        // Note: We don't update the last seen time here because this method is immutable.
                        // Last seen updates should happen elsewhere, possibly in the application layer
                        // after successful routing.
                    } else {
                        return RoutingDecision::Error {
                            error: Error::not_signed_in(),
                            conversation_id: Some(message.header().conversation_id.clone())
                                .unwrap_or_default(),
                        };
                    }
                }
                _ => {}
            }
        }

        // Check if message is addressed to this coordinator
        if receiver.name() == b"COORDINATOR"
            && (receiver.namespace() == &self.namespace[..] || receiver.namespace().is_empty())
        {
            // Special case: If this is a response from a pending connection, handle it specially
            if let Identity::Remote { identity: ref dealer_identity } = identity {
                if self.get_pending_connection(dealer_identity).is_some() {
                    return RoutingDecision::PendingConnectionResponse {
                        dealer_identity: dealer_identity.clone(),
                    };
                }
            }
            
            return RoutingDecision::SelfTarget;
        }

        // Route to local component if namespace matches or is empty
        if receiver.namespace().is_empty() || receiver.namespace() == &self.namespace[..] {
            if let Some(component) = self.directory.get_local_component(&receiver) {
                return RoutingDecision::Local {
                    target_identity: component.identity.clone(),
                };
            } else {
                return RoutingDecision::Error {
                    error: Error::receiver_unknown_with_data(receiver.to_string().into()),
                    conversation_id: Some(message.header().conversation_id.clone())
                        .unwrap_or_default(),
                };
            }
        }

        // Route to remote coordinator
        if let Some(coordinator) = self.directory.get_coordinator(receiver.namespace()) {
            RoutingDecision::Remote {
                target_dealer_identity: coordinator.dealer_identity.clone(),
            }
        } else {
            RoutingDecision::Error {
                error: Error::node_unknown_with_data(receiver.namespace().into()),
                conversation_id: Some(message.header().conversation_id.clone()).unwrap_or_default(),
            }
        }
    }
}
