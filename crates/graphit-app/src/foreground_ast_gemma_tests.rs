//! Actual AST corpus composition; model/SDK qualification belongs to its producers.
use super::*;
use arrow_array::{FixedSizeListArray, Float32Array, StringArray, UInt64Array};
use futures_util::{FutureExt, TryStreamExt};
use graphit_auth::profile::{
    LocalCredentials, LocalDevice, LocalExecution, ProfileCredentials, ProfileData,
    ProfileExpectation, ProviderConfig, S3Location, S3Source, Secret,
};
use graphit_daemon::{
    model_service as model_api, supervisor::ProjectContext, watcher::ChangeBatch,
};
use model_api::{ModelSettings, RuntimeSettings, SelectionSettings, SlotSettings, TaskKind};
use secrecy::SecretString;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::{
    collections::BTreeSet,
    num::{NonZeroU32, NonZeroUsize},
    panic::AssertUnwindSafe,
    path::Path,
};

#[path = "../../graphit-daemon/tests/support/memory_gemma.rs"]
mod gemma_fixture;

const REVISION: &str = "bfcd298762cc34d0357ece5ebdd31791a3a374d8";
const INITIAL: &[(&str, &str, &str)] = &[
    (
        "keep.rs",
        "gemma_keep",
        "pub fn gemma_keep() -> i32 { 42 }\n",
    ),
    (
        "remove.rs",
        "gemma_remove",
        "pub fn gemma_remove() -> i32 { 7 }\n",
    ),
];
const CHANGED: &[(&str, &str, &str)] = &[
    (
        "keep.rs",
        "gemma_changed",
        "// moved range\npub fn gemma_changed() -> i32 { 43 }\n",
    ),
    (
        "added.rs",
        "gemma_added",
        "pub fn gemma_added() -> i32 { 11 }\n",
    ),
];

fn configure_local_auth(
    installation: &InstallationPaths,
    policy: &HubResourcePolicy,
) -> Result<(), Box<dyn std::error::Error>> {
    let cancellation = Cancellation::default();
    let request = policy.request_context(Instant::now(), &cancellation)?;
    let private = installation_bootstrap::prepare(
        installation,
        BootstrapLimits::new(65536, 256)?,
        request.snapshot(),
    )
    .into_installation()?;
    let identity = InstallationIdentityStore::new(installation)?
        .ensure(Duration::from_secs(5), &cancellation)?;
    private.revalidate(request.snapshot())?;
    let scope = CapturedPublicationScope::installation(
        installation,
        identity.unit_id().clone(),
        [installation.root().to_owned()],
        ScopeLimits::new(16, 65536, 256)?,
        request.snapshot(),
    )?;
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            scope,
            AuthDocumentLimits::new(1048576, 65536, 256, 65536)?,
            request.snapshot(),
        )?,
        ProfileLimits {
            bytes: 1048576,
            nodes: 65536,
            scalar_bytes: 65536,
            collection_entries: 1024,
            providers: 16,
            profiles: 16,
        },
        &request,
    )?;
    let before = store.load(&request)?;
    let outcome = store.mutate(before, &request, |doc, request| {
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
                username: "AST Gemma fixture".into(),
                organization: None,
                teams: vec![],
                credentials: ProfileCredentials::Local(LocalCredentials {
                    anonymous: false,
                    mcp_key: Some(Secret::new(SecretString::from("ast-gemma-private-fixture"))),
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
    })?;
    assert!(outcome.failure().is_none(), "{:?}", outcome.failure());
    assert!(
        outcome.cleanup_failure().is_none(),
        "{:?}",
        outcome.cleanup_failure()
    );
    Ok(())
}

struct ObservedPublication {
    entities: Vec<graphit_ast::entity_search::Entity>,
}

/// Assert the real package's schema, rows and same-Copy Raw association under ReadAst.
fn observe(
    owner: &ForegroundSource,
    config: &ConfigSnapshot,
    credential: &str,
    resources: &crate::foreground_memory_jobs::MemoryJobResources,
    corpus: &[(&str, &str, &str)],
    incompatible: bool,
) -> io::Result<Option<ObservedPublication>> {
    let source = owner.server.as_ref().unwrap();
    let selected = &owner.projects.as_ref().unwrap()[0];
    let cancellation = Cancellation::default();
    let original = owner
        .policy
        .request_context(Instant::now(), &cancellation)
        .map_err(io_error)?;
    let live = source.live_projects();
    let runtime = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        crate::project_graph::with_exported_graph(
            &source.graph_server(),
            crate::project_graph::ReadInput {
                auth: &owner.auth,
                policy: source.policy(),
                base: config,
                live: &live,
                selected: (&selected.0, selected.1, &selected.2),
                credential,
                local: false,
                request: &original,
            },
            |graph, _| {
                let search = runtime
                    .block_on(graphit_ast::entity_search::EntitySearch::open(
                        Arc::clone(graph),
                        Arc::clone(original.snapshot()),
                    ))?
                    .ok_or(io::ErrorKind::NotFound)?;
                let model = crate::ast_semantic::expected_native_nn_model(
                    &resources.local.as_ref().unwrap().expected,
                )
                .map_err(io_error)?;
                let opened = runtime.block_on(search.nn(&model));
                if incompatible {
                    let error = match opened {
                        Err(error) => error,
                        Ok(_) => panic!("old published NN must reject the new real identity"),
                    };
                    assert!(
                        matches!(
                            error
                                .get_ref()
                                .and_then(|inner| inner
                                    .downcast_ref::<graphit_data::lance_branch_store::BranchError>(
                                )),
                            Some(graphit_data::lance_branch_store::BranchError::SchemaMismatch)
                        ),
                        "retain typed schema mismatch: {error:?}"
                    );
                    search.revalidate()?;
                    return Ok(None);
                }
                let nn = opened?.ok_or(io::ErrorKind::NotFound)?;
                let raw = runtime.block_on(graph.open_lance_sidecar(
                    "source.lance", graphit_ast::indexed_source::source_schema(), original.snapshot(),
                ))?.ok_or(io::ErrorKind::NotFound)?;
                let functions = graphit_ast::icebug_query::execute(
                    graph,
                    original.snapshot(),
                    &runtime,
                    "MATCH (n:Function) RETURN n.name, n.path",
                    &Default::default(),
                    source.policy().graph_query,
                )?;
                assert_eq!(functions.rows.len(), corpus.len());
                for (path, name, _) in corpus {
                    assert!(
                        functions
                            .rows
                            .iter()
                            .any(|row| row[0]["value"] == *name && row[1]["value"] == *path)
                    );
                }
                let mut stream = runtime
                    .block_on(nn.scan().try_into_stream())
                    .map_err(io_error)?;
                let mut identities = Vec::new();
                while let Some(batch) = runtime.block_on(stream.try_next()).map_err(io_error)? {
                    // The persisted corpus also contains lexical rows without vectors.
                    // Validate the real model's eligible transient NativeNN view.
                    let batch = graphit_ast::corpus::vectors(&batch, &model, original.snapshot())?;
                    graphit_ast::native_nn::validate_native_nn(&batch, &model, original.snapshot())
                        .map_err(io_error)?;
                    let text = |name| {
                        batch
                            .column_by_name(name)
                            .unwrap()
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap()
                    };
                    let versions = batch
                        .column_by_name("raw_version")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<UInt64Array>()
                        .unwrap();
                    let vectors = batch
                        .column_by_name("vector")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<FixedSizeListArray>()
                        .unwrap();
                    for row in 0..batch.num_rows() {
                        assert!(versions.value(row) > 0);
                        let path = text("path").value(row);
                        let (_, _, body) = corpus
                            .iter()
                            .find(|entry| entry.0 == path)
                            .expect("NN has no deleted or unrelated file");
                        assert!(body.contains(text("projection").value(row)));
                        let vector = vectors.value(row);
                        let values = vector.as_any().downcast_ref::<Float32Array>().unwrap();
                        assert_eq!(values.len(), usize::try_from(model.dimensions).unwrap());
                        assert!(values.values().iter().all(|value| value.is_finite()));
                        let norm = values
                            .values()
                            .iter()
                            .map(|value| f64::from(*value).powi(2))
                            .sum::<f64>()
                            .sqrt();
                        assert!(
                            (norm - 1.0).abs() < 1e-4,
                            "actual normalized vector: {norm}"
                        );
                        identities.push((
                            text("uid").value(row).to_owned(),
                            text("data_key").value(row).to_owned(),
                            versions.value(row),
                        ));
                    }
                }
                let keys = identities
                    .iter()
                    .map(|(uid, key, _)| (uid.as_str(), key.as_str()))
                    .collect::<Vec<_>>();
                let entities = runtime.block_on(search.entities(&keys))?;
                assert_eq!(
                    entities.len(),
                    identities.len(),
                    "all NN rows resolve in same Copy"
                );
                assert!(entities.len() >= corpus.len());
                for entity in &entities {
                    assert!(!entity.uid.is_empty() && !entity.entity_key.is_empty());
                    let (_, _, body) = corpus.iter().find(|entry| entry.0 == entity.path).unwrap();
                    let mut expected_sha256 = String::with_capacity(64);
                    for byte in Sha256::digest(body.as_bytes()).iter() {
                        write!(&mut expected_sha256, "{byte:02x}")
                            .expect("writing SHA256 bytes to String");
                    }
                    assert_eq!(entity.source_sha256, expected_sha256);
                    assert_eq!(
                        entity.raw_version,
                        identities
                            .iter()
                            .find(|(uid, key, _)| uid == &entity.uid && key == &entity.data_key)
                            .expect("entity resolves the same NativeNN FILE pin")
                            .2
                    );
                    assert_eq!(entity.extent_status, "Exact");
                    let start = usize::try_from(entity.start_byte.unwrap()).unwrap();
                    let end = usize::try_from(entity.end_byte.unwrap()).unwrap();
                    assert!(!body[start..end].is_empty());
                }
                for (path, name, body) in corpus {
                    let entity = entities
                        .iter()
                        .find(|entity| entity.name == *name && entity.path == *path)
                        .unwrap();
                    let start = usize::try_from(entity.start_byte.unwrap()).unwrap();
                    let end = usize::try_from(entity.end_byte.unwrap()).unwrap();
                    assert!(body[start..end].contains(name));
                    let observed = runtime
                        .block_on(graphit_ast::indexed_source::read_indexed_source(
                            &raw,
                            path,
                            original.snapshot(),
                        ))
                        .map_err(io_error)?;
                    assert!(matches!(
                        observed.source(original.snapshot()).map_err(io_error)?,
                        graphit_ast::source_inspection::IndexedSource::Available(text) if text == *body
                    ));
                }
                search.revalidate()?;
                Ok(Some(ObservedPublication {
                    entities,

                }))
            },
        )
        .map_err(io_error)?
    })
}

async fn query(
    owner: &ForegroundSource,
    credential: &str,
    resources: &crate::foreground_memory_jobs::MemoryJobResources,
    corpus: &[(&str, &str, &str)],
    publication: &ObservedPublication,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = owner.server.as_ref().unwrap();
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-graphit-auth-origin", "runtime".parse()?);
    headers.insert(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {credential}").parse()?,
    );
    let result = source
        .graph_server()
        .call_search_mcp(
            source,
            serde_json::from_value(json!({
                "name":"graphit_ast_search",
                "arguments":{"query":corpus[0].1,"mode":"semantic","k":32}
            }))?,
            headers,
            CancellationToken::new(),
        )
        .await;
    assert_ne!(result.is_error, Some(true), "{result:?}");
    let value = result
        .structured_content
        .as_ref()
        .expect("structured actual CodeQuery result");
    assert_eq!(value["status"], "available");
    let expected =
        crate::ast_semantic::expected_native_nn_model(&resources.local.as_ref().unwrap().expected)?;
    assert_eq!(value["model"]["id"], "embeddinggemma2");
    assert_eq!(value["model"]["revision"], REVISION);
    assert_eq!(value["model"]["dimensions"], json!(expected.dimensions));
    assert_eq!(value["model"]["vector_id"], json!(expected.vector_id));
    for key in ["query_model_generation", "raw_generation", "nn_generation"] {
        assert!(value.get(key).is_none());
    }
    let hits = value["results"].as_array().unwrap();
    let mut seen = BTreeSet::new();
    for hit in hits {
        let uid = hit["uid"].as_str().unwrap();
        let data_key = hit["data_key"].as_str().unwrap();
        assert!(
            seen.insert((uid, data_key)),
            "duplicate Native result identity"
        );
        let entity = publication
            .entities
            .iter()
            .find(|entity| entity.uid == uid && entity.data_key == data_key)
            .expect("every Native hit belongs to the observed publication");
        assert_eq!(hit["name"], entity.name);
        assert_eq!(hit["path"], entity.path);
        assert_eq!(hit["kind"], entity.kind);
        assert_eq!(hit["language"], entity.language);
        assert_eq!(hit["grammar"], entity.grammar);
        assert!(hit["distance"].as_f64().is_some_and(f64::is_finite));
    }
    for (path, name, _) in corpus {
        let entity = publication
            .entities
            .iter()
            .find(|entity| entity.name == *name && entity.path == *path)
            .unwrap();
        assert!(
            seen.contains(&(entity.uid.as_str(), entity.data_key.as_str())),
            "Native query includes expected function {name} in {path}"
        );
    }
    assert!(
        hits.iter()
            .all(|hit| corpus.iter().any(|entry| hit["path"] == entry.0))
    );
    Ok(())
}

#[derive(Debug)]
struct ShutdownFailures(Vec<ForegroundFailure>);
impl std::fmt::Display for ShutdownFailures {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Foreground cleanup failures: {:?}", self.0)
    }
}
impl std::error::Error for ShutdownFailures {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0
            .first()
            .map(|failure| failure as &(dyn std::error::Error + 'static))
    }
}

/// Repair retained custody, preserving every failure even if the repair succeeds.
async fn shutdown_owned(owner: ForegroundSource) -> Result<(), Box<dyn std::error::Error>> {
    let mut failure = match owner.shutdown().await {
        Ok(()) => return Ok(()),
        Err(failure) => failure,
    };
    let mut failures = Vec::new();
    for _ in 0..2 {
        let Some(retained) = failure.owner.take() else {
            break;
        };
        failures.push(failure);
        failure = match retained.shutdown().await {
            Ok(()) => return Err(Box::new(ShutdownFailures(failures))),
            Err(failure) => failure,
        };
    }
    failures.push(failure);
    Err(Box::new(ShutdownFailures(failures)))
}

enum OriginalFailure {
    Error(Box<dyn std::error::Error>),
    Panic(Box<dyn std::any::Any + Send>),
}
impl std::fmt::Debug for OriginalFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error(error) => formatter.debug_tuple("Error").field(error).finish(),
            Self::Panic(payload) => {
                let text = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied());
                formatter
                    .debug_tuple("Panic")
                    .field(&text.unwrap_or("opaque payload retained"))
                    .finish()
            }
        }
    }
}
#[derive(Debug)]
struct PhaseCleanupFailure {
    original: OriginalFailure,
    cleanup: Box<dyn std::error::Error>,
}
impl std::fmt::Display for PhaseCleanupFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "AST Gemma phase failed: {:?}; cleanup: {}",
            self.original, self.cleanup
        )
    }
}
impl std::error::Error for PhaseCleanupFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.original {
            OriginalFailure::Error(error) => Some(error.as_ref()),
            OriginalFailure::Panic(_) => Some(self.cleanup.as_ref()),
        }
    }
}

async fn phase(
    installation: &InstallationPaths,
    project: &ProjectPaths,
    fixture: &gemma_fixture::GemmaFixture,
    dimensions: u64,
    threads: NonZeroU32,
    prior: Option<[u8; 32]>,
) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let cancellation = Cancellation::default();
    let settings = ModelSettings {
        slots: vec![SlotSettings {
            name: crate::models::DEFAULT_SLOT.into(),
            selection: SelectionSettings {
                id: "embeddinggemma2".into(),
                revision: REVISION.into(),
                task: TaskKind::Embedding,
            },
            bundle: fixture.directory.path().into(),
            manifest_relative: format!("{dimensions}.json").into(),
            runtime: RuntimeSettings::EmbeddingGemma2 {
                threads,
                queue_capacity: NonZeroUsize::new(8).unwrap(),
            },
        }],
    };
    let global = ConfigStore::global(installation)?;
    global.set(
        ConfigKey::new("daemon.models")?,
        serde_json::to_value(settings)?,
        Duration::from_secs(5),
        &cancellation,
    )?;
    let config = ConfigSnapshot::new(installation.brand(), [global.load()?])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let startup = policy.request_context(Instant::now(), &cancellation)?;
    let provider = active_provider(installation, &config, &startup)?.unwrap();
    let inputs = [ProjectBootstrapInput {
        project,
        explicit_version: "main",
        pin: None,
        detached_base: "",
    }];
    let mut owner = match ForegroundSource::start_with_inventory(
        installation,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        &startup,
        Some(&provider),
    ) {
        Ok(owner) => owner,
        Err(mut failure) => {
            if let Some(retained) = failure.owner.take()
                && let Err(cleanup) = shutdown_owned(*retained).await
            {
                return Err(Box::new(PhaseCleanupFailure {
                    original: OriginalFailure::Error(Box::new(failure)),
                    cleanup,
                }));
            }
            return Err(Box::new(failure));
        }
    };
    let result = AssertUnwindSafe(async {
        let server = owner.server.as_ref().unwrap();
        let expected = crate::foreground_memory_jobs::expected_model(
            installation,
            Some(&provider),
            startup.snapshot(),
            &server.project_modules(&startup)?,
        )?
        .unwrap();
        let qualified = fixture.config(
            &[(crate::models::DEFAULT_SLOT, dimensions)],
            startup.snapshot(),
            threads,
        )?;
        let reference = model_api::ExpectedModel::new(
            &qualified,
            crate::models::DEFAULT_SLOT,
            provider.name(),
        )?;
        assert_eq!(expected.config_id(), reference.config_id());
        let identity = expected.vector_id();
        if let Some(prior) = prior {
            assert_ne!(identity, prior);
        }
        owner.memory_jobs = Some(ForegroundMemoryJobs::new(
            Arc::clone(&owner.auth),
            (owner.policy.clone()).clone(),
            Some(expected),
            true,
            None,
            crate::foreground_memory_jobs::test_broker_cache(&(owner.policy.clone())),
        )?);
        let executable = std::env::var_os("GRAPHIT_TEST_MODEL_SERVER")
            .expect("actual compiled graphit-model-server fixture");
        let path = Path::new(&executable);
        assert!(path.is_absolute() && path.is_file());
        let resources = owner
            .memory_jobs
            .as_mut()
            .unwrap()
            .start(
                installation,
                vec![("PATH".into(), path.parent().unwrap().as_os_str().to_owned())],
                owner.scope,
                owner.native,
                NativeBinding::KiroV3,
                &startup,
            )
            .await?
            .expect("configured embedding resources");
        owner
            .server
            .as_mut()
            .unwrap()
            .attach_semantic_model(Some(Arc::clone(&resources)))?;
        owner.memory_job_resources = Arc::downgrade(&resources);
        let credential = {
            let key = owner.auth.runtime().check(startup.snapshot())?;
            std::str::from_utf8(key.secret(startup.snapshot())?)?.to_owned()
        };
        let selected = owner.projects.as_ref().unwrap()[0].clone();
        let context = ProjectContext {
            id: selected.1,
            version: selected.2.clone(),
        };
        let ast = Arc::new(ForegroundAstSources::new(
            Arc::clone(&owner.auth),
            ForegroundAstSourceConfig {
                live: owner.server.as_ref().unwrap().graph_server().live_ast(),
                installation: installation.clone(),
                base: config.clone(),
                policy: owner.policy.clone(),
                scope: owner.scope,
                native: owner.native,
                binding: NativeBinding::KiroV3,
                environment: Vec::new(),
            },
            startup.snapshot(),
        )?);
        owner.ast_jobs = Some(Arc::clone(&ast));
        if prior.is_some() {
            assert!(observe(&owner, &config, &credential, &resources, CHANGED, true)?.is_none());
        }
        let corpus = if prior.is_some() { CHANGED } else { INITIAL };
        let indexed = ast
            .index(
                &selected.0,
                &context,
                ChangeBatch {
                    rescan: true,
                    ..Default::default()
                },
                &cancellation,
            )
            .await?;
        if prior.is_none() {
            assert!(indexed.is_some(), "initial Raw publication");
        }
        assert!(
            ast.index_semantic(
                selected.0.clone(),
                context.clone(),
                Some(Arc::clone(&resources)),
                prior.is_some().then(BTreeSet::new),
                std::collections::BTreeSet::new(),
                prior.is_some(),
                cancellation.clone(),
                cancellation.clone()
            )
            .await?
            .version
            .is_some()
        );
        let publication = observe(&owner, &config, &credential, &resources, corpus, false)?
            .expect("published NN observation");
        query(&owner, &credential, &resources, corpus, &publication).await?;
        if prior.is_none() {
            for (path, _, body) in CHANGED {
                std::fs::write(project.root().join(path), body)?;
            }
            std::fs::remove_file(project.root().join("remove.rs"))?;
            assert!(
                ast.index(
                    &selected.0,
                    &context,
                    ChangeBatch {
                        rescan: true,
                        ..Default::default()
                    },
                    &cancellation
                )
                .await?
                .is_some()
            );
            assert!(
                ast.index_semantic(
                    selected.0.clone(),
                    context,
                    Some(Arc::clone(&resources)),
                    None,
                    std::collections::BTreeSet::new(),
                    false,
                    cancellation.clone(),
                    cancellation.clone()
                )
                .await?
                .version
                .is_some()
            );
            let publication = observe(&owner, &config, &credential, &resources, CHANGED, false)?
                .expect("published NN observation");
            query(&owner, &credential, &resources, CHANGED, &publication).await?;
            let original = owner
                .policy
                .request_context(Instant::now(), &cancellation)?;
            let live = owner.server.as_ref().unwrap().live_projects();
            crate::project_graph::with_read(
                crate::project_graph::ReadInput {
                    auth: &owner.auth,
                    policy: owner.server.as_ref().unwrap().policy(),
                    base: &config,
                    live: &live,
                    selected: (&selected.0, selected.1, &selected.2),
                    credential: &credential,
                    local: false,
                    request: &original,
                },
                |capture| {
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            let graphit_ast::source_index_locator::SourceIndexPresence::Available(
                                raw,
                            ) = graphit_ast::source_index_locator::open_local_source_index(
                                owner.auth.installation(),
                                capture.project(),
                                original.snapshot(),
                                owner.server.as_ref().unwrap().policy().index,
                            )
                            .await
                            .map_err(io_error)?
                            else {
                                return Err(io::Error::from(io::ErrorKind::NotFound));
                            };
                            let removed = raw
                                .read("remove.rs", original.snapshot())
                                .await
                                .map_err(io_error)?;
                            assert!(matches!(
                                removed.source(original.snapshot()).map_err(io_error)?,
                                graphit_ast::source_inspection::IndexedSource::Missing
                            ));
                            raw.revalidate(original.snapshot()).map_err(io_error)
                        })
                    })
                },
            )??;
        }
        Ok::<_, Box<dyn std::error::Error>>(identity)
    })
    .catch_unwind()
    .await;
    let cleanup = shutdown_owned(owner).await;
    match (result, cleanup) {
        (Ok(result), Ok(())) => result,
        (Err(panic), Ok(())) => std::panic::resume_unwind(panic),
        (Ok(Ok(_)), Err(cleanup)) => Err(cleanup),
        (Ok(Err(original)), Err(cleanup)) => Err(Box::new(PhaseCleanupFailure {
            original: OriginalFailure::Error(original),
            cleanup,
        })),
        (Err(panic), Err(cleanup)) => Err(Box::new(PhaseCleanupFailure {
            original: OriginalFailure::Panic(panic),
            cleanup,
        })),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a model-server executable and the pinned EmbeddingGemma GGUF for native inference"]
async fn productive_gemma_ast_incremental_query_and_model_change()
-> Result<(), Box<dyn std::error::Error>> {
    let asset = std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL")
        .expect("explicit verified BF16 GGUF asset locator; absence is not a skip");
    assert!(Path::new(&asset).is_absolute() && Path::new(&asset).is_file());
    let fixture = gemma_fixture::GemmaFixture::new(Path::new(&asset));
    let (_storage, installation, project, _, _) = ast_fixture();
    let cancellation = Cancellation::default();
    let global = ConfigStore::global(&installation)?;
    global.set(
        ConfigKey::new("modules.embedding")?,
        json!(true),
        Duration::from_secs(5),
        &cancellation,
    )?;
    let config = ConfigSnapshot::new(installation.brand(), [global.load()?])?;
    configure_local_auth(&installation, &HubResourcePolicy::resolve(&config)?)?;
    for (path, _, body) in INITIAL {
        std::fs::write(project.root().join(path), body)?;
    }
    let available = std::thread::available_parallelism()?.get().min(4);
    let defaults = NonZeroU32::new(u32::try_from(available)?).unwrap();
    let changed = NonZeroU32::new(if defaults.get() == 1 { 2 } else { 1 }).unwrap();
    let first = phase(&installation, &project, &fixture, 768, defaults, None).await?;
    // Changing scheduling alone preserves vector identity; this recipe change does not.
    let manifest_path = fixture.directory.path().join("768.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
    manifest["recipe"]["max_tokens"] = json!(4096);
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest)?)?;
    let second = phase(&installation, &project, &fixture, 768, changed, Some(first)).await?;
    let third = phase(
        &installation,
        &project,
        &fixture,
        128,
        changed,
        Some(second),
    )
    .await?;
    assert_ne!(third, first);
    Ok(())
}
