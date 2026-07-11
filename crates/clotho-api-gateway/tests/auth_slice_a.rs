//! Slice A auth integration tests (requires Postgres).

use axum::http::StatusCode;
use clotho_api_gateway::auth::{hash_token, mint_plaintext_token};
use clotho_api_gateway::control::{self, Bootstrap, CreateRepoRequest};
use clotho_api_gateway::forgejo::{ForgejoConfig, RepoInfo, TokenSource};
use clotho_api_gateway::{init_db, router_with_pool, GatewayConfig};
use tokio::net::TcpListener;

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.into())
}

fn database_url() -> Option<String> {
    let url = env_or(
        "CLOTHO_AUTH_TEST_DATABASE_URL",
        &env_or(
            "CLOTHO_CONTROL_PLANE_TEST_DATABASE_URL",
            "postgres://clotho:clotho-dev@localhost:5432/clotho",
        ),
    );
    if url.trim().is_empty() {
        None
    } else {
        Some(url)
    }
}

fn test_config(auth_required: bool) -> GatewayConfig {
    GatewayConfig {
        vcs_grpc_url: env_or("CLOTHO_AUTH_TEST_VCS_GRPC_URL", "http://localhost:50051"),
        diff_grpc_url: env_or("CLOTHO_AUTH_TEST_DIFF_GRPC_URL", "http://localhost:50055"),
        merge_queue_grpc_url: env_or(
            "CLOTHO_AUTH_TEST_MERGE_QUEUE_GRPC_URL",
            "http://localhost:50053",
        ),
        agent_gateway_url: env_or(
            "CLOTHO_AUTH_TEST_AGENT_GATEWAY_URL",
            "http://localhost:8090",
        ),
        agent_admin_token: String::new(),
        compute_grpc_url: env_or(
            "CLOTHO_AUTH_TEST_COMPUTE_GRPC_URL",
            "http://localhost:50057",
        ),
        storage_grpc_url: "http://localhost:50052".into(),
        large_file_threshold_bytes: 10 * 1024 * 1024,
        storage_sdk_bridge_url: String::new(),
        webhook_secret: String::new(),
        webhook_url: String::new(),
        web_url: env_or("CLOTHO_AUTH_TEST_WEB_URL", "http://localhost:3100"),
        compute_provider: "daytona".into(),
        compute_default_image: "ubuntu:22.04".into(),
        actions_timeout_seconds: 900,
        configured_providers: std::collections::HashMap::new(),
        bootstrap_user_name: "clotho".into(),
        bootstrap_user_email: "admin@clotho.internal".into(),
        bootstrap_org_name: "clotho".into(),
        bootstrap_org_display_name: "Clotho".into(),
        auth_required,
        auth_provider: "bootstrap".into(),
        clerk: None,
        public_git_url: env_or("CLOTHO_AUTH_TEST_PUBLIC_GIT_URL", "http://localhost:13000"),
        forgejo: ForgejoConfig {
            base_url: env_or("CLOTHO_AUTH_TEST_FORGEJO_URL", "http://localhost:13000"),
            owner: "clotho".into(),
            token: TokenSource::Inline(String::new()),
        },
    }
}

async fn setup() -> Option<(sqlx::PgPool, Bootstrap, GatewayConfig)> {
    let url = database_url()?;
    let config = test_config(false);
    let pool = init_db(&url).await.ok()?;
    let bootstrap = Bootstrap::from_config(&config);
    control::ensure_bootstrap(&pool, &bootstrap).await.ok()?;
    Some((pool, bootstrap, config))
}

#[tokio::test]
async fn invalid_token_returns_unauthorized_when_auth_required() {
    let Some((pool, bootstrap, mut config)) = setup().await else {
        return;
    };
    config.auth_required = true;
    let app = router_with_pool(config, Some(pool), bootstrap).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let res = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/me"))
        .header("Authorization", "Bearer clotho_tok_invalid")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn token_mint_patch_and_delete_repo() {
    let Some((pool, bootstrap, config)) = setup().await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let repo_name = format!("auth-test-{suffix}");
    let req = CreateRepoRequest {
        name: repo_name.clone(),
        description: "before".into(),
        visibility: "public".into(),
        kind: "code".into(),
        large_file_threshold_bytes: None,
        network_mode: "public".into(),
        network_tags: vec![],
        default_branch: "main".into(),
        owner_org: String::new(),
    };
    let forgejo = RepoInfo {
        id: 1,
        name: repo_name.clone(),
        full_name: format!("clotho/{}", repo_name),
        ..Default::default()
    };
    let resolved = control::resolve_org(&pool, &bootstrap, "").await.unwrap();
    control::insert_repo(&pool, &bootstrap.user_id, &req, &resolved, &forgejo)
        .await
        .unwrap();

    let app = router_with_pool(config.clone(), Some(pool.clone()), bootstrap.clone()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    let created: serde_json::Value = client
        .post(format!("{base}/api/v1/tokens"))
        .json(&serde_json::json!({ "name": "test" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = created["token"].as_str().unwrap().to_string();
    let auth = format!("Bearer {token}");

    let me: serde_json::Value = client
        .get(format!("{base}/api/v1/me"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["user"]["name"], "clotho");

    let patched = client
        .patch(format!("{base}/api/v1/repos/{repo_name}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "description": "after" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patched.status(), StatusCode::OK);
    let body: serde_json::Value = patched.json().await.unwrap();
    assert_eq!(body["description"], "after");
    assert!(body.get("info").is_some());
    assert!(body.get("forgejo").is_none());

    let deleted = client
        .delete(format!("{base}/api/v1/repos/{repo_name}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let token_id = created["id"].as_str().unwrap();
    let revoked = client
        .delete(format!("{base}/api/v1/tokens/{token_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn non_admin_gets_forbidden_on_patch_repo() {
    let Some((pool, bootstrap, config)) = setup().await else {
        return;
    };
    let other_id = format!("other-{}", uuid::Uuid::new_v4().simple());
    let other_name = format!("other-{}", &other_id[..12]);
    sqlx::query(
        "insert into users (id, name, email, display_name) values ($1,$2,$3,$4) on conflict (id) do nothing",
    )
    .bind(&other_id)
    .bind(&other_name)
    .bind("other@clotho.internal")
    .bind(&other_name)
    .execute(&pool)
    .await
    .unwrap();

    let plaintext = mint_plaintext_token();
    sqlx::query(
        "insert into api_tokens (id, user_id, name, token_hash, token_prefix) values ($1,$2,'t',$3,$4)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&other_id)
    .bind(hash_token(&plaintext))
    .bind(&plaintext[..plaintext.len().min(12)])
    .execute(&pool)
    .await
    .unwrap();

    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let repo_name = format!("forbid-{suffix}");
    let req = CreateRepoRequest {
        name: repo_name.clone(),
        description: "x".into(),
        visibility: "public".into(),
        kind: "code".into(),
        large_file_threshold_bytes: None,
        network_mode: "public".into(),
        network_tags: vec![],
        default_branch: "main".into(),
        owner_org: String::new(),
    };
    let forgejo = RepoInfo {
        id: 2,
        name: repo_name.clone(),
        full_name: format!("clotho/{}", repo_name),
        ..Default::default()
    };
    let resolved = control::resolve_org(&pool, &bootstrap, "").await.unwrap();
    control::insert_repo(&pool, &bootstrap.user_id, &req, &resolved, &forgejo)
        .await
        .unwrap();

    let app = router_with_pool(config, Some(pool), bootstrap).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let res = reqwest::Client::new()
        .patch(format!("http://{addr}/api/v1/repos/{repo_name}"))
        .header("Authorization", format!("Bearer {plaintext}"))
        .json(&serde_json::json!({ "description": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
