/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! `UPnP` listener for IP camera.
//!

extern crate url;

use std::sync::Arc;

use foxbox_core::config_store::ConfigService;
use foxbox_core::upnp::{UpnpListener, UpnpService};
use foxbox_taxonomy::manager::*;

use super::IPCameraAdapter;
use super::IPCameraDescription;
use super::IpCameraServiceMap;

pub struct IpCameraUpnpListener {
    manager: Arc<AdapterManager>,
    services: IpCameraServiceMap,
    config: Arc<ConfigService>,
}

impl IpCameraUpnpListener {
    pub fn new(manager: &Arc<AdapterManager>, services: IpCameraServiceMap, config: &Arc<ConfigService>) -> Box<Self> {
        Box::new(IpCameraUpnpListener {
            manager: manager.clone(),
            services: services,
            config: config.clone(),
        })
    }
}

impl UpnpListener for IpCameraUpnpListener {
    // This will called each time that the device advertises itself using UPNP.
    // The D-Link cameras post an advertisement once when we do our search
    // (when the adapter is started) and 4 times in a row about once every
    // 3 minutes when they're running.
    fn upnp_discover(&self, service: &UpnpService) -> bool {
        macro_rules! try_get {
            ($hash:expr, $key:expr) => (match $hash.get($key) {
                Some(val) => val,
                None => return false
            })
        }

        let model_name = try_get!(service.description, "/root/device/modelName");
        let known_models = ["DCS-5010L", "DCS-5020L", "DCS-5025L", "Link-IpCamera"];
        let model_name_str: &str = model_name;
        if !known_models.contains(&model_name_str) {
            return false;
        }

        let url = try_get!(service.description, "/root/device/presentationURL");

        // The UDN is typically of the for uuid:SOME-UID-HERE, but some devices
        // response with just a UUID. We strip off the uuid: prefix, if it exists
        // and use the resulting UUID as the service id.
        let udn = try_get!(service.description, "/root/device/UDN")
            .trim_left_matches("uuid:")
            .to_owned();

        // TODO: We really need to update the IP/camera name in the event that
        //       it changed. I'll add this once we start persisting the camera
        //       information in a database.

        let name = try_get!(service.description, "/root/device/friendlyName").clone();
        let manufacturer = try_get!(service.description, "/root/device/manufacturer");

        let camera = IPCameraDescription {
            udn: udn,
            url: url.to_owned(),
            manufacturer: manufacturer.to_owned(),
            model_name: model_name.to_owned(),
            name: name,
        };
        IPCameraAdapter::init_service(&self.manager, self.services.clone(), &self.config,
                                      camera).unwrap();
        true
    }
}
