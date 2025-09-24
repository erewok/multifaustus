# State Diagrams

This section presents the state machines for each node type in the MultiPaxos implementation. Understanding these state transitions is crucial for comprehending node behavior and system dynamics.

## Replica State Machine

Replicas manage client requests and coordinate with leaders to maintain consistent state across the distributed system.

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> Proposing : receive_client_request()
    Proposing --> WaitingDecision : send_propose_to_leaders()
    WaitingDecision --> Executing : receive_decision()
    WaitingDecision --> Reproposing : timeout / higher_ballot_preemption
    Executing --> Idle : execute_command()
    Reproposing --> WaitingDecision : retry_propose()
    Reproposing --> Idle : give_up_after_max_retries()

    state Idle {
        [*] --> ProcessingRequests
        ProcessingRequests --> CheckingSlots
        CheckingSlots --> ProcessingRequests
    }

    state Executing {
        [*] --> ValidatingCommand
        ValidatingCommand --> ApplyingCommand
        ApplyingCommand --> SendingResponse
        SendingResponse --> [*]
    }
```

### State Descriptions

**Idle**: Replica is ready to accept new client requests and process incoming decisions.

**Proposing**: Replica is preparing to send command proposals to leaders for available slots.

**WaitingDecision**: Replica has sent proposals and is waiting for consensus decisions from leaders.

**Executing**: Replica is applying decided commands to its state machine and preparing client responses.

**Reproposing**: Replica is retrying failed proposals, typically due to leader preemption or timeout.

### Key Transitions

- **Client Request**: Triggers transition from Idle to Proposing
- **Proposal Sent**: Moves from Proposing to WaitingDecision
- **Decision Received**: Advances from WaitingDecision to Executing
- **Timeout/Preemption**: Causes transition to Reproposing state
- **Command Applied**: Returns to Idle state for next request cycle

## Leader State Machine

Leaders coordinate the consensus process through ballot management and two-phase protocol execution.

```mermaid
stateDiagram-v2
    [*] --> Inactive
    Inactive --> ScoutPhase : start_election()
    ScoutPhase --> CommanderPhase : receive_adopted()
    ScoutPhase --> Preempted : receive_preempted()
    CommanderPhase --> Active : sufficient_acceptances()
    Active --> CommanderPhase : new_proposal()
    Active --> Preempted : higher_ballot_detected()
    Preempted --> Inactive : backoff_complete()
    Inactive --> ScoutPhase : retry_election()

    state ScoutPhase {
        [*] --> SendingP1a
        SendingP1a --> CollectingP1b
        CollectingP1b --> EvaluatingPromises
        EvaluatingPromises --> [*] : majority_achieved
        EvaluatingPromises --> SendingP1a : insufficient_responses
    }

    state CommanderPhase {
        [*] --> SendingP2a
        SendingP2a --> CollectingP2b
        CollectingP2b --> BroadcastingDecision
        BroadcastingDecision --> [*]
    }

    state Active {
        [*] --> MonitoringProposals
        MonitoringProposals --> ProcessingProposal
        ProcessingProposal --> MonitoringProposals
    }
```

### State Descriptions

**Inactive**: Leader is not currently participating in consensus, waiting for election opportunity.

**ScoutPhase**: Leader is attempting to become active by gathering promises from acceptors (Phase 1).

**CommanderPhase**: Leader is proposing specific values and collecting acceptances (Phase 2).

**Active**: Leader is successfully coordinating consensus and processing new proposals.

**Preempted**: Leader has been superseded by a higher ballot and must become inactive.

### Key Transitions

- **Start Election**: Inactive leader begins scout phase with new ballot
- **Adopted**: Scout phase succeeds, leader becomes active
- **Preempted**: Higher ballot detected, leader becomes inactive
- **New Proposal**: Active leader starts commander phase for new slot
- **Backoff Complete**: Preempted leader waits before retry

## Acceptor State Machine

Acceptors maintain the persistent storage for consensus decisions and enforce protocol safety properties.

```mermaid
stateDiagram-v2
    [*] --> Listening
    Listening --> PromiseEvaluation : receive_p1a()
    PromiseEvaluation --> PromiseGiven : ballot_acceptable()
    PromiseEvaluation --> Listening : ballot_rejected()
    PromiseGiven --> AcceptEvaluation : receive_p2a()
    PromiseGiven --> Preempted : higher_ballot_p1a()
    AcceptEvaluation --> ProposalAccepted : proposal_consistent()
    AcceptEvaluation --> PromiseGiven : proposal_rejected()
    ProposalAccepted --> Listening : send_p2b()
    Preempted --> Listening : update_promise()

    state Listening {
        [*] --> MonitoringMessages
        MonitoringMessages --> PeriodicCleanup
        PeriodicCleanup --> MonitoringMessages
    }

    state PromiseEvaluation {
        [*] --> CheckingBallot
        CheckingBallot --> ComparingPromises
        ComparingPromises --> [*]
    }

    state AcceptEvaluation {
        [*] --> ValidatingProposal
        ValidatingProposal --> CheckingConsistency
        CheckingConsistency --> [*]
    }

    state ProposalAccepted {
        [*] --> StoringAcceptance
        StoringAcceptance --> SendingResponse
        SendingResponse --> [*]
    }
```

### State Descriptions

**Listening**: Acceptor is ready to receive and process protocol messages from leaders.

**PromiseEvaluation**: Acceptor is evaluating a P1a message to determine if it should make a promise.

**PromiseGiven**: Acceptor has promised not to accept lower ballots and awaits P2a messages.

**AcceptEvaluation**: Acceptor is evaluating a P2a message for consistency with previous promises.

**ProposalAccepted**: Acceptor has accepted a proposal and is updating persistent state.

**Preempted**: Acceptor has received a higher ballot and must update its promises.

### Key Transitions

- **P1a Received**: Triggers evaluation of prepare request
- **Promise Made**: Acceptor commits to ignoring lower ballots
- **P2a Received**: Triggers evaluation of accept request
- **Proposal Accepted**: Acceptor stores the accepted value
- **Higher Ballot**: Causes preemption and promise update

## Cross-Node Interaction Patterns

The state machines interact through message exchanges that drive state transitions across the distributed system:

**Client-Replica Interaction**: Client requests trigger replica state changes from Idle to Proposing.

**Replica-Leader Coordination**: Replica proposals cause leaders to enter Commander phase from Active state.

**Leader-Acceptor Protocol**: Leader scout and commander phases drive acceptor state transitions through promise and accept evaluations.

**Failure Detection**: Timeouts in any node can trigger state transitions that initiate recovery procedures.

These state machines operate concurrently across multiple nodes, with message passing providing the coordination mechanism that ensures consistent behavior throughout the distributed system.
