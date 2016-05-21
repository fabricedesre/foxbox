// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rusqlite::Connection;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use broker::SharedBroker;

fn escape<T>(string: &str) -> String {
    // http://www.sqlite.org/faq.html#q14
    format!("{}", string).replace("'", "''")
}

/// An enum representing a worker state.
#[derive(Copy, Clone, Debug, PartialEq)]
enum WorkerState {
    Stopped,
    Running,
}

type Url = String; // FIXME: should be the url type from hyper.
type User = i32;   // FIXME: should be the user type from foxbox_users.

/// A Worker representation, that we'll keep synchronized with the runtime.
#[derive(Debug, PartialEq)]
pub struct WorkerInfo {
    url: Url,
    user: User,
    state: Cell<WorkerState>,
}

impl WorkerInfo {
    fn new(url: Url, user: User, initial_state: WorkerState) -> Self {
        WorkerInfo {
            url: url,
            user: user,
            state: Cell::new(initial_state),
        }
    }

    /// Creates a unique key for this WorkerInfo.
    fn key(&self) -> String {
        WorkerInfo::key_from(&self.url, self.user)
    }

    fn key_from(url: &Url, user: User) -> String {
        use std::hash::{Hash, Hasher, SipHasher};

        let mut hasher = SipHasher::new();
        url.hash(&mut hasher);
        user.hash(&mut hasher);
        format!("{}", hasher.finish())
    }
}

/// The entire set of workers.
pub struct JsWorkers {
    db: Option<Connection>,
    workers: HashMap<String, WorkerInfo>,
    broker: SharedBroker,
}

impl JsWorkers {
    pub fn new(config_root: &str, broker: &SharedBroker) -> Self {
        // TODO: Read the current set of workers from disk, creating the DB if it doesn't exist yet.

        JsWorkers {
            workers: HashMap::new(),
            db: None,
            broker: broker.clone(),
        }
    }

    pub fn has_worker(&self, url: Url, user: User) -> bool {
        self.workers.contains_key(&WorkerInfo::key_from(&url, user))
    }

    /// Returns the current info for this worker.
    /// Note that this is a live value.
    pub fn get_worker_info(&self, url: Url, user: User) -> Option<&WorkerInfo> {
        self.workers.get(&WorkerInfo::key_from(&url, user))
    }

    /// TODO: improve error case.
    pub fn add_worker(&mut self, url: Url, user: User) -> Result<(), ()> {
        if self.has_worker(url.clone(), user) {
            return Err(());
        }

        let info = WorkerInfo::new(url, user, WorkerState::Stopped);
        self.workers.insert(info.key(), info);
        Ok(())
    }

    /// TODO: improve error case.
    pub fn remove_worker(&mut self, url: Url, user: User) -> Result<(), ()> {
        if !self.has_worker(url.clone(), user) {
            return Err(());
        }

        let key = WorkerInfo::key_from(&url, user);
        self.workers.remove(&key);
        Ok(())
    }

    /// TODO: improve error case.
    pub fn stop_worker(&self, url: Url, user: User) -> Result<(), ()> {
        if let Some(worker_info) = self.get_worker_info(url.clone(), user) {
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
    pub fn start_worker(&self, url: Url, user: User) -> Result<(), ()> {
        if let Some(worker_info) = self.get_worker_info(url.clone(), user) {
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

    let mut list = JsWorkers::new("", &MessageBroker::new_shared());
    let url = "http://example.com/worker.js".to_owned();
    let user1: User = 0;
    let user2: User = 1;
    let user3: User = 2;

    // Start with an empty set.
    assert_eq!(list.count(), 0);

    {
        // Add one worker.
        list.add_worker(url.clone(), user1).unwrap();
        assert_eq!(list.count(), 1);

        // Check that we don't get anything for another user.
        assert_eq!(list.get_worker_info(url.clone(), user2), None);

        // Get the worker info back and check state.
        let info = list.get_worker_info(url.clone(), user1).unwrap();
        assert_eq!(info.state.get(), WorkerState::Stopped);

        // Start the worker and check the new state.
        list.start_worker(url.clone(), user1).unwrap();
        assert_eq!(info.state.get(), WorkerState::Running);

        // Check error when starting an already running worker.
        assert_eq!(list.start_worker(url.clone(), user1).is_err(), true);
        assert_eq!(info.state.get(), WorkerState::Running);

        // Stop the worker an check state.
        list.stop_worker(url.clone(), user1).unwrap();
        assert_eq!(info.state.get(), WorkerState::Stopped);

        // Check error when stopping an already stopped worker.
        assert_eq!(list.stop_worker(url.clone(), user1).is_err(), true);
        assert_eq!(info.state.get(), WorkerState::Stopped);
    }

    // Check errors when starting or stopping a non-existant worker.
    assert_eq!(list.start_worker(url.clone(), user3).is_err(), true);
    assert_eq!(list.stop_worker(url.clone(), user3).is_err(), true);

    // Check error when removing a non-existant worker.
    assert_eq!(list.remove_worker(url.clone(), user2).is_err(), true);
    assert_eq!(list.count(), 1);

    // Remove the worker we actually added.
    list.remove_worker(url.clone(), user1).unwrap();
    assert_eq!(list.count(), 0);

    // Check that we can add several workers.
    list.add_worker(url.clone(), user1).unwrap();
    assert_eq!(list.count(), 1);
    list.add_worker(url.clone(), user2).unwrap();
    assert_eq!(list.count(), 2);
    list.add_worker(url.clone(), user3).unwrap();
    assert_eq!(list.count(), 3);

    // ... and remove them, out of order.
    list.remove_worker(url.clone(), user2).unwrap();
    assert_eq!(list.count(), 2);
    list.remove_worker(url.clone(), user1).unwrap();
    assert_eq!(list.count(), 1);
    list.remove_worker(url.clone(), user3).unwrap();
    assert_eq!(list.count(), 0);
}
