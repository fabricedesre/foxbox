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

use foxbox_users::SessionToken;

use iron::{Handler, headers, IronResult, Request, Response};
use iron::headers::ContentType;
use iron::method::Method;
use iron::prelude::Chain;
use iron::request::Body;
use iron::status::Status;
use std::sync::{Arc, Mutex};

use broker::{Message, SharedBroker};

pub struct Router {
    broker: SharedBroker,
}

impl Router {
    pub fn new(broker: &SharedBroker) -> Self {
        Router { broker: broker.clone() }
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
