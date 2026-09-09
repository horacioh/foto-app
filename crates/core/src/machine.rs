//! The state machine contract every piece of domain logic implements.
//!
//! `transition` must be pure: no I/O, no clock, no randomness. Anything the
//! machine needs from the outside world arrives as an [`StateMachine::Event`];
//! anything it wants done is returned as an [`StateMachine::Effect`] for the
//! [`crate::actor`] runtime to execute. This keeps machines trivially unit
//! testable and lets them run unchanged on native, web and server.

/// A pure, typed state machine.
pub trait StateMachine {
    /// Persisted after every transition by the actor runtime.
    type State: Clone + std::fmt::Debug + PartialEq;
    /// Inputs: user intents, effect results, timer ticks.
    type Event: std::fmt::Debug;
    /// Outputs: things the runtime must do on the machine's behalf.
    type Effect: std::fmt::Debug + PartialEq;

    fn transition(state: Self::State, event: Self::Event) -> Step<Self::State, Self::Effect>;

    /// Whether the machine has reached a state it will never leave. The
    /// runtime uses this to stop the actor and release its mailbox.
    fn is_terminal(_state: &Self::State) -> bool {
        false
    }
}

/// Result of one transition.
#[derive(Debug, Clone, PartialEq)]
pub struct Step<S, E> {
    pub state: S,
    pub effects: Vec<E>,
}

impl<S, E> Step<S, E> {
    pub fn stay(state: S) -> Self {
        Self { state, effects: Vec::new() }
    }

    pub fn to(state: S, effects: impl Into<Vec<E>>) -> Self {
        Self { state, effects: effects.into() }
    }

    pub fn with(mut self, effect: E) -> Self {
        self.effects.push(effect);
        self
    }
}

/// Exponential backoff shared by every machine that retries network work.
/// Returns milliseconds. Capped so a long outage does not turn into hours.
pub fn backoff_ms(attempt: u32) -> u64 {
    const BASE: u64 = 1_000;
    const CAP: u64 = 5 * 60 * 1_000;
    BASE.saturating_mul(1u64 << attempt.min(16)).min(CAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff_ms(0), 1_000);
        assert_eq!(backoff_ms(1), 2_000);
        assert_eq!(backoff_ms(3), 8_000);
        assert_eq!(backoff_ms(30), 300_000);
    }
}
