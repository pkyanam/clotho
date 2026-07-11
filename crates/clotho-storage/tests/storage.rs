//! Stage 2 exit-condition tests (docs/prd.md §5): upload a large synthetic
//! file, then a near-duplicate with small modifications, and prove — via
//! ground-truth byte counts measured by listing the object store — that only
//! the changed chunks were newly written, and that both files reconstruct
//! byte-identical to source. Entirely through the gRPC API.
//!
//! Requires an S3-compatible endpoint (MinIO in dev): set
//! `CLOTHO_STORAGE_TEST_S3_ENDPOINT` (e.g. `http://localhost:9000`, as in the
//! compose stack — `just dev` provisions the `clotho-storage-test` bucket).
//! Tests are skipped when it is unset so plain `cargo test` stays green.
//! Optional overrides: `CLOTHO_STORAGE_TEST_S3_{BUCKET,ACCESS_KEY,SECRET_KEY}`,
//! and `CLOTHO_STORAGE_TEST_FILE_MB` (default 256) for the synthetic file size.
//! CI and `just test-storage` set `CLOTHO_TEST_FAIL_ON_SKIP=1`. Every exact
//! per-test object prefix is removed afterward; set
//! `CLOTHO_TEST_KEEP_FIXTURES_ON_FAILURE=1` to retain only a failed fixture.

use clotho_common::pb::storage::v1::{
    storage_client::StorageClient, DownloadFileRequest, GetStorageStatsRequest,
    GetStorageStatsResponse, UploadFileRequest, UploadFileResponse,
};
use clotho_storage::{ArachneEngine, StorageConfig, StorageService};
use futures::FutureExt;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};

const UPLOAD_BLOCK_BYTES: usize = 2 * 1024 * 1024;

fn test_config(prefix: &str, data_dir: &std::path::Path) -> Option<StorageConfig> {
    let endpoint = match std::env::var("CLOTHO_STORAGE_TEST_S3_ENDPOINT") {
        Ok(endpoint) if !endpoint.is_empty() => endpoint,
        _ if env_truthy("CLOTHO_TEST_FAIL_ON_SKIP") => {
            panic!(
                "live-test gate refused to skip: CLOTHO_STORAGE_TEST_S3_ENDPOINT not set (start MinIO via `just dev`)"
            )
        }
        _ => {
            eprintln!(
                "skipping: CLOTHO_STORAGE_TEST_S3_ENDPOINT not set (start MinIO via `just dev`)"
            );
            return None;
        }
    };
    let env_or = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
    Some(StorageConfig {
        endpoint,
        bucket: env_or("CLOTHO_STORAGE_TEST_S3_BUCKET", "clotho-storage-test"),
        access_key: env_or("CLOTHO_STORAGE_TEST_S3_ACCESS_KEY", "clotho"),
        secret_key: env_or("CLOTHO_STORAGE_TEST_S3_SECRET_KEY", "clotho-dev"),
        region: "us-east-1".into(),
        prefix: Some(prefix.to_string()),
        data_dir: data_dir.to_path_buf(),
    })
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

async fn cleanup_storage_fixture(config: &StorageConfig, failed: bool) -> Result<(), String> {
    let prefix = config
        .prefix
        .as_deref()
        .ok_or_else(|| "test storage cleanup requires an exact prefix".to_string())?;
    if !prefix.starts_with("run-") || prefix.contains('/') {
        return Err(format!("refusing to clean unsafe test prefix {prefix:?}"));
    }
    if failed && env_truthy("CLOTHO_TEST_KEEP_FIXTURES_ON_FAILURE") {
        eprintln!("preserving failed storage fixture prefix {prefix}");
        return Ok(());
    }

    let allow_http = config.endpoint.starts_with("http://");
    let store = AmazonS3Builder::new()
        .with_endpoint(&config.endpoint)
        .with_bucket_name(&config.bucket)
        .with_access_key_id(&config.access_key)
        .with_secret_access_key(&config.secret_key)
        .with_region(&config.region)
        .with_allow_http(allow_http)
        .build()
        .map_err(|error| format!("build storage fixture cleanup client: {error}"))?;
    let path = ObjectPath::from(prefix);
    let mut listing = store.list(Some(&path));
    let mut locations = Vec::new();
    while let Some(item) = tokio_stream::StreamExt::next(&mut listing).await {
        locations.push(
            item.map_err(|error| format!("list storage fixture prefix {prefix}: {error}"))?
                .location,
        );
    }
    drop(listing);
    for location in locations {
        store
            .delete(&location)
            .await
            .map_err(|error| format!("delete storage fixture object {location}: {error}"))?;
    }
    let mut remaining = store.list(Some(&path));
    if let Some(item) = tokio_stream::StreamExt::next(&mut remaining).await {
        let location = item
            .map_err(|error| format!("verify storage fixture cleanup {prefix}: {error}"))?
            .location;
        return Err(format!(
            "storage fixture cleanup left object {location} under {prefix}"
        ));
    }
    Ok(())
}

async fn finish_storage_fixture(
    config: &StorageConfig,
    outcome: Result<(), Box<dyn std::any::Any + Send>>,
) {
    let cleanup = cleanup_storage_fixture(config, outcome.is_err()).await;
    match outcome {
        Ok(()) => cleanup.unwrap_or_else(|error| panic!("fixture cleanup failed: {error}")),
        Err(payload) => {
            if let Err(error) = cleanup {
                eprintln!("fixture cleanup also failed after test failure: {error}");
            }
            std::panic::resume_unwind(payload);
        }
    }
}

/// Unique per-run key prefix so runs never see each other's objects.
fn unique_prefix(test: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("run-{nanos}-{test}")
}

async fn start_server(config: &StorageConfig) -> StorageClient<Channel> {
    let engine = ArachneEngine::new(config).await.unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(
        Server::builder()
            .add_service(StorageService::new(engine).into_server())
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );
    StorageClient::connect(format!("http://{addr}"))
        .await
        .unwrap()
}

fn file_size_bytes() -> usize {
    let mb = std::env::var("CLOTHO_STORAGE_TEST_FILE_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(256);
    mb * 1024 * 1024
}

/// Deterministic pseudo-random bytes (xorshift64*) — incompressible, so
/// storage byte counts track content bytes closely.
fn synthetic_file(len: usize, seed: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(len + 8);
    let mut state = seed | 1;
    while out.len() < len {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        out.extend_from_slice(&(state.wrapping_mul(0x2545F4914F6CDD1D)).to_le_bytes());
    }
    out.truncate(len);
    out
}

async fn upload(client: &mut StorageClient<Channel>, data: &[u8]) -> UploadFileResponse {
    let blocks: Vec<UploadFileRequest> = data
        .chunks(UPLOAD_BLOCK_BYTES)
        .map(|b| UploadFileRequest { data: b.to_vec() })
        .collect();
    client
        .upload_file(tokio_stream::iter(blocks))
        .await
        .unwrap()
        .into_inner()
}

async fn download(client: &mut StorageClient<Channel>, file_hash: &str) -> Vec<u8> {
    let mut stream = client
        .download_file(DownloadFileRequest {
            file_hash: file_hash.to_string(),
            offset: 0,
            length: 0,
        })
        .await
        .unwrap()
        .into_inner();
    let mut out = Vec::new();
    while let Some(msg) = stream.message().await.unwrap() {
        out.extend_from_slice(&msg.data);
    }
    out
}

async fn download_range(
    client: &mut StorageClient<Channel>,
    file_hash: &str,
    offset: u64,
    length: u64,
) -> Vec<u8> {
    let mut stream = client
        .download_file(DownloadFileRequest {
            file_hash: file_hash.to_string(),
            offset,
            length,
        })
        .await
        .unwrap()
        .into_inner();
    let mut out = Vec::new();
    while let Some(msg) = stream.message().await.unwrap() {
        out.extend_from_slice(&msg.data);
    }
    out
}

async fn stats(client: &mut StorageClient<Channel>) -> GetStorageStatsResponse {
    client
        .get_storage_stats(GetStorageStatsRequest {})
        .await
        .unwrap()
        .into_inner()
}

#[tokio::test(flavor = "multi_thread")]
async fn dedup_is_measured_and_reconstruction_is_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let Some(config) = test_config(&unique_prefix("dedup"), dir.path()) else {
        return;
    };

    let outcome = std::panic::AssertUnwindSafe(async {
        let mut client = start_server(&config).await;

        let size = file_size_bytes();
        let file_a = synthetic_file(size, 0xC10740);

        // A fresh prefix must start empty — the baseline for all byte counts.
        let stats_0 = stats(&mut client).await;
        assert_eq!(stats_0.total_bytes, 0);

        // Upload the original file.
        let resp_a = upload(&mut client, &file_a).await;
        assert_eq!(resp_a.file_size, size as u64);
        let stats_a = stats(&mut client).await;
        // Everything is new: the store now holds roughly the file (incompressible
        // data; small chunk-framing and shard-metadata overhead).
        assert!(
            stats_a.total_bytes >= (size as u64 * 95) / 100,
            "stats after A: {stats_a:?}"
        );
        assert!(
            stats_a.xorb_count >= 1 && stats_a.shard_count >= 1,
            "stats after A: {stats_a:?}"
        );

        // Near-duplicate: overwrite 64 KiB in the middle AND insert 1 KiB at the
        // quarter point. The insertion shifts every following byte, which defeats
        // fixed-size blocking — only content-defined chunking dedups past it.
        let mut file_b = file_a.clone();
        let overwrite = synthetic_file(64 * 1024, 0xBAD5EED);
        let mid = size / 2;
        file_b[mid..mid + overwrite.len()].copy_from_slice(&overwrite);
        let insert = synthetic_file(1024, 0x1235EED);
        let quarter = size / 4;
        file_b.splice(quarter..quarter, insert.iter().copied());

        let resp_b = upload(&mut client, &file_b).await;
        let stats_b = stats(&mut client).await;

        // The dedup must be measured, not assumed: at least 95% of the
        // near-duplicate's bytes deduped against existing chunks...
        assert!(
            resp_b.deduped_bytes >= (resp_b.file_size * 95) / 100,
            "expected >=95% dedup, got {resp_b:?}"
        );
        // ...and the ground-truth growth of the object store is a small fraction
        // of the file: only the changed chunks (plus metadata) were newly written.
        let growth = stats_b.total_bytes - stats_a.total_bytes;
        assert!(
            growth < (size as u64 * 5) / 100,
            "store grew {growth} bytes after near-duplicate upload; stats {stats_b:?}"
        );
        assert_eq!(
            growth, resp_b.stored_bytes_written,
            "engine-counted vs listed bytes"
        );

        // Re-uploading identical content writes no new chunk data at all.
        let resp_a2 = upload(&mut client, &file_a).await;
        assert_eq!(resp_a2.file_hash, resp_a.file_hash);
        assert_eq!(
            resp_a2.new_bytes, 0,
            "identical re-upload must dedup fully: {resp_a2:?}"
        );

        // Both files reconstruct byte-identical to source.
        let roundtrip_a = download(&mut client, &resp_a.file_hash).await;
        assert!(
            roundtrip_a == file_a,
            "file A did not reconstruct byte-identical"
        );
        let roundtrip_b = download(&mut client, &resp_b.file_hash).await;
        assert!(
            roundtrip_b == file_b,
            "file B did not reconstruct byte-identical"
        );
        let range_start = (size / 3) as u64;
        let range_length = 3 * 1024 * 1024 + 17;
        let ranged = download_range(
            &mut client,
            &resp_a.file_hash,
            range_start,
            range_length as u64,
        )
        .await;
        assert_eq!(
            ranged,
            file_a[range_start as usize..range_start as usize + range_length],
            "ranged reconstruction crossed Xet segments incorrectly"
        );

        eprintln!(
            "measured: file={} B; after A store={} B; near-duplicate wrote {} B ({:.2}% of file); \
         dedup {}/{} chunks",
            size,
            stats_a.total_bytes,
            growth,
            growth as f64 * 100.0 / size as f64,
            resp_b.deduped_chunks,
            resp_b.deduped_chunks + resp_b.new_chunks,
        );
    })
    .catch_unwind()
    .await;
    finish_storage_fixture(&config, outcome).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn dedup_state_survives_restart_via_object_store() {
    let prefix = unique_prefix("restart");
    let size = 32 * 1024 * 1024;
    let file = synthetic_file(size, 0x5EED);
    let cleanup_dir = tempfile::tempdir().unwrap();
    let Some(cleanup_config) = test_config(&prefix, cleanup_dir.path()) else {
        return;
    };

    let outcome = std::panic::AssertUnwindSafe(async {
        // First engine instance: upload, then drop it (local shard cache and all).
        let (hash, first_stats) = {
            let dir = tempfile::tempdir().unwrap();
            let Some(config) = test_config(&prefix, dir.path()) else {
                return;
            };
            let mut client = start_server(&config).await;
            let resp = upload(&mut client, &file).await;
            (resp.file_hash, stats(&mut client).await)
        };

        // Second instance with a fresh local directory must rebuild the dedup
        // index from the object store's shards/ prefix alone.
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(&prefix, dir.path()).unwrap();
        let mut client = start_server(&config).await;

        let resp = upload(&mut client, &file).await;
        assert_eq!(resp.file_hash, hash);
        assert_eq!(
            resp.new_bytes, 0,
            "restarted engine must dedup against S3 state: {resp:?}"
        );
        let after = stats(&mut client).await;
        assert_eq!(
            after.xorb_bytes, first_stats.xorb_bytes,
            "no new xorb bytes after restart"
        );

        let roundtrip = download(&mut client, &hash).await;
        assert!(
            roundtrip == file,
            "file did not reconstruct byte-identical after restart"
        );
    })
    .catch_unwind()
    .await;
    finish_storage_fixture(&cleanup_config, outcome).await;
}
