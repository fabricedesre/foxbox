/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use hyper::error::Error as HyperError;
use hyper::net::{ HttpsListener, Openssl, Ssl };
use hyper::server::Server;
use iron::{ Protocol, ServerFactory };
use std::net::SocketAddr;
use std::sync::Arc;
use certificate_manager::CertificateManager;

pub struct SniServerFactory<S: Ssl + Clone + Send> {
    ssl: S
}

impl SniServerFactory<Openssl> {
    pub fn new(ssl: &mut CertificateManager) -> Self {
        SniServerFactory {
            ssl: Openssl {
                context: Arc::new(ssl.get_context_provider().context().unwrap())
            }
        }
    }
}

impl ServerFactory<HttpsListener<Openssl>> for SniServerFactory<Openssl> {
    fn protocol(&self) -> Protocol {
        Protocol::Https
    }

    fn create_server(&self, sock_addr: SocketAddr)
        -> Result<Server<HttpsListener<Openssl>>, HyperError> {
        Server::https(sock_addr, self.ssl.clone())
    }
}
