# Node Types

As mentioned previously, there are three node types here:

- Replicas
- Leaders
- Acceptors

Paxos is generally fault-tolerant in the face of _n_ faults with _2n+1_ Acceptors (and _n+1_ Leaders and Replicas).

## Replicas

Replicas serve as the primary interface between clients and the consensus system while maintaining the replicated state machine.

### Responsibilities

**Client Interface**: Replicas receive requests from clients and return responses after command execution.

**State Maintenance**: Each replica maintains a copy of the application state and applies commands in the agreed sequence.

**Command Proposal**: When receiving client requests, replicas propose commands to leaders for inclusion in the consensus sequence.

**Command Execution**: Replicas execute commands in slot order, ensuring all replicas maintain identical state.

### Key State Variables

- `slot_in`: Next slot number for new proposals
- `slot_out`: Next slot number for command execution
- `proposals`: Commands proposed but not yet decided
- `decisions`: Commands that have been decided by consensus
- `requests`: Client requests awaiting proposal

### Operation Flow

1. Receive client request
2. Add request to pending queue
3. Propose command to leaders for available slots
4. Wait for consensus decision
5. Execute command when slot becomes ready
6. Return response to client

## Leaders

Leaders coordinate the consensus process by managing the two-phase Paxos protocol and handling leader election.

### Responsibilities

**Consensus Coordination**: Leaders run the two-phase protocol (prepare/accept) to achieve agreement on command values.

**Proposal Management**: Leaders receive proposals from replicas and coordinate their inclusion in the consensus sequence.

**Conflict Resolution**: When multiple leaders compete, the system resolves conflicts through ballot number comparison.

**Decision Broadcasting**: Once consensus is achieved, leaders broadcast decisions to all replicas.

### Key State Variables

- `ballot_number`: Current ballot for this leader
- `proposals`: Commands being processed for consensus
- `p1b_responses`: Promise responses from acceptors
- `p2b_responses`: Accept responses from acceptors
- `active`: Whether this leader is currently active

### Operation Phases

**Scout Phase**: Leader attempts to become active by gathering promises from acceptors.

**Commander Phase**: Active leader proposes specific values for slots and gathers acceptances.

**Decision Phase**: Leader broadcasts decisions once sufficient acceptances are received.

## Acceptors

Acceptors provide the persistent storage layer for the consensus protocol, ensuring safety properties are maintained.

### Responsibilities

**Promise Management**: Acceptors track ballot numbers and promise not to accept proposals from lower-numbered ballots.

**Proposal Storage**: Acceptors store accepted proposals and their associated ballot numbers.

**Safety Enforcement**: Acceptors ensure that only one value can be chosen for each slot by rejecting conflicting proposals.

**Persistence**: Acceptors maintain durable storage of consensus state to survive failures.

### Key State Variables

- `promised`: Highest ballot number promised for each slot
- `accepted`: Accepted proposals with their ballot numbers
- Per-slot tracking of promises and acceptances

### Protocol Behavior

**Phase 1a Response**: When receiving prepare requests, acceptors either promise to ignore lower ballots or reject the request.

**Phase 2a Response**: When receiving accept requests, acceptors either accept the proposal (if consistent with promises) or reject it.

## Node Interaction Patterns

### Normal Operation

1. Client sends request to replica
2. Replica proposes command to leader
3. Leader runs two-phase protocol with acceptors
4. Leader broadcasts decision to replicas
5. Replicas execute command and respond to client

### Leader Election

1. Multiple leaders may attempt to become active
2. Leaders send prepare requests with unique ballot numbers
3. Higher-numbered ballots preempt lower-numbered ones
4. Only one leader becomes active for each ballot

### Failure Recovery

1. Failed nodes are detected through timeouts
2. Remaining nodes continue operation
3. New leaders can be elected if current leader fails
4. State is recovered from acceptor persistence
