use super::*;

#[test]
fn compiler_deadline_requires_a_completed_process_and_its_own_expired_original() {
    let cancellation = Cancellation::default();
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        cancellation.clone(),
    );
    let live = SnapshotContext::new(
        Instant::now() + Duration::from_secs(5),
        cancellation.clone(),
    );
    for (reason, cleanup, eligible) in [
        (graphit_process::Reason::TimedOut, Cleanup::Complete, true),
        (
            graphit_process::Reason::TimedOut,
            Cleanup::NotSpawned,
            false,
        ),
        (graphit_process::Reason::TimedOut, Cleanup::TimedOut, false),
        (graphit_process::Reason::Cancelled, Cleanup::Complete, false),
        (graphit_process::Reason::Stopped, Cleanup::Complete, false),
    ] {
        let failure = graphit_process::Failure {
            reason,
            cleanup,
            status: None,
            pipes: graphit_process::PipeFinalization {
                stdout: graphit_process::PipeEnd::Eof,
                stderr: graphit_process::PipeEnd::Eof,
            },
            observed: graphit_process::Counts::default(),
            delivered: graphit_process::Counts::default(),
            transcript: None,
            remaining: None,
        };
        let error = io::Error::new(io::ErrorKind::TimedOut, failure);
        assert_eq!(
            completed_compiler_deadline(&error, &expired, false).is_some(),
            eligible,
            "{reason:?}/{cleanup:?}"
        );
        assert!(completed_compiler_deadline(&error, &expired, true).is_none());
        assert!(completed_compiler_deadline(&error, &live, false).is_none());
    }
    assert!(
        completed_compiler_deadline(&io::ErrorKind::TimedOut.into(), &expired, false).is_none()
    );
}

#[cfg(unix)]
mod process {
    use super::*;
    use graphit_core::brand::Brand;
    use std::os::unix::fs::PermissionsExt;

    struct Fixture {
        directory: tempfile::TempDir,
        installation: InstallationPaths,
        project: ProjectPaths,
        config: ConfigSnapshot,
        policy: HubResourcePolicy,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let docker = directory.path().join("docker");
            std::fs::write(
                &docker,
                r#"#!/bin/sh
case "$1" in
run)
  for arg in "$@"; do
    case "$arg" in
      'type=bind,"source='*',target=/output')
        output=${arg#type=bind,\"source=}; output=${output%\",target=/output}
        printf 'partial compiler output' > "$output/index.scip"
        printf '%s\n' "$output" > "$SCIP_TEST_DIR/output-path" ;;
    esac
  done
  printf 'compiler-private-stdout-canary\n'
  printf 'compiler-private-stderr-canary\n' >&2
  : > "$SCIP_TEST_DIR/started"
  trap 'exit 130' TERM INT
  while :; do /bin/sleep 0.05; done ;;
ps) printf '%s\n' aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ;;
rm)
  [ -f "$SCIP_TEST_DIR/fail-cleanup" ] && exit 24
  : > "$SCIP_TEST_DIR/removed" ;;
*) exit 91 ;;
esac
"#,
            )
            .unwrap();
            std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o700)).unwrap();
            let installation = InstallationPaths::resolve(
                Brand::default(),
                directory.path(),
                None,
                Some(&directory.path().join("installation")),
            )
            .unwrap();
            let project = ProjectPaths::at(directory.path(), &installation).unwrap();
            let key = ConfigKey::new("hub.limits.request.deadline_ms").unwrap();
            let config = ConfigSnapshot::new(
                installation.brand(),
                [ConfigSnapshot::capture_environment(
                    installation.brand(),
                    std::slice::from_ref(&key),
                    [(
                        key.environment_variable(installation.brand()).into(),
                        "500".into(),
                    )],
                )
                .unwrap()],
            )
            .unwrap();
            let policy = HubResourcePolicy::resolve(&config).unwrap();
            Self {
                directory,
                installation,
                project,
                config,
                policy,
            }
        }

        fn service(&self) -> ScipService {
            ScipService::new(
                vec![
                    ("PATH".into(), self.directory.path().as_os_str().to_owned()),
                    (
                        "SCIP_TEST_DIR".into(),
                        self.directory.path().as_os_str().to_owned(),
                    ),
                ],
                self.installation.brand(),
            )
            .unwrap()
        }

        async fn index(
            &self,
            service: &ScipService,
            cancellation: &Cancellation,
            guard: &impl Fn() -> io::Result<()>,
        ) -> io::Result<Option<ScipOutput>> {
            service
                .index(
                    ScipFamily::Rust,
                    &tests::context(&self.project, "main"),
                    &self.installation,
                    &self.config,
                    &self.policy,
                    cancellation,
                    guard,
                )
                .await
        }

        fn assert_partial_output_discarded(&self) {
            assert!(self.directory.path().join("started").exists());
            assert!(self.directory.path().join("removed").exists());
            let output =
                std::fs::read_to_string(self.directory.path().join("output-path")).unwrap();
            assert!(!Path::new(output.trim()).exists());
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn derived_compiler_timeout_permits_native_fallback_after_joined_cleanup() {
        let fixture = Fixture::new();
        let service = fixture.service();
        let cancellation = Cancellation::default();
        let parent = SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            cancellation.clone(),
        );
        let deadline = parent.required_deadline().unwrap();
        let result = fixture
            .index(&service, &cancellation, &|| parent.check())
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "the existing syntax producer receives unavailability"
        );
        parent.check().unwrap();
        assert_eq!(parent.required_deadline().unwrap(), deadline);
        fixture.assert_partial_output_discarded();
        let state = service.state.lock().await;
        assert!(state.container.is_none());
        assert!(state.remaining.is_none());
        drop(state);
        service.shutdown(&fixture.policy).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn expired_parent_remains_terminal_after_derived_compiler_timeout() {
        let fixture = Fixture::new();
        let service = fixture.service();
        let cancellation = Cancellation::default();
        let parent = SnapshotContext::new(
            Instant::now() + Duration::from_millis(250),
            cancellation.clone(),
        );
        let error = fixture
            .index(&service, &cancellation, &|| parent.check())
            .await
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        fixture.assert_partial_output_discarded();
        service.shutdown(&fixture.policy).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_parent_never_permits_compiler_fallback() {
        let fixture = Fixture::new();
        let service = fixture.service();
        let cancellation = Cancellation::default();
        let parent = SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            cancellation.clone(),
        );
        let stop = cancellation.clone();
        let marker = fixture.directory.path().join("started");
        let stopper = tokio::spawn(async move {
            let started = tokio::time::timeout(Duration::from_secs(5), async {
                while !marker.exists() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .is_ok();
            stop.cancel();
            started
        });
        let error = fixture
            .index(&service, &cancellation, &|| parent.check())
            .await
            .err()
            .unwrap();
        assert!(
            stopper.await.unwrap(),
            "the actual compiler process must start"
        );
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        fixture.assert_partial_output_discarded();
        service.shutdown(&fixture.policy).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn revoked_authority_remains_terminal_after_derived_compiler_timeout() {
        let fixture = Fixture::new();
        let service = fixture.service();
        let cancellation = Cancellation::default();
        let guard = || {
            if fixture.directory.path().join("started").exists() {
                Err(io::ErrorKind::PermissionDenied.into())
            } else {
                Ok(())
            }
        };
        let error = fixture
            .index(&service, &cancellation, &guard)
            .await
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        fixture.assert_partial_output_discarded();
        service.shutdown(&fixture.policy).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failed_container_cleanup_remains_terminal_and_retains_owners() {
        let fixture = Fixture::new();
        let service = fixture.service();
        std::fs::write(fixture.directory.path().join("fail-cleanup"), "").unwrap();
        let cancellation = Cancellation::default();
        let parent = SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            cancellation.clone(),
        );
        let error = fixture
            .index(&service, &cancellation, &|| parent.check())
            .await
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        let failure = error
            .get_ref()
            .unwrap()
            .downcast_ref::<CleanupFailure>()
            .unwrap();
        assert_eq!(
            failure.compiler.as_ref().unwrap().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(service.state.lock().await.container.is_some());
        assert!(!fixture.directory.path().join("removed").exists());
        let error = fixture
            .index(&service, &cancellation, &|| parent.check())
            .await
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        std::fs::remove_file(fixture.directory.path().join("fail-cleanup")).unwrap();
        service.shutdown(&fixture.policy).await.unwrap();
        fixture.assert_partial_output_discarded();
    }
}
