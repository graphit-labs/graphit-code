use super::*;
use graphit_ast::{catalog::CatalogErrorCode, profiles::ProfileErrorCode};
use graphit_core::{brand::Brand, file_lock::Cancellation};
use serde_json::json;
use std::time::{Duration, Instant};

struct Fixture {
    _temp: tempfile::TempDir,
    installation: InstallationPaths,
    project: ProjectPaths,
    base: ConfigSnapshot,
    policy: HubResourcePolicy,
}

impl Fixture {
    fn new() -> io::Result<Self> {
        let temp = tempfile::tempdir()?;
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )?;
        let root = temp.path().join("project");
        std::fs::create_dir_all(root.join(installation.brand().directory()))?;
        let project = ProjectPaths::at(&root, &installation)?;
        graphit_core::project_identity::ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &Cancellation::default())?
            .ensure(&graphit_core::project_identity::ProjectMetadata::new(
                "profile-fixture",
            )?)?;
        let base = ConfigSnapshot::new(installation.brand(), [])?;
        let policy = HubResourcePolicy::resolve(&base).map_err(err)?;
        Ok(Self {
            _temp: temp,
            installation,
            project,
            base,
            policy,
        })
    }

    fn set(&self, source: ConfigSource, key: &str, value: &str) -> io::Result<()> {
        let store = match source {
            ConfigSource::Global => ConfigStore::global(&self.installation)?,
            ConfigSource::Project => ConfigStore::project(&self.project)?,
            _ => unreachable!("fixture only writes real persistent stores"),
        };
        store.set(
            ConfigKey::new(key)?,
            json!(value),
            Duration::from_secs(5),
            &Cancellation::default(),
        )?;
        Ok(())
    }

    fn config(&self, original: &SnapshotContext) -> io::Result<ConfigSnapshot> {
        project_config(&self.installation, &self.project, &self.base, original)
    }
}

fn write_profile(path: &Path, content: &str) -> io::Result<()> {
    std::fs::create_dir_all(path.parent().expect("fixture profile parent"))?;
    std::fs::write(path, content)
}

#[test]
fn stock_profiles_route_basename_suffix_and_reject_unsupported() -> io::Result<()> {
    let fixture = Fixture::new()?;
    let cancel = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .map_err(err)?;
    let original = request.snapshot();
    let loader = AstProfileLoader::new(fixture.policy.clone(), original)?;
    let loaded = loader.load(
        &fixture.installation,
        &fixture.project,
        &fixture.config(original)?,
        original,
    )?;
    assert!(loaded.allows(Path::new("src/main.rs"), original)?);
    assert!(loaded.allows(Path::new("src/UPPER.RS"), original)?);
    assert!(loaded.allows(Path::new("nested/Dockerfile"), original)?);
    assert!(loaded.allows(Path::new("nested/dockerFILE"), original)?);
    assert!(loaded.allows(Path::new("image.containerfile"), original)?);
    for path in [
        "src/main.mts",
        "src/main.cts",
        "tools/build-direction-source.d.mts",
        "src/types.d.cts",
        "src/main.cjs",
    ] {
        let path = Path::new(path);
        assert!(matches!(
            loaded.resolve(path, original)?,
            Selection::Candidates(_)
        ));
        assert_eq!(loaded.native_scip_compiler(path, original)?, None);
        assert!(loaded.allows(path, original)?);
    }
    assert!(!loaded.allows(Path::new("unmatched.unsupported-fixture"), original)?);
    assert!(loaded.selectors.iter().any(|route| route == ".rs"));
    assert!(loaded.selectors.iter().any(|route| route == "Dockerfile"));
    loaded.revalidate(original)
}

#[test]
fn native_compiler_routes_admit_profile_free_sources_and_preserve_cancellation() -> io::Result<()> {
    let fixture = Fixture::new()?;
    let cancel = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .map_err(err)?;
    let original = request.snapshot();
    let loaded = AstProfileLoader::new(fixture.policy.clone(), original)?.load(
        &fixture.installation,
        &fixture.project,
        &fixture.config(original)?,
        original,
    )?;
    for extension in ["vb", "VB"] {
        let path = PathBuf::from(format!("src/main.{extension}"));
        assert!(matches!(
            loaded.resolve(&path, original)?,
            Selection::Skip(SelectionSkip::NoSelector)
        ));
        assert_eq!(
            loaded.native_scip_compiler(&path, original)?,
            Some(ScipCompiler::Maintained(ScipFamily::Dotnet))
        );
        assert!(loaded.allows(&path, original)?);
        assert!(
            loaded
                .selectors
                .contains(&format!(".{}", extension.to_ascii_lowercase()))
        );
    }
    for extension in ["fs", "fsi", "fsx"] {
        let path = PathBuf::from(format!("src/main.{extension}"));
        assert_eq!(loaded.native_scip_compiler(&path, original)?, None);
        assert!(!loaded.allows(&path, original)?);
        assert!(!loaded.selectors.contains(&format!(".{extension}")));
    }
    assert_eq!(
        loaded.native_scip_compiler(Path::new("src/lib.rs"), original)?,
        None
    );
    assert_eq!(
        loaded.native_scip_compiler(Path::new("unknown.unsupported"), original)?,
        None
    );
    cancel.cancel();
    assert_eq!(
        loaded
            .native_scip_compiler(Path::new("src/main.vb"), original)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(
        loaded
            .allows(Path::new("src/main.vb"), original)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    Ok(())
}

#[test]
fn native_compiler_routes_do_not_bypass_grammar_filters() -> io::Result<()> {
    for (key, value) in [
        ("ast.grammars_whitelist", "rust"),
        ("ast.grammars_blacklist", "rust"),
    ] {
        let fixture = Fixture::new()?;
        fixture.set(ConfigSource::Project, key, value)?;
        let cancel = Cancellation::default();
        let request = fixture
            .policy
            .request_context(Instant::now(), &cancel)
            .map_err(err)?;
        let original = request.snapshot();
        let loaded = AstProfileLoader::new(fixture.policy.clone(), original)?.load(
            &fixture.installation,
            &fixture.project,
            &fixture.config(original)?,
            original,
        )?;
        for path in ["main.vb", "main.fs"] {
            assert_eq!(
                loaded.native_scip_compiler(Path::new(path), original)?,
                None
            );
            assert!(!loaded.allows(Path::new(path), original)?);
        }
        assert!(
            !loaded
                .selectors
                .iter()
                .any(|route| route == ".vb" || route == ".fs")
        );
        if key == "ast.grammars_blacklist" {
            assert!(matches!(
                loaded.resolve(Path::new("lib.rs"), original)?,
                Selection::Skip(SelectionSkip::Filtered)
            ));
            assert!(!loaded.allows(Path::new("lib.rs"), original)?);
            assert_eq!(
                loaded.native_scip_compiler(Path::new("lib.rs"), original)?,
                None
            );
        }
    }
    Ok(())
}

#[test]
fn native_compiler_routes_preserve_custom_and_exclusive_profile_authority() -> io::Result<()> {
    let fixture = Fixture::new()?;
    let profile = fixture
        .project
        .brand_directory()
        .join("ast/queries/rust.yaml");
    write_profile(
        &profile,
        "language: rust\nextensions: [\".vb\", \".custom-rs\"]\n",
    )?;
    let cancel = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .map_err(err)?;
    let original = request.snapshot();
    let loader = AstProfileLoader::new(fixture.policy.clone(), original)?;
    let loaded = loader.load(
        &fixture.installation,
        &fixture.project,
        &fixture.config(original)?,
        original,
    )?;
    assert!(loaded.allows(Path::new("main.vb"), original)?);
    assert_eq!(
        loaded.native_scip_compiler(Path::new("main.vb"), original)?,
        None
    );
    let Selection::Candidates(profiles) = loaded.resolve(Path::new("main.vb"), original)? else {
        panic!("custom syntax route must remain selected");
    };
    assert_eq!(profiles[0].language().value(), "rust");
    assert!(
        !loaded.allows(Path::new("lib.rs"), original)?,
        "replaced stock routes must not return through native admission"
    );
    write_profile(
        &profile,
        "language: rust\nexclusive: true\nextensions: [\".vb\"]\n",
    )?;
    let exclusive = loader.load(
        &fixture.installation,
        &fixture.project,
        &fixture.config(original)?,
        original,
    )?;
    assert!(matches!(
        exclusive.resolve(Path::new("main.vb"), original)?,
        Selection::Skip(SelectionSkip::NoSelector)
    ));
    assert!(!exclusive.allows(Path::new("main.vb"), original)?);
    assert_eq!(
        exclusive.native_scip_compiler(Path::new("main.vb"), original)?,
        None
    );
    fixture.set(ConfigSource::Project, "ast.grammar", "vb=rust")?;
    let explicit = loader.load(
        &fixture.installation,
        &fixture.project,
        &fixture.config(original)?,
        original,
    )?;
    assert!(explicit.allows(Path::new("main.vb"), original)?);
    assert_eq!(
        explicit.native_scip_compiler(Path::new("main.vb"), original)?,
        None
    );
    Ok(())
}

#[test]
fn runtime_user_project_layers_replace_routes_and_preserve_other_stock() -> io::Result<()> {
    let fixture = Fixture::new()?;
    let cancel = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .map_err(err)?;
    let original = request.snapshot();
    let runtime = fixture
        .installation
        .framework_runtime(env!("GRAPHIT_COMPILED_BUILD_VERSION"))?
        .join("ast/queries/rust.yaml");
    write_profile(&runtime, "language: rust\nextensions: [\".runtime-rs\"]\n")?;
    let loader = AstProfileLoader::new(fixture.policy.clone(), original)?;
    let config = fixture.config(original)?;
    let runtime_loaded = loader.load(&fixture.installation, &fixture.project, &config, original)?;
    assert!(runtime_loaded.allows(Path::new("file.runtime-rs"), original)?);
    assert!(!runtime_loaded.allows(Path::new("file.rs"), original)?);
    assert!(runtime_loaded.allows(Path::new("Dockerfile"), original)?);

    let user = fixture.installation.global_path(&["ast", "queries"])?;
    write_profile(
        &user.join("rust.yaml"),
        "language: rust\nmerge: true\nextensions: [\".user-rs\"]\n",
    )?;
    let user_loaded = loader.load(&fixture.installation, &fixture.project, &config, original)?;
    assert!(user_loaded.allows(Path::new("file.user-rs"), original)?);
    assert!(!user_loaded.allows(Path::new("file.runtime-rs"), original)?);

    fixture.set(ConfigSource::Project, "ast.queries_dir", "custom/queries")?;
    write_profile(
        &fixture.project.root().join("custom/queries/rust.yml"),
        "language: rust\nmerge: true\nextensions: [\".project-rs\"]\n",
    )?;
    let project_loaded = loader.load(
        &fixture.installation,
        &fixture.project,
        &fixture.config(original)?,
        original,
    )?;
    assert!(project_loaded.allows(Path::new("file.project-rs"), original)?);
    assert!(!project_loaded.allows(Path::new("file.user-rs"), original)?);
    assert!(project_loaded.allows(Path::new("Dockerfile"), original)?);
    // Earlier catalogs remain immutable when a higher-layer file appears.
    assert!(user_loaded.allows(Path::new("file.user-rs"), original)?);
    runtime_loaded.revalidate(original)?;
    user_loaded.revalidate(original)?;
    project_loaded.revalidate(original)
}

#[test]
fn persisted_allow_deny_aliases_filter_stock_and_project_wins() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fixture.set(ConfigSource::Global, "ast.grammars_whitelist", " go ")?;
    fixture.set(
        ConfigSource::Project,
        "ast.grammars_whitelist",
        " RUST, tree-sitter-go ",
    )?;
    fixture.set(
        ConfigSource::Project,
        "ast.grammars_blacklist",
        " TREE-SITTER-GO ",
    )?;
    let cancel = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .map_err(err)?;
    let original = request.snapshot();
    let config = fixture.config(original)?;
    assert_eq!(
        config.get::<String>(&ConfigKey::new("ast.grammars_whitelist")?)?,
        Some(" RUST, tree-sitter-go ".into())
    );
    let loaded = AstProfileLoader::new(fixture.policy.clone(), original)?.load(
        &fixture.installation,
        &fixture.project,
        &config,
        original,
    )?;
    assert!(loaded.allows(Path::new("file.rs"), original)?);
    assert!(!loaded.allows(Path::new("file.go"), original)?);
    assert!(!loaded.allows(Path::new("Dockerfile"), original)?);
    assert!(loaded.selectors.iter().any(|route| route == ".rs"));
    assert!(!loaded.selectors.iter().any(|route| route == ".go"));
    loaded.revalidate(original)
}

#[test]
fn exclusive_requires_explicit_mapping_and_disabled_mapping_is_causal() -> io::Result<()> {
    let fixture = Fixture::new()?;
    write_profile(
        &fixture
            .project
            .brand_directory()
            .join("ast/queries/optin.yaml"),
        "language: local-optin\nexclusive: true\nextensions: [\".optin\"]\n",
    )?;
    fixture.set(
        ConfigSource::Project,
        "ast.grammars_whitelist",
        "local-optin",
    )?;
    let cancel = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .map_err(err)?;
    let original = request.snapshot();
    let loader = AstProfileLoader::new(fixture.policy.clone(), original)?;
    let automatic = loader.load(
        &fixture.installation,
        &fixture.project,
        &fixture.config(original)?,
        original,
    )?;
    assert!(!automatic.allows(Path::new("file.optin"), original)?);
    assert!(!automatic.selectors.iter().any(|route| route == ".optin"));

    fixture.set(
        ConfigSource::Project,
        "ast.grammar",
        " OPTIN = tree-sitter-local-optin ",
    )?;
    let explicit = loader.load(
        &fixture.installation,
        &fixture.project,
        &fixture.config(original)?,
        original,
    )?;
    assert!(explicit.allows(Path::new("file.OPTIN"), original)?);
    assert!(explicit.selectors.iter().any(|route| route == ".optin"));
    fixture.set(
        ConfigSource::Project,
        "ast.grammars_blacklist",
        "LOCAL-OPTIN",
    )?;
    let error = match loader.load(
        &fixture.installation,
        &fixture.project,
        &fixture.config(original)?,
        original,
    ) {
        Ok(_) => panic!("explicit disabled grammar was accepted"),
        Err(error) => error,
    };
    let catalog_error = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<graphit_ast::catalog::CatalogError>())
        .expect("original catalog diagnostic");
    assert_eq!(catalog_error.code(), CatalogErrorCode::DisabledGrammar);
    Ok(())
}

#[test]
fn invalid_profile_keeps_decode_diagnostic_and_captured_revalidation() -> io::Result<()> {
    let fixture = Fixture::new()?;
    let cancel = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .map_err(err)?;
    let original = request.snapshot();
    let loader = AstProfileLoader::new(fixture.policy.clone(), original)?;
    let invalid = b"language: invalid\nparser: unsupported-parser\n";
    let error = match loader.decode("project/invalid.yaml", invalid, original) {
        Ok(_) => panic!("invalid parser accepted"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    let diagnostic = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<ProfileDecodeFailure>())
        .expect("loader retains original profile diagnostic");
    assert_eq!(diagnostic.profile, "project/invalid.yaml");
    assert_eq!(diagnostic.code, ProfileErrorCode::InvalidField);
    assert_eq!(diagnostic.schema_path, Some("parser"));
    assert!(diagnostic.span.is_some());

    let path = fixture
        .project
        .brand_directory()
        .join("ast/queries/invalid.yaml");
    write_profile(&path, std::str::from_utf8(invalid).unwrap())?;
    let loaded = loader.load(
        &fixture.installation,
        &fixture.project,
        &fixture.config(original)?,
        original,
    )?;
    // The directory reports invalid profiles and preserves valid stock routing.
    assert!(loaded.allows(Path::new("still.rs"), original)?);
    loaded.revalidate(original)?;
    std::fs::write(path, "language: repaired\nextensions: [\".repaired\"]\n")?;
    assert!(loaded.revalidate(original).is_err());
    Ok(())
}

#[test]
fn custom_compilers_override_stock_routes_and_admit_profile_free_extensions() -> io::Result<()> {
    let fixture = Fixture::new()?;
    let cancel = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .map_err(err)?;
    let original = request.snapshot();
    ConfigStore::project(&fixture.project)?.set(
        ConfigKey::new("ast.scip.parsers")?,
        json!(".RS=registry.test/custom@sha256:abc,xyz=registry.test/custom@sha256:abc"),
        Duration::from_secs(5),
        &cancel,
    )?;
    let loaded = AstProfileLoader::new(fixture.policy.clone(), original)?.load(
        &fixture.installation,
        &fixture.project,
        &fixture.config(original)?,
        original,
    )?;
    for path in ["src/main.RS", "src/main.xyz"] {
        assert!(loaded.allows(Path::new(path), original)?);
        assert_eq!(
            loaded.selected_scip_compiler(Path::new(path), original)?,
            Some(ScipCompiler::CustomImage(
                "registry.test/custom@sha256:abc".into()
            ))
        );
    }
    assert!(loaded.selectors.contains(&".xyz".to_owned()));
    assert!(
        matches!(
            loaded.resolve(Path::new("src/main.rs"), original)?,
            Selection::Candidates(_)
        ),
        "custom compiler preserves stock syntax extraction"
    );
    assert!(matches!(
        loaded.resolve(Path::new("src/main.xyz"), original)?,
        Selection::Skip(SelectionSkip::NoSelector)
    ));
    cancel.cancel();
    assert_eq!(
        loaded
            .selected_scip_compiler(Path::new("src/main.xyz"), original)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    Ok(())
}
