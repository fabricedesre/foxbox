//! This module defines the metadata on devices and services.
//!
//! Note that all the data structures in this module represent
//! snapshots of subsets of the devices available. None of these data
//! structures are live, so there is always the possibility that
//! devices may have been added or removed from the `FoxBox` by the time
//! these data structures are read.

use channel::*;
use parse::*;
pub use util::{ Exactly, Maybe, Id, AdapterId, ServiceId, KindId, TagId, VendorId };

use std::collections::{ HashSet, HashMap };

// A helper macro to create a Id<ServiceId> without boilerplate.
#[macro_export]
macro_rules! service_id {
    ($val:expr) => (Id::<ServiceId>::new($val))
}

// A helper macro to create a Id<AdapterId> without boilerplate.
#[macro_export]
macro_rules! adapter_id {
    ($val:expr) => (Id::<AdapterId>::new($val))
}

// A helper macro to create a Id<TagId> without boilerplate.
#[macro_export]
macro_rules! tag_id {
    ($val:expr) => (Id::<TagId>::new($val))
}

/// Metadata on a service. A service is a device or collection of devices
/// that may offer services. The `FoxBox` itself is a service offering
/// services such as a clock, communicating with the user through her
/// smart devices, etc.
///
/// # JSON
///
/// A service is represented by an object with the following fields:
///
/// - id: string - an id unique to this service;
/// - adapter: string;
/// - tags: array of strings;
/// - properties: object;
/// - getters: object (keys are string identifiers, for more details on values see Channel<Getter>);
/// - setters: object (keys are string identifiers, for more details on values see Channel<Setter>);
///
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Service {
    /// Tags describing the service.
    ///
    /// These tags can be set by the user, adapters or
    /// applications. They are used by applications to find services and
    /// services.
    ///
    /// For instance, a user may set tag "entrance" to all services
    /// placed in the entrance of his house, or a tag "blue" to a service
    /// controlling blue lights. An adapter may set tags "plugged" or
    /// "battery" to devices that respectively depend on a plugged
    /// power source or on a battery.
    pub tags: HashSet<Id<TagId>>,

    /// An id unique to this service.
    pub id: Id<ServiceId>,

    /// Service properties that are set at creation time.
    /// For instance, these can be device manufacturer, model, etc.
    pub properties: HashMap<String, String>,

    /// Channels connected directly to this service.
    pub channels: HashMap<Id<Channel>, Channel>,

    /// Identifier of the adapter for this service.
    pub adapter: Id<AdapterId>,
}

impl Service {
    /// Create an empty service.
    pub fn empty(id: &Id<ServiceId>, adapter: &Id<AdapterId>) -> Self {
        Service {
            tags: HashSet::new(),
            channels: HashMap::new(),
            properties: HashMap::new(),
            id: id.clone(),
            adapter: adapter.clone(),
        }
    }
}

impl ToJSON for Service {
    fn to_json(&self) -> JSON {
        vec![
            ("id", self.id.to_json()),
            ("adapter", self.adapter.to_json()),
            ("tags", self.tags.to_json()),
            ("properties", self.properties.to_json()),
            ("channels", self.channels.to_json()),
        ].to_json()
    }
}
