//! Antigravity's native headless NDJSON; opaque values remain borrowed.
pub use crate::cli_codex_wire::CodexWireLimits as AntigravityWireLimits;
use crate::cli_codex_wire::{CodexWireError, audit_json_frame};
use graphit_core::file_io::SnapshotContext;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{borrow::Cow, fmt, io};

#[derive(Deserialize)]
struct Discriminator<'a> {
    #[serde(borrow)]
    event: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct AntigravityInit<'a> {
    #[serde(borrow)]
    pub conversation_id: Cow<'a, str>,
    #[serde(borrow)]
    pub init: AntigravityInitSettings<'a>,
}
#[derive(Deserialize)]
pub struct AntigravityInitSettings<'a> {
    #[serde(borrow, default)]
    pub model: Option<Cow<'a, str>>,
}
#[derive(Deserialize)]
pub struct AntigravityStep<'a> {
    #[serde(borrow)]
    pub conversation_id: Cow<'a, str>,
    pub step_index: u64,
    #[serde(borrow)]
    pub state: Cow<'a, str>,
    #[serde(borrow)]
    pub step_type: Cow<'a, str>,
    #[serde(borrow, default)]
    pub text_delta: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub tool_name: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub tool_info: Option<&'a RawValue>,
    #[serde(borrow, default)]
    pub subagent_info: Option<&'a RawValue>,
    #[serde(borrow, default)]
    pub usage: Option<&'a RawValue>,
    pub duration_seconds: Option<f64>,
}
#[derive(Deserialize)]
pub struct AntigravityResult<'a> {
    #[serde(borrow)]
    pub conversation_id: Cow<'a, str>,
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    #[serde(borrow, default)]
    pub response: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub error: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub usage: Option<&'a RawValue>,
    pub duration_seconds: Option<f64>,
    pub num_turns: Option<u64>,
}
impl AntigravityResult<'_> {
    pub fn successful(&self) -> bool {
        self.status == "SUCCESS" && self.error.is_none() && self.response.is_some()
    }
}
pub enum AntigravityEventData<'a> {
    Init(AntigravityInit<'a>),
    Step(AntigravityStep<'a>),
    Result(AntigravityResult<'a>),
}
pub struct AntigravityEvent<'a> {
    pub native: &'a RawValue,
    pub data: AntigravityEventData<'a>,
}
pub type AntigravityEventView<'a> = &'a AntigravityEvent<'a>;

pub enum AntigravityWireError {
    Context(io::Error),
    Audit(CodexWireError),
    Json(serde_json::Error),
    Schema,
}
impl fmt::Debug for AntigravityWireError {
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
impl fmt::Display for AntigravityWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for AntigravityWireError {}
fn parse<'a, T: Deserialize<'a>>(native: &'a RawValue) -> Result<T, AntigravityWireError> {
    serde_json::from_str(native.get()).map_err(AntigravityWireError::Json)
}
fn identifier(text: &str, limits: &AntigravityWireLimits) -> Result<(), AntigravityWireError> {
    if text.is_empty() || text.len() > limits.id_bytes {
        Err(AntigravityWireError::Schema)
    } else {
        Ok(())
    }
}
fn object(value: &RawValue) -> Result<(), AntigravityWireError> {
    if value.get().starts_with('{') {
        Ok(())
    } else {
        Err(AntigravityWireError::Schema)
    }
}
fn duration(value: Option<f64>) -> Result<(), AntigravityWireError> {
    if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
        Err(AntigravityWireError::Schema)
    } else {
        Ok(())
    }
}
pub(crate) fn validate_antigravity_limits(
    limits: &AntigravityWireLimits,
) -> Result<(), AntigravityWireError> {
    limits.validate().map_err(AntigravityWireError::Audit)
}
pub fn decode_antigravity_frame<'a>(
    frame: &'a [u8],
    limits: &AntigravityWireLimits,
    original: &SnapshotContext,
) -> Result<Option<AntigravityEvent<'a>>, AntigravityWireError> {
    original.check().map_err(AntigravityWireError::Context)?;
    validate_antigravity_limits(limits)?;
    audit_json_frame(frame, limits, original).map_err(AntigravityWireError::Audit)?;
    let native: &RawValue = serde_json::from_slice(frame).map_err(AntigravityWireError::Json)?;
    let discriminator: Discriminator<'_> = parse(native)?;
    let data = match discriminator.event.as_ref() {
        "init" => {
            let init: AntigravityInit<'_> = parse(native)?;
            identifier(&init.conversation_id, limits)?;
            if let Some(model) = &init.init.model {
                identifier(model, limits)?;
            }
            AntigravityEventData::Init(init)
        }
        "step_update" => {
            #[derive(Deserialize)]
            struct Envelope<'a> {
                #[serde(borrow)]
                step_update: AntigravityStep<'a>,
            }
            let step = parse::<Envelope<'_>>(native)?.step_update;
            identifier(&step.conversation_id, limits)?;
            identifier(&step.state, limits)?;
            identifier(&step.step_type, limits)?;
            if let Some(name) = &step.tool_name {
                identifier(name, limits)?;
            }
            for value in [step.tool_info, step.subagent_info, step.usage]
                .into_iter()
                .flatten()
            {
                object(value)?;
            }
            duration(step.duration_seconds)?;
            AntigravityEventData::Step(step)
        }
        "result" => {
            #[derive(Deserialize)]
            struct Envelope<'a> {
                #[serde(borrow)]
                result: AntigravityResult<'a>,
            }
            let result = parse::<Envelope<'_>>(native)?.result;
            // Native startup errors may precede init and have an empty conversation id.
            if result.conversation_id.len() > limits.id_bytes {
                return Err(AntigravityWireError::Schema);
            }
            identifier(&result.status, limits)?;
            if result.status == "SUCCESS" {
                identifier(&result.conversation_id, limits)?;
                if result.response.is_none() || result.error.is_some() {
                    return Err(AntigravityWireError::Schema);
                }
            }
            if let Some(usage) = result.usage {
                object(usage)?;
            }
            duration(result.duration_seconds)?;
            AntigravityEventData::Result(result)
        }
        _ => return Ok(None),
    };
    original.check().map_err(AntigravityWireError::Context)?;
    Ok(Some(AntigravityEvent { native, data }))
}

/// Encode a single native user turn; close the Process stdin after these bytes.
pub fn encode_antigravity_prompt(prompt: &str) -> Result<Vec<u8>, serde_json::Error> {
    #[derive(serde::Serialize)]
    struct Message<'a> {
        content: &'a str,
    }
    #[derive(serde::Serialize)]
    struct Input<'a> {
        event: &'static str,
        message: Message<'a>,
    }
    let mut bytes = serde_json::to_vec(&Input {
        event: "user",
        message: Message { content: prompt },
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}
