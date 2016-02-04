/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Needed to derive `Serialize` on ServiceProperties
#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]

// Needed for IntoIter in context.rs
#![feature(collections)]

extern crate core;
extern crate getopts;
extern crate hyper;
#[macro_use]
extern crate iron;
extern crate mio;
extern crate mount;
extern crate router;
extern crate serde;
extern crate staticfile;
extern crate uuid;
extern crate ws;

mod context;
mod dummy_adapter;
mod events;
mod http_server;
mod service;
mod controler;
mod websocket_server;

use context::Context;
use controler::Controler;
use getopts::Options;
use std::env;

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optflag("v", "verbose", "Toggle verbose output");
    opts.optopt("n", "name", "Set local host name", "HOSTNAME");
    opts.optflag("h", "help", "Print this help menu");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => { m }
        Err(_) => {
            print_usage(&program, opts);
            return;
        }
    };
    if matches.opt_present("h") {
        print_usage(&program, opts);
        return;
    }

    if let Ok(mut event_loop) = mio::EventLoop::new() {
        let sender = event_loop.channel();

        let context = Context::shared(matches.opt_present("v"),
                                      matches.opt_str("n"));
        let mut controler = Controler::new(sender, context);
        controler.start();

        event_loop.run(&mut controler)
                  .unwrap_or_else(|_| {  panic!("Starting the event loop failed!"); });
    } else {
        panic!("Creating the event loop failed!");
    }
}
