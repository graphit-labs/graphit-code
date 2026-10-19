//! OpenCode 2 ACP updates after the SDK has decoded the JSON-RPC envelope.
//! Raw JSON/frame auditing belongs to the caller's bounded transport, before the SDK.
use agent_client_protocol::{UntypedMessage, schema::v1::SessionNotification};
use serde_json::Value;

pub enum OpencodeUpdate {
    Known(Box<SessionNotification>),
    Child(Box<SessionNotification>),
    Unknown(Value),
}
impl OpencodeUpdate {
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::Known(event) | Self::Child(event) => Some(event.session_id.0.as_ref()),
            Self::Unknown(params) => params.get("sessionId").and_then(Value::as_str),
        }
    }
}
impl std::fmt::Debug for OpencodeUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Known(_) => "KnownUpdate",
            Self::Child(_) => "ChildUpdate",
            Self::Unknown(_) => "UnknownUpdate",
        })
    }
}
pub enum OpencodeWireError {
    Envelope,
    IdLimit,
    Decode(serde_json::Error),
}
impl std::fmt::Debug for OpencodeWireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Envelope => "Envelope",
            Self::IdLimit => "IdLimit",
            Self::Decode(_) => "Decode",
        })
    }
}
pub fn decode_update(
    message: UntypedMessage,
    id_bytes: usize,
) -> Result<Option<OpencodeUpdate>, OpencodeWireError> {
    let (method, params) = message.into_parts();
    if method != "session/update" {
        return Ok(None);
    }
    let session = params
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or(OpencodeWireError::Envelope)?;
    if session.is_empty() || session.contains('\0') {
        return Err(OpencodeWireError::Envelope);
    }
    if session.len() > id_bytes {
        return Err(OpencodeWireError::IdLimit);
    }
    let update = params
        .get("update")
        .and_then(Value::as_object)
        .ok_or(OpencodeWireError::Envelope)?;
    let child = update
        .get("_meta")
        .and_then(Value::as_object)
        .and_then(|meta| meta.get("opencode/child-session"));
    if let Some(child) = child {
        for key in ["id", "parentID"] {
            let id = child
                .get(key)
                .and_then(Value::as_str)
                .ok_or(OpencodeWireError::Envelope)?;
            if id.is_empty() || id.contains('\0') {
                return Err(OpencodeWireError::Envelope);
            }
            if id.len() > id_bytes {
                return Err(OpencodeWireError::IdLimit);
            }
        }
        if child.get("id").and_then(Value::as_str) == Some(session)
            || child
                .get("depth")
                .and_then(Value::as_u64)
                .is_none_or(|depth| depth == 0)
        {
            return Err(OpencodeWireError::Envelope);
        }
    }
    let is_child = child.is_some();
    let kind = update
        .get("sessionUpdate")
        .and_then(Value::as_str)
        .ok_or(OpencodeWireError::Envelope)?;
    for key in ["messageId", "toolCallId"] {
        if update
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| value.len() > id_bytes)
        {
            return Err(OpencodeWireError::IdLimit);
        }
    }
    if !matches!(
        kind,
        "user_message_chunk"
            | "agent_message_chunk"
            | "agent_thought_chunk"
            | "tool_call"
            | "tool_call_update"
            | "plan"
            | "available_commands_update"
            | "current_mode_update"
            | "config_option_update"
            | "session_info_update"
            | "usage_update"
            | "notice"
            | "compaction_update"
            | "compaction_summary_chunk"
    ) {
        return Ok(Some(OpencodeUpdate::Unknown(params)));
    }
    serde_json::from_value(params)
        .map(|event| {
            Some(if is_child {
                OpencodeUpdate::Child(Box::new(event))
            } else {
                OpencodeUpdate::Known(Box::new(event))
            })
        })
        .map_err(OpencodeWireError::Decode)
}

/// Audit the native JSON frame with the existing AI auditor before SDK decoding.
/// The caller retains its original bytes separately; audit statistics do not preserve lexemes.
pub fn audit_frame(
    frame: &[u8],
    limits: &crate::cli_codex_wire::CodexWireLimits,
    original: &graphit_core::file_io::SnapshotContext,
) -> Result<crate::cli_codex_wire::CodexWireStats, crate::cli_codex_wire::CodexWireError> {
    crate::cli_codex_wire::audit_json_frame(frame, limits, original)
}
