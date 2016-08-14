/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate hyper;
extern crate time;
extern crate url;

use foxbox_core::config_store::ConfigService;
use foxbox_taxonomy::api::{ Error, InternalError };
use foxbox_taxonomy::channel::*;
use foxbox_taxonomy::services::*;
use rustc_serialize::base64::{ FromBase64, ToBase64, STANDARD };
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::io::{ BufWriter, ErrorKind };
use std::io::prelude::*;
use std::path::Path;
use std::sync::Arc;

pub fn create_service_id(service_id: &str) -> Id<ServiceId> {
    Id::new(&format!("service:{}@link.mozilla.org", service_id))
}

pub fn create_channel_id(operation: &str, service_id: &str) -> Id<Channel>
{
    Id::new(&format!("channel:{}.{}@link.mozilla.org", operation, service_id))
}

#[derive(Clone)]
pub struct IpCamera {
    pub udn: String,
    url: String,
    snapshot_dir: String,
    config: Arc<ConfigService>,

    upnp_name: String,

    pub image_list_id: Id<Channel>,
    pub image_newest_id: Id<Channel>,
    pub snapshot_id: Id<Channel>,
    pub username_id: Id<Channel>,
    pub password_id: Id<Channel>,
}

impl IpCamera {
    pub fn new(udn: &str, url: &str, upnp_name: &str, root_snapshot_dir: &str, config: &Arc<ConfigService>) -> Result<Self, Error> {
        let camera = IpCamera {
            udn: udn.to_owned(),
            url: url.to_owned(),
            snapshot_dir: format!("{}/{}", root_snapshot_dir, udn),
            config: config.clone(),
            upnp_name: upnp_name.to_owned(),
            image_list_id: create_channel_id("image_list", udn),
            image_newest_id: create_channel_id("image_newest", udn),
            snapshot_id: create_channel_id("snapshot", udn),
            username_id: create_channel_id("username", udn),
            password_id: create_channel_id("password", udn),
        };
        // Create a directory to store snapshots for this camera.
        if let Err(err) = fs::create_dir_all(&camera.snapshot_dir) {
            if err.kind() != ErrorKind::AlreadyExists {
                error!("Unable to create directory {}: {}", camera.snapshot_dir, err);
                return Err(Error::Internal(InternalError::GenericError(format!("cannot create {}", camera.snapshot_dir))));
            }
        }
        Ok(camera)
    }

    #[cfg(not(test))]
    fn get_bytes(&self, url: &str, username: &str, password: &str) -> Result<Vec<u8>, Error> {
        use self::hyper::header::{ Authorization, Basic, Connection };
        let client = hyper::Client::new();
        let get_result = client.get(url)
                               .header(
                                   Authorization(
                                       Basic {
                                           username: username.to_owned(),
                                           password: Some(password.to_owned())
                                       }
                                   )
                               )
                               .header(Connection::close())
                               .send();
        let mut res = match get_result {
            Ok(res) => res,
            Err(err) => {
                warn!("GET on {} failed: {}", url, err);
                return Err(Error::Internal(InternalError::InvalidInitialService));
            }
        };

        if res.status != self::hyper::status::StatusCode::Ok {
            warn!("GET on {} failed: {}", url, res.status);
            return Err(Error::Internal(InternalError::InvalidInitialService));
        }

        let mut image = Vec::new();
        match res.read_to_end(&mut image) {
            Ok(_) => Ok(image),
            Err(err) => {
                warn!("read of image data from {} failed: {}", url, err);
                Err(Error::Internal(InternalError::InvalidInitialService))
            }
        }
    }

    #[cfg(test)]
    fn get_bytes(&self, url: &str, username: &str, _password: &str) -> Result<Vec<u8>, Error> {
        // For testing assume that url is a filename.
        if username == "get_bytes:fail" {
            Err(Error::Internal(InternalError::GenericError("get_bytes".to_owned())))
        } else {
            self.read_image(url)
        }
    }

    fn config_key(&self, key: &str) -> String {
        format!("{}.{}", self.udn, key)
    }

    fn get_config(&self, key: &str) -> Option<String> {
        self.config.get("ip_camera", &self.config_key(key))
    }

    fn set_config(&self, key: &str, value: &str) {
        self.config.set("ip_camera", &self.config_key(key), value);
    }

    pub fn get_username(&self) -> String {
        if let Some(username) = self.get_config("username") {
            return username;
        }
        String::from("")
    }

    pub fn set_username(&self, username: &str) {
        self.set_config("username", username);
    }

    pub fn get_password(&self) -> String {
        if let Some(password) = self.get_config("password") {
            if let Ok(password_bytes) = password.from_base64() {
                if let Ok(password_str) = String::from_utf8(password_bytes) {
                    return password_str;
                }
            }
        }
        String::from("")
    }

    pub fn set_password(&self, password: &str) {
        // We base64 encode the password when we store it. The cameras only
        // use HTTP Basic Authentication, which just base64 encodes the username
        // and password anyway, so this is no less secure.

        self.set_config("password", &password.as_bytes().to_base64(STANDARD));
    }

    pub fn get_image_list(&self) -> Vec<String> {
        let mut array: Vec<String> = vec!();
        if let Ok(iter) = fs::read_dir(Path::new(&self.snapshot_dir)) {
            for entry in iter {
                if let Ok(entry) = entry {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_file() {
                            array.push(String::from(entry.file_name().to_str().unwrap()));
                        }
                    }
                }
            }
        }
        array
    }

    pub fn read_image(&self, full_filename: &str) -> Result<Vec<u8>, Error> {
        let mut options = fs::OpenOptions::new();
        options.read(true);
        if let Ok(mut image_file) = options.open(full_filename) {
            let mut image = Vec::new();
            if let Ok(_) = image_file.read_to_end(&mut image) {
                return Ok(image);
            }
            warn!("Error reading {}", full_filename);
        } else {
            warn!("Image {} not found", full_filename);
        }
        Err(Error::Internal(InternalError::InvalidInitialService))
    }

    pub fn get_image(&self, filename: &str) -> Result<Vec<u8>, Error> {
        let full_filename = format!("{}/{}", self.snapshot_dir, filename);
        self.read_image(&full_filename)
    }

    pub fn get_newest_image(&self) -> Result<Vec<u8>, Error> {
        let mut newest_image_time = 0;
        let mut newest_image = None;
        if let Ok(iter) = fs::read_dir(Path::new(&self.snapshot_dir)) {
            for entry in iter {
                if let Ok(entry) = entry {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_file() {
                            let time = metadata.ctime();
                            if newest_image_time <= time {
                                newest_image_time = time;
                                newest_image = Some(String::from(entry.file_name().to_str().unwrap()));
                            }
                        }
                    }
                }
            }
        }

        if newest_image.is_none() {
            return Err(Error::Internal(InternalError::InvalidInitialService));
        }
        self.get_image(&newest_image.unwrap())
    }

    pub fn take_snapshot(&self) -> Result<String, Error> {
        let image_url = "image/jpeg.cgi";
        let url = format!("{}/{}", self.url, image_url);

        let image = match self.get_bytes(&url, &self.get_username(), &self.get_password()) {
            Ok(image) => image,
            Err(err) => {
                warn!("Error '{:?}' retrieving image from camera {}", err, self.url);
                return Err(Error::Internal(InternalError::InvalidInitialService));
            }
        };

        let mut options = fs::OpenOptions::new();
        options.write(true);
        options.create(true);
        options.truncate(true);

        let filename_base = time::strftime("%Y-%m-%d-%H%M%S", &time::now()).unwrap();
        let mut full_filename;
        let image_file;
        let mut loop_count = 0;
        let mut filename;
        loop {
            if loop_count == 0 {
                filename = filename_base.clone();
            } else {
                filename = format!("{}-{}", filename_base, loop_count);
            }
            full_filename = format!("{}/{}.jpg", self.snapshot_dir, filename);

            if fs::metadata(full_filename.clone()).is_ok() {
                // File exists
                loop_count += 1;
                continue;
            }

            image_file = match options.open(full_filename.clone()) {
                Ok(file) => file,
                Err(err) => {
                    warn!("Unable to open {}: {:?}", full_filename, err.kind());
                    return Err(Error::Internal(InternalError::InvalidInitialService));
                }
            };

            break;
        }
        let mut writer = BufWriter::new(&image_file);
        match writer.write_all(&image) {
            Ok(_) => {}
            Err(err) => {
                warn!("Error '{:?}' writing snapshot.jpg for camera {}", err, self.udn);
                return Err(Error::Internal(InternalError::InvalidInitialService));
            }
        }
        info!("Took a snapshot from {}: {}", self.udn, full_filename);
        Ok(format!("{}.jpg", filename))
    }
}

#[cfg(test)]
use std::io;

#[cfg(test)]
pub fn remove_file<P: AsRef<Path>>(filename: P) -> io::Result<()> {
    if filename.as_ref().is_file() {
        fs::remove_file(filename)
    } else {
        // File doesn't exist -> we're good
        Ok(())
    }
}

#[cfg(test)]
pub fn remove_dir_all<P: AsRef<Path>>(dirname: P) -> io::Result<()> {
    if dirname.as_ref().is_dir() {
        fs::remove_dir_all(dirname)
    } else {
        // Directory doesn't exist -> we're good.
        Ok(())
    }
}

#[cfg(test)]
describe! ip_camera {

    before_each {
        use foxbox_core::config_store::ConfigService;
        use std::sync::Arc;
        use uuid::Uuid;

        let uniq_str = format!("{}", Uuid::new_v4());
        let config_filename = format!("ip-camera-test-conf-{}.tmp", uniq_str);
        let config = ConfigService::new(&config_filename);
        let snapshot_dir = format!("ip-camera-test-snapshot-dir-{}.tmp", uniq_str);
    }

    after_each {
        remove_file(&config_filename).unwrap();
        remove_dir_all(&snapshot_dir).unwrap();
    }

    describe! good_camera {

        before_each {
            let snapshot_dir = snapshot_dir.clone();
            let camera = IpCamera::new("udn", "test/ip-camera", "upnp_name", &snapshot_dir, &Arc::new(config)).unwrap();
        }

        it "should store username" {
            assert_eq!(camera.get_username(), "");
            camera.set_username("foobar_username");
            assert_eq!(camera.get_username(), "foobar_username");
        }

        it "test invalid stored password" {
            camera.set_config("password", "invalid password");
            assert_eq!(camera.get_password(), "");
        }

        it "should store password" {
            camera.set_password("foobar_password");
            assert_eq!(camera.get_password(), "foobar_password");

            let stored_password = camera.get_config("password").unwrap();
            assert!(stored_password != "foobar_password");
        }

        failing "non-existant latest image" {
            // Make sure that get_newest_image returns an empty list
            remove_dir_all(&snapshot_dir).unwrap();
            camera.get_newest_image().unwrap();
        }

        it "image list tests" {
            let images = camera.get_image_list();
            assert_eq!(images.len(), 0);

            camera.take_snapshot().unwrap();

            let images = camera.get_image_list();
            assert_eq!(images.len(), 1);

            let image_data = camera.get_newest_image().unwrap();
            let sample_image_data = camera.read_image("test/ip-camera/image/jpeg.cgi").unwrap();
            assert_eq!(image_data, sample_image_data);
        }

        failing "bad snapshot name" {
            // Removing the snapshot dir will cause get_image to fail.
            remove_dir_all(&snapshot_dir).unwrap();
            camera.get_image("xxx").unwrap();
        }

        failing "take_snapshot - get_bytes failure" {
            camera.set_username("get_bytes:fail");
            let result = camera.take_snapshot();

            // Do cleanup now since we're going to panic
            remove_file(&config_filename).unwrap();
            remove_dir_all(&snapshot_dir).unwrap();

            result.unwrap();
        }

        failing "take_snapshot - no snapshot dir" {
            remove_dir_all(&snapshot_dir).unwrap();
            camera.take_snapshot().unwrap();
        }
    }

    failing "bad snapshot dir" {
        // Pick a root directory that we can't create
        IpCamera::new("udn", "test/ip-camera", "upnp_name", "/unwritable", &Arc::new(config)).unwrap();
    }

    failing "take_snapsot - bad url" {
        let camera = IpCamera::new("udn", "xxx/ip-camera", "upnp_name", &snapshot_dir, &Arc::new(config)).unwrap();
        remove_dir_all(&snapshot_dir).unwrap();
        camera.take_snapshot().unwrap();
    }
}
