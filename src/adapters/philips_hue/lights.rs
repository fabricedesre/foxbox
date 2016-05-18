/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Module that implements lights for `PhilipsHueAdapter`
//!
//! This module implements AdapterManager-facing functionality.
//! It registers a service for every light and adds setters and
//! getters according to the light type.

use foxbox_taxonomy::api::Error;
use foxbox_taxonomy::manager::*;
use foxbox_taxonomy::services::*;
use foxbox_taxonomy::values::{ Type };
use super::*;
use super::hub_api::HubApi;
use std::collections::HashSet;
use std::sync::{ Arc, Mutex };

const CUSTOM_PROPERTY_MANUFACTURER: &'static str = "manufacturer";
const CUSTOM_PROPERTY_MODEL: &'static str = "model";
const CUSTOM_PROPERTY_NAME: &'static str = "name";
const CUSTOM_PROPERTY_TYPE: &'static str = "type";

#[derive(Clone)]
pub struct Light {
    api: Arc<Mutex<HubApi>>,
    hub_id: String,
    light_id: String,
    service_id: Id<ServiceId>,
    pub get_available_id: Id<Channel>,
    pub get_power_id: Id<Channel>,
    pub set_power_id: Id<Channel>,
    pub get_color_id: Id<Channel>,
    pub set_color_id: Id<Channel>,
}

impl Light {
    pub fn new(api: Arc<Mutex<HubApi>>, hub_id: &str, light_id: &str)
        -> Self
    {
        Light {
            api: api,
            hub_id: hub_id.to_owned(),
            light_id: light_id.to_owned(),
            service_id: create_light_id(&hub_id, &light_id),
            get_available_id: create_getter_id("available", &hub_id, &light_id),
            get_power_id: create_getter_id("power", &hub_id, &light_id),
            set_power_id: create_setter_id("power", &hub_id, &light_id),
            get_color_id: create_getter_id("color", &hub_id, &light_id),
            set_color_id: create_setter_id("color", &hub_id, &light_id),
        }
    }
    pub fn start(&self) {
        // Nothing to do, yet
    }
    pub fn stop(&self) {
        // Nothing to do, yet
    }
    pub fn init_service(&mut self, manager: Arc<AdapterManager>,
        services: LightServiceMap) -> Result<(), Error>
    {
        let adapter_id = create_adapter_id();
        let status = self.api.lock().unwrap().get_light_status(&self.light_id);

        if status.lighttype == "Extended color light" {

            info!("New Philips Hue `Extended Color Light` service for light {} on bridge {}",
                self.light_id, self.hub_id);

            let mut service = Service::empty(self.service_id.clone(), adapter_id.clone());
            service.properties.insert(CUSTOM_PROPERTY_MANUFACTURER.to_owned(),
                status.manufacturername.to_owned());
            service.properties.insert(CUSTOM_PROPERTY_MODEL.to_owned(),
                status.modelid.to_owned());
            service.properties.insert(CUSTOM_PROPERTY_NAME.to_owned(),
                status.name.to_owned());
            service.properties.insert(CUSTOM_PROPERTY_TYPE.to_owned(),
                "Light/ColorLight".to_owned());
            service.tags.insert(tag_id!("type:Light/ColorLight"));

            try!(manager.add_service(service));

            // The `available` getter yields `On` when the light
            // is plugged in and `Off` when it is not. Availability
            // Has no effect on the API other than that you won't
            // see the light change because it lacks external power.
            try!(manager.add_getter(Channel {
                tags: HashSet::new(),
                adapter: adapter_id.clone(),
                id: self.get_available_id.clone(),
                service: self.service_id.clone(),
                kind: ChannelKind::Extension {
                    vendor: Id::new("foxlink@mozilla.com"),
                    adapter: Id::new("Philips Hue Adapter"),
                    kind: Id::new("available"),
                    typ: Type::OnOff,
                },
            }));

            try!(manager.add_getter(Channel {
                tags: HashSet::new(),
                adapter: adapter_id.clone(),
                id: self.get_power_id.clone(),
                service: self.service_id.clone(),
                kind: ChannelKind::LightOn,
            }));

            try!(manager.add_setter(Channel {
                tags: HashSet::new(),
                adapter: adapter_id.clone(),
                id: self.set_power_id.clone(),
                service: self.service_id.clone(),
                kind: ChannelKind::LightOn,
            }));

            try!(manager.add_getter(Channel {
                tags: HashSet::new(),
                adapter: adapter_id.clone(),
                id: self.get_color_id.clone(),
                service: self.service_id.clone(),
                kind: ChannelKind::Extension {
                    vendor: Id::new("foxlink@mozilla.com"),
                    adapter: Id::new("Philips Hue Adapter"),
                    kind: Id::new("Color"),
                    typ: Type::Color,
                },
            }));

            try!(manager.add_setter(Channel {
                tags: HashSet::new(),
                adapter: adapter_id.clone(),
                id: self.set_color_id.clone(),
                service: self.service_id.clone(),
                kind: ChannelKind::Extension {
                    vendor: Id::new("foxlink@mozilla.com"),
                    adapter: Id::new("Philips Hue Adapter"),
                    kind: Id::new("Color"),
                    typ: Type::Color,
                },
            }));

            let mut services_lock = services.lock().unwrap();
            services_lock.getters.insert(self.get_available_id.clone(), self.clone());
            services_lock.getters.insert(self.get_power_id.clone(), self.clone());
            services_lock.setters.insert(self.set_power_id.clone(), self.clone());
            services_lock.getters.insert(self.get_color_id.clone(), self.clone());
            services_lock.setters.insert(self.set_color_id.clone(), self.clone());

        } else if status.lighttype == "Dimmable light" {
            info!("New Philips Hue `Dimmable Light` service for light {} on bridge {}",
                self.light_id, self.hub_id);
            let mut service = Service::empty(self.service_id.clone(), adapter_id.clone());
            service.properties.insert(CUSTOM_PROPERTY_MANUFACTURER.to_owned(),
                status.manufacturername.to_owned());
            service.properties.insert(CUSTOM_PROPERTY_MODEL.to_owned(),
                status.modelid.to_owned());
            service.properties.insert(CUSTOM_PROPERTY_NAME.to_owned(),
                status.name.to_owned());
            service.properties.insert(CUSTOM_PROPERTY_TYPE.to_owned(),
                "Light/DimmerLight".to_owned());
            service.tags.insert(tag_id!("type:Light/DimmerLight"));

            try!(manager.add_service(service));

            try!(manager.add_getter(Channel {
                tags: HashSet::new(),
                adapter: adapter_id.clone(),
                id: self.get_available_id.clone(),
                service: self.service_id.clone(),
                kind: ChannelKind::Extension {
                    vendor: Id::new("foxlink@mozilla.com"),
                    adapter: Id::new("Philips Hue Adapter"),
                    kind: Id::new("available"),
                    typ: Type::OnOff,
                },
            }));

            try!(manager.add_getter(Channel {
                tags: HashSet::new(),
                adapter: adapter_id.clone(),
                id: self.get_power_id.clone(),
                service: self.service_id.clone(),
                kind: ChannelKind::LightOn,
            }));

            try!(manager.add_setter(Channel {
                tags: HashSet::new(),
                adapter: adapter_id.clone(),
                id: self.set_power_id.clone(),
                service: self.service_id.clone(),
                kind: ChannelKind::LightOn,
            }));

            let mut services_lock = services.lock().unwrap();
            services_lock.getters.insert(self.get_available_id.clone(), self.clone());
            services_lock.getters.insert(self.get_power_id.clone(), self.clone());
            services_lock.setters.insert(self.set_power_id.clone(), self.clone());

        } else {
            warn!("Ignoring unsupported Hue light type {}, ID {} on bridge {}",
                status.lighttype, self.light_id, self.hub_id);
        }
        Ok(())
    }

    pub fn get_available(&self) -> bool {
        let status = self.api.lock().unwrap().get_light_status(&self.light_id);
        status.state.reachable
    }

    pub fn get_power(&self) -> bool {
        let status = self.api.lock().unwrap().get_light_status(&self.light_id);
        status.state.on
    }

    pub fn set_power(&self, on: bool) {
        self.api.lock().unwrap().set_light_power(&self.light_id, on);
    }

    #[allow(dead_code)]
    pub fn get_brightness(&self) -> f64 {
        // Hue API gives brightness value in [0, 254]
        let ls = self.api.lock().unwrap().get_light_status(&self.light_id);
        ls.state.bri as f64 / 254f64
    }

    #[allow(dead_code)]
    pub fn set_brightness(&self, bri: f64) {
        // Hue API takes brightness value in [0, 254]
        let bri = bri.max(0f64).min(1f64); // [0,1]

        // convert to value space used by Hue
        let bri: u32 = (bri * 254f64) as u32;

        self.api.lock().unwrap().set_light_brightness(&self.light_id, bri);
    }

    pub fn get_color(&self) -> (f64, f64, f64) {
        // Hue API gives hue angle in [0, 65535], and sat and val in [0, 254]
        let ls = self.api.lock().unwrap().get_light_status(&self.light_id);
        let hue: f64 = ls.state.hue.unwrap_or(0) as f64 / 65536f64 * 360f64;
        let sat: f64 = ls.state.sat.unwrap_or(0) as f64 / 254f64;
        let val: f64 = ls.state.bri as f64 / 254f64;
        (hue, sat, val)
    }

    pub fn set_color(&self, hsv: (f64, f64, f64)) {
        // Hue API takes hue angle in [0, 65535], and sat and val in [0, 254]
        let (hue, sat, val) = hsv;
        let hue = ((hue % 360f64) + 360f64) % 360f64; // [0,360)
        let sat = sat.max(0f64).min(1f64); // [0,1]
        let val = val.max(0f64).min(1f64); // [0,1]

        // convert to value space used by Hue
        let hue: u32 = (hue * 65536f64 / 360f64) as u32;
        let sat: u32 = (sat * 254f64) as u32;
        let val: u32 = (val * 254f64) as u32;

        self.api.lock().unwrap().set_light_color(&self.light_id, (hue, sat, val));
    }
}
