// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Manages launching the runtime and relaying messages.

extern crate url;

use bitsparrow::Encoder;
use foxbox_core::broker::SharedBroker;
use foxbox_core::managed_process::ManagedProcess;
use foxbox_core::jsworkers::Message as BrokerMessage;
use self::url::Url;
use serde::Serialize;
use serde_json;
use std::cell::Cell;
use std::cell::RefCell;
use std::fmt::Debug;
use std::io::Error as IoError;
use std::process::Command;
use std::sync::mpsc::channel;
use std::thread;
use std::thread::Builder;
use workers::JsWorkers;
use ws;
use ws::{Handler, listen, Sender, Result as WsResult, Message, Handshake, CloseCode, Error, ErrorKind};

#[derive(Clone, Copy, PartialEq)]
enum ClientType {
    Unknown,
    Runtime,
    Browser,
}

/// Handles the jsworkers websocket server.
struct RuntimeWsHandler {
    pub out: Sender,
    broker: SharedBroker<BrokerMessage>,
    mode: Cell<ClientType>,
    id: RefCell<Option<String>>,
}

impl RuntimeWsHandler {
    fn new(out: Sender, broker: SharedBroker<BrokerMessage>) -> Self {
        RuntimeWsHandler {
            out: out,
            broker: broker,
            mode: Cell::new(ClientType::Unknown),
            id: RefCell::new(None),
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
            guard.send_message("workers", BrokerMessage::RunnerWS { out: self.out.clone() });
        }

        // Client urls are "/client/:id" so we extract the id here.
        if resource.starts_with("/client/") {
            let id = resource.split('/').last().unwrap();
            *self.id.borrow_mut() = Some(id.to_string());
            info!("WS client connection for id {}", id);
            self.mode.set(ClientType::Browser);
        }

        // Wrong resource path, rejecting connection.
        if self.is_unknown() {
            return Err(Error::new(ErrorKind::Internal, "Unknown resource path"));
        }

        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> WsResult<()> {
        info!("Message from jsworkers ws: {}", msg);

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
            let mut workers = JsWorkers::new(&root, &broker);

            // Set up our broker listener and a thread to process messages on it.
            let (tx, rx) = channel::<BrokerMessage>();
            broker.lock().unwrap().add_actor("workers", tx);
            thread::Builder::new()
                .name("JsWorkers_Actor".to_owned())
                .spawn(move || {
                    // We keep track of ws senders here to save us from spawing a extra thread
                    // for each connection to send messages.
                    let mut runtime_ws_out: Option<Sender> = None;

                    loop {
                        let res = rx.recv().unwrap();
                        match res {
                            BrokerMessage::Start { ref worker, ref tx } => {
                                if let Some(ref out) = runtime_ws_out {
                                    // If we don't already run this worker, add it to our set
                                    // as a running one.
                                    // TODO: just pass WorkerInfo objects around.
                                    if !workers.has_worker(worker.user, worker.url.clone()) {
                                        workers.add_worker(worker.user, worker.url.clone());
                                    }
                                    workers.start_worker(worker.user, worker.url.clone());

                                    send_json_to_ws(out,
                                                    "StartWorker",
                                                    &workers.get_worker_info(worker.user, worker.url.clone()));

                                    // Return the ws url for the client side.
                                    // TODO: don't hardcode `localhost`
                                    tx.send(BrokerMessage::ClientEndpoint {
                                        ws_url: format!("ws://localhost:2016/client/{}", worker.key()),
                                    }).unwrap_or(());
                                } else {
                                    // TODO: queue the requests and drain them when the runtime
                                    // comes up.
                                    error!("Can't start worker because runtime is not up yet!");
                                }
                            }
                            BrokerMessage::Stop { ref worker, ref tx } => {
                                if let Some(ref out) = runtime_ws_out {
                                    // Serialize the worker info and send it the the runtime.
                                    send_json_to_ws(out, "StopWorker", worker);
                                } else {
                                    // TODO: queue the requests and drain them when the runtime
                                    // comes up.
                                    error!("Can't stop worker because runtime is not up yet!");
                                }
                            }
                            BrokerMessage::GetList { user, ref tx } => {
                                let user_workers = workers.get_workers_for(user);
                                let infos = BrokerMessage::List { list: user_workers };
                                tx.send(infos).unwrap();
                            }
                            BrokerMessage::Shutdown => break,
                            BrokerMessage::StopAll => {
                                // This message happens when the js runner itself shuts down.
                                workers.stop_all();
                            }
                            BrokerMessage::RunnerWS { ref out } => {
                                info!("jsrunner WS is ready.");
                                runtime_ws_out = Some(out.clone());
                            }
                            _ => {
                                info!("Unexpected message by the  `workers` actor {:?}", res);
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
                        // Stop the js runner.
                        if let Ok(ref runner) = jsrunner {
                            info!("Shutting down the js runner");
                            // TODO: make the borrow checker happy.
                            //runner.shutdown();
                        }
                    }
                    _ => {
                        info!("Unexpected message received by the `runtime` actor {:?}", res);
                    }
                }
            }
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
                .arg("-jsconsole")
                .arg("-ws")
                .arg("ws://localhost:2016/runtime/")
                .spawn()
        })
    }
}
