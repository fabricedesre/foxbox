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
}

impl Serialize for WorkerState {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
        where S: Serializer
    {
        match *self {
            WorkerState::Stopped => serializer.serialize_str("Stopped"),
            WorkerState::Running => serializer.serialize_str("Running"),
        }
    }
}

/// A Worker representation, that we'll keep synchronized with the runtime.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkerInfo {
    pub url: Url,
    pub user: User,
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
            id: WorkerInfoKey,
        }
        let info = SerializableInfo {
            url: self.url.clone(),
            user: self.user.clone(),
            state: self.state.get(),
            id: self.key(),
        };
        info.serialize(serializer)
    }
}

impl WorkerInfo {
    pub fn new(user: User, url: Url, initial_state: WorkerState) -> Self {
        WorkerInfo {
            url: url,
            user: user,
            state: Cell::new(initial_state),
        }
    }

    pub fn default(user: User, url: Url) -> Self {
        WorkerInfo {
            url: url,
            user: user,
            state: Cell::new(WorkerState::Stopped),
        }
    }

    /// Creates a unique key for this WorkerInfo.
    pub fn key(&self) -> WorkerInfoKey {
        WorkerInfo::key_from(self.user.clone(), &self.url)
    }

    pub fn key_from(user: User, url: &str) -> WorkerInfoKey {
        use std::hash::{Hash, Hasher, SipHasher};

        let mut hasher = SipHasher::new();
        url.hash(&mut hasher);
        user.hash(&mut hasher);
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
        #[serde(skip_serializing)]
        tx: Sender<Message>,
    },
    // Stop a worker. Router -> Runtime.
    Stop {
        worker: WorkerInfo,
        #[serde(skip_serializing)]
        tx: Sender<Message>,
    },
    // Notifies that we need to set all workers in the `stopped` state. WebSocket -> Runtime
    StopAll,
}
