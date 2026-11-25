//! Repeatable middleware-only measurements; no network, browser, or authentication server.
use std::{convert::Infallible, error::Error, time::Instant};

use graphit_http::cors::{self, Limits, Options, Origins};
use http::{HeaderName, HeaderValue, Method, Request, Response, header::*};
use tower::{Layer, Service, ServiceExt, service_fn};

fn main() -> Result<(), Box<dyn Error>> {
    let mode = std::env::args()
        .nth(1)
        .ok_or("expected loopback, exact or preflight")?;
    let (origins, origin, preflight) = match mode.as_str() {
        "loopback" => (Origins::LoopbackHttp, "http://localhost:3000", false),
        "exact" | "preflight" => (
            Origins::Exact(
                (0..32)
                    .map(|n| HeaderValue::from_str(&format!("https://host{n}.example")))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            "https://host31.example",
            mode == "preflight",
        ),
        _ => return Err("expected loopback, exact or preflight".into()),
    };
    let layer = cors::layer(Options {
        origins,
        methods: vec![Method::GET, Method::POST, Method::DELETE, Method::OPTIONS],
        allowed_headers: vec![
            AUTHORIZATION,
            CONTENT_TYPE,
            HeaderName::from_static("mcp-session-id"),
        ],
        exposed_headers: vec![WWW_AUTHENTICATE, HeaderName::from_static("mcp-session-id")],
        limits: Limits {
            max_entries: 64,
            max_config_bytes: 4096,
            max_origin_bytes: 256,
        },
    })?
    .ok_or("enabled layer missing")?;
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let next =
        service_fn(|_: Request<()>| async { Ok::<_, Infallible>(Response::new(Vec::<u8>::new())) });
    let mut service = layer.layer(next);
    let mut request = Request::builder()
        .method(if preflight {
            Method::OPTIONS
        } else {
            Method::POST
        })
        .uri("/mcp")
        .header(ORIGIN, origin)
        .body(())?;
    if preflight {
        request.headers_mut().insert(
            ACCESS_CONTROL_REQUEST_METHOD,
            HeaderValue::from_static("POST"),
        );
        request.headers_mut().insert(
            ACCESS_CONTROL_REQUEST_HEADERS,
            HeaderValue::from_static("authorization,content-type,mcp-session-id"),
        );
    }
    let operations = 10_000;
    let warmups = 100;
    let mut samples = Vec::with_capacity(operations);
    let elapsed = runtime.block_on(async {
        for _ in 0..warmups {
            let response = service.ready().await?.call(request.clone()).await?;
            assert_eq!(response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN], origin);
        }
        let begin = Instant::now();
        for _ in 0..operations {
            let operation = Instant::now();
            let response = service
                .ready()
                .await?
                .call(std::hint::black_box(request.clone()))
                .await?;
            assert_eq!(response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN], origin);
            assert!(
                response
                    .headers()
                    .get(ACCESS_CONTROL_ALLOW_CREDENTIALS)
                    .is_none()
            );
            drop(std::hint::black_box(response));
            samples.push(operation.elapsed().as_nanos());
        }
        Ok::<_, Infallible>(begin.elapsed())
    })?;
    samples.sort_unstable();
    println!(
        "mode={mode} operations={operations} warmups={warmups} elapsed_ms={:.3} ops_per_second={:.2} p50_us={:.3} p95_us={:.3}",
        elapsed.as_secs_f64() * 1000.0,
        operations as f64 / elapsed.as_secs_f64(),
        samples[operations / 2] as f64 / 1000.0,
        samples[operations * 95 / 100] as f64 / 1000.0
    );
    Ok(())
}
