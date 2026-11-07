//! Immutable native files followed by a conditional catalog publication.
use futures_util::StreamExt;
use graphit_core::{
    artifact_store::{CapturedSourceFile, CapturedSourceReader},
    project_identity::ProjectId,
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion,
    native_root::{NativeComponent, NativeRoot, NativeRootInput},
    policy::RequestContext,
    remote_metadata::{
        AuthorInput, DependencyInput, ManifestInput, MetadataLimits, RemoteArtifactManifest,
        VersionInput,
    },
    remote_storage::{registry_key, release_prefix},
};
use object_store::{
    CopyMode, CopyOptions, ObjectStore, ObjectStoreExt, PutMode, UpdateVersion,
    path::Path as ObjectPath,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    io::{self, Read},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationDependency {
    pub artifact_type: ArtifactType,
    pub id: String,
    pub selector: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationMetadata {
    pub provider: String,
    pub artifact_type: ArtifactType,
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub author: Option<String>,
    pub dependencies: Option<Vec<PublicationDependency>>,
}
impl PublicationMetadata {
    pub(crate) fn validate(
        &self,
        limits: MetadataLimits,
    ) -> io::Result<(ArtifactId, ConcreteVersion)> {
        if !matches!(
            self.artifact_type,
            ArtifactType::Ast | ArtifactType::Knowledge
        ) || self.provider.is_empty()
            || self.provider.trim() != self.provider
            || self.provider.chars().any(char::is_control)
            || self.version.eq_ignore_ascii_case("latest")
            || self.name.is_empty()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "explicit native publication identity and provider required",
            ));
        }
        if self.name.contains('/')
            || self.name.len() > limits.scalar_bytes
            || self.provider.len() > limits.scalar_bytes
            || self
                .description
                .as_ref()
                .is_some_and(|s| s.len() > limits.scalar_bytes)
            || self.tags.len() > limits.collection_entries
            || self.tags.iter().any(|s| s.len() > limits.scalar_bytes)
            || self.author.as_ref().is_some_and(|s| {
                s.is_empty()
                    || s.trim() != s
                    || s.len() > limits.scalar_bytes
                    || s.chars().any(char::is_control)
            })
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        ArtifactId::new(&self.name, limits.projection)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        Ok((
            ArtifactId::new(&self.id, limits.projection)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
            ConcreteVersion::new(&self.version, limits.projection)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
        ))
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct PublicationReceipt {
    pub producer: ProjectId,
    pub artifact_type: ArtifactType,
    pub id: String,
    pub version: String,
    pub root_sha256: String,
    pub catalog: &'static str,
}
#[derive(Debug)]
pub(crate) struct PublicationFailure {
    pub receipt: PublicationReceipt,
    pub stage: &'static str,
    pub cause: io::Error,
}
impl std::fmt::Display for PublicationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "native publication failed at {} (catalog {})",
            self.stage, self.receipt.catalog
        )
    }
}
impl std::error::Error for PublicationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

pub(crate) fn key(prefix: &str, relative: &str) -> io::Result<ObjectPath> {
    let value = format!("{prefix}/{relative}");
    if value.len() > 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "S3 object key exceeds 1024 bytes",
        ));
    }
    ObjectPath::parse(value).map_err(io::Error::other)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) async fn guarded<T>(
    request: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
    operation: impl Future<Output = object_store::Result<T>>,
) -> io::Result<T> {
    check()?;
    let deadline = request.snapshot().required_deadline()?;
    let value = tokio::select! {
        biased;
        ()=request.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
        ()=tokio::time::sleep_until(deadline.into())=>Err(io::ErrorKind::TimedOut.into()),
        value=operation=>value.map_err(io::Error::other),
    }?;
    check()?;
    Ok(value)
}
fn storage_error(cause: &io::Error, predicate: impl FnOnce(&object_store::Error) -> bool) -> bool {
    cause
        .get_ref()
        .and_then(|e| e.downcast_ref::<object_store::Error>())
        .is_some_and(predicate)
}
async fn equal_file(
    store: &dyn ObjectStore,
    target: &ObjectPath,
    file: &CapturedSourceFile,
    request: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<()> {
    let got = guarded(request, check, store.get(target)).await?;
    if got.meta.size != file.len() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "immutable native object differs",
        ));
    }
    let mut chunks = got.into_stream();
    let mut source = file.stream(0)?;
    let mut local = [0u8; 64 * 1024];
    while let Some(chunk) = tokio::select! {
        ()=request.cancellation().cancelled()=>return Err(io::ErrorKind::Interrupted.into()),
        ()=tokio::time::sleep_until(request.snapshot().required_deadline()?.into())=>return Err(io::ErrorKind::TimedOut.into()),
        result=chunks.next()=>result,
    } {
        let chunk = chunk.map_err(io::Error::other)?;
        for remote in chunk.chunks(local.len()) {
            check()?;
            source.read_exact(&mut local[..remote.len()])?;
            if remote != &local[..remote.len()] {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "immutable native object differs",
                ));
            }
        }
    }
    if source.read(&mut local[..1])? != 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    check()
}

/// At most one 5 MiB multipart part is retained, regardless of file size.
pub(crate) async fn upload_file(
    store: &dyn ObjectStore,
    prefix: &str,
    target: &ObjectPath,
    file: &CapturedSourceFile,
    reuse_existing: bool,
    request: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<()> {
    check()?;
    if file.is_empty() {
        return match guarded(
            request,
            check,
            store.put_opts(target, Vec::<u8>::new().into(), PutMode::Create.into()),
        )
        .await
        {
            Ok(_) => Ok(()),
            Err(cause)
                if reuse_existing
                    && storage_error(&cause, |e| {
                        matches!(e, object_store::Error::AlreadyExists { .. })
                    }) =>
            {
                equal_file(store, target, file, request, check).await
            }
            Err(cause) => Err(cause),
        };
    }
    let nonce = tempfile::Builder::new()
        .prefix("native-upload-")
        .tempfile()?;
    let token = nonce
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(io::ErrorKind::InvalidData)?;
    let staging = key(prefix, token)?;
    let mut upload = guarded(request, check, store.put_multipart(&staging)).await?;
    let mut completed = false;
    let operation = async {
        let mut stream = file.stream(0)?;
        let mut remaining = file.len();
        while remaining != 0 {
            check()?;
            let mut part = vec![
                0u8;
                usize::try_from(remaining.min(5 * 1024 * 1024))
                    .map_err(io::Error::other)?
            ];
            let mut n = 0;
            while n < part.len() {
                check()?;
                let count = stream.read(&mut part[n..])?;
                if count == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                n += count;
            }
            remaining -= u64::try_from(n).map_err(io::Error::other)?;
            guarded(request, check, upload.put_part(part.into())).await?;
        }
        check()?;
        if stream.read(&mut [0u8; 1])? != 0 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        guarded(request, check, upload.complete()).await?;
        completed = true;
        match guarded(
            request,
            check,
            store.copy_opts(
                &staging,
                target,
                CopyOptions::new().with_mode(CopyMode::Create),
            ),
        )
        .await
        {
            Ok(()) => Ok(()),
            Err(cause)
                if reuse_existing
                    && storage_error(&cause, |e| {
                        matches!(e, object_store::Error::AlreadyExists { .. })
                    }) =>
            {
                equal_file(store, target, file, request, check).await
            }
            Err(cause) => Err(cause),
        }
    }
    .await;
    let abort = if completed {
        None
    } else {
        guarded(request, check, upload.abort()).await.err()
    };
    // Complete may have taken effect even when its response was lost. Only our own key is removed.
    let deletion = guarded(request, check, store.delete(&staging)).await.err();
    if abort.is_some() || deletion.is_some() {
        let cause = operation.err().unwrap_or_else(|| {
            io::Error::other("immutable upload completed before temporary cleanup failed")
        });
        return Err(io::Error::new(
            cause.kind(),
            UploadCleanupFailure {
                cause,
                abort,
                deletion,
                temporary: staging.to_string(),
            },
        ));
    }
    operation
}
#[derive(Debug)]
struct UploadCleanupFailure {
    cause: io::Error,
    abort: Option<io::Error>,
    deletion: Option<io::Error>,
    temporary: String,
}
impl std::fmt::Display for UploadCleanupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "owned upload cleanup failed (abort: {}, deletion: {})",
            self.abort.is_some(),
            self.deletion.is_some()
        )
    }
}
impl std::error::Error for UploadCleanupFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

pub(crate) struct CatalogIdentity<'a> {
    pub(crate) storage_prefix: &'a str,
    pub(crate) producer: ProjectId,
    pub(crate) artifact_type: ArtifactType,
    pub(crate) id: &'a ArtifactId,
    pub(crate) limits: MetadataLimits,
}
pub(crate) struct Catalog {
    pub(crate) current: Option<RemoteArtifactManifest>,
    key: ObjectPath,
    mode: PutMode,
}
impl Catalog {
    pub(crate) fn conditional_delete_target(&self) -> io::Result<(&ObjectPath, &str)> {
        match &self.mode {
            PutMode::Update(marker) => marker
                .e_tag
                .as_deref()
                .filter(|etag| !etag.is_empty() && *etag != "*")
                .map(|etag| (&self.key, etag))
                .ok_or_else(|| io::ErrorKind::Unsupported.into()),
            _ => Err(io::ErrorKind::NotFound.into()),
        }
    }
}

/// Only the current authorized producer/type/full-ID manifest can authorize reuse.
pub(crate) async fn read_catalog(
    store: &dyn ObjectStore,
    identity: CatalogIdentity<'_>,
    request: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<Catalog> {
    let CatalogIdentity {
        storage_prefix,
        producer,
        artifact_type,
        id,
        limits,
    } = identity;
    let catalog_key = key(storage_prefix, &registry_key(producer, artifact_type, id)?)?;
    let (current, mode) = match guarded(request, check, store.get(&catalog_key)).await {
        Ok(got) => {
            if got.meta.size > limits.bytes as u64 {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            let marker = UpdateVersion {
                e_tag: got.meta.e_tag.clone(),
                version: got.meta.version.clone(),
            };
            if marker.e_tag.is_none() && marker.version.is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "catalog has no conditional update marker",
                ));
            }
            let bytes = guarded(request, check, got.bytes()).await?;
            let parsed = RemoteArtifactManifest::decode(&bytes, limits, request)
                .map_err(io::Error::other)?;
            if parsed.producer() != producer
                || parsed.artifact_type() != artifact_type
                || parsed.id() != id
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
            (Some(parsed), PutMode::Update(marker))
        }
        Err(cause)
            if storage_error(&cause, |e| {
                matches!(e, object_store::Error::NotFound { .. })
            }) =>
        {
            (None, PutMode::Create)
        }
        Err(cause) => return Err(cause),
    };
    Ok(Catalog {
        current,
        key: catalog_key,
        mode,
    })
}

/// Preserve rejected/uncertain/published effects independently of final delivery.
pub(crate) async fn write_catalog(
    store: &dyn ObjectStore,
    catalog: Catalog,
    encoded: Vec<u8>,
    effect: &mut &'static str,
    request: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<()> {
    check()?;
    *effect = "uncertain";
    let written = tokio::select! {
        biased;
        () = request.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(request.snapshot().required_deadline()?.into()) => Err(io::ErrorKind::TimedOut.into()),
        result = store.put_opts(&catalog.key, encoded.into(), catalog.mode.into()) => result.map_err(io::Error::other),
    };
    if let Err(cause) = written {
        if storage_error(&cause, |e| {
            matches!(
                e,
                object_store::Error::AlreadyExists { .. }
                    | object_store::Error::Precondition { .. }
                    | object_store::Error::NotSupported { .. }
                    | object_store::Error::PermissionDenied { .. }
            )
        }) {
            *effect = "rejected";
        }
        return Err(cause);
    }
    *effect = "published";
    Ok(())
}

fn metadata_error(error: graphit_hub::remote_metadata::MetadataError) -> io::Error {
    use graphit_hub::remote_metadata::MetadataError;
    let kind = match error {
        MetadataError::InvalidDocument => io::ErrorKind::InvalidInput,
        MetadataError::Limit => io::ErrorKind::FileTooLarge,
        MetadataError::Cancelled => io::ErrorKind::Interrupted,
        MetadataError::Deadline => io::ErrorKind::TimedOut,
        MetadataError::InvalidPolicy => io::ErrorKind::Other,
    };
    io::Error::new(kind, error)
}
fn encoded_catalog(
    producer: ProjectId,
    metadata: &PublicationMetadata,
    digest: &[u8; 32],
    current: Option<&RemoteArtifactManifest>,
    limits: MetadataLimits,
    request: &RequestContext<'_>,
) -> io::Result<Vec<u8>> {
    let (id, version) = metadata.validate(limits)?;
    let digest_hex = hex(digest);
    let mut versions: Vec<VersionInput<'_>> = Vec::new();
    let old_hashes: Vec<String> = current
        .as_ref()
        .map(|c| c.versions().iter().map(|v| hex(v.hash())).collect())
        .unwrap_or_default();
    if let Some(current) = &current {
        for (old, hash) in current.versions().iter().zip(&old_hashes) {
            if old.version() == &version {
                if old.hash() != digest {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "artifact version already names another native root",
                    ));
                }
            } else {
                versions.push(VersionInput {
                    version: old.version().as_str(),
                    hash,
                });
            }
        }
    }
    versions.push(VersionInput {
        version: version.as_str(),
        hash: &digest_hex,
    });
    let dependencies: Vec<_> = match &metadata.dependencies {
        Some(dependencies) => dependencies
            .iter()
            .map(|dependency| DependencyInput {
                artifact_type: dependency.artifact_type,
                id: &dependency.id,
                selector: &dependency.selector,
            })
            .collect(),
        None => current
            .map(|current| {
                current
                    .dependencies()
                    .iter()
                    .map(|dependency| DependencyInput {
                        artifact_type: dependency.artifact_type(),
                        id: dependency.id().as_str(),
                        selector: dependency.selector(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    let tags: Vec<_> = metadata.tags.iter().map(String::as_str).collect();
    RemoteArtifactManifest::encode(
        &ManifestInput {
            producer,
            artifact_type: metadata.artifact_type,
            id: id.as_str(),
            name: &metadata.name,
            description: metadata.description.as_deref(),
            tags: &tags,
            author: metadata
                .author
                .as_deref()
                .map(|username| AuthorInput { username }),
            latest: Some(version.as_str()),
            versions: &versions,
            dependencies: &dependencies,
        },
        limits,
        request,
    )
    .map_err(metadata_error)
}

/// Validate the real descriptor and all declared metadata before acquiring storage or writing project metadata.
pub(crate) fn validate_input(
    producer: ProjectId,
    metadata: &PublicationMetadata,
    components: &[NativeComponent],
    limits: MetadataLimits,
    request: &RequestContext<'_>,
) -> io::Result<()> {
    metadata.validate(limits)?;
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
    )
    .map_err(metadata_error)?;
    let digest: [u8; 32] = Sha256::digest(&root).into();
    encoded_catalog(producer, metadata, &digest, None, limits, request)?;
    Ok(())
}

/// Caller retains the real PROJECT admission, lease, STS expiry and source owners.
pub(crate) struct PublishInput<'a> {
    pub store: &'a dyn ObjectStore,
    pub storage_prefix: &'a str,
    pub producer: ProjectId,
    pub metadata: &'a PublicationMetadata,
    pub reader: &'a CapturedSourceReader,
    pub components: &'a [NativeComponent],
    pub limits: MetadataLimits,
}

pub(crate) async fn publish(
    input: PublishInput<'_>,
    request: &RequestContext<'_>,
    check: impl Fn() -> io::Result<()>,
) -> io::Result<PublicationReceipt> {
    let PublishInput {
        store,
        storage_prefix,
        producer,
        metadata,
        reader,
        components,
        limits,
    } = input;
    check()?;
    reader.check_request(request.snapshot())?;
    let (id, version) = metadata.validate(limits)?;
    let root = NativeRoot::encode(
        &NativeRootInput {
            producer,
            artifact_type: metadata.artifact_type,
            id: id.as_str(),
            version: version.as_str(),
            components,
        },
        limits,
        request,
    )
    .map_err(metadata_error)?;
    let digest: [u8; 32] = Sha256::digest(&root).into();
    let release = release_prefix(producer, metadata.artifact_type, &id, &version, &digest)?;
    let prefix = format!("{storage_prefix}/{}", release.trim_end_matches('/'));
    let mut receipt = PublicationReceipt {
        producer,
        artifact_type: metadata.artifact_type,
        id: metadata.id.clone(),
        version: metadata.version.clone(),
        root_sha256: hex(&digest),
        catalog: "not_attempted",
    };
    let mut stage = "catalog_read";
    let operation = async {
        let catalog = read_catalog(
            store,
            CatalogIdentity {
                storage_prefix,
                producer,
                artifact_type: metadata.artifact_type,
                id: &id,
                limits,
            },
            request,
            &check,
        )
        .await?;
        let encoded = encoded_catalog(
            producer,
            metadata,
            &digest,
            catalog.current.as_ref(),
            limits,
            request,
        )?;
        stage = "objects";
        for node in reader
            .nodes()
            .filter(|node| node.kind() == graphit_fs::NodeKind::File)
        {
            check()?;
            let relative = node.relative().to_str().ok_or(io::ErrorKind::InvalidData)?;
            let relative = relative.replace(std::path::MAIN_SEPARATOR, "/");
            let mapped = if metadata.artifact_type == ArtifactType::Ast {
                if let Some(tail) = relative.strip_prefix("corpus.lance/") {
                    format!("corpus.lance/{tail}")
                } else if relative.starts_with("source.lance/") {
                    relative
                } else {
                    format!("graph/{relative}")
                }
            } else {
                relative
            };
            let target = key(&prefix, &mapped)?;
            let file = reader.open_file(node.relative())?;
            upload_file(
                store,
                &format!("{storage_prefix}/v1/projects/{producer}/artifacts/staging"),
                &target,
                &file,
                true,
                request,
                &check,
            )
            .await?;
        }
        reader.revalidate_bindings()?;
        stage = "descriptor";
        let target = key(&prefix, "native.json")?;
        match guarded(
            request,
            &check,
            store.put_opts(&target, root.clone().into(), PutMode::Create.into()),
        )
        .await
        {
            Ok(_) => {}
            Err(cause)
                if storage_error(&cause, |e| {
                    matches!(e, object_store::Error::AlreadyExists { .. })
                }) =>
            {
                let got = guarded(request, &check, store.get(&target)).await?;
                if got.meta.size != root.len() as u64 {
                    return Err(io::ErrorKind::AlreadyExists.into());
                }
                let bytes = guarded(request, &check, got.bytes()).await?;
                if bytes.as_ref() != root {
                    return Err(io::ErrorKind::AlreadyExists.into());
                }
            }
            Err(cause) => return Err(cause),
        }
        // A source change while the descriptor PUT was in flight must not
        // advertise the release. Per-operation guards retain only authority.
        reader.revalidate_bindings()?;
        stage = "catalog_write";
        write_catalog(
            store,
            catalog,
            encoded,
            &mut receipt.catalog,
            request,
            &check,
        )
        .await?;
        stage = "delivery";
        check()?;
        reader.revalidate_bindings()?;
        Ok(())
    }
    .await;
    match operation {
        Ok(()) => Ok(receipt),
        Err(cause) => Err(io::Error::new(
            cause.kind(),
            PublicationFailure {
                receipt,
                stage,
                cause,
            },
        )),
    }
}

pub(crate) async fn ast_components(
    graph: &std::sync::Arc<graphit_data::CapturedLocalGraph>,
    request: &RequestContext<'_>,
) -> io::Result<Vec<NativeComponent>> {
    use graphit_hub::native_root::{NativeRole, NativeSnapshot};
    let original = request.snapshot();
    let reader = graph.source_reader(original).map_err(io::Error::other)?;
    let file = reader.open_file(std::path::Path::new("icebug.json"))?;
    let mut stream = file.stream(0)?;
    let digest = graphit_hub::remote_storage::hash_reader(&mut stream, original)?;
    let mut components = vec![NativeComponent {
        role: NativeRole::Graph,
        path: "graph".to_owned(),
        snapshot: NativeSnapshot::Icebug {
            manifest_path: "icebug.json".to_owned(),
            manifest_sha256: digest,
        },
    }];
    let root = reader.source_path(original)?;
    for (directory, role) in [
        ("source.lance", NativeRole::Source),
        ("corpus.lance", NativeRole::EntitySearch),
    ] {
        if !reader
            .nodes()
            .any(|n| n.relative() == std::path::Path::new(directory))
        {
            continue;
        }
        graph.revalidate(original).map_err(io::Error::other)?;
        let directory_root = root.join(directory);
        let uri = directory_root.to_str().ok_or(io::ErrorKind::InvalidData)?;
        let dataset = lance::dataset::builder::DatasetBuilder::from_uri(uri)
            .load()
            .await
            .map_err(io::Error::other)?;
        let location = dataset.manifest_location();
        let (dataset_version, manifest_path) = (location.version, location.path.clone());
        let base = ObjectPath::from_filesystem_path(&directory_root).map_err(io::Error::other)?;
        let relative = manifest_path
            .prefix_match(&base)
            .ok_or(io::ErrorKind::InvalidData)?
            .map(|s| s.as_ref().to_owned())
            .collect::<Vec<_>>()
            .join("/");
        let file = reader.open_file(&std::path::Path::new(directory).join(&relative))?;
        let digest = graphit_hub::remote_storage::hash_reader(&mut file.stream(0)?, original)?;
        components.push(NativeComponent {
            role,
            path: directory.to_owned(),
            snapshot: NativeSnapshot::Lance {
                dataset_version,
                manifest_path: relative,
                manifest_sha256: digest,
            },
        });
    }
    graph.revalidate(original).map_err(io::Error::other)?;
    Ok(components)
}

pub(crate) fn metadata_limits(
    content: graphit_hub::native::ContentLimits,
    projection: graphit_hub::ProjectionLimits,
) -> MetadataLimits {
    MetadataLimits {
        bytes: content.max_input_bytes,
        nodes: content.max_nodes,
        scalar_bytes: content.max_scalar_bytes,
        collection_entries: content.max_collection_entries,
        versions: content.max_collection_entries,
        dependencies: content.max_collection_entries,
        projection,
    }
}

fn error_source<'a>(
    error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a (dyn std::error::Error + 'static)> {
    if let Some(error) = error.downcast_ref::<io::Error>() {
        return error
            .get_ref()
            .map(|inner| inner as &(dyn std::error::Error + 'static))
            .or_else(|| std::error::Error::source(error));
    }
    if let Some(error) = error.downcast_ref::<std::sync::Arc<io::Error>>() {
        return Some(error.as_ref());
    }
    error.source()
}

pub(crate) fn cause_code(cause: &io::Error) -> &'static str {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(cause);
    while let Some(error) = current {
        if let Some(error) = error.downcast_ref::<object_store::Error>() {
            return match error {
                object_store::Error::AlreadyExists { .. }
                | object_store::Error::Precondition { .. } => "conditional_conflict",
                object_store::Error::NotSupported { .. } => "storage_condition_unsupported",
                object_store::Error::NotFound { .. } => "storage_object_absent",
                _ => "storage_operation_failed",
            };
        }
        if error.is::<UploadCleanupFailure>() {
            return "upload_and_owned_cleanup_failed";
        }
        if error.is::<crate::native_storage_credentials::StorageCredentialsError>() {
            return "storage_credentials_failed";
        }
        if let Some(error) = error.downcast_ref::<io::Error>() {
            match error.kind() {
                io::ErrorKind::PermissionDenied => return "permission_denied",
                io::ErrorKind::TimedOut => return "deadline_expired",
                io::ErrorKind::Interrupted => return "cancelled",
                io::ErrorKind::InvalidData => return "invalid_snapshot_or_metadata",
                _ => {}
            }
        }
        current = error_source(error);
    }
    match cause.kind() {
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::TimedOut => "deadline_expired",
        io::ErrorKind::Interrupted => "cancelled",
        io::ErrorKind::AlreadyExists => "conditional_conflict",
        io::ErrorKind::InvalidData => "invalid_snapshot_or_metadata",
        io::ErrorKind::Unsupported => "unsupported_storage_operation",
        _ => "native_operation_failed",
    }
}

pub(crate) fn cleanup_residue(cause: &io::Error) -> Option<&str> {
    upload_cleanup_failure(cause).map(|cleanup| cleanup.temporary.as_str())
}

/// Finite causes keep credential-bearing storage errors out of public diagnostics.
pub(crate) fn cleanup_causes(cause: &io::Error) -> Option<serde_json::Value> {
    upload_cleanup_failure(cause).map(|cleanup| {
        serde_json::json!({
            "primary": cause_code(&cleanup.cause),
            "abort": cleanup.abort.as_ref().map(cause_code),
            "deletion": cleanup.deletion.as_ref().map(cause_code),
        })
    })
}

fn upload_cleanup_failure(cause: &io::Error) -> Option<&UploadCleanupFailure> {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(cause);
    while let Some(error) = current {
        if let Some(cleanup) = error.downcast_ref::<UploadCleanupFailure>() {
            return Some(cleanup);
        }
        current = error_source(error);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{
        artifact_store::{
            CopyPolicy, RetainedBytesLimits, RetainedFileSet, SnapshotPolicy, SourceReadLimits,
        },
        brand::Brand,
        config::ConfigSnapshot,
        file_lock::Cancellation,
    };
    use graphit_hub::policy::HubResourcePolicy;
    use std::{sync::Arc, time::Instant};
    fn policy() -> HubResourcePolicy {
        let brand = Brand::new("graphit").unwrap();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        HubResourcePolicy::resolve(&config).unwrap()
    }
    #[test]
    fn cleanup_diagnostics_preserve_primary_and_cleanup_causes_without_storage_payloads() {
        let cause = io::Error::other(UploadCleanupFailure {
            cause: io::Error::other(io::Error::new(
                io::ErrorKind::InvalidData,
                "private source and storage payload",
            )),
            abort: Some(io::Error::new(io::ErrorKind::Interrupted, "private abort")),
            deletion: Some(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private credential",
            )),
            temporary: "owned/staging/native-upload-test".into(),
        });
        assert_eq!(cause_code(&cause), "upload_and_owned_cleanup_failed");
        assert_eq!(
            cleanup_residue(&cause),
            Some("owned/staging/native-upload-test")
        );
        assert_eq!(
            cleanup_causes(&cause),
            Some(serde_json::json!({
                "primary":"invalid_snapshot_or_metadata",
                "abort":"cancelled",
                "deletion":"permission_denied",
            }))
        );
        assert!(cleanup_causes(&io::ErrorKind::Interrupted.into()).is_none());
    }

    fn metadata() -> PublicationMetadata {
        PublicationMetadata {
            provider: "fixture-provider".into(),
            artifact_type: ArtifactType::Ast,
            id: "namespace/ação".into(),
            version: "feature/a-ç".into(),
            name: "Fixture".into(),
            description: Some("Native publication".into()),
            tags: vec!["native".into()],
            author: None,
            dependencies: None,
        }
    }
    fn capture(
        directory: &std::path::Path,
        policy: &HubResourcePolicy,
        request: &RequestContext<'_>,
    ) -> CapturedSourceReader {
        let copy = policy.copy();
        let retained = RetainedBytesLimits::new(
            usize::try_from(copy.total_file_bytes_limit()).unwrap(),
            copy.node_limit(),
            copy.total_relative_path_bytes_limit(),
        )
        .unwrap();
        let snapshot = SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            copy,
            RetainedFileSet::new([], retained).unwrap(),
            retained,
        )
        .unwrap();
        CapturedSourceReader::capture(
            directory,
            graphit_fs::NodeKind::Directory,
            &snapshot,
            SourceReadLimits::new(64 * 1024, 64 * 1024, copy.total_file_bytes_limit()).unwrap(),
            Arc::clone(request.snapshot()),
        )
        .unwrap()
    }
    fn components(bytes: &[u8]) -> Vec<NativeComponent> {
        vec![NativeComponent {
            role: graphit_hub::native_root::NativeRole::Graph,
            path: "graph".into(),
            snapshot: graphit_hub::native_root::NativeSnapshot::Icebug {
                manifest_path: "icebug.json".into(),
                manifest_sha256: Sha256::digest(bytes).into(),
            },
        }]
    }
    #[tokio::test]
    async fn immutable_objects_descriptor_and_conditional_catalog_preserve_versions() {
        let policy = policy();
        let cancel = Cancellation::default();
        let request = policy.request_context(Instant::now(), &cancel).unwrap();
        let limits = metadata_limits(policy.content(), policy.projection());
        let producer: ProjectId = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let bytes = b"physical native graph manifest";
        std::fs::write(directory.path().join("icebug.json"), bytes).unwrap();
        let reader = capture(directory.path(), &policy, &request);
        let store = object_store::memory::InMemory::new();
        let mut metadata = metadata();
        let first = publish(
            PublishInput {
                store: &store,
                storage_prefix: "tenant/hub",
                producer,
                metadata: &metadata,
                reader: &reader,
                components: &components(bytes),
                limits,
            },
            &request,
            || request.check(),
        )
        .await
        .unwrap();
        assert_eq!(first.catalog, "published");
        let (id, version) = metadata.validate(limits).unwrap();
        let root_bytes = NativeRoot::encode(
            &NativeRootInput {
                producer,
                artifact_type: ArtifactType::Ast,
                id: &metadata.id,
                version: &metadata.version,
                components: &components(bytes),
            },
            limits,
            &request,
        )
        .unwrap();
        let root_digest: [u8; 32] = Sha256::digest(&root_bytes).into();
        let release =
            release_prefix(producer, ArtifactType::Ast, &id, &version, &root_digest).unwrap();
        let first_root = key("tenant/hub", &format!("{release}native.json")).unwrap();
        let retained = store.get(&first_root).await.unwrap().bytes().await.unwrap();
        let repeated = publish(
            PublishInput {
                store: &store,
                storage_prefix: "tenant/hub",
                producer,
                metadata: &metadata,
                reader: &reader,
                components: &components(bytes),
                limits,
            },
            &request,
            || request.check(),
        )
        .await
        .unwrap();
        assert_eq!(first.root_sha256, repeated.root_sha256);
        metadata.version = "second/版本".into();
        let second = publish(
            PublishInput {
                store: &store,
                storage_prefix: "tenant/hub",
                producer,
                metadata: &metadata,
                reader: &reader,
                components: &components(bytes),
                limits,
            },
            &request,
            || request.check(),
        )
        .await
        .unwrap();
        assert_ne!(first.root_sha256, second.root_sha256);
        let catalog = store
            .get(
                &key(
                    "tenant/hub",
                    &registry_key(producer, ArtifactType::Ast, &id).unwrap(),
                )
                .unwrap(),
            )
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let catalog = RemoteArtifactManifest::decode(&catalog, limits, &request).unwrap();
        assert_eq!(catalog.versions().len(), 2);
        assert_eq!(catalog.latest().unwrap().as_str(), "second/版本");
        assert_eq!(
            store.get(&first_root).await.unwrap().bytes().await.unwrap(),
            retained
        );
        let other_directory = tempfile::tempdir().unwrap();
        std::fs::write(
            other_directory.path().join("icebug.json"),
            b"another manifest",
        )
        .unwrap();
        let other_reader = capture(other_directory.path(), &policy, &request);
        let different = components(b"another manifest");
        let failure = publish(
            PublishInput {
                store: &store,
                storage_prefix: "tenant/hub",
                producer,
                metadata: &metadata,
                reader: &other_reader,
                components: &different,
                limits,
            },
            &request,
            || request.check(),
        )
        .await
        .unwrap_err();
        let failure = failure
            .get_ref()
            .unwrap()
            .downcast_ref::<PublicationFailure>()
            .unwrap();
        assert_eq!(failure.receipt.catalog, "not_attempted");
        assert_eq!(failure.cause.kind(), io::ErrorKind::AlreadyExists);
        let keys: Vec<_> = store.list(None).collect::<Vec<_>>().await;
        assert!(keys.iter().all(|key| {
            !key.as_ref()
                .unwrap()
                .location
                .as_ref()
                .contains("/artifacts/staging/")
        }));
    }
    #[tokio::test]
    async fn dependency_updates_preserve_replace_clear_and_reject_before_objects() {
        let policy = policy();
        let cancel = Cancellation::default();
        let request = policy.request_context(Instant::now(), &cancel).unwrap();
        let limits = metadata_limits(policy.content(), policy.projection());
        let producer: ProjectId = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let bytes = b"physical graph";
        std::fs::write(directory.path().join("icebug.json"), bytes).unwrap();
        let reader = capture(directory.path(), &policy, &request);
        let store = object_store::memory::InMemory::new();
        let mut metadata = metadata();
        let (id, _) = metadata.validate(limits).unwrap();
        let catalog_key = key(
            "tenant/hub",
            &registry_key(producer, ArtifactType::Ast, &id).unwrap(),
        )
        .unwrap();
        for (dependencies, expected) in [
            (
                Some(vec![PublicationDependency {
                    artifact_type: ArtifactType::Skill,
                    id: "fixture/first".into(),
                    selector: "release/one".into(),
                }]),
                Some("fixture/first"),
            ),
            (None, Some("fixture/first")),
            (
                Some(vec![PublicationDependency {
                    artifact_type: ArtifactType::Knowledge,
                    id: "fixture/replacement".into(),
                    selector: "latest".into(),
                }]),
                Some("fixture/replacement"),
            ),
            (Some(vec![]), None),
        ] {
            metadata.dependencies = dependencies;
            let receipt = publish(
                PublishInput {
                    store: &store,
                    storage_prefix: "tenant/hub",
                    producer,
                    metadata: &metadata,
                    reader: &reader,
                    components: &components(bytes),
                    limits,
                },
                &request,
                || request.check(),
            )
            .await
            .unwrap();
            assert_eq!(receipt.catalog, "published");
            let bytes = store
                .get(&catalog_key)
                .await
                .unwrap()
                .bytes()
                .await
                .unwrap();
            let catalog = RemoteArtifactManifest::decode(&bytes, limits, &request).unwrap();
            assert_eq!(
                catalog
                    .dependencies()
                    .first()
                    .map(|dependency| dependency.id().as_str()),
                expected
            );
            assert_eq!(
                catalog.dependencies().len(),
                usize::from(expected.is_some())
            );
            assert_eq!(catalog.versions().len(), 1);
        }
        let before: Vec<_> = store
            .list(None)
            .map(|entry| entry.unwrap().location)
            .collect()
            .await;
        metadata.version = "invalid-dependency".into();
        metadata.dependencies = Some(vec![PublicationDependency {
            artifact_type: ArtifactType::Skill,
            id: "fixture/invalid".into(),
            selector: "s".repeat(limits.scalar_bytes + 1),
        }]);
        let error = publish(
            PublishInput {
                store: &store,
                storage_prefix: "tenant/hub",
                producer,
                metadata: &metadata,
                reader: &reader,
                components: &components(bytes),
                limits,
            },
            &request,
            || request.check(),
        )
        .await
        .unwrap_err();
        let failure = error
            .get_ref()
            .unwrap()
            .downcast_ref::<PublicationFailure>()
            .unwrap();
        assert_eq!(failure.stage, "catalog_read");
        assert_eq!(failure.receipt.catalog, "not_attempted");
        assert!(
            failure
                .cause
                .get_ref()
                .unwrap()
                .is::<graphit_hub::remote_metadata::MetadataError>()
        );
        let after: Vec<_> = store
            .list(None)
            .map(|entry| entry.unwrap().location)
            .collect()
            .await;
        assert_eq!(
            before, after,
            "invalid dependencies create no immutable objects"
        );
    }

    #[tokio::test]
    async fn changed_source_at_publication_boundaries_cleans_staging_and_blocks_catalog() {
        use futures_util::{FutureExt, TryStreamExt};
        for (replace_binding, after_descriptor) in [(false, false), (true, false), (false, true)] {
            let policy = policy();
            let cancel = Cancellation::default();
            let request = policy.request_context(Instant::now(), &cancel).unwrap();
            let limits = metadata_limits(policy.content(), policy.projection());
            let producer: ProjectId = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
            let parent = tempfile::tempdir().unwrap();
            let directory = parent.path().join("graph");
            std::fs::create_dir(&directory).unwrap();
            let source = directory.join("icebug.json");
            let bytes = b"physical graph";
            std::fs::write(&source, bytes).unwrap();
            let reader = capture(&directory, &policy, &request);
            let store = object_store::memory::InMemory::new();
            let metadata = metadata();
            let changed = std::cell::Cell::new(false);
            let failure = publish(
                PublishInput {
                    store: &store,
                    storage_prefix: "tenant/hub",
                    producer,
                    metadata: &metadata,
                    reader: &reader,
                    components: &components(bytes),
                    limits,
                },
                &request,
                || {
                    request.check()?;
                    let objects = store
                        .list(None)
                        .try_collect::<Vec<_>>()
                        .now_or_never()
                        .expect("in-memory list is immediately ready")
                        .map_err(io::Error::other)?;
                    let reached_boundary = objects.iter().any(|entry| {
                        if after_descriptor {
                            entry.location.as_ref().ends_with("/native.json")
                        } else {
                            entry.location.as_ref().contains("/staging/")
                        }
                    });
                    if !changed.get() && reached_boundary {
                        // Change the actual source after multipart completion or
                        // while the descriptor PUT is completing.
                        if replace_binding {
                            std::fs::rename(&source, parent.path().join("replaced-source"))?;
                            std::fs::write(&source, bytes)?;
                        } else {
                            std::fs::write(&source, b"changed graph contents")?;
                        }
                        changed.set(true);
                    }
                    request.check()
                },
            )
            .await
            .unwrap_err();
            assert!(changed.get(), "the source change must actually happen");
            request.check().unwrap();
            let failure = failure
                .get_ref()
                .unwrap()
                .downcast_ref::<PublicationFailure>()
                .unwrap();
            assert_eq!(
                failure.stage,
                if after_descriptor {
                    "descriptor"
                } else {
                    "objects"
                }
            );
            assert_eq!(failure.receipt.catalog, "not_attempted");
            assert_eq!(cause_code(&failure.cause), "invalid_snapshot_or_metadata");
            assert!(cleanup_residue(&failure.cause).is_none());
            let remaining: Vec<_> = store
                .list(None)
                .map(|entry| entry.unwrap().location)
                .collect()
                .await;
            assert_eq!(remaining.len(), if after_descriptor { 2 } else { 1 });
            assert!(
                !remaining
                    .iter()
                    .any(|path| path.as_ref().contains("/staging/"))
            );
            assert!(
                !remaining
                    .iter()
                    .any(|path| path.as_ref().contains("/registry/"))
            );
            assert_eq!(
                remaining
                    .iter()
                    .any(|path| path.as_ref().ends_with("/native.json")),
                after_descriptor
            );
            let payload = remaining
                .iter()
                .find(|path| path.as_ref().ends_with("/graph/icebug.json"))
                .unwrap();
            assert_eq!(
                store
                    .get(payload)
                    .await
                    .unwrap()
                    .bytes()
                    .await
                    .unwrap()
                    .as_ref(),
                bytes,
            );
        }
    }

    #[tokio::test]
    async fn catalog_success_is_retained_when_the_final_guard_fails() {
        use futures_util::FutureExt;
        let policy = policy();
        let cancel = Cancellation::default();
        let request = policy.request_context(Instant::now(), &cancel).unwrap();
        let limits = metadata_limits(policy.content(), policy.projection());
        let producer: ProjectId = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let bytes = b"physical graph";
        std::fs::write(directory.path().join("icebug.json"), bytes).unwrap();
        let reader = capture(directory.path(), &policy, &request);
        let store = object_store::memory::InMemory::new();
        let metadata = metadata();
        let (id, _) = metadata.validate(limits).unwrap();
        let catalog = key(
            "tenant/hub",
            &registry_key(producer, ArtifactType::Ast, &id).unwrap(),
        )
        .unwrap();
        let failure = publish(
            PublishInput {
                store: &store,
                storage_prefix: "tenant/hub",
                producer,
                metadata: &metadata,
                reader: &reader,
                components: &components(bytes),
                limits,
            },
            &request,
            || {
                request.check()?;
                match store
                    .get(&catalog)
                    .now_or_never()
                    .expect("in-memory read is immediately ready")
                {
                    Ok(_) => Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "test final guard expired",
                    )),
                    Err(object_store::Error::NotFound { .. }) => Ok(()),
                    Err(error) => Err(io::Error::other(error)),
                }
            },
        )
        .await
        .unwrap_err();
        let failure = failure
            .get_ref()
            .unwrap()
            .downcast_ref::<PublicationFailure>()
            .unwrap();
        assert_eq!(failure.receipt.catalog, "published");
        assert_eq!(failure.stage, "delivery");
        assert_eq!(failure.cause.kind(), io::ErrorKind::TimedOut);
        let bytes = store.get(&catalog).await.unwrap().bytes().await.unwrap();
        assert_eq!(
            RemoteArtifactManifest::decode(&bytes, limits, &request)
                .unwrap()
                .versions()
                .len(),
            1
        );
    }

    /// Forward every operation to the real SDK store; only schedule one actual competing catalog write.
    #[derive(Debug)]
    struct ConcurrentCatalogWrite {
        inner: Arc<object_store::memory::InMemory>,
        catalog: ObjectPath,
        replacement: Vec<u8>,
        once: std::sync::atomic::AtomicBool,
    }
    impl std::fmt::Display for ConcurrentCatalogWrite {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("concurrent catalog writer")
        }
    }
    #[async_trait::async_trait]
    impl ObjectStore for ConcurrentCatalogWrite {
        async fn put_opts(
            &self,
            path: &ObjectPath,
            payload: object_store::PutPayload,
            opts: object_store::PutOptions,
        ) -> object_store::Result<object_store::PutResult> {
            self.inner.put_opts(path, payload, opts).await
        }
        async fn put_multipart_opts(
            &self,
            path: &ObjectPath,
            opts: object_store::PutMultipartOptions,
        ) -> object_store::Result<Box<dyn object_store::MultipartUpload>> {
            self.inner.put_multipart_opts(path, opts).await
        }
        async fn get_opts(
            &self,
            path: &ObjectPath,
            opts: object_store::GetOptions,
        ) -> object_store::Result<object_store::GetResult> {
            let observed = self.inner.get_opts(path, opts).await;
            if path == &self.catalog && self.once.swap(false, std::sync::atomic::Ordering::SeqCst) {
                let mode = match &observed {
                    Ok(got) => PutMode::Update(UpdateVersion {
                        e_tag: got.meta.e_tag.clone(),
                        version: got.meta.version.clone(),
                    }),
                    Err(object_store::Error::NotFound { .. }) => PutMode::Create,
                    _ => return observed,
                };
                self.inner
                    .put_opts(path, self.replacement.clone().into(), mode.into())
                    .await?;
            }
            observed
        }
        fn delete_stream(
            &self,
            paths: futures_util::stream::BoxStream<'static, object_store::Result<ObjectPath>>,
        ) -> futures_util::stream::BoxStream<'static, object_store::Result<ObjectPath>> {
            self.inner.delete_stream(paths)
        }
        fn list(
            &self,
            prefix: Option<&ObjectPath>,
        ) -> futures_util::stream::BoxStream<'static, object_store::Result<object_store::ObjectMeta>>
        {
            self.inner.list(prefix)
        }
        async fn list_with_delimiter(
            &self,
            prefix: Option<&ObjectPath>,
        ) -> object_store::Result<object_store::ListResult> {
            self.inner.list_with_delimiter(prefix).await
        }
        async fn copy_opts(
            &self,
            from: &ObjectPath,
            to: &ObjectPath,
            options: object_store::CopyOptions,
        ) -> object_store::Result<()> {
            self.inner.copy_opts(from, to, options).await
        }
    }
    #[tokio::test]
    async fn actual_conditional_catalog_create_and_update_conflicts_preserve_the_competing_write() {
        let policy = policy();
        let cancel = Cancellation::default();
        let request = policy.request_context(Instant::now(), &cancel).unwrap();
        let limits = metadata_limits(policy.content(), policy.projection());
        let producer: ProjectId = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let bytes = b"physical graph";
        std::fs::write(directory.path().join("icebug.json"), bytes).unwrap();
        let reader = capture(directory.path(), &policy, &request);
        let store = Arc::new(object_store::memory::InMemory::new());
        let mut metadata = metadata();
        publish(
            PublishInput {
                store: store.as_ref(),
                storage_prefix: "tenant/hub",
                producer,
                metadata: &metadata,
                reader: &reader,
                components: &components(bytes),
                limits,
            },
            &request,
            || request.check(),
        )
        .await
        .unwrap();
        let (id, _) = metadata.validate(limits).unwrap();
        let catalog = key(
            "tenant/hub",
            &registry_key(producer, ArtifactType::Ast, &id).unwrap(),
        )
        .unwrap();
        let previous = store.get(&catalog).await.unwrap().bytes().await.unwrap();
        let mut replacement: serde_json::Value = serde_json::from_slice(&previous).unwrap();
        replacement["name"] = serde_json::json!("Concurrent metadata");
        let replacement = serde_json::to_vec(&replacement).unwrap();
        metadata.version = "second/version".into();
        for create_race in [false, true] {
            if create_race {
                store.delete(&catalog).await.unwrap();
            }
            let race = ConcurrentCatalogWrite {
                inner: Arc::clone(&store),
                catalog: catalog.clone(),
                replacement: replacement.clone(),
                once: std::sync::atomic::AtomicBool::new(true),
            };
            let failure = publish(
                PublishInput {
                    store: &race,
                    storage_prefix: "tenant/hub",
                    producer,
                    metadata: &metadata,
                    reader: &reader,
                    components: &components(bytes),
                    limits,
                },
                &request,
                || request.check(),
            )
            .await
            .unwrap_err();
            let effect = failure
                .get_ref()
                .unwrap()
                .downcast_ref::<PublicationFailure>()
                .unwrap();
            assert_eq!(effect.stage, "catalog_write");
            assert_eq!(effect.receipt.catalog, "rejected");
            assert_eq!(cause_code(&effect.cause), "conditional_conflict");
            let current = store.get(&catalog).await.unwrap().bytes().await.unwrap();
            assert_eq!(current.as_ref(), replacement.as_slice());
            let current = RemoteArtifactManifest::decode(&current, limits, &request).unwrap();
            assert_eq!(current.name(), "Concurrent metadata");
            assert_eq!(current.versions().len(), 1);
        }
    }

    #[test]
    fn explicit_publication_preflight_rejects_nonconcrete_or_invalid_metadata() {
        let policy = policy();
        let limits = metadata_limits(policy.content(), policy.projection());
        let mut data = metadata();
        assert!(data.validate(limits).is_ok());
        data.version = "latest".into();
        assert!(data.validate(limits).is_err());
        data = metadata();
        data.name = "not/a/name".into();
        assert!(data.validate(limits).is_err());
        data = metadata();
        data.provider.clear();
        assert!(data.validate(limits).is_err());
    }
}
