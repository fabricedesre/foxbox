/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! An adapter providing access to IP cameras. Currently only the following IP cameras are
//! supported: `DLink DCS-5010L`, `DLink DCS-5020L` and `DLink DCS-5025`.
//!

extern crate serde_json;

mod api;
mod upnp_listener;

use config_store::ConfigService;
use foxbox_taxonomy::api::{Error, InternalError, User};
use foxbox_taxonomy::manager::*;
use foxbox_taxonomy::selector::*;
use foxbox_taxonomy::services::*;
use foxbox_taxonomy::values::{ Value, Json, Binary, Type, TypeError};
use traits::Controller;
use transformable_channels::mpsc::*;
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
        udn: &str, url: &str, name: &str, manufacturer: &str, model_name: &str) -> Result<(), Error>
    {
        let service_id = create_service_id(udn);

        let adapter_id = Self::id();
        let mut service = Service::empty(&service_id, &adapter_id);

        service.properties.insert(CUSTOM_PROPERTY_MANUFACTURER.to_owned(),
                                           manufacturer.to_owned());
        service.properties.insert(CUSTOM_PROPERTY_MODEL.to_owned(), model_name.to_owned());
        service.properties.insert(CUSTOM_PROPERTY_NAME.to_owned(), name.to_owned());
        service.properties.insert(CUSTOM_PROPERTY_URL.to_owned(), url.to_owned());
        service.properties.insert(CUSTOM_PROPERTY_UDN.to_owned(), udn.to_owned());
        service.tags.insert(tag_id!(&format!("name:{}", name)));

        // Since the upnp_discover will be called about once very 3 minutes we want to ignore
        // discoveries if the camera is already registered.
        if let Err(error) = adapt.add_service(service) {
            if let Error::InternalError(ref internal_error) = error {
                if let InternalError::DuplicateService(_) = *internal_error {
                    debug!("Found {} @ {} UDN {} (ignoring since it already exists)",
                           model_name,
                           url,
                           udn);
                    return Ok(());
                }
            }

            panic!(error);
        }

        info!("Adding IpCamera {} Manufacturer: {} Model: {} Name: {}",
              udn,
              manufacturer,
              model_name,
              name);

        let getter_image_list_id = create_getter_id("image_list", udn);
        try!(adapt.add_channel(Channel {
            supports_fetch: true,
            kind: ChannelKind::Extension {
                vendor: Id::new("foxlink@mozilla.com"),
                adapter: Id::new("IPCam Adapter"),
                kind: Id::new("image_list"),
                typ: Type::Json,
            },
            ..Channel::empty(&getter_image_list_id, &service_id, &adapter_id)
        }));

        let getter_image_newest_id = create_getter_id("image_newest", udn);
        try!(adapt.add_channel(Channel {
            supports_fetch: true,
            kind: ChannelKind::Extension {
                vendor: Id::new("foxlink@mozilla.com"),
                adapter: Id::new("IPCam Adapter"),
                kind: Id::new("latest image"),
                typ: Type::Binary,
            },
            ..Channel::empty(&getter_image_newest_id, &service_id, &adapter_id)
        }));

        let setter_snapshot_id = create_setter_id("snapshot", udn);
        try!(adapt.add_channel(Channel {
            supports_send: true,
            kind: ChannelKind::TakeSnapshot,
            ..Channel::empty(&setter_snapshot_id, &service_id, &adapter_id)
        }));

        let getter_username_id = create_getter_id("username", udn);
        try!(adapt.add_channel(Channel {
            supports_fetch: true,
            kind: ChannelKind::Username,
            ..Channel::empty(&getter_username_id, &service_id, &adapter_id)
        }));

        let setter_username_id = create_setter_id("username", udn);
        try!(adapt.add_channel(Channel {
            supports_send: true,
            kind: ChannelKind::Username,
            ..Channel::empty(&setter_username_id, &service_id, &adapter_id)
        }));

        let getter_password_id = create_getter_id("password", udn);
        try!(adapt.add_channel(Channel {
            supports_fetch: true,
            kind: ChannelKind::Password,
            ..Channel::empty(&getter_password_id, &service_id, &adapter_id)
        }));

        let setter_password_id = create_setter_id("password", udn);
        try!(adapt.add_channel(Channel {
            supports_send: true,
            kind: ChannelKind::Password,
            ..Channel::empty(&setter_password_id, &service_id, &adapter_id)
        }));

        let mut serv = services.lock().unwrap();
        let camera_obj = try!(IpCamera::new(udn, url, name, &serv.snapshot_root, config));
        let camera = Arc::new(camera_obj);
        serv.getters.insert(getter_image_list_id, camera.clone());
        serv.getters.insert(getter_image_newest_id, camera.clone());
        serv.setters.insert(setter_snapshot_id, camera.clone());
        serv.getters.insert(getter_username_id, camera.clone());
        serv.setters.insert(setter_username_id, camera.clone());
        serv.getters.insert(getter_password_id, camera.clone());
        serv.setters.insert(setter_password_id, camera.clone());

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
                None => return (id.clone(), Err(Error::InternalError(InternalError::NoSuchChannel(id))))
            };

            if id == camera.get_username_id {
                let rsp = camera.get_username();
                return (id, Ok(Some(Value::String(Arc::new(rsp)))));
            }

            if id == camera.get_password_id {
                let rsp = camera.get_password();
                return (id, Ok(Some(Value::String(Arc::new(rsp)))));
            }

            if id == camera.image_list_id {
                let rsp = camera.get_image_list();
                return (id, Ok(Some(Value::Json(Arc::new(Json(serde_json::to_value(&rsp)))))));
            }

            if id == camera.image_newest_id {
                return match camera.get_newest_image() {
                    Ok(rsp) => (id, Ok(Some(Value::Binary(Binary {
                        data: Arc::new(rsp),
                        mimetype: Id::new("image/jpeg")
                    })))),
                    Err(err) => (id, Err(err))
                };
            }

            (id.clone(), Err(Error::InternalError(InternalError::NoSuchChannel(id))))
        }).collect()
    }

    fn send_values(&self, mut values: HashMap<Id<Channel>, Value>, _: User) -> ResultMap<Id<Channel>, (), Error> {
        values.drain().map(|(id, value)| {
            let camera = match self.services.lock().unwrap().setters.get(&id) {
                Some(camera) => camera.clone(),
                None => { return (id, Err(Error::InternalError(InternalError::InvalidInitialService))); }
            };

            if id == camera.set_username_id {
                if let Value::String(ref username) = value {
                    camera.set_username(username);
                    return (id, Ok(()));
                }
                return (id, Err(Error::TypeError(TypeError {
                                got:value.get_type(),
                                expected: Type::String
                            })))
            }

            if id == camera.set_password_id {
                if let Value::String(ref password) = value {
                    camera.set_password(password);
                    return (id, Ok(()));
                }
                return (id, Err(Error::TypeError(TypeError {
                                got:value.get_type(),
                                expected: Type::String
                            })))
            }

            if id == camera.snapshot_id {
                return match camera.take_snapshot() {
                    Ok(_) => (id, Ok(())),
                    Err(err) => (id, Err(err))
                };
            }

            (id.clone(), Err(Error::InternalError(InternalError::NoSuchChannel(id))))
        }).collect()
    }

    fn register_watch(&self, mut watch: Vec<WatchTarget>) -> WatchResult
    {
        watch.drain(..).map(|(id, _, _)| {
            (id.clone(), Err(Error::GetterDoesNotSupportWatching(id)))
        }).collect()
    }
}
