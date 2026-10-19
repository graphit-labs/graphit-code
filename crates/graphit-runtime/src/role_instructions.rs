//! Captured managed-role templates; rendering content grants no authority.
use crate::{
    ManagedRole,
    instructions::{
        CapturedInstruction, InstructionError, InstructionLimits, InstructionOrigin,
        InstructionValue, InstructionVariable, RenderedInstruction, render_instruction,
    },
};
use graphit_core::{brand::Brand, file_io::SnapshotContext};
use std::{
    io,
    num::{NonZeroU64, NonZeroUsize},
};

/// Raw defaults are included before rendering, never inserted as rendered text.
pub struct RoleTemplateSet {
    entry: &'static str,
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
    ($entry:literal) => {
        RoleTemplateSet {
            entry: $entry,
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
                    name: "roles/header.md",
                    origin: InstructionOrigin::CompiledDefault,
                    markdown: include_str!("../../../templates/agent-instructions/roles/header.md"),
                },
                CapturedInstruction {
                    name: "protocol.md",
                    origin: InstructionOrigin::CompiledDefault,
                    markdown: include_str!("../../../templates/agent-instructions/protocol.md"),
                },
            ],
        }
    };
}
pub fn role_templates(role: ManagedRole) -> RoleTemplateSet {
    match role {
        ManagedRole::Scout => role_set!("roles/scout.md"),
        ManagedRole::Tracker => role_set!("roles/tracker.md"),
        ManagedRole::Scribe => role_set!("roles/scribe.md"),
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
    instruction: RenderedInstruction,
}
impl RenderedRole {
    pub fn role(&self) -> ManagedRole {
        self.role
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn body(&self) -> &str {
        self.instruction.body()
    }
    pub fn origin(&self) -> InstructionOrigin {
        self.instruction.origin()
    }
    pub fn into_parts(self) -> (String, String) {
        (self.name, self.instruction.into_body())
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
    let variables = [
        InstructionVariable {
            name: "agent_name",
            value: InstructionValue::Text(&name),
        },
        InstructionVariable {
            name: "role_summary",
            value: InstructionValue::Text(role.summary()),
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
    let instruction = render_instruction(
        role_templates(role).entry(),
        candidates,
        &variables,
        limits,
        original,
    )?;
    original.check().map_err(InstructionError::Request)?;
    Ok(RenderedRole {
        role,
        name,
        instruction,
    })
}
/// Defaults bound the small built-in bundle; installation uses its effective policy.
pub(crate) const DEFAULT_ROLE_LIMITS: InstructionLimits = InstructionLimits {
    max_templates: NonZeroUsize::new(3).unwrap(),
    max_template_bytes: NonZeroUsize::new(65_536).unwrap(),
    max_values: NonZeroUsize::new(16).unwrap(),
    max_variable_bytes: NonZeroUsize::new(4096).unwrap(),
    max_output_bytes: NonZeroUsize::new(65_536).unwrap(),
    fuel: NonZeroU64::new(10_000).unwrap(),
    recursion: NonZeroUsize::new(32).unwrap(),
};
