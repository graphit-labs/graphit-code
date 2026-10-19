//! Native command-hook plans. Shell commands are supplied by the actual host caller.
use super::{DocumentFormat, FieldValue, NativeBinding, document::TupleFields};
use crate::policy::{RequestContext, SharedWriteLimits};
use std::{collections::BTreeMap, io, time::Duration};

pub struct CommandHookInput<'a> {
    /// Private managed identity, not an invented native configuration field.
    pub identifier: &'a str,
    /// Native spelling already selected by the caller's runtime codec.
    pub native_event: &'a str,
    pub command: &'a str,
    pub matcher: Option<&'a str>,
    pub timeout: Option<Duration>,
}

/// A validated native schema; neither this plan nor a path grants publication access.
pub struct NativeHookPlan {
    pub(crate) identifier: String,
    pub(crate) binding: NativeBinding,
    pub(crate) event: String,
    pub(crate) path: Vec<String>,
    pub(crate) group: Option<TupleFields>,
    pub(crate) tuple: FieldValue,
    pub(crate) fields: TupleFields,
    format: DocumentFormat,
    project_config: Option<&'static str>,
}
impl NativeHookPlan {
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
    pub fn binding(&self) -> NativeBinding {
        self.binding
    }
    pub fn native_event(&self) -> &str {
        &self.event
    }
    pub fn format(&self) -> DocumentFormat {
        self.format
    }
    /// Kimi uses a captured global configuration directory, never a PROJECT copy.
    pub fn project_config_path(&self) -> Option<&'static str> {
        self.project_config
    }
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid native command hook")
}
fn codec(error: super::DocumentError) -> io::Error {
    let kind = match error {
        super::DocumentError::Interrupted => io::ErrorKind::Interrupted,
        super::DocumentError::TimedOut => io::ErrorKind::TimedOut,
        super::DocumentError::LimitExceeded => io::ErrorKind::FileTooLarge,
        _ => io::ErrorKind::InvalidInput,
    };
    io::Error::new(kind, error)
}

/// Plan only documented command hooks. This does not select defaults or install all codecs.
pub fn plan_command_hook(
    binding: NativeBinding,
    input: CommandHookInput<'_>,
    limits: SharedWriteLimits,
    request: &RequestContext<'_>,
) -> io::Result<NativeHookPlan> {
    request.remaining()?;
    use NativeBinding::*;
    let event = input.native_event;
    let supported = match binding {
        Codex | Claude | Qwen => matches!(
            event,
            "SessionStart"
                | "UserPromptSubmit"
                | "SubagentStart"
                | "PostToolUse"
                | "Stop"
                | "SessionEnd"
        ),
        Cursor => matches!(
            event,
            "sessionStart" | "postToolUse" | "stop" | "sessionEnd"
        ),
        Antigravity => matches!(
            event,
            "PreInvocation" | "PostInvocation" | "PostToolUse" | "Stop"
        ),
        Kimi => matches!(
            event,
            "SessionStart"
                | "UserPromptSubmit"
                | "SubagentStart"
                | "PostToolUse"
                | "Stop"
                | "SessionEnd"
        ),
        KiroV3 | OpenCode => false,
    };
    if !supported {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native command hook unavailable",
        ));
    }
    if input.identifier.is_empty()
        || input.identifier.len() > limits.scalar_bytes
        || !input
            .identifier
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
        || input.command.trim().is_empty()
        || input.command.contains('\0')
        || input.command.len() > limits.scalar_bytes
        || input
            .matcher
            .is_some_and(|m| m.contains('\0') || m.len() > limits.scalar_bytes)
    {
        return Err(invalid());
    }
    let grouped = matches!(binding, Codex | Claude | Qwen)
        || binding == Antigravity && event == "PostToolUse";
    let matcher_allowed = match binding {
        Codex | Claude | Qwen => matches!(
            event,
            "SessionStart" | "SubagentStart" | "PostToolUse" | "SessionEnd"
        ),
        Cursor => event == "postToolUse",
        Antigravity => event == "PostToolUse",
        Kimi => true,
        _ => false,
    };
    if input.matcher.is_some() && !matcher_allowed {
        return Err(invalid());
    }
    let mut values = BTreeMap::from([("command".into(), FieldValue::String(input.command.into()))]);
    if binding == Kimi {
        values.insert("event".into(), FieldValue::String(event.into()));
    } else {
        values.insert("type".into(), FieldValue::String("command".into()));
    }
    if !grouped && let Some(matcher) = input.matcher {
        values.insert("matcher".into(), FieldValue::String(matcher.into()));
    }
    let fields = ["type", "command", "event", "matcher"]
        .into_iter()
        .map(|key| (key.into(), values.get(key).cloned()))
        .collect();
    if let Some(timeout) = input.timeout {
        if timeout.is_zero() || timeout.subsec_nanos() != 0 {
            return Err(invalid());
        }
        let seconds = timeout.as_secs();
        if binding == Kimi && !(1..=600).contains(&seconds)
            || binding == Codex && event == "SessionEnd" && seconds > 3
        {
            return Err(invalid());
        }
        let native = if binding == Qwen && seconds >= 1000 {
            i64::try_from(timeout.as_millis()).map_err(|_| invalid())?
        } else {
            i64::try_from(seconds).map_err(|_| invalid())?
        };
        values.insert("timeout".into(), FieldValue::Integer(native));
    }
    let group = grouped.then(|| {
        BTreeMap::from([(
            "matcher".into(),
            input.matcher.map(|m| FieldValue::String(m.into())),
        )])
    });
    let path = match binding {
        Kimi => vec!["hooks".into()],
        Antigravity => vec![input.identifier.into(), event.into()],
        _ => vec!["hooks".into(), event.into()],
    };
    let project_config = match binding {
        Codex => Some(".codex/hooks.json"),
        Claude => Some(".claude/settings.json"),
        Cursor => Some(".cursor/hooks.json"),
        Qwen => Some(".qwen/settings.json"),
        Antigravity => Some(".agents/hooks.json"),
        Kimi => None,
        _ => return Err(invalid()),
    };
    let format = if binding == Kimi {
        DocumentFormat::Toml
    } else {
        DocumentFormat::Jsonc
    };
    let plan = NativeHookPlan {
        identifier: input.identifier.into(),
        binding,
        event: event.into(),
        path,
        group,
        tuple: FieldValue::Object(values),
        fields,
        format,
        project_config,
    };
    // Reuse the existing bounded native value validator/renderer before retaining a plan.
    let mut document = super::NativeDocument::parse(DocumentFormat::Json, b"{}", limits, request)
        .map_err(codec)?;
    document.set(&["hook"], plan.tuple.clone()).map_err(codec)?;
    document.render().map_err(codec)?;
    request.remaining()?;
    Ok(plan)
}
