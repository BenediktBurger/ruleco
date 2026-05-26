pub const VERSION: u8 = 0; // LECO protocol version

/// Message types defined in the LECO protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Undefined = 0,
    Json = 1,
}

impl From<MessageType> for u8 {
    fn from(message_type: MessageType) -> u8 {
        message_type as u8
    }
}

impl From<u8> for MessageType {
    fn from(value: u8) -> Self {
        match value {
            0 => MessageType::Undefined,
            1 => MessageType::Json,
            _ => MessageType::Undefined, // Default for unknown values
        }
    }
}

pub const DEFAULT_COORDINATOR_PORT: u16 = 12300;

pub const DATA_HEADER_SIZE: usize = 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataMessageType {
    Undefined = 0,
    Json = 1,
}

impl From<DataMessageType> for u8 {
    fn from(message_type: DataMessageType) -> u8 {
        message_type as u8
    }
}

impl From<u8> for DataMessageType {
    fn from(value: u8) -> Self {
        match value {
            0 => DataMessageType::Undefined,
            1 => DataMessageType::Json,
            _ => DataMessageType::Undefined,
        }
    }
}
