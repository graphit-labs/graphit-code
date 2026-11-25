use super::*;

#[test]
fn prepared_publication_keeps_actual_store_binding_without_effects_on_swap() {
    let cancellation = Cancellation::default();
    let context = context(&cancellation);
    let fixture = Fixture::new();
    let store = fixture.store(&context);
    let other_store = fixture.store(&context);
    let snapshot = store.load(&context).unwrap();
    let prepared = store
        .prepare(snapshot, &context, |doc, ctx| {
            doc.ensure_default_local(doc.revision(), 1, ctx).map(|_| ())
        })
        .unwrap();

    assert_eq!(
        error(other_store.commit(prepared)),
        AuthStoreError::Core(AuthDocumentError::Denied)
    );

    assert!(!fixture.root.join("auth.json").exists());
}

#[test]
fn fresh_request_publishes_expected_bytes_without_a_snapshot_request_proof() {
    let cancelled = Cancellation::default();
    let old = context(&cancelled);
    let fixture = Fixture::new();
    let store = fixture.store(&old);
    let snapshot = store.load(&old).unwrap();
    cancelled.cancel();
    let live = Cancellation::default();
    let fresh = context(&live);
    applied(
        store
            .mutate(snapshot, &fresh, |doc, ctx| {
                doc.ensure_default_local(doc.revision(), 1, ctx).map(|_| ())
            })
            .unwrap(),
    );
    assert_eq!(store.load(&fresh).unwrap().document().providers().len(), 1);
}
use crate::profile::{
    AiService, BrokerService, OidcTokens, ProfileClaimsUpdate, ProfileCredentials, ProfileData,
    ProfileExpectation, ProfileIdentityUpdate, ProviderConfig, ProviderKind, S3Location, S3Source,
    Secret, TokenExpectation, TokenUpdate,
};
use graphit_core::{
    auth_document::{AuthDocumentLimits, publication::AuthPublicationEffect},
    file_lock::Cancellation,
};
use graphit_hub::native::ContentLimits;
use secrecy::SecretString;
use std::{
    fs,
    path::{Path, PathBuf},
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
fn open_store(root: &Path, context: &RequestContext<'_>, policy: ProfileLimits) -> AuthStore {
    let physical = AuthDocumentStore::new(
        root.to_owned(),
        AuthDocumentLimits::new(CAP).unwrap(),
        context.snapshot(),
    )
    .unwrap();
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
        drop(graphit_fs::create_private_directory(&root).unwrap());
        Self { _temp: temp, root }
    }
    fn store(&self, context: &RequestContext<'_>) -> AuthStore {
        open_store(&self.root, context, limits())
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
fn applied(effect: AuthPublicationEffect) {
    assert_eq!(effect, AuthPublicationEffect::Applied);
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
    applied(
        store
            .physical
            .publish(old, body, context.snapshot())
            .unwrap(),
    );
}

#[test]
fn identity_and_rotating_tokens_are_published_in_one_atomic_transition() {
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
fn plain_path_absence_and_explicit_provider_only_publication() {
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
    assert_eq!(fixture.children(), 2); // auth.json and persistent publisher lock
    assert_eq!(
        store.publish(loaded, &ctx).unwrap(),
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
fn stale_content_cas_preserves_current_document() {
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    let ctx = context(&cancellation);
    let store = fixture.store(&ctx);
    let stale = store.load(&ctx).unwrap();
    applied(store.publish(store.load(&ctx).unwrap(), &ctx).unwrap());
    let current = fixture.bytes();
    assert_eq!(
        error(store.mutate(stale, &ctx, |doc, ctx| {
            doc.ensure_default_local(doc.revision(), 1, ctx).map(|_| ())
        })),
        AuthStoreError::Core(AuthDocumentError::Conflict)
    );
    assert_eq!(fixture.bytes(), current);
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
        let result = store.mutate(losing, &ctx, |doc, ctx| {
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
        });
        assert_eq!(
            error(result),
            AuthStoreError::Core(AuthDocumentError::Conflict)
        );
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
fn whole_document_capacity_rejects_growth_without_write() {
    let fixture = Fixture::new();
    let c = Cancellation::default();
    let ctx = context(&c);
    let store = fixture.store(&ctx);
    seed(&store, &ctx);
    let bytes = fixture.bytes();
    let mut policy = limits();
    policy.profiles = 2;
    let limited = open_store(&fixture.root, &ctx, policy);
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

#[cfg(unix)]
#[test]
fn configured_links_and_equal_body_replacement_keep_typed_profile_content() {
    use std::{io::Write, os::unix::fs::symlink};
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    let ctx = context(&cancellation);
    let store = fixture.store(&ctx);
    seed(&store, &ctx);
    let bytes = fixture.bytes();
    fs::hard_link(
        fixture.root.join("auth.json"),
        fixture.root.join("kept.json"),
    )
    .unwrap();
    let alias = fixture.root.with_extension("selected");
    symlink(&fixture.root, &alias).unwrap();
    let selected = open_store(&alias, &ctx, limits());
    let snapshot = selected.load(&ctx).unwrap();
    assert_eq!(snapshot.document().active_name(), Some("alice"));
    let mut replacement = tempfile::Builder::new()
        .make_in(&fixture.root, graphit_fs::create_private_file)
        .unwrap();
    replacement.write_all(&bytes).unwrap();
    replacement
        .into_temp_path()
        .persist(fixture.root.join("auth.json"))
        .unwrap();
    applied(
        selected
            .mutate(snapshot, &ctx, |doc, ctx| {
                doc.activate("bob", doc.revision(), ctx)
            })
            .unwrap(),
    );
    assert_eq!(
        selected.load(&ctx).unwrap().document().active_name(),
        Some("bob")
    );
    assert_eq!(fs::read(fixture.root.join("kept.json")).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn credential_privacy_is_checked_without_permission_repair() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    let ctx = context(&cancellation);
    let store = fixture.store(&ctx);
    seed(&store, &ctx);
    let bytes = fixture.bytes();
    let path = fixture.root.join("auth.json");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        error(store.load(&ctx)),
        AuthStoreError::Core(AuthDocumentError::Denied)
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
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
fn lifecycle_fence_current_content_keeps_birth_and_activation_incarnations() {
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
fn lifecycle_fence_activation_capacity_and_valid_maximum_preserve_content() {
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
        let limited = open_store(&fixture.root, &fresh, policy);
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
