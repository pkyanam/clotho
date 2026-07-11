//! Stage 17 Clerk AuthProvider tests (mocked HS256 JWT; requires Postgres).

use axum::http::StatusCode;
use clotho_api_gateway::auth_provider::ClerkConfig;
use clotho_api_gateway::control::{self, Bootstrap};
use clotho_api_gateway::forgejo::{ForgejoConfig, TokenSource};
use clotho_api_gateway::{init_db, router_with_pool, GatewayConfig};
use jsonwebtoken::{encode, EncodingKey, Header};
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

const JWT_SECRET: &str = "stage17-clerk-test-secret";

fn clerk_config() -> GatewayConfig {
    GatewayConfig {
        vcs_grpc_url: "http://localhost:50051".into(),
        diff_grpc_url: "http://localhost:50055".into(),
        merge_queue_grpc_url: "http://localhost:50053".into(),
        agent_gateway_url: "http://localhost:8090".into(),
        agent_admin_token: String::new(),
        compute_grpc_url: "http://localhost:50057".into(),
        webhook_secret: String::new(),
        webhook_url: String::new(),
        web_url: "http://localhost:3100".into(),
        compute_provider: "daytona".into(),
        compute_default_image: "ubuntu:22.04".into(),
        actions_timeout_seconds: 900,
        configured_providers: std::collections::HashMap::new(),
        bootstrap_user_name: "clotho".into(),
        bootstrap_user_email: "admin@clotho.internal".into(),
        bootstrap_org_name: "clotho".into(),
        bootstrap_org_display_name: "Clotho".into(),
        auth_required: true,
        auth_provider: "clerk".into(),
        clerk: Some(ClerkConfig {
            publishable_key: "pk_test".into(),
            secret_key: String::new(),
            jwt_secret: Some(JWT_SECRET.into()),
            jwks_url: None,
            issuer: None,
            authorized_parties: vec![],
        }),
        public_git_url: "http://localhost:13000".into(),
        forgejo: ForgejoConfig {
            base_url: "http://localhost:13000".into(),
            owner: "clotho".into(),
            token: TokenSource::Inline(String::new()),
        },
    }
}

fn mint_clerk_jwt(sub: &str, org_id: Option<&str>, org_slug: Option<&str>) -> String {
    let mut claims = serde_json::json!({
        "sub": sub,
        "email": format!("{sub}@example.com"),
        "username": sub,
        "name": sub,
        "exp": chrono::Utc::now().timestamp() + 3600,
        "iat": chrono::Utc::now().timestamp(),
    });
    if let Some(oid) = org_id {
        claims["org_id"] = serde_json::json!(oid);
        claims["org_role"] = serde_json::json!("org:admin");
    }
    if let Some(slug) = org_slug {
        claims["org_slug"] = serde_json::json!(slug);
    }
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .unwrap()
}

async fn setup() -> Option<(sqlx::PgPool, Bootstrap, GatewayConfig)> {
    let url = database_url()?;
    let config = clerk_config();
    let pool = init_db(&url).await.ok()?;
    let bootstrap = Bootstrap::from_config(&config);
    control::ensure_bootstrap(&pool, &bootstrap).await.ok()?;
    Some((pool, bootstrap, config))
}

#[tokio::test]
async fn clerk_session_maps_user_and_org() {
    let Some((pool, bootstrap, config)) = setup().await else {
        return;
    };
    let app = router_with_pool(config, Some(pool.clone()), bootstrap).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let clerk_user = format!("user_{suffix}");
    let clerk_org = format!("org_{suffix}");
    let org_slug = format!("acme-{suffix}");
    let jwt = mint_clerk_jwt(&clerk_user, Some(&clerk_org), Some(&org_slug));

    let client = reqwest::Client::new();
    let me: serde_json::Value = client
        .get(format!("http://{addr}/api/v1/me"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["user"]["name"], clerk_user);

    let linked: Option<(String,)> =
        sqlx::query_as("select user_id from clerk_user_links where clerk_user_id = $1")
            .bind(&clerk_user)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(linked.is_some());

    let org_linked: Option<(String,)> =
        sqlx::query_as("select org_id from clerk_org_links where clerk_org_id = $1")
            .bind(&clerk_org)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(org_linked.is_some());
}

#[tokio::test]
async fn agent_token_rejected_on_human_routes() {
    let Some((pool, bootstrap, config)) = setup().await else {
        return;
    };
    let app = router_with_pool(config, Some(pool), bootstrap).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let agent_bearer =
        "Bearer clotho_agt_0000000000000000000000000000000000000000000000000000000000000000";
    let client = reqwest::Client::new();

    let me = client
        .get(format!("http://{addr}/api/v1/me"))
        .header("Authorization", agent_bearer)
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::UNAUTHORIZED);

    // Agent admin is human-only (ADR-0016); agent tokens must not pass.
    let agents = client
        .get(format!("http://{addr}/api/v1/agents"))
        .header("Authorization", agent_bearer)
        .send()
        .await
        .unwrap();
    assert_eq!(agents.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn fabric_layer_auth_lists_providers() {
    let Some((pool, bootstrap, mut config)) = setup().await else {
        return;
    };
    // Use bootstrap for this list test so we don't need a session.
    config.auth_provider = "bootstrap".into();
    config.clerk = None;
    config.auth_required = false;
    let app = router_with_pool(config, Some(pool), bootstrap).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/providers?layer=auth"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["layer"], "auth");
    let ids: Vec<&str> = body["providers"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["id"].as_str())
        .collect();
    assert!(ids.contains(&"bootstrap"));
    assert!(ids.contains(&"clerk"));

    let storage: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/providers?layer=storage"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(storage["providers"][0]["configured"], false);

    let network: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/providers?layer=network"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let tailscale = network["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "tailscale")
        .unwrap();
    assert_eq!(tailscale["configured"], false);
}

#[tokio::test]
async fn clotho_tok_still_works_under_clerk_provider() {
    let Some((pool, bootstrap, config)) = setup().await else {
        return;
    };
    let app = router_with_pool(config, Some(pool.clone()), bootstrap.clone()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Mint via bootstrap open path isn't available (auth_required); insert token.
    let plaintext = clotho_api_gateway::auth::mint_plaintext_token();
    let hash = clotho_api_gateway::auth::hash_token(&plaintext);
    sqlx::query(
        "insert into api_tokens (id, user_id, name, token_hash, token_prefix) values ($1,$2,'clerk-test',$3,$4)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&bootstrap.user_id)
    .bind(&hash)
    .bind(&plaintext[..plaintext.len().min(12)])
    .execute(&pool)
    .await
    .unwrap();

    let me: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/me"))
        .header("Authorization", format!("Bearer {plaintext}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["user"]["name"], "clotho");
}
