use super::message_port::Identity;
use super::ClockPort;
use super::DirectoryPort;
use crate::core::domain::RoutingResult;
use ruleco_core::message::MessageView;

/// Interface for routing messages
///
/// This port encapsulates the logic for determining where a message should be sent
/// based on its content and the current state of the system.
///
/// The `sender_identity` parameter includes transport context (Local/Remote/SelfTarget)
/// which is essential because:
/// - Local components MUST be validated (sign-in check, identity verification)
/// - Remote coordinators are already authenticated via DEALER connection
/// - Self messages require special internal processing
pub trait RoutingPort<D: DirectoryPort, C: ClockPort> {
    /// Route a message based on its destination and the identity context of its source
    ///
    /// # Parameters
    /// * `message` - The message to route
    /// * `sender_identity` - The Identity context of the sender (Local/Remote/SelfTarget)
    ///
    /// # Returns
    /// The Identity to route to, or a RoutingError for sending an error response
    fn route_message(&self, message: &MessageView, sender_identity: &Identity) -> RoutingResult;
}
