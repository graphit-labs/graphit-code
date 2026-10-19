//! Actual HTTP response lifecycle with a blocking queue occupied by the Hub kernel lock.
use super::*;
#[path = "native_install_auth.rs"]
pub(crate) mod auth;
use graphit_hub::policy::RequestError;
use serde_json::json;

fn context(cancel: &Cancellation) -> RequestContext<'_> {
    let config = ConfigSnapshot::new(&graphit_core::brand::Brand::default(), []).unwrap();
    RequestContext::new(
        HubResourcePolicy::resolve(&config).unwrap().content(),
        cancel,
        Instant::now() + Duration::from_secs(120),
    )
    .unwrap()
}
fn run(work: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .max_blocking_threads(1)
                .enable_all()
                .build()
                .unwrap()
                .block_on(work)
        })
        .unwrap()
        .join()
        .unwrap();
}
async fn journey(mode: u8) {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let broker = auth::Broker::start().await;
    let f = auth::Fixture::new(&ctx, true);
    f.profile(&broker, auth::now() + 600, "broker-api", false, &ctx);
    let mut policy = NativeInstallPolicy {
        hub: f.policy.clone(),
        graph: LocalGraphPolicy::resolve(&f.config, ctx.snapshot()).unwrap(),
        timeout: Duration::from_secs(10),
    };
    policy.timeout = if mode == 0 {
        Duration::from_millis(50)
    } else {
        Duration::from_secs(10)
    };
    let owner = NativeInstallHttp::new(
        Arc::clone(&f.auth),
        f.paths.clone(),
        f.registry.clone(),
        policy,
        CancellationToken::new(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}{}", listener.local_addr().unwrap(), ROUTE);
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let router = owner.router();
    let serving = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    // Real retained kernel-lock wait occupies the only SDK blocking worker.
    let held = f
        .registry
        .writer(ctx.remaining().unwrap().unwrap(), ctx.cancellation())
        .unwrap();
    let registry = f.registry.clone();
    let (started, ready) = std::sync::mpsc::channel();
    let occupier = tokio::task::spawn_blocking(move || {
        let cancel = Cancellation::default();
        started.send(()).unwrap();
        registry.writer(Duration::from_secs(30), &cancel).map(drop)
    });
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    let key = f.key(&ctx);
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let response = client
        .post(url)
        .bearer_auth(&*key)
        .header("x-graphit-auth-origin", "runtime")
        .json(&json!({"input":f.temp.path().join("not-opened.zip"),
            "producer":"01ARZ3NDEKTSV4RRFFQ69G5FAV","id":"queued/native","version":"v1"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(owner.0.jobs.lock().await.len(), 1);
    assert!(f.auth.runtime().check(ctx.snapshot()).is_ok());
    let result = match mode {
        0 => {
            tokio::time::sleep(Duration::from_millis(80)).await;
            drop(held);
            occupier.await.unwrap().unwrap();
            assert!(response.bytes().await.is_err());
            owner
                .0
                .jobs
                .lock()
                .await
                .join_next()
                .await
                .unwrap()
                .unwrap()
        }
        1 => {
            drop(response);
            // Observe actual TCP response closure before releasing the SDK queue.
            tokio::time::sleep(Duration::from_millis(80)).await;
            drop(held);
            occupier.await.unwrap().unwrap();
            owner
                .0
                .jobs
                .lock()
                .await
                .join_next()
                .await
                .unwrap()
                .unwrap()
        }
        2 => {
            let first = owner.clone();
            let waiter = tokio::spawn(async move { first.shutdown().await });
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert!(!waiter.is_finished());
            waiter.abort();
            assert!(waiter.await.unwrap_err().is_cancelled());
            assert_eq!(owner.0.jobs.lock().await.len(), 1);
            assert!(f.auth.runtime().check(ctx.snapshot()).is_ok());
            let next = owner.clone();
            let waiter = tokio::spawn(async move { next.shutdown().await });
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert!(!waiter.is_finished());
            assert!(f.auth.runtime().check(ctx.snapshot()).is_ok());
            drop(held);
            occupier.await.unwrap().unwrap();
            assert!(response.bytes().await.is_err());
            let error = waiter.await.unwrap().unwrap_err();
            assert_eq!(owner.0.jobs.lock().await.len(), 0);
            Err(error)
        }
        _ => unreachable!(),
    };
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("queued Original must refuse construction"),
    };
    let causal = error
        .get_ref()
        .unwrap()
        .downcast_ref::<RequestError>()
        .unwrap();
    match causal {
        RequestError::Content(error) => {
            if mode == 0 {
                assert!(matches!(error, graphit_hub::native::ContentError::TimedOut));
            } else {
                assert!(matches!(
                    error,
                    graphit_hub::native::ContentError::Cancelled
                ));
            }
        }
        RequestError::DeadlineOverflow => panic!("captured deadline must stay finite"),
    }
    assert_eq!(broker.requests(), [0, 0, 0]);
    assert_eq!(f.registry.load().unwrap().entries().count(), 0);
    assert!(f.auth.runtime().check(ctx.snapshot()).is_ok());
    owner.shutdown().await.unwrap();
    let _ = stop.send(());
    serving.await.unwrap();
    f.remove(&ctx);
    broker.shutdown().await;
}
#[test]
fn actual_queued_deadline_has_no_fabricated_terminal() {
    run(journey(0));
}
#[test]
fn actual_response_closed_cancels_retained_sdk_job() {
    run(journey(1));
}
#[test]
fn actual_aborted_shutdown_waiter_retains_owner_and_join() {
    run(journey(2));
}

#[test]
fn actual_pre_admission_mutex_wait_observes_deadline_and_stop() {
    run(async {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = auth::Fixture::new(&ctx, true);
        for stopped in [false, true] {
            let mut policy = NativeInstallPolicy::capture(&f.config, ctx.snapshot()).unwrap();
            policy.timeout = Duration::from_millis(50);
            let owner = NativeInstallHttp::new(
                Arc::clone(&f.auth),
                f.paths.clone(),
                f.registry.clone(),
                policy,
                CancellationToken::new(),
            )
            .unwrap();
            let held = owner.0.jobs.lock().await;
            let mut headers = HeaderMap::new();
            headers.insert("x-graphit-auth-origin", "runtime".parse().unwrap());
            headers.insert(
                header::AUTHORIZATION,
                format!("Bearer {}", f.key(&ctx).as_str()).parse().unwrap(),
            );
            let body = Bytes::from(serde_json::to_vec(&json!({"input":f.temp.path().join("not-opened.zip"),
                "producer":"01ARZ3NDEKTSV4RRFFQ69G5FAV","id":"pre-admission/native","version":"v1"})).unwrap());
            let request_owner = owner.clone();
            let request =
                tokio::spawn(async move { install(State(request_owner), headers, body).await });
            if stopped {
                tokio::time::sleep(Duration::from_millis(10)).await;
                owner.0.stop.cancel();
            }
            let response = tokio::time::timeout(Duration::from_secs(2), request)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                response.status(),
                if stopped {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::REQUEST_TIMEOUT
                }
            );
            assert_eq!(held.len(), 0);
            drop(held);
            owner.shutdown().await.unwrap();
        }
        assert_eq!(f.registry.load().unwrap().entries().count(), 0);
        f.remove(&ctx);
    });
}
