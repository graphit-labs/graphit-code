use super::*;
use crate::foreground_knowledge_sources::{
    ForegroundKnowledgeSources, KnowledgeSemanticStatus, KnowledgeSourceConfig,
};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture, ScopeInput,
};
use graphit_core::{
    brand::Brand,
    config::{ConfigDocument, ConfigSource, ConfigStore},
    project_identity::ProjectMetadata,
};
use graphit_data::{
    knowledge_document_update::KnowledgeDocumentUpdateResult,
    knowledge_index::{
        KnowledgeDateRead, KnowledgeIndexContentLimits, KnowledgeIndexReadLimits,
        read_native_knowledge_dates,
    },
};
use graphit_knowledge::desired_reconciliation::KnowledgeDesiredReconciliationResult;
use serde_json::json;
use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Instant;
use std::{collections::BTreeSet, error::Error};

fn fixture() -> (
    tempfile::TempDir,
    InstallationPaths,
    ProjectPaths,
    ConfigSnapshot,
    HubResourcePolicy,
) {
    let temp = tempfile::tempdir().unwrap();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let project = ProjectPaths::at(&root, &installation).unwrap();
    let cancel = Cancellation::default();
    ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .ensure(&ProjectMetadata::new("actual-knowledge-project").unwrap())
        .unwrap();
    let pairs = [
        ("modules.ast", false),
        ("modules.memory", false),
        ("modules.knowledge", true),
        ("modules.embedding", true),
        ("modules.dream", false),
    ];
    let config = ConfigSnapshot::new(
        installation.brand(),
        [ConfigDocument::new(
            installation.brand(),
            ConfigSource::Global,
            pairs
                .into_iter()
                .map(|(key, value)| (ConfigKey::new(key).unwrap(), json!(value))),
        )
        .unwrap()],
    )
    .unwrap();
    let store = ConfigStore::global(&installation).unwrap();
    for (key, value) in pairs {
        store
            .set(
                ConfigKey::new(key).unwrap(),
                json!(value),
                Duration::from_secs(5),
                &cancel,
            )
            .unwrap();
    }
    let policy = HubResourcePolicy::resolve(&config).unwrap();
    (temp, installation, project, config, policy)
}

#[test]
fn knowledge_native_batch_current_thread_serves_authenticated_http_and_joins() {
    std::thread::Builder::new()
        .name("knowledge-source-reactor".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(actual_current_thread_batch_scenario())
                .unwrap();
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn actual_current_thread_batch_scenario() -> Result<(), Box<dyn Error>> {
    use futures_util::FutureExt;
    use graphit_data::knowledge_index::{KnowledgeIndexRead, read_native_knowledge_index};

    let (_temp, installation, project, config, policy) = fixture();
    std::fs::create_dir(project.root().join("docs"))?;
    let startup_cancel = Cancellation::default();
    let startup = policy.request_context(Instant::now(), &startup_cancel)?;
    let owner = Box::pin(ForegroundSource::start_and_publish(
        &installation,
        &[ProjectBootstrapInput {
            project: &project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        }],
        0,
        &config,
        NativeBinding::Codex,
        Vec::new(),
        &startup,
    ))
    .await?;
    let address = owner.address().unwrap();
    let auth = Arc::clone(&owner.auth);
    let (_, id, version) = owner.projects.as_ref().unwrap()[0].clone();
    let selected = graphit_daemon::supervisor::ProjectContext { id, version };
    let rescan = owner.supervisors.rescan_handle(&selected.key()).unwrap();
    let observe_cancel = Cancellation::default();
    let observe = policy.request_context(Instant::now(), &observe_cancel)?;
    let captured =
        CapturedPublicationScope::project(&project, id, owner.scope, observe.snapshot())?;
    let record = captured.binding()?;
    let runtime = auth.runtime().check(observe.snapshot())?;
    let raw = zeroize::Zeroizing::new(
        std::str::from_utf8(runtime.secret(observe.snapshot())?)?.to_owned(),
    );
    let caller = auth.issuer().authenticate_runtime(raw.as_str(), &observe)?;
    let authority = ProjectRecordAuthority::new(auth.issuer());
    let read = authority.admit_authenticated_read(
        &caller,
        &observe,
        ScopeInput {
            installation: auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &captured,
                    record: &record,
                },
                version: &selected.version,
            }),
            providers: &[],
            binding: NativeBinding::Codex,
            global_version: "",
        },
        ProjectReadModule::Knowledge,
    )?;
    let limits = KnowledgeIndexReadLimits {
        content: KnowledgeIndexContentLimits {
            max_identifier_bytes: NonZeroUsize::new(policy.content().max_scalar_bytes).unwrap(),
        },
        native: owner.native,
        batch_rows: NonZeroU32::new(1).unwrap(),
    };
    let stop = Cancellation::default();
    let service_stop = stop.clone();
    let mut running = tokio::task::JoinSet::new();
    running.spawn(async move { owner.run(&service_stop).await });
    let mut early_completion = None;
    let result = std::panic::AssertUnwindSafe(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        std::fs::write(
            project.root().join("docs/reactor.md"),
            "---\nid: reactor-page\n---\n# Reactor\nActual Native batch body\n",
        )?;
        assert!(rescan.request_rescan());
        let response = client
            .post(format!("http://{address}/admin/daemon"))
            .header("x-graphit-auth-origin", "runtime")
            .bearer_auth(raw.as_str())
            .json(&json!({"operation":"status"}))
            .send()
            .await?;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let expires = Instant::now() + Duration::from_secs(15);
        loop {
            match Box::pin(read_native_knowledge_index(
                observe.snapshot(),
                &read,
                limits,
            ))
            .await
            {
                Ok(KnowledgeIndexRead::Published { documents, .. })
                    if documents
                        .iter()
                        .any(|document| document.body.contains("Actual Native batch body")) =>
                {
                    break;
                }
                _ if Instant::now() < expires => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                other => {
                    panic!("current-thread Source did not publish the real document: {other:?}")
                }
            }
        }
        early_completion = running.try_join_next();
        assert!(
            early_completion.is_none(),
            "Source ended before cancellation"
        );
        assert!(rescan.request_rescan());
        Ok::<(), Box<dyn Error>>(())
    })
    .catch_unwind()
    .await;
    stop.cancel();
    let (joined, late) = if let Some(completion) = early_completion {
        (Some(completion), false)
    } else {
        match tokio::time::timeout(Duration::from_secs(10), running.join_next()).await {
            Ok(completion) => (completion, false),
            Err(_) => {
                // A missed assertion deadline must not abort an owner of native work.
                (running.join_next().await, true)
            }
        }
    };
    let drained: Result<(), Box<dyn Error>> = match joined {
        Some(Ok(Ok(()))) => Ok(()),
        Some(Ok(Err(mut failure))) => {
            if let Some(owner) = failure.owner.take()
                && let Err(cleanup) = Box::pin(owner.shutdown()).await
            {
                eprintln!(
                    "Native Source cleanup failed; retained_owner={}: {cleanup:?}",
                    cleanup.owner.is_some()
                );
                return Err(Box::new(cleanup));
            }
            Err(Box::new(failure))
        }
        Some(Err(error)) => Err(Box::new(error)),
        None => Err("Source owner completion missing".into()),
    };
    let result = match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    };
    result?;
    assert!(!late, "Source cancellation exceeded the test deadline");
    drained?;
    assert!(
        !rescan.request_rescan(),
        "joined supervisor must reject new work"
    );
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn knowledge_only_watcher_publishes_without_models_and_helper_preserves_noop_and_queued_cancel()
-> Result<(), Box<dyn Error>> {
    let (_temp, installation, project, config, policy) = fixture();
    std::fs::create_dir(project.root().join("docs"))?;
    std::fs::write(
        project.root().join("docs/a.md"),
        "---\nid: actual-a\n---\n# A\nBody\n",
    )?;
    let startup_cancel = Cancellation::default();
    let startup = policy.request_context(Instant::now(), &startup_cancel)?;
    let inputs = [ProjectBootstrapInput {
        project: &project,
        explicit_version: "",
        pin: None,
        detached_base: "",
    }];
    // SDK futures stay on the heap while this fixture uses the default worker stack.
    let mut owner = Box::pin(ForegroundSource::start_and_publish(
        &installation,
        &inputs,
        0,
        &config,
        NativeBinding::Codex,
        Vec::new(),
        &startup,
    ))
    .await?;
    assert!(owner.memory_jobs.is_none());
    let (_, id, version) = owner.projects.as_ref().unwrap()[0].clone();
    let selected = graphit_daemon::supervisor::ProjectContext { id, version };
    let observe_cancel = Cancellation::default();
    let observe = policy.request_context(Instant::now(), &observe_cancel)?;
    let captured =
        CapturedPublicationScope::project(&project, selected.id, owner.scope, observe.snapshot())?;
    let record = captured.binding()?;
    let scope = ScopeInput {
        installation: owner.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &record,
            },
            version: &selected.version,
        }),
        providers: &[],
        binding: NativeBinding::Codex,
        global_version: "",
    };
    let runtime = owner.auth.runtime().check(observe.snapshot())?;
    let raw = std::str::from_utf8(runtime.secret(observe.snapshot())?)?;
    let caller = owner.auth.issuer().authenticate_runtime(raw, &observe)?;
    let authority = ProjectRecordAuthority::new(owner.auth.issuer());
    let read = authority.admit_authenticated_read(
        &caller,
        &observe,
        scope,
        ProjectReadModule::Knowledge,
    )?;
    let limits = KnowledgeIndexReadLimits {
        content: KnowledgeIndexContentLimits {
            max_identifier_bytes: NonZeroUsize::new(policy.content().max_scalar_bytes).unwrap(),
        },
        native: owner.native,
        batch_rows: NonZeroU32::new(1).unwrap(),
    };
    let expires = Instant::now() + Duration::from_secs(15);
    let observed = loop {
        match Box::pin(read_native_knowledge_dates(
            observe.snapshot(),
            &read,
            limits,
        ))
        .await
        {
            Ok(KnowledgeDateRead::Published { generation, dates }) if dates.len() == 1 => {
                break generation.version;
            }
            _ if Instant::now() < expires => tokio::time::sleep(Duration::from_millis(10)).await,
            other => panic!("actual watcher did not publish its document: {other:?}"),
        }
    };
    // Join the watcher before exercising the same production helper directly.
    // Its initial callback published without Raw/NN or a model resource.
    Box::pin(owner.supervisors.shutdown()).await?;
    // This setting bounds a model RPC, not this background document reconciliation.
    let rpc_policy = HubResourcePolicy::resolve(&ConfigSnapshot::new(
        installation.brand(),
        [ConfigDocument::new(
            installation.brand(),
            ConfigSource::Command,
            [(ConfigKey::new("hub.limits.request.deadline_ms")?, json!(1))],
        )?],
    )?)?;
    let helper = ForegroundKnowledgeSources::new(
        Arc::clone(&owner.auth),
        KnowledgeSourceConfig {
            installation: Arc::new(installation.clone()),
            base: Arc::new(config.clone()),
            policy: rpc_policy,
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::Codex,
        },
    );
    let live = Cancellation::default();
    let nn_cancelled = Cancellation::default();
    nn_cancelled.cancel();
    // A record-only hint stops inference, never the full source pass or its delta.
    let no_op = Box::pin(helper.index(
        &project,
        &selected,
        true,
        false,
        &BTreeSet::new(),
        &live,
        &nn_cancelled,
        &live,
        None,
    ))
    .await?
    .expect("Knowledge is enabled");
    assert!(matches!(
        no_op.semantics,
        KnowledgeSemanticStatus::ModelsUnavailable
    ));
    assert!(
        matches!(no_op.documents, KnowledgeDesiredReconciliationResult::Existing(
        KnowledgeDocumentUpdateResult::Unchanged { generation }) if generation.version == observed)
    );
    let code = [project.root().join("code.rs")].into_iter().collect();
    assert!(
        Box::pin(helper.index(
            &project, &selected, false, false, &code, &live, &live, &live, None
        ))
        .await?
        .is_none()
    );
    use graphit_data::knowledge_index::{KnowledgeIndexRead, read_native_knowledge_index};
    let document = project.root().join("docs/a.md");
    std::fs::write(&document, "---\nid: actual-a\n---\n# A\nChanged body\n")?;
    let changed = [document].into_iter().collect();
    let updated = Box::pin(helper.index(
        &project,
        &selected,
        false,
        false,
        &changed,
        &live,
        &nn_cancelled,
        &live,
        None,
    ))
    .await?
    .expect("a document event must reach reconciliation despite stopped inference");
    assert!(matches!(updated.documents,
        KnowledgeDesiredReconciliationResult::Existing(
            KnowledgeDocumentUpdateResult::Updated { ref generation, .. }
        ) if generation.version > observed));
    let KnowledgeIndexRead::Published {
        generation: changed_generation,
        documents,
    } = Box::pin(read_native_knowledge_index(
        observe.snapshot(),
        &read,
        limits,
    ))
    .await?
    else {
        panic!("the updated document must remain published")
    };
    assert_eq!(documents.len(), 1);
    assert!(documents[0].body.contains("Changed body"));

    let readme = project.root().join("README.MD");
    std::fs::write(&readme, "# Readme\nChanged root readme\n")?;
    let changed = [readme].into_iter().collect();
    Box::pin(helper.index(
        &project, &selected, false, false, &changed, &live, &live, &live, None,
    ))
    .await?
    .expect("a selected README event must reach reconciliation");
    let KnowledgeIndexRead::Published {
        generation,
        documents,
    } = Box::pin(read_native_knowledge_index(
        observe.snapshot(),
        &read,
        limits,
    ))
    .await?
    else {
        panic!("the document and README must remain published")
    };
    assert!(generation.version > changed_generation.version);
    assert_eq!(documents.len(), 2);
    assert!(
        documents
            .iter()
            .any(|document| document.body.contains("Changed root readme"))
    );
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let error = Box::pin(helper.index(
        &project,
        &selected,
        true,
        false,
        &BTreeSet::new(),
        &cancelled,
        &cancelled,
        &cancelled,
        None,
    ))
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    Box::pin(owner.shutdown()).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn knowledge_only_watcher_publishes_empty_selection_without_docs_directory_or_models()
-> Result<(), Box<dyn Error>> {
    for present in [false, true] {
        let (_temp, installation, project, config, policy) = fixture();
        if present {
            std::fs::create_dir(project.root().join("docs"))?;
        }
        let startup_cancel = Cancellation::default();
        let startup = policy.request_context(Instant::now(), &startup_cancel)?;
        let inputs = [ProjectBootstrapInput {
            project: &project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        }];
        let owner = ForegroundSource::start_and_publish(
            &installation,
            &inputs,
            0,
            &config,
            NativeBinding::Codex,
            Vec::new(),
            &startup,
        )
        .await?;
        let outcome: Result<(), Box<dyn Error>> = async {
            assert!(owner.memory_jobs.is_none());
            let (_, id, version) = owner.projects.as_ref().unwrap()[0].clone();
            let selected = graphit_daemon::supervisor::ProjectContext { id, version };
            let observe_cancel = Cancellation::default();
            let observe = policy.request_context(Instant::now(), &observe_cancel)?;
            let captured = CapturedPublicationScope::project(
                &project,
                selected.id,
                owner.scope,
                observe.snapshot(),
            )?;
            let record = captured.binding()?;
            let scope = ScopeInput {
                installation: owner.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &captured,
                        record: &record,
                    },
                    version: &selected.version,
                }),
                providers: &[],
                binding: NativeBinding::Codex,
                global_version: "",
            };
            let runtime = owner.auth.runtime().check(observe.snapshot())?;
            let caller = owner.auth.issuer().authenticate_runtime(
                std::str::from_utf8(runtime.secret(observe.snapshot())?)?,
                &observe,
            )?;
            let authority = ProjectRecordAuthority::new(owner.auth.issuer());
            let read = authority.admit_authenticated_read(
                &caller,
                &observe,
                scope,
                ProjectReadModule::Knowledge,
            )?;
            let limits = KnowledgeIndexReadLimits {
                content: KnowledgeIndexContentLimits {
                    max_identifier_bytes: NonZeroUsize::new(policy.content().max_scalar_bytes)
                        .unwrap(),
                },
                native: owner.native,
                batch_rows: NonZeroU32::new(1).unwrap(),
            };
            let expires = Instant::now() + Duration::from_secs(15);
            loop {
                let actual = read_native_knowledge_dates(observe.snapshot(), &read, limits).await;
                match actual {
                    Ok(KnowledgeDateRead::Published { generation, dates }) => {
                        assert!(dates.is_empty());
                        assert_eq!(generation.version, 1);
                        break;
                    }
                    _ if Instant::now() < expires => {
                        tokio::time::sleep(Duration::from_millis(10)).await
                    }
                    other => {
                        return Err(std::io::Error::other(format!(
                            "empty watcher publication absent; docs_present={present}: {other:?}"
                        ))
                        .into());
                    }
                }
            }
            Ok(())
        }
        .await;
        let shutdown = owner.shutdown().await;
        outcome?;
        shutdown?;
    }
    Ok(())
}

#[test]
fn knowledge_controls_reload_real_project_configuration_without_publishing_invalid_or_disabled_selection()
 {
    std::thread::Builder::new()
        .name("knowledge-foreground-controls".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(actual_foreground_controls_scenario());
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn actual_foreground_controls_scenario() {
    use futures_util::FutureExt;
    use graphit_data::knowledge_index::{KnowledgeIndexRead, read_native_knowledge_index};

    let (_temp, installation, project, config, policy) = fixture();
    std::fs::create_dir(project.root().join("docs")).unwrap();
    std::fs::create_dir(project.root().join("handbook")).unwrap();
    for (path, body) in [
        ("docs/a.md", "Original body"),
        ("handbook/b.md", "Handbook body"),
    ] {
        std::fs::write(
            project.root().join(path),
            format!("---\nid: control-page\n---\n# Controls\n{body}\n"),
        )
        .unwrap();
    }
    let startup_cancel = Cancellation::default();
    let startup = policy
        .request_context(Instant::now(), &startup_cancel)
        .unwrap();
    let inputs = [ProjectBootstrapInput {
        project: &project,
        explicit_version: "",
        pin: None,
        detached_base: "",
    }];
    let mut owner = ForegroundSource::start_and_publish(
        &installation,
        &inputs,
        0,
        &config,
        NativeBinding::Codex,
        Vec::new(),
        &startup,
    )
    .await
    .unwrap();
    owner.supervisors.shutdown().await.unwrap();
    let (_, id, version) = owner.projects.as_ref().unwrap()[0].clone();
    let selected = graphit_daemon::supervisor::ProjectContext { id, version };
    let helper = ForegroundKnowledgeSources::new(
        Arc::clone(&owner.auth),
        KnowledgeSourceConfig {
            installation: Arc::new(installation.clone()),
            base: Arc::new(config),
            policy: policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::Codex,
        },
    );
    let result = std::panic::AssertUnwindSafe(async {
        let live = Cancellation::default();
        let observe = policy.request_context(Instant::now(), &live).unwrap();
        let captured = CapturedPublicationScope::project(
            &project,
            selected.id,
            owner.scope,
            observe.snapshot(),
        )
        .unwrap();
        let record = captured.binding().unwrap();
        let scope = ScopeInput {
            installation: owner.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &captured,
                    record: &record,
                },
                version: &selected.version,
            }),
            providers: &[],
            binding: NativeBinding::Codex,
            global_version: "",
        };
        let runtime = owner.auth.runtime().check(observe.snapshot()).unwrap();
        let caller = owner
            .auth
            .issuer()
            .authenticate_runtime(
                std::str::from_utf8(runtime.secret(observe.snapshot()).unwrap()).unwrap(),
                &observe,
            )
            .unwrap();
        let authority = ProjectRecordAuthority::new(owner.auth.issuer());
        let read = authority
            .admit_authenticated_read(&caller, &observe, scope, ProjectReadModule::Knowledge)
            .unwrap();
        let limits = KnowledgeIndexReadLimits {
            content: KnowledgeIndexContentLimits {
                max_identifier_bytes: NonZeroUsize::new(policy.content().max_scalar_bytes).unwrap(),
            },
            native: owner.native,
            batch_rows: NonZeroU32::new(1).unwrap(),
        };
        let paths = BTreeSet::new();
        let first = helper
            .index(
                &project, &selected, true, false, &paths, &live, &live, &live, None,
            )
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            first.semantics,
            KnowledgeSemanticStatus::ModelsUnavailable
        ));
        let KnowledgeIndexRead::Published {
            generation: initial,
            documents,
        } = read_native_knowledge_index(observe.snapshot(), &read, limits)
            .await
            .unwrap()
        else {
            panic!("actual first corpus required")
        };
        assert_eq!(documents.len(), 1);
        assert!(documents[0].body.contains("Original body"));

        let store = ConfigStore::project(&project).unwrap();
        store
            .set(
                ConfigKey::new("knowledge.docs_dir").unwrap(),
                json!("handbook"),
                Duration::from_secs(5),
                &live,
            )
            .unwrap();
        store
            .set(
                ConfigKey::new("modules.knowledge").unwrap(),
                json!(false),
                Duration::from_secs(5),
                &live,
            )
            .unwrap();
        assert!(
            helper
                .index(
                    &project, &selected, true, false, &paths, &live, &live, &live, None
                )
                .await
                .unwrap()
                .is_none()
        );
        let KnowledgeDateRead::Published {
            generation: disabled,
            ..
        } = read_native_knowledge_dates(observe.snapshot(), &read, limits)
            .await
            .unwrap()
        else {
            panic!("disabling cannot erase the published corpus")
        };
        assert_eq!(disabled, initial);

        store
            .set(
                ConfigKey::new("modules.knowledge").unwrap(),
                json!(true),
                Duration::from_secs(5),
                &live,
            )
            .unwrap();
        let code_paths = [project.root().join("code.rs")].into_iter().collect();
        assert!(
            helper
                .index(
                    &project,
                    &selected,
                    false,
                    false,
                    &code_paths,
                    &live,
                    &live,
                    &live,
                    None
                )
                .await
                .unwrap()
                .is_none()
        );
        let changed = helper
            .index(
                &project, &selected, true, false, &paths, &live, &live, &live, None,
            )
            .await
            .unwrap()
            .unwrap();
        let KnowledgeDesiredReconciliationResult::Existing(
            KnowledgeDocumentUpdateResult::Updated {
                generation,
                recovery_key,
            },
        ) = changed.documents
        else {
            panic!("controls must reconcile the newly selected real source")
        };
        assert_eq!(generation.version, initial.version + 1);
        assert!(!recovery_key.transaction_id().is_empty());
        let KnowledgeIndexRead::Published {
            generation: actual,
            documents,
        } = read_native_knowledge_index(observe.snapshot(), &read, limits)
            .await
            .unwrap()
        else {
            panic!("updated corpus required")
        };
        assert_eq!(actual, generation);
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].slug, "control-page");
        assert!(documents[0].body.contains("Handbook body"));
        assert!(!documents[0].body.contains("Original body"));

        store
            .set(
                ConfigKey::new("knowledge.docs_dir").unwrap(),
                json!(true),
                Duration::from_secs(5),
                &live,
            )
            .unwrap();
        let invalid = helper
            .index(
                &project, &selected, true, false, &paths, &live, &live, &live, None,
            )
            .await
            .unwrap_err();
        let invalid_settings = invalid
            .get_ref()
            .and_then(|cause| {
                cause.downcast_ref::<graphit_knowledge::settings::KnowledgeSettingsError>()
            })
            .expect("configuration error must preserve its concrete cause");
        assert_eq!(invalid_settings.kind(), io::ErrorKind::InvalidData);
        let cancelled = Cancellation::default();
        cancelled.cancel();
        assert_eq!(
            helper
                .index(
                    &project, &selected, true, false, &paths, &cancelled, &cancelled, &cancelled,
                    None
                )
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        let KnowledgeDateRead::Published {
            generation: intact, ..
        } = read_native_knowledge_dates(observe.snapshot(), &read, limits)
            .await
            .unwrap()
        else {
            panic!("invalid controls or cancellation cannot erase documents")
        };
        assert_eq!(intact, actual);
    })
    .catch_unwind()
    .await;
    let shutdown = owner.shutdown().await;
    if let Err(panic) = result {
        eprintln!(
            "controls cleanup after fixture failure: foreground_owner_ok={}",
            shutdown.is_ok()
        );
        std::panic::resume_unwind(panic);
    }
    shutdown.unwrap();
}

#[allow(
    dead_code,
    reason = "shared HTTP fixture also supplies barriers, headers and origins to other targets"
)]
use crate::rerank_test_http as embedding_http;

#[test]
fn knowledge_foreground_actual_direct_preserves_document_effect_on_semantic_failure_and_joins() {
    std::thread::Builder::new()
        .name("knowledge-foreground-direct".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(actual_foreground_model_scenario(false));
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
#[ignore = "requires pinned BGE F16 and the qualified model server executable"]
fn knowledge_foreground_actual_local_bge_rerank_uses_control_without_local_embedding() {
    std::thread::Builder::new()
        .name("knowledge-foreground-local-rerank".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(actual_foreground_model_scenario(true))
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn actual_foreground_model_scenario(local_rerank: bool) {
    use crate::{
        foreground_knowledge_sources::KnowledgeAfterPublication,
        foreground_memory_jobs::MemoryJobResources,
        knowledge_semantic::{KnowledgeSemanticError, KnowledgeSemanticIndexResult},
    };
    use futures_util::FutureExt;
    use graphit_data::knowledge_embeddings::{
        KnowledgeEmbeddingPublication, KnowledgeEmbeddingRead, capture_knowledge_embedding_corpus,
    };

    let (_temp, installation, project, config, _) = fixture();
    let config = ConfigSnapshot::new(
        installation.brand(),
        [
            config.layer(ConfigSource::Global).unwrap().clone(),
            ConfigDocument::new(
                installation.brand(),
                ConfigSource::Command,
                [
                    (
                        ConfigKey::new("broker.allow_loopback_http").unwrap(),
                        json!(true),
                    ),
                    // Explicit caller budget includes a cold load of the real local model.
                    (
                        ConfigKey::new("hub.limits.request.deadline_ms").unwrap(),
                        json!(180000),
                    ),
                ],
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let policy = HubResourcePolicy::resolve(&config).unwrap();
    let startup_cancel = Cancellation::default();
    let startup = policy
        .request_context(Instant::now(), &startup_cancel)
        .unwrap();
    let mut replies: Vec<_> = (0..3)
        .map(|i| {
            embedding_http::reply(json!({"data":[
                {"index":0,"embedding":[1.0+i as f32,2.0+i as f32,3.0+i as f32,4.0+i as f32]},
            ]}))
        })
        .collect();
    if !local_rerank {
        replies.extend((0..3).map(|_| {
            embedding_http::reply(json!({"results":[
                {"index":0,"relevance_score":0.1},{"index":1,"relevance_score":0.9},
            ]}))
        }));
    }
    replies.push(embedding_http::Reply {
        status: 503,
        body: "unavailable".into(),
        delay: Duration::ZERO,
    });
    if !local_rerank {
        replies.extend((0..2).map(|i| {
            embedding_http::reply(json!({"data":[
                {"index":0,"embedding":[10.0+i as f32,20.0,30.0,40.0]},
            ]}))
        }));
    }
    let first_inference = Arc::new(tokio::sync::Semaphore::new(0));
    let releases = if local_rerank {
        Vec::new()
    } else {
        vec![Arc::clone(&first_inference)]
    };
    // Native commits and deliberate no-op reads can precede the next request.
    let http_fixture = embedding_http::fixture_with_origin_and_barriers(
        |_| (replies, Vec::new()),
        releases,
        Some(startup.snapshot().required_deadline().unwrap()),
    )
    .await;
    std::fs::create_dir(project.root().join("docs")).unwrap();
    let document = project.root().join("docs/a.md");

    // The real Foreground Auth factory captures a Direct binding. No local
    // model bundle or process is needed by either indexing or query metadata.
    {
        use graphit_auth::{profile::*, store::AuthStore};
        use graphit_core::{
            auth_document::{AuthDocumentLimits, AuthDocumentStore},
            installation_bootstrap::{self, BootstrapLimits},
            installation_identity::InstallationIdentityStore,
        };
        use secrecy::SecretString;
        let private = installation_bootstrap::prepare(
            &installation,
            BootstrapLimits::new(65536, 256).unwrap(),
            startup.snapshot(),
        )
        .into_installation()
        .unwrap();
        let identity = InstallationIdentityStore::new(&installation)
            .unwrap()
            .ensure(Duration::from_secs(5), &startup_cancel)
            .unwrap();
        private.revalidate(startup.snapshot()).unwrap();
        let scope = CapturedPublicationScope::installation(
            &installation,
            identity.unit_id().clone(),
            [installation.root().to_owned()],
            ScopeLimits::new(16, 65536, 256).unwrap(),
            startup.snapshot(),
        )
        .unwrap();
        let store = AuthStore::new(
            AuthDocumentStore::capture(
                scope,
                AuthDocumentLimits::new(1048576, 65536, 256, 65536).unwrap(),
                startup.snapshot(),
            )
            .unwrap(),
            ProfileLimits {
                bytes: 1048576,
                nodes: 65536,
                scalar_bytes: 65536,
                collection_entries: 1024,
                providers: 16,
                profiles: 16,
            },
            &startup,
        )
        .unwrap();
        let publication = store
            .mutate(store.load(&startup).unwrap(), &startup, |doc, original| {
                doc.add_provider(
                    "cpu".into(),
                    ProviderConfig {
                        kind: ProviderKind::Local {
                            allow_aws_credential_chain: false,
                            broker: None,
                        },
                        embedding: AiService::Direct {
                            protocol: DirectProtocol::OpenAi,
                            endpoint: http_fixture.origin.clone(),
                            model: "fixture-embedding".into(),
                            model_revision: "weights-r1".into(),
                            recipe: "fixture-document-v1".into(),
                            dimensions: 4,
                            request_dimensions: None,
                            batch_endpoint: None,
                        },
                        rerank: if local_rerank {
                            AiService::Local(LocalExecution {
                                device: LocalDevice::Cpu,
                                device_id: 0,
                            })
                        } else {
                            AiService::Direct {
                                protocol: DirectProtocol::Cohere,
                                endpoint: http_fixture.origin.clone(),
                                model: "fixture-rerank".into(),
                                model_revision: "rerank-r1".into(),
                                recipe: "fixture-cross-encoder-v1".into(),
                                dimensions: 0,
                                request_dimensions: None,
                                batch_endpoint: None,
                            }
                        },
                        s3: S3Location {
                            bucket: None,
                            region: None,
                            endpoint: None,
                            prefix: None,
                            source: S3Source::LocalCredentials,
                        },
                    },
                    doc.revision(),
                    1,
                    original,
                )?;
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("cpu").unwrap().revision(),
                    profile_revision: None,
                };
                doc.replace_profile_data(
                    ProfileData {
                        name: "local".into(),
                        provider: "cpu".into(),
                        username: "native fixture".into(),
                        organization: None,
                        teams: vec![],
                        credentials: ProfileCredentials::Local(LocalCredentials {
                            anonymous: false,
                            mcp_key: Some(Secret::new(SecretString::from("fixture-local-only"))),
                            broker_key: None,
                            embedding_api_key: Some(Secret::new(SecretString::from(
                                "fixture-direct-only",
                            ))),
                            rerank_api_key: (!local_rerank)
                                .then(|| Secret::new(SecretString::from("fixture-rerank-only"))),
                            s3: None,
                        }),
                    },
                    expected,
                    1,
                    original,
                )?;
                doc.activate("local", doc.revision(), original)?;
                Ok(())
            })
            .unwrap();
        assert!(publication.failure().is_none());
        assert!(publication.cleanup_failure().is_none());
    }
    let environment = if local_rerank {
        let executable = std::fs::canonicalize(
            std::env::var_os("GRAPHIT_TEST_MODEL_SERVER").expect("explicit qualified model server"),
        )
        .unwrap();
        crate::foreground_source::foreground_memory_jobs_tests::install_rerank_fixture_model(
            &installation,
        );
        let mut environment = vec![(
            "PATH".into(),
            executable.parent().unwrap().as_os_str().to_owned(),
        )];
        // The model child receives an explicit environment, including the
        // fixture's staging directory when the scenario selects one.
        for name in ["TMPDIR", "TMP", "TEMP"] {
            if let Some(value) = std::env::var_os(name) {
                assert!(std::path::Path::new(&value).is_absolute());
                assert!(std::path::Path::new(&value).is_dir());
                environment.push((name.into(), value));
            }
        }
        environment
    } else {
        Vec::new()
    };
    let config = crate::ast_profile_loader::project_config(
        &installation,
        &project,
        &config,
        startup.snapshot(),
    )
    .unwrap();
    let inputs = [ProjectBootstrapInput {
        project: &project,
        explicit_version: "",
        pin: None,
        detached_base: "",
    }];
    let mut owner = ForegroundSource::start_and_publish(
        &installation,
        &inputs,
        0,
        &config,
        NativeBinding::Codex,
        environment,
        &startup,
    )
    .await
    .unwrap();
    let (_, id, version) = owner.projects.as_ref().unwrap()[0].clone();
    let selected = graphit_daemon::supervisor::ProjectContext { id, version };
    // The watcher wiring is qualified by NoM06/empty09. Join it here so each
    // explicit production operation has an observable receipt and no racing writer.
    owner.supervisors.shutdown().await.unwrap();
    let auth = Arc::clone(&owner.auth);
    let scope_limits = owner.scope;
    let native_limits = owner.native;
    for (id, filename, body) in [
        ("actual-a", "a.md", "orbital literal α"),
        ("actual-b", "b.md", "orbital literal β"),
        ("actual-c", "c.md", "orbital literal γ"),
    ] {
        std::fs::write(
            project.root().join("docs").join(filename),
            format!("---\nid: {id}\n---\n# Orbital {id}\n\n{body}\n"),
        )
        .unwrap();
    }
    let helper = ForegroundKnowledgeSources::new(
        Arc::clone(&owner.auth),
        KnowledgeSourceConfig {
            installation: Arc::new(installation.clone()),
            base: Arc::new(config.clone()),
            policy: policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::Codex,
        },
    );
    let key = auth.runtime().check(startup.snapshot()).unwrap();
    let caller = auth
        .issuer()
        .authenticate_runtime(
            std::str::from_utf8(key.secret(startup.snapshot()).unwrap()).unwrap(),
            &startup,
        )
        .unwrap();
    let mut service = auth
        .issuer()
        .capture_service(&caller, graphit_auth::ServicePurpose::Embedding)
        .unwrap();
    let http = crate::search_rerank::http_client(&mut service, &config, &policy)
        .unwrap()
        .map(Arc::new);
    let resources = Arc::new(
        MemoryJobResources::remote(
            Arc::clone(&owner.auth),
            http,
            owner.broker_verifiers.clone().unwrap(),
            policy.clone(),
            owner.scope,
            owner.native,
            NativeBinding::Codex,
        )
        .unwrap(),
    );
    assert!(resources.local.is_none());
    assert!(Arc::ptr_eq(&owner.auth, &resources.auth));
    let mut pending_owner = Some(owner);
    let serving = if local_rerank {
        None
    } else {
        let owner = pending_owner.as_mut().unwrap();
        let source = owner.server.as_ref().unwrap().clone();
        let listener = owner.listener.take().unwrap();
        Some(tokio::spawn(async move { source.serve(listener).await }))
    };
    let run_stop = Cancellation::default();
    let mut running = None;
    let result = std::panic::AssertUnwindSafe(async {
        let observe_cancel = Cancellation::default();
        let observe = policy.request_context(Instant::now(), &observe_cancel).unwrap();
        let captured = CapturedPublicationScope::project(&project, selected.id, scope_limits, observe.snapshot()).unwrap();
        let record = captured.binding().unwrap();
        let scope = ScopeInput { installation: auth.installation(), project: Some(ProjectCapture {
            scope: ScopeCapture { live: &captured, record: &record }, version: &selected.version,
        }), providers: &[], binding: NativeBinding::Codex, global_version: "" };
        let runtime = auth.runtime().check(observe.snapshot()).unwrap();
        let caller = auth.issuer().authenticate_runtime(
            std::str::from_utf8(runtime.secret(observe.snapshot()).unwrap()).unwrap(), &observe,
        ).unwrap();
        let authority = ProjectRecordAuthority::new(auth.issuer());
        let read = authority.admit_authenticated_read(&caller, &observe, scope, ProjectReadModule::Knowledge).unwrap();
        let limits = KnowledgeIndexReadLimits { content: KnowledgeIndexContentLimits {
            max_identifier_bytes: NonZeroUsize::new(policy.content().max_scalar_bytes).unwrap(),
        }, native: native_limits, batch_rows: NonZeroU32::new(1).unwrap() };
        let live = Cancellation::default();
        let paths = BTreeSet::new();
        let http = reqwest::Client::builder().no_proxy().build().unwrap();
        let indexing = helper.index(&project, &selected, true, false, &paths, &live, &live, &live, Some(&resources));
        let first = if local_rerank {
            indexing.await.unwrap().unwrap()
        } else {
            let visible_before_nn = async {
                tokio::time::timeout(Duration::from_secs(20), http_fixture.observed.notified()).await.unwrap();
                // The genuine provider request is held before its response:
                // Source/FTS must already be available through the real MCP.
                let address = crate::source_entry::address(&installation, native_limits, &observe).await.unwrap();
                let connection = crate::ast_source::connect_source(&installation, address, &http, native_limits, &observe, ()).await.unwrap();
                let input = crate::knowledge_search_entry::Arguments {
                    query: "orbital".into(), mode: crate::knowledge_search_entry::Mode::Fts,
                    metric: None, top_k: Some(3), page_size: Some(3), preview: true,
                    rerank: Some(false), cursor: None, cursor_file: None, project_dir: Some(project.root().to_owned()),
                    artifact_id: None, artifact_version: None, context_alias: None,
                };
                let mut call = crate::knowledge_search_entry::prepare(&input, None, &observe).unwrap();
                crate::mcp_output::json_call(&mut call);
                let reply = connection.peer().call_tool(call).await.unwrap();
                assert_ne!(reply.is_error, Some(true), "Source/FTS before NN: {reply:?}");
                let body = reply.structured_content.unwrap();
                assert!(!body["native"]["hits"].as_array().unwrap().is_empty());
                let KnowledgeEmbeddingRead::Published(corpus) = capture_knowledge_embedding_corpus(observe.snapshot(), &read, limits).await.unwrap()
                    else { panic!("Source is published before the provider responds"); };
                assert!(corpus.semantic_generation().is_none(), "no vector has been calculated yet");
                connection.cancel().await.unwrap();
                first_inference.add_permits(1);
            };
            let (first, ()) = tokio::join!(indexing, visible_before_nn);
            first.unwrap().unwrap()
        };
        let KnowledgeSemanticStatus::Published(semantic) = first.semantics else { panic!("real model publication required") };
        let KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Committed { generation: semantic_generation, .. }) = *semantic
            else { panic!("actual sidecar commit required") };
        let KnowledgeDateRead::Published { generation: document_generation, dates } = read_native_knowledge_dates(observe.snapshot(), &read, limits).await.unwrap()
            else { panic!("actual document publication required") };
        assert_eq!(dates.len(), 3);
        let KnowledgeEmbeddingRead::Published(old_pin) = capture_knowledge_embedding_corpus(observe.snapshot(), &read, limits).await.unwrap()
            else { panic!("actual same-generation corpus required") };
        assert_eq!(old_pin.source_generation(), &document_generation);
        assert_eq!(old_pin.semantic_generation().as_ref(), Some(&semantic_generation));
        let repeat = helper.index(&project, &selected, true, false, &BTreeSet::new(), &live, &live, &live, Some(&resources)).await.unwrap().unwrap();
        assert!(matches!(repeat.documents, KnowledgeDesiredReconciliationResult::Existing(KnowledgeDocumentUpdateResult::Unchanged { ref generation }) if generation == &document_generation));
        assert!(matches!(repeat.semantics, KnowledgeSemanticStatus::Published(value) if matches!(*value, KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Unchanged))));

        #[cfg(target_os = "linux")]
        {
            // Count only this fixture's retained directory descriptors, not
            // shared process mappings, unrelated tests or Lance data handles.
            let captured_directories = || {
                std::fs::read_dir("/proc/self/fd").unwrap().filter_map(Result::ok)
                    .filter(|entry| {
                        std::fs::read_link(entry.path()).is_ok_and(|path| {
                            (path.starts_with(project.root()) || path.starts_with(installation.root()))
                                && std::fs::metadata(entry.path()).is_ok_and(|metadata| metadata.is_dir())
                        })
                    }).count()
            };
            let baseline = captured_directories();
            let (pending, receiver) = tokio::sync::mpsc::unbounded_channel();
            let raw = std::str::from_utf8(runtime.secret(observe.snapshot()).unwrap()).unwrap();
            let semantic_limits = crate::knowledge_semantic::KnowledgeSemanticIndexLimits {
                read: limits,
                write: graphit_data::knowledge_index::KnowledgeIndexWriteLimits {
                    content: limits.content,
                    native: native_limits,
                    artifacts: policy.artifacts(),
                },
            };
            let idle = helper.embed_pending_loop(
                &project, &selected, &live, &resources, raw, semantic_limits, receiver,
            );
            tokio::pin!(idle);
            assert!(futures_util::poll!(idle.as_mut()).is_pending());
            assert_eq!(captured_directories(), baseline, "no project capture before the first FILE");
            pending.send("actual-a".to_owned()).unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    assert!(futures_util::poll!(idle.as_mut()).is_pending(), "open stream remains live");
                    if captured_directories() == baseline { break; }
                    tokio::task::yield_now().await;
                }
            }).await.expect("completed FILE releases its admissions before waiting for another FILE");
            drop(pending);
            let status = idle.await.unwrap();
            assert!(matches!(status, KnowledgeSemanticStatus::Published(value)
                if matches!(*value, KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Unchanged))));
            let KnowledgeEmbeddingRead::Published(reopened) = capture_knowledge_embedding_corpus(observe.snapshot(), &read, limits).await.unwrap()
                else { panic!("idle admission release preserves the published corpus") };
            assert_eq!(reopened.source_generation(), &document_generation);
            assert_eq!(reopened.semantic_generation().as_ref(), Some(&semantic_generation));
        }

        // Publication is complete before the real foreground starts consuming
        // commands. Its run loop owns the LocalModel receiver and child cleanup.
        if local_rerank {
            let owner = pending_owner.take().unwrap();
            let signal = run_stop.clone();
            running = Some(tokio::spawn(async move { owner.run(&signal).await }));
        }

        // Exercise the actual CLI codec and one Source MCP session so the opaque
        // cursor binding is identical across all six precedence combinations.
        let address = crate::source_entry::address(&installation, native_limits, &observe).await.unwrap();
        let connection = crate::ast_source::connect_source(&installation, address, &http, native_limits, &observe, ()).await.unwrap();
        let arguments = |rerank| crate::knowledge_search_entry::Arguments {
            query:"orbital".into(), mode:crate::knowledge_search_entry::Mode::Fts,
            metric:None, top_k:Some(3), page_size:Some(2), preview:true, rerank,
            cursor:None, cursor_file:None, project_dir:Some(project.root().to_owned()),
            artifact_id:None, artifact_version:None, context_alias:None,
        };
        let prepare = |input: &crate::knowledge_search_entry::Arguments| {
            let mut call = crate::knowledge_search_entry::prepare(input, None, &observe).unwrap();
            crate::mcp_output::json_call(&mut call);
            call
        };
        let baseline = connection.peer().call_tool(prepare(&arguments(Some(false)))).await.expect("Knowledge Source baseline MCP search with rerank=false");
        assert_ne!(baseline.is_error, Some(true), "{baseline:?}");
        let baseline = baseline.structured_content.unwrap();
        assert_eq!(baseline["native"]["total"], 3);
        assert_eq!(baseline["native"]["hits"].as_array().unwrap().len(), 2);
        assert!(baseline["native"]["next_cursor"].is_object());
        let mut expected_documents = Vec::new();
        let cases = if local_rerank {
            vec![(false, vec![(None,false),(Some(true),true),(Some(false),false)])]
        } else {
            vec![
                (false, vec![(None,false),(Some(false),false),(Some(true),true)]),
                (true, vec![(None,true),(Some(false),false),(Some(true),true)]),
            ]
        };
        for (configured, cases) in cases {
            ConfigStore::global(&installation).unwrap().set(ConfigKey::new("search.rerank").unwrap(), json!(configured), Duration::from_secs(5), &live).unwrap();
            for (flag, enabled) in cases {
                let started = Instant::now();
                let result = connection.peer().call_tool(prepare(&arguments(flag))).await.unwrap_or_else(|error| panic!(
                    "Knowledge Source MCP search: configured={configured} input={flag:?} effective={enabled} elapsed={:?}: {error:?}",
                    started.elapsed(),
                ));
                assert_ne!(result.is_error, Some(true), "{result:?}");
                let actual = result.structured_content.unwrap();
                let mut expected = baseline.clone();
                if enabled && local_rerank {
                    let mut actual = actual.clone();
                    let hits = actual["native"]["hits"].as_array_mut().unwrap();
                    let scores: Vec<_> = hits.iter().map(|hit| hit["rerank_score"].as_f64().filter(|score| score.is_finite()).expect("real BGE scalar score")).collect();
                    assert!(scores.windows(2).all(|scores| scores[0] >= scores[1]));
                    for hit in hits.iter_mut() { hit.as_object_mut().unwrap().remove("rerank_score"); }
                    hits.sort_by_key(|hit| hit["focused_source"]["slug"].as_str().unwrap().to_owned());
                    expected["native"]["hits"].as_array_mut().unwrap().sort_by_key(|hit| hit["focused_source"]["slug"].as_str().unwrap().to_owned());
                    assert_eq!(actual, expected, "Local rerank must retain the exact candidate page and all retrieval fields");
                    continue;
                }
                if enabled {
                    let hits = expected["native"]["hits"].as_array_mut().unwrap();
                    hits[0]["rerank_score"] = json!(0.1);
                    hits[1]["rerank_score"] = json!(0.9);
                    hits.reverse();
                    let slugs: Vec<_> = baseline["native"]["hits"].as_array().unwrap().iter()
                        .map(|hit| hit["focused_source"]["slug"].as_str().unwrap()).collect();
                    let selected = old_pin.read_selected_documents(observe.snapshot(), &read, &slugs).await.unwrap();
                    expected_documents.push(crate::foreground_knowledge::search::rerank_texts(&selected, &document_generation, &slugs).unwrap().into_iter().map(str::to_owned).collect::<Vec<_>>());
                }
                assert_eq!(actual, expected, "config={configured} input={flag:?}");
            }
        }
        let mut empty_arguments = arguments(Some(true));
        empty_arguments.query = "absentzzzz".into();
        let empty = connection.peer().call_tool(prepare(&empty_arguments)).await.unwrap();
        assert_ne!(empty.is_error, Some(true), "{empty:?}");
        let empty = empty.structured_content.unwrap();
        assert_eq!(empty["native"]["total"], 0);
        assert!(empty["native"]["hits"].as_array().unwrap().is_empty());
        connection.cancel().await.unwrap();

        std::fs::write(&document, "---\nid: actual-a\n---\n# A\nChanged body after model success\n").unwrap();
        let error = helper.index(&project, &selected, true, false, &BTreeSet::new(), &live, &live, &live, Some(&resources)).await.unwrap_err();
        let known = error.get_ref().and_then(|cause| cause.downcast_ref::<KnowledgeAfterPublication>()).expect("known document receipt must survive Models failure");
        let KnowledgeDesiredReconciliationResult::Existing(KnowledgeDocumentUpdateResult::Updated { generation, recovery_key }) = known.documents.as_ref()
            else { panic!("real document update and recovery key required") };
        assert_eq!(generation.version, document_generation.version + 1);
        assert!(!recovery_key.transaction_id().is_empty());
        let semantic_io = known.source().unwrap().downcast_ref::<io::Error>().unwrap();
        let Some(KnowledgeSemanticError::Inference(cause)) = semantic_io.get_ref().and_then(|cause| cause.downcast_ref::<KnowledgeSemanticError>()) else { panic!("Direct error cause must survive the publication receipt"); };
        assert!(matches!(cause.downcast_ref::<crate::embedding_service::EmbeddingServiceError>(), Some(crate::embedding_service::EmbeddingServiceError::Direct(graphit_ai::direct_embedding::DirectEmbeddingError::Embedding { source: graphit_ai::embedding::EmbeddingError::Status(status), .. })) if status.as_u16()==503));
        let KnowledgeDateRead::Published { generation: actual, .. } = read_native_knowledge_dates(observe.snapshot(), &read, limits).await.unwrap()
            else { panic!("known document effect cannot become absence") };
        assert_eq!(&actual, generation);
        let KnowledgeEmbeddingRead::Published(after_failure) = capture_knowledge_embedding_corpus(observe.snapshot(), &read, limits).await.unwrap()
            else { panic!("semantic failure must preserve document corpus") };
        assert_eq!(after_failure.source_generation(), generation);
        let retained = after_failure.stored_inputs().collect::<Vec<_>>();
        assert_eq!(
            retained.iter().map(|(input, _)| input.slug).collect::<BTreeSet<_>>(),
            BTreeSet::from(["actual-b", "actual-c"]),
            "a failed refresh must exclude the stale vector and keep unaffected vectors"
        );
        for (input, identity) in retained {
            let before = old_pin.read_reusable_embedding_batch(observe.snapshot(), &read, identity, std::slice::from_ref(&input)).await.unwrap();
            let after = after_failure.read_reusable_embedding_batch(observe.snapshot(), &read, identity, std::slice::from_ref(&input)).await.unwrap();
            assert_eq!(before.num_rows(), 1);
            assert_eq!(before, after);
        }
        assert_eq!(old_pin.source_generation(), &document_generation);

        let actual = if !local_rerank {
            let c_before = after_failure.read_selected_documents(observe.snapshot(), &read, &["actual-c"]).await.unwrap();
            let delta_path = project.root().join("docs/b.md");
            std::fs::write(&delta_path, "---\nid: actual-b\n---\n# B\nA new delta while A still needs inference\n").unwrap();
            let recovered = helper.index(&project, &selected, false, true, &BTreeSet::from([delta_path]),
                &live, &live, &live, Some(&resources)).await.unwrap().unwrap();
            assert!(matches!(recovered.documents, KnowledgeDesiredReconciliationResult::Existing(KnowledgeDocumentUpdateResult::Updated { .. })));
            let metadata = resources.metadata(&observe).await.unwrap();
            assert_eq!(metadata.revision, "weights-r1");
            let KnowledgeEmbeddingRead::Published(current) = capture_knowledge_embedding_corpus(observe.snapshot(), &read, limits).await.unwrap()
                else { panic!("recovery preserves actual Source"); };
            let slugs = current.stored_inputs().map(|(input, identity)| {
                assert_eq!(identity.vector_id(), metadata.vector_id, "resuming pending files keeps the configured model space");
                input.slug.to_owned()
            }).collect::<BTreeSet<_>>();
            assert_eq!(slugs, BTreeSet::from(["actual-a".into(), "actual-b".into(), "actual-c".into()]));
            let c_after = current.read_selected_documents(observe.snapshot(), &read, &["actual-c"]).await.unwrap();
            assert_eq!(c_after.state, c_before.state);
            assert_eq!(c_after.documents, c_before.documents,
                "another FILE's delta must not reparse or rewrite intact C");
            assert_eq!(c_after.generation, *current.source_generation());
            assert_ne!(current.semantic_generation(), after_failure.semantic_generation());
            current.source_generation().clone()
        } else { actual };

        let queued_cancel = Cancellation::default();
        queued_cancel.cancel();
        let queued_paths = BTreeSet::new();
        assert_eq!(helper.index(&project, &selected, true, false, &queued_paths, &queued_cancel, &queued_cancel, &queued_cancel, Some(&resources)).await.unwrap_err().kind(), io::ErrorKind::Interrupted);
        let KnowledgeDateRead::Published { generation: unchanged, .. } = read_native_knowledge_dates(observe.snapshot(), &read, limits).await.unwrap()
            else { panic!("cancelled indexing cannot erase documents") };
        assert_eq!(unchanged, actual);
        expected_documents
    }).catch_unwind().await;
    run_stop.cancel();
    let mut run_finished = match running {
        Some(running) => Some(running.await),
        None => None,
    };
    let retained_finished = if let Some(Ok(Err(failure))) = &mut run_finished {
        match failure.owner.take() {
            Some(owner) => Some(owner.shutdown().await),
            None => None,
        }
    } else {
        None
    };
    let owner_finished = match pending_owner {
        Some(owner) => Some(owner.shutdown().await),
        None => None,
    };
    let source_finished = match serving {
        Some(serving) => Some(serving.await),
        None => None,
    };
    let expected_documents = match result {
        Ok(documents) => documents,
        Err(panic) => {
            eprintln!(
                "cleanup after fixture failure: foreground_owner_ok={} runner_ok={} source_ok={} retained_owner_ok={}",
                owner_finished.as_ref().is_none_or(Result::is_ok),
                run_finished
                    .as_ref()
                    .is_none_or(|result| matches!(result, Ok(Ok(())))),
                source_finished
                    .as_ref()
                    .is_none_or(|result| matches!(result, Ok(Ok(())))),
                retained_finished.as_ref().is_none_or(Result::is_ok),
            );
            http_fixture.server.abort();
            let _ = http_fixture.server.await;
            std::panic::resume_unwind(panic);
        }
    };
    if let Some(result) = retained_finished {
        result.unwrap();
    }
    if let Some(result) = owner_finished {
        result.unwrap();
    }
    if let Some(result) = source_finished {
        result.unwrap().unwrap();
    }
    if let Some(result) = run_finished {
        result.unwrap().unwrap();
    }
    let requests = http_fixture.server.await.unwrap();
    assert_eq!(
        requests.len(),
        if local_rerank { 4 } else { 9 },
        "disabled, repeat and cancellation must send no inference"
    );
    assert!(
        requests[..3]
            .iter()
            .all(|request| request.body["input"].as_array().unwrap().len() == 1)
    );
    assert_eq!(
        requests.last().unwrap().body["input"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    if !local_rerank {
        assert!(
            requests[7..]
                .iter()
                .all(|request| request.body["input"].as_array().unwrap().len() == 1)
        );
        let resumed = requests[7..]
            .iter()
            .map(|request| request.body["input"][0].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(
            resumed
                .iter()
                .any(|text| text.contains("Changed body after model success"))
        );
        assert!(
            resumed
                .iter()
                .any(|text| text.contains("A new delta while A still needs inference"))
        );
        assert!(
            resumed
                .iter()
                .all(|text| !text.contains("orbital literal γ")),
            "intact C must reuse its existing vector without inference"
        );
    }
    for (request, expected) in requests[3..if local_rerank { 3 } else { 6 }]
        .iter()
        .zip(expected_documents)
    {
        assert!(
            request
                .head
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-rerank-only")
        );
        assert_eq!(request.body["query"], "orbital");
        assert_eq!(request.body["documents"].as_array().unwrap().len(), 2);
        assert_eq!(
            request.body["documents"],
            json!(expected),
            "ordered full bodies from the retained page pin"
        );
    }
}
