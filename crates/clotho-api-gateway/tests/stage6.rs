//! Stage 6 exit-condition test (docs/prd.md §5): a human can browse a repo,
//! view a PR with a structured diff, and see which agent sessions have
//! touched it recently — all through the api-gateway REST surface the web
//! app consumes (docs/adr/0007). Conflicted commits surface as first-class,
//! flagged, materialized conflicts (ADR-0006), never hidden.
//!
//! Requires the dev stack (`just dev`), then run via `just test-collab`, or
//! set `CLOTHO_COLLAB_TEST_GATEWAY_URL` (e.g. `http://localhost:8080`).
//! Skipped when unset so plain `cargo test` stays green. Optional overrides
//! as in tests/gateway.rs.

use clotho_common::pb::vcs::v1::{
    vcs_client::VcsClient, CommitRequest, FileChange, IntegrateCommitRequest,
};
use serde_json::{json, Value};

struct TestEnv {
    gateway_url: String,
    vcs_grpc_url: String,
    forgejo_url: String,
    forgejo_user: String,
    forgejo_password: String,
}

fn test_env() -> Option<TestEnv> {
    let Ok(gateway_url) = std::env::var("CLOTHO_COLLAB_TEST_GATEWAY_URL") else {
        eprintln!(
            "skipping: CLOTHO_COLLAB_TEST_GATEWAY_URL not set (start the stack via `just dev`)"
        );
        return None;
    };
    let env_or = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
    Some(TestEnv {
        gateway_url,
        vcs_grpc_url: env_or("CLOTHO_COLLAB_TEST_VCS_GRPC_URL", "http://localhost:50051"),
        forgejo_url: env_or("CLOTHO_COLLAB_TEST_FORGEJO_URL", "http://localhost:13000"),
        forgejo_user: env_or("CLOTHO_COLLAB_TEST_FORGEJO_USER", "clotho"),
        forgejo_password: env_or("CLOTHO_COLLAB_TEST_FORGEJO_PASSWORD", "clotho-dev"),
    })
}

async fn get_json(env: &TestEnv, path: &str, context: &str) -> Value {
    let response = reqwest::Client::new()
        .get(format!("{}{path}", env.gateway_url))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{context}: {e}"));
    let status = response.status();
    let body = response.text().await.unwrap();
    assert!(status.is_success(), "{context}: {status}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{context}: invalid json: {e}"))
}

fn commit_request(
    repo: &str,
    parents: Vec<String>,
    path: &str,
    content: &[u8],
    message: &str,
) -> CommitRequest {
    CommitRequest {
        repo: repo.into(),
        parent_commit_ids: parents,
        files: vec![FileChange {
            path: path.into(),
            content: content.to_vec(),
            executable: false,
        }],
        deleted_paths: vec![],
        message: message.into(),
        author_name: "agent-a".into(),
        author_email: "agent-a@agents.clotho.internal".into(),
    }
}

#[tokio::test]
async fn human_browses_repo_and_reviews_structured_pr_diff() {
    let Some(env) = test_env() else { return };
    let http = reqwest::Client::new();

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("stage6-{nanos}");

    // 1. Provision through the gateway (Stage 3 flow).
    let created: Value = http
        .post(format!("{}/api/v1/repos", env.gateway_url))
        .json(&json!({ "name": name }))
        .send()
        .await
        .expect("gateway reachable")
        .json()
        .await
        .unwrap();
    let initial_commit = created["initial_commit_id"].as_str().unwrap().to_string();
    let owner = created["owner"].as_str().unwrap().to_string();

    // 2. A CLI/human write creates real code through the REST edge.
    let mut vcs = VcsClient::connect(env.vcs_grpc_url.clone()).await.unwrap();
    let code: Value = http
        .post(format!("{}/api/v1/repos/{name}/commits", env.gateway_url))
        .json(&json!({
            "message": "add spin",
            "author_name": "stage6-cli",
            "author_email": "stage6-cli@clotho.internal",
            "files": [{
                "path": "src/lib.rs",
                "content": "pub fn spin() -> u32 {\n    1\n}\n"
            }]
        }))
        .send()
        .await
        .expect("gateway commit reachable")
        .json()
        .await
        .unwrap();
    let code_commit_id = code["commit_id"].as_str().unwrap().to_string();

    // 3. Repo browsing through the gateway: list, detail, tree, file,
    //    commits, op log.
    let repos = get_json(&env, "/api/v1/repos", "list repos").await;
    assert!(
        repos["repos"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["name"] == name.as_str()),
        "new repo appears in the list"
    );

    let detail = get_json(&env, &format!("/api/v1/repos/{name}"), "repo detail").await;
    assert_eq!(detail["main_commit_id"], code_commit_id);
    assert_eq!(detail["forgejo"]["default_branch"], "main");

    let tree = get_json(&env, &format!("/api/v1/repos/{name}/tree"), "tree").await;
    let files = tree["files"].as_array().unwrap();
    assert_eq!(files[0]["path"], "src/lib.rs");
    assert_eq!(files[0]["conflicted"], false);

    let file = get_json(
        &env,
        &format!("/api/v1/repos/{name}/file?path=src/lib.rs"),
        "file contents",
    )
    .await;
    assert_eq!(file["binary"], false);
    assert!(file["content"].as_str().unwrap().contains("pub fn spin"));

    let commits = get_json(&env, &format!("/api/v1/repos/{name}/commits"), "commits").await;
    let commit_ids: Vec<&str> = commits["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["commit_id"].as_str().unwrap())
        .collect();
    assert_eq!(commit_ids[0], code_commit_id, "newest first");
    assert!(commit_ids.contains(&initial_commit.as_str()));

    let op_log = get_json(&env, &format!("/api/v1/repos/{name}/oplog"), "op log").await;
    assert!(op_log["entries"].as_array().unwrap().len() >= 3);

    // 4. A PR in Forgejo (base branch pinned at the initial commit, head =
    //    main), then reviewed through the gateway's structured diff.
    let repo_api = format!("{}/api/v1/repos/{owner}/{name}", env.forgejo_url);
    let forgejo = |req: reqwest::RequestBuilder| {
        req.basic_auth(env.forgejo_user.clone(), Some(env.forgejo_password.clone()))
    };
    forgejo(http.post(format!("{repo_api}/branches")).json(&json!({
        "new_branch_name": "review-base",
        "old_ref_name": initial_commit,
    })))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap();
    let pr: Value = forgejo(http.post(format!("{repo_api}/pulls")).json(&json!({
        "title": "add spin",
        "head": "main",
        "base": "review-base",
    })))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    let pr_number = pr["number"].as_i64().unwrap();

    let pulls = get_json(&env, &format!("/api/v1/repos/{name}/pulls"), "pull list").await;
    assert!(pulls["pulls"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["number"] == pr_number));

    let diff = get_json(
        &env,
        &format!("/api/v1/repos/{name}/pulls/{pr_number}/diff"),
        "structured pr diff",
    )
    .await;
    assert_eq!(diff["to_commit_id"], code_commit_id);
    assert_eq!(diff["conflicted"], false);
    let file_diff = &diff["files"][0];
    assert_eq!(file_diff["path"], "src/lib.rs");
    assert_eq!(file_diff["status"], "added");
    assert_eq!(file_diff["language"], "rust");
    let symbol = &file_diff["symbols"][0];
    assert_eq!(symbol["name"], "spin");
    assert_eq!(symbol["kind"], "function");
    assert_eq!(symbol["status"], "added");
    let hunk = &file_diff["hunks"][0];
    assert_eq!(hunk["lines"][0]["kind"], "add");
    assert_eq!(hunk["lines"][0]["text"], "pub fn spin() -> u32 {");

    // 5. Agent presence: the endpoint answers (the audit → sessions data
    //    path is exercised end-to-end in the agent-gateway's stage 4 test).
    let sessions = get_json(
        &env,
        &format!("/api/v1/repos/{name}/agent-sessions"),
        "agent sessions",
    )
    .await;
    assert!(sessions["sessions"].as_array().is_some());

    // 6. Conflicts stay first-class all the way to the browser: two agents
    //    diverge on the same file, the integration lands as a conflicted
    //    commit (ADR-0006), and the gateway flags + materializes it.
    let side_a = vcs
        .commit(commit_request(
            &name,
            vec![code_commit_id.clone()],
            "src/lib.rs",
            b"pub fn spin() -> u32 {\n    2\n}\n",
            "agent a: spin -> 2",
        ))
        .await
        .unwrap()
        .into_inner();
    let side_b = vcs
        .commit(commit_request(
            &name,
            vec![code_commit_id.clone()],
            "src/lib.rs",
            b"pub fn spin() -> u32 {\n    3\n}\n",
            "agent b: spin -> 3",
        ))
        .await
        .unwrap()
        .into_inner();
    for commit_id in [&side_a.commit_id, &side_b.commit_id] {
        vcs.integrate_commit(IntegrateCommitRequest {
            repo: name.clone(),
            commit_id: commit_id.clone(),
        })
        .await
        .unwrap();
    }

    let tree = get_json(
        &env,
        &format!("/api/v1/repos/{name}/tree"),
        "conflicted tree",
    )
    .await;
    let entry = tree["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "src/lib.rs")
        .expect("conflicted file still listed");
    assert_eq!(entry["conflicted"], true, "tree flags the conflict");

    let file = get_json(
        &env,
        &format!("/api/v1/repos/{name}/file?path=src/lib.rs"),
        "conflicted file contents",
    )
    .await;
    assert_eq!(file["conflicted"], true);
    let content = file["content"].as_str().unwrap();
    assert!(
        content.contains("<<<<<<<") && content.contains(">>>>>>>"),
        "materialized jj conflict markers are visible: {content}"
    );
}
