use std::collections::{HashMap, HashSet};
use std::time::Duration;

use tracing::error;

use crate::messages;
use crate::nodes::clock::{ClockAction, ClockProvider};
use crate::nodes::mailbox::Mailbox;
use crate::types;

pub enum LeaderMessageIn {
    Propose(messages::ProposeMessage),
    P1b(messages::P1bMessage),
    P2b(messages::P2bMessage),
    Preempted(messages::PreemptedMessage),
    Adopted(messages::AdoptedMessage),
}

pub enum LeaderScheduledAction {
    SendScout(types::BallotNumber),
    RetryProposal(u64), // slot number
    HeartbeatCheck,
}

pub enum LeaderEvent {
    Message(messages::SendableMessage),
    Timer(LeaderScheduledAction),
    Tick, // Regular check for timeouts
}

pub struct Leader {
    node_id: types::LeaderId,
    address: types::Address,
    config: types::Config,
    mailbox: Mailbox,
    active: bool,
    // Ballot number, proposals, promises, etc.
    ballot_number: types::BallotNumber,
    proposals: HashMap<u64, types::Command>,
    // We probably need only AcceptorID HashSets instead of the full message here
    p1b_responses: HashMap<types::BallotNumber, HashSet<types::AcceptorId>>,
    p2b_responses: HashMap<u64, HashSet<types::AcceptorId>>,
    // Clock provider for scheduling timeouts and retries
    clock: Box<dyn ClockProvider + Send>,
    // Current timeout duration for adaptive backoff
    current_timeout: Duration,
}

impl Leader {
    pub fn new(
        leader_id: types::LeaderId,
        config: types::Config,
        mailbox: Mailbox,
        clock: Box<dyn ClockProvider + Send>,
    ) -> anyhow::Result<Leader> {
        let addr = config
            .get_address(leader_id.as_ref())
            .ok_or(anyhow::anyhow!("Failed to get address"))?;
        let mut leader = Leader {
            node_id: leader_id.clone(),
            address: addr.clone(),
            current_timeout: config.timeout_config.min_timeout,
            config,
            mailbox,
            active: false,
            ballot_number: types::BallotNumber::new(leader_id),
            proposals: HashMap::new(),
            p1b_responses: HashMap::new(),
            p2b_responses: HashMap::new(),
            clock,
        };

        // Start with a scout (Phase 1)
        leader.send_p1a(leader.ballot_number.clone())?;
        // Schedule a retry in case initial scout fails
        leader.schedule_scout_retry()?;

        Ok(leader)
    }

    pub fn accept_message(&mut self, msg: messages::SendableMessage) -> () {
        self.mailbox.receive(msg);
    }

    pub fn work_on_message(&mut self) -> bool {
        let received_msg = match self.mailbox.process_latest_in() {
            None => return false,
            Some(msg_in) => msg_in,
        };

        let inbox_received = match received_msg.message {
            messages::Message::Propose(_msg) => LeaderMessageIn::Propose(_msg),
            messages::Message::P1b(_msg) => LeaderMessageIn::P1b(_msg),
            messages::Message::P2b(_msg) => LeaderMessageIn::P2b(_msg),
            messages::Message::Preempted(_msg) => LeaderMessageIn::Preempted(_msg),
            messages::Message::Adopted(_msg) => LeaderMessageIn::Adopted(_msg),
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

    pub fn handle_msg(&mut self, msg: LeaderMessageIn) -> anyhow::Result<()> {
        // quorum is from a majority of Acceptors
        let quorum = (self.config.acceptors.len() / 2) + 1;
        match msg {
            LeaderMessageIn::Propose(propose_msg) => {
                // Only accept proposal if slot is not already proposed
                if !self.proposals.contains_key(&propose_msg.slot_number) {
                    self.proposals
                        .insert(propose_msg.slot_number, propose_msg.command.clone());

                    // Only start Phase 2 if leader is active
                    if self.active {
                        self.send_p2a(
                            self.ballot_number.clone(),
                            propose_msg.slot_number,
                            propose_msg.command,
                        )?;
                    }
                }
            }
            LeaderMessageIn::P1b(p1b_msg) => {
                // Collect P1b responses for the ballot
                let ballot = p1b_msg.ballot_number.clone();
                // HashSet solves for: we may end up pushing the same message multiple times if the same acceptor responds again
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
                    // Reset timeout on successful Phase 1
                    self.reset_timeout();
                    // Cancel any pending scout retries since we succeeded
                    self.clock.cancel(&ClockAction::SendScout {
                        ballot: self.ballot_number.clone(),
                    });
                    let proposals: Vec<(u64, types::Command)> = self
                        .proposals
                        .iter()
                        .map(|(&slot, command)| (slot, command.clone()))
                        .collect();
                    for (slot, command) in proposals {
                        self.send_p2a(ballot.clone(), slot, command)?;
                    }
                    // Maybe clear responses to avoid duplicate sends
                    // self.p1b_responses.remove(&ballot);
                }
            }
            LeaderMessageIn::P2b(p2b_msg) => {
                // Collect P2b responses for the slot
                let slot = p2b_msg.slot_number;
                // HashSet solves for: we may end up pushing the same message multiple times if the same acceptor responds again
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
                    self.active = false;
                    self.ballot_number = types::BallotNumber {
                        round: preempted_msg.ballot_number.round + 1,
                        leader: self.node_id.clone(),
                    };
                    // Schedule a scout retry with backoff instead of immediate retry
                    self.schedule_scout_retry()?;
                }
            }
            LeaderMessageIn::Adopted(adopted_msg) => {
                // Only process if this is for our current ballot
                if self.ballot_number == adopted_msg.ballot_number {
                    // Reset timeout on successful adoption
                    self.reset_timeout();
                    // Cancel any pending scout retries
                    self.clock.cancel(&ClockAction::SendScout {
                        ballot: self.ballot_number.clone(),
                    });
                    // Merge accepted proposals into self.proposals
                    // For every slot number, add the proposal with the highest ballot number
                    let mut pmax: HashMap<u64, types::BallotNumber> = HashMap::new();

                    for pvalue in &adopted_msg.accepted {
                        let slot = pvalue.slot;
                        if !pmax.contains_key(&slot) || pmax[&slot] < pvalue.ballot_number {
                            pmax.insert(slot, pvalue.ballot_number.clone());
                            self.proposals.insert(slot, pvalue.command.clone());
                        }
                    }

                    // Start a commander (Phase 2) for every proposal
                    let proposals: Vec<(u64, types::Command)> = self
                        .proposals
                        .iter()
                        .map(|(&slot, command)| (slot, command.clone()))
                        .collect();

                    for (slot, command) in proposals {
                        self.send_p2a(self.ballot_number.clone(), slot, command)?;
                    }

                    // Set the leader as active
                    self.active = true;
                }
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

    /// Handle timer events from the clock system
    pub fn handle_timer(&mut self, action: ClockAction) -> anyhow::Result<()> {
        match action {
            ClockAction::SendScout { ballot } => {
                // Retry scout (Phase 1) with the specified ballot
                self.send_p1a(ballot)?;
                // Schedule another retry with exponential backoff
                self.schedule_scout_retry()?;
            }
            ClockAction::RetryProposal { slot } => {
                // Retry proposal for a specific slot if we still have it
                if let Some(command) = self.proposals.get(&slot).cloned() {
                    if self.active {
                        self.send_p2a(self.ballot_number.clone(), slot, command)?;
                    }
                }
                // Could schedule another retry here if needed
            }
            ClockAction::LeaderHeartbeat => {
                // Send periodic heartbeat (could be implemented as a low-priority operation)
                // For now, just reset timeout since we're alive
                self.reset_timeout();
            }
            _ => {
                // Ignore other action types not relevant to leaders
            }
        }
        Ok(())
    }

    /// Schedule a scout retry with exponential backoff
    fn schedule_scout_retry(&mut self) -> anyhow::Result<()> {
        let timeout = self
            .current_timeout
            .min(self.config.timeout_config.max_timeout);
        self.clock.schedule(
            ClockAction::SendScout {
                ballot: self.ballot_number.clone(),
            },
            timeout,
        );

        // Exponential backoff for next retry
        self.current_timeout = Duration::from_millis(
            (self.current_timeout.as_millis() as f32
                * self.config.timeout_config.timeout_multiplier) as u64,
        )
        .min(self.config.timeout_config.max_timeout);

        Ok(())
    }

    /// Reset timeout to minimum value (called on successful operations)
    fn reset_timeout(&mut self) {
        self.current_timeout = self.config.timeout_config.min_timeout;
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
                (lead.into(), Address::new("127.0.0.1".to_string(), 8081)),
                (accept1.into(), Address::new("127.0.0.1".to_string(), 8086)),
                (accept2.into(), Address::new("127.0.0.1".to_string(), 8087)),
                (accept3.into(), Address::new("127.0.0.1".to_string(), 8088)),
            ]),
            None,
        );
        let clock = Box::new(crate::nodes::clock::MockClock::new());
        Leader::new(lead, config, mailbox, clock).unwrap()
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

    #[test]
    fn leader_handles_adopted_message_correctly() {
        let mut leader = setup();

        // Initially, leader should not be active
        assert!(!leader.active);

        // Create some commands with different ballot numbers
        let command1 = Command {
            client_id: leader.node_id.as_ref().clone(),
            request_id: 1,
            op: CommandType::Op(vec![1, 2, 3]),
        };
        let command2 = Command {
            client_id: leader.node_id.as_ref().clone(),
            request_id: 2,
            op: CommandType::Op(vec![4, 5, 6]),
        };

        // Create an older ballot number for slot 1
        let older_ballot = BallotNumber {
            round: 1,
            leader: LeaderId::new(2), // Different leader
        };

        // Ensure the current ballot has a higher round
        leader.ballot_number.round = 2;

        // Create pvalues with different ballot numbers for the same slot
        let pvalue1_old = PValue {
            ballot_number: older_ballot,
            slot: 1,
            command: command1.clone(),
        };
        let pvalue1_new = PValue {
            ballot_number: leader.ballot_number.clone(),
            slot: 1,
            command: command2.clone(),
        };
        let pvalue2 = PValue {
            ballot_number: leader.ballot_number.clone(),
            slot: 2,
            command: command1.clone(),
        };

        // Clear outbox first (constructor sends P1a)
        leader.mailbox.clear_outbox();

        // Send adopted message with mixed pvalues
        let adopted_msg = messages::AdoptedMessage {
            src: leader.node_id.clone(),
            ballot_number: leader.ballot_number.clone(),
            accepted: vec![pvalue1_old, pvalue1_new, pvalue2],
        };

        leader
            .handle_msg(LeaderMessageIn::Adopted(adopted_msg))
            .unwrap();

        // Leader should now be active
        assert!(leader.active);

        // Leader should have adopted the command with the highest ballot for slot 1
        assert_eq!(leader.proposals.get(&1), Some(&command2));
        assert_eq!(leader.proposals.get(&2), Some(&command1));

        // Leader should have sent P2a messages for all proposals
        let p2a_messages: Vec<_> = leader
            .mailbox
            .outbox
            .iter()
            .filter(|msg| matches!(msg.message, Message::P2a(_)))
            .collect();

        assert_eq!(p2a_messages.len(), 2 * leader.config.acceptors.len()); // 2 proposals * 3 acceptors

        // Verify the P2a messages contain the correct proposals
        let p2a_slots: HashSet<u64> = leader
            .mailbox
            .outbox
            .iter()
            .filter_map(|msg| {
                if let Message::P2a(p2a_msg) = &msg.message {
                    Some(p2a_msg.slot_number)
                } else {
                    None
                }
            })
            .collect();

        assert!(p2a_slots.contains(&1));
        assert!(p2a_slots.contains(&2));
    }

    #[test]
    fn leader_ignores_adopted_message_for_wrong_ballot() {
        let mut leader = setup();

        // Initially, leader should not be active
        assert!(!leader.active);

        // Create a different ballot number
        let wrong_ballot = BallotNumber {
            round: leader.ballot_number.round + 1,
            leader: LeaderId::new(2),
        };

        let command = Command {
            client_id: leader.node_id.as_ref().clone(),
            request_id: 1,
            op: CommandType::Op(vec![1, 2, 3]),
        };

        let pvalue = PValue {
            ballot_number: wrong_ballot.clone(),
            slot: 1,
            command: command.clone(),
        };

        // Clear outbox first (constructor sends P1a)
        leader.mailbox.clear_outbox();

        // Send adopted message with wrong ballot
        let adopted_msg = messages::AdoptedMessage {
            src: leader.node_id.clone(),
            ballot_number: wrong_ballot,
            accepted: vec![pvalue],
        };

        leader
            .handle_msg(LeaderMessageIn::Adopted(adopted_msg))
            .unwrap();

        // Leader should still not be active
        assert!(!leader.active);

        // Leader should not have adopted any proposals
        assert!(leader.proposals.is_empty());

        // No P2a messages should have been sent
        let p2a_count = leader
            .mailbox
            .outbox
            .iter()
            .filter(|msg| matches!(msg.message, Message::P2a(_)))
            .count();

        assert_eq!(p2a_count, 0);
    }

    #[test]
    fn leader_schedules_scout_retry_on_preemption() {
        let mut leader = setup();

        // Get access to the mock clock for testing
        let initial_timeout = leader.current_timeout;

        // Clear outbox first (constructor sends P1a and schedules retry)
        leader.mailbox.clear_outbox();

        // Create a higher ballot number to preempt the leader
        let higher_ballot = BallotNumber {
            round: leader.ballot_number.round + 1,
            leader: LeaderId::new(2), // Different leader
        };

        let preempted_msg = messages::PreemptedMessage {
            src: LeaderId::new(2), // From the leader with higher ballot
            ballot_number: higher_ballot.clone(),
        };

        // Handle preemption - this should schedule a retry
        leader
            .handle_msg(LeaderMessageIn::Preempted(preempted_msg))
            .unwrap();

        // Leader should no longer be active
        assert!(!leader.active);

        // Leader should have updated its ballot number
        assert_eq!(leader.ballot_number.round, higher_ballot.round + 1);

        // Timeout should have increased due to backoff
        assert!(leader.current_timeout > initial_timeout);

        // No immediate P1a should be sent (it's scheduled instead)
        let p1a_count = leader
            .mailbox
            .outbox
            .iter()
            .filter(|msg| matches!(msg.message, Message::P1a(_)))
            .count();
        assert_eq!(
            p1a_count, 0,
            "No immediate P1a should be sent, only scheduled"
        );
    }

    #[test]
    fn leader_handles_timeout_and_retries_scout() {
        let mut leader = setup();
        let original_ballot = leader.ballot_number.clone();

        // Clear outbox first
        leader.mailbox.clear_outbox();

        // Simulate a timeout by manually calling the timer handler
        let timer_action = ClockAction::SendScout {
            ballot: original_ballot.clone(),
        };

        leader.handle_timer(timer_action).unwrap();

        // Should have sent a P1a retry
        let p1a_messages: Vec<_> = leader
            .mailbox
            .outbox
            .iter()
            .filter(|msg| matches!(msg.message, Message::P1a(_)))
            .collect();

        assert_eq!(
            p1a_messages.len(),
            leader.config.acceptors.len(),
            "Should send P1a to all acceptors"
        );
    }

    #[test]
    fn leader_cancels_scout_retry_on_successful_adoption() {
        let mut leader = setup();

        // Create an adopted message
        let command = Command {
            client_id: leader.node_id.as_ref().clone(),
            request_id: 1,
            op: CommandType::Op(vec![1, 2, 3]),
        };

        let pvalue = PValue {
            ballot_number: leader.ballot_number.clone(),
            slot: 1,
            command: command.clone(),
        };

        let adopted_msg = messages::AdoptedMessage {
            src: leader.node_id.clone(),
            ballot_number: leader.ballot_number.clone(),
            accepted: vec![pvalue],
        };

        // Handle adoption
        leader
            .handle_msg(LeaderMessageIn::Adopted(adopted_msg))
            .unwrap();

        // Leader should be active now
        assert!(leader.active);

        // Timeout should be reset to minimum
        assert_eq!(
            leader.current_timeout,
            leader.config.timeout_config.min_timeout
        );
    }
}
