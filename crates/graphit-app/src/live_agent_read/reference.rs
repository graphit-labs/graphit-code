//! Resolve one clicked Knowledge target inside persisted, freshly admitted contexts.
use super::*;
use graphit_data::knowledge_reference_query::{
    KnowledgeReferenceOwnedNamespace, KnowledgeReferenceOwnedScope, KnowledgeReferenceSource,
};

#[derive(Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReferenceLookup {
    pub(crate) namespace: KnowledgeReferenceOwnedNamespace,
    pub(crate) witness: Option<[u8; 32]>,
}
impl ReferenceLookup {
    fn matches(&self, source: &KnowledgeReferenceSource, slug: &str) -> bool {
        source.slug == slug
            && source.namespace.as_ref() == Some(&self.namespace)
            && source.witness.is_some()
            && self
                .witness
                .is_none_or(|witness| source.witness == Some(witness))
    }
}
fn omitted(error: &Error) -> bool {
    match error {
        Error::ContextChanged => true,
        Error::Read(reference_explorer::Failure::Physical(error))
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
            ) =>
        {
            true
        }
        Error::Read(reference_explorer::Failure::Invalid("inactive_project_selection")) => true,
        Error::Read(error) => matches!(error.response().0.as_u16(), 403 | 404),
        _ => false,
    }
}
fn remote_omitted(error: &crate::remote_native_mount::ErrorKind) -> bool {
    error.status() == axum::http::StatusCode::FORBIDDEN
        || matches!(
            error,
            crate::remote_native_mount::ErrorKind::SelectionChanged
                | crate::remote_native_mount::ErrorKind::InvalidBinding
        )
}
fn focused(refs: &contexts::EvidenceReferences) -> Option<&KnowledgeReferenceSource> {
    match refs {
        contexts::EvidenceReferences::Native(read) => read.focused_source.as_ref(),
        contexts::EvidenceReferences::Imported(read) => read.focused_source.as_ref(),
    }
}

pub(super) async fn resolve(
    context: &ReadContext<'_>,
    selections: Vec<&ArtifactSelection>,
    slug: &str,
    generation: &str,
    lookup: &ReferenceLookup,
) -> Result<Vec<u8>, Error> {
    let r = context.resources;
    if let KnowledgeReferenceOwnedScope::User(subject) = &lookup.namespace.scope {
        reference_explorer::text(subject, r.policy.content).map_err(invalid)?;
    }
    if let Some(alias) = &lookup.namespace.context {
        reference_explorer::text(alias, r.policy.content).map_err(invalid)?;
    }
    context.check()?;
    let (caller, verifier, provider) = context.identity;
    let hub = if selections.iter().any(|selection| selection.source == "hub") {
        match r.auth.issuer().admit_web_installed_hub_read(
            caller,
            verifier,
            context.original,
            provider,
            r.auth.installation(),
        ) {
            Ok(read) => Some(read),
            Err(error) => {
                let error = Error::from(error);
                context.check()?;
                if omitted(&error) {
                    None
                } else {
                    return Err(error);
                }
            }
        }
    } else {
        None
    };
    let mut selectors = Vec::new();
    for selection in selections {
        context.check()?;
        context.require_selection(selection)?;
        if selection.source == "project" {
            if selection.project_id.as_deref() != Some(selection.id.as_str())
                || selection.instance_id.is_some()
                || selection.project_kind.is_some()
                || selection.artifact_type != "knowledge"
            {
                return Err(invalid("selected_context_changed"));
            }
            let mut matching = r
                .projects
                .iter()
                .enumerate()
                .filter(|(_, (_, id, version))| {
                    id.to_string() == selection.id && version.version() == selection.version
                });
            let Some((index, project)) = matching.next() else {
                continue;
            };
            if matching.next().is_some() {
                return Err(invalid("selection_ambiguous"));
            }
            if let Err(error) = r.require_active(project) {
                let error = Error::from(error);
                if omitted(&error) {
                    continue;
                } else {
                    return Err(error);
                }
            }
            selectors.push(contexts::ContextSelection {
                selection,
                source: SourceKey::ProjectKnowledge,
                imported: None,
                project_index: Some(index),
            });
        } else if let Some(hub) = &hub {
            match contexts::select_contexts(r, hub, std::iter::once(selection)) {
                Ok(mut selected) => selectors.append(&mut selected),
                Err(error) => {
                    context.check()?;
                    let error = Error::from(error);
                    if !omitted(&error) {
                        return Err(error);
                    }
                }
            }
        }
    }
    // Retain independent scopes before admissions and readers that borrow them.
    let mut scopes = Vec::new();
    for (index, selected) in selectors.iter().enumerate() {
        if let Some(project) = selected.project_index {
            let (paths, id, _) = &r.projects[project];
            let scope = CapturedPublicationScope::project(
                paths,
                *id,
                r.policy.scope,
                context.original.snapshot(),
            )?;
            let binding = scope.binding()?;
            scopes.push((index, scope, binding));
        }
    }
    let authority = ProjectRecordAuthority::new(r.auth.issuer());
    let mut admissions = Vec::new();
    for (index, scope, binding) in &scopes {
        let project = selectors[*index]
            .project_index
            .ok_or_else(|| invalid("selected_context_changed"))?;
        let result = authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(caller),
                verifier,
                backend: context.backend,
                original: context.original,
                scope: BrokerProjectScope {
                    installation: r.auth.installation(),
                    project: ProjectCapture {
                        scope: ScopeCapture {
                            live: scope,
                            record: binding,
                        },
                        version: &r.projects[project].2,
                    },
                    providers: &[],
                },
                provider,
                module: ProjectReadModule::Knowledge,
            })
            .await;
        match result {
            Ok(read) => admissions.push((*index, read)),
            Err(error) => {
                context.check()?;
                let error = Error::from(reference_explorer::Failure::from(error));
                if !omitted(&error) {
                    return Err(error);
                }
            }
        }
    }
    let imported = if selectors.iter().any(|selected| selected.imported.is_some()) {
        match r.auth.issuer().admit_web_imported_knowledge_read(
            caller,
            verifier,
            context.original,
            provider,
            r.auth.installation(),
        ) {
            Ok(read) => Some(read),
            Err(error) => {
                context.check()?;
                let error = Error::from(error);
                if omitted(&error) {
                    None
                } else {
                    return Err(error);
                }
            }
        }
    } else {
        None
    };
    let registry = r
        .policy
        .imported_knowledge
        .as_ref()
        .map(|policy| &policy.registry);
    let mut remotes = Vec::new();
    let mut remote_indices = std::collections::BTreeSet::new();
    if let (Some(read), Some(registry)) = (&imported, registry) {
        for (index, selected) in selectors.iter().enumerate() {
            let Some((id, version)) = &selected.imported else {
                continue;
            };
            match crate::remote_native_mount::Selection::capture(
                crate::remote_native_mount::ReadAdmission::Knowledge(read),
                registry,
                id,
                version,
            ) {
                Ok(Some(remote)) => {
                    remote_indices.insert(index);
                    remotes.push((index, remote))
                }
                Ok(None) => {}
                Err(error) => {
                    context.check()?;
                    if !remote_omitted(&error) {
                        return Err(io::Error::other(error).into());
                    }
                    remote_indices.insert(index);
                }
            }
        }
    }
    let mut services = Vec::new();
    for (remote_index, (_, remote)) in remotes.iter().enumerate() {
        match r.auth.issuer().capture_imported_knowledge_hub_service(
            imported
                .as_ref()
                .ok_or_else(|| invalid("selected_context_unavailable"))?,
            remote.record().provider_name(),
        ) {
            Ok(mut service) => match service.mode() {
                Ok(mode) => services.push((remote_index, service, mode)),
                Err(error) => {
                    context.check()?;
                    let error = crate::remote_native_mount::ErrorKind::Service(error);
                    if !remote_omitted(&error) {
                        return Err(io::Error::other(error).into());
                    }
                }
            },
            Err(error) => {
                context.check()?;
                let error = crate::remote_native_mount::ErrorKind::Service(error);
                if !remote_omitted(&error) {
                    return Err(io::Error::other(error).into());
                }
            }
        }
    }
    let mut verifiers = Vec::new();
    for (_, service, mode) in &mut services {
        verifiers.push(if *mode == graphit_auth::ServiceMode::BrokerNative {
            Some(
                r.native_verifiers
                    .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
                    .get(service)
                    .map_err(io::Error::other)?,
            )
        } else {
            None
        });
    }
    let service_indices = services
        .iter()
        .map(|(index, _, _)| *index)
        .collect::<Vec<_>>();
    let mut leases = Vec::new();
    for ((_, service, _), verifier) in services.iter_mut().zip(&verifiers) {
        let result = match verifier.as_deref() {
            Some(verifier) => service.native_credential(verifier).await,
            None => service.credential(),
        };
        leases.push(match result {
            Ok(lease) => Some(lease),
            Err(error) => {
                context.check()?;
                let error = crate::remote_native_mount::ErrorKind::Service(error);
                if remote_omitted(&error) {
                    None
                } else {
                    return Err(io::Error::other(error).into());
                }
            }
        });
    }
    let mut opened = Vec::new();
    for (remote_index, (index, selection)) in remotes.into_iter().enumerate() {
        let Some(service_index) = service_indices
            .iter()
            .position(|actual| *actual == remote_index)
        else {
            continue;
        };
        let Some(lease) = &leases[service_index] else {
            continue;
        };
        let registry = registry.ok_or_else(|| invalid("selected_context_unavailable"))?;
        match crate::source_server::remote_native::open(
            selection,
            lease,
            r.native_http,
            r.native_allow_loopback_http,
            crate::native_publication::metadata_limits(
                r.policy.content,
                registry.limits().projection(),
            ),
        )
        .await
        {
            Ok(reader) => opened.push((index, service_index, reader)),
            Err(error) => {
                context.check()?;
                if !remote_omitted(&error) {
                    return Err(io::Error::other(error).into());
                }
            }
        }
    }
    let mut pins = Vec::new();
    for (_, service_index, opened) in &opened {
        let lease = leases[*service_index]
            .as_ref()
            .ok_or_else(|| invalid("selected_context_unavailable"))?;
        pins.push(match opened.mount.wiki(&opened.selection, lease).await {
            Ok(pin) => Some(pin),
            Err(error) => {
                context.check()?;
                if remote_omitted(&error) {
                    None
                } else {
                    return Err(io::Error::other(error).into());
                }
            }
        });
    }
    let mut readers = Vec::new();
    for (index, selected) in selectors.iter().enumerate() {
        context.check()?;
        let result = if selected.project_index.is_some() {
            let Some((_, read)) = admissions.iter().find(|(actual, _)| *actual == index) else {
                continue;
            };
            contexts::capture_project_optional(
                selected,
                context.original.snapshot(),
                read,
                r.policy,
            )
            .await
        } else if let Some(remote) = opened.iter().position(|(actual, _, _)| *actual == index) {
            let Some(pin) = &pins[remote] else { continue };
            let lease = leases[opened[remote].1]
                .as_ref()
                .ok_or_else(|| invalid("selected_context_unavailable"))?;
            contexts::capture_remote_imported(selected, &opened[remote].2, pin, lease)
                .await
                .map(Some)
        } else if remote_indices.contains(&index) {
            continue;
        } else if let Some(read) = &imported {
            contexts::capture_local_imported(selected, r, read)
                .await
                .map(Some)
        } else {
            continue;
        };
        let reader = match result {
            Ok(reader) => reader,
            Err(error) => {
                context.check()?;
                let error = Error::from(error);
                if omitted(&error) {
                    continue;
                } else {
                    return Err(error);
                }
            }
        };
        if let Some(reader) = reader {
            readers.push((index, reader));
        }
    }
    let limits = foreground_knowledge::limits(r.policy)
        .map_err(foreground_knowledge::Failure::from)
        .map_err(Error::Knowledge)?;
    let mut candidates = Vec::new();
    let mut only = None;
    for (index, reader) in &readers {
        context.check()?;
        let refs = match reader
            .references(
                slug,
                limits,
                NonZeroUsize::new(20).expect("positive page size"),
            )
            .await
        {
            Ok(refs) => refs,
            Err(error) => {
                context.check()?;
                let error = Error::from(error);
                if omitted(&error) {
                    continue;
                } else {
                    return Err(error);
                }
            }
        };
        let Some(focus) = focused(&refs).filter(|focus| lookup.matches(focus, slug)) else {
            continue;
        };
        let selection = selectors[*index].selection;
        candidates.push(Candidate {
            title: focus.title.clone(),
            path: focus.slug.clone(),
            source_path: focus.source_path.clone(),
            context: selection.clone(),
        });
        if candidates.len() == 1 {
            only = Some((*index, refs));
        } else {
            only = None;
        }
    }
    context.check_current()?;
    if candidates.is_empty() {
        return Err(io::Error::from(io::ErrorKind::NotFound).into());
    }
    let maximum = r.policy.content.max_output_bytes;
    if candidates.len() > 1 {
        return crate::task_server::browser::codec::encode_response(
            &json!({"generation":generation,"candidates":candidates}),
            maximum,
        )
        .map_err(Error::from);
    }
    let (index, refs) = only.ok_or_else(|| invalid("invalid_live_read"))?;
    let reader = &readers
        .iter()
        .find(|(actual, _)| *actual == index)
        .ok_or_else(|| invalid("invalid_live_read"))?
        .1;
    let document = reader.document(slug, limits).await;
    context.check()?;
    let document = document?.ok_or(Error::ContextChanged)?;
    reader.recheck()?;
    if let Some(project) = selectors[index].project_index {
        r.require_active(&r.projects[project])?;
    }
    context.check_current()?;
    crate::task_server::browser::codec::encode_response(
        &LiveEvidencePage {
            title: document.title,
            path: document.slug,
            source_path: document.source_path,
            content: document.body,
            context: selectors[index].selection.clone(),
            generation: generation.to_owned(),
            refs,
        },
        maximum,
    )
    .map_err(Error::from)
}
