//! FILE instructions use the same captured Markdown renderer as maintained instructions.
use super::*;
use graphit_runtime::instructions::{
    CapturedInstruction, InstructionLimits, InstructionOrigin, InstructionValue,
    InstructionVariable, render_instruction,
};
use std::{
    collections::BTreeMap,
    num::{NonZeroU64, NonZeroUsize},
};

pub(super) fn render(
    state: &State,
    source: &SourceRef,
    binding: graphit_hub::NativeBinding,
    entry: &str,
    bytes: &[u8],
    request: &RequestContext<'_>,
) -> io::Result<String> {
    let markdown = std::str::from_utf8(bytes)
        .map_err(|cause| io::Error::new(io::ErrorKind::InvalidData, cause))?;
    let brand = state.policy.installation.brand();
    let mut values = BTreeMap::from([
        ("brand".to_owned(), brand.key().to_owned()),
        ("brand_display".to_owned(), brand.key().to_owned()),
        (
            "brand_marker".to_owned(),
            brand.environment_prefix().to_owned(),
        ),
        ("artifact_id".to_owned(), source.id().as_str().to_owned()),
        (
            "artifact_version".to_owned(),
            source.version().as_str().to_owned(),
        ),
        ("agent_binding".to_owned(), binding.as_str().to_owned()),
    ]);
    // Resolve names through the existing namespace API. A template name never adds grants.
    for prefix in ["mcp_tool_", "skill_name_"] {
        for (offset, _) in markdown.match_indices(prefix) {
            let name = markdown[offset..]
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .next()
                .ok_or(io::ErrorKind::InvalidInput)?;
            let suffix = name
                .strip_prefix(prefix)
                .ok_or(io::ErrorKind::InvalidInput)?;
            let value = if prefix == "mcp_tool_" {
                brand.mcp_tool_name(&suffix.split('_').collect::<Vec<_>>())?
            } else {
                brand.skill_name(suffix)?
            };
            values.entry(name.to_owned()).or_insert(value);
        }
    }
    let variables: Vec<_> = values
        .iter()
        .map(|(name, value)| InstructionVariable {
            name,
            value: InstructionValue::Text(value),
        })
        .collect();
    let content = state.policy.content;
    let nz = |value| NonZeroUsize::new(value).ok_or(io::ErrorKind::InvalidInput);
    let limits = InstructionLimits {
        max_templates: nz(content.max_collection_entries)?,
        max_template_bytes: nz(content.max_input_bytes)?,
        max_values: nz(content.max_nodes)?,
        max_variable_bytes: nz(content.max_input_bytes)?,
        max_output_bytes: nz(content.max_output_bytes)?,
        fuel: NonZeroU64::new(
            u64::try_from(content.max_nodes)
                .map_err(io::Error::other)?
                .checked_mul(16)
                .ok_or(io::ErrorKind::InvalidInput)?,
        )
        .ok_or(io::ErrorKind::InvalidInput)?,
        recursion: nz(content.max_depth.min(500))?,
    };
    render_instruction(
        entry,
        &[CapturedInstruction {
            name: entry,
            origin: InstructionOrigin::Hub,
            markdown,
        }],
        &variables,
        limits,
        request.snapshot(),
    )
    .map(|value| value.into_body())
    .map_err(|cause| match cause {
        graphit_runtime::instructions::InstructionError::Request(cause) => cause,
        graphit_runtime::instructions::InstructionError::LimitExceeded(_) => {
            io::Error::new(io::ErrorKind::FileTooLarge, cause)
        }
        _ => io::Error::new(io::ErrorKind::InvalidData, cause),
    })
}
