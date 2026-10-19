//! Transient first-prompt context authorized for this run's own recipient.
use super::{InstallResources, RunRequest};
use crate::project_hook_context::ProjectMemoryRows;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, VerifiedRoleGrant};
use graphit_core::module_settings::Module;
use graphit_hub::policy::RequestContext;
use graphit_records::memory_store::{MemoryListFilter, MemoryListRead, NativeMemoryStore};
use std::{borrow::Cow, io};

pub(super) async fn prompt<'a>(
    resources: &InstallResources<'_>,
    selected: usize,
    input: &'a RunRequest,
    original: &RequestContext<'_>,
    recipient: graphit_hub::native::Role,
    grant: &VerifiedRoleGrant<'_>,
) -> io::Result<Cow<'a, str>> {
    original.snapshot().check()?;
    let modules = resources
        .modules
        .get(selected)
        .ok_or(io::ErrorKind::InvalidInput)?;
    if input.prompt.is_empty() || input.prompt.len() > resources.policy.content.max_input_bytes {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if recipient == graphit_hub::native::Role::Tracker || !modules.enabled(Module::Memory) {
        return Ok(Cow::Borrowed(&input.prompt));
    }
    let authority = ProjectRecordAuthority::new(resources.auth.issuer());
    let admission = authority
        .admit_read(grant, original, ProjectReadModule::Memory)
        .map_err(io::Error::other)?;
    let (state, memories) = match NativeMemoryStore::list(
        original.snapshot(),
        &admission,
        MemoryListFilter {
            mandatory: Some(true),
            ..Default::default()
        },
        resources.memory_read,
    )
    .await
    .map_err(io::Error::other)?
    {
        MemoryListRead::NoCorpus => ("no_corpus", Vec::new()),
        MemoryListRead::Published { memories, .. } => ("published", memories),
    };
    let capture = admission.project().map_err(io::Error::other)?;
    let text = crate::role_tool_server::session_hook::compose_memory(
        capture.version.version(),
        state,
        ProjectMemoryRows(&memories),
        Some(&input.prompt),
        resources.policy.content.max_input_bytes,
        original.snapshot(),
    )?;
    admission.recheck().map_err(io::Error::other)?;
    original.snapshot().check()?;
    Ok(Cow::Owned(text))
}
