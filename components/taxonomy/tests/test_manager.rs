extern crate foxbox_taxonomy;
extern crate libc;
extern crate transformable_channels;
#[macro_use]
extern crate assert_matches;

use foxbox_taxonomy::manager::*;
use foxbox_taxonomy::fake_adapter::*;
use foxbox_taxonomy::api::{ API, Error, InternalError, TargetMap, Targetted, User, WatchEvent as Event };
use foxbox_taxonomy::selector::*;
use foxbox_taxonomy::services::*;
use foxbox_taxonomy::values::*;

use transformable_channels::mpsc::*;

use std::collections::{ HashMap, HashSet };
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

// Trivial utility function to convert the old TargetMap format to the newer one, to avoid
// having to rewrite the tests.
fn target_map<K, T>(mut source: Vec<(Vec<K>, T)>) -> TargetMap<K, T> where K: Clone, T: Clone {
    source.drain(..).map(|(v, t)| Targetted::new(v, t)).collect()
}

pub fn get_db_environment() -> PathBuf {
    use libc::getpid;
    use std::thread;
    let tid = format!("{:?}", thread::current()).replace("(", "+").replace(")", "+");
    let s = format!("./tagstore_db_test-{}-{}.sqlite", unsafe { getpid() }, tid.replace("/", "42"));
    PathBuf::from(s)
}

pub fn remove_test_db() {
    use std::fs;

    let dbfile = get_db_environment();
    match fs::remove_file(dbfile.clone()) {
        Err(e) => panic!("Error {} cleaning up {}", e, dbfile.display()),
        _ => assert!(true)
    }
}

#[test]
#[allow(unused_variables)]
fn test_tags_in_db() {
    // Simple RAII style struct to delete the test db.
    struct AutoDeleteDb { };
    impl Drop for AutoDeleteDb {
        fn drop(&mut self) {
            remove_test_db();
        }
    }
    let auto_db = AutoDeleteDb { };

    let id_1 = Id::<AdapterId>::new("adapter id 1");
    let service_id_1 = Id::<ServiceId>::new("service id 1");
    let getter_id_1 = Id::<Channel>::new("getter id 1");
    let setter_id_1 = Id::<Channel>::new("setter id 1");

    let tag_id_1 = Id::<TagId>::new("tag id 1");
    let tag_id_2 = Id::<TagId>::new("tag id 2");
    let tag_id_3 = Id::<TagId>::new("tag id 3");
    let tag_id_4 = Id::<TagId>::new("tag id 4");

    let service_1 = Service {
        id: service_id_1.clone(),
        adapter: id_1.clone(),
        tags: HashSet::new(),
        properties: HashMap::new(),
        getters: HashMap::new(),
        setters: HashMap::new(),
    };

    let getter_1 = Channel {
        id: getter_id_1.clone(),
        service: service_id_1.clone(),
        adapter: id_1.clone(),
        tags: HashSet::new(),
        kind: ChannelKind::LightOn,
    };

    let setter_1 = Channel {
        id: setter_id_1.clone(),
        service: service_id_1.clone(),
        adapter: id_1.clone(),
        tags: HashSet::new(),
        kind: ChannelKind::LightOn,
    };

    // Fist "session", starting from an empty state.
    {
        let manager = AdapterManager::new(Some(get_db_environment()));
        manager.add_adapter(Arc::new(FakeAdapter::new(&id_1))).unwrap();
        manager.add_service(service_1.clone()).unwrap();
        manager.add_getter(getter_1.clone()).unwrap();
        manager.add_setter(setter_1.clone()).unwrap();

        manager.add_service_tags(vec![ServiceSelector::new().with_id(service_id_1.clone())],
                                 vec![tag_id_1.clone(), tag_id_2.clone()]);

        manager.add_getter_tags(vec![ChannelSelector::new().with_id(getter_id_1.clone())],
                                vec![tag_id_2.clone(), tag_id_3.clone()]);

        manager.add_setter_tags(vec![ChannelSelector::new().with_id(setter_id_1.clone())],
                                vec![tag_id_1.clone(), tag_id_4.clone(), tag_id_3.clone()]);

        manager.remove_getter(&getter_id_1).unwrap();
        manager.remove_setter(&setter_id_1).unwrap();
        manager.remove_service(&service_id_1).unwrap();
        assert_eq!(manager.get_services(vec![]).len(), 0);

        // Re-add the same service, getter and setter to check if we persisted the tags.
        manager.add_service(service_1.clone()).unwrap();
        manager.add_getter(getter_1.clone()).unwrap();
        manager.add_setter(setter_1.clone()).unwrap();

        let services = manager.get_services(vec![]);
        assert_eq!(services.len(), 1);

        let ref service = services[0];
        assert_eq!(service.tags.len(), 2);
        assert_eq!(service.tags.contains(&tag_id_1), true);
        assert_eq!(service.tags.contains(&tag_id_2), true);

        let getters = manager.get_getter_channels(vec![ChannelSelector::new()]);
        assert_eq!(getters.len(), 1);
        let ref getter = getters[0];
        assert_eq!(getter.tags.len(), 2);
        assert_eq!(getter.tags.contains(&tag_id_2), true);
        assert_eq!(getter.tags.contains(&tag_id_3), true);

        let setters = manager.get_setter_channels(vec![ChannelSelector::new()]);
        assert_eq!(setters.len(), 1);
        let ref setter = setters[0];
        assert_eq!(setter.tags.len(), 3);
        assert_eq!(setter.tags.contains(&tag_id_1), true);
        assert_eq!(setter.tags.contains(&tag_id_3), true);
        assert_eq!(setter.tags.contains(&tag_id_4), true);

        manager.remove_adapter(&id_1).unwrap();
        manager.stop();
    }

    // Second "session", starting with content added in session 1.
    {
        let manager = AdapterManager::new(Some(get_db_environment()));
        manager.add_adapter(Arc::new(FakeAdapter::new(&id_1))).unwrap();
        manager.add_service(service_1.clone()).unwrap();
        manager.add_getter(getter_1.clone()).unwrap();
        manager.add_setter(setter_1.clone()).unwrap();

        let services = manager.get_services(vec![]);
        assert_eq!(services.len(), 1);

        let ref service = services[0];
        assert_eq!(service.tags.len(), 2);
        assert_eq!(service.tags.contains(&tag_id_1), true);
        assert_eq!(service.tags.contains(&tag_id_2), true);

        let getters = manager.get_getter_channels(vec![ChannelSelector::new()]);
        assert_eq!(getters.len(), 1);
        let ref getter = getters[0];
        assert_eq!(getter.tags.len(), 2);
        assert_eq!(getter.tags.contains(&tag_id_2), true);
        assert_eq!(getter.tags.contains(&tag_id_3), true);

        let setters = manager.get_setter_channels(vec![ChannelSelector::new()]);
        assert_eq!(setters.len(), 1);
        let ref setter = setters[0];
        assert_eq!(setter.tags.len(), 3);
        assert_eq!(setter.tags.contains(&tag_id_1), true);
        assert_eq!(setter.tags.contains(&tag_id_3), true);
        assert_eq!(setter.tags.contains(&tag_id_4), true);

        // Remove all the tags, to check in session 3 if we start empty again.
        manager.remove_service_tags(vec![ServiceSelector::new().with_id(service_id_1.clone())],
                                    vec![tag_id_1.clone(), tag_id_2.clone()]);
        let services = manager.get_services(vec![]);
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].tags.len(), 0);

        manager.remove_getter_tags(vec![ChannelSelector::new().with_id(getter_id_1.clone())],
                                vec![tag_id_2.clone(), tag_id_3.clone()]);
        let getters = manager.get_getter_channels(vec![ChannelSelector::new()]);
        assert_eq!(getters.len(), 1);
        assert_eq!(getters[0].tags.len(), 0);

        manager.remove_setter_tags(vec![ChannelSelector::new().with_id(setter_id_1.clone())],
                                vec![tag_id_1.clone(), tag_id_4.clone(), tag_id_3.clone()]);
        let setters = manager.get_setter_channels(vec![ChannelSelector::new()]);
        assert_eq!(setters.len(), 1);
        assert_eq!(setters[0].tags.len(), 0);

        manager.remove_adapter(&id_1).unwrap();
        manager.stop();
    }

    // Third "session", checking that we have no tags anymore.
    {
        let manager = AdapterManager::new(Some(get_db_environment()));
        manager.add_adapter(Arc::new(FakeAdapter::new(&id_1))).unwrap();
        manager.add_service(service_1.clone()).unwrap();
        manager.add_getter(getter_1.clone()).unwrap();
        manager.add_setter(setter_1.clone()).unwrap();

        let services = manager.get_services(vec![]);
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].tags.len(), 0);

        let getters = manager.get_getter_channels(vec![ChannelSelector::new()]);
        assert_eq!(getters.len(), 1);
        assert_eq!(getters[0].tags.len(), 0);

        let setters = manager.get_setter_channels(vec![ChannelSelector::new()]);
        assert_eq!(setters.len(), 1);
        assert_eq!(setters[0].tags.len(), 0);

        manager.remove_adapter(&id_1).unwrap();
        manager.stop();
    }
}

#[test]
fn test_add_remove_adapter() {
    for clear in vec![false, true] {
		println!("# Starting with test with clear {}.\n", clear);

        let manager = AdapterManager::new(None);
        let id_1 = Id::new("id 1");
        let id_2 = Id::new("id 2");

        println!("* Adding two distinct test adapters should work.");
        manager.add_adapter(Arc::new(FakeAdapter::new(&id_1))).unwrap();
        manager.add_adapter(Arc::new(FakeAdapter::new(&id_2))).unwrap();

        println!("* Attempting to add yet another test adapter with id_1 or id_2 should fail.");
        match manager.add_adapter(Arc::new(FakeAdapter::new(&id_1))) {
            Err(Error::InternalError(InternalError::DuplicateAdapter(ref id))) if *id == id_1 => {},
            other => panic!("Unexpected result {:?}", other)
        }
        match manager.add_adapter(Arc::new(FakeAdapter::new(&id_2))) {
            Err(Error::InternalError(InternalError::DuplicateAdapter(ref id))) if *id == id_2 => {},
            other => panic!("Unexpected result {:?}", other)
        }

        println!("* Removing id_1 should succeed. At this stage, we still shouldn't be able to add id_2, \
                  but we should be able to re-add id_1");
        manager.remove_adapter(&id_1).unwrap();
        match manager.add_adapter(Arc::new(FakeAdapter::new(&id_2))) {
            Err(Error::InternalError(InternalError::DuplicateAdapter(ref id))) if *id == id_2 => {},
            other => panic!("Unexpected result {:?}", other)
        }
        manager.add_adapter(Arc::new(FakeAdapter::new(&id_1))).unwrap();

        println!("* Removing id_1 twice should fail the second time.");
        manager.remove_adapter(&id_1).unwrap();
        match manager.remove_adapter(&id_1) {
            Err(Error::InternalError(InternalError::NoSuchAdapter(ref id))) if *id == id_1 => {},
            other => panic!("Unexpected result {:?}", other)
        }

        if clear {
            println!("* Clearing does not break the manager.\n");
            manager.stop();
        } else {
            println!("* Not clearing does not break the manager.\n");
        }
    }
}

#[test]
fn test_add_remove_services() {
    println!("");
    for clear in vec![false, true] {
		println!("# Starting with test with clear {}.", clear);

        let manager = AdapterManager::new(None);
        let id_1 = Id::<AdapterId>::new("adapter id 1");
        let id_2 = Id::<AdapterId>::new("adapter id 2");
        let id_3 = Id::<AdapterId>::new("adapter id 3");


        let getter_id_1 = Id::<Channel>::new("getter id 1");
        let getter_id_2 = Id::<Channel>::new("getter id 2");
        let getter_id_3 = Id::<Channel>::new("getter id 3");

        let setter_id_1 = Id::<Channel>::new("setter id 1");
        let setter_id_2 = Id::<Channel>::new("setter id 2");
        let setter_id_3 = Id::<Channel>::new("setter id 3");

        let service_id_1 = Id::<ServiceId>::new("service id 1");
        let service_id_2 = Id::<ServiceId>::new("service id 2");
        let service_id_3 = Id::<ServiceId>::new("service id 3");

        let getter_1 = Channel {
            id: getter_id_1.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let setter_1 = Channel {
            id: setter_id_1.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let getter_1_with_bad_service = Channel {
            id: getter_id_1.clone(),
            service: service_id_3.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let setter_1_with_bad_service = Channel {
            id: setter_id_1.clone(),
            service: service_id_3.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let getter_2_with_bad_adapter = Channel {
            adapter: id_3.clone(),
            .. getter_1.clone()
        };

        let setter_2_with_bad_adapter = Channel {
            adapter: id_3.clone(),
            .. setter_1.clone()
        };

        let service_1 = Service {
            id: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            properties: HashMap::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
        };

        let getter_2 = Channel {
            id: getter_id_2.clone(),
            service: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let setter_2 = Channel {
            id: setter_id_2.clone(),
            service: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let service_2 = Service {
            id: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            properties: HashMap::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
        };

        let service_2_with_channels = Service {
            getters: vec![(getter_id_2.clone(), getter_2.clone())].iter().cloned().collect(),
            setters: vec![(setter_id_2.clone(), setter_2.clone())].iter().cloned().collect(),
            ..service_2.clone()
        };

        println!("* Adding a service should fail if there is no adapter.");
        match manager.add_service(service_1.clone()) {
            Err(Error::InternalError(InternalError::NoSuchAdapter(ref err))) if *err == id_1 => {},
            other => panic!("Unexpected result {:?}", other)
        }

        println!("* Adding a service should fail if the adapter doesn't exist.");
        manager.add_adapter(Arc::new(FakeAdapter::new(&id_2))).unwrap();
        match manager.add_service(service_1.clone()) {
            Err(Error::InternalError(InternalError::NoSuchAdapter(ref err))) if *err == id_1 => {},
            other => panic!("Unexpected result {:?}", other)
        }

        println!("* Adding a service should fail if the service is not empty.");
        match manager.add_service(service_2_with_channels.clone()) {
            Err(Error::InternalError(InternalError::InvalidInitialService)) => {},
            other => panic!("Unexpected result {:?}", other)
        }

        println!("* We shouldn't have any channels.");
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new()]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new()]).len(), 0);

        println!("* Make sure that none of the services has been added.");
        assert_eq!(manager.get_services(vec![ServiceSelector::new()]).len(), 0);

        println!("* Adding a service can succeed.");
        manager.add_adapter(Arc::new(FakeAdapter::new(&id_1))).unwrap();
        manager.add_service(service_1.clone()).unwrap();
        assert_eq!(manager.get_services(vec![ServiceSelector::new()]).len(), 1);

        println!("* Make sure that we are finding the right service.");
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_id(service_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_id(service_id_2.clone())]).len(), 0);

        println!("* Adding a second service with the same id should fail.");
        match manager.add_service(service_1.clone()) {
            Err(Error::InternalError(InternalError::DuplicateService(ref err))) if *err == service_id_1 => {},
            other => panic!("Unexpected result {:?}", other)
        }

        println!("* Adding channels should fail if the service doesn't exist.");
        match manager.add_getter(getter_1_with_bad_service.clone()) {
            Err(Error::InternalError(InternalError::NoSuchService(ref err))) if *err == service_id_3 => {},
            other => panic!("Unexpected result {:?}", other)
        }
        match manager.add_setter(setter_1_with_bad_service.clone()) {
            Err(Error::InternalError(InternalError::NoSuchService(ref err))) if *err == service_id_3 => {},
            other => panic!("Unexpected result {:?}", other)
        }

        println!("* The attempt shouldn't let any channel lying around.");
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new()]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new()]).len(), 0);

        println!("* Adding channels should fail if the adapter doesn't match that of its service.");
        match manager.add_getter(getter_2_with_bad_adapter) {
            Err(Error::InternalError(InternalError::ConflictingAdapter(ref err_1, ref err_2)))
                if *err_1 == id_3 && *err_2 == id_1 => {},
            Err(Error::InternalError(InternalError::ConflictingAdapter(ref err_1, ref err_2)))
                if *err_1 == id_1 && *err_2 == id_3 => {},
            other => panic!("Unexpected result {:?}", other)
        }
        match manager.add_setter(setter_2_with_bad_adapter) {
            Err(Error::InternalError(InternalError::ConflictingAdapter(ref err_1, ref err_2)))
                if *err_1 == id_3 && *err_2 == id_1 => {},
            Err(Error::InternalError(InternalError::ConflictingAdapter(ref err_1, ref err_2)))
                if *err_1 == id_1 && *err_2 == id_3 => {},
            other => panic!("Unexpected result {:?}", other)
        }

        println!("* The attempt shouldn't let any channel lying around.");
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new()]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new()]).len(), 0);

        println!("* Adding getter channels can succeed.");
        manager.add_getter(getter_1.clone()).unwrap();
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new()]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new()]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_id(getter_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_id(setter_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 0);

        println!("* Adding setter channels can succeed.");
        manager.add_setter(setter_1.clone()).unwrap();
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new()]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new()]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_id(getter_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_id(setter_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 1);

        println!("* Removing getter channels can succeed.");
        manager.remove_getter(&getter_id_1).unwrap();
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new()]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new()]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_id(getter_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_id(setter_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 1);

        println!("* Removing setter channels can succeed.");
        manager.remove_setter(&setter_id_1).unwrap();
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new()]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new()]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_id(getter_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_id(setter_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 0);

        println!("* We can remove a service without channels.");
        manager.remove_service(&service_id_1).unwrap();

        println!("* We can add several services, then several channels.");
        manager.add_service(service_1.clone()).unwrap();
        manager.add_service(service_2.clone()).unwrap();
        manager.add_getter(getter_1.clone()).unwrap();
        manager.add_setter(setter_1.clone()).unwrap();
        manager.add_getter(getter_2.clone()).unwrap();
        manager.add_setter(setter_2.clone()).unwrap();
        assert_eq!(manager.get_services(vec![ServiceSelector::new()]).len(), 2);
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_id(service_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_id(service_id_2.clone())]).len(), 1);
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_id(service_id_3.clone())]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new()]).len(), 2);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new()]).len(), 2);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_id(getter_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_id(setter_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_id(getter_id_2.clone())]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_id(setter_id_2.clone())]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_id(getter_id_3.clone())]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_id(setter_id_3.clone())]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_parent(service_id_2.clone())]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_parent(service_id_2.clone())]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_parent(service_id_3.clone())]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_parent(service_id_3.clone())]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 2);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 2);

        println!("* We can remove a service with channels.");
        manager.remove_service(&service_id_1).unwrap();
        assert_eq!(manager.get_services(vec![ServiceSelector::new()]).len(), 1);
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_id(service_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_id(service_id_2.clone())]).len(), 1);
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_id(service_id_3.clone())]).len(), 0);

        println!("* Removing a service with channels also removes its channels.");
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new()]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new()]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_id(getter_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_id(setter_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_parent(service_id_1.clone())]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_kind(ChannelKind::LightOn)]).len(), 1);

        println!("* Removing a service with channels doesn't remove other channels.");
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_id(getter_id_2.clone())]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_id(setter_id_2.clone())]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_parent(service_id_2.clone())]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_parent(service_id_2.clone())]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_parent(service_id_3.clone())]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_parent(service_id_3.clone())]).len(), 0);

        if clear {
            println!("* Clearing does not break the manager.
");
            manager.stop();
        } else {
            println!("* Not clearing does not break the manager.
");
        }
    }
}

#[test]
fn test_add_remove_tags() {
    println!("");
    for clear in vec![false, true] {
		println!("# Starting with test with clear {}.
", clear);

        let manager = AdapterManager::new(None);
        let id_1 = Id::<AdapterId>::new("adapter id 1");
        let id_2 = Id::<AdapterId>::new("adapter id 2");

        let getter_id_1 = Id::<Channel>::new("getter id 1");
        let getter_id_2 = Id::<Channel>::new("getter id 2");

        let setter_id_1 = Id::<Channel>::new("setter id 1");
        let setter_id_2 = Id::<Channel>::new("setter id 2");

        let service_id_1 = Id::<ServiceId>::new("service id 1");
        let service_id_2 = Id::<ServiceId>::new("service id 2");

        let getter_1 = Channel {
            id: getter_id_1.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let setter_1 = Channel {
            id: setter_id_1.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let service_1 = Service {
            id: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            properties: HashMap::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
        };

        let getter_2 = Channel {
            id: getter_id_2.clone(),
            service: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let setter_2 = Channel {
            id: setter_id_2.clone(),
            service: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let service_2 = Service {
            id: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            properties: HashMap::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
        };

        let tag_1 = Id::<TagId>::new("tag_1");
        let tag_2 = Id::<TagId>::new("tag_2");
        let tag_3 = Id::<TagId>::new("tag_3");

        println!("* Initially, there are no tags.");
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_tags(vec![tag_1.clone()])]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_tags(vec![tag_1.clone()])]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_tags(vec![tag_1.clone()])]).len(), 0);

        println!("* After adding an adapter, service, getter, setter, still no tags.");
        manager.add_adapter(Arc::new(FakeAdapter::new(&id_1))).unwrap();
        manager.add_service(service_1.clone()).unwrap();
        manager.add_getter(getter_1.clone()).unwrap();
        manager.add_setter(setter_1.clone()).unwrap();
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_tags(vec![tag_1.clone()])]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_tags(vec![tag_1.clone()])]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_tags(vec![tag_1.clone()])]).len(), 0);

        println!("* Removing tags from non-existent services and channels doesn't hurt and returns 0.");
        assert_eq!(manager
            .remove_service_tags(
                vec![ServiceSelector::new().with_id(service_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            0);
        assert_eq!(manager
            .remove_getter_tags(
                vec![ChannelSelector::new().with_id(getter_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            0);
        assert_eq!(manager
            .remove_setter_tags(
                vec![ChannelSelector::new().with_id(setter_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            0);

        println!("* Adding tags to non-existent services and channels doesn't hurt and returns 0.");
        assert_eq!(manager
            .add_service_tags(
                vec![ServiceSelector::new().with_id(service_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            0);
        assert_eq!(manager
            .add_getter_tags(
                vec![ChannelSelector::new().with_id(getter_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            0);
        assert_eq!(manager
            .add_setter_tags(
                vec![ChannelSelector::new().with_id(setter_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            0);

        println!("* There are still no tags.");
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_tags(vec![tag_2.clone()])]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_tags(vec![tag_2.clone()])]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_tags(vec![tag_2.clone()])]).len(), 0);

        println!("* Removing non-added tags from existent services and channels doesn't hurt and returns 1.");
        manager.add_adapter(Arc::new(FakeAdapter::new(&id_2))).unwrap();
        manager.add_service(service_2.clone()).unwrap();
        manager.add_getter(getter_2.clone()).unwrap();
        manager.add_setter(setter_2.clone()).unwrap();
        assert_eq!(manager
            .remove_service_tags(
                vec![ServiceSelector::new().with_id(service_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            1);
        assert_eq!(manager
            .remove_getter_tags(
                vec![ChannelSelector::new().with_id(getter_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            1);
        assert_eq!(manager
            .remove_setter_tags(
                vec![ChannelSelector::new().with_id(setter_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            1);

        println!("* We can add tags tags to services and channels, this returns 1.");
        assert_eq!(manager
            .add_service_tags(
                vec![ServiceSelector::new().with_id(service_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            1);
        assert_eq!(manager
            .add_getter_tags(
                vec![ChannelSelector::new().with_id(getter_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            1);
        assert_eq!(manager
            .add_setter_tags(
                vec![ChannelSelector::new().with_id(setter_id_2.clone())], vec![tag_2.clone(), tag_3.clone()]
            ),
            1);

        println!("* We can select using these tags.");
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_tags(vec![tag_1.clone()])]).len(), 0);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_tags(vec![tag_1.clone()])]).len(), 0);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_tags(vec![tag_1.clone()])]).len(), 0);
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_tags(vec![tag_2.clone()])]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_tags(vec![tag_2.clone()])]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_tags(vec![tag_2.clone()])]).len(), 1);
        assert_eq!(manager.get_services(vec![ServiceSelector::new().with_tags(vec![tag_3.clone()])]).len(), 1);
        assert_eq!(manager.get_getter_channels(vec![ChannelSelector::new().with_tags(vec![tag_3.clone()])]).len(), 1);
        assert_eq!(manager.get_setter_channels(vec![ChannelSelector::new().with_tags(vec![tag_3.clone()])]).len(), 1);

        println!("* The tags are only applied to the right services/getters.");
        assert_eq!(manager.get_services(vec![
            ServiceSelector::new()
                .with_tags(vec![tag_2.clone()])
                .with_id(service_id_1.clone())
            ]).len(), 0
        );
        assert_eq!(manager.get_getter_channels(vec![
            ChannelSelector::new()
                .with_tags(vec![tag_2.clone()])
                .with_id(getter_id_1.clone())
            ]).len(), 0
        );
        assert_eq!(manager.get_setter_channels(vec![
            ChannelSelector::new()
                .with_tags(vec![tag_2.clone()])
                .with_id(setter_id_1.clone())
            ]).len(), 0
        );

        println!("* The tags are applied to the right services/getters.");
        let selection = manager.get_services(vec![
            ServiceSelector::new()
                .with_tags(vec![tag_2.clone()])
                .with_id(service_id_2.clone())
            ]);
        assert_eq!(selection.len(), 1);
        assert_eq!(selection[0].id, service_id_2);
        assert_eq!(selection[0].tags.len(), 2);
        assert!(selection[0].tags.contains(&tag_2));
        assert!(selection[0].tags.contains(&tag_3));

        let selection = manager.get_getter_channels(vec![
            ChannelSelector::new()
                .with_tags(vec![tag_2.clone()])
                .with_id(getter_id_2.clone())
        ]);
        assert_eq!(selection.len(), 1);
        assert_eq!(selection[0].id, getter_id_2);
        assert_eq!(selection[0].tags.len(), 2);
        assert!(selection[0].tags.contains(&tag_2));
        assert!(selection[0].tags.contains(&tag_3));

        let selection = manager.get_setter_channels(vec![
            ChannelSelector::new()
                .with_tags(vec![tag_2.clone()])
                .with_id(setter_id_2.clone())
        ]);
        assert_eq!(selection.len(), 1);
        assert_eq!(selection[0].id, setter_id_2);
        assert_eq!(selection[0].tags.len(), 2);
        assert!(selection[0].tags.contains(&tag_2));
        assert!(selection[0].tags.contains(&tag_3));

        let selection = manager.get_services(vec![
            ServiceSelector::new()
                .with_tags(vec![tag_3.clone()])
                .with_id(service_id_2.clone())
            ]);
        assert_eq!(selection.len(), 1);
        assert_eq!(selection[0].id, service_id_2);
        assert_eq!(selection[0].tags.len(), 2);
        assert!(selection[0].tags.contains(&tag_2));
        assert!(selection[0].tags.contains(&tag_3));

        let selection = manager.get_getter_channels(vec![
            ChannelSelector::new()
                .with_tags(vec![tag_3.clone()])
                .with_id(getter_id_2.clone())
        ]);
        assert_eq!(selection.len(), 1);
        assert_eq!(selection[0].id, getter_id_2);
        assert_eq!(selection[0].tags.len(), 2);
        assert!(selection[0].tags.contains(&tag_2));
        assert!(selection[0].tags.contains(&tag_3));

        let selection = manager.get_setter_channels(vec![
            ChannelSelector::new()
                .with_tags(vec![tag_3.clone()])
                .with_id(setter_id_2.clone())
        ]);
        assert_eq!(selection.len(), 1);
        assert_eq!(selection[0].id, setter_id_2);
        assert_eq!(selection[0].tags.len(), 2);
        assert!(selection[0].tags.contains(&tag_2));
        assert!(selection[0].tags.contains(&tag_3));

        println!("* We can remove tags, both existent and non-existent.");
        assert_eq!(manager
            .remove_service_tags(
                vec![ServiceSelector::new().with_id(service_id_2.clone())], vec![tag_1.clone(), tag_3.clone()]
            ),
            1);
        assert_eq!(manager
            .remove_getter_tags(
                vec![ChannelSelector::new().with_id(getter_id_2.clone())], vec![tag_1.clone(), tag_3.clone()]
            ),
            1);
        assert_eq!(manager
            .remove_setter_tags(
                vec![ChannelSelector::new().with_id(setter_id_2.clone())], vec![tag_1.clone(), tag_3.clone()]
            ),
            1);

        println!("* Looking by tags has been updated.");
        let selection = manager.get_services(vec![
            ServiceSelector::new()
                .with_tags(vec![tag_2.clone()])
                .with_id(service_id_2.clone())
            ]);
        assert_eq!(selection.len(), 1);
        assert_eq!(selection[0].id, service_id_2);
        assert_eq!(selection[0].tags.len(), 1);
        assert!(selection[0].tags.contains(&tag_2));

        let selection = manager.get_getter_channels(vec![
            ChannelSelector::new()
                .with_tags(vec![tag_2.clone()])
                .with_id(getter_id_2.clone())
        ]);
        assert_eq!(selection.len(), 1);
        assert_eq!(selection[0].id, getter_id_2);
        assert_eq!(selection[0].tags.len(), 1);
        assert!(selection[0].tags.contains(&tag_2));

        let selection = manager.get_setter_channels(vec![
            ChannelSelector::new()
                .with_tags(vec![tag_2.clone()])
                .with_id(setter_id_2.clone())
        ]);
        assert_eq!(selection.len(), 1);
        assert_eq!(selection[0].id, setter_id_2);
        assert_eq!(selection[0].tags.len(), 1);
        assert!(selection[0].tags.contains(&tag_2));

        let selection = manager.get_services(vec![
            ServiceSelector::new()
                .with_tags(vec![tag_3.clone()])
                .with_id(service_id_2.clone())
            ]);
        assert_eq!(selection.len(), 0);

        let selection = manager.get_getter_channels(vec![
            ChannelSelector::new()
                .with_tags(vec![tag_3.clone()])
                .with_id(getter_id_2.clone())
        ]);
        assert_eq!(selection.len(), 0);

        let selection = manager.get_setter_channels(vec![
            ChannelSelector::new()
                .with_tags(vec![tag_3.clone()])
                .with_id(setter_id_2.clone())
        ]);
        assert_eq!(selection.len(), 0);

        if clear {
            println!("* Clearing does not break the manager.
");
            manager.stop();
        } else {
            println!("* Not clearing does not break the manager.
");
        }
    }

    println!("");
}

#[test]
fn test_fetch() {
    println!("");

    for clear in vec![false, true] {
		println!("# Starting with test with clear {}.
", clear);

        let manager = AdapterManager::new(None);
        let id_1 = Id::<AdapterId>::new("adapter id 1");
        let id_2 = Id::<AdapterId>::new("adapter id 2");


        let getter_id_1_1 = Id::<Channel>::new("getter id 1.1");
        let getter_id_1_2 = Id::<Channel>::new("getter id 1.2");
        let getter_id_1_3 = Id::<Channel>::new("getter id 1.3");
        let getter_id_2 = Id::<Channel>::new("getter id 2");

        let service_id_1 = Id::<ServiceId>::new("service id 1");
        let service_id_2 = Id::<ServiceId>::new("service id 2");

        let getter_1_1 = Channel {
            id: getter_id_1_1.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let getter_1_2 = Channel {
            id: getter_id_1_2.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let getter_1_3 = Channel {
            id: getter_id_1_3.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let getter_2 = Channel {
            id: getter_id_2.clone(),
            service: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let service_1 = Service {
            id: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            properties: HashMap::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
        };

        let service_2 = Service {
            id: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            properties: HashMap::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
        };

        let adapter_1 = FakeAdapter::new(&id_1);
        let adapter_2 = FakeAdapter::new(&id_2);
        let tweak_1 = adapter_1.get_tweak();
        println!("* Without adapters, fetching values from a selector that has no channels returns an empty vector.");
        assert_eq!(manager.fetch_values(vec![ChannelSelector::new()], User::None).len(), 0);

        println!("* With adapters, fetching values from a selector that has no channels returns an empty vector.");
        manager.add_adapter(Arc::new(adapter_1)).unwrap();
        manager.add_adapter(Arc::new(adapter_2)).unwrap();
        manager.add_service(service_1.clone()).unwrap();
        manager.add_service(service_2.clone()).unwrap();
        assert_eq!(manager.fetch_values(vec![ChannelSelector::new()], User::None).len(), 0);

        println!("* Fetching empty values from a selector that has channels returns a vector of empty values.");
        manager.add_getter(getter_1_1.clone()).unwrap();
        manager.add_getter(getter_1_2.clone()).unwrap();
        manager.add_getter(getter_1_3.clone()).unwrap();
        manager.add_getter(getter_2.clone()).unwrap();
        let data = manager.fetch_values(vec![ChannelSelector::new()], User::None);
        assert_eq!(data.len(), 4);

        for result in data.values() {
            if let Ok(None) = *result {
                // We're good.
            } else {
                panic!("Unexpected result {:?}", result)
            }
        }

        println!("* Fetching values returns the right values.");
        tweak_1(Tweak::InjectGetterValue(getter_id_1_1.clone(), Ok(Some(Value::OnOff(OnOff::On)))));
        tweak_1(Tweak::InjectGetterValue(getter_id_1_2.clone(), Ok(Some(Value::OnOff(OnOff::Off)))));
        let data = manager.fetch_values(vec![ChannelSelector::new()], User::None);
        assert_eq!(data.len(), 4);
        match data.get(&getter_id_1_1) {
            Some(&Ok(Some(Value::OnOff(OnOff::On)))) => {},
            other => panic!("Unexpected result, {:?}", other)
        }
        match data.get(&getter_id_1_2) {
            Some(&Ok(Some(Value::OnOff(OnOff::Off)))) => {},
            other => panic!("Unexpected result, {:?}", other)
        }
        match data.get(&getter_id_1_3) {
            Some(&Ok(None)) => {},
            other => panic!("Unexpected result, {:?}", other)
        }
        match data.get(&getter_id_2) {
            Some(&Ok(None)) => {},
            other => panic!("Unexpected result, {:?}", other)
        }

        println!("* Fetching values returns the right errors.");
        tweak_1(Tweak::InjectGetterValue(getter_id_1_1.clone(), Err(Error::InternalError(InternalError::NoSuchChannel(getter_id_1_1.clone())))));
        let data = manager.fetch_values(vec![ChannelSelector::new()], User::None);
        assert_eq!(data.len(), 4);
        match data.get(&getter_id_1_1) {
            Some(&Err(Error::InternalError(InternalError::NoSuchChannel(ref id)))) if *id == getter_id_1_1 => {},
            other => panic!("Unexpected result, {:?}", other)
        }
        match data.get(&getter_id_1_2) {
            Some(&Ok(Some(Value::OnOff(OnOff::Off)))) => {},
            other => panic!("Unexpected result, {:?}", other)
        }
        match data.get(&getter_id_1_3) {
            Some(&Ok(None)) => {},
            other => panic!("Unexpected result, {:?}", other)
        }
        match data.get(&getter_id_2) {
            Some(&Ok(None)) => {},
            other => panic!("Unexpected result, {:?}", other)
        }

        println!("* Fetching a value that causes an internal type error returns that error.");
        tweak_1(Tweak::InjectGetterValue(getter_id_1_1.clone(), Ok(Some(Value::OpenClosed(OpenClosed::Open)))));
        let data = manager.fetch_values(vec![ChannelSelector::new()], User::None);
        assert_eq!(data.len(), 4);
        match data.get(&getter_id_1_1) {
            Some(&Err(Error::TypeError(TypeError {
                got: Type::OpenClosed,
                expected: Type::OnOff,
            }))) => {},
            other => panic!("Unexpected result, {:?}", other)
        }
        match data.get(&getter_id_1_2) {
            Some(&Ok(Some(Value::OnOff(OnOff::Off)))) => {},
            other => panic!("Unexpected result, {:?}", other)
        }
        match data.get(&getter_id_1_3) {
            Some(&Ok(None)) => {},
            other => panic!("Unexpected result, {:?}", other)
        }
        match data.get(&getter_id_2) {
            Some(&Ok(None)) => {},
            other => panic!("Unexpected result, {:?}", other)
        }

        if clear {
            println!("* Clearing does not break the manager.
");
            manager.stop();
        } else {
            println!("* Not clearing does not break the manager.
");
        }
    }

    // FIXME: Should test fetching with tags.

    println!("");
}

#[test]
fn test_send() {
    println!("");

    for clear in vec![false, true] {
		println!("# Starting with test with clear {}.
", clear);

        let manager = AdapterManager::new(None);
        let id_1 = Id::<AdapterId>::new("adapter id 1");
        let id_2 = Id::<AdapterId>::new("adapter id 2");

        let setter_id_1_1 = Id::<Channel>::new("setter id 1.1");
        let setter_id_1_2 = Id::<Channel>::new("setter id 1.2");
        let setter_id_1_3 = Id::<Channel>::new("setter id 1.3");
        let setter_id_2 = Id::<Channel>::new("setter id 2");

        let service_id_1 = Id::<ServiceId>::new("service id 1");
        let service_id_2 = Id::<ServiceId>::new("service id 2");

        let setter_1_1 = Channel {
            id: setter_id_1_1.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let setter_1_2 = Channel {
            id: setter_id_1_2.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let setter_1_3 = Channel {
            id: setter_id_1_3.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let setter_2 = Channel {
            id: setter_id_2.clone(),
            service: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let service_1 = Service {
            id: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            properties: HashMap::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
        };

        let service_2 = Service {
            id: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            properties: HashMap::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
        };

        let adapter_1 = FakeAdapter::new(&id_1);
        let adapter_2 = FakeAdapter::new(&id_2);
        let tweak_1 = adapter_1.get_tweak();
        let rx_adapter_1 = adapter_1.take_rx();
        let rx_adapter_2 = adapter_2.take_rx();

        println!("* Without adapters, sending values to a selector that has no channels returns an empty vector.");
        let data = manager.send_values(target_map(vec![(vec![ChannelSelector::new()], Value::OnOff(OnOff::On))]), User::None);

        assert_eq!(data.len(), 0);

        println!("* With adapters, sending values to a selector that has no channels returns an empty vector.");
        manager.add_adapter(Arc::new(adapter_1)).unwrap();
        manager.add_adapter(Arc::new(adapter_2)).unwrap();
        manager.add_service(service_1.clone()).unwrap();
        manager.add_service(service_2.clone()).unwrap();
        let data = manager.send_values(target_map(vec![(vec![ChannelSelector::new()], Value::OnOff(OnOff::On))]), User::None);
        assert_eq!(data.len(), 0);

        println!("* Sending well-typed values to channels succeeds if the adapter succeeds.");
        manager.add_setter(setter_1_1.clone()).unwrap();
        manager.add_setter(setter_1_2.clone()).unwrap();
        manager.add_setter(setter_1_3.clone()).unwrap();
        manager.add_setter(setter_2.clone()).unwrap();

        let data = manager.send_values(target_map(vec![(vec![ChannelSelector::new()], Value::OnOff(OnOff::On))]), User::None);
        assert_eq!(data.len(), 4);
        for result in data.values() {
            if let Ok(()) = *result {
                // We're good.
            } else {
                panic!("Unexpected result {:?}", result)
            }
        }

        println!("* All the values should have been received.");
        let mut data = HashMap::new();
        for _ in 0..3 {
            let Effect::ValueSent(id, value) = rx_adapter_1.try_recv().unwrap();
            data.insert(id, value);
        }
        assert_eq!(data.len(), 3);

        let value = rx_adapter_2.recv().unwrap();
        if let Effect::ValueSent(id, Value::OnOff(OnOff::On)) = value {
            assert_eq!(id, setter_id_2);
        } else {
            panic!("Unexpected value {:?}", value)
        }

        println!("* No further value should have been received.");
        assert_matches!(rx_adapter_1.try_recv(), Err(_));
        assert_matches!(rx_adapter_2.try_recv(), Err(_));

        println!("* Sending ill-typed values to channels will cause type errors.");
        let data = manager.send_values(target_map(vec![
            (vec![
                ChannelSelector::new().with_id(setter_id_1_1.clone()),
                ChannelSelector::new().with_id(setter_id_1_2.clone()),
                ChannelSelector::new().with_id(setter_id_2.clone()),
            ], Value::OpenClosed(OpenClosed::Closed)),
            (vec![
                ChannelSelector::new().with_id(setter_id_1_3.clone()).clone()
            ], Value::OnOff(OnOff::On))
        ]), User::None);
        assert_eq!(data.len(), 4);
        for id in vec![&setter_id_1_1, &setter_id_1_2, &setter_id_2] {
            match data.get(id) {
                Some(&Err(Error::TypeError(TypeError {
                    got: Type::OpenClosed,
                    expected: Type::OnOff
                }))) => {},
                other => panic!("Unexpected result for {:?}: {:?}", id, other)
            }
        }
        match data.get(&setter_id_1_3) {
            Some(&Ok(())) => {},
            other => panic!("Unexpected result for {:?}: {:?}", setter_id_1_3, other)
        }

        println!("* All the well-typed values should have been received.");
        match rx_adapter_1.try_recv().unwrap() {
            Effect::ValueSent(ref id, Value::OnOff(OnOff::On)) if *id == setter_id_1_3 => {},
            effect => panic!("Unexpected effect {:?}", effect)
        }

        println!("* No further value should have been received.");
        assert_matches!(rx_adapter_1.try_recv(), Err(_));
        assert_matches!(rx_adapter_2.try_recv(), Err(_));

        println!("* Sending values that cause channel errors will propagate the errors.");
        tweak_1(Tweak::InjectSetterError(setter_id_1_1.clone(), Some(Error::InternalError(InternalError::InvalidInitialService))));

        let data = manager.send_values(target_map(vec![(vec![ChannelSelector::new()], Value::OnOff(OnOff::On))]), User::None);
        assert_eq!(data.len(), 4);
        for id in vec![&setter_id_2, &setter_id_1_2, &setter_id_2] {
            match data.get(id) {
                Some(&Ok(())) => {},
                other => panic!("Unexpected result for {:?}: {:?}", id, other)
            }
        }

        for id in vec![&setter_id_1_1] {
            match data.get(id) {
                Some(&Err(Error::InternalError(InternalError::InvalidInitialService))) => {},
                other => panic!("Unexpected result for {:?}: {:?}", id, other)
            }
        }

        println!("* All the non-errored values should have been received.");
        for _ in 0..2 {
            match rx_adapter_1.try_recv().unwrap() {
                Effect::ValueSent(ref id, Value::OnOff(OnOff::On)) if *id != setter_id_1_1 => {},
                effect => panic!("Unexpected effect {:?}", effect)
            }
        }
        match rx_adapter_2.try_recv().unwrap() {
            Effect::ValueSent(ref id, Value::OnOff(OnOff::On)) if *id == setter_id_2 => {},
            effect => panic!("Unexpected effect {:?}", effect)
        }

        println!("* No further value should have been received.");
        assert_matches!(rx_adapter_1.try_recv(), Err(_));
        assert_matches!(rx_adapter_2.try_recv(), Err(_));
        tweak_1(Tweak::InjectSetterError(setter_id_1_1.clone(), None));

        if clear {
            println!("* Clearing does not break the manager.
");
            manager.stop();
        } else {
            println!("* Not clearing does not break the manager.
");
        }
    }

    println!("");
}


#[test]
fn test_watch() {
    println!("");

    for clear in vec![false, true] {
		println!("# Starting with test with clear {}.
", clear);

        let manager = AdapterManager::new(None);
        let id_1 = Id::<AdapterId>::new("adapter id 1");
        let id_2 = Id::<AdapterId>::new("adapter id 2");


        let getter_id_1_1 = Id::<Channel>::new("getter id 1.1");
        let getter_id_1_2 = Id::<Channel>::new("getter id 1.2");
        let getter_id_1_3 = Id::<Channel>::new("getter id 1.3");
        let getter_id_1_4 = Id::<Channel>::new("getter id 1.4");
        let getter_id_2 = Id::<Channel>::new("getter id 2");

        let service_id_1 = Id::<ServiceId>::new("service id 1");
        let service_id_2 = Id::<ServiceId>::new("service id 2");

        let getter_1_1 = Channel {
            id: getter_id_1_1.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let getter_1_2 = Channel {
            id: getter_id_1_2.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let getter_1_3 = Channel {
            id: getter_id_1_3.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let getter_1_4 = Channel {
            id: getter_id_1_4.clone(),
            service: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let getter_2 = Channel {
            id: getter_id_2.clone(),
            service: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            kind: ChannelKind::LightOn,
        };

        let service_1 = Service {
            id: service_id_1.clone(),
            adapter: id_1.clone(),
            tags: HashSet::new(),
            properties: HashMap::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
        };

        let service_2 = Service {
            id: service_id_2.clone(),
            adapter: id_2.clone(),
            tags: HashSet::new(),
            properties: HashMap::new(),
            getters: HashMap::new(),
            setters: HashMap::new(),
        };

        let tag_1 = Id::<TagId>::new("tag 1");

        let adapter_1 = FakeAdapter::new(&id_1);
        let adapter_2 = FakeAdapter::new(&id_2);
        let tweak_1 = adapter_1.get_tweak();
        let tweak_2 = adapter_2.get_tweak();

        let mut guards = vec![];

        println!("* Without adapters, watching values from a selector that has no channels does nothing.");
        let (tx_watch_1, rx_watch_1) = channel();
        thread::spawn(move || {
            for msg in rx_watch_1 {
                panic!("We should not have received any message {:?}", msg);
            }
        });
        guards.push(manager.watch_values(target_map(vec![(
            vec![ChannelSelector::new().with_id(Id::new("No such getter"))],
            Exactly::Always
        )]), Box::new(tx_watch_1)));

        println!("* With adapters, watching values from a selector that has no channels does nothing.");
        manager.add_adapter(Arc::new(adapter_1)).unwrap();
        manager.add_adapter(Arc::new(adapter_2)).unwrap();
        manager.add_service(service_1.clone()).unwrap();
        manager.add_service(service_2.clone()).unwrap();
        let (tx_watch, rx_watch) = channel();
        thread::spawn(move || {
            for msg in rx_watch {
                panic!("We should not have received any message {:?}", msg);
            }
        });
        guards.push(manager.watch_values(target_map(vec![(
            vec![ChannelSelector::new().with_id(Id::new("No such getter"))],
            Exactly::Always
        )]), Box::new(tx_watch)));

        println!("* We can observe channels being added.");
        let (tx_watch, rx_watch) = channel();
        let guard = manager.watch_values(target_map(vec![(
            vec![ChannelSelector::new()],
            Exactly::Always
        )]), Box::new(tx_watch)); // We keep `guard` out of `guards` to drop it manually later.

        manager.add_getter(getter_1_1.clone()).unwrap();
        manager.add_getter(getter_1_2.clone()).unwrap();
        manager.add_getter(getter_1_3.clone()).unwrap();
        manager.add_getter(getter_2.clone()).unwrap();

        let events : HashSet<_> = (0..4).map(|_| {
            match rx_watch.recv().unwrap() {
                Event::ChannelAdded(id) => id,
                other => panic!("Unexpected event {:?}", other)
            }
        }).collect();

        assert_eq!(events.len(), 4);

        assert_matches!(rx_watch.try_recv(), Err(_));

        println!("* We can observe channels being removed.");

        manager.remove_getter(&getter_id_1_2).unwrap();
        match rx_watch.recv().unwrap() {
            Event::ChannelRemoved(ref id) if *id == getter_id_1_2 => {}
            other => panic!("Unexpected event {:?}", other)
        }
        assert_matches!(rx_watch.try_recv(), Err(_));

        println!("* We can observe value changes.");
        tweak_1(Tweak::InjectGetterValue(getter_id_1_1.clone(), Ok(Some(Value::OnOff(OnOff::On)))));
        tweak_1(Tweak::InjectGetterValue(getter_id_1_2.clone(), Ok(Some(Value::OnOff(OnOff::On)))));
        tweak_1(Tweak::InjectGetterValue(getter_id_1_3.clone(), Ok(Some(Value::OnOff(OnOff::Off)))));
        let events : HashMap<_, _> = (0..2).map(|_| {
            match rx_watch.recv().unwrap() {
                Event::EnterRange {
                    from,
                    value
                } => (from, value),
                other => panic!("Unexpected event {:?}", other)
            }
        }).collect();
        assert_eq!(events.get(&getter_id_1_1).unwrap(), &Value::OnOff(OnOff::On));
        assert_eq!(events.get(&getter_id_1_3).unwrap(), &Value::OnOff(OnOff::Off));

        println!("* We only observe channels that still exist.");
        assert_matches!(rx_watch.try_recv(), Err(_));

        println!("* We can have several watchers at once");
        assert_eq!(manager.add_getter_tags(vec![
            ChannelSelector::new().with_id(getter_id_1_3.clone()),
            ChannelSelector::new().with_id(getter_id_2.clone()),
        ], vec![tag_1.clone()]), 2);

        let (tx_watch_2, rx_watch_2) = channel();
        guards.push(manager.watch_values(target_map(vec![(
            vec![
                ChannelSelector::new()
                    .with_tags(vec![tag_1.clone()])
            ],
            Exactly::Exactly(Value::Range(Box::new(Range::Eq(Value::OnOff(OnOff::On)))))
        )]), Box::new(tx_watch_2)));

        println!("* Value changes are observed on both watchers");
        tweak_1(Tweak::InjectGetterValue(getter_id_1_1.clone(), Ok(Some(Value::OnOff(OnOff::Off)))));
        tweak_1(Tweak::InjectGetterValue(getter_id_1_2.clone(), Ok(Some(Value::OnOff(OnOff::Off)))));
        tweak_1(Tweak::InjectGetterValue(getter_id_1_3.clone(), Ok(Some(Value::OnOff(OnOff::Off)))));
        tweak_2(Tweak::InjectGetterValue(getter_id_2.clone(), Ok(Some(Value::OnOff(OnOff::Off)))));
        tweak_2(Tweak::InjectGetterValue(getter_id_2.clone(), Ok(Some(Value::OnOff(OnOff::On)))));

        let mut events : HashMap<_, _> = (0..4).map(|_| {
            match rx_watch.recv().unwrap() {
                Event::EnterRange { from, value } => (from, value),
                other => panic!("Unexpected event {:?}", other)
            }
        }).collect();
        match rx_watch_2.recv().unwrap() {
            Event::EnterRange { from, value } => {
                events.insert(from, value);
            }
            other => panic!("Unexpected event {:?}", other)
        }
        assert_eq!(events.len(), 3);
        assert_matches!(rx_watch.try_recv(), Err(_));
        assert_matches!(rx_watch_2.try_recv(), Err(_));

        println!("* Watchers with ranges emit both EnterRange and ExitRange");

        tweak_2(Tweak::InjectGetterValue(getter_id_2.clone(), Ok(Some(Value::OnOff(OnOff::Off)))));
        match rx_watch_2.recv().unwrap() {
            Event::ExitRange { ref from, .. } if *from == getter_id_2 => { }
            other => panic!("Unexpected event {:?}", other)
        }
        match rx_watch.recv().unwrap() {
            Event::EnterRange { ref from, .. } if *from == getter_id_2 => { }
            other => panic!("Unexpected event {:?}", other)
        }
        assert_matches!(rx_watch.try_recv(), Err(_));
        assert_matches!(rx_watch_2.try_recv(), Err(_));


        println!("* We stop receiving value change notifications once we have dropped the guard.");
        drop(guard);
        assert_matches!(rx_watch.try_recv(), Err(_));

        tweak_1(Tweak::InjectGetterValue(getter_id_1_1.clone(), Ok(Some(Value::OnOff(OnOff::On)))));
        tweak_1(Tweak::InjectGetterValue(getter_id_1_2.clone(), Ok(Some(Value::OnOff(OnOff::On)))));
        tweak_1(Tweak::InjectGetterValue(getter_id_1_3.clone(), Ok(Some(Value::OnOff(OnOff::On)))));
        tweak_2(Tweak::InjectGetterValue(getter_id_2.clone(), Ok(Some(Value::OnOff(OnOff::On)))));

        let events : HashSet<_> = (0..2).map(|_| {
                match rx_watch_2.recv().unwrap() {
                    Event::EnterRange { from, .. } => from,
                    other => panic!("Unexpected event {:?}", other)
                }
        }).collect();
        assert_eq!(events.len(), 2);
        assert!(events.contains(&getter_id_1_3));
        assert!(events.contains(&getter_id_2));

        assert_matches!(rx_watch_2.try_recv(), Err(_));
        assert_matches!(rx_watch.try_recv(), Err(_));

        println!("* We stop receiving connection notifications once we have dropped the guard.");
        manager.add_getter(getter_1_4.clone()).unwrap();
        assert_matches!(rx_watch.try_recv(), Err(_));

        println!("* We stop receiving disconnection notifications once we have dropped the guard.");
        manager.remove_getter(&getter_id_1_4).unwrap();
        assert_matches!(rx_watch.try_recv(), Err(_));

        println!("* We are notified when a getter is added to a watch by changing a tag.");

        assert_eq!(manager.add_getter_tags(vec![
            ChannelSelector::new().with_id(getter_id_1_1.clone()),
            ChannelSelector::new().with_id(getter_id_2.clone()),
        ], vec![tag_1.clone()]), 2);
        match rx_watch_2.recv().unwrap() {
            Event::ChannelAdded(ref id) if *id == getter_id_1_1 => { }
            other => panic!("Unexpected event {:?}", other)
        }
        assert_matches!(rx_watch_2.try_recv(), Err(_));


        println!("* We are notified when a getter is removed from a watch by changing a tag.");

        assert_eq!(manager.remove_getter_tags(vec![
            ChannelSelector::new().with_id(getter_id_1_1.clone()),
        ], vec![tag_1.clone()]), 1);
        match rx_watch_2.recv().unwrap() {
            Event::ChannelRemoved(ref id) if *id == getter_id_1_1 => { }
            other => panic!("Unexpected event {:?}", other)
        }
        assert_matches!(rx_watch_2.try_recv(), Err(_));

        println!("* Make sure that we havne't forgotten to eat a message.");
        thread::sleep(std::time::Duration::new(1, 0));
        assert_matches!(rx_watch.try_recv(), Err(_));
        assert_matches!(rx_watch_2.try_recv(), Err(_));

        if clear {
            println!("* Clearing does not break the manager.
");
            manager.stop();
        } else {
            println!("* Not clearing does not break the manager.
");
        }
    }

    println!("");
}
