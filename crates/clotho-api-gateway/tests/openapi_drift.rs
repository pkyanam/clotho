//! Stage 15: ensure every stable gateway route path appears in docs/openapi.yaml.
//! Hand-maintained OpenAPI is the product contract; this test fails CI when a
//! route is added to the Axum router without documenting it.

const OPENAPI: &str = include_str!("../../../docs/openapi.yaml");

/// Path templates as they appear under `paths:` in the OpenAPI document.
/// Keep in sync with `clotho_api_gateway::router` route registrations.
const EXPECTED_PATHS: &[&str] = &[
    "/healthz",
    "/openapi.yaml",
    "/api/v1/users",
    "/api/v1/orgs",
    "/api/v1/orgs/{org}",
    "/api/v1/orgs/{org}/repos",
    "/api/v1/activity",
    "/api/v1/repos",
    "/api/v1/repos/{name}",
    "/api/v1/repos/{name}/tree",
    "/api/v1/repos/{name}/file",
    "/api/v1/repos/{name}/commits",
    "/api/v1/repos/{name}/oplog",
    "/api/v1/repos/{name}/submit",
    "/api/v1/repos/{name}/issues",
    "/api/v1/repos/{name}/issues/{number}",
    "/api/v1/repos/{name}/issues/{number}/comments",
    "/api/v1/repos/{name}/pulls",
    "/api/v1/repos/{name}/pulls/{number}",
    "/api/v1/repos/{name}/pulls/{number}/comments",
    "/api/v1/repos/{name}/pulls/{number}/reviews",
    "/api/v1/repos/{name}/pulls/{number}/merge",
    "/api/v1/repos/{name}/pulls/{number}/diff",
    "/api/v1/repos/{name}/branches",
    "/api/v1/repos/{name}/actions/runs",
    "/api/v1/repos/{name}/actions/runs/{run_id}",
    "/api/v1/repos/{name}/actions/runs/{run_id}/logs",
    "/api/v1/repos/{name}/actions/config",
    "/api/v1/providers",
    "/api/v1/providers/{provider}",
    "/api/v1/providers/{provider}/connect",
    "/api/v1/compute/providers",
    "/api/v1/compute/providers/{provider}",
    "/api/v1/repos/{name}/commits/{sha}/statuses",
    "/api/v1/repos/{name}/agent-sessions",
    "/api/v1/webhooks/forgejo",
    // Secrets use `{repo}` path param (same routes, alternate template name).
    "/api/v1/orgs/{org}/secrets",
    "/api/v1/orgs/{org}/secrets/{secretName}",
    "/api/v1/repos/{repo}/secrets",
    "/api/v1/repos/{repo}/secrets/{secretName}",
];

#[test]
fn openapi_documents_every_stable_route() {
    let mut missing = Vec::new();
    for path in EXPECTED_PATHS {
        // Paths appear as YAML keys like `  /api/v1/repos:` or `  /api/v1/repos/{name}:`.
        let needle = format!("  {path}:");
        if !OPENAPI.contains(&needle) {
            missing.push(*path);
        }
    }
    assert!(
        missing.is_empty(),
        "docs/openapi.yaml is missing path entries for: {missing:?}\n\
         Add them to the OpenAPI document (and this list if intentional)."
    );
}

#[test]
fn openapi_declares_error_envelope() {
    assert!(
        OPENAPI.contains("error:") && OPENAPI.contains("$ref: \"#/components/schemas/Error\""),
        "OpenAPI must document the {{ \"error\": \"...\" }} envelope"
    );
}

#[test]
fn openapi_is_openapi_3() {
    assert!(
        OPENAPI.starts_with("openapi: 3."),
        "expected OpenAPI 3.x document"
    );
}
