use crate::core::ports::message_port::Identity;
use ruleco_core::message::ConversationId;

/// Error returned when routing fails, includes context for sending error response
#[derive(Debug)]
pub struct RoutingError {
    /// The error to send back
    pub error: ruleco_core::errors::Error,
    /// The conversation ID to use in the error response
    pub conversation_id: ConversationId,
}

impl RoutingError {
    pub fn new(error: ruleco_core::errors::Error, conversation_id: ConversationId) -> Self {
        Self {
            error,
            conversation_id,
        }
    }
}

/// Routing result: identity to send to, or error
pub type RoutingResult = Result<Identity, RoutingError>;
