use graphit_core::version_context::{Checkout, ScopedPin, VersionContext, VersionSource};
use std::fs;
use std::io;
use std::thread;

mod support;
use support::Fixture;

const BRANCH: Checkout<'static> = Checkout::Branch {
    name: "feature/x",
    default: "main",
};

#[test]
fn checkout_facts_and_explicit_selection_remain_independent() {
    let fixture = Fixture::new();
    let plain = VersionContext::select(&fixture.project(), Checkout::NonGit, "", None, "").unwrap();
    let whitespace =
        VersionContext::select(&fixture.project(), Checkout::NonGit, "  ", None, "  ").unwrap();
    assert_eq!(plain, whitespace);
    assert_eq!(plain.version(), "main");
    assert_eq!(plain.default_version(), "main");
    assert_eq!(plain.source(), VersionSource::Default);
    assert!(!plain.is_git());
    assert!(!plain.is_detached());
    assert_eq!(plain.lance_branch(), "");
    let branch = VersionContext::select(&fixture.project(), BRANCH, "", None, "").unwrap();
    assert_eq!(branch.version(), "feature/x");
    assert_eq!(branch.source(), VersionSource::Checkout);
    assert_eq!(branch.lance_branch(), "feature/x");
    let pin = ScopedPin::new(&fixture.project(), " from-env ").unwrap();
    let pinned = VersionContext::select(&fixture.project(), BRANCH, "", Some(&pin), "").unwrap();
    let explicit =
        VersionContext::select(&fixture.project(), BRANCH, " from-call ", Some(&pin), "").unwrap();
    assert_eq!(pinned.version(), "from-env");
    assert_eq!(pinned.source(), VersionSource::Pin);
    assert_eq!(explicit.version(), "from-call");
    assert_eq!(explicit.source(), VersionSource::Explicit);
    assert_eq!(branch.version(), "feature/x");
    assert_eq!(plain.clone(), plain);
}

#[test]
fn detached_commit_is_isolated_from_branch_and_base_hints() {
    let fixture = Fixture::new();
    let commit = "0123456789012345678901234567890123456789";
    let detached = Checkout::Detached {
        commit,
        default: "develop",
    };
    for base in ["", "refs/heads/release/1.2", "refs/heads/refs/heads/topic"] {
        let context = VersionContext::select(&fixture.project(), detached, "", None, base).unwrap();
        assert_eq!(context.version(), format!("git:{commit}"));
        assert_eq!(context.default_version(), "develop");
        assert_eq!(context.source(), VersionSource::DetachedCommit);
        assert!(context.is_git());
        assert!(context.is_detached());
    }
    let context =
        VersionContext::select(&fixture.project(), detached, "chosen", None, "base").unwrap();
    assert_eq!(context.version(), "chosen");
    assert!(context.is_detached());
}

#[test]
fn nearest_scope_isolates_nested_projects_and_other_worktrees() {
    let fixture = Fixture::new();
    let root = fixture.path("root");
    let nested = root.join("pkg/deep");
    let other = fixture.path("other-worktree");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir(&other).unwrap();
    fs::write(root.join(".git"), b"gitdir: shared-root").unwrap();
    fs::write(other.join(".git"), b"gitdir: shared-root").unwrap();
    let pin = ScopedPin::new(&fixture.discover(&nested).unwrap(), "pinned").unwrap();
    let local = VersionContext::select(
        &fixture.discover(&root).unwrap(),
        BRANCH,
        "",
        Some(&pin),
        "",
    )
    .unwrap();
    assert_eq!(local.version(), "pinned");
    assert_eq!(local.scope(), root.canonicalize().unwrap());
    let independent = VersionContext::select(
        &fixture.discover(&other).unwrap(),
        BRANCH,
        "",
        Some(&pin),
        "",
    )
    .unwrap();
    assert_eq!(independent.version(), "feature/x");
    fs::create_dir(root.join("pkg/.graphit")).unwrap();
    fs::write(
        root.join("pkg/.graphit/project.json"),
        br#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","name":"nested","ephemeral":false}"#,
    )
    .unwrap();
    let subproject = VersionContext::select(
        &fixture.discover(&nested).unwrap(),
        BRANCH,
        "",
        Some(&pin),
        "",
    )
    .unwrap();
    assert_eq!(subproject.version(), "feature/x");
    assert_eq!(subproject.scope(), root.join("pkg").canonicalize().unwrap());
    assert!(!root.join("graphit.lock.json").exists());
    assert!(!other.join("graphit.lock.json").exists());
    assert!(!nested.join("graphit.lock.json").exists());
}

#[test]
fn invalid_inputs_fail_before_a_context_is_produced() {
    let fixture = Fixture::new();
    for checkout in [
        Checkout::Branch {
            name: "",
            default: "main",
        },
        Checkout::Branch {
            name: "a\0b",
            default: "main",
        },
        Checkout::Detached {
            commit: "0123456789012345678901234567890123456789",
            default: "",
        },
        Checkout::Detached {
            commit: "HEAD",
            default: "main",
        },
    ] {
        assert_eq!(
            VersionContext::select(&fixture.project(), checkout, "explicit", None, "")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert_eq!(
        VersionContext::select(&fixture.project(), BRANCH, "a\0b", None, "")
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        ScopedPin::new(&fixture.project(), " \t ")
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        ScopedPin::new(&fixture.project(), "a\nb")
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        VersionContext::select(&fixture.project(), BRANCH, "valid", None, "a\0b")
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        fixture.discover(fixture.path("absent")).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    fs::write(fixture.path("file"), b"not a directory").unwrap();
    assert_eq!(
        fixture.discover(fixture.path("file")).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(
        fs::read_dir(fixture.root())
            .unwrap()
            .all(|entry| entry.unwrap().file_name() == "file")
    );
}

#[test]
fn concurrent_calls_for_one_project_do_not_share_a_selection() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<VersionContext>();
    let fixture = Fixture::new();
    let pin = ScopedPin::new(&fixture.project(), "pin").unwrap();
    thread::scope(|scope| {
        let calls: Vec<_> = (0..4)
            .map(|i| {
                scope.spawn({
                    let fixture = &fixture;
                    let pin = &pin;
                    move || {
                        let selected = format!("branch/{i}");
                        let context = VersionContext::select(
                            &fixture.project(),
                            BRANCH,
                            &selected,
                            Some(pin),
                            "",
                        )
                        .unwrap();
                        for _ in 0..50 {
                            assert_eq!(context.version(), selected);
                            thread::yield_now();
                        }
                        context
                    }
                })
            })
            .collect();
        for (i, call) in calls.into_iter().enumerate() {
            assert_eq!(call.join().unwrap().version(), format!("branch/{i}"));
        }
    });
    assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn symlink_aliases_use_the_same_canonical_scope() {
    let fixture = Fixture::new();
    let project = fixture.path("project");
    let alias = fixture.path("alias");
    fs::create_dir(&project).unwrap();
    fs::create_dir(project.join(".graphit")).unwrap();
    fs::write(
        project.join(".graphit/project.json"),
        br#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","name":"project","ephemeral":false}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(&project, &alias).unwrap();
    let pin = ScopedPin::new(&fixture.discover(&alias).unwrap(), "selected").unwrap();
    let context = VersionContext::select(
        &fixture.discover(&project).unwrap(),
        BRANCH,
        "",
        Some(&pin),
        "",
    )
    .unwrap();
    assert_eq!(context.version(), "selected");
    assert_eq!(context.scope(), project.canonicalize().unwrap());
}
