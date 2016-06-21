/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

 /// Enum to describe all the messages exchanged by the jsworker system.

use serde::{ Serialize, Serializer };
use std::cell::Cell;
use std::sync::mpsc::Sender;
use ws::Sender as WsSender;

pub type Url = String; // FIXME: should be the url type from hyper.
pub type User = String;   // FIXME: should be the user type from foxbox_users.

/// An enum representing a worker state.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum WorkerState {
    Stopped,
    Running,
    Hibernating, // Used to track workers that should run but wait for a wake up call.
}

impl WorkerState {
    pub fn as_int(&self) -> u32 {
        match *self {
            WorkerState::Stopped => 0,
            WorkerState::Running => 1,
            WorkerState::Hibernating => 2,
        }
    }

    pub fn from_int(value: u32) -> Self {
        match value {
            0 => WorkerState::Stopped,
            1 => WorkerState::Running,
            2 => WorkerState::Hibernating,
            _ => { panic!("Invalid value: {}", value); }
        }
    }
}

impl Serialize for WorkerState {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
        where S: Serializer
    {
        match *self {
            WorkerState::Stopped => serializer.serialize_str("Stopped"),
            WorkerState::Running => serializer.serialize_str("Running"),
            WorkerState::Hibernating => serializer.serialize_str("Hibernating"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkerKind {
    Web,
    Service,
}

impl WorkerKind {
    fn to_string(&self) -> String {
        match *self {
            WorkerKind::Web => String::from("WebWorker"),
            WorkerKind::Service => String::from("ServiceWorker"),
        }
    }

    pub fn as_int(&self) -> u32 {
        match *self {
            WorkerKind::Web => 0,
            WorkerKind::Service => 1,
        }
    }

    pub fn from_int(value: u32) -> Self {
        match value {
            0 => WorkerKind::Web,
            1 => WorkerKind::Service,
            _ => { panic!("Invalid value: {}", value); }
        }
    }
}

impl Serialize for WorkerKind {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
        where S: Serializer
    {
        match *self {
            WorkerKind::Web => serializer.serialize_str("WebWorker"),
            WorkerKind::Service => serializer.serialize_str("ServiceWorker"),
        }
    }
}

/// A Worker representation, that we'll keep synchronized with the runtime.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkerInfo {
    pub url: Url,
    pub user: User,
    pub kind: WorkerKind,
    pub state: Cell<WorkerState>,
}

pub type WorkerInfoKey = String;

impl Serialize for WorkerInfo {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
        where S: Serializer
    {
        #[derive(Serialize)]
        struct SerializableInfo {
            url: Url,
            user: User,
            state: WorkerState,
            kind: WorkerKind,
            id: WorkerInfoKey,
        }
        let info = SerializableInfo {
            url: self.url.clone(),
            user: self.user.clone(),
            state: self.state.get(),
            kind: self.kind.clone(),
            id: self.key(),
        };
        info.serialize(serializer)
    }
}

impl WorkerInfo {
    pub fn new(user: User, url: Url, kind: WorkerKind, initial_state: WorkerState) -> Self {
        WorkerInfo {
            url: url,
            user: user,
            kind: kind,
            state: Cell::new(initial_state),
        }
    }

    pub fn new_webworker(user: User, url: Url, initial_state: WorkerState) -> Self {
        WorkerInfo::new(user, url, WorkerKind::Web, initial_state)
    }

    pub fn new_serviceworker(user: User, url: Url, initial_state: WorkerState) -> Self {
        WorkerInfo::new(user, url, WorkerKind::Service, initial_state)
    }

    /// Creates a unique key for this WorkerInfo.
    pub fn key(&self) -> WorkerInfoKey {
        WorkerInfo::key_from(self.user.clone(), &self.url, &self.kind)
    }

    pub fn key_from(user: User, url: &str, kind: &WorkerKind) -> WorkerInfoKey {
        use std::hash::{Hash, Hasher, SipHasher};

        let mut hasher = SipHasher::new();
        url.hash(&mut hasher);
        user.hash(&mut hasher);
        kind.to_string().hash(&mut hasher);
        format!("{}", hasher.finish())
    }
}

#[derive(Clone, Debug, Serialize)]
pub enum BrowserMessageKind {
    Message,
    Error,
}

impl From<BrowserMessageKind> for String {
    fn from(s: BrowserMessageKind) -> String {
        match s {
            BrowserMessageKind::Message => String::from("message"),
            BrowserMessageKind::Error => String::from("error"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub enum Message {
    // Notifies that we have a browser connection closing. WebSocket -> Runtime
    BrowserWSClosed {
        worker_id: WorkerInfoKey,
        handler_id: String,
    },
    // Notifies that we have a browser connection established. WebSocket -> Runtime
    BrowserWSOpened {
        #[serde(skip_serializing)]
        out: WsSender,
        worker_id: WorkerInfoKey,
        handler_id: String,
    },
    // Result value when starting a worker, giving the url of the ws used.
    ClientEndpoint {
        ws_url: String,
    },
    // Get the list of all workers for a user. Router -> Runtime.
    GetList {
        user: User,
        #[serde(skip_serializing)]
        tx: Sender<Message>,
    },
    // Result value for GetList. Runtime -> Router
    List {
        list: Vec<WorkerInfo>,
    },
    // Notifies that we have a js runner connection established. WebSocket -> Runtime
    RunnerWSOpened {
        #[serde(skip_serializing)]
        out: WsSender,
    },
    // Sends a payload to the browser for a given worker.
    SendToBrowser {
        data: Vec<u8>,
        id: WorkerInfoKey,
        kind: BrowserMessageKind,
    },
    // Sends a payload to the runtime for a given worker.
    SendToRuntime {
        data: Vec<u8>,
        id: WorkerInfoKey,
    },
    // Notifies that the foxbox is shutting down. Broadcasted by the main controller.
    Shutdown,
    // Start a worker. Router -> Runtime.
    Start {
        worker: WorkerInfo,
        host: String,
        #[serde(skip_serializing)]
        tx: Sender<Message>,
    },
    // Stop a worker. Router -> Runtime.
    Stop {
        worker: WorkerInfo,
    },
    // Notifies that we need to set all workers in the `stopped` state. WebSocket -> Runtime
    StopAll,
    // Wake up a worker. Worker DB -> Runtime.
    Wakeup {
        worker: WorkerInfo,
    },
}
