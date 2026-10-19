//! Pure current Kiro hook plans. Publication and runtime authority remain with callers.
use super::{DocumentError, DocumentFormat, FieldValue, NativeBinding, NativeDocument, content};
use crate::policy::{RequestContext, SharedWriteLimits};
use std::{collections::BTreeMap, fmt, time::Duration};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KiroHookTrigger {
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    PreToolUse,
    PostToolUse,
    PostFileCreate,
    PostFileSave,
    PostFileDelete,
    PreTaskExec,
    PostTaskExec,
    Stop,
    /// Recognized by CLI V3; generating a plan does not invoke a manual hook.
    Manual,
}
impl KiroHookTrigger {
    fn native_name(self) -> &'static str {
        match self {
            Self::SessionStart => "SessionStart",
            Self::SessionEnd => "SessionEnd",
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::PostFileCreate => "PostFileCreate",
            Self::PostFileSave => "PostFileSave",
            Self::PostFileDelete => "PostFileDelete",
            Self::PreTaskExec => "PreTaskExec",
            Self::PostTaskExec => "PostTaskExec",
            Self::Stop => "Stop",
            Self::Manual => "Manual",
        }
    }
    fn accepts_matcher(self) -> bool {
        matches!(
            self,
            Self::PreToolUse
                | Self::PostToolUse
                | Self::PostFileCreate
                | Self::PostFileSave
                | Self::PostFileDelete
        )
    }
}
#[derive(Clone, Copy)]
pub enum KiroHookAction<'a> {
    /// Serialized for the target shell by the actual runtime producer.
    Command(&'a str),
    AgentPrompt(&'a str),
}
pub struct KiroHookInput<'a> {
    pub identifier: &'a str,
    pub trigger: KiroHookTrigger,
    pub matcher: Option<&'a str>,
    pub action: KiroHookAction<'a>,
    pub timeout: Option<Duration>,
}

/// One current-schema tuple, usable in profile-scoped or standalone hook arrays.
pub struct KiroHookPlan {
    identifier: String,
    trigger: KiroHookTrigger,
    tuple: FieldValue,
}
impl KiroHookPlan {
    pub fn binding(&self) -> NativeBinding {
        NativeBinding::KiroV3
    }
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
    pub fn trigger(&self) -> KiroHookTrigger {
        self.trigger
    }
    pub fn native_trigger(&self) -> &'static str {
        self.trigger.native_name()
    }
    pub fn array_path(&self) -> &'static [&'static str] {
        &["hooks"]
    }
    /// Standalone document schema; profile-scoped arrays contain the tuples directly.
    pub fn version(&self) -> &'static str {
        "v1"
    }
    pub fn tuple(&self) -> &FieldValue {
        &self.tuple
    }

    /// Native JSON data for the profile renderer, not the tagged SDK enum encoding.
    pub fn profile_value(&self) -> serde_json::Value {
        fn native(value: &FieldValue) -> serde_json::Value {
            match value {
                FieldValue::String(text) => serde_json::Value::String(text.clone()),
                FieldValue::Boolean(value) => serde_json::Value::Bool(*value),
                FieldValue::Integer(value) => serde_json::Value::from(*value),
                FieldValue::Array(values) => {
                    serde_json::Value::Array(values.iter().map(native).collect())
                }
                FieldValue::Object(values) => serde_json::Value::Object(
                    values
                        .iter()
                        .map(|(key, value)| (key.clone(), native(value)))
                        .collect(),
                ),
            }
        }
        native(&self.tuple)
    }

    pub fn critical_fields(&self) -> &'static [&'static str] {
        &["name", "trigger", "action", "matcher"]
    }
}
impl fmt::Debug for KiroHookPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KiroHookPlan")
            .field("binding", &self.binding())
            .field("trigger", &self.trigger)
            .field("identifier_bytes", &self.identifier.len())
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KiroHookError {
    UnsupportedBinding,
    UnsupportedAction,
    UnsupportedMatcher,
    InvalidIdentifier,
    InvalidInput,
    InvalidTimeout,
    Document(DocumentError),
}
impl fmt::Display for KiroHookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Kiro hook plan: {self:?}")
    }
}
impl std::error::Error for KiroHookError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Document(error) => Some(error),
            _ => None,
        }
    }
}
impl From<DocumentError> for KiroHookError {
    fn from(value: DocumentError) -> Self {
        Self::Document(value)
    }
}
pub fn plan_kiro_hook(
    binding: NativeBinding,
    input: KiroHookInput<'_>,
    limits: SharedWriteLimits,
    original: &RequestContext<'_>,
) -> Result<KiroHookPlan, KiroHookError> {
    let mut document = NativeDocument::parse(DocumentFormat::Json, b"{}", limits, original)?;
    if binding != NativeBinding::KiroV3 {
        return Err(KiroHookError::UnsupportedBinding);
    }
    content::validate_name(input.identifier).map_err(|_| KiroHookError::InvalidIdentifier)?;
    if input.matcher.is_some() && !input.trigger.accepts_matcher() {
        return Err(KiroHookError::UnsupportedMatcher);
    }
    let body = match input.action {
        KiroHookAction::Command(body) => body,
        KiroHookAction::AgentPrompt(body) => {
            if input.timeout.is_some() {
                return Err(KiroHookError::UnsupportedAction);
            }
            body
        }
    };
    let mut input_bytes = 0usize;
    for text in [Some(input.identifier), Some(body), input.matcher]
        .into_iter()
        .flatten()
    {
        input_bytes = input_bytes
            .checked_add(text.len())
            .ok_or(DocumentError::LimitExceeded)?;
        if text.len() > limits.scalar_bytes || input_bytes > limits.input_bytes {
            return Err(DocumentError::LimitExceeded.into());
        }
    }
    if body.trim().is_empty()
        || [Some(body), input.matcher]
            .into_iter()
            .flatten()
            .any(|text| text.contains('\0'))
    {
        return Err(KiroHookError::InvalidInput);
    }
    let timeout = input
        .timeout
        .map(|duration| {
            if duration.subsec_nanos() != 0 {
                return Err(KiroHookError::InvalidTimeout);
            }
            i64::try_from(duration.as_secs()).map_err(|_| KiroHookError::InvalidTimeout)
        })
        .transpose()?;
    let text = |value: &str| FieldValue::String(value.to_owned());
    let (kind, key) = match input.action {
        KiroHookAction::Command(_) => ("command", "command"),
        KiroHookAction::AgentPrompt(_) => ("agent", "prompt"),
    };
    let mut fields = BTreeMap::from([
        ("name".into(), text(input.identifier)),
        ("trigger".into(), text(input.trigger.native_name())),
        (
            "action".into(),
            FieldValue::Object(BTreeMap::from([
                ("type".into(), text(kind)),
                (key.into(), text(body)),
            ])),
        ),
    ]);
    if let Some(matcher) = input.matcher {
        fields.insert("matcher".into(), text(matcher));
    }
    if let Some(timeout) = timeout {
        fields.insert("timeout".into(), FieldValue::Integer(timeout));
    }
    let plan = KiroHookPlan {
        identifier: input.identifier.to_owned(),
        trigger: input.trigger,
        tuple: FieldValue::Object(fields),
    };
    document.set(&["version"], text(plan.version()))?;
    document.set(
        plan.array_path(),
        FieldValue::Array(vec![plan.tuple.clone()]),
    )?;
    Ok(plan)
}
#[cfg(test)]
mod tests;
