//! Remote FILE installation through the existing authenticated Source request owner.
use super::*;
mod content_selection;
pub(super) mod lifecycle;
mod listing;
mod templates;
pub(super) mod update;
use crate::remote_file_install::{
    FileSelection, FileTransport,
    client::{
        InstallArguments, InstalledContentArguments, ListArguments, RemoteArguments,
        RemoteContentArguments, Transport,
    },
};
use graphit_auth::{ExternalEffect, Obligation, Permission, ServiceMode};
use graphit_core::module_settings::Module;
use graphit_core::{
    artifact_store::{
        ArtifactMode, ArtifactStore, CapturedPublicationScope, CopyPolicy, PayloadOverlay,
        PayloadPolicy, PayloadSelection, ScopeContext, SourceSnapshot, StoreOwner,
        ValidatedPayloadPlan,
    },
    native_path::NativePath,
    project_identity::ProjectIdentityStore,
};
use graphit_hub::{
    ArtifactType, InstallationTarget, ProjectionRequest, RemoteFileOrigin, SourceRef,
    materialization::DerivedArtifact,
    native::{
        AgentDescriptor, RenderContext, SkillDescriptor, ToolGrants, render_agent, render_skill,
    },
    publication::{PublicationOutcome, ScopedCoordinator},
    registry::{
        ArtifactDisplayMetadata, HubRegistry, Installation, InstalledArtifact, InstalledBackend,
        InstalledPayload,
    },
    remote_version::VersionSelector,
};
use std::path::{Path, PathBuf};

pub(super) const TOOL: &str = "graphit_hub_install";
const UPDATE: &str = "graphit_hub_update";
const SYNC: &str = "graphit_hub_sync";
const CONTENT: &str = "graphit_hub_content";
const LIST: &str = "graphit_hub_list";
pub(super) fn handles(name: &str) -> bool {
    matches!(name, TOOL | UPDATE | SYNC | CONTENT | LIST)
        || name == crate::sync_entry::TOOL
        || name == crate::local_file_install::TOOL
        || crate::remote_file_install::lifecycle::Operation::from_tool(name).is_some()
}
pub(super) fn definitions() -> [Tool; 10] {
    [
        TOOL,
        crate::sync_entry::TOOL,
        crate::local_file_install::TOOL,
        UPDATE,
        SYNC,
        CONTENT,
        LIST,
        "graphit_hub_remove",
        "graphit_hub_recover",
        "graphit_hub_forget",
    ]
    .map(|name| definition(name).expect("known FILE tool"))
}
pub(super) fn definition(name: &str) -> Option<Tool> {
    if name == crate::sync_entry::TOOL {
        return Some(super::sync::definition());
    }
    if name == crate::local_file_install::TOOL {
        return Some(super::local_file_install::definition());
    }
    let (description, schema) = match name {
        TOOL => (
            "Install an authorized remote FILE artifact with verified cache and explicit global or project scope.",
            install_schema(),
        ),
        UPDATE => (
            "Update one installed FILE artifact, or omit selection to update the captured target's remote installations. Local authored entries are left unchanged; failures include per-item effects and unattempted items.",
            update::schema(),
        ),
        SYNC => (
            "Reapply installed local and remote Hub artifacts from their recorded sources and exact versions; preserve aliases, modes and user changes.",
            update::sync_schema(),
        ),
        CONTENT => (
            "Read authorized installed FILE content without hydration, or explicitly fetch remote content into a temporary request-owned stage.",
            content_schema(),
        ),
        LIST => (
            "List installed Hub metadata with explicit scope, version filters and pagination. Cache metadata grants no remote project access.",
            list_schema(),
        ),
        "graphit_hub_remove" => (
            "Remove an exact local PROJECT target or a concrete GlobalCache/GlobalNative metadata version; no remote payload or storage access.",
            lifecycle::schema(crate::remote_file_install::lifecycle::Operation::Remove),
        ),
        "graphit_hub_recover" => (
            "Recover the selected FILE PROJECT pending operation; global cache/native metadata cannot enter a physical journal.",
            lifecycle::schema(crate::remote_file_install::lifecycle::Operation::Recover),
        ),
        "graphit_hub_forget" => (
            "Forget same-key FILE PROJECT metadata/pending state and preserve physical leftovers; global scope is not accepted.",
            lifecycle::schema(crate::remote_file_install::lifecycle::Operation::Forget),
        ),
        _ => return None,
    };
    Some(Tool::new(
        name.to_owned(),
        description,
        schema.as_object().expect("static schema").clone(),
    ))
}
fn file_kind_schema() -> Value {
    json!({"type":"string","enum":["skill","command","workflow","agent","rule","mcp","language","power"]})
}
fn selection_schema() -> Value {
    let text = json!({"type":["string","null"]});
    json!({"type":"object","properties":{
        "transport":{"type":"string","enum":["broker","standalone"]},"provider":{"type":"string"},"profile":text,"identity_provider":text,
        "producer":{"type":"string"},"artifact_type":file_kind_schema(),"id":{"type":"string"},"version":{"type":"string"},"exact":{"type":"boolean"}
    },"required":["transport","provider","producer","artifact_type","id","version","exact"],"additionalProperties":false})
}
fn install_schema() -> Value {
    let text = json!({"type":["string","null"]});
    json!({"type":"object","properties":{"selection":selection_schema(),"global":{"type":"boolean"},"project_dir":text,"project_version":text,"binding":text,"alias":text,"entry_name":text,"mode":{"type":"string","enum":["copy","link"],"description":"Rendered agents, skills, commands and workflows require copy; original sources may use link. Omission preserves an existing mode or defaults to copy."}},"required":["selection","global"],"additionalProperties":false})
}
fn content_schema() -> Value {
    let text = json!({"type":["string","null"]});
    json!({"type":"object","properties":{
        "installed":{"type":"object","properties":{"artifact_type":file_kind_schema(),"id":{"type":"string"},"version":text,"identity_provider":text,"global":{"type":"boolean"},"project_dir":text,"project_version":text,"binding":text,"path":text},"required":["artifact_type","id","global"],"additionalProperties":false},
        "remote":{"type":"object","properties":{"selection":selection_schema(),"path":{"type":"string"}},"required":["selection","path"],"additionalProperties":false}
    },"oneOf":[{"required":["installed"]},{"required":["remote"]}],"additionalProperties":false})
}
fn list_schema() -> Value {
    let text = json!({"type":["string","null"]});
    json!({"type":"object","properties":{"global":{"type":"boolean"},"identity_provider":text,"project_dir":text,"project_version":text,"artifact_type":{"anyOf":[file_kind_schema(),{"type":"null"}]},"id":text,"version":text,"binding":text,"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1}},"required":["global"],"additionalProperties":false})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContentInput {
    installed: Option<InstalledContentArguments>,
    remote: Option<RemoteContentArguments>,
}
enum Operation {
    Install,
    Update,
    InstalledContent {
        path: String,
        source: Box<SourceRef>,
    },
    RemoteContent(String),
}
impl Operation {
    fn writes(&self) -> bool {
        matches!(self, Self::Install | Self::Update)
    }
}

#[derive(Debug)]
pub(crate) struct FileEffectFailure {
    pub outcome: Value,
    cause: io::Error,
}
impl FileEffectFailure {
    pub(crate) fn observed(outcome: Value, cause: io::Error) -> io::Error {
        io::Error::new(cause.kind(), Self { outcome, cause })
    }
}
impl std::fmt::Display for FileEffectFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Hub FILE operation failed")
    }
}
impl std::error::Error for FileEffectFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
#[derive(Debug)]
struct PublicationCleanup(graphit_hub::publication::PublicationFailure);
pub(super) fn cleanup_failure(failure: graphit_hub::publication::PublicationFailure) -> io::Error {
    io::Error::other(PublicationCleanup(failure))
}
impl std::fmt::Display for PublicationCleanup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Hub FILE publication cleanup remains pending")
    }
}
impl std::error::Error for PublicationCleanup {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        use graphit_hub::publication::PublicationFailure;
        Some(match &self.0 {
            PublicationFailure::Local(error) => error,
            PublicationFailure::Projection(error) => error,
            PublicationFailure::Physical(error) => error,
        })
    }
}
fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}
pub(super) fn version<'a>(
    input: &RemoteArguments,
    literal: &'a ConcreteVersion,
) -> VersionSelector<'a> {
    if !input.exact {
        if input.version == "latest" {
            return VersionSelector::Latest;
        }
        let mut parts = input
            .version
            .strip_prefix('v')
            .unwrap_or(&input.version)
            .split('.');
        if let Some(major) = parts.next().and_then(|part| part.parse::<u64>().ok()) {
            let minor = parts.next();
            if parts.next().is_none() {
                match minor {
                    None => return VersionSelector::ReleasePrefix { major, minor: None },
                    Some(value) => {
                        if let Ok(minor) = value.parse() {
                            return VersionSelector::ReleasePrefix {
                                major,
                                minor: Some(minor),
                            };
                        }
                    }
                }
            }
        }
    }
    VersionSelector::Exact(literal)
}
pub(super) fn remote_origin(input: &RemoteArguments) -> RemoteFileOrigin {
    match input.transport {
        Transport::Broker => RemoteFileOrigin::Broker {
            provider: input.provider.clone(),
        },
        Transport::Standalone => RemoteFileOrigin::Standalone {
            provider: input.provider.clone(),
            profile: input.profile.clone(),
        },
    }
}

async fn execute(
    state: &State,
    name: &str,
    input: Value,
    credential: &str,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    if name == crate::sync_entry::TOOL {
        return super::sync::execute(state, input, credential, request).await;
    }
    if name == crate::local_file_install::TOOL {
        return super::local_file_install::execute(state, input, credential, request);
    }
    if name == LIST {
        let input = serde_json::from_value(input)
            .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
        return listing::execute(state, input, credential, request).await;
    }
    if name == SYNC {
        let input = serde_json::from_value(input)
            .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
        return update::sync(state, input, credential, request).await;
    }
    if name == UPDATE && input.get("selection").is_none() {
        let input = serde_json::from_value(input)
            .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
        return update::execute(state, input, credential, request).await;
    }
    if let Some(operation) = crate::remote_file_install::lifecycle::Operation::from_tool(name) {
        let input = serde_json::from_value(input)
            .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
        return lifecycle::execute(state, operation, input, credential, request).await;
    }
    let (mut input, operation) = if name == CONTENT {
        let input: ContentInput = serde_json::from_value(input)
            .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
        match (input.installed, input.remote) {
            (Some(input), None) => {
                let (input, path, source) =
                    content_selection::installed(state, input, credential, request)?;
                (
                    input,
                    Operation::InstalledContent {
                        path,
                        source: Box::new(source),
                    },
                )
            }
            (None, Some(input)) => {
                PayloadSelection::new(PathBuf::from(&input.path), graphit_fs::NodeKind::File)?;
                (
                    InstallArguments {
                        selection: input.selection,
                        global: true,
                        project_dir: None,
                        project_version: None,
                        binding: None,
                        alias: None,
                        entry_name: None,
                        mode: None,
                    },
                    Operation::RemoteContent(input.path),
                )
            }
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        }
    } else {
        let input = serde_json::from_value(input)
            .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
        (
            input,
            if name == UPDATE {
                Operation::Update
            } else if name == TOOL {
                Operation::Install
            } else {
                return Err(io::ErrorKind::InvalidInput.into());
            },
        )
    };
    let mut outcome =
        json!({"status":"error","cache":"not_attempted","publication":"not_attempted"});
    let result = install(
        state,
        &mut input,
        &operation,
        credential,
        request,
        &mut outcome,
    )
    .await;
    match result {
        Ok(()) => {
            outcome["status"] = "ok".into();
            Ok(outcome)
        }
        Err(cause) => {
            // A failed final authority/binding check must never deliver read content.
            if let Some(fields) = outcome.as_object_mut() {
                fields.remove("content");
            }
            if outcome["cache"] == "in_progress" {
                outcome["cache"] = "not_confirmed".into();
            }
            Err(io::Error::new(
                cause.kind(),
                FileEffectFailure { outcome, cause },
            ))
        }
    }
}
async fn install(
    state: &State,
    input: &mut InstallArguments,
    operation: &Operation,
    credential: &str,
    request: &RequestContext<'_>,
    outcome: &mut Value,
) -> io::Result<()> {
    let policy = &state.policy.native_install.hub;
    input.selection.validate(policy)?;
    if input
        .project_dir
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if input.global
        && (input.project_dir.is_some()
            || input.project_version.is_some()
            || input.binding.is_some()
            || input.alias.is_some()
            || input.entry_name.is_some()
            || input.mode.is_some())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    for value in input
        .alias
        .as_deref()
        .into_iter()
        .chain(input.entry_name.as_deref())
    {
        if value.contains('/') {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        ArtifactId::new(value, policy.projection()).map_err(io::Error::other)?;
    }
    let producer = input.selection.producer.parse().map_err(io::Error::other)?;
    let kind = input.selection.kind.artifact_type();
    let id = ArtifactId::new(&input.selection.id, policy.projection()).map_err(io::Error::other)?;
    let literal = ConcreteVersion::new(&input.selection.version, policy.projection())
        .map_err(io::Error::other)?;
    let issuer = state.auth.issuer();
    let caller = issuer
        .authenticate_runtime(credential, request)
        .map_err(denied)?;
    let mut service = match input.selection.transport {
        Transport::Broker => issuer.capture_hub_service(&caller, &input.selection.provider),
        Transport::Standalone => issuer.capture_standalone_s3_service(
            &caller,
            &input.selection.provider,
            input.selection.profile.as_deref(),
        ),
    }
    .map_err(denied)?;
    let verifier = if service.mode().map_err(denied)? == ServiceMode::BrokerNative {
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or(io::ErrorKind::NotConnected)?
                .get(&mut service)
                .map_err(io::Error::other)?,
        )
    } else {
        None
    };
    let lease = match verifier.as_deref() {
        Some(verifier) => service.native_credential(verifier).await.map_err(denied)?,
        None => service.credential().map_err(denied)?,
    };
    let mut identity = input
        .selection
        .identity_provider
        .as_deref()
        .map(|provider| issuer.capture_hub_service(&caller, provider))
        .transpose()
        .map_err(denied)?;
    let identity_verifier = if let Some(identity) = identity.as_mut() {
        if identity.mode().map_err(denied)? != ServiceMode::BrokerNative {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or(io::ErrorKind::NotConnected)?
                .get(identity)
                .map_err(io::Error::other)?,
        )
    } else {
        None
    };
    let identity_lease = match (identity.as_mut(), identity_verifier.as_deref()) {
        (Some(identity), Some(verifier)) => {
            Some(identity.native_credential(verifier).await.map_err(denied)?)
        }
        _ => None,
    };
    let subject = match &identity_lease {
        Some(identity) => identity.user_subject().map_err(denied)?,
        None => lease.user_subject().map_err(denied)?,
    };
    let transport = FileTransport::open(
        &lease,
        subject,
        producer,
        &state.native_http,
        state.native_allow_loopback_http,
    )
    .await?;
    // Token renewal precedes this fresh management proof, as in Native installation.
    let manager = issuer
        .authenticate_runtime(credential, request)
        .map_err(denied)?;
    let session = if operation.writes() {
        Some(issuer.bootstrap_admin(&manager).map_err(denied)?)
    } else {
        None
    };
    let parent = session
        .as_ref()
        .map(|session| issuer.admin_parent(&manager, session))
        .transpose()
        .map_err(denied)?;
    let before = || -> io::Result<()> {
        request.check()?;
        transport.check()?;
        if let Some(parent) = &parent {
            for permission in [
                Permission::Configure,
                Permission::ManageCache,
                Permission::External(ExternalEffect::PublishedFilesystem),
            ] {
                parent.authorize(permission).map_err(denied)?;
            }
        } else if manager
            .authorize(Permission::Read(Module::Hub))
            .map_err(denied)?
            != Obligation::Unconditional
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    };
    before()?;
    let installation = &state.policy.installation;
    let live = state.auth.installation().live;
    let StoreOwner::Installation { installation_id } = live.owner() else {
        return Err(io::ErrorKind::PermissionDenied.into());
    };
    if live.owner_root() != installation.root() {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let cache_scope = CapturedPublicationScope::installation(
        installation,
        installation_id.clone(),
        [installation.root().to_owned()],
        state.policy.scope,
        request.snapshot(),
    )?;
    let cache = ArtifactStore::new(cache_scope.clone(), policy.artifacts());
    let projects = state.live_projects.snapshot()?;
    let project = if input.global {
        None
    } else {
        let index = match input.project_dir.as_deref() {
            Some(root) => super::select_project(
                &projects.projects,
                &std::fs::canonicalize(root)?,
                input.project_version.as_deref(),
            )
            .map_err(io::Error::other)?,
            None if input.project_version.is_none() => {
                projects.current.ok_or(io::ErrorKind::NotFound)?
            }
            None => return Err(io::ErrorKind::InvalidInput.into()),
        };
        Some(
            projects
                .projects
                .get(index)
                .ok_or(io::ErrorKind::NotFound)?,
        )
    };
    let (scope, registry, root) = if let Some((paths, project_id, version)) = project {
        if !live.allowed_roots().any(|root| root == paths.root()) || version.scope() != paths.root()
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let identity = ProjectIdentityStore::at(paths)
            .load()?
            .ok_or(io::ErrorKind::NotFound)?;
        if identity.id() != *project_id {
            return Err(io::ErrorKind::InvalidData.into());
        }
        (
            CapturedPublicationScope::project(
                paths,
                *project_id,
                state.policy.scope,
                request.snapshot(),
            )?,
            HubRegistry::project(paths, &identity, policy.hub())?,
            NativePath::new(paths.root().to_owned())?,
        )
    } else {
        (
            cache_scope,
            state.policy.registry.clone(),
            NativePath::new(installation.root().to_owned())?,
        )
    };
    let consumer = graphit_hub::registry::consumer_for_artifact(scope.publication_slot(), kind);
    let origin = remote_origin(&input.selection);
    let binding = input.binding.unwrap_or(state.policy.binding);
    let target = if input.global {
        InstallationTarget::GlobalCache
    } else {
        InstallationTarget::for_artifact(binding, kind)
    };
    let (source, resolved) = if let Operation::InstalledContent { source, .. } = operation {
        if source.producer() != producer
            || source.artifact_type() != kind
            || source.id() != &id
            || source.version() != &literal
            || source.remote_file_origin() != Some(&origin)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        transport.authorize(policy).await?;
        (source.as_ref().clone(), None)
    } else {
        let resolved = transport
            .resolve(
                FileSelection {
                    producer,
                    kind,
                    id: &id,
                    version: version(&input.selection, &literal),
                },
                policy,
            )
            .await?;
        let source = SourceRef::hub_cache(
            installation,
            producer,
            kind,
            id,
            None,
            resolved.version.clone(),
            policy.projection(),
        )
        .map_err(io::Error::other)?
        .with_remote_file_origin(origin, resolved.archive_sha256, policy.projection())
        .map_err(io::Error::other)?;
        (source, Some(resolved))
    };
    outcome["producer"] = producer.to_string().into();
    outcome["id"] = input.selection.id.clone().into();
    outcome["version"] = source.version().as_str().into();
    outcome["global"] = input.global.into();
    if let Operation::RemoteContent(path) = operation {
        before()?;
        let stage = transport
            .download(
                resolved.as_ref().ok_or(io::ErrorKind::InvalidData)?,
                installation.root(),
                policy,
            )
            .await?;
        outcome["content"] = transport.read_text_file(
            stage.path(),
            path,
            state.policy.index,
            state.policy.content.max_output_bytes,
        )?;
        outcome["cache"] = "ephemeral".into();
        before()?;
        return Ok(());
    }
    let previous = {
        let document = registry.load()?;
        let existing = if input.global {
            document.get_version_for_consumer(
                &consumer,
                kind,
                source.id().as_str(),
                source.version(),
            )
        } else {
            document.get_for_consumer(&consumer, kind, source.id().as_str())
        };
        let exact = existing.and_then(|artifact| {
            artifact
                .installations
                .iter()
                .find(|entry| entry.target == target)
        });
        let old = if exact.is_none() && input.global && matches!(operation, Operation::Update) {
            document
                .entries()
                .filter(|(entry_kind, entry_id, _)| {
                    *entry_kind == kind && *entry_id == source.id().as_str()
                })
                .flat_map(|(_, _, artifact)| artifact.installations.iter())
                .find(|entry| {
                    entry.target == target
                        && entry.original_source.producer() == producer
                        && entry.original_source.remote_file_origin() == source.remote_file_origin()
                })
        } else {
            exact
        };
        if let Some(old) = old {
            if old.original_source.producer() != producer
                || old.original_source.remote_file_origin() != source.remote_file_origin()
            {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            if matches!(operation, Operation::InstalledContent { .. })
                && old.original_source != source
            {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            if matches!(operation, Operation::Update) {
                if input.alias.is_none() {
                    input.alias = old.alias.clone();
                }
                if input.entry_name.is_none() {
                    input.entry_name = old.entry_name.clone();
                }
            }
        } else if matches!(
            operation,
            Operation::Update | Operation::InstalledContent { .. }
        ) {
            return Err(io::ErrorKind::NotFound.into());
        }
        old.cloned()
    };
    if let Operation::InstalledContent { path, .. } = operation {
        before()?;
        if !transport.verify_cache(&source, &cache, source.directory().as_path())? {
            return Err(io::ErrorKind::NotFound.into());
        }
        outcome["content"] = transport.read_text_file(
            source.directory().as_path(),
            path,
            state.policy.index,
            state.policy.content.max_output_bytes,
        )?;
        outcome["cache"] = "Verified".into();
        if let Some((paths, id, version)) = project
            && !state.live_projects.is_active(paths, *id, version)?
        {
            return Err(io::ErrorKind::NotFound.into());
        }
        let current = registry.load()?;
        let current = if input.global {
            current.get_version_for_consumer(
                &consumer,
                kind,
                source.id().as_str(),
                source.version(),
            )
        } else {
            current.get_for_consumer(&consumer, kind, source.id().as_str())
        };
        if current.and_then(|entry| {
            entry
                .installations
                .iter()
                .find(|entry| entry.target == target)
        }) != previous.as_ref()
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        before()?;
        return Ok(());
    }
    let resolved = resolved.as_ref().ok_or(io::ErrorKind::InvalidData)?;
    before()?;
    let held_parent =
        crate::native_install::parent(installation, policy, source.directory().as_path(), request)?;
    outcome["cache"] = "in_progress".into();
    let change = transport
        .cache(
            resolved,
            &cache,
            &source,
            source
                .directory()
                .as_path()
                .parent()
                .ok_or(io::ErrorKind::InvalidInput)?,
            policy,
        )
        .await?;
    outcome["cache"] = format!("{change:?}").into();
    before()?;
    held_parent.revalidate(request.snapshot())?;
    if let Some((paths, id, version)) = project
        && !state.live_projects.is_active(paths, *id, version)?
    {
        return Err(io::ErrorKind::NotFound.into());
    }
    if input.global {
        let mut writer = registry.writer(
            request
                .snapshot()
                .required_deadline()?
                .saturating_duration_since(Instant::now()),
            request.cancellation(),
        )?;
        if let Some(old) =
            writer
                .snapshot()
                .get_version(kind, source.id().as_str(), source.version())
            && old.installations.iter().any(|entry| {
                entry.original_source.producer() != producer
                    || entry.original_source.remote_file_origin() != source.remote_file_origin()
            })
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        if matches!(operation, Operation::Update)
            && let Some(previous) = &previous
            && writer
                .snapshot()
                .get_version(
                    kind,
                    source.id().as_str(),
                    previous.original_source.version(),
                )
                .and_then(|entry| {
                    entry
                        .installations
                        .iter()
                        .find(|entry| entry.target == target)
                })
                .is_none_or(|current| !current.same_source(previous))
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        before()?;
        let changed = writer.put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: kind,
            id: source.id().clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalCache,
                original_source: source,
                display: Some(ArtifactDisplayMetadata::new(
                    resolved.manifest.name(),
                    resolved.manifest.description(),
                    resolved.manifest.tags(),
                    policy.hub(),
                )?),
                alias: None,
                entry_name: None,
                backend: InstalledBackend::GlobalCache,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })?;
        outcome["publication"] = if changed {
            "MetadataOnly"
        } else {
            "VerifiedUnchanged"
        }
        .into();
        before()?;
        return Ok(());
    }
    scope.revalidate_bounded(&ScopeContext::from_snapshot(request.snapshot()))?;
    let core = ArtifactStore::new(scope, policy.artifacts());
    let coordinator = ScopedCoordinator::new(registry, core, root)?;
    let display = ArtifactDisplayMetadata::new(
        resolved.manifest.name(),
        resolved.manifest.description(),
        resolved.manifest.tags(),
        policy.hub(),
    )?;
    let expected = if matches!(operation, Operation::Update) {
        previous.as_ref()
    } else {
        None
    };
    let result = publish(
        state,
        ProjectionOptions {
            binding: input.binding,
            alias: input.alias.as_deref(),
            entry_name: input.entry_name.as_deref(),
            mode: input.mode,
        },
        &source,
        &coordinator,
        (Some(&display), expected),
        request,
        &before,
    );
    match result {
        Ok(publication) => {
            outcome["publication"] = format!("{:?}", publication.change).into();
            if publication.change == graphit_hub::publication::PublicationChange::Unsupported {
                return Err(io::ErrorKind::Unsupported.into());
            }
            outcome["possible_parents"] =
                serde_json::to_value(&publication.possible_parents).map_err(io::Error::other)?;
            if let Some(cleanup) = publication.cleanup_pending {
                return Err(cleanup_failure(cleanup));
            }
        }
        Err(error) => {
            if let Some(publication) = error.get_ref().and_then(|error| {
                error.downcast_ref::<graphit_hub::publication::PublicationError>()
            }) {
                outcome["publication"] = json!({"committed":publication.committed,"hub_pending":publication.hub_intent.is_some(),"core_pending":publication.core_pending.is_some()});
            }
            return Err(error);
        }
    }
    before()?;
    Ok(())
}

/// Projection fields shared by the local and remote source consumers.
pub(super) struct ProjectionOptions<'a> {
    pub binding: Option<graphit_hub::NativeBinding>,
    pub alias: Option<&'a str>,
    pub entry_name: Option<&'a str>,
    pub mode: Option<crate::remote_file_install::client::PhysicalMode>,
}

fn preview_failure(error: graphit_hub::publication::PublicationError) -> io::Error {
    use graphit_hub::publication::PublicationFailure;
    let kind = match &error.failure {
        PublicationFailure::Local(cause) => cause.kind(),
        PublicationFailure::Physical(cause) => cause.kind(),
        PublicationFailure::Projection(cause) => cause.kind(),
    };
    io::Error::new(kind, error)
}

pub(super) fn publish(
    state: &State,
    input: ProjectionOptions<'_>,
    source: &SourceRef,
    coordinator: &ScopedCoordinator,
    metadata: (Option<&ArtifactDisplayMetadata>, Option<&Installation>),
    request: &RequestContext<'_>,
    check: &dyn Fn() -> io::Result<()>,
) -> io::Result<PublicationOutcome> {
    let (display, expected) = metadata;
    check()?;
    let policy = &state.policy.native_install.hub;
    let binding = input.binding.unwrap_or(state.policy.binding);
    let projection = || ProjectionRequest {
        binding,
        source,
        alias: input.alias,
        entry_name: input.entry_name,
    };
    let mode = input
        .mode
        .map(|mode| mode.artifact_mode())
        .or_else(|| {
            expected.and_then(|installation| match &installation.backend {
                InstalledBackend::Physical { mode, .. } => Some(*mode),
                _ => None,
            })
        })
        .unwrap_or(ArtifactMode::Copy(CopyPolicy::RejectFileLinks));
    let writer = || -> io::Result<graphit_hub::publication::PublicationWriter> {
        check()?;
        let writer = coordinator.writer(request).map_err(io::Error::other)?;
        check()?;
        let target = InstallationTarget::for_artifact(binding, source.artifact_type());
        if let Some(old) = writer
            .snapshot()
            .get_for_consumer(
                &graphit_hub::registry::consumer_for_artifact(
                    *writer.consumer(),
                    source.artifact_type(),
                ),
                source.artifact_type(),
                source.id().as_str(),
            )
            .and_then(|artifact| {
                artifact
                    .installations
                    .iter()
                    .find(|entry| entry.target == target)
            })
            && (old.original_source.producer() != source.producer()
                || old.original_source.remote_file_origin() != source.remote_file_origin()
                || (source.origin() == graphit_hub::SourceOrigin::LocalLink
                    && (old.original_source.origin() != source.origin()
                        || old.original_source.directory() != source.directory())))
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        if let Some(expected) = expected
            && writer
                .snapshot()
                .get_for_consumer(
                    &graphit_hub::registry::consumer_for_artifact(
                        *writer.consumer(),
                        source.artifact_type(),
                    ),
                    source.artifact_type(),
                    source.id().as_str(),
                )
                .and_then(|artifact| {
                    artifact
                        .installations
                        .iter()
                        .find(|entry| entry.target == target)
                })
                .is_none_or(|current| !current.same_source(expected))
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        if input.mode.is_some()
            && !matches!(
                writer
                    .preview(projection(), mode)
                    .map_err(|error| preview_failure(*error))?,
                graphit_hub::Projection::Physical(_)
            )
        {
            return Err(io::ErrorKind::Unsupported.into());
        }
        Ok(writer)
    };

    if matches!(
        source.artifact_type(),
        ArtifactType::Command | ArtifactType::Workflow
    ) {
        let mut writer = writer()?;
        let preview = writer
            .preview(projection(), mode)
            .map_err(|error| preview_failure(*error))?;
        let graphit_hub::Projection::Physical(selected) = preview else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        let entry = selected
            .selected()
            .input()
            .as_path()
            .strip_prefix(source.directory().as_path())
            .map_err(io::Error::other)?;
        let snapshot = SourceSnapshot::capture(
            source.directory().as_path(),
            graphit_fs::NodeKind::Directory,
            &policy.snapshot_policy(CopyPolicy::RejectFileLinks, [entry.to_owned()])?,
            request.snapshot(),
        )?;
        let bytes = snapshot.retained(entry).ok_or(io::ErrorKind::NotFound)?;
        let rendered = templates::render(
            state,
            source,
            binding,
            entry.to_str().ok_or(io::ErrorKind::InvalidData)?,
            bytes,
            request,
        )?;
        let file_mode = snapshot
            .nodes()
            .find(|node| node.relative() == entry)
            .ok_or(io::ErrorKind::NotFound)?
            .mode();
        let root_mode = snapshot
            .nodes()
            .next()
            .ok_or(io::ErrorKind::InvalidData)?
            .mode();
        let selection = PayloadSelection::new(entry.to_owned(), graphit_fs::NodeKind::File)?;
        let plan = ValidatedPayloadPlan::new(
            snapshot,
            [PayloadOverlay::new(
                entry.to_owned(),
                entry.to_owned(),
                rendered.into_bytes(),
                file_mode,
            )?],
            &PayloadPolicy::new(
                graphit_fs::NodeKind::Directory,
                root_mode,
                policy.materialization().snapshots,
            ),
            request.snapshot(),
        )?;
        let derived = DerivedArtifact::new(
            source.clone(),
            plan,
            binding.schema(),
            selected.name(),
            (selection.clone(), selection),
            policy.materialization().lineage,
        )?;
        check()?;
        return writer
            .publish_derived(projection(), mode, derived, display)
            .map_err(io::Error::other);
    }
    if !matches!(
        source.artifact_type(),
        ArtifactType::Agent | ArtifactType::Skill
    ) {
        let mut writer = writer()?;
        check()?;
        return writer
            .publish(projection(), mode, display)
            .map_err(io::Error::other);
    }
    let mut writer = writer()?;
    let preview = writer
        .preview(projection(), mode)
        .map_err(|error| preview_failure(*error))?;
    let graphit_hub::Projection::Physical(selected) = preview else {
        return Err(io::ErrorKind::InvalidInput.into());
    };
    let entry_path = if source.artifact_type() == ArtifactType::Agent {
        selected
            .selected()
            .input()
            .as_path()
            .strip_prefix(source.directory().as_path())
            .map_err(io::Error::other)?
            .to_owned()
    } else {
        PathBuf::from("SKILL.md")
    };
    let entry = entry_path.to_str().ok_or(io::ErrorKind::InvalidData)?;
    let snapshot = SourceSnapshot::capture(
        source.directory().as_path(),
        graphit_fs::NodeKind::Directory,
        &policy.snapshot_policy(CopyPolicy::RejectFileLinks, [PathBuf::from(entry)])?,
        request.snapshot(),
    )?;
    let mut renderer = RenderContext::new(
        policy.content(),
        request.cancellation(),
        request.snapshot().required_deadline()?,
    )
    .map_err(io::Error::other)?;
    let bytes = snapshot
        .retained(Path::new(entry))
        .ok_or(io::ErrorKind::NotFound)?;
    let instruction = templates::render(state, source, binding, entry, bytes, request)?;
    let bytes = instruction.as_bytes();
    let rendered = if source.artifact_type() == ArtifactType::Agent {
        let descriptor = AgentDescriptor::parse(bytes, &mut renderer).map_err(io::Error::other)?;
        let grants = ToolGrants::native(binding, descriptor.read_only(), descriptor.allows_shell());
        render_agent(
            binding,
            &descriptor,
            Some(selected.name()),
            &grants,
            &mut renderer,
        )
        .map_err(io::Error::other)?
    } else {
        let descriptor = SkillDescriptor::parse(bytes, &mut renderer).map_err(io::Error::other)?;
        render_skill(binding, &descriptor, Some(selected.name()), &mut renderer)
            .map_err(io::Error::other)?
    };
    let root_mode = snapshot
        .nodes()
        .next()
        .ok_or(io::ErrorKind::InvalidData)?
        .mode();
    let file_mode = snapshot
        .nodes()
        .find(|node| node.relative() == Path::new(entry))
        .ok_or(io::ErrorKind::NotFound)?
        .mode();
    let output = if source.artifact_type() == ArtifactType::Agent {
        "rendered-agent"
    } else {
        "SKILL.md"
    };
    let selection = if source.artifact_type() == ArtifactType::Agent {
        (
            PayloadSelection::new(entry.into(), graphit_fs::NodeKind::File)?,
            PayloadSelection::new(output.into(), graphit_fs::NodeKind::File)?,
        )
    } else {
        (
            PayloadSelection::new(PathBuf::new(), graphit_fs::NodeKind::Directory)?,
            PayloadSelection::new(PathBuf::new(), graphit_fs::NodeKind::Directory)?,
        )
    };
    let plan = ValidatedPayloadPlan::new(
        snapshot,
        [PayloadOverlay::new(
            entry.into(),
            output.into(),
            rendered.bytes,
            file_mode,
        )?],
        &PayloadPolicy::new(
            graphit_fs::NodeKind::Directory,
            root_mode,
            policy.materialization().snapshots,
        ),
        request.snapshot(),
    )?;
    let derived = DerivedArtifact::new(
        source.clone(),
        plan,
        binding.schema(),
        &rendered.provenance.effective_name,
        selection,
        policy.materialization().lineage,
    )?;
    check()?;
    writer
        .publish_derived(projection(), mode, derived, display)
        .map_err(io::Error::other)
}

fn failure(code: &str) -> CallToolResult {
    crate::mcp_output::typed_error(json!({"status":"error","code":code}))
}
fn operation_failure(error: io::Error) -> CallToolResult {
    let local = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<FileEffectFailure>())
        .is_some_and(|effect| effect.outcome["source"] == "local");
    let code = match error.kind() {
        io::ErrorKind::PermissionDenied => "authentication_or_permission_denied",
        io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => "request_cancelled_or_expired",
        io::ErrorKind::InvalidInput => "invalid_arguments",
        io::ErrorKind::AlreadyExists => "installation_conflict",
        io::ErrorKind::InvalidData if local => "invalid_local_artifact",
        io::ErrorKind::InvalidData => "invalid_remote_file",
        io::ErrorKind::FileTooLarge if local => "local_artifact_too_large",
        io::ErrorKind::FileTooLarge => "remote_file_too_large",
        io::ErrorKind::NotFound => "artifact_not_found",
        _ => "file_installation_failed",
    };
    if let Some(effect) = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<FileEffectFailure>())
    {
        return crate::mcp_output::typed_error(
            json!({"status":"error","code":code,"outcome":effect.outcome}),
        );
    }
    failure(code)
}
pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let name = call.name;
    let input = Value::Object(call.arguments.unwrap_or_default());
    if serde_json::to_vec(&input).map_or(true, |bytes| {
        bytes.len() > server.0.policy.content.max_input_bytes
    }) {
        return failure("invalid_arguments");
    }
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure("incoming_credentials_required");
    };
    if parts.headers.contains_key(axum::http::header::ORIGIN) {
        return failure("browser_origin_not_allowed");
    }
    if parts
        .headers
        .get_all(axum::http::header::AUTHORIZATION)
        .iter()
        .count()
        != 1
        || parts
            .headers
            .get_all("x-graphit-auth-origin")
            .iter()
            .count()
            != 1
    {
        return failure("ambiguous_or_missing_credentials");
    }
    if parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|value| value.to_str().ok())
        != Some("runtime")
    {
        return failure("runtime_management_required");
    }
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
    else {
        return failure("credential_required");
    };
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let handle = Handle::current();
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! { jobs=state.jobs.lock()=>jobs, ()=context.ct.cancelled()=>return failure("request_cancelled_or_expired"), ()=state.stop.cancelled()=>return failure("stopped"), ()=tokio::time::sleep_until(deadline.into())=>return failure("request_cancelled_or_expired") };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure("stopped");
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = handle.block_on(async {
                let request = RequestContext::new(owner.policy.content, &original_cancel, deadline)
                    .map_err(io::Error::other)?;
                execute(&owner, &name, input, &credential, &request).await
            });
            let _ = send.send(result);
        });
    }
    tokio::select! { result=receive=>match result {Ok(Ok(payload))=>crate::mcp_output::typed(payload),Ok(Err(error))=>operation_failure(error),Err(_)=>failure("file_job_failed")}, ()=context.ct.cancelled()=>failure("request_cancelled_or_expired"), ()=state.stop.cancelled()=>failure("stopped"), ()=tokio::time::sleep_until(deadline.into())=>failure("request_cancelled_or_expired") }
}
