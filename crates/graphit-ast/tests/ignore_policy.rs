use graphit_ast::ignore_policy::{AstIgnorePolicy, AstIgnorePolicyError};
use graphit_core::{
    brand::Brand,
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
    file_io::SnapshotContext,
    file_lock::Cancellation,
    ignore_rules::IgnoreScope,
};
use serde_json::{Value, json};
use std::{
    error::Error,
    ffi::OsString,
    fs, io,
    path::Path,
    time::{Duration, Instant},
};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}

fn snapshot(brand: &Brand, settings: &[(&str, Value)]) -> io::Result<ConfigSnapshot> {
    let entries = settings
        .iter()
        .map(|(key, value)| Ok((ConfigKey::new(key)?, value.clone())))
        .collect::<io::Result<Vec<_>>>()?;
    ConfigSnapshot::new(
        brand,
        [ConfigDocument::new(brand, ConfigSource::Project, entries)?],
    )
}

fn policy(settings: &[(&str, Value)]) -> Result<AstIgnorePolicy, Box<dyn Error>> {
    Ok(AstIgnorePolicy::from_config(
        &snapshot(&Brand::default(), settings)?,
        &context(),
    )?)
}

fn load_scope(root: &Path, policy: &AstIgnorePolicy) -> io::Result<IgnoreScope> {
    let defaults: Vec<&str> = policy.defaults().iter().map(String::as_str).collect();
    IgnoreScope::load(root, "", Some(policy.filename()), &defaults)
}

#[test]
fn defaults_protect_git_brand_and_only_root_docs() -> TestResult {
    let temp = TempDir::new()?;
    let policy = policy(&[])?;
    assert_eq!(policy.filename(), ".astignore");
    assert_eq!(policy.defaults(), &[".git/", ".graphit/", "/docs/"]);
    let scope = load_scope(temp.path(), &policy)?;
    for name in [
        ".git/refs/heads/main",
        "nested/.git/index",
        ".graphit/runtime/state.json",
        "nested/.graphit/cache.json",
        "docs/schema.proto",
    ] {
        assert!(scope.is_ignored(name, false)?, "{name}");
    }
    for name in [
        "graphit.lock.json",
        "fixtures/graphit.lock.json",
        "internal/x/docs/schema.proto",
        "src/lib.rs",
        ".github/actions/build.rs",
        "README.md",
    ] {
        assert!(!scope.is_ignored(name, false)?, "{name}");
    }
    let brand = Brand::new("product-x")?;
    let custom = AstIgnorePolicy::from_config(&snapshot(&brand, &[])?, &context())?;
    assert_eq!(custom.defaults(), &[".git/", ".product-x/", "/docs/"]);
    let custom_scope = load_scope(temp.path(), &custom)?;
    assert!(custom_scope.is_ignored(".git/index", false)?);
    assert!(custom_scope.is_ignored(".product-x/runtime/a.json", false)?);
    assert!(!custom_scope.is_ignored("product-x.lock.json", false)?);
    assert!(!custom_scope.is_ignored("fixtures/product-x.lock.json", false)?);
    assert!(!custom_scope.is_ignored(".graphit/runtime/a.json", false)?);
    Ok(())
}

#[test]
fn git_metadata_mutation_does_not_invalidate_selected_source() -> TestResult {
    use graphit_core::{
        artifact_store::{
            CapturedPublicationScope, CapturedSourceReader, ProjectIgnoreCriteria,
            ProjectSelectionCriteria, ProjectSelectionLimits, ScopeLimits, SourceReadLimits,
        },
        project_paths::{InstallationPaths, ProjectPaths},
    };
    use std::sync::Arc;

    let temp = TempDir::new()?;
    let root = temp.path().join("project");
    fs::create_dir_all(root.join(".graphit"))?;
    fs::create_dir(root.join(".git"))?;
    fs::write(root.join("a.rs"), "fn original() {}")?;
    fs::write(root.join(".git/refs.json"), "{\"revision\":1}")?;
    fs::write(root.join(".astignore"), "!.git/\n!.git/refs.json\n")?;
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("global")),
    )?;
    let project = ProjectPaths::at(&root, &installation)?;
    let original = Arc::new(context());
    let scope = CapturedPublicationScope::project(
        &project,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse()?,
        ScopeLimits::new(1, 4096, 64)?,
        &original,
    )?;
    let ignore = policy(&[])?;
    let defaults: Vec<_> = ignore.defaults().iter().map(String::as_str).collect();
    let reader = CapturedSourceReader::capture_selected_project_metadata(
        &scope,
        ProjectSelectionCriteria {
            subtree: Some(Path::new(".")),
            extras: &[],
            include_readme: false,
            extensions: &[".rs", ".json"],
            ignore: ProjectIgnoreCriteria {
                protected_defaults: &defaults,
                custom_control: Some(ignore.filename()),
                git_control: Some(".gitignore"),
            },
        },
        ProjectSelectionLimits {
            max_depth: 16,
            max_input_items: 64,
            max_input_bytes: 65536,
            max_observed_path_bytes: 4096,
            max_selected_file_bytes: 65536,
            max_control_file_bytes: 65536,
        },
        SourceReadLimits::new(65536, 4096, 65536)?,
        &original,
    )?;
    assert!(
        reader
            .nodes()
            .any(|node| node.relative() == Path::new("a.rs"))
    );
    assert!(
        !reader
            .nodes()
            .any(|node| node.relative().starts_with(".git"))
    );
    fs::write(root.join(".git/refs.json"), "{\"revision\":2}")?;
    fs::write(root.join(".git/index.lock"), "temporary Git state")?;
    reader.revalidate_content()?;
    fs::write(root.join("a.rs"), "fn changed() {}")?;
    assert_eq!(
        reader.revalidate_content().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    Ok(())
}

#[test]
fn index_docs_reads_only_its_actual_winning_boolean() -> TestResult {
    let temp = TempDir::new()?;
    let policy = policy(&[
        ("ast.index_docs", json!(true)),
        ("knowledge.docs_dir", json!([1, 2])),
    ])?;
    assert_eq!(policy.defaults(), &[".git/", ".graphit/"]);
    let scope = load_scope(temp.path(), &policy)?;
    assert!(!scope.is_ignored("docs/schema.proto", false)?);
    assert!(scope.is_ignored(".graphit/runtime/state.json", false)?);
    assert!(!scope.is_ignored("graphit.lock.json", false)?);
    Ok(())
}

#[test]
fn admitted_lexical_aliases_remain_anchored_to_the_project() -> TestResult {
    let temp = TempDir::new()?;
    for (configured, expected) in [
        ("./docs", "docs"),
        ("docs//x", "docs/x"),
        ("docs/../x", "x"),
        ("  ./docs  ", "docs"),
        ("api/../docs/./x/", "docs/x"),
    ] {
        let policy = policy(&[("knowledge.docs_dir", json!(configured))])?;
        let scope = load_scope(temp.path(), &policy)?;
        assert!(
            scope.is_ignored(format!("{expected}/schema.json"), false)?,
            "{configured}"
        );
        assert!(
            !scope.is_ignored(format!("nested/{expected}/schema.json"), false)?,
            "{configured}"
        );
    }
    Ok(())
}

#[test]
fn project_root_and_escaping_docs_do_not_empty_the_graph() -> TestResult {
    let temp = TempDir::new()?;
    for configured in [".", "./", "docs/..", "../outside", "docs/../../outside"] {
        let policy = policy(&[("knowledge.docs_dir", json!(configured))])?;
        assert_eq!(policy.defaults().len(), 2, "{configured}");
        assert!(!load_scope(temp.path(), &policy)?.is_ignored("src/lib.rs", false)?);
    }
    let absolute = temp.path().join("outside");
    let absolute = absolute.to_str().ok_or("fixture path is not UTF-8")?;
    let policy = policy(&[("knowledge.docs_dir", json!(absolute))])?;
    assert_eq!(policy.defaults().len(), 2);
    Ok(())
}

#[test]
fn literal_glob_names_do_not_widen_the_docs_tree() -> TestResult {
    let temp = TempDir::new()?;
    for (name, lookalike) in [
        ("docs[part]", "docsp"),
        ("docs*literal", "docsXXliteral"),
        ("docs?literal", "docsXliteral"),
        ("docs{a,b}", "docsa"),
        ("#docs!🦀", "docs!🦀"),
    ] {
        // Windows forbids physical '*'/'?' names; the Core matcher can still
        // qualify their literal policy patterns without creating those entries.
        if !cfg!(windows) || !name.contains(['*', '?']) {
            fs::create_dir(temp.path().join(name))?;
            fs::create_dir(temp.path().join(lookalike))?;
        }
        let policy = policy(&[("knowledge.docs_dir", json!(name))])?;
        let scope = load_scope(temp.path(), &policy)?;
        assert!(
            scope.is_ignored(format!("{name}/file.json"), false)?,
            "{name}"
        );
        assert!(
            !scope.is_ignored(format!("{lookalike}/file.json"), false)?,
            "{name}"
        );
        assert!(
            !scope.is_ignored(format!("nested/{name}/file.json"), false)?,
            "{name}"
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn native_backslash_is_literal_in_the_unix_core_matcher() -> TestResult {
    let temp = TempDir::new()?;
    let name = r"docs\literal[🦀]";
    fs::create_dir(temp.path().join(name))?;
    let policy = policy(&[("knowledge.docs_dir", json!(name))])?;
    let scope = load_scope(temp.path(), &policy)?;
    assert!(scope.is_ignored(format!("{name}/schema.json"), false)?);
    assert!(!scope.is_ignored("docsliteral[🦀]/schema.json", false)?);
    assert!(!scope.is_ignored("docs/literal[🦀]/schema.json", false)?);
    assert!(!scope.is_ignored(format!("nested/{name}/schema.json"), false)?);
    Ok(())
}

#[test]
fn custom_rules_cannot_reopen_defaults_but_keep_core_reinclusion() -> TestResult {
    let temp = TempDir::new()?;
    fs::create_dir(temp.path().join("src"))?;
    fs::write(temp.path().join(".gitignore"), "from-git.rs\n")?;
    fs::write(
        temp.path().join(".astignore"),
        "!.git/\n!.git/index\n!docs/\n!.graphit/\nsrc/*\n!src/keep.rs\n!from-git.rs\n",
    )?;
    let policy = policy(&[])?;
    let scope = load_scope(temp.path(), &policy)?;
    assert!(!scope.should_descend(".git")?);
    assert!(scope.is_ignored(".git/index", false)?);
    assert!(scope.is_ignored("docs/schema.proto", false)?);
    assert!(scope.is_ignored(".graphit/runtime/state.json", false)?);
    assert!(!scope.is_ignored("graphit.lock.json", false)?);
    assert!(!scope.is_ignored("from-git.rs", false)?);
    assert!(scope.should_descend("src")?);
    let child = scope.child("src")?;
    assert!(child.is_ignored("src/other.rs", false)?);
    assert!(!child.is_ignored("src/keep.rs", false)?);
    Ok(())
}

#[test]
fn wrong_winner_types_do_not_fall_back_to_defaults_or_lower_layers() -> TestResult {
    let brand = Brand::default();
    let key = ConfigKey::new("ast.index_docs")?;
    let global = ConfigDocument::new(&brand, ConfigSource::Global, [(key.clone(), json!(true))])?;
    let project = ConfigDocument::new(&brand, ConfigSource::Project, [(key, json!("false"))])?;
    let captured = ConfigSnapshot::new(&brand, [global, project])?;
    assert!(matches!(
        AstIgnorePolicy::from_config(&captured, &context()),
        Err(AstIgnorePolicyError::Setting {
            key: "ast.index_docs",
            ..
        })
    ));
    for value in [json!(12), json!(["docs"]), json!({"path": "docs"})] {
        assert!(matches!(
            policy(&[("knowledge.docs_dir", value)])
                .unwrap_err()
                .downcast_ref::<AstIgnorePolicyError>(),
            Some(AstIgnorePolicyError::Setting {
                key: "knowledge.docs_dir",
                ..
            })
        ));
    }
    assert!(
        ConfigDocument::new(
            &brand,
            ConfigSource::Project,
            [(ConfigKey::new("ast.index_docs")?, Value::Null)]
        )
        .is_err()
    );
    Ok(())
}

fn environment(brand: &Brand, values: &[(&str, &str)]) -> io::Result<ConfigSnapshot> {
    let keys: Vec<ConfigKey> = values
        .iter()
        .map(|(key, _)| ConfigKey::new(key))
        .collect::<io::Result<_>>()?;
    let variables: Vec<(OsString, OsString)> = keys
        .iter()
        .zip(values)
        .map(|(key, (_, value))| (key.environment_variable(brand).into(), (*value).into()))
        .collect();
    ConfigSnapshot::new(
        brand,
        [ConfigSnapshot::capture_environment(
            brand, &keys, variables,
        )?],
    )
}

#[test]
fn owned_environment_bool_and_literal_string_have_distinct_null_semantics() -> TestResult {
    let brand = Brand::default();
    let boolean = environment(&brand, &[("ast.index_docs", "true")])?;
    assert_eq!(
        AstIgnorePolicy::from_config(&boolean, &context())?
            .defaults()
            .len(),
        2
    );
    let null_boolean = environment(&brand, &[("ast.index_docs", "null")])?;
    assert!(matches!(
        AstIgnorePolicy::from_config(&null_boolean, &context()),
        Err(AstIgnorePolicyError::Setting {
            key: "ast.index_docs",
            ..
        })
    ));
    let null_string = environment(&brand, &[("knowledge.docs_dir", "null")])?;
    let policy = AstIgnorePolicy::from_config(&null_string, &context())?;
    assert_eq!(policy.defaults().last().unwrap(), "/null/");
    Ok(())
}

#[test]
fn fresh_configurations_change_policy_without_mutating_retained_data() -> TestResult {
    let old = policy(&[])?;
    let changed = policy(&[("knowledge.docs_dir", json!("api[🦀]"))])?;
    let enabled = policy(&[("ast.index_docs", json!(true))])?;
    assert_eq!(old.defaults().last().unwrap(), "/docs/");
    assert_ne!(changed.defaults(), old.defaults());
    assert_eq!(enabled.defaults().len(), 2);
    let temp = TempDir::new()?;
    assert!(load_scope(temp.path(), &changed)?.is_ignored("api[🦀]/schema.proto", false)?);
    assert!(!load_scope(temp.path(), &changed)?.is_ignored("docs/schema.proto", false)?);
    assert!(load_scope(temp.path(), &old)?.is_ignored("docs/schema.proto", false)?);
    Ok(())
}

#[test]
fn invalid_docs_and_genuine_original_failures_remain_errors() -> TestResult {
    for directory in ["", "   ", "docs\0x", "docs\nx", "docs\rx"] {
        assert!(matches!(
            policy(&[("knowledge.docs_dir", json!(directory))])
                .unwrap_err()
                .downcast_ref::<AstIgnorePolicyError>(),
            Some(AstIgnorePolicyError::DocsDirectory(_))
        ));
    }
    let captured = snapshot(&Brand::default(), &[("ast.index_docs", json!("wrong"))])?;
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = SnapshotContext::service(cancellation);
    match AstIgnorePolicy::from_config(&captured, &cancelled) {
        Err(AstIgnorePolicyError::Operation(error)) => {
            assert_eq!(error.kind(), io::ErrorKind::Interrupted)
        }
        other => panic!("unexpected cancelled operation: {other:?}"),
    }
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    match AstIgnorePolicy::from_config(&captured, &expired) {
        Err(AstIgnorePolicyError::Operation(error)) => {
            assert_eq!(error.kind(), io::ErrorKind::TimedOut)
        }
        other => panic!("unexpected expired operation: {other:?}"),
    }
    assert_eq!(policy(&[])?.defaults().len(), 3);
    Ok(())
}
