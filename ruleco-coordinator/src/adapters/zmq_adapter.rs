use std::i64;

use crate::core::ports::message_port::Identity;
use crate::core::ports::{ConnectionManagementPort, MessagePort};
use ruleco_core::message::ConversationId;
use zmq;

const DEALER_POLL_TIMEOUT_MS: i64 = 10;

/// ZMQ implementation of the message port and connection management port
pub struct ZmqAdapter {
    /// The ZMQ context
    context: zmq::Context,
    /// The ROUTER socket for communicating with local components
    router_socket: zmq::Socket,
    /// The DEALER sockets for communicating with remote coordinators
    dealer_sockets: std::collections::HashMap<Vec<u8>, zmq::Socket>,
}

impl ZmqAdapter {
    /// Create a new ZMQ adapter
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let context = zmq::Context::new();
        let router_socket = context.socket(zmq::ROUTER)?;

        Ok(Self {
            context,
            router_socket,
            dealer_sockets: std::collections::HashMap::new(),
        })
    }

    /// Connect to a remote coordinator
    fn connect_to_coordinator_impl(
        &mut self,
        address: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let dealer_socket = self.context.socket(zmq::DEALER)?;
        dealer_socket.connect(address)?;
        let dealer_identity = ConversationId::new().as_bytes().to_vec();
        self.dealer_sockets
            .insert(dealer_identity.clone(), dealer_socket);
        Ok(dealer_identity)
    }

    /// Disconnect from a remote coordinator
    fn disconnect_from_coordinator_impl(
        &mut self,
        dealer_identity: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.dealer_sockets.remove(dealer_identity);
        Ok(())
    }

    /// Iterator over dealer sockets
    fn dealer_sockets_iter(&self) -> impl Iterator<Item = (&Vec<u8>, &zmq::Socket)> {
        self.dealer_sockets.iter()
    }

    /// Poll router socket for messages
    fn poll_router(&self, timeout_ms: i64) -> Result<bool, Box<dyn std::error::Error>> {
        let mut poll_items = vec![self.router_socket.as_poll_item(zmq::POLLIN)];
        Ok(zmq::poll(&mut poll_items, timeout_ms)? > 0)
    }

    /// Receive a message from the router socket
    fn receive_from_router(&self) -> Result<(Identity, Vec<Vec<u8>>), Box<dyn std::error::Error>> {
        let identity = self.router_socket.recv_bytes(0)?;
        let frames = self.router_socket.recv_multipart(0)?;
        Ok((Identity::Component { identity }, frames))
    }

    /// Poll dealer sockets for messages
    fn poll_dealers(&self) -> Result<(Vec<usize>, Vec<Vec<u8>>), Box<dyn std::error::Error>> {
        let dealer_identities: Vec<Vec<u8>> = self
            .dealer_sockets_iter()
            .map(|(id, _)| id.clone())
            .collect();

        if dealer_identities.is_empty() {
            return Ok((Vec::new(), dealer_identities));
        }

        let mut poll_items = Vec::with_capacity(1 + dealer_identities.len());
        poll_items.push(self.router_socket.as_poll_item(zmq::POLLIN));
        for socket in self.dealer_sockets.values() {
            poll_items.push(socket.as_poll_item(zmq::POLLIN));
        }

        let mut readable = Vec::new();
        if zmq::poll(&mut poll_items[..], DEALER_POLL_TIMEOUT_MS)? > 0 {
            for (i, item) in poll_items.iter().enumerate() {
                if item.is_readable() {
                    readable.push(i);
                }
            }
        }

        Ok((readable, dealer_identities))
    }

    /// Receive a message from a dealer socket
    fn receive_from_dealer(
        &self,
        socket: &zmq::Socket,
    ) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        let frames = socket.recv_multipart(0)?;
        if frames.is_empty() {
            return Err("Invalid message format: no parts".into());
        }
        Ok(frames)
    }
}

impl MessagePort for ZmqAdapter {
    fn send(
        &self,
        dest_identity: &Identity,
        frames: Vec<Vec<u8>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match dest_identity {
            Identity::Component { identity } => {
                self.router_socket.send(identity, zmq::SNDMORE)?;
                self.router_socket.send_multipart(frames, 0)?;
            }
            Identity::Coordinator { identity } => {
                if let Some(socket) = self.dealer_sockets.get(identity) {
                    socket.send_multipart(frames, 0)?;
                } else {
                    return Err("Coordinator connection not found".into());
                }
            }
            Identity::SelfTarget => {
                return Err("Self-targeted messages should be handled internally".into());
            }
        }
        Ok(())
    }

    fn recv(&self, timeout_ms: u64) -> Result<Option<(Identity, Vec<Vec<u8>>)>, Box<dyn std::error::Error>> {
        let timeout_i64 = i64::try_from(timeout_ms).unwrap_or(i64::MAX);

        if self.poll_router(timeout_i64)? {
            Ok(Some(self.receive_from_router()?))
        } else {
            Ok(None)
        }
    }

    fn recv_coordinator_sign_ins(
        &self,
    ) -> Result<Vec<(Identity, Vec<Vec<u8>>)>, Box<dyn std::error::Error>> {
        let mut messages = Vec::new();
        let (readable, dealer_ids) = self.poll_dealers()?;

        for index in readable {
            if index > 0 && index <= dealer_ids.len() {
                let dealer_id = &dealer_ids[index - 1];
                if let Some(socket) = self.dealer_sockets.get(dealer_id) {
                    if let Ok(frames) = self.receive_from_dealer(socket) {
                        messages.push((Identity::Coordinator { identity: dealer_id.clone() }, frames));
                    }
                }
            }
        }

        Ok(messages)
    }
}

impl ConnectionManagementPort for ZmqAdapter {
    fn listen_for_components(&mut self, address: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.router_socket.bind(address)?;
        Ok(())
    }

    fn connect_to_coordinator(
        &mut self,
        address: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        self.connect_to_coordinator_impl(address)
    }

    fn disconnect_from_coordinator(
        &mut self,
        identity: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.disconnect_from_coordinator_impl(identity)
    }
}
