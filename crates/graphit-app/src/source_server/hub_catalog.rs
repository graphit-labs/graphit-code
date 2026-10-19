//! Installed GlobalNative metadata. No payload or remote storage is opened.
use super::SourceServer;
use graphit_auth::broker::{BrokerVerifier, oauth::web::AuthenticatedWebCaller};
use graphit_core::artifact_store::StoreOwner;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceOrigin,
    policy::RequestContext,
    registry::{HubScope, InstalledBackend},
};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Scope {
    GlobalInstallation,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Query {
    pub scope: Scope,
    #[serde(default, deserialize_with = "optional")]
    pub artifact_type: Option<ArtifactType>,
    #[serde(default, deserialize_with = "optional")]
    pub id: Option<String>,
    #[serde(default, deserialize_with = "optional")]
    pub version: Option<String>,
    #[serde(default)]
    pub offset: usize,
    pub limit: usize,
}
fn optional<'de, T: Deserialize<'de>, D: serde::Deserializer<'de>>(
    value: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(value).map(Some)
}
#[derive(Serialize)]
struct Display<'a> {
    name: &'a str,
    description: Option<&'a str>,
    tags: &'a [String],
}
#[derive(Serialize)]
struct Entry<'a> {
    artifact_type: ArtifactType,
    id: &'a str,
    version: &'a str,
    producer: graphit_core::project_identity::ProjectId,
    target: &'static str,
    origin: SourceOrigin,
    provider: Option<&'a str>,
    alias: Option<&'a str>,
    entry_name: Option<&'a str>,
    display: Option<Display<'a>>,
}
#[derive(Serialize)]
struct Page<'a> {
    scope: &'static str,
    entries: Vec<Entry<'a>>,
    next_offset: Option<usize>,
}
fn library(kind: ArtifactType) -> bool {
    matches!(
        kind,
        ArtifactType::Ast | ArtifactType::Knowledge | ArtifactType::Language
    )
}
fn denied(_: graphit_auth::AuthError) -> io::Error {
    io::ErrorKind::PermissionDenied.into()
}

impl SourceServer {
    pub(crate) fn installed_native_catalog<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
        original: &'a RequestContext<'a>,
        query: Query,
    ) -> io::Result<Vec<u8>> {
        let policy = &self.0.policy.native_install.hub;
        let _ = query.scope;
        if query.limit == 0
            || query.limit > policy.content().max_collection_entries
            || query.offset.checked_add(query.limit).is_none()
            || query.artifact_type.is_some_and(|kind| !library(kind))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if let Some(id) = &query.id {
            ArtifactId::new(id, policy.projection()).map_err(|_| io::ErrorKind::InvalidInput)?;
        }
        if let Some(version) = &query.version {
            ConcreteVersion::new(version, policy.projection())
                .map_err(|_| io::ErrorKind::InvalidInput)?;
        }
        let capture = self.0.auth.installation();
        let read = self
            .0
            .auth
            .issuer()
            .admit_web_installed_hub_read(caller, verifier, original, provider, capture)
            .map_err(denied)?;
        let registry = &self.0.policy.registry;
        let before = || -> io::Result<()> {
            original.check()?;
            if self.0.stop.is_cancelled() {
                return Err(io::ErrorKind::NotConnected.into());
            }
            read.recheck().map_err(denied)?;
            let scope = read.installation().live;
            let StoreOwner::Installation { installation_id } = scope.owner() else {
                return Err(io::ErrorKind::PermissionDenied.into());
            };
            if scope.owner_root() != self.0.policy.installation.root()
                || registry.path().parent() != Some(scope.owner_root())
                || !matches!(registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str())
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            Ok(())
        };
        before()?;
        let document = registry.load()?;
        let mut entries = Vec::new();
        let mut matched = 0usize;
        let mut next_offset = None;
        let consumer = graphit_core::artifact_store::PublicationSlot::Scope;
        for (kind, id, artifact) in document.entries_for_consumer(&consumer) {
            if !library(kind)
                || query.artifact_type.is_some_and(|selected| selected != kind)
                || query.id.as_deref().is_some_and(|selected| selected != id)
            {
                continue;
            }
            for installed in &artifact.installations {
                original.check()?;
                if installed.target != InstallationTarget::GlobalNative {
                    continue;
                }
                let source = &installed.original_source;
                if query
                    .version
                    .as_deref()
                    .is_some_and(|selected| selected != source.version().as_str())
                {
                    continue;
                }
                if matched < query.offset {
                    matched += 1;
                    continue;
                }
                if entries.len() == query.limit {
                    next_offset = Some(matched);
                    break;
                }
                entries.push(Entry {
                    artifact_type: kind,
                    id,
                    version: source.version().as_str(),
                    producer: source.producer(),
                    target: "global_native",
                    origin: source.origin(),
                    provider: match &installed.backend {
                        InstalledBackend::RemoteNative(remote) => Some(remote.provider_name()),
                        _ => None,
                    },
                    alias: installed.alias.as_deref(),
                    entry_name: installed.entry_name.as_deref(),
                    display: installed.display.as_ref().map(|value| Display {
                        name: value.name(),
                        description: value.description(),
                        tags: value.tags(),
                    }),
                });
                matched += 1;
            }
            if next_offset.is_some() {
                break;
            }
        }
        before()?;
        let output = serde_json::to_vec(&Page {
            scope: "global_installation",
            entries,
            next_offset,
        })
        .map_err(io::Error::other)?;
        if output.len() > policy.content().max_output_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        before()?;
        Ok(output)
    }
}
