use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};
mod ladybug;
mod safety;
mod test_selection;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FastTest {
    NativeHubPublication,
    RoleHubFile,
    AstGenericMainNativeNn,
    Integration {
        package: &'static str,
        target: &'static str,
    },
    Library {
        package: &'static str,
        filter: &'static str,
    },
    NamedLibrary {
        package: &'static str,
        filter: &'static str,
        selector: &'static str,
    },
}

impl FastTest {
    fn arguments(&self) -> Vec<&'static str> {
        match *self {
            Self::RoleHubFile => vec![
                "test",
                "--locked",
                "-p",
                "graphit-app",
                "--lib",
                "role_tool_server::tests::hub::role_hub_file_metadata_uses_current_role_service_and_real_broker_project_grant",
                "--",
                "--ignored",
                "--exact",
                "--nocapture",
            ],
            Self::NativeHubPublication => vec![
                "test",
                "--locked",
                "-p",
                "graphit-app",
                "--test",
                "source_executable",
                "native_publication::compiled_hub_publish_native_uses_broker_sts_and_preserves_versions",
                "--",
                "--ignored",
                "--exact",
                "--nocapture",
            ],
            Self::AstGenericMainNativeNn => vec![
                "test",
                "--locked",
                "-p",
                "graphit-app",
                "--test",
                "ast_generic_nn_executable",
                "generic_main_native_nn",
                "--",
                "--ignored",
                "--exact",
                "--test-threads=1",
            ],
            Self::Integration { package, target } => {
                vec!["test", "--locked", "-p", package, "--test", target]
            }
            Self::Library { package, filter }
            | Self::NamedLibrary {
                package, filter, ..
            } => {
                vec!["test", "--locked", "-p", package, "--lib", filter]
            }
        }
    }

    fn label(&self) -> String {
        match *self {
            Self::NativeHubPublication => "graphit-app/native-hub-publication".into(),
            Self::RoleHubFile => "graphit-app/role-hub-file".into(),
            Self::AstGenericMainNativeNn => "graphit-app/ast-generic-main-native-nn".into(),
            Self::Integration { package, target } => format!("{package}/{target}"),
            Self::NamedLibrary {
                package, selector, ..
            } => format!("{package}/{selector}"),
            Self::Library {
                package, filter, ..
            } => {
                let target = filter
                    .rsplit("::")
                    .find(|part| !part.is_empty() && *part != "tests")
                    .unwrap_or(filter);
                format!("{package}/{target}")
            }
        }
    }
}

const POSTGRESQL_SEMANTIC_TEST: FastTest = FastTest::Integration {
    package: "graphit-ast",
    target: "postgresql_semantic",
};
const SOURCE_PUBLICATION_ORDER_TEST: FastTest = FastTest::Integration {
    package: "graphit-core",
    target: "project_source_publication_order",
};
const GIT_INSPECTION_TEST: FastTest = FastTest::Library {
    package: "graphit-app",
    filter: "role_tool_server::tests::git_inspection_",
};
const PROJECT_HOOK_INSTALL_TEST: FastTest = FastTest::Integration {
    package: "graphit-app",
    target: "project_hook_install",
};
const PROJECT_COORDINATOR_INSTALL_TEST: FastTest = FastTest::Integration {
    package: "graphit-app",
    target: "project_coordinator_install",
};
const MCP_SOURCE_TEST: FastTest = FastTest::Integration {
    package: "graphit-app",
    target: "mcp_source",
};
const ROLE_LIVE_TEST: FastTest = FastTest::Library {
    package: "graphit-app",
    filter: "role_tool_server::tests::actual_role_live_",
};
const ROLE_HOOK_EXECUTABLE_TEST: FastTest = FastTest::Integration {
    package: "graphit-app",
    target: "role_hook_v3_executable",
};
const ROLE_HOOK_TEST: FastTest = FastTest::Library {
    package: "graphit-app",
    filter: "role_tool_server::tests::role_hook_",
};
const MANAGED_LIVE_TEST: FastTest = FastTest::Library {
    package: "graphit-app",
    filter: "role_tool_server::tests::run_bootstrap_http_live_context_",
};
const SOURCE_STREAM_TEST: FastTest = FastTest::Integration {
    package: "graphit-data",
    target: "lance_source_stream",
};
const KIMI_UNIT_TEST: FastTest = FastTest::NamedLibrary {
    package: "graphit-ai",
    filter: "cli_kimi::tests::",
    selector: "cli_kimi_unit",
};

const COPILOT_INTEGRATION_TEST: FastTest = FastTest::Integration {
    package: "graphit-ai",
    target: "cli_copilot",
};

const REGISTERED_TESTS: &[FastTest] = &[
    FastTest::Integration {
        package: "graphit-app",
        target: "git_hooks_executable",
    },
    FastTest::AstGenericMainNativeNn,
    FastTest::NativeHubPublication,
    FastTest::RoleHubFile,
    POSTGRESQL_SEMANTIC_TEST,
    SOURCE_PUBLICATION_ORDER_TEST,
    GIT_INSPECTION_TEST,
    PROJECT_HOOK_INSTALL_TEST,
    PROJECT_COORDINATOR_INSTALL_TEST,
    MCP_SOURCE_TEST,
    ROLE_LIVE_TEST,
    ROLE_HOOK_TEST,
    ROLE_HOOK_EXECUTABLE_TEST,
    MANAGED_LIVE_TEST,
    SOURCE_STREAM_TEST,
    FastTest::Integration {
        package: "graphit-data",
        target: "lance_module_creation",
    },
    FastTest::Library {
        package: "graphit-app",
        filter: "foreground_memory_jobs_watch_configs_follow_each_project_and_captured_overrides",
    },
    FastTest::Library {
        package: "graphit-app",
        filter: "foreground_memory_jobs_missing_model_keeps_source_without_compute",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "now_executable",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "now_presence_integration",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "managed_role_run",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "source_executable",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "models_cli",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "help_cli",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "version_cli",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "package",
    },
    FastTest::Library {
        package: "graphit-auth",
        filter: "interactive_login::",
    },
    FastTest::Integration {
        package: "graphit-knowledge",
        target: "compilation_fingerprint",
    },
    FastTest::Integration {
        package: "graphit-ast",
        target: "source_ranges",
    },
    FastTest::Integration {
        package: "graphit-records",
        target: "gates",
    },
    FastTest::Library {
        package: "graphit-ai",
        filter: "model_bindings_tests::",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "cli_claude",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "cli_qwen",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "cli_cursor",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "cli_antigravity",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "knowledge_antigravity_ask",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "cli_grok",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "knowledge_grok_ask",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "cli_opencode",
    },
    KIMI_UNIT_TEST,
    COPILOT_INTEGRATION_TEST,
    FastTest::Integration {
        package: "graphit-app",
        target: "knowledge_copilot_ask",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "cli_kimi",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "knowledge_opencode_ask",
    },
    FastTest::Integration {
        package: "graphit-app",
        target: "knowledge_kimi_ask",
    },
    FastTest::Integration {
        package: "graphit-process",
        target: "detached",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "ranking",
    },
    FastTest::Integration {
        package: "graphit-runtime",
        target: "output",
    },
    FastTest::Integration {
        package: "graphit-knowledge",
        target: "references",
    },
    FastTest::Library {
        package: "graphit-core",
        filter: "artifact_store::source_reader::",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "direct_embeddings",
    },
    FastTest::Integration {
        package: "graphit-runtime",
        target: "roles",
    },
    FastTest::Integration {
        package: "graphit-knowledge",
        target: "staleness",
    },
    FastTest::Integration {
        package: "graphit-ast",
        target: "scalar_policy",
    },
    FastTest::Library {
        package: "graphit-auth",
        filter: "issuer::tests::records::",
    },
    FastTest::Integration {
        package: "graphit-knowledge",
        target: "settings",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "cohere_embeddings",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "voyage_embeddings",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "google_embeddings",
    },
    FastTest::Integration {
        package: "graphit-process",
        target: "capture",
    },
    FastTest::Integration {
        package: "graphit-process",
        target: "incremental",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "native_rerank",
    },
    FastTest::Integration {
        package: "graphit-ai",
        target: "cohere_reranking",
    },
    FastTest::Library {
        package: "graphit-core",
        filter: "artifact_store::native_source::",
    },
    FastTest::Library {
        package: "graphit-core",
        filter: "file_io::",
    },
    FastTest::Library {
        package: "graphit-fs",
        filter: "read_directory::",
    },
    FastTest::Library {
        package: "graphit-auth",
        filter: "issuer::tests::service_credentials::",
    },
    FastTest::Library {
        package: "graphit-auth",
        filter: "issuer::tests::user_subject::",
    },
    FastTest::Library {
        package: "graphit-hub",
        filter: "remote_version::",
    },
    FastTest::Integration {
        package: "graphit-knowledge",
        target: "publication_metadata",
    },
    FastTest::Integration {
        package: "graphit-knowledge",
        target: "project_document",
    },
    FastTest::Integration {
        package: "graphit-ast",
        target: "prepared_input",
    },
    FastTest::Integration {
        package: "graphit-knowledge",
        target: "ignore_policy",
    },
    FastTest::Integration {
        package: "graphit-ast",
        target: "profiles",
    },
    FastTest::Integration {
        package: "graphit-knowledge",
        target: "preview",
    },
    FastTest::Library {
        package: "graphit-auth",
        filter: "issuer::tests::profile_manager::broker_login::",
    },
    FastTest::Library {
        package: "graphit-auth",
        filter: "issuer::tests::profile_manager::",
    },
    FastTest::Integration {
        package: "graphit-ast",
        target: "catalog",
    },
    FastTest::Integration {
        package: "graphit-knowledge",
        target: "document",
    },
    FastTest::Integration {
        package: "graphit-knowledge",
        target: "markdown_export",
    },
    FastTest::Integration {
        package: "graphit-knowledge",
        target: "published_lint",
    },
    FastTest::Integration {
        package: "graphit-ast",
        target: "symbols",
    },
    FastTest::Library {
        package: "graphit-daemon",
        filter: "watcher::tests::",
    },
    FastTest::Library {
        package: "graphit-daemon",
        filter: "supervisor::tests::",
    },
    FastTest::Library {
        package: "graphit-daemon",
        filter: "services::tests::",
    },
];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Rust guardrail: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mode = env::args().nth(1).ok_or(
        "usage: cargo xtask <check|test SELECTOR...|gate|qualify|native [OPTIONS]|cargo COMMAND...>",
    )?;
    let workspace = workspace_root()?;
    let root = workspace.as_path();
    match mode.as_str() {
        "native" => ladybug::prepare(root, &env::args().skip(2).collect::<Vec<_>>()).map(|_| ()),
        "cargo" => {
            let args = env::args().skip(2).collect::<Vec<_>>();
            if !matches!(
                args.first().map(String::as_str),
                Some("build" | "check" | "clippy" | "test" | "rustc")
            ) {
                return Err(
                    "usage: cargo xtask cargo <build|check|clippy|test|rustc> [Cargo arguments]"
                        .into(),
                );
            }
            execute(root, &args.iter().map(String::as_str).collect::<Vec<_>>())
        }
        "check" => check(root),
        "safety" => safety::run(root),
        "test" => test_selection::run_selected(root, &env::args().skip(2).collect::<Vec<_>>()),
        "gate" => {
            static_checks(root)?;
            tests(root)
        }
        "qualify" => qualify(root),
        _ => Err(format!("unknown guardrail {mode:?}")),
    }
}

fn workspace_root() -> Result<PathBuf, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["locate-project", "--workspace", "--message-format", "plain"])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let manifest = PathBuf::from(
        String::from_utf8(output.stdout)
            .map_err(|error| error.to_string())?
            .trim(),
    );
    manifest
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "workspace root missing".into())
}

fn execute(root: &Path, arguments: &[&str]) -> Result<(), String> {
    let cargo = env::var_os("CARGO")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cargo"));
    eprintln!("Rust guardrail: cargo {}", arguments.join(" "));
    let mut command = Command::new(cargo);
    command
        .args(arguments)
        .current_dir(root)
        // Cargo supplies these for each package. Inheriting xtask metadata
        // invalidates dependency build scripts that watch the same variables.
        .env_remove("CARGO_MANIFEST_DIR")
        .env_remove("CARGO_PKG_NAME")
        .env_remove("CARGO_PKG_VERSION_MAJOR")
        .env_remove("CARGO_PKG_VERSION_MINOR")
        .env_remove("CARGO_PKG_VERSION_PATCH")
        .env_remove("CARGO_PKG_VERSION_PRE");
    let _prepared = if matches!(
        arguments.first(),
        Some(&"build" | &"check" | &"clippy" | &"test" | &"rustc")
    ) {
        let prepared = ladybug::prepare(
            root,
            &arguments
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>(),
        )?;
        prepared.apply(&mut command);
        Some(prepared)
    } else {
        None
    };
    if arguments.first() == Some(&"test") && env::var_os("RUST_MIN_STACK").is_none() {
        // Compound unoptimized Lance/S3 futures exceed libtest's 2 MiB default.
        // This applies to test children only; production runtime is unchanged.
        command.env("RUST_MIN_STACK", "8388608");
    }
    let status = command.status().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo {} failed: {status}", arguments.join(" ")))
    }
}

fn check(root: &Path) -> Result<(), String> {
    source_checks(root)
}

fn static_checks(root: &Path) -> Result<(), String> {
    source_checks(root)?;
    execute(
        root,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--offline",
            "--keep-going",
            "--",
            "-D",
            "warnings",
        ],
    )
}

fn source_checks(root: &Path) -> Result<(), String> {
    safety::run(root)?;
    if cfg!(windows) {
        // cargo fmt --all passes every target to one rustfmt invocation and
        // exceeds CreateProcess's command-line size in this workspace. Keep
        // its local path-dependency coverage.
        let metadata = cargo_metadata::MetadataCommand::new()
            .manifest_path(root.join("Cargo.toml"))
            .other_options(vec!["--locked".into(), "--offline".into()])
            .exec()
            .map_err(|error| error.to_string())?;
        for package in metadata.packages.iter().filter(|p| p.source.is_none()) {
            execute(
                root,
                &[
                    "fmt",
                    "--manifest-path",
                    package.manifest_path.as_str(),
                    "--package",
                    package.name.as_ref(),
                    "--",
                    "--check",
                ],
            )?;
        }
        Ok(())
    } else {
        execute(root, &["fmt", "--all", "--", "--check"])
    }
}

fn tests(root: &Path) -> Result<(), String> {
    execute(
        root,
        &[
            "test",
            "--workspace",
            "--locked",
            "--offline",
            "--no-fail-fast",
        ],
    )
}

fn qualify(root: &Path) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("native model and S3 qualification requires Linux".into());
    }
    let reranker = "GRAPHIT_BGE_RERANKER_V2_M3_MODEL";
    native_nn_resource(reranker, env::var_os(reranker))?;
    execute_test(root, FastTest::AstGenericMainNativeNn)?;
    execute(
        root,
        &[
            "test",
            "--locked",
            "--offline",
            "-p",
            "graphit-app",
            "--lib",
            "--",
            "--ignored",
            "--test-threads=1",
            "native_ast_fts_local_bge_rerank_without_embedding_preserves_the_published_page",
            "knowledge_foreground_actual_local_bge_rerank_uses_control_without_local_embedding",
        ],
    )?;
    execute_test(root, FastTest::RoleHubFile)?;
    execute(
        root,
        &[
            "test",
            "--locked",
            "--offline",
            "-p",
            "graphit-app",
            "--test",
            "source_executable",
            "native_publication::file_installation",
            "--",
            "--ignored",
        ],
    )
}

fn native_nn_resource(name: &str, value: Option<std::ffi::OsString>) -> Result<(), String> {
    let path = PathBuf::from(
        value
            .ok_or_else(|| format!("{name} must name an existing absolute native CPU resource"))?,
    );
    if !path.is_absolute() || !path.is_file() {
        return Err(format!(
            "{name} must name an existing absolute native CPU resource"
        ));
    }
    Ok(())
}

fn execute_test(root: &Path, test: FastTest) -> Result<(), String> {
    if matches!(test, FastTest::AstGenericMainNativeNn) {
        if !cfg!(target_os = "linux") {
            return Err("AST generic Main NN is Linux-only; Windows/macOS UNRUN".into());
        }
        for name in ["GRAPHIT_TEST_MODEL_SERVER", "GRAPHIT_EMBEDDINGGEMMA2_MODEL"] {
            native_nn_resource(name, env::var_os(name))?;
        }
    }
    if matches!(test, FastTest::RoleHubFile) {
        let path = env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
            .map(PathBuf::from)
            .ok_or(
                "GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN must name the owned Broker storage fixture",
            )?;
        if !path.is_absolute() || !path.is_file() {
            return Err("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN must name an existing absolute Broker storage fixture".into());
        }
    }
    let mut arguments = test.arguments();
    arguments.insert(1, "--offline");
    execute(root, &arguments)
}
