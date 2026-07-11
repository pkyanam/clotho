//! Stage 3 exit-condition test (docs/prd.md §5): creating a repo through
//! Clotho's API produces a real Forgejo project with working issues/PRs,
//! backed by a jj-managed git repo — and commits written through the
//! clotho-vcs gRPC API render in Forgejo.
//!
//! Requires the dev stack (`just dev`), then run via `just test-collab`, or
//! set `CLOTHO_COLLAB_TEST_GATEWAY_URL` (e.g. `http://localhost:8080`).
//! Skipped when unset so plain `cargo test` stays green. Optional overrides:
//! `CLOTHO_COLLAB_TEST_VCS_GRPC_URL` (default `http://localhost:50051`),
//! `CLOTHO_COLLAB_TEST_FORGEJO_URL` (default `http://localhost:13000`), and
//! `CLOTHO_COLLAB_TEST_FORGEJO_{USER,PASSWORD}` (default the dev admin,
//! `clotho`/`clotho-dev`).
//! `CLOTHO_TEST_FAIL_ON_SKIP=1` makes a missing live endpoint fatal (CI and
//! `just test-collab` set it). Test-owned repos are removed after the test;
//! `CLOTHO_TEST_KEEP_FIXTURES_ON_FAILURE=1` preserves only failed fixtures.

mod support;

use clotho_common::pb::vcs::v1::{vcs_client::VcsClient, CommitRequest, FileChange};
use futures::FutureExt;
use serde_json::{json, Value};

struct TestEnv {
    gateway_url: String,
    vcs_grpc_url: String,
    forgejo_url: String,
    forgejo_user: String,
    forgejo_password: String,
}

fn test_env() -> Option<TestEnv> {
    let gateway_url = support::live_env(
        "CLOTHO_COLLAB_TEST_GATEWAY_URL",
        "start the stack via `just dev`",
    )?;
    let env_or = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
    Some(TestEnv {
        gateway_url,
        vcs_grpc_url: env_or("CLOTHO_COLLAB_TEST_VCS_GRPC_URL", "http://localhost:50051"),
        forgejo_url: env_or("CLOTHO_COLLAB_TEST_FORGEJO_URL", "http://localhost:13000"),
        forgejo_user: env_or("CLOTHO_COLLAB_TEST_FORGEJO_USER", "clotho"),
        forgejo_password: env_or("CLOTHO_COLLAB_TEST_FORGEJO_PASSWORD", "clotho-dev"),
    })
}

async fn forgejo_json(env: &TestEnv, request: reqwest::RequestBuilder, context: &str) -> Value {
    let response = request
        .basic_auth(&env.forgejo_user, Some(&env.forgejo_password))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{context}: {e}"));
    let status = response.status();
    let body = response.text().await.unwrap();
    assert!(status.is_success(), "{context}: {status}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{context}: invalid json: {e}"))
}

async fn gateway_json(env: &TestEnv, path: &str, context: &str) -> Value {
    let response = reqwest::get(format!("{}{}", env.gateway_url, path))
        .await
        .unwrap_or_else(|e| panic!("{context}: {e}"));
    let status = response.status();
    let body = response.text().await.unwrap();
    assert!(status.is_success(), "{context}: {status}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{context}: invalid json: {e}"))
}

#[tokio::test]
async fn repo_created_through_clotho_api_is_a_real_forgejo_project() {
    let Some(env) = test_env() else { return };
    let http = reqwest::Client::new();

    // Unique per-run name so runs never collide.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("stage3-{nanos}");

    let outcome = std::panic::AssertUnwindSafe(async {
        // 1. One call to Clotho's API provisions both systems.
        let response = http
            .post(format!("{}/api/v1/repos", env.gateway_url))
            .json(&json!({ "name": name }))
            .send()
            .await
            .expect("gateway reachable");
        assert_eq!(response.status(), 201, "{}", response.text().await.unwrap());
        let created: Value = response.json().await.unwrap();
        let owner = created["owner"].as_str().unwrap().to_string();
        let initial_commit = created["initial_commit_id"].as_str().unwrap().to_string();
        assert_eq!(created["info"]["default_branch"], "main");
        assert_eq!(created["info"]["has_issues"], true);
        assert_eq!(created["info"]["has_pull_requests"], true);
        let repo_api = format!("{}/api/v1/repos/{owner}/{name}", env.forgejo_url);

        // 2. Commit through the clotho-vcs gRPC API — no git CLI anywhere.
        let mut vcs = VcsClient::connect(env.vcs_grpc_url.clone()).await.unwrap();
        let commit = vcs
            .commit(CommitRequest {
                repo: name.clone(),
                parent_commit_ids: vec![],
                files: vec![FileChange {
                    path: "README.md".into(),
                    content: b"# stage3\n\nwoven by agents.\n".to_vec(),
                    executable: false,
                }],
                deleted_paths: vec![],
                message: "add readme via clotho-vcs".into(),
                author_name: "agent-a".into(),
                author_email: "agent-a@agents.clotho.internal".into(),
            })
            .await
            .unwrap()
            .into_inner();

        // 3. Forgejo renders both commits on main (it reads the same git repo).
        let commits = forgejo_json(
            &env,
            http.get(format!("{repo_api}/commits?sha=main&limit=10")),
            "list commits",
        )
        .await;
        let shas: Vec<&str> = commits
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["sha"].as_str().unwrap())
            .collect();
        assert!(
            shas.contains(&commit.commit_id.as_str()),
            "vcs commit visible"
        );
        assert!(
            shas.contains(&initial_commit.as_str()),
            "initial commit visible"
        );

        // 4. Issues work.
        let issue = forgejo_json(
            &env,
            http.post(format!("{repo_api}/issues"))
                .json(&json!({ "title": "first thread", "body": "filed via API" })),
            "create issue",
        )
        .await;
        assert_eq!(issue["state"], "open");
        let facade_issue = http
            .post(format!("{}/api/v1/repos/{name}/issues", env.gateway_url))
            .json(&json!({ "title": "facade thread", "body": "filed through Clotho" }))
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(
            facade_issue["number"],
            issue["number"].as_i64().unwrap() + 1
        );
        let facade_comment = http
            .post(format!(
                "{}/api/v1/repos/{name}/issues/{}/comments",
                env.gateway_url, facade_issue["number"]
            ))
            .json(&json!({ "body": "comment through Clotho" }))
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(facade_comment["body"], "comment through Clotho");
        let facade_list = gateway_json(
            &env,
            &format!("/api/v1/repos/{name}/issues?state=all"),
            "list issues through facade",
        )
        .await;
        assert!(facade_list["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["number"] == facade_issue["number"]));

        // 5. PRs work: branch at the initial commit, PR for the agent's commit.
        forgejo_json(
            &env,
            http.post(format!("{repo_api}/branches")).json(&json!({
                "new_branch_name": "checkpoint-1",
                "old_ref_name": initial_commit,
            })),
            "create branch",
        )
        .await;
        let pr = forgejo_json(
            &env,
            http.post(format!("{repo_api}/pulls")).json(&json!({
                "title": "add readme",
                "head": "main",
                "base": "checkpoint-1",
            })),
            "create pull request",
        )
        .await;
        assert_eq!(pr["state"], "open");
        assert_eq!(pr["mergeable"], true);

        // 6. Duplicate creation is rejected cleanly at the edge.
        let dup = http
            .post(format!("{}/api/v1/repos", env.gateway_url))
            .json(&json!({ "name": name }))
            .send()
            .await
            .unwrap();
        assert_eq!(dup.status(), 409);
    })
    .catch_unwind()
    .await;
    let cleanup =
        support::cleanup_repo_fixture(&http, &env.gateway_url, &name, outcome.is_err()).await;
    support::finish_after_cleanup(outcome, cleanup);
}
