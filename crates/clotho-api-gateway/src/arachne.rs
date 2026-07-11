//! Arachne large-file bridge on Clotho's canonical REST commit/read path.

use clotho_common::lfs_pointer::{LfsPointer, PointerError};
use clotho_common::pb::storage::v1::{DownloadFileRequest, UploadFileRequest, UploadFileResponse};
use tokio_stream::iter;

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
