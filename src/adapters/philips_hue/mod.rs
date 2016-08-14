/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! The `PhilipsHueAdapter`
//!
//! This adapter implements support for Philips Hue bridges.

// Clippy complains about `hub_id` and `hub_ip` being too similar,
// suggests renaming to `hub_i_p`.
#![allow(clippy)]

pub mod discovery;
pub mod http;
pub mod hub;
pub mod hub_api;
pub mod lights;
pub mod structs;

use foxbox_core::traits::Controller;
use foxbox_taxonomy::api::{ Error, InternalError, User };
use foxbox_taxonomy::channel::*;
use foxbox_taxonomy::manager::*;
use foxbox_taxonomy::services::*;
use foxbox_taxonomy::values::{ Color, OnOff, Value };

use std::collections::HashMap;
use std::sync::{ Arc, Mutex };
use std::thread;
use self::hub::Hub;
use self::lights::Light;
use transformable_channels::mpsc::*;

static ADAPTER_NAME: &'static str = "Philips Hue adapter (built-in)";
static ADAPTER_VENDOR: &'static str = "team@link.mozilla.org";
static ADAPTER_VERSION: [u32;4] = [0, 0, 0, 0];

/// Philips Hue Adapter's main loop handles messages of these types.
#[allow(dead_code)]
pub enum HueAction {
    TriggerDiscovery,
    AddHub(String, String),         // Hub id, hub ip
    AddLight(String, String),       // Hub id, light id
    RemoveHub(String),              // Hub id
    RemoveLight(String, String),    // Hub id, light id
    StopAdapter,
}

pub type LightServiceMap = Arc<Mutex<LightServiceMapInternal>>;

pub struct LightServiceMapInternal {
    getters: HashMap<Id<Channel>, Light>,
    setters: HashMap<Id<Channel>, Light>,
}

#[derive(Clone)]
pub struct PhilipsHueAdapter<C> {
    /// A reference to the AdapterManager.
    manager: Arc<AdapterManager>,

    controller: C,

    services: LightServiceMap,

    /// Tx channel for sending messages to the adapter's main loop.
    tx: Arc<Mutex<RawSender<HueAction>>>,

    /// The ID of this adapter (permanently fixed)
    adapter_id: Id<AdapterId>,
}

impl<C: Controller> PhilipsHueAdapter<C> {
    #[allow(dead_code)]
    pub fn init(manager: &Arc<AdapterManager>, controller: C) -> Result<(), Error> {
        let services = Arc::new(Mutex::new(LightServiceMapInternal {
            getters: HashMap::new(),
            setters: HashMap::new(),
        }));

        let (tx, rx) = channel();

        let adapter = PhilipsHueAdapter {
            manager: manager.clone(),
            controller: controller.clone(),
            services: services.clone(),
            tx: Arc::new(Mutex::new(tx.clone())),
            adapter_id: create_adapter_id(),
        };

        try!(manager.add_adapter(Arc::new(adapter.clone())));

        // Trigger discovery
        let _ = tx.send(HueAction::TriggerDiscovery);

        let manager = manager.clone();
        let services = services.clone();

        thread::spawn(move || {
            debug!("Starting Philips Hue Adapter main thread");

            let mut hubs: HashMap<String, Arc<Mutex<Hub<C>>>> = HashMap::new();
            let mut lights: HashMap<String, Arc<Mutex<Light>>> = HashMap::new();

            let discovery = discovery::Discovery::new(adapter.clone());

            'recv: for action in rx {
                match action {
                    HueAction::TriggerDiscovery => {
                        debug!("HueAction::TriggerDiscovery received");
                        discovery.do_discovery();
                    },
                    HueAction::AddHub(hub_id, hub_ip) => {
                        debug!("HueAction::AddHub({},{}) received", hub_id, hub_ip);
                        let is_known_hub = hubs.contains_key(&hub_id);
                        if is_known_hub {
                            let mut hub = hubs.get_mut(&hub_id).unwrap().lock().unwrap();
                            hub.update_ip(&hub_ip);
                        } else {
                            let new_hub = Hub::new(adapter.clone(), &hub_id, &hub_ip);
                            new_hub.start();
                            hubs.insert(hub_id, Arc::new(Mutex::new(new_hub)));
                        }
                    },
                    HueAction::AddLight(hub_id, light_id) => {
                        debug!("HueAction::AddLight({},{}) received", hub_id, light_id);
                        let id = format!("{}::{}", hub_id, light_id);
                        let is_known_light = lights.contains_key(&id);
                        if is_known_light {
                            warn!("Ignoring request to add pre-existing Hue light");
                        } else {
                            // TODO: check if hub is known
                            let hub = hubs.get(&hub_id).unwrap().lock().unwrap();
                            let mut new_light: Light = Light::new(hub.api.clone(), &hub_id,
                                &light_id);
                            let _ = new_light.init_service(manager.clone(), services.clone());
                            new_light.start();
                            lights.insert(id, Arc::new(Mutex::new(new_light)));
                        }
                    },
                    // Currently unused
                    HueAction::RemoveHub(hub_id) => {
                        debug!("HueAction::RemoveHub({}) received", hub_id);
                        let is_known_hub = hubs.contains_key(&hub_id);
                        if is_known_hub {
                            let _ = hubs.remove(&hub_id);
                        } else {
                            warn!("Ignoring request to remove unknown Hue hub");
                        }
                    },
                    // Currently unused
                    HueAction::RemoveLight(hub_id, light_id) => {
                        debug!("HueAction::RemoveLight({},{}) received", hub_id, light_id);
                        let id = format!("{}::{}", hub_id, light_id);
                        let is_known_light = lights.contains_key(&id);
                        if is_known_light {
                            let _ = lights.remove(&id);
                        } else {
                            warn!("Ignoring request to remove unknown Hue hub");
                        }
                    },
                    // TODO: Currently unused, but required for teardown
                    HueAction::StopAdapter => {
                        debug!("HueAction::StopAdapter received");
                        break;
                    },
                }
            }

            debug!("Stopping Philips Hue Adapter main thread.");

            for (_, light) in lights.drain() {
                light.lock().unwrap().stop();
            }

            for (_, hub) in hubs.drain() {
                hub.lock().unwrap().stop();
            }

        });

        Ok(())
    }

    // TODO: This should be called on tear-down by the adapter manager or
    // the taxonomy manager, but currently it can't be because the adapter
    // manager is never given a reference and .stop() is not part of a
    // trait shared amongst adapters.
    #[allow(dead_code)]
    pub fn stop(&self) {
        let _ = self.tx.lock().unwrap().send(HueAction::StopAdapter);
    }

    pub fn send(&self, action: HueAction) {
        let _ = self.tx.lock().unwrap().send(action);
    }
}

pub fn create_adapter_id() -> Id<AdapterId> {
    Id::new("philips_hue@link.mozilla.org")
}

pub fn create_light_id(hub_id: &str, light_id: &str) -> Id<ServiceId> {
    Id::new(&format!("service:{}.{}.{}", light_id, hub_id, create_adapter_id()))
}

pub fn create_channel_id(op: &str, hub_id: &str, light_id: &str) -> Id<Channel> {
    Id::new(&format!("channel:{}.{}.{}.{}", op, light_id, hub_id, create_adapter_id()))
}

impl<C: Controller> Adapter for PhilipsHueAdapter<C> {
    fn id(&self) -> Id<AdapterId> {
        create_adapter_id()
    }

    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    fn vendor(&self) -> &str {
        ADAPTER_VENDOR
    }

    fn version(&self) -> &[u32; 4] {
        &ADAPTER_VERSION
    }

    fn fetch_values(&self, mut set: Vec<Id<Channel>>, _: User)
        -> ResultMap<Id<Channel>, Option<Value>, Error>
    {
        set.drain(..).map(|id| {
            let light = match self.services.lock().unwrap().getters.get(&id) {
                Some(light) => light.clone(),
                None => return (id.clone(), Err(Error::Internal(InternalError::NoSuchChannel(id))))
            };

            if id == light.get_available_id {
                if light.get_available() {
                    return (id, Ok(Some(Value::new(OnOff::On))));
                } else {
                    return (id, Ok(Some(Value::new(OnOff::Off))));
                }
            }
            if id == light.channel_power_id {
                if light.get_power() {
                    return (id, Ok(Some(Value::new(OnOff::On))));
                } else {
                    return (id, Ok(Some(Value::new(OnOff::Off))));
                }
            }
            if id == light.channel_color_id {
                let (h, s, v) = light.get_color();
                return (id, Ok(Some(Value::new(Color::HSV(h, s, v)))));
            }

            (id.clone(), Err(Error::Internal(InternalError::NoSuchChannel(id))))
        }).collect()
    }

    fn send_values(&self, mut values: HashMap<Id<Channel>, Value>, _: User)
        -> ResultMap<Id<Channel>, (), Error>
    {
        values.drain().map(|(id, value)| {
            let light = match self.services.lock().unwrap().setters.get(&id) {
                Some(light) => light.clone(),
                None => return (id.clone(), Err(Error::Internal(InternalError::NoSuchChannel(id))))
            };

            if id == light.channel_power_id {
                match value.cast::<OnOff>() {
                    Ok(&OnOff::On)  => { light.set_power(true); },
                    Ok(&OnOff::Off) => { light.set_power(false); },
                    Err(err) => return (id, Err(err))
                }
                return (id, Ok(()));
            }
            if id == light.channel_color_id {
                match value.cast::<Color>() {
                    Ok(&Color::HSV(ref h, ref s, ref v)) => { light.set_color((h.clone(), s.clone(), v.clone())); },
                    Err(err) => return (id, Err(err))
                }
                return (id, Ok(()));
            }

            (id.clone(), Err(Error::Internal(InternalError::NoSuchChannel(id))))
        }).collect()
    }
}
