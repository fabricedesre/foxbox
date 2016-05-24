//! This crate defines the high-level API for accessing Connected Devices.
#![feature(custom_derive, plugin, stmt_expr_attributes)]
#![plugin(serde_macros)]
#![plugin(clippy)]
// To prevent clippy being noisy with derive(...)
#![allow(used_underscore_binding)]

#[macro_use]
extern crate lazy_static;

extern crate chrono;
extern crate libc;
#[macro_use]
extern crate log;
extern crate rusqlite;
extern crate serde;
extern crate serde_json;
extern crate string_cache;
extern crate sublock;
extern crate transformable_channels;

/// Metadata on devices
pub mod services;

/// Public-facing API
pub mod api;

/// Tools for parsing from JSON.
pub mod parse;

/// Selecting one or more devices. Exposed through the API.
pub mod selector;

/// Values that may be sent to/received from devices
pub mod values;

/// Various utilities
pub mod util;

/// The back-end thread, in charge of the heavy lifting of managing adapters.
mod backend;

/// The manager provides an API for (un)registering adapters, services, channels, and
/// uses these to implements the taxonomy API.
pub mod manager;

/// The API for defining Adapters.
pub mod adapter;

/// Utilities for writing Adapters.
pub mod adapter_utils;

/// Utility module for inserting values in maps and keeping the insertion reversible in case of
/// any error.
pub mod transact;

/// Implementation of the database storing tags.
pub mod tag_storage;

/// Implementation of a fake adapter, controlled entirely programmatically. Designed to be used
/// as a component of tests.
pub mod fake_adapter;

/// Serialization and deserialization.
pub mod io;