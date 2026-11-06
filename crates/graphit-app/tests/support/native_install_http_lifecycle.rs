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
#[test]
fn owner_constructor_accepts_canonical_spelling_and_denies_another_installation() {
    run(async {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = auth::Fixture::new(&ctx, true);
        let policy = NativeInstallPolicy {
            hub: f.policy.clone(),
            graph: LocalGraphPolicy::resolve(&f.config, ctx.snapshot()).unwrap(),
            timeout: Duration::from_secs(10),
        };
        let canonical = InstallationPaths::resolve(
            f.paths.brand().clone(),
            f.temp.path(),
            None,
            Some(f.auth.installation().live.owner_root()),
        )
        .unwrap();
        for paths in [f.paths.clone(), canonical] {
            NativeInstallHttp::new(
                Arc::clone(&f.auth),
                paths,
                f.registry.clone(),
                policy.clone(),
                CancellationToken::new(),
            )
            .unwrap();
        }
        let other = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            f.paths.brand().clone(),
            f.temp.path(),
            None,
            Some(other.path()),
        )
        .unwrap();
        let error = NativeInstallHttp::new(
            Arc::clone(&f.auth),
            paths,
            f.registry.clone(),
            policy,
            CancellationToken::new(),
        )
        .err()
        .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    });
}

#[cfg(unix)]
#[test]
fn owner_constructor_rejects_same_spelling_after_physical_root_replacement() {
    run(async {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = auth::Fixture::new(&ctx, true);
        let policy = NativeInstallPolicy {
            hub: f.policy.clone(),
            graph: LocalGraphPolicy::resolve(&f.config, ctx.snapshot()).unwrap(),
            timeout: Duration::from_secs(10),
        };
        let held = f.temp.path().join("held-installation");
        std::fs::rename(f.paths.root(), held).unwrap();
        std::fs::create_dir(f.paths.root()).unwrap();
        let error = NativeInstallHttp::new(
            Arc::clone(&f.auth),
            f.paths.clone(),
            f.registry.clone(),
            policy,
            CancellationToken::new(),
        )
        .err()
        .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    });
}

async fn completed(owner: &NativeInstallHttp) -> io::Result<Completion> {
    tokio::time::timeout(Duration::from_secs(5), async {
        owner
            .0
            .jobs
            .lock()
            .await
            .join_next()
            .await
            .unwrap()
            .unwrap()
    })
    .await
    .expect("retained SDK job must finish after queue release")
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
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
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
            // The Original was captured before response admission. Waiting one
            // policy interval from this observed response bounds its real deadline.
            tokio::time::sleep_until(tokio::time::Instant::now() + owner.0.policy.timeout).await;
            drop(held);
            tokio::time::timeout(Duration::from_secs(5), occupier)
                .await
                .expect("released SDK queue must finish")
                .unwrap()
                .unwrap();
            assert!(response.bytes().await.is_err());
            completed(&owner).await
        }
        1 => {
            drop(response);
            // Observe the owner's actual close/cancellation branch, not elapsed time.
            tokio::time::timeout(
                Duration::from_secs(2),
                owner.0.response_closed_observed.cancelled(),
            )
            .await
            .expect("owner must observe the closed HTTP response before SDK admission");
            drop(held);
            tokio::time::timeout(Duration::from_secs(5), occupier)
                .await
                .expect("released SDK queue must finish")
                .unwrap()
                .unwrap();
            completed(&owner).await
        }
        2 => {
            let first = owner.clone();
            let waiter = tokio::spawn(async move { first.shutdown().await });
            tokio::time::timeout(Duration::from_secs(2), owner.0.stop.cancelled())
                .await
                .expect("first shutdown waiter must be polled");
            assert!(!waiter.is_finished());
            waiter.abort();
            assert!(waiter.await.unwrap_err().is_cancelled());
            assert_eq!(owner.0.jobs.lock().await.len(), 1);
            assert!(f.auth.runtime().check(ctx.snapshot()).is_ok());
            let mut waiter = Box::pin(owner.shutdown());
            assert!(futures_util::poll!(&mut waiter).is_pending());
            assert!(f.auth.runtime().check(ctx.snapshot()).is_ok());
            drop(held);
            tokio::time::timeout(Duration::from_secs(5), occupier)
                .await
                .expect("released SDK queue must finish")
                .unwrap()
                .unwrap();
            assert!(response.bytes().await.is_err());
            let error = tokio::time::timeout(Duration::from_secs(5), waiter)
                .await
                .expect("retained owner must join its SDK job")
                .unwrap_err();
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
    tokio::time::timeout(Duration::from_secs(5), owner.shutdown())
        .await
        .expect("owner shutdown must finish")
        .unwrap();
    let _ = stop.send(());
    tokio::time::timeout(Duration::from_secs(5), serving)
        .await
        .expect("HTTP listener must stop after owned jobs finish")
        .unwrap();
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
            let mut request = Box::pin(install(State(owner.clone()), headers, body));
            assert!(futures_util::poll!(&mut request).is_pending());
            if stopped {
                owner.0.stop.cancel();
            }
            let response = tokio::time::timeout(Duration::from_secs(2), request)
                .await
                .expect("admission mutex wait must observe its captured deadline or stop");
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
            tokio::time::timeout(Duration::from_secs(5), owner.shutdown())
                .await
                .expect("owner shutdown must finish")
                .unwrap();
        }
        assert_eq!(f.registry.load().unwrap().entries().count(), 0);
        f.remove(&ctx);
    });
}
