//! gRPC surface over the engine.

use clotho_common::pb::storage::v1::{
    storage_server::{Storage, StorageServer},
    DownloadFileRequest, DownloadFileResponse, GetStorageStatsRequest, GetStorageStatsResponse,
    UploadFileRequest, UploadFileResponse,
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

use crate::engine::{ArachneEngine, EngineError};

pub struct StorageService {
    engine: ArachneEngine,
}

impl StorageService {
    pub fn new(engine: ArachneEngine) -> Self {
        Self { engine }
    }

    pub fn into_server(self) -> StorageServer<Self> {
        StorageServer::new(self)
    }
}

impl From<EngineError> for Status {
    fn from(err: EngineError) -> Self {
        match &err {
            EngineError::InvalidHash(_) | EngineError::InvalidRange { .. } => {
                Status::invalid_argument(err.to_string())
            }
            EngineError::FileNotFound(_) => Status::not_found(err.to_string()),
            EngineError::Config(_)
            | EngineError::Store(_)
            | EngineError::Core(_)
            | EngineError::Io(_)
            | EngineError::Other(_) => Status::internal(err.to_string()),
        }
    }
}

#[tonic::async_trait]
impl Storage for StorageService {
    async fn upload_file(
        &self,
        request: Request<Streaming<UploadFileRequest>>,
    ) -> Result<Response<UploadFileResponse>, Status> {
        let mut stream = request.into_inner();
        let mut uploader = self.engine.begin_upload();
        while let Some(msg) = stream.message().await? {
            uploader.write(&msg.data).await?;
        }
        let outcome = uploader.finish().await?;
        tracing::info!(
            file = %outcome.file_hash,
            size = outcome.file_size,
            new_bytes = outcome.new_bytes,
            deduped_bytes = outcome.deduped_bytes,
            stored_bytes_written = outcome.stored_bytes_written,
            "file uploaded"
        );
        Ok(Response::new(UploadFileResponse {
            file_hash: outcome.file_hash,
            file_size: outcome.file_size,
            new_bytes: outcome.new_bytes,
            deduped_bytes: outcome.deduped_bytes,
            new_chunks: outcome.new_chunks,
            deduped_chunks: outcome.deduped_chunks,
            stored_bytes_written: outcome.stored_bytes_written,
        }))
    }

    type DownloadFileStream = ReceiverStream<Result<DownloadFileResponse, Status>>;

    async fn download_file(
        &self,
        request: Request<DownloadFileRequest>,
    ) -> Result<Response<Self::DownloadFileStream>, Status> {
        let req = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        let (out_tx, out_rx) = tokio::sync::mpsc::channel(4);
        let engine = self.engine.clone();
        tokio::spawn(async move {
            engine
                .download_range(&req.file_hash, req.offset, req.length, tx)
                .await
        });
        tokio::spawn(async move {
            let mut rx: tokio::sync::mpsc::Receiver<Result<bytes::Bytes, EngineError>> = rx;
            while let Some(item) = rx.recv().await {
                let mapped = item
                    .map(|data| DownloadFileResponse {
                        data: data.to_vec(),
                    })
                    .map_err(Status::from);
                if out_tx.send(mapped).await.is_err() {
                    break;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(out_rx)))
    }

    async fn get_storage_stats(
        &self,
        _request: Request<GetStorageStatsRequest>,
    ) -> Result<Response<GetStorageStatsResponse>, Status> {
        let stats = self.engine.stats().await?;
        Ok(Response::new(GetStorageStatsResponse {
            xorb_count: stats.xorb_count,
            xorb_bytes: stats.xorb_bytes,
            shard_count: stats.shard_count,
            shard_bytes: stats.shard_bytes,
            total_bytes: stats.xorb_bytes + stats.shard_bytes,
        }))
    }
}
