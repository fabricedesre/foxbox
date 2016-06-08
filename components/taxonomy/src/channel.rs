use io::*;
use parse::*;
use util::*;
use values::*;

use std::collections::HashSet;
use std::hash::{ Hash, Hasher };
use std::sync::Arc;


#[derive(Debug, Clone)]
pub struct Signature {
    pub accepts: Maybe<Arc<Format>>,
    pub returns: Maybe<Arc<Format>>
}
impl Signature {
    /// Shortcut for building a signature that accepts some arg, returns nothing.
    pub fn accepts(spec: Maybe<Arc<Format>>) -> Self {
        Signature {
            accepts: spec,
            returns: Maybe::Nothing
        }
    }

    /// Shortcut for building a signature that accepts nothing, returns some value.
    pub fn returns(spec: Maybe<Arc<Format>>) -> Self {
        Signature {
            returns: spec,
            accepts: Maybe::Nothing
        }
    }

    pub fn nothing() -> Self {
        Signature {
            returns: Maybe::Nothing,
            accepts: Maybe::Nothing
        }
    }
}

impl ToJSON for Signature {
    fn to_json(&self) -> JSON {
        let mut vec = vec![];
        for &(key, value) in &[("accepts", &self.accepts), ("returns", &self.returns)] {
            let spec;
            match *value {
                Maybe::Nothing => continue,
                Maybe::Required(ref format) => {
                    spec = vec![("requires", format.description())]
                },
                Maybe::Optional(ref format) => {
                    spec = vec![("optional", format.description())]
                }
            }
            vec.push((key, spec.to_json()))
        }
        vec.to_json()
    }
}

#[derive(Clone, Debug, Default)]
pub struct FeatureId;

/// An channel represents a single place where data can enter or
/// leave a device. Note that channels support either a single kind
/// of getter or a single kind of setter. Devices that support both
/// getters or setters, or several kinds of getters, or several kinds of
/// setters, are represented as services containing several channels.
#[derive(Debug, Default, Clone)]
pub struct Channel {
    /// Tags describing the channel.
    ///
    /// These tags can be set by the user, adapters or
    /// applications. They are used to regroup channels for rules.
    ///
    /// For instance "entrance".
    pub tags: HashSet<Id<TagId>>,

    /// An id unique to this channel.
    pub id: Id<Channel>,

    /// The service owning this channel.
    pub service: Id<ServiceId>,

    /// Identifier of the adapter for this channel.
    pub adapter: Id<AdapterId>,

    /// Description of the feature, in a format designed for discovery by applications.
    ///
    /// By convention, developers should prefix with "x-" for features that are not standardized yet.
    ///
    /// # Picking a good name
    ///
    /// - `"string"` - **BAD**: not a feature. What does this string represent? Is it a name? A password? etc.
    /// - `"bool"` - **BAD**: not a feature. What does this bool represent?
    /// - `"onoff"` - **BAD**: ambigous feature. Does this turn off the light or the safety?
    ///      Could lead to physical accidents.
    /// - `"light/onoff"` - **OK**: unambiguous, if the format of messages is "on"/"off". Just don't
    ///      accept booleans here.
    /// - `"light/on"` - **GOOD**: unambiguous.
    /// - `"temperature"` - **BAD**: ambiguous feature. Does this set the temperature of a heater, an
    //       oven or a blowtorch? Could lead to physical accidents.
    /// - `"heater/temperature"` - **BAD** ambiguous format. Is the temperature in ºC, ºF or K? Could
    ///      lead to accident.
    /// - `"heater/temperature-c"` - **GOOD** we know the feature, the device and the unit.
    /// - `"camera/image"` - **OK** slightly ambiguous, as the camera could decide of a format,
    ///      a resolution, etc.
    /// - `"camera/image-png"` - **GOOD** we know the feature, the device and the format. Note that
    ///      we still don't know the resolution.
    /// - `"mydevice/warpspeed"` - **BAD** that doesn't look standard, prefer `"x-mydevice/x-warpspeed"`.
    ///
    /// # Good descriptions
    ///
    /// - "heater/temperature-c" - all the data is provided to avoid accidents.
    /// - "light/is-on"
    pub feature: Id<FeatureId>,

    /// The format used by operation `Send`.
    ///
    /// If `None`, this channel does not support operation `Send`.
    /// If `Some`, this channel supports the operation. See the signature
    /// to determine the type of values that are accepted and returned by
    /// the channel.
    pub supports_send: Option<Signature>,

    /// The format used by operation `Fetch`.
    ///
    /// If `None`, this channel does not support operation `Fetch`.
    /// If `Some`, this channel supports the operation. See the signature
    /// to determine the type of values that are accepted and returned by
    /// the channel.
    pub supports_fetch: Option<Signature>,


    /// The format used by operation `Watch`.
    ///
    /// If `None`, this channel does not support operation `Watch`.
    /// If `Some`, this channel supports the operation. See the signature
    /// to determine the type of values that may serve as condition
    /// and may be notified by the channel.
    pub supports_watch: Option<Signature>,
}


impl ToJSON for Channel {
    fn to_json(&self) -> JSON {
        vec![
            ("id", self.id.to_json()),
            ("adapter", self.adapter.to_json()),
            ("tags", self.tags.to_json()),
            ("service", self.service.to_json()),
            ("feature", self.feature.to_json()),
            ("supports_send", self.supports_send.to_json()),
            ("supports_fetch", self.supports_fetch.to_json()),
        ].to_json()
    }
}

impl Eq for Channel {
}

impl PartialEq for Channel {
     fn eq(&self, other: &Self) -> bool {
         self.id.eq(&other.id)
     }
}

impl Hash for Channel {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.id.hash(state)
    }
}


lazy_static! {
    /// Standardized channel: determine whether a door is locked (not simply closed).
    ///
    /// Features:
    /// - fetch from this channel to determine whether the door is locked;
    /// - send to this channel to lock/unlock it;
    /// - watch this channel to be informed when it is (un)locked.
    pub static ref DOOR_IS_LOCKED : Channel = Channel {
        feature: Id::new("door/is-locked"),
        supports_send: Some(Signature::accepts(Maybe::Required(format::OPEN_CLOSED.clone()))),
        supports_fetch: Some(Signature::returns(Maybe::Required(format::OPEN_CLOSED.clone()))),
        supports_watch: Some(Signature::returns(Maybe::Required(format::OPEN_CLOSED.clone()))),
        .. Channel::default()
    };

    /// Standardized channel: determine whether a door is opened.
    ///
    /// Features:
    /// - fetch from this channel to determine whether the door is opened;
    /// - send to this channel to open/close it;
    /// - watch this channel to be informed when it is opened/closed.
    pub static ref DOOR_IS_OPEN : Channel = Channel {
        feature: Id::new("door/is-open"),
        supports_send: Some(Signature::accepts(Maybe::Required(format::OPEN_CLOSED.clone()))),
        supports_fetch: Some(Signature::returns(Maybe::Required(format::OPEN_CLOSED.clone()))),
        supports_watch: Some(Signature::returns(Maybe::Required(format::OPEN_CLOSED.clone()))),
        .. Channel::default()
    };

    /// Standardized channel: determine whether a light is on.
    ///
    /// Features:
    /// - fetch from this channel to determine whether the light is on;
    /// - send to this channel to turn the light on/off;
    /// - watch this channel to be informed when it is turned on/off.
    pub static ref LIGHT_IS_ON : Channel = Channel {
        feature: Id::new("light/is-on"),
        supports_send: Some(Signature::accepts(Maybe::Required(format::ON_OFF.clone()))),
        supports_fetch: Some(Signature::returns(Maybe::Required(format::ON_OFF.clone()))),
        supports_watch: Some(Signature::returns(Maybe::Required(format::ON_OFF.clone()))),
        .. Channel::default()
    };

    /// Standardized channel: determine the color of a light.
    pub static ref LIGHT_COLOR_HSV : Channel = Channel {
        feature: Id::new("light/color-hsv"),
        supports_send: Some(Signature::accepts(Maybe::Required(format::COLOR.clone()))),
        supports_fetch: Some(Signature::returns(Maybe::Required(format::COLOR.clone()))),
        supports_watch: Some(Signature::returns(Maybe::Required(format::COLOR.clone()))),
        .. Channel::default()
    };

    /// Standardized channel: log text to a console, a file, etc.
    ///
    /// Features:
    /// - send to this channel to log a string.
    pub static ref LOG: Channel = Channel {
        feature: Id::new("log/append-text"),
        supports_send: Some(Signature::accepts(Maybe::Required(format::STRING.clone()))),
        .. Channel::default()
    };

    /// Standardized channel: access the username of a device.
    pub static ref USERNAME: Channel = Channel {
        feature: Id::new("security/username"),
        supports_send: Some(Signature::accepts(Maybe::Required(format::STRING.clone()))),
        supports_fetch: Some(Signature::returns(Maybe::Required(format::STRING.clone()))),
        .. Channel::default()
    };

    /// Standardized channel: access the password of a device.
    pub static ref PASSWORD: Channel = Channel {
        feature: Id::new("security/password"),
        supports_send: Some(Signature::accepts(Maybe::Required(format::STRING.clone()))),
        supports_fetch: Some(Signature::returns(Maybe::Required(format::STRING.clone()))),
        .. Channel::default()
    };

    /// Standardized channel: determine whether a device is currently accessible.
    pub static ref AVAILABLE: Channel = Channel {
        feature: Id::new("device/available"),
        supports_fetch: Some(Signature::returns(Maybe::Required(format::ON_OFF.clone()))),
        .. Channel::default()
    };
}

