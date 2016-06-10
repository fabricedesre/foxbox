//! Launching and running the script

use ast::{ Script, Statement, UncheckedCtx } ;
use compile::{ Compiler, CompiledCtx, ExecutableDevEnv } ;
pub use compile::{ Error as CompileError, SourceError, TypeError };
use compile;

use foxbox_taxonomy::api;
use foxbox_taxonomy::api::{ API, Error as APIError, Targetted, User, WatchEvent };
use foxbox_taxonomy::channel::Channel;
use foxbox_taxonomy::util::{ Exactly, Id };
use foxbox_taxonomy::values::Duration;

use transformable_channels::mpsc::*;

use std::collections::HashSet;
use std::fmt;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::thread;
use std::sync::Mutex;

/// Running and controlling a single script.
pub struct Execution<Env> where Env: ExecutableDevEnv + Debug + 'static {
    command_sender: Option<Box<ExtSender<ExecutionOp>>>,
    phantom: PhantomData<Env>,
}

impl<Env> Execution<Env> where Env: ExecutableDevEnv + Debug + 'static {
    pub fn new() -> Self {
        Execution {
            command_sender: None,
            phantom: PhantomData,
        }
    }

    /// Start executing the script.
    ///
    /// # Memory warning
    ///
    /// If you do not consume the values from `on_event`, they will remain stored forever.
    /// You have been warned.
    ///
    /// # Errors
    ///
    /// The first event sent to `on_event` is a `ExecutionEvent::Starting`, which informs the
    /// caller of whether the execution could start. Possible reasons that would prevent execution
    /// are:
    /// - `RunningError:AlreadyRunning` if the script is already running;
    /// - a compilation error if the script was incorrect.
    pub fn start<S>(&mut self, env: Env, script: Script<UncheckedCtx>, owner: User, on_event: S) ->
        Result<(), Error>
        where S: ExtSender<ExecutionEvent> + Clone
    {
        let name = script.name.clone();
        info!("[Recipe '{}'] Starting compilation of script.", name);
        if self.command_sender.is_some() {
            let err = Err(Error::StartStopError(StartStopError::AlreadyRunning));
            let _ = on_event.send(ExecutionEvent::Starting {
                result: err.clone()
            });
            err
        } else {
            // One-time channel, used to wait until compilation is complete.
            let (tx_init, rx_init) = channel();

            let (tx, rx) = channel();
            self.command_sender = Some(Box::new(tx.clone()));
            thread::spawn(move || {
                match ExecutionTask::<Env>::new(script, owner, tx, rx) {
                    Err(er) => {
                        info!("[Recipe '{}'] Compilation failed {:?}", name, er);
                        let _ = on_event.send(ExecutionEvent::Starting {
                            result: Err(er.clone())
                        });
                        let _ = tx_init.send(Err(er));
                    },
                    Ok(mut task) => {
                        info!("[Recipe '{}'] Compilation succeeded.", name);
                        let _ = on_event.send(ExecutionEvent::Starting {
                            result: Ok(())
                        });
                        task.run(env, on_event, &tx_init);
                    }
                }
            });
            match rx_init.recv() {
                Ok(result) => result,
                Err(_) => Err(Error::StartStopError(StartStopError::ThreadError))
            }
        }
    }


    /// Stop executing the script, asynchronously.
    ///
    /// # Errors
    ///
    /// Produces RunningError:NotRunning if the script is not running yet.
    pub fn stop<F>(&mut self, on_result: F) where F: Fn(Result<(), Error>) + Send + 'static {
        match self.command_sender {
            None => {
                /* Nothing to stop */
                on_result(Err(Error::StartStopError(StartStopError::NotRunning)));
            },
            Some(ref tx) => {
                // Shutdown the application, asynchronously.
                let _ignored = tx.send(ExecutionOp::Stop(Mutex::new(Box::new(on_result))));
            }
        };
        self.command_sender = None;
    }
}

impl<Env> Drop for Execution<Env> where Env: ExecutableDevEnv + Debug + 'static {
    fn drop(&mut self) {
        let _ignored = self.stop(|_ignored| { });
    }
}

/// A script ready to be executed. Each script is meant to be
/// executed in an individual thread.
pub struct ExecutionTask<Env> where Env: ExecutableDevEnv {
    script: Script<CompiledCtx<Env>>,
    owner: User,

    /// Communicating with the thread running script.
    tx: Box<ExtSender<ExecutionOp>>,
    rx: Receiver<ExecutionOp>,
}

#[derive(Debug)]
pub enum ExecutionEvent {
    Starting {
        result: Result<(), Error>,
    },
    Stopped {
        result: Result<(), Error>
    },
    Sent {
        rule_index: usize,
        statement_index: usize,
        result: Vec<(Id<Channel>, Result<(), Error>)>
    },
    TimerStart {
        rule_index: usize,
        condition_index: usize,
    },
    TimerCancel {
        rule_index: usize,
        condition_index: usize,
    },
    ChannelError {
        id: Id<Channel>,
        error: APIError,
    }
}

enum ExecutionOp {
    /// We have received an update from the AdapterManager.
    Update {
        /// The individual event.
        event: WatchEvent,

        /// The rule to which this event applies.
        rule_index: usize,

        /// The index to which this event applies.
        condition_index: usize
    },

    /// A channel state has enter/left its target range and we
    /// have waited long enough to trigger the consequences.
    UpdateCondition {
        /// The channel that has changed state.
        id: Id<Channel>,

        /// `true` if the condition is now met, `false` otherwise.
        is_met: bool,

        /// The rule to which this event applies.
        rule_index: usize,

        /// The index to which this event applies.
        condition_index: usize,
    },

    /// Time to stop executing the script.
    Stop(Mutex<Box<Fn(Result<(), Error>) + Send>>)
}

impl Debug for ExecutionOp {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        use self::ExecutionOp::*;
        match *self {
            Update {..} => formatter.write_str("Update"),
            UpdateCondition { .. } => formatter.write_str("UpdateCondition"),
            Stop (_) => formatter.write_str("Stop")
        }
    }
}

struct ConditionState {
    match_is_met: bool,

    /// The set of getters for which the condition is met.
    per_getter: HashSet<Id<Channel>>,

    /// If `None`, a duration is attached to this condition and we need to make sure that the
    /// condition remains true for at least `duration` before we decide whether to proceed with
    /// statements.
    duration: Option<Duration>
}
struct RuleState<Env> where Env: ExecutableDevEnv {
    rule_is_met: bool,
    per_condition: Vec<ConditionState>,
    ongoing_timer: Option<Env::TimerGuard>, // FIXME: It's actually a guard.
}

impl<Env> ExecutionTask<Env> where Env: ExecutableDevEnv + Debug {
    /// Create a new execution task.
    ///
    /// The caller is responsible for spawning a new thread and
    /// calling `run()`.
    fn new<S>(script: Script<UncheckedCtx>, owner: User, tx: S, rx: Receiver<ExecutionOp>) -> Result<Self, Error>
        where S: ExtSender<ExecutionOp> + Clone
    {
        let compiler = try!(Compiler::new().map_err(|err| Error::CompileError(err)));
        let script = try!(compiler.compile(script).map_err(|err| Error::CompileError(err)));

        Ok(ExecutionTask {
            script: script,
            owner: owner,
            rx: rx,
            tx: Box::new(tx)
        })
    }

    /// Execute the monitoring task.
    /// This currently expects to be executed in its own thread.
    fn run<S>(&mut self, env: Env, on_event: S, tx_init: &ExtSender<Result<(), Error>>) where S: ExtSender<ExecutionEvent> + Clone {
        info!("[Recipe '{}'] Starting execution of script", self.script.name);

        let mut witnesses = Vec::new();
        let api = env.api();

        // Generate the state of rules, conditions, getters and start
        // listening to changes in the getters.

        // FIXME: We could optimize requests by detecting if several share a `TargetMap<ChannelSelector, Exactly<Range>>`
        let mut per_rule : Vec<_> = self.script.rules.iter().zip(0 as usize..).map(|(rule, rule_index)| {
            let per_condition = rule.conditions.iter().zip(0 as usize..).map(|(condition, condition_index)| {
                // We will often end up watching several times the
                // same channel. For the moment, we do not attempt to
                // optimize either I/O (which we expect will be
                // optimized by `watcher`) or condition checking
                // (which we should eventually optimize, if we find
                // out that we end up with large rulesets).

                let channels = api.get_channels(condition.source.clone());
                info!("[Recipe '{}'] Initializing rule {} condition {}. Currently, it can listen to {} channels.", self.script.name,
                    rule_index, condition_index, channels.len());

                let rule_index = rule_index.clone();
                let condition_index = condition_index.clone();
                witnesses.push(
                    api.watch_values(
                        vec![Targetted {
                            select: condition.source.clone(),
                            payload: Exactly::Exactly(condition.when.clone())
                        }],
                        Box::new(self.tx.map(move |event| {
                            ExecutionOp::Update {
                                event: event,
                                rule_index: rule_index,
                                condition_index: condition_index
                            }
                        }))));
                ConditionState {
                    match_is_met: false,
                    per_getter: HashSet::new(),
                    duration: condition.duration.clone(),
                }
            }).collect();

            RuleState {
                rule_is_met: false,
                per_condition: per_condition,
                ongoing_timer: None,
            }
        }).collect();

        let _ = tx_init.send(Ok(()));

        for msg in self.rx.iter() {
            match msg {
                ExecutionOp::Stop(cb) => {
                    info!("[Recipe '{}'] Shutting down recipe.", self.script.name);

                    // Leave the loop. Watching will stop once
                    // `witnesses` is dropped.
                    cb.lock().unwrap()(Ok(()));
                    return;
                },
                ExecutionOp::UpdateCondition { id, is_met, rule_index, condition_index } => {
                    debug!("[Recipe '{}'] Updating the state of rule {}, condition {} => {}", self.script.name, rule_index, condition_index, is_met);
                    self.update_conditions(&self.script.name, id, is_met, &mut per_rule,
                        rule_index, condition_index, &api, &on_event);
                }
                ExecutionOp::Update { event, rule_index, condition_index } => {
                    match event {
                        WatchEvent::Error {
                            channel,
                            error
                        } => {
                            warn!("[Recipe '{}'] Initialization error for {}: {}", self.script.name, channel, error);
                            let _ = on_event.send(ExecutionEvent::ChannelError {
                                id: channel,
                                error: error,
                            });
                        },
                        WatchEvent::ChannelRemoved(id) => {
                            debug!("[Recipe '{}'] Removed getter {}, resetting condition to `false`", self.script.name, id);
                            // A channel was removed. Its condition is therefore not met anymore.
                            let msg = ExecutionOp::UpdateCondition {
                                id: id.clone(),
                                is_met: false,
                                rule_index: rule_index,
                                condition_index: condition_index
                            };
                            // This send will fail only if the thread is already down.
                            let _ = self.tx.send(msg);
                        },
                        WatchEvent::ChannelAdded(id) => {
                            debug!("[Recipe '{}'] Added getter {}.", self.script.name, id);
                            // A channel was added. Nothing to do.
                        }
                        WatchEvent::EnterRange { channel: id, value, .. } => {
                            debug!("[Recipe '{}'] Getter {} has entered the range for rule {}, condition {}: {:?}", self.script.name, id, rule_index, condition_index, value);
                            // We have entered a range. If there is a
                            // timer, start it, otherwise update conditions.
                            let msg = move || {
                                ExecutionOp::UpdateCondition {
                                    id: id.clone(),
                                    is_met: true,
                                    rule_index: rule_index,
                                    condition_index: condition_index
                                }
                            };
                            let duration = match per_rule[rule_index].
                                per_condition[condition_index].
                                duration {
                                None => {
                                    debug!("[Recipe '{}'] No timer for rule {}, condition {}, we should trigger the execution immediately.", self.script.name, rule_index, condition_index);
                                    let _ = self.tx.send(msg());
                                    continue
                                }
                                Some(ref duration) => {
                                    debug!("[Recipe '{}'] There is a timer for rule {}, condition {}, we should trigger the execution in {:?}s.", self.script.name, rule_index, condition_index, duration);
                                    duration.clone()
                                }
                            };

                            let tx = self.tx.map(move |()| {
                                msg()
                            });
                            per_rule[rule_index].ongoing_timer =
                                Some(env.start_timer(duration.clone(), Box::new(tx)));
                            let _ = on_event.send(ExecutionEvent::TimerStart {
                                rule_index: rule_index,
                                condition_index: condition_index,
                            });
                        }
                        WatchEvent::ExitRange { channel: id, value, .. } => {
                            debug!("[Recipe '{}'] Getter {} has left the range for rule {}, condition {}: {:?}", self.script.name, id, rule_index, condition_index, value);
                            if per_rule[rule_index].ongoing_timer.is_some() {
                                debug!("[Recipe '{}'] I need to cancel the timer for rule {}, condition {}", self.script.name, id, rule_index);
                                // Cancel the timer.
                                per_rule[rule_index].ongoing_timer.take();
                                let _ = on_event.send(ExecutionEvent::TimerCancel {
                                    rule_index: rule_index,
                                    condition_index: condition_index,
                                });
                            }
                            // Regardless, update the condition.
                            let msg = ExecutionOp::UpdateCondition {
                                id: id,
                                is_met: false,
                                rule_index: rule_index,
                                condition_index: condition_index
                            };
                            let _ = self.tx.send(msg);
                        }
                    }
                }
            }
        };
    }

    /// A getter just entered/left a range. Update the conditions to determine whether
    /// we now need to fire the statements.
    fn update_conditions<S>(&self, name: &str, id: Id<Channel>, getter_is_met: bool,
            per_rule: &mut Vec<RuleState<Env>>, rule_index: usize, condition_index: usize,
            api: &Env::API, on_event: &S)
            where S: ExtSender<ExecutionEvent> + Clone
    {
        use std::mem::replace;

        let was_met = if getter_is_met {
            !per_rule[rule_index]
                .per_condition[condition_index]
                .per_getter
                .insert(id)
        } else {
             per_rule[rule_index]
                .per_condition[condition_index]
                .per_getter
                .remove(&id)
        };

        debug!("[Thinkerbell update_condition {}] Updating condition for getter: {} => {}", name, was_met, getter_is_met);
        if was_met == getter_is_met {
            debug!("[Thinkerbell update_condition {}] Nothing has changed.", name);
            // Nothing has changed, no need to update any further.
            return;
        }

        // 1. Is the match met?
        //
        // The match is met iff any of the getters
        // meets the condition.
        let some_getter_is_met = getter_is_met ||
            per_rule[rule_index]
            .per_condition[condition_index]
            .per_getter
            .len() > 0;

        per_rule[rule_index]
            .per_condition[condition_index]
            .match_is_met = some_getter_is_met;

        // 2. Is the condition met?
        //
        // The condition is met iff all of the
        // matches are met.
        let condition_is_met =
            per_rule[rule_index]
            .per_condition
            .iter()
            .find(|condition_state| condition_state.match_is_met)
            .is_some();

        // 3. Are we in a case in which the
        // condition was not met and is now met?
        let condition_was_met =
            replace(&mut per_rule[rule_index].rule_is_met, condition_is_met);

        debug!("[Thinkerbell update_condition {}] Updating condition for rule: {} => {}", name, condition_was_met, condition_is_met);

        if !condition_was_met && condition_is_met {
            // Ahah, we have just triggered the statements!
            debug!("[Thinkerbell update_condition {}] Triggering {} statements.", name, self.script.rules[rule_index].execute.len());
            for (statement, statement_index) in self.script.rules[rule_index].execute.iter().zip(0..) {
                debug!("[Thinkerbell update_condition {}] Triggering statement {}/{}.", name, statement_index, self.script.rules[rule_index].execute.len());
                let result = statement.eval(&api, &self.owner);
                debug!("[Thinkerbell update_condition {}] Statement result {}/{}: {:?}.", name, statement_index, self.script.rules[rule_index].execute.len(), result);
                if result.is_empty() {
                    warn!("[Recipe '{}'] In rule {}, attempting to trigger statement {}, couldn't find any receiver channel.", name,
                            rule_index, condition_index);
                }

                let _ = on_event.send(ExecutionEvent::Sent {
                    rule_index: rule_index,
                    statement_index: statement_index,
                    result: result,
                });
            }
        }
        debug!("[Thinkerbell update_condition {}] done.", name);
    }
}


impl<Env> Statement<CompiledCtx<Env>> where Env: ExecutableDevEnv {
    fn eval(&self, api: &Env::API, owner: &User) ->  Vec<(Id<Channel>, Result<(), Error>)> {
        api.send_values(vec![Targetted {
            select: self.destination.clone(),
            payload: self.value.clone()
        }], owner.clone())
            .into_iter()
            .map(|(id, result)|
                 (id, result.map_err(|err| Error::APIError(err))))
            .collect()
    }
}



#[derive(Clone, Debug)]
pub enum StartStopError {
    AlreadyRunning,
    NotRunning,
    ThreadError,
}

#[derive(Clone, Debug)]
pub enum Error {
    CompileError(compile::Error),
    StartStopError(StartStopError),
    APIError(api::Error),
}
