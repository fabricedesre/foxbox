/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

///
/// Example cUrl request:
/// curl -X PUT -d '[[[{"id":"setter:talk@link.mozilla.org"}], {"String": "hello world"}]]' http://localhost:3000/api/v1/channels/set
///

use foxbox_taxonomy::adapter::*;
use foxbox_taxonomy::manager::AdapterManager;
use foxbox_taxonomy::api::{ Error, InternalError, User };
use foxbox_taxonomy::services::{ AdapterId, Channel, ChannelKind, Id, Service, ServiceId };
use foxbox_taxonomy::values::{ Type, Value };
use std::collections::{ HashMap, HashSet };
use std::sync::Arc;
use transformable_channels::mpsc::*;

pub mod engine;
pub use self::engine::TtsEngine;

// eSpeak is the only engine supported for now.
mod espeak;
use self::espeak::EspeakEngine;

static ADAPTER_ID: &'static str = "espeak_adapter@link.mozilla.org";
static ADAPTER_NAME: &'static str = "eSpeak adapter";
static ADAPTER_VENDOR: &'static str = "team@link.mozilla.org";
static ADAPTER_VERSION: [u32;4] = [0, 0, 0, 0];

pub struct TtsAdapter<T> {
    talk_setter_id: Id<Channel>,
    engine: T
}

impl<T: TtsEngine> Adapter for TtsAdapter<T> {
    fn id(&self) -> Id<AdapterId> {
        adapter_id!(ADAPTER_ID)
    }

    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    fn vendor(&self) -> &str {
        ADAPTER_VENDOR
    }

    fn version(&self) -> &[u32;4] {
        &ADAPTER_VERSION
    }

    fn fetch_values(&self, mut set: Vec<Id<Channel>>, _: User) -> ResultMap<Id<Channel>, Option<Value>, Error> {
        set.drain(..).map(|id| {
            (id.clone(), Err(Error::InternalError(InternalError::NoSuchChannel(id))))
        }).collect()
    }

    fn send_values(&self, mut values: HashMap<Id<Channel>, Value>, _: User) -> ResultMap<Id<Channel>, (), Error> {
        use core::ops::Deref;

        values.drain().map(|(id, value)| {
            if id == self.talk_setter_id {
                if let Value::String(text) = value {
                    self.engine.say(text.deref());
                    return (id, Ok(()));
                }
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

pub fn init(adapt: &Arc<AdapterManager>) -> Result<(), Error> {
    let engine = EspeakEngine { };
    if !engine.init() {
        warn!("eSpeak initialization failed!");
        return Err(Error::InternalError(InternalError::GenericError("eSpeak initialization failed!".to_owned())));
    }

    let talk_setter_id = Id::new("setter:talk@link.mozilla.org");
    try!(adapt.add_adapter(Arc::new(TtsAdapter {
        talk_setter_id: talk_setter_id.clone(),
        engine: engine
    })));
    let service_id = service_id!("espeak@link.mozilla.org");
    let adapter_id = adapter_id!(ADAPTER_ID);
    try!(adapt.add_service(Service::empty(&service_id, &adapter_id)));
    try!(adapt.add_channel(Channel {
        kind: ChannelKind::Extension {
            vendor: Id::new(ADAPTER_VENDOR),
            adapter: Id::new(ADAPTER_NAME),
            kind: Id::new("Sentence"),
            typ: Type::String,
        },
        supports_send: true,
        .. Channel::empty(&talk_setter_id, &service_id, &adapter_id)
    }));
    Ok(())
}
