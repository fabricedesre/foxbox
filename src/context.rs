/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate serde_json;

use service::Service;
use std::collections::HashMap;
use std::sync::{ Arc, Mutex };

// The `global` context available to all.
pub struct Context {
    pub verbose: bool,
    pub hostname: String,
    pub http_port: u16,

    services: HashMap<String, Box<Service>>
}

pub type SharedContext = Arc<Mutex<Context>>;

impl Context {
    pub fn new(verbose: bool, hostname: Option<String>) -> Context {
        Context { services: HashMap::new(),
                  verbose: verbose,
                  hostname:  hostname.unwrap_or("localhost".to_string()),
                  http_port: 3000 }
    }

    pub fn shared(verbose: bool, hostname: Option<String>) -> SharedContext {
        Arc::new(Mutex::new(Context::new(verbose, hostname)))
    }

    pub fn add_service(&mut self, service: Box<Service>) {
        let service_id = service.get_properties().id;
        self.services.insert(service_id, service);
    }

    pub fn remove_service(&mut self, id: String) {
        self.services.remove(&id);
    }

    pub fn services_count(&self) -> usize {
        self.services.len()
    }

    pub fn get_service(&self, id: &str) -> Option<&Box<Service>> {
        self.services.get(id)
    }

    pub fn services_as_json(&self) -> Result<String, serde_json::error::Error> {
        serde_json::to_string(&self.services)
    }
}
