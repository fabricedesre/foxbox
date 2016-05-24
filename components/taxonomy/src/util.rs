use parse::*;

use std::cmp::PartialEq;
use std::collections::HashMap;
use std::hash::{ Hash, Hasher };
use std::marker::PhantomData;
use std::fmt;

use string_cache::Atom;

use serde::ser::{ Serialize, Serializer };
use serde::de::{ Deserialize, Deserializer, Error, Type };

/// A marker for a request that a expects a specific value.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Exactly<T> {
    /// No constraint.
    Always,

    /// Expect a specific value.
    Exactly(T),

    /// Never accept a constraint. This can happen, for instance, we have have
    /// attempted to `and` two conflicting `Exactly`
    Never,
}

impl<T> Parser<Exactly<T>> for Exactly<T> where T: Parser<T> {
    fn description() -> String {
        T::description()
    }
    /// Parse a single value from JSON, consuming as much as necessary from JSON.
    fn parse(path: Path, source: &JSON) -> Result<Self, ParseError> {
        if let JSON::Null = *source {
            Ok(Exactly::Always)
        } else {
            T::parse(path, source).map(Exactly::Exactly)
        }
    }
}

impl<T> Exactly<T> where T: PartialEq {
    /// Combine two constraints.
    pub fn and(self, other: Self) -> Self {
        use self::Exactly::*;
        match (self, other) {
            (Never, _) | (_, Never) => Never,
            (Always, x) | (x, Always) => x,
            (Exactly(x), Exactly(y)) =>
                if x == y {
                    Exactly(y)
                } else {
                    Never
                }
        }
    }

    pub fn is_empty(&self) -> bool {
        match *self {
            Exactly::Always => true,
            _ => false,
        }
    }

    pub fn matches(&self, value: &T) -> bool {
        match *self {
            Exactly::Always => true,
            Exactly::Exactly(ref id) => id == value,
            _ => false
        }
    }
}

impl<T> Default for Exactly<T> {
    fn default() -> Self {
        Exactly::Always
    }
}

/// A variant of `PhantomData` that supports [De]serialization
#[derive(Clone, Debug, PartialEq, Hash, Eq)]
pub struct Phantom<T> {
    phantom: PhantomData<T>
}

impl<T> Default for Phantom<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Phantom<T> {
    pub fn new() -> Self {
        Phantom {
            phantom: PhantomData
        }
    }
}
impl<T> Serialize for Phantom<T> {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
        where S: Serializer {
            ().serialize(serializer)
    }
}
impl<T> Deserialize for Phantom<T> {
    fn deserialize<D>(_: &mut D) -> Result<Self, D::Error>
        where D: Deserializer {
        // Nothing to consume
        Ok(Phantom {
            phantom: PhantomData
        })
    }
}


/// A bunch of results coming from different sources.
pub type ResultMap<K, T, E> = HashMap<K, Result<T, E>>;

/// A bunch of instructions, going to different targets.
pub type TargetMap<K, T> = Vec<Targetted<K, T>>;

#[derive(Clone)]
pub struct Targetted<K, T> where K: Clone, T: Clone {
    pub select: Vec<K>,
    pub payload: T
}
impl<K, T> Default for Targetted<K, T> where T: Default + Clone, K: Clone {
    fn default() -> Self {
        Targetted {
            select: vec![],
            payload: T::default()
        }
    }
}
impl<K, T> Targetted<K, T> where K: Clone, T: Clone {
    pub fn new(select: Vec<K>, payload: T) -> Self {
        Targetted {
            select: select,
            payload: payload
        }
    }
}

/// A unique id for values of a given kind.
///
/// # Performance
///
/// This data structure is tuned for specific use cases and should be used accordingly.
///
/// - Using an instances of `Id` as a key in a `HashMap` or `HashSet` is much faster than
///   using a `String`.
/// - Comparing two instances of `Id` is much faster than comparing two `String`s.
/// - Instances of `Id` take very little memory.
/// - Cloning an instance of `Id` is relatively fast, but should be avoided if possible.
/// - Calling `Id::new`, on the other hand, is *very slow*. **Always prefer cloning to calling
///   `Id::new`**.
///
/// # (De)serialization
///
/// Serialized values of this type are represented by plain strings.
///
/// ```
/// extern crate serde;
/// extern crate serde_json;
/// extern crate foxbox_taxonomy;
///
/// #[derive(Debug)]
/// struct UniqueId;
///
/// let my_id = foxbox_taxonomy::util::Id::<UniqueId>::new("Unique Identifier");
///
/// assert_eq!(my_id.to_string(), "Unique Identifier");
///
/// let my_serialized_id = serde_json::to_string(&my_id).unwrap();
/// assert_eq!(my_serialized_id, "\"Unique Identifier\"");
///
/// let my_deserialized_id: foxbox_taxonomy::util::Id<UniqueId> =
///     serde_json::from_str("\"Unique Identifier\"").unwrap();
/// assert_eq!(my_deserialized_id, my_id);
/// ```
#[derive(Debug, Clone)]
pub struct Id<T> {
    id: Atom,

    phantom: Phantom<T>,
}

impl<T> Id<T> {
    pub fn new(id: &str) -> Self {
        Id {
            id: Atom::from(id),
            phantom: Phantom::new(),
        }
    }

    pub fn as_atom(&self) -> &Atom {
        &self.id
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.id.as_ref())
    }
}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl<T> Eq for Id<T> {
}

impl<T> Hash for Id<T> {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.id.hash(state)
    }
}

impl<T> Serialize for Id<T> {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
        where S: Serializer {
        self.id.as_ref().serialize(serializer)
    }
}

impl<T> Parser<Id<T>> for Id<T> {
    fn description() -> String {
        "Id".to_owned()
    }
    /// Parse a single value from JSON, consuming as much as necessary from JSON.
    fn parse(path: Path, source: &JSON) -> Result<Self, ParseError> {
        if let JSON::String(ref string) = *source {
            Ok(Id::new(string))
        } else {
            Err(ParseError::type_error("id", &path, "string"))
        }
    }
}

impl<T> ToJSON for Id<T> {
    fn to_json(&self) -> JSON {
        JSON::String(self.to_string())
    }
}

impl<T> Deserialize for Id<T> {
    fn deserialize<D>(deserializer: &mut D) -> Result<Self, D::Error>
        where D: Deserializer {
        let deserialized_string = try!(String::deserialize(deserializer));

        Ok(Id {
            id: Atom::from(deserialized_string),
            phantom: Phantom::new(),
        })
    }
}

impl<T, U> ToJSON for HashMap<Id<U>, T> where T: ToJSON {
    fn to_json(&self) -> JSON {
        JSON::Object(self.iter().map(|(k, v)| (k.to_string(), T::to_json(v))).collect())
    }
}


/// By default, the (de)serialization of trivial enums by Serde is surprising, e.g.
/// in JSON,  `enum Foo {A, B, C}` will produce `{"\"A\": []"}` for `A`, where `"\"A\""`
/// would be expected.
///
/// Implementing serialization is very simple, but deserialization is much more annoying.
/// This struct lets us implement simply the deserialization to a predictable and well-specified
/// list of strings.
///
/// # Example
///
/// ```
/// extern crate serde;
/// use serde::de::{Deserialize, Deserializer};
///
/// extern crate foxbox_taxonomy;
/// use foxbox_taxonomy::util::TrivialEnumVisitor;
///
/// enum Foo { A, B, C }
///
/// impl Deserialize for Foo {
///   fn deserialize<D>(deserializer: &mut D) -> Result<Self, D::Error> where D: Deserializer {
///     deserializer.deserialize_string(TrivialEnumVisitor::new(|source| {
///       match source {
///         "A" => Ok(Foo::A),
///         "B" => Ok(Foo::B),
///         "C" => Ok(Foo::C),
///          _ => Err(())
///       }
///    }))
///   }
/// }
///
/// # fn main() {}
/// ```
pub struct TrivialEnumVisitor<T> where T: Deserialize {
    parser: Box<Fn(&str) -> Result<T, ()>>
}

impl<T> TrivialEnumVisitor<T> where T: Deserialize {
    pub fn new<F>(parser: F) -> Self
        where F: Fn(&str) -> Result<T, ()> + 'static {
            TrivialEnumVisitor {
                parser: Box::new(parser)
            }
        }
    fn parse<E>(&self, source: &str) -> Result<T, E>
        where E: Error
    {
        (self.parser)(source)
            .map_err(|()| E::unknown_field(&source.to_owned()))
    }
}

use serde::de::Visitor;
impl<T> Visitor for TrivialEnumVisitor<T> where T: Deserialize {
    type Value = T;
    fn visit_str<E>(&mut self, v: &str) -> Result<T, E>
        where E: Error,
    {
        self.parse(v)
    }

    fn visit_string<E>(&mut self, v: String) -> Result<T, E>
        where E: Error,
    {
        self.parse(&v)
    }

    fn visit_bytes<E>(&mut self, v: &[u8]) -> Result<T, E>
        where E: Error,
    {
        use std::str;
        match str::from_utf8(v) {
            Ok(s) => self.parse(s),
            Err(_) => Err(E::invalid_type(Type::String)),
        }
    }

    fn visit_byte_buf<E>(&mut self, v: Vec<u8>) -> Result<T, E>
        where E: Error,
    {
        match String::from_utf8(v) {
            Ok(s) => self.parse(&s),
            Err(_) => Err(Error::invalid_type(Type::String)),
        }
    }
}

/// A marker for Id.
/// Only useful for writing `Id<ServiceId>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash, Eq)]
pub struct ServiceId;

/// A marker for Id.
/// Only useful for writing `Id<AdapterId>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash, Eq)]
pub struct AdapterId;

// A marker for Id.
/// Only useful for writing `Id<TagId>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash, Eq)]
pub struct TagId;


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash, Eq)]
pub struct KindId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash, Eq)]
pub struct VendorId;

#[derive(Clone, Debug)]
pub struct MimeTypeId;

/// Helper function, to check that a type implements Sync.
pub fn is_sync<T: Sync>() {}
