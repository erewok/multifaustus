use std::collections::HashMap;
use std::time::Duration;

use tracing::{debug, error, info};

use crate::constants::WINDOW;
use crate::messages;
use crate::nodes::clock::{ClockAction, ClockProvider};
use crate::nodes::mailbox::Mailbox;
use crate::types;

pub enum ReplicaMessageIn {
    Request(messages::RequestMessage),
    Decision(messages::DecisionMessage),
}

pub struct Replica {
    node_id: types::ReplicaId,
    address: types::Address,
    slot_in: u64,
    slot_out: u64,
    proposals: HashMap<u64, types::Command>,
    decisions: HashMap<u64, types::Command>,
    requests: Vec<types::Command>,
    config: types::Config,
    mailbox: Mailbox,
    // Clock provider for scheduling timeouts and retries
    clock: Box<dyn ClockProvider + Send>,
    // Track when proposals were sent for timeout management
    proposal_times: HashMap<u64, Duration>, // slot -> timeout duration
}

impl Replica {
    pub fn new(
        replica_id: types::ReplicaId,
        config: types::Config,
        mailbox: Mailbox,
        clock: Box<dyn ClockProvider + Send>,
    ) -> anyhow::Result<Replica> {
        let addr = config
            .get_address(replica_id.as_ref())
            .ok_or(anyhow::anyhow!("Failed to get address"))?;

        Ok(Replica {
            node_id: replica_id,
            address: addr.clone(),
            slot_in: 1,
            slot_out: 1,
            proposals: HashMap::new(),
            decisions: HashMap::new(),
            requests: Vec::new(),
            config,
            mailbox,
            clock,
            proposal_times: HashMap::new(),
        })
    }

    /// Initialize periodic timeout checks (should be called after construction)
    pub fn start_periodic_checks(&mut self) -> anyhow::Result<()> {
        // Start the slot progress monitoring
        self.schedule_slot_check()?;
        Ok(())
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
            messages::Message::Request(_msg) => ReplicaMessageIn::Request(_msg),
            messages::Message::Decision(_msg) => ReplicaMessageIn::Decision(_msg),
            _ => {
                error!(
                    "{}: Replica received unexpected message in mailbox: {:?}",
                    self.node_id, received_msg.message
                );
                return false;
            }
        };
        if let Err(e) = self.handle_msg(inbox_received) {
            error!("{}: Error handling message: {}", self.node_id, e);
            false
        } else {
            true
        }
    }
    // A replica runs in an infinite loop, receiving
    // messages. Replicas receive two kinds of messages:

    // - Requests: When it receives a request from a client, the
    // replica adds the request to set requests. Next, the replica
    // invokes the function propose().
    // - Decisions: Decisions may arrive out-of-order and multiple
    // times. For each decision message, the replica adds the
    // decision to the set decisions. Then, in a loop, it considers
    // which decisions are ready for execution before trying to
    // receive more messages. If there is a decision corresponding to
    // the current slot out, the replica first checks to see if it
    // has proposed a different command for that slot. If so, the
    // replica removes that command from the set proposals and
    // returns it to set requests so it can be proposed again at a
    // later time. Next, the replica invokes perform().
    pub fn handle_msg(&mut self, msg: ReplicaMessageIn) -> anyhow::Result<()> {
        match msg {
            ReplicaMessageIn::Request(req) => {
                debug!("{}: received RequestMessage: {:?}", req.src, req.command);
                self.requests.push(req.command.clone());
            }
            ReplicaMessageIn::Decision(dec) => {
                debug!("{}: received DecisionMessage: {:?}", dec.src, dec.command);
                self.decisions.insert(dec.slot_number, dec.command.clone());

                // Clean up timeout tracking for this slot since we got a decision
                self.proposal_times.remove(&dec.slot_number);

                while self.decisions.contains_key(&self.slot_out) {
                    if let Some(_proposal) = self.proposals.get(&self.slot_out) {
                        // In any case, we will delete the proposal from self.proposals
                        if Some(_proposal) != self.decisions.get(&self.slot_out) {
                            self.proposals.remove(&self.slot_out).map(|proposal| {
                                self.requests.push(proposal);
                            });
                        } else {
                            let _ = self.proposals.remove(&self.slot_out);
                        }
                    }
                    // Also clean up timeout tracking as we advance slot_out
                    self.proposal_times.remove(&self.slot_out);
                    self.perform(self.slot_out);
                }
            }
        };
        self.propose()?;
        Ok(())
    }

    // perform() is invoked with the same sequence of commands at
    // all replicas. First, it checks to see if it has already
    // performed the command. Different replicas may end up proposing
    // the same command for different slots, and thus the same
    // command may be decided multiple times. The corresponding
    // operation is evaluated only if the command is new and it is
    // not a reconfiguration request. If so, perform() applies the
    // requested operation to the application state. In either case,
    // the function increments slot_out.
    pub fn perform(&mut self, slot: u64) {
        if let Some(command) = self.decisions.get(&slot) {
            for s in 1..self.slot_out {
                if self.decisions.get(&s) == Some(command) {
                    self.slot_out += 1;
                    return;
                }
            }
            if let types::CommandType::Reconfig(_) = &command.op {
                self.slot_out += 1;
                return;
            }
        }
        self.slot_out += 1;
    }

    // propose() tries to transfer requests from the set requests
    // to proposals. It uses slot_in to look for unused slots within
    // the window of slots with known configurations. For each such
    // slot, it first checks if the configuration for that slot is
    // different from the prior slot by checking if the decision in
    // (slot_in - WINDOW) is a reconfiguration command. If so, the
    // function updates the configuration for slot s. Then the
    // function pops a request from requests and adds it as a
    // proposal for slot_in to the set proposals. Finally, it sends a
    // Propose message to all leaders in the configuration of
    // slot_in.
    pub fn propose(&mut self) -> anyhow::Result<()> {
        let mut new_proposals = Vec::new(); // Track newly created proposals

        while !self.requests.is_empty() && self.slot_in < self.slot_out + WINDOW {
            if !self.decisions.contains_key(&self.slot_in) {
                let command = self.requests.remove(0);
                self.proposals.insert(self.slot_in, command.clone());
                let leaders: Vec<_> = self.config.leaders.iter().cloned().collect();
                for ldr in leaders {
                    self.send_message(ldr, self.slot_in, command.clone())?;
                }
                // Track this as a new proposal that needs timeout monitoring
                new_proposals.push(self.slot_in);
            }
            self.slot_in += 1;
            if self.slot_in > WINDOW && self.decisions.contains_key(&(self.slot_in - WINDOW)) {
                if let types::CommandType::Reconfig(config) =
                    &self.decisions[&(self.slot_in - WINDOW)].op
                {
                    self.config = config.clone();
                    info!(
                        "{}: updated config: {:?}",
                        self.slot_in - WINDOW,
                        self.decisions[&(self.slot_in - WINDOW)].op
                    );
                }
            }
        }

        // Schedule timeouts for new proposals
        if !new_proposals.is_empty() {
            self.schedule_proposal_timeouts(new_proposals)?;
        }

        Ok(())
    }

    /// Schedule timeouts for newly created proposals
    fn schedule_proposal_timeouts(&mut self, slots: Vec<u64>) -> anyhow::Result<()> {
        let slots_len = slots.len();
        for slot in slots {
            let timeout = self.config.timeout_config.min_timeout;
            self.proposal_times.insert(slot, timeout);
        }

        // Schedule a general repropose check if not already scheduled
        // (This is a periodic check, not per-proposal)
        if self.proposal_times.len() == slots_len {
            // This was the first batch of proposals, start the periodic check
            self.schedule_repropose_check()?;
        }

        Ok(())
    }

    /// Handle timer events from the clock system
    pub fn handle_timer(&mut self, action: ClockAction) -> anyhow::Result<()> {
        match action {
            ClockAction::ReproposePendingRequests => {
                // Retry proposals that haven't received decisions
                self.repropose_pending_requests()?;
            }
            ClockAction::CheckSlotWindow => {
                // Check if slot_out progress is stuck and try to advance
                self.check_slot_progress()?;
            }
            _ => {
                // Ignore action types not relevant to replicas
            }
        }
        Ok(())
    }

    /// Repropose requests for slots that haven't received decisions within timeout
    fn repropose_pending_requests(&mut self) -> anyhow::Result<()> {
        let mut slots_to_repropose = Vec::new();

        // Find slots with proposals but no decisions that have timed out
        for &slot in self.proposals.keys() {
            if !self.decisions.contains_key(&slot) {
                // This proposal hasn't received a decision yet
                slots_to_repropose.push(slot);
            }
        }

        // Repropose to leaders (they might have changed or previous messages lost)
        for slot in slots_to_repropose {
            if let Some(command) = self.proposals.get(&slot).cloned() {
                let leaders: Vec<_> = self.config.leaders.iter().cloned().collect();
                for ldr in leaders {
                    self.send_message(ldr, slot, command.clone())?;
                }
                // Update timeout for this proposal
                let timeout = self.config.timeout_config.min_timeout;
                self.proposal_times.insert(slot, timeout);
            }
        }

        // Schedule next check
        self.schedule_repropose_check()?;
        Ok(())
    }

    /// Check if slot_out is making progress, and handle stalls
    fn check_slot_progress(&mut self) -> anyhow::Result<()> {
        // This is a more complex scenario - if slot_out is stuck waiting for a decision
        // that may never come, we might need to trigger leader election or other recovery
        // For now, just schedule the next check
        self.schedule_slot_check()?;
        Ok(())
    }

    /// Schedule a repropose check
    fn schedule_repropose_check(&mut self) -> anyhow::Result<()> {
        let timeout = self.config.timeout_config.min_timeout * 2; // Slightly longer interval
        self.clock
            .schedule(ClockAction::ReproposePendingRequests, timeout);
        Ok(())
    }

    /// Schedule a slot progress check
    fn schedule_slot_check(&mut self) -> anyhow::Result<()> {
        let timeout = self.config.timeout_config.max_timeout; // Longer interval for progress checks
        self.clock.schedule(ClockAction::CheckSlotWindow, timeout);
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

    fn send_message(
        &mut self,
        ldr: types::LeaderId,
        slot: u64,
        command: types::Command,
    ) -> anyhow::Result<()> {
        let msg = messages::ProposeMessage {
            src: self.node_id,
            slot_number: slot,
            command: command.clone(),
        };
        let ldr_address = self
            .config
            .get_address(ldr.as_ref())
            .ok_or(anyhow::anyhow!("Leader address not found"))?;
        let sendable = messages::SendableMessage {
            src: self.address.clone(),
            dst: ldr_address.clone(),
            message: messages::Message::Propose(msg),
        };
        self.mailbox.send(sendable);
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

    fn setup() -> Replica {
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
        Replica::new(rep, config, mailbox, clock).unwrap()
    }

    #[test]
    fn replica_proposes_on_request() {
        // Setup
        let mut replica = setup();

        // Inject request
        let command = Command {
            client_id: replica.node_id.as_ref().clone(),
            request_id: 1,
            op: CommandType::Op(vec![1, 2, 3]),
        };
        let req_msg = RequestMessage {
            src: replica.address.clone(),
            command: command.clone(),
        };
        replica
            .handle_msg(ReplicaMessageIn::Request(req_msg))
            .unwrap();

        // Assert proposal created
        assert!(replica.proposals.values().any(|c| c == &command));
        // Assert outgoing Propose message
        assert!(replica
            .mailbox
            .outbox
            .iter()
            .any(|msg| matches!(msg.message, Message::Propose(_))));
    }

    // Add more tests for decision handling, duplicate decisions, etc.

    #[test]
    fn replica_schedules_timeouts_for_proposals() {
        let mut replica = setup();

        // Inject a request to trigger proposal
        let command = Command {
            client_id: replica.node_id.as_ref().clone(),
            request_id: 1,
            op: CommandType::Op(vec![1, 2, 3]),
        };
        let req_msg = RequestMessage {
            src: replica.address.clone(),
            command: command.clone(),
        };
        replica
            .handle_msg(ReplicaMessageIn::Request(req_msg))
            .unwrap();

        // Should have created timeout tracking for the proposal
        assert!(
            !replica.proposal_times.is_empty(),
            "Should have timeout tracking for proposals"
        );

        // Should have one proposal
        assert_eq!(replica.proposals.len(), 1);

        // The proposal should be for slot 1 (slot_in starts at 1)
        assert!(replica.proposals.contains_key(&1));
        assert!(replica.proposal_times.contains_key(&1));
    }

    #[test]
    fn replica_cleans_up_timeouts_on_decision() {
        let mut replica = setup();

        // Create a proposal first
        let command = Command {
            client_id: replica.node_id.as_ref().clone(),
            request_id: 1,
            op: CommandType::Op(vec![1, 2, 3]),
        };
        let req_msg = RequestMessage {
            src: replica.address.clone(),
            command: command.clone(),
        };
        replica
            .handle_msg(ReplicaMessageIn::Request(req_msg))
            .unwrap();

        // Verify timeout tracking exists
        assert!(replica.proposal_times.contains_key(&1));

        // Now send a decision for that slot
        let decision_msg = DecisionMessage {
            src: LeaderId::new(1), // Decision comes from a leader
            slot_number: 1,
            command: command.clone(),
        };
        replica
            .handle_msg(ReplicaMessageIn::Decision(decision_msg))
            .unwrap();

        // Timeout tracking should be cleaned up
        assert!(
            !replica.proposal_times.contains_key(&1),
            "Timeout tracking should be cleaned up after decision"
        );
    }

    #[test]
    fn replica_handles_repropose_timer() {
        let mut replica = setup();

        // Create a proposal that hasn't received a decision
        replica.proposals.insert(
            1,
            Command {
                client_id: replica.node_id.as_ref().clone(),
                request_id: 1,
                op: CommandType::Op(vec![1, 2, 3]),
            },
        );
        replica.proposal_times.insert(1, Duration::from_millis(100));

        // Clear outbox to test reproposing
        replica.mailbox.clear_outbox();

        // Handle repropose timer
        replica
            .handle_timer(ClockAction::ReproposePendingRequests)
            .unwrap();

        // Should have sent new Propose messages to all leaders
        let propose_messages: Vec<_> = replica
            .mailbox
            .outbox
            .iter()
            .filter(|msg| matches!(msg.message, Message::Propose(_)))
            .collect();

        // Should send to all leaders in config (we have 1 leader in setup)
        assert_eq!(propose_messages.len(), replica.config.leaders.len());
    }
}
