use super::*;
use crate::foreground_memory_jobs::MemoryJobResources;
use arrow_array::{RecordBatch, StringArray};
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    native_nn::{NativeNnJsonInput, encode_native_nn_json},
    native_nn::{NativeNnSource, NativeNnTsInput, encode_native_nn_ts},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_graph::{
        AstGraphJsonInput, AstGraphTsInput, AstSemanticGraphBatch, assemble_graph_json,
        assemble_graph_ts,
    },
    semantic_model::{JsonProjection, ParsedJsonDocument},
    source_index_locator::{SourceIndexPresence, open_local_source_index},
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::GrammarLanguage,
    tree_sitter_model::{ParsedTreeSitterDocument, TsProjection},
};
use graphit_ast::{
    indexed_source::MissingSourceReason,
    native_nn_physical_source::capture_native_nn_physical_source, source_inspection::IndexedSource,
};
use graphit_auth::ProjectRecordAuthority;
use graphit_auth::ServicePurpose;
use graphit_auth::{
    Permission, ServiceOrigin, ServiceRuleInput,
    profile::{LocalDevice, LocalExecution},
};
use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigSnapshot, ConfigStore},
    file_io::SnapshotContext,
};
use graphit_daemon::model_service as model_api;
use graphit_daemon::model_service::EmbedInput;
use graphit_daemon::model_service::ExpectedModel;
use graphit_data::lance_module_store::LanceModuleStore;
use rmcp::{
    RoleClient,
    service::{ClientLifecycleMode, ClientServiceExt},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::collections::HashMap;
use std::{fs, path::PathBuf};
use tokio::net::TcpStream;

use crate::foreground_source::foreground_memory_jobs_tests::{
    model_fixture, model_peer::ModelPeer,
};

const BODY: &str = "mod answer { pub fn first() -> i32 { 1 } pub fn second() -> i32 { 2 } }";

struct Fixture {
    source: super::super::test_fixture::Fixture,
    model: ModelPeer,
    resources: Arc<MemoryJobResources>,
    _bundle: model_fixture::Fixture,
}
impl Fixture {
    fn local_profile(source: &super::super::test_fixture::Fixture) {
        use graphit_auth::{
            profile::{
                AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
                ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
            },
            store::AuthStore,
        };
        use graphit_core::auth_document::{AuthDocumentLimits, AuthDocumentStore};
        let cancel = Cancellation::default();
        let request = RequestContext::new(
            source.server.0.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let installation = source.auth.installation();
        let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
            installation.live.owner()
        else {
            panic!("actual installation")
        };
        let scope = CapturedPublicationScope::installation(
            &source.paths,
            installation_id.clone(),
            installation.live.allowed_roots().map(ToOwned::to_owned),
            source.server.0.policy.scope,
            request.snapshot(),
        )
        .unwrap();
        let store = AuthStore::new(
            AuthDocumentStore::capture(
                scope,
                AuthDocumentLimits::new(1048576, 65536, 256, 65536).unwrap(),
                request.snapshot(),
            )
            .unwrap(),
            ProfileLimits {
                bytes: 1048576,
                nodes: 65536,
                scalar_bytes: 65536,
                collection_entries: 1024,
                providers: 16,
                profiles: 16,
            },
            &request,
        )
        .unwrap();
        let observed = store.load(&request).unwrap();
        let outcome = store
            .mutate(observed, &request, |doc, request| {
                doc.add_provider(
                    "cpu".into(),
                    ProviderConfig {
                        kind: ProviderKind::Local {
                            allow_aws_credential_chain: false,
                            broker: None,
                        },
                        embedding: AiService::Local(LocalExecution {
                            device: LocalDevice::Cpu,
                            device_id: 0,
                        }),
                        rerank: AiService::Disabled,
                        s3: S3Location {
                            bucket: None,
                            region: None,
                            endpoint: None,
                            prefix: None,
                            source: S3Source::LocalCredentials,
                        },
                    },
                    doc.revision(),
                    1,
                    request,
                )?;
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("cpu").unwrap().revision(),
                    profile_revision: None,
                };
                doc.replace_profile_data(
                    ProfileData {
                        name: "local".into(),
                        provider: "cpu".into(),
                        username: "semantic fixture".into(),
                        organization: None,
                        teams: vec![],
                        credentials: ProfileCredentials::Local(LocalCredentials {
                            anonymous: false,
                            mcp_key: Some(Secret::new(secrecy::SecretString::from(
                                "semantic-local-fixture",
                            ))),
                            broker_key: None,
                            embedding_api_key: None,
                            rerank_api_key: None,
                            s3: None,
                        }),
                    },
                    expected,
                    1,
                    request,
                )?;
                doc.activate("local", doc.revision(), request)?;
                Ok(())
            })
            .unwrap();
        assert!(outcome.failure().is_none());
        assert!(outcome.cleanup_failure().is_none());
    }

    async fn new(permissions: &[Permission]) -> Self {
        let execution = LocalExecution {
            device: LocalDevice::Cpu,
            device_id: 0,
        };
        let rules =
            [ServiceOrigin::Runtime, ServiceOrigin::Local].map(|origin| ServiceRuleInput::Local {
                origin,
                provider: "cpu",
                purpose: ServicePurpose::Embedding,
                execution,
            });
        let mut source = super::super::test_fixture::fixture_with_projects_and_rules(
            permissions,
            Brand::default(),
            NativeBinding::KiroV3,
            &["first-project", "second-project"],
            &rules,
        );
        Self::local_profile(&source);
        let ast = source.paths.root().join("ast");
        drop(graphit_fs::create_private_directory(&ast).unwrap());
        for (_, id, _) in source.projects.iter() {
            drop(graphit_fs::create_private_directory(&ast.join(id.to_string())).unwrap());
        }
        let mut bundle = model_fixture::Fixture::metadata(model_api::TaskKind::Embedding);
        bundle.body["recipe"]["dimensions"] = serde_json::json!(128);
        fs::write(
            bundle.directory.path().join("model.json"),
            serde_json::to_vec(&bundle.body).unwrap(),
        )
        .unwrap();
        let settings = bundle.settings(&[crate::models::DEFAULT_SLOT]);
        let cancel = Cancellation::default();
        let global = ConfigStore::global(&source.paths).unwrap();
        global
            .set(
                ConfigKey::new("daemon.models").unwrap(),
                serde_json::to_value(settings).unwrap(),
                Duration::from_secs(5),
                &cancel,
            )
            .unwrap();
        global
            .set(
                Module::Memory.key(),
                json!(false),
                Duration::from_secs(5),
                &cancel,
            )
            .unwrap();
        let config = ConfigSnapshot::new(source.paths.brand(), [global.load().unwrap()]).unwrap();
        source.server.attach_project_hook(&config).unwrap();
        let resolved = bundle.config(&[crate::models::DEFAULT_SLOT]);
        let expected = ExpectedModel::new(&resolved, crate::models::DEFAULT_SLOT, "cpu").unwrap();
        let policy = graphit_hub::policy::HubResourcePolicy::resolve(&config).unwrap();
        let mut resources = MemoryJobResources::remote(
            Arc::clone(&source.auth),
            None,
            crate::foreground_memory_jobs::test_broker_cache(&policy),
            policy,
            source.server.0.policy.scope,
            source.server.0.policy.index,
            NativeBinding::KiroV3,
        )
        .unwrap();
        let model = ModelPeer::bind(&mut resources, expected).await;
        let resources = Arc::new(resources);
        source
            .server
            .attach_semantic_model(Some(Arc::clone(&resources)))
            .unwrap();
        assert!(Arc::ptr_eq(
            &source.server.0.semantic_model().unwrap().unwrap(),
            &resources
        ));
        Self {
            source,
            model,
            resources,
            _bundle: bundle,
        }
    }
    fn path(&self, project: usize, name: &str) -> PathBuf {
        self.source
            .paths
            .root()
            .join("projects")
            .join(self.source.projects[project].1.to_string())
            .join("ast")
            .join(name)
    }
    async fn raw(&self, project: usize, body: Option<&str>) -> u64 {
        self.raw_file(project, "code.rs", body, BODY).await
    }
    async fn raw_file(
        &self,
        project: usize,
        name: &str,
        body: Option<&str>,
        physical: &str,
    ) -> u64 {
        raw_file_for_source(&self.source, project, name, body, physical).await
    }
    // Publish genuine parser facts into the same FILE stores used by the daemon.
    // NN publication remains separate so corruption/absence tests can exercise
    // actual current-store mismatches without manufacturing a portable bundle.
    async fn publish_graph(
        &self,
        project_index: usize,
        graph: AstSemanticGraphBatch,
        text: &str,
        original: &Arc<SnapshotContext>,
    ) {
        publish_graph_for_source(&self.source, project_index, graph, text, original, false).await;
    }
    async fn publish_nn(
        &self,
        project_index: usize,
        path: &str,
        batch: RecordBatch,
        model: &graphit_ast::native_nn::NativeNnModel<'_>,
        original: &Arc<SnapshotContext>,
    ) {
        use futures_util::TryStreamExt;
        use graphit_data::lance_branch_store::{BranchSnapshot, col, lit};
        let branch = self.source.projects[project_index].2.lance_branch();
        let mut pin = BranchSnapshot::capture_observed(
            self.path(project_index, "corpus.lance").to_str().unwrap(),
            (!branch.is_empty()).then_some(branch.as_str()),
            None,
        )
        .await
        .unwrap();
        let wanted = graphit_ast::corpus::vector_fields(model).unwrap();
        if pin.schema().field_with_name("vector").ok() != Some(wanted.field(0)) {
            pin = pin.drop_vector("vector", original).await.unwrap();
            pin = pin.add_nullable_columns(wanted, original).await.unwrap();
        }
        let mut scan = pin.scan();
        scan.filter_expr(col("path").eq(lit(path)));
        let rows = scan
            .try_into_stream()
            .await
            .unwrap()
            .try_collect::<Vec<_>>()
            .await
            .unwrap();
        let current =
            arrow_select::concat::concat_batches(&Arc::new(pin.schema().clone()), &rows).unwrap();
        let lexical_schema = graphit_ast::entity_search::schema();
        let lexical = RecordBatch::try_new(
            lexical_schema.clone(),
            lexical_schema
                .fields()
                .iter()
                .map(|f| current.column_by_name(f.name()).unwrap().clone())
                .collect(),
        )
        .unwrap();
        let prepared_schema = graphit_ast::native_nn::native_nn_projection_schema();
        let prepared = RecordBatch::try_new(
            prepared_schema.clone(),
            prepared_schema
                .fields()
                .iter()
                .map(|f| batch.column_by_name(f.name()).unwrap().clone())
                .collect(),
        )
        .unwrap();
        let encoded = graphit_ast::corpus::encode_file(
            Arc::new(pin.schema().clone()),
            lexical,
            prepared,
            Some(&current),
            original,
        )
        .unwrap();
        pin = graphit_ast::live_search::prepare_file(&pin, path, encoded, original)
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
        if let Some(patch) = graphit_ast::corpus::prepare_embeddings(&pin, &batch, model, original)
            .await
            .unwrap()
        {
            patch.commit().await.unwrap();
        }
    }
    async fn publish(&self, project_index: usize, infer: bool) -> Vec<String> {
        let cancel = Cancellation::default();
        let request = RequestContext::new(
            self.source.server.0.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let original = request.snapshot();
        let (paths, id, version) = &self.source.projects[project_index];
        let live =
            CapturedPublicationScope::project(paths, *id, self.resources.scope, original).unwrap();
        let binding = live.binding().unwrap();
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &live,
                record: &binding,
            },
            version,
        };
        let raw_index = match open_local_source_index(
            self.source.auth.installation(),
            project,
            original,
            self.resources.native,
        )
        .await
        .unwrap()
        {
            SourceIndexPresence::Available(index) => index,
            _ => panic!("actual Raw dataset"),
        };
        let raw = raw_index.read("code.rs", original).await.unwrap();
        let physical = if raw.missing_reason() == Some(MissingSourceReason::TextNotStored) {
            Some(
                capture_native_nn_physical_source(
                    project,
                    &["code.rs"],
                    self.resources.policy.copy(),
                    self.resources.policy.retained(),
                    self.source.server.0.policy.read,
                    original,
                )
                .unwrap(),
            )
        } else {
            None
        };
        let text = match &physical {
            Some(source) => source.source(original).unwrap(),
            None => match raw.source(original).unwrap() {
                IndexedSource::Available(text) => text,
                _ => panic!("Source text"),
            },
        };
        let draft = semantic_catalog("semantic-rust-fixture", b"language: rust\ngrammar: tree-sitter-rust\nembed_labels: [Function]\ncontext_types: {mod_item: Namespace, impl_item: Struct, trait_item: Trait, function_item: Function}\ncontext_name_queries: {impl_item: '(impl_item type: (_) @name) @context'}\nqueries:\n  - {data_key: functions, graph_label: Function, pattern: '(function_item name: (identifier) @name)'}\n");
        let prepared = PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            GrammarLanguage::from_language(tree_sitter_rust::LANGUAGE.into()).unwrap(),
            original,
        )
        .unwrap();
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
            original,
        )
        .unwrap();
        let parsed = ParsedTreeSitterDocument::collect(
            &prepared,
            &input,
            &["code.rs"],
            semantic_identity_limits(),
        )
        .unwrap();
        let document = parsed.assemble().unwrap();
        let projections = document.code_projections().unwrap();
        let indices: Vec<_> = projections
            .iter()
            .map(|projection| match projection {
                TsProjection::Available(p) => p.entity_index,
                TsProjection::Unavailable(_) => {
                    panic!("actual configured Function projection required")
                }
            })
            .collect();
        let uids = indices
            .iter()
            .map(|&index| {
                document.entities().unwrap()[index]
                    .identity()
                    .uid()
                    .as_str()
                    .to_owned()
            })
            .collect();
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document: &document,
                source: match &physical {
                    Some(capture) => NativeNnSource::Physical { capture, raw: &raw },
                    None => NativeNnSource::Indexed(&raw),
                },
            },
            original,
        )
        .unwrap();
        self.publish_graph(project_index, graph, text, original)
            .await;
        if !infer {
            return uids;
        }
        let caller = self
            .source
            .auth
            .issuer()
            .authenticate_runtime(&self.source.raw, &request)
            .unwrap();
        let mut service = self
            .source
            .auth
            .issuer()
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = service.local().unwrap();
        let projection: Vec<_> = projections
            .iter()
            .map(|projection| match projection {
                TsProjection::Available(p) => p.text.to_owned(),
                TsProjection::Unavailable(_) => unreachable!("validated fixture projections"),
            })
            .collect();
        let output = self
            .resources
            .local
            .as_ref()
            .unwrap()
            .client
            .embed(
                &lease,
                self.source.auth.runtime(),
                &self.resources.local.as_ref().unwrap().expected,
                graphit_daemon::model_service::EmbedOperation {
                    input: EmbedInput::CodeDocument,
                    texts: projection,
                    deadline: request.snapshot().required_deadline().unwrap(),
                },
                TcpStream::connect(self.resources.local.as_ref().unwrap().address),
            )
            .await
            .unwrap();
        let source = match &physical {
            Some(capture) => NativeNnSource::Physical { capture, raw: &raw },
            None => NativeNnSource::Indexed(&raw),
        };
        let model = crate::ast_semantic::expected_native_nn_model(
            &self.resources.local.as_ref().unwrap().expected,
        )
        .unwrap();
        let batch = encode_native_nn_ts(
            &model,
            &[NativeNnTsInput {
                document: &document,
                source,
                entity_indices: &indices,
                vectors: &output.vectors,
                model_generation: Some(output.generation),
            }],
            original,
        )
        .unwrap();
        Box::pin(self.publish_nn(project_index, raw.path(), batch, &model, original)).await;
        uids
    }
    async fn publish_json(&self, project_index: usize) -> Vec<String> {
        let cancel = Cancellation::default();
        let request = RequestContext::new(
            self.source.server.0.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let original = request.snapshot();
        let (paths, id, version) = &self.source.projects[project_index];
        let live =
            CapturedPublicationScope::project(paths, *id, self.resources.scope, original).unwrap();
        let binding = live.binding().unwrap();
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &live,
                record: &binding,
            },
            version,
        };
        let SourceIndexPresence::Available(raw_index) = open_local_source_index(
            self.source.auth.installation(),
            project,
            original,
            self.resources.native,
        )
        .await
        .unwrap() else {
            panic!("actual JSON Raw")
        };
        let raw = raw_index.read("data.json", original).await.unwrap();
        let IndexedSource::Available(text) = raw.source(original).unwrap() else {
            panic!("actual JSON text")
        };
        let draft = semantic_catalog(
            "semantic-json-fixture",
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../graphit-ast/tests/fixtures/catalog/shipped/json.yaml"
            )),
        );
        let prepared = graphit_ast::tree_sitter_context::PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            GrammarLanguage::from_language(tree_sitter_json::LANGUAGE.into()).unwrap(),
            original,
        )
        .unwrap();
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
            original,
        )
        .unwrap();
        let parsed = ParsedJsonDocument::collect(
            &prepared,
            &input,
            &["data.json"],
            semantic_identity_limits(),
        )
        .unwrap();
        let document = parsed.assemble().unwrap();
        let projections = document.code_projections().unwrap();
        let projections: Vec<_> = projections
            .iter()
            .map(|p| match p {
                JsonProjection::Available(p) => p,
                JsonProjection::Unavailable(_) => panic!("JSON diagnostics"),
            })
            .collect();
        let caller = self
            .source
            .auth
            .issuer()
            .authenticate_runtime(&self.source.raw, &request)
            .unwrap();
        let mut service = self
            .source
            .auth
            .issuer()
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = service.local().unwrap();
        let output = self
            .resources
            .local
            .as_ref()
            .unwrap()
            .client
            .embed(
                &lease,
                self.source.auth.runtime(),
                &self.resources.local.as_ref().unwrap().expected,
                graphit_daemon::model_service::EmbedOperation {
                    input: EmbedInput::CodeDocument,
                    texts: projections.iter().map(|p| p.text.to_owned()).collect(),
                    deadline: request.snapshot().required_deadline().unwrap(),
                },
                TcpStream::connect(self.resources.local.as_ref().unwrap().address),
            )
            .await
            .unwrap();
        let indices: Vec<_> = projections.iter().map(|p| p.entity_index).collect();
        let uids = projections
            .iter()
            .map(|p| p.entity.identity().uid().as_str().to_owned())
            .collect();
        let model = crate::ast_semantic::expected_native_nn_model(
            &self.resources.local.as_ref().unwrap().expected,
        )
        .unwrap();
        let graph = assemble_graph_json(
            AstGraphJsonInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
            },
            original,
        )
        .unwrap();
        let batch = encode_native_nn_json(
            &model,
            &[NativeNnJsonInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &indices,
                vectors: &output.vectors,
                model_generation: Some(output.generation),
            }],
            original,
        )
        .unwrap();
        self.publish_graph(project_index, graph, text, original)
            .await;
        Box::pin(self.publish_nn(project_index, raw.path(), batch, &model, original)).await;
        uids
    }
    async fn close(mut self) {
        self.source.server.shutdown().await.unwrap();
        self.model.shutdown().await.unwrap();
    }
}

fn source_path(
    source: &super::super::test_fixture::Fixture,
    project: usize,
    name: &str,
) -> PathBuf {
    source
        .paths
        .root()
        .join("projects")
        .join(source.projects[project].1.to_string())
        .join("ast")
        .join(name)
}

async fn raw_file_for_source(
    source: &super::super::test_fixture::Fixture,
    project: usize,
    name: &str,
    body: Option<&str>,
    physical: &str,
) -> u64 {
    let path = source_path(source, project, "source.lance");
    // Production's captured cohort requires private ancestors as well as a
    // private leaf; ordinary create_dir_all would make new AST ancestors 0755.
    let mut directory = source.paths.root().to_owned();
    let relative = path
        .parent()
        .unwrap()
        .strip_prefix(&directory)
        .unwrap()
        .to_owned();
    for component in relative.components() {
        directory.push(component);
        if !directory.exists() {
            drop(graphit_fs::create_private_directory(&directory).unwrap());
        }
    }
    fs::write(source.projects[project].0.root().join(name), physical).unwrap();
    let schema = graphit_ast::indexed_source::source_schema();
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(vec![name])),
            Arc::new(StringArray::from(vec![body])),
        ],
    )
    .unwrap();
    let store = if path.exists() {
        let store = LanceModuleStore::open(path.to_str().unwrap(), schema)
            .await
            .unwrap();
        store
            .snapshot()
            .prepare_replace(vec![batch])
            .await
            .unwrap()
            .commit()
            .await
            .unwrap()
    } else {
        LanceModuleStore::create(path.to_str().unwrap(), schema, vec![batch])
            .await
            .unwrap()
    };
    store.snapshot().version()
}

async fn publish_graph_for_source(
    source: &super::super::test_fixture::Fixture,
    project_index: usize,
    graph: AstSemanticGraphBatch,
    text: &str,
    original: &Arc<SnapshotContext>,
    unknown_extents: bool,
) {
    use graphit_ast::{entity_search, live_graph::LiveFileInput, semantic_graph::AstGraphValue};
    use graphit_data::{
        graph_writer::{GraphEdge, GraphNode, GraphValue},
        lance_branch_store::BranchSnapshot,
    };
    let (paths, id, version) = &source.projects[project_index];
    let policy = &source.server.0.policy;
    let scope = CapturedPublicationScope::project(paths, *id, policy.scope, original).unwrap();
    let record = scope.binding().unwrap();
    let project = ProjectCapture {
        scope: ScopeCapture {
            live: &scope,
            record: &record,
        },
        version,
    };
    let (live, directory) = source
        .server
        .0
        .graph
        .live_ast()
        .capture(
            source.auth.installation(),
            project,
            policy.index,
            original,
            true,
        )
        .await
        .unwrap()
        .unwrap();
    let _fence = live.write_fence(original).await.unwrap();
    directory.revalidate(original).unwrap();
    let fts_path = source_path(source, project_index, "corpus.lance");
    if !fts_path.exists() {
        let schema = graphit_ast::corpus::schema();
        LanceModuleStore::create(
            fts_path.to_str().unwrap(),
            schema.clone(),
            vec![RecordBatch::new_empty(schema)],
        )
        .await
        .unwrap();
    }
    let branch = version.lance_branch();
    let fts = BranchSnapshot::capture_observed(
        fts_path.to_str().unwrap(),
        (!branch.is_empty()).then_some(branch.as_str()),
        None,
    )
    .await
    .unwrap();
    let parts = graph.into_parts();
    let mut lexical = entity_search::encode_file(&parts, text, original).unwrap();
    if unknown_extents {
        let schema = lexical.schema();
        let mut columns = lexical.columns().to_vec();
        columns[schema.index_of("extent_status").unwrap()] =
            Arc::new(StringArray::from(vec!["Unknown"; lexical.num_rows()]));
        for name in ["start_byte", "end_byte", "start_line", "end_line"] {
            columns[schema.index_of(name).unwrap()] =
                Arc::new(arrow_array::UInt64Array::from(vec![
                    None;
                    lexical.num_rows()
                ]));
        }
        lexical = RecordBatch::try_new(schema, columns).unwrap();
    }
    let lexical = graphit_ast::corpus::encode_file(
        Arc::new(fts.schema().clone()),
        lexical,
        RecordBatch::new_empty(graphit_ast::native_nn::native_nn_projection_schema()),
        None,
        original,
    )
    .unwrap();
    let prepared =
        graphit_ast::live_search::prepare_file(&fts, parts.association.path(), lexical, original)
            .await
            .unwrap();
    let nodes = parts
        .nodes
        .into_iter()
        .chain([parts.file_observation])
        .map(|node| {
            let (label, key, properties) = node.into_parts();
            GraphNode {
                label,
                key,
                properties: properties
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            key,
                            match value {
                                AstGraphValue::String(value) => GraphValue::String(value),
                                AstGraphValue::Int64(value) => GraphValue::Int64(value),
                                AstGraphValue::Bool(value) => GraphValue::Bool(value),
                            },
                        )
                    })
                    .collect(),
            }
        })
        .collect::<Vec<_>>();
    let edges = parts
        .edges
        .into_iter()
        .map(|edge| {
            let (kind, source_key, target_key, uid) = edge.into_parts();
            GraphEdge {
                kind,
                source_key,
                target_key,
                uid,
            }
        })
        .collect::<Vec<_>>();
    live.graph
        .replace_file(
            LiveFileInput {
                path: parts.association.path(),
                source_sha256: *parts.association.source_sha256(),
                raw_version: parts.association.raw_version(),
                nodes: &nodes,
                edges: &edges,
                definitions: &[],
                references: &[],
                scip_document: None,
                oracle: &[],
                oracle_file: None,
                pending_compiler: None,
                scip_compiler: None,
            },
            original,
        )
        .unwrap();
    let mut indexed = prepared.commit().await.unwrap();
    for column in entity_search::TEXT_COLUMNS {
        let name = format!("{column}_code_fts");
        let index = match indexed.prepare_code_fts_append(column, &name).await {
            Ok(index) => index,
            Err(graphit_data::lance_branch_store::BranchError::JsonIndexReplacementMissing {
                ..
            }) => Some(indexed.prepare_code_fts_index(column, &name).await.unwrap()),
            Err(error) => panic!("actual FILE FTS index: {error}"),
        };
        if let Some(index) = index {
            indexed = index.commit().await.unwrap();
        }
    }
    directory.revalidate(original).unwrap();
    original.check().unwrap();
}

async fn client(
    server: &SourceServer,
    credential: &str,
    local: bool,
) -> (
    rmcp::service::RunningService<RoleClient, ()>,
    tokio::task::JoinHandle<io::Result<()>>,
) {
    let listener = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let uri = format!("http://{}/mcp", listener.address());
    let owner = server.clone();
    let serving = tokio::spawn(async move { owner.serve(listener).await });
    let mut bearer = axum::http::HeaderValue::from_str(&format!("Bearer {credential}")).unwrap();
    bearer.set_sensitive(true);
    let headers = HashMap::from([
        (axum::http::header::AUTHORIZATION, bearer),
        (
            axum::http::HeaderName::from_static("x-graphit-auth-origin"),
            axum::http::HeaderValue::from_static(if local { "local" } else { "runtime" }),
        ),
    ]);
    let client = ()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                reqwest::Client::new(),
                StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
            },
        )
        .await
        .unwrap();
    (client, serving)
}
fn params(mut value: Value) -> CallToolRequestParams {
    value
        .as_object_mut()
        .unwrap()
        .insert("ai_optimized".into(), json!(false));
    CallToolRequestParams::new(TOOL).with_arguments(value.as_object().unwrap().clone())
}
fn arguments() -> Arguments {
    serde_json::from_value(json!({"query":"return a number","k":2})).unwrap()
}
async fn direct(f: &Fixture, project: usize) -> Result<Value, SemanticError> {
    let snapshot = f.source.server.0.live_projects.snapshot().unwrap();
    execute(
        &f.source.server.0,
        &snapshot.projects[project],
        arguments(),
        &f.source.raw,
        false,
        &Cancellation::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .await
}

#[tokio::test]
async fn actual_code_document_nn_mcp_query_and_raw_associations() {
    let f = Fixture::new(&[Permission::Read(Module::Ast)]).await;
    let raw_generation = f.raw(0, Some(BODY)).await;
    let uids = f.publish(0, true).await;
    f.raw(1, Some(BODY)).await;
    f.publish(1, true).await;
    let (client, serving) = client(&f.source.server, &f.source.raw, false).await;
    assert!(
        client
            .list_tools(Default::default())
            .await
            .unwrap()
            .tools
            .iter()
            .any(|tool| tool.name == TOOL)
    );
    let schema = client
        .call_tool(
            CallToolRequestParams::new("graphit_ast_schema")
                .with_arguments(json!({"ai_optimized":false}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(schema.is_error, Some(false));
    let schema = schema.structured_content.unwrap();
    assert!(schema["generation"].is_array());
    assert_eq!(
        schema["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["label"] == "Function")
            .unwrap()["primary_key"],
        "entity_key"
    );
    let graph = client
        .call_tool(
            CallToolRequestParams::new("graphit_ast_query").with_arguments(
                json!({"ai_optimized":false,"query":"MATCH (n:Function) RETURN n.entity_key,n.uid,n.data_key,n.name,n.path,n.lang,n.grammar,n.source_sha256,n.extent_status,n.raw_version,n.start_byte,n.end_byte,n.start_line,n.end_line"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(graph.is_error, Some(false));
    let graph = graph.structured_content.unwrap();
    let graph_rows = graph["rows"].as_array().unwrap();
    assert_eq!(graph_rows.len(), uids.len());
    let lexical = client
        .call_tool(params(json!({"query":"first","mode":"fts","k":2})))
        .await
        .unwrap();
    assert_eq!(lexical.is_error, Some(false));
    let lexical = lexical.structured_content.unwrap();
    assert_eq!(lexical["status"], "available");
    assert!(lexical.get("graph_generation").is_none());
    assert!(lexical.get("search_generation").is_none());
    let result = client
        .call_tool(params(
            json!({"query":"return a number","mode":"semantic","k":2}),
        ))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false));
    let payload = result.structured_content.unwrap();
    let vector_result = client
        .call_tool(params(
            json!({"query":"return a number","mode":"vector","k":2}),
        ))
        .await
        .unwrap();
    assert_eq!(vector_result.is_error, Some(false));
    let vector_payload = vector_result.structured_content.unwrap();
    assert_eq!(vector_payload["results"].as_array().unwrap().len(), 2);
    assert_eq!(payload["status"], "available");
    for reply in [&payload, &vector_payload] {
        assert!(reply.get("graph_generation").is_none());
        assert!(reply.get("search_generation").is_none());
        for row in reply["results"].as_array().unwrap() {
            let entity = graph_rows
                .iter()
                .find(|entity| {
                    entity[1]["value"] == row["uid"] && entity[2]["value"] == row["data_key"]
                })
                .unwrap();
            for (index, field) in [
                "entity_key",
                "uid",
                "data_key",
                "name",
                "path",
                "language",
                "grammar",
                "source_sha256",
                "extent_status",
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(row[field], entity[index]["value"], "{field}");
            }
            for (index, field) in [
                "raw_version",
                "start_byte",
                "end_byte",
                "start_line",
                "end_line",
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(
                    row[field].as_u64().unwrap().to_string(),
                    entity[index + 9]["value"].as_str().unwrap(),
                    "{field}"
                );
            }
        }
    }
    let rows = payload["results"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert!(uids.contains(&row["uid"].as_str().unwrap().to_owned()));
        assert_eq!(row["path"], "code.rs");
        assert!(row["distance"].is_number());
        assert!(row.get("vector").is_none());
        assert!(row.get("projection").is_none());
    }
    let snapshot = f.source.server.0.live_projects.snapshot().unwrap();
    let local_result = execute(
        &f.source.server.0,
        &snapshot.projects[0],
        arguments(),
        "semantic-local-fixture",
        true,
        &Cancellation::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .await
    .unwrap();
    assert_eq!(local_result["status"], "available");
    assert!(matches!(
        execute(
            &f.source.server.0,
            &snapshot.projects[1],
            arguments(),
            "wrong-local-fixture",
            true,
            &Cancellation::default(),
            Instant::now() + Duration::from_secs(30)
        )
        .await,
        Err(SemanticError::Authentication(_))
    ));
    assert_eq!(
        direct(&f, 1).await.unwrap()["results"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // Live Raw may advance independently: intact FILE rows retain and resolve
    // their genuine historical Source, rather than borrowing the new pin.
    let advanced = f.raw(0, Some(BODY)).await;
    assert!(advanced > raw_generation);
    let retained = direct(&f, 0).await.unwrap();
    assert!(
        retained["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["raw_version"] == raw_generation)
    );
    // Replacement rows withdraw their vectors in the same corpus transaction.
    // Querying the newly published FILE before inference reports honest coverage.
    f.publish(0, false).await;
    assert_eq!(direct(&f, 0).await.unwrap()["status"], "index_empty");
    f.publish(0, true).await;
    assert_eq!(direct(&f, 0).await.unwrap()["status"], "available");
    f.raw(0, None).await;
    f.publish(0, true).await;
    assert_eq!(direct(&f, 0).await.unwrap()["status"], "available");
    fs::write(
        f.source.projects[0].0.root().join("code.rs"),
        "pub fn changed() {}",
    )
    .unwrap();
    assert_eq!(
        direct(&f, 0).await.unwrap()["status"],
        "available",
        "unparsed disk changes do not invalidate available corpus results"
    );
    client.cancel().await.unwrap();
    f.source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    f.close().await;
}

#[tokio::test]
async fn available_search_survives_independent_graph_and_source_unavailability() {
    let f = Fixture::new(&[Permission::Read(Module::Ast)]).await;
    f.raw(0, Some(BODY)).await;
    f.publish(0, true).await;
    let cancellation = Cancellation::default();
    let request = RequestContext::new(
        f.source.server.0.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let selected = &f.source.projects[0];
    let capture = CapturedPublicationScope::project(
        &selected.0,
        selected.1,
        f.source.server.0.policy.scope,
        request.snapshot(),
    )
    .unwrap();
    let binding = capture.binding().unwrap();
    let project = ProjectCapture {
        scope: ScopeCapture {
            live: &capture,
            record: &binding,
        },
        version: &selected.2,
    };
    let (root, _) =
        crate::foreground_ast_sources::progressive::location(f.source.auth.installation(), project)
            .unwrap();
    fs::rename(root.join("graph"), root.join("unavailable-graph")).unwrap();
    fs::rename(
        f.path(0, "source.lance"),
        f.path(0, "unavailable-source.lance"),
    )
    .unwrap();
    let (client, serving) = client(&f.source.server, &f.source.raw, false).await;
    for mode in ["fts", "semantic", "hybrid"] {
        let response = client
            .call_tool(params(
                json!({"query":"first","mode":mode,"k":2,"rerank":false}),
            ))
            .await
            .unwrap();
        assert_eq!(response.is_error, Some(false), "{mode}: {response:?}");
        let response = response.structured_content.unwrap();
        assert_eq!(response["status"], "available", "{mode}: {response}");
        assert!(!response["results"].as_array().unwrap().is_empty());
    }
    client.cancel().await.unwrap();
    f.source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    f.close().await;
}

#[tokio::test]
async fn absent_empty_denied_and_cancelled_query_do_not_contact_closed_model() {
    let mut f = Fixture::new(&[Permission::Read(Module::Ast)]).await;
    f.raw(0, Some(BODY)).await;
    f.publish(0, true).await;
    let pair_cancel = Cancellation::default();
    let pair_request = RequestContext::new(
        f.source.server.0.policy.content,
        &pair_cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    // Keep the real Graph/FTS FILE while removing this private fixture's NN
    // dataset. No graph or model presence can manufacture vector availability.
    graphit_data::lance_branch_store::BranchSnapshot::capture_observed(
        f.path(0, "corpus.lance").to_str().unwrap(),
        None,
        None,
    )
    .await
    .unwrap()
    .drop_vector("vector", pair_request.snapshot())
    .await
    .unwrap();
    f.model.shutdown().await.unwrap();
    assert!(
        TcpStream::connect(f.resources.local.as_ref().unwrap().address)
            .await
            .is_err()
    );
    assert_eq!(direct(&f, 0).await.unwrap()["status"], "index_absent");
    assert_eq!(direct(&f, 1).await.unwrap()["status"], "index_absent");
    // An existing local FILE store still has no NN observation. Its existence
    // must not trigger discovery or local model RPC before returning absence.
    f.raw(1, Some(BODY)).await;
    let live_projects = f.source.server.0.live_projects.snapshot().unwrap();
    let selected = &live_projects.projects[1];
    let capture = crate::project_graph::PreparedRead::capture(
        &f.source.server.0.policy,
        &f.source.server.0.live_projects,
        (&selected.0, selected.1, &selected.2),
        Arc::clone(pair_request.snapshot()),
    )
    .unwrap();
    let publication = crate::foreground_ast_sources::progressive::ProgressivePublication::open(
        f.source.auth.installation(),
        capture.project(),
        f.source.server.0.policy.index,
        Arc::clone(pair_request.snapshot()),
        f.source.server.0.graph.live_ast(),
        &|| {
            capture
                .check(
                    &f.source.server.0.policy,
                    f.source.server.0.graph.project_config()?,
                    &f.source.server.0.live_projects,
                )
                .map_err(io::Error::other)
        },
    )
    .await
    .unwrap();
    assert_eq!(direct(&f, 1).await.unwrap()["status"], "index_absent");
    drop(publication);
    let global = ConfigStore::global(&f.source.paths).unwrap();
    for module in [Module::Ast, Module::Knowledge, Module::Memory] {
        global
            .set(
                module.key(),
                json!(false),
                Duration::from_secs(5),
                &Cancellation::default(),
            )
            .unwrap();
    }
    let disabled_config =
        ConfigSnapshot::new(f.source.paths.brand(), [global.load().unwrap()]).unwrap();
    let mut disabled = SourceServer::new(
        Arc::clone(&f.source.auth),
        Arc::clone(&f.source.projects),
        0,
        f.source.server.0.policy.clone(),
        CancellationToken::new(),
    )
    .unwrap();
    disabled.attach_project_hook(&disabled_config).unwrap();
    disabled
        .attach_semantic_model(Some(Arc::clone(&f.resources)))
        .unwrap();
    let disabled_snapshot = disabled.0.live_projects.snapshot().unwrap();
    assert_eq!(
        execute(
            &disabled.0,
            &disabled_snapshot.projects[0],
            arguments(),
            &f.source.raw,
            false,
            &Cancellation::default(),
            Instant::now() + Duration::from_secs(30)
        )
        .await
        .unwrap()["status"],
        "model_disabled"
    );
    assert!(
        crate::foreground_memory_jobs::expected_model(
            &f.source.paths,
            None,
            &Arc::new(graphit_core::artifact_store::SnapshotContext::new(
                Instant::now() + Duration::from_secs(30),
                Cancellation::default()
            )),
            &disabled
                .project_modules(
                    &graphit_hub::policy::RequestContext::new(
                        disabled.0.policy.content,
                        &Cancellation::default(),
                        Instant::now() + Duration::from_secs(30)
                    )
                    .unwrap()
                )
                .unwrap()
        )
        .unwrap()
        .is_none()
    );
    disabled.shutdown().await.unwrap();
    let cancel = Cancellation::default();
    // Restore AST for the independent empty-index and cancellation scenarios;
    // consumers read the current module settings, including this global change.
    global
        .set(
            Module::Ast.key(),
            json!(true),
            Duration::from_secs(5),
            &cancel,
        )
        .unwrap();
    let request = RequestContext::new(
        f.source.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let model = crate::ast_semantic::expected_native_nn_model(
        &f.resources.local.as_ref().unwrap().expected,
    )
    .unwrap();
    let path = f.path(0, "corpus.lance");
    let pin = graphit_data::lance_branch_store::BranchSnapshot::capture_observed(
        path.to_str().unwrap(),
        None,
        None,
    )
    .await
    .unwrap();
    // The corpus already contains lexical rows. Restore only its nullable
    // vector field: existing records keep their identity and receive no vector.
    pin.add_nullable_columns(
        graphit_ast::corpus::vector_fields(&model).unwrap(),
        request.snapshot(),
    )
    .await
    .unwrap();
    assert_eq!(direct(&f, 0).await.unwrap()["status"], "index_empty");
    // A changed recipe at the same width is rejected before the closed endpoint.
    f._bundle.body["recipe"]["max_tokens"] = json!(4096);
    let config = f._bundle.config(&[crate::models::DEFAULT_SLOT]);
    let changed = ExpectedModel::new(&config, crate::models::DEFAULT_SLOT, "cpu").unwrap();
    assert_eq!(
        changed.dimensions(),
        f.resources.local.as_ref().unwrap().expected.dimensions()
    );
    assert_ne!(
        changed.vector_id(),
        f.resources.local.as_ref().unwrap().expected.vector_id()
    );
    let model = crate::ast_semantic::expected_native_nn_model(&changed).unwrap();
    let pin = graphit_data::lance_branch_store::BranchSnapshot::capture_observed(
        path.to_str().unwrap(),
        None,
        None,
    )
    .await
    .unwrap();
    let pin = pin.drop_vector("vector", request.snapshot()).await.unwrap();
    pin.add_nullable_columns(
        graphit_ast::corpus::vector_fields(&model).unwrap(),
        request.snapshot(),
    )
    .await
    .unwrap();
    let SemanticError::Operation(error) = direct(&f, 0).await.unwrap_err() else {
        panic!("expected the current NN schema error")
    };
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert!(matches!(
        error.get_ref().and_then(|cause| cause
            .downcast_ref::<graphit_data::lance_branch_store::BranchError>()),
        Some(graphit_data::lance_branch_store::BranchError::SchemaMismatch)
    ));
    cancel.cancel();
    let snapshot = f.source.server.0.live_projects.snapshot().unwrap();
    assert!(matches!(
        execute(
            &f.source.server.0,
            &snapshot.projects[0],
            arguments(),
            &f.source.raw,
            false,
            &cancel,
            Instant::now() + Duration::from_secs(30)
        )
        .await,
        Err(SemanticError::Authentication(_) | SemanticError::Operation(_))
    ));
    // Queue ingress on the real retained-job registry, then cancel over MCP.
    let held = f.source.server.0.jobs.lock().await;
    let (client, serving) = client(&f.source.server, &f.source.raw, false).await;
    let resource_owners = Arc::strong_count(&f.resources);
    let pending = client
        .send_cancellable_request(
            rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(params(
                json!({"query":"pending","k":1}),
            ))),
            Default::default(),
        )
        .await
        .unwrap();
    pending.cancel(None).await.unwrap();
    drop(held);
    tokio::time::timeout(Duration::from_secs(5), f.source.server.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(Arc::strong_count(&f.resources), resource_owners);
    client.cancel().await.unwrap();
    serving.await.unwrap().unwrap();
    f.close().await;
    let f = Fixture::new(&[Permission::Read(Module::Memory)]).await;
    for project in 0..2 {
        assert!(matches!(
            direct(&f, project).await,
            Err(SemanticError::Authentication(_))
        ));
    }
    f.close().await;
}

fn semantic_identity_limits() -> IdentityLimits {
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

fn semantic_catalog(profile_id: &str, bytes: &[u8]) -> CatalogDraft {
    let original = SnapshotContext::service(Cancellation::default());
    let id = ProfileId::new(profile_id, 256).unwrap();
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

#[tokio::test]
async fn actual_json_code_document_nn_mcp_query_retains_data_keys_and_names() {
    let f = Fixture::new(&[Permission::Read(Module::Ast)]).await;
    let body = r#"{"outer": {"answer": 42, "escaped\u0000": true}}"#;
    f.raw_file(0, "data.json", Some(body), body).await;
    let uids = f.publish_json(0).await;
    let (client, serving) = client(&f.source.server, &f.source.raw, false).await;
    let result = client
        .call_tool(params(
            json!({"query":"find answer", "mode":"semantic", "k":10}),
        ))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false));
    let payload = result.structured_content.unwrap();
    assert_eq!(payload["status"], "available");
    let rows = payload["results"].as_array().unwrap();
    assert_eq!(rows.len(), uids.len());
    assert!(rows.iter().any(|row| row["name"] == "escaped\0"));
    for row in rows {
        assert!(uids.iter().any(|uid| row["uid"] == uid.as_str()));
        assert_eq!(row["path"], "data.json");
        assert_eq!(row["data_key"], "pairs");
        assert_eq!(row["language"], "json");
        assert!(row["distance"].is_number());
        assert!(row.get("vector").is_none());
    }
    client.cancel().await.unwrap();
    f.source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    f.close().await;
}

// Test setup derives a real Native admission for the production admitted kernel.
async fn execute(
    state: &State,
    selection: &(
        graphit_core::project_paths::ProjectPaths,
        graphit_core::project_identity::ProjectId,
        graphit_core::version_context::VersionContext,
    ),
    input: Arguments,
    credential: &str,
    local: bool,
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<Value, SemanticError> {
    let request =
        RequestContext::new(state.policy.content, cancel, deadline).map_err(io::Error::other)?;
    let original = request.snapshot();
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, &request)
    } else {
        issuer.authenticate_runtime(credential, &request)
    }
    .map_err(SemanticError::Authentication)?;
    let (paths, id, version) = selection;
    let live = CapturedPublicationScope::project(paths, *id, state.policy.scope, original)?;
    let record = live.binding()?;
    let project = ProjectCapture {
        scope: ScopeCapture {
            live: &live,
            record: &record,
        },
        version,
    };
    let authority = ProjectRecordAuthority::new(issuer);
    let read = authority
        .admit_authenticated_read(
            &caller,
            &request,
            ScopeInput {
                installation: state.auth.installation(),
                project: Some(project),
                providers: &[],
                binding: state.policy.binding,
                global_version: "",
            },
            ProjectReadModule::Ast,
        )
        .map_err(SemanticError::Authentication)?;
    Box::pin(execute_admitted(
        state,
        selection,
        input,
        &request,
        &read,
        EmbeddingCaller::Native(&caller),
    ))
    .await
}

#[cfg(target_os = "linux")]
mod lexical_rerank;
