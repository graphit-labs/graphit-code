//! Captured managed-role templates; rendering content grants no authority.
use crate::{
    ManagedRole,
    instructions::{
        CapturedInstruction, InstructionError, InstructionLimits, InstructionOrigin,
        InstructionValue, InstructionVariable, render_instruction,
    },
    policy_instructions::{PROFILE_MODULES, append_delegate_mandates},
};
use graphit_core::{brand::Brand, file_io::SnapshotContext};
use std::{
    io,
    num::{NonZeroU64, NonZeroUsize},
};

/// Raw defaults are included before rendering, never inserted as rendered text.
pub struct RoleTemplateSet {
    entry: &'static str,
    summary: &'static str,
    candidates: &'static [CapturedInstruction<'static>],
}
impl RoleTemplateSet {
    pub fn entry(&self) -> &'static str {
        self.entry
    }
    pub fn candidates(&self) -> &'static [CapturedInstruction<'static>] {
        self.candidates
    }
}
macro_rules! role_set {
    ($entry:literal, $summary:literal) => {
        RoleTemplateSet {
            entry: $entry,
            summary: $summary,
            candidates: &[
                CapturedInstruction {
                    name: $entry,
                    origin: InstructionOrigin::CompiledDefault,
                    markdown: include_str!(concat!(
                        "../../../templates/agent-instructions/",
                        $entry
                    )),
                },
                CapturedInstruction {
                    name: $summary,
                    origin: InstructionOrigin::CompiledDefault,
                    markdown: include_str!(concat!(
                        "../../../templates/agent-instructions/",
                        $summary
                    )),
                },
                CapturedInstruction {
                    name: "roles/header.md",
                    origin: InstructionOrigin::CompiledDefault,
                    markdown: include_str!("../../../templates/agent-instructions/roles/header.md"),
                },
                CapturedInstruction {
                    name: "protocol.md",
                    origin: InstructionOrigin::CompiledDefault,
                    markdown: include_str!("../../../templates/agent-instructions/protocol.md"),
                },
                CapturedInstruction {
                    name: "shared/agent-workflow.md",
                    origin: InstructionOrigin::CompiledDefault,
                    markdown: include_str!(
                        "../../../templates/agent-instructions/shared/agent-workflow.md"
                    ),
                },
            ],
        }
    };
}
pub fn role_templates(role: ManagedRole) -> RoleTemplateSet {
    match role {
        ManagedRole::Scout => role_set!("roles/scout.md", "roles/scout-summary.md"),
        ManagedRole::Tracker => role_set!("roles/tracker.md", "roles/tracker-summary.md"),
        ManagedRole::Scribe => role_set!("roles/scribe.md", "roles/scribe-summary.md"),
    }
}
/// Names supplied by the effective MCP/skill configuration, not permissions.
pub struct RoleAliases<'a> {
    pub module_skill: &'a str,
    pub skills: &'a [&'a str],
}
#[derive(Debug)]
pub struct RenderedRole {
    role: ManagedRole,
    name: String,
    summary: String,
    body: String,
    origin: InstructionOrigin,
}
impl RenderedRole {
    pub fn role(&self) -> ManagedRole {
        self.role
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn summary(&self) -> &str {
        &self.summary
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn origin(&self) -> InstructionOrigin {
        self.origin
    }
    pub fn into_parts(self) -> (String, String) {
        (self.name, self.body)
    }
}
/// Render caller-captured candidates once using the same strict engine as defaults.
/// The caller retains and rechecks physical/Auth captures until publication.
pub fn render_role(
    role: ManagedRole,
    brand: &Brand,
    candidates: &[CapturedInstruction<'_>],
    aliases: RoleAliases<'_>,
    limits: InstructionLimits,
    original: &SnapshotContext,
) -> Result<RenderedRole, InstructionError> {
    original.check().map_err(InstructionError::Request)?;
    let name = format!("{}-{}", brand.key(), role.key());
    if name.len() > 64 {
        return Err(InstructionError::Request(
            io::ErrorKind::InvalidInput.into(),
        ));
    }
    let templates = role_templates(role);
    let mut variables = vec![
        InstructionVariable {
            name: "brand",
            value: InstructionValue::Text(brand.key()),
        },
        InstructionVariable {
            name: "agent_name",
            value: InstructionValue::Text(&name),
        },
        InstructionVariable {
            name: "mcp_tool_module_skill",
            value: InstructionValue::Text(aliases.module_skill),
        },
        InstructionVariable {
            name: "skill_names",
            value: InstructionValue::TextList(aliases.skills),
        },
    ];
    let summary = if candidates.iter().any(|template| {
        template.name == templates.summary && template.origin != InstructionOrigin::CompiledDefault
    }) {
        render_instruction(templates.summary, candidates, &variables, limits, original)?
            .body()
            .trim_end_matches('\n')
            .to_owned()
    } else {
        role.summary().to_owned()
    };
    variables.push(InstructionVariable {
        name: "role_summary",
        value: InstructionValue::Text(&summary),
    });
    let instruction =
        render_instruction(templates.entry(), candidates, &variables, limits, original)?;
    let origin = instruction.origin();
    let mut selected = Vec::new();
    for (index, name) in role.modules().iter().enumerate() {
        let module = PROFILE_MODULES
            .into_iter()
            .find(|module| module.name() == *name)
            .expect("managed role module");
        let skill = match aliases.skills.get(index) {
            Some(alias) => (*alias).to_owned(),
            None => brand.skill_name(name).map_err(InstructionError::Request)?,
        };
        selected.push((module, skill));
    }
    let selected: Vec<_> = selected
        .iter()
        .map(|(module, skill)| (*module, skill.as_str()))
        .collect();
    let body =
        append_delegate_mandates(instruction.into_body(), brand, &selected, limits, original)?;
    Ok(RenderedRole {
        role,
        name,
        summary,
        body,
        origin,
    })
}
/// Defaults bound the small built-in bundle; installation uses its effective policy.
pub(crate) const DEFAULT_ROLE_LIMITS: InstructionLimits = InstructionLimits {
    max_templates: NonZeroUsize::new(9).unwrap(),
    max_template_bytes: NonZeroUsize::new(65_536).unwrap(),
    max_values: NonZeroUsize::new(32).unwrap(),
    max_variable_bytes: NonZeroUsize::new(4096).unwrap(),
    max_output_bytes: NonZeroUsize::new(65_536).unwrap(),
    fuel: NonZeroU64::new(10_000).unwrap(),
    recursion: NonZeroUsize::new(32).unwrap(),
};
