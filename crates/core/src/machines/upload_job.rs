//! `UploadJob`: takes one local asset from "discovered on this device" to
//! "encrypted original is durable on the album owner's storage node and the
//! feed entry referencing it has been accepted".
//!
//! ```text
//! Discovered ─Start─▶ Hashing ─Hashed─▶ Encrypting(i) ─ChunkSealed─▶ … ─AllSealed─▶
//!   RequestingGrant ─GrantIssued─▶ Uploading(i, attempt) ─ChunkUploaded─▶ … ─▶
//!   Committing ─Committed─▶ Done
//!
//! Uploading ─ChunkFailed(transient)─▶ Uploading(i, attempt+1) after ScheduleRetry
//! Uploading ─ChunkFailed(permanent)─▶ Failed
//! any non-terminal ─Cancel─▶ Failed(Cancelled)
//! ```
//!
//! Everything slow (hashing, sealing, HTTP) is an effect executed by the
//! runtime; the machine only tracks progress, so it is fully unit tested
//! without touching a file or a socket.

use crate::machine::{backoff_ms, StateMachine, Step};
use photos_protocol::{AlbumId, AssetId, BlobId, StorageNodeId, UploadGrant};

pub const MAX_CHUNK_ATTEMPTS: u32 = 8;

pub struct UploadJob;

/// Immutable facts about the job, carried through every state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobInfo {
    pub asset_id: AssetId,
    pub album_id: AlbumId,
    pub storage_node_id: StorageNodeId,
    pub local_uri: String,
    pub plaintext_size: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Discovered {
        info: JobInfo,
    },
    Hashing {
        info: JobInfo,
    },
    Encrypting {
        info: JobInfo,
        content_hash: BlobId,
        total_chunks: u32,
        next_chunk: u32,
    },
    RequestingGrant {
        info: JobInfo,
        blob_id: BlobId,
        total_chunks: u32,
        sealed_size: u64,
    },
    Uploading {
        info: JobInfo,
        blob_id: BlobId,
        grant: UploadGrant,
        total_chunks: u32,
        next_chunk: u32,
        attempt: u32,
    },
    Committing {
        info: JobInfo,
        blob_id: BlobId,
    },
    Done {
        info: JobInfo,
        blob_id: BlobId,
    },
    Failed {
        info: JobInfo,
        reason: FailReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailReason {
    Cancelled,
    HashFailed(String),
    SealFailed(String),
    GrantDenied(String),
    /// A chunk failed with a non-transient error, or ran out of retries.
    UploadFailed {
        chunk: u32,
        message: String,
    },
    CommitRejected(String),
}

#[derive(Debug)]
pub enum Event {
    Start,
    Hashed {
        content_hash: BlobId,
        total_chunks: u32,
    },
    HashFailed(String),
    ChunkSealed {
        index: u32,
    },
    SealFailed(String),
    /// The runtime has sealed every chunk and computed the ciphertext id.
    AllSealed {
        blob_id: BlobId,
        sealed_size: u64,
    },
    GrantIssued(UploadGrant),
    GrantDenied(String),
    ChunkUploaded {
        index: u32,
    },
    ChunkFailed {
        index: u32,
        transient: bool,
        message: String,
    },
    RetryTimerFired,
    Committed,
    CommitRejected(String),
    Cancel,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    HashFile {
        local_uri: String,
    },
    SealChunk {
        index: u32,
        last: bool,
    },
    RequestGrant {
        album_id: AlbumId,
        storage_node_id: StorageNodeId,
        bytes: u64,
    },
    UploadChunk {
        blob_id: BlobId,
        index: u32,
        grant_id: photos_protocol::GrantId,
    },
    ScheduleRetry {
        after_ms: u64,
    },
    /// Push the `AssetAdded` feed entry referencing the durable blob.
    Commit {
        asset_id: AssetId,
        blob_id: BlobId,
    },
    /// Remove sealed temp chunks for this asset.
    Cleanup {
        asset_id: AssetId,
    },
}

impl State {
    pub fn info(&self) -> &JobInfo {
        match self {
            State::Discovered { info }
            | State::Hashing { info }
            | State::Encrypting { info, .. }
            | State::RequestingGrant { info, .. }
            | State::Uploading { info, .. }
            | State::Committing { info, .. }
            | State::Done { info, .. }
            | State::Failed { info, .. } => info,
        }
    }

    fn fail(&self, reason: FailReason) -> Step<State, Effect> {
        fail_with(self.info().clone(), reason)
    }
}

fn fail_with(info: JobInfo, reason: FailReason) -> Step<State, Effect> {
    let asset_id = info.asset_id;
    Step::to(State::Failed { info, reason }, vec![Effect::Cleanup { asset_id }])
}

impl StateMachine for UploadJob {
    type State = State;
    type Event = Event;
    type Effect = Effect;

    fn is_terminal(state: &State) -> bool {
        matches!(state, State::Done { .. } | State::Failed { .. })
    }

    fn transition(state: State, event: Event) -> Step<State, Effect> {
        use State as S;

        if let Event::Cancel = event {
            return if Self::is_terminal(&state) {
                Step::stay(state)
            } else {
                state.fail(FailReason::Cancelled)
            };
        }

        match (state, event) {
            (S::Discovered { info }, Event::Start) => {
                let local_uri = info.local_uri.clone();
                Step::to(S::Hashing { info }, vec![Effect::HashFile { local_uri }])
            }

            (S::Hashing { info }, Event::Hashed { content_hash, total_chunks }) => {
                let total_chunks = total_chunks.max(1);
                Step::to(
                    S::Encrypting { info, content_hash, total_chunks, next_chunk: 0 },
                    vec![Effect::SealChunk { index: 0, last: total_chunks == 1 }],
                )
            }
            (s @ S::Hashing { .. }, Event::HashFailed(msg)) => s.fail(FailReason::HashFailed(msg)),

            (
                S::Encrypting { info, content_hash, total_chunks, next_chunk },
                Event::ChunkSealed { index },
            ) if index == next_chunk => {
                let next = next_chunk + 1;
                let mut step = Step::stay(S::Encrypting {
                    info,
                    content_hash,
                    total_chunks,
                    next_chunk: next,
                });
                if next < total_chunks {
                    step = step
                        .with(Effect::SealChunk { index: next, last: next + 1 == total_chunks });
                }
                step
            }
            (
                S::Encrypting { info, total_chunks, next_chunk, .. },
                Event::AllSealed { blob_id, sealed_size },
            ) if next_chunk == total_chunks => {
                let effect = Effect::RequestGrant {
                    album_id: info.album_id,
                    storage_node_id: info.storage_node_id,
                    bytes: sealed_size,
                };
                Step::to(
                    S::RequestingGrant { info, blob_id, total_chunks, sealed_size },
                    vec![effect],
                )
            }
            (s @ S::Encrypting { .. }, Event::SealFailed(msg)) => {
                s.fail(FailReason::SealFailed(msg))
            }

            (S::RequestingGrant { info, blob_id, total_chunks, .. }, Event::GrantIssued(grant)) => {
                let effect = Effect::UploadChunk { blob_id, index: 0, grant_id: grant.id };
                Step::to(
                    S::Uploading { info, blob_id, grant, total_chunks, next_chunk: 0, attempt: 0 },
                    vec![effect],
                )
            }
            (s @ S::RequestingGrant { .. }, Event::GrantDenied(msg)) => {
                s.fail(FailReason::GrantDenied(msg))
            }

            (
                S::Uploading { info, blob_id, grant, total_chunks, next_chunk, .. },
                Event::ChunkUploaded { index },
            ) if index == next_chunk => {
                let next = next_chunk + 1;
                if next == total_chunks {
                    let asset_id = info.asset_id;
                    Step::to(
                        S::Committing { info, blob_id },
                        vec![Effect::Commit { asset_id, blob_id }],
                    )
                } else {
                    let effect = Effect::UploadChunk { blob_id, index: next, grant_id: grant.id };
                    Step::to(
                        S::Uploading {
                            info,
                            blob_id,
                            grant,
                            total_chunks,
                            next_chunk: next,
                            attempt: 0,
                        },
                        vec![effect],
                    )
                }
            }
            (
                S::Uploading { info, blob_id, grant, total_chunks, next_chunk, attempt },
                Event::ChunkFailed { index, transient, message },
            ) if index == next_chunk => {
                let attempt = attempt + 1;
                if !transient || attempt >= MAX_CHUNK_ATTEMPTS {
                    return fail_with(info, FailReason::UploadFailed { chunk: index, message });
                }
                Step::to(
                    S::Uploading { info, blob_id, grant, total_chunks, next_chunk, attempt },
                    vec![Effect::ScheduleRetry { after_ms: backoff_ms(attempt) }],
                )
            }
            (
                S::Uploading { info, blob_id, grant, total_chunks, next_chunk, attempt },
                Event::RetryTimerFired,
            ) => {
                let effect = Effect::UploadChunk { blob_id, index: next_chunk, grant_id: grant.id };
                Step::to(
                    S::Uploading { info, blob_id, grant, total_chunks, next_chunk, attempt },
                    vec![effect],
                )
            }

            (S::Committing { info, blob_id }, Event::Committed) => {
                let asset_id = info.asset_id;
                Step::to(S::Done { info, blob_id }, vec![Effect::Cleanup { asset_id }])
            }
            (s @ S::Committing { .. }, Event::CommitRejected(msg)) => {
                s.fail(FailReason::CommitRejected(msg))
            }

            // Stale or out-of-order events (e.g. a late ChunkUploaded after a
            // retry was scheduled) are ignored rather than corrupting state.
            (state, event) => {
                tracing::debug!(?state, ?event, "ignored event");
                Step::stay(state)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photos_protocol::GrantId;

    fn info() -> JobInfo {
        JobInfo {
            asset_id: AssetId::new(),
            album_id: AlbumId::new(),
            storage_node_id: StorageNodeId::new(),
            local_uri: "ph://ABC".into(),
            plaintext_size: 10 * 1024 * 1024,
        }
    }

    fn grant(info: &JobInfo) -> UploadGrant {
        UploadGrant {
            id: GrantId::new(),
            album_id: info.album_id,
            storage_node_id: info.storage_node_id,
            grantee: photos_protocol::AccountId::new(),
            max_bytes: 100,
            expires_at: 0,
            signature: vec![],
        }
    }

    fn apply(state: State, events: Vec<Event>) -> (State, Vec<Effect>) {
        let mut effects = Vec::new();
        let mut state = state;
        for e in events {
            let step = UploadJob::transition(state, e);
            state = step.state;
            effects.extend(step.effects);
        }
        (state, effects)
    }

    #[test]
    fn happy_path_three_chunks() {
        let info = info();
        let hash = BlobId([1; 32]);
        let blob = BlobId([2; 32]);
        let g = grant(&info);

        let (state, effects) = apply(
            State::Discovered { info: info.clone() },
            vec![
                Event::Start,
                Event::Hashed { content_hash: hash, total_chunks: 3 },
                Event::ChunkSealed { index: 0 },
                Event::ChunkSealed { index: 1 },
                Event::ChunkSealed { index: 2 },
                Event::AllSealed { blob_id: blob, sealed_size: 99 },
                Event::GrantIssued(g.clone()),
                Event::ChunkUploaded { index: 0 },
                Event::ChunkUploaded { index: 1 },
                Event::ChunkUploaded { index: 2 },
                Event::Committed,
            ],
        );

        assert_eq!(state, State::Done { info: info.clone(), blob_id: blob });
        assert_eq!(
            effects,
            vec![
                Effect::HashFile { local_uri: "ph://ABC".into() },
                Effect::SealChunk { index: 0, last: false },
                Effect::SealChunk { index: 1, last: false },
                Effect::SealChunk { index: 2, last: true },
                Effect::RequestGrant {
                    album_id: info.album_id,
                    storage_node_id: info.storage_node_id,
                    bytes: 99
                },
                Effect::UploadChunk { blob_id: blob, index: 0, grant_id: g.id },
                Effect::UploadChunk { blob_id: blob, index: 1, grant_id: g.id },
                Effect::UploadChunk { blob_id: blob, index: 2, grant_id: g.id },
                Effect::Commit { asset_id: info.asset_id, blob_id: blob },
                Effect::Cleanup { asset_id: info.asset_id },
            ]
        );
        assert!(UploadJob::is_terminal(&state));
    }

    fn uploading(info: &JobInfo) -> State {
        State::Uploading {
            info: info.clone(),
            blob_id: BlobId([2; 32]),
            grant: grant(info),
            total_chunks: 2,
            next_chunk: 1,
            attempt: 0,
        }
    }

    #[test]
    fn transient_failure_backs_off_then_retries_same_chunk() {
        let info = info();
        let (state, effects) = apply(
            uploading(&info),
            vec![
                Event::ChunkFailed { index: 1, transient: true, message: "timeout".into() },
                Event::RetryTimerFired,
            ],
        );
        let State::Uploading { next_chunk, attempt, grant, .. } = &state else {
            panic!("{state:?}")
        };
        assert_eq!((*next_chunk, *attempt), (1, 1));
        assert_eq!(
            effects,
            vec![
                Effect::ScheduleRetry { after_ms: backoff_ms(1) },
                Effect::UploadChunk { blob_id: BlobId([2; 32]), index: 1, grant_id: grant.id },
            ]
        );
    }

    #[test]
    fn permanent_failure_or_exhausted_retries_fail_the_job() {
        let info = info();
        let (state, effects) = apply(
            uploading(&info),
            vec![Event::ChunkFailed { index: 1, transient: false, message: "403".into() }],
        );
        assert_eq!(
            state,
            State::Failed {
                info: info.clone(),
                reason: FailReason::UploadFailed { chunk: 1, message: "403".into() }
            }
        );
        assert_eq!(effects, vec![Effect::Cleanup { asset_id: info.asset_id }]);

        let mut events: Vec<Event> = Vec::new();
        for _ in 0..MAX_CHUNK_ATTEMPTS {
            events.push(Event::ChunkFailed { index: 1, transient: true, message: "flaky".into() });
            events.push(Event::RetryTimerFired);
        }
        let (state, _) = apply(uploading(&info), events);
        assert!(matches!(
            state,
            State::Failed { reason: FailReason::UploadFailed { chunk: 1, .. }, .. }
        ));
    }

    #[test]
    fn stale_events_are_ignored() {
        let info = info();
        let start = uploading(&info);
        let (state, effects) = apply(
            start.clone(),
            vec![
                Event::ChunkUploaded { index: 0 },
                Event::ChunkSealed { index: 7 },
                Event::Hashed { content_hash: BlobId([9; 32]), total_chunks: 1 },
            ],
        );
        assert_eq!(state, start);
        assert!(effects.is_empty());
    }

    #[test]
    fn cancel_from_any_live_state_cleans_up_and_is_idempotent() {
        let info = info();
        let (state, effects) = apply(State::Hashing { info: info.clone() }, vec![Event::Cancel]);
        assert_eq!(state, State::Failed { info: info.clone(), reason: FailReason::Cancelled });
        assert_eq!(effects, vec![Effect::Cleanup { asset_id: info.asset_id }]);

        let done = State::Done { info: info.clone(), blob_id: BlobId([2; 32]) };
        let (state, effects) = apply(done.clone(), vec![Event::Cancel]);
        assert_eq!(state, done);
        assert!(effects.is_empty());
    }

    #[test]
    fn grant_denied_fails() {
        let info = info();
        let (state, _) = apply(
            State::RequestingGrant {
                info: info.clone(),
                blob_id: BlobId([2; 32]),
                total_chunks: 1,
                sealed_size: 10,
            },
            vec![Event::GrantDenied("quota exceeded".into())],
        );
        assert_eq!(
            state,
            State::Failed { info, reason: FailReason::GrantDenied("quota exceeded".into()) }
        );
    }
}
