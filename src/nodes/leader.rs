use std::collections::{HashMap, HashSet};

use tracing::error;

use crate::messages;
use crate::nodes::mailbox::Mailbox;
use crate::types;

pub enum LeaderMessageIn {
    Propose(messages::ProposeMessage),
    P1b(messages::P1bMessage),
    P2b(messages::P2bMessage),
    Preempted(messages::PreemptedMessage),
    Adopted(messages::AdoptedMessage),
}

pub struct Leader {
    node_id: types::LeaderId,
    address: types::Address,
    config: types::Config,
    mailbox: Mailbox,
    // Ballot number, proposals, promises, etc.
    ballot_number: types::BallotNumber,
    proposals: HashMap<u64, types::Command>,
    // Instead of Vec, use HashMap to track responses per ballot/slot for each acceptorId
    // to avoid storing the same message multiple times from the same acceptor.
    // We probably need only AcceptorID HashSets instead of the full message here
    p1b_responses: HashMap<types::BallotNumber, HashSet<types::AcceptorId>>,
    p2b_responses: HashMap<u64, HashSet<types::AcceptorId>>,
}

impl Leader {
    pub fn new(
        leader_id: types::LeaderId,
        config: types::Config,
        mailbox: Mailbox,
    ) -> anyhow::Result<Leader> {
        let addr = config
            .get_address(leader_id.as_ref())
            .ok_or(anyhow::anyhow!("Failed to get address"))?;
        Ok(Leader {
            node_id: leader_id.clone(),
            address: addr.clone(),
            config,
            mailbox,
            ballot_number: types::BallotNumber::new(leader_id),
            proposals: HashMap::new(),
            p1b_responses: HashMap::new(),
            p2b_responses: HashMap::new(),
        })
    }

    pub fn accept_message(&mut self, msg: messages::SendableMessage) -> () {
        self.mailbox.receive(msg);
    }

    pub fn work_on_message(&mut self) -> bool {
        let received_msg = match self.mailbox.process_latest_in() {
            None => return false,
            Some(msg_in) => msg_in
        };

        let inbox_received = match received_msg.message {
            messages::Message::Propose(_msg) => LeaderMessageIn::Propose(_msg),
            messages::Message::P1b(_msg) => LeaderMessageIn::P1b(_msg),
            messages::Message::P2b(_msg) => LeaderMessageIn::P2b(_msg),
            messages::Message::Preempted(_msg) => LeaderMessageIn::Preempted(_msg),
            messages::Message::Adopted(_msg) => LeaderMessageIn::Adopted(_msg),
            msg => {
                error!("{}: Leader received unexpected message in mailbox: {:?}", self.node_id, msg);
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

    pub fn handle_msg(&mut self, msg: LeaderMessageIn) -> anyhow::Result<()> {
        // quorum is from a majority of Acceptors
        let quorum = (self.config.acceptors.len() / 2) + 1;
        match msg {
            LeaderMessageIn::Propose(propose_msg) => {
                // Accept proposal for slot
                self.proposals
                    .insert(propose_msg.slot_number, propose_msg.command.clone());
                // Start Phase 1 (Prepare) for this slot if needed
                // For simplicity, always start Phase 1 for new proposals
                self.send_p1a(self.ballot_number.clone())?;
            }
            LeaderMessageIn::P1b(p1b_msg) => {
                // Collect P1b responses for the ballot
                let ballot = p1b_msg.ballot_number.clone();
                // Bug: we may end up pushing the same message multiple times if the same acceptor responds again
                self.p1b_responses
                    .entry(ballot.clone())
                    .and_modify(|r| {
                        r.insert(p1b_msg.src);
                    })
                    .or_insert_with(|| {
                        let mut m = HashSet::new();
                        m.insert(p1b_msg.src);
                        m
                    });
                // If quorum reached, start Phase 2 for all proposals
                if self
                    .p1b_responses
                    .get(&ballot)
                    .map(|v| v.len())
                    .unwrap_or_default()
                    >= quorum
                {
                    let proposals: Vec<(u64, types::Command)> = self
                        .proposals
                        .iter()
                        .map(|(&slot, command)| (slot, command.clone()))
                        .collect();
                    for (slot, command) in proposals {
                        self.send_p2a(ballot.clone(), slot, command)?;
                    }
                    // Optionally clear responses to avoid duplicate sends
                    // self.p1b_responses.remove(&ballot);
                }
            }
            LeaderMessageIn::P2b(p2b_msg) => {
                // Collect P2b responses for the slot
                let slot = p2b_msg.slot_number;
                // Bug: we may end up pushing the same message multiple times if the same acceptor responds again
                self.p2b_responses
                    .entry(slot)
                    .and_modify(|r| {
                        r.insert(p2b_msg.src);
                    })
                    .or_insert_with(|| {
                        let mut m = HashSet::new();
                        m.insert(p2b_msg.src);
                        m
                    });
                // If quorum reached, send Decision to replicas for this slot
                if self
                    .p2b_responses
                    .get(&slot)
                    .map(|v| v.len())
                    .unwrap_or_default()
                    >= quorum
                {
                    if let Some(command) = self.proposals.get(&slot) {
                        self.send_decision(slot, command.clone())?;
                    }
                }
            }
            LeaderMessageIn::Preempted(preempted_msg) => {
                // Update ballot if preempted by higher ballot
                if preempted_msg.ballot_number > self.ballot_number {
                    self.ballot_number = preempted_msg.ballot_number.clone();
                    // Need to update self.proposals...?
                    // Restart Phase 1 for all pending proposals
                    self.send_p1a(self.ballot_number.clone())?;
                }
            }
            LeaderMessageIn::Adopted(adopted_msg) => {
                // Adopt ballot and previously accepted proposals
                self.ballot_number = adopted_msg.ballot_number.clone();
                // Merge accepted proposals into self.proposals
            }
        }
        Ok(())
    }

    /// Send a P1a (prepare) message to all acceptors for the given ballot.
    pub fn send_p1a(&mut self, ballot: types::BallotNumber) -> anyhow::Result<()> {
        for acc in &self.config.acceptors {
            let msg = messages::P1aMessage {
                src: self.node_id.clone(),
                ballot_number: ballot.clone(),
            };
            let acc_address = self
                .config
                .get_address(acc.as_ref())
                .ok_or(anyhow::anyhow!("Acceptor address not found"))?;
            let sendable = messages::SendableMessage {
                src: self.address.clone(),
                dst: acc_address.clone(),
                message: messages::Message::P1a(msg),
            };
            self.mailbox.send(sendable);
        }
        Ok(())
    }

    /// Send a P2a (accept) message to all acceptors for the given ballot, slot, and command.
    pub fn send_p2a(
        &mut self,
        ballot: types::BallotNumber,
        slot: u64,
        command: types::Command,
    ) -> anyhow::Result<()> {
        for acc in &self.config.acceptors {
            let msg = messages::P2aMessage {
                src: self.node_id.clone(),
                ballot_number: ballot.clone(),
                slot_number: slot,
                command: command.clone(),
            };
            let acc_address = self
                .config
                .get_address(acc.as_ref())
                .ok_or(anyhow::anyhow!("Acceptor address not found"))?;
            let sendable = messages::SendableMessage {
                src: self.address.clone(),
                dst: acc_address.clone(),
                message: messages::Message::P2a(msg),
            };
            self.mailbox.send(sendable);
        }
        Ok(())
    }

    /// Send a Decision message to all replicas for the given slot and command.
    pub fn send_decision(&mut self, slot: u64, command: types::Command) -> anyhow::Result<()> {
        for rep in &self.config.replicas {
            let msg = messages::DecisionMessage {
                src: self.node_id.clone(),
                slot_number: slot,
                command: command.clone(),
            };
            let rep_address = self
                .config
                .get_address(rep.as_ref())
                .ok_or(anyhow::anyhow!("Replica address not found"))?;
            let sendable = messages::SendableMessage {
                src: self.address.clone(),
                dst: rep_address.clone(),
                message: messages::Message::Decision(msg),
            };
            self.mailbox.send(sendable);
        }
        Ok(())
    }
    /// Helper to drain the outbox
    pub fn drain_outbox(&mut self) {
        self.mailbox.clear_outbox();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::*;
    use crate::nodes::mailbox::Mailbox;
    use crate::types::*;
    use std::collections::{BTreeMap, HashSet};

    fn setup() -> Leader {
        let mailbox = Mailbox::new();
        let rep = ReplicaId::new(1);
        let accept1 = AcceptorId::new(1);
        let accept2 = AcceptorId::new(2);
        let accept3 = AcceptorId::new(3);
        let lead = LeaderId::new(1);

        let config = Config::new(
            HashSet::from([rep]),
            HashSet::from([accept1, accept2, accept3]),
            HashSet::from([lead]),
            BTreeMap::from([
                (rep.into(), Address::new("127.0.0.1".to_string(), 8080)),
                (lead.into(), Address::new("127.0.0.1".to_string(), 8082)),
                (accept1.into(), Address::new("127.0.0.1".to_string(), 8084)),
                (accept2.into(), Address::new("127.0.0.1".to_string(), 8086)),
                (accept3.into(), Address::new("127.0.0.1".to_string(), 8088)),
            ]),
        );
        Leader::new(lead, config, mailbox).unwrap()
    }

    #[test]
    fn leader_sees_quorum_for_accepted_proposal() {
        let mut leader = setup();

        // Create an accepted P1a message response
        let command = Command {
            client_id: leader.node_id.as_ref().clone(),
            request_id: 1,
            op: CommandType::Op(vec![1, 2, 3]),
        };
        // insert command into leader's proposals at slot 1
        leader.proposals.insert(1, command.clone());
        let accepted_msg = messages::P1bMessage {
            src: AcceptorId::new(1),
            ballot_number: leader.ballot_number.clone(),
            accepted: vec![PValue {
                ballot_number: leader.ballot_number.clone(),
                slot: 1,
                command: command.clone(),
            }],
        };
        leader
            .handle_msg(LeaderMessageIn::P1b(accepted_msg))
            .unwrap();
        // No quorum yet
        assert!(leader
            .mailbox
            .outbox
            .iter()
            .filter(|msg| matches!(msg.message, Message::P2a(_)))
            .collect::<Vec<&messages::SendableMessage>>()
            .is_empty());
        // After sending Propose
        // Simulate quorum P1b responses
        // ...call handle_msg with enough P1bMessage to reach quorum...
        // for f=1, need 2f+1=3 acceptors for f failures, which means quorum is a *majority* of 2.

        let p1b_msg_extra = messages::P1bMessage {
            src: AcceptorId::new(2),
            ballot_number: leader.ballot_number.clone(),
            accepted: vec![PValue {
                ballot_number: leader.ballot_number.clone(),
                slot: 1,
                command: command,
            }],
        };
        leader
            .handle_msg(LeaderMessageIn::P1b(p1b_msg_extra))
            .unwrap();

        // Assert outgoing P2a and Decision messages
        assert!(
            leader
                .mailbox
                .outbox
                .iter()
                .any(|msg| matches!(msg.message, Message::P2a(_))),
            "*** Leader.outbox length is: {} ***",
            leader.mailbox.outbox.len()
        );
    }

    #[test]
    fn leader_reaches_quorum_and_sends_decision_for_adopted_proposal() {
        let mut leader = setup();

        // Create a command that was adopted
        let command = Command {
            client_id: leader.node_id.as_ref().clone(),
            request_id: 1,
            op: CommandType::Op(vec![1, 2, 3]),
        };
        // insert command into leader's proposals at slot 1
        leader.proposals.insert(1, command);

        // Create an accepted P2a message response
        let p2b_msg = messages::P2bMessage {
            src: AcceptorId::new(1),
            slot_number: 1,
            ballot_number: leader.ballot_number.clone(),
        };
        leader.handle_msg(LeaderMessageIn::P2b(p2b_msg)).unwrap();
        // No quorum yet
        assert!(leader
            .mailbox
            .outbox
            .iter()
            .filter(|msg| matches!(msg.message, Message::Decision(_)))
            .collect::<Vec<&messages::SendableMessage>>()
            .is_empty());
        // Simulate quorum P1b responses
        // ...call handle_msg with enough P1bMessage to reach quorum...
        // for f=1, need 2f+1=3 acceptors for f failures, which means quorum is a *majority* of 2.
        let p2b_msg_extra = messages::P2bMessage {
            src: AcceptorId::new(2),
            slot_number: 1,
            ballot_number: leader.ballot_number.clone(),
        };
        leader
            .handle_msg(LeaderMessageIn::P2b(p2b_msg_extra))
            .unwrap();
        assert!(
            leader
                .mailbox
                .outbox
                .iter()
                .any(|msg| matches!(msg.message, Message::Decision(_))),
            "*** Leader.outbox length is: {} ***",
            leader.mailbox.outbox.len()
        );
    }

    // Add more tests for preemption, ballot adoption, etc.
}
