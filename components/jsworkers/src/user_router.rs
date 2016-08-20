// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The http router for the users public urls.

/// All endpoints are authenticated. Valid urls are:
///
/// GET|POST|PUT|DELETE /user/$user_name/foo.com/path/to/file.html
///    Returns the response from the service worker for this url and method.

use foxbox_core::broker::SharedBroker;
use foxbox_core::jsworkers::{Message, User, WorkerInfo};
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

pub struct UserRouter {
    broker: SharedBroker<Message>,
}

impl UserRouter {
    pub fn new(broker: &SharedBroker<Message>) -> Self {
        UserRouter { broker: broker.clone() }
    }

    fn read_body_to_string<'a, 'b: 'a>(body: &mut Body<'a, 'b>) -> Result<String, IOError> {
        let mut s = String::new();
        try!(body.read_to_string(&mut s));
        Ok(s)
    }

}

impl Handler for UserRouter {
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

        let user = user.unwrap_or(DEFAULT_USER.to_owned());

        // Check if the path matches the user.
        // XXX: figure out shared/public urls for a user.

        if req.url.path[0] == user {
            // Send the request to the runtime. This holds access to the worker's list to
            // figure out if we have a worker for this domain, so let's do a single roundtrip.
            let (tx, rx) = channel::<Message>();
            let mut body = Vec::new();
            req.body.read_to_end(&mut body);
            let message = Message::GetUserResource {
                url: format!("{}", req.url),
                method: req.method.clone(),
                headers: req.headers.clone(),
                body: body,
                user: user,
                tx: tx,
            };

            if self.broker.lock().unwrap().send_message("workers", message).is_err() {
                return Ok(Response::with(Status::InternalServerError));
            }
            let res = rx.recv();
            if res.is_err() {
                return Ok(Response::with(Status::InternalServerError));
            }
            let res = res.unwrap();
            match res {
                Message::UserResourceResponse { id, status, headers, body } => {
                    let mut response = Response::with(body);
                    response.status = Some(status);
                    response.headers = headers;
                    return Ok(response);
                }
                _ => {
                    error!("Unexpected message: {:?}", res);
                    return Ok(Response::with(Status::InternalServerError));
                }
            }
        }

        // Fallthrough, returning a 404.
        error!("Unknown url: {}", req.url);

        Ok(Response::with((Status::NotFound, format!("Unknown url: {}", req.url))))
    }
}

pub fn create<T>(controller: T) -> Chain
    where T: Controller {

    let router = UserRouter::new(&controller.get_jsworkers_broker());

    let auth_endpoints = if cfg!(feature = "authentication") {
        // Keep this list in sync with all the (url path, http method) from
        // the handle() method and with the CORS chain in http_server.rs
        vec![
            AuthEndpoint(vec![Method::Get, Method::Post, Method::Put, Method::Delete], "user".to_owned()),
        ]
    } else {
        vec![]
    };

    let mut chain = Chain::new(router);
    chain.around(controller.get_users_manager().get_middleware(auth_endpoints));

    chain
}
