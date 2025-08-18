use crate::core::domain::{ComponentEntry, CoordinatorEntry, RoutingDecision};
use crate::core::ports::{ClockPort, DirectoryPort, RoutingPort};
use jsonrpsee_types::{ErrorCode, ErrorObject, Request};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use ruleco_core::message::{ConversationId, MessageView};

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
        identity: Vec<u8>,
    ) -> Result<(), Error> {
        // If the component name doesn't have a namespace, prepend our namespace to it
        let full_name = if component_name.has_namespace() {
            component_name
        } else {
            FullName::new(self.namespace.clone(), component_name.name().to_vec())
        };

        let component = ComponentEntry {
            name: full_name,
            identity,
            last_seen: self.clock.now(),
        };

        self.directory.add_local_component(component)
    }

    /// Handle a component signing out
    pub fn handle_sign_out(
        &mut self,
        component_name: FullName,
    ) -> Result<Option<ComponentEntry>, Error> {
        self.directory.remove_local_component(component_name)
    }

    /// Check for timed out components
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

        timed_out_components
    }

    /// Add a coordinator to our network view
    pub fn add_coordinator(&mut self, entry: CoordinatorEntry) -> Result<(), Error> {
        self.directory.add_coordinator(entry)
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
    /// Route a message based on its destination
    fn route_message(&self, message: &MessageView, sender_identity: &[u8]) -> RoutingDecision {
        // Parse sender and receiver
        let sender = match message.try_sender(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError))) {
            Ok(sender) => sender,
            Err(error) => {
                return RoutingDecision::Error {
                    error,
                    conversation_id: Some(message.header().conversation_id.clone())
                        .unwrap_or(ConversationId::default()),
                }
            }
        };

        let receiver = match message.try_receiver(|_| Error::JsonRpc(ErrorObject::from(ErrorCode::ParseError))) {
            Ok(receiver) => receiver,
            Err(error) => {
                return RoutingDecision::Error {
                    error,
                    conversation_id: Some(message.header().conversation_id.clone())
                        .unwrap_or_default(),
                }
            }
        };

        // Check if sender is signed in (unless it's a sign_in message to coordinator)
        let is_sign_in_to_coordinator =
            (receiver.name() == b"COORDINATOR") && self.is_sign_in_message(message);

        if !is_sign_in_to_coordinator {
            // Check if sender is in our local directory with matching identity
            if let Some(component) = self.directory.get_local_component(&sender) {
                if component.identity != sender_identity {
                    return RoutingDecision::Error {
                        error: Error::duplicate_name_with_data(serde_json::Value::String(sender.to_string())),
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

        // Check if message is addressed to this coordinator
        if receiver.name() == b"COORDINATOR"
            && (receiver.namespace() == &self.namespace[..] || receiver.namespace().is_empty())
        {
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
