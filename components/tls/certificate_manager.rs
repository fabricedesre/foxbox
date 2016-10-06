/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::io;
use std::io::{ Error as IoError };
use std::path::PathBuf;
use std::sync::{ Arc, RwLock };

use certificate_record::CertificateRecord;
use ssl_context::SslContextProvider;
use utils::*;

const DEFAULT_BOX_NAME: &'static str = "foxbox.local";

#[derive(Clone)]
pub struct CertificateManager {
    directory: PathBuf,
    ssl_hosts: Arc<RwLock<HashMap<String, CertificateRecord>>>,

    // Observer
    context_provider: Arc<Box<SslContextProvider>>
}

impl CertificateManager {
    pub fn new(directory: PathBuf, context_provider: Box<SslContextProvider>) -> Self {
        CertificateManager {
            directory: directory,
            ssl_hosts: Arc::new(RwLock::new(HashMap::new())),
            context_provider: Arc::new(context_provider),
        }
    }

    #[cfg(test)]
    pub fn new_for_test() -> Self {
        use ssl_context::SniSslContextProvider;
        let mut test_certs_directory = PathBuf::from(current_dir!());
        test_certs_directory.push("test_fixtures");
        test_certs_directory.push("certs");

        CertificateManager {
            directory: test_certs_directory,
            ssl_hosts: Arc::new(RwLock::new(HashMap::new())),
            context_provider: Arc::new(Box::new(SniSslContextProvider::new()))
        }
    }

    pub fn get_certs_dir(&self) -> PathBuf {
        self.directory.clone()
    }

    pub fn get_box_certificate(&self) -> io::Result<CertificateRecord> {
        self.get_or_generate_self_signed_certificate(DEFAULT_BOX_NAME)
    }

    /// Generate a self signed certificate for the given name.
    /// This will write the self signed certificates to the filesystem that this
    /// CertificateManager is configured for.
    fn get_or_generate_self_signed_certificate(&self, hostname: &str)
        -> io::Result<CertificateRecord> {

        if let Some(certificate_record) = self.get_certificate(hostname) {
            debug!("Using existing self-signed cert for {}", hostname);
            return Ok(certificate_record);
        }

        // Reload before this operation so we don't
        // overwrite any existing certificates.
        // NOTE: This could be racy, however a race won't happen unless
        // we're generating self signed certs for the same name.
        try!(self.reload());

        if let Some(certificate_record) = self.get_certificate(hostname) {
            debug!("Using existing self-signed cert for {}", hostname);
            return Ok(certificate_record);
        }

        let result = generate_self_signed_certificate(hostname, self.directory.clone());
        if let Ok(certificate_record) = result {
            self.add_certificate(certificate_record.clone());
            Ok(certificate_record)
        } else {
            result
        }
    }

    pub fn reload(&self) -> Result<(), IoError> {
        let certificates =  try!(create_records_from_directory(&self.directory.clone()));
        {
            let mut current_hosts = checklock!(self.ssl_hosts.write());
            current_hosts.clear();
            current_hosts.extend(certificates);
        }

        self.notify_provider();
        Ok(())
    }

    pub fn add_certificate(&self, certificate_record: CertificateRecord) {
        {
            checklock!(self.ssl_hosts.write())
                .insert(certificate_record.hostname.clone(), certificate_record);
        }

        self.notify_provider();
    }

    pub fn get_certificate(&self, hostname: &str) -> Option<CertificateRecord> {
        let ssl_hosts = checklock!(self.ssl_hosts.read());
        let cert_record = ssl_hosts.get(hostname);

        if let Some(record) = cert_record {
            Some(record.clone())
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn remove_certificate(&self, hostname: &str) {
        {
            checklock!(self.ssl_hosts.write()).remove(hostname);
        }

        self.notify_provider();
    }

    pub fn get_context_provider(&self) -> Arc<Box<SslContextProvider>> {
        self.context_provider.clone()
    }

    fn notify_provider(&self) {
        let ssl_hosts = checklock!(self.ssl_hosts.read()).clone();

        self.context_provider.update(ssl_hosts);
    }
}

#[cfg(test)]
mod certificate_manager_test {
    use openssl::ssl::{ SslContext, SslMethod };
    use std::collections::HashMap;
    use std::io::{ Error, ErrorKind };
    use std::path::PathBuf;
    use std::sync::{ Arc, Mutex };
    use std::sync::mpsc::{ channel, Sender };
    use certificate_record::CertificateRecord;
    use ssl_context::SslContextProvider;

    use super::*;

    #[derive(Clone)]
    pub struct TestSslContextProvider {
        update_called: Arc<Mutex<Sender<bool>>>
    }

    impl TestSslContextProvider {
        fn new(update_chan: Sender<bool>) -> Self {
            TestSslContextProvider {
                update_called: Arc::new(Mutex::new(update_chan))
            }
        }
    }

    impl SslContextProvider for TestSslContextProvider {
        fn context(&self) -> Result<SslContext, Error> {
            SslContext::new(SslMethod::Sslv23).map_err(|_| {
                Error::new(ErrorKind::InvalidInput, "An SSL certificate could not be configured")
            })
        }

        fn update(&self, _: HashMap<String, CertificateRecord>) -> () {
            self.update_called.lock().unwrap().send(true).unwrap_or(())
        }
    }

    fn test_cert_record() -> CertificateRecord {
        CertificateRecord::new_from_components(
            "test.example.com".to_owned(),
            PathBuf::from("/test/privkey.pem"),
            PathBuf::from("/test/cert.pem"),
            "010203040506070809000a0b0c0d0e0f".to_owned()
        ).unwrap()
    }

    #[test]
    fn should_allow_certificates_to_be_added() {
        let cert_record = test_cert_record();
        let (tx_update_called, _) = channel();

        let cert_manager = CertificateManager::new(
            PathBuf::from(current_dir!()),
            Box::new(TestSslContextProvider::new(tx_update_called))
        );

        cert_manager.add_certificate(cert_record.clone());

        let certificate = cert_manager.get_certificate("test.example.com")
                                      .unwrap();

        assert!(certificate == cert_record);

        cert_manager.remove_certificate("test.example.com");
        assert!(cert_manager.get_certificate("test.example.com").is_none());
    }

    #[test]
    fn should_allow_certificates_to_be_removed() {
        let cert_record = test_cert_record();
        let (tx_update_called, _) = channel();
        let cert_manager = CertificateManager::new(
            PathBuf::from(current_dir!()),
            Box::new(TestSslContextProvider::new(tx_update_called))
        );

        cert_manager.add_certificate(cert_record);

        cert_manager.remove_certificate("test.example.com");

        assert!(cert_manager.get_certificate("test.example.com").is_none());
    }

    #[test]
    fn should_update_configured_providers_when_cert_added() {
        let cert_record = test_cert_record();
        let (tx_update_called, rx_update_called) = channel();
        let cert_manager = CertificateManager::new(
            PathBuf::from(current_dir!()),
            Box::new(TestSslContextProvider::new(tx_update_called))
        );

        cert_manager.add_certificate(cert_record);

        assert!(
            rx_update_called.recv().unwrap(),
            "Did not receive notification from handler after add"
        );
    }

    #[test]
    fn should_update_configured_providers_when_cert_removed() {
        let cert_record = test_cert_record();
        let (tx_update_called, rx_update_called) = channel();
        let cert_manager = CertificateManager::new(
            PathBuf::from(current_dir!()),
            Box::new(TestSslContextProvider::new(tx_update_called))
        );

        cert_manager.add_certificate(cert_record);

        assert!(
            rx_update_called.recv().unwrap(),
            "Did not receive notification from handler after add"
        );

        cert_manager.remove_certificate(&test_cert_record().hostname);

        assert!(
            rx_update_called.recv().unwrap(),
            "Did not receive notification from handler after remove"
        );
    }
}
