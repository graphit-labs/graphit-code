//! Classification over the SDK's already decoded ACP notification parameters.
use agent_client_protocol::{UntypedMessage, schema::v1::SessionNotification};
use serde_json::Value;

/// This does not audit raw JSON, duplicate fields, or SDK frame allocation.
pub fn decode_notification(
    message: UntypedMessage,
    id_bytes: usize,
) -> Result<Option<SessionNotification>, KiroWireError> {
    let (method, params) = message.into_parts();
    if method != "session/update" {
        return Ok(None);
    }
    let Some(session) = params.get("sessionId").and_then(Value::as_str) else {
        // Preserve serde's actual required-field/type error for malformed envelopes.
        return serde_json::from_value(params)
            .map(Some)
            .map_err(KiroWireError::Decode);
    };
    if session.len() > id_bytes {
        return Err(KiroWireError::IdLimit);
    }
    let Some(update) = params.get("update").and_then(Value::as_object) else {
        return serde_json::from_value(params)
            .map(Some)
            .map_err(KiroWireError::Decode);
    };
    let Some(kind) = update.get("sessionUpdate").and_then(Value::as_str) else {
        return serde_json::from_value(params)
            .map(Some)
            .map_err(KiroWireError::Decode);
    };
    // The pinned stable SDK enum has no unknown variant. Unknown extension data
    // is ignored; a malformed known variant must still fail deserialization.
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
        return Ok(None);
    }
    for key in ["messageId", "toolCallId"] {
        if update
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|id| id.len() > id_bytes)
        {
            return Err(KiroWireError::IdLimit);
        }
    }
    serde_json::from_value(params)
        .map(Some)
        .map_err(KiroWireError::Decode)
}

pub enum KiroWireError {
    IdLimit,
    Decode(serde_json::Error),
}
impl std::fmt::Debug for KiroWireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::IdLimit => "IdLimit",
            Self::Decode(_) => "Decode",
        })
    }
}
