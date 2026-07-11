//! Clotho-owned Actions facade and compute provider metadata.
//!
//! Stage 10 turns Stage 7 CI from "just a commit status" into a Clotho run
//! record with logs and provider/sandbox metadata. This first store is
//! gateway-local; ADR-0012 calls out Postgres persistence as the next hardening
//! step once the API shape settles.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use clotho_common::pb::vcs::v1::GetHeadsRequest;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tokio::sync::Mutex;

use crate::auth;
use crate::control;
use crate::error::ApiError;
use crate::notifications;
use crate::{ci, AppState};

#[derive(Clone)]
pub struct ActionsDefaults {
    pub provider: String,
    pub default_image: String,
    pub timeout_seconds: u32,
    /// Env-derived configured flags keyed by provider id (no secrets).
    /// Used as a fallback when clotho-compute ListProviders is unreachable.
    pub configured_providers: HashMap<String, bool>,
}

impl ActionsDefaults {
    pub fn default_image_or_fallback(&self) -> String {
        if self.default_image.is_empty() {
            "ubuntu:22.04".into()
        } else {
            self.default_image.clone()
        }
    }

    pub fn is_configured(&self, provider: &str) -> bool {
        self.configured_providers
            .get(&provider.to_lowercase())
            .copied()
            .unwrap_or(false)
    }
}

pub struct ActionsState {
    next_id: AtomicU64,
    runs: Mutex<HashMap<String, ActionRun>>,
    logs: Mutex<HashMap<String, String>>,
    configs: Mutex<HashMap<String, ActionsConfig>>,
    defaults: ActionsDefaults,
    pool: Option<PgPool>,
}

pub(crate) struct NewActionRun {
    pub repo: String,
    pub commit_id: String,
    pub branch: String,
    pub trigger: String,
    pub actor: String,
    pub workflow: String,
    pub release_version: String,
    pub release_manifest_sha256: String,
}

impl ActionsState {
    pub fn default_provider(&self) -> String {
        self.defaults.provider.clone()
    }

    pub fn provider_configured(&self, provider: &str) -> bool {
        self.defaults.is_configured(provider)
    }

    pub fn new(defaults: ActionsDefaults) -> Self {
        Self {
            next_id: AtomicU64::new(1),
            runs: Mutex::new(HashMap::new()),
            logs: Mutex::new(HashMap::new()),
            configs: Mutex::new(HashMap::new()),
            defaults,
            pool: None,
        }
    }

    pub fn with_pool(defaults: ActionsDefaults, pool: PgPool) -> Self {
        Self {
            pool: Some(pool),
            ..Self::new(defaults)
        }
    }

    pub async fn config_for(&self, repo: &str) -> ActionsConfig {
        if let Some(pool) = &self.pool {
            match sqlx::query(
                r#"
                select enabled, provider, default_image, timeout_seconds,
                       accelerator, gpu_types
                from actions_configs
                where repo = $1
                "#,
            )
            .bind(repo)
            .fetch_optional(pool)
            .await
            {
                Ok(Some(row)) => {
                    return ActionsConfig {
                        enabled: row.get("enabled"),
                        provider: row.get("provider"),
                        default_image: row.get("default_image"),
                        timeout_seconds: row.get::<i32, _>("timeout_seconds").max(0) as u32,
                        accelerator: row.get("accelerator"),
                        gpu_types: row.get("gpu_types"),
                    };
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(%repo, error = %e, "failed to load actions config"),
            }
        }
        self.configs
            .lock()
            .await
            .get(repo)
            .cloned()
            .unwrap_or_else(|| ActionsConfig {
                enabled: true,
                provider: self.defaults.provider.clone(),
                default_image: self.defaults.default_image_or_fallback(),
                timeout_seconds: self.defaults.timeout_seconds,
                accelerator: "cpu".into(),
                gpu_types: vec![],
            })
    }

    pub async fn set_config(&self, repo: String, config: ActionsConfig) {
        if let Some(pool) = &self.pool {
            match sqlx::query(
                r#"
                insert into actions_configs
                    (repo, enabled, provider, default_image, timeout_seconds,
                     accelerator, gpu_types)
                values ($1, $2, $3, $4, $5, $6, $7)
                on conflict (repo) do update set
                    enabled = excluded.enabled,
                    provider = excluded.provider,
                    default_image = excluded.default_image,
                    timeout_seconds = excluded.timeout_seconds,
                    accelerator = excluded.accelerator,
                    gpu_types = excluded.gpu_types,
                    updated_at = now()
                "#,
            )
            .bind(&repo)
            .bind(config.enabled)
            .bind(&config.provider)
            .bind(&config.default_image)
            .bind(config.timeout_seconds as i32)
            .bind(&config.accelerator)
            .bind(&config.gpu_types)
            .execute(pool)
            .await
            {
                Ok(_) => return,
                Err(e) => tracing::warn!(%repo, error = %e, "failed to persist actions config"),
            }
        }
        self.configs.lock().await.insert(repo, config);
    }

    pub async fn create_run(&self, new: NewActionRun) -> ActionRun {
        let config = self.config_for(&new.repo).await;
        let id = match self.next_run_id().await {
            Some(id) => id,
            None => format!("run-{}", self.next_id.fetch_add(1, Ordering::Relaxed)),
        };
        let run = ActionRun {
            id: id.clone(),
            repo: new.repo,
            commit_id: new.commit_id,
            branch: new.branch,
            status: "queued".into(),
            conclusion: String::new(),
            trigger: new.trigger,
            actor: new.actor,
            workflow: new.workflow.clone(),
            release_version: new.release_version,
            release_manifest_sha256: new.release_manifest_sha256,
            provider: config.provider,
            sandbox_id: String::new(),
            created_at_millis: now_millis(),
            started_at_millis: 0,
            finished_at_millis: 0,
            duration_ms: 0,
            jobs: vec![ActionJob {
                id: format!("{id}-job-1"),
                run_id: id.clone(),
                name: format!("clotho-{}", new.workflow),
                status: "queued".into(),
                exit_code: None,
            }],
            log_text: String::new(),
        };
        if let Some(pool) = &self.pool {
            let jobs = serde_json::to_value(&run.jobs).unwrap_or_else(|_| serde_json::json!([]));
            match sqlx::query(
                r#"
                insert into action_runs
                    (id, repo, commit_id, branch, status, conclusion, trigger, actor,
                     workflow, release_version, release_manifest_sha256,
                     provider, sandbox_id, created_at_millis, started_at_millis,
                     finished_at_millis, duration_ms, jobs)
                values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
                "#,
            )
            .bind(&run.id)
            .bind(&run.repo)
            .bind(&run.commit_id)
            .bind(&run.branch)
            .bind(&run.status)
            .bind(&run.conclusion)
            .bind(&run.trigger)
            .bind(&run.actor)
            .bind(&run.workflow)
            .bind(&run.release_version)
            .bind(&run.release_manifest_sha256)
            .bind(&run.provider)
            .bind(&run.sandbox_id)
            .bind(run.created_at_millis)
            .bind(run.started_at_millis)
            .bind(run.finished_at_millis)
            .bind(run.duration_ms as i64)
            .bind(jobs)
            .execute(pool)
            .await
            {
                Ok(_) => {
                    if let Err(e) =
                        sqlx::query("insert into action_logs (run_id, log_text) values ($1, '')")
                            .bind(&run.id)
                            .execute(pool)
                            .await
                    {
                        tracing::warn!(run_id = %run.id, error = %e, "failed to initialize action log");
                    }
                    return run;
                }
                Err(e) => {
                    tracing::warn!(run_id = %run.id, error = %e, "failed to persist action run")
                }
            }
        }
        self.runs.lock().await.insert(id.clone(), run.clone());
        self.logs.lock().await.insert(id, String::new());
        run
    }

    pub async fn mark_running(&self, run_id: &str) {
        if let Some(pool) = &self.pool {
            match db_run_by_id(pool, run_id).await {
                Ok(Some(mut run)) => {
                    run.status = "running".into();
                    run.started_at_millis = now_millis();
                    if let Some(job) = run.jobs.first_mut() {
                        job.status = "running".into();
                    }
                    if let Err(e) = update_run(pool, &run).await {
                        tracing::warn!(%run_id, error = %e, "failed to mark action run running");
                    }
                    return;
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(%run_id, error = %e, "failed to load action run"),
            }
        }
        if let Some(run) = self.runs.lock().await.get_mut(run_id) {
            run.status = "running".into();
            run.started_at_millis = now_millis();
            if let Some(job) = run.jobs.first_mut() {
                job.status = "running".into();
            }
        }
    }

    pub async fn finish_run(&self, run_id: &str, update: FinishedRun) {
        let finished = now_millis();
        if let Some(pool) = &self.pool {
            match db_run_by_id(pool, run_id).await {
                Ok(Some(mut run)) => {
                    apply_finished(&mut run, update, finished);
                    if let Err(e) = update_run(pool, &run).await {
                        tracing::warn!(%run_id, error = %e, "failed to persist finished action run");
                    }
                    if let Err(e) = sqlx::query(
                        r#"
                        insert into action_logs (run_id, log_text)
                        values ($1, $2)
                        on conflict (run_id) do update set
                            log_text = excluded.log_text,
                            updated_at = now()
                        "#,
                    )
                    .bind(run_id)
                    .bind(&run.log_text)
                    .execute(pool)
                    .await
                    {
                        tracing::warn!(%run_id, error = %e, "failed to persist action log");
                    }
                    if run.conclusion == "failure" || run.conclusion == "failed" {
                        let notify_user_id = control::get_repo_by_name(pool, &run.repo)
                            .await
                            .map(|r| r.created_by)
                            .unwrap_or_else(|_| String::new());
                        let notify_user_id = if notify_user_id.is_empty() {
                            notifications::user_id_by_name(pool, "clotho")
                                .await
                                .unwrap_or_default()
                        } else {
                            notify_user_id
                        };
                        if !notify_user_id.is_empty() {
                            notifications::notify_action_failed(
                                pool,
                                &run.repo,
                                run_id,
                                &run.conclusion,
                                &notify_user_id,
                            )
                            .await;
                        }
                    }
                    return;
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(%run_id, error = %e, "failed to load action run"),
            }
        }
        if let Some(run) = self.runs.lock().await.get_mut(run_id) {
            let logs = update.logs.clone();
            apply_finished(run, update, finished);
            self.logs.lock().await.insert(run_id.to_string(), logs);
        }
    }

    async fn list_runs(&self, repo: &str, limit: i64, before: Option<i64>) -> Vec<ActionRun> {
        if let Some(pool) = &self.pool {
            let result = if let Some(before) = before {
                sqlx::query(
                    r#"
                    select id, repo, commit_id, branch, status, conclusion, trigger, actor,
                           workflow, release_version, release_manifest_sha256,
                           provider, sandbox_id, created_at_millis, started_at_millis,
                           finished_at_millis, duration_ms, jobs
                    from action_runs
                    where repo = $1 and created_at_millis < $2
                    order by created_at_millis desc, id desc
                    limit $3
                    "#,
                )
                .bind(repo)
                .bind(before)
                .bind(limit)
                .fetch_all(pool)
                .await
            } else {
                sqlx::query(
                    r#"
                    select id, repo, commit_id, branch, status, conclusion, trigger, actor,
                           workflow, release_version, release_manifest_sha256,
                           provider, sandbox_id, created_at_millis, started_at_millis,
                           finished_at_millis, duration_ms, jobs
                    from action_runs
                    where repo = $1
                    order by created_at_millis desc, id desc
                    limit $2
                    "#,
                )
                .bind(repo)
                .bind(limit)
                .fetch_all(pool)
                .await
            };
            match result {
                Ok(rows) => return rows.into_iter().map(row_to_run).collect(),
                Err(e) => tracing::warn!(%repo, error = %e, "failed to list action runs"),
            }
        }
        let mut runs: Vec<_> = self
            .runs
            .lock()
            .await
            .values()
            .filter(|run| run.repo == repo)
            .cloned()
            .collect();
        runs.sort_by_key(|run| Reverse(run.created_at_millis));
        if let Some(before) = before {
            runs.retain(|run| run.created_at_millis < before);
        }
        runs.truncate(limit.max(0) as usize);
        runs
    }

    pub(crate) async fn get_run(&self, repo: &str, run_id: &str) -> Result<ActionRun, ApiError> {
        if let Some(pool) = &self.pool {
            match db_run_by_id(pool, run_id).await {
                Ok(Some(run)) if run.repo == repo => return Ok(run),
                Ok(_) => {}
                Err(e) => tracing::warn!(%repo, %run_id, error = %e, "failed to get action run"),
            }
        }
        self.runs
            .lock()
            .await
            .get(run_id)
            .filter(|run| run.repo == repo)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("action run {run_id:?} not found")))
    }

    async fn logs_for(&self, repo: &str, run_id: &str) -> Result<ActionLog, ApiError> {
        self.get_run(repo, run_id).await?;
        if let Some(pool) = &self.pool {
            match sqlx::query("select log_text from action_logs where run_id = $1")
                .bind(run_id)
                .fetch_optional(pool)
                .await
            {
                Ok(Some(row)) => {
                    return Ok(ActionLog {
                        run_id: run_id.into(),
                        text: row.get("log_text"),
                    });
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(%repo, %run_id, error = %e, "failed to load action log"),
            }
        }
        let text = self
            .logs
            .lock()
            .await
            .get(run_id)
            .cloned()
            .unwrap_or_default();
        Ok(ActionLog {
            run_id: run_id.into(),
            text,
        })
    }

    async fn next_run_id(&self) -> Option<String> {
        let pool = self.pool.as_ref()?;
        match sqlx::query_scalar::<_, i64>("select nextval('clotho_action_run_seq')")
            .fetch_one(pool)
            .await
        {
            Ok(id) => Some(format!("run-{id}")),
            Err(e) => {
                tracing::warn!(error = %e, "failed to allocate durable action run id");
                None
            }
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ActionRun {
    pub id: String,
    pub repo: String,
    pub commit_id: String,
    pub branch: String,
    pub status: String,
    pub conclusion: String,
    pub trigger: String,
    pub actor: String,
    pub workflow: String,
    pub release_version: String,
    pub release_manifest_sha256: String,
    pub provider: String,
    pub sandbox_id: String,
    pub created_at_millis: i64,
    pub started_at_millis: i64,
    pub finished_at_millis: i64,
    pub duration_ms: u64,
    pub jobs: Vec<ActionJob>,
    #[serde(skip)]
    log_text: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ActionJob {
    pub id: String,
    pub run_id: String,
    pub name: String,
    pub status: String,
    pub exit_code: Option<i32>,
}

pub struct FinishedRun {
    pub status: String,
    pub conclusion: String,
    pub exit_code: Option<i32>,
    pub logs: String,
    pub provider: String,
    pub sandbox_id: String,
}

#[derive(Serialize)]
pub struct ActionLog {
    pub run_id: String,
    pub text: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ActionsConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub default_image: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u32,
    #[serde(default = "default_accelerator")]
    pub accelerator: String,
    #[serde(default)]
    pub gpu_types: Vec<String>,
}

fn default_enabled() -> bool {
    true
}

fn default_provider() -> String {
    "daytona".into()
}

fn default_timeout() -> u32 {
    900
}

fn default_accelerator() -> String {
    "cpu".into()
}

#[derive(Serialize)]
pub struct ActionRunListResponse {
    pub runs: Vec<ActionRun>,
    pub next_cursor: Option<i64>,
}

#[derive(Deserialize)]
pub struct ListRunsQuery {
    pub limit: Option<u32>,
    pub before: Option<i64>,
}

#[derive(Deserialize)]
pub struct CreateActionRunRequest {
    #[serde(default)]
    pub commit_id: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default = "default_manual_actor")]
    pub actor: String,
    #[serde(default = "default_workflow")]
    pub workflow: String,
    #[serde(default)]
    pub release_version: String,
}

fn default_branch() -> String {
    "main".into()
}

fn default_manual_actor() -> String {
    "manual".into()
}

fn default_workflow() -> String {
    "ci".into()
}

#[derive(Clone, Serialize)]
pub struct ProviderCapabilitiesJson {
    pub one_shot_jobs: bool,
    pub persistent_workspaces: bool,
    pub snapshots: bool,
    pub templates: bool,
    pub regions: Vec<String>,
    pub ssh: bool,
    pub desktop: bool,
    pub public_url: bool,
    pub file_api: bool,
    pub terminal_streaming: bool,
    pub gpu: bool,
    pub gpu_types: Vec<String>,
    pub cost_hints: String,
}

impl ProviderCapabilitiesJson {
    pub fn feature_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        if self.one_shot_jobs {
            tags.push("one-shot-jobs".into());
        }
        if self.persistent_workspaces {
            tags.push("persistent-workspaces".into());
        }
        if self.snapshots {
            tags.push("snapshots".into());
        }
        if self.templates {
            tags.push("templates".into());
        }
        if self.ssh {
            tags.push("ssh".into());
        }
        if self.desktop {
            tags.push("desktop".into());
        }
        if self.public_url {
            tags.push("public-url".into());
        }
        if self.file_api {
            tags.push("file-api".into());
        }
        if self.terminal_streaming {
            tags.push("terminal-streaming".into());
        }
        if self.gpu {
            tags.push("gpu".into());
        }
        if !self.gpu_types.is_empty() {
            tags.push(format!("gpu-types:{}", self.gpu_types.join(",")));
        }
        tags
    }
}

#[derive(Clone, Serialize)]
pub struct ComputeProviderJson {
    pub id: String,
    pub name: String,
    /// Implementation kind: `direct`, `bridge`, or `stub`.
    pub kind: String,
    pub enabled: bool,
    pub configured: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub configured_reason: String,
    /// Flat capability tags for simple UI badges (backward compatible).
    pub capabilities: Vec<String>,
    /// Structured capability flags (Stage 12).
    pub capability_detail: ProviderCapabilitiesJson,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub default_snapshot: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

#[derive(Serialize)]
pub struct ComputeProviderListResponse {
    pub providers: Vec<ComputeProviderJson>,
    pub default_provider_id: String,
}

pub async fn list_runs(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<ListRunsQuery>,
) -> Result<Json<ActionRunListResponse>, ApiError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 100) as i64;
    let runs = state.actions.list_runs(&name, limit, query.before).await;
    let next_cursor = if runs.len() == limit as usize {
        runs.last().map(|run| run.created_at_millis)
    } else {
        None
    };
    Ok(Json(ActionRunListResponse { runs, next_cursor }))
}

pub async fn get_run(
    State(state): State<Arc<AppState>>,
    Path((name, run_id)): Path<(String, String)>,
) -> Result<Json<ActionRun>, ApiError> {
    Ok(Json(state.actions.get_run(&name, &run_id).await?))
}

pub async fn get_logs(
    State(state): State<Arc<AppState>>,
    Path((name, run_id)): Path<(String, String)>,
) -> Result<Json<ActionLog>, ApiError> {
    Ok(Json(state.actions.logs_for(&name, &run_id).await?))
}

pub async fn create_run(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(req): Json<CreateActionRunRequest>,
) -> Result<(StatusCode, Json<ActionRun>), ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let repo = if let Some(pool) = &state.pool {
        let repo = control::require_repo_permission(pool, &name, &auth.user_id, "write").await?;
        if repo.repo.network_mode == "tailscale" {
            if !crate::providers::tailscale_configured(&state, Some(&repo.org_name)).await {
                return Err(ApiError::InvalidRequest(
                    "repository requires Tailscale private networking, but its org OAuth client is not live-verified"
                        .into(),
                ));
            }
            return Err(ApiError::InvalidRequest(
                "repository requires Tailscale private networking; no configured compute provider currently advertises private-net attachment"
                    .into(),
            ));
        }
        Some(repo)
    } else {
        None
    };
    let config = state.actions.config_for(&name).await;
    if !config.enabled {
        return Err(ApiError::InvalidRequest(
            "actions are disabled for this repo".into(),
        ));
    }

    let workflow = req.workflow.trim().to_ascii_lowercase();
    if !matches!(
        workflow.as_str(),
        "ci" | "evaluate" | "inference" | "benchmark"
    ) {
        return Err(ApiError::InvalidRequest(
            "workflow must be ci, evaluate, inference, or benchmark".into(),
        ));
    }
    let release_version = req.release_version.trim().to_string();
    if workflow != "ci" && release_version.is_empty() {
        return Err(ApiError::InvalidRequest(format!(
            "{workflow} runs must be pinned to an immutable release"
        )));
    }
    let release_binding = if release_version.is_empty() {
        None
    } else {
        let pool = state.pool.as_ref().ok_or_else(|| {
            ApiError::Internal("release-pinned Actions require the control plane".into())
        })?;
        let repo = repo.as_ref().ok_or_else(|| {
            ApiError::Internal("release-pinned Actions require repository metadata".into())
        })?;
        let binding =
            crate::releases::release_binding(pool, &repo.repo.id, &release_version).await?;
        if !binding.ready {
            return Err(ApiError::Conflict(format!(
                "release {release_version:?} is not marked ready"
            )));
        }
        Some(binding)
    };
    let commit_id = if let Some(binding) = &release_binding {
        if !req.commit_id.trim().is_empty() && req.commit_id != binding.commit_id {
            return Err(ApiError::Conflict(
                "commit_id does not match the immutable release".into(),
            ));
        }
        binding.commit_id.clone()
    } else if req.commit_id.trim().is_empty() {
        let mut vcs = state.vcs.clone();
        let heads = vcs
            .get_heads(GetHeadsRequest { repo: name.clone() })
            .await?
            .into_inner();
        heads.main_commit_id
    } else {
        req.commit_id
    };
    if commit_id.is_empty() {
        return Err(ApiError::InvalidRequest(
            "commit_id is required for an unborn repository".into(),
        ));
    }

    let run = state
        .actions
        .create_run(NewActionRun {
            repo: name.clone(),
            commit_id: commit_id.clone(),
            branch: req.branch,
            trigger: if release_binding.is_some() {
                "release".into()
            } else {
                "manual".into()
            },
            actor: req.actor,
            workflow,
            release_version,
            release_manifest_sha256: release_binding
                .map(|binding| binding.manifest_sha256)
                .unwrap_or_default(),
        })
        .await;
    tokio::spawn(ci::run_existing(state, run.id.clone(), name, commit_id));
    Ok((StatusCode::ACCEPTED, Json(run)))
}

pub async fn get_config(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ActionsConfig>, ApiError> {
    Ok(Json(state.actions.config_for(&name).await))
}

pub async fn put_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(mut config): Json<ActionsConfig>,
) -> Result<Json<ActionsConfig>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    if let Some(pool) = &state.pool {
        control::require_repo_permission(pool, &name, &auth.user_id, "admin").await?;
    }
    if config.provider.trim().is_empty() {
        config.provider = state.actions.defaults.provider.clone();
    }
    if config.default_image.trim().is_empty() {
        config.default_image = state.actions.defaults.default_image_or_fallback();
    }
    if config.timeout_seconds == 0 {
        config.timeout_seconds = state.actions.defaults.timeout_seconds;
    }
    if !matches!(config.accelerator.as_str(), "cpu" | "gpu") {
        return Err(ApiError::InvalidRequest(
            "accelerator must be cpu or gpu".into(),
        ));
    }
    if config.accelerator == "cpu" {
        config.gpu_types.clear();
    } else {
        let providers = list_providers_for(&state).await;
        let provider = providers
            .providers
            .iter()
            .find(|provider| provider.id.eq_ignore_ascii_case(&config.provider))
            .ok_or_else(|| {
                ApiError::InvalidRequest(format!(
                    "compute provider {:?} is not registered",
                    config.provider
                ))
            })?;
        if !provider.capability_detail.gpu {
            return Err(ApiError::InvalidRequest(format!(
                "compute provider {:?} does not advertise GPU jobs",
                config.provider
            )));
        }
        for gpu_type in &config.gpu_types {
            if !provider
                .capability_detail
                .gpu_types
                .iter()
                .any(|supported| supported.eq_ignore_ascii_case(gpu_type))
            {
                return Err(ApiError::InvalidRequest(format!(
                    "GPU type {gpu_type:?} is not advertised by provider {:?}",
                    config.provider
                )));
            }
        }
    }
    state.actions.set_config(name, config.clone()).await;
    Ok(Json(config))
}

/// List every registered compute provider (Stage 12 registry).
///
/// Prefers live metadata from clotho-compute (`ListProviders`); falls back to
/// env-derived descriptors when the compute service is unreachable so settings
/// pages still render in partial-stack dev.
pub async fn providers(State(state): State<Arc<AppState>>) -> Json<ComputeProviderListResponse> {
    Json(list_providers_for(&state).await)
}

pub async fn provider(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<Json<ComputeProviderJson>, ApiError> {
    let list = list_providers_for(&state).await;
    list.providers
        .into_iter()
        .find(|p| p.id.eq_ignore_ascii_case(&provider))
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("compute provider {provider:?} not found")))
}

pub async fn list_providers_for(state: &AppState) -> ComputeProviderListResponse {
    let mut list = match fetch_providers_from_compute(state).await {
        Ok(list) if !list.providers.is_empty() => list,
        Ok(_) => fallback_providers(state),
        Err(e) => {
            tracing::warn!(error = %e, "compute ListProviders failed; using env fallback");
            fallback_providers(state)
        }
    };
    // Overlay Clotho-stored secrets so settings show Configured without env keys.
    overlay_secret_configured(state, &mut list).await;
    list
}

/// Mark providers as configured when a Clotho secret is present (docs/adr/0014).
///
/// **Honest rule:** only mark configured when the secret path can actually run
/// jobs. For ComputeSDK that also requires a bridge URL (env) so clotho-compute
/// can reach the sidecar; secrets alone are not enough.
async fn overlay_secret_configured(state: &AppState, list: &mut ComputeProviderListResponse) {
    let bridge_url_set = std::env::var("CLOTHO_COMPUTE_SDK_BRIDGE_URL")
        .map(|u| !u.trim().is_empty())
        .unwrap_or(false);

    for p in &mut list.providers {
        if p.id == "computesdk" {
            let secrets_ready = crate::secrets::computesdk_secrets_ready(state).await;
            if p.configured {
                // Live bridge health already said ready (env keys on sidecar).
                continue;
            }
            let bridge_down = p.configured_reason.to_lowercase().contains("unreachable")
                || p.configured_reason.to_lowercase().contains("not running");
            if bridge_down {
                // Do not claim configured when the sidecar is down.
                if secrets_ready {
                    p.configured_reason = format!(
                        "{}; upstream keys are stored in Clotho",
                        p.configured_reason
                    );
                }
                continue;
            }
            if secrets_ready && bridge_url_set {
                // Bridge may report "no upstream" from process env while Clotho
                // secrets will be injected per job — that is enough to run.
                p.configured = true;
                if let Some(meta) =
                    crate::secrets::provider_secret_configured(state, "computesdk").await
                {
                    p.configured_reason = if meta.value_last4.is_empty() {
                        "connected via Clotho secret".into()
                    } else {
                        format!("connected · ···{}", meta.value_last4)
                    };
                } else {
                    p.configured_reason = "connected via Clotho secret".into();
                }
            } else if secrets_ready && !bridge_url_set {
                p.configured = false;
                p.configured_reason =
                    "upstream keys stored; start the ComputeSDK bridge (`just dev-compute-bridge`)"
                        .into();
            } else if p.configured_reason.is_empty() {
                p.configured_reason =
                    "connect ComputeSDK upstream keys in Clotho settings (and run the bridge)"
                        .into();
            }
            continue;
        }

        if p.configured {
            continue;
        }
        if let Some(meta) = crate::secrets::provider_secret_configured(state, &p.id).await {
            p.configured = true;
            p.configured_reason = if meta.value_last4.is_empty() {
                "connected via Clotho secret".into()
            } else {
                format!("connected · ···{}", meta.value_last4)
            };
            if !p.notes.contains("Clotho secret") {
                p.notes = format!(
                    "{}; credentials from Clotho secrets store",
                    p.notes.trim_end_matches(';')
                )
                .trim_start_matches("; ")
                .to_string();
            }
        }
    }
}

async fn fetch_providers_from_compute(
    state: &AppState,
) -> Result<ComputeProviderListResponse, String> {
    use clotho_common::pb::compute::v1::ListProvidersRequest;

    let resp = state
        .compute
        .clone()
        .list_providers(ListProvidersRequest {})
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();

    let providers = resp.providers.into_iter().map(provider_from_pb).collect();
    Ok(ComputeProviderListResponse {
        providers,
        default_provider_id: if resp.default_provider_id.is_empty() {
            state.actions.defaults.provider.clone()
        } else {
            resp.default_provider_id
        },
    })
}

fn provider_from_pb(p: clotho_common::pb::compute::v1::ProviderInfo) -> ComputeProviderJson {
    let detail = match p.capabilities {
        Some(c) => ProviderCapabilitiesJson {
            one_shot_jobs: c.one_shot_jobs,
            persistent_workspaces: c.persistent_workspaces,
            snapshots: c.snapshots,
            templates: c.templates,
            regions: c.regions,
            ssh: c.ssh,
            desktop: c.desktop,
            public_url: c.public_url,
            file_api: c.file_api,
            terminal_streaming: c.terminal_streaming,
            gpu: c.gpu,
            gpu_types: c.gpu_types,
            cost_hints: c.cost_hints,
        },
        None => ProviderCapabilitiesJson {
            one_shot_jobs: false,
            persistent_workspaces: false,
            snapshots: false,
            templates: false,
            regions: vec![],
            ssh: false,
            desktop: false,
            public_url: false,
            file_api: false,
            terminal_streaming: false,
            gpu: false,
            gpu_types: vec![],
            cost_hints: String::new(),
        },
    };
    let capabilities = detail.feature_tags();
    ComputeProviderJson {
        id: p.id,
        name: p.name,
        kind: p.kind,
        enabled: p.enabled,
        configured: p.configured,
        configured_reason: p.configured_reason,
        capabilities,
        capability_detail: detail,
        default_snapshot: p.default_snapshot,
        notes: p.notes,
    }
}

fn fallback_providers(state: &AppState) -> ComputeProviderListResponse {
    let default = state.actions.defaults.provider.clone();
    let mut providers = vec![
        env_provider(
            "daytona",
            "Daytona",
            "direct",
            &default,
            state.actions.defaults.is_configured("daytona"),
            "not connected — add credentials in Clotho settings",
            ProviderCapabilitiesJson {
                one_shot_jobs: true,
                persistent_workspaces: true,
                snapshots: true,
                templates: false,
                regions: vec![],
                ssh: false,
                desktop: false,
                public_url: false,
                file_api: true,
                terminal_streaming: false,
                gpu: true,
                gpu_types: vec![
                    "H100".into(),
                    "H200".into(),
                    "RTX-PRO-6000".into(),
                    "RTX-4090".into(),
                    "RTX-5090".into(),
                ],
                cost_hints: "Daytona cloud sandbox (API-key billed)".into(),
            },
            state.actions.defaults.default_image_or_fallback(),
            "direct Rust provider (env fallback; clotho-compute unreachable)".into(),
        ),
        env_provider(
            "computesdk",
            "ComputeSDK Bridge",
            "bridge",
            &default,
            state.actions.defaults.is_configured("computesdk"),
            "bridge not ready — start with `just dev-compute-bridge` and connect upstream keys",
            ProviderCapabilitiesJson {
                one_shot_jobs: true,
                persistent_workspaces: false,
                snapshots: true,
                templates: true,
                regions: vec![],
                ssh: false,
                desktop: false,
                public_url: false,
                file_api: true,
                terminal_streaming: false,
                gpu: false,
                gpu_types: vec![],
                cost_hints: "depends on upstream ComputeSDK provider".into(),
            },
            String::new(),
            "optional TypeScript sidecar (env fallback; clotho-compute unreachable)".into(),
        ),
        env_provider(
            "box",
            "Box",
            "direct",
            &default,
            state.actions.defaults.is_configured("box"),
            "not connected — add credentials in Clotho settings",
            ProviderCapabilitiesJson {
                one_shot_jobs: true,
                persistent_workspaces: true,
                snapshots: true,
                templates: false,
                regions: vec![],
                ssh: true,
                desktop: true,
                public_url: true,
                file_api: true,
                terminal_streaming: true,
                gpu: false,
                gpu_types: vec![],
                cost_hints: "persistent Ubuntu VM; TTL / pay-per-use (see Box dashboard)".into(),
            },
            String::new(),
            "ascii Box API v1 (https://ascii.dev/api/box/v1); credentials from Clotho secrets or env".into(),
        ),
    ];
    // Mark enabled only on the default id.
    for p in &mut providers {
        p.enabled = p.id == default;
    }
    ComputeProviderListResponse {
        providers,
        default_provider_id: default,
    }
}

#[allow(clippy::too_many_arguments)]
fn env_provider(
    id: &str,
    name: &str,
    kind: &str,
    default: &str,
    configured: bool,
    unconfigured_reason: &str,
    detail: ProviderCapabilitiesJson,
    default_snapshot: String,
    notes: String,
) -> ComputeProviderJson {
    let capabilities = detail.feature_tags();
    ComputeProviderJson {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        enabled: id == default,
        configured,
        configured_reason: if configured {
            String::new()
        } else {
            unconfigured_reason.into()
        },
        capabilities,
        capability_detail: detail,
        default_snapshot,
        notes,
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn apply_finished(run: &mut ActionRun, update: FinishedRun, finished: i64) {
    run.status = update.status;
    run.conclusion = update.conclusion;
    run.finished_at_millis = finished;
    run.duration_ms = if run.started_at_millis > 0 {
        (finished - run.started_at_millis).max(0) as u64
    } else {
        0
    };
    if !update.provider.is_empty() {
        run.provider = update.provider;
    }
    run.sandbox_id = update.sandbox_id;
    run.log_text = update.logs;
    if let Some(job) = run.jobs.first_mut() {
        job.status = run.status.clone();
        job.exit_code = update.exit_code;
    }
}

async fn db_run_by_id(pool: &PgPool, run_id: &str) -> Result<Option<ActionRun>, sqlx::Error> {
    sqlx::query(
        r#"
        select id, repo, commit_id, branch, status, conclusion, trigger, actor,
               workflow, release_version, release_manifest_sha256,
               provider, sandbox_id, created_at_millis, started_at_millis,
               finished_at_millis, duration_ms, jobs
        from action_runs
        where id = $1
        "#,
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(row_to_run))
}

async fn update_run(pool: &PgPool, run: &ActionRun) -> Result<(), sqlx::Error> {
    let jobs = serde_json::to_value(&run.jobs).unwrap_or_else(|_| serde_json::json!([]));
    sqlx::query(
        r#"
        update action_runs set
            status = $2,
            conclusion = $3,
            provider = $4,
            sandbox_id = $5,
            started_at_millis = $6,
            finished_at_millis = $7,
            duration_ms = $8,
            jobs = $9
        where id = $1
        "#,
    )
    .bind(&run.id)
    .bind(&run.status)
    .bind(&run.conclusion)
    .bind(&run.provider)
    .bind(&run.sandbox_id)
    .bind(run.started_at_millis)
    .bind(run.finished_at_millis)
    .bind(run.duration_ms as i64)
    .bind(jobs)
    .execute(pool)
    .await?;
    Ok(())
}

fn row_to_run(row: sqlx::postgres::PgRow) -> ActionRun {
    let jobs: serde_json::Value = row.get("jobs");
    ActionRun {
        id: row.get("id"),
        repo: row.get("repo"),
        commit_id: row.get("commit_id"),
        branch: row.get("branch"),
        status: row.get("status"),
        conclusion: row.get("conclusion"),
        trigger: row.get("trigger"),
        actor: row.get("actor"),
        workflow: row.get("workflow"),
        release_version: row.get("release_version"),
        release_manifest_sha256: row.get("release_manifest_sha256"),
        provider: row.get("provider"),
        sandbox_id: row.get("sandbox_id"),
        created_at_millis: row.get("created_at_millis"),
        started_at_millis: row.get("started_at_millis"),
        finished_at_millis: row.get("finished_at_millis"),
        duration_ms: row.get::<i64, _>("duration_ms").max(0) as u64,
        jobs: serde_json::from_value(jobs).unwrap_or_default(),
        log_text: String::new(),
    }
}
