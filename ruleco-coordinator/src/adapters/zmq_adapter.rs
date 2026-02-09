use crate::core::ports::message_port::Identity;
use crate::core::ports::{ConnectionManagementPort, MessagePort};
use ruleco_core::message::ConversationId;
use zmq;

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

    /// Poll for messages with a timeout
    ///
    /// # Parameters
    /// * `timeout_ms` - Timeout in milliseconds
    ///
    /// # Returns
    /// A vector of indices of readable sockets, or empty vector if no sockets are readable
    fn poll_messages(&self, timeout_ms: i64) -> Result<Vec<usize>, Box<dyn std::error::Error>> {
        // Pre-allocate poll items vector with capacity for router + estimated dealer sockets
        // This reduces allocations compared to the previous approach
        let mut poll_items = Vec::with_capacity(1 + self.dealer_sockets.len());

        // Add router socket first (index 0)
        poll_items.push(self.router_socket.as_poll_item(zmq::POLLIN));

        // Add dealer sockets (indices 1..n)
        // We collect the sockets first to avoid borrowing conflicts
        let dealer_sockets: Vec<&zmq::Socket> = self.dealer_sockets.values().collect();

        for socket in dealer_sockets {
            poll_items.push(socket.as_poll_item(zmq::POLLIN));
        }

        // Poll for messages with a timeout
        let mut readable_indices = Vec::new();
        if zmq::poll(&mut poll_items[..], timeout_ms)? > 0 {
            // Collect indices of readable sockets first
            for (i, poll_item) in poll_items.iter().enumerate() {
                if poll_item.is_readable() {
                    readable_indices.push(i);
                }
            }
        }

        Ok(readable_indices)
    }

    /// Receive a message from a socket (ROUTER), returning the identity and frames
    fn receive_frames_from_router(
        &self,
    ) -> Result<(Vec<u8>, Vec<Vec<u8>>), Box<dyn std::error::Error>> {
        let identity = self.router_socket.recv_bytes(0)?;
        let frames = self.router_socket.recv_multipart(0)?;
        Ok((identity, frames))
    }

    /// Receive frames from a DEALER socket (no identity prefix)
    fn receive_frames_from_dealer(
        &self,
        socket: &zmq::Socket,
    ) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        let frames = socket.recv_multipart(0)?;
        if frames.is_empty() {
            return Err("Invalid message format: no parts".into());
        }
        Ok(frames)
    }

    /// Receive all available messages within a timeout
    ///
    /// This polls all sockets (ROUTER and DEALERs) and returns all available messages.
    fn receive_frames_all(
        &self,
        timeout_ms: u64,
    ) -> Result<Vec<(Vec<u8>, Vec<Vec<u8>>)>, Box<dyn std::error::Error>> {
        let mut messages: Vec<(Vec<u8>, Vec<Vec<u8>>)> = Vec::new();

        let readable_indices = self.poll_messages(timeout_ms as i64)?;

        if !readable_indices.is_empty() {
            let dealer_identities: Vec<Vec<u8>> = self
                .dealer_sockets_iter()
                .map(|(identity, _)| identity.clone())
                .collect();

            if readable_indices.contains(&0) {
                let (identity, frames) = self.receive_frames_from_router()?;
                messages.push((identity, frames));
            }

            for &index in &readable_indices {
                if index > 0 && index <= dealer_identities.len() {
                    let dealer_identity = &dealer_identities[index - 1];
                    if let Some(socket) = self.dealer_sockets.get(dealer_identity) {
                        let frames = self.receive_frames_from_dealer(socket)?;
                        // For DEALER, we tag with the dealer identity for routing purposes
                        messages.push((dealer_identity.clone(), frames));
                    }
                }
            }
        }

        Ok(messages)
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

    fn recv(&self) -> Result<(Identity, Vec<Vec<u8>>), Box<dyn std::error::Error>> {
        let poll_item = self.router_socket.as_poll_item(zmq::POLLIN);

        if zmq::poll(&mut [poll_item], -1)? > 0 {
            let (identity, frames) = self.receive_frames_from_router()?;
            return Ok((Identity::Component { identity }, frames));
        }

        Err("No messages available".into())
    }

    fn recv_all(
        &self,
        timeout_ms: u64,
    ) -> Result<Vec<(Identity, Vec<Vec<u8>>)>, Box<dyn std::error::Error>> {
        let mut messages: Vec<(Identity, Vec<Vec<u8>>)> = Vec::new();
        let raw_messages = self.receive_frames_all(timeout_ms)?;

        for (identity, frames) in raw_messages {
            let ident_enum = if self.dealer_sockets.contains_key(&identity) {
                Identity::Coordinator { identity }
            } else {
                Identity::Component { identity }
            };
            messages.push((ident_enum, frames));
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
