//! Compose one readonly PROJECT hook context from captured inputs and real SDK admissions.
use graphit_auth::{AuthError, ProjectReadAdmission};
use graphit_core::{
    file_io::SnapshotContext,
    module_settings::{Module, ModuleSettings},
};
use graphit_hub::{native::NativeBinding, policy::RequestContext};
use graphit_records::{
    memory_attributes::MemoryClass,
    memory_store::{
        Memory, MemoryId, MemoryListFilter, MemoryListRead, MemoryStoreError,
        MemoryStoreReadLimits, NativeMemoryStore,
    },
};
use graphit_runtime::{
    EncodedOutput, HookEvent, OutputContext, OutputError, OutputLimits, OutputMessage, encode,
    instructions::{
        InstructionError, InstructionLimit, InstructionLimits, InstructionValue,
        InstructionVariable, render_instruction,
    },
    policy_instructions::{
        MandateAudience, PolicyPrompt, PolicyTemplateSet, module_mandate, policy_prompt,
    },
};
use serde::{Serialize, Serializer, ser::SerializeSeq};
use std::{error::Error, fmt, io};
mod task;
mod user;
use graphit_records::task_store::{TaskStoreError, TaskStoreReadLimits};
pub use task::TaskContextSelection;
pub(crate) use user::UserContext;

/// Data selected by the real caller; selecting a binding or audience grants no authority.
pub struct ProjectHookInput<'a> {
    pub task_context: Option<&'a TaskContextSelection>,
    pub binding: NativeBinding,
    pub event: HookEvent,
    pub modules: &'a ModuleSettings,
    pub audience: MandateAudience,
    pub variables: &'a [InstructionVariable<'a>],
    pub instruction_limits: InstructionLimits,
    pub output_limits: OutputLimits,
}

/// Causal failures. Formatting never includes record bodies, variables or SDK diagnostics.
pub enum HookContextError {
    Request(io::Error),
    Auth(AuthError),
    Memory(Box<MemoryStoreError>),
    Task(Box<TaskStoreError>),
    MissingTaskAdmission,
    InvalidTaskSelection,
    MissingTaskSession,
    MissingTask,
    TaskSessionMismatch,
    TaskRecipientDenied,
    UserRecipientDenied,
    Instruction(InstructionError),
    Output(OutputError),
    Json(serde_json::Error),
    MissingMemoryAdmission,
    ReservedVariable,
    UnsupportedEvent,
    ProjectVersionMismatch,
    ProjectScopeMismatch,
}
impl fmt::Debug for HookContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Request(_) => "HookContextError::Request",
            Self::Auth(_) => "HookContextError::Auth",
            Self::Memory(_) => "HookContextError::Memory",
            Self::Task(_) => "HookContextError::Task",
            Self::MissingTaskAdmission => "HookContextError::MissingTaskAdmission",
            Self::InvalidTaskSelection => "HookContextError::InvalidTaskSelection",
            Self::MissingTaskSession => "HookContextError::MissingTaskSession",
            Self::MissingTask => "HookContextError::MissingTask",
            Self::TaskSessionMismatch => "HookContextError::TaskSessionMismatch",
            Self::TaskRecipientDenied => "HookContextError::TaskRecipientDenied",
            Self::UserRecipientDenied => "HookContextError::UserRecipientDenied",
            Self::Instruction(_) => "HookContextError::Instruction",
            Self::Output(_) => "HookContextError::Output",
            Self::Json(_) => "HookContextError::Json",
            Self::MissingMemoryAdmission => "HookContextError::MissingMemoryAdmission",
            Self::ReservedVariable => "HookContextError::ReservedVariable",
            Self::UnsupportedEvent => "HookContextError::UnsupportedEvent",
            Self::ProjectVersionMismatch => "HookContextError::ProjectVersionMismatch",
            Self::ProjectScopeMismatch => "HookContextError::ProjectScopeMismatch",
        })
    }
}
impl fmt::Display for HookContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PROJECT hook context failed: {self:?}")
    }
}
impl Error for HookContextError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Auth(e) => Some(e),
            Self::Memory(e) => Some(e.as_ref()),
            Self::Task(e) => Some(e.as_ref()),
            Self::Instruction(e) => Some(e),
            Self::Output(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct ProjectMemoryRow<'a> {
    id: MemoryId,
    revision: u64,
    title: &'a str,
    body: &'a str,
    classification: &'static str,
    mandatory: bool,
    important: bool,
}
pub(crate) struct ProjectMemoryRows<'a>(pub(crate) &'a [Memory]);
impl Serialize for ProjectMemoryRows<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for memory in self.0 {
            sequence.serialize_element(&ProjectMemoryRow {
                id: memory.id,
                revision: memory.revision,
                title: &memory.title,
                body: &memory.body,
                classification: classification(memory.attributes.classification),
                mandatory: memory.attributes.mandatory,
                important: memory.attributes.important,
            })?;
        }
        sequence.end()
    }
}

struct JsonWriter<'a> {
    original: &'a SnapshotContext,
    bytes: Vec<u8>,
    maximum: usize,
    overflow: Option<HookContextError>,
    first: Option<HookContextError>,
}
impl<'a> JsonWriter<'a> {
    fn new(original: &'a SnapshotContext, input: &ProjectHookInput<'_>) -> Self {
        // Ties retain this order. These are existing per-payload bounds, not a new quota.
        let mut maximum = input.instruction_limits.max_variable_bytes.get();
        let mut overflow = HookContextError::Instruction(InstructionError::LimitExceeded(
            InstructionLimit::VariableBytes,
        ));
        for (cap, cause) in [
            (
                input.instruction_limits.max_output_bytes.get(),
                HookContextError::Instruction(InstructionError::LimitExceeded(
                    InstructionLimit::OutputBytes,
                )),
            ),
            (
                input.output_limits.max_context_bytes,
                HookContextError::Output(OutputError::LimitExceeded),
            ),
            (
                input.output_limits.max_output_bytes,
                HookContextError::Output(OutputError::LimitExceeded),
            ),
        ] {
            if cap < maximum {
                maximum = cap;
                overflow = cause;
            }
        }
        Self {
            original,
            bytes: Vec::new(),
            maximum,
            overflow: Some(overflow),
            first: None,
        }
    }
    fn fail(&mut self, cause: HookContextError) -> io::Error {
        if self.first.is_none() {
            self.first = Some(cause);
        }
        // Serde transports this terminal signal; the typed first cause stays here.
        io::ErrorKind::InvalidData.into()
    }
    fn check(&mut self) -> io::Result<()> {
        if self.first.is_some() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if let Err(cause) = self.original.check() {
            return Err(self.fail(HookContextError::Request(cause)));
        }
        Ok(())
    }
}
impl io::Write for JsonWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.check()?;
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|end| end > self.maximum)
        {
            let cause = self
                .overflow
                .take()
                .expect("first overflow retains its selected cause");
            return Err(self.fail(cause));
        }
        self.bytes
            .try_reserve_exact(bytes.len())
            .map_err(|_| self.fail(HookContextError::Instruction(InstructionError::Allocation)))?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        // Never retry persistent cancellation, including writes of an empty chunk.
        self.write(bytes).map(|_| ())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.check()
    }
}
fn encode_memories(
    original: &SnapshotContext,
    memories: &[Memory],
    input: &ProjectHookInput<'_>,
) -> Result<String, HookContextError> {
    original.check().map_err(HookContextError::Request)?;
    encode_json(original, &ProjectMemoryRows(memories), input)
}
fn encode_json(
    original: &SnapshotContext,
    value: &impl Serialize,
    input: &ProjectHookInput<'_>,
) -> Result<String, HookContextError> {
    original.check().map_err(HookContextError::Request)?;
    let mut writer = JsonWriter::new(original, input);
    if let Err(cause) = serde_json::to_writer(&mut writer, value) {
        return Err(writer.first.take().unwrap_or(HookContextError::Json(cause)));
    }
    original.check().map_err(HookContextError::Request)?;
    // Successful serde_json writes are UTF-8. Moving this buffer copies no body.
    Ok(String::from_utf8(writer.bytes).expect("successful JSON encoding is UTF-8"))
}
fn classification(class: MemoryClass) -> &'static str {
    match class {
        MemoryClass::Fact => "fact",
        MemoryClass::Skill => "skill",
        MemoryClass::Tension => "tension",
        MemoryClass::Decision => "decision",
        MemoryClass::Convention => "convention",
        MemoryClass::Correction => "correction",
    }
}
fn render(
    set: PolicyTemplateSet,
    variables: &[InstructionVariable<'_>],
    input: &ProjectHookInput<'_>,
    original: &RequestContext<'_>,
    sources: Option<&crate::instruction_overrides::InstructionSources<'_>>,
) -> Result<String, HookContextError> {
    if let Some(sources) = sources {
        return sources
            .render(
                set.entry(),
                set.candidates(),
                variables,
                input.instruction_limits,
                original,
            )
            .map(|value| value.into_body())
            .map_err(HookContextError::Instruction);
    }
    render_instruction(
        set.entry(),
        set.candidates(),
        variables,
        input.instruction_limits,
        original.snapshot(),
    )
    .map(|value| value.into_body())
    .map_err(HookContextError::Instruction)
}

fn check_version(
    admission: &ProjectReadAdmission<'_>,
    input: &ProjectHookInput<'_>,
) -> Result<(), HookContextError> {
    let version = admission
        .project()
        .map_err(HookContextError::Auth)?
        .version
        .version();
    if !input.variables.iter().any(|v| {
        v.name == "project_version"
            && matches!(v.value, InstructionValue::Text(text) if text == version)
    }) {
        return Err(HookContextError::ProjectVersionMismatch);
    }
    Ok(())
}

fn check_project_pair(
    memory: &ProjectReadAdmission<'_>,
    task: &ProjectReadAdmission<'_>,
) -> Result<(), HookContextError> {
    let memory = memory.project().map_err(HookContextError::Auth)?;
    let task = task.project().map_err(HookContextError::Auth)?;
    if memory.scope.live.owner() != task.scope.live.owner()
        || memory.scope.live.owner_root() != task.scope.live.owner_root()
        || memory.scope.live.runtime_root() != task.scope.live.runtime_root()
        || memory.scope.live.brand().key() != task.scope.live.brand().key()
        || memory.version.version() != task.version.version()
        || memory.version.lance_branch() != task.version.lance_branch()
    {
        return Err(HookContextError::ProjectScopeMismatch);
    }
    Ok(())
}

/// Returns a complete native codec payload; the caller consumes its official channel.
/// Kiro uses the current native V3 profile hooks.
pub async fn render_project_hook(
    original: &RequestContext<'_>,
    input: ProjectHookInput<'_>,
    memory: Option<(&ProjectReadAdmission<'_>, MemoryStoreReadLimits)>,
    task: Option<(&ProjectReadAdmission<'_>, TaskStoreReadLimits)>,
) -> Result<EncodedOutput, HookContextError> {
    render_captured_hook(original, input, memory, task, None, None).await
}

pub(crate) async fn render_captured_hook(
    original: &RequestContext<'_>,
    input: ProjectHookInput<'_>,
    memory: Option<(&ProjectReadAdmission<'_>, MemoryStoreReadLimits)>,
    task: Option<(&ProjectReadAdmission<'_>, TaskStoreReadLimits)>,
    user: Option<UserContext<'_>>,
    sources: Option<&crate::instruction_overrides::InstructionSources<'_>>,
) -> Result<EncodedOutput, HookContextError> {
    original.remaining().map_err(HookContextError::Request)?;
    input
        .event
        .native_name(input.binding)
        .map_err(|_| HookContextError::UnsupportedEvent)?;
    let output = OutputContext::new(input.output_limits, original.snapshot())
        .map_err(HookContextError::Output)?;
    if !input
        .event
        .accepts_context(input.binding)
        .map_err(HookContextError::Output)?
    {
        return encode(input.binding, input.event, OutputMessage::Quiet, &output)
            .map_err(HookContextError::Output);
    }

    if user.is_some() && !matches!(input.audience, MandateAudience::Coordinator) {
        return Err(HookContextError::UserRecipientDenied);
    }
    if let Some(selection) = input.task_context {
        if !matches!(input.audience, MandateAudience::Coordinator) {
            return Err(HookContextError::TaskRecipientDenied);
        }
        selection.validate(original)?;
    }
    if !input.modules.enabled(Module::Hooks) {
        return encode(input.binding, input.event, OutputMessage::Quiet, &output)
            .map_err(HookContextError::Output);
    }
    if input.variables.iter().any(|v| {
        matches!(
            v.name,
            "module_mandates"
                | "project_memory_state"
                | "project_memory_json"
                | "project_task_state"
                | "project_task_json"
                | "user_memory_state"
                | "user_memory_json"
        )
    }) {
        return Err(HookContextError::ReservedVariable);
    }
    if input.modules.enabled(Module::Memory) {
        let (memory_admission, _) = memory.ok_or(HookContextError::MissingMemoryAdmission)?;
        if input.task_context.is_some() && input.modules.enabled(Module::Task) {
            let (task_admission, _) = task.ok_or(HookContextError::MissingTaskAdmission)?;
            check_project_pair(memory_admission, task_admission)?;
        }
    }
    let (task_state, task_json, task_admission) = if let Some(selection) = input.task_context {
        if input.modules.enabled(Module::Task) {
            let (admission, limits) = task.ok_or(HookContextError::MissingTaskAdmission)?;
            check_version(admission, &input)?;
            let context = task::read(original, selection, admission, limits).await?;
            (
                "loaded",
                encode_json(original.snapshot(), &context, &input)?,
                Some(admission),
            )
        } else {
            ("disabled", String::new(), None)
        }
    } else {
        ("unselected", String::new(), None)
    };
    let (state, memories, admission) = if input.modules.enabled(Module::Memory) {
        let (admission, limits) = memory.ok_or(HookContextError::MissingMemoryAdmission)?;
        check_version(admission, &input)?;
        match NativeMemoryStore::list(
            original.snapshot(),
            admission,
            MemoryListFilter {
                mandatory: Some(true),
                ..Default::default()
            },
            limits,
        )
        .await
        .map_err(|e| HookContextError::Memory(Box::new(e)))?
        {
            MemoryListRead::NoCorpus => ("no_corpus", Vec::new(), Some(admission)),
            MemoryListRead::Published { memories, .. } => (
                if memories.is_empty() {
                    "empty"
                } else {
                    "loaded"
                },
                memories,
                Some(admission),
            ),
        }
    } else {
        ("disabled", Vec::new(), None)
    };
    let (user_state, user_memories) = match user {
        None => ("unselected", &[][..]),
        Some(_) if !input.modules.enabled(Module::Memory) => ("disabled", &[][..]),
        Some(UserContext::Unavailable) => ("unavailable", &[][..]),
        Some(UserContext::Captured(read)) => user::projection(read),
    };
    let corpus = encode_memories(original.snapshot(), &memories, &input)?;
    let user_corpus = encode_memories(original.snapshot(), user_memories, &input)?;
    let mut mandates = Vec::new();
    for module in [
        Module::Task,
        Module::Memory,
        Module::Ast,
        Module::Hub,
        Module::Knowledge,
    ] {
        if input.modules.enabled(module) {
            // The closed list above consists solely of supported compiled mandates.
            let set = module_mandate(module, input.audience).expect("supported policy module");
            let body = render(set, input.variables, &input, original, sources)?;
            let tag = set.module_tag().expect("module mandate tag");
            mandates.push(format!("<{tag}>{body}</{tag}>"));
        }
    }
    let texts: Vec<_> = mandates.iter().map(String::as_str).collect();
    let mut variables: Vec<_> = input
        .variables
        .iter()
        .map(|v| InstructionVariable {
            name: v.name,
            value: v.value,
        })
        .collect();
    variables.extend([
        InstructionVariable {
            name: "module_mandates",
            value: InstructionValue::TextList(&texts),
        },
        InstructionVariable {
            name: "project_memory_state",
            value: InstructionValue::Text(state),
        },
        InstructionVariable {
            name: "project_memory_json",
            value: InstructionValue::Text(&corpus),
        },
        InstructionVariable {
            name: "user_memory_state",
            value: InstructionValue::Text(user_state),
        },
        InstructionVariable {
            name: "user_memory_json",
            value: InstructionValue::Text(&user_corpus),
        },
        InstructionVariable {
            name: "project_task_state",
            value: InstructionValue::Text(task_state),
        },
        InstructionVariable {
            name: "project_task_json",
            value: InstructionValue::Text(&task_json),
        },
    ]);
    let body = render(
        policy_prompt(PolicyPrompt::ProjectHookContext),
        &variables,
        &input,
        original,
        sources,
    )?;
    let encoded = encode(
        input.binding,
        input.event,
        OutputMessage::Context(&body),
        &output,
    )
    .map_err(HookContextError::Output)?;
    if let Some(admission) = admission {
        admission.recheck().map_err(HookContextError::Auth)?;
    }
    if let Some(admission) = task_admission {
        admission.recheck().map_err(HookContextError::Auth)?;
    }
    original.remaining().map_err(HookContextError::Request)?;
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};
    use graphit_records::memory_attributes::MemoryAttributes;
    use std::{
        io::Write,
        num::{NonZeroU64, NonZeroU128, NonZeroUsize},
        time::{Duration, Instant},
    };

    fn input(modules: &ModuleSettings, cap: usize) -> ProjectHookInput<'_> {
        ProjectHookInput {
            task_context: None,
            binding: NativeBinding::KiroV3,
            event: HookEvent::SessionStart,
            modules,
            audience: MandateAudience::Coordinator,
            variables: &[],
            instruction_limits: InstructionLimits {
                max_templates: NonZeroUsize::new(32).unwrap(),
                max_template_bytes: NonZeroUsize::new(65536).unwrap(),
                max_values: NonZeroUsize::new(128).unwrap(),
                max_variable_bytes: NonZeroUsize::new(cap).unwrap(),
                max_output_bytes: NonZeroUsize::new(cap * 2).unwrap(),
                fuel: NonZeroU64::new(100000).unwrap(),
                recursion: NonZeroUsize::new(32).unwrap(),
            },
            output_limits: OutputLimits {
                max_context_bytes: cap * 2,
                max_output_bytes: cap * 2,
            },
        }
    }
    fn modules() -> ModuleSettings {
        ModuleSettings::resolve(&ConfigSnapshot::new(&Brand::default(), []).unwrap()).unwrap()
    }
    fn row() -> Memory {
        Memory {
            id: MemoryId::new(NonZeroU128::new(1).unwrap()),
            revision: 1,
            title: "quote \" α".into(),
            body: "line\n\0\\ {{ data }}".into(),
            attributes: MemoryAttributes {
                classification: MemoryClass::Convention,
                mandatory: true,
                important: false,
            },
            tags: Vec::new(),
            references: Vec::new(),
            reference_generation: 1,
            created_by: String::new(),
            updated_by: String::new(),
            created_at: 1,
            updated_at: 1,
        }
    }
    #[test]
    fn borrowed_json_exact_escaped_utf8_cap_and_all_four_minima() {
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(10),
            Cancellation::default(),
        );
        let modules = modules();
        let rows = [row()];
        let expected = r#"[{"id":"00000000000000000000000001","revision":1,"title":"quote \" α","body":"line\n\u0000\\ {{ data }}","classification":"convention","mandatory":true,"important":false}]"#;
        assert_eq!(
            encode_memories(&original, &rows, &input(&modules, expected.len())).unwrap(),
            expected
        );
        for selected in 0..4 {
            let cap = expected.len() - 1;
            let mut limits = input(&modules, cap * 2);
            match selected {
                0 => limits.instruction_limits.max_variable_bytes = NonZeroUsize::new(cap).unwrap(),
                1 => limits.instruction_limits.max_output_bytes = NonZeroUsize::new(cap).unwrap(),
                2 => limits.output_limits.max_context_bytes = cap,
                _ => limits.output_limits.max_output_bytes = cap,
            }
            let cause = encode_memories(&original, &rows, &limits).unwrap_err();
            match selected {
                0 => assert!(matches!(
                    cause,
                    HookContextError::Instruction(InstructionError::LimitExceeded(
                        InstructionLimit::VariableBytes
                    ))
                )),
                1 => assert!(matches!(
                    cause,
                    HookContextError::Instruction(InstructionError::LimitExceeded(
                        InstructionLimit::OutputBytes
                    ))
                )),
                _ => assert!(matches!(
                    cause,
                    HookContextError::Output(OutputError::LimitExceeded)
                )),
            }
        }
    }
    #[test]
    fn failed_append_keeps_prefix_capacity_and_first_cause_on_ties() {
        let cancel = Cancellation::default();
        let original =
            SnapshotContext::new(Instant::now() + Duration::from_secs(10), cancel.clone());
        let modules = modules();
        let mut limits = input(&modules, 2);
        limits.instruction_limits.max_output_bytes = NonZeroUsize::new(2).unwrap();
        limits.output_limits.max_context_bytes = 2;
        limits.output_limits.max_output_bytes = 2;
        let mut writer = JsonWriter::new(&original, &limits);
        writer.write_all(b"[]").unwrap();
        let capacity = writer.bytes.capacity();
        assert!(writer.write_all(b"oversized").is_err());
        cancel.cancel();
        assert!(writer.write_all(b"").is_err());
        assert_eq!(writer.bytes, b"[]");
        assert_eq!(writer.bytes.capacity(), capacity);
        assert!(matches!(
            writer.first,
            Some(HookContextError::Instruction(
                InstructionError::LimitExceeded(InstructionLimit::VariableBytes)
            ))
        ));
    }
    #[test]
    fn cancelled_and_expired_original_fail_before_json_append() {
        let modules = modules();
        let limits = input(&modules, 1024);
        let cancel = Cancellation::default();
        let original =
            SnapshotContext::new(Instant::now() + Duration::from_secs(10), cancel.clone());
        let mut writer = JsonWriter::new(&original, &limits);
        cancel.cancel();
        assert!(writer.write_all(b"x").is_err());
        assert!(writer.bytes.is_empty());
        assert!(
            matches!(writer.first, Some(HookContextError::Request(ref e)) if e.kind() == io::ErrorKind::Interrupted)
        );
        let original = SnapshotContext::new(
            Instant::now() - Duration::from_secs(1),
            Cancellation::default(),
        );
        assert!(
            matches!(encode_memories(&original, &[row()], &limits), Err(HookContextError::Request(e)) if e.kind() == io::ErrorKind::TimedOut)
        );
        let mut writer = JsonWriter::new(&original, &limits);
        assert!(writer.write_all(b"x").is_err());
        assert!(writer.bytes.is_empty());
    }
}
