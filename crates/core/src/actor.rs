//! Minimal actor runtime for [`StateMachine`]s.
//!
//! An actor is: a mailbox of events, the machine's current state, an outbox
//! of effects not yet executed, a [`StateStore`] that persists state and
//! outbox together after every change, and an [`EffectHandler`] that performs
//! effects and turns their results back into events. The loop itself is
//! platform agnostic (it only needs an async executor), so the same code runs
//! under tokio on server/mobile and under `wasm-bindgen-futures` on the web.
//!
//! Crash safety: state and outbox are checkpointed as one unit before any
//! effect runs and again after each effect completes. Rehydrating with
//! [`Actor::resume`] replays whatever was still in the outbox, so a process
//! dying mid-effect never strands a job. Effects may therefore run more than
//! once and handlers must be idempotent (content-addressed uploads are).

use crate::machine::StateMachine;
use tokio::sync::mpsc;

/// Performs effects. Implementations own the I/O (HTTP clients, disk, crypto,
/// timers). Returning `Some(event)` feeds the result back into the machine.
/// An effect may be replayed after a crash, so handlers must be idempotent.
pub trait EffectHandler<M: StateMachine> {
    fn handle(&mut self, effect: M::Effect) -> impl std::future::Future<Output = Option<M::Event>>;
}

/// Persists machine state plus the outbox of effects still to run, as one
/// atomic write, so an actor can be rehydrated after a crash via
/// [`Actor::resume`] without losing in-flight work.
pub trait StateStore<M: StateMachine> {
    fn save(&mut self, state: &M::State, pending: &[M::Effect]);
}

/// A store that keeps nothing. Useful for tests and for machines whose state
/// is cheap to rebuild.
pub struct NoStore;

impl<M: StateMachine> StateStore<M> for NoStore {
    fn save(&mut self, _state: &M::State, _pending: &[M::Effect]) {}
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
    /// Outbox: effects emitted by transitions but not yet completed, in
    /// execution order.
    pending: Vec<M::Effect>,
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
        Self::resume(initial, Vec::new(), handler, store, capacity)
    }

    /// Rehydrates an actor from a checkpoint written by [`StateStore::save`].
    /// `pending` effects are replayed by [`Actor::run`] (or [`Actor::drain`])
    /// before any new mailbox event is processed.
    pub fn resume(
        state: M::State,
        pending: Vec<M::Effect>,
        handler: H,
        store: S,
        capacity: usize,
    ) -> (Self, Mailbox<M>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { state, pending, rx, handler, store }, Mailbox { tx })
    }

    pub fn state(&self) -> &M::State {
        &self.state
    }

    /// Effects emitted but not yet completed.
    pub fn pending(&self) -> &[M::Effect] {
        &self.pending
    }

    /// Applies one event, checkpoints state + outbox, then executes the
    /// resulting effects and immediately applies any events they produce, so
    /// a single external event can drive a whole chain of effects.
    pub async fn step(&mut self, event: M::Event) {
        self.apply(event);
        self.store.save(&self.state, &self.pending);
        self.drain().await;
    }

    /// Runs the machine's transition and queues its effects at the front of
    /// the outbox (depth first: an effect's consequences run before its
    /// siblings). Does not persist.
    fn apply(&mut self, event: M::Event) {
        let step = M::transition(self.state.clone(), event);
        self.state = step.state;
        self.pending.splice(0..0, step.effects);
    }

    /// Executes every outboxed effect. After each one completes its result
    /// event (if any) is applied and the new state + remaining outbox are
    /// checkpointed together, so a crash at any point replays at most the
    /// effect that was in flight.
    pub async fn drain(&mut self) {
        while !self.pending.is_empty() {
            let effect = self.pending.remove(0);
            if let Some(event) = self.handler.handle(effect).await {
                self.apply(event);
            }
            self.store.save(&self.state, &self.pending);
        }
    }

    /// Drives the actor until the mailbox closes or the machine terminates,
    /// replaying any outboxed effects first.
    pub async fn run(mut self) -> M::State {
        self.drain().await;
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

    #[derive(Debug, Clone, PartialEq)]
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

    /// Records every checkpoint as `(n, outboxed effects)`.
    struct Recording(Vec<(u32, Vec<CounterEffect>)>);

    impl StateStore<Counter> for Recording {
        fn save(&mut self, state: &CounterState, pending: &[CounterEffect]) {
            self.0.push((state.n, pending.to_vec()));
        }
    }

    #[tokio::test]
    async fn effects_feed_back_and_state_is_persisted_with_outbox() {
        let (mut actor, _mailbox) =
            Actor::<Counter, _, _>::new(CounterState { n: 0 }, Doubler, Recording(vec![]), 8);
        actor.step(CounterEvent::Inc).await;
        assert_eq!(actor.state().n, 2);
        actor.step(CounterEvent::Inc).await;
        assert_eq!(actor.state().n, 6);
        assert_eq!(
            actor.store.0,
            vec![
                (1, vec![CounterEffect::Double(1)]),
                (2, vec![]),
                (3, vec![CounterEffect::Double(3)]),
                (6, vec![]),
            ]
        );
    }

    #[tokio::test]
    async fn resume_replays_outboxed_effects_after_crash() {
        // The checkpoint written before an effect runs is what survives if
        // the process dies while that effect is in flight.
        let (mut actor, _) =
            Actor::<Counter, _, _>::new(CounterState { n: 0 }, Doubler, Recording(vec![]), 8);
        actor.step(CounterEvent::Inc).await;
        let (n, pending) = actor.store.0.remove(0);
        assert_eq!((n, pending.as_slice()), (1, &[CounterEffect::Double(1)][..]));

        // Rehydrate from it: Double(1) is replayed and the job reaches the
        // same state it would have without the crash.
        let (mut resumed, _) =
            Actor::<Counter, _, _>::resume(CounterState { n }, pending, Doubler, NoStore, 8);
        resumed.drain().await;
        assert_eq!(resumed.state().n, 2);
        assert!(resumed.pending().is_empty());
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
