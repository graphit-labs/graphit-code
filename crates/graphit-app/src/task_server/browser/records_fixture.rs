//! Isolated browser fixtures: native publication and typed Registry metadata in one realm.
use super::*;
use graphit_auth::{ProjectWriteOperation, ScopeInput};
use graphit_core::{
    artifact_store::StoreOwner,
    git_checkout::GitCheckout,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::InstallationPaths,
};
use graphit_data::{
    knowledge_document_projection as projection, knowledge_index as index,
    knowledge_references as refs,
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
    policy::HubResourcePolicy,
    registry::{HubRegistry, Installation, InstalledArtifact, InstalledBackend, InstalledPayload},
};
use std::num::NonZeroUsize;

pub(crate) fn registry_bytes(server: &TaskServer) -> Vec<u8> {
    std::fs::read(
        server
            .0
            .policy
            .imported_knowledge
            .as_ref()
            .unwrap()
            .registry
            .path(),
    )
    .unwrap()
}

pub(crate) fn content_limits(server: &TaskServer) -> graphit_hub::native::ContentLimits {
    server.0.policy.content
}

pub(crate) fn stop(server: &TaskServer) {
    server.0.stop.cancel();
}

pub(crate) fn retire_checkout(server: &TaskServer, index: usize) {
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let (paths, id, _) = &snapshot.projects[index];
    server
        .0
        .live_projects
        .retire_checkout(*id, paths.root())
        .unwrap();
}

pub(crate) fn add_current_project(server: &TaskServer, root: &std::path::Path) -> Value {
    let installation = server.0.auth.installation().live;
    let installation = InstallationPaths::resolve(
        installation.brand().clone(),
        installation.owner_root(),
        None,
        Some(installation.owner_root()),
    )
    .unwrap();
    let paths = ProjectPaths::at(root, &installation).unwrap();
    let cancel = Cancellation::default();
    let id = ProjectIdentityStore::at(&paths)
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .ensure(&ProjectMetadata::new("added").unwrap())
        .unwrap()
        .id();
    let checkout = GitCheckout::inspect(root, Duration::from_secs(5), &cancel).unwrap();
    let version = VersionContext::select(&paths, checkout.as_checkout(), "", None, "").unwrap();
    let selected = json!({
        "project_dir": graphit_core::native_path::NativePath::new(root.to_owned()).unwrap(),
        "project_id": id,
        "project_version": version.version(),
    });
    let snapshot = server.0.live_projects.snapshot().unwrap();
    let mut projects = snapshot.projects.as_ref().clone();
    projects.push((paths, id, version));
    let current = projects.len() - 1;
    server
        .0
        .live_projects
        .replace(Arc::new(projects), current)
        .unwrap();
    selected
}

pub(crate) fn with_import_registry(server: &TaskServer) -> TaskServer {
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let installation = server.0.auth.installation().live;
    let StoreOwner::Installation { installation_id } = installation.owner() else {
        panic!("installation owner");
    };
    let paths = InstallationPaths::resolve(
        installation.brand().clone(),
        installation.owner_root(),
        None,
        Some(installation.owner_root()),
    )
    .unwrap();
    let policy = &server.0.policy;
    let configuration = graphit_core::config::ConfigSnapshot::new(
        paths.brand(),
        [HubResourcePolicy::distribution_defaults(paths.brand()).unwrap()],
    )
    .unwrap();
    let limits = HubResourcePolicy::resolve(&configuration).unwrap().hub();
    let registry = HubRegistry::global(&paths, installation_id, limits).unwrap();
    let mut policy = policy.clone();
    policy.imported_knowledge = Some(crate::task_server::ImportedKnowledgePolicy {
        registry,
        directory: policy.read.native,
    });
    TaskServer::new(
        Arc::clone(&server.0.auth),
        Arc::clone(&snapshot.projects),
        snapshot.current.expect("active fixture"),
        policy,
        CancellationToken::new(),
    )
    .unwrap()
}
pub(crate) async fn publish(
    server: &TaskServer,
    raw: &str,
    selected: usize,
    pages: &[projection::AuthoredPageFields<'_>],
    references: Option<&[refs::KnowledgeReferenceFields<'_>]>,
) -> index::KnowledgeFirstPublication {
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap();
    let snapshot = server.0.live_projects.snapshot().unwrap();
    let (paths, id, version) = &snapshot.projects[selected];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.policy.scope, original.snapshot())
            .unwrap();
    let binding = project.binding().unwrap();
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_write(
            &caller,
            &original,
            ScopeInput {
                installation: server.0.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &project,
                        record: &binding,
                    },
                    version,
                }),
                providers: &[],
                binding: server.0.policy.binding,
                global_version: "",
            },
            ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    let content = server.0.policy.content;
    let n = |v| NonZeroUsize::new(v).unwrap();
    let prepared = projection::prepare(
        original.snapshot(),
        pages,
        &[],
        projection::KnowledgeDocumentProjectionLimits {
            max_documents: n(content.max_nodes),
            max_observations: n(content.max_nodes),
            max_rows: n(content.max_nodes),
            max_field_bytes: n(content.max_input_bytes),
            max_text_bytes: n(content.max_input_bytes),
        },
    )
    .unwrap();
    let limits = index::KnowledgeIndexWriteLimits {
        content: crate::foreground_knowledge::limits(&server.0.policy)
            .unwrap()
            .content,
        native: server.0.policy.read.native,
        artifacts: server.0.policy.write.artifacts,
    };
    if let Some(fields) = references {
        let references = refs::prepare_knowledge_references(
            original.snapshot(),
            &prepared,
            fields,
            limits.content,
        )
        .unwrap();
        index::publish_native_authored_with_references(
            original.snapshot(),
            &admission,
            &prepared,
            &references,
            limits,
        )
        .await
        .unwrap()
    } else {
        index::publish_native_authored(original.snapshot(), &admission, &prepared, limits)
            .await
            .unwrap()
    }
}
pub(crate) async fn update_preserving(
    server: &TaskServer,
    raw: &str,
    selected: usize,
    pages: &[projection::AuthoredPageFields<'_>],
) -> index::KnowledgeGeneration {
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap();
    let snapshot = server.0.live_projects.snapshot().unwrap();
    let (paths, id, version) = &snapshot.projects[selected];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.policy.scope, original.snapshot())
            .unwrap();
    let binding = project.binding().unwrap();
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_write(
            &caller,
            &original,
            ScopeInput {
                installation: server.0.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &project,
                        record: &binding,
                    },
                    version,
                }),
                providers: &[],
                binding: server.0.policy.binding,
                global_version: "",
            },
            ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    let read = authority
        .admit_authenticated_read(
            &caller,
            &original,
            ScopeInput {
                installation: server.0.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &project,
                        record: &binding,
                    },
                    version,
                }),
                providers: &[],
                binding: server.0.policy.binding,
                global_version: "",
            },
            graphit_auth::ProjectReadModule::Knowledge,
        )
        .unwrap();
    let content = server.0.policy.content;
    let n = |v| NonZeroUsize::new(v).unwrap();
    let prepared = projection::prepare(
        original.snapshot(),
        pages,
        &[],
        projection::KnowledgeDocumentProjectionLimits {
            max_documents: n(content.max_nodes),
            max_observations: n(content.max_nodes),
            max_rows: n(content.max_nodes),
            max_field_bytes: n(content.max_input_bytes),
            max_text_bytes: n(content.max_input_bytes),
        },
    )
    .unwrap();
    let limits = index::KnowledgeIndexWriteLimits {
        content: crate::foreground_knowledge::limits(&server.0.policy)
            .unwrap()
            .content,
        native: server.0.policy.read.native,
        artifacts: server.0.policy.write.artifacts,
    };
    use graphit_data::knowledge_document_update::{
        KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
        KnowledgeDocumentUpdateLimits, KnowledgeDocumentUpdateResult,
        update_existing_native_knowledge_documents,
    };
    let read_limits = crate::foreground_knowledge::limits(&server.0.policy).unwrap();
    let index::KnowledgeIndexPageRead::Published {
        generation: expected,
        page: graphit_data::knowledge_snapshot::KnowledgeSnapshotPage::Found { document, .. },
    } = index::read_native_knowledge_page(
        original.snapshot(),
        &read,
        pages[0].slug.expect("explicit fixture slug"),
        read_limits,
    )
    .await
    .unwrap()
    else {
        panic!("existing publication");
    };
    assert_eq!(document.slug, pages[0].slug.expect("explicit fixture slug"));
    let updates = pages
        .iter()
        .map(|page| KnowledgeDocumentReferenceUpdate {
            slug: page.slug.expect("explicit fixture slug"),
            authored: KnowledgeDocumentAuthoredUpdate::Preserve,
        })
        .collect::<Vec<_>>();
    let result = update_existing_native_knowledge_documents(
        original.snapshot(),
        &read,
        &admission,
        &expected,
        &prepared,
        &updates,
        KnowledgeDocumentUpdateLimits {
            read: read_limits,
            write: limits,
        },
    )
    .await
    .unwrap();
    match result {
        KnowledgeDocumentUpdateResult::Updated { generation, .. } => {
            assert!(generation.version > expected.version);
            generation
        }
        KnowledgeDocumentUpdateResult::Unchanged { .. } => panic!("changed title must republish"),
    }
}

pub(crate) async fn register_copy(
    server: &TaskServer,
    published: &index::KnowledgeFirstPublication,
    id: &str,
    version: &str,
    selected: usize,
) -> crate::reference_explorer::SourceKey {
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap();
    let installation = server.0.auth.installation().live;
    let paths = InstallationPaths::resolve(
        installation.brand().clone(),
        installation.owner_root(),
        None,
        Some(installation.owner_root()),
    )
    .unwrap();
    let policy = server.0.policy.imported_knowledge.as_ref().unwrap();
    let projection = policy.registry.clone();
    let id = ArtifactId::new(id, server.0.policy.role_reference).unwrap();
    let version = ConcreteVersion::new(version, server.0.policy.role_reference).unwrap();
    let snapshot = server.0.live_projects.snapshot().unwrap();
    let source = SourceRef::hub_cache(
        &paths,
        snapshot.projects[selected].1,
        ArtifactType::Knowledge,
        id.clone(),
        None,
        version.clone(),
        server.0.policy.role_reference,
    )
    .unwrap();
    let mut builder =
        lance::dataset::builder::DatasetBuilder::from_uri(&published.default.corpus_uri);
    builder = match published.default.physical_branch.as_deref() {
        Some(branch) => builder.with_branch(branch, Some(published.default.version)),
        None => builder.with_version(published.default.version),
    };
    // Keep the large native SDK futures off the enclosing Live MCP test future.
    let mut corpus = Box::pin(builder.load()).await.unwrap();
    let target = source.directory().as_path().join("index.lance");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    Box::pin(corpus.deep_clone(target.to_str().unwrap(), published.default.version, None))
        .await
        .unwrap();
    let mut writer = projection
        .writer(
            original.remaining().unwrap().unwrap(),
            original.cancellation(),
        )
        .unwrap();
    writer
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: ArtifactType::Knowledge,
            id: id.clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source: source,
                alias: None,
                entry_name: None,
                display: None,
                backend: InstalledBackend::Native,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })
        .unwrap();
    crate::reference_explorer::SourceKey::ImportedKnowledge {
        artifact: crate::reference_explorer::ArtifactSelector {
            id: id.as_str().into(),
            version: version.as_str().into(),
            context_alias: None,
        },
    }
}
pub(crate) fn copy_tree(source: &std::path::Path, target: &std::path::Path) {
    std::fs::create_dir(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target)
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
pub(crate) fn page<'a>(
    slug: &'a str,
    title: &'a str,
    targets: &'a [&'a str],
) -> projection::AuthoredPageFields<'a> {
    projection::AuthoredPageFields {
        slug: Some(slug),
        title: Some(title),
        body: Some("# Complete native body é\n"),
        kind: Some("page"),
        summary: Some("complete native summary"),
        description: Some("complete native description"),
        summary_truncated: false,
        source_path: "docs/records.md",
        physical_digest: None,
        compilation_digest: projection::CompilationDigest32::from_bytes([7; 32]),
        recipe: projection::RecipeFields {
            id: projection::AUTHORED_RECIPE_ID,
            parameters: Some(projection::AuthoredParameters {
                fallback_title: "title",
                kind_on_absent: "page",
                max_summary_bytes: NonZeroUsize::new(32).unwrap(),
            }),
        },
        updated_at: Some(std::time::UNIX_EPOCH - Duration::from_nanos(1)),
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: targets,
    }
}
