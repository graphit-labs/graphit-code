// Shared real Source/Auth fixture for unit and compiled-CLI ingress tests.
use super::{NativeInstallPolicy, SourcePolicy, SourceServer};
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ServiceAdmissionLimits,
    ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy, profile::ProfileLimits,
};
use graphit_core::{
    artifact_store::{
        CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet, ScopeLimits, SnapshotPolicy,
        SourceReadLimits,
    },
    auth_document::AuthDocumentLimits,
    brand::Brand,
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    installation_bootstrap::BootstrapLimits,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectId, ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::RuntimeKeyLimits,
    version_context::VersionContext,
};
use graphit_daemon::auth_startup::{DaemonAuth, StartupPolicy};
use graphit_hub::{
    NativeBinding, native::ContentLimits, policy::RequestContext, registry::HubRegistry,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
pub struct Fixture {
    pub temp: tempfile::TempDir,
    pub server: SourceServer,
    pub stop: CancellationToken,
    pub raw: String,
    pub auth: Arc<DaemonAuth>,
    pub paths: InstallationPaths,
    pub projects: Arc<Vec<(ProjectPaths, ProjectId, VersionContext)>>,
}

pub fn fixture(permissions: &[Permission], brand: Brand, binding: NativeBinding) -> Fixture {
    fixture_with_projects(permissions, brand, binding, &["project"])
}

pub fn fixture_with_projects(
    permissions: &[Permission],
    brand: Brand,
    binding: NativeBinding,
    names: &[&str],
) -> Fixture {
    fixture_with_projects_and_rules(permissions, brand, binding, names, &[])
}

pub fn fixture_with_projects_and_rules(
    permissions: &[Permission],
    brand: Brand,
    binding: NativeBinding,
    names: &[&str],
    rules: &[graphit_auth::ServiceRuleInput<'_>],
) -> Fixture {
    fixture_with_projects_and_authority(permissions, &[], brand, binding, names, rules)
}

pub fn fixture_with_projects_and_authority(
    permissions: &[Permission],
    administrative: &[Permission],
    brand: Brand,
    binding: NativeBinding,
    names: &[&str],
    rules: &[graphit_auth::ServiceRuleInput<'_>],
) -> Fixture {
    fixture_with_imported_ast_provider(
        permissions,
        administrative,
        brand,
        binding,
        names,
        rules,
        None,
    )
}

pub fn fixture_with_imported_ast_provider(
    permissions: &[Permission],
    administrative: &[Permission],
    brand: Brand,
    binding: NativeBinding,
    names: &[&str],
    rules: &[graphit_auth::ServiceRuleInput<'_>],
    imported_ast_provider: Option<&str>,
) -> Fixture {
    fixture_with_web_read_providers(
        permissions,
        administrative,
        brand,
        binding,
        names,
        rules,
        (imported_ast_provider, None),
    )
}

pub(crate) fn fixture_with_web_read_providers(
    permissions: &[Permission],
    administrative: &[Permission],
    brand: Brand,
    binding: NativeBinding,
    names: &[&str],
    rules: &[graphit_auth::ServiceRuleInput<'_>],
    providers: (Option<&str>, Option<&str>),
) -> Fixture {
    let (imported_ast_provider, installed_hub_provider) = providers;
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallationPaths::resolve(
        brand,
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    let cancel = Cancellation::default();
    let content = ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 4096,
        max_collection_entries: 64,
    };
    let ctx =
        RequestContext::new(content, &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let scope = ScopeLimits::new(16, 4096, 256).unwrap();
    let source = || SourceAuthorityPolicy {
        ordinary: permissions,
        administrative,
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    let authority = IssuerAuthorityPolicy::new(
        source(),
        source(),
        IssuerLimits {
            max_permissions: 64,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 4096,
        },
        ServiceAuthorityInput {
            rules,
            limits: ServiceAdmissionLimits {
                max_rules: 16,
                max_binding_bytes: 8192,
            },
            native_refresh_margin: Duration::ZERO,
        },
    )
    .unwrap();
    let authority = match imported_ast_provider {
        Some(provider) => authority.with_web_imported_ast_read(provider).unwrap(),
        None => authority,
    };
    let authority = match installed_hub_provider {
        Some(provider) => authority.with_web_installed_hub_read(provider).unwrap(),
        None => authority,
    };
    let policy = StartupPolicy {
        bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
        runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
        scope,
        document: AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
        profiles: ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
        authority,
        ttl: TtlPolicy {
            ttl: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
            refresh_margin: Duration::from_secs(120),
        },
    };
    let project_paths: Vec<_> = names
        .iter()
        .map(|name| {
            let root = temp.path().join(name);
            std::fs::create_dir(&root).unwrap();
            std::fs::write(root.join("raw"), "actual native payload").unwrap();
            ProjectPaths::at(&root, &paths).unwrap()
        })
        .collect();
    let auth = Arc::new(DaemonAuth::start(&paths, &project_paths, policy, &ctx).unwrap());
    let raw = {
        let key = auth.runtime().check(ctx.snapshot()).unwrap();
        String::from_utf8(key.secret(ctx.snapshot()).unwrap().to_vec()).unwrap()
    };
    let projects: Vec<_> = project_paths
        .into_iter()
        .zip(names)
        .map(|(project, name)| {
            let id = ProjectIdentityStore::at(&project)
                .writer(Duration::from_secs(5), &cancel)
                .unwrap()
                .ensure(&ProjectMetadata::new(name).unwrap())
                .unwrap()
                .id();
            let checkout =
                GitCheckout::inspect(project.root(), Duration::from_secs(5), &cancel).unwrap();
            let version =
                VersionContext::select(&project, checkout.as_checkout(), "", None, "").unwrap();
            (project, id, version)
        })
        .collect();
    let retained = RetainedBytesLimits::new(65536, 32, 4096).unwrap();
    let native_config = graphit_core::config::ConfigSnapshot::new(paths.brand(), []).unwrap();
    let native_original = graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    let policy = SourcePolicy {
        graph_query: graphit_ast::icebug_query::QueryOptions {
            buffer_pool_bytes: 128 * 1024 * 1024,
            query_threads: 2,
            max_rows: (content).max_collection_entries,
            max_bytes: (content).max_output_bytes,
            max_depth: (content).max_depth,
        },
        installation: paths.clone(),
        native_install: NativeInstallPolicy::capture(&native_config, &native_original).unwrap(),
        content,
        timeout: Duration::from_secs(30),
        scope,
        file: SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            CopyLimits::new(1048576, 128, 65536, 8).unwrap(),
            RetainedFileSet::new([], retained).unwrap(),
            retained,
        )
        .unwrap(),
        read: SourceReadLimits::new(65536, 4096, 1048576).unwrap(),
        path: retained,
        index: NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
        binding,
        registry: {
            let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
                auth.installation().live.owner()
            else {
                unreachable!()
            };
            let config =
                graphit_core::config::ConfigSnapshot::new(auth.installation().live.brand(), [])
                    .unwrap();
            HubRegistry::global(
                &paths,
                installation_id,
                graphit_hub::policy::HubResourcePolicy::resolve(&config)
                    .unwrap()
                    .hub(),
            )
            .unwrap()
        },
    };
    let projects = Arc::new(projects);
    let stop = CancellationToken::new();
    let server =
        SourceServer::new(auth.clone(), projects.clone(), 0, policy, stop.clone()).unwrap();
    Fixture {
        temp,
        server,
        stop,
        raw,
        auth,
        paths,
        projects,
    }
}
