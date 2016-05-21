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

use std::io::{Error as IOError, Read};
use std::sync::mpsc::channel;

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
}

impl Handler for Router {
    fn handle(&self, req: &mut Request) -> IronResult<Response> {
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
            // Sends a "start" message to the worker set and wait for the answer.
            let (tx, rx) = channel::<Message>();
            let source = itry!(Self::read_body_to_string(&mut req.body));
            let message = Message::Start {
                url: source,
                user: user.unwrap_or(0),
                tx: tx,
            };

            if self.broker.lock().unwrap().send_message("workers", message).is_err() {
                return Ok(Response::with(Status::InternalServerError));
            }

            let res = rx.recv();
            if res.is_err() {
                return Ok(Response::with(Status::InternalServerError));
            }

            return Ok(Response::with(Status::Ok));
        }

        if req.url.path == ["stop"] {
            return Ok(Response::with(Status::Ok));
        }

        if req.url.path == ["list"] {
            return Ok(Response::with(Status::Ok));
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
