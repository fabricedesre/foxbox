/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Stores subscription information for `WebPush`.
//!
//! # The `WebPush` database
//!
//! The "subscriptions" table stores the `WebPush` subscription metadata
//! necessary to encrypt a message (ECDH public key) and to send said
//! message to a push service (push URI). The user is identified via
//! an ID shared with the Users database. Each user may have any number
//! of active subscriptions.
//!
//! The "resources" table stores the resources that the user is watching
//! in order to receive notifications. Adapters may publish a message
//! to a given resource and all users watching that resource will be
//! issued a push notification on each of their subscriptions.
//!

use foxbox_taxonomy::api::User;
use super::Subscription;
use libc::c_int;
use rusqlite::{ self, Connection };

fn escape(string: &str) -> String {
    // http://www.sqlite.org/faq.html#q14
    string.replace("'", "''")
}

fn escape_option(opt: &Option<String>) -> Option<String> {
    match *opt {
        Some(ref x) => { return Some(escape(x)); },
        None => { return None; },
    };
}

fn user_to_str(user: &User) -> String {
    match *user {
        User::Id(ref user)  => user.clone(),
        User::None          => String::from("")
    }
}

pub struct WebPushDb {
    db: Connection,
}

impl WebPushDb {
    /// Opens the database at `path` and creates it if not available yet.
    pub fn new(path: &str) -> Self {
        let db = Connection::open(path).unwrap();
        db.execute("CREATE TABLE IF NOT EXISTS subscriptions (
                    user_id     TEXT,
                    push_uri    TEXT NOT NULL UNIQUE,
                    public_key  TEXT NOT NULL,
                    auth        TEXT
            )", &[]).unwrap();

        db.execute("CREATE TABLE IF NOT EXISTS resources (
                    user_id     TEXT,
                    resource    TEXT NOT NULL
            )", &[]).unwrap();

        WebPushDb {
            db: db
        }
    }

    /// Adds a new push subscription `sub` bound to the user `user_id`.
    pub fn subscribe(&self, user_id: &User, sub: &Subscription) -> rusqlite::Result<c_int> {
        self.db.execute("INSERT INTO subscriptions VALUES ($1, $2, $3, $4)",
                        &[&escape(&user_to_str(user_id)), &escape(&sub.push_uri), &escape(&sub.public_key), &escape_option(&sub.auth)]
        )
    }

    /// Removes an existing push subscription identified by `push_uri`.
    pub fn unsubscribe(&self, _: &User, push_uri: &str) -> rusqlite::Result<c_int> {
        self.db.execute("DELETE FROM subscriptions WHERE push_uri=$1",
                        &[&escape(push_uri)]
        )
    }

    /// Sets the resources to subscribe to notifications for the user `user_id`.
    pub fn set_resources(&self, user_id: &User, resources: &[String]) -> rusqlite::Result<()> {
        try!(self.db.execute("DELETE FROM resources WHERE user_id=$1",
                             &[&user_to_str(user_id)]));
        for resource in resources.iter() {
            try!(self.db.execute("INSERT INTO resources VALUES ($1, $2)",
                                 &[&escape(&user_to_str(user_id)), &escape(resource)]
            ));
        }
        Ok(())
    }

    /// Gets the resources subscribed to by the user `user_id`.
    pub fn get_resources(&self, user_id: &User) -> rusqlite::Result<Vec<String>> {
        let mut subs = Vec::new();
        let mut stmt = try!(self.db.prepare("SELECT resource FROM resources WHERE user_id=$1"));
        let rows = try!(stmt.query(&[&user_to_str(user_id)]));
        let (count, _) = rows.size_hint();
        subs.reserve_exact(count);
        for result_row in rows {
            let row = try!(result_row);
            subs.push(row.get(0));
        }
        Ok(subs)
    }

    /// Gets the push subscriptions for the user `user_id`.
    pub fn get_subscriptions(&self, user_id: &User) -> rusqlite::Result<Vec<Subscription>> {
        let mut subs = Vec::new();
        let mut stmt = try!(self.db.prepare("SELECT push_uri, public_key, auth FROM subscriptions WHERE user_id=$1"));
        let rows = try!(stmt.query(&[&user_to_str(user_id)]));
        let (count, _) = rows.size_hint();
        subs.reserve_exact(count);
        for result_row in rows {
            let row = try!(result_row);
            subs.push(Subscription {
                push_uri: row.get(0),
                public_key: row.get(1),
                auth: row.get(2)
            });
        }
        Ok(subs)
    }

    /// Gets the push subscriptions for users who are subscribed to `resource` notifications.
    pub fn get_resource_subscriptions(&self, resource: &str) -> rusqlite::Result<Vec<Subscription>> {
        let mut subs = Vec::new();
        let mut stmt = try!(self.db.prepare("SELECT push_uri, public_key, auth FROM subscriptions WHERE
                                             user_id IN (SELECT user_id FROM resources WHERE resource=$1)"));
        let rows = try!(stmt.query(&[&escape(resource)]));
        let (count, _) = rows.size_hint();
        subs.reserve_exact(count);
        for result_row in rows {
            let row = try!(result_row);
            subs.push(Subscription {
                push_uri: row.get(0),
                public_key: row.get(1),
                auth: row.get(2)
            });
        }
        Ok(subs)
    }
}

#[cfg(test)]
pub fn get_db_environment() -> String {
    use libc::getpid;
    use std::thread;
    let tid = format!("{:?}", thread::current());
    format!("./webpush_db_test-{}-{}.sqlite", unsafe { getpid() }, tid.replace("/", "42"))
}

#[cfg(test)]
pub fn remove_test_db() {
    use std::path::Path;
    use std::fs;

    let dbfile = get_db_environment();
    match fs::remove_file(Path::new(&dbfile)) {
        Err(e) => panic!("Error {} cleaning up {}", e, dbfile),
        _ => assert!(true)
    }
}

#[cfg(test)]
describe! tests {
    before_each {
        use foxbox_taxonomy::api::User;
        let db = WebPushDb::new(&get_db_environment());
    }

    it "should manage subscription correctly" {
        use super::super::Subscription;

        let subs0 = db.get_subscriptions(&User::Id(String::from("1"))).unwrap();
        assert_eq!(subs0.len(), 0);

        let sub = Subscription {
            push_uri: "test_push_uri".to_owned(),
            public_key: "test_public_key".to_owned(),
            auth: Some("test_auth".to_owned())
        };
        db.subscribe(&User::Id(String::from("1")), &sub).unwrap();

        let subs1 = db.get_subscriptions(&User::Id(String::from("1"))).unwrap();
        assert_eq!(subs1.len(), 1);
        assert_eq!(subs1[0], sub);

        db.unsubscribe(&User::Id(String::from("1")), &sub.push_uri).unwrap();

        let subs2 = db.get_subscriptions(&User::Id(String::from("1"))).unwrap();
        assert_eq!(subs2.len(), 0);
    }

    it "should manage resources correctly" {
        let res0 = db.get_resources(&User::Id(String::from("1"))).unwrap();
        assert_eq!(res0.len(), 0);

        let res = vec!["resource1".to_owned(), "resource2".to_owned()];
        db.set_resources(&User::Id(String::from("1")), &res).unwrap();

        let res1 = db.get_resources(&User::Id(String::from("1"))).unwrap();
        assert_eq!(res1.len(), 2);
        assert_eq!(res1[0], "resource1".to_owned());
        assert_eq!(res1[1], "resource2".to_owned());

        db.set_resources(&User::Id(String::from("1")), &[]).unwrap();

        let res2 = db.get_resources(&User::Id(String::from("1"))).unwrap();
        assert_eq!(res2.len(), 0);
    }

    it "should yield subscriptions given a resource" {
        use super::super::Subscription;

        db.subscribe(&User::Id(String::from("1")), &Subscription {
            push_uri: "u1_sub0_puri".to_owned(),
            public_key: "u1_sub0_pkey".to_owned(),
            auth: Some("u1_sub0_auth".to_owned())
        }).unwrap();
        db.subscribe(&User::Id(String::from("1")), &Subscription {
            push_uri: "u1_sub1_puri".to_owned(),
            public_key: "u1_sub1_pkey".to_owned(),
            auth: None
        }).unwrap();
        db.subscribe(&User::Id(String::from("2")), &Subscription {
            push_uri: "u2_sub0_puri".to_owned(),
            public_key: "u2_sub0_pkey".to_owned(),
            auth: Some("u2_sub0_auth".to_owned())
        }).unwrap();
        let u3_sub0 = Subscription {
            push_uri: "u3_sub0_puri".to_owned(),
            public_key: "u3_sub0_pkey".to_owned(),
            auth: Some("u3_sub0_auth".to_owned())
        };
        db.subscribe(&User::Id(String::from("3")), &u3_sub0).unwrap();

        db.set_resources(&User::Id(String::from("1")), &["res1".to_owned()]).unwrap();
        db.set_resources(&User::Id(String::from("2")), &["res1".to_owned(), "res2".to_owned()]).unwrap();
        db.set_resources(&User::Id(String::from("3")), &["res2".to_owned(), "res3".to_owned()]).unwrap();

        let subs1 = db.get_resource_subscriptions("res1").unwrap();
        assert_eq!(subs1.len(), 3);

        let subs2 = db.get_resource_subscriptions("res2").unwrap();
        assert_eq!(subs2.len(), 2);

        let subs3 = db.get_resource_subscriptions("res3").unwrap();
        assert_eq!(subs3.len(), 1);
        assert_eq!(subs3[0], u3_sub0);

        let subs4 = db.get_resource_subscriptions("res4").unwrap();
        assert_eq!(subs4.len(), 0);
    }

    after_each {
        remove_test_db();
    }
}
