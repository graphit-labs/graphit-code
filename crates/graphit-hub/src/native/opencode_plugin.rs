//! OpenCode's native plugin glue. Runtime context is never rendered into this file.
use super::{ContentError, RenderContext};
use crate::policy::RequestContext;
use serde::Serialize;
use std::{io, process::Command, time::Duration};
mod publication;
pub use publication::{PluginPublicationError, publish_opencode_plugin, remove_opencode_plugin};

pub struct OpenCodePluginInput<'a> {
    pub identifier: &'a str,
    pub context: &'a Command,
    pub compaction: &'a Command,
    pub idle: Option<&'a Command>,
    pub deleted: Option<&'a Command>,
    pub timeout: Duration,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
}
pub struct OpenCodePluginPlan {
    identifier: String,
    relative_entry: String,
    bytes: Vec<u8>,
}
impl OpenCodePluginPlan {
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
    pub fn relative_entry(&self) -> &str {
        &self.relative_entry
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
#[derive(Serialize)]
struct Invocation<'a> {
    program: &'a str,
    args: Vec<&'a str>,
}
fn invocation<'a>(
    command: &'a Command,
    context: &mut RenderContext<'_>,
) -> io::Result<Invocation<'a>> {
    // Credentials/env and ambient process cwd never become a generated plugin snapshot.
    if command.get_envs().next().is_some() || command.get_current_dir().is_some() {
        return Err(invalid());
    }
    let program = command
        .get_program()
        .to_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(invalid)?;
    if program.contains('\0') {
        return Err(invalid());
    }
    context.input(program.len()).map_err(codec)?;
    context.string(program, 3).map_err(codec)?;
    let mut args = Vec::new();
    for arg in command.get_args() {
        context
            .collection(args.len().checked_add(1).ok_or_else(invalid)?)
            .map_err(codec)?;
        let arg = arg
            .to_str()
            .filter(|a| !a.contains('\0'))
            .ok_or_else(invalid)?;
        context.input(arg.len()).map_err(codec)?;
        context.string(arg, 4).map_err(codec)?;
        args.push(arg);
    }
    Ok(Invocation { program, args })
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid OpenCode plugin plan")
}
fn codec(error: ContentError) -> io::Error {
    let kind = match error {
        ContentError::Cancelled => io::ErrorKind::Interrupted,
        ContentError::TimedOut => io::ErrorKind::TimedOut,
        ContentError::LimitExceeded => io::ErrorKind::FileTooLarge,
        _ => io::ErrorKind::InvalidInput,
    };
    io::Error::new(kind, error)
}

/// The caller supplies invocations from its one typed Graphit command producer.
pub fn render_opencode_plugin(
    input: OpenCodePluginInput<'_>,
    request: &RequestContext<'_>,
) -> io::Result<OpenCodePluginPlan> {
    request.remaining()?;
    let limits = request.content_limits();
    if input.identifier.is_empty()
        || input.identifier.len() > limits.max_scalar_bytes
        || !input
            .identifier
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        || input.timeout.is_zero()
        || !input.timeout.subsec_nanos().is_multiple_of(1_000_000)
        || input.max_input_bytes == 0
        || input.max_input_bytes > limits.max_input_bytes
        || input.max_output_bytes == 0
        || input.max_output_bytes > limits.max_output_bytes
    {
        return Err(invalid());
    }
    let timeout = u32::try_from(input.timeout.as_millis()).map_err(|_| invalid())?;
    // Node timers have a signed 32-bit millisecond range.
    if timeout > i32::MAX as u32 {
        return Err(invalid());
    }
    #[derive(Serialize)]
    struct Configuration<'a> {
        identifier: &'a str,
        context: Invocation<'a>,
        compaction: Invocation<'a>,
        idle: Option<Invocation<'a>>,
        deleted: Option<Invocation<'a>>,
        timeout: u32,
        max_input_bytes: usize,
        max_output_bytes: usize,
    }
    let mut context = RenderContext::new(
        limits,
        request.cancellation(),
        request.snapshot().required_deadline()?,
    )
    .map_err(codec)?;
    context.input(input.identifier.len()).map_err(codec)?;
    context.string(input.identifier, 2).map_err(codec)?;
    let configuration = Configuration {
        identifier: input.identifier,
        context: invocation(input.context, &mut context)?,
        compaction: invocation(input.compaction, &mut context)?,
        idle: input
            .idle
            .map(|c| invocation(c, &mut context))
            .transpose()?,
        deleted: input
            .deleted
            .map(|c| invocation(c, &mut context))
            .transpose()?,
        timeout,
        max_input_bytes: input.max_input_bytes,
        max_output_bytes: input.max_output_bytes,
    };
    let template = include_str!("opencode_plugin.mjs");
    let prefix = "import { execFile } from 'node:child_process';\nimport { resolve } from 'node:path';\nconst configuration = ";
    let fixed = prefix
        .len()
        .checked_add(2)
        .and_then(|n| n.checked_add(template.len()))
        .ok_or_else(invalid)?;
    context.output(fixed).map_err(codec)?;
    let encoded = super::render::json(&configuration, &mut context).map_err(codec)?;
    let length = prefix
        .len()
        .checked_add(encoded.len())
        .and_then(|n| n.checked_add(2))
        .and_then(|n| n.checked_add(template.len()))
        .ok_or_else(invalid)?;
    if length > limits.max_output_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    bytes.extend_from_slice(prefix.as_bytes());
    bytes.extend_from_slice(&encoded);
    bytes.extend_from_slice(b";\n");
    bytes.extend_from_slice(template.as_bytes());
    request.remaining()?;
    Ok(OpenCodePluginPlan {
        identifier: input.identifier.into(),
        relative_entry: format!(".opencode/plugins/{}.js", input.identifier),
        bytes,
    })
}
