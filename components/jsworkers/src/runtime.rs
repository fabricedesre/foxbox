// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Manages launching the runtime and relaying messages.

extern crate url;

use broker::{ Message as BrokerMessage, SharedBroker };
use self::url::Url;
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
    broker: SharedBroker,
    mode: Cell<ClientType>,
}

impl RuntimeWsHandler {
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

        info!("Opening jsworkers ws: url is {}, resource is {}", url, resource);

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
    pub fn start(runtime_path: &str, config_root: &str, broker: &SharedBroker) {
        info!("Starting jsworkers runtime {}", runtime_path);
        let path = runtime_path.to_string();
        let root = config_root.to_string();
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
                    loop {
                        let res = rx.recv().unwrap();
                        match res {
                            BrokerMessage::Start { ref url, user, ref tx } => { },
                            BrokerMessage::Stop { ref url, user, ref tx } => { },
                            BrokerMessage::GetList { user, ref tx } => {
                                let user_workers = workers.get_workers_for(user);
                                let infos = BrokerMessage::List {
                                    list: user_workers
                                };
                                tx.send(infos).unwrap();
                            },
                            BrokerMessage::Shutdown => break,
                            BrokerMessage::StopAll => {
                                // This message happens when the runtime itself shuts down.
                                workers.stop_all();
                            }
                            _ => { info!("Unexpected message in JsWorkers_Actor thread {}", res); }
                        }
                    }
                })
                .unwrap();

            // Start the ws server.
            thread::Builder::new()
                .name("JsWorkers_WsServer".to_owned())
                .spawn(move || {
                    listen("localhost:2016", |out| RuntimeWsHandler {
                        out: out,
                        broker: broker.clone(),
                        mode: Cell::new(ClientType::Unknown),
                    }).unwrap();
                })
                .unwrap();

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
        });
    }
}
