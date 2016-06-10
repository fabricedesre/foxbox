//!
//! Values manipulated by services
//!

#![allow(identity_op)] // Keep clippy happy with [De]serialize
#![allow(transmute_ptr_to_ref)] // Keep clippy happy with mopaify

use api::Error;
use io;
use io::{ BinaryTarget, BinarySource };
use parse::*;
use util::*;

use std::cmp::{ PartialOrd, Ordering };
use std::fmt::Debug;
use std::sync::Arc;
use std::{ error, fmt };

use chrono::{ Duration as ChronoDuration, DateTime, Local, TimeZone, UTC };
use mopa;
use serde_json;

/// Representation of a type error.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TypeError {
    /// The type we expected.
    pub expected: String,

    /// The type we actually got.
    pub got: String,
}

impl TypeError {
    pub fn new(expected: &Arc<io::Format>, got: &Value) -> Self {
        TypeError {
            expected: expected.description(),
            got: got.description()
        }
    }
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

/// Representation of an actual value that can be sent to/received
/// from a service.
///
/// Values are designed to be cloned, rather than `Rc`/`Arc`-ed.
#[derive(Debug, Clone)]
pub struct Value {
    content: Arc<ValueImpl>,
}

struct ValueImpl {
    /// The data held by the value.
    data: Box<Data>,

    /// A closure for `T::description()`. We cannot store this directly in a Box<Data>
    /// because `description()` has no receiver, hence cannot be turned into a virtual
    /// method by Rust's trait system.
    describe: Box<Fn() -> String + Send + Sync>,

    /// A closure for `T::eq()` (from `PartialEq`). We cannot store this directly in a
    /// Box<Data> because `eq()` uses the `Self` type, hence cannot be turned into a virtual
    /// method by Rust's trait system.
    eq: Box<Fn(&Data, &Data) -> bool + Send + Sync>,
}
impl ValueImpl {
    fn new<T>(data: T) -> Self where T: Data + Debug + PartialEq + Sized {
        let describe = || T::description();
        let eq = |me: &Data, other: &Data| {
            let me = me.downcast_ref::<T>().unwrap(); // By definition, `me` has type `T`.
            match other.downcast_ref::<T>() {
                None => false,
                Some(other) => me.eq(other)
            }
        };
        ValueImpl {
            data: Box::new(data),
            describe: Box::new(describe),
            eq: Box::new(eq),
        }
    }
}
impl fmt::Debug for ValueImpl {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.data.fmt(f)
    }
}



impl Value {
    pub fn new<T>(data: T) -> Self where T: Data + Debug + PartialEq + Sized {
        Value {
            content: Arc::new(ValueImpl::new(data)),
        }
    }

    pub fn cast<T>(&self) -> Result<&T, Error> where T: Data + Sized {
        match self.content.data.downcast_ref::<T>() {
            None => Err(Error::TypeError(TypeError {
                expected: T::description(),
                got: self.description()
            })),
            Some(r) => Ok(r)
        }
    }

    pub fn downcast<T>(&self) -> Option<&T> where T: Data {
        self.content.data.downcast_ref::<T>()
    }

    pub fn description(&self) -> String {
        (self.content.describe)()
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        (self.content.eq)(&*self.content.data, &*other.content.data)
    }
}

pub trait Data: Debug + Send + Sync + mopa::Any {
    /// A human-readable description of the _type_ of the value.
    ///
    /// Used mainly in `TypeError` error messages.
    fn description() -> String where Self: Sized;

    /// Attempt to build a `Value` from a json `source` and `binary` components.
    fn parse(path: Path, source: &JSON, binary: &BinarySource) -> Result<Self, Error> where Self: Sized;

    /// Serialize a `Value` into a `JSON`, storing binary data in `binary`.
    fn serialize(source: &Self, binary: &BinaryTarget) -> Result<JSON, Error> where Self: Sized;

    /// Shorthand for parsing from a string.
    ///
    /// Used mainly for testing purposes.
    fn parse_str(source: &str) -> Result<Self, Error> where Self: Sized {
        serde_json::from_str(source)
            .map_err(|err| Error::ParseError(ParseError::JSON(JSONError(err))))
            .and_then(|json| Self::parse(Path::new(), &json, &BinarySource))
    }

    fn parse_vec(path: Path, source: &JSON, binary: &BinarySource) -> Result<Vec<Self>, Error> where Self: Sized {
        match source.as_array() {
            None => Err(Error::TypeError(TypeError {
                expected: "array".to_owned(),
                got: "something else".to_owned()
            })),
            Some(array) => {
                let mut result = Vec::with_capacity(array.len());
                for (item, i) in array.iter().zip(0..) {
                    let got = try!(path.push_index(i, |path| Self::parse(path, item, binary)));
                    result.push(got);
                }
                Ok(result)
            }
        }
    }

    fn parse_field(path: Path, source: &JSON, binary: &BinarySource, field_name: &str) -> Result<Self, Error> where Self: Sized {
        match Self::parse_opt_field(path.clone(), source, binary, field_name) { // FIXME: Get rid of this `path.clone()`
            Some(result) => result,
            None => Err(Error::ParseError(ParseError::missing_field(field_name, &path)))
        }
    }

    fn parse_opt_field(path: Path, source: &JSON, binary: &BinarySource, field_name: &str) -> Option<Result<Self, Error>> where Self: Sized {
        if let JSON::Object(ref obj) = *source {
            if let Some(v) = obj.get(field_name) {
                Some(Self::parse(path, v, binary))
            } else {
                None
            }
        } else {
            Some(Err(Error::ParseError(ParseError::type_error(field_name, &path, "object"))))
        }
    }
}
mopafy!(Data);

impl<T> Parser<T> for T where T: Data {
    fn description() -> String {
        T::description()
    }
    fn parse(path: Path, source: &JSON) -> Result<Self, ParseError> {
        match T::parse(path, source, &BinarySource) {
            Ok(ok) => Ok(ok),
            Err(Error::ParseError(err)) => Err(err),
            Err(err) => Err(ParseError::InternalError(format!("{}", err)))
        }
    }
}

impl Data for String {
    fn description() -> String {
        "String".to_owned()
    }
    fn parse(path: Path, source: &JSON, _binary: &BinarySource) -> Result<String, Error> {
        match source.as_string() {
            None => Err(Error::ParseError(ParseError::type_error("String", &path, "string"))),
            Some(s) => Ok(s.to_owned())
        }
    }

    fn serialize(source: &String, _binary: &BinaryTarget) -> Result<JSON, Error> {
        Ok(JSON::String(source.clone()))
    }
}

impl Data for () {
    fn description() -> String {
        "Nothing".to_owned()
    }
    /// Attempt to build a `Value` from a json `source` and `binary` components.
    fn parse(_: Path, _: &JSON, _: &BinarySource) -> Result<Self, Error> {
        Ok(())
    }

    /// Serialize a `Value` into a `JSON`, storing binary data in `binary`.
    fn serialize(_: &Self, _: &BinaryTarget) -> Result<JSON, Error> {
        Ok(JSON::Null)
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
    /// use foxbox_taxonomy::api::Error;
    /// use foxbox_taxonomy::io::*;
    /// use foxbox_taxonomy::parse::*;
    /// use foxbox_taxonomy::values::*;
    ///
    /// let parsed = OnOff::parse_str("\"On\"").unwrap();
    /// assert_eq!(parsed, OnOff::On);
    ///
    /// let serialized: JSON = OnOff::serialize(&OnOff::On, &BinaryTarget).unwrap();
    /// assert_eq!(serialized.as_string().unwrap(), "On");
    /// ```
    On,

    /// # JSON
    ///
    /// Represented by "Off".
    ///
    /// ```
    /// use foxbox_taxonomy::api::Error;
    /// use foxbox_taxonomy::io::*;
    /// use foxbox_taxonomy::parse::*;
    /// use foxbox_taxonomy::values::*;
    ///
    /// let parsed = OnOff::parse_str("\"Off\"").unwrap();
    /// assert_eq!(parsed, OnOff::Off);
    ///
    /// let serialized: JSON = OnOff::serialize(&OnOff::Off, &BinaryTarget).unwrap();
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

impl Data for OnOff {
    fn description() -> String {
        "On/Off".to_owned()
    }
    fn parse(path: Path, source: &JSON, _binary: &BinarySource) -> Result<Self, Error> {
        let result = match source.as_string() {
            Some("On") => OnOff::On,
            Some("Off") => OnOff::Off,
            Some(str) => return Err(Error::ParseError(ParseError::unknown_constant(str, &path))),
            None => return Err(Error::ParseError(ParseError::type_error("OnOff", &path, "string")))
        };
        Ok(result)
    }

    fn serialize(source: &Self, _binary: &BinaryTarget) -> Result<JSON, Error> {
        let str = match *source {
            OnOff::On => "On",
            OnOff::Off => "Off",
        };
        Ok(JSON::String(str.to_owned()))
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
    /// use foxbox_taxonomy::api::Error;
    /// use foxbox_taxonomy::io::*;
    /// use foxbox_taxonomy::parse::*;
    /// use foxbox_taxonomy::values::*;
    ///
    /// let parsed = OpenClosed::parse_str("\"Open\"").unwrap();
    /// assert_eq!(parsed, OpenClosed::Open);
    ///
    /// let serialized: JSON = OpenClosed::serialize(&OpenClosed::Open, &BinaryTarget).unwrap();
    /// assert_eq!(serialized.as_string().unwrap(), "Open");
    /// ```
    Open,

    /// # JSON
    ///
    /// Represented by "Closed".
    ///
    /// ```
    /// use foxbox_taxonomy::api::Error;
    /// use foxbox_taxonomy::io::*;
    /// use foxbox_taxonomy::parse::*;
    /// use foxbox_taxonomy::values::*;
    ///
    /// let parsed = OpenClosed::parse_str("\"Closed\"").unwrap();
    /// assert_eq!(parsed, OpenClosed::Closed);
    ///
    /// let serialized: JSON = OpenClosed::serialize(&OpenClosed::Closed, &BinaryTarget).unwrap();
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

impl Data for OpenClosed {
    fn description() -> String {
        "Open/Closed".to_owned()
    }
    fn parse(path: Path, source: &JSON, _binary: &BinarySource) -> Result<Self, Error> {
        let result = match source.as_string() {
            Some("Open") => OpenClosed::Open,
            Some("Closed") => OpenClosed::Closed,
            Some(str) => return Err(Error::ParseError(ParseError::unknown_constant(str, &path))),
            None => return Err(Error::ParseError(ParseError::type_error("OpenClosed", &path, "string")))
        };
        Ok(result)
    }
    fn serialize(source: &Self, _binary: &BinaryTarget) -> Result<JSON, Error> {
        let str = match *source {
            OpenClosed::Open => "Open",
            OpenClosed::Closed => "Closed",
        };
        Ok(JSON::String(str.to_owned()))
    }
}

/// An locked/unlocked state.
///
/// # JSON
///
/// Values of this type are represented by strings "Locked" | "Unlocked".
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum IsLocked {
    /// # JSON
    ///
    /// Represented by "Locked".
    ///
    /// ```
    /// use foxbox_taxonomy::values::*;
    /// use foxbox_taxonomy::parse::*;
    ///
    /// let parsed = IsLocked::from_str("\"Locked\"").unwrap();
    /// assert_eq!(parsed, IsLocked::Locked);
    ///
    /// let serialized: JSON = IsLocked::Locked.to_json();
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
    /// let parsed = IsLocked::from_str("\"Unlocked\"").unwrap();
    /// assert_eq!(parsed, IsLocked::Unlocked);
    ///
    /// let serialized: JSON = IsLocked::Unlocked.to_json();
    /// assert_eq!(serialized.as_string().unwrap(), "Unlocked");
    /// ```
    Unlocked,
}

impl IsLocked {
    fn as_bool(&self) -> bool {
        match *self {
            IsLocked::Locked => true,
            IsLocked::Unlocked => false,
        }
    }
}

impl Data for IsLocked {
    fn description() -> String {
        "IsLocked".to_owned()
    }
    fn parse(path: Path, source: &JSON, _binary: &BinarySource) -> Result<Self, Error> {
        match source.as_string() {
            Some("Locked") => Ok(IsLocked::Locked),
            Some("Unlocked") => Ok(IsLocked::Unlocked),
            Some(str) => Err(Error::ParseError(ParseError::unknown_constant(str, &path))),
            None => Err(Error::ParseError(ParseError::type_error("IsLocked", &path, "string")))
        }
    }
    fn serialize(source: &Self, _binary: &BinaryTarget) -> Result<JSON, Error> {
        let str = match *source {
            IsLocked::Locked => "Locked",
            IsLocked::Unlocked => "Unlocked"
        };
        Ok(JSON::String(str.to_owned()))
    }
}

impl ToJSON for IsLocked {
    fn to_json(&self) -> JSON {
        match *self {
            IsLocked::Locked => JSON::String("Locked".to_owned()),
            IsLocked::Unlocked => JSON::String("Unlocked".to_owned())
        }
    }
}

impl PartialOrd for IsLocked {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IsLocked {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bool().cmp(&other.as_bool())
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
    /// use foxbox_taxonomy::api::Error;
    /// use foxbox_taxonomy::io::*;
    /// use foxbox_taxonomy::parse::*;
    /// use foxbox_taxonomy::values::*;
    ///
    /// let parsed = IsSecure::parse_str("\"Insecure\"").unwrap();
    /// assert_eq!(parsed, IsSecure::Insecure);
    ///
    /// let serialized: JSON = IsSecure::serialize(&IsSecure::Insecure, &BinaryTarget).unwrap();
    /// assert_eq!(serialized.as_string().unwrap(), "Insecure");
    /// ```
    Insecure,

    /// # JSON
    ///
    /// Represented by "Secure".
    ///
    /// ```
    /// use foxbox_taxonomy::api::Error;
    /// use foxbox_taxonomy::io::*;
    /// use foxbox_taxonomy::parse::*;
    /// use foxbox_taxonomy::values::*;
    ///
    /// let parsed = IsSecure::parse_str("\"Secure\"").unwrap();
    /// assert_eq!(parsed, IsSecure::Secure);
    ///
    /// let serialized: JSON = IsSecure::serialize(&IsSecure::Secure, &BinaryTarget).unwrap();
    /// assert_eq!(serialized.as_string().unwrap(), "Secure");
    /// ```
    Secure,
}

impl Data for IsSecure {
    fn description() -> String {
        "Secure/Insecure".to_owned()
    }
    fn parse(path: Path, source: &JSON, _binary: &BinarySource) -> Result<Self, Error> {
        let result = match source.as_string() {
            Some("Secure") => IsSecure::Secure,
            Some("Insecure") => IsSecure::Insecure,
            Some(str) => return Err(Error::ParseError(ParseError::unknown_constant(str, &path))),
            None => return Err(Error::ParseError(ParseError::type_error("IsSecure", &path, "string")))
        };
        Ok(result)
    }
    fn serialize(source: &Self, _binary: &BinaryTarget) -> Result<JSON, Error> {
        let str = match *source {
            IsSecure::Secure => "Secure",
            IsSecure::Insecure => "Insecure",
        };
        Ok(JSON::String(str.to_owned()))
    }
}

impl IsSecure {
    fn as_bool(&self) -> bool {
        match *self {
            IsSecure::Insecure => false,
            IsSecure::Secure => true,
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
#[derive(Debug, Clone, PartialEq)]
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
    fn parse(path: Path, source: &JSON) -> Result<Self, ParseError> {
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
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Color {
    /// # JSON
    ///
    /// Values are represented as an object {h: float, s: float, v: float},
    /// where h is an arbitrary hue angle (interpreted mod 360), and s and
    /// v are between 0 and 1.
    ///
    /// ```
    /// use foxbox_taxonomy::api::Error;
    /// use foxbox_taxonomy::io::*;
    /// use foxbox_taxonomy::parse::*;
    /// use foxbox_taxonomy::values::*;
    ///
    /// println!("Testing parsing");
    /// let source = "{
    ///   \"h\": 220.5,
    ///   \"s\": 0.8,
    ///   \"v\": 0.4
    /// }";
    ///
    /// let parsed = Color::parse_str(source).unwrap();
    /// let Color::HSV(h, s, v) = parsed;
    /// assert_eq!(h, 220.5);
    /// assert_eq!(s, 0.8);
    /// assert_eq!(v, 0.4);
    ///
    /// println!("Testing serialization");
    /// let serialized : JSON = Color::serialize(&parsed, &BinaryTarget).unwrap();
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
    /// match Color::parse_str(source_2) {
    ///   Err(Error::ParseError(ParseError::TypeError{..})) => {},
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
    /// match Color::parse_str(source_4) {
    ///   Err(Error::ParseError(ParseError::MissingField{ref name, ..})) if &name as &str == "h" => {},
    ///   other => panic!("Unexpected result {:?}", other)
    /// }
    /// ```
    HSV(f64, f64, f64)
}
impl Data for Color {
    fn description() -> String {
        "Color {h, s, v}".to_owned()
    }
    fn parse(path: Path, source: &JSON, _binary: &BinarySource) -> Result<Self, Error> {
        let h = try!(path.push("h", |path| f64::take(path, source, "h")));
        let s = try!(path.push("s", |path| f64::take(path, source, "s")));
        let v = try!(path.push("v", |path| f64::take(path, source, "v")));
        // h can be any hue angle, will be interpreted (mod 360) in [0, 360).
        for &(val, ref name) in &vec![(&s, "s"), (&v, "v")] {
            if *val < 0. || *val > 1. {
                return Err(Error::ParseError(ParseError::type_error(name, &path, "a number in [0, 1]")));
            }
        }
        Ok(Color::HSV(h, s, v))
    }
    fn serialize(source: &Self, _binary: &BinaryTarget) -> Result<JSON, Error> {
        let &Color::HSV(ref h, ref s, ref v) = source;
        let vec = vec![("h", h), ("s", s), ("v", v)];
        Ok(vec.to_json())
    }
}


/// Representation of an object in JSON. It is often (albeit not
/// always) possible to choose a more precise data structure for
/// representing values send/accepted by a service. If possible,
/// adapters should rather pick such more precise data structure.
#[derive(Debug, Clone, PartialEq)]
pub struct Json(pub serde_json::value::Value);

impl Data for Json {
    fn description() -> String {
        "JSON".to_owned()
    }
    fn parse(_path: Path, source: &JSON, _binary: &BinarySource) -> Result<Self, Error> {
        Ok(Json(source.clone()))
    }
    fn serialize(source: &Self, _binary: &BinaryTarget) -> Result<JSON, Error> {
        Ok(source.0.clone())
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

/// A (probably large) binary value.
///
/// Since this value is considered large, `clone()` is not implemented.
#[derive(Debug, PartialEq)]
pub struct Binary {
    /// The binary data.
   pub data: Vec<u8>,

   /// The mime type.
   pub mimetype: Id<MimeTypeId>,
}

impl Data for Binary {
    fn description() -> String {
        "Binary".to_owned()
    }
    fn parse(path: Path, source: &JSON, _binary: &BinarySource) -> Result<Self, Error> {
        let data = try!(path.push("data", |path| Vec::<u8>::take(path, source, "data").map_err(Error::ParseError)));
        let mimetype = try!(path.push("mimetype", |path| Id::take(path, source, "mimetype").map_err(Error::ParseError)));
        Ok(Binary {
            data: data,
            mimetype: mimetype
        })
    }
    fn serialize(source: &Self, _binary: &BinaryTarget) -> Result<JSON, Error> {
        Ok(source.to_json())
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
/// use foxbox_taxonomy::api::Error;
/// use foxbox_taxonomy::io::*;
/// use foxbox_taxonomy::parse::*;
/// use foxbox_taxonomy::values::*;
///
/// use chrono::Datelike;
///
/// # fn main() {
///
/// let ts = TimeStamp::parse_str("\"2014-11-28T21:45:59.324310806+09:00\"").unwrap();
/// let date_time = ts.as_datetime();
/// assert_eq!(date_time.year(), 2014);
/// assert_eq!(date_time.month(), 11);
/// assert_eq!(date_time.day(), 28);
///
///
/// let serialized: JSON = TimeStamp::serialize(&ts, &BinaryTarget).unwrap();
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

impl Data for TimeStamp {
    fn description() -> String {
        "TimeStamp (RFC 3339)".to_owned()
    }
    fn parse(path: Path, source: &JSON, _binary: &BinarySource) -> Result<Self, Error> {
        use chrono::{ DateTime, UTC };
        use std::str::FromStr;
        if let JSON::String(ref str) = *source {
            if let Ok(dt) = DateTime::<UTC>::from_str(str) {
                return Ok(TimeStamp(dt));
            }
        }
        Err(Error::ParseError(ParseError::type_error("TimeStamp", &path, "date string (RFC 3339)")))
    }
    fn serialize(source: &Self, _binary: &BinaryTarget) -> Result<JSON, Error> {
         Ok(JSON::String(source.0.to_rfc3339()))
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


/// A comparison between two values.
///
/// # JSON
///
/// A range is an object with one field `{key: value}`.
///
#[derive(Clone, Debug, PartialOrd, PartialEq)]
pub enum Range<T> where T: Data + PartialOrd + PartialEq {
    /// Leq(x) accepts any value v such that v <= x.
    ///
    /// # JSON
    ///
    /// ```
    /// extern crate foxbox_taxonomy;
    /// extern crate serde_json;
    ///
    /// use foxbox_taxonomy::io::*;
    /// use foxbox_taxonomy::parse::*;
    /// use foxbox_taxonomy::values::*;
    ///
    /// # fn main() {
    ///
    /// let source = "{
    ///   \"Leq\": \"On\"
    /// }";
    ///
    /// let parsed = Range::<OnOff>::from_str(source).unwrap();
    /// if let Range::Leq(OnOff::On) = parsed {
    ///   // Ok
    /// } else {
    ///   panic!();
    /// }
    ///
    /// let as_json = Range::<OnOff>::serialize(&parsed, &BinaryTarget).unwrap();
    /// let as_string = serde_json::to_string(&as_json).unwrap();
    /// assert_eq!(as_string, "{\"Leq\":\"On\"}");
    ///
    /// # }
    /// ```
    Leq(T),

    /// Geq(x) accepts any value v such that v >= x.
    Geq(T),

    /// BetweenEq {min, max} accepts any value v such that `min <= v`
    /// and `v <= max`. If `max < min`, it never accepts anything.
    BetweenEq { min:T, max:T },

    /// OutOfStrict {min, max} accepts any value v such that `v < min`
    /// or `max < v`
    OutOfStrict { min:T, max:T },

    /// Eq(x) accespts any value v such that v == x
    Eq(T),
}


impl<T> Range<T> where T: Data + PartialOrd + PartialEq {
    /// Determine if a value is accepted by this range.
    pub fn contains(&self, value: &Value) -> bool {
        use self::Range::*;
        let content = if let Some(content) = value.downcast::<T>() {
            content
        } else {
            return false;
        };
        match *self {
            Leq(ref max) => content <= max,
            Geq(ref min) => content >= min,
            BetweenEq { ref min, ref max } => min <= content && content <= max,
            OutOfStrict { ref min, ref max } => content < min || max < content,
            Eq(ref val) => content == val,
        }
    }
}

impl<T> Data for Range<T> where T: Data + PartialOrd + PartialEq {
    fn description() -> String {
        format!("Range of {}", T::description())
    }
    fn parse(path: Path, source: &JSON, binary: &BinarySource) -> Result<Self, Error> {
        use self::Range::*;
        match *source {
            JSON::Object(ref obj) if obj.len() == 1 => {
                let result = if let Some(v) = obj.get("Leq") {
                    Leq(try!(path.push("Leq", |path| T::parse(path, v, binary))))
                } else if let Some(v) = obj.get("Geq") {
                    Geq(try!(path.push("Geq", |path| T::parse(path, v, binary))))
                } else if let Some(v) = obj.get("Eq") {
                    Eq(try!(path.push("eq", |path| T::parse(path, v, binary))))
                } else if let Some(v) = obj.get("BetweenEq") {
                    let mut bounds = try!(path.push("BetweenEq", |path| T::parse_vec(path, v, binary)));
                    if bounds.len() == 2 {
                        let max = bounds.pop().unwrap();
                        let min = bounds.pop().unwrap();
                        BetweenEq {
                            min: min,
                            max: max
                        }
                    } else {
                        return Err(Error::ParseError(ParseError::type_error("BetweenEq", &path, "an array of two values")))
                    }
                } else if let Some(v) = obj.get("OutOfStrict") {
                    let mut bounds = try!(path.push("OutOfStrict", |path| T::parse_vec(path, v, binary)));
                    if bounds.len() == 2 {
                        let max = bounds.pop().unwrap();
                        let min = bounds.pop().unwrap();
                        OutOfStrict {
                            min: min,
                            max: max
                        }
                    } else {
                        return Err(Error::ParseError(ParseError::type_error("OutOfStrict", &path, "an array of two values")))
                    }
                } else {
                    return Err(Error::ParseError(ParseError::type_error("Range", &path, "a field Eq, Leq, Geq, BetweenEq or OutOfStrict")))
                };
                Ok(result)
            }
            _ => Err(Error::ParseError(ParseError::type_error("Range", &path, "object")))
        }
    }

    fn serialize(source: &Self, binary: &BinaryTarget) -> Result<JSON, Error> {
        let (key, value) = match *source {
            Range::Eq(ref val) => ("Eq", try!(T::serialize(val, binary))),
            Range::Geq(ref val) => ("Geq", try!(T::serialize(val, binary))),
            Range::Leq(ref val) => ("Leq", try!(T::serialize(val, binary))),
            Range::BetweenEq { ref min, ref max } => ("BetweenEq", JSON::Array(vec![
                try!(T::serialize(min, binary)),
                try!(T::serialize(max, binary))
            ])),
            Range::OutOfStrict { ref min, ref max } => ("OutOfStrict", JSON::Array(vec![
                try!(T::serialize(min, binary)),
                try!(T::serialize(max, binary))
            ])),
        };
        Ok(vec![(key, value)].to_json())
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

impl Duration {
    pub fn as_duration(&self) -> ChronoDuration {
        self.0
    }
}

impl Data for Duration {
    fn description() -> String {
        "Duration (s)".to_owned()
    }
    fn parse(path: Path, source: &JSON, _binary: &BinarySource) -> Result<Self, Error> {
        let val = try!(f64::parse(path, source).map_err(Error::ParseError));
        Ok(Duration(ChronoDuration::milliseconds((val * 1000.) as i64)))
    }
    fn serialize(source: &Self, _binary: &BinaryTarget) -> Result<JSON, Error> {
        Ok(source.to_json())
    }
}


impl ToJSON for Duration {
    fn to_json(&self) -> JSON {
        let val = self.0.num_milliseconds() as f64 / 1000 as f64;
        JSON::F64(val)
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


/// A library of standardized instances of `Format` for most common cases.
pub mod format {
    use io::*;
    use values::*;
    use std::sync::Arc;

    lazy_static! {
        pub static ref ON_OFF : Arc<Format> = Arc::new(Format::new::<OnOff>());
        pub static ref OPEN_CLOSED : Arc<Format> = Arc::new(Format::new::<OpenClosed>());
        pub static ref IS_SECURE : Arc<Format> = Arc::new(Format::new::<IsSecure>());
        pub static ref IS_LOCKED : Arc<Format> = Arc::new(Format::new::<IsLocked>());
        pub static ref COLOR : Arc<Format> = Arc::new(Format::new::<Color>());
        pub static ref JSON: Arc<Format> = Arc::new(Format::new::<Json>());
        pub static ref STRING : Arc<Format> = Arc::new(Format::new::<String>());
        pub static ref UNIT : Arc<Format> = Arc::new(Format::new::<()>());
        pub static ref BINARY : Arc<Format> = Arc::new(Format::new::<Binary>());
        pub static ref TIMESTAMP : Arc<Format> = Arc::new(Format::new::<TimeStamp>());
        pub static ref DURATION : Arc<Format> = Arc::new(Format::new::<Duration>());
    }
}