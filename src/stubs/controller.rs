/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate rand;

use foxbox_core::config_store::ConfigService;
use foxbox_core::profile_service::{ ProfilePath, ProfileService };
use foxbox_core::traits::Controller;
use foxbox_core::upnp::UpnpManager;
use foxbox_users::UsersManager;
use std::vec::IntoIter;
use serde_json;
use std::io;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tls::{ CertificateManager, CertificateRecord, SniSslContextProvider };
use ws;

#[derive(Clone)]
pub struct ControllerStub {
    pub config: Arc<ConfigService>,
    profile_service: Arc<ProfileService>
}

impl ControllerStub {
    pub fn new() -> Self {
        let path = format!("/tmp/{}", rand::random::<i32>());
        let profile_service = ProfileService::new(ProfilePath::Custom(path));
        ControllerStub {
            config: Arc::new(
                ConfigService::new(&profile_service.path_for("foxbox.conf"))
            ),
            profile_service: Arc::new(profile_service)
        }
    }
}

impl Default for ControllerStub {
    fn default() -> Self {
        ControllerStub::new()
    }
}

impl Controller for ControllerStub {
    fn run(&mut self, _: &AtomicBool) {}
    fn adapter_started(&self, _: String) {}
    fn adapter_notification(&self, _: serde_json::value::Value) {}
    fn http_as_addrs(&self) -> Result<IntoIter<SocketAddr>, io::Error> {
        ("localhost", 3000).to_socket_addrs()
    }

    fn ws_as_addrs(&self) -> Result<IntoIter<SocketAddr>, io::Error> {
        ("localhost", 4000).to_socket_addrs()
    }

    fn add_websocket(&mut self, socket: ws::Sender) {}
    fn remove_websocket(&mut self, socket: ws::Sender) {}
    fn broadcast_to_websockets(&self, data: serde_json::value::Value) {}

    fn get_config(&self) -> Arc<ConfigService> {
        self.config.clone()
    }
    fn get_upnp_manager(&self) -> Arc<UpnpManager> {
        Arc::new(UpnpManager::new())
    }
    fn get_users_manager(&self) -> Arc<UsersManager> {
        Arc::new(UsersManager::new(&self.profile_service.path_for("unused")))
    }
    fn get_profile(&self) -> &ProfileService {
        &self.profile_service
    }
    fn get_tls_enabled(&self) -> bool {
        false
    }

    fn get_hostname(&self) -> String {
        String::from("localhost")
    }

    fn get_box_certificate(&self) -> io::Result<CertificateRecord> {
        CertificateRecord::new_from_components("foxbox.local".to_owned(),
                                        PathBuf::from("a/file.pem"),
                                        PathBuf::from("b/file.pem"),
                                        "abcdef".to_owned())
    }

    fn get_certificate_manager(&self) -> CertificateManager {
       CertificateManager::new(PathBuf::from(current_dir!()), Box::new(SniSslContextProvider::new()))
    }
}
