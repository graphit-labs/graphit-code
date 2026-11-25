#[path = "support/corpus_rows.rs"]
mod corpus_rows;
use arrow_array::{Array, ArrayRef, ListArray, RecordBatch, StringArray, StructArray};
use arrow_schema::Schema;
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    indexed_source::{IndexedSourceSnapshot, read_branch_source, source_schema},
    native_nn::{
        NativeNnError, NativeNnGenericInput, NativeNnModel, NativeNnSource,
        encode_native_nn_generic, validate_native_nn,
    },
    native_nn_query::{NativeNnMetric, NativeNnQueryError, query_corpus_nn},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    source_inspection::IndexedSource,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::builtin,
    tree_sitter_generic_model::{
        GenericCodeProjection, GenericProjectionIssue, PreparedGenericTreeSitterModel,
    },
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use std::{error::Error, sync::Arc};
use tempfile::TempDir;
type TestResult = Result<(), Box<dyn Error>>;
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
    let original = SnapshotContext::service(Cancellation::default());
    let id = ProfileId::new("native-nn-generic-fixture", 256).unwrap();
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

async fn capture_raw(
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
        .capture_target(Some("rust"))
        .await
        .unwrap()
        .materialize()
        .await
        .unwrap();
    assert_eq!(store.snapshot().version(), 1);
    read_branch_source(&named, path, original).await.unwrap()
}
fn strings<'a>(batch: &'a RecordBatch, name: &str) -> &'a StringArray {
    batch
        .column_by_name(name)
        .unwrap()
        .as_any()
        .downcast_ref()
        .unwrap()
}
fn replace(batch: &RecordBatch, name: &str, column: ArrayRef) -> RecordBatch {
    let mut columns = batch.columns().to_vec();
    columns[batch.schema().index_of(name).unwrap()] = column;
    RecordBatch::try_new(batch.schema(), columns).unwrap()
}

fn model() -> NativeNnModel<'static> {
    NativeNnModel {
        vector_id: [91; 32],
        dimensions: 2,
        model_id: "generic-codec-fixture",
        revision: "one",
    }
}

fn factory<'a>(
    profile: &'a graphit_ast::catalog::EffectiveProfile,
    original: &SnapshotContext,
) -> Result<PreparedGenericTreeSitterModel<'a>, Box<dyn Error>> {
    Ok(PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(
            profile,
            builtin(profile.grammar().value())?,
            original,
        )?,
        original,
    )?)
}

#[tokio::test]
async fn raw_names_schema_and_graph_fts_keep_original_tuple() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let text = "{\"café 日本\":{\"ação\":1}}";
    let raw = capture_raw(&temp, "source.lance", "model.native", text, &original).await;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("missing Source")
    };
    let draft = catalog(
        br#"language: native-fixture
grammar: tree-sitter-json
context_types: {pair: Pair}
context_name_queries: {pair: '(pair key: (_) @name) @context'}
embed_labels: [Pair]
queries:
  - graph_label: Pair
    data_key: pairs
    name_is_data: true
    pattern: '(pair key: (string) @name) @unit'
    span_capture: unit
"#,
    );
    let factory = factory(&draft.profiles()[0], &original)?;
    let segments = [OriginSegment::Direct {
        prepared: ByteSpan {
            start: 0,
            end: text.len() as u64,
        },
        original: ByteSpan {
            start: 0,
            end: text.len() as u64,
        },
    }];
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
    let parsed = factory.collect(&input, &["model.native"], limits())?;
    let document = parsed.assemble()?;
    assert_eq!(document.entities()?.len(), 2);
    let indices = [0, 1];
    let vectors = [vec![1., 0.], vec![0., 1.]];
    let encode = |indices: &[usize], vectors: &[Vec<f32>], generation| {
        encode_native_nn_generic(
            &model(),
            &[NativeNnGenericInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: indices,
                vectors,
                model_generation: generation,
            }],
            &original,
        )
    };
    let batch = encode(&indices, &vectors, Some(7))?;
    let pending = graphit_ast::native_nn::prepare_native_nn_generic(
        &document,
        NativeNnSource::Indexed(&raw),
        &indices,
        &original,
    )?;
    assert_eq!(
        pending.schema(),
        graphit_ast::native_nn::native_nn_projection_schema()
    );
    let completed = graphit_ast::native_nn::complete_native_nn(
        &pending,
        &model(),
        Some(7),
        &vectors,
        &original,
    )?;
    assert_eq!(completed, batch);

    assert_eq!(batch.schema().fields().len(), 21);
    assert_eq!(
        batch.schema().metadata()["graphit.ast.nn.format"],
        graphit_ast::native_nn::NATIVE_NN_FORMAT
    );
    assert_eq!(
        strings(&batch, "name").iter().collect::<Vec<_>>(),
        [Some("\"café 日本\""), Some("\"ação\"")]
    );
    assert!(
        strings(&batch, "name_kind")
            .iter()
            .all(|v| v == Some("NativeSource"))
    );
    let graph = graphit_ast::semantic_graph::assemble_graph_generic(
        graphit_ast::semantic_graph::AstGraphGenericInput {
            document: &document,
            source: NativeNnSource::Indexed(&raw),
        },
        &original,
    )?
    .into_parts();
    let fts = graphit_ast::entity_search::encode_file(&graph, text, &original)?;
    assert_eq!(
        strings(&fts, "name").iter().collect::<Vec<_>>(),
        [Some("café 日本"), Some("ação")]
    );
    assert_eq!(strings(&fts, "uid"), strings(&batch, "uid"));
    assert_eq!(strings(&fts, "data_key"), strings(&batch, "data_key"));
    let base = graphit_ast::corpus::schema();
    let mut fields = base.fields().to_vec();
    fields.extend(
        graphit_ast::corpus::vector_fields(&model())?
            .fields()
            .iter()
            .cloned(),
    );
    let schema = Arc::new(arrow_schema::Schema::new_with_metadata(
        fields,
        base.metadata().clone(),
    ));
    let corpus =
        graphit_ast::corpus::encode_file(schema, fts.clone(), pending.clone(), None, &original)?;
    assert_eq!(
        graphit_ast::corpus::projections(&corpus, &original)?,
        pending
    );
    let corpus = graphit_ast::corpus::attach_embeddings(&corpus, &batch, &model(), &original)?;
    assert_eq!(strings(&corpus, "name"), strings(&fts, "name"));
    assert_eq!(strings(&corpus, "projection_name"), strings(&batch, "name"));
    assert_eq!(
        graphit_ast::corpus::vectors(&corpus, &model(), &original)?,
        batch
    );

    assert!(matches!(
        encode(&[0, 0], &vectors, Some(7)),
        Err(NativeNnError::DuplicateIdentity)
    ));
    assert!(matches!(
        encode(&[0], &[vec![f32::NAN, 0.]], Some(7)),
        Err(NativeNnError::InvalidVector)
    ));
    assert!(matches!(
        encode(&[0], &[vec![1.]], Some(7)),
        Err(NativeNnError::InvalidVector)
    ));
    assert!(matches!(
        encode(&[0], &vectors[..1], None),
        Ok(ref batch) if batch.num_rows() > 0 && batch.column_by_name("model_generation").unwrap().null_count() == batch.num_rows()
    ));
    let foreign = Arc::new(SnapshotContext::service(Cancellation::default()));
    assert!(matches!(
        encode_native_nn_generic(
            &model(),
            &[NativeNnGenericInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &indices,
                vectors: &vectors,
                model_generation: Some(7)
            }],
            &foreign
        ),
        Err(NativeNnError::RequestMismatch)
    ));
    let mut metadata = batch.schema().metadata().clone();
    metadata.insert("graphit.ast.nn.format".into(), "ast-native-nn-2".into());
    let old = RecordBatch::try_new(
        Arc::new(Schema::new_with_metadata(
            batch.schema().fields().clone(),
            metadata,
        )),
        batch.columns().to_vec(),
    )?;
    assert!(matches!(
        validate_native_nn(&old, &model(), &original),
        Err(NativeNnError::SchemaMismatch)
    ));
    let published = corpus_rows::persist(
        &temp.path().join("corpus.lance"),
        fts,
        &batch,
        &model(),
        None,
        &original,
    )
    .await?;
    let matches = query_corpus_nn(
        &published,
        &model(),
        &vectors[0],
        2,
        NativeNnMetric::L2,
        &original,
    )
    .await?;
    let rows = matches.rows(&original)?;
    assert_eq!(rows.iter().map(RecordBatch::num_rows).sum::<usize>(), 2);
    for row_batch in rows {
        validate_native_nn(row_batch, &model(), &original)?;
        assert!(
            strings(row_batch, "name_kind")
                .iter()
                .all(|v| v == Some("NativeSource"))
        );
    }
    let old_uri = temp.path().join("tag2-nn.lance");
    LanceModuleStore::create(
        old_uri.to_str().ok_or("old NN path")?,
        old.schema(),
        vec![old.clone()],
    )
    .await?;
    let old_branches =
        LanceBranchStore::open(old_uri.to_str().ok_or("old NN path")?, old.schema(), None).await?;
    let old_target = old_branches.capture_target(None).await?;
    assert!(matches!(
        query_corpus_nn(
            &old_target,
            &model(),
            &vectors[0],
            2,
            NativeNnMetric::L2,
            &original
        )
        .await,
        Err(NativeNnQueryError::Operation(_))
    ));
    for (column, values) in [
        ("name_kind", vec!["Identifier", "NativeSource"]),
        ("name", vec!["café 日本", "\"ação\""]),
        ("language", vec!["Rust", "native-fixture"]),
    ] {
        let bad = replace(&batch, column, Arc::new(StringArray::from(values)));
        assert!(
            validate_native_nn(&bad, &model(), &original).is_err(),
            "accepted {column}"
        );
    }
    let other_raw = capture_raw(&temp, "foreign.lance", "model.native", text, &original).await;
    assert!(
        encode_native_nn_generic(
            &model(),
            &[NativeNnGenericInput {
                document: &document,
                source: NativeNnSource::Indexed(&other_raw),
                entity_indices: &indices,
                vectors: &vectors,
                model_generation: Some(7)
            }],
            &original
        )
        .is_err()
    );
    original.cancellation().cancel();
    assert!(encode(&indices, &vectors, Some(7)).is_err());
    Ok(())
}

#[tokio::test]
async fn namespace_frames_without_entity_queries_are_complete_and_native() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let raw = capture_raw(
        &temp,
        "source.lance",
        "namespace.cs",
        "namespace N { class C { int F() { return 1; } } }",
        &original,
    )
    .await;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("missing Source")
    };
    let draft = catalog(NAMESPACE_PROFILE);
    let factory = factory(&draft.profiles()[0], &original)?;
    let segments = [OriginSegment::Direct {
        prepared: ByteSpan {
            start: 0,
            end: text.len() as u64,
        },
        original: ByteSpan {
            start: 0,
            end: text.len() as u64,
        },
    }];
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
    let parsed = factory.collect(&input, &["namespace.cs"], limits())?;
    let document = parsed.assemble()?;
    assert!(document.entities()?[0].owners().is_empty());
    let frames = document.owner_frames(0)?.collect::<Vec<_>>();
    assert_eq!(
        frames
            .iter()
            .map(|f| (f.label, f.anchor.text()))
            .collect::<Vec<_>>(),
        [("Namespace", "N"), ("Class", "C")]
    );
    assert_eq!(frames[0].extent.unwrap().range().text(), text);
    assert_eq!(
        frames[1].extent.unwrap().range().text(),
        "class C { int F() { return 1; } }"
    );
    assert!(matches!(
        document.code_projections()?[0],
        GenericCodeProjection::Available { .. }
    ));
    let batch = encode_native_nn_generic(
        &model(),
        &[NativeNnGenericInput {
            document: &document,
            source: NativeNnSource::Indexed(&raw),
            entity_indices: &[0],
            vectors: &[vec![1., 0.]],
            model_generation: Some(3),
        }],
        &original,
    )?;
    let owners = batch
        .column_by_name("namespace")
        .unwrap()
        .as_any()
        .downcast_ref::<ListArray>()
        .unwrap()
        .value(0);
    let owners = owners.as_any().downcast_ref::<StructArray>().unwrap();
    let names = owners
        .column_by_name("name")
        .unwrap()
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(names.iter().collect::<Vec<_>>(), [Some("N"), Some("C")]);
    assert!(document.owner_frames(1).is_err());
    Ok(())
}

#[tokio::test]
async fn borrowed_embedded_input_preserves_original_names_and_cancellation() -> TestResult {
    let declaration_pattern =
        "(variable_declaration (variable_declarator name: (identifier) @name)) @unit";
    for (body, normalizer, expected_name, wrapper, available, pattern) in [
        ("var α;", "", "α", false, true, declaration_pattern),
        (
            "var &#x03B1;;",
            "numeric",
            "&#x03B1;",
            false,
            false,
            declaration_pattern,
        ),
        (
            "&#x03B1;",
            "numeric",
            "&#x03B1;",
            false,
            true,
            "(identifier) @name @unit",
        ),
        ("α", "", "α", true, false, declaration_pattern),
    ] {
        let temp = TempDir::new()?;
        let original = Arc::new(SnapshotContext::service(Cancellation::default()));
        let text = format!("<code><![CDATA[{body}]]></code>");
        let raw = capture_raw(&temp, "source.lance", "embedded.xml", &text, &original).await;
        let IndexedSource::Available(text) = raw.source(&original)? else {
            panic!("missing Source")
        };
        let host_yaml = format!(
            r#"language: native-host
grammar: tree-sitter-xml
context_types: {{}}
queries:
  - graph_label: Element
    data_key: elements
    pattern: '(element (STag (Name) @name)) @unit'
    span_capture: unit
embedded:
  - pattern: '(element (STag (Name) @_tag) (content (CDSect (CData) @body)) (#eq? @_tag "code"))'
    text_capture: body
    default: native-code
    normalize: '{normalizer}'
    wrap_prefix: '{}'
    wrap_suffix: '{}'
text_normalizers:
  numeric:
    numeric_char_refs: true
"#,
            if wrapper { "var " } else { "" },
            if wrapper { ";" } else { "" }
        );
        let host = catalog(host_yaml.as_bytes());
        let host_factory = factory(&host.profiles()[0], &original)?;
        let child_yaml = format!(
            r#"language: native-code
grammar: tree-sitter-javascript
context_types: {{}}
embed_labels: [Variable]
queries:
  - graph_label: Variable
    data_key: variables
    pattern: '{pattern}'
    span_capture: unit
"#
        );
        let child = catalog(child_yaml.as_bytes());
        let child_factory = factory(&child.profiles()[0], &original)?;
        let segments = [OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: text.len() as u64,
            },
            original: ByteSpan {
                start: 0,
                end: text.len() as u64,
            },
        }];
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
        let parsed = host_factory.collect(&input, &["embedded.xml"], limits())?;
        let prepared = parsed
            .prepare_embedded(0)?
            .map_err(|issue| format!("embedded preparation: {issue:?}"))?;
        let batch=prepared.with_input(|input| {
            let parsed=child_factory.collect(input,&["embedded.xml"],limits())?;
            let document=parsed.assemble()?;
            let projections=document.code_projections()?;
            assert!(std::ptr::eq(document.original_context()?,original.as_ref()));
            assert_eq!(document.entities()?.len(),1);
            assert_eq!(document.entities()?[0].name().raw().text(),expected_name);
            let result=encode_native_nn_generic(&model(),&[NativeNnGenericInput { document:&document,source:NativeNnSource::Indexed(&raw),entity_indices:&[0],vectors:&[vec![1.,0.]],model_generation:Some(5)}],&original);
            if available {
                assert!(matches!(projections[0],GenericCodeProjection::Available{..}),"body={body:?} normalizer={normalizer:?} unavailable={:?}",match &projections[0]{GenericCodeProjection::Unavailable{reasons,..}=>Some(reasons),_=>None});
                Ok::<_,Box<dyn Error>>(Some(result?))
            } else {
                assert!(matches!(&projections[0],GenericCodeProjection::Unavailable{reasons,..} if reasons.contains(&GenericProjectionIssue::ExtentUnavailable)));
                assert!(result.is_err());
                Ok(None)
            }
        })?;
        if let Some(batch) = batch {
            assert_eq!(strings(&batch, "name").value(0), expected_name);
        }
        let cancelled: Result<(), Box<dyn Error>> = prepared.with_input(|input| {
            let parsed = child_factory.collect(input, &["embedded.xml"], limits())?;
            let document = parsed.assemble()?;
            assert!(std::ptr::eq(
                document.original_context()?,
                original.as_ref()
            ));
            original.cancellation().cancel();
            assert!(document.original_context().is_err());
            let result = encode_native_nn_generic(
                &model(),
                &[NativeNnGenericInput {
                    document: &document,
                    source: NativeNnSource::Indexed(&raw),
                    entity_indices: &[0],
                    vectors: &[vec![1., 0.]],
                    model_generation: Some(5),
                }],
                &original,
            );
            assert!(result.is_err());
            result?;
            Ok(())
        });
        assert!(cancelled.is_err());
        let reused_after_cancel = prepared.with_input(|_| Ok::<_, Box<dyn Error>>(()));
        assert!(reused_after_cancel.is_err());
    }
    Ok(())
}

const NAMESPACE_PROFILE: &[u8] = br#"language: native-csharp
grammar: tree-sitter-c_sharp
context_types: {namespace_declaration: Namespace, class_declaration: Class}
context_name_queries: {namespace_declaration: '(namespace_declaration name: (_) @name) @context', class_declaration: '(class_declaration name: (_) @name) @context'}
embed_labels: [Function]
queries:
  - graph_label: Function
    data_key: functions
    pattern: '(method_declaration name: (identifier) @name) @unit'
    span_capture: unit
"#;

#[tokio::test]
async fn namespace_without_exact_extent_keeps_projection_unavailable() -> TestResult {
    use graphit_ast::prepared_input::SyntheticReason;
    let temp = TempDir::new()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let source = "namespace N { class C { int F() { return 1; } }";
    let raw = capture_raw(&temp, "source.lance", "namespace.cs", source, &original).await;
    let IndexedSource::Available(source) = raw.source(&original)? else {
        panic!("missing Source")
    };
    let prepared = format!("{source}}}");
    let n = source.len() as u64;
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan { start: 0, end: n },
            original: ByteSpan { start: 0, end: n },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: n,
                end: n + 1,
            },
            reason: SyntheticReason::Wrapper,
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
    let draft = catalog(NAMESPACE_PROFILE);
    let factory = factory(&draft.profiles()[0], &original)?;
    let parsed = factory.collect(&input, &["namespace.cs"], limits())?;
    let document = parsed.assemble()?;
    assert_eq!(document.entities()?.len(), 1);
    let frames = document.owner_frames(0)?.collect::<Vec<_>>();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].anchor.text(), "N");
    assert!(frames[0].extent.is_none());
    assert!(frames[0].conflicts.contains(
        &graphit_ast::tree_sitter_generic_model::GenericEntityIssue::NonExactContextExtent
    ));
    assert!(document.diagnostics()?.iter().any(|diagnostic| diagnostic.context_index.is_some() && diagnostic.issue == graphit_ast::tree_sitter_generic_model::GenericEntityIssue::NonExactContextExtent));
    assert!(
        matches!(&document.code_projections()?[0],GenericCodeProjection::Unavailable{reasons,..} if reasons.contains(&GenericProjectionIssue::OwnerExtentUnavailable))
    );
    assert!(
        encode_native_nn_generic(
            &model(),
            &[NativeNnGenericInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &[0],
                vectors: &[vec![1., 0.]],
                model_generation: Some(9)
            }],
            &original
        )
        .is_err()
    );
    Ok(())
}
