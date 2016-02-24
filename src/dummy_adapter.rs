/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use controller::Controller;
use events::*;
use iron::{ Request, Response, IronResult };
use iron::headers::ContentType;
use iron::status::Status;
use router::Router;
use service::{ Service, ServiceAdapter, ServiceProperties };
use std::time::Duration;
use std::thread;
use uuid::Uuid;
use ws::Message as WsMessage;

struct DummyService<T> {
    controller: T,
    properties: ServiceProperties,
    dont_kill: bool
}

impl<T: Controller> DummyService<T> {
    fn new(controller: T, id: u32) -> Self {
        println!("Creating dummy service");
        let service_id = Uuid::new_v4().to_simple_string();
        DummyService {
            controller: controller.clone(),
            properties: ServiceProperties {
                id: service_id.clone(),
                name: "dummy service".to_owned(),
                description: "really nothing to see".to_owned(),
                http_url: controller.get_http_root_for_service(service_id.clone()),
                ws_url: controller.get_ws_root_for_service(service_id)
            },
            dont_kill: id % 3 == 0
        }
    }
}

impl<T: Controller> Service for DummyService<T> {
    fn get_properties(&self) -> ServiceProperties {
        self.properties.clone()
    }

    // Starts the service, it will just spawn a thread and send messages once
    // in a while.
    fn start(&self) {
        let props = self.properties.clone();
        let can_kill = !self.dont_kill;
        let controller = self.controller.clone();
        thread::spawn(move || {
            println!("Hello from dummy service thread!");
            let mut i = 0;
            loop {
                thread::sleep(Duration::from_millis(1000));
                let txt = format!("Bip #{} from {}", i, props.id);
                controller.send_event(
                    EventData::Notification { message: WsMessage::text(txt.clone()) }).unwrap();
                controller.send_event(
                    EventData::Notification { message: WsMessage::binary(txt.into_bytes()) }).unwrap();
                i += 1;
                if i == 3 && can_kill {
                    break;
                }
            }
            controller.send_event(
                EventData::ServiceStop { id: props.id.to_string() }).unwrap();
        });
    }

    fn stop(&self) {
        println!("Stopping dummy service");
    }

    // Processes a http request.
    fn process_request(&self, req: &Request) -> IronResult<Response> {
        let cmd = req.extensions.get::<Router>().unwrap().find("command").unwrap_or("");
        let mut response = Response::with(format!("Got command {} at url {}", cmd, req.url));
        response.status = Some(Status::Ok);
        response.headers.set(ContentType::plaintext());
        Ok(response)
    }
}

pub struct DummyAdapter<T> {
    name: String,
    controller: T
}

impl<T: Controller> DummyAdapter<T> {
    pub fn new(controller: T) -> Self {
        println!("Creating dummy adapter");
        DummyAdapter { name: "DummyAdapter".to_owned(),
                       controller: controller }
    }
}

impl<T: Controller> ServiceAdapter for DummyAdapter<T> {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn start(&self) {
        let mut id = 0;
        let controller = self.controller.clone();
        thread::spawn(move || {
            controller.send_event(
                EventData::AdapterStart { name: "Dummy Service Adapter".to_owned() }).unwrap();
            loop {
                thread::sleep(Duration::from_millis(2000));
                id += 1;
                let service = DummyService::new(controller.clone(), id);
                let service_id = service.get_properties().id;
                service.start();
                controller.add_service(Box::new(service));
                controller.send_event(EventData::ServiceStart { id: service_id }).unwrap();

                // Create at most 7 dummy services.
                if id == 7 {
                    break;
                }
            }
        });
    }

    fn stop(&self) {
        println!("Stopping dummy adapter");
    }
}
