#[path = "support/corpus_rows.rs"]
mod corpus_rows;
use arrow_array::{Array, ArrayRef, RecordBatch, StringArray};
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    indexed_source::{
        IndexedSourceSnapshot, read_branch_source, read_indexed_source, source_schema,
    },
    native_nn::{
        NativeNnError, NativeNnModel, NativeNnSource, NativeNnTsInput, encode_native_nn_ts,
        validate_native_nn,
    },
    native_nn_query::{NativeNnMetric, query_corpus_nn},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    source_inspection::IndexedSource,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::GrammarLanguage,
    tree_sitter_model::{ParsedTreeSitterDocument, TsProjection},
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
    let id = ProfileId::new("semantic-rust-fixture", 256).unwrap();
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
    catalog(format!("language: rust\ngrammar: tree-sitter-rust\nanon_func_types: []\nembed_labels: [Struct, Field]\ncontext_types: {{struct_item: Record}}\nqueries:\n{queries}").as_bytes())
}
fn factory<'a>(
    profile: &'a graphit_ast::catalog::EffectiveProfile,
    original: &SnapshotContext,
) -> Result<PreparedTreeSitterContexts<'a>, Box<dyn Error>> {
    Ok(PreparedTreeSitterContexts::prepare(
        profile,
        GrammarLanguage::from_language(tree_sitter_rust::LANGUAGE.into())?,
        original,
    )?)
}

fn model() -> NativeNnModel<'static> {
    NativeNnModel {
        vector_id: [83; 32],
        dimensions: 2,
        model_id: "actual-rust-fixture",
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

#[tokio::test]
async fn genuine_rust_configured_data_keys_roundtrip_main_publish_query_and_physical_source()
-> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let raw = raw(
        &temp,
        "source.lance",
        "model.rs",
        "pub struct r#Type { pub r#type: u32, value: Option<u8>, other: bool }",
        &original,
    )
    .await;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("actual source")
    };
    let draft = custom(
        "  - {data_key: first, graph_label: Struct, pattern: '(struct_item name: (type_identifier) @name)'}\n  - {data_key: second, graph_label: Struct, pattern: '(struct_item name: (type_identifier) @name)'}\n  - {data_key: fields, graph_label: Field, pattern: '(field_declaration name: (field_identifier) @name)'}\n",
    );
    let profile = &draft.profiles()[0];
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
    let factory = factory(profile, &original)?;
    let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &["model.rs"], limits())?;
    let document = parsed.assemble()?;
    assert_eq!(document.entities()?.len(), 5);
    assert_eq!(
        document.entities()?[0].identity().uid().as_str(),
        document.entities()?[1].identity().uid().as_str()
    );
    let batch = encode_native_nn_ts(
        &model(),
        &[NativeNnTsInput {
            document: &document,
            source: NativeNnSource::Indexed(&raw),
            entity_indices: &[0, 1, 2, 3, 4],
            vectors: &[
                vec![1., 0.],
                vec![0., 1.],
                vec![1., 1.],
                vec![2., 1.],
                vec![3., 1.],
            ],
            model_generation: Some(7),
        }],
        &original,
    )?;
    let graph = graphit_ast::semantic_graph::assemble_graph_ts(
        graphit_ast::semantic_graph::AstGraphTsInput {
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
        5,
        NativeNnMetric::L2,
        &original,
    )
    .await?;
    let rows = found.rows(&original)?;
    let keys: Vec<_> = rows
        .iter()
        .flat_map(|b| strings(b, "data_key").iter().flatten())
        .collect();
    assert_eq!(keys.len(), 5);
    assert!(keys.contains(&"first") && keys.contains(&"second"));
    assert_eq!(keys.iter().filter(|key| **key == "fields").count(), 3);
    for queried in rows {
        validate_native_nn(queried, &model(), &original)?;
        for row in 0..queried.num_rows() {
            let key = strings(queried, "data_key").value(row);
            let uid = strings(queried, "uid").value(row);
            let actual = (0..batch.num_rows())
                .find(|i| {
                    strings(&batch, "data_key").value(*i) == key
                        && strings(&batch, "uid").value(*i) == uid
                })
                .unwrap();
            for field in [
                "name",
                "kind",
                "projection",
                "raw_origin",
                "raw_physical_branch",
            ] {
                assert_eq!(
                    strings(queried, field).value(row),
                    strings(&batch, field).value(actual)
                );
            }
            assert_eq!(
                queried
                    .column_by_name("namespace")
                    .unwrap()
                    .slice(row, 1)
                    .to_data(),
                batch
                    .column_by_name("namespace")
                    .unwrap()
                    .slice(actual, 1)
                    .to_data()
            );
        }
    }
    let duplicate = replace(
        &batch,
        "data_key",
        Arc::new(StringArray::from(vec!["first"; batch.num_rows()])),
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
    std::fs::write(project_paths.root().join("model.rs"), text)?;
    let null_batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec!["model.rs"])),
            Arc::new(StringArray::from(vec![None::<&str>])),
        ],
    )?;
    let null_store = LanceModuleStore::create(
        temp.path().join("null_raw.lance").to_str().unwrap(),
        source_schema(),
        vec![null_batch],
    )
    .await?;
    let null_raw = read_indexed_source(&null_store.snapshot(), "model.rs", &original).await?;
    let missing = read_indexed_source(&null_store.snapshot(), "absent.rs", &original).await?;
    let physical = graphit_ast::native_nn_physical_source::capture_native_nn_physical_source(
        ProjectCapture {
            scope: ScopeCapture {
                live: &project_scope,
                record: &project_record,
            },
            version: &version,
        },
        &["model.rs"],
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
        ParsedTreeSitterDocument::collect(&factory, &physical_input, &["model.rs"], limits())?;
    let physical_document = physical_parsed.assemble()?;
    let physical_encode = |raw| {
        encode_native_nn_ts(
            &model(),
            &[NativeNnTsInput {
                document: &physical_document,
                source: NativeNnSource::Physical {
                    capture: &physical,
                    raw,
                },
                entity_indices: &[0, 1, 2, 3, 4],
                vectors: &[
                    vec![1., 0.],
                    vec![0., 1.],
                    vec![1., 1.],
                    vec![2., 1.],
                    vec![3., 1.],
                ],
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
    std::fs::write(project_paths.root().join("model.rs"), "{\"changed\":true}")?;
    assert!(matches!(
        physical_encode(&null_raw),
        Err(NativeNnError::PhysicalSource(_))
    ));
    assert!(applied.version() > 0);
    Ok(())
}

#[tokio::test]
async fn stock_rust_full_declarations_and_causal_association_validation() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let body = "pub struct r#Type { pub r#type: u32, value: Option<u8>, other: bool }";
    let raw = raw(&temp, "source.lance", "model.rs", body, &original).await;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("actual source")
    };
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/rust.yaml"
    ));
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
    let factory = factory(&draft.profiles()[0], &original)?;
    let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &["model.rs"], limits())?;
    let document = parsed.assemble()?;
    let indices: Vec<_> = document
        .code_projections()?
        .iter()
        .filter_map(|p| match p {
            TsProjection::Available(p) => Some(p.entity_index),
            _ => None,
        })
        .collect();
    assert_eq!(indices.len(), 4);
    let vectors = vec![vec![1., 2.]; indices.len()];
    let encode = |indices: &[usize], vectors: &[Vec<f32>], original: &Arc<SnapshotContext>| {
        encode_native_nn_ts(
            &model(),
            &[NativeNnTsInput {
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
    assert_eq!(batch.num_columns(), 21);
    assert!(
        strings(&batch, "name_kind")
            .iter()
            .all(|kind| kind == Some("Identifier"))
    );
    assert_eq!(strings(&batch, "raw_origin").value(0), "branch");
    assert_eq!(strings(&batch, "raw_physical_branch").value(0), "rust");
    assert_eq!(
        batch
            .column_by_name("raw_version")
            .unwrap()
            .as_any()
            .downcast_ref::<arrow_array::UInt64Array>()
            .unwrap()
            .value(0),
        raw.version()
    );
    assert!(
        strings(&batch, "name")
            .iter()
            .flatten()
            .any(|n| n == "type")
    );
    for (row, index) in indices.iter().enumerate() {
        let entity = &document.entities()?[*index];
        assert_eq!(
            strings(&batch, "uid").value(row),
            entity.identity().uid().as_str()
        );
        assert_eq!(
            strings(&batch, "projection").value(row),
            document
                .code_projections()?
                .iter()
                .find_map(|p| match p {
                    TsProjection::Available(p) if p.entity_index == *index => Some(p.text),
                    _ => None,
                })
                .unwrap()
        );
        if entity.identity().label() == "Field" {
            assert_eq!(entity.namespaces().len(), 1);
            assert_eq!(entity.namespaces()[0].name, "Type");
            let owners = batch
                .column_by_name("namespace")
                .unwrap()
                .as_any()
                .downcast_ref::<arrow_array::ListArray>()
                .unwrap()
                .value(row);
            let owners = owners
                .as_any()
                .downcast_ref::<arrow_array::StructArray>()
                .unwrap();
            let names = owners
                .column_by_name("name")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            assert_eq!(names.value(0), "Type");
        }
    }
    validate_native_nn(&batch, &model(), &original)?;
    for (indices, vectors) in [
        (&[usize::MAX][..], vec![vec![1., 2.]]),
        (&indices[..], vec![]),
    ] {
        assert!(matches!(
            encode(indices, &vectors, &original),
            Err(NativeNnError::InvalidInput)
        ));
    }
    assert!(matches!(
        encode(
            &[indices[0], indices[0]],
            &[vec![1., 2.], vec![1., 2.]],
            &original
        ),
        Err(NativeNnError::DuplicateIdentity)
    ));
    for vector in [vec![f32::NAN, 2.], vec![f32::INFINITY, 2.], vec![1.]] {
        assert!(matches!(
            encode(&[indices[0]], &[vector], &original),
            Err(NativeNnError::InvalidVector)
        ));
    }
    let foreign = Arc::new(SnapshotContext::service(Cancellation::default()));
    assert!(matches!(
        encode(&indices, &vectors, &foreign),
        Err(NativeNnError::RequestMismatch)
    ));
    for (column, content) in [
        ("name", "wrong"),
        ("name", "bad\0name"),
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
    for altered in [
        NativeNnModel {
            dimensions: 3,
            ..model()
        },
        NativeNnModel {
            revision: "other",
            ..model()
        },
        NativeNnModel {
            vector_id: [4; 32],
            ..model()
        },
    ] {
        assert!(matches!(
            validate_native_nn(&batch, &altered, &original),
            Err(NativeNnError::SchemaMismatch)
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
    let copied_parsed =
        ParsedTreeSitterDocument::collect(&factory, &copied, &["model.rs"], limits())?;
    let other = copied_parsed.assemble()?;
    let wrong_parsed =
        ParsedTreeSitterDocument::collect(&factory, &input, &["other.rs"], limits())?;
    let wrong = wrong_parsed.assemble()?;
    for other in [&other, &wrong] {
        assert!(matches!(
            encode_native_nn_ts(
                &model(),
                &[NativeNnTsInput {
                    document: other,
                    source: NativeNnSource::Indexed(&raw),
                    entity_indices: &indices,
                    vectors: &vectors,
                    model_generation: Some(7)
                }],
                &original
            ),
            Err(NativeNnError::InvalidInput)
        ));
    }
    assert!(matches!(
        encode_native_nn_ts(
            &model(),
            &[NativeNnTsInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &indices,
                vectors: &vectors,
                model_generation: None
            }],
            &original
        ),
        Ok(ref batch) if batch.num_rows() > 0 && batch.column_by_name("model_generation").unwrap().null_count() == batch.num_rows()
    ));
    assert!(matches!(
        encode_native_nn_ts(
            &NativeNnModel {
                dimensions: 0,
                ..model()
            },
            &[],
            &original
        ),
        Err(NativeNnError::InvalidModel)
    ));
    original.cancellation().cancel();
    assert!(matches!(
        encode(&indices, &vectors, &original),
        Err(NativeNnError::Operation(_))
    ));
    Ok(())
}

#[tokio::test]
async fn unavailable_conflicting_nonexact_and_nonembedding_selections_are_rejected() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let good = "  - {data_key: structs, graph_label: Struct, pattern: '(struct_item name: (type_identifier) @name)'}\n";
    let partial = "  - {data_key: structs, graph_label: Struct, span_capture: span, pattern: '(struct_item name: (type_identifier) @name body: (field_declaration_list) @span)'}\n";
    for (i, query, nonembedding) in [
        (0, format!("{good}{partial}"), false),
        (1, format!("{partial}{good}"), false),
        (2, good.to_owned(), true),
        (3, "  - {data_key: structs, graph_label: Struct, span_capture: absent, pattern: '(struct_item name: (type_identifier) @name)'}\n".to_owned(), false),
    ] {
        let raw = raw(&temp, &format!("unavailable{i}.lance"), "model.rs", "struct S { a: u8 }", &original).await;
        let IndexedSource::Available(text) = raw.source(&original)? else { panic!("source") };
        let mut draft_bytes = format!("language: rust\ngrammar: tree-sitter-rust\ncontext_types: {{struct_item: Struct}}\nanon_func_types: []\nembed_labels: [Struct]\nqueries:\n{query}");
        if nonembedding { draft_bytes = draft_bytes.replace("embed_labels: [Struct]", "embed_labels: [Field]"); }
        let draft = catalog(draft_bytes.as_bytes());
        let segments = [OriginSegment::Direct {prepared: ByteSpan {start: 0, end: text.len() as u64}, original: ByteSpan {start: 0, end: text.len() as u64}}];
        let input = PreparedInput::new(text.as_bytes(), text.as_bytes(), OriginPlan {segments: &segments, removed_original: &[], anchors: &[]}, &original)?;
        let factory = factory(&draft.profiles()[0], &original)?;
        let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &["model.rs"], limits())?;
        let document = parsed.assemble()?;
        assert!(!document.entities()?.is_empty());
        let input = |selected, vectors, generation| NativeNnTsInput {document: &document, source: NativeNnSource::Indexed(&raw), entity_indices: selected, vectors, model_generation: generation};
        let vectors = [vec![1., 2.]];
        assert!(matches!(encode_native_nn_ts(&model(), &[input(&[0], &vectors, Some(7))], &original), Err(NativeNnError::InvalidInput)));
        let empty = encode_native_nn_ts(&model(), &[input(&[], &[], None)], &original)?;
        assert_eq!(empty.num_rows(), 0);
        assert_eq!(empty.num_columns(), 21);
    }
    // Original projection gaps cannot be promoted to persisted NN rows.
    let raw = raw(
        &temp,
        "gapped.lance",
        "model.rs",
        "struct S { a: u8 }",
        &original,
    )
    .await;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("source")
    };
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/rust.yaml"
    ));
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan { start: 0, end: 10 },
            original: ByteSpan { start: 0, end: 10 },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: 10,
                end: text.len() as u64,
            },
            reason: graphit_ast::prepared_input::SyntheticReason::InsertedByTransform,
        },
    ];
    let removed = [ByteSpan {
        start: 10,
        end: text.len() as u64,
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
    let factory = factory(&draft.profiles()[0], &original)?;
    let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &["model.rs"], limits())?;
    let document = parsed.assemble()?;
    assert!(
        document
            .code_projections()?
            .iter()
            .any(|p| matches!(p, TsProjection::Unavailable(_)))
    );
    assert!(matches!(
        encode_native_nn_ts(
            &model(),
            &[NativeNnTsInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &[0],
                vectors: &[vec![1., 2.]],
                model_generation: Some(7)
            }],
            &original
        ),
        Err(NativeNnError::InvalidInput)
    ));
    Ok(())
}

#[tokio::test]
async fn configured_impl_type_namespaces_preserve_native_type_expressions() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let raw = raw(
        &temp,
        "type_owner.lance",
        "model.rs",
        "impl crate::module::S<u8> { const C: () = { struct T { a:u8 } () }; }",
        &original,
    )
    .await;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("source")
    };
    let draft = catalog(b"language: rust\ngrammar: tree-sitter-rust\nanon_func_types: []\nembed_labels: [Struct, Field]\ncontext_types: {impl_item: Record, struct_item: Struct}\ncontext_name_queries: {impl_item: '(impl_item type: (_) @name) @context'}\nqueries:\n  - {data_key: structs, graph_label: Struct, pattern: '(struct_item name: (type_identifier) @name)'}\n  - {data_key: fields, graph_label: Field, pattern: '(field_declaration name: (field_identifier) @name)'}\n");
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
    let factory = factory(&draft.profiles()[0], &original)?;
    let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &["model.rs"], limits())?;
    let document = parsed.assemble()?;
    let batch = encode_native_nn_ts(
        &model(),
        &[NativeNnTsInput {
            document: &document,
            source: NativeNnSource::Indexed(&raw),
            entity_indices: &[0, 1],
            vectors: &[vec![1., 2.], vec![2., 1.]],
            model_generation: Some(7),
        }],
        &original,
    )?;
    assert_eq!(batch.num_rows(), 2);
    for row in 0..batch.num_rows() {
        let owners = batch
            .column_by_name("namespace")
            .unwrap()
            .as_any()
            .downcast_ref::<arrow_array::ListArray>()
            .unwrap()
            .value(row);
        let owners = owners
            .as_any()
            .downcast_ref::<arrow_array::StructArray>()
            .unwrap();
        let names = owners
            .column_by_name("name")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(names.value(0), "crate::module::S<u8>");
    }
    validate_native_nn(&batch, &model(), &original)?;
    Ok(())
}
