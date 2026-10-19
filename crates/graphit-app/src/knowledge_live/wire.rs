use graphit_hub::NativeBinding;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A selector, never a captured source or an authorization proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSelection {
    pub source: String,
    pub project_id: Option<String>,
    pub instance_id: Option<String>,
    pub project_kind: Option<String>,
    #[serde(rename = "type")]
    pub artifact_type: String,
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSelection {
    pub project_id: String,
    pub project_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    pub request_id: String,
    pub project: ProjectSelection,
    pub binding: NativeBinding,
    pub agent: ArtifactSelection,
    pub artifacts: Vec<ArtifactSelection>,
    pub title: Option<String>,
    pub prompt: Option<String>,
    pub model: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Preparing,
    Ready,
    Running,
    Failed,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCode {
    Cancelled,
    Deadline,
    PermissionDenied,
    Unavailable,
    InvalidContext,
    Native,
    Storage,
    Interrupted,
    CleanupIncomplete,
    Limit,
    Corrupt,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendRequest {
    pub turn_id: String,
    pub text: String,
}

/// Observed completion can survive a failed history write. It is not a replay event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownTerminal {
    pub turn_id: String,
    pub outcome: TurnOutcome,
    pub error: Option<FailureCode>,
    pub joined: bool,
    pub clean: bool,
    pub persisted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub id: String,
    pub project: ProjectSelection,
    pub binding: NativeBinding,
    pub agent: ArtifactSelection,
    pub artifacts: Vec<ArtifactSelection>,
    pub title: Option<String>,
    pub model: String,
    pub state: SessionState,
    pub cursor: ReplayCursor,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayCursor {
    pub generation: String,
    #[serde(with = "decimal_sequence")]
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    #[serde(with = "decimal_sequence")]
    pub sequence: u64,
    pub turn_id: Option<String>,
    pub data: EventData,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventData {
    State {
        state: SessionState,
    },
    Prep {
        message: String,
        artifact: Option<ArtifactSelection>,
    },
    Prompt {
        text: String,
    },
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    ToolUse {
        tool_call_id: String,
        name: Option<String>,
        title: String,
        input: Value,
    },
    ToolResult {
        tool_call_id: String,
        output: Value,
        is_error: bool,
    },
    Stdout {
        text: String,
    },
    Stderr {
        text: String,
    },
    Error {
        code: FailureCode,
    },
    TurnDone {
        outcome: TurnOutcome,
        joined: bool,
        clean: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replay {
    pub session: Session,
    pub events: Vec<Event>,
}

/// Decimal JSON strings preserve the complete u64 domain in browser clients.
mod decimal_sequence {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    pub(super) fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.is_empty()
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(D::Error::custom("invalid decimal sequence"));
        }
        value.parse().map_err(D::Error::custom)
    }
}
