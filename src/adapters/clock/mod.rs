//! An adapter providing time-related services, such as the current
//! timestamp or the current time of day.

use foxbox_taxonomy::api::{ Error, InternalError, Operation, User };
use foxbox_taxonomy::channel::*;
use foxbox_taxonomy::manager::*;
use foxbox_taxonomy::services::*;
use foxbox_taxonomy::values::{ format, Duration as ValDuration, Range, TimeStamp, Value };

use transformable_channels::mpsc::*;

use std::collections::HashMap;
use std::sync::{ Arc, Mutex };

use chrono;
use chrono::{ DateTime, Duration, Local, NaiveTime, Timelike };
use timer;

static ADAPTER_NAME: &'static str = "Clock adapter (built-in)";
static ADAPTER_VENDOR: &'static str = "team@link.mozilla.org";
static ADAPTER_VERSION: [u32;4] = [0, 0, 0, 0];

#[derive(Clone)]
enum Op {
    Enter(Id<Channel>, Value),
    Exit(Id<Channel>, Value),
}

enum Movement { Enter, Exit }

pub struct Clock {
    /// Timer used to dispatch `register_watch` requests.
    timer: Mutex<timer::Timer>,

    getter_timestamp_id: Id<Channel>,
    getter_time_of_day_id: Id<Channel>,
    getter_interval_id: Id<Channel>,
}

/// A guard used to cancel watching for values.
struct Guard(Vec<timer::Guard>);
impl AdapterWatchGuard for Guard {
}

impl Clock {
    pub fn id() -> Id<AdapterId> {
        Id::new("clock@link.mozilla.org")
    }
    pub fn service_clock_id() -> Id<ServiceId> {
        Id::new("service:clock@link.mozilla.org")
    }
    pub fn getter_timestamp_id() -> Id<Channel> {
        Id::new("getter:timestamp.clock@link.mozilla.org")
    }
    pub fn getter_time_of_day_id() -> Id<Channel> {
        Id::new("getter:timeofday.clock@link.mozilla.org")
    }
    pub fn getter_interval_id() -> Id<Channel> {
        Id::new("getter:interval.clock@link.mozilla.org")
    }
}
impl Adapter for Clock {
    fn id(&self) -> Id<AdapterId> {
        Self::id()
    }

    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    fn vendor(&self) -> &str {
        ADAPTER_VENDOR
    }

    fn version(&self) -> &[u32;4] {
        &ADAPTER_VERSION
    }

    fn fetch_values(&self, mut set: Vec<Id<Channel>>, _: User) -> ResultMap<Id<Channel>, Option<Value>, Error> {
        set.drain(..).map(|id| {
            if id == self.getter_timestamp_id {
                let date = TimeStamp::from_datetime(chrono::UTC::now());
                (id, Ok(Some(Value::TimeStamp(date))))
            } else if id == self.getter_time_of_day_id {
                use chrono::Timelike;
                let date = chrono::Local::now();
                let duration = chrono::Duration::seconds(date.num_seconds_from_midnight() as i64);
                (id, Ok(Some(Value::Duration(ValDuration::from(duration)))))
            } else {
                (id.clone(), Err(Error::InternalError(InternalError::NoSuchChannel(id))))
            }
        }).collect()
    }

    fn send_values(&self, mut values: HashMap<Id<Channel>, Value>, _: User) -> ResultMap<Id<Channel>, (), Error> {
        values.drain()
            .map(|(id, _)| {
                (id.clone(), Err(Error::InternalError(InternalError::NoSuchChannel(id))))
            })
            .collect()
    }

    fn register_watch(&self, mut watch: Vec<WatchTarget>) -> WatchResult
    {
        watch.drain(..).map(|(id, filter, tx)| {
            let tx = tx.map(|msg| {
                match msg {
                    Op::Enter(id, value) => {
                        WatchEvent::Enter {
                            id: id,
                            value: value
                        }
                    },
                    Op::Exit(id, value) => {
                        WatchEvent::Exit {
                            id: id,
                            value: value
                        }
                    },
                }
            });
            (id.clone(), match filter {
                Some(Value::Range(range)) => self.aux_register_watch(&id, &*range, Box::new(tx.clone())),
                _ => Err(Error::GetterRequiresThresholdForWatching(id)),
            })
        }).collect()
    }
}

impl Clock {
    fn aux_register_watch(&self, id: &Id<Channel>, range: &Range, tx: Box<ExtSender<Op>>)
        -> Result<Box<AdapterWatchGuard>, Error>
    {
        match () {
            _ if *id == self.getter_time_of_day_id => self.aux_register_watch_timeofday(id, range, tx),
            _ if *id == self.getter_timestamp_id => self.aux_register_watch_timestamp(id, range, tx),
            _ if *id == self.getter_interval_id => self.aux_register_watch_interval(id, range, tx),
            _ => Err(Error::OperationNotSupported(Operation::Watch, id.clone()))
        }
    }

    fn aux_register_watch_interval(&self, id: &Id<Channel>, range: &Range, tx: Box<ExtSender<Op>>)
        -> Result<Box<AdapterWatchGuard>, Error>
    {
        use foxbox_taxonomy::values::Range::*;

        // Determine when to call the trigger.
        let duration = match *range {
            Eq (ref val) | Geq (ref val) => {
                try!(val.as_duration().map_err(Error::TypeError))
                    .clone().into()
            }
            _ => return Err(Error::InvalidValue)
        };

        debug!(target: "clock@link.mozilla.org", "[clock@link.mozilla.org] Scheduling a repeating watch with a duration of {}", duration);

        let id = id.clone();
        let guard = self.timer.lock().unwrap().schedule_repeating(duration, move || {
            // Send Enter followed immediately by Exit, to make sure that Thinkerbell
            // rules reset themselves.
            let _ = tx.send(Op::Enter(id.clone(),
                Value::Duration(ValDuration::from(duration))));
            let _ = tx.send(Op::Exit(id.clone(),
                Value::Duration(ValDuration::from(duration))));
        });
        Ok(Box::new(Guard(vec![guard])))
    }

    fn aux_register_watch_timeofday(&self, id: &Id<Channel>, range: &Range, tx: Box<ExtSender<Op>>)
        -> Result<Box<AdapterWatchGuard>, Error>
    {
        use foxbox_taxonomy::values::Range::*;

        // Determine when to call the trigger. Repeat duration is always one day.
        let mut thresholds = match *range {
            Leq (ref val) => {
                // Equivalent to BetweenEq { min: 0am, max: val }
                let ts : chrono::Duration = try!(val.as_duration().map_err(Error::TypeError))
                    .clone().into();
                vec![(Movement::Enter, chrono::Duration::seconds(0)), (Movement::Exit, ts)]
            }
            Geq (ref val) => {
                // Equivalent to BetweenEq { min: val, max: 0am }
                let ts = try!(val.as_duration().map_err(Error::TypeError))
                    .clone().into();
                vec![(Movement::Enter, ts), (Movement::Exit, Duration::days(1))]
            }
            BetweenEq { ref min, ref max } => {
                let ts_min = try!(min.as_duration().map_err(Error::TypeError))
                    .clone().into();
                let ts_max = try!(max.as_duration().map_err(Error::TypeError))
                    .clone().into();
                vec![(Movement::Enter, ts_min), (Movement::Exit, ts_max)]
            }
            OutOfStrict { ref min, ref max } => {
                // Equivalent to BetweenEq {min: 0am, max: min} and BetweenEq {min: max, max: 0am}
                let ts_min = try!(min.as_duration().map_err(Error::TypeError))
                    .clone().into();
                let ts_max = try!(max.as_duration().map_err(Error::TypeError))
                    .clone().into();
                vec![(Movement::Exit, ts_min), (Movement::Enter, ts_max)]
            }
            Eq (ref val) => {
                let ts : chrono::Duration = try!(val.as_duration().map_err(Error::TypeError))
                    .clone().into();
                vec![(Movement::Enter, ts.clone()), (Movement::Exit, ts)]
            }
        };

        // Determine when the next timers needs to launch.
        let now = chrono::Local::now();
        let guards : Vec<timer::Guard> = thresholds.drain(..).filter_map(|(movement, threshold)| {
            let date = match Self::get_next_date(&now, threshold) {
                Err(_) => return None,
                Ok(date) => date,
            };
            let id = id.clone();
            let tx = tx.clone();
            let guard = self.timer.lock().unwrap().schedule(date, Some(Duration::days(1)), move || {
                let naive_time = chrono::Local::now().time();
                let duration = Duration::hours(naive_time.hour() as i64)
                    + Duration::minutes(naive_time.minute() as i64)
                    + Duration::seconds(naive_time.second() as i64);

                let event = match movement {
                    Movement::Enter => Op::Enter(id.clone(),
                        Value::Duration(ValDuration::from(duration))),
                    Movement::Exit => Op::Exit(id.clone(),
                        Value::Duration(ValDuration::from(duration))),
                };
                let _ = tx.send(event);
            });
            Some(guard)
        }).collect();
        Ok(Box::new(Guard(guards)))
    }

    fn get_next_date(now: &DateTime<Local>, time_of_day: Duration)
        -> Result<DateTime<Local>, Error>
    {
        match chrono::Local::today().and_time(NaiveTime::from_hms(0, 0, 0) + time_of_day) {
            None => Err(Error::InvalidValue),
            Some(date) => {
                if date >= *now  {
                    Ok(date)
                } else {
                    // Otherwise, shift to tomorrow.
                    match date.checked_add(Duration::days(1)) {
                        None => Err(Error::InvalidValue),
                        Some(date) => Ok(date)
                    }
                }
            }
        }
    }

    fn aux_register_watch_timestamp(&self, id: &Id<Channel>, range: &Range, tx: Box<ExtSender<Op>>)
        -> Result<Box<AdapterWatchGuard>, Error>
    {
        use foxbox_taxonomy::values::Range::*;

        // Now determine when/if to call the trigger.
        let mut thresholds = match *range {
            Leq (_) => {
                // This variant doesn't make sense.
                return Ok(Box::new(Guard(vec![])))
            }
            Geq (ref val) | Eq (ref val) => {
                let ts = *try!(val.as_timestamp().map_err(Error::TypeError))
                    .as_datetime();
                vec![(Movement::Enter, ts)]
            }
            OutOfStrict { ref min, ref max } => {
                let ts_min = *try!(min.as_timestamp().map_err(Error::TypeError))
                    .as_datetime();
                let ts_max = *try!(max.as_timestamp().map_err(Error::TypeError))
                    .as_datetime();
                vec![(Movement::Exit, ts_min), (Movement::Enter, ts_max)]
            }
            BetweenEq { ref min, ref max } => {
                let ts_min = *try!(min.as_timestamp().map_err(Error::TypeError))
                    .as_datetime();
                let ts_max = *try!(max.as_timestamp().map_err(Error::TypeError))
                    .as_datetime();
                vec![(Movement::Enter, ts_min), (Movement::Exit, ts_max)]
            }
        };

        // Determine when/if the next timers needs to launch.
        let now = chrono::UTC::now();
        let guards : Vec<timer::Guard> = thresholds.drain(..).filter_map(|(movement, date)| {
            if date < now {
                return None
            }
            let id = id.clone();
            let tx = tx.clone();
            let guard = self.timer.lock().unwrap().schedule_with_date(date, move || {
                let naive_time = chrono::Local::now().time();
                let duration = Duration::hours(naive_time.hour() as i64)
                    + Duration::minutes(naive_time.minute() as i64)
                    + Duration::seconds(naive_time.second() as i64);

                let event = match movement {
                    Movement::Enter => Op::Enter(id.clone(),
                        Value::Duration(ValDuration::from(duration))),
                    Movement::Exit => Op::Exit(id.clone(),
                        Value::Duration(ValDuration::from(duration))),
                };
                let _ = tx.send(event);
            });
            Some(guard)
        }).collect();
        Ok(Box::new(Guard(guards)))
    }
}

impl Clock {
    pub fn init(adapt: &Arc<AdapterManager>) -> Result<(), Error> {
        let getter_timestamp_id = Clock::getter_timestamp_id();
        let getter_time_of_day_id = Clock::getter_time_of_day_id();
        let getter_interval_id = Clock::getter_interval_id();
        let service_clock_id = Clock::service_clock_id();
        let adapter_id = Clock::id();
        let clock = Arc::new(Clock {
            timer: Mutex::new(timer::Timer::new()),
            getter_timestamp_id: getter_timestamp_id.clone(),
            getter_time_of_day_id: getter_time_of_day_id.clone(),
            getter_interval_id: getter_interval_id.clone(),
        });
        try!(adapt.add_adapter(clock));
        let mut service = Service::empty(&service_clock_id, &adapter_id);
        service.properties.insert("model".to_owned(), "Mozilla clock v1".to_owned());
        try!(adapt.add_service(service));
        try!(adapt.add_channel(Channel {
            feature: Id::new("clock/time-of-day-seconds"),
            supports_fetch: Some(Signature::returns(Maybe::Required(format::DURATION.clone()))),
            supports_watch: Some(Signature {
                accepts: Maybe::Required(format::DURATION.clone()),
                returns: Maybe::Required(format::DURATION.clone())
            }),
            id: getter_time_of_day_id,
            service: service_clock_id.clone(),
            adapter: adapter_id.clone(),
            ..Channel::default()
        }));
        try!(adapt.add_channel(Channel {
            feature: Id::new("clock/time-timestamp-rfc-3339"),
            supports_fetch: Some(Signature::returns(Maybe::Required(format::TIMESTAMP.clone()))),
            supports_watch: Some(Signature {
                accepts: Maybe::Required(format::TIMESTAMP.clone()),
                returns: Maybe::Required(format::TIMESTAMP.clone())
            }),
            id: getter_timestamp_id,
            service: service_clock_id.clone(),
            adapter: adapter_id.clone(),
            ..Channel::default()
        }));
        try!(adapt.add_channel(Channel {
            feature: Id::new("clock/time-interval-seconds"),
            supports_watch: Some(Signature {
                accepts: Maybe::Required(format::DURATION.clone()),
                returns: Maybe::Required(format::TIMESTAMP.clone())
            }),
            id: getter_interval_id,
            service: service_clock_id.clone(),
            adapter: adapter_id.clone(),
            ..Channel::default()
        }));
        Ok(())
    }
}
