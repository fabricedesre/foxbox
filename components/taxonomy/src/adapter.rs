use api::{ Error, User };
use services::*;
use values::*;

use transformable_channels::mpsc::*;

use std::collections::HashMap;
use std::sync::Arc;

pub type ResultMap<K, T, E> = HashMap<K, Result<T, E>>;

/// A witness that we are currently watching for a value.
/// Watching stops when the guard is dropped.
pub trait AdapterWatchGuard : Send + Sync {
}

/// An API that adapter managers must implement
pub trait AdapterManagerHandle: Send {
    /// Add an adapter to the system.
    ///
    /// This version is optimized for Adapters that already implement Sync.
    ///
    /// # Errors
    ///
    /// Returns an error if an adapter with the same id is already present.
    fn add_adapter(& self, adapter: Arc<Adapter>) -> Result<(), Error>;

    /// Remove an adapter from the system, including all its services and channels.
    ///
    /// # Errors
    ///
    /// Returns an error if no adapter with this identifier exists. Otherwise, attempts
    /// to cleanup as much as possible, even if for some reason the system is in an
    /// inconsistent state.
    fn remove_adapter(& self, id: &Id<AdapterId>) -> Result<(), Error>;

    /// Add a service to the system. Called by the adapter when a new
    /// service (typically a new device) has been detected/configured.
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
    /// - there is no adapter with id `service.lock`.
    fn add_service(& self, service: Service) -> Result<(), Error>;

    /// Remove a service previously registered on the system. Typically, called by
    /// an adapter when a service (e.g. a device) is disconnected.
    ///
    /// # Errors
    ///
    /// Returns an error if any of:
    /// - there is no such service;
    /// - there is an internal inconsistency, in which case this method will still attempt to
    /// cleanup before returning an error.
    fn remove_service(& self, service_id: &Id<ServiceId>) -> Result<(), Error>;

    /// Add a channel to the system. Typically, this is called by the adapter when a new
    /// service has been detected/configured. Some services may gain/lose channels at
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
    fn add_channel(& self, setter: Channel) -> Result<(), Error>;

    /// Remove a setter previously registered on the system. Typically, called by
    /// an adapter when a service is reconfigured to remove one of its getters.
    ///
    /// # Error
    ///
    /// This method returns an error if the setter is not registered or if the service
    /// is not registered. In either case, it attemps to clean as much as possible, even
    /// if the state is inconsistent.
    fn remove_channel(& self, id: &Id<Channel>) -> Result<(), Error>;
}

pub enum WatchEvent {
    /// Fired when we enter the range specified when we started watching, or if no range was
    /// specified, fired whenever a new value is available.
    Enter {
        id: Id<Channel>,
        value: Value
    },

    /// Fired when we exit the range specified when we started watching. If no range was
    /// specified, never fired.
    Exit {
        id: Id<Channel>,
        value: Value
    }
}

/// API that adapters must implement.
///
/// # Requirements
///
/// Channels and Services are expected to have a stable id, which persists between reboots
/// and { dis, re }connections.
///
/// Note that all methods are blocking. However, the underlying implementatino of adapters is
/// expected to either return quickly or be able to handle several requests concurrently.
pub trait Adapter: Send + Sync {
    /// An id unique to this adapter. This id must persist between
    /// reboots/reconnections.
    fn id(&self) -> Id<AdapterId>;

    /// The name of the adapter.
    fn name(&self) -> &str;
    fn vendor(&self) -> &str;
    fn version(&self) -> &[u32;4];
    // ... more metadata

    /// Request values from a group of channels.
    ///
    /// The AdapterManager always attempts to group calls to `fetch_values` by `Adapter`, and then
    /// expects the adapter to attempt to minimize the connections with the actual devices.
    ///
    /// The AdapterManager is in charge of keeping track of the age of values.
    fn fetch_values(&self, mut target: Vec<Id<Channel>>, _: User) -> ResultMap<Id<Channel>, Option<Value>, Error>;

    /// Request that values be sent to channels.
    ///
    /// The AdapterManager always attempts to group calls to `send_values` by `Adapter`, and then
    /// expects the adapter to attempt to minimize the connections with the actual devices.
    fn send_values(&self, values: HashMap<Id<Channel>, Value>, user: User) -> ResultMap<Id<Channel>, (), Error>;

    /// Watch a bunch of getters as they change.
    ///
    /// The `AdapterManager` always attempts to group calls to `fetch_values` by `Adapter`, and
    /// then expects the adapter to attempt to minimize the connections with the actual devices.
    /// The Adapter should however be ready to handle concurrent `register_watch` on the same
    /// devices, possibly with distinct `Option<Range>` options.
    ///
    /// If a `Range` option is set, the watcher expects to receive `EnterRange`/`ExitRange` events
    /// whenever the value available on the device enters/exits the range.
    fn register_watch(&self, Vec<WatchTarget>) ->
            WatchResult;

    /// Signal the adapter that it is time to stop.
    ///
    /// Ideally, the adapter should not return until all its threads have been stopped.
    fn stop(&self) {
        // By default, do nothing.
    }
}

pub type WatchTarget = (Id<Channel>, Option<Value>, Box<ExtSender<WatchEvent>>);
pub type WatchResult = Vec<(Id<Channel>, Result<Box<AdapterWatchGuard>, Error>)>;
