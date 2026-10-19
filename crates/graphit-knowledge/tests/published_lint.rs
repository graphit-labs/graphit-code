use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{
    knowledge_document_projection::CompilationDigest32,
    knowledge_index::{KnowledgeDocument, KnowledgeGeneration, KnowledgeRecipeParameters},
};
use graphit_knowledge::{
    document::{DocumentParseError, DocumentParseLimits},
    published_lint::{
        PublishedBrokenLink, PublishedLintError as Error, PublishedLintLimits as Limits,
        PublishedLintPolicy as Policy, PublishedStaleReason as Reason,
        lint_published_snapshot as lint,
    },
    published_metadata_lint::{
        PublishedMetadataField as Field, PublishedMetadataLintError, PublishedMetadataLintLimits,
        PublishedMetadataSeverity as Severity,
    },
    references::{ReferenceError, ReferenceLimits},
};
use std::{
    error::Error as StdError,
    io,
    num::NonZeroUsize,
    time::{Duration, Instant, UNIX_EPOCH},
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
fn generation() -> KnowledgeGeneration {
    KnowledgeGeneration {
        corpus_uri: "observed-test-corpus".into(),
        version: 7,
        physical_branch: Some("test".into()),
        inherited_target: None,
    }
}
fn policy() -> Policy {
    Policy {
        evaluated_at: UNIX_EPOCH + Duration::from_secs(100),
        max_age: None,
    }
}
fn limits() -> Limits {
    Limits {
        max_pages: n(20),
        max_input_bytes: n(100_000),
        max_findings: n(100),
        max_reference_result_bytes: n(4096),
        document: DocumentParseLimits {
            max_document_bytes: n(50_000),
            max_input_bytes: n(50_000),
            max_frontmatter_bytes: n(30_000),
            max_yaml_events: n(1000),
            max_markdown_events: n(1000),
            max_yaml_depth: n(20),
            max_scalar_bytes: n(30_000),
            max_links: n(100),
            max_projection_bytes: n(100_000),
            max_summary_bytes: n(1000),
        },
        references: ReferenceLimits {
            max_identifier_bytes: n(4096),
        },
        metadata: PublishedMetadataLintLimits {
            max_pages: n(20),
            max_input_bytes: n(100_000),
            max_findings: n(100),
        },
    }
}
fn document(slug: &str) -> KnowledgeDocument {
    KnowledgeDocument {
        slug: slug.into(),
        title: "Título".into(),
        body: "one two three four five six seven eight nine ten eleven".into(),
        kind: "domain/custom 🍀".into(),
        summary: "preview".into(),
        description: Some("Descrição".into()),
        summary_truncated: false,
        source_path: "docs/page.md".into(),
        physical_digest: None,
        compilation_digest: CompilationDigest32::from_bytes([17; 32]),
        recipe: "authored-document-v1".into(),
        recipe_parameters: KnowledgeRecipeParameters {
            fallback_title: "fallback".into(),
            kind_on_absent: "domain/custom 🍀".into(),
            max_summary_bytes: n(1),
        },
        updated_at: None,
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: Vec::new(),
    }
}
fn input_bytes(d: &KnowledgeDocument) -> usize {
    d.slug.len()
        + d.title.len()
        + d.kind.len()
        + d.description.as_deref().unwrap_or("").len()
        + d.body.len()
        + d.recipe_parameters.fallback_title.len()
        + d.resolved_wiki_targets
            .iter()
            .map(String::len)
            .sum::<usize>()
}

#[test]
fn complete_corpus_counts_broken_pairs_self_cycles_and_isolation() {
    let mut a = document("a");
    a.resolved_wiki_targets = vec!["b".into(), "missing".into(), "missing".into(), "a".into()];
    let mut b = document("b");
    b.resolved_wiki_targets = vec!["a".into(), "missing".into()];
    let mut z = document("z");
    z.resolved_wiki_targets = vec!["z".into()];
    let docs = [z, b, a];
    let before = docs.clone();
    let observed = generation();
    let report = lint(&context(), &observed, &docs, policy(), limits()).unwrap();
    assert!(std::ptr::eq(report.generation, &observed));
    assert_eq!((report.total_pages, report.issue_count), (3, 3));
    assert_eq!(report.orphans, ["z"]);
    assert_eq!(
        report.broken_links,
        [
            PublishedBrokenLink {
                source: "a".into(),
                target: "missing".into()
            },
            PublishedBrokenLink {
                source: "b".into(),
                target: "missing".into()
            }
        ]
    );
    assert!(report.short_pages.is_empty());
    assert!(report.stale_pages.is_empty());
    assert!(report.metadata.findings.is_empty());
    assert_eq!(docs, before);
}

#[test]
fn literal_body_zero_ten_eleven_and_markup_use_unicode_whitespace() {
    let bodies = [
        "",
        "one\u{2003}two\u{a0}three\tfour\nfive six seven eight nine ten",
        "one two three four five six seven eight nine ten eleven",
        "**bold** [label](https://example.test) <span>html</span> `code`\n```\ninside fence\n```",
    ];
    let mut docs: Vec<_> = bodies
        .iter()
        .enumerate()
        .map(|(i, body)| {
            let mut d = document(&format!("p{i}"));
            d.body = (*body).into();
            d
        })
        .collect();
    // Frontmatter contributes many words, but only the literal body is short.
    let mut yaml = document("yaml");
    yaml.body = format!(
        "\u{feff}---\r\ndescription: '{}'\r\n---\r\nonly body\r\n",
        "metadata ".repeat(200)
    );
    docs.push(yaml);
    let observed = generation();
    let report = lint(&context(), &observed, &docs, policy(), limits()).unwrap();
    assert_eq!(
        report
            .short_pages
            .iter()
            .map(|p| (p.slug, p.word_count))
            .collect::<Vec<_>>(),
        [("p0", 0), ("p1", 10), ("p3", 8), ("yaml", 2)]
    );
    assert_eq!(report.short_pages[0].slug.as_ptr(), docs[0].slug.as_ptr());
}

#[test]
fn metadata_is_effective_integral_and_weak_findings_do_not_increment_issues() {
    let mut z = document("z");
    z.kind = " \u{2003}".into();
    z.title.clear();
    z.description = None;
    z.summary = "Useful summary".repeat(10_000);
    let mut a = document("a");
    a.description = Some("\u{a0}\n".into());
    a.summary.clear();
    z.resolved_wiki_targets = vec!["a".into()];
    a.resolved_wiki_targets = vec!["z".into()];
    let docs = [z, a];
    let before = docs.clone();
    let observed = generation();
    let mut cap = limits();
    cap.max_input_bytes = n(docs.iter().map(input_bytes).sum());
    cap.max_findings = n(4);
    let report = lint(&context(), &observed, &docs, policy(), cap).unwrap();
    assert_eq!(report.issue_count, 1);
    assert_eq!(report.metadata.required_errors, 1);
    assert_eq!(
        report
            .metadata
            .findings
            .iter()
            .map(|f| (f.slug, f.field, f.severity))
            .collect::<Vec<_>>(),
        [
            ("z", Field::Kind, Severity::Required),
            ("z", Field::Title, Severity::Recommended),
            ("z", Field::Description, Severity::Recommended),
            ("a", Field::Description, Severity::Recommended)
        ]
    );
    assert_eq!(docs, before);
    cap.max_findings = n(3);
    assert!(matches!(
        lint(&context(), &observed, &docs, policy(), cap),
        Err(Error::Limit("findings"))
    ));
    for description in [None, Some(""), Some(" \t\u{2003}")] {
        let mut d = document("p");
        d.description = description.map(str::to_owned);
        let docs = [d];
        let report = lint(&context(), &observed, &docs, policy(), limits()).unwrap();
        assert_eq!(report.metadata.findings.len(), 1);
        assert_eq!(report.issue_count, 1); // orphan only
    }
}

#[test]
fn fixed_clock_age_is_strict_and_markers_precede_age_even_when_disabled() {
    let mut exact = document("exact");
    exact.updated_at = Some(UNIX_EPOCH + Duration::from_secs(90));
    let mut old = document("old");
    old.updated_at = Some(UNIX_EPOCH + Duration::from_secs(90) - Duration::from_nanos(1));
    let mut future = document("future");
    future.updated_at = Some(UNIX_EPOCH + Duration::from_secs(101));
    let mut marked = document("marked");
    marked.updated_at = old.updated_at;
    let since = UNIX_EPOCH + Duration::from_secs(200);
    marked.stale_since = Some(since);
    let mut missing = document("missing");
    missing.stale_after = Some(Duration::from_nanos(1));
    let docs = [marked, future, old, exact, missing];
    let observed = generation();
    let enabled = Policy {
        max_age: Some(Duration::from_secs(10)),
        ..policy()
    };
    let report = lint(&context(), &observed, &docs, enabled, limits()).unwrap();
    assert_eq!(
        report
            .stale_pages
            .iter()
            .map(|p| (p.slug, p.reason))
            .collect::<Vec<_>>(),
        [
            ("marked", Reason::Persisted { since }),
            (
                "old",
                Reason::Age {
                    updated_at: docs[2].updated_at.unwrap()
                }
            )
        ]
    );
    let disabled = lint(&context(), &observed, &docs, policy(), limits()).unwrap();
    assert_eq!(disabled.stale_pages.len(), 1);
    assert_eq!(disabled.stale_pages[0].reason, Reason::Persisted { since });
    assert!(matches!(
        lint(
            &context(),
            &observed,
            &docs,
            Policy {
                max_age: Some(Duration::ZERO),
                ..policy()
            },
            limits()
        ),
        Err(Error::InvalidAgePolicy)
    ));
}

#[test]
fn each_owned_capacity_accepts_exact_and_rejects_one_below_without_mutation() {
    let mut a = document("aa");
    a.body = "short".into();
    a.resolved_wiki_targets = vec!["gone".into(), "other".into()];
    let b = document("bb");
    let docs = [a, b];
    let before = docs.clone();
    let observed = generation();
    let gen_before = observed.clone();
    // bb orphan 2 bytes; aa repeated in both missing pairs: (2+4)+(2+5) = 13.
    let mut cap = limits();
    cap.max_pages = n(2);
    cap.max_input_bytes = n(docs.iter().map(input_bytes).sum());
    cap.max_findings = n(4);
    cap.max_reference_result_bytes = n(15);
    let report = lint(&context(), &observed, &docs, policy(), cap).unwrap();
    assert_eq!(report.issue_count, 4);
    assert_eq!(report.orphans, ["bb"]);
    assert_eq!(report.broken_links.len(), 2);
    for (index, name) in ["pages", "input bytes", "findings", "reference result bytes"]
        .into_iter()
        .enumerate()
    {
        let mut low = cap;
        match index {
            0 => low.max_pages = n(1),
            1 => low.max_input_bytes = n(cap.max_input_bytes.get() - 1),
            2 => low.max_findings = n(3),
            3 => low.max_reference_result_bytes = n(14),
            _ => unreachable!(),
        }
        let error = lint(&context(), &observed, &docs, policy(), low).unwrap_err();
        assert!(
            matches!(error, Error::Limit(actual) if actual == name),
            "{error:?}"
        );
        assert_eq!(docs, before);
        assert_eq!(observed, gen_before);
    }
}

#[test]
fn duplicate_and_self_targets_do_not_multiply_broken_diagnostics() {
    let mut d = document("p");
    d.resolved_wiki_targets = vec!["p".into(), "ghost".into(), "ghost".into()];
    let docs = [d];
    let observed = generation();
    assert_eq!(
        lint(&context(), &observed, &docs, policy(), limits())
            .unwrap()
            .broken_links
            .len(),
        1
    );
}

#[test]
fn child_metadata_and_reference_budgets_remain_independent() {
    let mut a = document("aa");
    a.title.clear();
    a.description = None;
    a.resolved_wiki_targets = vec!["bb".into()];
    let mut b = document("bb");
    b.resolved_wiki_targets = vec!["aa".into()];
    let docs = [a, b];
    let observed = generation();
    let before = docs.clone();
    let metadata_bytes = docs
        .iter()
        .map(|d| {
            d.slug.len()
                + d.title.len()
                + d.kind.len()
                + d.description.as_deref().unwrap_or("").len()
        })
        .sum::<usize>();
    let mut exact = limits();
    exact.metadata.max_pages = n(2);
    exact.metadata.max_input_bytes = n(metadata_bytes);
    exact.metadata.max_findings = n(2);
    exact.references.max_identifier_bytes = n(2);
    let report = lint(&context(), &observed, &docs, policy(), exact).unwrap();
    assert_eq!(report.metadata.findings.len(), 2);
    assert_eq!(report.issue_count, 0);
    for boundary in 0..4 {
        let mut low = exact;
        match boundary {
            0 => low.metadata.max_pages = n(1),
            1 => low.metadata.max_input_bytes = n(metadata_bytes - 1),
            2 => low.metadata.max_findings = n(1),
            3 => low.references.max_identifier_bytes = n(1),
            _ => unreachable!(),
        }
        let error = lint(&context(), &observed, &docs, policy(), low).unwrap_err();
        if boundary < 3 {
            assert!(matches!(
                error,
                Error::Metadata(PublishedMetadataLintError::LimitExceeded(_))
            ));
        } else {
            assert!(matches!(
                error,
                Error::References(ReferenceError::LimitExceeded(_))
            ));
        }
        assert_eq!(docs, before);
    }
}

#[test]
fn producer_caps_and_malformed_syntax_preserve_typed_causes() {
    let docs = [document("p")];
    let observed = generation();
    let mut cap = limits();
    cap.document.max_document_bytes = n(docs[0].body.len());
    lint(&context(), &observed, &docs, policy(), cap).unwrap();
    cap.document.max_document_bytes = n(docs[0].body.len() - 1);
    let error = lint(&context(), &observed, &docs, policy(), cap).unwrap_err();
    assert!(matches!(
        error,
        Error::Parse(DocumentParseError::LimitExceeded(_))
    ));
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<DocumentParseError>()
            .is_some()
    );
    let mut cap = limits();
    cap.references.max_identifier_bytes = n(1);
    lint(&context(), &observed, &docs, policy(), cap).unwrap();
    let mut d = document("pp");
    d.description = None;
    let docs = [d];
    let error = lint(&context(), &observed, &docs, policy(), cap).unwrap_err();
    assert!(matches!(
        error,
        Error::References(ReferenceError::LimitExceeded(_))
    ));
    let mut cap = limits();
    cap.metadata.max_input_bytes = n(1);
    let error = lint(&context(), &observed, &docs, policy(), cap).unwrap_err();
    assert!(matches!(
        error,
        Error::Metadata(PublishedMetadataLintError::LimitExceeded(_))
    ));
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<PublishedMetadataLintError>()
            .is_some()
    );
    for body in ["---\ntitle: [broken\n---\nbody", "---\ntitle: ok\n"] {
        let mut d = document("p");
        d.body = body.into();
        let before = d.clone();
        let docs = [d];
        let error = lint(&context(), &observed, &docs, policy(), limits()).unwrap_err();
        assert!(matches!(
            error,
            Error::Parse(DocumentParseError::Yaml { .. } | DocumentParseError::UnclosedFrontmatter)
        ));
        assert!(
            error
                .source()
                .unwrap()
                .downcast_ref::<DocumentParseError>()
                .is_some()
        );
        assert_eq!(docs[0], before);
    }
}

#[test]
fn duplicate_and_invalid_identifiers_are_errors_instead_of_partial_reports() {
    let observed = generation();
    for (docs, duplicate) in [
        (vec![document("p"), document("p")], true),
        (vec![document("")], false),
    ] {
        let before = docs.clone();
        let error = lint(&context(), &observed, &docs, policy(), limits()).unwrap_err();
        assert!(matches!(error, Error::References(ReferenceError::DuplicatePage)) == duplicate);
        if !duplicate {
            assert!(matches!(
                error,
                Error::References(ReferenceError::InvalidIdentifier)
            ));
        }
        assert_eq!(docs, before);
    }
}

#[test]
fn initial_request_errors_precede_empty_policy_and_limits() {
    let observed = generation();
    let cancel = Cancellation::default();
    let canceled = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
    cancel.cancel();
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    let docs = [document("p")];
    for (ctx, expected) in [
        (&canceled, io::ErrorKind::Interrupted),
        (&expired, io::ErrorKind::TimedOut),
    ] {
        for input in [&[][..], &docs[..]] {
            let error = lint(
                ctx,
                &observed,
                input,
                Policy {
                    max_age: Some(Duration::ZERO),
                    ..policy()
                },
                limits(),
            )
            .unwrap_err();
            assert!(matches!(&error, Error::Request(e) if e.kind() == expected));
            assert_eq!(
                error
                    .source()
                    .unwrap()
                    .downcast_ref::<io::Error>()
                    .unwrap()
                    .kind(),
                expected
            );
        }
    }
    assert!(matches!(
        lint(
            &context(),
            &observed,
            &[],
            Policy {
                max_age: Some(Duration::ZERO),
                ..policy()
            },
            limits()
        ),
        Err(Error::NoDocuments)
    ));
}
