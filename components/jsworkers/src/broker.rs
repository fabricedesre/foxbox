// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A message broker that let you register as a named target to receive and send messages.

use std::collections::HashMap;
use std::fmt::{ Display, Formatter, Result as FmtResult };
use std::result::Result;
use std::sync::{ Arc, Mutex };
use std::sync::mpsc::Sender;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Message {
    AddWorker,
    RemoveWorker,
    Shutdown,
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match *self {
            Message::AddWorker => write!(f, "{}", "AddWorker"),
            Message::RemoveWorker => write!(f, "{}", "RemoveWorker"),
            Message::Shutdown => write!(f, "{}", "Shutdown"),
        }
    }
}

#[derive(Debug)]
pub enum BrokerError {
    DuplicateTarget,
    NoSuchTarget,
    SendingError,
}

pub struct MessageBroker {
    actors: HashMap<String, Sender<Message>>
}

pub type SharedBroker = Arc<Mutex<MessageBroker>>;

impl MessageBroker {
    pub fn new() -> Self {
        MessageBroker {
            actors: HashMap::new(),
        }
    }

    pub fn new_shared() -> SharedBroker {
        Arc::new(Mutex::new(MessageBroker::new()))
    }

    pub fn add_actor(&mut self, target: &str, sender: Sender<Message>) -> Result<(), BrokerError> {
        if self.actors.contains_key(target) {
            return Err(BrokerError::DuplicateTarget);
        }

        self.actors.insert(target.to_string(), sender);
        Ok(())
    }

    pub fn remove_actor(&mut self, target: &str) -> Result<(), BrokerError> {
        if !self.actors.contains_key(target) {
            return Err(BrokerError::NoSuchTarget);
        }

        self.actors.remove(target);
        Ok(())
    }

    pub fn send_message(&mut self, target: &str, message: Message) -> Result<(), BrokerError> {
        if !self.actors.contains_key(target) {
            return Err(BrokerError::NoSuchTarget);
        }

        let res = self.actors.get(target).unwrap().send(message);
        if let Ok(_) = res {
            return Ok(());
        } else {
            return Err(BrokerError::SendingError);
        }
    }

    // TODO: figure out if we should return something else than void.
    pub fn broadcast_message(&mut self, message: Message) {
        info!("Broadcasting {}", message.clone());
        let ref actors = self.actors;
        for (target, actor) in actors {
            debug!("Sending {} to {}", message.clone(), target);
            actor.send(message);
        }
    }
}

#[test]
fn test_broker() {
    use std::sync::mpsc::channel;
    use std::thread;

    let mut broker = MessageBroker::new_shared();

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
            guard.send_message("actor1", Message::AddWorker);
        });
        let msg = rx1.recv();
        assert_eq!(msg.unwrap(), Message::AddWorker);
    }

    // Check that we can broadcast a message.
    {
        let b = broker.clone();
        thread::spawn(move || {
            let mut guard = b.lock().unwrap();
            guard.broadcast_message(Message::Shutdown);
        });
        let msg = rx1.recv();
        assert_eq!(msg.unwrap(), Message::Shutdown);
        let msg = rx2.recv();
        assert_eq!(msg.unwrap(), Message::Shutdown);
    }

    // Remove the actors.
    {
        let mut guard = broker.lock().unwrap();
        guard.remove_actor("actor1").unwrap();
        guard.remove_actor("actor2").unwrap();
        assert!(guard.remove_actor("actor1").is_err());
    }
}
