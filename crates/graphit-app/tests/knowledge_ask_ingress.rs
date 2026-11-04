#![cfg(target_os = "linux")]
//! Current compiled CLI and real Source/Auth/FULL16/Process; no vendor login claim.
use graphit_app::{
    native_install_http::NativeInstallPolicy,
    source_server::{SourcePolicy, SourceServer},
};
#[path = "support/knowledge_cli_daemon.rs"]
mod cli_daemon;
#[path = "support/knowledge_ask_server.rs"]
mod fixture_support;
use graphit_auth::{
    Permission, ProjectCapture, ProjectRecordAuthority, ProjectWriteOperation, ScopeCapture,
    ScopeInput,
};
use graphit_core::{
    artifact_store::{ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeLimits},
    brand::Brand,
    file_lock::Cancellation,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
};
use graphit_data::{knowledge_document_projection as h, knowledge_index::*};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use std::{
    io,
    num::NonZeroUsize,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn n(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn original(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 1024 * 1024,
            max_output_bytes: 8 * 1024 * 1024,
            max_depth: 64,
            max_nodes: 65536,
            max_scalar_bytes: 1024 * 1024,
            max_collection_entries: 65536,
        },
        cancel,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap()
}
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
async fn publish(f: &fixture_support::Fixture, ctx: &RequestContext<'_>) {
    let (paths, id, version) = &f.projects[0];
    let live = CapturedPublicationScope::project(
        paths,
        *id,
        ScopeLimits::new(16, 4096, 256).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let record = live.binding().unwrap();
    let caller = f.auth.issuer().authenticate_runtime(&f.raw, ctx).unwrap();
    let authority = ProjectRecordAuthority::new(f.auth.issuer());
    let admission = authority
        .admit_authenticated_write(
            &caller,
            ctx,
            ScopeInput {
                installation: f.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &live,
                        record: &record,
                    },
                    version,
                }),
                providers: &[],
                binding: NativeBinding::Codex,
                global_version: "",
            },
            ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    let pages = [h::AuthoredPageFields {
        slug: Some("Paris"),
        title: Some("Capital β"),
        body: Some("Paris é a capital.\n"),
        kind: Some("page"),
        summary: Some("literal"),
        description: None,
        summary_truncated: false,
        source_path: "docs/fixture.md",
        physical_digest: None,
        compilation_digest: h::CompilationDigest32::from_bytes([2; 32]),
        recipe: h::RecipeFields {
            id: h::AUTHORED_RECIPE_ID,
            parameters: Some(h::AuthoredParameters {
                fallback_title: "fallback",
                kind_on_absent: "page",
                max_summary_bytes: n(16),
            }),
        },
        updated_at: None,
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: &[],
    }];
    let rows = h::prepare(
        ctx.snapshot(),
        &pages,
        &[],
        h::KnowledgeDocumentProjectionLimits {
            max_documents: n(8),
            max_observations: n(8),
            max_rows: n(64),
            max_field_bytes: n(65536),
            max_text_bytes: n(131072),
        },
    )
    .unwrap();
    let copy = CopyLimits::new(1024 * 1024, 1024, 65536, 16).unwrap();
    publish_native_authored(
        ctx.snapshot(),
        &admission,
        &rows,
        KnowledgeIndexWriteLimits {
            content: KnowledgeIndexContentLimits {
                max_identifier_bytes: n(256),
            },
            native: native(),
            artifacts: ArtifactLimits::new(65536, 4096, copy).unwrap(),
        },
    )
    .await
    .unwrap();
}
fn cli_command(f: &fixture_support::Fixture) -> impl Fn() -> Command + use<> {
    let checkout = f.projects[0].0.root().to_owned();
    let home = f.temp.path().to_owned();
    let global_dir = f.paths.brand().environment_variable("GLOBAL_DIR").unwrap();
    let installation = f.paths.root().to_owned();
    move || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .current_dir(&checkout)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &home)
            .env(&global_dir, &installation);
        command
    }
}
fn run(test: impl std::future::Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap()
                .block_on(test)
        })
        .unwrap()
        .join()
        .unwrap();
}
#[test]
fn compiled_cli_preserves_query_model_and_startup_capture_and_writes_only_final_answer() {
    run(async {
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap();
        let f = fixture_support::fixture(
            &[
                Permission::Read(Module::Ast),
                Permission::Read(Module::Knowledge),
                Permission::MutateModule(Module::Knowledge),
            ],
            brand,
            NativeBinding::Codex,
        );
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        let program = f.temp.path().join("codex");
        std::fs::write(&program,r#"#!/usr/bin/python3
import json,sys,os
assert sys.argv[1:4] == ['exec','--json','--model']
assert sys.argv[4] in ['literal model;$(native)', 'retrieval model;$(native)']
assert sys.argv[5:] == ['-']
assert os.environ['CAPTURED'] == 'startup only;$(literal)'
assert os.path.basename(os.getcwd()) == 'project'
prompt=json.load(sys.stdin)
explicit = sys.argv[4] == 'literal model;$(native)'
assert prompt['question'] == ('  literal β\n  ' if explicit else 'Paris')
assert prompt['pages'][0]['slug'] == 'Paris'
print('{"type":"turn.started"}',flush=True)
print('{"type":"item.completed","item":{"id":"i","type":"reasoning","text":"progress never goes to CLI stdout"}}',flush=True)
print(json.dumps({"type":"item.completed","item":{"id":"i","type":"agent_message","text":"actual β [[Paris]]" if explicit else "retrieved β [[Paris]]"}}),flush=True)
print('{"type":"turn.completed","usage":{"input_tokens":8,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0,"cache_write_input_tokens":0}}',flush=True)
"#).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        // The compiled PROJECT command maintains its checkout through the real
        // Foreground route before it calls Ask.
        let command = cli_command(&f);
        let mut daemon = cli_daemon::CliDaemon::start(
            f,
            NativeBinding::Codex,
            vec![("CAPTURED".into(), "startup only;$(literal)".into())],
            &ctx,
        )
        .await
        .unwrap();
        // Caller PATH/environment must not override the captured process binding.
        let mut cmd = command();
        cmd.env("PATH", "/no-codex-in-client-path")
            .env("CAPTURED", "client differs")
            .args([
                "knowledge",
                "ask",
                "--query",
                "  literal β\n  ",
                "--slug",
                "Paris",
                "--model",
                "literal model;$(native)",
            ]);
        let result = tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap();
        assert!(
            result.status.success(),
            "compiled Ask failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, "actual β [[Paris]]\n".as_bytes());
        assert!(result.stderr.is_empty());
        for mode in ["fts", "hybrid"] {
            let mut automatic = command();
            automatic
                .env("PATH", "/no-codex-in-client-path")
                .env("CAPTURED", "client differs")
                .args([
                    "knowledge",
                    "ask",
                    "--query",
                    "Paris",
                    "--retrieval",
                    mode,
                    "--top-k",
                    "1",
                    "--model",
                    "retrieval model;$(native)",
                ]);
            let automatic = tokio::task::spawn_blocking(move || automatic.output().unwrap())
                .await
                .unwrap();
            assert!(
                automatic.status.success(),
                "{}",
                String::from_utf8_lossy(&automatic.stderr)
            );
            assert_eq!(automatic.stdout, "retrieved β [[Paris]]\n".as_bytes());
            assert!(automatic.stderr.is_empty());
        }
        for flags in [
            vec!["--retrieval", "fts"],
            vec!["--retrieval", "fts", "--top-k", "0"],
            vec!["--retrieval", "fts", "--top-k", "1", "--slug", "Paris"],
            vec!["--retrieval", "fts", "--top-k", "1", "--metric", "dot"],
            vec!["--slug", "Paris", "--top-k", "1"],
        ] {
            let mut invalid = command();
            invalid
                .args([
                    "knowledge",
                    "ask",
                    "--query",
                    "Paris",
                    "--model",
                    "literal model;$(native)",
                ])
                .args(flags);
            let invalid = tokio::task::spawn_blocking(move || invalid.output().unwrap())
                .await
                .unwrap();
            assert!(!invalid.status.success());
            assert!(invalid.stdout.is_empty());
        }
        let mut invalid = command();
        invalid.args([
            "knowledge",
            "ask",
            "--query",
            "q",
            "--slug",
            "missing",
            "--model",
            "m",
        ]);
        let invalid = tokio::task::spawn_blocking(move || invalid.output().unwrap())
            .await
            .unwrap();
        assert!(!invalid.status.success());
        assert!(invalid.stdout.is_empty());
        assert!(
            String::from_utf8(invalid.stderr)
                .unwrap()
                .contains("page selection")
        );
        daemon.stop().await.unwrap();
    });
}

#[test]
fn missing_optional_codex_keeps_source_and_compiled_ask_has_actionable_error() {
    run(async {
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap();
        let f = fixture_support::fixture(
            &[
                Permission::Read(Module::Ast),
                Permission::Read(Module::Knowledge),
            ],
            brand,
            NativeBinding::Codex,
        );
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        let command = cli_command(&f);
        // CliDaemon captures PATH at this fixture's fresh directory, which
        // contains no completion executable even if the host has Codex.
        let mut daemon = cli_daemon::CliDaemon::start(f, NativeBinding::Codex, Vec::new(), &ctx)
            .await
            .unwrap();
        let mut cmd = command();
        cmd.args([
            "knowledge",
            "ask",
            "--query",
            "q",
            "--slug",
            "Paris",
            "--model",
            "m",
        ]);
        let output = tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(
            text.contains("Knowledge Ask native CLI unavailable:"),
            "{text}"
        );
        assert!(
            text.contains("install the configured CLI in the startup PATH and restart the service"),
            "{text}"
        );
        let mut source = command();
        source.args(["ast", "source", "raw"]).stdin(Stdio::null());
        let output = tokio::task::spawn_blocking(move || source.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "Source CLI native error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"actual native payload");
        daemon.stop().await.unwrap();
    });
}

#[test]
fn compiled_cli_kiro_v3_preserves_captured_inputs_and_waits_for_terminal() {
    run(async {
        use tokio::io::AsyncReadExt;

        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap();
        let f = fixture_support::fixture(
            &[
                Permission::Read(Module::Ast),
                Permission::Read(Module::Knowledge),
                Permission::MutateModule(Module::Knowledge),
            ],
            brand,
            NativeBinding::KiroV3,
        );
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        std::os::unix::fs::symlink(
            env!("CARGO_BIN_EXE_kiro-knowledge-fixture"),
            f.temp.path().join("kiro-cli"),
        )
        .unwrap();
        let journal = f.temp.path().join("kiro-cli-journal");
        let query = "  literal β\n  ";
        let model = "literal model;$(native)";
        let mut cmd = tokio::process::Command::from(cli_command(&f)());
        let mut daemon = cli_daemon::CliDaemon::start(
            f,
            NativeBinding::KiroV3,
            vec![
                ("KIRO_CAPTURED".into(), "literal-captured".into()),
                ("KIRO_FIXTURE_MODEL".into(), model.into()),
                ("KIRO_FIXTURE_MODE".into(), "stream_gate".into()),
                ("KIRO_FIXTURE_EXPECTED_QUESTION".into(), query.into()),
                (
                    "KIRO_FIXTURE_JOURNAL".into(),
                    journal.clone().into_os_string(),
                ),
            ],
            &ctx,
        )
        .await
        .unwrap();
        cmd.env("PATH", "/no-kiro-in-client-path")
            .env("KIRO_CAPTURED", "different client environment")
            .env("KIRO_FIXTURE_MODEL", "different client model")
            .env(
                "KIRO_FIXTURE_EXPECTED_QUESTION",
                "different client question",
            )
            .args([
                "knowledge",
                "ask",
                "--query",
                query,
                "--slug",
                "Paris",
                "--model",
                model,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let events = match std::fs::read_to_string(&journal) {
                    Ok(events) => events,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
                    Err(error) => panic!("cannot read peer journal: {error}"),
                };
                if events.lines().any(|event| event == "progress_sent") {
                    break;
                }
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "compiled CLI exited before the peer queued progress"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let before = std::fs::read_to_string(&journal).unwrap();
        for required in [
            "engine_v3",
            "initialize",
            "literal_inputs_checked",
            "new",
            "select",
            "prompt",
            "expected_question_checked",
            "authorized_render_checked",
            "progress_sent",
        ] {
            assert!(before.lines().any(|event| event == required), "{required}");
        }
        assert!(!before.lines().any(|event| event == "terminal"));
        let peer_pid: u32 = before
            .lines()
            .find_map(|event| event.strip_prefix("peer_pid:"))
            .expect("stream-gated peer must report its actual PID")
            .parse()
            .unwrap();
        assert_ne!(peer_pid, 0);
        let mut stdout = child.stdout.take().unwrap();
        let mut first_byte = [0; 1];
        assert!(
            tokio::time::timeout(Duration::from_millis(100), stdout.read(&mut first_byte))
                .await
                .is_err(),
            "CLI must keep stdout open and empty until the native terminal"
        );
        child.stdout = Some(stdout);
        std::fs::write(journal.with_extension("finish"), b"finish").unwrap();
        let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
            .await
            .unwrap()
            .unwrap();
        assert!(output.status.success(), "compiled Kiro V3 Ask must succeed");
        assert_eq!(output.stdout, b"fresh fixture answer\n");
        assert!(output.stderr.is_empty());
        let events = std::fs::read_to_string(&journal).unwrap();
        let events: Vec<_> = events.lines().collect();
        let position = |name| events.iter().position(|event| *event == name).unwrap();
        assert!(position("progress_sent") < position("terminal"));
        daemon.stop().await.unwrap();
        // Process cleanup can signal the peer before its SDK driver writes a footer.
        // The real daemon join must leave the observed native child absent.
        assert!(
            !std::path::Path::new("/proc")
                .join(peer_pid.to_string())
                .try_exists()
                .unwrap(),
            "native peer survived the compiled CLI and real daemon join"
        );
    });
}
