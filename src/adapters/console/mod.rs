//! A simple adapter designe solely to print messages on the console.
//!
//! Useful for logging.

use foxbox_taxonomy::api::{ Error, InternalError, User };
use foxbox_taxonomy::manager::*;
use foxbox_taxonomy::services::*;
use foxbox_taxonomy::values::{ Value };

use transformable_channels::mpsc::*;

use std::collections::HashMap;
use std::sync::Arc;


static ADAPTER_NAME: &'static str = "Console adapter (built-in)";
static ADAPTER_VENDOR: &'static str = "team@link.mozilla.org";
static ADAPTER_VERSION: [u32;4] = [0, 0, 0, 0];

pub struct Console {
    setter_stdout_id: Id<Channel>
}

impl Console {
    pub fn id() -> Id<AdapterId> {
        Id::new("console@link.mozilla.org")
    }
    pub fn service_console_id() -> Id<ServiceId> {
        Id::new("service:console@link.mozilla.org")
    }
    pub fn setter_stdout_id() -> Id<Channel> {
        Id::new("setter:stdout@link.mozilla.org")
    }
}
impl Adapter for Console {
    fn id(&self) -> Id<AdapterId> {
        Self::id()
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

    fn send_values(&self, mut values: HashMap<Id<Channel>, Value>, user: User) -> ResultMap<Id<Channel>, (), Error> {
        values.drain()
            .map(|(id, value)| {
                let result = {
                    if id == self.setter_stdout_id {
                        if let Value::String(s) = value {
                            info!("[console@link.mozilla.org] {} (user {:?})", s, user);
                        } else {
                            info!("[console@link.mozilla.org] {:?} (user {:?})", value, user);
                        }
                        Ok(())
                    } else {
                        Err(Error::InternalError(InternalError::NoSuchChannel(id.clone())))
                    }
                };
                (id, result)
            })
            .collect()
    }

    fn register_watch(&self, mut watch: Vec<WatchTarget>) -> WatchResult
    {
        watch.drain(..).map(|(id, _, _)| {
            (id.clone(), Err(Error::GetterDoesNotSupportWatching(id)))
        }).collect()
    }
}


impl Console {
    pub fn init(adapt: &Arc<AdapterManager>) -> Result<(), Error> {
        let service_console_id = Console::service_console_id();
        let setter_stdout_id = Console::setter_stdout_id();
        let adapter_id = Console::id();
        let console = Arc::new(Console {
            setter_stdout_id: setter_stdout_id.clone()
        });
        try!(adapt.add_adapter(console));
        let mut service = Service::empty(&service_console_id, &adapter_id);
        service.properties.insert("model".to_owned(), "Mozilla console v1".to_owned());
        try!(adapt.add_service(service));
        try!(adapt.add_channel(Channel {
            kind: ChannelKind::Log,
            supports_send: true,
            ..Channel::empty(&setter_stdout_id, &service_console_id, &adapter_id)
        }));
        Ok(())
    }
}
