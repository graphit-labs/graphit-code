//! Distinguish the run deadline from expiry of the actual registered Web entry.
use super::*;

#[derive(Clone, Copy)]
enum Expires {
    Original,
    WebEntry,
}

async fn exercise(expires: Expires) {
    let (_temp, tasks, raw) = fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &[],
        Some(&[
            Permission::ReadUser,
            Permission::MutateUser,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ]),
        false,
    );
    let mut browser = policy();
    if matches!(expires, Expires::WebEntry) {
        browser.session_lifetime = Duration::from_secs(2);
    }
    let mut fixture = Fixture::with_tasks(browser, Some((tasks, raw))).await;
    fixture.login().await;
    let (web, entry) = fixture
        .service
        .0
        .sessions
        .lock()
        .await
        .iter()
        .next()
        .map(|(key, value)| (key.clone(), Arc::clone(value)))
        .unwrap();
    let tasks = &fixture.tasks.as_ref().unwrap().0;
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("Live MCP expiry metadata fixture"));
    let resources = tasks.record_resources(&snapshot);
    let cancellation = Cancellation::default();
    let budget = if matches!(expires, Expires::Original) {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(90)
    };
    let mut original = RequestContext::new(
        resources.policy.content,
        &cancellation,
        Instant::now() + budget,
    )
    .unwrap();
    let rendered = render_mcp_agent(&mut original);
    let bearer = zeroize::Zeroizing::new(entry.credentials.lock().await.access_token().to_owned());
    let flow = fixture
        .service
        .0
        .flow(entry.provider)
        .unwrap_or_else(|_| panic!("Live MCP expiry flow fixture"));
    let (provider, verifier) = &fixture.service.0.providers[entry.provider];
    let caller = flow
        .verify_access(
            &entry.registered,
            &bearer,
            BrokerRequestContext::new(original.snapshot()),
        )
        .await
        .unwrap();
    let identity = (&caller, verifier.as_ref(), provider.as_str());
    let issuer = resources.auth.issuer();
    let scope = crate::foreground_user_memory::capture(
        issuer,
        resources.auth.installation(),
        &caller,
        verifier,
        &original,
        resources.policy,
    )
    .unwrap();
    let binding = scope.binding().unwrap();
    let capture = || ScopeCapture {
        live: &scope,
        record: &binding,
    };
    let read = issuer
        .admit_web_user_knowledge_read(&caller, verifier, &original, provider, capture())
        .unwrap();
    let write = issuer
        .admit_web_user_knowledge_write(&caller, verifier, &original, provider, capture())
        .unwrap();
    let access = Access::user(&read, Some(&write)).unwrap();
    drop(
        ArtifactStore::new(scope.clone(), resources.policy.write.artifacts)
            .writer_with_context(original.snapshot())
            .unwrap(),
    );
    let backend = task::backend(&format!(
        "{}/v1/project/access/authorize",
        fixture.issuer.origin
    ));
    let live = history();
    let created = live
        .create(
            &access,
            CreateRequest {
                request_id: "live-mcp-expiry".into(),
                project: ProjectSelection {
                    project_id: resources.projects[0].1.to_string(),
                    project_version: resources.projects[0].2.version().into(),
                },
                binding: NativeBinding::KiroV3,
                agent: artifact("agent", "fixture/expiry"),
                artifacts: vec![],
                title: None,
                prompt: None,
                model: "fixture-model".into(),
            },
        )
        .unwrap();
    let turn = SendRequest {
        turn_id: "expiring-turn".into(),
        text: "read before expiry".into(),
    };
    prepare_mcp_history(
        &live, &access, &created, &resources, &backend, identity, &original, &rendered,
    )
    .await;
    live.emit_fixture(
        &access,
        &created.id,
        Some(&turn.turn_id),
        EventData::State {
            state: SessionState::Running,
        },
    )
    .unwrap();
    live.emit_fixture(
        &access,
        &created.id,
        Some(&turn.turn_id),
        EventData::Prompt {
            text: turn.text.clone(),
        },
    )
    .unwrap();
    let stored = live
        .read_contexts(&access, &created.id, &created.cursor.generation)
        .unwrap();
    let session = stored.session;
    let read_contexts = stored.contexts;
    let broker = LiveMcpBroker::new();
    let mut route = broker
        .begin_run(Registration {
            sessions: &fixture.service,
            web_session: &web,
            entry: Arc::clone(&entry),
            session: &session,
            read_contexts: &read_contexts,
            turn: &turn,
            identity,
            original: &original,
        })
        .await
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = broker.router(resources.policy.content, fixture.stop.clone());
    fixture.servers.push(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let installation = launch_installation(&resources);
    let launch = route
        .launch(
            identity,
            &original,
            address,
            &installation,
            &installation.root().join("native-bin/graphit"),
        )
        .unwrap();
    let mut transport = process(&original);
    launch
        .apply_to(original.snapshot(), &mut transport)
        .unwrap();
    let uri = env(&transport, "GRAPHIT_LIVE_MCP_URL");
    let actions = async {
        let connection = client(&uri, &bearer, &web).await;
        value(
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({})))
                .await
                .unwrap(),
        );
        let calls = fixture.issuer.project_calls.load(Ordering::SeqCst);
        let deadline = match expires {
            Expires::Original => original.deadline().unwrap(),
            Expires::WebEntry => entry.expires,
        };
        tokio::time::sleep_until(deadline.into()).await;
        let denied = connection
            .peer()
            .call_tool(call(live_agent_tools::HEADS, json!({})))
            .await
            .unwrap();
        assert_eq!(denied.is_error, Some(true));
        let code = &denied.structured_content.as_ref().unwrap()["error"]["code"];
        match expires {
            Expires::Original => {
                assert_eq!(code, "deadline");
                assert!(!entry.cancelled.is_cancelled());
                assert!(entry.expires > Instant::now());
            }
            Expires::WebEntry => {
                assert_eq!(code, "permission_denied");
                original.check().unwrap();
                caller
                    .check(verifier, BrokerRequestContext::new(original.snapshot()))
                    .unwrap();
            }
        }
        assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), calls);
        assert!(!cancellation.is_cancelled());
        connection.cancel().await.unwrap();
    };
    tokio::pin!(actions);
    {
        let serving = route.serve(run_context(
            &resources,
            &backend,
            &original,
            &access,
            &live,
            (&session, &turn),
            &[],
        ));
        tokio::pin!(serving);
        match expires {
            Expires::Original => {
                let run_ended = tokio::select! {result=&mut serving=>{assert_eq!(result.unwrap_err().kind(),io::ErrorKind::TimedOut);true},()=&mut actions=>false};
                if run_ended {
                    actions.await;
                } else {
                    assert_eq!(serving.await.unwrap_err().kind(), io::ErrorKind::TimedOut);
                }
            }
            Expires::WebEntry => {
                tokio::select! {()=&mut actions=>{},result=&mut serving=>panic!("run stopped before Web expiry response: {}",result.is_ok())};
            }
        }
    }
    drop(route);
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn live_mcp_run_deadline_denies_without_a_new_project_read() {
    exercise(Expires::Original).await;
}

#[tokio::test]
async fn live_mcp_actual_web_entry_expiry_denies_while_original_and_jwt_stay_valid() {
    exercise(Expires::WebEntry).await;
}
