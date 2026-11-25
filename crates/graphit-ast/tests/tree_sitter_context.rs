use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{
        ByteSpan, OriginAnchor, OriginPlan, OriginSegment, OriginalProjection, PreparedInput,
        SyntheticReason,
    },
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    tree_sitter_context::{
        CaptureContextScope, ContextError, ContextIssue, ContextMissingReason, ContextName,
        ContextPolicyOrigin, ContextSource, ContextualTreeSitterDocument,
        PreparedTreeSitterContexts,
    },
    tree_sitter_engine::GrammarLanguage,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{
    error::Error,
    io::ErrorKind,
    time::{Duration, Instant},
};
type TestResult = Result<(), Box<dyn Error>>;
fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}
fn catalog(bytes: &[u8]) -> CatalogDraft {
    let original = context();
    let id = ProfileId::new("context-fixture", 256).unwrap();
    let decoder = ProfileDecoder::new(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })
        .unwrap(),
    );
    let decoded = [decoder
        .decode(
            ProfileInput { id: &id, bytes },
            &ProfileLimits::new(ProfileLimitsInput {
                input_bytes: 1 << 20,
                nodes: 32768,
                depth: 64,
                text_bytes: 1 << 20,
                scalar_bytes: 32768,
                collection_entries: 4096,
            })
            .unwrap(),
            &original,
        )
        .unwrap()];
    CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles: &decoded,
        }],
        &CatalogLimits::new(CatalogLimitsInput {
            policy_entries: 4096,
            selector_bytes: 32768,
        })
        .unwrap(),
        &original,
    )
    .unwrap()
}

fn identity(
    text: &str,
    original: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    let bound = ByteSpan {
        start: 0,
        end: u64::try_from(text.len())?,
    };
    let segments = if text.is_empty() {
        vec![]
    } else {
        vec![OriginSegment::Direct {
            prepared: bound,
            original: bound,
        }]
    };
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        original,
    )?;
    action(&input)
}

fn json() -> Result<GrammarLanguage, graphit_ast::tree_sitter_engine::EngineError> {
    GrammarLanguage::from_language(tree_sitter_json::LANGUAGE.into())
}
fn rust() -> Result<GrammarLanguage, graphit_ast::tree_sitter_engine::EngineError> {
    GrammarLanguage::from_language(tree_sitter_rust::LANGUAGE.into())
}
fn json_profile(configuration: &str, queries: &str) -> CatalogDraft {
    catalog(
        format!("language: json\ngrammar: tree-sitter-json\n{configuration}\nqueries:\n{queries}")
            .as_bytes(),
    )
}
fn numbers() -> &'static str {
    "  - data_key: numbers\n    graph_label: Value\n    pattern: '(number) @name'\n"
}
fn scope_for<'a>(
    doc: &'a ContextualTreeSitterDocument<'_, '_, '_, '_>,
    text: &str,
) -> Result<&'a CaptureContextScope, Box<dyn Error>> {
    let obs = doc.observations()?;
    doc.scopes()?
        .iter()
        .find(|s| {
            obs.groups[s.query_index].matches[s.match_index].captures[s.capture_index]
                .prepared
                .text()
                == text
        })
        .ok_or_else(|| format!("capture missing: {text}").into())
}
fn names(
    doc: &ContextualTreeSitterDocument<'_, '_, '_, '_>,
    scope: &CaptureContextScope,
) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let contexts = doc.contexts()?;
    Ok(scope
        .contexts
        .iter()
        .map(|&i| {
            let c = &contexts[i];
            let text = match &c.name {
                ContextName::Found { selected, .. } => selected.prepared.text(),
                _ => panic!("linked Missing"),
            };
            (c.label.clone(), text.into())
        })
        .collect())
}

#[test]
fn native_queries_preserve_alternatives_self_and_missing_ancestors() -> TestResult {
    let original = context();
    for (query, quoted, pattern) in [
        (
            "(pair key: (string (string_content) @name)) @context",
            false,
            0,
        ),
        ("(pair (string) @name) @context", true, 0),
        (
            "(pair value: (number) @name) @context\n(pair key: (string (string_content) @name)) @context",
            false,
            1,
        ),
    ] {
        // The first alternative only applies to numeric-valued pairs; the outer
        // object falls through to the second native pattern.
        let draft = json_profile(
            &format!(
                "context_types: {{pair: Pair}}\ncontext_name_queries: {{pair: {}}}\nanon_func_types: []",
                serde_json::to_string(query)?
            ),
            "  - data_key: names\n    graph_label: Value\n    pattern: '[(string_content) (number)] @name'\n",
        );
        let factory =
            PreparedTreeSitterContexts::prepare(&draft.profiles()[0], json()?, &original)?;
        identity(r#"{"outer":{"same":1},"same":2}"#, &original, |input| {
            let doc = factory.collect(input)?;
            let scope = scope_for(&doc, "1")?;
            let linked = names(&doc, scope)?;
            if pattern == 1 {
                assert_eq!(linked, vec![("Pair".into(), "outer".into())]);
            } else if quoted {
                assert_eq!(
                    linked,
                    vec![
                        ("Pair".into(), "\"same\"".into()),
                        ("Pair".into(), "\"outer\"".into())
                    ]
                );
            } else {
                assert_eq!(
                    linked,
                    vec![
                        ("Pair".into(), "same".into()),
                        ("Pair".into(), "outer".into())
                    ]
                );
            }
            assert!(
                matches!(doc.contexts()?[scope.contexts.last().copied().unwrap()].name,ContextName::Found{pattern_index:Some(p),..} if p==pattern)
            );
            if !quoted {
                let key = scope_for(&doc, "outer")?;
                assert!(key.contexts.is_empty());
                assert!(!key.excluded_self.is_empty());
            }
            Ok(())
        })?;
    }
    let draft = json_profile(
        "context_types: {pair: Pair}\ncontext_name_queries: {pair: '(pair key: (string (string_content) @name)) @context'}\nanon_func_types: []",
        numbers(),
    );
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], json()?, &original)?;
    identity(r#"{"outer":{"":1}}"#, &original, |input| {
        let doc = factory.collect(input)?;
        let scope = scope_for(&doc, "1")?;
        assert_eq!(names(&doc, scope)?, vec![("Pair".into(), "outer".into())]);
        assert_eq!(scope.unresolved.len(), 1);
        assert!(matches!(
            doc.contexts()?[scope.unresolved[0]].name,
            ContextName::Missing {
                reason: ContextMissingReason::NoQueryMatch
            }
        ));
        assert!(
            doc.diagnostics()?
                .iter()
                .any(|d| d.issue == ContextIssue::MissingName)
        );
        Ok(())
    })?;
    let omitted = json_profile(
        "context_types: {pair: Pair}\nanon_func_types: []",
        numbers(),
    );
    let factory = PreparedTreeSitterContexts::prepare(&omitted.profiles()[0], json()?, &original)?;
    identity(r#"{"a":1}"#, &original, |input| {
        let doc = factory.collect(input)?;
        let scope = scope_for(&doc, "1")?;
        assert!(scope.contexts.is_empty());
        assert!(matches!(
            doc.contexts()?[scope.unresolved[0]].name,
            ContextName::Missing {
                reason: ContextMissingReason::NoDefaultNameField
            }
        ));
        Ok(())
    })
}

#[test]
fn native_query_errors_and_capture_contracts_are_not_silently_accepted() -> TestResult {
    let original = context();
    for query in [
        "",
        "(unknown) @context",
        "(pair key: (unknown) @name) @context",
        "(pair",
        "(pair) @context",
        "(pair) @name",
        "(pair key: (_) @name) @owner",
        "(pair key: (_) @name value: (_) @name) @context",
        "(object (pair key: (_) @name) @context) @context",
    ] {
        let draft = json_profile(
            &format!(
                "context_types: {{pair: Pair}}\ncontext_name_queries: {{pair: {}}}\nanon_func_types: []",
                serde_json::to_string(query)?
            ),
            numbers(),
        );
        assert!(
            PreparedTreeSitterContexts::prepare(&draft.profiles()[0], json()?, &original).is_err(),
            "invalid query {query}"
        );
    }
    let draft = json_profile(
        "context_types: {pair: Pair}\ncontext_name_queries: {pair: '(object (pair key: (_) @name)) @context'}",
        numbers(),
    );
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], json()?, &original)?;
    identity(r#"{"a":1}"#, &original, |input| {
        assert!(matches!(
            factory.collect(input),
            Err(ContextError::InvalidQuery { .. })
        ));
        Ok(())
    })?;
    for config in [
        "context_types: {unknown: Owner}",
        "anon_func_types: [unknown]",
        "context_types: {}\ncontext_name_queries: {pair: '(pair key: (_) @name) @context'}",
    ] {
        let draft = json_profile(config, numbers());
        assert!(matches!(
            PreparedTreeSitterContexts::prepare(&draft.profiles()[0], json()?, &original),
            Err(ContextError::UnknownContextKind { .. } | ContextError::UnknownAnonymousKind { .. })
        ));
    }
    Ok(())
}

#[test]
fn default_provenance_rust_fields_empty_groups_and_original_read_controls() -> TestResult {
    let original = context();
    for config in ["", "context_types: {}\nanon_func_types: []"] {
        let draft = json_profile(config, numbers());
        let factory =
            PreparedTreeSitterContexts::prepare(&draft.profiles()[0], json()?, &original)?;
        identity("1", &original, |input| {
            let doc = factory.collect(input)?;
            let c = doc.configuration()?;
            assert!(c.applicable_context_types.is_empty());
            assert!(doc.contexts()?.is_empty());
            if config.is_empty() {
                assert_eq!(c.context_policy, ContextPolicyOrigin::Default);
                assert_eq!(c.default_contexts.len(), 11);
                assert_eq!(c.default_anonymous.len(), 3);
            } else {
                assert_eq!(c.context_policy, ContextPolicyOrigin::Declared);
                assert!(c.default_contexts.is_empty());
            }
            Ok(())
        })?;
    }
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/rust.yaml"));
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &original)?;
    identity(
        "mod outer { fn f() { const A: i32 = 1; } }",
        &original,
        |input| {
            let doc = factory.collect(input)?;
            assert_eq!(
                names(&doc, scope_for(&doc, "A")?)?,
                vec![
                    ("Function".into(), "f".into()),
                    ("Namespace".into(), "outer".into())
                ]
            );
            assert_eq!(
                names(&doc, scope_for(&doc, "f")?)?,
                vec![("Namespace".into(), "outer".into())]
            );
            Ok(())
        },
    )?;
    let empty = json_profile("context_types: {}\nanon_func_types: []", "  []");
    let factory = PreparedTreeSitterContexts::prepare(&empty.profiles()[0], json()?, &original)?;
    identity("[1]", &original, |input| {
        let doc = factory.collect(input)?;
        assert!(doc.observations()?.groups.is_empty());
        assert!(doc.scopes()?.is_empty());
        assert!(!doc.observations()?.nodes.is_empty());
        Ok(())
    })?;
    let unused = json_profile(
        "context_types: {pair: Pair}\ncontext_name_queries: {pair: '(pair key: (string (string_content) @name)) @context'}\nanon_func_types: []",
        "  []",
    );
    let factory = PreparedTreeSitterContexts::prepare(&unused.profiles()[0], json()?, &original)?;
    identity(r#"{"unused":{"nested":2}}"#, &original, |input| {
        let doc = factory.collect(input)?;
        assert!(doc.contexts()?.is_empty());
        assert!(doc.diagnostics()?.is_empty());
        Ok(())
    })?;
    let selected = json_profile(
        "context_types: {pair: Pair}\ncontext_name_queries: {pair: '(pair key: (string (string_content) @name)) @context'}\nanon_func_types: []",
        "  - data_key: one\n    graph_label: Value\n    pattern: '((number) @name (#eq? @name \"1\"))'\n",
    );
    let factory = PreparedTreeSitterContexts::prepare(&selected.profiles()[0], json()?, &original)?;
    identity(r#"{"é🙂":1,"unused":{"nested":2}}"#, &original, |input| {
        let doc = factory.collect(input)?;
        assert_eq!(doc.contexts()?.len(), 1);
        assert_eq!(
            names(&doc, scope_for(&doc, "1")?)?,
            vec![("Pair".into(), "é🙂".into())]
        );
        Ok(())
    })?;
    for expected in [ErrorKind::Interrupted, ErrorKind::TimedOut] {
        let request = if expected == ErrorKind::Interrupted {
            context()
        } else {
            SnapshotContext::new(
                Instant::now() + Duration::from_secs(1),
                Cancellation::default(),
            )
        };
        let draft = json_profile("context_types: {}\nanon_func_types: []", numbers());
        let factory =
            PreparedTreeSitterContexts::prepare(&draft.profiles()[0], json()?, &original)?;
        identity("1", &request, |input| {
            let doc = factory.collect(input)?;
            if expected == ErrorKind::Interrupted {
                request.cancellation().cancel()
            } else {
                std::thread::sleep(Duration::from_millis(1100))
            }
            assert!(
                matches!(doc.scopes(),Err(ContextError::Operation(ref e))if e.kind()==expected)
            );
            assert!(
                matches!(factory.collect(input),Err(ContextError::Operation(ref e))if e.kind()==expected)
            );
            Ok(())
        })?;
        assert!(
            matches!(PreparedTreeSitterContexts::prepare(&draft.profiles()[0],json()?,&request),Err(ContextError::Operation(ref e))if e.kind()==expected)
        );
    }
    Ok(())
}

#[test]
fn native_scopes_survive_nonexact_origins_and_actual_recovery() -> TestResult {
    let original = context();
    let draft = json_profile(
        "context_types: {pair: Pair}\ncontext_name_queries: {pair: '(pair key: (string (string_content) @name)) @context'}\nanon_func_types: []",
        numbers(),
    );
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], json()?, &original)?;
    let prepared = br#"{"a":1}"#;
    let bound = ByteSpan {
        start: 0,
        end: prepared.len() as u64,
    };
    let synthetic = [OriginSegment::Synthetic {
        prepared: bound,
        reason: SyntheticReason::Wrapper,
    }];
    let removed = [ByteSpan { start: 0, end: 1 }];
    let input = PreparedInput::new(
        b"A",
        prepared,
        OriginPlan {
            segments: &synthetic,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let doc = factory.collect(&input)?;
    let scope = scope_for(&doc, "1")?;
    assert_eq!(names(&doc, scope)?, vec![("Pair".into(), "a".into())]);
    let c = &doc.contexts()?[scope.contexts[0]];
    assert!(
        matches!(c.name,ContextName::Found{ref selected,..}if matches!(selected.original,OriginalProjection::Synthetic(_)))
    );
    assert!(
        doc.diagnostics()?
            .iter()
            .any(|d| d.issue == ContextIssue::NonExactName)
    );
    let fragments = [ByteSpan { start: 0, end: 5 }];
    let mapped = [OriginSegment::MappedAtom {
        prepared: bound,
        original_fragments: &fragments,
    }];
    let input = PreparedInput::new(
        b"ALPHA",
        prepared,
        OriginPlan {
            segments: &mapped,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let doc = factory.collect(&input)?;
    let scope = scope_for(&doc, "1")?;
    assert_eq!(names(&doc, scope)?, vec![("Pair".into(), "a".into())]);
    assert!(
        matches!(doc.contexts()?[scope.contexts[0]].name,ContextName::Found{ref selected,..}if matches!(selected.original,OriginalProjection::Unmappable(_)))
    );
    let direct = [
        OriginSegment::Direct {
            prepared: ByteSpan { start: 0, end: 5 },
            original: ByteSpan { start: 0, end: 5 },
        },
        OriginSegment::Direct {
            prepared: ByteSpan { start: 5, end: 7 },
            original: ByteSpan { start: 6, end: 8 },
        },
    ];
    let removed = [ByteSpan { start: 5, end: 6 }];
    let input = PreparedInput::new(
        br#"{"a":X1}"#,
        prepared,
        OriginPlan {
            segments: &direct,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let doc = factory.collect(&input)?;
    let scope = scope_for(&doc, "1")?;
    assert!(matches!(
        doc.contexts()?[scope.contexts[0]]
            .owner
            .as_ref()
            .ok_or("native owner")?
            .original,
        OriginalProjection::Compound(_)
    ));
    assert!(
        doc.diagnostics()?
            .iter()
            .any(|d| d.issue == ContextIssue::NonExactOwner)
    );
    let missing = json_profile(
        "context_types: {array: Array}\nanon_func_types: []",
        "  - data_key: missing\n    graph_label: Value\n    pattern: '(MISSING \"]\") @name'\n",
    );
    let factory = PreparedTreeSitterContexts::prepare(&missing.profiles()[0], json()?, &original)?;
    let direct = [OriginSegment::Direct {
        prepared: ByteSpan { start: 0, end: 4 },
        original: ByteSpan { start: 0, end: 4 },
    }];
    let removed = [ByteSpan { start: 4, end: 6 }];
    let candidates = [4, 6];
    let anchors = [OriginAnchor {
        prepared: 4,
        original_candidates: &candidates,
    }];
    let input = PreparedInput::new(
        b"[1,2XX",
        b"[1,2",
        OriginPlan {
            segments: &direct,
            removed_original: &removed,
            anchors: &anchors,
        },
        &original,
    )?;
    let doc = factory.collect(&input)?;
    let observations = doc.observations()?;
    let captured = &observations.groups[0].matches[0].captures[0];
    assert!(matches!(
        captured.original,
        OriginalProjection::Ambiguous(_)
    ));
    assert_eq!(captured.native.range, 4..4);
    assert!(observations.nodes.iter().any(|n| n.missing));
    let scope = &doc.scopes()?[0];
    assert!(!scope.unresolved.is_empty());
    assert!(
        doc.diagnostics()?
            .iter()
            .any(|d| d.issue == ContextIssue::RecoveredOwner)
    );
    Ok(())
}

#[test]
fn repeated_native_captures_properties_and_generation_survive_factory_drop() -> TestResult {
    let original = context();
    let draft = json_profile(
        "context_types: {array: Array}\ncontext_name_queries: {array: '(array . (number) . (number) @name) @context'}\nanon_func_types: []",
        "  - data_key: sequence\n    graph_label: Value\n    name_capture: item\n    pattern: '((array (number) @item (\",\" (number) @item)*) @name (#set! role \"raw\"))'\n",
    );
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], json()?, &original)?;
    identity("[1,2,3]", &original, |input| {
        let doc = factory.collect(input)?;
        drop(factory);
        let observations = doc.observations()?;
        let matched = &observations.groups[0].matches[0];
        assert_eq!(
            matched
                .captures
                .iter()
                .filter(|c| c.native.name == "item")
                .count(),
            3
        );
        assert!(
            matched
                .properties
                .iter()
                .any(|p| p.key == "role" && p.value.as_deref() == Some("raw"))
        );
        assert_eq!(doc.scopes()?.len(), matched.captures.len());
        for (capture_index, capture) in matched.captures.iter().enumerate() {
            let scope = &doc.scopes()?[capture_index];
            assert_eq!(scope.capture_index, capture_index);
            assert_eq!(
                observations.nodes[scope.capture_node_index].node_id,
                capture.native.node_id
            );
            if capture.prepared.text() == "2" {
                assert!(scope.contexts.is_empty());
                assert_eq!(scope.excluded_self.len(), 1);
            }
            if matches!(capture.prepared.text(), "1" | "3") {
                assert_eq!(names(&doc, scope)?, vec![("Array".into(), "2".into())]);
            }
        }
        Ok(())
    })
}

#[test]
fn sdk_recovered_name_stays_found_and_linked_with_recovery() -> TestResult {
    let original = context();
    let draft = json_profile(
        "context_types: {pair: Pair}\ncontext_name_queries: {pair: '(pair key: (_) @name) @context'}\nanon_func_types: []",
        numbers(),
    );
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], json()?, &original)?;
    identity(r#"{"bad\q":1}"#, &original, |input| {
        let doc = factory.collect(input)?;
        let scope = scope_for(&doc, "1")?;
        assert_eq!(scope.contexts.len(), 1);
        let index = scope.contexts[0];
        let observation = &doc.contexts()?[index];
        match &observation.name {
            ContextName::Found { selected, .. } => {
                assert!(selected.recovery.contains_error || selected.recovery.contains_missing);
                assert_eq!(selected.prepared.text(), r#""bad\q""#);
                let nodes = &doc.observations()?.nodes;
                assert_eq!(nodes[selected.node_index].field.as_deref(), Some("key"));
            }
            _ => panic!("recovered selected name remains Found"),
        }
        assert!(
            doc.diagnostics()?
                .iter()
                .any(|d| d.context_index == index && d.issue == ContextIssue::RecoveredName)
        );
        Ok(())
    })
}

#[test]
fn builtin_hcl_toml_javascript_contexts_and_language_ownership() -> TestResult {
    let original = context();
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/hcl.yaml"));
    let language = graphit_ast::tree_sitter_engine::builtin("tree-sitter-hcl")?;
    let factory =
        PreparedTreeSitterContexts::prepare(&draft.profiles()[0], language.clone(), &original)?;
    drop(language);
    identity(
        "resource \"aws_instance\" \"web\" { count = 1 }\nvariable \"region\" { default = 2 }\nresource \"aws_instance\" \"\" { count = 3 }\nresource \"aws_instance\" \"web\" \"third\" { count = 4 }\nresource \"aws_instance\" \"\" \"third\" { count = 5 }",
        &original,
        |input| {
            let doc = factory.collect(input)?;
            for (value, expected, alternative) in [
                ("1", "web", 0),
                ("2", "region", 1),
                ("3", "aws_instance", 1),
                ("4", "web", 0),
                ("5", "aws_instance", 1),
            ] {
                let scope = scope_for(&doc, value)?;
                assert_eq!(
                    names(&doc, scope)?,
                    vec![("Resource".into(), expected.into())]
                );
                match &doc.contexts()?[scope.contexts[0]].name {
                    ContextName::Found { pattern_index, .. } => {
                        assert_eq!(*pattern_index, Some(alternative))
                    }
                    _ => panic!("native HCL query"),
                }
            }
            assert!(scope_for(&doc, "web")?.contexts.is_empty());
            assert!(!scope_for(&doc, "web")?.excluded_self.is_empty());
            Ok(())
        },
    )?;
    drop(factory);
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/toml.yaml"));
    let factory = PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        graphit_ast::tree_sitter_engine::builtin("tree-sitter-toml")?,
        &original,
    )?;
    identity(
        "[server]\nport=1\n[a.b]\nport=2\n[[workers]]\nport=3\n",
        &original,
        |input| {
            let doc = factory.collect(input)?;
            for (value, label, name, alternative) in [
                ("1", "Table", "server", 0),
                ("2", "Table", "a.b", 1),
                ("3", "ArrayTable", "workers", 0),
            ] {
                let scope = scope_for(&doc, value)?;
                assert_eq!(names(&doc, scope)?, vec![(label.into(), name.into())]);
                assert!(
                    matches!(doc.contexts()?[scope.contexts[0]].name,ContextName::Found{pattern_index:Some(n),..}if n==alternative)
                );
            }
            assert!(scope_for(&doc, "server")?.contexts.is_empty());
            Ok(())
        },
    )?;
    drop(factory);
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/javascript.yaml"));
    let factory = PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        graphit_ast::tree_sitter_engine::builtin("tree-sitter-javascript")?,
        &original,
    )?;
    identity(
        "const f=()=>{const x=1;return x;}; class C { method(){const y=2;} } function outer(){ const z=3; } const a=1,b=2; const named=function inner(){const n=4;}; const gen=function* innerG(){const g=5;}; function* declGen(){const h=6;}",
        &original,
        |input| {
            let doc = factory.collect(input)?;
            let x = scope_for(&doc, "x")?;
            assert_eq!(
                names(&doc, x)?,
                vec![
                    ("Function".into(), "f".into()),
                    ("Variable".into(), "f".into())
                ]
            );
            assert_eq!(
                doc.contexts()?[x.contexts[0]].source,
                ContextSource::Anonymous
            );
            let ctx = &doc.contexts()?[x.contexts[0]];
            assert_eq!(
                doc.observations()?.nodes[ctx.owner.as_ref().ok_or("anonymous owner")?.node_index]
                    .kind,
                "variable_declarator"
            );
            assert_eq!(
                names(&doc, scope_for(&doc, "y")?)?,
                vec![
                    ("Method".into(), "method".into()),
                    ("Class".into(), "C".into())
                ]
            );
            assert_eq!(
                names(&doc, scope_for(&doc, "z")?)?,
                vec![("Function".into(), "outer".into())]
            );
            for (captured, name, trigger) in [
                ("n", "named", "function_expression"),
                ("g", "gen", "generator_function"),
            ] {
                let scope = scope_for(&doc, captured)?;
                assert_eq!(
                    names(&doc, scope)?,
                    vec![
                        ("Function".into(), name.into()),
                        ("Variable".into(), name.into())
                    ]
                );
                let observation = &doc.contexts()?[scope.contexts[0]];
                assert_eq!(observation.source, ContextSource::Anonymous);
                assert_eq!(
                    doc.observations()?.nodes[observation.trigger.node_index].kind,
                    trigger
                );
                assert_eq!(
                    doc.observations()?.nodes[observation
                        .owner
                        .as_ref()
                        .ok_or("native anonymous owner")?
                        .node_index]
                        .kind,
                    "variable_declarator"
                );
            }
            assert_eq!(
                names(&doc, scope_for(&doc, "h")?)?,
                vec![("Function".into(), "declGen".into())]
            );
            assert_eq!(
                names(&doc, scope_for(&doc, "b")?)?,
                vec![("Variable".into(), "a".into())]
            );
            assert!(scope_for(&doc, "f")?.contexts.is_empty());
            assert!(!scope_for(&doc, "f")?.excluded_self.is_empty());
            Ok(())
        },
    )?;
    drop(factory);
    for config in ["", "context_types: {}\nanon_func_types: []"] {
        let draft=catalog(format!("language: javascript\ngrammar: tree-sitter-javascript\n{config}\nqueries:\n  - data_key: variables\n    graph_label: Variable\n    pattern: '(variable_declarator name: (identifier) @name)'\n").as_bytes());
        let factory = PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            graphit_ast::tree_sitter_engine::builtin("tree-sitter-javascript")?,
            &original,
        )?;
        identity(
            "function outer(){const z=1;} const f=()=>{const x=2;};",
            &original,
            |input| {
                let doc = factory.collect(input)?;
                if config.is_empty() {
                    assert_eq!(
                        names(&doc, scope_for(&doc, "z")?)?,
                        vec![("Function".into(), "outer".into())]
                    );
                    assert_eq!(
                        names(&doc, scope_for(&doc, "x")?)?,
                        vec![("Function".into(), "f".into())]
                    );
                    assert_eq!(doc.configuration()?.applicable_context_types.len(), 3);
                    assert!(
                        doc.configuration()?
                            .applicable_anon_types
                            .iter()
                            .any(|k| k == "function")
                    );
                } else {
                    assert!(doc.contexts()?.is_empty());
                    assert!(doc.scopes()?.iter().all(|s| s.contexts.is_empty()));
                }
                Ok(())
            },
        )?;
    }
    Ok(())
}
