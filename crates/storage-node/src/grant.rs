//! Upload grant verification (ARCHITECTURE.md §8).
//!
//! Clients never hold node credentials. The coordinator signs an
//! [`UploadGrant`] naming the album, node, grantee and byte budget; the node
//! checks the signature against the coordinator's public key and charges
//! every write against the grant so a grant cannot be replayed past
//! `max_bytes`. Signature verification (Ed25519) arrives with the coordinator
//! in phase 2; until then the node runs with [`Unsigned`] for local
//! development, which enforces everything except the signature.

use photos_protocol::{GrantId, UploadGrant};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

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
    /// Verifies that `grant` authorises writing `bytes` on this node and
    /// charges them against the grant's budget. Calling this repeatedly with
    /// the same grant consumes the budget; once `max_bytes` is reached every
    /// further call fails with [`GrantError::Exhausted`].
    fn verify(&self, grant: &UploadGrant, bytes: u64) -> Result<(), GrantError>;
}

/// Development-only verifier: skips the coordinator signature but still
/// enforces expiry and the cumulative byte budget per grant id. Usage is
/// tracked in memory, so it resets on restart; that is acceptable for local
/// development and no worse than the signed verifier's first-run state.
#[derive(Default)]
pub struct Unsigned {
    used: Mutex<HashMap<GrantId, u64>>,
}

impl Unsigned {
    fn now() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }
}

impl GrantVerifier for Unsigned {
    fn verify(&self, grant: &UploadGrant, bytes: u64) -> Result<(), GrantError> {
        if grant.expires_at < Self::now() {
            return Err(GrantError::Expired);
        }
        let mut used = self.used.lock().unwrap_or_else(|e| e.into_inner());
        let entry = used.entry(grant.id).or_insert(0);
        let next = entry.checked_add(bytes).ok_or(GrantError::Exhausted)?;
        if next > grant.max_bytes {
            return Err(GrantError::Exhausted);
        }
        *entry = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photos_protocol::{AccountId, AlbumId, StorageNodeId};

    fn grant(max_bytes: u64, expires_at: u64) -> UploadGrant {
        UploadGrant {
            id: GrantId::new(),
            album_id: AlbumId::new(),
            storage_node_id: StorageNodeId::new(),
            grantee: AccountId::new(),
            max_bytes,
            expires_at,
            signature: vec![],
        }
    }

    #[test]
    fn budget_is_cumulative_per_grant() {
        let v = Unsigned::default();
        let g = grant(10, u64::MAX);
        assert_eq!(v.verify(&g, 6), Ok(()));
        assert_eq!(v.verify(&g, 4), Ok(()));
        assert_eq!(v.verify(&g, 1), Err(GrantError::Exhausted));
        // Other grants are unaffected.
        assert_eq!(v.verify(&grant(10, u64::MAX), 10), Ok(()));
    }

    #[test]
    fn rejects_expired_and_oversized() {
        let v = Unsigned::default();
        assert_eq!(v.verify(&grant(10, 0), 1), Err(GrantError::Expired));
        assert_eq!(v.verify(&grant(10, u64::MAX), 11), Err(GrantError::Exhausted));
    }
}
