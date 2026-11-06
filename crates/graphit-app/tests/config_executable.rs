//! Configuration through the actual Main, with private stores and no daemon.
use crate::finite_cli;
use finite_cli::FiniteOutput;
use graphit_core::{
    brand::Brand,
    config::ConfigStore,
    file_lock::Cancellation,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::Duration,
};

struct Fixture {
    _temp: tempfile::TempDir,
    global: PathBuf,
    project: PathBuf,
    outside: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let global = temp.path().join("installation");
        let project = temp.path().join("project");
        let outside = temp.path().join("outside");
        fs::create_dir_all(project.join(Brand::default().directory())).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let installation =
            InstallationPaths::resolve(Brand::default(), &project, None, Some(&global)).unwrap();
        let paths = ProjectPaths::at(&project, &installation).unwrap();
        ProjectIdentityStore::at(&paths)
            .writer(Duration::ZERO, &Cancellation::default())
            .unwrap()
            .ensure(&ProjectMetadata::new("config-fixture").unwrap())
            .unwrap();
        Self {
            _temp: temp,
            global,
            project,
            outside,
        }
    }

    fn command(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .env_clear()
            .current_dir(cwd)
            .arg("--global-dir")
            .arg(&self.global)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn call(&self, args: &[&str]) -> Output {
        self.command(&self.project, args).finite_output().unwrap()
    }

    fn json(&self, args: &[&str]) -> Value {
        successful(self.call(args))
    }

    fn project_file(&self) -> PathBuf {
        self.project
            .join(Brand::default().directory())
            .join("project.json")
    }
}

fn successful(output: Output) -> Value {
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn denied(output: Output) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

fn invalid_argument(output: Output) {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("error: invalid value"));
}

#[test]
fn main_config_csv_values_and_inline_secrets_follow_the_same_parser() {
    let f = Fixture::new();
    for key in [
        "ast.grammar",
        "ast.grammars_whitelist",
        "ast.grammars_blacklist",
        "ast.scip.parsers",
        "knowledge.extensions",
    ] {
        for value in ["md,txt", "false", "123", ""] {
            f.json(&["config", "set", key, value]);
            assert_eq!(f.json(&["config", "get", key])["value"]["value"], value);
            let inline = format!("{key}={value}");
            let result = f.json(&["-c", &inline, "config", "get", key]);
            assert_eq!(result["source"], "command");
            assert_eq!(result["value"]["value"], value);
        }
    }
    let name = graphit_core::config::ConfigKey::new("knowledge.extensions")
        .unwrap()
        .environment_variable(&Brand::default());
    let environment = successful(
        f.command(&f.project, &["config", "get", "knowledge.extensions"])
            .env(&name, "md, txt")
            .finite_output()
            .unwrap(),
    );
    assert_eq!(environment["source"], "environment");
    assert_eq!(environment["value"]["value"], "md, txt");
    let command = successful(
        f.command(
            &f.project,
            &[
                "-c",
                "knowledge.extensions=",
                "config",
                "get",
                "knowledge.extensions",
            ],
        )
        .env(&name, "md, txt")
        .finite_output()
        .unwrap(),
    );
    assert_eq!(command["source"], "command");
    assert_eq!(command["value"]["value"], "");
    for text in ["null", "false", "123", "fixture,secret"] {
        let inline = format!("client.secret={text}");
        let result = f.json(&["-c", &inline, "config", "get", "client.secret"]);
        assert_eq!(result["value"], json!({"kind":"secret","configured":true}));
    }
    assert!(!f.global.exists());
}

#[test]
fn main_config_typed_crud_and_scoped_resolution() {
    let f = Fixture::new();
    assert_eq!(
        f.json(&["config", "--global", "set", "custom.shared", "global"]),
        json!({"changed":true})
    );
    assert_eq!(
        f.json(&["config", "set", "custom.shared", "project"]),
        json!({"changed":true})
    );
    assert_eq!(
        f.json(&["config", "set", "custom.shared", "project"]),
        json!({"changed":false})
    );
    let project = f.json(&["config", "get", "custom.shared"]);
    assert_eq!(
        project,
        json!({"key":"custom.shared","source":"project","value":{"kind":"public","value":"project"}})
    );
    let global = f.json(&["config", "--global", "get", "custom.shared"]);
    assert_eq!(global["source"], "global");
    assert_eq!(global["value"]["value"], "global");

    let name = graphit_core::config::ConfigKey::new("custom.shared")
        .unwrap()
        .environment_variable(&Brand::default());
    let environment = successful(
        f.command(&f.project, &["config", "get", "custom.shared"])
            .env(&name, "environment")
            .finite_output()
            .unwrap(),
    );
    assert_eq!(environment["source"], "environment");
    assert_eq!(environment["value"]["value"], "environment");
    let inline = successful(
        f.command(
            &f.project,
            &[
                "-c",
                "custom.shared=command",
                "config",
                "get",
                "custom.shared",
            ],
        )
        .env(&name, "environment")
        .finite_output()
        .unwrap(),
    );
    assert_eq!(inline["source"], "command");
    assert_eq!(inline["value"]["value"], "command");
    let global = successful(
        f.command(
            &f.project,
            &[
                "-c",
                "custom.shared=command",
                "config",
                "--global",
                "get",
                "custom.shared",
            ],
        )
        .env(&name, "environment")
        .finite_output()
        .unwrap(),
    );
    assert_eq!(global["source"], "global");
    assert_eq!(global["value"]["value"], "global");

    for (key, text, expected) in [
        ("custom.empty", "", json!("")),
        ("custom.false", "false", json!(false)),
        ("custom.zero", "0", json!(0)),
        (
            "custom.whitespace",
            "  literal text  ",
            json!("  literal text  "),
        ),
        ("custom.array", "[1,false]", json!([1, false])),
        (
            "custom.object",
            "{\"enabled\":true}",
            json!({"enabled":true}),
        ),
    ] {
        f.json(&["config", "set", key, text]);
        assert_eq!(f.json(&["config", "get", key])["value"]["value"], expected);
    }
    let entries = f.json(&["config", "list"]);
    let entries = entries.as_array().unwrap();
    assert!(
        entries
            .windows(2)
            .all(|pair| pair[0]["key"].as_str().unwrap() < pair[1]["key"].as_str().unwrap())
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry["key"] == "custom.false" && entry["value"]["value"] == false)
    );
    let subdirectory = f.project.join("nested");
    fs::create_dir(&subdirectory).unwrap();
    let nested = successful(
        f.command(&subdirectory, &["config", "get", "custom.shared"])
            .finite_output()
            .unwrap(),
    );
    assert_eq!(nested, project);
    assert_eq!(
        f.json(&["config", "unset", "custom.shared"]),
        json!({"changed":true})
    );
    assert_eq!(
        f.json(&["config", "unset", "custom.shared"]),
        json!({"changed":false})
    );
    assert_eq!(
        f.json(&["config", "get", "custom.shared"])["value"]["value"],
        "global"
    );
    assert!(!f.global.join("auth.json").exists());
    assert!(!f.global.join("runtime").exists());
}

#[test]
fn main_config_root_githooks_preserves_project_and_resolves_global_env_command() {
    let f = Fixture::new();
    let before: Value = serde_json::from_slice(&fs::read(f.project_file()).unwrap()).unwrap();
    let hooks =
        json!({"pre-commit":["printf 'ação, 日本語 \\\"ok\\\"\\n'", "git diff --cached --check"]});
    let hooks_text = hooks.to_string();
    f.json(&["config", "--global", "set", "githooks", &hooks_text]);
    assert_eq!(f.json(&["config", "get", "githooks"])["source"], "global");
    f.json(&["config", "set", "githooks", &hooks_text]);
    let root: Value = serde_json::from_slice(&fs::read(f.project_file()).unwrap()).unwrap();
    assert_eq!(root["id"], before["id"]);
    assert_eq!(root["name"], before["name"]);
    assert_eq!(root["githooks"], hooks);
    assert!(root["settings"].get("githooks").is_none());
    let installation =
        InstallationPaths::resolve(Brand::default(), &f.project, None, Some(&f.global)).unwrap();
    let paths = ProjectPaths::at(&f.project, &installation).unwrap();
    let store = ProjectIdentityStore::at(&paths);
    let identity = store.load().unwrap().unwrap();
    let mut metadata = identity.metadata().clone();
    metadata.description = "Unicode λ e espaços".into();
    store
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .update(identity.id(), &metadata)
        .unwrap();
    f.json(&["config", "set", "custom.flag", "true"]);
    let root: Value = serde_json::from_slice(&fs::read(f.project_file()).unwrap()).unwrap();
    assert_eq!(root["description"], metadata.description);
    assert_eq!(root["githooks"], hooks);
    let override_text = json!({"pre-push":["printf 'env, λ'"]}).to_string();
    let environment = successful(
        f.command(&f.project, &["config", "get", "githooks"])
            .env("GRAPHIT_GITHOOKS", &override_text)
            .finite_output()
            .unwrap(),
    );
    assert_eq!(environment["source"], "environment");
    assert_eq!(environment["value"]["value"], override_text);
    let inline = format!("githooks={hooks_text}");
    let command = successful(
        f.command(&f.project, &["-c", &inline, "config", "get", "githooks"])
            .env("GRAPHIT_GITHOOKS", &override_text)
            .finite_output()
            .unwrap(),
    );
    assert_eq!(command["source"], "command");
    assert_eq!(command["value"]["value"], hooks);
    f.json(&["config", "set", "githooks", "{}"]);
    assert_eq!(
        f.json(&["config", "get", "githooks"])["value"]["value"],
        json!({})
    );
    f.json(&["config", "unset", "githooks"]);
    assert_eq!(f.json(&["config", "get", "githooks"])["source"], "global");
    for key in [
        "id",
        "name",
        "description",
        "cluster",
        "ephemeral",
        "git.hooks",
        "settings.githooks",
    ] {
        let unchanged = fs::read(f.project_file()).unwrap();
        assert!(!f.call(&["config", "set", key, "invalid"]).status.success());
        assert_eq!(fs::read(f.project_file()).unwrap(), unchanged);
    }
    assert!(!paths.brand_directory().join("config.json").exists());
    fs::write(
        paths.brand_directory().join("config.json"),
        b"invalid abandoned file",
    )
    .unwrap();
    assert_eq!(f.json(&["config", "get", "githooks"])["source"], "global");
    assert_eq!(store.load().unwrap().unwrap().id(), identity.id());
}

#[test]
fn main_config_reads_have_no_effects_and_global_works_without_a_project() {
    let f = Fixture::new();
    let project_before = fs::read(f.project_file()).unwrap();
    let empty = successful(
        f.command(&f.outside, &["config", "--global", "list"])
            .finite_output()
            .unwrap(),
    );
    assert_eq!(empty, json!([]));
    denied(
        f.command(&f.outside, &["config", "--global", "get", "custom.missing"])
            .finite_output()
            .unwrap(),
    );
    assert!(!f.global.exists());
    assert_eq!(fs::read(f.project_file()).unwrap(), project_before);
    assert!(!f.project_file().with_file_name("config.json").exists());
    denied(
        f.command(&f.outside, &["config", "set", "custom.missing", "true"])
            .finite_output()
            .unwrap(),
    );
    assert!(!f.global.exists());
    successful(
        f.command(
            &f.outside,
            &["config", "--global", "set", "custom.setting", "1"],
        )
        .finite_output()
        .unwrap(),
    );
    let captured = successful(
        f.command(&f.outside, &["config", "get", "custom.setting"])
            .finite_output()
            .unwrap(),
    );
    assert_eq!(captured["source"], "global");
    assert_eq!(captured["value"]["value"], 1);
    denied(f.call(&[
        "--project-version",
        "elsewhere",
        "config",
        "get",
        "custom.setting",
    ]));
    let help = f.call(&["config", "--help"]);
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    for name in ["set", "get", "list", "unset"] {
        assert!(help.contains(name));
    }
    assert_eq!(fs::read(f.project_file()).unwrap(), project_before);
}

#[test]
fn main_config_preserves_policy_types_readonly_and_concurrent_updates() {
    let f = Fixture::new();
    f.json(&["config", "set", "modules.sync", "false"]);
    let installation =
        InstallationPaths::resolve(Brand::default(), &f.outside, None, Some(&f.global)).unwrap();
    let project = ProjectPaths::at(&f.project, &installation).unwrap();
    let document = ConfigStore::project(&project).unwrap().load().unwrap();
    let snapshot =
        graphit_core::config::ConfigSnapshot::new(&Brand::default(), [document]).unwrap();
    assert!(
        !graphit_core::module_settings::ModuleSettings::resolve(&snapshot)
            .unwrap()
            .enabled(graphit_core::module_settings::Module::Sync)
    );
    let before = fs::read(f.project_file()).unwrap();
    for (key, value) in [
        ("modules.sync", "invalid-boolean"),
        ("custom.value", "null"),
        ("models.embedding.id", "project-is-forbidden"),
        ("modules.daemon_ui", "true"),
    ] {
        denied(f.call(&["config", "set", key, value]));
        assert_eq!(fs::read(f.project_file()).unwrap(), before);
    }
    let removed = f.call(&["config", "set", "ast.allow_native_grammars", "true"]);
    assert!(String::from_utf8_lossy(&removed.stderr).contains("was removed"));
    invalid_argument(removed);
    assert_eq!(fs::read(f.project_file()).unwrap(), before);
    let forbidden = f.call(&["config", "set", "auth.token", "forbidden"]);
    assert!(!forbidden.status.success());
    assert!(forbidden.stdout.is_empty());
    assert_eq!(fs::read(f.project_file()).unwrap(), before);
    let permissions = fs::metadata(f.project_file()).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(f.project_file(), readonly).unwrap();
    denied(f.call(&["config", "set", "custom.readonly", "1"]));
    assert_eq!(fs::read(f.project_file()).unwrap(), before);
    fs::set_permissions(f.project_file(), permissions).unwrap();
    let [first, second] = concurrent([
        f.command(&f.project, &["config", "set", "custom.concurrent_a", "1"]),
        f.command(&f.project, &["config", "set", "custom.concurrent_b", "2"]),
    ])
    .unwrap();
    successful(first);
    successful(second);
    assert_eq!(
        f.json(&["config", "get", "custom.concurrent_a"])["value"]["value"],
        1
    );
    assert_eq!(
        f.json(&["config", "get", "custom.concurrent_b"])["value"]["value"],
        2
    );
}

#[test]
fn main_config_secret_inspection_and_errors_never_echo_the_value() {
    let f = Fixture::new();
    let project_before = fs::read(f.project_file()).unwrap();
    let secret = "synthetic-config-secret-for-output-check";
    let set = f.call(&["config", "--global", "set", "client.secret", secret]);
    assert!(set.status.success());
    assert!(!String::from_utf8_lossy(&set.stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&set.stderr).contains(secret));
    for args in [
        vec!["config", "get", "client.secret"],
        vec!["config", "--global", "get", "client.secret"],
        vec!["config", "list"],
        vec!["config", "--global", "list"],
    ] {
        let output = f.call(&args);
        assert!(output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
        assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
        let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
        let entry = payload.as_array().map_or(&payload, |values| {
            values
                .iter()
                .find(|entry| entry["key"] == "client.secret")
                .unwrap()
        });
        assert_eq!(entry["value"], json!({"kind":"secret","configured":true}));
    }
    let denied = f.call(&["config", "set", "client.secret", secret]);
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&denied.stderr).contains(secret));
    assert_eq!(fs::read(f.project_file()).unwrap(), project_before);
    assert!(!f.project_file().with_file_name("config.json").exists());
    assert!(!f.global.join("auth.json").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(f.global.join("config.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }
}

#[test]
fn main_config_parser_list_replaces_lower_list_and_removed_native_option_is_rejected() {
    let f = Fixture::new();
    f.json(&[
        "config",
        "--global",
        "set",
        "ast.scip.parsers",
        ".zig=registry.example/team/parser:1",
    ]);
    f.json(&[
        "config",
        "set",
        "ast.scip.parsers",
        ".exm=localhost:5000/parser:dev",
    ]);
    assert_eq!(
        f.json(&["config", "get", "ast.scip.parsers"])["value"]["value"],
        json!(".exm=localhost:5000/parser:dev")
    );
    f.json(&["config", "set", "ast.scip.parsers", ""]);
    assert_eq!(
        f.json(&["config", "get", "ast.scip.parsers"])["value"]["value"],
        json!("")
    );
    let project = fs::read(f.project_file()).unwrap();
    let global = fs::read(f.global.join("config.json")).unwrap();
    for args in [
        vec!["config", "set", "ast.allow_native_grammars", "true"],
        vec![
            "config",
            "--global",
            "set",
            "ast.allow_native_grammars",
            "true",
        ],
        vec!["-c", "ast.allow_native_grammars=true", "config", "list"],
    ] {
        let output = f.call(&args);
        assert!(String::from_utf8_lossy(&output.stderr).contains("was removed"));
        invalid_argument(output);
        assert_eq!(fs::read(f.project_file()).unwrap(), project);
        assert_eq!(fs::read(f.global.join("config.json")).unwrap(), global);
    }
}

fn concurrent(commands: [Command; 2]) -> std::io::Result<[Output; 2]> {
    let first = finite_cli::request(&commands[0])?;
    let second = finite_cli::request(&commands[1])?;
    finite_cli::runtime()?.block_on(async {
        let host = finite_cli::host(2).await?;
        let (first, second) = tokio::join!(
            finite_cli::capture(&host, first),
            finite_cli::capture(&host, second)
        );
        finite_cli::joined(host).await?;
        Ok([first?, second?])
    })
}
