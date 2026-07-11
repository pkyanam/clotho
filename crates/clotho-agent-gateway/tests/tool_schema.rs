use clotho_agent_gateway::mcp::StartActionRunParams;
use serde_json::{json, Value};

#[test]
fn action_start_schema_exposes_the_rest_idempotency_contract() {
    let schema = serde_json::to_value(schemars::schema_for!(StartActionRunParams))
        .expect("serialize start_action_run schema");
    let properties = schema["properties"]
        .as_object()
        .expect("start_action_run object properties");

    assert!(properties.contains_key("repo"));
    assert!(properties.contains_key("workflow"));
    assert!(properties.contains_key("release_version"));
    assert!(properties.contains_key("idempotency_key"));
    assert!(properties["idempotency_key"]["description"]
        .as_str()
        .is_some_and(|description| description.contains("Stable retry key")));

    let required = schema["required"]
        .as_array()
        .expect("start_action_run required fields");
    assert!(required.contains(&Value::String("repo".into())));
    assert!(!required.contains(&Value::String("idempotency_key".into())));

    let params = StartActionRunParams {
        repo: "weave".into(),
        commit_id: None,
        branch: None,
        actor: Some("agent".into()),
        workflow: Some("ci".into()),
        release_version: None,
        idempotency_key: Some("retry-01".into()),
    };
    assert_eq!(
        serde_json::to_value(params).expect("serialize start_action_run arguments"),
        json!({
            "repo": "weave",
            "commit_id": null,
            "branch": null,
            "actor": "agent",
            "workflow": "ci",
            "release_version": null,
            "idempotency_key": "retry-01"
        })
    );
}
