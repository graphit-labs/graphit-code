//! CLI transport for the native host hook context.
mod input;
use crate::project_hook_context::TaskContextSelection;
use graphit_core::{
    native_document_file::NativeDocumentFileLimits, native_path::NativePath,
    project_paths::InstallationPaths,
};
use graphit_hub::{
    native::NativeBinding,
    policy::{HubResourcePolicy, RequestContext, ResourceSetting},
};
use graphit_runtime::HookEvent;
pub use input::{read as read_host_input, read_stdin as read_host_stdin};
use rmcp::model::CallToolRequestParams;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, clap::ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Codex,
    Claude,
    Cursor,
    #[value(name = "opencode")]
    OpenCode,
    Kiro,
    Qwen,
    Kimi,
    Antigravity,
}
impl Engine {
    pub const ALL: [Self; 8] = [
        Self::Codex,
        Self::Claude,
        Self::Cursor,
        Self::OpenCode,
        Self::Kiro,
        Self::Qwen,
        Self::Kimi,
        Self::Antigravity,
    ];
    pub const fn binding(self) -> NativeBinding {
        match self {
            Self::Codex => NativeBinding::Codex,
            Self::Claude => NativeBinding::Claude,
            Self::Cursor => NativeBinding::Cursor,
            Self::OpenCode => NativeBinding::OpenCode,
            Self::Kiro => NativeBinding::KiroV3,
            Self::Qwen => NativeBinding::Qwen,
            Self::Kimi => NativeBinding::Kimi,
            Self::Antigravity => NativeBinding::Antigravity,
        }
    }
}

#[derive(Clone, Copy, Debug, clap::ValueEnum, Deserialize, Serialize)]
pub enum Event {
    #[value(name = "SessionStart")]
    SessionStart,
    #[value(name = "UserPromptSubmit")]
    UserPromptSubmit,
    #[value(name = "SubagentStart")]
    SubagentStart,
    #[value(name = "BeforeAgent")]
    BeforeAgent,
    #[value(name = "PostToolUse")]
    PostToolUse,
    #[value(name = "AfterTool")]
    AfterTool,
    #[value(name = "PreInvocation")]
    PreInvocation,
    #[value(name = "PostInvocation")]
    PostInvocation,
    #[value(name = "Stop")]
    Stop,
    #[value(name = "SessionEnd")]
    SessionEnd,
    #[value(name = "SystemTransform")]
    SystemTransform,
    #[value(name = "SessionCompacting")]
    SessionCompacting,
}
impl Event {
    pub const ALL: [Self; 12] = [
        Self::SessionStart,
        Self::UserPromptSubmit,
        Self::SubagentStart,
        Self::BeforeAgent,
        Self::PostToolUse,
        Self::AfterTool,
        Self::PreInvocation,
        Self::PostInvocation,
        Self::Stop,
        Self::SessionEnd,
        Self::SystemTransform,
        Self::SessionCompacting,
    ];
    pub const fn semantic(self) -> HookEvent {
        match self {
            Self::SessionStart => HookEvent::SessionStart,
            Self::UserPromptSubmit => HookEvent::UserPromptSubmit,
            Self::SubagentStart => HookEvent::SubagentStart,
            Self::BeforeAgent => HookEvent::BeforeAgent,
            Self::PostToolUse => HookEvent::PostToolUse,
            Self::AfterTool => HookEvent::AfterTool,
            Self::PreInvocation => HookEvent::PreInvocation,
            Self::PostInvocation => HookEvent::PostInvocation,
            Self::Stop => HookEvent::Stop,
            Self::SessionEnd => HookEvent::SessionEnd,
            Self::SystemTransform => HookEvent::SystemTransform,
            Self::SessionCompacting => HookEvent::SessionCompacting,
        }
    }
}

#[derive(Clone, Copy, Default, clap::ValueEnum, PartialEq, Eq)]
pub enum Recipient {
    #[default]
    Coordinator,
    Role,
}

#[derive(clap::Args)]
pub struct Arguments {
    /// Explicitly select the authenticated managed-role transport.
    #[arg(long, value_enum, default_value = "coordinator")]
    pub recipient: Recipient,
    #[arg(long, value_enum)]
    pub engine: Engine,
    #[arg(long, value_enum)]
    pub event: Event,
    /// Select a retained local project; this path grants no access.
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    /// Explicit Graphit session to recover; host session IDs grant no access.
    #[arg(long)]
    pub task_session: Option<String>,
    /// Explicit Graphit task in the selected session. Repeat to select more tasks.
    #[arg(long = "task-id", requires = "task_session")]
    pub task_ids: Vec<String>,
}

pub(crate) fn binding_event(engine: Engine, event: Event) -> (NativeBinding, HookEvent) {
    (engine.binding(), event.semantic())
}

#[derive(Serialize)]
struct Input<'a> {
    engine: Engine,
    event: Event,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_dir: Option<NativePath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_context: Option<TaskContextSelection>,
}

pub fn prepare(
    installation: &InstallationPaths,
    arguments: &Arguments,
    version: Option<&str>,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.snapshot().required_deadline()?;
    let (binding, event) = binding_event(arguments.engine, arguments.event);
    event.native_name(binding).map_err(io::Error::other)?;
    if arguments.recipient == Recipient::Role
        && (arguments.task_session.is_some() || !arguments.task_ids.is_empty())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Task hook context requires the Coordinator recipient",
        ));
    }
    let task_context = match &arguments.task_session {
        Some(session_id) => {
            let selection: TaskContextSelection = serde_json::from_value(serde_json::json!({
                "session_id": session_id, "task_ids": arguments.task_ids,
            }))
            .map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "Task hook selection invalid")
            })?;
            selection.validate(original).map_err(io::Error::other)?;
            Some(selection)
        }
        None if arguments.task_ids.is_empty() => None,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Task IDs require a Graphit session",
            ));
        }
    };
    // The managed-role launcher owns these transient transport inputs. Presence
    // only denies Root loading; it never authenticates a Role or grants access.
    if arguments.recipient == Recipient::Coordinator
        && [
            "GRAPHIT_ROLE_BEARER",
            "GRAPHIT_ROLE_REFERENCE",
            "GRAPHIT_ROLE_SELECTION",
        ]
        .iter()
        .any(|key| std::env::var_os(key).is_some())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Root session hook is unavailable in a managed-role transport context",
        ));
    }
    if arguments.recipient == Recipient::Role
        && [
            "GRAPHIT_ROLE_BEARER",
            "GRAPHIT_ROLE_REFERENCE",
            "GRAPHIT_ROLE_SELECTION",
        ]
        .iter()
        .any(|key| std::env::var_os(key).is_none())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Role hook requires complete run transport",
        ));
    }
    let serde_json::Value::Object(input) = serde_json::to_value(Input {
        engine: arguments.engine,
        event: arguments.event,
        project_dir: arguments
            .project_dir
            .clone()
            .map(NativePath::new)
            .transpose()?,
        project_version: version,
        task_context,
    })
    .map_err(io::Error::other)?
    else {
        unreachable!("typed hook input object")
    };
    if arguments.recipient == Recipient::Role {
        let arguments = serde_json::json!({
            "selection": {"project_dir": input.get("project_dir"), "project_version": input.get("project_version")},
            "input": {"engine":input["engine"], "event":input["event"]},
        });
        let name = installation.brand().mcp_tool_name(&["session", "hook"])?;
        Ok(CallToolRequestParams::new(name)
            .with_arguments(arguments.as_object().expect("typed hook envelope").clone()))
    } else {
        Ok(CallToolRequestParams::new("graphit_session_hook").with_arguments(input))
    }
}

pub async fn call(
    installation: &InstallationPaths,
    recipient: Recipient,
    prepared: CallToolRequestParams,
    native: NativeDocumentFileLimits,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
    output: &mut impl Write,
) -> io::Result<()> {
    let deadline = original.snapshot().required_deadline()?;
    let address = crate::source_entry::address(installation, native, original).await?;
    let client = reqwest::Client::new();
    let result = if recipient == Recipient::Role {
        let proxy = crate::role_proxy::RoleProxy::from_environment(
            address,
            client,
            policy.content(),
            policy.projection(),
            std::time::Duration::from_millis(
                policy.limit(ResourceSetting::RequestDeadlineMs).value,
            ),
            original,
        )
        .await?;
        let result = proxy.call_tool_original(prepared, original).await;
        let joined = proxy.shutdown().await;
        let result = result?;
        joined?;
        result
    } else {
        let connection =
            crate::ast_source::connect_source(installation, address, &client, native, original, ())
                .await?;
        let result = tokio::select! {
            result = connection.peer().call_tool(prepared) => result.map_err(io::Error::other),
            () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(deadline.into()) => Err(io::ErrorKind::TimedOut.into()),
        };
        let joined = connection.cancel().await.map_err(io::Error::other);
        let result = result?;
        joined?;
        result
    };
    original.snapshot().check()?;
    if result.is_error == Some(true) {
        return Err(io::Error::other(
            "Session hook service rejected the request",
        ));
    }
    let text = if let [content] = result.content.as_slice() {
        content.as_text()
    } else {
        None
    }
    .ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Session hook response requires one text payload",
        )
    })?;
    if text.text.len() > policy.content().max_output_bytes {
        return Err(io::ErrorKind::InvalidData.into());
    }
    output.write_all(text.text.as_bytes())?;
    output.flush()?;
    original.snapshot().check()
}
