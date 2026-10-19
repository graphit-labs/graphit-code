//! Qwen Code headless text-input JSONL. Native payloads are borrowed and never diagnostics.
pub use crate::cli_codex_wire::CodexWireLimits as QwenWireLimits;
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
pub struct QwenInit<'a> {
    #[serde(borrow)]
    pub uuid: Cow<'a, str>,
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow)]
    pub model: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct QwenSystem<'a> {
    #[serde(borrow)]
    pub subtype: Cow<'a, str>,
    #[serde(borrow)]
    pub uuid: Cow<'a, str>,
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct QwenMessage<'a> {
    #[serde(borrow, default)]
    pub uuid: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow)]
    pub message: &'a RawValue,
    #[serde(borrow)]
    pub parent_tool_use_id: &'a RawValue,
}
#[derive(Deserialize)]
pub struct QwenStream<'a> {
    #[serde(borrow)]
    pub uuid: Cow<'a, str>,
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow)]
    pub event: &'a RawValue,
    #[serde(borrow)]
    pub parent_tool_use_id: &'a RawValue,
}
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct QwenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub server_tool_use: Option<QwenServerToolUse>,
    pub cache_creation: Option<QwenCacheCreation>,
}
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct QwenServerToolUse {
    pub web_search_requests: u64,
}
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct QwenCacheCreation {
    pub ephemeral_1h_input_tokens: u64,
    pub ephemeral_5m_input_tokens: u64,
}
#[derive(Deserialize)]
pub struct QwenResult<'a> {
    #[serde(borrow)]
    pub subtype: Cow<'a, str>,
    #[serde(borrow)]
    pub uuid: Cow<'a, str>,
    pub duration_ms: u64,
    pub duration_api_ms: u64,
    pub is_error: bool,
    pub num_turns: u64,
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow, default)]
    pub result: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub usage: &'a RawValue,
    #[serde(borrow, rename = "modelUsage", default)]
    pub model_usage: Option<&'a RawValue>,
    #[serde(borrow)]
    pub permission_denials: &'a RawValue,
    #[serde(borrow, default)]
    pub error: Option<&'a RawValue>,
}
impl QwenResult<'_> {
    pub fn successful(&self) -> bool {
        !self.is_error && self.subtype == "success"
    }
}
pub enum QwenEventData<'a> {
    Init(QwenInit<'a>),
    System(QwenSystem<'a>),
    Assistant(QwenMessage<'a>),
    User(QwenMessage<'a>),
    Stream(QwenStream<'a>),
    Result(QwenResult<'a>),
    Control,
}
pub struct QwenEvent<'a> {
    pub native: &'a RawValue,
    pub data: QwenEventData<'a>,
}
#[expect(
    clippy::large_enum_variant,
    reason = "Keep audited native frames borrowed inline without allocating each event."
)]
pub enum QwenDecodedFrame<'a> {
    Ignored,
    Known(QwenEvent<'a>),
}
pub type QwenEventView<'a> = &'a QwenEvent<'a>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QwenWireReason {
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
pub struct QwenWireError {
    reason: QwenWireReason,
    cause: Option<Cause>,
}
impl QwenWireError {
    pub fn reason(&self) -> QwenWireReason {
        self.reason
    }
    fn policy() -> Self {
        Self {
            reason: QwenWireReason::Policy,
            cause: None,
        }
    }
    fn schema() -> Self {
        Self {
            reason: QwenWireReason::Schema,
            cause: None,
        }
    }
    fn json(e: serde_json::Error) -> Self {
        Self {
            reason: QwenWireReason::Schema,
            cause: Some(Cause::Json(e)),
        }
    }
    fn audit(e: CodexWireError) -> Self {
        Self {
            reason: QwenWireReason::Audit,
            cause: Some(Cause::Audit(e)),
        }
    }
    fn context(e: io::Error) -> Self {
        Self {
            reason: QwenWireReason::Context,
            cause: Some(Cause::Io(e)),
        }
    }
}
impl fmt::Debug for QwenWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("QwenWireError");
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
impl fmt::Display for QwenWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for QwenWireError {}
fn parse<'a, T: Deserialize<'a>>(text: &'a str) -> Result<T, QwenWireError> {
    serde_json::from_str(text).map_err(QwenWireError::json)
}
fn identifier(text: &str, limits: &QwenWireLimits) -> Result<(), QwenWireError> {
    if text.is_empty() || text.len() > limits.id_bytes {
        Err(QwenWireError::policy())
    } else {
        Ok(())
    }
}
fn object(value: &RawValue) -> Result<(), QwenWireError> {
    if value.get().starts_with('{') {
        Ok(())
    } else {
        Err(QwenWireError::schema())
    }
}
#[derive(Deserialize)]
struct Content<'a> {
    #[serde(borrow)]
    content: &'a RawValue,
    #[serde(borrow)]
    role: Cow<'a, str>,
    #[serde(borrow, default)]
    id: Option<Cow<'a, str>>,
    #[serde(rename = "type", borrow, default)]
    kind: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    model: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    usage: Option<&'a RawValue>,
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
    tool_use_id: Option<Cow<'a, str>>,
}
fn parent(raw: &RawValue, limits: &QwenWireLimits) -> Result<(), QwenWireError> {
    if raw.get() == "null" {
        return Ok(());
    }
    let id: Cow<'_, str> = parse(raw.get())?;
    identifier(&id, limits)
}
fn block(raw: &RawValue, limits: &QwenWireLimits) -> Result<(), QwenWireError> {
    let value: Block<'_> = parse(raw.get())?;
    match value.kind.as_ref() {
        "text" if value.text.is_none() => return Err(QwenWireError::schema()),
        "thinking" if value.thinking.is_none() => return Err(QwenWireError::schema()),
        "tool_use" => {
            #[derive(Deserialize)]
            struct Tool<'a> {
                #[serde(borrow)]
                id: Cow<'a, str>,
                #[serde(borrow)]
                name: Cow<'a, str>,
                #[serde(borrow)]
                input: &'a RawValue,
            }
            let tool: Tool<'_> = parse(raw.get())?;
            identifier(&tool.id, limits)?;
            identifier(&tool.name, limits)?;
            // Native input is unknown JSON, including null; never an authority grant.
            let _ = tool.input;
        }
        "tool_result" => identifier(
            value
                .tool_use_id
                .as_deref()
                .ok_or_else(QwenWireError::schema)?,
            limits,
        )?,
        _ => {}
    }
    let _ = value.signature;
    Ok(())
}
struct Blocks<'a> {
    original: &'a SnapshotContext,
    limits: &'a QwenWireLimits,
    cause: &'a mut Option<QwenWireError>,
}
impl<'de> Visitor<'de> for Blocks<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("native content blocks")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(raw) = seq.next_element::<&RawValue>()? {
            let result = self
                .original
                .check()
                .map_err(QwenWireError::context)
                .and_then(|()| block(raw, self.limits));
            if let Err(error) = result {
                *self.cause = Some(error);
                return Err(de::Error::custom("native block"));
            }
        }
        Ok(())
    }
}
fn message(
    e: &QwenMessage<'_>,
    assistant: bool,
    limits: &QwenWireLimits,
    original: &SnapshotContext,
) -> Result<(), QwenWireError> {
    parent(e.parent_tool_use_id, limits)?;
    identifier(&e.session_id, limits)?;
    if let Some(uuid) = &e.uuid {
        identifier(uuid, limits)?;
    }
    let content: Content<'_> = parse(e.message.get())?;
    if assistant {
        identifier(e.uuid.as_deref().ok_or_else(QwenWireError::schema)?, limits)?;
        identifier(
            content.id.as_deref().ok_or_else(QwenWireError::schema)?,
            limits,
        )?;
        identifier(
            content.model.as_deref().ok_or_else(QwenWireError::schema)?,
            limits,
        )?;
        if content.kind.as_deref() != Some("message") || content.role != "assistant" {
            return Err(QwenWireError::schema());
        }
        let _: QwenUsage = parse(content.usage.ok_or_else(QwenWireError::schema)?.get())?;
    } else if content.role != "user" {
        return Err(QwenWireError::schema());
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
    result.map_err(QwenWireError::json)?;
    d.end().map_err(QwenWireError::json)
}
struct Denials<'a> {
    original: &'a SnapshotContext,
    limits: &'a QwenWireLimits,
    cause: &'a mut Option<QwenWireError>,
}
impl<'de> Visitor<'de> for Denials<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("native permission denials")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        #[derive(Deserialize)]
        struct Denial<'a> {
            #[serde(borrow)]
            tool_name: Cow<'a, str>,
            #[serde(borrow)]
            tool_use_id: Cow<'a, str>,
            #[serde(borrow)]
            tool_input: &'a RawValue,
        }
        while let Some(raw) = seq.next_element::<&RawValue>()? {
            let result = (|| {
                self.original.check().map_err(QwenWireError::context)?;
                let denial: Denial<'_> = parse(raw.get())?;
                identifier(&denial.tool_name, self.limits)?;
                identifier(&denial.tool_use_id, self.limits)?;
                let _ = denial.tool_input;
                Ok(())
            })();
            if let Err(error) = result {
                *self.cause = Some(error);
                return Err(de::Error::custom("native denial"));
            }
        }
        Ok(())
    }
}
fn denials(
    raw: &RawValue,
    limits: &QwenWireLimits,
    original: &SnapshotContext,
) -> Result<(), QwenWireError> {
    let mut cause = None;
    let mut d = serde_json::Deserializer::from_str(raw.get());
    let parsed = d.deserialize_seq(Denials {
        original,
        limits,
        cause: &mut cause,
    });
    if let Some(error) = cause {
        return Err(error);
    }
    parsed.map_err(QwenWireError::json)?;
    d.end().map_err(QwenWireError::json)
}
#[derive(Deserialize)]
struct Partial<'a> {
    #[serde(rename = "type", borrow)]
    kind: Cow<'a, str>,
    index: Option<u64>,
    #[serde(borrow, default)]
    tool_use_id: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    content: Option<&'a RawValue>,
    #[serde(borrow, default)]
    goal_state: Option<&'a RawValue>,
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
}
pub(crate) fn validate_qwen_limits(limits: &QwenWireLimits) -> Result<(), QwenWireError> {
    limits.validate().map_err(QwenWireError::audit)
}
pub fn decode_qwen_frame<'a>(
    frame: &'a [u8],
    limits: &QwenWireLimits,
    original: &SnapshotContext,
) -> Result<QwenDecodedFrame<'a>, QwenWireError> {
    original.check().map_err(QwenWireError::context)?;
    limits.validate().map_err(QwenWireError::audit)?;
    audit_json_frame(frame, limits, original).map_err(QwenWireError::audit)?;
    let native: &RawValue = serde_json::from_slice(frame).map_err(QwenWireError::json)?;
    let d: Discriminator<'_> = parse(native.get())?;
    let data = match d.kind.as_ref() {
        "system" => {
            let system: QwenSystem<'_> = parse(native.get())?;
            identifier(&system.uuid, limits)?;
            identifier(&system.session_id, limits)?;
            identifier(&system.subtype, limits)?;
            if system.subtype == "init" {
                let e: QwenInit<'_> = parse(native.get())?;
                identifier(&e.model, limits)?;
                QwenEventData::Init(e)
            } else {
                QwenEventData::System(system)
            }
        }
        "assistant" | "user" => {
            let e: QwenMessage<'_> = parse(native.get())?;
            message(&e, d.kind == "assistant", limits, original)?;
            if d.kind == "assistant" {
                QwenEventData::Assistant(e)
            } else {
                QwenEventData::User(e)
            }
        }
        "stream_event" => {
            let e: QwenStream<'_> = parse(native.get())?;
            identifier(&e.uuid, limits)?;
            identifier(&e.session_id, limits)?;
            parent(e.parent_tool_use_id, limits)?;
            let partial: Partial<'_> = parse(e.event.get())?;
            match partial.kind.as_ref() {
                "message_start" => {
                    #[derive(Deserialize)]
                    struct Start<'a> {
                        #[serde(borrow)]
                        id: Cow<'a, str>,
                        #[serde(borrow)]
                        role: Cow<'a, str>,
                        #[serde(borrow)]
                        model: Cow<'a, str>,
                        content: [(); 0],
                    }
                    let start: Start<'_> =
                        parse(partial.message.ok_or_else(QwenWireError::schema)?.get())?;
                    identifier(&start.id, limits)?;
                    identifier(&start.model, limits)?;
                    if start.role != "assistant" {
                        return Err(QwenWireError::schema());
                    }
                    let _ = start.content;
                }
                "content_block_start" => {
                    partial.index.ok_or_else(QwenWireError::schema)?;
                    block(
                        partial.content_block.ok_or_else(QwenWireError::schema)?,
                        limits,
                    )?;
                }
                "content_block_stop" => {
                    partial.index.ok_or_else(QwenWireError::schema)?;
                }
                "tool_progress" => {
                    identifier(
                        partial
                            .tool_use_id
                            .as_deref()
                            .ok_or_else(QwenWireError::schema)?,
                        limits,
                    )?;
                    object(partial.content.ok_or_else(QwenWireError::schema)?)?;
                }
                "goal_state" => {
                    object(partial.goal_state.ok_or_else(QwenWireError::schema)?)?;
                }
                "content_block_delta" => {
                    partial.index.ok_or_else(QwenWireError::schema)?;
                    let delta: Delta<'_> =
                        parse(partial.delta.ok_or_else(QwenWireError::schema)?.get())?;
                    let missing = match delta.kind.as_ref() {
                        "text_delta" => delta.text.is_none(),
                        "thinking_delta" => delta.thinking.is_none(),
                        "input_json_delta" => delta.partial_json.is_none(),
                        _ => false,
                    };
                    if missing {
                        return Err(QwenWireError::schema());
                    }
                }
                _ => {}
            }
            QwenEventData::Stream(e)
        }
        "result" => {
            let e: QwenResult<'_> = parse(native.get())?;
            identifier(&e.uuid, limits)?;
            identifier(&e.session_id, limits)?;
            identifier(&e.subtype, limits)?;
            if !e.is_error && (!e.successful() || e.result.is_none()) {
                return Err(QwenWireError::schema());
            }
            let _: QwenUsage = parse(e.usage.get())?;
            if let Some(usage) = e.model_usage {
                object(usage)?;
            }
            denials(e.permission_denials, limits, original)?;
            if let Some(error) = e.error {
                #[derive(Deserialize)]
                struct ErrorMessage<'a> {
                    #[serde(borrow)]
                    message: Cow<'a, str>,
                }
                let native_error: ErrorMessage<'_> = parse(error.get())?;
                let _ = native_error.message;
            }
            QwenEventData::Result(e)
        }
        "control_request" => {
            #[derive(Deserialize)]
            struct Request<'a> {
                #[serde(borrow)]
                request_id: Cow<'a, str>,
                #[serde(borrow)]
                request: &'a RawValue,
            }
            let request: Request<'_> = parse(native.get())?;
            identifier(&request.request_id, limits)?;
            #[derive(Deserialize)]
            struct RequestPayload<'a> {
                #[serde(borrow)]
                subtype: Cow<'a, str>,
            }
            let payload: RequestPayload<'_> = parse(request.request.get())?;
            identifier(&payload.subtype, limits)?;
            QwenEventData::Control
        }
        "control_response" => {
            #[derive(Deserialize)]
            struct Response<'a> {
                #[serde(borrow)]
                response: &'a RawValue,
            }
            #[derive(Deserialize)]
            struct Payload<'a> {
                #[serde(borrow)]
                request_id: Cow<'a, str>,
                #[serde(borrow)]
                subtype: Cow<'a, str>,
            }
            let response: Response<'_> = parse(native.get())?;
            let payload: Payload<'_> = parse(response.response.get())?;
            identifier(&payload.request_id, limits)?;
            identifier(&payload.subtype, limits)?;
            match payload.subtype.as_ref() {
                "success" => {
                    #[derive(Deserialize)]
                    struct Success<'a> {
                        #[serde(borrow)]
                        response: &'a RawValue,
                    }
                    let value: Success<'_> = parse(response.response.get())?;
                    let _ = value.response;
                }
                "error" => {
                    #[derive(Deserialize)]
                    struct Error<'a> {
                        #[serde(borrow)]
                        error: &'a RawValue,
                    }
                    let value: Error<'_> = parse(response.response.get())?;
                    if !value.error.get().starts_with('"') {
                        object(value.error)?;
                    }
                }
                _ => {}
            }
            QwenEventData::Control
        }
        _ => {
            original.check().map_err(QwenWireError::context)?;
            return Ok(QwenDecodedFrame::Ignored);
        }
    };
    original.check().map_err(QwenWireError::context)?;
    Ok(QwenDecodedFrame::Known(QwenEvent { native, data }))
}
