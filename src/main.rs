/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Needed to derive `Serialize` on ServiceProperties
#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]
// For Docopt macro
#![plugin(docopt_macros)]

// Make linter fail for every warning
#![plugin(clippy)]
#![deny(clippy)]
// Needed for many #[derive(...)] macros
#![allow(used_underscore_binding)]

#![cfg_attr(test, feature(const_fn))] // Dependency of stainless
#![cfg_attr(test, plugin(stainless))] // Test runner

#![feature(associated_consts)]

extern crate chrono;
extern crate core;
extern crate docopt;
extern crate env_logger;
#[macro_use]
extern crate foxbox_core;
#[macro_use]
extern crate foxbox_taxonomy;
extern crate foxbox_thinkerbell;
extern crate foxbox_users;
#[macro_use]
extern crate hyper;
#[macro_use]
extern crate iron;
extern crate iron_cors;
#[cfg(test)]
extern crate iron_test;
extern crate jsworkers;
extern crate libc;
#[macro_use]
extern crate log;
extern crate mio;
extern crate mount;
extern crate nix;
extern crate openssl;
extern crate openssl_sys;
extern crate rand;
extern crate router;
extern crate rustc_serialize;
extern crate rusqlite;
extern crate serde;
extern crate serde_json;
extern crate staticfile;
extern crate tls;
extern crate time;
extern crate timer;
extern crate transformable_channels;
extern crate unicase;
extern crate url;
extern crate ws;
extern crate multicast_dns;

// adapters
extern crate openzwave_adapter as openzwave;

#[cfg(test)]
extern crate regex;
#[cfg(test)]
extern crate tempdir;
#[cfg(test)]
extern crate uuid;

mod adapters;
mod controller;
mod http_server;
mod registration;
mod static_router;
mod taxonomy_router;
mod tunnel_controller;
mod ws_server;

#[cfg(test)]
mod stubs {
    #![allow(dead_code)]
    #![allow(unused_variables)]
    #![allow(boxed_local)]
    pub mod controller;
}

use controller::FoxBox;
use env_logger::LogBuilder;
use tunnel_controller:: { TunnelConfig, Tunnel };
use libc::{ sighandler_t, SIGINT };
use log::{ LogRecord, LogLevelFilter };

use multicast_dns::host::HostManager;
use foxbox_core::profile_service::ProfilePath;
use std::env;
use std::sync::atomic::{ AtomicBool, Ordering, ATOMIC_BOOL_INIT };
use tls::TlsOption;
use foxbox_core::traits::Controller;
use foxbox_core::utils;

docopt!(Args derive Debug, "
Usage: foxbox [-v] [-h] [-l <hostname>] [-p <port>] [-w <wsport>] [-d <profile_path>] [-r <url>] [-i <iface>] [-t <tunnel>] [-s <secret>] [--disable-tls] [--dns-domain <domain>] [--dns-api <url>] [-j <path>] [-c <namespace;key;value>]...

Options:
    -v, --verbose            Toggle verbose output.
    -l, --local-name <hostname>    Set local hostname. [default: foxbox]
    -p, --port <port>        Set port to listen on for http connections. [default: 3000]
    -w, --wsport <wsport>    Set port to listen on for websocket. [default: 4000]
    -d, --profile <path>     Set profile path to store user data.
    -r, --register <url>     Change the url of the registration endpoint. [default: http://knilxof.org:4242]
    -i, --iface <iface>      Specify the local IP interface.
    -t, --tunnel <tunnel>    Set the tunnel endpoint's hostname. If omitted, the tunnel is disabled.
    -s, --tunnel-secret <secret>       Set the tunnel shared secret. [default: secret]
        --disable-tls                  Run as a plain HTTP server, disabling encryption.
        --dns-domain <domain>          Set the top level domain for public DNS [default: box.knilxof.org]
        --dns-api <url>                Set the DNS API endpoint [default: https://knilxof.org:5300]
    -j, --jsrunner <path>    Set the path to the js engine used for js workers.
    -c, --config <namespace;key;value>  Set configuration override
    -h, --help               Print this help menu.
",
        flag_local_name: String,
        flag_port: u16,
        flag_wsport: u16,
        flag_profile: Option<String>,
        flag_register: String,
        flag_iface: Option<String>,
        flag_tunnel: Option<String>,
        flag_tunnel_secret: String,
        flag_disable_tls: bool,
        flag_dns_domain: String,
        flag_dns_api: String,
        flag_jsrunner: Option<String>,
        flag_config: Option<Vec<String>>);

/// Updates local host name with the provided host name string. If requested host name
/// is not available (used by anyone else on the same network) then collision
/// resolution logic is triggered and alternative name is chosen automatically
/// (host name plus "-2", "-3" and etc. postfix). This function blocks until host name
/// is updated and returns actual host name.
///
/// # Panics
///
/// Panics if provided host name is not valid non-FQDN host name.
///
/// # Arguments
///
/// * `hostname` - host name name we'd like to set (should be a valid non-FQDN host name).
fn update_hostname(hostname: String) -> String {
    let host_manager = HostManager::new();

    if !host_manager.is_valid_name(&hostname) {
        panic!("Host name `{}` is not a valid host name!", &hostname);
    }

    host_manager.set_name(&hostname)
}

// Handle SIGINT (Ctrl-C) for manual shutdown.
// Signal handlers must not do anything substantial. To trigger shutdown, we atomically
// flip this flag; the event loop checks the flag and exits accordingly.
static SHUTDOWN_FLAG: AtomicBool = ATOMIC_BOOL_INIT;
unsafe fn handle_sigint(_:i32) {
    SHUTDOWN_FLAG.store(true, Ordering::Release);
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[inline]
fn tid_str() -> String {
    // gettid only exists for the linux and android variants of nix
    format!("({}) ", nix::unistd::gettid())
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
#[inline]
fn tid_str() -> &'static str {
    ""
}

fn main() {
    unsafe {
        libc::signal(SIGINT, handle_sigint as sighandler_t);
    }

    let mut builder = LogBuilder::new();
    let istty = unsafe { libc::isatty(libc::STDERR_FILENO as i32) } != 0;
    if istty {
        // Colorized output formatter
        let format = |record: &LogRecord| {
            let t = time::now();
            let level_color = match record.level() {
                log::LogLevel::Error => "\x1b[1;31m",  // bold red
                log::LogLevel::Warn  => "\x1b[1;33m",  // bold yellow
                log::LogLevel::Info  => "\x1b[1;32m",  // bold green
                log::LogLevel::Debug => "\x1b[1;34m",  // bold blue
                log::LogLevel::Trace => "\x1b[1;35m"   // bold magenta
            };
            format!("[\x1b[90m{}.{:03}\x1b[0m] {}{}{:5}\x1b[0m {}",
                time::strftime("%Y-%m-%d %H:%M:%S", &t).unwrap(),
                t.tm_nsec / 1_000_000,
                tid_str(),
                level_color,
                record.level(),
                record.args()
            )
        };
        builder.format(format).filter(None, LogLevelFilter::Info);
    } else {
        // Plain output formatter
        let format = |record: &LogRecord| {
            let t = time::now();
            format!("{}.{:03} {}{:5} {}",
                time::strftime("%Y-%m-%d %H:%M:%S", &t).unwrap(),
                t.tm_nsec / 1_000_000,
                tid_str(),
                record.level(),
                record.args()
            )
        };
        builder.format(format).filter(None, LogLevelFilter::Info);
    }

    if env::var("RUST_LOG").is_ok() {
       builder.parse(&env::var("RUST_LOG").unwrap());
    }
    builder.init().unwrap();

    let args: Args = Args::docopt().decode().unwrap_or_else(|e| e.exit());

    let local_name = update_hostname(args.flag_local_name.to_owned());
    let local_name = format!("{}.local", local_name);

    let mut controller = FoxBox::new(
        args.flag_verbose, local_name.clone(), args.flag_port,
        args.flag_wsport,
        if args.flag_disable_tls { TlsOption::Disabled } else { TlsOption::Enabled },
        match args.flag_profile {
            Some(p) => ProfilePath::Custom(p),
            None => ProfilePath::Default
        },
        args.flag_jsrunner);

    // Override config values
    {
        if let Some(flags) = args.flag_config {
            for flag in flags {
                let items: Vec<String> = utils::split_escaped(&flag, ';');
                if items.len() >= 3 {
                    let namespace = items[0].clone();
                    let key = items[1].clone();
                    let value = items[2..].join(";");
                    warn!("Setting config override: {}::{}->{}", namespace, key, value);
                    controller.config.set_override(&namespace, &key, &value);
                } else {
                    error!("Config override requires three fields: {}", flag)
                }
            }
        }
    }

    // The registrar manages registration with the registration server, and DNS
    // server. The registration server is used to orchestrate box discovery by
    // clients via an "nUPNP like" method where the box registers itself with an
    // externally available cloud service that a client can use to discover any
    // boxes local to itself. See: https://github.com/fxbox/registration_server
    //
    // The registrar also manages the assignment of names resolvable via a public
    // DNS server. The box registers its local ip address with a DNS server so that
    // a name can be resolved to the _local_ ip address. It also registers a unique
    // domain for the HTTPS tunnel. These public domain names are then verifiable
    // by LetsEncrypt during the validation phase using a dns-01 challenge.
    // See: https://letsencrypt.github.io/acme-spec/#dns
    //
    // Once the names have been created in the DNS server, a LetsEncrypt client will
    // issue certificates for each name - the local name will be the common name of
    // the certificate, and every other name will be a subject alternative name.
    let registrar = registration::Registrar::new(controller.get_certificate_manager(),
                                                 args.flag_dns_domain,
                                                 args.flag_register,
                                                 args.flag_dns_api);

    // Start the tunnel.
    let mut tunnel: Option<Tunnel> = None;
    if let Some(tunnel_url) = args.flag_tunnel {
        tunnel = Some(Tunnel::new(TunnelConfig::new(tunnel_url,
                                                    args.flag_tunnel_secret,
                                                    args.flag_port,
                                                    args.flag_wsport,
                                                    registrar.get_remote_dns_name())));
        tunnel.as_mut().unwrap().start().unwrap();
    }

    registrar.start(args.flag_iface, &tunnel,
                    args.flag_port,  &controller);

    controller.run(&SHUTDOWN_FLAG);

    if let Some(mut tunnel) = tunnel {
        tunnel.stop().unwrap();
    }
}

#[cfg(test)]
describe! main {
    describe! args {
        it "should have default values" {
            let argv = || vec!["foxbox"];
            let args: super::super::Args = super::super::Args::docopt().argv(argv().into_iter())
                .decode().unwrap();

            assert_eq!(args.flag_verbose, false);
            assert_eq!(args.flag_local_name, "foxbox");
            assert_eq!(args.flag_port, 3000);
            assert_eq!(args.flag_wsport, 4000);
            assert_eq!(args.flag_register, "http://knilxof.org:4242");
            assert_eq!(args.flag_dns_domain, "box.knilxof.org");
            assert_eq!(args.flag_dns_api, "https://knilxof.org:5300");
            assert_eq!(args.flag_iface, None);
            assert_eq!(args.flag_tunnel, None);
            assert_eq!(args.flag_config, None);
            assert_eq!(args.flag_help, false);
        }

        it "should support short form" {
            let argv = || vec!["foxbox",
                               "-v",
                               "-p", "1234",
                               "-l", "foobar",
                               "-w", "4567",
                               "-r", "http://foo.bar:6868/register",
                               "-i", "eth99",
                               "-t", "tunnel.host",
                               "-c", "ns;key;value"];

           let args: super::super::Args = super::super::Args::docopt().argv(argv().into_iter())
               .decode().unwrap();

            assert_eq!(args.flag_verbose, true);
            assert_eq!(args.flag_local_name, "foobar");
            assert_eq!(args.flag_port, 1234);
            assert_eq!(args.flag_wsport, 4567);
            assert_eq!(args.flag_register, "http://foo.bar:6868/register");
            assert_eq!(args.flag_iface.unwrap(), "eth99");
            assert_eq!(args.flag_tunnel.unwrap(), "tunnel.host");
            assert_eq!(args.flag_config.unwrap(), vec!["ns;key;value"]);
        }

        it "should support long form" {
            let argv = || vec!["foxbox",
                               "--verbose",
                               "--port", "1234",
                               "--local-name", "foobar",
                               "--wsport", "4567",
                               "--register", "http://foo.bar:6868/register",
                               "--iface", "eth99",
                               "--tunnel", "tunnel.host",
                               "--config", "ns;key;value"];

            let args: super::super::Args = super::super::Args::docopt().argv(argv().into_iter())
                .decode().unwrap();

            assert_eq!(args.flag_verbose, true);
            assert_eq!(args.flag_local_name, "foobar");
            assert_eq!(args.flag_port, 1234);
            assert_eq!(args.flag_wsport, 4567);
            assert_eq!(args.flag_register, "http://foo.bar:6868/register");
            assert_eq!(args.flag_iface.unwrap(), "eth99");
            assert_eq!(args.flag_tunnel.unwrap(), "tunnel.host");
            assert_eq!(args.flag_config.unwrap(), vec!["ns;key;value"]);
        }
    }
}
