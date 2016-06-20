//! An adapter providing access to the Thinkerbell rules engine.

use foxbox_taxonomy::api::{ Error, InternalError, User };
use foxbox_taxonomy::channel::*;
use foxbox_taxonomy::io;
use foxbox_taxonomy::manager::*;
use foxbox_taxonomy::parse::*;
use foxbox_taxonomy::services::{ AdapterId, ServiceId, Service };
use foxbox_taxonomy::util::{ Id, Maybe };
use foxbox_taxonomy::values::{ format, Data, Duration, Json, Value, OnOff };

use foxbox_thinkerbell::ast::*;
use foxbox_thinkerbell::compile::ExecutableDevEnv;
use foxbox_thinkerbell::manager::{ ScriptManager, ScriptId, Error as ScriptManagerError };
use foxbox_thinkerbell::run::ExecutionEvent;

use timer;
use transformable_channels::mpsc::*;

use std::collections::HashMap;
use std::fmt;
use std::path;
use std::sync::{ Arc, Mutex };
use std::thread;

use serde_json;

static ADAPTER_NAME: &'static str = "Thinkerbell adapter (built-in)";
static ADAPTER_VENDOR: &'static str = "team@link.mozilla.org";
static ADAPTER_VERSION: [u32;4] = [0, 0, 0, 0];

/// `ThinkerbellAdapter` hooks up the rules engine (if this, then that) as an adapter.
///
/// Each "rule", or "script", is a JSON-serialized structure according to Thinkerbell conventions.
///
/// This adapter exposes a root service, with one `AddThinkerbellRule` setter (to add a new rule).
/// Each rule that has been added is exposed as its own service, with the following getters/setters:
/// - Set Enabled (setter) -- toggles whether or not the script is enabled
/// - Get Enabled (getter) -- returns whether or not the script is enabled
/// - Remove (setter) -- removes the script
///
/// This adapter performs most actions by delegating channel messages to its main thread.
#[derive(Clone)]
pub struct ThinkerbellAdapter {

    /// The sending end of the channel for sending messages to `ThinkerbellAdapter`'s main loop.
    tx: Arc<Mutex<RawSender<ThinkAction>>>,

    /// A reference to the AdapterManager.
    adapter_manager: Arc<AdapterManager>,

    /// The ID of this adapter (permanently fixed)
    adapter_id: Id<AdapterId>,

    /// The ID of the root service's "Add Rule" setter.
    setter_add_rule_id: Id<Channel>,

    /// The `FeatureId` for accessing the on/off state of a rule.
    feature_rule_on: Id<FeatureId>,

    feature_source: Id<FeatureId>,
    feature_remove: Id<FeatureId>,
}

/// Thinkerbell requires an execution environment following this API.
#[derive(Clone)]
struct ThinkerbellExecutionEnv {
    adapter_manager: Arc<AdapterManager>,

    // FIXME: Timer's not clonable, so we should only use one, right? Does this have to be mutexed?
    timer: Arc<Mutex<timer::Timer>>
}
impl fmt::Debug for ThinkerbellExecutionEnv {
    fn fmt(&self, _: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        Ok(())
    }
}


impl ExecutableDevEnv for ThinkerbellExecutionEnv {
    // We don't support watches, so we don't care about the type of WatchGuard.
    type WatchGuard = WatchGuard;
    type API = AdapterManager;

    fn api(&self) -> &Self::API {
        &*self.adapter_manager
    }

    type TimerGuard = timer::Guard;
    fn start_timer(&self, duration: Duration, sender: Box<ExtSender<()>>) -> Self::TimerGuard {
        let timer = self.timer.lock().unwrap(); // FIXME: There's no way to report this error...
        timer.schedule_with_delay(duration.into(), move || {
            let _ = sender.send(());
        })
    }
}

/// Convert a `ScriptManagerError` into an API Error.
/// We can't implement From<T> because `ScriptManagerError` is in a different crate.
fn sm_error(e: ScriptManagerError) -> Error {
    Error::InternalError(InternalError::GenericError(format!("{:?}", e)))
}

impl Adapter for ThinkerbellAdapter {
    fn id(&self) -> Id<AdapterId> {
        self.adapter_id.clone()
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

    fn fetch_values(&self, set: Vec<Id<Channel>>, _: User) -> ResultMap<Id<Channel>, Option<Value>, Error> {
        set.iter().map(|id| {
            let (tx, rx) = channel();
            let _ = self.tx.lock().unwrap().send(ThinkAction::RespondToGetter(tx, id.clone()));
            match rx.recv() {
                Ok(result) => (id.clone(), result),
                // If an error occurs, the channel/thread died!
                Err(recv_err) => (id.clone(), Err(Error::InternalError(
                        InternalError::GenericError(format!("{:?}", recv_err))))),
            }
        }).collect()
    }

    fn send_values(&self, values: HashMap<Id<Channel>, Value>, user: User) -> ResultMap<Id<Channel>, (), Error> {
        values.iter()
            .map(|(id, value)| {
                let (tx, rx) = channel();
                let _ = self.tx.lock().unwrap().send(ThinkAction::RespondToSetter(tx, id.clone(), value.clone(), user.clone()));
                match rx.recv() {
                    Ok(result) => (id.clone(), result),
                    // If an error occurs, the channel died!
                    Err(recv_err) => (id.clone(), Err(Error::InternalError(
                            InternalError::GenericError(format!("{:?}", recv_err))))),
                }
            })
            .collect()
    }
}

/// `ThinkerbellAdapter`'s main loop handles messages of these types.
enum ThinkAction {
    AddRuleService(Id<ScriptId>),
    RemoveRuleService(Id<ScriptId>),
    RespondToGetter(RawSender<Result<Option<Value>, Error>>, Id<Channel>),
    RespondToSetter(RawSender<Result<(), Error>>, Id<Channel>, Value, User),
}

/// An internal data structure to track getters and setters.
struct ThinkerbellRule {
    script_id: Id<ScriptId>,
    service_id: Id<ServiceId>,
    getter_source_id: Id<Channel>,
    channel_is_enabled_id: Id<Channel>,
    setter_remove_id: Id<Channel>,
}

impl ThinkerbellAdapter {

    #[allow(cyclomatic_complexity)]
    fn main(
        &self,
        rx: Receiver<ThinkAction>,
        mut script_manager: ScriptManager<ThinkerbellExecutionEnv, RawSender<(Id<ScriptId>, ExecutionEvent)>>
    ) {
        // Store an in-memory list of all of the rules (their getters, setters, etc.).
        // We need to track these to respond to getter/setter requests.
        let mut rules: Vec<ThinkerbellRule> = Vec::new();

        'recv: for action in rx {
            match action {
                // After a script has been started, start a Service for that script.
                // The script has already been started with ScriptManager at this point;
                // we're just adding the adapter's services now.
                ThinkAction::AddRuleService(script_id) => {
                    // If the rule already existed (i.e. we're overwriting the original source),
                    // we don't need to re-add a Service. This is safe, because a Service doesn't
                    // know or care about the contents of the rule, just the ID.
                    for ref rule in &rules {
                        if rule.script_id == script_id {
                            info!("[thinkerbell@link.mozilla.org] No need to create a new service for this rule; ID '{}' already exists.", &script_id);
                            continue 'recv;
                        }
                    }
                    match self.add_rule_service(script_id) {
                        Ok(rule) => {
                            rules.push(rule);
                        },
                        Err(e) => {
                            error!("[thinkerbell@link.mozilla.org] Unable to add Thinkerbell Rule Service: {:?}", e);
                        }
                    };
                },
                // After a script has been stopped, remove the Service for that script.
                // The script has already been removed from ScriptManager at this point;
                // we're just updating the Service-level bookkeeping.
                ThinkAction::RemoveRuleService(script_id) => {
                    if let Some(position) = rules.iter().position(|ref r| r.script_id == script_id) {
                        let rule = rules.remove(position);
                        match self.remove_rule_service(&rule) {
                            Ok(_) => {},
                            Err(e) => {
                                error!("[thinkerbell@link.mozilla.org] Unable to remove Thinkerbell Rule Service: {:?}", e)
                            }
                        }
                    }
                },
                // Respond to a pending Getter request.
                ThinkAction::RespondToGetter(tx, getter_id) => {
                    for ref rule in &rules {
                        if getter_id == rule.channel_is_enabled_id {
                            let is_enabled = script_manager.is_enabled(&rule.script_id);
                            let _ = tx.send(Ok(Some(Value::new(if is_enabled { OnOff::On } else { OnOff::Off }))));
                            continue 'recv;
                        } else if getter_id == rule.getter_source_id {
                            match script_manager.get_source_and_owner(&rule.script_id) {
                                Ok((source, _)) => {
                                    match serde_json::from_str::<JSON>(&source) {
                                        Ok(json) => {
                                            let _ = tx.send(Ok(Some(Value::new(Json(json)))));
                                        }
                                        Err(err) => {
                                            warn!("[thinkerbell_adapter] The source for rule {} was stored in the db but cannot be parsed", rule.script_id);
                                            let _ = tx.send(Err(Error::ParseError(ParseError::JSON(JSONError(err)))));
                                        }
                                    }
                                },
                                Err(e) => {
                                    let _ = tx.send(Err(sm_error(e)));
                                }
                            };
                            continue 'recv;
                        }
                    }
                    let _ = tx.send(Err(Error::InternalError(InternalError::NoSuchChannel(getter_id.clone()))));
                },
                // Respond to a pending Setter request.
                ThinkAction::RespondToSetter(tx, setter_id, value, user) => {
                    // Add a new rule (with the given JSON source).
                    if setter_id == self.setter_add_rule_id {
                        match value.cast::<RuleSource>() {
                            Ok(rule_source) => {
                                let script_id = Id::new(&rule_source.script.name);
                                match script_manager.put(&script_id, &rule_source.source, &user) {
                                    Err(err) => {let _ = tx.send(Err(sm_error(err))) ;}
                                    Ok(ok) => {
                                        let _ = tx.send(Ok(ok));
                                        let _ = self.tx.lock().unwrap().send(ThinkAction::AddRuleService(script_id.clone()));
                                    }
                                }
                            },
                            Err(err) => {
                                let _ = tx.send(Err(err));
                            }
                        }
                    } else {
                        // The rest of the rules are script/rule-specific.
                        // NOTE: This linear search is not ideal, but tracking getters/setters in maps
                        // would be far more complex until we have a simpler way to track state within
                        // getter/setter API requests. In any case, this loop should be plenty fast for now.
                        for ref rule in &rules {
                            if setter_id == rule.channel_is_enabled_id {
                                match value.cast::<OnOff>() {
                                    Ok(&OnOff::On) => {
                                        let _ = tx.send(script_manager.set_enabled(&rule.script_id, true).map_err(sm_error));
                                    },
                                    Ok(&OnOff::Off) => {
                                        let _ = tx.send(script_manager.set_enabled(&rule.script_id, false).map_err(sm_error));
                                    },
                                    Err(err) => {
                                        let _ = tx.send(Err(err));
                                    },
                                }
                                continue 'recv;
                            } else if setter_id == rule.setter_remove_id {
                                let _ = tx.send(script_manager.remove(&rule.script_id).map_err(sm_error));
                                let _ = self.tx.lock().unwrap().send(ThinkAction::RemoveRuleService(rule.script_id.clone()));
                                continue 'recv;
                            }
                        }
                        // If we got here, no setters matched.
                        let _ = tx.send(Err(Error::InternalError(InternalError::NoSuchChannel(setter_id.clone()))));
                    }
                }
            }
        }
    }

    /// Add a new service for a script. (This does not start this script, this just adds a Service.)
    fn add_rule_service(&self, script_id: Id<ScriptId>) -> Result<ThinkerbellRule, Error> {
        let service_id = Id::new(&format!("thinkerbell/{}", script_id.as_atom()));

        let rule = ThinkerbellRule {
            script_id: script_id.clone(),
            service_id: service_id.clone(),
            getter_source_id: Id::new(&format!("{}/source", service_id.as_atom())),
            channel_is_enabled_id: Id::new(&format!("{}/is-rule-enabled", service_id.as_atom())),
            setter_remove_id: Id::new(&format!("{}/remove", service_id.as_atom())),
        };

        try!(self.adapter_manager.add_service(Service::empty(&service_id, &self.adapter_id)));

        try!(self.adapter_manager.add_channel(Channel {
            feature: self.feature_rule_on.clone(),
            supports_fetch: Some(Signature::returns(Maybe::Required(format::ON_OFF.clone()))),
            supports_send: Some(Signature::accepts(Maybe::Required(format::ON_OFF.clone()))),
            id: rule.channel_is_enabled_id.clone(),
            service: service_id.clone(),
            adapter: self.adapter_id.clone(),
            ..Channel::default()
        }));

        // Add getter for script source
        try!(self.adapter_manager.add_channel(Channel {
            feature: self.feature_source.clone(),
            supports_fetch: Some(Signature::returns(Maybe::Required(format::STRING.clone()))),
            id: rule.getter_source_id.clone(),
            service: service_id.clone(),
            adapter: self.adapter_id.clone(),
            ..Channel::default()
        }));


        // Add setter for removing this rule.
        try!(self.adapter_manager.add_channel(Channel {
            feature: self.feature_remove.clone(),
            supports_send: Some(Signature::accepts(Maybe::Nothing)),
            id: rule.setter_remove_id.clone(),
            service: service_id.clone(),
            adapter: self.adapter_id.clone(),
            ..Channel::default()
        }));
        info!("[thinkerbell@link.mozilla.org] Added Thinkerbell Rule for '{}'", &script_id.to_string());

        Ok(rule)
    }

    /// Remove an already-added Service (this does not stop the script).
    fn remove_rule_service(&self, rule: &ThinkerbellRule) -> Result<(), Error> {
        info!("[thinkerbell@link.mozilla.org] Removed Thinkerbell Rule for '{}'", &rule.script_id.to_string());
        self.adapter_manager.remove_service(&rule.service_id)
    }

    /// Everything is initialized here, but the real work happens in the main() loop.
    pub fn init(manager: &Arc<AdapterManager>, scripts_path: &str) -> Result<(), Error> {
        let adapter_id = Id::new("thinkerbell@link.mozilla.org");
        let setter_add_rule_id = Id::new("thinkerbell-add-rule");
        let root_service_id = Id::new("thinkerbell-root-service");
        let feature_rule_on = Id::new("thinkerbell/is-rule-enabled");
        let feature_add_rule = Id::new("thinkerbell/add-rule");
        let feature_remove = Id::new("thinkerbell/remove-rule-id");
        let feature_source = Id::new("thinkerbell/rule-source");


        // Prepare the script execution environment and load existing scripts.
        let (tx_env, rx_env) = channel();
        let env = ThinkerbellExecutionEnv {
            adapter_manager: manager.clone(),
            timer: Arc::new(Mutex::new(timer::Timer::new()))
        };

        let mut script_manager = try!(
            ScriptManager::new(env, path::Path::new(scripts_path), Box::new(tx_env)).map_err(sm_error));

        let result_map = try!(script_manager.load().map_err(sm_error));

        let (tx, rx) = channel();

        for script_id in result_map.keys() {
            let _ = tx.send(ThinkAction::AddRuleService(script_id.clone()));
        }

        let adapter = ThinkerbellAdapter {
            tx: Arc::new(Mutex::new(tx)),
            adapter_manager: manager.clone(),
            adapter_id: adapter_id.clone(),
            setter_add_rule_id: setter_add_rule_id.clone(),
            feature_rule_on: feature_rule_on,
            feature_source: feature_source,
            feature_remove: feature_remove,
        };

        // Add the adapter and the root service (the one that exposes `AddThinkerbellRule` for adding new rules).
        let rule_source_format = Arc::new(io::Format::new::<RuleSource>());
        try!(manager.add_adapter(Arc::new(adapter.clone())));
        try!(manager.add_service(Service::empty(&root_service_id, &adapter_id)));
        try!(manager.add_channel(Channel {
            feature: feature_add_rule,
            supports_send: Some(Signature::accepts(Maybe::Required(rule_source_format))),
            id: setter_add_rule_id,
            service: root_service_id.clone(),
            adapter: adapter_id.clone(),
            ..Channel::default()
        }));

        thread::spawn(move || {
            info!("[thinkerbell@link.mozilla.org] Started Thinkerbell main thread.");
            adapter.main(rx, script_manager)
        });

        // FIXME: We need to consume the events from the execution environment to prevent the
        // queue from growing unboundedly, but right now we don't use these events.
        // FIXME: When a script stops due to an error, we should update our state accordingly.
        // (Right now we only update the state when the script is explicitly started/stopped.)
        thread::spawn(move || {
            loop {
                let _ = rx_env.recv();
            }
        });

        Ok(())
    }
}


/// In-memory representation of a script.
#[derive(Debug)]
struct RuleSource {
    /// Parsed script.
    script: Script<UncheckedCtx>,

    /// The actual source code.
    source: String
}

impl PartialEq for RuleSource {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Data for RuleSource {
    fn description() -> String {
        "Thinkerbell Rule".to_owned()
    }
    fn parse(path: Path, source: &JSON, _: &io::BinarySource) -> Result<Self, Error> {
        let script = try!(Script::<UncheckedCtx>::parse(path, source)
            .map_err(Error::ParseError));
        match serde_json::to_string(source) {
            Ok(source) =>
                Ok(RuleSource {
                    script: script,
                    source: source
                }),
            Err(err) =>
                Err(Error::SerializeError(io::SerializeError::JSON(err.to_string())))
        }
    }
    fn serialize(source: &Self, _binary: &io::BinaryTarget) -> Result<JSON, Error> {
        serde_json::from_str(&source.source)
            .map_err(|err| Error::ParseError(ParseError::JSON(JSONError(err))))
    }
}


