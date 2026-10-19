use super::*;
use crate::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::module_settings::Module;

fn dream(f: &Fixture, policy: u8) -> RoleDefinition {
    f.definition(
        Role::Dream,
        &[
            Permission::Read(Module::Memory),
            Permission::WriteFencedMemory,
        ],
        policy,
        limits(),
    )
}

#[test]
fn exact_reuse_preserves_grants_admissions_and_run_session_ownership() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let definition = dream(&f, 1);
    let reference = definition.reference().clone();
    let permissions = definition.permissions().to_vec();
    issuer
        .register_or_reuse_hub_definition(&parent, definition)
        .unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(60))
        .unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, &permissions, &f.scope())
        .unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::FencedMemory)
        .unwrap();

    let fresh_ctx = request(&cancel);
    let fresh_caller = issuer
        .authenticate_runtime(&f.secret(), &fresh_ctx)
        .unwrap();
    let fresh_session = issuer.bootstrap_admin(&fresh_caller).unwrap();
    let fresh_parent = issuer.admin_parent(&fresh_caller, &fresh_session).unwrap();
    issuer
        .register_or_reuse_hub_definition(&fresh_parent, dream(&f, 1))
        .unwrap();
    assert_eq!(issuer.lock().unwrap().profiles.len(), 1);
    assert_eq!(issuer.lock().unwrap().profiles[0].generation, 1);
    assert_eq!(read.recheck(), Ok(()));
    assert_eq!(write.recheck(), Ok(Obligation::MemoryFenceRequired));
    assert!(matches!(
        issuer.issue(&fresh_parent, &run, &reference, &permissions, &f.scope()),
        Err(AuthError::Denied)
    ));
    assert_eq!(
        issuer.register_definition(&parent, dream(&f, 1)),
        Err(AuthError::Invalid)
    );
    assert_eq!(
        issuer.register_or_reuse_hub_definition(&parent, dream(&f, 2)),
        Err(AuthError::Invalid)
    );
    assert_eq!(read.recheck(), Ok(()));
    assert_eq!(write.recheck(), Ok(Obligation::MemoryFenceRequired));
    assert_eq!(issuer.lock().unwrap().profiles[0].generation, 1);

    issuer
        .replace_definition(&parent, &reference, dream(&f, 1))
        .unwrap();
    assert_eq!(issuer.lock().unwrap().profiles[0].generation, 2);
    assert!(grant.authorize(permissions[0]).is_err());
    assert!(read.recheck().is_err());
    assert!(write.recheck().is_err());
}

#[test]
fn reuse_compares_all_private_fields_and_rejects_conflicting_strict_profiles() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    issuer.register_definition(&parent, dream(&f, 1)).unwrap();
    for field in ["binding", "permissions", "policy"] {
        let mut different = dream(&f, 1);
        match field {
            "binding" => different.binding = NativeBinding::Codex,
            "permissions" => different.permissions.pop().map(|_| ()).unwrap(),
            "policy" => different = dream(&f, 2),
            _ => unreachable!(),
        }
        assert_eq!(
            issuer.register_or_reuse_hub_definition(&parent, different),
            Err(AuthError::Invalid),
            "{field}"
        );
        let state = issuer.lock().unwrap();
        assert_eq!(state.profiles.len(), 1);
        assert_eq!(state.profiles[0].generation, 1);
        assert_eq!(
            state.profiles[0].definition.reference,
            dream(&f, 1).reference
        );
    }
    // Strict registration permits distinct policy references. Reuse must not
    // hide a conflicting profile merely because it finds an equal one first.
    issuer.register_definition(&parent, dream(&f, 2)).unwrap();
    assert_eq!(
        issuer.register_or_reuse_hub_definition(&parent, dream(&f, 1)),
        Err(AuthError::Invalid)
    );
    let state = issuer.lock().unwrap();
    assert_eq!(state.profiles.len(), 2);
    assert!(state.profiles.iter().all(|profile| profile.generation == 1));
}

#[test]
fn registration_and_reuse_require_management_and_permission_subset() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    for registered in [false, true] {
        for admin in [
            vec![
                Permission::Read(Module::Memory),
                Permission::WriteFencedMemory,
            ],
            vec![Permission::ManageAccessGrants],
        ] {
            let issuer = fixture_issuer(&f, &ctx, clock(), &[], &admin, limits(), 60);
            if registered {
                issuer.lock().unwrap().profiles.push(Profile {
                    definition: dream(&f, 1),
                    generation: 7,
                });
            }
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let session = issuer.bootstrap_admin(&caller).unwrap();
            let parent = issuer.admin_parent(&caller, &session).unwrap();
            assert_eq!(
                issuer.register_or_reuse_hub_definition(&parent, dream(&f, 1)),
                Err(AuthError::Denied)
            );
            let state = issuer.lock().unwrap();
            assert_eq!(state.profiles.len(), usize::from(registered));
            if registered {
                assert_eq!(state.profiles[0].generation, 7);
            }
        }
    }
}

#[test]
fn registration_and_reuse_reject_foreign_cancelled_expired_parents() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    for registered in [false, true] {
        for invalid in ["foreign", "cancelled", "session", "deadline"] {
            let clock = clock();
            let issuer = fixture_issuer(
                &f,
                &ctx,
                Arc::clone(&clock),
                &all_permissions(),
                &all_permissions(),
                limits(),
                60,
            );
            if registered {
                issuer.lock().unwrap().profiles.push(Profile {
                    definition: dream(&f, 1),
                    generation: 7,
                });
            }
            let foreign = f.issuer(&ctx, limits());
            let parent_issuer = if invalid == "foreign" {
                &foreign
            } else {
                &issuer
            };
            let parent_cancel = Cancellation::default();
            let deadline = Instant::now() + Duration::from_secs(1);
            let parent_ctx = request_before(&parent_cancel, deadline);
            let caller = parent_issuer
                .authenticate_runtime(&f.secret(), &parent_ctx)
                .unwrap();
            let session = parent_issuer.bootstrap_admin(&caller).unwrap();
            let parent = parent_issuer.admin_parent(&caller, &session).unwrap();
            match invalid {
                "cancelled" => parent_cancel.cancel(),
                "session" => clock.seconds.store(60, Ordering::SeqCst),
                "deadline" => {
                    std::thread::sleep(deadline.saturating_duration_since(Instant::now()))
                }
                _ => {}
            }
            assert!(
                issuer
                    .register_or_reuse_hub_definition(&parent, dream(&f, 1))
                    .is_err(),
                "{invalid}"
            );
            let state = issuer.lock().unwrap();
            assert_eq!(state.profiles.len(), usize::from(registered), "{invalid}");
            if registered {
                assert_eq!(state.profiles[0].generation, 7, "{invalid}");
            }
        }
    }
}

#[test]
fn hub_registration_rejects_tool_owned_sources_without_mutation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    for registered in [false, true] {
        if registered {
            issuer.register_definition(&parent, dream(&f, 1)).unwrap();
        }
        let definition = RoleDefinition::tool_owned(
            Role::Scout,
            &Brand::default(),
            NativeBinding::KiroV3,
            &[Permission::Read(Module::Hub)],
            [1; 32],
            limits(),
        )
        .unwrap();
        assert_eq!(
            issuer.register_or_reuse_hub_definition(&parent, definition),
            Err(AuthError::Denied)
        );
        assert_eq!(
            issuer.lock().unwrap().profiles.len(),
            usize::from(registered)
        );
    }
}

#[test]
fn concurrent_registration_or_reuse_is_atomic_for_equal_and_different_definitions() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    for policies in [[1, 1], [1, 2]] {
        let issuer = f.issuer(&ctx, limits());
        let barrier = std::sync::Barrier::new(2);
        let results = std::thread::scope(|threads| {
            let handles: Vec<_> = policies
                .into_iter()
                .map(|policy| {
                    let issuer = &issuer;
                    let barrier = &barrier;
                    let f = &f;
                    threads.spawn(move || {
                        let cancel = Cancellation::default();
                        let ctx = request(&cancel);
                        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
                        let session = issuer.bootstrap_admin(&caller).unwrap();
                        let parent = issuer.admin_parent(&caller, &session).unwrap();
                        let definition = dream(f, policy);
                        barrier.wait();
                        issuer.register_or_reuse_hub_definition(&parent, definition)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            results.iter().filter(|result| result.is_ok()).count(),
            if policies[0] == policies[1] { 2 } else { 1 }
        );
        assert!(
            results
                .iter()
                .all(|result| matches!(result, Ok(()) | Err(AuthError::Invalid)))
        );
        let state = issuer.lock().unwrap();
        assert_eq!(state.profiles.len(), 1);
        assert_eq!(state.profiles[0].generation, 1);
    }
}
