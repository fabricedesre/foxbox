//! Representation of data.
//!
//! Instances of `Payload` are typically combinations of JSON and binary components.
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

/// Placeholder.
#[derive(Clone, Debug, Serialize)]
pub struct SerializeError;
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

pub trait Format: Send + Sync + 'static {
    fn description(&self) -> String;
    fn parse(&self, source: &JSON, _binary: &BinarySource) -> Result<Value, ParseError> {
        // Placeholder implementation
        Value::parse(Path::new(), source)
    }
    fn serialize(&self, source: &Value, _binary: &BinaryTarget) -> Result<JSON, SerializeError> {
        // Placeholder implementation
        Ok(source.to_json())
    }

}

impl fmt::Debug for Format {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        fmt.write_str(&self.description())
    }
}


#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Payload {
    json: JSON
}

impl Payload {
    /// Serialize a `Value` into a `Payload`.
    /// Note that this is a placeholder method. It will disappear very soon.
    #[deprecated]
    pub fn from_value_auto(value: &Value) -> Payload {
        Payload {
            json: value.to_json()
        }
    }

    pub fn empty() -> Self {
        Payload {
            json: JSON::Null
        }
    }

    /// Serialize a `Value` into a `Payload`.
    pub fn from_value(value: &Value, format: &Arc<Format>) -> Result<Payload, Error> {
        match format.serialize(value, &BinaryTarget) {
            Ok(json) => Ok(Payload {
                json: json
            }),
            Err(err) => Err(Error::SerializeError(err))
        }
    }
    pub fn to_value(&self, format: &Arc<Format>) -> Result<Value, Error> {
        format.parse(&self.json, &BinarySource)
            .map_err(Error::ParseError)
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

