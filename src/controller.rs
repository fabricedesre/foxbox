/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate serde_json;
extern crate collections;
extern crate mio;

use core::marker::Reflect;
use dummy_adapter::DummyAdapter;
use events::{ EventData, EventSender };
use http_server::HttpServer;
use iron::{Request, Response, IronResult};
use iron::headers::{ ContentType, AccessControlAllowOrigin };
use iron::status::Status;
use mio::{ NotifyError, EventLoop };
use self::collections::vec::IntoIter;
use service::{ Service, ServiceAdapter, ServiceProperties };
use std::cell::RefCell;
use std::collections::hash_map::HashMap;
use std::io;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::sync::{ Arc, Mutex };
use ws_server::WsServer;
use ws;

#[derive(Clone)]
pub struct FoxBox {
    event_sender: Option<EventSender>,
    pub verbose: bool,
    hostname: String,
    http_port: u16,
    ws_port: u16,
    services: Arc<Mutex<HashMap<String, Box<Service>>>>,
    websockets: Arc<Mutex<HashMap<ws::util::Token, ws::Sender>>>,
}

pub const DEFAULT_HTTP_PORT: u16 = 3000;
const DEFAULT_WS_PORT: u16 = 4000;
const DEFAULT_HOSTNAME: &'static str = "::"; // ipv6 default.

pub trait Controller : Send + Sync + Clone + Reflect + 'static {
    fn run(&mut self);
    fn dispatch_service_request(&self, id: String, request: &Request) -> IronResult<Response>;
    fn add_service(&self, service: Box<Service>);
    fn remove_service(&self, id: String);
    fn get_service_properties(&self, id: String) -> Option<ServiceProperties>;
    fn services_count(&self) -> usize;
    fn services_as_json(&self) -> Result<String, serde_json::error::Error>;
    fn get_http_root_for_service(&self, service_id: String) -> String;
    fn get_ws_root_for_service(&self, service_id: String) -> String;
    fn http_as_addrs(&self) -> Result<IntoIter<SocketAddr>, io::Error>;
    fn send_event(&self, data: EventData) -> Result<(), NotifyError<EventData>>;

    fn add_websocket(&mut self, socket: ws::Sender);
    fn remove_websocket(&mut self, socket: ws::Sender);
    fn broadcast_to_websockets(&self, message: ws::Message);
}

impl FoxBox {

    pub fn new(verbose: bool,
               hostname: Option<String>,
               http_port: Option<u16>,
               ws_port: Option<u16>) -> Self {

        FoxBox {
            services: Arc::new(Mutex::new(HashMap::new())),
            websockets: Arc::new(Mutex::new(HashMap::new())),
            verbose: verbose,
            hostname:  hostname.unwrap_or(DEFAULT_HOSTNAME.to_owned()),
            http_port: http_port.unwrap_or(DEFAULT_HTTP_PORT),
            ws_port: ws_port.unwrap_or(DEFAULT_WS_PORT),

            event_sender: None
        }
    }
}

impl Controller for FoxBox {

    fn run(&mut self) {
        println!("Starting controller");

        let mut event_loop = mio::EventLoop::new().unwrap();
        self.event_sender = Some(event_loop.channel());

        HttpServer::new(self.clone()).start();
        WsServer::start(self.clone(), self.hostname.to_owned(), self.ws_port);
        DummyAdapter::new(self.clone()).start();
        event_loop.run(&mut FoxBoxEventLoop { controller: self.clone() }).unwrap();
    }

    fn dispatch_service_request(&self, id: String, request: &Request) -> IronResult<Response> {
        let services = self.services.lock().unwrap();
        match services.get(&id) {
            None => {
                let mut response = Response::with(format!("No Such Service: {}", id));
                response.status = Some(Status::BadRequest);
                response.headers.set(AccessControlAllowOrigin::Any);
                response.headers.set(ContentType::plaintext());
                Ok(response)
            }
            Some(service) => {
                service.process_request(&request)
            }
        }
    }

    fn send_event(&self, data: EventData) -> Result<(), NotifyError<EventData>> {
        self.event_sender.clone().unwrap().send(data)
    }

    fn add_service(&self, service: Box<Service>) {
        let mut services = self.services.lock().unwrap();
        let service_id = service.get_properties().id;
        services.insert(service_id, service);
    }

    fn remove_service(&self, id: String) {
        let mut services = self.services.lock().unwrap();
        services.remove(&id);
    }

    fn services_count(&self) -> usize {
        let services = self.services.lock().unwrap();
        services.len()
    }

    fn get_service_properties(&self, id: String) -> Option<ServiceProperties> {
        let services = self.services.lock().unwrap();
        services.get(&id).map(|v| v.get_properties().clone() )
    }

    fn services_as_json(&self) -> Result<String, serde_json::error::Error> {
        let services = self.services.lock().unwrap();
        let mut array: Vec<&Box<Service>> = vec!();
        for service in services.values() {
            array.push(service);
        }
        serde_json::to_string(&array)
    }

    fn get_http_root_for_service(&self, service_id: String) -> String {
        format!("http://{}:{}/services/{}/", self.hostname, self.http_port, service_id)
    }

    fn get_ws_root_for_service(&self, service_id: String) -> String {
        format!("ws://{}:{}/services/{}/", self.hostname, self.ws_port, service_id)
    }

    fn http_as_addrs(&self) -> Result<IntoIter<SocketAddr>, io::Error> {
        (self.hostname.as_str(), self.http_port).to_socket_addrs()
    }

    fn add_websocket(&mut self, socket: ws::Sender) {
        self.websockets.lock().unwrap().insert(socket.token(), socket);
    }

    fn remove_websocket(&mut self, socket: ws::Sender) {
        self.websockets.lock().unwrap().remove(&socket.token());
    }

    fn broadcast_to_websockets(&self, message: ws::Message) {
        let shared = RefCell::new(message);
        for socket in self.websockets.lock().unwrap().values() {
            match socket.send(shared.clone().into_inner()) {
                Ok(_) => (),
                Err(err) => println!("Error sending to socket: {}", err)
            }
        }
    }
}

struct FoxBoxEventLoop {
    controller: FoxBox
}

impl mio::Handler for FoxBoxEventLoop {
    type Timeout = ();
    type Message = EventData;

    fn notify(&mut self, _: &mut EventLoop<Self>, data: EventData) {
        println!("Receiving a notification! {}", data.description());

        match data {
            EventData::ServiceStart { ref id } => {
                println!("Service started: {:?}",
                         self.controller.get_service_properties(id.to_owned()));

                println!("ServiceStart {} We now have {} services.",
                         id, self.controller.services_count());
                self.controller.broadcast_to_websockets(data.as_ws_message().unwrap());
            }
            EventData::ServiceStop { ref id } => {
                self.controller.remove_service(id.clone());
                println!("ServiceStop {} We now have {} services.",
                         id, self.controller.services_count());
                self.controller.broadcast_to_websockets(data.as_ws_message().unwrap());
            }
            EventData::Notification { message } =>
                self.controller.broadcast_to_websockets(message),
            _ => { }
        }
    }
}


#[cfg(test)]
describe! controller {

    before_each {
        use stubs::service::ServiceStub;

        let service = ServiceStub;
        let controller = FoxBox::new(false, Some("localhost".to_owned()), None, None);
    }

    describe! add_service {
        it "should increase number of services" {
            controller.add_service(Box::new(service));
            assert_eq!(controller.services_count(), 1);
        }

        it "should make service available" {
            controller.add_service(Box::new(service));

            match controller.get_service_properties("1".to_owned()) {
                Some(props) => {
                    assert_eq!(props.id, "1");
                }
                None => assert!(false, "No service with id 1")
            }
        }

        it "should create http root" {
            controller.add_service(Box::new(service));
            assert_eq!(controller.get_http_root_for_service("1".to_string()),
                       "http://localhost:3000/services/1/");
        }

        it "should create ws root" {
            controller.add_service(Box::new(service));
            assert_eq!(controller.get_ws_root_for_service("1".to_string()),
                       "ws://localhost:4000/services/1/");
        }

        it "should return a json" {
            controller.add_service(Box::new(service));

            match controller.services_as_json() {
                Ok(txt) => assert_eq!(txt, "[{\"id\":\"1\",\"name\":\"dummy service\",\"description\":\"really nothing to see\",\"http_url\":\"2\",\"ws_url\":\"3\"}]"),
                Err(err) => assert!(false, err)
            }
        }
    }


    it "should delete a service" {
        controller.add_service(Box::new(service));
        let id = "1".to_owned();
        controller.remove_service(id);
        assert_eq!(controller.services_count(), 0);
    }
}
