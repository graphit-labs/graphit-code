use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use graphit_http::cors::{self, ConfigError, Limits, Options, Origins};
use http::{HeaderName, HeaderValue, Method, Request, Response, StatusCode, header::*};
use tower::{Layer, ServiceExt, service_fn};

fn options(origins: Origins) -> Options {
    Options {
        origins,
        methods: vec![Method::GET, Method::POST, Method::DELETE, Method::OPTIONS],
        allowed_headers: vec![
            AUTHORIZATION,
            CONTENT_TYPE,
            HeaderName::from_static("last-event-id"),
            HeaderName::from_static("mcp-session-id"),
            HeaderName::from_static("mcp-protocol-version"),
            HeaderName::from_static("x-graphit-request"),
        ],
        exposed_headers: vec![WWW_AUTHENTICATE, HeaderName::from_static("mcp-session-id")],
        limits: Limits {
            max_entries: 64,
            max_config_bytes: 4096,
            max_origin_bytes: 256,
        },
    }
}

fn exact(values: &[&str]) -> Origins {
    Origins::Exact(
        values
            .iter()
            .map(|v| HeaderValue::from_str(v).unwrap())
            .collect(),
    )
}

fn request(method: Method, origin: Option<&str>) -> Request<()> {
    let mut request = Request::builder()
        .method(method)
        .uri("/mcp")
        .body(())
        .unwrap();
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
    }
    request
}

async fn regular(origins: Origins, origin: Option<&str>) -> Response<Vec<u8>> {
    let next = service_fn(|_: Request<()>| async {
        Ok::<_, Infallible>(
            Response::builder()
                .status(401)
                .body(b"raw\0\xff".to_vec())
                .unwrap(),
        )
    });
    let layer = cors::layer(options(origins)).unwrap().unwrap();
    let response = layer
        .layer(next)
        .oneshot(request(Method::POST, origin))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.body(), b"raw\0\xff");
    assert!(
        response
            .headers()
            .get(ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .is_none()
    );
    assert!(
        response
            .headers()
            .get("access-control-allow-private-network")
            .is_none()
    );
    response
}

fn tokens(response: &Response<Vec<u8>>, name: HeaderName) -> Vec<String> {
    response
        .headers()
        .get_all(name)
        .iter()
        .flat_map(|v| v.to_str().unwrap().split(','))
        .map(|s| s.trim().to_ascii_lowercase())
        .collect()
}

#[tokio::test]
async fn loopback_http_supports_local_browsers_and_rejects_spoofed_origins() {
    for origin in [
        "http://localhost",
        "http://LOCALHOST:3000",
        "http://localhost:0",
        "http://127.0.0.1:8080",
        "http://127.0.0.2:9000",
        "http://127.255.255.254",
        "http://[::1]:8080",
        "http://[::ffff:127.0.0.2]:9000",
    ] {
        let response = regular(Origins::LoopbackHttp, Some(origin)).await;
        assert_eq!(
            response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN],
            origin,
            "{origin}"
        );
        assert!(tokens(&response, VARY).contains(&"origin".to_owned()));
    }
    for origin in [
        "https://localhost:3000",
        "http://localhost.attacker.example",
        "http://localhost.",
        "http://128.0.0.1",
        "http://[::2]",
        "http://[::ffff:192.0.2.1]",
        "https://external.example",
        "http://user@localhost",
        "http://localhost/",
        "http://localhost/path",
        "http://localhost?x=1",
        "http://localhost#fragment",
        "http://localhost:",
        "http://localhost:wat",
        "http://localhost:65536",
        "http://[::1]:wrong",
        "http://[invalid]",
        "http://2130706433",
        "http://0177.0.0.1",
        "null",
        "not a URL",
        "file://localhost",
        "",
    ] {
        let response = regular(Origins::LoopbackHttp, Some(origin)).await;
        assert!(
            response
                .headers()
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none(),
            "{origin}"
        );
    }
}

#[tokio::test]
async fn exact_list_replaces_loopback_and_preserves_byte_exact_matching() {
    for origin in ["https://external.example", "https://localhost:3000"] {
        let response = regular(
            exact(&["https://external.example", "https://localhost:3000"]),
            Some(origin),
        )
        .await;
        assert_eq!(response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN], origin);
    }
    for origin in [
        "http://localhost:3000",
        "https://external.example:443",
        "https://EXTERNAL.example",
        "https://external.example.attacker",
        "null",
    ] {
        let response = regular(exact(&["https://external.example"]), Some(origin)).await;
        assert!(
            response
                .headers()
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
    }
}

#[tokio::test]
async fn missing_origin_and_denied_actual_requests_still_reach_authentication() {
    for origins in [Origins::LoopbackHttp, exact(&["https://external.example"])] {
        let response = regular(origins, None).await;
        assert!(
            response
                .headers()
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
    }
    let response = regular(
        exact(&["https://external.example"]),
        Some("https://denied.example"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn preflight_is_handled_before_auth_and_advertises_only_explicit_fields() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&calls);
    let next = service_fn(move |_: Request<()>| {
        count.fetch_add(1, Ordering::Relaxed);
        async { Ok::<_, Infallible>(Response::new(Vec::<u8>::new())) }
    });
    let mut req = request(Method::OPTIONS, Some("https://external.example"));
    req.headers_mut().insert(
        ACCESS_CONTROL_REQUEST_METHOD,
        HeaderValue::from_static("POST"),
    );
    req.headers_mut().insert(
        ACCESS_CONTROL_REQUEST_HEADERS,
        HeaderValue::from_static(
            "Authorization,Content-Type,Mcp-Protocol-Version,Mcp-Session-Id,Last-Event-ID",
        ),
    );
    req.headers_mut().insert(
        "access-control-request-private-network",
        HeaderValue::from_static("true"),
    );
    let service = cors::layer(options(exact(&["https://external.example"])))
        .unwrap()
        .unwrap()
        .layer(next);
    let response = service.clone().oneshot(req.clone()).await.unwrap();
    assert!(response.status().is_success());
    assert!(response.body().is_empty());
    assert_eq!(
        response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN],
        "https://external.example"
    );
    let methods = tokens(&response, ACCESS_CONTROL_ALLOW_METHODS);
    assert_eq!(methods.len(), 4);
    assert!(methods.contains(&"delete".to_owned()));
    assert!(!methods.contains(&"patch".to_owned()));
    let headers = tokens(&response, ACCESS_CONTROL_ALLOW_HEADERS);
    for required in [
        "authorization",
        "content-type",
        "mcp-protocol-version",
        "mcp-session-id",
        "last-event-id",
    ] {
        assert!(headers.contains(&required.to_owned()));
    }
    assert!(!headers.contains(&"x-unconfigured".to_owned()));
    for header in [
        ACCESS_CONTROL_ALLOW_CREDENTIALS,
        ACCESS_CONTROL_MAX_AGE,
        HeaderName::from_static("access-control-allow-private-network"),
    ] {
        assert!(response.headers().get(header).is_none());
    }
    req.headers_mut().insert(
        ACCESS_CONTROL_REQUEST_METHOD,
        HeaderValue::from_static("PATCH"),
    );
    let response = service.clone().oneshot(req.clone()).await.unwrap();
    assert!(!tokens(&response, ACCESS_CONTROL_ALLOW_METHODS).contains(&"patch".to_owned()));
    req.headers_mut()
        .insert(ORIGIN, HeaderValue::from_static("https://denied.example"));
    let response = service.oneshot(req).await.unwrap();
    assert!(
        response
            .headers()
            .get(ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn disabled_mcp_does_not_intercept_options_and_discovery_is_independent() {
    assert!(cors::layer(options(Origins::Disabled)).unwrap().is_none());
    let mcp = service_fn(|req: Request<()>| async move {
        assert_eq!(req.method(), Method::OPTIONS);
        Ok::<_, Infallible>(
            Response::builder()
                .status(401)
                .body(b"auth".to_vec())
                .unwrap(),
        )
    });
    let response = mcp
        .oneshot(request(Method::OPTIONS, Some("https://browser.example")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        response
            .headers()
            .keys()
            .all(|n| !n.as_str().starts_with("access-control-"))
    );
    let discovery = service_fn(|_: Request<()>| async {
        Ok::<_, Infallible>(Response::new(b"public metadata".to_vec()))
    });
    let service = cors::layer(options(Origins::Any))
        .unwrap()
        .unwrap()
        .layer(discovery);
    let response = service
        .oneshot(request(Method::GET, Some("https://browser.example")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert_eq!(response.body(), b"public metadata");
    assert!(
        response
            .headers()
            .get(ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .is_none()
    );
}

#[tokio::test]
async fn any_uses_a_constant_wildcard_without_credentials_or_origin_reflection() {
    for origin in [Some("https://browser.example"), Some("null"), None] {
        let response = regular(Origins::Any, origin).await;
        assert_eq!(response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN], "*");
        assert!(!tokens(&response, VARY).contains(&"origin".to_owned()));
    }
}

#[tokio::test]
async fn ambiguous_and_oversized_origins_are_not_reflected() {
    for origins in [Origins::LoopbackHttp, exact(&["http://localhost:3000"])] {
        let mut req = request(Method::POST, Some("http://localhost:3000"));
        req.headers_mut()
            .append(ORIGIN, HeaderValue::from_static("http://localhost:3000"));
        let next = service_fn(|_: Request<()>| async {
            Ok::<_, Infallible>(Response::new(Vec::<u8>::new()))
        });
        let layer = cors::layer(options(origins)).unwrap().unwrap();
        let response = layer.layer(next).oneshot(req).await.unwrap();
        assert!(
            response
                .headers()
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
    }
    let mut opts = options(Origins::LoopbackHttp);
    opts.limits.max_origin_bytes = "http://localhost".len();
    let next =
        service_fn(|_: Request<()>| async { Ok::<_, Infallible>(Response::new(Vec::<u8>::new())) });
    let service = cors::layer(opts).unwrap().unwrap().layer(next);
    let response = service
        .clone()
        .oneshot(request(Method::GET, Some("http://localhost")))
        .await
        .unwrap();
    assert_eq!(
        response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN],
        "http://localhost"
    );
    let response = service
        .clone()
        .oneshot(request(Method::GET, Some("http://localhost:9")))
        .await
        .unwrap();
    assert!(
        response
            .headers()
            .get(ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
    let mut req = request(Method::GET, None);
    req.headers_mut()
        .insert(ORIGIN, HeaderValue::from_bytes(&[0xff]).unwrap());
    let response = service.oneshot(req).await.unwrap();
    assert!(
        response
            .headers()
            .get(ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[derive(Default)]
struct StreamingBody {
    identity: Option<Arc<()>>,
}

#[tokio::test]
async fn regular_responses_preserve_nonclone_body_challenge_and_security_headers() {
    let identity = Arc::new(());
    let captured = Arc::clone(&identity);
    let next = service_fn(move |_: Request<()>| {
        let identity = Arc::clone(&captured);
        async move {
            Ok::<_, Infallible>(
                Response::builder()
                    .status(401)
                    .header(
                        WWW_AUTHENTICATE,
                        "Bearer resource_metadata=\"https://code.example/metadata\"",
                    )
                    .header("mcp-session-id", "test-session")
                    .header(CONTENT_TYPE, "text/event-stream")
                    .header(CACHE_CONTROL, "no-cache")
                    .header(CONNECTION, "keep-alive")
                    .header("x-accel-buffering", "no")
                    .header("x-content-type-options", "nosniff")
                    .header("x-frame-options", "DENY")
                    .header(VARY, "Accept-Encoding")
                    .body(StreamingBody {
                        identity: Some(identity),
                    })
                    .unwrap(),
            )
        }
    });
    let response = cors::layer(options(exact(&["https://external.example"])))
        .unwrap()
        .unwrap()
        .layer(next)
        .oneshot(request(Method::POST, Some("https://external.example")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(Arc::ptr_eq(
        response.body().identity.as_ref().unwrap(),
        &identity
    ));
    assert_eq!(
        response.headers()[WWW_AUTHENTICATE],
        "Bearer resource_metadata=\"https://code.example/metadata\""
    );
    assert_eq!(response.headers()["mcp-session-id"], "test-session");
    for (name, value) in [
        ("content-type", "text/event-stream"),
        ("cache-control", "no-cache"),
        ("connection", "keep-alive"),
        ("x-accel-buffering", "no"),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
    ] {
        assert_eq!(response.headers()[name], value);
    }
    let exposed = response.headers()[ACCESS_CONTROL_EXPOSE_HEADERS]
        .to_str()
        .unwrap();
    assert!(exposed.contains("www-authenticate"));
    assert!(exposed.contains("mcp-session-id"));
    let vary = response
        .headers()
        .get_all(VARY)
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect::<Vec<_>>()
        .join(",");
    assert!(vary.to_ascii_lowercase().contains("accept-encoding"));
    assert!(vary.to_ascii_lowercase().contains("origin"));
}

#[tokio::test]
async fn inner_service_errors_are_propagated_without_rewriting() {
    let next = service_fn(|_: Request<()>| async { Err::<Response<Vec<u8>>, _>("inner sentinel") });
    let result = cors::layer(options(Origins::LoopbackHttp))
        .unwrap()
        .unwrap()
        .layer(next)
        .oneshot(request(Method::GET, Some("http://localhost")))
        .await;
    assert_eq!(result.unwrap_err(), "inner sentinel");
}

#[tokio::test]
async fn cloned_layers_keep_independent_captured_policies() {
    let a = cors::layer(options(exact(&["https://a.example"])))
        .unwrap()
        .unwrap();
    let b = cors::layer(options(exact(&["https://b.example"])))
        .unwrap()
        .unwrap();
    for (layer, allowed, denied) in [
        (a.clone(), "https://a.example", "https://b.example"),
        (b, "https://b.example", "https://a.example"),
        (a, "https://a.example", "https://b.example"),
    ] {
        let next = service_fn(|_: Request<()>| async {
            Ok::<_, Infallible>(Response::new(Vec::<u8>::new()))
        });
        let service = layer.layer(next);
        assert_eq!(
            service
                .clone()
                .oneshot(request(Method::GET, Some(allowed)))
                .await
                .unwrap()
                .headers()[ACCESS_CONTROL_ALLOW_ORIGIN],
            allowed
        );
        assert!(
            service
                .oneshot(request(Method::GET, Some(denied)))
                .await
                .unwrap()
                .headers()
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
    }
}

fn config_error(opts: Options) -> ConfigError {
    cors::layer(opts).unwrap_err()
}

#[test]
fn invalid_configuration_returns_typed_errors_before_a_layer_can_panic() {
    assert_eq!(config_error(options(exact(&[]))), ConfigError::EmptyOrigins);
    for value in [
        "*",
        "null",
        "https://user:private-marker@example.com",
        "https://example.com/",
        "https://example.com?private-marker",
        "https://example.com#private-marker",
        "https://example.com:65536",
        "https://example.com:wat",
        "ftp://example.com",
        "http://[invalid]",
        "",
    ] {
        let error = config_error(options(exact(&[value])));
        assert_eq!(error, ConfigError::InvalidOrigin, "{value}");
        assert!(!format!("{error:?}: {error}").contains("private-marker"));
    }
    let mut opts = options(Origins::LoopbackHttp);
    opts.methods.clear();
    assert_eq!(config_error(opts), ConfigError::EmptyMethods);
    let mut opts = options(Origins::LoopbackHttp);
    opts.methods.push(Method::from_bytes(b"*").unwrap());
    assert_eq!(config_error(opts), ConfigError::WildcardField);
    for exposed in [false, true] {
        let mut opts = options(Origins::LoopbackHttp);
        if exposed {
            opts.exposed_headers.push(HeaderName::from_static("*"));
        } else {
            opts.allowed_headers.push(HeaderName::from_static("*"));
        }
        assert_eq!(config_error(opts), ConfigError::WildcardField);
    }
}

#[test]
fn configuration_budgets_cover_exact_boundaries_and_debug_is_redacted() {
    let opts = options(exact(&["https://private-marker.example"]));
    let summary = format!("{opts:?}");
    assert!(!summary.contains("private-marker"));
    let Origins::Exact(origins) = &opts.origins else {
        unreachable!()
    };
    let entries = origins.len()
        + opts.methods.len()
        + opts.allowed_headers.len()
        + opts.exposed_headers.len();
    let bytes = origins
        .iter()
        .map(|v| v.as_bytes().len() + 1)
        .sum::<usize>()
        + opts
            .methods
            .iter()
            .map(|v| v.as_str().len() + 1)
            .sum::<usize>()
        + opts
            .allowed_headers
            .iter()
            .chain(&opts.exposed_headers)
            .map(|v| v.as_str().len() + 1)
            .sum::<usize>();
    for scenario in 0..6 {
        let mut opts = options(exact(&["https://private-marker.example"]));
        opts.limits = Limits {
            max_entries: entries,
            max_config_bytes: bytes,
            max_origin_bytes: "https://private-marker.example".len(),
        };
        match scenario {
            0 => assert!(cors::layer(opts).unwrap().is_some()),
            1 => {
                opts.limits.max_entries -= 1;
                assert_eq!(config_error(opts), ConfigError::TooManyEntries);
            }
            2 => {
                opts.limits.max_config_bytes -= 1;
                assert_eq!(config_error(opts), ConfigError::TooManyBytes);
            }
            3 => {
                opts.limits.max_origin_bytes -= 1;
                assert_eq!(config_error(opts), ConfigError::OriginTooLong);
            }
            4 => {
                opts.limits.max_config_bytes = 0;
                assert_eq!(config_error(opts), ConfigError::InvalidLimits);
            }
            5 => {
                opts.limits.max_entries = 0;
                assert_eq!(config_error(opts), ConfigError::InvalidLimits);
            }
            _ => unreachable!(),
        }
    }
    let mut opts = options(Origins::Disabled);
    opts.limits.max_origin_bytes = 0;
    assert_eq!(config_error(opts), ConfigError::InvalidLimits);
}

#[tokio::test]
async fn captured_exact_origins_release_large_shared_backing_buffers() {
    struct Owner {
        bytes: Vec<u8>,
        drops: Arc<AtomicUsize>,
    }
    impl AsRef<[u8]> for Owner {
        fn as_ref(&self) -> &[u8] {
            &self.bytes
        }
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let origin = "https://external.example";
    let mut buffer = vec![b'x'; 1024 * 1024];
    buffer[..origin.len()].copy_from_slice(origin.as_bytes());
    let shared = bytes::Bytes::from_owner(Owner {
        bytes: buffer,
        drops: Arc::clone(&drops),
    });
    let mut value = HeaderValue::from_maybe_shared(shared.slice(..origin.len())).unwrap();
    value.set_sensitive(true);
    drop(shared);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    let layer = cors::layer(options(Origins::Exact(vec![value])))
        .unwrap()
        .unwrap();
    assert_eq!(
        drops.load(Ordering::Relaxed),
        1,
        "layer must retain only the checked origin bytes"
    );
    assert!(!format!("{layer:?}").contains(origin));
    let next =
        service_fn(|_: Request<()>| async { Ok::<_, Infallible>(Response::new(Vec::<u8>::new())) });
    let response = layer
        .layer(next)
        .oneshot(request(Method::GET, Some(origin)))
        .await
        .unwrap();
    assert_eq!(response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN], origin);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}
