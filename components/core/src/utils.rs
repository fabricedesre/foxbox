/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::io::Read;
use xml::reader::{ EventReader, XmlEvent };

// Macros to help with json serializing of undeclared structs.
// json_value!({ }) returns a serde_json::value::Value from an anonymous struct.
// json!({ }) returns a string from an anonymous struct.
// Combining both macros let you build arbitrary complex json objects easily.
//
// Examples: json!({ type: "light", value: 1.2, on: true });
//           json!({ type: "notification", value: json_value!({ r: 54, g: 12, b: 88 }) });

#[macro_export]
macro_rules! json {
    ({ $($i:ident: $v:expr),* }) => {
        {
            use std::collections::BTreeMap;
            let mut map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            $(
                map.insert(String::from(stringify!($i)), serde_json::to_value(&$v));
            )*
            serde_json::to_string(&map).unwrap_or("{}".to_owned())
        }
    };

    ([ $($v:expr),* ]) => {
        {
            let mut vec: Vec<serde_json::Value> = Vec::new();
            $(
                vec.push(serde_json::to_value(&$v));
            )*
            serde_json::to_string(&vec).unwrap_or("[]".to_owned())
        }
    }
}

#[macro_export]
macro_rules! json_value {
    ({ $($i:ident: $v:expr),* }) => {
        {
            use std::collections::BTreeMap;
            let mut map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            $(
                map.insert(String::from(stringify!($i)), serde_json::to_value(&$v));
            )*
            serde_json::to_value(&map)
        }
    };

    ([ $($v:expr),* ]) => {
        {
            let mut vec: Vec<serde_json::Value> = Vec::new();
            $(
                vec.push(serde_json::to_value(&$v));
            )*
            serde_json::to_value(&vec)
        }
    }
}

#[macro_export]
macro_rules! current_dir {
    () => {
        {
            use std::path::PathBuf;
            let mut this_file = PathBuf::from(file!());
            this_file.pop();
            this_file.to_str().unwrap().to_owned()
        }
    };
}

#[macro_export]
macro_rules! checklock (
    ($e: expr) => {
        match $e {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("lock was poisoned - using anyway..");
                poisoned.into_inner()
            },
        }
    }
);

#[cfg(test)]
#[macro_use]
describe! json {
    before_each {
        use serde_json;
    }

    describe! objects {
        it "should support string attributes" {
            assert_eq!(json!({ foo: "bar" }), r#"{"foo":"bar"}"#);
        }

        it "should support numbers" {
            assert_eq!(json!({ val: 1.02 }), r#"{"val":1.02}"#);
        }

        it "should support mixed types" {
            assert_eq!(json!({ type: "light", value: 1.2, on: true }),
                        r#"{"on":true,"type":"light","value":1.2}"#);
        }

        it "should support arrays" {
            assert_eq!(json!({ array: vec![1, 2, 3]}), r#"{"array":[1,2,3]}"#);
        }

        it "should support sub-objects" {
            assert_eq!(json!({ type: "complex", sub: json_value!({ a: 1, b: "foo" }) }),
                        r#"{"sub":{"a":1,"b":"foo"},"type":"complex"}"#);
        }

        it "should support sub-sub-objects" {
            assert_eq!(json!({ one: json_value!({ two: json_value!({ three: 3 }) }) }),
                        r#"{"one":{"two":{"three":3}}}"#);
        }
    }

    describe! arrays {
        it "should support numbers" {
            assert_eq!(json!([1, 100, 1000]), r#"[1,100,1000]"#);
        }

        it "should support sub-arrays" {
            assert_eq!(json!(["one", "two", json_value!(["three", "four"]), 5]),
                        r#"["one","two",["three","four"],5]"#);
        }

        it "should support sub-sub-arrays" {
            assert_eq!(json!(["one", json_value!(["two", json_value!(["three"]) ]) ]),
                        r#"["one",["two",["three"]]]"#);
        }

        it "should support mixed types" {
            assert_eq!(json!(["one", 2, 3.33, false]), r#"["one",2,3.33,false]"#);
        }

        it "should support objects" {
            assert_eq!(json!([ json_value!({foo: "bar"}) ]), r#"[{"foo":"bar"}]"#);
        }
    }
}

pub fn parse_simple_xml<R: Read>(data: R) -> Result<HashMap<String, String>, String> {
    let parser = EventReader::new(data);
    let mut values = HashMap::<String, String>::new();
    let mut ignore = HashMap::<String, bool>::new();
    let mut key = String::new();
    for e in parser {
        match e {
            Ok(XmlEvent::StartElement { name, .. }) => {
                key.push('/');
                key.push_str(name.local_name.as_str());
                if !ignore.contains_key(&key) && values.contains_key(&key) {
                    // Array of elements, let's just ignore those for now and maybe
                    // find a better XML parser ;)
                    ignore.insert(key.clone(), true);
                    values.remove(&key);
                }
            }
            Ok(XmlEvent::EndElement { name, .. }) => {
                // Should ensure truncated name and given name match?
                match key.rfind('/') {
                    Some(x) => { key.truncate(x); }
                    _ => { return Err(format!("broken key {} at ending element {}", key, name)); }
                }
            }
            Ok(XmlEvent::Characters(x)) | Ok(XmlEvent::CData(x)) => {
                if !ignore.contains_key(&key) {
                    values.entry(key.clone()).or_insert_with(|| { String::new() }).push_str(x.as_str());
                }
            }
            Err(e) => { return Err(format!("parse error {}", e)); }
            _ => { }
        }
    }
    Ok(values)
}

#[allow(dead_code)]
pub fn escape(unescaped_string: &str, to_escape: Vec<char>) -> String {
    let mut escaped_string = String::new();
    for chr in unescaped_string.to_owned().chars() {
        if chr == '\\' || to_escape.contains(&chr) {
            escaped_string.push('\\');
        }
        escaped_string.push(chr);
    }
    escaped_string
}

#[allow(dead_code)]
pub fn unescape(escaped_string: &str) -> String {
    let mut unescaped_string = String::new();
    let mut escape_flag = false;
    for chr in escaped_string.to_owned().chars() {
        if escape_flag {
            escape_flag = false;
            unescaped_string.push(chr);
        } else if chr == '\\' {
            escape_flag = true;
        } else {
            unescaped_string.push(chr);
        }
    }
    unescaped_string
}

pub fn split_escaped(string: &str, separator: char) -> Vec<String> {
    let mut split = Vec::new();
    let mut section = String::new();
    let mut escape_flag = false;
    for chr in string.to_owned().chars() {
        if escape_flag {
            escape_flag = false;
            section.push(chr);
        } else if chr == '\\' {
            escape_flag = true;
        } else if chr == separator {
            split.push(section);
            section = String::new();
        } else {
            section.push(chr);
        }
    }
    split.push(section);
    split
}

#[cfg(test)]
describe! string_escaping {
    it "should escape strings" {
        assert_eq!(escape(r#"foo;bar\baz"#, vec![';']), r#"foo\;bar\\baz"#.to_owned())
    }

    it "should unescape strings" {
        assert_eq!(unescape(r#"foo\;bar\\b\az"#), r#"foo;bar\baz"#.to_owned())
    }

    it "should split escaped strings" {
        assert_eq!(split_escaped(r#"foo\;foo;bar;"#, ';'), vec!["foo;foo", "bar", ""]);
    }
}
