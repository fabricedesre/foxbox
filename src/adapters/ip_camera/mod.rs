/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! An adapter providing access to IP cameras. Currently only the following IP cameras are
//! supported: `DLink DCS-5010L`, `DLink DCS-5020L` and `DLink DCS-5025`.
//!

extern crate serde_json;

mod api;
mod upnp_listener;

use foxbox_core::config_store::ConfigService;
use foxbox_core::traits::Controller;
use foxbox_taxonomy::api::{Error, InternalError, User};
use foxbox_taxonomy::channel::*;
use foxbox_taxonomy::manager::*;
use foxbox_taxonomy::services::*;
use foxbox_taxonomy::values::{ Binary, Json, Value };
use foxbox_taxonomy::values::format;
use self::api::*;
use self::upnp_listener::IpCameraUpnpListener;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const CUSTOM_PROPERTY_MANUFACTURER: &'static str = "manufacturer";
const CUSTOM_PROPERTY_MODEL: &'static str = "model";
const CUSTOM_PROPERTY_NAME: &'static str = "name";
const CUSTOM_PROPERTY_URL: &'static str = "url";
const CUSTOM_PROPERTY_UDN: &'static str = "udn";

static ADAPTER_NAME: &'static str = "IP Camera adapter";
static ADAPTER_VENDOR: &'static str = "team@link.mozilla.org";
static ADAPTER_VERSION: [u32; 4] = [0, 0, 0, 0];
static SNAPSHOT_DIR: &'static str = "snapshots";

pub type IpCameraServiceMap = Arc<Mutex<IpCameraServiceMapInternal>>;

pub struct IpCameraServiceMapInternal {
    getters: HashMap<Id<Channel>, Arc<IpCamera>>,
    setters: HashMap<Id<Channel>, Arc<IpCamera>>,
    snapshot_root: String,
}

pub struct IPCameraAdapter {
    services: IpCameraServiceMap,
}

pub struct IPCameraDescription {
    udn: String,
    url: String,
    name: String,
    manufacturer: String,
    model_name: String,
}

impl IPCameraAdapter {
    pub fn id() -> Id<AdapterId> {
        Id::new("ip-camera@link.mozilla.org")
    }

    pub fn init<C>(adapt: &Arc<AdapterManager>, controller: C) -> Result<(), Error>
        where C: Controller
    {
        let services = Arc::new(Mutex::new(IpCameraServiceMapInternal {
            getters: HashMap::new(),
            setters: HashMap::new(),
            snapshot_root: controller.get_profile().path_for(SNAPSHOT_DIR),
        }));
        let ip_camera_adapter = Arc::new(IPCameraAdapter {
            services: services.clone(),
        });

        try!(adapt.add_adapter(ip_camera_adapter));

        // The UPNP listener will add camera service for discovered cameras
        let upnp = controller.get_upnp_manager();
        let listener = IpCameraUpnpListener::new(adapt, services, &controller.get_config());
        upnp.add_listener("IpCameraTaxonomy".to_owned(), listener);

        // The UPNP service searches for ssdp:all which the D-Link cameras
        // don't seem to respond to. So we search for this instead, which
        // they do respond to.
        upnp.search(Some("urn:cellvision:service:Null:1".to_owned())).unwrap();
        Ok(())
    }

    pub fn init_service(adapt: &Arc<AdapterManager>, services: IpCameraServiceMap, config: &Arc<ConfigService>,
        description: IPCameraDescription) -> Result<(), Error>
    {
        let service_id = create_service_id(&description.udn);

        let adapter_id = Self::id();
        let mut service = Service::empty(&service_id, &adapter_id);

        service.properties.insert(CUSTOM_PROPERTY_MANUFACTURER.to_owned(),
                                           description.manufacturer.clone());
        service.properties.insert(CUSTOM_PROPERTY_MODEL.to_owned(), description.model_name.clone());
        service.properties.insert(CUSTOM_PROPERTY_NAME.to_owned(), description.name.clone());
        service.properties.insert(CUSTOM_PROPERTY_URL.to_owned(), description.url.clone());
        service.properties.insert(CUSTOM_PROPERTY_UDN.to_owned(), description.udn.clone());
        service.tags.insert(tag_id!(&format!("name:{}", description.name)));

        // Since the upnp_discover will be called about once very 3 minutes we want to ignore
        // discoveries if the camera is already registered.
        if let Err(error) = adapt.add_service(service) {
            if let Error::Internal(ref internal_error) = error {
                if let InternalError::DuplicateService(_) = *internal_error {
                    debug!("Found {} @ {} UDN {} (ignoring since it already exists)",
                           description.model_name,
                           description.url,
                           description.udn);
                    return Ok(());
                }
            }

            panic!(error);
        }

        info!("Adding IpCamera {} Manufacturer: {} Model: {} Name: {}",
              description.udn,
              description.manufacturer,
              description.model_name,
              description.name);

        let getter_image_list_id = create_channel_id("image_list", &description.udn);
        try!(adapt.add_channel(Channel {
            feature: Id::new("camera/x-image-list"),
            supports_fetch: Some(Signature::returns(Maybe::Required(format::JSON.clone()))),
            id: getter_image_list_id.clone(),
            service: service_id.clone(),
            adapter: adapter_id.clone(),
            ..Channel::default()
        }));

        let getter_image_newest_id = create_channel_id("image_newest", &description.udn);
        try!(adapt.add_channel(Channel {
            feature: Id::new("camera/x-latest-image"),
            supports_fetch: Some(Signature::returns(Maybe::Required(format::BINARY.clone()))),
            id: getter_image_newest_id.clone(),
            service: service_id.clone(),
            adapter: adapter_id.clone(),
            ..Channel::default()
        }));

        let setter_snapshot_id = create_channel_id("snapshot", &description.udn);
        try!(adapt.add_channel(Channel {
            feature: Id::new("camera/store-snapshot"),
            supports_send: Some(Signature::returns(Maybe::Nothing)),
            id: setter_snapshot_id.clone(),
            service: service_id.clone(),
            adapter: adapter_id.clone(),
            ..Channel::default()
        }));

        let channel_username_id = create_channel_id("username", &description.udn);
        try!(adapt.add_channel(Channel {
            id: channel_username_id.clone(),
            service: service_id.clone(),
            adapter: adapter_id.clone(),
            ..USERNAME.clone()
        }));

        let channel_password_id = create_channel_id("password", &description.udn);
        try!(adapt.add_channel(Channel {
            id: channel_password_id.clone(),
            service: service_id.clone(),
            adapter: adapter_id.clone(),
            ..PASSWORD.clone()
        }));

        let mut serv = services.lock().unwrap();
        let camera_obj = try!(IpCamera::new(&description.udn, &description.url, &description.name, &serv.snapshot_root, config));
        let camera = Arc::new(camera_obj);
        serv.getters.insert(getter_image_list_id, camera.clone());
        serv.getters.insert(getter_image_newest_id, camera.clone());
        serv.setters.insert(setter_snapshot_id, camera.clone());
        serv.getters.insert(channel_username_id.clone(), camera.clone());
        serv.setters.insert(channel_username_id, camera.clone());
        serv.getters.insert(channel_password_id.clone(), camera.clone());
        serv.setters.insert(channel_password_id, camera.clone());

        Ok(())
    }
}

impl Adapter for IPCameraAdapter {
    fn id(&self) -> Id<AdapterId> {
        Self::id()
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

    fn fetch_values(&self,
                    mut set: Vec<Id<Channel>>,
                    _: User)
                    -> ResultMap<Id<Channel>, Option<Value>, Error> {
        set.drain(..).map(|id| {
            let camera = match self.services.lock().unwrap().getters.get(&id) {
                Some(camera) => camera.clone(),
                None => return (id.clone(), Err(Error::Internal(InternalError::NoSuchChannel(id))))
            };

            if id == camera.username_id {
                let rsp = camera.get_username();
                return (id, Ok(Some(Value::new(rsp))));
            }

            if id == camera.password_id {
                let rsp = camera.get_password();
                return (id, Ok(Some(Value::new(rsp))));
            }

            if id == camera.image_list_id {
                let rsp = camera.get_image_list();
                return (id, Ok(Some(Value::new(Json(serde_json::to_value(&rsp))))));
            }

            if id == camera.image_newest_id {
                return match camera.get_newest_image() {
                    Ok(rsp) => (id, Ok(Some(Value::new(Binary {
                        data: rsp,
                        mimetype: Id::new("image/jpeg")
                    })))),
                    Err(err) => (id, Err(err))
                };
            }

            (id.clone(), Err(Error::Internal(InternalError::NoSuchChannel(id))))
        }).collect()
    }

    fn send_values(&self, mut values: HashMap<Id<Channel>, Value>, _: User) -> ResultMap<Id<Channel>, (), Error> {
        values.drain().map(|(id, value)| {
            let camera = match self.services.lock().unwrap().setters.get(&id) {
                Some(camera) => camera.clone(),
                None => { return (id, Err(Error::Internal(InternalError::InvalidInitialService))); }
            };

            if id == camera.username_id {
                return match value.cast::<String>() {
                    Ok(username) => {
                        camera.set_username(username);
                        (id, Ok(()))
                    }
                    Err(err) => (id, Err(err))
                }
            }

            if id == camera.password_id {
                return match value.cast::<String>() {
                    Ok(password) => {
                        camera.set_password(password);
                        (id, Ok(()))
                    }
                    Err(err) => (id, Err(err))
                }
            }

            if id == camera.snapshot_id {
                return match camera.take_snapshot() {
                    Ok(_) => (id, Ok(())),
                    Err(err) => (id, Err(err))
                };
            }

            (id.clone(), Err(Error::Internal(InternalError::NoSuchChannel(id))))
        }).collect()
    }
}
