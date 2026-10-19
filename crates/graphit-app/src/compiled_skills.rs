//! Materialize the maintained skill bodies and companions through the Core owner.
use crate::{
    managed_roles::{
        InstallDestination, InstallRequest, InstallResources, TemplateObservation, capture_template,
    },
    source_server::SourcePolicy,
};
use graphit_auth::{AdminParent, Permission};
use graphit_core::{
    artifact_store::{
        ArtifactError, ArtifactKey, ArtifactMode, ArtifactStore, CapturedPublicationScope,
        CopyPolicy, GeneratedNode, PayloadMode, PayloadPolicy, PublicationSlot, ScopeContext,
        ValidatedPayloadPlan,
    },
    module_settings::Module,
    native_document_file::CapturedDocumentDirectory,
    project_identity::ProjectIdentityStore,
    project_paths::ProjectPaths,
};
use graphit_hub::{
    AdapterId, ArtifactType, ConcreteVersion, InstallationTarget,
    native::{RenderContext, SkillDescriptor, render_skill},
    policy::RequestContext,
    registry::{HubRegistry, Installation},
};
use graphit_runtime::{
    instructions::{
        CapturedInstruction, InstructionOrigin, InstructionValue, InstructionVariable,
        render_instruction,
    },
    skill_instructions::skill_templates,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Component, Path, PathBuf},
};

pub mod remote;

const MODULES: [Module; 5] = [
    Module::Task,
    Module::Memory,
    Module::Ast,
    Module::Knowledge,
    Module::Hub,
];
const DOMAIN: &str = "module-skill";
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HubSelection {
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_provider: Option<String>,
}
#[derive(Deserialize, Serialize)]
pub struct SkillEffect {
    pub module: String,
    pub name: String,
    pub path: String,
    pub origins: BTreeMap<String, String>,
    pub publication: Option<String>,
    pub pending: Option<bool>,
    pub observation_failed: bool,
}
pub(crate) struct SkillExecution {
    pub effects: Vec<SkillEffect>,
    pub cause: Option<io::Error>,
}
struct Compiled {
    nodes: Vec<GeneratedNode>,
    relative: PathBuf,
    origins: BTreeMap<String, String>,
    observed: Vec<TemplateObservation>,
    selected: Option<(HubRegistry, HubSelection, Installation, PublicationSlot)>,
}
impl Compiled {
    fn revalidate(
        &self,
        policy: &SourcePolicy,
        parent: &AdminParent<'_>,
        original: &RequestContext<'_>,
    ) -> io::Result<()> {
        for item in &self.observed {
            item.revalidate(original)?;
        }
        if let Some((registry, selection, admitted, consumer)) = &self.selected {
            parent
                .authorize(Permission::Read(Module::Hub))
                .map_err(io::Error::other)?;
            let version =
                ConcreteVersion::new(&selection.version, policy.native_install.hub.projection())
                    .map_err(io::Error::other)?;
            let document = registry.load()?;
            let still_selected = document
                .get_version_for_consumer(consumer, ArtifactType::Skill, &selection.id, &version)
                .is_some_and(|entry| {
                    entry
                        .installations
                        .iter()
                        .any(|item| item.target == admitted.target && item.same_source(admitted))
                });
            if !still_selected {
                return Err(io::ErrorKind::Interrupted.into());
            }
        }
        original.check()
    }
}

pub(crate) fn publish(
    resources: &InstallResources<'_>,
    input: &InstallRequest,
    store: &ArtifactStore,
    parent: &AdminParent<'_>,
    original: &RequestContext<'_>,
    docs_dir: Option<&str>,
    native: Option<(&remote::NativeSkillTransport, &str)>,
) -> SkillExecution {
    let policy = resources.policy;
    let scope_context = ScopeContext::from_snapshot(original.snapshot());
    let before = || -> io::Result<()> {
        for permission in [
            Permission::Configure,
            Permission::External(graphit_auth::ExternalEffect::PublishedFilesystem),
        ] {
            parent.authorize(permission).map_err(io::Error::other)?;
        }
        store.scope().revalidate_bounded(&scope_context)?;
        original.check()
    };
    let mut effects = Vec::new();
    let result = (|| -> io::Result<()> {
        before()?;
        let source = crate::managed_roles::instruction_project(
            resources.projects,
            resources.current,
            input,
        )?;
        let paths = source.map(|(paths, _, _)| paths);
        let brand = policy.installation.brand();
        let binding = input.binding.unwrap_or(policy.binding);
        let global = if input.destination == InstallDestination::Global {
            Some(
                resources
                    .global_paths
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            "captured native global directories unavailable",
                        )
                    })?
                    .directories(binding)?,
            )
        } else {
            None
        };
        let target_root = if let Some(global) = &global {
            global.skills.as_path()
        } else {
            paths.ok_or(io::ErrorKind::InvalidInput)?.root()
        };
        let expected_owner = if global.is_some() {
            policy.installation.root()
        } else {
            paths.ok_or(io::ErrorKind::InvalidInput)?.root()
        };
        if store.scope().owner_root() != expected_owner
            || store.scope().brand() != brand
            || input
                .skill_aliases
                .keys()
                .chain(input.skill_bundles.keys())
                .any(|name| !MODULES.iter().any(|m| m.name() == name))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let names = MODULES
            .iter()
            .map(|m| {
                input
                    .skill_aliases
                    .get(m.name())
                    .cloned()
                    .map(Ok)
                    .unwrap_or_else(|| brand.skill_name(m.name()))
            })
            .collect::<io::Result<Vec<_>>>()?;
        if names.iter().collect::<BTreeSet<_>>().len() != names.len() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        for (module, name) in MODULES.into_iter().zip(names) {
            before()?;
            if input.skill_bundles.contains_key(module.name()) {
                parent
                    .authorize(Permission::Read(Module::Hub))
                    .map_err(io::Error::other)?;
            }
            let selected = select_bundle(policy, paths, input, module, original)?;
            let remote_source = selected.as_ref().and_then(|(_, _, installed, _)| {
                installed
                    .original_source
                    .remote_file_origin()
                    .map(|_| installed.original_source.clone())
            });
            let publication =
                |origin_check: Option<&dyn Fn() -> io::Result<()>>| -> io::Result<()> {
                    let admitted = || -> io::Result<()> {
                        before()?;
                        if let Some(check) = origin_check {
                            check()?;
                        }
                        Ok(())
                    };
                    admitted()?;
                    let mut compiled = compile(
                        resources, input, module, &name, original, docs_dir, selected,
                    )?;
                    let nodes = std::mem::take(&mut compiled.nodes);
                    let (namespace, relative) = if global.is_some() {
                        let index = store
                            .scope()
                            .allowed_roots()
                            .position(|root| root == target_root)
                            .ok_or(io::ErrorKind::PermissionDenied)?;
                        (format!("global-native-{index}"), PathBuf::from(&name))
                    } else {
                        let root = compiled
                            .relative
                            .components()
                            .next()
                            .ok_or(io::ErrorKind::InvalidData)?;
                        let adapter = AdapterId::ALL
                            .into_iter()
                            .find(|adapter| {
                                Path::new(adapter.layout().root) == Path::new(root.as_os_str())
                            })
                            .ok_or(io::ErrorKind::InvalidData)?;
                        (
                            InstallationTarget::SharedAdapter { adapter }
                                .namespace()
                                .to_owned(),
                            compiled.relative.clone(),
                        )
                    };
                    let key = ArtifactKey::new(&namespace, DOMAIN, module.name())?;
                    let parent_path =
                        target_root.join(relative.parent().ok_or(io::ErrorKind::InvalidInput)?);
                    let held =
                        capture_parent(&parent_path, target_root, policy, original, &admitted)?;
                    let check = || -> io::Result<()> {
                        admitted()?;
                        compiled.revalidate(policy, parent, original)?;
                        for directory in &held {
                            directory.revalidate(original.snapshot())?;
                        }
                        original.check()
                    };
                    check()?;
                    let mode = payload_modes(target_root)?.0;
                    let plan = ValidatedPayloadPlan::generated(
                        nodes,
                        &PayloadPolicy::new(
                            graphit_fs::NodeKind::Directory,
                            mode,
                            store.limits().copy_limits(),
                        ),
                        original.snapshot(),
                    )?;
                    // Only the current selected key may be recovered by this operation.
                    let mut writer = store
                        .writer_with_context(original.snapshot())
                        .map_err(artifact_error)?;
                    if writer.load().map_err(artifact_error)?.is_some_and(|s| {
                        s.pending()
                            .is_some_and(|pending| pending.key() != Some(&key))
                    }) {
                        return Err(io::ErrorKind::WouldBlock.into());
                    }
                    writer.recover().map_err(artifact_error)?;
                    check()?;
                    effects.push(SkillEffect {
                        module: module.name().into(),
                        name,
                        path: if global.is_some() {
                            target_root.join(&relative).to_string_lossy().into_owned()
                        } else {
                            relative.to_string_lossy().into_owned()
                        },
                        origins: compiled.origins.clone(),
                        publication: None,
                        pending: None,
                        observation_failed: false,
                    });
                    let effect = effects.last_mut().expect("effect inserted");
                    let publication = (|| -> io::Result<()> {
                        let change = writer
                            .publish_generated(
                                &key,
                                &plan,
                                &target_root.join(&relative),
                                graphit_fs::NodeKind::Directory,
                                ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
                            )
                            .map_err(artifact_error)?;
                        effect.publication = Some(format!("{change:?}"));
                        check()?;
                        check()
                    })();
                    if check().is_ok() {
                        match writer.load() {
                            Ok(snapshot) => {
                                effect.pending =
                                    Some(snapshot.is_some_and(|s| s.pending().is_some()))
                            }
                            Err(_) => effect.observation_failed = true,
                        }
                    } else {
                        effect.observation_failed = true;
                    }
                    publication.map_err(|cause| {
                        io::Error::new(
                            cause.kind(),
                            SkillPublicationError {
                                module: module.name(),
                                relative: relative.clone(),
                                cause,
                            },
                        )
                    })
                };
            if let Some(source) = remote_source {
                let (native, credential) = native.ok_or(io::ErrorKind::NotConnected)?;
                let selection = input
                    .skill_bundles
                    .get(module.name())
                    .ok_or(io::ErrorKind::InvalidData)?;
                native.with_source(
                    resources,
                    credential,
                    selection,
                    &source,
                    original,
                    |check| publication(Some(check)),
                )?;
            } else {
                publication(None)?;
            }
        }
        before()
    })();
    SkillExecution {
        effects,
        cause: result.err(),
    }
}

fn select_bundle(
    policy: &SourcePolicy,
    paths: Option<&ProjectPaths>,
    input: &InstallRequest,
    module: Module,
    original: &RequestContext<'_>,
) -> io::Result<Option<(HubRegistry, HubSelection, Installation, PublicationSlot)>> {
    let Some(selection) = input.skill_bundles.get(module.name()) else {
        return Ok(None);
    };
    let version = ConcreteVersion::new(&selection.version, policy.native_install.hub.projection())
        .map_err(io::Error::other)?;
    let (registry, consumer) = if let Some(paths) = paths {
        let identity = ProjectIdentityStore::at(paths)
            .load()?
            .ok_or(io::ErrorKind::NotFound)?;
        let scope = CapturedPublicationScope::project(
            paths,
            identity.id(),
            policy.scope,
            original.snapshot(),
        )?;
        (
            HubRegistry::project(paths, &identity, policy.native_install.hub.hub())?,
            scope.publication_slot(),
        )
    } else {
        (policy.registry.clone(), PublicationSlot::Scope)
    };
    let document = registry.load()?;
    let artifact = document
        .get_version_for_consumer(&consumer, ArtifactType::Skill, &selection.id, &version)
        .ok_or(io::ErrorKind::NotFound)?;
    let mut matching = artifact.installations.iter().filter(|item| {
        item.target
            .includes(input.binding.unwrap_or(policy.binding))
    });
    let installed = matching.next().ok_or(io::ErrorKind::NotFound)?;
    if matching.next().is_some() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    if selection.identity_provider.is_some()
        && !matches!(
            installed.original_source.remote_file_origin(),
            Some(graphit_hub::RemoteFileOrigin::Standalone { .. })
        )
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(Some((
        registry,
        selection.clone(),
        installed.clone(),
        consumer,
    )))
}

fn compile(
    resources: &InstallResources<'_>,
    input: &InstallRequest,
    module: Module,
    name: &str,
    original: &RequestContext<'_>,
    docs_dir: Option<&str>,
    selected: Option<(HubRegistry, HubSelection, Installation, PublicationSlot)>,
) -> io::Result<Compiled> {
    let policy = resources.policy;
    let paths =
        crate::managed_roles::instruction_project(resources.projects, resources.current, input)?
            .map(|(paths, _, _)| paths);
    let brand = policy.installation.brand();
    original.check()?;
    let set = skill_templates(module).map_err(io::Error::other)?;
    let binding = input.binding.unwrap_or(policy.binding);
    let mut observed = Vec::new();
    for candidate in set.candidates() {
        if let Some(paths) = paths {
            observed.push(capture_template(
                paths.root(),
                &Path::new(brand.directory())
                    .join("rules")
                    .join(candidate.name),
                InstructionOrigin::Project,
                policy.index,
                original,
            )?);
        }
        observed.push(capture_template(
            policy.installation.root(),
            &Path::new("rules").join(candidate.name),
            InstructionOrigin::Global,
            policy.index,
            original,
        )?);
    }
    let mut hub_description = None;
    let mut metadata = serde_json::Map::new();
    if let Some((_, _, installed, _)) = &selected {
        let root = installed.original_source.directory().as_path();
        if !paths.is_some_and(|paths| root.starts_with(paths.root()))
            && !root.starts_with(policy.installation.root())
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let mut body_capture = capture_template(
            root,
            Path::new("SKILL.md"),
            InstructionOrigin::Hub,
            policy.index,
            original,
        )?;
        let raw = body_capture
            .body
            .as_deref()
            .ok_or(io::ErrorKind::NotFound)?;
        let mut context = RenderContext::new(
            policy.native_install.hub.content(),
            original.cancellation(),
            original.snapshot().required_deadline()?,
        )
        .map_err(io::Error::other)?;
        let descriptor =
            SkillDescriptor::parse(raw.as_bytes(), &mut context).map_err(io::Error::other)?;
        let body = descriptor.body().to_owned();
        hub_description = Some(descriptor.description().to_owned());
        if let Some(license) = descriptor.license() {
            metadata.insert("license".into(), serde_json::json!(license));
        }
        if let Some(compatibility) = descriptor.compatibility() {
            metadata.insert("compatibility".into(), serde_json::json!(compatibility));
        }
        if let Some(extra) = descriptor.metadata() {
            metadata.insert("metadata".into(), serde_json::json!(extra));
        }
        body_capture.name = set.body_entry().to_owned();
        body_capture.body = Some(body);
        observed.push(body_capture);
        for reference in set.references() {
            let mut captured = capture_template(
                root,
                Path::new(reference.relative_path),
                InstructionOrigin::Hub,
                policy.index,
                original,
            )?;
            captured.name = reference.entry.to_owned();
            observed.push(captured);
        }
    }
    let mut candidates = set
        .candidates()
        .iter()
        .map(|item| CapturedInstruction {
            name: item.name,
            origin: item.origin,
            markdown: item.markdown,
        })
        .collect::<Vec<_>>();
    for item in &observed {
        if let Some(body) = item.body.as_deref() {
            candidates.push(CapturedInstruction {
                name: &item.name,
                origin: item.origin,
                markdown: body,
            });
        }
    }
    let description_entry = format!("skills/{}/description.md", module.name());
    if let Some(description) = hub_description.as_deref() {
        candidates.push(CapturedInstruction {
            name: &description_entry,
            origin: InstructionOrigin::Hub,
            markdown: description,
        });
    }
    let mut values = BTreeMap::from([
        ("brand".to_owned(), brand.key().to_owned()),
        ("brand_display".to_owned(), brand.key().to_owned()),
        (
            "brand_marker".to_owned(),
            brand.environment_prefix().to_owned(),
        ),
    ]);
    if let Some(docs_dir) = docs_dir {
        values.insert("docs_dir".into(), docs_dir.to_owned());
    }
    if input.destination == InstallDestination::Global {
        values.insert("publication_scope".into(), "global".into());
    }
    for candidate in &candidates {
        for prefix in ["mcp_tool_", "skill_name_"] {
            for (offset, _) in candidate.markdown.match_indices(prefix) {
                let key = candidate.markdown[offset..]
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .next()
                    .ok_or(io::ErrorKind::InvalidInput)?;
                let suffix = key
                    .strip_prefix(prefix)
                    .ok_or(io::ErrorKind::InvalidInput)?;
                let value = if prefix == "mcp_tool_" {
                    if suffix == "module_skill" {
                        input
                            .module_skill
                            .clone()
                            .map(Ok)
                            .unwrap_or_else(|| brand.mcp_tool_name(&["module", "skill"]))?
                    } else {
                        brand.mcp_tool_name(&suffix.split('_').collect::<Vec<_>>())?
                    }
                } else {
                    input
                        .skill_aliases
                        .get(suffix)
                        .cloned()
                        .map(Ok)
                        .unwrap_or_else(|| brand.skill_name(suffix))?
                };
                values.insert(key.to_owned(), value);
            }
        }
    }
    let variables = values
        .iter()
        .map(|(name, value)| InstructionVariable {
            name,
            value: InstructionValue::Text(value),
        })
        .collect::<Vec<_>>();
    let mut origins = BTreeMap::new();
    let content = policy.native_install.hub.content();
    let nz = |value| std::num::NonZeroUsize::new(value).ok_or(io::ErrorKind::InvalidInput);
    let limits = graphit_runtime::instructions::InstructionLimits {
        max_templates: nz(content.max_collection_entries)?,
        max_template_bytes: nz(content.max_input_bytes)?,
        max_values: nz(content.max_nodes)?,
        max_variable_bytes: nz(content.max_input_bytes)?,
        max_output_bytes: nz(content.max_output_bytes)?,
        fuel: std::num::NonZeroU64::new(
            u64::try_from(content.max_nodes)
                .map_err(io::Error::other)?
                .checked_mul(16)
                .ok_or(io::ErrorKind::InvalidInput)?,
        )
        .ok_or(io::ErrorKind::InvalidInput)?,
        recursion: nz(content.max_depth.min(500))?,
    };
    let mut render = |entry: &str| -> io::Result<String> {
        let result =
            render_instruction(entry, &candidates, &variables, limits, original.snapshot())
                .map_err(|cause| match cause {
                    graphit_runtime::instructions::InstructionError::Request(cause) => cause,
                    graphit_runtime::instructions::InstructionError::LimitExceeded(_) => {
                        io::Error::new(io::ErrorKind::FileTooLarge, cause)
                    }
                    _ => io::Error::new(io::ErrorKind::InvalidData, cause),
                })?;
        origins.insert(entry.to_owned(), format!("{:?}", result.origin()));
        Ok(result.into_body())
    };
    let body = render(set.body_entry())?;
    let description = render(&format!("skills/{}/description.md", module.name()))?;
    metadata.insert("name".into(), serde_json::json!(name));
    metadata.insert(
        "description".into(),
        serde_json::json!(description.trim_end_matches('\n')),
    );
    let yaml = serde_saphyr::to_string(&metadata).map_err(io::Error::other)?;
    let header = format!("---\n{}\n---\n{body}", yaml.trim_end_matches('\n'));
    let mut context = RenderContext::new(
        policy.native_install.hub.content(),
        original.cancellation(),
        original.snapshot().required_deadline()?,
    )
    .map_err(io::Error::other)?;
    let descriptor =
        SkillDescriptor::parse(header.as_bytes(), &mut context).map_err(io::Error::other)?;
    let native =
        render_skill(binding, &descriptor, Some(name), &mut context).map_err(io::Error::other)?;
    let relative = Path::new(&native.relative_entry)
        .parent()
        .ok_or(io::ErrorKind::InvalidInput)?
        .to_owned();
    let (directory_mode, file_mode) =
        payload_modes(paths.map_or(policy.installation.root(), ProjectPaths::root))?;
    let mut nodes = vec![
        GeneratedNode::directory(PathBuf::new(), directory_mode)?,
        GeneratedNode::file("SKILL.md".into(), native.bytes, file_mode)?,
    ];
    if !set.references().is_empty() {
        nodes.push(GeneratedNode::directory(
            "references".into(),
            directory_mode,
        )?);
    }
    for companion in set.references() {
        nodes.push(GeneratedNode::file(
            companion.relative_path.into(),
            render(companion.entry)?.into_bytes(),
            file_mode,
        )?);
    }
    for item in &observed {
        item.revalidate(original)?;
    }
    original.check()?;
    Ok(Compiled {
        nodes,
        relative,
        origins,
        observed,
        selected,
    })
}
fn payload_modes(root: &Path) -> io::Result<(PayloadMode, PayloadMode)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = root;
        Ok((
            PayloadMode::from_permissions(&fs::Permissions::from_mode(0o700))?,
            PayloadMode::from_permissions(&fs::Permissions::from_mode(0o600))?,
        ))
    }
    #[cfg(not(unix))]
    {
        let permissions = fs::metadata(root)?.permissions();
        Ok((
            PayloadMode::from_permissions(&permissions)?,
            PayloadMode::from_permissions(&permissions)?,
        ))
    }
}
pub(crate) fn capture_parent(
    target: &Path,
    root: &Path,
    policy: &SourcePolicy,
    original: &RequestContext<'_>,
    before: &impl Fn() -> io::Result<()>,
) -> io::Result<Vec<CapturedDocumentDirectory>> {
    target.strip_prefix(root).map_err(io::Error::other)?;
    before()?;
    let mut anchor = root;
    loop {
        match fs::symlink_metadata(anchor) {
            Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => break,
            Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
            Err(cause) if cause.kind() == io::ErrorKind::NotFound => {
                anchor = anchor.parent().ok_or(io::ErrorKind::InvalidInput)?;
            }
            Err(cause) => return Err(cause),
        }
        before()?;
    }
    // Observe only the necessary creation chain; this does not add anchor/HOME
    // to the publication scope's allowed destination roots.
    let relative = target.strip_prefix(anchor).map_err(io::Error::other)?;
    let mut held = vec![CapturedDocumentDirectory::capture(
        anchor,
        policy.index,
        original.snapshot(),
    )?];
    let mut current = anchor.to_owned();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        before()?;
        for directory in &held {
            directory.revalidate(original.snapshot())?;
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {}
            Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
            Err(cause) if cause.kind() == io::ErrorKind::NotFound => {
                graphit_fs::create_private_directory(&current)?;
            }
            Err(cause) => return Err(cause),
        }
        held.push(CapturedDocumentDirectory::capture(
            &current,
            policy.index,
            original.snapshot(),
        )?);
    }
    before()?;
    Ok(held)
}

#[derive(Debug)]
struct SkillPublicationError {
    module: &'static str,
    relative: PathBuf,
    cause: io::Error,
}
impl std::fmt::Display for SkillPublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "skill {} publication at {} failed: {}",
            self.module,
            self.relative.display(),
            self.cause,
        )
    }
}
impl std::error::Error for SkillPublicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

fn artifact_error(cause: ArtifactError) -> io::Error {
    io::Error::new(cause.kind(), cause)
}

/// Resolve the caller's effective project settings through the maintained Knowledge SDK.
pub(crate) fn docs_dir(
    config: &graphit_core::config::ConfigSnapshot,
    content: graphit_hub::native::ContentLimits,
    original: &RequestContext<'_>,
) -> io::Result<String> {
    use graphit_knowledge::settings::{KnowledgeSettings, KnowledgeSettingsLimits};
    let nz = |value| std::num::NonZeroUsize::new(value).ok_or(io::ErrorKind::InvalidInput);
    let settings = KnowledgeSettings::resolve(
        original.snapshot(),
        config,
        None,
        KnowledgeSettingsLimits {
            max_input_bytes: nz(content.max_input_bytes)?,
            max_docs_dir_bytes: nz(content.max_scalar_bytes)?,
            max_path_components: nz(content.max_depth)?,
            max_extension_tokens: nz(content.max_collection_entries)?,
            max_extensions: nz(content.max_collection_entries)?,
            max_extension_bytes: nz(content.max_scalar_bytes)?,
            max_projection_bytes: nz(content.max_output_bytes)?,
        },
    )
    .map_err(|cause| io::Error::new(cause.kind(), cause))?;
    Ok(settings.docs_dir().to_owned())
}
