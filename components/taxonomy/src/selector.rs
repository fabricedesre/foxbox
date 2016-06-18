//! Selectors for services and channels.
//!
//! The high-level API of Project Link always offers access by selectors, rather than by individual
//! services/channels. This allows operations such as sending a temperature to all heaters in the
//! living room (that's a selector), rather than needing to access every single heater one by one.

pub use parse::*;
use channel::*;
use services::Service;
use util::*;

use std::hash::Hash;
use std::collections::HashSet;

fn merge<T>(mut a: HashSet<T>, b: Vec<T>) -> HashSet<T> where T: Hash + Eq {
    for x in b {
        a.insert(x);
    }
    a
}

pub trait SelectedBy<T> {
    fn matches(&self, &T) -> bool;
}

impl SelectedBy<Option<Signature>> for Exactly<bool> {
    fn matches(&self, sig: &Option<Signature>) -> bool {
        match (self, sig) {
            (&Exactly::Always, _) |
            (&Exactly::Exactly(true), &Some(_)) |
            (&Exactly::Exactly(false), &None) => true,
            _ => false
        }
    }
}

/// A trait used to let `ServiceSelector` work on complex data structures
/// that are not necessarily exactly Selector.
pub trait ServiceLike {
    fn id(&self) -> &Id<ServiceId>;
    fn adapter(&self) -> &Id<AdapterId>;
    fn with_tags<F>(&self, f: F) -> bool where F: Fn(&HashSet<Id<TagId>>) -> bool;
    fn has_channels<F>(&self, f: F) -> bool where F: Fn(&Channel) -> bool;
}

impl ServiceLike for Service {
    fn id(&self) -> &Id<ServiceId> {
        &self.id
    }
    fn adapter(&self) -> &Id<AdapterId> {
        &self.adapter
    }
    fn with_tags<F>(&self, f: F) -> bool where F: Fn(&HashSet<Id<TagId>>) -> bool {
        f(&self.tags)
    }
    fn has_channels<F>(&self, f: F) -> bool where F: Fn(&Channel) -> bool {
        for chan in self.channels.values() {
            if f(chan) {
                return true;
            }
        }
        false
    }
}

/// A selector for one or more services.
///
///
/// # Example
///
/// ```
/// use foxbox_taxonomy::selector::*;
/// use foxbox_taxonomy::services::*;
///
/// let selector = ServiceSelector::new()
///   .with_tags(vec![Id::<TagId>::new("entrance")])
///   .with_channels(vec![ChannelSelector::new() /* can be more restrictive */]);
/// ```
///
/// # JSON
///
/// A selector is an object with the following fields:
///
/// - (optional) string `id`: accept only a service with a given id;
/// - (optional) array of string `tags`:  accept only services with all the tags in the array;
/// - (optional) array of objects `channels` (see `ChannelSelector`): accept only services with
///    channels matching all the selectors in this array;
///
/// While each field is optional, at least one field must be provided.
///
/// ```
/// use foxbox_taxonomy::selector::*;
///
/// // A selector with all fields defined.
/// let json_selector = "{
///   \"id\": \"setter 1\",
///   \"tags\": [\"tag 1\", \"tag 2\"],
///   \"channels\": [{
///     \"feature\": \"chronometer/is-ready\"
///   }]
/// }";
///
/// ServiceSelector::from_str(json_selector).unwrap();
///
/// // The following will be rejected because no field is provided:
/// let json_empty = "{}";
/// match ServiceSelector::from_str(json_empty) {
///   Err(ParseError::EmptyObject {..}) => { /* as expected */ },
///   other => panic!("Unexpected result {:?}", other)
/// }
/// ```
#[derive(Clone, Debug, Deserialize, Default)]
pub struct ServiceSelector {
    /// If `Exactly(id)`, return only the service with the corresponding id.
    pub id: Exactly<Id<ServiceId>>,

    ///  Restrict results to services that have all the tags in `tags`.
    pub tags: HashSet<Id<TagId>>,

    /// Restrict results to services that have all the channels in `channels`.
    pub channels: Vec<ChannelSelector>,

    /// Make sure that we can't instantiate from another crate.
    private: (),
}

impl Parser<ServiceSelector> for ServiceSelector {
    fn description() -> String {
        "ServiceSelector".to_owned()
    }
    fn parse(path: Path, source: &JSON) -> Result<Self, ParseError> {
        let mut is_empty = true;
        let id = try!(match path.push("id", |path| Exactly::take_opt(path, source, "id")) {
            None => Ok(Exactly::Always),
            Some(result) => {
                is_empty = false;
                result
            }
        });
        let tags : HashSet<_> = match path.push("tags", |path| Id::take_vec_opt(path, source, "tags")) {
            None => HashSet::new(),
            Some(Ok(mut vec)) => {
                is_empty = false;
                vec.drain(..).collect()
            }
            Some(Err(err)) => return Err(err),
        };
        let channels = match path.push("channels", |path| ChannelSelector::take_vec_opt(path, source, "channels")) {
            None => vec![],
            Some(Ok(vec)) => {
                is_empty = false;
                vec
            }
            Some(Err(err)) => return Err(err)
        };

        if is_empty {
            Err(ParseError::empty_object(&path))
        } else {
            Ok(ServiceSelector {
                id: id,
                tags: tags,
                channels: channels,
                private: ()
            })
        }
    }
}

impl ServiceSelector {
    /// Create a new selector that accepts all services.
    pub fn new() -> Self {
        Self::default()
    }

    /// Selector for a service with a specific id.
    pub fn with_id(self, id: &Id<ServiceId>) -> Self {
        ServiceSelector {
            id: self.id.and(Exactly::Exactly(id.clone())),
            .. self
        }
    }

    ///  Restrict results to services that have all the tags in `tags`.
    pub fn with_tags(self, tags: Vec<Id<TagId>>) -> Self {
        ServiceSelector {
            tags: merge(self.tags, tags),
            .. self
        }
    }

    /// Restrict results to services that have all the channels in `channels`.
    pub fn with_channels(mut self, mut channels: Vec<ChannelSelector>) -> Self {
        ServiceSelector {
            channels: {self.channels.append(&mut channels); self.channels},
            .. self
        }
    }

    /// Restrict results to services that are accepted by two selector.
    pub fn and(mut self, mut other: ServiceSelector) -> Self {
        ServiceSelector {
            id: self.id.and(other.id),
            tags: self.tags.union(&other.tags).cloned().collect(),
            channels: { self.channels.append(&mut other.channels); self.channels },
            private: (),
        }
    }

    pub fn matches<T>(&self, service: &T) -> bool
        where T: ServiceLike
    {
        if !self.id.matches(service.id()) {
            return false;
        }
        if !service.with_tags(|tags| has_selected_tags(&self.tags, tags)) {
            return false;
        }
        // If any of the getter selectors doesn't find a getter,
        // we don't match.
        let channels_fail = self.channels.iter().any(|selector| {
            !service.has_channels(|channel| {
                selector.matches(&self.tags, channel)
            })
        });
        if channels_fail {
            return false;
        }

        true
    }
}

impl SelectedBy<ServiceSelector> for Service {
    fn matches(&self, selector: &ServiceSelector) -> bool {
        selector.matches(self)
    }
}


/// A selector for one or more getter channels.
///
///
/// # Example
///
/// ```
/// use foxbox_taxonomy::selector::*;
/// use foxbox_taxonomy::services::*;
///
/// let selector = ChannelSelector::new()
///   .with_parent(&Id::new("foxbox"))
///   .with_feature(&Id::new("light/is-on"));
/// ```
///
/// # JSON
///
/// A selector is an object with the following fields:
///
/// - (optional) string `id`: accept only a channel with a given id;
/// - (optional) string `service`: accept only channels of a service with a given id;
/// - (optional) array of string `tags`:  accept only channels with all the tags in the array;
/// - (optional) array of string `service_tags`:  accept only channels of a service with all the
///        tags in the array;
/// - (optional) string|object `kind` (see `ChannelKind`): accept only channels of a given kind.
///
/// While each field is optional, at least one field must be provided.
///
/// ```
/// use foxbox_taxonomy::selector::*;
///
/// // A selector with all fields defined.
/// let json_selector = "{                         \
///   \"id\": \"setter 1\",                        \
///   \"service\": \"service 1\",                  \
///   \"tags\": [\"tag 1\", \"tag 2\"],            \
///   \"service_tags\": [\"tag 3\", \"tag 4\"],    \
///   \"kind\": \"Ready\"                          \
/// }";
///
/// ChannelSelector::from_str(json_selector).unwrap();
///
/// // The following will be rejected because no field is provided:
/// let json_empty = "{}";
/// match ChannelSelector::from_str(json_empty) {
///   Err(ParseError::EmptyObject {..}) => { /* as expected */ },
///   other => panic!("Unexpected result {:?}", other)
/// }
/// ```
#[derive(Clone, Debug, Deserialize, Default)]
pub struct ChannelSelector {
    /// If `Exactly(id)`, return only the channel with the corresponding id.
    pub id: Exactly<Id<Channel>>,

    /// If `Eactly(id)`, return only channels that are children of
    /// service `id`.
    pub parent: Exactly<Id<ServiceId>>,

    ///  Restrict results to channels that have all the tags in `tags`.
    pub tags: HashSet<Id<TagId>>,

    ///  Restrict results to channels offered by a service that has all the tags in `tags`.
    pub service_tags: HashSet<Id<TagId>>,

    /// If `Exactly(k)`, restrict results to channels that provide feature `k`
    pub feature: Exactly<Id<FeatureId>>,

    pub supports_send: Exactly<bool>,
    pub supports_fetch: Exactly<bool>,
    pub supports_watch: Exactly<bool>,

    /// Make sure that we can't instantiate from another crate.
    private: (),
}

impl Parser<ChannelSelector> for ChannelSelector {
    fn description() -> String {
        "ChannelSelector".to_owned()
    }
    fn parse(path: Path, source: &JSON) -> Result<Self, ParseError> {
        let mut is_empty = true;
        let id = try!(match path.push("id", |path| Exactly::take_opt(path, source, "id")) {
            None => Ok(Exactly::Always),
            Some(result) => {
                is_empty = false;
                result
            }
        });
        let service_id = try!(match path.push("service", |path| Exactly::take_opt(path, source, "service")) {
            None => Ok(Exactly::Always),
            Some(result) => {
                is_empty = false;
                result
            }
        });
        let tags : HashSet<_> = match path.push("tags", |path| Id::take_vec_opt(path, source, "tags")) {
            None => HashSet::new(),
            Some(Ok(mut vec)) => {
                is_empty = false;
                vec.drain(..).collect()
            }
            Some(Err(err)) => return Err(err),
        };
        let service_tags : HashSet<_> = match path.push("service_tags", |path| Id::take_vec_opt(path, source, "service_tags")) {
            None => HashSet::new(),
            Some(Ok(mut vec)) => {
                is_empty = false;
                vec.drain(..).collect()
            }
            Some(Err(err)) => return Err(err),
        };
        let feature = try!(match path.push("feature", |path| Exactly::take_opt(path, source, "feature")) {
            None => Ok(Exactly::Always),
            Some(result) => {
                is_empty = false;
                result
            }
        });
        let supports_send = try!(match path.push("supports_send", |path| Exactly::take_opt(path, source, "supports_send")) {
            None => Ok(Exactly::Always),
            Some(result) => result
        });
        let supports_fetch = try!(match path.push("supports_fetch", |path| Exactly::take_opt(path, source, "supports_fetch")) {
            None => Ok(Exactly::Always),
            Some(result) => result
        });
        let supports_watch = try!(match path.push("supports_watch", |path| Exactly::take_opt(path, source, "supports_watch")) {
            None => Ok(Exactly::Always),
            Some(result) => result
        });
        if is_empty {
            Err(ParseError::empty_object(&path))
        } else {
            Ok(ChannelSelector {
                id: id,
                parent: service_id,
                tags: tags,
                service_tags: service_tags,
                feature: feature,
                supports_send: supports_send,
                supports_fetch: supports_fetch,
                supports_watch: supports_watch,
                private: ()
            })
        }
    }
}

impl ChannelSelector {
    /// Create a new selector that accepts all getter channels.
    pub fn new() -> Self {
        Self::default()
    }

    /// Restrict to a channel with a specific id.
    pub fn with_id(self, id: &Id<Channel>) -> Self {
        ChannelSelector {
            id: self.id.and(Exactly::Exactly(id.clone())),
            .. self
        }
    }

    /// Restrict to a channel with a specific parent.
    pub fn with_parent(self, id: &Id<ServiceId>) -> Self {
        ChannelSelector {
            parent: self.parent.and(Exactly::Exactly(id.clone())),
            .. self
        }
    }

    /// Restrict to a channel with a specific kind.
    pub fn with_feature(self, feature: &Id<FeatureId>) -> Self {
        ChannelSelector {
            feature: self.feature.and(Exactly::Exactly(feature.clone())),
            .. self
        }
    }

    ///  Restrict to channels that have all the tags in `tags`.
    pub fn with_tags(self, tags: Vec<Id<TagId>>) -> Self {
        ChannelSelector {
            tags: merge(self.tags, tags),
            .. self
        }
    }

    pub fn with_supports_watch(self, value: Exactly<bool>) -> Self {
        ChannelSelector {
            supports_watch: self.supports_watch.and(value),
            .. self
        }
    }

    pub fn with_supports_fetch(self, value: Exactly<bool>) -> Self {
        ChannelSelector {
            supports_fetch: self.supports_fetch.and(value),
            .. self
        }
    }

    pub fn with_supports_send(self, value: Exactly<bool>) -> Self {
        ChannelSelector {
            supports_send: self.supports_send.and(value),
            .. self
        }
    }

    ///  Restrict to channels offered by a service that has all the tags in `tags`.
    pub fn with_service_tags(self, tags: Vec<Id<TagId>>) -> Self {
        ChannelSelector {
            service_tags: merge(self.service_tags, tags),
            .. self
        }
    }

    /// Restrict to channels that are accepted by two selector.
    pub fn and(self, other: Self) -> Self {
        ChannelSelector {
            id: self.id.and(other.id),
            parent: self.parent.and(other.parent),
            tags: self.tags.union(&other.tags).cloned().collect(),
            service_tags: self.service_tags.union(&other.service_tags).cloned().collect(),
            feature: self.feature.and(other.feature),
            supports_send: self.supports_send.and(other.supports_send),
            supports_fetch: self.supports_fetch.and(other.supports_fetch),
            supports_watch: self.supports_watch.and(other.supports_watch),
            private: (),
        }
    }

    /// Determine if a channel is matched by this selector.
    pub fn matches(&self, service_tags: &HashSet<Id<TagId>>, channel: &Channel) -> bool {
        if !self.id.matches(&channel.id) {
            return false;
        }
        if !self.parent.matches(&channel.service) {
            return false;
        }
        if !self.feature.matches(&channel.feature) {
            return false;
        }
        if !(&self.supports_send as &SelectedBy<_>).matches(&channel.supports_send) {
            return false;
        }
        if !(&self.supports_watch as &SelectedBy<_>).matches(&channel.supports_watch) {
            return false;
        }
        if !(&self.supports_fetch as &SelectedBy<_>).matches(&channel.supports_fetch) {
            return false;
        }
        if !has_selected_tags(&self.tags, &channel.tags) {
            return false;
        }
        if !has_selected_tags(&self.service_tags, service_tags) {
            return false;
        }
        true
    }
}

/// A parser for `ChannelSelector` that makes sure that the `feature` field is provided.
#[derive(Clone)]
pub struct ChannelSelectorWithFeature;

impl Parser<ChannelSelector> for ChannelSelectorWithFeature {
    fn description() -> String {
        "ChannelSelector (feature must be specified)".to_owned()
    }
    fn parse(path: Path, source: &JSON) -> Result<ChannelSelector, ParseError> {
        let selector = try!(ChannelSelector::parse(path.clone(), source));
        if let Exactly::Exactly(_) = selector.feature {
            Ok(selector)
        } else {
            Err(ParseError::missing_field("feature", &path))
        }
    }
}

fn has_selected_tags(actual: &HashSet<Id<TagId>>, requested: &HashSet<Id<TagId>>) -> bool {
    for tag in &*actual {
        if !requested.contains(tag) {
            return false;
        }
    }
    true
}
