//! Productive init -> managed daemon -> anonymous UI, in a fresh installation.
use super::*;
struct LocalInstallation {
    temp: tempfile::TempDir,
    paths: InstallationPaths,
    project: std::path::PathBuf,
    runtime: std::path::PathBuf,
}
impl LocalInstallation {
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command.arg("--json");
        command
            .current_dir(&self.project)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.temp.path())
            .env(
                self.paths
                    .brand()
                    .environment_variable("GLOBAL_DIR")
                    .unwrap(),
                self.paths.root(),
            )
            .env("XDG_RUNTIME_DIR", &self.runtime)
            .env("XDG_CONFIG_HOME", &self.runtime)
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}", self.runtime.join("absent-bus").display()),
            );
        command
    }
    async fn output(&self, args: &[&str]) -> std::process::Output {
        let mut command = self.command();
        command.args(args);
        let output = tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}
impl Drop for LocalInstallation {
    fn drop(&mut self) {
        let _ = self.command().args(["daemon", "stop"]).output();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_fresh_init_opens_anonymous_local_ui_without_browser_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("new-project");
    fs::create_dir(&project).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet", "--initial-branch=main"])
            .arg(&project)
            .status()
            .unwrap()
            .success()
    );
    let runtime = temp.path().join("private-runtime");
    fs::create_dir(&runtime).unwrap();
    let paths = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    let cancellation = Cancellation::default();
    for (key, value) in [
        ("modules.embedding", false),
        ("modules.dream", false),
        ("modules.ast", false),
    ] {
        ConfigStore::global(&paths)
            .unwrap()
            .set(
                ConfigKey::new(key).unwrap(),
                json!(value),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
    }
    drop(graphit_fs::create_private_directory(&paths.models_directory()).unwrap());
    let foreign_model = paths.models_directory().join("other-program.txt");
    fs::write(&foreign_model, "unrelated").unwrap();
    assert!(!paths.auth_file().exists());
    let f = LocalInstallation {
        temp,
        paths,
        project,
        runtime,
    };
    let coordinator_source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../templates/agent-instructions/coordinator");
    let unrelated = f.temp.path().join("unrelated");
    fs::create_dir(&unrelated).unwrap();
    let lock = ConfigStore::global(&f.paths).unwrap().lock_path();
    fs::remove_file(&lock).unwrap();
    fs::create_dir(&lock).unwrap();
    let partial = f
        .command()
        .args(["init", "--project-dir"])
        .arg(&unrelated)
        .output()
        .unwrap();
    fs::remove_dir(lock).unwrap();
    assert!(!partial.status.success());
    let partial: Value = serde_json::from_slice(&partial.stdout).unwrap();
    assert_eq!(partial["status"], "error");
    assert_eq!(partial["failed_step"], "agent_configuration");
    assert!(
        partial["project"]["identity"]["id"].is_string(),
        "{partial}"
    );
    assert!(unrelated.join(".graphit/project.json").is_file());

    assert!(!f.project.join(".graphit").exists());
    let default = f
        .output(&[
            "init",
            "--coordinator-source",
            coordinator_source.to_str().unwrap(),
        ])
        .await;
    let default: Value = serde_json::from_slice(&default.stdout).unwrap();
    assert_eq!(default["project"]["directory"], json!(f.project));
    assert_eq!(default["sync"]["status"], "ok");
    assert_eq!(default["agent"], "codex");
    assert!(default["sync"]["steps"]["native_global"]["roles_skills"]["roles"].is_array());
    f.output(&[
        "memory",
        "create",
        "--scope",
        "user",
        "--idempotency-key",
        "local-memory",
        "--title",
        "Local standing memory",
        "--body",
        "installation-personal-marker",
        "--mandatory",
    ])
    .await;
    let index = f.output(&["memory", "index", "--scope", "user"]).await;
    assert!(serde_json::from_slice::<Value>(&index.stdout).is_ok());
    let search = f
        .output(&["memory", "search", "--scope", "user", "standing"])
        .await;
    assert!(
        String::from_utf8_lossy(&search.stdout).contains("Local standing memory"),
        "{}",
        String::from_utf8_lossy(&search.stdout)
    );
    f.output(&["daemon", "stop"]).await;
    let output = f.output(&["ui", "--no-open"]).await;
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let url = result["url"].as_str().unwrap();
    let origin = url.strip_suffix("/task/gantt").unwrap();
    let http = client();
    assert_eq!(http.get(url).send().await.unwrap().status(), StatusCode::OK);
    let identity = http
        .get(format!("{origin}/auth/session"))
        .send()
        .await
        .unwrap();
    assert_eq!(identity.status(), StatusCode::OK);
    let identity = identity.json::<Value>().await.unwrap();
    assert_eq!(identity["mode"], "local");
    assert_eq!(identity["anonymous"], true);
    assert!(identity["subject"].is_null());
    for (path, body) in [
        ("tasks", json!({"selection":{}})),
        ("memory", json!({"input":{}})),
        ("knowledge", json!({"input":{}})),
    ] {
        let reply = http
            .post(format!("{origin}/api/{path}/contexts"))
            .header(header::ORIGIN, origin)
            .header("x-graphit-request", "ui")
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = reply.status();
        let reply = reply.text().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{path}: {reply}");
        let value: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(
            value["contexts"].as_array().unwrap().len(),
            1,
            "{path}: {reply}"
        );
    }
    let personal = http
        .post(format!("{origin}/api/user-memory/list"))
        .header(header::ORIGIN, origin)
        .header("x-graphit-request", "ui")
        .json(&json!({"input":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(personal.status(), StatusCode::OK);
    let personal: Value = personal.json().await.unwrap();
    assert_eq!(
        personal["native"]["memories"][0]["body"],
        "installation-personal-marker"
    );
    let scope = &personal["scope_id"];
    let target = &personal["native"]["memories"][0]["id"];
    let linked = http
        .post(format!("{origin}/api/user-memory/create"))
        .header(header::ORIGIN, origin)
        .header("x-graphit-request", "ui")
        .json(&json!({"input":{"idempotency_key":"local-link","title":"Related local memory",
            "body":"Uses the standing memory","attributes":{"classification":"fact","mandatory":false,"important":false},"tags":[],
            "references":[{"target_kind":"memory","scope":{"User":scope},"id":target,"context":null,"relation":"supports","field":null}]}}))
        .send().await.unwrap();
    let status = linked.status();
    let linked = linked.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{linked}");
    let focus = serde_json::from_str::<Value>(&linked).unwrap()["native"]["id"].clone();
    let references = http
        .post(format!("{origin}/api/records/references"))
        .header(header::ORIGIN, origin)
        .header("x-graphit-request", "ui")
        .json(
            &json!({"input":{"focus":{"kind":"memory","scope":{"user":scope},"id":focus},
            "outgoing_source":{"kind":"user_memory"},"include_user":true,
            "pages":[{"source":{"kind":"user_memory"},"direction":"outgoing"}]}}),
        )
        .send()
        .await
        .unwrap();
    let status = references.status();
    let references = references.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{references}");
    assert!(references.contains("supports"), "{references}");
    let hook = f
        .output(&[
            "_session-hook",
            "--engine",
            "codex",
            "--event",
            "SessionStart",
            "--project-dir",
            ".",
        ])
        .await;
    let hook = String::from_utf8(hook.stdout).unwrap();
    assert!(hook.contains("<GRAPHIT>"), "{hook}");
    assert!(
        hook.contains("Use the connected MCP catalog for every Graphit operation."),
        "{hook}"
    );
    assert!(hook.contains("USER Memory state: loaded"), "{hook}");
    assert!(hook.contains("installation-personal-marker"), "{hook}");
    let providers: Value =
        serde_json::from_slice(&f.output(&["provider", "list"]).await.stdout).unwrap();
    assert!(
        providers["providers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "local")
    );
    let accounts: Value =
        serde_json::from_slice(&f.output(&["account", "list"]).await.stdout).unwrap();
    assert!(accounts["active"].is_null());
    let post = http
        .post(format!("{origin}/api/tasks/contexts"))
        .header(header::ORIGIN, "https://foreign.example")
        .header("x-graphit-request", "ui")
        .json(&json!({"selection":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(post.status(), StatusCode::FORBIDDEN);
    // The D0 provider must remain editable through the ordinary Auth clock/API.
    f.output(&[
        "provider",
        "update",
        "local",
        "--embedding-mode",
        "disabled",
        "--rerank-mode",
        "disabled",
    ])
    .await;
    let foreign_agent = f.project.join(".codex/agents/foreign.toml");
    fs::write(&foreign_agent, "name = \"foreign\"\n").unwrap();
    let identity = f.project.join(".graphit/project.json");
    let source = coordinator_source.to_str().unwrap();
    f.output(&["init", "--agent", "kiro-v3", "--coordinator-source", source])
        .await;
    let kiro = f.project.join(".kiro/agents");
    assert!(fs::read_dir(&kiro).unwrap().next().is_some());
    let saved_identity = fs::read(&identity).unwrap();
    f.output(&["remove", "--agent", "codex"]).await;
    assert_eq!(fs::read(&identity).unwrap(), saved_identity);
    assert!(foreign_agent.is_file() && kiro.is_dir());
    assert!(!f.project.join(".codex/agents/graphit-scout.toml").exists());
    f.output(&["remove"]).await;
    assert!(!identity.exists());
    assert!(foreign_agent.is_file());
    let repeated = f.output(&["remove"]).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&repeated.stdout).unwrap()["state"],
        "absent"
    );

    f.output(&["init", "--coordinator-source", source]).await;
    let package = f.temp.path().join("package");
    fs::create_dir_all(package.join("graphit-runtime/broker")).unwrap();
    let executable = package.join("graphit");
    if fs::hard_link(env!("CARGO_BIN_EXE_graphit"), &executable).is_err() {
        fs::copy(env!("CARGO_BIN_EXE_graphit"), &executable).unwrap();
    }
    fs::write(
        package.join("graphit-model-server"),
        "owned model executable",
    )
    .unwrap();
    fs::write(package.join("graphit-runtime/build.json"), "{}").unwrap();
    fs::write(
        package.join("graphit-runtime/broker/keep"),
        "separate product",
    )
    .unwrap();
    fs::write(package.join("foreign"), "other program").unwrap();
    let mut files = serde_json::Map::new();
    for relative in [
        "graphit",
        "graphit-model-server",
        "graphit-runtime/build.json",
    ] {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let mut input = fs::File::open(package.join(relative)).unwrap();
        let mut hash = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = input.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        files.insert(
            relative.into(),
            json!(
                hash.finalize()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ),
        );
    }
    fs::write(
        package.join("graphit-runtime/installed-files.json"),
        serde_json::to_vec(&json!({"files":files})).unwrap(),
    )
    .unwrap();
    let template = f.command();
    let packaged = || {
        let mut command = Command::new(&executable);
        command
            .current_dir(&f.project)
            .env_clear()
            .envs(
                template
                    .get_envs()
                    .filter_map(|(key, value)| value.map(|value| (key, value))),
            )
            .arg("--json");
        command
    };
    let registration = packaged()
        .args(["init", "--coordinator-source", source])
        .output()
        .unwrap();
    assert!(
        registration.status.success(),
        "{}",
        String::from_utf8_lossy(&registration.stderr)
    );
    let wrong_root = f.temp.path().join("unregistered-installation");
    let unregistered = packaged()
        .arg("--global-dir")
        .arg(wrong_root)
        .arg("uninstall")
        .output()
        .unwrap();
    assert!(
        unregistered.status.success(),
        "{}",
        String::from_utf8_lossy(&unregistered.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&unregistered.stdout).unwrap()["package"]["state"],
        "unregistered"
    );
    assert!(executable.is_file() && f.paths.config_file().is_file());
    let shared_root = f.temp.path().join("shared-installation");
    for arguments in [
        &["config", "set", "--global", "agent", "codex"][..],
        &["daemon", "start"][..],
    ] {
        let output = packaged()
            .arg("--global-dir")
            .arg(&shared_root)
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let shared = packaged()
        .arg("--global-dir")
        .arg(&shared_root)
        .arg("uninstall")
        .output()
        .unwrap();
    assert!(
        shared.status.success(),
        "{}",
        String::from_utf8_lossy(&shared.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&shared.stdout).unwrap()["package"]["state"],
        "shared"
    );
    assert!(executable.is_file());

    let cache = f.paths.root().join("cache");
    fs::rename(&cache, f.temp.path().join("original-cache")).unwrap();
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join("replacement"), "unrelated").unwrap();
    let uninstall = packaged().arg("uninstall").output().unwrap();
    assert!(
        uninstall.status.success(),
        "{} {}",
        String::from_utf8_lossy(&uninstall.stderr),
        String::from_utf8_lossy(&uninstall.stdout)
    );
    let receipt: Value = serde_json::from_slice(&uninstall.stdout).unwrap();
    assert_eq!(receipt["package"]["state"], "removed");
    assert!(!executable.exists());
    assert!(!f.paths.config_file().exists());
    assert_eq!(fs::read_to_string(foreign_model).unwrap(), "unrelated");
    assert_eq!(
        fs::read_to_string(cache.join("replacement")).unwrap(),
        "unrelated"
    );
    assert!(identity.is_file() && foreign_agent.is_file());
    assert!(package.join("graphit-runtime/broker/keep").is_file());
    assert!(package.join("foreign").is_file());
}
