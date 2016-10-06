/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Cryptographic operations for `WebPush`.
//!
//! Implemented as described in the draft IETF RFC:
//! https://tools.ietf.org/html/draft-ietf-webpush-encryption-02
//! https://tools.ietf.org/html/draft-ietf-webpush-protocol-04
//! https://tools.ietf.org/html/draft-ietf-httpbis-encryption-encoding-01
//!

extern crate libc;
extern crate crypto;

#[cfg(test)]
use self::crypto::aead::AeadDecryptor;
use self::crypto::aead::AeadEncryptor;
use self::crypto::aes_gcm::AesGcm;
use self::crypto::aes::KeySize;
use self::crypto::hkdf::{ hkdf_expand, hkdf_extract };
use self::crypto::hmac::Hmac;
use self::crypto::sha2::Sha256;
use self::crypto::mac::Mac;

use std::cmp::min;
use std::ffi::{ CString, CStr };
use std::ptr;
use std::sync::{ Arc, Mutex };
use rand::Rng;
use rand::os::OsRng;

use rustc_serialize::base64::{ FromBase64, ToBase64, URL_SAFE };
use rustc_serialize::hex::{ ToHex, FromHex };

const NID_X9_62_PRIMVE256V1: libc::c_int = 415;

const EVP_PKEY_EC: libc::c_int = 408;
const EVP_PKEY_OP_PARAMGEN: libc::c_int = 2;
const EVP_PKEY_CTRL_EC_PARAMGEN_CURVE_NID: libc::c_int = 4097;

const AESGCM_TAG_LEN: usize = 16;

#[derive(Debug)]
pub struct EncryptData {
    pub salt: String,
    pub output: Vec<u8>
}

struct AuthData {
    pub auth: Vec<u8>,
    pub key_context: Vec<u8>
}

#[repr(C)]
enum EcPointConversion {
    Uncompressed = 4
}

type EcKey = libc::c_void;
type EcPoint = libc::c_void;
type EcGroup = libc::c_void;
type EvpPkey = libc::c_void;
type EvpPkeyCtx = libc::c_void;
type BnCtx = libc::c_void;

// TODO: switch to rust-openssl crate once the missing EC_*** and EVP_PKEY_*** APIs are added
//       instead of using the FFI directly
extern "C" {
    fn EC_KEY_new_by_curve_name(nid: libc::c_int) -> *mut EcKey;
    fn EC_KEY_free(key: *mut EcKey);
    fn EC_KEY_get0_group(key: *const EcKey) -> *const EcGroup;
    fn EC_KEY_set_public_key(key: *mut EcKey, pub_key: *const EcPoint) -> libc::c_int;
    fn EC_KEY_get0_public_key(key: *const EcKey) -> *mut EcPoint;

    fn EC_POINT_hex2point(group: *const EcGroup, hex: *const libc::c_char, p: *mut EcPoint, ctx: *mut BnCtx) -> *mut EcPoint;
    fn EC_POINT_point2hex(group: *const EcGroup, point: *const EcPoint, form: EcPointConversion, ctx: *mut BnCtx) -> *mut libc::c_char;
    fn EC_POINT_free(point: *mut EcPoint);

    fn EVP_PKEY_new() -> *mut EvpPkey;
    fn EVP_PKEY_free(pkey: *mut EvpPkey);
    fn EVP_PKEY_set1_EC_KEY(evpKey: *mut EvpPkey, ecKey: *mut EcKey) -> libc::c_int;
    fn EVP_PKEY_get1_EC_KEY(pkey: *mut EvpPkey) -> *mut EcKey;

    fn EVP_PKEY_CTX_new_id(id: libc::c_int, engine: *mut libc::c_void) -> *mut EvpPkeyCtx;
    fn EVP_PKEY_CTX_new(pkey: *mut EvpPkey, engine: *mut libc::c_void) -> *mut EvpPkeyCtx;
    fn EVP_PKEY_CTX_free(ctx: *mut EvpPkeyCtx);
    fn EVP_PKEY_CTX_ctrl(ctx: *mut EvpPkeyCtx, keytype: libc::c_int, optype: libc::c_int, cmd: libc::c_int, p1: libc::c_int, p2: *mut libc::c_void) -> libc::c_int;

    fn EVP_PKEY_paramgen_init(ctx: *mut EvpPkeyCtx) -> libc::c_int;
    fn EVP_PKEY_paramgen(ctx: *mut EvpPkeyCtx, ppkey: *mut *mut EvpPkey) -> libc::c_int;

    fn EVP_PKEY_keygen_init(ctx: *mut EvpPkeyCtx) -> libc::c_int;
    fn EVP_PKEY_keygen(ctx: *mut EvpPkeyCtx, ppkey: *mut *mut EvpPkey) -> libc::c_int;

    fn EVP_PKEY_derive_init(ctx: *mut EvpPkeyCtx) -> libc::c_int;
    fn EVP_PKEY_derive_set_peer(ctx: *mut EvpPkeyCtx, peer: *mut EvpPkey) -> libc::c_int;
    fn EVP_PKEY_derive(ctx: *mut EvpPkeyCtx, key: *mut libc::c_char, size: *mut libc::size_t) -> libc::c_int;

    fn CRYPTO_free(ptr: *mut libc::c_void);
}

/// Creates an `OpenSSL` representation of the given ECDH X9.62 public key,
/// represented as a string of hex digits.
fn ecdh_import_public_key(public_key : String) -> *mut EvpPkey {
    let eckey;
    let mut ecpoint = ptr::null_mut();
    let mut peer_key = ptr::null_mut();
    let native_key = CString::new(public_key).unwrap();

    unsafe {
        loop {
            eckey = EC_KEY_new_by_curve_name(NID_X9_62_PRIMVE256V1);
            if eckey.is_null() {
                warn!("cannot create EC X9.62 key");
                break;
            }

            let ecgroup = EC_KEY_get0_group(eckey);
            if ecgroup.is_null() {
                warn!("cannot get EC group from key");
                break;
            }

            ecpoint = EC_POINT_hex2point(ecgroup, native_key.as_ptr(), ptr::null_mut(), ptr::null_mut());
            if ecpoint.is_null() {
                warn!("cannot convert raw EC public key to EC point");
                break;
            }

            if EC_KEY_set_public_key(eckey, ecpoint) != 1 {
                warn!("cannot set EC public key");
                break;
            }

            peer_key = EVP_PKEY_new();
            if peer_key.is_null() {
                warn!("cannot create EVP pkey");
                break;
            }

            if EVP_PKEY_set1_EC_KEY(peer_key, eckey) != 1 {
                warn!("cannot initialize EVP pkey from EC key");
                EVP_PKEY_free(peer_key);
                peer_key = ptr::null_mut();
                break;
            }

            break;
        }

        if !eckey.is_null() { EC_KEY_free(eckey); }
        if !ecpoint.is_null() { EC_POINT_free(ecpoint); }
    }

    peer_key
}

/// Generates an `OpenSSL` representation of the parameters describing
/// an ECDH X9.62 key pair.
fn ecdh_generate_params() -> *mut EvpPkey {
    let ctx;
    let mut params = ptr::null_mut();

    unsafe {
        loop {
            ctx = EVP_PKEY_CTX_new_id(EVP_PKEY_EC, ptr::null_mut());
            if ctx.is_null() {
                warn!("cannot create param context");
                break;
            }

            if EVP_PKEY_paramgen_init(ctx) != 1 {
                warn!("cannot init param context");
                break;
            }

            if EVP_PKEY_CTX_ctrl(ctx, EVP_PKEY_EC, EVP_PKEY_OP_PARAMGEN, EVP_PKEY_CTRL_EC_PARAMGEN_CURVE_NID, NID_X9_62_PRIMVE256V1, ptr::null_mut()) != 1 {
                warn!("cannot set param context as X9.62 P256V1");
                break;
            }

            if EVP_PKEY_paramgen(ctx, &mut params) != 1 || params.is_null() {
                warn!("cannot generate params from context");
                break;
            }

            break;
        }

        if !ctx.is_null() { EVP_PKEY_CTX_free(ctx); }
    }

    params
}

/// Generates an `OpenSSL` representation of an ECDH X9.62 key pair.
fn ecdh_generate_key_pair() -> *mut EvpPkey {
    let params;
    let mut ctx = ptr::null_mut();
    let mut key = ptr::null_mut();

    unsafe {
        loop {
            params = ecdh_generate_params();
            if params.is_null() {
                break;
            }

            ctx = EVP_PKEY_CTX_new(params, ptr::null_mut());
            if ctx.is_null() {
                warn!("cannot create key context");
                break;
            }

            if EVP_PKEY_keygen_init(ctx) != 1 {
                warn!("cannot init key context");
                break;
            }

            if EVP_PKEY_keygen(ctx, &mut key) != 1 || key.is_null() {
                warn!("cannot generate public/private key pair from key context");
                break;
            }

            break;
        }

        if !ctx.is_null() { EVP_PKEY_CTX_free(ctx); }
        if !params.is_null() { EVP_PKEY_free(params); }
    }

    key
}

/// Derives a shared key from an ECDH X9.62 public/private key pair and
/// another (peer) ECDH X9.62 public key.
fn ecdh_derive_shared_key(local_key: *mut EvpPkey, peer_key: *mut EvpPkey) -> Option<Vec<u8>> {
    if local_key.is_null() || peer_key.is_null() {
        return None;
    }

    let mut status = None;
    let ctx;

    unsafe {
        loop {
            ctx = EVP_PKEY_CTX_new(local_key, ptr::null_mut());
            if ctx.is_null() {
                warn!("cannot create shared key context");
                break;
            }

            if EVP_PKEY_derive_init(ctx) != 1 {
                warn!("cannot init shared context");
                break;
            }

            if EVP_PKEY_derive_set_peer(ctx, peer_key) != 1 {
                warn!("cannot set peer key for shared context");
                break;
            }

            let mut shared_len : libc::size_t = 0;
            if EVP_PKEY_derive(ctx, ptr::null_mut(), &mut shared_len) != 1 {
                warn!("cannot get shared key length from shared context");
                break;
            }

            let shared_key = vec![0u8; shared_len];
            if EVP_PKEY_derive(ctx, shared_key.as_ptr() as *mut libc::c_char, &mut shared_len) != 1 {
                warn!("cannot get shared key from shared context");
                break;
            }

            status = Some(shared_key);
            break;
        }

        if !ctx.is_null() { EVP_PKEY_CTX_free(ctx); }
    }

    status
}

/// Creates a string of hex digits representing an ECDH X9.62 public key
/// given an `OpenSSL` public/private key pair.
fn ecdh_export_public_key(key: *mut EvpPkey) -> Option<String> {
    if key.is_null() {
        return None;
    }

    let mut status = None;
    let mut buf = ptr::null_mut();
    let eckey;

    unsafe {
        loop {
            eckey = EVP_PKEY_get1_EC_KEY(key);
            if eckey.is_null() {
                warn!("cannot get local ec key from local key");
                break;
            }

            let ecpoint = EC_KEY_get0_public_key(eckey);
            if ecpoint.is_null() {
                warn!("cannot get public key from local ec key");
                break;
            }

            let ecgroup = EC_KEY_get0_group(eckey);
            if ecgroup.is_null() {
                warn!("cannot get group from local ec key");
            }

            buf = EC_POINT_point2hex(ecgroup, ecpoint, EcPointConversion::Uncompressed, ptr::null_mut());
            if buf.is_null() {
                warn!("cannot get uncompressed public key from local ec point");
                break;
            }

            status = Some(CStr::from_ptr(buf).to_string_lossy().into_owned());
            break;
        }

        if !buf.is_null() { CRYPTO_free(buf as *mut libc::c_void); }
        if !eckey.is_null() { EC_KEY_free(eckey); }
    }

    status
}

struct KeyPairStore {
    key: *mut EvpPkey,
}

impl Drop for KeyPairStore {
    fn drop(&mut self) {
        if !self.key.is_null() {
            unsafe { EVP_PKEY_free(self.key); }
        }
    }
}

#[derive(Clone)]
pub struct CryptoContext {
    /// base64 encoding representing the local public key.
    public_key: String,
    key_pair: Arc<Mutex<KeyPairStore>>
}

unsafe impl Send for CryptoContext {}
unsafe impl Sync for CryptoContext {}

impl CryptoContext {
    pub fn new() -> Option<Self> {
        let local_key = ecdh_generate_key_pair();
        let public_key = ecdh_export_public_key(local_key);

        if local_key.is_null() || public_key.is_none() {
            return None;
        }

        let public_key_bytes = match public_key.unwrap().from_hex() {
            Ok(x) => x,
            Err(e) => {
                warn!("Could not derive keys: {:?}", e);
                return None;
            }
        };

        Some(CryptoContext {
            public_key: public_key_bytes.to_base64(URL_SAFE),
            // This needs to be protected by a mutex because OpenSSL updates
            // the reference count, even if we shouldn't need to modify anything
            // else with the local key.
            key_pair: Arc::new(Mutex::new(KeyPairStore {
                key: local_key
            }))
        })
    }

    pub fn get_public_key(&self, auth: bool) -> String {
        if auth {
            self.public_key.replace("=", "")
        } else {
            self.public_key.clone()
        }
    }

    /// Derives a shared key from the given peer's public key and our local key pair.
    ///
    /// * `raw_peer_key` is the peer's public ECDH key represented as hex digits.
    fn ecdh_derive_keys(&self, raw_peer_key : String) -> Option<Vec<u8>> {
        let peer_key = ecdh_import_public_key(raw_peer_key);
        let key_pair = self.key_pair.lock().unwrap().key;
        let shared_key = ecdh_derive_shared_key(key_pair, peer_key);
        if !peer_key.is_null() {
            unsafe { EVP_PKEY_free(peer_key); }
        }
        shared_key
    }

    fn aesgcm128_append_key(key_context: &mut Vec<u8>, key: &[u8]) {
        assert!(key.len() <= 255);
        key_context.push(0u8);
        key_context.push(key.len() as u8);
        key_context.extend_from_slice(key);
    }

    fn aesgcm128_auth_data(&self, auth: &Option<String>, peer_key: &[u8], encrypt: bool) -> Option<AuthData> {
        let auth_bytes = match *auth {
            Some(ref x) => match x.from_base64() {
                Ok(y) => y,
                Err(e) => {
                    warn!("could not base64 decode auth: {:?}", e);
                    return None;
                }
            },
            None => { return None; }
        };

        let local_key = match self.public_key.from_base64() {
            Ok(x) => x,
            Err(e) => {
                panic!("could not base64 decode local public key: {:?}", e);
            }
        };

        // Context is used later for encrypt key and nonce derivation
        // https://tools.ietf.org/html/draft-ietf-httpbis-encryption-encoding-01#section-4.2
        //
        //  "context = label || 0x00 ||
        //             length(recipient_public) || recipient_public ||
        //             length(sender_public) || sender_public
        //
        // "The two length fields are encoded as a two octet unsigned integer in
        //  network byte order."
        //
        // The label is "P-256" defined here:
        // https://tools.ietf.org/html/draft-ietf-webpush-encryption-02#section-5
        //
        // "The label for this curve is the string "P-256" encoded in ASCII (that
        //  is, the octet sequence 0x50, 0x2d, 0x32, 0x35, 0x36)."
        let mut key_context : Vec<u8> = Vec::with_capacity(peer_key.len() + local_key.len() + 11);
        key_context.extend_from_slice(b"P-256\x00");
        if encrypt {
            Self::aesgcm128_append_key(&mut key_context, peer_key);
            Self::aesgcm128_append_key(&mut key_context, &local_key);
        } else {
            Self::aesgcm128_append_key(&mut key_context, &local_key);
            Self::aesgcm128_append_key(&mut key_context, peer_key);
        }
        key_context.push(1u8);

        Some(AuthData {
            auth: auth_bytes,
            key_context: key_context,
        })
    }

    fn aesgcm128_common(&self, salt: &[u8], shared_key: &[u8], auth: Option<AuthData>) -> ([u8; 32], [u8; 32], [u8; 12]) {
        let sha = Sha256::new();
        let mut encrypt_info : Vec<u8> = Vec::new();
        let mut nonce_info : Vec<u8> = Vec::new();

        // Create the HKDF salt from our shared key and transaction salt
        let mut salt_hmac = Hmac::new(Sha256::new(), salt);
        match auth {
            Some(ad) => {
                // We may have an additional shared secret
                // https://tools.ietf.org/html/draft-ietf-httpbis-encryption-encoding-01#section-4.3
                //
                //       auth_info = "Content-Encoding: auth" || 0x00
                //             IKM = HKDF(authentication, raw_key, auth_info, 32)
                let mut prk = [0u8; 32];
                hkdf_extract(sha, &ad.auth, shared_key, &mut prk);

                let auth_info = b"Content-Encoding: auth\x00";
                let mut ikm = [0u8; 32];
                hkdf_expand(sha, &prk, auth_info, &mut ikm);
                salt_hmac.input(&ikm);

                // https://tools.ietf.org/html/draft-ietf-httpbis-encryption-encoding-01#section-3.2
                //
                // To generate the encryption key:
                //
                // "cek_info = "Content-Encoding: aesgcm" || 0x00 || context"
                // "CEK = HMAC-SHA-256(PRK, cek_info || 0x01)"
                //
                // "Unless otherwise specified, the context is a zero length octet
                //  sequence.  Specifications that use this content encoding MAY specify
                //  the use of an expanded context to cover additional inputs in the key
                //  derivation."
                //
                // https://tools.ietf.org/html/draft-ietf-httpbis-encryption-encoding-01#section-3.3
                //
                // To generate the nonce:
                //
                // "nonce_info = "Content-Encoding: nonce" || 0x00 || context"
                // "NONCE = HMAC-SHA-256(PRK, nonce_info || 0x01) XOR SEQ"
                //
                encrypt_info.extend_from_slice(b"Content-Encoding: aesgcm\x00");
                encrypt_info.extend_from_slice(&ad.key_context);
                nonce_info.extend_from_slice(b"Content-Encoding: nonce\x00");
                nonce_info.extend_from_slice(&ad.key_context);
            }
            None => {
                // Legacy standard/implementation
                //
                // https://tools.ietf.org/html/draft-ietf-httpbis-encryption-encoding-00#section-4.3
                //
                // "Note that in the absence of an authentication secret, the input
                //  keying material is simply the raw keying material:
                //
                //      IKM = raw_key"
                salt_hmac.input(&shared_key[..]);

                // https://tools.ietf.org/html/draft-ietf-httpbis-encryption-encoding-00#section-3.2
                //
                // To generate the encryption key:
                //
                // "cek_info = "Content-Encoding: aesgcm128" || 0x00 || context"
                // "CEK = HMAC-SHA-256(PRK, cek_info || 0x01)"
                //
                // "Unless otherwise specified, the context is a zero length octet
                //  sequence.  Specifications that use this content encoding MAY specify
                //  the use of an expanded context to cover additional inputs in the key
                //  derivation."
                //
                // https://tools.ietf.org/html/draft-ietf-httpbis-encryption-encoding-00#section-3.3
                //
                // To generate the nonce:
                //
                // "nonce_info = "Content-Encoding: nonce" || 0x00 || context"
                // "NONCE = HMAC-SHA-256(PRK, nonce_info || 0x01) XOR SEQ"
                //
                // Note that while context may be empty, we are still missing the 0x00 byte.
                // This is required for interop with Firefox.
                encrypt_info.extend_from_slice(b"Content-Encoding: aesgcm128\x01");
                nonce_info.extend_from_slice(b"Content-Encoding: nonce\x01");
            }
        };

        let hkdf_salt = salt_hmac.result();

        // Create the AES-GCM encryption key
        // https://tools.ietf.org/html/draft-thomson-http-encryption-01#section-3.2
        let mut encrypt_key = [0u8; 32];
        hkdf_extract(sha, hkdf_salt.code(), &encrypt_info, &mut encrypt_key);

        // Create the AES-GCM nonce
        // https://tools.ietf.org/html/draft-thomson-http-encryption-01#section-3.3
        let mut nonce = [0u8; 32];
        hkdf_extract(sha, hkdf_salt.code(), &nonce_info, &mut nonce);

        // Sequence number is the same size as the nonce
        // https://tools.ietf.org/html/draft-thomson-http-encryption-01#section-3.3
        //
        // "The record sequence number (SEQ) is a 96-bit unsigned integer in network
        //  byte order that starts at zero."
        let seq = [0u8; 12];
        (encrypt_key, nonce, seq)
    }

    fn aesgcm128_record_nonce(&self, nonce: &[u8], seq: &mut[u8; 12]) -> [u8; 12] {
        // Generate the nonce for this record
        // https://tools.ietf.org/html/draft-thomson-http-encryption-01#section-3.3
        //
        // "NONCE = HMAC-SHA-256(PRK, "Content-Encoding: nonce" || 0x01) ^ SEQ"
        let mut record_nonce = [0u8; 12];
        let mut i = seq.len();
        while i > 0 {
            i -= 1;
            record_nonce[i] = nonce[i] ^ seq[i];
        }

        // Increment the sequence number in network-order
        i = seq.len();
        while i > 0 {
            i -= 1;
            if seq[i] == 255 {
                seq[i] = 0;
            } else {
                seq[i] += 1;
                break;
            }
        }

        record_nonce
    }

    #[cfg(test)]
    /// Decrypts the given payload using AES-GCM 128-bit with the shared key and salt.
    /// The shared key and salt are not used directly but rather are used to derive
    /// the encryption key and nonce as defined in the draft RFC.
    fn aesgcm128_decrypt(&self, mut input: Vec<u8>, shared_key: &[u8], salt: &[u8], auth: Option<AuthData>, record_size: usize) -> Option<String> {
        let has_auth = auth.is_some();
        let (decrypt_key, nonce, mut seq) = self.aesgcm128_common(salt, shared_key, auth);
        let mut chunks = Vec::new();
        let mut total_size = 0;

        while !input.is_empty() {
            let mut bound = min(record_size, input.len());
            if bound <= AESGCM_TAG_LEN {
                return None;
            }
            bound -= AESGCM_TAG_LEN;

            let chunk: Vec<_> = input.drain(0..bound).collect();
            let tag: Vec<_> = input.drain(0..AESGCM_TAG_LEN).collect();
            let record_nonce = self.aesgcm128_record_nonce(&nonce, &mut seq);
            let mut output = vec![0u8; chunk.len()];

            // Fail to decrypt if ends on a record boundary.
            // https://tools.ietf.org/html/draft-ietf-httpbis-encryption-encoding-01#section-2
            //
            // "A sequence of full-sized records can be truncated to produce a
            //  shorter sequence of records with valid authentication tags.  To
            //  prevent an attacker from truncating a stream, an encoder MUST append
            //  a record that contains only padding and is smaller than the full
            //  record size if the final record ends on a record boundary.  A
            //  receiver MUST treat the stream as failed due to truncation if the
            //  final record is the full record size."
            if input.is_empty() && bound == record_size {
                return None;
            }

            let mut cipher = AesGcm::new(KeySize::KeySize128, &decrypt_key[0..16], &record_nonce, &[0; 0]);
            if !cipher.decrypt(&chunk[..], &mut output[..], &tag[..]) {
                return None;
            }

            // Strip padding from the plaintext
            let padding_len = if has_auth {
                let padding: Vec<_> = output.drain(0..2).collect();
                ((padding[0] as usize) << 8) + padding[1] as usize
            } else {
                let padding: Vec<_> = output.drain(0..1).collect();
                padding[0] as usize
            };
            let _: Vec<_> = output.drain(0..padding_len).collect();
            total_size += output.len();
            chunks.push(output);
        }

        let mut out = Vec::with_capacity(total_size);
        for chunk in chunks {
            out.extend_from_slice(&chunk[..]);
        }

        match String::from_utf8(out) {
            Ok(s) => Some(s),
            Err(_) => None
        }
    }

    /// Encrypts the given payload using AES-GCM 128-bit with the shared key and salt.
    /// The shared key and salt are not used directly but rather are used to derive
    /// the encryption key and nonce as defined in the draft RFC.
    fn aesgcm128_encrypt(&self, input: String, shared_key: &[u8], salt: &[u8; 16], auth: Option<AuthData>, record_size: usize) -> Vec<u8> {
        assert!(!input.is_empty(), "input cannot be empty");
        assert!(record_size > 2, "record size must be greater than the padding");

        let has_auth = auth.is_some();
        let (encrypt_key, nonce, mut seq) = self.aesgcm128_common(salt, shared_key, auth);
        let mut raw_input = input.into_bytes();
        let mut chunks = Vec::new();
        let mut padding = false;
        let mut total_size = 0;

        while !raw_input.is_empty() || padding {
            // Add padding to input data in accordance with
            // https://tools.ietf.org/html/draft-ietf-httpbis-encryption-encoding-00#section-2
            //
            // "Padding consists of a length byte, followed that number of zero-valued octets.
            //  A receiver MUST fail to decrypt if any padding octet other than the first is
            //  non-zero"
            //
            // or
            //
            // https://tools.ietf.org/html/draft-ietf-httpbis-encryption-encoding-01#section-2
            //
            // "Padding consists of a two octet unsigned integer in network byte order, followed
            //  that number of zero-valued octets."
            raw_input.insert(0, 0);
            if has_auth { raw_input.insert(0, 0); }

            let bound = min(record_size, raw_input.len());
            let chunk: Vec<_> = raw_input.drain(0..bound).collect();
            let record_nonce = self.aesgcm128_record_nonce(&nonce, &mut seq);

            // If the final chunk ended on a record boundary, then we
            // need to append one more record with just padding.
            // https://tools.ietf.org/html/draft-thomson-http-encryption-01#section-2
            //
            // "an encoder MUST append a record that contains only padding and is smaller
            //  than the full record size if the final record ends on a record boundary."
            padding = bound == record_size && raw_input.is_empty();

            // With the generation AES-GCM key/nonce pair, encrypt the payload
            let mut cipher = AesGcm::new(KeySize::KeySize128, &encrypt_key[0..16], &record_nonce, &[0; 0]);
            let mut tag = [0u8; AESGCM_TAG_LEN];
            let mut out = vec![0u8; chunk.len() + tag.len()];
            out.truncate(chunk.len());
            cipher.encrypt(&chunk[..], &mut out, &mut tag);

            // Append the authentication tag to the record payload
            // https://tools.ietf.org/html/draft-thomson-http-encryption-01#section-2
            //
            // "Valid records always contain at least one byte of padding and a 16
            // octet authentication tag."
            out.extend_from_slice(&tag);
            total_size += out.len();
            chunks.push(out);
        }

        let mut out = Vec::with_capacity(total_size);
        for chunk in chunks {
            out.extend_from_slice(&chunk[..]);
        }
        out
    }

    /// Encrypt a payload using the given public key according to the `WebPush`
    /// RFC specifications.
    pub fn encrypt(&self, peer_key: &str, input: String, auth: &Option<String>, record_size: usize) -> Option<EncryptData> {
        // Derive public and secret keys from peer public key
        let peer_key_bytes = match peer_key.from_base64() {
            Ok(x) => x,
            Err(e) => {
                warn!("could not base64 decode peer key: {:?}", e);
                return None;
            }
        };

        let auth_data = self.aesgcm128_auth_data(auth, &peer_key_bytes, true);

        let shared_key = match self.ecdh_derive_keys(peer_key_bytes.to_hex()) {
            Some(key) => key,
            None => {
                warn!("could not derive keys");
                return None;
            }
        };

        // Create the salt for this transaction
        // https://tools.ietf.org/html/draft-thomson-http-encryption-01#section-3.1
        //
        // "The "salt" parameter MUST be present, and MUST be exactly 16 octets long
        //  when decoded.  The "salt" parameter MUST NOT be reused for two different
        //  payload bodies that have the same input keying material; generating a
        //  random salt for every application of the content encoding ensures that
        //  content encryption key reuse is highly unlikely."
        let mut gen = OsRng::new().unwrap();
        let mut salt = [0u8; 16];
        gen.fill_bytes(&mut salt);

        let salt_b64 = if auth_data.is_some() {
            salt.to_base64(URL_SAFE).replace("=", "")
        } else {
            salt.to_base64(URL_SAFE)
        };

        Some(EncryptData {
            salt: salt_b64,
            output: self.aesgcm128_encrypt(input, &shared_key, &salt, auth_data, record_size)
        })
    }

    #[cfg(test)]
    pub fn decrypt(&self, peer_key: &str, input: Vec<u8>, salt: &str, auth: &Option<String>, record_size: usize) -> Option<String> {
        // Derive public and secret keys from peer public key
        let peer_key_bytes = match peer_key.from_base64() {
            Ok(x) => x,
            Err(e) => {
                warn!("could not base64 decode peer key: {:?}", e);
                return None;
            }
        };

        let auth_data = self.aesgcm128_auth_data(auth, &peer_key_bytes, false);

        let shared_key = match self.ecdh_derive_keys(peer_key_bytes.to_hex()) {
            Some(key) => key,
            None => {
                warn!("could not derive keys");
                return None;
            }
        };

        let salt_bytes = match salt.from_base64() {
            Ok(x) => x,
            Err(e) => {
                warn!("could not base64 decode salt: {:?}", e);
                return None;
            }
        };

        self.aesgcm128_decrypt(input, &shared_key, &salt_bytes, auth_data, record_size)
    }
}

#[cfg(test)]
describe! aesgcm128 {
    it "should encrypt one record" {
        use super::CryptoContext;

        let crypto = CryptoContext::new().unwrap();
        let input = String::from("test");
        let shared_key = [14, 55, 71, 109, 215, 177, 33, 176, 142, 43, 241, 48, 179, 164, 96, 220, 146, 176, 76, 1, 63, 108, 78, 67, 141, 55, 125, 200, 40, 153, 252, 85];
        let salt = [23, 249, 70, 109, 205, 73, 187, 20, 140, 197, 163, 250, 114, 55, 122, 88];
        let output = crypto.aesgcm128_encrypt(input, &shared_key, &salt, None, 4096);
        let expected = vec![177, 172, 8, 114, 38, 164, 249, 255, 11, 140, 152, 0, 194, 82, 79, 121, 26, 116, 68, 34, 182];
        assert_eq!(output, expected);
    }

    it "should decrypt one record" {
        use super::CryptoContext;

        let crypto = CryptoContext::new().unwrap();
        let input = vec![177, 172, 8, 114, 38, 164, 249, 255, 11, 140, 152, 0, 194, 82, 79, 121, 26, 116, 68, 34, 182];
        let shared_key = [14, 55, 71, 109, 215, 177, 33, 176, 142, 43, 241, 48, 179, 164, 96, 220, 146, 176, 76, 1, 63, 108, 78, 67, 141, 55, 125, 200, 40, 153, 252, 85];
        let salt = [23, 249, 70, 109, 205, 73, 187, 20, 140, 197, 163, 250, 114, 55, 122, 88];
        let output = crypto.aesgcm128_decrypt(input, &shared_key, &salt, None, 4096);
        assert_eq!(output, Some(String::from("test")));
    }
}

#[cfg(test)]
describe! ecdh {
    it "should encrypt and decrypt payload" {
        use super::CryptoContext;

        let local = CryptoContext::new().unwrap();
        let peer = CryptoContext::new().unwrap();
        let input = String::from("testing ecdh");
        let auth = None;
        let rs = 4096;
        let encrypt_data = local.encrypt(&peer.public_key, input.clone(), &auth, rs).unwrap();
        let decrypt_data = peer.decrypt(&local.public_key, encrypt_data.output, &encrypt_data.salt, &auth, rs).unwrap();
        assert_eq!(input, decrypt_data);
    }

    it "should encrypt and decrypt payload using auth" {
        use super::CryptoContext;
        use rustc_serialize::base64::{ ToBase64, STANDARD };

        let local = CryptoContext::new().unwrap();
        let peer = CryptoContext::new().unwrap();
        let input = String::from("testing ecdh");
        let auth = Some([0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5].to_base64(STANDARD));
        let rs = 4096;
        let encrypt_data = local.encrypt(&peer.public_key, input.clone(), &auth, rs).unwrap();
        let decrypt_data = peer.decrypt(&local.public_key, encrypt_data.output, &encrypt_data.salt, &auth, rs).unwrap();
        assert_eq!(input, decrypt_data);
    }
}
