//! Stage 11 control-plane integration tests: users, orgs, repos, activity.
//!
//! Spawns the gateway in-process against the real dev Postgres and the
//! running local services (clotho-vcs on :50051, Forgejo on :13000). Skipped
//! when `CLOTHO_STAGE11_TEST_DATABASE_URL` is empty. Override Forgejo token
//! via `CLOTHO_STAGE11_TEST_FORGEJO_TOKEN` (defaults to `CLOTHO_FORGEJO_TOKEN`).

use serde_json::{json, Value};
use tokio::net::TcpListener;

use clotho_api_gateway::control::{ensure_bootstrap, Bootstrap};
use clotho_api_gateway::forgejo::{ForgejoConfig, TokenSource};
use clotho_api_gateway::{init_db, router_with_pool, GatewayConfig};

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.into())
}

fn database_url() -> Option<String> {
    let url = env_or("CLOTHO_STAGE11_TEST_DATABASE_URL", "");
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

fn forgejo_token() -> TokenSource {
    let token = std::env::var("CLOTHO_STAGE11_TEST_FORGEJO_TOKEN")
        .or_else(|_| std::env::var("CLOTHO_FORGEJO_TOKEN"))
        .unwrap_or_default();
    if token.is_empty() {
        TokenSource::File(
            env_or(
                "CLOTHO_STAGE11_TEST_FORGEJO_TOKEN_FILE",
                "/run/clotho/forgejo-token",
            )
            .into(),
        )
    } else {
        TokenSource::Inline(token)
    }
}

fn test_config() -> GatewayConfig {
    GatewayConfig {
        vcs_grpc_url: env_or("CLOTHO_STAGE11_TEST_VCS_GRPC_URL", "http://localhost:50051"),
        diff_grpc_url: env_or(
            "CLOTHO_STAGE11_TEST_DIFF_GRPC_URL",
            "http://localhost:50055",
        ),
        merge_queue_grpc_url: env_or(
            "CLOTHO_STAGE11_TEST_MERGE_QUEUE_GRPC_URL",
            "http://localhost:50053",
        ),
        agent_gateway_url: env_or(
            "CLOTHO_STAGE11_TEST_AGENT_GATEWAY_URL",
            "http://localhost:8090",
        ),
        agent_admin_token: String::new(),
        compute_grpc_url: env_or(
            "CLOTHO_STAGE11_TEST_COMPUTE_GRPC_URL",
            "http://localhost:50057",
        ),
        storage_grpc_url: "http://localhost:50052".into(),
        large_file_threshold_bytes: 10 * 1024 * 1024,
        storage_sdk_bridge_url: String::new(),
        webhook_secret: String::new(),
        webhook_url: String::new(),
        web_url: env_or("CLOTHO_STAGE11_TEST_WEB_URL", "http://localhost:3100"),
        compute_provider: env_or("CLOTHO_STAGE11_TEST_COMPUTE_PROVIDER", "daytona"),
        compute_default_image: env_or("CLOTHO_STAGE11_TEST_COMPUTE_SNAPSHOT", "ubuntu:22.04"),
        actions_timeout_seconds: 900,
        configured_providers: {
            let mut m = std::collections::HashMap::new();
            m.insert("daytona".into(), false);
            m.insert("computesdk".into(), false);
            m.insert("box".into(), false);
            m
        },
        bootstrap_user_name: env_or("CLOTHO_STAGE11_TEST_BOOTSTRAP_USER_NAME", "clotho"),
        bootstrap_user_email: env_or(
            "CLOTHO_STAGE11_TEST_BOOTSTRAP_USER_EMAIL",
            "admin@clotho.internal",
        ),
        bootstrap_org_name: env_or("CLOTHO_STAGE11_TEST_BOOTSTRAP_ORG_NAME", "clotho"),
        bootstrap_org_display_name: env_or(
            "CLOTHO_STAGE11_TEST_BOOTSTRAP_ORG_DISPLAY_NAME",
            "Clotho",
        ),
        auth_required: false,
        auth_provider: "bootstrap".into(),
        clerk: None,
        public_git_url: env_or(
            "CLOTHO_STAGE11_TEST_PUBLIC_GIT_URL",
            "http://localhost:13000",
        ),
        forgejo: ForgejoConfig {
            base_url: env_or("CLOTHO_STAGE11_TEST_FORGEJO_URL", "http://localhost:13000"),
            owner: env_or("CLOTHO_STAGE11_TEST_FORGEJO_OWNER", "clotho"),
            token: forgejo_token(),
        },
    }
}

async fn spawn_gateway() -> Option<(u16, sqlx::PgPool, GatewayConfig)> {
    let database_url = database_url()?;
    let config = test_config();
    let pool = init_db(&database_url).await.ok()?;
    let bootstrap = Bootstrap::from_config(&config);
    ensure_bootstrap(&pool, &bootstrap).await.unwrap();
    let router = router_with_pool(config.clone(), Some(pool.clone()), bootstrap).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    Some((addr.port(), pool, config))
}

async fn get(client: &reqwest::Client, url: &str, path: &str) -> Value {
    let response = client.get(format!("{url}{path}")).send().await.unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert!(status.is_success(), "{path}: {status}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{path}: invalid json: {e}"))
}

async fn post_json(client: &reqwest::Client, url: &str, path: &str, body: Value) -> (u16, Value) {
    let response = client
        .post(format!("{url}{path}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    let json = if text.trim().is_empty() {
        json!(null)
    } else {
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: invalid json: {e}: {text}"))
    };
    (status.as_u16(), json)
}

#[tokio::test]
async fn control_plane_users_and_orgs() {
    let Some((port, _pool, _config)) = spawn_gateway().await else {
        return;
    };
    let url = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

    let users = get(&client, &url, "/api/v1/users").await;
    assert!(!users["users"].as_array().unwrap().is_empty());

    let orgs = get(&client, &url, "/api/v1/orgs").await;
    assert!(!orgs["orgs"].as_array().unwrap().is_empty());

    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("stage11-org-{suffix}");
    let (status, created) = post_json(
        &client,
        &url,
        "/api/v1/orgs",
        json!({"name": name, "display_name": format!("Stage 11 {suffix}"), "git_owner": "clotho"}),
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(created["name"], name);

    let detail = get(&client, &url, &format!("/api/v1/orgs/{name}")).await;
    assert_eq!(detail["org"]["name"], name);
    assert!(!detail["members"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn control_plane_repo_creation_and_metadata() {
    let Some((port, _pool, config)) = spawn_gateway().await else {
        return;
    };
    let url = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("stage11-repo-{suffix}");
    let (status, created) = post_json(
        &client,
        &url,
        "/api/v1/repos",
        json!({
            "name": name,
            "description": "Stage 11 control-plane repo",
            "visibility": "public",
            "default_branch": "main",
        }),
    )
    .await;
    assert_eq!(status, 201, "{:?}", created);
    assert_eq!(created["name"], name);
    assert!(created.get("owner_org").is_some());
    assert_eq!(created["visibility"], "public");
    assert_eq!(created["default_branch"], "main");
    assert!(created["clone_url"].as_str().unwrap().ends_with(".git"));
    assert_eq!(created["provider"], "daytona");
    assert_eq!(created["configured"], false);

    let detail = get(&client, &url, &format!("/api/v1/repos/{name}")).await;
    assert_eq!(detail["name"], name);
    assert_eq!(detail["visibility"], "public");
    assert_eq!(detail["default_branch"], "main");
    assert_eq!(detail["provider"], "daytona");
    assert_eq!(detail["configured"], false);

    // The repo should be returned by both global and org-scoped list APIs.
    let list = get(&client, &url, "/api/v1/repos").await;
    assert!(
        list["repos"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["name"] == name),
        "repo should appear in /api/v1/repos"
    );

    let org_name = config.bootstrap_org_name;
    let org_list = get(&client, &url, &format!("/api/v1/orgs/{org_name}/repos")).await;
    assert!(
        org_list["repos"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["name"] == name),
        "repo should appear under its org"
    );

    let activity = get(&client, &url, "/api/v1/activity?limit=10").await;
    assert!(
        activity["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event_type"] == "repo.created"),
        "repo creation should appear in activity feed"
    );
}
