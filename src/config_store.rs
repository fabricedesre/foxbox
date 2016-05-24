/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde_json;
use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::sync::{ Mutex, RwLock };

type ConfigNameSpace = BTreeMap<String, String>;

type ConfigTree = BTreeMap<String, ConfigNameSpace>;

#[derive(Debug)]
pub struct ConfigStore {
    file_name: String,
    save_lock: Mutex<()>,
    config: ConfigTree,
    overrides: ConfigTree
}

impl ConfigStore {
    pub fn new(file_name: &str) -> Self {
        ConfigStore {
            file_name: file_name.to_owned(),
            save_lock: Mutex::new(()),
            config: ConfigStore::load(file_name),
            overrides: ConfigTree::new()
        }
    }

    pub fn set(&mut self, namespace: &str, property: &str, value: &str) {
        debug!("Setting config for {}::{} to {}", namespace, property, value);
        if !self.config.contains_key(namespace) {
            self.config.insert(namespace.to_owned(), ConfigNameSpace::new());
        }
        self.config.get_mut(namespace).unwrap().insert(property.to_owned(), value.to_owned());
        // TODO: Should be more intelligent than save on every write
        self.save();
    }

    pub fn get(&self, namespace: &str, property: &str) -> Option<&String> {
        match self.get_override(namespace, property) {
            Some(value) => Some(value),
            None => self.get_no_override(namespace, property)
        }
    }

    fn get_no_override(&self, namespace: &str, property: &str) -> Option<&String> {
        if self.config.contains_key(namespace) {
            let res = self.config.get(namespace).unwrap().get(property);
            debug!("Config result for {}::{} is {:?}", namespace, property, res);
            res
        } else {
            debug!("No config result for {}::{}", namespace, property);
            None
        }
    }

    pub fn set_override(&mut self, namespace: &str, property: &str, value: &str) {
        debug!("Setting config override for {}::{} to {}", namespace, property, value);
        if !self.overrides.contains_key(namespace) {
            self.overrides.insert(namespace.to_owned(), ConfigNameSpace::new());
        }
        self.overrides.get_mut(namespace).unwrap().insert(property.to_owned(), value.to_owned());
    }

    fn get_override(&self, namespace: &str, property: &str) -> Option<&String> {
        if self.overrides.contains_key(namespace) {
            let res = self.overrides.get(namespace).unwrap().get(property);
            debug!("Config override for {}::{} is {:?}", namespace, property, res);
            res
        } else {
            None
        }
    }

    fn load(file_name: &str) -> ConfigTree {
        let empty_config = BTreeMap::new();
        let file = match File::open(&Path::new(file_name)) {
            Ok(file) => {
                file
            },
            Err(error) => {
                debug!("Unable to open configuration file {}: {}",
                    file_name, error.to_string());
                return empty_config;
            }
        };
        let parsed_config: ConfigTree = match serde_json::from_reader(&file) {
            Ok(value) => value,
            Err(error) => {
                error!("Unable to generate JSON from config file {}: {}",
                    file_name, error.to_string());
                    empty_config
            }
        };

        debug!("Parsed config file: {:?}", parsed_config);
        parsed_config
    }

    fn save(&self) {
        let file_path = Path::new(&self.file_name);
        let mut update_name = self.file_name.clone();
        update_name.push_str(".updated");
        let update_path = Path::new(&update_name);

        let conf_as_json = serde_json::to_string_pretty(&self.config).unwrap();

        let _ = self.save_lock.lock().unwrap();
        match File::create(update_path)
            .map(|mut file| file.write_all(conf_as_json.as_bytes()))
            .and_then(|_| { fs::copy(&update_path, &file_path) })
            .and_then(|_| { fs::remove_file(&update_path) }) {
                Ok(_) => debug!("Wrote configuration file {}", self.file_name),
                Err(error) => error!("While writing configuration file{}: {}",
                    self.file_name, error.to_string())
            };
    }
}

pub struct ConfigService {
    store: RwLock<ConfigStore>
}

impl ConfigService {
    pub fn new(file_name: &str) -> Self {
        ConfigService {
            store: RwLock::new(ConfigStore::new(file_name))
        }
    }

    pub fn get(&self, namespace: &str, property: &str) -> Option<String> {
        self.store.read().unwrap().get(namespace, property)
            .map(|value| { value.to_owned() })
    }

    pub fn get_or_set_default(&self, namespace: &str, property: &str, default: &str) -> String {
        self.get(namespace, property).unwrap_or_else(|| {
            self.set(namespace, property, default);
            default.to_owned()
        })
    }

    pub fn set(&self, namespace: &str, property: &str, value: &str) {
        self.store.write().unwrap().set(namespace, property, value);
    }

    pub fn set_override(&self, namespace: &str, property: &str, value: &str) {
        self.store.write().unwrap().set_override(namespace, property, value);
    }
}

#[cfg(test)]
describe! config {

    before_each {
        use uuid::Uuid;
        use std::fs;
        let config_file_name = format!("conftest-{}.tmp", Uuid::new_v4().to_simple_string());
    }

    after_each {
        fs::remove_file(config_file_name).unwrap_or(());
    }

    describe! config_store {
        before_each {
            let mut config = ConfigStore::new(&config_file_name);
        }

        it "should remember properties" {
            config.set("foo", "bar", "baz");
            let foo_bar = config.get("foo", "bar").unwrap();
            assert_eq!(foo_bar, "baz");
        }

        it "should return None on non-existent namespaces" {
            config.set("foo", "bar", "baz");
            assert_eq!(config.get("foofoo", "bar"), None);
        }

        it "should return None on non-existent properties" {
            config.set("foo", "bar", "baz");
            assert_eq!(config.get("foo", "barbar"), None);
        }
    }

    describe! config_service {
        before_each {
            let config = ConfigService::new(&config_file_name);
        }

        it "should remember properties" {
            config.set("foo", "bar", "baz");
            let foo_bar = config.get("foo", "bar").unwrap();
            assert_eq!(foo_bar, "baz");
        }

        it "should return the default value when needed" {
            let res = config.get_or_set_default("foo", "bar", "default");
            assert_eq!(res, "default");

            let foo_bar = config.get("foo", "bar").unwrap();
            assert_eq!(foo_bar, "default");
        }

        it "should return None on non-existent namespaces" {
            config.set("foo", "bar", "baz");
            assert_eq!(config.get("foofoo", "bar"), None);
        }

        it "should return None on non-existent properties" {
            config.set("foo", "bar", "baz");
            assert_eq!(config.get("foo", "barbar"), None);
        }

        it "should accept overrides" {
            config.set("foo", "bar", "baz");
            let foo_bar = config.get("foo", "bar").unwrap();
            assert_eq!(foo_bar, "baz");
            config.set_override("foo", "bar", "bazbaz");
            let foo_baz = config.get("foo", "bar").unwrap();
            assert_eq!(foo_baz, "bazbaz");
        }
    }

    describe! restarts {
        it "ConfigStore should remember things over restarts" {
            // Block to make `config` go out of scope
            {
                let mut config = ConfigStore::new(&config_file_name);
                config.set("foo", "bar", "baz");
            }
            // `config` should now be out of scope and dropped
            {
                let config = ConfigStore::new(&config_file_name);
                let foo_bar = config.get("foo", "bar").unwrap();
                assert_eq!(foo_bar, "baz");
            };
        }

        it "ConfigStore should forget overrides over restarts" {
            {
                let mut config = ConfigStore::new(&config_file_name);
                config.set("foo", "bar", "baz");
                config.set_override("foo", "bar", "bazbaz");
            }
            {
                let config = ConfigStore::new(&config_file_name);
                let foo_bar = config.get("foo", "bar").unwrap();
                assert_eq!(foo_bar, "baz");
            };
        }

        it "ConfigService should remember things over restarts" {
            // Block to make `config` go out of scope
            {
                let config = ConfigService::new(&config_file_name);
                config.set("foo", "bar", "baz");
            }
            // `config` should now be out of scope and dropped
            {
                let config = ConfigService::new(&config_file_name);
                let foo_bar = config.get("foo", "bar").unwrap();
                assert_eq!(foo_bar, "baz");
            };
        }
    }
}
