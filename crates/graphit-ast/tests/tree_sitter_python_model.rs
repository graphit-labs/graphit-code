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
        TsEntityDeclaredType, TsEntityExtent, TsExport, TsLocalTargetState, TsProjection,
    },
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use std::{collections::BTreeSet, error::Error, sync::Arc};
use tempfile::TempDir;
type TestResult = Result<(), Box<dyn Error>>;

#[tokio::test]
async fn genuine_python_stock_families_preserve_native_owners_values_policies_and_owned_graph()
-> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let path = "native_roles.py";
    let snapshot = raw(
        &temp,
        include_str!("fixtures/python/native_roles.py"),
        path,
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("actual stored Raw Source");
    };
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/python.yaml"
    ));
    let mut owned = None;
    with_model(text, &original, &draft, &[path], |document| {
        let entities = document.entities()?;
        let names = |label: &str| {
            entities
                .iter()
                .filter(|e| e.identity().label() == label)
                .map(|e| e.name().to_owned())
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(
            names("Class"),
            BTreeSet::from(["Base".into(), "Child".into()])
        );
        assert_eq!(
            names("Function"),
            BTreeSet::from([
                "run".into(),
                "helper".into(),
                "caller".into(),
                "bindings".into()
            ])
        );
        assert_eq!(
            names("Import"),
            BTreeSet::from(["tools".into(), "pkg.core".into()])
        );
        assert_eq!(
            names("Variable"),
            BTreeSet::from(["BASE".into(), "callback".into()])
        );
        assert_eq!(
            names("Parameter"),
            BTreeSet::from([
                "self".into(),
                "x".into(),
                "y".into(),
                "z".into(),
                "value".into(),
                "arg".into(),
                "items".into()
            ])
        );
        assert_eq!(names("Value"), BTreeSet::from(["1".into()]));
        assert!(names("Method").is_empty() && names("Comment").is_empty());
        let variable = entities.iter().find(|e| e.name() == "BASE").unwrap();
        assert_eq!(variable.contributions().len(), 2);
        let value = entities
            .iter()
            .find(|e| e.identity().label() == "Value")
            .unwrap();
        assert!(value.contributions()[0].derived_value.is_some());
        assert_eq!(value.namespaces().last().unwrap().name, "BASE");
        for entity in entities {
            assert_eq!(
                entity.name_raw().text(),
                &text[entity.name_raw().start()..entity.name_raw().end()]
            );
            assert!(
                matches!(entity.extent(), TsEntityExtent::Exact(_)),
                "unavailable extent {}",
                entity.name()
            );
        }
        let child = entities
            .iter()
            .find(|e| e.name() == "Child" && e.identity().label() == "Class")
            .unwrap();
        let TsEntityExtent::Exact(extent) = child.extent() else {
            panic!("native wrapper extent");
        };
        assert!(extent.range().text().starts_with("@mark\nclass Child"));
        let base = entities
            .iter()
            .find(|e| e.name() == "Base" && e.identity().label() == "Class")
            .unwrap();
        let leading = base.contributions()[0]
            .python_declaration
            .as_ref()
            .unwrap()
            .leading_string
            .as_ref()
            .unwrap();
        assert_eq!(leading.kind, "string");
        assert!(leading.raw.unwrap().text().contains("Authored multiline"));
        let run = entities.iter().find(|e| e.name() == "run").unwrap();
        assert_eq!(
            run.namespaces()
                .iter()
                .map(|n| n.name.as_str())
                .collect::<Vec<_>>(),
            ["Child"]
        );
        let TsEntityCallable::Function(function) = run.callable() else {
            panic!("Python native function");
        };
        assert!(matches!(
            function.complexity,
            TsComplexity::Score { value: 3, .. }
        ));
        assert_eq!(function.export, TsExport::NamePrefix(true));
        let signature = function.signature.python.as_ref().unwrap();
        assert_eq!(
            signature.return_type.as_ref().unwrap().raw.unwrap().text(),
            "int"
        );
        assert!(
            signature
                .parameters
                .as_ref()
                .unwrap()
                .raw
                .unwrap()
                .text()
                .contains("z: str=\"a\"")
        );
        let x = entities
            .iter()
            .find(|e| e.name() == "x" && e.identity().label() == "Parameter")
            .unwrap();
        assert_eq!(
            x.namespaces()
                .iter()
                .map(|n| n.name.as_str())
                .collect::<Vec<_>>(),
            ["Child", "run"]
        );
        let TsEntityDeclaredType::Native(ty) = x.declared_type() else {
            panic!("native annotated parameter");
        };
        assert_eq!(ty.raw.unwrap().text(), "int");
        assert!(
            document
                .code_projections()?
                .iter()
                .filter(|p| matches!(p, TsProjection::Available(_)))
                .count()
                >= 8
        );
        assert!(
            document
                .local_relations()?
                .iter()
                .any(|r| matches!(r.target, TsLocalTargetState::Resolved { .. }))
        );
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&snapshot),
            },
            &original,
        )?;
        assert_eq!(graph.nodes().len(), entities.len());
        assert!(!graph.edges().is_empty());
        assert_eq!(graph.association().path(), path);
        assert_eq!(graph.file_observation().label(), "AstFileObservation");
        for node in graph.nodes() {
            let AstGraphValue::String(metadata) = node.properties().get("metadata_json").unwrap()
            else {
                panic!("owned metadata");
            };
            let metadata: serde_json::Value = serde_json::from_str(metadata)?;
            for contribution in metadata["contributions"].as_array().unwrap() {
                if contribution["derived_value"].is_null() {
                    assert!(contribution["python_declaration"].is_object());
                    assert!(
                        contribution["python_declaration"]["leading_string"]
                            .get("literal_source")
                            .is_none()
                    );
                }
                if let Some(signature) = contribution["callable"].get("python_signature") {
                    assert!(signature["parameters"].get("literal_source").is_none());
                }
            }
        }
        with_model(text, &original, &draft, &[path], |second| {
            let again = assemble_graph_ts(
                AstGraphTsInput {
                    document: second,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            assert_eq!(graph.nodes(), again.nodes());
            assert_eq!(graph.edges(), again.edges());
            assert_eq!(graph.outcomes(), again.outcomes());
            assert_eq!(graph.file_observation(), again.file_observation());
            Ok(())
        })?;
        owned = Some(graph);
        Ok(())
    })?;
    fn require_send<T: Send>(_: &T) {}
    require_send(owned.as_ref().unwrap());
    assert!(!owned.unwrap().nodes().is_empty());
    Ok(())
}

#[tokio::test]
async fn genuine_python_local_calls_retain_ambiguity_shadowing_and_unsupported_environments()
-> TestResult {
    for (case, source, expected) in [
        (
            "one",
            "def Helper(): pass\ndef Caller(): Helper()\n",
            "resolved",
        ),
        ("zero", "def Caller(): Missing()\n", "notfound"),
        (
            "ambiguous",
            "def Helper(): pass\ndef Helper(): pass\ndef Caller(): Helper()\n",
            "ambiguous",
        ),
        (
            "canonical",
            "def K(): pass\ndef K(): pass\ndef Caller(): K()\n",
            "ambiguous",
        ),
        (
            "parameter",
            "def Helper(): pass\ndef Caller(Helper): Helper()\n",
            "unresolved",
        ),
        (
            "later",
            "def Helper(): pass\ndef Caller():\n Helper()\n Helper=other\n",
            "unresolved",
        ),
        (
            "import",
            "def Helper(): pass\nimport package as Helper\ndef Caller(): Helper()\n",
            "unresolved",
        ),
        (
            "class",
            "def Helper(): pass\nclass C:\n def Helper(self): pass\n def Caller(self): Helper()\n",
            "resolved",
        ),
        (
            "header",
            "def Helper(): pass\ndef Caller(x=Helper()): return x\n",
            "unresolved",
        ),
        (
            "comprehension",
            "def Helper(): pass\ndef Caller(items): return [Helper() for Helper in items]\n",
            "unresolved",
        ),
        (
            "iterable",
            "def Helper(): pass\ndef Caller(): return [x for x in Helper()]\n",
            "unresolved",
        ),
        (
            "lambda",
            "def Helper(): pass\ndef Caller():\n f=lambda: Helper()\n return f\n",
            "unresolved",
        ),
        (
            "global",
            "def Helper(): pass\ndef Caller():\n global Helper\n Helper()\n",
            "unresolved",
        ),
        (
            "member",
            "def Helper(): pass\ndef Caller(obj): obj.Helper()\n",
            "unresolved",
        ),
        (
            "for",
            "def Helper(): pass\ndef Caller(items):\n for Helper in items:\n  Helper()\n",
            "unresolved",
        ),
        (
            "with",
            "def Helper(): pass\ndef Caller():\n with manager() as Helper:\n  Helper()\n",
            "unresolved",
        ),
        (
            "except",
            "def Helper(): pass\ndef Caller():\n try: pass\n except Error as Helper: Helper()\n",
            "unresolved",
        ),
        (
            "private_name",
            "def __hidden(): pass\nclass C:\n def run(self): return __hidden()\n",
            "unresolved",
        ),
        (
            "class_cell",
            "def __class__(): pass\nclass C:\n def run(self): return __class__()\n",
            "unresolved",
        ),
        (
            "decorator",
            "def Helper(): pass\ndef mark(x): return lambda f:f\n@mark(Helper())\ndef Caller(): pass\n",
            "unresolved",
        ),
        (
            "class_header",
            "def Helper(): pass\nclass C(Helper()): pass\n",
            "unresolved",
        ),
    ] {
        let temp = TempDir::new()?;
        let original = Arc::new(context());
        let path = format!("{case}.py");
        let snapshot = raw(&temp, source, &path, &original).await?;
        let IndexedSource::Available(text) = snapshot.source(&original)? else {
            panic!("genuine source");
        };
        let draft = catalog(include_bytes!(
            "../../../crates/graphit-ast/profiles/queries/python.yaml"
        ));
        with_model(text, &original, &draft, &[&path], |document| {
            let observations = document.parsed()?.context_document()?.observations()?;
            let results = document.local_relations()?;
            let chosen = results
                .iter()
                .filter(|r| {
                    let relation = &document.relations().unwrap()[r.relation_index];
                    let node = relation.sources[r.source_index].node_index;
                    &text[observations.nodes[node].range.clone()]
                        == match case {
                            "zero" => "Missing",
                            "canonical" => "K",
                            "private_name" => "__hidden",
                            "class_cell" => "__class__",
                            _ => "Helper",
                        }
                        && relation.query.value().relation_type().value() == "CALLS"
                })
                .collect::<Vec<_>>();
            assert!(!chosen.is_empty(), "{case}: real CALLS capture");
            for result in chosen {
                assert!(!result.evidence_node_indices.is_empty());
                match (&result.target, expected) {
                    (TsLocalTargetState::Resolved { entity_index }, "resolved") => {
                        let target = &document.entities()?[*entity_index];
                        assert_eq!(target.name(), "Helper");
                        assert!(
                            target.namespaces().is_empty(),
                            "{case}: no implicit class closure"
                        );
                    }
                    (TsLocalTargetState::NotFound, "notfound")
                    | (TsLocalTargetState::Ambiguous { .. }, "ambiguous")
                    | (TsLocalTargetState::Unresolved { .. }, "unresolved") => {}
                    (actual, _) => panic!("{case}: expected {expected}, actual {actual:?}"),
                }
            }
            if expected != "resolved" {
                let graph = assemble_graph_ts(
                    AstGraphTsInput {
                        document,
                        source: NativeNnSource::Indexed(&snapshot),
                    },
                    &original,
                )?;
                assert!(graph.edges().is_empty(), "{case}: no unproved Graph edge");
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
async fn genuine_python_mixed_name_kinds_survive_sdk_publication_and_query() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let path = "names.py";
    let source = include_str!("fixtures/python/names.py");
    let snapshot = raw(&temp, source, path, &original).await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("actual stored Source");
    };
    let profile = include_str!("../../../crates/graphit-ast/profiles/queries/python.yaml").replace(
        "embed_labels:\n",
        "embed_labels:\n  - Value\n  - Import\n  - Parameter\n",
    );
    let draft = catalog(profile.as_bytes());
    let model = NativeNnModel {
        vector_id: [7; 32],
        dimensions: 2,
        model_id: "controlled-fixture",
        revision: "python-names",
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
                _ => panic!("genuine selected Python projection unavailable"),
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
        assert_eq!(variables.len(), 3);
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
async fn genuine_python_source_original_selection_vector_and_empty_guards_are_causal() -> TestResult
{
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let path = "guards.py";
    let snapshot = raw(&temp, "def F(): pass\n", path, &original).await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("actual Raw Source");
    };
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/python.yaml"
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
    with_model(text, &original, &draft, &["wrong.py"], |document| {
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
    let empty_raw = raw(&empty_temp, "", "empty.py", &original).await?;
    let IndexedSource::Available(empty) = empty_raw.source(&original)? else {
        panic!("stored empty Source");
    };
    with_model(empty, &original, &draft, &["empty.py"], |document| {
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
    let transformed_raw = raw(&transformed_temp, "a=1\n", "nonexact.py", &original).await?;
    let IndexedSource::Available(authored) = transformed_raw.source(&original)? else {
        panic!("actual authored Source");
    };
    let prepared = "a=12\n";
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
    let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &["nonexact.py"], limits())?;
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
async fn genuine_python_canonical_names_values_custom_captures_and_absent_rules_survive()
-> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let path = "names.py";
    let source = concat!(include_str!("fixtures/python/names.py"), "obj.field = 3\n");
    let snapshot = raw(&temp, source, path, &original).await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("genuine Source");
    };
    let profile = include_str!("../../../crates/graphit-ast/profiles/queries/python.yaml")
        .replace("  CALLS: { labels: [Function] }\n", "")
        .replace("queries:\n", r#"queries:
  - data_key: bespoke
    graph_label: AstFileObservation
    span_capture: declaration
    pattern: '((function_definition name: (identifier) @name) @declaration (#set! contract "python"))'
  - data_key: public_names
    graph_label: PublicName
    name_reject: '^_'
    pattern: '(function_definition name: (identifier) @name)'
  - data_key: owned_members
    graph_label: NativeMember
    parent_capture: parent
    parent_label: Class
    pattern: '(class_definition name: (identifier) @parent body: (block (function_definition name: (identifier) @name)))'
  - data_key: defaults
    graph_label: Parameter
    value_capture: value
    value_label: DefaultValue
    pattern: '(parameters (typed_default_parameter name: (identifier) @name value: (integer) @value))'
  - data_key: outside_values
    graph_label: Parameter
    value_capture: value
    value_label: BadDefault
    pattern: '(function_definition name: (identifier) @value parameters: (parameters (identifier) @name))'
  - data_key: unsupported_names
    graph_label: OddName
    pattern: '(assignment left: (attribute) @name)'
"#);
    let draft = catalog(profile.as_bytes());
    with_model(text, &original, &draft, &[path], |document| {
        let entities = document.entities()?;
        let canonical = entities
            .iter()
            .filter(|e| e.identity().label() == "Function" && e.name() == "K")
            .collect::<Vec<_>>();
        assert_eq!(canonical.len(), 2);
        assert_eq!(
            canonical
                .iter()
                .map(|e| e.name_raw().text())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["K", "K"])
        );
        let variables = entities
            .iter()
            .filter(|e| e.identity().label() == "Variable" && e.name() == "a")
            .collect::<Vec<_>>();
        assert_eq!(variables.len(), 3);
        assert_eq!(
            variables
                .iter()
                .map(|e| e
                    .namespaces()
                    .iter()
                    .map(|n| n.name.as_str())
                    .collect::<Vec<_>>())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([vec![], vec!["A"], vec!["B"]])
        );
        for name in ["_private", "__init__"] {
            let e = entities
                .iter()
                .find(|e| e.identity().label() == "Function" && e.name() == name)
                .unwrap();
            let TsEntityCallable::Function(details) = e.callable() else {
                panic!("callable");
            };
            assert_eq!(details.export, TsExport::NamePrefix(false));
        }
        for (name, visible) in [("tools", true), ("_hidden", false), ("pkg.core", true)] {
            let e = entities
                .iter()
                .find(|e| e.identity().label() == "Import" && e.name() == name)
                .unwrap();
            assert_eq!(
                e.contributions()[0]
                    .python_declaration
                    .as_ref()
                    .unwrap()
                    .export,
                TsExport::NamePrefix(visible)
            );
        }
        let values = entities
            .iter()
            .filter(|e| e.identity().label() == "Value")
            .map(|e| e.name_raw().text())
            .collect::<BTreeSet<_>>();
        let default = entities
            .iter()
            .find(|e| e.data_key() == "defaults" && e.name() == "p")
            .unwrap();
        assert!(
            matches!(&default.contributions()[0].declared_value, graphit_ast::tree_sitter_model::TsDeclaredValue::Native { captures, .. } if captures.len() == 1 && matches!(captures[0].binding, graphit_ast::tree_sitter_model::TsValueBinding::Subject { ordinal: 0 }))
        );
        assert!(
            entities
                .iter()
                .any(|e| e.identity().label() == "DefaultValue" && e.name_raw().text() == "2")
        );
        assert!(
            entities
                .iter()
                .all(|e| e.identity().label() != "BadDefault")
        );
        assert!(values.contains("None"));
        assert!(values.contains("'line\\nraw'"));
        let bespoke = entities
            .iter()
            .filter(|e| e.data_key() == "bespoke")
            .collect::<Vec<_>>();
        assert_eq!(bespoke.len(), 6);
        let public = entities
            .iter()
            .filter(|e| e.identity().label() == "PublicName")
            .collect::<Vec<_>>();
        assert_eq!(public.len(), 4);
        assert!(public.iter().all(|e| !e.name().starts_with('_')));
        assert!(
            bespoke
                .iter()
                .all(|e| e.identity().label() == "AstFileObservation"
                    && matches!(e.extent(), TsEntityExtent::Exact(_)))
        );
        let member = entities
            .iter()
            .find(|e| e.data_key() == "owned_members")
            .unwrap();
        assert_eq!(member.identity().label(), "NativeMember");
        assert_eq!(
            member
                .namespaces()
                .iter()
                .map(|n| n.name.as_str())
                .collect::<Vec<_>>(),
            ["A"]
        );
        assert!(entities.iter().all(|e| e.identity().label() != "OddName"));
        let groups = &document
            .parsed()?
            .context_document()?
            .observations()?
            .groups;
        let custom = groups
            .iter()
            .find(|g| g.query.value().data_key().value() == "bespoke")
            .unwrap();
        assert!(custom.matches.iter().all(|m| {
            m.properties
                .iter()
                .any(|p| p.key == "contract" && p.value.as_deref() == Some("python"))
        }));
        let unsupported = groups
            .iter()
            .find(|g| g.query.value().data_key().value() == "unsupported_names")
            .unwrap();
        assert_eq!(unsupported.matches.len(), 1);
        assert!(!document.diagnostics()?.is_empty());
        let relations = document.relations()?;
        assert!(document.local_relations()?.iter().any(|r| relations[r.relation_index].target_rule.is_none() && matches!(r.target, TsLocalTargetState::Unresolved { reason: graphit_ast::tree_sitter_model::TsLocalUnresolvedReason::MissingTargetRule })));
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&snapshot),
            },
            &original,
        )?;
        assert!(graph.edges().is_empty());
        assert_eq!(
            graph
                .nodes()
                .iter()
                .filter(|n| n.label() == "AstFileObservation")
                .count(),
            6
        );
        assert!(
            graph
                .file_observation()
                .properties()
                .contains_key("is_file_observation")
        );
        Ok(())
    })
}

#[tokio::test]
async fn genuine_python_policies_preserve_opaque_strings_attachments_and_nested_metrics()
-> TestResult {
    let source = "def outer():\n    \"\"\"first\nraw\n\"\"\"\n    if True: pass\n    def nested():\n        if True: pass\n        if True: pass\n    f = lambda: 1 if flag else 0\nprevious = 0 # trailing\n# leading\n@mark\ndef _F(x):\n    r\"\"\"opaque\\nraw\"\"\"\n    return x\n";
    for disabled in [false, true] {
        let temp = TempDir::new()?;
        let original = Arc::new(context());
        let path = "policy.py";
        let snapshot = raw(&temp, source, path, &original).await?;
        let IndexedSource::Available(text) = snapshot.source(&original)? else {
            panic!("genuine Source");
        };
        let mut profile =
            include_str!("../../../crates/graphit-ast/profiles/queries/python.yaml").to_owned();
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
            for (name, score) in [("outer", 2), ("nested", 3), ("_F", 1)] {
                let entity = document
                    .entities()?
                    .iter()
                    .find(|e| e.identity().label() == "Function" && e.name() == name)
                    .unwrap();
                let TsEntityCallable::Function(details) = entity.callable() else {
                    panic!("native callable");
                };
                if disabled {
                    assert!(matches!(details.complexity, TsComplexity::Disabled));
                } else {
                    assert!(
                        matches!(details.complexity, TsComplexity::Score { value, .. } if value == score),
                        "{name}: {:?}",
                        details.complexity
                    );
                }
                assert_eq!(details.export, TsExport::NamePrefix(name != "_F"));
                let declaration = entity.contributions()[0]
                    .python_declaration
                    .as_ref()
                    .unwrap();
                if name == "_F" {
                    assert_eq!(declaration.attachments.len(), 1);
                    assert_eq!(declaration.attachments[0].raw.unwrap().text(), "# leading");
                    assert_eq!(
                        declaration
                            .leading_string
                            .as_ref()
                            .unwrap()
                            .raw
                            .unwrap()
                            .text(),
                        "r\"\"\"opaque\\nraw\"\"\""
                    );
                    let TsEntityExtent::Exact(extent) = entity.extent() else {
                        panic!("wrapper extent");
                    };
                    assert!(extent.range().text().starts_with("@mark\ndef _F"));
                } else if name == "outer" {
                    assert_eq!(
                        declaration
                            .leading_string
                            .as_ref()
                            .unwrap()
                            .raw
                            .unwrap()
                            .text(),
                        "\"\"\"first\nraw\n\"\"\""
                    );
                } else {
                    assert!(declaration.leading_string.is_none());
                }
            }
            assert!(
                document
                    .entities()?
                    .iter()
                    .all(|e| e.identity().label() != "Comment")
            );
            Ok(())
        })?;
    }
    let original = context();
    let profile = include_str!("../../../crates/graphit-ast/profiles/queries/python.yaml").replace(
        "exports:\n  strategy: no_prefix\n  config:\n    prefix: \"_\"\n",
        "exports: { strategy: none }\n",
    );
    let draft = catalog(profile.as_bytes());
    with_model(
        "def _F(): pass\n",
        &original,
        &draft,
        &["no_export.py"],
        |document| {
            let TsEntityCallable::Function(details) = document.entities()?[0].callable() else {
                panic!("callable");
            };
            assert_eq!(details.export, TsExport::NotConfigured);
            Ok(())
        },
    )
}

#[test]
fn python_native_generic_signature_annotations_and_recovery_remain_observations() -> TestResult {
    let original = context();
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/python.yaml"
    ));
    with_model(
        "def identity[T](value: T) -> T: return value\nitems: list[int] = []\n",
        &original,
        &draft,
        &["generic.py"],
        |document| {
            let function = document
                .entities()?
                .iter()
                .find(|e| e.name() == "identity")
                .unwrap();
            let TsEntityCallable::Function(details) = function.callable() else {
                panic!("native function");
            };
            assert_eq!(
                details
                    .signature
                    .python
                    .as_ref()
                    .unwrap()
                    .type_parameters
                    .as_ref()
                    .unwrap()
                    .raw
                    .unwrap()
                    .text(),
                "[T]"
            );
            let variable = document
                .entities()?
                .iter()
                .find(|e| e.name() == "items" && e.identity().label() == "Variable")
                .unwrap();
            let TsEntityDeclaredType::Native(ty) = variable.declared_type() else {
                panic!("native annotation");
            };
            assert_eq!(ty.raw.unwrap().text(), "list[int]");
            Ok(())
        },
    )?;
    with_model(
        "def Broken(x):\n    return (\n",
        &original,
        &draft,
        &["broken.py"],
        |document| {
            assert!(!document.diagnostics()?.is_empty());
            assert!(
                document
                    .code_projections()?
                    .iter()
                    .all(|p| matches!(p, TsProjection::Unavailable { .. }))
            );
            Ok(())
        },
    )
}
