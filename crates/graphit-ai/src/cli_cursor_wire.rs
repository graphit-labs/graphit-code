//! Cursor print-mode NDJSON; native payloads remain borrowed and never diagnostics.
pub use crate::cli_codex_wire::CodexWireLimits as CursorWireLimits;
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
pub struct CursorInit<'a> {
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow)]
    pub model: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct CursorMessage<'a> {
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow)]
    pub message: &'a RawValue,
}
#[derive(Deserialize)]
pub struct CursorToolCall<'a> {
    #[serde(borrow)]
    pub subtype: Cow<'a, str>,
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow)]
    pub call_id: Cow<'a, str>,
    #[serde(borrow)]
    pub tool_call: &'a RawValue,
}
#[derive(Deserialize)]
pub struct CursorResult<'a> {
    #[serde(borrow)]
    pub subtype: Cow<'a, str>,
    pub is_error: bool,
    pub duration_ms: u64,
    pub duration_api_ms: u64,
    #[serde(borrow)]
    pub result: Cow<'a, str>,
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    #[serde(borrow, default)]
    pub request_id: Option<Cow<'a, str>>,
}
impl CursorResult<'_> {
    pub fn successful(&self) -> bool {
        self.subtype == "success" && !self.is_error
    }
}
pub enum CursorEventData<'a> {
    Init(CursorInit<'a>),
    User(CursorMessage<'a>),
    Assistant(CursorMessage<'a>),
    ToolCall(CursorToolCall<'a>),
    Result(CursorResult<'a>),
}
pub struct CursorEvent<'a> {
    pub native: &'a RawValue,
    pub data: CursorEventData<'a>,
}
pub enum CursorDecodedFrame<'a> {
    Ignored,
    Known(CursorEvent<'a>),
}
pub type CursorEventView<'a> = &'a CursorEvent<'a>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorWireReason {
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
pub struct CursorWireError {
    reason: CursorWireReason,
    cause: Option<Cause>,
}
impl CursorWireError {
    pub fn reason(&self) -> CursorWireReason {
        self.reason
    }
    fn policy() -> Self {
        Self {
            reason: CursorWireReason::Policy,
            cause: None,
        }
    }
    fn schema() -> Self {
        Self {
            reason: CursorWireReason::Schema,
            cause: None,
        }
    }
    fn json(e: serde_json::Error) -> Self {
        Self {
            reason: CursorWireReason::Schema,
            cause: Some(Cause::Json(e)),
        }
    }
    fn audit(e: CodexWireError) -> Self {
        Self {
            reason: CursorWireReason::Audit,
            cause: Some(Cause::Audit(e)),
        }
    }
    fn context(e: io::Error) -> Self {
        Self {
            reason: CursorWireReason::Context,
            cause: Some(Cause::Io(e)),
        }
    }
}
impl fmt::Debug for CursorWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("CursorWireError");
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
impl fmt::Display for CursorWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for CursorWireError {}
fn parse<'a, T: Deserialize<'a>>(text: &'a str) -> Result<T, CursorWireError> {
    serde_json::from_str(text).map_err(CursorWireError::json)
}
fn identifier(text: &str, limits: &CursorWireLimits) -> Result<(), CursorWireError> {
    if text.is_empty() || text.len() > limits.id_bytes {
        Err(CursorWireError::policy())
    } else {
        Ok(())
    }
}
fn object(value: &RawValue) -> Result<(), CursorWireError> {
    if value.get().starts_with('{') {
        Ok(())
    } else {
        Err(CursorWireError::schema())
    }
}

#[derive(Deserialize)]
struct Content<'a> {
    #[serde(borrow)]
    role: Cow<'a, str>,
    #[serde(borrow)]
    content: &'a RawValue,
}
#[derive(Deserialize)]
struct Block<'a> {
    #[serde(rename = "type", borrow)]
    kind: Cow<'a, str>,
    #[serde(borrow, default)]
    text: Option<Cow<'a, str>>,
}
struct Blocks<'a> {
    original: &'a SnapshotContext,
    cause: &'a mut Option<CursorWireError>,
}
impl<'de> Visitor<'de> for Blocks<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("native content array")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(raw) = seq.next_element::<&RawValue>()? {
            let check = (|| {
                self.original.check().map_err(CursorWireError::context)?;
                let block: Block<'_> = parse(raw.get())?;
                if block.kind == "text" && block.text.is_none() {
                    return Err(CursorWireError::schema());
                }
                Ok(())
            })();
            if let Err(cause) = check {
                *self.cause = Some(cause);
                return Err(de::Error::custom("native content"));
            }
        }
        Ok(())
    }
}
fn message(
    e: &CursorMessage<'_>,
    role: &str,
    limits: &CursorWireLimits,
    original: &SnapshotContext,
) -> Result<(), CursorWireError> {
    identifier(&e.session_id, limits)?;
    object(e.message)?;
    let content: Content<'_> = parse(e.message.get())?;
    if content.role != role {
        return Err(CursorWireError::schema());
    }
    let mut cause = None;
    let mut d = serde_json::Deserializer::from_str(content.content.get());
    let parsed = d.deserialize_seq(Blocks {
        original,
        cause: &mut cause,
    });
    if let Some(cause) = cause {
        return Err(cause);
    }
    parsed.map_err(CursorWireError::json)?;
    d.end().map_err(CursorWireError::json)
}
pub(crate) fn validate_cursor_limits(limits: &CursorWireLimits) -> Result<(), CursorWireError> {
    limits.validate().map_err(CursorWireError::audit)
}
pub fn decode_cursor_frame<'a>(
    frame: &'a [u8],
    limits: &CursorWireLimits,
    original: &SnapshotContext,
) -> Result<CursorDecodedFrame<'a>, CursorWireError> {
    original.check().map_err(CursorWireError::context)?;
    validate_cursor_limits(limits)?;
    audit_json_frame(frame, limits, original).map_err(CursorWireError::audit)?;
    let native: &RawValue = serde_json::from_slice(frame).map_err(CursorWireError::json)?;
    let d: Discriminator<'_> = parse(native.get())?;
    let data = match d.kind.as_ref() {
        "system" => {
            #[derive(Deserialize)]
            struct System<'a> {
                #[serde(borrow)]
                subtype: Cow<'a, str>,
            }
            let system: System<'_> = parse(native.get())?;
            if system.subtype != "init" {
                return Ok(CursorDecodedFrame::Ignored);
            }
            let e: CursorInit<'_> = parse(native.get())?;
            identifier(&e.session_id, limits)?;
            identifier(&e.model, limits)?;
            CursorEventData::Init(e)
        }
        "user" | "assistant" => {
            let e: CursorMessage<'_> = parse(native.get())?;
            message(&e, &d.kind, limits, original)?;
            if d.kind == "user" {
                CursorEventData::User(e)
            } else {
                CursorEventData::Assistant(e)
            }
        }
        "tool_call" => {
            let e: CursorToolCall<'_> = parse(native.get())?;
            identifier(&e.session_id, limits)?;
            identifier(&e.call_id, limits)?;
            object(e.tool_call)?;
            if !matches!(e.subtype.as_ref(), "started" | "completed") {
                return Ok(CursorDecodedFrame::Ignored);
            }
            CursorEventData::ToolCall(e)
        }
        "result" => {
            let e: CursorResult<'_> = parse(native.get())?;
            identifier(&e.session_id, limits)?;
            if let Some(id) = &e.request_id {
                identifier(id, limits)?;
            }
            CursorEventData::Result(e)
        }
        _ => return Ok(CursorDecodedFrame::Ignored),
    };
    original.check().map_err(CursorWireError::context)?;
    Ok(CursorDecodedFrame::Known(CursorEvent { native, data }))
}
