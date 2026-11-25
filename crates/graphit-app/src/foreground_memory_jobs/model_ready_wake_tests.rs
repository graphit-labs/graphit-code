//! Exercise the Foreground M-ready handoff to existing Knowledge-only supervisors.
use super::*;
use graphit_daemon::supervisor::{DesiredProject, ProjectContext, ProjectSupervisor};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires GRAPHIT_TEST_MODEL_SERVER and the pinned GRAPHIT_EMBEDDINGGEMMA2_MODEL BF16 GGUF"]
async fn model_ready_rescans_existing_knowledge_only_supervisors() {
    let fixture = Fixture::new();
    let _bundle = native_gguf_source::install_gguf(&fixture);
    let cancellation = Cancellation::default();
    let global = ConfigStore::global(&fixture.installation).unwrap();
    for (module, enabled) in [
        (Module::Memory, false),
        (Module::Ast, false),
        (Module::Knowledge, true),
        (Module::Embedding, true),
    ] {
        global
            .set(
                module.key(),
                json!(enabled),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
    }
    let config = ConfigSnapshot::new(fixture.config.brand(), [global.load().unwrap()]).unwrap();
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let provider = active_provider(&fixture.installation, &config, &original)
        .unwrap()
        .unwrap();
    let mut owner = ForegroundSource::start_with_inventory(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &config,
        NativeBinding::KiroV3,
        &original,
        Some(&provider),
    )
    .unwrap();

    // Capture all configuration before starting the real watchers. No file or
    // config is modified while M starts, so a filesystem event cannot mask the
    // missing explicit wake that this regression used to cause.
    let projects = Arc::clone(owner.projects.as_ref().unwrap());
    let contexts: Vec<_> = projects
        .iter()
        .map(|(_, id, version)| ProjectContext {
            id: *id,
            version: version.clone(),
        })
        .collect();
    let watches = projects
        .iter()
        .map(|(paths, _, _)| project_watch_config(paths, &config, &original))
        .collect::<io::Result<Vec<_>>>()
        .unwrap();
    let expected: Vec<_> = contexts.iter().map(ProjectContext::key).collect();
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    let observation: io::Result<Vec<_>> = async {
        owner
            .supervisors
            .reconcile(
                contexts
                    .iter()
                    .cloned()
                    .map(|context| DesiredProject {
                        context,
                        active: true,
                    })
                    .collect(),
                |context| {
                    let watch = contexts
                        .iter()
                        .position(|candidate| candidate.key() == context.key())
                        .map(|index| watches[index].clone());
                    let send = send.clone();
                    async move {
                        let watch = watch.ok_or(io::ErrorKind::NotFound)?;
                        ProjectSupervisor::start(context, watch, move |context, batch, _cancel| {
                            let send = send.clone();
                            async move {
                                send.send((context.key(), batch.rescan))
                                    .map_err(io::Error::other)
                            }
                        })
                        .await
                    }
                },
            )
            .await?;
        let mut initial = Vec::new();
        for _ in &expected {
            let event = tokio::time::timeout(Duration::from_secs(10), receive.recv())
                .await
                .map_err(io::Error::other)?
                .ok_or(io::ErrorKind::BrokenPipe)?;
            initial.push(event);
        }
        if initial
            .iter()
            .any(|(key, rescan)| !rescan || !expected.contains(key))
        {
            return Err(io::Error::other("unexpected initial supervisor event"));
        }
        if owner.server.as_ref().unwrap().semantic_model()?.is_some() {
            return Err(io::Error::other("fixture already has M before attach_jobs"));
        }
        owner
            .attach_jobs(
                &fixture.installation,
                &config,
                NativeBinding::KiroV3,
                &fixture.environment(),
                &original,
            )
            .await?;
        if owner.server.as_ref().unwrap().semantic_model()?.is_none() {
            return Err(io::Error::other(
                "M was not attached by the real Foreground owner",
            ));
        }
        let mut wakes = Vec::new();
        for _ in &expected {
            let event = tokio::time::timeout(Duration::from_secs(10), receive.recv())
                .await
                .map_err(io::Error::other)?
                .ok_or(io::ErrorKind::BrokenPipe)?;
            wakes.push(event);
        }
        Ok(wakes)
    }
    .await;
    let cleanup = owner.shutdown().await;
    let wakes = observation.expect("Knowledge-only supervisors must receive the M-ready rescan");
    cleanup.unwrap();
    assert_eq!(wakes.len(), expected.len());
    for key in expected {
        assert_eq!(
            wakes
                .iter()
                .filter(|(actual, rescan)| actual == &key && *rescan)
                .count(),
            1
        );
    }
}
