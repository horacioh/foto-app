//! Storage node library (ARCHITECTURE.md §8).
//!
//! HTTP surface:
//!
//! | Method | Path | Purpose |
//! |--------|------|---------|
//! | GET    | `/health` | liveness + backend usage |
//! | HEAD   | `/blobs/{id}` | manifest of present chunks (resume) |
//! | PUT    | `/blobs/{id}/chunks/{n}?total={t}` | store one sealed chunk, requires a grant |
//! | GET    | `/blobs/{id}` | stream the whole blob (all chunks, in order) |
//! | DELETE | `/blobs/{id}` | remove a blob, requires a grant |
//!
//! Blob ids are BLAKE3 of the full ciphertext; the node verifies the hash
//! once every chunk has arrived and refuses to mark the blob complete if it
//! does not match, so a client can never poison another client's cache.

pub mod backend;
pub mod grant;
mod routes;

use backend::Backend;
use grant::GrantVerifier;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub backend: Arc<dyn Backend>,
    pub grants: Arc<dyn GrantVerifier>,
}

pub use routes::router;
