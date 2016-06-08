//! Utilities for writing adapters.

use api::{ Error, InternalError, User };
use channel::Channel;
use io::*;
use manager::*;
use util::{ Id, AdapterId };
use values::*;

use std::collections::HashMap;
use std::sync::{ Arc, Mutex };

use transformable_channels::mpsc::*;

/// A simple way of converting an Adapter to an Adapter + Sync.
///
/// Hardly optimal, but useful for testing and prototyping.
pub struct MakeSyncAdapter<T> where T: Adapter {
    lock: Mutex<Arc<T>>,
    id: Id<AdapterId>,
    name: String,
    vendor: String,
    version: [u32; 4],
}

impl<T> MakeSyncAdapter<T> where T: Adapter {
    pub fn new(adapter: T) -> Self {
        MakeSyncAdapter {
            id: adapter.id().clone(),
            name: adapter.name().to_owned(),
            vendor: adapter.vendor().to_owned(),
            version: adapter.version().to_owned(),
            lock: Mutex::new(Arc::new(adapter)),
        }
    }
}
impl<T> Adapter for MakeSyncAdapter<T> where T: Adapter {
    fn id(&self) -> Id<AdapterId> {
        self.id.clone()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn vendor(&self) -> &str {
        &self.vendor
    }

    fn version(&self) -> &[u32;4] {
        &self.version
    }

    fn fetch_values(&self, set: Vec<Id<Channel>>, user: User) -> ResultMap<Id<Channel>, Option<Value>, Error> {
        self.lock.lock().unwrap().fetch_values(set, user)
    }

    fn send_values(&self, values: HashMap<Id<Channel>, Value>, user: User) -> ResultMap<Id<Channel>, (), Error> {
        self.lock.lock().unwrap().send_values(values, user)
    }

    fn register_watch(&self, watch: Vec<WatchTarget>) -> WatchResult {
        self.lock.lock().unwrap().register_watch(watch)
    }
}


pub struct RawAdapterForAdapter {
    adapter: Arc<Adapter>
}
impl RawAdapterForAdapter {
    pub fn new(adapter: Arc<Adapter>) -> Self {
        RawAdapterForAdapter {
            adapter: adapter
        }
    }
}

impl RawAdapter for RawAdapterForAdapter {
    fn id(&self) -> Id<AdapterId> {
        self.adapter.id()
    }
    fn stop(&self) {
        self.adapter.stop()
    }
    fn fetch_values(&self, mut target: Vec<(Id<Channel>, Arc<Format>)>, user: User) -> OpResult<(Payload, Arc<Format>)> {
        let types : HashMap<_, _> = target.iter().cloned().collect();
        let channels : Vec<_> = target.drain(..).map(|(id, _)| id).collect();
        let values = self.adapter.fetch_values(channels, user);
        values.iter().map(|(id, result)| {
            let result = match *result {
                Err(ref err) => Err(err.clone()),
                Ok(None) => Ok(None),
                Ok(Some(ref value)) => {
                    match types.get(id) {
                        None => Err(Error::InternalError(InternalError::WrongChannel(id.clone()))),
                        Some(type_) => Payload::from_value(value, type_)
                            .map(|payload| Some((payload, type_.clone())))
                    }
                },
            };
            (id.clone(), result)
        }).collect()
    }

    fn send_values(&self, mut values: HashMap<Id<Channel>, (Payload, Arc<Format>)>, user: User) -> ResultMap<Id<Channel>, (), Error> {
        let mut send = HashMap::new();
        let mut failures = HashMap::new();
        for (id, (payload, type_)) in values.drain() {
            match payload.to_value(&type_) {
                Err(err) => {
                    failures.insert(id, Err(err));
                },
                Ok(value) => {
                    send.insert(id, value);
                },
            }
        }
        let mut results = self.adapter.send_values(send, user);
        results.extend(failures);
        results
    }

    fn register_watch(&self, mut targets: Vec<RawWatchTarget>) -> WatchResult {
        let mut send : Vec<(_, _, Box<ExtSender<WatchEvent<Value>>>)> = Vec::new();
        let mut failures = Vec::new();
        for (id, filter, event_type, sender) in targets.drain(..) { // FIXME: Do I really need event_type? Why? Shouldn't it be part of ChannelKind?
            let sender = Box::new(sender.map(move |event| {
                match event {
                    WatchEvent::Enter { id, value } => {
                        match Payload::from_value(&value, &event_type) {
                            Ok(payload) =>
                                WatchEvent::Enter { id: id, value: (payload, event_type.clone()) },
                            Err(err) =>
                                WatchEvent::Error { id: id, error: err }
                        }
                    }
                    WatchEvent::Exit { id, value } => {
                        match Payload::from_value(&value, &event_type) {
                            Ok(payload) =>
                                WatchEvent::Exit { id: id, value: (payload, event_type.clone()) },
                            Err(err) =>
                                WatchEvent::Error { id: id, error: err }
                        }
                    }
                    WatchEvent::Error { id, error } =>
                        WatchEvent::Error { id: id, error: error }
                }
            }));
            if let Some((payload, type_)) = filter {
                match payload.to_value(&type_) {
                    Err(err) => { failures.push((id, Err(err))); },
                    Ok(value) => { send.push((id, Some(value), sender)); },
                }
            } else {
                send.push((id, None, sender));
            }
        }
        let mut result = self.adapter.register_watch(send);
        result.extend(failures);
        result
    }
}
