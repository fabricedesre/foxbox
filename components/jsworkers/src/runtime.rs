// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Manages launching the runtime and relaying messages.

extern crate url;
extern crate hyper;

use bitsparrow::{Decoder, Encoder};
use foxbox_core::broker::SharedBroker;
use foxbox_core::managed_process::ManagedProcess;
use foxbox_core::jsworkers::{BrowserMessageKind, Message as BrokerMessage, WorkerInfoKey};
use iron::Headers;
use self::hyper::status::StatusCode;
use self::url::Url;
use serde::Serialize;
use serde_json;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt::Debug;
use std::io::Error as IoError;
use std::ops::Deref;
use std::process::Command;
use std::sync::mpsc::{channel, Sender as MpscSender};
use std::thread;
use std::thread::Builder;
use uuid::Uuid;
use workers::JsWorkers;
use ws;
use ws::{Handler, listen, Sender, Result as WsResult, Message, Handshake, CloseCode, Error, ErrorKind};

#[derive(Clone, Copy, PartialEq)]
enum ClientType {
    Unknown,
    Runtime,
    Browser,
}

type HandlerId = String;

/// Handles the jsworkers websocket server.
// TODO: split that in two different handlers for the runtime and browser ws connections?
struct RuntimeWsHandler {
    pub out: Sender,
    broker: SharedBroker<BrokerMessage>,
    mode: Cell<ClientType>,
    worker_id: RefCell<Option<WorkerInfoKey>>, // For browser ws, this is the WorkerInfo key.
    handler_id: HandlerId, // A unique id for this handler, used to keep track of handlers shutdown.
}

impl RuntimeWsHandler {
    fn new(out: Sender, broker: SharedBroker<BrokerMessage>) -> Self {
        RuntimeWsHandler {
            out: out,
            broker: broker,
            mode: Cell::new(ClientType::Unknown),
            worker_id: RefCell::new(None),
            handler_id: format!("{}", Uuid::new_v4()),
        }
    }

    fn close_with_error(&mut self, reason: &'static str) -> WsResult<()> {
        error!("Closing jsworkers ws: {}", reason);
        self.out.close_with_reason(ws::CloseCode::Error, reason)
    }

    fn is_runtime(&self) -> bool {
        self.mode.get() == ClientType::Runtime
    }

    fn is_browser(&self) -> bool {
        self.mode.get() == ClientType::Browser
    }

    fn is_unknown(&self) -> bool {
        self.mode.get() == ClientType::Unknown
    }
}

impl Handler for RuntimeWsHandler {
    fn on_open(&mut self, handshake: Handshake) -> WsResult<()> {
        let resource = &handshake.request.resource()[..];

        // creating a fake url to get the path and query parsed
        let url = match Url::parse(&format!("http://localhost{}", resource)) {
            Ok(val) => val,
            _ => return self.close_with_error("Invalid path"),
        };

        info!("Opening jsworkers ws: url is {}, resource is {}",
              url,
              resource);

        if resource == "/runtime/" {
            self.mode.set(ClientType::Runtime);
            let mut guard = self.broker.lock().unwrap();
            guard.send_message("workers", BrokerMessage::RunnerWSOpened { out: self.out.clone() });
        }

        // Client urls are "/client/:id" so we extract the id here.
        if resource.starts_with("/client/") {
            let id = resource.split('/').last().unwrap();
            *self.worker_id.borrow_mut() = Some(id.to_string());
            info!("WS browser connection for id {}", id);
            self.mode.set(ClientType::Browser);
            let mut guard = self.broker.lock().unwrap();
            guard.send_message("workers",
                               BrokerMessage::BrowserWSOpened {
                                   out: self.out.clone(),
                                   worker_id: id.to_string(),
                                   handler_id: self.handler_id.clone(),
                               });
        }

        // Wrong resource path, rejecting connection.
        if self.is_unknown() {
            return Err(Error::new(ErrorKind::Internal, "Unknown resource path"));
        }

        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> WsResult<()> {
        if self.is_runtime() {
            debug!("Message from js runner ws: {}", msg);

            // Relay the payload to the right browser ws.
            let mut decoder = Decoder::new(msg.into_data());

            // Helper macro to decode & unwrap a given data of a given type.
            macro_rules! decode {
                ($name:ident, $kind:ident) => (
                    let $name = decoder.$kind();
                    if $name.is_err() {
                        error!("Unable to decode `$name` in runtime web socket message: {:?}", $name.err());
                        return Ok(());
                    }
                    let $name = $name.unwrap();
                );
            }

            decode!(id, string);
            decode!(kind, string);

            // Service worker registration result: we don't relay it to the
            // client websocket, but use it as an http response.
            if kind == "sw-registration-result" {
                decode!(json, string);

                if !decoder.end() {
                    error!("Unexpected data after `json` in sw-registration-result web socket message.");
                    return Ok(());
                }

                #[derive(Deserialize)]
                struct RegistrationResult {
                    success: bool,
                    error: Option<String>,
                }
                let result: Result<RegistrationResult, serde_json::Error> = serde_json::from_str(&json);
                if result.is_err() {
                    error!("Error decoded json string `{}` : {:?}`", json, result.err());
                    return Ok(());
                }
                let result = result.unwrap();

                // Message decoding is ok, let's forward it to the right WS.
                let mut guard = self.broker.lock().unwrap();
                guard.send_message("workers",
                                   BrokerMessage::RegisterResult {
                                       id: id.clone(),
                                       success: result.success,
                                       error: result.error.unwrap_or("".to_owned()),
                                   });
                return Ok(());
            }

            // Service worker request result: we don't relay it to the
            // client websocket, but use it as an http response.
            if kind == "user-request-response" {
                decode!(status, uint16);
                let status = StatusCode::from_u16(status);

                decode!(header_count, uint32);
                let mut headers = Headers::new();
                for _ in 0..header_count {
                    decode!(hname, string);
                    decode!(hvalue, string);
                    debug!("Header {} : {}", hname.clone(), hvalue.clone());
                    // Remove encoding headers since we don't re-encode the body.
                    if hname == "content-encoding" ||
                       hname == "transfer-encoding" {
                        continue;
                    }

                    // TODO: deal with multiple headers with the same name.
                    headers.set_raw(hname, vec![hvalue.into_bytes()]);
                }

                decode!(body, bytes);

                if !decoder.end() {
                    error!("Unexpected data after `json` in sw-registration-result web socket message.");
                    return Ok(());
                }

                // Message decoding is ok, let's forward it to the right WS.
                let mut guard = self.broker.lock().unwrap();
                guard.send_message("workers",
                                   BrokerMessage::UserResourceResponse {
                                       id: id.clone(),
                                       status: status,
                                       headers: headers,
                                       body: body,
                                   });
                return Ok(());
            }

            let e_kind: BrowserMessageKind = {
                if kind == "message" {
                    BrowserMessageKind::Message
                } else if kind == "error" {
                    BrowserMessageKind::Error
                } else {
                    error!("Unexpected `kind` value in runtime web socket message: {}", kind);
                    return Ok(());
                }
            };

            decode!(payload, bytes);

            if !decoder.end() {
                error!("Unexpected data after `payload` in runtime web socket message.");
                return Ok(());
            }

            // Message decoding is ok, let's forward it to the right WS.
            let mut guard = self.broker.lock().unwrap();
            guard.send_message("workers",
                               BrokerMessage::SendToBrowser {
                                   id: id,
                                   kind: e_kind,
                                   data: payload,
                               });
        } else {
            // We know which client this comes from so we don't need to wrap the message with
            // bitsparrow.
            let id = self.worker_id.borrow().clone().unwrap_or("".to_owned());
            let mut guard = self.broker.lock().unwrap();
            guard.send_message("workers",
                               BrokerMessage::SendToRuntime {
                                   id: id.clone(),
                                   data: msg.into_data(),
                               });
        }
        Ok(())
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        match code {
            CloseCode::Normal => info!("The jsworkers ws client is done with the connection."),
            CloseCode::Away => info!("The jsworkers ws client is leaving the site."),
            _ => error!("The jsworkers ws client encountered an error: {}.", reason),
        }

        if self.is_runtime() {
            self.broker.lock().unwrap().broadcast_message(BrokerMessage::StopAll);
        } else {
            let worker_id = self.worker_id.borrow().clone().unwrap_or("".to_owned());
            self.broker.lock().unwrap().send_message("workers",
                                                     BrokerMessage::BrowserWSClosed {
                                                         worker_id: worker_id,
                                                         handler_id: self.handler_id.clone(),
                                                     });
        }
    }

    fn on_error(&mut self, err: Error) {
        error!("The jsworkers ws server encountered an error: {:?}", err);
    }
}

/// Sends a command and JSON payload over a WS.
fn send_json_to_ws<T>(out: &Sender, command: &str, obj: &T) where T: Serialize + Debug {
    let buffer = Encoder::new()
        .string(command)
        .string(&serde_json::to_string(obj).unwrap())
        .end()
        .unwrap();
    out.send(Message::Binary(buffer));
}

pub struct Runtime;

impl Runtime {
    pub fn start(runtime_path: &str, config_root: &str, broker: &SharedBroker<BrokerMessage>) {
        info!("Starting jsworkers runtime");
        let root = config_root.to_string();
        let path = runtime_path.to_string();

        // Start the runtime thread.
        let broker = broker.clone();

        let _ = Builder::new().name("JsWorkers_runtime".to_owned()).spawn(move || {
            info!("Initializing JsWorkers at {}", root.clone());
            let mut workers = JsWorkers::new(&root, &broker);

            // Set up our broker listener and a thread to process messages on it.
            let (tx, rx) = channel::<BrokerMessage>();
            broker.lock().unwrap().add_actor("workers", tx);
            thread::Builder::new()
                .name("JsWorkers_Actor".to_owned())
                .spawn(move || {
                    // We keep track of ws senders here to save us from spawing a extra thread
                    // for each connection to send messages.

                    // The ws used to communicate with the runtime.
                    let mut runtime_ws_out: Option<Sender> = None;
                    // The ws used to communicate with the browsers. This keeps track of all the
                    // clients connected to a given worker.
                    let mut browser_ws_out:
                        HashMap<WorkerInfoKey, RefCell<HashMap<HandlerId, Sender>>> = HashMap::new();

                    // The tx that we'll have to send a response to when registering a service worker.
                    let mut register_result_tx: HashMap<WorkerInfoKey, MpscSender<BrokerMessage>> = HashMap::new();

                    loop {
                        let message = rx.recv().unwrap();
                        match message {
                            BrokerMessage::Start { ref worker, ref host, ref tx } => {
                                info!("Message::Start {:?}", worker);
                                if let Some(ref out) = runtime_ws_out {
                                    // If we don't already run this worker, add it to our set
                                    // as a running one.
                                    if !workers.has_worker(worker) {
                                        workers.add_worker(worker);
                                    }

                                    send_json_to_ws(out,
                                                    "StartWebWorker",
                                                    &workers.get_worker_info(worker.user.clone(),
                                                                             worker.url.clone(),
                                                                             worker.kind.clone()));

                                    // Return the ws url for the client side.
                                    // TODO: use wss:// if tls is enabled.
                                    tx.send(BrokerMessage::ClientEndpoint {
                                        ws_url: format!("ws://{}:2016/client/{}", host, worker.key()),
                                    }).unwrap_or(());
                                } else {
                                    // TODO: queue the requests and drain them when the runtime
                                    // comes up.
                                    error!("Can't start worker because runtime is not up yet!");
                                }
                            }
                            BrokerMessage::Wakeup { ref worker } => {
                                debug!("Message::Wakeup {:?}", worker);
                                if let Some(ref out) = runtime_ws_out {
                                    // If we don't already run this worker, add it to our set
                                    // as a running one.
                                    if !workers.has_worker(worker) {
                                        workers.add_worker(worker);
                                    }

                                    send_json_to_ws(out,
                                                    "RegisterServiceWorker",
                                                    &workers.get_worker_info(worker.user.clone(),
                                                                             worker.url.clone(),
                                                                             worker.kind.clone()));
                                } else {
                                    // TODO: queue the requests and drain them when the runtime
                                    // comes up.
                                    error!("Can't wake up worker because runtime is not up yet!");
                                }
                            }
                            BrokerMessage::GetList { ref user, ref tx } => {
                                let user_workers = workers.get_workers_for(user);
                                let infos = BrokerMessage::List { list: user_workers };
                                tx.send(infos).unwrap();
                            }
                            BrokerMessage::Shutdown => {
                                info!("Sending shutdown message to js runtime...");
                                if let Some(ref out) = runtime_ws_out {
                                    // Send a simple shutdown message.
                                    #[derive(Serialize, Debug)]
                                    struct Payload;
                                    let payload: Payload = Payload;
                                    send_json_to_ws(out, "Shutdown", &payload);
                                }
                            }
                            BrokerMessage::StopAll => {
                                // This message happens when the js runner itself shuts down.
                            }
                            BrokerMessage::RunnerWSOpened { ref out } => {
                                info!("jsrunner WS is ready.");
                                runtime_ws_out = Some(out.clone());
                                workers.wake_up_workers();
                            }
                            BrokerMessage::BrowserWSOpened { ref out, ref worker_id, ref handler_id } => {
                                info!("browser WS is ready for {}.", worker_id);
                                if !browser_ws_out.contains_key(worker_id) {
                                    browser_ws_out.insert(worker_id.clone(), RefCell::new(HashMap::new()));
                                }
                                browser_ws_out.get(worker_id)
                                              .unwrap()
                                              .borrow_mut()
                                              .insert(handler_id.clone(), out.clone());
                            }
                            BrokerMessage::BrowserWSClosed { ref worker_id, ref handler_id } => {
                                if browser_ws_out.contains_key(worker_id) {
                                    // Notify the js runner that a web client disconnected.
                                    if let Some(ref out) = runtime_ws_out {
                                        let info = workers.get_worker_info_from_key(worker_id).unwrap();
                                        send_json_to_ws(out,
                                                        "StopWebWorker",
                                                        &info);
                                    }
                                    browser_ws_out.get(worker_id)
                                                  .unwrap()
                                                  .borrow_mut()
                                                  .remove(handler_id);
                                } else {
                                    error!("Unable to remove ws for worker {}.", worker_id);
                                }
                            }
                            BrokerMessage::SendToBrowser { ref id, ref kind, ref data } => {
                                // TODO: should we buffer the messages waiting for the
                                // browser ws to be ready?
                                if browser_ws_out.contains_key(id) {
                                    if let Some(handlers) = browser_ws_out.get(id) {
                                        let iter = handlers.borrow();
                                        for (_, sender) in iter.deref() {
                                            sender.send(Message::Text(String::from(kind.clone())));
                                            sender.send(Message::Binary(data.clone()));
                                        }
                                    } else {
                                        error!("Unable to relay message to id {}", id);
                                    }
                                } else {
                                    error!("Can't relay a message to unknown id {}", id);
                                }
                            }
                            BrokerMessage::SendToRuntime { ref id, ref data } => {
                                if let Some(ref out) = runtime_ws_out {
                                    // Send the message and the payload to the runtime.
                                    let buffer = Encoder::new()
                                                    .string("Message")
                                                    .string(id)
                                                    .bytes(data)
                                                    .end()
                                                    .unwrap();
                                    out.send(Message::Binary(buffer));
                                } else {
                                    // TODO: queue the requests and drain them when the runtime
                                    // comes up.
                                    error!("Can't relay message to runtime worker because runtime is not up yet!");
                                }
                            }
                            BrokerMessage::Register { ref worker, ref host, ref tx } => {
                                info!("Message::Register {:?}", worker);
                                if let Some(ref out) = runtime_ws_out {
                                    // If we don't already run this worker, add it to our set
                                    // as a running one.
                                    if !workers.has_worker(worker) {
                                        workers.add_worker(worker);
                                    }
                                    let worker_id = worker.key();
                                    register_result_tx.insert(worker_id.clone(), tx.clone());
                                    send_json_to_ws(out,
                                                    "RegisterServiceWorker",
                                                    &workers.get_worker_info(worker.user.clone(),
                                                                             worker.url.clone(),
                                                                             worker.kind.clone()));
                                }
                            }
                            BrokerMessage::RegisterResult { ref id, ref success, ref error } => {
                                info!("Message::RegisterResult");
                                if register_result_tx.contains_key(id) {
                                    // Relay the message to the http handler.
                                    register_result_tx.get(id).unwrap().send(message.clone());
                                    register_result_tx.remove(id);
                                } else {
                                    error!("No tx registered for id {}", id);
                                }
                            }
                            BrokerMessage::GetUserResource { method, url, headers, body, user, tx } => {
                                // TODO: check that we have a worker that could serve this url,
                                // using the worker's url and registration scope.

                                // Send the request to the runtime, with a custom id to track the
                                // response to send back.
                                if let Some(ref out) = runtime_ws_out {
                                    let id = format!("{}", Uuid::new_v4());
                                    register_result_tx.insert(id.clone(), tx.clone());
                                    let mut encoder = Encoder::new()
                                        .string("GetUserResource")
                                        .string(&id)
                                        .string(method.as_ref())
                                        .string(&url)
                                        .uint32(headers.len() as u32);
                                    // Add all the headers.
                                    for header in headers.iter() {
                                        encoder = encoder.string(header.name())
                                                         .string(&header.value_string());
                                    }
                                    // Add the logged in user as an extra header, to let workers
                                    // decide on the access control they want to do.
                                    encoder = encoder.string("x-user")
                                                     .string(&user)
                                    // And finally the body of the request.
                                                     .bytes(&body);
                                    let buffer = encoder.end().unwrap();
                                    out.send(Message::Binary(buffer));
                                } else {
                                    // TODO: return a 500 error?

                                    error!("Can't relay message to runtime worker because runtime is not up yet!");
                                }
                            }
                            BrokerMessage::UserResourceResponse { ref id, ref status, ref headers, ref body } => {
                                info!("Message::RegisterResult");
                                if register_result_tx.contains_key(id) {
                                    // Relay the message to the http handler.
                                    register_result_tx.get(id).unwrap().send(message.clone());
                                    register_result_tx.remove(id);
                                } else {
                                    error!("No tx registered for id {}", id);
                                }
                            }
                            _ => {
                                error!("Unexpected message by the `workers` actor {:?}", message);
                            }
                        }
                    }
                })
                .unwrap();

            Runtime::start_ws(&broker);

            let jsrunner = Runtime::start_jsrunner(&path);

            let (tx, rx) = channel::<BrokerMessage>();
            broker.lock().unwrap().add_actor("runtime", tx);
            loop {
                let res = rx.recv().unwrap();
                match res {
                    BrokerMessage::Shutdown => {
                        break;
                    }
                    _ => {
                        info!("Unexpected message received by the `runtime` actor {:?}", res);
                    }
                }
            }

            info!("Shutting down main js runtime loop...");
        });
    }

    fn start_ws(broker: &SharedBroker<BrokerMessage>) {
        // Start the ws server.
        info!("Starting jsworkers ws server");
        let broker = broker.clone();
        thread::Builder::new()
            .name("JsWorkers_WsServer".to_owned())
            .spawn(move || {
                listen("localhost:2016",
                       |out| RuntimeWsHandler::new(out, broker.clone()))
                    .unwrap();
            })
            .unwrap();
    }

    fn start_jsrunner(runtime_path: &str) -> Result<ManagedProcess, IoError> {
        let path = runtime_path.to_string();
        info!("Starting jsrunner {}", runtime_path);

        ManagedProcess::start(move || {
            Command::new(&path)
                .arg("-ws")
                .arg("ws://localhost:2016/runtime/")
                .spawn()
        })
    }
}
