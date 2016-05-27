// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/// Enum to describe all the messages exchanged by the jsworker system.

use std::sync::mpsc::Sender;
use workers::{User, WorkerInfo};
use ws::Sender as WsSender;

#[derive(Clone, Debug, Serialize)]
pub enum Message {
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
    // Notifies that we have a ws runner connection established. WebSocket -> Runtime
    RunnerWS {
        #[serde(skip_serializing)]
        out: WsSender,
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
