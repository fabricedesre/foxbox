// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use workers::{Url, User, WorkerInfo};

use serde::{Serialize, Serializer};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::sync::mpsc::Sender;

#[derive(Clone, Debug)]
pub enum Message {
    Start {
        url: Url,
        user: User,
        tx: Sender<Message>,
    },
    Stop {
        url: Url,
        user: User,
        tx: Sender<Message>,
    },
    GetList {
        user: User,
        tx: Sender<Message>,
    },
    List {
        list: Vec<WorkerInfo>,
    },
    StopAll,
    Shutdown,
}

impl Serialize for Message {
    // TODO: serialize propertly not just List.
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error> where S: Serializer {
        match *self {
            Message::Start { ref url, user, ref tx } => serializer.serialize_str("Start"),
            Message::Stop { ref url, user, ref tx } => serializer.serialize_str("Stop"),
            Message::GetList { user, ref tx } => serializer.serialize_str("GetList"),
            Message::List { ref list } => list.serialize(serializer),
            Message::StopAll => serializer.serialize_str("StopAll"),
            Message::Shutdown => serializer.serialize_str("Shutdown"),
        }
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match *self {
            Message::Start { ref url, user, ref tx } => write!(f, "{}", "Start"),
            Message::Stop { ref url, user, ref tx } => write!(f, "{}", "Stop"),
            Message::GetList { user, ref tx } => write!(f, "{}", "GetList"),
            Message::List { ref list } => write!(f, "{}", "List"),
            Message::StopAll => write!(f, "{}", "StopAll"),
            Message::Shutdown => write!(f, "{}", "Shutdown"),
        }
    }
}
