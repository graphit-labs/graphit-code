mod support;

use graphit_core::file_lock::Cancellation;
use graphit_core::git_checkout::GitCheckout;
use graphit_core::version_context::{ScopedPin, VersionContext, VersionSource};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use support::Fixture;

fn git(directory: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    command.arg("-C").arg(directory).args(args);
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    let output = command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}

fn init(root: &Path, name: &str, format: &str) -> PathBuf {
    let repo = root.join(name);
    fs::create_dir(&repo).unwrap();
    git(
        &repo,
        &[
            "init",
            "--initial-branch=main",
            &format!("--ref-format={format}"),
        ],
    );
    repo
}

fn commit(repo: &Path) {
    git(
        repo,
        &[
            "-c",
            "user.name=Graphit Fixture",
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

fn inspect(repo: &Path) -> GitCheckout {
    GitCheckout::inspect(repo, Duration::from_secs(5), &Cancellation::default()).unwrap()
}

fn branch(repo: &Path, name: &str, default: &str) {
    assert_eq!(
        inspect(repo),
        GitCheckout::Branch {
            name: name.into(),
            default: default.into()
        }
    );
}

#[test]
fn loose_packed_defaults_and_detached_selection() {
    let fixture = Fixture::new();
    let repo = init(fixture.root(), "repo com espaço", "files");
    branch(&repo, "main", "main");
    git(&repo, &["symbolic-ref", "HEAD", "refs/heads/fresh"]);
    branch(&repo, "fresh", "fresh");
    commit(&repo);
    branch(&repo, "fresh", "main");
    git(&repo, &["branch", "master"]);
    branch(&repo, "fresh", "master");
    git(&repo, &["branch", "main"]);
    git(&repo, &["checkout", "-b", "feature/ação"]);
    branch(&repo, "feature/ação", "main");
    let sub = repo.join("nested");
    fs::create_dir(&sub).unwrap();
    branch(&sub, "feature/ação", "main");
    git(&repo, &["pack-refs", "--all"]);
    branch(&sub, "feature/ação", "main");
    // Origin HEAD may name an as-yet-unfetched branch; this is still a default selector.
    git(
        &repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/release/next",
        ],
    );
    branch(&repo, "feature/ação", "release/next");
    git(&repo, &["checkout", "--detach"]);
    let facts = inspect(&repo);
    assert_eq!(
        facts,
        GitCheckout::Detached {
            commit: git(&repo, &["rev-parse", "HEAD"]),
            default: "release/next".into()
        }
    );
    let pin = ScopedPin::new(&fixture.discover(&repo).unwrap(), "pinned").unwrap();
    let context = VersionContext::select(
        &fixture.discover(&repo).unwrap(),
        facts.as_checkout(),
        "",
        Some(&pin),
        "refs/heads/base",
    )
    .unwrap();
    assert_eq!(
        (context.version(), context.source(), context.is_detached()),
        ("pinned", VersionSource::Pin, true)
    );
    let context = VersionContext::select(
        &fixture.discover(&repo).unwrap(),
        facts.as_checkout(),
        "explicit",
        Some(&pin),
        "base",
    )
    .unwrap();
    assert_eq!(context.version(), "explicit");
    let context = VersionContext::select(
        &fixture.discover(&repo).unwrap(),
        facts.as_checkout(),
        "",
        None,
        "refs/heads/base",
    )
    .unwrap();
    assert_eq!(
        context.version(),
        format!("git:{}", git(&repo, &["rev-parse", "HEAD"]))
    );
}

#[test]
fn linked_worktree_uses_own_head_and_common_refs() {
    let fixture = Fixture::new();
    let repo = init(fixture.root(), "main", "files");
    commit(&repo);
    let worktree = fixture.path("linked");
    let worktree_argument = worktree.as_path();
    #[cfg(windows)]
    let worktree_argument = dunce::simplified(worktree_argument);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "topic",
            worktree_argument.to_str().unwrap(),
        ],
    );
    git(
        &repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    branch(&repo, "main", "main");
    branch(&worktree, "topic", "main");
    let pin = ScopedPin::new(&fixture.discover(&repo).unwrap(), "initial-pin").unwrap();
    let facts = inspect(&worktree);
    let selected = VersionContext::select(
        &fixture.discover(&worktree).unwrap(),
        facts.as_checkout(),
        "",
        Some(&pin),
        "",
    )
    .unwrap();
    assert_eq!(selected.version(), "topic");
}

#[test]
fn submodule_and_separate_git_directory() {
    let fixture = Fixture::new();
    let source = init(fixture.root(), "source", "files");
    commit(&source);
    let parent = init(fixture.root(), "parent", "files");
    commit(&parent);
    let source_argument = source.as_path();
    #[cfg(windows)]
    let source_argument = dunce::simplified(source_argument);
    git(
        &parent,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            source_argument.to_str().unwrap(),
            "sub",
        ],
    );
    let sub = parent.join("sub");
    assert!(fs::metadata(sub.join(".git")).unwrap().is_file());
    git(&sub, &["checkout", "-b", "sub-topic"]);
    branch(&sub, "sub-topic", "main");
    let separate = fixture.path("separate");
    fs::create_dir(&separate).unwrap();
    let gitdir = fixture.path("external.git");
    let gitdir_argument = gitdir.as_path();
    #[cfg(windows)]
    let gitdir_argument = dunce::simplified(gitdir_argument);
    git(
        &separate,
        &[
            "init",
            "--initial-branch=outside",
            "--separate-git-dir",
            gitdir_argument.to_str().unwrap(),
        ],
    );
    branch(&separate, "outside", "outside");
}

#[test]
fn reftable_unborn_attached_detached_and_worktree() {
    let fixture = Fixture::new();
    let repo = init(fixture.root(), "table", "reftable");
    git(&repo, &["symbolic-ref", "HEAD", "refs/heads/fresh"]);
    branch(&repo, "fresh", "fresh");
    commit(&repo);
    git(&repo, &["branch", "master"]);
    branch(&repo, "fresh", "master");
    git(&repo, &["branch", "main"]);
    branch(&repo, "fresh", "main");
    let worktree = fixture.path("table-linked");
    let worktree_argument = worktree.as_path();
    #[cfg(windows)]
    let worktree_argument = dunce::simplified(worktree_argument);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "table/topic",
            worktree_argument.to_str().unwrap(),
        ],
    );
    git(
        &repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/release",
        ],
    );
    branch(&repo, "fresh", "release");
    branch(&worktree, "table/topic", "release");
    git(&worktree, &["checkout", "--detach"]);
    assert_eq!(
        inspect(&worktree),
        GitCheckout::Detached {
            commit: git(&worktree, &["rev-parse", "HEAD"]),
            default: "release".into()
        }
    );
}

#[test]
fn invalid_inputs_and_corrupt_repository_fail_closed() {
    let fixture = Fixture::new();
    assert_eq!(inspect(fixture.root()), GitCheckout::NonGit);
    let file = fixture.path("file");
    fs::write(&file, "value").unwrap();
    assert_eq!(
        GitCheckout::inspect(&file, Duration::from_secs(1), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        GitCheckout::inspect(
            fixture.path("missing"),
            Duration::from_secs(1),
            &Cancellation::default()
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::NotFound
    );
    let repo = init(fixture.root(), "corrupt", "files");
    commit(&repo);
    git(&repo, &["checkout", "-b", "topic"]);
    fs::write(repo.join("payload"), "a blob, not a commit").unwrap();
    let blob = git(&repo, &["hash-object", "-w", "payload"]);
    fs::write(repo.join(".git/refs/heads/topic"), format!("{blob}\n")).unwrap();
    assert_eq!(
        GitCheckout::inspect(&repo, Duration::from_secs(1), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::write(repo.join(".git/refs/heads/topic"), "not-an-object-id\n").unwrap();
    assert_eq!(
        GitCheckout::inspect(&repo, Duration::from_secs(1), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::write(repo.join(".git/config"), "[broken").unwrap();
    assert_eq!(
        GitCheckout::inspect(&repo, Duration::from_secs(1), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::write(
        repo.join(".git/config"),
        "[core]\nrepositoryformatversion = 0\nbare = false\n",
    )
    .unwrap();
    fs::write(repo.join(".git/HEAD"), "nonsense").unwrap();
    assert_eq!(
        GitCheckout::inspect(&repo, Duration::from_secs(1), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    let marker = fixture.path("bad-marker");
    fs::create_dir(&marker).unwrap();
    fs::write(marker.join(".git"), "gitdir: nonexistent").unwrap();
    assert_eq!(
        GitCheckout::inspect(&marker, Duration::from_secs(1), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn sha256_repository_is_read_by_git() {
    let fixture = Fixture::new();
    let sha = fixture.path("sha");
    fs::create_dir(&sha).unwrap();
    git(
        &sha,
        &[
            "init",
            "--object-format=sha256",
            "--ref-format=reftable",
            "--initial-branch=main",
        ],
    );
    commit(&sha);
    git(&sha, &["checkout", "-b", "hash/topic"]);
    branch(&sha, "hash/topic", "main");
}

#[test]
fn corrupt_inner_boundary_cannot_inherit_outer_checkout() {
    let fixture = Fixture::new();
    let outer = init(fixture.root(), "outer", "files");
    commit(&outer);
    let inner = outer.join("inner");
    fs::create_dir_all(inner.join(".git")).unwrap();
    assert_eq!(
        GitCheckout::inspect(&inner, Duration::from_secs(1), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn missing_promisor_commit_fails_without_fetch_or_writes() {
    let fixture = Fixture::new();
    let repo = init(fixture.root(), "promisor", "files");
    commit(&repo);
    let oid = git(&repo, &["rev-parse", "HEAD"]);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    git(
        &repo,
        &[
            "config",
            "remote.origin.url",
            &format!("http://{}/missing.git", listener.local_addr().unwrap()),
        ],
    );
    git(&repo, &["config", "remote.origin.promisor", "true"]);
    git(&repo, &["config", "extensions.partialClone", "origin"]);
    let object = repo.join(".git/objects").join(&oid[..2]).join(&oid[2..]);
    fs::remove_file(object).unwrap();
    let before = snapshot(&repo);
    assert_eq!(
        GitCheckout::inspect(&repo, Duration::from_secs(1), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert!(matches!(listener.accept(), Err(e) if e.kind() == io::ErrorKind::WouldBlock));
    assert_eq!(snapshot(&repo), before);
    // HEAD can become detached without asking Git to load the missing commit.
    fs::write(repo.join(".git/HEAD"), format!("{oid}\n")).unwrap();
    let before = snapshot(&repo);
    assert_eq!(
        GitCheckout::inspect(&repo, Duration::from_secs(1), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert!(matches!(listener.accept(), Err(e) if e.kind() == io::ErrorKind::WouldBlock));
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn concurrent_worktrees_keep_independent_call_contexts() {
    let fixture = Fixture::new();
    let repo = init(fixture.root(), "main", "reftable");
    commit(&repo);
    let other = fixture.path("other");
    let other_argument = other.as_path();
    #[cfg(windows)]
    let other_argument = dunce::simplified(other_argument);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "other/topic",
            other_argument.to_str().unwrap(),
        ],
    );
    std::thread::scope(|scope| {
        for (path, expected) in [(&repo, "main"), (&other, "other/topic")] {
            let project = fixture.discover(path).unwrap();
            scope.spawn(move || {
                for _ in 0..10 {
                    let facts = inspect(path);
                    let context =
                        VersionContext::select(&project, facts.as_checkout(), "", None, "")
                            .unwrap();
                    assert_eq!(context.version(), expected);
                    let explicit = VersionContext::select(
                        &project,
                        facts.as_checkout(),
                        "call-only",
                        None,
                        "",
                    )
                    .unwrap();
                    assert_eq!(explicit.version(), "call-only");
                }
            });
        }
    });
    branch(&repo, "main", "main");
    branch(&other, "other/topic", "main");
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut result = Vec::new();
    fn visit(root: &Path, directory: &Path, result: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, result);
            } else {
                result.push((
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                ));
            }
        }
    }
    visit(root, root, &mut result);
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

#[test]
fn invocation_scope_is_isolated_and_reads_do_not_write() {
    let fixture = Fixture::new();
    let target = init(fixture.root(), "target", "reftable");
    commit(&target);
    git(&target, &["checkout", "-b", "target/topic"]);
    let other = init(fixture.root(), "other", "files");
    let before = snapshot(fixture.root());
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "scope_child", "--nocapture"])
        .env("GRAPHIT_TEST_TARGET", &target)
        .env("GRAPHIT_TEST_NONGIT", fixture.root())
        .env("GIT_DIR", other.join(".git"))
        .env("GIT_WORK_TREE", &other)
        .env("GIT_COMMON_DIR", other.join(".git"))
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.repositoryFormatVersion")
        .env("GIT_CONFIG_VALUE_0", "999")
        .env(
            "GIT_CONFIG_PARAMETERS",
            "'core.repositoryFormatVersion=999'",
        )
        .env("GIT_NAMESPACE", "wrong")
        .env("GIT_CEILING_DIRECTORIES", &target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(snapshot(fixture.root()), before);
}

#[test]
fn scope_child() {
    let Some(target) = std::env::var_os("GRAPHIT_TEST_TARGET") else {
        return;
    };
    branch(Path::new(&target), "target/topic", "main");
    let nongit = std::env::var_os("GRAPHIT_TEST_NONGIT").unwrap();
    assert_eq!(inspect(Path::new(&nongit)), GitCheckout::NonGit);
}

#[test]
fn detached_commit_is_disjoint_from_real_git_branch_names() {
    let fixture = Fixture::new();
    let repo = init(fixture.root(), "collision", "files");
    commit(&repo);
    let oid = git(&repo, &["rev-parse", "HEAD"]);
    let branch = format!("git/commit/{oid}");
    git(&repo, &["checkout", "-b", &branch]);
    let project = fixture.discover(&repo).unwrap();
    let attached_facts = inspect(&repo);
    let attached =
        VersionContext::select(&project, attached_facts.as_checkout(), "", None, "").unwrap();
    assert_eq!(attached.version(), branch);
    git(&repo, &["checkout", "--detach", &oid]);
    let detached_facts = inspect(&repo);
    let detached =
        VersionContext::select(&project, detached_facts.as_checkout(), "", None, "").unwrap();
    assert_eq!(detached.version(), format!("git:{oid}"));
    assert_ne!(attached.version(), detached.version());
    assert_ne!(attached.lance_branch(), detached.lance_branch());
    assert!(detached.lance_branch().starts_with("h-"));
    assert!(!detached.lance_branch().contains(':'));
    // This logical namespace cannot coincide with any branch accepted by Git.
    let result = Command::new("git")
        .args([
            "check-ref-format",
            &format!("refs/heads/{}", detached.version()),
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
}
