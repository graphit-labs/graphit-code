use super::*;
#[path = "web_hub_mutation/catalog_write.rs"]
mod catalog_write;
use crate::{
    WebHubProjectMutationInput, WebHubPublicationInput,
    profile::{AuthDocument, ProfileError, ProfileLimits},
};

fn document_limits() -> ProfileLimits {
    ProfileLimits {
        bytes: 128 * 1024,
        nodes: 8192,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 32,
    }
}

fn issuer(f: &Fixture, ctx: &RequestContext<'_>, rules: &[ServiceRuleInput<'_>]) -> RoleIssuer {
    let policy = super::super::service_credentials::policy(
        rules,
        &[],
        limits(),
        ServiceAdmissionLimits {
            max_rules: 16,
            max_binding_bytes: 8192,
        },
        Duration::ZERO,
    )
    .unwrap();
    RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store_policy(ctx, document_limits()),
        policy,
        ttl(),
        ctx,
    )
    .unwrap()
}

fn principal(origin: &str, subject: &str, scopes: &[&str]) -> Value {
    json!({"provider":"broker","issuer":origin,"subject":subject,"scopes":scopes})
}

fn configure(issuer: &RoleIssuer, ctx: &RequestContext<'_>, entries: Value) {
    let result = issuer
        .local
        .mutate(issuer.local.load(ctx).unwrap(), ctx, |doc, ctx| {
            let mut value: Value = serde_json::from_slice(&doc.encode(ctx)?).unwrap();
            value["document_revision"] = json!(doc.revision() + 1);
            value["web_hub_administrators"] = entries;
            *doc =
                AuthDocument::decode(&serde_json::to_vec(&value).unwrap(), document_limits(), ctx)?;
            Ok(())
        })
        .unwrap();
    assert!(result.failure().is_none());
    assert!(result.cleanup_failure().is_none());
}

fn claims(origin: &str, resource: &str, client: &str, subject: &str) -> Value {
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    json!({"iss":origin,"sub":subject,"aud":["native-api",resource],
        "client_id":client,"token_use":"access","jti":"web-hub-effects","scope":"",
        "iat":now,"exp":now+600,"preferred_username":"display-only"})
}

fn decision(oauth: &OAuthFixture, subject: &str, action: &str, status: u16) {
    let mut value = reply(&oauth.origin);
    value["subject"]["subject"] = json!(subject);
    value["module"] = json!("hub");
    value["action"] = json!(action);
    *oauth.state.project_response.lock().unwrap() = (status, value.to_string());
}

#[test]
fn web_hub_private_policy_uses_closed_bounded_auth_document_schema() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let base = AuthDocument::new(document_limits(), &ctx).unwrap();
    let mut value: Value = serde_json::from_slice(&base.encode(&ctx).unwrap()).unwrap();
    assert!(
        AuthDocument::decode(&base.encode(&ctx).unwrap(), document_limits(), &ctx)
            .unwrap()
            .web_hub_administrators()
            .is_empty()
    );
    let entry = principal("https://broker.example", "web-a", &["project", "global"]);
    value["web_hub_administrators"] = json!([entry]);
    let bytes = serde_json::to_vec(&value).unwrap();
    let decoded = AuthDocument::decode(&bytes, document_limits(), &ctx).unwrap();
    let roundtrip: Value = serde_json::from_slice(&decoded.encode(&ctx).unwrap()).unwrap();
    assert_eq!(roundtrip, value);
    let mut invalid = vec![Value::Null, json!([entry.clone(), entry.clone()])];
    for (key, bad) in [
        ("provider", json!("")),
        ("issuer", json!("https://user:password@broker.example")),
        ("subject", json!("")),
        ("scopes", json!([])),
        ("scopes", json!(["project", "project"])),
        ("scopes", json!(["unknown"])),
        ("scopes", Value::Null),
        ("extra", json!(true)),
    ] {
        let mut changed = entry.clone();
        changed[key] = bad;
        invalid.push(json!([changed]));
    }
    for policy in invalid {
        let mut invalid = value.clone();
        invalid["web_hub_administrators"] = policy;
        assert!(
            AuthDocument::decode(
                &serde_json::to_vec(&invalid).unwrap(),
                document_limits(),
                &ctx
            )
            .is_err()
        );
    }
    let mut small = document_limits();
    small.scalar_bytes = "web_hub_administrators".len() - 1;
    assert!(matches!(
        AuthDocument::decode(&bytes, small, &ctx),
        Err(ProfileError::Limit)
    ));
    let duplicate = String::from_utf8(bytes.clone()).unwrap().replace(
        "\"subject\":\"web-a\"",
        "\"subject\":\"web-a\",\"subject\":\"web-b\"",
    );
    assert!(AuthDocument::decode(duplicate.as_bytes(), document_limits(), &ctx).is_err());
    let mut trailing = bytes;
    trailing.extend_from_slice(b"{}");
    assert!(AuthDocument::decode(&trailing, document_limits(), &ctx).is_err());
    cancel.cancel();
    assert!(matches!(decoded.encode(&ctx), Err(ProfileError::Cancelled)));
}

#[tokio::test]
async fn web_hub_mutation_matches_exact_principal_and_separates_project_global() {
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = issuer(&f, &ctx, &[]);
    let revision = add_provider(&issuer, &ctx, &oauth.origin);
    let verifier = verifier(&oauth.origin, revision);
    let flow = WebOAuth::new(
        &verifier,
        oauth_limits(),
        "https://workspace.example/auth/callback",
        false,
    )
    .unwrap();
    let context = BrokerRequestContext::new(ctx.snapshot());
    let registered_a = flow.register(context).await.unwrap();
    let registered_b = flow.register(context).await.unwrap();
    let raw_a = token(
        &key,
        "one",
        &claims(
            &oauth.origin,
            registered_a.resource(),
            registered_a.client_id(),
            "web-a",
        ),
    );
    let raw_b = token(
        &key,
        "one",
        &claims(
            &oauth.origin,
            registered_b.resource(),
            registered_b.client_id(),
            "web-b",
        ),
    );
    let a = flow
        .verify_access(&registered_a, &raw_a, context)
        .await
        .unwrap();
    let b = flow
        .verify_access(&registered_b, &raw_b, context)
        .await
        .unwrap();
    assert_ne!(a.subject(), b.subject());
    let access = backend(&format!("{}/v1/project/access/authorize", oauth.origin));
    let input = || WebHubProjectMutationInput {
        caller: &a,
        verifier: &verifier,
        original: &ctx,
        provider: "broker",
        scope: project_scope(&f),
        backend: &access,
    };
    // Hub/read by itself does not grant local FILE administration.
    decision(&oauth, "web-a", "read", 200);
    let calls = oauth.count("/v1/project/access/authorize");
    assert!(
        issuer
            .admit_web_hub_project_mutation(input())
            .await
            .is_err()
    );
    assert!(
        issuer
            .admit_web_hub_global_mutation(&a, &verifier, &ctx, "broker", f.realm())
            .is_err()
    );
    assert_eq!(oauth.count("/v1/project/access/authorize"), calls);
    configure(
        &issuer,
        &ctx,
        json!([principal(&oauth.origin, "web-a", &["global"])]),
    );
    let global = issuer
        .admit_web_hub_global_mutation(&a, &verifier, &ctx, "broker", f.realm())
        .unwrap();
    assert!(global.project().unwrap().is_none());
    global.recheck_for_effect().await.unwrap();
    assert!(
        issuer
            .admit_web_hub_project_mutation(input())
            .await
            .is_err()
    );
    assert!(
        issuer
            .admit_web_hub_global_mutation(&b, &verifier, &ctx, "broker", f.realm())
            .is_err()
    );
    assert_eq!(oauth.count("/v1/project/access/authorize"), calls);
    configure(
        &issuer,
        &ctx,
        json!([principal(&oauth.origin, "web-a", &["project"])]),
    );
    assert_eq!(global.recheck(), Err(AuthError::Denied));
    assert!(
        issuer
            .admit_web_hub_global_mutation(&a, &verifier, &ctx, "broker", f.realm())
            .is_err()
    );
    let mut wrong = input();
    wrong.caller = &b;
    assert!(issuer.admit_web_hub_project_mutation(wrong).await.is_err());
    assert_eq!(oauth.count("/v1/project/access/authorize"), calls);
    decision(&oauth, "web-a", "read", 403);
    assert!(
        issuer
            .admit_web_hub_project_mutation(input())
            .await
            .is_err()
    );
    // A write reply cannot substitute for the exact current Hub/read decision.
    decision(&oauth, "web-a", "write", 200);
    assert!(
        issuer
            .admit_web_hub_project_mutation(input())
            .await
            .is_err()
    );
    decision(&oauth, "web-a", "read", 400);
    assert!(
        issuer
            .admit_web_hub_project_mutation(input())
            .await
            .is_err()
    );
    decision(&oauth, "web-a", "read", 503);
    assert!(
        issuer
            .admit_web_hub_project_mutation(input())
            .await
            .is_err()
    );
    decision(&oauth, "web-a", "read", 200);
    let before_read = oauth.count("/v1/project/access/authorize");
    let admitted = issuer
        .admit_web_hub_project_mutation(input())
        .await
        .unwrap();
    assert_eq!(oauth.count("/v1/project/access/authorize"), before_read + 1);
    assert_eq!(
        admitted.project().unwrap().unwrap().version.version(),
        f.version.version()
    );
    assert!(std::ptr::eq(admitted.request(), &ctx));
    decision(&oauth, "web-a", "read", 403);
    assert!(admitted.recheck_for_effect().await.is_err());
    assert_eq!(admitted.recheck(), Ok(()));
    let foreign = request(&cancel);
    let mut wrong = input();
    wrong.original = &foreign;
    let calls = oauth.count("/v1/project/access/authorize");
    assert!(issuer.admit_web_hub_project_mutation(wrong).await.is_err());
    assert_eq!(oauth.count("/v1/project/access/authorize"), calls);
    configure(
        &issuer,
        &ctx,
        json!([principal(
            "https://different.example",
            "web-a",
            &["project", "global"]
        )]),
    );
    assert!(
        issuer
            .admit_web_hub_project_mutation(input())
            .await
            .is_err()
    );
    assert_eq!(oauth.count("/v1/project/access/authorize"), calls);
    assert!(
        flow.verify_access(&registered_a, "invalid-cookie-or-token", context)
            .await
            .is_err()
    );
    assert_eq!(oauth.count("/userinfo"), 0);
    assert_eq!(oauth.count("/token"), 0);
    assert!(
        issuer
            .local
            .load(&ctx)
            .unwrap()
            .document()
            .profiles()
            .is_empty()
    );
}

#[tokio::test]
async fn web_hub_mutation_rechecks_policy_physical_captures_cancel_and_deadline() {
    for ending in 0..5 {
        let key = pair();
        let oauth = OAuthFixture::new(&key).await;
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(1);
        let ctx = if ending == 4 {
            request_before(&cancel, deadline)
        } else {
            request(&cancel)
        };
        let f = Fixture::new(&ctx);
        let issuer = issuer(&f, &ctx, &[]);
        let revision = add_provider(&issuer, &ctx, &oauth.origin);
        configure(
            &issuer,
            &ctx,
            json!([principal(&oauth.origin, "web-a", &["project"])]),
        );
        let verifier = verifier(&oauth.origin, revision);
        let flow = WebOAuth::new(
            &verifier,
            oauth_limits(),
            "https://workspace.example/auth/callback",
            false,
        )
        .unwrap();
        let context = BrokerRequestContext::new(ctx.snapshot());
        let registered = flow.register(context).await.unwrap();
        let raw = token(
            &key,
            "one",
            &claims(
                &oauth.origin,
                registered.resource(),
                registered.client_id(),
                "web-a",
            ),
        );
        let caller = flow
            .verify_access(&registered, &raw, context)
            .await
            .unwrap();
        let backend = backend(&format!("{}/v1/project/access/authorize", oauth.origin));
        decision(&oauth, "web-a", "read", 200);
        let admission = issuer
            .admit_web_hub_project_mutation(WebHubProjectMutationInput {
                caller: &caller,
                verifier: &verifier,
                original: &ctx,
                provider: "broker",
                scope: project_scope(&f),
                backend: &backend,
            })
            .await
            .unwrap();
        match ending {
            0 => configure(&issuer, &ctx, json!([])),
            1 | 2 => {
                let root = if ending == 1 {
                    f.project.owner_root()
                } else {
                    f.installation.owner_root()
                };
                std::fs::rename(root, root.with_extension("replaced")).unwrap();
                std::fs::create_dir(root).unwrap();
            }
            3 => cancel.cancel(),
            _ => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
        }
        let calls = oauth.count("/v1/project/access/authorize");
        assert_eq!(
            admission.recheck(),
            Err(if ending < 3 {
                AuthError::Denied
            } else {
                AuthError::Request
            })
        );
        assert!(admission.recheck_for_effect().await.is_err());
        assert_eq!(oauth.count("/v1/project/access/authorize"), calls);
        assert_eq!(oauth.count("/userinfo"), 0);
    }
}

fn write_rule(origin: &str, purpose: ServicePurpose) -> ServiceRuleInput<'_> {
    ServiceRuleInput::Remote {
        origin: ServiceOrigin::Web,
        binding: RemoteServiceBindingInput::Broker {
            provider: "broker",
            purpose,
            endpoint: origin,
            resource: None,
        },
        credential: CredentialSource::IncomingBearer,
        incoming: None,
    }
}

#[tokio::test]
async fn web_hub_publication_reads_source_and_uses_own_web_hub_write_bearer() {
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let rules = [write_rule(&oauth.origin, ServicePurpose::HubWrite)];
    let issuer = issuer(&f, &ctx, &rules);
    let revision = add_provider(&issuer, &ctx, &oauth.origin);
    configure(
        &issuer,
        &ctx,
        json!([principal(&oauth.origin, "web-a", &["project"])]),
    );
    let verifier = verifier(&oauth.origin, revision);
    let flow = WebOAuth::new(
        &verifier,
        oauth_limits(),
        "https://workspace.example/auth/callback",
        false,
    )
    .unwrap();
    let context = BrokerRequestContext::new(ctx.snapshot());
    let registered = flow.register(context).await.unwrap();
    let raw = token(
        &key,
        "one",
        &claims(
            &oauth.origin,
            registered.resource(),
            registered.client_id(),
            "web-a",
        ),
    );
    let caller = flow
        .verify_access(&registered, &raw, context)
        .await
        .unwrap();
    let backend = backend(&format!("{}/v1/project/access/authorize", oauth.origin));
    let authority = ProjectRecordAuthority::new(&issuer);
    decision(&oauth, "web-a", "read", 200);
    let source = authority
        .admit_broker_read(BrokerRecordRead {
            caller: BrokerProjectCaller::Web(&caller),
            verifier: &verifier,
            backend: &backend,
            original: &ctx,
            scope: project_scope(&f),
            provider: "broker",
            module: ProjectReadModule::Hub,
        })
        .await
        .unwrap();
    decision(&oauth, "web-a", "write", 403);
    let before = oauth.count("/v1/project/access/authorize");
    let publication = issuer
        .admit_web_hub_publication(WebHubPublicationInput {
            caller: &caller,
            verifier: &verifier,
            original: &ctx,
            provider: "broker",
            installation: f.realm(),
            source: &source,
        })
        .unwrap();
    let mut selection = issuer
        .capture_web_hub_write_service(&publication, "broker")
        .unwrap();
    assert_eq!(selection.mode().unwrap(), ServiceMode::BrokerIncoming);
    let lease = selection.credential().unwrap();
    assert_eq!(lease.credential().unwrap(), Some(raw.as_str()));
    assert!(matches!(
        lease.binding().unwrap(),
        ServiceBindingView::Broker {
            purpose: ServicePurpose::HubWrite,
            provider: "broker",
            ..
        }
    ));
    assert_eq!(
        lease.user_subject().unwrap().unwrap().subject().unwrap(),
        "web-a"
    );
    assert_eq!(oauth.count("/v1/project/access/authorize"), before);
    assert!(
        issuer
            .capture_web_hub_write_service(&publication, "unknown")
            .is_err()
    );
    assert!(
        issuer
            .capture_web_hub_write_service(&publication, "")
            .is_err()
    );
    for mismatch in 0..4 {
        let mut rule = write_rule(&oauth.origin, ServicePurpose::HubWrite);
        if let ServiceRuleInput::Remote {
            binding:
                RemoteServiceBindingInput::Broker {
                    endpoint,
                    resource,
                    purpose,
                    ..
                },
            credential,
            ..
        } = &mut rule
        {
            match mismatch {
                0 => *purpose = ServicePurpose::HubRead,
                1 => *endpoint = "https://different.example",
                2 => *resource = Some("https://different.example/mcp"),
                _ => *credential = CredentialSource::NativeAccess,
            }
        }
        if mismatch == 3 {
            assert!(matches!(
                super::super::service_credentials::policy(
                    &[rule],
                    &[],
                    limits(),
                    ServiceAdmissionLimits {
                        max_rules: 16,
                        max_binding_bytes: 8192
                    },
                    Duration::ZERO,
                ),
                Err(AuthError::Invalid)
            ));
            continue;
        }
        let denied = self::issuer(&f, &ctx, &[rule]);
        // A proof from a different issuer instance cannot select its rules.
        assert!(
            denied
                .capture_web_hub_write_service(&publication, "broker")
                .is_err()
        );
        let read_authority = ProjectRecordAuthority::new(&denied);
        decision(&oauth, "web-a", "read", 200);
        let source = read_authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(&caller),
                verifier: &verifier,
                backend: &backend,
                original: &ctx,
                scope: project_scope(&f),
                provider: "broker",
                module: ProjectReadModule::Hub,
            })
            .await
            .unwrap();
        let publication = denied
            .admit_web_hub_publication(WebHubPublicationInput {
                caller: &caller,
                verifier: &verifier,
                original: &ctx,
                provider: "broker",
                installation: f.realm(),
                source: &source,
            })
            .unwrap();
        assert!(
            denied
                .capture_web_hub_write_service(&publication, "broker")
                .is_err()
        );
    }
    assert_eq!(oauth.count("/userinfo"), 0);
    assert_eq!(oauth.count("/token"), 0);
    assert!(
        issuer
            .local
            .load(&ctx)
            .unwrap()
            .document()
            .profiles()
            .is_empty()
    );
    cancel.cancel();
    assert!(lease.credential().is_err());
    assert_eq!(publication.recheck(), Err(AuthError::Request));
}
