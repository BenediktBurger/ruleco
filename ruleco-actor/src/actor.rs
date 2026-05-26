use crate::handlers;
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use jsonrpsee_types::request::Request;
use jsonrpsee_types::Id;
use log::warn;
use ruleco_core::full_name::FullName;
use ruleco_core::message::{MessageBuilder, MessageView};
use ruleco_core::protocol_constants::MessageType;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

enum ActorCommand {
    Call {
        receiver: FullName,
        method: String,
        params: Option<Value>,
        response_tx: Sender<Result<Value>>,
    },
    Stop,
}

pub struct Actor {
    dealer: zmq::Socket,
    _context: zmq::Context,
    full_name: Option<FullName>,
    handlers: HashMap<String, handlers::MethodHandler>,
    action_handlers: HashMap<String, handlers::ActionHandler>,
    running: bool,
    parameters: HashMap<String, Value>,
    message_id_counter: u32,
    message_id_counter_arc: Arc<AtomicU32>,
}

pub struct ActorHandle {
    full_name: Arc<Mutex<Option<FullName>>>,
    command_tx: Sender<ActorCommand>,
    stop_flag: Arc<AtomicBool>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

struct ProcessedMessage {
    should_stop: bool,
    reply: Option<(FullName, Value)>,
}

impl Actor {
    pub fn connect(address: &str) -> Result<Self> {
        let context = zmq::Context::new();
        let dealer = context.socket(zmq::DEALER)?;
        let full_address = if address.starts_with("tcp://") {
            address.to_string()
        } else {
            format!("tcp://{address}")
        };
        dealer.connect(&full_address)?;

        Ok(Self {
            dealer,
            _context: context,
            full_name: None,
            handlers: HashMap::new(),
            action_handlers: HashMap::new(),
            running: false,
            parameters: HashMap::new(),
            message_id_counter: 1,
            message_id_counter_arc: Arc::new(AtomicU32::new(1)),
        })
    }

    pub fn sign_in(&mut self, name: &str) -> Result<FullName> {
        let sender = FullName::from_str(name)?;
        let receiver = FullName::from_slice(b"COORDINATOR")?;

        let id = self.next_id();
        let request = Request::borrowed("sign_in", None, Id::Number(id));

        let message = MessageBuilder::new()
            .receiver(receiver)
            .sender(sender)
            .payload_json(&request)?
            .build()?;

        let frames = message.to_frames();
        self.dealer.send_multipart(frames, 0)?;

        let response_frames = self.recv_multipart(5000)?;
        let view = MessageView::new(response_frames)?;
        let response_value: Value = view.payload_as_json_value()?;

        if response_value.get("error").is_some() {
            let error_msg = response_value
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown sign-in error");
            anyhow::bail!("Sign-in failed: {error_msg}");
        }

        let sender_fullname = view.sender().as_ref()
            .map_err(|e| anyhow::anyhow!("Invalid sender in sign-in response: {e}"))?
            .clone();
        let namespace = sender_fullname.namespace();
        if namespace.is_empty() {
            anyhow::bail!("Coordinator response missing namespace");
        }

        let full_name = FullName::new(namespace.to_vec(), name.as_bytes().to_vec());
        self.full_name = Some(full_name.clone());
        self.running = true;

        Ok(full_name)
    }

    pub fn sign_out(&mut self) -> Result<()> {
        let id = self.next_id();
        let full_name = self
            .full_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not signed in"))?
            .clone();

        let request = Request::borrowed("sign_out", None, Id::Number(id));

        let message = MessageBuilder::new()
            .receiver(FullName::from_slice(b"COORDINATOR")?)
            .sender(full_name)
            .payload_json(&request)?
            .build()?;

        let frames = message.to_frames();
        self.dealer.send_multipart(frames, 0)?;

        let response_frames = self.recv_multipart(5000)?;
        let view = MessageView::new(response_frames)?;
        let response_value: Value = view.payload_as_json_value()?;

        if response_value.get("error").is_some() {
            let error_msg = response_value
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown sign-out error");
            anyhow::bail!("Sign-out failed: {error_msg}");
        }

        self.full_name = None;
        self.running = false;
        Ok(())
    }

    pub fn register_method(&mut self, name: &str, handler: handlers::MethodHandler) {
        self.handlers.insert(name.to_string(), handler);
    }

    pub fn register_action(&mut self, name: &str, handler: handlers::ActionHandler) {
        self.action_handlers.insert(name.to_string(), handler);
    }

    pub fn full_name(&self) -> Option<&FullName> {
        self.full_name.as_ref()
    }

    pub fn call(
        &mut self,
        receiver: &FullName,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value> {
        let id = self.next_id();
        let full_name = self
            .full_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not signed in"))?
            .clone();

        let params_raw = params.map(|p| {
            serde_json::to_string(&p)
                .map(|s| serde_json::value::RawValue::from_string(s).unwrap())
                .unwrap()
        });
        let request = Request::owned(method.to_string(), params_raw, Id::Number(id));

        let message = MessageBuilder::new()
            .receiver(receiver.clone())
            .sender(full_name)
            .payload_json(&request)?
            .build()?;

        let frames = message.to_frames();
        self.dealer.send_multipart(frames, 0)?;

        let response_frames = self.recv_multipart(5000)?;
        let view = MessageView::new(response_frames)?;
        let response_value: Value = view.payload_as_json_value()?;

        if let Some(error) = response_value.get("error") {
            let error_msg = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            let error_code = error
                .get("code")
                .and_then(|c| c.as_i64())
                .unwrap_or(-1);
            anyhow::bail!("Call failed (code {error_code}): {error_msg}");
        }

        Ok(response_value)
    }

    pub fn step(&mut self, timeout_ms: i64) -> Result<bool> {
        if !self.running {
            return Ok(false);
        }

        let mut poll_items = vec![self.dealer.as_poll_item(zmq::POLLIN)];
        if zmq::poll(&mut poll_items, timeout_ms)? == 0 {
            return Ok(true);
        }

        let frames = self.recv_multipart(0)?;
        let view = match MessageView::new(frames) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse message: {e:?}");
                return Ok(true);
            }
        };

        self.handle_message(&view)?;
        Ok(true)
    }

    pub fn spawn(self) -> Result<ActorHandle> {
        let full_name = Arc::new(Mutex::new(self.full_name.clone()));
        let stop_flag = Arc::new(AtomicBool::new(false));

        let (command_tx, command_rx) = crossbeam_channel::bounded::<ActorCommand>(64);

        let full_name_clone = full_name.clone();
        let stop_flag_clone = stop_flag.clone();
        let message_id_counter = self.message_id_counter_arc.clone();

        let thread_handle = thread::spawn(move || {
            Self::event_loop(
                self.dealer,
                self.handlers,
                self.action_handlers,
                self.parameters,
                full_name_clone,
                stop_flag_clone,
                command_rx,
                message_id_counter,
            );
        });

        Ok(ActorHandle {
            full_name,
            command_tx,
            stop_flag,
            thread_handle: Some(thread_handle),
        })
    }

    fn event_loop(
        dealer: zmq::Socket,
        handlers: HashMap<String, handlers::MethodHandler>,
        action_handlers: HashMap<String, handlers::ActionHandler>,
        mut parameters: HashMap<String, Value>,
        full_name: Arc<Mutex<Option<FullName>>>,
        stop_flag: Arc<AtomicBool>,
        command_rx: Receiver<ActorCommand>,
        message_id_counter: Arc<AtomicU32>,
    ) {
        let mut running = true;

        while running && !stop_flag.load(Ordering::SeqCst) {
            while let Ok(cmd) = command_rx.try_recv() {
                match cmd {
                    ActorCommand::Call {
                        receiver,
                        method,
                        params,
                        response_tx,
                    } => {
                        let result = Self::send_call(
                            &dealer,
                            &full_name,
                            &receiver,
                            &method,
                            params,
                            &message_id_counter,
                        );
                        let _ = response_tx.send(result);
                    }
                    ActorCommand::Stop => {
                        running = false;
                        break;
                    }
                }
            }

            if !running {
                break;
            }

            let mut poll_items = vec![dealer.as_poll_item(zmq::POLLIN)];
            if zmq::poll(&mut poll_items, 100).unwrap_or(0) == 0 {
                continue;
            }

            let frames = match dealer.recv_multipart(0) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let view = match MessageView::new(frames) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Err(e) = Self::handle_message_static(
                &dealer,
                &view,
                &handlers,
                &action_handlers,
                &mut parameters,
                &full_name,
                &mut running,
            ) {
                warn!("Error handling message in spawned actor: {e}");
            }
        }
    }

    fn send_call(
        dealer: &zmq::Socket,
        full_name: &Arc<Mutex<Option<FullName>>>,
        receiver: &FullName,
        method: &str,
        params: Option<Value>,
        message_id_counter: &AtomicU32,
    ) -> Result<Value> {
        let my_name = full_name
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not signed in"))?
            .clone();

        let id = Id::Number(message_id_counter.fetch_add(1, Ordering::SeqCst) as u64);

        let params_raw = params.map(|p| {
            serde_json::to_string(&p)
                .map(|s| serde_json::value::RawValue::from_string(s).unwrap())
                .unwrap()
        });
        let request = Request::owned(method.to_string(), params_raw, id);

        let message = MessageBuilder::new()
            .receiver(receiver.clone())
            .sender(my_name)
            .payload_json(&request)?
            .build()?;

        dealer.send_multipart(message.to_frames(), 0)?;

        let response_frames = {
            let mut poll_items = vec![dealer.as_poll_item(zmq::POLLIN)];
            if zmq::poll(&mut poll_items, 5000)? == 0 {
                anyhow::bail!("Timeout waiting for response");
            }
            dealer.recv_multipart(0)?
        };

        let view = MessageView::new(response_frames)?;
        let response_value: Value = view.payload_as_json_value()?;

        if let Some(error) = response_value.get("error") {
            let error_msg = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("Call failed: {error_msg}");
        }

        Ok(response_value)
    }

    fn process_incoming_message(
        view: &MessageView,
        handlers: &HashMap<String, handlers::MethodHandler>,
        action_handlers: &HashMap<String, handlers::ActionHandler>,
        parameters: &mut HashMap<String, Value>,
    ) -> Result<ProcessedMessage> {
        let content = match view.content_frame() {
            Some(c) => c,
            None => return Ok(ProcessedMessage { should_stop: false, reply: None }),
        };

        if view.header().message_type_enum() != MessageType::Json {
            return Ok(ProcessedMessage { should_stop: false, reply: None });
        }

        let json_value: Value = serde_json::from_slice(content)?;

        if json_value.get("method").is_none() {
            return Ok(ProcessedMessage { should_stop: false, reply: None });
        }

        let method = json_value
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let params = json_value.get("params").cloned();
        let id = Self::extract_id(&json_value);

        let should_stop = method == "shut_down";

        let response = handlers::dispatch_request(
            &method,
            params,
            id,
            handlers,
            action_handlers,
            parameters,
        );

        let reply = if let Some(resp_value) = response {
            if let Ok(sender) = view.sender() {
                Some((sender.clone(), resp_value))
            } else {
                None
            }
        } else {
            None
        };

        Ok(ProcessedMessage { should_stop, reply })
    }

    fn handle_message(&mut self, view: &MessageView) -> Result<()> {
        let result = Self::process_incoming_message(
            view,
            &self.handlers,
            &self.action_handlers,
            &mut self.parameters,
        )?;

        if result.should_stop {
            self.running = false;
        }

        if let Some((sender, resp_value)) = result.reply {
            self.send_response(&sender, view, &resp_value)?;
        }

        Ok(())
    }

    fn handle_message_static(
        dealer: &zmq::Socket,
        view: &MessageView,
        handlers: &HashMap<String, handlers::MethodHandler>,
        action_handlers: &HashMap<String, handlers::ActionHandler>,
        parameters: &mut HashMap<String, Value>,
        full_name: &Arc<Mutex<Option<FullName>>>,
        running: &mut bool,
    ) -> Result<()> {
        let result = Self::process_incoming_message(
            view,
            handlers,
            action_handlers,
            parameters,
        )?;

        if result.should_stop {
            *running = false;
        }

        if let Some((sender, resp_value)) = result.reply {
            let full_name_guard = full_name.lock().unwrap();
            if let Some(ref my_name) = *full_name_guard {
                let msg = MessageBuilder::new()
                    .receiver(sender)
                    .sender(my_name.clone())
                    .conversation_id(view.header().conversation_id.clone())
                    .payload_json(&resp_value)?
                    .build()?;

                dealer.send_multipart(msg.to_frames(), 0)?;
            }
        }

        Ok(())
    }

    fn extract_id(json_value: &Value) -> Id {
        match json_value.get("id") {
            Some(Value::Number(n)) => Id::Number(n.as_u64().unwrap_or(0)),
            Some(Value::String(s)) => Id::Str(s.clone().into()),
            Some(Value::Null) | None => Id::Null,
            _ => Id::Null,
        }
    }

    fn send_response(
        &self,
        recipient: &FullName,
        original_view: &MessageView,
        response_value: &Value,
    ) -> Result<()> {
        let my_name = self
            .full_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not signed in"))?;

        let message = MessageBuilder::new()
            .receiver(recipient.clone())
            .sender(my_name.clone())
            .conversation_id(original_view.header().conversation_id.clone())
            .payload_json(response_value)?
            .build()?;

        let frames = message.to_frames();
        self.dealer.send_multipart(frames, 0)?;
        Ok(())
    }

    fn next_id(&mut self) -> u64 {
        let id = self.message_id_counter as u64;
        self.message_id_counter += 1;
        id
    }

    fn recv_multipart(&self, timeout_ms: i64) -> Result<Vec<Vec<u8>>> {
        if timeout_ms > 0 {
            let mut poll_items = vec![self.dealer.as_poll_item(zmq::POLLIN)];
            if zmq::poll(&mut poll_items, timeout_ms)? == 0 {
                anyhow::bail!("Timeout waiting for response");
            }
        }
        Ok(self.dealer.recv_multipart(0)?)
    }
}

impl ActorHandle {
    pub fn call(
        &self,
        receiver: &FullName,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value> {
        let (response_tx, response_rx) = crossbeam_channel::bounded::<Result<Value>>(1);

        self.command_tx.send(ActorCommand::Call {
            receiver: receiver.clone(),
            method: method.to_string(),
            params,
            response_tx,
        })?;

        response_rx.recv()?
    }

    pub fn is_running(&self) -> bool {
        if let Some(ref thread) = self.thread_handle {
            !thread.is_finished() && !self.stop_flag.load(Ordering::SeqCst)
        } else {
            false
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        let _ = self.command_tx.send(ActorCommand::Stop);
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread_handle.take() {
            let _ = thread.join();
        }
        Ok(())
    }

    pub fn full_name(&self) -> Option<FullName> {
        self.full_name.lock().unwrap().clone()
    }

    pub fn sign_out(&mut self) -> Result<()> {
        self.full_name
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not signed in"))?;

        let (response_tx, response_rx) = crossbeam_channel::bounded::<Result<Value>>(1);

        self.command_tx.send(ActorCommand::Call {
            receiver: FullName::from_slice(b"COORDINATOR")?,
            method: "sign_out".to_string(),
            params: None,
            response_tx,
        })?;

        let response = response_rx.recv()??;
        if let Some(error) = response.get("error") {
            let error_msg = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown sign-out error");
            anyhow::bail!("Sign-out failed: {error_msg}");
        }

        // Note: There is a small race window where the event loop may process
        // one more incoming message between the sign_out response arriving and
        // the stop_flag being checked. This is benign since the actor is disconnecting.

        self.stop_flag.store(true, Ordering::SeqCst);
        let _ = self.command_tx.send(ActorCommand::Stop);

        if let Some(thread) = self.thread_handle.take() {
            let _ = thread.join();
        }

        let mut full_name_guard = self.full_name.lock().unwrap();
        *full_name_guard = None;

        Ok(())
    }
}
