//! Claude Code print-mode JSONL. Native payloads are borrowed and never diagnostics.
pub use crate::cli_codex_wire::CodexWireLimits as ClaudeWireLimits;
use crate::cli_codex_wire::{CodexWireError, audit_json_frame};
use graphit_core::file_io::SnapshotContext;
use serde::{
    Deserialize, Deserializer,
    de::{self, SeqAccess, Visitor},
};
use serde_json::value::RawValue;
use std::{borrow::Cow, fmt, io};

#[derive(Deserialize)]
struct Discriminator<'a> {
    #[serde(rename = "type", borrow)]
    kind: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct ClaudeInit<'a> {
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow)]
    pub model: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct ClaudeMessage<'a> {
    #[serde(borrow)]
    pub message: &'a RawValue,
    #[serde(borrow, default)]
    pub parent_tool_use_id: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub session_id: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub error: Option<Cow<'a, str>>,
}
#[derive(Deserialize)]
pub struct ClaudeStream<'a> {
    #[serde(borrow)]
    pub uuid: Cow<'a, str>,
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow)]
    pub event: &'a RawValue,
    #[serde(borrow, default)]
    pub parent_tool_use_id: Option<Cow<'a, str>>,
}
#[derive(Deserialize)]
pub struct ClaudeOrigin<'a> {
    #[serde(borrow)]
    pub kind: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct ClaudeResult<'a> {
    #[serde(borrow)]
    pub subtype: Cow<'a, str>,
    pub duration_ms: u64,
    pub duration_api_ms: u64,
    pub is_error: bool,
    pub num_turns: u64,
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow, default)]
    pub result: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub usage: Option<&'a RawValue>,
    #[serde(borrow, rename = "modelUsage", default)]
    pub model_usage: Option<&'a RawValue>,
    #[serde(borrow, default)]
    pub total_cost_usd: Option<&'a RawValue>,
    #[serde(borrow, default)]
    pub terminal_reason: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub origin: Option<ClaudeOrigin<'a>>,
    #[serde(borrow, default)]
    pub deferred_tool_use: Option<&'a RawValue>,
}
impl ClaudeResult<'_> {
    pub fn primary(&self) -> bool {
        self.origin
            .as_ref()
            .is_none_or(|origin| origin.kind == "human")
    }
    pub fn successful(&self) -> bool {
        !self.is_error
            && self.subtype == "success"
            && self.deferred_tool_use.is_none()
            && !matches!(
                self.terminal_reason.as_deref(),
                Some("aborted_streaming" | "aborted_tools")
            )
    }
}
pub enum ClaudeEventData<'a> {
    Init(ClaudeInit<'a>),
    System,
    Assistant(ClaudeMessage<'a>),
    User(ClaudeMessage<'a>),
    Stream(ClaudeStream<'a>),
    Result(ClaudeResult<'a>),
    RateLimit,
}
pub struct ClaudeEvent<'a> {
    pub native: &'a RawValue,
    pub data: ClaudeEventData<'a>,
}
#[expect(
    clippy::large_enum_variant,
    reason = "Keep the audited native frame borrowed inline without allocating each event."
)]
pub enum ClaudeDecodedFrame<'a> {
    Ignored,
    Known(ClaudeEvent<'a>),
}
pub type ClaudeEventView<'a> = &'a ClaudeEvent<'a>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaudeWireReason {
    Policy,
    Audit,
    Schema,
    Context,
}
enum Cause {
    Audit(CodexWireError),
    Json(serde_json::Error),
    Io(io::Error),
}
pub struct ClaudeWireError {
    reason: ClaudeWireReason,
    cause: Option<Cause>,
}
impl ClaudeWireError {
    pub fn reason(&self) -> ClaudeWireReason {
        self.reason
    }
    fn policy() -> Self {
        Self {
            reason: ClaudeWireReason::Policy,
            cause: None,
        }
    }
    fn schema() -> Self {
        Self {
            reason: ClaudeWireReason::Schema,
            cause: None,
        }
    }
    fn json(e: serde_json::Error) -> Self {
        Self {
            reason: ClaudeWireReason::Schema,
            cause: Some(Cause::Json(e)),
        }
    }
    fn audit(e: CodexWireError) -> Self {
        Self {
            reason: ClaudeWireReason::Audit,
            cause: Some(Cause::Audit(e)),
        }
    }
    fn context(e: io::Error) -> Self {
        Self {
            reason: ClaudeWireReason::Context,
            cause: Some(Cause::Io(e)),
        }
    }
}
impl fmt::Debug for ClaudeWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("ClaudeWireError");
        d.field("reason", &self.reason);
        match &self.cause {
            Some(Cause::Audit(e)) => {
                d.field("audit", e);
            }
            Some(Cause::Json(e)) => {
                d.field("category", &e.classify())
                    .field("line", &e.line())
                    .field("column", &e.column());
            }
            Some(Cause::Io(e)) => {
                d.field("io_kind", &e.kind());
            }
            None => {}
        }
        d.finish()
    }
}
impl fmt::Display for ClaudeWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for ClaudeWireError {}
fn parse<'a, T: Deserialize<'a>>(text: &'a str) -> Result<T, ClaudeWireError> {
    serde_json::from_str(text).map_err(ClaudeWireError::json)
}
fn identifier(text: &str, limits: &ClaudeWireLimits) -> Result<(), ClaudeWireError> {
    if text.is_empty() || text.len() > limits.id_bytes {
        Err(ClaudeWireError::policy())
    } else {
        Ok(())
    }
}
fn object(value: &RawValue) -> Result<(), ClaudeWireError> {
    if value.get().starts_with('{') {
        Ok(())
    } else {
        Err(ClaudeWireError::schema())
    }
}
#[derive(Deserialize)]
struct Content<'a> {
    #[serde(borrow)]
    content: &'a RawValue,
    #[serde(borrow, default)]
    model: Option<Cow<'a, str>>,
}
#[derive(Deserialize)]
struct Block<'a> {
    #[serde(rename = "type", borrow)]
    kind: Cow<'a, str>,
    #[serde(borrow, default)]
    text: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    thinking: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    signature: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    id: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    name: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    input: Option<&'a RawValue>,
    #[serde(borrow, default)]
    tool_use_id: Option<Cow<'a, str>>,
}
struct Blocks<'a> {
    original: &'a SnapshotContext,
    limits: &'a ClaudeWireLimits,
    cause: &'a mut Option<ClaudeWireError>,
}
impl<'de> Visitor<'de> for Blocks<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("native content blocks")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(raw) = seq.next_element::<&RawValue>()? {
            let result = (|| {
                self.original.check().map_err(ClaudeWireError::context)?;
                let block: Block<'_> = parse(raw.get())?;
                match block.kind.as_ref() {
                    "text" if block.text.is_none() => return Err(ClaudeWireError::schema()),
                    "thinking" if block.thinking.is_none() || block.signature.is_none() => {
                        return Err(ClaudeWireError::schema());
                    }
                    "tool_use" | "server_tool_use" => {
                        identifier(
                            block.id.as_deref().ok_or_else(ClaudeWireError::schema)?,
                            self.limits,
                        )?;
                        identifier(
                            block.name.as_deref().ok_or_else(ClaudeWireError::schema)?,
                            self.limits,
                        )?;
                        object(block.input.ok_or_else(ClaudeWireError::schema)?)?;
                    }
                    "tool_result" | "advisor_tool_result" => identifier(
                        block
                            .tool_use_id
                            .as_deref()
                            .ok_or_else(ClaudeWireError::schema)?,
                        self.limits,
                    )?,
                    _ => {}
                }
                Ok(())
            })();
            if let Err(error) = result {
                *self.cause = Some(error);
                return Err(de::Error::custom("native block"));
            }
        }
        Ok(())
    }
}
fn message(
    e: &ClaudeMessage<'_>,
    assistant: bool,
    limits: &ClaudeWireLimits,
    original: &SnapshotContext,
) -> Result<(), ClaudeWireError> {
    if let Some(id) = &e.parent_tool_use_id {
        identifier(id, limits)?;
    }
    if let Some(id) = &e.session_id {
        identifier(id, limits)?;
    }
    let content: Content<'_> = parse(e.message.get())?;
    if assistant {
        identifier(
            content
                .model
                .as_deref()
                .ok_or_else(ClaudeWireError::schema)?,
            limits,
        )?;
    }
    if !assistant && content.content.get().starts_with('"') {
        let _: Cow<'_, str> = parse(content.content.get())?;
        return Ok(());
    }
    let mut cause = None;
    let mut d = serde_json::Deserializer::from_str(content.content.get());
    let result = d.deserialize_seq(Blocks {
        original,
        limits,
        cause: &mut cause,
    });
    if let Some(error) = cause {
        return Err(error);
    }
    result.map_err(ClaudeWireError::json)?;
    d.end().map_err(ClaudeWireError::json)
}
#[derive(Deserialize)]
struct Partial<'a> {
    #[serde(rename = "type", borrow)]
    kind: Cow<'a, str>,
    #[serde(borrow, default)]
    delta: Option<&'a RawValue>,
    #[serde(borrow, default)]
    content_block: Option<&'a RawValue>,
    #[serde(borrow, default)]
    message: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Delta<'a> {
    #[serde(rename = "type", borrow)]
    kind: Cow<'a, str>,
    #[serde(borrow, default)]
    text: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    thinking: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    partial_json: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    signature: Option<Cow<'a, str>>,
}
pub(crate) fn validate_claude_limits(limits: &ClaudeWireLimits) -> Result<(), ClaudeWireError> {
    limits.validate().map_err(ClaudeWireError::audit)
}
pub fn decode_claude_frame<'a>(
    frame: &'a [u8],
    limits: &ClaudeWireLimits,
    original: &SnapshotContext,
) -> Result<ClaudeDecodedFrame<'a>, ClaudeWireError> {
    original.check().map_err(ClaudeWireError::context)?;
    limits.validate().map_err(ClaudeWireError::audit)?;
    audit_json_frame(frame, limits, original).map_err(ClaudeWireError::audit)?;
    let native: &RawValue = serde_json::from_slice(frame).map_err(ClaudeWireError::json)?;
    let d: Discriminator<'_> = parse(native.get())?;
    let data = match d.kind.as_ref() {
        "system" => {
            #[derive(Deserialize)]
            struct System<'a> {
                #[serde(borrow)]
                subtype: Cow<'a, str>,
            }
            let system: System<'_> = parse(native.get())?;
            if system.subtype == "init" {
                let e: ClaudeInit<'_> = parse(native.get())?;
                identifier(&e.session_id, limits)?;
                identifier(&e.model, limits)?;
                ClaudeEventData::Init(e)
            } else {
                ClaudeEventData::System
            }
        }
        "assistant" | "user" => {
            let e: ClaudeMessage<'_> = parse(native.get())?;
            message(&e, d.kind == "assistant", limits, original)?;
            if d.kind == "assistant" {
                ClaudeEventData::Assistant(e)
            } else {
                ClaudeEventData::User(e)
            }
        }
        "stream_event" => {
            let e: ClaudeStream<'_> = parse(native.get())?;
            identifier(&e.uuid, limits)?;
            identifier(&e.session_id, limits)?;
            if let Some(id) = &e.parent_tool_use_id {
                identifier(id, limits)?;
            }
            let partial: Partial<'_> = parse(e.event.get())?;
            match partial.kind.as_ref() {
                "message_start" => object(partial.message.ok_or_else(ClaudeWireError::schema)?)?,
                "content_block_start" => {
                    object(partial.content_block.ok_or_else(ClaudeWireError::schema)?)?
                }
                "content_block_delta" => {
                    let delta: Delta<'_> =
                        parse(partial.delta.ok_or_else(ClaudeWireError::schema)?.get())?;
                    let missing = match delta.kind.as_ref() {
                        "text_delta" => delta.text.is_none(),
                        "thinking_delta" => delta.thinking.is_none(),
                        "input_json_delta" => delta.partial_json.is_none(),
                        "signature_delta" => delta.signature.is_none(),
                        _ => false,
                    };
                    if missing {
                        return Err(ClaudeWireError::schema());
                    }
                }
                _ => {}
            }
            ClaudeEventData::Stream(e)
        }
        "result" => {
            let e: ClaudeResult<'_> = parse(native.get())?;
            identifier(&e.session_id, limits)?;
            if let Some(usage) = e.usage {
                object(usage)?;
            }
            if let Some(usage) = e.model_usage {
                object(usage)?;
            }
            if let Some(cost) = e.total_cost_usd {
                let cost: f64 = parse(cost.get())?;
                if !cost.is_finite() || cost < 0.0 {
                    return Err(ClaudeWireError::schema());
                }
            }
            ClaudeEventData::Result(e)
        }
        "rate_limit_event" => {
            #[derive(Deserialize)]
            struct Rate<'a> {
                #[serde(borrow)]
                rate_limit_info: &'a RawValue,
            }
            let rate: Rate<'_> = parse(native.get())?;
            object(rate.rate_limit_info)?;
            ClaudeEventData::RateLimit
        }
        _ => {
            original.check().map_err(ClaudeWireError::context)?;
            return Ok(ClaudeDecodedFrame::Ignored);
        }
    };
    original.check().map_err(ClaudeWireError::context)?;
    Ok(ClaudeDecodedFrame::Known(ClaudeEvent { native, data }))
}
