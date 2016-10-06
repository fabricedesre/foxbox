/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![feature(plugin)]
#![plugin(serde_derive)]

#![plugin(clippy)]
#![deny(clippy)]


#[macro_use]
extern crate hyper;
extern crate iron;
#[macro_use]
extern crate log;
extern crate mktemp;
extern crate openssl;
extern crate openssl_sys;
extern crate serde;
extern crate serde_json;

macro_rules! checklock (
    ($e: expr) => {
        match $e {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
);

macro_rules! current_dir {
    () => {
        {
            use std::path::PathBuf;
            let mut this_file = PathBuf::from(file!());
            this_file.pop();
            this_file.to_str().unwrap().to_owned()
        }
    };
}

mod certificate_manager;
mod certificate_record;
mod dns_client;
mod https_server_factory;
mod letsencrypt;
mod ssl_context;
mod utils;

pub use certificate_manager::*;
pub use certificate_record::*;
pub use dns_client::*;
pub use https_server_factory::*;
pub use letsencrypt::*;
pub use ssl_context::*;

#[derive(Clone, Eq, PartialEq)]
pub enum TlsOption {
    Enabled,
    Disabled,
}
