use super::*;
use futures_util::FutureExt;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::{oneshot, watch},
};

#[test]
fn startup_selection_does_not_capture_an_unselected_bundle() {
    let fixture = Fixture::new();
    fixture.install_model();
    let cancellation = Cancellation::default();
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let global = ConfigStore::global(&fixture.installation).unwrap();
    let snapshot = ConfigSnapshot::new(fixture.config.brand(), [global.load().unwrap()]).unwrap();
    let mut settings = model_api::ModelSettings::read(&snapshot).unwrap().unwrap();
    settings.slots[0].name = "unselected".into();
    settings.slots[0].bundle = fixture
        .installation
        .root()
        .join("missing-unselected-bundle");
    global
        .set(
            ConfigKey::new("daemon.models").unwrap(),
            serde_json::to_value(settings).unwrap(),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    let modules = graphit_core::module_settings::ModuleSettings::resolve(&fixture.config).unwrap();
    assert!(
        crate::foreground_memory_jobs::expected_model(
            &fixture.installation,
            None,
            original.snapshot(),
            &[modules]
        )
        .unwrap()
        .is_none()
    );
}

/// Holds actual response bytes, after the native child has admitted the request.
pub(super) struct ModelResponseBoundary {
    pub(super) socket: TcpStream,
    pub(super) arrived: Option<oneshot::Sender<()>>,
    pub(super) release: Option<oneshot::Receiver<()>>,
    pub(super) buffered: Vec<u8>,
}
impl AsyncRead for ModelResponseBoundary {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.arrived.is_some() {
            let mut bytes = [0; 4096];
            let mut staged = ReadBuf::new(&mut bytes);
            std::task::ready!(Pin::new(&mut this.socket).poll_read(cx, &mut staged))?;
            if staged.filled().is_empty() {
                return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
            }
            this.buffered.extend_from_slice(staged.filled());
            this.arrived.take().unwrap().send(()).unwrap();
        }
        if !this.buffered.is_empty() {
            if let Some(release) = this.release.as_mut() {
                std::task::ready!(Pin::new(release).poll(cx))
                    .map_err(|_| io::ErrorKind::BrokenPipe)?;
                this.release = None;
            }
            let count = output.remaining().min(this.buffered.len());
            output.put_slice(&this.buffered[..count]);
            this.buffered.drain(..count);
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut this.socket).poll_read(cx, output)
    }
}
impl AsyncWrite for ModelResponseBoundary {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().socket).poll_write(cx, bytes)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().socket).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().socket).poll_shutdown(cx)
    }
}

async fn refresh_error(
    observer: &mut watch::Receiver<Option<Result<(), io::ErrorKind>>>,
) -> io::ErrorKind {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            observer.changed().await.unwrap();
            if let Some(Err(error)) = *observer.borrow_and_update() {
                return error;
            }
        }
    })
    .await
    .expect("Source rejected the invalid global configuration")
}

async fn changed_model(
    server: &SourceServer,
    observer: &mut watch::Receiver<Option<Result<(), io::ErrorKind>>>,
    config_id: Option<model_api::ConfigId>,
) -> Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let current = server.semantic_model().unwrap();
            if current
                .as_ref()
                .map(|model| model.local.as_ref().unwrap().expected.config_id())
                == config_id
            {
                return current;
            }
            observer.changed().await.unwrap();
            observer.borrow_and_update();
        }
    })
    .await
    .expect("Source published the acknowledged global model selection")
}

#[tokio::test]
async fn global_model_refresh_keeps_inflight_delivery_and_two_projects() {
    run_global_model_refresh(&["modules.ast", "modules.knowledge"]).await;
}

#[tokio::test]
async fn global_model_refresh_with_all_module_jobs() {
    run_global_model_refresh(&[]).await;
}

#[tokio::test]
async fn supervisor_knowledge_publishes_while_ast_models_response_is_pending() {
    use futures_util::TryStreamExt;
    use graphit_ast::native_nn_index_locator::{NativeNnIndexPresence, open_local_native_nn_index};
    use graphit_data::knowledge_index::{KnowledgeDateRead, read_native_knowledge_dates};

    let fixture = Fixture::new();
    fixture.install_model();
    let cancellation = Cancellation::default();
    ConfigStore::project(&fixture.projects[0])
        .unwrap()
        .set(
            Module::Memory.key(),
            json!(false),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    fs::write(fixture.projects[0].root().join(".astignore"), "docs/\n").unwrap();
    fs::write(
        fixture.projects[0].root().join("initial.rs"),
        "pub fn ordering_initial_alpha() -> i32 { 1 }\n",
    )
    .unwrap();
    fs::create_dir(fixture.projects[0].root().join("docs")).unwrap();
    fs::write(
        fixture.projects[0].root().join("docs/independent.md"),
        "---\nid: independent\n---\n# a\na b\n",
    )
    .unwrap();
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let mut foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs()[..1],
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &original,
    )
    .await
    .unwrap();
    let resources = foreground
        .server
        .as_ref()
        .unwrap()
        .semantic_model()
        .unwrap()
        .unwrap();
    let auth = Arc::clone(&foreground.auth);
    let child_address = resources.local.as_ref().unwrap().address;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let boundary_address = listener.local_addr().unwrap();
    let (observed, mut arrival) = tokio::sync::mpsc::channel(1);
    let release_all = tokio_util::sync::CancellationToken::new();
    let release_signal = release_all.clone();
    let proxy_cancel = tokio_util::sync::CancellationToken::new();
    let proxy_signal = proxy_cancel.clone();
    let proxy = tokio::spawn(async move {
        let mut copies = tokio::task::JoinSet::new();
        let mut failure = None;
        loop {
            let accepted = tokio::select! {
                _ = proxy_signal.cancelled() => break,
                accepted = listener.accept() => accepted,
            };
            let (mut incoming, _) = match accepted {
                Ok(accepted) => accepted,
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            };
            let observed = observed.clone();
            let release_signal = release_signal.clone();
            copies.spawn(async move {
                let (arrived, arrival) = oneshot::channel();
                let (release, released) = oneshot::channel();
                let forward = async {
                    let mut actual = ModelResponseBoundary {
                        socket: TcpStream::connect(child_address).await.unwrap(),
                        arrived: Some(arrived),
                        release: Some(released),
                        buffered: Vec::new(),
                    };
                    // Every connection forwards the real authenticated RPC. Hold
                    // delivery of all real replies until the shared release.
                    tokio::io::copy_bidirectional(&mut incoming, &mut actual).await
                };
                let observe = async {
                    if arrival.await.is_ok() {
                        let _ = observed.try_send(());
                        release_signal.cancelled().await;
                    }
                    let _ = release.send(());
                };
                let (forwarded, ()) = tokio::join!(forward, observe);
                forwarded
            });
        }
        while let Some(copy) = copies.join_next().await {
            match copy {
                Ok(Ok(_)) => (),
                Ok(Err(error))
                    if matches!(
                        error.kind(),
                        io::ErrorKind::BrokenPipe
                            | io::ErrorKind::ConnectionReset
                            | io::ErrorKind::UnexpectedEof
                    ) => {}
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                }
                Err(error) => {
                    failure.get_or_insert_with(|| io::Error::other(error));
                }
            }
        }
        failure.map_or(Ok(()), Err)
    });
    let exercise = std::panic::AssertUnwindSafe(async {
        let selected = foreground.projects.as_ref().unwrap()[0].clone();
        let scope = CapturedPublicationScope::project(
            &selected.0,
            selected.1,
            foreground.scope,
            original.snapshot(),
        )
        .unwrap();
        let record = scope.binding().unwrap();
        let runtime = auth.runtime().check(original.snapshot()).unwrap();
        let raw = std::str::from_utf8(runtime.secret(original.snapshot()).unwrap()).unwrap();
        let issuer = auth.issuer();
        let caller = issuer.authenticate_runtime(raw, &original).unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        let read = authority
            .admit_authenticated_read(
                &caller,
                &original,
                ScopeInput {
                    installation: auth.installation(),
                    project: Some(ProjectCapture {
                        scope: ScopeCapture {
                            live: &scope,
                            record: &record,
                        },
                        version: &selected.2,
                    }),
                    providers: &[],
                    binding: NativeBinding::KiroV3,
                    global_version: "",
                },
                ProjectReadModule::Knowledge,
            )
            .unwrap();
        let policy = crate::foreground_tasks::task_policy(
            &foreground.policy,
            foreground.scope,
            foreground.native,
            NativeBinding::KiroV3,
        )
        .unwrap();
        let limits = crate::foreground_knowledge::limits(&policy).unwrap();
        let (baseline_documents, baseline_nn) =
            tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let documents = read_native_knowledge_dates(original.snapshot(), &read, limits)
                        .await
                        .unwrap();
                    if let KnowledgeDateRead::Published { generation, dates } = documents
                        && dates.len() == 1
                        && let NativeNnIndexPresence::Available(index) =
                            Box::pin(open_local_native_nn_index(
                                auth.installation(),
                                ProjectCapture {
                                    scope: ScopeCapture {
                                        live: &scope,
                                        record: &record,
                                    },
                                    version: &selected.2,
                                },
                                original.snapshot(),
                                foreground.native,
                            ))
                            .await
                            .unwrap()
                    {
                        let scan = index.snapshot(original.snapshot()).unwrap().scan();
                        let mut stream = scan.try_into_stream().await.unwrap();
                        let mut has_rows = false;
                        while let Some(batch) = stream.try_next().await.unwrap() {
                            has_rows |= batch.num_rows() > 0;
                        }
                        if has_rows {
                            break (generation.version, index.version());
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("actual supervisor baseline publishes documents and nonempty AST NN");

        // Keep the existing supervisor. attach_jobs refreshes the genuine
        // child selection and would replace a test transport address. Publish
        // this delivery boundary before changing inputs and requesting a rescan.
        let mut redirected = crate::foreground_memory_jobs::MemoryJobResources::remote(
            Arc::clone(&resources.auth),
            resources.http.clone(),
            Arc::clone(&resources.brokers),
            resources.policy.clone(),
            resources.scope,
            resources.native,
            resources.binding,
        )
        .unwrap();
        redirected.local = Some(crate::foreground_memory_jobs::OwnedLocalEmbedding {
            client: Arc::clone(&resources.local.as_ref().unwrap().client),
            expected: Arc::clone(&resources.local.as_ref().unwrap().expected),
            address: boundary_address,
        });
        foreground
            .publish_semantic_model(Some(Arc::new(redirected)))
            .unwrap();
        assert_eq!(
            foreground
                .server
                .as_ref()
                .unwrap()
                .semantic_model()
                .unwrap()
                .unwrap()
                .local
                .as_ref()
                .unwrap()
                .address,
            boundary_address
        );
        fs::write(
            selected.0.root().join("pending_fresh.rs"),
            "pub fn ordering_fresh_beta() -> i32 { 2 + 3 }\n",
        )
        .unwrap();
        fs::write(
            selected.0.root().join("docs/independent.md"),
            "---\nid: independent\n---\n# b\nb a a\n",
        )
        .unwrap();
        let key = graphit_daemon::supervisor::ProjectContext {
            id: selected.1,
            version: selected.2.clone(),
        }
        .key();
        let rescan = foreground.supervisors.rescan_handle(&key).unwrap();
        assert!(rescan.request_rescan());
        tokio::time::timeout(Duration::from_secs(30), arrival.recv())
            .await
            .expect("actual Models response bytes reached the retained boundary")
            .expect("real response observer remains owned");
        let documents = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                match read_native_knowledge_dates(original.snapshot(), &read, limits)
                    .await
                    .unwrap()
                {
                    KnowledgeDateRead::Published { generation, dates }
                        if generation.version > baseline_documents =>
                    {
                        break dates;
                    }
                    _ => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .expect("Knowledge publishes a NEW document generation while AST reply delivery is held");
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].slug, "independent");
        let NativeNnIndexPresence::Available(held) = Box::pin(open_local_native_nn_index(
            auth.installation(),
            ProjectCapture {
                scope: ScopeCapture {
                    live: &scope,
                    record: &record,
                },
                version: &selected.2,
            },
            original.snapshot(),
            foreground.native,
        ))
        .await
        .unwrap() else {
            panic!("held AST retains its actual baseline corpus")
        };
        assert_eq!(
            held.version(),
            baseline_nn,
            "fresh AST NN cannot publish without its Models response"
        );
        drop(held);
        assert!(!proxy.is_finished());

        release_all.cancel();
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let NativeNnIndexPresence::Available(index) = Box::pin(open_local_native_nn_index(
                    auth.installation(),
                    ProjectCapture {
                        scope: ScopeCapture {
                            live: &scope,
                            record: &record,
                        },
                        version: &selected.2,
                    },
                    original.snapshot(),
                    foreground.native,
                ))
                .await
                .unwrap() else {
                    panic!("fresh AST cannot erase the existing corpus")
                };
                if index.version() > baseline_nn {
                    let scan = index.snapshot(original.snapshot()).unwrap().scan();
                    let mut stream = scan.try_into_stream().await.unwrap();
                    while let Some(batch) = stream.try_next().await.unwrap() {
                        let paths = batch
                            .column_by_name("path")
                            .unwrap()
                            .as_any()
                            .downcast_ref::<arrow_array::StringArray>()
                            .unwrap();
                        if paths.iter().any(|path| path == Some("pending_fresh.rs")) {
                            return;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("released real AST RPC publishes the fresh file in a later NN generation");
    })
    .catch_unwind()
    .await;
    // Always release actual replies, then cancel/join both retained Source jobs
    // and the native child before checking any assertion or proxy task outcome.
    release_all.cancel();
    let stopped = foreground.shutdown().await;
    proxy_cancel.cancel();
    let joined = proxy.await;
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
    stopped.unwrap();
    joined.unwrap().unwrap();
    assert!(TcpStream::connect(child_address).await.is_err());
    assert!(TcpStream::connect(boundary_address).await.is_err());
}

async fn run_global_model_refresh(disabled_modules: &[&str]) {
    let fixture = Fixture::new();
    fixture.install_model();
    let cancellation = Cancellation::default();
    for project in &fixture.projects {
        for key in disabled_modules {
            ConfigStore::project(project)
                .unwrap()
                .set(
                    ConfigKey::new(key).unwrap(),
                    json!(false),
                    Duration::from_secs(5),
                    &cancellation,
                )
                .unwrap();
        }
    }
    let memories = fixture.seed_memories().await;
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &original,
    )
    .await
    .unwrap();
    let initial = std::panic::AssertUnwindSafe(async {
        assert_indexed(&foreground, 0, &memories[0]).await;
        assert_indexed(&foreground, 1, &memories[1]).await;
    })
    .catch_unwind()
    .await;
    if let Err(panic) = initial {
        foreground.shutdown().await.unwrap();
        std::panic::resume_unwind(panic);
    }
    let server = foreground.server.as_ref().unwrap().clone();
    let projects = Arc::clone(foreground.projects.as_ref().unwrap());
    let old = server.semantic_model().unwrap().unwrap();
    let old_id = old.local.as_ref().unwrap().expected.config_id();
    let vector_id = old.local.as_ref().unwrap().expected.vector_id();
    let old_metadata = Arc::downgrade(&old.local.as_ref().unwrap().expected);
    let address = old.local.as_ref().unwrap().address;
    let source_address = foreground.address().unwrap();
    let native = foreground.native;
    let mut refreshes = foreground.model_refreshes.subscribe();
    let auth = Arc::clone(&foreground.auth);
    let service = Cancellation::default();
    let signal = service.clone();
    let running = tokio::spawn(async move { foreground.run(&signal).await });
    let exercise = std::panic::AssertUnwindSafe(async {
        let issuer = auth.issuer();
        let runtime = auth.runtime().check(original.snapshot()).unwrap();
        let raw = std::str::from_utf8(runtime.secret(original.snapshot()).unwrap()).unwrap();
        let caller = issuer.authenticate_runtime(raw, &original).unwrap();
        let mut binding = issuer.capture_service(&caller, graphit_auth::ServicePurpose::Embedding).unwrap();
        let lease = binding.local().unwrap();
        let (arrived, arrival) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let connection = async {
            Ok::<_, io::Error>(ModelResponseBoundary {
                socket: TcpStream::connect(address).await?, arrived: Some(arrived),
                release: Some(released), buffered: Vec::new(),
            })
        };
        let mut pending = Box::pin(old.local.as_ref().unwrap().client.embed(
            &lease, auth.runtime(), &old.local.as_ref().unwrap().expected,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["a b".into()],
                deadline: original.snapshot().required_deadline().unwrap(),
            }, connection,
        ));
        tokio::select! {
            arrived = arrival => arrived.unwrap(),
            result = &mut pending => panic!("request completed before its real response boundary: {result:?}"),
        }
        let global = ConfigStore::global(&fixture.installation).unwrap();
        let snapshot = ConfigSnapshot::new(fixture.config.brand(), [global.load().unwrap()]).unwrap();
        let mut settings = model_api::ModelSettings::read(&snapshot).unwrap().unwrap();
        let slot = settings.slots[0].name.clone();
        let model_api::RuntimeSettings::EmbeddingGemma2 { threads, .. } = &mut settings.slots[0].runtime else {
            panic!("qualified pinned GGUF CPU model")
        };
        *threads = NonZeroU32::new(3).unwrap();
        let valid = serde_json::to_value(settings).unwrap();
        global.set(ConfigKey::new("daemon.models").unwrap(), valid.clone(), Duration::from_secs(5), &cancellation).unwrap();
        let resolved = crate::foreground_memory_jobs::resolve_global_models(&fixture.installation, active_provider(&fixture.installation, &fixture.config, &original).unwrap().as_ref(), original.snapshot()).unwrap();
        let new_id = resolved.config_id(&slot).unwrap();
        assert_ne!(new_id, old_id);
        let current = changed_model(&server, &mut refreshes, Some(new_id)).await.unwrap();
        assert_eq!(current.local.as_ref().unwrap().expected.vector_id(), vector_id);
        release.send(()).unwrap();
        let output = pending.as_mut().await.unwrap();
        assert_eq!(output.config_id, old_id);
        assert_eq!(output.vectors.len(), 1);
        drop(pending);
        drop(old);
        let mut generation = None;
        for (project, id, version) in projects.iter() {
            let selected = model_api::ModelServiceClient::embed(
                &current.local.as_ref().unwrap().client, &lease, auth.runtime(), &current.local.as_ref().unwrap().expected,
                model_api::EmbedOperation { input: EmbedInput::Query, texts: vec!["a b".into()], deadline: original.snapshot().required_deadline().unwrap() }, TcpStream::connect(current.local.as_ref().unwrap().address),
            ).await.unwrap();
            assert_eq!(selected.config_id, new_id);
            assert!(selected.generation > output.generation);
            generation = Some(selected.generation);
            let indexed = current.index(project, &graphit_daemon::supervisor::ProjectContext {
                id: *id, version: version.clone(),
            }, &cancellation).await.unwrap();
            assert!(!matches!(indexed, model_api::MemoryEmbeddingIndexRead::NoCorpus));
        }
        let wrong = graphit_daemon::supervisor::ProjectContext { id: projects[1].1, version: projects[1].2.clone() };
        assert_eq!(current.index(&projects[0].0, &wrong, &cancellation).await.unwrap_err().kind(), io::ErrorKind::InvalidInput);
        refreshes.borrow_and_update();
        global.set(ConfigKey::new("daemon.models").unwrap(), json!({"slots":"invalid"}), Duration::from_secs(5), &cancellation).unwrap();
        refresh_error(&mut refreshes).await;
        assert_eq!(server.semantic_model().unwrap().unwrap().local.as_ref().unwrap().expected.config_id(), new_id);
        let still_valid = current.local.as_ref().unwrap().client.embed(&lease, auth.runtime(), &current.local.as_ref().unwrap().expected,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["a".into()],
                deadline: original.snapshot().required_deadline().unwrap(),
            }, TcpStream::connect(current.local.as_ref().unwrap().address)).await.unwrap();
        assert_eq!(still_valid.config_id, new_id);
        assert_eq!(Some(still_valid.generation), generation);
        refreshes.borrow_and_update();
        global.set(ConfigKey::new("daemon.models").unwrap(), json!({"slots":[]}), Duration::from_secs(5), &cancellation).unwrap();
        changed_model(&server, &mut refreshes, None).await;
        let disabled = current.local.as_ref().unwrap().client.embed(
            &lease, auth.runtime(), &current.local.as_ref().unwrap().expected,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["disabled selection".into()],
                deadline: original.snapshot().required_deadline().unwrap(),
            }, TcpStream::connect(current.local.as_ref().unwrap().address),
        ).await.unwrap_err();
        assert_eq!(disabled.kind(), model_api::ModelErrorKind::Selection,
            "an acknowledged disabled slot must reject fresh inference even through a retained old resource");
        let (status, observed) = crate::foreground_project_control::send(
            &fixture.installation,
            source_address,
            &reqwest::Client::new(),
            (native, fixture.policy.content()),
            &crate::foreground_project_control::ProjectCommand::MaintainCheckout {
                project_id: projects[1].1,
                project_dir: graphit_core::native_path::NativePath::new(projects[1].0.root().to_owned()).unwrap(),
                version: projects[1].2.version().to_owned(),
                detached_base: String::new(),
                select: true,
            },
            &original,
        ).await.unwrap();
        assert!(status.is_success(), "disabled-model checkout maintenance: {observed}");
        assert!(server.semantic_model().unwrap().is_none());
        drop(current);
        refreshes.borrow_and_update();
        global.set(ConfigKey::new("daemon.models").unwrap(), valid, Duration::from_secs(5), &cancellation).unwrap();
        let resumed = changed_model(&server, &mut refreshes, Some(new_id)).await.unwrap();
        let result = resumed.local.as_ref().unwrap().client.embed(&lease, auth.runtime(), &resumed.local.as_ref().unwrap().expected,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["b".into()],
                deadline: original.snapshot().required_deadline().unwrap(),
            }, TcpStream::connect(resumed.local.as_ref().unwrap().address)).await.unwrap();
        assert_eq!(result.config_id, new_id);
        assert!(result.generation > still_valid.generation);
        drop(resumed);
        refreshes.borrow_and_update();
        global.set(ConfigKey::new("daemon.models").unwrap(), json!({"slots":[]}), Duration::from_secs(5), &cancellation).unwrap();
        changed_model(&server, &mut refreshes, None).await;
        for (index, (project, id, _)) in projects.iter().enumerate() {
            let (status, observed) = crate::foreground_project_control::send(
                &fixture.installation, source_address, &reqwest::Client::new(),
                (native, fixture.policy.content()),
                &crate::foreground_project_control::ProjectCommand::RetireCheckout {
                    project_id: *id,
                    project_dir: graphit_core::native_path::NativePath::new(project.root().to_owned()).unwrap(),
                }, &original,
            ).await.unwrap();
            assert!(status.is_success(), "disabled-model checkout retirement: {observed}");
            if index + 1 < projects.len() {
                assert!(TcpStream::connect(address).await.is_ok(), "remaining project retains lightweight control");
            }
        }
        assert!(TcpStream::connect(address).await.is_err(), "last model consumer closes child before Source shutdown");
        tokio::time::timeout(Duration::from_secs(30), async {
            while old_metadata.upgrade().is_some() { tokio::task::yield_now().await; }
        }).await.expect("old App metadata released after its borrowers completed");
    }).catch_unwind().await;
    service.cancel();
    let stopped = running.await.unwrap();
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
    stopped.unwrap();
    assert!(TcpStream::connect(address).await.is_err());
}

#[tokio::test]
async fn browser_owner_without_projects_refreshes_model_and_joins_last_owner() {
    let mut fixture = Fixture::new();
    fixture.configure_browser("https://broker.example", false);
    fixture.install_model();
    let cancellation = Cancellation::default();
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &[],
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &original,
    )
    .await
    .unwrap();
    assert!(foreground.projects.as_ref().unwrap().is_empty());
    let server = foreground.server.as_ref().unwrap().clone();
    let initial = server.semantic_model().unwrap().unwrap();
    let address = initial.local.as_ref().unwrap().address;
    let auth = Arc::clone(&foreground.auth);
    let mut refreshes = foreground.model_refreshes.subscribe();
    let service = Cancellation::default();
    let signal = service.clone();
    let running = tokio::spawn(async move { foreground.run(&signal).await });
    let exercise = std::panic::AssertUnwindSafe(async {
        let issuer = auth.issuer();
        let runtime = auth.runtime().check(original.snapshot()).unwrap();
        let caller = issuer
            .authenticate_runtime(
                std::str::from_utf8(runtime.secret(original.snapshot()).unwrap()).unwrap(),
                &original,
            )
            .unwrap();
        let mut binding = issuer
            .capture_service(&caller, graphit_auth::ServicePurpose::Embedding)
            .unwrap();
        let lease = binding.local().unwrap();
        let first = initial
            .local
            .as_ref()
            .unwrap()
            .client
            .embed(
                &lease,
                auth.runtime(),
                &initial.local.as_ref().unwrap().expected,
                graphit_daemon::model_service::EmbedOperation {
                    input: EmbedInput::Query,
                    texts: vec!["a".into()],
                    deadline: original.snapshot().required_deadline().unwrap(),
                },
                TcpStream::connect(address),
            )
            .await
            .unwrap();
        let global = ConfigStore::global(&fixture.installation).unwrap();
        let snapshot =
            ConfigSnapshot::new(fixture.config.brand(), [global.load().unwrap()]).unwrap();
        let mut settings = model_api::ModelSettings::read(&snapshot).unwrap().unwrap();
        let slot = settings.slots[0].name.clone();
        let model_api::RuntimeSettings::EmbeddingGemma2 { threads, .. } =
            &mut settings.slots[0].runtime
        else {
            panic!("qualified CPU runtime")
        };
        *threads = NonZeroU32::new(3).unwrap();
        global
            .set(
                ConfigKey::new("daemon.models").unwrap(),
                serde_json::to_value(settings).unwrap(),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
        let resolved = crate::foreground_memory_jobs::resolve_global_models(
            &fixture.installation,
            active_provider(&fixture.installation, &fixture.config, &original)
                .unwrap()
                .as_ref(),
            original.snapshot(),
        )
        .unwrap();
        let current = changed_model(
            &server,
            &mut refreshes,
            Some(resolved.config_id(&slot).unwrap()),
        )
        .await
        .unwrap();
        let next = current
            .local
            .as_ref()
            .unwrap()
            .client
            .embed(
                &lease,
                auth.runtime(),
                &current.local.as_ref().unwrap().expected,
                graphit_daemon::model_service::EmbedOperation {
                    input: EmbedInput::Query,
                    texts: vec!["b".into()],
                    deadline: original.snapshot().required_deadline().unwrap(),
                },
                TcpStream::connect(current.local.as_ref().unwrap().address),
            )
            .await
            .unwrap();
        assert_ne!(first.config_id, next.config_id);
        assert!(next.generation > first.generation);
        global
            .set(
                ConfigKey::new("daemon.models").unwrap(),
                json!({"slots":[]}),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
        changed_model(&server, &mut refreshes, None).await;
        assert!(
            TcpStream::connect(address).await.is_ok(),
            "Browser retains lightweight control with no PROJECT"
        );
    })
    .catch_unwind()
    .await;
    service.cancel();
    let stopped = running.await.unwrap();
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
    stopped.unwrap();
    assert!(TcpStream::connect(address).await.is_err());
}
