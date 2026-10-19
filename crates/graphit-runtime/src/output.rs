use graphit_core::file_io::SnapshotContext;
use graphit_hub::native::NativeBinding;
use serde::Serialize;
use std::{
    fmt,
    io::{self, Write},
};

/// Semantic events. Native spelling is selected by the explicit binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookEvent {
    SessionStart,
    UserPromptSubmit,
    SubagentStart,
    BeforeAgent,
    PostToolUse,
    AfterTool,
    PreInvocation,
    PostInvocation,
    Stop,
    SessionEnd,
    SystemTransform,
    SessionCompacting,
}
impl HookEvent {
    pub fn native_name(self, binding: NativeBinding) -> Result<&'static str, OutputError> {
        Ok(profile(binding, self)?.name)
    }
    /// The host callback or command transport that consumes this payload.
    pub fn channel(self, binding: NativeBinding) -> Result<DeliveryChannel, OutputError> {
        Ok(profile(binding, self)?.channel)
    }
    /// Output capability only; it grants no access to context or lifecycle effects.
    pub fn accepts_context(self, binding: NativeBinding) -> Result<bool, OutputError> {
        Ok(!matches!(
            profile(binding, self)?.wire,
            Wire::Observe | Wire::EmptyObject | Wire::StopDecision
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryChannel {
    CommandStdout,
    PluginSystem,
    PluginCompaction,
}

/// Already composed input, never evidence of authorization or successful delivery.
pub enum OutputMessage<'a> {
    Context(&'a str),
    Quiet,
}

/// Positive byte limits for each payload; no hidden defaults or batch accounting.
#[derive(Clone, Copy, Debug)]
pub struct OutputLimits {
    pub max_context_bytes: usize,
    pub max_output_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputError {
    InvalidLimits,
    UnsupportedBoundary,
    ContextUnavailable,
    EmptyContext,
    LimitExceeded,
    Cancelled,
    DeadlineExceeded,
    Allocation,
    Encoding,
}
impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "invalid runtime output limits",
            Self::UnsupportedBoundary => "unsupported native runtime boundary",
            Self::ContextUnavailable => "native boundary cannot deliver context",
            Self::EmptyContext => "runtime context is empty",
            Self::LimitExceeded => "runtime output limit exceeded",
            Self::Cancelled => "runtime output request cancelled",
            Self::DeadlineExceeded => "runtime output request deadline exceeded",
            Self::Allocation => "runtime output allocation unavailable",
            Self::Encoding => "runtime output encoding failed",
        })
    }
}
impl std::error::Error for OutputError {}

/// Private limits and the original borrowed request; encoding never replaces it.
///
/// ```compile_fail
/// use graphit_runtime::OutputContext;
/// fn clone(c: &OutputContext<'_>) { let _: OutputContext<'_> = c.clone(); }
/// ```
/// ```compile_fail
/// use graphit_runtime::OutputContext;
/// fn inspect(c: &OutputContext<'_>) { println!("{c:?}"); }
/// ```
/// ```compile_fail
/// use graphit_runtime::OutputContext;
/// let _: OutputContext<'_> = serde_json::from_str("{}").unwrap();
/// ```
pub struct OutputContext<'a> {
    request: &'a SnapshotContext,
    limits: OutputLimits,
}
impl<'a> OutputContext<'a> {
    pub fn new(limits: OutputLimits, request: &'a SnapshotContext) -> Result<Self, OutputError> {
        if limits.max_context_bytes == 0 || limits.max_output_bytes == 0 {
            return Err(OutputError::InvalidLimits);
        }
        check(request)?;
        Ok(Self { request, limits })
    }
}

/// Complete private payload. The channel must be consumed by the real host adapter.
/// This type deliberately does not implement Debug, Clone or serialization.
///
/// ```compile_fail
/// use graphit_runtime::EncodedOutput;
/// let _: EncodedOutput = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_runtime::EncodedOutput;
/// fn inspect(o: &EncodedOutput) { println!("{o:?}"); }
/// ```
/// ```compile_fail
/// use graphit_runtime::EncodedOutput;
/// fn duplicate(o: &EncodedOutput) { let _: EncodedOutput = o.clone(); }
/// ```
/// ```compile_fail
/// use graphit_runtime::EncodedOutput;
/// fn persist(o: &EncodedOutput) { let _ = serde_json::to_string(o); }
/// ```
/// ```compile_fail
/// use graphit_runtime::EncodedOutput;
/// fn extract(o: EncodedOutput) { let _ = o.bytes; }
/// ```
pub struct EncodedOutput {
    binding: NativeBinding,
    event: HookEvent,
    channel: DeliveryChannel,
    bytes: Vec<u8>,
}
impl EncodedOutput {
    pub fn binding(&self) -> NativeBinding {
        self.binding
    }
    pub fn event(&self) -> HookEvent {
        self.event
    }
    pub fn channel(&self) -> DeliveryChannel {
        self.channel
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Transfer the completed payload to the real adapter without copying its body.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[derive(Clone, Copy)]
enum Wire {
    EventJson,
    AdditionalContext,
    Text,
    InjectSteps,
    Observe,
    EmptyObject,
    StopDecision,
}
struct Profile {
    name: &'static str,
    wire: Wire,
    channel: DeliveryChannel,
}
fn profile(binding: NativeBinding, event: HookEvent) -> Result<Profile, OutputError> {
    use HookEvent::*;
    use NativeBinding::*;
    use Wire::*;
    let (name, wire) = match (binding, event) {
        (Codex | Claude | Qwen, SessionStart) => ("SessionStart", EventJson),
        (Codex | Claude | Qwen, UserPromptSubmit) => ("UserPromptSubmit", EventJson),
        (Codex | Claude | Qwen, SubagentStart) => ("SubagentStart", EventJson),
        (Codex | Claude | Qwen, PostToolUse) => ("PostToolUse", EventJson),
        (Codex | Claude | Qwen, Stop) => ("Stop", Observe),
        (Codex | Claude | Qwen, SessionEnd) => ("SessionEnd", Observe),
        (Cursor, SessionStart) => ("sessionStart", AdditionalContext),
        (Cursor, PostToolUse) => ("postToolUse", AdditionalContext),
        (Cursor, Stop) => ("stop", Observe),
        (Cursor, SessionEnd) => ("sessionEnd", Observe),
        (NativeBinding::KiroV3, SessionStart) => ("SessionStart", Text),
        (NativeBinding::KiroV3, UserPromptSubmit) => ("UserPromptSubmit", Text),
        (NativeBinding::KiroV3, PostToolUse) => ("PostToolUse", Text),
        (NativeBinding::KiroV3, Stop) => ("Stop", Observe),
        (NativeBinding::KiroV3, SessionEnd) => ("SessionEnd", Observe),
        (Kimi, UserPromptSubmit) => ("UserPromptSubmit", Text),
        (Kimi, SessionStart) => ("SessionStart", Observe),
        (Kimi, SubagentStart) => ("SubagentStart", Observe),
        (Kimi, PostToolUse) => ("PostToolUse", Observe),
        (Kimi, Stop) => ("Stop", Observe),
        (Kimi, SessionEnd) => ("SessionEnd", Observe),
        (Antigravity, PreInvocation) => ("PreInvocation", InjectSteps),
        (Antigravity, PostInvocation) => ("PostInvocation", InjectSteps),
        (Antigravity, PostToolUse) => ("PostToolUse", EmptyObject),
        (Antigravity, Stop) => ("Stop", StopDecision),
        (OpenCode, SystemTransform) => ("context", Text),
        (OpenCode, SessionCompacting) => ("compaction", Text),
        (OpenCode, Stop) => ("session.idle", Observe),
        (OpenCode, SessionEnd) => ("session.deleted", Observe),
        _ => return Err(OutputError::UnsupportedBoundary),
    };
    let channel = match event {
        SystemTransform => DeliveryChannel::PluginSystem,
        SessionCompacting => DeliveryChannel::PluginCompaction,
        _ => DeliveryChannel::CommandStdout,
    };
    Ok(Profile {
        name,
        wire,
        channel,
    })
}

/// Serialize without lifecycle effects. No returned bytes exist on any error.
pub fn encode(
    binding: NativeBinding,
    event: HookEvent,
    message: OutputMessage<'_>,
    context: &OutputContext<'_>,
) -> Result<EncodedOutput, OutputError> {
    check(context.request)?;
    let profile = profile(binding, event)?;
    let text = match message {
        OutputMessage::Quiet => None,
        OutputMessage::Context(text) => {
            if matches!(
                profile.wire,
                Wire::Observe | Wire::EmptyObject | Wire::StopDecision
            ) {
                return Err(OutputError::ContextUnavailable);
            }
            if text.len() > context.limits.max_context_bytes {
                return Err(OutputError::LimitExceeded);
            }
            // Byte admission precedes even the whitespace scan.
            if !nonblank(text, context.request)? {
                return Err(OutputError::EmptyContext);
            }
            check(context.request)?;
            Some(text)
        }
    };
    let mut writer = BufferWriter {
        request: context.request,
        bytes: Vec::new(),
        maximum: context.limits.max_output_bytes,
        failure: None,
    };
    if serialize(&profile, text, &mut writer).is_err() {
        return Err(writer.failure.unwrap_or(OutputError::Encoding));
    }
    check(context.request)?;
    Ok(EncodedOutput {
        binding,
        event,
        channel: profile.channel,
        bytes: writer.bytes,
    })
}

fn check(request: &SnapshotContext) -> Result<(), OutputError> {
    request.check().map_err(|error| match error.kind() {
        io::ErrorKind::Interrupted => OutputError::Cancelled,
        io::ErrorKind::TimedOut => OutputError::DeadlineExceeded,
        _ => OutputError::Encoding,
    })
}
fn nonblank(text: &str, request: &SnapshotContext) -> Result<bool, OutputError> {
    for (index, character) in text.chars().enumerate() {
        if index % 1024 == 0 {
            check(request)?;
        }
        if !character.is_whitespace() {
            return Ok(true);
        }
    }
    Ok(false)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EventOutput<'a> {
    hook_specific_output: EventContext<'a>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EventContext<'a> {
    hook_event_name: &'a str,
    additional_context: &'a str,
}
#[derive(Serialize)]
struct AdditionalOutput<'a> {
    additional_context: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Injection<'a> {
    inject_steps: [Ephemeral<'a>; 1],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Ephemeral<'a> {
    ephemeral_message: &'a str,
}
#[derive(Serialize)]
struct StopOutput {
    decision: &'static str,
}
#[derive(Serialize)]
struct EmptyOutput {}

fn serialize<W: Write>(profile: &Profile, text: Option<&str>, writer: &mut W) -> io::Result<()> {
    let json_error = |_: serde_json::Error| io::Error::from(io::ErrorKind::Other);
    match (profile.wire, text) {
        (Wire::EventJson, Some(body)) => serde_json::to_writer(
            writer,
            &EventOutput {
                hook_specific_output: EventContext {
                    hook_event_name: profile.name,
                    additional_context: body,
                },
            },
        )
        .map_err(json_error),
        (Wire::AdditionalContext, Some(body)) => serde_json::to_writer(
            writer,
            &AdditionalOutput {
                additional_context: body,
            },
        )
        .map_err(json_error),
        (Wire::InjectSteps, Some(body)) => serde_json::to_writer(
            writer,
            &Injection {
                inject_steps: [Ephemeral {
                    ephemeral_message: body,
                }],
            },
        )
        .map_err(json_error),
        (Wire::Text, Some(body)) => {
            for chunk in body.as_bytes().chunks(4096) {
                writer.write_all(chunk)?;
            }
            Ok(())
        }
        (Wire::StopDecision, None) => {
            serde_json::to_writer(writer, &StopOutput { decision: "stop" }).map_err(json_error)
        }
        (Wire::EmptyObject, None) => {
            serde_json::to_writer(writer, &EmptyOutput {}).map_err(json_error)
        }
        (_, None) => Ok(()),
        _ => Err(io::ErrorKind::InvalidInput.into()),
    }
}

struct BufferWriter<'a> {
    request: &'a SnapshotContext,
    bytes: Vec<u8>,
    maximum: usize,
    failure: Option<OutputError>,
}
impl BufferWriter<'_> {
    fn check(&mut self) -> io::Result<()> {
        if self.failure.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        check(self.request).map_err(|cause| self.fail(cause))
    }
    fn fail(&mut self, cause: OutputError) -> io::Error {
        if self.failure.is_none() {
            self.failure = Some(cause);
        }
        io::ErrorKind::Other.into()
    }
}
impl Write for BufferWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.check()?;
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|len| len > self.maximum)
        {
            return Err(self.fail(OutputError::LimitExceeded));
        }
        self.bytes
            .try_reserve_exact(bytes.len())
            .map_err(|_| self.fail(OutputError::Allocation))?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        // Never retry a persistent cancellation, including an empty chunk.
        self.write(bytes).map(|_| ())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};

    #[test]
    fn failed_output_append_preserves_first_limit_cause_and_buffer() {
        let cancellation = Cancellation::default();
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(10),
            cancellation.clone(),
        );
        let mut writer = BufferWriter {
            request: &original,
            bytes: Vec::new(),
            maximum: 2,
            failure: None,
        };
        writer.write_all(b"{}").unwrap();
        let capacity = writer.bytes.capacity();
        assert!(writer.write_all(b"extra").is_err());
        cancellation.cancel();
        assert!(writer.write_all(b"").is_err());
        assert!(writer.flush().is_err());
        assert_eq!(writer.failure, Some(OutputError::LimitExceeded));
        assert_eq!(writer.bytes, b"{}");
        assert_eq!(writer.bytes.capacity(), capacity);
    }
}
