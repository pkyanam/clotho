//! Stage 4 exit-condition test (docs/prd.md §5): a real MCP client
//! authenticates as a scoped agent identity, checkpoints work, commits and
//! submits a change, then restores — entirely through MCP tool calls. Plus the
//! guard rails: scope enforcement, rejected credentials, and the per-call
//! audit log.
//!
//! Requires the dev stack (`just dev`), then run via `just test-agent`, or
//! set `CLOTHO_AGENT_TEST_MCP_URL` (e.g. `http://localhost:8090`). Skipped
//! when unset so plain `cargo test` stays green. Optional overrides:
//! `CLOTHO_AGENT_TEST_ADMIN_TOKEN` (default the dev stack's admin token) and
//! `CLOTHO_AGENT_TEST_VCS_GRPC_URL` (default `http://localhost:50051`).

use clotho_common::pb::vcs::v1::{
    vcs_client::VcsClient, CommitRequest, FileChange, GetHeadsRequest, InitRepoRequest,
};
use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::ServiceExt as _;
use serde_json::{json, Value};

struct TestEnv {
    mcp_base_url: String,
    admin_token: String,
    vcs_grpc_url: String,
    api_url: String,
}

fn test_env() -> Option<TestEnv> {
    let Ok(mcp_base_url) = std::env::var("CLOTHO_AGENT_TEST_MCP_URL") else {
        eprintln!("skipping: CLOTHO_AGENT_TEST_MCP_URL not set (start the stack via `just dev`)");
        return None;
    };
    let env_or = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
    Some(TestEnv {
        mcp_base_url,
        admin_token: env_or("CLOTHO_AGENT_TEST_ADMIN_TOKEN", "clotho-agent-admin-dev"),
        vcs_grpc_url: env_or("CLOTHO_AGENT_TEST_VCS_GRPC_URL", "http://localhost:50051"),
        api_url: env_or("CLOTHO_AGENT_TEST_API_URL", "http://localhost:8080"),
    })
}

async fn admin_post(env: &TestEnv, path: &str, body: Value, context: &str) -> Value {
    let response = reqwest::Client::new()
        .post(format!("{}{path}", env.mcp_base_url))
        .bearer_auth(&env.admin_token)
        .json(&body)
        .send()
        .await
        .unwrap_or_else(|e| panic!("{context}: {e}"));
    let status = response.status();
    let body = response.text().await.unwrap();
    assert!(status.is_success(), "{context}: {status}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{context}: invalid json: {e}"))
}

async fn mcp_client(
    env: &TestEnv,
    token: &str,
) -> rmcp::service::RunningService<rmcp::RoleClient, ClientInfo> {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", env.mcp_base_url))
            .auth_header(token),
    );
    ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("clotho-stage4-test", "0.1.0"),
    )
    .serve(transport)
    .await
    .expect("mcp client connects")
}

/// Call a tool and return (is_error, parsed JSON of the first text block).
async fn call_tool(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ClientInfo>,
    name: &str,
    arguments: Value,
) -> (bool, Value) {
    let result = client
        .call_tool(
            CallToolRequestParams::new(name.to_string())
                .with_arguments(arguments.as_object().cloned().unwrap()),
        )
        .await
        .unwrap_or_else(|e| panic!("call_tool {name}: {e}"));
    let result = serde_json::to_value(&result).unwrap();
    let is_error = result["isError"].as_bool().unwrap_or(false);
    let text = result["content"][0]["text"].as_str().unwrap_or("{}");
    (is_error, serde_json::from_str(text).unwrap_or(Value::Null))
}

#[tokio::test]
async fn scoped_agent_checkpoints_breaks_and_restores_over_mcp() {
    let Some(env) = test_env() else { return };

    // A unique repo per run, seeded with a known-good commit. Repo creation
    // and raw commits are the platform's job (the api-gateway in Stage 3);
    // the MCP surface is checkpoint/restore/orient/diff.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let repo = format!("stage4-{nanos}");
    let mut vcs = VcsClient::connect(env.vcs_grpc_url.clone()).await.unwrap();
    vcs.init_repo(InitRepoRequest { name: repo.clone() })
        .await
        .unwrap();
    let good = vcs
        .commit(CommitRequest {
            repo: repo.clone(),
            parent_commit_ids: vec![],
            files: vec![FileChange {
                path: "src/lib.rs".into(),
                content: b"pub fn spin() -> u32 { 1 }\n".to_vec(),
                executable: false,
            }],
            deleted_paths: vec![],
            message: "known good state".into(),
            author_name: "agent-a".into(),
            author_email: "agent-a@agents.clotho.internal".into(),
        })
        .await
        .unwrap()
        .into_inner();

    // 1. Provision a scoped agent identity: this repo and bounded tools.
    let agent_name = format!("weaver-{nanos}");
    admin_post(
        &env,
        "/admin/v1/agents",
        json!({ "name": agent_name, "description": "stage 4 exit-condition agent" }),
        "create agent",
    )
    .await;
    let minted = admin_post(
        &env,
        &format!("/admin/v1/agents/{agent_name}/tokens"),
        json!({
            "allowed_repos": [repo],
            "allowed_tools": [
                "checkpoint",
                "restore_to",
                "orient_repo",
                "diff_symbol",
                "commit",
                "submit_change",
                "get_file",
                "list_repos",
                "get_activity"
            ],
        }),
        "mint token",
    )
    .await;
    let token_id = minted["token_id"].as_str().unwrap();

    // 1b. List agents and tokens; revoke is idempotent on a fresh token later.
    let listed = reqwest::Client::new()
        .get(format!("{}/admin/v1/agents", env.mcp_base_url))
        .bearer_auth(&env.admin_token)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(
        listed["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == agent_name.as_str()),
        "list agents missing {agent_name}"
    );
    let tokens = reqwest::Client::new()
        .get(format!(
            "{}/admin/v1/agents/{agent_name}/tokens",
            env.mcp_base_url
        ))
        .bearer_auth(&env.admin_token)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(tokens["tokens"].as_array().unwrap().len(), 1);
    let token = minted["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("clotho_agt_"));

    // 2. A real MCP client connects over streamable HTTP with that token.
    let client = mcp_client(&env, &token).await;
    let tools = client.list_tools(Default::default()).await.unwrap();
    let mut names: Vec<_> = tools.tools.iter().map(|t| t.name.as_ref()).collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "checkpoint",
            "commit",
            "diff_symbol",
            "get_activity",
            "get_file",
            "list_repos",
            "orient_repo",
            "restore_to",
            "submit_change"
        ]
    );

    // Bounded platform listings preserve the canonical REST page envelope.
    let (err, mcp_repos) = call_tool(&client, "list_repos", json!({ "limit": 1 })).await;
    assert!(!err, "list_repos failed: {mcp_repos}");
    let rest_repos: Value = reqwest::get(format!("{}/api/v1/repos?limit=1", env.api_url))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(mcp_repos["repos"], rest_repos["repos"]);
    assert_eq!(mcp_repos["next_cursor"], rest_repos["next_cursor"]);

    let (err, mcp_activity) = call_tool(&client, "get_activity", json!({ "limit": 1 })).await;
    assert!(!err, "get_activity failed: {mcp_activity}");
    let rest_activity: Value = reqwest::get(format!("{}/api/v1/activity?limit=1", env.api_url))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(mcp_activity["events"], rest_activity["events"]);
    assert_eq!(mcp_activity["next_cursor"], rest_activity["next_cursor"]);

    // 3. Orientation: heads, main target, op log, and file tree.
    let (err, orient) = call_tool(&client, "orient_repo", json!({ "repo": repo })).await;
    assert!(!err, "orient_repo failed: {orient}");
    assert_eq!(orient["main_commit_id"], good.commit_id);
    assert_eq!(orient["files"][0]["path"], "src/lib.rs");
    assert!(orient["op_log"].as_array().unwrap().len() >= 2);

    // REST-backed failures remain tool results and preserve the canonical
    // Clotho code plus request correlation in MCP error data.
    let (err, missing) = call_tool(
        &client,
        "get_file",
        json!({ "repo": repo.clone(), "path": "does-not-exist.txt" }),
    )
    .await;
    assert!(err, "missing file unexpectedly succeeded: {missing}");
    let rest_missing: Value = reqwest::get(format!(
        "{}/api/v1/repos/{repo}/file?path=does-not-exist.txt",
        env.api_url
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(missing["data"]["code"], rest_missing["code"]);
    assert_eq!(missing["data"]["code"], "not_found");
    assert!(
        missing["data"]["request_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "MCP error omitted REST request correlation: {missing}"
    );

    // 4. Checkpoint the known-good state.
    let (err, checkpoint) = call_tool(
        &client,
        "checkpoint",
        json!({ "repo": repo, "label": "before risky refactor" }),
    )
    .await;
    assert!(!err, "checkpoint failed: {checkpoint}");
    let checkpoint_op = checkpoint["operation_id"].as_str().unwrap().to_string();

    // 5. Intentionally break something, now through the agent-native write
    //    path instead of raw vcs gRPC.
    let (err, broken) = call_tool(
        &client,
        "commit",
        json!({
            "repo": repo.clone(),
            "message": "risky refactor (broken)",
            "files": [{
                "path": "src/lib.rs",
                "content": "pub fn spin() -> u32 { compile_error!() }\n"
            }]
        }),
    )
    .await;
    assert!(!err, "commit failed: {broken}");
    let broken_commit_id = broken["commit_id"].as_str().unwrap().to_string();

    // 6. Land it through the merge queue over MCP. In this simple case the
    //    commit already fast-forwarded main at write time, so integration is
    //    idempotent; the important property is that agents no longer need a
    //    raw gRPC escape hatch to submit.
    let (err, submitted) = call_tool(
        &client,
        "submit_change",
        json!({ "repo": repo.clone(), "commit_id": broken_commit_id }),
    )
    .await;
    assert!(!err, "submit_change failed: {submitted}");
    assert_eq!(submitted["commit_id"], broken_commit_id);
    assert_eq!(submitted["conflicted"], false);

    // 7. The structured diff names the damaged symbol — no patch text.
    let (err, diff) = call_tool(&client, "diff_symbol", json!({ "repo": repo })).await;
    assert!(!err, "diff_symbol failed: {diff}");
    assert_eq!(diff["from_commit_id"], good.commit_id);
    assert_eq!(diff["to_commit_id"], broken_commit_id);
    assert_eq!(diff["files"][0]["language"], "rust");
    let symbol = &diff["files"][0]["symbols"][0];
    assert_eq!(symbol["name"], "spin");
    assert_eq!(symbol["kind"], "function");
    assert_eq!(symbol["status"], "modified");

    // 8. Restore to the checkpoint — entirely through MCP.
    let (err, restored) = call_tool(
        &client,
        "restore_to",
        json!({ "repo": repo, "operation_id": checkpoint_op }),
    )
    .await;
    assert!(!err, "restore_to failed: {restored}");
    let heads = vcs
        .get_heads(GetHeadsRequest { repo: repo.clone() })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(heads.main_commit_id, good.commit_id, "main is good again");

    // 9. Scope enforcement: a token for a different repo is denied (and the
    //    denial is audited), without leaking whether the repo exists.
    let outsider_name = format!("outsider-{nanos}");
    admin_post(
        &env,
        "/admin/v1/agents",
        json!({ "name": outsider_name }),
        "create outsider agent",
    )
    .await;
    let outsider_token = admin_post(
        &env,
        &format!("/admin/v1/agents/{outsider_name}/tokens"),
        json!({ "allowed_repos": ["some-other-repo"], "allowed_tools": ["*"] }),
        "mint outsider token",
    )
    .await["token"]
        .as_str()
        .unwrap()
        .to_string();
    let outsider = mcp_client(&env, &outsider_token).await;
    let (err, denied) = call_tool(
        &outsider,
        "checkpoint",
        json!({ "repo": repo, "label": "should not work" }),
    )
    .await;
    assert!(err, "out-of-scope call must be an error result");
    assert_eq!(denied, Value::Null); // denial message is plain text, not JSON
    outsider.cancel().await.unwrap();

    // 10. Garbage credentials never reach the tools.
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", env.mcp_base_url))
            .auth_header("clotho_agt_not_a_real_token"),
    );
    let unauthorized = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("clotho-stage4-test", "0.1.0"),
    )
    .serve(transport)
    .await;
    assert!(unauthorized.is_err(), "bad token must fail to initialize");

    client.cancel().await.unwrap();

    // 11. Every call above is in the audit log with the agent's identity.
    let audit = reqwest::Client::new()
        .get(format!(
            "{}/admin/v1/agents/{agent_name}/audit",
            env.mcp_base_url
        ))
        .bearer_auth(&env.admin_token)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let entries = audit.as_array().unwrap();
    let calls: Vec<(&str, &str)> = entries
        .iter()
        .map(|e| (e["tool"].as_str().unwrap(), e["status"].as_str().unwrap()))
        .collect();
    for tool in [
        "orient_repo",
        "checkpoint",
        "commit",
        "submit_change",
        "diff_symbol",
        "restore_to",
    ] {
        assert!(
            calls.contains(&(tool, "ok")),
            "audit log missing ok entry for {tool}: {calls:?}"
        );
    }
    let outsider_audit = reqwest::Client::new()
        .get(format!(
            "{}/admin/v1/agents/{outsider_name}/audit",
            env.mcp_base_url
        ))
        .bearer_auth(&env.admin_token)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let denied_entry = &outsider_audit.as_array().unwrap()[0];
    assert_eq!(denied_entry["tool"], "checkpoint");
    assert_eq!(denied_entry["status"], "denied");
    assert_eq!(denied_entry["repo"], repo);

    // 12. Repo presence (Stage 6): the sessions view aggregates that audit
    //     trail per (agent, token) — both agents show up on this repo, the
    //     denied outsider flagged as such, newest activity first.
    let sessions = reqwest::Client::new()
        .get(format!(
            "{}/admin/v1/repos/{repo}/sessions",
            env.mcp_base_url
        ))
        .bearer_auth(&env.admin_token)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let sessions = sessions.as_array().unwrap();
    let weaver = sessions
        .iter()
        .find(|s| s["agent"] == agent_name.as_str())
        .expect("weaver session present");
    assert_eq!(weaver["last_status"], "ok");
    assert!(weaver["tool_calls"].as_i64().unwrap() >= 6);
    let outsider_session = sessions
        .iter()
        .find(|s| s["agent"] == outsider_name.as_str())
        .expect("outsider session present");
    assert_eq!(outsider_session["last_status"], "denied");
    assert_eq!(outsider_session["last_tool"], "checkpoint");

    // 13. Revoke the primary token and confirm it no longer authenticates.
    let revoke = reqwest::Client::new()
        .delete(format!(
            "{}/admin/v1/agents/{agent_name}/tokens/{token_id}",
            env.mcp_base_url
        ))
        .bearer_auth(&env.admin_token)
        .send()
        .await
        .unwrap();
    assert_eq!(revoke.status(), 204);
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", env.mcp_base_url))
            .auth_header(minted["token"].as_str().unwrap()),
    );
    let revoked = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("clotho-stage4-test", "0.1.0"),
    )
    .serve(transport)
    .await;
    assert!(revoked.is_err(), "revoked token must fail to initialize");
}
