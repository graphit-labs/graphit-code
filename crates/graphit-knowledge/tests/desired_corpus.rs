#![cfg(target_os = "linux")]
//! Complete desired corpus through the real Auth/Core selection producers.
use graphit_auth::*;
use graphit_auth::{profile::ProfileLimits, store::AuthStore};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, ProjectSelectionLimits, ScopeBindingRecord, ScopeLimits,
        SourceReadLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    module_settings::Module,
    native_path::NativePath,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_hub::{
    ArtifactType, NativeBinding, ProjectionLimits, SourceOrigin, SourceRecord, SourceRef,
    native::{ContentLimits, Role, RoleCredentialReference},
    policy::RequestContext,
};
use std::{
    num::NonZeroUsize,
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
fn issuer_limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 64,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 1024,
    }
}
fn permissions() -> Vec<Permission> {
    vec![
        Permission::Read(Module::Memory),
        Permission::Read(Module::Task),
        Permission::Read(Module::Knowledge),
        Permission::MutateModule(Module::Knowledge),
        Permission::WriteMemory,
        Permission::WriteTask,
        Permission::ManageAuthProfiles,
        Permission::ManageAccessGrants,
        Permission::Delegate,
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
        let perms = permissions();
        let source = SourceAuthorityPolicy {
            ordinary: &perms,
            administrative: &perms,
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
    fn issue(
        &self,
        issuer: &RoleIssuer,
        parent: &AdminParent<'_>,
    ) -> (RoleBearer, RoleCredentialReference) {
        self.issue_for(issuer, parent, &self.version)
    }
    fn issue_for(
        &self,
        issuer: &RoleIssuer,
        parent: &AdminParent<'_>,
        version: &VersionContext,
    ) -> (RoleBearer, RoleCredentialReference) {
        let source = SourceRef::restore(
            SourceRecord {
                remote_file_origin: None,
                archive_sha256: None,
                origin: SourceOrigin::LocalLink,
                producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
                artifact_type: ArtifactType::Agent,
                id: format!("roles/coordinator-{}", version.version()),
                physical_id: None,
                version: "1.0.0".into(),
                root: NativePath::new(self.root.clone()).unwrap(),
            },
            ProjectionLimits::new(128, 4096, 32, 128, 32).unwrap(),
        )
        .unwrap();
        let perms = permissions();
        let definition = RoleDefinition::new(
            Role::Coordinator,
            &source,
            NativeBinding::KiroV3,
            &perms,
            [1; 32],
            issuer_limits(),
        )
        .unwrap();
        let reference = definition.reference().clone();
        issuer.register_definition(parent, definition).unwrap();
        let run = issuer
            .start_run(parent, &self.scope_for(version), Duration::from_secs(3600))
            .unwrap();
        let bearer = issuer
            .issue(parent, &run, &reference, &perms, &self.scope_for(version))
            .unwrap();
        (bearer, reference)
    }
}

use graphit_core::config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource};
use graphit_knowledge::{
    desired_corpus::{
        DesiredCorpusError as Error, KnowledgeDesiredPreparationLimits as Preparation,
        KnowledgeSourceCaptureLimits as Capture, capture_knowledge_sources as capture,
        prepare_desired_knowledge_corpus as prepare,
    },
    document::DocumentParseLimits,
    native_document_mapping::NativeAuthoredMappingLimits,
    publication_metadata::PublicationMetadataLimits,
    settings::KnowledgeSettingsLimits,
};
fn capture_limits() -> Capture {
    Capture {
        settings: KnowledgeSettingsLimits {
            max_input_bytes: n(4096),
            max_docs_dir_bytes: n(4096),
            max_path_components: n(32),
            max_extension_tokens: n(64),
            max_extensions: n(64),
            max_extension_bytes: n(128),
            max_projection_bytes: n(4096),
        },
        selection: ProjectSelectionLimits {
            max_depth: 32,
            max_input_items: 128,
            max_input_bytes: 65536,
            max_observed_path_bytes: 4096,
            max_selected_file_bytes: 4096,
            max_control_file_bytes: 4096,
        },
        read: SourceReadLimits::new(4096, 4096, 65536).unwrap(),
    }
}
fn preparation() -> Preparation {
    Preparation {
        parser: DocumentParseLimits {
            max_document_bytes: n(4096),
            max_input_bytes: n(4096),
            max_frontmatter_bytes: n(4096),
            max_yaml_events: n(256),
            max_markdown_events: n(256),
            max_yaml_depth: n(16),
            max_scalar_bytes: n(4096),
            max_links: n(128),
            max_projection_bytes: n(65536),
            max_summary_bytes: n(1024),
        },
        metadata: PublicationMetadataLimits {
            max_input_bytes: n(4096),
            max_title_bytes: n(1024),
            max_kind_bytes: n(1024),
            max_description_bytes: n(1024),
            max_effective_summary_bytes: n(1024),
            max_effective_metadata_bytes: n(8192),
            max_owned_metadata_bytes: n(8192),
            max_root_entries: n(128),
            max_typed_nodes: n(256),
            max_typed_depth: n(16),
            max_typed_scalar_bytes: n(4096),
            max_typed_events: n(512),
            max_typed_lookahead_characters: n(4096),
            max_typed_flow_depth: n(16),
        },
        mapping: NativeAuthoredMappingLimits {
            max_field_bytes: n(4096),
            max_uri_bytes: n(4096),
            max_target_bytes: n(4096),
        },
    }
}
fn config(entries: &[(&str, serde_json::Value)]) -> ConfigSnapshot {
    let brand = Brand::new("knowledge-test").unwrap();
    ConfigSnapshot::new(
        &brand,
        [ConfigDocument::new(
            &brand,
            ConfigSource::Global,
            entries
                .iter()
                .map(|(key, value)| (ConfigKey::new(key).unwrap(), value.clone())),
        )
        .unwrap()],
    )
    .unwrap()
}
fn put(f: &Fixture, relative: impl AsRef<Path>, body: impl AsRef<[u8]>) {
    let path = f.root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}
fn setup_grant<'a>(
    f: &'a Fixture,
    issuer: &'a RoleIssuer,
    ctx: &'a RequestContext<'a>,
) -> (RoleBearer, RoleCredentialReference) {
    let caller = issuer.authenticate_runtime(&f.secret(), ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    f.issue(issuer, &parent)
}
fn assert_no_birth(f: &Fixture) {
    assert!(!f.project.runtime_root().join("knowledge").exists());
}

#[test]
fn configured_selection_is_complete_deduplicated_and_borrows_whole_bodies() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let (bearer, reference) = setup_grant(&f, &issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let authored = "\u{feff}---\r\nid: ' Manual Ω '\r\ntitle: Guia\r\ndescription: 'Literal'\r\n---\r\n# Outro\r\n\r\nOlá 世界.\r\n";
    put(&f, "docs/guide.md", authored);
    put(&f, "docs/ação.md", "# Ação\nConteúdo");
    put(&f, "ReadMe.md", "# Readme\nRaiz");
    put(&f, "outside.md", "# Fora");
    for path in [
        "docs/custom.md",
        "docs/git.md",
        "docs/program.o",
        "docs/node_modules/private.md",
    ] {
        put(&f, path, "# Excluído");
    }
    put(&f, ".wikiignore", "docs/custom.md\n!docs/node_modules/\n");
    put(&f, ".gitignore", "docs/git.md\n");
    let snapshot = config(&[("knowledge.extensions", serde_json::json!("md,o"))]);
    let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    let corpus = prepare(ctx.snapshot(), &read, &sources, "spec", preparation()).unwrap();
    let docs = corpus.documents(ctx.snapshot(), &read).unwrap();
    assert_eq!(docs.len(), 3);
    assert_eq!(
        docs.iter().map(|d| d.relative()).collect::<Vec<_>>(),
        [
            Path::new("ReadMe.md"),
            Path::new("docs/ação.md"),
            Path::new("docs/guide.md")
        ]
    );
    let guide = docs.iter().find(|d| d.slug() == " Manual Ω ").unwrap();
    assert_eq!(guide.compiled().whole_source(), authored);
    assert_eq!(guide.compiled().title(), "Guia");
    assert_eq!(guide.compiled().description(), Some("Literal"));
    assert_eq!(guide.compiled().kind(), "spec");
    assert_eq!(
        guide.source_path(),
        "knowledge-source/t/646f6373/67756964652e6d64"
    );
    let reader = sources.reader(ctx.snapshot(), &read).unwrap();
    assert_eq!(
        guide.compiled().whole_source().as_ptr(),
        reader.retained(guide.relative()).unwrap().as_ptr()
    );
    let node = reader
        .nodes()
        .find(|n| n.relative() == guide.relative())
        .unwrap();
    assert_eq!(
        guide.compiled().physical_source_digest().as_bytes(),
        &node.digest().unwrap()
    );
    let views = corpus
        .authored_inputs(
            ctx.snapshot(),
            &read,
            Some(UNIX_EPOCH + Duration::from_secs(5)),
            Some(Duration::from_secs(60)),
        )
        .unwrap();
    assert_eq!(views.len(), 3);
    assert!(views.iter().all(|v| v.stale_since.is_none()
        && v.updated_at == Some(UNIX_EPOCH + Duration::from_secs(5))
        && v.stale_after == Some(Duration::from_secs(60))));
    let all = capture(
        ctx.snapshot(),
        &read,
        &snapshot,
        Some("."),
        capture_limits(),
    )
    .unwrap();
    let all_corpus = prepare(ctx.snapshot(), &read, &all, "spec", preparation()).unwrap();
    let all_docs = all_corpus.documents(ctx.snapshot(), &read).unwrap();
    assert_eq!(all_docs.len(), 4);
    assert_eq!(
        all_docs
            .iter()
            .filter(|d| d.relative() == Path::new("ReadMe.md"))
            .count(),
        1
    );
    assert_no_birth(&f);
}

#[test]
fn empty_and_readme_only_are_real_selections_and_late_errors_never_become_empty() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let (bearer, reference) = setup_grant(&f, &issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let snapshot = config(&[]);
    let empty = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    let corpus = prepare(ctx.snapshot(), &read, &empty, "spec", preparation()).unwrap();
    assert!(corpus.documents(ctx.snapshot(), &read).unwrap().is_empty());
    put(&f, "README.md", "# Raiz");
    let readme = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    let corpus = prepare(ctx.snapshot(), &read, &readme, "spec", preparation()).unwrap();
    assert_eq!(corpus.documents(ctx.snapshot(), &read).unwrap().len(), 1);
    let disabled = config(&[("knowledge.include_readme", serde_json::json!(false))]);
    let sources = capture(ctx.snapshot(), &read, &disabled, None, capture_limits()).unwrap();
    assert!(
        prepare(ctx.snapshot(), &read, &sources, "spec", preparation())
            .unwrap()
            .documents(ctx.snapshot(), &read)
            .unwrap()
            .is_empty()
    );
    put(&f, "docs/a-valid.md", "# Valido");
    put(&f, "docs/z-invalid.md", [0xff]);
    let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    assert!(matches!(
        prepare(ctx.snapshot(), &read, &sources, "spec", preparation()),
        Err(Error::Parser(_))
    ));
    std::fs::remove_dir_all(f.root.join("docs")).unwrap();
    put(
        &f,
        "docs",
        "A FILE cannot be the configured document directory",
    );
    assert!(matches!(
        capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()),
        Err(Error::Source { .. })
    ));
    assert_no_birth(&f);
}

#[test]
fn authored_id_requires_strings_and_preserves_quoted_numeric_literals() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let (bearer, reference) = setup_grant(&f, &issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let snapshot = config(&[]);
    put(&f, "docs/a.md", "# Antes");
    for bad in [
        "null",
        "true",
        "123",
        "1.5",
        ".nan",
        "[]",
        "{}",
        "''",
        "'../wrong'",
        "'a/b'",
    ] {
        put(&f, "docs/z.md", format!("---\nid: {bad}\n---\n# Último"));
        let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
        let result = prepare(ctx.snapshot(), &read, &sources, "spec", preparation());
        assert!(
            matches!(
                result,
                Err(Error::TypedId(_) | Error::InvalidId | Error::Identity(_))
            ),
            "{bad}: {result:?}"
        );
    }
    for literal in ["'123'", "'true'", "' Manual Ω '"] {
        put(
            &f,
            "docs/z.md",
            format!("---\nid: {literal}\n---\n# Literal"),
        );
        let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
        let corpus = prepare(ctx.snapshot(), &read, &sources, "spec", preparation()).unwrap();
        let docs = corpus.documents(ctx.snapshot(), &read).unwrap();
        assert_eq!(
            docs.iter()
                .find(|d| d.relative() == Path::new("docs/z.md"))
                .unwrap()
                .slug(),
            literal
                .strip_prefix('\'')
                .unwrap()
                .strip_suffix('\'')
                .unwrap()
        );
    }
    put(&f, "docs/z.md", "---\nid: ---\n---\n# Contexto");
    let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    let corpus = prepare(ctx.snapshot(), &read, &sources, "spec", preparation()).unwrap();
    assert_eq!(
        corpus
            .documents(ctx.snapshot(), &read)
            .unwrap()
            .iter()
            .find(|d| d.relative() == Path::new("docs/z.md"))
            .unwrap()
            .slug(),
        "---"
    );
    put(&f, "docs/z.md", "---\nid: ...\n---\n# Contexto");
    let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    assert!(matches!(prepare(ctx.snapshot(), &read, &sources, "spec", preparation()),
        Err(Error::Identity(graphit_data::knowledge_document_lifecycle::KnowledgeSourceIdentityError::InvalidAuthoredId))));
    put(
        &f,
        "docs/z.md",
        "---\nid: Manual\ncustom: .nan\ndescription: '.nan'\n---\n# Editoriais",
    );
    let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    let corpus = prepare(ctx.snapshot(), &read, &sources, "spec", preparation()).unwrap();
    let docs = corpus.documents(ctx.snapshot(), &read).unwrap();
    let editorial = docs
        .iter()
        .find(|d| d.relative() == Path::new("docs/z.md"))
        .unwrap();
    assert_eq!(editorial.slug(), "Manual");
    assert_eq!(editorial.compiled().description(), Some(".nan"));
    for (yaml, literal) in [
        ("{id: 'A,B', custom: .nan}", "A,B"),
        ("{id: Manual\n continued, custom: .nan}", "Manual continued"),
    ] {
        put(
            &f,
            "docs/z.md",
            format!("---\n{yaml}\n---\n# Tipos contextuais"),
        );
        let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
        let corpus = prepare(ctx.snapshot(), &read, &sources, "spec", preparation()).unwrap();
        let docs = corpus.documents(ctx.snapshot(), &read).unwrap();
        assert_eq!(
            docs.iter()
                .find(|d| d.relative() == Path::new("docs/z.md"))
                .unwrap()
                .slug(),
            literal
        );
    }
    for yaml in ["id: !!str .nan", "id: !!str 123"] {
        put(
            &f,
            "docs/z.md",
            format!("---\n{yaml}\n---\n# Tags rejeitadas"),
        );
        let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
        assert!(matches!(
            prepare(ctx.snapshot(), &read, &sources, "spec", preparation()),
            Err(Error::Parser(
                graphit_knowledge::document::DocumentParseError::UnsupportedFeature("YAML tag")
            ))
        ));
    }
    for yaml in ["id: A\nid: B", "id: &anchor A", "id: *missing"] {
        put(&f, "docs/z.md", format!("---\n{yaml}\n---\n# Invalid"));
        let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
        assert!(matches!(
            prepare(ctx.snapshot(), &read, &sources, "spec", preparation()),
            Err(Error::Parser(_))
        ));
    }
    assert_no_birth(&f);
}

#[test]
fn rename_and_slug_collision_obey_literal_or_default_identity() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let (bearer, reference) = setup_grant(&f, &issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let snapshot = config(&[]);
    put(&f, "docs/a.md", "---\nid: Manual\n---\n# Página");
    let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    let corpus = prepare(ctx.snapshot(), &read, &sources, "spec", preparation()).unwrap();
    let before = corpus.documents(ctx.snapshot(), &read).unwrap()[0]
        .source_path()
        .to_owned();
    std::fs::rename(f.root.join("docs/a.md"), f.root.join("docs/b.md")).unwrap();
    let next = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    let next = prepare(ctx.snapshot(), &read, &next, "spec", preparation()).unwrap();
    let docs = next.documents(ctx.snapshot(), &read).unwrap();
    assert_eq!(docs[0].slug(), "Manual");
    assert_ne!(docs[0].source_path(), before);
    assert!(corpus.documents(ctx.snapshot(), &read).is_err());
    put(&f, "docs/b.md", "# Sem ID");
    let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    let corpus = prepare(ctx.snapshot(), &read, &sources, "spec", preparation()).unwrap();
    let before = corpus.documents(ctx.snapshot(), &read).unwrap()[0]
        .slug()
        .to_owned();
    std::fs::rename(f.root.join("docs/b.md"), f.root.join("docs/c.md")).unwrap();
    let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    let corpus = prepare(ctx.snapshot(), &read, &sources, "spec", preparation()).unwrap();
    let after = corpus.documents(ctx.snapshot(), &read).unwrap()[0]
        .slug()
        .to_owned();
    assert_ne!(after, before);
    put(
        &f,
        "docs/d.md",
        format!("---\nid: '{after}'\n---\n# Colisão"),
    );
    let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    assert!(matches!(
        prepare(ctx.snapshot(), &read, &sources, "spec", preparation()),
        Err(Error::DuplicateSlug {
            first: 0,
            second: 1
        })
    ));
    assert_no_birth(&f);
}

#[test]
fn native_non_utf8_filename_is_rejected_without_replacement() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let (bearer, reference) = setup_grant(&f, &issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let relative = PathBuf::from(OsString::from_vec(b"docs/\xff.md".to_vec()));
    put(&f, &relative, "Corpo sem heading");
    let snapshot = config(&[]);
    let error = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits())
        .err()
        .unwrap();
    assert!(matches!(error, Error::Source { .. }));
    assert_no_birth(&f);
}

#[test]
fn individual_field_file_and_real_control_limits_are_enforced() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let (bearer, reference) = setup_grant(&f, &issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let a = "# A\nAlfa";
    let b = "# B\nBeta";
    put(&f, "docs/a.md", a);
    put(&f, "docs/b.md", b);
    let snapshot = config(&[]);
    let mut exact_capture = capture_limits();
    exact_capture.selection.max_selected_file_bytes = u64::try_from(a.len().max(b.len())).unwrap();
    let sources = capture(ctx.snapshot(), &read, &snapshot, None, exact_capture).unwrap();
    let corpus = prepare(ctx.snapshot(), &read, &sources, "spec", preparation()).unwrap();
    let docs = corpus.documents(ctx.snapshot(), &read).unwrap();
    // These are the nine public mapping input fields, not allocator/RSS quantities.
    let fields: Vec<&str> = docs
        .iter()
        .flat_map(|d| {
            [
                d.slug(),
                d.source_path(),
                d.compiled().whole_source(),
                d.compiled().title(),
                d.compiled().kind(),
                d.compiled().summary(),
                d.compiled().description().unwrap_or(""),
                d.compiled().recipe_policy().fallback_title.as_str(),
                d.compiled().recipe_policy().kind_on_absent.as_str(),
            ]
        })
        .collect();
    let largest = fields.iter().map(|field| field.len()).max().unwrap();
    let mut exact = preparation();
    exact.mapping.max_field_bytes = n(largest);
    prepare(ctx.snapshot(), &read, &sources, "spec", exact).unwrap();
    let mut exceeded = exact;
    exceeded.mapping.max_field_bytes = n(largest - 1);
    assert!(matches!(
        prepare(ctx.snapshot(), &read, &sources, "spec", exceeded),
        Err(Error::Identity(_) | Error::LimitExceeded("field bytes"))
    ));
    exact_capture.selection.max_selected_file_bytes -= 1;
    assert!(
        matches!(capture(ctx.snapshot(), &read, &snapshot, None, exact_capture),
        Err(Error::Source { cause: ref e, .. }) if e.kind() == std::io::ErrorKind::FileTooLarge)
    );
    let mut parser = preparation();
    parser.parser.max_document_bytes = n(a.len() - 1);
    assert!(matches!(
        prepare(ctx.snapshot(), &read, &sources, "spec", parser),
        Err(Error::Parser(_))
    ));
    let control = "ignored.md\n";
    put(&f, ".wikiignore", control);
    let mut limited = capture_limits();
    limited.selection.max_control_file_bytes = (control.len() - 1) as u64;
    assert!(
        matches!(capture(ctx.snapshot(), &read, &snapshot, None, limited),
        Err(Error::Source { cause: ref e, .. }) if e.kind() == std::io::ErrorKind::FileTooLarge)
    );
    limited.selection.max_control_file_bytes += 1;
    capture(ctx.snapshot(), &read, &snapshot, None, limited).unwrap();
    let invalid_config = config(&[("knowledge.include_readme", serde_json::json!(42))]);
    assert!(matches!(
        capture(
            ctx.snapshot(),
            &read,
            &invalid_config,
            None,
            capture_limits()
        ),
        Err(Error::Settings(_))
    ));
    assert_no_birth(&f);
}

#[test]
fn source_body_membership_and_control_changes_invalidate_before_and_after_preparation() {
    for mutation in ["body", "new_file", "control", "removed"] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let (bearer, reference) = setup_grant(&f, &issuer, &ctx);
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
            .unwrap();
        let snapshot = config(&[]);
        put(&f, "docs/a.md", "# Antes");
        let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
        let corpus = prepare(ctx.snapshot(), &read, &sources, "spec", preparation()).unwrap();
        match mutation {
            "body" => put(&f, "docs/a.md", "# Depois"),
            "new_file" => put(&f, "docs/b.md", "# Novo"),
            "control" => put(&f, ".wikiignore", "docs/a.md\n"),
            "removed" => std::fs::remove_file(f.root.join("docs/a.md")).unwrap(),
            _ => unreachable!(),
        }
        assert!(
            matches!(
                prepare(ctx.snapshot(), &read, &sources, "spec", preparation()),
                Err(Error::Source { .. })
            ),
            "{mutation}"
        );
        assert!(
            matches!(
                corpus.documents(ctx.snapshot(), &read),
                Err(Error::Source { .. })
            ),
            "{mutation}"
        );
        assert!(
            matches!(
                corpus.authored_inputs(ctx.snapshot(), &read, None, None),
                Err(Error::Source { .. })
            ),
            "{mutation}"
        );
        assert!(
            matches!(
                sources.reader(ctx.snapshot(), &read),
                Err(Error::Source { .. })
            ),
            "{mutation}"
        );
        assert_no_birth(&f);
    }
}

#[test]
fn original_scope_module_revocation_cancellation_and_deadline_are_real_guards() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let memory = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let snapshot = config(&[]);
    put(&f, "docs/a.md", "# A");
    let sources = capture(ctx.snapshot(), &read, &snapshot, None, capture_limits()).unwrap();
    let corpus = prepare(ctx.snapshot(), &read, &sources, "spec", preparation()).unwrap();
    assert!(matches!(
        capture(ctx.snapshot(), &memory, &snapshot, None, capture_limits()),
        Err(Error::WrongAdmission)
    ));
    let foreign = Arc::new(SnapshotContext::new(
        ctx.deadline().expect("finite fixture deadline"),
        Cancellation::default(),
    ));
    assert!(matches!(
        capture(&foreign, &read, &snapshot, None, capture_limits()),
        Err(Error::DifferentRequest)
    ));
    assert!(matches!(
        prepare(&foreign, &read, &sources, "spec", preparation()),
        Err(Error::DifferentRequest)
    ));
    let expired = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    assert!(
        matches!(capture(&expired, &read, &snapshot, None, capture_limits()),
        Err(Error::Request(ref e)) if e.kind() == std::io::ErrorKind::TimedOut)
    );
    let install = InstallationPaths::resolve(
        f.installation.brand().clone(),
        &f.root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let paths = ProjectPaths::at(&f.root, &install).unwrap();
    let version = VersionContext::select(&paths, Checkout::NonGit, "other", None, "").unwrap();
    let (other_bearer, other_reference) = f.issue_for(&issuer, &parent, &version);
    let other = issuer
        .verify(
            other_bearer.expose_for_transport(),
            &other_reference,
            f.scope_for(&version),
            &ctx,
        )
        .unwrap();
    let other_read = authority
        .admit_read(&other, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    assert!(matches!(
        prepare(ctx.snapshot(), &other_read, &sources, "spec", preparation()),
        Err(Error::ScopeChanged)
    ));
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        corpus.documents(ctx.snapshot(), &read),
        Err(Error::Auth(AuthError::Denied))
    ));
    cancel.cancel();
    assert!(
        matches!(prepare(ctx.snapshot(), &read, &sources, "spec", preparation()),
        Err(Error::Request(ref e)) if e.kind() == std::io::ErrorKind::Interrupted)
    );
    assert_no_birth(&f);
}
