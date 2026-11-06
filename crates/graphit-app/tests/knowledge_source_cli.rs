#![cfg(target_os = "linux")]
//! Compiled CLI to real Source/Auth/Lance, without an embedding provider.
use crate::knowledge_ask_server as fixture_support;
use crate::knowledge_cli_daemon as cli_daemon;
use graphit_app::foreground_source::ForegroundSource;
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
use graphit_daemon::project_bootstrap::ProjectBootstrapInput;
use graphit_data::{knowledge_document_projection as h, knowledge_index::*};
use graphit_hub::{NativeBinding, policy::RequestContext};
use std::{
    num::NonZeroUsize,
    process::Command,
    time::{Duration, Instant},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_knowledge_source_reads_full_markdown_slices_and_missing_identity() {
    let f = fixture_support::fixture(
        &[
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
            Permission::Configure,
        ],
        Brand::default(),
        NativeBinding::KiroV3,
    );
    let config = graphit_core::config::ConfigSnapshot::new(f.paths.brand(), []).unwrap();
    let policy = graphit_hub::policy::HubResourcePolicy::resolve(&config).unwrap();
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        policy.content(),
        &cancel,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap();
    let body = format!(
        "# Whole CLI\n{}\nCLI-LAST-LINE\n",
        "Literal **Markdown** é.\n".repeat(100)
    );
    {
        let (paths, id, version) = &f.projects[0];
        let live = CapturedPublicationScope::project(
            paths,
            *id,
            ScopeLimits::new(16, 4096, 256).unwrap(),
            request.snapshot(),
        )
        .unwrap();
        let record = live.binding().unwrap();
        let caller = f
            .auth
            .issuer()
            .authenticate_runtime(&f.raw, &request)
            .unwrap();
        let authority = ProjectRecordAuthority::new(f.auth.issuer());
        let write = authority
            .admit_authenticated_write(
                &caller,
                &request,
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
                    binding: NativeBinding::KiroV3,
                    global_version: "",
                },
                ProjectWriteOperation::Knowledge,
            )
            .unwrap();
        let n = |v| NonZeroUsize::new(v).unwrap();
        let rows = h::prepare(
            request.snapshot(),
            &[h::AuthoredPageFields {
                slug: Some("alpha"),
                title: Some("CLI whole page"),
                body: Some(&body),
                kind: Some("page"),
                summary: Some("short"),
                description: None,
                summary_truncated: false,
                source_path: "docs/reader.md",
                physical_digest: None,
                compilation_digest: h::CompilationDigest32::from_bytes([3; 32]),
                recipe: h::RecipeFields {
                    id: h::AUTHORED_RECIPE_ID,
                    parameters: Some(h::AuthoredParameters {
                        fallback_title: "fallback",
                        kind_on_absent: "page",
                        max_summary_bytes: n(64),
                    }),
                },
                updated_at: None,
                stale_since: None,
                stale_after: None,
                resolved_wiki_targets: &[],
            }],
            &[],
            h::KnowledgeDocumentProjectionLimits {
                max_documents: n(4),
                max_observations: n(4),
                max_rows: n(16),
                max_field_bytes: n(65536),
                max_text_bytes: n(131072),
            },
        )
        .unwrap();
        let copy = CopyLimits::new(1048576, 1024, 65536, 16).unwrap();
        publish_native_authored(
            request.snapshot(),
            &write,
            &rows,
            KnowledgeIndexWriteLimits {
                content: KnowledgeIndexContentLimits {
                    max_identifier_bytes: n(4096),
                },
                native: NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
                artifacts: ArtifactLimits::new(65536, 4096, copy).unwrap(),
            },
        )
        .await
        .unwrap();
    }
    // The compiled PROJECT command maintains its checkout through the ordinary
    // Foreground project-control route before calling the Source reader.
    let project = f.projects[0].0.clone();
    f.stop.cancel();
    f.server.shutdown().await.unwrap();
    let fixture_support::Fixture {
        temp,
        paths: installation,
        server,
        auth,
        ..
    } = f;
    drop(server);
    drop(auth);
    let owner = ForegroundSource::start(
        &installation,
        &[ProjectBootstrapInput {
            project: &project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        }],
        0,
        &config,
        NativeBinding::KiroV3,
        &request,
    )
    .unwrap();
    let stop = Cancellation::default();
    let service = stop.clone();
    let serving = tokio::spawn(async move { owner.run(&service).await });
    for (slug, tail) in [("alpha", false), ("alpha", true), ("missing", false)] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .current_dir(project.root())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", temp.path())
            .env(
                installation
                    .brand()
                    .environment_variable("GLOBAL_DIR")
                    .unwrap(),
                installation.root(),
            )
            .args(["knowledge", "source", slug]);
        if tail {
            command.args(["--tail", "2", "--line-numbers"]);
        }
        let output = cli_daemon::output(command).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        if slug == "missing" {
            assert_eq!(result["native"]["kind"], "page_absent");
            continue;
        }
        assert_eq!(result["native"]["kind"], "found");
        let source = result["native"]["selection"]["source"].as_str().unwrap();
        assert!(source.contains("CLI-LAST-LINE"));
        if tail {
            assert!(source.len() < 100);
        } else {
            assert!(source.len() > 1000);
            assert!(source.ends_with(&body));
        }
    }
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(5), serving)
        .await
        .expect("Source owner must finish after cancellation")
        .unwrap()
        .unwrap();
}
