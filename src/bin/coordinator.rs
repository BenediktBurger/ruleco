//! # Coordinator
//!
//! Route messages between different Components in a LECO network

use std::{
    collections::HashMap,
    io,
    time::{Duration, Instant},
};

use json::{is_sign_in, ErrorResponse, Request, Response};
use ruleco::{
    self,
    control_protocol::{Error, Message},
    core::FullName,
    json::{self, to_vec},
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

/// Combine sending socket information with a message
struct SendingContainer<T: zmq::Sendable> {
    receiving_namespace: Vec<u8>,
    msg_cont: MessageContainer<T>,
}
// TODO maybe combine with MessageContainer?

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
    /// Create a new Coordinator.
    ///
    /// For a port number of 0, it won't bind to any port at all!
    fn new(name: String, port: Option<u16>) -> Self {
        let ctx = zmq::Context::new();
        let router = ctx.socket(zmq::ROUTER).unwrap();
        let port = port.unwrap_or(12300);
        if port != 0 {
            router.bind(&format!("tcp://*:{port}")).unwrap();
        }
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

    /// Start a continuous loop routing messages.
    fn routing(&mut self) {
        self.running = true;
        while self.running {
            let _ = self.loop_element();
        }
        // TODO move somehow in loop
        self.check_timeouts();
    }

    fn loop_element(&mut self) -> () {
        let msg_cont = match self.read_message() {
            Ok(msg_cont) => msg_cont,
            Err(_err) => return (),
        };
        match self.route_message(msg_cont) {
            Some(s_m_c) => self.send_routed_message(s_m_c),
            _ => (),
        }
    }

    fn read_message(&self) -> Result<MessageContainer<Vec<u8>>, io::Error> {
        let identity = self.router.recv_bytes(0)?;
        let frames = self.router.recv_multipart(0)?;
        let message = Message::new(frames)?;
        Ok(MessageContainer { identity, message })
    }

    /// Take a MessageContainer and handle it until it is ready to be sent.
    ///
    /// This method does everything short of reading and sending a message.
    fn route_message(
        &mut self,
        msg_cont: MessageContainer<Vec<u8>>,
    ) -> Option<SendingContainer<Vec<u8>>> {
        let identity = msg_cont.identity;
        let mut message = msg_cont.message;
        let sender_name = message.sender();
        let mut receiver_name = message.receiver();
        println!("message read from {:?}", sender_name.name);
        let valid = self.check_message(&identity, &message, &sender_name, &receiver_name);
        match valid {
            Err(error) => {
                let message = self.create_error(
                    message.sender_frame().to_vec(),
                    error,
                    Some(message.header().conversation_id),
                );
                return Some(SendingContainer {
                    receiving_namespace: Vec::new(),
                    msg_cont: MessageContainer { identity, message },
                });
            }
            Ok(()) => {
                if receiver_name.name == b"COORDINATOR"
                    && (receiver_name.namespace == self.namespace
                        || receiver_name.namespace.len() == 0)
                {
                    message = self.handle_message_content(&message, &sender_name);
                    // find somehow the routing stuff
                    receiver_name = message.receiver();
                }
                match self.find_routing_information(&receiver_name) {
                    Err(error) => {
                        let message = self.create_error(
                            message.receiver_frame().to_vec(),
                            error,
                            Some(message.header().conversation_id),
                        );
                        match self.find_routing_information(&message.receiver()) {
                            Err(_err) => {
                                println!("Could not send 'receiver not found' to original sender.");
                                None
                            }
                            Ok((namespace, identity)) => Some(SendingContainer {
                                receiving_namespace: namespace,
                                msg_cont: MessageContainer { identity, message },
                            }),
                        }
                    }
                    Ok((namespace, identity)) => Some(SendingContainer {
                        receiving_namespace: namespace,
                        msg_cont: MessageContainer { identity, message },
                    }),
                }
            }
        }
    }

    /// Find the correct namespace and identity of the receiver or raise an error.
    fn find_routing_information(
        &self,
        receiver_name: &FullName,
    ) -> Result<(Vec<u8>, Vec<u8>), Error> {
        if receiver_name.namespace == self.namespace || receiver_name.namespace.len() == 0 {
            match self.components.get(receiver_name.name) {
                Some(comp) => Ok((Vec::new(), comp.identity.clone())),
                None => Err(Error::ReceiverUnknown),
            }
        } else {
            // TODO add here the remote node.
            Err(Error::NodeUnknown)
        }
    }

    /// Send a message once valid receiver information has been found
    fn send_routed_message<T: zmq::Sendable>(&self, s_cont: SendingContainer<T>) {
        if s_cont.receiving_namespace.len() == 0 {
            self.send_local_message(s_cont.msg_cont)
        } // else send to other namespaces
    }

    /// Check whether the message is from a signed_in Component or signing in.
    fn check_message(
        &mut self,
        identity: &Vec<u8>,
        message: &Message,
        sender_name: &FullName,
        receiver_name: &FullName,
    ) -> Result<(), Error> {
        let sender = sender_name.name;
        let component = self.components.get_mut(sender);
        match component {
            Some(component) => {
                if component.identity == *identity {
                    component.timestamp = Instant::now();
                    Ok(())
                } else {
                    Err(Error::DuplicateName)
                }
            }
            None => {
                if receiver_name.name == b"COORDINATOR"
                    && is_sign_in(&message.content_frame().unwrap()[..])
                {
                    self.sign_in(identity, sender_name)
                } else {
                    Err(Error::NotSignedIn)
                }
            }
        }
    }

    fn send_local_ping(&self, identity: &Vec<u8>, name: &Vec<u8>) {
        let rq = Request::build(0, "pong");
        let message = Message::build(
            name.to_vec(),
            self.full_name.clone(),
            None,
            None,
            1,
            ruleco::core::ContentTypes::Frame(to_vec(&rq)),
        );
        let msg_cont = MessageContainer { identity, message };
        self.send_local_message(msg_cont);
    }

    fn check_timeouts(&mut self) {
        for (k, v) in self.components.iter() {
            if v.timestamp.elapsed() >= Duration::from_secs(10) {
                self.send_local_ping(&v.identity, k);
            }
        }
        self.components
            .retain(|_, comp: &mut Component| comp.timestamp.elapsed() <= Duration::from_secs(30));
    }

    fn create_error(
        &self,
        receiver: Vec<u8>,
        error: Error,
        conversation_id: Option<&[u8]>,
    ) -> Message {
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
        message
    }

    fn create_response(
        &self,
        receiver: Vec<u8>,
        id: u16,
        conversation_id: Option<&[u8]>,
        result: impl Serialize,
    ) -> Message {
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
        message
    }

    fn send_local_message<T: zmq::Sendable>(&self, msg_cont: MessageContainer<T>) {
        self.router.send(msg_cont.identity, zmq::SNDMORE).unwrap();
        self.router
            .send_multipart(msg_cont.message.to_frames(), 0)
            .unwrap()
    }

    /// Handle the content of a message which is directed to this Coordinator itself.
    fn handle_message_content(&mut self, message: &Message, sender_name: &FullName) -> Message {
        println!("handle message");
        let receiver = message.sender_frame().to_vec();
        let conversation_id: Option<&[u8]> = Some(message.header().conversation_id);
        let content = match message.content_frame() {
            Some(content) => content,
            None => return self.create_error(receiver, Error::ParseError, conversation_id),
        };
        let request = match serde_json::from_slice::<Request>(content) {
            Ok(request) => request,
            Err(_err) => return self.create_error(receiver, Error::ParseError, conversation_id),
        };
        let result: Result<Option<u8>, Error> = match &request.method[..] {
            "sign_in" => Ok(None), // already handled during check_message
            "sign_out" => self.sign_out(sender_name),
            "pong" => Ok(None),
            "shut_down" => self.shut_down(),
            _ => Err(Error::InvalidRequest),
        };
        match result {
            Ok(result) => self.create_response(receiver, request.id, conversation_id, result),
            Err(error) => self.create_error(receiver, error, conversation_id),
        }
    }

    fn sign_in<E>(&mut self, identity: &Vec<u8>, sender_name: &FullName) -> Result<(), E> {
        self.components
            .insert(sender_name.name.to_vec(), Component::build(identity));
        Ok(())
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

    fn make_coordinator_with_port(port: u16) -> Coordinator {
        // TODO make it close the router afterwards
        let mut c = Coordinator::new("N1".to_string(), Some(port));
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

    /// Make a Coordinator bound to a certain port number.
    fn make_live_coordinator() -> Coordinator {
        make_coordinator_with_port(12345)
    }

    /// Make a Coordinator without binding to a port lest the port is already bound
    fn make_coordinator() -> Coordinator {
        make_coordinator_with_port(0)
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
    fn test_find_routing_local_without_namespace() {
        let c = make_coordinator();
        let r = c.find_routing_information(&FullName::from_slice(b"com_A").unwrap());
        assert_eq!(r, Ok((b"".to_vec(), b"id_A".to_vec())))
    }

    #[test]
    fn test_find_routing_local_without_namespace_fails() {
        let c = make_coordinator();
        let r = c.find_routing_information(&FullName::from_slice(b"com_X").unwrap());
        assert_eq!(r, Err(Error::ReceiverUnknown))
    }

    #[test]
    fn test_find_routing_local_with_namespace() {
        let c = make_coordinator();
        let r = c.find_routing_information(&FullName::from_slice(b"N1.com_B").unwrap());
        assert_eq!(r, Ok((b"".to_vec(), b"id_B".to_vec())))
    }

    #[test]
    fn test_find_routing_local_with_namespace_fails() {
        let c = make_coordinator();
        let r = c.find_routing_information(&FullName::from_slice(b"N1.com_X").unwrap());
        assert_eq!(r, Err(Error::ReceiverUnknown))
    }

    #[test]
    fn test_find_routing_unknown_namespace_fails() {
        let c = make_coordinator();
        let r = c.find_routing_information(&FullName::from_slice(b"NX.com_B").unwrap());
        assert_eq!(r, Err(Error::NodeUnknown))
    }

    #[test]
    fn test_route_message() {
        let mut c = make_coordinator();
        let message = Message::build(
            b"com_B".to_vec(),
            b"com_A".to_vec(),
            None,
            None,
            1,
            ruleco::core::ContentTypes::Null,
        );
        let scm = c
            .route_message(MessageContainer {
                identity: b"id_A".to_vec(),
                message: message.clone(),
            })
            .unwrap();
        assert_eq!(scm.msg_cont.message.to_frames(), message.to_frames());
        assert_eq!(scm.msg_cont.identity, b"id_B")
    }

    #[test]
    fn test_route_message_ping() {
        let mut c = make_coordinator();
        let request = Request::build(1, "pong");
        let response = Response::build(1, None::<()>);
        let message = Message::build(
            b"COORDINATOR".to_vec(),
            b"N1.com_A".to_vec(),
            None,
            None,
            1,
            ruleco::core::ContentTypes::Frame(to_vec(&request)),
        );
        let scm = c
            .route_message(MessageContainer {
                identity: b"id_A".to_vec(),
                message: message,
            })
            .unwrap();
        assert_eq!(scm.receiving_namespace, b"".to_vec());
        assert_eq!(scm.msg_cont.identity, b"id_A".to_vec());
        let m2 = scm.msg_cont.message;
        assert_eq!(m2.content_frame().unwrap(), &to_vec(&response))
    }

    #[test]
    fn test_check_message() -> Result<(), Error> {
        let mut c = make_coordinator();
        let identity = b"id_A".to_vec();
        let message = make_message();
        let sender_name = FullName {
            namespace: b"",
            name: b"com_A",
        };
        let receiver_name = FullName {
            namespace: b"",
            name: b"com_B",
        };
        let result = c.check_message(&identity, &message, &sender_name, &receiver_name);
        result
    }
    #[test]
    fn test_check_message_not_signed_in() {
        let mut c = make_coordinator();
        let identity = b"id_A".to_vec();
        let message = make_message();
        let sender_name = FullName {
            namespace: b"",
            name: b"com_C",
        };
        let receiver_name = FullName {
            namespace: b"",
            name: b"com_B",
        };
        let result = c.check_message(&identity, &message, &sender_name, &receiver_name);
        assert![result.is_err_and(|err| err == Error::NotSignedIn)]
    }

    #[test]
    fn test_with_communicator() {
        let comm = Communicator::build("comm", None, Some(12345));
        let mut coor = make_live_coordinator();
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
