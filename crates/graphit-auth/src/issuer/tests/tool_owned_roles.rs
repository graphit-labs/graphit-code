use super::*;
use graphit_hub::native::RoleSource;

fn managed(role: Role, permission: Permission, policy: u8) -> RoleDefinition {
    RoleDefinition::tool_owned(
        role,
        &Brand::default(),
        NativeBinding::KiroV3,
        &[permission],
        [policy; 32],
        limits(),
    )
    .unwrap()
}

#[test]
fn three_managed_definitions_require_real_parent_and_role_permission() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let fixture = Fixture::new(&ctx);
    let issuer = fixture.issuer(&ctx, limits());
    let caller = issuer
        .authenticate_runtime(&fixture.secret(), &ctx)
        .unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let run = issuer
        .start_run(&parent, &fixture.scope(), Duration::from_secs(60))
        .unwrap();
    for (role, permission, expected) in [
        (
            Role::Scout,
            Permission::Read(graphit_core::module_settings::Module::Hub),
            Obligation::Unconditional,
        ),
        (
            Role::Tracker,
            Permission::InspectGit,
            Obligation::Unconditional,
        ),
        (Role::Scribe, Permission::WriteOwnTask, Obligation::OwnActor),
    ] {
        let definition = managed(role, permission, 1);
        let reference = definition.reference().clone();
        assert!(
            matches!(reference.source(), RoleSource::ToolOwned(brand) if brand == &Brand::default())
        );
        assert!(
            issuer
                .issue(&parent, &run, &reference, &[permission], &fixture.scope())
                .is_err()
        );
        issuer.register_definition(&parent, definition).unwrap();
        let token = issuer
            .issue(&parent, &run, &reference, &[permission], &fixture.scope())
            .unwrap();
        let grant = issuer
            .verify(
                token.expose_for_transport(),
                &reference,
                fixture.scope(),
                &ctx,
            )
            .unwrap();
        assert_eq!(grant.authorize(permission), Ok(expected));
        assert_eq!(
            grant.authorize(Permission::Delegate),
            Err(AuthError::Denied)
        );
    }
    for role in [Role::Coordinator, Role::Dream] {
        assert!(matches!(
            RoleDefinition::tool_owned(
                role,
                &Brand::default(),
                NativeBinding::KiroV3,
                &[Permission::Read(graphit_core::module_settings::Module::Hub)],
                [1; 32],
                limits()
            ),
            Err(AuthError::Denied)
        ));
    }
    assert!(matches!(
        RoleDefinition::tool_owned(
            Role::Scout,
            &Brand::default(),
            NativeBinding::KiroV3,
            &[Permission::WriteTask],
            [1; 32],
            limits()
        ),
        Err(AuthError::Denied)
    ));
}

#[test]
fn replace_preserves_source_and_binding_and_revokes_previous_generation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let fixture = Fixture::new(&ctx);
    let issuer = fixture.issuer(&ctx, limits());
    let caller = issuer
        .authenticate_runtime(&fixture.secret(), &ctx)
        .unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let permission = Permission::Read(graphit_core::module_settings::Module::Hub);
    let definition = managed(Role::Scout, permission, 1);
    let reference = definition.reference().clone();
    issuer.register_definition(&parent, definition).unwrap();
    let run = issuer
        .start_run(&parent, &fixture.scope(), Duration::from_secs(60))
        .unwrap();
    let token = issuer
        .issue(&parent, &run, &reference, &[permission], &fixture.scope())
        .unwrap();
    assert!(
        issuer
            .verify(
                token.expose_for_transport(),
                &reference,
                fixture.scope(),
                &ctx
            )
            .is_ok()
    );
    let hub = fixture.definition(Role::Scout, &[permission], 2, limits());
    assert_eq!(
        issuer.replace_definition(&parent, &reference, hub),
        Err(AuthError::Denied)
    );
    let foreign = RoleDefinition::tool_owned(
        Role::Scout,
        &Brand::new("other").unwrap(),
        NativeBinding::KiroV3,
        &[permission],
        [2; 32],
        limits(),
    )
    .unwrap();
    assert_eq!(
        issuer.replace_definition(&parent, &reference, foreign),
        Err(AuthError::Denied)
    );
    let binding = RoleDefinition::tool_owned(
        Role::Scout,
        &Brand::default(),
        NativeBinding::Codex,
        &[permission],
        [2; 32],
        limits(),
    )
    .unwrap();
    assert_eq!(
        issuer.replace_definition(&parent, &reference, binding),
        Err(AuthError::Denied)
    );
    let next = managed(Role::Scout, permission, 2);
    let next_reference = next.reference().clone();
    issuer
        .replace_definition(&parent, &reference, next)
        .unwrap();
    assert!(
        issuer
            .verify(
                token.expose_for_transport(),
                &reference,
                fixture.scope(),
                &ctx
            )
            .is_err()
    );
    assert!(
        issuer
            .verify(
                token.expose_for_transport(),
                &next_reference,
                fixture.scope(),
                &ctx
            )
            .is_err()
    );
    let next_token = issuer
        .issue(
            &parent,
            &run,
            &next_reference,
            &[permission],
            &fixture.scope(),
        )
        .unwrap();
    assert!(
        issuer
            .verify(
                next_token.expose_for_transport(),
                &next_reference,
                fixture.scope(),
                &ctx
            )
            .is_ok()
    );
    cancel.cancel();
    assert_eq!(
        issuer.replace_definition(
            &parent,
            &next_reference,
            managed(Role::Scout, permission, 3)
        ),
        Err(AuthError::Request)
    );
}

#[test]
fn definition_data_does_not_bypass_administrative_policy() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let fixture = Fixture::new(&ctx);
    let mut administrative = all_permissions();
    administrative.retain(|permission| *permission != Permission::ManageAccessGrants);
    let issuer = RoleIssuer::new(
        fixture.realm(),
        Arc::clone(&fixture.runtime),
        fixture.auth_store(&ctx),
        authority(&all_permissions(), &administrative, limits(), 7200),
        ttl(),
        &ctx,
    )
    .unwrap();
    let caller = issuer
        .authenticate_runtime(&fixture.secret(), &ctx)
        .unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let permission = Permission::Read(graphit_core::module_settings::Module::Hub);
    assert_eq!(
        issuer.register_definition(&parent, managed(Role::Scout, permission, 1)),
        Err(AuthError::Denied)
    );
}

#[test]
fn administrative_effects_require_the_live_parent_policy() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let fixture = Fixture::new(&ctx);
    let issuer = RoleIssuer::new(
        fixture.realm(),
        Arc::clone(&fixture.runtime),
        fixture.auth_store(&ctx),
        authority(&[], &[Permission::Configure], limits(), 7200),
        ttl(),
        &ctx,
    )
    .unwrap();
    let caller = issuer
        .authenticate_runtime(&fixture.secret(), &ctx)
        .unwrap();
    assert_eq!(
        caller.authorize(Permission::Configure),
        Err(AuthError::Denied)
    );
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    assert_eq!(
        parent.authorize(Permission::Configure),
        Ok(Obligation::Unconditional)
    );
    assert_eq!(
        parent.authorize(Permission::ManageAccessGrants),
        Err(AuthError::Denied)
    );
    cancel.cancel();
    assert_eq!(
        parent.authorize(Permission::Configure),
        Err(AuthError::Request)
    );
}

#[test]
fn configure_managed_definition_is_idempotent_and_changed_policy_revokes() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let fixture = Fixture::new(&ctx);
    let issuer = fixture.issuer(&ctx, limits());
    let caller = issuer
        .authenticate_runtime(&fixture.secret(), &ctx)
        .unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let permission = Permission::Read(graphit_core::module_settings::Module::Hub);
    let reference = issuer
        .configure_tool_owned_definition(&parent, managed(Role::Scout, permission, 1))
        .unwrap();
    let run = issuer
        .start_run(&parent, &fixture.scope(), Duration::from_secs(60))
        .unwrap();
    let token = issuer
        .issue(&parent, &run, &reference, &[permission], &fixture.scope())
        .unwrap();
    assert_eq!(
        issuer
            .configure_tool_owned_definition(&parent, managed(Role::Scout, permission, 1))
            .unwrap(),
        reference
    );
    assert!(
        issuer
            .verify(
                token.expose_for_transport(),
                &reference,
                fixture.scope(),
                &ctx
            )
            .is_ok()
    );
    let next = issuer
        .configure_tool_owned_definition(&parent, managed(Role::Scout, permission, 2))
        .unwrap();
    assert_ne!(reference, next);
    assert!(
        issuer
            .verify(
                token.expose_for_transport(),
                &reference,
                fixture.scope(),
                &ctx
            )
            .is_err()
    );
    assert_eq!(
        issuer.configure_tool_owned_definition(
            &parent,
            fixture.definition(Role::Scout, &[permission], 3, limits())
        ),
        Err(AuthError::Denied)
    );
    cancel.cancel();
    assert_eq!(
        issuer.configure_tool_owned_definition(&parent, managed(Role::Scout, permission, 2)),
        Err(AuthError::Request)
    );
}
