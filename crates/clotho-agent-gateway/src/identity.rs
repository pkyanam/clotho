//! Agent identity: scoped tokens in Postgres, a model deliberately distinct
//! from human identities (docs/prd.md §2/§6 — never a flag on a user row),
//! plus the per-call audit log.

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
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct Agent {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
}

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
             returning id, name, description, created_at",
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
