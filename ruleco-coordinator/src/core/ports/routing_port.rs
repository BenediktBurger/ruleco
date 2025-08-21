use crate::core::{domain::RoutingDecision, ports::message_receiver_port::Identity};
use ruleco_core::message::MessageView;

/// Interface for routing messages
///
/// This port encapsulates the logic for determining where a message should be sent
/// based on its content and the current state of the system.
pub trait RoutingPort {
    /// Route a message based on its destination
    ///
    /// # Parameters
    /// * `message` - The message to route
    /// * `sender_identity` - The ZMQ identity of the component that sent the message
    ///
    /// # Returns
    /// A routing decision indicating where the message should be sent
    fn route_message(&self, message: &MessageView, sender_identity: &Identity) -> RoutingDecision;
}
