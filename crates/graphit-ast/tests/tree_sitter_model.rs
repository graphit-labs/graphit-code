use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{
        ByteSpan, OriginAnchor, OriginPlan, OriginSegment, OriginalProjection, PreparedInput,
        SyntheticReason,
    },
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_fields::ExtentObservation,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::GrammarLanguage,
    tree_sitter_model::{
        ParsedTreeSitterDocument, TreeSitterModelError, TreeSitterSemanticDocument, TsCapability,
        TsDiagnosticLocation, TsEntityDeclaredType, TsEntityExtent, TsIssue, TsOwnerSelection,
        TsProjection,
    },
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{error::Error, io::ErrorKind};
type TestResult = Result<(), Box<dyn Error>>;
fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}
fn limits() -> IdentityLimits {
    IdentityLimits::new(IdentityLimitsInput {
        source_bytes: 1 << 20,
        path_components: 16,
        path_component_bytes: 1024,
        identity_text_bytes: 32768,
        owner_depth: 64,
        uid_bytes: 1 << 20,
    })
    .unwrap()
}
fn catalog(bytes: &[u8]) -> CatalogDraft {
    let original = context();
    let id = ProfileId::new("model-fixture", 256).unwrap();
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
fn stock() -> CatalogDraft {
    catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/rust.yaml"
    ))
}
fn profile(config: &str, queries: &str) -> CatalogDraft {
    catalog(format!("language: rust\ngrammar: tree-sitter-rust\nanon_func_types: []\nembed_labels: [Struct, Field]\n{config}\nqueries:\n{queries}").as_bytes())
}
fn queries() -> &'static str {
    "  - data_key: structs\n    graph_label: Struct\n    pattern: '(struct_item name: (type_identifier) @name)'\n  - data_key: fields\n    graph_label: Field\n    pattern: '(field_declaration name: (field_identifier) @name)'\n"
}
fn rust() -> Result<GrammarLanguage, graphit_ast::tree_sitter_engine::EngineError> {
    GrammarLanguage::from_language(tree_sitter_rust::LANGUAGE.into())
}
fn with_source(
    text: &str,
    original: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    let span = ByteSpan {
        start: 0,
        end: u64::try_from(text.len())?,
    };
    let segments = if text.is_empty() {
        vec![]
    } else {
        vec![OriginSegment::Direct {
            prepared: span,
            original: span,
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
fn inspect(
    text: &str,
    draft: &CatalogDraft,
    action: impl FnOnce(&TreeSitterSemanticDocument<'_, '_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    let original = context();
    with_source(text, &original, |input| {
        let factory =
            PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &original)?;
        let parsed =
            ParsedTreeSitterDocument::collect(&factory, input, &["src", "model.rs"], limits())?;
        drop(factory);
        let model = parsed.assemble()?;
        action(&model)
    })
}

#[test]
fn stock_native_struct_field_raw_names_types_extents_and_uid() -> TestResult {
    let text = "/// doc\r\n#[derive(Clone)]\r\npub struct r#Type<'a> {\r\n #[cfg(feature = \"x\")]\r\n pub(crate) r#type: &'a [u8; 3],\r\n value: Option<(u32, bool)>,\r\n cb: fn(&str) -> usize,\r\n}\r\n";
    inspect(text, &stock(), |model| {
        assert!(model.diagnostics()?.is_empty());
        let entities = model.entities()?;
        assert_eq!(entities.len(), 4);
        let structure = entities
            .iter()
            .find(|e| e.identity().label() == "Struct")
            .unwrap();
        assert_eq!(structure.name(), "Type");
        assert_eq!(structure.name_raw().text(), "r#Type");
        assert_eq!(
            structure.name_raw().text().as_ptr(),
            text[structure.name_raw().start()..].as_ptr()
        );
        assert!(matches!(
            structure.declared_type(),
            TsEntityDeclaredType::NotApplicable
        ));
        let TsEntityExtent::Exact(extent) = structure.extent() else {
            panic!("structure extent")
        };
        assert!(extent.range().text().starts_with("pub struct"));
        assert!(extent.range().text().ends_with('}'));
        assert!(!extent.range().text().contains("derive"));
        let field = entities.iter().find(|e| e.name() == "type").unwrap();
        assert_eq!(field.identity().label(), "Field");
        assert_eq!(field.name_raw().text(), "r#type");
        assert_eq!(field.namespaces()[0].name, "Type");
        let TsEntityExtent::Exact(extent) = field.extent() else {
            panic!("field extent")
        };
        assert_eq!(extent.range().text(), "pub(crate) r#type: &'a [u8; 3]");
        let TsEntityDeclaredType::Present(ty) = field.declared_type() else {
            panic!("field type")
        };
        assert_eq!(ty.raw.text(), "&'a [u8; 3]");
        assert!(matches!(ty.syntax.as_ref(), syn::Type::Reference(_)));
        assert_eq!(
            model
                .code_projections()?
                .iter()
                .filter(|p| matches!(p, TsProjection::Available(_)))
                .count(),
            4
        );
        let context = model.parsed()?.context_document()?;
        assert!(
            context
                .scopes()?
                .iter()
                .any(|s| !s.excluded_self.is_empty())
        );
        assert!(
            model
                .capabilities()?
                .iter()
                .any(|c| c.state == TsCapability::ObservedOnlyRelation)
        );
        Ok(())
    })?;
    inspect("pub struct S { pub a: u8, }", &stock(), |model| {
        let entities = model.entities()?;
        let structure = entities.iter().find(|e| e.name() == "S").unwrap();
        assert_eq!(
            structure.identity().uid().as_str(),
            "[\"ast-syntax-root-1\",[\"src\",\"model.rs\"],\"rust\",\"tree-sitter-rust\",\"Struct\",[11,12],[]]"
        );
        let field = entities.iter().find(|e| e.name() == "a").unwrap();
        assert_eq!(
            field.identity().uid().as_str(),
            "[\"ast-syntax-root-1\",[\"src\",\"model.rs\"],\"rust\",\"tree-sitter-rust\",\"Field\",[19,20],[[\"Struct\",11,12]]]"
        );
        Ok(())
    })
}

#[test]
fn native_namespaces_homonyms_and_policy_empty() -> TestResult {
    inspect(
        "mod left { struct Same { a:u8 } } mod right { struct Same { a:u8 } } fn host(){ struct Nested { x:u8 } }",
        &stock(),
        |model| {
            assert!(model.diagnostics()?.is_empty());
            let entities = model.entities()?;
            let same: Vec<_> = entities.iter().filter(|e| e.name() == "Same").collect();
            assert_eq!(same.len(), 2);
            assert_ne!(
                same[0].identity().uid().as_str(),
                same[1].identity().uid().as_str()
            );
            assert_eq!(same[0].namespaces()[0].name, "left");
            assert_eq!(same[1].namespaces()[0].name, "right");
            let field = entities.iter().find(|e| e.name() == "x").unwrap();
            assert_eq!(
                field
                    .namespaces()
                    .iter()
                    .map(|n| n.name.as_str())
                    .collect::<Vec<_>>(),
                ["host", "Nested"]
            );
            assert_eq!(field.namespaces()[0].label, "Function");
            assert_eq!(entities.len(), 7); // The same assembler also emits the real host function.
            Ok(())
        },
    )?;
    inspect(
        "struct S {a:u8}",
        &profile("context_types: {}", queries()),
        |model| {
            assert!(model.entities()?.iter().all(|e| e.namespaces().is_empty()));
            assert!(
                model
                    .entities()?
                    .iter()
                    .find(|e| e.name() == "a")
                    .unwrap()
                    .contributions()[0]
                    .container_node_index
                    .is_some()
            );
            Ok(())
        },
    )
}

#[test]
fn configured_missing_impl_stays_missing_explicit_type_is_native() -> TestResult {
    let text = "impl S { const C: () = { struct T { a:u8 } () }; }";
    inspect(
        text,
        &profile(
            "context_types: {impl_item: Struct, struct_item: Struct}",
            queries(),
        ),
        |model| {
            assert!(model.entities()?.is_empty());
            assert!(
                model
                    .diagnostics()?
                    .iter()
                    .any(|d| matches!(d.issue, TsIssue::MissingOwner { .. }))
            );
            assert!(
                model
                    .parsed()?
                    .context_document()?
                    .scopes()?
                    .iter()
                    .any(|s| !s.unresolved.is_empty())
            );
            Ok(())
        },
    )?;
    let config = "context_types: {impl_item: Struct, struct_item: Struct}\ncontext_name_queries: {impl_item: '(impl_item type: (_) @name) @context'}";
    inspect(text, &profile(config, queries()), |model| {
        assert_eq!(model.entities()?.len(), 2);
        assert!(model.diagnostics()?.is_empty());
        let structure = model.entities()?.iter().find(|e| e.name() == "T").unwrap();
        assert_eq!(structure.namespaces()[0].name, "S");
        assert_eq!(
            structure.namespaces()[0].selection,
            TsOwnerSelection::ConfiguredRustImplType
        );
        Ok(())
    })?;
    inspect(
        text,
        &profile(
            "context_types: {impl_item: Struct, struct_item: Struct}\ncontext_name_queries: {impl_item: '(impl_item type: (generic_type (type_identifier) @name)) @context'}",
            queries(),
        ),
        |model| {
            assert!(model.entities()?.is_empty());
            assert!(
                model
                    .diagnostics()?
                    .iter()
                    .any(|d| matches!(d.issue, TsIssue::MissingOwner { .. }))
            );
            Ok(())
        },
    )
}

#[test]
fn tuple_unit_enum_union_other_roles_and_local_recovery() -> TestResult {
    inspect(
        "struct Tuple(pub u8,String); struct Unit; enum E { V { a:u8 } } union U { a:u8 }",
        &stock(),
        |model| {
            assert_eq!(model.entities()?.len(), 2);
            assert!(
                model
                    .entities()?
                    .iter()
                    .all(|e| e.identity().label() == "Struct")
            );
            assert_eq!(
                model
                    .diagnostics()?
                    .iter()
                    .filter(|d| matches!(d.issue, TsIssue::UnsupportedDeclarationFamily { .. }))
                    .count(),
                2
            );
            assert!(
                model
                    .capabilities()?
                    .iter()
                    .any(|c| c.state == TsCapability::ObservedOnlyFamily)
            );
            Ok(())
        },
    )?;
    inspect(
        "struct Broken { a: } struct Fine { b:u8 }",
        &stock(),
        |model| {
            assert!(model.diagnostics()?.iter().any(|d| matches!(
                d.issue,
                TsIssue::Recovery { .. } | TsIssue::MissingDeclaredType
            )));
            let projections = model.code_projections()?;
            assert!(
                projections
                    .iter()
                    .any(|p| matches!(p,TsProjection::Available(a) if a.entity.name()=="Fine"))
            );
            assert!(
                projections
                    .iter()
                    .any(|p| matches!(p,TsProjection::Available(a) if a.entity.name()=="b"))
            );
            assert!(
                projections
                    .iter()
                    .any(|p| matches!(p, TsProjection::Unavailable(_)))
            );
            Ok(())
        },
    )?;
    inspect("", &stock(), |model| {
        assert!(model.entities()?.is_empty());
        assert!(model.code_projections()?.is_empty());
        Ok(())
    })
}

#[test]
fn local_span_association_repetition_missing_and_invalid_contributions() -> TestResult {
    let paired = "  - data_key: structs\n    graph_label: Struct\n    span_capture: span\n    pattern: '(source_file (struct_item name: (type_identifier) @name) @span (struct_item name: (type_identifier) @name) @span)'\n";
    inspect(
        "struct A; struct B;",
        &profile("context_types: {}", paired),
        |model| {
            assert_eq!(model.entities()?.len(), 2);
            assert!(model.diagnostics()?.is_empty());
            let group = &model.parsed()?.context_document()?.observations()?.groups[0];
            assert_eq!(group.matches[0].captures.len(), 4);
            for e in model.entities()? {
                assert_eq!(e.contributions()[0].span_capture_indices.len(), 1);
                assert!(matches!(e.extent(), TsEntityExtent::Exact(_)));
            }
            Ok(())
        },
    )?;
    let repeated = "  - data_key: structs\n    graph_label: Struct\n    span_capture: span\n    pattern: '(struct_item name: (type_identifier) @name @name) @span @span'\n";
    inspect(
        "struct S;",
        &profile("context_types: {}", repeated),
        |model| {
            assert!(model.diagnostics()?.is_empty());
            let entities = model.entities()?;
            assert_eq!(entities.len(), 1);
            assert_eq!(entities[0].contributions().len(), 2);
            assert!(
                entities[0]
                    .contributions()
                    .iter()
                    .all(|c| c.span_capture_indices.len() == 2)
            );
            assert!(entities[0].conflicts().is_empty());
            Ok(())
        },
    )?;
    for invalid in [
        "  - data_key: structs\n    graph_label: Struct\n    span_capture: absent\n    pattern: '(struct_item name: (type_identifier) @name)'\n",
        "  - data_key: structs\n    graph_label: Struct\n    span_capture: span\n    pattern: '(struct_item name: (type_identifier) @name body: (field_declaration_list) @span)'\n",
    ] {
        for reverse in [false, true] {
            let good = "  - data_key: structs\n    graph_label: Struct\n    pattern: '(struct_item name: (type_identifier) @name)'\n";
            let combined = if reverse {
                format!("{good}{invalid}")
            } else {
                format!("{invalid}{good}")
            };
            inspect(
                "struct S {a:u8}",
                &profile("context_types: {}", &combined),
                |model| {
                    let e = &model.entities()?[0];
                    assert_eq!(e.contributions().len(), 2);
                    assert_eq!(e.conflicts().len(), 1);
                    assert!(matches!(e.extent(), TsEntityExtent::Conflict(_)));
                    assert!(!e.diagnostic_indices().is_empty());
                    assert!(matches!(
                        &model.code_projections()?[0],
                        TsProjection::Unavailable(_)
                    ));
                    Ok(())
                },
            )?;
        }
    }
    Ok(())
}

#[test]
fn nested_span_fragments_belong_to_the_most_specific_native_candidate() -> TestResult {
    let query = "  - data_key: structs\n    graph_label: Struct\n    span_capture: span\n    pattern: '(struct_item name: (type_identifier) @name body: (field_declaration_list (field_declaration type: (array_type length: (block (struct_item name: (type_identifier) @name @span) @span))))) @span'\n";
    inspect(
        "struct Outer { a: [u8; { struct Inner; 1 }] }",
        &profile("context_types: {}", query),
        |model| {
            let observations = model.parsed()?.context_document()?.observations()?;
            assert_eq!(observations.groups[0].matches.len(), 1);
            assert_eq!(observations.groups[0].matches[0].captures.len(), 5);
            let entities = model.entities()?;
            assert_eq!(entities.len(), 2);
            let outer = entities.iter().find(|e| e.name() == "Outer").unwrap();
            let inner = entities.iter().find(|e| e.name() == "Inner").unwrap();
            assert!(outer.diagnostic_indices().is_empty());
            assert!(matches!(outer.extent(), TsEntityExtent::Exact(_)));
            assert!(matches!(inner.extent(), TsEntityExtent::Missing));
            assert_eq!(inner.contributions()[0].span_capture_indices.len(), 1);
            assert!(inner.diagnostic_indices().iter().any(|&i| matches!(
                model.diagnostics().unwrap()[i].issue,
                TsIssue::InvalidSpan { .. }
            )));
            let projections = model.code_projections()?;
            assert_eq!(projections.len(), 2);
            assert_eq!(
                projections
                    .iter()
                    .filter(|p| matches!(p, TsProjection::Available(_)))
                    .count(),
                1
            );
            assert_eq!(
                projections
                    .iter()
                    .filter(|p| matches!(p, TsProjection::Unavailable(_)))
                    .count(),
                1
            );
            Ok(())
        },
    )
}

#[test]
fn empty_profile_and_query_configuration_diagnostics_have_real_locations() -> TestResult {
    let normalizers = "context_types: {}\ntext_normalizers: {string_literal: {replace: {x: y}}}";
    inspect("struct S;", &profile(normalizers, "  []"), |model| {
        assert!(model.entities()?.is_empty());
        assert!(model.capabilities()?.is_empty());
        assert!(
            model
                .parsed()?
                .context_document()?
                .observations()?
                .groups
                .is_empty()
        );
        let diagnostics = model.diagnostics()?;
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].location, TsDiagnosticLocation::Profile);
        assert!(matches!(
            diagnostics[0].issue,
            TsIssue::UnsupportedProfileNormalization
        ));
        Ok(())
    })?;
    let query = "  - data_key: structs\n    graph_label: Struct\n    name_reject: '^S$'\n    pattern: '(struct_item name: (type_identifier) @name)'\n";
    inspect("", &profile("context_types: {}", query), |model| {
        assert!(model.entities()?.is_empty());
        let observations = model.parsed()?.context_document()?.observations()?;
        assert!(observations.groups[0].matches.is_empty());
        let diagnostics = model.diagnostics()?;
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].location,
            TsDiagnosticLocation::Query { query_index: 0 }
        );
        assert!(matches!(
            diagnostics[0].issue,
            TsIssue::UnsupportedConfiguration(_)
        ));
        Ok(())
    })
}

#[test]
fn native_mapped_name_merge_extent_and_declared_type_conflicts() -> TestResult {
    let original = context();
    let source = "struct S { a:i32, b:u64 }";
    let prepared = "struct S { a:i32, a:u64 }";
    let b = source.find("b:").unwrap() as u64;
    let a = source.find("a:").unwrap() as u64;
    let fragments = [ByteSpan {
        start: a,
        end: a + 1,
    }];
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan { start: 0, end: b },
            original: ByteSpan { start: 0, end: b },
        },
        OriginSegment::MappedAtom {
            prepared: ByteSpan {
                start: b,
                end: b + 1,
            },
            original_fragments: &fragments,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: b + 1,
                end: prepared.len() as u64,
            },
            original: ByteSpan {
                start: b + 1,
                end: source.len() as u64,
            },
        },
    ];
    let removed = [ByteSpan {
        start: b,
        end: b + 1,
    }];
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
    assert!(
        matches!(input.project_prepared(b,b+1)?,OriginalProjection::Exact(r) if r.text()=="a" && r.start()==a as usize)
    );
    let draft = stock();
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &original)?;
    let parsed =
        ParsedTreeSitterDocument::collect(&factory, &input, &["src", "mapped.rs"], limits())?;
    let model = parsed.assemble()?;
    let entities = model.entities()?;
    let field = entities
        .iter()
        .find(|e| e.identity().label() == "Field")
        .unwrap();
    assert_eq!(field.name(), "a");
    assert_eq!(field.contributions().len(), 2);
    assert_eq!(field.contributions()[0].name_capture.prepared.text(), "a");
    assert_eq!(field.contributions()[1].name_capture.prepared.text(), "a");
    assert!(matches!(
        &field.contributions()[0].extent,
        ExtentObservation::Exact(_)
    ));
    assert!(matches!(
        &field.contributions()[1].extent,
        ExtentObservation::NonExact(OriginalProjection::Compound(_))
    ));
    assert_eq!(field.conflicts().len(), 1);
    assert_eq!(field.model_conflicts().len(), 1);
    assert_eq!(field.model_conflicts()[0].first_contribution, 0);
    assert_eq!(field.model_conflicts()[0].second_contribution, 1);
    assert!(matches!(
        field.declared_type(),
        TsEntityDeclaredType::Conflict(_)
    ));
    assert!(
        model
            .code_projections()?
            .iter()
            .all(|p| matches!(p, TsProjection::Unavailable(_)))
    );
    let duplicate = format!(
        "{}{}",
        queries(),
        "  - data_key: alt_fields\n    graph_label: Field\n    pattern: '(field_declaration name: (field_identifier) @name)'\n"
    );
    inspect(
        "struct S {a:u8}",
        &profile("context_types: {struct_item: Struct}", &duplicate),
        |model| {
            let fields: Vec<_> = model
                .entities()?
                .iter()
                .filter(|e| e.identity().label() == "Field")
                .collect();
            assert_eq!(fields.len(), 2);
            assert_eq!(
                fields[0].identity().uid().as_str(),
                fields[1].identity().uid().as_str()
            );
            assert_ne!(fields[0].data_key(), fields[1].data_key());
            Ok(())
        },
    )
}

#[test]
fn genuine_prepared_input_synthetic_and_partial_atom_keep_native_causes() -> TestResult {
    let text = "struct S { a:u8 }";
    let name = text.find("a:").unwrap() as u64;
    let end = text.len() as u64;
    let original = context();
    let draft = stock();
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &original)?;
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: name,
            },
            original: ByteSpan {
                start: 0,
                end: name,
            },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: name,
                end: name + 1,
            },
            reason: SyntheticReason::InsertedByTransform,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: name + 1,
                end,
            },
            original: ByteSpan {
                start: name + 1,
                end,
            },
        },
    ];
    let removed = [ByteSpan {
        start: name,
        end: name + 1,
    }];
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let parsed =
        ParsedTreeSitterDocument::collect(&factory, &input, &["src", "synthetic.rs"], limits())?;
    let model = parsed.assemble()?;
    assert_eq!(model.entities()?.len(), 1);
    assert!(
        model
            .diagnostics()?
            .iter()
            .any(|d| matches!(d.original, Some(OriginalProjection::Synthetic(_))))
    );
    assert!(matches!(
        &model.code_projections()?[0],
        TsProjection::Unavailable(_)
    ));
    assert!(
        model
            .parsed()?
            .context_document()?
            .scopes()?
            .iter()
            .any(|s| !s.contexts.is_empty())
    );
    let fragments = [ByteSpan {
        start: name,
        end: name + 4,
    }];
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: name,
            },
            original: ByteSpan {
                start: 0,
                end: name,
            },
        },
        OriginSegment::MappedAtom {
            prepared: ByteSpan {
                start: name,
                end: name + 4,
            },
            original_fragments: &fragments,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: name + 4,
                end,
            },
            original: ByteSpan {
                start: name + 4,
                end,
            },
        },
    ];
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let parsed =
        ParsedTreeSitterDocument::collect(&factory, &input, &["src", "partial.rs"], limits())?;
    let model = parsed.assemble()?;
    assert_eq!(model.entities()?.len(), 1);
    assert!(
        model
            .diagnostics()?
            .iter()
            .any(|d| matches!(d.original, Some(OriginalProjection::Unmappable(_))))
    );
    Ok(())
}

#[test]
fn original_cancelled_getters_factory_and_limits_remain_causal() -> TestResult {
    let original = context();
    let draft = stock();
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &original)?;
    with_source("struct S;", &original, |input| {
        let parsed =
            ParsedTreeSitterDocument::collect(&factory, input, &["src", "model.rs"], limits())?;
        let model = parsed.assemble()?;
        original.cancellation().cancel();
        assert!(
            matches!(model.entities(),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        assert!(
            matches!(model.code_projections(),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        assert!(
            matches!(parsed.source(),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        assert!(
            matches!(parsed.assemble(),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        assert!(
            matches!(ParsedTreeSitterDocument::collect(&factory,input,&["src","model.rs"],limits()),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        Ok(())
    })?;
    let original = context();
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &original)?;
    with_source("struct S;", &original, |input| {
        let bounded = IdentityLimits::new(IdentityLimitsInput {
            source_bytes: 1,
            path_components: 1,
            path_component_bytes: 1,
            identity_text_bytes: 1,
            owner_depth: 1,
            uid_bytes: 1,
        })?;
        assert!(matches!(
            ParsedTreeSitterDocument::collect(&factory, input, &["src"], bounded),
            Err(TreeSitterModelError::Range(_))
        ));
        Ok(())
    })
}

#[test]
fn unsupported_config_normalizers_and_embed_policy_are_explicit() -> TestResult {
    let bad = "  - data_key: structs\n    graph_label: Struct\n    name_reject: '^S$'\n    pattern: '(struct_item name: (type_identifier) @name)'\n";
    inspect("struct S;", &profile("context_types: {}", bad), |model| {
        assert!(model.entities()?.is_empty());
        assert!(
            matches!(&model.capabilities()?[0].state,TsCapability::UnsupportedConfiguration(fields) if fields==&["name_reject"])
        );
        assert!(
            model
                .diagnostics()?
                .iter()
                .any(|d| matches!(d.issue, TsIssue::UnsupportedConfiguration(_)))
        );
        assert_eq!(
            model.parsed()?.context_document()?.observations()?.groups[0]
                .matches
                .len(),
            1
        );
        Ok(())
    })?;
    let normalizers = "context_types: {}\ntext_normalizers: {string_literal: {replace: {x: y}}}";
    inspect(
        "struct S {a:u8}",
        &profile(normalizers, queries()),
        |model| {
            assert!(model.entities()?.is_empty());
            assert!(
                model
                    .capabilities()?
                    .iter()
                    .all(|c| c.state == TsCapability::UnsupportedProfileNormalization)
            );
            assert!(
                model
                    .diagnostics()?
                    .iter()
                    .all(|d| matches!(d.issue, TsIssue::UnsupportedProfileNormalization))
            );
            assert_eq!(
                model
                    .parsed()?
                    .context_document()?
                    .observations()?
                    .groups
                    .len(),
                2
            );
            Ok(())
        },
    )?;
    let raw = format!(
        "language: rust\ngrammar: tree-sitter-rust\ncontext_types: {{struct_item: Struct}}\nanon_func_types: []\nembed_labels: [Struct]\nqueries:\n{}",
        queries()
    );
    inspect("struct S{a:u8}", &catalog(raw.as_bytes()), |model| {
        assert_eq!(model.entities()?.len(), 2);
        assert_eq!(model.code_projections()?.len(), 1);
        assert!(
            matches!(&model.code_projections()?[0],TsProjection::Available(a) if a.entity.identity().label()=="Struct")
        );
        Ok(())
    })
}

#[test]
fn origin_plan_can_drop_and_deadline_checks_survive_assembly() -> TestResult {
    use std::time::{Duration, Instant};
    let original = context();
    let draft = stock();
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &original)?;
    let source = String::from("struct S {a:u8}");
    let parsed = {
        let bound = ByteSpan {
            start: 0,
            end: source.len() as u64,
        };
        let segments = [OriginSegment::Direct {
            prepared: bound,
            original: bound,
        }];
        let input = PreparedInput::new(
            source.as_bytes(),
            source.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &[],
                anchors: &[],
            },
            &original,
        )?;
        ParsedTreeSitterDocument::collect(&factory, &input, &["src", "model.rs"], limits())?
    };
    drop(factory);
    assert_eq!(parsed.assemble()?.entities()?.len(), 2);
    let request = SnapshotContext::new(
        Instant::now() + Duration::from_secs(1),
        Cancellation::default(),
    );
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &request)?;
    with_source("struct S;", &request, |input| {
        let parsed =
            ParsedTreeSitterDocument::collect(&factory, input, &["src", "model.rs"], limits())?;
        let model = parsed.assemble()?;
        std::thread::sleep(Duration::from_millis(1100));
        assert!(
            matches!(model.entities(),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::TimedOut)
        );
        assert!(
            matches!(model.diagnostics(),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::TimedOut)
        );
        assert!(
            matches!(model.code_projections(),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::TimedOut)
        );
        Ok(())
    })
}

#[test]
fn sdk_missing_delimiter_exposes_ambiguous_original_anchor() -> TestResult {
    let original = context();
    let prepared = "struct S {";
    let source = "struct S {XX";
    let bound = prepared.len() as u64;
    let segments = [OriginSegment::Direct {
        prepared: ByteSpan {
            start: 0,
            end: bound,
        },
        original: ByteSpan {
            start: 0,
            end: bound,
        },
    }];
    let removed = [ByteSpan {
        start: bound,
        end: source.len() as u64,
    }];
    let candidates = [bound, source.len() as u64];
    let anchors = [OriginAnchor {
        prepared: bound,
        original_candidates: &candidates,
    }];
    let input = PreparedInput::new(
        source.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &removed,
            anchors: &anchors,
        },
        &original,
    )?;
    let draft = profile(
        "context_types: {struct_item: Struct}",
        "  - data_key: missing\n    graph_label: Field\n    pattern: '(MISSING \"}\") @name'\n",
    );
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &original)?;
    let parsed =
        ParsedTreeSitterDocument::collect(&factory, &input, &["src", "missing.rs"], limits())?;
    let model = parsed.assemble()?;
    assert!(model.entities()?.is_empty());
    let native = parsed.context_document()?.observations()?;
    assert_eq!(native.groups[0].matches.len(), 1);
    assert_eq!(
        native.groups[0].matches[0].captures[0].native.range,
        prepared.len()..prepared.len()
    );
    assert!(
        matches!(&native.groups[0].matches[0].captures[0].original,OriginalProjection::Ambiguous(r) if r.len()==2)
    );
    assert!(
        model
            .diagnostics()?
            .iter()
            .any(|d| matches!(d.original, Some(OriginalProjection::Ambiguous(_))))
    );
    Ok(())
}

#[test]
fn original_type_decoder_error_and_configured_raw_namespace_are_observable() -> TestResult {
    let original = context();
    let source = "struct S { a:123 }";
    let prepared = "struct S { a:u8 }";
    let start = source.find("123").unwrap() as u64;
    let fragments = [ByteSpan {
        start,
        end: start + 3,
    }];
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: start,
            },
            original: ByteSpan {
                start: 0,
                end: start,
            },
        },
        OriginSegment::MappedAtom {
            prepared: ByteSpan {
                start,
                end: start + 2,
            },
            original_fragments: &fragments,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: start + 2,
                end: prepared.len() as u64,
            },
            original: ByteSpan {
                start: start + 3,
                end: source.len() as u64,
            },
        },
    ];
    let input = PreparedInput::new(
        source.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let draft = stock();
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &original)?;
    let parsed =
        ParsedTreeSitterDocument::collect(&factory, &input, &["src", "invalid-type.rs"], limits())?;
    let model = parsed.assemble()?;
    let field = model
        .entities()?
        .iter()
        .find(|e| e.identity().label() == "Field")
        .unwrap();
    assert!(matches!(
        field.declared_type(),
        TsEntityDeclaredType::Unavailable
    ));
    assert!(
        model
            .diagnostics()?
            .iter()
            .any(|d| matches!(d.issue, TsIssue::DeclaredType(_))
                && matches!(d.original,Some(OriginalProjection::Exact(r)) if r.text()=="123"))
    );
    let index = model
        .entities()?
        .iter()
        .position(|e| e.name() == "a")
        .unwrap();
    assert!(
        model
            .code_projections()?
            .iter()
            .any(|p| matches!(p,TsProjection::Unavailable(u) if u.entity_index==index))
    );
    inspect(
        "struct r#type { r#match:u8 }",
        &profile("context_types: {struct_item: Record}", queries()),
        |model| {
            assert!(model.diagnostics()?.is_empty());
            let field = model
                .entities()?
                .iter()
                .find(|e| e.identity().label() == "Field")
                .unwrap();
            assert_eq!(field.name(), "match");
            assert_eq!(field.name_raw().text(), "r#match");
            assert_eq!(field.namespaces()[0].label, "Record");
            assert_eq!(field.namespaces()[0].name, "type");
            assert_eq!(field.namespaces()[0].anchor.text(), "r#type");
            Ok(())
        },
    )
}

fn callable_details<'a, 'doc, 'o, 'v, 'request>(
    entity: &'a graphit_ast::tree_sitter_model::TsEntity<'doc, 'o, 'v, 'request>,
) -> &'a graphit_ast::tree_sitter_model::TsFunctionDetails<'doc, 'o, 'v> {
    match entity.callable() {
        graphit_ast::tree_sitter_model::TsEntityCallable::Function(d) => d,
        _ => panic!("expected native callable details"),
    }
}
fn callable_profile(config: &str, signature: bool) -> CatalogDraft {
    let signatures = if signature {
        "  - data_key: functions\n    graph_label: Function\n    pattern: '(function_signature_item name: (identifier) @name)'\n"
    } else {
        ""
    };
    let raw = format!(
        "language: rust\ngrammar: tree-sitter-rust\n{config}\nanon_func_types: []\nembed_labels: [Function, Method]\nqueries:\n  - data_key: functions\n    graph_label: Function\n    pattern: '(function_item name: (identifier) @name)'\n{signatures}"
    );
    catalog(raw.as_bytes())
}

#[test]
fn native_functions_signatures_methods_stock_owners_and_defaultness() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsCallableKind, TsComplexity, TsExport};
    use syn::ext::IdentExt;
    let source = "pub fn r#type<'a,T:Copy>(x:&'a T)->u8 where T:Sync {1}\r\nstruct S; impl<T> crate::S<T> { pub fn run(&self, x:&T)->u8 {1} pub(crate) fn make()->Self {loop{}} default fn step(&self){} } trait Tr { fn required(&self)->u8; fn provided(&self)->u8 {1} } pub unsafe extern \"C\" fn abi(x:i32,...)->i32{x} async fn async_fn((x,y):(u8,bool)){} const fn const_fn()->u8{1}";
    inspect(source, &stock(), |model| {
        assert!(model.diagnostics()?.is_empty());
        let entities = model.entities()?;
        let free = entities.iter().find(|e| e.name() == "type").unwrap();
        assert_eq!(free.name_raw().text(), "r#type");
        assert_eq!(
            free.identity().uid().as_str(),
            r#"["ast-syntax-root-1",["src","model.rs"],"rust","tree-sitter-rust","Function",[7,13],[]]"#
        );
        assert!(matches!(
            free.declared_type(),
            TsEntityDeclaredType::NotApplicable
        ));
        let d = callable_details(free);
        assert_eq!(d.kind, TsCallableKind::Function);
        assert_eq!(
            d.signature.raw.unwrap().text(),
            "fn r#type<'a,T:Copy>(x:&'a T)->u8 where T:Sync "
        );
        let sig = d.signature.syntax.as_ref().unwrap();
        assert_eq!(sig.ident.unraw().to_string(), "type");
        assert_eq!(sig.inputs.len(), 1);
        assert_eq!(sig.generics.params.len(), 2);
        assert!(sig.generics.where_clause.is_some());
        assert!(matches!(sig.output, syn::ReturnType::Type(_, _)));
        assert_eq!(d.export, TsExport::Modifier(true));
        for (name, kind) in [
            ("run", TsCallableKind::ImplMethod),
            ("make", TsCallableKind::ImplMethod),
            ("step", TsCallableKind::ImplMethod),
            ("provided", TsCallableKind::TraitMethod),
        ] {
            let e = entities.iter().find(|e| e.name() == name).unwrap();
            assert_eq!(e.identity().label(), "Function");
            assert_eq!(callable_details(e).kind, kind);
            assert!(callable_details(e).signature.syntax.is_some());
            if kind == TsCallableKind::ImplMethod {
                assert_eq!(e.namespaces()[0].name, "crate::S<T>");
                assert_eq!(e.namespaces()[0].anchor.text(), "crate::S<T>");
                assert_eq!(
                    e.namespaces()[0].selection,
                    TsOwnerSelection::ConfiguredRustImplType
                );
            }
        }
        let abi = callable_details(entities.iter().find(|e| e.name() == "abi").unwrap())
            .signature
            .syntax
            .as_ref()
            .unwrap();
        assert!(abi.unsafety.is_some());
        assert!(abi.variadic.is_some());
        assert_eq!(
            abi.abi.as_ref().unwrap().name.as_ref().unwrap().value(),
            "C"
        );
        let asynchronous =
            callable_details(entities.iter().find(|e| e.name() == "async_fn").unwrap())
                .signature
                .syntax
                .as_ref()
                .unwrap();
        assert!(asynchronous.asyncness.is_some());
        assert!(asynchronous.inputs.first().is_some_and(
            |p| matches!(p,syn::FnArg::Typed(arg) if matches!(arg.pat.as_ref(),syn::Pat::Tuple(_)))
        ));
        assert!(
            callable_details(entities.iter().find(|e| e.name() == "const_fn").unwrap())
                .signature
                .syntax
                .as_ref()
                .unwrap()
                .constness
                .is_some()
        );
        let step = callable_details(entities.iter().find(|e| e.name() == "step").unwrap());
        assert!(step.defaultness_node_index.is_some());
        assert_eq!(step.signature.raw.unwrap().text(), "default fn step(&self)");
        assert!(matches!(
            step.complexity,
            TsComplexity::Score { value: 1, .. }
        ));
        let make = callable_details(entities.iter().find(|e| e.name() == "make").unwrap());
        assert_eq!(make.export, TsExport::Modifier(true));
        assert!(matches!(
            make.visibility.syntax,
            Some(syn::Visibility::Restricted(_))
        ));
        assert!(make.signature.syntax.as_ref().unwrap().receiver().is_none());
        assert!(!entities.iter().any(|e| e.name() == "required"));
        assert_eq!(
            model
                .code_projections()?
                .iter()
                .filter(|p| matches!(p, TsProjection::Available(_)))
                .count(),
            9
        ); // Struct S plus eight callables.
        Ok(())
    })?;
    let config = "context_types: {impl_item: Struct, function_item: Function}";
    inspect(
        "impl S{fn m(&self){}}",
        &callable_profile(config, false),
        |model| {
            assert!(model.entities()?.is_empty());
            assert!(
                model
                    .diagnostics()?
                    .iter()
                    .any(|d| matches!(d.issue, TsIssue::MissingOwner { .. }))
            );
            Ok(())
        },
    )?;
    let config = "context_types: {trait_item: Trait}\ncomplexity: {node_types: [if_expression], operators: []}";
    inspect(
        "trait Tr{fn req(&self)->u8;} unsafe extern \"C\"{fn foreign(x:u8)->u8;}",
        &callable_profile(config, true),
        |model| {
            assert!(model.diagnostics()?.is_empty());
            assert_eq!(model.entities()?.len(), 2);
            for e in model.entities()? {
                let d = callable_details(e);
                assert!(matches!(
                    d.kind,
                    TsCallableKind::TraitSignature | TsCallableKind::ForeignFunction
                ));
                assert!(d.body_node_index.is_none());
                assert_eq!(d.complexity, TsComplexity::NoBody);
                assert!(
                    matches!(e.extent(), TsEntityExtent::Exact(x) if x.range().text().ends_with(';'))
                );
            }
            Ok(())
        },
    )
}

#[test]
fn callable_native_docs_attributes_inner_placement_and_unsupported_expression() -> TestResult {
    use graphit_ast::tree_sitter_model::{
        TsAttachmentContent, TsAttachmentPlacement, TsCallableIssueKind,
    };
    let source = "// ordinary\n/// leading\r\n/** block */\n#[inline]\n#[doc = \"escaped\\ntext\"]\n#[doc = r#\" raw body \"#]\npub fn docs(){ //! inner\n #![doc = \" body\"]\n}\n/// belongs to Stop\nstruct Stop;\nfn after(){}\n#[doc=include_str!(\"file\")]\nfn external(){}";
    inspect(source, &stock(), |model| {
        let entities = model.entities()?;
        let docs = entities.iter().find(|e| e.name() == "docs").unwrap();
        let d = callable_details(docs);
        let texts: Vec<_> = d
            .attachments
            .iter()
            .filter_map(|a| {
                if let TsAttachmentContent::Documentation(text) = &a.content {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            texts,
            [
                " leading",
                " block ",
                "escaped\ntext",
                " raw body ",
                " inner",
                " body"
            ]
        );
        assert_eq!(
            d.attachments
                .iter()
                .filter(|a| a.placement == TsAttachmentPlacement::Inner)
                .count(),
            2
        );
        assert!(matches!(
            d.attachments[0].content,
            TsAttachmentContent::Comment
        ));
        assert!(d.issues.is_empty());
        assert!(
            matches!(docs.extent(),TsEntityExtent::Exact(x) if x.range().text().starts_with("pub fn docs"))
        );
        assert!(
            callable_details(entities.iter().find(|e| e.name() == "after").unwrap())
                .attachments
                .is_empty()
        );
        let external = entities.iter().find(|e| e.name() == "external").unwrap();
        assert!(callable_details(external).issues.iter().any(|i| matches!(
            i.kind,
            TsCallableIssueKind::UnsupportedDocumentationExpression
        )));
        assert!(!external.diagnostic_indices().is_empty());
        let projections = model.code_projections()?;
        assert!(projections.iter().any(|p|matches!(p,TsProjection::Unavailable(u) if u.entity_index==entities.iter().position(|e|e.name()=="external").unwrap())));
        Ok(())
    })
}

#[test]
fn callable_complexity_uses_native_nodes_and_export_visibility() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsCallableKind, TsComplexity, TsExport};
    let source = "fn one(){} fn three(){if true{}while false{}} fn five(){if true{}for x in []{}while false{}if false{}} fn logic(){if a && b {}} fn outer(){fn inner(){if true{}while false{}}if true{}} fn arms(x:u8){match x{0=>{},_=>{}}} fn r#pub(){let _=\"pub && || if\";} pub fn visible(){} pub(crate) fn crate_visible(){}";
    inspect(source, &stock(), |model| {
        assert!(model.diagnostics()?.is_empty());
        let entities = model.entities()?;
        for (name, score) in [
            ("one", 1),
            ("three", 3),
            ("five", 5),
            ("logic", 3),
            ("outer", 2),
            ("inner", 3),
            ("arms", 3),
            ("pub", 1),
        ] {
            let d = callable_details(entities.iter().find(|e| e.name() == name).unwrap());
            assert!(
                matches!(d.complexity,TsComplexity::Score{value,..} if value==score),
                "{name}"
            );
            assert_eq!(d.kind, TsCallableKind::Function);
        }
        for (name, exported) in [("pub", false), ("visible", true), ("crate_visible", true)] {
            assert_eq!(
                callable_details(entities.iter().find(|e| e.name() == name).unwrap()).export,
                TsExport::Modifier(exported)
            );
        }
        Ok(())
    })?;
    inspect(
        "fn f(){if true{}}",
        &callable_profile("context_types: {}", false),
        |model| {
            assert_eq!(
                callable_details(&model.entities()?[0]).complexity,
                TsComplexity::Disabled
            );
            Ok(())
        },
    )?;
    inspect(
        "fn f(){}",
        &callable_profile(
            "context_types: {}\ncomplexity: {head_calls: {node_type: call_expression, names: [if]}}",
            false,
        ),
        |model| {
            assert_eq!(
                callable_details(&model.entities()?[0]).complexity,
                TsComplexity::Unavailable
            );
            assert!(!model.entities()?[0].diagnostic_indices().is_empty());
            Ok(())
        },
    )
}

#[test]
fn relation_observations_bind_real_nested_callers_and_decorator_owners() -> TestResult {
    use graphit_ast::tree_sitter_model::TsRelationSourceState;
    inspect(
        "#[inline] fn outer(){callee(); ns::callee(); obj.m(); obj.field; #[inline] fn inner(){callee();}} impl Tr for S{}",
        &stock(),
        |model| {
            let entities = model.entities()?;
            let mut calls = 0;
            let mut inner_calls = 0;
            let mut decorators = 0;
            let mut implements = 0;
            for relation in model.relations()? {
                assert!(std::ptr::eq(
                    relation.matched,
                    &model.parsed()?.context_document()?.observations()?.groups
                        [relation.query_index]
                        .matches[relation.match_index]
                ));
                match relation.query.value().relation_type().value().as_str() {
                    "CALLS" => {
                        calls += 1;
                        assert!(relation.target_rule.is_some());
                        for source in &relation.sources {
                            let TsRelationSourceState::Bound { entity_indices, .. } = &source.state
                            else {
                                panic!("missing real caller")
                            };
                            assert_eq!(entity_indices.len(), 1);
                            if entities[entity_indices[0]].name() == "inner" {
                                inner_calls += 1;
                            } else {
                                assert_eq!(entities[entity_indices[0]].name(), "outer");
                            }
                        }
                    }
                    "DECORATOR" => {
                        decorators += 1;
                        assert!(
                            matches!(&relation.sources[0].state,TsRelationSourceState::Bound{entity_indices,..} if entities[entity_indices[0]].name()==if decorators==1 {"outer"}else{"inner"})
                        );
                    }
                    "IMPLEMENTS" => {
                        implements += 1;
                        assert!(matches!(
                            relation.sources[0].state,
                            TsRelationSourceState::MissingContainer
                        ));
                    }
                    _ => {}
                }
            }
            assert_eq!((calls, inner_calls, decorators, implements), (4, 1, 2, 1));
            assert!(
                model
                    .capabilities()?
                    .iter()
                    .any(|c| c.state == TsCapability::ObservedOnlyRelation)
            );
            Ok(())
        },
    )
}

#[test]
fn callable_repeated_capture_cache_conflicts_and_local_failure_are_causal() -> TestResult {
    use graphit_ast::tree_sitter_model::{
        TsCallable, TsCallableIssueKind, TsEntityCallable, TsModelConflictField,
        TsRelationSourceState,
    };
    let raw = "language: rust\ngrammar: tree-sitter-rust\ncontext_types: {}\nembed_labels: [Function]\nqueries:\n  - data_key: functions\n    graph_label: Function\n    pattern: '(function_item name: (identifier) @name @name)'\n";
    inspect("fn a(){}", &catalog(raw.as_bytes()), |model| {
        let c = model.entities()?[0].contributions();
        assert_eq!(c.len(), 2);
        let (TsCallable::Function(a), TsCallable::Function(b)) = (&c[0].callable, &c[1].callable)
        else {
            panic!("missing callable")
        };
        assert!(std::rc::Rc::ptr_eq(a, b));
        assert!(model.diagnostics()?.is_empty());
        Ok(())
    })?;
    let original = context();
    let draft = stock();
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &original)?;
    let source = "fn a(x:i32){} fn b(x:u64){}";
    let prepared = "fn a(x:i32){} fn a(x:u64){}";
    let b = source.find("b(").unwrap() as u64;
    let a = source.find("a(").unwrap() as u64;
    let original_fragments = [ByteSpan {
        start: a,
        end: a + 1,
    }];
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan { start: 0, end: b },
            original: ByteSpan { start: 0, end: b },
        },
        OriginSegment::MappedAtom {
            prepared: ByteSpan {
                start: b,
                end: b + 1,
            },
            original_fragments: &original_fragments,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: b + 1,
                end: prepared.len() as u64,
            },
            original: ByteSpan {
                start: b + 1,
                end: source.len() as u64,
            },
        },
    ];
    let removed = [ByteSpan {
        start: b,
        end: b + 1,
    }];
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
    let parsed =
        ParsedTreeSitterDocument::collect(&factory, &input, &["src", "model.rs"], limits())?;
    let model = parsed.assemble()?;
    assert_eq!(model.entities()?.len(), 1);
    let e = &model.entities()?[0];
    assert_eq!(e.contributions().len(), 2);
    assert!(matches!(e.callable(), TsEntityCallable::Conflict(_)));
    assert!(matches!(
        e.declared_type(),
        TsEntityDeclaredType::NotApplicable
    ));
    assert!(
        e.model_conflicts()
            .iter()
            .any(|c| c.field == TsModelConflictField::Callable
                && c.first_contribution == 0
                && c.second_contribution == 1)
    );
    assert!(matches!(
        model.code_projections()?[0],
        TsProjection::Unavailable(_)
    ));
    inspect(
        "default fn bad(){} fn broken(){callee();let x=;} fn fine(){}",
        &stock(),
        |model| {
            let es = model.entities()?;
            assert!(
                callable_details(es.iter().find(|e| e.name() == "bad").unwrap())
                    .issues
                    .iter()
                    .any(|i| matches!(i.kind, TsCallableIssueKind::Signature(_)))
            );
            let fine = es.iter().position(|e| e.name() == "fine").unwrap();
            assert!(
                model
                    .code_projections()?
                    .iter()
                    .any(|p| matches!(p,TsProjection::Available(a)if a.entity_index==fine))
            );
            assert!(
                matches!(&model.relations()?[0].sources[0].state,TsRelationSourceState::Bound{entity_indices,..} if es[entity_indices[0]].name()=="broken"&&!es[entity_indices[0]].diagnostic_indices().is_empty())
            );
            Ok(())
        },
    )?;
    inspect(
        "impl S{fn m(&self){callee();}}",
        &callable_profile("context_types: {impl_item: Struct}", false),
        |model| {
            assert!(model.entities()?.is_empty());
            Ok(())
        },
    )?;
    let policy = "language: rust\ngrammar: tree-sitter-rust\ncontext_types: {impl_item: Struct}\nembed_labels: [Function]\nqueries:\n  - data_key: functions\n    graph_label: Function\n    pattern: '(function_item name: (identifier) @name)'\n  - type: relation\n    data_key: calls\n    relation_type: CALLS\n    pattern: '(call_expression function: (identifier) @name)'\n";
    inspect(
        "impl S{fn m(&self){callee();}}",
        &catalog(policy.as_bytes()),
        |model| {
            assert!(matches!(
                model.relations()?[0].sources[0].state,
                TsRelationSourceState::UnavailableContainer { .. }
            ));
            Ok(())
        },
    )?;
    original.cancellation().cancel();
    assert!(
        matches!(model.relations(),Err(TreeSitterModelError::Operation(e))if e.kind()==ErrorKind::Interrupted)
    );
    Ok(())
}

#[test]
fn callable_explicit_method_label_and_unsupported_export_are_observable() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsCallableIssueKind, TsExport};
    let policy = "language: rust\ngrammar: tree-sitter-rust\ncontext_types: {impl_item: Struct}\ncontext_name_queries: {impl_item: '(impl_item type: (_) @name) @context'}\nembed_labels: [Method]\nqueries:\n  - data_key: methods\n    graph_label: Method\n    pattern: '(function_item name: (identifier) @name)'\n";
    inspect(
        "fn free(){} impl S{fn m(&self){}}",
        &catalog(policy.as_bytes()),
        |model| {
            assert_eq!(model.entities()?.len(), 1);
            assert_eq!(model.entities()?[0].identity().label(), "Method");
            assert!(
                model
                    .diagnostics()?
                    .iter()
                    .any(|d| matches!(d.issue, TsIssue::UnsupportedDeclarationFamily { .. }))
            );
            assert!(matches!(
                model.code_projections()?[0],
                TsProjection::Available(_)
            ));
            Ok(())
        },
    )?;
    inspect(
        "fn f(){}",
        &callable_profile(
            "context_types: {}\nexports: {strategy: no_prefix, config: {prefix: _}}",
            false,
        ),
        |model| {
            let d = callable_details(&model.entities()?[0]);
            assert_eq!(d.export, TsExport::Unavailable);
            assert!(
                d.issues
                    .iter()
                    .any(|i| matches!(i.kind, TsCallableIssueKind::UnsupportedExportPolicy))
            );
            Ok(())
        },
    )
}

#[test]
fn callable_attachment_mapping_cannot_hide_an_original_intervening_item() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsAttachmentContent, TsCallableIssueKind};
    let source = "/// doc\nconst X:u8=1;\nfn f(){}";
    let prepared = "/// doc\n             \nfn f(){}";
    let start = source.find("const").unwrap() as u64;
    let end = source.find("\nfn").unwrap() as u64;
    let fragments = [ByteSpan { start, end }];
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: start,
            },
            original: ByteSpan {
                start: 0,
                end: start,
            },
        },
        OriginSegment::MappedAtom {
            prepared: ByteSpan { start, end },
            original_fragments: &fragments,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: end,
                end: prepared.len() as u64,
            },
            original: ByteSpan {
                start: end,
                end: source.len() as u64,
            },
        },
    ];
    let original = context();
    let draft = stock();
    let factory = PreparedTreeSitterContexts::prepare(&draft.profiles()[0], rust()?, &original)?;
    let input = PreparedInput::new(
        source.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let parsed =
        ParsedTreeSitterDocument::collect(&factory, &input, &["src", "model.rs"], limits())?;
    let model = parsed.assemble()?;
    assert_eq!(model.entities()?.len(), 1);
    let details = callable_details(&model.entities()?[0]);
    assert_eq!(details.attachments.len(), 1);
    assert!(matches!(
        details.attachments[0].content,
        TsAttachmentContent::Unavailable
    ));
    assert!(
        details
            .issues
            .iter()
            .any(|i| matches!(i.kind, TsCallableIssueKind::InvalidAttachmentOrigin))
    );
    assert!(matches!(
        model.code_projections()?[0],
        TsProjection::Unavailable(_)
    ));
    Ok(())
}

#[test]
fn explicit_context_metadata_is_not_silently_ignored_by_specialized_model() -> TestResult {
    for metadata in [
        "graph_label: Struct",
        "graph_label: Custom",
        "type: relation\n    relation_type: CALLS",
    ] {
        let queries = format!(
            "  - data_key: structs\n    {metadata}\n    context_capture: unit\n    pattern: '(struct_item name: (type_identifier) @name) @unit'\n"
        );
        inspect(
            "struct S;",
            &profile("context_types: {}", &queries),
            |model| {
                assert!(model.entities()?.is_empty());
                assert!(model.relations()?.is_empty());
                assert!(model.local_relations()?.is_empty());
                assert!(
                    matches!(&model.capabilities()?[0].state,TsCapability::UnsupportedConfiguration(fields) if fields==&["context_capture"])
                );
                assert!(model.diagnostics()?.iter().any(|d|matches!(&d.issue,TsIssue::UnsupportedConfiguration(fields) if fields==&["context_capture"])));
                assert_eq!(
                    model.parsed()?.context_document()?.observations()?.groups[0]
                        .matches
                        .len(),
                    1
                );
                Ok(())
            },
        )?;
    }
    Ok(())
}
