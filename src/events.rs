/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate mio;

use service::ServiceID;
pub use ws::Message; // Needs to be pub to be accessible from the tests.

pub enum EventData {
    AdapterStart { name: String },
    ServiceStart { id: ServiceID },
    ServiceStop { id: ServiceID },
    Notification { message: Message }
}

impl EventData {
    pub fn description(&self) -> String {
        match *self {
            EventData::AdapterStart { ref name } => name.to_owned(),
            EventData::ServiceStart { ref id }
            | EventData::ServiceStop { ref id } => id.to_owned(),
            EventData::Notification { ref message } => {
                match *message {
                    Message::Text(ref msg) => msg.to_owned(),
                    Message::Binary(_) => "<binary blob>".to_owned()
                }
            }
        }
    }

    pub fn as_ws_message(&self) -> Option<Message> {
        // Beware, ugly unsafe json serialization ahead.
        match *self {
            EventData::AdapterStart { ref name } =>
                Some(Message::text(format!("{{ \"type\": \"core/adapter/start\", \"name\": \"{}\"}}", name))),
            EventData::ServiceStart { ref id } =>
                Some(Message::text(format!("{{ \"type\": \"core/service/start\", \"id\": \"{}\"}}", id))),
            EventData::ServiceStop { ref id } =>
                Some(Message::text(format!("{{ \"type\": \"core/service/stop\", \"id\": \"{}\"}}", id))),
            EventData::Notification { ref message } => None
        }
    }
}

pub type EventSender = mio::Sender<EventData>;

#[cfg(test)]
describe! event_data {
    it "AdapterStart should return its name as a description" {
        let data = EventData::AdapterStart { name: "name".to_owned() };
        assert_eq!(data.description(), "name");
    }

    it "ServiceStart should return its ID as a description" {
        let data = EventData::ServiceStart { id: "id".to_owned() };
        assert_eq!(data.description(), "id");
    }

    // TODO Factorize this test with the one above once there's a way to loop over a random emum.
    // https://github.com/rust-lang/rfcs/issues/284
    it "ServiceStop should return its ID as a description" {
        let data = EventData::ServiceStop { id: "id".to_owned() };
        assert_eq!(data.description(), "id");
    }

    it "A textual notification should return the text as a description" {
        let data = EventData::Notification { message: Message::text("Hello World!") };
        assert_eq!(data.description(), "Hello World!");
    }

    it "A binary notification should return '<binary blob> as a description" {
        let data = EventData::Notification { message: Message::binary(vec![1, 2]) };
        assert_eq!(data.description(), "<binary blob>");
    }
}
