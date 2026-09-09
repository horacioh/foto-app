//! Upload grant verification (ARCHITECTURE.md §8).
//!
//! Clients never hold node credentials. The coordinator signs an
//! [`UploadGrant`] naming the album, node, grantee and byte budget; the node
//! checks the signature against the coordinator's public key. Signature
//! verification (Ed25519) arrives with the coordinator in phase 2; until then
//! the node runs with [`AllowAll`] for local development.

use photos_protocol::UploadGrant;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GrantError {
    #[error("grant missing")]
    Missing,
    #[error("grant malformed")]
    Malformed,
    #[error("grant expired")]
    Expired,
    #[error("grant is for a different node")]
    WrongNode,
    #[error("grant exhausted")]
    Exhausted,
    #[error("bad signature")]
    BadSignature,
}

pub trait GrantVerifier: Send + Sync + 'static {
    /// Verifies that `grant` authorises writing `bytes` on this node.
    fn verify(&self, grant: &UploadGrant, bytes: u64) -> Result<(), GrantError>;
}

/// Development-only verifier: accepts any well-formed grant.
pub struct AllowAll;

impl GrantVerifier for AllowAll {
    fn verify(&self, grant: &UploadGrant, bytes: u64) -> Result<(), GrantError> {
        if bytes > grant.max_bytes {
            return Err(GrantError::Exhausted);
        }
        Ok(())
    }
}
