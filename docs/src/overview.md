# Multifaustus Implementation

Multifaustus is an implementation of Multipaxos, a distributed consensus algorithm that enables multiple nodes in a distributed system to agree on a sequence of values, even in the presence of failures. This implementation provides a Rust-based MultiPaxos system with three distinct node types working together to achieve consensus.

## System Purpose

This implementation serves as a fault-tolerant distributed state machine where:

- Multiple clients can submit requests concurrently
- All replicas maintain identical state through ordered command execution
- The system continues operating despite node failures
- Consistency is maintained through the Paxos consensus protocol

## Key Components

The system consists of three node types:

**Replicas** maintain application state and interface with clients. They receive client requests, coordinate with leaders for ordering, and execute commands in the agreed sequence.

**Leaders** coordinate the consensus process by proposing commands and managing the two-phase protocol required for agreement among acceptors.

**Acceptors** provide fault-tolerant storage for the consensus protocol, maintaining promises and accepted proposals to ensure safety properties.

## Algorithm Foundation

This implementation follows the MultiPaxos protocol as described in:

- "Paxos Made Simple" by Leslie Lamport (2001)
- "Paxos Made Moderately Complex" by Robbert van Renesse and Deniz Altınbüken (2015)

The implementation uses a sans-IO design where message handling is separated from network transport, enabling flexible deployment and testing strategies.

## Quick Start

To understand the system:

1. Review the [System Overview](./architecture/overview.md) for architectural concepts
2. Examine [Node Types](./architecture/nodes.md) for detailed component behavior
3. Study the [Message Protocol](./architecture/messages.md) for interaction patterns
4. See [State Diagrams](./architecture/state-diagram.md) for node state transitions

For implementation details, consult the API Reference section.
