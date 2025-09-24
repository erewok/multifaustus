use std::collections::VecDeque;

use crate::messages;

/// Sans-IO mailbox for nodes to send and receive messages.
#[derive(Clone, Debug)]
pub struct Mailbox {
    pub inbox: VecDeque<messages::SendableMessage>,
    pub outbox: VecDeque<messages::SendableMessage>,
}

impl Default for Mailbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Mailbox {
    pub fn new() -> Self {
        Mailbox {
            inbox: VecDeque::new(),
            outbox: VecDeque::new(),
        }
    }

    pub fn receive(&mut self, msg: messages::SendableMessage) {
        self.inbox.push_back(msg);
    }

    pub fn process_latest_in(&mut self) -> Option<messages::SendableMessage> {
        self.inbox.pop_front()
    }

    pub fn send(&mut self, msg: messages::SendableMessage) {
        self.outbox.push_back(msg);
    }

    pub fn deliver_sent(&mut self) -> Option<messages::SendableMessage> {
        self.outbox.pop_front()
    }

    pub fn clear_inbox(&mut self) {
        self.inbox.clear();
    }

    pub fn clear_outbox(&mut self) {
        self.outbox.clear();
    }
}
