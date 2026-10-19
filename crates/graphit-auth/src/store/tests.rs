use super::*;

#[test]
fn prepared_publication_keeps_actual_store_binding_without_effects_on_swap() {
    let cancellation = Cancellation::default();
    let context = context(&cancellation);
    let fixture = Fixture::new();
    let store = fixture.store(&context);
    let other_capture = fixture.store(&context);
    let snapshot = store.load(&context).unwrap();
    let prepared = store
        .prepare(snapshot, &context, |doc, ctx| {
            doc.ensure_default_local(doc.revision(), 1, ctx).map(|_| ())
        })
        .unwrap();

    assert_eq!(
        error(other_capture.commit(prepared)),
        AuthStoreError::Core(AuthDocumentError::Denied)
    );

    assert!(!fixture.root.join("auth.json").exists());
    assert!(!fixture.root.join(".auth.json.stage").exists());
}

#[test]
fn prepare_rejects_replacement_request_before_mutation() {
    let cancellation = Cancellation::default();
    let original = context(&cancellation);
    let replacement = context(&cancellation);
    let fixture = Fixture::new();
    let store = fixture.store(&original);
    let snapshot = store.load(&original).unwrap();
    let mut called = false;
    assert_eq!(
        error(store.prepare(snapshot, &replacement, |_, _| {
            called = true;
            Ok(())
        })),
        AuthStoreError::Core(AuthDocumentError::Conflict)
    );
    assert!(!called);

    assert!(!fixture.root.join("auth.json").exists());
}
use crate::profile::{
    AiService, BrokerService, OidcTokens, ProfileClaimsUpdate, ProfileCredentials, ProfileData,
    ProfileExpectation, ProfileIdentityUpdate, ProviderConfig, ProviderKind, S3Location, S3Source,
    Secret, TokenExpectation, TokenUpdate,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    auth_document::{
        AuthDocumentLimits, publication::AuthPublicationEffect, recovery::AuthCleanupEffect,
    },
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    project_paths::InstallationPaths,
};
use graphit_hub::native::ContentLimits;
use secrecy::SecretString;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const CAP: usize = 128 * 1024;
fn limits() -> ProfileLimits {
    ProfileLimits {
        bytes: CAP,
        nodes: 8192,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 32,
    }
}
fn context(cancellation: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: CAP,
            max_output_bytes: CAP,
            max_depth: 32,
            max_nodes: 8192,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}
fn capture(root: &Path, context: &RequestContext<'_>, policy: ProfileLimits) -> AuthStore {
    let paths = InstallationPaths::resolve(
        Brand::new("graphit").unwrap(),
        root.parent().unwrap(),
        None,
        Some(root),
    )
    .unwrap();
    let prepared = installation_bootstrap::prepare(
        &paths,
        BootstrapLimits::new(65536, 256).unwrap(),
        context.snapshot(),
    );
    assert_eq!(prepared.failure(), None);
    let private = prepared.into_installation().unwrap();
    private.revalidate(context.snapshot()).unwrap();
    let scope = CapturedPublicationScope::installation(
        private.paths(),
        InstallationId::new("auth-store-fixture").unwrap(),
        [root.to_path_buf()],
        ScopeLimits::new(8, 65536, 256).unwrap(),
        context.snapshot(),
    )
    .unwrap();
    let physical = AuthDocumentStore::capture(
        scope,
        AuthDocumentLimits::new(CAP, 65536, 256, 65536).unwrap(),
        context.snapshot(),
    )
    .unwrap();
    private.revalidate(context.snapshot()).unwrap();
    AuthStore::new(physical, policy, context).unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("installation");
        Self { _temp: temp, root }
    }
    fn store(&self, context: &RequestContext<'_>) -> AuthStore {
        capture(&self.root, context, limits())
    }
    fn bytes(&self) -> Vec<u8> {
        fs::read(self.root.join("auth.json")).unwrap()
    }
    fn children(&self) -> usize {
        fs::read_dir(&self.root).unwrap().count()
    }
}
fn error<T>(result: Result<T, AuthStoreError>) -> AuthStoreError {
    result.err().expect("operation must fail")
}
fn applied(publication: AuthDocumentPublication) {
    assert_eq!(publication.effect(), AuthPublicationEffect::Applied);
    assert_eq!(publication.failure(), None);
    assert_eq!(publication.cleanup_failure(), None);
    assert_eq!(publication.cleanup_effect(), AuthCleanupEffect::Absent);
    assert!(publication.into_pending_cleanup().is_none());
}
fn secret(value: &str) -> Secret {
    Secret::new(SecretString::from(value))
}
fn broker() -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Broker(BrokerService {
            endpoint: "https://broker.example".into(),
            mcp_resource: None,
            allow_anonymous: false,
        }),
        embedding: AiService::Broker,
        rerank: AiService::Broker,
        s3: S3Location {
            bucket: None,
            region: None,
            endpoint: None,
            prefix: None,
            source: S3Source::BrokerScope,
        },
    }
}
fn data(name: &str) -> ProfileData {
    ProfileData {
        name: name.into(),
        provider: "company".into(),
        username: name.into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Broker {
            issuer: "https://broker.example".into(),
            subject: name.into(),
            tokens: OidcTokens {
                access_token: secret("access-fixture"),
                refresh_token: Some(secret("refresh-fixture")),
                id_token: secret("id-fixture"),
                expires_at: 100,
            },
            s3_disabled: true,
        },
    }
}
fn add_profile(
    doc: &mut AuthDocument,
    name: &str,
    context: &RequestContext<'_>,
) -> Result<(), ProfileError> {
    doc.replace_profile_data(
        data(name),
        ProfileExpectation {
            document_revision: doc.revision(),
            provider_revision: doc.provider("company").unwrap().revision(),
            profile_revision: doc.profile(name).map(|profile| profile.revision()),
        },
        20,
        context,
    )
}
fn seed(store: &AuthStore, context: &RequestContext<'_>) {
    let empty = store.load(context).unwrap();
    applied(
        store
            .mutate(empty, context, |doc, context| {
                doc.add_provider("company".into(), broker(), doc.revision(), 10, context)?;
                add_profile(doc, "alice", context)?;
                add_profile(doc, "bob", context)?;
                doc.activate("alice", doc.revision(), context)
            })
            .unwrap(),
    );
}
fn rotate(doc: &mut AuthDocument, context: &RequestContext<'_>) -> Result<(), ProfileError> {
    let profile = doc.profile("alice").unwrap();
    let expected = TokenExpectation {
        document_revision: doc.revision(),
        provider_revision: doc.provider("company").unwrap().revision(),
        profile_revision: profile.revision(),
        token_revision: profile.token_revision(),
    };
    doc.rotate_tokens(
        "alice",
        TokenUpdate {
            access_token: secret("new-access-fixture"),
            refresh_token: secret("new-refresh-fixture"),
            identity: ProfileIdentityUpdate::Preserve,
            expires_at: 200,
        },
        expected,
        30,
        context,
    )
}
fn raw_publish(store: &AuthStore, body: &[u8], context: &RequestContext<'_>) {
    let old = store.physical.read(context.snapshot()).unwrap();
    applied(store.physical.publish(old, body, context.snapshot()));
}

#[test]
fn identity_and_rotating_tokens_are_published_in_one_physical_transition() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let ctx = context(&c);
    let store = fixture.store(&ctx);
    seed(&store, &ctx);
    let snapshot = store.load(&ctx).unwrap();
    let before = snapshot.document().profile("alice").unwrap();
    let expected = TokenExpectation {
        document_revision: snapshot.document().revision(),
        provider_revision: before.provider_revision(),
        profile_revision: before.revision(),
        token_revision: before.token_revision(),
    };
    applied(
        store
            .mutate(snapshot, &ctx, |doc, ctx| {
                doc.rotate_tokens(
                    "alice",
                    TokenUpdate {
                        access_token: secret("new-access-fixture"),
                        refresh_token: secret("new-refresh-fixture"),
                        identity: ProfileIdentityUpdate::Replace {
                            id_token: secret("new-id-fixture"),
                            claims: ProfileClaimsUpdate {
                                username: "renamed-alice".into(),
                                organization: Some("new-organization".into()),
                                teams: vec!["new-team".into()],
                            },
                        },
                        expires_at: 200,
                    },
                    expected,
                    30,
                    ctx,
                )
            })
            .unwrap(),
    );
    let persisted = store.load(&ctx).unwrap();
    let document = persisted.document();
    assert_eq!(document.revision(), expected.document_revision + 1);
    assert_eq!(document.active_name(), Some("alice"));
    let profile = document.profile("alice").unwrap();
    assert_eq!(profile.revision(), expected.profile_revision + 1);
    assert_eq!(profile.token_revision(), expected.token_revision + 1);
    assert_eq!(profile.provider_revision(), expected.provider_revision);
    assert_eq!(profile.created_at(), 20);
    assert_eq!(profile.updated_at(), 30);
    let data = profile.data().expect("Code fixture");
    assert_eq!(data.name, "alice");
    assert_eq!(data.provider, "company");
    assert_eq!(data.username, "renamed-alice");
    assert_eq!(data.organization.as_deref(), Some("new-organization"));
    assert_eq!(data.teams, ["new-team"]);
    let ProfileCredentials::Broker {
        issuer,
        subject,
        tokens,
        s3_disabled,
    } = &data.credentials
    else {
        panic!()
    };
    assert_eq!(issuer, "https://broker.example");
    assert_eq!(subject, "alice");
    assert!(*s3_disabled);
    assert_eq!(tokens.access_token.expose(), "new-access-fixture");
    assert_eq!(
        tokens.refresh_token.as_ref().unwrap().expose(),
        "new-refresh-fixture"
    );
    assert_eq!(tokens.id_token.expose(), "new-id-fixture");
    assert_eq!(tokens.expires_at, 200);
    assert_eq!(fixture.children(), 2);
}

#[test]
fn sdk_bootstrap_absence_and_explicit_provider_only_publication() {
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    let ctx = context(&cancellation);
    let store = fixture.store(&ctx);
    let empty = store.load(&ctx).unwrap();
    assert!(!empty.was_present());
    assert_eq!(empty.document().revision(), 1);
    assert!(empty.document().providers().is_empty());
    assert!(empty.document().profiles().is_empty());
    assert_eq!(empty.document().active_name(), None);
    assert_eq!(fixture.children(), 0);

    applied(
        store
            .mutate(empty, &ctx, |doc, ctx| {
                assert!(doc.ensure_default_local(doc.revision(), 10, ctx)?);
                Ok(())
            })
            .unwrap(),
    );
    let loaded = store.load(&ctx).unwrap();
    assert!(loaded.was_present());
    assert_eq!(loaded.document().providers().len(), 1);
    assert!(loaded.document().profiles().is_empty());
    assert_eq!(loaded.document().active_name(), None);
    assert_eq!(fixture.children(), 2); // auth.json and persistent bounded lock
    assert_eq!(
        store.publish(loaded, &ctx).unwrap().effect(),
        AuthPublicationEffect::Unchanged
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in ["auth.json", ".auth.json.lock"] {
            assert_eq!(
                fs::metadata(fixture.root.join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        assert_eq!(
            fs::metadata(&fixture.root).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}

#[test]
fn explicit_empty_publication_has_no_provider_or_login() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let ctx = context(&c);
    let store = fixture.store(&ctx);
    applied(store.publish(store.load(&ctx).unwrap(), &ctx).unwrap());
    let doc = store.load(&ctx).unwrap();
    assert!(doc.document().providers().is_empty());
    assert!(doc.document().profiles().is_empty());
}

#[test]
fn malformed_existing_document_is_denied_without_mutation() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let seed_ctx = context(&c);
    let store = fixture.store(&seed_ctx);
    raw_publish(&store, b"{", &seed_ctx);
    let ctx = context(&c);
    assert_eq!(
        error(fixture.store(&ctx).load(&ctx)),
        AuthStoreError::Profile(ProfileError::InvalidDocument)
    );

    assert_eq!(fixture.bytes(), b"{");
}

#[test]
fn invalid_policy_cancelled_and_deadline_requests_have_no_auth_effect() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let ctx = context(&c);
    let physical = fixture.store(&ctx).physical;
    let mut invalid = limits();
    invalid.profiles = 0;
    assert_eq!(
        error(AuthStore::new(physical, invalid, &ctx)),
        AuthStoreError::Profile(ProfileError::InvalidPolicy)
    );
    let store = fixture.store(&ctx);
    c.cancel();
    assert_eq!(
        error(store.load(&ctx)),
        AuthStoreError::Core(AuthDocumentError::Cancelled)
    );
    let c = Cancellation::default();
    let ctx = context(&c);
    let store = fixture.store(&ctx);
    let expired = RequestContext::new(
        ContentLimits {
            max_input_bytes: CAP,
            max_output_bytes: CAP,
            max_depth: 32,
            max_nodes: 8192,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        &c,
        Instant::now() + Duration::from_millis(20),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        error(store.load(&expired)),
        AuthStoreError::Core(AuthDocumentError::Deadline)
    );
    assert_eq!(fixture.children(), 0);
}

#[test]
fn fresh_request_and_other_store_are_denied_before_closure() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let ctx = context(&c);
    let store = fixture.store(&ctx);
    for different_request in [false, true] {
        let snapshot = store.load(&ctx).unwrap();
        let other = fixture.store(&ctx);
        let fresh = context(&c);
        let (target, request) = if different_request {
            (&store, &fresh)
        } else {
            (&other, &ctx)
        };
        let mut called = false;
        assert_eq!(
            error(target.mutate(snapshot, request, |_, _| {
                called = true;
                Ok(())
            })),
            AuthStoreError::Core(AuthDocumentError::Conflict)
        );
        assert!(!called);
    }
    assert_eq!(fixture.children(), 0);
}

#[test]
fn stale_before_mutation_is_denied_without_invoking_closure() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let ctx = context(&c);
    let store = fixture.store(&ctx);
    let stale = store.load(&ctx).unwrap();
    applied(store.publish(store.load(&ctx).unwrap(), &ctx).unwrap());
    let mut called = false;
    assert_eq!(
        error(store.mutate(stale, &ctx, |_, _| {
            called = true;
            Ok(())
        })),
        AuthStoreError::Core(AuthDocumentError::Conflict)
    );
    assert!(!called);
}

#[test]
fn competing_account_token_provider_and_metadata_publications_keep_winner_bytes() {
    for operation in 0..4 {
        let fixture = Fixture::new();
        let c = Cancellation::default();
        let ctx = context(&c);
        let store = fixture.store(&ctx);
        seed(&store, &ctx);
        let losing = store.load(&ctx).unwrap();
        let winning = store.load(&ctx).unwrap();
        let mut winner_bytes = Vec::new();
        let result = store
            .mutate(losing, &ctx, |doc, ctx| {
                // Deterministically interleave an independent publication after the
                // outer precheck, before its final Core comparison under the lock.
                applied(
                    store
                        .mutate(winning, ctx, |winner, ctx| match operation {
                            0 => winner.activate("bob", winner.revision(), ctx),
                            1 => rotate(winner, ctx),
                            2 => winner.update_provider(
                                "company",
                                broker(),
                                winner.revision(),
                                winner.provider("company").unwrap().revision(),
                                40,
                                ctx,
                            ),
                            _ => add_profile(winner, "alice", ctx),
                        })
                        .unwrap(),
                );
                winner_bytes = fixture.bytes();
                doc.logout(None, doc.revision(), ctx)
            })
            .unwrap();
        assert_eq!(result.effect(), AuthPublicationEffect::NotApplied);
        // Atomic replacement invalidates the retained observation without
        // accepting the previous inode or overwriting the winning publication.
        assert_eq!(result.failure(), Some(AuthDocumentError::Conflict));
        assert_eq!(result.cleanup_failure(), None);
        assert_eq!(fixture.bytes(), winner_bytes);
        assert_eq!(fixture.children(), 2);
    }
}

#[test]
fn all_four_logical_revisions_and_closure_errors_fail_before_publication() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let ctx = context(&c);
    let store = fixture.store(&ctx);
    seed(&store, &ctx);
    let bytes = fixture.bytes();
    for wrong in 0..4 {
        let snapshot = store.load(&ctx).unwrap();

        let result = store.mutate(snapshot, &ctx, |doc, ctx| {
            let profile = doc.profile("alice").unwrap();
            let mut expected = TokenExpectation {
                document_revision: doc.revision(),
                provider_revision: profile.provider_revision(),
                profile_revision: profile.revision(),
                token_revision: profile.token_revision(),
            };
            match wrong {
                0 => expected.document_revision += 1,
                1 => expected.provider_revision += 1,
                2 => expected.profile_revision += 1,
                _ => expected.token_revision += 1,
            }
            doc.rotate_tokens(
                "alice",
                TokenUpdate {
                    access_token: secret("changed"),
                    refresh_token: secret("changed-refresh"),
                    identity: ProfileIdentityUpdate::Replace {
                        id_token: secret("changed-id"),
                        claims: ProfileClaimsUpdate {
                            username: "changed-name".into(),
                            organization: Some("changed-organization".into()),
                            teams: vec!["changed-team".into()],
                        },
                    },
                    expires_at: 200,
                },
                expected,
                30,
                ctx,
            )
        });
        assert_eq!(
            error(result),
            AuthStoreError::Profile(if wrong == 1 {
                ProfileError::Stale
            } else {
                ProfileError::Conflict
            })
        );

        assert_eq!(fixture.bytes(), bytes);
    }
    let snapshot = store.load(&ctx).unwrap();
    assert_eq!(
        error(store.mutate(snapshot, &ctx, |_, _| Err(ProfileError::InvalidDocument))),
        AuthStoreError::Profile(ProfileError::InvalidDocument)
    );
    assert_eq!(fixture.bytes(), bytes);
    assert_eq!(fixture.children(), 2);
}

#[test]
fn whole_document_capacity_rejects_growth_without_write_or_stage() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let ctx = context(&c);
    let store = fixture.store(&ctx);
    seed(&store, &ctx);
    let bytes = fixture.bytes();
    let mut policy = limits();
    policy.profiles = 2;
    let limited = capture(&fixture.root, &ctx, policy);
    let snapshot = limited.load(&ctx).unwrap();

    assert_eq!(
        error(limited.mutate(snapshot, &ctx, |doc, ctx| add_profile(doc, "charlie", ctx))),
        AuthStoreError::Profile(ProfileError::Limit)
    );

    assert_eq!(fixture.bytes(), bytes);
    assert_eq!(fixture.children(), 2);
}

#[test]
fn cancellation_after_precheck_aborts_encoder_before_any_publication() {
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    let ctx = context(&cancellation);
    let store = fixture.store(&ctx);
    let snapshot = store.load(&ctx).unwrap();
    let result = store.mutate(snapshot, &ctx, |_, _| {
        cancellation.cancel();
        Ok(())
    });
    assert_eq!(
        error(result),
        AuthStoreError::Profile(ProfileError::Cancelled)
    );

    assert_eq!(fixture.children(), 0);
}

#[test]
fn occupied_stage_requires_explicit_discard_and_never_adopts_contents() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let ctx = context(&c);
    let store = fixture.store(&ctx);
    let empty = store.load(&ctx).unwrap();
    // Manufacture only a negative fixture, not a birth/ownership proof.
    let stage = fixture.root.join(".auth.json.stage");
    fs::write(&stage, b"not JSON and never adopted").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stage, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let blocked = store.publish(empty, &ctx).unwrap();
    assert_eq!(blocked.effect(), AuthPublicationEffect::NotApplied);
    assert_eq!(blocked.failure(), Some(AuthDocumentError::Conflict));
    assert_eq!(fs::read(&stage).unwrap(), b"not JSON and never adopted");
    assert!(!fixture.root.join("auth.json").exists());
    let recovered = store.recover_stage(&ctx);
    assert_eq!(recovered.effect(), AuthCleanupEffect::Applied);
    assert_eq!(recovered.failure(), None);
    assert!(recovered.into_pending_cleanup().is_none());
    assert!(!stage.exists());
    applied(store.publish(store.load(&ctx).unwrap(), &ctx).unwrap());
    assert_eq!(
        store.recover_stage(&ctx).effect(),
        AuthCleanupEffect::Absent
    );
}

#[cfg(unix)]
#[test]
fn replacement_restored_mtime_parent_privacy_and_hardlinks_are_denied() {
    use std::os::unix::fs::PermissionsExt;
    for attack in 0..5 {
        let fixture = Fixture::new();
        let c = Cancellation::default();
        let ctx = context(&c);
        let store = fixture.store(&ctx);
        applied(store.publish(store.load(&ctx).unwrap(), &ctx).unwrap());
        let snapshot = store.load(&ctx).unwrap();
        let auth = fixture.root.join("auth.json");
        let bytes = fixture.bytes();
        match attack {
            0 => {
                let replacement = fixture.root.join("replacement");
                fs::write(&replacement, &bytes).unwrap();
                fs::set_permissions(&replacement, fs::Permissions::from_mode(0o600)).unwrap();
                fs::rename(replacement, &auth).unwrap();
            }
            1 => {
                let modified = fs::metadata(&auth).unwrap().modified().unwrap();
                let mut changed = bytes.clone();
                changed[0] = b' ';
                fs::write(&auth, changed).unwrap();
                fs::File::options()
                    .write(true)
                    .open(&auth)
                    .unwrap()
                    .set_times(fs::FileTimes::new().set_modified(modified))
                    .unwrap();
            }
            2 => fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o755)).unwrap(),
            3 => fs::hard_link(&auth, fixture.root.join("foreign-link")).unwrap(),
            _ => {
                let moved = fixture.root.with_extension("old");
                fs::rename(&fixture.root, moved).unwrap();
                fs::create_dir(&fixture.root).unwrap();
                fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let mut called = false;
        let failure = error(store.mutate(snapshot, &ctx, |_, _| {
            called = true;
            Ok(())
        }));
        assert!(matches!(
            failure,
            AuthStoreError::Core(AuthDocumentError::Conflict | AuthDocumentError::Denied)
        ));
        assert!(!called);
        assert!(!fixture.root.join(".auth.json.stage").exists());
    }
}

#[cfg(unix)]
#[test]
fn recovery_preserves_foreign_links_public_or_oversized_stage_and_auth_bytes() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for attack in 0..4 {
        let fixture = Fixture::new();
        let c = Cancellation::default();
        let ctx = context(&c);
        let store = fixture.store(&ctx);
        applied(store.publish(store.load(&ctx).unwrap(), &ctx).unwrap());
        let bytes = fixture.bytes();
        let stage = fixture.root.join(".auth.json.stage");
        match attack {
            0 => symlink(fixture.root.join("auth.json"), &stage).unwrap(),
            1 => fs::hard_link(fixture.root.join("auth.json"), &stage).unwrap(),
            _ => {
                fs::write(
                    &stage,
                    if attack == 2 {
                        vec![0; 1]
                    } else {
                        vec![0; CAP + 1]
                    },
                )
                .unwrap();
                fs::set_permissions(
                    &stage,
                    fs::Permissions::from_mode(if attack == 2 { 0o644 } else { 0o600 }),
                )
                .unwrap();
            }
        }
        let recovery = store.recover_stage(&ctx);
        assert_eq!(recovery.effect(), AuthCleanupEffect::NotApplied);
        assert!(recovery.failure().is_some());
        assert!(fs::symlink_metadata(&stage).is_ok());
        assert_eq!(fixture.bytes(), bytes);
    }
}

#[test]
fn process_death_after_typed_snapshot_before_publish_preserves_old_document() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let ctx = context(&c);
    let store = fixture.store(&ctx);
    seed(&store, &ctx);
    let old = fixture.bytes();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "store::tests::crash_before_publish_child",
            "--ignored",
            "--nocapture",
        ])
        .env("GRAPHIT_AUTH_STORE_CRASH_ROOT", &fixture.root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("bounded AuthStore crash fixture exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(status.code(), Some(23));
    assert_eq!(fixture.bytes(), old);
    assert_eq!(fixture.children(), 2);
    let fresh = context(&c);
    let restarted = fixture.store(&fresh);
    assert_eq!(
        restarted.load(&fresh).unwrap().document().active_name(),
        Some("alice")
    );
    assert_eq!(
        restarted.recover_stage(&fresh).effect(),
        AuthCleanupEffect::Absent
    );
}
#[test]
#[ignore = "bounded parent fixture invokes this child"]
fn crash_before_publish_child() {
    let root = PathBuf::from(
        std::env::var_os("GRAPHIT_AUTH_STORE_CRASH_ROOT").expect("child fixture root"),
    );
    let c = Cancellation::default();
    let ctx = context(&c);
    let store = capture(&root, &ctx, limits());
    let snapshot = store.load(&ctx).unwrap();
    assert_eq!(snapshot.document().active_name(), Some("alice"));
    std::process::exit(23);
}

fn lifecycle_binding(doc: &AuthDocument, name: &str) -> (u64, u64, u64, u64) {
    let p = doc.profile(name).unwrap();
    (
        doc.provider(p.provider()).unwrap().revision(),
        p.revision(),
        p.token_revision(),
        p.provider_revision(),
    )
}

#[test]
fn lifecycle_fence_fresh_physical_snapshots_keep_birth_and_activation_incarnations() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let initial_ctx = context(&c);
    let initial_store = fixture.store(&initial_ctx);
    seed(&initial_store, &initial_ctx);
    let stale = initial_store.load(&initial_ctx).unwrap();
    let original = lifecycle_binding(stale.document(), "alice");
    let original_revision = stale.document().revision();
    for active in ["bob", "alice"] {
        let fresh = context(&c);
        let store = fixture.store(&fresh);
        applied(
            store
                .mutate(store.load(&fresh).unwrap(), &fresh, |doc, ctx| {
                    doc.activate(active, doc.revision(), ctx)
                })
                .unwrap(),
        );
    }
    let fresh = context(&c);
    let store = fixture.store(&fresh);
    let returned = store.load(&fresh).unwrap();
    assert_eq!(
        lifecycle_binding(returned.document(), "alice"),
        (original.0, original.1 + 1, original.2, original.3)
    );
    assert_eq!(
        returned.document().profile("alice").unwrap().updated_at(),
        20
    );
    let winner = fixture.bytes();

    assert!(matches!(
        error(initial_store.publish(stale, &initial_ctx)),
        AuthStoreError::Core(AuthDocumentError::Conflict | AuthDocumentError::Denied)
    ));

    assert_eq!(fixture.bytes(), winner);
    applied(
        store
            .mutate(returned, &fresh, |doc, ctx| add_profile(doc, "bob", ctx))
            .unwrap(),
    );
    let after = store.load(&fresh).unwrap();
    let own = lifecycle_binding(after.document(), "alice");
    assert_eq!(own, (original.0, original.1 + 1, original.2, original.3));
    let bytes = fixture.bytes();

    assert_eq!(
        error(store.mutate(after, &fresh, |doc, ctx| doc.activate(
            "alice",
            original_revision,
            ctx
        ))),
        AuthStoreError::Profile(ProfileError::Conflict)
    );

    assert_eq!(fixture.bytes(), bytes);
    for recreate_provider in [false, true] {
        let ctx = context(&c);
        let store = fixture.store(&ctx);
        let old = store.load(&ctx).unwrap();
        let old_binding = lifecycle_binding(old.document(), "alice");
        applied(
            store
                .mutate(old, &ctx, |doc, ctx| {
                    if recreate_provider {
                        doc.remove_provider("company", true, doc.revision(), ctx)?;
                        doc.add_provider("company".into(), broker(), doc.revision(), 10, ctx)?;
                        assert_eq!(doc.provider("company").unwrap().revision(), doc.revision());
                    } else {
                        doc.logout(Some("alice"), doc.revision(), ctx)?;
                    }
                    add_profile(doc, "alice", ctx)?;
                    let p = doc.profile("alice").unwrap();
                    assert_eq!(
                        (p.revision(), p.token_revision()),
                        (doc.revision(), doc.revision())
                    );
                    doc.activate("alice", doc.revision(), ctx)
                })
                .unwrap(),
        );
        let fresh = context(&c);
        let recaptured = fixture.store(&fresh);
        let loaded = recaptured.load(&fresh).unwrap();
        let new_binding = lifecycle_binding(loaded.document(), "alice");
        assert!(new_binding.1 > old_binding.1 && new_binding.2 > old_binding.2);
        assert_eq!(
            loaded
                .document()
                .profile("alice")
                .unwrap()
                .data()
                .expect("Code fixture")
                .username,
            "alice"
        );
        if recreate_provider {
            assert!(new_binding.0 > old_binding.0);
        } else {
            assert_eq!(new_binding.0, old_binding.0);
        }

        let bytes = fixture.bytes();
        assert_eq!(
            error(
                recaptured.mutate(loaded, &fresh, |doc, ctx| doc.rotate_tokens(
                    "alice",
                    TokenUpdate {
                        access_token: secret("new-access"),
                        refresh_token: secret("new-refresh"),
                        identity: ProfileIdentityUpdate::Preserve,
                        expires_at: 200
                    },
                    TokenExpectation {
                        document_revision: doc.revision(),
                        provider_revision: old_binding.0,
                        profile_revision: old_binding.1,
                        token_revision: old_binding.2
                    },
                    30,
                    ctx
                ))
            ),
            AuthStoreError::Profile(if recreate_provider {
                ProfileError::Stale
            } else {
                ProfileError::Conflict
            })
        );

        assert_eq!(fixture.bytes(), bytes);
        assert_eq!(fixture.children(), 2);
    }
}

#[test]
fn lifecycle_fence_activation_capacity_and_valid_maximum_never_publish_or_stage() {
    for maximum in [false, true] {
        let fixture = Fixture::new();
        let c = Cancellation::default();
        let ctx = context(&c);
        let store = fixture.store(&ctx);
        seed(&store, &ctx);
        let mut v: serde_json::Value = serde_json::from_slice(&fixture.bytes()).unwrap();
        v["document_revision"] = serde_json::json!(if maximum { u64::MAX } else { 9 });
        if maximum {
            for p in v["providers"].as_array_mut().unwrap() {
                p["revision"] = serde_json::json!(u64::MAX);
            }
            for p in v["profiles"].as_array_mut().unwrap() {
                for field in ["revision", "token_revision", "provider_revision"] {
                    p[field] = serde_json::json!(u64::MAX);
                }
            }
        } else {
            v["active_profile"] = serde_json::json!("bob");
            v["profiles"][0]["revision"] = serde_json::json!(9);
        }
        let body = serde_json::to_vec(&v).unwrap();
        let mut projected = AuthDocument::decode(&body, limits(), &ctx).unwrap();
        let mut policy = limits();
        if !maximum {
            projected.activate("alice", 9, &ctx).unwrap();
            policy.bytes = projected.encode(&ctx).unwrap().len() - 1;
            assert!(body.len() <= policy.bytes);
        }
        raw_publish(&store, &body, &ctx);
        let bytes = fixture.bytes();
        let fresh = context(&c);
        let limited = capture(&fixture.root, &fresh, policy);
        let snapshot = limited.load(&fresh).unwrap();

        assert_eq!(
            error(limited.mutate(snapshot, &fresh, |doc, ctx| doc.activate(
                "alice",
                doc.revision(),
                ctx
            ))),
            AuthStoreError::Profile(ProfileError::Limit)
        );

        assert_eq!(fixture.bytes(), bytes);
        assert_eq!(fixture.children(), 2);
    }
}
