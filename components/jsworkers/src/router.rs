// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The http router for the jsworkers runtime.

/// All endpoints are authenticated. Valid urls are:
///
/// POST /start
///   description: starts a new worker or let the client re-connect to an already created one.
///   input: { url: <worker_url> }
///   output: 200 { url: <worker_url>, ws: <websocket_url> }
///
/// POST /stop
///   description: stops a worker.
///   input: { url: <worker_url> }
///   output: 200 { url: <worker_url>, ws: <websocket_url> }
///
/// GET /list
///   description: returns the list of workers installed for this user.
///   input: None
///   output: 200 [{ state: Running|Stopped, url: <worker_url>, ws: <websocket_url> }*]

use broker::{Message, SharedBroker};
use foxbox_users::SessionToken;

use iron::{Handler, headers, IronResult, Request, Response};
use iron::headers::ContentType;
use iron::method::Method;
use iron::prelude::Chain;
use iron::request::Body;
use iron::status::Status;
use serde_json;
use std::io::{Error as IOError, Read};
use std::sync::mpsc::channel;
use workers::User;

pub struct Router {
    broker: SharedBroker,
}

impl Router {
    pub fn new(broker: &SharedBroker) -> Self {
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
            user: user.unwrap_or(0),
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

    fn handle_start(&self, req: &mut Request, user: Option<User>) -> IronResult<Response> {
        // Sends a "Start" message to the worker set and wait for the answer.
        let (tx, rx) = channel::<Message>();
        let source = itry!(Self::read_body_to_string(&mut req.body));
        let message = Message::Start {
            url: source,
            user: user.unwrap_or(0), // TODO: respect the `authentication` feature.
            tx: tx,
        };

        if self.broker.lock().unwrap().send_message("workers", message).is_err() {
            return Ok(Response::with(Status::InternalServerError));
        }

        let res = rx.recv();
        if res.is_err() {
            return Ok(Response::with(Status::InternalServerError));
        }

        Ok(Response::with(Status::Ok))
    }

    fn handle_stop(&self, _: &mut Request, _: Option<User>) -> IronResult<Response> {
        return Ok(Response::with(Status::Ok));
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
            _ => None,
        };

        info!("HTTP Request path is {:?}, user is {:?}",
              req.url.path,
              user);

        if req.url.path == ["start"] && req.method == Method::Post {
            return self.handle_start(req, user);
        }

        if req.url.path == ["stop"] && req.method == Method::Post {
            return self.handle_stop(req, user);
        }

        if req.url.path == ["list"] && req.method == Method::Get {
            return self.handle_list(user);
        }

        // Fallthrough, returning a 404.
        Ok(Response::with((Status::NotFound, format!("Unknown url: {}", req.url))))
    }
}

pub fn create(broker: &SharedBroker) -> Chain {
    // TODO: add authentication support.
    // That requires access to controller.get_users_manager() which is not yet available to code
    // in components/ crates.

    let router = Router::new(broker);
    Chain::new(router)
}
