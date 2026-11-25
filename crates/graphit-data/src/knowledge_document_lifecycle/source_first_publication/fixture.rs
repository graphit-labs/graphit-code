//! Real PROJECT setup for Source publication and synchronization-history tests.
use crate::{knowledge_document_projection as h, knowledge_index::*};
use graphit_auth::*;
use graphit_auth::{profile::ProfileLimits, store::AuthStore};
use graphit_core::{
    artifact_store::PublicationScope,
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_lock::Cancellation,
    installation_identity::InstallationId,
    module_settings::Module,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::RuntimeKeyOwner,
    version_context::{Checkout, VersionContext},
};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use std::{
    num::NonZeroUsize,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, UNIX_EPOCH},
};

pub fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
pub fn request(cancel: &Cancellation) -> RequestContext<'_> {
    request_deadline(cancel, Instant::now() + Duration::from_secs(180))
}
pub fn request_deadline(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
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
        Permission::ManageAuthProfiles,
        Permission::ManageAccessGrants,
        Permission::Delegate,
    ]
}

pub struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    runtime: Arc<RuntimeKeyOwner>,
    installation: PublicationScope,
    pub project: PublicationScope,
    version: VersionContext,
}
impl Fixture {
    pub fn new(ctx: &RequestContext<'_>) -> Self {
        Self::with_version(ctx, "")
    }
    pub fn with_version(ctx: &RequestContext<'_>, version_name: &str) -> Self {
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
        let private = {
            std::fs::create_dir_all(install.root().parent().unwrap()).unwrap();
            match graphit_fs::create_private_directory(install.root()) {
                Ok(_) => (),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(error) => panic!("private credential root: {error}"),
            }
            install.root().to_owned()
        };
        let runtime = Arc::new(RuntimeKeyOwner::start(private, ctx.snapshot()).unwrap());
        let paths = ProjectPaths::at(&root, &install).unwrap();

        let installation = PublicationScope::installation(
            &install,
            InstallationId::new("local-installation").unwrap(),
            [paths.root().to_owned()],
        )
        .unwrap();
        let project =
            PublicationScope::project(&paths, "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap())
                .unwrap();
        let version =
            VersionContext::select(&paths, Checkout::NonGit, version_name, None, "").unwrap();

        Self {
            _dir: dir,
            root,
            runtime,
            installation,

            project,

            version,
        }
    }
    pub fn realm(&self) -> &PublicationScope {
        &self.installation
    }
    pub fn scope(&self) -> ScopeInput<'_> {
        self.scope_for(&self.version)
    }
    pub fn named_version(&self, name: &str) -> VersionContext {
        let install = InstallationPaths::resolve(
            self.installation.brand().clone(),
            &self.root,
            None,
            Some(self.installation.owner_root()),
        )
        .unwrap();
        let paths = ProjectPaths::at(&self.root, &install).unwrap();
        VersionContext::select(&paths, Checkout::NonGit, name, None, "").unwrap()
    }
    pub fn scope_for<'a>(&'a self, version: &'a VersionContext) -> ScopeInput<'a> {
        ScopeInput {
            installation: self.realm(),
            project: Some(ProjectSelection {
                scope: &self.project,
                version,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        }
    }
    pub fn issuer(&self, ctx: &RequestContext<'_>) -> RoleIssuer {
        self.issuer_permissions(ctx, &permissions())
    }
    pub fn issuer_permissions(&self, ctx: &RequestContext<'_>, perms: &[Permission]) -> RoleIssuer {
        let paths = InstallationPaths::resolve(
            self.installation.brand().clone(),
            &self.root,
            None,
            Some(self.installation.owner_root()),
        )
        .unwrap();
        let scope = PublicationScope::installation(
            &paths,
            InstallationId::new("local-installation").unwrap(),
            [self.installation.owner_root().to_owned()],
        )
        .unwrap();
        let physical = AuthDocumentStore::new(
            scope.owner_root().to_owned(),
            AuthDocumentLimits::new(4096).unwrap(),
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
    pub fn secret(&self) -> String {
        std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap()
    }
}

fn content() -> KnowledgeIndexContentLimits {
    KnowledgeIndexContentLimits {
        max_identifier_bytes: n(256),
    }
}

pub fn write_limits() -> KnowledgeIndexWriteLimits {
    KnowledgeIndexWriteLimits { content: content() }
}
pub fn hlimits() -> h::KnowledgeDocumentProjectionLimits {
    h::KnowledgeDocumentProjectionLimits {
        max_documents: n(8),
        max_observations: n(8),
        max_rows: n(64),
        max_field_bytes: n(65536),
        max_text_bytes: n(131072),
    }
}
pub fn page<'a>(slug: &'a str, targets: &'a [&'a str]) -> h::AuthoredPageFields<'a> {
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
