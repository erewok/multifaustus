use std::collections::HashMap;

use tracing::error;

use crate::messages;
use crate::nodes::clock::{ClockAction, ClockProvider};
use crate::nodes::mailbox::Mailbox;
use crate::types;

pub enum AcceptorMessageIn {
    P1a(messages::P1aMessage),
    P2a(messages::P2aMessage),
}

pub struct Acceptor {
    node_id: types::AcceptorId,
    address: types::Address,
    config: types::Config,
    mailbox: Mailbox,
    // State per slot: promised ballot, accepted ballot, accepted command
    promised: HashMap<u64, types::BallotNumber>,
    accepted: HashMap<u64, (types::BallotNumber, types::Command)>,
    // Clock provider for periodic cleanup and heartbeat
    clock: Box<dyn ClockProvider + Send>,
}

impl Acceptor {
    pub fn new(
        acceptor_id: types::AcceptorId,
        config: types::Config,
        mailbox: Mailbox,
        clock: Box<dyn ClockProvider + Send>,
    ) -> anyhow::Result<Acceptor> {
        let addr = config
            .get_address(acceptor_id.as_ref())
            .ok_or(anyhow::anyhow!("Failed to get address"))?;
        Ok(Acceptor {
            node_id: acceptor_id,
            address: addr.clone(),
            config,
            mailbox,
            promised: HashMap::new(),
            accepted: HashMap::new(),
            clock,
        })
    }

    pub fn accept_message(&mut self, msg: messages::SendableMessage) {
        self.mailbox.receive(msg);
    }

    pub fn work_on_message(&mut self) -> bool {
        let received_msg = match self.mailbox.process_latest_in() {
            None => return false,
            Some(msg_in) => msg_in,
        };

        let inbox_received = match received_msg.message {
            messages::Message::P1a(_msg) => AcceptorMessageIn::P1a(_msg),
            messages::Message::P2a(_msg) => AcceptorMessageIn::P2a(_msg),
            msg => {
                error!(
                    "{}: Leader received unexpected message in mailbox: {:?}",
                    self.node_id, msg
                );
                return false; // Ignore other messages
            }
        };
        if let Err(e) = self.handle_msg(inbox_received) {
            error!("{}: Error handling message: {}", self.node_id, e);
            false
        } else {
            true
        }
    }

    pub fn handle_msg(&mut self, msg: AcceptorMessageIn) -> anyhow::Result<()> {
        match msg {
            AcceptorMessageIn::P1a(p1a_msg) => {
                // For all slots, update promised if ballot >= promised
                // For simplicity, treat promised as a global ballot (can be per-slot for full generality)
                let ballot_number = p1a_msg.ballot_number.clone();
                let mut accepted = Vec::new();
                // Collect all accepted proposals for this ballot
                for (&slot, (accepted_ballot, command)) in &self.accepted {
                    if accepted_ballot == &ballot_number {
                        accepted.push(types::PValue {
                            ballot_number: accepted_ballot.clone(),
                            slot,
                            command: command.clone(),
                        });
                    }
                }
                // Update promised if ballot >= promised
                let promised_ballot = self
                    .promised
                    .get(&0)
                    .cloned()
                    .unwrap_or_else(|| types::BallotNumber::new(p1a_msg.src));
                if ballot_number >= promised_ballot {
                    self.promised.insert(0, ballot_number.clone()); // Update global promised
                    self.send_p1b(p1a_msg.src, ballot_number, accepted)?;
                }
            }
            AcceptorMessageIn::P2a(p2a_msg) => {
                let ballot = p2a_msg.ballot_number.clone();
                let slot = p2a_msg.slot_number;
                let promised_ballot = self
                    .promised
                    .get(&slot)
                    .cloned()
                    .unwrap_or_else(|| types::BallotNumber::new(p2a_msg.src));
                if ballot >= promised_ballot {
                    // Accept the proposal
                    self.promised.insert(slot, ballot.clone());
                    self.accepted
                        .insert(slot, (ballot.clone(), p2a_msg.command.clone()));
                    self.send_p2b(p2a_msg.src, ballot, slot)?;
                }
            }
        }
        Ok(())
    }

    /// Send a P1b (promise) message to the leader.
    pub fn send_p1b(
        &mut self,
        leader: types::LeaderId,
        ballot: types::BallotNumber,
        accepted: Vec<types::PValue>,
    ) -> anyhow::Result<()> {
        let msg = messages::P1bMessage {
            src: self.node_id,
            ballot_number: ballot,
            accepted,
        };
        let ldr_address = self
            .config
            .get_address(leader.as_ref())
            .ok_or(anyhow::anyhow!("Leader address not found"))?;
        let sendable = messages::SendableMessage {
            src: self.address.clone(),
            dst: ldr_address.clone(),
            message: messages::Message::P1b(msg),
        };
        self.mailbox.send(sendable);
        Ok(())
    }

    /// Send a P2b (accepted) message to the leader.
    pub fn send_p2b(
        &mut self,
        leader: types::LeaderId,
        ballot: types::BallotNumber,
        slot: u64,
    ) -> anyhow::Result<()> {
        let msg = messages::P2bMessage {
            src: self.node_id,
            ballot_number: ballot,
            slot_number: slot,
        };
        let ldr_address = self
            .config
            .get_address(leader.as_ref())
            .ok_or(anyhow::anyhow!("Leader address not found"))?;
        let sendable = messages::SendableMessage {
            src: self.address.clone(),
            dst: ldr_address.clone(),
            message: messages::Message::P2b(msg),
        };
        self.mailbox.send(sendable);
        Ok(())
    }

    /// Handle timer events from the clock system
    pub fn handle_timer(&mut self, action: ClockAction) -> anyhow::Result<()> {
        match action {
            ClockAction::AcceptorHeartbeat => {
                // Perform periodic maintenance tasks
                self.cleanup_old_state()?;
            }
            _ => {
                // Ignore action types not relevant to acceptors
            }
        }
        Ok(())
    }

    /// Clean up old promises and acceptances for completed slots
    fn cleanup_old_state(&mut self) -> anyhow::Result<()> {
        // In a full implementation, this could:
        // 1. Remove promises/acceptances for very old slots
        // 2. Compact state for slots that are likely committed
        // 3. Send heartbeat signals to other nodes

        // For now, just schedule the next heartbeat
        self.schedule_heartbeat()?;
        Ok(())
    }

    /// Schedule periodic heartbeat
    fn schedule_heartbeat(&mut self) -> anyhow::Result<()> {
        let timeout = self.config.timeout_config.max_timeout;
        self.clock.schedule(ClockAction::AcceptorHeartbeat, timeout);
        Ok(())
    }

    /// Initialize periodic checks (should be called after construction)
    pub fn start_periodic_checks(&mut self) -> anyhow::Result<()> {
        self.schedule_heartbeat()?;
        Ok(())
    }

    /// Check for expired timers and handle them
    pub fn check_timers(&mut self) -> anyhow::Result<Vec<ClockAction>> {
        let expired = self.clock.check_timers();
        for action in &expired {
            self.handle_timer(action.clone())?;
        }
        Ok(expired)
    }

    /// Helper to drain the outbox
    pub fn drain_outbox(&mut self) {
        self.mailbox.clear_outbox();
    }

    // Add methods for sending Promise and Accepted messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::*;
    use crate::nodes::mailbox::Mailbox;
    use crate::types::*;
    use std::collections::{BTreeMap, HashSet};

    fn setup() -> Acceptor {
        let mailbox = Mailbox::new();
        let rep = ReplicaId::new(1);
        let accept = AcceptorId::new(1);
        let lead = LeaderId::new(1);

        let config = Config::new(
            HashSet::from([rep]),
            HashSet::from([accept]),
            HashSet::from([lead]),
            BTreeMap::from([
                (rep.into(), Address::new("127.0.0.1".to_string(), 8080)),
                (accept.into(), Address::new("127.0.0.1".to_string(), 8081)),
                (lead.into(), Address::new("127.0.0.1".to_string(), 8082)),
            ]),
            None,
        );
        let clock = Box::new(crate::nodes::clock::MockClock::new());
        let acceptor = Acceptor::new(accept, config, mailbox, clock).unwrap();
        acceptor
    }

    #[test]
    fn acceptor_promises_and_accepts() {
        let mut acceptor = setup();

        // Inject P1a
        let ballot = BallotNumber::new(LeaderId::new(1));
        let p1a_msg = P1aMessage {
            src: LeaderId::new(1),
            ballot_number: ballot.clone(),
        };
        acceptor
            .handle_msg(AcceptorMessageIn::P1a(p1a_msg))
            .unwrap();

        // Assert outgoing P1b message
        assert!(acceptor
            .mailbox
            .outbox
            .iter()
            .any(|msg| matches!(msg.message, Message::P1b(_))));
    }

    // Add more tests for P2a handling, ballot rejection, etc.

    #[test]
    fn acceptor_handles_heartbeat_timer() {
        let mut acceptor = setup();

        // Handle heartbeat timer
        acceptor
            .handle_timer(ClockAction::AcceptorHeartbeat)
            .unwrap();

        // Should not panic or produce errors
        // In a full implementation, this might send heartbeat messages
        // or perform state cleanup
    }
}
