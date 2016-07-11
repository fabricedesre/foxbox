// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use foxbox_core::broker::SharedBroker;
use foxbox_core::jsworkers::{Message, Url, User, WorkerInfo, WorkerInfoKey, WorkerKind, WorkerState};

use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::mpsc::channel;

fn escape(string: &str) -> String {
    // http://www.sqlite.org/faq.html#q14
    format!("{}", string).replace("'", "''")
}

/// The entire set of workers.
pub struct JsWorkers {
    db: Connection,
    workers: HashMap<WorkerInfoKey, WorkerInfo>,
    broker: SharedBroker<Message>,
}

impl JsWorkers {
    pub fn new(config_root: &str, broker: &SharedBroker<Message>) -> Self {
        // Open the database.
        let db = Connection::open(format!("{}/jsworkers.sqlite", config_root)).unwrap_or_else(|err| {
            panic!("Unable to open jsworkers database: {}", err);
        });

        // Create the table if if doesn't exist yet.
        db.execute("CREATE TABLE IF NOT EXISTS workers (
                key    TEXT NOT NULL PRIMARY KEY,
                url    TEXT NOT NULL,
                user   TEXT NOT NULL,
                kind   INTEGER,
                state  INTEGER
        )", &[]).unwrap_or_else(|err| {
            panic!("Unable to create jsworkers database: {}", err);
        });

        // Read the list of stored workers.
        let mut workers = HashMap::new();
        {
            let mut stmt = db.prepare("SELECT key, url, user, kind, state FROM workers").unwrap();
            let rows = stmt.query(&[]).unwrap();
            for result_row in rows {
                let row = result_row.unwrap();
                let key: WorkerInfoKey = row.get(0);
                let url: Url = row.get(1);
                let user: User = row.get(2);
                let sql_kind: i64 = row.get(3);
                let sql_state: i64 = row.get(4);
                let mut state: WorkerState = WorkerState::from_int(sql_state as u32);
                // Put running workers in hibernation. They will wake up when they are woken up
                // by start_all_workers() or start_worker().
                if state == WorkerState::Running {
                    state = WorkerState::Hibernating;
                }
                workers.insert(key, WorkerInfo::new(user,
                                                    url,
                                                    WorkerKind::from_int(sql_kind as u32),
                                                    state));
            }
        }
        info!("Loaded {} workers from the database.", workers.len());

        JsWorkers {
            workers: workers,
            db: db,
            broker: broker.clone(),
        }
    }

    // Updates a worker in the database. The only field that can actually change is the
    // state, so we don't update the other fields.
    fn update_worker_in_db(&self, info: &WorkerInfo) {
        info!("update_worker_in_db {:?}", info);
        let sql_state: i64 = info.state.get().as_int() as i64;
        self.db.execute("UPDATE workers SET state = $1 WHERE key = $2",
                        &[&sql_state, &escape(&info.key())]).unwrap();

    }

    fn add_worker_in_db(&self, info: &WorkerInfo) {
        info!("add_worker_in_db {:?}", info);
        let sql_kind: i64 = info.kind.as_int() as i64;
        let sql_state: i64 = info.state.get().as_int() as i64;
        self.db.execute("INSERT OR IGNORE INTO workers (key, url, user, kind, state) VALUES ($1, $2, $3, $4, $5)",
                        &[&escape(&info.key()),
                          &escape(&info.url),
                          &escape(&info.user),
                          &sql_kind,
                          &sql_state]).unwrap();
    }

    fn remove_worker_from_db(&self, info: &WorkerInfo) {
        info!("remove_worker_from_db {:?}", info);
        self.db.execute("DELETE from workers where key = $1", &[&escape(&info.key())]).unwrap();
    }

    pub fn has_worker(&self, info: &WorkerInfo) -> bool {
        self.workers.contains_key(&info.key())
    }

    /// Returns the current info for this worker.
    /// Note that this is a live value.
    pub fn get_worker_info(&self, user: User, url: Url, kind: WorkerKind) -> Option<&WorkerInfo> {
        self.workers.get(&WorkerInfo::key_from(user, &url, &kind))
    }

    pub fn get_workers_for(&mut self, user: &User) -> Vec<WorkerInfo> {
        let mut res = Vec::new();
        let ref w = self.workers;
        for (_, info) in w {
            if info.user == *user {
                res.push(info.clone());
            }
        }
        res
    }

    pub fn stop_all(&self) {
        let ref w = self.workers;
        for (_, info) in w {
            info.state.set(WorkerState::Stopped);
            self.update_worker_in_db(info);
        }
    }

    /// TODO: improve error case.
    pub fn add_worker(&mut self, info: &WorkerInfo) -> Result<(), ()> {
        info!("add_worker {:?}", info);
        if self.has_worker(info) {
            error!("already in our worker list.");
            return Err(());
        }

        let new_worker = info.clone();
        new_worker.state.set(WorkerState::Stopped);
        self.add_worker_in_db(&new_worker);
        self.workers.insert(new_worker.key(), new_worker);
        Ok(())
    }

    /// TODO: improve error case.
    pub fn remove_worker(&mut self, info: &WorkerInfo) -> Result<(), ()> {
        if !self.has_worker(info) {
            return Err(());
        }

        self.remove_worker_from_db(&info);
        self.workers.remove(&info.key());
        Ok(())
    }

    /// TODO: improve error case.
    pub fn stop_worker(&self, info: &WorkerInfo) -> Result<(), ()> {
        if let Some(worker_info) = self.get_worker_info(info.user.clone(),
                                                        info.url.clone(),
                                                        info.kind.clone()) {
            if worker_info.state.get() == WorkerState::Stopped {
                return Err(());
            }

            // Mark the worker as stopped.
            worker_info.state.set(WorkerState::Stopped);
            self.update_worker_in_db(worker_info);

            return Ok(());
        } else {
            return Err(());
        }
    }

    /// TODO: improve error case.
    pub fn start_worker(&self, info: &WorkerInfo) -> Result<(), ()> {
        info!("start_worker {:?}", info);
        if let Some(worker_info) = self.get_worker_info(info.user.clone(),
                                                        info.url.clone(),
                                                        info.kind.clone()) {
            if worker_info.state.get() == WorkerState::Running {
                return Err(());
            }

            // Mark the worker as running.
            worker_info.state.set(WorkerState::Running);
            self.update_worker_in_db(worker_info);

            return Ok(());
        } else {
            return Err(());
        }
    }

    pub fn wake_up_workers(&self) {
        info!("wake_up_workers");
        let ref w = self.workers;
        for (_, info) in w {
            if info.state.get() == WorkerState::Hibernating {
                info!("Waking up worker {:?}", info);
                let message = Message::Wakeup {
                    worker: info.clone(),
                };

                self.broker.lock().unwrap().send_message("workers", message);
            }
        }
    }

    pub fn count(&self) -> usize {
        self.workers.len()
    }
}

#[test]
fn test_workers() {
    use foxbox_core::broker::MessageBroker;
    use serde_json;

    let mut list = JsWorkers::new("", &MessageBroker::new_shared());
    let url = "http://example.com/worker.js".to_owned();
    let url2 = "http://example.com/worker2.js".to_owned();
    let user1: User = String::from("User0");
    let user2: User = String::from("User1");
    let user3: User = String::from("User2");

    let info1 = WorkerInfo::default(user1.clone(), url.clone());
    let info1_2 = WorkerInfo::default(user1.clone(), url2.clone());
    let info2 = WorkerInfo::default(user2.clone(), url.clone());
    let info3 = WorkerInfo::default(user3.clone(), url.clone());

    // Start with an empty set.
    assert_eq!(list.count(), 0);

    {
        // Add one worker.
        list.add_worker(&info1).unwrap();
        assert_eq!(list.count(), 1);

        // Check that we don't get anything for another user.
        assert_eq!(list.get_worker_info(user2.clone(), url.clone()), None);

        // Get the worker info back and check state.
        let info = list.get_worker_info(user1.clone(), url.clone()).unwrap();
        assert_eq!(info.state.get(), WorkerState::Stopped);

        // Start the worker and check the new state.
        list.start_worker(&info1).unwrap();
        assert_eq!(info.state.get(), WorkerState::Running);

        // Check error when starting an already running worker.
        assert_eq!(list.start_worker(&info1).is_err(), true);
        assert_eq!(info.state.get(), WorkerState::Running);

        // Stop the worker an check state.
        list.stop_worker(&info1).unwrap();
        assert_eq!(info.state.get(), WorkerState::Stopped);

        // Check error when stopping an already stopped worker.
        assert_eq!(list.stop_worker(&info1).is_err(), true);
        assert_eq!(info.state.get(), WorkerState::Stopped);
    }

    {
        // Start the worker again and check that stop_all works.
        list.start_worker(&info1).unwrap();
        let info = list.get_worker_info(user1.clone(), url.clone()).unwrap();
        assert_eq!(info.state.get(), WorkerState::Running);
        list.stop_all();
        assert_eq!(info.state.get(), WorkerState::Stopped);
    }

    // Check errors when starting or stopping a non-existant worker.
    assert_eq!(list.start_worker(&info3).is_err(), true);
    assert_eq!(list.stop_worker(&info3).is_err(), true);

    // Check error when removing a non-existant worker.
    assert_eq!(list.remove_worker(&info2).is_err(), true);
    assert_eq!(list.count(), 1);

    // Remove the worker we actually added.
    list.remove_worker(&info1).unwrap();
    assert_eq!(list.count(), 0);

    // Check that we can add several workers.
    list.add_worker(&info1).unwrap();
    assert_eq!(list.count(), 1);
    list.add_worker(&info2).unwrap();
    assert_eq!(list.count(), 2);
    list.add_worker(&info3).unwrap();
    assert_eq!(list.count(), 3);
    list.add_worker(&info1_2).unwrap();
    assert_eq!(list.count(), 4);

    // Check the workers per user list.
    let mut all = list.get_workers_for(&user1);
    assert_eq!(all.len(), 2);

    all = list.get_workers_for(&user2);
    let serialized = serde_json::to_string(&all).unwrap();
    assert_eq!(serialized,
               r#"[{"url":"http://example.com/worker.js","user":"User1","state":"Stopped","id":"6030026276773233015"}]"#);

    // ... and remove them, out of order.
    list.remove_worker(&info2).unwrap();
    assert_eq!(list.count(), 3);
    list.remove_worker(&info1).unwrap();
    assert_eq!(list.count(), 2);
    list.remove_worker(&info3).unwrap();
    assert_eq!(list.count(), 1);
    list.remove_worker(&info1_2).unwrap();
    assert_eq!(list.count(), 0);
}