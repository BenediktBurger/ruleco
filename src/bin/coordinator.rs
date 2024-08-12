//! # Coordinator
//!
//! Route messages between different Components in a LECO network

use std::{collections::HashMap, io, time::Instant};

use json::is_sign_in;
use ruleco::{
    self,
    control_protocol::{Error, Message},
    core::FullName,
};
use zmq;

fn main() {
    let mut coordinator = Coordinator::new("abc".to_string(), 12345);
    coordinator.routing();
}

mod json {
    //! Do some json interpreting
    //! Replace later with proper crate, e.g. jsonrpsee
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct Request {
        jsonrpc: String,
        id: u16,
        method: String,
    }

    pub fn is_sign_in(slice: &[u8]) -> bool {
        match serde_json::from_slice::<Request>(slice) {
            Err(_) => false,
            Ok(request) => request.method == String::from("sign_in"),
        }
    }
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

struct Nodes {
    timestamps: HashMap<Vec<u8>, std::time::Instant>,
}

struct Coordinator {
    namespace: Vec<u8>,
    full_name: Vec<u8>,
    router: zmq::Socket,
    components: HashMap<Vec<u8>, Component>,
}

impl Coordinator {
    fn new(name: String, port: u16) -> Self {
        let ctx = zmq::Context::new();
        let router = ctx.socket(zmq::ROUTER).unwrap();
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
        }
    }
    fn routing(&mut self) {
        //do some loop
        self.loop_element();
    }
    fn loop_element(&mut self) -> Result<(), io::Error> {
        let msg_cont = match self.read_message() {
            Ok(msg_cont) => msg_cont,
            Err(err) => return Err(err),
        };
        let sender = msg_cont.message.sender();
        let receiver = msg_cont.message.receiver();
        let valid = self.check_message(&msg_cont, &sender, &receiver);
        match valid {
            Err(error) => self.send_error(msg_cont.message.sender_frame().to_vec(), error),
            Ok(()) => self.send_message(msg_cont.message),
        }
        // handle
        Ok(())
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

    fn create_error_message(error: Error) -> String {
        let code = error.code();
        format!(
            "\"jsonrpc\":\"2.0\",\"error\": {{\"code\"{code}, \"message\": \"Error happened\"}}"
        )
    }

    fn send_error(&self, receiver: Vec<u8>, error: Error) {
        let error_msg: Vec<u8> = Self::create_error_message(error).into_bytes();
        let message = Message::build(
            receiver,
            self.full_name.clone(),
            None,
            None,
            1,
            ruleco::core::ContentTypes::Frame(error_msg),
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
        if let Some(component) = self.components.get(message.receiver().name) {
            self.send_local_message(MessageContainer {
                identity: &component.identity[..],
                message,
            })
        }
    }

    
}
#[cfg(test)]
    mod test {
        use super::*;

        fn make_coordinator() -> Coordinator{
            // TODO make it close the router afterwards
            let mut c = Coordinator::new("N1".to_string(), 12345);
            c.components.insert(b"com_A".to_vec(), Component {identity: b"id_A".to_vec(), timestamp: Instant::now()});
            c.components.insert(b"com_B".to_vec(), Component {identity: b"id_B".to_vec(), timestamp: Instant::now()});
            c
        }

        fn make_message() -> Message {
            Message::build(b"receiver".to_vec(), b"sender".to_vec(), None, None, 1, ruleco::core::ContentTypes::Null)
        }

        #[test]
        fn test_check_message() -> Result<(), Error> {
            let mut c = make_coordinator();
            let msg_cont = MessageContainer {
                identity: b"id_A".to_vec(), message: make_message()
            };
            let sender_name = FullName{namespace: b"", name: b"com_A"};
            let receiver_name = FullName{namespace: b"", name: b"com_B"};
            c.check_message(&msg_cont, &sender_name, &receiver_name)
        }
        #[test]
        fn test_check_message_not_signed_in() {
            let mut c = make_coordinator();
            let msg_cont = MessageContainer {
                identity: b"id_A".to_vec(), message: make_message()
            };
            let sender_name = FullName{namespace: b"", name: b"com_C"};
            let receiver_name = FullName{namespace: b"", name: b"com_B"};
            assert![c.check_message(&msg_cont, &sender_name, &receiver_name).is_err_and(|err| err == Error::NotSignedIn)]
        }
    }