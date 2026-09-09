//! `photos-core`: everything that is not UI and not a database.
//!
//! The crate is organised around one idea (see `docs/ARCHITECTURE.md` §7):
//! every piece of domain logic is a pure [`machine::StateMachine`], and the
//! [`actor`] runtime is the only place that performs I/O. The same crate is
//! linked into the iOS/Android apps (via `photos-core-uniffi`), the web app
//! (via `photos-core-wasm`), the coordinator and the storage node.

pub mod actor;
pub mod asset;
pub mod crypto;
pub mod machine;
pub mod machines;

pub use photos_protocol as protocol;

/// Semantic version of the core, exposed to every binding so the UI can show
/// which core it is talking to.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
