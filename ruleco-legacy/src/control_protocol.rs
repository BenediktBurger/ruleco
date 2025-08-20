pub use ruleco_core::message::{Header, Message};

pub mod communicator;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    // JSONRPC 2.0 defined errors
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
    ParseError,
    ServerError,
    // LECO errors
    NotSignedIn,
    DuplicateName,
    NodeUnknown,
    ReceiverUnknown,
}

impl Error {
    pub fn code(&self) -> i16 {
        match &self {
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::InternalError => -32603,
            Self::ParseError => -32700,
            Self::ServerError => -32000,
            Self::NotSignedIn => -32090,
            Self::DuplicateName => -32091,
            Self::NodeUnknown => -32092,
            Self::ReceiverUnknown => -32093,
            //_ => -32000,
        }
    }

    pub fn message(&self) -> &str {
        match &self {
            Self::NotSignedIn => "Component not signed in yet!",
            Self::DuplicateName => "The name is already taken.",
            Self::NodeUnknown => "Node is unknown.",
            Self::ReceiverUnknown => "Receiver is not in addresses list.",
            _ => "Server error.",
        }
    }
}
