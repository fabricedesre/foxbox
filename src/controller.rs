/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate mio;

use context::SharedContext;
use dummy_adapter::DummyAdapter;
use events::*;
use http_server::HttpServer;
use mio::EventLoop;
use service::{ Service, ServiceAdapter };
use websocket_server::WebsocketServer;

pub struct Controller {
    sender: EventSender,
    context: SharedContext
}

impl Controller {
    /// Construct a new `Controller`.
    ///
    /// ```
    /// # use service_manager::Controller;
    /// let controller = Controller::new();
    /// ```
    pub fn new(sender: EventSender, context: SharedContext) -> Controller {
        Controller {
            sender: sender,
            context: context
        }
    }

    pub fn start(&self) {
        println!("Starting controller");

        // Start the http server.
        let http_server = HttpServer::new(self.context.clone());
        http_server.start();

        // Start the websocket server.
        let mut websocket_server = WebsocketServer::new(self.context.clone());
        websocket_server.start();

        // Start the dummy adapter.
        let dummy_adapter = DummyAdapter::new(self.sender.clone(), self.context.clone());
        dummy_adapter.start();
    }
}

impl mio::Handler for Controller {
    type Timeout = ();
    type Message = EventData;

    fn notify(&mut self,
              _: &mut EventLoop<Controller>,
              data: EventData) {
        println!("Receiving a notification! {}", data.description());

        let mut context = self.context.lock().unwrap();
        match data {
            EventData::ServiceStart { id } => {
                // The service should be added already, panic if that's not the
                // case.
                match context.get_service(&id) {
                    None => panic!(format!("Missing service with id {}", id)),
                    Some(service) => {
                        context.notify_new_service(service);
                    }
                }

                println!("ServiceStart {} We now have {} services.", id, context.services_count());
            }
            EventData::ServiceStop { id } => {
                context.remove_service(id.clone());
                println!("ServiceStop {} We now have {} services.", id, context.services_count());
            }
            _ => { }
        }
    }
}
