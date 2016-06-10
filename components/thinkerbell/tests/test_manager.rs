#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]
extern crate foxbox_taxonomy;
extern crate foxbox_thinkerbell;
extern crate serde;
extern crate transformable_channels;

use std::fs::File;
use std::io::Read;
use std::path::Path;
use transformable_channels::mpsc::*;

use foxbox_thinkerbell::fake_env::FakeEnv;
use foxbox_thinkerbell::manager::*;

use foxbox_taxonomy::api::User;
use foxbox_taxonomy::util::Id;

fn load_json(path: &str) -> String {
    let mut file = File::open(path).unwrap();
    let mut source = String::new();
    file.read_to_string(&mut source).unwrap();
    source
}

#[test]
fn test_database_add_remove_script() {
    let (tx_env, _) = channel();
    let env = FakeEnv::new(Box::new(tx_env));

    println!("* Cleaning up the database.");
    let (tx, _) = channel();
    let mut db = ScriptManager::new(env, Path::new("./test_script_database.sqlite"), Box::new(tx)).unwrap();

    db.remove_all().unwrap();


    println!("* Initially, there should be no recipe running.");
    assert_eq!(db.get_running_count(), 0);

    println!("* Putting a recipe in the database. It should be reported as running.");
    let name = Id::<ScriptId>::new("Sample Ruleset");
    db.put(&name, &load_json("./examples/ruleset.json"), &User::Id(String::from("1"))).unwrap();
    assert_eq!(db.get_running_count(), 1);


    println!("* The recipe should have the user with which it was stored.");
    let (_, owner) = db.get_source_and_owner(&name).unwrap();
    assert_eq!(owner, User::Id(String::from("1")));

    println!("* Enable the recipe again. It should still be reported as running.");
    db.set_enabled(&name, true).unwrap();
    assert_eq!(db.get_running_count(), 1);

    println!("* Disable the recipe. It should not be reported as running anymore.");
    db.set_enabled(&name, false).unwrap();
    assert_eq!(db.get_running_count(), 0);

    println!("* Enable the recipe again. It should be reported as running once again.");
    db.set_enabled(&name, true).unwrap();
    assert_eq!(db.get_running_count(), 1);

    println!("* Remove the recipe. It should not be reported as running anymore.");
    db.remove(&name).unwrap();
    assert_eq!(db.get_running_count(), 0);

    println!("* Add again the recipe. It should be reported as running again.");
    db.put(&name, &load_json("./examples/ruleset.json"), &User::Id(String::from("1"))).unwrap();
    assert_eq!(db.get_running_count(), 1);

    println!("* Overwrite the recipe. It should still be reported as running.");
    db.put(&name, &load_json("./examples/ruleset.json"), &User::Id(String::from("1"))).unwrap();
    assert_eq!(db.get_running_count(), 1);
}
