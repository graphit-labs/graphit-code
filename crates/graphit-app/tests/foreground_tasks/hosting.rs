use super::*;
use axum::{extract::RawQuery, response::Redirect};
use graphit_auth::{
    ProjectWriteOperation, RecordStorageAdmission, RecordStorageResolver, RemoteRecordStorage,
    UserMemoryWriteOperation,
    broker::{BrokerPolicy, BrokerRequestContext, BrokerVerifier, JsonLimits, VerifierLimits},
};
use graphit_core::artifact_store::{CopyPolicy, StoreOwner, UserIdentity};
use graphit_data::graph_writer::{GraphEdge, GraphNode, GraphValue, IcebugWriter};
use graphit_data::{
    knowledge_document_projection as projection, knowledge_index as index,
    knowledge_references as refs,
};
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
    materialization::{LineageLimits, MaterializationLimits},
    registry::{
        DocumentLimits, HubLimits, HubRegistry, Installation, InstalledArtifact, InstalledBackend,
        InstalledPayload,
    },
};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{
        CreateMemory, Memory, MemoryId, MemoryReference, MemoryStoreReadLimits,
        MemoryStoreWriteLimits, NativeMemoryStore, UpdateMemory,
    },
    user_memory_store::NativeUserMemoryStore,
};
use std::collections::BTreeMap;

pub(super) const USER_RESOURCE: &str = "https://mcp.example/user-memory";

fn memory_limits(seed: &Seed) -> MemoryStoreWriteLimits {
    MemoryStoreWriteLimits {
        read: MemoryStoreReadLimits {
            native: seed.policy.read.native,
            batch_rows: seed.policy.read.batch_rows,
            max_query_bytes: NonZeroUsize::new(seed.policy.content.max_input_bytes).unwrap(),
        },
        artifacts: seed.policy.write.artifacts,
        max_input_bytes: NonZeroUsize::new(seed.policy.content.max_input_bytes).unwrap(),
        max_row_bytes: NonZeroUsize::new(seed.policy.content.max_output_bytes).unwrap(),
    }
}

fn memory_input(key: &str, references: Vec<MemoryReference>) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: key.into(),
        body: format!("Complete native body: {key}"),
        attributes: MemoryAttributes {
            classification: MemoryClass::Fact,
            mandatory: false,
            important: false,
        },
        tags: vec!["foreground-references".into()],
        references: Some(references),
    }
}

fn memory_reference(seed: &Seed, kind: &str, id: &str) -> MemoryReference {
    let project = ProjectIdentityStore::at(&seed.project)
        .load()
        .unwrap()
        .unwrap()
        .id();
    serde_json::from_value(json!({
        "target_kind":kind, "scope":{"Project":project}, "id":id,
        "context":null, "relation":"supports", "field":"evidence"
    }))
    .unwrap()
}

enum MemorySeed {
    Create(CreateMemory),
    Update(MemoryId, UpdateMemory),
}

async fn write_memory(seed: &Seed, operation: MemorySeed) -> Memory {
    write_memory_at(seed, &seed.project, operation).await
}

async fn write_memory_at(seed: &Seed, paths: &ProjectPaths, operation: MemorySeed) -> Memory {
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        seed.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let id = ProjectIdentityStore::at(paths)
        .load()
        .unwrap()
        .unwrap()
        .id();
    let checkout = GitCheckout::inspect(paths.root(), Duration::from_secs(5), &cancel).unwrap();
    let version = VersionContext::select(paths, checkout.as_checkout(), "", None, "").unwrap();
    let project =
        CapturedPublicationScope::project(paths, id, seed.policy.scope, original.snapshot())
            .unwrap();
    let binding = project.binding().unwrap();
    let issuer = seed.auth.issuer();
    let caller = issuer.authenticate_runtime(&seed.raw, &original).unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_write(
            &caller,
            &original,
            ScopeInput {
                installation: seed.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &project,
                        record: &binding,
                    },
                    version: &version,
                }),
                providers: &[],
                binding: NativeBinding::Codex,
                global_version: "",
            },
            ProjectWriteOperation::Memory,
        )
        .unwrap();
    match operation {
        MemorySeed::Create(input) => {
            NativeMemoryStore::create(original.snapshot(), &admission, input, memory_limits(seed))
                .await
                .unwrap()
        }
        MemorySeed::Update(id, input) => NativeMemoryStore::update(
            original.snapshot(),
            &admission,
            id,
            input,
            memory_limits(seed),
        )
        .await
        .unwrap(),
    }
}

fn incoming_user(seed: &Seed, broker: &Broker) -> BrokerVerifier {
    seed.provider(
        "seed",
        ProviderKind::Broker(BrokerService {
            endpoint: broker.issuer.origin.clone(),
            mcp_resource: Some(USER_RESOURCE.into()),
            allow_anonymous: false,
        }),
    );
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        seed.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let issuer = seed.auth.issuer();
    let caller = issuer.authenticate_runtime(&seed.raw, &original).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let manager = NativeProfileManager::new(issuer);
    let snapshot = manager
        .begin(ProfileManagerAuthority::Admin(&parent))
        .unwrap();
    let revision = snapshot
        .providers()
        .find(|p| p.name() == "seed")
        .unwrap()
        .revision();
    BrokerVerifier::new(
        BrokerPolicy {
            endpoint: &broker.issuer.origin,
            mcp_resource: Some(USER_RESOURCE),
            provider_revision: revision,
            network: ClientPolicy {
                origin: &broker.issuer.origin,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
        },
        NetworkLimits {
            url_bytes: 8192,
            request_body_bytes: 8192,
            response_body_bytes: 32768,
            max_in_flight: 8,
            header_bytes: 16384,
            header_count: 32,
            max_idle_per_host: 2,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 2,
            max_certificate_bytes: 8192,
        },
        VerifierLimits {
            token_bytes: 4096,
            json: JsonLimits {
                bytes: 32768,
                nodes: 256,
                depth: 16,
                scalar_bytes: 8192,
                collection_entries: 64,
            },
            max_keys: 8,
            metadata_ttl: Duration::from_secs(60),
            key_ttl: Duration::from_secs(60),
            unknown_key_cooldown: Duration::from_secs(1),
        },
    )
    .unwrap()
}

async fn write_user_memory(
    seed: &Seed,
    verifier: &BrokerVerifier,
    token: &str,
    input: CreateMemory,
) -> Memory {
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        seed.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let proof = verifier
        .verify(token, BrokerRequestContext::new(original.snapshot()))
        .await
        .unwrap();
    let issuer = seed.auth.issuer();
    let verified = issuer
        .user_from_incoming(&proof, verifier, &original)
        .unwrap();
    let StoreOwner::Installation { installation_id } = seed.auth.installation().live.owner() else {
        panic!("installation owner");
    };
    let user = CapturedPublicationScope::user(
        &seed.paths,
        installation_id.clone(),
        UserIdentity::new(
            verified.issuer().unwrap().into(),
            verified.subject().unwrap().into(),
        )
        .unwrap(),
        seed.policy.scope,
        original.snapshot(),
    )
    .unwrap();
    let binding = user.binding().unwrap();
    let admission = issuer
        .admit_incoming_user_memory_write(
            &proof,
            verifier,
            &original,
            "seed",
            ScopeCapture {
                live: &user,
                record: &binding,
            },
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    NativeUserMemoryStore::create(original.snapshot(), &admission, input, memory_limits(seed))
        .await
        .unwrap()
}

struct LocalSeedStorage;

impl RecordStorageResolver for LocalSeedStorage {
    fn resolve<'a>(
        &'a self,
        admission: RecordStorageAdmission<'a>,
    ) -> std::pin::Pin<
        Box<dyn Future<Output = std::io::Result<Option<RemoteRecordStorage>>> + Send + 'a>,
    > {
        Box::pin(async move {
            admission.recheck().map_err(std::io::Error::other)?;
            Ok(None)
        })
    }
}

fn authored_page<'a>(slug: &'a str, title: &'a str) -> projection::AuthoredPageFields<'a> {
    projection::AuthoredPageFields {
        slug: Some(slug),
        title: Some(title),
        body: Some("# Complete foreground reference page\n"),
        kind: Some("page"),
        summary: Some("Native reference journey"),
        description: Some("Authored fields published through the native SDK"),
        summary_truncated: false,
        source_path: "docs/foreground-references.md",
        physical_digest: None,
        compilation_digest: projection::CompilationDigest32::from_bytes(
            Sha256::digest(b"# Complete foreground reference page\n").into(),
        ),
        recipe: projection::RecipeFields {
            id: projection::AUTHORED_RECIPE_ID,
            parameters: Some(projection::AuthoredParameters {
                fallback_title: "Native page",
                kind_on_absent: "page",
                max_summary_bytes: NonZeroUsize::new(64).unwrap(),
            }),
        },
        updated_at: Some(UNIX_EPOCH + Duration::from_secs(1)),
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: &[],
    }
}

async fn publish_knowledge(
    seed: &Seed,
    paths: &ProjectPaths,
    task_id: &str,
    session_id: &str,
    memory_id: &str,
    imported: bool,
) -> index::KnowledgeFirstPublication {
    publish_knowledge_with_projection(seed, paths, task_id, session_id, memory_id, imported, true)
        .await
}

async fn publish_knowledge_with_projection(
    seed: &Seed,
    paths: &ProjectPaths,
    task_id: &str,
    session_id: &str,
    memory_id: &str,
    imported: bool,
    with_references: bool,
) -> index::KnowledgeFirstPublication {
    publish_knowledge_with_mode(
        seed,
        paths,
        task_id,
        session_id,
        memory_id,
        imported,
        if with_references {
            KnowledgeFixtureMode::References
        } else {
            KnowledgeFixtureMode::SourceOnly
        },
    )
    .await
}

enum KnowledgeFixtureMode {
    BrowserReferences,
    References,
    SourceOnly,
    Empty,
}

async fn publish_knowledge_with_mode(
    seed: &Seed,
    paths: &ProjectPaths,
    task_id: &str,
    session_id: &str,
    memory_id: &str,
    imported: bool,
    mode: KnowledgeFixtureMode,
) -> index::KnowledgeFirstPublication {
    let with_references = matches!(
        mode,
        KnowledgeFixtureMode::References | KnowledgeFixtureMode::BrowserReferences
    );
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        seed.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap();
    let referenced_id = ProjectIdentityStore::at(&seed.project)
        .load()
        .unwrap()
        .unwrap()
        .id();
    let id = ProjectIdentityStore::at(paths)
        .load()
        .unwrap()
        .unwrap()
        .id();
    let checkout = GitCheckout::inspect(paths.root(), Duration::from_secs(5), &cancel).unwrap();
    let version = VersionContext::select(paths, checkout.as_checkout(), "", None, "").unwrap();
    let project =
        CapturedPublicationScope::project(paths, id, seed.policy.scope, original.snapshot())
            .unwrap();
    let binding = project.binding().unwrap();
    let issuer = seed.auth.issuer();
    let caller = issuer.authenticate_runtime(&seed.raw, &original).unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_write(
            &caller,
            &original,
            ScopeInput {
                installation: seed.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &project,
                        record: &binding,
                    },
                    version: &version,
                }),
                providers: &[],
                binding: NativeBinding::Codex,
                global_version: "",
            },
            ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    let additional_titles: Vec<_> = if matches!(mode, KnowledgeFixtureMode::BrowserReferences) {
        (1..=21)
            .map(|index| format!("Foreground additional {index:02}"))
            .collect()
    } else {
        vec![]
    };
    let mut pages = if matches!(mode, KnowledgeFixtureMode::Empty) {
        vec![]
    } else if !with_references {
        vec![authored_page("Projection absent", "Projection absent")]
    } else if imported {
        vec![authored_page("Imported namespace", "Imported namespace")]
    } else {
        vec![
            authored_page("Foreground wiki", "Foreground wiki"),
            authored_page("No outgoing", "No outgoing"),
        ]
    };
    pages.extend(
        additional_titles
            .iter()
            .map(|title| authored_page(title, title)),
    );
    let n = |v| NonZeroUsize::new(v).unwrap();
    let prepared = projection::prepare(
        original.snapshot(),
        &pages,
        &[],
        projection::KnowledgeDocumentProjectionLimits {
            max_documents: n(seed.policy.content.max_nodes),
            max_observations: n(seed.policy.content.max_nodes),
            max_rows: n(seed.policy.content.max_nodes),
            max_field_bytes: n(seed.policy.content.max_input_bytes),
            max_text_bytes: n(seed.policy.content.max_input_bytes),
        },
    )
    .unwrap();
    let limits = index::KnowledgeIndexWriteLimits {
        content: index::KnowledgeIndexContentLimits {
            max_identifier_bytes: n(seed.policy.content.max_scalar_bytes),
        },
        native: seed.policy.read.native,
        artifacts: seed.policy.write.artifacts,
    };
    if !with_references {
        return index::publish_native_authored(original.snapshot(), &admission, &prepared, limits)
            .await
            .unwrap();
    }
    let authored = [
        refs::KnowledgeAuthoredReference {
            target_kind: refs::KnowledgeReferenceTargetKind::Task,
            scope: refs::KnowledgeReferenceScope::Project(referenced_id),
            context: None,
            id: task_id,
            relation: "supports",
            field: Some("task"),
        },
        refs::KnowledgeAuthoredReference {
            target_kind: refs::KnowledgeReferenceTargetKind::Session,
            scope: refs::KnowledgeReferenceScope::Project(referenced_id),
            context: None,
            id: session_id,
            relation: "supports",
            field: Some("session"),
        },
        refs::KnowledgeAuthoredReference {
            target_kind: refs::KnowledgeReferenceTargetKind::Memory,
            scope: refs::KnowledgeReferenceScope::Project(referenced_id),
            context: None,
            id: memory_id,
            relation: "supports",
            field: Some("memory"),
        },
    ];
    let mut fields = if imported {
        vec![refs::KnowledgeReferenceFields {
            slug: "Imported namespace",
            state: refs::KnowledgeDocumentState::Active,
            namespace: refs::KnowledgeReferenceNamespace {
                scope: refs::KnowledgeReferenceScope::Project(id),
                context: None,
            },
            authored: &authored,
        }]
    } else {
        vec![
            refs::KnowledgeReferenceFields {
                slug: "Foreground wiki",
                state: refs::KnowledgeDocumentState::Active,
                namespace: refs::KnowledgeReferenceNamespace {
                    scope: refs::KnowledgeReferenceScope::Project(id),
                    context: None,
                },
                authored: &authored,
            },
            refs::KnowledgeReferenceFields {
                slug: "No outgoing",
                state: refs::KnowledgeDocumentState::Active,
                namespace: refs::KnowledgeReferenceNamespace {
                    scope: refs::KnowledgeReferenceScope::Project(id),
                    context: None,
                },
                authored: &[],
            },
        ]
    };
    fields.extend(
        additional_titles
            .iter()
            .map(|title| refs::KnowledgeReferenceFields {
                slug: title,
                state: refs::KnowledgeDocumentState::Active,
                namespace: refs::KnowledgeReferenceNamespace {
                    scope: refs::KnowledgeReferenceScope::Project(id),
                    context: None,
                },
                authored: &[],
            }),
    );
    let references =
        refs::prepare_knowledge_references(original.snapshot(), &prepared, &fields, limits.content)
            .unwrap();
    index::publish_native_authored_with_references(
        original.snapshot(),
        &admission,
        &prepared,
        &references,
        limits,
    )
    .await
    .unwrap()
}

async fn register_knowledge(seed: &Seed, published: &index::KnowledgeFirstPublication) -> Value {
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        seed.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap();
    let StoreOwner::Installation { installation_id } = seed.auth.installation().live.owner() else {
        panic!("installation owner");
    };
    let policy = &seed.policy;
    let limits = HubLimits::new(
        DocumentLimits {
            bytes: policy.content.max_input_bytes,
            entries: policy.content.max_collection_entries,
            key_bytes: policy.content.max_scalar_bytes,
            json_nodes: policy.content.max_nodes,
            json_string_bytes: policy.content.max_input_bytes,
        },
        policy.role_reference,
        policy.write.artifacts,
        MaterializationLimits {
            output: policy.write.artifacts.copy_limits(),
            snapshots: CopyLimits::new(16 * 1024 * 1024, 512, 65536, 16).unwrap(),
            lineage: LineageLimits::new(4096, 8192, 65536).unwrap(),
        },
    )
    .unwrap();
    let registry = HubRegistry::global(&seed.paths, installation_id, limits).unwrap();
    let id = ArtifactId::new("vendor/foreground-references", policy.role_reference).unwrap();
    let version = ConcreteVersion::new("exact-version", policy.role_reference).unwrap();
    let project = ProjectIdentityStore::at(&seed.project)
        .load()
        .unwrap()
        .unwrap()
        .id();
    let source = SourceRef::hub_cache(
        &seed.paths,
        project,
        ArtifactType::Knowledge,
        id.clone(),
        None,
        version.clone(),
        policy.role_reference,
    )
    .unwrap();
    let generation = &published.default;
    let builder = lance::dataset::builder::DatasetBuilder::from_uri(&generation.corpus_uri);
    let builder = match generation.physical_branch.as_deref() {
        Some(branch) => builder.with_branch(branch, Some(generation.version)),
        None => builder.with_version(generation.version),
    };
    let mut corpus = Box::pin(builder.load()).await.unwrap();
    let target = source.directory().as_path().join("index.lance");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    Box::pin(corpus.deep_clone(target.to_str().unwrap(), generation.version, None))
        .await
        .unwrap();
    registry
        .writer(
            original.remaining().unwrap().unwrap(),
            original.cancellation(),
        )
        .unwrap()
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: ArtifactType::Knowledge,
            id: id.clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source: source,
                alias: None,
                entry_name: None,
                display: None,
                backend: InstalledBackend::Native,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })
        .unwrap();
    json!({"kind":"imported_knowledge","artifact":{"id":id.as_str(),"version":version.as_str()}})
}

async fn record_read(
    client: &reqwest::Client,
    origin: &str,
    cookie: &str,
    path: &str,
    body: Value,
) -> reqwest::Response {
    client
        .post(format!("{origin}{path}"))
        .header(header::ORIGIN, origin)
        .header(header::COOKIE, cookie)
        .json(&body)
        .send()
        .await
        .unwrap()
}

fn install_ast_probe_context(fixture: &Prepared, project: &ProjectPaths) {
    use arrow_array::{RecordBatch, StringArray};
    use graphit_ast::{
        catalog::{
            CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind,
        },
        entity_search,
        indexed_source::{read_indexed_source, source_schema},
        native_nn::NativeNnSource,
        prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
        profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
        scalar_policy::{RegexLimits, ScalarPolicy},
        semantic_graph::{
            AstGraphRelationTarget, AstGraphTsInput, AstGraphValue, assemble_graph_ts,
        },
        source_inspection::IndexedSource,
        symbols::{IdentityLimits, IdentityLimitsInput},
        tree_sitter_context::PreparedTreeSitterContexts,
        tree_sitter_engine::builtin,
        tree_sitter_model::ParsedTreeSitterDocument,
    };
    use graphit_core::{
        artifact_store::{CapturedSourceReader, SourceReadLimits},
        installation_identity::InstallationIdentityStore,
        native_path::NativePath,
    };
    use graphit_data::{
        graph_search_sidecar::GraphSearchSpool, lance_module_store::LanceModuleStore,
    };
    use graphit_hub::native_package::{self, PackageKind};

    // Lance remains on the existing multithread runtime; parser owners stay local.
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let config = fixture.snapshot(None);
            let policy = HubResourcePolicy::resolve(&config).unwrap();
            let cancel = Cancellation::default();
            let request = policy.request_context(Instant::now(), &cancel).unwrap();
            let original = Arc::clone(request.snapshot());
            let id = ProjectIdentityStore::at(project)
                .load()
                .unwrap()
                .unwrap()
                .id();
            let source_fixture = tempfile::tempdir_in(fixture.temp.path()).unwrap();
            let corpus = source_fixture.path().join("source.lance");
            let text = fs::read_to_string(project.root().join("main.rs")).unwrap();
            let linked = fs::read_to_string(project.root().join("relationships.go")).unwrap();
            original.check().unwrap();
            let raw_store = LanceModuleStore::create(
                corpus.to_str().unwrap(),
                source_schema(),
                vec![
                    RecordBatch::try_new(
                        source_schema(),
                        vec![
                            Arc::new(StringArray::from(vec!["main.rs", "relationships.go"])),
                            Arc::new(StringArray::from(vec![text.as_str(), linked.as_str()])),
                        ],
                    )
                    .unwrap(),
                ],
            )
            .await
            .unwrap();
            drop(text);
            let raw_snapshot = raw_store.snapshot();
            let mut writer = IcebugWriter::new(Arc::clone(&original)).unwrap();
            let mut search = GraphSearchSpool::new(graphit_ast::corpus::schema(), Arc::clone(&original)).unwrap();
            let files: [(&str, &[u8]); 2] = [
                ("main.rs", include_bytes!("../../../../crates/graphit-ast/profiles/queries/rust.yaml")),
                ("relationships.go", include_bytes!("../../../../crates/graphit-ast/profiles/queries/go.yaml")),
            ];
            for (path, profile) in files {
            let raw = read_indexed_source(&raw_snapshot, path, &original)
                .await
                .unwrap();
            let IndexedSource::Available(text) = raw.source(&original).unwrap() else {
                panic!("published fixture Raw must retain its actual source");
            };
            let profile_id = ProfileId::new(if path == "main.rs" { "browser-rust" } else { "browser-go" }, 256).unwrap();
            let decoder = ProfileDecoder::new(
                ScalarPolicy::new(RegexLimits {
                    compiled_size_policy_bytes: 1 << 20,
                    dfa_cache_policy_bytes: 1 << 20,
                })
                .unwrap(),
            );
            let profiles = [decoder
                .decode(
                    ProfileInput {
                        id: &profile_id,
                        bytes: profile,
                    },
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
            let catalog = CatalogDraft::build(
                &[ProfileLayer {
                    kind: ProfileLayerKind::Runtime,
                    profiles: &profiles,
                }],
                &CatalogLimits::new(CatalogLimitsInput {
                    policy_entries: 4096,
                    selector_bytes: 32768,
                })
                .unwrap(),
                &original,
            )
            .unwrap();
            let factory = PreparedTreeSitterContexts::prepare(
                &catalog.profiles()[0],
                builtin(catalog.profiles()[0].grammar().value()).unwrap(),
                &original,
            )
            .unwrap();
            let span = ByteSpan {
                start: 0,
                end: u64::try_from(text.len()).unwrap(),
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
            )
            .unwrap();
            let components = [path];
            let parsed = ParsedTreeSitterDocument::collect(
                &factory,
                &input,
                &components,
                IdentityLimits::new(IdentityLimitsInput {
                    source_bytes: 1 << 20,
                    path_components: 16,
                    path_component_bytes: 4096,
                    identity_text_bytes: 4096,
                    owner_depth: 64,
                    uid_bytes: 32768,
                })
                .unwrap(),
            )
            .unwrap();
            let document = parsed.assemble().unwrap();
            let parts = assemble_graph_ts(
                AstGraphTsInput {
                    document: &document,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )
            .unwrap()
            .into_parts();
            let expected = if path == "main.rs" { "Alpha" } else { "LinkedAlpha" };
            assert!(parts.nodes.iter().any(|node| {
                node.label() == "Function"
                    && matches!(node.properties().get("name"), Some(AstGraphValue::String(found)) if found == expected)
            }));
            if path == "main.rs" {
                assert!(!parts.outcomes.is_empty());
                assert!(parts.outcomes.iter().all(|outcome| matches!(
                    outcome.target(), AstGraphRelationTarget::ObservedOnly
                )));
            } else {
                assert!(parts.edges.iter().any(|edge| edge.kind() == "CALLS"));
            }
            search
                .push(&graphit_ast::corpus::encode_file(graphit_ast::corpus::schema(),entity_search::encode_file(&parts,text,&original).unwrap(),RecordBatch::new_empty(graphit_ast::native_nn::native_nn_projection_schema()),None,&original).unwrap())
                .unwrap();
            let nodes = parts
                .nodes
                .into_iter()
                .chain(std::iter::once(parts.file_observation))
                .map(|node| {
                    let (label, key, properties) = node.into_parts();
                    let properties: BTreeMap<_, _> = properties
                        .into_iter()
                        .map(|(key, value)| {
                            let value = match value {
                                AstGraphValue::String(value) => GraphValue::String(value),
                                AstGraphValue::Int64(value) => GraphValue::Int64(value),
                                AstGraphValue::Bool(value) => GraphValue::Bool(value),
                            };
                            (key, value)
                        })
                        .collect();
                    GraphNode {
                        label,
                        key,
                        properties,
                    }
                });
            let edges = parts.edges.into_iter().map(|edge| {
                let (kind, source_key, target_key, uid) = edge.into_parts();
                GraphEdge {
                    kind,
                    source_key,
                    target_key,
                    uid,
                }
            });
            writer = writer.push_file(nodes, edges).unwrap();
            }
            let output = writer.finish().unwrap();
            search.finish(&output, entity_search::TEXT_COLUMNS).await.unwrap();
            let clone = raw_snapshot
                .spawn_native_clone_local(
                    tempfile::tempdir_in(fixture.temp.path()).unwrap(),
                    Arc::clone(&original),
                )
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            fs::rename(clone.directory(), output.directory().join("source.lance")).unwrap();
            original.check().unwrap();
            {
                let files = policy
                    .snapshot_policy(CopyPolicy::RejectFileLinks, std::iter::empty())
                    .unwrap();
                let read = SourceReadLimits::new(65536, 4096, 1 << 20).unwrap();
                let graph = CapturedSourceReader::capture(
                    &output.directory(),
                    graphit_fs::NodeKind::Directory,
                    &files,
                    read,
                    Arc::clone(&original),
                )
                .unwrap();
                let archive = fixture.temp.path().join("app-ast.graphit");
                native_package::export_ast_graph(&graph, &archive, &original)
                    .unwrap();
                let package = CapturedSourceReader::capture(
                    &archive,
                    graphit_fs::NodeKind::File,
                    &files,
                    read,
                    Arc::clone(&original),
                )
                .unwrap();
                let installed = fixture.temp.path().join("imported-app-ast");
                native_package::import(
                    &package,
                    Some(PackageKind::Ast),
                    &installed,
                    policy.copy(),
                    &original,
                )
                .unwrap();
                let identity = InstallationIdentityStore::new(&fixture.paths)
                    .unwrap()
                    .load()
                    .unwrap();
                let registry =
                    HubRegistry::global(&fixture.paths, identity.unit_id().unwrap(), policy.hub())
                        .unwrap();
                let projection = registry.limits().projection();
                let source = SourceRef::local_link(
                    NativePath::new(installed).unwrap(),
                    id,
                    ArtifactType::Ast,
                    ArtifactId::new("app-ast", projection).unwrap(),
                    ConcreteVersion::new("release-1", projection).unwrap(),
                    projection,
                )
                .unwrap();
                registry
                    .writer(Duration::from_secs(5), &cancel)
                    .unwrap()
                    .put_metadata(InstalledArtifact {
                        consumer: graphit_core::artifact_store::PublicationSlot::Scope,
                        artifact_type: ArtifactType::Ast,
                        id: source.id().clone(),
                        installations: vec![Installation {
                            target: InstallationTarget::GlobalNative,
                            original_source: source,
                            alias: None,
                            entry_name: None,
                            display: None,
                            backend: InstalledBackend::Native,
                            payload: InstalledPayload::Original {
                                selection_binding: None,
                            },
                        }],
                    })
                    .unwrap();
            }
            original.check().unwrap();
        });
    });
}

pub(super) async fn authorize(
    State(issuer): State<Arc<Issuer>>,
    RawQuery(raw): RawQuery,
) -> Redirect {
    let fields: HashMap<String, String> =
        url::form_urlencoded::parse(raw.as_deref().unwrap_or("").as_bytes())
            .into_owned()
            .collect();
    assert_eq!(fields["client_id"], "web-client");
    assert_eq!(fields["response_type"], "code");
    assert_eq!(fields["code_challenge_method"], "S256");
    let mut callback = url::Url::parse(&fields["redirect_uri"]).unwrap();
    assert_eq!(callback.host_str(), Some("127.0.0.1"));
    assert_eq!(callback.path(), "/auth/callback");
    *issuer.nonce.lock().unwrap() = fields["nonce"].clone();
    *issuer.challenge.lock().unwrap() = fields["code_challenge"].clone();
    callback
        .query_pairs_mut()
        .append_pair("code", "code")
        .append_pair("state", &fields["state"]);
    Redirect::to(callback.as_str())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hosted_task_routes_signed_oauth() {
    verify_hosted_task_routes(None).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "native browser qualification requires a browser observer and GRAPHIT_UI_BROWSER_PROBE_FILE"]
async fn hosted_task_routes_signed_oauth_and_browser_probe() {
    verify_hosted_task_routes(Some(browser_probe_path())).await;
}

fn browser_probe_path() -> std::path::PathBuf {
    std::env::var_os("GRAPHIT_UI_BROWSER_PROBE_FILE")
        .map(std::path::PathBuf::from)
        .expect("set GRAPHIT_UI_BROWSER_PROBE_FILE to a private probe path")
}

async fn verify_hosted_task_routes(probe: Option<std::path::PathBuf>) {
    let broker = Broker::new().await;
    let fixture = prepared(&broker).await;
    let root = fixture.temp.path().join("other-project");
    fs::create_dir(&root).unwrap();
    let other = ProjectPaths::at(&root, &fixture.paths).unwrap();
    let cancel = Cancellation::default();
    ProjectIdentityStore::at(&other)
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .ensure(&ProjectMetadata::new("other-project").unwrap())
        .unwrap();
    let other_id = ProjectIdentityStore::at(&other)
        .load()
        .unwrap()
        .unwrap()
        .id();
    fs::write(
        fixture.project.root().join("main.rs"),
        "fn Alpha() {}\nfn Beta() { Alpha(); }\n",
    )
    .unwrap();
    fs::write(
        other.root().join("main.rs"),
        "fn OtherAlpha() {}\nfn Beta() { OtherAlpha(); }\n",
    )
    .unwrap();
    if probe.is_some() {
        fs::write(
            fixture.project.root().join("relationships.go"),
            "package fixture\nfunc LinkedAlpha() {}\nfunc LinkedBeta() { LinkedAlpha() }\n",
        )
        .unwrap();
        install_ast_probe_context(&fixture, &fixture.project);
    }
    let physical_path = "raw.unknown";
    let physical_text = "physical source outside the selected grammars\n";
    fs::write(fixture.project.root().join(physical_path), physical_text).unwrap();
    broker.issuer.ast_allowed.store(true, Ordering::SeqCst);
    let global = ConfigStore::global(&fixture.paths).unwrap();
    for (key, enabled) in [
        ("modules.ast", true),
        ("modules.knowledge", false),
        ("modules.embedding", false),
        ("modules.dream", false),
    ] {
        global
            .set(
                ConfigKey::new(key).unwrap(),
                json!(enabled),
                Duration::from_secs(5),
                &cancel,
            )
            .unwrap();
    }
    let environment = vec![("PATH".into(), fixture.temp.path().as_os_str().to_owned())];
    let owner = fixture
        .start_browser_records(
            config(&broker.issuer),
            &[&fixture.project, &other],
            environment,
        )
        .await
        .unwrap();
    let origin = format!("http://{}", owner.address().unwrap());
    let cancellation = Cancellation::default();
    let stopping = cancellation.clone();
    let running = tokio::spawn(async move { owner.run(&stopping).await });
    let client = client();
    for path in [
        "/task/gantt",
        "/task/explorer",
        "/task/sessions",
        "/task/explorer/task-deep",
        "/task/sessions/session-deep",
        "/ast/contexts",
        "/ast/explorer/project",
    ] {
        let response = client.get(format!("{origin}{path}")).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );
        assert_eq!(
            response.bytes().await.unwrap().as_ref(),
            include_bytes!("../../assets/ui/index.html")
        );
    }
    assert_eq!(
        client
            .get(format!("{origin}/auth/session"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let cookie = login(&client, &origin, &broker.issuer).await;
    let contexts = read(
        &client,
        &origin,
        &cookie,
        "contexts",
        json!({"selection":{}}),
    )
    .await;
    assert_eq!(contexts.status(), StatusCode::OK);
    let contexts = contexts.json::<Value>().await.unwrap();
    assert_eq!(contexts["contexts"].as_array().unwrap().len(), 2);
    assert_eq!(
        contexts["current"]["project_id"],
        ProjectIdentityStore::at(&fixture.project)
            .load()
            .unwrap()
            .unwrap()
            .id()
            .to_string()
    );
    let response = read(
        &client,
        &origin,
        &cookie,
        "catalogue",
        json!({"selection":contexts["current"],"input":{"filter":{},"page_size":20}}),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "hosted task catalogue: {}",
        response.text().await.unwrap()
    );
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let page = response.json::<Value>().await.unwrap();
    let results = page["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    let scheduled = results
        .iter()
        .find(|task| task["id"] == fixture.task)
        .expect("seeded scheduled task in the current catalogue");
    assert_eq!(
        scheduled["schedule"],
        json!({"start_date":"2028-02-28","end_date":"2028-03-01","due_date":"2028-03-02"})
    );
    let dependent = results
        .iter()
        .find(|task| task["id"] == fixture.dependent)
        .expect("seeded dependent task in the current catalogue");
    assert_eq!(dependent["depends_on"], json!([fixture.task]));
    assert_eq!(dependent.get("schedule"), Some(&Value::Null));
    assert!(!page.to_string().contains(&fixture.fence));
    assert!(
        !page
            .to_string()
            .contains(&*broker.issuer.issued.lock().unwrap())
    );

    // AST admission is independent of the Task permission used above.
    broker.issuer.allowed.store(false, Ordering::SeqCst);
    assert_eq!(
        read(
            &client,
            &origin,
            &cookie,
            "catalogue",
            json!({"selection":contexts["current"],"input":{"filter":{},"page_size":20}}),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let ast_contexts = client
        .post(format!("{origin}/api/ast/contexts"))
        .header(header::ORIGIN, &origin)
        .header(header::COOKIE, &cookie)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(ast_contexts.status(), StatusCode::OK);
    let ast_contexts = ast_contexts.json::<Value>().await.unwrap();
    assert_eq!(ast_contexts["contexts"].as_array().unwrap().len(), 2);
    let context = &ast_contexts["current"];
    let selection =
        json!({"project_dir":context["project_dir"],"project_version":context["project_version"]});
    let sample = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let response = client
                .post(format!("{origin}/api/ast/sample"))
                .header(header::ORIGIN, &origin)
                .header(header::COOKIE, &cookie)
                .json(&json!({"selection":selection}))
                .send()
                .await
                .unwrap();
            let status = response.status();
            let body = response.text().await.unwrap();
            match status {
                StatusCode::OK if body.contains("Alpha") => break body,
                StatusCode::OK | StatusCode::NOT_FOUND => tokio::task::yield_now().await,
                _ => panic!("unexpected AST sample response {status}: {body}"),
            }
        }
    })
    .await
    .expect("local AST producer did not publish Alpha");
    let sample: Value = serde_json::from_str(&sample).unwrap();
    assert!(
        sample["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["name"] == "Alpha")
    );
    let schema = client
        .post(format!("{origin}/api/ast/schema"))
        .header(header::ORIGIN, &origin)
        .header(header::COOKIE, &cookie)
        .json(&json!({"selection":selection}))
        .send()
        .await
        .unwrap();
    assert_eq!(schema.status(), StatusCode::OK);

    // The interactive qualification reuses the compiled host and local signed issuer.
    let mut finished = true;
    if let Some(path) = &probe {
        fs::write(path, serde_json::to_vec(&json!({"origin":origin,"task_id":fixture.task,"session_id":fixture.session,"other_project_id":other_id.to_string(),"ast_physical_path":physical_path,"ast_physical_text":physical_text,"ast_selection":selection,"ast_contexts":ast_contexts,"ast_path":"/ast/explorer/project","task_read_allowed":false})).unwrap()).unwrap();
        let completion = path.with_extension("done");
        let deadline = Instant::now() + Duration::from_secs(900);
        finished = false;
        while Instant::now() < deadline {
            if completion.exists() {
                finished = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    cancellation.cancel();
    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    fixture.assert_clean();
    assert!(finished, "browser probe expired; host was drained");
    assert_eq!(broker.issuer.userinfo.load(Ordering::SeqCst), 0);
    let check = fixture.start(None).unwrap();
    check.shutdown().await.unwrap();
    fixture.assert_clean();
    if let Some(path) = probe {
        fs::write(path.with_extension("result.json"), serde_json::to_vec(&json!({"finished":true,"project_calls":broker.issuer.calls.load(Ordering::SeqCst),"userinfo_calls":0,"host_joined":true,"discovery_clean":true})).unwrap()).unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hosted_record_references_signed_oauth() {
    verify_hosted_record_references(None).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "native browser qualification requires a browser observer and GRAPHIT_UI_BROWSER_PROBE_FILE"]
async fn hosted_record_references_signed_oauth_and_browser_probe() {
    verify_hosted_record_references(Some(browser_probe_path())).await;
}

async fn verify_hosted_record_references(probe: Option<std::path::PathBuf>) {
    let records_browser =
        probe.is_some() && std::env::var("GRAPHIT_UI_BROWSER_SCENARIO").as_deref() == Ok("db07");
    let broker = Broker::with_record_sources(true).await;
    broker
        .issuer
        .memory_writes
        .store(records_browser, Ordering::SeqCst);
    let seed = Seed::with_browser_records(true, records_browser).await;
    seed.provider("broker", broker_kind(&broker.issuer.origin));
    let other = seed.reference_project.as_ref().unwrap().clone();
    let mirror = seed.reference_mirror.as_ref().unwrap().clone();
    let unprojected = seed.unprojected_knowledge.as_ref().unwrap().clone();
    let project = ProjectIdentityStore::at(&seed.project)
        .load()
        .unwrap()
        .unwrap()
        .id();
    let other_id = ProjectIdentityStore::at(&other)
        .load()
        .unwrap()
        .unwrap()
        .id();
    let unprojected_id = ProjectIdentityStore::at(&unprojected)
        .load()
        .unwrap()
        .unwrap()
        .id();
    let created = write_memory(
        &seed,
        MemorySeed::Create(memory_input("Created foreground memory", vec![])),
    )
    .await;
    let task_references = json!([
        {"target":{"type":"memory","id":created.id.to_string(),"scope":"project","scope_id":project},"relation":"supports","field":"evidence"},
        {"target":{"type":"knowledge","id":"Foreground wiki","scope":"project","scope_id":project},"relation":"derived_from","field":"contract"}
    ]);
    let session = seed
        .write(
            "create_session",
            json!({
                "idempotency_key":"foreground-references-session", "title":"References session",
                "description":"Native references journey", "strategy":"bounded",
                "references":task_references,
            }),
        )
        .await;
    let task = seed
        .write(
            "create_task",
            json!({
                "idempotency_key":"foreground-references-task", "session_id":session["id"],
                "title":"References journey", "description":"Task to native Memory and Knowledge",
                "task_type":"TASK", "priority":1, "references":task_references,
                "checks":[
                    {"kind":"acceptance","text":"Read the native references journey"},
                    {"kind":"test","text":"Validate the compiled browser journey"}
                ],
            }),
        )
        .await;
    let task_id = task["specification"]["id"].as_str().unwrap();
    let session_id = session["id"].as_str().unwrap();
    let updated = write_memory(
        &seed,
        MemorySeed::Update(
            created.id,
            UpdateMemory {
                expected_revision: created.revision,
                idempotency_key: "foreground-references-memory-update".into(),
                reason: "Capture the linked native records".into(),
                title: Some("Revised foreground memory".into()),
                body: Some("The revised body preserves creation and history.".into()),
                classification: None,
                mandatory: None,
                important: None,
                tags: None,
                references: Some(vec![
                    memory_reference(&seed, "task", task_id),
                    memory_reference(&seed, "session", session_id),
                    memory_reference(&seed, "knowledge", "Foreground wiki"),
                ]),
            },
        ),
    )
    .await;
    assert!(updated.revision > created.revision);
    // The HTTP contract requests one row per page. Only the interactive
    // Browser probe needs enough rows to cross its default page size.
    let additional_memories = if probe.is_some() { 21 } else { 1 };
    for index in 1..=additional_memories {
        let name = format!("Additional memory {index}");
        let input = memory_input(&name, vec![memory_reference(&seed, "task", task_id)]);
        write_memory(&seed, MemorySeed::Create(input)).await;
    }
    if records_browser {
        publish_knowledge_with_mode(
            &seed,
            &seed.project,
            task_id,
            session_id,
            &updated.id.to_string(),
            false,
            KnowledgeFixtureMode::BrowserReferences,
        )
        .await;
    } else {
        publish_knowledge(
            &seed,
            &seed.project,
            task_id,
            session_id,
            &updated.id.to_string(),
            false,
        )
        .await;
    }

    let imported_publication = publish_knowledge(
        &seed,
        &other,
        task_id,
        session_id,
        &updated.id.to_string(),
        true,
    )
    .await;
    let import_source = register_knowledge(&seed, &imported_publication).await;
    let verifier = incoming_user(&seed, &broker);
    let user_id: Option<MemoryId> = if records_browser {
        None
    } else {
        seed.auth
            .issuer()
            .set_record_storage_resolver(Arc::new(LocalSeedStorage))
            .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let seed_token = broker.issuer.jwt(json!({
            "iss":broker.issuer.origin, "sub":"web-subject", "aud":USER_RESOURCE,
            "client_id":"native-client", "jti":"foreground-references-user-seed",
            "token_use":"access", "iat":now, "exp":now+600,
        }));
        let user = write_user_memory(
            &seed,
            &verifier,
            &seed_token,
            memory_input(
                "Own USER reference",
                vec![
                    memory_reference(&seed, "task", task_id),
                    memory_reference(&seed, "memory", &updated.id.to_string()),
                ],
            ),
        )
        .await;
        write_user_memory(
            &seed,
            &verifier,
            &seed_token,
            memory_input(
                "Additional own USER reference",
                vec![memory_reference(&seed, "task", task_id)],
            ),
        )
        .await;
        Some(user.id)
    };
    // The test reopens SDK publications; the live seed issuer/runtime is removed by prepare.
    let refs_task = task["specification"]["id"].clone();
    let refs_session = session["id"].clone();
    let memory_id = updated.id;
    let mirror_memory = write_memory_at(
        &seed,
        &mirror,
        MemorySeed::Create(memory_input(
            "Mirror memory",
            vec![memory_reference(&seed, "knowledge", "Projection absent")],
        )),
    )
    .await;
    publish_knowledge_with_projection(
        &seed,
        &unprojected,
        task_id,
        session_id,
        &mirror_memory.id.to_string(),
        false,
        false,
    )
    .await;
    let additional_projects = seed.browser_projects.clone();
    if records_browser {
        publish_knowledge_with_mode(
            &seed,
            &additional_projects[0],
            task_id,
            session_id,
            &memory_id.to_string(),
            false,
            KnowledgeFixtureMode::Empty,
        )
        .await;
    }
    let fixture = seed.prepare().await;
    let mut project_inputs = vec![&fixture.project, &other, &mirror, &unprojected];
    project_inputs.extend(additional_projects.iter());
    let owner = if records_browser {
        use std::os::unix::fs::PermissionsExt;
        let program = fixture.temp.path().join("codex");
        fs::write(&program, r#"#!/usr/bin/python3
import json,sys,time
query=sys.stdin.read()
print(json.dumps({"type":"turn.started"}),flush=True)
if "hold-browser-question" in query:
    time.sleep(30)
print(json.dumps({"type":"item.completed","item":{"id":"answer","type":"agent_message","text":"Observed browser fixture answer"}}),flush=True)
print(json.dumps({"type":"turn.completed","usage":{"input_tokens":8,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0,"cache_write_input_tokens":0}}),flush=True)
"#).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        fixture
            .start_browser_records(
                config(&broker.issuer),
                &project_inputs,
                vec![("PATH".into(), fixture.temp.path().as_os_str().to_owned())],
            )
            .await
            .unwrap()
    } else {
        fixture
            .start_projects(Some(config(&broker.issuer)), &project_inputs)
            .unwrap()
    };
    let origin = format!("http://{}", owner.address().unwrap());
    let cancellation = Cancellation::default();
    let stopping = cancellation.clone();
    let running = tokio::spawn(async move { owner.run(&stopping).await });
    let client = client();
    let cookie = login(&client, &origin, &broker.issuer).await;
    let contexts = read(
        &client,
        &origin,
        &cookie,
        "contexts",
        json!({"selection":{}}),
    )
    .await;
    assert_eq!(contexts.status(), StatusCode::OK);
    let contexts: Value = contexts.json().await.unwrap();
    assert_eq!(
        contexts["contexts"].as_array().unwrap().len(),
        if records_browser { 6 } else { 4 }
    );
    let additional_selections: Vec<_> = additional_projects
        .iter()
        .map(|project| {
            let id = ProjectIdentityStore::at(project)
                .load()
                .unwrap()
                .unwrap()
                .id();
            contexts["contexts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["project_id"] == json!(id))
                .unwrap()
                .clone()
        })
        .collect();
    let selection = contexts["current"].clone();
    assert_eq!(selection["project_id"], json!(project));
    let matching: Vec<_> = contexts["contexts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| {
            item["project_id"] == selection["project_id"]
                && item["project_version"] == selection["project_version"]
                && item["project_dir"] != selection["project_dir"]
        })
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "mirror discovery retains exact NativePath"
    );
    let mirror_selection = (*matching[0]).clone();
    let unprojected_selection = contexts["contexts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["project_id"] == json!(unprojected_id))
        .unwrap();
    let mirror_task = record_read(
        &client,
        &origin,
        &cookie,
        "/api/tasks/get",
        json!({"selection":mirror_selection,"input":{"id":refs_task}}),
    )
    .await;
    assert_eq!(mirror_task.status(), StatusCode::OK);
    assert!(
        mirror_task
            .text()
            .await
            .unwrap()
            .contains("References journey")
    );
    let mirror_read = record_read(
        &client,
        &origin,
        &cookie,
        "/api/memory/history",
        json!({"selection":mirror_selection,"input":{"id":mirror_memory.id.to_string()}}),
    )
    .await;
    assert_eq!(mirror_read.status(), StatusCode::OK);
    let mirror_read: Value = mirror_read.json().await.unwrap();
    assert!(mirror_read["native"].to_string().contains("Mirror memory"));
    assert!(
        !mirror_read["native"]
            .to_string()
            .contains("Revised foreground memory")
    );
    let missing_page = record_read(
        &client,
        &origin,
        &cookie,
        "/api/knowledge/page",
        json!({"selection":unprojected_selection,"input":{"slug":"Projection absent"}}),
    )
    .await;
    assert_eq!(missing_page.status(), StatusCode::OK);
    let missing_page: Value = missing_page.json().await.unwrap();
    assert_eq!(missing_page["focused_source"]["namespace"], Value::Null);
    assert_eq!(missing_page["focused_source"]["witness"], Value::Null);
    if !records_browser {
        let missing_refs = record_read(
        &client,
        &origin,
        &cookie,
        "/api/records/references",
        json!({
            "selection":unprojected_selection, "input":{
                "focus":{"kind":"knowledge","scope":{"project":unprojected_id},"id":"Projection absent"},
                "outgoing_source":{"kind":"project_knowledge"}, "page_size":20,
                "pages":[
                    {"source":{"kind":"project_knowledge"},"direction":"outgoing"},
                    {"source":{"kind":"project_knowledge"},"direction":"incoming"}
                ]
            }
        }),
    )
    .await;
        assert_eq!(missing_refs.status(), StatusCode::OK);
        let missing_refs: Value = missing_refs.json().await.unwrap();
        let missing_read = &missing_refs["sources"][0]["native"]["published"]["read"];
        assert_eq!(missing_read["focused_source"]["namespace"], Value::Null);
        assert_eq!(missing_read["focused_source"]["witness"], Value::Null);
        assert!(!missing_read["warnings"].as_array().unwrap().is_empty());
        assert_eq!(missing_refs["outgoing_complete"], false);
        assert_eq!(missing_refs["incoming_complete"], false);
        assert_eq!(
            missing_refs["sources"][0]["navigation"]["outgoing"],
            json!([])
        );
    }

    let memory = record_read(
        &client,
        &origin,
        &cookie,
        "/api/memory/get",
        json!({"selection":selection,"input":{"id":memory_id}}),
    )
    .await;
    assert_eq!(memory.status(), StatusCode::OK);
    let memory: Value = memory.json().await.unwrap();
    assert!(
        memory["native"]
            .to_string()
            .contains("Revised foreground memory")
    );
    assert!(
        memory["native"]
            .to_string()
            .contains("Created foreground memory")
    );
    let history = record_read(
        &client,
        &origin,
        &cookie,
        "/api/memory/history",
        json!({"selection":selection,"input":{"id":memory_id}}),
    )
    .await;
    assert_eq!(history.status(), StatusCode::OK);
    let history: Value = history.json().await.unwrap();
    assert!(
        history["native"]
            .to_string()
            .contains("Revised foreground memory")
    );
    let page = record_read(
        &client,
        &origin,
        &cookie,
        "/api/knowledge/page",
        json!({"selection":selection,"input":{"slug":"Foreground wiki"}}),
    )
    .await;
    assert_eq!(page.status(), StatusCode::OK);
    let page: Value = page.json().await.unwrap();
    assert_eq!(page["focused_source"]["slug"], "Foreground wiki");
    assert_eq!(
        page["focused_source"]["namespace"]["scope"],
        json!({"project":project})
    );
    let empty = record_read(
        &client,
        &origin,
        &cookie,
        "/api/knowledge/page",
        json!({"selection":selection,"input":{"slug":"No outgoing"}}),
    )
    .await;
    assert_eq!(empty.status(), StatusCode::OK);
    let empty: Value = empty.json().await.unwrap();
    assert!(empty["focused_source"]["witness"].is_array());
    let imported = record_read(
        &client,
        &origin,
        &cookie,
        "/api/knowledge/imported/page",
        json!({"input":{"source":import_source,"slug":"Imported namespace"}}),
    )
    .await;
    assert_eq!(imported.status(), StatusCode::OK);
    let imported: Value = imported.json().await.unwrap();
    assert!(imported.get("selection").is_none());
    assert_eq!(imported["source"], import_source);
    assert_eq!(
        imported["focused_source"]["namespace"]["scope"],
        json!({"project":other_id})
    );
    let own = record_read(
        &client,
        &origin,
        &cookie,
        if records_browser {
            "/api/user-memory/list"
        } else {
            "/api/user-memory/get"
        },
        if records_browser {
            json!({"input":{}})
        } else {
            json!({"input":{"id":user_id}})
        },
    )
    .await;
    assert_eq!(own.status(), StatusCode::OK);
    let own: Value = own.json().await.unwrap();
    let user_scope = own["scope_id"].as_str().unwrap();
    if records_browser {
        assert_eq!(own["native"]["state"], "no_corpus");
    } else {
        assert!(own["native"].to_string().contains("Own USER reference"));
    }
    if !records_browser {
        let focus = json!({"kind":"task","scope":{"project":project},"id":refs_task});
        let all_request = json!({
            "selection":selection,
            "input":{"focus":focus,"include_user":true,"contexts":[import_source["artifact"]],"page_size":1},
        });
        let all = record_read(
            &client,
            &origin,
            &cookie,
            "/api/records/references",
            all_request.clone(),
        )
        .await;
        assert_eq!(all.status(), StatusCode::OK);
        let all: Value = all.json().await.unwrap();
        let sources = all["sources"].as_array().unwrap();
        assert_eq!(
            sources
                .iter()
                .map(|s| s["source"]["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "project_task",
                "project_memory",
                "project_knowledge",
                "user_memory",
                "imported_knowledge"
            ]
        );
        assert_eq!(all["incoming_complete"], false);
        assert!(sources.iter().all(|s| s["native"] != json!("no_corpus")));
        let first = record_read(
            &client,
            &origin,
            &cookie,
            "/api/records/references",
            json!({"selection":selection,"input":{
                "focus":{"kind":"memory","scope":{"project":project},"id":memory_id},
                "outgoing_source":{"kind":"project_memory"}, "page_size":1,
                "pages":[{"source":{"kind":"project_task"},"direction":"incoming"}],
            }}),
        )
        .await;
        assert_eq!(first.status(), StatusCode::OK);
        let first: Value = first.json().await.unwrap();
        let incoming = &first["sources"][0]["native"]["published"]["incoming"];
        assert_eq!(incoming["total"], 2);
        assert_eq!(incoming["results"].as_array().unwrap().len(), 1);
        assert_eq!(incoming["complete"], true);
        assert_eq!(first["incoming_complete"], false);
        assert!(!incoming["next_cursor"].is_null());
        let first_edge = incoming["results"][0].clone();
        let next = record_read(
        &client, &origin, &cookie, "/api/records/references",
        json!({"selection":selection,"input":{
            "focus":{"kind":"memory","scope":{"project":project},"id":memory_id},
            "outgoing_source":{"kind":"project_memory"},"page_size":1,
            "pages":[{"source":{"kind":"project_task"},"direction":"incoming","cursor":incoming["next_cursor"]}],
        }}),
    ).await;
        assert_eq!(next.status(), StatusCode::OK);
        let next: Value = next.json().await.unwrap();
        let incoming = &next["sources"][0]["native"]["published"]["incoming"];
        assert_eq!(incoming["results"].as_array().unwrap().len(), 1);
        assert_eq!(incoming["complete"], true);
        assert!(incoming["next_cursor"].is_null());
        assert_eq!(next["incoming_complete"], true);
        assert!(
            first_edge != incoming["results"][0],
            "incoming cursor advances to a distinct native edge"
        );
        let both_edges = json!([first_edge, incoming["results"][0]]).to_string();
        assert!(
            both_edges.contains(refs_task.as_str().unwrap()),
            "Task backlink is observed"
        );
        assert!(
            both_edges.contains(refs_session.as_str().unwrap()),
            "Session backlink is observed"
        );
        broker.issuer.memory_allowed.store(false, Ordering::SeqCst);
        let denied = record_read(
            &client,
            &origin,
            &cookie,
            "/api/records/references",
            all_request,
        )
        .await;
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        let denied: Value = denied.json().await.unwrap();
        assert_eq!(denied["error"]["code"], "project_access_denied");
        assert!(denied.get("sources").is_none());
        broker.issuer.memory_allowed.store(true, Ordering::SeqCst);
    }
    let mut finished = true;
    if let Some(path) = &probe {
        fs::write(path, serde_json::to_vec(&json!({
            "origin":origin, "task_id":refs_task, "session_id":refs_session,
            "memory_id":memory_id, "user_memory_id":user_id, "selection":selection,
            "contexts":contexts, "additional_selections":additional_selections, "mirror_selection":mirror_selection, "mirror_memory_id":mirror_memory.id.to_string(),
            "missing_selection":unprojected_selection, "missing_slug":"Projection absent", "import_source":import_source, "import_slug":"Imported namespace",
            "wiki_slug":"Foreground wiki", "empty_slug":"No outgoing",
            "user_scope_id":user_scope, "task_read_allowed":true,
            "scenario":if records_browser {"db07"} else {"references"}, "http_preflight":if records_browser {"signed_oauth_user_no_corpus"} else {"all_corpora_signed_oauth"}, "control":"issuer_hold_entered_release", "fixture_contract":if records_browser {"first_memory_publication_v2"} else {"mirror_issuer_wait_registered_v1"}, "iab_qualified":false,
        })).unwrap()).unwrap();
        let completion = path.with_extension("done");
        let deadline = Instant::now() + Duration::from_secs(900);
        finished = false;
        while Instant::now() < deadline {
            if completion.exists() {
                finished = true;
                break;
            }
            let deny_task = path.with_extension("deny-task");
            if records_browser && deny_task.exists() {
                broker.issuer.allowed.store(false, Ordering::SeqCst);
                fs::remove_file(deny_task).unwrap();
                fs::write(path.with_extension("task-denied"), "task denied\n").unwrap();
            }
            let arm = path.with_extension("hold");
            if arm.exists() {
                broker.issuer.hold.store(true, Ordering::SeqCst);
                fs::remove_file(arm).unwrap();
                fs::write(path.with_extension("armed"), "armed\n").unwrap();
            }
            let release = path.with_extension("release");
            if release.exists() {
                broker.issuer.hold.store(false, Ordering::SeqCst);
                broker.issuer.release.notify_waiters();
                fs::remove_file(release).unwrap();
                fs::write(path.with_extension("released"), "released\n").unwrap();
            }
            tokio::select! {
                _ = broker.issuer.entered.notified() => {
                    fs::write(path.with_extension("entered"), "issuer entered\n").unwrap();
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
        }
    }
    broker.issuer.hold.store(false, Ordering::SeqCst);
    broker.issuer.release.notify_waiters();
    let shutdown_started = Instant::now();
    cancellation.cancel();
    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    while broker.issuer.active.load(Ordering::SeqCst) != 0 {
        assert!(
            shutdown_started.elapsed() < Duration::from_secs(30),
            "issuer handler drain deadline"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let shutdown_elapsed_ms = shutdown_started.elapsed().as_millis();
    fixture.assert_clean();
    assert!(finished, "browser probe expired; host was drained");
    assert_eq!(broker.issuer.userinfo.load(Ordering::SeqCst), 0);
    let reopened = fixture.start(None).unwrap();
    reopened.shutdown().await.unwrap();
    fixture.assert_clean();
    if let Some(path) = probe {
        fs::write(
            path.with_extension("result.json"),
            serde_json::to_vec(&json!({
                "finished":true, "http_preflight":if records_browser {"signed_oauth_user_no_corpus"} else {"all_corpora_signed_oauth"},
                "host_joined":true, "discovery_clean":true, "userinfo_calls":0,
                "shutdown_elapsed_ms":shutdown_elapsed_ms,"issuer_handlers_after_drain":0,
                "iab_qualified":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hosted_record_session_expiry_preflight() {
    verify_hosted_record_session_expiry(None).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "native browser qualification requires a browser observer and GRAPHIT_UI_BROWSER_PROBE_FILE"]
async fn hosted_record_session_expiry_and_browser_probe() {
    verify_hosted_record_session_expiry(Some(browser_probe_path())).await;
}

async fn verify_hosted_record_session_expiry(probe: Option<std::path::PathBuf>) {
    let broker = Broker::new().await;
    let fixture = prepared(&broker).await;
    let mut settings = config(&broker.issuer);
    settings["session_lifetime_ms"] = json!(30_000);
    settings["refresh_margin_ms"] = json!(1_000);
    let owner = fixture.start(Some(settings)).unwrap();
    let origin = format!("http://{}", owner.address().unwrap());
    let cancellation = Cancellation::default();
    let stopping = cancellation.clone();
    let running = tokio::spawn(async move { owner.run(&stopping).await });
    let client = client();
    let cookie = login(&client, &origin, &broker.issuer).await;
    let contexts = read(
        &client,
        &origin,
        &cookie,
        "contexts",
        json!({"selection":{}}),
    )
    .await;
    assert_eq!(contexts.status(), StatusCode::OK);
    let contexts: Value = contexts.json().await.unwrap();
    let selection = contexts["current"].clone();
    let task = record_read(
        &client,
        &origin,
        &cookie,
        "/api/tasks/get",
        json!({"selection":selection,"input":{"id":fixture.task}}),
    )
    .await;
    assert_eq!(
        task.status(),
        StatusCode::OK,
        "task before session expiry: {}",
        task.text().await.unwrap()
    );
    let mut finished = true;
    if let Some(path) = &probe {
        fs::write(path, serde_json::to_vec(&json!({
            "origin":origin,"task_id":fixture.task,"selection":selection,"task_read_allowed":true,
            "scenario":"session_expiration","session_lifetime_ms":30_000,
            "http_preflight":"actual_task_reader_before_session_expiration",
        })).unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(300);
        finished = false;
        while Instant::now() < deadline {
            if path.with_extension("done").exists() {
                finished = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    cancellation.cancel();
    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    fixture.assert_clean();
    assert!(finished, "session-expiry probe expired; host was drained");
    assert_eq!(broker.issuer.userinfo.load(Ordering::SeqCst), 0);
    fixture.start(None).unwrap().shutdown().await.unwrap();
    fixture.assert_clean();
    if let Some(path) = probe {
        fs::write(
            path.with_extension("result.json"),
            serde_json::to_vec(&json!({
                "finished":true,"host_joined":true,"discovery_clean":true,"userinfo_calls":0,
                "scenario":"session_expiration","browser_qualified":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
