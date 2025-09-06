# Multipaxos

This is version of multipaxos in Rust for fun (it's a hobby for when the surf is flat). This implementation is informed by the following references:

- "Paxos Made Simple", [Leslie Lamport 2001](https://lamport.azurewebsites.net/pubs/paxos-simple.pdf)
- "Paxos Made Moderately Complex", by "Robbert van Renesse and Deniz Altınbüken" (2015) see [paxos.systems](https://paxos.systems)

## State Machine

This algorithm is primarily implemented as a state machine in a sans-IO style.

I tried to use the role names from "Paxos Made Moderately Comlex":

- replicas,
- leaders, and
- acceptors

### Proposers

Proposers have the following responsibilities:

- Maintains application state
- Receives requests from clients
- Asks leaders to serialize the requests so all replicas see the same sequence
- Applies serialized requests to the application state
- Responds to clients

### Acceptors

Acceptors have the following responsbilities:

- Receives requests from replicas
- Serializes requests and responds to replicas

### Replicas (Learners)

Replicas have the following responsibilities:

- Maintains the fault tolerant memory of Paxos

### State Machine Updates

Each process has an inbox and an outbox (queues) where inbound messages can be added and outbound messages can be staged for delivery.