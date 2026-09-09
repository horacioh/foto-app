//! Content encryption primitives (ARCHITECTURE.md §4).
//!
//! Only the per-asset layer lives here for now: sealing plaintext chunks with
//! XChaCha20-Poly1305 and content-addressing the ciphertext with BLAKE3. Key
//! wrapping (album keys, sealed boxes, Argon2id passphrase KDF) lands in phase 1.
//!
//! Chunks are bound to their position: the nonce is derived from the chunk
//! index and the asset id is authenticated as associated data, so chunks
//! cannot be reordered or moved between assets without detection.

use chacha20poly1305::aead::{Aead, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use photos_protocol::{AssetId, BlobId};
use rand::RngCore;

pub const KEY_LEN: usize = 32;
pub const TAG_LEN: usize = 16;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("authentication failed")]
    Tamper,
}

/// Random 256-bit key that encrypts every blob belonging to one asset.
#[derive(Clone, PartialEq, Eq)]
pub struct AssetKey([u8; KEY_LEN]);

impl AssetKey {
    pub fn generate() -> Self {
        let mut k = [0u8; KEY_LEN];
        OsRng.fill_bytes(&mut k);
        Self(k)
    }

    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new((&self.0).into())
    }
}

impl std::fmt::Debug for AssetKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AssetKey(..)")
    }
}

/// Which blob of an asset a chunk belongs to. Authenticated as part of the
/// associated data so a thumbnail chunk can never be served as an original.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BlobRole {
    Original = 0,
    Preview = 1,
    Thumbnail = 2,
    Metadata = 3,
}

fn nonce_for(role: BlobRole, chunk_index: u32, last: bool) -> XNonce {
    let mut n = [0u8; 24];
    n[0] = role as u8;
    n[1] = u8::from(last);
    n[4..8].copy_from_slice(&chunk_index.to_le_bytes());
    XNonce::from(n)
}

fn aad(asset: AssetId) -> [u8; 16] {
    *asset.0.as_bytes()
}

/// Encrypts one plaintext chunk. `last` must be true only for the final
/// chunk so a truncated blob is detected.
pub fn seal_chunk(
    key: &AssetKey,
    asset: AssetId,
    role: BlobRole,
    chunk_index: u32,
    last: bool,
    plaintext: &[u8],
) -> Vec<u8> {
    key.cipher()
        .encrypt(&nonce_for(role, chunk_index, last), Payload { msg: plaintext, aad: &aad(asset) })
        .expect("XChaCha20-Poly1305 encryption is infallible for in-memory buffers")
}

pub fn open_chunk(
    key: &AssetKey,
    asset: AssetId,
    role: BlobRole,
    chunk_index: u32,
    last: bool,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    key.cipher()
        .decrypt(&nonce_for(role, chunk_index, last), Payload { msg: ciphertext, aad: &aad(asset) })
        .map_err(|_| CryptoError::Tamper)
}

/// Content address of a (possibly multi-chunk) ciphertext blob: BLAKE3 over
/// the concatenated sealed chunks in order.
pub fn blob_id(sealed_chunks: impl IntoIterator<Item = impl AsRef<[u8]>>) -> BlobId {
    let mut hasher = blake3::Hasher::new();
    for chunk in sealed_chunks {
        hasher.update(chunk.as_ref());
    }
    BlobId(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = AssetKey::generate();
        let asset = AssetId::new();
        let sealed = seal_chunk(&key, asset, BlobRole::Original, 0, true, b"hello");
        assert_eq!(sealed.len(), 5 + TAG_LEN);
        let opened = open_chunk(&key, asset, BlobRole::Original, 0, true, &sealed).unwrap();
        assert_eq!(opened, b"hello");
    }

    #[test]
    fn detects_reordering_and_role_confusion() {
        let key = AssetKey::generate();
        let asset = AssetId::new();
        let sealed = seal_chunk(&key, asset, BlobRole::Original, 3, false, b"chunk");
        assert_eq!(
            open_chunk(&key, asset, BlobRole::Original, 4, false, &sealed),
            Err(CryptoError::Tamper)
        );
        assert_eq!(
            open_chunk(&key, asset, BlobRole::Thumbnail, 3, false, &sealed),
            Err(CryptoError::Tamper)
        );
        assert_eq!(
            open_chunk(&key, asset, BlobRole::Original, 3, true, &sealed),
            Err(CryptoError::Tamper)
        );
        assert_eq!(
            open_chunk(&key, AssetId::new(), BlobRole::Original, 3, false, &sealed),
            Err(CryptoError::Tamper)
        );
    }

    #[test]
    fn blob_id_is_deterministic_over_chunks() {
        let a = blob_id([b"ab".as_slice(), b"cd".as_slice()]);
        let b = blob_id([b"abcd".as_slice()]);
        assert_eq!(a, b);
        assert_ne!(a, blob_id([b"abce".as_slice()]));
    }
}
