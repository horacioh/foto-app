//! Concrete state machines (ARCHITECTURE.md §7).
//!
//! Each module is one machine: a `State` enum, an `Event` enum, an `Effect`
//! enum and a [`crate::machine::StateMachine`] impl with unit tests. Machines
//! never import I/O crates.
//!
//! Implemented:
//! - [`upload_job`]: reference machine, one per asset being uploaded.
//!
//! Planned (see the supervision tree in the architecture doc):
//! `account_session`, `feed_sync`, `library_scanner`, `download_job`,
//! `storage_evictor`, `smart_album_materializer`, `key_rotator`,
//! `ml_indexer`, `importer`; server side `grant_issuer`, `node_health`,
//! `replication`, `blob_gc`.

pub mod upload_job;
