// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Manages launching the runtime and relaying messages.

extern crate url;

use broker::SharedBroker;
use self::url::Url;
use std::process::Command;
use std::time::Duration;
use std::thread;
use std::thread::Builder;
use workers::JsWorkers;
use ws;
use ws::{Handler, Sender, Result, Message, Handshake, CloseCode, Error};
use ws::listen;

struct RuntimeWsHandler {
    pub out: Sender,
}

impl RuntimeWsHandler {
    fn close_with_error(&mut self, reason: &'static str) -> Result<()> {
        self.out.close_with_reason(ws::CloseCode::Error, reason)
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

        info!("Opening WS url is {}, resource is {}", url, resource);

        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
        info!("Message from websocket: {}", msg);

        Ok(())
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        match code {
            CloseCode::Normal => info!("The ws client is done with the connection."),
            CloseCode::Away => info!("The ws client is leaving the site."),
            _ => error!("The ws client encountered an error: {}.", reason),
        }
    }

    fn on_error(&mut self, err: Error) {
        error!("The ws server encountered an error: {:?}", err);
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
            let workers = JsWorkers::new(&root, &broker);

            // Starts the ws server.
            thread::Builder::new()
                .name("JsWorkers_WsServer".to_owned())
                .spawn(move || {
                    listen("localhost:2016", |out| RuntimeWsHandler { out: out }).unwrap();
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
