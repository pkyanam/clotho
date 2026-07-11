use std::any::Any;

const FAIL_ON_SKIP_ENV: &str = "CLOTHO_TEST_FAIL_ON_SKIP";
const KEEP_ON_FAILURE_ENV: &str = "CLOTHO_TEST_KEEP_FIXTURES_ON_FAILURE";

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

pub fn live_env(name: &str, hint: &str) -> Option<String> {
    let value = std::env::var(name).unwrap_or_default();
    if !value.is_empty() {
        return Some(value);
    }

    let message = format!("{name} not set ({hint})");
    if env_truthy(FAIL_ON_SKIP_ENV) {
        panic!("live-test gate refused to skip: {message}");
    }
    eprintln!("skipping: {message}");
    None
}

pub fn keep_fixture_on_failure(failed: bool) -> bool {
    failed && env_truthy(KEEP_ON_FAILURE_ENV)
}

/// Remove only test-owned repositories through the canonical Clotho REST
/// boundary. This intentionally refuses arbitrary names so a cleanup bug can
/// never delete a contributor's repository.
pub async fn cleanup_repo_fixture(
    client: &reqwest::Client,
    gateway_url: &str,
    name: &str,
    failed: bool,
) -> Result<(), String> {
    const TEST_PREFIXES: [&str; 3] = ["stage3-", "stage6-", "stage11-repo-"];
    if !TEST_PREFIXES.iter().any(|prefix| name.starts_with(prefix)) {
        return Err(format!(
            "refusing to clean non-test repository name {name:?}"
        ));
    }
    if keep_fixture_on_failure(failed) {
        eprintln!("preserving failed collaboration fixture {name}");
        return Ok(());
    }

    let response = client
        .delete(format!(
            "{}/api/v1/repos/{name}",
            gateway_url.trim_end_matches('/')
        ))
        .send()
        .await
        .map_err(|error| format!("delete collaboration fixture {name}: {error}"))?;
    let status = response.status();
    if status == reqwest::StatusCode::NO_CONTENT || status == reqwest::StatusCode::NOT_FOUND {
        return Ok(());
    }
    let body = response.text().await.unwrap_or_default();
    Err(format!(
        "delete collaboration fixture {name}: {status}: {body}"
    ))
}

pub fn finish_after_cleanup(outcome: Result<(), Box<dyn Any + Send>>, cleanup: Result<(), String>) {
    match outcome {
        Ok(()) => cleanup.unwrap_or_else(|error| panic!("fixture cleanup failed: {error}")),
        Err(payload) => {
            if let Err(error) = cleanup {
                eprintln!("fixture cleanup also failed after test failure: {error}");
            }
            std::panic::resume_unwind(payload);
        }
    }
}
