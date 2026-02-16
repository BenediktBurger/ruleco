use crate::core::ports::message_port::Identity;
use crate::core::ports::{ConnectionManagementPort, MessagePort};
use ruleco_core::message::MessageView;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// Mock implementation of message port and connection management for testing
pub struct MockAdapter {
    /// All sent messages (Identity -> frames)
    pub sent_messages: Rc<RefCell<Vec<(Identity, Vec<Vec<u8>>)>>>,
    /// Messages to be received from device socket (pre-populated for tests)
    pub receive_queue_device: Rc<RefCell<VecDeque<(Identity, Vec<Vec<u8>>)>>>,
    /// Messages to be received from dealer socket (pre-populated for tests)
    pub receive_queue_dealer: Rc<RefCell<VecDeque<(Identity, Vec<Vec<u8>>)>>>,
    /// Connected DEALER identities
    pub connected_dealers: Rc<RefCell<Vec<Identity>>>,
    /// Next dealer identity to return
    next_dealer_identity: RefCell<usize>,
}

impl MockAdapter {
    /// Create a new mock adapter
    pub fn new() -> Self {
        Self {
            sent_messages: Rc::new(RefCell::new(Vec::new())),
            receive_queue_device: Rc::new(RefCell::new(VecDeque::new())),
            receive_queue_dealer: Rc::new(RefCell::new(VecDeque::new())),
            connected_dealers: Rc::new(RefCell::new(Vec::new())),
            next_dealer_identity: RefCell::new(1),
        }
    }

    /// Get a clone of sent messages
    pub fn get_all_sent_messages(&self) -> Vec<(Identity, Vec<Vec<u8>>)> {
        self.sent_messages
            .borrow()
            .iter()
            .map(|(identity, frames)| (identity.clone(), frames.clone()))
            .collect()
    }

    /// Get messages sent to local components (Identity::Component)
    pub fn get_sent_to_local(&self) -> Vec<(Vec<u8>, MessageView)> {
        self.sent_messages
            .borrow()
            .iter()
            .filter_map(|(identity, frames)| {
                if let Identity::Component { identity } = identity {
                    if let Ok(view) = MessageView::new(frames.clone()) {
                        Some((identity.clone(), view))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get messages sent to remote coordinators (Identity::Coordinator)
    pub fn get_sent_to_remote(&self) -> Vec<(Vec<u8>, MessageView)> {
        self.sent_messages
            .borrow()
            .iter()
            .filter_map(|(identity, frames)| {
                if let Identity::Coordinator { identity } = identity {
                    if let Ok(view) = MessageView::new(frames.clone()) {
                        Some((identity.clone(), view))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    /// Add a message to be received from a local component
    pub fn add_local_message(&self, identity: Vec<u8>, message: MessageView) {
        self.receive_queue_device.borrow_mut().push_back((
            Identity::Component { identity },
            message.raw_frames().to_vec(),
        ));
    }

    /// Add a message to be received from a remote coordinator (dealer)
    pub fn add_dealer_message(&self, dealer_identity: Vec<u8>, message: MessageView) {
        self.receive_queue_dealer.borrow_mut().push_back((
            Identity::Coordinator {
                identity: dealer_identity,
            },
            message.raw_frames().to_vec(),
        ));
    }

    /// Add raw frames to be received from local component
    pub fn add_local_message_raw(&self, identity: Vec<u8>, frames: Vec<Vec<u8>>) {
        self.receive_queue_device
            .borrow_mut()
            .push_back((Identity::Component { identity }, frames));
    }

    /// Add raw frames to be received from remote coordinator (dealer)
    pub fn add_dealer_message_raw(&self, dealer_identity: Vec<u8>, frames: Vec<Vec<u8>>) {
        self.receive_queue_dealer.borrow_mut().push_back((
            Identity::Coordinator {
                identity: dealer_identity,
            },
            frames,
        ));
    }

    /// Clear all recorded sent messages
    pub fn clear_sent_messages(&self) {
        self.sent_messages.borrow_mut().clear();
    }

    /// Check if dealer identity is connected
    pub fn is_dealer_connected(&self, dealer_identity: &[u8]) -> bool {
        self.connected_dealers
            .borrow()
            .iter()
            .any(|id| matches!(id, Identity::Coordinator { identity } if identity.as_slice() == dealer_identity))
    }

    /// Get next dealer identity
    fn next_dealer_identity(&self) -> Vec<u8> {
        let mut next = self.next_dealer_identity.borrow_mut();
        let identity = format!("dealer-{}", *next);
        *next += 1;
        identity.into_bytes()
    }
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MessagePort for MockAdapter {
    fn send(
        &self,
        dest_identity: &Identity,
        frames: Vec<Vec<u8>>,
    ) -> Result<(), Box<dyn std::error::Error>>{
        match dest_identity {
            Identity::SelfTarget => {
                return Err("send_to_self not implemented for MockAdapter".into());
            }
            _ => {
                self.sent_messages.borrow_mut().push((dest_identity.clone(), frames));
            }
        }
        Ok(())
    }

    fn recv(&self, _timeout_ms: i64) -> Result<Option<(Identity, Vec<Vec<u8>>)>, Box<dyn std::error::Error>>{
        let mut queue = self.receive_queue_device.borrow_mut();
        Ok(queue.pop_front())
    }

    fn recv_coordinator_sign_ins(
        &self,
    ) -> Result<Vec<(Identity, Vec<Vec<u8>>)>, Box<dyn std::error::Error>>{
        let mut messages = Vec::new();
        let mut queue = self.receive_queue_dealer.borrow_mut();

        while let Some((identity, frames)) = queue.pop_front() {
            messages.push((identity, frames));
        }

        Ok(messages)
    }
}

impl ConnectionManagementPort for MockAdapter {
    fn listen_for_components(&mut self, _address: &str) -> Result<(), Box<dyn std::error::Error>>{
        Ok(())
    }

    fn connect_to_coordinator(
        &mut self,
        _address: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>>{
        let identity = self.next_dealer_identity();
        self.connected_dealers
            .borrow_mut()
            .push(Identity::Coordinator {
                identity: identity.clone(),
            });
        Ok(identity)
    }

    fn disconnect_from_coordinator(
        &mut self,
        dealer_identity: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>{
        self.connected_dealers.borrow_mut().retain(|id| {
            !matches!(id, Identity::Coordinator { identity } if identity.as_slice() == dealer_identity)
        });
        Ok(())
    }
}
