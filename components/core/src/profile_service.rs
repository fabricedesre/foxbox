/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// Simple service that helps with managing files in a configurable
/// directory.

use std::env;
use std::fs;
use std::io::ErrorKind;

pub enum ProfilePath {
    Default,
    Custom(String)
}

pub struct ProfileService {
    profile_path: String
}

fn get_env_var(name: &str) -> Option<String> {
    if let Some(value) = env::var_os(name) {
        return match value.into_string() {
            Ok(s) => Some(s),
            Err(_) => None
        };
    }
    None
}

impl ProfileService {
    pub fn new(profile_path: ProfilePath) -> Self {
        // If no explicit profile directory is set, follow the Freedesktop
        // standard: If $XDG_DATA_HOME is either not set or empty, a default
        // equal to $HOME/.local/share is used.
        let dir = match profile_path {
            ProfilePath::Custom(path) => path,
            ProfilePath::Default => {
                if let Some(xdg) = get_env_var("XDG_DATA_HOME") {
                    format!("{}/foxbox", xdg)
                } else {
                    if let Some(home) = get_env_var("HOME") {
                        format!("{}/.local/share/foxbox", home)
                    } else {
                        panic!("Unable to get $HOME value");
                    }
                }
            }
        };

        // Create the directory if needed. Panic if we can't or if there is an
        // existing file with the same path.
        match fs::metadata(dir.clone()) {
            Ok(meta) => {
                if !meta.is_dir() {
                    panic!("The path {} is a file, and can't be used as a profile.",
                           dir);
                }
            },
            Err(_) => {
                fs::create_dir_all(dir.clone()).unwrap_or_else(|err| {
                    if err.kind() != ErrorKind::AlreadyExists {
                        panic!("Unable to create directory {} : {}", dir, err);
                    }
                });
            }
        }

        ProfileService {
            profile_path: dir
        }
    }

    // Returns an absolute path for a file.
    // This doesn't try to create the file.
    pub fn path_for(&self, relative_path: &str) -> String {
        format!("{}/{}", self.profile_path, relative_path)
    }
}

#[test]
#[should_panic]
fn test_bogus_path() {
    let _ = ProfileService::new(
      ProfilePath::Custom("/cant_create/that/path".to_owned()));
}

#[test]
fn test_default_profile() {
    use std::fs::File;
    use std::io::Write;

    let profile = ProfileService::new(ProfilePath::Default);
    let path = profile.path_for("test.conf");
    // We can't assert anything on the path value since it's environment
    // dependant, but we should be able to create & delete the file.
    // We let the test panic if something goes wrong.
    let mut f = File::create(path.clone()).unwrap();
    f.write_all(b"Hello, world!").unwrap();
    fs::remove_file(path).unwrap();
    assert!(true);
}

#[test]
fn test_custom_profile() {

    use tempdir::TempDir;

    let profile_dir = TempDir::new_in("/tmp", "foxbox").unwrap();
    let profile_path = String::from(profile_dir.into_path()
                                    .to_str().unwrap());

    let profile = ProfileService::new(ProfilePath::Custom(profile_path
                                                          .clone()));

    let path = profile.path_for("test.conf");
    assert_eq!(path, format!("{}/test.conf", profile_path));
}
