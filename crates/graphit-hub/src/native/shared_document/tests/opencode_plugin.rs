use super::*;
use crate::native::opencode_plugin::{
    OpenCodePluginInput, OpenCodePluginPlan, PluginPublicationError, publish_opencode_plugin,
    remove_opencode_plugin, render_opencode_plugin,
};
use graphit_core::artifact_store::ArtifactChange;
use std::process::Command;

fn directory_limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(64 * 1024, 16 * 1024, 128, 64 * 1024).unwrap()
}
fn command() -> Command {
    let mut command = Command::new("graphit");
    command.args(["hook", "literal '$`\\\"; $(false), espaços é"]);
    command
}
fn plan(timeout: Duration, request: &RequestContext<'_>) -> OpenCodePluginPlan {
    let command = command();
    render_opencode_plugin(
        OpenCodePluginInput {
            identifier: "graphit-context",
            context: &command,
            compaction: &command,
            idle: Some(&command),
            deleted: Some(&command),
            timeout,
            max_input_bytes: 8192,
            max_output_bytes: 8192,
        },
        request,
    )
    .unwrap()
}
fn publish(f: &Fixture, plan: &OpenCodePluginPlan, request: &RequestContext<'_>) -> ArtifactChange {
    publish_opencode_plugin(&f.core, &f.root, plan, directory_limits(), request, || {
        Ok(())
    })
    .unwrap()
}

#[test]
fn plugin_producer_preserves_configuration_and_uses_the_tested_runtime_template() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let plan = plan(Duration::from_secs(3), &request);
    let prefix = "import { execFile } from 'node:child_process';\nimport { resolve } from 'node:path';\nconst configuration = ";
    let template = include_str!("../../opencode_plugin.mjs");
    let source = std::str::from_utf8(plan.bytes()).unwrap();
    let encoded = source
        .strip_prefix(prefix)
        .unwrap()
        .strip_suffix(template)
        .unwrap()
        .strip_suffix(";\n")
        .unwrap();
    let invocation = serde_json::json!({"program":"graphit", "args":["hook", "literal '$`\\\"; $(false), espaços é"]});
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(encoded).unwrap(),
        serde_json::json!({
            "identifier":"graphit-context", "context":invocation, "compaction":invocation,
            "idle":invocation, "deleted":invocation, "timeout":3000,
            "max_input_bytes":8192, "max_output_bytes":8192
        })
    );
    assert_eq!(
        plan.relative_entry(),
        ".opencode/plugins/graphit-context.js"
    );
    assert_eq!(publish(&f, &plan, &request), ArtifactChange::Created);
    assert_eq!(publish(&f, &plan, &request), ArtifactChange::Unchanged);
    assert_eq!(
        fs::read(f.root.join(plan.relative_entry())).unwrap(),
        plan.bytes()
    );
}

#[test]
fn plugin_producer_rejects_invalid_or_private_command_configuration() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let mut oversized = command();
    oversized.arg("x".repeat(request.content_limits().max_scalar_bytes + 1));
    let mut private_environment = command();
    private_environment.env("PRIVATE", "not-rendered");
    let mut private_cwd = command();
    private_cwd.current_dir("private-location");
    let empty_program = Command::new("");
    for (case, command, expected) in [
        ("oversized", &oversized, io::ErrorKind::FileTooLarge),
        (
            "private_environment",
            &private_environment,
            io::ErrorKind::InvalidInput,
        ),
        ("private_cwd", &private_cwd, io::ErrorKind::InvalidInput),
        ("empty_program", &empty_program, io::ErrorKind::InvalidInput),
    ] {
        let result = render_opencode_plugin(
            OpenCodePluginInput {
                identifier: "graphit-context",
                context: command,
                compaction: command,
                idle: None,
                deleted: None,
                timeout: Duration::from_secs(3),
                max_input_bytes: 8192,
                max_output_bytes: 8192,
            },
            &request,
        );
        assert_eq!(
            result.err().map(|error| error.kind()),
            Some(expected),
            "{case}"
        );
    }
}

#[test]
fn equal_foreign_plugin_is_not_adopted_and_owned_removal_preserves_other_plugins() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let plan = plan(Duration::from_secs(3), &request);
    let destination = f.root.join(plan.relative_entry());
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(&destination, plan.bytes()).unwrap();
    assert!(
        publish_opencode_plugin(
            &f.core,
            &f.root,
            &plan,
            directory_limits(),
            &request,
            || Ok(())
        )
        .is_err()
    );
    assert_eq!(fs::read(&destination).unwrap(), plan.bytes());
    fs::remove_file(&destination).unwrap();
    publish(&f, &plan, &request);
    fs::write(
        destination.parent().unwrap().join("foreign.mjs"),
        "export default async()=>({});",
    )
    .unwrap();
    assert_eq!(
        remove_opencode_plugin(&f.core, plan.identifier(), &request, || Ok(())).unwrap(),
        ArtifactChange::Removed
    );
    assert!(!destination.exists());
    assert!(destination.parent().unwrap().join("foreign.mjs").exists());
}

#[test]
fn plugin_reports_an_effect_before_post_publication_guard_failure_and_recovers() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let plan = plan(Duration::from_secs(3), &request);
    let destination = f.root.join(plan.relative_entry());
    let error = publish_opencode_plugin(
        &f.core,
        &f.root,
        &plan,
        directory_limits(),
        &request,
        || {
            if destination.exists() {
                Err(io::ErrorKind::PermissionDenied.into())
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert!(
        matches!(error, PluginPublicationError::AfterPublication { change:ArtifactChange::Created, cause } if cause.kind()==io::ErrorKind::PermissionDenied)
    );
    assert_eq!(fs::read(&destination).unwrap(), plan.bytes());
    assert_eq!(publish(&f, &plan, &request), ArtifactChange::Unchanged);
    remove_opencode_plugin(&f.core, plan.identifier(), &request, || Ok(())).unwrap();
}
