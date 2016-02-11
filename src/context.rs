/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate serde_json;
extern crate collections;

use self::collections::vec::IntoIter;
use service::Service;
use std::collections::HashMap;
use std::io::Error;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::sync::{ Arc, Mutex };
//use websocket_server::WebsocketHandler;
use std::cell::RefCell;

// The `global` context available to all.
pub struct Context {
    pub verbose: bool,

    hostname: String,
    http_port: u16,
    ws_port: u16,
    services: HashMap<String, Box<Service>>,

    //websockets: Vec<Arc<WebsocketHandler>>
}

const DEFAULT_HTTP_PORT: u16 = 3000;
const DEFAULT_WS_PORT: u16 = 4000;
const DEFAULT_HOSTNAME: &'static str = "::"; // ipv6 default.

pub type SharedContext = Arc<Mutex<Context>>;

impl Context {
    pub fn new(verbose: bool, hostname: Option<String>,
               http_port: Option<u16>, ws_port: Option<u16>) -> Context {

        Context { services: HashMap::new(),
                  //websockets: Vec::new(),
                  verbose: verbose,
                  hostname:  hostname.unwrap_or(DEFAULT_HOSTNAME.to_owned()),
                  http_port: http_port.unwrap_or(DEFAULT_HTTP_PORT),
                  ws_port: ws_port.unwrap_or(DEFAULT_WS_PORT) }
    }

    pub fn shared(verbose: bool, hostname: Option<String>,
                  http_port: Option<u16>,
                  ws_port: Option<u16>) -> SharedContext {
        Arc::new(Mutex::new(Context::new(verbose, hostname, http_port, ws_port)))
    }

    pub fn add_service(&mut self, service: Box<Service>) {
        let service_id = service.get_properties().id;
        self.services.insert(service_id, service);
    }

    pub fn remove_service(&mut self, id: String) {
        self.services.remove(&id);
    }

    pub fn services_count(&self) -> usize {
        self.services.len()
    }

    pub fn get_service(&self, id: &str) -> Option<&Box<Service>> {
        self.services.get(id)
    }

    pub fn notify_new_service(&self, service: &Box<Service>) {
        // Will iterate over global_ws to notify about service information.
    }

    pub fn services_as_json(&self) -> Result<String, serde_json::error::Error> {
        serde_json::to_string(&self.services)
    }

    pub fn get_http_root_for_service(&self, service_id: String) -> String {
        format!("http://{}:{}/services/{}/", self.hostname, self.http_port, service_id)
    }

    pub fn get_ws_root_for_service(&self, service_id: String) -> String {
        format!("ws://{}:{}/services/{}/", self.hostname, self.ws_port, service_id)
    }

    pub fn http_as_addrs(&self) -> Result<IntoIter<SocketAddr>, Error> {
        (self.hostname.as_str(), self.http_port).to_socket_addrs()
    }

    pub fn ws_as_addrs(&self) -> Result<IntoIter<SocketAddr>, Error> {
        (self.hostname.as_str(), self.ws_port).to_socket_addrs()
    }

    /*pub fn add_websocket(&mut self, ws: Arc<WebsocketHandler>) {
        self.websockets.push(ws);
    }*/
}

#[test]
#[allow(unused_variables)]
fn test_context_service() {
    use service::{ Service, ServiceProperties };
    use iron::{Request, Response, IronResult};

    struct ServiceStub;

    impl Service for ServiceStub {
        fn get_properties(&self) -> ServiceProperties {
            ServiceProperties {
                id: "1".to_owned(),
                name: "dummy service".to_owned(),
                description: "really nothing to see".to_owned(),
                http_url: "2".to_owned(),
                ws_url: "3".to_owned()
            }
        }
        fn start(&self)  {}
        fn stop(&self) {}
        fn process_request(&self, req: &Request) -> IronResult<Response> {
            Ok(Response::new())
        }
    }

    let service = ServiceStub;
    let context = Context::shared(false, Some("localhost".to_owned()), None, None);
    let mut foo = context.lock().unwrap();

    assert_eq!(foo.services.is_empty(), true);
    foo.add_service(Box::new(service));
    assert_eq!(foo.services.is_empty(), false);
    assert_eq!(foo.services_count(), 1);

    // Verify that the service with id "1" is there.
    {
        let service1 = foo.get_service("1");
        match service1 {
            Some(svc) => {
                assert_eq!(svc.get_properties().id, "1");
            }
            None => assert!(false, "No service with id 1")
        }
    }

    let id = "1".to_owned();
    assert_eq!(foo.get_http_root_for_service(id.clone()),
               "http://localhost:3000/services/1/");
    assert_eq!(foo.get_ws_root_for_service(id.clone()),
               "ws://localhost:4000/services/1/");

    match foo.services_as_json() {
        Ok(txt) => assert_eq!(txt, "{\"1\":{\"id\":\"1\",\"name\":\"dummy service\",\"description\":\"really nothing to see\",\"http_url\":\"2\",\"ws_url\":\"3\"}}"),
        Err(err) => assert!(false, err)
    }

    foo.remove_service(id);
    assert_eq!(foo.services_count(), 0);
    assert_eq!(foo.services.is_empty(), true);
}
