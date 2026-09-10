//! Asset-level types that live *inside* the encrypted metadata blob
//! (`docs/ARCHITECTURE.md` §5 "Deduplication"). Nothing here is ever sent to
//! the coordinator in the clear; the server only sees [`photos_protocol::BlobId`]s,
//! which hash ciphertext under per-asset keys and therefore never collide for
//! the same photo uploaded twice.

use photos_protocol::{AssetId, BlobId};
use serde::{Deserialize, Serialize};

/// BLAKE3 of the *plaintext* original. Two files with the same `ContentHash`
/// are byte-identical and are deduplicated on device before any encryption
/// or upload happens. Distinct from [`BlobId`], which hashes the ciphertext.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    pub fn of(plaintext: &[u8]) -> Self {
        Self(*blake3::hash(plaintext).as_bytes())
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl std::fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContentHash({})", &self.to_hex()[..12])
    }
}

/// 64-bit perceptual hash (dHash/pHash for images; for video, the hash of the
/// poster frame) used to surface *near* duplicates — bursts, edited copies,
/// re-encoded messenger downloads. Near duplicates are only ever suggested to
/// the user via the built-in `Duplicates` smart album, never merged silently.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PerceptualHash(pub u64);

impl PerceptualHash {
    /// Hamming distance ≤ this is treated as "looks the same".
    pub const NEAR_THRESHOLD: u32 = 10;

    pub fn distance(self, other: Self) -> u32 {
        (self.0 ^ other.0).count_ones()
    }

    pub fn is_near(self, other: Self) -> bool {
        self.distance(other) <= Self::NEAR_THRESHOLD
    }
}

impl std::fmt::Debug for PerceptualHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PerceptualHash({:016x})", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Photo,
    Video,
    Live,
}

/// Plaintext of an asset's metadata blob, encrypted with `K_asset`. Members of
/// an album that contains the asset can decrypt it and index it locally.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssetMeta {
    pub asset_id: AssetId,
    pub kind: AssetKind,
    pub content_hash: ContentHash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perceptual_hash: Option<PerceptualHash>,
    pub original_blob: BlobId,
    pub plaintext_size: u64,
    /// Unix milliseconds.
    pub captured_at: i64,
    pub width: u32,
    pub height: u32,
    /// Milliseconds; `None` for stills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exif: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_deterministic_and_content_sensitive() {
        let a = ContentHash::of(b"same photo");
        assert_eq!(a, ContentHash::of(b"same photo"));
        assert_ne!(a, ContentHash::of(b"same photo, edited"));
    }

    /// The same original uploaded twice (fresh `K_asset` each time) yields two
    /// different blob ids but one content hash — so dedup must key on the
    /// latter, and the server cannot dedup at all.
    #[test]
    fn content_hash_is_stable_while_blob_id_is_not() {
        use crate::crypto::{blob_id, seal_chunk, AssetKey, BlobRole};
        let plaintext = b"an original file";
        let asset = AssetId::new();
        let sealed_once =
            seal_chunk(&AssetKey::generate(), asset, BlobRole::Original, 0, true, plaintext);
        let sealed_twice =
            seal_chunk(&AssetKey::generate(), asset, BlobRole::Original, 0, true, plaintext);
        assert_ne!(blob_id([&sealed_once]), blob_id([&sealed_twice]));
        assert_eq!(ContentHash::of(plaintext), ContentHash::of(plaintext));
        assert_ne!(ContentHash::of(plaintext).0, blob_id([&sealed_once]).0);
    }

    #[test]
    fn perceptual_distance_and_threshold() {
        let a = PerceptualHash(0b1111_0000);
        let b = PerceptualHash(0b1111_0011);
        assert_eq!(a.distance(b), 2);
        assert!(a.is_near(b));
        assert!(!a.is_near(PerceptualHash(!a.0)));
    }

    #[test]
    fn meta_round_trips_through_json() {
        let meta = AssetMeta {
            asset_id: AssetId::new(),
            kind: AssetKind::Photo,
            content_hash: ContentHash::of(b"x"),
            perceptual_hash: Some(PerceptualHash(42)),
            original_blob: BlobId([7; 32]),
            plaintext_size: 1,
            captured_at: 1_700_000_000_000,
            width: 4032,
            height: 3024,
            duration_ms: None,
            exif: None,
            caption: None,
            tags: vec!["beach".into()],
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("duration_ms"));
        assert_eq!(serde_json::from_str::<AssetMeta>(&json).unwrap(), meta);
    }
}
