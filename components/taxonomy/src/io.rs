//! Representation of data.
//!
//! Instances of `Payload` are typically combinations of JSON and binary components.
use api::Error;
use parse::*;
use values::*;

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

    /// Serialize a `Value` into a `Payload`.
    pub fn from_value(value: &Value, type_: &Type) -> Result<Payload, Error> {
        // Placeholder implementation. Future versions will actually use `type_`
        // for serialization purposes.
        if value.get_type() != *type_ {
            return Err(Error::TypeError(TypeError {expected: type_.clone(), got: value.get_type()} ));
        }
        Ok(Payload {
            json: value.to_json()
        })
    }
    pub fn to_value(&self, type_: &Type) -> Result<Value, Error> {
        // Placeholder implementation. Future versions will actually use `type_`
        // for deserialization purposes.
        let value = try!(Value::parse(Path::new(), &self.json).map_err(Error::ParseError));
        if value.get_type() != *type_ {
            return Err(Error::TypeError(TypeError {expected: type_.clone(), got: value.get_type()} ));
        }
        Ok(value)
    }
}

impl ToJSON for Payload {
    fn to_json(&self) -> JSON {
        self.json.clone()
    }
}

impl ToJSON for (Payload, Type) {
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