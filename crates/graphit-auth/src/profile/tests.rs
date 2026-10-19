use super::*;
use graphit_core::file_lock::Cancellation;
use graphit_hub::native::ContentLimits;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn limits() -> ProfileLimits {
    ProfileLimits {
        bytes: 128 * 1024,
        nodes: 8192,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 32,
    }
}
fn context(c: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 128 * 1024,
            max_output_bytes: 128 * 1024,
            max_depth: 32,
            max_nodes: 8192,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        c,
        deadline,
    )
    .unwrap()
}
fn secret(v: &str) -> Secret {
    Secret::new(SecretString::from(v))
}
fn local() -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Local {
            allow_aws_credential_chain: false,
            broker: None,
        },
        embedding: AiService::Local(LocalExecution {
            device: LocalDevice::Cpu,
            device_id: 0,
        }),
        rerank: AiService::Disabled,
        s3: S3Location {
            bucket: None,
            region: None,
            endpoint: None,
            prefix: None,
            source: S3Source::LocalCredentials,
        },
    }
}
fn broker() -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Broker(BrokerService {
            endpoint: "https://broker.example".into(),
            mcp_resource: Some("https://code.example/mcp".into()),
            allow_anonymous: false,
        }),
        embedding: AiService::Broker,
        rerank: AiService::Broker,
        s3: S3Location {
            bucket: Some("objects".into()),
            region: Some("region-1".into()),
            endpoint: Some("https://s3.example".into()),
            prefix: Some("graphit/".into()),
            source: S3Source::BrokerScope,
        },
    }
}

#[test]
fn broker_inference_policy_rejects_local_direct_on_create_update_and_decode() {
    let cancellation = Cancellation::default();
    let ctx = context(&cancellation, Instant::now() + Duration::from_secs(30));
    let base = fixture(&ctx);
    let bytes = base.encode(&ctx).unwrap();
    for field in ["embedding", "rerank"] {
        for service in [
            AiService::Local(LocalExecution {
                device: LocalDevice::Cpu,
                device_id: 0,
            }),
            AiService::Direct {
                protocol: DirectProtocol::OpenAi,
                endpoint: "https://inference.example/embeddings".into(),
                model: "model".into(),
                model_revision: "revision".into(),
                recipe: "recipe".into(),
                dimensions: 3,
                request_dimensions: None,
                batch_endpoint: None,
            },
        ] {
            let raw_service = match &service {
                AiService::Local(_) => {
                    json!({"mode": "local", "execution": {"device": "cpu", "device_id": 0}})
                }
                _ => {
                    json!({"mode": "direct", "protocol": "openai", "endpoint": "https://inference.example/embeddings", "model": "model", "model_revision": "revision", "recipe": "recipe", "dimensions": 3})
                }
            };
            let mut config = broker();
            if field == "embedding" {
                config.embedding = service;
            } else {
                config.rerank = service;
            }
            let mut document = AuthDocument::decode(&bytes, limits(), &ctx).unwrap();
            assert_eq!(
                document.add_provider(
                    "invalid".into(),
                    config.clone(),
                    document.revision(),
                    50,
                    &ctx
                ),
                Err(ProfileError::InvalidDocument)
            );
            assert_eq!(
                document.update_provider(
                    "company",
                    config,
                    document.revision(),
                    document.provider("company").unwrap().revision(),
                    50,
                    &ctx
                ),
                Err(ProfileError::InvalidDocument)
            );
            assert_eq!(&*document.encode(&ctx).unwrap(), &*bytes);
            let mut raw = value(&base, &ctx);
            raw["providers"][0]["config"][field] = raw_service;
            assert_eq!(
                error(decode_value(&raw, &ctx)),
                ProfileError::InvalidDocument
            );
        }
    }
    let mut config = broker();
    config.embedding = AiService::Disabled;
    config.rerank = AiService::Disabled;
    validate_config(&config).unwrap();
}
fn local_data(name: &str, provider: &str) -> ProfileData {
    ProfileData {
        name: name.into(),
        provider: provider.into(),
        username: "alice".into(),
        organization: Some("org".into()),
        teams: vec!["platform".into()],
        credentials: ProfileCredentials::Local(LocalCredentials {
            anonymous: false,
            mcp_key: Some(secret("mcp-sentinel")),
            broker_key: None,
            embedding_api_key: Some(secret("embedding-sentinel")),
            rerank_api_key: Some(secret("rerank-sentinel")),
            s3: Some(LocalS3 {
                access_key_id: Some(secret("access-sentinel")),
                secret_access_key: Some(secret("secret-sentinel")),
                session_token: Some(secret("session-sentinel")),
                expires_at: Some(100),
                aws_profile: Some("developer".into()),
            }),
        }),
    }
}
fn broker_data(name: &str) -> ProfileData {
    ProfileData {
        name: name.into(),
        provider: "company".into(),
        username: "alice".into(),
        organization: None,
        teams: vec!["engineering".into()],
        credentials: ProfileCredentials::Broker {
            issuer: "https://broker.example".into(),
            subject: "subject-1".into(),
            tokens: OidcTokens {
                access_token: secret("access-sentinel"),
                refresh_token: Some(secret("refresh-sentinel")),
                id_token: secret("id-sentinel"),
                expires_at: 100,
            },
            s3_disabled: true,
        },
    }
}
fn replace(d: &mut AuthDocument, data: ProfileData, now: i64, ctx: &RequestContext<'_>) {
    let expectation = ProfileExpectation {
        document_revision: d.revision(),
        provider_revision: d.provider(&data.provider).unwrap().revision(),
        profile_revision: d.profile(&data.name).map(ProfileRecord::revision),
    };
    d.replace_profile_data(data, expectation, now, ctx).unwrap();
}
fn fixture(ctx: &RequestContext<'_>) -> AuthDocument {
    let mut d = AuthDocument::new(limits(), ctx).unwrap();
    d.add_provider("workstation".into(), local(), d.revision(), 10, ctx)
        .unwrap();
    d.add_provider("company".into(), broker(), d.revision(), 20, ctx)
        .unwrap();
    replace(&mut d, local_data("local-user", "workstation"), 30, ctx);
    replace(&mut d, broker_data("broker-user"), 40, ctx);
    d.activate("broker-user", d.revision(), ctx).unwrap();
    d
}
fn value(d: &AuthDocument, ctx: &RequestContext<'_>) -> Value {
    serde_json::from_slice(&d.encode(ctx).unwrap()).unwrap()
}
fn decode_value(v: &Value, ctx: &RequestContext<'_>) -> Result<AuthDocument, ProfileError> {
    AuthDocument::decode(&serde_json::to_vec(v).unwrap(), limits(), ctx)
}
fn error<T>(r: Result<T, ProfileError>) -> ProfileError {
    match r {
        Ok(_) => panic!("unexpected auth data acceptance"),
        Err(e) => e,
    }
}

#[test]
fn lifecycle_fence_birth_provider_is_not_reused_after_remove_and_recreate() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = AuthDocument::new(limits(), &ctx).unwrap();
    d.add_provider("local".into(), local(), d.revision(), 10, &ctx)
        .unwrap();
    let old_document = d.revision();
    let old_provider = d.provider("local").unwrap().revision();
    d.remove_provider("local", false, d.revision(), &ctx)
        .unwrap();
    d.add_provider("local".into(), local(), d.revision(), 10, &ctx)
        .unwrap();
    let born = d.provider("local").unwrap();
    assert!(
        born.revision() > old_provider,
        "provider birth must distinguish the old incarnation"
    );
    assert_eq!(born.revision(), d.revision());
    assert_eq!(born.created_at(), 10);
    assert_eq!(born.updated_at(), 10);
    assert_eq!(
        error(d.update_provider("local", local(), old_document, old_provider, 11, &ctx)),
        ProfileError::Conflict
    );
    assert_eq!(
        error(d.update_provider("local", local(), d.revision(), old_provider, 11, &ctx)),
        ProfileError::Conflict
    );
}

#[test]
fn lifecycle_fence_birth_profile_and_tokens_are_not_reused_after_logout() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = AuthDocument::new(limits(), &ctx).unwrap();
    d.add_provider("local".into(), local(), d.revision(), 10, &ctx)
        .unwrap();
    replace(&mut d, local_data("alice", "local"), 20, &ctx);
    let p = d.profile("alice").unwrap();
    let own_before = (p.provider_revision(), p.revision(), p.token_revision());
    let old_document = d.revision();
    d.logout(Some("alice"), d.revision(), &ctx).unwrap();
    replace(&mut d, local_data("alice", "local"), 20, &ctx);
    let p = d.profile("alice").unwrap();
    let own_after = (p.provider_revision(), p.revision(), p.token_revision());
    assert_ne!(
        own_after, own_before,
        "same key/identity/timestamps cannot reuse profile birth"
    );
    assert!(own_after.1 > own_before.1 && own_after.2 > own_before.2);
    assert_eq!(
        (p.revision(), p.token_revision()),
        (d.revision(), d.revision())
    );
    assert_eq!((p.created_at(), p.updated_at()), (20, 20));
    let ProfileCredentials::Local(creds) = &p.data().unwrap().credentials else {
        panic!()
    };
    assert_eq!(creds.mcp_key.as_ref().unwrap().expose(), "mcp-sentinel");
    for doc_revision in [old_document, d.revision()] {
        assert_eq!(
            error(d.replace_profile_data(
                local_data("alice", "local"),
                ProfileExpectation {
                    document_revision: doc_revision,
                    provider_revision: own_before.0,
                    profile_revision: Some(own_before.1),
                },
                21,
                &ctx
            )),
            ProfileError::Conflict
        );
    }
}

#[test]
fn lifecycle_fence_activation_restoration_changes_only_selected_profile_revision() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = fixture(&ctx);
    d.activate("local-user", d.revision(), &ctx).unwrap();
    let a = d.profile("local-user").unwrap();
    let before = (
        a.revision(),
        a.token_revision(),
        a.provider_revision(),
        a.created_at(),
        a.updated_at(),
    );
    let data_before = value(&d, &ctx)["profiles"][1]["data"].clone();
    d.activate("broker-user", d.revision(), &ctx).unwrap();
    d.activate("local-user", d.revision(), &ctx).unwrap();
    let a = d.profile("local-user").unwrap();
    assert_eq!(
        a.revision(),
        before.0 + 1,
        "A to B to A must not restore the old own binding"
    );
    assert_eq!(
        (
            a.token_revision(),
            a.provider_revision(),
            a.created_at(),
            a.updated_at()
        ),
        (before.1, before.2, before.3, before.4)
    );
    assert_eq!(value(&d, &ctx)["profiles"][1]["data"], data_before);
    let current = a.revision();
    let old_document = d.revision();
    d.activate("local-user", d.revision(), &ctx).unwrap();
    assert_eq!(d.profile("local-user").unwrap().revision(), current);
    assert_eq!(d.revision(), old_document + 1);
}
#[test]
fn local_and_broker_roundtrip_preserves_all_current_fields_without_sts_shape() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let d = fixture(&ctx);
    let bytes = d.encode(&ctx).unwrap();
    let result = AuthDocument::decode(&bytes, limits(), &ctx).unwrap();
    assert_eq!(&*bytes, &*result.encode(&ctx).unwrap());
    assert_eq!(result.providers().len(), 2);
    assert_eq!(result.profiles().len(), 2);
    assert_eq!(
        result
            .active(&ctx)
            .unwrap()
            .1
            .data()
            .expect("Local/Code fixture")
            .username,
        "alice"
    );
    let p = result.profile("local-user").unwrap();
    assert_eq!(p.created_at(), 30);
    assert_eq!(p.updated_at(), 30);
    let ProfileCredentials::Local(l) = &p.data().expect("Local/Code fixture").credentials else {
        panic!()
    };
    assert_eq!(l.mcp_key.as_ref().unwrap().expose(), "mcp-sentinel");
    assert_eq!(
        l.embedding_api_key.as_ref().unwrap().expose(),
        "embedding-sentinel"
    );
    assert_eq!(
        l.rerank_api_key.as_ref().unwrap().expose(),
        "rerank-sentinel"
    );
    let s = l.s3.as_ref().unwrap();
    assert_eq!(
        s.access_key_id.as_ref().unwrap().expose(),
        "access-sentinel"
    );
    assert_eq!(
        s.secret_access_key.as_ref().unwrap().expose(),
        "secret-sentinel"
    );
    assert_eq!(
        s.session_token.as_ref().unwrap().expose(),
        "session-sentinel"
    );
    assert_eq!(s.expires_at, Some(100));
    assert_eq!(s.aws_profile.as_deref(), Some("developer"));
    let v = value(&d, &ctx);
    let b = &v["profiles"][0]["data"]["credentials"];
    assert_eq!(b["kind"], "broker");
    assert!(b.get("s3").is_none());
    assert!(b.get("local").is_none());
    assert_eq!(b["tokens"]["token_type"], "Bearer");
    assert_eq!(b["s3_disabled"], true);
}
#[test]
fn local_execution_codec_is_canonical_and_defaults_to_cpu_without_legacy_aliases() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel, Instant::now() + Duration::from_secs(30));
    let mut document = AuthDocument::new(limits(), &ctx).unwrap();
    document
        .ensure_default_local(document.revision(), 1, &ctx)
        .unwrap();
    let encoded = value(&document, &ctx);
    for purpose in ["embedding", "rerank"] {
        assert_eq!(
            encoded["providers"][0]["config"][purpose],
            json!({"mode":"local","execution":{"device":"cpu","device_id":0}})
        );
        let mut old_shape = encoded.clone();
        old_shape["providers"][0]["config"][purpose] =
            json!({"mode":"local","onnx":{"device":"cpu","device_id":0}});
        assert!(decode_value(&old_shape, &ctx).is_err());
        let mut mixed = encoded.clone();
        mixed["providers"][0]["config"][purpose]["onnx"] = json!({"device":"cpu","device_id":0});
        assert!(decode_value(&mixed, &ctx).is_err());
        for device in ["coreml", "onnx", "", "CPU"] {
            let mut invalid = encoded.clone();
            invalid["providers"][0]["config"][purpose]["execution"]["device"] = json!(device);
            assert!(decode_value(&invalid, &ctx).is_err());
        }
    }
    assert_eq!(
        document.encode(&ctx).unwrap().as_slice(),
        AuthDocument::decode(&document.encode(&ctx).unwrap(), limits(), &ctx)
            .unwrap()
            .encode(&ctx)
            .unwrap()
            .as_slice()
    );
    assert!(document.profiles().is_empty());
}

#[test]
fn fresh_format_unknown_null_duplicate_trailing_and_invalid_numeric_are_denied() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let d = fixture(&ctx);
    let v = value(&d, &ctx);
    for (field, bad) in [
        ("format", json!("go-auth-v4")),
        ("format", json!("rust-auth-v1 ")),
        ("epoch", json!(0)),
        ("epoch", json!(2)),
        ("document_revision", json!(0)),
        ("document_revision", json!(-1)),
        ("document_revision", json!(1.0)),
        ("providers", Value::Null),
        ("active_profile", Value::Null),
    ] {
        let mut n = v.clone();
        n[field] = bad;
        assert_eq!(error(decode_value(&n, &ctx)), ProfileError::InvalidDocument);
    }
    let mut n = v.clone();
    n["ignored"] = json!({"huge":"x"});
    assert_eq!(error(decode_value(&n, &ctx)), ProfileError::InvalidDocument);
    let text = String::from_utf8(d.encode(&ctx).unwrap().to_vec()).unwrap();
    for bad in [
        format!("{text} {{}}"),
        text.replacen("{", "{\"format\":\"rust-auth-v1\",", 1),
        text.replacen(
            "\"access_token\":",
            "\"access_token\":\"duplicate\",\"access_token\":",
            1,
        ),
    ] {
        assert_eq!(
            error(AuthDocument::decode(bad.as_bytes(), limits(), &ctx)),
            ProfileError::InvalidDocument
        );
    }
    assert_eq!(
        error(AuthDocument::decode(
            br#"{"version":4,"providers":{},"profiles":{}}"#,
            limits(),
            &ctx
        )),
        ProfileError::InvalidDocument
    );
}
#[test]
fn duplicate_names_null_optional_and_broker_persisted_s3_or_static_keys_are_denied() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let d = fixture(&ctx);
    let v = value(&d, &ctx);
    let mut n = v.clone();
    let duplicate = n["providers"][0].clone();
    n["providers"].as_array_mut().unwrap().push(duplicate);
    assert_eq!(error(decode_value(&n, &ctx)), ProfileError::InvalidDocument);
    let mut n = v.clone();
    let duplicate = n["profiles"][0].clone();
    n["profiles"].as_array_mut().unwrap().push(duplicate);
    assert_eq!(error(decode_value(&n, &ctx)), ProfileError::InvalidDocument);
    for field in ["s3", "mcp_key", "broker_key", "local"] {
        let mut n = v.clone();
        n["profiles"][0]["data"]["credentials"][field] = json!({});
        assert_eq!(error(decode_value(&n, &ctx)), ProfileError::InvalidDocument);
    }
    let mut n = v.clone();
    n["profiles"][0]["data"]["credentials"]["tokens"]["refresh_token"] = Value::Null;
    assert_eq!(error(decode_value(&n, &ctx)), ProfileError::InvalidDocument);
    let mut n = v.clone();
    n["profiles"][0]["data"]["credentials"]["tokens"]["token_type"] = json!("bearer");
    assert_eq!(error(decode_value(&n, &ctx)), ProfileError::InvalidDocument);
}
fn nodes(v: &Value) -> usize {
    1 + match v {
        Value::Object(m) => m.len() + m.values().map(nodes).sum::<usize>(),
        Value::Array(a) => a.iter().map(nodes).sum(),
        _ => 0,
    }
}
fn max_scalar(v: &Value) -> usize {
    match v {
        Value::Object(m) => m
            .iter()
            .map(|(k, v)| k.len().max(max_scalar(v)))
            .max()
            .unwrap_or(0),
        Value::Array(a) => a.iter().map(max_scalar).max().unwrap_or(0),
        Value::String(s) => s.len(),
        _ => 0,
    }
}
#[test]
fn exact_byte_node_scalar_provider_profile_and_collection_boundaries() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let d = fixture(&ctx);
    let bytes = d.encode(&ctx).unwrap();
    let v = value(&d, &ctx);
    let mut policy = limits();
    policy.bytes = bytes.len();
    policy.nodes = nodes(&v);
    policy.scalar_bytes = max_scalar(&v);
    policy.providers = 2;
    policy.profiles = 2;
    policy.collection_entries = 6;
    let loaded = AuthDocument::decode(&bytes, policy, &ctx).unwrap();
    assert_eq!(&*loaded.encode(&ctx).unwrap(), &*bytes);
    for change in 0..6 {
        let mut p = policy;
        match change {
            0 => p.bytes -= 1,
            1 => p.nodes -= 1,
            2 => p.scalar_bytes -= 1,
            3 => p.providers -= 1,
            4 => p.profiles -= 1,
            _ => p.collection_entries -= 1,
        }
        assert_eq!(
            error(AuthDocument::decode(&bytes, p, &ctx)),
            ProfileError::Limit
        );
    }
    for change in 0..6 {
        let mut p = limits();
        match change {
            0 => p.bytes = 0,
            1 => p.nodes = 0,
            2 => p.scalar_bytes = 0,
            3 => p.providers = 0,
            4 => p.profiles = 0,
            _ => p.collection_entries = 0,
        }
        assert_eq!(
            error(AuthDocument::decode(&bytes, p, &ctx)),
            ProfileError::InvalidPolicy
        );
    }
}
#[test]
fn escaped_scalar_is_bounded_before_owned_secret_copy() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let d = fixture(&ctx);
    let mut v = value(&d, &ctx);
    v["profiles"][0]["data"]["credentials"]["tokens"]["access_token"] = json!("x".repeat(4097));
    assert_eq!(error(decode_value(&v, &ctx)), ProfileError::Limit);
    let mut raw = String::from_utf8(d.encode(&ctx).unwrap().to_vec()).unwrap();
    raw = raw.replace("access-sentinel", &"\\u0078".repeat(4097));
    assert_eq!(
        error(AuthDocument::decode(raw.as_bytes(), limits(), &ctx)),
        ProfileError::Limit
    );
}
#[test]
fn cancellation_and_original_deadline_apply_to_decode_encode_and_mutations() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = fixture(&ctx);
    let bytes = d.encode(&ctx).unwrap();
    let revision = d.revision();
    c.cancel();
    assert_eq!(error(d.encode(&ctx)), ProfileError::Cancelled);
    assert_eq!(
        error(AuthDocument::decode(&bytes, limits(), &ctx)),
        ProfileError::Cancelled
    );
    assert_eq!(
        error(d.logout(None, revision, &ctx)),
        ProfileError::Cancelled
    );
    assert_eq!(d.revision(), revision);
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_millis(2));
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(
        error(AuthDocument::decode(&bytes, limits(), &ctx)),
        ProfileError::Deadline
    );
}
#[test]
fn ensure_default_materializes_only_provider_and_preserves_explicit_settings() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = AuthDocument::new(limits(), &ctx).unwrap();
    assert!(d.ensure_default_local(1, 10, &ctx).unwrap());
    assert_eq!(d.profiles.len(), 0);
    assert!(d.active_name().is_none());
    assert_eq!(d.provider("local").unwrap().revision(), d.revision());
    assert!(!d.ensure_default_local(2, 99, &ctx).unwrap());
    assert_eq!(d.revision(), 2);
    let mut config = local();
    config.embedding = AiService::Disabled;
    d.update_provider(
        "local",
        config.clone(),
        d.revision(),
        d.provider("local").unwrap().revision(),
        20,
        &ctx,
    )
    .unwrap();
    let r = d.revision();
    assert!(!d.ensure_default_local(r, 30, &ctx).unwrap());
    assert_eq!(d.provider("local").unwrap().config(), &config);
    assert_eq!(d.revision(), r);
    let mut d = AuthDocument::new(limits(), &ctx).unwrap();
    d.add_provider("local".into(), broker(), 1, 10, &ctx)
        .unwrap();
    assert_eq!(
        error(d.ensure_default_local(2, 20, &ctx)),
        ProfileError::Conflict
    );
}
#[test]
fn provider_update_stales_active_preserves_records_and_logout_can_remove_them() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = fixture(&ctx);
    let created = d.provider("company").unwrap().created_at();
    let provider_revision = d.provider("company").unwrap().revision();
    d.update_provider(
        "company",
        broker(),
        d.revision(),
        provider_revision,
        50,
        &ctx,
    )
    .unwrap();
    assert_eq!(d.provider("company").unwrap().created_at(), created);
    assert_eq!(
        d.profile("broker-user").unwrap().provider_revision(),
        provider_revision
    );
    assert_eq!(error(d.active(&ctx)), ProfileError::Stale);
    assert_eq!(
        error(d.activate("broker-user", d.revision(), &ctx)),
        ProfileError::Stale
    );
    let bytes = d.encode(&ctx).unwrap();
    let mut loaded = AuthDocument::decode(&bytes, limits(), &ctx).unwrap();
    assert_eq!(error(loaded.active(&ctx)), ProfileError::Stale);
    loaded.logout(None, loaded.revision(), &ctx).unwrap();
    assert!(loaded.profile("broker-user").is_none());
    assert!(loaded.active_name().is_none());
}
#[test]
fn missing_provider_and_profile_selectors_never_fallback_but_stale_data_survives() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let d = fixture(&ctx);
    let mut v = value(&d, &ctx);
    v["providers"].as_array_mut().unwrap().remove(0);
    let mut loaded = decode_value(&v, &ctx).unwrap();
    assert_eq!(error(loaded.active(&ctx)), ProfileError::Missing);
    loaded
        .logout(Some("broker-user"), loaded.revision(), &ctx)
        .unwrap();
    assert_eq!(loaded.profiles.len(), 1);
    let mut v = value(&d, &ctx);
    v["active_profile"] = json!("missing");
    let loaded = decode_value(&v, &ctx).unwrap();
    assert_eq!(error(loaded.active(&ctx)), ProfileError::Missing);
}
#[test]
fn referenced_provider_removal_requires_explicit_cascade() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = fixture(&ctx);
    let before = d.encode(&ctx).unwrap();
    assert_eq!(
        error(d.remove_provider("company", false, d.revision(), &ctx)),
        ProfileError::Conflict
    );
    assert_eq!(&*before, &*d.encode(&ctx).unwrap());
    d.remove_provider("company", true, d.revision(), &ctx)
        .unwrap();
    assert!(d.provider("company").is_none());
    assert!(d.profile("broker-user").is_none());
    assert!(d.active_name().is_none());
    assert!(d.profile("local-user").is_some());
}
#[test]
fn data_replacement_preserves_created_timestamp_and_requires_exact_cas() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = fixture(&ctx);
    let before = d.encode(&ctx).unwrap();
    let token_revision = d.profile("broker-user").unwrap().token_revision();
    let expected = ProfileExpectation {
        document_revision: d.revision(),
        provider_revision: d.provider("company").unwrap().revision(),
        profile_revision: Some(d.profile("broker-user").unwrap().revision()),
    };
    assert_eq!(
        error(d.replace_profile_data(broker_data("broker-user"), expected, 39, &ctx)),
        ProfileError::Conflict
    );
    assert_eq!(&*before, &*d.encode(&ctx).unwrap());
    d.replace_profile_data(broker_data("broker-user"), expected, 50, &ctx)
        .unwrap();
    let p = d.profile("broker-user").unwrap();
    assert_eq!(p.created_at(), 40);
    assert_eq!(p.updated_at(), 50);
    assert_eq!(p.revision(), expected.profile_revision.unwrap() + 1);
    assert_eq!(p.token_revision(), token_revision + 1);
    assert_eq!(
        error(d.replace_profile_data(broker_data("broker-user"), expected, 60, &ctx)),
        ProfileError::Conflict
    );
}
fn update() -> TokenUpdate {
    TokenUpdate {
        access_token: secret("access-rotated"),
        refresh_token: secret("refresh-rotated"),
        identity: ProfileIdentityUpdate::Preserve,
        expires_at: 200,
    }
}
fn token_expectation(d: &AuthDocument) -> TokenExpectation {
    let p = d.profile("broker-user").unwrap();
    TokenExpectation {
        document_revision: d.revision(),
        provider_revision: p.provider_revision(),
        profile_revision: p.revision(),
        token_revision: p.token_revision(),
    }
}
fn identity_update(username: &str, organization: Option<&str>, teams: &[&str]) -> TokenUpdate {
    let mut tokens = update();
    tokens.identity = ProfileIdentityUpdate::Replace {
        id_token: secret("new-id-sentinel"),
        claims: ProfileClaimsUpdate {
            username: username.into(),
            organization: organization.map(str::to_owned),
            teams: teams.iter().map(|v| (*v).to_owned()).collect(),
        },
    };
    tokens
}
#[test]
fn refreshed_id_and_claims_change_together_without_changing_the_principal() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    for (organization, teams) in [
        (None, vec![]),
        (Some("new-org"), vec!["new-team", "new-team"]),
    ] {
        let mut d = fixture(&ctx);
        let ProfileBody::Named(data) = &mut d.profiles[0].data else {
            panic!("Code fixture must retain named identity")
        };
        data.organization = Some("old-org".into());
        let expected = token_expectation(&d);
        d.rotate_tokens(
            "broker-user",
            identity_update("alice-renamed", organization, &teams),
            expected,
            50,
            &ctx,
        )
        .unwrap();
        let bytes = d.encode(&ctx).unwrap();
        let roundtrip = AuthDocument::decode(&bytes, limits(), &ctx).unwrap();
        assert_eq!(&*bytes, &*roundtrip.encode(&ctx).unwrap());
        assert_eq!(roundtrip.revision(), expected.document_revision + 1);
        assert_eq!(roundtrip.active_name(), Some("broker-user"));
        let p = roundtrip.profile("broker-user").unwrap();
        assert_eq!(p.revision(), expected.profile_revision + 1);
        assert_eq!(p.token_revision(), expected.token_revision + 1);
        assert_eq!(p.provider_revision(), expected.provider_revision);
        assert_eq!(p.created_at(), 40);
        assert_eq!(p.updated_at(), 50);
        assert_eq!(p.data().expect("Local/Code fixture").name, "broker-user");
        assert_eq!(p.data().expect("Local/Code fixture").provider, "company");
        assert_eq!(
            p.data().expect("Local/Code fixture").username,
            "alice-renamed"
        );
        assert_eq!(
            p.data()
                .expect("Local/Code fixture")
                .organization
                .as_deref(),
            organization
        );
        assert_eq!(p.data().expect("Local/Code fixture").teams, teams);
        let ProfileCredentials::Broker {
            issuer,
            subject,
            tokens,
            s3_disabled,
        } = &p.data().expect("Local/Code fixture").credentials
        else {
            panic!()
        };
        assert_eq!(issuer, "https://broker.example");
        assert_eq!(subject, "subject-1");
        assert!(*s3_disabled);
        assert_eq!(tokens.id_token.expose(), "new-id-sentinel");
        assert_eq!(tokens.access_token.expose(), "access-rotated");
        assert_eq!(
            tokens.refresh_token.as_ref().unwrap().expose(),
            "refresh-rotated"
        );
        assert_eq!(tokens.expires_at, 200);
    }
}
#[test]
fn identity_replacement_conflicts_and_invalid_claims_preserve_all_data() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = fixture(&ctx);
    let expected = token_expectation(&d);
    let before = d.encode(&ctx).unwrap();
    for wrong in 0..4 {
        let mut e = expected;
        match wrong {
            0 => e.document_revision += 1,
            1 => e.provider_revision += 1,
            2 => e.profile_revision += 1,
            _ => e.token_revision += 1,
        }
        assert_eq!(
            error(d.rotate_tokens(
                "broker-user",
                identity_update("changed", None, &[]),
                e,
                50,
                &ctx
            )),
            if wrong == 1 {
                ProfileError::Stale
            } else {
                ProfileError::Conflict
            },
        );
        assert_eq!(&*before, &*d.encode(&ctx).unwrap());
    }
    for (username, organization, teams) in [
        ("", None, vec![]),
        (" leading", None, vec![]),
        ("control\n", None, vec![]),
        ("valid", Some(""), vec![]),
        ("valid", Some(" padded "), vec![]),
        ("valid", None, vec![""]),
        ("valid", None, vec!["control\n"]),
    ] {
        assert_eq!(
            error(d.rotate_tokens(
                "broker-user",
                identity_update(username, organization, &teams),
                expected,
                50,
                &ctx
            )),
            ProfileError::InvalidDocument,
        );
        assert_eq!(&*before, &*d.encode(&ctx).unwrap());
    }
    let mut invalid = identity_update("valid", None, &[]);
    if let ProfileIdentityUpdate::Replace { id_token, .. } = &mut invalid.identity {
        *id_token = secret("");
    }
    assert_eq!(
        error(d.rotate_tokens("broker-user", invalid, expected, 50, &ctx)),
        ProfileError::InvalidDocument
    );
    assert_eq!(&*before, &*d.encode(&ctx).unwrap());
    assert_eq!(
        error(d.rotate_tokens(
            "broker-user",
            identity_update("changed", None, &[]),
            expected,
            39,
            &ctx
        )),
        ProfileError::Conflict
    );
    assert_eq!(&*before, &*d.encode(&ctx).unwrap());
}
#[test]
fn refreshed_identity_uses_the_prospective_document_byte_and_node_limits() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let original = fixture(&ctx).encode(&ctx).unwrap();
    let replacement = || {
        identity_update(
            "alice\\\"é-refreshed",
            Some("new-organization"),
            &["new-team", "second-team"],
        )
    };
    let mut accepted = AuthDocument::decode(&original, limits(), &ctx).unwrap();
    accepted
        .rotate_tokens(
            "broker-user",
            replacement(),
            token_expectation(&accepted),
            50,
            &ctx,
        )
        .unwrap();
    let output = accepted.encode(&ctx).unwrap();
    let output_nodes = nodes(&serde_json::from_slice(&output).unwrap());
    assert!(output.len() > original.len());
    assert!(output_nodes > nodes(&serde_json::from_slice(&original).unwrap()));
    for boundary in 0..3 {
        let mut policy = limits();
        policy.bytes = output.len() - usize::from(boundary == 1);
        policy.nodes = output_nodes - usize::from(boundary == 2);
        let mut d = AuthDocument::decode(&original, policy, &ctx).unwrap();
        let expected = token_expectation(&d);
        let result = d.rotate_tokens("broker-user", replacement(), expected, 50, &ctx);
        if boundary == 0 {
            result.unwrap();
            assert_eq!(&*d.encode(&ctx).unwrap(), &*output);
        } else {
            assert_eq!(error(result), ProfileError::Limit);
            assert_eq!(&*d.encode(&ctx).unwrap(), &*original);
        }
    }
}
#[test]
fn identity_claim_scalars_and_teams_are_admitted_before_any_replacement() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let original = fixture(&ctx).encode(&ctx).unwrap();
    for scalar in ["username", "organization", "team", "id"] {
        for length in [4096, 4097] {
            let content = "x".repeat(length);
            let one_team = [content.as_str()];
            let mut u = identity_update(
                if scalar == "username" {
                    &content
                } else {
                    "valid"
                },
                (scalar == "organization").then_some(content.as_str()),
                if scalar == "team" { &one_team } else { &[] },
            );
            if scalar == "id"
                && let ProfileIdentityUpdate::Replace { id_token, .. } = &mut u.identity
            {
                *id_token = secret(&content);
            }
            let mut d = AuthDocument::decode(&original, limits(), &ctx).unwrap();
            let expected = token_expectation(&d);
            let result = d.rotate_tokens("broker-user", u, expected, 50, &ctx);
            if length == 4096 {
                result.unwrap();
            } else {
                assert_eq!(error(result), ProfileError::Limit);
                assert_eq!(&*d.encode(&ctx).unwrap(), &*original);
            }
        }
    }
    for count in [64, 65] {
        let mut d = AuthDocument::decode(&original, limits(), &ctx).unwrap();
        let expected = token_expectation(&d);
        let teams = vec!["same-team"; count];
        let result = d.rotate_tokens(
            "broker-user",
            identity_update("valid", None, &teams),
            expected,
            50,
            &ctx,
        );
        if count == 64 {
            result.unwrap();
            assert_eq!(
                d.profile("broker-user")
                    .unwrap()
                    .data()
                    .expect("Local/Code fixture")
                    .teams
                    .len(),
                count
            );
        } else {
            assert_eq!(error(result), ProfileError::Limit);
            assert_eq!(&*d.encode(&ctx).unwrap(), &*original);
        }
    }
}
#[test]
fn replacement_denials_leave_identity_and_tokens_unchanged() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    for scenario in 0..7 {
        let mut d = fixture(&ctx);
        match scenario {
            0 => d.activate("local-user", d.revision(), &ctx).unwrap(),
            1 => d
                .update_provider(
                    "company",
                    broker(),
                    d.revision(),
                    d.provider("company").unwrap().revision(),
                    60,
                    &ctx,
                )
                .unwrap(),
            2 => d.revision = u64::MAX,
            3 => {
                d.revision = u64::MAX;
                d.profiles[0].revision = u64::MAX;
            }
            4 => {
                d.revision = u64::MAX;
                d.profiles[0].token_revision = u64::MAX;
            }
            _ => (),
        }
        let expected = token_expectation(&d);
        let before = d.encode(&ctx).unwrap();
        let denied_cancellation = Cancellation::default();
        let denied = context(
            &denied_cancellation,
            Instant::now()
                + if scenario == 6 {
                    Duration::from_millis(2)
                } else {
                    Duration::from_secs(30)
                },
        );
        if scenario == 5 {
            denied_cancellation.cancel();
        } else if scenario == 6 {
            std::thread::sleep(Duration::from_millis(5));
        }
        let expected_error = match scenario {
            0 => ProfileError::Conflict,
            1 => ProfileError::Stale,
            2..=4 => ProfileError::Limit,
            5 => ProfileError::Cancelled,
            _ => ProfileError::Deadline,
        };
        assert_eq!(
            error(d.rotate_tokens(
                "broker-user",
                identity_update("changed", Some("changed-org"), &["changed-team"]),
                expected,
                70,
                &denied
            )),
            expected_error,
        );
        assert_eq!(&*before, &*d.encode(&ctx).unwrap());
    }
}
#[test]
fn rotating_tokens_compare_four_revisions_and_active_selector_without_partial_updates() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = fixture(&ctx);
    let ProfileBody::Named(data) = &mut d.profiles[0].data else {
        panic!("Code fixture must retain named identity")
    };
    data.organization = Some("old-org".into());
    let expectation = token_expectation(&d);
    let before = d.encode(&ctx).unwrap();
    for n in 0..4 {
        let mut e = expectation;
        match n {
            0 => e.document_revision += 1,
            1 => e.provider_revision += 1,
            2 => e.profile_revision += 1,
            _ => e.token_revision += 1,
        };
        let actual = error(d.rotate_tokens("broker-user", update(), e, 50, &ctx));
        assert_eq!(
            actual,
            if n == 1 {
                ProfileError::Stale
            } else {
                ProfileError::Conflict
            }
        );
        assert_eq!(&*d.encode(&ctx).unwrap(), &*before);
    }
    d.rotate_tokens("broker-user", update(), expectation, 50, &ctx)
        .unwrap();
    let ProfileCredentials::Broker { tokens, .. } = &d
        .profile("broker-user")
        .unwrap()
        .data()
        .expect("Local/Code fixture")
        .credentials
    else {
        panic!()
    };
    assert_eq!(tokens.access_token.expose(), "access-rotated");
    assert_eq!(
        tokens.refresh_token.as_ref().unwrap().expose(),
        "refresh-rotated"
    );
    assert_eq!(tokens.id_token.expose(), "id-sentinel");
    assert_eq!(tokens.expires_at, 200);
    assert_eq!(
        d.profile("broker-user")
            .unwrap()
            .data()
            .expect("Local/Code fixture")
            .username,
        "alice"
    );
    assert_eq!(
        d.profile("broker-user")
            .unwrap()
            .data()
            .expect("Code fixture")
            .organization
            .as_deref(),
        Some("old-org")
    );
    assert_eq!(
        d.profile("broker-user")
            .unwrap()
            .data()
            .expect("Local/Code fixture")
            .teams,
        ["engineering"]
    );
    assert_eq!(
        error(d.rotate_tokens("broker-user", update(), expectation, 60, &ctx)),
        ProfileError::Conflict
    );
    let expectation = token_expectation(&d);
    d.activate("local-user", d.revision(), &ctx).unwrap();
    let mut e = expectation;
    e.document_revision = d.revision();
    assert_eq!(
        error(d.rotate_tokens("broker-user", update(), e, 60, &ctx)),
        ProfileError::Conflict
    );
}
#[test]
fn revision_overflow_and_global_prospective_capacity_fail_before_mutation() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = fixture(&ctx);
    let mut v = value(&d, &ctx);
    v["document_revision"] = json!(u64::MAX);
    let mut loaded = decode_value(&v, &ctx).unwrap();
    assert_eq!(
        error(loaded.logout(None, u64::MAX, &ctx)),
        ProfileError::Limit
    );
    assert_eq!(loaded.profiles.len(), 2);
    v["providers"][0]["revision"] = json!(u64::MAX);
    let mut loaded = decode_value(&v, &ctx).unwrap();
    assert_eq!(
        error(loaded.update_provider("company", broker(), u64::MAX, u64::MAX, 99, &ctx)),
        ProfileError::Limit
    );
    let bytes = d.encode(&ctx).unwrap();
    let mut policy = limits();
    policy.bytes = bytes.len();
    let mut loaded = AuthDocument::decode(&bytes, policy, &ctx).unwrap();
    let before = loaded.encode(&ctx).unwrap();
    assert_eq!(
        error(loaded.add_provider("extra".into(), local(), loaded.revision(), 99, &ctx)),
        ProfileError::Limit
    );
    assert_eq!(&*before, &*loaded.encode(&ctx).unwrap());
    d.revision = u64::MAX;
    d.profiles[0].token_revision = u64::MAX;
    assert_eq!(
        error(d.rotate_tokens("broker-user", update(), token_expectation(&d), 50, &ctx)),
        ProfileError::Limit
    );
}
#[test]
fn ai_protocol_dimension_modes_and_local_platform_rules_preserved() {
    {
        let device = LocalDevice::Metal;
        assert!(
            LocalExecution {
                device,
                device_id: 0
            }
            .validate_platform(true)
            .is_ok()
        );
        assert!(
            LocalExecution {
                device,
                device_id: 0
            }
            .validate_platform(false)
            .is_err()
        );
        assert!(
            LocalExecution {
                device,
                device_id: 1
            }
            .validate_platform(true)
            .is_err()
        );
    }
    for device in [LocalDevice::Auto, LocalDevice::Cpu, LocalDevice::Cuda] {
        assert!(
            LocalExecution {
                device,
                device_id: 7
            }
            .validate_platform(false)
            .is_ok()
        );
    }
    assert!(
        LocalExecution {
            device: LocalDevice::Auto,
            device_id: 1
        }
        .validate_platform(true)
        .is_err()
    );
    for p in [
        DirectProtocol::OpenAi,
        DirectProtocol::OpenAiCompatible,
        DirectProtocol::Cohere,
        DirectProtocol::Voyage,
    ] {
        let mut cfg = local();
        cfg.embedding = AiService::Direct {
            protocol: p,
            endpoint: "https://ai.example/v1".into(),
            model: "model".into(),
            model_revision: "fixture-deployment-2026-10".into(),
            recipe: "fixture-vector-v1".into(),
            dimensions: 128,
            request_dimensions: None,
            batch_endpoint: None,
        };
        assert!(validate_config(&cfg).is_ok());
        if let AiService::Direct { dimensions, .. } = &mut cfg.embedding {
            *dimensions = 0;
        }
        assert!(validate_config(&cfg).is_err());
    }
    for p in [
        DirectProtocol::Cohere,
        DirectProtocol::CohereV2,
        DirectProtocol::Voyage,
        DirectProtocol::VoyageV1,
        DirectProtocol::Jina,
        DirectProtocol::JinaV1,
    ] {
        let mut cfg = local();
        cfg.rerank = AiService::Direct {
            protocol: p,
            endpoint: "https://ai.example".into(),
            model: "rerank".into(),
            model_revision: "fixture-deployment-2026-10".into(),
            recipe: "fixture-vector-v1".into(),
            dimensions: 0,
            request_dimensions: None,
            batch_endpoint: None,
        };
        assert!(validate_config(&cfg).is_ok());
        if let AiService::Direct { dimensions, .. } = &mut cfg.rerank {
            *dimensions = 1;
        }
        assert!(validate_config(&cfg).is_err());
    }
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut v = value(&fixture(&ctx), &ctx);
    v["providers"][1]["config"]["embedding"]["protocol"] = json!("openai");
    assert!(decode_value(&v, &ctx).is_err());
}
#[test]
fn direct_rerank_rejects_embedding_protocols_before_create_update_and_decode() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel, Instant::now() + Duration::from_secs(30));
    let base = fixture(&ctx);
    let bytes = base.encode(&ctx).unwrap();
    for (protocol, name) in [
        (DirectProtocol::OpenAi, "openai"),
        (DirectProtocol::OpenAiCompatible, "openai-compatible"),
        (DirectProtocol::GoogleGemini1, "google-gemini-1"),
        (DirectProtocol::GoogleGemini2, "google-gemini-2"),
    ] {
        let google = matches!(
            protocol,
            DirectProtocol::GoogleGemini1 | DirectProtocol::GoogleGemini2
        );
        let model = if google {
            "models/retrieval"
        } else {
            "embedding-model"
        };
        let endpoint = if google {
            "https://ai.example/v1/models/retrieval:embedContent"
        } else {
            "https://ai.example/v1/embeddings"
        };
        let batch =
            google.then(|| "https://ai.example/v1/models/retrieval:batchEmbedContents".to_owned());
        let service = AiService::Direct {
            protocol,
            endpoint: endpoint.into(),
            model: model.into(),
            model_revision: "deployment-1".into(),
            recipe: "retrieval-v1".into(),
            dimensions: 128,
            request_dimensions: None,
            batch_endpoint: batch.clone(),
        };
        let mut cfg = local();
        cfg.embedding = service.clone();
        cfg.validate_ai_services().unwrap();
        cfg.embedding = local().embedding;
        cfg.rerank = service;
        assert_eq!(
            cfg.validate_ai_services(),
            Err(ProfileError::InvalidDocument)
        );
        let mut doc = AuthDocument::decode(&bytes, limits(), &ctx).unwrap();
        assert_eq!(
            doc.add_provider("invalid".into(), cfg.clone(), doc.revision(), 50, &ctx),
            Err(ProfileError::InvalidDocument)
        );
        assert_eq!(
            doc.update_provider(
                "workstation",
                cfg,
                doc.revision(),
                doc.provider("workstation").unwrap().revision(),
                50,
                &ctx
            ),
            Err(ProfileError::InvalidDocument)
        );
        assert_eq!(&*doc.encode(&ctx).unwrap(), &*bytes);
        let mut raw = value(&base, &ctx);
        let providers = raw["providers"].as_array_mut().unwrap();
        let provider = providers
            .iter_mut()
            .find(|p| p["name"] == "workstation")
            .unwrap();
        provider["config"]["rerank"] = json!({
            "mode": "direct", "protocol": name, "endpoint": endpoint,
            "model": model, "model_revision": "deployment-1", "recipe": "retrieval-v1",
            "dimensions": 128, "request_dimensions": null, "batch_endpoint": batch,
        });
        assert_eq!(
            error(decode_value(&raw, &ctx)),
            ProfileError::InvalidDocument
        );
    }
}
#[test]
fn endpoints_mcp_resources_provider_boundaries_and_local_credentials_validation() {
    for url in [
        "https://example.org/v1",
        "http://localhost:8000",
        "http://127.0.0.1:8000",
        "http://[::1]:8000",
    ] {
        assert!(valid_url(url, false).is_ok(), "{url}");
    }
    for url in [
        "http://example.org",
        "https://user:pass@example.org",
        "ftp://example.org",
        " /relative",
        "https://example.org/#secret",
    ] {
        assert!(valid_url(url, false).is_err(), "{url}");
    }
    for url in [
        "https://example.org/mcp?x=1",
        "https://example.org/mcp#x",
        "https://example.org/mcp ",
    ] {
        assert!(valid_url(url, true).is_err());
    }
    let mut cfg = broker();
    let ProviderKind::Broker(b) = &mut cfg.kind else {
        panic!()
    };
    b.allow_anonymous = true;
    assert!(validate_config(&cfg).is_err());
    let mut cfg = local();
    cfg.s3.source = S3Source::BrokerScope;
    assert!(validate_config(&cfg).is_err());
    cfg.s3.source = S3Source::AwsChain;
    assert!(validate_config(&cfg).is_err());
    cfg.kind = ProviderKind::Local {
        allow_aws_credential_chain: true,
        broker: None,
    };
    assert!(validate_config(&cfg).is_ok());
    let mut cfg = local();
    cfg.kind = ProviderKind::Local {
        allow_aws_credential_chain: false,
        broker: Some(BrokerService {
            endpoint: "https://broker.example".into(),
            mcp_resource: None,
            allow_anonymous: false,
        }),
    };
    cfg.embedding = AiService::Broker;
    cfg.rerank = AiService::Broker;
    assert!(validate_data(&local_data("local-user", "workstation"), Some(&cfg)).is_err());
    if let ProviderKind::Local {
        broker: Some(b), ..
    } = &mut cfg.kind
    {
        b.allow_anonymous = true;
    }
    assert!(validate_data(&local_data("local-user", "workstation"), Some(&cfg)).is_ok());
    let mut data = local_data("local-user", "workstation");
    let ProfileCredentials::Local(l) = &mut data.credentials else {
        panic!()
    };
    l.s3.as_mut().unwrap().secret_access_key = None;
    assert!(validate_data(&data, None).is_err());
    assert!(validate_data(&broker_data("broker-user"), Some(&local())).is_err());
}
#[test]
fn stale_credentials_survive_provider_kind_changes_as_data_only() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = fixture(&ctx);
    d.activate("local-user", d.revision(), &ctx).unwrap();
    d.update_provider(
        "workstation",
        broker(),
        d.revision(),
        d.provider("workstation").unwrap().revision(),
        50,
        &ctx,
    )
    .unwrap();
    let loaded = AuthDocument::decode(&d.encode(&ctx).unwrap(), limits(), &ctx).unwrap();
    assert!(matches!(
        loaded
            .profile("local-user")
            .unwrap()
            .data()
            .expect("Local/Code fixture")
            .credentials,
        ProfileCredentials::Local(_)
    ));
    assert_eq!(error(loaded.active(&ctx)), ProfileError::Stale);
}
#[test]
fn failures_do_not_echo_sensitive_input() {
    assert!(
        !ProfileError::InvalidDocument
            .to_string()
            .contains("sentinel")
    );
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut v = value(&fixture(&ctx), &ctx);
    v["profiles"][0]["data"]["credentials"]["tokens"]["access_token"] =
        json!("secret-sentinel\ninvalid");
    assert_eq!(
        error(decode_value(&v, &ctx)).to_string(),
        "invalid auth profile document"
    );
}

#[test]
#[ignore = "executed only by bounded writer failure subprocess harness"]
fn writer_error_child() {
    codec::writer_failure_case(&std::env::var("GRAPHIT_AUTH_WRITER_CASE").unwrap());
}
#[test]
fn writer_errors_return_without_serde_retry_on_cancel_deadline_or_limit() {
    for case in ["limit", "cancelled", "deadline"] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "profile::tests::writer_error_child",
                "--ignored",
                "--nocapture",
            ])
            .env("GRAPHIT_AUTH_WRITER_CASE", case)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "writer fixture {case}");
                break;
            }
            if Instant::now() >= until {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("writer retry loop {case}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn owned_input_caps_and_prospective_node_and_token_byte_limits_leave_state_unchanged() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let d = fixture(&ctx);
    let bytes = d.encode(&ctx).unwrap();
    let mut policy = limits();
    policy.nodes = nodes(&value(&d, &ctx));
    let mut loaded = AuthDocument::decode(&bytes, policy, &ctx).unwrap();
    let mut data = broker_data("broker-user");
    data.teams.push("additional".into());
    let expectation = ProfileExpectation {
        document_revision: loaded.revision(),
        provider_revision: loaded.provider("company").unwrap().revision(),
        profile_revision: Some(loaded.profile("broker-user").unwrap().revision()),
    };
    assert_eq!(
        error(loaded.replace_profile_data(data, expectation, 50, &ctx)),
        ProfileError::Limit
    );
    assert_eq!(&*loaded.encode(&ctx).unwrap(), &*bytes);
    let mut policy = limits();
    policy.bytes = bytes.len();
    let mut loaded = AuthDocument::decode(&bytes, policy, &ctx).unwrap();
    let expectation = token_expectation(&loaded);
    let mut update = update();
    update.access_token = secret(&"x".repeat(3000));
    assert_eq!(
        error(loaded.rotate_tokens("broker-user", update, expectation, 50, &ctx)),
        ProfileError::Limit
    );
    assert_eq!(&*loaded.encode(&ctx).unwrap(), &*bytes);
    let mut oversized = local();
    oversized.s3.endpoint = Some(format!("https://example.org/{}", "x".repeat(4096)));
    assert_eq!(
        error(loaded.add_provider("new".into(), oversized, loaded.revision(), 50, &ctx)),
        ProfileError::Limit
    );
    assert_eq!(&*loaded.encode(&ctx).unwrap(), &*bytes);
    let mut data = broker_data("broker-user");
    data.teams = (0..65).map(|n| format!("team-{n}")).collect();
    let expectation = ProfileExpectation {
        document_revision: loaded.revision(),
        provider_revision: loaded.provider("company").unwrap().revision(),
        profile_revision: Some(loaded.profile("broker-user").unwrap().revision()),
    };
    assert_eq!(
        error(loaded.replace_profile_data(data, expectation, 50, &ctx)),
        ProfileError::Limit
    );
    assert_eq!(&*loaded.encode(&ctx).unwrap(), &*bytes);
}

#[test]
fn local_static_broker_key_is_explicit_and_roundtrips_in_broker_ai_mode() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = AuthDocument::new(limits(), &ctx).unwrap();
    let mut config = local();
    config.kind = ProviderKind::Local {
        allow_aws_credential_chain: false,
        broker: Some(BrokerService {
            endpoint: "https://broker.example".into(),
            mcp_resource: None,
            allow_anonymous: false,
        }),
    };
    config.embedding = AiService::Broker;
    config.rerank = AiService::Broker;
    d.add_provider("automation".into(), config, 1, 10, &ctx)
        .unwrap();
    let mut data = local_data("ci", "automation");
    let ProfileCredentials::Local(c) = &mut data.credentials else {
        panic!()
    };
    c.broker_key = Some(secret("static-broker-sentinel"));
    replace(&mut d, data, 20, &ctx);
    assert_eq!(error(d.active(&ctx)), ProfileError::NoActive);
    let loaded = AuthDocument::decode(&d.encode(&ctx).unwrap(), limits(), &ctx).unwrap();
    let ProfileCredentials::Local(c) = &loaded
        .profile("ci")
        .unwrap()
        .data()
        .expect("Local/Code fixture")
        .credentials
    else {
        panic!()
    };
    assert_eq!(
        c.broker_key.as_ref().unwrap().expose(),
        "static-broker-sentinel"
    );
}

#[test]
fn maximum_document_revision_provider_cas_and_current_credential_mismatch_are_denied() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = fixture(&ctx);
    let before = d.encode(&ctx).unwrap();
    assert_eq!(
        error(d.update_provider("company", broker(), d.revision(), 99, 50, &ctx)),
        ProfileError::Conflict
    );
    assert_eq!(&*d.encode(&ctx).unwrap(), &*before);
    let mut v = value(&d, &ctx);
    v["document_revision"] = json!(u64::MAX);
    v["profiles"][0]["revision"] = json!(u64::MAX);
    let mut loaded = decode_value(&v, &ctx).unwrap();
    let expectation = ProfileExpectation {
        document_revision: loaded.revision(),
        provider_revision: loaded.provider("company").unwrap().revision(),
        profile_revision: Some(u64::MAX),
    };
    assert_eq!(
        error(loaded.replace_profile_data(broker_data("broker-user"), expectation, 50, &ctx)),
        ProfileError::Limit
    );
    assert_eq!(
        error(loaded.rotate_tokens(
            "broker-user",
            update(),
            token_expectation(&loaded),
            50,
            &ctx
        )),
        ProfileError::Limit
    );
    let mut v = value(&d, &ctx);
    v["profiles"][0]["data"]["provider"] = json!("workstation");
    v["profiles"][0]["provider_revision"] = json!(d.provider("workstation").unwrap().revision());
    assert_eq!(error(decode_value(&v, &ctx)), ProfileError::InvalidDocument);
    let mut v = value(&d, &ctx);
    v["profiles"][0]["provider_revision"] = json!(0);
    assert_eq!(error(decode_value(&v, &ctx)), ProfileError::InvalidDocument);
}

#[test]
fn explicit_local_anonymous_assertion_roundtrips_and_cannot_claim_named_identity() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = AuthDocument::new(limits(), &ctx).unwrap();
    let mut config = local();
    config.kind = ProviderKind::Local {
        allow_aws_credential_chain: false,
        broker: Some(BrokerService {
            endpoint: "https://broker.example".into(),
            mcp_resource: None,
            allow_anonymous: true,
        }),
    };
    config.embedding = AiService::Broker;
    config.rerank = AiService::Broker;
    d.add_provider("public".into(), config.clone(), 1, 10, &ctx)
        .unwrap();
    let mut data = local_data("public-user", "public");
    data.username = "anonymous".into();
    data.organization = None;
    data.teams.clear();
    let ProfileCredentials::Local(c) = &mut data.credentials else {
        panic!()
    };
    c.anonymous = true;
    c.s3 = None;
    c.mcp_key = None;
    c.embedding_api_key = None;
    c.rerank_api_key = None;
    replace(&mut d, data, 20, &ctx);
    let mut v = value(&d, &ctx);
    assert_eq!(
        v["profiles"][0]["data"]["credentials"]["local"]["anonymous"],
        true
    );
    assert!(decode_value(&v, &ctx).is_ok());
    v["profiles"][0]["data"]["username"] = json!("alice");
    assert_eq!(error(decode_value(&v, &ctx)), ProfileError::InvalidDocument);
    let mut v = value(&d, &ctx);
    v["profiles"][0]["data"]["teams"] = json!(["admin"]);
    assert_eq!(error(decode_value(&v, &ctx)), ProfileError::InvalidDocument);
    let mut v = value(&d, &ctx);
    v["providers"][0]["config"]["kind"]["broker"]["allow_anonymous"] = json!(false);
    assert_eq!(error(decode_value(&v, &ctx)), ProfileError::InvalidDocument);
    let mut v = value(&d, &ctx);
    v["profiles"][0]["data"]["credentials"]["local"]["broker_key"] = json!("static-key");
    assert_eq!(error(decode_value(&v, &ctx)), ProfileError::InvalidDocument);
}

#[test]
fn lifecycle_fence_first_activation_and_unrelated_edits_preserve_own_bindings() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut d = fixture(&ctx);
    d.active_profile = None;
    let original = d.profile("local-user").unwrap();
    let (revision, token, provider, created, updated) = (
        original.revision(),
        original.token_revision(),
        original.provider_revision(),
        original.created_at(),
        original.updated_at(),
    );
    d.activate("local-user", d.revision(), &ctx).unwrap();
    let (_, p) = d.active(&ctx).unwrap();
    assert_eq!(
        (
            p.revision(),
            p.token_revision(),
            p.provider_revision(),
            p.created_at(),
            p.updated_at()
        ),
        (revision + 1, token, provider, created, updated)
    );
    let expected = ProfileExpectation {
        document_revision: d.revision(),
        provider_revision: provider,
        profile_revision: Some(revision + 1),
    };
    replace(&mut d, broker_data("broker-user"), 50, &ctx);
    d.update_provider(
        "company",
        broker(),
        d.revision(),
        d.provider("company").unwrap().revision(),
        60,
        &ctx,
    )
    .unwrap();
    let (_, p) = d.active(&ctx).unwrap();
    assert_eq!(
        (p.revision(), p.token_revision(), p.provider_revision()),
        (revision + 1, token, provider)
    );
    assert_eq!(
        error(d.replace_profile_data(local_data("local-user", "workstation"), expected, 60, &ctx)),
        ProfileError::Conflict
    );
    d.replace_profile_data(
        local_data("local-user", "workstation"),
        ProfileExpectation {
            document_revision: d.revision(),
            ..expected
        },
        60,
        &ctx,
    )
    .unwrap();
    assert_eq!(d.profile("local-user").unwrap().revision(), revision + 2);
}

#[test]
fn lifecycle_fence_codec_rejects_future_own_revisions_and_accepts_exact_upper_bound() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let d = fixture(&ctx);
    let original = value(&d, &ctx);
    for field in ["revision", "token_revision", "provider_revision"] {
        let mut v = original.clone();
        v["profiles"][0][field] = json!(d.revision() + 1);
        assert_eq!(error(decode_value(&v, &ctx)), ProfileError::InvalidDocument);
        v["profiles"][0][field] = json!(d.revision());
        let loaded = decode_value(&v, &ctx).unwrap();
        loaded.encode(&ctx).unwrap();
    }
    let mut v = original;
    v["providers"][0]["revision"] = json!(d.revision() + 1);
    assert_eq!(error(decode_value(&v, &ctx)), ProfileError::InvalidDocument);
    v["providers"][0]["revision"] = json!(d.revision());
    decode_value(&v, &ctx).unwrap().encode(&ctx).unwrap();
    for field in 0..4 {
        let mut invalid = fixture(&ctx);
        match field {
            0 => invalid.providers[0].revision = invalid.revision() + 1,
            1 => invalid.profiles[0].revision = invalid.revision() + 1,
            2 => invalid.profiles[0].token_revision = invalid.revision() + 1,
            _ => invalid.profiles[0].provider_revision = invalid.revision() + 1,
        }
        assert_eq!(error(invalid.encode(&ctx)), ProfileError::InvalidDocument);
    }
}

#[test]
fn lifecycle_fence_valid_maximum_document_rejects_all_mutations_before_effects() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut v = value(&fixture(&ctx), &ctx);
    v["document_revision"] = json!(u64::MAX);
    for provider in v["providers"].as_array_mut().unwrap() {
        provider["revision"] = json!(u64::MAX);
    }
    for profile in v["profiles"].as_array_mut().unwrap() {
        for field in ["revision", "token_revision", "provider_revision"] {
            profile[field] = json!(u64::MAX);
        }
    }
    let mut d = decode_value(&v, &ctx).unwrap();
    let before = d.encode(&ctx).unwrap();
    assert_eq!(
        error(d.activate("local-user", u64::MAX, &ctx)),
        ProfileError::Limit
    );
    assert_eq!(error(d.logout(None, u64::MAX, &ctx)), ProfileError::Limit);
    assert_eq!(
        error(d.add_provider("new".into(), local(), u64::MAX, 60, &ctx)),
        ProfileError::Limit
    );
    assert_eq!(
        error(d.update_provider("company", broker(), u64::MAX, u64::MAX, 60, &ctx)),
        ProfileError::Limit
    );
    assert_eq!(
        error(d.replace_profile_data(
            broker_data("broker-user"),
            ProfileExpectation {
                document_revision: u64::MAX,
                provider_revision: u64::MAX,
                profile_revision: Some(u64::MAX)
            },
            60,
            &ctx
        )),
        ProfileError::Limit
    );
    assert_eq!(
        error(d.rotate_tokens("broker-user", update(), token_expectation(&d), 60, &ctx)),
        ProfileError::Limit
    );
    assert_eq!(&*before, &*d.encode(&ctx).unwrap());
}

#[test]
fn lifecycle_fence_activation_digit_growth_is_counted_before_mutation() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut v = value(&fixture(&ctx), &ctx);
    v["document_revision"] = json!(9);
    v["profiles"][1]["revision"] = json!(9);
    let mut future = decode_value(&v, &ctx).unwrap();
    let original = future.encode(&ctx).unwrap();
    future.activate("local-user", 9, &ctx).unwrap();
    let exact = future.encode(&ctx).unwrap().len();
    assert!(exact > original.len());
    let token = future.profile("local-user").unwrap().token_revision();
    for cap in [exact - 1, exact] {
        let mut policy = limits();
        policy.bytes = cap;
        let mut d = AuthDocument::decode(&original, policy, &ctx).unwrap();
        let result = d.activate("local-user", 9, &ctx);
        if cap == exact {
            result.unwrap();
            assert_eq!(d.profile("local-user").unwrap().revision(), 10);
            assert_eq!(d.profile("local-user").unwrap().token_revision(), token);
            assert_eq!(d.encode(&ctx).unwrap().len(), exact);
        } else {
            assert_eq!(error(result), ProfileError::Limit);
            assert_eq!(&*d.encode(&ctx).unwrap(), &*original);
        }
    }
}

#[test]
fn lifecycle_fence_activation_uses_explicit_capacity_and_original_cancel_and_deadline() {
    let c = Cancellation::default();
    let baseline = context(&c, Instant::now() + Duration::from_secs(30));
    let mut v = value(&fixture(&baseline), &baseline);
    v["document_revision"] = json!(9);
    v["profiles"][1]["revision"] = json!(9);
    let mut future = decode_value(&v, &baseline).unwrap();
    let original = future.encode(&baseline).unwrap();
    future.activate("local-user", 9, &baseline).unwrap();
    let exact = future.encode(&baseline).unwrap().len();
    for scenario in 0..3 {
        let cancellation = Cancellation::default();
        let request = RequestContext::new(
            ContentLimits {
                max_input_bytes: 128 * 1024,
                max_output_bytes: 128 * 1024,
                max_depth: 32,
                max_nodes: 8192,
                max_scalar_bytes: 4096,
                max_collection_entries: 64,
            },
            &cancellation,
            if scenario == 2 {
                Instant::now() + Duration::from_millis(2)
            } else {
                Instant::now() + Duration::from_secs(30)
            },
        )
        .unwrap();
        let mut policy = limits();
        if scenario == 0 {
            policy.bytes = exact - 1;
        }
        let mut d = AuthDocument::decode(&original, policy, &baseline).unwrap();
        if scenario == 1 {
            cancellation.cancel();
        } else if scenario == 2 {
            std::thread::sleep(Duration::from_millis(5));
        }
        let error_expected = match scenario {
            0 => ProfileError::Limit,
            1 => ProfileError::Cancelled,
            _ => ProfileError::Deadline,
        };
        assert_eq!(error(d.activate("local-user", 9, &request)), error_expected);
        assert_eq!(&*d.encode(&baseline).unwrap(), &*original);
    }
}

#[test]
fn lifecycle_fence_first_selector_nodes_are_admitted_before_own_revision_changes() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut v = value(&fixture(&ctx), &ctx);
    v.as_object_mut().unwrap().remove("active_profile");
    let mut policy = limits();
    policy.nodes = nodes(&v);
    let bytes = serde_json::to_vec(&v).unwrap();
    let mut d = AuthDocument::decode(&bytes, policy, &ctx).unwrap();
    let before = d.encode(&ctx).unwrap();
    assert_eq!(
        error(d.activate("local-user", d.revision(), &ctx)),
        ProfileError::Limit
    );
    assert_eq!(&*d.encode(&ctx).unwrap(), &*before);
    assert_eq!(d.active_name(), None);
}

#[test]
fn direct_openai_dimensions_roundtrip_and_invalid_options_reject() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    let mut config = local();
    config.embedding = AiService::Direct {
        protocol: DirectProtocol::OpenAi,
        endpoint: "https://ai.example/v1/embeddings".into(),
        model: "text-embedding-3-small".into(),
        model_revision: "fixture-deployment-2026-10".into(),
        recipe: "fixture-vector-v1".into(),
        dimensions: 512,
        request_dimensions: Some(512),
        batch_endpoint: None,
    };
    let mut document = AuthDocument::new(limits(), &ctx).unwrap();
    document
        .add_provider("ai".into(), config.clone(), document.revision(), 1, &ctx)
        .unwrap();
    let encoded = value(&document, &ctx);
    assert_eq!(
        encoded["providers"][0]["config"]["embedding"]["model_revision"],
        "fixture-deployment-2026-10"
    );
    assert_eq!(
        encoded["providers"][0]["config"]["embedding"]["recipe"],
        "fixture-vector-v1"
    );
    for field in ["model_revision", "recipe"] {
        let mut missing = encoded.clone();
        missing["providers"][0]["config"]["embedding"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(decode_value(&missing, &ctx).is_err());
        for invalid in [
            Value::Null,
            json!(""),
            json!(" revision "),
            json!("revision\n"),
            json!("x".repeat(limits().scalar_bytes + 1)),
        ] {
            let mut rejected = encoded.clone();
            rejected["providers"][0]["config"]["embedding"][field] = invalid;
            assert!(decode_value(&rejected, &ctx).is_err());
        }
    }
    assert_eq!(
        encoded["providers"][0]["config"]["embedding"]["request_dimensions"],
        512
    );
    assert_eq!(
        decode_value(&encoded, &ctx)
            .unwrap()
            .provider("ai")
            .unwrap()
            .config(),
        &config
    );
    for (field, invalid) in [
        ("request_dimensions", json!(0)),
        ("request_dimensions", json!(1536)),
        ("request_dimensions", json!(513)),
        ("request_dimensions", json!(-1)),
        ("request_dimensions", json!(4294967296u64)),
        ("protocol", json!("openai-compatible")),
        ("protocol", json!("openai-embeddings-v1")),
        ("model", json!("unsupported")),
        ("batch_endpoint", json!("https://ai.example/batch")),
    ] {
        let mut rejected = encoded.clone();
        rejected["providers"][0]["config"]["embedding"][field] = invalid;
        assert!(decode_value(&rejected, &ctx).is_err());
    }
    let mut absent = encoded;
    absent["providers"][0]["config"]["embedding"]
        .as_object_mut()
        .unwrap()
        .remove("request_dimensions");
    absent["providers"][0]["config"]["embedding"]["protocol"] = json!("openai-compatible");
    let decoded = decode_value(&absent, &ctx).unwrap();
    assert!(matches!(
        decoded.provider("ai").unwrap().config().embedding,
        AiService::Direct {
            request_dimensions: None,
            ..
        }
    ));
    assert!(
        value(&decoded, &ctx)["providers"][0]["config"]["embedding"]
            .get("request_dimensions")
            .is_none()
    );
}

#[test]
fn direct_google_families_capture_two_literal_routes_and_reject_missing_or_derived_data() {
    let c = Cancellation::default();
    let ctx = context(&c, Instant::now() + Duration::from_secs(30));
    for protocol in [DirectProtocol::GoogleGemini1, DirectProtocol::GoogleGemini2] {
        let mut config = local();
        config.embedding = AiService::Direct {
            protocol,
            endpoint: "https://ai.example/v1/models/retrieval:embedContent".into(),
            batch_endpoint: Some(
                "https://ai.example/v1/models/retrieval:batchEmbedContents".into(),
            ),
            model: "models/retrieval".into(),
            model_revision: "fixture-deployment-2026-10".into(),
            recipe: "fixture-vector-v1".into(),
            dimensions: 768,
            request_dimensions: None,
        };
        let mut document = AuthDocument::new(limits(), &ctx).unwrap();
        document
            .add_provider("ai".into(), config.clone(), document.revision(), 1, &ctx)
            .unwrap();
        let encoded = value(&document, &ctx);
        assert_eq!(
            decode_value(&encoded, &ctx)
                .unwrap()
                .provider("ai")
                .unwrap()
                .config(),
            &config
        );
        for field in ["batch_endpoint", "model", "endpoint", "protocol"] {
            let mut rejected = encoded.clone();
            rejected["providers"][0]["config"]["embedding"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(decode_value(&rejected, &ctx).is_err());
        }
        for (field, invalid) in [
            ("model", json!("retrieval")),
            ("model", json!("models/not/allowed")),
            ("model", json!("models/é")),
            ("model", json!("models/")),
            (
                "batch_endpoint",
                json!("https://foreign.example/v1/models/retrieval:batchEmbedContents"),
            ),
            (
                "batch_endpoint",
                json!("https://ai.example/v1/models/foreign:batchEmbedContents"),
            ),
            (
                "batch_endpoint",
                json!("https://ai.example/v1/models/retrieval:embedContent"),
            ),
            (
                "endpoint",
                json!("https://ai.example/v1/models/retrieval:embedContent?key=hidden"),
            ),
            ("protocol", json!("google")),
            ("request_dimensions", json!(768)),
            ("dimensions", json!(0)),
        ] {
            let mut rejected = encoded.clone();
            rejected["providers"][0]["config"]["embedding"][field] = invalid;
            assert!(decode_value(&rejected, &ctx).is_err());
        }
    }
}

#[test]
fn device_codec_roundtrip_presence_and_closed_token_shape() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel, Instant::now() + Duration::from_secs(10));
    let mut document = AuthDocument::new(limits(), &ctx).unwrap();
    let mut config = broker();
    let ProviderKind::Broker(service) = &mut config.kind else {
        unreachable!()
    };
    service.mcp_resource = None;
    document
        .add_provider(
            "device-provider".into(),
            config,
            document.revision(),
            1,
            &ctx,
        )
        .unwrap();
    document
        .replace_broker_device_profile(
            BrokerDeviceProfileData {
                name: "device".into(),
                provider: "device-provider".into(),
                issuer: "https://broker.example".into(),
                subject: "signed-subject".into(),
                tokens: BrokerDeviceTokens {
                    access_token: secret("access-only-canary"),
                    expires_at: 100,
                },
                s3_disabled: false,
                metadata: BrokerDeviceMetadata::default(),
            },
            ProfileExpectation {
                document_revision: document.revision(),
                provider_revision: document.provider("device-provider").unwrap().revision(),
                profile_revision: None,
            },
            2,
            &ctx,
        )
        .unwrap();
    let bytes = document.encode(&ctx).unwrap();
    let decoded = AuthDocument::decode(&bytes, limits(), &ctx).unwrap();
    assert_eq!(&*bytes, &*decoded.encode(&ctx).unwrap());
    assert!(decoded.profile("device").unwrap().data().is_none());
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    let data = &original["profiles"][0]["data"];
    assert!(data.get("username").is_none());
    assert!(data.get("teams").is_none());
    assert!(data["credentials"]["tokens"].get("id_token").is_none());
    for variant in 0..9 {
        let mut value = original.clone();
        let data = &mut value["profiles"][0]["data"];
        match variant {
            0 => data["username"] = Value::Null,
            1 => data["teams"] = Value::Null,
            2 => data["credentials"]["tokens"]["id_token"] = "forbidden".into(),
            3 => data["credentials"]["tokens"]["refresh_token"] = "forbidden".into(),
            4 => data["credentials"]["tokens"]["token_type"] = "bearer".into(),
            5 => data["credentials"]["kind"] = "broker".into(),
            6 => data["credentials"]["extra"] = true.into(),
            7 => data["organization"] = "".into(),
            8 => data["teams"] = json!([""]),
            _ => unreachable!(),
        }
        assert!(
            AuthDocument::decode(value.to_string().as_bytes(), limits(), &ctx).is_err(),
            "variant {variant}"
        );
    }
    let mut value = original;
    value["profiles"][0]["data"]["teams"] = json!([]);
    let decoded = AuthDocument::decode(value.to_string().as_bytes(), limits(), &ctx).unwrap();
    assert_eq!(
        decoded
            .profile("device")
            .unwrap()
            .broker_device_data()
            .unwrap()
            .metadata
            .teams
            .as_ref()
            .unwrap()
            .len(),
        0
    );
}
