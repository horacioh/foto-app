//! Wire types shared by every component. Anything the server can read is here;
//! anything encrypted is carried as an opaque [`Ciphertext`].
//!
//! Keep this crate dependency-light: it is compiled into the mobile apps
//! (via `photos-core-uniffi`) and to WASM (via `photos-core-wasm`).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(AccountId);
id_type!(DeviceId);
id_type!(AlbumId);
id_type!(AssetId);
id_type!(StorageNodeId);
id_type!(GrantId);

/// BLAKE3 hash of an encrypted blob. Blobs are content addressed by their
/// ciphertext so the storage node can verify integrity without keys.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlobId(pub [u8; 32]);

impl BlobId {
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hi = (chunk[0] as char).to_digit(16)?;
            let lo = (chunk[1] as char).to_digit(16)?;
            out[i] = (hi * 16 + lo) as u8;
        }
        Some(Self(out))
    }
}

impl std::fmt::Debug for BlobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlobId({})", &self.to_hex()[..12])
    }
}

impl std::fmt::Display for BlobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Opaque encrypted bytes. The server stores and forwards these, never reads them.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Ciphertext(pub Vec<u8>);

impl std::fmt::Debug for Ciphertext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ciphertext({} bytes)", self.0.len())
    }
}

/// Size of a plaintext chunk. Each chunk is sealed independently so uploads
/// resume per chunk and downloads decrypt progressively.
pub const CHUNK_SIZE: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Owner,
    Contributor,
    Viewer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlbumKind {
    Manual,
    Smart,
    /// `library`, `recents`, ... Never shareable except `recents`.
    Builtin,
}

/// One entry in an album's append-only change feed. `payload` is encrypted
/// with the album key; `signature` is by the device's Ed25519 key over
/// `(album_id, seq_hint, payload)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedEntry {
    pub album_id: AlbumId,
    /// Assigned by the server. `None` before the entry has been accepted.
    pub seq: Option<u64>,
    pub device_id: DeviceId,
    pub payload: Ciphertext,
    pub signature: Vec<u8>,
}

/// A coordinator-signed permission for a client to upload up to `max_bytes`
/// into `album_id` on `storage_node_id`, charged to the album owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadGrant {
    pub id: GrantId,
    pub album_id: AlbumId,
    pub storage_node_id: StorageNodeId,
    pub grantee: AccountId,
    pub max_bytes: u64,
    /// Unix seconds.
    pub expires_at: u64,
    /// Ed25519 signature by the coordinator over the canonical encoding of
    /// the fields above.
    pub signature: Vec<u8>,
}

/// Which chunks of a blob a storage node already holds. Returned by
/// `HEAD /blobs/{id}` so clients can resume.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BlobManifest {
    pub total_chunks: Option<u32>,
    pub present_chunks: Vec<u32>,
    pub complete: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_id_hex_roundtrip() {
        let id = BlobId([7u8; 32]);
        assert_eq!(BlobId::from_hex(&id.to_hex()), Some(id));
        assert_eq!(BlobId::from_hex("zz"), None);
    }
}
