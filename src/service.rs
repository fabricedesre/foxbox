/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use iron::{Request, Response, IronResult};
use serde::ser::{ Serialize, Serializer };

pub type ServiceID = String;

#[derive(Debug, Clone, Serialize)]
pub struct ServiceProperties {
    pub id: ServiceID,
    pub name: String,
    pub description: String,
    pub http_url: String,
    pub ws_url: String
}

pub trait Service : Send + Sync {
    fn get_properties(&self) -> ServiceProperties;
    fn start(&self);
    fn stop(&self);
    fn process_request(&self, req: &Request) -> IronResult<Response>;
}

impl Serialize for Service {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
        where S: Serializer {
        let props = self.get_properties();
        props.serialize(serializer)
    }
}

pub trait ServiceAdapter {
    fn get_name(&self) -> String;
    fn start(&self);
    fn stop(&self);
}


#[cfg(test)]
describe! service {
    before_each {
        extern crate serde_json;
        use stubs::service::ServiceStub;

        // Box<T> is required because Service is a Trait object. We can't manipulate is unless
        // we have a reference to it
        let service: Box<Service> = Box::new(ServiceStub);
    }

    it "should be serializable" {
        // This works because serialize() is called thanks to to_string(). This is not a real unit
        // test, then. A mock should be used below. As mocks are not easily spy-able, that would
        // require a to implement each of the functions defined here[1]. That sounds overkill for a
        // simple 3-line-function.
        //
        // FIXME: Create a Mock of a Serializer, once we can easily spy on them.
        //
        // [1] https://serde-rs.github.io/serde/serde/serde/ser/trait.Serializer.html
        let serialized_json = serde_json::to_string(&service).unwrap();
        assert_eq!(serialized_json, "{\"id\":\"1\",\"name\":\"dummy service\",\"description\":\"\
        really nothing to see\",\"http_url\":\"2\",\"ws_url\":\"3\"}");
    }
}
