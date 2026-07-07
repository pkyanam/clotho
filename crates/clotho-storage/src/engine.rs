//! The Arachne engine: Xet-style chunk-level dedup over any S3-compatible store.
//!
//! Design (ADR-0002): we embed the xet-core crates for everything
//! protocol-shaped — GearHash content-defined chunking (~64 KiB average
//! chunks), the deduplication driver, the serialized xorb object format
//! (~64 MiB immutable chunk containers), and the shard metadata format that
//! maps file hashes to (xorb, chunk-range) sequences. What xet-core leaves
//! abstract — where xorbs and shards actually live — is exactly where HF's
//! hosted CAS service would sit; here it is a plain S3-compatible object
//! store instead:
//!
//! - `xorbs/<hash>`  — serialized xorb objects (immutable, content-addressed)
//! - `shards/<name>` — shard files (dedup index + file reconstruction info)
//!
//! `xet_data`'s [`DeduplicationDataInterface`] is the seam: our [`DedupShim`]
//! implements it by querying a local [`ShardFileManager`] (rebuilt from the
//! `shards/` prefix at startup, so the object store stays the source of
//! truth) and writing new xorbs straight to the object store. The engine
//! never talks to any HF service, and nothing here is AWS-specific — the
//! backend is endpoint + credentials config only (MinIO, R2, B2, Ceph,
//! Garage, AWS, ...).

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use xet_core_structures::merklehash::MerkleHash;
use xet_core_structures::metadata_shard::file_structs::FileDataSequenceEntry;
use xet_core_structures::metadata_shard::shard_file_reconstructor::FileReconstructor;
use xet_core_structures::metadata_shard::ShardFileManager;
use xet_core_structures::xorb_object::{RawXorbData, SerializedXorbObject, XorbObject};
use xet_core_structures::CoreError;
use xet_data::deduplication::{Chunker, DeduplicationDataInterface, FileDeduper};
use xet_data::progress_tracking::upload_tracking::FileXorbDependency;

const XORB_PREFIX: &str = "xorbs";
const SHARD_PREFIX: &str = "shards";

/// Downloads are streamed in blocks no larger than this, so a segment spanning
/// a whole xorb (~64 MiB unpacked) never exceeds gRPC message limits.
const DOWNLOAD_BLOCK_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("invalid storage configuration: {0}")]
    Config(String),

    #[error("invalid file hash {0:?}")]
    InvalidHash(String),

    #[error("no file stored with hash {0}")]
    FileNotFound(String),

    #[error("object store error: {0}")]
    Store(#[from] object_store::Error),

    #[error("xet-core error: {0}")]
    Core(#[from] CoreError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

/// S3-compatible backend configuration, from `CLOTHO_STORAGE_*` env vars.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// e.g. `http://localhost:9000` (MinIO) or any S3-compatible endpoint.
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    /// Optional key prefix inside the bucket (namespacing / tests).
    pub prefix: Option<String>,
    /// Local scratch directory for the shard index cache.
    pub data_dir: PathBuf,
}

impl StorageConfig {
    pub fn from_env() -> Result<Self, EngineError> {
        let var = |name: &str| {
            std::env::var(name).map_err(|_| EngineError::Config(format!("{name} must be set")))
        };
        Ok(Self {
            endpoint: var("CLOTHO_STORAGE_S3_ENDPOINT")?,
            bucket: var("CLOTHO_STORAGE_S3_BUCKET")?,
            access_key: var("CLOTHO_STORAGE_S3_ACCESS_KEY")?,
            secret_key: var("CLOTHO_STORAGE_S3_SECRET_KEY")?,
            region: std::env::var("CLOTHO_STORAGE_S3_REGION")
                .unwrap_or_else(|_| "us-east-1".to_string()),
            prefix: std::env::var("CLOTHO_STORAGE_S3_PREFIX")
                .ok()
                .filter(|p| !p.is_empty()),
            data_dir: std::env::var("CLOTHO_STORAGE_DATA_DIR")
                .unwrap_or_else(|_| "./data/storage".to_string())
                .into(),
        })
    }
}

/// Measured result of one upload.
#[derive(Debug, Clone)]
pub struct UploadOutcome {
    /// Hex MerkleHash of the whole file — the download handle.
    pub file_hash: String,
    pub file_size: u64,
    pub new_bytes: u64,
    pub deduped_bytes: u64,
    pub new_chunks: u64,
    pub deduped_chunks: u64,
    /// Serialized bytes actually written to the object store by this upload.
    pub stored_bytes_written: u64,
}

/// Ground-truth usage, measured by listing the object store.
#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    pub xorb_count: u64,
    pub xorb_bytes: u64,
    pub shard_count: u64,
    pub shard_bytes: u64,
}

fn xorb_path(hash: &MerkleHash) -> ObjectPath {
    ObjectPath::from(format!("{XORB_PREFIX}/{}", hash.hex()))
}

/// Serialize a xorb, write it to the object store, and index its chunks so
/// later uploads dedup against it. Content-addressed keys make concurrent
/// identical writes idempotent.
async fn write_xorb(
    store: &Arc<dyn ObjectStore>,
    shards: &Arc<ShardFileManager>,
    bytes_written: &AtomicU64,
    xorb: RawXorbData,
) -> Result<(), EngineError> {
    let xorb_info = xorb.xorb_info.clone();
    let hash = xorb.hash();
    // Serialization compresses chunk-by-chunk (lz4/bg4 heuristics) — CPU-bound,
    // so keep it off the async workers.
    let serialized =
        tokio::task::spawn_blocking(move || SerializedXorbObject::from_xorb(xorb, true))
            .await
            .map_err(|e| EngineError::Other(format!("xorb serialization task failed: {e}")))??;
    let n_bytes = serialized.serialized_data.len() as u64;
    store
        .put(
            &xorb_path(&hash),
            PutPayload::from(serialized.serialized_data),
        )
        .await?;
    bytes_written.fetch_add(n_bytes, Ordering::Relaxed);
    shards.add_xorb_block(xorb_info).await?;
    tracing::debug!(xorb = %hash.hex(), bytes = n_bytes, "xorb written");
    Ok(())
}

/// The content-addressed-store shim xet-core's dedup driver runs against:
/// chunk queries hit the local shard index; new xorbs go to the object store.
struct DedupShim {
    store: Arc<dyn ObjectStore>,
    shards: Arc<ShardFileManager>,
    bytes_written: Arc<AtomicU64>,
}

#[async_trait]
impl DeduplicationDataInterface for DedupShim {
    type ErrorType = EngineError;

    async fn chunk_hash_dedup_query(
        &self,
        query_hashes: &[MerkleHash],
    ) -> Result<Option<(usize, FileDataSequenceEntry, bool)>, EngineError> {
        Ok(self
            .shards
            .chunk_hash_dedup_query(query_hashes)
            .await?
            .map(|(n, fse)| (n, fse, true)))
    }

    async fn register_global_dedup_query(
        &mut self,
        _chunk_hash: MerkleHash,
    ) -> Result<(), EngineError> {
        // Every upload already dedups against the full shard index; there is
        // no separate global-dedup service in this deployment.
        Ok(())
    }

    async fn complete_global_dedup_queries(&mut self) -> Result<bool, EngineError> {
        Ok(false)
    }

    async fn register_new_xorb(&mut self, xorb: RawXorbData) -> Result<(), EngineError> {
        write_xorb(&self.store, &self.shards, &self.bytes_written, xorb).await
    }

    async fn register_xorb_dependencies(&mut self, _dependencies: &[FileXorbDependency]) {}
}

/// One in-flight streaming upload. Feed bytes with [`write`](Self::write),
/// then [`finish`](Self::finish) to get the file hash and measured metrics.
pub struct FileUploader {
    chunker: Chunker,
    deduper: FileDeduper<DedupShim>,
    store: Arc<dyn ObjectStore>,
    shards: Arc<ShardFileManager>,
    bytes_written: Arc<AtomicU64>,
}

impl FileUploader {
    pub async fn write(&mut self, data: &[u8]) -> Result<(), EngineError> {
        let chunks = self.chunker.next_block(data, false);
        if !chunks.is_empty() {
            self.deduper.process_chunks(&chunks).await?;
        }
        Ok(())
    }

    pub async fn finish(mut self) -> Result<UploadOutcome, EngineError> {
        if let Some(chunk) = self.chunker.finish() {
            self.deduper.process_chunks(&[chunk]).await?;
        }
        let (file_hash, remaining, metrics) = self.deduper.finalize(None);

        // Cut the final (partial) xorb and fill its hash into the file infos.
        let (last_xorb, file_infos) = remaining.finalize();
        if last_xorb.num_bytes() > 0 {
            write_xorb(&self.store, &self.shards, &self.bytes_written, last_xorb).await?;
        }
        for (_, file_info, _) in file_infos {
            self.shards.add_file_reconstruction_info(file_info).await?;
        }

        // Persist the shard metadata next to the xorbs, so the dedup index and
        // reconstruction info survive restarts from the object store alone.
        if let Some(shard_path) = self.shards.flush().await? {
            let bytes = tokio::fs::read(&shard_path).await?;
            let name = shard_path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| {
                    EngineError::Other(format!("unexpected shard path {shard_path:?}"))
                })?;
            let n_bytes = bytes.len() as u64;
            self.store
                .put(
                    &ObjectPath::from(format!("{SHARD_PREFIX}/{name}")),
                    PutPayload::from(bytes),
                )
                .await?;
            self.bytes_written.fetch_add(n_bytes, Ordering::Relaxed);
            tracing::debug!(shard = name, bytes = n_bytes, "shard written");
        }

        Ok(UploadOutcome {
            file_hash: file_hash.hex(),
            file_size: metrics.total_bytes,
            new_bytes: metrics.new_bytes,
            deduped_bytes: metrics.deduped_bytes,
            new_chunks: metrics.new_chunks,
            deduped_chunks: metrics.deduped_chunks,
            stored_bytes_written: self.bytes_written.load(Ordering::Relaxed),
        })
    }
}

/// The engine itself. Cheap to clone; safe for concurrent uploads/downloads.
#[derive(Clone)]
pub struct ArachneEngine {
    store: Arc<dyn ObjectStore>,
    shards: Arc<ShardFileManager>,
}

impl ArachneEngine {
    /// Connect to the object store and rebuild the local shard index from the
    /// `shards/` prefix, so dedup state persists across restarts.
    pub async fn new(config: &StorageConfig) -> Result<Self, EngineError> {
        let allow_http = config.endpoint.starts_with("http://");
        let s3 = AmazonS3Builder::new()
            .with_endpoint(&config.endpoint)
            .with_bucket_name(&config.bucket)
            .with_access_key_id(&config.access_key)
            .with_secret_access_key(&config.secret_key)
            .with_region(&config.region)
            .with_allow_http(allow_http)
            .build()
            .map_err(|e| EngineError::Config(e.to_string()))?;
        let store: Arc<dyn ObjectStore> = match &config.prefix {
            Some(prefix) => Arc::new(object_store::prefix::PrefixStore::new(s3, prefix.as_str())),
            None => Arc::new(s3),
        };

        let shard_dir = config.data_dir.join("shards");
        tokio::fs::create_dir_all(&shard_dir).await?;
        let downloaded = Self::sync_shards_from_store(&store, &shard_dir).await?;
        let shards = ShardFileManager::new_in_session_directory(&shard_dir, true).await?;
        tracing::info!(
            shard_dir = %shard_dir.display(),
            shards_downloaded = downloaded,
            "Arachne engine ready"
        );
        Ok(Self { store, shards })
    }

    async fn sync_shards_from_store(
        store: &Arc<dyn ObjectStore>,
        shard_dir: &Path,
    ) -> Result<usize, EngineError> {
        let mut listing = store.list(Some(&ObjectPath::from(SHARD_PREFIX)));
        let mut downloaded = 0;
        while let Some(meta) = listing.next().await.transpose()? {
            let Some(name) = meta.location.filename() else {
                continue;
            };
            let local = shard_dir.join(name);
            if tokio::fs::try_exists(&local).await? {
                continue;
            }
            let bytes = store.get(&meta.location).await?.bytes().await?;
            tokio::fs::write(&local, &bytes).await?;
            downloaded += 1;
        }
        Ok(downloaded)
    }

    pub fn begin_upload(&self) -> FileUploader {
        let shim = DedupShim {
            store: self.store.clone(),
            shards: self.shards.clone(),
            bytes_written: Arc::new(AtomicU64::new(0)),
        };
        let bytes_written = shim.bytes_written.clone();
        FileUploader {
            chunker: Chunker::default(),
            deduper: FileDeduper::new(shim, 0),
            store: self.store.clone(),
            shards: self.shards.clone(),
            bytes_written,
        }
    }

    /// Reconstruct a file byte-identically, streaming blocks into `tx`.
    /// Errors are delivered through the channel so the caller sees them
    /// mid-stream.
    pub async fn download(&self, file_hash: &str, tx: mpsc::Sender<Result<Bytes, EngineError>>) {
        if let Err(e) = self.download_inner(file_hash, &tx).await {
            let _ = tx.send(Err(e)).await;
        }
    }

    async fn download_inner(
        &self,
        file_hash: &str,
        tx: &mpsc::Sender<Result<Bytes, EngineError>>,
    ) -> Result<(), EngineError> {
        let hash = MerkleHash::from_hex(file_hash)
            .map_err(|_| EngineError::InvalidHash(file_hash.to_string()))?;
        let (file_info, _) = self
            .shards
            .get_file_reconstruction_info(&hash)
            .await?
            .ok_or_else(|| EngineError::FileNotFound(file_hash.to_string()))?;

        // Segments referencing the same xorb are typically adjacent, so a
        // one-entry cache avoids refetching without holding many xorbs in
        // memory.
        let mut cached: Option<(MerkleHash, XorbObject, Bytes)> = None;
        for segment in &file_info.segments {
            if cached.as_ref().map(|(h, _, _)| *h) != Some(segment.xorb_hash) {
                let bytes = self
                    .store
                    .get(&xorb_path(&segment.xorb_hash))
                    .await?
                    .bytes()
                    .await?;
                let object = XorbObject::deserialize(&mut Cursor::new(&bytes))?;
                cached = Some((segment.xorb_hash, object, bytes));
            }
            let (_, object, bytes) = cached.as_ref().expect("just populated");
            let data = object.get_bytes_by_chunk_range(
                &mut Cursor::new(bytes),
                segment.chunk_index_start,
                segment.chunk_index_end,
            )?;
            debug_assert_eq!(data.len() as u32, segment.unpacked_segment_bytes);
            let data = Bytes::from(data);
            for offset in (0..data.len()).step_by(DOWNLOAD_BLOCK_BYTES) {
                let end = (offset + DOWNLOAD_BLOCK_BYTES).min(data.len());
                if tx.send(Ok(data.slice(offset..end))).await.is_err() {
                    // Receiver hung up; stop reconstructing.
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Measure real usage by listing the object store — ground truth for the
    /// dedup numbers, independent of anything the engine tracked in memory.
    pub async fn stats(&self) -> Result<StorageStats, EngineError> {
        let mut stats = StorageStats::default();
        for (prefix, count, bytes) in [
            (XORB_PREFIX, &mut stats.xorb_count, &mut stats.xorb_bytes),
            (SHARD_PREFIX, &mut stats.shard_count, &mut stats.shard_bytes),
        ] {
            let mut listing = self.store.list(Some(&ObjectPath::from(prefix)));
            while let Some(meta) = listing.next().await.transpose()? {
                *count += 1;
                *bytes += meta.size;
            }
        }
        Ok(stats)
    }
}
