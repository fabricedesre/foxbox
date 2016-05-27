extern crate libc;
extern crate hyper;

use std::collections::HashMap;
use std::ffi::{ CString, CStr };
use std::io::{ Read, Cursor };
use std::ptr;
use std::thread;
use utils::parse_simple_xml;
use std::sync::{ Arc, Mutex };

#[allow(dead_code)]
#[derive(Debug)]
#[repr(C)]
enum EventType {
    ControlActionRequest,
    ControlActionComplete,
    ControlGetVarRequest,
    ControlGetVarComplete,
    DiscoveryAdvertisementAlive,
    DiscoveryAdvertisementByebye,
    DiscoverySearchResult,
    DiscoverySearchTimeout,
    SubscriptionRequest,
    Received,
    RenewalComplete,
    SubscribeComplete,
    UnsubscribeComplete,
    AutorenewalFailed,
    SubscriptionExpired
}

const LINE_SIZE: usize = 180;

#[repr(C)]
struct Discovery {
    err_code: libc::c_int,
    expires: libc::c_int,
    device_id: [libc::c_char; LINE_SIZE],
    device_type: [libc::c_char; LINE_SIZE],
    service_type: [libc::c_char; LINE_SIZE],
    service_ver: [libc::c_char; LINE_SIZE],
    location: [libc::c_char; LINE_SIZE],
    os: [libc::c_char; LINE_SIZE],
    date: [libc::c_char; LINE_SIZE],
    ext: [libc::c_char; LINE_SIZE],
    dest_addr: *mut libc::sockaddr_in,
}

type ClientHandle = libc::c_int;

type ClientCallbackPtr = extern fn(event_type: EventType, event: *const libc::c_void, cookie: *mut libc::c_void);

#[link(name = "upnp")]
extern {
    fn UpnpInit(hostIp: *const libc::c_char, destPort: libc::c_ushort) -> libc::c_int;
    fn UpnpRegisterClient(callback: ClientCallbackPtr, cookie: *mut libc::c_void, handle: *mut ClientHandle) -> libc::c_int;
    fn UpnpUnRegisterClient(handle: ClientHandle) -> libc::c_int;
    fn UpnpSearchAsync(handle: ClientHandle, maxAttempts: libc::c_int, target: *const libc::c_char, cookie: *const libc::c_void) -> libc::c_int;
}

#[derive(Debug)]
pub struct UpnpMsearchHeader {
    pub device_id: String,
    pub device_type: String,
    pub service_type: String,
    pub service_ver: String,
    pub location: String,
    pub os: String,
    pub date: String,
    pub ext: String,
    pub expires: i32,
    pub alive: bool,
}

#[derive(Debug)]
pub struct UpnpService {
    pub msearch: UpnpMsearchHeader,
    pub description: HashMap<String, String>,
    pub description_data: String
}

pub trait UpnpListener : Send {
    fn upnp_discover(&self, service: &UpnpService) -> bool;
}

type UpnpListeners = Arc<Mutex<HashMap<String, Box<UpnpListener>>>>;

struct UpnpHandle {
    client: ClientHandle,
    cookie: *mut UpnpListeners
}

impl Drop for UpnpHandle {
    fn drop(&mut self) {
        debug!("releasing handles (client={} cookie={:?})", self.client, self.cookie);
        if self.client != 0 {
            unsafe { UpnpUnRegisterClient(self.client); }
        }
        if !self.cookie.is_null() {
            unsafe { Box::from_raw(self.cookie); }
        }
    }
}

pub struct UpnpManager {
    listeners: UpnpListeners,
    handle: Arc<UpnpHandle>
}

unsafe impl Send for UpnpManager {}
unsafe impl Sync for UpnpManager {}

impl UpnpManager {
    pub fn new() -> Self {
        UpnpManager {
            listeners: Arc::new(Mutex::new(HashMap::new())),
            handle: Arc::new(UpnpHandle { client: 0, cookie: ptr::null_mut() })
        }
    }

    fn notify_service(listeners: UpnpListeners, service: UpnpService) {
        for l in listeners.lock().unwrap().values() {
            l.upnp_discover(&service);
        }
    }

    fn msearch_callback(listeners: UpnpListeners, data: &Discovery, alive: bool) {
        let header = UpnpMsearchHeader {
            device_id: unsafe { CStr::from_ptr(&data.device_id[0]).to_string_lossy().into_owned() },
            device_type: unsafe { CStr::from_ptr(&data.device_type[0]).to_string_lossy().into_owned() },
            service_type: unsafe { CStr::from_ptr(&data.service_type[0]).to_string_lossy().into_owned() },
            service_ver: unsafe { CStr::from_ptr(&data.service_ver[0]).to_string_lossy().into_owned() },
            location: unsafe { CStr::from_ptr(&data.location[0]).to_string_lossy().into_owned() },
            os: unsafe { CStr::from_ptr(&data.os[0]).to_string_lossy().into_owned() },
            date: unsafe { CStr::from_ptr(&data.date[0]).to_string_lossy().into_owned() },
            ext: unsafe { CStr::from_ptr(&data.ext[0]).to_string_lossy().into_owned() },
            expires: data.expires,
            alive: alive,
        };

        trace!("UPnP msearch callback: header {:?}, alive {}", header, alive);

        // No need to fetch the description XML if the device notified us
        // that it is disconnecting; should be even bother to tell adapters
        // about this?
        if !alive {
            UpnpManager::notify_service(listeners, UpnpService {
                msearch: header,
                description: HashMap::new(),
                description_data: String::new()
            });
            return;
        }

        thread::spawn(move || {
            // Note we must be careful to actually handle these errors gracefully
            // since the network or end device can fail us easily.
            let client = hyper::Client::new();
            let mut res = match client.get(&header.location).header(hyper::header::Connection::close()).send() {
                Ok(x) => x,
                Err(e) => { warn!("failed to send request {}: {:?}", header.location, e); return; }
            };

            let mut body = String::new();
            match res.read_to_string(&mut body) {
                Ok(x) => x,
                Err(e) => { warn!("failed to get response {}: {:?}", header.location, e); return; }
            };

            trace!("UPnP body: {:?}", body);

            let values;
            {
                let cursor = Cursor::new(&body);
                values = match parse_simple_xml(cursor) {
                    Ok(x) => x,
                    Err(e) => { warn!("failed to parse response {}: {:?}", header.location, e); return; }
                };
            }

            trace!("UPnP values: {:?}", values);

            UpnpManager::notify_service(listeners, UpnpService {
                msearch: header,
                description: values,
                description_data: body
            });
        });
    }

    extern fn callback(event_type: EventType, event: *const libc::c_void, cookie: *mut libc::c_void) {
        let listeners: *mut UpnpListeners = cookie as *mut UpnpListeners;
        if listeners == ptr::null_mut() { panic!("invalid cookie"); }

        let data: *const Discovery;
        let alive: bool;
        match event_type {
            EventType::DiscoverySearchResult | EventType::DiscoveryAdvertisementAlive => {
                data = event as *const Discovery;
                alive = true;
            },
            EventType::DiscoveryAdvertisementByebye => {
                data = event as *const Discovery;
                alive = false;
            },
            // Timeout really just lets us know the search is done, it may or may not
            // have found devices
            EventType::DiscoverySearchTimeout => { return; },
            _ => { warn!("unhandled callback event {:?}", event_type); return; },
        };

        if data == ptr::null() { panic!("null discovery"); }
        unsafe { UpnpManager::msearch_callback((*listeners).clone(), &(*data), alive); }
    }

    fn initialize() -> Result<(), i32> {
        let err = unsafe { UpnpInit(ptr::null(), 0) };
        debug!("initialized ({})", err);
        match err {
            0 => Ok(()),
            _ => Err(err)
        }
    }

    pub fn search(&self, target: Option<String>) -> Result<(), i32> {
        let target = match target {
            Some(x) => CString::new(x),
            None => CString::new("ssdp:all")
        }.unwrap();

        let cookie = self.handle.cookie as *mut libc::c_void;
        let err = unsafe { UpnpSearchAsync(self.handle.client, 1, target.as_ptr(), cookie) };

        info!("UPnP search for devices matching {:?} ({})", target, err);
        match err {
            0 => Ok(()),
            _ => Err(err)
        }
    }

    pub fn add_listener(&self, id: String, listener: Box<UpnpListener>) {
        let mut listeners = self.listeners.lock().unwrap();
        listeners.insert(id, listener);
    }

    pub fn start(&mut self) -> Result<(), i32> {
        UpnpManager::initialize().unwrap();

        let handle = Arc::get_mut(&mut self.handle).unwrap();
        handle.cookie = Box::into_raw(Box::new(self.listeners.clone()));
        let cookie = handle.cookie as *mut libc::c_void;
        let client: *mut ClientHandle = &mut handle.client as *mut ClientHandle;
        let err = unsafe { UpnpRegisterClient(UpnpManager::callback, cookie, client) };

        debug!("registered client ({})", err);
        match err {
            0 => Ok(()),
            _ => Err(err)
        }
    }
}

impl Default for UpnpManager {
    fn default() -> Self {
        UpnpManager::new()
    }
}
