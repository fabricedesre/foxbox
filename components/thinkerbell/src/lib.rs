#![feature(custom_derive, proc_macro)]
//! This create defines mechanisms for executing simple scripts on the
//! server.
//!
//! By design, these scripts have limited capabilities. Each script
//! takes the form of a set of rules: "when any of the input services
//! of foo matches some condition, send some value to all of the onput
//! services of bar".
//!
//! See module `ast` for more details on the grammar of scripts.

extern crate foxbox_taxonomy;

extern crate transformable_channels;

extern crate chrono;

#[macro_use]
extern crate log;
extern crate rusqlite;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;


/// Definition of the AST.
pub mod ast;

/// Compiling an AST into something runnable.
pub mod compile;

/// Actually executing code.
pub mod run;

/// Miscellaneous internal utilities.
pub mod util;

/// An implementation of Thinkerbell's Execution Environment on top of fake devices.
/// Useful mainly for writing tests.
pub mod fake_env;

/// ScriptManager manages storing and executing scripts.
pub mod manager;
