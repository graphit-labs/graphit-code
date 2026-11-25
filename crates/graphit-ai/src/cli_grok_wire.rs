//! Grok's native streaming-json frames; opaque payloads remain borrowed.
pub use crate::cli_codex_wire::CodexWireLimits as GrokWireLimits;
use crate::cli_codex_wire::{CodexWireError, audit_json_frame};
use graphit_core::file_io::SnapshotContext;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{borrow::Cow, fmt, io};

#[derive(Deserialize)]
struct Discriminator<'a> {
    #[serde(borrow, rename = "type")]
    kind: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct GrokText<'a> {
    #[serde(borrow)]
    pub data: Cow<'a, str>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrokEnd<'a> {
    #[serde(borrow)]
    pub stop_reason: Cow<'a, str>,
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow)]
    pub request_id: Cow<'a, str>,
    #[serde(borrow, default)]
    pub usage: Option<&'a RawValue>,
    #[serde(borrow, default)]
    pub model_usage: Option<&'a RawValue>,
}
#[derive(Deserialize)]
pub struct GrokError<'a> {
    #[serde(borrow)]
    pub message: Cow<'a, str>,
}
pub enum GrokEventData<'a> {
    Text(GrokText<'a>),
    End(GrokEnd<'a>),
    Error(GrokError<'a>),
    MaxTurnsReached,
    Progress,
}
pub struct GrokEvent<'a> {
    pub native: &'a RawValue,
    pub data: GrokEventData<'a>,
}
pub type GrokEventView<'a> = &'a GrokEvent<'a>;
pub enum GrokWireError {
    Context(io::Error),
    Audit(CodexWireError),
    Json(serde_json::Error),
    Schema,
}
impl fmt::Debug for GrokWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(error) => f.debug_tuple("Context").field(&error.kind()).finish(),
            Self::Audit(error) => f.debug_tuple("Audit").field(error).finish(),
            Self::Json(error) => f
                .debug_struct("Json")
                .field("category", &error.classify())
                .field("line", &error.line())
                .field("column", &error.column())
                .finish(),
            Self::Schema => f.write_str("Schema"),
        }
    }
}
impl fmt::Display for GrokWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for GrokWireError {}
fn parse<'a, T: Deserialize<'a>>(native: &'a RawValue) -> Result<T, GrokWireError> {
    serde_json::from_str(native.get()).map_err(GrokWireError::Json)
}
fn identifier(value: &str, limits: &GrokWireLimits) -> Result<(), GrokWireError> {
    if value.is_empty() || value.len() > limits.id_bytes {
        Err(GrokWireError::Schema)
    } else {
        Ok(())
    }
}
fn shape(value: &RawValue, opening: char) -> Result<(), GrokWireError> {
    if value.get().starts_with(opening) {
        Ok(())
    } else {
        Err(GrokWireError::Schema)
    }
}
pub(crate) fn validate_grok_limits(limits: &GrokWireLimits) -> Result<(), GrokWireError> {
    limits.validate().map_err(GrokWireError::Audit)
}
pub fn decode_grok_frame<'a>(
    frame: &'a [u8],
    limits: &GrokWireLimits,
    original: &SnapshotContext,
) -> Result<Option<GrokEvent<'a>>, GrokWireError> {
    original.check().map_err(GrokWireError::Context)?;
    validate_grok_limits(limits)?;
    audit_json_frame(frame, limits, original).map_err(GrokWireError::Audit)?;
    let native: &RawValue = serde_json::from_slice(frame).map_err(GrokWireError::Json)?;
    let discriminator: Discriminator<'_> = parse(native)?;
    let data = match discriminator.kind.as_ref() {
        "text" => GrokEventData::Text(parse(native)?),
        "thought" => {
            let _: GrokText<'_> = parse(native)?;
            GrokEventData::Progress
        }
        "end" => {
            let end: GrokEnd<'_> = parse(native)?;
            identifier(&end.stop_reason, limits)?;
            identifier(&end.session_id, limits)?;
            // Upstream emits an empty requestId when its prompt response lacks one.
            if end.request_id.len() > limits.id_bytes {
                return Err(GrokWireError::Schema);
            }
            for value in [end.usage, end.model_usage].into_iter().flatten() {
                shape(value, '{')?;
            }
            GrokEventData::End(end)
        }
        "error" => GrokEventData::Error(parse(native)?),
        "max_turns_reached" => GrokEventData::MaxTurnsReached,
        "tool_call" | "tool_call_update" => {
            #[derive(Deserialize)]
            struct Tool<'a> {
                #[serde(borrow, rename = "toolCallId")]
                id: Cow<'a, str>,
                #[serde(borrow)]
                content: &'a RawValue,
                #[serde(borrow)]
                locations: &'a RawValue,
            }
            let tool: Tool<'_> = parse(native)?;
            identifier(&tool.id, limits)?;
            shape(tool.content, '[')?;
            shape(tool.locations, '[')?;
            GrokEventData::Progress
        }
        "usage" => {
            #[derive(Deserialize)]
            struct Usage<'a> {
                #[serde(borrow, default)]
                usage: Option<&'a RawValue>,
            }
            if let Some(usage) = parse::<Usage<'_>>(native)?.usage {
                shape(usage, '{')?;
            }
            GrokEventData::Progress
        }
        "plan"
        | "available_commands"
        | "auto_compact_started"
        | "auto_compact_completed"
        | "auto_compact_failed"
        | "auto_compact_cancelled"
        | "auto_continue_completed"
        | "image_compressed"
        | "memory_flush_started"
        | "memory_flush_completed"
        | "memory_capture_activity" => GrokEventData::Progress,
        _ => return Ok(None),
    };
    original.check().map_err(GrokWireError::Context)?;
    Ok(Some(GrokEvent { native, data }))
}
