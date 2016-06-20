//! Representation of data.
//!
//! Instances of `Payload` are typically combinations of JSON and binary components.
#![allow(identity_op)] // FIXME: Remove this once Clippy accepts Serialize

use api::Error;
use parse::*;
use values::*;

use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

/// Placeholder.
pub struct BinarySource;

/// Placeholder.
pub struct BinaryTarget;


#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum SerializeError {
    JSON(String)
}
impl fmt::Display for SerializeError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        (self as &fmt::Debug).fmt(formatter)
    }
}
impl StdError for SerializeError {
    fn description(&self) -> &str {
        // Placeholder
        ""
    }
}
impl From<SerializeError> for Error {
    fn from(v: SerializeError) -> Error {
        Error::SerializeError(v)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Payload {
    json: JSON
}

impl Payload {
    fn new(json: JSON) -> Self {
        Payload {
            json: json
        }
    }
    pub fn empty() -> Self {
        Self::new(JSON::Null)
    }

    /// Serialize a `Value` into a `Payload`.
    pub fn from_value(value: &Value, format: &Arc<Format>) -> Result<Payload, Error> {
        format.serialize(value, &BinaryTarget)
            .map(Self::new)
    }
    pub fn from_data<T>(data: T, format: &Arc<Format>) -> Result<Payload, Error> where T: Data + PartialEq {
        Self::from_value(&Value::new(data), format)
    }
    pub fn to_value(&self, format: &Arc<Format>) -> Result<Value, Error> {
        format.parse(Path::new(), &self.json, &BinarySource)
    }
}

impl ToJSON for Payload {
    fn to_json(&self) -> JSON {
        self.json.clone()
    }
}

impl ToJSON for (Payload, Arc<Format>) {
    fn to_json(&self) -> JSON {
        self.0.to_json()
    }
}

impl Parser<Payload> for Payload {
    fn description() -> String {
        "JSON".to_owned()
    }
    fn parse(_: Path, source: &JSON) -> Result<Self, ParseError> {
        Ok(Payload {
            json: source.clone()
        })
    }
}

pub struct Format {
    description: Box<Fn() -> String + Send + Sync>,
    #[allow(type_complexity)] // This type is used exactly once in the code.
    parse: Box<Fn(Path, &JSON, &BinarySource) -> Result<Value, Error> + Send + Sync>,
    serialize: Box<Fn(&Value, &BinaryTarget) -> Result<JSON, Error> + Send + Sync>
}
impl Format {
    #[allow(new_without_default)] // Clippy's warning doesn't make sense.
    pub fn new<T>() -> Self where T: Data + PartialEq {
        let description = || T::description();
        Format {
            description: Box::new(description),
            parse: Box::new(|path, source, binary| {
                T::parse(path, source, binary)
                    .map(Value::new)
            }),
            serialize: Box::new(|value, target| {
                let value : &Value = value;
                let data = try!(value.cast::<T>());
                T::serialize(data, target)
            }),
        }
    }

    /// A human-readable description of the _type_ of the value.
    ///
    /// Used mainly in `TypeError` error messages.
    pub fn description(&self) -> String {
        (self.description)()
    }

    /// Attempt to build a `Value` from a json `source` and `binary` components.
    pub fn parse(&self, path: Path, source: &JSON, binary: &BinarySource) -> Result<Value, Error> {
        (self.parse)(path, source, binary)
    }

    /// Serialize a `Value` into a `JSON`, storing binary data in `binary`.
    pub fn serialize(&self, source: &Value, binary: &BinaryTarget) -> Result<JSON, Error> {
        (self.serialize)(source, binary)
    }
}

impl fmt::Debug for Format {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.description().fmt(formatter)
    }
}
