use crate::full_name::{FullName, FullNameError};
use crate::protocol_constants::{MessageType, VERSION};
use serde::Serialize;
use serde_json::Value;
use std::io;
use uuid::Uuid;

/// Represent a Conversation ID (UUID)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationId(pub [u8; 16]);

impl ConversationId {
    /// Create a new random conversation ID
    pub fn new() -> Self {
        Self(create_conversation_id())
    }

    /// Create a ConversationId from a byte array
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Get the byte array reference
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl Default for ConversationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Represent a Message ID (3 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageId(pub [u8; 3]);

impl MessageId {
    /// Create a MessageId from a byte array
    pub fn from_bytes(bytes: [u8; 3]) -> Self {
        Self(bytes)
    }

    /// Get the byte array reference
    pub fn as_bytes(&self) -> &[u8; 3] {
        &self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self([0, 0, 0])
    }
}

/// Create a new conversation id
pub fn create_conversation_id() -> [u8; 16] {
    let uuid = Uuid::now_v7();
    return uuid.into_bytes();
}

/// Header information for a message (owned)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub conversation_id: ConversationId,
    pub message_id: MessageId,
    pub message_type: u8,
}

impl Header {
    /// Create a new Header
    pub fn new(conversation_id: ConversationId, message_id: MessageId, message_type: u8) -> Self {
        Self {
            conversation_id,
            message_id,
            message_type,
        }
    }

    /// Create a Header from a header frame slice
    pub fn from_slice(frame: &[u8]) -> Result<Self, io::Error> {
        if frame.len() < 20 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Header frame too short",
            ));
        }

        let conversation_id = ConversationId::from_bytes(frame[0..16].try_into().unwrap());
        let message_id = MessageId::from_bytes(frame[16..19].try_into().unwrap());
        let message_type = frame[19];

        Ok(Self::new(conversation_id, message_id, message_type))
    }

    /// Get the raw message type value.
    pub fn message_type_raw(&self) -> u8 {
        self.message_type
    }

    /// Get the message type, interpreting it according to the standard `MessageType` enum.
    /// Unknown types (e.g., custom user types) will be mapped to `MessageType::Undefined`.
    pub fn message_type_enum(&self) -> MessageType {
        self.message_type_raw().into()
    }
}

/// Different types of content
pub enum ContentTypes {
    Frames(Vec<Vec<u8>>),
    Frame(Vec<u8>),
    Null,
}

/// Error type for Message operations
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Full name error: {0}")]
    FullName(#[from] FullNameError),
    #[error("Invalid frame count")]
    InvalidFrameCount,
    #[error("JSON serialization error: {0}")]
    JsonSerialization(#[from] serde_json::Error),
    #[error("JSON-RPC error: {0}")]
    JsonRpc(String),
}
impl From<MessageError> for io::Error {
    fn from(err: MessageError) -> io::Error {
        match err {
            MessageError::Io(io_err) => io_err,
            _ => io::Error::new(io::ErrorKind::Other, err),
        }
    }
}

/// Zero-copy view of a message for efficient inspection
///
/// This struct owns the raw frame data and provides zero-copy views into it,
/// allowing efficient inspection without copying the underlying data
/// during parsing from raw frames.
#[derive(Debug, Clone)]
pub struct MessageView {
    frames: Vec<Vec<u8>>,
    version: u8,
    receiver: Result<FullName, FullNameError>,
    sender: Result<FullName, FullNameError>,
    header: Header,
    // `payload` is a view into `self.frames`.
    payload: &'static [Vec<u8>],
}

impl MessageView {
    /// Create a MessageView from raw frames
    ///
    /// This takes ownership of the frames and parses them into zero-copy views.
    pub fn new(frames: Vec<Vec<u8>>) -> Result<Self, MessageError> {
        if frames.len() < 4 {
            return Err(MessageError::InvalidFrameCount);
        }

        let version_byte = *frames[0]
            .get(0)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing version byte"))?;

        // Parse FullName instances, which now own their data.
        let receiver_result = FullName::from_slice(&frames[1]);
        let sender_result = FullName::from_slice(&frames[2]);

        let header = Header::from_slice(&frames[3])?;
        let payload_slice: &[Vec<u8>] = &frames[4..];

        // SAFETY: The `payload_slice` points to data inside `frames`, which is owned by this struct.
        // Extending its lifetime to 'static is safe as long as `self.frames` is never moved
        // or dropped before `self.payload` is used. Since `frames` is owned and pinned within
        // this struct, and `payload` is just a view, this is a safe usage of `transmute` for
        // lifetime extension in this context.
        let payload_extended: &'static [Vec<u8>] = unsafe { std::mem::transmute(payload_slice) };

        Ok(Self {
            frames,
            version: version_byte,
            receiver: receiver_result,
            sender: sender_result,
            header,
            payload: payload_extended, // Zero-copy view of the payload
        })
    }

    /// Get the raw frames
    pub fn raw_frames(&self) -> &[Vec<u8>] {
        &self.frames
    }

    // Accessor methods
    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn receiver(&self) -> &Result<FullName, FullNameError> {
        &self.receiver
    }

    pub fn sender(&self) -> &Result<FullName, FullNameError> {
        &self.sender
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn payload(&self) -> &[Vec<u8>] {
        self.payload
    }

    pub fn content_frame(&self) -> Option<&Vec<u8>> {
        self.payload.first()
    }

    /// Extract sender with standard error handling
    ///
    /// Converts a `Result<FullName, FullNameError>` to a standard error type
    /// that can be used in higher-level application logic.
    pub fn try_sender<E, F>(&self, error_fn: F) -> Result<&FullName, E>
    where
        F: FnOnce(&FullNameError) -> E,
    {
        self.sender.as_ref().map_err(error_fn)
    }

    /// Extract receiver with standard error handling
    ///
    /// Converts a `Result<FullName, FullNameError>` to a standard error type
    /// that can be used in higher-level application logic.
    pub fn try_receiver<E, F>(&self, error_fn: F) -> Result<&FullName, E>
    where
        F: FnOnce(&FullNameError) -> E,
    {
        self.receiver.as_ref().map_err(error_fn)
    }

    /// Deserialize the payload as JSON
    pub fn payload_as_json<T>(&self) -> Result<T, MessageError>
    where
        T: serde::de::DeserializeOwned,
    {
        let content_frame = self
            .content_frame()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "No content frame"))?;

        Ok(serde_json::from_slice(content_frame)?)
    }

    /// Deserialize the payload as a generic JSON value for flexible handling
    ///
    /// This method is particularly useful when you don't know the exact structure
    /// of the JSON payload. You can then use `crate::jsonrpc_utils::classify_jsonrpc_message`
    /// to determine what type of JSON-RPC message it is.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruleco_core::message::MessageView;
    /// use ruleco_core::jsonrpc_utils::{classify_jsonrpc_message, JsonRpcMessageType};
    ///
    /// // Assuming you have a MessageView with a JSON payload
    /// // let view: MessageView = ...;
    /// //
    /// // match view.payload_as_json_value() {
    /// //     Ok(json_value) => {
    /// //         match classify_jsonrpc_message(&json_value) {
    /// //             JsonRpcMessageType::Request => { /* Handle request */ },
    /// //             JsonRpcMessageType::Notification => { /* Handle notification */ },
    /// //             JsonRpcMessageType::SuccessResponse => { /* Handle success response */ },
    /// //             JsonRpcMessageType::ErrorResponse => { /* Handle error response */ },
    /// //             JsonRpcMessageType::Batch(types) => { /* Handle batch */ },
    /// //             JsonRpcMessageType::Invalid => { /* Handle invalid message */ },
    /// //         }
    /// //     },
    /// //     Err(e) => { /* Handle deserialization error */ }
    /// // }
    /// ```
    pub fn payload_as_json_value(&self) -> Result<Value, MessageError> {
        self.payload_as_json::<Value>()
    }

    /// Convert the MessageView to an owned Message
    ///
    /// This consumes the MessageView and reconstructs an owned Message.
    /// It's efficient because it reuses the owned `frames` data.
    pub fn into_owned(self) -> Result<Message, MessageError> {
        Message::from_frames(self.frames)
    }
}

/// Represent a Message (owned components)
#[derive(Clone)]
pub struct Message {
    version: u8,
    receiver: FullName,
    sender: FullName,
    header: Header,
    payload: Vec<Vec<u8>>,
}

impl Message {
    /// Create a new Message from parsed components
    pub fn new(
        version: u8,
        receiver: FullName,
        sender: FullName,
        header: Header,
        payload: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            version,
            receiver,
            sender,
            header,
            payload,
        }
    }

    /// Create a Message from raw frames
    pub fn from_frames(frames: Vec<Vec<u8>>) -> Result<Self, MessageError> {
        if frames.len() < 4 {
            return Err(MessageError::InvalidFrameCount);
        }

        let version = *frames[0]
            .get(0)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing version byte"))?;

        // Validate receiver and sender by trying to parse them
        let receiver = FullName::from_slice(&frames[1])?;
        let sender = FullName::from_slice(&frames[2])?;

        let header = Header::from_slice(&frames[3])?;
        let payload = frames[4..].to_vec();

        Ok(Self::new(version, receiver, sender, header, payload))
    }

    /// Create a Message from a MessageView
    pub fn from_view(view: MessageView) -> Result<Self, MessageError> {
        Self::from_frames(view.raw_frames().to_vec())
    }

    /// Convert the Message back to raw frames
    pub fn to_frames(&self) -> Vec<Vec<u8>> {
        let mut header_frame = self.header.conversation_id.0.to_vec();
        header_frame.extend_from_slice(&self.header.message_id.0);
        header_frame.push(self.header.message_type);

        vec![
            vec![self.version],
            self.receiver.to_vec(),
            self.sender.to_vec(),
            header_frame,
        ]
        .into_iter()
        .chain(self.payload.clone())
        .collect()
    }

    /// Convert the Message to a MessageView
    ///
    /// This is an efficient conversion that reuses the serialized frame data.
    pub fn to_view(&self) -> Result<MessageView, MessageError> {
        MessageView::new(self.to_frames())
    }

    // Accessor methods
    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn receiver(&self) -> &FullName {
        &self.receiver
    }

    pub fn sender(&self) -> &FullName {
        &self.sender
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn payload(&self) -> &[Vec<u8>] {
        &self.payload
    }

    pub fn content_frame(&self) -> Option<&Vec<u8>> {
        self.payload.first()
    }

    /// Deserialize the payload as JSON
    pub fn payload_as_json<T>(&self) -> Result<T, MessageError>
    where
        T: serde::de::DeserializeOwned,
    {
        let content_frame = self
            .content_frame()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "No content frame"))?;

        Ok(serde_json::from_slice(content_frame)?)
    }

    /// Deserialize the payload as a generic JSON value for flexible handling
    ///
    /// This method is particularly useful when you don't know the exact structure
    /// of the JSON payload. You can then use `crate::jsonrpc_utils::classify_jsonrpc_message`
    /// to determine what type of JSON-RPC message it is.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruleco_core::message::Message;
    /// use ruleco_core::jsonrpc_utils::{classify_jsonrpc_message, JsonRpcMessageType};
    ///
    /// // Assuming you have a Message with a JSON payload
    /// // let message: Message = ...;
    /// //
    /// // match message.payload_as_json_value() {
    /// //     Ok(json_value) => {
    /// //         match classify_jsonrpc_message(&json_value) {
    /// //             JsonRpcMessageType::Request => { /* Handle request */ },
    /// //             JsonRpcMessageType::Notification => { /* Handle notification */ },
    /// //             JsonRpcMessageType::SuccessResponse => { /* Handle success response */ },
    /// //             JsonRpcMessageType::ErrorResponse => { /* Handle error response */ },
    /// //             JsonRpcMessageType::Batch(types) => { /* Handle batch */ },
    /// //             JsonRpcMessageType::Invalid => { /* Handle invalid message */ },
    /// //         }
    /// //     },
    /// //     Err(e) => { /* Handle deserialization error */ }
    /// // }
    /// ```
    pub fn payload_as_json_value(&self) -> Result<Value, MessageError> {
        self.payload_as_json::<Value>()
    }
}

/// Builder for Message
pub struct MessageBuilder {
    receiver: Option<FullName>,
    sender: Option<FullName>,
    conversation_id: Option<ConversationId>,
    message_id: Option<MessageId>,
    message_type: Option<u8>,
    payload: Vec<Vec<u8>>,
}

impl MessageBuilder {
    pub fn new() -> Self {
        Self {
            receiver: None,
            sender: None,
            conversation_id: None,
            message_id: None,
            message_type: None,
            payload: Vec::new(),
        }
    }

    pub fn receiver(mut self, receiver: FullName) -> Self {
        self.receiver = Some(receiver);
        self
    }

    pub fn receiver_bytes(mut self, receiver: Vec<u8>) -> Result<Self, MessageError> {
        let receiver_fullname = FullName::from_slice(&receiver)?;
        self.receiver = Some(receiver_fullname);
        Ok(self)
    }

    pub fn sender(mut self, sender: FullName) -> Self {
        self.sender = Some(sender);
        self
    }

    pub fn sender_bytes(mut self, sender: Vec<u8>) -> Result<Self, MessageError> {
        let sender_fullname = FullName::from_slice(&sender)?;
        self.sender = Some(sender_fullname);
        Ok(self)
    }

    pub fn conversation_id(mut self, conversation_id: ConversationId) -> Self {
        self.conversation_id = Some(conversation_id);
        self
    }

    pub fn message_id(mut self, message_id: MessageId) -> Self {
        self.message_id = Some(message_id);
        self
    }

    pub fn message_type(mut self, message_type: u8) -> Self {
        self.message_type = Some(message_type);
        self
    }

    pub fn payload_single(mut self, frame: Vec<u8>) -> Self {
        self.payload = vec![frame];
        self
    }

    pub fn payload_multi(mut self, frames: Vec<Vec<u8>>) -> Self {
        self.payload = frames;
        self
    }

    /// Set the payload as a serialized JSON object
    pub fn payload_json<T>(mut self, value: &T) -> Result<Self, MessageError>
    where
        T: Serialize,
    {
        let json_bytes = serde_json::to_vec(value)?;
        self.payload = vec![json_bytes];
        self.message_type = Some(MessageType::Json.into());
        Ok(self)
    }

    pub fn build(self) -> Result<Message, MessageError> {
        let receiver = self.receiver.ok_or_else(|| {
            MessageError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Receiver is required",
            ))
        })?;

        let sender = self.sender.ok_or_else(|| {
            MessageError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Sender is required",
            ))
        })?;

        let message_type = self
            .message_type
            .unwrap_or_else(|| MessageType::Undefined.into());

        let header = Header::new(
            self.conversation_id.unwrap_or_default(),
            self.message_id.unwrap_or_default(),
            message_type,
        );

        Ok(Message::new(
            VERSION,
            receiver,
            sender,
            header,
            self.payload,
        ))
    }
}

impl Default for MessageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonrpc_utils::{classify_jsonrpc_message, JsonRpcMessageType};
    use crate::protocol_constants::MessageType;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    fn create_test_message() -> Message {
        let receiver = FullName::from_slice(b"N1.receiver").unwrap();
        let sender = FullName::from_slice(b"N1.sender").unwrap();

        MessageBuilder::new()
            .receiver(receiver)
            .sender(sender)
            .message_type(MessageType::Json.into())
            .payload_single(b"content".to_vec())
            .build()
            .unwrap()
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestPayload {
        name: String,
        value: i32,
    }

    #[test]
    fn test_message_creation() {
        let msg = create_test_message();
        assert_eq!(msg.version(), VERSION);
        assert_eq!(msg.receiver().namespace(), b"N1");
        assert_eq!(msg.receiver().name(), b"receiver");
        assert_eq!(msg.sender().namespace(), b"N1");
        assert_eq!(msg.sender().name(), b"sender");
        assert_eq!(msg.header().message_type_enum(), MessageType::Json);
        assert_eq!(msg.content_frame().unwrap(), &b"content".to_vec());
    }

    #[test]
    fn test_message_roundtrip() {
        let original_msg = create_test_message();
        let frames = original_msg.to_frames();
        let reconstructed_msg = Message::from_frames(frames).unwrap();

        assert_eq!(original_msg.version(), reconstructed_msg.version());
        assert_eq!(original_msg.receiver(), reconstructed_msg.receiver());
        assert_eq!(original_msg.sender(), reconstructed_msg.sender());
        assert_eq!(original_msg.header(), reconstructed_msg.header());
        assert_eq!(original_msg.payload(), reconstructed_msg.payload());
    }

    #[test]
    fn test_header_message_type_enum() {
        let receiver = FullName::from_slice(b"N1.receiver").unwrap();
        let sender = FullName::from_slice(b"N1.sender").unwrap();

        let msg = MessageBuilder::new()
            .receiver(receiver)
            .sender(sender)
            .message_type(MessageType::Json.into())
            .payload_single(b"content".to_vec())
            .build()
            .unwrap();

        assert_eq!(msg.header().message_type_enum(), MessageType::Json);

        // Test with a custom type
        let custom_msg = MessageBuilder::new()
            .receiver(FullName::from_slice(b"N1.receiver").unwrap())
            .sender(FullName::from_slice(b"N1.sender").unwrap())
            .message_type(150) // Custom type
            .payload_single(b"content".to_vec())
            .build()
            .unwrap();

        assert_eq!(
            custom_msg.header().message_type_enum(),
            MessageType::Undefined
        );
        assert_eq!(custom_msg.header().message_type_raw(), 150);
    }

    #[test]
    fn test_message_view() {
        let frames = vec![
            vec![VERSION],
            b"N1.receiver".to_vec(),
            b"N1.sender".to_vec(),
            vec![
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 0, 1, 1,
            ], // header with message type 1
            b"content".to_vec(),
        ];

        let view = MessageView::new(frames.clone()).unwrap();
        assert_eq!(view.version(), VERSION);
        assert_eq!(view.receiver().as_ref().unwrap().namespace(), b"N1");
        assert_eq!(view.receiver().as_ref().unwrap().name(), b"receiver");
        assert_eq!(view.sender().as_ref().unwrap().namespace(), b"N1");
        assert_eq!(view.sender().as_ref().unwrap().name(), b"sender");
        assert_eq!(view.header().message_type_enum(), MessageType::Json);
        assert_eq!(view.content_frame().unwrap(), &b"content".to_vec());
        assert_eq!(view.raw_frames(), frames);
    }

    #[test]
    fn test_message_conversion() {
        let frames = vec![
            vec![VERSION],
            b"N1.receiver".to_vec(),
            b"N1.sender".to_vec(),
            vec![
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 0, 1, 1,
            ], // header with message type 1
            b"content".to_vec(),
        ];

        // Create a view from frames
        let view = MessageView::new(frames.clone()).unwrap();

        // Convert view to owned message
        let msg = Message::from_view(view).unwrap();
        assert_eq!(msg.version(), VERSION);
        assert_eq!(msg.receiver().namespace(), b"N1");
        assert_eq!(msg.receiver().name(), b"receiver");
        assert_eq!(msg.sender().namespace(), b"N1");
        assert_eq!(msg.sender().name(), b"sender");
        assert_eq!(msg.header().message_type_enum(), MessageType::Json);
        assert_eq!(msg.content_frame().unwrap(), &b"content".to_vec());

        // Convert owned message back to frames
        let reconstructed_frames = msg.to_frames();
        assert_eq!(frames, reconstructed_frames);
    }

    #[test]
    fn test_json_payload() {
        let payload = TestPayload {
            name: "test".to_string(),
            value: 42,
        };

        let receiver = FullName::from_slice(b"N1.receiver").unwrap();
        let sender = FullName::from_slice(b"N1.sender").unwrap();

        let msg = MessageBuilder::new()
            .receiver(receiver)
            .sender(sender)
            .payload_json(&payload)
            .unwrap()
            .build()
            .unwrap();

        // Check that the message type is set to JSON
        assert_eq!(msg.header().message_type_enum(), MessageType::Json);

        // Deserialize the payload back
        let deserialized_payload: TestPayload = msg.payload_as_json().unwrap();
        assert_eq!(payload, deserialized_payload);
    }

    #[test]
    fn test_flexible_json_handling() {
        // Test with a JSON-RPC-like request
        let request_json = json!({
            "method": "subtract",
            "params": [42, 23],
            "id": 1
        });

        let receiver1 = FullName::from_slice(b"N1.receiver").unwrap();
        let sender1 = FullName::from_slice(b"N1.sender").unwrap();

        let msg = MessageBuilder::new()
            .receiver(receiver1)
            .sender(sender1)
            .payload_json(&request_json)
            .unwrap()
            .build()
            .unwrap();

        // Deserialize the payload as a generic JSON value
        let json_value = msg.payload_as_json_value().unwrap();

        // Verify it's the correct structure
        assert_eq!(json_value["method"], "subtract");
        assert_eq!(json_value["params"].as_array().unwrap().len(), 2);
        assert_eq!(json_value["id"], 1);

        // Classify the message
        assert_eq!(
            classify_jsonrpc_message(&json_value),
            JsonRpcMessageType::Request
        );

        // Test with a JSON-RPC-like response
        let response_json = json!({
            "result": 19,
            "id": 1
        });

        let receiver2 = FullName::from_slice(b"N1.receiver").unwrap();
        let sender2 = FullName::from_slice(b"N1.sender").unwrap();

        let msg2 = MessageBuilder::new()
            .receiver(receiver2)
            .sender(sender2)
            .payload_json(&response_json)
            .unwrap()
            .build()
            .unwrap();

        // Deserialize the response payload as a generic JSON value
        let json_value2 = msg2.payload_as_json_value().unwrap();

        // Verify it's the correct structure
        assert_eq!(json_value2["result"], 19);
        assert_eq!(json_value2["id"], 1);

        // Classify the message
        assert_eq!(
            classify_jsonrpc_message(&json_value2),
            JsonRpcMessageType::SuccessResponse
        );

        // Test with a batch (array of requests/responses)
        let batch_json = json!([
            {
                "method": "subtract",
                "params": [42, 23],
                "id": 1
            },
            {
                "method": "add",
                "params": [1, 2],
                "id": 2
            }
        ]);

        let receiver3 = FullName::from_slice(b"N1.receiver").unwrap();
        let sender3 = FullName::from_slice(b"N1.sender").unwrap();

        let msg3 = MessageBuilder::new()
            .receiver(receiver3)
            .sender(sender3)
            .payload_json(&batch_json)
            .unwrap()
            .build()
            .unwrap();

        // Deserialize the batch payload as a generic JSON value
        let json_value3 = msg3.payload_as_json_value().unwrap();

        // Verify it's an array
        assert!(json_value3.is_array());
        assert_eq!(json_value3.as_array().unwrap().len(), 2);

        // Classify the message
        assert_eq!(
            classify_jsonrpc_message(&json_value3),
            JsonRpcMessageType::Batch(vec![
                JsonRpcMessageType::Request,
                JsonRpcMessageType::Request
            ])
        );
    }
}
