// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use broker::SharedBroker;
use message::Message;

use rusqlite::Connection;
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::path::PathBuf;

fn escape<T>(string: &str) -> String {
    // http://www.sqlite.org/faq.html#q14
    format!("{}", string).replace("'", "''")
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

/// The entire set of workers.
pub struct JsWorkers {
    db: Option<Connection>,
    workers: HashMap<String, WorkerInfo>, // The key is a WorkerInfo key.
    broker: SharedBroker<Message>,
}

impl JsWorkers {
    pub fn new(config_root: &str, broker: &SharedBroker<Message>) -> Self {
        // TODO: Read the current set of workers from disk, creating the DB if it doesn't exist yet.

        JsWorkers {
            workers: HashMap::new(),
            db: None,
            broker: broker.clone(),
        }
    }

    pub fn has_worker(&self, user: User, url: Url) -> bool {
        self.workers.contains_key(&WorkerInfo::key_from(&url, user))
    }

    /// Returns the current info for this worker.
    /// Note that this is a live value.
    pub fn get_worker_info(&self, user: User, url: Url) -> Option<&WorkerInfo> {
        self.workers.get(&WorkerInfo::key_from(&url, user))
    }

    pub fn get_workers_for(&mut self, user: User) -> Vec<WorkerInfo> {
        let mut res = Vec::new();
        let ref w = self.workers;
        for (_, info) in w {
            if info.user == user {
                res.push(info.clone());
            }
        }
        res
    }

    pub fn stop_all(&self) {
        let ref w = self.workers;
        for (_, info) in w {
            info.state.set(WorkerState::Stopped);
        }
    }

    /// TODO: improve error case.
    pub fn add_worker(&mut self, user: User, url: Url) -> Result<(), ()> {
        if self.has_worker(user, url.clone()) {
            return Err(());
        }

        let info = WorkerInfo::new(url, user, WorkerState::Stopped);
        self.workers.insert(info.key(), info);
        Ok(())
    }

    /// TODO: improve error case.
    pub fn remove_worker(&mut self, user: User, url: Url) -> Result<(), ()> {
        if !self.has_worker(user, url.clone()) {
            return Err(());
        }

        let key = WorkerInfo::key_from(&url, user);
        self.workers.remove(&key);
        Ok(())
    }

    /// TODO: improve error case.
    pub fn stop_worker(&self, user: User, url: Url) -> Result<(), ()> {
        if let Some(worker_info) = self.get_worker_info(user, url.clone()) {
            if worker_info.state.get() == WorkerState::Stopped {
                return Err(());
            }
            // TODO: call something to actually stop the worker.

            // Mark the worker as stopped.
            worker_info.state.set(WorkerState::Stopped);

            return Ok(());
        } else {
            return Err(());
        }
    }

    /// TODO: improve error case.
    pub fn start_worker(&self, user: User, url: Url) -> Result<(), ()> {
        if let Some(worker_info) = self.get_worker_info(user, url.clone()) {
            if worker_info.state.get() == WorkerState::Running {
                return Err(());
            }

            // TODO: call something to actually start the worker.

            // Mark the worker as running.
            worker_info.state.set(WorkerState::Running);

            return Ok(());
        } else {
            return Err(());
        }
    }

    pub fn count(&self) -> usize {
        self.workers.len()
    }
}

#[test]
fn test_workers() {
    use broker::MessageBroker;
    use serde_json;

    let mut list = JsWorkers::new("", &MessageBroker::new_shared());
    let url = "http://example.com/worker.js".to_owned();
    let url2 = "http://example.com/worker2.js".to_owned();
    let user1: User = 0;
    let user2: User = 1;
    let user3: User = 2;

    // Start with an empty set.
    assert_eq!(list.count(), 0);

    {
        // Add one worker.
        list.add_worker(user1, url.clone()).unwrap();
        assert_eq!(list.count(), 1);

        // Check that we don't get anything for another user.
        assert_eq!(list.get_worker_info(user2, url.clone()), None);

        // Get the worker info back and check state.
        let info = list.get_worker_info(user1, url.clone()).unwrap();
        assert_eq!(info.state.get(), WorkerState::Stopped);

        // Start the worker and check the new state.
        list.start_worker(user1, url.clone()).unwrap();
        assert_eq!(info.state.get(), WorkerState::Running);

        // Check error when starting an already running worker.
        assert_eq!(list.start_worker(user1, url.clone()).is_err(), true);
        assert_eq!(info.state.get(), WorkerState::Running);

        // Stop the worker an check state.
        list.stop_worker(user1, url.clone()).unwrap();
        assert_eq!(info.state.get(), WorkerState::Stopped);

        // Check error when stopping an already stopped worker.
        assert_eq!(list.stop_worker(user1, url.clone()).is_err(), true);
        assert_eq!(info.state.get(), WorkerState::Stopped);
    }

    {
        // Start the worker again and check that stop_all works.
        list.start_worker(user1, url.clone()).unwrap();
        let info = list.get_worker_info(user1, url.clone()).unwrap();
        assert_eq!(info.state.get(), WorkerState::Running);
        list.stop_all();
        assert_eq!(info.state.get(), WorkerState::Stopped);
    }

    // Check errors when starting or stopping a non-existant worker.
    assert_eq!(list.start_worker(user3, url.clone()).is_err(), true);
    assert_eq!(list.stop_worker(user3, url.clone()).is_err(), true);

    // Check error when removing a non-existant worker.
    assert_eq!(list.remove_worker(user2, url.clone()).is_err(), true);
    assert_eq!(list.count(), 1);

    // Remove the worker we actually added.
    list.remove_worker(user1, url.clone()).unwrap();
    assert_eq!(list.count(), 0);

    // Check that we can add several workers.
    list.add_worker(user1, url.clone()).unwrap();
    assert_eq!(list.count(), 1);
    list.add_worker(user2, url.clone()).unwrap();
    assert_eq!(list.count(), 2);
    list.add_worker(user3, url.clone()).unwrap();
    assert_eq!(list.count(), 3);
    list.add_worker(user1, url2.clone()).unwrap();
    assert_eq!(list.count(), 4);

    // Check the workers per user list.
    let mut all = list.get_workers_for(user1);
    assert_eq!(all.len(), 2);

    all = list.get_workers_for(user2);
    let serialized = serde_json::to_string(&all).unwrap();
    assert_eq!(serialized,
               r#"[{"url":"http://example.com/worker.js","user":1,"state":"Stopped","id":"15634489503557940443"}]"#);

    // ... and remove them, out of order.
    list.remove_worker(user2, url.clone()).unwrap();
    assert_eq!(list.count(), 3);
    list.remove_worker(user1, url.clone()).unwrap();
    assert_eq!(list.count(), 2);
    list.remove_worker(user3, url.clone()).unwrap();
    assert_eq!(list.count(), 1);
    list.remove_worker(user1, url2.clone()).unwrap();
    assert_eq!(list.count(), 0);
}
