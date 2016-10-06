/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use foxbox_core::managed_process::ManagedProcess;

// Assumes Unix
use std::process::{ Child, Command };
use std::io::Result;
use url::{ SchemeData, Url };

pub type TunnelProcess = ManagedProcess;

pub struct Tunnel {
    config: TunnelConfig,
    pub tunnel_process: Option<TunnelProcess>
}

#[derive(Clone, Debug)]
pub struct TunnelConfig {
    /// The socket address that the box connects to to establish the tunnel.
    tunnel_url: Url,
    tunnel_secret: String,
    local_http_port: u16,
    local_ws_port: u16,
    remote_name: String
}

impl TunnelConfig {
    pub fn new(tunnel_url: String,
               tunnel_secret: String,
               local_http_port: u16,
               local_ws_port: u16,
               remote_name: String) -> Self {

        let tunnel_url = match Url::parse(&tunnel_url) {
            Ok(url) => {
                match url.scheme_data {
                    SchemeData::Relative(..) => {
                        url
                    },
                    SchemeData::NonRelative(..) => {
                        // We don't care about the scheme, we just want domain
                        // and port, but Url does not parse them properly without
                        // the scheme, so we append a fake 'http'.
                        match Url::parse(&format!("http://{}", tunnel_url)) {
                            Ok(url) => url,
                            Err(err) => {
                                error!("Could not parse tunnel url.
                                        Try something like knilxof.org:443");
                                panic!(err)
                            }
                        }
                    },
                }
            },
            Err(err) => {
                error!("Could not parse tunnel url.
                        Try something like knilxof.org:443");
                panic!(err)
            }
        };

        TunnelConfig {
            tunnel_url: tunnel_url,
            tunnel_secret: tunnel_secret,
            local_http_port: local_http_port,
            local_ws_port: local_ws_port,
            remote_name: remote_name
        }
    }

    /// Describes how to spawn the pagekite process.
    /// pagekite requires a user (remote_name) and a shared secret to be able
    /// to connect us with the bridge. For the first prototype we will have a
    /// secret common to all boxes, but in the end we will need a secret per
    /// box. Unfortunately pagekite does not provide a way to add a new
    /// domain/secret pair while the bridge is running, but it provides the
    /// possibility to delegate the authentication to a dynamic DNS server.
    /// XXX We will move to DNS authentication after the first prototype if
    /// we keep using pagekite.
    /// https://github.com/fxbox/foxbox/issues/177#issuecomment-194778308
    pub fn spawn(&self) -> Result<Child> {
        let domain = match self.tunnel_url.domain() {
            Some(domain) => domain,
            None => {
                panic!("No tunnel domain found. Cannot start tunneling");
            }
        };

        let port = match self.tunnel_url.port() {
            Some(port) => port,
            None => {
                panic!("No tunnel port found. Cannot start tunneling");
            }
        };

        let res = Command::new("pagekite")
                .arg(format!("--frontend={}", format!("{}:{}", domain, port)))
                // XXX remove http service once we support https
                .arg(format!("--service_on=http,https:{}:localhost:{}:{}",
                             self.remote_name,
                             self.local_http_port,
                             self.tunnel_secret))
                .arg(format!("--service_on=websocket:{}:localhost:{}:{}",
                             self.remote_name,
                             self.local_ws_port,
                             self.tunnel_secret))
                .spawn();
        if res.is_err() {
            error!("Failed to launch pagekite.py, check that it's installed and in your $PATH.");
        }
        res
    }
}

impl Tunnel {
    /// Create a new Tunnel object containing the necessary
    /// configuration
    pub fn new(config: TunnelConfig) -> Tunnel {
        Tunnel {
            config: config,
            tunnel_process: None
        }
    }

    /// Start the Tunnel process if it has not already been started
    pub fn start(&mut self) -> Result<()> {
        if let Some(_) = self.tunnel_process {
            // Already started
            Ok(())
        } else {
            let tunnel_config = self.config.clone();

            self.tunnel_process = Some(ManagedProcess::start(move || {
                tunnel_config.spawn()
            }).unwrap());

            Ok(())
        }
    }

    /// Stop the tunnel process if it is runnnig
    pub fn stop(&mut self) -> Result<()> {
        match self.tunnel_process.take() {
            None => Ok(()),
            Some(process) => process.shutdown()
        }
    }

    pub fn get_frontend_name(&self) -> Option<String> {
        match self.config.tunnel_url.host() {
            Some(host) => Some(host.to_string()),
            None => None
        }
    }
}
