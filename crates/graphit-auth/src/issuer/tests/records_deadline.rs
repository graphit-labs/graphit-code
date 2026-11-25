use super::records::{issue, read_denied, write_denied};
use super::*;
use crate::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::module_settings::Module;

#[test]
fn original_deadline_expires_without_replacement_request() {
    let cancel = Cancellation::default();
    let startup = request(&cancel);
    let f = Fixture::new(&startup);
    let issuer = f.issuer(&startup, limits());
    let ctx = request(&cancel);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteOwnTask],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let authenticated_read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
        .unwrap();
    let authenticated_write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::OwnTask)
        .unwrap();
    std::thread::sleep(
        ctx.deadline()
            .unwrap()
            .saturating_duration_since(Instant::now())
            + Duration::from_millis(1),
    );
    assert_eq!(read.recheck(), Err(AuthError::Request));
    assert_eq!(write.recheck(), Err(AuthError::Request));
    assert_eq!(authenticated_read.recheck(), Err(AuthError::Request));
    assert_eq!(authenticated_write.recheck(), Err(AuthError::Request));
    read_denied(&authenticated_read);
    write_denied(&authenticated_write);
    read_denied(&read);
    write_denied(&write);
}
