#[path = "support/corpus_rows.rs"]
mod corpus_rows;
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput, SyntheticReason},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::builtin,
    tree_sitter_model::{
        ParsedTreeSitterDocument, TreeSitterModelError, TreeSitterSemanticDocument, TsCallableKind,
        TsComplexity, TsEntityCallable, TsEntityDeclaredType, TsEntityExtent, TsExport,
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
        "../../../crates/graphit-ast/profiles/queries/go.yaml"
    ))
}
fn custom(queries: &str) -> CatalogDraft {
    catalog(format!("language: go\ngrammar: tree-sitter-go\nanon_func_types: []\nembed_labels: [Struct, Field, Function, Method]\ncontext_types: {{type_spec: Struct, function_declaration: Function, method_declaration: Method}}\nqueries:\n{queries}").as_bytes())
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
        let factory = PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            builtin("tree-sitter-go")?,
            &original,
        )?;
        let parsed =
            ParsedTreeSitterDocument::collect(&factory, input, &["src", "model.go"], limits())?;
        drop(factory);
        let model = parsed.assemble()?;
        action(&model)
    })
}

#[test]
fn grouped_structs_multiple_fields_and_callable_native_details() -> TestResult {
    let source = "package example\r\ntype (\r\n First struct { A, B int; Tag []byte \x60json:\"tag\"\x60; *First }\r\n Second struct { C string }\r\n)\r\nfunc Greet[T any](x T) T { return x }\r\nfunc (s *First) Read() int { return s.A }\r\nfunc (s Second) Read() string { return s.C }\r\nfunc External()\r\n";
    inspect(source, &stock(), |model| {
        assert!(model.diagnostics()?.is_empty());
        let entities = model.entities()?;
        assert_eq!(entities.len(), 15);
        assert_eq!(
            entities
                .iter()
                .filter(|e| e.identity().label() == "Parameter")
                .count(),
            3
        );
        assert_eq!(
            entities
                .iter()
                .filter(|e| e.identity().label() == "Type")
                .count(),
            2
        );
        let first = entities.iter().find(|e| e.name() == "First").unwrap();
        let second = entities.iter().find(|e| e.name() == "Second").unwrap();
        let TsEntityExtent::Exact(a) = first.extent() else {
            panic!("First extent")
        };
        let TsEntityExtent::Exact(b) = second.extent() else {
            panic!("Second extent")
        };
        assert!(a.range().text().starts_with("First struct"));
        assert!(!a.range().text().contains("Second"));
        assert_eq!(b.range().text(), "Second struct { C string }");
        let a = entities.iter().find(|e| e.name() == "A").unwrap();
        let b = entities.iter().find(|e| e.name() == "B").unwrap();
        assert_ne!(a.identity().uid().as_str(), b.identity().uid().as_str());
        for field in [a, b] {
            assert_eq!(field.namespaces()[0].name, "First");
            let TsEntityExtent::Exact(extent) = field.extent() else {
                panic!("field extent")
            };
            assert_eq!(extent.range().text(), "A, B int");
            let TsEntityDeclaredType::Native(ty) = field.declared_type() else {
                panic!("Go type")
            };
            assert_eq!(ty.kind, "type_identifier");
            assert_eq!(ty.raw.unwrap().text(), "int");
        }
        let read: Vec<_> = entities.iter().filter(|e| e.name() == "Read").collect();
        assert_eq!(read.len(), 2);
        assert_ne!(
            read[0].identity().uid().as_str(),
            read[1].identity().uid().as_str()
        );
        for method in read {
            assert!(method.namespaces().is_empty());
            let TsEntityCallable::Function(details) = method.callable() else {
                panic!("method details")
            };
            assert_eq!(details.kind, TsCallableKind::MethodGo);
            assert!(details.signature.syntax.is_none());
            let native = details.signature.go.as_ref().unwrap();
            assert!(
                native
                    .receiver
                    .as_ref()
                    .unwrap()
                    .raw
                    .unwrap()
                    .text()
                    .starts_with("(s ")
            );
            assert_eq!(
                native.parameters.as_ref().unwrap().raw.unwrap().text(),
                "()"
            );
            assert!(native.result.as_ref().unwrap().raw.is_some());
            assert_eq!(details.export, TsExport::CapitalizedName(true));
        }
        let generic = entities.iter().find(|e| e.name() == "Greet").unwrap();
        let TsEntityCallable::Function(details) = generic.callable() else {
            panic!("generic")
        };
        assert_eq!(
            details
                .signature
                .go
                .as_ref()
                .unwrap()
                .type_parameters
                .as_ref()
                .unwrap()
                .raw
                .unwrap()
                .text(),
            "[T any]"
        );
        let external = entities.iter().find(|e| e.name() == "External").unwrap();
        let TsEntityCallable::Function(details) = external.callable() else {
            panic!("bodyless")
        };
        assert_eq!(details.complexity, TsComplexity::NoBody);
        assert_eq!(details.signature.raw.unwrap().text(), "func External()");
        assert_eq!(model.code_projections()?.len(), 12);
        assert!(
            model
                .code_projections()?
                .iter()
                .all(|p| matches!(p, TsProjection::Available(_)))
        );
        Ok(())
    })
}

#[test]
fn subject_diagnostics_and_repeated_contributions_are_order_independent() -> TestResult {
    let good = "  - {data_key: fields, graph_label: Field, pattern: '(field_declaration name: (field_identifier) @name)'}\n";
    let bad = "  - {data_key: fields, graph_label: Field, span_capture: span, pattern: '(field_declaration name: (field_identifier) @name @span (#eq? @name \"A\"))'}\n";
    for reverse in [false, true] {
        let queries = if reverse {
            format!("{good}{bad}")
        } else {
            format!("{bad}{good}")
        };
        inspect(
            "package example\ntype First struct {A,B int}\n",
            &custom(&queries),
            |model| {
                let a = model.entities()?.iter().find(|e| e.name() == "A").unwrap();
                let b = model.entities()?.iter().find(|e| e.name() == "B").unwrap();
                assert!(!a.diagnostic_indices().is_empty());
                assert!(b.diagnostic_indices().is_empty());
                assert_eq!(a.contributions().len(), 2);
                assert_eq!(b.contributions().len(), 1);
                assert_eq!(
                    model
                        .code_projections()?
                        .iter()
                        .filter(|p| matches!(p, TsProjection::Available(_)))
                        .count(),
                    1
                );
                Ok(())
            },
        )?;
    }
    let repeated = format!(
        "{good}{good}  - {{data_key: second, graph_label: Field, pattern: '(field_declaration name: (field_identifier) @name)'}}\n"
    );
    inspect(
        "package example\ntype First struct {A,B int}\n",
        &custom(&repeated),
        |model| {
            assert!(model.diagnostics()?.is_empty());
            assert_eq!(model.entities()?.len(), 4);
            for name in ["A", "B"] {
                let e: Vec<_> = model
                    .entities()?
                    .iter()
                    .filter(|e| e.name() == name)
                    .collect();
                assert_eq!(e.len(), 2);
                assert_eq!(
                    e[0].identity().uid().as_str(),
                    e[1].identity().uid().as_str()
                );
                assert_ne!(e[0].data_key(), e[1].data_key());
                assert_eq!(e.iter().map(|e| e.contributions().len()).sum::<usize>(), 3);
            }
            Ok(())
        },
    )
}

#[test]
fn native_comments_unicode_exports_and_nested_function_metrics() -> TestResult {
    let source = "package example\nvar previous = 1 // trailing\nfunc lower() {}\n// Detached\n\nfunc Detached() {}\n// Native\n// doc\nfunc Éclair(x bool) bool { nested := func() { if x {} }; _ = nested; if x && x { return true }; return false }\nfunc ǅelta() {}\n";
    inspect(source, &stock(), |model| {
        assert!(model.diagnostics()?.is_empty());
        for (name, attached, export) in [
            ("lower", 0, false),
            ("Detached", 0, true),
            ("Éclair", 2, true),
            ("ǅelta", 0, false),
        ] {
            let e = model.entities()?.iter().find(|e| e.name() == name).unwrap();
            let TsEntityCallable::Function(details) = e.callable() else {
                panic!("callable")
            };
            assert_eq!(details.attachments.len(), attached);
            assert_eq!(details.export, TsExport::CapitalizedName(export));
            if name == "Éclair" {
                assert_eq!(details.attachments[0].raw.unwrap().text(), "// Native");
                assert!(matches!(
                    &details.complexity,
                    TsComplexity::Score { value: 3, .. }
                ));
            }
        }
        Ok(())
    })
}

#[test]
fn recovery_configuration_and_original_cancellation_are_causal() -> TestResult {
    inspect(
        "package example\ntype First struct { A,B [] }\n",
        &stock(),
        |model| {
            assert!(!model.diagnostics()?.is_empty());
            assert!(
                model
                    .code_projections()?
                    .iter()
                    .all(|p| matches!(p, TsProjection::Unavailable(_)))
            );
            Ok(())
        },
    )?;
    let parent = "  - {data_key: fields, graph_label: Field, parent_capture: owner, parent_label: Struct, pattern: '(field_declaration name: (field_identifier) @name) @owner'}\n";
    inspect(
        "package example\ntype First struct { A,B int }\n",
        &custom(parent),
        |model| {
            assert_eq!(model.entities()?.len(), 2);
            assert!(model.diagnostics()?.iter().any(|d| matches!(
                d.issue,
                graphit_ast::tree_sitter_model::TsIssue::InvalidExplicitParent { .. }
            )));
            assert!(
                model
                    .code_projections()?
                    .iter()
                    .all(|p| matches!(p, TsProjection::Unavailable(_)))
            );
            Ok(())
        },
    )?;
    let original = context();
    let draft = stock();
    let factory = PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        builtin("tree-sitter-go")?,
        &original,
    )?;
    with_source("package example\nfunc F(){}\n", &original, |input| {
        let parsed = ParsedTreeSitterDocument::collect(&factory, input, &["model.go"], limits())?;
        let model = parsed.assemble()?;
        original.cancellation().cancel();
        assert!(
            matches!(model.entities(),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        assert!(
            matches!(model.code_projections(),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        assert!(
            matches!(parsed.assemble(),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        assert!(
            matches!(ParsedTreeSitterDocument::collect(&factory,input,&["model.go"],limits()),Err(TreeSitterModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        Ok(())
    })
}

use arrow_array::{RecordBatch, StringArray};
use graphit_ast::{
    indexed_source::{IndexedSourceSnapshot, read_branch_source, source_schema},
    native_nn::{
        NativeNnModel, NativeNnSource, NativeNnTsInput, encode_native_nn_ts, validate_native_nn,
    },
    source_inspection::IndexedSource,
};
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use std::sync::Arc;
use tempfile::TempDir;
async fn raw(
    temp: &TempDir,
    name: &str,
    path: &str,
    text: &str,
    original: &Arc<SnapshotContext>,
) -> IndexedSourceSnapshot {
    let batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec![path])),
            Arc::new(StringArray::from(vec![text])),
        ],
    )
    .unwrap();
    let store = LanceModuleStore::create(
        temp.path().join(name).to_str().unwrap(),
        source_schema(),
        vec![batch],
    )
    .await
    .unwrap();
    let branches = LanceBranchStore::open(
        temp.path().join(name).to_str().unwrap(),
        source_schema(),
        None,
    )
    .await
    .unwrap();
    let named = branches
        .capture_target(Some("go"))
        .await
        .unwrap()
        .materialize()
        .await
        .unwrap();
    assert_eq!(store.snapshot().version(), 1);
    read_branch_source(&named, path, original).await.unwrap()
}

#[test]
fn shared_nonexact_type_and_expired_original_preserve_causes() -> TestResult {
    let text = "package example\ntype First struct {A,B int}\n";
    let start = text.find("int").unwrap() as u64;
    let end = text.len() as u64;
    let original = context();
    let draft = stock();
    let factory = PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        builtin("tree-sitter-go")?,
        &original,
    )?;
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
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start,
                end: start + 3,
            },
            reason: SyntheticReason::InsertedByTransform,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: start + 3,
                end,
            },
            original: ByteSpan {
                start: start + 3,
                end,
            },
        },
    ];
    let removed = [ByteSpan {
        start,
        end: start + 3,
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
    let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &["model.go"], limits())?;
    let model = parsed.assemble()?;
    for name in ["A", "B"] {
        let field = model.entities()?.iter().find(|e| e.name() == name).unwrap();
        assert!(!field.diagnostic_indices().is_empty());
        assert!(matches!(
            field.declared_type(),
            TsEntityDeclaredType::Unavailable
        ));
    }
    assert!(
        model
            .code_projections()?
            .iter()
            .all(|p| matches!(p, TsProjection::Unavailable(_)))
    );
    let expired = SnapshotContext::new(std::time::Instant::now(), Cancellation::default());
    assert!(
        PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            builtin("tree-sitter-go")?,
            &expired
        )
        .is_err()
    );
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        OriginPlan {
            segments: &[],
            removed_original: &[],
            anchors: &[],
        },
        &expired,
    );
    assert!(matches!(
        input,
        Err(graphit_ast::prepared_input::OriginError::DeadlineExceeded)
    ));
    Ok(())
}

#[tokio::test]
async fn genuine_go_codec_and_source_integrity_use_explicit_controlled_vectors() -> TestResult {
    let mut validated_batch = None;
    let mut corpus_lexical = None;
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let raw = raw(
        &temp,
        "source.lance",
        "model.go",
        "package example\nimport alias \"example/path\"\ntype First struct {A,B int}\ntype Reader interface {Read() int}\ntype Count int\nconst C = 1\nvar V int = 2\nfunc (s *First) Read() int {return s.A}\n",
        &original,
    )
    .await;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("actual Raw")
    };
    let draft = stock();
    let factory = PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        builtin("tree-sitter-go")?,
        &original,
    )?;
    let model_config = NativeNnModel {
        vector_id: [91; 32],
        dimensions: 2,
        model_id: "controlled-go-codec-fixture",
        revision: "one",
    };
    with_source(text, &original, |input| {
        let parsed = ParsedTreeSitterDocument::collect(&factory, input, &["model.go"], limits())?;
        let model = parsed.assemble()?;
        assert!(model.diagnostics()?.is_empty());
        let indices: Vec<_> = model
            .code_projections()?
            .iter()
            .filter_map(|p| match p {
                TsProjection::Available(p) => Some(p.entity_index),
                TsProjection::Unavailable(_) => None,
            })
            .collect();
        assert_eq!(indices.len(), 10);
        for label in ["Interface", "Type", "Constant", "Variable"] {
            assert!(
                indices
                    .iter()
                    .any(|&i| model.entities().unwrap()[i].identity().label() == label)
            );
        }
        // These controlled vectors qualify the codec; actual M remains Root's App companion.
        let vectors = vec![vec![1.0, 2.0]; indices.len()];
        let encode = |indices: &[usize], vectors: &[Vec<f32>], request: &Arc<SnapshotContext>| {
            encode_native_nn_ts(
                &model_config,
                &[NativeNnTsInput {
                    document: &model,
                    source: NativeNnSource::Indexed(&raw),
                    entity_indices: indices,
                    vectors,
                    model_generation: Some(11),
                }],
                request,
            )
        };

        let literal_draft = catalog(b"language: go\ngrammar: tree-sitter-go\ncontext_types: {}\nembed_labels: [Import]\nqueries:\n  - {data_key: imports, graph_label: Import, pattern: '(import_spec path: (interpreted_string_literal) @name)'}\n");
        let literal_factory = PreparedTreeSitterContexts::prepare(
            &literal_draft.profiles()[0],
            builtin("tree-sitter-go")?,
            &original,
        )?;
        let literal_parsed =
            ParsedTreeSitterDocument::collect(&literal_factory, input, &["model.go"], limits())?;
        let literal = literal_parsed.assemble()?;
        assert!(literal.diagnostics()?.is_empty());
        assert!(matches!(
            literal.entities()?[0].name_representation(),
            graphit_ast::tree_sitter_model::TsNameRepresentation::LiteralSource { .. }
        ));
        assert!(matches!(
            literal.code_projections()?[0],
            TsProjection::Available(_)
        ));
        let literal_batch = encode_native_nn_ts(
            &model_config,
            &[NativeNnTsInput {
                document: &literal,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &[0],
                vectors: &vectors[..1],
                model_generation: Some(11),
            }],
            &original,
        )?;
        assert_eq!(literal_batch.num_rows(), 1);
        let kind = literal_batch
            .column_by_name("name_kind")
            .unwrap()
            .as_any()
            .downcast_ref::<arrow_array::StringArray>()
            .unwrap();
        assert_eq!(kind.value(0), "LiteralSource");
        let name = literal_batch
            .column_by_name("name")
            .unwrap()
            .as_any()
            .downcast_ref::<arrow_array::StringArray>()
            .unwrap();
        assert_eq!(name.value(0), literal.entities()?[0].name());
        let batch = encode(&indices, &vectors, &original)?;
        validate_native_nn(&batch, &model_config, &original)?;
        assert_eq!(batch.num_rows(), 10);
        let languages = batch
            .column_by_name("language")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(languages.value(0), "go");
        for bad in [
            vec![vec![1.0]; indices.len()],
            vec![vec![f32::NAN, 0.0]; indices.len()],
            vec![],
        ] {
            assert!(encode(&indices, &bad, &original).is_err());
        }
        assert!(encode(&[0, 0], &vectors[..2], &original).is_err());
        assert!(encode(&[usize::MAX], &vectors[..1], &original).is_err());
        assert!(encode(&indices, &vectors, &Arc::new(context())).is_err());

        for (embed, query) in [
            (
                "[Field]",
                "(field_declaration name: (field_identifier) @name @span)",
            ),
            ("[]", "(field_declaration name: (field_identifier) @name)"),
        ] {
            let span = if embed == "[]" {
                ""
            } else {
                "span_capture: span,"
            };
            let draft=catalog(format!("language: go\ngrammar: tree-sitter-go\nanon_func_types: []\nembed_labels: {embed}\ncontext_types: {{type_spec: Struct}}\nqueries:\n  - {{data_key: fields, graph_label: Field, {span} pattern: '{query}'}}\n").as_bytes());
            let factory = PreparedTreeSitterContexts::prepare(
                &draft.profiles()[0],
                builtin("tree-sitter-go")?,
                &original,
            )?;
            let parsed =
                ParsedTreeSitterDocument::collect(&factory, input, &["model.go"], limits())?;
            let document = parsed.assemble()?;
            assert_eq!(document.entities()?.len(), 2);
            assert!(
                encode_native_nn_ts(
                    &model_config,
                    &[NativeNnTsInput {
                        document: &document,
                        source: NativeNnSource::Indexed(&raw),
                        entity_indices: &[0, 1],
                        vectors: &vectors[..2],
                        model_generation: Some(11)
                    }],
                    &original
                )
                .is_err()
            );
        }

        let wrong_model = NativeNnModel {
            dimensions: 3,
            ..model_config
        };
        assert!(validate_native_nn(&batch, &wrong_model, &original).is_err());
        let wrong_grammar = StringArray::from(vec!["tree-sitter-rust"; batch.num_rows()]);
        let mut columns = batch.columns().to_vec();
        columns[batch.schema().index_of("grammar").unwrap()] = Arc::new(wrong_grammar);
        let corrupt = RecordBatch::try_new(batch.schema(), columns)?;
        assert!(validate_native_nn(&corrupt, &model_config, &original).is_err());
        let copy = text.to_owned();
        with_source(&copy, &original, |copied| {
            let parsed =
                ParsedTreeSitterDocument::collect(&factory, copied, &["model.go"], limits())?;
            let document = parsed.assemble()?;
            assert!(
                encode_native_nn_ts(
                    &model_config,
                    &[NativeNnTsInput {
                        document: &document,
                        source: NativeNnSource::Indexed(&raw),
                        entity_indices: &indices,
                        vectors: &vectors,
                        model_generation: Some(11)
                    }],
                    &original
                )
                .is_err()
            );
            Ok(())
        })?;
        let parsed_wrong =
            ParsedTreeSitterDocument::collect(&factory, input, &["other.go"], limits())?;
        let wrong_path = parsed_wrong.assemble()?;
        assert!(
            encode_native_nn_ts(
                &model_config,
                &[NativeNnTsInput {
                    document: &wrong_path,
                    source: NativeNnSource::Indexed(&raw),
                    entity_indices: &indices,
                    vectors: &vectors,
                    model_generation: Some(11)
                }],
                &original
            )
            .is_err()
        );
        let graph = graphit_ast::semantic_graph::assemble_graph_ts(
            graphit_ast::semantic_graph::AstGraphTsInput {
                document: &model,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?
        .into_parts();
        corpus_lexical = Some(graphit_ast::entity_search::encode_file(
            &graph, text, &original,
        )?);
        validated_batch = Some(batch);
        Ok(())
    })?;
    let batch = validated_batch.ok_or("genuine encoded Go batch missing")?;
    let applied = corpus_rows::persist(
        &temp.path().join("corpus.lance"),
        corpus_lexical.ok_or("genuine lexical rows missing")?,
        &batch,
        &model_config,
        None,
        &original,
    )
    .await?;
    let found = graphit_ast::native_nn_query::query_corpus_nn(
        &applied,
        &model_config,
        &[1.0, 2.0],
        10,
        graphit_ast::native_nn_query::NativeNnMetric::L2,
        &original,
    )
    .await?;
    let rows = found.rows(&original)?;
    assert_eq!(rows.iter().map(RecordBatch::num_rows).sum::<usize>(), 10);
    for queried in rows {
        validate_native_nn(queried, &model_config, &original)?;
        let keys = queried
            .column_by_name("data_key")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let ids = queried
            .column_by_name("uid")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let original_keys = batch
            .column_by_name("data_key")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let original_ids = batch
            .column_by_name("uid")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for i in 0..queried.num_rows() {
            let expected = (0..batch.num_rows())
                .find(|&j| {
                    keys.value(i) == original_keys.value(j) && ids.value(i) == original_ids.value(j)
                })
                .unwrap();
            assert_eq!(queried.slice(i, 1), batch.slice(expected, 1));
        }
    }
    Ok(())
}
#[test]
fn exact_supported_profile_pair_is_required() -> TestResult {
    let draft = catalog(
        b"language: custom-go\ngrammar: tree-sitter-go\nanon_func_types: []\nqueries: []\n",
    );
    let original = context();
    let factory = PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        builtin("tree-sitter-go")?,
        &original,
    )?;
    with_source("package example\nfunc F(){}\n", &original, |input| {
        assert!(matches!(
            ParsedTreeSitterDocument::collect(&factory, input, &["model.go"], limits()),
            Err(TreeSitterModelError::UnsupportedProfile)
        ));
        Ok(())
    })
}

#[test]
fn generic_receiver_and_local_type_keep_actual_ancestry() -> TestResult {
    let source = "package example\ntype Box[T any] struct { Value T }\nfunc (b *Box[T]) Read() T {type Local struct {A int}; var x Local; _=x; return b.Value}\n";
    inspect(source, &stock(), |model| {
        assert!(model.diagnostics()?.is_empty());
        let entities = model.entities()?;
        let method = entities.iter().find(|e| e.name() == "Read").unwrap();
        let TsEntityCallable::Function(details) = method.callable() else {
            panic!("generic receiver")
        };
        assert_eq!(
            details
                .signature
                .go
                .as_ref()
                .unwrap()
                .receiver
                .as_ref()
                .unwrap()
                .raw
                .unwrap()
                .text(),
            "(b *Box[T])"
        );
        assert!(method.namespaces().is_empty());
        let local = entities
            .iter()
            .find(|e| e.name() == "Local" && e.identity().label() == "Struct")
            .unwrap();
        assert!(matches!(local.callable(), TsEntityCallable::NotApplicable));
        assert_eq!(local.namespaces()[0].name, "Read");
        Ok(())
    })
}

#[test]
fn inserted_newline_cannot_reclassify_original_trailing_comment() -> TestResult {
    let original_text = "package example\nvar x=1 // doc\nfunc F(){}\n";
    let prepared = "package example\nvar x=1\n // doc\nfunc F(){}\n";
    let insertion = original_text.find(" // doc").unwrap() as u64;
    let end = original_text.len() as u64;
    let original = context();
    let draft = stock();
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: insertion,
            },
            original: ByteSpan {
                start: 0,
                end: insertion,
            },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: insertion,
                end: insertion + 1,
            },
            reason: SyntheticReason::InsertedByTransform,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: insertion + 1,
                end: end + 1,
            },
            original: ByteSpan {
                start: insertion,
                end,
            },
        },
    ];
    let input = PreparedInput::new(
        original_text.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let factory = PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        builtin("tree-sitter-go")?,
        &original,
    )?;
    let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &["model.go"], limits())?;
    let model = parsed.assemble()?;
    let function = model.entities()?.iter().find(|e| e.name() == "F").unwrap();
    let TsEntityCallable::Function(details) = function.callable() else {
        panic!("function")
    };
    assert!(details.attachments.is_empty());
    assert!(details.issues.iter().any(|i| matches!(
        i.kind,
        graphit_ast::tree_sitter_model::TsCallableIssueKind::InvalidAttachmentOrigin
    )));
    assert!(!function.diagnostic_indices().is_empty());
    assert!(
        model
            .code_projections()?
            .iter()
            .any(|p| matches!(p, TsProjection::Unavailable(_)))
    );
    assert!(model.code_projections()?.iter().any(|p| matches!(p, TsProjection::Available(p) if model.entities().unwrap()[p.entity_index].name() == "x")));
    Ok(())
}

#[test]
fn remaining_go_declarations_keep_native_subjects_and_extents() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsDeclaredValue, TsNameRepresentation, TsValueBinding};
    let source = "package sample\nimport alias \"example/path\"\ntype (\nReader interface { Read(p []byte) (n int, err error) }\nScalar int\nBox struct { A, B int }\n)\nconst A, B = 1, 2\nvar C, D int = 3, 4\nfunc F(x, y int) (z int) { var Local int; _ = Local; return x+y }\n";
    inspect(source, &stock(), |model| {
        assert!(
            model.diagnostics()?.is_empty(),
            "{:?}",
            model
                .diagnostics()?
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        let entities = model.entities()?;
        let pick = |name: &str, label: &str| {
            entities
                .iter()
                .find(|e| e.name() == name && e.identity().label() == label)
                .unwrap_or_else(|| {
                    panic!(
                        "missing {name}/{label}; actual {:?}",
                        entities
                            .iter()
                            .map(|e| (e.name(), e.identity().label()))
                            .collect::<Vec<_>>()
                    )
                })
        };
        for (name, label, extent) in [
            (
                "Reader",
                "Interface",
                "Reader interface { Read(p []byte) (n int, err error) }",
            ),
            (
                "Reader",
                "Type",
                "Reader interface { Read(p []byte) (n int, err error) }",
            ),
            ("Scalar", "Type", "Scalar int"),
            ("Box", "Struct", "Box struct { A, B int }"),
            ("Box", "Type", "Box struct { A, B int }"),
            ("A", "Constant", "A, B = 1, 2"),
            ("B", "Constant", "A, B = 1, 2"),
            ("C", "Variable", "C, D int = 3, 4"),
            ("D", "Variable", "C, D int = 3, 4"),
            ("x", "Parameter", "x, y int"),
            ("y", "Parameter", "x, y int"),
            ("z", "Parameter", "z int"),
        ] {
            let entity = pick(name, label);
            assert_eq!(
                entity.name_representation(),
                TsNameRepresentation::Identifier
            );
            let TsEntityExtent::Exact(actual) = entity.extent() else {
                panic!("missing extent for {name}/{label}");
            };
            assert_eq!(actual.range().text(), extent);
            if matches!(label, "Type" | "Interface" | "Parameter") {
                assert!(matches!(
                    entity.declared_type(),
                    TsEntityDeclaredType::Native(_)
                ));
            }
        }
        assert_ne!(
            pick("Reader", "Interface").identity().uid().as_str(),
            pick("Reader", "Type").identity().uid().as_str()
        );
        assert!(pick("Reader", "Interface").namespaces().is_empty());
        assert_eq!(pick("p", "Parameter").namespaces()[0].name, "Reader");
        assert_eq!(pick("p", "Parameter").namespaces()[0].label, "Struct");
        assert_eq!(pick("x", "Parameter").namespaces()[0].name, "F");
        for (name, label, literal) in [
            ("A", "Constant", "1"),
            ("B", "Constant", "2"),
            ("C", "Variable", "3"),
            ("D", "Variable", "4"),
        ] {
            let entity = pick(name, label);
            let bound: Vec<_> = entity
                .contributions()
                .iter()
                .flat_map(|c| match &c.declared_value {
                    TsDeclaredValue::Native { captures, .. } => captures
                        .iter()
                        .filter(|c| matches!(c.binding, TsValueBinding::Subject { .. }))
                        .collect::<Vec<_>>(),
                    _ => vec![],
                })
                .collect();
            assert!(
                !bound.is_empty(),
                "missing bound literal for {name}/{label}"
            );
            assert!(
                bound
                    .iter()
                    .all(|c| c.syntax.raw.unwrap().text() == literal)
            );
            assert!(
                entity
                    .contributions()
                    .iter()
                    .any(|c| matches!(c.declared_value, TsDeclaredValue::NotConfigured))
            );
            assert_eq!(
                entity.contributions()[0]
                    .go_declaration
                    .as_ref()
                    .unwrap()
                    .export,
                TsExport::CapitalizedName(true)
            );
        }
        assert_eq!(
            pick("Local", "Variable").contributions()[0]
                .go_declaration
                .as_ref()
                .unwrap()
                .export,
            TsExport::CapitalizedName(false)
        );
        let import = pick("\"example/path\"", "Import");
        assert_eq!(
            import.name_representation(),
            TsNameRepresentation::LiteralSource {
                native_kind: "interpreted_string_literal"
            }
        );
        let metadata = import.contributions()[0].go_declaration.as_ref().unwrap();
        assert_eq!(
            metadata.alias.as_ref().unwrap().raw.unwrap().text(),
            "alias"
        );
        assert!(!model.code_projections()?.iter().any(|p| matches!(p, TsProjection::Available(p) if matches!(p.entity.identity().label(),"Import"|"Parameter"))));
        Ok(())
    })
}

#[test]
fn remaining_go_literal_source_preserves_author_bytes_and_optional_types() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsDeclaredValue, TsValueBinding};
    let source = "package sample\nconst (\nHuge = 184467440737095516160001\nOct = 077\nHex = 0XFF_FF\nBinary = 0B1010_0011\nHexFloat = 0x1.fp3\nDecimal = 0.12345678901234567890123456789\nBytes = \"\\xff\\377\\u00ff\"\nRune = '\\377'\nRaw = \x60first\r\nsecond\x60\nInherited\n)\nvar Missing int\n";
    inspect(source, &stock(), |model| {
        assert!(
            model.diagnostics()?.is_empty(),
            "{:?}",
            model
                .diagnostics()?
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        for (name, literal, kind) in [
            ("Huge", "184467440737095516160001", "int_literal"),
            ("Oct", "077", "int_literal"),
            ("Hex", "0XFF_FF", "int_literal"),
            ("Binary", "0B1010_0011", "int_literal"),
            ("HexFloat", "0x1.fp3", "float_literal"),
            (
                "Decimal",
                "0.12345678901234567890123456789",
                "float_literal",
            ),
            (
                "Bytes",
                "\"\\xff\\377\\u00ff\"",
                "interpreted_string_literal",
            ),
            ("Rune", "'\\377'", "rune_literal"),
            ("Raw", "\x60first\r\nsecond\x60", "raw_string_literal"),
        ] {
            let entity = model.entities()?.iter().find(|e| e.name() == name).unwrap();
            assert!(matches!(
                entity.declared_type(),
                TsEntityDeclaredType::NotApplicable
            ));
            let captures: Vec<_> = entity
                .contributions()
                .iter()
                .flat_map(|c| match &c.declared_value {
                    TsDeclaredValue::Native { captures, .. } => captures
                        .iter()
                        .filter(|c| matches!(c.binding, TsValueBinding::Subject { .. }))
                        .collect::<Vec<_>>(),
                    _ => vec![],
                })
                .collect();
            assert!(!captures.is_empty());
            assert!(
                captures
                    .iter()
                    .all(|c| c.syntax.kind == kind && c.syntax.raw.unwrap().text() == literal)
            );
        }
        let inherited = model
            .entities()?
            .iter()
            .find(|e| e.name() == "Inherited")
            .unwrap();
        assert!(
            inherited
                .contributions()
                .iter()
                .all(|c| matches!(c.declared_value, TsDeclaredValue::NotConfigured))
        );
        let missing = model
            .entities()?
            .iter()
            .find(|e| e.name() == "Missing")
            .unwrap();
        assert!(matches!(
            missing.declared_type(),
            TsEntityDeclaredType::Native(_)
        ));
        assert!(
            missing
                .contributions()
                .iter()
                .all(|c| matches!(c.declared_value, TsDeclaredValue::NotConfigured))
        );
        Ok(())
    })
}

#[test]
fn remaining_go_local_resolution_uses_scope_receiver_and_access_evidence() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsLocalTargetState, TsLocalUnresolvedReason};
    let source = "package sample\ntype Box struct { A, B int }\ntype Other struct { A int }\nfunc F() int { return 1 }\nfunc (b *Box) Read() { F() }\nfunc (b *Other) Read() {}\nfunc Caller(b *Box, other *Other, obj any) { F(); b.Read(); other.Read(); _ = b.A; b.A = 1; b.A += 2; obj.Read(); func(){ F() }() }\nfunc Shadow(F func()) { F() }\nfunc Local() { F := func(){}; F() }\nfunc Before() { F := F(); _ = F }\n";
    inspect(source, &stock(), |model| {
        assert!(
            model.diagnostics()?.is_empty(),
            "{:?}",
            model
                .diagnostics()?
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        let relations = model.relations()?;
        let entities = model.entities()?;
        let mut direct = 0;
        let mut methods = Vec::new();
        let mut reads = 0;
        let mut writes = 0;
        let mut write_only = 0;
        let mut shadow = 0;
        let mut anonymous = 0;
        let mut unknown = 0;
        for result in model.local_relations()? {
            let observation = &relations[result.relation_index];
            let capture = &observation.matched.captures[result.capture_index];
            let relation = observation.query.value().relation_type().value().as_str();
            match &result.target {
                TsLocalTargetState::Resolved { entity_index } => {
                    let target = &entities[*entity_index];
                    if relation == "CALLS" && capture.native.text == "F" {
                        assert_eq!(target.name(), "F");
                        assert_eq!(target.identity().label(), "Function");
                        direct += 1;
                    }
                    if relation == "CALLS" && capture.native.text == "Read" {
                        assert_eq!(target.name(), "Read");
                        methods.push(target.contributions()[0].declaration_node_index);
                    }
                    if relation == "READS_FIELD" && capture.native.text == "A" {
                        assert_eq!(target.name(), "A");
                        assert_eq!(target.namespaces()[0].name, "Box");
                        reads += 1;
                    }
                    if relation == "WRITES_FIELD" {
                        assert_eq!(target.name(), "A");
                        assert_eq!(target.namespaces()[0].name, "Box");
                        writes += 1;
                    }
                }
                TsLocalTargetState::Unresolved {
                    reason: TsLocalUnresolvedReason::Shadowed { .. },
                } => shadow += 1,
                TsLocalTargetState::Unresolved {
                    reason: TsLocalUnresolvedReason::AnonymousSource { .. },
                } => anonymous += 1,
                TsLocalTargetState::Unresolved {
                    reason: TsLocalUnresolvedReason::NotReadAccess { .. },
                } => write_only += 1,
                TsLocalTargetState::Unresolved {
                    reason: TsLocalUnresolvedReason::UnknownType { .. },
                } if relation == "CALLS" => unknown += 1,
                _ => {}
            }
        }
        assert_eq!(direct, 3);
        assert_eq!(methods.len(), 2);
        assert_ne!(methods[0], methods[1]);
        assert_eq!(reads, 2);
        assert_eq!(writes, 2);
        assert_eq!(write_only, 1);
        assert_eq!(shadow, 2);
        assert_eq!(anonymous, 1);
        assert_eq!(unknown, 1);
        Ok(())
    })
}

#[test]
fn remaining_go_value_bindings_are_query_order_independent() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsDeclaredValue, TsValueBinding};
    let queries = [
        "  - {data_key: constants, graph_label: Constant, pattern: '(const_spec (identifier) @name)'}\n",
        "  - {data_key: constants, graph_label: Constant, value_capture: value, value_label: Value, pattern: '(const_spec (identifier) @name value: (expression_list (int_literal) @value))'}\n",
        "  - {data_key: variables, graph_label: Variable, pattern: '(var_spec name: (identifier) @name)'}\n",
        "  - {data_key: variables, graph_label: Variable, value_capture: value, value_label: Value, pattern: '(var_spec name: (identifier) @name value: (expression_list (int_literal) @value))'}\n",
    ];
    let mut snapshots = Vec::new();
    for reverse in [false, true] {
        let order: Vec<_> = if reverse {
            queries.iter().rev().collect()
        } else {
            queries.iter().collect()
        };
        let draft=catalog(format!("language: go\ngrammar: tree-sitter-go\ncontext_types: {{}}\nembed_labels: [Constant,Variable]\nqueries:\n{}",order.into_iter().copied().collect::<String>()).as_bytes());
        let mut snapshot = Vec::new();
        inspect(
            "package sample\nconst A,B=1,2\nvar C,D int=3,4\n",
            &draft,
            |model| {
                assert!(model.diagnostics()?.is_empty());
                assert_eq!(
                    model
                        .entities()?
                        .iter()
                        .filter(|e| e.identity().label() == "Value")
                        .count(),
                    4
                );
                for e in model.entities()? {
                    if e.identity().label() == "Value" {
                        let owner = e.contributions()[0].derived_value.as_ref().unwrap();
                        snapshot.push((
                            e.name().to_owned(),
                            e.identity().uid().as_str().to_owned(),
                            vec![owner.owner_uid.clone()],
                        ));
                        continue;
                    }
                    assert!(e.model_conflicts().is_empty());
                    let mut values: Vec<_> = e
                        .contributions()
                        .iter()
                        .flat_map(|c| match &c.declared_value {
                            TsDeclaredValue::Native { captures, .. } => captures
                                .iter()
                                .filter(|c| matches!(c.binding, TsValueBinding::Subject { .. }))
                                .map(|c| c.syntax.raw.unwrap().text().to_owned())
                                .collect::<Vec<_>>(),
                            _ => vec![],
                        })
                        .collect();
                    values.sort();
                    values.dedup();
                    assert_eq!(
                        values,
                        vec![
                            match e.name() {
                                "A" => "1",
                                "B" => "2",
                                "C" => "3",
                                "D" => "4",
                                _ => panic!("unexpected subject"),
                            }
                            .to_owned()
                        ]
                    );
                    snapshot.push((
                        e.name().to_owned(),
                        e.identity().uid().as_str().to_owned(),
                        values,
                    ));
                }
                assert_eq!(model.code_projections()?.len(), 4);
                Ok(())
            },
        )?;
        snapshot.sort();
        snapshots.push(snapshot);
    }
    assert_eq!(snapshots[0], snapshots[1]);
    Ok(())
}

#[test]
fn remaining_go_explicit_parent_has_real_capture_provenance() -> TestResult {
    let draft=catalog(b"language: go\ngrammar: tree-sitter-go\ncontext_types: {}\nembed_labels: [Parameter]\nqueries:\n  - {data_key: parameters, graph_label: Parameter, parent_capture: owner, parent_label: Function, pattern: '(function_declaration name: (identifier) @owner parameters: (parameter_list (parameter_declaration name: (identifier) @name)))'}\n");
    inspect("package sample\nfunc F(x int) {}\n", &draft, |model| {
        assert!(
            model.diagnostics()?.is_empty(),
            "{:?}",
            model
                .diagnostics()?
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        let e = &model.entities()?[0];
        assert_eq!(e.name(), "x");
        assert_eq!(e.namespaces().len(), 1);
        let owner = &e.namespaces()[0];
        assert_eq!(owner.name, "F");
        assert_eq!(owner.label, "Function");
        assert_eq!(owner.context_index, None);
        assert!(owner.explicit_parent_capture_index.is_some());
        assert!(matches!(
            model.code_projections()?[0],
            TsProjection::Available(_)
        ));
        Ok(())
    })
}

#[test]
fn remaining_go_local_switch_dynamic_and_candidate_states_are_causal() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsLocalTargetState, TsLocalUnresolvedReason};
    let source = "package sample\nimport pkg \"example/pkg\"\ntype I interface { M() }\ntype Alias = I\ntype Box struct { I; A int }\nfunc F() any {return nil}\nfunc D() {}\nfunc D() {}\nfunc C(x I, y Alias, z interface { M() }, b Box) {\n switch F := 1; F { case 1: }; F()\n switch F := F().(type) { case int: F(); default: F() }; F()\n x.M(); y.M(); z.M(); pkg.M(); D(); Missing(); _ = b.A; b.M()\n}\n";
    inspect(source, &stock(), |model| {
        assert!(model.diagnostics()?.is_empty());
        let resolutions = model.local_relations()?;
        assert_eq!(
            resolutions
                .iter()
                .filter(|r| matches!(
                    r.target,
                    TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::Shadowed { .. }
                    }
                ))
                .count(),
            2
        );
        assert_eq!(resolutions.iter().filter(|r| matches!(r.target, TsLocalTargetState::Resolved { entity_index } if model.entities().unwrap()[entity_index].name() == "F")).count(), 3);
        assert_eq!(resolutions.iter().filter(|r| matches!(&r.target, TsLocalTargetState::Ambiguous { entity_indices } if entity_indices.len() == 2 && entity_indices.iter().all(|&i| model.entities().unwrap()[i].name() == "D"))).count(), 1);
        assert_eq!(
            resolutions
                .iter()
                .filter(|r| matches!(r.target, TsLocalTargetState::NotFound))
                .count(),
            1
        );
        assert_eq!(
            resolutions
                .iter()
                .filter(|r| model.relations().unwrap()[r.relation_index]
                    .query
                    .value()
                    .relation_type()
                    .value()
                    == "CALLS")
                .filter(|r| matches!(
                    r.target,
                    TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::DynamicReceiver { .. }
                    }
                ))
                .count(),
            3
        );
        assert_eq!(
            resolutions
                .iter()
                .filter(|r| model.relations().unwrap()[r.relation_index]
                    .query
                    .value()
                    .relation_type()
                    .value()
                    == "CALLS")
                .filter(|r| matches!(
                    r.target,
                    TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::ExternalImport { .. }
                    }
                ))
                .count(),
            1
        );
        assert!(resolutions.iter().any(|r| matches!(r.target, TsLocalTargetState::Resolved { entity_index } if model.entities().unwrap()[entity_index].name() == "A")));
        assert!(resolutions.iter().any(|r| matches!(
            r.target,
            TsLocalTargetState::Unresolved {
                reason: TsLocalUnresolvedReason::PromotedMember { .. }
            }
        )));
        Ok(())
    })
}

#[test]
fn remaining_go_whole_constant_parent_is_not_a_first_name_owner() -> TestResult {
    let draft = catalog(b"language: go\ngrammar: tree-sitter-go\ncontext_types: {}\nembed_labels: [Field]\nqueries:\n  - {data_key: fields, graph_label: Field, parent_capture: owner, parent_label: Constant, pattern: '(const_spec value: (expression_list (call_expression arguments: (argument_list (composite_literal type: (struct_type (field_declaration_list (field_declaration name: (field_identifier) @name)))))))) @owner'}\n");
    inspect(
        "package sample\nconst A,B = unsafe.Sizeof(struct{X int}{}),1\n",
        &draft,
        |model| {
            assert_eq!(model.entities()?.len(), 1);
            assert_eq!(model.entities()?[0].name(), "X");
            assert!(model.entities()?[0].namespaces().is_empty());
            assert!(model.diagnostics()?.iter().any(|d| matches!(
                d.issue,
                graphit_ast::tree_sitter_model::TsIssue::InvalidExplicitParent { .. }
            )));
            assert!(matches!(
                model.code_projections()?[0],
                TsProjection::Unavailable(_)
            ));
            Ok(())
        },
    )
}

#[test]
fn remaining_go_initializer_parts_and_tuple_arity_are_native_observations() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsDeclaredValue, TsValueBinding};
    let draft = catalog(b"language: go\ngrammar: tree-sitter-go\ncontext_types: {}\nembed_labels: []\nqueries:\n  - {data_key: tuple, graph_label: Variable, value_capture: value, value_label: Value, pattern: '(var_spec name: (identifier) @name value: (expression_list (call_expression) @value))'}\n  - {data_key: whole, graph_label: Variable, value_capture: value, value_label: Value, pattern: '(var_spec name: (identifier) @name value: (expression_list) @value)'}\n  - {data_key: part, graph_label: Variable, value_capture: value, value_label: Value, pattern: '(var_spec name: (identifier) @name value: (expression_list (binary_expression (int_literal) @value)))'}\n");
    inspect(
        "package sample\nvar A,B = Pair()\nvar C = 1+2\n",
        &draft,
        |model| {
            assert!(model.diagnostics()?.is_empty());
            for e in model.entities()? {
                for c in e.contributions() {
                    let TsDeclaredValue::Native {
                        initializer: Some(initializer),
                        captures,
                        ..
                    } = &c.declared_value
                    else {
                        panic!("native initializer")
                    };
                    assert!(initializer.raw.is_some());
                    for capture in captures {
                        assert!(capture.syntax.raw.is_some());
                        match e.data_key() {
                            "tuple" => assert!(matches!(
                                capture.binding,
                                TsValueBinding::UnprovenArity {
                                    subjects: 2,
                                    expressions: 1
                                }
                            )),
                            "whole" => {
                                assert!(matches!(capture.binding, TsValueBinding::Initializer))
                            }
                            "part" => assert!(matches!(
                                capture.binding,
                                TsValueBinding::ExpressionPart { ordinal: 0 }
                            )),
                            _ => panic!("actual data key"),
                        }
                    }
                }
            }
            assert_eq!(model.entities()?.len(), 6);
            Ok(())
        },
    )
}

#[test]
fn remaining_go_name_reject_keeps_other_native_subject_available() -> TestResult {
    let draft = catalog(b"language: go\ngrammar: tree-sitter-go\ncontext_types: {}\nembed_labels: [Constant]\nqueries:\n  - {data_key: constants, graph_label: Constant, name_reject: '^A$', pattern: '(const_spec (identifier) @name)'}\n");
    inspect("package sample\nconst A,B = 1,2\n", &draft, |model| {
        assert_eq!(model.entities()?.len(), 1);
        assert_eq!(model.entities()?[0].name(), "B");
        assert!(model.entities()?[0].diagnostic_indices().is_empty());
        assert!(model.diagnostics()?.iter().any(|d| matches!(
            d.issue,
            graphit_ast::tree_sitter_model::TsIssue::ExcludedName { .. }
        )));
        assert!(matches!(
            model.code_projections()?[0],
            TsProjection::Available(_)
        ));
        Ok(())
    })
}

#[test]
fn remaining_go_alias_imports_variadic_and_anonymous_struct_fields_are_native() -> TestResult {
    use graphit_ast::tree_sitter_model::TsNameRepresentation;
    let draft = catalog(b"language: go\ngrammar: tree-sitter-go\ncontext_types: {function_declaration: Function}\nembed_labels: [Type,Field]\nqueries:\n  - {data_key: imports, graph_label: Import, pattern: '(import_spec path: [(interpreted_string_literal) (raw_string_literal)] @name)'}\n  - {data_key: types, graph_label: Type, pattern: '(type_alias name: (type_identifier) @name)'}\n  - {data_key: fields, graph_label: Field, pattern: '(field_declaration name: (field_identifier) @name)'}\n  - {data_key: parameters, graph_label: Parameter, pattern: '(variadic_parameter_declaration name: (identifier) @name)'}\n");
    inspect(
        "package sample\nimport (_ \"example/a\"; . `example/b`)\ntype Alias = int\nvar Holder struct { A,B int }\nfunc F(_ int, xs ...string) {}\n",
        &draft,
        |model| {
            assert!(
                model.diagnostics()?.is_empty(),
                "{:?}",
                model
                    .diagnostics()?
                    .iter()
                    .map(|d| &d.issue)
                    .collect::<Vec<_>>()
            );
            assert_eq!(model.entities()?.len(), 6);
            for (name, alias) in [("\"example/a\"", "_"), ("`example/b`", ".")] {
                let e = model.entities()?.iter().find(|e| e.name() == name).unwrap();
                assert!(matches!(
                    e.name_representation(),
                    TsNameRepresentation::LiteralSource { .. }
                ));
                assert_eq!(
                    e.contributions()[0]
                        .go_declaration
                        .as_ref()
                        .unwrap()
                        .alias
                        .as_ref()
                        .unwrap()
                        .raw
                        .unwrap()
                        .text(),
                    alias
                );
            }
            let alias = model
                .entities()?
                .iter()
                .find(|e| e.name() == "Alias")
                .unwrap();
            assert!(
                matches!(alias.extent(), TsEntityExtent::Exact(e) if e.range().text() == "Alias = int")
            );
            assert!(alias.namespaces().is_empty());
            for name in ["A", "B"] {
                let e = model.entities()?.iter().find(|e| e.name() == name).unwrap();
                assert!(e.namespaces().is_empty());
                assert!(matches!(e.declared_type(), TsEntityDeclaredType::Native(_)));
            }
            let parameter = model.entities()?.iter().find(|e| e.name() == "xs").unwrap();
            assert_eq!(parameter.namespaces()[0].name, "F");
            assert!(matches!(
                parameter.declared_type(),
                TsEntityDeclaredType::Native(_)
            ));
            assert!(model.entities()?.iter().all(|e| e.name() != "_"));
            Ok(())
        },
    )
}

#[test]
fn configured_value_label_preserves_native_rows_and_distinct_owners() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsNameRepresentation, TsOwnerSelection, TsValueBinding};
    let source = "package p\nvar a,b=1,2\nvar c=1\nvar empty=\"\"\nvar x,y=pair()\nfunc pair()(int,int){return 1,2}\n";
    inspect(source, &stock(), |model| {
        assert!(model.diagnostics()?.is_empty());
        let entities = model.entities()?;
        let values: Vec<_> = entities
            .iter()
            .filter(|e| e.identity().label() == "Value")
            .collect();
        assert_eq!(values.len(), 4);
        for (owner_name, literal) in [("a", "1"), ("b", "2"), ("c", "1"), ("empty", "\"\"")] {
            let owner = entities
                .iter()
                .find(|e| e.name() == owner_name && e.identity().label() == "Variable")
                .unwrap();
            let row = values
                .iter()
                .find(|e| e.namespaces().last().is_some_and(|n| n.name == owner_name))
                .unwrap();
            assert_eq!(row.name(), literal);
            assert_eq!(row.name_raw().text(), literal);
            assert_eq!(row.data_key(), "variables");
            assert!(matches!(
                row.name_representation(),
                TsNameRepresentation::LiteralSource { .. }
            ));
            let TsEntityExtent::Exact(extent) = row.extent() else {
                panic!("value extent");
            };
            assert_eq!(extent.range().text(), literal);
            assert_eq!(extent.range().start(), row.name_raw().start());
            let namespace = row.namespaces().last().unwrap();
            assert_eq!(namespace.label, "Variable");
            assert_eq!(
                namespace.selection,
                TsOwnerSelection::DerivedDeclarationIdentifier
            );
            assert_eq!(namespace.context_index, None);
            assert_eq!(namespace.explicit_parent_capture_index, None);
            let contribution = &row.contributions()[0];
            let provenance = contribution.derived_value.as_ref().unwrap();
            assert_eq!(provenance.owner_uid, owner.identity().uid().as_str());
            assert_eq!(provenance.owner_label, "Variable");
            assert_eq!(
                contribution.occurrence.name_capture_index,
                Some(provenance.value_capture_index)
            );
            let observations = model.parsed()?.context_document()?.observations()?;
            let captures = &observations.groups[contribution.occurrence.query_index].matches
                [contribution.occurrence.match_index]
                .captures;
            assert_eq!(
                captures[provenance.owner_name_capture_index].native.node_id,
                observations.nodes[provenance.owner_name_node_index].node_id
            );
            assert!(matches!(provenance.binding, TsValueBinding::Subject { .. }));
            assert_eq!(
                contribution.name_capture.native.node_id,
                model.parsed()?.context_document()?.observations()?.groups
                    [contribution.occurrence.query_index]
                    .matches[contribution.occurrence.match_index]
                    .captures[provenance.value_capture_index]
                    .native
                    .node_id
            );
            assert!(owner.contributions().iter().any(|c| matches!(
                c.declared_value,
                graphit_ast::tree_sitter_model::TsDeclaredValue::Native { .. }
            )));
        }
        let ones: Vec<_> = values.iter().filter(|e| e.name() == "1").collect();
        assert_ne!(
            ones[0].identity().uid().as_str(),
            ones[1].identity().uid().as_str()
        );
        assert!(
            !values
                .iter()
                .any(|e| ["x", "y"].contains(&e.namespaces().last().unwrap().name.as_str()))
        );
        assert!(model.code_projections()?.iter().all(|p| match p {
            TsProjection::Available(p) => p.entity.identity().label() != "Value",
            TsProjection::Unavailable(p) => entities[p.entity_index].identity().label() != "Value",
        }));
        Ok(())
    })
}

#[test]
fn configured_value_label_preserves_repeated_queries_and_unproven_bindings() -> TestResult {
    use graphit_ast::tree_sitter_model::{TsDeclaredValue, TsIssue, TsValueBinding};
    let query = "  - data_key: variables\n    graph_label: Variable\n    pattern: '(var_spec name: (identifier) @name value: (expression_list (_) @value))'\n    value_capture: value\n    value_label: Datum\n";
    let draft = custom(&format!(
        "{query}{query}{}",
        query.replace("data_key: variables", "data_key: secondary")
    ));
    inspect(
        "package p\nvar a,b=1,2\nvar x,y=pair()\nvar ident=outside\n",
        &draft,
        |model| {
            let rows: Vec<_> = model
                .entities()?
                .iter()
                .filter(|e| e.identity().label() == "Datum")
                .collect();
            assert_eq!(rows.len(), 6);
            for literal in ["1", "2", "outside"] {
                let primary = rows
                    .iter()
                    .find(|e| e.name() == literal && e.data_key() == "variables")
                    .unwrap();
                let secondary = rows
                    .iter()
                    .find(|e| e.name() == literal && e.data_key() == "secondary")
                    .unwrap();
                assert_eq!(
                    primary.identity().uid().as_str(),
                    secondary.identity().uid().as_str()
                );
                assert_eq!(primary.contributions().len(), 2);
                assert_eq!(secondary.contributions().len(), 1);
                assert!(primary.conflicts().is_empty());
                assert!(primary.model_conflicts().is_empty());
                assert_ne!(
                    primary.contributions()[0].occurrence.query_index,
                    primary.contributions()[1].occurrence.query_index
                );
            }
            assert!(!rows.iter().any(|e| e.name() == "pair()"));
            for name in ["x", "y"] {
                let owner = model
                    .entities()?
                    .iter()
                    .find(|e| e.name() == name && e.data_key() == "variables")
                    .unwrap();
                assert!(
                    owner.diagnostic_indices().is_empty(),
                    "value ambiguity must not poison valid declaration"
                );
                assert!(owner.contributions().iter().all(|c| matches!(&c.declared_value,
                TsDeclaredValue::Native { captures, .. } if captures.iter().any(|v| matches!(v.binding,
                    TsValueBinding::UnprovenArity { subjects: 2, expressions: 1 })))));
            }
            assert!(
                model
                    .diagnostics()?
                    .iter()
                    .any(|d| matches!(d.issue, TsIssue::UnprovenValueOwner { .. }))
            );
            Ok(())
        },
    )?;
    let outside = custom(
        "  - data_key: variables\n    graph_label: Variable\n    pattern: '(var_spec name: (identifier) @name type: (_) @value)'\n    value_capture: value\n    value_label: Datum\n",
    );
    inspect("package p\nvar a int=1\n", &outside, |model| {
        assert!(
            !model
                .entities()?
                .iter()
                .any(|e| e.identity().label() == "Datum")
        );
        assert!(
            model
                .diagnostics()?
                .iter()
                .any(|d| matches!(d.issue, TsIssue::InvalidValueCapture { .. }))
        );
        Ok(())
    })
}

#[test]
fn configured_value_label_fragments_and_initializer_ownership_are_explicit() -> TestResult {
    use graphit_ast::tree_sitter_model::TsValueBinding;
    let fragment = custom(
        "  - data_key: variables\n    graph_label: Variable\n    pattern: '(var_spec name: (identifier) @name value: (expression_list (call_expression function: (identifier) @value)))'\n    value_capture: value\n    value_label: Datum\n",
    );
    inspect(
        "package p\nvar one=f()\nvar a,b=f(),g()\n",
        &fragment,
        |model| {
            let value = model
                .entities()?
                .iter()
                .find(|e| e.identity().label() == "Datum")
                .unwrap();
            assert_eq!(value.name(), "f");
            let rows: Vec<_> = model
                .entities()?
                .iter()
                .filter(|e| e.identity().label() == "Datum")
                .collect();
            assert_eq!(rows.len(), 3);
            for (owner, literal) in [("a", "f"), ("b", "g")] {
                let row = rows
                    .iter()
                    .find(|e| e.namespaces().last().unwrap().name == owner)
                    .unwrap();
                assert_eq!(row.name(), literal);
                assert!(row.diagnostic_indices().is_empty());
            }
            assert_eq!(value.namespaces().last().unwrap().name, "one");
            assert!(matches!(
                value.contributions()[0]
                    .derived_value
                    .as_ref()
                    .unwrap()
                    .binding,
                TsValueBinding::ExpressionPart { ordinal: 0 }
            ));
            Ok(())
        },
    )?;
    let initializer = custom(
        "  - data_key: variables\n    graph_label: Variable\n    pattern: '(var_spec name: (identifier) @name value: (expression_list) @value)'\n    value_capture: value\n    value_label: Datum\n",
    );
    inspect(
        "package p\nvar one=f()\nvar a,b=1,2\n",
        &initializer,
        |model| {
            let values: Vec<_> = model
                .entities()?
                .iter()
                .filter(|e| e.identity().label() == "Datum")
                .collect();
            assert_eq!(values.len(), 1);
            assert_eq!(values[0].name(), "f()");
            assert!(matches!(
                values[0].contributions()[0]
                    .derived_value
                    .as_ref()
                    .unwrap()
                    .binding,
                TsValueBinding::Initializer
            ));
            assert!(model.diagnostics()?.iter().any(|d| matches!(
                d.issue,
                graphit_ast::tree_sitter_model::TsIssue::UnprovenValueOwner {
                    binding: TsValueBinding::Initializer,
                    ..
                }
            )));
            Ok(())
        },
    )
}

#[test]
fn configured_value_label_nonexact_and_recovery_do_not_fabricate_rows() -> TestResult {
    let authored = "package p\nvar a=1\n";
    let prepared = "package p\nvar a=12\n";
    let insertion = authored.find("1").unwrap() as u64 + 1;
    let end = authored.len() as u64;
    let cancellation = Cancellation::default();
    let original = SnapshotContext::service(cancellation.clone());
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: insertion,
            },
            original: ByteSpan {
                start: 0,
                end: insertion,
            },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: insertion,
                end: insertion + 1,
            },
            reason: SyntheticReason::InsertedByTransform,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: insertion + 1,
                end: end + 1,
            },
            original: ByteSpan {
                start: insertion,
                end,
            },
        },
    ];
    let input = PreparedInput::new(
        authored.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let draft = stock();
    let factory = PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        builtin("tree-sitter-go")?,
        &original,
    )?;
    let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &["model.go"], limits())?;
    let model = parsed.assemble()?;
    assert!(
        !model
            .entities()?
            .iter()
            .any(|e| e.identity().label() == "Value")
    );
    assert!(model.diagnostics()?.iter().any(|d| matches!(
        d.issue,
        graphit_ast::tree_sitter_model::TsIssue::NonExact { role: "value", .. }
    )));
    cancellation.cancel();
    assert!(model.entities().is_err());
    let recovery = custom(
        "  - data_key: variables\n    graph_label: Variable\n    pattern: '(var_spec name: (identifier) @name value: (expression_list (_) @value))'\n    value_capture: value\n    value_label: Datum\n",
    );
    inspect("package p\nvar a=func(){if {}}\n", &recovery, |model| {
        assert!(
            !model
                .entities()?
                .iter()
                .any(|e| e.identity().label() == "Datum")
        );
        assert!(model.diagnostics()?.iter().any(|d| matches!(
            d.issue,
            graphit_ast::tree_sitter_model::TsIssue::Recovery { .. }
        )));
        Ok(())
    })
}

#[test]
fn configured_value_label_function_literal_does_not_become_a_callable_source() -> TestResult {
    use graphit_ast::tree_sitter_model::{
        TsLocalTargetState, TsLocalUnresolvedReason, TsRelationSourceState,
    };
    let draft=catalog(b"language: go\ngrammar: tree-sitter-go\ncontext_types: {function_declaration: Function}\nanon_func_types: [func_literal]\ntarget_rules:\n  CALLS: {labels: [Function]}\nqueries:\n  - {data_key: functions, graph_label: Function, pattern: '(function_declaration name: (identifier) @name)'}\n  - data_key: variables\n    graph_label: Variable\n    pattern: '(var_spec name: (identifier) @name value: (expression_list (func_literal) @value))'\n    value_capture: value\n    value_label: Datum\n  - {data_key: calls, type: relation, relation_type: CALLS, pattern: '(call_expression function: (identifier) @name)'}\n");
    inspect(
        "package p\nfunc F(){}\nvar x=func(){F()}\n",
        &draft,
        |model| {
            assert!(
                model
                    .entities()?
                    .iter()
                    .any(|e| e.identity().label() == "Datum" && e.name() == "func(){F()}")
            );
            assert_eq!(model.relations()?.len(), 1);
            assert!(matches!(
                model.relations()?[0].sources[0].state,
                TsRelationSourceState::UnavailableContainer { .. }
            ));
            assert!(matches!(
                model.local_relations()?[0].target,
                TsLocalTargetState::Unresolved {
                    reason: TsLocalUnresolvedReason::AnonymousSource { .. }
                }
            ));
            Ok(())
        },
    )
}
