use std::{
    error::Error,
    io::ErrorKind,
    time::{Duration, Instant},
};

use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{
        ByteSpan, OriginAnchor, OriginPlan, OriginSegment, OriginalProjection, PreparedInput,
        SyntheticReason,
    },
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_fields::{ContributionConflictField, ExtentObservation},
    semantic_model::{
        JsonDeclaredValue, JsonEntityExtent, JsonIssue, JsonLiteral, JsonProjection,
        ParsedJsonDocument, SemanticModelError,
    },
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::GrammarLanguage,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};

type TestResult = Result<(), Box<dyn Error>>;

fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}

fn limits() -> IdentityLimits {
    IdentityLimits::new(IdentityLimitsInput {
        source_bytes: 1 << 20,
        path_components: 32,
        path_component_bytes: 4096,
        identity_text_bytes: 4096,
        owner_depth: 64,
        uid_bytes: 32768,
    })
    .unwrap()
}

fn catalog(bytes: &[u8]) -> CatalogDraft {
    let original = context();
    let id = ProfileId::new("semantic-json-fixture", 256).unwrap();
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

fn custom(queries: &str) -> CatalogDraft {
    catalog(format!("language: json\ngrammar: tree-sitter-json\ncontext_types: {{pair: Pair}}\ncontext_name_queries: {{pair: '(pair key: (_) @name) @context'}}\nembed_labels: [Pair, Value, Custom]\nqueries:\n{queries}").as_bytes())
}

fn prepare_profile<'p>(
    draft: &'p CatalogDraft,
    original: &SnapshotContext,
) -> Result<PreparedTreeSitterContexts<'p>, SemanticModelError> {
    Ok(PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        GrammarLanguage::from_language(tree_sitter_json::LANGUAGE.into())
            .map_err(graphit_ast::tree_sitter_profile::TreeSitterProfileError::Engine)?,
        original,
    )?)
}

fn with_input(
    text: &str,
    original: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    let prepared = text.to_owned();
    assert_ne!(text.as_ptr(), prepared.as_ptr());
    let span = ByteSpan {
        start: 0,
        end: u64::try_from(text.len())?,
    };
    let segments = [OriginSegment::Direct {
        prepared: span,
        original: span,
    }];
    let input = PreparedInput::new(
        text.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        original,
    )?;
    action(&input)
}

#[test]
fn stock_profile_actual_collector_identity_merge_namespace_and_code_projection() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/json.yaml"));
    let original = context();
    let factory = prepare_profile(&draft, &original)?;
    let source = String::from(
        "{\r\n\"outer\":{\"a\":1,\"b\":1},\"s\":\"x\",\"flag\":false,\"items\":[1,\"x\",true,null]}\r\n",
    );
    with_input(&source, &original, |input| {
        let parsed = ParsedJsonDocument::collect(&factory, input, &["src", "Ω.json"], limits())?;
        let model = parsed.assemble()?;
        assert!(model.diagnostics().is_empty());
        assert_eq!(model.parsed().observations().groups.len(), 6);
        assert_eq!(model.entities().len(), 14);
        let pairs: Vec<_> = model
            .entities()
            .iter()
            .filter(|entity| entity.identity().label() == "Pair")
            .collect();
        let mut names: Vec<_> = pairs.iter().map(|entity| entity.name()).collect();
        names.sort_unstable();
        assert_eq!(names, ["a", "b", "flag", "items", "outer", "s"]);
        for entity in &pairs {
            assert!(entity.conflicts().is_empty());
            assert_eq!(
                entity.name_raw().text().as_ptr(),
                source[entity.name_raw().start()..].as_ptr()
            );
            assert_eq!(
                entity.identity().subject().start(),
                entity.name_raw().start()
            );
            let JsonEntityExtent::Exact(extent) = entity.extent() else {
                panic!("actual full pair extent")
            };
            assert_eq!(
                extent.range().text(),
                &source[extent.range().start()..extent.range().end()]
            );
            assert!(extent.range().text().contains(':'));
            if matches!(entity.name(), "a" | "b") {
                assert_eq!(
                    entity
                        .namespace()
                        .iter()
                        .map(|owner| &*owner.name)
                        .collect::<Vec<_>>(),
                    ["outer"]
                );
                assert_eq!(entity.identity().context().len(), 1);
                assert_eq!(entity.contributions().len(), 2);
            }
        }
        let a = pairs.iter().find(|entity| entity.name() == "a").unwrap();
        let b = pairs.iter().find(|entity| entity.name() == "b").unwrap();
        assert_ne!(a.identity().uid().as_str(), b.identity().uid().as_str());
        let JsonDeclaredValue::Present(JsonLiteral::Number { decoded, .. }) = a.declared_value()
        else {
            panic!("decoded number")
        };
        assert_eq!(decoded.get(), "1");
        let a_value = model
            .entities()
            .iter()
            .find(|entity| {
                entity.identity().label() == "Value"
                    && entity.data_key() == "pairs"
                    && entity
                        .namespace()
                        .last()
                        .is_some_and(|owner| &*owner.name == "a")
            })
            .unwrap();
        assert_eq!(
            a_value
                .namespace()
                .iter()
                .map(|owner| &*owner.name)
                .collect::<Vec<_>>(),
            ["outer", "a"]
        );
        assert_eq!(a_value.name_raw().text(), "1");
        assert_eq!(
            model
                .entities()
                .iter()
                .filter(|entity| entity.data_key() == "array_items")
                .count(),
            4
        );
        let projections = model.code_projections()?;
        assert_eq!(projections.len(), 6);
        for projection in projections {
            let JsonProjection::Available(projection) = projection else {
                panic!("exact CodeDocument view")
            };
            assert_eq!(projection.entity.identity().label(), "Pair");
            assert_eq!(projection.language, "json");
            assert_eq!(projection.grammar, "tree-sitter-json");
            assert_eq!(
                projection.text.as_ptr(),
                source[projection.extent.range().start()..].as_ptr()
            );
            assert_eq!(projection.text, projection.extent.range().text());
        }
        Ok(())
    })
}

#[test]
fn native_homonyms_duplicate_keys_paths_labels_and_data_keys_remain_distinct() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/json.yaml"));
    let original = context();
    let factory = prepare_profile(&draft, &original)?;
    with_input(
        r#"{"a":1,"b":1,"nest":{"a":1},"a":2}"#,
        &original,
        |input| {
            let first = ParsedJsonDocument::collect(&factory, input, &["a.json"], limits())?;
            let second = ParsedJsonDocument::collect(&factory, input, &["b.json"], limits())?;
            let a = first.assemble()?;
            let b = second.assemble()?;
            assert_eq!(a.entities().len(), 9);
            assert_eq!(
                a.entities()
                    .iter()
                    .filter(|e| e.identity().label() == "Pair")
                    .count(),
                5
            );
            let mut uids: Vec<_> = a
                .entities()
                .iter()
                .map(|e| e.identity().uid().as_str())
                .collect();
            uids.sort_unstable();
            uids.dedup();
            assert_eq!(uids.len(), 9);
            for (a, b) in a.entities().iter().zip(b.entities()) {
                assert_ne!(a.identity().uid().as_str(), b.identity().uid().as_str());
            }
            Ok(())
        },
    )?;
    let custom = custom(
        "  - data_key: one\n    graph_label: Custom\n    pattern: '(pair key:(string) @name)'\n  - data_key: two\n    graph_label: Custom\n    pattern: '(pair key:(string) @name)'\n  - data_key: one\n    graph_label: Pair\n    pattern: '(pair key:(string) @name)'\n",
    );
    let factory = prepare_profile(&custom, &original)?;
    with_input(r#"{"a":1}"#, &original, |input| {
        let parsed = ParsedJsonDocument::collect(&factory, input, &["a.json"], limits())?;
        let model = parsed.assemble()?;
        assert_eq!(model.entities().len(), 3);
        assert_eq!(
            model.entities()[0].identity().uid().as_str(),
            model.entities()[1].identity().uid().as_str()
        );
        assert_ne!(
            model.entities()[0].data_key(),
            model.entities()[1].data_key()
        );
        assert_ne!(
            model.entities()[0].identity().uid().as_str(),
            model.entities()[2].identity().uid().as_str()
        );
        Ok(())
    })
}

#[test]
fn full_token_custom_queries_decode_empty_escaped_long_strings_and_exact_numbers() -> TestResult {
    let draft = custom(
        "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name value:(string) @value)'\n    value_capture: value\n    value_label: Value\n  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name value:(number) @value)'\n    value_capture: value\n    value_label: Value\n  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name value:[(true)(false)(null)] @value)'\n    value_capture: value\n    value_label: Value\n",
    );
    let original = context();
    let factory = prepare_profile(&draft, &original)?;
    let long = "Ω".repeat(300);
    let strings = serde_json::json!({"": "", "  ": "  ", "\n": "a\nb\"\\", "long": long});
    let string_source = serde_json::to_string(&strings)?;
    let source = format!(
        "{{\r\n{},\"huge\":123456789012345678901234567890,\"small\":1e-400,\"bool\":false,\"null\":null}}\r\n",
        &string_source[1..string_source.len() - 1]
    );
    with_input(&source, &original, |input| {
        let parsed = ParsedJsonDocument::collect(&factory, input, &["values.json"], limits())?;
        let model = parsed.assemble()?;
        assert!(model.diagnostics().is_empty());
        assert_eq!(model.entities().len(), 16);
        for key in ["", "  ", "\n", "long"] {
            let pair = model
                .entities()
                .iter()
                .find(|e| e.identity().label() == "Pair" && e.name() == key)
                .unwrap();
            assert!(!pair.identity().subject().text().is_empty());
            assert_eq!(serde_json::from_str::<String>(pair.name_raw().text())?, key);
            let JsonDeclaredValue::Present(JsonLiteral::String { raw, decoded }) =
                pair.declared_value()
            else {
                panic!("mature string codec")
            };
            assert_eq!(serde_json::from_str::<String>(raw.text())?, &**decoded);
            assert_eq!(decoded.as_ref(), strings[key].as_str().unwrap());
        }
        for (key, expected) in [
            ("huge", "123456789012345678901234567890"),
            ("small", "1e-400"),
        ] {
            let pair = model
                .entities()
                .iter()
                .find(|e| e.identity().label() == "Pair" && e.name() == key)
                .unwrap();
            let JsonDeclaredValue::Present(JsonLiteral::Number { raw, decoded }) =
                pair.declared_value()
            else {
                panic!("validated exact JSON number")
            };
            assert_eq!(raw.text(), expected);
            assert_eq!(decoded.get(), expected);
            assert_eq!(decoded.get().as_ptr(), raw.text().as_ptr());
        }
        let boolean = model
            .entities()
            .iter()
            .find(|e| e.identity().label() == "Pair" && e.name() == "bool")
            .unwrap();
        assert!(matches!(
            boolean.declared_value(),
            JsonDeclaredValue::Present(JsonLiteral::Bool { decoded: false, .. })
        ));
        let null = model
            .entities()
            .iter()
            .find(|e| e.identity().label() == "Pair" && e.name() == "null")
            .unwrap();
        assert!(matches!(
            null.declared_value(),
            JsonDeclaredValue::Present(JsonLiteral::Null { .. })
        ));
        assert_eq!(model.parsed().observations().groups.len(), 3);
        Ok(())
    })?;
    let stock = catalog(include_bytes!("fixtures/catalog/shipped/json.yaml"));
    let factory = prepare_profile(&stock, &original)?;
    with_input(r#"{"":""}"#, &original, |input| {
        let parsed = ParsedJsonDocument::collect(&factory, input, &["empty.json"], limits())?;
        assert!(
            parsed
                .observations()
                .groups
                .iter()
                .all(|g| g.matches.is_empty())
        );
        assert!(parsed.assemble()?.entities().is_empty());
        Ok(())
    })
}

#[test]
fn native_value_and_explicit_full_pair_extents_conflict_without_first_wins() -> TestResult {
    let draft = custom(
        "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name value:(array (number) @value))'\n    value_capture: value\n    value_label: Value\n",
    );
    let original = context();
    let factory = prepare_profile(&draft, &original)?;
    with_input(r#"{"k":[1,1.0]}"#, &original, |input| {
        let parsed = ParsedJsonDocument::collect(&factory, input, &["conflict.json"], limits())?;
        let model = parsed.assemble()?;
        assert_eq!(model.entities().len(), 3);
        let pair = model
            .entities()
            .iter()
            .find(|e| e.identity().label() == "Pair")
            .unwrap();
        assert_eq!(pair.contributions().len(), 2);
        assert!(matches!(
            pair.declared_value(),
            JsonDeclaredValue::Conflict(_)
        ));
        assert_eq!(pair.conflicts()[0].field, ContributionConflictField::Value);
        let values: Vec<_> = model
            .entities()
            .iter()
            .filter(|e| e.identity().label() == "Value")
            .collect();
        assert_ne!(
            values[0].identity().uid().as_str(),
            values[1].identity().uid().as_str()
        );
        assert_eq!(
            values.iter().map(|e| e.name()).collect::<Vec<_>>(),
            ["1", "1.0"]
        );
        assert!(
            model
                .code_projections()?
                .iter()
                .any(|p| matches!(p, JsonProjection::Unavailable(p) if !p.conflicts.is_empty()))
        );
        Ok(())
    })?;
    let draft = custom(
        "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name)'\n  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair value:(object (pair key:(string) @name))) @outer'\n    span_capture: outer\n",
    );
    let factory = prepare_profile(&draft, &original)?;
    with_input(r#"{"outer":{"inner":1}}"#, &original, |input| {
        let parsed = ParsedJsonDocument::collect(&factory, input, &["extent.json"], limits())?;
        let model = parsed.assemble()?;
        let inner = model
            .entities()
            .iter()
            .find(|e| e.name() == "inner")
            .unwrap();
        assert_eq!(inner.contributions().len(), 2);
        assert_eq!(
            inner.conflicts()[0].field,
            ContributionConflictField::Extent
        );
        assert!(matches!(inner.extent(), JsonEntityExtent::Conflict(_)));
        let spans: Vec<_> = inner
            .contributions()
            .iter()
            .map(|c| match &c.extent {
                ExtentObservation::Exact(extent) => extent.range().text(),
                _ => panic!("two genuine full pair ranges"),
            })
            .collect();
        assert_eq!(spans, [r#""inner":1"#, r#""outer":{"inner":1}"#]);
        assert!(
            model
                .code_projections()?
                .iter()
                .any(|p| matches!(p, JsonProjection::Unavailable(p) if !p.conflicts.is_empty()))
        );
        Ok(())
    })
}

#[test]
fn missing_configuration_and_native_recovery_preserve_causal_observations() -> TestResult {
    let draft = custom(
        "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name)'\n    value_capture: absent\n    value_label: Value\n    span_capture: absent\n  - data_key: relations\n    type: relation\n    relation_type: CALLS\n    pattern: '(pair key:(string) @name)'\n",
    );
    let original = context();
    let factory = prepare_profile(&draft, &original)?;
    with_input(r#"{"a":1}"#, &original, |input| {
        let parsed = ParsedJsonDocument::collect(&factory, input, &["missing.json"], limits())?;
        let model = parsed.assemble()?;
        assert_eq!(model.entities().len(), 1);
        assert!(matches!(
            model.entities()[0].extent(),
            JsonEntityExtent::Missing
        ));
        assert!(matches!(
            model.entities()[0].declared_value(),
            JsonDeclaredValue::Missing
        ));
        assert!(model.diagnostics().iter().any(|d| matches!(
            d.issue,
            JsonIssue::MissingCapture {
                role: "value",
                declared: false
            }
        )));
        assert!(
            model
                .diagnostics()
                .iter()
                .any(|d| matches!(d.issue, JsonIssue::UnsupportedQueryKind))
        );
        assert!(matches!(
            model.code_projections()?[0],
            JsonProjection::Unavailable(_)
        ));
        assert_eq!(model.parsed().observations().groups.len(), 2);
        Ok(())
    })?;
    let ordinary =
        "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name)'\n";
    let missing_parent = "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name)'\n    parent_capture: absent\n    parent_label: Pair\n";
    for queries in [
        format!("{ordinary}{missing_parent}"),
        format!("{missing_parent}{ordinary}"),
    ] {
        let draft = custom(&queries);
        let factory = prepare_profile(&draft, &original)?;
        with_input(r#"{"a":1}"#, &original, |input| {
            let parsed =
                ParsedJsonDocument::collect(&factory, input, &["missing-parent.json"], limits())?;
            let model = parsed.assemble()?;
            assert_eq!(model.entities().len(), 1);
            assert_eq!(model.entities()[0].contributions().len(), 1);
            let (diagnostic_index, _) = model
                .diagnostics()
                .iter()
                .enumerate()
                .find(|(_, diagnostic)| {
                    matches!(
                        diagnostic.issue,
                        JsonIssue::MissingCapture {
                            role: "parent",
                            declared: false
                        }
                    )
                })
                .unwrap();
            assert!(
                model.entities()[0]
                    .diagnostic_indices()
                    .contains(&diagnostic_index)
            );
            assert!(matches!(
                model.code_projections()?[0],
                JsonProjection::Unavailable(_)
            ));
            Ok(())
        })?;
    }
    let default = catalog(b"language: json\ngrammar: tree-sitter-json\nqueries: []\n");
    let factory = prepare_profile(&default, &original)?;
    with_input("1", &original, |input| {
        assert!(matches!(
            ParsedJsonDocument::collect(&factory, input, &["default.json"], limits()),
            Err(SemanticModelError::UnsupportedContextConfiguration)
        ));
        Ok(())
    })?;
    let stock = catalog(include_bytes!("fixtures/catalog/shipped/json.yaml"));
    let factory = prepare_profile(&stock, &original)?;
    with_input(r#"{"a":}"#, &original, |input| {
        let parsed = ParsedJsonDocument::collect(&factory, input, &["recovery.json"], limits())?;
        let model = parsed.assemble()?;
        assert!(model.parsed().observations().tree.has_syntax_errors());
        assert!(
            model
                .diagnostics()
                .iter()
                .any(|d| matches!(d.issue, JsonIssue::Recovery { .. }))
        );
        assert!(
            model
                .code_projections()?
                .iter()
                .all(|p| matches!(p, JsonProjection::Unavailable(_)))
        );
        Ok(())
    })
}

#[test]
fn actual_origin_statuses_and_zero_width_missing_ambiguity_do_not_fabricate_uid() -> TestResult {
    let draft = custom(
        "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name)'\n",
    );
    let original = context();
    let factory = prepare_profile(&draft, &original)?;
    let source = "XX\"a\":1}";
    let prepared = "{\"a\":1}";
    let segments = [
        OriginSegment::Synthetic {
            prepared: ByteSpan { start: 0, end: 1 },
            reason: SyntheticReason::Wrapper,
        },
        OriginSegment::Direct {
            prepared: ByteSpan { start: 1, end: 7 },
            original: ByteSpan { start: 2, end: 8 },
        },
    ];
    let removed = [ByteSpan { start: 0, end: 2 }];
    let input = PreparedInput::new(
        source.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let parsed = ParsedJsonDocument::collect(&factory, &input, &["compound.json"], limits())?;
    let model = parsed.assemble()?;
    assert_eq!(model.entities().len(), 1);
    assert!(matches!(
        model.entities()[0].extent(),
        JsonEntityExtent::Exact(_)
    ));
    assert_eq!(model.entities()[0].identity().subject().text(), "\"a\"");

    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan { start: 0, end: 5 },
            original: ByteSpan { start: 0, end: 5 },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan { start: 5, end: 6 },
            reason: SyntheticReason::InsertedByTransform,
        },
        OriginSegment::Direct {
            prepared: ByteSpan { start: 6, end: 7 },
            original: ByteSpan { start: 6, end: 7 },
        },
    ];
    let removed = [ByteSpan { start: 5, end: 6 }];
    let input = PreparedInput::new(
        b"{\"a\":X}",
        b"{\"a\":1}",
        OriginPlan {
            segments: &segments,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let parsed = ParsedJsonDocument::collect(&factory, &input, &["compound.json"], limits())?;
    let model = parsed.assemble()?;
    assert_eq!(model.entities().len(), 1);
    assert!(matches!(
        model.entities()[0].extent(),
        JsonEntityExtent::NonExact(OriginalProjection::Compound(_))
    ));
    assert!(matches!(
        model.code_projections()?[0],
        JsonProjection::Unavailable(_)
    ));

    for synthetic in [true, false] {
        let fragments = [ByteSpan { start: 0, end: 1 }];
        let origin_bytes: &[u8] = if synthetic { b"" } else { b"X" };
        let prepared = "{\"a\":1}";
        let segment = if synthetic {
            OriginSegment::Synthetic {
                prepared: ByteSpan { start: 0, end: 7 },
                reason: SyntheticReason::Wrapper,
            }
        } else {
            OriginSegment::MappedAtom {
                prepared: ByteSpan { start: 0, end: 7 },
                original_fragments: &fragments,
            }
        };
        let segments = [segment];
        let input = PreparedInput::new(
            origin_bytes,
            prepared.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &[],
                anchors: &[],
            },
            &original,
        )?;
        let parsed = ParsedJsonDocument::collect(&factory, &input, &["origin.json"], limits())?;
        let model = parsed.assemble()?;
        assert!(model.entities().is_empty());
        assert!(
            model
                .diagnostics()
                .iter()
                .any(|d| matches!(d.issue, JsonIssue::NonExact { role: "name", .. }))
        );
    }
    let number =
        custom("  - data_key: numbers\n    graph_label: Value\n    pattern: '(number) @name'\n");
    let factory = prepare_profile(&number, &original)?;
    let fragments = [ByteSpan { start: 0, end: 4 }];
    let segments = [OriginSegment::MappedAtom {
        prepared: ByteSpan { start: 0, end: 1 },
        original_fragments: &fragments,
    }];
    let input = PreparedInput::new(
        b"true",
        b"1",
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let parsed = ParsedJsonDocument::collect(&factory, &input, &["kind-mismatch.json"], limits())?;
    let model = parsed.assemble()?;
    assert!(matches!(
        model.parsed().observations().groups[0].matches[0].captures[0].original,
        OriginalProjection::Exact(_)
    ));
    assert!(model.entities().is_empty());
    assert!(model.diagnostics().iter().any(|d| matches!(
        d.issue,
        JsonIssue::OriginalLiteralKindMismatch { .. }
    ) && matches!(d.original, Some(OriginalProjection::Exact(raw)) if raw.text()=="true")));

    let missing = custom(
        "  - data_key: recovery\n    graph_label: Pair\n    pattern: '(MISSING \"]\") @name'\n",
    );
    let factory = prepare_profile(&missing, &original)?;
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
    let parsed = ParsedJsonDocument::collect(&factory, &input, &["ambiguous.json"], limits())?;
    let model = parsed.assemble()?;
    let captured = &model.parsed().observations().groups[0].matches[0].captures[0];
    assert!(matches!(
        captured.original,
        OriginalProjection::Ambiguous(_)
    ));
    assert_eq!(captured.native.range, 4..4);
    assert!(model.entities().is_empty());
    assert!(
        model
            .diagnostics()
            .iter()
            .any(|d| matches!(d.issue, JsonIssue::Recovery { .. })
                && matches!(d.original, Some(OriginalProjection::Ambiguous(_))))
    );
    Ok(())
}

#[test]
fn original_cancel_expiry_and_mature_decoder_errors_remain_observable() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/json.yaml"));
    let prepare_context = context();
    let factory = prepare_profile(&draft, &prepare_context)?;
    for kind in [ErrorKind::Interrupted, ErrorKind::TimedOut] {
        let original = if kind == ErrorKind::Interrupted {
            context()
        } else {
            SnapshotContext::new(
                Instant::now() + Duration::from_secs(1),
                Cancellation::default(),
            )
        };
        with_input(r#"{"a":1}"#, &original, |input| {
            let parsed =
                ParsedJsonDocument::collect(&factory, input, &["controls.json"], limits())?;
            if kind == ErrorKind::Interrupted {
                original.cancellation().cancel();
            } else {
                std::thread::sleep(Duration::from_millis(1100));
            }
            assert!(
                matches!(parsed.assemble(), Err(SemanticModelError::Operation(cause)) if cause.kind()==kind)
            );
            Ok(())
        })?;
    }
    let malformed = custom(
        "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name value:(string) @value)'\n    value_capture: value\n    value_label: Value\n",
    );
    let original = context();
    let factory = prepare_profile(&malformed, &original)?;
    with_input(r#"{"a":"\uxxxx"}"#, &original, |input| {
        let parsed = ParsedJsonDocument::collect(&factory, input, &["decode.json"], limits())?;
        let model = parsed.assemble()?;
        assert!(
            model
                .diagnostics()
                .iter()
                .any(|d| matches!(d.issue, JsonIssue::Literal(_)))
        );
        assert!(
            model
                .code_projections()?
                .iter()
                .all(|p| matches!(p, JsonProjection::Unavailable(_)))
        );
        Ok(())
    })
}

#[test]
fn native_context_queries_drive_json_names_and_report_unresolved_owners() -> TestResult {
    let original = context();
    let queries =
        "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key: (string) @name)'\n";
    let source = r#"{"outer":{"scope":1,"inner":2}}"#;
    // Context names are native captures, including a selected nested key. The
    // adapter must not silently substitute the old hard-coded key path.
    let draft = catalog(format!(
        "language: json\ngrammar: tree-sitter-json\ncontext_types: {{pair: Pair}}\ncontext_name_queries: {{pair: '(pair value: (object (pair key: (string) @name))) @context'}}\nembed_labels: [Pair]\nqueries:\n{queries}"
    ).as_bytes());
    let factory = prepare_profile(&draft, &original)?;
    with_input(source, &original, |input| {
        let parsed = ParsedJsonDocument::collect(&factory, input, &["contexts.json"], limits())?;
        let model = parsed.assemble()?;
        let inner = model
            .entities()
            .iter()
            .find(|entity| entity.name() == "inner")
            .unwrap();
        assert_eq!(inner.namespace().len(), 1);
        assert_eq!(inner.namespace()[0].name.as_ref(), "scope");
        assert_eq!(inner.namespace()[0].anchor.text(), r#""scope""#);
        let scope = model
            .entities()
            .iter()
            .find(|entity| entity.name() == "scope")
            .unwrap();
        assert!(
            scope.namespace().is_empty(),
            "a context name excludes its own scope"
        );
        assert_eq!(
            match &inner.namespace()[0].extent {
                ExtentObservation::Exact(extent) => extent.range().text(),
                _ => panic!("exact context owner"),
            },
            r#""outer":{"scope":1,"inner":2}"#
        );
        Ok(())
    })?;
    let missing = catalog(format!(
        "language: json\ngrammar: tree-sitter-json\ncontext_types: {{pair: Pair}}\ncontext_name_queries: {{pair: '(pair key: (string (escape_sequence) @name)) @context'}}\nembed_labels: [Pair]\nqueries:\n{queries}"
    ).as_bytes());
    let factory = prepare_profile(&missing, &original)?;
    with_input(source, &original, |input| {
        let parsed =
            ParsedJsonDocument::collect(&factory, input, &["missing-context.json"], limits())?;
        let model = parsed.assemble()?;
        assert!(
            model
                .diagnostics()
                .iter()
                .any(|diagnostic| matches!(diagnostic.issue, JsonIssue::InvalidParent))
        );
        assert!(
            !model
                .entities()
                .iter()
                .any(|entity| entity.name() == "inner")
        );
        Ok(())
    })?;
    let invalid = catalog(format!(
        "language: json\ngrammar: tree-sitter-json\ncontext_types: {{pair: Pair}}\ncontext_name_queries: {{pair: '(pair key: (string) @name)'}}\nqueries:\n{queries}"
    ).as_bytes());
    assert!(matches!(
        prepare_profile(&invalid, &original),
        Err(SemanticModelError::Context(_))
    ));
    Ok(())
}
