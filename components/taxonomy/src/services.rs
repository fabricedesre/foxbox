//! This module defines the metadata on devices and services.
//!
//! Note that all the data structures in this module represent
//! snapshots of subsets of the devices available. None of these data
//! structures are live, so there is always the possibility that
//! devices may have been added or removed from the `FoxBox` by the time
//! these data structures are read.

use parse::*;
use values::*;
pub use util::{ Exactly, Id, AdapterId, ServiceId, KindId, TagId, VendorId };

use serde::ser::{ Serialize, Serializer };
use serde::de::{ Deserialize, Deserializer, Error };

use std::hash::{ Hash, Hasher };
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Getter channels connected directly to this service.
    pub getters: HashMap<Id<Channel>, Channel>,

    /// Setter channels connected directly to this service.
    pub setters: HashMap<Id<Channel>, Channel>,

    /// Identifier of the adapter for this service.
    pub adapter: Id<AdapterId>,
}

impl Service {
    /// Create an empty service.
    pub fn empty(id: Id<ServiceId>, adapter: Id<AdapterId>) -> Self {
        Service {
            tags: HashSet::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
            properties: HashMap::new(),
            id: id,
            adapter: adapter,
        }
    }
}

impl ToJSON for Service {
    fn to_json(&self) -> JSON {
        let mut source = vec![
            ("id", self.id.to_json()),
            ("adapter", self.adapter.to_json()),
            ("tags", self.tags.to_json()),
            ("properties", self.properties.to_json()),
            ("getters", self.getters.to_json()),
            ("setters", self.setters.to_json()),
        ];

        let map = source.drain(..)
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        JSON::Object(map)
    }
}


/// The kind of the channel, i.e. a strongly-typed description of
/// _what_ the channel can do. Used both for locating channels
/// (e.g. "I need a clock" or "I need something that can provide
/// pictures") and for determining the data structure that these
/// channel can provide or consume.
///
/// A number of channel kinds are standardized, and provided as a set
/// of strongly-typed enum constructors. It is clear, however, that
/// many devices will offer channels that cannot be described by
/// pre-existing constructors. For this purpose, this enumeration
/// offers a constructor `Extension`, designed to describe novel
/// channels.
///
/// # JSON
///
/// With the exception of the `Extension` kind, of all variants are
/// represented by a string with their name, e.g.
///
/// ```
/// use foxbox_taxonomy::services::*;
/// use foxbox_taxonomy::parse::*;
///
/// let parsed = ChannelKind::from_str("\"Ready\"").unwrap();
/// assert_eq!(parsed, ChannelKind::Ready);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChannelKind {
    /// The service is ready. Used for instance once a countdown has
    /// reached completion.
    ///
    /// # JSON
    ///
    /// This kind is represented by string "Ready".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"Ready\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::Ready);
    /// ```
    Ready,

    //
    // # Boolean
    //

    /// The service is used to detect or decide whether some light
    /// is on or off.
    ///
    /// # JSON
    ///
    /// This kind is represented by string "LightOn".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"LightOn\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::LightOn);
    /// ```
    LightOn,

    /// The service is used to detect or decide whether some device
    /// is open or closed.
    ///
    /// # JSON
    ///
    /// This kind is represented by string "OpenClosed".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"OpenClosed\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::OpenClosed);
    /// ```
    OpenClosed,

    /// The service is used to deterct or decide whether a door lock
    /// is locked or unlocked.
    /// # JSON
    ///
    /// This kind is represented by string "DoorLocked".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"DoorLocked\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::DoorLocked);
    /// ```
    DoorLocked,

    //
    // # String
    //

    /// The service is used to read or set the username associated with the
    /// service. This is typically used for devices which have additional
    /// authentication (like an IP Camera).
    ///
    /// # JSON
    ///
    /// This kind is represented by the string "Username".
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"Username\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::Username);
    /// ```
    Username,

    /// The service is used to read or set the password associated with the
    /// service. This is typically used for devices which have additional
    /// authentication (like an IP Camera).
    ///
    /// # JSON
    ///
    /// This kind is represented by the string "Password".
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"Password\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::Password);
    /// ```
    Password,

    //
    // # Time
    //

    /// The service is used to execute something after a given delay.
    ///
    /// # JSON
    ///
    /// This kind is represented by string "Countdown".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"Countdown\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::Countdown);
    /// ```
    Countdown,

    /// The service is used to execute something at a regular interval.
    ///
    /// # JSON
    ///
    /// This kind is represented by string "CountEveryInterval".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"CountEveryInterval\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::CountEveryInterval);
    /// ```
    CountEveryInterval,

    /// The service is used to read or set the current absolute time.
    /// Used for instance to wait until a specific time and day before
    /// triggering an action, or to set the appropriate time on a new
    /// device.
    ///
    /// # JSON
    ///
    /// This kind is represented by string "CurrentTime".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"CurrentTime\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::CurrentTime);
    /// ```
    CurrentTime,

    /// The service is used to read or set the current time of day.
    /// Used for instance to trigger an action at a specific hour
    /// every day.
    ///
    /// # JSON
    ///
    /// This kind is represented by string "CurrentTimeOfDay".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"CurrentTimeOfDay\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::CurrentTimeOfDay);
    /// ```
    CurrentTimeOfDay,

    /// The service is part of a countdown. This is the time
    /// remaining until the countdown is elapsed.
    ///
    /// # JSON
    ///
    /// This kind is represented by string "RemainingTime".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"RemainingTime\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::RemainingTime);
    /// ```
    RemainingTime,

    //
    // # Temperature
    //

    ///
    /// # JSON
    ///
    /// This kind is represented by string "OvenTemperature".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = ChannelKind::from_str("\"OvenTemperature\"").unwrap();
    /// assert_eq!(parsed, ChannelKind::OvenTemperature);
    /// ```
    OvenTemperature,

    //
    // # Thinkerbell
    //

    AddThinkerbellRule,
    RemoveThinkerbellRule,
    ThinkerbellRuleSource,
    ThinkerbellRuleOn,

    /// Capture a new snapshot.
    ///
    /// # JSON
    ///
    /// This kind is represented by string "TakeSnapshot".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let source = r#""TakeSnapshot""#;
    ///
    /// let parsed = ChannelKind::from_str(source).unwrap();
    /// assert_eq!(parsed, ChannelKind::TakeSnapshot);
    ///
    /// let serialized = parsed.to_json();
    /// assert_eq!(serialized.as_string().unwrap(), "TakeSnapshot");
    /// ```
    TakeSnapshot,

    /// Write to a log file
    ///
    /// # JSON
    ///
    /// This kind is represented by string "Log".
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let source = r#""Log""#;
    ///
    /// let parsed = ChannelKind::from_str(source).unwrap();
    /// assert_eq!(parsed, ChannelKind::Log);
    ///
    /// let serialized = parsed.to_json();
    /// assert_eq!(serialized.as_string().unwrap(), "Log");
    /// ```
    Log,

    //
    // # WebPush
    //

    WebPushNotify,

    // TODO: Add more

    /// An operation of a kind that has not been standardized yet.
    ///
    /// # JSON
    ///
    /// This kind is represented by an object with the following fields:
    ///
    /// - string `vendor`
    /// - string `adapter`
    /// - string `kind`
    /// - string `type` (see Type)
    ///
    /// ```
    /// use foxbox_taxonomy::services::*;
    /// use foxbox_taxonomy::parse::*;
    /// use foxbox_taxonomy::values::*;
    ///
    /// let source = "{
    ///   \"vendor\": \"mozilla.org\",
    ///   \"adapter\": \"foxlink@mozilla.org\",
    ///   \"kind\": \"GroundHumidity\",
    ///   \"type\": \"ExtNumeric\"
    /// }";
    ///
    /// let parsed = ChannelKind::from_str(source).unwrap();
    ///
    /// if let ChannelKind::Extension { vendor, adapter, kind, typ } = parsed {
    ///   assert_eq!(vendor.to_string(), "mozilla.org");
    ///   assert_eq!(adapter.to_string(), "foxlink@mozilla.org");
    ///   assert_eq!(kind.to_string(), "GroundHumidity");
    ///   assert_eq!(typ, Type::ExtNumeric);
    /// } else {
    ///   panic!()
    /// }
    ///
    /// ```
    Extension {
        /// The vendor. Used for namespacing purposes, to avoid
        /// confusing two incompatible extensions with similar
        /// names. For instance, "foxlink@mozilla.com".
        vendor: Id<VendorId>,

        /// Identification of the adapter introducing this operation.
        /// Designed to aid with tracing and debugging.
        adapter: Id<AdapterId>,

        /// A string describing the nature of the value, designed to
        /// let applications discover the devices.
        ///
        /// Examples: `"GroundHumidity"`.
        kind: Id<KindId>,

        /// The data type of the value.
        typ: Type
    }
}

impl Parser<ChannelKind> for ChannelKind {
    fn description() -> String {
        "ChannelKind".to_owned()
    }
    /// Parse a single value from JSON, consuming as much as necessary from JSON.
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        if let Some(str) = source.as_string() {
            return match str {
                "Ready" => Ok(ChannelKind::Ready),
                "LightOn" => Ok(ChannelKind::LightOn),
                "OpenClosed" => Ok(ChannelKind::OpenClosed),
                "DoorLocked" => Ok(ChannelKind::DoorLocked),
                "Username" => Ok(ChannelKind::Username),
                "Password" => Ok(ChannelKind::Password),
                "Countdown" => Ok(ChannelKind::Countdown),
                "CountEveryInterval" => Ok(ChannelKind::CountEveryInterval),
                "CurrentTime" => Ok(ChannelKind::CurrentTime),
                "CurrentTimeOfDay" => Ok(ChannelKind::CurrentTimeOfDay),
                "AddThinkerbellRule" => Ok(ChannelKind::AddThinkerbellRule),
                "RemoveThinkerbellRule" => Ok(ChannelKind::RemoveThinkerbellRule),
                "ThinkerbellRuleSource" => Ok(ChannelKind::ThinkerbellRuleSource),
                "ThinkerbellRuleOn" => Ok(ChannelKind::ThinkerbellRuleOn),
                "RemainingTime" => Ok(ChannelKind::RemainingTime),
                "OvenTemperature" => Ok(ChannelKind::OvenTemperature),
                "TakeSnapshot" => Ok(ChannelKind::TakeSnapshot),
                "Log" => Ok(ChannelKind::Log),
                "WebPushNotify" => Ok(ChannelKind::WebPushNotify),
                _ => Err(ParseError::unknown_constant(str, &path))
            }
        }
        if source.is_object() {
            for key in &["vendor", "adapter", "kind", "type"] {
                if source.find(key).is_none() {
                    return Err(ParseError::type_error("ChannelKind", &path, "string|object {vendor, adapter, kind, type}"))
                }
            }
            let vendor = try!(path.push("vendor", |path| Id::take(path, source, "vendor")));
            let adapter = try!(path.push("adapter", |path| Id::take(path, source, "adapter")));
            let kind = try!(path.push("kind", |path| Id::take(path, source, "kind")));
            let typ = try!(path.push("type", |path| Type::take(path, source, "type")));
            Ok(ChannelKind::Extension {
                vendor: vendor,
                adapter: adapter,
                kind: kind,
                typ: typ
            })
        } else {
            Err(ParseError::type_error("ChannelKind", &path, "string|object {vendor, adapter, kind, type}"))
        }
    }
}

impl ToJSON for ChannelKind {
    fn to_json(&self) -> JSON {
        use self::ChannelKind::*;
        match *self {
            Ready => JSON::String("Ready".to_owned()),
            LightOn => JSON::String("LightOn".to_owned()),
            OpenClosed => JSON::String("OpenClosed".to_owned()),
            DoorLocked => JSON::String("DoorLocked".to_owned()),
            Username => JSON::String("Username".to_owned()),
            Password => JSON::String("Password".to_owned()),
            CurrentTime => JSON::String("CurrentTime".to_owned()),
            CurrentTimeOfDay => JSON::String("CurrentTimeOfDay".to_owned()),
            CountEveryInterval => JSON::String("CountEveryInterval".to_owned()),
            Countdown => JSON::String("Countdown".to_owned()),
            RemainingTime => JSON::String("RemainingTime".to_owned()),
            OvenTemperature => JSON::String("OvenTemperature".to_owned()),
            AddThinkerbellRule => JSON::String("AddThinkerbellRule".to_owned()),
            RemoveThinkerbellRule => JSON::String("RemoveThinkerbellRule".to_owned()),
            ThinkerbellRuleSource => JSON::String("ThinkerbellRuleSource".to_owned()),
            ThinkerbellRuleOn => JSON::String("ThinkerbellRuleOn".to_owned()),
            TakeSnapshot => JSON::String("TakeSnapshot".to_owned()),
            Log => JSON::String("Log".to_owned()),
            WebPushNotify => JSON::String("WebPushNotify".to_owned()),
            Extension { ref vendor, ref adapter, ref kind, ref typ } => {
                vec![
                    ("vendor", vendor.to_json()),
                    ("adapter", adapter.to_json()),
                    ("kind", kind.to_json()),
                    ("type", typ.to_json()),
                ].to_json()
            }
        }
    }
}

impl ChannelKind {
    /// Get the type of values used to communicate with this service.
    pub fn get_type(&self) -> Type {
        use self::ChannelKind::*;
        use values::Type;
        match *self {
            Ready => Type::Unit,
            LightOn => Type::OnOff,
            OpenClosed => Type::OpenClosed,
            DoorLocked => Type::DoorLocked,
            CurrentTime => Type::TimeStamp,
            CurrentTimeOfDay | RemainingTime | Countdown | CountEveryInterval => Type::Duration,
            OvenTemperature => Type::Temperature,
            AddThinkerbellRule => Type::ThinkerbellRule,
            RemoveThinkerbellRule => Type::Unit,
			ThinkerbellRuleSource => Type::String,
            ThinkerbellRuleOn => Type::OnOff,
            Log => Type::String,
            TakeSnapshot => Type::Unit,
			Username | Password => Type::String,
            WebPushNotify => Type::WebPushNotify,
            Extension { ref typ, ..} => typ.clone(),
        }
    }
}

/// An channel represents a single place where data can enter or
/// leave a device. Note that channels support either a single kind
/// of getter or a single kind of setter. Devices that support both
/// getters or setters, or several kinds of getters, or several kinds of
/// setters, are represented as services containing several channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    /// Tags describing the channel.
    ///
    /// These tags can be set by the user, adapters or
    /// applications. They are used to regroup channels for rules.
    ///
    /// For instance "entrance".
    #[serde(default)]
    pub tags: HashSet<Id<TagId>>,

    /// An id unique to this channel.
    pub id: Id<Channel>,

    /// The service owning this channel.
    pub service: Id<ServiceId>,

    /// Identifier of the adapter for this channel.
    pub adapter: Id<AdapterId>,

    pub kind: ChannelKind,
}

impl ToJSON for Channel {
    fn to_json(&self) -> JSON {
        let mut source = vec![
            ("id", self.id.to_json()),
            ("adapter", self.adapter.to_json()),
            ("tags", self.tags.to_json()),
            ("service", self.service.to_json()),
            ("kind", self.kind.to_json()),
        ];

        let map = source.drain(..)
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        JSON::Object(map)
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
