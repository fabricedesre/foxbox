// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Manages launching the runtime and relaying messages.

extern crate url;

use bitsparrow::Encoder;
use broker::SharedBroker;
use message::Message as BrokerMessage;
use self::url::Url;
use serde_json;
use std::cell::Cell;
use std::process::Command;
use std::sync::mpsc::channel;
use std::time::Duration;
use std::thread;
use std::thread::Builder;
use workers::JsWorkers;
use ws;
use ws::{Handler, Sender, Result, Message, Handshake, CloseCode, Error, ErrorKind};
use ws::listen;

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
}

impl RuntimeWsHandler {
    fn new(out: Sender, broker: SharedBroker<BrokerMessage>) -> Self {
        RuntimeWsHandler {
            out: out,
            broker: broker,
            mode: Cell::new(ClientType::Unknown),
        }
    }

    fn close_with_error(&mut self, reason: &'static str) -> Result<()> {
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
    fn on_open(&mut self, handshake: Handshake) -> Result<()> {
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
        }

        if resource == "/client/" {
            self.mode.set(ClientType::Browser);
        }

        // Wrong resource path, rejecting connection.
        if self.is_unknown() {
            return Err(Error::new(ErrorKind::Internal, "Unknown resource path"));
        }

        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
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
                                    // Serialize the worker info and send it the the runtime.
                                    let buffer = Encoder::new()
                                        .string("StartWorker")
                                        .string(&serde_json::to_string(worker).unwrap())
                                        .end()
                                        .unwrap();
                                    out.send(Message::Binary(buffer));
                                } else {
                                    // TODO: queue the requests and drain them when the runtime
                                    // comes up.
                                    error!("Can't start worker because runtime is not up yet!");
                                }
                            }
                            BrokerMessage::Stop { ref worker, ref tx } => {
                                if let Some(ref out) = runtime_ws_out {
                                    // Serialize the worker info and send it the the runtime.
                                    let buffer = Encoder::new()
                                        .string("StopWorker")
                                        .string(&serde_json::to_string(worker).unwrap())
                                        .end()
                                        .unwrap();
                                    out.send(Message::Binary(buffer));
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
                                runtime_ws_out = Some(out.clone());
                            }
                            _ => {
                                info!("Unexpected message in JsWorkers_Actor thread {:?}", res);
                            }
                        }
                    }
                })
                .unwrap();

            Runtime::start_ws(&broker);

            // This is a blocking call.
            Runtime::start_jsrunner(&path);
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

    fn start_jsrunner(runtime_path: &str) {
        let path = runtime_path.to_string();
        info!("Starting jsrunner {}", runtime_path);
        loop {
            let output = Command::new(&path)
                .arg("-ws")
                .arg("ws://localhost:2016/runtime/")
                .output()
                .unwrap_or_else(|e| panic!("failed to execute process: {}", e));

            error!("Failed to run jsworkers runtime:\nstdout:\n{}\nstderr:\n{}",
                   String::from_utf8_lossy(&output.stdout),
                   String::from_utf8_lossy(&output.stderr));

            // Wait 1 second before restarting.
            // TODO: exponential backoff?? something better anyway.
            thread::sleep(Duration::new(1, 0));
        }
    }
}
