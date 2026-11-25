//! Pure compiled policy templates. Callers capture overrides and render with
//! their original request; selecting content grants no authority or availability.
use crate::instructions::{
    CapturedInstruction, InstructionError, InstructionLimit, InstructionLimits, InstructionOrigin,
    InstructionValue, InstructionVariable, render_instruction,
};
use graphit_core::module_settings::Module;
use graphit_core::{brand::Brand, file_io::SnapshotContext};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MandateAudience {
    Coordinator,
    Delegate,
}

/// This partial catalog reports unsupported modules explicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedPolicyModule {
    module: Module,
}
impl UnsupportedPolicyModule {
    pub fn module(&self) -> Module {
        self.module
    }
}
impl fmt::Display for UnsupportedPolicyModule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "module {} has no compiled policy mandate",
            self.module.name()
        )
    }
}
impl std::error::Error for UnsupportedPolicyModule {}

/// A closed selection of generic policy; contains no live context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyPrompt {
    RouterPreamble,
    ModuleWrapper,
    Router,
    CoreInvariant,
    CursorCompensation,
    AntigravityCompensation,
    DocumentationConsistencyReminder,
    RoutingFallback,
    BootstrapHeader,
    ProjectVersionNotice,
    BootstrapMemoryFetch,
    BootstrapMemoryEmpty,
    BootstrapMemoryLoadedNotice,
    BootstrapMemoryRecall,
    BootstrapTaskSession,
    BootstrapTaskLifecycle,
    BootstrapTaskRecall,
    BootstrapTaskPlanning,
    WorkerMemoryFetch,
    WorkerMemoryLoadedNotice,
    WorkerMemoryRecall,
    WorkerTaskRecall,
    WorkerDelegatedContract,
    WorkerDeliveryValidation,
    WorkerProtocolHeader,
    DreamConsolidation,
    UserDreamConsolidation,
    ProjectHookContext,
}

/// Immutable defaults, including their reusable compiled include sources.
#[derive(Clone, Copy)]
pub struct PolicyTemplateSet {
    entry: &'static str,
    module_tag: Option<&'static str>,
    candidates: &'static [CapturedInstruction<'static>],
}
impl PolicyTemplateSet {
    pub fn entry(&self) -> &'static str {
        self.entry
    }
    pub fn module_tag(&self) -> Option<&'static str> {
        self.module_tag
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
pub(crate) const AGENT_REFERENCE_CONTRACT: CapturedInstruction<'static> =
    captured!("shared/references/agent-contract.md");
pub(crate) const AGENT_REFERENCE_EXAMPLE: CapturedInstruction<'static> =
    captured!("shared/references/agent-example.md");
pub(crate) const KNOWLEDGE_REFERENCE_CONTRACT: CapturedInstruction<'static> =
    captured!("shared/references/knowledge-contract.md");
pub(crate) const AGENT_REFERENCE_MANDATE: CapturedInstruction<'static> =
    captured!("shared/references/agent-mandate.md");
pub(crate) const REFERENCE_QUERY_CONTRACT: CapturedInstruction<'static> =
    captured!("shared/references/query-contract.md");
pub(crate) const PERSISTENCE_BOUNDARY: CapturedInstruction<'static> =
    captured!("shared/persistence-boundary.md");

macro_rules! policy_set {
    ($entry:literal, $tag:expr $(, $include:literal)*) => {
        PolicyTemplateSet {
            entry: $entry,
            module_tag: $tag,
            candidates: &[
                captured!($entry),
                AGENT_REFERENCE_CONTRACT,
                AGENT_REFERENCE_EXAMPLE,
                KNOWLEDGE_REFERENCE_CONTRACT,
                AGENT_REFERENCE_MANDATE,
                REFERENCE_QUERY_CONTRACT,
                PERSISTENCE_BOUNDARY,
                $(captured!($include),)*
            ],
        }
    };
}

/// Audience changes only the Task mandate, never permissions or module settings.
pub fn module_mandate(
    module: Module,
    audience: MandateAudience,
) -> Result<PolicyTemplateSet, UnsupportedPolicyModule> {
    Ok(match module {
        Module::Task => match audience {
            MandateAudience::Coordinator => {
                policy_set!("policy/mandates/task-coordinator.md", Some("task_rule"))
            }
            MandateAudience::Delegate => {
                policy_set!("policy/mandates/task-delegate.md", Some("task_rule"))
            }
        },
        Module::Memory => policy_set!("policy/mandates/memory.md", Some("mem_rule")),
        Module::Ast => policy_set!("policy/mandates/ast.md", Some("ast_rule")),
        Module::Hub => policy_set!("policy/mandates/hub.md", Some("hub_rule")),
        Module::Knowledge => policy_set!("policy/mandates/knowledge.md", Some("doc_rule")),
        Module::Daemon
        | Module::Sync
        | Module::Embedding
        | Module::Hooks
        | Module::Agent
        | Module::Dream
        | Module::DaemonUi => return Err(UnsupportedPolicyModule { module }),
    })
}

/// Select a fragment without resolving configuration or runtime lifecycle.
pub fn policy_prompt(prompt: PolicyPrompt) -> PolicyTemplateSet {
    match prompt {
        PolicyPrompt::RouterPreamble => policy_set!(
            "policy/hooks/router-preamble.md",
            None,
            "shared/agent-workflow.md"
        ),
        PolicyPrompt::ModuleWrapper => policy_set!("policy/hooks/module-wrapper.md", None),
        PolicyPrompt::Router => policy_set!(
            "policy/hooks/router.md",
            None,
            "policy/hooks/router-preamble.md",
            "shared/agent-workflow.md"
        ),
        PolicyPrompt::CoreInvariant => policy_set!("policy/hooks/core-invariant.md", None),
        PolicyPrompt::CursorCompensation => {
            policy_set!("policy/hooks/cursor-compensation.md", None)
        }
        PolicyPrompt::AntigravityCompensation => {
            policy_set!("policy/hooks/antigravity-compensation.md", None)
        }
        PolicyPrompt::DocumentationConsistencyReminder => {
            policy_set!("policy/hooks/documentation-consistency-reminder.md", None)
        }
        PolicyPrompt::RoutingFallback => policy_set!("policy/hooks/routing-fallback.md", None),
        PolicyPrompt::BootstrapHeader => policy_set!("policy/hooks/bootstrap-header.md", None),
        PolicyPrompt::ProjectVersionNotice => {
            policy_set!("policy/hooks/project-version-notice.md", None)
        }
        PolicyPrompt::BootstrapMemoryFetch => {
            policy_set!("policy/hooks/bootstrap-memory-fetch.md", None)
        }
        PolicyPrompt::BootstrapMemoryEmpty => {
            policy_set!("policy/hooks/bootstrap-memory-empty.md", None)
        }
        PolicyPrompt::BootstrapMemoryLoadedNotice => {
            policy_set!("policy/hooks/bootstrap-memory-loaded-notice.md", None)
        }
        PolicyPrompt::BootstrapMemoryRecall => {
            policy_set!("policy/hooks/bootstrap-memory-recall.md", None)
        }
        PolicyPrompt::BootstrapTaskSession => {
            policy_set!("policy/hooks/bootstrap-task-session.md", None)
        }
        PolicyPrompt::BootstrapTaskLifecycle => {
            policy_set!("policy/hooks/bootstrap-task-lifecycle.md", None)
        }
        PolicyPrompt::BootstrapTaskRecall => {
            policy_set!("policy/hooks/bootstrap-task-recall.md", None)
        }
        PolicyPrompt::BootstrapTaskPlanning => {
            policy_set!("policy/hooks/bootstrap-task-planning.md", None)
        }
        PolicyPrompt::WorkerMemoryFetch => policy_set!("policy/hooks/worker-memory-fetch.md", None),
        PolicyPrompt::WorkerMemoryLoadedNotice => {
            policy_set!("policy/hooks/worker-memory-loaded-notice.md", None)
        }
        PolicyPrompt::WorkerMemoryRecall => {
            policy_set!("policy/hooks/worker-memory-recall.md", None)
        }
        PolicyPrompt::WorkerTaskRecall => policy_set!("policy/hooks/worker-task-recall.md", None),
        PolicyPrompt::WorkerDelegatedContract => policy_set!(
            "policy/hooks/worker-delegated-contract.md",
            None,
            "protocol.md",
            "shared/agent-workflow.md"
        ),
        PolicyPrompt::WorkerDeliveryValidation => {
            policy_set!("policy/hooks/worker-delivery-validation.md", None)
        }
        PolicyPrompt::WorkerProtocolHeader => {
            policy_set!("policy/hooks/worker-protocol-header.md", None)
        }
        PolicyPrompt::DreamConsolidation => {
            policy_set!("policy/prompts/dream-consolidation.md", None)
        }
        PolicyPrompt::UserDreamConsolidation => {
            policy_set!("policy/prompts/user-dream-consolidation.md", None)
        }
        PolicyPrompt::ProjectHookContext => policy_set!(
            "policy/hooks/project-hook-context.md",
            None,
            "policy/hooks/bootstrap-header.md",
            "policy/hooks/project-version-notice.md",
            "policy/hooks/router.md",
            "policy/hooks/router-preamble.md",
            "policy/hooks/core-invariant.md",
            "shared/agent-workflow.md",
            "policy/hooks/cursor-compensation.md",
            "policy/hooks/antigravity-compensation.md"
        ),
    }
}

pub const PROFILE_MODULES: [Module; 5] = [
    Module::Task,
    Module::Memory,
    Module::Ast,
    Module::Hub,
    Module::Knowledge,
];

pub fn append_coordinator_mandates(
    body: String,
    brand: &Brand,
    limits: InstructionLimits,
    original: &SnapshotContext,
) -> Result<String, InstructionError> {
    let mut body = body;
    append_profile_text(
        &mut body,
        include_str!("../../../templates/agent-instructions/shared/agent-workflow.md"),
        limits,
    )?;
    append_profile_text(
        &mut body,
        include_str!("../../../templates/agent-instructions/policy/hooks/core-invariant.md"),
        limits,
    )?;
    let skills = PROFILE_MODULES
        .map(|module| brand.skill_name(module.name()))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(InstructionError::Request)?;
    let modules: Vec<_> = PROFILE_MODULES
        .into_iter()
        .zip(skills.iter().map(String::as_str))
        .collect();
    append_profile_mandates(
        body,
        brand,
        MandateAudience::Coordinator,
        &modules,
        limits,
        original,
    )
}

pub(crate) fn append_delegate_mandates(
    mut body: String,
    brand: &Brand,
    modules: &[(Module, &str)],
    limits: InstructionLimits,
    original: &SnapshotContext,
) -> Result<String, InstructionError> {
    let set = policy_prompt(PolicyPrompt::WorkerDelegatedContract);
    let general = render_instruction(set.entry(), set.candidates(), &[], limits, original)?;
    append_profile_text(&mut body, general.body(), limits)?;
    append_profile_mandates(
        body,
        brand,
        MandateAudience::Delegate,
        modules,
        limits,
        original,
    )
}

fn append_profile_text(
    body: &mut String,
    text: &str,
    limits: InstructionLimits,
) -> Result<(), InstructionError> {
    let extra = text.len().saturating_add(2);
    if body.len().saturating_add(extra) > limits.max_output_bytes.get() {
        return Err(InstructionError::LimitExceeded(
            InstructionLimit::OutputBytes,
        ));
    }
    body.try_reserve(extra)
        .map_err(|_| InstructionError::Allocation)?;
    body.push_str("\n\n");
    body.push_str(text);
    Ok(())
}

/// Static profile policy uses the same module mandates as a session hook, with
/// no live records, selectors or claims. Availability still comes from MCP.
fn append_profile_mandates(
    mut body: String,
    brand: &Brand,
    audience: MandateAudience,
    modules: &[(Module, &str)],
    limits: InstructionLimits,
    original: &SnapshotContext,
) -> Result<String, InstructionError> {
    for &(module, skill) in modules {
        original.check().map_err(InstructionError::Request)?;
        let set = module_mandate(module, audience).map_err(|_| InstructionError::InvalidName)?;
        let mut slots = BTreeMap::new();
        for candidate in set.candidates() {
            for (offset, _) in candidate.markdown.match_indices("mcp_tool_") {
                let slot = candidate.markdown[offset..]
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .next()
                    .expect("slot prefix");
                let parts = slot["mcp_tool_".len()..].split('_').collect::<Vec<_>>();
                slots.insert(
                    slot,
                    brand
                        .mcp_tool_name(&parts)
                        .map_err(InstructionError::Request)?,
                );
            }
        }
        let skill_slot = format!("skill_name_{}", module.name());
        let variables: Vec<_> = slots
            .iter()
            .map(|(name, value)| InstructionVariable {
                name,
                value: InstructionValue::Text(value),
            })
            .chain([
                InstructionVariable {
                    name: &skill_slot,
                    value: InstructionValue::Text(skill),
                },
                InstructionVariable {
                    name: "brand_display",
                    value: InstructionValue::Text(brand.key()),
                },
                InstructionVariable {
                    name: "mandate_audience",
                    value: InstructionValue::Text(match audience {
                        MandateAudience::Coordinator => "coordinator",
                        MandateAudience::Delegate => "delegate",
                    }),
                },
            ])
            .collect();
        let mandate =
            render_instruction(set.entry(), set.candidates(), &variables, limits, original)?;
        let tag = set.module_tag().expect("compiled module mandate");
        let extra = mandate.body().len() + 2 * tag.len() + 9;
        if body.len().saturating_add(extra) > limits.max_output_bytes.get() {
            return Err(InstructionError::LimitExceeded(
                InstructionLimit::OutputBytes,
            ));
        }
        body.try_reserve(extra)
            .map_err(|_| InstructionError::Allocation)?;
        use std::fmt::Write;
        write!(body, "\n\n<{tag}>\n{}</{tag}>\n", mandate.body()).expect("String writing");
    }
    original.check().map_err(InstructionError::Request)?;
    Ok(body)
}
