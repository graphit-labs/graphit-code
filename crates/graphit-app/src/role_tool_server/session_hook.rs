//! Memory-only Kiro V3 context under the already verified Role grant.
use super::{State, ToolResult, error, parse};
use crate::{
    project_hook_context::ProjectMemoryRows,
    session_hook_entry::{Engine, Event, binding_event},
};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, VerifiedRoleGrant};
use graphit_core::{
    config::{ConfigSnapshot, ConfigSource, ConfigStore},
    file_io::SnapshotContext,
    module_settings::{Module, ModuleSettings},
    project_paths::ProjectPaths,
};
use graphit_hub::{
    native::{NativeBinding, Role},
    policy::RequestContext,
};
use graphit_records::memory_store::{MemoryListFilter, MemoryListRead, NativeMemoryStore};
use graphit_runtime::{OutputContext, OutputLimits, OutputMessage, encode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Write};

#[derive(Serialize)]
struct ProjectMemory<'a> {
    project_version: &'a str,
    memory_state: &'static str,
    memories: ProjectMemoryRows<'a>,
}
struct ContextWriter<'a> {
    original: &'a SnapshotContext,
    maximum: usize,
    bytes: Vec<u8>,
    first: Option<io::Error>,
}
impl ContextWriter<'_> {
    fn fail(&mut self, cause: io::Error) -> io::Error {
        if self.first.is_none() {
            self.first = Some(cause);
        }
        io::ErrorKind::InvalidData.into()
    }
}
impl Write for ContextWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.first.is_some() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if let Err(error) = self.original.check() {
            return Err(self.fail(error));
        }
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|length| length > self.maximum)
        {
            return Err(self.fail(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Role context exceeds captured byte limit",
            )));
        }
        if self.bytes.try_reserve_exact(bytes.len()).is_err() {
            return Err(self.fail(io::ErrorKind::OutOfMemory.into()));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.write(bytes).map(|_| ())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.original.check()
    }
}
pub(crate) fn compose_memory(
    version: &str,
    state: &'static str,
    memories: ProjectMemoryRows<'_>,
    prompt: Option<&str>,
    maximum: usize,
    original: &SnapshotContext,
) -> io::Result<String> {
    original.check()?;
    let mut writer = ContextWriter {
        original,
        maximum,
        bytes: Vec::new(),
        first: None,
    };
    let result = (|| -> io::Result<()> {
        writer.write_all(b"[Standing context]\nPROJECT mandatory Memory:\n")?;
        serde_json::to_writer(
            &mut writer,
            &ProjectMemory {
                project_version: version,
                memory_state: state,
                memories,
            },
        )
        .map_err(io::Error::other)?;
        if let Some(prompt) = prompt {
            writer.write_all(b"\n\n")?;
            writer.write_all(prompt.as_bytes())?;
        }
        writer.flush()
    })();
    if let Err(cause) = result {
        return Err(writer.first.take().unwrap_or(cause));
    }
    original.check()?;
    Ok(String::from_utf8(writer.bytes).expect("JSON and prompt are UTF-8"))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    engine: Engine,
    event: Event,
}
pub(super) async fn read(
    state: &State,
    paths: &ProjectPaths,
    recipient: Role,
    grant: &VerifiedRoleGrant<'_>,
    input: Option<Value>,
    original: &RequestContext<'_>,
) -> ToolResult {
    let input: Input = parse(input)?;
    let (binding, event) = binding_event(input.engine, input.event);
    if binding != NativeBinding::KiroV3
        || state.task.binding != binding
        || !matches!(recipient, Role::Scout | Role::Tracker | Role::Scribe)
    {
        return Err(error("invalid_hook_recipient"));
    }
    original
        .snapshot()
        .check()
        .map_err(|_| error("request_cancelled_or_expired"))?;
    grant
        .actor()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let base = &state.policy.config;
    let layers = [
        ConfigSource::Command,
        ConfigSource::Environment,
        ConfigSource::Distribution,
    ]
    .into_iter()
    .filter_map(|kind| base.layer(kind).cloned())
    .chain([
        ConfigStore::global(&state.policy.source.installation)
            .and_then(|store| store.load())
            .map_err(|_| error("hook_configuration_failed"))?,
        ConfigStore::project(paths)
            .and_then(|store| store.load())
            .map_err(|_| error("hook_configuration_failed"))?,
    ]);
    let modules = ModuleSettings::resolve(
        &ConfigSnapshot::new(base.brand(), layers)
            .map_err(|_| error("hook_configuration_failed"))?,
    )
    .map_err(|_| error("hook_configuration_failed"))?;
    original
        .snapshot()
        .check()
        .map_err(|_| error("request_cancelled_or_expired"))?;
    grant
        .actor()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let limits = OutputLimits {
        max_context_bytes: state.policy.source.content.max_output_bytes,
        max_output_bytes: state.policy.source.content.max_output_bytes,
    };
    let output =
        OutputContext::new(limits, original.snapshot()).map_err(|_| error("hook_output_failed"))?;
    let text = if recipient == Role::Tracker
        || !modules.enabled(Module::Hooks)
        || !modules.enabled(Module::Memory)
    {
        None
    } else {
        let authority = ProjectRecordAuthority::new(state.auth.issuer());
        let admission = authority
            .admit_read(grant, original, ProjectReadModule::Memory)
            .map_err(|_| error("authentication_or_permission_denied"))?;
        let (memory_state, memories) = match NativeMemoryStore::list(
            original.snapshot(),
            &admission,
            MemoryListFilter {
                mandatory: Some(true),
                ..Default::default()
            },
            state.policy.memory_read,
        )
        .await
        .map_err(|cause| match cause {
            graphit_records::memory_store::MemoryStoreError::Request(cause)
                if matches!(
                    cause.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
                ) =>
            {
                error("request_cancelled_or_expired")
            }
            graphit_records::memory_store::MemoryStoreError::Auth(_) => {
                error("authentication_or_permission_denied")
            }
            _ => error("memory_read_failed"),
        })? {
            MemoryListRead::NoCorpus => ("no_corpus", Vec::new()),
            MemoryListRead::Published { memories, .. } => ("published", memories),
        };
        let capture = admission
            .project()
            .map_err(|_| error("authentication_or_permission_denied"))?;
        let text = compose_memory(
            capture.version.version(),
            memory_state,
            ProjectMemoryRows(&memories),
            None,
            limits.max_context_bytes,
            original.snapshot(),
        )
        .map_err(|cause| match cause.kind() {
            io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => {
                error("request_cancelled_or_expired")
            }
            _ => error("hook_context_failed"),
        })?;
        admission
            .recheck()
            .map_err(|_| error("authentication_or_permission_denied"))?;
        Some(text)
    };
    let message = text
        .as_deref()
        .map_or(OutputMessage::Quiet, OutputMessage::Context);
    let bytes = encode(binding, event, message, &output)
        .map_err(|_| error("hook_output_failed"))?
        .into_bytes();
    grant
        .actor()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    original
        .snapshot()
        .check()
        .map_err(|_| error("request_cancelled_or_expired"))?;
    Ok(Value::String(
        String::from_utf8(bytes).map_err(|_| error("hook_output_failed"))?,
    ))
}
