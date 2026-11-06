//! Native Git registry -> installed-style CLI dispatcher -> supervised commands.
use crate::finite_cli;
use finite_cli::FiniteOutput;
use graphit_core::{
    brand::Brand,
    file_lock::Cancellation,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::Duration,
};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    global: PathBuf,
    path: std::ffi::OsString,
}
impl Fixture {
    fn new() -> Result<Self> {
        Self::with_bare(false)
    }
    fn with_bare(bare: bool) -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("projeto com espaços çλ");
        fs::create_dir(&root)?;
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
        let installation = InstallationPaths::resolve(
            brand,
            temp.path(),
            None,
            Some(&temp.path().join("global")),
        )?;
        let project = ProjectPaths::at(&root, &installation)?;
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &Cancellation::default())?
            .ensure(&ProjectMetadata::new("git-hook-fixture")?)?;
        let executable = Path::new(env!("CARGO_BIN_EXE_graphit"));
        let path = std::env::join_paths(
            std::iter::once(executable.parent().unwrap().to_owned()).chain(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            )),
        )?;
        let fixture = Self {
            global: installation.root().to_owned(),
            root,
            path,
            _temp: temp,
        };
        fixture.git(if bare {
            &["init", "--bare", "-b", "main"]
        } else {
            &["init", "-b", "main"]
        })?;
        fixture.git(&["config", "user.name", "Hook fixture"])?;
        fixture.git(&["config", "user.email", "hook@example.invalid"])?;
        fixture.settings(json!({}))?;
        Ok(fixture)
    }
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(
            which::which_in(program, Some(&self.path), &self.root)
                .expect("private fixture executable"),
        );
        command
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", &self.path)
            .env("HOME", self._temp.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                self._temp.path().join("absent-git-config"),
            )
            .env("GRAPHIT_GLOBAL_DIR", &self.global)
            .env("GRAPHIT_TEST_EXECUTABLE", std::env::current_exe().unwrap());
        for key in ["SystemRoot", "WINDIR", "TEMP", "TMP", "PATHEXT"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
    }
    fn git(&self, args: &[&str]) -> Result<Output> {
        success(self.command("git").args(args).finite_output()?)
    }
    fn graphit(&self, args: &[&str]) -> Result<Output> {
        success(
            self.command(env!("CARGO_BIN_EXE_graphit"))
                .args(args)
                .finite_output()?,
        )
    }
    fn settings(&self, hooks: Value) -> Result {
        let path = self.root.join(".graphit/project.json");
        let mut document: Value = serde_json::from_slice(&fs::read(&path)?)?;
        document["githooks"] = hooks;
        document["settings"] = json!({"modules.task":false, "modules.memory":false,
            "modules.embedding":false, "modules.ast":false});
        fs::write(path, serde_json::to_vec(&document)?)?;
        Ok(())
    }
    fn hook(&self, event: &str) -> Command {
        let mut command = self.command("git");
        command.args(["hook", "run", event, "--"]);
        command
    }
}
fn success(output: Output) -> Result<Output> {
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "{}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(output)
}
fn fixture_command(step: &str) -> String {
    format!(
        "GRAPHIT_TEST_STEP={step} GRAPHIT_TEST_POSITIONAL_1=\"$1\" GRAPHIT_TEST_POSITIONAL_2=\"$2\" exec \"$GRAPHIT_TEST_EXECUTABLE\" --exact git_hooks_executable::native_hook_fixture --nocapture"
    )
}

#[test]
fn registry_order_live_configuration_failure_and_third_party_hooks() -> Result {
    let f = Fixture::new()?;
    // Configured hooks still run alongside a third party's hook directory and registry.
    let hooks = f.root.join("third-party hooks");
    fs::create_dir(&hooks)?;
    let traditional = hooks.join("pre-commit");
    fs::write(&traditional, "#!/bin/sh\nprintf 'directory\\n' >> order\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&traditional, fs::Permissions::from_mode(0o755))?;
    }
    f.git(&["config", "core.hooksPath", "third-party hooks"])?;
    f.git(&["config", "hook.third-party.event", "pre-commit"])?;
    f.git(&[
        "config",
        "hook.third-party.command",
        "printf 'third-party\\n' >> order",
    ])?;
    f.settings(
        json!({"pre-commit":["printf 'one, literal\\n' >> order", "printf 'two\\n' >> order"]}),
    )?;
    f.graphit(&["git", "sync"])?;
    let key = "hook.graphit--pre-commit.command";
    assert_eq!(
        String::from_utf8(f.git(&["config", "--local", "--get", key])?.stdout)?.trim(),
        "graphit git hook pre-commit --"
    );
    success(f.hook("pre-commit").finite_output()?)?;
    assert_eq!(
        fs::read_to_string(f.root.join("order"))?,
        "third-party\none, literal\ntwo\ndirectory\n"
    );
    f.settings(json!({"pre-commit":["printf 'updated\\n' >> order"]}))?;
    success(f.hook("pre-commit").finite_output()?)?;
    assert!(
        fs::read_to_string(f.root.join("order"))?.ends_with("third-party\nupdated\ndirectory\n")
    );
    // Updating commands needs no installation and never puts the commands in Git's registry.
    assert_eq!(
        f.git(&["config", "--local", "--get", key])?.stdout,
        b"graphit git hook pre-commit --\n"
    );
    f.settings(json!({"pre-commit":["printf 'before failure\\n' >> order", "exit 17", "printf 'wrong' > after-failure"]}))?;
    let rejected = f.hook("pre-commit").finite_output()?;
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("command 2 failed"));
    assert!(!f.root.join("after-failure").exists());
    let direct = f
        .command(env!("CARGO_BIN_EXE_graphit"))
        .args(["git", "hook", "pre-commit"])
        .finite_output()?;
    assert_eq!(direct.status.code(), Some(17));
    fs::write(f.root.join("tracked"), "payload\n")?;
    f.git(&["add", "tracked"])?;
    let blocked = f
        .command("git")
        .args(["commit", "-m", "must be rejected"])
        .finite_output()?;
    assert!(!blocked.status.success());
    assert!(
        !f.command("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .finite_output()?
            .status
            .success()
    );
    f.settings(json!({"post-commit":["exit 23"]}))?;
    f.graphit(&["git", "sync"])?;
    f.git(&["commit", "-m", "post-hook cannot undo this commit"])?;
    assert!(
        f.command("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .finite_output()?
            .status
            .success()
    );
    assert_eq!(
        f.git(&["config", "--local", "--get", "core.hooksPath"])?
            .stdout,
        b"third-party hooks\n"
    );
    f.settings(json!({}))?;
    f.graphit(&["git", "sync"])?;
    assert!(
        !f.command("git")
            .args([
                "config",
                "--local",
                "--get",
                "hook.graphit--post-commit.event"
            ])
            .finite_output()?
            .status
            .success()
    );
    assert_eq!(
        f.git(&["config", "--local", "--get", "hook.third-party.event"])?
            .stdout,
        b"pre-commit\n"
    );
    assert_eq!(
        fs::read_to_string(traditional)?,
        "#!/bin/sh\nprintf 'directory\\n' >> order\n"
    );
    assert!(!f.global.join("daemon").exists()); // no daemon/auth/model initialization for quality.
    Ok(())
}

#[test]
fn pre_push_preserves_arguments_and_replays_stdin_without_a_push() -> Result {
    let f = Fixture::new()?;
    f.settings(json!({"pre-push":[fixture_command("stdin-one"), fixture_command("stdin-two")]}))?;
    f.graphit(&["git", "install"])?;
    assert!(
        !f.command("git")
            .args(["config", "--get", "core.hooksPath"])
            .finite_output()?
            .status
            .success()
    );
    let payload = b"refs/heads/main abc refs/heads/main def\n\0bytes, preserve\n";
    let path = f.root.join("stdin payload ç");
    fs::write(&path, payload)?;
    let args = ["--global-dir", "remote with spaces, quotes \" and ç"];
    let mut command = f.command("git");
    success(
        command
            .args(["hook", "run", "--to-stdin"])
            .arg(&path)
            .args(["pre-push", "--"])
            .args(args)
            .finite_output()?,
    )?;
    for step in ["stdin-one", "stdin-two"] {
        assert_eq!(fs::read(f.root.join(step))?, payload);
        let data: Value = serde_json::from_slice(&fs::read(f.root.join(format!("{step}.json")))?)?;
        assert_eq!(data["event"], "pre-push");
        assert_eq!(data["count"], "2");
        assert_eq!(data["arguments"], json!(args));
        assert_eq!(
            fs::canonicalize(data["cwd"].as_str().unwrap())?,
            fs::canonicalize(&f.root)?
        );
        assert_eq!(data["positional"], json!(args));
    }
    Ok(())
}

#[test]
fn bare_receive_preserves_git_directory_cwd_and_payload() -> Result {
    let f = Fixture::with_bare(true)?;
    f.settings(json!({"pre-receive":[fixture_command("receive")]}))?;
    f.graphit(&["git", "sync"])?;
    let input = f.root.join("receive-input");
    let bytes = b"old new refs/heads/main\n";
    fs::write(&input, bytes)?;
    success(
        f.command("git")
            .args(["hook", "run", "--to-stdin"])
            .arg(&input)
            .args(["pre-receive"])
            .finite_output()?,
    )?;
    let context: Value = serde_json::from_slice(&fs::read(f.root.join("receive-context.json"))?)?;
    assert_eq!(
        fs::canonicalize(context["cwd"].as_str().unwrap())?,
        fs::canonicalize(&f.root)?
    );
    assert_eq!(context["event"], "pre-receive");
    assert_eq!(fs::read(f.root.join("receive-output"))?, bytes);
    Ok(())
}

async fn wait_child(child: &mut tokio::process::Child) -> Result<std::process::ExitStatus> {
    match tokio::time::timeout(Duration::from_secs(8), child.wait()).await {
        Ok(status) => Ok(status?),
        Err(_) => {
            child.kill().await?;
            Err(io::Error::new(io::ErrorKind::TimedOut, "hook child did not join").into())
        }
    }
}

#[test]
fn output_arrives_before_the_hook_completes() -> Result {
    let f = Fixture::new()?;
    f.settings(json!({"pre-commit":[fixture_command("stream")]}))?;
    f.graphit(&["git", "sync"])?;
    finite_cli::runtime()?.block_on(async {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut child = tokio::process::Command::from(f.hook("pre-commit"))
            .env("GRAPHIT_TEST_SOCKET", listener.local_addr()?.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        // Git may route hook stdout to stderr; both streams precede release.
        async fn reader(
            input: impl tokio::io::AsyncRead + Unpin,
            tx: tokio::sync::mpsc::UnboundedSender<String>,
        ) -> io::Result<String> {
            let mut lines = tokio::io::BufReader::new(input).lines();
            let mut text = String::new();
            while let Some(line) = lines.next_line().await? {
                for marker in [
                    "stream-visible-before-release",
                    "stderr-visible-before-release",
                ] {
                    if line.ends_with(marker) {
                        let _ = tx.send(marker.to_owned());
                    }
                }
                text.push_str(&line);
                text.push('\n');
            }
            Ok(text)
        }
        let out = tokio::spawn(reader(child.stdout.take().unwrap(), tx.clone()));
        let err = tokio::spawn(reader(child.stderr.take().unwrap(), tx));
        let (mut release, _) =
            tokio::time::timeout(Duration::from_secs(8), listener.accept()).await??;
        let first = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;
        let second = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;
        release.write_all(b"continue").await?;
        drop(release);
        let status = wait_child(&mut child).await?;
        let text = tokio::time::timeout(Duration::from_secs(8), out).await???
            + &tokio::time::timeout(Duration::from_secs(8), err).await???;
        assert!(status.success());
        let observed = std::collections::BTreeSet::from([
            first?.ok_or("first stream marker missing")?,
            second?.ok_or("second stream marker missing")?,
        ]);
        assert_eq!(
            observed,
            std::collections::BTreeSet::from([
                "stream-visible-before-release".to_owned(),
                "stderr-visible-before-release".to_owned(),
            ])
        );
        assert!(text.contains("stream-visible-before-release"));
        Ok(())
    })
}

#[cfg(unix)]
#[test]
fn cancellation_joins_the_command_and_its_descendant() -> Result {
    let f = Fixture::new()?;
    f.settings(json!({"pre-commit":[fixture_command("tree"), "printf wrong > after-cancel"]}))?;
    f.graphit(&["git", "sync"])?;
    finite_cli::runtime()?.block_on(async {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut git = tokio::process::Command::from(f.hook("pre-commit"))
            .env("GRAPHIT_TEST_SOCKET", listener.local_addr()?.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut sockets = Vec::new();
        let mut dispatcher = None;
        for _ in 0..2 {
            let (socket, _) =
                tokio::time::timeout(Duration::from_secs(8), listener.accept()).await??;
            let mut reader = tokio::io::BufReader::new(socket);
            let mut line = String::new();
            tokio::time::timeout(Duration::from_secs(8), reader.read_line(&mut line)).await??;
            if let Some(pid) = line.trim().strip_prefix("dispatcher=") {
                dispatcher = Some(pid.parse::<i32>()?);
            }
            sockets.push(reader.into_inner());
        }
        let dispatcher =
            rustix::process::Pid::from_raw(dispatcher.ok_or("missing dispatcher PID")?)
                .ok_or("invalid dispatcher PID")?;
        rustix::process::kill_process(dispatcher, rustix::process::Signal::INT)?;
        for socket in &mut sockets {
            let mut byte = [0];
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(8), socket.read(&mut byte)).await??,
                0
            );
        }
        assert!(!wait_child(&mut git).await?.success());
        assert!(!f.root.join("after-cancel").exists());
        Ok(())
    })
}

#[cfg(target_os = "linux")]
#[test]
fn cancellation_does_not_wait_for_stdin_eof() -> Result {
    let f = Fixture::new()?;
    f.settings(json!({"pre-push":["printf wrong > after-cancel"]}))?;
    finite_cli::runtime()?.block_on(async {
        use tokio::io::AsyncWriteExt;
        let mut child = tokio::process::Command::from(f.command(env!("CARGO_BIN_EXE_graphit")))
            .args(["git", "hook", "pre-push", "--", "origin", "remote"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut input = child.stdin.take().unwrap();
        input
            .write_all(b"keep this pipe open, without EOF\n")
            .await?;
        let pid = child.id().unwrap();
        // Observe the actual supervised copy before racing signal setup.
        let helper = tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let children = fs::read_dir(format!("/proc/{pid}/task"))?
                    .filter_map(|entry| entry.ok())
                    .filter_map(|entry| fs::read_to_string(entry.path().join("children")).ok())
                    .collect::<Vec<_>>()
                    .join(" ");
                if let Some(helper) = children.split_whitespace().find(|p| {
                    fs::read(format!("/proc/{p}/cmdline"))
                        .is_ok_and(|v| v.split(|b| *b == 0).any(|a| a == b"_git-hook-input"))
                }) {
                    return Ok::<_, io::Error>(helper.to_owned());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await??;
        let owned =
            rustix::process::Pid::from_raw(i32::try_from(pid)?).ok_or("invalid owned PID")?;
        rustix::process::kill_process(owned, rustix::process::Signal::INT)?;
        let status = wait_child(&mut child).await?;
        assert_eq!(status.code(), Some(130));
        assert!(!Path::new(&format!("/proc/{helper}")).exists());
        assert!(!f.root.join("after-cancel").exists());
        drop(input); // EOF remains withheld until cancellation has actually joined.
        Ok(())
    })
}

#[test]
fn invalid_configuration_is_rejected_before_commands_start() -> Result {
    let f = Fixture::new()?;
    for hooks in [
        json!({"pre-commit":"not CSV, not an array"}),
        json!({"typo":["true"]}),
        json!({"pre-commit":[""]}),
        json!({"pre-commit":["\u{0}"]}),
        json!({"proc-receive":["true","false"]}),
    ] {
        f.settings(hooks)?;
        let result = f
            .command(env!("CARGO_BIN_EXE_graphit"))
            .args(["git", "sync"])
            .finite_output()?;
        assert!(!result.status.success());
        assert!(
            !f.command("git")
                .args([
                    "config",
                    "--local",
                    "--get",
                    "hook.graphit--pre-commit.command"
                ])
                .finite_output()?
                .status
                .success()
        );
    }
    Ok(())
}

/// A real executable child without requiring Python, accounts or provider tools.
#[test]
fn native_hook_fixture() -> Result {
    let Ok(step) = std::env::var("GRAPHIT_TEST_STEP") else {
        return Ok(());
    };
    match step.as_str() {
        "stdin-one" | "stdin-two" => {
            let mut bytes = Vec::new();
            io::stdin().read_to_end(&mut bytes)?;
            fs::write(&step, bytes)?;
            // The shell explicitly opts in to positional arguments in this fixture.
            let args = [
                std::env::var("GRAPHIT_HOOK_ARG_1")?,
                std::env::var("GRAPHIT_HOOK_ARG_2")?,
            ];
            fs::write(
                format!("{step}.json"),
                serde_json::to_vec(&json!({"event":std::env::var("GRAPHIT_HOOK_NAME")?,
                "count":std::env::var("GRAPHIT_HOOK_ARG_COUNT")?,"arguments":args,"positional":[std::env::var("GRAPHIT_TEST_POSITIONAL_1")?,std::env::var("GRAPHIT_TEST_POSITIONAL_2")?],
                "cwd":std::env::current_dir()?.to_str().unwrap()}))?,
            )?;
        }
        "receive" => {
            io::copy(
                &mut io::stdin().lock(),
                &mut fs::File::create("receive-output")?,
            )?;
            fs::write(
                "receive-context.json",
                serde_json::to_vec(&json!({
                    "cwd":std::env::current_dir()?,"event":std::env::var("GRAPHIT_HOOK_NAME")?
                }))?,
            )?;
        }
        "stream" => {
            println!("stream-visible-before-release");
            io::stdout().flush()?;
            eprintln!("stderr-visible-before-release");
            io::stderr().flush()?;
            let mut socket = TcpStream::connect(std::env::var("GRAPHIT_TEST_SOCKET")?)?;
            socket.set_read_timeout(Some(Duration::from_secs(30)))?;
            let mut byte = [0];
            socket.read_exact(&mut byte)?;
        }
        #[cfg(unix)]
        "tree" => {
            let mut child = Command::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    "git_hooks_executable::native_hook_fixture",
                    "--nocapture",
                ])
                .env("GRAPHIT_TEST_STEP", "descendant")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let mut socket = TcpStream::connect(std::env::var("GRAPHIT_TEST_SOCKET")?)?;
            socket.set_read_timeout(Some(Duration::from_secs(30)))?;
            writeln!(
                socket,
                "dispatcher={}",
                rustix::process::getppid()
                    .ok_or("missing native dispatcher parent")?
                    .as_raw_nonzero()
            )?;
            let mut byte = [0];
            socket.read_exact(&mut byte)?;
            let _ = child.kill();
            let _ = child.wait();
        }
        #[cfg(unix)]
        "descendant" => {
            let mut socket = TcpStream::connect(std::env::var("GRAPHIT_TEST_SOCKET")?)?;
            socket.set_read_timeout(Some(Duration::from_secs(30)))?;
            writeln!(socket, "descendant={}", std::process::id())?;
            let mut byte = [0];
            socket.read_exact(&mut byte)?;
        }
        _ => return Err("unknown hook fixture step".into()),
    }
    Ok(())
}
