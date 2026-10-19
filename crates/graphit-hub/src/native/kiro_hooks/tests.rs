use super::*;
use crate::native::ContentLimits;
use graphit_core::file_lock::Cancellation;
use serde_json::{Value, json};
use std::time::Instant;

fn limits() -> SharedWriteLimits {
    SharedWriteLimits {
        input_bytes: 8192,
        output_bytes: 8192,
        journal_bytes: 8192,
        nodes: 128,
        depth: 16,
        scalar_bytes: 1024,
        entries: 64,
        hook_tuples: 1,
        references: 1,
        documents: 1,
    }
}
fn request(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 8192,
            max_output_bytes: 8192,
            max_depth: 16,
            max_nodes: 128,
            max_scalar_bytes: 1024,
            max_collection_entries: 64,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
fn input(trigger: KiroHookTrigger) -> KiroHookInput<'static> {
    KiroHookInput {
        identifier: "graphit-start",
        trigger,
        matcher: None,
        action: KiroHookAction::Command("already-rendered-command"),
        timeout: None,
    }
}
fn encoded(plan: &KiroHookPlan, request: &RequestContext<'_>) -> Value {
    let mut doc = NativeDocument::parse(DocumentFormat::Json, b"{}", limits(), request).unwrap();
    {
        let version = plan.version();
        doc.set(&["version"], FieldValue::String(version.into()))
            .unwrap();
    }
    doc.set(
        plan.array_path(),
        FieldValue::Array(vec![plan.tuple().clone()]),
    )
    .unwrap();
    serde_json::from_str(&doc.render().unwrap()).unwrap()
}

#[test]
fn current_trigger_matrix_has_one_native_shape_for_profiles_and_standalone() {
    let cancel = Cancellation::default();
    let req = request(&cancel, Instant::now() + Duration::from_secs(30));
    for (trigger, event) in [
        (KiroHookTrigger::SessionStart, "SessionStart"),
        (KiroHookTrigger::SessionEnd, "SessionEnd"),
        (KiroHookTrigger::UserPromptSubmit, "UserPromptSubmit"),
        (KiroHookTrigger::PreToolUse, "PreToolUse"),
        (KiroHookTrigger::PostToolUse, "PostToolUse"),
        (KiroHookTrigger::PostFileCreate, "PostFileCreate"),
        (KiroHookTrigger::PostFileSave, "PostFileSave"),
        (KiroHookTrigger::PostFileDelete, "PostFileDelete"),
        (KiroHookTrigger::PreTaskExec, "PreTaskExec"),
        (KiroHookTrigger::PostTaskExec, "PostTaskExec"),
        (KiroHookTrigger::Stop, "Stop"),
        (KiroHookTrigger::Manual, "Manual"),
    ] {
        let plan = plan_kiro_hook(NativeBinding::KiroV3, input(trigger), limits(), &req).unwrap();
        assert_eq!(plan.array_path(), &["hooks"]);
        assert_eq!(plan.version(), "v1");
        assert_eq!(
            plan.critical_fields(),
            &["name", "trigger", "action", "matcher"]
        );
        assert_eq!(
            encoded(&plan, &req),
            json!({"version":"v1","hooks":[{
                "name":"graphit-start","trigger":event,
                "action":{"type":"command","command":"already-rendered-command"}
            }]})
        );
    }
}

#[test]
fn borrowed_bodies_and_authored_matchers_remain_literal_data() {
    let cancel = Cancellation::default();
    let req = request(&cancel, Instant::now() + Duration::from_secs(30));
    let body = "quotes \" ' $HOME %PATH% `literal`\nUnicode ação <&> {{ mcp_tool_xpto }} ${var}\n";
    for (binding, action) in [
        (NativeBinding::KiroV3, KiroHookAction::Command(body)),
        (NativeBinding::KiroV3, KiroHookAction::AgentPrompt(body)),
    ] {
        for matcher in ["", "@custom/server", "mcp_custom_tool.*"] {
            let plan = plan_kiro_hook(
                binding,
                KiroHookInput {
                    action,
                    matcher: Some(matcher),
                    ..input(KiroHookTrigger::PreToolUse)
                },
                limits(),
                &req,
            )
            .unwrap();
            let doc = encoded(&plan, &req);
            let tuple = { &doc["hooks"][0] };
            assert_eq!(tuple["matcher"], matcher);
            let actual = match (binding, action) {
                (_, KiroHookAction::Command(_)) => &tuple["action"]["command"],
                (_, KiroHookAction::AgentPrompt(_)) => &tuple["action"]["prompt"],
            };
            assert_eq!(actual, body);
            assert!(!format!("{plan:?}").contains(body));
            assert!(tuple.get("permissions").is_none());
        }
    }
}

#[test]
fn unsupported_bindings_actions_and_matchers_fail_explicitly() {
    let cancel = Cancellation::default();
    let req = request(&cancel, Instant::now() + Duration::from_secs(30));
    for binding in NativeBinding::ALL {
        if !matches!(binding, NativeBinding::KiroV3) {
            assert_eq!(
                plan_kiro_hook(binding, input(KiroHookTrigger::Stop), limits(), &req).unwrap_err(),
                KiroHookError::UnsupportedBinding
            );
        }
    }
    assert_eq!(
        plan_kiro_hook(
            NativeBinding::KiroV3,
            KiroHookInput {
                action: KiroHookAction::AgentPrompt("rendered body"),
                timeout: Some(Duration::ZERO),
                ..input(KiroHookTrigger::Stop)
            },
            limits(),
            &req
        )
        .unwrap_err(),
        KiroHookError::UnsupportedAction
    );
    for trigger in [
        KiroHookTrigger::SessionStart,
        KiroHookTrigger::SessionEnd,
        KiroHookTrigger::UserPromptSubmit,
        KiroHookTrigger::PreTaskExec,
        KiroHookTrigger::PostTaskExec,
        KiroHookTrigger::Stop,
        KiroHookTrigger::Manual,
    ] {
        assert_eq!(
            plan_kiro_hook(
                NativeBinding::KiroV3,
                KiroHookInput {
                    matcher: Some("*"),
                    ..input(trigger)
                },
                limits(),
                &req
            )
            .unwrap_err(),
            KiroHookError::UnsupportedMatcher
        );
    }
    for trigger in [
        KiroHookTrigger::PostFileCreate,
        KiroHookTrigger::PostFileSave,
        KiroHookTrigger::PostFileDelete,
    ] {
        let plan = plan_kiro_hook(
            NativeBinding::KiroV3,
            KiroHookInput {
                matcher: Some("\\.(rs|md)$"),
                ..input(trigger)
            },
            limits(),
            &req,
        )
        .unwrap();
        assert_eq!(encoded(&plan, &req)["hooks"][0]["matcher"], "\\.(rs|md)$");
    }
}

#[test]
fn timeouts_are_explicit_integer_seconds_including_native_zero() {
    let cancel = Cancellation::default();
    let req = request(&cancel, Instant::now() + Duration::from_secs(30));
    for binding in [NativeBinding::KiroV3] {
        for timeout in [None, Some(Duration::ZERO), Some(Duration::from_secs(7))] {
            let plan = plan_kiro_hook(
                binding,
                KiroHookInput {
                    timeout,
                    ..input(KiroHookTrigger::Stop)
                },
                limits(),
                &req,
            )
            .unwrap();
            let doc = encoded(&plan, &req);
            let tuple = { &doc["hooks"][0] };
            assert_eq!(
                tuple.get("timeout").and_then(Value::as_u64),
                timeout.map(|d| d.as_secs())
            );
            assert!(!plan.critical_fields().contains(&"timeout"));
        }
        for timeout in [Duration::from_millis(1500), Duration::from_secs(u64::MAX)] {
            assert_eq!(
                plan_kiro_hook(
                    binding,
                    KiroHookInput {
                        timeout: Some(timeout),
                        ..input(KiroHookTrigger::Stop)
                    },
                    limits(),
                    &req
                )
                .unwrap_err(),
                KiroHookError::InvalidTimeout
            );
        }
    }
}

#[test]
fn invalid_inputs_are_rejected_without_echoing_values() {
    let cancel = Cancellation::default();
    let req = request(&cancel, Instant::now() + Duration::from_secs(30));
    for identifier in [
        "",
        "Upper",
        "-start",
        "end-",
        "two--parts",
        "../path",
        "nul\0value",
    ] {
        let error = plan_kiro_hook(
            NativeBinding::KiroV3,
            KiroHookInput {
                identifier,
                ..input(KiroHookTrigger::Stop)
            },
            limits(),
            &req,
        )
        .unwrap_err();
        assert_eq!(error, KiroHookError::InvalidIdentifier);
        if !identifier.is_empty() {
            assert!(!format!("{error:?} {error}").contains(identifier));
        }
    }
    let numeric = plan_kiro_hook(
        NativeBinding::KiroV3,
        KiroHookInput {
            identifier: "3-start",
            ..input(KiroHookTrigger::Stop)
        },
        limits(),
        &req,
    )
    .unwrap();
    assert_eq!(numeric.identifier(), "3-start");
    for body in ["", " \n\t", "nul\0value"] {
        for action in [
            KiroHookAction::Command(body),
            KiroHookAction::AgentPrompt(body),
        ] {
            assert_eq!(
                plan_kiro_hook(
                    NativeBinding::KiroV3,
                    KiroHookInput {
                        action,
                        ..input(KiroHookTrigger::Stop)
                    },
                    limits(),
                    &req
                )
                .unwrap_err(),
                KiroHookError::InvalidInput
            );
        }
    }
    assert_eq!(
        plan_kiro_hook(
            NativeBinding::KiroV3,
            KiroHookInput {
                matcher: Some("nul\0matcher"),
                ..input(KiroHookTrigger::PreToolUse)
            },
            limits(),
            &req
        )
        .unwrap_err(),
        KiroHookError::InvalidInput
    );
}

#[test]
fn existing_codec_limits_bound_the_generated_document() {
    let cancel = Cancellation::default();
    let req = request(&cancel, Instant::now() + Duration::from_secs(30));
    for body in [" ".repeat(1025), "\0".repeat(1025)] {
        assert_eq!(
            plan_kiro_hook(
                NativeBinding::KiroV3,
                KiroHookInput {
                    action: KiroHookAction::AgentPrompt(&body),
                    ..input(KiroHookTrigger::Stop)
                },
                limits(),
                &req
            )
            .unwrap_err(),
            KiroHookError::Document(DocumentError::LimitExceeded)
        );
    }
    for changed in [
        SharedWriteLimits {
            input_bytes: 30,
            scalar_bytes: 30,
            ..limits()
        },
        SharedWriteLimits {
            scalar_bytes: 10,
            ..limits()
        },
        SharedWriteLimits {
            output_bytes: 32,
            scalar_bytes: 32,
            ..limits()
        },
        SharedWriteLimits {
            depth: 2,
            ..limits()
        },
        SharedWriteLimits {
            nodes: 3,
            entries: 3,
            ..limits()
        },
        SharedWriteLimits {
            entries: 1,
            ..limits()
        },
    ] {
        assert_eq!(
            plan_kiro_hook(
                NativeBinding::KiroV3,
                input(KiroHookTrigger::Stop),
                changed,
                &req
            )
            .unwrap_err(),
            KiroHookError::Document(DocumentError::LimitExceeded)
        );
    }
    assert_eq!(
        plan_kiro_hook(
            NativeBinding::KiroV3,
            input(KiroHookTrigger::Stop),
            SharedWriteLimits {
                output_bytes: 0,
                ..limits()
            },
            &req
        )
        .unwrap_err(),
        KiroHookError::Document(DocumentError::InvalidPolicy)
    );
}

#[test]
fn original_cancellation_and_deadline_are_preserved() {
    let cancel = Cancellation::default();
    let req = request(&cancel, Instant::now() + Duration::from_secs(30));
    cancel.cancel();
    assert_eq!(
        plan_kiro_hook(
            NativeBinding::KiroV3,
            input(KiroHookTrigger::Stop),
            limits(),
            &req
        )
        .unwrap_err(),
        KiroHookError::Document(DocumentError::Interrupted)
    );
    let cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_millis(50);
    let req = request(&cancel, deadline);
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    assert_eq!(
        plan_kiro_hook(
            NativeBinding::KiroV3,
            input(KiroHookTrigger::Stop),
            limits(),
            &req
        )
        .unwrap_err(),
        KiroHookError::Document(DocumentError::TimedOut)
    );
}
