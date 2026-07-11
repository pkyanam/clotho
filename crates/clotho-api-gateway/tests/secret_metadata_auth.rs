//! Adversarial authorization checks for write-only secret metadata.

use std::panic::AssertUnwindSafe;

use axum::http::StatusCode;
use clotho_api_gateway::auth::{hash_token, mint_plaintext_token};
use clotho_api_gateway::control::Bootstrap;
use clotho_api_gateway::forgejo::{ForgejoConfig, TokenSource};
use clotho_api_gateway::{init_db, router_with_pool, GatewayConfig};
use futures::FutureExt;
use serde_json::Value;
use tokio::net::TcpListener;

fn database_url() -> Option<String> {
    for name in [
        "CLOTHO_SECRET_AUTH_TEST_DATABASE_URL",
        "CLOTHO_STAGE11_TEST_DATABASE_URL",
        "CLOTHO_CONTROL_PLANE_TEST_DATABASE_URL",
    ] {
        if let Ok(value) = std::env::var(name) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    let message = "CLOTHO_SECRET_AUTH_TEST_DATABASE_URL is not set (set it to a disposable Postgres database or run `just test-collab`)";
    let fail_on_skip = std::env::var("CLOTHO_TEST_FAIL_ON_SKIP")
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);
    if fail_on_skip {
        panic!("live-test gate refused to skip: {message}");
    }
    eprintln!("skipping: {message}");
    None
}

fn test_config(suffix: &str) -> GatewayConfig {
    GatewayConfig {
        vcs_grpc_url: "http://localhost:50051".into(),
        diff_grpc_url: "http://localhost:50055".into(),
        merge_queue_grpc_url: "http://localhost:50053".into(),
        agent_gateway_url: "http://localhost:8090".into(),
        agent_admin_token: String::new(),
        compute_grpc_url: "http://localhost:50057".into(),
        storage_grpc_url: "http://localhost:50052".into(),
        large_file_threshold_bytes: 10 * 1024 * 1024,
        storage_sdk_bridge_url: String::new(),
        webhook_secret: String::new(),
        webhook_url: String::new(),
        web_url: "http://localhost:3100".into(),
        compute_provider: "disabled".into(),
        compute_default_image: String::new(),
        actions_timeout_seconds: 30,
        configured_providers: std::collections::HashMap::new(),
        bootstrap_user_name: format!("secret-auth-user-a-{suffix}"),
        bootstrap_user_email: "secret-auth-a@clotho.invalid".into(),
        bootstrap_org_name: format!("secret-auth-org-a-{suffix}"),
        bootstrap_org_display_name: "Secret auth organization A".into(),
        // Exercise the local bootstrap fallback explicitly: no credential is
        // allowed to resolve to the bootstrap admin, but a supplied invalid
        // Bearer token must still fail instead of falling back.
        auth_required: false,
        auth_provider: "bootstrap".into(),
        clerk: None,
        public_git_url: "http://localhost:13000".into(),
        forgejo: ForgejoConfig {
            base_url: "http://localhost:13000".into(),
            owner: "clotho".into(),
            token: TokenSource::Inline(String::new()),
        },
    }
}

struct Fixture {
    user_a: String,
    user_b: String,
    org_a: String,
    org_b: String,
    repo_a: String,
    repo_b: String,
    token_a: String,
    token_b: String,
    secret_name: &'static str,
}

async fn seed(pool: &sqlx::PgPool, suffix: &str) -> Fixture {
    let fixture = Fixture {
        user_a: format!("secret-auth-user-a-{suffix}"),
        user_b: format!("secret-auth-user-b-{suffix}"),
        org_a: format!("secret-auth-org-a-{suffix}"),
        org_b: format!("secret-auth-org-b-{suffix}"),
        repo_a: format!("secret-auth-repo-a-{suffix}"),
        repo_b: format!("secret-auth-repo-b-{suffix}"),
        token_a: mint_plaintext_token(),
        token_b: mint_plaintext_token(),
        secret_name: "BUILD_CREDENTIAL",
    };

    for (id, name) in [
        (&fixture.user_a, format!("secret-user-a-{suffix}")),
        (&fixture.user_b, format!("secret-user-b-{suffix}")),
    ] {
        sqlx::query("insert into users (id, name, email, display_name) values ($1, $2, '', $2)")
            .bind(id)
            .bind(name)
            .execute(pool)
            .await
            .unwrap();
    }

    for (id, name, creator) in [
        (&fixture.org_a, &fixture.org_a, &fixture.user_a),
        (&fixture.org_b, &fixture.org_b, &fixture.user_b),
    ] {
        sqlx::query(
            "insert into orgs (id, name, display_name, forgejo_owner, created_by) values ($1, $2, $2, 'clotho', $3)",
        )
        .bind(id)
        .bind(name)
        .bind(creator)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("insert into org_memberships (org_id, user_id, role) values ($1, $2, 'admin')")
            .bind(id)
            .bind(creator)
            .execute(pool)
            .await
            .unwrap();
    }

    for (id, name, org_id, creator) in [
        (
            format!("secret-auth-repo-id-a-{suffix}"),
            &fixture.repo_a,
            &fixture.org_a,
            &fixture.user_a,
        ),
        (
            format!("secret-auth-repo-id-b-{suffix}"),
            &fixture.repo_b,
            &fixture.org_b,
            &fixture.user_b,
        ),
    ] {
        sqlx::query("insert into repos (id, org_id, name, created_by) values ($1, $2, $3, $4)")
            .bind(&id)
            .bind(org_id)
            .bind(name)
            .bind(creator)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into repo_permissions (repo_id, user_id, permission) values ($1, $2, 'admin')",
        )
        .bind(&id)
        .bind(creator)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"insert into secrets
               (id, scope, org_id, repo_id, name, description, ciphertext, created_by)
               values ($1, 'repo', $2, $3, $4, 'metadata-only test record', $5, $6)"#,
        )
        .bind(format!("secret-auth-repo-secret-{id}"))
        .bind(org_id)
        .bind(&id)
        .bind(fixture.secret_name)
        .bind(vec![0x5au8; 32])
        .bind(creator)
        .execute(pool)
        .await
        .unwrap();
    }

    for (org_id, creator) in [
        (&fixture.org_a, &fixture.user_a),
        (&fixture.org_b, &fixture.user_b),
    ] {
        sqlx::query(
            r#"insert into secrets
               (id, scope, org_id, name, description, ciphertext, created_by)
               values ($1, 'org', $2, $3, 'metadata-only test record', $4, $5)"#,
        )
        .bind(format!("secret-auth-org-secret-{org_id}"))
        .bind(org_id)
        .bind(fixture.secret_name)
        .bind(vec![0xa5u8; 32])
        .bind(creator)
        .execute(pool)
        .await
        .unwrap();
    }

    for (user_id, token) in [
        (&fixture.user_a, &fixture.token_a),
        (&fixture.user_b, &fixture.token_b),
    ] {
        sqlx::query(
            "insert into api_tokens (id, user_id, name, token_hash, token_prefix) values ($1, $2, 'secret-auth-test', $3, $4)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(user_id)
        .bind(hash_token(token))
        .bind(token.chars().take(12).collect::<String>())
        .execute(pool)
        .await
        .unwrap();
    }

    fixture
}

async fn cleanup(pool: &sqlx::PgPool, fixture: &Fixture) {
    let _ = sqlx::query("delete from secrets where org_id = any($1)")
        .bind(vec![fixture.org_a.clone(), fixture.org_b.clone()])
        .execute(pool)
        .await;
    let _ = sqlx::query("delete from repos where name = any($1)")
        .bind(vec![fixture.repo_a.clone(), fixture.repo_b.clone()])
        .execute(pool)
        .await;
    let _ = sqlx::query("delete from orgs where id = any($1)")
        .bind(vec![fixture.org_a.clone(), fixture.org_b.clone()])
        .execute(pool)
        .await;
    for user_id in [&fixture.user_a, &fixture.user_b] {
        let _ = sqlx::query("delete from users where id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
    }
}

async fn get(
    client: &reqwest::Client,
    base: &str,
    path: &str,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = client.get(format!("{base}{path}"));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let body = response.json().await.unwrap();
    (status, body)
}

fn assert_metadata_only(secret: &Value) {
    assert!(secret.get("value").is_none());
    assert!(secret.get("ciphertext").is_none());
    assert_eq!(secret["value_last4"], "");
}

#[tokio::test]
async fn secret_metadata_requires_owning_admin_without_existence_or_value_leaks() {
    let Some(database_url) = database_url() else {
        return;
    };
    let pool = init_db(&database_url).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let config = test_config(&suffix);
    let bootstrap = Bootstrap::from_config(&config);
    let fixture = seed(&pool, &suffix).await;
    let app = router_with_pool(config, Some(pool.clone()), bootstrap).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    let outcome = AssertUnwindSafe(async {
        for (org, repo, token) in [
            (&fixture.org_a, &fixture.repo_a, &fixture.token_a),
            (&fixture.org_b, &fixture.repo_b, &fixture.token_b),
        ] {
            let (status, list) = get(
                &client,
                &base,
                &format!("/api/v1/orgs/{org}/secrets"),
                Some(token),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(list["secrets"].as_array().unwrap().len(), 1);
            assert_metadata_only(&list["secrets"][0]);

            let (status, secret) = get(
                &client,
                &base,
                &format!("/api/v1/orgs/{org}/secrets/{}", fixture.secret_name),
                Some(token),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_metadata_only(&secret);

            let (status, list) = get(
                &client,
                &base,
                &format!("/api/v1/repos/{repo}/secrets"),
                Some(token),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(list["secrets"].as_array().unwrap().len(), 1);
            assert_metadata_only(&list["secrets"][0]);

            let (status, secret) = get(
                &client,
                &base,
                &format!("/api/v1/repos/{repo}/secrets/{}", fixture.secret_name),
                Some(token),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_metadata_only(&secret);
        }

        for (existing, absent) in [
            (
                format!(
                    "/api/v1/orgs/{}/secrets/{}",
                    fixture.org_b, fixture.secret_name
                ),
                format!("/api/v1/orgs/{}/secrets/DOES_NOT_EXIST", fixture.org_b),
            ),
            (
                format!(
                    "/api/v1/repos/{}/secrets/{}",
                    fixture.repo_b, fixture.secret_name
                ),
                format!("/api/v1/repos/{}/secrets/DOES_NOT_EXIST", fixture.repo_b),
            ),
        ] {
            let (existing_status, existing_body) =
                get(&client, &base, &existing, Some(&fixture.token_a)).await;
            let (absent_status, absent_body) =
                get(&client, &base, &absent, Some(&fixture.token_a)).await;
            assert_eq!(existing_status, StatusCode::FORBIDDEN);
            assert_eq!(absent_status, existing_status);
            for field in ["code", "message", "retryable"] {
                assert_eq!(absent_body[field], existing_body[field], "{field}");
            }
            assert_eq!(existing_body["code"], "permission_denied");
            assert!(!existing_body.to_string().contains(fixture.secret_name));
        }

        for path in [
            format!("/api/v1/orgs/{}/secrets", fixture.org_b),
            format!("/api/v1/repos/{}/secrets", fixture.repo_b),
        ] {
            let (status, body) = get(&client, &base, &path, Some(&fixture.token_a)).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
            assert_eq!(body["code"], "permission_denied");
        }

        let own_secret = format!(
            "/api/v1/orgs/{}/secrets/{}",
            fixture.org_a, fixture.secret_name
        );
        let (status, secret) = get(&client, &base, &own_secret, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_metadata_only(&secret);

        for token in ["clotho_tok_invalid", "not-a-clotho-token"] {
            let (status, body) = get(&client, &base, &own_secret, Some(token)).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
            assert_eq!(body["code"], "unauthenticated");
            assert!(!body.to_string().contains(fixture.secret_name));
        }
    })
    .catch_unwind()
    .await;

    server.abort();
    cleanup(&pool, &fixture).await;
    if let Err(payload) = outcome {
        std::panic::resume_unwind(payload);
    }
}
