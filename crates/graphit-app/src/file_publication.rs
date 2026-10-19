//! Ordinary FILE payload upload followed by a conditional current catalog.
mod archive;
pub mod client;
use crate::native_publication::{self, CatalogIdentity};
use graphit_core::{artifact_store::CapturedSourceReader, project_identity::ProjectId};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion,
    policy::{HubResourcePolicy, RequestContext},
    remote_metadata::{
        AuthorInput, DependencyInput, ManifestInput, MetadataLimits, RemoteArtifactManifest,
        VersionInput,
    },
    remote_storage::release_prefix,
};
use object_store::{ObjectStore, path::Path as ObjectPath};
use serde::{Deserialize, Serialize};
use std::io;

pub(crate) use crate::native_publication::PublicationDependency as Dependency;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    pub(crate) provider: String,
    pub(crate) artifact_type: crate::remote_file_install::client::Kind,
    pub(crate) id: String,
    pub(crate) version: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    pub(crate) author: Option<String>,
    pub(crate) dependencies: Option<Vec<Dependency>>,
    pub(crate) expected_sha256: Option<String>,
    #[serde(default)]
    pub(crate) set_latest: bool,
}
impl Metadata {
    pub(crate) fn validate(
        &self,
        policy: &HubResourcePolicy,
    ) -> io::Result<(ArtifactId, ConcreteVersion)> {
        if self.provider.is_empty()
            || self.provider.trim() != self.provider
            || self.provider.chars().any(char::is_control)
            || self.provider.len() > policy.content().max_scalar_bytes
            || self.version.eq_ignore_ascii_case("latest")
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if let Some(expected) = &self.expected_sha256
            && (expected.len() != 64
                || !expected
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok((
            ArtifactId::new(&self.id, policy.projection())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?,
            ConcreteVersion::new(&self.version, policy.projection())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?,
        ))
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct Receipt {
    pub(crate) producer: ProjectId,
    pub(crate) artifact_type: ArtifactType,
    pub(crate) id: String,
    pub(crate) version: String,
    pub(crate) archive_sha256: String,
    pub(crate) payload: &'static str,
    pub(crate) catalog: &'static str,
}
#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) receipt: Receipt,
    pub(crate) stage: &'static str,
    pub(crate) cause: io::Error,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FILE publication failed at {} (payload {}, catalog {})",
            self.stage, self.receipt.payload, self.receipt.catalog
        )
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
pub(crate) struct Prepared<'a> {
    metadata: &'a Metadata,
    producer: ProjectId,
    reader: Option<&'a CapturedSourceReader>,
    policy: &'a HubResourcePolicy,
    archive: archive::Archive,
    id: ArtifactId,
    version: ConcreteVersion,
}

/// Validate finalized actual bytes and supplied metadata before storage I/O.
pub(crate) fn prepare<'a>(
    metadata: &'a Metadata,
    producer: ProjectId,
    reader: &'a CapturedSourceReader,
    policy: &'a HubResourcePolicy,
    original: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<Prepared<'a>> {
    check()?;
    let (id, version) = metadata.validate(policy)?;
    let archive = archive::spool(reader, policy, original, check)?;
    prepare_archive(
        Prepared {
            metadata,
            producer,
            reader: Some(reader),
            policy,
            archive,
            id,
            version,
        },
        original,
        check,
    )
}

/// The caller owns a private directory containing the finalized payload.zip.
pub(crate) fn prepare_received<'a>(
    metadata: &'a Metadata,
    producer: ProjectId,
    owner: tempfile::TempDir,
    policy: &'a HubResourcePolicy,
    original: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<Prepared<'a>> {
    check()?;
    let (id, version) = metadata.validate(policy)?;
    let file = std::fs::File::open(owner.path().join("payload.zip"))?;
    let archive = archive::received(owner, file, policy, original, check)?;
    prepare_archive(
        Prepared {
            metadata,
            producer,
            reader: None,
            policy,
            archive,
            id,
            version,
        },
        original,
        check,
    )
}

fn prepare_archive<'a>(
    prepared: Prepared<'a>,
    original: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<Prepared<'a>> {
    let Prepared {
        metadata,
        producer,
        policy,
        ref archive,
        ..
    } = prepared;
    // Expected hash is a catalog precondition, not a manifest field. The actual
    // metadata and finalized digest can be codec-validated before that read.
    let mut preflight = metadata.clone();
    preflight.expected_sha256 = None;
    encoded_catalog(
        &preflight,
        producer,
        &archive.sha256,
        None,
        native_publication::metadata_limits(policy.content(), policy.projection()),
        original,
    )?;
    check()?;
    Ok(prepared)
}

pub(crate) struct Input<'a> {
    pub(crate) store: &'a dyn ObjectStore,
    pub(crate) storage_prefix: &'a str,
    pub(crate) prepared: Prepared<'a>,
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn encoded_catalog(
    metadata: &Metadata,
    producer: ProjectId,
    digest: &[u8; 32],
    current: Option<&RemoteArtifactManifest>,
    limits: MetadataLimits,
    original: &RequestContext<'_>,
) -> io::Result<Vec<u8>> {
    let digest = hex(digest);
    let old_hashes: Vec<_> = current
        .map(|c| c.versions().iter().map(|v| hex(v.hash())).collect())
        .unwrap_or_default();
    let mut versions = Vec::new();
    if let Some(current) = current {
        for (old, hash) in current.versions().iter().zip(&old_hashes) {
            if old.version().as_str() == metadata.version {
                if hash != &digest && metadata.expected_sha256.as_ref() != Some(hash) {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "FILE version replacement requires the current expected hash",
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
    if metadata.expected_sha256.is_some()
        && !current.is_some_and(|c| {
            c.versions()
                .iter()
                .any(|v| v.version().as_str() == metadata.version)
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "FILE replacement version is absent",
        ));
    }
    versions.push(VersionInput {
        version: &metadata.version,
        hash: &digest,
    });
    let dependencies: Vec<_> = match &metadata.dependencies {
        Some(dependencies) => dependencies
            .iter()
            .map(|d| DependencyInput {
                artifact_type: d.artifact_type,
                id: &d.id,
                selector: &d.selector,
            })
            .collect(),
        None => current
            .map(|c| {
                c.dependencies()
                    .iter()
                    .map(|d| DependencyInput {
                        artifact_type: d.artifact_type(),
                        id: d.id().as_str(),
                        selector: d.selector(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    let tags: Vec<_> = metadata.tags.iter().map(String::as_str).collect();
    RemoteArtifactManifest::encode(
        &ManifestInput {
            producer,
            artifact_type: metadata.artifact_type.artifact_type(),
            id: &metadata.id,
            name: &metadata.name,
            description: metadata.description.as_deref(),
            tags: &tags,
            author: metadata
                .author
                .as_deref()
                .map(|username| AuthorInput { username }),
            latest: if metadata.set_latest {
                Some(metadata.version.as_str())
            } else {
                current
                    .and_then(RemoteArtifactManifest::latest)
                    .map(ConcreteVersion::as_str)
            },
            versions: &versions,
            dependencies: &dependencies,
        },
        limits,
        original,
    )
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

pub(crate) async fn publish(
    input: Input<'_>,
    original: &RequestContext<'_>,
    check: impl Fn() -> io::Result<()>,
) -> io::Result<Receipt> {
    let Input {
        store,
        storage_prefix,
        prepared,
    } = input;
    check()?;
    let Prepared {
        producer,
        metadata,
        reader,
        policy,
        archive,
        id,
        version,
    } = prepared;
    archive.reader.check_request(original.snapshot())?;
    if let Some(reader) = reader {
        reader.check_request(original.snapshot())?;
    }
    let limits = native_publication::metadata_limits(policy.content(), policy.projection());
    let digest = archive.sha256;
    let mut receipt = Receipt {
        producer,
        artifact_type: metadata.artifact_type.artifact_type(),
        id: metadata.id.clone(),
        version: metadata.version.clone(),
        archive_sha256: hex(&digest),
        payload: "not_attempted",
        catalog: "not_attempted",
    };
    let mut stage = "catalog_read";
    let operation = async {
        let catalog = native_publication::read_catalog(
            store,
            CatalogIdentity {
                storage_prefix,
                producer,
                artifact_type: receipt.artifact_type,
                id: &id,
                limits,
            },
            original,
            &check,
        )
        .await?;
        let reuse_existing = catalog.current.as_ref().is_some_and(|c| {
            c.versions()
                .iter()
                .any(|v| v.version() == &version && v.hash() == &digest)
        });
        let encoded = encoded_catalog(
            metadata,
            producer,
            &digest,
            catalog.current.as_ref(),
            limits,
            original,
        )?;
        let release = release_prefix(producer, receipt.artifact_type, &id, &version, &digest)?;
        let prefix = format!("{storage_prefix}/{}", release.trim_end_matches('/'));
        let key = format!("{prefix}/payload.zip");
        if key.len() > 1024 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let target = ObjectPath::parse(key).map_err(io::Error::other)?;
        stage = "payload";
        check()?;
        receipt.payload = "uncertain";
        if let Err(cause) = native_publication::upload_file(
            store,
            &format!("{storage_prefix}/v1/projects/{producer}/artifacts/staging"),
            &target,
            &archive.file,
            reuse_existing,
            original,
            &check,
        )
        .await
        {
            if native_publication::cause_code(&cause) == "conditional_conflict" {
                receipt.payload = "rejected";
            }
            return Err(cause);
        }
        receipt.payload = "published";
        archive.reader.revalidate_bindings()?;
        if let Some(reader) = reader {
            reader.revalidate_bindings()?;
        }
        stage = "catalog_write";
        native_publication::write_catalog(
            store,
            catalog,
            encoded,
            &mut receipt.catalog,
            original,
            &check,
        )
        .await?;
        stage = "delivery";
        check()?;
        archive.reader.revalidate_content()?;
        if let Some(reader) = reader {
            reader.revalidate_content()?;
        }
        Ok(())
    }
    .await;
    match operation {
        Ok(()) => Ok(receipt),
        Err(cause) => Err(io::Error::new(
            cause.kind(),
            Failure {
                receipt,
                stage,
                cause,
            },
        )),
    }
}

#[cfg(test)]
mod tests;
