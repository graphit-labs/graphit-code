#[path = "support/corpus_rows.rs"]
mod corpus_rows;
use arrow_array::{Array, RecordBatch, StringArray};
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    indexed_source::{IndexedSourceSnapshot, read_branch_source, source_schema},
    native_nn::{
        NativeNnError, NativeNnModel, NativeNnSource, NativeNnTsInput, encode_native_nn_ts,
        validate_native_nn,
    },
    native_nn_query::{NativeNnMetric, query_corpus_nn},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_graph::{AstGraphTsInput, AstGraphValue, assemble_graph_ts},
    source_inspection::IndexedSource,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::builtin,
    tree_sitter_model::{
        ParsedTreeSitterDocument, TreeSitterSemanticDocument, TsComplexity, TsEntityCallable,
        TsEntityExtent, TsExport, TsLocalTargetState, TsLocalUnresolvedReason,
        TsNameRepresentation, TsOwnerSelection,
    },
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use std::{collections::BTreeSet, error::Error, sync::Arc};
use tempfile::TempDir;
type TestResult = Result<(), Box<dyn Error>>;

#[tokio::test]
async fn genuine_ecma_source_original_selection_vector_and_empty_guards_are_causal() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let path = "guards.js";
    let snapshot = raw(&temp, "function F(){}", path, &original).await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("actual Raw Source");
    };
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/javascript.yaml"
    ));
    let model = NativeNnModel {
        vector_id: [7; 32],
        dimensions: 2,
        model_id: "controlled-fixture",
        revision: "guards",
    };
    with_model(text, &original, &draft, &[path], |document| {
        let encode = |indices: &[usize], vectors: &[Vec<f32>], request: &Arc<SnapshotContext>| {
            encode_native_nn_ts(
                &model,
                &[NativeNnTsInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                    entity_indices: indices,
                    vectors,
                    model_generation: Some(9),
                }],
                request,
            )
        };
        validate_native_nn(
            &encode(&[0], &[vec![1.0, 0.0]], &original)?,
            &model,
            &original,
        )?;
        assert!(matches!(
            encode(&[99], &[vec![1.0, 0.0]], &original),
            Err(NativeNnError::InvalidInput)
        ));
        assert!(matches!(
            encode(&[0, 0], &vec![vec![1.0, 0.0]; 2], &original),
            Err(NativeNnError::DuplicateIdentity)
        ));
        for vector in [vec![1.0], vec![f32::NAN, 0.0], vec![1.0, f32::INFINITY]] {
            assert!(matches!(
                encode(&[0], &[vector], &original),
                Err(NativeNnError::InvalidVector)
            ));
        }
        let foreign = Arc::new(context());
        assert!(matches!(
            encode(&[0], &[vec![1.0, 0.0]], &foreign),
            Err(NativeNnError::RequestMismatch)
        ));
        assert!(
            assemble_graph_ts(
                AstGraphTsInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot)
                },
                &foreign
            )
            .is_err()
        );
        Ok(())
    })?;
    let copied = text.to_owned();
    with_model(&copied, &original, &draft, &[path], |document| {
        assert!(matches!(
            encode_native_nn_ts(
                &model,
                &[NativeNnTsInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                    entity_indices: &[0],
                    vectors: &[vec![1.0, 0.0]],
                    model_generation: Some(9)
                }],
                &original
            ),
            Err(NativeNnError::InvalidInput)
        ));
        Ok(())
    })?;
    with_model(text, &original, &draft, &["wrong.js"], |document| {
        assert!(matches!(
            encode_native_nn_ts(
                &model,
                &[NativeNnTsInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                    entity_indices: &[0],
                    vectors: &[vec![1.0, 0.0]],
                    model_generation: Some(9)
                }],
                &original
            ),
            Err(NativeNnError::InvalidInput)
        ));
        Ok(())
    })?;
    let empty_temp = TempDir::new()?;
    let empty_raw = raw(&empty_temp, "", "empty.js", &original).await?;
    let IndexedSource::Available(empty) = empty_raw.source(&original)? else {
        panic!("stored empty Source");
    };
    with_model(empty, &original, &draft, &["empty.js"], |document| {
        assert!(document.entities()?.is_empty());
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&empty_raw),
            },
            &original,
        )?;
        assert!(graph.nodes().is_empty());
        assert_eq!(graph.file_observation().label(), "AstFileObservation");
        let batch = encode_native_nn_ts(
            &model,
            &[NativeNnTsInput {
                document,
                source: NativeNnSource::Indexed(&empty_raw),
                entity_indices: &[],
                vectors: &[],
                model_generation: None,
            }],
            &original,
        )?;
        assert_eq!(batch.num_rows(), 0);
        Ok(())
    })?;
    let transformed_temp = TempDir::new()?;
    let transformed_raw = raw(&transformed_temp, "const a=1;", "nonexact.js", &original).await?;
    let IndexedSource::Available(authored) = transformed_raw.source(&original)? else {
        panic!("actual authored Source");
    };
    let prepared = "const a=12;";
    let insertion = authored.find('1').unwrap() as u64 + 1;
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
            reason: graphit_ast::prepared_input::SyntheticReason::InsertedByTransform,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: insertion + 1,
                end: prepared.len() as u64,
            },
            original: ByteSpan {
                start: insertion,
                end: authored.len() as u64,
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
    let factory = PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        builtin(draft.profiles()[0].grammar().value())?,
        &original,
    )?;
    let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &["nonexact.js"], limits())?;
    let document = parsed.assemble()?;
    assert!(document.diagnostics()?.iter().any(|d| matches!(
        d.issue,
        graphit_ast::tree_sitter_model::TsIssue::NonExact { .. }
    )));
    assert!(document.code_projections()?.iter().all(|p| matches!(
        p,
        graphit_ast::tree_sitter_model::TsProjection::Unavailable { .. }
    )));
    assert!(matches!(
        encode_native_nn_ts(
            &model,
            &[NativeNnTsInput {
                document: &document,
                source: NativeNnSource::Indexed(&transformed_raw),
                entity_indices: &[0],
                vectors: &[vec![1.0, 0.0]],
                model_generation: Some(9)
            }],
            &original
        ),
        Err(NativeNnError::InvalidInput)
    ));
    original.cancellation().cancel();
    assert!(
        matches!(document.entities(),Err(graphit_ast::tree_sitter_model::TreeSitterModelError::Operation(e)) if e.kind() == std::io::ErrorKind::Interrupted)
    );
    assert!(document.code_projections().is_err());
    assert!(parsed.assemble().is_err());
    assert!(encode_native_nn_ts(&model, &[], &original).is_err());
    Ok(())
}

#[tokio::test]
async fn genuine_ecma_null_raw_uses_project_physical_capture_and_revalidates_change() -> TestResult
{
    use graphit_auth::{ProjectCapture, ScopeCapture};
    use graphit_core::{
        artifact_store::{CapturedPublicationScope, ScopeLimits},
        brand::Brand,
        git_checkout::GitCheckout,
        installation_identity::InstallationIdentityStore,
        project_identity::{ProjectIdentityStore, ProjectMetadata},
        project_paths::{InstallationPaths, ProjectPaths},
        version_context::VersionContext,
    };
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let timeout = std::time::Duration::from_secs(5);
    let paths = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )?;
    InstallationIdentityStore::new(&paths)?.ensure(timeout, original.cancellation())?;
    let root = temp.path().join("project");
    std::fs::create_dir(&root)?;
    let project_paths = ProjectPaths::at(&root, &paths)?;
    let project_id = ProjectIdentityStore::at(&project_paths)
        .writer(timeout, original.cancellation())?
        .ensure(&ProjectMetadata::new("actual-ecma-project")?)?;
    let project_scope = CapturedPublicationScope::project(
        &project_paths,
        project_id.id(),
        ScopeLimits::new(16, 4096, 256)?,
        &original,
    )?;
    let record = project_scope.binding()?;
    let checkout = GitCheckout::inspect(project_paths.root(), timeout, original.cancellation())?;
    let version = VersionContext::select(&project_paths, checkout.as_checkout(), "", None, "")?;
    std::fs::write(root.join("physical.js"), "export function F(){}")?;
    let null = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec!["physical.js"])),
            Arc::new(StringArray::from(vec![None::<&str>])),
        ],
    )?;
    let store = LanceModuleStore::create(
        temp.path().join("null.lance").to_str().unwrap(),
        source_schema(),
        vec![null],
    )
    .await?;
    let snapshot = graphit_ast::indexed_source::read_indexed_source(
        &store.snapshot(),
        "physical.js",
        &original,
    )
    .await?;
    let capture = graphit_ast::native_nn_physical_source::capture_native_nn_physical_source(
        ProjectCapture {
            scope: ScopeCapture {
                live: &project_scope,
                record: &record,
            },
            version: &version,
        },
        &["physical.js"],
        graphit_core::artifact_store::CopyLimits::new(65536, 16, 4096, 8)?,
        graphit_core::artifact_store::RetainedBytesLimits::new(1, 1, 256)?,
        graphit_core::artifact_store::SourceReadLimits::new(65536, 1024, 65536)?,
        &original,
    )?;
    let text = capture.source(&original)?;
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/javascript.yaml"
    ));
    with_model(text, &original, &draft, &["physical.js"], |document| {
        let model = NativeNnModel {
            vector_id: [7; 32],
            dimensions: 2,
            model_id: "controlled-fixture",
            revision: "physical",
        };
        let encode = || {
            encode_native_nn_ts(
                &model,
                &[NativeNnTsInput {
                    document,
                    source: NativeNnSource::Physical {
                        capture: &capture,
                        raw: &snapshot,
                    },
                    entity_indices: &[0],
                    vectors: &[vec![1.0, 0.0]],
                    model_generation: Some(9),
                }],
                &original,
            )
        };
        validate_native_nn(&encode()?, &model, &original)?;
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Physical {
                    capture: &capture,
                    raw: &snapshot,
                },
            },
            &original,
        )?;
        assert_eq!(graph.nodes().len(), 1);
        std::fs::write(root.join("physical.js"), "export function Changed(){}")?;
        assert!(matches!(encode(), Err(NativeNnError::PhysicalSource(_))));
        Ok(())
    })
}

#[tokio::test]
async fn genuine_ecma_policies_preserve_export_comments_and_nested_or_disabled_metrics()
-> TestResult {
    let source = "function outer(){if(true){} function nested(){if(true){} if(true){}} const arrow=()=>{if(true){}};}\nconst previous=0; // trailing\n// leading\nexport function F(x){return x}\n";
    for disabled in [false, true] {
        let temp = TempDir::new()?;
        let original = Arc::new(context());
        let path = "policy.js";
        let snapshot = raw(&temp, source, path, &original).await?;
        let IndexedSource::Available(text) = snapshot.source(&original)? else {
            panic!("actual Source");
        };
        let mut profile =
            include_str!("../../../crates/graphit-ast/profiles/queries/javascript.yaml").to_owned();
        if disabled {
            let start = profile.find("complexity:").unwrap();
            let end = profile[start..].find("embed_labels:").unwrap() + start;
            profile.replace_range(
                start..end,
                "complexity: {node_types: [], operators: []}\n\n",
            );
        }
        let draft = catalog(profile.as_bytes());
        with_model(text, &original, &draft, &[path], |document| {
            for (name, score) in [("outer", 2), ("nested", 3), ("arrow", 2), ("F", 1)] {
                let entity = document
                    .entities()?
                    .iter()
                    .find(|e| e.identity().label() == "Function" && e.name() == name)
                    .unwrap();
                let TsEntityCallable::Function(details) = entity.callable() else {
                    panic!("native callable metadata");
                };
                if disabled {
                    assert!(matches!(details.complexity, TsComplexity::Disabled));
                } else {
                    assert!(
                        matches!(details.complexity,TsComplexity::Score { value,.. } if value == score),
                        "{name}: {:?}",
                        details.complexity
                    );
                }
                assert_eq!(details.export, TsExport::Statement(name == "F"));
                if name == "F" {
                    assert_eq!(details.attachments.len(), 1);
                    assert_eq!(details.attachments[0].raw.unwrap().text(), "// leading");
                    assert!(matches!(
                        details.attachments[0].content,
                        graphit_ast::tree_sitter_model::TsAttachmentContent::Comment
                    ));
                }
            }
            assert!(
                document
                    .entities()?
                    .iter()
                    .all(|e| e.identity().label() != "Comment")
            );
            let graph = assemble_graph_ts(
                AstGraphTsInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            let node = graph.nodes().iter().find(|n|matches!(n.properties().get("name"),Some(AstGraphValue::String(s)) if s == "F")).unwrap();
            let AstGraphValue::String(metadata) = node.properties().get("metadata_json").unwrap()
            else {
                panic!("owned metadata");
            };
            let metadata: serde_json::Value = serde_json::from_str(metadata)?;
            assert_eq!(
                metadata["contributions"][0]["callable"]["attachments"][0]["literal_source"],
                "// leading"
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[tokio::test]
async fn genuine_ecma_local_binding_roles_preserve_import_shadow_and_this_boundaries() -> TestResult
{
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let path = "bindings.js";
    let source = include_str!("fixtures/ecma/binding_scopes.js");
    let snapshot = raw(&temp, source, path, &original).await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("genuine Raw Source");
    };
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/javascript.yaml"
    ));
    with_model(text, &original, &draft, &[path], |document| {
        let relations = document.relations()?;
        let outcomes = document.local_relations()?;
        let nodes = &document.parsed()?.context_document()?.observations()?.nodes;
        let mut names = std::collections::BTreeMap::<
            String,
            Vec<&graphit_ast::tree_sitter_model::TsLocalRelationResolution>,
        >::new();
        for outcome in outcomes {
            let relation = &relations[outcome.relation_index];
            let raw = &relation.matched.captures[outcome.capture_index].original;
            let graphit_ast::prepared_input::OriginalProjection::Exact(name) = raw else {
                panic!("real exact capture");
            };
            println!(
                "{} {:?} {:?}",
                relation.query.value().relation_type().value(),
                name.text(),
                outcome.target
            );
            if relation.query.value().relation_type().value() == "CALLS" {
                names
                    .entry(name.text().to_owned())
                    .or_default()
                    .push(outcome);
            }
            assert!(
                outcome
                    .evidence_node_indices
                    .iter()
                    .all(|&i| i < nodes.len())
            );
        }
        assert!(matches!(
            names["before"][0].target,
            TsLocalTargetState::Unresolved {
                reason: TsLocalUnresolvedReason::TemporalDeadZone { .. }
            }
        ));
        assert!(matches!(
            names["before"][1].target,
            TsLocalTargetState::Resolved { .. }
        ));
        assert!(matches!(
            names["Block"][0].target,
            TsLocalTargetState::Resolved { .. }
        ));
        assert!(matches!(
            names["Legacy"][0].target,
            TsLocalTargetState::Unresolved {
                reason: TsLocalUnresolvedReason::UnsupportedSyntax { .. }
            }
        ));
        for name in ["param", "binding", "rest"] {
            assert!(
                matches!(
                    names[name][0].target,
                    TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::Shadowed { .. }
                    }
                ),
                "unindexed parameter blocks outer {name}"
            );
        }
        for name in ["Alias", "run"] {
            assert!(matches!(
                names[name][0].target,
                TsLocalTargetState::Unresolved {
                    reason: TsLocalUnresolvedReason::ExternalImport { .. }
                }
            ));
        }
        let reads = outcomes
            .iter()
            .filter(|o| {
                relations[o.relation_index]
                    .query
                    .value()
                    .relation_type()
                    .value()
                    == "READS_FIELD"
            })
            .collect::<Vec<_>>();
        assert_eq!(reads.len(), 6, "namespace member read remains observed");
        assert!(matches!(
            reads[0].target,
            TsLocalTargetState::Unresolved {
                reason: TsLocalUnresolvedReason::ExternalImport { .. }
            }
        ));
        let reads = &reads[1..];
        assert_eq!(reads.len(), 5);
        assert!(
            matches!(reads[0].target, TsLocalTargetState::Resolved { .. }),
            "arrow retains native class this"
        );
        assert!(
            matches!(
                reads[1].target,
                TsLocalTargetState::Unresolved {
                    reason: TsLocalUnresolvedReason::DynamicReceiver { .. }
                }
            ),
            "ordinary function does not inherit class this"
        );
        assert!(matches!(
            reads[2].target,
            TsLocalTargetState::Unresolved {
                reason: TsLocalUnresolvedReason::NotReadAccess { .. }
            }
        ));
        assert!(matches!(
            reads[3].target,
            TsLocalTargetState::Resolved { .. }
        ));
        let TsLocalTargetState::Resolved { entity_index } = reads[4].target else {
            panic!("native static receiver");
        };
        let entity = &document.entities()?[entity_index];
        let TsEntityExtent::Exact(extent) = entity.extent() else {
            panic!("full static field extent");
        };
        assert_eq!(extent.range().text(), "static field=2");
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&snapshot),
            },
            &original,
        )?;
        assert!(!graph.edges().is_empty());
        Ok(())
    })
}

#[tokio::test]
async fn genuine_ecma_canonical_targets_and_ts_rules_bind_actual_declarations() -> TestResult {
    for (path,source,profile) in [
        ("canonical.js","function a(){} function caller(){\\u0061();}",include_str!("../../../crates/graphit-ast/profiles/queries/javascript.yaml").to_owned()),
        ("rules.ts",include_str!("fixtures/ecma/declarations.ts"),include_str!("../../../crates/graphit-ast/profiles/queries/typescript.yaml").replace("target_rules:\n","target_rules:\n  IMPLEMENTS: {labels: [Interface]}\n  DECORATOR: {labels: [Function]}\n")),
    ] {
        let temp = TempDir::new()?;
        let original = Arc::new(context());
        let snapshot = raw(&temp,source,path,&original).await?;
        let IndexedSource::Available(text) = snapshot.source(&original)? else { panic!("actual Source"); };
        let draft = catalog(profile.as_bytes());
        with_model(text,&original,&draft,&[path],|document| {
            let relations = document.relations()?;
            let outcomes = document.local_relations()?;
            for kind in if path == "canonical.js" { &["CALLS"][..] } else { &["IMPLEMENTS","DECORATOR"][..] } {
                let outcome = outcomes.iter().find(|o|relations[o.relation_index].query.value().relation_type().value() == *kind).expect("actual configured relation");
                let TsLocalTargetState::Resolved { entity_index } = outcome.target else { panic!("{path} {kind}: {:?}",outcome.target); };
                let target = &document.entities()?[entity_index];
                assert_eq!(target.name(),match *kind { "CALLS" => "a","IMPLEMENTS" => "I","DECORATOR" => "mark",_ => unreachable!() });
                if *kind == "DECORATOR" {
                    let source = &relations[outcome.relation_index].sources[outcome.source_index];
                    let graphit_ast::tree_sitter_model::TsRelationSourceState::Bound { entity_indices,.. } = &source.state else { panic!("native export declaration ownership"); };
                    assert!(entity_indices.iter().all(|&i|document.entities().unwrap()[i].name() == "C"));
                }
            }
            let graph = assemble_graph_ts(AstGraphTsInput { document,source: NativeNnSource::Indexed(&snapshot) },&original)?;
            assert!(!graph.edges().is_empty());
            Ok(())
        })?;
    }
    Ok(())
}

#[tokio::test]
async fn genuine_ecma_mixed_name_kinds_survive_sdk_publication_and_query() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let path = "names.js";
    let source = include_str!("fixtures/ecma/names.js");
    let snapshot = raw(&temp, source, path, &original).await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("actual stored Source");
    };
    let profile = include_str!("../../../crates/graphit-ast/profiles/queries/javascript.yaml")
        .replace(
            "embed_labels:\n",
            "embed_labels:\n  - Value\n  - Pair\n  - Import\n  - Export\n  - Parameter\n",
        );
    let draft = catalog(profile.as_bytes());
    let model = NativeNnModel {
        vector_id: [7; 32],
        dimensions: 2,
        model_id: "controlled-fixture",
        revision: "ecma-names",
    };
    let mut mixed = None;
    let mut corpus_lexical = None;
    with_model(text, &original, &draft, &[path], |document| {
        let projections = document.code_projections()?;
        let mut indices = Vec::new();
        for projection in &projections {
            match projection {
                graphit_ast::tree_sitter_model::TsProjection::Available(p) => {
                    indices.push(p.entity_index)
                }
                _ => panic!("genuine selected ECMA projection unavailable"),
            }
        }
        assert_eq!(indices.len(), document.entities()?.len());
        let vectors = vec![vec![1.0, 0.0]; indices.len()];
        let batch = encode_native_nn_ts(
            &model,
            &[NativeNnTsInput {
                document,
                source: NativeNnSource::Indexed(&snapshot),
                entity_indices: &indices,
                vectors: &vectors,
                model_generation: Some(9),
            }],
            &original,
        )?;
        validate_native_nn(&batch, &model, &original)?;
        let strings = |name: &str| {
            batch
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
        };
        assert!(
            strings("name_kind")
                .iter()
                .flatten()
                .any(|s| s == "LiteralSource")
        );
        assert!(
            strings("name_kind")
                .iter()
                .flatten()
                .any(|s| s == "Identifier")
        );
        let variables = (0..batch.num_rows())
            .filter(|&i| {
                strings("data_key").value(i) == "variables" && strings("name").value(i) == "a"
            })
            .collect::<Vec<_>>();
        assert_eq!(variables.len(), 2);
        assert_ne!(
            strings("uid").value(variables[0]),
            strings("uid").value(variables[1])
        );
        let mut columns = batch.columns().to_vec();
        let name_index = batch.schema().index_of("name")?;
        let mut names = strings("name")
            .iter()
            .map(|s| s.unwrap().to_owned())
            .collect::<Vec<_>>();
        names[variables[0]] = "differentCanonicalName".to_owned();
        columns[name_index] = Arc::new(StringArray::from(names));
        let corrupt = RecordBatch::try_new(batch.schema(), columns)?;
        assert!(matches!(
            validate_native_nn(&corrupt, &model, &original),
            Err(NativeNnError::InvalidRow)
        ));

        for (column, value) in [("language", "go"), ("grammar", "tree-sitter-go")] {
            let mut columns = batch.columns().to_vec();
            let column_index = batch.schema().index_of(column)?;
            let values = (0..batch.num_rows())
                .map(|i| {
                    if i == 0 {
                        value
                    } else {
                        strings(column).value(i)
                    }
                })
                .collect::<Vec<_>>();
            columns[column_index] = Arc::new(StringArray::from(values));
            let wrong_domain = RecordBatch::try_new(batch.schema(), columns)?;
            assert!(matches!(
                validate_native_nn(&wrong_domain, &model, &original),
                Err(NativeNnError::InvalidRow)
            ));
        }
        let mut columns = batch.columns().to_vec();
        let column_index = batch.schema().index_of("projection")?;
        let values = (0..batch.num_rows())
            .map(|i| {
                if i == 0 {
                    "different Source projection"
                } else {
                    strings("projection").value(i)
                }
            })
            .collect::<Vec<_>>();
        columns[column_index] = Arc::new(StringArray::from(values));
        let wrong_digest = RecordBatch::try_new(batch.schema(), columns)?;
        assert!(matches!(
            validate_native_nn(&wrong_digest, &model, &original),
            Err(NativeNnError::InvalidRow)
        ));
        let graph = graphit_ast::semantic_graph::assemble_graph_ts(
            graphit_ast::semantic_graph::AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&snapshot),
            },
            &original,
        )?
        .into_parts();
        corpus_lexical = Some(graphit_ast::entity_search::encode_file(
            &graph, text, &original,
        )?);
        mixed = Some(batch);
        Ok(())
    })?;
    let mixed = mixed.unwrap();
    let published = corpus_rows::persist(
        &temp.path().join("corpus.lance"),
        corpus_lexical.ok_or("genuine lexical rows missing")?,
        &mixed,
        &model,
        None,
        &original,
    )
    .await?;
    let queried = query_corpus_nn(
        &published,
        &model,
        &[1.0, 0.0],
        mixed.num_rows(),
        NativeNnMetric::L2,
        &original,
    )
    .await?;
    let source_strings = |name: &str| {
        mixed
            .column_by_name(name)
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
    };
    let mut count = 0;
    for batch in queried.rows(&original)? {
        validate_native_nn(batch, &model, &original)?;
        let strings = |name: &str| {
            batch
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
        };
        for row in 0..batch.num_rows() {
            let source_row = (0..mixed.num_rows())
                .find(|&i| {
                    source_strings("uid").value(i) == strings("uid").value(row)
                        && source_strings("data_key").value(i) == strings("data_key").value(row)
                })
                .unwrap();
            for field in mixed.schema().fields() {
                assert_eq!(
                    batch
                        .column_by_name(field.name())
                        .unwrap()
                        .slice(row, 1)
                        .to_data(),
                    mixed
                        .column_by_name(field.name())
                        .unwrap()
                        .slice(source_row, 1)
                        .to_data()
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, mixed.num_rows());
    Ok(())
}

#[tokio::test]
async fn genuine_ecma_families_keep_canonical_names_literals_native_ranges_and_graph() -> TestResult
{
    for (path, source, profile, expected) in [
        (
            "component.tsx",
            include_str!("fixtures/ecma/component.tsx"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/tsx.yaml").as_slice(),
            &[
                "Import",
                "Interface",
                "Class",
                "Field",
                "Value",
                "Method",
                "Function",
                "Variable",
                "Parameter",
            ][..],
        ),
        (
            "component.mjs",
            include_str!("fixtures/ecma/component.jsx"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/javascript.yaml")
                .as_slice(),
            &[
                "Class",
                "Field",
                "Value",
                "Method",
                "Function",
                "Variable",
                "Parameter",
            ][..],
        ),
        (
            "component.jsx",
            include_str!("fixtures/ecma/component.jsx"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/javascript.yaml")
                .as_slice(),
            &[
                "Class",
                "Field",
                "Value",
                "Method",
                "Function",
                "Variable",
                "Parameter",
            ][..],
        ),
        (
            "declarations.ts",
            include_str!("fixtures/ecma/declarations.ts"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/typescript.yaml")
                .as_slice(),
            &[
                "Function",
                "Parameter",
                "Interface",
                "Type",
                "Enum",
                "EnumMember",
                "Class",
                "Field",
                "Value",
                "Method",
            ][..],
        ),
        (
            "names.js",
            include_str!("fixtures/ecma/names.js"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/javascript.yaml")
                .as_slice(),
            &[
                "Variable",
                "Function",
                "Parameter",
                "Import",
                "Pair",
                "Value",
            ][..],
        ),
        (
            "local.js",
            include_str!("fixtures/ecma/local.js"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/javascript.yaml")
                .as_slice(),
            &["Function", "Variable", "Class", "Field", "Value", "Method"][..],
        ),
        (
            "recovery.tsx",
            include_str!("fixtures/ecma/recovery.tsx"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/tsx.yaml").as_slice(),
            &[][..],
        ),
    ] {
        let temp = TempDir::new()?;
        let original = Arc::new(context());
        let snapshot = raw(&temp, source, path, &original).await?;
        let IndexedSource::Available(text) = snapshot.source(&original)? else {
            panic!("genuine Raw Source");
        };
        let draft = catalog(profile);
        with_model(text, &original, &draft, &[path], |document| {
            let entities = document.entities()?;
            let families = entities
                .iter()
                .map(|e| e.identity().label())
                .collect::<BTreeSet<_>>();
            println!("{path} families={families:?}");
            for entity in entities {
                println!(
                    "{path} {} {} {:?} namespaces={:?} issues={:?}",
                    entity.identity().label(),
                    entity.data_key(),
                    entity.name(),
                    entity
                        .namespaces()
                        .iter()
                        .map(|n| (&n.name, n.selection))
                        .collect::<Vec<_>>(),
                    entity.diagnostic_indices()
                );
            }
            for label in expected {
                assert!(families.contains(label), "{path}: missing {label}");
            }
            assert!(!families.contains("JSXTag"));
            for entity in entities {
                assert_eq!(
                    &source[entity.name_raw().start()..entity.name_raw().end()],
                    entity.name_raw().text()
                );
                if let TsEntityExtent::Exact(extent) = entity.extent() {
                    assert_eq!(
                        &source[extent.range().start()..extent.range().end()],
                        extent.range().text()
                    );
                    assert!(extent.range().start() <= entity.name_raw().start());
                    assert!(extent.range().end() >= entity.name_raw().end());
                }
                if entity
                    .contributions()
                    .iter()
                    .all(|c| c.derived_value.is_none())
                {
                    assert!(
                        entity
                            .contributions()
                            .iter()
                            .all(|c| c.ecma_declaration.is_some() && c.go_declaration.is_none())
                    );
                }
            }
            if path == "names.js" {
                let duplicates = entities
                    .iter()
                    .filter(|e| e.identity().label() == "Variable" && e.name() == "a")
                    .collect::<Vec<_>>();
                assert_eq!(duplicates.len(), 2);
                assert_ne!(
                    duplicates[0].identity().uid().as_str(),
                    duplicates[1].identity().uid().as_str()
                );
                assert_eq!(
                    duplicates
                        .iter()
                        .map(|e| e.name_raw().text())
                        .collect::<BTreeSet<_>>(),
                    BTreeSet::from(["a", "\\u0061"])
                );
                for entity in duplicates {
                    assert!(entity.namespaces().is_empty());
                    assert!(
                        entity
                            .contributions()
                            .iter()
                            .all(|c| !c.excluded_context_indices.is_empty())
                    );
                    let TsEntityExtent::Exact(extent) = entity.extent() else {
                        panic!("complete actual declarator");
                    };
                    assert_eq!(
                        extent.range().text(),
                        if entity.name_raw().text() == "a" {
                            "a=1"
                        } else {
                            "\\u0061=2"
                        }
                    );
                    assert!(entity.model_conflicts().is_empty());
                }
                let literal = entities
                    .iter()
                    .find(|e| {
                        e.identity().label() == "Pair"
                            && e.name_representation()
                                == TsNameRepresentation::LiteralSource {
                                    native_kind: "string_fragment",
                                }
                    })
                    .expect("actual literal Pair");
                assert_eq!(literal.name(), "a");
                let value = entities
                    .iter()
                    .find(|e| {
                        e.contributions().iter().any(|c| {
                            c.derived_value
                                .as_ref()
                                .is_some_and(|d| d.owner_uid == literal.identity().uid().as_str())
                        })
                    })
                    .expect("literal Pair derived Value");
                assert_eq!(
                    value.namespaces().last().unwrap().selection,
                    TsOwnerSelection::DerivedDeclarationLiteralSource
                );
                assert_eq!(
                    value.namespaces().last().unwrap().anchor.text(),
                    literal.name_raw().text()
                );
            }
            if path == "component.tsx" {
                let render = entities
                    .iter()
                    .find(|e| e.identity().label() == "Method" && e.name() == "render")
                    .unwrap();
                let TsEntityCallable::Function(details) = render.callable() else {
                    panic!("real method metadata");
                };
                assert!(matches!(
                    details.complexity,
                    TsComplexity::Score { value: 3, .. }
                ));
                assert_eq!(details.export, TsExport::Statement(false));
                assert_eq!(
                    details
                        .signature
                        .ecma
                        .as_ref()
                        .unwrap()
                        .parameters
                        .as_ref()
                        .unwrap()
                        .raw
                        .unwrap()
                        .text(),
                    "(props:Props)"
                );
                let app = entities
                    .iter()
                    .find(|e| e.identity().label() == "Function" && e.name() == "App")
                    .unwrap();
                let TsEntityCallable::Function(details) = app.callable() else {
                    panic!("assigned native arrow");
                };
                assert_eq!(details.export, TsExport::Statement(true));
                assert_eq!(details.signature.raw.unwrap().text(), "(props:Props)=>");
            }
            if path == "local.js" {
                let relations = document.relations()?;
                let outcomes = document.local_relations()?;
                assert_eq!(
                    outcomes.len(),
                    relations.iter().map(|r| r.sources.len()).sum::<usize>()
                );
                let calls = outcomes
                    .iter()
                    .filter(|o| {
                        relations[o.relation_index]
                            .query
                            .value()
                            .relation_type()
                            .value()
                            == "CALLS"
                    })
                    .collect::<Vec<_>>();
                assert_eq!(calls.len(), 5);
                assert!(matches!(
                    calls[0].target,
                    TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::TemporalDeadZone { .. }
                    }
                ));
                let TsLocalTargetState::Ambiguous { entity_indices } = &calls[1].target else {
                    panic!("two actual F declarations");
                };
                assert_eq!(entity_indices.len(), 2);
                assert!(matches!(calls[2].target, TsLocalTargetState::NotFound));
                let TsLocalTargetState::Resolved { entity_index } = calls[3].target else {
                    panic!("native const G binding");
                };
                assert_eq!(entities[entity_index].name(), "G");
                assert_eq!(entities[entity_index].namespaces()[0].name, "caller");
                assert!(matches!(
                    calls[4].target,
                    TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::AnonymousSource { .. }
                    }
                ));
                let reads = outcomes
                    .iter()
                    .filter(|o| {
                        relations[o.relation_index]
                            .query
                            .value()
                            .relation_type()
                            .value()
                            == "READS_FIELD"
                    })
                    .collect::<Vec<_>>();
                assert_eq!(reads.len(), 3);
                assert!(matches!(
                    reads[0].target,
                    TsLocalTargetState::Resolved { .. }
                ));
                assert!(matches!(
                    reads[1].target,
                    TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::NotReadAccess { .. }
                    }
                ));
                assert!(matches!(
                    reads[2].target,
                    TsLocalTargetState::Resolved { .. }
                ));
                let writes = outcomes
                    .iter()
                    .filter(|o| {
                        relations[o.relation_index]
                            .query
                            .value()
                            .relation_type()
                            .value()
                            == "WRITES_FIELD"
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    writes.len(),
                    1,
                    "no augmented/computed WRITES capture invented"
                );
                assert!(matches!(
                    writes[0].target,
                    TsLocalTargetState::Resolved { .. }
                ));
            }
            if path == "recovery.tsx" {
                assert!(!document.diagnostics()?.is_empty());
                assert!(
                    entities.is_empty(),
                    "no native declaration owner may be invented under ERROR"
                );
                let observations = document.parsed()?.context_document()?.observations()?;
                assert!(
                    observations
                        .groups
                        .iter()
                        .any(|g| g.query.value().graph_label().value() == "Parameter"
                            && !g.matches.is_empty()),
                    "actual Parameter capture remains retained"
                );
            }
            let graph = assemble_graph_ts(
                AstGraphTsInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            assert_eq!(graph.nodes().len(), entities.len());
            assert_eq!(graph.association().path(), path);
            for node in graph.nodes() {
                let AstGraphValue::String(metadata) =
                    node.properties().get("metadata_json").unwrap()
                else {
                    panic!("owned metadata");
                };
                let metadata: serde_json::Value = serde_json::from_str(metadata)?;
                for contribution in metadata["contributions"].as_array().unwrap() {
                    if contribution["derived_value"].is_null() {
                        assert!(contribution["ecma_declaration"].is_object());
                    }
                    if contribution["callable"]["status"] == "Function" {
                        assert!(contribution["callable"]["ecma_signature"].is_object());
                    }
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}
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

fn with_model(
    text: &str,
    original: &SnapshotContext,
    draft: &CatalogDraft,
    path: &[&str],
    action: impl FnOnce(&TreeSitterSemanticDocument<'_, '_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    with_source(text, original, |input| {
        let factory = PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            builtin(draft.profiles()[0].grammar().value())?,
            original,
        )?;
        let parsed = ParsedTreeSitterDocument::collect(&factory, input, path, limits())?;
        let document = parsed.assemble()?;
        action(&document)
    })
}
async fn raw(
    temp: &TempDir,
    text: &str,
    path: &str,
    original: &Arc<SnapshotContext>,
) -> Result<IndexedSourceSnapshot, Box<dyn Error>> {
    let uri = temp.path().join("raw.lance");
    let batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec![path])),
            Arc::new(StringArray::from(vec![text])),
        ],
    )?;
    LanceModuleStore::create(uri.to_str().unwrap(), source_schema(), vec![batch]).await?;
    let target = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
        .await?
        .capture_target(Some("fixture"))
        .await?
        .materialize()
        .await?;
    Ok(read_branch_source(&target, path, original).await?)
}

#[tokio::test]
async fn genuine_ecma_review_scopes_block_false_edges_and_preserve_type_forward_refs() -> TestResult
{
    let js = include_str!("../../../crates/graphit-ast/profiles/queries/javascript.yaml");
    let value_rules = js.replace(
        "target_rules:\n",
        "target_rules:\n  INSTANTIATES: {labels: [Class]}\n",
    );
    let types = include_str!("../../../crates/graphit-ast/profiles/queries/typescript.yaml")
        .replace(
            "target_rules:\n",
            "target_rules:\n  IMPLEMENTS: {labels: [Interface]}\n",
        );
    for (path, source, profile) in [
        (
            "catch.js",
            "function Target(){} function Caller(){try{}catch(Target){Target();}}",
            js,
        ),
        (
            "catch_pattern.js",
            "function Target(){} function Caller(){try{}catch({value:Target}){Target();}}",
            js,
        ),
        (
            "annex.js",
            "function Target(){} function Caller(){if(flag){function Target(){}} Target();}",
            js,
        ),
        (
            "closer.js",
            "function Target(){} function Caller(){if(flag){function Target(){}} {const Target=()=>0;Target();}}",
            js,
        ),
        (
            "header.js",
            "function Target(){} function Caller(x=Target()){var Target;Target();}",
            js,
        ),
        (
            "forward.ts",
            "class C implements I {} interface I {}",
            types.as_str(),
        ),
        (
            "reverse.ts",
            "interface I {} class C implements I {}",
            types.as_str(),
        ),
        (
            "value_tdz.js",
            "function Caller(){new C(); class C {}}",
            value_rules.as_str(),
        ),
    ] {
        let temp = TempDir::new()?;
        let original = Arc::new(context());
        let snapshot = raw(&temp, source, path, &original).await?;
        let IndexedSource::Available(text) = snapshot.source(&original)? else {
            panic!("actual Raw Source");
        };
        let draft = catalog(profile.as_bytes());
        with_model(text, &original, &draft, &[path], |document| {
            let relations = document.relations()?;
            let outcomes = document.local_relations()?;
            let local = outcomes
                .iter()
                .filter(|o| {
                    matches!(
                        relations[o.relation_index]
                            .query
                            .value()
                            .relation_type()
                            .value()
                            .as_str(),
                        "CALLS" | "IMPLEMENTS" | "INSTANTIATES"
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                local.len(),
                if path == "header.js" { 2 } else { 1 },
                "{path}"
            );
            println!(
                "{path} {:?}",
                local.iter().map(|o| &o.target).collect::<Vec<_>>()
            );
            match path {
                "catch.js" | "catch_pattern.js" => assert!(matches!(
                    local[0].target,
                    TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::Shadowed { .. }
                    }
                )),
                "annex.js" => assert!(matches!(
                    local[0].target,
                    TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::UnsupportedSyntax { .. }
                    }
                )),
                "value_tdz.js" => assert!(matches!(
                    local[0].target,
                    TsLocalTargetState::Unresolved {
                        reason: TsLocalUnresolvedReason::TemporalDeadZone { .. }
                    }
                )),
                _ => {
                    let TsLocalTargetState::Resolved { entity_index } = local[0].target else {
                        panic!("{path} genuine target {:?}", local[0].target);
                    };
                    let target = &document.entities()?[entity_index];
                    assert_eq!(
                        target.name(),
                        if path.ends_with(".ts") { "I" } else { "Target" }
                    );
                    if path == "header.js" {
                        assert_eq!(target.name_raw().text(), "Target");
                        assert!(matches!(
                            local[1].target,
                            TsLocalTargetState::Unresolved {
                                reason: TsLocalUnresolvedReason::Shadowed { .. }
                            }
                        ));
                    }
                }
            }
            let graph = assemble_graph_ts(
                AstGraphTsInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            if matches!(
                path,
                "catch.js" | "catch_pattern.js" | "annex.js" | "value_tdz.js"
            ) {
                assert!(
                    graph.edges().is_empty(),
                    "{path} has no invented semantic edge"
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[tokio::test]
async fn genuine_ecma_custom_keys_properties_unsupported_names_and_absent_rules_survive()
-> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let path = "custom.mjs";
    let source = "function Target(){} function Caller(){Target();} class C{#secret(){} [key](){} }";
    let snapshot = raw(&temp, source, path, &original).await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("actual Raw Source");
    };
    let profile=include_str!("../../../crates/graphit-ast/profiles/queries/javascript.yaml")
        .replace("  CALLS: { labels: [Function, Method] }\n","")
        .replace("queries:\n",r#"queries:
  - data_key: bespoke
    graph_label: AstFileObservation
    span_capture: declaration
    pattern: '((function_declaration name: (identifier) @name) @declaration (#set! contract "ecma"))'
  - data_key: private_methods
    graph_label: OddMethod
    pattern: '(method_definition name: (private_property_identifier) @name)'
  - data_key: computed_methods
    graph_label: OddMethod
    pattern: '(method_definition name: (computed_property_name) @name)'
"#).replace("embed_labels:\n","embed_labels:\n  - OddMethod\n");
    let draft = catalog(profile.as_bytes());
    with_model(text, &original, &draft, &[path], |document| {
        let entities = document.entities()?;
        let bespoke = entities
            .iter()
            .filter(|e| e.data_key() == "bespoke")
            .collect::<Vec<_>>();
        assert_eq!(bespoke.len(), 2);
        assert!(
            bespoke
                .iter()
                .all(|e| e.identity().label() == "AstFileObservation")
        );
        let parsed = document.parsed()?;
        let groups = &parsed.context_document()?.observations()?.groups;
        let group = groups
            .iter()
            .find(|g| g.query.value().data_key().value() == "bespoke")
            .expect("configured custom key");
        assert_eq!(group.matches.len(), 2);
        assert!(group.matches.iter().all(|m| {
            m.properties
                .iter()
                .any(|p| p.key == "contract" && p.value.as_deref() == Some("ecma"))
        }));

        assert!(
            entities.iter().all(|e| e.identity().label() != "OddMethod"),
            "unsupported names do not fabricate canonical identity"
        );
        let unsupported_groups = groups
            .iter()
            .filter(|g| {
                matches!(
                    g.query.value().data_key().value().as_str(),
                    "private_methods" | "computed_methods"
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(unsupported_groups.len(), 2);
        assert!(unsupported_groups.iter().all(|g| g.matches.len() == 1));
        let actual_names = unsupported_groups
            .iter()
            .flat_map(|g| &g.matches)
            .flat_map(|m| &m.captures)
            .filter(|c| c.native.name == "name")
            .map(|c| c.native.text.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual_names, BTreeSet::from(["#secret", "[key]"]));
        let diagnostics = document
            .diagnostics()?
            .iter()
            .filter(|d| {
                matches!(
                    d.issue,
                    graphit_ast::tree_sitter_model::TsIssue::UnsupportedDeclarationFamily { .. }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 2);
        for d in diagnostics {
            let Some(graphit_ast::prepared_input::OriginalProjection::Exact(name)) = d.original
            else {
                panic!("actual unsupported name anchor");
            };
            assert!(actual_names.contains(name.text()));
        }
        let relations = document.relations()?;
        let outcome = document
            .local_relations()?
            .iter()
            .find(|o| {
                relations[o.relation_index]
                    .query
                    .value()
                    .relation_type()
                    .value()
                    == "CALLS"
            })
            .expect("raw call");
        assert!(relations[outcome.relation_index].target_rule.is_none());
        assert!(matches!(
            outcome.target,
            TsLocalTargetState::Unresolved {
                reason: TsLocalUnresolvedReason::MissingTargetRule
            }
        ));
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&snapshot),
            },
            &original,
        )?;
        assert_eq!(
            graph
                .nodes()
                .iter()
                .filter(|n| n.label() == "AstFileObservation")
                .count(),
            2
        );
        assert_eq!(graph.file_observation().label(), "AstFileObservation");
        assert!(
            graph
                .file_observation()
                .properties()
                .contains_key("is_file_observation")
        );
        assert!(graph.edges().is_empty());
        Ok(())
    })?;
    let ns_temp = TempDir::new()?;
    let ns_path = "namespace.ts";
    let ns_source = "namespace Outer {export namespace Inner {export function Target(){} }}";
    let ns_snapshot = raw(&ns_temp, ns_source, ns_path, &original).await?;
    let IndexedSource::Available(ns_text) = ns_snapshot.source(&original)? else {
        panic!("actual namespace Source");
    };
    let ns_profile = include_str!("../../../crates/graphit-ast/profiles/queries/typescript.yaml")
        .replace(
            "context_types:\n",
            "context_types:\n  internal_module: Namespace\n",
        );
    let ns_draft = catalog(ns_profile.as_bytes());
    with_model(ns_text, &original, &ns_draft, &[ns_path], |document| {
        let target = document
            .entities()?
            .iter()
            .find(|e| e.identity().label() == "Function" && e.name() == "Target")
            .expect("native namespaced function");
        assert_eq!(
            target
                .namespaces()
                .iter()
                .map(|n| n.name.as_str())
                .collect::<Vec<_>>(),
            ["Outer", "Inner"]
        );
        assert!(
            target
                .namespaces()
                .iter()
                .all(|n| !n.anchor.text().is_empty())
        );
        Ok(())
    })
}
