use compile::ExecutableDevEnv;

use foxbox_taxonomy::api::{ API, Error, User };
use foxbox_taxonomy::channel::*;
use foxbox_taxonomy::manager::*;
use foxbox_taxonomy::parse::*;
use foxbox_taxonomy::services::*;
use foxbox_taxonomy::values::*;

use std::cmp::{ Ord, PartialOrd, Ordering as OrdOrdering };
use std::fmt;
use std::collections::{ BinaryHeap, HashMap };
use std::sync::{ Arc, Mutex };
use std::sync::atomic::{ AtomicBool, Ordering as AtomicOrdering };
use std::thread;

use transformable_channels::mpsc::*;

use chrono::{ DateTime, UTC };


static VERSION : [u32;4] = [0, 0, 0, 0];

/// A back-end holding values and watchers, shared by all the virtual adapters of this simulator.
struct TestSharedAdapterBackend {
    /// The latest known value for each getter.
    getter_values: HashMap<Id<Channel>, Result<Value, Error>>,
    setter_errors: HashMap<Id<Channel>, Error>,

    /// A channel to inform when things happen.
    on_event: Box<ExtSender<FakeEnvEvent>>,

    /// All watchers. 100% counter-optimized
    counter: usize,

    timers: BinaryHeap<Timer>,
    trigger_timers_until: Option<DateTime<UTC>>,

    watchers: HashMap<usize, (Id<Channel>, Option<Box<Range>>, Box<ExtSender<WatchEvent<Value>>>)>,
}

impl TestSharedAdapterBackend {
    fn new(on_event: Box<ExtSender<FakeEnvEvent>>) -> Self {
        TestSharedAdapterBackend {
            getter_values: HashMap::new(),
            setter_errors: HashMap::new(),
            on_event: on_event,
            counter: 0,
            watchers: HashMap::new(),
            timers: BinaryHeap::new(),
            trigger_timers_until: None,
        }
    }

    fn fetch_values(&self, mut getters: Vec<Id<Channel>>, _: User) -> ResultMap<Id<Channel>, Option<Value>, Error> {
        getters.drain(..).map(|id| {
            let got = self.getter_values.get(&id).cloned();
            match got {
                None => (id, Ok(None)),
                Some(Ok(value)) => (id, Ok(Some(value))),
                Some(Err(err)) => (id, Err(err)),
            }
        }).collect()
    }

    fn send_values(&self, mut values: HashMap<Id<Channel>, Value>, _: User) -> ResultMap<Id<Channel>, (), Error> {
        values.drain().map(|(id, value)| {
            match self.setter_errors.get(&id) {
                None => {
                    let event = FakeEnvEvent::Send {
                        id: id.clone(),
                        value: value
                    };
                    let _ = self.on_event.send(event);
                    (id, Ok(()))
                }
                Some(error) => {
                    (id, Err(error.clone()))
                }
            }
        }).collect()
    }

    fn register_watch(&mut self, mut source: Vec<(Id<Channel>, Option<Value>, Box<ExtSender<WatchEvent<Value>>>)>) ->
            Vec<(Id<Channel>, Result<usize, Error>)>
    {
        let results = source.drain(..).filter_map(|(id, range, tx)| {
            let range = match range {
                Some(Value::Range(range)) => Some(range),
                None => None,
                _ => return None, // FIXME: Log
            };
            let result = (id.clone(), Ok(self.counter));
            self.watchers.insert(self.counter, (id, range, tx));
            self.counter += 1;
            Some(result)
        }).collect();
        results
    }

    fn remove_watch(&mut self, key: usize) {
        let _ = self.watchers.remove(&key);
    }

    fn inject_setter_errors(&mut self, mut errors: Vec<(Id<Channel>, Option<Error>)>)
    {
        for (id, error) in errors.drain(..) {
            match error {
                None => {
                    self.setter_errors.remove(&id);
                },
                Some(error) => {
                    self.setter_errors.insert(id, error);
                }
            }
        }
    }

    fn inject_getter_values(&mut self, mut values: Vec<(Id<Channel>, Result<Value, Error>)>)
    {
        for (id, value) in values.drain(..) {
            let old = self.getter_values.insert(id.clone(), value.clone());
            match value {
                Err(_) => continue,
                Ok(value) => {
                    for watcher in self.watchers.values() {
                        let (ref watched_id, ref range, ref cb) = *watcher;
                        if *watched_id != id {
                            continue;
                        }
                        if let Some(ref range) = *range {
                            let was_met = if let Some(Ok(ref value)) = old {
                                range.contains(value)
                            } else {
                                false
                            };
                            match (was_met, range.contains(&value)) {
                                (false, true) => {
                                    let _ = cb.send(WatchEvent::Enter {
                                        id: id.clone(),
                                        value: value.clone()
                                    });
                                }
                                (true, false) => {
                                    let _ = cb.send(WatchEvent::Exit {
                                        id: id.clone(),
                                        value: value.clone()
                                    });
                                }
                                _ => continue
                            }
                        } else {
                            let _ = cb.send(WatchEvent::Enter {
                                id: id.clone(),
                                value: value.clone()
                            });
                        }
                    }
                }
            }
        }
    }

    fn trigger_timers_until(&mut self, date: DateTime<UTC>) {
        self.trigger_timers_until = Some(date);
        loop {
            if let Some(ref timer) = self.timers.peek() {
                if timer.date > date {
                    break;
                }
            } else {
                break;
            }
            self.timers.pop().unwrap().trigger();
        }
    }

    fn execute(&mut self, op: AdapterOp) {
        use self::AdapterOp::*;
        match op {
            FetchValues { getters, tx } => {
                let _ = tx.send(self.fetch_values(getters, User::None));
            }
            SendValues { values, tx } => {
                let _ = tx.send(self.send_values(values, User::None));
            }
            Watch { source, tx } => {
                let _ = tx.send(self.register_watch(source));
            }
            Unwatch(key) => {
                let _ = self.remove_watch(key);
            }
            InjectGetterValues(values, tx) => {
                let _ = self.inject_getter_values(values);
                let _ = tx.send(FakeEnvEvent::Done);
            }
            InjectSetterErrors(errors, tx) => {
                let _ = self.inject_setter_errors(errors);
                let _ = tx.send(FakeEnvEvent::Done);
            }
            TriggerTimersUntil(date ,tx) => {
                let _ = self.trigger_timers_until(date.into());
                let _ = tx.send(FakeEnvEvent::Done);
            }
            ResetTimers(tx) => {
                self.trigger_timers_until = None;
                self.timers.clear();
                let _ = tx.send(FakeEnvEvent::Done);
            }
            AddTimer(timer) => {
                match self.trigger_timers_until {
                    None => {
                        self.timers.push(timer);
                    }
                    Some(ref date) if *date < timer.date => {
                        self.timers.push(timer);
                    }
                    Some(_) => {
                        timer.trigger()
                    }
                }
            }
        }
    }
}

/// Events of interest, that should be displayed to the user of the simulator.
#[derive(Debug)]
pub enum FakeEnvEvent {
    /// Some value was sent to a setter channel.
    Send {
        id: Id<Channel>,
        value: Value
    },

    /// Some error took place.
    Error(Error),

    /// Handling of an instruction is complete.
    Done,
}

/// An adapter for the simulator.
struct TestAdapter {
    id: Id<AdapterId>,

    /// The back-end holding the state of this adapter. Shared between all adapters.
    back_end: Arc<Mutex<Box<ExtSender<AdapterOp>>>>,
}
impl TestAdapter {
    fn new(id: Id<AdapterId>, back_end: Box<ExtSender<AdapterOp>>) -> Self {
        TestAdapter {
            id: id,
            back_end: Arc::new(Mutex::new(back_end)),
        }
    }
}

impl Adapter for TestAdapter {
    /// An id unique to this adapter. This id must persist between
    /// reboots/reconnections.
    fn id(&self) -> Id<AdapterId> {
        self.id.clone()
    }

    /// The name of the adapter.
    fn name(&self) -> &str {
        "Test Adapter"
    }
    fn vendor(&self) -> &str {
        "test@foxlink"
    }
    fn version(&self) -> &[u32;4] {
        &VERSION
    }
    // ... more metadata

    /// Request values from a group of channels.
    ///
    /// The AdapterManager always attempts to group calls to `fetch_values` by `Adapter`, and then
    /// expects the adapter to attempt to minimize the connections with the actual devices.
    ///
    /// The AdapterManager is in charge of keeping track of the age of values.
    fn fetch_values(&self, getters: Vec<Id<Channel>>, _: User) -> ResultMap<Id<Channel>, Option<Value>, Error> {
        let (tx, rx) = channel();
        self.back_end.lock().unwrap().send(AdapterOp::FetchValues {
            getters: getters,
            tx: Box::new(tx),
        }).unwrap();
        rx.recv().unwrap()
    }

    /// Request that values be sent to channels.
    ///
    /// The AdapterManager always attempts to group calls to `send_values` by `Adapter`, and then
    /// expects the adapter to attempt to minimize the connections with the actual devices.
    fn send_values(&self, values: HashMap<Id<Channel>, Value>, _: User) -> ResultMap<Id<Channel>, (), Error> {
        let (tx, rx) = channel();
        self.back_end.lock().unwrap().send(AdapterOp::SendValues {
            values: values,
            tx: Box::new(tx),
        }).unwrap();
        rx.recv().unwrap()
    }

    /// Watch a bunch of getters as they change.
    ///
    /// The `AdapterManager` always attempts to group calls to `fetch_values` by `Adapter`, and
    /// then expects the adapter to attempt to minimize the connections with the actual devices.
    /// The Adapter should however be ready to handle concurrent `register_watch` on the same
    /// devices, possibly with distinct `Option<Range>` options.
    ///
    /// If a `Range` option is set, the watcher expects to receive `EnterRange`/`ExitRange` events
    /// whenever the value available on the device enters/exits the range. If the `Range` is
    /// a `Range::Eq(x)`, the adapter may decide to reject the request or to interpret it as
    /// a `Range::BetweenEq { min: x, max: x }`.
    ///
    /// If no `Range` option is set, the watcher expects to receive `EnterRange` events whenever
    /// a new value is available on the device. The adapter may decide to reject the request if
    /// this is clearly not the expected usage for a device, or to throttle it.
    fn register_watch(&self, source: Vec<(Id<Channel>, Option<Value>, Box<ExtSender<WatchEvent<Value>>>)>) ->
            Vec<(Id<Channel>, Result<Box<AdapterWatchGuard>, Error>)>
    {
        let (tx, rx) = channel();
        self.back_end.lock().unwrap().send(AdapterOp::Watch {
            source: source,
            tx: Box::new(tx),
        }).unwrap();
        let received = rx.recv().unwrap();
        let tx_unregister = self.back_end.lock().unwrap().clone();
        received.iter().map(|&(ref id, ref result)| {
            let tx_unregister = tx_unregister.clone();
            (id.clone(), match *result {
                Err(ref err) => Err(err.clone()),
                Ok(ref key) => {
                    let guard = TestAdapterWatchGuard {
                        tx: Arc::new(Mutex::new(Box::new(tx_unregister))),
                        key: key.clone()
                    };
                    Ok(Box::new(guard) as Box<AdapterWatchGuard>)
                }
            })
        }).collect()
    }
}

/// A watchguard for a TestAdapter
#[derive(Clone)]
struct TestAdapterWatchGuard {
    tx: Arc<Mutex<Box<ExtSender<AdapterOp>>>>,
    key: usize,
}
impl AdapterWatchGuard for TestAdapterWatchGuard {}
impl Drop for TestAdapterWatchGuard {
    fn drop(&mut self) {
        let _ = self.tx.lock().unwrap().send(AdapterOp::Unwatch(self.key));
    }
}

#[derive(Clone)]
struct Timer {
    is_dropped: Arc<AtomicBool>,
    date: DateTime<UTC>,
    on_triggered: Box<ExtSender<()>>,
}
impl Timer {
    fn trigger(&self) {
        if self.is_dropped.load(AtomicOrdering::Relaxed) {
            return;
        }
        let _ = self.on_triggered.send(());
    }
}
impl Eq for Timer {}
impl Ord for Timer {
    fn cmp(&self, other: &Self) -> OrdOrdering {
        self.date.cmp(&other.date).reverse()
    }
}
impl PartialEq for Timer {
    fn eq(&self, other: &Self) -> bool {
        self.date == other.date
    }
}
impl PartialOrd for Timer {
    fn partial_cmp(&self, other: &Self) -> Option<OrdOrdering> {
        Some(self.cmp(other))
    }
}

pub struct TimerGuard(Arc<AtomicBool>);
impl Drop for TimerGuard {
    fn drop(&mut self) {
        self.0.store(true, AtomicOrdering::Relaxed)
    }
}

/// The test environment.
#[derive(Clone)]
pub struct FakeEnv {
    /// The manager in charge of all adapters.
    manager: Arc<AdapterManager>,

    ///
    on_event: Box<ExtSender<FakeEnvEvent>>,
    back_end: Box<ExtSender<AdapterOp>>,
}
impl fmt::Debug for FakeEnv {
    fn fmt(&self, _: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        Ok(())
    }
}
impl ExecutableDevEnv for FakeEnv {
    // Don't bother stopping watches.
    type WatchGuard = <AdapterManager as API>::WatchGuard;
    type API = AdapterManager;

    fn api(&self) -> &Self::API {
        &self.manager
    }

    type TimerGuard = TimerGuard;
    fn start_timer(&self, duration: Duration, timer: Box<ExtSender<()>>) -> Self::TimerGuard {
        let is_dropped = Arc::new(AtomicBool::new(false));
        let trigger = Timer {
            date: UTC::now() + duration.into(),
            on_triggered: timer,
            is_dropped: is_dropped.clone()
        };
        let _ = self.back_end.send(AdapterOp::AddTimer(trigger));
        TimerGuard(is_dropped)
    }
}
impl FakeEnv {
    pub fn new(on_event: Box<ExtSender<FakeEnvEvent>>) -> Self {
        let (tx, rx) = channel();
        let on_event_clone = on_event.clone();
        thread::spawn(move || {
            let mut back_end = TestSharedAdapterBackend::new(on_event_clone);
            for msg in rx {
                back_end.execute(msg);
            }
        });

        FakeEnv {
            on_event: on_event,
            manager: Arc::new(AdapterManager::new(None)),
            back_end: Box::new(tx),
        }
    }

    fn report_error<T>(&self, result: Result<T, Error>) {
        match result {
            Ok(_) => {},
            Err(err) => {
                let _ = self.on_event.send(FakeEnvEvent::Error(err));
            }
        }
    }

    /// Execute instructions issued by the user.
    pub fn execute(&self, instruction: Instruction) {
        use self::Instruction::*;
        match instruction {
            AddAdapters(vec) => {
                for id in vec {
                    let id = Id::new(&id);
                    let adapter = Arc::new(TestAdapter::new(id, self.back_end.clone()));
                    let result = self.manager.add_adapter(adapter);
                    self.report_error(result);
                }
                let _ = self.on_event.send(FakeEnvEvent::Done);
            },
            AddServices(vec) => {
                for service in vec {
                    let result = self.manager.add_service(service);
                    self.report_error(result);
                }
                let _ = self.on_event.send(FakeEnvEvent::Done);
            },
            AddChannels(vec) => {
                for channel in vec {
                    let result = self.manager.add_channel(channel);
                    self.report_error(result);
                }
                let _ = self.on_event.send(FakeEnvEvent::Done);
            },
            RemoveChannels(vec) => {
                for channel in vec {
                    let result = self.manager.remove_channel(&channel);
                    self.report_error(result);
                }
                let _ = self.on_event.send(FakeEnvEvent::Done);
            },
            InjectGetterValues(vec) => {
                self.back_end.send(AdapterOp::InjectGetterValues(vec, self.on_event.clone())).unwrap();
            },
            InjectSetterErrors(vec) => {
                self.back_end.send(AdapterOp::InjectSetterErrors(vec, self.on_event.clone())).unwrap();
            }
            TriggerTimersUntil(date) => {
                self.back_end.send(AdapterOp::TriggerTimersUntil(date, self.on_event.clone())).unwrap();
            }
            ResetTimers => {
                self.back_end.send(AdapterOp::ResetTimers(self.on_event.clone())).unwrap();
            }
//            _ => unimplemented!()
        }
    }
}

#[derive(Debug)]
/// Instructions given to the simulator by the user.
pub enum Instruction {
    AddAdapters(Vec<String>),
    AddServices(Vec<Service>),
    AddChannels(Vec<Channel>),
    RemoveChannels(Vec<Id<Channel>>),
    InjectGetterValues(Vec<(Id<Channel>, Result<Value, Error>)>),
    InjectSetterErrors(Vec<(Id<Channel>, Option<Error>)>),
    TriggerTimersUntil(TimeStamp),
    ResetTimers,
}

/// Operations internal to a TestAdapter.
enum AdapterOp {
    FetchValues {
        getters: Vec<Id<Channel>>,
        tx: Box<ExtSender<ResultMap<Id<Channel>, Option<Value>, Error>>>
    },
    SendValues {
        values: HashMap<Id<Channel>, Value>,
        tx: Box<ExtSender<ResultMap<Id<Channel>, (), Error>>>
    },
    Watch {
        source: Vec<(Id<Channel>, Option<Value>, Box<ExtSender<WatchEvent<Value>>>)>,
        tx: Box<ExtSender<Vec<(Id<Channel>, Result<usize, Error>)>>>
    },
    AddTimer(Timer),
    Unwatch(usize),
    InjectGetterValues(Vec<(Id<Channel>, Result<Value, Error>)>, Box<ExtSender<FakeEnvEvent>>),
    InjectSetterErrors(Vec<(Id<Channel>, Option<Error>)>, Box<ExtSender<FakeEnvEvent>>),
    TriggerTimersUntil(TimeStamp, Box<ExtSender<FakeEnvEvent>>),
    ResetTimers(Box<ExtSender<FakeEnvEvent>>),
}

struct SetterErrorParser;
impl Parser<(Id<Channel>, Option<Error>)> for SetterErrorParser {
    fn description() -> String {
        "SetterErrorParser".to_owned()
    }
    fn parse(path: Path, source: &JSON) -> Result<(Id<Channel>, Option<Error>), ParseError> {
        unimplemented!()
    }
}

struct GetterValueParser;
impl Parser<(Id<Channel>, Result<Value, Error>)> for GetterValueParser {
    fn description() -> String {
        "GetterValueParser".to_owned()
    }
    fn parse(path: Path, source: &JSON) -> Result<(Id<Channel>, Result<Value, Error>), ParseError> {
        unimplemented!()
    }
}


struct AddChannelsParser;
impl Parser<Instruction> for AddChannelsParser {
    fn description() -> String {
        "AddChannelsParser".to_owned()
    }
    fn parse(path: Path, source: &JSON) -> Result<Instruction, ParseError> {
        unimplemented!()
    }
}


struct AddServicesParser;
impl Parser<Instruction> for AddServicesParser {
    fn description() -> String {
        "AddServicesParser".to_owned()
    }
    fn parse(path: Path, source: &JSON) -> Result<Instruction, ParseError> {
        unimplemented!()
    }
}

impl Parser<Instruction> for Instruction {
    fn description() -> String {
        "Instruction".to_owned()
    }

    fn parse(path: Path, source: &JSON) -> Result<Instruction, ParseError> {
        if let JSON::String(ref s) = *source {
            if &*s == "ResetTimers" {
                Ok(Instruction::ResetTimers)
            } else {
                Err(ParseError::unknown_fields(vec![s.clone()], &path))
            }
        } else if let Some(result) = path.push_str("TriggerTimersUntil", |path| TimeStamp::take_opt(path, source, "TriggerTimersUntil")) {
            Ok(Instruction::TriggerTimersUntil(try!(result)))
        } else if let Some(result) = path.push_str("InjectSetterErrors", |path| SetterErrorParser::take_vec_opt(path, source, "InjectSetterErrors")) {
            Ok(Instruction::InjectSetterErrors(try!(result)))
        } else if let Some(result) = path.push_str("InjectGetterValues", |path| GetterValueParser::take_vec_opt(path, source, "InjectGetterValues")) {
            Ok(Instruction::InjectGetterValues(try!(result)))
        } else if let Some(result) = path.push_str("RemoveChannels", |path| Vec::take_opt(path, source, "RemoveChannels")) {
            Ok(Instruction::RemoveChannels(try!(result)))
        } else if let Some(result) = path.push_str("AddChannels", |path| AddChannelsParser::take_opt(path, source, "AddChannels")) {
            result
        } else if let Some(result) = path.push_str("AddServices", |path| AddServicesParser::take_opt(path, source, "AddServices")) {
            result
        } else if let Some(result) = path.push_str("AddAdapters", |path| Vec::take_opt(path, source, "AddAdapters")) {
            Ok(Instruction::AddAdapters(try!(result)))
        } else {
            unimplemented!()
        }
    }
}


