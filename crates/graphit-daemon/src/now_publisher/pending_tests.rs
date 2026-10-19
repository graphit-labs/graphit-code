use super::*;
use axum::{Router, extract::ws::WebSocketUpgrade, routing::get};
use graphit_core::file_lock::Cancellation;
use graphit_http::{
    client::{ClientPolicy, Limits},
    websocket::WebSocketLimits,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{mpsc, oneshot};

struct Dropped(Arc<AtomicUsize>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn pending_observation_keeps_ticks_and_original_time_then_completes_or_drops() {
    let (first, second) = crate::project_now::tests::publisher_observations().await;
    let first_time = first.generated_at().to_owned();
    let second_time = second.generated_at().to_owned();
    assert_ne!(first_time, second_time);
    assert_eq!(first.project_id(), second.project_id());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (messages, mut received) = mpsc::channel(4);
    let (control, _) = tokio::sync::broadcast::channel::<axum::extract::ws::Message>(2);
    let outgoing = control.clone();
    let router = Router::new().route(
        "/",
        get(move |upgrade: WebSocketUpgrade| {
            let messages = messages.clone();
            let mut controls = outgoing.subscribe();
            async move {
                upgrade.on_upgrade(move |mut socket| async move {
                    loop {
                    tokio::select! {
                        message = socket.recv() => match message {
                            Some(Ok(axum::extract::ws::Message::Text(text))) => {
                                messages.send(serde_json::from_str::<serde_json::Value>(&text).unwrap()).await.unwrap();
                            }
                            Some(Ok(axum::extract::ws::Message::Close(_))) | None | Some(Err(_)) => break,
                            _ => (),
                        },
                        command = controls.recv() => {
                            socket.send(command.unwrap()).await.unwrap();
                        }
                    }
                }
            })
            }
        }),
    );
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let client = WebSocketClient::new(
        ClientPolicy {
            origin: &origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        Limits {
            request_body_bytes: 65536,
            response_body_bytes: 65536,
            url_bytes: 2048,
            header_bytes: 16384,
            header_count: 32,
            max_in_flight: 4,
            max_idle_per_host: 2,
            idle_timeout: Duration::from_secs(5),
            max_certificates: 2,
            max_certificate_bytes: 8192,
        },
        WebSocketLimits {
            frame_bytes: NonZeroUsize::new(65536).unwrap(),
            message_bytes: NonZeroUsize::new(65536).unwrap(),
            send_bytes: NonZeroUsize::new(65536).unwrap(),
        },
    )
    .unwrap();
    let cancellation = Cancellation::default();
    let mut socket = client
        .connect(
            &origin,
            HeaderMap::new(),
            NetworkRequestContext::with_deadline(
                Some(Instant::now() + Duration::from_secs(30)),
                &cancellation,
            ),
        )
        .await
        .unwrap();
    let published_at = Instant::now();
    socket
        .send(Message::Text(serde_json::to_string(&first).unwrap()))
        .await
        .unwrap();
    assert_eq!(received.recv().await.unwrap()["generated_at"], first_time);
    control
        .send(axum::extract::ws::Message::Text(
            r#"{"type":"welcome","heartbeat_timeout_ns":"4000000000"}"#.into(),
        ))
        .unwrap();
    let WebSocketRead::Message(Message::Text(welcome)) = socket.receive().await.unwrap() else {
        panic!("actual controlled Welcome expected");
    };
    let Control::Welcome {
        heartbeat_timeout_ns,
    } = decode(&welcome, NonZeroUsize::new(65536).unwrap()).unwrap()
    else {
        panic!("actual controlled Welcome expected");
    };
    let (cadence, first_tick) = negotiated_schedule(&heartbeat_timeout_ns, published_at).unwrap();
    assert_eq!(cadence, Duration::from_secs(2));
    assert_eq!(first_tick, published_at + cadence);
    let starts = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let (complete, completion) = oneshot::channel();
    let started = starts.clone();
    let guard = Dropped(drops.clone());
    let mut pending: Option<PendingObservation<'_>> = Some(Box::pin(async move {
        let _guard = guard;
        started.fetch_add(1, Ordering::SeqCst);
        completion.await.unwrap()
    }));
    let mut observed = first;
    assert!(matches!(
        receive_observation(&mut socket, Instant::now(), &mut observed, &mut pending)
            .await
            .unwrap(),
        WebSocketRead::Wakeup
    ));
    assert_eq!(starts.load(Ordering::SeqCst), 0);
    let mut primed = Box::pin(receive_observation(
        &mut socket,
        Instant::now() + Duration::from_secs(30),
        &mut observed,
        &mut pending,
    ));
    std::future::poll_fn(|context| {
        assert!(primed.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    control
        .send(axum::extract::ws::Message::Ping(Vec::new().into()))
        .unwrap();
    assert!(matches!(
        primed.await.unwrap(),
        WebSocketRead::Message(Message::Ping(_))
    ));
    // Control completion, not the producer or a protocol TTL. These genuine
    // observations were composed above; no blocked Lance read is simulated.
    for _ in 0..2 {
        assert!(matches!(
            receive_observation(&mut socket, Instant::now(), &mut observed, &mut pending)
                .await
                .unwrap(),
            WebSocketRead::Wakeup
        ));
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        assert!(pending.is_some());
        assert_eq!(observed.generated_at(), first_time);
        socket
            .send(Message::Text(serde_json::to_string(&observed).unwrap()))
            .await
            .unwrap();
        assert_eq!(received.recv().await.unwrap()["generated_at"], first_time);
    }
    let mut wait = Box::pin(receive_observation(
        &mut socket,
        Instant::now() + Duration::from_secs(30),
        &mut observed,
        &mut pending,
    ));
    std::future::poll_fn(|context| {
        assert!(wait.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    complete.send(Ok(second)).unwrap();
    std::future::poll_fn(|context| {
        assert!(wait.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    control
        .send(axum::extract::ws::Message::Ping(Vec::new().into()))
        .unwrap();
    assert!(matches!(
        wait.await.unwrap(),
        WebSocketRead::Message(Message::Ping(_))
    ));
    assert!(pending.is_none());
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(observed.generated_at(), second_time);
    // Sending on this same socket also detects cancellation of receive_until
    // when the controlled observation became Ready.
    socket
        .send(Message::Text(serde_json::to_string(&observed).unwrap()))
        .await
        .unwrap();
    assert_eq!(received.recv().await.unwrap()["generated_at"], second_time);
    let (fail, failure) = oneshot::channel();
    let guard = Dropped(drops.clone());
    pending = Some(Box::pin(async move {
        let _guard = guard;
        failure.await.unwrap()
    }));
    let mut wait = Box::pin(receive_observation(
        &mut socket,
        Instant::now() + Duration::from_secs(30),
        &mut observed,
        &mut pending,
    ));
    std::future::poll_fn(|context| {
        assert!(wait.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    fail.send(Err(NowPublisherError::InvalidScope)).unwrap();
    assert!(matches!(wait.await, Err(NowPublisherError::InvalidScope)));
    drop(pending.take());
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    assert!(matches!(
        socket.send(Message::Text(String::new())).await,
        Err(WebSocketError::Closed)
    ));
    drop(socket);

    let (_abandon, abandoned) = oneshot::channel::<Result<NowLocal, NowPublisherError>>();
    let guard = Dropped(drops.clone());
    pending = Some(Box::pin(async move {
        let _guard = guard;
        abandoned.await.unwrap()
    }));
    drop(pending.take());
    assert_eq!(drops.load(Ordering::SeqCst), 3);
    stop.send(()).unwrap();
    server.await.unwrap();
}
