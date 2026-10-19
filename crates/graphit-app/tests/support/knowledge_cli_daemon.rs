//! Productive daemon composition for compiled Knowledge CLI journeys.
use super::fixture_support::Fixture;
use graphit_app::foreground_source::{ForegroundFailure, ForegroundSource};
use graphit_core::{
    config::{ConfigSnapshot, ConfigStore},
    file_lock::Cancellation,
    runtime_key::RuntimeKeyMutationOutcome,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_daemon::project_bootstrap::ProjectBootstrapInput;
use graphit_hub::{NativeBinding, policy::RequestContext};
use std::{ffi::OsString, sync::Arc};

pub struct CliDaemon {
    service: Arc<Cancellation>,
    served: Option<tokio::task::JoinHandle<(Result<(), ForegroundFailure>, tempfile::TempDir)>>,
}
pub struct SeedCleanupFailure {
    outcome: RuntimeKeyMutationOutcome,
    _auth: Arc<DaemonAuth>,
    _temp: tempfile::TempDir,
}
impl std::fmt::Debug for SeedCleanupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SeedCleanupFailure")
            .field("failure", &self.outcome.failure())
            .field("cleanup_failure", &self.outcome.cleanup_failure())
            .finish_non_exhaustive()
    }
}
pub struct DaemonCleanupFailure {
    pub failure: ForegroundFailure,
    _temp: tempfile::TempDir,
}
impl std::fmt::Debug for DaemonCleanupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.failure.fmt(f)
    }
}
impl Drop for CliDaemon {
    fn drop(&mut self) {
        // Cancellation request only; the running task retains the installation.
        self.service.cancel();
    }
}
impl CliDaemon {
    pub async fn start(
        fixture: Fixture,
        binding: NativeBinding,
        extra: Vec<(OsString, OsString)>,
        original: &RequestContext<'_>,
    ) -> Result<Self, Box<SeedCleanupFailure>> {
        let mut environment = vec![("PATH".into(), fixture.temp.path().as_os_str().to_owned())];
        environment.extend(extra);
        let selected_cli = graphit_core::config::ConfigKey::new("ai.cli").unwrap();
        let captured = ConfigSnapshot::capture_environment(
            fixture.paths.brand(),
            &[selected_cli],
            environment.clone(),
        )
        .unwrap();
        let config = ConfigSnapshot::new(
            fixture.paths.brand(),
            [
                ConfigStore::global(&fixture.paths).unwrap().load().unwrap(),
                captured,
            ],
        )
        .unwrap();
        fixture.server.shutdown().await.unwrap();
        let Fixture {
            temp,
            server,
            raw,
            auth,
            paths,
            projects,
            stop,
        } = fixture;
        drop(server);
        drop(stop);
        let outcome = auth.remove_runtime(original.snapshot());
        if outcome.failure().is_some() || outcome.cleanup_failure().is_some() {
            return Err(Box::new(SeedCleanupFailure {
                outcome,
                _auth: auth,
                _temp: temp,
            }));
        }
        drop(outcome);
        drop(auth);
        drop(raw);
        let (project, _, version) = &projects[0];
        let owner = ForegroundSource::start_and_publish(
            &paths,
            &[ProjectBootstrapInput {
                project,
                explicit_version: version.version(),
                pin: None,
                detached_base: "",
            }],
            0,
            &config,
            binding,
            environment,
            original,
        )
        .await
        .unwrap();
        let service = Arc::new(Cancellation::default());
        let running = Arc::clone(&service);
        let served = tokio::spawn(async move {
            let result = owner.run(&running).await;
            (result, temp)
        });
        Ok(Self {
            service,
            served: Some(served),
        })
    }

    pub async fn stop(&mut self) -> Result<(), Box<DaemonCleanupFailure>> {
        self.service.cancel();
        // Borrow the handle so cancellation of this wait preserves its output.
        let (result, temp) = self
            .served
            .as_mut()
            .expect("daemon stop observed once")
            .await
            .unwrap();
        self.served.take();
        match result {
            Ok(()) => Ok(()),
            Err(failure) => Err(Box::new(DaemonCleanupFailure {
                failure,
                _temp: temp,
            })),
        }
    }
}
