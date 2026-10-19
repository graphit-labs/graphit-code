//! Pure compiled policy templates. Callers capture overrides and render with
//! their original request; selecting content grants no authority or availability.
use crate::instructions::{CapturedInstruction, InstructionOrigin};
use graphit_core::module_settings::Module;
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
        PolicyPrompt::RouterPreamble => policy_set!("policy/hooks/router-preamble.md", None),
        PolicyPrompt::ModuleWrapper => policy_set!("policy/hooks/module-wrapper.md", None),
        PolicyPrompt::Router => policy_set!(
            "policy/hooks/router.md",
            None,
            "policy/hooks/router-preamble.md"
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
            "protocol.md"
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
            "policy/hooks/core-invariant.md"
        ),
    }
}
