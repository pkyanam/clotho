//! Compute (CCI) integration tests.
//!
//! The Daytona round-trip is env-gated: it self-skips unless `DAYTONA_API_KEY`
//! is set, so plain `cargo test` and CI stay green without a paid credential
//! (docs/adr/0008). Run it against the real provider with `just test-compute`
//! after filling in `.env`. Registry and stub tests always run.

use std::collections::HashMap;
use std::sync::Arc;

use clotho_compute::{
    BoxStubProvider, ComputeError, ComputeProvider, ComputeSdkBridgeProvider, DaytonaProvider,
    DisabledProvider, JobFile, JobSpec, ProviderRegistry,
};

#[tokio::test]
async fn disabled_provider_fails_cleanly() {
    let provider = DisabledProvider::new("no key in test");
    let err = provider
        .run_job(JobSpec {
            commands: vec!["true".into()],
            ..Default::default()
        })
        .await
        .expect_err("disabled provider must fail");
    assert!(matches!(err, ComputeError::Disabled(_)), "got {err:?}");
    assert!(!provider.descriptor().configured);
}

#[tokio::test]
async fn registry_lists_multiple_providers_with_capabilities() {
    let reg = ProviderRegistry::new(
        vec![
            Arc::new(DisabledProvider::with_id("daytona", "no key")),
            Arc::new(ComputeSdkBridgeProvider::unconfigured()),
            Arc::new(BoxStubProvider::unconfigured()),
        ],
        "daytona",
    );
    let infos = reg.list_infos();
    assert_eq!(infos.len(), 3);
    let ids: Vec<_> = infos.iter().map(|(d, _)| d.id.as_str()).collect();
    assert_eq!(ids, ["daytona", "computesdk", "box"]);

    let box_d = reg.get("box").expect("box registered");
    assert!(box_d.capabilities.persistent_workspaces);
    assert!(box_d.capabilities.ssh);
    assert!(box_d.capabilities.desktop);
    // Box API v1 also supports create → commands → delete as one-shot.
    assert!(box_d.capabilities.one_shot_jobs);

    let daytona = reg.get("daytona").expect("daytona registered");
    assert!(!daytona.configured);

    // Routing with no configured one-shot provider still resolves default.
    let err = reg
        .run_job(JobSpec {
            commands: vec!["true".into()],
            ..Default::default()
        })
        .await
        .expect_err("must fail without credentials");
    assert!(matches!(err, ComputeError::Disabled(_)));
}

#[tokio::test]
async fn daytona_runs_a_real_job_and_reports_exit_and_logs() {
    let Some(provider) = DaytonaProvider::from_env() else {
        eprintln!("skipping daytona test: DAYTONA_API_KEY not set");
        return;
    };

    let d = provider.descriptor();
    assert!(d.configured);
    assert!(d.capabilities.one_shot_jobs);
    assert!(d.capabilities.file_api);

    // Success path: a staged file is present and the command exits 0.
    let mut env = HashMap::new();
    env.insert("CLOTHO_CI".to_string(), "1".to_string());
    let spec = JobSpec {
        label: "clotho-compute-test".into(),
        snapshot: String::new(),
        files: vec![JobFile {
            path: "/tmp/clotho-test/marker.txt".into(),
            content: b"clotho-marker\n".to_vec(),
        }],
        commands: vec![
            "cat /tmp/clotho-test/marker.txt".into(),
            "echo env=$CLOTHO_CI".into(),
        ],
        env,
        timeout_secs: 120,
        provider_id: String::new(),
    };
    let result = provider.run_job(spec).await.expect("daytona job runs");
    assert_eq!(result.provider, "daytona");
    assert_eq!(result.exit_code, 0, "logs:\n{}", result.logs);
    assert!(
        result.logs.contains("clotho-marker"),
        "staged file not read; logs:\n{}",
        result.logs
    );
    assert!(
        result.logs.contains("env=1"),
        "env not passed; logs:\n{}",
        result.logs
    );

    // Failure path: a non-zero exit is reported, not swallowed.
    let fail = JobSpec {
        label: "clotho-compute-test-fail".into(),
        commands: vec!["echo about-to-fail".into(), "exit 7".into()],
        timeout_secs: 120,
        ..Default::default()
    };
    let result = provider.run_job(fail).await.expect("daytona job runs");
    assert_eq!(result.exit_code, 7, "logs:\n{}", result.logs);
}
