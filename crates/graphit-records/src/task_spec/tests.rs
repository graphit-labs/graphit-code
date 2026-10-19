use super::*;
use graphit_core::file_lock::Cancellation;
use std::time::{Duration, Instant};

fn id(value: u128) -> CheckId {
    CheckId::new(NonZeroU128::new(value).unwrap())
}
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}

fn verification() -> CheckVerification {
    CheckVerification {
        evidence: "proof".into(),
        verified_by: "actor".into(),
        occurred_at: 7,
    }
}
fn check(value: u128, kind: CheckKind, text: &str) -> CheckRecord {
    CheckRecord {
        id: id(value),
        kind,
        text: text.into(),
        status: CheckStatus::Passed,
        verification: Some(verification()),
        supersession: None,
    }
}
fn snapshot() -> TaskSpecification {
    let mut historical = check(3, CheckKind::Acceptance, "retired");
    historical.status = CheckStatus::Superseded;
    historical.supersession = Some(CheckSupersession {
        actor: "old-actor".into(),
        reason: "old-reason".into(),
        occurred_at: 8,
        replacement: None,
    });
    TaskSpecification {
        id: TaskId::new(NonZeroU128::new(42).unwrap()),
        revision: 10,
        title: "old".into(),
        description: "desc".into(),
        task_type: "Task".into(),
        priority: 2,
        schedule: None,
        checks: vec![
            check(1, CheckKind::Acceptance, "accept"),
            check(2, CheckKind::Test, "test"),
            historical,
        ],
    }
}
fn revise<'a>() -> RevisionCommand<'a> {
    RevisionCommand {
        expected_revision: 10,
        title: FieldUpdate::Keep,
        description: FieldUpdate::Keep,
        task_type: FieldUpdate::Keep,
        priority: None,
        schedule: FieldUpdate::Keep,
        add_checks: &[],
        actor: "writer",
        reason: "edit",
        occurred_at: 11,
    }
}
fn supersede<'a>() -> CheckSupersessionCommand<'a> {
    CheckSupersessionCommand {
        expected_revision: 10,
        check_id: id(1),
        actor: "writer",
        reason: "replace",
        occurred_at: 11,
        replacement: Some(ReplacementCheck {
            id: id(4),
            text: "new acceptance",
            kind: None,
        }),
    }
}
fn verify<'a>() -> CheckVerificationCommand<'a> {
    CheckVerificationCommand {
        expected_revision: 10,
        check_id: id(1),
        passed: true,
        evidence: " new proof ",
        actor: " writer ",
        occurred_at: 11,
    }
}

#[test]
fn each_effective_spec_field_resets_active_without_aliasing_before() {
    for field_index in 0..3 {
        let observed = snapshot();
        let original = context();
        let mut command = revise();
        match field_index {
            0 => command.title = FieldUpdate::Set(" new title "),
            1 => command.description = FieldUpdate::Set(" new description "),
            _ => command.task_type = FieldUpdate::Set(" BUG "),
        }
        let delta = plan_revision(&observed, command, &original).unwrap();
        assert!(std::ptr::eq(delta.before(), &observed));
        assert_eq!(delta.before(), &snapshot());
        assert_eq!(delta.after().revision, 11);
        for active in &delta.after().checks[..2] {
            assert_eq!(active.status, CheckStatus::Pending);
            assert!(active.verification.is_none());
            assert!(active.supersession.is_none());
        }
        assert_eq!(delta.after().checks[2], observed.checks[2]);
        for (old, new) in observed.checks.iter().zip(&delta.after().checks) {
            assert_eq!((old.id, old.kind, &old.text), (new.id, new.kind, &new.text));
        }
        assert_eq!(delta.audit().kind, TaskSpecAuditKind::Revision);
        assert_eq!(
            (
                &*delta.audit().actor,
                &*delta.audit().reason,
                delta.audit().occurred_at
            ),
            ("writer", "edit", 11)
        );
        assert_eq!(observed, snapshot());
    }
}

#[test]
fn equal_after_unicode_trim_is_no_change_and_blank_fields_reject() {
    let observed = snapshot();
    let original = context();
    let mut command = revise();
    assert!(matches!(
        plan_revision(&observed, command, &original),
        Err(TaskSpecError::NoChange)
    ));
    command.title = FieldUpdate::Set("\u{2003}old\u{2003}");
    command.description = FieldUpdate::Set(" desc ");
    command.task_type = FieldUpdate::Set(" Task ");
    assert!(matches!(
        plan_revision(&observed, command, &original),
        Err(TaskSpecError::NoChange)
    ));
    for field_index in 0..3 {
        let mut blank = revise();
        match field_index {
            0 => blank.title = FieldUpdate::Set(" \t"),
            1 => blank.description = FieldUpdate::Set(""),
            _ => blank.task_type = FieldUpdate::Set("\u{2003}"),
        }
        assert!(matches!(
            plan_revision(&observed, blank, &original),
            Err(TaskSpecError::InvalidInput(_))
        ));
    }
    command = revise();
    command.task_type = FieldUpdate::Set(" task ");
    let changed = plan_revision(&observed, command, &original).unwrap();
    assert_eq!(changed.after().task_type, "task");
    assert_eq!(changed.after().checks[0].status, CheckStatus::Pending);
    assert_eq!(observed, snapshot());
}

#[test]
fn priority_only_and_add_only_preserve_all_old_verification() {
    let observed = snapshot();
    let original = context();
    let mut command = revise();
    command.priority = Some(4);
    let delta = plan_revision(&observed, command, &original).unwrap();
    assert_eq!(delta.after().priority, 4);
    assert_eq!(delta.after().checks, observed.checks);
    command.priority = Some(2);
    assert!(matches!(
        plan_revision(&observed, command, &original),
        Err(TaskSpecError::NoChange)
    ));
    command.priority = Some(5);
    assert!(matches!(
        plan_revision(&observed, command, &original),
        Err(TaskSpecError::InvalidInput(_))
    ));
    let additions = [NewCheck {
        id: id(4),
        kind: CheckKind::Test,
        text: " extra test ",
    }];
    command = revise();
    command.add_checks = &additions;
    let delta = plan_revision(&observed, command, &original).unwrap();
    assert_eq!(&delta.after().checks[..3], observed.checks);
    assert_eq!(delta.after().checks[3].text, "extra test");
    assert_eq!(delta.after().checks[3].status, CheckStatus::Pending);
    assert!(delta.after().checks[3].verification.is_none());
}

#[test]
fn add_dedup_includes_history_and_keeps_distinct_kinds() {
    let observed = snapshot();
    let original = context();
    let ignored = [
        NewCheck {
            id: id(9),
            kind: CheckKind::Acceptance,
            text: " retired ",
        },
        NewCheck {
            id: id(10),
            kind: CheckKind::Test,
            text: " \t ",
        },
    ];
    let mut command = revise();
    command.add_checks = &ignored;
    assert!(matches!(
        plan_revision(&observed, command, &original),
        Err(TaskSpecError::NoChange)
    ));
    let additions = [
        NewCheck {
            id: id(4),
            kind: CheckKind::Test,
            text: "retired",
        },
        NewCheck {
            id: id(5),
            kind: CheckKind::Test,
            text: " retired ",
        },
    ];
    command.add_checks = &additions;
    let delta = plan_revision(&observed, command, &original).unwrap();
    assert_eq!(delta.after().checks.len(), 4);
    assert_eq!(delta.after().checks[2].status, CheckStatus::Superseded);
    assert_eq!(
        (delta.after().checks[3].id, delta.after().checks[3].kind),
        (id(4), CheckKind::Test)
    );
    let collision = [NewCheck {
        id: id(1),
        kind: CheckKind::Test,
        text: "genuinely new",
    }];
    command.add_checks = &collision;
    assert!(matches!(
        plan_revision(&observed, command, &original),
        Err(TaskSpecError::DuplicateCheckId)
    ));
    let duplicate_ids = [
        NewCheck {
            id: id(4),
            kind: CheckKind::Test,
            text: "first",
        },
        NewCheck {
            id: id(4),
            kind: CheckKind::Test,
            text: "second",
        },
    ];
    command.add_checks = &duplicate_ids;
    assert!(matches!(
        plan_revision(&observed, command, &original),
        Err(TaskSpecError::DuplicateCheckId)
    ));
    // An ignored logical duplicate does not reserve its supplied data ID.
    let ignored_id = [
        NewCheck {
            id: id(4),
            kind: CheckKind::Acceptance,
            text: "retired",
        },
        NewCheck {
            id: id(4),
            kind: CheckKind::Test,
            text: "new",
        },
    ];
    command.add_checks = &ignored_id;
    assert_eq!(
        plan_revision(&observed, command, &original)
            .unwrap()
            .after()
            .checks
            .len(),
        4
    );
}

#[test]
fn supersession_preserves_complete_passed_before_and_after_evidence() {
    let observed = snapshot();
    let original = context();
    let delta = plan_check_supersession(&observed, supersede(), &original).unwrap();
    assert_eq!(delta.before(), &snapshot());
    assert_eq!(delta.after().checks[0].status, CheckStatus::Superseded);
    assert_eq!(
        delta.after().checks[0].verification,
        observed.checks[0].verification
    );
    let metadata = delta.after().checks[0].supersession.as_ref().unwrap();
    assert_eq!(
        (
            &*metadata.actor,
            &*metadata.reason,
            metadata.occurred_at,
            metadata.replacement
        ),
        ("writer", "replace", 11, Some(id(4)))
    );
    assert_eq!(&delta.after().checks[1..3], &observed.checks[1..3]);
    assert_eq!(
        (delta.after().checks[3].kind, delta.after().checks[3].status),
        (CheckKind::Acceptance, CheckStatus::Pending)
    );
    assert_eq!(
        delta.audit().kind,
        TaskSpecAuditKind::CheckSuperseded {
            check_id: id(1),
            replacement: Some(id(4))
        }
    );
    assert_eq!(observed, snapshot());
}

#[test]
fn required_kinds_and_explicit_different_replacement_kind() {
    let mut observed = snapshot();
    let original = context();
    let mut command = supersede();
    command.replacement = None;
    assert!(matches!(
        plan_check_supersession(&observed, command, &original),
        Err(TaskSpecError::MissingRequiredKind)
    ));
    command.check_id = id(2);
    assert!(matches!(
        plan_check_supersession(&observed, command, &original),
        Err(TaskSpecError::MissingRequiredKind)
    ));
    command = supersede();
    command.replacement.as_mut().unwrap().kind = Some(CheckKind::Test);
    assert!(matches!(
        plan_check_supersession(&observed, command, &original),
        Err(TaskSpecError::MissingRequiredKind)
    ));
    observed
        .checks
        .push(check(5, CheckKind::Acceptance, "other acceptance"));
    let delta = plan_check_supersession(&observed, command, &original).unwrap();
    assert_eq!(delta.after().checks.last().unwrap().kind, CheckKind::Test);
    command.replacement = None;
    let delta = plan_check_supersession(&observed, command, &original).unwrap();
    assert_eq!(delta.after().checks.len(), observed.checks.len());
}

#[test]
fn supersession_unknown_inactive_blank_and_historical_collisions_reject() {
    let observed = snapshot();
    let original = context();
    let mut command = supersede();
    command.check_id = id(99);
    assert!(matches!(
        plan_check_supersession(&observed, command, &original),
        Err(TaskSpecError::UnknownCheck)
    ));
    command.check_id = id(3);
    assert!(matches!(
        plan_check_supersession(&observed, command, &original),
        Err(TaskSpecError::InactiveCheck)
    ));
    command = supersede();
    command.reason = " \t ";
    assert!(matches!(
        plan_check_supersession(&observed, command, &original),
        Err(TaskSpecError::InvalidInput(_))
    ));
    for replacement in [
        ReplacementCheck {
            id: id(3),
            text: "fresh",
            kind: None,
        },
        ReplacementCheck {
            id: id(4),
            text: " retired ",
            kind: None,
        },
    ] {
        command = supersede();
        command.replacement = Some(replacement);
        assert!(matches!(
            plan_check_supersession(&observed, command, &original),
            Err(TaskSpecError::DuplicateReplacement)
        ));
    }
    command.replacement = Some(ReplacementCheck {
        id: id(4),
        text: " ",
        kind: None,
    });
    assert!(matches!(
        plan_check_supersession(&observed, command, &original),
        Err(TaskSpecError::InvalidInput(_))
    ));
    assert_eq!(observed, snapshot());
}

#[test]
fn orphan_self_and_cyclic_historical_chains_reject_but_valid_chain_works() {
    let original = context();
    for target in [id(99), id(3)] {
        let mut observed = snapshot();
        observed.checks[2]
            .supersession
            .as_mut()
            .unwrap()
            .replacement = Some(target);
        assert!(matches!(
            plan_check_verification(&observed, verify(), &original),
            Err(TaskSpecError::InvalidInput(_))
        ));
    }
    let mut observed = snapshot();
    observed.checks[2]
        .supersession
        .as_mut()
        .unwrap()
        .replacement = Some(id(4));
    let mut other = check(4, CheckKind::Test, "history");
    other.status = CheckStatus::Superseded;
    other.supersession = Some(CheckSupersession {
        actor: "actor".into(),
        reason: "reason".into(),
        occurred_at: 9,
        replacement: Some(id(3)),
    });
    observed.checks.push(other);
    assert!(matches!(
        plan_check_verification(&observed, verify(), &original),
        Err(TaskSpecError::InvalidInput(_))
    ));
    observed.checks[3]
        .supersession
        .as_mut()
        .unwrap()
        .replacement = Some(id(1));
    let result = plan_check_verification(&observed, verify(), &original).unwrap();
    assert_eq!(&result.after().checks[2..], &observed.checks[2..]);
}

#[test]
fn verification_pass_fail_and_repeated_events_keep_complete_before() {
    let observed = snapshot();
    let original = context();
    for passed in [true, false] {
        let mut command = verify();
        command.passed = passed;
        let delta = plan_check_verification(&observed, command, &original).unwrap();
        assert_eq!(delta.before(), &snapshot());
        assert_eq!(
            delta.after().checks[0].status,
            if passed {
                CheckStatus::Passed
            } else {
                CheckStatus::Failed
            }
        );
        assert_eq!(
            delta.after().checks[0].verification.as_ref().unwrap(),
            &CheckVerification {
                evidence: "new proof".into(),
                verified_by: "writer".into(),
                occurred_at: 11
            }
        );
        assert_eq!(&delta.after().checks[1..], &observed.checks[1..]);
        assert_eq!(
            delta.audit().kind,
            TaskSpecAuditKind::CheckVerified { check_id: id(1) }
        );
        let mut repeated = command;
        repeated.expected_revision = 11;
        assert_eq!(
            plan_check_verification(delta.after(), repeated, &original)
                .unwrap()
                .after()
                .revision,
            12
        );
    }
    assert_eq!(observed, snapshot());
}

#[test]
fn verification_unknown_inactive_blank_and_stale_revision_reject() {
    let observed = snapshot();
    let original = context();
    let mut command = verify();
    command.check_id = id(99);
    assert!(matches!(
        plan_check_verification(&observed, command, &original),
        Err(TaskSpecError::UnknownCheck)
    ));
    command.check_id = id(3);
    assert!(matches!(
        plan_check_verification(&observed, command, &original),
        Err(TaskSpecError::InactiveCheck)
    ));
    command = verify();
    command.evidence = " \t ";
    assert!(matches!(
        plan_check_verification(&observed, command, &original),
        Err(TaskSpecError::InvalidInput(_))
    ));
    command = verify();
    command.actor = " ";
    assert!(matches!(
        plan_check_verification(&observed, command, &original),
        Err(TaskSpecError::InvalidInput(_))
    ));
    command = verify();
    command.expected_revision = 9;
    assert!(matches!(
        plan_check_verification(&observed, command, &original),
        Err(TaskSpecError::ConflictObservation)
    ));
    let mut rev = revise();
    rev.expected_revision = 9;
    rev.priority = Some(3);
    assert!(matches!(
        plan_revision(&observed, rev, &original),
        Err(TaskSpecError::ConflictObservation)
    ));
    let mut sup = supersede();
    sup.expected_revision = 9;
    assert!(matches!(
        plan_check_supersession(&observed, sup, &original),
        Err(TaskSpecError::ConflictObservation)
    ));
}

#[test]
fn malformed_observations_and_revision_overflow_never_mutate() {
    let original = context();
    for variant in 0..6 {
        let mut observed = snapshot();
        match variant {
            0 => observed.revision = 0,
            1 => observed.priority = 5,
            2 => observed.title = " old ".into(),
            3 => observed.checks[0].verification = None,
            4 => observed.checks[1].id = id(1),
            _ => observed.checks[0].status = CheckStatus::Pending,
        }
        assert!(plan_check_verification(&observed, verify(), &original).is_err());
    }
    let mut observed = snapshot();
    observed.revision = u64::MAX;
    let mut command = verify();
    command.expected_revision = u64::MAX;
    assert!(matches!(
        plan_check_verification(&observed, command, &original),
        Err(TaskSpecError::RevisionOverflow)
    ));
    assert_eq!(observed.revision, u64::MAX);
}

#[test]
fn original_cancel_and_deadline_precede_invalid_input() {
    let mut invalid = snapshot();
    invalid.revision = 0;
    let cancelled = context();
    cancelled.cancellation().cancel();
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    for (original, kind) in [
        (&cancelled, io::ErrorKind::Interrupted),
        (&expired, io::ErrorKind::TimedOut),
    ] {
        for result in [
            plan_revision(&invalid, revise(), original),
            plan_check_verification(&invalid, verify(), original),
            plan_check_supersession(&invalid, supersede(), original),
        ] {
            match result {
                Err(TaskSpecError::Request(error)) => assert_eq!(error.kind(), kind),
                other => panic!("{other:?}"),
            }
        }
    }
}

#[test]
fn command_borrows_end_and_owned_after_survives_delta_consumption() {
    let observed = snapshot();
    let original = context();
    let delta = {
        let title = String::from("temporary title");
        let actor = String::from("temporary actor");
        let reason = String::from("temporary reason");
        let mut command = revise();
        command.title = FieldUpdate::Set(&title);
        command.actor = &actor;
        command.reason = &reason;
        plan_revision(&observed, command, &original).unwrap()
    };
    assert_eq!(delta.after().title, "temporary title");
    assert_eq!(delta.audit().actor, "temporary actor");
    assert_eq!(delta.before(), &snapshot());
    let (before, after, audit) = delta.into_parts();
    assert!(std::ptr::eq(before, &observed));
    assert_eq!(audit.actor, "temporary actor");
    assert_eq!(audit.reason, "temporary reason");
    assert_eq!(after.title, "temporary title");
    assert_eq!(observed, snapshot());
}

#[test]
fn long_check_set_is_iterative_and_preserves_history() {
    let mut observed = snapshot();
    for index in 4..104 {
        observed
            .checks
            .push(check(index, CheckKind::Test, &format!("check {index}")));
    }
    let original = context();
    let mut command = revise();
    command.priority = Some(3);
    let after = plan_revision(&observed, command, &original).unwrap();
    assert_eq!(after.after().checks.len(), 103);
    assert_eq!(after.after().checks, observed.checks);
}

#[test]
fn every_command_into_parts_conserves_borrowed_before_owned_after_and_audit() {
    let observed = snapshot();
    let original = context();
    let mut command = revise();
    command.priority = Some(3);
    for delta in [
        plan_revision(&observed, command, &original).unwrap(),
        plan_check_verification(&observed, verify(), &original).unwrap(),
        plan_check_supersession(&observed, supersede(), &original).unwrap(),
    ] {
        let expected_after = delta.after().clone();
        let expected_audit_kind = delta.audit().kind;
        let expected_actor = delta.audit().actor.clone();
        let expected_reason = delta.audit().reason.clone();
        let expected_at = delta.audit().occurred_at;
        let (before, after, audit) = delta.into_parts();
        assert!(std::ptr::eq(before, &observed));
        assert_eq!(before, &snapshot());
        assert_eq!(after, expected_after);
        assert_eq!(audit.kind, expected_audit_kind);
        assert_eq!(audit.actor, expected_actor);
        assert_eq!(audit.reason, expected_reason);
        assert_eq!(audit.occurred_at, expected_at);
    }
    assert_eq!(observed, snapshot());
}

#[test]
fn every_command_reports_revision_overflow_without_mutating_before() {
    let mut observed = snapshot();
    observed.revision = u64::MAX;
    let before = observed.clone();
    let original = context();
    let mut revision = revise();
    revision.expected_revision = u64::MAX;
    revision.priority = Some(3);
    let mut verification = verify();
    verification.expected_revision = u64::MAX;
    let mut supersession = supersede();
    supersession.expected_revision = u64::MAX;
    for result in [
        plan_revision(&observed, revision, &original),
        plan_check_verification(&observed, verification, &original),
        plan_check_supersession(&observed, supersession, &original),
    ] {
        assert!(matches!(result, Err(TaskSpecError::RevisionOverflow)));
        assert_eq!(observed, before);
    }
}

#[test]
fn long_replacement_chains_and_shared_tails_are_validated_iteratively() {
    let mut observed = snapshot();
    for value in 4..2004 {
        let mut retired = check(value, CheckKind::Test, &format!("retired {value}"));
        retired.status = CheckStatus::Superseded;
        retired.supersession = Some(CheckSupersession {
            actor: "historian".into(),
            reason: "replacement".into(),
            occurred_at: value as u64,
            replacement: Some(id(if value == 2003 { 2 } else { value + 1 })),
        });
        observed.checks.push(retired);
    }
    observed.checks[2]
        .supersession
        .as_mut()
        .unwrap()
        .replacement = Some(id(4));
    let before = observed.clone();
    let original = context();
    let delta = plan_check_verification(&observed, verify(), &original).unwrap();
    assert_eq!(delta.after().checks[2..], observed.checks[2..]);
    assert_eq!(observed, before);
    drop(delta);
    observed
        .checks
        .last_mut()
        .unwrap()
        .supersession
        .as_mut()
        .unwrap()
        .replacement = Some(id(4));
    assert!(matches!(
        plan_check_verification(&observed, verify(), &original),
        Err(TaskSpecError::InvalidInput("cyclic replacement"))
    ));
}

fn schedule(start: &str, end: &str) -> TaskSchedule {
    TaskSchedule::new(
        Some(start.parse().unwrap()),
        Some(end.parse().unwrap()),
        None,
    )
    .unwrap()
}

#[test]
fn schedule_only_replacement_and_clear_preserve_complete_checks_and_before() {
    let mut observed = snapshot();
    observed.schedule = Some(schedule("2024-02-28", "2024-03-01"));
    let before = observed.clone();
    let original = context();
    for value in [
        Some(TaskSchedule::new(None, None, Some("2025-01-01".parse().unwrap())).unwrap()),
        None,
    ] {
        let mut command = revise();
        command.schedule = FieldUpdate::Set(value);
        let delta = plan_revision(&observed, command, &original).unwrap();
        assert_eq!(delta.before(), &before);
        assert_eq!(delta.after().schedule, value);
        assert_eq!(delta.after().revision, 11);
        assert_eq!(delta.after().checks, before.checks);
        assert_eq!(delta.audit().kind, TaskSpecAuditKind::Revision);
        assert_eq!(delta.audit().actor, "writer");
        assert_eq!(delta.audit().occurred_at, 11);
        let (borrowed, after, audit) = delta.into_parts();
        assert!(std::ptr::eq(borrowed, &observed));
        assert_eq!(after.schedule, value);
        assert_eq!(audit.reason, "edit");
    }
    assert_eq!(observed, before);
}

#[test]
fn schedule_keep_equal_and_clear_unscheduled_are_no_change() {
    let mut observed = snapshot();
    let original = context();
    let mut command = revise();
    command.schedule = FieldUpdate::Set(None);
    assert!(matches!(
        plan_revision(&observed, command, &original),
        Err(TaskSpecError::NoChange)
    ));
    observed.schedule = Some(schedule("2024-02-29", "2024-02-29"));
    for update in [FieldUpdate::Keep, FieldUpdate::Set(observed.schedule)] {
        command.schedule = update;
        assert!(matches!(
            plan_revision(&observed, command, &original),
            Err(TaskSpecError::NoChange)
        ));
    }
    command.schedule = FieldUpdate::Keep;
    command.priority = Some(3);
    assert_eq!(
        plan_revision(&observed, command, &original)
            .unwrap()
            .after()
            .schedule,
        observed.schedule
    );
}

#[test]
fn normative_revision_and_check_events_conserve_schedule() {
    let mut observed = snapshot();
    observed.schedule = Some(schedule("2024-12-31", "2025-01-01"));
    let before = observed.clone();
    let original = context();
    let mut command = revise();
    command.title = FieldUpdate::Set("new title");
    let delta = plan_revision(&observed, command, &original).unwrap();
    assert_eq!(delta.after().schedule, observed.schedule);
    assert_eq!(delta.after().checks[0].status, CheckStatus::Pending);
    assert_eq!(delta.after().checks[2], observed.checks[2]);
    command.schedule = FieldUpdate::Set(None);
    let cleared = plan_revision(&observed, command, &original).unwrap();
    assert_eq!(cleared.after().schedule, None);
    assert_eq!(cleared.after().checks[0].status, CheckStatus::Pending);
    for delta in [
        plan_check_verification(&observed, verify(), &original).unwrap(),
        plan_check_supersession(&observed, supersede(), &original).unwrap(),
    ] {
        assert_eq!(delta.after().schedule, observed.schedule);
        assert_eq!(delta.before(), &before);
    }
    assert_eq!(observed, before);
}

#[test]
fn schedule_revision_retains_conflict_overflow_and_original_request_errors() {
    let observed = snapshot();
    let before = observed.clone();
    let mut command = revise();
    command.schedule = FieldUpdate::Set(Some(schedule("2024-02-29", "2024-03-01")));
    command.expected_revision = 9;
    assert!(matches!(
        plan_revision(&observed, command, &context()),
        Err(TaskSpecError::ConflictObservation)
    ));
    let mut overflow = observed.clone();
    overflow.revision = u64::MAX;
    command.expected_revision = u64::MAX;
    assert!(matches!(
        plan_revision(&overflow, command, &context()),
        Err(TaskSpecError::RevisionOverflow)
    ));
    let cancelled = context();
    cancelled.cancellation().cancel();
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    for (original, expected) in [
        (&cancelled, io::ErrorKind::Interrupted),
        (&expired, io::ErrorKind::TimedOut),
    ] {
        match plan_revision(&observed, command, original) {
            Err(TaskSpecError::Request(error)) => assert_eq!(error.kind(), expected),
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(observed, before);
    assert_eq!(overflow.revision, u64::MAX);
}
