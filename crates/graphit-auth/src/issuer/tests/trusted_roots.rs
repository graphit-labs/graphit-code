use super::*;
use crate::{ProjectReadModule, ProjectRecordAuthority};
use graphit_core::module_settings::Module;

fn additional_project(
    f: &Fixture,
    original: &RequestContext<'_>,
) -> (CapturedPublicationScope, ScopeBindingRecord, VersionContext) {
    let root = f._dir.path().join("additional-project");
    std::fs::create_dir(&root).unwrap();
    let installation = InstallationPaths::resolve(
        f.installation.brand().clone(),
        &f.source_root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let paths = ProjectPaths::at(&root, &installation).unwrap();
    let project = CapturedPublicationScope::project(
        &paths,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        ScopeLimits::new(16, 4096, 128).unwrap(),
        original.snapshot(),
    )
    .unwrap();
    let record = project.binding().unwrap();
    let version =
        VersionContext::select(&paths, Checkout::NonGit, "development", None, "").unwrap();
    (project, record, version)
}

#[test]
fn trusted_roots_runtime_configure_preserves_issuer_and_module_permissions() {
    let cancellation = Cancellation::default();
    let original = request(&cancellation);
    let f = Fixture::new(&original);
    let issuer = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(&original),
        authority(
            &[Permission::Configure, Permission::Read(Module::Task)],
            &[],
            limits(),
            7200,
        ),
        ttl(),
        &original,
    )
    .unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &original).unwrap();
    let actor = actor_bytes(&caller.actor().unwrap());
    let epoch = issuer.epoch;
    let (project, record, version) = additional_project(&f, &original);
    let capture = ScopeCapture {
        live: &project,
        record: &record,
    };
    let scope = || ScopeInput {
        installation: f.realm(),
        project: Some(ProjectCapture {
            scope: capture,
            version: &version,
        }),
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    assert_eq!(
        issuer.validate_scope(&scope(), &original),
        Err(AuthError::Denied)
    );
    issuer
        .trust_project_root(&caller, &original, capture)
        .unwrap();
    issuer
        .trust_project_root(&caller, &original, capture)
        .unwrap();
    issuer.validate_scope(&scope(), &original).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    authority
        .admit_authenticated_read(&caller, &original, scope(), ProjectReadModule::Task)
        .unwrap();
    assert!(matches!(
        authority.admit_authenticated_read(&caller, &original, scope(), ProjectReadModule::Memory),
        Err(AuthError::Denied)
    ));
    assert_eq!(issuer.epoch, epoch);
    assert_eq!(actor_bytes(&caller.actor().unwrap()), actor);
    assert!(Arc::ptr_eq(&issuer.runtime, &f.runtime));
    assert!(
        !issuer
            .realm
            .allowed_roots()
            .any(|root| root == project.owner_root())
    );
    assert!(
        issuer
            .realm_record
            .matches_scope_bounded(
                f.realm().live,
                &ScopeContext::from_snapshot(original.snapshot())
            )
            .unwrap()
    );
}

#[test]
fn trusted_roots_reject_permission_foreign_original_scope_and_cancellation() {
    let cancellation = Cancellation::default();
    let original = request(&cancellation);
    let f = Fixture::new(&original);
    let issuer = f.issuer(&original, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &original).unwrap();
    let (project, record, _) = additional_project(&f, &original);
    let capture = ScopeCapture {
        live: &project,
        record: &record,
    };
    let denied = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(&original),
        authority(&[Permission::Read(Module::Memory)], &[], limits(), 7200),
        ttl(),
        &original,
    )
    .unwrap();
    let denied_caller = denied.authenticate_runtime(&f.secret(), &original).unwrap();
    assert_eq!(
        denied.trust_project_root(&denied_caller, &original, capture),
        Err(AuthError::Denied)
    );
    assert!(
        !denied
            .trusted_roots
            .read()
            .unwrap()
            .contains(project.owner_root())
    );
    let another = request(&cancellation);
    assert_eq!(
        issuer.trust_project_root(&caller, &another, capture),
        Err(AuthError::Denied)
    );
    assert_eq!(
        issuer.trust_project_root(&caller, &original, f.realm()),
        Err(AuthError::Denied)
    );
    let wrong = ScopeCapture {
        live: &project,
        record: &f.project_record,
    };
    assert_eq!(
        issuer.trust_project_root(&caller, &original, wrong),
        Err(AuthError::Denied)
    );
    assert!(
        !issuer
            .trusted_roots
            .read()
            .unwrap()
            .contains(project.owner_root())
    );
    cancellation.cancel();
    assert_eq!(
        issuer.trust_project_root(&caller, &original, capture),
        Err(AuthError::Request)
    );
    assert!(
        !issuer
            .trusted_roots
            .read()
            .unwrap()
            .contains(project.owner_root())
    );
}
