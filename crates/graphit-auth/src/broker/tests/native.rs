use super::*;

fn access(fixture: &Fixture) -> Value {
    let mut claims = fixture.claims();
    claims["client_id"] =
        fixture.state.broker.lock().unwrap()["authentication"]["client_id"].clone();
    claims["aud"] = json!([
        fixture.state.broker.lock().unwrap()["authentication"]["access_token_audience"],
        fixture.resource
    ]);
    claims
}
fn id(fixture: &Fixture) -> Value {
    let mut claims = access(fixture);
    let values = claims.as_object_mut().unwrap();
    values.remove("token_use");
    values.remove("client_id");
    values.remove("jti");
    claims["aud"] = fixture.state.broker.lock().unwrap()["authentication"]["client_id"].clone();
    claims["nonce"] = json!("nonce-fixture");
    claims["exp"] = json!(claims["iat"].as_u64().unwrap() + 900);
    claims
}
fn deny<T>(result: Result<T, BrokerError>) -> BrokerError {
    result.err().expect("native verification must fail")
}

#[tokio::test]
async fn access_identity_attributes_remain_token_bound_without_profile_requests() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    *fixture.state.userinfo_status.lock().unwrap() = 503;
    let mut verifier = fixture.verifier(limits());
    let start = Moment {
        utc: SystemTime::now(),
        mono: Instant::now(),
    };
    let clock = Arc::new(TestClock(Mutex::new(start)));
    verifier.clock = clock.clone();
    let cancel = Cancellation::default();
    let original = context(&cancel);
    let ctx = BrokerRequestContext::new(&original);
    let issued = start.utc.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let mut claims = access(&fixture);
    claims["iat"] = json!(issued);
    claims["exp"] = json!(issued + 60);
    claims["preferred_username"] = json!("access-user");
    claims["groups"] = json!(["access-team"]);
    let raw = token(&key, "one", &claims);
    let mut id_claims = id(&fixture);
    id_claims["preferred_username"] = json!("id-profile-user");
    id_claims["groups"] = json!(["id-profile-team"]);
    let raw_id = token(&key, "one", &id_claims);
    let retained = verifier
        .verify_login(&raw_id, &raw, "nonce-fixture", ctx)
        .await
        .unwrap();
    assert_eq!(retained.username(), Some("access-user"));
    assert_eq!(retained.teams(), &["access-team"]);
    assert_eq!(
        retained.profile_claims().unwrap().username(),
        "id-profile-user"
    );
    let counts = fixture.state.counts.lock().unwrap().clone();
    claims["preferred_username"] = json!("changed-user");
    claims["groups"] = json!(["changed-team"]);
    let changed_raw = token(&key, "one", &claims);
    let changed = verifier
        .verify_refresh(&changed_raw, None, "stable-subject", ctx)
        .await
        .unwrap();
    assert_eq!(changed.username(), Some("changed-user"));
    assert_eq!(changed.teams(), &["changed-team"]);
    retained.check(&verifier, ctx).unwrap();
    assert_eq!(retained.username(), Some("access-user"));
    assert_eq!(retained.teams(), &["access-team"]);
    assert_eq!(*fixture.state.counts.lock().unwrap(), counts);
    *clock.0.lock().unwrap() = Moment {
        utc: start.utc + Duration::from_secs(61),
        mono: start.mono + Duration::from_secs(61),
    };
    assert_eq!(
        retained.check(&verifier, ctx),
        Err(BrokerError::InvalidToken)
    );
    assert_eq!(
        changed.check(&verifier, ctx),
        Err(BrokerError::InvalidToken)
    );
    assert_eq!(*fixture.state.counts.lock().unwrap(), counts);
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn native_nonce_and_subject_exact_scalar_bounds_are_admitted_before_fetch() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    let c = Cancellation::default();
    let original = context(&c);
    let ctx = BrokerRequestContext::new(&original);
    let nonce = "x".repeat(limits().json.scalar_bytes);
    let mut id_claims = id(&fixture);
    id_claims["nonce"] = json!(nonce);
    let raw_id = token(&key, "one", &id_claims);
    let raw_access = token(&key, "one", &access(&fixture));
    let verifier = fixture.verifier(limits());
    verifier
        .verify_login(&raw_id, &raw_access, &nonce, ctx)
        .await
        .unwrap();
    let mut small = limits();
    small.json.scalar_bytes -= 1;
    let bounded = fixture.verifier(small);
    let counts = fixture.state.counts.lock().unwrap().clone();
    assert_eq!(
        deny(
            bounded
                .verify_login(&raw_id, &raw_access, &nonce, ctx)
                .await
        ),
        BrokerError::Limit
    );
    assert_eq!(*fixture.state.counts.lock().unwrap(), counts);
    let subject = "s".repeat(limits().json.scalar_bytes);
    let mut access_claims = access(&fixture);
    access_claims["sub"] = json!(subject);
    let raw_access = token(&key, "one", &access_claims);
    verifier
        .verify_refresh(&raw_access, None, &subject, ctx)
        .await
        .unwrap();
    assert_eq!(
        deny(
            bounded
                .verify_refresh(&raw_access, None, &subject, ctx)
                .await
        ),
        BrokerError::Limit
    );
    assert_eq!(*fixture.state.counts.lock().unwrap(), counts);
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn native_failed_cancelled_or_dropped_reload_consumes_cooldown_and_releases_fetch() {
    let old = pair();
    let new = pair();
    for action in 0..3 {
        let fixture = Fixture::new(&old, "old").await;
        let mut policy = limits();
        policy.unknown_key_cooldown = Duration::from_secs(10);
        let verifier = Arc::new(fixture.verifier(policy));
        let c = Cancellation::default();
        let original = context(&c);
        let ctx = BrokerRequestContext::new(&original);
        verifier.metadata(ctx).await.unwrap();
        *fixture.state.keys.lock().unwrap() = key_set(&new, "new");
        let raw_id = token(&new, "new", &id(&fixture));
        let raw_access = token(&new, "new", &access(&fixture));
        if action == 0 {
            *fixture.state.jwks_status.lock().unwrap() = 500;
            assert_eq!(
                deny(
                    verifier
                        .verify_login(&raw_id, &raw_access, "nonce-fixture", ctx)
                        .await
                ),
                BrokerError::InvalidMetadata
            );
        } else {
            *fixture.state.jwks_delay.lock().unwrap() = Duration::from_secs(2);
            let child_cancel = Cancellation::default();
            let captured_cancel = child_cancel.clone();
            let captured_verifier = verifier.clone();
            let child_id = raw_id.clone();
            let child_access = raw_access.clone();
            let pending = tokio::spawn(async move {
                let original = context(&captured_cancel);
                captured_verifier
                    .verify_login(
                        &child_id,
                        &child_access,
                        "nonce-fixture",
                        BrokerRequestContext::new(&original),
                    )
                    .await
                    .map(|_| ())
            });
            fixture.wait_for("/keys", 2).await;
            if action == 1 {
                child_cancel.cancel();
                assert_eq!(pending.await.unwrap(), Err(BrokerError::Cancelled));
            } else {
                pending.abort();
                assert!(pending.await.unwrap_err().is_cancelled());
            }
        }
        *fixture.state.jwks_delay.lock().unwrap() = Duration::ZERO;
        *fixture.state.jwks_status.lock().unwrap() = 200;
        assert_eq!(
            deny(
                verifier
                    .verify_login(&raw_id, &raw_access, "nonce-fixture", ctx)
                    .await
            ),
            BrokerError::InvalidToken
        );
        assert_eq!(fixture.count("/keys"), 2);
        assert_eq!(fixture.count("/userinfo"), 0);
    }
}

#[tokio::test]
async fn expiry_during_native_discovery_cannot_create_retained_proof_after_utc_rollback() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    *fixture.state.jwks_delay.lock().unwrap() = Duration::from_millis(40);
    let mut verifier = fixture.verifier(limits());
    let start = Moment {
        utc: SystemTime::now(),
        mono: Instant::now(),
    };
    let clock = Arc::new(TestClock(Mutex::new(start)));
    verifier.clock = clock.clone();
    let verifier = Arc::new(verifier);
    let issued = start.utc.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let mut id_claims = id(&fixture);
    id_claims["iat"] = json!(issued);
    id_claims["exp"] = json!(issued + 10);
    let raw_id = token(&key, "one", &id_claims);
    let mut access_claims = access(&fixture);
    access_claims["iat"] = json!(issued);
    let raw_access = token(&key, "one", &access_claims);
    let captured = verifier.clone();
    let pending = tokio::spawn(async move {
        let c = Cancellation::default();
        let original = context(&c);
        captured
            .verify_login(
                &raw_id,
                &raw_access,
                "nonce-fixture",
                BrokerRequestContext::new(&original),
            )
            .await
            .map(|_| ())
    });
    fixture.wait_for("/keys", 1).await;
    *clock.0.lock().unwrap() = Moment {
        utc: start.utc - Duration::from_secs(100),
        mono: start.mono + Duration::from_secs(20),
    };
    assert_eq!(pending.await.unwrap(), Err(BrokerError::InvalidToken));
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn api_only_variable_metadata_login_refresh_and_mcp_denial() {
    let key = pair();
    for omit in [false, true] {
        let fixture = Fixture::new(&key, "one").await;
        {
            let mut metadata = fixture.state.broker.lock().unwrap();
            metadata["authentication"]["client_id"] = json!("native-deployment-client");
            metadata["authentication"]["access_token_audience"] = json!("native-deployment-api");
            metadata["authentication"]["audiences"] = json!(["native-deployment-api"]);
            if omit {
                metadata["authentication"]
                    .as_object_mut()
                    .unwrap()
                    .remove("mcp_resources");
            } else {
                metadata["authentication"]["mcp_resources"] = json!([]);
            }
        }
        let verifier = fixture.verifier_resource(None, limits());
        let c = Cancellation::default();
        let original = context(&c);
        let ctx = BrokerRequestContext::new(&original);
        let raw_access = token(&key, "one", &access(&fixture));
        let id_claims = id(&fixture);
        let id_expiration = id_claims["exp"].as_u64().unwrap();
        let raw_id = token(&key, "one", &id_claims);
        let proof = verifier
            .verify_login(&raw_id, &raw_access, "nonce-fixture", ctx)
            .await
            .unwrap();
        assert_eq!(proof.subject(), "stable-subject");
        assert_eq!(proof.issuer(), fixture.origin);
        assert_eq!(proof.provider_revision(), 7);
        assert_eq!(proof.profile_claims().unwrap().username(), "fixture-user");
        assert_eq!(
            proof.profile_claims().unwrap().organization(),
            Some("fixture-org")
        );
        assert_eq!(proof.profile_claims().unwrap().teams(), ["fixture-team"]);
        proof.check(&verifier, ctx).unwrap();
        proof
            .check(&verifier, BrokerRequestContext::new(&original))
            .unwrap();
        assert_eq!(
            proof.expires_at(),
            UNIX_EPOCH + Duration::from_secs(id_expiration)
        );
        let metadata = verifier.metadata(ctx).await.unwrap();
        assert_eq!(metadata.client_id(), "native-deployment-client");
        assert_eq!(metadata.access_token_audience(), "native-deployment-api");
        let refreshed = verifier
            .verify_refresh(&raw_access, None, "stable-subject", ctx)
            .await
            .unwrap();
        assert!(refreshed.profile_claims().is_none());
        refreshed.check(&verifier, ctx).unwrap();
        let mut refresh_id = id(&fixture);
        refresh_id.as_object_mut().unwrap().remove("nonce");
        let refresh_id = token(&key, "one", &refresh_id);
        assert!(
            verifier
                .verify_refresh(&raw_access, Some(&refresh_id), "stable-subject", ctx)
                .await
                .unwrap()
                .profile_claims()
                .is_some()
        );
        assert_eq!(
            deny(verifier.verify(&raw_access, ctx).await),
            BrokerError::InvalidPolicy
        );
        assert_eq!(fixture.count("/.well-known/graphit-broker"), 1);
        assert_eq!(fixture.count("/.well-known/openid-configuration"), 1);
        assert_eq!(fixture.count("/keys"), 1);
        assert_eq!(fixture.count("/userinfo"), 0);
        assert_eq!(fixture.count("/token"), 0);
    }
}

#[tokio::test]
async fn resource_pair_and_native_claim_matrix_never_call_userinfo() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    let verifier = fixture.verifier(limits());
    let c = Cancellation::default();
    let original = context(&c);
    let ctx = BrokerRequestContext::new(&original);
    let raw_id = token(&key, "one", &id(&fixture));
    let raw_access = token(&key, "one", &access(&fixture));
    verifier
        .verify_login(&raw_id, &raw_access, "nonce-fixture", ctx)
        .await
        .unwrap();
    for (field, value) in [
        ("iss", json!("https://wrong.example")),
        ("sub", json!("")),
        ("iat", Value::Null),
        ("iat", json!(id(&fixture)["exp"].as_u64().unwrap() + 1)),
        ("exp", Value::Null),
        ("exp", json!(0)),
        ("exp", json!(u64::MAX)),
        ("nbf", Value::Null),
        ("nbf", json!(u64::MAX)),
        ("nonce", json!("wrong")),
        ("nonce", Value::Null),
        ("aud", json!("broker-api")),
        ("aud", json!(["graphit-cli", "unadvertised"])),
        ("aud", json!(["graphit-cli", "graphit-cli"])),
        ("aud", json!(["graphit-cli", "broker-api"])),
        ("azp", json!("wrong")),
        ("azp", Value::Null),
        ("preferred_username", json!("")),
        ("preferred_username", Value::Null),
    ] {
        let mut claims = id(&fixture);
        claims[field] = value;
        let raw = token(&key, "one", &claims);
        assert!(
            verifier
                .verify_login(&raw, &raw_access, "nonce-fixture", ctx)
                .await
                .is_err(),
            "accepted invalid ID field {field}"
        );
    }
    for (field, value) in [
        ("iss", json!("https://wrong.example")),
        ("client_id", json!("other-client")),
        ("token_use", json!("id")),
        ("jti", json!("")),
        ("jti", Value::Null),
        ("sub", json!("other-subject")),
        ("iat", Value::Null),
        ("exp", Value::Null),
        ("exp", json!(u64::MAX)),
        ("aud", json!([fixture.resource])),
        ("aud", json!(["broker-api"])),
        (
            "aud",
            json!(["broker-api", fixture.resource, fixture.resource]),
        ),
        ("nbf", Value::Null),
    ] {
        let mut claims = access(&fixture);
        claims[field] = value;
        let raw = token(&key, "one", &claims);
        assert!(
            verifier
                .verify_login(&raw_id, &raw, "nonce-fixture", ctx)
                .await
                .is_err(),
            "accepted invalid access field {field}"
        );
    }
    assert_eq!(
        deny(
            verifier
                .verify_refresh(&raw_access, Some(&raw_id), "wrong-subject", ctx)
                .await
        ),
        BrokerError::InvalidToken
    );
    assert_eq!(
        deny(
            verifier
                .verify_refresh(&raw_access, None, "wrong-subject", ctx)
                .await
        ),
        BrokerError::InvalidToken
    );
    // Allowed multiple ID audiences require exact azp, without requiring token_use.
    let mut valid = id(&fixture);
    valid["aud"] = json!(["graphit-cli", "broker-api", fixture.resource]);
    valid["azp"] = json!("graphit-cli");
    let raw = token(&key, "one", &valid);
    verifier
        .verify_login(&raw, &raw_access, "nonce-fixture", ctx)
        .await
        .unwrap();
    assert_eq!(fixture.count("/userinfo"), 0);
    assert_eq!(fixture.count("/keys"), 1);
}

#[tokio::test]
async fn retained_native_and_mcp_proofs_bind_request_instance_and_expiry_without_renewal() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    let mut verifier = fixture.verifier(limits());
    let start = Moment {
        utc: SystemTime::now(),
        mono: Instant::now(),
    };
    let clock = Arc::new(TestClock(Mutex::new(start)));
    verifier.clock = clock.clone();
    let c = Cancellation::default();
    let original = context(&c);
    let ctx = BrokerRequestContext::new(&original);
    let issued = start.utc.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let mut id_claims = id(&fixture);
    id_claims["iat"] = json!(issued);
    id_claims["exp"] = json!(issued + 10);
    let raw_id = token(&key, "one", &id_claims);
    let mut access_claims = access(&fixture);
    access_claims["iat"] = json!(issued);
    let raw_access = token(&key, "one", &access_claims);
    let native = verifier
        .verify_login(&raw_id, &raw_access, "nonce-fixture", ctx)
        .await
        .unwrap();
    let mcp = verifier.verify(&raw_access, ctx).await.unwrap();
    mcp.check(&verifier, ctx).unwrap();
    mcp.check(&verifier, BrokerRequestContext::new(&original))
        .unwrap();
    let fresh = SnapshotContext::new(original.deadline().unwrap(), c.clone());
    assert_eq!(
        native.check(&verifier, BrokerRequestContext::new(&fresh)),
        Err(BrokerError::InvalidToken)
    );
    assert_eq!(
        mcp.check(&verifier, BrokerRequestContext::new(&fresh)),
        Err(BrokerError::InvalidToken)
    );
    let other = fixture.verifier(limits());
    assert_eq!(native.check(&other, ctx), Err(BrokerError::InvalidToken));
    assert_eq!(mcp.check(&other, ctx), Err(BrokerError::InvalidToken));
    let mut revised = fixture.verifier(limits());
    revised.revision = 8;
    assert_eq!(native.check(&revised, ctx), Err(BrokerError::InvalidToken));
    *clock.0.lock().unwrap() = Moment {
        utc: start.utc - Duration::from_secs(100),
        mono: start.mono + Duration::from_secs(1),
    };
    native.check(&verifier, ctx).unwrap(); // rollback does not renew the original mono expiry
    *clock.0.lock().unwrap() = Moment {
        utc: start.utc - Duration::from_secs(100),
        mono: start.mono + Duration::from_secs(20),
    };
    assert_eq!(native.check(&verifier, ctx), Err(BrokerError::InvalidToken));
    *clock.0.lock().unwrap() = Moment {
        utc: start.utc + Duration::from_secs(7200),
        mono: start.mono + Duration::from_secs(7200),
    };
    assert_eq!(mcp.check(&verifier, ctx), Err(BrokerError::InvalidToken));
    c.cancel();
    assert_eq!(native.check(&verifier, ctx), Err(BrokerError::Cancelled));
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert_eq!(
        mcp.check(&verifier, BrokerRequestContext::new(&expired)),
        Err(BrokerError::Deadline)
    );
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn native_pair_rotation_charges_only_one_forced_fetch_for_both_kids() {
    let old = pair();
    let new = pair();
    let fixture = Fixture::new(&old, "old").await;
    let verifier = fixture.verifier(limits());
    let c = Cancellation::default();
    let original = context(&c);
    let ctx = BrokerRequestContext::new(&original);
    verifier.metadata(ctx).await.unwrap();
    let id_key = key_set(&new, "new-id")["keys"][0].clone();
    let access_key = key_set(&new, "new-access")["keys"][0].clone();
    *fixture.state.keys.lock().unwrap() = json!({"keys":[id_key,access_key]});
    let raw_id = token(&new, "new-id", &id(&fixture));
    let raw_access = token(&new, "new-access", &access(&fixture));
    verifier
        .verify_login(&raw_id, &raw_access, "nonce-fixture", ctx)
        .await
        .unwrap();
    assert_eq!(fixture.count("/keys"), 2);
    let unknown_access = token(&new, "missing", &access(&fixture));
    assert!(
        verifier
            .verify_login(&raw_id, &unknown_access, "nonce-fixture", ctx)
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/keys"), 2); // no second reload for a token in the same pair
    assert_eq!(fixture.count("/userinfo"), 0);
    // If the one reload only supplies one of the two keys, the pair fails without a third fetch.
    let fixture = Fixture::new(&old, "old").await;
    let verifier = fixture.verifier(limits());
    verifier.metadata(ctx).await.unwrap();
    *fixture.state.keys.lock().unwrap() = key_set(&new, "new-id");
    let raw_id = token(&new, "new-id", &id(&fixture));
    let raw_access = token(&new, "new-access", &access(&fixture));
    assert!(
        verifier
            .verify_login(&raw_id, &raw_access, "nonce-fixture", ctx)
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/keys"), 2);
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn api_only_metadata_and_native_input_boundaries_fail_without_authority() {
    let key = pair();
    let c = Cancellation::default();
    for malformed in [Value::Null, json!("not an array"), json!(["dup", "dup"])] {
        let fixture = Fixture::new(&key, "one").await;
        fixture.state.broker.lock().unwrap()["authentication"]["mcp_resources"] = malformed;
        let verifier = fixture.verifier_resource(None, limits());
        let original = context(&c);
        assert_eq!(
            deny(
                verifier
                    .metadata(BrokerRequestContext::new(&original))
                    .await
            ),
            BrokerError::InvalidMetadata
        );
        assert_eq!(fixture.count("/keys"), 0);
        assert_eq!(fixture.count("/userinfo"), 0);
    }
    let fixture = Fixture::new(&key, "one").await;
    let raw_id = token(&key, "one", &id(&fixture));
    let raw_access = token(&key, "one", &access(&fixture));
    let original = context(&c);
    let ctx = BrokerRequestContext::new(&original);
    let mut policy = limits();
    policy.token_bytes = raw_id.len().max(raw_access.len());
    let verifier = fixture.verifier(policy);
    verifier
        .verify_login(&raw_id, &raw_access, "nonce-fixture", ctx)
        .await
        .unwrap();
    policy.token_bytes -= 1;
    let short = fixture.verifier(policy);
    assert_eq!(
        deny(
            short
                .verify_login(&raw_id, &raw_access, "nonce-fixture", ctx)
                .await
        ),
        BrokerError::Limit
    );
    let counts = fixture.state.counts.lock().unwrap().clone();
    let verifier = fixture.verifier(limits());
    assert_eq!(
        deny(verifier.verify_login(&raw_id, &raw_access, "", ctx).await),
        BrokerError::InvalidToken
    );
    assert_eq!(
        deny(verifier.verify_refresh(&raw_access, None, "", ctx).await),
        BrokerError::InvalidToken
    );
    let large = "x".repeat(limits().json.scalar_bytes + 1);
    assert_eq!(
        deny(
            verifier
                .verify_login(&raw_id, &raw_access, &large, ctx)
                .await
        ),
        BrokerError::Limit
    );
    assert_eq!(
        deny(
            verifier
                .verify_refresh(&raw_access, None, &large, ctx)
                .await
        ),
        BrokerError::Limit
    );
    let duplicate = id(&fixture)
        .to_string()
        .replacen("{", "{\"nonce\":\"duplicate\",", 1);
    let duplicate = token_header(&key, &json!({"alg":"EdDSA","kid":"one"}), &duplicate);
    assert!(
        verifier
            .verify_login(&duplicate, &raw_access, "nonce-fixture", ctx)
            .await
            .is_err()
    );
    let malicious = token_header(
        &key,
        &json!({"alg":"EdDSA","kid":"one","jku":"https://evil.example/keys"}),
        &id(&fixture).to_string(),
    );
    assert!(
        verifier
            .verify_login(&malicious, &raw_access, "nonce-fixture", ctx)
            .await
            .is_err()
    );
    assert_eq!(*fixture.state.counts.lock().unwrap(), counts);
}
