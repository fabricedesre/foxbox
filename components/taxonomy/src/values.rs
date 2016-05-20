//!
//! Values manipulated by services
//!
use parse::*;
use util::*;

use std::cmp::{ PartialOrd, Ordering };
use std::collections::HashMap;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;
use std::{ error, fmt };

use chrono::{ Duration as ChronoDuration, DateTime, Local, TimeZone, UTC };

use serde_json;
use serde::ser::{ Serialize, Serializer };
use serde::de::{ Deserialize, Deserializer, Error, Visitor };

/// Representation of a type error.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TypeError {
    /// The type we expected.
    pub expected: Type,

    /// The type we actually got.
    pub got: Type,
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Expected {:?} but got {:?}", self.expected, self.got)
    }
}

impl error::Error for TypeError {
    fn description(&self) -> &str {
        "Expected a type but got another type"
    }

    fn cause(&self) -> Option<&error::Error> {
        None
    }
}

///
/// The type of values manipulated by endpoints.
///
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Serialize, Deserialize)]
pub enum Type {
    ///
    /// # Trivial values
    ///

    /// An empty value. Used for instance to inform that a countdown
    /// has reached 0 or that a device is ready.
    Unit,

    ///
    /// # Boolean values
    ///

    /// A boolean on/off state. Used for various two-states switches.
    OnOff,

    /// A boolean open/closed state. Used for instance for doors,
    /// windows, etc.
    OpenClosed,

    /// A boolean locked/unlocked states. Used for door locks.
    DoorLocked,

    /// A boolean secure/insecure states. Used for zwave pairing and zwave secure devices.
    IsSecure,

    ///
    /// # Time
    ///

    /// A duration. Used for instance in countdowns.
    Duration,

    /// A precise timestamp. Used for instance to determine when an
    /// event has taken place.
    TimeStamp,

    ThinkerbellRule,

    WebPushNotify,

    Temperature,
    String,
    ///
    /// ...
    ///
    Color,
    Json,
    Binary,

    ExtBool,
    ExtNumeric,
    Range,
}
impl Parser<Type> for Type {
    fn description() -> String {
        "Type".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        use self::Type::*;
        match *source {
            JSON::String(ref string) => match &*string as &str {
                "Unit" => Ok(Unit),
                "OnOff" => Ok(OnOff),
                "OpenClosed" => Ok(OpenClosed),
                "DoorLocked" => Ok(DoorLocked),
                "IsSecure" => Ok(IsSecure),
                "Duration" => Ok(Duration),
                "TimeStamp" => Ok(TimeStamp),
                "Temperature" => Ok(Temperature),
                "ThinkerbellRule" => Ok(ThinkerbellRule),
                "WebPushNotify" => Ok(WebPushNotify),
                "String" => Ok(String),
                "Color" => Ok(Color),
                "Json" => Ok(Json),
                "Binary" => Ok(Binary),
                "ExtBool" => Ok(ExtBool),
                "ExtNumeric" => Ok(ExtNumeric),
                "Range" => Ok(Range),
                _ => Err(ParseError::unknown_constant(string, &path))
            },
            _ => Err(ParseError::type_error("Type", &path, "string"))
        }
    }
}
impl ToJSON for Type {
    fn to_json(&self) -> JSON {
        use self::Type::*;
        let key = match *self {
            Unit => "Unit",
            OnOff => "OnOff",
            OpenClosed => "OpenClosed",
            DoorLocked => "DoorLocked",
            IsSecure => "IsSecure",
            Duration => "Duration",
            TimeStamp => "TimeStamp",
            Temperature => "Temperature",
            ThinkerbellRule => "ThinkerbellRule",
            WebPushNotify => "WebPushNotify",
            String => "String",
            Color => "Color",
            Json => "Json",
            Binary => "Binary",
            ExtBool => "ExtBool",
            ExtNumeric => "ExtNumeric",
            Range => "Range",
        };
        JSON::String(key.to_owned())
    }
}

impl Type {
    /// Determine whether using `Range::Eq` for this type is
    /// appropriate. Typically, using `Range::Eq` for a floating point
    /// number is a bad idea.
    pub fn supports_eq(&self) -> bool {
        use self::Type::*;
        match *self {
            Duration | TimeStamp | Temperature | ExtNumeric | Color | ThinkerbellRule => false,
            WebPushNotify | Unit | String | Json | Binary | OnOff | OpenClosed |
            IsSecure | DoorLocked | ExtBool => true,
            Range => false,
        }
    }

    pub fn ensure_eq(&self, other: &Self) -> Result<(), TypeError> {
        if self == other {
            Ok(())
        } else {
            Err(TypeError {
                expected: self.clone(),
                got: other.clone(),
            })
        }
    }
}

/// An on/off state.
///
/// # JSON
///
/// This kind is represented by strings "On" | "Off".
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum OnOff {
    /// # JSON
    ///
    /// Represented by "On".
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = OnOff::from_str("\"On\"").unwrap();
    /// assert_eq!(parsed, OnOff::On);
    ///
    /// let serialized: JSON = OnOff::On.to_json();
    /// assert_eq!(serialized.as_string().unwrap(), "On");
    /// ```
    On,

    /// # JSON
    ///
    /// Represented by "Off".
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = OnOff::from_str("\"Off\"").unwrap();
    /// assert_eq!(parsed, OnOff::Off);
    ///
    /// let serialized: JSON = OnOff::Off.to_json();
    /// assert_eq!(serialized.as_string().unwrap(), "Off");
    /// ```
    Off,
}

impl OnOff {
    fn as_bool(&self) -> bool {
        match *self {
            OnOff::On => true,
            OnOff::Off => false,
        }
    }
}

impl Parser<OnOff> for OnOff {
    fn description() -> String {
        "OnOff".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        match source.as_string() {
            Some("On") => Ok(OnOff::On),
            Some("Off") => Ok(OnOff::Off),
            Some(str) => Err(ParseError::unknown_constant(str, &path)),
            None => Err(ParseError::type_error("OnOff", &path, "string"))
        }
    }
}

impl ToJSON for OnOff {
    fn to_json(&self) -> JSON {
        match *self {
            OnOff::On => JSON::String("On".to_owned()),
            OnOff::Off => JSON::String("Off".to_owned())
        }
    }
}
impl Into<Value> for OnOff {
    fn into(self) -> Value {
        Value::OnOff(self)
    }
}

impl PartialOrd for OnOff {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OnOff {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bool().cmp(&other.as_bool())
    }
}

///
/// # (De)serialization
///
/// Values of this type are represented by strings "On" | "Off".
///
/// ```
/// extern crate serde;
/// extern crate serde_json;
/// extern crate foxbox_taxonomy;
///
/// let on = serde_json::to_string(&foxbox_taxonomy::values::OnOff::On).unwrap();
/// assert_eq!(on, "\"On\"");
///
/// let on : foxbox_taxonomy::values::OnOff = serde_json::from_str("\"On\"").unwrap();
/// assert_eq!(on, foxbox_taxonomy::values::OnOff::On);
///
/// let off = serde_json::to_string(&foxbox_taxonomy::values::OnOff::Off).unwrap();
/// assert_eq!(off, "\"Off\"");
///
/// let off : foxbox_taxonomy::values::OnOff = serde_json::from_str("\"Off\"").unwrap();
/// assert_eq!(off, foxbox_taxonomy::values::OnOff::Off);
/// ```
impl Serialize for OnOff {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error> where S: Serializer {
        match *self {
            OnOff::On => "On".serialize(serializer),
            OnOff::Off => "Off".serialize(serializer)
        }
    }
}
impl Deserialize for OnOff {
    fn deserialize<D>(deserializer: &mut D) -> Result<Self, D::Error> where D: Deserializer {
        deserializer.deserialize_string(TrivialEnumVisitor::new(|source| {
            match source {
                "On" => Ok(OnOff::On),
                "Off" => Ok(OnOff::Off),
                _ => Err(())
            }
        }))
    }
}

/// An open/closed state.
///
/// # JSON
///
/// Values of this type are represented by strings "Open" | "Closed".
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum OpenClosed {
    /// # JSON
    ///
    /// Represented by "Open".
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = OpenClosed::from_str("\"Open\"").unwrap();
    /// assert_eq!(parsed, OpenClosed::Open);
    ///
    /// let serialized: JSON = OpenClosed::Open.to_json();
    /// assert_eq!(serialized.as_string().unwrap(), "Open");
    /// ```
    Open,

    /// # JSON
    ///
    /// Represented by "Closed".
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = OpenClosed::from_str("\"Closed\"").unwrap();
    /// assert_eq!(parsed, OpenClosed::Closed);
    ///
    /// let serialized: JSON = OpenClosed::Closed.to_json();
    /// assert_eq!(serialized.as_string().unwrap(), "Closed");
    /// ```
    Closed,
}

impl OpenClosed {
    fn as_bool(&self) -> bool {
        match *self {
            OpenClosed::Open => true,
            OpenClosed::Closed => false,
        }
    }
}

impl Parser<OpenClosed> for OpenClosed {
    fn description() -> String {
        "OpenClosed".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        match source.as_string() {
            Some("Open") => Ok(OpenClosed::Open),
            Some("Closed") => Ok(OpenClosed::Closed),
            Some(str) => Err(ParseError::unknown_constant(str, &path)),
            None => Err(ParseError::type_error("OpenClosed", &path, "string"))
        }
    }
}

impl ToJSON for OpenClosed {
    fn to_json(&self) -> JSON {
        match *self {
            OpenClosed::Open => JSON::String("Open".to_owned()),
            OpenClosed::Closed => JSON::String("Closed".to_owned())
        }
    }
}
impl Into<Value> for OpenClosed {
    fn into(self) -> Value {
        Value::OpenClosed(self)
    }
}

impl PartialOrd for OpenClosed {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OpenClosed {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bool().cmp(&other.as_bool())
    }
}

///
/// # (De)serialization
///
/// Values of this state are represented by strings "Open"|"Closed".
///
/// ```
/// extern crate serde;
/// extern crate serde_json;
/// extern crate foxbox_taxonomy;
///
/// let open = serde_json::to_string(&foxbox_taxonomy::values::OpenClosed::Open).unwrap();
/// assert_eq!(open, "\"Open\"");
///
/// let open : foxbox_taxonomy::values::OpenClosed = serde_json::from_str("\"Open\"").unwrap();
/// assert_eq!(open, foxbox_taxonomy::values::OpenClosed::Open);
///
/// let closed = serde_json::to_string(&foxbox_taxonomy::values::OpenClosed::Closed).unwrap();
/// assert_eq!(closed, "\"Closed\"");
///
/// let closed : foxbox_taxonomy::values::OpenClosed = serde_json::from_str("\"Closed\"").unwrap();
/// assert_eq!(closed, foxbox_taxonomy::values::OpenClosed::Closed);
/// ```
impl Serialize for OpenClosed {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error> where S: Serializer {
        match *self {
            OpenClosed::Open => "Open".serialize(serializer),
            OpenClosed::Closed => "Closed".serialize(serializer)
        }
    }
}
impl Deserialize for OpenClosed {
    fn deserialize<D>(deserializer: &mut D) -> Result<Self, D::Error> where D: Deserializer {
        deserializer.deserialize_string(TrivialEnumVisitor::new(|source| {
            match source {
                "Open" | "open" => Ok(OpenClosed::Open),
                "Closed" | "closed" => Ok(OpenClosed::Closed),
                _ => Err(())
            }
        }))
    }
}

/// An locked/unlocked state.
///
/// # JSON
///
/// Values of this type are represented by strings "Locked" | "Unlocked".
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DoorLocked {
    /// # JSON
    ///
    /// Represented by "Locked".
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = DoorLocked::from_str("\"Locked\"").unwrap();
    /// assert_eq!(parsed, DoorLocked::Locked);
    ///
    /// let serialized: JSON = DoorLocked::Locked.to_json();
    /// assert_eq!(serialized.as_string().unwrap(), "Locked");
    /// ```
    Locked,

    /// # JSON
    ///
    /// Represented by "Unlocked".
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = DoorLocked::from_str("\"Unlocked\"").unwrap();
    /// assert_eq!(parsed, DoorLocked::Unlocked);
    ///
    /// let serialized: JSON = DoorLocked::Unlocked.to_json();
    /// assert_eq!(serialized.as_string().unwrap(), "Unlocked");
    /// ```
    Unlocked,
}

impl DoorLocked {
    fn as_bool(&self) -> bool {
        match *self {
            DoorLocked::Locked => true,
            DoorLocked::Unlocked => false,
        }
    }
}

impl Parser<DoorLocked> for DoorLocked {
    fn description() -> String {
        "DoorLocked".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        match source.as_string() {
            Some("Locked") => Ok(DoorLocked::Locked),
            Some("Unlocked") => Ok(DoorLocked::Unlocked),
            Some(str) => Err(ParseError::unknown_constant(str, &path)),
            None => Err(ParseError::type_error("DoorLocked", &path, "string"))
        }
    }
}

impl ToJSON for DoorLocked {
    fn to_json(&self) -> JSON {
        match *self {
            DoorLocked::Locked => JSON::String("Locked".to_owned()),
            DoorLocked::Unlocked => JSON::String("Unlocked".to_owned())
        }
    }
}
impl Into<Value> for DoorLocked {
    fn into(self) -> Value {
        Value::DoorLocked(self)
    }
}

impl PartialOrd for DoorLocked {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DoorLocked {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bool().cmp(&other.as_bool())
    }
}

///
/// # (De)serialization
///
/// Values of this state are represented by strings "Locked"|"Unlocked".
///
/// ```
/// extern crate serde;
/// extern crate serde_json;
/// extern crate foxbox_taxonomy;
///
/// let locked = serde_json::to_string(&foxbox_taxonomy::values::DoorLocked::Locked).unwrap();
/// assert_eq!(locked, "\"Locked\"");
///
/// let locked : foxbox_taxonomy::values::DoorLocked = serde_json::from_str("\"Locked\"").unwrap();
/// assert_eq!(locked, foxbox_taxonomy::values::DoorLocked::Locked);
///
/// let unlocked = serde_json::to_string(&foxbox_taxonomy::values::DoorLocked::Unlocked).unwrap();
/// assert_eq!(unlocked, "\"Unlocked\"");
///
/// let unlocked : foxbox_taxonomy::values::DoorLocked = serde_json::from_str("\"Unlocked\"").unwrap();
/// assert_eq!(unlocked, foxbox_taxonomy::values::DoorLocked::Unlocked);
/// ```
impl Serialize for DoorLocked {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error> where S: Serializer {
        match *self {
            DoorLocked::Locked => "Locked".serialize(serializer),
            DoorLocked::Unlocked => "Unlocked".serialize(serializer)
        }
    }
}
impl Deserialize for DoorLocked {
    fn deserialize<D>(deserializer: &mut D) -> Result<Self, D::Error> where D: Deserializer {
        deserializer.deserialize_string(TrivialEnumVisitor::new(|source| {
            match source {
                "Locked" | "locked" => Ok(DoorLocked::Locked),
                "Unlocked" | "unlocked" => Ok(DoorLocked::Unlocked),
                _ => Err(())
            }
        }))
    }
}

/// A secure/insecure state.
///
/// # JSON
///
/// This kind is represented by strings "Secure" | "Insecure".
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum IsSecure {
    /// # JSON
    ///
    /// Represented by "Insecure".
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = IsSecure::from_str("\"Insecure\"").unwrap();
    /// assert_eq!(parsed, IsSecure::Insecure);
    ///
    /// let serialized: JSON = IsSecure::Insecure.to_json();
    /// assert_eq!(serialized.as_string().unwrap(), "Insecure");
    /// ```
    Insecure,

    /// # JSON
    ///
    /// Represented by "Secure".
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = IsSecure::from_str("\"Secure\"").unwrap();
    /// assert_eq!(parsed, IsSecure::Secure);
    ///
    /// let serialized: JSON = IsSecure::Secure.to_json();
    /// assert_eq!(serialized.as_string().unwrap(), "Secure");
    /// ```
    Secure,
}

impl IsSecure {
    fn as_bool(&self) -> bool {
        match *self {
            IsSecure::Insecure => false,
            IsSecure::Secure => true,
        }
    }
}

impl Parser<IsSecure> for IsSecure {
    fn description() -> String {
        "IsSecure".to_string()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        match source.as_string() {
            Some("Insecure") => Ok(IsSecure::Insecure),
            Some("Secure") => Ok(IsSecure::Secure),
            Some(str) => Err(ParseError::unknown_constant(str, &path)),
            None => Err(ParseError::type_error("IsSecure", &path, "string"))
        }
    }
}

impl ToJSON for IsSecure {
    fn to_json(&self) -> JSON {
        match *self {
            IsSecure::Insecure => JSON::String("Insecure".to_owned()),
            IsSecure::Secure => JSON::String("Secure".to_owned())
        }
    }
}
impl Into<Value> for IsSecure {
    fn into(self) -> Value {
        Value::IsSecure(self)
    }
}

impl PartialOrd for IsSecure {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IsSecure {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bool().cmp(&other.as_bool())
    }
}

///
/// # (De)serialization
///
/// Values of this type are represented by strings "Insecure" | "Secure".
///
/// ```
/// extern crate serde;
/// extern crate serde_json;
/// extern crate foxbox_taxonomy;
///
/// let insecure = serde_json::to_string(&foxbox_taxonomy::values::IsSecure::Insecure).unwrap();
/// assert_eq!(insecure, "\"Insecure\"");
///
/// let insecure : foxbox_taxonomy::values::IsSecure = serde_json::from_str("\"Insecure\"").unwrap();
/// assert_eq!(insecure, foxbox_taxonomy::values::IsSecure::Insecure);
///
/// let secure = serde_json::to_string(&foxbox_taxonomy::values::IsSecure::Secure).unwrap();
/// assert_eq!(secure, "\"Secure\"");
///
/// let secure : foxbox_taxonomy::values::IsSecure = serde_json::from_str("\"Secure\"").unwrap();
/// assert_eq!(secure, foxbox_taxonomy::values::IsSecure::Secure);
/// ```
impl Serialize for IsSecure {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error> where S: Serializer {
        match *self {
            IsSecure::Insecure => "Insecure".serialize(serializer),
            IsSecure::Secure => "Secure".serialize(serializer)
        }
    }
}
impl Deserialize for IsSecure {
    fn deserialize<D>(deserializer: &mut D) -> Result<Self, D::Error> where D: Deserializer {
        deserializer.deserialize_string(TrivialEnumVisitor::new(|source| {
            match source {
                "Insecure" => Ok(IsSecure::Insecure),
                "Secure" => Ok(IsSecure::Secure),
                _ => Err(())
            }
        }))
    }
}

impl fmt::Display for IsSecure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", match *self { IsSecure::Insecure => "insecure", IsSecure::Secure => "secure" })
    }
}

/// A temperature. Internal representation may be either Fahrenheit or
/// Celcius. The `FoxBox` adapters are expected to perform conversions
/// to the format requested by their devices.
///
/// # JSON
///
/// Values of this type are represented by objects `{F; float}` or `{C: float}`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Temperature {
    /// Fahrenheit
    ///
    /// # JSON
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let source = "{
    ///   \"F\": 100
    /// }";
    /// let parsed = Temperature::from_str(source).unwrap();
    /// if let Temperature::F(100.) = parsed {
    ///    // As expected
    /// } else {
    ///    panic!()
    /// }
    ///
    /// let serialized : JSON = parsed.to_json();
    /// let val = serialized.find("F").unwrap().as_f64().unwrap();
    /// assert_eq!(val, 100.)
    /// ```
    F(f64),

    /// Celcius
    ///
    /// # JSON
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let source = "{
    ///   \"C\": 100
    /// }";
    /// let parsed = Temperature::from_str(source).unwrap();
    /// if let Temperature::C(100.) = parsed {
    ///    // As expected
    /// } else {
    ///    panic!()
    /// }
    ///
    /// let serialized : JSON = parsed.to_json();
    /// let val = serialized.find("C").unwrap().as_f64().unwrap();
    /// assert_eq!(val, 100.)
    /// ```
    C(f64),
}

impl Temperature {
    /// Get a temperature in Fahrenheit.
    pub fn as_f(&self) -> f64 {
        unimplemented!();
    }

    /// Get a temperature in Celcius.
    pub fn as_c(&self) -> f64 {
        unimplemented!();
    }
}

impl Parser<Temperature> for Temperature {
    fn description() -> String {
        "Temperature".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        if !source.is_object() {
            return Err(ParseError::type_error("Temperature", &path, "object"));
        }
        if let Some(result) = path.push("F", |path| f64::take_opt(path, source, "F")) {
            return result.map(Temperature::F);
        }
        if let Some(result) = path.push("C", |path| f64::take_opt(path, source, "C")) {
            return result.map(Temperature::C);
        }
        Err(ParseError::missing_field("C|F", &path))
    }
}
impl ToJSON for Temperature {
    fn to_json(&self) -> JSON {
        match *self {
            Temperature::C(val) => {
                JSON::Object(vec![("C".to_owned(), JSON::F64(val))].iter().cloned().collect())
            }
            Temperature::F(val) => {
                JSON::Object(vec![("F".to_owned(), JSON::F64(val))].iter().cloned().collect())
            }
        }
    }
}
impl PartialOrd for Temperature {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.as_c().partial_cmp(&other.as_c())
    }
}

/// A color. Internal representation may vary. The `FoxBox` adapters are
/// expected to perform conversions to the format requested by their
/// device.
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum Color {
    /// # JSON
    ///
    /// Values are represented as an object {h: float, s: float, v: float},
    /// where h is an arbitrary hue angle (interpreted mod 360), and s and
    /// v are between 0 and 1.
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// println!("Testing parsing");
    /// let source = "{
    ///   \"h\": 220.5,
    ///   \"s\": 0.8,
    ///   \"v\": 0.4
    /// }";
    ///
    /// let parsed = Color::from_str(source).unwrap();
    /// let Color::HSV(h, s, v) = parsed;
    /// assert_eq!(h, 220.5);
    /// assert_eq!(s, 0.8);
    /// assert_eq!(v, 0.4);
    ///
    /// println!("Testing serialization");
    /// let serialized : JSON = parsed.to_json();
    /// let h = serialized.find("h").unwrap().as_f64().unwrap();
    /// assert_eq!(h, 220.5);
    /// let s = serialized.find("s").unwrap().as_f64().unwrap();
    /// assert_eq!(s, 0.8);
    /// let v = serialized.find("v").unwrap().as_f64().unwrap();
    /// assert_eq!(v, 0.4);
    ///
    ///
    /// println!("Testing parsing error (saturation not in [0, 1])");
    /// // This source will not parse.
    /// let source_2 = "{
    ///   \"h\": -9.9,
    ///   \"s\": 1.1,
    ///   \"v\": 0.4
    /// }";
    ///
    /// match Color::from_str(source_2) {
    ///   Err(ParseError::TypeError{..}) => {},
    ///   other => panic!("Unexpected result {:?}", other)
    /// }
    ///
    ///
    /// println!("Testing parsing error (missing field)");
    /// // This source does not specify h, so it will not parse.
    /// let source_4 = "{
    ///   \"s\": 0.1,
    ///   \"v\": 0.2
    /// }";
    ///
    /// match Color::from_str(source_4) {
    ///   Err(ParseError::MissingField{ref name, ..}) if &name as &str == "h" => {},
    ///   other => panic!("Unexpected result {:?}", other)
    /// }
    /// ```
    HSV(f64, f64, f64)
}
impl Parser<Color> for Color {
    fn description() -> String {
        "Color".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        let h = try!(path.push("h", |path| f64::take(path, source, "h")));
        let s = try!(path.push("s", |path| f64::take(path, source, "s")));
        let v = try!(path.push("v", |path| f64::take(path, source, "v")));
        // h can be any hue angle, will be interpreted (mod 360) in [0, 360).
        for &(val, ref name) in &vec![(&s, "s"), (&v, "v")] {
            if *val < 0. || *val > 1. {
                return Err(ParseError::type_error(name, &path, "a number in [0, 1]"));
            }
        }
        Ok(Color::HSV(h, s, v))
    }
}

impl ToJSON for Color {
    fn to_json(&self) -> JSON {
        let Color::HSV(ref h, ref s, ref v) = *self;
        let mut vec = vec![("h", h), ("s", s), ("v", v)];
        let map = vec.drain(..)
            .map(|(name, value)| (name.to_owned(), JSON::F64(*value)))
            .collect();
        JSON::Object(map)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebPushNotify {
    pub resource: String,
    pub message: String,
}

impl Parser<WebPushNotify> for WebPushNotify {
    fn description() -> String {
        "WebPushNotify".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        let resource = try!(path.push("resource", |path| String::take(path, source, "resource")));
        let message = try!(path.push("message", |path| String::take(path, source, "message")));
        Ok(WebPushNotify { resource: resource, message: message})
    }
}

impl ToJSON for WebPushNotify {
    fn to_json(&self) -> JSON {
        vec![
            ("resource", &self.resource),
            ("message", &self.message),
        ].to_json()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkerbellRule {
    pub name: String,
    pub source: String,
}

impl Parser<ThinkerbellRule> for ThinkerbellRule {
    fn description() -> String {
        "ThinkerbellRuleSource".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        let name = try!(path.push("name", |path| String::take(path, source, "name")));
        let script_source = try!(path.push("source", |path| String::take(path, source, "source")));
        Ok(ThinkerbellRule { name: name, source: script_source })
    }
}
impl ToJSON for ThinkerbellRule {
    fn to_json(&self) -> JSON {
        vec![
            ("name", &self.name),
            ("source", &self.source),
        ].to_json()
    }
}

/// Representation of an object in JSON. It is often (albeit not
/// always) possible to choose a more precise data structure for
/// representing values send/accepted by a service. If possible,
/// adapters should rather pick such more precise data structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Json(pub serde_json::value::Value);

impl Parser<Json> for Json {
    fn description() -> String {
        "Json value".to_owned()
    }
    fn parse(_: Path, source: &mut JSON) -> Result<Self, ParseError> {
        Ok(Json(source.clone()))
    }
}
impl ToJSON for Json {
    fn to_json(&self) -> JSON {
        self.0.clone()
    }
}

impl PartialOrd for Json {
    /// Two Json objects are never comparable to each other.
    fn partial_cmp(&self, _: &Self) -> Option<Ordering> {
        None
    }
}

/// A data structure holding a boolean value of a type that has not
/// been standardized yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtValue<T> where T: Debug + Clone + PartialEq + PartialOrd + Serialize + Deserialize {
    pub value: T,

    /// The vendor. Used for namespacing purposes, to avoid
    /// confusing two incompatible extensions with similar
    /// names. For instance, "foxlink@mozilla.com".
    pub vendor: Id<VendorId>,

    /// Identification of the adapter introducing this value.
    /// Designed to aid with tracing and debugging.
    pub adapter: Id<AdapterId>,

    /// A string describing the nature of the value, designed to
    /// aid with type-checking.
    ///
    /// Examples: `"PresenceDetected"`.
    pub kind: Id<KindId>,
}

impl<T> Parser<ExtValue<T>> for ExtValue<T>
    where T: Debug + Clone + PartialEq + PartialOrd + Serialize + Deserialize + Parser<T>
{
    fn description() -> String {
        format!("ExtValue<{}>", T::description())
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        let vendor = try!(path.push("vendor", |path| Id::take(path, source, "vendor")));
        let adapter = try!(path.push("adapter", |path| Id::take(path, source, "adapter")));
        let kind = try!(path.push("kind", |path| Id::take(path, source, "kind")));
        let value = try!(path.push("value", |path| T::take(path, source, "value")));
        Ok(ExtValue {
            vendor: vendor,
            adapter: adapter,
            kind: kind,
            value: value
        })
    }
}

impl<T> ToJSON for ExtValue<T>
    where T: Debug + Clone + PartialEq + PartialOrd + Serialize + Deserialize + ToJSON
{
    fn to_json(&self) -> JSON {
        let mut source = vec![
            ("value", self.value.to_json()),
            ("vendor", JSON::String(self.vendor.to_string())),
            ("adapter", JSON::String(self.adapter.to_string())),
            ("kind", JSON::String(self.kind.to_string())),
        ];
        let map = source.drain(..)
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        JSON::Object(map)
    }
}

impl<T> PartialEq<ExtValue<T>> for ExtValue<T>
    where T: Debug + Clone + PartialEq + PartialOrd + Serialize + Deserialize
{
    fn eq(&self, other: &Self) -> bool {
        if self.vendor != other.vendor
        || self.kind != other.kind {
            false
        } else {
            self.value.eq(&other.value)
        }
    }
}

impl<T> PartialOrd<ExtValue<T>> for ExtValue<T>
    where T: Debug + Clone + PartialEq + PartialOrd + Serialize + Deserialize
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.vendor != other.vendor
        || self.kind != other.kind {
            None
        } else {
            self.value.partial_cmp(&other.value)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binary {
   /// The actual data. We put it behind an `Arc` to make sure
   /// that cloning remains inexpensive.
   pub data: Arc<Vec<u8>>,

   /// The mime type. Should probably be an Id<MimeTypeId>.
   pub mimetype: Id<MimeTypeId>,
}

impl Parser<Binary> for Binary {
    fn description() -> String {
        "Binary".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        let data = try!(path.push("data", |path| Vec::<u8>::take(path, source, "data")));
        let mimetype = try!(path.push("mimetype", |path| Id::take(path, source, "mimetype")));
        Ok(Binary {
            data: Arc::new(data),
            mimetype: mimetype
        })
    }
}

impl ToJSON for Binary {
    fn to_json(&self) -> JSON {
        let mut source = vec![
            ("data", JSON::Array(self.data.iter().map(|x| JSON::U64(*x as u64)).collect())),
            ("mimetype", JSON::String(self.mimetype.to_string()))
        ];
        let map = source.drain(..)
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        JSON::Object(map)
    }
}

/// Representation of an actual value that can be sent to/received
/// from a service.
///
/// # JSON
///
/// Values of this state are represented by an object `{ key: value }`, where key is one of
/// `Unit`, `OnOff`, `OpenClosed`, ... The `value` for `Unit` is ignored.
///
/// # Other forms of (de)serialization
///
/// Values of this state are represented by an object `{ key: value }`, where key is one of
/// `Unit`, `OnOff`, `OpenClosed`, ... The `value` for `Unit` is ignored.
///
/// ```
/// extern crate serde;
/// extern crate serde_json;
/// extern crate foxbox_taxonomy;
///
/// # fn main() {
/// use foxbox_taxonomy::values::Value::*;
/// use foxbox_taxonomy::values::OnOff::*;
/// use foxbox_taxonomy::values::OpenClosed::*;
///
/// let unit = serde_json::to_string(&Unit).unwrap();
/// assert_eq!(unit, "{\"Unit\":[]}");
///
/// let unit : foxbox_taxonomy::values::Value = serde_json::from_str("{\"Unit\":[]}").unwrap();
/// assert_eq!(unit, Unit);
///
/// let on = serde_json::to_string(&OnOff(On)).unwrap();
/// assert_eq!(on, "{\"OnOff\":\"On\"}");
///
/// let on : foxbox_taxonomy::values::Value = serde_json::from_str("{\"OnOff\":\"On\"}").unwrap();
/// assert_eq!(on, OnOff(On));
///
/// let open = serde_json::to_string(&OpenClosed(Open)).unwrap();
/// assert_eq!(open, "{\"OpenClosed\":\"Open\"}");
///
/// let open : foxbox_taxonomy::values::Value = serde_json::from_str("{\"OpenClosed\":\"Open\"}").unwrap();
/// assert_eq!(open, OpenClosed(Open));
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// An absolute time and date.
    ///
    /// # JSON
    ///
    /// Represented as `{"TimeStamp": string}`, where `string` is formatted as RFC 3339 such as
    /// `"2014-11-28T21:45:59.324310806+09:00"`.
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"Unit\": []
    /// }";
    ///
    /// let parsed = Value::from_str(source).unwrap();
    /// if let Value::Unit = parsed {
    ///   // ok
    /// } else {
    ///   panic!();
    /// }
    ///
    ///
    /// let serialized: JSON = parsed.to_json();
    /// if let JSON::Object(ref obj) = serialized {
    ///   let serialized = obj.get("Unit").unwrap();
    ///   assert!(serialized.is_null());
    /// }
    /// # }
    /// ```
    Unit,

    /// An on/off value.
    ///
    /// # JSON
    ///
    /// Represented as `{"OnOff": string}`, where `string` is "On" or "Off".
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"OnOff\": \"On\"
    /// }";
    ///
    /// let parsed = Value::from_str(source).unwrap();
    /// if let Value::OnOff(OnOff::On) = parsed {
    ///   // ok
    /// } else {
    ///   panic!();
    /// }
    ///
    ///
    /// let serialized: JSON = parsed.to_json();
    /// if let JSON::Object(ref obj) = serialized {
    ///   let serialized = obj.get("OnOff").unwrap();
    ///   assert_eq!(serialized.as_string().unwrap(), "On");
    /// }
    /// # }
    /// ```
    OnOff(OnOff),

    /// An open/closed value.
    ///
    /// # JSON
    ///
    /// Represented as `{"OpenClosed": string}`, where `string` is "Open" or "Closed".
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"OpenClosed\": \"Open\"
    /// }";
    ///
    /// let parsed = Value::from_str(source).unwrap();
    /// if let Value::OpenClosed(OpenClosed::Open) = parsed {
    ///   // ok
    /// } else {
    ///   panic!();
    /// }
    ///
    ///
    /// let serialized: JSON = parsed.to_json();
    /// if let JSON::Object(ref obj) = serialized {
    ///   let serialized = obj.get("OpenClosed").unwrap();
    ///   assert_eq!(serialized.as_string().unwrap(), "Open");
    /// }
    /// # }
    /// ```
    OpenClosed(OpenClosed),

    /// An locked/unlocked value.
    ///
    /// # JSON
    ///
    /// Represented as `{"DoorLocked": string}`, where `string` is "Locked" or "Unlocked".
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"DoorLocked\": \"Locked\"
    /// }";
    ///
    /// let parsed = Value::from_str(source).unwrap();
    /// if let Value::DoorLocked(DoorLocked::Locked) = parsed {
    ///   // ok
    /// } else {
    ///   panic!();
    /// }
    ///
    ///
    /// let serialized: JSON = parsed.to_json();
    /// if let JSON::Object(ref obj) = serialized {
    ///   let serialized = obj.get("DoorLocked").unwrap();
    ///   assert_eq!(serialized.as_string().unwrap(), "Locked");
    /// }
    /// # }
    /// ```
    DoorLocked(DoorLocked),

    /// A secure/insecure value.
    ///
    /// # JSON
    ///
    /// Represented as `{"IsSecure": string}` where `string` is "Secure" or "Insecure".
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"IsSecure\": \"Secure\"
    /// }";
    ///
    /// let parsed = Value::from_str(source).unwrap();
    /// if let Value::IsSecure(IsSecure::Secure) = parsed {
    ///   // ok
    /// } else {
    ///   panic!();
    /// }
    ///
    /// let serialized: JSON = parsed.to_json();
    /// if let JSON::Object(ref obj) = serialized {
    ///   let serialized = obj.get("IsSecure").unwrap();
    ///   assert_eq!(serialized.as_string().unwrap(), "Secure");
    /// }
    /// # }
    /// ```
    IsSecure(IsSecure),

    /// An absolute time and date.
    ///
    /// # JSON
    ///
    /// Represented as `{"TimeStamp": string}`, where `string` is formatted as RFC 3339 such as
    /// `"2014-11-28T21:45:59.324310806+09:00"`.
    ///
    /// ```
    /// extern crate chrono;
    /// extern crate foxbox_taxonomy;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    /// use chrono::Datelike;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"TimeStamp\": \"2014-11-28T21:45:59.324310806+09:00\"
    /// }";
    ///
    /// let parsed = Value::from_str(source).unwrap();
    /// if let Value::TimeStamp(ref ts) = parsed {
    ///   let date_time = ts.as_datetime();
    ///   assert_eq!(date_time.year(), 2014);
    ///   assert_eq!(date_time.month(), 11);
    ///   assert_eq!(date_time.day(), 28);
    /// } else {
    ///   panic!();
    /// }
    ///
    ///
    /// let serialized: JSON = parsed.to_json();
    /// if let JSON::Object(ref obj) = serialized {
    ///   let serialized = obj.get("TimeStamp").unwrap();
    ///   assert!(serialized.as_string().unwrap().starts_with("2014-11-28"));
    /// } else {
    ///   panic!();
    /// }
    /// # }
    /// ```
    TimeStamp(TimeStamp),

    /// A duration, also used to represent a time of day.
    ///
    /// # JSON
    ///
    /// Represented by `{Duration: float}`, where the number, is a (floating-point)
    /// number of seconds. If this value use used for time of day, the duration is
    /// since the start of the day, in local time.
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    /// extern crate chrono;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    /// use chrono::Duration as ChronoDuration;
    ///
    /// # fn main() {
    ///
    /// let parsed = Value::from_str("{\"Duration\": 60.01}").unwrap();
    /// if let Value::Duration(d) = parsed.clone() {
    ///   let duration : ChronoDuration = d.into();
    ///   assert_eq!(duration.num_seconds(), 60);
    ///   assert_eq!(duration.num_milliseconds(), 60010);
    /// } else {
    ///   panic!();
    /// }
    ///
    ///
    /// let serialized: JSON = parsed.to_json();
    /// if let JSON::Object(ref obj) = serialized {
    ///   let serialized = obj.get("Duration").unwrap();
    ///   assert!(serialized.as_f64().unwrap() >= 60. && serialized.as_f64().unwrap() < 61.);
    /// } else {
    ///   panic!();
    /// }
    /// # }
    /// ```
    Duration(Duration),

    /// A temperature.
    ///
    /// # JSON
    ///
    /// Represented by `{Temperature: {C: float}}` or `{Temperature: {F: float}}`.
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    /// extern crate chrono;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"Temperature\": {
    ///     \"C\": 2.0
    ///   }
    /// }";
    /// let parsed = Value::from_str(source).unwrap();
    /// if let Value::Temperature(Temperature::C(ref val)) = parsed {
    ///   assert_eq!(*val, 2.0);
    /// } else {
    ///   panic!();
    /// }
    ///
    ///
    /// let serialized: JSON = parsed.to_json();
    /// let val = serialized.find_path(&["Temperature", "C"]).unwrap().as_f64().unwrap();
    /// assert_eq!(val, 2.0);
    /// # }
    /// ```
    Temperature(Temperature),

    /// A color.
    ///
    /// # JSON
    ///
    /// Represented by `{Color: {h: float, s: float, v: float}}`,
    /// where s and v are in [0, 1] and h will be interpreted (mod 360) in [0, 360).
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"Color\": {
    ///     \"h\": 23.5,
    ///     \"s\": 0.2,
    ///     \"v\": 0.4
    ///   }
    /// }";
    /// let parsed = Value::from_str(source).unwrap();
    /// if let Value::Color(Color::HSV(23.5, 0.2, 0.4)) = parsed {
    ///   // Ok.
    /// } else {
    ///   panic!();
    /// }
    ///
    ///
    /// let serialized: JSON = parsed.to_json();
    /// let val = serialized.find_path(&["Color", "s"]).unwrap().as_f64().unwrap();
    /// assert_eq!(val, 0.2);
    /// # }
    /// ```
    Color(Color),

    /// A string.
    ///
    /// # JSON
    ///
    /// Represented by `{String: string}`.
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    /// extern crate chrono;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"String\": \"foobar\"
    /// }";
    /// let parsed = Value::from_str(source).unwrap();
    /// if let Value::String(ref str) = parsed {
    ///   assert_eq!(&*str as &str, "foobar");
    /// } else {
    ///   panic!();
    /// }
    ///
    ///
    /// let serialized: JSON = parsed.to_json();
    /// let val = serialized.find_path(&["String"]).unwrap().as_string().unwrap();
    /// assert_eq!(&val as &str, "foobar");
    /// # }
    /// ```
    String(Arc<String>),

    // FIXME: Add more as we identify needs

    ThinkerbellRule(ThinkerbellRule),
    WebPushNotify(WebPushNotify),

    /// A boolean value representing a unit that has not been
    /// standardized yet into the API.
    ExtBool(ExtValue<bool>),

    /// A numeric value representing a unit that has not been
    /// standardized yet into the API.
    ExtNumeric(ExtValue<f64>),

    /// A Json value. We put it behind an `Arc` to make sure that
    /// cloning remains inexpensive.
    ///
    /// # JSON
    ///
    /// Represented by `{Json: JSON}` where `JSON` is a JSON object.
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    /// extern crate chrono;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"Json\": { \"foo\": \"bar\" }
    /// }";
    /// let parsed = Value::from_str(source).unwrap();
    /// if let Value::Json(ref obj) = parsed {
    ///   assert_eq!(obj.0.find_path(&["foo"]).unwrap().as_string().unwrap(), "bar")
    /// } else {
    ///   panic!();
    /// }
    ///
    ///
    /// let serialized: JSON = parsed.to_json();
    /// let val = serialized.find_path(&["Json", "foo"]).unwrap().as_string().unwrap();
    /// assert_eq!(val, "bar");
    /// # }
    /// ```
    Json(Arc<Json>),

    /// Binary data.
    ///
    /// # JSON
    ///
    /// Represented by `{Binary: {data: array, mimetype: string}}`.
    ///
    /// **This representation is likely to change in the future.**
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    /// extern crate chrono;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"Binary\": { \"data\": [0, 1, 2], \"mimetype\": \"binary/raw\" }
    /// }";
    /// let parsed = Value::from_str(source).unwrap();
    /// if let Value::Binary(ref obj) = parsed {
    ///   assert_eq!(obj.mimetype.to_string(), "binary/raw".to_owned());
    ///   assert_eq!(*obj.data, vec![0, 1, 2]);
    /// } else {
    ///   panic!();
    /// }
    ///
    ///
    /// let serialized: JSON = parsed.to_json();
    /// let val = serialized.find_path(&["Binary", "mimetype"]).unwrap().as_string().unwrap();
    /// assert_eq!(val, "binary/raw");
    /// # }
    /// ```
    Binary(Binary),
    Range(Box<Range>)
}


lazy_static! {
    static ref VALUE_PARSER:
        HashMap<&'static str, Box<Fn(Path, &mut JSON) -> Result<Value, ParseError> + Sync>> =
    {
        use self::Value::*;
        use std::string::String as StdString;
        let mut map : HashMap<&'static str, Box<Fn(Path, &mut JSON) -> Result<Value, ParseError> + Sync>> = HashMap::new();
        map.insert("Unit", Box::new(|_, _| Ok(Unit)));
        map.insert("OnOff", Box::new(|path, v| {
            let value = try!(path.push("OnOff", |path| self::OnOff::parse(path, v)));
            Ok(OnOff(value))
        }));
        map.insert("OpenClosed", Box::new(|path, v| {
            let value = try!(path.push("OpenClosed", |path| self::OpenClosed::parse(path, v)));
            Ok(OpenClosed(value))
        }));
        map.insert("DoorLocked", Box::new(|path, v| {
            let value = try!(path.push("DoorLocked", |path| self::DoorLocked::parse(path, v)));
            Ok(DoorLocked(value))
        }));
        map.insert("IsSecure", Box::new(|path, v| {
            let value = try!(path.push("IsSecure", |path| self::IsSecure::parse(path, v)));
            Ok(IsSecure(value))
        }));
        map.insert("Duration", Box::new(|path, v| {
            let value = try!(path.push("Duration", |path| self::Duration::parse(path, v)));
            Ok(Duration(value))
        }));
        map.insert("TimeStamp", Box::new(|path, v| {
            let value = try!(path.push("TimeStamp", |path| self::TimeStamp::parse(path, v)));
            Ok(TimeStamp(value))
        }));
        map.insert("Temperature", Box::new(|path, v| {
            let value = try!(path.push("Temperature", |path| self::Temperature::parse(path, v)));
            Ok(Temperature(value))
        }));
        map.insert("ThinkerbellRule", Box::new(|path, v| {
            let value = try!(path.push("ThinkerbellRule", |path| self::ThinkerbellRule::parse(path, v)));
            Ok(ThinkerbellRule(value))
        }));
        map.insert("WebPushNotify", Box::new(|path, v| {
            let value = try!(path.push("WebPushNotify", |path| self::WebPushNotify::parse(path, v)));
            Ok(WebPushNotify(value))
        }));
        map.insert("Color", Box::new(|path, v| {
            let value = try!(path.push("Color", |path| self::Color::parse(path, v)));
            Ok(Color(value))
        }));
        map.insert("String", Box::new(|path, v| {
            let value = try!(path.push("String", |path| Arc::<StdString>::parse(path, v)));
            Ok(String(value))
        }));
        map.insert("Json", Box::new(|path, v| {
            let value = try!(path.push("Json", |path| Arc::<self::Json>::parse(path, v)));
            Ok(Json(value))
        }));
        map.insert("ExtBool", Box::new(|path, v| {
            let value = try!(path.push("ExtBool", |path| self::ExtValue::<bool>::parse(path, v)));
            Ok(ExtBool(value))
        }));
        map.insert("ExtNumeric", Box::new(|path, v| {
            let value = try!(path.push("ExtNumeric", |path| self::ExtValue::<f64>::parse(path, v)));
            Ok(ExtNumeric(value))
        }));
        map.insert("Binary", Box::new(|path, v| {
            let value = try!(path.push("Binary", |path| self::Binary::parse(path, v)));
            Ok(Binary(value))
        }));
        map.insert("Range", Box::new(|path, v| {
            let value = try!(path.push("Range", |path| self::Range::parse(path, v)));
            Ok(Range(Box::new(value)))
        }));
        map
    };
    static ref VALUE_KEYS: String = {
        let vec : Vec<_> = VALUE_PARSER.keys().cloned().collect();
        format!("{:?}", vec)
    };
}

impl Parser<Value> for Value {
    fn description() -> String {
        "Value".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        match *source {
            JSON::Null => Ok(Value::Unit),
            JSON::String(ref str) if &*str == "Unit" => Ok(Value::Unit),
            JSON::Object(ref mut obj) if obj.len() == 1 => {
                let mut vec : Vec<_> = obj.iter_mut().collect();
                let (k, v) = vec.pop().unwrap(); // We checked the length just above.
                match VALUE_PARSER.get(&k as &str) {
                    None => Err(ParseError::type_error("Value", &path, &&*self::VALUE_KEYS)),
                    Some(parser) => path.push(k, |path| parser(path, v))
                }
            }
            _ => Err(ParseError::type_error("Value", &path, "object with a single field"))
        }
    }
}

impl ToJSON for Value {
    fn to_json(&self) -> JSON {
        use self::Value::*;
        let (key, value) = match *self {
            Unit => ("Unit", JSON::Null),
            OnOff(ref val) => ("OnOff", val.to_json()),
            OpenClosed(ref val) => ("OpenClosed", val.to_json()),
            DoorLocked(ref val) => ("DoorLocked", val.to_json()),
            IsSecure(ref val) => ("IsSecure", val.to_json()),
            Duration(ref val) => ("Duration", val.to_json()),
            TimeStamp(ref val) => ("TimeStamp", val.to_json()),
            Color(ref val) => ("Color", val.to_json()),
            String(ref val) => ("String", val.to_json()),
            Json(ref val) => ("Json", val.to_json()),
            Binary(ref val) => ("Binary", val.to_json()),
            Temperature(ref val) => ("Temperature", val.to_json()),
            ThinkerbellRule(ref val) => ("ThinkerbellRule", val.to_json()),
            WebPushNotify(ref val) => ("WebPushNotify", val.to_json()),
            ExtBool(ref val) => ("ExtBool", val.to_json()),
            ExtNumeric(ref val) => ("ExtNumeric", val.to_json()),
            Range(ref val) => ("Range", val.to_json()),
        };
        let source = vec![(key.to_owned(), value)];
        JSON::Object(source.iter().cloned().collect())
    }
}


impl Value {
    pub fn get_type(&self) -> Type {
        match *self {
            Value::Unit => Type::Unit,
            Value::OnOff(_) => Type::OnOff,
            Value::OpenClosed(_) => Type::OpenClosed,
            Value::DoorLocked(_) => Type::DoorLocked,
            Value::IsSecure(_) => Type::IsSecure,
            Value::String(_) => Type::String,
            Value::Duration(_) => Type::Duration,
            Value::TimeStamp(_) => Type::TimeStamp,
            Value::Temperature(_) => Type::Temperature,
            Value::Color(_) => Type::Color,
            Value::Json(_) => Type::Json,
            Value::Binary(_) => Type::Binary,
            Value::ExtBool(_) => Type::ExtBool,
            Value::ExtNumeric(_) => Type::ExtNumeric,
            Value::ThinkerbellRule(_) => Type::ThinkerbellRule,
            Value::WebPushNotify(_) => Type::WebPushNotify,
            Value::Range(_) => Type::Range,
        }
    }

    pub fn as_timestamp(&self) -> Result<&TimeStamp, TypeError> {
        match *self {
            Value::TimeStamp(ref x) => Ok(x),
            _ => Err(TypeError {expected: Type::TimeStamp, got: self.get_type()})
        }
    }

    pub fn as_duration(&self) -> Result<&Duration, TypeError> {
        match *self {
            Value::Duration(ref x) => Ok(x),
            _ => Err(TypeError {expected: Type::Duration, got: self.get_type()})
        }
    }
}

impl PartialOrd for Value {
    /// Two values of the same type can be compared using the usual
    /// comparison for values of this type. Two values of distinct
    /// types cannot be compared.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use self::Value::*;
        use std::cmp::Ordering::*;
        match (self, other) {
            (&Unit, &Unit) => Some(Equal),
            (&Unit, _) => None,

            (&OnOff(ref a), &OnOff(ref b)) => a.partial_cmp(b),
            (&OnOff(_), _) => None,

            (&OpenClosed(ref a), &OpenClosed(ref b)) => a.partial_cmp(b),
            (&OpenClosed(_), _) => None,

            (&DoorLocked(ref a), &DoorLocked(ref b)) => a.partial_cmp(b),
            (&DoorLocked(_), _) => None,

            (&IsSecure(ref a), &IsSecure(ref b)) => a.partial_cmp(b),
            (&IsSecure(_), _) => None,

            (&Duration(ref a), &Duration(ref b)) => a.partial_cmp(b),
            (&Duration(_), _) => None,

            (&TimeStamp(ref a), &TimeStamp(ref b)) => a.partial_cmp(b),
            (&TimeStamp(_), _) => None,

            (&Temperature(ref a), &Temperature(ref b)) => a.partial_cmp(b),
            (&Temperature(_), _) => None,

            (&Color(ref a), &Color(ref b)) => a.partial_cmp(b),
            (&Color(_), _) => None,

            (&ExtBool(ref a), &ExtBool(ref b)) => a.partial_cmp(b),
            (&ExtBool(_), _) => None,

            (&ExtNumeric(ref a), &ExtNumeric(ref b)) => a.partial_cmp(b),
            (&ExtNumeric(_), _) => None,

            (&String(ref a), &String(ref b)) => a.partial_cmp(b),
            (&String(_), _) => None,

            (&Json(ref a), &Json(ref b)) => a.partial_cmp(b),
            (&Json(_), _) => None,

            (&ThinkerbellRule(ref a), &ThinkerbellRule(ref b)) => a.name.partial_cmp(&b.name),
            (&ThinkerbellRule(_), _) => None,

            (&WebPushNotify(ref a), &WebPushNotify(ref b)) => a.resource.partial_cmp(&b.resource),
            (&WebPushNotify(_), _) => None,

            (&Binary(self::Binary {mimetype: ref a_mimetype, data: ref a_data}),
             &Binary(self::Binary {mimetype: ref b_mimetype, data: ref b_data})) if a_mimetype == b_mimetype => a_data.partial_cmp(b_data),
            (&Binary(_), _) => None,

            (&Range(_), &Range(_)) => None,
            (&Range(_), _) => None,
        }
    }
}

/// An absolute time and date.
///
/// # JSON
///
/// Represented by a string. This data structure accepts string formatted as RFC 3339 such as
/// `"2014-11-28T21:45:59.324310806+09:00"`.
///
/// ```
/// extern crate chrono;
/// extern crate foxbox_taxonomy;
///
/// use foxbox_taxonomy::values::*;
/// use foxbox_taxonomy::parse::*;
/// use chrono::Datelike;
///
/// # fn main() {
///
/// let parsed = TimeStamp::from_str("\"2014-11-28T21:45:59.324310806+09:00\"").unwrap();
/// let date_time = parsed.as_datetime().clone();
/// assert_eq!(date_time.year(), 2014);
/// assert_eq!(date_time.month(), 11);
/// assert_eq!(date_time.day(), 28);
///
///
/// let serialized: JSON = parsed.to_json();
/// assert!(serialized.as_string().unwrap().starts_with("2014-11-28"));
///
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub struct TimeStamp(DateTime<UTC>);
impl TimeStamp {
    pub fn from_datetime(datetime: DateTime<UTC>) -> Self {
        TimeStamp(datetime)
    }
    pub fn as_datetime(&self) -> &DateTime<UTC> {
        &self.0
    }
    pub fn from_s(s: i64) -> Self {
        use chrono;
        let naive = chrono::naive::datetime::NaiveDateTime::from_timestamp(s, 0);
        let date = DateTime::<UTC>::from_utc(naive, UTC);
        TimeStamp(date)
    }
}
impl Parser<TimeStamp> for TimeStamp {
    fn description() -> String {
        "TimeStamp".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        if let JSON::String(ref str) = *source {
            if let Ok(dt) = DateTime::<UTC>::from_str(str) {
                return Ok(TimeStamp(dt));
            }
        }
        Err(ParseError::type_error("TimeStamp", &path, "date string"))
    }
}
impl ToJSON for TimeStamp {
    fn to_json(&self) -> JSON {
        JSON::String(self.0.to_rfc3339())
    }
}
impl Into<DateTime<UTC>> for TimeStamp  {
    fn into(self) -> DateTime<UTC> {
        self.0
    }
}
impl Into<DateTime<Local>> for TimeStamp  {
    fn into(self) -> DateTime<Local> {
        self.0.with_timezone(&Local)
    }
}
impl<T> From<DateTime<T>> for TimeStamp where T: TimeZone {
    fn from(date: DateTime<T>) -> Self {
        TimeStamp(date.with_timezone(&UTC))
    }
}

impl Serialize for TimeStamp {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
        where S: Serializer {
        let str = self.0.to_rfc3339();
        str.serialize(serializer)
    }
}
impl Deserialize for TimeStamp {
    fn deserialize<D>(deserializer: &mut D) -> Result<Self, D::Error>
        where D: Deserializer {
        let str = try!(String::deserialize(deserializer));
        match DateTime::<UTC>::from_str(&str) {
            Ok(dt) => Ok(TimeStamp(dt)),
            Err(_) => Err(D::Error::custom("Invalid date"))
        }
    }
}

/// A comparison between two values.
///
/// # JSON
///
/// A range is an object with one field `{key: value}`.
///
#[derive(Clone, Deserialize, Serialize, Debug, PartialEq)]
pub enum Range {
    /// Leq(x) accepts any value v such that v <= x.
    ///
    /// # JSON
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    /// extern crate serde_json;
    ///
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"Leq\": { \"OnOff\": \"On\" }
    /// }";
    ///
    /// let parsed = Range::from_str(source).unwrap();
    /// if let Range::Leq(ref leq) = parsed {
    ///   assert_eq!(*leq, Value::OnOff(OnOff::On));
    /// } else {
    ///   panic!();
    /// }
    ///
    /// let as_json = parsed.to_json();
    /// let as_string = serde_json::to_string(&as_json).unwrap();
    /// assert_eq!(as_string, "{\"Leq\":{\"OnOff\":\"On\"}}");
    ///
    /// # }
    /// ```
    Leq(Value),

    /// Geq(x) accepts any value v such that v >= x.
    Geq(Value),

    /// BetweenEq {min, max} accepts any value v such that `min <= v`
    /// and `v <= max`. If `max < min`, it never accepts anything.
    BetweenEq { min:Value, max:Value },

    /// OutOfStrict {min, max} accepts any value v such that `v < min`
    /// or `max < v`
    OutOfStrict { min:Value, max:Value },

    /// Eq(x) accespts any value v such that v == x
    Eq(Value),
}

impl Parser<Range> for Range {
    fn description() -> String {
        "Range".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        use self::Range::*;
        match *source {
            JSON::Object(ref mut obj) if obj.len() == 1 => {
                if let Some(leq) = obj.get_mut("Leq") {
                    return Ok(Leq(try!(path.push("Leq", |path| Value::parse(path, leq)))))
                }
                if let Some(geq) = obj.get_mut("Geq") {
                    return Ok(Geq(try!(path.push("Geq", |path| Value::parse(path, geq)))))
                }
                if let Some(eq) = obj.get_mut("Eq") {
                    return Ok(Eq(try!(path.push("eq", |path| Value::parse(path, eq)))))
                }
                if let Some(between) = obj.get_mut("BetweenEq") {
                    let mut bounds = try!(path.push("BetweenEq", |path| Vec::<Value>::parse(path, between)));
                    if bounds.len() == 2 {
                        let max = bounds.pop().unwrap();
                        let min = bounds.pop().unwrap();
                        return Ok(BetweenEq {
                            min: min,
                            max: max
                        })
                    } else {
                        return Err(ParseError::type_error("BetweenEq", &path, "an array of two values"))
                    }
                }
                if let Some(outof) = obj.get_mut("OutOfStrict") {
                    let mut bounds = try!(path.push("OutOfStrict", |path| Vec::<Value>::parse(path, outof)));
                    if bounds.len() == 2 {
                        let max = bounds.pop().unwrap();
                        let min = bounds.pop().unwrap();
                        return Ok(OutOfStrict {
                            min: min,
                            max: max
                        })
                    } else {
                        return Err(ParseError::type_error("OutOfStrict", &path, "an array of two values"))
                    }
                }
                Err(ParseError::type_error("Range", &path, "a field Eq, Leq, Geq, BetweenEq or OutOfStrict"))
            }
            _ => Err(ParseError::type_error("Range", &path, "object"))
        }
    }
}

impl ToJSON for Range {
    fn to_json(&self) -> JSON {
        let (key, value) = match *self {
            Range::Eq(ref val) => ("Eq", val.to_json()),
            Range::Geq(ref val) => ("Geq", val.to_json()),
            Range::Leq(ref val) => ("Leq", val.to_json()),
            Range::BetweenEq { ref min, ref max } => ("BetweenEq", JSON::Array(vec![min.to_json(), max.to_json()])),
            Range::OutOfStrict { ref min, ref max } => ("OutOfStrict", JSON::Array(vec![min.to_json(), max.to_json()])),
        };
        vec![(key, value)].to_json()
    }
}

impl Range {
    /// Determine if a value is accepted by this range.
    pub fn contains(&self, value: &Value) -> bool {
        use self::Range::*;
        match *self {
            Leq(ref max) => value <= max,
            Geq(ref min) => value >= min,
            BetweenEq { ref min, ref max } => min <= value && value <= max,
            OutOfStrict { ref min, ref max } => value < min || max < value,
            Eq(ref val) => value == val,
        }
    }

    /// Get the type associated to this range.
    ///
    /// If this range has a `min` and a `max` with conflicting types,
    /// produce an error.
    pub fn get_type(&self) -> Result<Type, TypeError> {
        use self::Range::*;
        match *self {
            Leq(ref v) | Geq(ref v) | Eq(ref v) => Ok(v.get_type()),
            BetweenEq {ref min, ref max} | OutOfStrict {ref min, ref max} => {
                let min_typ = min.get_type();
                let max_typ = max.get_type();
                if min_typ == max_typ {
                    Ok(min_typ)
                } else {
                    Err(TypeError {
                        expected: min_typ,
                        got: max_typ
                    })
                }
            }
        }
    }
}


/// A duration, also used to represent a time of day.
///
/// # JSON
///
/// Represented by a (floating-point) number of seconds.
///
/// ```
/// extern crate foxbox_taxonomy;
/// extern crate chrono;
///
/// use foxbox_taxonomy::values::*;
/// use foxbox_taxonomy::parse::*;
/// use chrono::Duration as ChronoDuration;
///
/// # fn main() {
///
/// let parsed = Duration::from_str("60.01").unwrap();
/// let duration : ChronoDuration = parsed.clone().into();
/// assert_eq!(duration.num_seconds(), 60);
/// assert_eq!(duration.num_milliseconds(), 60010);
///
///
/// let serialized: JSON = parsed.to_json();
/// assert_eq!(serialized.as_f64().unwrap(), 60.01);
///
/// # }
/// ```
#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq)]
pub struct Duration(ChronoDuration);

impl Parser<Duration> for Duration {
    fn description() -> String {
        "Duration".to_owned()
    }
    fn parse(path: Path, source: &mut JSON) -> Result<Self, ParseError> {
        let val = try!(f64::parse(path, source));
        Ok(Duration(ChronoDuration::milliseconds((val * 1000.) as i64)))
    }
}

impl ToJSON for Duration {
    fn to_json(&self) -> JSON {
        let val = self.0.num_milliseconds() as f64 / 1000 as f64;
        JSON::F64(val)
    }
}

impl Into<Value> for Duration {
    fn into(self) -> Value {
        Value::Duration(self)
    }
}

///
/// # Serialization
///
/// Values are deserialized to a floating-point number of seconds.
///
/// ```
/// extern crate serde;
/// extern crate serde_json;
/// extern crate foxbox_taxonomy;
/// extern crate chrono;
///
/// # fn main() {
/// use foxbox_taxonomy::values::*;
///
/// let duration = Duration::from(chrono::Duration::milliseconds(3141));
///
/// let duration_as_json = serde_json::to_string(&duration).unwrap();
/// assert_eq!(duration_as_json, "3.141");
///
/// let duration_back : Duration = serde_json::from_str(&duration_as_json).unwrap();
/// assert_eq!(duration, duration_back);
/// # }
/// ```
impl Serialize for Duration {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
        where S: Serializer
     {
         serializer.serialize_f64(self.0.num_milliseconds() as f64 / 1000 as f64)
     }
}
impl From<ChronoDuration> for Duration {
    fn from(source: ChronoDuration) -> Self {
        Duration(source)
    }
}
impl Into<ChronoDuration> for Duration {
    fn into(self) -> ChronoDuration {
        self.0
    }
}

impl Deserialize for Duration {
    /// Deserialize this value given this `Deserializer`.
    fn deserialize<D>(deserializer: &mut D) -> Result<Self, D::Error>
        where D: Deserializer
    {
        struct DurationVisitor;
        impl Visitor for DurationVisitor
        {
            type Value = Duration;
            fn visit_f64<E>(&mut self, v: f64) -> Result<Self::Value, E>
                where E: Error,
            {
                Ok(Duration(ChronoDuration::milliseconds((v * 1000.) as i64)))
            }
            fn visit_i64<E>(&mut self, v: i64) -> Result<Self::Value, E>
                where E: Error,
            {
                Ok(Duration(ChronoDuration::milliseconds(v * 1000)))
            }
            fn visit_u64<E>(&mut self, v: u64) -> Result<Self::Value, E>
                where E: Error,
            {
                self.visit_i64(v as i64)
            }
        }
        deserializer.deserialize_f64(DurationVisitor)
            .or_else(|_| deserializer.deserialize_i64(DurationVisitor))
    }
}
