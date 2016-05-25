// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]

extern crate foxbox_users;
#[macro_use]
extern crate iron;

#[macro_use]
extern crate log;

extern crate rusqlite;
extern crate serde;
extern crate serde_json;
extern crate ws;

pub mod broker;
pub mod runtime;
pub mod router;
mod workers;
