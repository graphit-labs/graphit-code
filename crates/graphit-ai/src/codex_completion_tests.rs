use super::*;
use graphit_core::file_lock::Cancellation;
use std::time::{Duration, Instant};

const START: &[u8] = br#"{"type":"turn.started"}"#;
const UNKNOWN: &[u8] = br#"{"type":"synthetic.unknown","bounded":true}"#;
const THREAD: &[u8] = br#"{"type":"thread.started","thread_id":"synthetic-thread"}"#;
const PROVIDER_ERROR: &[u8] = br#"{"type":"error","message":"synthetic-private-canary"}"#;

fn wire_limits() -> &'static CodexWireLimits {
    &CodexWireLimits {
        frame_bytes: 1024 * 1024,
        depth: 16,
        nodes: 10_000,
        fields: 10_000,
        array_items: 10_000,
        string_bytes: 65_536,
        decoded_string_bytes: 1024 * 1024,
        id_bytes: 1024,
    }
}
fn limits() -> CompletionLimits {
    CompletionLimits {
        max_graphit_attempts: 64,
        max_response_bytes: 65_536,
        max_model_id_bytes: 256,
    }
}
fn original() -> Arc<SnapshotContext> {
    original_until(Instant::now() + Duration::from_secs(30))
}
fn original_until(deadline: Instant) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(deadline, Cancellation::default()))
}
fn message(text: &str, phase: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"type":format!("item.{phase}"),
        "item":{"id":"synthetic-item","type":"agent_message","text":text}}))
    .unwrap()
}
fn usage(values: [i64; 5]) -> Vec<u8> {
    format!("{{\"type\":\"turn.completed\",\"usage\":{{\"input_tokens\":{},\"cached_input_tokens\":{},\"output_tokens\":{},\"reasoning_output_tokens\":{},\"cache_write_input_tokens\":{}}}}}",
        values[0], values[1], values[2], values[3], values[4]).into_bytes()
}
fn complete<'n>(
    run: &mut CodexCompletionRun<'n>,
    text: Option<&str>,
    values: [i64; 5],
) -> CompletionOutcome<'n> {
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    attempt.observe_frame(START).unwrap();
    if let Some(text) = text {
        attempt.observe_frame(&message(text, "completed")).unwrap();
    }
    attempt.observe_frame(&usage(values)).unwrap();
    attempt.finish_at_wire_eof()
}
fn failure(outcome: CompletionOutcome<'_>) -> CompletionFailure {
    match outcome {
        CompletionOutcome::Failed(failure) => failure,
        _ => panic!("expected typed failure"),
    }
}
fn assert_kind(outcome: CompletionOutcome<'_>, expected: CompletionFailureKind) {
    assert_eq!(failure(outcome).kind(), expected);
}
fn unavailable(
    outcome: CompletionOutcome<'_>,
    reason: ModelEvidenceUnavailableReason,
) -> UsageEvidence {
    match outcome {
        CompletionOutcome::EvidenceUnavailable(end) => {
            assert_eq!(end.reason(), Some(reason));
            end.usage()
        }
        _ => panic!("expected missing model evidence"),
    }
}

#[test]
fn m01_last_completed_message_wins_with_each_decoded_candidate_cap() {
    let native = original();
    let mut policy = limits();
    policy.max_response_bytes = 4;
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), policy).unwrap();
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    attempt.observe_frame(START).unwrap();
    attempt
        .observe_frame(&message("AAAA", "completed"))
        .unwrap();
    attempt
        .observe_frame(&message("ignored-start", "started"))
        .unwrap();
    attempt
        .observe_frame(&message("ignored-update", "updated"))
        .unwrap();
    attempt
        .observe_frame(&message("BBBB", "completed"))
        .unwrap();
    attempt.observe_frame(&usage([8, 5, 7, 3, 2])).unwrap();
    match attempt.finish_at_wire_eof() {
        CompletionOutcome::Answer(answer) => {
            assert_eq!(answer.text(), "BBBB");
            assert_eq!(answer.end().requested_model(), Some("synthetic-model"));
            assert_eq!(answer.end().ordinal(), 1);
            assert_eq!(answer.end().purpose(), AttemptPurpose::Request);
            assert_eq!(answer.end().reason(), None);
            assert_eq!(answer.end().native_rpc_calls(), NativeRpcCalls::Unobserved);
        }
        _ => panic!("completed answer expected"),
    }
    let mut attempt = run.begin_attempt(AttemptPurpose::Retry, None).unwrap();
    attempt.observe_frame(START).unwrap();
    assert_eq!(
        attempt
            .observe_frame(&message("AAAAA", "completed"))
            .err()
            .unwrap()
            .kind(),
        CompletionFailureKind::ResponseLimit
    );
    assert_eq!(
        attempt
            .observe_frame(&message("BBBB", "completed"))
            .err()
            .unwrap()
            .kind(),
        CompletionFailureKind::ResponseLimit
    );
    let error = failure(attempt.finish_at_wire_eof());
    assert_eq!(error.ordinal(), Some(2));
    assert_eq!(error.purpose(), Some(AttemptPurpose::Retry));
    assert_eq!(run.totals().observed_attempts, 1);
    assert_eq!(run.totals().unavailable_attempts, 1);
}

#[test]
fn m02_empty_is_present_missing_turn_or_message_never_invents_answer() {
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    match complete(&mut run, Some(""), [0; 5]) {
        CompletionOutcome::Answer(answer) => assert_eq!(answer.text(), ""),
        _ => panic!("empty completed message is present"),
    }
    assert!(matches!(
        unavailable(
            complete(&mut run, None, [0; 5]),
            ModelEvidenceUnavailableReason::NoCompletedModelMessage
        ),
        UsageEvidence::Observed(_)
    ));
    let mut attempt = run.begin_attempt(AttemptPurpose::Synthesis, None).unwrap();
    attempt.observe_frame(START).unwrap();
    attempt
        .observe_frame(&message("partial", "completed"))
        .unwrap();
    assert_eq!(
        unavailable(
            attempt.finish_at_wire_eof(),
            ModelEvidenceUnavailableReason::NoCompletedTurn
        ),
        UsageEvidence::Unavailable(UsageUnavailableReason::NoNativeUsage)
    );
    assert_eq!(run.totals().observed_attempts, 2);
    assert_eq!(run.totals().unavailable_attempts, 1);
}

#[test]
fn m02_reasoning_tools_and_item_error_are_no_completed_model_message() {
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    attempt.observe_frame(START).unwrap();
    for frame in [
        br#"{"type":"item.completed","item":{"id":"synthetic-r","type":"reasoning","text":"synthetic-private-canary"}}"#.as_slice(),
        br#"{"type":"item.completed","item":{"id":"synthetic-c","type":"command_execution","command":"synthetic-command","aggregated_output":"synthetic-private-canary","exit_code":0,"status":"completed"}}"#.as_slice(),
        br#"{"type":"item.completed","item":{"id":"synthetic-e","type":"error","message":"synthetic-private-canary"}}"#.as_slice(),
    ] { attempt.observe_frame(frame).unwrap(); }
    attempt.observe_frame(&usage([0; 5])).unwrap();
    assert!(matches!(
        unavailable(
            attempt.finish_at_wire_eof(),
            ModelEvidenceUnavailableReason::NoCompletedModelMessage
        ),
        UsageEvidence::Observed(_)
    ));
}

#[test]
fn m03_strict_sequence_and_provider_errors_survive_completed_turn() {
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    let completed = usage([0; 5]);
    let item = message("bounded", "completed");
    for frames in [
        vec![THREAD, THREAD],
        vec![START, START],
        vec![item.as_slice()],
        vec![completed.as_slice()],
        vec![START, THREAD],
        vec![START, completed.as_slice(), completed.as_slice()],
        vec![START, completed.as_slice(), item.as_slice()],
    ] {
        let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
        for frame in frames {
            let _ = attempt.observe_frame(frame);
        }
        assert_kind(attempt.finish_at_wire_eof(), CompletionFailureKind::Phase);
    }
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    for frame in [
        UNKNOWN,
        THREAD,
        START,
        item.as_slice(),
        completed.as_slice(),
        UNKNOWN,
    ] {
        attempt.observe_frame(frame).unwrap();
    }
    // Completed turn is insufficient before actual EOF: subsequent top-level error wins.
    assert_eq!(
        attempt.observe_frame(PROVIDER_ERROR).err().unwrap().kind(),
        CompletionFailureKind::Provider
    );
    let failure = failure(attempt.finish_at_wire_eof());
    assert!(matches!(failure.usage(), UsageEvidence::Observed(_)));
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    attempt.observe_frame(START).unwrap();
    let _ = attempt
        .observe_frame(br#"{"type":"turn.failed","error":{"message":"synthetic-private-canary"}}"#);
    assert_kind(
        attempt.finish_at_wire_eof(),
        CompletionFailureKind::Provider,
    );
}

#[test]
fn m04_each_negative_native_metric_is_invalid_and_cannot_use_estimate() {
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    for index in 0..5 {
        let mut values = [0; 5];
        values[index] = -1;
        let mut attempt = run
            .begin_attempt(
                AttemptPurpose::Request,
                Some(KnownTextEstimate {
                    input_text_bytes: 8,
                    output_text_bytes: 8,
                }),
            )
            .unwrap();
        attempt.observe_frame(START).unwrap();
        assert_eq!(
            attempt.observe_frame(&usage(values)).err().unwrap().kind(),
            CompletionFailureKind::InvalidUsage
        );
        let failure = failure(attempt.finish_at_wire_eof());
        assert_eq!(
            failure.usage(),
            UsageEvidence::Unavailable(UsageUnavailableReason::InvalidNativeUsage)
        );
        assert!(
            matches!(&failure.cause, FailureCause::InvalidUsage { _event } if matches!(_event.view(), CodexEventView::TurnCompleted { usage } if [usage.input_tokens,usage.cached_input_tokens,usage.output_tokens,usage.reasoning_output_tokens,usage.cache_write_input_tokens][index] == -1))
        );
    }
    assert_eq!(run.totals().estimated_attempts, 0);
    assert_eq!(run.totals().unavailable_attempts, 5);
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    attempt.observe_frame(START).unwrap();
    attempt.observe_frame(br#"{"type":"turn.completed","usage":{"input_tokens":0,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0}}"#).unwrap();
    match unavailable(
        attempt.finish_at_wire_eof(),
        ModelEvidenceUnavailableReason::NoCompletedModelMessage,
    ) {
        UsageEvidence::Observed(v) => assert_eq!(v.cache_write_input_tokens, 0),
        _ => panic!("normalized optional cache write"),
    }
}

#[test]
fn m05_five_observed_metrics_accumulate_without_token_quotas() {
    for metric in 0..5 {
        let native = original();
        let mut run =
            CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits())
                .unwrap();
        let mut values = [0; 5];
        values[metric] = 1;
        for _ in 0..2 {
            assert!(matches!(
                complete(&mut run, Some(""), values),
                CompletionOutcome::Answer(_)
            ));
        }
        let totals = run.totals();
        assert_eq!(
            [
                totals.observed_input_tokens,
                totals.observed_cached_input_tokens,
                totals.observed_output_tokens,
                totals.observed_reasoning_output_tokens,
                totals.observed_cache_write_input_tokens
            ][metric],
            2
        );
        assert_eq!(totals.observed_attempts, 2);
        drop(run.begin_attempt(AttemptPurpose::Retry, None).unwrap());
        assert_eq!(run.admitted_attempts(), 3);
        assert_eq!(run.totals().unavailable_attempts, 1);
    }
}

#[test]
fn m05_native_totals_above_u64_keep_exact_observed_usage() {
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    for _ in 0..3 {
        assert!(matches!(
            complete(&mut run, Some(""), [i64::MAX; 5]),
            CompletionOutcome::Answer(_)
        ));
    }
    let exact = (i64::MAX as u128) * 3;
    assert_eq!(run.totals().observed_input_tokens, exact);
    assert_eq!(run.totals().observed_cached_input_tokens, exact);
    assert_eq!(run.totals().observed_output_tokens, exact);
    assert_eq!(run.totals().observed_reasoning_output_tokens, exact);
    assert_eq!(run.totals().observed_cache_write_input_tokens, exact);
    assert!(exact > u128::from(u64::MAX));
    assert_eq!(run.totals().observed_attempts, 3);
}

#[test]
fn m06_mixed_provenance_across_request_retry_synthesis_and_drop() {
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    assert!(matches!(
        complete(&mut run, Some(""), [8, 5, 7, 3, 2]),
        CompletionOutcome::Answer(_)
    ));
    let estimate = KnownTextEstimate {
        input_text_bytes: 7,
        output_text_bytes: 8,
    };
    let attempt = run
        .begin_attempt(AttemptPurpose::Retry, Some(estimate))
        .unwrap();
    assert_eq!(
        unavailable(
            attempt.finish_at_wire_eof(),
            ModelEvidenceUnavailableReason::NoCompletedTurn
        ),
        UsageEvidence::Estimated(estimate)
    );
    let attempt = run.begin_attempt(AttemptPurpose::Synthesis, None).unwrap();
    drop(attempt);
    let mut attempt = run
        .begin_attempt(AttemptPurpose::Retry, Some(estimate))
        .unwrap();
    attempt.observe_frame(START).unwrap();
    attempt.observe_frame(&usage([0; 5])).unwrap();
    drop(attempt);
    let totals = run.totals();
    assert_eq!(
        (
            totals.observed_attempts,
            totals.estimated_attempts,
            totals.unavailable_attempts
        ),
        (2, 1, 1)
    );
    assert_eq!(
        (
            totals.estimated_known_input_tokens,
            totals.estimated_known_output_tokens
        ),
        (1, 2)
    );
    assert_eq!(
        (
            totals.estimated_input_text_bytes,
            totals.estimated_output_text_bytes
        ),
        (7, 8)
    );
    assert_eq!(run.admitted_attempts(), 4);
    assert_eq!(run.native_rpc_calls(), NativeRpcCalls::Unobserved);
}

#[test]
fn m07_named_floor_estimate_exact_bytes_u128_without_quota() {
    for bytes in [0, 3, 4, 7, 8] {
        let estimate = KnownTextEstimate {
            input_text_bytes: bytes,
            output_text_bytes: bytes,
        };
        assert_eq!(estimate.input_tokens(), bytes / 4);
        assert_eq!(estimate.output_tokens(), bytes / 4);
        assert_eq!(estimate.estimator(), Estimator::Utf8KnownTextBytesFloorDiv4);
        assert_eq!(
            estimate.coverage(),
            EstimateCoverage::KnownTextOnlyNativeTokenizerUnknown
        );
    }
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    let estimate = KnownTextEstimate {
        input_text_bytes: u64::MAX,
        output_text_bytes: u64::MAX,
    };
    for _ in 0..3 {
        drop(
            run.begin_attempt(AttemptPurpose::Synthesis, Some(estimate))
                .unwrap(),
        );
    }
    assert_eq!(
        run.totals().estimated_input_text_bytes,
        u128::from(u64::MAX) * 3
    );
    assert_eq!(
        run.totals().estimated_output_text_bytes,
        u128::from(u64::MAX) * 3
    );
    assert_eq!(
        run.totals().estimated_known_input_tokens,
        u128::from(u64::MAX / 4) * 3
    );
    assert_eq!(run.totals().estimated_attempts, 3);
}

#[test]
fn m08_every_admitted_attempt_counts_and_denial_never_refunds() {
    let native = original();
    let mut policy = limits();
    policy.max_graphit_attempts = 3;
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), policy).unwrap();
    drop(run.begin_attempt(AttemptPurpose::Request, None).unwrap());
    let mut attempt = run.begin_attempt(AttemptPurpose::Retry, None).unwrap();
    let _ = attempt.observe_frame(PROVIDER_ERROR);
    assert_kind(
        attempt.finish_at_wire_eof(),
        CompletionFailureKind::Provider,
    );
    drop(run.begin_attempt(AttemptPurpose::Synthesis, None).unwrap());
    assert_eq!(
        run.begin_attempt(AttemptPurpose::Retry, None)
            .err()
            .unwrap()
            .kind(),
        CompletionFailureKind::MaxGraphitAttempts
    );
    assert_eq!(run.admitted_attempts(), 3);
    assert_eq!(run.totals().unavailable_attempts, 3);
}

#[test]
fn m09_original_cancellation_at_begin_frame_eof_and_first_cause() {
    for stage in 0..3 {
        let native = original();
        let mut run =
            CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits())
                .unwrap();
        if stage == 0 {
            native.cancellation().cancel();
            assert_eq!(
                run.begin_attempt(AttemptPurpose::Request, None)
                    .err()
                    .unwrap()
                    .io_kind(),
                Some(io::ErrorKind::Interrupted)
            );
            assert_eq!(run.admitted_attempts(), 0);
        } else {
            let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
            if stage == 2 {
                attempt.observe_frame(START).unwrap();
                attempt
                    .observe_frame(&message("bounded", "completed"))
                    .unwrap();
                attempt.observe_frame(&usage([0; 5])).unwrap();
            }
            native.cancellation().cancel();
            if stage == 1 {
                assert_eq!(
                    attempt.observe_frame(START).err().unwrap().io_kind(),
                    Some(io::ErrorKind::Interrupted)
                );
            }
            let failure = failure(attempt.finish_at_wire_eof());
            assert_eq!(failure.io_kind(), Some(io::ErrorKind::Interrupted));
            assert_eq!(run.admitted_attempts(), 1);
            assert_eq!(run.totals().observed_attempts, u64::from(stage == 2));
        }
    }
    for provider_first in [true, false] {
        let native = original();
        let mut run =
            CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits())
                .unwrap();
        let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
        if provider_first {
            let _ = attempt.observe_frame(PROVIDER_ERROR);
        }
        native.cancellation().cancel();
        let _ = attempt.observe_frame(PROVIDER_ERROR);
        assert_kind(
            attempt.finish_at_wire_eof(),
            if provider_first {
                CompletionFailureKind::Provider
            } else {
                CompletionFailureKind::NativeIo
            },
        );
    }
}

#[test]
fn m09_original_deadline_never_refreshes_and_usage_preserves_first_provider() {
    for stage in 0..3 {
        let deadline = Instant::now() + Duration::from_millis(100);
        let native = original_until(deadline);
        let mut run =
            CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits())
                .unwrap();
        if stage == 0 {
            std::thread::sleep(
                deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
            assert_eq!(
                run.begin_attempt(AttemptPurpose::Request, None)
                    .err()
                    .unwrap()
                    .io_kind(),
                Some(io::ErrorKind::TimedOut)
            );
        } else {
            let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
            std::thread::sleep(
                deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
            if stage == 1 {
                assert_eq!(
                    attempt.observe_frame(START).err().unwrap().io_kind(),
                    Some(io::ErrorKind::TimedOut)
                );
            }
            assert_eq!(
                failure(attempt.finish_at_wire_eof()).io_kind(),
                Some(io::ErrorKind::TimedOut)
            );
        }
    }
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    attempt.observe_frame(START).unwrap();
    attempt.observe_frame(&usage([2, 0, 0, 0, 0])).unwrap();
    let _ = attempt.observe_frame(PROVIDER_ERROR);
    assert_kind(
        attempt.finish_at_wire_eof(),
        CompletionFailureKind::Provider,
    );
    assert_eq!(run.totals().observed_input_tokens, 2);
    drop(run.begin_attempt(AttemptPurpose::Retry, None).unwrap());
    assert_eq!(run.admitted_attempts(), 2);
    assert_eq!(run.totals().unavailable_attempts, 1);
}

#[test]
fn m11_native_source_chain_retains_actual_error_without_payload_display() {
    let error = io::Error::new(io::ErrorKind::PermissionDenied, "synthetic-private-canary");
    let failure = CompletionFailure::negative(FailureCause::Native(NativeCause(error)));
    assert_eq!(failure.io_kind(), Some(io::ErrorKind::PermissionDenied));
    let mut source: &(dyn Error + 'static) = &failure;
    loop {
        assert!(!format!("{source:?} {source}").contains("synthetic-private-canary"));
        match source.source() {
            Some(next) => source = next,
            None => break,
        }
    }
    assert!(
        matches!(&failure.cause, FailureCause::Native(raw) if raw.0.to_string() == "synthetic-private-canary")
    );
}

#[test]
fn m12_three_limits_positive_model_header_and_ignored_frame_observations() {
    let native = original();
    for index in 0..3 {
        let mut policy = limits();
        match index {
            0 => policy.max_graphit_attempts = 0,
            1 => policy.max_response_bytes = 0,
            2 => policy.max_model_id_bytes = 0,
            _ => unreachable!(),
        }
        assert_eq!(
            CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), policy)
                .err()
                .unwrap()
                .kind(),
            CompletionFailureKind::InvalidLimits
        );
    }
    assert_eq!(
        CodexCompletionRun::new(&native, Some(""), wire_limits(), limits())
            .err()
            .unwrap()
            .kind(),
        CompletionFailureKind::ModelId
    );
    let mut policy = limits();
    policy.max_model_id_bytes = 4;
    assert_eq!(
        CodexCompletionRun::new(&native, Some("abcde"), wire_limits(), policy)
            .err()
            .unwrap()
            .kind(),
        CompletionFailureKind::ModelId
    );
    let mut run = CodexCompletionRun::new(&native, Some("abcd"), wire_limits(), policy).unwrap();
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    for _ in 0..3 {
        attempt.observe_frame(UNKNOWN).unwrap();
    }
    assert_eq!(attempt.frames, 3);
    assert!(matches!(
        attempt.finish_at_wire_eof(),
        CompletionOutcome::EvidenceUnavailable(_)
    ));
}

#[test]
fn m12_frame_observation_overflow_keeps_real_first_failure() {
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    attempt.frames = u64::MAX;
    assert_eq!(
        attempt.observe_frame(UNKNOWN).err().unwrap().kind(),
        CompletionFailureKind::ArithmeticOverflow
    );
    assert_eq!(attempt.frames, u64::MAX);
    assert_kind(
        attempt.finish_at_wire_eof(),
        CompletionFailureKind::ArithmeticOverflow,
    );
}

#[test]
fn m12_decoded_utf8_response_cap_is_neither_escape_length_nor_raw_frame_length() {
    let native = original();
    let mut policy = limits();
    policy.max_response_bytes = 4;
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), policy).unwrap();
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    attempt.observe_frame(START).unwrap();
    attempt.observe_frame(br#"{"type":"item.completed","item":{"id":"synthetic-item","type":"agent_message","text":"\u00e9\u00e9"}}"#).unwrap();
    attempt.observe_frame(&usage([0; 5])).unwrap();
    match attempt.finish_at_wire_eof() {
        CompletionOutcome::Answer(answer) => assert_eq!(answer.text(), "éé"),
        _ => panic!("4 decoded UTF8 bytes expected"),
    }
    let mut attempt = run.begin_attempt(AttemptPurpose::Retry, None).unwrap();
    attempt.observe_frame(START).unwrap();
    assert_eq!(
        attempt
            .observe_frame(&message("ééa", "completed"))
            .err()
            .unwrap()
            .kind(),
        CompletionFailureKind::ResponseLimit
    );
    assert_kind(
        attempt.finish_at_wire_eof(),
        CompletionFailureKind::ResponseLimit,
    );
}

#[test]
fn m14_process_exit_is_no_input_and_model_data_has_no_native_rpc_proof() {
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    let attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    assert_eq!(
        unavailable(
            attempt.finish_at_wire_eof(),
            ModelEvidenceUnavailableReason::NoCompletedTurn
        ),
        UsageEvidence::Unavailable(UsageUnavailableReason::NoNativeUsage)
    );
    match complete(&mut run, Some("bounded"), [0; 5]) {
        CompletionOutcome::Answer(answer) => {
            assert!(matches!(
                answer.end().usage(),
                UsageEvidence::Observed(CodexNormalizedUsage {
                    input_tokens: 0,
                    ..
                })
            ));
            assert_eq!(answer.end().native_rpc_calls(), NativeRpcCalls::Unobserved);
        }
        _ => panic!("model data expected"),
    }
    assert_eq!(run.native_rpc_calls(), NativeRpcCalls::Unobserved);
}

#[test]
fn run_borrows_original_answer_does_not_retain_it_and_wire_error_is_real() {
    let native = original();
    let weak = Arc::downgrade(&native);
    let outcome = {
        let mut run =
            CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits())
                .unwrap();
        assert!(Arc::ptr_eq(run.original(), &native));
        complete(&mut run, Some("bounded"), [0; 5])
    };
    assert_eq!(Arc::strong_count(&native), 1);
    assert!(matches!(outcome, CompletionOutcome::Answer(_)));
    drop(outcome);
    drop(native);
    assert!(weak.upgrade().is_none());
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    let err = attempt.observe_frame(b"invalid").unwrap_err();
    assert_eq!(err.kind(), CompletionFailureKind::Wire);
    assert!(matches!(err.cause, FailureCause::Wire(_)));
    assert_eq!(
        attempt.finish_without_wire_eof().1.unwrap().kind(),
        CompletionFailureKind::Wire
    );
}

#[test]
fn invalid_wire_policy_is_rejected_at_run_construction_before_any_attempt() {
    for index in 0..10 {
        let native = original();
        let mut w = *wire_limits();
        match index {
            0 => w.frame_bytes = 0,
            1 => w.depth = 0,
            2 => w.nodes = 0,
            3 => w.fields = 0,
            4 => w.array_items = 0,
            5 => w.string_bytes = 0,
            6 => w.decoded_string_bytes = 0,
            7 => w.id_bytes = 0,
            8 => w.frame_bytes = 8 * 1024 * 1024 + 1,
            9 => w.depth = 129,
            _ => unreachable!(),
        }
        assert_eq!(
            CodexCompletionRun::new(&native, Some("m"), &w, limits())
                .err()
                .unwrap()
                .kind(),
            CompletionFailureKind::Wire
        );
        native.cancellation().cancel();
        assert_eq!(
            CodexCompletionRun::new(&native, Some("m"), &w, limits())
                .err()
                .unwrap()
                .io_kind(),
            Some(io::ErrorKind::Interrupted)
        );
    }
}

#[test]
fn incomplete_attempt_exposes_observed_usage_without_synthetic_failure_or_reaccount() {
    let native = original();
    let mut run =
        CodexCompletionRun::new(&native, Some("synthetic-model"), wire_limits(), limits()).unwrap();
    let mut attempt = run.begin_attempt(AttemptPurpose::Request, None).unwrap();
    attempt.observe_frame(START).unwrap();
    attempt.observe_frame(&usage([8, 2, 3, 1, 9])).unwrap();
    let (evidence, failure) = attempt.finish_without_wire_eof();
    assert!(failure.is_none());
    let UsageEvidence::Observed(value) = evidence else {
        panic!("actual observed usage required")
    };
    assert_eq!(
        (
            value.input_tokens,
            value.cached_input_tokens,
            value.output_tokens,
            value.reasoning_output_tokens,
            value.cache_write_input_tokens
        ),
        (8, 2, 3, 1, 9)
    );
    let totals = run.totals();
    assert_eq!(totals.observed_input_tokens, 8);
    assert_eq!(totals.observed_output_tokens, 3);
    assert_eq!(totals.observed_cache_write_input_tokens, 9);
    assert_eq!(totals.observed_attempts, 1);
}

#[test]
fn requested_model_is_optional_without_inventing_native_model_evidence() {
    let native = original();
    for requested in [None, Some("explicit/native-model")] {
        let mut run = CodexCompletionRun::new(&native, requested, wire_limits(), limits()).unwrap();
        let CompletionOutcome::Answer(answer) =
            complete(&mut run, Some("literal answer"), [1, 0, 1, 0, 0])
        else {
            panic!("complete native turn must yield its actual answer");
        };
        assert_eq!(answer.end().requested_model(), requested);
        assert_eq!(answer.text(), "literal answer");
    }
    assert!(matches!(
        CodexCompletionRun::new(&native, Some(""), wire_limits(), limits()),
        Err(error) if error.kind() == CompletionFailureKind::ModelId
    ));
}
