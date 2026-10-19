//! Render captured Markdown instructions without discovering or publishing content.
//! Request checks are cooperative; SDK fuel and recursion limits do not bound RSS.
use graphit_core::file_io::SnapshotContext;
use minijinja::{AutoEscape, Environment, ErrorKind, UndefinedBehavior, value::Value};
use std::{
    collections::BTreeMap,
    fmt, io,
    num::{NonZeroU64, NonZeroUsize},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum InstructionOrigin {
    Project,
    Global,
    Hub,
    CompiledDefault,
}

pub struct CapturedInstruction<'a> {
    pub name: &'a str,
    pub origin: InstructionOrigin,
    pub markdown: &'a str,
}
#[derive(Clone, Copy)]
pub enum InstructionValue<'a> {
    Text(&'a str),
    Bool(bool),
    Unsigned(u64),
    TextList(&'a [&'a str]),
}
pub struct InstructionVariable<'a> {
    pub name: &'a str,
    pub value: InstructionValue<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct InstructionLimits {
    pub max_templates: NonZeroUsize,
    /// Entry plus names and Markdown of every captured candidate, including losers.
    pub max_template_bytes: NonZeroUsize,
    /// Variable entries plus text-list items.
    pub max_values: NonZeroUsize,
    /// Names/text bytes; bool counts one byte, integers count decimal digits.
    pub max_variable_bytes: NonZeroUsize,
    pub max_output_bytes: NonZeroUsize,
    pub fuel: NonZeroU64,
    /// MiniJinja's supported range is 1..=500 without stack growth.
    pub recursion: NonZeroUsize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstructionLimit {
    Templates,
    TemplateBytes,
    Values,
    VariableBytes,
    OutputBytes,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstructionErrorKind {
    InvalidName,
    InvalidVariable,
    DuplicateTemplate,
    DuplicateVariable,
    MissingTemplate,
    InvalidLimits,
    LimitExceeded(InstructionLimit),
    Allocation,
    Request(io::ErrorKind),
    Template(ErrorKind),
}
pub enum InstructionError {
    InvalidName,
    InvalidVariable,
    DuplicateTemplate,
    DuplicateVariable,
    MissingTemplate,
    InvalidLimits,
    LimitExceeded(InstructionLimit),
    Allocation,
    Request(io::Error),
    Template(minijinja::Error),
}
impl InstructionError {
    pub fn kind(&self) -> InstructionErrorKind {
        match self {
            Self::InvalidName => InstructionErrorKind::InvalidName,
            Self::InvalidVariable => InstructionErrorKind::InvalidVariable,
            Self::DuplicateTemplate => InstructionErrorKind::DuplicateTemplate,
            Self::DuplicateVariable => InstructionErrorKind::DuplicateVariable,
            Self::MissingTemplate => InstructionErrorKind::MissingTemplate,
            Self::InvalidLimits => InstructionErrorKind::InvalidLimits,
            Self::LimitExceeded(v) => InstructionErrorKind::LimitExceeded(*v),
            Self::Allocation => InstructionErrorKind::Allocation,
            Self::Request(e) => InstructionErrorKind::Request(e.kind()),
            Self::Template(e) => InstructionErrorKind::Template(e.kind()),
        }
    }
}
impl fmt::Debug for InstructionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("InstructionError")
            .field(&self.kind())
            .finish()
    }
}
impl fmt::Display for InstructionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("instruction rendering failed")
    }
}
impl std::error::Error for InstructionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Template(e) => Some(e),
            _ => None,
        }
    }
}
pub struct InstructionSelection {
    name: String,
    origin: InstructionOrigin,
}
impl InstructionSelection {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn origin(&self) -> InstructionOrigin {
        self.origin
    }
}
pub struct RenderedInstruction {
    body: String,
    origin: InstructionOrigin,
    selections: Vec<InstructionSelection>,
}
impl RenderedInstruction {
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn into_body(self) -> String {
        self.body
    }
    pub fn origin(&self) -> InstructionOrigin {
        self.origin
    }
    /// All selected logical templates, not a claim that every one was evaluated.
    pub fn selections(&self) -> &[InstructionSelection] {
        &self.selections
    }
}
impl fmt::Debug for RenderedInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderedInstruction")
            .field("bytes", &self.body.len())
            .field("origin", &self.origin)
            .field("selected", &self.selections.len())
            .finish()
    }
}

fn charge(
    total: &mut usize,
    amount: usize,
    maximum: NonZeroUsize,
    kind: InstructionLimit,
) -> Result<(), InstructionError> {
    *total = total
        .checked_add(amount)
        .filter(|v| *v <= maximum.get())
        .ok_or(InstructionError::LimitExceeded(kind))?;
    Ok(())
}
fn name_valid(name: &str) -> bool {
    name.ends_with(".md")
        && !name.starts_with("defaults/")
        && !name.contains(['\\', ':'])
        && !name.chars().any(char::is_control)
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
fn variable_valid(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn checked_request(original: &SnapshotContext) -> Result<(), InstructionError> {
    original.check().map_err(InstructionError::Request)
}
fn copy(text: &str) -> Result<String, InstructionError> {
    let mut value = String::new();
    value
        .try_reserve_exact(text.len())
        .map_err(|_| InstructionError::Allocation)?;
    value.push_str(text);
    Ok(value)
}

/// Resolve a captured bundle and render only its requested entry.
/// Required missing lookups fail. Explicit SDK optional constructs remain supported.
/// This function has no filesystem loader or callback through which templates can do I/O.
pub fn render_instruction(
    entry: &str,
    candidates: &[CapturedInstruction<'_>],
    variables: &[InstructionVariable<'_>],
    limits: InstructionLimits,
    original: &SnapshotContext,
) -> Result<RenderedInstruction, InstructionError> {
    checked_request(original)?;
    if limits.recursion.get() > 500 {
        return Err(InstructionError::InvalidLimits);
    }
    let mut template_bytes = 0;
    charge(
        &mut template_bytes,
        entry.len(),
        limits.max_template_bytes,
        InstructionLimit::TemplateBytes,
    )?;
    if !name_valid(entry) {
        return Err(InstructionError::InvalidName);
    }
    let mut count = 0;
    charge(
        &mut count,
        candidates.len(),
        limits.max_templates,
        InstructionLimit::Templates,
    )?;
    let mut captured = BTreeMap::new();
    for candidate in candidates {
        checked_request(original)?;
        charge(
            &mut template_bytes,
            candidate.name.len(),
            limits.max_template_bytes,
            InstructionLimit::TemplateBytes,
        )?;
        charge(
            &mut template_bytes,
            candidate.markdown.len(),
            limits.max_template_bytes,
            InstructionLimit::TemplateBytes,
        )?;
        if !name_valid(candidate.name) {
            return Err(InstructionError::InvalidName);
        }
        if captured
            .insert((candidate.name, candidate.origin), candidate.markdown)
            .is_some()
        {
            return Err(InstructionError::DuplicateTemplate);
        }
    }
    let mut values = 0;
    let mut variable_bytes = 0;
    let mut context = BTreeMap::new();
    for variable in variables {
        checked_request(original)?;
        charge(&mut values, 1, limits.max_values, InstructionLimit::Values)?;
        charge(
            &mut variable_bytes,
            variable.name.len(),
            limits.max_variable_bytes,
            InstructionLimit::VariableBytes,
        )?;
        if !variable_valid(variable.name) {
            return Err(InstructionError::InvalidVariable);
        }
        if context.contains_key(variable.name) {
            return Err(InstructionError::DuplicateVariable);
        }
        let value = match variable.value {
            InstructionValue::Text(text) => {
                charge(
                    &mut variable_bytes,
                    text.len(),
                    limits.max_variable_bytes,
                    InstructionLimit::VariableBytes,
                )?;
                Value::from(text)
            }
            InstructionValue::Bool(boolean) => {
                charge(
                    &mut variable_bytes,
                    1,
                    limits.max_variable_bytes,
                    InstructionLimit::VariableBytes,
                )?;
                Value::from(boolean)
            }
            InstructionValue::Unsigned(number) => {
                let digits = if number == 0 {
                    1
                } else {
                    number.ilog10() as usize + 1
                };
                charge(
                    &mut variable_bytes,
                    digits,
                    limits.max_variable_bytes,
                    InstructionLimit::VariableBytes,
                )?;
                Value::from(number)
            }
            InstructionValue::TextList(texts) => {
                charge(
                    &mut values,
                    texts.len(),
                    limits.max_values,
                    InstructionLimit::Values,
                )?;
                for text in texts {
                    checked_request(original)?;
                    charge(
                        &mut variable_bytes,
                        text.len(),
                        limits.max_variable_bytes,
                        InstructionLimit::VariableBytes,
                    )?;
                }
                texts.iter().map(|text| Value::from(*text)).collect()
            }
        };
        context.insert(variable.name, value);
    }
    let mut selected = BTreeMap::new();
    let mut defaults = BTreeMap::new();
    for ((name, origin), markdown) in captured {
        selected.entry(name).or_insert((origin, markdown));
        if origin == InstructionOrigin::CompiledDefault {
            defaults.insert(name, markdown);
        }
    }
    let origin = selected
        .get(entry)
        .map(|(origin, _)| *origin)
        .ok_or(InstructionError::MissingTemplate)?;
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_auto_escape_callback(|_| AutoEscape::None);
    env.set_keep_trailing_newline(true);
    env.set_trim_blocks(false);
    env.set_lstrip_blocks(false);
    env.set_recursion_limit(limits.recursion.get());
    env.set_fuel(Some(limits.fuel.get()));
    let mut selections = Vec::new();
    selections
        .try_reserve_exact(selected.len())
        .map_err(|_| InstructionError::Allocation)?;
    for (name, (origin, markdown)) in selected {
        checked_request(original)?;
        env.add_template(name, markdown)
            .map_err(InstructionError::Template)?;
        selections.push(InstructionSelection {
            name: copy(name)?,
            origin,
        });
    }
    for (name, markdown) in defaults {
        checked_request(original)?;
        let mut alias = copy("defaults/")?;
        alias
            .try_reserve_exact(name.len())
            .map_err(|_| InstructionError::Allocation)?;
        alias.push_str(name);
        env.add_template_owned(alias, markdown)
            .map_err(InstructionError::Template)?;
    }
    checked_request(original)?;
    let template = env
        .get_template(entry)
        .map_err(InstructionError::Template)?;
    let mut writer = BoundedWriter {
        original,
        maximum: limits.max_output_bytes,
        body: String::new(),
        cause: None,
    };
    let result = template.render_captured_to(Value::from_iter(context), &mut writer);
    // The writer's first domain cause precedes the SDK's wrapping error.
    if let Some(cause) = writer.cause.take() {
        return Err(cause);
    }
    let captured = result.map_err(InstructionError::Template)?;
    drop(captured);
    checked_request(original)?;
    Ok(RenderedInstruction {
        body: writer.body,
        origin,
        selections,
    })
}

struct BoundedWriter<'a> {
    original: &'a SnapshotContext,
    maximum: NonZeroUsize,
    body: String,
    cause: Option<InstructionError>,
}
impl BoundedWriter<'_> {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.cause.is_some() {
            return Err(io::Error::other("instruction writer already failed"));
        }
        let result = (|| {
            checked_request(self.original)?;
            let mut length = self.body.len();
            charge(
                &mut length,
                bytes.len(),
                self.maximum,
                InstructionLimit::OutputBytes,
            )?;
            let text = std::str::from_utf8(bytes).map_err(|e| {
                InstructionError::Request(io::Error::new(io::ErrorKind::InvalidData, e))
            })?;
            self.body
                .try_reserve_exact(bytes.len())
                .map_err(|_| InstructionError::Allocation)?;
            self.body.push_str(text);
            Ok(())
        })();
        if let Err(error) = result {
            let kind = match &error {
                InstructionError::Request(e) => e.kind(),
                _ => io::ErrorKind::Other,
            };
            self.cause = Some(error);
            return Err(io::Error::new(kind, "instruction writer failed"));
        }
        Ok(())
    }
}
impl io::Write for BoundedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.append(bytes).map(|()| bytes.len())
    }
    // std's default write_all retries Interrupted, which would erase cancellation.
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.append(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.append(&[])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};
    #[test]
    fn checked_admission_does_not_wrap() {
        let mut count = usize::MAX;
        assert!(matches!(
            charge(
                &mut count,
                1,
                NonZeroUsize::MAX,
                InstructionLimit::TemplateBytes
            ),
            Err(InstructionError::LimitExceeded(
                InstructionLimit::TemplateBytes
            ))
        ));
    }
    #[test]
    fn writer_keeps_first_cause_and_checks_empty_writes() {
        use io::Write;
        let cancellation = Cancellation::default();
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(10),
            cancellation.clone(),
        );
        let mut writer = BoundedWriter {
            original: &original,
            maximum: NonZeroUsize::MIN,
            body: String::new(),
            cause: None,
        };
        assert!(writer.write_all(b"xx").is_err());
        cancellation.cancel();
        assert!(writer.write_all(&[]).is_err());
        assert!(matches!(
            writer.cause,
            Some(InstructionError::LimitExceeded(
                InstructionLimit::OutputBytes
            ))
        ));
        let mut cancelled = BoundedWriter {
            original: &original,
            maximum: NonZeroUsize::MIN,
            body: String::new(),
            cause: None,
        };
        assert_eq!(
            cancelled.write_all(&[]).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(
            matches!(cancelled.cause, Some(InstructionError::Request(ref e)) if e.kind() == io::ErrorKind::Interrupted)
        );
    }
}
