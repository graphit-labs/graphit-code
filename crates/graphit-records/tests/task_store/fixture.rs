use graphit_auth::*;
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeBindingRecord, ScopeLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    native_path::NativePath,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_hub::{
    ArtifactType, NativeBinding, ProjectionLimits, SourceOrigin, SourceRecord, SourceRef,
    native::{ContentLimits, Role, RoleCredentialReference},
    policy::RequestContext,
};
use graphit_records::task_store::{TaskStoreReadLimits, TaskStoreWriteLimits};
use std::{
    num::NonZeroU32,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
pub fn request(cancel: &Cancellation) -> RequestContext<'_> {
    request_until(cancel, Instant::now() + Duration::from_secs(120))
}
pub fn request_until(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 1048576,
            max_output_bytes: 1048576,
            max_depth: 64,
            max_nodes: 65536,
            max_scalar_bytes: 65536,
            max_collection_entries: 65536,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
pub fn limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 16,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 4096,
    }
}
pub fn read_limits() -> TaskStoreReadLimits {
    TaskStoreReadLimits {
        native: NativeDocumentFileLimits::new(1048576, 65536, 256, 8 * 1024 * 1024).unwrap(),
        batch_rows: NonZeroU32::new(16).unwrap(),
    }
}
pub fn write_limits() -> TaskStoreWriteLimits {
    TaskStoreWriteLimits {
        read: read_limits(),
        references: graphit_records::task_store::TaskReferencesInputLimits {
            max_entries: std::num::NonZeroUsize::new(65536).unwrap(),
            max_text_bytes: std::num::NonZeroUsize::new(1048576).unwrap(),
        },
        artifacts: ArtifactLimits::new(
            1048576,
            65536,
            CopyLimits::new(1048576, 128, 65536, 16).unwrap(),
        )
        .unwrap(),
    }
}
pub struct Fixture {
    pub installation: CapturedPublicationScope,
    pub project: CapturedPublicationScope,
    installation_record: ScopeBindingRecord,
    project_record: ScopeBindingRecord,
    pub version: VersionContext,
    pub root: PathBuf,
    pub runtime: Arc<RuntimeKeyOwner>,
    _temp: tempfile::TempDir,
}
impl Fixture {
    pub fn new(ctx: &RequestContext<'_>, version: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let root = temp.path().join("project");
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(&root).unwrap();
        let install =
            InstallationPaths::resolve(Brand::default(), &root, Some(&home), None).unwrap();
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
        ProjectIdentityStore::at(&paths)
            .writer(Duration::from_secs(5), ctx.snapshot().cancellation())
            .unwrap()
            .ensure_with_id(
                &ProjectMetadata::new("task-fixture").unwrap(),
                "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            )
            .unwrap();
        let sl = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &install,
            InstallationId::new("native-task-test").unwrap(),
            [root.clone()],
            sl,
            ctx.snapshot(),
        )
        .unwrap();
        let project = CapturedPublicationScope::project(
            &paths,
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            sl,
            ctx.snapshot(),
        )
        .unwrap();
        let version = VersionContext::select(&paths, Checkout::NonGit, version, None, "").unwrap();
        let installation_record = installation.binding().unwrap();
        let project_record = project.binding().unwrap();
        Self {
            installation,
            project,
            installation_record,
            project_record,
            version,
            root,
            runtime,
            _temp: temp,
        }
    }
    pub fn scope(&self) -> ScopeInput<'_> {
        ScopeInput {
            installation: ScopeCapture {
                live: &self.installation,
                record: &self.installation_record,
            },
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.project_record,
                },
                version: &self.version,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        }
    }
    pub fn issuer(&self, ctx: &RequestContext<'_>) -> RoleIssuer {
        let paths = InstallationPaths::resolve(
            self.installation.brand().clone(),
            &self.root,
            None,
            Some(self.installation.owner_root()),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("native-task-test").unwrap(),
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
        let store = Arc::new(
            store::AuthStore::new(
                physical,
                profile::ProfileLimits {
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
        let permissions = [
            Permission::Read(Module::Task),
            Permission::WriteTask,
            Permission::WriteOwnTask,
            Permission::CoordinateTask,
            Permission::ManageAccessGrants,
            Permission::Delegate,
        ];
        let sessions = AdminSessionPolicy {
            lifetime: Duration::from_secs(3600),
            maximum: Duration::from_secs(3600),
        };
        let authority = IssuerAuthorityPolicy::new(
            SourceAuthorityPolicy {
                ordinary: &permissions,
                administrative: &permissions,
                sessions,
            },
            SourceAuthorityPolicy {
                ordinary: &permissions,
                administrative: &permissions,
                sessions,
            },
            limits(),
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
            self.scope().installation,
            Arc::clone(&self.runtime),
            store,
            authority,
            TtlPolicy {
                ttl: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
                refresh_margin: Duration::from_secs(120),
            },
            ctx,
        )
        .unwrap()
    }
    #[allow(
        dead_code,
        reason = "shared fixture; exercised by task_store integration tests"
    )]
    pub fn issue(
        &self,
        issuer: &RoleIssuer,
        ctx: &RequestContext<'_>,
    ) -> (RoleBearer, RoleCredentialReference, RunHandle) {
        self.issue_for(issuer, ctx, self.scope())
    }
    #[allow(
        dead_code,
        reason = "shared fixture; registered credentials are exercised by task_store integration tests"
    )]
    pub fn issue_registered(
        &self,
        issuer: &RoleIssuer,
        ctx: &RequestContext<'_>,
        reference: &RoleCredentialReference,
    ) -> (RoleBearer, RunHandle) {
        let secret =
            std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, ctx).unwrap();
        let admin = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &admin).unwrap();
        let scope = self.scope();
        let run = issuer
            .start_run(&parent, &scope, Duration::from_secs(3600))
            .unwrap();
        let permissions = [
            Permission::Read(Module::Task),
            Permission::WriteTask,
            Permission::WriteOwnTask,
            Permission::CoordinateTask,
        ];
        let bearer = issuer
            .issue(&parent, &run, reference, &permissions, &scope)
            .unwrap();
        (bearer, run)
    }
    #[allow(
        dead_code,
        reason = "shared fixture; exercised by task_store integration tests"
    )]
    pub fn issue_for(
        &self,
        issuer: &RoleIssuer,
        ctx: &RequestContext<'_>,
        scope: ScopeInput<'_>,
    ) -> (RoleBearer, RoleCredentialReference, RunHandle) {
        let secret =
            std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, ctx).unwrap();
        let admin = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &admin).unwrap();
        let source = SourceRef::restore(
            SourceRecord {
                remote_file_origin: None,
                archive_sha256: None,
                origin: SourceOrigin::LocalLink,
                producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
                artifact_type: ArtifactType::Agent,
                id: "roles/coordinator".into(),
                physical_id: None,
                version: "1.0.0".into(),
                root: NativePath::new(self.root.clone()).unwrap(),
            },
            ProjectionLimits::new(128, 4096, 32, 128, 32).unwrap(),
        )
        .unwrap();
        let permissions = [
            Permission::Read(Module::Task),
            Permission::WriteTask,
            Permission::WriteOwnTask,
            Permission::CoordinateTask,
        ];
        let definition = RoleDefinition::new(
            Role::Coordinator,
            &source,
            NativeBinding::KiroV3,
            &permissions,
            [if scope.project.unwrap().version.version() == "main" {
                2
            } else {
                1
            }; 32],
            limits(),
        )
        .unwrap();
        let reference = definition.reference().clone();
        issuer.register_definition(&parent, definition).unwrap();
        let run = issuer
            .start_run(&parent, &scope, Duration::from_secs(3600))
            .unwrap();
        let bearer = issuer
            .issue(&parent, &run, &reference, &permissions, &scope)
            .unwrap();
        (bearer, reference, run)
    }
}
