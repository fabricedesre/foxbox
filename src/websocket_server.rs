/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/*
 * uses the ws-rs crate
 */

/*
use context::SharedContext;
use std::sync::Arc;
use std::thread;
use ws::{Builder, Error, ErrorKind, Factory, Handler, Handshake, Sender, Request, Response, Result, Message, CloseCode};
use std::cell::RefCell;

pub struct WebsocketServer {
    context: SharedContext
}

enum HandlerKind {
    Unknown,
    // Global messages related to service discovery.
    Global,
    // Service specific messages.
    Service
}

pub struct WebsocketHandler {
    out: Sender,
    context: SharedContext,
    kind: HandlerKind,
    service_id: Option<String>
}

impl Drop for WebsocketHandler {
    fn drop(&mut self) {
        let svc = self.service_id.clone().unwrap_or("".to_owned()).clone();
        println!("dropping WebsocketHandler {}", svc);
        // TODO: unregister ourselves from context.
    }
}

impl WebsocketHandler {
    fn maybe_enqueue(s: &Arc<Self>, pred: bool) {
        if pred {
            s.context.lock().unwrap().add_websocket(s.clone());
        }
    }
}

impl Handler for WebsocketHandler {
    // Check if the path matches a service id, and close the connection
    // if this is not the case.
    fn on_request(&mut self, req: &Request) -> Result<Response> {
        println!("on_request {}", req.resource());
        let service = req.resource()[1..].to_owned();
        self.service_id = Some(service.clone());

        // Hardcoded endpoint where clients can listen to general notifications,
        // like services starting and stoping.
        if service == "services" {
            let res = try!(Response::from_request(req));
            self.kind = HandlerKind::Global;
            return Ok(res);
        }

        // Look for a service.
        let guard = self.context.lock().unwrap();
        match guard.get_service(&service) {
            None => Err(Error::new(ErrorKind::Internal, "No such service")),
            Some(_) => {
                // We have a service!
                let res = try!(Response::from_request(req));
                {
                    self.kind = HandlerKind::Service;
                }

                //let ctxt = self.context.clone();
                // Let's attach add reference to ourselves into the
                // Context's websocket vector.
                //
                // This fails with "cannot move out of borrowed content [E0507]" :

                let ws = Arc::new(*self);
                WebsocketHandler::maybe_enqueue(&Arc::new(*self), true);
                // guard.add_ws(Arc::new(*self));
                Ok(res)
            }
        }
    }

    fn on_open(&mut self, shake: Handshake) -> Result<()> {
        let service = shake.request.resource()[1..].to_owned();
        println!("on_open");

        if service == "services" {
            // Bind to the global websocket broadcaster.
        } else {
            // Bind to a service websocket broadcaster.
        }

        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
        // Echo the message back
        self.out.send(msg)
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        // The WebSocket protocol allows for a utf8 reason for the closing state after the
        // close code. WS-RS will attempt to interpret this data as a utf8 description of the
        // reason for closing the connection. I many cases, `reason` will be an empty string.
        // So, you may not normally want to display `reason` to the user,
        // but let's assume that we know that `reason` is human-readable.
        match code {
            CloseCode::Normal => println!("The client is done with the connection."),
            CloseCode::Away   => println!("The client is leaving the site."),
            _ => println!("The client encountered an error: {}", reason),
        }
    }
}

struct WebsocketFactory {
    context: SharedContext
}

impl WebsocketFactory {
    fn new(context: SharedContext) -> WebsocketFactory {
        WebsocketFactory {
            context: context
        }
    }
}

impl Factory for WebsocketFactory {
    type Handler = WebsocketHandler;

    fn connection_made(&mut self, sender: Sender) -> Self::Handler {
        WebsocketHandler {
            out: sender,
            context: self.context.clone(),
            kind: HandlerKind::Unknown,
            service_id: None
        }
    }
}

impl WebsocketServer {
    pub fn new(context: SharedContext) -> WebsocketServer {
        WebsocketServer { context: context }
    }

    pub fn start(&self) {
        let addrs: Vec<_> =
            self.context.lock().unwrap().ws_as_addrs().unwrap().collect();

        let context = self.context.clone();
        thread::Builder::new().name("WebsocketServer".to_owned())
                              .spawn(move || {
            Builder::new().build(WebsocketFactory::new(context)).unwrap()
                          .listen(addrs[0]).unwrap();
        }).unwrap();
    }
}
*/

/*
  uses the websocket crate
*/
/*
use context::SharedContext;
use hyper::uri::RequestUri;
use std::thread;
use std::io::{ Read, Write };
use std::sync::Arc;
use websocket::{ Server, Message };
use websocket::client::Client;
use websocket::message::Type;
use websocket::header::WebSocketProtocol;
use websocket::receiver::Receiver;
use websocket::sender::Sender;
use websocket::server::Connection;
use websocket::server::request::Request;
use websocket::stream::WebSocketStream;
use websocket::ws::dataframe::DataFrame;

pub trait WebsocketWrapper : Send + Sync {

}

struct GlobalWebsocketWrapper<R: Read, W: Write> {
    receiver: Receiver<R>,
    sender: Sender<W>,
}

impl<R: Read, W: Write> GlobalWebsocketWrapper<R, W> {
    fn new(sender: Sender<W>, receiver: Receiver<R>) -> GlobalWebsocketWrapper<R, W> {
        GlobalWebsocketWrapper {
            sender: sender,
            receiver: receiver
        }
    }
}

impl<R: Read, W: Write> WebsocketWrapper for GlobalWebsocketWrapper<R, W> {

}

pub struct WebsocketServer {
    context: SharedContext
}

impl WebsocketServer {
    pub fn new(context: SharedContext) -> WebsocketServer {
        WebsocketServer { context: context }
    }

    pub fn handle_connection<R: Read, W: Write>(context: SharedContext, request: Request<R, W>) {
        let headers = request.headers.clone();
        println!("===========================================================");
        println!(" url: {}", request.url);
        println!(" websocket headers: {}", headers);
        println!("===========================================================");

        let service = match request.url {
            RequestUri::AbsolutePath(ref p) => { println!("Got an AbsolutePath"); p[1..].to_owned() },
            _ => { "_".to_owned() }
        };

        // Special case, the global endpoint.
        if service == "services" {
            println!("Creating wrapper for global endpoint.");
            let mut response = request.accept();
            if let client = Ok(response.send()) {
                let guard = context.lock().unwrap();
                let (mut sender, mut receiver) = client.split();
                //guard.add_websocket(Arc::new(GlobalWebsocketWrapper::new(Arc::new(sender), Arc::new(receiver))));
            }
            return;
        }

        // Look for a service.
        let guard = context.lock().unwrap();
        match guard.get_service(&service) {
            None => { },
            Some(_) => {
                // We have a service! Create a wrapper for this service.
                let mut response = request.accept();
                return;
            }
        }

        // In all other circumstances, reject the request.
        request.fail();
    }

    pub fn start(&mut self) {
        let context1 = self.context.clone();
        let addrs: Vec<_> =
            context1.lock().unwrap().ws_as_addrs().unwrap().collect();

        let context2 = self.context.clone();
        thread::Builder::new().name("WebsocketServer".to_owned())
                              .spawn(move || {
            let server = Server::bind(addrs[0]).unwrap();
            for connection in server {
                println!("*** WebSocket connection!");
                thread::spawn(move || {
                    if let Ok(c) = connection {
                        if let Ok(cc) = c.read_request() {
                            WebsocketServer::handle_connection(context2.clone(), cc)
                        }
                    }
                });
            }
        }).unwrap();
    }
}

*/
