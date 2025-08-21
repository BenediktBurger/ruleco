use crate::core::ports::MessageSenderPort;
use ruleco_core::message::MessageView;
use zmq;

/// ZMQ implementation of the message sender port
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

    /// Get a reference to the router socket for receiving messages
    pub fn router_socket(&self) -> &zmq::Socket {
        &self.router_socket
    }

    /// Get a reference to a dealer socket
    pub fn get_dealer_socket(
        &self,
        dealer_identity: &[u8],
    ) -> Result<&zmq::Socket, Box<dyn std::error::Error>> {
        self.dealer_sockets
            .get(dealer_identity)
            .ok_or_else(|| "Dealer socket not found".into())
    }

    /// Iterator over dealer sockets
    pub fn dealer_sockets_iter(&self) -> impl Iterator<Item = (&Vec<u8>, &zmq::Socket)> {
        self.dealer_sockets.iter()
    }

    /// Send a message to a local component
    fn send_to_local_impl(
        &self,
        identity: &[u8],
        message: &MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.router_socket.send(identity, zmq::SNDMORE)?;
        self.router_socket.send_multipart(message.raw_frames(), 0)?;
        Ok(())
    }

    /// Send a message to a remote coordinator
    fn send_to_remote_impl(
        &self,
        dealer_identity: &[u8],
        message: &MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(socket) = self.dealer_sockets.get(dealer_identity) {
            socket.send_multipart(message.raw_frames(), 0)?;
            Ok(())
        } else {
            Err("Dealer socket not found".into())
        }
    }
}

impl MessageSenderPort for ZmqAdapter {
    fn send_to_local(
        &self,
        identity: &[u8],
        message: &MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_to_local_impl(identity, message)
    }

    fn send_to_remote(
        &self,
        dealer_identity: &[u8],
        message: &MessageView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_to_remote_impl(dealer_identity, message)
    }
}
