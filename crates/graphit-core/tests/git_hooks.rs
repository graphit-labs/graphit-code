mod support;

use graphit_core::brand::Brand;
use graphit_core::file_lock::{Cancellation, FileLock};
use graphit_core::git_hooks::{Event, Failure, Options, Registration, Registry, Stage};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use support::Fixture;

fn command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).args(args);
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    command.env("GIT_CONFIG_NOSYSTEM", "1").env(
        "GIT_CONFIG_GLOBAL",
        if cfg!(windows) { "NUL" } else { "/dev/null" },
    );
    command
}
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = command(root, args).output().unwrap();
    assert!(
        output.status.success(),
        "Git fixture {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn init(root: &Path) {
    git(root, &["init", "--initial-branch=main"]);
}
fn options() -> Options {
    Options {
        timeout: Duration::from_secs(10),
        max_command_bytes: 256,
        max_snapshot_bytes: 32768,
    }
}
fn registry(root: &Path, brand: &str) -> Registry {
    Registry::capture(
        root,
        Brand::new(brand).unwrap(),
        options(),
        &Cancellation::default(),
    )
    .unwrap()
}
fn registration(event: Event, command: &str) -> Registration<'_> {
    Registration { event, command }
}
fn get(root: &Path, key: &str) -> Vec<u8> {
    git(
        root,
        &["config", "get", "--local", "--no-includes", "--null", key],
    )
}
fn set(root: &Path, key: &str, value: &str) {
    git(
        root,
        &["config", "set", "--local", "--all", "--", key, value],
    );
}

#[test]
fn all_events_reconcile_idempotently_and_remove_only_owned_fields() {
    let fixture = Fixture::new();
    let root = fixture.root();
    init(root);
    for event in Event::ALL {
        assert_eq!(event.as_str().parse::<Event>().unwrap(), event);
    }
    for name in ["", "Pre-Commit", "../pre-commit", "pre-commit\n", "unknown"] {
        assert!(name.parse::<Event>().is_err());
    }
    set(root, "hook.graphit--pre-commit.description", "preserved");
    set(root, "hook.graphit--unknown.command", "preserved unknown");
    set(root, "hook.third-pre-commit.command", "third");
    set(root, "hook.third-pre-commit.event", "pre-commit");
    let manager = registry(root, "graphit");
    let cancel = Cancellation::default();
    assert!(!root.join(".git/.hooks-manager.lock").exists());
    let desired: Vec<_> = Event::ALL
        .into_iter()
        .map(|e| registration(e, "printf 'quotes \\\" UTF8 ç'\nprintf second"))
        .collect();
    let report = manager.reconcile(&desired, &cancel).unwrap();
    assert_eq!(report.selected, Event::ALL);
    assert_eq!(report.changed, Event::ALL);
    assert_eq!(report.git_config_writes, 112);
    let path = root.join(".git/config");
    let contents = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let report = manager.reconcile(&desired, &cancel).unwrap();
    assert_eq!(report.git_config_writes, 0);
    assert!(report.changed.is_empty());
    assert!(report.removed.is_empty());
    assert_eq!(fs::read(&path).unwrap(), contents);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    let other = registry(root, "other-brand");
    other
        .reconcile(
            &[registration(Event::PrePush, "--literal-command")],
            &cancel,
        )
        .unwrap();
    assert_eq!(
        get(root, "hook.other-brand--pre-push.command"),
        b"--literal-command\0"
    );
    git(
        root,
        &[
            "config",
            "set",
            "--local",
            "--append",
            "hook.graphit--pre-commit.enabled",
            "true",
        ],
    );
    let report = manager
        .reconcile(&[registration(Event::PreCommit, "new command")], &cancel)
        .unwrap();
    assert_eq!(report.changed, [Event::PreCommit]);
    assert_eq!(report.removed.len(), 27);
    assert_eq!(report.git_config_writes, 4 + 27 * 3);
    assert_eq!(get(root, "hook.graphit--pre-commit.enabled"), b"true\0");
    assert_eq!(
        get(root, "hook.graphit--pre-commit.description"),
        b"preserved\0"
    );
    assert_eq!(
        get(root, "hook.graphit--unknown.command"),
        b"preserved unknown\0"
    );
    assert_eq!(get(root, "hook.third-pre-commit.command"), b"third\0");
    assert_eq!(
        get(root, "hook.other-brand--pre-push.command"),
        b"--literal-command\0"
    );
    assert_eq!(
        manager.reconcile(&[], &cancel).unwrap().git_config_writes,
        3
    );
    assert_eq!(
        manager.reconcile(&[], &cancel).unwrap().git_config_writes,
        0
    );
}

#[test]
fn bare_repository_registry_and_git_shell_preserve_native_context() {
    let fixture = Fixture::new();
    let root = fixture.root().join("bare com espaços ç");
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "--bare"]);
    let manager = registry(&root, "graphit");
    manager
        .reconcile(
            &[registration(Event::PreReceive, "exit 0")],
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(
        get(&root, "hook.graphit--pre-receive.event"),
        b"pre-receive\0"
    );
    let shell = graphit_core::git_hooks::shell_path(
        &root,
        Duration::from_secs(5),
        &Cancellation::default(),
    )
    .unwrap();
    assert!(shell.is_absolute() && shell.is_file());
    let output = command(&root, &["hook", "run", "pre-receive"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn invalid_inputs_and_precancellation_create_no_auxiliary_state() {
    let fixture = Fixture::new();
    let root = fixture.root();
    init(root);
    let brand = Brand::default();
    for opts in [
        Options {
            timeout: Duration::ZERO,
            ..options()
        },
        Options {
            timeout: Duration::MAX,
            ..options()
        },
        Options {
            max_command_bytes: 0,
            ..options()
        },
        Options {
            max_command_bytes: usize::MAX,
            ..options()
        },
        Options {
            max_snapshot_bytes: 1,
            ..options()
        },
        Options {
            max_snapshot_bytes: usize::MAX,
            ..options()
        },
    ] {
        assert_eq!(
            Registry::capture(root, brand.clone(), opts, &Cancellation::default())
                .unwrap_err()
                .failure,
            Failure::InvalidOptions
        );
    }
    let manager = registry(root, "graphit");
    let contents = fs::read(root.join(".git/config")).unwrap();
    for value in ["", " \n\t", "nul\0payload", &"x".repeat(257)] {
        let error = manager
            .reconcile(
                &[registration(Event::PreCommit, value)],
                &Cancellation::default(),
            )
            .unwrap_err();
        assert_eq!(error.failure, Failure::InvalidRegistration);
        assert!(!error.may_have_changed);
    }
    let duplicate = [
        registration(Event::PreCommit, "one"),
        registration(Event::PreCommit, "two"),
    ];
    assert_eq!(
        manager
            .reconcile(&duplicate, &Cancellation::default())
            .unwrap_err()
            .failure,
        Failure::InvalidRegistration
    );
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(matches!(
        manager.reconcile(&[], &cancel).unwrap_err().failure,
        Failure::Io {
            kind: std::io::ErrorKind::Interrupted,
            ..
        }
    ));
    assert!(Registry::capture(root, brand, options(), &cancel).is_err());
    assert!(!root.join(".git/.hooks-manager.lock").exists());
    assert_eq!(fs::read(root.join(".git/config")).unwrap(), contents);
    let secret = registration(Event::PreCommit, "private-command-marker");
    assert!(!format!("{secret:?}").contains("private-command-marker"));
    assert!(!format!("{manager:?}").contains(&root.display().to_string()));
}

#[test]
fn brand_and_event_pairs_have_unambiguous_independent_namespaces() {
    let fixture = Fixture::new();
    let root = fixture.root();
    init(root);
    let cancel = Cancellation::default();
    let first = registry(root, "graphit");
    for (brand, left, right) in [
        ("graphit-post", Event::PostUpdate, Event::Update),
        ("graphit-prepare", Event::PrepareCommitMsg, Event::CommitMsg),
    ] {
        let second = registry(root, brand);
        let left_desired = [registration(left, "left command")];
        let right_desired = [registration(right, "right command")];
        first.reconcile(&left_desired, &cancel).unwrap();
        second.reconcile(&right_desired, &cancel).unwrap();
        assert_eq!(
            first
                .reconcile(&left_desired, &cancel)
                .unwrap()
                .git_config_writes,
            0
        );
        assert_eq!(
            second
                .reconcile(&right_desired, &cancel)
                .unwrap()
                .git_config_writes,
            0
        );
        second.reconcile(&[], &cancel).unwrap();
        assert_eq!(
            first
                .reconcile(&left_desired, &cancel)
                .unwrap()
                .git_config_writes,
            0
        );
        second.reconcile(&right_desired, &cancel).unwrap();
        first.reconcile(&[], &cancel).unwrap();
        assert_eq!(
            second
                .reconcile(&right_desired, &cancel)
                .unwrap()
                .git_config_writes,
            0
        );
        second.reconcile(&[], &cancel).unwrap();
    }
    let inner = registry(root, "graphit--post");
    inner
        .reconcile(
            &[registration(Event::Update, "inner delimiter brand")],
            &cancel,
        )
        .unwrap();
    first
        .reconcile(&[registration(Event::PostUpdate, "base brand")], &cancel)
        .unwrap();
    first.reconcile(&[], &cancel).unwrap();
    assert_eq!(
        inner
            .reconcile(
                &[registration(Event::Update, "inner delimiter brand")],
                &cancel
            )
            .unwrap()
            .git_config_writes,
        0
    );
}

#[test]
fn capture_requires_an_explicit_working_tree_root() {
    let fixture = Fixture::new();
    let root = fixture.root();
    assert!(
        Registry::capture(root, Brand::default(), options(), &Cancellation::default()).is_err()
    );
    init(root);
    let nested = root.join("nested");
    fs::create_dir(&nested).unwrap();
    assert_eq!(
        Registry::capture(
            &nested,
            Brand::default(),
            options(),
            &Cancellation::default()
        )
        .unwrap_err()
        .failure,
        Failure::InvalidCheckout
    );
    assert!(!root.join(".git/.hooks-manager.lock").exists());
}

fn commit(root: &Path) {
    git(
        root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    );
}

#[test]
fn worktrees_and_submodules_are_resolved_by_git_without_changing_other_scopes() {
    let fixture = Fixture::new();
    let root = fixture.root();
    init(root);
    commit(root);
    let linked = root.join("linked");
    git(
        root,
        &["worktree", "add", "-b", "linked", linked.to_str().unwrap()],
    );
    let include = root.join("include.config");
    fs::write(
        &include,
        "[hook \"included\"]\ncommand = third party\nevent = pre-commit\n",
    )
    .unwrap();
    let global = root.join("global.config");
    fs::write(
        &global,
        "[hook \"global\"]\ncommand = global third\nevent = pre-commit\n",
    )
    .unwrap();
    set(root, "include.path", include.to_str().unwrap());
    set(root, "extensions.worktreeConfig", "true");
    git(
        &linked,
        &[
            "config",
            "set",
            "--worktree",
            "hook.worktree.command",
            "worktree third",
        ],
    );
    let worktree_config = native_path(&git(
        &linked,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config.worktree",
        ],
    ));
    let original = fs::read(&worktree_config).unwrap();
    let global_before = fs::read(&global).unwrap();
    let include_before = fs::read(&include).unwrap();
    let manager = registry(&linked, "graphit");
    let cancel = Cancellation::default();
    manager
        .reconcile(
            &[registration(Event::PreCommit, "trusted command")],
            &cancel,
        )
        .unwrap();
    assert_eq!(
        get(root, "hook.graphit--pre-commit.command"),
        b"trusted command\0"
    );
    assert_eq!(
        registry(root, "graphit")
            .reconcile(
                &[registration(Event::PreCommit, "trusted command")],
                &cancel
            )
            .unwrap()
            .git_config_writes,
        0
    );
    assert!(root.join(".git/.hooks-manager.lock").is_file());
    assert!(
        !native_path(&git(&linked, &["rev-parse", "--absolute-git-dir"]))
            .join(".hooks-manager.lock")
            .exists()
    );
    assert_eq!(fs::read(worktree_config).unwrap(), original);
    assert_eq!(fs::read(global).unwrap(), global_before);
    assert_eq!(fs::read(&include).unwrap(), include_before);
    assert_eq!(
        get(root, "include.path"),
        format!("{}\0", include.display()).as_bytes()
    );
    let sub_origin = root.join("origin");
    fs::create_dir(&sub_origin).unwrap();
    init(&sub_origin);
    commit(&sub_origin);
    git(
        root,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            sub_origin.to_str().unwrap(),
            "module",
        ],
    );
    let sub = root.join("module");
    registry(&sub, "graphit")
        .reconcile(
            &[registration(Event::PrePush, "submodule command")],
            &cancel,
        )
        .unwrap();
    assert_eq!(
        get(&sub, "hook.graphit--pre-push.command"),
        b"submodule command\0"
    );
}
fn native_path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .trim_end_matches(['\r', '\n']),
    )
}

#[test]
fn busy_git_config_and_malformed_config_are_errors_without_blind_retry() {
    let fixture = Fixture::new();
    let root = fixture.root();
    init(root);
    let manager = registry(root, "graphit");
    let path = root.join(".git/config");
    let original = fs::read(&path).unwrap();
    let busy = root.join(".git/config.lock");
    fs::write(&busy, b"other owner").unwrap();
    let error = manager
        .reconcile(
            &[registration(Event::PreCommit, "private-command")],
            &Cancellation::default(),
        )
        .unwrap_err();
    assert_eq!(error.stage, Stage::Write);
    assert!(error.may_have_changed);
    assert!(matches!(error.failure, Failure::Git { .. }));
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read(&busy).unwrap(), b"other owner");
    assert!(!format!("{error:?}{error}").contains("private-command"));
    fs::remove_file(busy).unwrap();
    manager
        .reconcile(
            &[registration(Event::PreCommit, "recovered")],
            &Cancellation::default(),
        )
        .unwrap();
    fs::write(&path, b"[broken\n").unwrap();
    let error = manager
        .reconcile(&[], &Cancellation::default())
        .unwrap_err();
    assert_eq!(error.stage, Stage::Read);
    assert!(!error.may_have_changed);
    assert_eq!(fs::read(path).unwrap(), b"[broken\n");
    assert!(!FileLock::is_locked(root.join(".git/.hooks-manager.lock")).unwrap());
}

#[test]
fn shared_lock_deadline_cancellation_and_concurrent_managers() {
    let fixture = Fixture::new();
    let root = fixture.root();
    init(root);
    let lock_path = root.join(".git/.hooks-manager.lock");
    let held = FileLock::try_acquire(&lock_path).unwrap();
    let short = Registry::capture(
        root,
        Brand::default(),
        Options {
            timeout: Duration::from_millis(150),
            ..options()
        },
        &Cancellation::default(),
    )
    .unwrap();
    let start = Instant::now();
    let error = short.reconcile(&[], &Cancellation::default()).unwrap_err();
    assert_eq!(error.stage, Stage::Lock);
    assert!(matches!(
        error.failure,
        Failure::Io {
            kind: std::io::ErrorKind::TimedOut,
            ..
        }
    ));
    assert!(start.elapsed() < Duration::from_secs(2));
    let manager = registry(root, "graphit");
    let cancel = Cancellation::default();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            cancel.cancel();
        });
        let error = manager.reconcile(&[], &cancel).unwrap_err();
        assert_eq!(error.stage, Stage::Lock);
        assert!(!error.may_have_changed);
        assert!(matches!(
            error.failure,
            Failure::Io {
                kind: std::io::ErrorKind::Interrupted,
                ..
            }
        ));
    });
    assert!(FileLock::is_locked(&lock_path).unwrap());
    held.release().unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = ["first", "second"]
            .into_iter()
            .map(|value| {
                let barrier = barrier.clone();
                let manager = manager.clone();
                scope.spawn(move || {
                    barrier.wait();
                    manager.reconcile(
                        &[
                            registration(Event::PreCommit, value),
                            registration(Event::PostCheckout, value),
                        ],
                        &Cancellation::default(),
                    )
                })
            })
            .collect();
        barrier.wait();
        handles
            .into_iter()
            .map(|h| h.join().unwrap().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.len(), 2);
    let first = get(root, "hook.graphit--pre-commit.command");
    let second = get(root, "hook.graphit--post-checkout.command");
    assert_eq!(first, second);
    assert!(first == b"first\0" || first == b"second\0");
    assert!(!FileLock::is_locked(lock_path).unwrap());
}

#[cfg(unix)]
fn executable(path: &Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o750)).unwrap();
}
#[cfg(unix)]
fn stamp(path: &Path) -> (Vec<u8>, std::time::SystemTime, u32) {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path).unwrap();
    (
        fs::read(path).unwrap(),
        metadata.modified().unwrap(),
        metadata.permissions().mode(),
    )
}

#[cfg(unix)]
#[test]
fn git_executes_registered_commands_and_user_hooks_with_arguments_and_failure() {
    let fixture = Fixture::new();
    let root = fixture.root();
    init(root);
    let scripts = root.join("custom hooks");
    fs::create_dir(&scripts).unwrap();
    set(root, "core.hooksPath", scripts.to_str().unwrap());
    let third = scripts.join("pre-commit");
    executable(
        &third,
        "#!/bin/sh\nprintf 'third\\n' >> calls\nprintf '%s\\n' \"$@\" >> third-args\n",
    );
    let binary = scripts.join("non-shell");
    fs::write(&binary, b"\x7fELF user bytes").unwrap();
    std::os::unix::fs::symlink(&binary, scripts.join("user-link")).unwrap();
    let third_stamp = stamp(&third);
    let binary_stamp = stamp(&binary);
    let link = fs::read_link(scripts.join("user-link")).unwrap();
    let helper = root.join("owned helper");
    executable(
        &helper,
        "#!/bin/sh\nprintf 'configured\\n' >> calls\nprintf '%s\\n' \"$@\" >> configured-args\n",
    );
    let manager = registry(root, "graphit");
    let cancel = Cancellation::default();
    manager
        .reconcile(
            &[registration(Event::PreCommit, "'./owned helper'")],
            &cancel,
        )
        .unwrap();
    let output = command(
        root,
        &[
            "hook",
            "run",
            "pre-commit",
            "--",
            "space arg",
            "quote'arg",
            "ç",
        ],
    )
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(root.join("calls")).unwrap(),
        b"configured\nthird\n"
    );
    assert_eq!(
        fs::read(root.join("configured-args")).unwrap(),
        "space arg\nquote'arg\nç\n".as_bytes()
    );
    assert_eq!(
        fs::read(root.join("third-args")).unwrap(),
        "space arg\nquote'arg\nç\n".as_bytes()
    );
    manager
        .reconcile(&[registration(Event::PreCommit, "exit 7")], &cancel)
        .unwrap();
    assert!(
        !command(root, &["hook", "run", "pre-commit"])
            .output()
            .unwrap()
            .status
            .success()
    );
    manager
        .reconcile(
            &[registration(
                Event::PreCommit,
                "graphit-fixture-missing-program",
            )],
            &cancel,
        )
        .unwrap();
    assert!(
        !command(root, &["hook", "run", "pre-commit"])
            .output()
            .unwrap()
            .status
            .success()
    );
    manager.reconcile(&[], &cancel).unwrap();
    assert_eq!(stamp(&third), third_stamp);
    assert_eq!(stamp(&binary), binary_stamp);
    assert_eq!(fs::read_link(scripts.join("user-link")).unwrap(), link);
    assert_eq!(
        get(root, "core.hooksPath"),
        format!("{}\0", scripts.display()).as_bytes()
    );
}

#[cfg(unix)]
#[test]
fn unicode_newline_roots_and_observed_symlink_lock() {
    let fixture = Fixture::new();
    let root = fixture.root().join("native-é\nroot");
    fs::create_dir(&root).unwrap();
    init(&root);
    let manager = registry(&root, "graphit");
    let victim = root.join("victim");
    fs::write(&victim, b"unchanged").unwrap();
    std::os::unix::fs::symlink(&victim, root.join(".git/.hooks-manager.lock")).unwrap();
    let error = manager
        .reconcile(&[], &Cancellation::default())
        .unwrap_err();
    assert_eq!(error.stage, Stage::Lock);
    assert!(!error.may_have_changed);
    assert_eq!(fs::read(victim).unwrap(), b"unchanged");
}

#[test]
fn routing_child() {
    let Some(root) = std::env::var_os("GRAPHIT_TEST_REGISTRY_ROOT") else {
        return;
    };
    registry(Path::new(&root), "graphit")
        .reconcile(
            &[registration(Event::PreCommit, "child trusted command")],
            &Cancellation::default(),
        )
        .unwrap();
}

#[test]
fn inherited_git_routing_cannot_redirect_registry_or_modify_global_configuration() {
    let fixture = Fixture::new();
    let root = fixture.root();
    init(root);
    let wrong = root.join("wrong");
    fs::create_dir(&wrong).unwrap();
    init(&wrong);
    let global = root.join("active-global.config");
    fs::write(
        &global,
        "[hook \"global-third\"]\ncommand = global command\nevent = pre-commit\n",
    )
    .unwrap();
    let global_before = fs::read(&global).unwrap();
    let wrong_before = fs::read(wrong.join(".git/config")).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "routing_child", "--nocapture"])
        .env("GRAPHIT_TEST_REGISTRY_ROOT", root)
        .env("GIT_DIR", wrong.join(".git"))
        .env("GIT_WORK_TREE", &wrong)
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "hook.graphit--pre-commit.command")
        .env("GIT_CONFIG_VALUE_0", "injected command")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        get(root, "hook.graphit--pre-commit.command"),
        b"child trusted command\0"
    );
    assert_eq!(fs::read(wrong.join(".git/config")).unwrap(), wrong_before);
    assert_eq!(fs::read(&global).unwrap(), global_before);
    let output = command(root, &["config", "get", "hook.global-third.command"])
        .env("GIT_CONFIG_GLOBAL", global)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"global command\n");
}

#[cfg(unix)]
#[test]
fn configuration_directory_permission_failure_is_visible_when_enforced() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let root = fixture.root();
    init(root);
    let manager = registry(root, "graphit");
    let directory = root.join(".git");
    let original_mode = fs::metadata(&directory).unwrap().permissions();
    let original = fs::read(directory.join("config")).unwrap();
    struct Restore(PathBuf, fs::Permissions);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, self.1.clone());
        }
    }
    // Create the coordination file before preventing config.lock creation.
    FileLock::try_acquire(directory.join(".hooks-manager.lock"))
        .unwrap()
        .release()
        .unwrap();
    let restore = Restore(directory.clone(), original_mode);
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
    let result = manager.reconcile(
        &[registration(Event::PreCommit, "trusted command")],
        &Cancellation::default(),
    );
    drop(restore);
    match result {
        Err(error) => {
            assert_eq!(error.stage, Stage::Write);
            assert!(error.may_have_changed);
            assert!(matches!(error.failure, Failure::Git { .. }));
            assert_eq!(fs::read(directory.join("config")).unwrap(), original);
            println!("configuration permission failure exercised=true");
        }
        Ok(_) => println!(
            "configuration permission failure exercised=false (privileged filesystem bypass)"
        ),
    }
    assert!(!FileLock::is_locked(directory.join(".hooks-manager.lock")).unwrap());
}

#[test]
fn selected_events_preserve_other_owned_and_user_hooks_during_install_and_remove() {
    let fixture = Fixture::new();
    let root = fixture.root();
    init(root);
    let cancel = Cancellation::default();
    let manager = registry(root, "graphit");
    manager
        .reconcile(
            &[
                registration(Event::PrePush, "existing quality gate"),
                registration(Event::PreCommit, "existing validation"),
                registration(Event::PostCheckout, "old checkout"),
            ],
            &cancel,
        )
        .unwrap();
    set(root, "hook.user-hook.command", "user command");
    set(root, "hook.user-hook.event", "post-commit");
    set(
        root,
        "hook.graphit--post-checkout.description",
        "user description",
    );
    let events = [
        Event::PostCheckout,
        Event::PostCommit,
        Event::PostMerge,
        Event::PostRewrite,
    ];
    let desired: Vec<_> = events
        .iter()
        .copied()
        .map(|event| registration(event, "graphit records"))
        .collect();
    let report = manager
        .reconcile_events(&events, &desired, &cancel)
        .unwrap();
    assert_eq!(report.changed, events);
    assert!(report.removed.is_empty());
    assert_eq!(report.git_config_writes, 16);
    assert_eq!(
        get(root, "hook.graphit--pre-push.command"),
        b"existing quality gate\0"
    );
    assert_eq!(
        get(root, "hook.graphit--pre-commit.command"),
        b"existing validation\0"
    );
    let before = fs::read(root.join(".git/config")).unwrap();
    assert_eq!(
        manager
            .reconcile_events(&events, &desired, &cancel)
            .unwrap()
            .git_config_writes,
        0
    );
    assert_eq!(fs::read(root.join(".git/config")).unwrap(), before);
    let report = manager.reconcile_events(&events, &[], &cancel).unwrap();
    assert_eq!(report.removed, events);
    assert_eq!(report.git_config_writes, 12);
    assert_eq!(
        get(root, "hook.graphit--pre-push.command"),
        b"existing quality gate\0"
    );
    assert_eq!(
        get(root, "hook.graphit--pre-commit.command"),
        b"existing validation\0"
    );
    assert_eq!(get(root, "hook.user-hook.command"), b"user command\0");
    assert_eq!(
        get(root, "hook.graphit--post-checkout.description"),
        b"user description\0"
    );
    assert_eq!(
        manager
            .reconcile_events(&events, &[], &cancel)
            .unwrap()
            .git_config_writes,
        0
    );
    // The original full reconciliation retains its intentional all-events API.
    assert_eq!(
        manager.reconcile(&[], &cancel).unwrap().removed,
        [Event::PreCommit, Event::PrePush]
    );
}

#[test]
fn selected_events_reject_duplicate_scope_and_outside_registrations_without_writes() {
    let fixture = Fixture::new();
    let root = fixture.root();
    init(root);
    let manager = registry(root, "graphit");
    let before = fs::read(root.join(".git/config")).unwrap();
    for (events, desired) in [
        (vec![Event::PostCommit, Event::PostCommit], vec![]),
        (
            vec![Event::PostCommit],
            vec![registration(Event::PrePush, "outside")],
        ),
        (vec![], vec![registration(Event::PostCommit, "outside")]),
    ] {
        let error = manager
            .reconcile_events(&events, &desired, &Cancellation::default())
            .unwrap_err();
        assert_eq!(error.failure, Failure::InvalidRegistration);
        assert_eq!(error.stage, Stage::Validation);
        assert!(!error.may_have_changed);
        assert!(!root.join(".git/.hooks-manager.lock").exists());
        assert_eq!(fs::read(root.join(".git/config")).unwrap(), before);
    }
}
