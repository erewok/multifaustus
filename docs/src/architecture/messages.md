# Message Protocol

The MultiPaxos implementation uses a structured message protocol to coordinate consensus between nodes. Understanding these messages is essential for comprehending system behavior and debugging distributed scenarios.

## Message Types

The protocol defines nine message types that facilitate the consensus algorithm and client interaction.

### Consensus Protocol Messages

#### Phase 1a (Prepare)

**P1aMessage**: Sent by leaders to acceptors to initiate ballot preparation.

- `src`: Leader identifier
- `ballot_number`: Proposed ballot number for this round

Leaders send P1a messages when attempting to become active or when starting a new consensus round.

#### Phase 1b (Promise)

**P1bMessage**: Sent by acceptors to leaders in response to P1a messages.

- `src`: Acceptor identifier
- `ballot_number`: Ballot number being promised
- `accepted`: List of previously accepted proposals (PValue structs)

Acceptors send P1b messages to promise they won't accept lower-numbered ballots and report any previously accepted proposals.

#### Phase 2a (Accept)

**P2aMessage**: Sent by leaders to acceptors to propose specific values.

- `src`: Leader identifier
- `ballot_number`: Current ballot number
- `slot_number`: Slot for this proposal
- `command`: Command being proposed

Leaders send P2a messages when proposing specific commands for consensus slots.

#### Phase 2b (Accepted)

**P2bMessage**: Sent by acceptors to leaders confirming proposal acceptance.

- `src`: Acceptor identifier
- `ballot_number`: Ballot number of accepted proposal
- `slot_number`: Slot number of accepted proposal

Acceptors send P2b messages to confirm they have accepted a specific proposal.

### Control Messages

#### Preempted

**PreemptedMessage**: Sent to notify leaders they have been preempted by higher ballots.

- `src`: Sender identifier
- `ballot_number`: Higher ballot number that caused preemption

This message informs leaders that a higher-numbered ballot is active, causing them to become inactive.

#### Decision

**DecisionMessage**: Sent by leaders to replicas announcing consensus decisions.

- `src`: Leader identifier
- `slot_number`: Slot for which decision was made
- `command`: Command that was chosen for this slot

Leaders broadcast decision messages once sufficient acceptors have accepted a proposal.

### Client Interaction Messages

#### Request

**RequestMessage**: Sent by clients to replicas requesting command execution.

- `src`: Client identifier
- `command`: Command to be executed

Clients use request messages to submit operations for execution by the replicated state machine.

#### Propose

**ProposeMessage**: Sent by replicas to leaders requesting command inclusion.

- `src`: Replica identifier
- `slot_number`: Proposed slot for the command
- `command`: Command to be included in consensus

Replicas send propose messages to request that leaders include specific commands in the consensus sequence.

## Message Flow Patterns

### Normal Operation Flow

```
Client → Replica: Request(command)
Replica → Leader: Propose(slot, command)
Leader → Acceptors: P1a(ballot)
Acceptors → Leader: P1b(ballot, accepted_proposals)
Leader → Acceptors: P2a(ballot, slot, command)
Acceptors → Leader: P2b(ballot, slot)
Leader → Replicas: Decision(slot, command)
```

### Leader Election Flow

```
Leader1 → Acceptors: P1a(ballot1)
Leader2 → Acceptors: P1a(ballot2)  // ballot2 > ballot1
Acceptors → Leader1: Preempted(ballot2)
Acceptors → Leader2: P1b(ballot2, previous_accepted)
Leader2 → Acceptors: P2a(ballot2, slot, command)
```

### Failure Recovery Flow

```
Replica → Leader: Propose(slot, command)
// Leader fails, timeout occurs
NewLeader → Acceptors: P1a(higher_ballot)
Acceptors → NewLeader: P1b(higher_ballot, accepted)
NewLeader → Acceptors: P2a(higher_ballot, slot, command)
```

## Message Routing

Messages are routed between nodes using address information maintained in the system configuration:

- Each node has a unique identifier and network address
- Messages specify source and destination addresses
- The transport layer handles actual message delivery
- Message queues buffer incoming and outgoing messages

## Error Handling

The protocol handles various error conditions:

**Message Loss**: Timeout mechanisms trigger retransmission of important messages.

**Out-of-Order Delivery**: Ballot numbers and slot numbers provide ordering guarantees.

**Duplicate Messages**: Idempotent message handling prevents state corruption.

**Network Partitions**: Majority requirements ensure progress only when sufficient nodes are available.
