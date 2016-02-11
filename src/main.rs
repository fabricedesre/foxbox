/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Needed to derive `Serialize` on ServiceProperties
#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]
// For Docopt macro
#![plugin(docopt_macros)]

// Needed for IntoIter in context.rs
#![feature(collections)]

// Make linter fail for every warning
#![plugin(clippy)]
#![deny(clippy)]

extern crate core;
extern crate docopt;
extern crate env_logger;
extern crate hyper;
#[macro_use]
extern crate iron;
#[macro_use]
extern crate log;
extern crate mio;
extern crate mount;
extern crate router;
extern crate rustc_serialize;
extern crate serde;
extern crate staticfile;
extern crate uuid;
extern crate websocket;
extern crate ws;

mod context;
mod dummy_adapter;
mod events;
mod http_server;
mod service;
mod controller;
mod websocket_server;

use context::Context;
use controller::Controller;

docopt!(Args derive Debug, "
Usage: foxbox [-v] [-h] [-n <hostname>] [-p <port>] [-w <wsport>]

Options:
    -v, --verbose            Toggle verbose output.
    -n, --name <hostname>    Set local hostname.
    -p, --port <port>        Set port to listen on for http connections.
    -w, --wsport <wsport>    Set port to listen on for websocket.
    -h, --help               Print this help menu.
",
        flag_name: Option<String>,
        flag_port: Option<u16>,
        flag_wsport: Option<u16>);

fn main() {
    env_logger::init().unwrap();

    let args: Args = Args::docopt().decode().unwrap_or_else(|e| e.exit());

    if let Ok(mut event_loop) = mio::EventLoop::new() {
        let sender = event_loop.channel();

        let context = Context::shared(args.flag_verbose, args.flag_name,
                                      args.flag_port, args.flag_wsport);
        let mut controller = Controller::new(sender, context);
        controller.start();

        event_loop.run(&mut controller)
                  .unwrap_or_else(|_| {  panic!("Starting the event loop failed!"); });
    } else {
        panic!("Creating the event loop failed!");
    }
}

#[test]
fn options_are_good() {
    // short form options
    {
        let argv = || vec!["foxbox", "-p", "1234", "-n", "foobar",
                           "-w", "4567", "-v"];

        let args: Args = Args::docopt().argv(argv().into_iter())
            .decode()
            .unwrap_or_else(|e| e.exit());

        assert_eq!(args.flag_verbose, true);
        assert_eq!(args.flag_name, Some("foobar".to_string()));
        assert_eq!(args.flag_port, Some(1234));
        assert_eq!(args.flag_wsport, Some(4567));
        assert_eq!(args.flag_help, false);
    }
    // long form options
    {
        let argv = || vec!["foxbox", "--port", "1234",
                           "--name", "foobar", "--wsport", "4567",
                           "--verbose"];

        let args: Args = Args::docopt().argv(argv().into_iter())
            .decode()
            .unwrap_or_else(|e| e.exit());

        assert_eq!(args.flag_verbose, true);
        assert_eq!(args.flag_name, Some("foobar".to_string()));
        assert_eq!(args.flag_port, Some(1234));
        assert_eq!(args.flag_wsport, Some(4567));
        assert_eq!(args.flag_help, false);
    }
}
