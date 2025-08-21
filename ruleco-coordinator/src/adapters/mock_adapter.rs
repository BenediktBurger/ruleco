use crate::core::ports::message_receiver_port::Identity;
use crate::core::ports::{MessageReceiverPort, MessageSenderPort};
use ruleco_core::message::MessageView;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// Mock implementation of message sender and receiver ports for testing
pub struct MockAdapter {
    /// Messages that would be sent to local components
    pub sent_to_local: Rc<RefCell<Vec<(Vec<u8>, MessageView)>>>,
    /// Messages that would be sent to remote coordinators
    pub sent_to_remote: Rc<RefCell<Vec<(Vec<u8>, MessageView)>>>,
    /// Messages to be received from local components (pre-populated for tests)
    pub local_receive_queue: Rc<RefCell<VecDeque<(Vec<u8>, MessageView)>>>,
    /// Messages to be received from remote coordinators (pre-populated for tests)
    pub remote_receive_queue: Rc<RefCell<VecDeque<(Vec<u8>, MessageView)>>>,
}

impl MockAdapter {
    /// Create a new mock adapter
    pub fn new() -> Self {
        Self {
            sent_to_local: Rc::new(RefCell::new(Vec::new())),
            sent_to_remote: Rc::new(RefCell::new(Vec::new())),
            local_receive_queue: Rc::new(RefCell::new(VecDeque::new())),
            remote_receive_queue: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    /// Add a message to be received from a local component
    pub fn add_local_message(&self, identity: Vec<u8>, message: MessageView) {
        self.local_receive_queue
            .borrow_mut()
            .push_back((identity, message));
    }

    /// Add a message to be received from a remote coordinator
    pub fn add_remote_message(&self, dealer_identity: Vec<u8>, message: MessageView) {
        self.remote_receive_queue
            .borrow_mut()
            .push_back((dealer_identity, message));
    }

    /// Clear all recorded sent messages
    pub fn clear_sent_messages(&self) {
        self.sent_to_local.borrow_mut().clear();
        self.sent_to_remote.borrow_mut().clear();
    }

    /// Get a clone of sent messages to local components
    pub fn get_sent_to_local(&self) -> Vec<(Vec<u8>, MessageView)> {
        self.sent_to_local
            .borrow()
            .iter()
            .map(|(identity, message)| (identity.clone(), message.clone()))
            .collect()
    }

    /// Get a clone of sent messages to remote coordinators
    pub fn get_sent_to_remote(&self) -> Vec<(Vec<u8>, MessageView)> {
        self.sent_to_remote
            .borrow()
            .iter()
            .map(|(identity, message)| (identity.clone(), message.clone()))
            .collect()
    }
}

impl MessageSenderPort for MockAdapter {
    fn send_to_local(
        &self,
        identity: &[u8],
        message: MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.sent_to_local
            .borrow_mut()
            .push((identity.to_vec(), message.clone()));
        Ok(())
    }

    fn send_to_remote(
        &self,
        dealer_identity: &[u8],
        message: MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.sent_to_remote
            .borrow_mut()
            .push((dealer_identity.to_vec(), message.clone()));
        Ok(())
    }
}

impl MessageReceiverPort for MockAdapter {
    fn receive_message_from_local(
        &self,
    ) -> Result<(Vec<u8>, MessageView), Box<dyn std::error::Error>> {
        self.local_receive_queue
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| "No messages in local receive queue".into())
    }

    fn receive_message_from_remote(
        &self,
        dealer_identity: &[u8],
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        // Find a message for this dealer identity
        let mut queue = self.remote_receive_queue.borrow_mut();
        let position = queue
            .iter()
            .position(|(id, _)| id == dealer_identity)
            .ok_or_else(|| "No messages in remote receive queue for this dealer")?;

        Ok(queue.remove(position).unwrap().1)
    }

    fn receive_messages(
        &self,
        _timeout_ms: u64,
    ) -> Result<Vec<(Identity, MessageView)>, Box<dyn std::error::Error>> {
        let mut messages = Vec::new();

        // Process all local messages
        while let Some((identity, message)) = self.local_receive_queue.borrow_mut().pop_front() {
            messages.push((Identity::Local { identity }, message));
        }

        // Process all remote messages
        let mut remote_queue = self.remote_receive_queue.borrow_mut();
        while let Some((dealer_identity, message)) = remote_queue.pop_front() {
            messages.push((
                Identity::Remote {
                    identity: dealer_identity,
                },
                message,
            ));
        }

        Ok(messages)
    }
}
