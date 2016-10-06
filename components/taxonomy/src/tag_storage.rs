/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

///! This is the database that holds tags associated to various objects.
///! It provides an api to manage Id <-> tags relationships.
///! All users share the same tags for objects.

use rusqlite::{ Connection, Result };
use std::path::PathBuf;
use util::{ Id, TagId };

fn escape<T>(string: &Id<T>) -> String {
    // http://www.sqlite.org/faq.html#q14
    format!("{}", string).replace("'", "''")
}

/// Creates a unique key for a (id, tag) tuple.
/// `SQlite` integers are i64 so we turn the hashed u64 into a String...
fn create_key<T>(id: &Id<T>, tag: &Id<TagId>) -> String {
    use std::hash::{ Hash, Hasher };
    use std::collections::hash_map::DefaultHasher;

    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    tag.hash(&mut hasher);
    format!("{}", hasher.finish())
}

/// A lighweight struct to manage the database. Creating these objects is very cheap because the
/// underlying database is created lazily when we need it.
pub struct TagStorage {
    db: Option<Connection>,
    path: PathBuf,
}

impl TagStorage {
    pub fn new(path: &PathBuf) -> Self {
        TagStorage {
            db: None,
            path: path.clone()
        }
    }

    // Ensures that we have a database ready. If we fail to open or create the database,
    // this will panic.
    fn ensure_db(&mut self) {
        if self.db.is_some() {
            return;
        }

        debug!("Opening taxonomy tags database at {}", self.path.display());
        let db = Connection::open(self.path.clone()).unwrap_or_else(|err| {
            panic!("Unable to open taxonomy tags database: {}", err);
        });

        db.execute("CREATE TABLE IF NOT EXISTS tags (
                    key    TEXT NOT NULL PRIMARY KEY,
                    id     TEXT NOT NULL,
                    tag    TEXT NOT NULL
            )", &[]).unwrap_or_else(|err| {
                panic!("Unable to create taxonomy tags database: {}", err);
            });

        self.db = Some(db);
    }

    // Debug printing.
    #[allow(dead_code)]
    fn dump(&mut self, msg: &str) {
        let mut stmt = self.db.as_ref().unwrap().prepare("SELECT * FROM tags").unwrap();
        let rows = stmt.query(&[]).unwrap();
        println!("+-----------------------------------------------------------------------------");
        println!("| {}", msg);
        println!("+-----------------------------------------------------------------------------");
        for result_row in rows {
            let row = result_row.unwrap();
            println!("| {} {} {}", row.get::<String>(0), row.get::<String>(1), row.get::<String>(2));
        }
        println!("+-----------------------------------------------------------------------------");
    }

    pub fn add_tag<T>(&mut self, id: &Id<T>, tag: &Id<TagId>) -> Result<()> {
        self.ensure_db();
        try!(self.db.as_ref().unwrap().execute("INSERT OR IGNORE INTO tags VALUES ($1, $2, $3)",
                        &[&create_key(id, tag), &escape(&id), &escape(&tag)]));
        Ok(())
    }

    pub fn add_tags<T>(&mut self, id: &Id<T>, tags: &[Id<TagId>]) -> Result<()> {
        for tag in tags {
            try!(self.add_tag(id, tag));
        }
        //self.dump("add_tags");
        Ok(())
    }

    pub fn remove_tag<T>(&mut self, id: &Id<T>, tag: &Id<TagId>) -> Result<()> {
        self.ensure_db();
        try!(self.db.as_ref().unwrap().execute("DELETE FROM tags WHERE key=$1", &[&create_key(id, tag)]));
        Ok(())
    }

    pub fn remove_tags<T>(&mut self, id: &Id<T>, tags: &[Id<TagId>]) -> Result<()> {
        for tag in tags {
            try!(self.remove_tag(id, tag));
        }
        Ok(())
    }

    pub fn remove_all_tags_for<T>(&mut self, id: &Id<T>) -> Result<()> {
        self.ensure_db();
        try!(self.db.as_ref().unwrap().execute("DELETE FROM tags WHERE id=$1", &[&escape(id)]));
        Ok(())
    }

    pub fn get_tags_for<T>(&mut self, id: &Id<T>) -> Result<Vec<Id<TagId>>> {
        self.ensure_db();
        let mut subs = Vec::new();
        let mut stmt = try!(self.db.as_ref().unwrap().prepare("SELECT tag FROM tags WHERE id=$1"));
        let rows = try!(stmt.query(&[&escape(&id)]));
        let (count, _) = rows.size_hint();
        subs.reserve_exact(count);
        for result_row in rows {
            let row = try!(result_row);
            let s: String = row.get(0);
            subs.push(Id::<TagId>::new(&s));
        }
        Ok(subs)
    }
}

#[cfg(test)]
pub fn get_db_environment() -> PathBuf {
    use libc::getpid;
    use std::thread;
    let tid = format!("{:?}", thread::current()).replace("(", "+").replace(")", "+");
    let s = format!("./tagstore_db_test-{}-{}.sqlite", unsafe { getpid() }, tid.replace("/", "42"));
    PathBuf::from(s)
}

#[cfg(test)]
pub fn remove_test_db() {
    use std::fs;

    let dbfile = get_db_environment();
    match fs::remove_file(dbfile.clone()) {
        Err(e) => panic!("Error {} cleaning up {}", e, dbfile.display()),
        _ => assert!(true)
    }
}

#[test]
fn test_keys() {
    use util::ServiceId;

    let key1 = create_key(&Id::<ServiceId>::new("abc"), &Id::<TagId>::new("defgh"));
    let key1_1 = create_key(&Id::<ServiceId>::new("abc"), &Id::<TagId>::new("defgh"));
    assert_eq!(key1, key1_1);

    let key2 = create_key(&Id::<ServiceId>::new("abcd"), &Id::<TagId>::new("efgh"));
    assert!(key2 != key1);
}

#[test]
#[allow(unused_variables)]
fn storage_test() {
    use util::ServiceId;

    // Simple RAII style struct to delete the test db.
    struct AutoDeleteDb { };
    impl Drop for AutoDeleteDb {
        fn drop(&mut self) {
            remove_test_db();
        }
    }
    let auto_db = AutoDeleteDb { };

    let mut store = TagStorage::new(&get_db_environment());

    let id1 = Id::<ServiceId>::new("first id");
    let id2 = Id::<ServiceId>::new("second id");

    // Start with an empty db.
    let mut tags = store.get_tags_for(&id1).unwrap();
    assert_eq!(tags.len(), 0);

    // Add a first tag.
    store.add_tag(&id1, &Id::new("tag1")).unwrap();
    tags = store.get_tags_for(&id1).unwrap();
    assert_eq!(tags.len(), 1);

    // Adding the same one is a no-op.
    store.add_tag(&id1, &Id::new("tag1")).unwrap();
    tags = store.get_tags_for(&id1).unwrap();
    assert_eq!(tags.len(), 1);

    // Adding a new tag.
    store.add_tag(&id1, &Id::new("tag2")).unwrap();
    tags = store.get_tags_for(&id1).unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags, [Id::new("tag1"), Id::new("tag2")]);

    // Add the same tags with a different id.
    store.add_tag(&id2, &Id::new("tag1")).unwrap();
    store.add_tag(&id2, &Id::new("tag2")).unwrap();
    tags = store.get_tags_for(&id1).unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags, [Id::new("tag1"), Id::new("tag2")]);
    tags = store.get_tags_for(&id2).unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags, [Id::new("tag1"), Id::new("tag2")]);

    // Non existing id.
    store.remove_tag(&Id::<ServiceId>::new("id3"), &Id::new("some tag")).unwrap();

    // Remove some tags from id2.
    store.remove_tag(&id2, &Id::new("tag1")).unwrap();
    tags = store.get_tags_for(&id2).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags, [Id::new("tag2")]);

    store.remove_tag(&id2, &Id::new("tag2")).unwrap();
    tags = store.get_tags_for(&id2).unwrap();
    assert_eq!(tags.len(), 0);

    // id1 should be unchanged.
    tags = store.get_tags_for(&id1).unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags, [Id::new("tag1"), Id::new("tag2")]);

    // Remove all the id1 tags.
    store.remove_all_tags_for(&id1).unwrap();
    tags = store.get_tags_for(&id1).unwrap();
    assert_eq!(tags.len(), 0);

    // Adding multiple tags at once.
    store.add_tags(&id1, &[Id::new("tag1"), Id::new("tag2"), Id::new("tag3")]).unwrap();
    tags = store.get_tags_for(&id1).unwrap();
    assert_eq!(tags.len(), 3);

    // Removing multiple tags at once.
    store.remove_tags(&id1, &[Id::new("tag1"), Id::new("tag2"), Id::new("tag3")]).unwrap();
    tags = store.get_tags_for(&id1).unwrap();
    assert_eq!(tags.len(), 0);
}
