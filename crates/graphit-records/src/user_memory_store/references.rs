use super::model::*;
use super::{
    NativeUserMemoryStore, UserMemoryGeneration, UserMemoryOwner, UserMemoryScopeId, native,
};
use crate::memory_store::{
    MemoryId, MemoryReference, MemoryReferenceProvenance, MemoryReferenceResolution,
    MemoryReferenceScope, MemoryReferenceTarget, MemoryStoreError, MemoryStoreReadLimits,
    references::{
        Direction, Head, Heads, PinnedQuery, QueriedPage, ReadIdentity, ReadOwner, query_pin,
    },
};
use crate::reference_projection::{
    AuthoredReferenceEntry, AuthoredReferencesField, LogicalScope, ReferenceCandidate,
    ReferenceTargetKind, SourceNamespace, project_authored_references,
};
use graphit_auth::UserReadAdmission;
use graphit_core::{file_io::SnapshotContext, native_path::NativePath};
use std::sync::Arc;

fn validate_target(
    original: &SnapshotContext,
    scope: &UserMemoryScopeId,
    target: &MemoryReferenceTarget,
) -> Result<(), MemoryStoreError> {
    let project_scope;
    let (kind, scope_id) = match &target.scope {
        MemoryReferenceScope::Project(id) => {
            project_scope = id.to_string();
            ("project", project_scope.as_str())
        }
        MemoryReferenceScope::User(id) => ("user", id.as_str()),
    };
    let entries = [AuthoredReferenceEntry::Tuple(ReferenceCandidate {
        target_type: crate::memory_store::model::kind(target.target_kind),
        id: &target.id,
        scope: Some(kind),
        scope_id: Some(scope_id),
        context: target.context.as_deref(),
        relation: "backlink",
        field: None,
    })];
    project_authored_references(
        original,
        SourceNamespace {
            scope: LogicalScope::User(scope.as_str()),
            context: None,
        },
        AuthoredReferencesField::Sequence(&entries),
    )?;
    Ok(())
}
fn resolution<'a>(
    reference: &MemoryReference,
    scope: &UserMemoryScopeId,
    heads: &'a Heads,
) -> Option<(MemoryId, &'a str)> {
    if reference.target_kind != ReferenceTargetKind::Memory
        || reference.context.is_some()
        || !matches!(&reference.scope,MemoryReferenceScope::User(id) if id==scope.as_str())
    {
        return None;
    }
    let id: MemoryId = reference.id.parse().ok()?;
    if id.to_string() != reference.id {
        return None;
    }
    let head = heads.get(&id).filter(|h| !h.tombstone)?;
    Some((id, &head.title))
}
struct EdgeContext<'a> {
    generation: &'a UserMemoryGeneration,
    scope: &'a UserMemoryScopeId,
    original: &'a SnapshotContext,
}
fn edge(
    source: MemoryId,
    head: &Head,
    ordinal: usize,
    reference: &MemoryReference,
    heads: &Heads,
    context: &EdgeContext<'_>,
) -> Result<UserMemoryReferenceEdge, MemoryStoreError> {
    let target = resolution(reference, context.scope, heads);

    context.original.check()?;
    Ok(UserMemoryReferenceEdge {
        source,
        source_scope: context.scope.clone(),
        source_revision: head.revision,
        reference_generation: head.reference_generation,
        source_snapshot: context.generation.clone(),
        reference: reference.clone(),
        provenance: MemoryReferenceProvenance::Authored,
        ordinal,
        source_observation: MemoryReferenceResolution::Memory {
            id: source,
            title: head.title.clone(),
        },
        target_resolution: target.map_or(MemoryReferenceResolution::Unresolved, |(id, title)| {
            MemoryReferenceResolution::Memory {
                id,
                title: title.to_owned(),
            }
        }),
    })
}
fn offsets(
    query: &UserMemoryReferencesQuery,
    binding: &UserReferenceBinding,
) -> Result<(usize, usize), MemoryStoreError> {
    let offset = |cursor: &Option<UserMemoryReferencesCursor>, direction| match cursor {
        None => Ok(0),
        Some(c) if c.binding.as_ref() == binding && c.direction == direction => Ok(c.offset),
        _ => Err(MemoryStoreError::Invalid("USER Memory reference cursor")),
    };
    Ok((
        offset(&query.outgoing_cursor, Direction::Outgoing)?,
        offset(&query.incoming_cursor, Direction::Incoming)?,
    ))
}
fn preflight_cursor(
    query: &UserMemoryReferencesQuery,
    owner: &UserMemoryOwner,
    scope: &graphit_core::artifact_store::CapturedPublicationScope,
) -> Result<(), MemoryStoreError> {
    for (cursor, direction) in [
        (&query.outgoing_cursor, Direction::Outgoing),
        (&query.incoming_cursor, Direction::Incoming),
    ] {
        if let Some(c) = cursor
            && (c.direction != direction
                || c.binding.outgoing != query.outgoing
                || c.binding.incoming != query.incoming
                || c.binding.root.as_path() != scope.runtime_root().join("memory/index.lance")
                || c.binding.generation.owner != *owner
                || c.binding.generation.physical_branch.is_some()
                || c.binding.generation.inherited_target.is_some())
        {
            return Err(MemoryStoreError::Invalid("USER Memory reference cursor"));
        }
    }
    Ok(())
}
impl NativeUserMemoryStore {
    /// Query only the authorized USER Memory source from one captured SDK pin.
    pub async fn references(
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
        query: UserMemoryReferencesQuery,
        limits: MemoryStoreReadLimits,
    ) -> Result<UserMemoryReferencesRead, MemoryStoreError> {
        native::check_read(original, admission)?;
        if query.page_size.get() > 100
            || query.outgoing.is_none() && query.incoming.is_none()
            || query.outgoing.is_none() && query.outgoing_cursor.is_some()
            || query.incoming.is_none() && query.incoming_cursor.is_some()
        {
            return Err(MemoryStoreError::Invalid("USER Memory reference query"));
        }
        original.check()?;
        let user = admission.user()?.live;
        // No caller path is retained or used for IO; this locator comes from the admitted scope.
        let owner = native::owner(user)?;
        original.check()?;
        let scope = UserMemoryScopeId::from_owner(&owner)?;
        if let Some(target) = &query.incoming {
            validate_target(original, &scope, target)?;
        }
        preflight_cursor(&query, &owner, user)?;
        native::check_read(original, admission)?;
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return if query.outgoing_cursor.is_some() || query.incoming_cursor.is_some() {
                Err(MemoryStoreError::Invalid("USER Memory reference cursor"))
            } else {
                Ok(UserMemoryReferencesRead::NoCorpus)
            };
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok(())
        };
        before()?;
        let generation = native::generation(&capture.pin, &capture.owner);

        let root = user.runtime_root().join("memory/index.lance");
        original.check()?;
        let binding = UserReferenceBinding {
            corpus_key: capture.guard.corpus_key(capture.pin.corpus_uri()),
            root: NativePath::new(root)?,
            generation: generation.clone(),
            outgoing: query.outgoing,
            incoming: query.incoming.clone(),
        };
        let (outgoing_offset, incoming_offset) = offsets(&query, &binding)?;
        let context = EdgeContext {
            generation: &generation,
            scope: &scope,
            original,
        };
        let result = query_pin(
            &capture.pin,
            PinnedQuery {
                identity: ReadIdentity {
                    original,
                    owner: ReadOwner::User(&capture.owner),
                },
                outgoing: query.outgoing,
                incoming: query.incoming.as_ref(),
                page_size: query.page_size,
                outgoing_offset,
                incoming_offset,
                limits,
            },
            &before,
            |id, head, ordinal, reference, heads| {
                edge(id, head, ordinal, reference, heads, &context)
            },
            |direction, offset| {
                original.check()?;
                Ok(UserMemoryReferencesCursor {
                    binding: Box::new(binding.clone()),
                    direction,
                    offset,
                })
            },
        )
        .await?;
        let page = |p: QueriedPage<UserMemoryReferenceEdge, UserMemoryReferencesCursor>| {
            Box::new(UserMemoryReferencePage {
                results: p.results,
                total: p.total,
                complete: p.complete,
                next_cursor: p.next_cursor,
            })
        };
        let complete = result.warnings.is_empty();
        let output = UserMemoryReferencesRead::Published {
            generation,
            outgoing: result.outgoing.map(page),
            incoming: result.incoming.map(page),
            coverage: Box::new(UserMemoryReferencesCoverage {
                owner: capture.owner,
                memory_user_source_complete: complete,
                inbound_other_sources_not_qualified: true,
            }),
            warnings: result.warnings,
        };
        original.check()?;
        before()?;
        Ok(output)
    }
}
