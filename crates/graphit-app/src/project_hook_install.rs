//! PROJECT hook publication through actual daemon authority and the shared native writer.
use crate::managed_roles::InstallResources;
use graphit_auth::{ExternalEffect, Permission};
use graphit_core::{
    artifact_store::{ArtifactStore, CapturedPublicationScope, ScopeContext},
    native_document_file::CapturedDocumentDirectory,
};
use graphit_hub::{
    NativeBinding,
    native::{
        ContentError, ContentLimits, DocumentError, RenderContext,
        inspect_kiro_coordinator_profile,
        kiro_hooks::{
            KiroHookAction, KiroHookError, KiroHookInput, KiroHookTrigger, plan_kiro_hook,
        },
        shared_document::SharedDocumentStore,
    },
    policy::RequestContext,
};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    fs, io,
    path::{Component, Path, PathBuf},
};

pub mod client;
pub(crate) mod native_publication;
pub const ROUTE: &str = "/admin/hooks";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookOperation {
    Install,
    Remove,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HookRequest {
    pub operation: HookOperation,
    pub binding: NativeBinding,
    pub profile: Option<String>,
    pub project_dir: PathBuf,
    pub kimi_config_dir: Option<PathBuf>,
    pub project_version: Option<String>,
}
impl HookRequest {
    pub(crate) fn validate(&self) -> io::Result<()> {
        match (self.binding, self.profile.as_deref()) {
            (NativeBinding::KiroV3, Some(profile)) => normal_profile(profile)?,
            (NativeBinding::KiroV3, None) | (_, Some(_)) => {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            (_, None) => {}
        }
        if (self.binding == NativeBinding::Kimi) != self.kimi_config_dir.is_some()
            || self.kimi_config_dir.as_ref().is_some_and(|path| {
                !path.is_absolute()
                    || path
                        .components()
                        .any(|part| matches!(part, Component::ParentDir))
            })
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }
}
pub(crate) fn hook_engine(binding: NativeBinding) -> crate::session_hook_entry::Engine {
    crate::session_hook_entry::Engine::ALL
        .into_iter()
        .find(|engine| engine.binding() == binding)
        .expect("closed native host binding")
}
pub(crate) fn context_events(
    binding: NativeBinding,
) -> &'static [crate::session_hook_entry::Event] {
    use crate::session_hook_entry::Event::*;
    match binding {
        NativeBinding::Codex
        | NativeBinding::Claude
        | NativeBinding::Qwen
        | NativeBinding::KiroV3 => &[SessionStart, UserPromptSubmit],
        NativeBinding::Cursor => &[SessionStart],
        NativeBinding::Kimi => &[UserPromptSubmit],
        NativeBinding::Antigravity => &[PreInvocation],
        NativeBinding::OpenCode => &[SystemTransform, SessionCompacting],
    }
}

pub(crate) fn hook_events(
    binding: NativeBinding,
) -> impl Iterator<Item = crate::session_hook_entry::Event> {
    use crate::session_hook_entry::Event;
    context_events(binding).iter().copied().chain(
        [Event::Stop, Event::SessionEnd]
            .into_iter()
            .filter(move |event| event.semantic().native_name(binding).is_ok()),
    )
}
pub(crate) fn kiro_plans(
    resources: &InstallResources<'_>,
    input: &HookRequest,
    paths: &graphit_core::project_paths::ProjectPaths,
    original: &RequestContext<'_>,
) -> io::Result<Vec<graphit_hub::native::kiro_hooks::KiroHookPlan>> {
    use crate::session_hook_entry::{Event, Recipient};
    let mut plans = Vec::new();
    for (event, suffix, trigger) in [
        (
            Event::SessionStart,
            "session-context",
            KiroHookTrigger::SessionStart,
        ),
        (
            Event::UserPromptSubmit,
            "prompt-context",
            KiroHookTrigger::UserPromptSubmit,
        ),
        (Event::Stop, "stop-sync", KiroHookTrigger::Stop),
        (
            Event::SessionEnd,
            "session-end-sync",
            KiroHookTrigger::SessionEnd,
        ),
    ] {
        let command = client::native_command(
            &resources.policy.installation,
            Some(paths),
            input.project_version.as_deref(),
            hook_engine(input.binding),
            event,
            Recipient::Coordinator,
        )?;
        plans.push(
            plan_kiro_hook(
                input.binding,
                KiroHookInput {
                    identifier: &format!("{}-{suffix}", paths.brand().key()),
                    trigger,
                    matcher: None,
                    action: KiroHookAction::Command(&command),
                    timeout: Some(resources.policy.timeout),
                },
                resources.policy.native_install.hub.shared(),
                original,
            )
            .map_err(plan_error)?,
        );
    }
    Ok(plans)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HookEffect {
    pub event: String,
    pub change: String,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HookOutcome {
    pub mcp: Option<String>,
    pub hooks: Vec<HookEffect>,
    pub failure: Option<String>,
}
pub struct HookExecution {
    pub outcome: HookOutcome,
    pub cause: Option<io::Error>,
}

pub fn execute(
    resources: InstallResources<'_>,
    credential: &str,
    input: &HookRequest,
    original: &RequestContext<'_>,
) -> HookExecution {
    let mut outcome = HookOutcome::default();
    let result = (|| -> io::Result<()> {
        original.snapshot().required_deadline()?;
        original.snapshot().check()?;
        input.validate()?;
        let caller = resources
            .auth
            .issuer()
            .authenticate_runtime(credential, original)
            .map_err(|error| {
                private_error(
                    io::ErrorKind::PermissionDenied,
                    "hook installation authority denied",
                    error,
                )
            })?;
        let session = resources
            .auth
            .issuer()
            .bootstrap_admin(&caller)
            .map_err(|error| {
                private_error(
                    io::ErrorKind::PermissionDenied,
                    "hook installation authority denied",
                    error,
                )
            })?;
        let parent = resources
            .auth
            .issuer()
            .admin_parent(&caller, &session)
            .map_err(|error| {
                private_error(
                    io::ErrorKind::PermissionDenied,
                    "hook installation authority denied",
                    error,
                )
            })?;
        parent.authorize(Permission::Configure).map_err(|error| {
            private_error(
                io::ErrorKind::PermissionDenied,
                "hook installation authority denied",
                error,
            )
        })?;
        parent
            .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
            .map_err(|error| {
                private_error(
                    io::ErrorKind::PermissionDenied,
                    "hook installation authority denied",
                    error,
                )
            })?;
        // Retained Runtime tuples select the scope; a request path supplies no authority.
        let selection = crate::managed_roles::InstallRequest {
            destination: crate::managed_roles::InstallDestination::Project,
            project_dir: Some(input.project_dir.clone()),
            project_version: input.project_version.clone(),
            binding: None,
            roles: Vec::new(),
            skills_only: false,
            rules: Vec::new(),
            skill_aliases: Default::default(),
            skill_bundles: Default::default(),
            module_skill: None,
        };
        let (paths, id, version) = crate::managed_roles::select_project(
            resources.projects,
            resources.current,
            &selection,
        )?;
        if !resources
            .auth
            .installation()
            .live
            .allowed_roots()
            .any(|root| root == paths.root())
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let scope = CapturedPublicationScope::project(
            paths,
            *id,
            resources.policy.scope,
            original.snapshot(),
        )?;
        if version.scope() != scope.owner_root() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let context = ScopeContext::from_snapshot(original.snapshot());
        let before = || -> io::Result<()> {
            parent.authorize(Permission::Configure).map_err(|error| {
                private_error(
                    io::ErrorKind::PermissionDenied,
                    "hook installation authority denied",
                    error,
                )
            })?;
            parent
                .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
                .map_err(|error| {
                    private_error(
                        io::ErrorKind::PermissionDenied,
                        "hook installation authority denied",
                        error,
                    )
                })?;
            scope.revalidate_bounded(&context)?;
            original.snapshot().check()
        };
        before()?;
        let hub = &resources.policy.native_install.hub;
        let core = ArtifactStore::new(scope.clone(), hub.artifacts());
        if input.binding != NativeBinding::KiroV3 {
            let mcp = (input.operation == HookOperation::Remove)
                .then(|| {
                    graphit_hub::native::mcp_config::NativeServerSpec::graphit(paths.brand().key())
                        .plan(input.binding)
                })
                .transpose()
                .map_err(io::Error::other)?;
            outcome.mcp = native_publication::publish(
                &resources,
                input,
                (paths, id, &scope),
                mcp.as_ref(),
                original,
                &before,
                &mut outcome,
            )?;
            return Ok(());
        }
        let binding = input.binding;
        let name = input
            .profile
            .as_deref()
            .ok_or(io::ErrorKind::InvalidInput)?;
        let plans = kiro_plans(&resources, input, paths, original)?;
        // Retain every existing PROJECT component before reading the profile.
        let components = [".kiro", "agents"];
        let root = CapturedDocumentDirectory::capture(
            paths.root(),
            resources.policy.index,
            original.snapshot(),
        )?;
        let mut directories = Vec::new();
        directories.try_reserve_exact(3).map_err(io::Error::other)?;
        directories.push(root);
        let mut target = directories[0].canonical_path().to_owned();
        for component in components {
            before()?;
            for directory in &directories {
                directory.revalidate(original.snapshot())?;
            }
            let child = target.join(component);
            match fs::symlink_metadata(&child) {
                Ok(meta) if meta.is_dir() && !meta.is_symlink() => {}
                Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
                Err(e) => return Err(e),
            }
            let captured = CapturedDocumentDirectory::capture(
                &child,
                resources.policy.index,
                original.snapshot(),
            )?;
            if captured.canonical_path() != child {
                return Err(io::ErrorKind::InvalidData.into());
            }
            for directory in &directories {
                directory.revalidate(original.snapshot())?;
            }
            before()?;
            target = child;
            directories.push(captured);
        }
        for directory in &directories {
            directory.revalidate(original.snapshot())?;
        }
        let held = &directories[2];
        let observed = held.read(OsStr::new(name), original.snapshot())?;
        validate_coordinator(
            observed.bytes().ok_or(io::ErrorKind::NotFound)?,
            name,
            &core,
            hub.content(),
            original,
        )?;
        before()?;
        held.revalidate_observation(&observed, original.snapshot())?;
        before()?;
        // A product namespace common to every brand, inside the authorized PROJECT.
        for directory in &directories {
            directory.revalidate(original.snapshot())?;
        }
        let shared = SharedDocumentStore::open(
            &directories[1]
                .canonical_path()
                .join(".graphit-shared-documents"),
        )?;
        for directory in &directories {
            directory.revalidate(original.snapshot())?;
        }
        before()?;
        let mut writer = shared.writer(held, name, &core, hub.shared(), original)?;
        held.revalidate_observation(&observed, original.snapshot())?;
        outcome
            .hooks
            .try_reserve_exact(plans.len())
            .map_err(io::Error::other)?;
        for plan in &plans {
            before()?;
            let change = match input.operation {
                HookOperation::Install => writer.sync_kiro_profile_hook(plan, hub.content())?,
                HookOperation::Remove => writer.remove_kiro_profile_hook(plan, hub.content())?,
            };
            outcome.hooks.push(HookEffect {
                event: plan.native_trigger().into(),
                change: format!("{change:?}"),
            });
            before()?;
        }
        drop(writer);
        if input.operation == HookOperation::Remove {
            let mcp =
                graphit_hub::native::mcp_config::NativeServerSpec::graphit(paths.brand().key())
                    .plan(binding)
                    .map_err(io::Error::other)?;
            outcome.mcp = native_publication::publish_mcp(
                &resources,
                input,
                paths.root(),
                &core,
                Some(&mcp),
                original,
                &before,
            )?;
        }
        Ok(())
    })();
    let cause = result.err();
    if let Some(error) = &cause {
        outcome.failure = Some(
            match error.kind() {
                io::ErrorKind::PermissionDenied => "hook installation denied",
                io::ErrorKind::NotFound => "hook target or captured project missing",
                io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => {
                    "hook installation input or native document invalid"
                }
                io::ErrorKind::AlreadyExists => "hook ownership collision",
                io::ErrorKind::Interrupted => "hook installation cancelled",
                io::ErrorKind::TimedOut => "hook installation deadline elapsed",
                io::ErrorKind::FileTooLarge => "hook installation exceeds resource bounds",
                _ => "hook native publication failed",
            }
            .into(),
        );
    }
    HookExecution { outcome, cause }
}
fn normal_profile(name: &str) -> io::Result<()> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || name.contains(['\0', '/', '\\'])
        || !name.ends_with(".md")
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
fn validate_coordinator(
    bytes: &[u8],
    file: &str,
    core: &ArtifactStore,
    limits: ContentLimits,
    original: &RequestContext<'_>,
) -> io::Result<()> {
    let name = file
        .strip_suffix(".md")
        .ok_or(io::ErrorKind::InvalidInput)?;
    let mut context = RenderContext::new(
        limits,
        original.cancellation(),
        original.snapshot().required_deadline()?,
    )
    .map_err(content_error)?;
    inspect_kiro_coordinator_profile(bytes, name, &mut context).map_err(content_error)?;
    if core.load().map_err(io::Error::other)?.is_some_and(|state| {
        state.entries().iter().any(|entry| {
            (entry.key().artifact_type() == "managed-role" && entry.key().id() == name)
                || entry.destination() == core.scope().owner_root().join(".kiro/agents").join(file)
        })
    }) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}
fn content_error(error: ContentError) -> io::Error {
    let kind = match error {
        ContentError::Cancelled => io::ErrorKind::Interrupted,
        ContentError::TimedOut => io::ErrorKind::TimedOut,
        ContentError::LimitExceeded => io::ErrorKind::FileTooLarge,
        ContentError::UnsupportedCapability => io::ErrorKind::PermissionDenied,
        _ => io::ErrorKind::InvalidData,
    };
    private_error(kind, "hook native profile invalid", error)
}

struct PrivateCause {
    label: &'static str,
    cause: Box<dyn std::error::Error + Send + Sync>,
}
impl std::fmt::Debug for PrivateCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookFailure")
            .field("category", &self.label)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for PrivateCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}
impl std::error::Error for PrivateCause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.cause)
    }
}
pub(crate) fn private_error(
    kind: io::ErrorKind,
    label: &'static str,
    cause: impl std::error::Error + Send + Sync + 'static,
) -> io::Error {
    io::Error::new(
        kind,
        PrivateCause {
            label,
            cause: Box::new(cause),
        },
    )
}

fn plan_error(error: KiroHookError) -> io::Error {
    let kind = match error {
        KiroHookError::Document(DocumentError::Interrupted) => io::ErrorKind::Interrupted,
        KiroHookError::Document(DocumentError::TimedOut) => io::ErrorKind::TimedOut,
        KiroHookError::Document(DocumentError::LimitExceeded) => io::ErrorKind::FileTooLarge,
        KiroHookError::UnsupportedBinding
        | KiroHookError::UnsupportedAction
        | KiroHookError::UnsupportedMatcher => io::ErrorKind::Unsupported,
        _ => io::ErrorKind::InvalidInput,
    };
    private_error(kind, "hook native plan invalid", error)
}
