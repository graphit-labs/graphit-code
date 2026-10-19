mod support;

use graphit_core::ignore_rules::IgnoreScope;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use support::Fixture;

fn write(root: &Path, path: &str, value: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value).unwrap();
}

fn load(root: &Path, start: &str, defaults: &[&str]) -> IgnoreScope {
    IgnoreScope::load(root, start, Some(".astignore"), defaults).unwrap()
}

#[test]
fn syntax_and_custom_overrides_work_without_git() {
    let fixture = Fixture::new();
    write(
        fixture.root(),
        ".gitignore",
        "\u{feff}# comment\n*.tmp\n/build/\nfolder/\n\\#literal\n\\!literal\n",
    );
    write(
        fixture.root(),
        ".astignore",
        "!keep.tmp\n!folder/\nfolder/**\n!folder/**/\n!folder/**/driver.rs\nfile?\n**/*.gen\n",
    );
    let rules = load(fixture.root(), "", &[]);
    for (path, directory, expected) in [
        ("lost.tmp", false, true),
        ("keep.tmp", false, false),
        ("folder/child/driver.rs", false, false),
        ("folder/child/other.rs", false, true),
        ("build/file", false, true),
        ("nested/build/file", false, false),
        ("folder", false, false),
        ("folder", true, false),
        ("#literal", false, true),
        ("!literal", false, true),
        ("file1", false, true),
        ("deep/fichier.gen", false, true),
        ("ação.rs", false, false),
        ("", true, false),
    ] {
        assert_eq!(
            rules.is_ignored(path, directory).unwrap(),
            expected,
            "{path}"
        );
    }
    assert!(rules.should_descend("folder").unwrap());
    assert!(!fixture.root().join(".git").exists());
}

#[test]
fn family_depth_and_defaults_have_one_precedence() {
    let fixture = Fixture::new();
    write(fixture.root(), ".gitignore", "*.rs\nroot-only\n");
    write(
        fixture.root(),
        ".astignore",
        "!allowed.rs\n*.custom\n!/.graphit/**\n",
    );
    write(
        fixture.root(),
        "child/.gitignore",
        "!*.rs\n!root-only\n!*.custom\n",
    );
    write(
        fixture.root(),
        "child/.astignore",
        "blocked.rs\n!*.custom\n!/.graphit/**\n!*.lock\n",
    );
    write(fixture.root(), "child/.graphit/keep", "value");
    fs::create_dir(fixture.root().join("sibling")).unwrap();
    let defaults = ["/.graphit/", "/graphit.lock.json", "*.lock"];
    let root = load(fixture.root(), "", &defaults);
    let child = root.child("child").unwrap();
    let scoped = load(fixture.root(), "child", &defaults);
    for (path, expected) in [
        ("child/allowed.rs", false),
        ("child/blocked.rs", true),
        ("child/other.rs", false),
        ("child/root-only", false),
        ("child/file.custom", false),
        ("child/file.lock", true),
        ("child/.graphit/keep", false),
        ("child/graphit.lock.json", false),
    ] {
        assert_eq!(child.is_ignored(path, false).unwrap(), expected, "{path}");
        assert_eq!(
            scoped.is_ignored(path, false).unwrap(),
            expected,
            "scoped {path}"
        );
    }
    let sibling = root.child("sibling").unwrap();
    assert!(sibling.is_ignored("sibling/other.rs", false).unwrap());
    assert!(sibling.is_ignored("sibling/file.custom", false).unwrap());
    assert!(root.is_ignored(".graphit/output", false).unwrap());
    assert!(!root.should_descend(".graphit").unwrap());
    assert!(root.is_ignored("graphit.lock.json", false).unwrap());
    assert_eq!(
        child
            .is_ignored("sibling/file.rs", false)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn project_boundary_and_configured_docs_defaults_are_local() {
    let fixture = Fixture::new();
    write(fixture.root(), ".gitignore", "*.rs\n");
    let project = fixture.path("project");
    write(&project, "docs/.astignore", "draft.md\n!README.md\n");
    write(&project, ".astignore", "README.md\n!docs/keep.md\n");
    let root = load(&project, "", &["/docs/"]);
    assert!(!root.is_ignored("main.rs", false).unwrap());
    assert!(root.is_ignored("docs/keep.md", false).unwrap());
    assert!(!root.is_ignored("internal/x/docs/keep.md", false).unwrap());
    let docs = load(&project, "docs", &[]); // Consumer's index_docs=true removes its docs default.
    assert!(docs.is_ignored("docs/draft.md", false).unwrap());
    assert!(!docs.is_ignored("docs/README.md", false).unwrap());
    assert!(!root.is_ignored("draft.md", false).unwrap());
    let defaults_for_docs_at_root: [&str; 0] = []; // docs_dir=. must not generate an exclude-all glob.
    assert!(
        !load(&project, "", &defaults_for_docs_at_root)
            .is_ignored("main.rs", false)
            .unwrap()
    );
}

fn walk(scope: &IgnoreScope, accepted: &mut Vec<PathBuf>) {
    let directory = scope.root().join(scope.directory());
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = scope.directory().join(entry.file_name());
        let is_dir = entry.file_type().unwrap().is_dir();
        if is_dir {
            if scope.should_descend(&path).unwrap() {
                walk(&scope.child(entry.file_name()).unwrap(), accepted);
            }
        } else if !scope.is_ignored(&path, false).unwrap() {
            accepted.push(path);
        }
    }
}

#[test]
fn reincluded_children_survive_traversal_and_defaults_prune() {
    let fixture = Fixture::new();
    write(
        fixture.root(),
        ".astignore",
        "generated/**\n!generated/**/\n!generated/**/driver.rs\nignored/\n!unrelated/keep.rs\n",
    );
    for file in [
        "generated/a/driver.rs",
        "generated/a/parser.rs",
        "ignored/other.rs",
        "deps/driver.rs",
        "live.rs",
    ] {
        write(fixture.root(), file, "fixture");
    }
    let rules = load(fixture.root(), "", &["/.astignore", "/deps/"]);
    assert!(rules.should_descend("generated").unwrap());
    assert!(!rules.should_descend("ignored").unwrap());
    assert!(!rules.should_descend("deps").unwrap());
    let mut accepted = Vec::new();
    walk(&rules, &mut accepted);
    accepted.sort();
    assert_eq!(
        accepted,
        vec![
            PathBuf::from("generated/a/driver.rs"),
            PathBuf::from("live.rs")
        ]
    );
    write(fixture.root(), ".astignore", "generated/\n");
    let no_whitelists = load(fixture.root(), "", &[]);
    assert!(!no_whitelists.should_descend("generated").unwrap());
}

#[test]
fn excluded_parents_require_standard_reinclusion_syntax() {
    let fixture = Fixture::new();
    for filename in [".gitignore", ".astignore"] {
        for text in [
            "generated/\n!generated/driver.rs\n",
            "!generated/driver.rs\ngenerated/\n",
        ] {
            write(fixture.root(), filename, text);
            let scope = load(fixture.root(), "", &[]);
            assert!(!scope.should_descend("generated").unwrap());
            assert!(scope.is_ignored("generated/driver.rs", false).unwrap());
            assert!(scope.is_ignored("generated/parser.rs", false).unwrap());
        }
        for text in [
            "generated/*\n!generated/driver.rs\n",
            "generated/\n!generated/\ngenerated/*\n!generated/driver.rs\n",
        ] {
            write(fixture.root(), filename, text);
            let scope = load(fixture.root(), "", &[]);
            assert!(scope.should_descend("generated").unwrap());
            assert!(!scope.is_ignored("generated/driver.rs", false).unwrap());
            assert!(scope.is_ignored("generated/parser.rs", false).unwrap());
        }
        for (text, ignored) in [
            ("generated/driver.rs\n!generated/driver.rs\n", false),
            ("!generated/driver.rs\ngenerated/driver.rs\n", true),
        ] {
            write(fixture.root(), filename, text);
            assert_eq!(
                load(fixture.root(), "", &[])
                    .is_ignored("generated/driver.rs", false)
                    .unwrap(),
                ignored
            );
        }
        fs::remove_file(fixture.path(filename)).unwrap();
    }
}

#[test]
fn unrelated_negations_do_not_reopen_excluded_parents_for_scoped_or_deleted_paths() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.path("target")).unwrap();
    write(fixture.root(), "target/.astignore", "!**/\n!*.rs\n");
    for filename in [".gitignore", ".astignore"] {
        write(fixture.root(), filename, "target/\n!.env.example\n");
        let root = load(fixture.root(), "", &[]);
        let scoped = load(fixture.root(), "target", &[]);
        assert!(!root.should_descend("target").unwrap());
        assert!(!root.is_ignored(".env.example", false).unwrap());
        for rules in [&root, &scoped] {
            assert!(rules.is_ignored("target/deleted.rs", false).unwrap());
            assert!(!rules.should_descend("target/nested").unwrap());
        }
        fs::remove_file(fixture.path(filename)).unwrap();
    }
}

#[test]
fn rule_sources_must_be_regular_files() {
    let fixture = Fixture::new();
    for filename in [".gitignore", ".astignore"] {
        fs::create_dir(fixture.path(filename)).unwrap();
        assert_eq!(
            IgnoreScope::load(fixture.root(), "", Some(".astignore"), &[])
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        fs::remove_dir(fixture.path(filename)).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn symlink_rule_sources_are_rejected_including_dangling_links() {
    let fixture = Fixture::new();
    let root = fixture.path("root");
    fs::create_dir(&root).unwrap();
    write(fixture.root(), "outside/rules", "*.rs\n");
    write(&root, "local-rules", "*.rs\n");
    for filename in [".gitignore", ".astignore"] {
        for target in [
            fixture.path("outside/rules"),
            root.join("local-rules"),
            fixture.path("missing"),
        ] {
            std::os::unix::fs::symlink(target, root.join(filename)).unwrap();
            assert_eq!(
                IgnoreScope::load(&root, "", Some(".astignore"), &[])
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
            fs::remove_file(root.join(filename)).unwrap();
        }
    }
}

#[test]
fn invalid_rules_paths_and_io_fail_without_partial_snapshots() {
    let fixture = Fixture::new();
    for path in ["../outside", "/absolute"] {
        assert_eq!(
            IgnoreScope::load(fixture.root(), path, None, &[])
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    for filename in ["", ".", "../name", "dir/name"] {
        assert_eq!(
            IgnoreScope::load(fixture.root(), "", Some(filename), &[])
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    let rules = load(fixture.root(), "", &[]);
    for path in ["../outside", "/absolute", "nested/../outside"] {
        assert_eq!(
            rules.is_ignored(path, false).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            rules.should_descend(path).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert_eq!(
        rules.child("missing").unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    write(fixture.root(), "file", "value");
    assert_eq!(
        rules.child("file").unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    write(fixture.root(), ".astignore", "valid\n[z-a]\n");
    assert_eq!(
        IgnoreScope::load(fixture.root(), "", Some(".astignore"), &[])
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::write(fixture.path(".astignore"), [0xff]).unwrap();
    assert_eq!(
        IgnoreScope::load(fixture.root(), "", Some(".astignore"), &[])
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::write(fixture.path(".astignore"), vec![b'x'; 256 * 1024 + 1]).unwrap();
    assert_eq!(
        IgnoreScope::load(fixture.root(), "", Some(".astignore"), &[])
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        IgnoreScope::load(fixture.root(), "", None, &["[z-a]"])
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn snapshots_reload_and_clones_allow_concurrent_reads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<IgnoreScope>();
    let fixture = Fixture::new();
    write(fixture.root(), ".astignore", "old.rs\n");
    let old = load(fixture.root(), "", &[]);
    write(fixture.root(), ".astignore", "new.rs\n");
    let new = load(fixture.root(), "", &[]);
    std::thread::scope(|scope| {
        for rules in [old.clone(), new.clone(), old.clone(), new.clone()] {
            scope.spawn(move || {
                for _ in 0..100 {
                    assert_ne!(
                        rules.is_ignored("old.rs", false).unwrap(),
                        rules.is_ignored("new.rs", false).unwrap()
                    );
                }
            });
        }
    });
    assert!(old.is_ignored("old.rs", false).unwrap());
    assert!(!new.is_ignored("old.rs", false).unwrap());
    assert!(new.is_ignored("new.rs", false).unwrap());
    assert!(!fixture.root().join(".git").exists());
}

#[cfg(unix)]
#[test]
fn child_directory_symlinks_are_not_followed() {
    let fixture = Fixture::new();
    let root = fixture.path("root");
    let outside = fixture.path("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    write(&outside, ".astignore", "*.rs\n");
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    let scope = load(&root, "", &[]);
    assert_eq!(
        scope.child("link").unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let alias = fixture.path("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    assert_eq!(load(&alias, "", &[]).root(), scope.root());
}
