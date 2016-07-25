// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The http router for the jsworkers runtime.

/// All endpoints are authenticated. Valid urls are:
///
/// POST /start
///   description: starts a new web worker or let the client re-connect to an already created one.
///   input: { url: <worker_url> }
///   output: 200 { url: <worker_url>, ws_url: <websocket_url> }
///
/// POST /register
///    description: registers a service worker.
///    input: { url: <worker_url>, options: <service_worker_registration_options> }
///    output: 200 { url: <worker_url> }
///
/// POST /unregister
///    description: registers a service worker.
///    input: { url: <worker_url>, options: <service_worker_registration_options> }
///    output: 200 { url: <worker_url> }
///
/// GET /list
///   description: returns the list of workers installed for this user.
///   input: None
///   output: 200 [{ kind: Web|Service, state: Running|Stopped, url: <worker_url>, ws_url: <websocket_url> }*]

use foxbox_core::broker::SharedBroker;
use foxbox_core::jsworkers::{Message, User, WorkerInfo, WorkerKind, WorkerState};
use foxbox_core::traits::Controller;

use foxbox_users::{AuthEndpoint, SessionToken};

use iron::{Handler, headers, IronResult, Request, Response};
use iron::error::IronError;
use iron::headers::ContentType;
use iron::method::Method;
use iron::prelude::Chain;
use iron::request::Body;
use iron::status::Status;
use serde_json;
use std::io::{Error as IOError, Read};
use std::sync::mpsc::channel;

// We never use that user when authentication is on.
static DEFAULT_USER: &'static str = "_dummy_user_";

pub struct Router {
    broker: SharedBroker<Message>,
}

impl Router {
    pub fn new(broker: &SharedBroker<Message>) -> Self {
        Router { broker: broker.clone() }
    }

    fn read_body_to_string<'a, 'b: 'a>(body: &mut Body<'a, 'b>) -> Result<String, IOError> {
        let mut s = String::new();
        try!(body.read_to_string(&mut s));
        Ok(s)
    }

    fn handle_list(&self, user: Option<User>) -> IronResult<Response> {
        // Sends a "List" message to the worker set and wait for the answer.
        let (tx, rx) = channel::<Message>();
        let message = Message::GetList {
            user: user.unwrap_or(DEFAULT_USER.to_owned()),
            tx: tx,
        };

        // Send a request to the Workers manager.
        if self.broker.lock().unwrap().send_message("workers", message).is_err() {
            error!("Failed to send GetList message");
            return Ok(Response::with(Status::InternalServerError));
        }

        // Read the response back.
        let res = rx.recv();
        if res.is_err() {
            error!("Failed to receive List message");
            return Ok(Response::with(Status::InternalServerError));
        }

        // Turn the result into a JSON response.
        let serialized = itry!(serde_json::to_string(&res.unwrap()));
        let mut response = Response::with(serialized);
        response.status = Some(Status::Ok);
        response.headers.set(ContentType::json());
        Ok(response)
    }

    // Extracts the url parameter from the request.
    fn get_worker_url(&self, req: &mut Request) -> IronResult<String> {
        let source = itry!(Self::read_body_to_string(&mut req.body));
        #[derive(Deserialize)]
        struct Params {
            url: String,
        }
        let params: Result<Params, serde_json::Error> = serde_json::from_str(&source);
        match params {
            Ok(val) => Ok(val.url),
            Err(err) => Err(IronError::new(err, Status::BadRequest))
        }
    }

    fn handle_start(&self, req: &mut Request, user: Option<User>) -> IronResult<Response> {
        let worker_url = match self.get_worker_url(req) {
            Ok(url) => url,
            Err(err) => { return Err(err); }
        };

        // Sends a "Start" message to the worker set and wait for the answer.
        let (tx, rx) = channel::<Message>();
        let message = Message::Start {
            worker: WorkerInfo::new_webworker(user.unwrap_or(DEFAULT_USER.to_owned()),
                                              worker_url.clone(),
                                              WorkerState::Stopped),
            host: req.url.host.serialize(),
            tx: tx,
        };

        if self.broker.lock().unwrap().send_message("workers", message).is_err() {
            return Ok(Response::with(Status::InternalServerError));
        }

        // There is no acknowledgment when a worker starts successfully, but we receive the
        // url for the ws endpoint.
        let res = rx.recv();
        if res.is_err() {
            return Ok(Response::with(Status::InternalServerError));
        }
        let res = res.unwrap();
        match res {
            Message::ClientEndpoint { ws_url } => {
                let mut response =
                    Response::with(json!({ url: worker_url, ws_url: ws_url }));
                response.status = Some(Status::Ok);
                response.headers.set(ContentType::json());
                return Ok(response);
            }
            _ => {
                return Ok(Response::with(Status::InternalServerError));
            }
        }
    }

    fn handle_register(&self, req: &mut Request, user: Option<User>) -> IronResult<Response> {
        let worker_url = match self.get_worker_url(req) {
            Ok(url) => url,
            Err(err) => { return Err(err); }
        };

        // Sends a "Register" message to the worker set.
        let message = Message::Register {
            worker: WorkerInfo::new_serviceworker(user.unwrap_or(DEFAULT_USER.to_owned()),
                                                  worker_url.clone(),
                                                  WorkerState::Stopped),
            host: req.url.host.serialize(),
        };

        if self.broker.lock().unwrap().send_message("workers", message).is_err() {
            return Ok(Response::with(Status::InternalServerError));
        }

        // There is no response when registering service workers.
        let mut response =
            Response::with(json!({ url: worker_url }));
        response.status = Some(Status::Ok);
        response.headers.set(ContentType::json());
        Ok(response)
    }
}

impl Handler for Router {
    fn handle(&self, req: &mut Request) -> IronResult<Response> {
        // Gets the user from the session token and dispatches the request processing.

        let user = match req.headers.clone().get::<headers::Authorization<headers::Bearer>>() {
            Some(&headers::Authorization(headers::Bearer { ref token })) => {
                match SessionToken::from_string(token) {
                    Ok(token) => Some(token.claims.id),
                    Err(_) => return Ok(Response::with(Status::Unauthorized)),
                }
            }
            _ => {
                error!("Unauthenticated request to {:?}", req.url.path);
                None
            }
        };

        if cfg!(feature = "authentication") && user == None {
            return Ok(Response::with(Status::Unauthorized));
        }

        if req.url.path == ["start"] && req.method == Method::Post {
            return self.handle_start(req, user);
        }

        if req.url.path == ["register"] && req.method == Method::Post {
            return self.handle_register(req, user);
        }

        /*if req.url.path == ["unregister"] && req.method == Method::Post {
            return self.handle_stop(req, user);
        }*/

        if req.url.path == ["list"] && req.method == Method::Get {
            return self.handle_list(user);
        }

        // Fallthrough, returning a 404.
        Ok(Response::with((Status::NotFound, format!("Unknown url: {}", req.url))))
    }
}

pub fn create<T>(controller: T) -> Chain
    where T: Controller {

    let router = Router::new(&controller.get_jsworkers_broker());

    let auth_endpoints = if cfg!(feature = "authentication") {
        // Keep this list in sync with all the (url path, http method) from
        // the handle() method and with the CORS chain in http_server.rs
        vec![
            AuthEndpoint(vec![Method::Get], "list".to_owned()),
            AuthEndpoint(vec![Method::Post], "start".to_owned()),
            AuthEndpoint(vec![Method::Post], "stop".to_owned()),
            AuthEndpoint(vec![Method::Post], "register".to_owned()),
            AuthEndpoint(vec![Method::Post], "unregister".to_owned()),
        ]
    } else {
        vec![]
    };

    let mut chain = Chain::new(router);
    chain.around(controller.get_users_manager().get_middleware(auth_endpoints));

    chain
}
