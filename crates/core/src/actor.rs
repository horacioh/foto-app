//! Minimal actor runtime for [`StateMachine`]s.
//!
//! An actor is: a mailbox of events, the machine's current state, a
//! [`StateStore`] that persists the state after every transition, and an
//! [`EffectHandler`] that performs effects and turns their results back into
//! events. The loop itself is platform agnostic (it only needs an async
//! executor), so the same code runs under tokio on server/mobile and under
//! `wasm-bindgen-futures` on the web.

use crate::machine::StateMachine;
use tokio::sync::mpsc;

/// Performs effects. Implementations own the I/O (HTTP clients, disk, crypto,
/// timers). Returning `Some(event)` feeds the result back into the machine.
pub trait EffectHandler<M: StateMachine> {
    fn handle(&mut self, effect: M::Effect) -> impl std::future::Future<Output = Option<M::Event>>;
}

/// Persists machine state so an actor can be rehydrated after a crash.
pub trait StateStore<M: StateMachine> {
    fn save(&mut self, state: &M::State);
}

/// A store that keeps nothing. Useful for tests and for machines whose state
/// is cheap to rebuild.
pub struct NoStore;

impl<M: StateMachine> StateStore<M> for NoStore {
    fn save(&mut self, _state: &M::State) {}
}

/// Handle used to send events to a running actor.
pub struct Mailbox<M: StateMachine> {
    tx: mpsc::Sender<M::Event>,
}

impl<M: StateMachine> Clone for Mailbox<M> {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}

impl<M: StateMachine> Mailbox<M> {
    pub async fn send(&self, event: M::Event) -> Result<(), M::Event> {
        self.tx.send(event).await.map_err(|e| e.0)
    }

    pub fn try_send(&self, event: M::Event) -> Result<(), M::Event> {
        self.tx.try_send(event).map_err(|e| match e {
            mpsc::error::TrySendError::Full(ev) | mpsc::error::TrySendError::Closed(ev) => ev,
        })
    }
}

pub struct Actor<M, H, S>
where
    M: StateMachine,
    H: EffectHandler<M>,
    S: StateStore<M>,
{
    state: M::State,
    rx: mpsc::Receiver<M::Event>,
    handler: H,
    store: S,
}

impl<M, H, S> Actor<M, H, S>
where
    M: StateMachine,
    H: EffectHandler<M>,
    S: StateStore<M>,
{
    /// Creates an actor and its mailbox. The caller is responsible for
    /// spawning [`Actor::run`] on whatever executor the platform provides.
    pub fn new(initial: M::State, handler: H, store: S, capacity: usize) -> (Self, Mailbox<M>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { state: initial, rx, handler, store }, Mailbox { tx })
    }

    pub fn state(&self) -> &M::State {
        &self.state
    }

    /// Applies one event, persists the new state, executes the resulting
    /// effects and immediately applies any events they produce, so a single
    /// external event can drive a whole chain of effects.
    pub async fn step(&mut self, event: M::Event) {
        let step = M::transition(self.state.clone(), event);
        if step.state != self.state {
            self.store.save(&step.state);
            self.state = step.state;
        }
        for effect in step.effects {
            if let Some(event) = self.handler.handle(effect).await {
                Box::pin(self.step(event)).await;
            }
        }
    }

    /// Drives the actor until the mailbox closes or the machine terminates.
    pub async fn run(mut self) -> M::State {
        while !M::is_terminal(&self.state) {
            match self.rx.recv().await {
                Some(event) => {
                    self.step(event).await;
                }
                None => break,
            }
        }
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Step;

    struct Counter;

    #[derive(Debug, Clone, PartialEq)]
    struct CounterState {
        n: u32,
    }

    #[derive(Debug)]
    enum CounterEvent {
        Inc,
        Doubled(u32),
    }

    #[derive(Debug, PartialEq)]
    enum CounterEffect {
        Double(u32),
    }

    impl StateMachine for Counter {
        type State = CounterState;
        type Event = CounterEvent;
        type Effect = CounterEffect;

        fn transition(
            state: CounterState,
            event: CounterEvent,
        ) -> Step<CounterState, CounterEffect> {
            match event {
                CounterEvent::Inc => {
                    let n = state.n + 1;
                    Step::to(CounterState { n }, vec![CounterEffect::Double(n)])
                }
                CounterEvent::Doubled(v) => Step::stay(CounterState { n: v }),
            }
        }

        fn is_terminal(state: &CounterState) -> bool {
            state.n >= 100
        }
    }

    struct Doubler;

    impl EffectHandler<Counter> for Doubler {
        async fn handle(&mut self, effect: CounterEffect) -> Option<CounterEvent> {
            match effect {
                CounterEffect::Double(v) => Some(CounterEvent::Doubled(v * 2)),
            }
        }
    }

    struct Recording(Vec<u32>);

    impl StateStore<Counter> for Recording {
        fn save(&mut self, state: &CounterState) {
            self.0.push(state.n);
        }
    }

    #[tokio::test]
    async fn effects_feed_back_and_state_is_persisted() {
        let (mut actor, _mailbox) =
            Actor::<Counter, _, _>::new(CounterState { n: 0 }, Doubler, Recording(vec![]), 8);
        actor.step(CounterEvent::Inc).await;
        assert_eq!(actor.state().n, 2);
        actor.step(CounterEvent::Inc).await;
        assert_eq!(actor.state().n, 6);
        assert_eq!(actor.store.0, vec![1, 2, 3, 6]);
    }

    #[tokio::test]
    async fn run_stops_at_terminal_state() {
        let (actor, mailbox) =
            Actor::<Counter, _, _>::new(CounterState { n: 60 }, Doubler, NoStore, 8);
        let handle = tokio::spawn(actor.run());
        mailbox.send(CounterEvent::Inc).await.unwrap();
        let final_state = handle.await.unwrap();
        assert_eq!(final_state.n, 122);
        assert!(mailbox.send(CounterEvent::Inc).await.is_err());
    }
}
