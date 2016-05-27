/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Adapter for `WebPush`.
//!
//! Implemented as described in the draft IETF RFC:
//! https://tools.ietf.org/html/draft-ietf-webpush-protocol-04
//!
//! Encryption and sending of push notifications is controlled by the
//! "webpush" build feature. Older versions of `OpenSSL` (< 1.0.0) are
//! missing the necessary APIs to support the implementation.
//!

mod crypto;
mod db;

use foxbox_taxonomy::api::{ Error, InternalError, User };
use foxbox_taxonomy::channel::*;
use foxbox_taxonomy::manager::*;
use foxbox_taxonomy::services::*;
use foxbox_taxonomy::values::{ Type, TypeError, Value, Json, WebPushNotify };

use hyper::header::{ ContentEncoding, Encoding, Authorization };
use hyper::Client;
use hyper::client::Body;
use rusqlite::{ self };
use self::crypto::CryptoContext;
use serde_json;
use std::cmp::max;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use foxbox_core::traits::Controller;

header! { (Encryption, "Encryption") => [String] }
header! { (EncryptionKey, "Encryption-Key") => [String] }
header! { (CryptoKey, "Crypto-Key") => [String] }
header! { (Ttl, "TTL") => [u32] }

static ADAPTER_NAME: &'static str = "WebPush adapter (built-in)";
static ADAPTER_VENDOR: &'static str = "team@link.mozilla.org";
static ADAPTER_VERSION: [u32;4] = [0, 0, 0, 0];
// This user identifier will be used when authentication is disabled.
static NO_AUTH_USER_ID: i32 = -1;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Subscription {
    pub push_uri: String,
    pub public_key: String,
    pub auth: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionGetter {
    subscriptions: Vec<Subscription>
}

impl SubscriptionGetter {
    fn new(subs: Vec<Subscription>) -> Self {
        SubscriptionGetter {
            subscriptions: subs
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceGetter {
    resources: Vec<String>
}

impl ResourceGetter {
    fn new(res: Vec<String>) -> Self {
        ResourceGetter {
            resources: res
        }
    }
}

impl Subscription {
    fn notify(&self, crypto: &CryptoContext, gcm_api_key: &str, message: &str) {
        // Make the record size at least the size of the encrypted message. We must
        // add 16 bytes for the encryption tag, 1 byte for padding and 1 byte to
        // ensure we don't end on a record boundary.
        //
        // https://tools.ietf.org/html/draft-ietf-webpush-encryption-02#section-3.2
        //
        // "An application server MUST encrypt a push message with a single record.
        //  This allows for a minimal receiver implementation that handles a single
        //  record. If the message is 4096 octets or longer, the "rs" parameter MUST
        //  be set to a value that is longer than the encrypted push message length."
        //
        // The push service is not obligated to accept larger records however.
        //
        // "Note that a push service is not required to support more than 4096 octets
        // of payload body, which equates to 4080 octets of cleartext, so the "rs"
        // parameter can be omitted for messages that fit within this limit."
        //
        let record_size = max(4096, message.len() + 18);
        let enc = match crypto.encrypt(&self.public_key, message.to_owned(), &self.auth, record_size) {
            Some(x) => x,
            None => {
                warn!("notity subscription {} failed for {}", self.push_uri, message);
                return;
            }
        };

        // If using Google's push service, we need to replace the given endpoint URI
        // with one known to work with WebPush, as support has not yet rolled out to
        // all of its servers.
        //
        // https://github.com/GoogleChrome/web-push-encryption/blob/dd8c58c62b1846c481ceb066c52da0d695c8415b/src/push.js#L69
        let push_uri = self.push_uri.replace("https://android.googleapis.com/gcm/send",
                                             "https://gcm-http.googleapis.com/gcm");

        let has_auth = self.auth.is_some();
        let public_key = crypto.get_public_key(has_auth);
        let client = Client::new();
        let mut req = client.post(&push_uri)
            .header(Encryption(format!("keyid=p256dh;salt={};rs={}", enc.salt, record_size)))
            .body(Body::BufBody(&enc.output, enc.output.len()));

        // If using Google's push service, we need to provide an Authorization header
        // which provides an API key permitting us to send push notifications. This
        // should be provided in foxbox.conf as webpush/gcm_api_key in base64.
        //
        // https://github.com/GoogleChrome/web-push-encryption/blob/dd8c58c62b1846c481ceb066c52da0d695c8415b/src/push.js#L84
        if push_uri != self.push_uri {
            if gcm_api_key.is_empty() {
                warn!("cannot notify subscription {}, GCM API key missing from foxbox.conf", push_uri);
                return;
            }
            req = req.header(Authorization(format!("key={}", gcm_api_key)));
        }

        req = if has_auth {
            req.header(ContentEncoding(vec![Encoding::EncodingExt(String::from("aesgcm"))]))
                .header(CryptoKey(format!("keyid=p256dh;dh={}", public_key)))

                // Set the TTL which controls how long the push service will wait before giving
                // up on delivery of the notification
                //
                // https://tools.ietf.org/html/draft-ietf-webpush-protocol-04#section-6.2
                //
                // "An application server MUST include the TTL (Time-To-Live) header
                //  field in its request for push message delivery.  The TTL header field
                //  contains a value in seconds that suggests how long a push message is
                //  retained by the push service.
                //
                //      TTL = 1*DIGIT
                //
                //  A push service MUST return a 400 (Bad Request) status code in
                //  response to requests that omit the TTL header field."
                //
                //  TODO: allow the notifier to control this; right now we default to 24 hours
                .header(Ttl(86400))
        } else {
            req.header(ContentEncoding(vec![Encoding::EncodingExt(String::from("aesgcm128"))]))
                .header(EncryptionKey(format!("keyid=p256dh;dh={}", public_key)))
        };

        // TODO: Add a retry mechanism if 429 Too Many Requests returned by push service
        let rsp = match req.send() {
            Ok(x) => x,
            Err(e) => { warn!("notify subscription {} failed: {:?}", push_uri, e); return; }
        };

        info!("notified subscription {} (status {:?})", push_uri, rsp.status);
    }
}

pub struct WebPush<C> {
    controller: C,
    crypto: CryptoContext,
    channel_resource_id: Id<Channel>,
    channel_subscribe_id: Id<Channel>,
    channel_unsubscribe_id: Id<Channel>,
    channel_notify_id: Id<Channel>,
}

impl<C: Controller> WebPush<C> {
    pub fn id() -> Id<AdapterId> {
        Id::new("webpush@link.mozilla.org")
    }

    pub fn service_webpush_id() -> Id<ServiceId> {
        Id::new("service:webpush@link.mozilla.org")
    }

    pub fn channel_resource_id() -> Id<Channel> {
        Id::new("channel:resource.webpush@link.mozilla.org")
    }

    pub fn channel_subscribe_id() -> Id<Channel> {
        Id::new("channel:subscription.webpush@link.mozilla.org")
    }

    pub fn channel_unsubscribe_id() -> Id<Channel> {
        Id::new("channel:unsubscribe.webpush@link.mozilla.org")
    }

    pub fn channel_notify_id() -> Id<Channel> {
        Id::new("channel:notify.webpush@link.mozilla.org")
    }
}

impl<C: Controller> Adapter for WebPush<C> {
    fn id(&self) -> Id<AdapterId> {
        Self::id()
    }

    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    fn vendor(&self) -> &str {
        ADAPTER_VENDOR
    }

    fn version(&self) -> &[u32;4] {
        &ADAPTER_VERSION
    }

    fn fetch_values(&self, mut set: Vec<Id<Channel>>, user: User) -> ResultMap<Id<Channel>, Option<Value>, Error> {
        set.drain(..).map(|id| {
            let user_id = if cfg!(feature = "authentication") {
                match user {
                    User::None => {
                        return (id,
                                Err(Error::InternalError(InternalError::GenericError("Cannot fetch from this channel without a user.".to_owned()))));
                    },
                    User::Id(id) => id
                }
            } else {
                NO_AUTH_USER_ID
            };

            macro_rules! getter_api {
                ($getter:ident, $getter_id:ident, $getter_type:ident) => (
                    if id == self.$getter_id {
                        match self.$getter(user_id) {
                            Ok(data) => {
                                let rsp = $getter_type::new(data);
                                return (id, Ok(Some(Value::Json(Arc::new(Json(serde_json::to_value(&rsp)))))));
                            },
                            Err(err) => return (id, Err(Error::InternalError(InternalError::GenericError(format!("Database error: {}", err)))))
                        };
                    }
                )
            }

            getter_api!(get_subscriptions, channel_subscribe_id, SubscriptionGetter);
            getter_api!(get_resources, channel_resource_id, ResourceGetter);
            (id.clone(), Err(Error::InternalError(InternalError::NoSuchChannel(id))))
        }).collect()
    }

    fn send_values(&self, mut values: HashMap<Id<Channel>, Value>, user: User) -> ResultMap<Id<Channel>, (), Error> {
        values.drain().map(|(id, value)| {
            let user_id = if cfg!(feature = "authentication") {
                match user {
                    User::None => {
                        return (id,
                            Err(Error::InternalError(InternalError::GenericError("Cannot send to this channel without a user.".to_owned()))));
                    },
                    User::Id(id) => id
                }
            } else {
                NO_AUTH_USER_ID
            };

            if id == self.channel_notify_id {
                match value {
                    Value::WebPushNotify(notification) => {
                        match self.set_notify(user_id, &notification) {
                            Ok(_) => return (id, Ok(())),
                            Err(err) => return (id, Err(Error::InternalError(InternalError::GenericError(format!("Database error: {}", err)))))
                        }
                    },
                   _ => return (id, Err(Error::TypeError(TypeError { expected: Type::WebPushNotify, got: value.get_type() })))
                }
            }

            let arc_json_value = match value {
                Value::Json(v) => v,
                _ => return (id, Err(Error::TypeError(TypeError { expected: Type::Json, got: value.get_type() })))
            };
            let Json(ref json_value) = *arc_json_value;

            macro_rules! setter_api {
                ($setter:ident, $setter_name: expr, $setter_id:ident, $setter_type:ident) => (
                    if id == self.$setter_id {
                        let data: Result<$setter_type, _> = serde_json::from_value(json_value.clone());
                        match data {
                            Ok(x) => {
                                self.$setter(user_id, &x).unwrap();
                                return (id, Ok(()));
                            }
                            Err(err) => return (id, Err(Error::InternalError(InternalError::GenericError(format!("While handling {}, cannot serialize value: {}, {:?}", $setter_name, err, json_value)))))
                        }
                    }
                )
            }

            setter_api!(set_resources, "set_resources", channel_resource_id, ResourceGetter);
            setter_api!(set_subscribe, "set_subscribe", channel_subscribe_id, SubscriptionGetter);
            setter_api!(set_unsubscribe, "set_unsubscribe", channel_unsubscribe_id, SubscriptionGetter);
            (id.clone(), Err(Error::InternalError(InternalError::NoSuchChannel(id))))
        }).collect()
    }
}

impl<C: Controller> WebPush<C> {
    pub fn init(controller: C, adapt: &Arc<AdapterManager>) -> Result<(), Error> {
        let wp = Arc::new(Self::new(controller));
        let id = WebPush::<C>::id();
        let service_id = WebPush::<C>::service_webpush_id();
        let channel_notify_id = WebPush::<C>::channel_notify_id();
        let channel_resource_id = WebPush::<C>::channel_resource_id();
        let channel_subscribe_id = WebPush::<C>::channel_subscribe_id();
        let channel_unsubscribe_id = WebPush::<C>::channel_unsubscribe_id();

        try!(adapt.add_adapter(wp));
        try!(adapt.add_service(Service::empty(&service_id, &id)));

        let template = Channel {
            service: service_id.clone(),
            adapter: id.clone(),
            ..Channel::default()
        };

        try!(adapt.add_channel(Channel {
            feature: Id::new("webpush/notify-msg"),
            supports_send: Some(Signature::accepts(Maybe::Required(Type::WebPushNotify))),
            id: channel_notify_id,
            ..template.clone()
        }));

        try!(adapt.add_channel(Channel {
            feature: Id::new("webpush/resource"),
            supports_fetch: Some(Signature::returns(Maybe::Required(Type::Json))), // FIXME: Turn this into a more specific type?
            supports_send: Some(Signature::accepts(Maybe::Required(Type::Json))), // FIXME: Turn this into a more specific type?
            id: channel_resource_id,
            ..template.clone()
        }));

        try!(adapt.add_channel(Channel {
            feature: Id::new("webpush/subscribe"),
            supports_fetch: Some(Signature::returns(Maybe::Required(Type::Json))), // FIXME: Turn this into a more specific type?
            supports_send: Some(Signature::accepts(Maybe::Required(Type::Json))), // FIXME: Turn this into a more specific type?
            id: channel_subscribe_id,
            ..template.clone()
        }));

        try!(adapt.add_channel(Channel {
            feature: Id::new("webpush/unsubscribe"),
            supports_send: Some(Signature::accepts(Maybe::Required(Type::Json))), // FIXME: Turn this into a more specific type?
            id: channel_unsubscribe_id,
            ..template.clone()
        }));
        Ok(())
    }

    fn new(controller: C) -> Self
    {
        WebPush {
            controller: controller,
            crypto: CryptoContext::new().unwrap(),
            channel_resource_id: Self::channel_resource_id(),
            channel_subscribe_id: Self::channel_subscribe_id(),
            channel_unsubscribe_id: Self::channel_unsubscribe_id(),
            channel_notify_id: Self::channel_notify_id(),
        }
    }

    fn get_db(&self) -> db::WebPushDb {
        db::WebPushDb::new(&self.controller.get_profile().path_for("webpush.sqlite"))
    }

    fn set_subscribe(&self, user_id: i32, setter: &SubscriptionGetter) -> rusqlite::Result<()> {
        let db = self.get_db();
        for sub in &setter.subscriptions {
            try!(db.subscribe(user_id, sub));
        }
        Ok(())
    }

    fn set_unsubscribe(&self, user_id: i32, setter: &SubscriptionGetter) -> rusqlite::Result<()> {
        let db = self.get_db();
        for sub in &setter.subscriptions {
            try!(db.unsubscribe(user_id, &sub.push_uri));
        }
        Ok(())
    }

    fn set_resources(&self, user_id: i32, setter: &ResourceGetter) -> rusqlite::Result<()> {
        try!(self.get_db().set_resources(user_id, &setter.resources));
        Ok(())
    }

    fn get_resources(&self, user_id: i32) -> rusqlite::Result<Vec<String>> {
        self.get_db().get_resources(user_id)
    }

    fn get_subscriptions(&self, user_id: i32) -> rusqlite::Result<Vec<Subscription>> {
        self.get_db().get_subscriptions(user_id)
    }

    fn get_resource_subscriptions(&self, resource: &str) -> rusqlite::Result<Vec<Subscription>> {
        self.get_db().get_resource_subscriptions(resource)
    }

    fn set_notify(&self, _: i32, setter: &WebPushNotify) -> rusqlite::Result<()> {
        info!("notify on resource {}: {}", setter.resource, setter.message);

        let subscriptions = try!(self.get_resource_subscriptions(&setter.resource));
        if subscriptions.is_empty() {
            debug!("no users listening on push resource");
        } else {
            let json = json!({resource: setter.resource, message: setter.message});
            let crypto = self.crypto.clone();
            let gcm_api_key = self.controller.get_config().get_or_set_default(
                "webpush", "gcm_api_key", "");

            thread::spawn(move || {
                for sub in subscriptions {
                    sub.notify(&crypto, &gcm_api_key, &json);
                }
            });
        }
        Ok(())
    }
}
