//! Adversarial human/private repository read matrix.
//!
//! Uses two humans and two organizations under required auth. All repository
//! fixtures are control-plane rows only: unauthorized route checks must return
//! before attempting the intentionally unreachable provider/VCS endpoints.

use std::sync::Arc;

use axum::http::StatusCode;
use clotho_api_gateway::auth::{hash_token, mint_plaintext_token};
use clotho_api_gateway::control::{self, Bootstrap};
use clotho_api_gateway::forgejo::{ForgejoConfig, TokenSource};
use clotho_api_gateway::{init_db, router_with_pool, GatewayConfig};
use futures::FutureExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use tokio::net::TcpListener;

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

fn database_url() -> Option<String> {
    let url = [
        "CLOTHO_AUTH_TEST_DATABASE_URL",
        "CLOTHO_CONTROL_PLANE_TEST_DATABASE_URL",
        "CLOTHO_STAGE11_TEST_DATABASE_URL",
    ]
    .into_iter()
    .find_map(|name| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    });
    if url.is_none() {
        let message = "no auth/control/Stage11 live-test database is configured";
        if env_truthy("CLOTHO_TEST_FAIL_ON_SKIP") {
            panic!("live-test gate refused to skip: {message}");
        }
        eprintln!("skipping: {message}");
    }
    url
}

fn config(auth_required: bool) -> GatewayConfig {
    GatewayConfig {
        vcs_grpc_url: "http://127.0.0.1:9".into(),
        diff_grpc_url: "http://127.0.0.1:9".into(),
        merge_queue_grpc_url: "http://127.0.0.1:9".into(),
        agent_gateway_url: "http://127.0.0.1:9".into(),
        agent_admin_token: String::new(),
        compute_grpc_url: "http://127.0.0.1:9".into(),
        storage_grpc_url: "http://127.0.0.1:9".into(),
        large_file_threshold_bytes: 10 * 1024 * 1024,
        storage_sdk_bridge_url: String::new(),
        webhook_secret: String::new(),
        webhook_url: String::new(),
        web_url: "http://127.0.0.1:9".into(),
        compute_provider: "disabled".into(),
        compute_default_image: "ubuntu:22.04".into(),
        actions_timeout_seconds: 30,
        configured_providers: std::collections::HashMap::new(),
        bootstrap_user_name: "clotho".into(),
        bootstrap_user_email: "admin@clotho.internal".into(),
        bootstrap_org_name: "clotho".into(),
        bootstrap_org_display_name: "Clotho".into(),
        auth_required,
        auth_provider: "bootstrap".into(),
        clerk: None,
        public_git_url: "http://127.0.0.1:9".into(),
        forgejo: ForgejoConfig {
            base_url: "http://127.0.0.1:9".into(),
            owner: "clotho".into(),
            token: TokenSource::Inline(String::new()),
        },
    }
}

#[derive(Clone)]
struct Fixture {
    user_a: String,
    user_b: String,
    org_a: String,
    org_b: String,
    org_a_name: String,
    public_repo: String,
    private_repo: String,
    internal_repo: String,
    duplicate_repo: String,
    missing_repo: String,
    repo_ids: Vec<String>,
    token_a: String,
    token_b: String,
    public_event: String,
    private_event: String,
    internal_event: String,
}

struct RepoSeed<'a> {
    id: &'a str,
    org_id: &'a str,
    org_name: &'a str,
    name: &'a str,
    visibility: &'a str,
    kind: &'a str,
    created_by: &'a str,
    forgejo_id: i64,
    order_days: i32,
}

async fn insert_repo(tx: &mut Transaction<'_, Postgres>, seed: RepoSeed<'_>) {
    sqlx::query(
        r#"insert into repos
           (id, org_id, name, visibility, kind, forgejo_owner, forgejo_repo_id,
            forgejo_full_name, created_by, updated_at)
           values ($1,$2,$3,$4,$5,$6,$7,$8,$9,now() + ($10 * interval '1 day'))"#,
    )
    .bind(seed.id)
    .bind(seed.org_id)
    .bind(seed.name)
    .bind(seed.visibility)
    .bind(seed.kind)
    .bind(seed.org_name)
    .bind(seed.forgejo_id)
    .bind(format!("{}/{}", seed.org_name, seed.name))
    .bind(seed.created_by)
    .bind(seed.order_days)
    .execute(&mut **tx)
    .await
    .unwrap();
    sqlx::query(
        "insert into repo_permissions (repo_id, user_id, permission) values ($1,$2,'admin')",
    )
    .bind(seed.id)
    .bind(seed.created_by)
    .execute(&mut **tx)
    .await
    .unwrap();
}

async fn insert_token(tx: &mut Transaction<'_, Postgres>, user_id: &str, label: &str) -> String {
    let token = mint_plaintext_token();
    let token_id = uuid::Uuid::new_v4().to_string();
    let prefix: String = token.chars().take(12).collect();
    sqlx::query(
        "insert into api_tokens (id,user_id,name,token_hash,token_prefix,scopes) values ($1,$2,$3,$4,$5,'{*}')",
    )
    .bind(token_id)
    .bind(user_id)
    .bind(label)
    .bind(hash_token(&token))
    .bind(prefix)
    .execute(&mut **tx)
    .await
    .unwrap();
    token
}

async fn create_fixture(pool: &PgPool, bootstrap: &Bootstrap) -> Fixture {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let short = &suffix[..10];
    let user_a = format!("authz-user-a-{short}");
    let user_b = format!("authz-user-b-{short}");
    let org_a = format!("authz-org-a-id-{short}");
    let org_b = format!("authz-org-b-id-{short}");
    let org_a_name = format!("authz-org-a-{short}");
    let org_b_name = format!("authz-org-b-{short}");
    let public_repo = format!("authz-public-{short}");
    let private_repo = format!("authz-private-{short}");
    let internal_repo = format!("authz-internal-{short}");
    let duplicate_repo = format!("authz-duplicate-{short}");
    let missing_repo = format!("authz-missing-{short}");
    let repo_ids = (0..5)
        .map(|_| uuid::Uuid::new_v4().to_string())
        .collect::<Vec<_>>();
    let public_event = format!("authz.public.{suffix}");
    let private_event = format!("authz.private.{suffix}");
    let internal_event = format!("authz.internal.{suffix}");

    let mut tx = pool.begin().await.unwrap();
    for (id, name) in [(&user_a, "A"), (&user_b, "B")] {
        sqlx::query("insert into users (id,name,email,display_name) values ($1,$1,$2,$3)")
            .bind(id)
            .bind(format!("{id}@example.invalid"))
            .bind(format!("Authz user {name}"))
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    for (id, name, owner) in [
        (&org_a, &org_a_name, &user_a),
        (&org_b, &org_b_name, &user_b),
    ] {
        sqlx::query(
            "insert into orgs (id,name,display_name,forgejo_owner,created_by) values ($1,$2,$2,$2,$3)",
        )
        .bind(id)
        .bind(name)
        .bind(owner)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query("insert into org_memberships (org_id,user_id,role) values ($1,$2,'admin')")
            .bind(id)
            .bind(owner)
            .execute(&mut *tx)
            .await
            .unwrap();
    }

    insert_repo(
        &mut tx,
        RepoSeed {
            id: &repo_ids[0],
            org_id: &org_a,
            org_name: &org_a_name,
            name: &public_repo,
            visibility: "public",
            kind: "model",
            created_by: &user_a,
            forgejo_id: 101,
            order_days: 1,
        },
    )
    .await;
    let release_manifest = json!({
        "kind": "model",
        "total_files": 1,
        "total_bytes": 5,
        "readiness": {"ready": true},
        "artifacts": [{
            "path": "README.md",
            "size_bytes": 5,
            "storage": "git",
            "oid_sha256": "",
        }],
        "metadata": {},
    });
    let release_digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&release_manifest).unwrap())
    );
    sqlx::query(
        r#"insert into repo_releases
           (id,repo_id,version,commit_id,manifest,manifest_sha256,created_by)
           values ($1,$2,'v1','deadbeef',$3,$4,$5)"#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&repo_ids[0])
    .bind(release_manifest)
    .bind(release_digest)
    .bind(&user_a)
    .execute(&mut *tx)
    .await
    .unwrap();
    insert_repo(
        &mut tx,
        RepoSeed {
            id: &repo_ids[1],
            org_id: &org_a,
            org_name: &org_a_name,
            name: &private_repo,
            visibility: "private",
            kind: "model",
            created_by: &user_a,
            forgejo_id: 102,
            order_days: 2,
        },
    )
    .await;
    insert_repo(
        &mut tx,
        RepoSeed {
            id: &repo_ids[2],
            org_id: &org_b,
            org_name: &org_b_name,
            name: &internal_repo,
            visibility: "internal",
            kind: "dataset",
            created_by: &user_b,
            forgejo_id: 103,
            order_days: 3,
        },
    )
    .await;
    insert_repo(
        &mut tx,
        RepoSeed {
            id: &repo_ids[3],
            org_id: &org_a,
            org_name: &org_a_name,
            name: &duplicate_repo,
            visibility: "public",
            kind: "code",
            created_by: &user_a,
            forgejo_id: 104,
            order_days: 4,
        },
    )
    .await;
    insert_repo(
        &mut tx,
        RepoSeed {
            id: &repo_ids[4],
            org_id: &org_b,
            org_name: &org_b_name,
            name: &duplicate_repo,
            visibility: "public",
            kind: "code",
            created_by: &user_b,
            forgejo_id: 105,
            order_days: 5,
        },
    )
    .await;
    // Open-local fallback is still the bootstrap human, not an anonymous
    // authorization bypass. Give it an explicit read grant for this fixture.
    sqlx::query("insert into repo_permissions (repo_id,user_id,permission) values ($1,$2,'read')")
        .bind(&repo_ids[1])
        .bind(&bootstrap.user_id)
        .execute(&mut *tx)
        .await
        .unwrap();

    let token_a = insert_token(&mut tx, &user_a, "authz-a").await;
    let token_b = insert_token(&mut tx, &user_b, "authz-b").await;
    for (actor, org, repo, event_type, days) in [
        (&user_a, &org_a, &repo_ids[0], &public_event, 1),
        (&user_a, &org_a, &repo_ids[1], &private_event, 2),
        (&user_b, &org_b, &repo_ids[2], &internal_event, 3),
    ] {
        sqlx::query(
            r#"insert into activity_events
               (actor_id,org_id,repo_id,event_type,payload,created_at)
               values ($1,$2,$3,$4,'{}',now() + ($5 * interval '1 day'))"#,
        )
        .bind(actor)
        .bind(org)
        .bind(repo)
        .bind(event_type)
        .bind(days)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    Fixture {
        user_a,
        user_b,
        org_a,
        org_b,
        org_a_name,
        public_repo,
        private_repo,
        internal_repo,
        duplicate_repo,
        missing_repo,
        repo_ids,
        token_a,
        token_b,
        public_event,
        private_event,
        internal_event,
    }
}

async fn cleanup_fixture(pool: &PgPool, fixture: &Fixture) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("delete from repos where id = any($1)")
        .bind(&fixture.repo_ids)
        .execute(&mut *tx)
        .await?;
    sqlx::query("delete from orgs where id = any($1)")
        .bind(vec![fixture.org_a.clone(), fixture.org_b.clone()])
        .execute(&mut *tx)
        .await?;
    sqlx::query("delete from users where id = any($1)")
        .bind(vec![fixture.user_a.clone(), fixture.user_b.clone()])
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

async fn spawn(
    pool: PgPool,
    bootstrap: Bootstrap,
    auth_required: bool,
) -> (String, tokio::task::JoinHandle<()>) {
    let app = router_with_pool(config(auth_required), Some(pool), bootstrap).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), task)
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

async fn get(
    client: &reqwest::Client,
    base: &str,
    path: &str,
    token: Option<&str>,
) -> reqwest::Response {
    let mut request = client.get(format!("{base}{path}"));
    if let Some(token) = token {
        request = request.header("Authorization", bearer(token));
    }
    request.send().await.unwrap()
}

fn protected_paths(repo: &str, owner: &str) -> Vec<String> {
    vec![
        format!("/api/v1/repos/{repo}"),
        format!("/api/v1/repos/{repo}/tree"),
        format!("/api/v1/repos/{repo}/artifacts"),
        format!("/api/v1/repos/{repo}/artifacts/preview?path=data.csv"),
        format!("/api/v1/repos/{repo}/file?path=README.md"),
        format!("/api/v1/repos/{repo}/storage"),
        format!("/api/v1/repos/{repo}/commits"),
        format!("/api/v1/repos/{repo}/oplog"),
        format!("/api/v1/repos/{repo}/issues"),
        format!("/api/v1/repos/{repo}/issues/1"),
        format!("/api/v1/repos/{repo}/labels"),
        format!("/api/v1/repos/{repo}/milestones"),
        format!("/api/v1/repos/{repo}/pulls"),
        format!("/api/v1/repos/{repo}/pulls/1"),
        format!("/api/v1/repos/{repo}/pulls/1/comments"),
        format!("/api/v1/repos/{repo}/pulls/1/reviews"),
        format!("/api/v1/repos/{repo}/pulls/1/diff"),
        format!("/api/v1/repos/{repo}/branches"),
        format!("/api/v1/repos/{repo}/commits/deadbeef/statuses"),
        format!("/api/v1/repos/{repo}/merge-policy"),
        format!("/api/v1/repos/{repo}/actions/runs"),
        format!("/api/v1/repos/{repo}/actions/runs/run-missing"),
        format!("/api/v1/repos/{repo}/actions/runs/run-missing/logs"),
        format!("/api/v1/repos/{repo}/actions/config"),
        format!("/api/v1/repos/{repo}/hub-imports"),
        format!("/api/v1/repos/{repo}/hub-imports/job-missing"),
        format!("/api/v1/repos/{repo}/releases"),
        format!("/api/v1/repos/{repo}/releases/v1"),
        format!("/api/v1/repos/{repo}/releases/v1/resolve/README.md"),
        format!("/api/v1/repos/{repo}/agent-sessions"),
        format!("/api/models/{owner}/{repo}"),
        format!("/{owner}/{repo}/resolve/main/README.md"),
    ]
}

async fn json_body(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    let body = response.json().await.unwrap();
    (status, body)
}

#[tokio::test]
async fn private_repo_reads_are_tenant_safe_and_lists_filter_before_pagination() {
    let Some(database_url) = database_url() else {
        return;
    };
    let pool = match init_db(&database_url).await {
        Ok(pool) => pool,
        Err(error) if !env_truthy("CLOTHO_TEST_FAIL_ON_SKIP") => {
            eprintln!("skipping: private-read live-test database unavailable: {error}");
            return;
        }
        Err(error) => panic!("private-read live-test database unavailable: {error}"),
    };
    let bootstrap = Bootstrap::from_config(&config(true));
    control::ensure_bootstrap(&pool, &bootstrap).await.unwrap();
    let fixture = Arc::new(create_fixture(&pool, &bootstrap).await);
    let (required_base, required_server) = spawn(pool.clone(), bootstrap.clone(), true).await;
    let (open_base, open_server) = spawn(pool.clone(), bootstrap, false).await;
    let client = reqwest::Client::new();

    let outcome = std::panic::AssertUnwindSafe(async {
        // Public reads remain anonymous even when the deployment requires
        // authentication globally.
        for suffix in [
            "merge-policy",
            "actions/config",
            "actions/runs",
            "hub-imports",
            "releases",
        ] {
            let public = get(
                &client,
                &required_base,
                &format!("/api/v1/repos/{}/{suffix}", fixture.public_repo),
                None,
            )
            .await;
            assert_eq!(public.status(), StatusCode::OK, "public {suffix}");
        }
        let public_hf = get(
            &client,
            &required_base,
            &format!("/api/models/{}/{}", fixture.org_a_name, fixture.public_repo),
            None,
        )
        .await;
        assert_eq!(public_hf.status(), StatusCode::OK);
        // HF resolve authorizes in its compatibility projection and then
        // delegates to the canonical release download. Anonymous public auth
        // survives both layers and reaches the intentionally absent VCS.
        let public_hf_download = get(
            &client,
            &required_base,
            &format!(
                "/{}/{}/resolve/main/README.md",
                fixture.org_a_name, fixture.public_repo
            ),
            None,
        )
        .await;
        assert_eq!(public_hf_download.status(), StatusCode::BAD_GATEWAY);

        // A supplied bad credential is a 401 for public, private, missing,
        // global-list, and activity reads; it is never ignored as anonymous.
        for path in [
            format!("/api/v1/repos/{}/merge-policy", fixture.public_repo),
            format!("/api/v1/repos/{}/merge-policy", fixture.private_repo),
            format!("/api/v1/repos/{}/merge-policy", fixture.missing_repo),
            "/api/v1/repos".into(),
            "/api/v1/activity".into(),
        ] {
            let response = client
                .get(format!("{required_base}{path}"))
                .header("Authorization", "Bearer clotho_tok_invalid")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        }
        let malformed = client
            .get(format!(
                "{required_base}/api/v1/repos/{}/merge-policy",
                fixture.public_repo
            ))
            .header("Authorization", "Basic invalid")
            .send()
            .await
            .unwrap();
        assert_eq!(malformed.status(), StatusCode::UNAUTHORIZED);

        // Every name-routed read returns before the unreachable providers for
        // a foreign private repository. Missing and unauthorized are the same
        // stable 404 envelope.
        let foreign_paths = protected_paths(&fixture.private_repo, &fixture.org_a_name);
        let missing_paths = protected_paths(&fixture.missing_repo, &fixture.org_a_name);
        for (foreign_path, missing_path) in foreign_paths.iter().zip(missing_paths.iter()) {
            let (foreign_status, foreign) = json_body(
                get(
                    &client,
                    &required_base,
                    foreign_path,
                    Some(&fixture.token_b),
                )
                .await,
            )
            .await;
            let (missing_status, missing) = json_body(
                get(
                    &client,
                    &required_base,
                    missing_path,
                    Some(&fixture.token_b),
                )
                .await,
            )
            .await;
            assert_eq!(foreign_status, StatusCode::NOT_FOUND, "{foreign_path}");
            assert_eq!(missing_status, StatusCode::NOT_FOUND, "{missing_path}");
            assert_eq!(foreign["code"], "not_found", "{foreign_path}");
            assert_eq!(foreign["message"], "repository not found", "{foreign_path}");
            assert_eq!(foreign["code"], missing["code"], "{foreign_path}");
            assert_eq!(foreign["message"], missing["message"], "{foreign_path}");
        }

        // Internal uses the same non-public permission rule, and globally
        // ambiguous names fail closed even for one of their owners.
        for repo in [&fixture.internal_repo, &fixture.duplicate_repo] {
            let response = get(
                &client,
                &required_base,
                &format!("/api/v1/repos/{repo}/merge-policy"),
                Some(&fixture.token_a),
            )
            .await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{repo}");
        }
        let duplicate_create = client
            .post(format!("{required_base}/api/v1/repos"))
            .header("Authorization", bearer(&fixture.token_a))
            .json(&json!({
                "name": fixture.duplicate_repo,
                "owner_org": fixture.org_a_name,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(duplicate_create.status(), StatusCode::CONFLICT);

        // Authorized non-public reads reach DB-owned surfaces successfully.
        for (repo, token) in [
            (&fixture.private_repo, &fixture.token_a),
            (&fixture.internal_repo, &fixture.token_b),
        ] {
            for suffix in [
                "merge-policy",
                "actions/config",
                "actions/runs",
                "hub-imports",
                "releases",
            ] {
                let response = get(
                    &client,
                    &required_base,
                    &format!("/api/v1/repos/{repo}/{suffix}"),
                    Some(token),
                )
                .await;
                assert_eq!(response.status(), StatusCode::OK, "{repo}/{suffix}");
            }
        }

        // Repository filtering happens before limit/cursor pagination.
        let anonymous: Value = get(&client, &required_base, "/api/v1/repos?limit=1", None)
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(anonymous["repos"][0]["name"], fixture.public_repo);

        let owner_a: Value = get(
            &client,
            &required_base,
            "/api/v1/repos?limit=1",
            Some(&fixture.token_a),
        )
        .await
        .json()
        .await
        .unwrap();
        assert_eq!(owner_a["repos"][0]["name"], fixture.private_repo);
        let cursor = owner_a["next_cursor"].as_str().unwrap();
        let owner_a_second: Value = get(
            &client,
            &required_base,
            &format!("/api/v1/repos?limit=1&cursor={cursor}"),
            Some(&fixture.token_a),
        )
        .await
        .json()
        .await
        .unwrap();
        assert_eq!(owner_a_second["repos"][0]["name"], fixture.public_repo);

        let owner_b: Value = get(
            &client,
            &required_base,
            "/api/v1/repos?limit=1",
            Some(&fixture.token_b),
        )
        .await
        .json()
        .await
        .unwrap();
        assert_eq!(owner_b["repos"][0]["name"], fixture.internal_repo);

        let foreign_org_list: Value = get(
            &client,
            &required_base,
            &format!("/api/v1/orgs/{}/repos?limit=100", fixture.org_a_name),
            Some(&fixture.token_b),
        )
        .await
        .json()
        .await
        .unwrap();
        assert_eq!(foreign_org_list["repos"].as_array().unwrap().len(), 1);
        assert_eq!(foreign_org_list["repos"][0]["name"], fixture.public_repo);

        // Activity visibility is filtered in SQL before the page limit.
        for (token, expected) in [
            (None, fixture.public_event.as_str()),
            (
                Some(fixture.token_a.as_str()),
                fixture.private_event.as_str(),
            ),
            (
                Some(fixture.token_b.as_str()),
                fixture.internal_event.as_str(),
            ),
        ] {
            let activity: Value = get(&client, &required_base, "/api/v1/activity?limit=1", token)
                .await
                .json()
                .await
                .unwrap();
            assert_eq!(activity["events"][0]["event_type"], expected);
        }

        // Open-local mode still resolves no-header requests as the bootstrap
        // principal, whose explicit fixture grant permits this private read.
        let local = get(
            &client,
            &open_base,
            &format!("/api/v1/repos/{}/merge-policy", fixture.private_repo),
            None,
        )
        .await;
        assert_eq!(local.status(), StatusCode::OK);
        let invalid_local = client
            .get(format!(
                "{open_base}/api/v1/repos/{}/merge-policy",
                fixture.public_repo
            ))
            .header("Authorization", "Bearer clotho_tok_invalid")
            .send()
            .await
            .unwrap();
        assert_eq!(invalid_local.status(), StatusCode::UNAUTHORIZED);
        let malformed_local_mutation = client
            .post(format!(
                "{open_base}/api/v1/repos/{}/issues",
                fixture.public_repo
            ))
            .header("Authorization", "Basic invalid")
            .json(&serde_json::json!({"title": "must not reach provider"}))
            .send()
            .await
            .unwrap();
        assert_eq!(malformed_local_mutation.status(), StatusCode::UNAUTHORIZED);
    })
    .catch_unwind()
    .await;

    required_server.abort();
    open_server.abort();
    let cleanup = cleanup_fixture(&pool, &fixture)
        .await
        .map_err(|error| error.to_string());
    match outcome {
        Ok(()) => cleanup.unwrap(),
        Err(payload) => {
            if let Err(error) = cleanup {
                eprintln!("fixture cleanup also failed: {error}");
            }
            std::panic::resume_unwind(payload);
        }
    }
}
