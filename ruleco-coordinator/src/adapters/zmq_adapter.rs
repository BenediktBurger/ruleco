use crate::core::ports::message_receiver_port::Identity;
use crate::core::ports::{MessageReceiverPort, MessageSenderPort};
use ruleco_core::message::MessageView;
use zmq;

/// ZMQ implementation of the message sender and receiver ports
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

    /// Bind the router socket to an address
    pub fn bind_router(&mut self, address: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.router_socket.bind(address)?;
        Ok(())
    }

    /// Connect to a remote coordinator
    pub fn connect_to_coordinator(
        &mut self,
        dealer_identity: Vec<u8>,
        address: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dealer_socket = self.context.socket(zmq::DEALER)?;
        dealer_socket.connect(address)?;
        self.dealer_sockets.insert(dealer_identity, dealer_socket);
        Ok(())
    }

    /// Disconnect from a remote coordinator
    pub fn disconnect_from_coordinator(
        &mut self,
        dealer_identity: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.dealer_sockets.remove(dealer_identity);
        Ok(())
    }

    /// Iterator over dealer sockets
    pub fn dealer_sockets_iter(&self) -> impl Iterator<Item = (&Vec<u8>, &zmq::Socket)> {
        self.dealer_sockets.iter()
    }

    /// Poll for messages with a timeout
    ///
    /// # Parameters
    /// * `timeout_ms` - Timeout in milliseconds
    ///
    /// # Returns
    /// A vector of indices of readable sockets, or empty vector if no sockets are readable
    pub fn poll_messages(&self, timeout_ms: i64) -> Result<Vec<usize>, Box<dyn std::error::Error>> {
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

    /// Receive all available messages within a timeout
    fn receive_messages_impl(
        &self,
        timeout_ms: u64,
    ) -> Result<Vec<(Identity, MessageView)>, Box<dyn std::error::Error>> {
        let mut messages: Vec<(Identity, MessageView)> = Vec::new();

        // Poll for messages with a timeout
        let readable_indices = self.poll_messages(timeout_ms as i64)?;

        if !readable_indices.is_empty() {
            // Collect dealer identities first
            let dealer_identities: Vec<Vec<u8>> = self
                .dealer_sockets_iter()
                .map(|(identity, _)| identity.clone())
                .collect();

            // Process messages based on which sockets are readable
            // Router socket is first in poll_items (index 0)
            if readable_indices.contains(&0) {
                let (identity, message) = Self::receive_message_impl(&self.router_socket)?;
                messages.push((Identity::Local { identity }, message));
            }

            // Process dealer messages for readable sockets
            // Dealer sockets start from index 1 in poll_items
            for &index in &readable_indices {
                if index > 0 && index <= dealer_identities.len() {
                    let dealer_identity = &dealer_identities[index - 1];
                    let message = Self::receive_message_from_dealer_impl(
                        self.dealer_sockets.get(dealer_identity).unwrap(),
                    )?;
                    messages.push((
                        Identity::Remote {
                            identity: dealer_identity.clone(),
                        },
                        message,
                    ));
                }
            }
        }

        Ok(messages)
    }

    /// Send a message to a local component
    fn send_to_local_impl(
        &self,
        identity: &[u8],
        message: MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.router_socket.send(identity, zmq::SNDMORE)?;
        self.router_socket.send_multipart(message.raw_frames(), 0)?;
        Ok(())
    }

    /// Send a message to a remote coordinator
    fn send_to_remote_impl(
        &self,
        dealer_identity: &[u8],
        message: MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(socket) = self.dealer_sockets.get(dealer_identity) {
            socket.send_multipart(message.raw_frames(), 0)?;
            Ok(())
        } else {
            Err("Dealer socket not found".into())
        }
    }

    /// Receive a message from a socket, returning the identity and message
    fn receive_message_impl(
        socket: &zmq::Socket,
    ) -> Result<(Vec<u8>, MessageView), Box<dyn std::error::Error>> {
        let identity = socket.recv_bytes(0)?;
        let frames = socket.recv_multipart(0)?;
        let message = MessageView::new(frames)?;

        Ok((identity, message))
    }

    /// Receive a message from a dealer socket (no identity part)
    fn receive_message_from_dealer_impl(
        socket: &zmq::Socket,
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let frames = socket.recv_multipart(0)?;
        if frames.is_empty() {
            return Err("Invalid message format: no parts".into());
        }

        let message = MessageView::new(frames)?;

        Ok(message)
    }
}

impl MessageSenderPort for ZmqAdapter {
    fn send_to_local(
        &self,
        identity: &[u8],
        message: MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_to_local_impl(identity, message)
    }

    fn send_to_remote(
        &self,
        dealer_identity: &[u8],
        message: MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_to_remote_impl(dealer_identity, message)
    }
}

impl MessageReceiverPort for ZmqAdapter {
    fn receive_message_from_local(
        &self,
    ) -> Result<(Vec<u8>, MessageView), Box<dyn std::error::Error>> {
        Self::receive_message_impl(&self.router_socket)
    }

    fn receive_message_from_remote(
        &self,
        dealer_identity: &[u8],
    ) -> Result<MessageView, Box<dyn std::error::Error>> {
        let socket = self
            .dealer_sockets
            .get(dealer_identity)
            .ok_or_else(|| -> Box<dyn std::error::Error> { "Dealer socket not found".into() })?;
        Self::receive_message_from_dealer_impl(socket)
    }

    fn receive_messages(
        &self,
        timeout_ms: u64,
    ) -> Result<Vec<(Identity, MessageView)>, Box<dyn std::error::Error>> {
        self.receive_messages_impl(timeout_ms)
    }
}
