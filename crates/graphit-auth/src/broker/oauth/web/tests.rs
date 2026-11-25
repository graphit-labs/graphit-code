use super::*;
use crate::broker::oauth::web::{RegisteredWebClient, WebOAuth};

const CALLBACK: &str = "https://workspace.example/auth/callback";

fn web_response(
    fixture: &Fixture,
    key: &Ed25519KeyPair,
    registered: &RegisteredWebClient,
    nonce: Option<&str>,
    refresh: &str,
    edit: impl FnOnce(&mut Value, &mut Value),
) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut access = json!({"iss":fixture.origin,"sub":"native-subject","client_id":registered.client_id(),
        "token_use":"access","jti":"web-jwt","aud":["native-api",registered.resource()],
        "iat":now,"exp":now+3600,"preferred_username":"web-user"});
    let mut id = json!({"iss":fixture.origin,"sub":"native-subject","aud":registered.client_id(),
        "iat":now,"exp":now+3600,"nonce":nonce,"preferred_username":"web-user"});
    edit(&mut access, &mut id);
    let mut body = json!({"access_token":token(key,"one",&access),"token_type":"Bearer","expires_in":300,"refresh_token":refresh});
    if nonce.is_some() {
        body["id_token"] = token(key, "one", &id).into();
    }
    *fixture.state.token.lock().unwrap() = body.to_string();
}
fn web_callback(registered: &RegisteredWebClient, state: &str) -> String {
    let mut url = Url::parse(registered.callback_uri()).unwrap();
    url.query_pairs_mut()
        .append_pair("code", "code")
        .append_pair("state", state);
    url.to_string()
}
fn login_fields(
    pending: &crate::broker::oauth::web::PendingWebLogin<'_>,
    ctx: BrokerRequestContext<'_>,
) -> BTreeMap<String, String> {
    let url = Url::parse(pending.authorization_url(ctx).unwrap()).unwrap();
    url.query_pairs().into_owned().collect()
}

#[tokio::test]
async fn registered_https_login_refresh_local_access_and_revoke() {
    registered_login_refresh_local_access_and_revoke(CALLBACK, false, "web").await;
}

#[tokio::test]
async fn registered_loopback_login_refresh_local_access_and_revoke() {
    for callback in [
        "http://127.0.0.1:38081/auth/callback",
        "http://[::1]:38081/auth/callback",
    ] {
        registered_login_refresh_local_access_and_revoke(callback, true, "native").await;
    }
}

async fn registered_login_refresh_local_access_and_revoke(
    callback: &str,
    allow_loopback_http: bool,
    application_type: &str,
) {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    *fixture.state.registration.lock().unwrap() = json!({
        "client_id":"web-client", "application_type":application_type,
        "redirect_uris":[callback], "token_endpoint_auth_method":"none"
    });
    let verifier = fixture.verifier(false);
    let flow = WebOAuth::new(&verifier, flow_limits(), callback, allow_loopback_http).unwrap();
    let cancel = Cancellation::default();
    let original = context(&cancel);
    let ctx = BrokerRequestContext::new(&original);
    let registered = flow.register(ctx).await.unwrap();
    assert_eq!(registered.client_id(), "web-client");
    assert_eq!(registered.resource(), format!("{}/v1", fixture.origin));
    let registration = fixture.state.forms.lock().unwrap()[0].1.clone();
    let registration: Value = serde_json::from_str(&registration).unwrap();
    assert_eq!(registration["redirect_uris"], json!([callback]));
    assert_eq!(registration["application_type"], application_type);
    assert_eq!(registration["token_endpoint_auth_method"], "none");
    let pending = flow.begin(&registered, ctx).await.unwrap();
    let fields = login_fields(&pending, ctx);
    assert_eq!(fields["client_id"], "web-client");
    assert_eq!(fields["resource"], registered.resource());
    assert_eq!(fields["code_challenge_method"], "S256");
    web_response(
        &fixture,
        &key,
        &registered,
        Some(&fields["nonce"]),
        "refresh-one",
        |_, _| {},
    );
    let credentials = flow
        .exchange(pending, &web_callback(&registered, &fields["state"]), ctx)
        .await
        .unwrap();
    assert_eq!(credentials.subject(), "native-subject");
    assert_eq!(credentials.profile_claims().unwrap().username(), "web-user");
    credentials.check(&flow, &registered, ctx).unwrap();
    let form = pairs(&fixture.state.forms.lock().unwrap().last().unwrap().1);
    assert_eq!(form["client_id"], registered.client_id());
    assert_eq!(form["resource"], registered.resource());
    assert_eq!(form["redirect_uri"], callback);
    let challenge = PkceCodeChallenge::from_code_verifier_sha256(&PkceCodeVerifier::new(
        form["code_verifier"].clone(),
    ));
    assert_eq!(challenge.as_str(), fields["code_challenge"]);
    let first = flow
        .verify_access(&registered, credentials.access_token(), ctx)
        .await
        .unwrap();
    assert_eq!(first.subject(), "native-subject");
    first.check(&verifier, ctx).unwrap();
    let later = context(&Cancellation::default());
    let laterctx = BrokerRequestContext::new(&later);
    assert!(first.check(&verifier, laterctx).is_err());
    flow.verify_access(&registered, credentials.access_token(), laterctx)
        .await
        .unwrap();
    assert_eq!(fixture.count("/userinfo"), 0);
    web_response(&fixture, &key, &registered, None, "refresh-two", |_, _| {});
    let refreshed = flow
        .refresh(
            &registered,
            credentials.refresh_token().unwrap(),
            credentials.subject(),
            laterctx,
        )
        .await
        .unwrap();
    assert_eq!(refreshed.refresh_token(), Some("refresh-two"));
    assert!(refreshed.profile_claims().is_none());
    flow.revoke(&registered, refreshed.refresh_token().unwrap(), laterctx)
        .await
        .unwrap();
    assert_eq!(fixture.count("/revoke"), 1);
    let form = pairs(&fixture.state.forms.lock().unwrap().last().unwrap().1);
    assert_eq!(form["client_id"], "web-client");
    assert_eq!(form["token"], "refresh-two");
    *fixture.state.userinfo_status.lock().unwrap() = 401;
    flow.verify_access(&registered, refreshed.access_token(), laterctx)
        .await
        .unwrap();
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn web_rejects_native_client_missing_api_resource_subject_and_nonce() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(false);
    let flow = WebOAuth::new(&verifier, flow_limits(), CALLBACK, false).unwrap();
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    let registered = flow.register(ctx).await.unwrap();
    for failure in 0..6 {
        let pending = flow.begin(&registered, ctx).await.unwrap();
        let fields = login_fields(&pending, ctx);
        web_response(
            &fixture,
            &key,
            &registered,
            Some(&fields["nonce"]),
            "fresh",
            |access, id| match failure {
                0 => {
                    access["client_id"] = "native-client".into();
                }
                1 => {
                    access["aud"] = json!(["native-api", fixture.resource]);
                }
                2 => {
                    id["aud"] = "native-client".into();
                }
                3 => {
                    id["sub"] = "foreign-subject".into();
                }
                4 => {
                    id["nonce"] = "foreign-nonce".into();
                }
                _ => {
                    access["aud"] = json!([registered.resource()]);
                }
            },
        );
        let error = flow
            .exchange(pending, &web_callback(&registered, &fields["state"]), ctx)
            .await
            .err()
            .unwrap();
        assert_eq!(
            error.kind(),
            NativeErrorKind::Broker(BrokerError::InvalidToken)
        );
        assert_eq!(error.effect(), RemoteEffect::MayHaveApplied);
    }
    assert_eq!(fixture.count("/token"), 6);
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn web_callback_original_and_provider_binding_cancel_before_effect() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(false);
    let flow = WebOAuth::new(&verifier, flow_limits(), CALLBACK, false).unwrap();
    let cancel = Cancellation::default();
    let original = context(&cancel);
    let ctx = BrokerRequestContext::new(&original);
    let registered = flow.register(ctx).await.unwrap();
    let pending = flow.begin(&registered, ctx).await.unwrap();
    let fields = login_fields(&pending, ctx);
    let other = context(&Cancellation::default());
    let otherctx = BrokerRequestContext::new(&other);
    assert!(pending.authorization_url(otherctx).is_err());
    let error = flow
        .exchange(
            pending,
            &web_callback(&registered, &fields["state"]),
            otherctx,
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.effect(), RemoteEffect::NotSent);
    assert_eq!(fixture.count("/token"), 0);
    for callback in [
        "https://foreign.example/auth/callback?code=c&state=x",
        "https://workspace.example/foreign?code=c&state=x",
        "https://workspace.example/auth/callback?code=c&state=x&state=y",
    ] {
        let pending = flow.begin(&registered, ctx).await.unwrap();
        assert_eq!(
            flow.exchange(pending, callback, ctx)
                .await
                .err()
                .unwrap()
                .effect(),
            RemoteEffect::NotSent
        );
    }
    let mut changed = fixture.verifier(false);
    changed.revision = 8;
    let changedflow = WebOAuth::new(&changed, flow_limits(), CALLBACK, false).unwrap();
    assert!(changedflow.begin(&registered, ctx).await.is_err());
    let pending = flow.begin(&registered, ctx).await.unwrap();
    let fields = login_fields(&pending, ctx);
    cancel.cancel();
    assert_eq!(
        flow.exchange(pending, &web_callback(&registered, &fields["state"]), ctx)
            .await
            .err()
            .unwrap()
            .kind(),
        NativeErrorKind::Broker(BrokerError::Cancelled)
    );
    assert_eq!(fixture.count("/token"), 0);
}

#[tokio::test]
async fn registration_metadata_and_redirect_contract_fail_without_native_relaxation() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    fixture
        .state
        .standard
        .lock()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("registration_endpoint");
    let verifier = fixture.verifier(false);
    let flow = WebOAuth::new(&verifier, flow_limits(), CALLBACK, false).unwrap();
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    assert_eq!(
        flow.register(ctx).await.err().unwrap().kind(),
        NativeErrorKind::Unsupported
    );
    assert!(
        WebOAuth::new(
            &verifier,
            flow_limits(),
            "http://workspace.example/auth/callback",
            true
        )
        .is_err()
    );
    assert!(
        WebOAuth::new(
            &verifier,
            flow_limits(),
            "https://u@workspace.example/auth/callback",
            false
        )
        .is_err()
    );
    assert!(
        WebOAuth::new(
            &verifier,
            flow_limits(),
            "https://workspace.example/auth/callback?override=1",
            false
        )
        .is_err()
    );
    assert!(
        WebOAuth::new(
            &verifier,
            flow_limits(),
            "http://127.0.0.1:12345/auth/callback",
            true
        )
        .is_ok()
    );
    let native = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    assert!(native.begin(CALLBACK, ctx).await.is_err());
    assert!(
        native
            .begin("http://127.0.0.1:50/callback", ctx)
            .await
            .is_ok()
    );

    let second = Fixture::new(&key).await;
    *second.state.registration.lock().unwrap() =
        json!({"client_id":"web-client","redirect_uris":["https://foreign.example/callback"]});
    let verifier = second.verifier(false);
    let flow = WebOAuth::new(&verifier, flow_limits(), CALLBACK, false).unwrap();
    assert_eq!(
        flow.register(ctx).await.err().unwrap().kind(),
        NativeErrorKind::InvalidResponse
    );
    assert_eq!(second.count("/register"), 1);
}

#[tokio::test]
async fn web_login_and_access_accept_missing_optional_profile() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(false);
    let flow = WebOAuth::new(&verifier, flow_limits(), CALLBACK, false).unwrap();
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    let registered = flow.register(ctx).await.unwrap();
    let pending = flow.begin(&registered, ctx).await.unwrap();
    let fields = login_fields(&pending, ctx);
    web_response(
        &fixture,
        &key,
        &registered,
        Some(&fields["nonce"]),
        "refresh-one",
        |access, id| {
            access.as_object_mut().unwrap().remove("preferred_username");
            id.as_object_mut().unwrap().remove("preferred_username");
        },
    );
    let credentials = flow
        .exchange(pending, &web_callback(&registered, &fields["state"]), ctx)
        .await
        .unwrap();
    assert_eq!(credentials.subject(), "native-subject");
    assert!(credentials.profile_claims().is_none());
    let caller = flow
        .verify_access(&registered, credentials.access_token(), ctx)
        .await
        .unwrap();
    assert_eq!(caller.subject(), "native-subject");
    assert_eq!(caller.username(), None);
    caller.check(&verifier, ctx).unwrap();
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn registration_type_matches_the_validated_callback_without_relaxing_metadata() {
    let loopback = "http://127.0.0.1:38081/auth/callback";
    for (callback, allow_loopback_http, returned) in [
        (CALLBACK, false, Some(json!("native"))),
        (CALLBACK, false, Some(json!(null))),
        (CALLBACK, false, Some(json!(7))),
        (loopback, true, Some(json!("web"))),
        (loopback, true, Some(json!("unknown"))),
        (loopback, true, Some(json!(null))),
        (loopback, true, None),
    ] {
        let key = pair();
        let fixture = Fixture::new(&key).await;
        let mut response = json!({"client_id":"web-client",
            "redirect_uris":[callback],"token_endpoint_auth_method":"none"});
        if let Some(application_type) = returned {
            response["application_type"] = application_type;
        }
        *fixture.state.registration.lock().unwrap() = response;
        let verifier = fixture.verifier(false);
        let flow = WebOAuth::new(&verifier, flow_limits(), callback, allow_loopback_http).unwrap();
        let cancel = Cancellation::default();
        let original = context(&cancel);
        let error = flow
            .register(BrokerRequestContext::new(&original))
            .await
            .err()
            .unwrap();
        assert_eq!(error.kind(), NativeErrorKind::InvalidResponse);
        assert_eq!(error.effect(), RemoteEffect::MayHaveApplied);
        assert_eq!(fixture.count("/register"), 1);
        assert_eq!(fixture.count("/token"), 0);
    }
}

#[tokio::test]
async fn loopback_registration_still_requires_explicit_http_admission() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(false);
    for callback in [
        "http://127.0.0.1:38081/auth/callback",
        "http://[::1]:38081/auth/callback",
    ] {
        assert_eq!(
            WebOAuth::new(&verifier, flow_limits(), callback, false)
                .err()
                .unwrap()
                .kind(),
            NativeErrorKind::InvalidRedirect
        );
    }
    assert_eq!(
        WebOAuth::new(
            &verifier,
            flow_limits(),
            "http://workspace.example/auth/callback",
            true
        )
        .err()
        .unwrap()
        .kind(),
        NativeErrorKind::InvalidRedirect
    );
    assert_eq!(fixture.count("/register"), 0);
}
