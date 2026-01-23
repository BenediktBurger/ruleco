use ruleco_core::message::ConversationId;

/// The decision made by the routing logic about where to send a message
#[derive(Debug)]
pub enum RoutingDecision {
    /// Send the message to a local component
    Local {
        /// The ZMQ identity of the target component
        target_identity: Vec<u8>,
    },
    /// Send the message to another coordinator
    Remote {
        /// The ZMQ dealer identity of the target coordinator
        target_dealer_identity: Vec<u8>,
    },
    /// Handle the message internally (addressed to this coordinator)
    SelfTarget,
    /// Handle a response from a remote coordinator we're connecting to
    PendingConnectionResponse {
        /// The ZMQ dealer identity of the remote coordinator
        dealer_identity: Vec<u8>,
    },
    /// Return an error to the sender
    Error {
        /// The error to send back
        error: ruleco_core::errors::Error,
        /// The conversation ID to use in the error response
        conversation_id: ConversationId,
    },
}
