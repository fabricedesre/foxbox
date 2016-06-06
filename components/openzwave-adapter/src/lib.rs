extern crate openzwave_stateful as openzwave;
extern crate foxbox_taxonomy as taxonomy;
extern crate transformable_channels;
#[macro_use]
extern crate log;

mod id_map;
mod watchers;


use taxonomy::channel::*;
use taxonomy::util::{ Id as TaxoId, Maybe, ref_eq };
use taxonomy::services::{ AdapterId, ServiceId, Service };
use taxonomy::values::*;
use taxonomy::api::{ Operation, ResultMap, Error as TaxoError, InternalError, User };
use taxonomy::adapter::{ AdapterManagerHandle, AdapterWatchGuard, WatchEvent };
use transformable_channels::mpsc::ExtSender;

use openzwave::{ ConfigPath, InitOptions, ZWaveManager, ZWaveNotification };
use openzwave::{ CommandClass, ValueGenre, ValueType, ValueID };
use openzwave::{ Controller, Node };

use std::error;
use std::fmt;
use std::{ fs, io };
use std::path::Path;
use std::thread;
use std::sync::mpsc;
use std::sync::{ Arc, Mutex };
use std::collections::HashMap;

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

fn taxo_kind_from_ozw_vid(vid: &ValueID) -> Option<&Channel> {
    match (vid.get_type(), vid.get_command_class(), vid.get_index()) {
        (ValueType::ValueType_Bool, Some(CommandClass::DoorLock),     0) => Some(&DOOR_IS_LOCKED),
        (ValueType::ValueType_Bool, Some(CommandClass::SensorBinary), _) => Some(&DOOR_IS_OPEN),
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
                let kind = if let Some(kind) = taxo_kind_from_ozw_vid(vid) {
                    kind
                } else {
                    return None;
                };
                if ref_eq(kind, &DOOR_IS_OPEN) {
                    Some(Value::OpenClosed(if value {OpenClosed::Open} else {OpenClosed::Closed}))
                } else if ref_eq(kind, &DOOR_IS_LOCKED) {
                    Some(Value::DoorLocked(if value {DoorLocked::Locked} else {DoorLocked::Unlocked}))
                } else {
                    None
                }
                // Some(ChannelKind::OnOff)  => Some(Value::OnOff(if value {OnOff::On} else {OnOff::Off})), // TODO support switches
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
                _ => { return Err(TaxoError::InvalidValue) } // TODO InvalidType would be better but we'll need to fix specific types for specific TaxoIds
            }
        }
        _ => { return Err(TaxoError::InternalError(InternalError::GenericError(format!("Unsupported OZW type: {:?}", vid.get_type())))) }
    };

    result.map_err(|e| TaxoError::InternalError(InternalError::GenericError(format!("Error while setting a value: {}", e))))
}

fn start_including(ozw: &ZWaveManager, home_id: u32, value: &Value) -> Result<(), TaxoError> {
    match *value {
        Value::IsSecure(ref is_secure) => {
            let is_secure_bool = *is_secure == IsSecure::Secure;
            try!(
                ozw.add_node(home_id, is_secure_bool)
                    .map_err(|e| TaxoError::InternalError(InternalError::GenericError(format!("Error while including node on network {}: {}", home_id, e))))
            );
            info!("[OpenZWaveAdapter] Controller on network {} is awaiting an include in {} mode, please do the appropriate steps to include a device.", home_id, is_secure);
            Ok(())
        }
        _ => Err(TaxoError::TypeError(TypeError { expected: Type::IsSecure.name(), got: value.get_type().name() }))
    }
}

fn start_excluding(ozw: &ZWaveManager, home_id: u32) -> Result<(), TaxoError> {
    try!(
         ozw.remove_node(home_id)
            .map_err(|e| TaxoError::InternalError(InternalError::GenericError(format!("Error while excluding node on network {}: {}", home_id, e)))));
    info!("[OpenZWaveAdapter] Controller on network {} is awaiting an exclude, please do the appropriate steps to exclude a device.", home_id);
    Ok(())
}

type ValueCache = HashMap<TaxoId<Channel>, Value>;

pub struct OpenzwaveAdapter {
    id: TaxoId<AdapterId>,
    name: String,
    vendor: String,
    version: [u32; 4],
    ozw: ZWaveManager,
    node_map: IdMap<ServiceId, Node>,
    getter_map: IdMap<Channel, ValueID>,
    setter_map: IdMap<Channel, ValueID>,
    watchers: Arc<Mutex<Watchers>>,
    value_cache: Arc<Mutex<ValueCache>>,
    controller_map: IdMap<ServiceId, Controller>,
    include_map: IdMap<Channel, Controller>,
    exclude_map: IdMap<Channel, Controller>,
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
    pub fn init<T: AdapterManagerHandle + Send + Sync + 'static>(box_manager: &Arc<T>, user_path: &str, devices: Option<String>) -> Result<(), Error> {

        try!(ensure_directory(user_path));

        let options = InitOptions {
            // We treat devices as a comma (with optional whitespace) delimited list of device names.
            devices: devices.map(|s| s.split(',').map(|s| s.trim().to_owned()).collect()),
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
            controller_map: IdMap::new(),
            include_map: IdMap::new(),
            exclude_map: IdMap::new(),
        });

        try!(box_manager.add_adapter(adapter.clone()));
        adapter.spawn_notification_thread(rx, box_manager);

        info!("[OpenzwaveAdapter] Started.");

        Ok(())
    }

    fn spawn_notification_thread<T: AdapterManagerHandle + Send + Sync + 'static>(&self, rx: mpsc::Receiver<ZWaveNotification>, box_manager: &Arc<T>) {
        let adapter_id = self.id.clone();
        let box_manager = box_manager.clone();
        let mut node_map = self.node_map.clone();
        let mut getter_map = self.getter_map.clone();
        let mut setter_map = self.setter_map.clone();
        let mut controller_map = self.controller_map.clone();
        let mut include_map = self.include_map.clone();
        let mut exclude_map = self.exclude_map.clone();

        let watchers = self.watchers.clone();
        let value_cache = self.value_cache.clone();

        thread::spawn(move || {
            for notification in rx {
                //debug!("Received notification {:?}", notification);
                match notification {
                    ZWaveNotification::ControllerReady(controller) => {
                        let home_id = controller.get_home_id();
                        info!("Opened ZWave Controller {} HomeId: {:08x}", controller.get_controller_path(), home_id);

                        let service_name = format!("OpenZWave-controller-{:08x}", home_id);
                        let service_id = TaxoId::new(&service_name);
                        controller_map.push(service_id.clone(), controller);

                        let mut service = Service::empty(&service_id, &adapter_id);
                        service.properties.insert(String::from("name"), format!("Service for controller {:08x}", home_id));

                        box_manager.add_service(service).unwrap_or_else(|e| {
                            error!("Couldn't add the service {}: {}", service_name, e);
                        });

                        let include_setter_name = format!("OpenZWave-controller-{:08x}-include", home_id);
                        let include_setter_id = TaxoId::new(&include_setter_name);
                        include_map.push(include_setter_id.clone(), controller);

                        box_manager.add_channel(Channel {
                            feature: TaxoId::new("zwave/include"),
                            supports_send: Some(Signature::accepts(Maybe::Required(Type::IsSecure))),
                            id: include_setter_id.clone(),
                            service: service_id.clone(),
                            adapter: adapter_id.clone(),
                            .. Channel::default()
                        }).unwrap_or_else(|e| {
                            error!("Couldn't add the setter {}: {}", include_setter_id, e);
                        });

                        let exclude_setter_name = format!("OpenZWave-controller-{:08x}-exclude", home_id);
                        let exclude_setter_id = TaxoId::new(&exclude_setter_name);
                        exclude_map.push(exclude_setter_id.clone(), controller);

                        box_manager.add_channel(Channel {
                            feature: TaxoId::new("zwave/exclude"),
                            supports_send: Some(Signature::nothing()),
                            id: exclude_setter_id.clone(),
                            service: service_id.clone(),
                            adapter: adapter_id.clone(),
                            .. Channel::default()
                        }).unwrap_or_else(|e| {
                            error!("Couldn't add the setter {}: {}", exclude_setter_id, e);
                        });
                    }
                    ZWaveNotification::NodeNew(_node)               => {}
                    ZWaveNotification::NodeAdded(node)              => {
                        let service_name = format!("OpenZWave-{:08x}-{:02x}", node.get_home_id(), node.get_id());
                        let service_id = TaxoId::new(&service_name);
                        node_map.push(service_id.clone(), node);

                        let mut service = Service::empty(&service_id, &adapter_id);
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
                    ZWaveNotification::NodeRemoved(node)           => {
                        if let Some(service_id) = node_map.remove_by_ozw(&node) {
                            box_manager.remove_service(&service_id).unwrap_or_else(|e| {
                                error!("Couldn't remove the service {}: {}", service_id, e);
                            });
                        }
                    }
                    ZWaveNotification::ValueAdded(vid)              => {
                        if vid.get_genre() != ValueGenre::ValueGenre_User { continue }

                        let value_id = format!("OpenZWave-{:08x}-{:016x}", vid.get_home_id(), vid.get_id());

                        let node_id = node_map.find_taxo_id_from_ozw(&vid.get_node()).unwrap();

                        let kind = taxo_kind_from_ozw_vid(&vid);
                        let chan = match kind {
                            None => continue,
                            Some(kind) => kind.clone()
                        };

                        let id = TaxoId::new(&value_id);

                        let mut chan = Channel {
                            id: id.clone(),
                            service: node_id,
                            adapter: adapter_id.clone(),
                            ..chan
                        };

                        if vid.is_write_only() {
                            // For some reason, the value is configured as not being readable.
                            // Make sure that the channel doesn't pretend the opposite.
                            chan.supports_fetch = None;
                            chan.supports_watch = None;
                        } else {
                            getter_map.push(id.clone(), vid);
                        }
                        if vid.is_read_only() {
                            // For some reason, the value is configured as not being writeable.
                            // Make sure that the channel doesn't pretend the opposite.
                            chan.supports_send = None;
                        } else {
                            setter_map.push(id.clone(), vid);
                        }


                        box_manager.add_channel(chan)
                            .unwrap_or_else(|e| {
                                error!("Couldn't add the getter {}: {}", value_id, e);
                            });
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
                    ZWaveNotification::ValueRemoved(vid)            => {
                        if let Some(getter_id) = getter_map.remove_by_ozw(&vid) {
                            box_manager.remove_channel(&getter_id).unwrap_or_else(|e| {
                                error!("Unable to remove getter_id {}: {}", getter_id, e);
                            });
                        }
                        if let Some(setter_id) = setter_map.remove_by_ozw(&vid) {
                            box_manager.remove_channel(&setter_id).unwrap_or_else(|e| {
                                error!("Unable to remove setter_id {}: {}", setter_id, e);
                            });
                        }
                    }
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

    fn fetch_values(&self, mut set: Vec<TaxoId<Channel>>, _: User) -> ResultMap<TaxoId<Channel>, Option<Value>, TaxoError> {
        set.drain(..).map(|id| {
            let ozw_vid = self.getter_map.find_ozw_from_taxo_id(&id);

            let taxo_value: Option<Option<Value>> = ozw_vid.map(|ozw_vid: ValueID| {
                if !ozw_vid.is_set() { return None }

                ozw_vid_as_taxo_value(&ozw_vid)
            });
            let value_result: Result<Option<Value>, TaxoError> = taxo_value.ok_or(TaxoError::OperationNotSupported(Operation::Fetch, id.clone()));
            (id, value_result)
        }).collect()
    }

    fn send_values(&self, mut values: HashMap<TaxoId<Channel>, Value>, _: User) -> ResultMap<TaxoId<Channel>, (), TaxoError> {
        values.drain().map(|(id, value)| {
            if let Some(ozw_vid) = self.setter_map.find_ozw_from_taxo_id(&id) {
                (id, set_ozw_vid_from_taxo_value(&ozw_vid, value))
            } else if let Some(ozw_controller) = self.include_map.find_ozw_from_taxo_id(&id) {
                (id, start_including(&self.ozw, ozw_controller.get_home_id(), &value))
            } else if let Some(ozw_controller) = self.exclude_map.find_ozw_from_taxo_id(&id) {
                (id, start_excluding(&self.ozw, ozw_controller.get_home_id()))
            } else {
                (id.clone(), Err(TaxoError::InternalError(InternalError::NoSuchChannel(id))))
            }
        }).collect()
    }

    fn register_watch(&self, mut values: Vec<(TaxoId<Channel>, Option<Value>, Box<ExtSender<WatchEvent<Value>>>)>) -> Vec<(TaxoId<Channel>, Result<Box<AdapterWatchGuard>, TaxoError>)> {
        debug!("[OpenzwaveAdapter::register_watch] Should register some watchers");
        values.drain(..).filter_map(|(id, range, sender)| {
            if self.getter_map.find_ozw_from_taxo_id(&id).is_none() {
                return Some((id.clone(), Err(TaxoError::OperationNotSupported(Operation::Watch, id))))
            }

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

