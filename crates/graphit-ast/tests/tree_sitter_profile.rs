use graphit_ast::{
    catalog::{
        CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind,
        ProvenanceView,
    },
    prepared_input::{
        ByteSpan, OriginAnchor, OriginPiece, OriginPlan, OriginSegment, OriginalProjection,
        PreparedInput, SyntheticReason, UnmappableReason,
    },
    profiles::{
        ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput, QueryKind,
    },
    scalar_policy::{RegexLimits, ScalarPolicy},
    tree_sitter_engine::{EngineError, GrammarLanguage, ParseControl},
    tree_sitter_profile::{CaptureBinding, PreparedTreeSitterProfile, TreeSitterProfileError},
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
    let id = ProfileId::new("tree-profile-fixture", 256).unwrap();
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

fn json_profile(queries: &str) -> CatalogDraft {
    catalog(format!("language: fixture\ngrammar: tree-sitter-json\nqueries:\n{queries}").as_bytes())
}

fn json() -> Result<GrammarLanguage, EngineError> {
    GrammarLanguage::from_language(tree_sitter_json::LANGUAGE.into())
}

fn rust() -> Result<GrammarLanguage, EngineError> {
    GrammarLanguage::from_language(tree_sitter_rust::LANGUAGE.into())
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

fn present(binding: &CaptureBinding) -> &[usize] {
    let CaptureBinding::Present(indices) = binding else {
        panic!("present capture slots")
    };
    indices
}

#[test]
fn stock_rust_metadata_relations_and_real_source_allocations_survive_factory_drop() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/rust.yaml"));
    let profile = &draft.profiles()[0];
    let prepare_original = context();
    let factory = PreparedTreeSitterProfile::prepare(profile, rust()?, &prepare_original)?;
    prepare_original.cancellation().cancel();
    let original = String::from("pub fn beta_β() { called(); }\r\n// Ω\0");
    let prepared = original.clone();
    assert_ne!(original.as_ptr(), prepared.as_ptr());
    let bound = ByteSpan {
        start: 0,
        end: u64::try_from(original.len())?,
    };
    let segments = [OriginSegment::Direct {
        prepared: bound,
        original: bound,
    }];
    let document_original = context();
    let input = PreparedInput::new(
        original.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &document_original,
    )?;
    let result = factory.collect_matches(&input)?;
    drop(factory);
    for group in &result.groups {
        assert!(group.roles.declaration.is_none());
        assert!(group.roles.context.is_none());
        for matched in &group.matches {
            assert_eq!(matched.bindings.declaration, CaptureBinding::NotConfigured);
            assert_eq!(matched.bindings.context, CaptureBinding::NotConfigured);
        }
    }
    assert!(std::ptr::eq(result.profile, profile));
    assert_eq!(
        result.groups.len(),
        profile.queries().value().as_slice().len()
    );
    assert_eq!(result.tree.source(), prepared);
    assert_ne!(result.tree.source().as_ptr(), prepared.as_ptr());
    let mut function = false;
    let mut call = false;
    for (group, descriptor) in result.groups.iter().zip(profile.queries().value().iter()) {
        assert!(std::ptr::eq(group.query, descriptor));
        assert!(matches!(
            group.query.value().pattern().provenance(),
            ProvenanceView::Declared(_)
        ));
        for found in &group.matches {
            for capture in &found.captures {
                let range = capture.native.range.clone();
                assert_eq!(
                    capture.prepared.text().as_ptr(),
                    prepared[range.clone()].as_ptr()
                );
                assert_eq!(capture.prepared.text(), capture.native.text);
                assert_eq!(capture.prepared.start_point(), capture.native.start);
                let OriginalProjection::Exact(source) = &capture.original else {
                    panic!("identity origin")
                };
                assert_eq!(source.text().as_ptr(), original[range].as_ptr());
                assert!(
                    result
                        .nodes
                        .iter()
                        .any(|node| node.node_id == capture.native.node_id)
                );
                function |= capture.native.text == "beta_β"
                    && *group.query.value().r#type().value() == QueryKind::Entity;
                call |= capture.native.text == "called"
                    && *group.query.value().r#type().value() == QueryKind::Relation;
            }
        }
    }
    assert!(function && call);
    for node in &result.nodes {
        if let Some(parent) = node.parent {
            assert!(parent < result.nodes.len());
            assert!(result.nodes[parent].range.start <= node.range.start);
            assert!(result.nodes[parent].range.end >= node.range.end);
        }
    }
    Ok(())
}

#[test]
fn multiple_patterns_missing_roles_aliases_and_duplicate_descriptors_are_explicit() -> TestResult {
    let draft = json_profile(
        r#"
  - data_key: pairs
    graph_label: Pair
    pattern: |
      (pair key: (string (string_content) @name) value: (number) @value)
      (pair key: (string (string_content) @name) value: (string))
    value_capture: value
    declaration_capture: value
    context_capture: value
    value_label: Value
  - data_key: pairs
    type: relation
    relation_type: USES
    pattern: |
      (pair key: (string (string_content) @name) value: (number) @value)
      (pair key: (string (string_content) @name) value: (string))
    value_capture: value
    declaration_capture: value
    context_capture: value
    value_label: Value
    name_is_data: true
  - data_key: missing
    graph_label: Pair
    pattern: '(pair key: (string (string_content) @name))'
    name_capture: absent
    declaration_capture: absent
    context_capture: absent
  - data_key: aliases
    graph_label: Pair
    pattern: '(pair key: (string (string_content) @name))'
    value_capture: name
    value_label: Value
    span_capture: name
    declaration_capture: name
    context_capture: name
    parent_capture: name
    parent_label: Pair
    qualifier_capture: name
"#,
    );
    let original = context();
    let factory = PreparedTreeSitterProfile::prepare(&draft.profiles()[0], json()?, &original)?;
    identity(r#"{"n":1,"s":"x"}"#, &original, |input| {
        let result = factory.collect_matches(input)?;
        assert_eq!(result.groups.len(), 4);
        for group in &result.groups[..2] {
            assert_eq!(group.roles.name.name, "name");
            assert_eq!(group.matches.len(), 2);
            assert_eq!(group.matches[0].pattern_index, 0);
            assert_eq!(group.matches[1].pattern_index, 1);
            assert_eq!(present(&group.matches[0].bindings.value).len(), 1);
            assert_eq!(
                group.matches[0].bindings.declaration,
                group.matches[0].bindings.value
            );
            assert_eq!(
                group.matches[0].bindings.context,
                group.matches[0].bindings.value
            );
            assert_eq!(
                group.matches[1].bindings.declaration,
                CaptureBinding::Missing { declared: true }
            );
            assert_eq!(
                group.matches[1].bindings.context,
                CaptureBinding::Missing { declared: true }
            );
            assert_eq!(
                group.matches[1].bindings.value,
                CaptureBinding::Missing { declared: true }
            );
            assert_eq!(
                group.matches[0].bindings.span,
                CaptureBinding::NotConfigured
            );
        }
        for (first, second) in result.groups[0]
            .matches
            .iter()
            .zip(&result.groups[1].matches)
        {
            assert_eq!(first.bindings, second.bindings);
            assert_eq!(
                first.captures.iter().map(|c| &c.native).collect::<Vec<_>>(),
                second
                    .captures
                    .iter()
                    .map(|c| &c.native)
                    .collect::<Vec<_>>()
            );
        }
        assert!(!*result.groups[0].query.value().name_is_data().value());
        assert!(*result.groups[1].query.value().name_is_data().value());
        for found in &result.groups[2].matches {
            assert_eq!(
                found.bindings.name,
                CaptureBinding::Missing { declared: false }
            );
            assert_eq!(
                found.bindings.declaration,
                CaptureBinding::Missing { declared: false }
            );
            assert_eq!(
                found.bindings.context,
                CaptureBinding::Missing { declared: false }
            );
            assert!(!found.captures.is_empty());
        }
        for found in &result.groups[3].matches {
            for alias in [
                &found.bindings.value,
                &found.bindings.span,
                &found.bindings.declaration,
                &found.bindings.context,
                &found.bindings.parent,
                &found.bindings.qualifier,
            ] {
                assert_eq!(alias, &found.bindings.name);
            }
        }
        Ok(())
    })
}

#[test]
fn name_is_data_preserves_quotes_blank_escaped_multiline_and_long_raw_text() -> TestResult {
    let draft = json_profile(
        r#"
  - data_key: raw
    graph_label: Literal
    pattern: '(string) @payload'
    name_capture: payload
    name_is_data: true
  - data_key: literal
    graph_label: Literal
    pattern: '(string) @payload'
    name_capture: literal
    name_is_data: true
"#,
    );
    let original = context();
    let factory = PreparedTreeSitterProfile::prepare(&draft.profiles()[0], json()?, &original)?;
    for source in [
        String::from(r#""  ""#),
        String::from(r#""a\nb""#),
        format!("\"{}\"", "x".repeat(300)),
    ] {
        identity(&source, &original, |input| {
            let result = factory.collect_matches(input)?;
            assert_eq!(result.groups[0].matches.len(), 1);
            let found = &result.groups[0].matches[0];
            let index = present(&found.bindings.name)[0];
            assert_eq!(found.captures[index].native.text, source);
            assert_eq!(found.captures[index].prepared.text(), source);
            assert_eq!(
                result.groups[1].matches[0].bindings.name,
                CaptureBinding::Missing { declared: false }
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn quantified_capture_order_properties_and_slot_indices_follow_the_actual_sdk() -> TestResult {
    let pattern = r#"((array (number) @item ("," (number) @item)*) @container (#set! @container role "aggregate"))"#;
    let draft = json_profile(&format!(
        "  - data_key: numbers\n    graph_label: Number\n    name_capture: item\n    span_capture: container\n    declaration_capture: item\n    context_capture: item\n    pattern: '{pattern}'\n"
    ));
    let original = context();
    let language = json()?;
    let factory =
        PreparedTreeSitterProfile::prepare(&draft.profiles()[0], language.clone(), &original)?;
    identity("[1,2,3]", &original, |input| {
        let result = factory.collect_matches(input)?;
        drop(factory);
        let group = &result.groups[0];
        let actual_query = language.compile_query(pattern)?;
        let native =
            result
                .tree
                .matches_with_context(&actual_query, ParseControl::default(), &original)?;
        assert_eq!(group.matches.len(), native.len());
        for (found, expected) in group.matches.iter().zip(native) {
            assert_eq!(found.pattern_index, expected.pattern_index);
            assert_eq!(found.properties, expected.properties);
            assert_eq!(
                found.captures.iter().map(|c| &c.native).collect::<Vec<_>>(),
                expected.captures.iter().collect::<Vec<_>>()
            );
            let slots = present(&found.bindings.name);
            assert_eq!(slots.len(), 3);
            assert_eq!(present(&found.bindings.declaration), slots);
            assert_eq!(present(&found.bindings.context), slots);
            assert_eq!(
                slots
                    .iter()
                    .map(|index| found.captures[*index].native.text.as_str())
                    .collect::<Vec<_>>(),
                ["1", "2", "3"]
            );
            let container = present(&found.bindings.span)[0];
            assert_eq!(found.captures[container].native.text, "[1,2,3]");
            let property = &found.properties[0];
            let capture_id = property.capture_id.ok_or("actual property capture ID")?;
            assert_eq!(group.capture_names()[capture_id], "container");
            assert_ne!(capture_id, container);
            assert_eq!(property.key, "role");
            assert_eq!(property.value.as_deref(), Some("aggregate"));
        }
        assert!(
            !group.matches.is_empty(),
            "actual SDK nodes: {:?}",
            result.nodes
        );
        Ok(())
    })
}

#[test]
fn transformed_origins_preserve_synthetic_compound_exact_and_partial_mapping() -> TestResult {
    let draft =
        json_profile("  - data_key: raw\n    graph_label: Value\n    pattern: '(number) @name'\n");
    let original = context();
    let factory = PreparedTreeSitterProfile::prepare(&draft.profiles()[0], json()?, &original)?;
    let synthetic = [OriginSegment::Synthetic {
        prepared: ByteSpan { start: 0, end: 1 },
        reason: SyntheticReason::Wrapper,
    }];
    let input = PreparedInput::new(
        b"",
        b"1",
        OriginPlan {
            segments: &synthetic,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let result = factory.collect_matches(&input)?;
    assert!(matches!(
        result.groups[0].matches[0].captures[0].original,
        OriginalProjection::Synthetic(SyntheticReason::Wrapper)
    ));
    let fragments = [ByteSpan { start: 0, end: 1 }, ByteSpan { start: 2, end: 3 }];
    let mapped = [OriginSegment::MappedAtom {
        prepared: ByteSpan { start: 0, end: 3 },
        original_fragments: &fragments,
    }];
    let removed = [ByteSpan { start: 1, end: 2 }];
    let input = PreparedInput::new(
        b"a-b",
        b"123",
        OriginPlan {
            segments: &mapped,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let result = factory.collect_matches(&input)?;
    let OriginalProjection::Compound(pieces) = &result.groups[0].matches[0].captures[0].original
    else {
        panic!("two original fragments")
    };
    assert_eq!(pieces.len(), 2);
    assert!(matches!(&pieces[0], OriginPiece::Original(range) if range.text() == "a"));
    assert!(matches!(&pieces[1], OriginPiece::Original(range) if range.text() == "b"));
    let fragment = [ByteSpan { start: 0, end: 1 }];
    let mapped = [OriginSegment::MappedAtom {
        prepared: ByteSpan { start: 0, end: 1 },
        original_fragments: &fragment,
    }];
    let input = PreparedInput::new(
        b"X",
        b"1",
        OriginPlan {
            segments: &mapped,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let result = factory.collect_matches(&input)?;
    assert!(
        matches!(&result.groups[0].matches[0].captures[0].original, OriginalProjection::Exact(range) if range.text() == "X")
    );
    let fragment = [ByteSpan { start: 0, end: 1 }];
    let mapped = [OriginSegment::MappedAtom {
        prepared: ByteSpan { start: 0, end: 3 },
        original_fragments: &fragment,
    }];
    let input = PreparedInput::new(
        b"X",
        b"1 2",
        OriginPlan {
            segments: &mapped,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let result = factory.collect_matches(&input)?;
    assert!(!result.groups[0].matches.is_empty());
    for found in &result.groups[0].matches {
        assert!(matches!(
            found.captures[0].original,
            OriginalProjection::Unmappable(UnmappableReason::PartialMappedAtom)
        ));
    }
    Ok(())
}

#[test]
fn actual_missing_nodes_keep_recovery_diagnostics_and_ambiguous_empty_origins() -> TestResult {
    let draft = catalog(b"language: rust\ngrammar: tree-sitter-rust\nqueries:\n  - data_key: missing\n    graph_label: Recovery\n    pattern: '(MISSING \";\") @name'\n");
    let operation = context();
    let language = rust()?;
    let prepared = "fn f(){let x=1}";
    assert!(prepared.ends_with('}'));
    let point = prepared.len() - 1;
    let original = format!("{}XX{}", &prepared[..point], &prepared[point..]);
    let point = u64::try_from(point)?;
    let end = u64::try_from(prepared.len())?;
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: point,
            },
            original: ByteSpan {
                start: 0,
                end: point,
            },
        },
        OriginSegment::Direct {
            prepared: ByteSpan { start: point, end },
            original: ByteSpan {
                start: point + 2,
                end: end + 2,
            },
        },
    ];
    let removed = [ByteSpan {
        start: point,
        end: point + 2,
    }];
    let candidates = [point, point + 2];
    let anchors = [OriginAnchor {
        prepared: point,
        original_candidates: &candidates,
    }];
    let input = PreparedInput::new(
        original.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &removed,
            anchors: &anchors,
        },
        &operation,
    )?;
    let factory = PreparedTreeSitterProfile::prepare(&draft.profiles()[0], language, &operation)?;
    let result = factory.collect_matches(&input)?;
    assert!(result.tree.has_syntax_errors());
    assert!(result.nodes.iter().any(|node| node.missing));
    let captured = &result.groups[0].matches[0].captures[0];
    assert_eq!(captured.native.range.start, usize::try_from(point)?);
    assert_eq!(captured.native.range.end, usize::try_from(point)?);
    assert!(captured.prepared.text().is_empty());
    let OriginalProjection::Ambiguous(origins) = &captured.original else {
        panic!("actual zero-width anchor alternatives")
    };
    assert_eq!(origins.len(), 2);
    assert_eq!(origins[0].start(), usize::try_from(point)?);
    assert_eq!(origins[1].start(), usize::try_from(point + 2)?);
    Ok(())
}

#[test]
fn sdk_query_errors_parser_mismatch_and_actual_original_controls_remain_errors() -> TestResult {
    let original = context();
    for pattern in [
        "(not_a_json_node) @name",
        "((number) @name (#custom? @name))",
        "((number) @name (#is? custom))",
    ] {
        let draft = json_profile(&format!(
            "  - data_key: invalid\n    graph_label: Value\n    pattern: '{pattern}'\n"
        ));
        let error = PreparedTreeSitterProfile::prepare(&draft.profiles()[0], json()?, &original)
            .err()
            .ok_or("actual SDK query error")?;
        assert!(matches!(
            error,
            TreeSitterProfileError::Engine(
                EngineError::Query(_) | EngineError::UnsupportedQuerySemantics(_)
            )
        ));
    }
    let antlr = catalog(b"language: fixture\nparser: antlr4\ngrammar: postgresql\nqueries: []\n");
    assert!(matches!(
        PreparedTreeSitterProfile::prepare(&antlr.profiles()[0], json()?, &original),
        Err(TreeSitterProfileError::ParserMismatch)
    ));
    let wrong_grammar = catalog(b"language: rust\ngrammar: tree-sitter-rust\nqueries:\n  - data_key: functions\n    graph_label: Function\n    pattern: '(function_item) @name'\n");
    assert!(matches!(
        PreparedTreeSitterProfile::prepare(&wrong_grammar.profiles()[0], json()?, &original),
        Err(TreeSitterProfileError::Engine(EngineError::Query(_)))
    ));
    let draft = json_profile(
        "  - data_key: numbers\n    graph_label: Number\n    pattern: '(number) @name'\n",
    );
    let factory = PreparedTreeSitterProfile::prepare(&draft.profiles()[0], json()?, &original)?;
    for (operation, expected) in [
        (context(), ErrorKind::Interrupted),
        (
            SnapshotContext::new(
                Instant::now() - Duration::from_secs(1),
                Cancellation::default(),
            ),
            ErrorKind::TimedOut,
        ),
    ] {
        if expected == ErrorKind::Interrupted {
            operation.cancellation().cancel();
        }
        let error = PreparedTreeSitterProfile::prepare(&draft.profiles()[0], json()?, &operation)
            .err()
            .ok_or("Original rejects preparation")?;
        assert!(
            matches!(error, TreeSitterProfileError::Operation(error) if error.kind() == expected)
        );
    }
    let cancelled = context();
    identity("1", &cancelled, |input| {
        cancelled.cancellation().cancel();
        assert!(
            matches!(factory.collect_matches(input), Err(TreeSitterProfileError::Operation(error)) if error.kind() == ErrorKind::Interrupted)
        );
        Ok(())
    })?;
    let expires = SnapshotContext::new(
        Instant::now() + Duration::from_secs(1),
        Cancellation::default(),
    );
    identity("1", &expires, |input| {
        let remaining = expires
            .deadline()
            .ok_or("finite fixture deadline")?
            .saturating_duration_since(Instant::now());
        std::thread::sleep(remaining + Duration::from_millis(2));
        assert!(
            matches!(factory.collect_matches(input), Err(TreeSitterProfileError::Operation(error)) if error.kind() == ErrorKind::TimedOut)
        );
        Ok(())
    })?;
    let fresh = context();
    identity("2", &fresh, |input| {
        assert_eq!(
            factory.collect_matches(input)?.groups[0].matches[0].captures[0]
                .native
                .text,
            "2"
        );
        Ok(())
    })
}

#[test]
fn antlr_only_selectors_are_rejected_before_tree_sitter_queries_compile() -> TestResult {
    let original = context();
    for field in [
        "select: '//*'",
        "keyword: ''",
        "child_rule: ''",
        "negate: false",
        "captures: {}",
    ] {
        let draft = json_profile(&format!(
            "  - data_key: numbers\n    graph_label: Number\n    pattern: '(number) @name'\n    {field}\n"
        ));
        assert!(
            matches!(
                PreparedTreeSitterProfile::prepare(&draft.profiles()[0], json()?, &original),
                Err(TreeSitterProfileError::UnsupportedAntlrQueryFields { query_index: 0 })
            ),
            "{field}"
        );
    }
    let draft =
        catalog(b"language: json\ngrammar: tree-sitter-json\nantlr_contexts: []\nqueries: []\n");
    assert!(matches!(
        PreparedTreeSitterProfile::prepare(&draft.profiles()[0], json()?, &original),
        Err(TreeSitterProfileError::UnsupportedAntlrContexts)
    ));
    Ok(())
}
