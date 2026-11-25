use super::*;

fn enable_device(f: &Fixture) {
    let mut metadata = f.state.standard.lock().unwrap();
    metadata["device_authorization_endpoint"] = format!("{}/device/authorize", f.origin).into();
    metadata["grant_types_supported"]
        .as_array_mut()
        .unwrap()
        .push("urn:ietf:params:oauth:grant-type:device_code".into());
}
fn device_token(f: &Fixture, key: &Ed25519KeyPair, change: impl FnOnce(&mut Value)) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut claims = json!({"iss":f.origin,"sub":"device-subject","aud":"native-api","client_id":"native-client","token_use":"access","jti":"device-jti","iat":now,"exp":now+3600});
    change(&mut claims);
    *f.state.token.lock().unwrap() = json!({"access_token":token(key,"one",&claims),"token_type":"Bearer","expires_in":300,"scope":""}).to_string();
}
#[tokio::test]
async fn device_access_only_forms_signed_metadata_and_identity() {
    for present in [false, true] {
        let key = pair();
        let f = Fixture::new(&key).await;
        enable_device(&f);
        device_token(&f, &key, |claims| {
            if present {
                claims["preferred_username"] = "signed-user".into();
                claims["organization"] = "signed-org".into();
                claims["groups"] = json!([]);
            }
        });
        let verifier = f.verifier(false);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let cancel = Cancellation::default();
        let original = context(&cancel);
        let ctx = BrokerRequestContext::new(&original);
        let pending = flow.begin_device(ctx).await.unwrap();
        let prompt = pending.instructions(ctx).unwrap();
        assert_eq!(prompt.user_code(), "ABCD-EFGH");
        assert!(prompt.verification_uri().ends_with("/device"));
        let tokens = flow.receive_device(pending, ctx).await.unwrap();
        let identity = tokens.identity(ctx).unwrap();
        assert_eq!(identity.subject(), "device-subject");
        assert_eq!(identity.username(), present.then_some("signed-user"));
        assert_eq!(identity.organization(), present.then_some("signed-org"));
        assert_eq!(identity.access_teams().is_some(), present);
        assert!(
            tokens.expires_at(ctx).unwrap()
                < i64::try_from(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        + 301
                )
                .unwrap()
        );
        let forms = f.state.forms.lock().unwrap();
        assert_eq!(forms.len(), 2);
        assert_eq!(
            pairs(&forms[0].1),
            BTreeMap::from([("client_id".into(), "native-client".into())])
        );
        assert_eq!(
            pairs(&forms[1].1),
            BTreeMap::from([
                ("client_id".into(), "native-client".into()),
                (
                    "grant_type".into(),
                    "urn:ietf:params:oauth:grant-type:device_code".into()
                ),
                ("device_code".into(), "gb_dc_device_secret_canary".into())
            ])
        );
        assert_eq!(f.count("/userinfo"), 0);
    }
}
#[tokio::test]
async fn device_pending_and_slow_down_follow_sdk_intervals() {
    let key = pair();
    let f = Fixture::new(&key).await;
    enable_device(&f);
    device_token(&f, &key, |_| {});
    f.state.device_steps.lock().unwrap().extend([
        (400, json!({"error":"authorization_pending"}).to_string()),
        (400, json!({"error":"slow_down"}).to_string()),
    ]);
    let verifier = f.verifier(false);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let cancel = Cancellation::default();
    let original = SnapshotContext::new(Instant::now() + Duration::from_secs(20), cancel.clone());
    let ctx = BrokerRequestContext::new(&original);
    let pending = flow.begin_device(ctx).await.unwrap();
    flow.receive_device(pending, ctx).await.unwrap();
    let times = f.state.device_times.lock().unwrap();
    assert_eq!(times.len(), 3);
    assert!(times[1].duration_since(times[0]) >= Duration::from_millis(950));
    assert!(times[2].duration_since(times[1]) >= Duration::from_millis(5950));
}
#[tokio::test]
async fn device_transport_failure_is_terminal_without_second_post() {
    let key = pair();
    let f = Fixture::new(&key).await;
    enable_device(&f);
    f.state
        .disconnect_device
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let verifier = f.verifier(false);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let cancel = Cancellation::default();
    let original = context(&cancel);
    let ctx = BrokerRequestContext::new(&original);
    let pending = flow.begin_device(ctx).await.unwrap();
    let error = tokio::time::timeout(Duration::from_secs(1), flow.receive_device(pending, ctx))
        .await
        .unwrap()
        .err()
        .unwrap();
    assert_eq!(error.effect(), RemoteEffect::MayHaveApplied);
    assert_eq!(f.count("/token"), 1);
    assert!(!format!("{error}").contains("canary"));
}
#[tokio::test]
async fn device_cancel_during_post_or_sdk_sleep_stops_owned_future() {
    for during_post in [false, true] {
        let key = pair();
        let f = Fixture::new(&key).await;
        enable_device(&f);
        if during_post {
            *f.state.token_gate.lock().unwrap() = Some(Arc::new(Notify::new()));
        } else {
            *f.state.status.lock().unwrap() = 400;
            *f.state.token.lock().unwrap() = json!({"error":"authorization_pending"}).to_string();
        }
        let verifier = f.verifier(false);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let cancel = Cancellation::default();
        let original = context(&cancel);
        let ctx = BrokerRequestContext::new(&original);
        let pending = flow.begin_device(ctx).await.unwrap();
        let receive = flow.receive_device(pending, ctx);
        tokio::pin!(receive);
        tokio::select! { result=&mut receive=>panic!("unexpected completion: {}",result.is_ok()), _=f.wait("/token",1)=>{} }
        cancel.cancel();
        let error = receive.await.err().unwrap();
        assert_eq!(
            error.kind(),
            NativeErrorKind::Broker(BrokerError::Cancelled)
        );
        assert_eq!(f.count("/token"), 1);
        if let Some(gate) = f.state.token_gate.lock().unwrap().take() {
            gate.notify_one();
        }
    }
}
#[tokio::test]
async fn device_rejects_effective_scope_refresh_and_invalid_signed_binding() {
    for variant in 0..13 {
        let key = pair();
        let f = Fixture::new(&key).await;
        enable_device(&f);
        device_token(&f, &key, |claims| match variant {
            4 => claims["iss"] = "https://wrong.example".into(),
            5 => claims["sub"] = "".into(),
            6 => claims["aud"] = "wrong".into(),
            7 => claims["client_id"] = "other".into(),
            8 => claims["token_use"] = "id".into(),
            9 => claims["exp"] = 0.into(),
            _ => {}
        });
        let mut response: Value = serde_json::from_str(&f.state.token.lock().unwrap()).unwrap();
        match variant {
            0 => response["scope"] = "openid".into(),
            1 => response["refresh_token"] = "forbidden".into(),
            2 => response["token_type"] = "MAC".into(),
            3 => response["expires_in"] = 0.into(),
            10 => response["access_token"] = "broken.jwt".into(),
            12 => response["scope"] = json!(["openid"]),
            _ => {}
        }
        let mut text = response.to_string();
        if variant == 11 {
            text = text.replacen("{", "{\"scope\":\"\",", 1);
        }
        *f.state.token.lock().unwrap() = text;
        let verifier = f.verifier(false);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let cancel = Cancellation::default();
        let original = context(&cancel);
        let ctx = BrokerRequestContext::new(&original);
        let pending = flow.begin_device(ctx).await.unwrap();
        let error = flow.receive_device(pending, ctx).await.err().unwrap();
        assert_eq!(error.effect(), RemoteEffect::MayHaveApplied);
        assert_eq!(f.count("/token"), 1);
        assert_eq!(f.count("/userinfo"), 0);
    }
}
#[tokio::test]
async fn device_sdk_ignores_extensions_and_effectively_empty_scopes_without_new_proof() {
    for variant in 0..5 {
        let key = pair();
        let f = Fixture::new(&key).await;
        enable_device(&f);
        device_token(&f, &key, |_| {});
        let mut response: Value = serde_json::from_str(&f.state.token.lock().unwrap()).unwrap();
        response["unknown_extension"] = json!({"nested":["ignored"]});
        response["id_token"] = if variant == 0 {
            Value::Null
        } else {
            "not-an-id-proof".into()
        };
        response["preferred_username"] = "not-a-profile".into();
        response["refresh_token"] = Value::Null;
        match variant {
            0 => {
                response.as_object_mut().unwrap().remove("scope");
            }
            1 => response["scope"] = "".into(),
            2 => response["scope"] = "   ".into(),
            3 => response["scope"] = Value::Null,
            _ => {}
        }
        *f.state.token.lock().unwrap() = response.to_string();
        let mut authorization: Value =
            serde_json::from_str(&f.state.device_response.lock().unwrap()).unwrap();
        authorization["extension"] = true.into();
        *f.state.device_response.lock().unwrap() = authorization.to_string();
        let verifier = f.verifier(false);
        let mut limits = flow_limits();
        if variant == 4 {
            limits.token_bytes = 32;
        }
        let flow = NativeOAuth::new(&verifier, limits).unwrap();
        let cancel = Cancellation::default();
        let original = context(&cancel);
        let ctx = BrokerRequestContext::new(&original);
        let pending = flow.begin_device(ctx).await.unwrap();
        let result = flow.receive_device(pending, ctx).await;
        if variant == 4 {
            assert!(result.is_err());
            assert_eq!(f.count("/token"), 1);
        } else {
            let tokens = result.unwrap();
            let identity = tokens.identity(ctx).unwrap();
            assert_eq!(identity.subject(), "device-subject");
            assert_eq!(identity.username(), None);
            assert!(identity.profile_claims().is_none());
            assert!(identity.access_teams().is_none());
        }
    }
}
#[tokio::test]
async fn device_discovery_provider_and_prompt_limits_deny_before_poll() {
    for variant in 0..7 {
        let key = pair();
        let f = Fixture::new(&key).await;
        if variant != 0 {
            enable_device(&f);
        }
        match variant {
            2 => {
                f.state.standard.lock().unwrap()["device_authorization_endpoint"] =
                    "https://foreign.example/device".into();
            }
            3 => {
                f.state.standard.lock().unwrap()["grant_types_supported"] =
                    json!(["authorization_code", "refresh_token"]);
            }
            4 => {
                let mut body: Value =
                    serde_json::from_str(&f.state.device_response.lock().unwrap()).unwrap();
                body["verification_uri"] = "https://foreign.example/device".into();
                *f.state.device_response.lock().unwrap() = body.to_string();
            }
            5 => {
                *f.state.device_response.lock().unwrap() =
                    "{\"device_code\":\"secret\",\"device_code\":\"second\"}".into();
            }
            _ => {}
        }
        let verifier = f.verifier(variant == 1);
        let mut limits = flow_limits();
        if variant == 6 {
            limits.form_body_bytes = 1;
        }
        let flow = NativeOAuth::new(&verifier, limits).unwrap();
        let cancel = Cancellation::default();
        let original = context(&cancel);
        let ctx = BrokerRequestContext::new(&original);
        assert!(flow.begin_device(ctx).await.is_err());
        assert_eq!(f.count("/token"), 0);
        if matches!(variant, 0 | 1 | 2 | 3 | 6) {
            assert_eq!(f.count("/device/authorize"), 0);
        }
    }
}
#[tokio::test]
async fn device_terminal_http_errors_do_not_poll_again() {
    for (status, code) in [
        (400, "access_denied"),
        (400, "expired_token"),
        (400, "invalid_grant"),
        (401, "invalid_client"),
        (503, "temporarily_unavailable"),
        (504, "temporarily_unavailable"),
        (500, "server_error"),
    ] {
        let key = pair();
        let f = Fixture::new(&key).await;
        enable_device(&f);
        *f.state.status.lock().unwrap() = status;
        *f.state.token.lock().unwrap() = json!({"error":code}).to_string();
        let verifier = f.verifier(false);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let cancel = Cancellation::default();
        let original = context(&cancel);
        let ctx = BrokerRequestContext::new(&original);
        let pending = flow.begin_device(ctx).await.unwrap();
        assert!(flow.receive_device(pending, ctx).await.is_err());
        assert_eq!(f.count("/token"), 1);
    }
}

#[tokio::test]
async fn device_default_interval_and_original_grant_deadline_are_real() {
    for deadline_case in [false, true] {
        let key = pair();
        let f = Fixture::new(&key).await;
        enable_device(&f);
        device_token(&f, &key, |_| {});
        let mut authorization: Value =
            serde_json::from_str(&f.state.device_response.lock().unwrap()).unwrap();
        authorization.as_object_mut().unwrap().remove("interval");
        if deadline_case {
            authorization["expires_in"] = 1.into();
        }
        *f.state.device_response.lock().unwrap() = authorization.to_string();
        f.state
            .device_steps
            .lock()
            .unwrap()
            .push_back((400, json!({"error":"authorization_pending"}).to_string()));
        let verifier = f.verifier(false);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let cancel = Cancellation::default();
        let original =
            SnapshotContext::new(Instant::now() + Duration::from_secs(20), cancel.clone());
        let ctx = BrokerRequestContext::new(&original);
        let pending = flow.begin_device(ctx).await.unwrap();
        if deadline_case {
            tokio::time::sleep(Duration::from_millis(1050)).await;
            let error = flow.receive_device(pending, ctx).await.err().unwrap();
            assert_eq!(error.kind(), NativeErrorKind::DeviceExpired);
            assert_eq!(f.count("/token"), 0);
        } else {
            flow.receive_device(pending, ctx).await.unwrap();
            let times = f.state.device_times.lock().unwrap();
            assert_eq!(times.len(), 2);
            assert!(times[1].duration_since(times[0]) >= Duration::from_millis(4950));
        }
    }
}
