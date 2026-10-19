use graphit_core::file_lock::Cancellation;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt, time::Instant};

/// Explicit caller policy; input, output and node quotas are aggregate per context.
#[derive(Clone, Copy, Debug)]
pub struct ContentLimits {
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_scalar_bytes: usize,
    pub max_collection_entries: usize,
}
impl ContentLimits {
    fn validate(self) -> Result<(), ContentError> {
        if [
            self.max_input_bytes,
            self.max_output_bytes,
            self.max_depth,
            self.max_nodes,
            self.max_scalar_bytes,
            self.max_collection_entries,
        ]
        .contains(&0)
            || self.max_depth > 80
            || self.max_input_bytes.checked_add(1).is_none()
            || self.max_nodes.checked_mul(4).is_none()
        {
            return Err(ContentError::InvalidPolicy);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentError {
    InvalidPolicy,
    InvalidContent,
    LimitExceeded,
    Cancelled,
    TimedOut,
    UnsupportedCapability,
}
impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPolicy => "invalid native content policy",
            Self::InvalidContent => "invalid native content",
            Self::LimitExceeded => "native content quota exceeded",
            Self::Cancelled => "native content operation cancelled",
            Self::TimedOut => "native content deadline exceeded",
            Self::UnsupportedCapability => {
                "native binding cannot represent the selected capability"
            }
        })
    }
}
impl std::error::Error for ContentError {}

/// One bounded batch. Failed work is not refunded and no environment is captured.
pub struct RenderContext<'a> {
    pub(crate) limits: ContentLimits,
    cancellation: &'a Cancellation,
    deadline: Option<Instant>,
    input_bytes: usize,
    output_bytes: usize,
    nodes: usize,
}
impl<'a> RenderContext<'a> {
    pub fn new(
        limits: ContentLimits,
        cancellation: &'a Cancellation,
        deadline: Instant,
    ) -> Result<Self, ContentError> {
        Self::with_deadline(limits, cancellation, Some(deadline))
    }
    pub(crate) fn with_deadline(
        limits: ContentLimits,
        cancellation: &'a Cancellation,
        deadline: Option<Instant>,
    ) -> Result<Self, ContentError> {
        limits.validate()?;
        let context = Self {
            limits,
            cancellation,
            deadline,
            input_bytes: 0,
            output_bytes: 0,
            nodes: 0,
        };
        context.check()?;
        Ok(context)
    }
    pub(crate) fn check(&self) -> Result<(), ContentError> {
        if self.cancellation.is_cancelled() {
            return Err(ContentError::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(ContentError::TimedOut);
        }
        Ok(())
    }
    pub(crate) fn input(&mut self, bytes: usize) -> Result<(), ContentError> {
        self.check()?;
        charge(&mut self.input_bytes, bytes, self.limits.max_input_bytes)
    }
    pub(crate) fn output(&mut self, bytes: usize) -> Result<(), ContentError> {
        self.check()?;
        charge(&mut self.output_bytes, bytes, self.limits.max_output_bytes)
    }
    pub fn input_bytes(&self) -> usize {
        self.input_bytes
    }
    pub fn output_bytes(&self) -> usize {
        self.output_bytes
    }

    pub(crate) fn tree(&mut self, value: &Value, depth: usize) -> Result<(), ContentError> {
        self.node(depth)?;
        match value {
            Value::Object(object) => {
                self.collection(object.len())?;
                for (key, value) in object {
                    self.scalar(key)?;
                    self.tree(value, depth + 1)?;
                }
            }
            Value::Array(array) => {
                self.collection(array.len())?;
                for value in array {
                    self.tree(value, depth + 1)?;
                }
            }
            Value::String(text) => self.scalar(text)?,
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn node(&mut self, depth: usize) -> Result<(), ContentError> {
        self.check()?;
        charge(&mut self.nodes, 1, self.limits.max_nodes)?;
        if depth > self.limits.max_depth {
            return Err(ContentError::LimitExceeded);
        }
        Ok(())
    }
    pub(crate) fn object(&mut self, keys: &[&str], depth: usize) -> Result<(), ContentError> {
        self.node(depth)?;
        self.collection(keys.len())?;
        for key in keys {
            self.scalar(key)?;
        }
        Ok(())
    }
    pub(crate) fn string(&mut self, text: &str, depth: usize) -> Result<(), ContentError> {
        self.node(depth)?;
        self.scalar(text)
    }
    pub(crate) fn scalar(&self, text: &str) -> Result<(), ContentError> {
        if text.len() > self.limits.max_scalar_bytes {
            Err(ContentError::LimitExceeded)
        } else {
            Ok(())
        }
    }
    pub(crate) fn collection(&self, length: usize) -> Result<(), ContentError> {
        if length > self.limits.max_collection_entries {
            Err(ContentError::LimitExceeded)
        } else {
            Ok(())
        }
    }
    fn yaml(&mut self, text: &str) -> Result<Value, ContentError> {
        self.check()?;
        let nodes = self
            .limits
            .max_nodes
            .checked_sub(self.nodes)
            .ok_or(ContentError::LimitExceeded)?;
        if nodes == 0 {
            return Err(ContentError::LimitExceeded);
        }
        let options = serde_saphyr::options! {
            budget: serde_saphyr::budget! {
                max_reader_input_bytes: Some(self.limits.max_input_bytes),
                max_events: nodes * 4,
                max_nodes: nodes * 2,
                max_depth: self.limits.max_depth,
                flow_nesting_limit: self.limits.max_depth,
                max_documents: 1,
                max_aliases: 0,
                max_anchors: 0,
                max_recorded_anchor_events: 0,
                max_recorded_anchor_bytes: 0,
                max_total_scalar_bytes: self.limits.max_input_bytes,
                max_total_comment_bytes: self.limits.max_input_bytes,
                max_merge_keys: 0,
                max_inclusion_depth: 0,
                simple_key_max_lookahead: self.limits.max_input_bytes,
            },
            alias_limits: serde_saphyr::alias_limits! {
                max_total_replayed_events: 0,
                max_replay_stack_depth: 0,
                max_alias_expansions_per_anchor: 0,
            },
            merge_keys: serde_saphyr::options::MergeKeyPolicy::Error,
            duplicate_keys: serde_saphyr::options::DuplicateKeyPolicy::Error,
            strict_booleans: true,
            reject_unsupported_tags: true,
            reject_non_finite_typeless_float: true,
            emit_comments: false,
            with_snippet: false,
        };
        let parsed: Result<Value, _> = serde_saphyr::from_str_with_options(text, options);
        if parsed.is_err() {
            self.nodes = self.limits.max_nodes;
        }
        let value: Value = parsed.map_err(|error| match error.without_snippet() {
            serde_saphyr::Error::Budget { breach, .. } => match breach {
                serde_saphyr::budget::BudgetBreach::Aliases { .. }
                | serde_saphyr::budget::BudgetBreach::Anchors { .. }
                | serde_saphyr::budget::BudgetBreach::Documents { .. }
                | serde_saphyr::budget::BudgetBreach::MergeKeys { .. } => {
                    ContentError::InvalidContent
                }
                _ => ContentError::LimitExceeded,
            },
            _ => ContentError::InvalidContent,
        })?;
        if !value.is_object() {
            return Err(ContentError::InvalidContent);
        }
        self.tree(&value, 1)?;
        Ok(value)
    }
}
fn charge(used: &mut usize, amount: usize, maximum: usize) -> Result<(), ContentError> {
    let next = used
        .checked_add(amount)
        .ok_or(ContentError::LimitExceeded)?;
    if next > maximum {
        return Err(ContentError::LimitExceeded);
    }
    *used = next;
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentMetadata {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub allows_shell: bool,
}

/// Validated content shared by Hub artifacts and managed roles. Body stays borrowed.
#[derive(Clone, Debug)]
pub struct AgentDescriptor<'a> {
    pub(crate) metadata: AgentMetadata,
    pub(crate) body: &'a str,
    pub(crate) input_fingerprint: [u8; 32],
}
impl<'a> AgentDescriptor<'a> {
    pub fn parse(input: &'a [u8], context: &mut RenderContext<'_>) -> Result<Self, ContentError> {
        context.input(input.len())?;
        let (header, body) = document(input)?;
        let metadata: AgentMetadata = serde_json::from_value(context.yaml(header)?)
            .map_err(|_| ContentError::InvalidContent)?;
        validate_metadata(&metadata.name, &metadata.description, context)?;
        Ok(Self {
            metadata,
            body,
            input_fingerprint: fingerprint(input, context)?,
        })
    }
    /// Trusted role producers use the same descriptor as parsed Hub agents.
    pub fn new(
        name: &str,
        description: &str,
        body: &'a str,
        read_only: bool,
        allows_shell: bool,
        context: &mut RenderContext<'_>,
    ) -> Result<Self, ContentError> {
        let length = name
            .len()
            .checked_add(description.len())
            .and_then(|n| n.checked_add(body.len()))
            .ok_or(ContentError::LimitExceeded)?;
        context.input(length)?;
        validate_metadata(name, description, context)?;
        validate_body(body)?;
        let metadata = AgentMetadata {
            name: name.into(),
            description: description.into(),
            read_only,
            allows_shell,
        };
        let value = serde_json::to_value(&metadata).map_err(|_| ContentError::InvalidContent)?;
        context.tree(&value, 1)?;
        // Length framing prevents distinct managed fields from sharing a preimage.
        let mut hash = Sha256::new();
        for bytes in [name.as_bytes(), description.as_bytes(), body.as_bytes()] {
            hash.update((bytes.len() as u64).to_le_bytes());
            for chunk in bytes.chunks(64 * 1024) {
                context.check()?;
                hash.update(chunk);
            }
        }
        hash.update([u8::from(read_only), u8::from(allows_shell)]);
        context.check()?;
        Ok(Self {
            metadata,
            body,
            input_fingerprint: hash.finalize().into(),
        })
    }
    pub fn name(&self) -> &str {
        &self.metadata.name
    }
    pub fn description(&self) -> &str {
        &self.metadata.description
    }
    pub fn body(&self) -> &str {
        self.body
    }
    pub fn read_only(&self) -> bool {
        self.metadata.read_only
    }
    pub fn allows_shell(&self) -> bool {
        self.metadata.allows_shell
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SkillMetadata {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}
#[derive(Clone, Debug)]
pub struct SkillDescriptor<'a> {
    pub(crate) metadata: SkillMetadata,
    pub(crate) body: &'a str,
    pub(crate) input_fingerprint: [u8; 32],
}
impl<'a> SkillDescriptor<'a> {
    pub fn parse(input: &'a [u8], context: &mut RenderContext<'_>) -> Result<Self, ContentError> {
        context.input(input.len())?;
        let (header, body) = document(input)?;
        let value = context.yaml(header)?;
        // Optional metadata has absence semantics; explicit null is invalid input.
        if value
            .as_object()
            .is_some_and(|m| m.values().any(Value::is_null))
        {
            return Err(ContentError::InvalidContent);
        }
        let metadata: SkillMetadata =
            serde_json::from_value(value).map_err(|_| ContentError::InvalidContent)?;
        validate_metadata(&metadata.name, &metadata.description, context)?;
        if metadata
            .compatibility
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.chars().count() > 500)
        {
            return Err(ContentError::InvalidContent);
        }
        Ok(Self {
            metadata,
            body,
            input_fingerprint: fingerprint(input, context)?,
        })
    }
    pub fn name(&self) -> &str {
        &self.metadata.name
    }
    pub fn description(&self) -> &str {
        &self.metadata.description
    }
    pub fn body(&self) -> &str {
        self.body
    }
    pub fn license(&self) -> Option<&str> {
        self.metadata.license.as_deref()
    }
    pub fn compatibility(&self) -> Option<&str> {
        self.metadata.compatibility.as_deref()
    }
    pub fn metadata(&self) -> Option<&BTreeMap<String, String>> {
        self.metadata.metadata.as_ref()
    }
}

pub(crate) fn validate_name(name: &str) -> Result<(), ContentError> {
    if name.is_empty()
        || name.len() > 64
        || name.starts_with('-')
        || name.ends_with('-')
        || name.contains("--")
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(ContentError::InvalidContent);
    }
    Ok(())
}
fn validate_metadata(
    name: &str,
    description: &str,
    context: &RenderContext<'_>,
) -> Result<(), ContentError> {
    validate_name(name)?;
    context.scalar(name)?;
    context.scalar(description)?;
    if description.is_empty() || description.chars().count() > 1024 {
        return Err(ContentError::InvalidContent);
    }
    Ok(())
}
fn validate_body(body: &str) -> Result<(), ContentError> {
    if body.trim().is_empty() {
        Err(ContentError::InvalidContent)
    } else {
        Ok(())
    }
}
/// Inspect a current PROJECT Coordinator profile under concrete native content limits.
/// This preserves foreign metadata values and the literal body; it conveys no authority.
pub fn inspect_kiro_coordinator_profile<'i>(
    input: &'i [u8],
    name: &str,
    context: &mut RenderContext<'_>,
) -> Result<(Value, &'i str), ContentError> {
    context.input(input.len())?;
    context.scalar(name)?;
    let (header, body) = document(input)?;
    let metadata = context.yaml(header)?;
    if ["scout", "tracker", "scribe", "dream"]
        .iter()
        .any(|role| name == *role || name.ends_with(&format!("-{role}")))
    {
        return Err(ContentError::UnsupportedCapability);
    }
    if name.is_empty()
        || metadata.get("name").and_then(Value::as_str) != Some(name)
        || metadata
            .get("description")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        || !metadata.get("tools").is_some_and(Value::is_array)
        || body.trim().is_empty()
    {
        return Err(ContentError::InvalidContent);
    }
    fn role_transport(value: &Value) -> bool {
        match value {
            Value::String(text) => text == "--role" || text.contains("_role-mcp"),
            Value::Array(values) => values.iter().any(role_transport),
            Value::Object(values) => values.values().any(role_transport),
            _ => false,
        }
    }
    if metadata.get("mcpServers").is_some_and(role_transport) {
        return Err(ContentError::UnsupportedCapability);
    }
    context.check()?;
    Ok((metadata, body))
}

fn document(input: &[u8]) -> Result<(&str, &str), ContentError> {
    let text = std::str::from_utf8(input).map_err(|_| ContentError::InvalidContent)?;
    let mut lines = text.split_inclusive('\n');
    if lines.next().map(|s| s.trim_end_matches(['\r', '\n'])) != Some("---") {
        return Err(ContentError::InvalidContent);
    }
    let start = text.find('\n').ok_or(ContentError::InvalidContent)? + 1;
    let mut offset = start;
    for line in lines {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let body = &text[offset + line.len()..];
            validate_body(body)?;
            return Ok((&text[start..offset], body));
        }
        offset += line.len();
    }
    Err(ContentError::InvalidContent)
}

pub(crate) fn fingerprint(
    bytes: &[u8],
    context: &RenderContext<'_>,
) -> Result<[u8; 32], ContentError> {
    let mut hash = Sha256::new();
    for chunk in bytes.chunks(64 * 1024) {
        context.check()?;
        hash.update(chunk);
    }
    context.check()?;
    Ok(hash.finalize().into())
}
