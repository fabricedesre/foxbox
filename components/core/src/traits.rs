/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use broker::SharedBroker;
use config_store::ConfigService;
use core::marker::Reflect;
use foxbox_users::UsersManager;
use jsworkers::Message;
use profile_service::ProfileService;
use serde_json;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::vec::IntoIter;
use tls::{ CertificateRecord, CertificateManager };
use upnp::UpnpManager;
use ws;

pub trait Controller : Send + Sync + Clone + Reflect + 'static {
    fn run(&mut self, shutdown_flag: &AtomicBool);
    fn adapter_started(&self, adapter: String);
    fn adapter_notification(&self, notification: serde_json::value::Value);
    fn http_as_addrs(&self) -> Result<IntoIter<SocketAddr>, io::Error>;
    fn ws_as_addrs(&self) -> Result<IntoIter<SocketAddr>, io::Error>;

    fn get_tls_enabled(&self) -> bool;
    fn get_certificate_manager(&self) -> CertificateManager;
    fn get_box_certificate(&self) -> io::Result<CertificateRecord>;
    fn get_hostname(&self) -> String;

    fn add_websocket(&mut self, socket: ws::Sender);
    fn remove_websocket(&mut self, socket: ws::Sender);
    fn broadcast_to_websockets(&self, data: serde_json::value::Value);

    fn get_config(&self) -> Arc<ConfigService>;
    fn get_upnp_manager(&self) -> Arc<UpnpManager>;
    fn get_users_manager(&self) -> Arc<UsersManager>;
    fn get_profile(&self) -> &ProfileService;
    fn get_jsworkers_broker(&self) -> SharedBroker<Message>;
}
