/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// This manages registration of the foxbox with the discovery endpoint.
/// For now it simply register itselfs every N minutes with the endpoint,
/// after trying more aggressively at first run.

extern crate get_if_addrs;
extern crate hyper;

use self::hyper::Client;
use self::hyper::header::Connection;
use self::hyper::status::StatusCode;
use self::get_if_addrs::{ IfAddr, Interface };
use foxbox_core::traits::Controller;
use serde_json;
use std::io::Read;
use std::time::Duration;
use std::thread;
use tls::{ CertificateManager, DnsRecord, get_san_cert_for, register_dns_record };
use tunnel_controller:: { Tunnel };

const REGISTRATION_INTERVAL_IN_MINUTES: u32 = 1;

pub struct Registrar {
    certificate_manager: CertificateManager,
    top_level_domain: String,
    registration_endpoint: String,
    dns_api_endpoint: String,
}

#[derive(Serialize, Debug)]
struct RegistrationRequest {
    message: String,
    client: String,

    local_ip: String,
}

impl Registrar {
    pub fn new(certificate_manager: CertificateManager,
               top_level_domain: String,
               registration_endpoint: String,
               dns_api_endpoint: String) -> Registrar {
        Registrar {
            certificate_manager: certificate_manager,
            top_level_domain: top_level_domain,
            registration_endpoint: format!("{}/register", registration_endpoint),
            dns_api_endpoint: dns_api_endpoint,
        }
    }

    fn get_fingerprint(&self) -> String {
        self.certificate_manager.get_box_certificate()
                                .unwrap()
                                .get_certificate_fingerprint()
    }

    fn get_common_name(&self) -> String {
        format!("{}.{}", self.get_fingerprint(), self.top_level_domain)
    }

    pub fn get_local_dns_name(&self) -> String {
        format!("local.{}", self.get_common_name())
    }

    pub fn get_remote_dns_name(&self) -> String {
        format!("remote.{}", self.get_common_name())
    }

    fn register_with_registration_server(&self, ip_addr: String, http_scheme: &str, box_port: u16, tunnel_enabled: bool) -> () {
        let message = json!({
            local_origin: format!("{}://{}:{}", http_scheme, self.get_local_dns_name(), box_port),
            tunnel_origin: if tunnel_enabled {
                Some(format!("{}://{}", http_scheme, self.get_remote_dns_name()))
            } else {
                None
            }
        });

        let body = match serde_json::to_string(&RegistrationRequest {
            message: message,
            client: self.get_fingerprint(),
            local_ip: ip_addr,
        }) {
            Ok(body) => body,
            Err(_) => {
                error!("registration server: Serialization error. Will not send registration request.");
                return;
            }
        };

        let client = Client::new();
        let res = client.post(&self.registration_endpoint)
            .header(Connection::close())
            .body(&body)
            .send();

        // Sanity checks, mostly to debug errors since we don't try
        // to recover from failures.
        if let Ok(mut response) = res {
            if response.status == StatusCode::Ok {
                let mut body = String::new();
                if let Ok(_) = response.read_to_string(&mut body) {
                    info!("registration server responded with: {}", body);
                } else {
                    warn!("registration server: Unable to read answer from {}", self.registration_endpoint);
                }
            }
        } else {
            warn!("registration server: Unable to send request to {}", self.registration_endpoint);
        }
    }

    /// Registers the boxes local IP address as an A record with the DNS server, and
    /// registers a CNAME record for the tunnel endpoint using the box's assigned
    /// names (local.<fingerprint>.box.knilxof.org and
    /// remote.<fingerprint>.box.knilxof.org).  The remote name (tunnel name), is
    /// only configured if the tunnel_frontend option is non-None.
    fn register_with_dns_server(&self, ip_addr: String, tunnel_frontend: Option<String>) {
        let client_certificate = self.certificate_manager.get_box_certificate().unwrap();

        let local_name = self.get_local_dns_name();
        // Create entry for local DNS
        info!("DNS server: Creating DNS entry for {}", local_name);
        let result = register_dns_record(
            client_certificate.clone(),
            &DnsRecord {
                record_type: "A",
                name: &local_name,
                value: &ip_addr,
            },
            &self.dns_api_endpoint.clone(),
        );

        if let Err(_) = result {
            warn!("DNS server: Could not create DNS entry for {}", local_name);
        }

        if let Some(tunnel_frontend) = tunnel_frontend {
            let remote_name = self.get_remote_dns_name();
            info!("DNS server: Creating DNS entry for {}", remote_name);
            let result = register_dns_record(
                client_certificate.clone(),
                &DnsRecord {
                    record_type: "CNAME",
                    name: &remote_name,
                    value: &tunnel_frontend,
                },
                &self.dns_api_endpoint.clone(),
            );

            if let Err(_) = result {
                warn!("DNS server: Could not create DNS entry for {}", remote_name);
            }
        }
    }

    fn register_certificates(&self) {
        if self.certificate_manager.get_certificate(&self.get_local_dns_name()).is_none() {
            let domains = vec![self.get_local_dns_name(), self.get_remote_dns_name()];

            info!("Getting/renewing LetsEncrypt certificate for: {:?}", domains);
            let rx = get_san_cert_for(
                domains.into_iter(),
                self.certificate_manager.clone(),
                self.dns_api_endpoint.clone()
            );

            rx.recv().unwrap().unwrap();
            self.certificate_manager.reload().unwrap();
        }
    }

    pub fn start<T: Controller>(self,
                                iface: Option<String>,
                                tunnel: &Option<Tunnel>,
                                box_port: u16,
                                controller: &T) {
        info!("registration server: Starting registration with {}",
                self.registration_endpoint);

        let ip_addr = self.get_ip_addr(&iface);
        if ip_addr == None {
            // TODO: retry later, in case we're racing with the network
            // configuration. https://github.com/fxbox/foxbox/issues/347
            return;
        }

        info!("Got ip address: {}", ip_addr.clone().unwrap());

        let tunnel_frontend = if let Some(ref tunnel) = *tunnel {
            tunnel.get_frontend_name()
        } else {
            None
        };
        let enabled_tls = controller.get_tls_enabled();

        let http_scheme = if enabled_tls {
            "https"
        } else {
            "http"
        };

        // Spawn a thread to register every REGISTRATION_INTERVAL_IN_MINUTES.
        thread::Builder::new().name("Registrar".to_owned())
            .spawn(move || {
                let tunnel_configured = tunnel_frontend.clone().is_some();

                if enabled_tls {
                    self.register_certificates();
                }

                loop {
                    // TODO: If the ip address changes, we need to update the dns server and
                    // registration server with the new IP address.
                    // https://github.com/fxbox/foxbox/issues/348
                    self.register_with_registration_server(
                        ip_addr.clone().unwrap(),
                        http_scheme,
                        box_port,
                        tunnel_configured
                    );
                    self.register_with_dns_server(ip_addr.clone().unwrap(), tunnel_frontend.clone());

                    // Go to sleep.
                    thread::sleep(Duration::from_secs(REGISTRATION_INTERVAL_IN_MINUTES as u64 * 60))
                }
            }).unwrap();
    }

    /// return the host IP address of the first valid interface.
    /// want_iface is an options string for the interface you want.
    pub fn get_ip_addr(&self, want_iface: &Option<String>) -> Option<String> {
        // Look for an ipv4 interface on eth* or wlan*.
        if let Ok(ifaces) = get_if_addrs::get_if_addrs() {
            if ifaces.is_empty() {
                error!("No IP interfaces found!");
                return None;
            }

            self.get_ip_addr_from_ifaces(&ifaces, want_iface)
        } else {
            error!("No IP interfaces found!");
            None
        }
    }

    /// This is a private function that to which we pass the ifaces
    /// This is so that we can shim get_if_addrs() in tests with a
    /// pre-set list of interfaces.
    fn get_ip_addr_from_ifaces(&self, ifaces: &[Interface],
                               want_iface: &Option<String>) -> Option<String> {

        let mut ip_addr: Option<String> = None;
        let mut ipv6_addr: Option<String> = None;

        for iface in ifaces {
            match want_iface.as_ref() {
                None =>
                    // Whitelist known good iface
                    if !(iface.name.starts_with("eth") ||
                         iface.name.starts_with("wlan") ||
                         iface.name.starts_with("en") ||
                         iface.name.starts_with("em") ||
                         iface.name.starts_with("wlp3s") ||
                         iface.name.starts_with("wlp4s")) {
                        continue;
                    },
                    Some(iface_name) =>
                        if &iface.name != iface_name {
                            continue;
                        }
            }
            if let IfAddr::V4(ref v4) = iface.addr {
                ip_addr = Some(format!("{}", v4.ip));
                break;
            } else if ipv6_addr.is_none() {
                if let IfAddr::V6(ref v6) = iface.addr {
                    ipv6_addr = Some(format!("{}", v6.ip));
                }
            }
        }

        if ip_addr.is_none() {
            if ipv6_addr.is_none() {
                error!("No IP interfaces found!");
            } else {
                ip_addr = ipv6_addr;
            }
        }
        ip_addr
    }

}

#[cfg(test)]
describe! registrar {

    before_each {
        use std::path::PathBuf;
        use tls::{ CertificateManager, SniSslContextProvider };
        let registrar = Registrar::new(
            CertificateManager::new(PathBuf::from(current_dir!()), Box::new(SniSslContextProvider::new())),
            "box.knilxof.org".to_owned(),
            "https://knilxof.org:4443/".to_owned(),
            "https://knilxof.org:5300".to_owned()
        );
    }

    it "should return an IP address when a machine has network interfaces" {
        use regex::Regex;

        let ip = registrar.get_ip_addr(&None).unwrap();
        let ipv4_regex = Regex::new(r"^(\d{1,3}\.){3}\d{1,3}$").unwrap();
        assert!(ipv4_regex.is_match(ip.as_str()));
    }

    describe! ipv4 {
        before_each {
            use super::super::get_if_addrs::*;
            use std::net::Ipv4Addr;

            let interfaces: Vec<Interface> = vec![
                Interface {
                    name: "lo".to_owned(),
                    addr: IfAddr::V4(Ifv4Addr {
                        ip: Ipv4Addr::new(127,0,0,1),
                        netmask: Ipv4Addr::new(255,0,0,0),
                        broadcast: None
                    })
                },
                Interface {
                    name: "docker0".to_owned(),
                    addr: IfAddr::V4(Ifv4Addr {
                        ip: Ipv4Addr::new(172,18,1,42),
                        netmask: Ipv4Addr::new(255,255,255,0),
                        broadcast: None
                    })
                },
                Interface {
                    name: "eth0".to_owned(),
                    addr: IfAddr::V4(Ifv4Addr {
                        ip: Ipv4Addr::new(192,168,0,4),
                        netmask: Ipv4Addr::new(255,255,255,0),
                        broadcast: Some(Ipv4Addr::new(192,160,0,255))
                    })
                },
                Interface {
                    name: "wlan0".to_owned(),
                    addr: IfAddr::V4(Ifv4Addr {
                        ip: Ipv4Addr::new(192,168,0,14),
                        netmask: Ipv4Addr::new(255,255,255,0),
                        broadcast: Some(Ipv4Addr::new(192,160,0,255))
                    })
                }
            ];
        }

        it "should default to eth0" {
            let ip_addr = registrar.get_ip_addr_from_ifaces(&interfaces, &None).unwrap();
            assert_eq!(ip_addr, "192.168.0.4");
        }

        it "should retrieve address from eth0" {
            let ip_addr = registrar.get_ip_addr_from_ifaces(&interfaces, &Some("eth0".to_owned()))
                .unwrap();
            assert_eq!(ip_addr, "192.168.0.4");
        }

        it "should retrieve address from wlan0" {
            let ip_addr = registrar.get_ip_addr_from_ifaces(&interfaces, &Some("wlan0".to_owned()))
                .unwrap();
            assert_eq!(ip_addr, "192.168.0.14");
        }

        it "should retrieve address from docker0" {
            let ip_addr = registrar.get_ip_addr_from_ifaces(&interfaces, &Some("docker0".to_owned())).unwrap();
            assert_eq!(ip_addr, "172.18.1.42");
        }
    }

    describe! ipv6 {
        before_each {
            use super::super::get_if_addrs::*;
            use std::net::{ Ipv4Addr, Ipv6Addr };

            let interfaces: Vec<Interface> = vec![
                Interface {
                    name: "eth0".to_owned(),
                    addr: IfAddr::V6(Ifv6Addr {
                        ip: Ipv6Addr::new(0,0,0,0,0,0xffff,0xc0a8,0x4),
                        netmask: Ipv6Addr::new(0xffff,0xffff,0xffff,0xffff,
                                               0xffff,0xffff,0xffff,0xffff),
                        broadcast: None
                    })
                },
                Interface {
                    name: "eth0".to_owned(),
                    addr: IfAddr::V4(Ifv4Addr {
                        ip: Ipv4Addr::new(192,168,0,4),
                        netmask: Ipv4Addr::new(255,255,255,0),
                        broadcast: Some(Ipv4Addr::new(192,160,0,255))
                    })
                },
                Interface {
                    name: "eth1".to_owned(),
                    addr: IfAddr::V6(Ifv6Addr {
                        ip: Ipv6Addr::new(0,0,0,0,0,0xffff,0xc0a8,0x4),
                        netmask: Ipv6Addr::new(0xffff,0xffff,0xffff,0xffff,
                                               0xffff,0xffff,0xffff,0xffff),
                        broadcast: None
                    })
                }
            ];
        }

        it "should return default IPv4" {
            let ip_addr = registrar.get_ip_addr_from_ifaces(&interfaces, &None)
                .unwrap();
            assert_eq!(ip_addr, "192.168.0.4");
        }

        it "should return IPv6" {
            let ip_addr = registrar.get_ip_addr_from_ifaces(&interfaces, &Some("eth1".to_owned()))
                .unwrap();
            assert_eq!(ip_addr, "::ffff:192.168.0.4");
        }

        it "should return IPv4 if both are specified" {
            let ip_addr = registrar.get_ip_addr_from_ifaces(&interfaces, &Some("eth0".to_owned()))
                .unwrap();
            assert_eq!(ip_addr, "192.168.0.4");
        }

        it "should return IPv4 and eth0 by default" {
            let ip_addr = registrar.get_ip_addr_from_ifaces(&interfaces, &None).unwrap();
            assert_eq!(ip_addr, "192.168.0.4");
        }
    }
}
