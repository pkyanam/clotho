//! Agent identity: scoped tokens in Postgres, a model deliberately distinct
//! from human identities (docs/prd.md §2/§6 — never a flag on a user row),
//! plus the per-call audit log.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rand::RngExt as _;
use sha2::{Digest as _, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Bearer tokens look like `clotho_agt_<64 hex chars>` — prefixed so leaked
/// credentials are recognizable in scanners, with 256 bits of entropy.
const TOKEN_PREFIX: &str = "clotho_agt_";

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("agent {0:?} not found")]
    AgentNotFound(String),
    #[error("agent name {0:?} already exists")]
    AgentExists(String),
    #[error("token {0:?} not found for agent {1:?}")]
    TokenNotFound(Uuid, String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct Agent {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_at: Option<DateTime<Utc>>,
}

/// Token metadata — never includes the plaintext bearer secret.
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct TokenMeta {
    pub id: Uuid,
    /// Recognizable prefix for display (`clotho_agt_…`).
    pub token_prefix: String,
    pub allowed_repos: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

const TOKEN_DISPLAY_PREFIX: &str = "clotho_agt_";

#[derive(Debug, serde::Serialize)]
pub struct MintedToken {
    /// The plaintext bearer token — returned exactly once, never stored.
    pub token: String,
    pub token_id: Uuid,
    pub agent: String,
    pub allowed_repos: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// The identity a request authenticated as. Injected into the MCP request's
/// extensions by the auth middleware; every tool handler reads it from there.
#[derive(Debug, Clone)]
pub struct AuthedAgent {
    pub agent_id: Uuid,
    pub token_id: Uuid,
    pub name: String,
    pub allowed_repos: Vec<String>,
    pub allowed_tools: Vec<String>,
}

/// Opaque copy of the presented agent bearer used only while forwarding an
/// already-authorized MCP call to the canonical REST edge. Deliberately does
/// not implement `Debug`, `Display`, or `Serialize` so request diagnostics
/// cannot print credential plaintext by accident.
#[derive(Clone)]
pub struct ForwardedAgentBearer(Arc<str>);

impl ForwardedAgentBearer {
    pub(crate) fn new(token: String) -> Self {
        Self(token.into())
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl AuthedAgent {
    pub fn may_use_tool(&self, tool: &str) -> bool {
        scope_allows(&self.allowed_tools, tool)
    }

    pub fn may_touch_repo(&self, repo: &str) -> bool {
        scope_allows(&self.allowed_repos, repo)
    }
}

fn scope_allows(scope: &[String], value: &str) -> bool {
    scope.iter().any(|s| s == "*" || s == value)
}

/// Result of revalidating one presented agent credential against the exact
/// repository and MCP tool chosen by the API handler.
#[derive(Debug)]
pub enum AuthorizationDecision {
    Authorized(AuthedAgent),
    InvalidCredential,
    ScopeDenied,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: i64,
    pub agent_id: Uuid,
    pub token_id: Uuid,
    pub tool: String,
    pub repo: String,
    #[serde(serialize_with = "hex_bytes")]
    pub args_digest: Vec<u8>,
    pub status: String,
    pub error: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

fn hex_bytes<S: serde::Serializer>(bytes: &[u8], ser: S) -> Result<S::Ok, S::Error> {
    ser.serialize_str(&bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// One agent session's recent activity on a repo: an (agent, token) pair
/// aggregated over its audit-log entries.
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct RepoSession {
    pub agent: String,
    pub agent_id: Uuid,
    pub token_id: Uuid,
    pub last_tool: String,
    /// 'ok' | 'denied' | 'error'
    pub last_status: String,
    pub last_seen: DateTime<Utc>,
    pub first_seen: DateTime<Utc>,
    pub tool_calls: i64,
}

pub fn sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

#[derive(Clone)]
pub struct IdentityStore {
    pool: PgPool,
}

impl IdentityStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_agent(
        &self,
        name: &str,
        description: &str,
    ) -> Result<Agent, IdentityError> {
        sqlx::query_as::<_, Agent>(
            "insert into agents (name, description) values ($1, $2)
             returning id, name, description, created_at, disabled_at",
        )
        .bind(name)
        .bind(description)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                IdentityError::AgentExists(name.to_string())
            }
            _ => IdentityError::Db(e),
        })
    }

    /// Mint a scoped bearer token for an agent. The plaintext is returned
    /// exactly once; only its SHA-256 lands in Postgres.
    pub async fn mint_token(
        &self,
        agent_name: &str,
        allowed_repos: Vec<String>,
        allowed_tools: Vec<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<MintedToken, IdentityError> {
        let agent_id: Option<Uuid> =
            sqlx::query_scalar("select id from agents where name = $1 and disabled_at is null")
                .bind(agent_name)
                .fetch_optional(&self.pool)
                .await?;
        let agent_id = agent_id.ok_or_else(|| IdentityError::AgentNotFound(agent_name.into()))?;

        let secret: [u8; 32] = rand::rng().random();
        let token = format!(
            "{TOKEN_PREFIX}{}",
            secret
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let token_id: Uuid = sqlx::query_scalar(
            "insert into agent_tokens (agent_id, token_hash, allowed_repos, allowed_tools, expires_at)
             values ($1, $2, $3, $4, $5) returning id",
        )
        .bind(agent_id)
        .bind(sha256(token.as_bytes()))
        .bind(&allowed_repos)
        .bind(&allowed_tools)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;

        Ok(MintedToken {
            token,
            token_id,
            agent: agent_name.to_string(),
            allowed_repos,
            allowed_tools,
            expires_at,
        })
    }

    /// Resolve a bearer token to its agent identity. `None` means the token
    /// is unknown, revoked, expired, or belongs to a disabled agent.
    pub async fn authenticate(&self, token: &str) -> Result<Option<AuthedAgent>, IdentityError> {
        if !token.starts_with(TOKEN_PREFIX) {
            return Ok(None);
        }
        #[derive(sqlx::FromRow)]
        struct Row {
            agent_id: Uuid,
            token_id: Uuid,
            name: String,
            allowed_repos: Vec<String>,
            allowed_tools: Vec<String>,
        }
        let row: Option<Row> = sqlx::query_as(
            "select a.id as agent_id, t.id as token_id, a.name, t.allowed_repos, t.allowed_tools
             from agent_tokens t join agents a on a.id = t.agent_id
             where t.token_hash = $1
               and t.revoked_at is null
               and (t.expires_at is null or t.expires_at > now())
               and a.disabled_at is null",
        )
        .bind(sha256(token.as_bytes()))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| AuthedAgent {
            agent_id: r.agent_id,
            token_id: r.token_id,
            name: r.name,
            allowed_repos: r.allowed_repos,
            allowed_tools: r.allowed_tools,
        }))
    }

    /// Revalidate a presented bearer and both of its independent scope axes.
    /// An empty repository denotes a platform tool and therefore checks only
    /// the tool scope, matching the MCP gateway's existing authorization rule.
    pub async fn authorize(
        &self,
        token: &str,
        repo: &str,
        tool: &str,
    ) -> Result<AuthorizationDecision, IdentityError> {
        let Some(agent) = self.authenticate(token).await? else {
            return Ok(AuthorizationDecision::InvalidCredential);
        };
        if !agent.may_use_tool(tool) || (!repo.is_empty() && !agent.may_touch_repo(repo)) {
            return Ok(AuthorizationDecision::ScopeDenied);
        }
        Ok(AuthorizationDecision::Authorized(agent))
    }

    /// Record one MCP tool invocation. Failure to audit is a hard error —
    /// the gateway refuses to silently drop provenance.
    pub async fn record_audit(
        &self,
        agent: &AuthedAgent,
        tool: &str,
        repo: &str,
        args_digest: &[u8],
        status: &str,
        error: Option<&str>,
    ) -> Result<(), IdentityError> {
        sqlx::query(
            "insert into agent_audit_log (agent_id, token_id, tool, repo, args_digest, status, error)
             values ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(agent.agent_id)
        .bind(agent.token_id)
        .bind(tool)
        .bind(repo)
        .bind(args_digest)
        .bind(status)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Recent agent sessions that touched one repo, newest activity first —
    /// the presence primitive behind the Stage 6 UI panel. A "session" is
    /// one (agent, token) pair aggregated over its audit entries within the
    /// window: which agent, under which credential, doing what, and when.
    pub async fn repo_sessions(
        &self,
        repo: &str,
        within_secs: i64,
        limit: i64,
    ) -> Result<Vec<RepoSession>, IdentityError> {
        Ok(sqlx::query_as::<_, RepoSession>(
            "select * from (
                 select distinct on (l.agent_id, l.token_id)
                     a.name as agent, l.agent_id, l.token_id,
                     l.tool as last_tool, l.status as last_status,
                     l.occurred_at as last_seen,
                     count(*) over w as tool_calls,
                     min(l.occurred_at) over w as first_seen
                 from agent_audit_log l join agents a on a.id = l.agent_id
                 where l.repo = $1
                   and l.occurred_at > now() - make_interval(secs => $2)
                 window w as (partition by l.agent_id, l.token_id)
                 order by l.agent_id, l.token_id, l.occurred_at desc
             ) sessions
             order by last_seen desc limit $3",
        )
        .bind(repo)
        .bind(within_secs as f64)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    /// All agents, optionally including disabled ones.
    pub async fn list_agents(&self, include_disabled: bool) -> Result<Vec<Agent>, IdentityError> {
        let rows = if include_disabled {
            sqlx::query_as::<_, Agent>(
                "select id, name, description, created_at, disabled_at
                 from agents order by name",
            )
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Agent>(
                "select id, name, description, created_at, disabled_at
                 from agents where disabled_at is null order by name",
            )
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows)
    }

    pub async fn get_agent(&self, name: &str) -> Result<Agent, IdentityError> {
        sqlx::query_as::<_, Agent>(
            "select id, name, description, created_at, disabled_at
             from agents where name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| IdentityError::AgentNotFound(name.into()))
    }

    /// Soft-disable an agent; all of its tokens stop authenticating at once.
    pub async fn disable_agent(&self, name: &str) -> Result<Agent, IdentityError> {
        sqlx::query_as::<_, Agent>(
            "update agents set disabled_at = now()
             where name = $1 and disabled_at is null
             returning id, name, description, created_at, disabled_at",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| IdentityError::AgentNotFound(name.into()))
    }

    /// Metadata for every token minted for one agent, newest first.
    pub async fn list_tokens(&self, agent_name: &str) -> Result<Vec<TokenMeta>, IdentityError> {
        let agent_id: Option<Uuid> = sqlx::query_scalar("select id from agents where name = $1")
            .bind(agent_name)
            .fetch_optional(&self.pool)
            .await?;
        let agent_id = agent_id.ok_or_else(|| IdentityError::AgentNotFound(agent_name.into()))?;

        let rows = sqlx::query_as::<_, TokenMeta>(
            "select id,
                    $2::text as token_prefix,
                    allowed_repos, allowed_tools, created_at, expires_at, revoked_at
             from agent_tokens
             where agent_id = $1
             order by created_at desc",
        )
        .bind(agent_id)
        .bind(TOKEN_DISPLAY_PREFIX)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn revoke_token(
        &self,
        agent_name: &str,
        token_id: Uuid,
    ) -> Result<(), IdentityError> {
        let result = sqlx::query(
            "update agent_tokens t
             set revoked_at = now()
             from agents a
             where t.agent_id = a.id
               and a.name = $1
               and t.id = $2
               and t.revoked_at is null",
        )
        .bind(agent_name)
        .bind(token_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(IdentityError::TokenNotFound(token_id, agent_name.into()));
        }
        Ok(())
    }

    pub async fn update_token_scopes(
        &self,
        agent_name: &str,
        token_id: Uuid,
        allowed_repos: Option<Vec<String>>,
        allowed_tools: Option<Vec<String>>,
    ) -> Result<TokenMeta, IdentityError> {
        let row = sqlx::query_as::<_, TokenMeta>(
            "update agent_tokens t
             set allowed_repos = coalesce($3, t.allowed_repos),
                 allowed_tools = coalesce($4, t.allowed_tools)
             from agents a
             where t.agent_id = a.id
               and a.name = $1
               and t.id = $2
               and t.revoked_at is null
             returning t.id,
                       $5::text as token_prefix,
                       t.allowed_repos, t.allowed_tools,
                       t.created_at, t.expires_at, t.revoked_at",
        )
        .bind(agent_name)
        .bind(token_id)
        .bind(allowed_repos)
        .bind(allowed_tools)
        .bind(TOKEN_DISPLAY_PREFIX)
        .fetch_optional(&self.pool)
        .await?;
        row.ok_or_else(|| IdentityError::TokenNotFound(token_id, agent_name.into()))
    }

    /// Recent audit entries for one agent, newest first.
    pub async fn audit_log(
        &self,
        agent_name: &str,
        limit: i64,
    ) -> Result<Vec<AuditEntry>, IdentityError> {
        Ok(sqlx::query_as::<_, AuditEntry>(
            "select l.id, l.agent_id, l.token_id, l.tool, l.repo, l.args_digest,
                    l.status, l.error, l.occurred_at
             from agent_audit_log l join agents a on a.id = l.agent_id
             where a.name = $1 order by l.id desc limit $2",
        )
        .bind(agent_name)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }
}
