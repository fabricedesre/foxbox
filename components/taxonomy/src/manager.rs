//! The Adapter manager
//!
//! This structure serves two roles:
//! - adapters use it to (un)register themselves, as well as services and channels;
//! - it exposes an implementation of the taxonomy API.

pub use adapter::*;
use api;
use api::{ API, Error, TargetMap, User };
use backend::*;
use channel::Channel;
use io::*;
use selector::*;
use services::*;
use util::is_sync;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{ Arc, Mutex, Weak };
use std::sync::atomic::{ AtomicBool, Ordering };
use std::thread;

use sublock::atomlock::*;
use transformable_channels::mpsc::*;

/// An implementation of the `AdapterManager`.
///
/// This implementation is `Sync` and supports any number of concurrent
/// readers *or* a single writer.
pub struct AdapterManager {
    /// The in-memory database is protected by a read-write lock.
    ///
    /// Each method is responsible for determining whether it needs read() or write()
    /// and releasing the lock as soon as possible.
    ///
    /// The Arc is necessary to let WatchGuard release watches upon Drop.
    back_end: Arc<MainLock<State>>,

    tx_watch: Arc<Mutex<RawSender<WatchOp>>>,
}

impl AdapterManager {
    /// Create an empty `AdapterManager`.
    /// This function does not attempt to load any state from the disk.
    pub fn new(db_path: Option<PathBuf>) -> Self {
        // The code should build only if AdapterManager implements Sync.
        is_sync::<AdapterManager>();

        let state = Arc::new(MainLock::new(|liveness| State::new(liveness, db_path)));
        let tx_watch = Arc::new(Mutex::new(Self::handle_watches(Arc::downgrade(&state))));
        AdapterManager {
            back_end: state,
            tx_watch: tx_watch,
        }
    }
}

impl Default for AdapterManager {
    fn default() -> Self {
        Self::new(None)
    }
}

impl AdapterManagerHandle for AdapterManager {
    /// Add an adapter to the system.
    ///
    /// # Errors
    ///
    /// Returns an error if an adapter with the same id is already present.
    fn add_adapter(&self, adapter: Arc<Adapter>) -> Result<(), Error> {
        self.back_end.write().unwrap().add_adapter(adapter)
    }

    /// Remove an adapter from the system, including all its services and channels.
    ///
    /// # Errors
    ///
    /// Returns an error if no adapter with this identifier exists. Otherwise, attempts
    /// to cleanup as much as possible, even if for some reason the system is in an
    /// inconsistent state.
    fn remove_adapter(&self, id: &Id<AdapterId>) -> Result<(), Error> {
        self.back_end.write().unwrap().remove_adapter(id)
    }

    /// Add a service to the system. Called by the adapter when a new
    /// service (typically a new device) has been detected/configured.
    ///
    /// The `service` must NOT have any channels yet. Channels must be added through
    /// `add_channel`.
    ///
    /// # Requirements
    ///
    /// The adapter is in charge of making sure that identifiers persist across reboots.
    ///
    /// # Errors
    ///
    /// Returns an error if any of:
    /// - `service` has channels;
    /// - a service with id `service.id` is already installed on the system;
    /// - there is no adapter with id `service.adapter`.
    fn add_service(&self, service: Service) -> Result<(), Error> {
        self.back_end.write().unwrap().add_service(service)
    }

    /// Remove a service previously registered on the system. Typically, called by
    /// an adapter when a service (e.g. a device) is disconnected.
    ///
    /// # Error
    ///
    /// Returns an error if any of:
    /// - there is no such service;
    /// - there is an internal inconsistency, in which case this method will still attempt to
    /// cleanup before returning an error.
    fn remove_service(&self, id: &Id<ServiceId>) -> Result<(), Error> {
        self.back_end.write().unwrap().remove_service(id)
    }

    /// Add a setter to the system. Typically, this is called by the adapter when a new
    /// service has been detected/configured. Some services may gain/lose getters at
    /// runtime depending on their configuration.
    ///
    /// # Requirements
    ///
    /// The adapter is in charge of making sure that identifiers persist across reboots.
    ///
    /// # Errors
    ///
    /// Returns an error if the adapter is not registered, the parent service is not
    /// registered, or a channel with the same identifier is already registered.
    /// In either cases, this method reverts all its changes.
    fn add_channel(&self, getter: Channel) -> Result<(), Error> {
        let request = {
            // Acquire and release lock asap.
            try!(self.back_end.write().unwrap().add_channel(getter))
        };
        if !request.is_empty() {
            debug!(target: "Taxonomy-manager", "manager.add_channel => need to register watches");
        }
        self.register_watches(request);
        Ok(())
    }

    /// Remove a setter previously registered on the system. Typically, called by
    /// an adapter when a service is reconfigured to remove one of its getters.
    ///
    /// # Error
    ///
    /// This method returns an error if the setter is not registered or if the service
    /// is not registered. In either case, it attemps to clean as much as possible, even
    /// if the state is inconsistent.
    fn remove_channel(&self, id: &Id<Channel>) -> Result<(), Error> {
        self.back_end.write().unwrap().remove_channel(id)
    }
}

/// A handle to the public API.
impl API for AdapterManager {
    /// Get the metadata on services matching some conditions.
    ///
    /// A call to `API::get_services(vec![req1, req2, ...])` will return
    /// the metadata on all services matching _either_ `req1` or `req2`
    /// or ...
    fn get_services(&self, selectors: Vec<ServiceSelector>) -> Vec<Service> {
        self.back_end.read().unwrap().get_services(selectors)
    }

    /// Label a set of services with a set of tags.
    ///
    /// A call to `API::put_service_tag(vec![req1, req2, ...], vec![tag1,
    /// ...])` will label all the services matching _either_ `req1` or
    /// `req2` or ... with `tag1`, ... and return the number of services
    /// matching any of the selectors.
    ///
    /// Some of the services may already be labelled with `tag1`, or
    /// `tag2`, ... They will not change state. They are counted in
    /// the resulting `usize` nevertheless.
    ///
    /// Note that this call is _not live_. In other words, if services
    /// are added after the call, they will not be affected.
    fn add_service_tags(&self, selectors: Vec<ServiceSelector>, tags: Vec<Id<TagId>>) -> usize {
        self.back_end.write().unwrap().add_service_tags(selectors, tags)
        // FIXME: This can cause watcher registrations
    }

    /// Remove a set of tags from a set of services.
    ///
    /// A call to `API::delete_service_tag(vec![req1, req2, ...], vec![tag1,
    /// ...])` will remove from all the services matching _either_ `req1` or
    /// `req2` or ... all of the tags `tag1`, ... and return the number of services
    /// matching any of the selectors.
    ///
    /// Some of the services may not be labelled with `tag1`, or `tag2`,
    /// ... They will not change state. They are counted in the
    /// resulting `usize` nevertheless.
    ///
    /// Note that this call is _not live_. In okther words, if services
    /// are added after the call, they will not be affected.
    fn remove_service_tags(&self, selectors: Vec<ServiceSelector>, tags: Vec<Id<TagId>>) -> usize {
        self.back_end.write().unwrap().remove_service_tags(selectors, tags)
    }

    /// Get a list of channels matching some conditions
    fn get_channels(&self, selectors: Vec<ChannelSelector>) -> Vec<Channel> {
        self.back_end.read().unwrap().get_channels(selectors)
    }

    /// Label a set of channels with a set of tags.
    ///
    /// A call to `API::put_{setter, setter}_tag(vec![req1, req2, ...], vec![tag1,
    /// ...])` will label all the channels matching _either_ `req1` or
    /// `req2` or ... with `tag1`, ... and return the number of channels
    /// matching any of the selectors.
    ///
    /// Some of the channels may already be labelled with `tag1`, or
    /// `tag2`, ... They will not change state. They are counted in
    /// the resulting `usize` nevertheless.
    ///
    /// Note that this call is _not live_. In other words, if channels
    /// are added after the call, they will not be affected.
    fn add_channel_tags(&self, selectors: Vec<ChannelSelector>, tags: Vec<Id<TagId>>) -> usize {
        let (request, result) = {
            // Acquire and release the write lock.
            self.back_end.write().unwrap().add_channel_tags(selectors, tags)
        };
        if !request.is_empty() {
            debug!(target: "Taxonomy-manager", "manager.add_getter_tags => need to register watches");
        }
        self.register_watches(request);
        result
    }

    /// Remove a set of tags from a set of channels.
    ///
    /// A call to `API::delete_{setter, setter}_tag(vec![req1, req2, ...], vec![tag1,
    /// ...])` will remove from all the channels matching _either_ `req1` or
    /// `req2` or ... all of the tags `tag1`, ... and return the number of channels
    /// matching any of the selectors.
    ///
    /// Some of the channels may not be labelled with `tag1`, or `tag2`,
    /// ... They will not change state. They are counted in the
    /// resulting `usize` nevertheless.
    ///
    /// Note that this call is _not live_. In other words, if channels
    /// are added after the call, they will not be affected.
    fn remove_channel_tags(&self, selectors: Vec<ChannelSelector>, tags: Vec<Id<TagId>>) -> usize {
        self.back_end.write().unwrap().remove_channel_tags(selectors, tags)
    }

    /// Read the latest value from a set of channels
    fn fetch_values(&self, selectors: Vec<ChannelSelector>, user: User) -> OpResult<(Payload, Arc<Format>)>
    {
        // First, prepare the request.
        let mut request;
        {
            // Make sure that the lock is released asap.
            request = self.back_end.read().unwrap().prepare_fetch_values(selectors);
        }
        // Now fetch the values
        let mut results = HashMap::new();
        for (_, (adapter, mut channels)) in request.drain() {
            let channels = channels.drain().collect();
            let got = adapter.fetch_values(channels, user.clone());

            results.extend(got);
        }
        results
    }

    /// Send a bunch of values to a set of channels
    fn send_values(&self, keyvalues: TargetMap<ChannelSelector, Payload>, user: User) ->
        ResultMap<Id<Channel>, (), Error>
    {
        // First, prepare the request.
        let mut prepared;
        {
            // Make sure that the lock is released asap.
            prepared = self.back_end.read().unwrap().prepare_send_values(keyvalues);
        }

        // Dispatch to adapter
        let mut results = HashMap::new();
        for (_, (adapter, request)) in prepared.drain() {
            let got = adapter.send_values(request, user.clone());
            results.extend(got);
        }

        results
    }

    /// Watch for any change
    fn watch_values(&self, watch: TargetMap<ChannelSelector, Exactly<(Payload, Arc<Format>)>>,
        on_event: Box<ExtSender<api::WatchEvent>>) -> Self::WatchGuard
    {
        let (request, watch_key, is_dropped) =
        {
            // Acquire and release write lock.
            self.back_end.write()
                .unwrap()
                .prepare_channel_watch(watch, on_event)
        };

        if !request.is_empty() {
            debug!(target: "Taxonomy-manager", "manager.watch_values => need to register watches");
        }
        self.register_watches(request);
        WatchGuard::new(self.tx_watch.lock().unwrap().internal_clone(), watch_key, is_dropped)
    }

    /// A value that causes a disconnection once it is dropped.
    type WatchGuard = WatchGuard;
}

/// Operations related to watching.
///
/// As the adapter side of operations can be slow, we want to keep them out of the `MainLock`. On the
/// other hand, we want to make sure that they take place in a predictable order, to avoid race
/// conditions. So we delegate them to a specialized background thread.
enum WatchOp {
    /// Start watching a bunch of channels, then register them as being watched.
    Start(WatchRequest, RawSender<()>),

    /// Release a watch, after the corresponding WatchGuard has been dropped.
    Release(WatchKey)
}

impl AdapterManager {
    /// Register watches on the dedicated background thread. This must be done outside of any
    /// lock!
    fn register_watches(&self, request: WatchRequest) {
        if !request.is_empty() {
            let (tx, rx) = channel();
            let _ = self.tx_watch.lock().unwrap().send(WatchOp::Start(request, tx));
            let _ = rx.recv();
        }
    }

    /// Start the background thread .
    fn handle_watches(state: Weak<MainLock<State>>) -> RawSender<WatchOp> {
        let (tx, rx) = channel();
        let state = state.clone();
        thread::spawn(move || {
            for msg in rx {
                match state.upgrade() {
                    None => return, // The manager has been dropped.
                    Some(backend) =>
                        match msg {
                            WatchOp::Start(request, tx) => {
                                let add = State::start_watch(request);
                                backend.write().unwrap().register_ongoing_watch(add);
                                let _ = tx.send(());
                            }
                            WatchOp::Release(request) => {
                                backend.write().unwrap().stop_watch(request)
                            }
                        }
                }
            }
        });
        tx
    }
}


/// A data structure that causes cancellation of a watch when dropped.
pub struct WatchGuard {
    tx_owner: Box<ExtSender<WatchOp>>,

    /// The cancellation key.
    key: WatchKey,

    /// Once dropped, the watch callbacks will stopped being called. Note
    /// that dropping this value is not sufficient to cancel the watch, as
    /// the adapters will continue sending updates.
    is_dropped: Arc<AtomicBool>,
}
impl WatchGuard {
    fn new(tx_owner: Box<ExtSender<WatchOp>>, key: WatchKey, is_dropped: Arc<AtomicBool>) -> Self
    {
        WatchGuard {
            tx_owner: tx_owner,
            key: key,
            is_dropped: is_dropped
        }
    }
}
impl Drop for WatchGuard {
    fn drop(&mut self) {
        self.is_dropped.store(true, Ordering::Relaxed);

        // Attempt to release the watch. In the unlikely case that we can't (perhaps if we're
        // dropping during shutdown), don't insist.
        // Note that we background this to avoid any risk of deadlock during the drop.
        let _ = self.tx_owner.send(WatchOp::Release(self.key));
    }
}


impl AdapterManager {
    pub fn stop(&self) {
        self.back_end.write().unwrap().stop()
    }
}
