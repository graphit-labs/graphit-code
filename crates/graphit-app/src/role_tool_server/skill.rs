use super::{State, ToolResult, error, parse};
use graphit_auth::{Permission, VerifiedRoleGrant};
use graphit_core::{
    config::ConfigKey, module_settings::Module, native_document_file::CapturedDocumentDirectory,
    project_paths::ProjectPaths,
};
use graphit_hub::{ArtifactType, ConcreteVersion, NativeBinding, policy::RequestContext};
use graphit_runtime::{
    instructions::{
        CapturedInstruction, InstructionOrigin, InstructionValue, InstructionVariable,
        render_instruction,
    },
    skill_instructions::skill_templates,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SkillModule {
    Ast,
    Hub,
    Task,
    Memory,
    Knowledge,
}
impl SkillModule {
    fn module(&self) -> Module {
        match self {
            Self::Ast => Module::Ast,
            Self::Hub => Module::Hub,
            Self::Task => Module::Task,
            Self::Memory => Module::Memory,
            Self::Knowledge => Module::Knowledge,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HubSelection {
    id: String,
    version: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    module: SkillModule,
    reference: Option<String>,
    hub: Option<HubSelection>,
}
pub(super) fn read(
    state: &State,
    paths: &ProjectPaths,
    effective_binding: NativeBinding,
    grant: &VerifiedRoleGrant<'_>,
    input: Option<Value>,
    original: &RequestContext<'_>,
) -> ToolResult {
    let input: Input = parse(input)?;
    let module = input.module.module();
    grant
        .authorize(Permission::Read(module))
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let set = skill_templates(module).map_err(|_| error("unsupported_module"))?;
    let entry = match &input.reference {
        None => set.body_entry(),
        Some(reference) => {
            set.references()
                .iter()
                .find(|item| item.relative_path == reference)
                .ok_or_else(|| error("unknown_reference"))?
                .entry
        }
    };
    let mut observed = Vec::new();
    let mut hub_capture = None;
    for candidate in set.candidates() {
        grant
            .authorize(Permission::Read(module))
            .map_err(|_| error("authentication_or_permission_denied"))?;
        for (root, relative, origin) in [
            (
                paths.root(),
                Path::new(paths.brand().directory())
                    .join("rules")
                    .join(candidate.name),
                InstructionOrigin::Project,
            ),
            (
                state.policy.source.installation.root(),
                Path::new("rules").join(candidate.name),
                InstructionOrigin::Global,
            ),
        ] {
            observed.push(
                crate::managed_roles::capture_template(
                    root,
                    &relative,
                    origin,
                    state.policy.source.index,
                    original,
                )
                .map_err(|_| error("skill_capture_failed"))?,
            );
        }
    }
    if let Some(selected) = input.hub {
        grant
            .authorize(Permission::Read(Module::Hub))
            .map_err(|_| error("authentication_or_permission_denied"))?;
        let (hub_registry, registry_parent) =
            super::hub::captured_global_registry(state, original)?;
        let registry = hub_registry
            .load_captured(&registry_parent, original.snapshot())
            .map_err(super::hub::io_failure)?;
        let version = ConcreteVersion::new(&selected.version, state.task.role_reference)
            .map_err(|_| error("invalid_hub_version"))?;
        let artifact = registry
            .get_version(ArtifactType::Skill, &selected.id, &version)
            .ok_or_else(|| error("selected_skill_unavailable"))?;
        let mut matching = artifact
            .installations
            .iter()
            .filter(|item| item.target.includes(effective_binding));
        let installation = matching
            .next()
            .ok_or_else(|| error("selected_skill_binding_unavailable"))?;
        if matching.next().is_some() {
            return Err(error("ambiguous_skill_binding"));
        }
        let source_directory = CapturedDocumentDirectory::capture(
            installation.original_source.directory().as_path(),
            state.policy.source.index,
            original.snapshot(),
        )
        .map_err(super::hub::io_failure)?;
        let root = source_directory.canonical_path();
        if !root.starts_with(paths.root())
            && !root.starts_with(state.auth.installation().live.owner_root())
        {
            return Err(error("skill_outside_captured_roots"));
        }
        for candidate in set.candidates() {
            observed.push(
                crate::managed_roles::capture_template(
                    root,
                    Path::new(candidate.name),
                    InstructionOrigin::Hub,
                    state.policy.source.index,
                    original,
                )
                .map_err(|_| error("skill_capture_failed"))?,
            );
        }
        if !observed.iter().any(|item| {
            item.origin == InstructionOrigin::Hub && item.name == entry && item.body.is_some()
        }) {
            return Err(error("selected_skill_entry_absent"));
        }
        hub_capture = Some((registry_parent, source_directory, installation.clone()));
    }
    let mut candidates: Vec<_> = set
        .candidates()
        .iter()
        .map(|item| CapturedInstruction {
            name: item.name,
            origin: item.origin,
            markdown: item.markdown,
        })
        .collect();
    for item in &observed {
        if let Some(body) = item.body.as_deref() {
            candidates.push(CapturedInstruction {
                name: &item.name,
                origin: item.origin,
                markdown: body,
            });
        }
    }
    let broker = crate::instruction_overrides::BrokerInstructions::load(
        state.auth.issuer(),
        state.policy.source.installation.root(),
        original,
    )
    .map_err(|_| error("broker_instruction_layer_unavailable"))?;
    candidates.extend(broker.candidates(set.candidates()));
    let mut values = BTreeMap::new();
    values.insert("tool_transport".to_owned(), "role".to_owned());
    values.insert("brand".to_owned(), paths.brand().key().to_owned());
    values.insert("brand_display".to_owned(), paths.brand().key().to_owned());
    let docs_dir = state
        .policy
        .config
        .get::<String>(&ConfigKey::new("knowledge.docs_dir").expect("known config key"))
        .map_err(|_| error("invalid_docs_dir"))?
        .unwrap_or_else(|| "docs".into());
    values.insert("docs_dir".to_owned(), docs_dir);
    // Names come from the maintained templates; authored overrides cannot add grants or facts.
    for candidate in set.candidates() {
        for (offset, _) in candidate.markdown.match_indices("mcp_tool_") {
            let name: &str = candidate.markdown[offset..]
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .next()
                .expect("prefix");
            let suffix = name.strip_prefix("mcp_tool_").expect("prefix");
            values.entry(name.to_owned()).or_insert(
                paths
                    .brand()
                    .mcp_tool_name(&suffix.split('_').collect::<Vec<_>>())
                    .map_err(|_| error("invalid_template_tool_name"))?,
            );
        }
    }
    let variables: Vec<_> = values
        .iter()
        .map(|(name, value)| InstructionVariable {
            name,
            value: InstructionValue::Text(value),
        })
        .collect();
    let rendered = render_instruction(
        entry,
        &candidates,
        &variables,
        state.policy.instructions,
        original.snapshot(),
    )
    .map_err(|_| error("instruction_render_failed"))?;
    for item in &observed {
        item.revalidate(original)
            .map_err(|_| error("skill_changed"))?;
    }
    if let Some((registry_parent, source_directory, installed)) = hub_capture {
        source_directory
            .revalidate(original.snapshot())
            .map_err(super::hub::io_failure)?;
        let current = state
            .policy
            .source
            .registry
            .load_captured(&registry_parent, original.snapshot())
            .map_err(super::hub::io_failure)?;
        if !current
            .get_version(
                ArtifactType::Skill,
                installed.original_source.id().as_str(),
                installed.original_source.version(),
            )
            .is_some_and(|artifact| {
                artifact
                    .installations
                    .iter()
                    .any(|current| current.same_source(&installed))
            })
        {
            return Err(error("installed_artifact_changed"));
        }
        grant
            .authorize(Permission::Read(Module::Hub))
            .map_err(|_| error("authentication_or_permission_denied"))?;
    }
    grant
        .authorize(Permission::Read(module))
        .map_err(|_| error("authentication_or_permission_denied"))?;
    Ok(
        json!({"module":module.name(),"reference":input.reference,"origin":format!("{:?}",rendered.origin()),"source":rendered.body(),"references":set.references().iter().map(|r|r.relative_path).collect::<Vec<_>>()}),
    )
}
