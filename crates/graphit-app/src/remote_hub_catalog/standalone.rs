//! Standalone discovery through the captured S3 handle and current shared ACLs.
use super::*;
use crate::remote_file_install::storage;
use graphit_hub::project_metadata::{HubProjectMetadata, project_metadata_key};
use object_store::{
    ObjectStore,
    list::{PaginatedListOptions, PaginatedListStore},
};
use std::{sync::Arc, time::Instant};

pub(super) async fn projects(
    lease: &ServiceCredentialLease<'_>,
    subject: Option<&VerifiedUserSubject<'_>>,
    input: &client::ProjectsArguments,
    policy: &HubResourcePolicy,
    allow_loopback_http: bool,
    before: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<ProjectPage> {
    let deadline = lease
        .original()
        .map_err(io::Error::other)?
        .snapshot()
        .required_deadline()?;
    let storage = storage::open(
        lease,
        allow_loopback_http,
        deadline,
        deadline.saturating_duration_since(Instant::now()),
    )
    .await?;
    let store: Arc<dyn ObjectStore> = storage.store.clone();
    let check = || -> io::Result<()> {
        before()?;
        lease.check().map_err(io::Error::other)?;
        if storage.expires_at.is_some_and(|e| Instant::now() >= e) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if let Some(subject) = subject {
            subject.check().map_err(io::Error::other)?;
        }
        Ok(())
    };
    check()?;
    let grants = access::capture(lease, subject, &store, &storage.prefix, policy, &check).await?;
    let view = lease.standalone_s3().map_err(io::Error::other)?;
    let scope = binding((
        "standalone",
        &input.provider,
        &input.profile,
        &input.identity_provider,
        view.location.bucket.as_deref(),
        view.location.endpoint.as_deref(),
        view.location.region.as_deref(),
        &storage.prefix,
        &grants.revision,
        input.id,
        &input.name_prefix,
        input.limit,
    ))?;
    let previous = cursor(input, Some(&scope), policy)?;
    if previous
        .as_ref()
        .is_some_and(|c| c.broker.is_some() || c.after.is_none())
        || (input.id.is_some() && previous.is_some())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let after = previous.and_then(|c| c.after);
    if let Some(after) = after {
        if !grants.needs_discovery() && !grants.ids().contains(&after) {
            return Err(restart());
        }
        let Some(project) = metadata(lease, &store, &storage.prefix, after, policy, &check).await?
        else {
            return Err(restart());
        };
        if !grants.allows(project.id, &project.name)
            || input
                .name_prefix
                .as_ref()
                .is_some_and(|p| !project.name.starts_with(p))
        {
            return Err(restart());
        }
    }
    let selection = Selection {
        lease,
        subject,
        store: &store,
        prefix: &storage.prefix,
        input,
        policy,
        grants: &grants,
        check: &check,
    };
    let mut projects = Vec::new();
    let mut more = false;
    if let Some(id) = input.id {
        if !grants.needs_discovery() && !grants.ids().contains(&id) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if let Some(project) = metadata(lease, &store, &storage.prefix, id, policy, &check).await? {
            if !grants.allows(project.id, &project.name) {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            projects.push(Project {
                id: project.id,
                name: project.name,
            });
        }
    } else if !grants.needs_discovery() {
        for id in grants
            .ids()
            .into_iter()
            .filter(|id| after.is_none_or(|after| *id > after))
        {
            if selection.accept(id, &mut projects).await? {
                more = true;
                break;
            }
        }
    } else {
        let prefix = format!("{}/", access::key(&storage.prefix, "v1/projects")?);
        let offset = after.map(|id| format!("{prefix}{id}/"));
        let mut token = None;
        loop {
            check()?;
            current(
                lease,
                subject,
                &store,
                &storage.prefix,
                policy,
                &grants,
                &check,
            )
            .await?;
            check()?;
            let options = PaginatedListOptions {
                delimiter: Some("/".into()),
                max_keys: Some(input.limit),
                offset: offset.clone(),
                page_token: token.clone(),
                ..Default::default()
            };
            let page = wait(
                lease.original().map_err(io::Error::other)?.snapshot(),
                storage.store.list_paginated(Some(&prefix), options),
            )
            .await?;
            check()?;
            if page
                .result
                .common_prefixes
                .len()
                .saturating_add(page.result.objects.len())
                > input.limit
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let mut candidates = page.result.common_prefixes;
            candidates.sort();
            for candidate in candidates {
                let id = candidate
                    .as_ref()
                    .strip_prefix(&prefix)
                    .ok_or(io::ErrorKind::InvalidData)?
                    .trim_end_matches('/')
                    .parse::<ProjectId>()
                    .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
                if after.is_some_and(|after| id <= after) {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                if selection.accept(id, &mut projects).await? {
                    more = true;
                    break;
                }
            }
            if more {
                break;
            }
            match page.page_token {
                None => break,
                Some(next) if next.is_empty() || token.as_ref() == Some(&next) => {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                Some(next) => token = Some(next),
            }
        }
    }
    check()?;
    current(
        lease,
        subject,
        &store,
        &storage.prefix,
        policy,
        &grants,
        &check,
    )
    .await?;
    for project in &projects {
        let current = metadata(lease, &store, &storage.prefix, project.id, policy, &check)
            .await?
            .ok_or_else(restart)?;
        if current.name != project.name || !grants.allows(current.id, &current.name) {
            return Err(restart());
        }
    }
    current(
        lease,
        subject,
        &store,
        &storage.prefix,
        policy,
        &grants,
        &check,
    )
    .await?;
    check()?;
    let next = if more {
        Some(next_cursor(
            &scope,
            Some(projects.last().ok_or(io::ErrorKind::InvalidData)?.id),
            None,
            policy,
        )?)
    } else {
        None
    };
    Ok(ProjectPage {
        projects,
        next_cursor: next,
    })
}
async fn current(
    lease: &ServiceCredentialLease<'_>,
    subject: Option<&VerifiedUserSubject<'_>>,
    store: &Arc<dyn ObjectStore>,
    prefix: &str,
    policy: &HubResourcePolicy,
    expected: &access::Access,
    check: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<()> {
    if access::capture(lease, subject, store, prefix, policy, check)
        .await?
        .revision
        != expected.revision
    {
        return Err(restart());
    }
    Ok(())
}
async fn metadata(
    lease: &ServiceCredentialLease<'_>,
    store: &Arc<dyn ObjectStore>,
    prefix: &str,
    id: ProjectId,
    policy: &HubResourcePolicy,
    check: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<Option<HubProjectMetadata>> {
    check()?;
    let bytes = access::document(
        lease,
        store,
        prefix,
        &project_metadata_key(id),
        policy.content().max_input_bytes,
        check,
    )
    .await?;
    check()?;
    bytes
        .map(|bytes| HubProjectMetadata::decode(&bytes, id, policy.content().max_input_bytes))
        .transpose()
}
struct Selection<'a, 'request> {
    lease: &'a ServiceCredentialLease<'request>,
    subject: Option<&'a VerifiedUserSubject<'a>>,
    store: &'a Arc<dyn ObjectStore>,
    prefix: &'a str,
    input: &'a client::ProjectsArguments,
    policy: &'a HubResourcePolicy,
    grants: &'a access::Access,
    check: &'a (dyn Fn() -> io::Result<()> + Sync),
}
impl Selection<'_, '_> {
    async fn accept(&self, id: ProjectId, projects: &mut Vec<Project>) -> io::Result<bool> {
        let Self {
            lease,
            subject,
            store,
            prefix,
            input,
            policy,
            grants,
            check,
        } = self;
        check()?;
        current(lease, *subject, store, prefix, policy, grants, *check).await?;
        let Some(project) = metadata(lease, store, prefix, id, policy, *check).await? else {
            return Ok(false);
        };
        if !grants.allows(project.id, &project.name)
            || input
                .name_prefix
                .as_ref()
                .is_some_and(|prefix| !project.name.starts_with(prefix))
        {
            return Ok(false);
        }
        if projects.len() == input.limit {
            return Ok(true);
        }
        projects.push(Project {
            id: project.id,
            name: project.name,
        });
        Ok(false)
    }
}
