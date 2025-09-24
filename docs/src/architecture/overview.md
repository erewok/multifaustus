# System Architecture Overview

The MultiPaxos implementation follows a distributed architecture where consensus is achieved through coordinated interaction between three types of nodes. The system is designed with separation of concerns, where each node type has distinct responsibilities in the consensus process.

## Architecture Principles

**Sans-IO Design**: Message processing logic is separated from network transport, enabling flexible deployment and comprehensive testing.

**State Machine Replication**: All replicas maintain identical state by executing the same sequence of commands in the same order.

**Fault Tolerance**: The system continues operating correctly despite node failures, provided sufficient nodes remain available.

**Configurable Deployment**: Node roles, network topology, and timeout parameters are configurable for different deployment scenarios.

## System Components

### Node Types

The system consists of three distinct node types:

- **Replicas**: Interface with clients and maintain application state
- **Leaders**: Coordinate consensus and manage proposal ordering
- **Acceptors**: Provide persistent storage for consensus decisions

### Communication Layer

Nodes communicate through a message-passing system with the following characteristics:

- Asynchronous message delivery
- Message queuing through mailbox abstraction
- Configurable timeouts and retry mechanisms
- Support for both reliable and unreliable network transports

### State Management

Each node maintains internal state appropriate to its role:

- **Slot-based ordering** for command sequencing
- **Ballot numbers** for leader election and conflict resolution
- **Promise tracking** for consensus safety
- **Configuration management** for dynamic reconfiguration

## Consensus Flow

The system achieves consensus through a two-phase protocol:

1. **Phase 1 (Prepare)**: Leaders request permission to propose values
2. **Phase 2 (Accept)**: Leaders propose specific values for agreement

This process ensures that only one value can be chosen for each position in the command sequence, maintaining consistency across all replicas.

## Failure Handling

The architecture addresses various failure scenarios:

- **Node failures**: Remaining nodes continue operation
- **Network partitions**: Majority partitions can make progress
- **Message loss**: Timeout and retry mechanisms ensure delivery
- **Leader failures**: New leaders can be elected automatically

## Configuration and Deployment

The system supports flexible deployment configurations:

- Variable numbers of each node type
- Customizable network topologies
- Adjustable timeout and retry parameters
- Dynamic reconfiguration capabilities
