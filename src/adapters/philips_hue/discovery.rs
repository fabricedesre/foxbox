/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Discovery for Philips Hue
//!
//! This module is a collection of discovery mechanisms for the PhilipsHueAdapter.
//! As of now it supports `UPnP`, and `nUPnP` – Philips' proprietary HTTP-based
//! discovery mechanism.
//!
//! Hue bridges also announce themselves via `mDNS`, but Foxbox does not support
//! this yet.

pub extern crate url;

use foxbox_core::traits::Controller;
use foxbox_core::upnp::{ UpnpListener, UpnpManager, UpnpService };
use serde_json;
use std::sync::{ Arc, Mutex };
use std::thread;
use super::{ HueAction, http, PhilipsHueAdapter };
use transformable_channels::mpsc::*;

static UPNP_MODEL_PATH: &'static str = "/root/device/modelName";
static UPNP_MODEL_NAME: &'static str = "Philips hue bridge";

pub struct Discovery<C> {
    adapter: PhilipsHueAdapter<C>,
    upnp_manager: Arc<Mutex<Arc<UpnpManager>>>,
}

impl<C: Controller> Discovery<C> {
    pub fn new(adapter: PhilipsHueAdapter<C>) -> Self
    {
        let upnp = adapter.controller.get_upnp_manager();
        let listener = PhilipsHueUpnpListener::new(adapter.clone());
        upnp.add_listener("PhilipsHueTaxonomy".to_owned(), listener);
        Discovery {
            adapter: adapter,
            upnp_manager: Arc::new(Mutex::new(upnp)),
        }
    }

    pub fn do_discovery(&self) {
        self.do_nupnp_discovery();
        self.do_upnp_discovery();
    }

    pub fn do_nupnp_discovery(&self) {
        let controller = self.adapter.controller.clone();
        let tx = self.adapter.tx.clone();
        thread::spawn(move || {
            let nupnp_enabled = controller.get_config().get_or_set_default(
                "philips_hue", "nupnp_enabled", "true");
            if nupnp_enabled == "true" {
                let nupnp_url = controller.get_config().get_or_set_default(
                    "philips_hue", "nupnp_url", "https://www.meethue.com/api/nupnp");
                let nupnp_hubs = nupnp_query(&nupnp_url);
                for nupnp in nupnp_hubs {
                    let _ = tx.lock().unwrap().send(HueAction::AddHub(nupnp.id.to_owned(),
                                nupnp.internalipaddress.to_owned()));
                }
            }
        });
    }

    pub fn do_upnp_discovery(&self) {
        let upnp = self.upnp_manager.lock().unwrap();
        // TODO: Still wondering which one of these triggers a bridge response.
        // It works without the search, but in this case we need to assume that
        // the Hue bridges might be triggered by some other adapter's search queries.
        // upnp.search(Some("urn:schemas-upnp-org:device:basic:1".to_owned())).unwrap();
        // upnp.search(Some("urn:schemas-upnp-org:device:Basic:1".to_owned())).unwrap();
        // upnp.search(Some("urn:schemas-upnp-org:device:libhue:idl".to_owned())).unwrap();
        // upnp.search(Some("upnp:rootdevice".to_owned())).unwrap();
        // upnp.search(Some("uuid:2f402f80-da50-11e1-9b23-00178825681a".to_owned())).unwrap();
        upnp.search(None).unwrap();  // Trigger a search for "ssdp:all"
    }
}

pub struct PhilipsHueUpnpListener<C> {
    adapter: PhilipsHueAdapter<C>
}

impl<C: Controller> PhilipsHueUpnpListener<C> {
    pub fn new(adapter: PhilipsHueAdapter<C>) -> Box<Self> {
        Box::new(PhilipsHueUpnpListener {
            adapter: adapter
        })
    }
}

impl<C: Controller> UpnpListener for PhilipsHueUpnpListener<C> {
    // This is called every time the device advertises itself via UPnP.
    // A Philips Hue brisge posts an advertisement about a minute after search.
    fn upnp_discover(&self, service: &UpnpService) -> bool {
        macro_rules! try_get {
            ($hash:expr, $key:expr) => (match $hash.get($key) {
                Some(val) => val,
                None => return false
            })
        }

        let model_name = try_get!(service.description, UPNP_MODEL_PATH);

        if !model_name.starts_with(UPNP_MODEL_NAME) {
            return false;
        }

        let serial = try_get!(service.description, "/root/device/serialNumber");
        let url = try_get!(service.description, "/root/URLBase");
        let model = try_get!(service.description, "/root/device/modelNumber");

        debug!("UPnP announcement for Philips Hue bridge serial {} model {} at {}",
            serial, model, url);

        if !url.starts_with("http://") {
            debug!("Unsupported URL scheme, expected HTTP.");
            return false;
        }

        if !url.ends_with(":80/") {
            debug!("Unsupported HTTP port, expected 80.");
            return false;
        }

        let ip: &str = url.rsplit("//").nth(0).unwrap_or("");
        let ip: &str = ip.split(':').nth(0).unwrap_or("");

        // CAVE: assuming raw IPv4 addresses
        if ip.len() < 7 || ip.len() > 15 {
            debug!("Unexpected IP address format");
        }

        let id;
        if serial.len() == 12 {
            // Turn serial into actual Hue ID by inserting "fffe" mid string
            let prefix = &serial[0..6];
            let suffix = &serial[6..];
            id = format!("{}fffe{}", prefix, suffix);
        } else {
            id = serial.clone();
        }

        let tx = self.adapter.tx.lock().unwrap();
        let _ = tx.send(HueAction::AddHub(id.to_owned(), ip.to_owned()));

        true
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NupnpEntry {
    id: String,
    internalipaddress: String
}

pub fn nupnp_query(server_url: &str) -> Vec<NupnpEntry> {
    // "[{\"id\":\"001788fffe243755\",\"internalipaddress\":\"192.168.5.129\"}]"
    debug!("Querying NUPnP server at {}", server_url);
    let empty_list = Vec::new();
    let nupnp_list = http::get(server_url)
        .map(parse_nupnp_response)
        .unwrap_or(empty_list);
    debug!("Parsed NUPnP response: {:?}", nupnp_list);
    nupnp_list
}

fn parse_nupnp_response(content: String) -> Vec<NupnpEntry> {
    match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(error) => {
            warn!("Unable to parse NUPnP response: {}", error.to_string());
            Vec::<NupnpEntry>::new()
        }
    }
}

#[test]
fn nupnp_results_are_properly_parsed() {
    let json = r#"[{"id":"001788","internalipaddress":"192.168.5.129"}]"#;
    let res: Vec<NupnpEntry> = parse_nupnp_response(json.to_owned());
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "001788");
    assert_eq!(res[0].internalipaddress, "192.168.5.129");
}

#[test]
fn nupnp_result_empty_on_error() {
    let broken_json = r#"[{"id":"001788","internalipa"#;
    let res: Vec<NupnpEntry> = parse_nupnp_response(broken_json.to_owned());
    assert_eq!(res.len(), 0);
}
