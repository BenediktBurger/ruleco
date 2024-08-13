//! Helper utility to communicate
//!
//!
use crate::{
    core::FullName,
    json::{to_vec, Request, Response},
};
use serde_json::Error;
use zmq;

use super::Message;

pub struct Communicator {
    name: Vec<u8>,
    full_name: Vec<u8>,
    socket: zmq::Socket,
}
impl Communicator {
    pub fn build(name: &str, host: Option<&str>, port: Option<u16>) -> Self {
        Self {
            name: name.as_bytes().to_vec(),
            full_name: name.as_bytes().to_vec(),
            socket: Self::create_socket(host, port),
        }
    }

    pub fn create_socket(host: Option<&str>, port: Option<u16>) -> zmq::Socket {
        let ctx = zmq::Context::new();
        let socket = ctx.socket(zmq::DEALER).unwrap();
        let host: &str = host.unwrap_or("localhost");
        let port = port.unwrap_or(12300);
        socket.connect(&format!("tcp://{host}:{port}")).unwrap();
        socket
    }

    pub fn send_message(&self, message: Message) {
        let _ = self.socket.send_multipart(message.frames, 0);
    }

    /// Poll whether a new message arrived
    pub fn poll(&self, timeout_ms: i64) -> bool {
        self.socket.poll(zmq::POLLIN, timeout_ms).unwrap() == 1
    }
    pub fn read_message(&self) -> Message {
        let frames = self.socket.recv_multipart(0).unwrap();
        Message::new(frames).unwrap()
    }

    pub fn send_rpc_message<T: ToString>(&self, receiver: String, method: T) -> Vec<u8> {
        let request_content = Request::build(0, method);
        let request = Message::build(
            receiver.into_bytes(),
            self.name.to_vec(),
            None,
            None,
            1,
            crate::core::ContentTypes::Frame(to_vec(&request_content)),
        );
        let cid = request.header().conversation_id.to_vec();
        self.send_message(request);
        cid
    }

    pub fn read_rpc_message(&self) -> Result<serde_json::Value, Error> {
        let response = self.read_message();
        match serde_json::from_slice::<Response>(response.content_frame().unwrap_or(&vec![])) {
            Ok(response) => Ok(response.result),
            Err(err) => Err(err),
        }
    }

    pub fn sign_in(&mut self) {
        self.send_rpc_message("COORDINATOR".to_string(), "sign_in");
        let response = self.read_message();
        match serde_json::from_slice::<Response>(response.content_frame().unwrap_or(&vec![])) {
            Ok(_response) => self.finish_sign_in(response.sender()),
            Err(_err) => (),
        }
    }
    fn finish_sign_in(&mut self, coordinator_name: FullName) {
        let mut full_name: Vec<u8> = coordinator_name.namespace.to_vec();
        full_name.push(46);
        full_name.extend(&self.name);
        self.full_name = full_name
    }

    pub fn sign_out(&mut self) {
        self.send_rpc_message("COORDINATOR".to_string(), "sign_out");
        let _response = self.read_message();
        self.full_name = self.name.clone()
    }

    pub fn ping(&self, receiver: String) {
        self.send_rpc_message(receiver, "pong");
    }
}
