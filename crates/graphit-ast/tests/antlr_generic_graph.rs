use arrow_array::{FixedSizeBinaryArray, RecordBatch, StringArray, UInt64Array};
use graphit_ast::{
    antlr_engine::{
        generic_model::{GenericAntlrObservations, GenericAntlrTree, ParsedGenericAntlrDocument},
        preprocessing::{CobolFormat, prepare_cobol},
    },
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    indexed_source::{IndexedSourceSnapshot, read_branch_source, source_schema},
    native_nn::{NativeNnError, NativeNnSource},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_graph::{
        AstGraphError, AstGraphRelationTarget, AstGraphValue,
        antlr_generic::{AstGraphAntlrGenericInput, assemble_graph_antlr_generic},
    },
    source_inspection::IndexedSource,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_engine::ParseControl,
    tree_sitter_profile::PreparedTreeSitterProfile,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use std::{error::Error, sync::Arc};
use tempfile::TempDir;
type TestResult = Result<(), Box<dyn Error>>;

// Codec vectors exercise representation only; no inference/model runtime is claimed.
fn assert_nn<'profile: 'source, 'source>(
    document: &ParsedGenericAntlrDocument<'profile, 'source, '_, '_>,
    raw: &IndexedSourceSnapshot,
    graph: &graphit_ast::semantic_graph::AstSemanticGraphBatch,
    original: &Arc<SnapshotContext>,
    guards: bool,
) -> TestResult {
    use graphit_ast::native_nn::{
        NativeNnAntlrGenericInput, NativeNnAntlrGenericProjection, NativeNnModel,
        encode_native_nn_antlr_generic, native_nn_antlr_generic_projections, validate_native_nn,
    };
    let observed = native_nn_antlr_generic_projections(document, original)?;
    let indices = observed
        .iter()
        .filter_map(|projection| match projection {
            NativeNnAntlrGenericProjection::Available {
                entity_index,
                extent,
            } => {
                let range = extent.range();
                assert_eq!(range.text(), &document.source()[range.start()..range.end()]);
                Some(*entity_index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !indices.is_empty(),
        "at least one genuine projection in each dialect"
    );
    let model = NativeNnModel {
        vector_id: [77; 32],
        dimensions: 2,
        model_id: "antlr-generic-codec-fixture",
        revision: "one",
    };
    let vectors = vec![vec![0.25, 0.75]; indices.len()];
    let encode = |selected: &[usize], vectors: &[Vec<f32>], context: &Arc<SnapshotContext>| {
        encode_native_nn_antlr_generic(
            &model,
            &[NativeNnAntlrGenericInput {
                document,
                source: NativeNnSource::Indexed(raw),
                entity_indices: selected,
                vectors,
                model_generation: Some(7),
            }],
            context,
        )
    };
    let batch = encode(&indices, &vectors, original)?;
    let pending = graphit_ast::native_nn::prepare_native_nn_antlr_generic(
        document,
        NativeNnSource::Indexed(raw),
        &indices,
        original,
    )?;
    assert_eq!(
        pending.schema(),
        graphit_ast::native_nn::native_nn_projection_schema()
    );
    let completed =
        graphit_ast::native_nn::complete_native_nn(&pending, &model, Some(7), &vectors, original)?;
    assert_eq!(completed, batch);

    let remote = encode_native_nn_antlr_generic(
        &model,
        &[NativeNnAntlrGenericInput {
            document,
            source: NativeNnSource::Indexed(raw),
            entity_indices: &indices,
            vectors: &vectors,
            model_generation: None,
        }],
        original,
    )?;
    validate_native_nn(&remote, &model, original)?;
    assert_eq!(
        remote
            .column_by_name("model_generation")
            .unwrap()
            .null_count(),
        remote.num_rows()
    );
    assert_eq!(
        batch
            .column_by_name("model_generation")
            .unwrap()
            .null_count(),
        0
    );
    assert_eq!(remote.schema(), batch.schema());

    validate_native_nn(&batch, &model, original)?;
    let text = |name: &str, row: usize| {
        batch
            .column_by_name(name)
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(row)
    };
    for (row, &index) in indices.iter().enumerate() {
        let entity = &document.entities()[index];
        let uid: graphit_data::CodeUid = entity.uid().parse()?;
        let key = graphit_data::CodePrimaryKey::derive(uid, entity.data_key())?.to_string();
        let node = graph.nodes().iter().find(|node| node.key() == key).unwrap();
        for (column, key) in [
            ("uid", "uid"),
            ("data_key", "data_key"),
            ("name", "name"),
            ("path", "path"),
            ("language", "lang"),
            ("grammar", "grammar"),
        ] {
            let AstGraphValue::String(value) = &node.properties()[key] else {
                panic!("string")
            };
            assert_eq!(text(column, row), value);
        }
        assert_eq!(text("projection", row), entity.extent().unwrap().text());
        assert_eq!(
            batch
                .column_by_name("raw_version")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap()
                .value(row),
            graph.association().raw_version()
        );
        assert_eq!(
            batch
                .column_by_name("model_generation")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap()
                .value(row),
            7
        );
        assert_eq!(
            batch
                .column_by_name("full_source_sha256")
                .unwrap()
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .unwrap()
                .value(row),
            graph.association().source_sha256()
        );
    }
    if guards {
        let foreign = Arc::new(context());
        assert!(matches!(
            encode(&indices, &vectors, &foreign),
            Err(NativeNnError::RequestMismatch)
        ));
        assert!(matches!(
            encode(&[indices[0]], &[vec![f32::NAN, 0.75]], original),
            Err(NativeNnError::InvalidVector)
        ));
        assert!(matches!(
            encode(&[indices[0]], &[vec![0.25]], original),
            Err(NativeNnError::InvalidVector)
        ));
        assert!(matches!(
            encode(
                &[indices[0], indices[0]],
                &[vec![0.25, 0.75], vec![0.25, 0.75]],
                original
            ),
            Err(NativeNnError::DuplicateIdentity)
        ));
        assert!(matches!(
            encode(&[usize::MAX], &[vec![0.25, 0.75]], original),
            Err(NativeNnError::InvalidInput)
        ));
    }
    if let Some(index) = observed.iter().find_map(|p| match p {
        NativeNnAntlrGenericProjection::Unavailable { entity_index, .. } => Some(*entity_index),
        _ => None,
    }) {
        assert!(matches!(
            encode(&[index], &[vec![0.25, 0.75]], original),
            Err(NativeNnError::InvalidInput)
        ));
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

async fn raw(
    temp: &TempDir,
    name: &str,
    path: &str,
    text: Option<&str>,
    original: &Arc<SnapshotContext>,
) -> Result<IndexedSourceSnapshot, Box<dyn Error>> {
    let uri = temp.path().join(name);
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
async fn native_tsql_shipped_graph_keeps_source_extent_identity_and_observed_calls() -> TestResult {
    let original = Arc::new(context());
    let temp = TempDir::new()?;
    let body =
        "-- Ω🙂\r\nCREATE TABLE dbo.orders(id INT); SELECT id FROM dbo.orders; EXEC dbo.work;";
    let raw = raw(&temp, "tsql.lance", "model.sql", Some(body), &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("source")
    };
    let profile = catalog(include_bytes!("fixtures/catalog/shipped/tsql.yaml"));
    with_source(text, &original, |input| {
        let document = ParsedGenericAntlrDocument::collect(
            &profile.profiles()[0],
            input,
            &["model.sql"],
            limits(),
        )?;
        assert!(matches!(
            document.observations(),
            GenericAntlrObservations::TSql(_)
        ));
        assert!(document.observations().main().syntax.diagnostics.is_empty());
        let table = document
            .entities()
            .iter()
            .find(|entity| entity.label() == "Table")
            .expect("actual table");
        assert_eq!(table.name().text(), "dbo.orders");
        assert!(table.extent().unwrap().text().starts_with("CREATE TABLE"));
        let graph = assemble_graph_antlr_generic(
            AstGraphAntlrGenericInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_eq!(graph.association().path(), "model.sql");
        assert!(graph.nodes().iter().any(|n| n.label() == "Table"));
        assert!(!graph.outcomes().is_empty());
        assert!(
            graph
                .outcomes()
                .iter()
                .all(|o| matches!(o.target(), AstGraphRelationTarget::ObservedOnly))
        );
        assert!(graph.edges().iter().all(|edge| edge.kind() != "CALLS"));
        assert_nn(&document, &raw, &graph, &original, true)?;
        let json: serde_json::Value = serde_json::from_str(graph.diagnostics_json())?;
        assert!(
            json["occurrences"]
                .as_array()
                .unwrap()
                .iter()
                .any(|o| o["policy"]["relation_type"] == "CALLS")
        );
        let row = graph.nodes().iter().find(|n| n.label() == "Table").unwrap();
        assert_eq!(
            row.key(),
            graphit_data::CodePrimaryKey::derive(table.uid().parse()?, table.data_key())?
                .to_string()
        );
        assert_eq!(
            row.properties()["start_byte"],
            AstGraphValue::Int64(table.extent().unwrap().start() as i64)
        );
        assert_eq!(
            row.properties()["name_start_byte"],
            AstGraphValue::Int64(body.find("dbo.orders").unwrap() as i64)
        );
        Ok(())
    })
}

#[tokio::test]
async fn native_cobol_fixed_source_copy_uses_separate_tree_and_real_program_scope() -> TestResult {
    let original = Arc::new(context());
    let temp = TempDir::new()?;
    let body = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HELLO.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n           COPY BOOK.\n       PROCEDURE DIVISION.\n           STOP RUN.\n       END PROGRAM HELLO.\n";
    let raw = raw(&temp, "cobol.lance", "model.cbl", Some(body), &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("source")
    };
    let profile = catalog(include_bytes!("fixtures/catalog/shipped/cobol85.yaml"));
    let prepared = prepare_cobol(text, CobolFormat::Fixed, &original)?;
    prepared.with_origin_plan(|plan| -> TestResult {
        let input =
            PreparedInput::new(text.as_bytes(), prepared.text().as_bytes(), plan, &original)?;
        let document = ParsedGenericAntlrDocument::collect(
            &profile.profiles()[0],
            &input,
            &["model.cbl"],
            limits(),
        )?;
        let GenericAntlrObservations::Cobol(observed) = document.observations() else {
            panic!("two trees")
        };
        assert!(observed.main.syntax.diagnostics.is_empty());
        assert!(observed.preprocessor.syntax.diagnostics.is_empty());
        assert_eq!(observed.preprocessor.occurrences.len(), 1);
        assert!(matches!(
            observed.preprocessor.occurrences[0].scope,
            graphit_ast::antlr_engine::cobol_copy::CopyProgramScope::Linked { .. }
        ));
        assert!(document.entities().iter().any(|e| e.label() == "Program"));
        let program=document.entities().iter().find(|e|e.label()=="Program").unwrap();
        assert_eq!(program.extent_provenance(),Some(graphit_ast::antlr_engine::generic_model::GenericAntlrExtentProvenance::SequentialOriginalCoverage));
        assert!(program.extent().unwrap().text().contains("       DATA DIVISION."));
        let span=&observed.main.syntax.nodes[program.declaration().node_index].span;
        let span=span.as_ref().unwrap();
        assert!(matches!(input.project_prepared(span.start as u64,span.end as u64)?,graphit_ast::prepared_input::OriginalProjection::Compound(_)));
        let graph = assemble_graph_antlr_generic(
            AstGraphAntlrGenericInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_nn(&document, &raw, &graph, &original, false)?;
        let json: serde_json::Value = serde_json::from_str(graph.diagnostics_json())?;
        assert_eq!(json["preprocessor"]["tree"], "CobolPreprocessor");
        assert_eq!(
            json["preprocessor"]["occurrences"][0]["scope"]["state"],
            "Linked"
        );
        assert!(
            graph
                .outcomes()
                .iter()
                .any(|outcome| outcome.observation_json().contains("CobolPreprocessor"))
        );
        assert!(graph.edges().iter().all(|e| e.kind() != "CALLS"));
        Ok(())
    })
}

#[tokio::test]
async fn native_postgresql_host_and_plpgsql_bodies_preserve_scopes_decisions_and_unicode()
-> TestResult {
    let original = Arc::new(context());
    let temp = TempDir::new()?;
    let profile = catalog(include_bytes!("fixtures/catalog/shipped/postgresql.yaml"));
    let body_profile = catalog(include_bytes!("fixtures/catalog/shipped/plpgsql.yaml"));
    let language = graphit_ast::tree_sitter_engine::builtin("tree-sitter-plpgsql")?;
    let body_profile =
        PreparedTreeSitterProfile::prepare(&body_profile.profiles()[0], language, &original)?;
    let body = "-- Ω\r\nCREATE FUNCTION same() RETURNS void LANGUAGE plpgsql AS $$DECLARE v text := 'á'; BEGIN IF true THEN NULL; END IF; END;$$;\r\nCREATE PROCEDURE same() LANGUAGE plpgsql AS $MiXeD$DECLARE v text := 'β'; BEGIN IF true THEN NULL; END IF; END;$MiXeD$;";
    let raw = raw(&temp, "postgres.lance", "model.sql", Some(body), &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("source")
    };
    with_source(text, &original, |input| {
        assert!(
            ParsedGenericAntlrDocument::collect(
                &profile.profiles()[0],
                input,
                &["model.sql"],
                limits()
            )
            .is_err()
        );
        let document = ParsedGenericAntlrDocument::collect_postgresql(
            &profile.profiles()[0],
            &body_profile,
            input,
            &["model.sql"],
            limits(),
            ParseControl::default(),
        )?;
        let GenericAntlrObservations::PostgreSql(observed) = document.observations() else {
            panic!("body owner")
        };
        assert_eq!(observed.bodies.len(), 2);
        let variables = document
            .entities()
            .iter()
            .filter(|e| e.label() == "Variable")
            .collect::<Vec<_>>();
        assert_eq!(variables.len(), 2);
        assert_ne!(variables[0].uid(), variables[1].uid());
        assert_eq!(
            variables[0].declaration().tree,
            GenericAntlrTree::PostgreSqlBody(0)
        );
        assert_eq!(
            variables[1].declaration().tree,
            GenericAntlrTree::PostgreSqlBody(1)
        );
        assert_eq!(variables[0].owner_indices().len(), 1);
        assert_eq!(variables[1].owner_indices().len(), 1);
        assert_ne!(variables[0].owner_indices(), variables[1].owner_indices());
        let graph = assemble_graph_antlr_generic(
            AstGraphAntlrGenericInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_nn(&document, &raw, &graph, &original, false)?;
        let json: serde_json::Value = serde_json::from_str(graph.diagnostics_json())?;
        assert_eq!(json["bodies"].as_array().unwrap().len(), 2);
        for index in 0..2 {
            assert_eq!(
                json["bodies"][index]["decisions"].as_array().unwrap().len(),
                1
            );
            assert_eq!(
                json["bodies"][index]["host_contexts"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
        }
        assert!(
            graph
                .nodes()
                .iter()
                .all(|n| !n.properties().contains_key("cyclomatic_complexity"))
        );
        Ok(())
    })
}

#[tokio::test]
async fn generic_antlr_graph_rejects_foreign_original_allocation_path_and_cancellation()
-> TestResult {
    let original = Arc::new(context());
    let temp = TempDir::new()?;
    let profile = catalog(include_bytes!("fixtures/catalog/shipped/tsql.yaml"));
    let raw = raw(
        &temp,
        "guards.lance",
        "model.sql",
        Some("CREATE TABLE T(ID INT);"),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("source")
    };
    with_source(text, &original, |input| {
        let document = ParsedGenericAntlrDocument::collect(
            &profile.profiles()[0],
            input,
            &["model.sql"],
            limits(),
        )?;
        let foreign = Arc::new(context());
        assert!(
            matches!(assemble_graph_antlr_generic(AstGraphAntlrGenericInput{document:&document,source:NativeNnSource::Indexed(&raw)},&foreign),Err(AstGraphError::Source(e))if matches!(*e,NativeNnError::RequestMismatch))
        );
        Ok(())
    })?;
    let copied = text.to_owned();
    with_source(&copied, &original, |input| {
        let document = ParsedGenericAntlrDocument::collect(
            &profile.profiles()[0],
            input,
            &["model.sql"],
            limits(),
        )?;
        assert!(
            matches!(assemble_graph_antlr_generic(AstGraphAntlrGenericInput{document:&document,source:NativeNnSource::Indexed(&raw)},&original),Err(AstGraphError::Source(e))if matches!(*e,NativeNnError::InvalidInput))
        );
        Ok(())
    })?;
    with_source(text, &original, |input| {
        let document = ParsedGenericAntlrDocument::collect(
            &profile.profiles()[0],
            input,
            &["wrong.sql"],
            limits(),
        )?;
        assert!(
            assemble_graph_antlr_generic(
                AstGraphAntlrGenericInput {
                    document: &document,
                    source: NativeNnSource::Indexed(&raw)
                },
                &original
            )
            .is_err()
        );
        Ok(())
    })?;
    with_source(text, &original, |input| {
        let document = ParsedGenericAntlrDocument::collect(
            &profile.profiles()[0],
            input,
            &["model.sql"],
            limits(),
        )?;
        original.cancellation().cancel();
        assert!(
            matches!(assemble_graph_antlr_generic(AstGraphAntlrGenericInput{document:&document,source:NativeNnSource::Indexed(&raw)},&original),Err(AstGraphError::Operation(e))if e.kind()==std::io::ErrorKind::Interrupted)
        );
        assert!(
            ParsedGenericAntlrDocument::collect(
                &profile.profiles()[0],
                input,
                &["model.sql"],
                limits()
            )
            .is_err()
        );
        Ok(())
    })
}

#[test]
fn compound_cobol_names_stay_raw_and_are_not_published_as_exact_entities() -> TestResult {
    let original = context();
    let profile=catalog(b"language: cobol85\nparser: antlr4\ngrammar: antlr-cobol85\nqueries: [{data_key: values, graph_label: Value, pattern: '//literal', type: entity}]\n");
    let source = "000100 IDENTIFICATION DIVISION.\n000200 PROGRAM-ID. HELLO.\n000300 PROCEDURE DIVISION.\n000400     DISPLAY 'HEL\n000500-    'LO'.\n000600     STOP RUN.\n";
    let prepared = prepare_cobol(source, CobolFormat::Fixed, &original)?;
    prepared.with_origin_plan(|plan| -> TestResult {
        let input=PreparedInput::new(source.as_bytes(),prepared.text().as_bytes(),plan,&original)?;
        let document=ParsedGenericAntlrDocument::collect(&profile.profiles()[0],&input,&["model.cbl"],limits())?;
        assert!(document.entities().is_empty());
        let observed=document.observations().main();assert!(!observed.occurrences.is_empty());
        assert!(observed.occurrences.iter().any(|o|matches!(&o.name,graphit_ast::antlr_engine::semantic::CaptureBinding::Present(n)if matches!(n.original,Some(graphit_ast::prepared_input::OriginalProjection::Compound(_))))));
        Ok(())
    })
}

#[test]
fn missing_descendant_outside_name_path_does_not_publish_exact_declaration_extent() -> TestResult {
    let original = context();
    let profile = catalog(include_bytes!("fixtures/catalog/shipped/tsql.yaml"));
    with_source("CREATE TABLE T(ID INT;", &original, |input| {
        let document = ParsedGenericAntlrDocument::collect(
            &profile.profiles()[0],
            input,
            &["model.sql"],
            limits(),
        )?;
        assert!(!document.observations().main().syntax.diagnostics.is_empty());
        for entity in document.entities().iter().filter(|e| e.label() == "Table") {
            assert!(entity.extent().is_none());
        }
        Ok(())
    })
}

#[test]
fn physical_cobol_extent_rejects_unrecorded_gaps_synthetic_and_reordered_pieces() -> TestResult {
    use graphit_ast::prepared_input::{OriginSegment, SyntheticReason};
    let original = context();
    let profile = catalog(include_bytes!("fixtures/catalog/shipped/cobol85.yaml"));
    let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HELLO.\n       PROCEDURE DIVISION.\n           STOP RUN.\n       END PROGRAM HELLO.\n";
    let prepared = prepare_cobol(source, CobolFormat::Fixed, &original)?;
    prepared.with_origin_plan(|plan| -> TestResult {
        let (index, span, physical) = plan
            .segments
            .iter()
            .enumerate()
            .find_map(|(index, segment)| match segment {
                OriginSegment::Direct { prepared, original }
                    if source[original.start as usize..original.end as usize]
                        .contains("PROCEDURE DIVISION") =>
                {
                    Some((index, *prepared, *original))
                }
                _ => None,
            })
            .expect("real preprocessor direct segment after the program name");
        let fragments = [
            ByteSpan {
                start: physical.end - 1,
                end: physical.end,
            },
            ByteSpan {
                start: physical.start,
                end: physical.end - 1,
            },
        ];
        for case in 0..5 {
            let mut segments = plan.segments.to_vec();
            let mut removed = plan.removed_original.to_vec();
            if case == 1 {
                segments[index] = OriginSegment::Synthetic {
                    prepared: span,
                    reason: SyntheticReason::InsertedByTransform,
                };
                removed.push(physical);
                removed.sort_by_key(|span| span.start);
                let mut merged: Vec<ByteSpan> = Vec::new();
                for span in removed {
                    if let Some(last) = merged.last_mut()
                        && last.end == span.start
                    {
                        last.end = span.end;
                    } else {
                        merged.push(span);
                    }
                }
                removed = merged;
            }
            if case == 2 {
                segments[index] = OriginSegment::MappedAtom {
                    prepared: span,
                    original_fragments: &fragments,
                };
            }
            if case >= 3 {
                let segment = segments
                    .iter_mut()
                    .find(|segment| {
                        matches!(segment,
                            OriginSegment::Synthetic { prepared, .. } if prepared.start > span.end
                        )
                    })
                    .expect("real line padding inside the program body");
                if let OriginSegment::Synthetic { reason, .. } = segment {
                    *reason = if case == 3 {
                        SyntheticReason::Wrapper
                    } else {
                        SyntheticReason::RecoveryToken
                    };
                }
            }
            let changed = OriginPlan {
                segments: &segments,
                removed_original: if case == 0 { &[] } else { &removed },
                anchors: &[],
            };
            let input = PreparedInput::new(
                source.as_bytes(),
                prepared.text().as_bytes(),
                changed,
                &original,
            );
            if case == 0 {
                assert!(matches!(
                    input,
                    Err(graphit_ast::prepared_input::OriginError::InvalidPlan)
                ));
                continue;
            }
            let input = input?;
            let document = ParsedGenericAntlrDocument::collect(
                &profile.profiles()[0],
                &input,
                &["model.cbl"],
                limits(),
            )?;
            let program = document
                .entities()
                .iter()
                .find(|e| e.label() == "Program")
                .expect("unchanged exact name remains observed");
            assert!(
                program.extent().is_none(),
                "case {case} cannot authorize a physical envelope"
            );
            assert!(program.extent_provenance().is_none());
        }
        Ok(())
    })
}
