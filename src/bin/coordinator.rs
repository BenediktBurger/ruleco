//! # Coordinator
//!
//! Route messages between different Components in a LECO network

use std::{collections::HashMap, io, time::Instant};

use json::{is_sign_in, ErrorResponse, Request, Response};
use ruleco::{
    self,
    control_protocol::{Error, Message},
    core::FullName,
    json,
};
use serde::Serialize;
use zmq;

fn main() {
    let mut coordinator = Coordinator::new("R1".to_string(), None);
    coordinator.routing();
}

/// Combine a socket identity and a message
struct MessageContainer<T: zmq::Sendable> {
    identity: T,
    message: ruleco::control_protocol::Message,
}

struct Component {
    identity: Vec<u8>,
    timestamp: Instant,
}
impl Component {
    fn build(identity: &Vec<u8>) -> Self {
        Self {
            identity: identity.clone(),
            timestamp: Instant::now(),
        }
    }
}

// struct Nodes {
//     timestamps: HashMap<Vec<u8>, std::time::Instant>,
// }

struct Coordinator {
    namespace: Vec<u8>,
    full_name: Vec<u8>,
    router: zmq::Socket,
    components: HashMap<Vec<u8>, Component>,
    running: bool,
}

impl Coordinator {
    fn new(name: String, port: Option<u16>) -> Self {
        let ctx = zmq::Context::new();
        let router = ctx.socket(zmq::ROUTER).unwrap();
        let port = port.unwrap_or(12300);
        router.bind(&format!("tcp://*:{port}")).unwrap();
        let components = HashMap::new();
        let mut full_name = name.into_bytes();
        let name_len = full_name.len();
        full_name.extend_from_slice(b".COORDINATOR");
        let namespace = full_name[..name_len].to_vec();
        Self {
            namespace,
            router,
            components,
            full_name,
            running: false,
        }
    }

    fn routing(&mut self) {
        self.running = true;
        while self.running {
            let _ = self.loop_element();
        }
    }

    fn loop_element(&mut self) -> () {
        let msg_cont = match self.read_message() {
            Ok(msg_cont) => msg_cont,
            Err(_err) => return (),
        };
        let sender_name = msg_cont.message.sender();
        let receiver_name = msg_cont.message.receiver();
        println!("message read from {:?}", sender_name.name);
        let valid = self.check_message(&msg_cont, &sender_name, &receiver_name);
        match valid {
            Err(error) => self.send_local_error_message(
                &msg_cont.identity,
                msg_cont.message.sender_frame().to_vec(),
                error,
                Some(msg_cont.message.header().conversation_id),
            ),
            Ok(()) => {
                if receiver_name.namespace == self.namespace || receiver_name.namespace.len() == 0 {
                    self.handle_message(&msg_cont, &sender_name);
                } else {
                    self.send_message(msg_cont.message);
                }
            }
        }
    }

    fn read_message(&self) -> Result<MessageContainer<Vec<u8>>, io::Error> {
        let identity = self.router.recv_bytes(0)?;
        let frames = self.router.recv_multipart(0)?;
        let message = Message::new(frames)?;
        Ok(MessageContainer { identity, message })
    }

    fn check_message(
        &mut self,
        msg_cont: &MessageContainer<Vec<u8>>,
        sender_name: &FullName,
        receiver_name: &FullName,
    ) -> Result<(), Error> {
        let sender = sender_name.name;
        let component = self.components.get_mut(sender);
        match component {
            Some(component) => {
                if component.identity == msg_cont.identity {
                    component.timestamp = Instant::now();
                    Ok(())
                } else {
                    Err(Error::DuplicateName)
                }
            }
            None => {
                if receiver_name.name == b"COORDINATOR"
                    && is_sign_in(&msg_cont.message.content_frame().unwrap()[..])
                {
                    Ok(())
                } else {
                    Err(Error::NotSignedIn)
                }
            }
        }
    }

    fn send_error(&self, receiver: Vec<u8>, error: Error, conversation_id: Option<&[u8]>) {
        println!("Send error with number {}", error.code());
        let error_r = ErrorResponse::build(0, error.code(), error.message());
        let error_msg: Vec<u8> = serde_json::to_vec(&error_r).unwrap();
        let message = Message::build(
            receiver,
            self.full_name.clone(),
            conversation_id,
            None,
            1,
            ruleco::core::ContentTypes::Frame(error_msg),
        );
        self.send_message(message)
    }

    /// Send a message locally, especially if the receiver is not a valid member
    fn send_local_error_message(
        &self,
        identity: &Vec<u8>,
        receiver: Vec<u8>,
        error: Error,
        conversation_id: Option<&[u8]>,
    ) {
        println!("Send error with number {}", error.code());
        let error_r = ErrorResponse::build(0, error.code(), error.message());
        let error_msg: Vec<u8> = serde_json::to_vec(&error_r).unwrap();
        let message = Message::build(
            receiver,
            self.full_name.clone(),
            conversation_id,
            None,
            1,
            ruleco::core::ContentTypes::Frame(error_msg),
        );
        let msg_cont = MessageContainer { identity, message };
        self.send_local_message(msg_cont)
    }

    fn send_response(
        &self,
        receiver: Vec<u8>,
        id: u16,
        conversation_id: Option<&[u8]>,
        result: impl Serialize,
    ) {
        let response = Response::build(id, result);
        let response_msg: Vec<u8> = serde_json::to_vec(&response).unwrap();
        let message = Message::build(
            receiver,
            self.full_name.clone(),
            conversation_id,
            None,
            1,
            ruleco::core::ContentTypes::Frame(response_msg),
        );
        self.send_message(message)
    }

    fn send_local_message<T: zmq::Sendable>(&self, msg_cont: MessageContainer<T>) {
        self.router.send(msg_cont.identity, zmq::SNDMORE).unwrap();
        self.router
            .send_multipart(msg_cont.message.to_frames(), 0)
            .unwrap()
    }

    fn send_message(&self, message: Message) {
        println!("Send message");
        if let Some(component) = self.components.get(message.receiver().name) {
            self.send_local_message(MessageContainer {
                identity: &component.identity[..],
                message,
            })
        } else {
            println!("Cannot deliver message to unknown component")
        }
    }

    fn handle_message(&mut self, msg_cont: &MessageContainer<Vec<u8>>, sender_name: &FullName) {
        println!("handle message");
        let message = &msg_cont.message;
        let receiver = message.sender_frame().to_vec();
        let conversation_id: Option<&[u8]> = Some(message.header().conversation_id);
        let content = match message.content_frame() {
            Some(content) => content,
            None => return self.send_error(receiver, Error::ParseError, conversation_id),
        };
        let request = match serde_json::from_slice::<Request>(content) {
            Ok(request) => request,
            Err(_err) => return self.send_error(receiver, Error::ParseError, conversation_id),
        };
        let result: Result<Option<u8>, Error> = match &request.method[..] {
            "sign_in" => self.sign_in(&msg_cont.identity, sender_name),
            "sign_out" => self.sign_out(sender_name),
            "pong" => Ok(None),
            "shut_down" => self.shut_down(),
            _ => Err(Error::InvalidRequest),
        };
        match result {
            Ok(result) => self.send_response(receiver, request.id, conversation_id, result),
            Err(error) => self.send_error(receiver, error, conversation_id),
        }
    }

    fn sign_in<E>(&mut self, identity: &Vec<u8>, sender_name: &FullName) -> Result<Option<u8>, E> {
        self.components
            .insert(sender_name.name.to_vec(), Component::build(identity));
        Ok(None)
    }

    fn sign_out<E>(&mut self, sender_name: &FullName) -> Result<Option<u8>, E> {
        self.components.remove(&sender_name.name.to_vec());
        Ok(None)
    }

    /// Stop the coordinator's routing action
    fn shut_down<E>(&mut self) -> Result<Option<u8>, E> {
        self.running = false;
        Ok(None)
    }
}
#[cfg(test)]
mod test {
    use ruleco::control_protocol::communicator::Communicator;
    use serde_json::Value;

    use super::*;

    fn make_coordinator() -> Coordinator {
        // TODO make it close the router afterwards
        let mut c = Coordinator::new("N1".to_string(), Some(12345));
        c.components.insert(
            b"com_A".to_vec(),
            Component {
                identity: b"id_A".to_vec(),
                timestamp: Instant::now(),
            },
        );
        c.components.insert(
            b"com_B".to_vec(),
            Component {
                identity: b"id_B".to_vec(),
                timestamp: Instant::now(),
            },
        );
        c
    }

    fn make_message() -> Message {
        Message::build(
            b"receiver".to_vec(),
            b"sender".to_vec(),
            None,
            None,
            1,
            ruleco::core::ContentTypes::Null,
        )
    }

    #[test]
    fn test_check_message() -> Result<(), Error> {
        let mut c = make_coordinator();
        let msg_cont = MessageContainer {
            identity: b"id_A".to_vec(),
            message: make_message(),
        };
        let sender_name = FullName {
            namespace: b"",
            name: b"com_A",
        };
        let receiver_name = FullName {
            namespace: b"",
            name: b"com_B",
        };
        let result = c.check_message(&msg_cont, &sender_name, &receiver_name);
        result
    }
    #[test]
    fn test_check_message_not_signed_in() {
        let mut c = make_coordinator();
        let msg_cont = MessageContainer {
            identity: b"id_A".to_vec(),
            message: make_message(),
        };
        let sender_name = FullName {
            namespace: b"",
            name: b"com_C",
        };
        let receiver_name = FullName {
            namespace: b"",
            name: b"com_B",
        };
        let result = c.check_message(&msg_cont, &sender_name, &receiver_name);
        assert![result.is_err_and(|err| err == Error::NotSignedIn)]
    }

    #[test]
    fn test_with_communicator() {
        let comm = Communicator::build("comm", None, Some(12345));
        let mut coor = make_coordinator();
        comm.send_rpc_message("COORDINATOR".to_string(), "sign_in");
        println!("start loop");
        coor.loop_element();
        println!("loop stopped");
        if comm.poll(300) {
            let result = comm.read_rpc_message().unwrap();
            assert_eq!(result, Value::Null);
        } else {
            panic!("No response!")
        }
    }
}
