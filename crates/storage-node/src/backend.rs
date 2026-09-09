//! Pluggable blob storage. `LocalDisk` ships now; `S3Compatible` and `Mirror`
//! are phase 3.

use bytes::Bytes;
use futures_util::stream::{self, Stream};
use photos_protocol::{BlobId, BlobManifest};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("blob not found")]
    NotFound,
    #[error("blob incomplete")]
    Incomplete,
    #[error("blob already complete")]
    Sealed,
    #[error("total chunk count differs from the one this blob was started with")]
    TotalMismatch,
    #[error("content hash mismatch")]
    HashMismatch,
}

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Ordered blob content, one item per stored chunk, so a download never holds
/// more than one chunk in memory.
pub type BlobStream = std::pin::Pin<Box<dyn Stream<Item = Result<Bytes, BackendError>> + Send>>;

/// Object-safe async storage backend. Uses boxed futures so `Arc<dyn Backend>`
/// works without an external async-trait crate.
pub trait Backend: Send + Sync + 'static {
    fn put_chunk(
        &self,
        id: BlobId,
        index: u32,
        total: u32,
        data: Bytes,
    ) -> BoxFuture<'_, Result<BlobManifest, BackendError>>;
    fn manifest(&self, id: BlobId) -> BoxFuture<'_, Result<BlobManifest, BackendError>>;
    /// Whole blob, chunks in order. Range support lands with video (phase 4).
    fn get(&self, id: BlobId) -> BoxFuture<'_, Result<BlobStream, BackendError>>;
    fn delete(&self, id: BlobId) -> BoxFuture<'_, Result<(), BackendError>>;
    fn used_bytes(&self) -> BoxFuture<'_, Result<u64, BackendError>>;
}

/// Layout: `<root>/<first two hex>/<blob id>/chunk.<n>` plus `complete` marker
/// once every chunk is present and the BLAKE3 of the concatenation matches `id`.
/// Blobs are content addressed, so they are immutable: `total` is fixed by the
/// first chunk written and no chunk is accepted once `complete` exists.
pub struct LocalDisk {
    root: PathBuf,
}

impl LocalDisk {
    pub async fn open(root: &Path) -> std::io::Result<Self> {
        tokio::fs::create_dir_all(root).await?;
        Ok(Self { root: root.to_path_buf() })
    }

    fn blob_dir(&self, id: BlobId) -> PathBuf {
        let hex = id.to_hex();
        self.root.join(&hex[..2]).join(hex)
    }

    async fn read_manifest(&self, dir: &Path) -> Result<BlobManifest, BackendError> {
        if !tokio::fs::try_exists(dir).await? {
            return Err(BackendError::NotFound);
        }
        let mut present = Vec::new();
        let mut total = None;
        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(n) = name.strip_prefix("chunk.") {
                if let Ok(n) = n.parse::<u32>() {
                    present.push(n);
                }
            } else if name == "total" {
                let s = tokio::fs::read_to_string(entry.path()).await?;
                total = s.trim().parse::<u32>().ok();
            }
        }
        present.sort_unstable();
        let complete = tokio::fs::try_exists(dir.join("complete")).await?;
        Ok(BlobManifest { total_chunks: total, present_chunks: present, complete })
    }

    /// Records `total` for a new blob or checks it against the recorded one.
    /// The value is written to a temp file and hard-linked into place, so the
    /// first writer wins atomically (with its content) when uploads race.
    async fn pin_total(dir: &Path, total: u32) -> Result<(), BackendError> {
        let path = dir.join("total");
        let tmp = dir.join(format!("total.{}.tmp", uuid::Uuid::new_v4()));
        tokio::fs::write(&tmp, total.to_string()).await?;
        let linked = tokio::fs::hard_link(&tmp, &path).await;
        tokio::fs::remove_file(&tmp).await?;
        match linked {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = tokio::fs::read_to_string(&path).await?;
                if existing.trim().parse::<u32>().ok() == Some(total) {
                    Ok(())
                } else {
                    Err(BackendError::TotalMismatch)
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn verify_and_seal(
        &self,
        id: BlobId,
        dir: &Path,
        total: u32,
    ) -> Result<(), BackendError> {
        let mut hasher = blake3::Hasher::new();
        for n in 0..total {
            let bytes = tokio::fs::read(dir.join(format!("chunk.{n}"))).await?;
            hasher.update(&bytes);
        }
        if hasher.finalize().as_bytes() != &id.0 {
            tokio::fs::remove_dir_all(dir).await?;
            return Err(BackendError::HashMismatch);
        }
        tokio::fs::write(dir.join("complete"), b"").await?;
        Ok(())
    }
}

impl Backend for LocalDisk {
    fn put_chunk(
        &self,
        id: BlobId,
        index: u32,
        total: u32,
        data: Bytes,
    ) -> BoxFuture<'_, Result<BlobManifest, BackendError>> {
        Box::pin(async move {
            let dir = self.blob_dir(id);
            tokio::fs::create_dir_all(&dir).await?;
            if tokio::fs::try_exists(dir.join("complete")).await? {
                return Err(BackendError::Sealed);
            }
            Self::pin_total(&dir, total).await?;
            let tmp = dir.join(format!("chunk.{index}.tmp"));
            tokio::fs::write(&tmp, &data).await?;
            tokio::fs::rename(&tmp, dir.join(format!("chunk.{index}"))).await?;

            let mut manifest = self.read_manifest(&dir).await?;
            if !manifest.complete && manifest.present_chunks.len() as u32 == total {
                self.verify_and_seal(id, &dir, total).await?;
                manifest.complete = true;
            }
            Ok(manifest)
        })
    }

    fn manifest(&self, id: BlobId) -> BoxFuture<'_, Result<BlobManifest, BackendError>> {
        Box::pin(async move { self.read_manifest(&self.blob_dir(id)).await })
    }

    fn get(&self, id: BlobId) -> BoxFuture<'_, Result<BlobStream, BackendError>> {
        Box::pin(async move {
            let dir = self.blob_dir(id);
            let manifest = self.read_manifest(&dir).await?;
            if !manifest.complete {
                return Err(BackendError::Incomplete);
            }
            let total = manifest.total_chunks.unwrap_or(0);
            let chunks = stream::unfold((dir, 0u32), move |(dir, n)| async move {
                if n >= total {
                    return None;
                }
                let item = tokio::fs::read(dir.join(format!("chunk.{n}")))
                    .await
                    .map(Bytes::from)
                    .map_err(BackendError::from);
                Some((item, (dir, n + 1)))
            });
            Ok(Box::pin(chunks) as BlobStream)
        })
    }

    fn delete(&self, id: BlobId) -> BoxFuture<'_, Result<(), BackendError>> {
        Box::pin(async move {
            let dir = self.blob_dir(id);
            match tokio::fs::remove_dir_all(&dir).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(BackendError::NotFound),
                Err(e) => Err(e.into()),
            }
        })
    }

    fn used_bytes(&self) -> BoxFuture<'_, Result<u64, BackendError>> {
        Box::pin(async move {
            let mut total = 0u64;
            let mut stack = vec![self.root.clone()];
            while let Some(dir) = stack.pop() {
                let mut entries = tokio::fs::read_dir(&dir).await?;
                while let Some(entry) = entries.next_entry().await? {
                    let meta = entry.metadata().await?;
                    if meta.is_dir() {
                        stack.push(entry.path());
                    } else {
                        total += meta.len();
                    }
                }
            }
            Ok(total)
        })
    }
}
