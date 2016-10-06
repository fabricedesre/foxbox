/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::io;
use std::io::{ Error, ErrorKind };
use std::fs;
use std::path::PathBuf;

use openssl::x509::X509;
use openssl::crypto::hash::Type;

const FINGERPRINT_DIGEST: Type = Type::SHA1;

pub fn vec_to_str(sha_vec: Vec<u8>) -> String {
    sha_vec.iter().fold("".to_owned(), |hash, component| {
        // Formatting is important, each byte must be printed as width '2'
        // with a '0' fill
        format!("{}{:02x}", hash, component)
    })
}

/// Defines a certificate, including the hostname it is for,
/// the private key file and the certificate file.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CertificateRecord {
    pub hostname: String,
    pub private_key_file: PathBuf,
    pub cert_file: PathBuf,
    pub full_chain: Option<PathBuf>,

    cert_fingerprint: String
}

fn get_x509_sha1_fingerprint_from_pem(pem_file: PathBuf) -> io::Result<String> {
    let mut file = try!(fs::File::open(pem_file.clone()));
    match X509::from_pem(&mut file) {
        Ok(pem_file_x509) => {
            pem_file_x509.fingerprint(FINGERPRINT_DIGEST)
                         .map(vec_to_str)
                         .ok_or_else(|| {
                             Error::new(
                                 ErrorKind::InvalidData,
                                 format!(
                                     "The PEM file '{:?}' is not valid (a fingerprint could not be determined)",
                                     pem_file
                                 )
                            )
                         })
        },
        Err(error) => {
            Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Could not load PEM file '{:?}': {}", pem_file, error))
            )
        }
    }
}

impl CertificateRecord {

    pub fn new(hostname: String,
               certificate_file: PathBuf,
               private_key_file: PathBuf,
               full_chain: Option<PathBuf>) -> io::Result<Self> {
        // Load PEMs

        let cert_fingerprint = try!(get_x509_sha1_fingerprint_from_pem(certificate_file.clone()));

        Ok(CertificateRecord {
            hostname: hostname,
            cert_file: fs::canonicalize(certificate_file).unwrap(),
            private_key_file: fs::canonicalize(private_key_file).unwrap(),

            full_chain: full_chain,

            cert_fingerprint: cert_fingerprint,
        })
    }

    /// Create a test CertificateRecord directly from its components
    /// (doesn't use the file system to load the PEM cert and get its
    /// fingerprint. Useful for testing.
    pub fn new_from_components(hostname: String,
                        certificate_file: PathBuf,
                        private_key_file: PathBuf,
                        cert_fingerprint: String) -> io::Result<Self> {

        Ok(CertificateRecord {
            hostname: hostname,
            cert_file: certificate_file,
            private_key_file: private_key_file,
            full_chain: None,
            cert_fingerprint: cert_fingerprint,
        })
    }

    pub fn get_certificate_fingerprint(&self) -> String {
        self.cert_fingerprint.clone()
    }
}

#[cfg(test)]
mod certificate_record_test {
    use std::path::PathBuf;
    use super::*;
    use super::get_x509_sha1_fingerprint_from_pem;

    #[test]
    fn test_fingerprint_certificate() {
        let mut cert_file = PathBuf::from(current_dir!());
        cert_file.push("test_fixtures");
        cert_file.push("cert.pem");
        assert_eq!(
            get_x509_sha1_fingerprint_from_pem(cert_file).unwrap(),
            "1fa576050d8b3710e57a2d62e84f6781504caf7e"
        );
    }

    #[test]
    fn test_vec_to_str() {
        let sha_vec: Vec<u8> = vec![0, 1, 2, 3, 4, 5, 6, 255, 244, 200, 100];

        assert!(vec_to_str(sha_vec) == "00010203040506fff4c864")
    }

    #[test]
    fn should_get_cert_fingerprint() {
        let certificate_record = CertificateRecord {
             cert_file: PathBuf::from("/test/cert.pem"),
             private_key_file: PathBuf::from("/test/privkey.pem"),
             hostname: "test.example.com".to_owned(),
             cert_fingerprint: "1234567890abcdef".to_owned(),
             full_chain: None
        };

        assert_eq!(
            certificate_record.get_certificate_fingerprint(),
            "1234567890abcdef"
        );
    }
}
