/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use foxbox_taxonomy::manager::*;
use hyper::net::{ NetworkListener };
use iron::{ AfterMiddleware, Chain, Handler,
            HttpServerFactory, Iron, IronResult, Request,
            Response, ServerFactory };
use iron_cors::CORS;
use iron::error::{ IronError };
use iron::method::Method;
use iron::status::Status;
use mount::Mount;
use router::NoRoute;
use static_router;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use taxonomy_router;
use tls::SniServerFactory;
use traits::Controller;

const THREAD_COUNT: usize = 8;

struct Custom404;

impl AfterMiddleware for Custom404 {
    fn catch(&self, _: &mut Request, err: IronError) -> IronResult<Response> {
        use std::io::Error as StdError;
        use std::io::ErrorKind;

        if let Some(_) = err.error.downcast::<NoRoute>() {
            // Router error
            return Ok(Response::with((Status::NotFound,
                                      format!("Unknown resource: {}", err))));
        } else if let Some(err) = err.error.downcast::<StdError>() {
            // StaticFile error
            if err.kind() == ErrorKind::NotFound {
                return Ok(Response::with((Status::NotFound,
                                          format!("Unknown resource: {}", err))));
            }
        }

        // Just let other errors go through, like 401.
        Err(err)
    }
}

struct Ping;

impl Handler for Ping {
    fn handle (&self, _: &mut Request) -> IronResult<Response> {
        Ok(Response::with(Status::NoContent))
    }
}

pub struct HttpServer<T: Controller> {
    controller: T
}

impl<T: Controller> HttpServer<T> {
    pub fn new(controller: T) -> Self {
        HttpServer { controller: controller }
    }

    pub fn start(&mut self, adapter_api: &Arc<AdapterManager>) {
        let taxonomy_chain = taxonomy_router::create(self.controller.clone(),
                                                      adapter_api);

        let users_manager = self.controller.get_users_manager();
        let mut mount = Mount::new();
        mount.mount("/", static_router::create(users_manager.clone()))
             .mount("/ping", Ping)
             .mount("/api/v1", taxonomy_chain)
             .mount("/users", users_manager.get_router_chain());

        let mut chain = Chain::new(mount);
        chain.link_after(Custom404);

        let cors = CORS::new(vec![
            (vec![Method::Get], "ping".to_owned()),
            (vec![Method::Get, Method::Post, Method::Put, Method::Delete],
             "services/:service/:command".to_owned()),
            (vec![Method::Get], "services/list".to_owned()),

            // Taxonomy router paths. Keep in sync with taxonomy_router.rs
            (vec![Method::Get, Method::Post], "api/v1/services".to_owned()),
            (vec![Method::Post, Method::Delete], "api/v1/services/tags".to_owned()),
            (vec![Method::Get, Method::Post], "api/v1/channels/getters".to_owned()),
            (vec![Method::Get, Method::Post], "api/v1/channels/setters".to_owned()),
            (vec![Method::Put], "api/v1/channels/get".to_owned()),
            (vec![Method::Put], "api/v1/channels/set".to_owned()),
            (vec![Method::Post, Method::Delete], "api/v1/channels/getters/tags".to_owned()),
            (vec![Method::Post, Method::Delete], "api/v1/channels/setters/tags".to_owned())
        ]);
        chain.link_after(cors);

        let addrs: Vec<_> = self.controller.http_as_addrs().unwrap().collect();

        if self.controller.get_tls_enabled() {
            let mut certificate_manager = self.controller.get_certificate_manager();
            let server_factory = SniServerFactory::new(&mut certificate_manager);
            start_server(addrs, chain, server_factory);
        } else {
            start_server(addrs, chain, HttpServerFactory {});
        }
    }
}

fn start_server<TListener, T>(addrs: Vec<SocketAddr>, chain: Chain, factory: T)
    where TListener: NetworkListener + Send + 'static,
          T: ServerFactory<TListener> + Send + 'static {

    thread::Builder::new().name("HttpServer".to_owned())
                          .spawn(move || {
        Iron::new(chain)
             .listen_with(addrs[0], THREAD_COUNT, &factory, None)
             .unwrap();
    }).unwrap();
}

#[cfg(test)]
describe! ping {
    before_each {
        use mount::Mount;
        use iron::Headers;
        use iron::status::Status;
        use iron_test::request;
        use super::Ping;

        let mut mount = Mount::new();
        mount.mount("/ping", Ping);
    }

    it "should response 204 NoContent" {
        let response = request::get("http://localhost:3000/ping",
                                    Headers::new(),
                                    &mount).unwrap();
        assert_eq!(response.status.unwrap(), Status::NoContent);
    }
}

#[cfg(test)]
describe! http_server {
    before_each {
        extern crate hyper;

        use foxbox_taxonomy::manager::AdapterManager;
        use std::thread;
        use std::sync::Arc;
        use std::time::Duration;
        use stubs::controller::ControllerStub;

        let taxo_manager = Arc::new(AdapterManager::new(None));

        let mut http_server = HttpServer::new(ControllerStub::new());
        http_server.start(&taxo_manager);
        // HACK: Let some time for the http server to start.
        thread::sleep(Duration::new(3, 0));
    }

    it "should get the appropriate CORS headers" {
        use iron::headers;
        use iron::method::Method;

        let endpoints = vec![
            (vec![Method::Get, Method::Post, Method::Put],
             "services/:service/:command".to_owned()),
            (vec![Method::Get], "services/list".to_owned())
        ];
        let client = hyper::Client::new();
        for endpoint in endpoints {
            let (_, path) = endpoint;
            let path = "http://localhost:3000/".to_owned() +
                       &(path.replace(":", "foo"));
            let res = client.get(&path).send();
            let headers = &res.unwrap().headers;
            assert!(headers.has::<headers::AccessControlAllowOrigin>());
            assert!(headers.has::<headers::AccessControlAllowHeaders>());
            assert!(headers.has::<headers::AccessControlAllowMethods>());
        };
    }

    it "should respond with 404" {
        use iron::status::Status;
        use std::io::Read;

        let client = hyper::Client::new();
        let path = "http://localhost:3000/foo/bar".to_owned();
        let mut res = client.get(&path).send().unwrap();
        assert_eq!(res.status, Status::NotFound);
        let mut body = String::new();
        res.read_to_string(&mut body).unwrap();
        assert_eq!(body, "Unknown resource: No such file or \
                   directory (os error 2)".to_owned());
    }
}
