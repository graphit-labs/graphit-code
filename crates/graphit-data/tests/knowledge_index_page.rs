//! Selected LOCAL reads with actual authenticated PROJECT admissions and SDK corpora.
use crate::knowledge_manifest_fixture as manifest;

use futures::TryStreamExt;
use graphit_auth::*;
use graphit_auth::{profile::ProfileLimits, store::AuthStore};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeBindingRecord, ScopeLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_data::knowledge_snapshot::KnowledgeSnapshotPage;
use graphit_data::{knowledge_document_projection as h, knowledge_index::*};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, UNIX_EPOCH},
};

fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    request_deadline(cancel, Instant::now() + Duration::from_secs(180))
}
fn request_deadline(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 65536,
            max_scalar_bytes: 4096,
            max_collection_entries: 128,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
fn issuer_limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 64,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 1024,
    }
}
fn permissions() -> Vec<Permission> {
    vec![
        Permission::Read(Module::Knowledge),
        Permission::MutateModule(Module::Knowledge),
    ]
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    runtime: Arc<RuntimeKeyOwner>,
    installation: CapturedPublicationScope,
    installation_record: ScopeBindingRecord,
    project: CapturedPublicationScope,
    project_record: ScopeBindingRecord,
    version: VersionContext,
    paths: ProjectPaths,
}
impl Fixture {
    fn new(ctx: &RequestContext<'_>) -> Self {
        Self::with_version(ctx, "")
    }
    fn with_version(ctx: &RequestContext<'_>, version_name: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let home = dir.path().join("home");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&home).unwrap();
        let install = InstallationPaths::resolve(
            Brand::new("knowledge-test").unwrap(),
            &root,
            Some(&home),
            None,
        )
        .unwrap();
        let private = installation_bootstrap::prepare(
            &install,
            BootstrapLimits::new(65536, 256).unwrap(),
            ctx.snapshot(),
        )
        .into_installation()
        .unwrap();
        let runtime = Arc::new(
            RuntimeKeyOwner::start(
                private,
                RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
                ctx.snapshot(),
            )
            .into_owner()
            .ok()
            .unwrap(),
        );
        let paths = ProjectPaths::at(&root, &install).unwrap();
        let limits = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &install,
            InstallationId::new("local-installation").unwrap(),
            [root.clone()],
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let project = CapturedPublicationScope::project(
            &paths,
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let version =
            VersionContext::select(&paths, Checkout::NonGit, version_name, None, "").unwrap();
        let installation_record = installation.binding().unwrap();
        let project_record = project.binding().unwrap();
        Self {
            _dir: dir,
            root,
            runtime,
            installation,
            installation_record,
            project,
            project_record,
            version,
            paths,
        }
    }
    fn realm(&self) -> ScopeCapture<'_> {
        ScopeCapture {
            live: &self.installation,
            record: &self.installation_record,
        }
    }
    fn scope(&self) -> ScopeInput<'_> {
        self.scope_for(&self.version)
    }
    fn scope_for<'a>(&'a self, version: &'a VersionContext) -> ScopeInput<'a> {
        ScopeInput {
            installation: self.realm(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.project_record,
                },
                version,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        }
    }
    fn issuer(&self, ctx: &RequestContext<'_>) -> RoleIssuer {
        self.issuer_permissions(ctx, &permissions())
    }
    fn issuer_permissions(&self, ctx: &RequestContext<'_>, perms: &[Permission]) -> RoleIssuer {
        let paths = InstallationPaths::resolve(
            self.installation.brand().clone(),
            &self.root,
            None,
            Some(self.installation.owner_root()),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("local-installation").unwrap(),
            [self.installation.owner_root().to_owned()],
            ScopeLimits::new(16, 4096, 128).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let physical = AuthDocumentStore::capture(
            scope,
            AuthDocumentLimits::new(4096, 65536, 256, 65536).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let local = Arc::new(
            AuthStore::new(
                physical,
                ProfileLimits {
                    bytes: 4096,
                    nodes: 4096,
                    scalar_bytes: 256,
                    collection_entries: 64,
                    providers: 16,
                    profiles: 16,
                },
                ctx,
            )
            .unwrap(),
        );
        let source = SourceAuthorityPolicy {
            ordinary: perms,
            administrative: perms,
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(3600),
                maximum: Duration::from_secs(3600),
            },
        };
        let policy = IssuerAuthorityPolicy::new(
            source,
            source,
            issuer_limits(),
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 65536,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap();
        RoleIssuer::new(
            self.realm(),
            self.runtime.clone(),
            local,
            policy,
            TtlPolicy {
                ttl: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
                refresh_margin: Duration::from_secs(120),
            },
            ctx,
        )
        .unwrap()
    }
    fn secret(&self) -> String {
        std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap()
    }
}

fn content() -> KnowledgeIndexContentLimits {
    KnowledgeIndexContentLimits {
        max_identifier_bytes: n(256),
    }
}
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn read_limits() -> KnowledgeIndexReadLimits {
    KnowledgeIndexReadLimits {
        content: content(),
        native: native(),
        batch_rows: NonZeroU32::new(1).unwrap(),
    }
}
fn write_limits() -> KnowledgeIndexWriteLimits {
    let copy = CopyLimits::new(1024 * 1024, 1024, 65536, 16).unwrap();
    KnowledgeIndexWriteLimits {
        content: content(),
        native: native(),
        artifacts: ArtifactLimits::new(65536, 4096, copy).unwrap(),
    }
}
fn hlimits() -> h::KnowledgeDocumentProjectionLimits {
    h::KnowledgeDocumentProjectionLimits {
        max_documents: n(8),
        max_observations: n(8),
        max_rows: n(64),
        max_field_bytes: n(65536),
        max_text_bytes: n(131072),
    }
}
fn page<'a>(slug: &'a str, targets: &'a [&'a str]) -> h::AuthoredPageFields<'a> {
    h::AuthoredPageFields {
        slug: Some(slug),
        title: Some(""),
        body: Some("---\ntitle: body literal\n---\ntext\n"),
        kind: Some("page"),
        summary: Some("text"),
        description: None,
        summary_truncated: false,
        source_path: "wiki/document.md",
        physical_digest: None,
        compilation_digest: h::CompilationDigest32::from_bytes([0xff; 32]),
        recipe: h::RecipeFields {
            id: h::AUTHORED_RECIPE_ID,
            parameters: Some(h::AuthoredParameters {
                fallback_title: "fallback",
                kind_on_absent: "page",
                max_summary_bytes: n(16),
            }),
        },
        updated_at: Some(UNIX_EPOCH - Duration::from_nanos(17)),
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: targets,
    }
}

fn native_test(future: impl std::future::Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(future)
        })
        .unwrap()
        .join()
        .unwrap();
}
async fn seed(
    ctx: &RequestContext<'_>,
    write: &ProjectWriteAdmission<'_>,
    pages: &[h::AuthoredPageFields<'_>],
) -> KnowledgeFirstPublication {
    let prepared = h::prepare(ctx.snapshot(), pages, &[], hlimits()).unwrap();
    publish_native_authored(ctx.snapshot(), write, &prepared, write_limits())
        .await
        .unwrap()
}
fn image(root: &std::path::Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn walk(
        root: &std::path::Path,
        at: &std::path::Path,
        out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) {
        if !at.exists() {
            return;
        }
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.insert(path.strip_prefix(root).unwrap().to_owned(), vec![]);
                walk(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}
#[test]
fn absence_and_empty_are_authorized_without_creation() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        let before = image(f.project.runtime_root());
        assert_eq!(
            read_native_knowledge_page(ctx.snapshot(), &read, "picked", read_limits())
                .await
                .unwrap(),
            KnowledgeIndexPageRead::NoCorpus
        );
        assert_eq!(image(f.project.runtime_root()), before);
        let write = authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
            .unwrap();
        let published = seed(&ctx, &write, &[]).await;
        let before = image(f.project.runtime_root());
        assert_eq!(
            read_native_knowledge_page(ctx.snapshot(), &read, "picked", read_limits())
                .await
                .unwrap(),
            KnowledgeIndexPageRead::Published {
                generation: published.default,
                page: KnowledgeSnapshotPage::PublishedEmpty
            }
        );
        assert_eq!(image(f.project.runtime_root()), before);
    });
}
#[test]
fn selected_body_has_per_document_limits_and_owned_full16() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        let write = authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
            .unwrap();
        let body = "λ".repeat(1000);
        let mut a = page("picked", &["picked", "ghost", "ghost"]);
        a.body = Some(&body);
        let mut b = page("other", &[]);
        b.body = Some(&body);
        let mut c = page("last", &[]);
        c.body = Some(&body);
        let publication = seed(&ctx, &write, &[a, b, c]).await;
        let limits = read_limits();
        let before = image(f.project.runtime_root());
        let KnowledgeIndexPageRead::Published {
            generation,
            page: KnowledgeSnapshotPage::Found { document, .. },
        } = read_native_knowledge_page(ctx.snapshot(), &read, "picked", limits)
            .await
            .unwrap()
        else {
            panic!("found");
        };
        assert_eq!(generation, publication.default);
        assert_eq!(document.body, body);
        assert_eq!(document.title, "");
        assert_eq!(document.description, None);
        assert_eq!(
            document.updated_at,
            Some(UNIX_EPOCH - Duration::from_nanos(17))
        );
        assert_eq!(document.resolved_wiki_targets, ["picked", "ghost", "ghost"]);
        let KnowledgeIndexRead::Published {
            documents: full, ..
        } = read_native_knowledge_index(ctx.snapshot(), &read, limits)
            .await
            .unwrap()
        else {
            panic!("full corpus");
        };
        assert_eq!(full.len(), 3);
        assert_eq!(
            full.iter().find(|page| page.slug == "picked").unwrap().body,
            body
        );
        assert!(matches!(
            read_native_knowledge_page(ctx.snapshot(), &read, "missing", limits)
                .await
                .unwrap(),
            KnowledgeIndexPageRead::Published {
                page: KnowledgeSnapshotPage::PageAbsent,
                ..
            }
        ));
        assert_eq!(image(f.project.runtime_root()), before);
        drop(read);
        drop(write);
        drop(caller);
        drop(issuer);
        drop(f);
        drop(ctx);
        assert_eq!(document.body, body);
    });
}
#[test]
fn named_and_inherited_generations_use_actual_admitted_branch() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_version(&ctx, "A");
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let write = authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
            .unwrap();
        let published = seed(&ctx, &write, &[page("picked", &[])]).await;
        let b = VersionContext::select(&f.paths, Checkout::NonGit, "B", None, "").unwrap();
        let before = image(f.project.runtime_root());
        for version in [&f.version, &b] {
            let read = authority
                .admit_authenticated_read(
                    &caller,
                    &ctx,
                    f.scope_for(version),
                    ProjectReadModule::Knowledge,
                )
                .unwrap();
            let KnowledgeIndexPageRead::Published {
                generation,
                page: KnowledgeSnapshotPage::Found { .. },
            } = read_native_knowledge_page(ctx.snapshot(), &read, "picked", read_limits())
                .await
                .unwrap()
            else {
                panic!("found")
            };
            if version.version() == "A" {
                assert_eq!(generation, published.named.clone().unwrap());
            } else {
                assert_eq!(generation.version, published.default.version);
                assert_eq!(generation.physical_branch, None);
                assert_eq!(
                    generation.inherited_target,
                    Some(b.lance_branch().as_str().to_owned())
                );
            }
        }
        assert_eq!(image(f.project.runtime_root()), before);
    });
}
async fn replace_rows(uri: &str, mutate: impl FnOnce(&mut Vec<arrow_array::RecordBatch>)) {
    let store = graphit_data::lance_module_store::LanceModuleStore::open(uri, physical_schema())
        .await
        .unwrap();
    let pin = store.snapshot();
    let mut scanner = pin.scan();
    scanner.batch_size(1);
    let mut stream = scanner.try_into_stream().await.unwrap();
    let mut batches = Vec::new();
    while let Some(batch) = stream.try_next().await.unwrap() {
        batches.push(batch);
    }
    drop(stream);
    drop(scanner);
    mutate(&mut batches);
    pin.prepare_replace(batches)
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
}
#[test]
fn selected_integrity_ignores_unrelated_rows_but_rejects_selected_duplicates() {
    native_test(async {
        for duplicate in [false, true] {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx);
            let issuer = f.issuer(&ctx);
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(&issuer);
            let read = authority
                .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
                .unwrap();
            let write = authority
                .admit_authenticated_write(
                    &caller,
                    &ctx,
                    f.scope(),
                    ProjectWriteOperation::Knowledge,
                )
                .unwrap();
            let published = seed(&ctx, &write, &[page("picked", &[]), page("late", &[])]).await;
            replace_rows(&published.default.corpus_uri, |batches| {
                let batch = &batches[1];
                let mut columns = batch.columns().to_vec();
                columns[if duplicate { 0 } else { 10 }] =
                    Arc::new(arrow_array::StringArray::from(vec![if duplicate {
                        "picked"
                    } else {
                        "invalid-recipe"
                    }]));
                batches[1] = arrow_array::RecordBatch::try_new(batch.schema(), columns).unwrap();
            })
            .await;
            let before = image(f.project.runtime_root());
            let selected =
                read_native_knowledge_page(ctx.snapshot(), &read, "picked", read_limits()).await;
            if duplicate {
                assert!(matches!(selected, Err(KnowledgeIndexError::DuplicateSlug)));
            } else {
                assert!(matches!(
                    selected.unwrap(),
                    KnowledgeIndexPageRead::Published {
                        page: KnowledgeSnapshotPage::Found { .. },
                        ..
                    }
                ));
                assert!(matches!(
                    read_native_knowledge_page(ctx.snapshot(), &read, "late", read_limits()).await,
                    Err(KnowledgeIndexError::InvalidField("recipe"))
                ));
            }
            assert!(matches!(
                read_native_knowledge_page(ctx.snapshot(), &read, "missing", read_limits())
                    .await
                    .unwrap(),
                KnowledgeIndexPageRead::Published {
                    page: KnowledgeSnapshotPage::PageAbsent,
                    ..
                }
            ));
            assert_eq!(image(f.project.runtime_root()), before);
        }
    });
}
#[test]
fn actual_factory_foreign_original_wrong_class_and_slug_fail_before_io() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        let before = image(f.project.runtime_root());
        let foreign = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(10),
            Cancellation::default(),
        ));
        assert!(matches!(
            read_native_knowledge_page(&foreign, &read, "picked", read_limits()).await,
            Err(KnowledgeIndexError::DifferentRequest)
        ));
        for slug in ["", "a/b", "a\\b", "..a", "a\n"] {
            assert!(matches!(
                read_native_knowledge_page(ctx.snapshot(), &read, slug, read_limits()).await,
                Err(KnowledgeIndexError::InvalidField("slug"))
            ));
        }
        let mut limits = read_limits();
        limits.content.max_identifier_bytes = n(1);
        assert!(matches!(
            read_native_knowledge_page(ctx.snapshot(), &read, "λλ", limits).await,
            Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"))
        ));
        let denied = f.issuer_permissions(&ctx, &[Permission::MutateModule(Module::Knowledge)]);
        let denied_caller = denied.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let denied_authority = ProjectRecordAuthority::new(&denied);
        assert!(matches!(
            denied_authority.admit_authenticated_read(
                &denied_caller,
                &ctx,
                f.scope(),
                ProjectReadModule::Knowledge
            ),
            Err(AuthError::Denied)
        ));
        let memory = f.issuer_permissions(&ctx, &[Permission::Read(Module::Memory)]);
        let memory_caller = memory.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let memory_authority = ProjectRecordAuthority::new(&memory);
        let wrong = memory_authority
            .admit_authenticated_read(&memory_caller, &ctx, f.scope(), ProjectReadModule::Memory)
            .unwrap();
        assert!(matches!(
            read_native_knowledge_page(ctx.snapshot(), &wrong, "picked", read_limits()).await,
            Err(KnowledgeIndexError::WrongAdmission)
        ));
        assert_eq!(image(f.project.runtime_root()), before);
    });
}
#[test]
fn cancellation_in_flight_preflight_and_deadline_are_causal() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        let write = authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
            .unwrap();
        seed(&ctx, &write, &[page("picked", &[])]).await;
        let before = image(f.project.runtime_root());
        let (result, ()) = tokio::join!(
            read_native_knowledge_page(ctx.snapshot(), &read, "picked", read_limits()),
            async {
                tokio::task::yield_now().await;
                cancel.cancel();
            }
        );
        let error = result.unwrap_err();
        assert!(
            matches!(&error,KnowledgeIndexError::Request(e) if e.kind()==std::io::ErrorKind::Interrupted)
        );
        assert!(std::error::Error::source(&error).is_some());
        assert!(matches!(
            read_native_knowledge_page(ctx.snapshot(), &read, "picked", read_limits()).await,
            Err(KnowledgeIndexError::Request(_))
        ));
        let deadline_cancel = Cancellation::default();
        let short = request_deadline(&deadline_cancel, Instant::now() + Duration::from_secs(1));
        let caller = issuer.authenticate_runtime(&f.secret(), &short).unwrap();
        let read = authority
            .admit_authenticated_read(&caller, &short, f.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        tokio::time::sleep_until(tokio::time::Instant::from_std(
            short.deadline().expect("finite fixture deadline"),
        ))
        .await;
        assert!(
            matches!(read_native_knowledge_page(short.snapshot(),&read,"picked",read_limits()).await,Err(KnowledgeIndexError::Request(e)) if e.kind()==std::io::ErrorKind::TimedOut)
        );
        assert_eq!(image(f.project.runtime_root()), before);
    });
}
#[test]
fn other_and_corrupt_native_corpora_are_errors_without_repair() {
    native_test(async {
        for file in [false, true] {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx);
            let issuer = f.issuer(&ctx);
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(&issuer);
            let read = authority
                .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
                .unwrap();
            let write = authority
                .admit_authenticated_write(
                    &caller,
                    &ctx,
                    f.scope(),
                    ProjectWriteOperation::Knowledge,
                )
                .unwrap();
            let published = seed(&ctx, &write, &[page("picked", &[])]).await;
            let path = std::path::Path::new(&published.default.corpus_uri);
            assert!(path.is_absolute());
            if file {
                let permissions = std::fs::metadata(path).unwrap().permissions();
                std::fs::remove_dir_all(path).unwrap();
                std::fs::write(path, b"other").unwrap();
                std::fs::set_permissions(path, permissions).unwrap();
            } else {
                manifest::corrupt_published_manifest(path);
            }
            let before = image(f.project.runtime_root());
            let error = read_native_knowledge_page(ctx.snapshot(), &read, "picked", read_limits())
                .await
                .unwrap_err();
            if file {
                assert!(matches!(error, KnowledgeIndexError::Request(_)));
            } else {
                assert!(matches!(
                    error,
                    KnowledgeIndexError::Branch(_) | KnowledgeIndexError::Store(_)
                ));
            }
            assert!(std::error::Error::source(&error).is_some());
            assert_eq!(image(f.project.runtime_root()), before);
        }
    });
}

mod reference_storage {
    use super::*;
    use arrow_array::Array;
    use graphit_data::knowledge_references::*;
    fn ns() -> KnowledgeReferenceNamespace<'static> {
        KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()),
            context: None,
        }
    }
    fn fields<'a>(
        slug: &'a str,
        state: KnowledgeDocumentState,
        authored: &'a [KnowledgeAuthoredReference<'a>],
    ) -> KnowledgeReferenceFields<'a> {
        KnowledgeReferenceFields {
            slug,
            state,
            namespace: ns(),
            authored,
        }
    }
    fn heads() -> KnowledgeHeadReadLimits {
        KnowledgeHeadReadLimits {
            max_slug_bytes: n(256),
            native: native(),
            batch_rows: NonZeroU32::new(1).unwrap(),
        }
    }
    #[test]
    fn native_complete_and_removed_pages_reopen_with_real_admissions() {
        native_test(async {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::with_version(&ctx, "named");
            let issuer = f.issuer(&ctx);
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(&issuer);
            let read = authority
                .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
                .unwrap();
            let write = authority
                .admit_authenticated_write(
                    &caller,
                    &ctx,
                    f.scope(),
                    ProjectWriteOperation::Knowledge,
                )
                .unwrap();
            let pages = [
                page("active", &["ghost", "active", "ghost"]),
                page("removed", &[]),
            ];
            let prepared = h::prepare(ctx.snapshot(), &pages, &[], hlimits()).unwrap();
            let refs = [KnowledgeAuthoredReference {
                target_kind: KnowledgeReferenceTargetKind::Memory,
                scope: KnowledgeReferenceScope::User(" foreign user "),
                id: " é ",
                context: None,
                relation: " supports ",
                field: Some(""),
            }; 2];
            let entries = [
                fields("active", KnowledgeDocumentState::Active, &refs),
                fields("removed", KnowledgeDocumentState::Removed, &[]),
            ];
            let projection =
                prepare_knowledge_references(ctx.snapshot(), &prepared, &entries, content())
                    .unwrap();
            let published = publish_native_authored_with_references(
                ctx.snapshot(),
                &write,
                &prepared,
                &projection,
                write_limits(),
            )
            .await
            .unwrap();
            let generation = published.named.clone().unwrap();
            let tree = image(f.project.runtime_root());
            let KnowledgeHeadRead::Published {
                generation: observed,
                heads: catalog,
            } = read_native_knowledge_heads(ctx.snapshot(), &read, heads())
                .await
                .unwrap()
            else {
                panic!()
            };
            assert_eq!(observed, generation);
            assert_eq!(catalog.len(), 1);
            assert_eq!(catalog[0].slug, "active");
            assert_eq!(
                read_native_knowledge_page(ctx.snapshot(), &read, "removed", read_limits())
                    .await
                    .unwrap(),
                KnowledgeIndexPageRead::Published {
                    generation: generation.clone(),
                    page: KnowledgeSnapshotPage::PageAbsent
                }
            );
            let KnowledgeIndexPageRead::Published {
                generation: observed,
                page:
                    KnowledgeSnapshotPage::Found {
                        document,
                        focused_source: Some(source),
                    },
            } = read_native_knowledge_page(ctx.snapshot(), &read, "active", read_limits())
                .await
                .unwrap()
            else {
                panic!()
            };
            assert_eq!(observed, generation);
            assert_eq!(document.body, pages[0].body.unwrap());
            assert_eq!(source.slug, document.slug);
            assert_eq!(source.title, document.title);
            assert_eq!(source.compilation_digest, document.compilation_digest);
            assert!(source.namespace.is_some());
            assert!(source.witness.is_some());
            assert_eq!(document.resolved_wiki_targets, ["ghost", "active", "ghost"]);
            let KnowledgeIndexRead::Published {
                generation: observed,
                documents,
            } = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
                .await
                .unwrap()
            else {
                panic!()
            };
            assert_eq!(observed, generation);
            assert_eq!(documents.len(), 1);
            assert_eq!(
                tree,
                image(f.project.runtime_root()),
                "reads never rewrite/import/reconcile"
            );
        });
    }
    #[test]
    fn selected_get_is_independent_while_full_reads_reject_removed_corruption() {
        native_test(async {
            for duplicate in [false, true] {
                let cancel = Cancellation::default();
                let ctx = request(&cancel);
                let f = Fixture::new(&ctx);
                let issuer = f.issuer(&ctx);
                let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
                let authority = ProjectRecordAuthority::new(&issuer);
                let read = authority
                    .admit_authenticated_read(
                        &caller,
                        &ctx,
                        f.scope(),
                        ProjectReadModule::Knowledge,
                    )
                    .unwrap();
                let write = authority
                    .admit_authenticated_write(
                        &caller,
                        &ctx,
                        f.scope(),
                        ProjectWriteOperation::Knowledge,
                    )
                    .unwrap();
                let pages = [page("picked", &[]), page("removed", &[])];
                let h = h::prepare(ctx.snapshot(), &pages, &[], hlimits()).unwrap();
                let entries = [
                    fields("picked", KnowledgeDocumentState::Active, &[]),
                    fields("removed", KnowledgeDocumentState::Removed, &[]),
                ];
                let projection =
                    prepare_knowledge_references(ctx.snapshot(), &h, &entries, content()).unwrap();
                let published = publish_native_authored_with_references(
                    ctx.snapshot(),
                    &write,
                    &h,
                    &projection,
                    write_limits(),
                )
                .await
                .unwrap();
                replace_rows(&published.default.corpus_uri, |batches| {
                    if duplicate {
                        // Two valid Removed rows with the same slug still violate physical identity.
                        batches.push(batches[1].clone());
                        return;
                    }
                    let batch = &batches[1];
                    let mut columns = batch.columns().to_vec();
                    let old = batch
                        .column(17)
                        .as_any()
                        .downcast_ref::<arrow_array::StructArray>()
                        .unwrap();
                    let mut fields = old.columns().to_vec();
                    let mut witness = arrow_array::builder::FixedSizeBinaryBuilder::new(32);
                    witness.append_value([0; 32]).unwrap();
                    fields[4] = Arc::new(witness.finish());
                    columns[17] = Arc::new(arrow_array::StructArray::new(
                        old.fields().clone(),
                        fields,
                        old.nulls().cloned(),
                    ));
                    batches[1] =
                        arrow_array::RecordBatch::try_new(batch.schema(), columns).unwrap();
                })
                .await;
                let before = image(f.project.runtime_root());
                let check = |error: KnowledgeIndexError| {
                    if duplicate {
                        assert!(matches!(error, KnowledgeIndexError::DuplicateSlug));
                    } else {
                        assert!(matches!(
                            error,
                            KnowledgeIndexError::InvalidField("reference_witness")
                        ));
                    }
                };
                assert!(matches!(
                    read_native_knowledge_page(ctx.snapshot(), &read, "picked", read_limits())
                        .await
                        .unwrap(),
                    KnowledgeIndexPageRead::Published {
                        page: KnowledgeSnapshotPage::Found { .. },
                        ..
                    }
                ));
                assert!(matches!(
                    read_native_knowledge_page(ctx.snapshot(), &read, "missing", read_limits())
                        .await
                        .unwrap(),
                    KnowledgeIndexPageRead::Published {
                        page: KnowledgeSnapshotPage::PageAbsent,
                        ..
                    }
                ));
                check(
                    read_native_knowledge_page(ctx.snapshot(), &read, "removed", read_limits())
                        .await
                        .unwrap_err(),
                );
                check(
                    read_native_knowledge_heads(ctx.snapshot(), &read, heads())
                        .await
                        .unwrap_err(),
                );
                check(
                    read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
                        .await
                        .unwrap_err(),
                );
                assert_eq!(image(f.project.runtime_root()), before);
            }
        });
    }
    #[test]
    fn all_removed_remains_published_and_wrong_source_namespace_creates_nothing() {
        native_test(async {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx);
            let issuer = f.issuer(&ctx);
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(&issuer);
            let read = authority
                .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
                .unwrap();
            let write = authority
                .admit_authenticated_write(
                    &caller,
                    &ctx,
                    f.scope(),
                    ProjectWriteOperation::Knowledge,
                )
                .unwrap();
            let pages = [page("removed", &[])];
            let h = h::prepare(ctx.snapshot(), &pages, &[], hlimits()).unwrap();
            let mut wrong = fields("removed", KnowledgeDocumentState::Removed, &[]);
            wrong.namespace.scope =
                KnowledgeReferenceScope::Project("01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap());
            let wrong = [wrong];
            let projection =
                prepare_knowledge_references(ctx.snapshot(), &h, &wrong, content()).unwrap();
            let tree = image(f.project.runtime_root());
            assert!(matches!(
                publish_native_authored_with_references(
                    ctx.snapshot(),
                    &write,
                    &h,
                    &projection,
                    write_limits()
                )
                .await,
                Err(KnowledgeIndexError::InvalidField("source_namespace"))
            ));
            assert_eq!(tree, image(f.project.runtime_root()));
            let entries = [fields("removed", KnowledgeDocumentState::Removed, &[])];
            let projection =
                prepare_knowledge_references(ctx.snapshot(), &h, &entries, content()).unwrap();
            let published = publish_native_authored_with_references(
                ctx.snapshot(),
                &write,
                &h,
                &projection,
                write_limits(),
            )
            .await
            .unwrap();
            assert_eq!(
                read_native_knowledge_page(ctx.snapshot(), &read, "removed", read_limits())
                    .await
                    .unwrap(),
                KnowledgeIndexPageRead::Published {
                    generation: published.default.clone(),
                    page: KnowledgeSnapshotPage::PageAbsent
                }
            );
            assert_eq!(
                read_native_knowledge_heads(ctx.snapshot(), &read, heads())
                    .await
                    .unwrap(),
                KnowledgeHeadRead::Published {
                    generation: published.default,
                    heads: vec![]
                }
            );
        });
    }
}

#[test]
fn selected_guide_heads_use_slug_or_source_identity_without_exposing_bodies() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        let write = authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
            .unwrap();
        let body = "large destination body ".repeat(1000);
        let mut target = page("authored-target", &[]);
        target.title = Some("Target guide");
        target.body = Some(&body);
        target.source_path = "docs/target.md";
        seed(&ctx, &write, &[target]).await;
        let limits = KnowledgeHeadReadLimits {
            max_slug_bytes: n(256),
            native: native(),
            batch_rows: NonZeroU32::new(1).unwrap(),
        };
        for result in [
            read_native_knowledge_head_by_slug(ctx.snapshot(), &read, "authored-target", limits)
                .await
                .unwrap(),
            read_native_knowledge_head_by_source_path(
                ctx.snapshot(),
                &read,
                "docs/target.md",
                limits,
            )
            .await
            .unwrap(),
        ] {
            let KnowledgeHeadRead::Published { heads, .. } = result else {
                panic!("published");
            };
            assert_eq!(heads.len(), 1);
            assert_eq!(heads[0].slug, "authored-target");
            assert_eq!(heads[0].title, "Target guide");
        }
        let KnowledgeHeadRead::Published { heads, .. } = read_native_knowledge_head_by_source_path(
            ctx.snapshot(),
            &read,
            "docs/target.md' OR true",
            limits,
        )
        .await
        .unwrap() else {
            panic!("published");
        };
        assert!(
            heads.is_empty(),
            "source identity is a literal SDK predicate"
        );
        let foreign = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        assert!(matches!(
            read_native_knowledge_head_by_slug(&foreign, &read, "authored-target", limits).await,
            Err(KnowledgeIndexError::DifferentRequest)
        ));
    });
}
