use super::*;
use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};
use std::time::{Duration, Instant};
fn policy() -> HubResourcePolicy {
    HubResourcePolicy::resolve(&ConfigSnapshot::new(&Brand::new("graphit").unwrap(), []).unwrap())
        .unwrap()
}
#[tokio::test]
async fn causal_io_error_survives_original_cancel_at_completion() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let error = wait(original.snapshot(), async {
        cancel.cancel();
        Err::<(), _>(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "known storage failure",
        ))
    })
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(error.to_string().contains("known storage failure"));
}
#[tokio::test]
async fn successful_late_io_never_becomes_catalog_result() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let error = wait(original.snapshot(), async {
        cancel.cancel();
        Ok::<_, io::Error>("late result")
    })
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
}
#[test]
fn catalog_restart_and_cli_cancellation_survive_nested_io_errors() {
    let nested = io::Error::other(io::Error::other(restart()));
    assert!(is_restart(&nested));
    let result = rmcp::model::CallToolResult::structured_error(
        serde_json::json!({"status":"error","code":"request_cancelled","private":"private-diagnostic-sentinel"}),
    );
    let nested = io::Error::other(io::Error::other(client::delivery_failure(&result, None)));
    assert_eq!(client::cli_exit_code(&nested), Some(130));
    assert!(!nested.to_string().contains("private-diagnostic-sentinel"));
}
#[test]
fn serialized_arguments_share_original_budget_and_invalid_exit() {
    let policy = policy();
    let cancel = Cancellation::default();
    let mut limits = policy.content();
    limits.max_input_bytes = 128;
    limits.max_scalar_bytes = 128;
    let original = graphit_hub::policy::RequestContext::new(
        limits,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let input = client::ProjectsArguments {
        transport: crate::remote_file_install::client::Transport::Standalone,
        provider: "p".repeat(32),
        profile: Some("explicit-profile".into()),
        identity_provider: Some("native-identity".into()),
        id: None,
        name_prefix: Some("n".repeat(32)),
        limit: 1,
        cursor: None,
    };
    input.validate(&policy).unwrap();
    let error = client::prepare_projects(&input, &policy, &original).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(client::cli_exit_code(&error), Some(2));
}

#[test]
fn malformed_cursor_is_an_argument_error_before_remote_selection() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let input = client::ProjectsArguments {
        transport: crate::remote_file_install::client::Transport::Standalone,
        provider: "storage".into(),
        profile: Some("profile".into()),
        identity_provider: None,
        id: None,
        name_prefix: None,
        limit: 1,
        cursor: Some("not a cursor".into()),
    };
    let error = client::prepare_projects(&input, &policy, &original).unwrap_err();
    assert_eq!(client::cli_exit_code(&error), Some(2));
}

#[test]
fn artifact_cursor_binds_current_revision_query_and_last_manifest_only() {
    let policy = policy();
    let scope = binding((
        "provider",
        7_u64,
        "PROJECT",
        "revision-4",
        "skill",
        "release/one",
        1,
    ))
    .unwrap();
    let encoded = artifacts::next_cursor(
        scope.clone(),
        artifacts::Position {
            artifact_type: graphit_hub::ArtifactType::Skill,
            id: "fixture/scoped".into(),
        },
        &policy,
    )
    .unwrap();
    let position = artifacts::cursor(Some(&encoded), Some(&scope), &policy)
        .unwrap()
        .unwrap();
    assert_eq!(position.id, "fixture/scoped");
    let revised = binding((
        "provider",
        7_u64,
        "PROJECT",
        "revision-5",
        "skill",
        "release/one",
        1,
    ))
    .unwrap();
    assert!(is_restart(
        &artifacts::cursor(Some(&encoded), Some(&revised), &policy)
            .err()
            .unwrap()
    ));
    let decoded = String::from_utf8(
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .unwrap(),
    )
    .unwrap();
    assert!(!decoded.contains("revision-4"));
    assert!(!decoded.contains("provider"));
}
#[test]
fn artifact_search_is_strict_literal_input_before_service_selection() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let mut raw = serde_json::json!({"transport":"broker","provider":"p","producer":"01D39ZY06FGSCTVN4T2V9PKHFZ","artifact_type":"ast","id":"fixture/ast","version":"latest","limit":1});
    let input: client::SearchArguments = serde_json::from_value(raw.clone()).unwrap();
    let prepared = client::prepare_search(&input, &policy, &original).unwrap();
    assert_eq!(prepared.name, "graphit_hub_search");
    assert_eq!(prepared.arguments.unwrap()["version"], "latest");
    raw["q"] = "not a literal filter".into();
    assert!(serde_json::from_value::<client::SearchArguments>(raw.clone()).is_err());
    raw.as_object_mut().unwrap().remove("q");
    raw["id"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<client::SearchArguments>(raw).is_err());
    let mut malformed = input;
    malformed.cursor = Some("invalid cursor".into());
    let error = client::prepare_search(&malformed, &policy, &original).unwrap_err();
    assert_eq!(client::cli_exit_code(&error), Some(2));
}
