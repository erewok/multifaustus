use crate::types;
use std::fmt;

#[derive(Clone, Debug)]
pub struct SendableMessage {
    pub src: types::Address,
    pub dst: types::Address,
    pub message: Message,
}

/// Enum of all protocol messages exchanged between nodes in MultiPaxos.
#[derive(Clone, Debug)]
pub enum Message {
    /// Phase 1a: Sent by leaders to acceptors to initiate a new ballot (prepare).
    P1a(P1aMessage),
    /// Phase 1b: Sent by acceptors to leaders in response to P1a, promising not to accept lower ballots and reporting previously accepted proposals.
    P1b(P1bMessage),
    /// Phase 2a: Sent by leaders to acceptors to propose a value for a slot (accept).
    P2a(P2aMessage),
    /// Phase 2b: Sent by acceptors to leaders in response to P2a, confirming acceptance of the proposal for a slot.
    P2b(P2bMessage),
    /// Sent by acceptors or other leaders to preempt a leader with a higher ballot.
    Preempted(PreemptedMessage),
    /// Sent by leaders to replicas to inform them of a chosen command for a slot.
    Decision(DecisionMessage),
    /// Sent by clients to replicas to request execution of a command.
    Request(RequestMessage),
    /// Sent by replicas to leaders to propose a command for a slot.
    Propose(ProposeMessage),
}

impl fmt::Display for SendableMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.message {
            Message::P1a(_) => write!(f, "P1a from {} => {}", self.src, self.dst),
            Message::P1b(_) => write!(f, "P1b from {} => {}", self.src, self.dst),
            Message::P2a(_) => write!(f, "P2a from {} => {}", self.src, self.dst),
            Message::P2b(_) => write!(f, "P2b from {} => {}", self.src, self.dst),
            Message::Preempted(_) => write!(f, "Preempted from {} => {}", self.src, self.dst),
            Message::Decision(_) => write!(f, "Decision from {} => {}", self.src, self.dst),
            Message::Request(_) => write!(f, "Request from {} => {}", self.src, self.dst),
            Message::Propose(_) => write!(f, "Propose from {} => {}", self.src, self.dst),
        }
    }
}

/// Sent by leaders (scouts) to acceptors in Phase 1 of Paxos to initiate a new ballot (prepare).
#[derive(Clone, Debug)]
pub struct P1aMessage {
    pub src: types::LeaderId,
    pub ballot_number: types::BallotNumber,
}

/// Sent by acceptors to leaders (scouts) in response to P1a, promising not to accept lower ballots and reporting previously accepted proposals.
#[derive(Clone, Debug)]
pub struct P1bMessage {
    pub src: types::AcceptorId,
    pub ballot_number: types::BallotNumber,
    pub accepted: Vec<types::PValue>,
}

/// Sent by leaders (commanders) to acceptors in Phase 2 of Paxos to propose a value for a slot (accept).
#[derive(Clone, Debug)]
pub struct P2aMessage {
    pub src: types::LeaderId,
    pub ballot_number: types::BallotNumber,
    pub slot_number: u64,
    pub command: types::Command,
}

/// Sent by acceptors to leaders (commanders) in response to P2a, confirming acceptance of the proposal for a slot.
/// This message is an indicator that the proposal has been Accepted/Decided by a single Acceptor.
#[derive(Clone, Debug)]
pub struct P2bMessage {
    pub src: types::AcceptorId,
    pub ballot_number: types::BallotNumber,
    pub slot_number: u64,
}

/// Sent by acceptors or other leaders to preempt a leader with a higher ballot.
#[derive(Clone, Debug)]
pub struct PreemptedMessage {
    pub src: types::LeaderId,
    pub ballot_number: types::BallotNumber,
}

/// Sent by leaders to replicas to inform them of a chosen command for a slot.
#[derive(Clone, Debug)]
pub struct DecisionMessage {
    pub src: types::LeaderId,
    pub slot_number: u64,
    pub command: types::Command,
}

/// Sent by clients to replicas to request execution of a command.
#[derive(Clone, Debug)]
pub struct RequestMessage {
    pub src: types::Address,
    pub command: types::Command,
}

/// Sent by replicas to leaders to propose a command for a slot.
#[derive(Clone, Debug)]
pub struct ProposeMessage {
    pub src: types::ReplicaId,
    pub slot_number: u64,
    pub command: types::Command,
}
