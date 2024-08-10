//! # Coordinator
//!
//! Route messages between different Components in a LECO network

use ruleco::{self, control_protocol::Message};
use zmq;

fn main() {
    let coordinator = Coordinator::new("abc".to_string(), 12345);
    coordinator.routing();
}

struct MessageContainer {
    identity: Vec<u8>,
    message: ruleco::control_protocol::Message,
}

struct Coordinator {
    name: String,
    router: zmq::Socket,
}

impl Coordinator {
    fn new(name: String, port: u16) -> Self {
        let ctx = zmq::Context::new();
        let router = ctx.socket(zmq::ROUTER).unwrap();
        router.bind(&format!("tcp://*:{port}")).unwrap();
        Self { name, router }
    }
    fn routing(&self) {
        //do some loop
        self.loop_element()
    }
    fn loop_element(&self) {
        let msg = self.read_message();
        // check validity
        // handle
    }

    fn read_message(&self) -> MessageContainer {
        let frames = self.router.recv_multipart(0).unwrap();
        let message = Message::new(frames[1..].to_vec()).unwrap();
        MessageContainer{identity: frames[0].clone(), message}
    }
}
