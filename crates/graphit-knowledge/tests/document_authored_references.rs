use graphit_core::{
    file_io::SnapshotContext, file_lock::Cancellation, project_identity::ProjectId,
};
use graphit_knowledge::{
    document::{DocumentParseLimits, ParseStats, parse_document},
    document_authored_references::DocumentAuthoredReferenceLimits,
    document_authored_references::{
        DocumentAuthoredReferenceError as E, project_document_authored_references,
    },
};
use graphit_records::reference_projection::{
    AuthoredReferenceUpdate as Update, LogicalScope, ReferenceProjectionError,
    ReferenceTargetKind as Kind, SourceNamespace,
};
use std::{
    error::Error,
    io,
    num::NonZeroUsize,
    time::{Duration, Instant},
};
fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
fn parser() -> DocumentParseLimits {
    DocumentParseLimits {
        max_document_bytes: n(100000),
        max_input_bytes: n(100000),
        max_frontmatter_bytes: n(90000),
        max_yaml_events: n(10000),
        max_markdown_events: n(10000),
        max_yaml_depth: n(100),
        max_scalar_bytes: n(50000),
        max_links: n(100),
        max_projection_bytes: n(100000),
        max_summary_bytes: n(1000),
    }
}
fn limits() -> DocumentAuthoredReferenceLimits {
    DocumentAuthoredReferenceLimits {
        max_results: n(100),
        max_identifier_bytes: n(1000),
    }
}
fn original() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn source() -> SourceNamespace<'static> {
    SourceNamespace {
        scope: LogicalScope::User("owner"),
        context: Some("installed"),
    }
}
fn doc(yaml: &str) -> String {
    format!("---\n{yaml}\n---\n# body\n")
}
fn run_error(yaml: &str) -> E {
    let original = original();
    let text = doc(yaml);
    let parsed = match parse_document(&original, text.as_bytes(), "fallback", parser()) {
        Ok(p) => p,
        Err(e) => return E::Syntax(e),
    };
    project_document_authored_references(&original, &parsed, source(), parser(), limits())
        .unwrap_err()
}
#[test]
fn omitted_empty_and_invalid_shapes_are_distinct() {
    let original = original();
    for text in ["# no frontmatter".into(), doc(""), doc("title: unrelated")] {
        let parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
        assert_eq!(
            project_document_authored_references(&original, &parsed, source(), parser(), limits())
                .unwrap(),
            Update::Preserve
        );
    }
    let text = doc("references: []");
    let parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
    assert_eq!(
        project_document_authored_references(&original, &parsed, source(), parser(), limits())
            .unwrap(),
        Update::Replace(vec![])
    );
    for yaml in [
        "references:",
        "references: null",
        "references: false",
        "references: {}",
        "references: [null]",
        "references: [{relation: x}]",
        "references: [{target: {type: task, id: a}, relation: x, unknown: x}]",
        "references: [{target: {type: task, id: a, unknown: x}, relation: x}]",
        "references: []\nreferences: []",
        "references: [{target: {type: task, id: a, id: b}, relation: x}]",
        "references: [{target: {type: task, id: a}, relation: x, relation: y}]",
    ] {
        assert!(
            matches!(
                run_error(yaml),
                E::Kernel(ReferenceProjectionError::InvalidShape) | E::Syntax(_)
            ),
            "{yaml}"
        );
    }
}
#[test]
fn real_namespaces_four_kinds_literals_order_and_duplicates() {
    let text = doc(
        "references:\n- target: {type: task, id: ' tarefa ' }\n  relation: ' supports '\n  field: 'é'\n- target: {type: session, id: ses-x}\n  relation: supports\n- target: {type: memory, id: mem-x, scope: user, scope_id: 'foreign user'}\n  relation: supports\n- target: {type: knowledge, id: 'Café', scope: project, scope_id: 01ARZ3NDEKTSV4RRFFQ69G5FAV}\n  relation: supports\n- target: {type: knowledge, id: 'Café'}\n  relation: supports\n- target: {type: knowledge, id: 'Café'}\n  relation: supports",
    );
    let original = original();
    let parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
    let Update::Replace(rows) =
        project_document_authored_references(&original, &parsed, source(), parser(), limits())
            .unwrap()
    else {
        panic!()
    };
    assert_eq!(rows.len(), 6);
    assert_eq!(
        rows.iter().map(|r| r.target_kind).collect::<Vec<_>>(),
        [
            Kind::Task,
            Kind::Session,
            Kind::Memory,
            Kind::Knowledge,
            Kind::Knowledge,
            Kind::Knowledge
        ]
    );
    assert_eq!(rows[0].id, " tarefa ");
    assert_eq!(rows[0].relation, " supports ");
    assert_eq!(rows[0].field, Some("é"));
    assert_eq!(rows[2].scope, LogicalScope::User("foreign user"));
    assert_eq!(
        rows[3].scope,
        LogicalScope::Project("01ARZ3NDEKTSV4RRFFQ69G5FAV".parse::<ProjectId>().unwrap())
    );
    assert_eq!(rows[3].context, None);
    assert_eq!(rows[4].context, Some("installed"));
    assert_eq!(rows[4], rows[5]);
    // The text is borrowed from the owner AST, with no output String copy.
    assert!(parsed.metadata.nodes().iter().any(|node| matches!(&node.kind,graphit_knowledge::document::MetadataKind::Scalar {value,..} if value.as_ptr()==rows[0].id.as_ptr())));
}
#[test]
fn mature_yaml_scalar_types_reject_coercion_and_keep_quotes() {
    for scalar in [
        "yes", "on", "true", "false", "123", "0xB", "1.25", "1e999", ".nan", ".inf", "-.inf",
        "null", "~",
    ] {
        for field in ["id", "scope_id", "context"] {
            let yaml = if field == "id" {
                format!(
                    "references: [{{target: {{type: knowledge, id: {scalar}}}, relation: supports}}]"
                )
            } else {
                format!(
                    "references: [{{target: {{type: knowledge, id: x, {field}: {scalar}}}, relation: supports}}]"
                )
            };
            let error = run_error(&yaml);
            assert!(matches!(error, E::SemanticYaml(_)), "{yaml}: {error:?}");
            assert!(error.source().is_some());
        }
        let original = original();
        let text = doc(&format!(
            "references: [{{target: {{type: knowledge, id: '{scalar}'}}, relation: supports}}]"
        ));
        let parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
        let Update::Replace(rows) =
            project_document_authored_references(&original, &parsed, source(), parser(), limits())
                .unwrap()
        else {
            panic!()
        };
        assert_eq!(rows[0].id, scalar);
    }
    for yaml in [
        "references: [{target: {type: task, id: x}, relation: null}]",
        "references: [{target: {type: task, id: x}, relation: supports, field: null}]",
    ] {
        assert!(matches!(run_error(yaml), E::SemanticYaml(_)));
    }
}
#[test]
fn unknown_metadata_preserves_primitive_keys_and_nonfinite_values() {
    let original = original();
    let text = doc(
        "0xB: x\n11: y\ntitle: .nan\nunknown: [.inf, 1e999, {on: true}]\n.nan: ignored\nreferences: [{target: {type: memory, id: x}, relation: supports}]",
    );
    let parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
    let Update::Replace(rows) =
        project_document_authored_references(&original, &parsed, source(), parser(), limits())
            .unwrap()
    else {
        panic!()
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "x");
}
#[test]
fn multiline_escaped_crlf_scalars_borrow_decoded_owner() {
    let original = original();
    for yaml in [
        "references:\n- target:\n    type: knowledge\n    id: |\n      Café\n      guide\n  relation: supports",
        "references: [{target: {type: knowledge, id: \"Café\\nguide\"}, relation: supports}]",
    ] {
        let text = doc(yaml).replace('\n', "\r\n");
        let parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
        let Update::Replace(rows) =
            project_document_authored_references(&original, &parsed, source(), parser(), limits())
                .unwrap()
        else {
            panic!()
        };
        assert!(rows[0].id.starts_with("Café\nguide"));
    }
}
#[test]
fn invalid_last_tuple_never_yields_a_partial_replacement() {
    let yaml = "references: [{target: {type: task, id: x}, relation: supports}, {target: {type: mystery, id: late-target}, relation: supports}]";
    assert!(matches!(
        run_error(yaml),
        E::Kernel(ReferenceProjectionError::UnknownTargetType)
    ));
    assert!(matches!(
        run_error(
            "references: [{target: {type: task, id: x, context: invalid}, relation: supports}]"
        ),
        E::Kernel(ReferenceProjectionError::ContextNotAllowed)
    ));
    assert!(matches!(
        run_error("references: [{target: {type: task, id: ''}, relation: supports}]"),
        E::Kernel(ReferenceProjectionError::EmptyTargetId)
    ));
    assert!(matches!(
        run_error(
            "references: [{target: {type: task, id: x, scope: project}, relation: supports}]"
        ),
        E::Kernel(ReferenceProjectionError::MissingScopeId)
    ));
}
#[test]
fn output_results_and_individual_identifiers_are_bounded_per_document() {
    let original = original();
    let text = doc(
        "references: [{target: {type: knowledge, id: 'é'}, relation: x}, {target: {type: knowledge, id: 'é'}, relation: x}]",
    );
    let parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
    let mut l = limits();
    l.max_results = n(2);
    l.max_identifier_bytes = n(9);
    let Update::Replace(items) =
        project_document_authored_references(&original, &parsed, source(), parser(), l).unwrap()
    else {
        panic!("explicit references must replace");
    };
    assert_eq!(items.len(), 2);
    l.max_results = n(1);
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), parser(), l),
        Err(E::LimitExceeded("reference results"))
    ));
    l = limits();
    l.max_identifier_bytes = n(8);
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), parser(), l),
        Err(E::LimitExceeded("namespace identifier bytes"))
    ));
    l.max_identifier_bytes = n(1);
    let no_context = SourceNamespace {
        scope: LogicalScope::User("x"),
        context: None,
    };
    assert!(matches!(
        project_document_authored_references(&original, &parsed, no_context, parser(), l),
        Err(E::LimitExceeded("identifier bytes"))
    ));
}
#[test]
fn actual_raw_bytes_sdk_budgets_and_mutable_stats() {
    let original = original();
    let text = doc(
        "references: [{target: {type: memory, id: x}, relation: supports}]\nunknown: [[[[x]]]]",
    );
    let mut parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
    parsed.stats = ParseStats::default();
    let mut p = parser();
    p.max_document_bytes = n(text.len() - 1);
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), p, limits()),
        Err(E::LimitExceeded("document bytes"))
    ));
    p = parser();
    p.max_input_bytes = n(text.len() - 1);
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), p, limits()),
        Err(E::LimitExceeded("input bytes"))
    ));
    p = parser();
    p.max_frontmatter_bytes = n(1);
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), p, limits()),
        Err(E::LimitExceeded("frontmatter bytes"))
    ));
    p = parser();
    p.max_yaml_events = n(4);
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), p, limits()),
        Err(E::SemanticYaml(_)) | Err(E::Syntax(_))
    ));
    p = parser();
    p.max_yaml_depth = n(2);
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), p, limits()),
        Err(E::SemanticYaml(_)) | Err(E::Syntax(_))
    ));
    p = parser();
    p.max_projection_bytes = n(4);
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), p, limits()),
        Err(E::SemanticYaml(_)) | Err(E::Syntax(_))
    ));
}
#[test]
fn public_envelope_and_metadata_inconsistency_are_rejected() {
    let original = original();
    let text = doc("references: [{target: {type: task, id: x}, relation: supports}]");
    let mut parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
    parsed.metadata.root = None;
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), parser(), limits()),
        Err(E::InvalidEnvelope)
    ));
    let mut parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
    parsed.envelope.yaml = None;
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), parser(), limits()),
        Err(E::InvalidEnvelope)
    ));
    let mut parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
    parsed.envelope.yaml = Some(usize::MAX..usize::MAX);
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), parser(), limits()),
        Err(E::InvalidEnvelope)
    ));
}
#[test]
fn supplied_original_cancellation_and_deadline_retain_causes() {
    let live = original();
    let text = doc("references: []");
    let parsed = parse_document(&live, text.as_bytes(), "fallback", parser()).unwrap();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancellation);
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    for (original, kind) in [
        (cancelled, io::ErrorKind::Interrupted),
        (expired, io::ErrorKind::TimedOut),
    ] {
        let e =
            project_document_authored_references(&original, &parsed, source(), parser(), limits())
                .unwrap_err();
        assert_eq!(
            e.source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            kind
        );
    }
    assert!(
        project_document_authored_references(&live, &parsed, source(), parser(), limits()).is_ok()
    );
}

#[test]
fn explicit_nested_indentation_and_quoted_multiline_keep_context() {
    let original = original();
    for yaml in [
        "references:\n  - target: {type: knowledge, id: page}\n    relation: |2\n      supports\n    field: >2\n      first\n      second",
        "references:\n  - target: {type: knowledge, id: page}\n    relation: \"sup\n      ports\"\n    field: \"first\n      second\"",
    ] {
        for crlf in [false, true] {
            let text = if crlf {
                doc(yaml).replace('\n', "\r\n")
            } else {
                doc(yaml)
            };
            let parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
            let Update::Replace(rows) = project_document_authored_references(
                &original,
                &parsed,
                source(),
                parser(),
                limits(),
            )
            .unwrap() else {
                panic!()
            };
            assert_eq!(rows.len(), 1);
            assert!(rows[0].relation == "supports\n" || rows[0].relation == "sup ports");
            assert!(
                rows[0].field == Some("first second\n") || rows[0].field == Some("first second")
            );
        }
    }
}
#[test]
fn swapped_metadata_cannot_reclassify_plain_scalars_as_quoted() {
    let original = original();
    let plain = doc("references: [{target: {type: knowledge, id: .nan}, relation: supports}]");
    let quoted = doc("references: [{target: {type: knowledge, id: '.nan'}, relation: supports}]");
    let mut parsed = parse_document(&original, plain.as_bytes(), "fallback", parser()).unwrap();
    let other = parse_document(&original, quoted.as_bytes(), "fallback", parser()).unwrap();
    parsed.metadata = other.metadata;
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), parser(), limits()),
        Err(E::InvalidEnvelope)
    ));
    // Single and double quotes give identical spans, shape and decoded text.
    // Only the syntax style differs; it must still fail integrity validation.
    let double = doc("references: [{target: {type: knowledge, id: \".nan\"}, relation: supports}]");
    let mut parsed = parse_document(&original, double.as_bytes(), "fallback", parser()).unwrap();
    let other = parse_document(&original, quoted.as_bytes(), "fallback", parser()).unwrap();
    assert_eq!(parsed.envelope, other.envelope);
    let mut differing_styles = 0;
    for (a, b) in parsed.metadata.nodes().iter().zip(other.metadata.nodes()) {
        assert_eq!(a.span, b.span);
        if let (
            graphit_knowledge::document::MetadataKind::Scalar {
                value: a,
                style: sa,
            },
            graphit_knowledge::document::MetadataKind::Scalar {
                value: b,
                style: sb,
            },
        ) = (&a.kind, &b.kind)
        {
            assert_eq!(a, b);
            differing_styles += usize::from(sa != sb);
        }
    }
    assert_eq!(differing_styles, 1);
    parsed.metadata = other.metadata;
    assert!(matches!(
        project_document_authored_references(&original, &parsed, source(), parser(), limits()),
        Err(E::InvalidEnvelope)
    ));
}
#[test]
fn project_namespace_inheritance_and_foreign_scope_are_kernel_contracts() {
    let original = original();
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse::<ProjectId>().unwrap();
    let source = SourceNamespace {
        scope: LogicalScope::Project(id),
        context: Some("native"),
    };
    let text = doc(
        "references: [{target: {type: knowledge, id: page}, relation: uses}, {target: {type: knowledge, id: other, scope: user, scope_id: owner}, relation: uses}, {target: {type: knowledge, id: remote, context: foreign}, relation: uses}]",
    );
    let parsed = parse_document(&original, text.as_bytes(), "fallback", parser()).unwrap();
    let Update::Replace(rows) =
        project_document_authored_references(&original, &parsed, source, parser(), limits())
            .unwrap()
    else {
        panic!()
    };
    assert_eq!(rows[0].scope, LogicalScope::Project(id));
    assert_eq!(rows[0].context, Some("native"));
    assert_eq!(rows[1].scope, LogicalScope::User("owner"));
    assert_eq!(rows[1].context, None);
    assert_eq!(rows[2].context, Some("foreign"));
    assert!(matches!(
        run_error("references: [{target: {type: memory, id: x, scope: alien}, relation: uses}]"),
        E::Kernel(ReferenceProjectionError::UnknownScope)
    ));
    let e = run_error(
        "references: [{target: {type: memory, id: x, scope: project, scope_id: bad}, relation: uses}]",
    );
    assert!(matches!(
        e,
        E::Kernel(ReferenceProjectionError::ProjectId(_))
    ));
    assert!(e.source().unwrap().source().is_some());
}
#[test]
fn upstream_rejected_shapes_never_become_admitted_documents() {
    let original = original();
    for yaml in [
        "null",
        "null: ignored",
        "references: []\nreferences: []",
        "references: [{target: {type: task, id: x, id: x}, relation: supports}]",
    ] {
        assert!(
            parse_document(&original, doc(yaml).as_bytes(), "fallback", parser()).is_err(),
            "{yaml}"
        );
    }
}
