use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_hub::native::NativeBinding;
use graphit_runtime::{
    DeliveryChannel, HookEvent, OutputContext, OutputError, OutputLimits, OutputMessage, encode,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn request() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn limits() -> OutputLimits {
    OutputLimits {
        max_context_bytes: 16_384,
        max_output_bytes: 32_768,
    }
}
const BODY: &str = "context \"quoted\" \\\n\t\u{0000} 日本語 ação ${literal}";

#[test]
fn json_context_is_one_valid_object_with_exact_native_fields() {
    let req = request();
    let ctx = OutputContext::new(limits(), &req).unwrap();
    for binding in [
        NativeBinding::Codex,
        NativeBinding::Claude,
        NativeBinding::Qwen,
    ] {
        for (event, name) in [
            (HookEvent::SessionStart, "SessionStart"),
            (HookEvent::UserPromptSubmit, "UserPromptSubmit"),
            (HookEvent::SubagentStart, "SubagentStart"),
            (HookEvent::PostToolUse, "PostToolUse"),
        ] {
            let output = encode(binding, event, OutputMessage::Context(BODY), &ctx).unwrap();
            assert_eq!(output.channel(), DeliveryChannel::CommandStdout);
            assert_eq!(output.binding(), binding);
            assert_eq!(output.event(), event);
            assert_eq!(event.native_name(binding).unwrap(), name);
            assert_eq!(
                serde_json::from_slice::<Value>(output.bytes()).unwrap(),
                json!({
                    "hookSpecificOutput": { "hookEventName": name, "additionalContext": BODY }
                })
            );
        }
    }
    for (event, name) in [
        (HookEvent::SessionStart, "sessionStart"),
        (HookEvent::PostToolUse, "postToolUse"),
    ] {
        let output = encode(
            NativeBinding::Cursor,
            event,
            OutputMessage::Context(BODY),
            &ctx,
        )
        .unwrap();
        assert_eq!(event.native_name(NativeBinding::Cursor).unwrap(), name);
        assert_eq!(
            serde_json::from_slice::<Value>(output.bytes()).unwrap(),
            json!({"additional_context": BODY})
        );
    }
    for event in [HookEvent::PreInvocation, HookEvent::PostInvocation] {
        let output = encode(
            NativeBinding::Antigravity,
            event,
            OutputMessage::Context(BODY),
            &ctx,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(output.bytes()).unwrap(),
            json!({
                "injectSteps": [{"ephemeralMessage": BODY}]
            })
        );
    }
}

#[test]
fn current_kiro_events_preserve_literal_command_stdout() {
    let req = request();
    let ctx = OutputContext::new(limits(), &req).unwrap();
    for (event, name) in [
        (HookEvent::SessionStart, "SessionStart"),
        (HookEvent::UserPromptSubmit, "UserPromptSubmit"),
        (HookEvent::PostToolUse, "PostToolUse"),
    ] {
        let output = encode(
            NativeBinding::KiroV3,
            event,
            OutputMessage::Context(BODY),
            &ctx,
        )
        .unwrap();
        assert_eq!(event.native_name(NativeBinding::KiroV3).unwrap(), name);
        assert_eq!(output.channel(), DeliveryChannel::CommandStdout);
        assert_eq!(output.bytes(), BODY.as_bytes());
    }
    assert!(matches!(
        encode(
            NativeBinding::KiroV3,
            HookEvent::BeforeAgent,
            OutputMessage::Context(BODY),
            &ctx
        ),
        Err(OutputError::UnsupportedBoundary)
    ));
}

#[test]
fn plugin_text_has_an_explicit_callback_channel() {
    let req = request();
    let ctx = OutputContext::new(limits(), &req).unwrap();
    for (event, name, channel) in [
        (
            HookEvent::SystemTransform,
            "context",
            DeliveryChannel::PluginSystem,
        ),
        (
            HookEvent::SessionCompacting,
            "compaction",
            DeliveryChannel::PluginCompaction,
        ),
    ] {
        let output = encode(
            NativeBinding::OpenCode,
            event,
            OutputMessage::Context(BODY),
            &ctx,
        )
        .unwrap();
        assert_eq!(event.native_name(NativeBinding::OpenCode).unwrap(), name);
        assert_eq!(output.bytes(), BODY.as_bytes());
        assert_eq!(output.channel(), channel);
        assert!(encode(NativeBinding::Claude, event, OutputMessage::Quiet, &ctx).is_err());
    }
}

#[test]
fn observation_events_never_silently_discard_context() {
    let req = request();
    let ctx = OutputContext::new(limits(), &req).unwrap();
    let text = encode(
        NativeBinding::Kimi,
        HookEvent::UserPromptSubmit,
        OutputMessage::Context(BODY),
        &ctx,
    )
    .unwrap();
    assert_eq!(text.bytes(), BODY.as_bytes());
    for event in [
        HookEvent::SessionStart,
        HookEvent::SubagentStart,
        HookEvent::PostToolUse,
        HookEvent::Stop,
        HookEvent::SessionEnd,
    ] {
        assert!(matches!(
            encode(
                NativeBinding::Kimi,
                event,
                OutputMessage::Context(BODY),
                &ctx
            ),
            Err(OutputError::ContextUnavailable)
        ));
        assert!(
            encode(NativeBinding::Kimi, event, OutputMessage::Quiet, &ctx)
                .unwrap()
                .bytes()
                .is_empty()
        );
    }
    for event in [HookEvent::PostToolUse, HookEvent::Stop] {
        assert!(matches!(
            encode(
                NativeBinding::Antigravity,
                event,
                OutputMessage::Context(BODY),
                &ctx
            ),
            Err(OutputError::ContextUnavailable)
        ));
    }
    assert_eq!(
        encode(
            NativeBinding::Antigravity,
            HookEvent::PostToolUse,
            OutputMessage::Quiet,
            &ctx
        )
        .unwrap()
        .bytes(),
        b"{}"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(
            encode(
                NativeBinding::Antigravity,
                HookEvent::Stop,
                OutputMessage::Quiet,
                &ctx
            )
            .unwrap()
            .bytes()
        )
        .unwrap(),
        json!({"decision":"stop"})
    );
    for (event, name) in [
        (HookEvent::Stop, "stop"),
        (HookEvent::SessionEnd, "sessionEnd"),
    ] {
        assert_eq!(event.native_name(NativeBinding::Cursor).unwrap(), name);
        assert!(
            encode(NativeBinding::Cursor, event, OutputMessage::Quiet, &ctx)
                .unwrap()
                .bytes()
                .is_empty()
        );
    }
    for (event, name) in [
        (HookEvent::Stop, "session.idle"),
        (HookEvent::SessionEnd, "session.deleted"),
    ] {
        assert_eq!(event.native_name(NativeBinding::OpenCode).unwrap(), name);
        assert!(!event.accepts_context(NativeBinding::OpenCode).unwrap());
        assert!(
            encode(NativeBinding::OpenCode, event, OutputMessage::Quiet, &ctx)
                .unwrap()
                .bytes()
                .is_empty()
        );
        assert!(matches!(
            encode(
                NativeBinding::OpenCode,
                event,
                OutputMessage::Context(BODY),
                &ctx
            ),
            Err(OutputError::ContextUnavailable)
        ));
    }
    for binding in NativeBinding::ALL {
        let result = encode(binding, HookEvent::BeforeAgent, OutputMessage::Quiet, &ctx);
        assert!(matches!(result, Err(OutputError::UnsupportedBoundary)));
    }
}

#[test]
fn byte_limits_cover_each_escaped_payload_without_batch_accounting() {
    let req = request();
    let ctx = OutputContext::new(limits(), &req).unwrap();
    let escaped = encode(
        NativeBinding::Claude,
        HookEvent::SessionStart,
        OutputMessage::Context(BODY),
        &ctx,
    )
    .unwrap();
    let n = escaped.bytes().len();
    for output_limit in [n, n - 1] {
        let c = OutputContext::new(
            OutputLimits {
                max_context_bytes: BODY.len(),
                max_output_bytes: output_limit,
            },
            &req,
        )
        .unwrap();
        for _ in 0..2 {
            let result = encode(
                NativeBinding::Claude,
                HookEvent::SessionStart,
                OutputMessage::Context(BODY),
                &c,
            );
            if output_limit == n {
                assert_eq!(result.unwrap().bytes(), escaped.bytes());
            } else {
                assert!(matches!(result, Err(OutputError::LimitExceeded)));
            }
        }
        assert!(
            encode(
                NativeBinding::Claude,
                HookEvent::Stop,
                OutputMessage::Quiet,
                &c
            )
            .unwrap()
            .bytes()
            .is_empty()
        );
    }
    let c = OutputContext::new(
        OutputLimits {
            max_context_bytes: BODY.len() - 1,
            ..limits()
        },
        &req,
    )
    .unwrap();
    assert!(matches!(
        encode(
            NativeBinding::KiroV3,
            HookEvent::SessionStart,
            OutputMessage::Context(BODY),
            &c
        ),
        Err(OutputError::LimitExceeded)
    ));
    let c = OutputContext::new(
        OutputLimits {
            max_context_bytes: 4,
            max_output_bytes: 4,
        },
        &req,
    )
    .unwrap();
    for _ in 0..3 {
        assert_eq!(
            encode(
                NativeBinding::Kimi,
                HookEvent::UserPromptSubmit,
                OutputMessage::Context("á"),
                &c
            )
            .unwrap()
            .bytes(),
            "á".as_bytes()
        );
    }
    assert!(matches!(
        encode(
            NativeBinding::Kimi,
            HookEvent::UserPromptSubmit,
            OutputMessage::Context("oversized"),
            &c
        ),
        Err(OutputError::LimitExceeded)
    ));
    assert_eq!(
        encode(
            NativeBinding::Kimi,
            HookEvent::UserPromptSubmit,
            OutputMessage::Context("ok"),
            &c
        )
        .unwrap()
        .bytes(),
        b"ok"
    );
}

#[test]
fn failure_returns_no_payload_and_never_replaces_original_request() {
    let req = request();
    let c = OutputContext::new(limits(), &req).unwrap();
    assert!(matches!(
        encode(
            NativeBinding::Kimi,
            HookEvent::UserPromptSubmit,
            OutputMessage::Context(" \t\n"),
            &c
        ),
        Err(OutputError::EmptyContext)
    ));

    req.cancellation().cancel();
    for message in [OutputMessage::Context("body"), OutputMessage::Quiet] {
        assert!(matches!(
            encode(
                NativeBinding::Kimi,
                HookEvent::UserPromptSubmit,
                message,
                &c
            ),
            Err(OutputError::Cancelled)
        ));
    }

    assert!(matches!(
        OutputContext::new(limits(), &req),
        Err(OutputError::Cancelled)
    ));
    let elapsed = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert!(matches!(
        OutputContext::new(limits(), &elapsed),
        Err(OutputError::DeadlineExceeded)
    ));
    for invalid in [
        OutputLimits {
            max_context_bytes: 0,
            ..limits()
        },
        OutputLimits {
            max_output_bytes: 0,
            ..limits()
        },
    ] {
        assert!(matches!(
            OutputContext::new(invalid, &request()),
            Err(OutputError::InvalidLimits)
        ));
    }
}

#[test]
fn large_text_remains_exact_across_copy_chunks() {
    let req = request();
    let body = "ação\n日本語".repeat(600);
    let c = OutputContext::new(limits(), &req).unwrap();
    let output = encode(
        NativeBinding::Kimi,
        HookEvent::UserPromptSubmit,
        OutputMessage::Context(&body),
        &c,
    )
    .unwrap();
    assert_eq!(output.bytes(), body.as_bytes());
}

#[test]
fn original_deadline_expires_after_context_creation_even_for_quiet_output() {
    let req = SnapshotContext::new(
        Instant::now() + Duration::from_millis(200),
        Cancellation::default(),
    );
    let c = OutputContext::new(limits(), &req).unwrap();
    encode(
        NativeBinding::Kimi,
        HookEvent::UserPromptSubmit,
        OutputMessage::Context("alive"),
        &c,
    )
    .unwrap();
    std::thread::sleep(
        req.deadline()
            .expect("finite fixture deadline")
            .saturating_duration_since(Instant::now())
            + Duration::from_millis(1),
    );
    for message in [OutputMessage::Context("after expiry"), OutputMessage::Quiet] {
        assert!(matches!(
            encode(
                NativeBinding::Kimi,
                HookEvent::UserPromptSubmit,
                message,
                &c
            ),
            Err(OutputError::DeadlineExceeded)
        ));
    }
}

#[test]
fn quiet_json_respects_output_cap_without_context_or_message_quota() {
    let req = request();
    for (binding, event, expected) in [
        (
            NativeBinding::Antigravity,
            HookEvent::PostToolUse,
            b"{}".as_slice(),
        ),
        (
            NativeBinding::Antigravity,
            HookEvent::Stop,
            b"{\"decision\":\"stop\"}".as_slice(),
        ),
    ] {
        for cap in [expected.len(), expected.len() - 1] {
            let context = OutputContext::new(
                OutputLimits {
                    max_context_bytes: 1,
                    max_output_bytes: cap,
                },
                &req,
            )
            .unwrap();
            let result = encode(binding, event, OutputMessage::Quiet, &context);
            if cap == expected.len() {
                assert_eq!(result.unwrap().bytes(), expected);
            } else {
                assert!(matches!(result, Err(OutputError::LimitExceeded)));
            }
        }
    }
}
