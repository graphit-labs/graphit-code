//! Pure compiled skills and companion-reference sources; no capture or publication.
use crate::instructions::{CapturedInstruction, InstructionOrigin};
use crate::policy_instructions::{
    AGENT_REFERENCE_CONTRACT, AGENT_REFERENCE_EXAMPLE, AGENT_REFERENCE_MANDATE,
    KNOWLEDGE_REFERENCE_CONTRACT, PERSISTENCE_BOUNDARY, REFERENCE_QUERY_CONTRACT,
};
use graphit_core::module_settings::Module;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedSkillModule {
    module: Module,
}
impl UnsupportedSkillModule {
    pub fn module(&self) -> Module {
        self.module
    }
}
impl fmt::Display for UnsupportedSkillModule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "module {} has no compiled skill", self.module.name())
    }
}
impl std::error::Error for UnsupportedSkillModule {}

/// Logical companion path and template identity, not a filesystem destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkillReferenceTemplate {
    pub relative_path: &'static str,
    pub entry: &'static str,
}

#[derive(Clone, Copy)]
pub struct SkillTemplateSet {
    body_entry: &'static str,
    description: &'static str,
    references: &'static [SkillReferenceTemplate],
    candidates: &'static [CapturedInstruction<'static>],
}
impl SkillTemplateSet {
    pub fn body_entry(&self) -> &'static str {
        self.body_entry
    }
    /// Maintained metadata text; strip only terminal LF padding, without rendering.
    pub fn description(&self) -> &'static str {
        self.description.trim_end_matches('\n')
    }
    pub fn references(&self) -> &'static [SkillReferenceTemplate] {
        self.references
    }
    pub fn candidates(&self) -> &'static [CapturedInstruction<'static>] {
        self.candidates
    }
}
macro_rules! captured {
    ($name:literal) => {
        CapturedInstruction {
            name: $name,
            origin: InstructionOrigin::CompiledDefault,
            markdown: include_str!(concat!("../../../templates/agent-instructions/", $name)),
        }
    };
}

/// Select known policy only. Module enablement, effective values and grants are caller inputs.
pub fn skill_templates(module: Module) -> Result<SkillTemplateSet, UnsupportedSkillModule> {
    Ok(match module {
        Module::Ast => SkillTemplateSet {
            body_entry: "skills/ast/entry.md",
            description: include_str!(
                "../../../templates/agent-instructions/skills/ast/description.md"
            ),
            references: &[SkillReferenceTemplate {
                relative_path: "references/impact-assessment.md",
                entry: "skills/ast/references/impact-assessment.md",
            }],
            candidates: &[
                captured!("skills/ast/entry.md"),
                captured!("skills/ast/description.md"),
                captured!("skills/ast/references/impact-assessment.md"),
                AGENT_REFERENCE_CONTRACT,
                AGENT_REFERENCE_EXAMPLE,
                KNOWLEDGE_REFERENCE_CONTRACT,
                AGENT_REFERENCE_MANDATE,
                REFERENCE_QUERY_CONTRACT,
                PERSISTENCE_BOUNDARY,
            ],
        },
        Module::Hub => SkillTemplateSet {
            body_entry: "skills/hub/entry.md",
            description: include_str!(
                "../../../templates/agent-instructions/skills/hub/description.md"
            ),
            references: &[SkillReferenceTemplate {
                relative_path: "references/discovery-cases.md",
                entry: "skills/hub/references/discovery-cases.md",
            }],
            candidates: &[
                captured!("skills/hub/entry.md"),
                captured!("skills/hub/description.md"),
                captured!("skills/hub/references/discovery-cases.md"),
                AGENT_REFERENCE_CONTRACT,
                AGENT_REFERENCE_EXAMPLE,
                KNOWLEDGE_REFERENCE_CONTRACT,
                AGENT_REFERENCE_MANDATE,
                REFERENCE_QUERY_CONTRACT,
                PERSISTENCE_BOUNDARY,
            ],
        },
        Module::Task => SkillTemplateSet {
            body_entry: "skills/task/entry.md",
            description: include_str!(
                "../../../templates/agent-instructions/skills/task/description.md"
            ),
            references: &[
                SkillReferenceTemplate {
                    relative_path: "references/execution.md",
                    entry: "skills/task/references/execution.md",
                },
                SkillReferenceTemplate {
                    relative_path: "references/planning.md",
                    entry: "skills/task/references/planning.md",
                },
                SkillReferenceTemplate {
                    relative_path: "references/session.md",
                    entry: "skills/task/references/session.md",
                },
                SkillReferenceTemplate {
                    relative_path: "references/worked-feature.md",
                    entry: "skills/task/references/worked-feature.md",
                },
                SkillReferenceTemplate {
                    relative_path: "references/worked-system.md",
                    entry: "skills/task/references/worked-system.md",
                },
            ],
            candidates: &[
                captured!("skills/task/entry.md"),
                captured!("skills/task/description.md"),
                captured!("skills/task/references/execution.md"),
                captured!("skills/task/references/planning.md"),
                captured!("skills/task/references/session.md"),
                captured!("skills/task/references/worked-feature.md"),
                captured!("skills/task/references/worked-system.md"),
                AGENT_REFERENCE_CONTRACT,
                AGENT_REFERENCE_EXAMPLE,
                KNOWLEDGE_REFERENCE_CONTRACT,
                AGENT_REFERENCE_MANDATE,
                REFERENCE_QUERY_CONTRACT,
                PERSISTENCE_BOUNDARY,
                captured!("protocol.md"),
            ],
        },
        Module::Memory => SkillTemplateSet {
            body_entry: "skills/memory/entry.md",
            description: include_str!(
                "../../../templates/agent-instructions/skills/memory/description.md"
            ),
            references: &[SkillReferenceTemplate {
                relative_path: "references/durable-context.md",
                entry: "skills/memory/references/durable-context.md",
            }],
            candidates: &[
                captured!("skills/memory/entry.md"),
                captured!("skills/memory/description.md"),
                captured!("skills/memory/references/durable-context.md"),
                AGENT_REFERENCE_CONTRACT,
                AGENT_REFERENCE_EXAMPLE,
                KNOWLEDGE_REFERENCE_CONTRACT,
                AGENT_REFERENCE_MANDATE,
                REFERENCE_QUERY_CONTRACT,
                PERSISTENCE_BOUNDARY,
            ],
        },
        Module::Knowledge => SkillTemplateSet {
            body_entry: "skills/knowledge/entry.md",
            description: include_str!(
                "../../../templates/agent-instructions/skills/knowledge/description.md"
            ),
            references: &[
                SkillReferenceTemplate {
                    relative_path: "references/documentation-design.md",
                    entry: "skills/knowledge/references/documentation-design.md",
                },
                SkillReferenceTemplate {
                    relative_path: "references/worked-domain.md",
                    entry: "skills/knowledge/references/worked-domain.md",
                },
            ],
            candidates: &[
                captured!("skills/knowledge/entry.md"),
                captured!("skills/knowledge/description.md"),
                captured!("skills/knowledge/references/documentation-design.md"),
                captured!("skills/knowledge/references/worked-domain.md"),
                AGENT_REFERENCE_CONTRACT,
                AGENT_REFERENCE_EXAMPLE,
                KNOWLEDGE_REFERENCE_CONTRACT,
                AGENT_REFERENCE_MANDATE,
                REFERENCE_QUERY_CONTRACT,
                PERSISTENCE_BOUNDARY,
            ],
        },
        Module::Daemon
        | Module::Sync
        | Module::Embedding
        | Module::Hooks
        | Module::Agent
        | Module::Dream
        | Module::DaemonUi => return Err(UnsupportedSkillModule { module }),
    })
}
