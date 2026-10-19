use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{
    knowledge_document_projection::CompilationDigest32,
    knowledge_index::{KnowledgeDocument, KnowledgeRecipeParameters},
};
use graphit_knowledge::published_metadata_lint::{
    PublishedMetadataField as Field, PublishedMetadataFinding as Finding,
    PublishedMetadataLintError as Error, PublishedMetadataLintLimits as Limits,
    PublishedMetadataSeverity as Severity, lint_published_metadata as lint,
};
use std::{
    error::Error as StdError,
    io,
    num::NonZeroUsize,
    time::{Duration, Instant},
};

fn n(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}

fn limits() -> Limits {
    Limits {
        max_pages: n(10),
        max_input_bytes: n(4096),
        max_findings: n(30),
    }
}

fn document(slug: &str) -> KnowledgeDocument {
    KnowledgeDocument {
        slug: slug.into(),
        title: "Policy title".into(),
        body: "# Body\n".into(),
        kind: "domain/a 🍀".into(),
        summary: "preview".into(),
        description: Some("  α\nβ  ".into()),
        summary_truncated: true,
        source_path: "docs/page.md".into(),
        physical_digest: None,
        compilation_digest: CompilationDigest32::from_bytes([17; 32]),
        recipe: "authored-document-v1".into(),
        recipe_parameters: KnowledgeRecipeParameters {
            fallback_title: "Policy title".into(),
            kind_on_absent: "domain/a 🍀".into(),
            max_summary_bytes: n(1),
        },
        updated_at: None,
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: Vec::new(),
    }
}

#[test]
fn integral_description_is_independent_from_editorial_preview() {
    let ctx = context();
    for description in [None, Some(""), Some(" \t\n\u{2003}")] {
        let mut d = document("page");
        d.description = description.map(str::to_owned);
        d.summary = "Derived useful paragraph".into();
        let before = format!("{d:?}");
        let docs = [d];
        let report = lint(&ctx, &docs, limits()).unwrap();
        assert_eq!(report.total_pages, 1);
        assert_eq!(report.required_errors, 0);
        assert_eq!(
            report.findings,
            [Finding {
                slug: "page",
                field: Field::Description,
                severity: Severity::Recommended
            }]
        );
        assert_eq!(report.findings[0].slug.as_ptr(), docs[0].slug.as_ptr());
        assert_eq!(format!("{:?}", docs[0]), before);
    }
    let mut d = document("α");
    // A one-byte editorial capacity can cut a two-byte character to an empty preview.
    d.summary.clear();
    let before = format!("{d:?}");
    let docs = [d];
    let report = lint(&ctx, &docs, limits()).unwrap();
    assert!(report.findings.is_empty());
    assert_eq!(report.required_errors, 0);
    assert_eq!(format!("{:?}", docs[0]), before);
    assert_eq!(docs[0].description.as_deref(), Some("  α\nβ  "));
}

#[test]
fn required_and_recommended_fields_keep_input_order_and_counts() {
    let mut first = document("z");
    first.kind = " \t\u{2003}".into();
    first.title.clear();
    first.description = None;
    let mut second = document("a");
    second.title = "\u{a0}\n".into();
    second.description = Some("\u{2003}".into());
    let docs = [first, second];
    let report = lint(&context(), &docs, limits()).unwrap();
    assert_eq!((report.total_pages, report.required_errors), (2, 1));
    assert_eq!(
        report.findings,
        [
            Finding {
                slug: "z",
                field: Field::Kind,
                severity: Severity::Required
            },
            Finding {
                slug: "z",
                field: Field::Title,
                severity: Severity::Recommended
            },
            Finding {
                slug: "z",
                field: Field::Description,
                severity: Severity::Recommended
            },
            Finding {
                slug: "a",
                field: Field::Title,
                severity: Severity::Recommended
            },
            Finding {
                slug: "a",
                field: Field::Description,
                severity: Severity::Recommended
            },
        ]
    );
    assert_eq!(docs[1].kind, "domain/a 🍀");
}

#[test]
fn each_blank_field_is_classified_without_other_quality_errors() {
    for (field, severity, required) in [
        (Field::Kind, Severity::Required, 1),
        (Field::Title, Severity::Recommended, 0),
        (Field::Description, Severity::Recommended, 0),
    ] {
        for blank in ["", " \r\n\t\u{2003}"] {
            let mut d = document("p");
            match field {
                Field::Kind => d.kind = blank.into(),
                Field::Title => d.title = blank.into(),
                Field::Description => d.description = Some(blank.into()),
            }
            let docs = [d];
            let report = lint(&context(), &docs, limits()).unwrap();
            assert_eq!(report.required_errors, required);
            assert_eq!(
                report.findings,
                [Finding {
                    slug: "p",
                    field,
                    severity
                }]
            );
        }
    }
}

fn assert_limit(error: Error, expected: &str) {
    assert!(matches!(error, Error::LimitExceeded(name) if name == expected));
}

#[test]
fn exact_quotas_pass_and_one_below_rejects_without_partial_report() {
    let mut first = document("é");
    first.kind = " ".into();
    first.title.clear();
    first.description = Some("  ".into());
    let mut second = document("z");
    second.kind.clear();
    second.title = "\t".into();
    second.description = None;
    // Uninspected content has its own Data reader budgets, not this quota.
    second.body = "b".repeat(100_000);
    second.summary = "s".repeat(100_000);
    let docs = [first, second];
    let exact = Limits {
        max_pages: n(2),
        max_input_bytes: n(7),
        max_findings: n(6),
    };
    let ctx = context();
    let report = lint(&ctx, &docs, exact).unwrap();
    assert_eq!((report.required_errors, report.findings.len()), (2, 6));
    let mut cap = exact;
    cap.max_pages = n(1);
    assert_limit(lint(&ctx, &docs, cap).unwrap_err(), "pages");
    cap = exact;
    cap.max_input_bytes = n(6);
    assert_limit(lint(&ctx, &docs, cap).unwrap_err(), "input bytes");
    cap = exact;
    cap.max_findings = n(5);
    assert_limit(lint(&ctx, &docs, cap).unwrap_err(), "findings");
    cap.max_findings = n(1);
    assert_limit(lint(&ctx, &docs, cap).unwrap_err(), "findings");
    assert_eq!(docs[1].body.len(), 100_000);
    assert_eq!(docs[1].summary.len(), 100_000);
}

#[test]
fn empty_corpus_is_an_operational_error_not_a_clean_report() {
    let error = lint(&context(), &[], limits()).unwrap_err();
    assert!(matches!(error, Error::NoDocuments));
    assert!(StdError::source(&error).is_none());
    assert!(!error.to_string().is_empty());
}

#[test]
fn original_cancellation_and_deadline_precede_empty_or_invalid_input() {
    let cancel = Cancellation::default();
    let canceled = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
    cancel.cancel();
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    let docs = [document("p")];
    let invalid = Limits {
        max_pages: n(1),
        max_input_bytes: n(1),
        max_findings: n(1),
    };
    for (original, expected) in [
        (&canceled, io::ErrorKind::Interrupted),
        (&expired, io::ErrorKind::TimedOut),
    ] {
        for input in [&[][..], &docs[..]] {
            let error = lint(original, input, invalid).unwrap_err();
            let source = StdError::source(&error)
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap();
            assert_eq!(source.kind(), expected);
            assert!(matches!(error, Error::Request(ref cause) if cause.kind() == expected));
        }
    }
}
