#[path = "support/corpus_rows.rs"]
mod corpus_rows;
use arrow_array::{Array, ArrayRef, RecordBatch, StringArray};
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    indexed_source::{IndexedSourceSnapshot, read_indexed_source, source_schema},
    native_nn::{
        NativeNnError, NativeNnJsonInput, NativeNnModel, NativeNnSource, encode_native_nn_json,
        validate_native_nn,
    },
    native_nn_query::{NativeNnMetric, query_corpus_nn},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_model::{JsonProjection, ParsedJsonDocument, SemanticModelError},
    source_inspection::IndexedSource,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::GrammarLanguage,
};
use graphit_auth::{ProjectCapture, ScopeCapture};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    installation_identity::InstallationIdentityStore,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    version_context::{Checkout, VersionContext},
};
use graphit_data::lance_module_store::LanceModuleStore;
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

fn model() -> NativeNnModel<'static> {
    NativeNnModel {
        vector_id: [83; 32],
        dimensions: 2,
        model_id: "actual-json-fixture",
        revision: "one",
    }
}
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
    read_indexed_source(&store.snapshot(), path, original)
        .await
        .unwrap()
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

#[tokio::test]
async fn genuine_json_names_namespace_selection_and_causal_rejection() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let body =
        r#"{"": {"escaped\u0000": 123456789012345678901234567890, "Ω": "\u0000"}, "flag": true}"#;
    let raw = raw(&temp, "source.lance", "data.json", body, &original).await;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("actual source")
    };
    let draft = custom(
        "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key: (string) @name)'\n",
    );
    let prepared = prepare_profile(&draft, &original)?;
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
    let parsed = ParsedJsonDocument::collect(&prepared, &input, &["data.json"], limits())?;
    let document = parsed.assemble()?;
    let indices: Vec<_> = document
        .code_projections()?
        .iter()
        .filter_map(|p| match p {
            JsonProjection::Available(p) => Some(p.entity_index),
            _ => None,
        })
        .collect();
    assert_eq!(indices.len(), 4);
    let vectors = vec![vec![1., 2.]; indices.len()];
    let encode = |indices: &[usize], vectors: &[Vec<f32>], original: &Arc<SnapshotContext>| {
        encode_native_nn_json(
            &model(),
            &[NativeNnJsonInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: indices,
                vectors,
                model_generation: Some(7),
            }],
            original,
        )
    };
    let batch = encode(&indices, &vectors, &original)?;
    let pending = graphit_ast::native_nn::prepare_native_nn_json(
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

    let remote = encode_native_nn_json(
        &model(),
        &[NativeNnJsonInput {
            document: &document,
            source: NativeNnSource::Indexed(&raw),
            entity_indices: &indices,
            vectors: &vectors,
            model_generation: None,
        }],
        &original,
    )?;
    validate_native_nn(&remote, &model(), &original)?;
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

    assert_eq!(batch.num_columns(), 21);
    assert!(
        strings(&batch, "name_kind")
            .iter()
            .all(|k| k == Some("JsonName"))
    );
    assert!(
        strings(&batch, "name")
            .iter()
            .flatten()
            .any(|n| n.is_empty())
    );
    assert!(
        strings(&batch, "name")
            .iter()
            .flatten()
            .any(|n| n == "escaped\0")
    );
    assert!(strings(&batch, "name").iter().flatten().any(|n| n == "Ω"));
    assert!(
        strings(&batch, "data_key")
            .iter()
            .flatten()
            .all(|n| n == "pairs")
    );
    for (row, index) in indices.iter().enumerate() {
        assert_eq!(
            strings(&batch, "uid").value(row),
            document.entities()[*index].identity().uid().as_str()
        );
    }
    let namespaces = batch
        .column_by_name("namespace")
        .unwrap()
        .as_any()
        .downcast_ref::<arrow_array::ListArray>()
        .unwrap();
    for (row, index) in indices.iter().enumerate() {
        let owners = namespaces.value(row);
        let owners = owners
            .as_any()
            .downcast_ref::<arrow_array::StructArray>()
            .unwrap();
        assert_eq!(owners.len(), document.entities()[*index].namespace().len());
        let owner_names = owners
            .column_by_name("name")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let owner_kinds = owners
            .column_by_name("kind")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for (owner_index, owner) in document.entities()[*index].namespace().iter().enumerate() {
            assert_eq!(owner_names.value(owner_index), owner.name.as_ref());
            assert_eq!(owner_kinds.value(owner_index), owner.label);
            assert_eq!(
                serde_json::from_str::<String>(owner.anchor.text())?,
                owner.name.as_ref()
            );
        }
    }
    validate_native_nn(&batch, &model(), &original)?;
    assert!(matches!(
        encode(&[usize::MAX], &[vec![1., 2.]], &original),
        Err(NativeNnError::InvalidInput)
    ));
    assert!(matches!(
        encode(
            &[indices[0], indices[0]],
            &[vec![1., 2.], vec![1., 2.]],
            &original
        ),
        Err(NativeNnError::DuplicateIdentity)
    ));
    assert!(matches!(
        encode(&indices, &[], &original),
        Err(NativeNnError::InvalidInput)
    ));
    assert!(matches!(
        encode(&[indices[0]], &[vec![f32::NAN, 2.]], &original),
        Err(NativeNnError::InvalidVector)
    ));
    assert!(matches!(
        encode(&[indices[0]], &[vec![1.]], &original),
        Err(NativeNnError::InvalidVector)
    ));
    let foreign = Arc::new(SnapshotContext::service(Cancellation::default()));
    assert!(matches!(
        encode(&indices, &vectors, &foreign),
        Err(NativeNnError::RequestMismatch)
    ));
    for (column, content) in [
        ("name", "wrong"),
        ("grammar", "arbitrary"),
        ("data_key", "bad\0key"),
    ] {
        let corrupt = replace(
            &batch,
            column,
            Arc::new(StringArray::from(vec![content; indices.len()])),
        );
        assert!(matches!(
            validate_native_nn(&corrupt, &model(), &original),
            Err(NativeNnError::InvalidRow)
        ));
    }
    let copy = text.to_owned();
    let copied = PreparedInput::new(
        copy.as_bytes(),
        copy.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let other_parsed = ParsedJsonDocument::collect(&prepared, &copied, &["data.json"], limits())?;
    let other = other_parsed.assemble()?;
    assert!(matches!(
        encode_native_nn_json(
            &model(),
            &[NativeNnJsonInput {
                document: &other,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &indices,
                vectors: &vectors,
                model_generation: Some(7)
            }],
            &original
        ),
        Err(NativeNnError::InvalidInput)
    ));
    let wrong_parsed = ParsedJsonDocument::collect(&prepared, &input, &["other.json"], limits())?;
    let wrong = wrong_parsed.assemble()?;
    assert!(matches!(
        encode_native_nn_json(
            &model(),
            &[NativeNnJsonInput {
                document: &wrong,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &indices,
                vectors: &vectors,
                model_generation: Some(7)
            }],
            &original
        ),
        Err(NativeNnError::InvalidInput)
    ));
    original.cancellation().cancel();
    assert!(matches!(
        encode(&indices, &vectors, &original),
        Err(NativeNnError::Operation(_))
    ));
    Ok(())
}

#[tokio::test]
async fn genuine_same_uid_distinct_data_keys_roundtrip_publish_and_query() -> TestResult {
    paired_roundtrip(1).await
}

#[tokio::test]
async fn bulk_embedding_checkpoint_preserves_exact_pairs_without_deep_planner_recursion()
-> TestResult {
    paired_roundtrip(2048).await
}

async fn paired_roundtrip(entries: usize) -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let source = serde_json::to_string(
        &(0..entries)
            .map(|i| (format!("answer_{i}"), i))
            .collect::<std::collections::BTreeMap<_, _>>(),
    )?;
    let raw = raw(&temp, "source.lance", "data.json", &source, &original).await;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("actual source")
    };
    let draft = custom(
        "  - data_key: first\n    graph_label: Custom\n    pattern: '(pair key: (string) @name)'\n  - data_key: second\n    graph_label: Custom\n    pattern: '(pair key: (string) @name)'\n",
    );
    let prepared = prepare_profile(&draft, &original)?;
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
    let parsed = ParsedJsonDocument::collect(&prepared, &input, &["data.json"], limits())?;
    let document = parsed.assemble()?;
    assert_eq!(document.entities().len(), 2 * entries);
    assert_eq!(
        document
            .entities()
            .iter()
            .filter(|entity| {
                entity.identity().uid().as_str() == document.entities()[0].identity().uid().as_str()
            })
            .count(),
        2
    );
    let indices = (0..document.entities().len()).collect::<Vec<_>>();
    let vectors = indices
        .iter()
        .map(|i| {
            if i % 2 == 0 {
                vec![1., 0.]
            } else {
                vec![0., 1.]
            }
        })
        .collect::<Vec<_>>();
    let batch = encode_native_nn_json(
        &model(),
        &[NativeNnJsonInput {
            document: &document,
            source: NativeNnSource::Indexed(&raw),
            entity_indices: &indices,
            vectors: &vectors,
            model_generation: Some(7),
        }],
        &original,
    )?;
    let graph = graphit_ast::semantic_graph::assemble_graph_json(
        graphit_ast::semantic_graph::AstGraphJsonInput {
            document: &document,
            source: NativeNnSource::Indexed(&raw),
        },
        &original,
    )?
    .into_parts();
    let lexical = graphit_ast::entity_search::encode_file(&graph, text, &original)?;
    let applied = corpus_rows::persist(
        &temp.path().join("corpus.lance"),
        lexical,
        &batch,
        &model(),
        None,
        &original,
    )
    .await?;
    let found = query_corpus_nn(
        &applied,
        &model(),
        &[1., 0.],
        2 * entries,
        NativeNnMetric::L2,
        &original,
    )
    .await?;
    let rows = found.rows(&original)?;
    let keys: Vec<_> = rows
        .iter()
        .flat_map(|b| strings(b, "data_key").iter().flatten())
        .collect();
    assert_eq!(keys.len(), 2 * entries);
    assert!(keys.contains(&"first") && keys.contains(&"second"));
    if entries > 1 {
        // Both categories exist for each UID. A partial checkpoint must not
        // accidentally match the cross product of independent UID/key lists.
        let uids = strings(&batch, "uid");
        let data_keys = strings(&batch, "data_key");
        let first = data_keys
            .iter()
            .position(|key| key == Some("first"))
            .unwrap();
        let second = (0..batch.num_rows())
            .find(|&i| data_keys.value(i) == "second" && uids.value(i) != uids.value(first))
            .unwrap();
        let subset = arrow_select::take::take_record_batch(
            &batch,
            &arrow_array::UInt32Array::from(vec![u32::try_from(first)?, u32::try_from(second)?]),
        )?;
        if let Some(update) =
            graphit_ast::corpus::prepare_embeddings(&applied, &subset, &model(), &original).await?
        {
            update.commit().await?;
        }
        return Ok(());
    }
    let duplicate = replace(
        &batch,
        "data_key",
        Arc::new(StringArray::from(vec!["first", "first"])),
    );
    assert!(matches!(
        validate_native_nn(&duplicate, &model(), &original),
        Err(NativeNnError::DuplicateIdentity)
    ));
    // NULL raw uses the actual project file capture, without inventing available text.
    let installation_paths = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )?;
    let timeout = std::time::Duration::from_secs(5);
    InstallationIdentityStore::new(&installation_paths)?
        .ensure(timeout, original.cancellation())?;
    std::fs::create_dir_all(installation_paths.global_path(&["runtime"])?)?;
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root)?;
    let project_paths = ProjectPaths::at(&project_root, &installation_paths)?;
    let identity = ProjectIdentityStore::at(&project_paths)
        .writer(timeout, original.cancellation())?
        .ensure(&ProjectMetadata::new("native-nn-physical")?)?;
    std::fs::create_dir_all(project_paths.project_runtime(identity.id(), &[])?)?;
    let project_scope = CapturedPublicationScope::project(
        &project_paths,
        identity.id(),
        ScopeLimits::new(16, 4096, 256)?,
        &original,
    )?;
    let project_record = project_scope.binding()?;
    let version = VersionContext::select(&project_paths, Checkout::NonGit, "", None, "")?;
    std::fs::write(project_paths.root().join("data.json"), text)?;
    let null_batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec!["data.json"])),
            Arc::new(StringArray::from(vec![None::<&str>])),
        ],
    )?;
    let null_store = LanceModuleStore::create(
        temp.path().join("null_raw.lance").to_str().unwrap(),
        source_schema(),
        vec![null_batch],
    )
    .await?;
    let null_raw = read_indexed_source(&null_store.snapshot(), "data.json", &original).await?;
    let missing = read_indexed_source(&null_store.snapshot(), "absent.json", &original).await?;
    let physical = graphit_ast::native_nn_physical_source::capture_native_nn_physical_source(
        ProjectCapture {
            scope: ScopeCapture {
                live: &project_scope,
                record: &project_record,
            },
            version: &version,
        },
        &["data.json"],
        graphit_core::artifact_store::CopyLimits::new(65536, 16, 4096, 8)?,
        graphit_core::artifact_store::RetainedBytesLimits::new(1, 1, 256)?,
        graphit_core::artifact_store::SourceReadLimits::new(65536, 1024, 65536)?,
        &original,
    )?;
    let physical_text = physical.source(&original)?;
    let physical_input = PreparedInput::new(
        physical_text.as_bytes(),
        physical_text.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let physical_parsed =
        ParsedJsonDocument::collect(&prepared, &physical_input, &["data.json"], limits())?;
    let physical_document = physical_parsed.assemble()?;
    let physical_encode = |raw| {
        encode_native_nn_json(
            &model(),
            &[NativeNnJsonInput {
                document: &physical_document,
                source: NativeNnSource::Physical {
                    capture: &physical,
                    raw,
                },
                entity_indices: &[0, 1],
                vectors: &[vec![1., 0.], vec![0., 1.]],
                model_generation: Some(7),
            }],
            &original,
        )
    };
    validate_native_nn(&physical_encode(&null_raw)?, &model(), &original)?;
    assert!(matches!(
        physical_encode(&missing),
        Err(NativeNnError::InvalidInput)
    ));
    assert!(matches!(
        physical_encode(&raw),
        Err(NativeNnError::InvalidInput)
    ));
    std::fs::write(project_paths.root().join("data.json"), "{\"changed\":true}")?;
    assert!(matches!(
        physical_encode(&null_raw),
        Err(NativeNnError::PhysicalSource(_))
    ));
    assert!(applied.version() > 0);
    Ok(())
}

#[tokio::test]
async fn unavailable_non_embedding_empty_and_raw_scalar_projections() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let cases = [
        (
            "conflict",
            r#"{"one":[1,2]}"#,
            "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name value:(array (number) @value))'\n    value_capture: value\n    value_label: Value\n",
            false,
        ),
        (
            "nonembed",
            r#"{"one":1}"#,
            "  - data_key: dormant\n    graph_label: Dormant\n    pattern: '(pair key:(string) @name)'\n",
            false,
        ),
        (
            "scalar",
            r#"[1e1000,true,false,null,""]"#,
            "  - data_key: scalars\n    graph_label: Value\n    pattern: '[(number)(true)(false)(null)(string)] @name'\n",
            true,
        ),
        (
            "empty",
            "{}",
            "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name)'\n",
            true,
        ),
    ];
    for (name, body, queries, available) in cases {
        let raw = raw(
            &temp,
            &format!("{name}.lance"),
            "data.json",
            body,
            &original,
        )
        .await;
        let IndexedSource::Available(text) = raw.source(&original)? else {
            panic!("actual source")
        };
        let draft = custom(queries);
        let prepared = prepare_profile(&draft, &original)?;
        let span = ByteSpan {
            start: 0,
            end: text.len() as u64,
        };
        let segments = [OriginSegment::Direct {
            prepared: span,
            original: span,
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
        let parsed = ParsedJsonDocument::collect(&prepared, &input, &["data.json"], limits())?;
        let document = parsed.assemble()?;
        let indices: Vec<_> = (0..document.entities().len()).collect();
        let vectors = vec![vec![1., 2.]; indices.len()];
        let encoded = encode_native_nn_json(
            &model(),
            &[NativeNnJsonInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &indices,
                vectors: &vectors,
                model_generation: (!indices.is_empty()).then_some(7),
            }],
            &original,
        );
        if available {
            let batch = encoded?;
            validate_native_nn(&batch, &model(), &original)?;
            if name == "scalar" {
                let names: Vec<_> = strings(&batch, "name").iter().flatten().collect();
                assert_eq!(names, ["1e1000", "true", "false", "null", ""]);
            } else {
                assert_eq!(batch.num_rows(), 0);
            }
            let changed = NativeNnModel {
                vector_id: [17; 32],
                ..model()
            };
            assert!(matches!(
                validate_native_nn(&batch, &changed, &original),
                Err(NativeNnError::SchemaMismatch)
            ));
        } else {
            assert!(matches!(encoded, Err(NativeNnError::InvalidInput)));
        }
    }
    let expired = SnapshotContext::new(std::time::Instant::now(), Cancellation::default());
    let empty = RecordBatch::new_empty(graphit_ast::native_nn::native_nn_schema(&model())?);
    assert!(matches!(
        validate_native_nn(&empty, &model(), &expired),
        Err(NativeNnError::Operation(_))
    ));
    Ok(())
}
