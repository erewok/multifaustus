use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

use crate::messages;

/// A scheduled action to be executed at a specific time.
#[derive(Debug, Clone)]
pub struct TimerEvent {
    pub when: Instant,
    pub action: ClockAction,
}

impl PartialEq for TimerEvent {
    fn eq(&self, other: &Self) -> bool {
        self.when == other.when
    }
}

impl Eq for TimerEvent {}

impl PartialOrd for TimerEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimerEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (earliest first)
        other.when.cmp(&self.when)
    }
}

/// Actions that can be scheduled for later execution.
#[derive(Debug, Clone)]
pub enum ClockAction {
    // Leader actions
    SendScout { ballot: crate::types::BallotNumber },
    RetryProposal { slot: u64 },
    LeaderHeartbeat,

    // Replica actions
    ReproposePendingRequests,
    CheckSlotWindow,

    // Acceptor actions
    AcceptorHeartbeat,

    // Custom action with identifier
    Custom(String),
}

/// Events that can be processed by nodes.
#[derive(Debug)]
pub enum ClockEvent {
    Message(Box<messages::SendableMessage>),
    Timer(ClockAction),
    Tick, // Regular check for timeouts
}

/// Trait for different clock implementations.
pub trait ClockProvider {
    /// Get the current time.
    fn now(&self) -> Instant;

    /// Schedule an action to occur after the given duration.
    fn schedule(&mut self, action: ClockAction, delay: Duration);

    /// Schedule an action to occur at a specific time.
    fn schedule_at(&mut self, action: ClockAction, when: Instant);

    /// Cancel all pending actions of a specific type.
    fn cancel(&mut self, action_type: &ClockAction);

    /// Get the next pending timer event, if any.
    fn next_timeout(&self) -> Option<Duration>;

    /// Check for expired timers and return them.
    fn check_timers(&mut self) -> Vec<ClockAction>;
}

/// A concrete clock implementation that can be used in production or tests.
pub struct Clock {
    provider: Box<dyn ClockProvider + Send>,
}

impl Clock {
    pub fn new(provider: Box<dyn ClockProvider + Send>) -> Self {
        Clock { provider }
    }

    pub fn now(&self) -> Instant {
        self.provider.now()
    }

    pub fn schedule(&mut self, action: ClockAction, delay: Duration) {
        self.provider.schedule(action, delay);
    }

    pub fn schedule_at(&mut self, action: ClockAction, when: Instant) {
        self.provider.schedule_at(action, when);
    }

    pub fn cancel(&mut self, action_type: &ClockAction) {
        self.provider.cancel(action_type);
    }

    pub fn next_timeout(&self) -> Option<Duration> {
        self.provider.next_timeout()
    }

    pub fn check_timers(&mut self) -> Vec<ClockAction> {
        self.provider.check_timers()
    }
}

/// A real-time clock provider for production use.
#[derive(Debug)]
pub struct SystemClock {
    timers: BinaryHeap<TimerEvent>,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemClock {
    pub fn new() -> Self {
        SystemClock {
            timers: BinaryHeap::new(),
        }
    }
}

impl ClockProvider for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn schedule(&mut self, action: ClockAction, delay: Duration) {
        let when = self.now() + delay;
        self.schedule_at(action, when);
    }

    fn schedule_at(&mut self, action: ClockAction, when: Instant) {
        self.timers.push(TimerEvent { when, action });
    }

    fn cancel(&mut self, action_type: &ClockAction) {
        // Note: This is a simple implementation that recreates the heap.
        // For better performance, consider using a more sophisticated data structure.
        let timers: Vec<_> = self
            .timers
            .drain()
            .filter(|timer| !SystemClock::actions_match(&timer.action, action_type))
            .collect();

        for timer in timers {
            self.timers.push(timer);
        }
    }

    fn next_timeout(&self) -> Option<Duration> {
        self.timers.peek().map(|timer| {
            let now = self.now();
            if timer.when > now {
                timer.when - now
            } else {
                Duration::from_millis(0)
            }
        })
    }

    fn check_timers(&mut self) -> Vec<ClockAction> {
        let now = self.now();
        let mut expired = Vec::new();

        while let Some(timer) = self.timers.peek() {
            if timer.when <= now {
                expired.push(self.timers.pop().unwrap().action);
            } else {
                break;
            }
        }

        expired
    }
}

impl SystemClock {
    fn actions_match(action1: &ClockAction, action2: &ClockAction) -> bool {
        use ClockAction::*;
        match (action1, action2) {
            (SendScout { .. }, SendScout { .. }) => true,
            (RetryProposal { .. }, RetryProposal { .. }) => true,
            (LeaderHeartbeat, LeaderHeartbeat) => true,
            (ReproposePendingRequests, ReproposePendingRequests) => true,
            (CheckSlotWindow, CheckSlotWindow) => true,
            (AcceptorHeartbeat, AcceptorHeartbeat) => true,
            (Custom(s1), Custom(s2)) => s1 == s2,
            _ => false,
        }
    }
}

/// A controllable clock for testing.
#[derive(Debug)]
pub struct MockClock {
    current_time: Instant,
    timers: BinaryHeap<TimerEvent>,
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MockClock {
    pub fn new() -> Self {
        MockClock {
            current_time: Instant::now(),
            timers: BinaryHeap::new(),
        }
    }

    /// Advance the mock clock by the given duration.
    pub fn advance(&mut self, duration: Duration) {
        self.current_time += duration;
    }

    /// Set the mock clock to a specific time.
    pub fn set_time(&mut self, time: Instant) {
        self.current_time = time;
    }

    /// Get all pending timers (for testing).
    pub fn pending_timers(&self) -> Vec<&TimerEvent> {
        self.timers.iter().collect()
    }
}

impl ClockProvider for MockClock {
    fn now(&self) -> Instant {
        self.current_time
    }

    fn schedule(&mut self, action: ClockAction, delay: Duration) {
        let when = self.current_time + delay;
        self.schedule_at(action, when);
    }

    fn schedule_at(&mut self, action: ClockAction, when: Instant) {
        self.timers.push(TimerEvent { when, action });
    }

    fn cancel(&mut self, action_type: &ClockAction) {
        let timers: Vec<_> = self
            .timers
            .drain()
            .filter(|timer| !MockClock::actions_match(&timer.action, action_type))
            .collect();

        for timer in timers {
            self.timers.push(timer);
        }
    }

    fn next_timeout(&self) -> Option<Duration> {
        self.timers.peek().map(|timer| {
            if timer.when > self.current_time {
                timer.when - self.current_time
            } else {
                Duration::from_millis(0)
            }
        })
    }

    fn check_timers(&mut self) -> Vec<ClockAction> {
        let mut expired = Vec::new();

        while let Some(timer) = self.timers.peek() {
            if timer.when <= self.current_time {
                expired.push(self.timers.pop().unwrap().action);
            } else {
                break;
            }
        }

        expired
    }
}

impl MockClock {
    fn actions_match(action1: &ClockAction, action2: &ClockAction) -> bool {
        use ClockAction::*;
        match (action1, action2) {
            (SendScout { .. }, SendScout { .. }) => true,
            (RetryProposal { .. }, RetryProposal { .. }) => true,
            (LeaderHeartbeat, LeaderHeartbeat) => true,
            (ReproposePendingRequests, ReproposePendingRequests) => true,
            (CheckSlotWindow, CheckSlotWindow) => true,
            (AcceptorHeartbeat, AcceptorHeartbeat) => true,
            (Custom(s1), Custom(s2)) => s1 == s2,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_mock_clock_basic_functionality() {
        let mut mock_clock = MockClock::new();

        // Test MockClock directly
        let _start_time = mock_clock.now();

        // Schedule an action
        let action = ClockAction::LeaderHeartbeat;
        mock_clock.schedule(action.clone(), Duration::from_millis(100));

        // Should have no expired timers yet
        assert!(mock_clock.check_timers().is_empty());

        // Advance time
        mock_clock.advance(Duration::from_millis(50));

        // Still no expired timers
        assert!(mock_clock.check_timers().is_empty());

        // Advance past the scheduled time
        mock_clock.advance(Duration::from_millis(60));

        // Now we should have an expired timer
        let expired = mock_clock.check_timers();
        assert_eq!(expired.len(), 1);
        matches!(expired[0], ClockAction::LeaderHeartbeat);
    }

    #[test]
    fn test_clock_scheduling_and_cancellation() {
        let mut mock_clock = MockClock::new();

        // Schedule multiple actions
        mock_clock.schedule(ClockAction::LeaderHeartbeat, Duration::from_millis(100));
        mock_clock.schedule(
            ClockAction::SendScout {
                ballot: crate::types::BallotNumber::new(crate::types::LeaderId::new(1)),
            },
            Duration::from_millis(200),
        );
        mock_clock.schedule(
            ClockAction::ReproposePendingRequests,
            Duration::from_millis(150),
        );

        // Cancel heartbeat timers
        mock_clock.cancel(&ClockAction::LeaderHeartbeat);

        // Advance time past all scheduled events
        mock_clock.advance(Duration::from_millis(300));

        let expired = mock_clock.check_timers();
        assert_eq!(expired.len(), 2); // Heartbeat should be cancelled

        // Should have SendScout and ReproposePendingRequests
        assert!(expired
            .iter()
            .any(|a| matches!(a, ClockAction::SendScout { .. })));
        assert!(expired
            .iter()
            .any(|a| matches!(a, ClockAction::ReproposePendingRequests)));
        assert!(!expired
            .iter()
            .any(|a| matches!(a, ClockAction::LeaderHeartbeat)));
    }

    #[test]
    fn test_next_timeout_calculation() {
        let mut mock_clock = MockClock::new();

        // No timers scheduled
        assert!(mock_clock.next_timeout().is_none());

        // Schedule a timer
        mock_clock.schedule(ClockAction::LeaderHeartbeat, Duration::from_millis(100));

        // Should return approximately 100ms
        let timeout = mock_clock.next_timeout().unwrap();
        assert!(timeout <= Duration::from_millis(100));
        assert!(timeout > Duration::from_millis(90));

        // Advance time
        mock_clock.advance(Duration::from_millis(50));

        // Should return approximately 50ms
        let timeout = mock_clock.next_timeout().unwrap();
        assert!(timeout <= Duration::from_millis(50));
        assert!(timeout > Duration::from_millis(40));
    }

    #[test]
    fn test_system_clock_basic() {
        let mut clock = Clock::new(Box::new(SystemClock::new()));

        // Just test that it doesn't panic
        let _now = clock.now();
        clock.schedule(ClockAction::LeaderHeartbeat, Duration::from_millis(1));
        let _timeout = clock.next_timeout();

        // Give it a moment and check (might be flaky in fast tests)
        std::thread::sleep(Duration::from_millis(2));
        let _expired = clock.check_timers();

        // The timer might or might not have expired depending on system timing
        // This test is mainly to ensure the system clock doesn't panic
    }

    #[test]
    fn test_timer_ordering() {
        let mut mock_clock = MockClock::new();

        // Schedule actions in reverse time order
        mock_clock.schedule(
            ClockAction::Custom("third".to_string()),
            Duration::from_millis(300),
        );
        mock_clock.schedule(
            ClockAction::Custom("first".to_string()),
            Duration::from_millis(100),
        );
        mock_clock.schedule(
            ClockAction::Custom("second".to_string()),
            Duration::from_millis(200),
        );

        // Advance to first timer
        mock_clock.advance(Duration::from_millis(150));
        let expired = mock_clock.check_timers();
        assert_eq!(expired.len(), 1);
        matches!(expired[0], ClockAction::Custom(ref s) if s == "first");

        // Advance to second timer
        mock_clock.advance(Duration::from_millis(60));
        let expired = mock_clock.check_timers();
        assert_eq!(expired.len(), 1);
        matches!(expired[0], ClockAction::Custom(ref s) if s == "second");

        // Advance to third timer
        mock_clock.advance(Duration::from_millis(110));
        let expired = mock_clock.check_timers();
        assert_eq!(expired.len(), 1);
        matches!(expired[0], ClockAction::Custom(ref s) if s == "third");
    }
}
