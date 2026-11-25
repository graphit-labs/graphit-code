use graphit_core::{
    file_io::SnapshotContext, file_lock::Cancellation, project_identity::ProjectId,
};
use graphit_records::reference_projection::*;
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};

const PROJECT: &str = "01KSH1CRFFG8Z74B5ZS78WW808";
const OTHER: &str = "01KSH1CRFFG8Z74B5ZS78WW809";

fn original() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}

fn source() -> SourceNamespace<'static> {
    SourceNamespace {
        scope: LogicalScope::Project(PROJECT.parse().unwrap()),
        context: Some("package@version"),
    }
}

fn candidate() -> ReferenceCandidate<'static> {
    ReferenceCandidate {
        target_type: "knowledge",
        id: "unresolved target",
        scope: None,
        scope_id: None,
        context: None,
        relation: "literal custom relation",
        field: Some(""),
    }
}

fn project<'text>(
    source: SourceNamespace<'text>,
    candidate: ReferenceCandidate<'text>,
) -> LogicalReference<'text> {
    let entries = [AuthoredReferenceEntry::Tuple(candidate)];
    let AuthoredReferenceUpdate::Replace(mut rows) = project_authored_references(
        &original(),
        source,
        AuthoredReferencesField::Sequence(&entries),
    )
    .unwrap() else {
        panic!("sequence must replace")
    };
    rows.remove(0)
}

fn error(candidate: ReferenceCandidate<'_>) -> ReferenceProjectionError {
    let entries = [AuthoredReferenceEntry::Tuple(candidate)];
    project_authored_references(
        &original(),
        source(),
        AuthoredReferencesField::Sequence(&entries),
    )
    .unwrap_err()
}

fn io_cause(error: &ReferenceProjectionError) -> &io::Error {
    error.source().unwrap().downcast_ref().unwrap()
}

#[test]
fn omission_clearing_and_invalid_shapes_are_distinct() {
    assert_eq!(
        project_authored_references(&original(), source(), AuthoredReferencesField::Omitted)
            .unwrap(),
        AuthoredReferenceUpdate::Preserve
    );
    assert_eq!(
        project_authored_references(
            &original(),
            source(),
            AuthoredReferencesField::Sequence(&[])
        )
        .unwrap(),
        AuthoredReferenceUpdate::Replace(vec![])
    );
    for field in [
        AuthoredReferencesField::Null,
        AuthoredReferencesField::NonSequence,
    ] {
        assert!(matches!(
            project_authored_references(&original(), source(), field),
            Err(ReferenceProjectionError::InvalidShape)
        ));
    }
    let entries = [
        AuthoredReferenceEntry::Tuple(candidate()),
        AuthoredReferenceEntry::InvalidShape,
    ];
    assert!(matches!(
        project_authored_references(
            &original(),
            source(),
            AuthoredReferencesField::Sequence(&entries)
        ),
        Err(ReferenceProjectionError::InvalidShape)
    ));
}

#[test]
fn all_target_kinds_accept_project_and_user_provenance_and_preserve_literals() {
    for (target_type, kind) in [
        ("task", ReferenceTargetKind::Task),
        ("session", ReferenceTargetKind::Session),
        ("memory", ReferenceTargetKind::Memory),
        ("knowledge", ReferenceTargetKind::Knowledge),
    ] {
        for (scope, scope_id, expected) in [
            (
                "project",
                OTHER,
                LogicalScope::Project(OTHER.parse().unwrap()),
            ),
            (
                "user",
                " opaque SUBJECT ",
                LogicalScope::User(" opaque SUBJECT "),
            ),
        ] {
            let row = project(
                source(),
                ReferenceCandidate {
                    target_type,
                    scope: Some(scope),
                    scope_id: Some(scope_id),
                    id: "  unresolved-ID  ",
                    relation: "  custom relation  ",
                    ..candidate()
                },
            );
            assert_eq!(row.target_kind, kind);
            assert_eq!(row.scope, expected);
            assert_eq!(row.id, "  unresolved-ID  ");
            assert_eq!(row.relation, "  custom relation  ");
            assert_eq!(row.field, Some(""));
            assert_eq!(row.context, None);
        }
    }
    let first = AuthoredReferenceEntry::Tuple(candidate());
    let entries = [
        first,
        AuthoredReferenceEntry::Tuple(ReferenceCandidate {
            id: "second",
            ..candidate()
        }),
        first,
    ];
    let AuthoredReferenceUpdate::Replace(rows) = project_authored_references(
        &original(),
        source(),
        AuthoredReferencesField::Sequence(&entries),
    )
    .unwrap() else {
        unreachable!()
    };
    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec!["unresolved target", "second", "unresolved target"]
    );
    assert_eq!(rows[0], rows[2]);
}

#[test]
fn scope_and_context_defaults_follow_the_exact_namespace() {
    let local = project(source(), candidate());
    assert_eq!(local.scope, source().scope);
    assert_eq!(local.context, source().context);
    let same = project(
        source(),
        ReferenceCandidate {
            scope: Some("project"),
            scope_id: Some(PROJECT),
            ..candidate()
        },
    );
    assert_eq!(same.context, source().context);
    let foreign = project(
        source(),
        ReferenceCandidate {
            scope_id: Some(OTHER),
            ..candidate()
        },
    );
    assert_eq!(foreign.context, None);
    let user = SourceNamespace {
        scope: LogicalScope::User("subject"),
        context: Some("remote@v"),
    };
    assert_eq!(project(user, candidate()).context, user.context);
    assert_eq!(
        project(
            user,
            ReferenceCandidate {
                scope: Some("user"),
                scope_id: Some("subject"),
                ..candidate()
            }
        )
        .context,
        user.context
    );
    assert_eq!(
        project(
            user,
            ReferenceCandidate {
                scope_id: Some("other-subject"),
                ..candidate()
            }
        )
        .context,
        None
    );
    assert_eq!(
        project(
            user,
            ReferenceCandidate {
                scope: Some("project"),
                scope_id: Some(PROJECT),
                ..candidate()
            }
        )
        .context,
        None
    );
    assert!(matches!(
        error(ReferenceCandidate {
            scope: Some("user"),
            ..candidate()
        }),
        ReferenceProjectionError::MissingScopeId
    ));
    let entries = [AuthoredReferenceEntry::Tuple(ReferenceCandidate {
        scope: Some("project"),
        ..candidate()
    })];
    assert!(matches!(
        project_authored_references(
            &original(),
            user,
            AuthoredReferencesField::Sequence(&entries)
        ),
        Err(ReferenceProjectionError::MissingScopeId)
    ));
    assert_eq!(
        project(
            source(),
            ReferenceCandidate {
                scope_id: Some(OTHER),
                context: Some(" opaque namespace "),
                ..candidate()
            }
        )
        .context,
        Some(" opaque namespace ")
    );
    assert!(matches!(
        error(ReferenceCandidate {
            target_type: "memory",
            context: Some("x"),
            ..candidate()
        }),
        ReferenceProjectionError::ContextNotAllowed
    ));
}

#[test]
fn keywords_and_required_text_reject_invalid_values_without_normalization() {
    for target_type in ["", "Task", "wiki", "unknown"] {
        assert!(matches!(
            error(ReferenceCandidate {
                target_type,
                ..candidate()
            }),
            ReferenceProjectionError::UnknownTargetType
        ));
    }
    for scope in ["", "Project", "installation", "unknown"] {
        assert!(matches!(
            error(ReferenceCandidate {
                scope: Some(scope),
                ..candidate()
            }),
            ReferenceProjectionError::UnknownScope
        ));
    }
    assert!(matches!(
        error(ReferenceCandidate {
            id: "",
            ..candidate()
        }),
        ReferenceProjectionError::EmptyTargetId
    ));
    assert!(matches!(
        error(ReferenceCandidate {
            relation: "",
            ..candidate()
        }),
        ReferenceProjectionError::EmptyRelation
    ));
    assert!(matches!(
        error(ReferenceCandidate {
            context: Some(""),
            ..candidate()
        }),
        ReferenceProjectionError::EmptyContext
    ));
}

#[test]
fn project_identity_errors_preserve_core_causes_and_user_namespace_is_opaque() {
    for id in [
        "invalid",
        "00000000000000000000000000",
        "01ksh1crffg8z74b5zs78ww808",
        "",
    ] {
        let error = error(ReferenceCandidate {
            scope_id: Some(id),
            ..candidate()
        });
        assert!(matches!(error, ReferenceProjectionError::ProjectId(_)));
        let core_error = id.parse::<ProjectId>().unwrap_err();
        assert_eq!(io_cause(&error).kind(), core_error.kind());
        assert_eq!(io_cause(&error).to_string(), core_error.to_string());
    }
    assert!(matches!(
        error(ReferenceCandidate {
            scope: Some("user"),
            scope_id: Some(""),
            ..candidate()
        }),
        ReferenceProjectionError::EmptyUserId
    ));
    for invalid_source in [
        SourceNamespace {
            scope: LogicalScope::User(""),
            context: None,
        },
        SourceNamespace {
            context: Some(""),
            ..source()
        },
    ] {
        assert!(
            project_authored_references(
                &original(),
                invalid_source,
                AuthoredReferencesField::Omitted
            )
            .is_err()
        );
    }
    let row = project(
        SourceNamespace {
            scope: LogicalScope::User(" "),
            context: None,
        },
        candidate(),
    );
    assert_eq!(row.scope, LogicalScope::User(" "));
}

#[test]
fn original_request_errors_take_precedence_and_keep_their_cause() {
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancellation);
    for field in [
        AuthoredReferencesField::Omitted,
        AuthoredReferencesField::Null,
        AuthoredReferencesField::NonSequence,
        AuthoredReferencesField::Sequence(&[]),
    ] {
        let error = project_authored_references(
            &cancelled,
            SourceNamespace {
                scope: LogicalScope::User(""),
                context: Some(""),
            },
            field,
        )
        .unwrap_err();
        assert!(matches!(error, ReferenceProjectionError::Request(_)));
        assert_eq!(io_cause(&error).kind(), io::ErrorKind::Interrupted);
    }
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    let error =
        project_authored_references(&expired, source(), AuthoredReferencesField::Sequence(&[]))
            .unwrap_err();
    assert_eq!(io_cause(&error).kind(), io::ErrorKind::TimedOut);
}

#[test]
fn result_outlives_entries_and_original_but_borrows_text_owner() {
    let id = String::from("borrowed target");
    let relation = String::from("borrowed relation");
    let subject = String::from("opaque owner");
    let result = {
        let original = original();
        let entries = vec![AuthoredReferenceEntry::Tuple(ReferenceCandidate {
            id: &id,
            relation: &relation,
            ..candidate()
        })];
        project_authored_references(
            &original,
            SourceNamespace {
                scope: LogicalScope::User(&subject),
                context: None,
            },
            AuthoredReferencesField::Sequence(&entries),
        )
        .unwrap()
    };
    let AuthoredReferenceUpdate::Replace(rows) = result else {
        unreachable!()
    };
    assert_eq!(rows[0].id.as_ptr(), id.as_ptr());
    assert_eq!(rows[0].relation.as_ptr(), relation.as_ptr());
    assert_eq!(rows[0].scope, LogicalScope::User(&subject));
}
