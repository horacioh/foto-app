//! Coordinator (ARCHITECTURE.md §2, §5, §6, §8, §10).
//!
//! The coordinator never sees plaintext. It owns:
//! - accounts, devices and *encrypted* key bundles;
//! - per-album append-only change feeds (opaque payloads, verified signatures);
//! - the storage node registry and upload grants (owner-pays accounting);
//! - quotas and, behind the `billing` feature, Stripe plans.
//!
//! Phase 0 ships the HTTP surface with every domain route answering
//! `501 Not Implemented` and a JSON body naming the phase that delivers it,
//! so clients can be built against the real paths from day one.

mod routes;

use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Config {
    pub public_url: String,
    pub database_url: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self { config: Arc::new(config) }
    }
}

pub use routes::router;
