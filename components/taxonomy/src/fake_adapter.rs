//! An adapter to a non-existing device, whose state is entirely controlled programmatically.
//! Used for testing.
use adapter::*;

use api::{ Error, User };
use channel::Channel;
use services::*;
use values::*;

use transformable_channels::mpsc::*;

use std::cell::RefCell;
use std::collections::HashMap ;
use std::collections::hash_map::Entry::*;
use std::sync::{ Arc, Mutex };
use std::sync::atomic::{ AtomicBool, Ordering} ;
use std::thread;

/// A tweak sent to the virtual device, to set a value, inject an error, ...
#[allow(enum_variant_names)]
pub enum Tweak {
    /// Inject a value in a virtual getter.
    InjectGetterValue(Id<Channel>, Result<Option<Value>, Error>),

    /// Inject an error in a virtual setter. All operations on this setter will
    /// raise the error until `None` is injected instead.
    InjectSetterError(Id<Channel>, Option<Error>)
}

/// Something that happened to the virtual device, e.g. a value was sent.
#[derive(Debug)]
pub enum Effect {
    ValueSent(Id<Channel>, Value)
}

fn dup<T>(t: T) -> (T, T) where T: Clone {
    (t.clone(), t)
}

struct TestWatchGuard(Arc<AtomicBool>);
impl AdapterWatchGuard for TestWatchGuard {}
impl Drop for TestWatchGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed)
    }
}

type SyncMap<K, V> = Arc<Mutex<HashMap<K, V>>>;

struct WatcherState {
    filter: Option<Box<Range>>,
    on_event: Box<ExtSender<WatchEvent<Value>>>,
    is_met: RefCell<bool>, /* is_met*/
    is_dropped: Arc<AtomicBool>, /* is_dropped */
}
pub struct FakeAdapter {
    id: Id<AdapterId>,
    name: String,
    tweak: Arc<Fn(Tweak) + Sync + Send>,
    tx_effect: Mutex<Box<ExtSender<Effect>>>,
    rx_effect: Mutex<Option<Receiver<Effect>>>,
    values: SyncMap<Id<Channel>, Result<Value, Error>>,
    senders: SyncMap<Id<Channel>, Error>,
    watchers: SyncMap<Id<Channel>, Vec<WatcherState>>
}

impl FakeAdapter {
    pub fn new(id: &Id<AdapterId>) -> Self {
        let (tx, rx) : (RawSender<(Tweak, RawSender<()>)>, _) = channel();
        let (tx_effect, rx_effect) = channel();

        let (values_main, values_thread) = dup(Arc::new(Mutex::new(HashMap::new())));
        let (senders_main, senders_thread) = dup(Arc::new(Mutex::new(HashMap::new())));
        let (watchers_main, watchers_thread) = dup(Arc::new(Mutex::new(HashMap::new())));

        let mutex = Arc::new(Mutex::new(tx));
        let tweak = move |msg| {
            let (tx, rx) = channel();
            mutex.lock().unwrap().send((msg, tx)).unwrap();
            rx.recv().unwrap();
        };
        let result = FakeAdapter {
            id: id.clone(),
            name: id.as_atom().to_string().clone(),
            values: values_main,
            senders: senders_main,
            tweak: Arc::new(tweak),
            tx_effect: Mutex::new(Box::new(tx_effect)),
            rx_effect: Mutex::new(Some(rx_effect)),
            watchers: watchers_main,
        };

        thread::spawn(move || {
            use self::Tweak::*;
            for (msg, tx) in rx {
                match msg {
                    InjectGetterValue(id, Ok(Some(value))) => {
                        values_thread.lock().unwrap().insert(id.clone(), Ok(value.clone()));
                        if let Some(watchers) = watchers_thread.lock().unwrap().get(&id) {
                            for watcher in watchers {
                                if watcher.is_dropped.load(Ordering::Relaxed) {
                                    continue;
                                }
                                match watcher.filter {
                                    None => {
                                        watcher.on_event.send(WatchEvent::Enter {
                                            id: id.clone(),
                                            value: value.clone()
                                        }).unwrap();
                                    }
                                    Some(ref range) => {
                                        match (range.contains(&value), *watcher.is_met.borrow()) {
                                            (true, false) => {
                                                watcher.on_event.send(WatchEvent::Enter {
                                                    id: id.clone(),
                                                    value: value.clone()
                                                }).unwrap();
                                            }
                                            (false, true) => {
                                                watcher.on_event.send(WatchEvent::Exit {
                                                    id: id.clone(),
                                                    value: value.clone()
                                                }).unwrap();
                                            }
                                            _ => {}
                                        }
                                        *watcher.is_met.borrow_mut() = range.contains(&value);
                                    }
                                }
                            }
                        }
                    },
                    InjectGetterValue(id, Err(error)) => {
                        values_thread.lock().unwrap().insert(id, Err(error));
                    },
                    InjectGetterValue(id, Ok(None)) => {
                        values_thread.lock().unwrap().remove(&id);
                    },
                    InjectSetterError(id, None) => {
                        senders_thread.lock().unwrap().remove(&id);
                    },
                    InjectSetterError(id, Some(err)) => {
                        senders_thread.lock().unwrap().insert(id, err);
                    }
                }
                tx.send(()).unwrap();
            }
        });
        result
    }

    pub fn take_rx(&self) -> Receiver<Effect> {
        self.rx_effect.lock().unwrap().take().unwrap()
    }

    pub fn get_tweak(&self) -> Arc<Fn(Tweak) + Sync + Send> {
        self.tweak.clone()
    }
}

static VERSION : [u32;4] = [0, 0, 0, 0];

impl Adapter for FakeAdapter {
    /// An id unique to this adapter. This id must persist between
    /// reboots/reconnections.
    fn id(&self) -> Id<AdapterId> {
        self.id.clone()
    }

    /// The name of the adapter.
    fn name(&self) -> &str {
        &self.name
    }

    fn vendor(&self) -> &str {
        "test@foxbox_adapters"
    }

    fn version(&self) -> &[u32;4] {
        &VERSION
    }

    /// Request a value from a channel. The `FoxBox` (not the adapter)
    /// is in charge of keeping track of the age of values.
    fn fetch_values(&self, mut channels: Vec<Id<Channel>>, _: User) -> ResultMap<Id<Channel>, Option<Value>, Error> {
        let map = self.values.lock().unwrap();
        channels.drain(..).map(|id| {
            let result = match map.get(&id) {
                None => Ok(None),
                Some(&Ok(ref value)) => Ok(Some(value.clone())),
                Some(&Err(ref error)) => Err(error.clone())
            };
            (id, result)
        }).collect()
    }

    /// Request that a value be sent to a channel.
    fn send_values(&self, mut values: HashMap<Id<Channel>, Value>, _: User) -> ResultMap<Id<Channel>, (), Error> {
        let map = self.senders.lock().unwrap();
        values.drain().map(|(id, value)| {
            let result = match map.get(&id) {
                None => {
                    self.tx_effect.lock().unwrap().send(Effect::ValueSent(id.clone(), value)).unwrap();
                    Ok(())
                }
                Some(error) => Err(error.clone())
            };
            (id, result)
        }).collect()
    }

    fn register_watch(&self, mut watch: Vec<WatchTarget>) -> WatchResult {
        let mut watchers = self.watchers.lock().unwrap();
        watch.drain(..).filter_map(|(id, filter, on_event)| {
            let is_dropped = Arc::new(AtomicBool::new(false));
            let range = match filter {
                None => None,
                Some(Value::Range(range)) => Some(range),
                Some(_) => {
                    // Ignore.
                    // FIXME: Log?
                    return None
                }
            };
            let watcher = WatcherState {
                filter: range,
                on_event: on_event,
                is_met: RefCell::new(false),
                is_dropped: is_dropped.clone()
            };
            match watchers.entry(id.clone()) {
                Occupied(mut entry) => {
                    entry.get_mut().push(watcher)
                }
                Vacant(entry) => {
                    entry.insert(vec![watcher]);
                }
            }
            let guard = Box::new(TestWatchGuard(is_dropped.clone())) as Box<AdapterWatchGuard>;
            Some((id, Ok(guard)))
        }).collect()
    }
}
