//! Arachne large-file bridge on Clotho's canonical REST commit/read path.

use clotho_common::lfs_pointer::{LfsPointer, PointerError};
use clotho_common::pb::storage::v1::{DownloadFileRequest, UploadFileRequest, UploadFileResponse};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_stream::{iter, wrappers::ReceiverStream};

use crate::error::ApiError;
use crate::AppState;

const UPLOAD_BLOCK_BYTES: usize = 2 * 1024 * 1024;

pub async fn store_payload(
    state: &AppState,
    payload: &[u8],
) -> Result<(Vec<u8>, UploadFileResponse), ApiError> {
    let blocks = payload
        .chunks(UPLOAD_BLOCK_BYTES)
        .map(|data| UploadFileRequest {
            data: data.to_vec(),
        })
        .collect::<Vec<_>>();
    let mut storage = state.storage.clone();
    let outcome = storage.upload_file(iter(blocks)).await?.into_inner();
    let pointer = LfsPointer::for_payload(payload, &outcome.file_hash).encode();
    Ok((pointer, outcome))
}

/// Stream an HTTP payload directly into Arachne while computing the standard
/// Git-LFS SHA-256 pointer. This avoids buffering Hub-sized model artifacts in
/// the gateway and enforces both advertised and actual size limits.
pub async fn store_http_payload(
    state: &AppState,
    mut response: reqwest::Response,
    expected_bytes: u64,
    max_bytes: u64,
) -> Result<(Vec<u8>, UploadFileResponse), ApiError> {
    if expected_bytes > max_bytes {
        return Err(ApiError::InvalidRequest(format!(
            "remote artifact is {expected_bytes} bytes; import limit is {max_bytes}"
        )));
    }
    let (sender, receiver) = mpsc::channel::<UploadFileRequest>(4);
    let producer = tokio::spawn(async move {
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|err| ApiError::Upstream(format!("remote artifact stream: {err}")))?
        {
            received = received.saturating_add(chunk.len() as u64);
            if received > max_bytes {
                return Err(ApiError::InvalidRequest(format!(
                    "remote artifact exceeded the {max_bytes}-byte import limit"
                )));
            }
            hasher.update(&chunk);
            sender
                .send(UploadFileRequest {
                    data: chunk.to_vec(),
                })
                .await
                .map_err(|_| ApiError::Upstream("Arachne upload stream closed".into()))?;
        }
        let digest = hasher.finalize();
        Ok::<_, ApiError>((format!("{digest:x}"), received))
    });

    let mut storage = state.storage.clone();
    let outcome = storage
        .upload_file(ReceiverStream::new(receiver))
        .await?
        .into_inner();
    let (oid_sha256, received) = producer
        .await
        .map_err(|err| ApiError::Internal(format!("remote import task failed: {err}")))??;
    if received != expected_bytes || outcome.file_size != received {
        return Err(ApiError::Upstream(format!(
            "remote artifact size mismatch: expected {expected_bytes}, received {received}"
        )));
    }
    let pointer = LfsPointer {
        oid_sha256,
        size: received,
        arachne_hash: outcome.file_hash.clone(),
    }
    .encode();
    Ok((pointer, outcome))
}

/// Return `None` for an ordinary git blob; materialize and verify Arachne
/// payloads for Clotho pointer blobs.
pub async fn materialize_pointer(
    state: &AppState,
    blob: &[u8],
) -> Result<Option<Vec<u8>>, ApiError> {
    let pointer = match LfsPointer::parse(blob) {
        Ok(pointer) => pointer,
        Err(PointerError::NotPointer) => return Ok(None),
        Err(err) => return Err(ApiError::Upstream(err.to_string())),
    };
    let mut storage = state.storage.clone();
    let mut stream = storage
        .download_file(DownloadFileRequest {
            file_hash: pointer.arachne_hash.clone(),
        })
        .await?
        .into_inner();
    let capacity = usize::try_from(pointer.size)
        .map_err(|_| ApiError::Upstream("Arachne payload is too large for this host".into()))?;
    let mut payload = Vec::with_capacity(capacity);
    while let Some(block) = stream.message().await? {
        payload.extend_from_slice(&block.data);
    }
    pointer
        .verify_payload(&payload)
        .map_err(|err| ApiError::Upstream(err.to_string()))?;
    Ok(Some(payload))
}

/// Read at most `max_bytes` of a logical Git/Arachne file. This is intended
/// for previews: it deliberately stops the Arachne stream early for a huge
/// artifact instead of materializing the full payload in gateway memory.
pub async fn read_prefix(
    state: &AppState,
    blob: &[u8],
    max_bytes: usize,
) -> Result<(Vec<u8>, bool), ApiError> {
    let pointer = match LfsPointer::parse(blob) {
        Ok(pointer) => pointer,
        Err(PointerError::NotPointer) => {
            let truncated = blob.len() > max_bytes;
            return Ok((blob[..blob.len().min(max_bytes)].to_vec(), truncated));
        }
        Err(err) => return Err(ApiError::Upstream(err.to_string())),
    };
    let mut storage = state.storage.clone();
    let mut stream = storage
        .download_file(DownloadFileRequest {
            file_hash: pointer.arachne_hash.clone(),
        })
        .await?
        .into_inner();
    let truncated = pointer.size > max_bytes as u64;
    let expected = usize::try_from(pointer.size.min(max_bytes as u64))
        .map_err(|_| ApiError::Upstream("Arachne preview is too large for this host".into()))?;
    let mut payload = Vec::with_capacity(expected);
    while payload.len() < expected {
        let Some(block) = stream.message().await? else {
            break;
        };
        let remaining = expected - payload.len();
        payload.extend_from_slice(&block.data[..block.data.len().min(remaining)]);
    }
    if !truncated {
        pointer
            .verify_payload(&payload)
            .map_err(|err| ApiError::Upstream(err.to_string()))?;
    }
    Ok((payload, truncated))
}
