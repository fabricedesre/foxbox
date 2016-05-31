// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A message broker that let you register as a named target to receive and send messages.

use std::collections::HashMap;
use std::fmt::Debug;
use std::result::Result;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;

#[derive(Debug)]
pub enum BrokerError {
    DuplicateTarget,
    NoSuchTarget,
    SendingError,
}

pub struct MessageBroker<T> {
    actors: HashMap<String, Sender<T>>,
}

pub type SharedBroker<T> = Arc<Mutex<MessageBroker<T>>>;

impl<T> MessageBroker<T> {
    pub fn new() -> Self {
        debug!("MessageBroker::new()");
        MessageBroker { actors: HashMap::new() }
    }

    pub fn new_shared() -> SharedBroker<T>
        where T: Send
    {
        debug!("MessageBroker::new_shared()");
        Arc::new(Mutex::new(MessageBroker::new()))
    }

    pub fn add_actor(&mut self, target: &str, sender: Sender<T>) -> Result<(), BrokerError>
        where T: Send
    {
        if self.actors.contains_key(target) {
            error!("MessageBroker::add_actor: `{}` is not a known target",
                   target);
            return Err(BrokerError::DuplicateTarget);
        }

        self.actors.insert(target.to_string(), sender);
        Ok(())
    }

    pub fn remove_actor(&mut self, target: &str) -> Result<(), BrokerError> {
        if !self.actors.contains_key(target) {
            error!("MessageBroker::remove_actor: `{}` is not a known target",
                   target);
            return Err(BrokerError::NoSuchTarget);
        }

        self.actors.remove(target);
        Ok(())
    }

    pub fn send_message(&mut self, target: &str, message: T) -> Result<(), BrokerError>
        where T: Send + Clone + Debug
    {
        if !self.actors.contains_key(target) {
            error!("MessageBroker::send_message: `{}` is not a known target",
                   target);
            return Err(BrokerError::NoSuchTarget);
        }

        let res = self.actors.get(target).unwrap().send(message.clone());
        if let Ok(_) = res {
            return Ok(());
        } else {
            error!("MessageBroker::send_message: error sending `{:?}` to `{}`",
                   message,
                   target);
            return Err(BrokerError::SendingError);
        }
    }

    // TODO: figure out if we should return something else than void.
    pub fn broadcast_message(&mut self, message: T)
        where T: Send + Clone + Debug
    {
        info!("Broadcasting {:?}", message.clone());
        let ref actors = self.actors;
        for (target, actor) in actors {
            debug!("Sending {:?} to {}", message.clone(), target);
            actor.send(message.clone());
        }
    }
}

#[test]
fn test_broker() {
    use std::sync::mpsc::channel;
    use std::thread;

    #[derive(Clone, Debug, Serialize)]
    enum Message {
        Unknown,
        Shutdown,
    }

    let broker = MessageBroker::new_shared();

    // Create the receiver and sender for two channels.
    let (tx1, rx1) = channel::<Message>();
    let (tx2, rx2) = channel::<Message>();

    {
        let mut guard = broker.lock().unwrap();
        guard.add_actor("actor1", tx1.clone()).unwrap();
        assert!(guard.add_actor("actor1", tx1.clone()).is_err());

        guard.add_actor("actor2", tx2.clone()).unwrap();
    }

    // Check that we can send a message.
    {
        let b = broker.clone();
        thread::spawn(move || {
            let mut guard = b.lock().unwrap();
            guard.send_message("actor1", Message::Shutdown);
        });
        let msg = rx1.recv();
        match msg.unwrap() {
            Message::Shutdown => {}
            _ => {
                panic!("Didn't get a Shutdown message");
            }
        }
    }

    // Check that we can broadcast a message.
    {
        let b = broker.clone();
        thread::spawn(move || {
            let mut guard = b.lock().unwrap();
            guard.broadcast_message(Message::Shutdown);
        });
        let msg = rx1.recv();
        match msg.unwrap() {
            Message::Shutdown => {}
            _ => {
                panic!("Didn't get a Shutdown message");
            }
        }
        let msg = rx2.recv();
        match msg.unwrap() {
            Message::Shutdown => {}
            _ => {
                panic!("Didn't get a Shutdown message");
            }
        }
    }

    // Remove the actors.
    {
        let mut guard = broker.lock().unwrap();
        guard.remove_actor("actor1").unwrap();
        guard.remove_actor("actor2").unwrap();
        assert!(guard.remove_actor("actor1").is_err());
    }
}
