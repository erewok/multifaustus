# Sequence Diagrams

This section illustrates the message flows and timing relationships between nodes during various operational scenarios. These diagrams help visualize the distributed coordination required for consensus.

## Normal Operation Sequence

The following diagram shows the complete flow from client request to response during normal system operation.

```mermaid
sequenceDiagram
    participant C as Client
    participant R as Replica
    participant L as Leader
    participant A1 as Acceptor1
    participant A2 as Acceptor2
    participant A3 as Acceptor3

    C->>R: Request(command)
    Note over R: Add to requests queue
    R->>L: Propose(slot_n, command)

    Note over L: Start consensus for slot_n
    L->>A1: P1a(ballot_num)
    L->>A2: P1a(ballot_num)
    L->>A3: P1a(ballot_num)

    A1->>L: P1b(ballot_num, accepted_vals)
    A2->>L: P1b(ballot_num, accepted_vals)
    A3->>L: P1b(ballot_num, accepted_vals)

    Note over L: Majority achieved, proceed to Phase 2
    L->>A1: P2a(ballot_num, slot_n, command)
    L->>A2: P2a(ballot_num, slot_n, command)
    L->>A3: P2a(ballot_num, slot_n, command)

    A1->>L: P2b(ballot_num, slot_n)
    A2->>L: P2b(ballot_num, slot_n)
    Note over L: Majority acceptance achieved

    L->>R: Decision(slot_n, command)
    Note over R: Execute command in slot order
    R->>C: Response(result)
```

### Key Points

- Client request triggers the entire consensus process
- Two-phase protocol ensures safety with majority agreement
- Decision broadcast enables all replicas to execute the command
- Response sent after command execution maintains consistency

## Leader Election Sequence

This diagram shows how leadership conflicts are resolved when multiple leaders compete.

```mermaid
sequenceDiagram
    participant L1 as Leader1
    participant L2 as Leader2
    participant A1 as Acceptor1
    participant A2 as Acceptor2
    participant A3 as Acceptor3

    Note over L1,L2: Both leaders attempt to become active

    L1->>A1: P1a(ballot=1.1)
    L1->>A2: P1a(ballot=1.1)
    L1->>A3: P1a(ballot=1.1)

    L2->>A1: P1a(ballot=2.2)
    L2->>A2: P1a(ballot=2.2)
    L2->>A3: P1a(ballot=2.2)

    Note over A1,A3: Higher ballot (2.2) takes precedence

    A1->>L1: Preempted(ballot=2.2)
    A2->>L1: Preempted(ballot=2.2)
    A3->>L1: Preempted(ballot=2.2)

    A1->>L2: P1b(ballot=2.2, previous_accepted)
    A2->>L2: P1b(ballot=2.2, previous_accepted)
    A3->>L2: P1b(ballot=2.2, previous_accepted)

    Note over L1: Becomes inactive, will retry later
    Note over L2: Becomes active, can proceed with proposals

    L2->>A1: P2a(ballot=2.2, slot_n, command)
    L2->>A2: P2a(ballot=2.2, slot_n, command)
    L2->>A3: P2a(ballot=2.2, slot_n, command)
```

### Key Points

- Multiple leaders can attempt activation simultaneously
- Higher ballot numbers take precedence (lexicographic ordering)
- Preempted leaders become inactive and must retry with higher ballots
- Only one leader becomes active per ballot round

## Failure Recovery Sequence

This diagram illustrates how the system handles leader failure and recovery.

```mermaid
sequenceDiagram
    participant R as Replica
    participant L1 as Leader1(Failed)
    participant L2 as Leader2(New)
    participant A1 as Acceptor1
    participant A2 as Acceptor2
    participant A3 as Acceptor3

    R->>L1: Propose(slot_n, command)
    Note over L1: Leader fails before processing

    Note over R: Timeout waiting for decision
    Note over L2: Detects leader failure, starts election

    L2->>A1: P1a(ballot=3.2)
    L2->>A2: P1a(ballot=3.2)
    L2->>A3: P1a(ballot=3.2)

    A1->>L2: P1b(ballot=3.2, previous_accepted)
    A2->>L2: P1b(ballot=3.2, previous_accepted)
    A3->>L2: P1b(ballot=3.2, previous_accepted)

    Note over L2: Check previous accepted values
    Note over L2: Must propose previously accepted values first

    L2->>A1: P2a(ballot=3.2, slot_k, prev_command)
    L2->>A2: P2a(ballot=3.2, slot_k, prev_command)
    L2->>A3: P2a(ballot=3.2, slot_k, prev_command)

    A1->>L2: P2b(ballot=3.2, slot_k)
    A2->>L2: P2b(ballot=3.2, slot_k)

    L2->>R: Decision(slot_k, prev_command)

    Note over L2: Now can process new proposals
    Note over R: Retries original proposal
    R->>L2: Propose(slot_n, command)

    L2->>A1: P2a(ballot=3.2, slot_n, command)
    L2->>A2: P2a(ballot=3.2, slot_n, command)
    L2->>A3: P2a(ballot=3.2, slot_n, command)

    A1->>L2: P2b(ballot=3.2, slot_n)
    A2->>L2: P2b(ballot=3.2, slot_n)

    L2->>R: Decision(slot_n, command)
```

### Key Points

- Failure detection occurs through timeouts
- New leader must first complete any partially accepted proposals
- Previously accepted values take priority over new proposals
- System maintains consistency despite leader changes

## Network Partition Scenario

This diagram shows behavior during a network partition where nodes are split into groups.

```mermaid
sequenceDiagram
    participant R1 as Replica1
    participant L1 as Leader1
    participant A1 as Acceptor1
    participant A2 as Acceptor2
    participant A3 as Acceptor3
    participant L2 as Leader2
    participant R2 as Replica2

    Note over R1,A2: Majority partition (3 nodes)
    Note over A3,R2: Minority partition (2 nodes)

    R1->>L1: Propose(slot_n, command1)
    R2->>L2: Propose(slot_n, command2)

    Note over L1: Can reach majority of acceptors
    L1->>A1: P1a(ballot=1.1)
    L1->>A2: P1a(ballot=1.1)
    Note over L1,A3: Network partition - A3 unreachable

    Note over L2: Cannot reach majority of acceptors
    L2->>A3: P1a(ballot=2.2)
    Note over A1,L2: Network partition - A1,A2 unreachable

    A1->>L1: P1b(ballot=1.1, [])
    A2->>L1: P1b(ballot=1.1, [])
    Note over L1: Majority achieved, proceed

    L1->>A1: P2a(ballot=1.1, slot_n, command1)
    L1->>A2: P2a(ballot=1.1, slot_n, command1)

    A1->>L1: P2b(ballot=1.1, slot_n)
    A2->>L1: P2b(ballot=1.1, slot_n)

    L1->>R1: Decision(slot_n, command1)

    Note over A3,R2: Minority partition cannot make progress
    Note over L2: Waiting for majority, times out

    Note over R1,R2: Network partition heals
    Note over L2: Discovers higher ballot decisions
    L2->>R2: Decision(slot_n, command1)
    Note over R2: Must apply majority decision
```

### Key Points

- Only partitions with acceptor majority can make progress
- Minority partitions cannot achieve consensus and wait
- When partition heals, minority adopts majority decisions
- Consistency is maintained across partition boundaries

## Concurrent Proposals Sequence

This diagram shows how multiple concurrent proposals are handled across different slots.

```mermaid
sequenceDiagram
    participant R1 as Replica1
    participant R2 as Replica2
    participant L as Leader
    participant A1 as Acceptor1
    participant A2 as Acceptor2
    participant A3 as Acceptor3

    Note over R1,R2: Multiple replicas propose simultaneously

    R1->>L: Propose(slot_5, command_A)
    R2->>L: Propose(slot_6, command_B)

    Note over L: Process both proposals concurrently

    par Slot 5 Processing
        L->>A1: P2a(ballot=1.1, slot_5, command_A)
        L->>A2: P2a(ballot=1.1, slot_5, command_A)
        L->>A3: P2a(ballot=1.1, slot_5, command_A)
    and Slot 6 Processing
        L->>A1: P2a(ballot=1.1, slot_6, command_B)
        L->>A2: P2a(ballot=1.1, slot_6, command_B)
        L->>A3: P2a(ballot=1.1, slot_6, command_B)
    end

    par Slot 5 Responses
        A1->>L: P2b(ballot=1.1, slot_5)
        A2->>L: P2b(ballot=1.1, slot_5)
    and Slot 6 Responses
        A1->>L: P2b(ballot=1.1, slot_6)
        A3->>L: P2b(ballot=1.1, slot_6)
    end

    Note over L: Both slots achieve majority

    par Decision Broadcast
        L->>R1: Decision(slot_5, command_A)
        L->>R2: Decision(slot_5, command_A)
    and
        L->>R1: Decision(slot_6, command_B)
        L->>R2: Decision(slot_6, command_B)
    end

    Note over R1,R2: Execute commands in slot order
```

### Key Points

- Multiple slots can be processed concurrently
- Each slot requires independent majority agreement
- Commands are executed in slot order regardless of decision timing
- Parallelization improves system throughput
