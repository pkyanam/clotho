//! Modular repository-hub import boundary. Hugging Face is the first provider;
//! imported bytes always land through Clotho VCS/Arachne, so the source Hub is
//! never the runtime source of truth.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use clotho_common::pb::mergequeue::v1::SubmitChangeRequest;
use clotho_common::pb::vcs::v1::{CommitRequest, FileChange};
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::control::{self, ActivityEventInput};
use crate::error::ApiError;
use crate::{validate_commit_path, AppState};

const DEFAULT_MAX_FILES: usize = 200;
const MAX_IMPORT_FILES: usize = 1_000;
const MAX_DISCOVERY_FILES: usize = 5_000;
const DEFAULT_MAX_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const MAX_IMPORT_BYTES: u64 = 1024 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
pub struct HubSecurityStatus {
    pub status: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HubFile {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub path: String,
    pub size: u64,
    #[serde(default, rename = "securityFileStatus")]
    pub security: Option<HubSecurityStatus>,
}

#[async_trait]
pub trait HubProvider: Send + Sync {
    fn id(&self) -> &'static str;
    async fn list_files(
        &self,
        repo_id: &str,
        repo_type: &str,
        revision: &str,
        token: Option<&str>,
    ) -> Result<Vec<HubFile>, ApiError>;
    async fn download(
        &self,
        repo_id: &str,
        repo_type: &str,
        revision: &str,
        path: &str,
        token: Option<&str>,
    ) -> Result<reqwest::Response, ApiError>;
}

#[derive(Clone)]
pub struct HuggingFaceHub {
    http: reqwest::Client,
}

impl HuggingFaceHub {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http }
    }

    fn repo_parts(repo_id: &str) -> Result<(&str, &str), ApiError> {
        let mut parts = repo_id.split('/');
        let namespace = parts.next().unwrap_or_default();
        let repo = parts.next().unwrap_or_default();
        if namespace.is_empty()
            || repo.is_empty()
            || parts.next().is_some()
            || ![namespace, repo].iter().all(|part| {
                !matches!(*part, "." | "..")
                    && part.len() <= 128
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            })
        {
            return Err(ApiError::InvalidRequest(
                "Hugging Face repo_id must be namespace/name".into(),
            ));
        }
        Ok((namespace, repo))
    }

    fn validate_revision(revision: &str) -> Result<(), ApiError> {
        if revision.is_empty()
            || revision.len() > 200
            || revision.chars().any(char::is_control)
            || revision
                .split('/')
                .any(|part| part.is_empty() || part == "..")
        {
            return Err(ApiError::InvalidRequest(
                "invalid Hugging Face revision".into(),
            ));
        }
        Ok(())
    }

    fn tree_url(repo_id: &str, repo_type: &str, revision: &str) -> Result<reqwest::Url, ApiError> {
        let (namespace, repo) = Self::repo_parts(repo_id)?;
        Self::validate_revision(revision)?;
        let collection = if repo_type == "dataset" {
            "datasets"
        } else {
            "models"
        };
        let mut url = reqwest::Url::parse("https://huggingface.co/")
            .map_err(|err| ApiError::Internal(format!("Hugging Face URL: {err}")))?;
        url.path_segments_mut()
            .map_err(|_| ApiError::Internal("Hugging Face URL cannot be a base".into()))?
            .extend(["api", collection, namespace, repo, "tree", revision]);
        url.query_pairs_mut()
            .append_pair("recursive", "true")
            .append_pair("expand", "true")
            .append_pair("limit", "100");
        Ok(url)
    }

    fn download_url(
        repo_id: &str,
        repo_type: &str,
        revision: &str,
        path: &str,
    ) -> Result<reqwest::Url, ApiError> {
        let (namespace, repo) = Self::repo_parts(repo_id)?;
        Self::validate_revision(revision)?;
        validate_commit_path(path)?;
        let mut url = reqwest::Url::parse("https://huggingface.co/")
            .map_err(|err| ApiError::Internal(format!("Hugging Face URL: {err}")))?;
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| ApiError::Internal("Hugging Face URL cannot be a base".into()))?;
        if repo_type == "dataset" {
            segments.push("datasets");
        }
        segments.extend([namespace, repo, "resolve", revision]);
        segments.extend(path.split('/'));
        drop(segments);
        Ok(url)
    }

    fn request(&self, url: reqwest::Url, token: Option<&str>) -> reqwest::RequestBuilder {
        let request = self
            .http
            .get(url)
            .header("user-agent", "clotho-hub-import/0.1");
        match token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    async fn checked_response(
        request: reqwest::RequestBuilder,
        context: &str,
        timeout: Duration,
    ) -> Result<reqwest::Response, ApiError> {
        let response = request
            .timeout(timeout)
            .send()
            .await
            .map_err(|err| ApiError::Upstream(format!("Hugging Face {context}: {err}")))?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let reason = if matches!(status.as_u16(), 401 | 403) {
            "repository is private or gated; store HUGGINGFACE_TOKEN in Clotho secrets"
        } else if status.as_u16() == 404 {
            "repository, revision, or file was not found"
        } else {
            "Hub request failed"
        };
        Err(ApiError::Upstream(format!(
            "Hugging Face {context} returned {status}: {reason}"
        )))
    }
}

#[async_trait]
impl HubProvider for HuggingFaceHub {
    fn id(&self) -> &'static str {
        "huggingface"
    }

    async fn list_files(
        &self,
        repo_id: &str,
        repo_type: &str,
        revision: &str,
        token: Option<&str>,
    ) -> Result<Vec<HubFile>, ApiError> {
        let mut url = Self::tree_url(repo_id, repo_type, revision)?;
        let mut files = Vec::new();
        loop {
            let response = Self::checked_response(
                self.request(url.clone(), token),
                "tree discovery",
                Duration::from_secs(30),
            )
            .await?;
            let next_cursor = response
                .headers()
                .get(reqwest::header::LINK)
                .and_then(|value| value.to_str().ok())
                .and_then(next_cursor);
            let page = response
                .json::<Vec<HubFile>>()
                .await
                .map_err(|err| ApiError::Upstream(format!("Hugging Face tree JSON: {err}")))?;
            files.extend(page.into_iter().filter(|file| file.entry_type == "file"));
            if files.len() > MAX_DISCOVERY_FILES {
                return Err(ApiError::InvalidRequest(format!(
                    "Hub repository has more than {MAX_DISCOVERY_FILES} files; choose a smaller source repository"
                )));
            }
            let Some(cursor) = next_cursor else { break };
            url.query_pairs_mut()
                .clear()
                .append_pair("recursive", "true")
                .append_pair("expand", "true")
                .append_pair("limit", "100")
                .append_pair("cursor", &cursor);
        }
        Ok(files)
    }

    async fn download(
        &self,
        repo_id: &str,
        repo_type: &str,
        revision: &str,
        path: &str,
        token: Option<&str>,
    ) -> Result<reqwest::Response, ApiError> {
        let url = Self::download_url(repo_id, repo_type, revision, path)?;
        Self::checked_response(
            self.request(url, token),
            &format!("download {path:?}"),
            Duration::from_secs(60 * 60),
        )
        .await
    }
}

fn next_cursor(link: &str) -> Option<String> {
    let next = link.split(',').find(|part| part.contains("rel=\"next\""))?;
    let url = next.split_once('<')?.1.split_once('>')?.0;
    let url = reqwest::Url::parse(url).ok()?;
    if url.scheme() != "https" || url.host_str() != Some("huggingface.co") {
        return None;
    }
    url.query_pairs()
        .find(|(key, _)| key == "cursor")
        .map(|(_, value)| value.into_owned())
}

#[derive(Debug, Deserialize)]
pub struct ImportHubRequest {
    pub repo_id: String,
    #[serde(default = "default_revision")]
    pub revision: String,
    /// Exact repository paths. Empty imports the complete bounded snapshot.
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default = "default_max_files")]
    pub max_files: usize,
    #[serde(default = "default_max_bytes")]
    pub max_total_bytes: u64,
    #[serde(default)]
    pub allow_unsafe: bool,
}

fn default_revision() -> String {
    "main".into()
}
fn default_max_files() -> usize {
    DEFAULT_MAX_FILES
}
fn default_max_bytes() -> u64 {
    DEFAULT_MAX_BYTES
}

#[derive(Debug, Serialize)]
pub struct ImportHubResponse {
    pub provider: String,
    pub source_repo_id: String,
    pub source_revision: String,
    pub commit_id: String,
    pub operation_id: String,
    pub files_imported: u64,
    pub logical_bytes: u64,
    pub arachne_files: u64,
    pub security_counts: BTreeMap<String, u64>,
    pub fast_forwarded: bool,
    pub conflicted: bool,
    pub conflicted_paths: Vec<String>,
}

pub async fn import_huggingface(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(request): Json<ImportHubRequest>,
) -> Result<Json<ImportHubResponse>, ApiError> {
    if !(1..=MAX_IMPORT_FILES).contains(&request.max_files) {
        return Err(ApiError::InvalidRequest(format!(
            "max_files must be between 1 and {MAX_IMPORT_FILES}"
        )));
    }
    if request.max_total_bytes == 0 || request.max_total_bytes > MAX_IMPORT_BYTES {
        return Err(ApiError::InvalidRequest(format!(
            "max_total_bytes must be between 1 and {MAX_IMPORT_BYTES}"
        )));
    }
    let auth = auth::resolve_auth(&headers, &state).await?;
    let repo = match &state.pool {
        Some(pool) => {
            control::require_repo_permission(pool, &name, &auth.user_id, "write").await?;
            control::get_repo_by_name(pool, &name).await?
        }
        None => {
            return Err(ApiError::Internal(
                "Hub import requires the Clotho control plane".into(),
            ))
        }
    };
    if !matches!(repo.kind.as_str(), "model" | "dataset") {
        return Err(ApiError::InvalidRequest(
            "Hugging Face import requires a model or dataset repository".into(),
        ));
    }
    let requested_paths = request.paths.iter().cloned().collect::<HashSet<_>>();
    if requested_paths.len() != request.paths.len() {
        return Err(ApiError::InvalidRequest(
            "import paths must be unique".into(),
        ));
    }
    for path in &request.paths {
        validate_commit_path(path)?;
    }

    let provider = HuggingFaceHub::new(state.http.clone());
    let token = crate::secrets::resolve_provider_api_key(&state, &name, provider.id()).await?;
    let discovered = provider
        .list_files(
            &request.repo_id,
            &repo.kind,
            &request.revision,
            token.as_deref(),
        )
        .await?;
    let mut files = discovered
        .into_iter()
        .filter(|file| requested_paths.is_empty() || requested_paths.contains(&file.path))
        .collect::<Vec<_>>();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    if !requested_paths.is_empty() {
        let found = files.iter().map(|file| &file.path).collect::<HashSet<_>>();
        let missing = requested_paths
            .iter()
            .filter(|path| !found.contains(path))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(ApiError::InvalidRequest(format!(
                "Hub paths were not found: {}",
                missing.join(", ")
            )));
        }
    }
    if files.is_empty() {
        return Err(ApiError::InvalidRequest(
            "Hub import selected no files".into(),
        ));
    }
    if files.len() > request.max_files {
        return Err(ApiError::InvalidRequest(format!(
            "Hub import selected {} files; max_files is {}",
            files.len(),
            request.max_files
        )));
    }
    let logical_bytes = files
        .iter()
        .try_fold(0u64, |total, file| total.checked_add(file.size))
        .ok_or_else(|| ApiError::InvalidRequest("Hub repository size overflow".into()))?;
    if logical_bytes > request.max_total_bytes {
        return Err(ApiError::InvalidRequest(format!(
            "Hub import is {logical_bytes} bytes; max_total_bytes is {}",
            request.max_total_bytes
        )));
    }
    let blocked = files
        .iter()
        .filter(|file| {
            matches!(
                file.security.as_ref().map(|status| status.status.as_str()),
                Some("unsafe" | "suspicious" | "caution")
            )
        })
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    if !request.allow_unsafe && !blocked.is_empty() {
        return Err(ApiError::InvalidRequest(format!(
            "Hub security scan blocked: {}",
            blocked.join(", ")
        )));
    }

    let threshold = u64::try_from(repo.large_file_threshold_bytes)
        .map_err(|_| ApiError::Internal("repository artifact threshold is invalid".into()))?;
    let mut changes = Vec::with_capacity(files.len());
    let mut arachne_files = 0u64;
    let mut security_counts = BTreeMap::new();
    for file in &files {
        validate_commit_path(&file.path)?;
        let status = file
            .security
            .as_ref()
            .map(|value| value.status.as_str())
            .unwrap_or("unscanned");
        *security_counts.entry(status.to_string()).or_insert(0) += 1;
        let response = provider
            .download(
                &request.repo_id,
                &repo.kind,
                &request.revision,
                &file.path,
                token.as_deref(),
            )
            .await?;
        let content = if file.size >= threshold && file.size > 0 {
            let (pointer, outcome) = crate::arachne::store_http_payload(
                &state,
                response,
                file.size,
                request.max_total_bytes,
            )
            .await?;
            arachne_files += 1;
            tracing::info!(
                repo = %name,
                source = %request.repo_id,
                path = %file.path,
                bytes = outcome.file_size,
                deduped_bytes = outcome.deduped_bytes,
                "Hub artifact streamed into Arachne"
            );
            pointer
        } else {
            let bytes = response
                .bytes()
                .await
                .map_err(|err| ApiError::Upstream(format!("Hub file download: {err}")))?;
            if bytes.len() as u64 != file.size {
                return Err(ApiError::Upstream(format!(
                    "Hub file {:?} size mismatch: expected {}, received {}",
                    file.path,
                    file.size,
                    bytes.len()
                )));
            }
            bytes.to_vec()
        };
        changes.push(FileChange {
            path: file.path.clone(),
            content,
            executable: false,
        });
    }

    let mut vcs = state.vcs.clone();
    let commit = vcs
        .commit(CommitRequest {
            repo: name.clone(),
            parent_commit_ids: vec![],
            files: changes,
            deleted_paths: vec![],
            message: format!(
                "import {}/{} from Hugging Face",
                request.repo_id, request.revision
            ),
            author_name: "clotho-hub-import".into(),
            author_email: "hub@clotho.internal".into(),
        })
        .await?
        .into_inner();
    let mut queue = state.queue.clone();
    let submitted = queue
        .submit_change(SubmitChangeRequest {
            repo: name.clone(),
            commit_id: commit.commit_id.clone(),
        })
        .await?
        .into_inner();
    if let Some(pool) = &state.pool {
        control::log_activity(
            pool,
            ActivityEventInput {
                actor_id: auth.user_id,
                org_id: Some(repo.org_id),
                repo_id: Some(repo.id),
                event_type: "repo.hub_imported".into(),
                payload: serde_json::json!({
                    "repo_name": name,
                    "provider": provider.id(),
                    "source_repo_id": request.repo_id,
                    "source_revision": request.revision,
                    "files": files.len(),
                    "logical_bytes": logical_bytes,
                    "arachne_files": arachne_files,
                    "security_counts": security_counts,
                }),
            },
        )
        .await?;
    }
    Ok(Json(ImportHubResponse {
        provider: provider.id().into(),
        source_repo_id: request.repo_id,
        source_revision: request.revision,
        commit_id: submitted.commit_id,
        operation_id: submitted.operation_id,
        files_imported: files.len() as u64,
        logical_bytes,
        arachne_files,
        security_counts,
        fast_forwarded: submitted.fast_forwarded,
        conflicted: submitted.conflicted,
        conflicted_paths: submitted.conflicted_paths,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_accepts_canonical_hugging_face_ids_and_safe_paths() {
        assert_eq!(
            HuggingFaceHub::tree_url("openai/gpt-oss", "model", "main")
                .unwrap()
                .as_str(),
            "https://huggingface.co/api/models/openai/gpt-oss/tree/main?recursive=true&expand=true&limit=100"
        );
        assert!(HuggingFaceHub::tree_url("org/data", "dataset", "refs/pr/3").is_ok());
        for bad in ["gpt", "a/b/c", "../x", "a/%2f"] {
            assert!(HuggingFaceHub::tree_url(bad, "model", "main").is_err());
        }
        assert!(HuggingFaceHub::download_url("a/b", "model", "main", "../secret").is_err());
    }

    #[test]
    fn cursor_parser_rejects_non_huggingface_links() {
        assert_eq!(
            next_cursor(
                "<https://huggingface.co/api/models/a/b/tree/main?cursor=abc>; rel=\"next\""
            ),
            Some("abc".into())
        );
        assert_eq!(
            next_cursor("<https://evil.example/tree?cursor=abc>; rel=\"next\""),
            None
        );
    }
}
