use crate::*;
use graphit_core::{
    artifact_store::{ArtifactLimits, ArtifactMode, CopyLimits, CopyPolicy},
    brand::Brand,
    native_path::NativePath,
    project_identity::ProjectId,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_fs::NodeKind;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

fn limits() -> ProjectionLimits {
    ProjectionLimits::new(4096, 32 * 1024, 128, 8192, 128).unwrap()
}
fn artifacts() -> ArtifactLimits {
    ArtifactLimits::new(
        64 * 1024,
        8192,
        CopyLimits::new(16 * 1024 * 1024, 512, 32 * 1024, 16).unwrap(),
    )
    .unwrap()
}
fn producer_id() -> ProjectId {
    "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap()
}
fn consumer_id() -> ProjectId {
    "01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap()
}
struct Fixture {
    temp: tempfile::TempDir,
    installation: InstallationPaths,
    project: ProjectPaths,
    producer: ProjectPaths,
}
impl Fixture {
    fn new() -> Self {
        Self::branded("graphit")
    }
    fn branded(brand: &str) -> Self {
        let temp = tempfile::tempdir_in(fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
        let installation = InstallationPaths::resolve(
            Brand::new(brand).unwrap(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let projects: Vec<_> = ["consumer", "producer"]
            .into_iter()
            .map(|name| {
                let root = temp.path().join(name);
                fs::create_dir(&root).unwrap();
                fs::create_dir(root.join(installation.brand().directory())).unwrap();
                ProjectPaths::at(&root, &installation).unwrap()
            })
            .collect();
        Self {
            temp,
            installation,
            project: projects[0].clone(),
            producer: projects[1].clone(),
        }
    }
    fn parents(&self) {
        for binding in NativeBinding::ALL {
            let layout = binding.adapter().layout();
            for folder in [Some(layout.skills), Some(layout.agents), layout.commands]
                .into_iter()
                .flatten()
            {
                fs::create_dir_all(self.project.root().join(layout.root).join(folder)).unwrap();
            }
        }
        fs::create_dir_all(self.project.root().join(".agents/skills")).unwrap();
    }
    fn source_at(&self, directory: &Path, kind: ArtifactType, id: &str) -> SourceRef {
        SourceRef::local_link(
            NativePath::new(directory.to_owned()).unwrap(),
            producer_id(),
            kind,
            ArtifactId::new(id, limits()).unwrap(),
            ConcreteVersion::new("feature/projeção", limits()).unwrap(),
            limits(),
        )
        .unwrap()
    }
    fn source(&self, kind: ArtifactType, id: &str) -> SourceRef {
        let directory = self.temp.path().join(format!("source-{}", kind.as_str()));
        fs::create_dir_all(&directory).unwrap();
        let canonical = match kind {
            ArtifactType::Command | ArtifactType::Workflow => Some("COMMAND.md"),
            ArtifactType::Agent => Some("AGENT.md"),
            ArtifactType::Rule => Some("RULE.md"),
            _ => None,
        };
        if let Some(name) = canonical {
            fs::write(directory.join(name), "body").unwrap();
        }
        self.source_at(&directory, kind, id)
    }
    fn planner(&self, mode: ArtifactMode) -> ProjectionPlanner {
        ProjectionPlanner::new(
            NativePath::new(self.project.root().to_owned()).unwrap(),
            NativePath::new(self.project.project_runtime(consumer_id(), &[]).unwrap()).unwrap(),
            limits(),
            artifacts(),
            mode,
        )
    }
}
fn request(source: &SourceRef, binding: NativeBinding) -> ProjectionRequest<'_> {
    ProjectionRequest {
        binding,
        source,
        alias: None,
        entry_name: None,
    }
}
fn physical(plan: Projection) -> ProjectionPlan {
    match plan {
        Projection::Physical(plan) => *plan,
        _ => panic!("expected physical route"),
    }
}
fn snapshot(root: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime, bool)> {
    fn walk(root: &Path, out: &mut Vec<(PathBuf, u64, std::time::SystemTime, bool)>) {
        let metadata = fs::symlink_metadata(root).unwrap();
        out.push((
            root.to_owned(),
            metadata.len(),
            metadata.modified().unwrap(),
            metadata.permissions().readonly(),
        ));
        if metadata.is_dir() {
            let mut children: Vec<_> = fs::read_dir(root)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            children.sort();
            for child in children {
                walk(&child, out);
            }
        }
    }
    let mut result = Vec::new();
    walk(root, &mut result);
    result
}

#[test]
fn all_adapters_and_types_have_explicit_routes_without_planner_writes() {
    let fixture = Fixture::new();
    fixture.parents();
    let sources: Vec<_> = ArtifactType::ALL
        .into_iter()
        .map(|kind| fixture.source(kind, "group/item"))
        .collect();
    let before = snapshot(fixture.temp.path());
    let planner = fixture.planner(ArtifactMode::Copy(CopyPolicy::RejectFileLinks));
    for binding in NativeBinding::ALL {
        for source in &sources {
            let result = planner.plan(request(source, binding));
            let kind = source.artifact_type();
            if matches!(binding, NativeBinding::Kimi | NativeBinding::KiroV3)
                && matches!(kind, ArtifactType::Command | ArtifactType::Workflow)
            {
                assert_eq!(result.unwrap_err().stage(), ProjectionStage::Unsupported);
                continue;
            }
            match result.unwrap() {
                Projection::Physical(plan) => {
                    let layout = binding.adapter().layout();
                    let (folder, file, node) = match kind {
                        ArtifactType::Skill => (layout.skills, "item".into(), NodeKind::Directory),
                        ArtifactType::Command | ArtifactType::Workflow => {
                            (layout.commands.unwrap(), "item.md".into(), NodeKind::File)
                        }
                        ArtifactType::Agent => (
                            layout.agents,
                            format!("item.{}", binding.agent_extension()),
                            NodeKind::File,
                        ),
                        _ => panic!(),
                    };
                    assert_eq!(
                        plan.destination().as_path(),
                        fixture
                            .project
                            .root()
                            .join(
                                if kind == ArtifactType::Skill && binding == NativeBinding::Codex {
                                    ".agents"
                                } else {
                                    layout.root
                                }
                            )
                            .join(folder)
                            .join(file)
                    );
                    assert_eq!(
                        plan.key().adapter(),
                        InstallationTarget::for_artifact(binding, kind).namespace()
                    );
                    assert_eq!(plan.key().artifact_type(), kind.as_str());
                    assert_eq!(plan.key().id(), "group/item");
                    assert_eq!(plan.selected().kind(), node);
                    assert!(!plan.destination().as_path().exists());
                    plan.revalidate_bindings().unwrap();
                }
                Projection::DynamicRule(plan) => {
                    assert_eq!(kind, ArtifactType::Rule);
                    assert!(plan.selected().input().as_path().ends_with("RULE.md"));
                    plan.revalidate_bindings().unwrap();
                }
                Projection::Native(plan) => {
                    assert!(matches!(
                        kind,
                        ArtifactType::Mcp
                            | ArtifactType::Ast
                            | ArtifactType::Knowledge
                            | ArtifactType::Language
                            | ArtifactType::Power
                    ));
                    assert_eq!(plan.key().id(), source.id().as_str());
                }
            }
        }
    }
    assert_eq!(snapshot(fixture.temp.path()), before);
}

#[test]
fn source_identity_uses_producer_remote_version_and_captured_roots() {
    let a = Fixture::new();
    let b = Fixture::branded("other");
    let id = ArtifactId::new("org/team/item", limits()).unwrap();
    let remote = ArtifactId::new("upstream/item", limits()).unwrap();
    let version = ConcreteVersion::new(&format!("branch/{}é", "x".repeat(300)), limits()).unwrap();
    let before_a = snapshot(a.temp.path());
    let before_b = snapshot(b.temp.path());
    let cached = SourceRef::hub_cache(
        &a.installation,
        producer_id(),
        ArtifactType::Skill,
        id.clone(),
        Some(remote.clone()),
        version.clone(),
        limits(),
    )
    .unwrap();
    assert_eq!(cached.producer(), producer_id());
    assert_ne!(cached.producer(), consumer_id());
    assert_eq!(cached.id(), &id);
    assert_eq!(cached.physical_id(), &remote);
    assert_eq!(cached.version(), &version);
    assert_eq!(cached.origin(), SourceOrigin::HubCache);
    assert!(
        cached.directory().as_path().starts_with(
            a.installation
                .root()
                .join("artifacts/modules")
                .join(producer_id().to_string())
                .join("skill")
        )
    );
    assert!(!cached.directory().as_path().exists());
    let resources = SourceRef::resources(
        &b.producer,
        producer_id(),
        ArtifactType::Skill,
        id.clone(),
        version.clone(),
        limits(),
    )
    .unwrap();
    assert_eq!(resources.origin(), SourceOrigin::Resources);
    assert!(
        resources
            .directory()
            .as_path()
            .starts_with(b.producer.brand_directory().join("artifacts/skill"))
    );
    assert!(
        !resources
            .directory()
            .as_path()
            .starts_with(b.project.root())
    );
    let local = a.source_at(
        &a.temp.path().join("exact-artifact"),
        ArtifactType::Skill,
        id.as_str(),
    );
    assert_eq!(
        local.directory().as_path(),
        a.temp.path().join("exact-artifact")
    );
    assert_eq!(local.origin(), SourceOrigin::LocalLink);
    assert_eq!(cached.root().as_path(), a.installation.root());
    assert_eq!(snapshot(a.temp.path()), before_a);
    assert_eq!(snapshot(b.temp.path()), before_b);
}

#[test]
fn identity_encoding_is_reversible_case_safe_and_prefix_distinct() {
    let fixture = Fixture::new();
    let build = |id: &str, version: &str| {
        SourceRef::hub_cache(
            &fixture.installation,
            producer_id(),
            ArtifactType::Command,
            ArtifactId::new(id, limits()).unwrap(),
            None,
            ConcreteVersion::new(version, limits()).unwrap(),
            limits(),
        )
        .unwrap()
    };
    let a = build("org/Item", &format!("{}/release/é", "a".repeat(130)));
    let b = build("org/item", &format!("{}/release/é", "a".repeat(130)));
    assert_ne!(a.directory(), b.directory());
    assert_ne!(
        a.directory()
            .as_path()
            .to_str()
            .unwrap()
            .to_ascii_lowercase(),
        b.directory()
            .as_path()
            .to_str()
            .unwrap()
            .to_ascii_lowercase()
    );
    fn decode(path: &Path, prefix: &str) -> String {
        let mut bytes = Vec::new();
        for component in path
            .iter()
            .filter_map(|part| part.to_str())
            .filter_map(|part| part.strip_prefix(prefix))
        {
            assert!(component.len() <= 128);
            for i in (0..component.len()).step_by(2) {
                bytes.push(u8::from_str_radix(&component[i..i + 2], 16).unwrap());
            }
        }
        String::from_utf8(bytes).unwrap()
    }
    assert_eq!(decode(a.directory().as_path(), "i-"), a.id().as_str());
    assert_eq!(decode(a.directory().as_path(), "v-"), a.version().as_str());
    let short = build("org/item", &"a".repeat(64));
    let long = build("org/item", &"a".repeat(128));
    assert!(
        !long
            .directory()
            .as_path()
            .starts_with(short.directory().as_path())
    );
    assert_ne!(
        build("org/item", "a/b").directory(),
        build("org/item", "a-b").directory()
    );
    assert_ne!(
        build("org/item", "a/b").directory(),
        build("org/item", "a%2Fb").directory()
    );
}

#[test]
fn source_builders_revalidate_id_and_version_under_the_current_budget() {
    let fixture = Fixture::new();
    let small = ProjectionLimits::new(4, 32 * 1024, 128, 8192, 128).unwrap();
    let large_id = ArtifactId::new("group/long", limits()).unwrap();
    let small_id = ArtifactId::new("id", limits()).unwrap();
    let large_version = ConcreteVersion::new("feature/long", limits()).unwrap();
    let version = ConcreteVersion::new("main", limits()).unwrap();
    for (id, remote, version) in [
        (large_id.clone(), Some(small_id.clone()), version.clone()),
        (small_id.clone(), Some(large_id.clone()), version.clone()),
        (
            small_id.clone(),
            Some(small_id.clone()),
            large_version.clone(),
        ),
    ] {
        assert_eq!(
            SourceRef::hub_cache(
                &fixture.installation,
                producer_id(),
                ArtifactType::Skill,
                id,
                remote,
                version,
                small
            )
            .unwrap_err()
            .stage(),
            ProjectionStage::Validate
        );
    }
    assert!(
        SourceRef::resources(
            &fixture.producer,
            producer_id(),
            ArtifactType::Skill,
            large_id.clone(),
            version.clone(),
            small
        )
        .is_err()
    );
    assert!(
        SourceRef::local_link(
            NativePath::new(fixture.temp.path().join("entry")).unwrap(),
            producer_id(),
            ArtifactType::Skill,
            large_id,
            version,
            small
        )
        .is_err()
    );
    assert!(
        SourceRef::resources(
            &fixture.producer,
            producer_id(),
            ArtifactType::Skill,
            small_id,
            large_version,
            small
        )
        .is_err()
    );
}

#[test]
fn names_include_final_extension_and_do_not_sanitize_or_silently_fallback() {
    let fixture = Fixture::new();
    fixture.parents();
    let source = fixture.source(ArtifactType::Command, "qualified/base");
    let planner = fixture.planner(ArtifactMode::Link);
    for invalid in [
        "",
        ".",
        "..",
        "a/b",
        "a\\b",
        "a:b",
        "a?b",
        "a*",
        "a|b",
        "a<b",
        "a>b",
        "a\"b",
        " a",
        "a ",
        "a.",
        "a\0b",
        "CON",
        "nul.txt",
        "COM1",
        "lpt9.name",
        "COM¹.txt",
        "lpt²",
        "LPT³",
    ] {
        let error = planner
            .plan(ProjectionRequest {
                alias: Some(invalid),
                ..request(&source, NativeBinding::Claude)
            })
            .unwrap_err();
        assert_eq!(error.stage(), ProjectionStage::Validate, "{invalid:?}");
    }
    for valid in ["ação normal", ".local", "com0", "LPT10", "a..b"] {
        let plan = physical(
            planner
                .plan(ProjectionRequest {
                    alias: Some(valid),
                    ..request(&source, NativeBinding::Claude)
                })
                .unwrap(),
        );
        assert_eq!(plan.name(), valid);
        assert_eq!(plan.key().id(), "qualified/base");
    }
    let max = "x".repeat(252);
    assert_eq!(
        physical(
            planner
                .plan(ProjectionRequest {
                    alias: Some(&max),
                    ..request(&source, NativeBinding::Claude)
                })
                .unwrap()
        )
        .destination()
        .as_path()
        .file_name()
        .unwrap()
        .len(),
        255
    );
    let overflow = "x".repeat(253);
    assert_eq!(
        planner
            .plan(ProjectionRequest {
                alias: Some(&overflow),
                ..request(&source, NativeBinding::Claude)
            })
            .unwrap_err()
            .stage(),
        ProjectionStage::Validate
    );
    let agent = fixture.source(ArtifactType::Agent, "team/agent");
    let max = "x".repeat(250);
    assert!(
        planner
            .plan(ProjectionRequest {
                alias: Some(&max),
                ..request(&agent, NativeBinding::Codex)
            })
            .is_ok()
    );
    let overflow = "x".repeat(251);
    assert_eq!(
        planner
            .plan(ProjectionRequest {
                alias: Some(&overflow),
                ..request(&agent, NativeBinding::Codex)
            })
            .unwrap_err()
            .stage(),
        ProjectionStage::Validate
    );
    let entry = physical(
        planner
            .plan(ProjectionRequest {
                entry_name: Some("entry"),
                ..request(&source, NativeBinding::Claude)
            })
            .unwrap(),
    );
    assert_eq!(entry.name(), "entry");
    let base = physical(
        planner
            .plan(ProjectionRequest {
                entry_name: Some(""),
                ..request(&source, NativeBinding::Claude)
            })
            .unwrap(),
    );
    assert_eq!(base.name(), "base");
    let alias = physical(
        planner
            .plan(ProjectionRequest {
                alias: Some("alias"),
                entry_name: Some("entry"),
                ..request(&source, NativeBinding::Claude)
            })
            .unwrap(),
    );
    assert_eq!(alias.name(), "alias");
    for _ in 0..3 {
        let repeated = physical(
            planner
                .plan(request(&source, NativeBinding::Claude))
                .unwrap(),
        );
        assert_eq!(repeated.key(), base.key());
        assert_eq!(repeated.destination(), base.destination());
        assert_eq!(repeated.mode(), base.mode());
    }
}

#[test]
fn preflight_rejects_alias_type_case_and_duplicate_key_collisions_before_source_io() {
    let fixture = Fixture::new();
    let absent = fixture.temp.path().join("not-hydrated");
    let command = fixture.source_at(&absent, ArtifactType::Command, "org/shared");
    let workflow = fixture.source_at(&absent, ArtifactType::Workflow, "other/shared");
    let planner = fixture.planner(ArtifactMode::Link);
    let before = snapshot(fixture.temp.path());
    for requests in [
        vec![
            request(&command, NativeBinding::Claude),
            request(&workflow, NativeBinding::Claude),
        ],
        vec![
            ProjectionRequest {
                alias: Some("SHARED"),
                ..request(&command, NativeBinding::Claude)
            },
            request(&workflow, NativeBinding::Claude),
        ],
        vec![
            ProjectionRequest {
                alias: Some("a"),
                ..request(&command, NativeBinding::Claude)
            },
            ProjectionRequest {
                alias: Some("b"),
                ..request(&command, NativeBinding::Claude)
            },
        ],
    ] {
        let error = planner.preflight(&requests).unwrap_err();
        assert_eq!(error.stage(), ProjectionStage::Collision);
    }
    assert_eq!(snapshot(fixture.temp.path()), before);
}

#[test]
fn fallback_is_regular_ordered_bounded_and_canonical_errors_never_fallback() {
    let fixture = Fixture::new();
    fixture.parents();
    let source = fixture.source(ArtifactType::Workflow, "team/flow");
    let root = source.directory().as_path();
    fs::write(root.join("00-first.md"), "first").unwrap();
    let planner = fixture.planner(ArtifactMode::Copy(CopyPolicy::RejectFileLinks));
    assert!(
        physical(
            planner
                .plan(request(&source, NativeBinding::Claude))
                .unwrap()
        )
        .selected()
        .input()
        .as_path()
        .ends_with("COMMAND.md")
    );
    fs::remove_file(root.join("COMMAND.md")).unwrap();
    fs::create_dir(root.join("00-dir")).unwrap();
    fs::write(root.join("z-last.md"), "last").unwrap();
    let before = snapshot(fixture.temp.path());
    assert!(
        physical(
            planner
                .plan(request(&source, NativeBinding::Claude))
                .unwrap()
        )
        .selected()
        .input()
        .as_path()
        .ends_with("00-first.md")
    );
    assert_eq!(snapshot(fixture.temp.path()), before);
    let bounded = ProjectionPlanner::new(
        NativePath::new(fixture.project.root().to_owned()).unwrap(),
        NativePath::new(fixture.project.project_runtime(consumer_id(), &[]).unwrap()).unwrap(),
        ProjectionLimits::new(4096, 32 * 1024, 1, 8192, 128).unwrap(),
        artifacts(),
        ArtifactMode::Link,
    );
    assert_eq!(
        bounded
            .plan(request(&source, NativeBinding::Claude))
            .unwrap_err()
            .stage(),
        ProjectionStage::Enumerate
    );
    let bounded = ProjectionPlanner::new(
        NativePath::new(fixture.project.root().to_owned()).unwrap(),
        NativePath::new(fixture.project.project_runtime(consumer_id(), &[]).unwrap()).unwrap(),
        ProjectionLimits::new(4096, 32 * 1024, 128, 1, 128).unwrap(),
        artifacts(),
        ArtifactMode::Link,
    );
    assert_eq!(
        bounded
            .plan(request(&source, NativeBinding::Claude))
            .unwrap_err()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    fs::create_dir(root.join("COMMAND.md")).unwrap();
    assert_eq!(
        planner
            .plan(request(&source, NativeBinding::Claude))
            .unwrap_err()
            .stage(),
        ProjectionStage::Source
    );
    fs::remove_dir(root.join("COMMAND.md")).unwrap();
    fs::remove_file(root.join("00-first.md")).unwrap();
    fs::remove_file(root.join("z-last.md")).unwrap();
    assert_eq!(
        planner
            .plan(request(&source, NativeBinding::Claude))
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
}

#[test]
fn source_and_destination_absence_kinds_and_captured_budgets_are_explicit() {
    let fixture = Fixture::new();
    let source = fixture.source(ArtifactType::Skill, "group/skill");
    let planner = fixture.planner(ArtifactMode::Link);
    let before = snapshot(fixture.temp.path());
    let targets = planner
        .preflight(&[request(&source, NativeBinding::Claude)])
        .unwrap();
    assert_eq!(targets[0].key().id(), "group/skill");
    assert_eq!(targets[0].name(), "skill");
    assert_eq!(targets[0].kind(), Some(NodeKind::Directory));
    assert_eq!(
        targets[0].destination().unwrap().as_path(),
        fixture.project.root().join(".claude/skills/skill")
    );
    assert_eq!(snapshot(fixture.temp.path()), before);
    let error = planner
        .plan(request(&source, NativeBinding::Claude))
        .unwrap_err();
    assert_eq!(
        (error.stage(), error.kind()),
        (ProjectionStage::Destination, io::ErrorKind::NotFound)
    );
    fs::create_dir_all(fixture.project.root().join(".claude")).unwrap();
    fs::write(fixture.project.root().join(".claude/skills"), "file").unwrap();
    assert_eq!(
        planner
            .plan(request(&source, NativeBinding::Claude))
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotADirectory
    );
    let file = fixture.temp.path().join("file-not-directory");
    fs::write(&file, "file").unwrap();
    let wrong = fixture.source_at(&file, ArtifactType::Skill, "group/wrong");
    assert_eq!(
        planner
            .plan(request(&wrong, NativeBinding::Claude))
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotADirectory
    );
    let absent = fixture.source_at(
        &fixture.temp.path().join("absent"),
        ArtifactType::Skill,
        "group/absent",
    );
    assert_eq!(
        planner
            .plan(request(&absent, NativeBinding::Claude))
            .unwrap_err()
            .stage(),
        ProjectionStage::Source
    );
    let tiny = ArtifactLimits::new(1024, 1, CopyLimits::new(1, 1, 1, 1).unwrap()).unwrap();
    let planner = ProjectionPlanner::new(
        NativePath::new(fixture.project.root().to_owned()).unwrap(),
        NativePath::new(fixture.project.project_runtime(consumer_id(), &[]).unwrap()).unwrap(),
        limits(),
        tiny,
        ArtifactMode::Link,
    );
    assert_eq!(
        planner
            .plan(request(&absent, NativeBinding::Claude))
            .unwrap_err()
            .stage(),
        ProjectionStage::Validate
    );
    let bounded = ProjectionLimits::new(1, 1024, 1, 1, 1).unwrap();
    assert!(ArtifactId::new("too-long", bounded).is_err());
    assert!(ConcreteVersion::new("too-long", bounded).is_err());
    for id in [
        "",
        "a//b",
        "/a",
        "a/",
        "a/../b",
        "a/CON.txt",
        "a\\b",
        " a",
        "a/ b",
    ] {
        assert!(ArtifactId::new(id, limits()).is_err(), "{id}");
    }
    for version in ["", " leading", "trailing ", "a\0b", "a\nb"] {
        assert!(ConcreteVersion::new(version, limits()).is_err());
    }
    for params in [
        (0, 1, 1, 1, 1),
        (1, 0, 1, 1, 1),
        (1, 1, 0, 1, 1),
        (1, 1, 1, 0, 1),
        (1, 1, 1, 1, 0),
    ] {
        assert!(ProjectionLimits::new(params.0, params.1, params.2, params.3, params.4).is_err());
    }
    let planner = ProjectionPlanner::new(
        NativePath::new(fixture.project.root().to_owned()).unwrap(),
        NativePath::new(fixture.project.project_runtime(consumer_id(), &[]).unwrap()).unwrap(),
        bounded,
        artifacts(),
        ArtifactMode::Link,
    );
    assert_eq!(
        planner
            .preflight(&[
                request(&absent, NativeBinding::Claude),
                request(&absent, NativeBinding::Cursor)
            ])
            .unwrap_err()
            .stage(),
        ProjectionStage::Validate
    );
}

#[test]
fn pure_removal_targets_survive_producer_source_deletion() {
    let fixture = Fixture::new();
    fixture.parents();
    let source = fixture.source(ArtifactType::Command, "producer/command");
    let planner = fixture.planner(ArtifactMode::Link);
    let published_binding = physical(
        planner
            .plan(request(&source, NativeBinding::Claude))
            .unwrap(),
    );
    fs::remove_dir_all(source.directory().as_path()).unwrap();
    let before = snapshot(fixture.temp.path());
    let targets = planner
        .preflight(&[request(&source, NativeBinding::Claude)])
        .unwrap();
    assert_eq!(targets[0].key(), published_binding.key());
    assert_eq!(targets[0].name(), published_binding.name());
    assert_eq!(
        targets[0].destination().unwrap(),
        published_binding.destination()
    );
    assert_eq!(targets[0].kind(), Some(NodeKind::File));
    assert_eq!(snapshot(fixture.temp.path()), before);
    assert_eq!(
        planner
            .plan(request(&source, NativeBinding::Claude))
            .unwrap_err()
            .stage(),
        ProjectionStage::Source
    );
    // Targets remain data; removal/Forget still require the consumer's persisted membership/proofs.
}

#[cfg(unix)]
#[test]
fn runtime_directory_and_alias_scope_cannot_become_a_projected_destination() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fixture.parents();
    let source = fixture.source(ArtifactType::Command, "org/command");
    let commands = fixture.project.root().join(".claude/commands");
    let runtime = fixture.project.project_runtime(consumer_id(), &[]).unwrap();
    fs::create_dir_all(runtime.parent().unwrap()).unwrap();
    let real = fixture.project.root().join("real-runtime");
    fs::create_dir(&real).unwrap();
    fs::remove_dir(&commands).unwrap();
    symlink(&real, &commands).unwrap();
    symlink(&real, &runtime).unwrap();
    let before = snapshot(fixture.temp.path());
    assert_eq!(
        fixture
            .planner(ArtifactMode::Link)
            .plan(request(&source, NativeBinding::Claude))
            .unwrap_err()
            .stage(),
        ProjectionStage::Destination
    );
    assert_eq!(snapshot(fixture.temp.path()), before);
    fs::remove_file(&runtime).unwrap();
    fs::create_dir(&runtime).unwrap();
    fs::remove_file(&commands).unwrap();
    symlink(&runtime, &commands).unwrap();
    assert_eq!(
        fixture
            .planner(ArtifactMode::Link)
            .plan(request(&source, NativeBinding::Claude))
            .unwrap_err()
            .stage(),
        ProjectionStage::Destination
    );
    fs::remove_file(&commands).unwrap();
    fs::create_dir(&commands).unwrap();
    let plan = physical(
        fixture
            .planner(ArtifactMode::Link)
            .plan(request(&source, NativeBinding::Claude))
            .unwrap(),
    );
    fs::remove_dir(&runtime).unwrap();
    symlink(&real, &runtime).unwrap();
    assert_eq!(
        plan.revalidate_bindings().unwrap_err().stage(),
        ProjectionStage::Destination
    );
}

#[cfg(unix)]
#[test]
fn file_links_keep_selected_input_and_retarget_requires_new_plan() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fixture.parents();
    let source = fixture.source(ArtifactType::Command, "org/command");
    let root = source.directory().as_path();
    let canonical = root.join("COMMAND.md");
    fs::remove_file(&canonical).unwrap();
    let first = root.join("first.md");
    let second = root.join("second.md");
    fs::write(&first, "first").unwrap();
    fs::write(&second, "second").unwrap();
    symlink(&first, &canonical).unwrap();
    let rejected = fixture
        .planner(ArtifactMode::Copy(CopyPolicy::RejectFileLinks))
        .plan(request(&source, NativeBinding::Claude))
        .unwrap_err();
    assert_eq!(rejected.stage(), ProjectionStage::Select);
    for mode in [
        ArtifactMode::Link,
        ArtifactMode::Copy(CopyPolicy::FollowRegularFiles),
    ] {
        let plan = physical(
            fixture
                .planner(mode)
                .plan(request(&source, NativeBinding::Claude))
                .unwrap(),
        );
        assert_eq!(plan.selected().input().as_path(), canonical);
        assert_eq!(plan.selected().canonical().as_path(), first);
        assert!(plan.selected().is_link());
        assert!(
            fs::symlink_metadata(plan.selected().input().as_path())
                .unwrap()
                .file_type()
                .is_symlink()
        );
        plan.revalidate_bindings().unwrap();
        fs::write(&first, "fresh live bytes").unwrap();
        plan.revalidate_bindings().unwrap();
        fs::remove_file(&canonical).unwrap();
        symlink(&second, &canonical).unwrap();
        assert_eq!(
            plan.revalidate_bindings().unwrap_err().stage(),
            ProjectionStage::SourceChanged
        );
        fs::remove_file(&canonical).unwrap();
        symlink(&first, &canonical).unwrap();
    }
    fs::remove_file(&canonical).unwrap();
    symlink(root.join("missing"), &canonical).unwrap();
    let error = fixture
        .planner(ArtifactMode::Link)
        .plan(request(&source, NativeBinding::Claude))
        .unwrap_err();
    assert_eq!(
        (error.stage(), error.kind()),
        (ProjectionStage::Source, io::ErrorKind::NotFound)
    );
}

#[cfg(unix)]
#[test]
fn directory_links_and_destination_aliases_are_captured_without_adoption() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fixture.parents();
    let first = fixture.temp.path().join("first-tree");
    let second = fixture.temp.path().join("second-tree");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let alias = fixture.temp.path().join("alias");
    symlink(&first, &alias).unwrap();
    let source = fixture.source_at(&alias, ArtifactType::Skill, "org/skill");
    assert_eq!(
        fixture
            .planner(ArtifactMode::Copy(CopyPolicy::FollowRegularFiles))
            .plan(request(&source, NativeBinding::Claude))
            .unwrap_err()
            .stage(),
        ProjectionStage::Select
    );
    let plan = physical(
        fixture
            .planner(ArtifactMode::Link)
            .plan(request(&source, NativeBinding::Claude))
            .unwrap(),
    );
    assert_eq!(plan.selected().input().as_path(), alias);
    assert!(plan.selected().is_link());
    fs::remove_file(&alias).unwrap();
    symlink(&second, &alias).unwrap();
    assert_eq!(
        plan.revalidate_bindings().unwrap_err().stage(),
        ProjectionStage::SourceChanged
    );
    let source = fixture.source(ArtifactType::Command, "org/command");
    let dest = fixture.project.root().join(".claude/commands");
    fs::remove_dir(&dest).unwrap();
    let real = fixture.project.root().join("real-destination");
    let changed = fixture.project.root().join("changed-destination");
    fs::create_dir(&real).unwrap();
    fs::create_dir(&changed).unwrap();
    symlink(&real, &dest).unwrap();
    let plan = physical(
        fixture
            .planner(ArtifactMode::Link)
            .plan(request(&source, NativeBinding::Claude))
            .unwrap(),
    );
    assert_eq!(plan.destination().as_path(), real.join("command.md"));
    fs::remove_file(&dest).unwrap();
    symlink(&changed, &dest).unwrap();
    assert_eq!(
        plan.revalidate_bindings().unwrap_err().stage(),
        ProjectionStage::DestinationChanged
    );
    fs::remove_file(&dest).unwrap();
    symlink(fixture.temp.path().join("outside"), &dest).unwrap();
    fs::create_dir(fixture.temp.path().join("outside")).unwrap();
    assert_eq!(
        fixture
            .planner(ArtifactMode::Link)
            .plan(request(&source, NativeBinding::Claude))
            .unwrap_err()
            .stage(),
        ProjectionStage::Destination
    );
}

#[cfg(unix)]
#[test]
fn fallback_non_unicode_names_are_rejected() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let fixture = Fixture::new();
    fixture.parents();
    let source = fixture.source(ArtifactType::Command, "org/command");
    let root = source.directory().as_path();
    fs::remove_file(root.join("COMMAND.md")).unwrap();
    let first = OsString::from_vec(vec![b'a', 0x80]);
    let last = OsString::from_vec(vec![b'a', 0xff]);
    if let Err(error) = fs::write(root.join(&last), "last") {
        // APFS can reject non-Unicode names before the planner can observe them.
        assert_eq!(error.raw_os_error(), Some(libc::EILSEQ));
        assert!(fs::read_dir(root).unwrap().next().is_none());
        return;
    }
    fs::write(root.join(&first), "first").unwrap();
    let planner = fixture.planner(ArtifactMode::Link);
    assert!(
        planner
            .plan(request(&source, NativeBinding::Claude))
            .is_err()
    );
    assert_eq!(fs::read(root.join(first)).unwrap(), b"first");
    assert_eq!(fs::read(root.join(last)).unwrap(), b"last");
}

#[cfg(target_os = "linux")]
#[test]
fn real_permission_error_does_not_become_canonical_absence() {
    use std::{
        os::unix::{fs::PermissionsExt, process::CommandExt},
        process::Command,
    };
    const CHILD: &str = "GRAPHIT_PROJECTION_PERMISSION_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let directory = PathBuf::from(root);
        let fixture = Fixture::new();
        let source = fixture.source_at(&directory, ArtifactType::Command, "org/command");
        let error = fixture
            .planner(ArtifactMode::Link)
            .plan(request(&source, NativeBinding::Claude))
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(matches!(
            error.stage(),
            ProjectionStage::Source | ProjectionStage::Select
        ));
        return;
    }
    let fixture = Fixture::new();
    let root = fixture.temp.path().join("denied");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("COMMAND.md"), "canonical").unwrap();
    fs::write(root.join("fallback.md"), "fallback").unwrap();
    fs::set_permissions(fixture.temp.path(), fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();
    let executable = std::env::current_exe().unwrap();
    let mut command = match fs::read_dir(&root) {
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Command::new(executable),
        Ok(_) => {
            // A privileged runner can bypass mode bits. Run the same test as an
            // unprivileged UID, with an executable reachable outside its checkout.
            let child = fixture.temp.path().join("projection-permission-child");
            fs::copy(executable, &child).unwrap();
            fs::set_permissions(&child, fs::Permissions::from_mode(0o755)).unwrap();
            let mut command = Command::new(child);
            command.uid(65534).gid(65534);
            command
        }
        Err(error) => panic!("could not inspect permission fixture: {error}"),
    };
    let result = command
        .args([
            "projection_tests::real_permission_error_does_not_become_canonical_absence",
            "--exact",
            "--nocapture",
        ])
        .env(CHILD, &root)
        .output();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let result = result.expect("permission helper must start");
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
