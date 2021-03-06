extern crate openzwave_stateful as openzwave;
extern crate foxbox_taxonomy as taxonomy;
extern crate transformable_channels;
#[macro_use]
extern crate log;

mod id_map;
mod watchers;


use taxonomy::util::Id as TaxoId;
use taxonomy::services::{ Setter, Getter, AdapterId, ServiceId, Service, Channel, ChannelKind };
use taxonomy::values::*;
use taxonomy::api::{ ResultMap, Error as TaxoError, InternalError, User };
use taxonomy::adapter::{ AdapterManagerHandle, AdapterWatchGuard, WatchEvent };
use transformable_channels::mpsc::ExtSender;

use openzwave::{ ConfigPath, InitOptions, ZWaveManager, ZWaveNotification };
use openzwave::{ CommandClass, ValueGenre, ValueType, ValueID };
use openzwave::{ Node };

use std::error;
use std::fmt;
use std::{ fs, io };
use std::path::Path;
use std::thread;
use std::sync::mpsc;
use std::sync::{ Arc, Mutex };
use std::collections::{ HashMap, HashSet };

use id_map::IdMap;
use watchers::Watchers;

pub use self::OpenzwaveAdapter as Adapter;

#[derive(Debug)]
pub enum Error {
    TaxonomyError(TaxoError),
    IOError(io::Error),
    OpenzwaveError(openzwave::Error),
    UnknownError
}

impl From<TaxoError> for Error {
    fn from(err: TaxoError) -> Self {
        Error::TaxonomyError(err)
    }
}

impl From<()> for Error {
    fn from(_: ()) -> Self {
        Error::UnknownError
    }
}

impl From<openzwave::Error> for Error {
    fn from(error: openzwave::Error) -> Self {
        Error::OpenzwaveError(error)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::IOError(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::TaxonomyError(ref err)  => write!(f, "{}: {}", error::Error::description(self), err),
            Error::OpenzwaveError(ref err) => write!(f, "{}: {}", error::Error::description(self), err),
            Error::IOError(ref err) => write!(f, "{}: {}", error::Error::description(self), err),
            Error::UnknownError => write!(f, "{}", error::Error::description(self)),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::TaxonomyError(_) => "Taxonomy Error",
            Error::OpenzwaveError(_) => "Openzwave Error",
            Error::IOError(_) => "I/O Error",
            Error::UnknownError => "Unknown error",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::TaxonomyError(ref err) => Some(err),
            Error::OpenzwaveError(ref err) => Some(err),
            Error::IOError(ref err) => Some(err),
            Error::UnknownError => None,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum EventType {
    Enter,
    Exit,
}

trait RangeChecker {
    fn should_send(&self, &Value, EventType) -> bool;
}

impl RangeChecker for Option<Box<Range>> {
    fn should_send(&self, value: &Value, event_type: EventType) -> bool {
        match *self {
            None => event_type == EventType::Enter, // no range means we send only Enter events
            Some(ref range) => range.contains(value)
        }
    }
}

impl RangeChecker for Range {
    fn should_send(&self, value: &Value, _: EventType) -> bool {
        self.contains(value)
    }
}

fn taxo_kind_from_ozw_vid(vid: &ValueID) -> Option<ChannelKind> {
    match (vid.get_type(), vid.get_command_class(), vid.get_index()) {
        (ValueType::ValueType_Bool, Some(CommandClass::DoorLock),     0) => Some(ChannelKind::DoorLocked),
        (ValueType::ValueType_Bool, Some(CommandClass::SensorBinary), _) => Some(ChannelKind::OpenClosed),
        // (ValueType::ValueType_Bool, Some(_)) => Some(ChannelKind::OnOff), TODO Find a proper type
        // Unrecognized command class or type - we don't know what to do with it.
        _ => None
    }
}

fn ozw_vid_as_taxo_value(vid: &ValueID) -> Option<Value> {
    if vid.get_command_class().is_none() {
        return None;
    }

    match vid.get_type() {
        ValueType::ValueType_Bool => {
            if let Ok(value) = vid.as_bool() {
                match taxo_kind_from_ozw_vid(vid) {
                    // Some(ChannelKind::OnOff)  => Some(Value::OnOff(if value {OnOff::On} else {OnOff::Off})), // TODO support switches
                    Some(ChannelKind::OpenClosed)  => Some(Value::OpenClosed(if value {OpenClosed::Open} else {OpenClosed::Closed})),
                    Some(ChannelKind::DoorLocked)  => Some(Value::DoorLocked(if value {DoorLocked::Locked} else {DoorLocked::Unlocked})),
                    _ => None,
                }
            } else {
                None
            }
        },
        _ => None,   // TODO: Support more ValueType's
    }
}

fn set_ozw_vid_from_taxo_value(vid: &ValueID, value: Value) -> Result<(), TaxoError> {
    if vid.get_command_class().is_none() {
        return Err(TaxoError::InternalError(InternalError::GenericError(format!("Unknown command class: {}", vid.get_command_class_id()))));
    }

    let result = match vid.get_type() {
        ValueType::ValueType_Bool => {
            match value {
                //Value::OnOff(onOff) => { vid.set_bool(onOff == OnOff::On) } // TODO support switches
                Value::OpenClosed(open_closed) => { vid.set_bool(open_closed == OpenClosed::Open) }
                Value::DoorLocked(locked_unlocked) => { vid.set_bool(locked_unlocked == DoorLocked::Locked) }
                _ => { return Err(TaxoError::InvalidValue(value)) } // TODO InvalidType would be better but we'll need to fix specific types for specific TaxoIds
            }
        }
        _ => { return Err(TaxoError::InternalError(InternalError::GenericError(format!("Unsupported OZW type: {:?}", vid.get_type())))) }
    };

    result.map_err(|e| TaxoError::InternalError(InternalError::GenericError(format!("Error while setting a value: {}", e))))
}

type ValueCache = HashMap<TaxoId<Getter>, Value>;

pub struct OpenzwaveAdapter {
    id: TaxoId<AdapterId>,
    name: String,
    vendor: String,
    version: [u32; 4],
    ozw: ZWaveManager,
    node_map: IdMap<ServiceId, Node>,
    getter_map: IdMap<Getter, ValueID>,
    setter_map: IdMap<Setter, ValueID>,
    watchers: Arc<Mutex<Watchers>>,
    value_cache: Arc<Mutex<ValueCache>>,
}

fn ensure_directory<T: AsRef<Path> + ?Sized>(directory: &T) -> Result<(), Error> {
    let path = directory.as_ref();
    if path.exists() && !path.is_dir() {
        return Err(
            Error::IOError(io::Error::new(io::ErrorKind::AlreadyExists, format!("The file {} already exists and isn't a directory.", path.display())))
        );
    }

    if !path.exists() {
        try!(fs::create_dir(path));
    }

    Ok(())
}

impl OpenzwaveAdapter {
    pub fn init<T: AdapterManagerHandle + Send + Sync + 'static> (box_manager: &Arc<T>, user_path: &str, device: Option<String>) -> Result<(), Error> {

        try!(ensure_directory(user_path));

        let options = InitOptions {
            device: device,
            config_path: ConfigPath::Default, // This is where the default system configuraton is, usually contains the device information.
            user_path: user_path, // This is where we can override the system configuration, and where the network layout and logs are stored.
        };

        let (ozw, rx) = try!(match openzwave::init(&options) {
            Err(openzwave::Error::NoDeviceFound) => {
                // early return: we should not impair foxbox startup for this error.
                // TODO manage errors at adapter start: https://github.com/fxbox/RFC/issues/14
                info!("[OpenzwaveAdapter] No ZWave device has been found.");
                return Ok(());
            },
            Err(openzwave::Error::CannotReadDevice(device, cause)) => {
                // early return for the same reason as above.
                error!("[OpenzwaveAdapter] Could not read the device {}: {}.", device, cause);
                return Ok(());
            }
            result => result
        });

        let name = String::from("OpenZwave Adapter");
        let adapter = Arc::new(OpenzwaveAdapter {
            id: TaxoId::new(&name),
            name: name,
            vendor: String::from("Mozilla"),
            version: [1, 0, 0, 0],
            ozw: ozw,
            node_map: IdMap::new(),
            getter_map: IdMap::new(),
            setter_map: IdMap::new(),
            watchers: Arc::new(Mutex::new(Watchers::new())),
            value_cache: Arc::new(Mutex::new(HashMap::new())),
        });

        adapter.spawn_notification_thread(rx, box_manager);
        try!(box_manager.add_adapter(adapter));

        info!("[OpenzwaveAdapter] Started.");

        Ok(())
    }

    fn spawn_notification_thread<T: AdapterManagerHandle + Send + Sync + 'static>(&self, rx: mpsc::Receiver<ZWaveNotification>, box_manager: &Arc<T>) {
        let adapter_id = self.id.clone();
        let box_manager = box_manager.clone();
        let mut node_map = self.node_map.clone();
        let mut getter_map = self.getter_map.clone();
        let mut setter_map = self.setter_map.clone();
        let watchers = self.watchers.clone();
        let value_cache = self.value_cache.clone();

        thread::spawn(move || {
            for notification in rx {
                //debug!("Received notification {:?}", notification);
                match notification {
                    ZWaveNotification::ControllerReady(_controller) => {}
                    ZWaveNotification::NodeNew(_node)               => {}
                    ZWaveNotification::NodeAdded(node)              => {
                        let service_name = format!("OpenZWave-{:08x}-{:02x}", node.get_home_id(), node.get_id());
                        let service_id = TaxoId::new(&service_name);
                        node_map.push(service_id.clone(), node);

                        let mut service = Service::empty(service_id.clone(), adapter_id.clone());
                        service.properties.insert(String::from("name"), node.get_name());
                        service.properties.insert(String::from("product_name"), node.get_product_name());
                        service.properties.insert(String::from("manufacturer_name"), node.get_manufacturer_name());
                        service.properties.insert(String::from("location"), node.get_location());

                        box_manager.add_service(service).unwrap_or_else(|e| {
                            error!("Couldn't add the service {}: {}", service_name, e);
                        });
                    }
                    ZWaveNotification::NodeNaming(_node)             => {
                        // unfortunately we can't change a service' properties :(
                        // https://github.com/fxbox/taxonomy/issues/97
                        // When it's done we can move the properties change from above to here.
                    }
                    ZWaveNotification::NodeRemoved(_node)           => {}
                    ZWaveNotification::ValueAdded(vid)              => {
                        if vid.get_genre() != ValueGenre::ValueGenre_User { continue }

                        let value_id = format!("OpenZWave-{:08x}-{:016x} ({})", vid.get_home_id(), vid.get_id(), vid.get_label());

                        let node_id = node_map.find_taxo_id_from_ozw(&vid.get_node()).unwrap();

                        let has_getter = !vid.is_write_only();
                        let has_setter = !vid.is_read_only();

                        let kind = taxo_kind_from_ozw_vid(&vid);
                        if kind.is_none() { continue }
                        let kind = kind.unwrap();

                        if has_getter {
                            let getter_id = TaxoId::new(&value_id);
                            getter_map.push(getter_id.clone(), vid);
                            box_manager.add_getter(Channel {
                                id: getter_id.clone(),
                                service: node_id.clone(),
                                adapter: adapter_id.clone(),
                                last_seen: None,
                                tags: HashSet::new(),
                                mechanism: Getter {
                                    kind: kind.clone(),
                                    updated: None
                                }
                            }).unwrap_or_else(|e| {
                                error!("Couldn't add the getter {}: {}", value_id, e);
                            });
                        }

                        if has_setter {
                            let setter_id = TaxoId::new(&value_id);
                            setter_map.push(setter_id.clone(), vid);
                            box_manager.add_setter(Channel {
                                id: setter_id.clone(),
                                service: node_id.clone(),
                                adapter: adapter_id.clone(),
                                last_seen: None,
                                tags: HashSet::new(),
                                mechanism: Setter {
                                    kind: kind,
                                    updated: None
                                }
                            }).unwrap_or_else(|e| {
                                error!("Couldn't add the setter {}: {}", value_id, e);
                            });
                        }
                    }
                    ZWaveNotification::ValueChanged(vid)          => {
                        match vid.get_type() {
                            ValueType::ValueType_Bool => {},
                            _ => continue // ignore non-bool vals for now
                        };

                        let taxo_id = match getter_map.find_taxo_id_from_ozw(&vid) {
                            Some(taxo_id) => taxo_id,
                            _ => continue
                        };

                        let taxo_value = match ozw_vid_as_taxo_value(&vid) {
                            Some(value) => value,
                            _ => continue
                        };

                        let watchers = watchers.lock().unwrap();

                        let watchers = match watchers.get_from_taxo_id(&taxo_id) {
                            Some(watchers) => watchers,
                            _ => continue
                        };

                        let previous_value = {
                            let mut cache = value_cache.lock().unwrap();
                            let previous = cache.get(&taxo_id).cloned();
                            cache.insert(taxo_id.clone(), taxo_value.clone());
                            previous
                        };

                        for &(ref range, ref sender) in &watchers {
                            debug!("[OpenzwaveAdapter::ValueChanged] Iterating over watcher {:?} {:?}", taxo_id, range);

                            let should_send_value = range.should_send(&taxo_value, EventType::Enter);

                            if let Some(ref previous_value) = previous_value {
                                let should_send_previous = range.should_send(previous_value, EventType::Exit);
                                // If the new and the old values are both in the same range, we
                                // need to send nothing.
                                if should_send_value && should_send_previous { continue }

                                if should_send_previous {
                                    debug!("Openzwave::Adapter::ValueChanged Sending event Exit {:?} {:?}", taxo_id, taxo_value);
                                    let sender = sender.lock().unwrap();
                                    sender.send(
                                        WatchEvent::Exit { id: taxo_id.clone(), value: taxo_value.clone() }
                                    ).unwrap_or_else(|_| {
                                        error!("Couldn't send the exit event {{ id: {:?}, value: {:?} }}", taxo_id, taxo_value);
                                    });
                                }
                            }

                            if should_send_value {
                                debug!("[OpenzwaveAdapter::ValueChanged] Sending event Enter {:?} {:?}", taxo_id, taxo_value);
                                let sender = sender.lock().unwrap();
                                sender.send(
                                    WatchEvent::Enter { id: taxo_id.clone(), value: taxo_value.clone() }
                                ).unwrap_or_else(|_| {
                                    error!("Couldn't send the enter event {{ id: {:?}, value: {:?} }}", taxo_id, taxo_value);
                                });
                            }
                        }
                    }
                    ZWaveNotification::ValueRemoved(_value)         => {}
                    ZWaveNotification::AwakeNodesQueried(ref controller) | ZWaveNotification::AllNodesQueried(ref controller) => {
                        debug!("[OpenzwaveAdapter] Writing the network config.");
                        controller.write_config();
                    }
                    ZWaveNotification::Generic(_string)             => {}
                    _ => {}
                }
            }
        });
    }
}

impl taxonomy::adapter::Adapter for OpenzwaveAdapter {
    fn id(&self) -> TaxoId<AdapterId> {
        self.id.clone()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn vendor(&self) -> &str {
        &self.vendor
    }

    fn version(&self) -> &[u32; 4] {
        &self.version
    }

    fn fetch_values(&self, mut set: Vec<TaxoId<Getter>>, _: User) -> ResultMap<TaxoId<Getter>, Option<Value>, TaxoError> {
        set.drain(..).map(|id| {
            let ozw_vid = self.getter_map.find_ozw_from_taxo_id(&id);

            let taxo_value: Option<Option<Value>> = ozw_vid.map(|ozw_vid: ValueID| {
                if !ozw_vid.is_set() { return None }

                ozw_vid_as_taxo_value(&ozw_vid)
            });
            let value_result: Result<Option<Value>, TaxoError> = taxo_value.ok_or(TaxoError::InternalError(InternalError::NoSuchGetter(id.clone())));
            (id, value_result)
        }).collect()
    }

    fn send_values(&self, mut values: HashMap<TaxoId<Setter>, Value>, _: User) -> ResultMap<TaxoId<Setter>, (), TaxoError> {
        values.drain().map(|(id, value)| {
            if let Some(ozw_vid) = self.setter_map.find_ozw_from_taxo_id(&id) {
                (id, set_ozw_vid_from_taxo_value(&ozw_vid, value))
            } else {
                (id.clone(), Err(TaxoError::InternalError(InternalError::NoSuchSetter(id))))
            }
        }).collect()
    }

    fn register_watch(&self, mut values: Vec<(TaxoId<Getter>, Option<Value>, Box<ExtSender<WatchEvent>>)>) -> Vec<(TaxoId<Getter>, Result<Box<AdapterWatchGuard>, TaxoError>)> {
        debug!("[OpenzwaveAdapter::register_watch] Should register some watchers");
        values.drain(..).filter_map(|(id, range, sender)| {
            let sender = Arc::new(Mutex::new(sender)); // Mutex is necessary because cb is not Sync.
            debug!("[OpenzwaveAdapter::register_watch] Should register a watcher for {:?} {:?}", id, range);
            let range = match range {
                None => None,
                Some(Value::Range(range)) => Some(range),
                Some(_) => {
                    // Ignore.
                    // FIXME: Log?
                    return None
                }
            };
            let watch_guard = {
                let mut watchers = self.watchers.lock().unwrap();
                watchers.push(id.clone(), range.clone(), sender.clone())
            };
            let value_result: Result<Box<AdapterWatchGuard>, TaxoError> = Ok(Box::new(watch_guard));

            // if there is a set value already, let's send it.
            let ozw_value: Option<ValueID> = self.getter_map.find_ozw_from_taxo_id(&id);
            if let Some(value) = ozw_value {
                if value.is_set() && value.get_type() == ValueType::ValueType_Bool {
                    if let Some(value) = ozw_vid_as_taxo_value(&value) {
                        self.value_cache.lock().unwrap().insert(id.clone(), value.clone());
                        if range.should_send(&value, EventType::Enter) {
                            debug!("[OpenzwaveAdapter::register_watch] Sending event Enter {:?} {:?}", id, value);
                            let sender = sender.lock().unwrap();
                            sender.send(
                                WatchEvent::Enter { id: id.clone(), value: value.clone() }
                            ).unwrap_or_else(|_| {
                                error!("Couldn't send the enter event {{ id: {:?}, value: {:?} }}", id, value);
                            });
                        }
                    }
                }
            }

            Some((id, value_result))
        }).collect()
    }

    fn stop(&self) {
        info!("[OpenzwaveAdapter::stop] Stopping the Openzwave adapter: writing the network config.");
        self.ozw.write_configs();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}

