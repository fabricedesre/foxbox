/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
extern crate url;

use self::url::Url;
use foxbox_core::traits::Controller;
use std::thread;
use ws;
use ws::{ Handler, Sender, Result, Message, Handshake, CloseCode, Error };
use ws::listen;

pub struct WsServer;

pub struct WsHandler<T> {
    pub out: Sender,
    pub controller: T
}

impl WsServer {

    pub fn start<T: Controller>(controller: T) {
        let addrs: Vec<_> = controller.ws_as_addrs().unwrap().collect();
        thread::Builder::new().name("WsServer".to_owned()).spawn(move || {

            listen(addrs[0], |out| {
                WsHandler {
                    out: out,
                    controller: controller.clone(),
                }
            }).unwrap();
        }).unwrap();
    }
}

impl<T: Controller> WsHandler<T> {

    fn close_with_error(&mut self, reason: &'static str) -> Result<()> {
        self.out.close_with_reason(ws::CloseCode::Error, reason)
    }
}

impl<T: Controller> Handler for WsHandler<T> {

    fn on_open(&mut self, handshake: Handshake) -> Result<()> {
        info!("Hello new ws connection");

        let resource = &handshake.request.resource()[..];

        // creating a fake url to get the path and query parsed
        let url = match Url::parse(&format!("http://box.fox{}", resource)) {
            Ok(val) => val,
            _ => return self.close_with_error("Invalid path"),
        };

        let auth = match url.query_pairs() {
            Some(pairs) => {
                pairs.iter()
                    .find(|ref set| set.0.to_lowercase() == "auth")
                    .map(|ref set| set.1.clone())
            },
            _ => return self.close_with_error("Missing authorization"),
        };

        let token = match auth {
            Some(val) => val,
            _ => return self.close_with_error("Missing authorization"),
        };

        if let Err(_) = self.controller.get_users_manager().verify_token(&token) {
            return self.close_with_error("Authorization failed");
        }

        self.controller.add_websocket(self.out.clone());

        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
        info!("Message from websocket ({:?}): {}", self.out.token(), msg);

        Ok(())
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        match code {
            CloseCode::Normal => info!("The ws client is done with the connection."),
            CloseCode::Away => info!("The ws client is leaving the site."),
            _ => error!("The ws client encountered an error: {}.", reason),
        }

        self.controller.remove_websocket(self.out.clone());
    }

    fn on_error(&mut self, err: Error) {
        error!("The ws server encountered an error: {:?}", err);
    }
}
