use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::time::Duration;

/// A ballot number is a lexicographically ordered pair of an integer
/// and the identifier of the ballot's leader.
#[derive(Clone, Debug, Hash, Eq, PartialEq, PartialOrd)]
pub struct BallotNumber {
    pub round: u64,
    pub leader: LeaderId,
}

impl BallotNumber {
    pub fn new(leader_id: LeaderId) -> Self {
        BallotNumber {
            round: 0,
            leader: leader_id,
        }
    }
}

/// PValue is a triple consisting of a ballot number, a slot number, a command.
#[derive(Clone, Debug, PartialEq)]
pub struct PValue {
    pub ballot_number: BallotNumber,
    pub slot: u64,
    pub command: Command,
}

/// A command consists of the process identifier of the client
// submitting the request, a client-local request identifier, and a command
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub client_id: NodeId,
    pub request_id: u64,
    pub op: CommandType,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandType {
    // An operation (which can be anything).
    Op(Vec<u8>),
    // A ReconfigCommand is a command that changes the
    // configuration of the system
    Reconfig(Config),
}

/// Used by leaders and acceptors to configure timeouts
/// for various operations.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeoutConfig {
    // Backoff parameters
    pub min_timeout: Duration,
    pub max_timeout: Duration,
    pub timeout_multiplier: f32,
    pub timeout_decrease: Duration,
}
impl Default for TimeoutConfig {
    fn default() -> Self {
        TimeoutConfig {
            min_timeout: Duration::from_millis(100),
            max_timeout: Duration::from_secs(10),
            timeout_multiplier: 1.5,
            timeout_decrease: Duration::from_millis(50),
        }
    }
}

/// A configuration consists of a list of replicas, a list of
/// acceptors and a list of leaders as well as a mapping of
/// IDs to addresses.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub replicas: HashSet<ReplicaId>,
    pub acceptors: HashSet<AcceptorId>,
    pub leaders: HashSet<LeaderId>,
    pub id_address_map: BTreeMap<NodeId, Address>,
    pub timeout_config: TimeoutConfig,
}

impl Config {
    pub fn new(
        replicas: HashSet<ReplicaId>,
        acceptors: HashSet<AcceptorId>,
        leaders: HashSet<LeaderId>,
        id_address_map: BTreeMap<NodeId, Address>,
        timeout_config: Option<TimeoutConfig>,
    ) -> Config {
        Config {
            replicas,
            acceptors,
            leaders,
            id_address_map,
            timeout_config: timeout_config.unwrap_or_default(),
        }
    }

    pub fn get_address(&self, id: &NodeId) -> Option<&Address> {
        self.id_address_map.get(id)
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Address {
    ip: String,
    port: u64,
}

impl Address {
    pub fn new(ip: String, port: u64) -> Address {
        Address { ip, port }
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

/// A ServerId is a unique identifier for a server in the system
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct NodeId(u64);

impl NodeId {
    pub fn new(id: u64) -> NodeId {
        NodeId(id)
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Node{}", self.0)
    }
}

/// Newtypes for the different kinds of servers in the system
/// These protect their internal data.
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, PartialOrd)]
pub struct AcceptorId(NodeId);

impl AcceptorId {
    pub fn new(id: u64) -> AcceptorId {
        AcceptorId(NodeId::new(id))
    }
    pub fn as_ref(&self) -> &NodeId {
        &self.0
    }
}

impl Into<NodeId> for AcceptorId {
    fn into(self) -> NodeId {
        self.0
    }
}

impl std::fmt::Display for AcceptorId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Acceptor{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, PartialOrd)]
pub struct LeaderId(NodeId);
impl std::fmt::Display for LeaderId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Leader{}", self.0)
    }
}

impl LeaderId {
    pub fn new(id: u64) -> LeaderId {
        LeaderId(NodeId::new(id))
    }
    pub fn as_ref(&self) -> &NodeId {
        &self.0
    }
}

impl Into<NodeId> for LeaderId {
    fn into(self) -> NodeId {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, PartialOrd)]
pub struct ReplicaId(NodeId);

impl ReplicaId {
    pub fn new(id: u64) -> ReplicaId {
        ReplicaId(NodeId::new(id))
    }
    pub fn as_ref(&self) -> &NodeId {
        &self.0
    }
}

impl Into<NodeId> for ReplicaId {
    fn into(self) -> NodeId {
        self.0
    }
}

impl std::fmt::Display for ReplicaId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Replica{}", self.0)
    }
}

pub trait Server {
    fn id(&self) -> &NodeId;
    fn address(&self) -> &Address;
}
