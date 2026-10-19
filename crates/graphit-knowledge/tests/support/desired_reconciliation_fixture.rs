//! Real setup shared only by this composition's public and private tests.
use graphit_auth::*;
use graphit_auth::{profile::ProfileLimits, store::AuthStore};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, ProjectSelectionLimits, ScopeBindingRecord, ScopeLimits,
        SourceReadLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
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
    time::{Duration, Instant},
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

pub struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    runtime: Arc<RuntimeKeyOwner>,
    installation: CapturedPublicationScope,
    installation_record: ScopeBindingRecord,
    pub project: CapturedPublicationScope,
    project_record: ScopeBindingRecord,
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
    pub fn realm(&self) -> ScopeCapture<'_> {
        ScopeCapture {
            live: &self.installation,
            record: &self.installation_record,
        }
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
    pub fn secret(&self) -> String {
        std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap()
    }
    pub fn issue(
        &self,
        issuer: &RoleIssuer,
        parent: &AdminParent<'_>,
    ) -> (RoleBearer, RoleCredentialReference) {
        self.issue_for(issuer, parent, &self.version)
    }
    pub fn issue_for(
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

use super::knowledge::{
    desired_corpus::{
        KnowledgeDesiredPreparationLimits as Preparation, KnowledgeSourceCaptureLimits as Capture,
    },
    document::DocumentParseLimits,
    native_document_mapping::NativeAuthoredMappingLimits,
    publication_metadata::PublicationMetadataLimits,
    settings::KnowledgeSettingsLimits,
};
use graphit_core::config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource};
pub fn capture_limits() -> Capture {
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
pub fn preparation() -> Preparation {
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
pub fn config(entries: &[(&str, serde_json::Value)]) -> ConfigSnapshot {
    config_brand(entries, Brand::new("knowledge-test").unwrap())
}
pub fn config_brand(entries: &[(&str, serde_json::Value)], brand: Brand) -> ConfigSnapshot {
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
pub fn put(f: &Fixture, relative: impl AsRef<Path>, body: impl AsRef<[u8]>) {
    let path = f.root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}
// Shared by admission and preparation failure tests.
#[allow(dead_code)]
pub fn assert_no_birth(f: &Fixture) {
    assert!(!f.project.runtime_root().join("knowledge").exists());
}

use super::knowledge::{
    compilation_fingerprint::CompilationFingerprintLimits,
    desired_reconciliation::KnowledgeDesiredReconciliationLimits,
    document_authored_references::DocumentAuthoredReferenceLimits, references::ReferenceLimits,
};
use arrow_array::{Array, RecordBatch, StringArray};
use futures::TryStreamExt;
use graphit_core::{
    artifact_store::{ArtifactLimits, CopyLimits},
    native_document_file::NativeDocumentFileLimits,
};
use graphit_data::{
    knowledge_document_lifecycle::KnowledgeDocumentLifecycleLimits,
    knowledge_document_projection::KnowledgeDocumentProjectionLimits, knowledge_index::*,
    lance_branch_store::BranchSnapshot,
};
use std::{collections::BTreeMap, error::Error, num::NonZeroU32};
pub fn limits() -> KnowledgeDesiredReconciliationLimits {
    let content = KnowledgeIndexContentLimits {
        max_identifier_bytes: n(4096),
    };
    let native = NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap();
    let copy = CopyLimits::new(1024 * 1024, 1024, 65536, 16).unwrap();
    KnowledgeDesiredReconciliationLimits {
        capture: capture_limits(),
        preparation: preparation(),
        fingerprint: CompilationFingerprintLimits {
            max_preimage_bytes: n(65536),
        },
        references: ReferenceLimits {
            max_identifier_bytes: n(4096),
        },
        projection: KnowledgeDocumentProjectionLimits {
            max_documents: n(64),
            max_observations: n(64),
            max_rows: n(128),
            max_field_bytes: n(65536),
            max_text_bytes: n(262144),
        },
        index: KnowledgeDocumentLifecycleLimits {
            read: KnowledgeIndexReadLimits {
                content,
                native,
                batch_rows: NonZeroU32::new(1).unwrap(),
            },
            write: KnowledgeIndexWriteLimits {
                content,
                native,
                artifacts: ArtifactLimits::new(65536, 4096, copy).unwrap(),
            },
        },
        authored_references: DocumentAuthoredReferenceLimits {
            max_results: n(256),
            max_identifier_bytes: n(4096),
        },
    }
}
pub async fn image(pin: &BranchSnapshot) -> Result<BTreeMap<String, RecordBatch>, Box<dyn Error>> {
    let mut scanner = pin.scan();
    scanner.batch_size(1);
    let mut stream = scanner.try_into_stream().await?;
    let mut rows = BTreeMap::new();
    while let Some(batch) = stream.try_next().await? {
        let slugs = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for row in 0..batch.num_rows() {
            rows.insert(slugs.value(row).to_owned(), batch.slice(row, 1));
        }
    }
    Ok(rows)
}
pub fn raw_projection(batch: &RecordBatch) -> &arrow_array::StructArray {
    batch.column(17).as_any().downcast_ref().unwrap()
}
pub fn state(batch: &RecordBatch) -> u8 {
    batch
        .column(16)
        .as_any()
        .downcast_ref::<arrow_array::UInt8Array>()
        .unwrap()
        .value(0)
}
pub fn text(batch: &RecordBatch, column: usize) -> &str {
    batch
        .column(column)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap()
        .value(0)
}
pub fn uri(f: &Fixture) -> String {
    f.project
        .runtime_root()
        .join("knowledge/index.lance")
        .to_str()
        .unwrap()
        .to_owned()
}
pub fn active(batch: &RecordBatch) {
    assert_eq!(
        state(batch),
        graphit_data::knowledge_references::KnowledgeDocumentState::Active as u8
    );
    assert_eq!(batch.num_columns(), 18);
}
pub fn incomplete(batch: &RecordBatch) {
    assert!(raw_projection(batch).is_null(0));
}
