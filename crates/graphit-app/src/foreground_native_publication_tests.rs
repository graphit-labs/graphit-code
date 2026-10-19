//! Publication against actual foreground SDK output, reusing the shared tiny CPU fixture.
use super::*;
use crate::native_publication::{
    PublicationMetadata, PublishInput, ast_components, metadata_limits, publish,
};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture, ScopeInput,
};
use graphit_core::artifact_store::CapturedPublicationScope;
use graphit_hub::{ArtifactType, native_root::NativeRole};
use object_store::{ObjectStoreExt, path::Path as ObjectPath};
use sha2::{Digest, Sha256};

fn metadata(kind: ArtifactType) -> PublicationMetadata {
    PublicationMetadata {
        provider: "captured-sdk".into(),
        artifact_type: kind,
        id: "sdk/publication".into(),
        version: "release/one".into(),
        name: "CapturedSDK".into(),
        description: None,
        tags: vec![],
        author: None,
        dependencies: None,
    }
}

async fn assert_uploaded_snapshots(
    store: &Arc<dyn object_store::ObjectStore>,
    producer: ProjectId,
    metadata: &PublicationMetadata,
    components: &[graphit_hub::native_root::NativeComponent],
    limits: graphit_hub::remote_metadata::MetadataLimits,
    request: &graphit_hub::policy::RequestContext<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    use graphit_hub::native_root::{NativeRoot, NativeRootInput};
    let root = NativeRoot::encode(
        &NativeRootInput {
            producer,
            artifact_type: metadata.artifact_type,
            id: &metadata.id,
            version: &metadata.version,
            components,
        },
        limits,
        request,
    )?;
    let digest: [u8; 32] = Sha256::digest(&root).into();
    let id = graphit_hub::ArtifactId::new(&metadata.id, limits.projection)?;
    let version = graphit_hub::ConcreteVersion::new(&metadata.version, limits.projection)?;
    let prefix = format!(
        "tenant/native/{}",
        graphit_hub::remote_storage::release_prefix(
            producer,
            metadata.artifact_type,
            &id,
            &version,
            &digest
        )?
    );
    let actual = store
        .get(&ObjectPath::parse(format!("{prefix}native.json"))?)
        .await?
        .bytes()
        .await?;
    let decoded = NativeRoot::decode(&actual, limits, request)?;
    assert_eq!(decoded.components(), components);
    let session = crate::remote_native_mount::admitted_session("captured-native", store);
    for component in decoded.components() {
        let bytes = store
            .get(&ObjectPath::parse(format!(
                "{prefix}{}/{}",
                component.path,
                component.snapshot.manifest_path()
            ))?)
            .await?
            .bytes()
            .await?;
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            *component.snapshot.manifest_sha256()
        );
        let graphit_hub::native_root::NativeSnapshot::Lance {
            dataset_version,
            manifest_path,
            ..
        } = &component.snapshot
        else {
            continue;
        };
        let path = ObjectPath::parse(format!("{prefix}{}", component.path))?;
        let mut uri = url::Url::parse("s3://captured-native/")?;
        uri.path_segments_mut()
            .unwrap()
            .clear()
            .extend(path.as_ref().split('/'));
        let location = if matches!(component.role, NativeRole::Wiki | NativeRole::EntitySearch) {
            let (pin, location) = graphit_data::lance_branch_store::BranchSnapshot::capture_native(
                uri.as_str(),
                Arc::clone(&session),
                *dataset_version,
            )
            .await?;
            match component.role {
                NativeRole::Wiki => {
                    graphit_data::knowledge_embeddings::native_knowledge_embedding_dimensions(
                        pin.schema(),
                    )?;
                }
                NativeRole::EntitySearch => graphit_ast::corpus::validate_schema(pin.schema())?,
                _ => unreachable!(),
            }
            location
        } else {
            let schema = match component.role {
                NativeRole::Source => graphit_ast::indexed_source::source_schema(),
                _ => unreachable!("only Graph uses Icebug"),
            };
            graphit_data::lance_module_store::LanceModuleStore::open_native_snapshot(
                uri.as_str(),
                schema,
                Arc::clone(&session),
                *dataset_version,
            )
            .await?
            .1
        };
        request.check()?;
        assert_eq!(
            location.version, *dataset_version,
            "{:?} SDK version",
            component.role
        );
        assert_eq!(
            location.path,
            ObjectPath::parse(format!("{path}/{manifest_path}"))?,
            "{:?} SDK manifest path",
            component.role
        );
    }
    Ok(())
}

/// Final current Native transport bytes for an existing Web fixture, without another service or environment mutation.
pub(crate) async fn received_archive_fixture(
    kind: ArtifactType,
    parent: &std::path::Path,
    policy: &crate::source_server::SourcePolicy,
    request: &graphit_hub::policy::RequestContext<'_>,
) -> io::Result<(tempfile::TempDir, std::path::PathBuf)> {
    use graphit_core::artifact_store::CapturedSourceReader;
    use graphit_hub::native_package::{self, PackageKind};
    let owner = tempfile::tempdir_in(parent)?;
    let destination = owner.path().join(match kind {
        ArtifactType::Ast => "received.ast",
        ArtifactType::Knowledge => "received.knowledge",
        _ => return Err(io::ErrorKind::InvalidInput.into()),
    });
    match kind {
        ArtifactType::Ast => {
            use graphit_data::graph_writer::{GraphNode, GraphValue, IcebugWriter};
            let graph = IcebugWriter::new(Arc::clone(request.snapshot()))?
                .push_file(
                    [GraphNode {
                        label: "File".into(),
                        key: "received.rs".into(),
                        properties: std::collections::BTreeMap::from([
                            ("name".into(), GraphValue::String("received.rs".into())),
                            ("path".into(), GraphValue::String("received.rs".into())),
                        ]),
                    }],
                    [],
                )?
                .finish()?;
            let reader = CapturedSourceReader::capture(
                &graph.directory(),
                graphit_fs::NodeKind::Directory,
                &policy.file,
                policy.read,
                Arc::clone(request.snapshot()),
            )?;
            native_package::export_ast_graph(&reader, &destination, request.snapshot())
                .map_err(io::Error::other)?;
        }
        ArtifactType::Knowledge => {
            let payload = owner.path().join("payload");
            std::fs::create_dir(&payload)?;
            let source_root = owner.path().join("source.lance");
            let schema = graphit_data::knowledge_index::physical_schema();
            let store = graphit_data::lance_module_store::LanceModuleStore::create(
                source_root.to_str().ok_or(io::ErrorKind::InvalidData)?,
                Arc::clone(&schema),
                vec![arrow_array::RecordBatch::new_empty(schema)],
            )
            .await
            .map_err(io::Error::other)?;
            let pin = store.snapshot();
            let clone = pin
                .spawn_native_clone_local(
                    tempfile::tempdir_in(owner.path())?,
                    Arc::clone(request.snapshot()),
                )?
                .await
                .map_err(io::Error::other)??;
            std::fs::rename(clone.directory(), payload.join("index.lance"))?;
            let reader = CapturedSourceReader::capture(
                &payload,
                graphit_fs::NodeKind::Directory,
                &policy.file,
                policy.read,
                Arc::clone(request.snapshot()),
            )?;
            native_package::export(
                &reader,
                PackageKind::Knowledge,
                &destination,
                request.snapshot(),
            )
            .map_err(io::Error::other)?;
        }
        _ => unreachable!(),
    }
    request.check()?;
    Ok((owner, destination))
}

// Round-trip finalized current transport bytes through the received-package consumer.
async fn assert_received_publication(
    reader: &graphit_core::artifact_store::CapturedSourceReader,
    metadata: &PublicationMetadata,
    producer: ProjectId,
    policy: &crate::source_server::SourcePolicy,
    request: &graphit_hub::policy::RequestContext<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    use graphit_hub::native_package::{self, PackageKind};
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("upload.package");
    match metadata.artifact_type {
        ArtifactType::Ast => {
            native_package::export_ast_graph(reader, &destination, request.snapshot())?;
        }
        ArtifactType::Knowledge => {
            native_package::export(
                reader,
                PackageKind::Knowledge,
                &destination,
                request.snapshot(),
            )?;
        }
        _ => unreachable!(),
    }
    let received = graphit_core::artifact_store::CapturedSourceReader::capture(
        &destination,
        graphit_fs::NodeKind::File,
        &policy.file,
        policy.read,
        Arc::clone(request.snapshot()),
    )?;
    let wrong_kind = match metadata.artifact_type {
        ArtifactType::Ast => ArtifactType::Knowledge,
        ArtifactType::Knowledge => ArtifactType::Ast,
        _ => unreachable!(),
    };
    assert!(
        crate::received_native::prepare(
            &received,
            directory.path(),
            wrong_kind,
            policy,
            request,
            &|| request.check()
        )
        .await
        .is_err(),
        "the declared artifact family must match the current package envelope"
    );
    let prepared = crate::received_native::prepare(
        &received,
        directory.path(),
        metadata.artifact_type,
        policy,
        request,
        &|| request.check(),
    )
    .await?;
    let stage_root = prepared
        .reader(request)?
        .source_path(request.snapshot())?
        .to_owned();
    let store: Arc<dyn object_store::ObjectStore> = Arc::new(object_store::memory::InMemory::new());
    let limits = metadata_limits(policy.content, policy.registry.limits().projection());
    let receipt = publish(
        PublishInput {
            store: store.as_ref(),
            storage_prefix: "tenant/native",
            producer,
            metadata,
            reader: prepared.reader(request)?,
            components: prepared.components(),
            limits,
        },
        request,
        || prepared.recheck(request, &|| request.check()),
    )
    .await?;
    assert_eq!(receipt.catalog, "published");
    assert_uploaded_snapshots(
        &store,
        producer,
        metadata,
        prepared.components(),
        limits,
        request,
    )
    .await?;
    if metadata.artifact_type == ArtifactType::Knowledge {
        let corrupt_path = directory.path().join("corrupt-corpus.knowledge");
        let mut input = zip::ZipArchive::new(std::fs::File::open(&destination)?)?;
        let mut output = zip::ZipWriter::new(std::fs::File::create(&corrupt_path)?);
        for index in 0..input.len() {
            let mut entry = input.by_index(index)?;
            let name = entry.name()?.into_owned();
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .unix_permissions(entry.unix_mode().unwrap_or(if entry.is_dir() {
                    0o700
                } else {
                    0o600
                }));
            if entry.is_dir() {
                output.add_directory(name.as_str(), options)?;
            } else {
                output.start_file(name.as_str(), options)?;
                if name.ends_with(".manifest") {
                    // Keep the SDK-produced envelope and invalidate protobuf
                    // content. Arbitrary truncation can panic in upstream;
                    // that removed bounds-patch guarantee is not this contract.
                    let mut bytes = Vec::new();
                    std::io::Read::read_to_end(&mut entry, &mut bytes)?;
                    let offset =
                        u64::from_le_bytes(bytes[bytes.len() - 16..bytes.len() - 8].try_into()?)
                            as usize;
                    let length = u32::from_le_bytes(bytes[offset..offset + 4].try_into()?) as usize;
                    bytes[offset + 4..offset + 4 + length].fill(0xff);
                    std::io::Write::write_all(&mut output, &bytes)?;
                } else {
                    std::io::copy(&mut entry, &mut output)?;
                }
            }
        }
        output.finish()?;
        let corrupted_reader = graphit_core::artifact_store::CapturedSourceReader::capture(
            &corrupt_path,
            graphit_fs::NodeKind::File,
            &policy.file,
            policy.read,
            Arc::clone(request.snapshot()),
        )?;
        assert!(
            crate::received_native::prepare(
                &corrupted_reader,
                directory.path(),
                ArtifactType::Knowledge,
                policy,
                request,
                &|| request.check()
            )
            .await
            .is_err(),
            "actual SDK pin rejects invalid protobuf inside its valid manifest envelope"
        );
    }
    drop(prepared);
    assert!(
        !stage_root.exists(),
        "received owned extraction is removed after SDK owners close"
    );
    assert!(
        destination.exists(),
        "the caller still owns the original uploaded file"
    );
    for entry in std::fs::read_dir(directory.path())? {
        assert!(
            !entry?
                .file_name()
                .to_string_lossy()
                .starts_with(".graphit-package-"),
            "successful and rejected preparations remove their owned stages"
        );
    }
    let cancelled = Cancellation::default();
    let cancelled_request = policy
        .native_install
        .hub
        .request_context(Instant::now(), &cancelled)?;
    let cancelled_reader = graphit_core::artifact_store::CapturedSourceReader::capture(
        &destination,
        graphit_fs::NodeKind::File,
        &policy.file,
        policy.read,
        Arc::clone(cancelled_request.snapshot()),
    )?;
    cancelled.cancel();
    let failure = crate::received_native::prepare(
        &cancelled_reader,
        directory.path(),
        metadata.artifact_type,
        policy,
        &cancelled_request,
        &|| cancelled_request.check(),
    )
    .await
    .err()
    .ok_or("cancelled native upload unexpectedly prepared")?;
    assert_eq!(failure.kind(), io::ErrorKind::Interrupted);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_publication_preserves_actual_ast_raw_nn_and_lexical_snapshots()
-> Result<(), Box<dyn std::error::Error>> {
    let (fixture, foreground, tasks, source, credential, selected) =
        foreground_memory_jobs_tests::browser_ast_search_fixture().await;
    let imported = fixture.publish_ast_search_package(&foreground, &credential, &selected);
    let cancel = Cancellation::default();
    let request = foreground.policy.request_context(Instant::now(), &cancel)?;
    let reader = graphit_core::artifact_store::CapturedSourceReader::capture(
        &imported.join("graph.icebug"),
        graphit_fs::NodeKind::Directory,
        &source.policy().file,
        source.policy().read,
        Arc::clone(request.snapshot()),
    )?;
    let graph = Arc::new(graphit_data::CapturedLocalGraph::from_reader(
        reader,
        Arc::clone(request.snapshot()),
        source.policy().native_install.graph,
    )?);
    let components = ast_components(&graph, &request).await?;
    for role in [
        NativeRole::Graph,
        NativeRole::Source,
        NativeRole::EntitySearch,
    ] {
        assert!(
            components.iter().any(|component| component.role == role),
            "actual published SDK component {role:?}"
        );
    }
    let reader = graph.source_reader(request.snapshot())?;
    let metadata = metadata(ArtifactType::Ast);
    let limits = metadata_limits(foreground.policy.content(), foreground.policy.projection());
    let store: Arc<dyn object_store::ObjectStore> = Arc::new(object_store::memory::InMemory::new());
    let receipt = publish(
        PublishInput {
            store: store.as_ref(),
            storage_prefix: "tenant/native",
            producer: selected.1,
            metadata: &metadata,
            reader,
            components: &components,
            limits,
        },
        &request,
        || {
            request.check()?;
            graph
                .revalidate(request.snapshot())
                .map_err(io::Error::other)
        },
    )
    .await?;
    assert_eq!(receipt.catalog, "published");
    assert_uploaded_snapshots(&store, selected.1, &metadata, &components, limits, &request).await?;
    assert_received_publication(reader, &metadata, selected.1, source.policy(), &request).await?;
    source.shutdown().await?;
    tasks.shutdown().await?;
    foreground.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_publication_preserves_actual_single_knowledge_dataset()
-> Result<(), Box<dyn std::error::Error>> {
    let (_fixture, foreground, tasks, source, credential, selected) =
        foreground_memory_jobs_tests::browser_knowledge_search_fixture().await;
    let cancel = Cancellation::default();
    let request = foreground.policy.request_context(Instant::now(), &cancel)?;
    let issuer = foreground.auth.issuer();
    let caller = issuer.authenticate_runtime(&credential, &request)?;
    let scope = CapturedPublicationScope::project(
        &selected.0,
        selected.1,
        foreground.scope,
        request.snapshot(),
    )?;
    let record = scope.binding()?;
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority.admit_authenticated_read(
        &caller,
        &request,
        ScopeInput {
            installation: foreground.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &scope,
                    record: &record,
                },
                version: &selected.2,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        },
        ProjectReadModule::Knowledge,
    )?;
    let task_policy = crate::foreground_tasks::task_policy(
        &foreground.policy,
        foreground.scope,
        foreground.native,
        NativeBinding::KiroV3,
    )?;
    let frozen = crate::knowledge_package_export::capture_native_knowledge(
        &request,
        &admission,
        &caller,
        crate::foreground_knowledge::limits(&task_policy)?,
    )
    .await?
    .ok_or("actual Knowledge corpus")?;
    assert!(frozen.generation.version > 0);
    assert_eq!(frozen.components.len(), 1);
    assert_eq!(frozen.components[0].role, NativeRole::Wiki);
    assert_eq!(frozen.components[0].path, "index.lance");
    let metadata = metadata(ArtifactType::Knowledge);
    let limits = metadata_limits(foreground.policy.content(), foreground.policy.projection());
    let store: Arc<dyn object_store::ObjectStore> = Arc::new(object_store::memory::InMemory::new());
    let receipt = publish(
        PublishInput {
            store: store.as_ref(),
            storage_prefix: "tenant/native",
            producer: selected.1,
            metadata: &metadata,
            reader: &frozen.reader,
            components: &frozen.components,
            limits,
        },
        &request,
        || {
            request.check()?;
            admission.recheck().map_err(io::Error::other)
        },
    )
    .await?;
    assert_eq!(receipt.catalog, "published");
    assert_uploaded_snapshots(
        &store,
        selected.1,
        &metadata,
        &frozen.components,
        limits,
        &request,
    )
    .await?;
    assert_received_publication(
        &frozen.reader,
        &metadata,
        selected.1,
        source.policy(),
        &request,
    )
    .await?;
    drop(frozen);
    drop(admission);
    drop(caller);
    source.shutdown().await?;
    tasks.shutdown().await?;
    foreground.shutdown().await?;
    Ok(())
}
