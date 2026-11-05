//! Native FULL16 source through the authenticated App boundary.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
use graphit_app::knowledge_source::*;
use graphit_auth::*;
use graphit_auth::{
    profile::{
        AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
        ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
    },
    store::AuthStore,
};
use graphit_core::text_slice;
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeBindingRecord, ScopeLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::BootstrapLimits,
    installation_identity::InstallationIdentityStore,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    version_context::{Checkout, VersionContext},
};
use graphit_daemon::auth_startup::{DaemonAuth, StartupPolicy};
use graphit_data::{knowledge_document_projection as h, knowledge_index::*};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use graphit_knowledge::document_source::{DocumentSourceError, DocumentSourceLimits};
use std::{
    collections::BTreeMap,
    fs, io,
    num::{NonZeroU32, NonZeroUsize},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, UNIX_EPOCH},
};
fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
fn request(cancel: &Cancellation) -> RequestContext<'_> {
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
        Instant::now() + Duration::from_secs(180),
    )
    .unwrap()
}
fn scopes() -> ScopeLimits {
    ScopeLimits::new(16, 4096, 256).unwrap()
}
fn bootstrap() -> BootstrapLimits {
    BootstrapLimits::new(65536, 256).unwrap()
}
fn documents() -> AuthDocumentLimits {
    AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn profiles() -> ProfileLimits {
    ProfileLimits {
        bytes: 65536,
        nodes: 4096,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 16,
    }
}
fn permissions() -> Vec<Permission> {
    vec![
        Permission::Configure,
        Permission::Read(Module::Knowledge),
        Permission::MutateModule(Module::Knowledge),
    ]
}
fn authority_policy(perms: &[Permission]) -> IssuerAuthorityPolicy {
    let source = SourceAuthorityPolicy {
        ordinary: perms,
        administrative: &[],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    IssuerAuthorityPolicy::new(
        source,
        source,
        IssuerLimits {
            max_permissions: 64,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 4096,
        },
        ServiceAuthorityInput {
            rules: &[],
            limits: ServiceAdmissionLimits {
                max_rules: 16,
                max_binding_bytes: 8192,
            },
            native_refresh_margin: Duration::ZERO,
        },
    )
    .unwrap()
}
fn ttl() -> TtlPolicy {
    TtlPolicy {
        ttl: Duration::from_secs(900),
        maximum: Duration::from_secs(3600),
        refresh_margin: Duration::from_secs(120),
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    install: InstallationPaths,
    paths: ProjectPaths,
    auth: DaemonAuth,
    project: CapturedPublicationScope,
    record: ScopeBindingRecord,
    version: VersionContext,
}
impl Fixture {
    fn new(ctx: &RequestContext<'_>, version: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        let install = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let paths = ProjectPaths::at(&root, &install).unwrap();
        let id = ProjectIdentityStore::at(&paths)
            .writer(Duration::from_secs(5), ctx.snapshot().cancellation())
            .unwrap()
            .ensure(&ProjectMetadata::new("fixture").unwrap())
            .unwrap()
            .id();
        let auth = DaemonAuth::start(
            &install,
            std::slice::from_ref(&paths),
            StartupPolicy {
                bootstrap: bootstrap(),
                runtime: graphit_core::runtime_key::RuntimeKeyLimits::new(65536, 256, 65536)
                    .unwrap(),
                scope: scopes(),
                document: documents(),
                profiles: profiles(),
                authority: authority_policy(&permissions()),
                ttl: ttl(),
            },
            ctx,
        )
        .unwrap();
        let project =
            CapturedPublicationScope::project(&paths, id, scopes(), ctx.snapshot()).unwrap();
        let record = project.binding().unwrap();
        let version = VersionContext::select(&paths, Checkout::NonGit, version, None, "").unwrap();
        Self {
            _temp: temp,
            install,
            paths,
            auth,
            project,
            record,
            version,
        }
    }
    fn other(&self) -> VersionContext {
        VersionContext::select(&self.paths, Checkout::NonGit, "B", None, "").unwrap()
    }
    fn scope(&self) -> ScopeInput<'_> {
        self.scope_for(&self.version)
    }
    fn scope_for<'a>(&'a self, version: &'a VersionContext) -> ScopeInput<'a> {
        ScopeInput {
            installation: self.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.record,
                },
                version,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        }
    }
    fn secret(&self, ctx: &RequestContext<'_>) -> String {
        String::from_utf8(
            self.auth
                .runtime()
                .check(ctx.snapshot())
                .unwrap()
                .secret(ctx.snapshot())
                .unwrap()
                .to_vec(),
        )
        .unwrap()
    }
    fn issuer(&self, ctx: &RequestContext<'_>, perms: &[Permission]) -> RoleIssuer {
        let local = AuthStore::new(
            AuthDocumentStore::capture(
                self.auth.installation().live.clone(),
                documents(),
                ctx.snapshot(),
            )
            .unwrap(),
            profiles(),
            ctx,
        )
        .unwrap();
        RoleIssuer::new(
            self.auth.installation(),
            self.auth.runtime().clone(),
            Arc::new(local),
            authority_policy(perms),
            ttl(),
            ctx,
        )
        .unwrap()
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
        body: Some(
            "\u{feff}---\r\ntitle: body literal\r\n---\r\nBody marker café\nsecond marker\n",
        ),
        kind: Some("page"),
        summary: Some("summary literal"),
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

fn publish_local(auth: &DaemonAuth, paths: &InstallationPaths, ctx: &RequestContext<'_>) {
    let id = InstallationIdentityStore::new(paths)
        .unwrap()
        .load()
        .unwrap()
        .unit_id()
        .unwrap()
        .clone();
    let scope = CapturedPublicationScope::installation(
        paths,
        id,
        [paths.root().to_owned()],
        scopes(),
        ctx.snapshot(),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(scope, documents(), ctx.snapshot()).unwrap(),
        profiles(),
        ctx,
    )
    .unwrap();
    let before = store.load(ctx).unwrap();
    let result = store
        .mutate(before, ctx, |doc, ctx| {
            doc.add_provider(
                "local".into(),
                ProviderConfig {
                    kind: ProviderKind::Local {
                        allow_aws_credential_chain: false,
                        broker: None,
                    },
                    embedding: AiService::Disabled,
                    rerank: AiService::Disabled,
                    s3: S3Location {
                        bucket: None,
                        region: None,
                        endpoint: None,
                        prefix: None,
                        source: S3Source::LocalCredentials,
                    },
                },
                doc.revision(),
                1,
                ctx,
            )?;
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("local").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(
                ProfileData {
                    name: "actual-local".into(),
                    provider: "local".into(),
                    username: "tester".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new("test-local-key".into())),
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                },
                expected,
                1,
                ctx,
            )?;
            doc.activate("actual-local", doc.revision(), ctx)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert!(result.failure().is_none());
    let ScopeCapture { live, record } = auth.installation();
    assert!(live.binding().unwrap() == *record);
}

fn policy() -> KnowledgeSourcePolicy {
    KnowledgeSourcePolicy {
        index: read_limits(),
        source: DocumentSourceLimits {
            max_input_bytes: n(65536),
            max_targets: n(32),
            max_virtual_bytes: n(65536),
            max_virtual_lines: n(4096),
            max_output_bytes: n(131072),
            max_matches: n(4096),
        },
    }
}
async fn publish<'a>(
    f: &'a Fixture,
    ctx: &'a RequestContext<'a>,
    authority: &'a ProjectRecordAuthority<'_>,
    caller: &'a AuthenticatedCaller<'a>,
    pages: &[h::AuthoredPageFields<'_>],
) -> KnowledgeFirstPublication {
    let write = authority
        .admit_authenticated_write(caller, ctx, f.scope(), ProjectWriteOperation::Knowledge)
        .unwrap();
    let rows = h::prepare(ctx.snapshot(), pages, &[], hlimits()).unwrap();
    publish_native_authored(ctx.snapshot(), &write, &rows, write_limits())
        .await
        .unwrap()
}
async fn read<'a>(
    f: &'a Fixture,
    ctx: &'a RequestContext<'a>,
    authority: &'a ProjectRecordAuthority<'_>,
    caller: &'a AuthenticatedCaller<'a>,
    slug: &str,
    selection: &text_slice::Request,
) -> Result<KnowledgeSourceRead, KnowledgeSourceError> {
    read_knowledge_source_authenticated(
        authority,
        caller,
        f.scope(),
        KnowledgeSourceRequest { slug, selection },
        policy(),
        ctx,
    )
    .await
}
fn image(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                result.insert(path.strip_prefix(root).unwrap().to_owned(), None);
                visit(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    Some(fs::read(path).unwrap()),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}
fn io_kind(error: &(dyn std::error::Error + 'static)) -> Option<io::ErrorKind> {
    if let Some(e) = error.downcast_ref::<io::Error>() {
        return Some(e.kind());
    }
    if let Some(e) = error.downcast_ref::<graphit_core::artifact_store::ArtifactError>() {
        return Some(e.kind());
    }
    error.source().and_then(io_kind)
}

#[test]
fn runtime_read_only_distinguishes_absent_and_published_empty_without_creating() {
    native_test(async {
        for published in [false, true] {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx, "A");
            let full = f.auth.issuer();
            let seed_caller = full.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
            let seed_authority = ProjectRecordAuthority::new(full);
            let first = if published {
                Some(publish(&f, &ctx, &seed_authority, &seed_caller, &[]).await)
            } else {
                None
            };
            let reader = f.issuer(&ctx, &[Permission::Read(Module::Knowledge)]);
            let caller = reader.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(&reader);
            let before = image(f._temp.path());
            let result = read(&f, &ctx, &authority, &caller, "guide", &Default::default())
                .await
                .unwrap();
            match first {
                None => {
                    assert_eq!(result, KnowledgeSourceRead::NoCorpus);
                    assert!(
                        !f.project
                            .runtime_root()
                            .join("knowledge/index.lance")
                            .exists()
                    );
                }
                Some(first) => assert_eq!(
                    result,
                    KnowledgeSourceRead::PublishedEmpty {
                        generation: first.named.unwrap()
                    }
                ),
            }
            assert_eq!(before, image(f._temp.path()));
        }
    });
}

#[test]
fn local_read_only_full16_exact_lookup_and_core_selection_are_owned() {
    native_test(async {
        let result = {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx, "A");
            publish_local(&f.auth, &f.install, &ctx);
            let full = f.auth.issuer();
            let seed_caller = full.authenticate_local("test-local-key", &ctx).unwrap();
            let seed_authority = ProjectRecordAuthority::new(full);
            let mut empty = page("aa-empty", &[]);
            empty.body = Some("");
            let target = page("zz-guide", &["dangling", "zz-guide", "dangling"]);
            let first = publish(&f, &ctx, &seed_authority, &seed_caller, &[empty, target]).await;
            let reader = f.issuer(&ctx, &[Permission::Read(Module::Knowledge)]);
            let caller = reader.authenticate_local("test-local-key", &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(&reader);
            assert!(!f.paths.root().join("wiki/document.md").exists());
            let before = image(f._temp.path());
            let full = read(
                &f,
                &ctx,
                &authority,
                &caller,
                "zz-guide",
                &Default::default(),
            )
            .await
            .unwrap();
            let KnowledgeSourceRead::Found {
                generation,
                slug,
                selection,
                focused_source,
            } = &full
            else {
                panic!("target in later native batch must be found")
            };
            assert!(focused_source.is_none());
            assert_eq!(Some(generation), first.named.as_ref());
            assert_eq!(slug, "zz-guide");
            assert!(selection.source.starts_with("---\n"));
            assert!(selection.source.contains("title: ''\n"));
            assert!(selection.source.contains("description: null\n"));
            assert!(selection.source.contains("1969-12-31T23:59:59.999999983Z"));
            assert!(selection.source.contains(&"ff".repeat(32)));
            assert!(selection.source.ends_with(target.body.unwrap()));
            assert_eq!(selection.total_lines, selection.source.split('\n').count());
            let marker = selection
                .source
                .split('\n')
                .position(|s| s == "Body marker café")
                .unwrap()
                + 1;
            let pattern = text_slice::Request {
                pattern: "BODY MARKER".into(),
                after: 1,
                line_numbers: true,
                ..Default::default()
            };
            let KnowledgeSourceRead::Found {
                selection: selected,
                ..
            } = read(&f, &ctx, &authority, &caller, "zz-guide", &pattern)
                .await
                .unwrap()
            else {
                panic!("same page")
            };
            assert_eq!(selected.total_lines, selection.total_lines);
            assert_eq!(selected.matches.len(), 2);
            assert_eq!(selected.matches[0].line_number, marker);
            assert_eq!(selected.matches[0].line, "Body marker café");
            assert!(selected.matches[0].is_match);
            assert_eq!(selected.matches[1].line_number, marker + 1);
            assert!(!selected.matches[1].is_match);
            let no_match = text_slice::Request {
                pattern: "unmatched-token".into(),
                ..Default::default()
            };
            let KnowledgeSourceRead::Found {
                selection: selected,
                ..
            } = read(&f, &ctx, &authority, &caller, "zz-guide", &no_match)
                .await
                .unwrap()
            else {
                panic!("no matches is a found page")
            };
            assert!(selected.source.is_empty() && selected.matches.is_empty());
            assert_eq!(selected.total_lines, selection.total_lines);
            for absent in ["missing", "ZZ-GUIDE", "zz-guide.md", " zz-guide "] {
                assert_eq!(
                    read(&f, &ctx, &authority, &caller, absent, &Default::default())
                        .await
                        .unwrap(),
                    KnowledgeSourceRead::PageAbsent {
                        generation: generation.clone()
                    }
                );
            }
            let KnowledgeSourceRead::Found { selection, .. } = read(
                &f,
                &ctx,
                &authority,
                &caller,
                "aa-empty",
                &Default::default(),
            )
            .await
            .unwrap() else {
                panic!("empty body has native header")
            };
            assert!(!selection.source.is_empty());
            assert!(selection.source.ends_with("---\n\n"));
            assert_eq!(before, image(f._temp.path()));
            full
        };
        // Neither the native scanner, Auth owner, request nor project still exists.
        assert!(matches!(result, KnowledgeSourceRead::Found { slug, .. } if slug == "zz-guide"));
    });
}

#[test]
fn authenticated_selection_uses_named_or_inherited_generation_without_materializing() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "A");
        let full = f.auth.issuer();
        let seed_caller = full.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
        let seed_authority = ProjectRecordAuthority::new(full);
        let first = publish(
            &f,
            &ctx,
            &seed_authority,
            &seed_caller,
            &[page("guide", &[])],
        )
        .await;
        let reader = f.issuer(&ctx, &[Permission::Read(Module::Knowledge)]);
        let caller = reader.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&reader);
        let b = f.other();
        let default = VersionContext::select(&f.paths, Checkout::NonGit, "", None, "").unwrap();
        let before = image(f._temp.path());
        for (version, expected) in [
            (&f.version, first.named.as_ref()),
            (&default, Some(&first.default)),
            (&b, None),
        ] {
            let result = read_knowledge_source_authenticated(
                &authority,
                &caller,
                f.scope_for(version),
                KnowledgeSourceRequest {
                    slug: "guide",
                    selection: &Default::default(),
                },
                policy(),
                &ctx,
            )
            .await
            .unwrap();
            let KnowledgeSourceRead::Found { generation, .. } = result else {
                panic!("fixture published")
            };
            if let Some(expected) = expected {
                assert_eq!(&generation, expected);
            } else {
                assert_eq!(generation.version, first.default.version);
                assert_eq!(generation.physical_branch, None);
                assert_eq!(
                    generation.inherited_target.as_deref(),
                    Some(b.lance_branch().as_str())
                );
            }
        }
        assert_eq!(before, image(f._temp.path()));
    });
}

#[test]
fn denied_foreign_empty_and_oversized_requests_precede_native_visibility() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "A");
        let allowed = f.issuer(&ctx, &[Permission::Read(Module::Knowledge)]);
        let caller = allowed.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&allowed);
        let denied = f.issuer(&ctx, &[]);
        let denied_caller = denied.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
        let denied_authority = ProjectRecordAuthority::new(&denied);
        let foreign_ctx = request(&cancel);
        let foreign_scope = Fixture::new(&ctx, "A");
        let before = image(f._temp.path());
        let foreign_before = image(foreign_scope._temp.path());
        for (a, c, original) in [
            (&denied_authority, &denied_caller, &ctx),
            (&authority, &caller, &foreign_ctx),
            (&denied_authority, &caller, &ctx),
        ] {
            let error = read_knowledge_source_authenticated(
                a,
                c,
                f.scope(),
                KnowledgeSourceRequest {
                    slug: "",
                    selection: &Default::default(),
                },
                policy(),
                original,
            )
            .await
            .unwrap_err();
            assert!(matches!(error, KnowledgeSourceError::Auth(_)));
            assert!(std::error::Error::source(&error).is_some());
        }
        let error = read_knowledge_source_authenticated(
            &authority,
            &caller,
            foreign_scope.scope(),
            KnowledgeSourceRequest {
                slug: "guide",
                selection: &Default::default(),
            },
            policy(),
            &ctx,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, KnowledgeSourceError::Auth(_)));
        assert_eq!(foreign_before, image(foreign_scope._temp.path()));
        assert!(matches!(
            read(&f, &ctx, &authority, &caller, "", &Default::default())
                .await
                .unwrap_err(),
            KnowledgeSourceError::InvalidSlug
        ));
        let oversized = "é".repeat(129);
        assert!(matches!(
            read(
                &f,
                &ctx,
                &authority,
                &caller,
                &oversized,
                &Default::default()
            )
            .await
            .unwrap_err(),
            KnowledgeSourceError::SlugLimit
        ));
        assert_eq!(before, image(f._temp.path()));
    });
}

#[test]
fn selected_identifier_limits_and_renderer_failures_preserve_cause_without_partial_source() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "A");
        let full = f.auth.issuer();
        let seed_caller = full.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
        let seed_authority = ProjectRecordAuthority::new(full);
        publish(
            &f,
            &ctx,
            &seed_authority,
            &seed_caller,
            &[
                page("aa-guide", &["selected-target-too-long"]),
                page("zz-longer-guide", &[]),
            ],
        )
        .await;
        let reader = f.issuer(&ctx, &[Permission::Read(Module::Knowledge)]);
        let caller = reader.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&reader);
        let before = image(f._temp.path());
        let mut cap = policy();
        cap.index.content.max_identifier_bytes = n("aa-guide".len());
        let error = read_knowledge_source_authenticated(
            &authority,
            &caller,
            f.scope(),
            KnowledgeSourceRequest {
                slug: "aa-guide",
                selection: &Default::default(),
            },
            cap,
            &ctx,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, KnowledgeSourceError::Data(ref e) if matches!(e.as_ref(), KnowledgeIndexError::LimitExceeded(_)))
        );
        assert!(std::error::Error::source(&error).is_some());
        for invalid_selection in [false, true] {
            let selection = text_slice::Request {
                is_regex: invalid_selection,
                pattern: if invalid_selection {
                    "[".into()
                } else {
                    String::new()
                },
                ..Default::default()
            };
            let mut cap = policy();
            if !invalid_selection {
                cap.source.max_virtual_bytes = n(1);
            }
            let error = read_knowledge_source_authenticated(
                &authority,
                &caller,
                f.scope(),
                KnowledgeSourceRequest {
                    slug: "aa-guide",
                    selection: &selection,
                },
                cap,
                &ctx,
            )
            .await
            .unwrap_err();
            assert!(
                matches!(error, KnowledgeSourceError::Source(ref e) if matches!(e.as_ref(), DocumentSourceError::Selection(_)) == invalid_selection)
            );
            if !invalid_selection {
                assert!(
                    matches!(error, KnowledgeSourceError::Source(ref e) if matches!(e.as_ref(), DocumentSourceError::Limit("virtual bytes")))
                );
            }
            assert!(std::error::Error::source(&error).is_some());
            assert!(
                !format!("{error:?} {error}").contains(f.project.runtime_root().to_str().unwrap())
            );
        }
        assert_eq!(before, image(f._temp.path()));
    });
}

#[test]
fn actual_other_and_corrupt_corpus_are_errors_without_repair_or_creation() {
    native_test(async {
        for mode in ["other", "unpublished", "corrupt"] {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx, if mode == "corrupt" { "" } else { "A" });
            let full = f.auth.issuer();
            let seed_caller = full.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
            let seed_authority = ProjectRecordAuthority::new(full);
            publish(
                &f,
                &ctx,
                &seed_authority,
                &seed_caller,
                &[page("guide", &[])],
            )
            .await;
            let path = f.project.runtime_root().join("knowledge/index.lance");
            if mode == "corrupt" {
                let mut manifests = 0;
                for entry in fs::read_dir(path.join("_versions")).unwrap() {
                    let manifest = entry.unwrap().path();
                    if manifest.extension().is_some_and(|ext| ext == "manifest") {
                        let mut bytes = fs::read(&manifest).unwrap();
                        let footer = bytes.len().checked_sub(16).unwrap();
                        let offset = usize::try_from(u64::from_le_bytes(
                            bytes[footer..footer + 8].try_into().unwrap(),
                        ))
                        .unwrap();
                        assert!(offset + 4 < footer);
                        // Invalidate protobuf while preserving Lance's real envelope.
                        bytes[offset + 4] = 0;
                        fs::write(manifest, bytes).unwrap();
                        manifests += 1;
                    }
                }
                assert!(manifests > 0);
            } else {
                let permissions = fs::metadata(&path).unwrap().permissions();
                fs::remove_dir_all(&path).unwrap();
                if mode == "other" {
                    fs::write(&path, b"real other file").unwrap();
                } else {
                    fs::create_dir(&path).unwrap();
                    fs::set_permissions(&path, permissions).unwrap();
                    fs::write(path.join("unexpected"), b"no published Main").unwrap();
                }
            }
            let reader = f.issuer(&ctx, &[Permission::Read(Module::Knowledge)]);
            let caller = reader.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(&reader);
            let before = image(f._temp.path());
            let result = read(&f, &ctx, &authority, &caller, "guide", &Default::default()).await;
            if mode == "unpublished" {
                assert_eq!(result.unwrap(), KnowledgeSourceRead::NoCorpus);
                assert_eq!(before, image(f._temp.path()));
                continue;
            }
            let error = result.unwrap_err();
            assert!(matches!(error, KnowledgeSourceError::Data(_)));
            if mode == "other" {
                assert!(
                    matches!(error, KnowledgeSourceError::Data(ref e) if matches!(e.as_ref(), KnowledgeIndexError::Request(_)))
                );
                assert!(io_kind(&error).is_some());
            } else {
                assert!(
                    matches!(error, KnowledgeSourceError::Data(ref e) if matches!(e.as_ref(), KnowledgeIndexError::Branch(_) | KnowledgeIndexError::Store(_)))
                );
            }
            assert!(std::error::Error::source(&error).is_some());
            assert!(!format!("{error:?} {error}").contains(path.to_str().unwrap()));
            assert_eq!(before, image(f._temp.path()));
        }
    });
}

#[test]
fn original_cancellation_in_flight_and_deadline_are_causal_and_read_only() {
    native_test(async {
        for case in 0..3 {
            let setup_cancel = Cancellation::default();
            let setup = request(&setup_cancel);
            let f = Fixture::new(&setup, "A");
            let full = f.auth.issuer();
            let seed_caller = full
                .authenticate_runtime(&f.secret(&setup), &setup)
                .unwrap();
            let seed_authority = ProjectRecordAuthority::new(full);
            publish(
                &f,
                &setup,
                &seed_authority,
                &seed_caller,
                &[page("guide", &[])],
            )
            .await;
            let cancel = Cancellation::default();
            let deadline = Instant::now() + Duration::from_secs(if case == 2 { 1 } else { 180 });
            let ctx = RequestContext::new(
                ContentLimits {
                    max_input_bytes: 65536,
                    max_output_bytes: 65536,
                    max_depth: 32,
                    max_nodes: 65536,
                    max_scalar_bytes: 4096,
                    max_collection_entries: 128,
                },
                &cancel,
                deadline,
            )
            .unwrap();
            let reader = f.issuer(&ctx, &[Permission::Read(Module::Knowledge)]);
            let caller = reader.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(&reader);
            let before = image(f._temp.path());
            if case == 0 {
                cancel.cancel();
            }
            if case == 2 {
                tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
            }
            let selection = text_slice::Request::default();
            let trigger = async {
                if case == 1 {
                    tokio::task::yield_now().await;
                    cancel.cancel();
                }
            };
            let (result, ()) = tokio::join!(
                read(&f, &ctx, &authority, &caller, "guide", &selection),
                trigger
            );
            let error = result.unwrap_err();
            assert_eq!(
                io_kind(&error),
                Some(if case == 2 {
                    io::ErrorKind::TimedOut
                } else {
                    io::ErrorKind::Interrupted
                })
            );
            if case != 1 {
                assert!(matches!(error, KnowledgeSourceError::Request(_)));
            }
            assert_eq!(before, image(f._temp.path()));
        }
    });
}

#[test]
fn selected_source_accepts_discarded_body_totals_and_rejects_invalid_slug_causally() {
    native_test(async {
        let body = "λ".repeat(1000);
        let owned = {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx, "A");
            let full = f.auth.issuer();
            let seed_caller = full.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
            let seed_authority = ProjectRecordAuthority::new(full);
            let reader = f.issuer(&ctx, &[Permission::Read(Module::Knowledge)]);
            let caller = reader.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(&reader);
            let selection = text_slice::Request::default();
            for published in [false, true] {
                if published {
                    let mut picked = page("picked", &[]);
                    picked.body = Some(&body);
                    let mut other = page("other", &[]);
                    other.body = Some(&body);
                    let mut last = page("last", &[]);
                    last.body = Some(&body);
                    publish(
                        &f,
                        &ctx,
                        &seed_authority,
                        &seed_caller,
                        &[picked, other, last],
                    )
                    .await;
                }
                let before = image(f._temp.path());
                for slug in ["a/b", "a\\b", "..picked", "a\n"] {
                    let error = read(&f, &ctx, &authority, &caller, slug, &selection)
                        .await
                        .unwrap_err();
                    assert!(
                        matches!(&error, KnowledgeSourceError::Data(e) if matches!(e.as_ref(), KnowledgeIndexError::InvalidField("slug")))
                    );
                    assert!(
                        std::error::Error::source(&error)
                            .unwrap()
                            .is::<KnowledgeIndexError>()
                    );
                    assert_eq!(format!("{error:?}"), "corpus");
                }
                assert_eq!(image(f._temp.path()), before);
            }
            let before = image(f._temp.path());
            let limits = policy();
            let result = read_knowledge_source_authenticated(
                &authority,
                &caller,
                f.scope(),
                KnowledgeSourceRequest {
                    slug: "picked",
                    selection: &selection,
                },
                limits,
                &ctx,
            )
            .await
            .unwrap();
            let KnowledgeSourceRead::Found {
                selection, slug, ..
            } = &result
            else {
                panic!("one selected source")
            };
            assert_eq!(slug, "picked");
            assert!(selection.source.ends_with(&body));
            assert_eq!(image(f._temp.path()), before);
            result
        };
        let KnowledgeSourceRead::Found { selection, .. } = owned else {
            panic!("owned source")
        };
        assert!(selection.source.ends_with(&body));
    });
}
