//! Physical install/sync consumer; source setup uses the actual Hub publication SDK.
use super::*;
use graphit_core::{
    artifact_store::{
        ArtifactMode, ArtifactStore, CapturedPublicationScope, CopyPolicy, ScopeLimits,
    },
    config::{ConfigKey, ConfigStore},
    native_path::NativePath,
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, ProjectionRequest, SourceRef,
    native::{RenderContext, SkillDescriptor, render_skill},
    publication::ScopedCoordinator,
    registry::{ArtifactDisplayMetadata, HubRegistry},
};

pub(super) fn local_hub_skill(f: &Fixture) -> std::path::PathBuf {
    let cancel = Cancellation::default();
    let request = f.policy.request_context(Instant::now(), &cancel).unwrap();
    let identity = ProjectIdentityStore::at(&f.project)
        .load()
        .unwrap()
        .unwrap();
    let source = SourceRef::resources(
        &f.project,
        identity.id(),
        ArtifactType::Skill,
        ArtifactId::new("fixtures/task-instructions", f.policy.projection()).unwrap(),
        ConcreteVersion::new("one", f.policy.projection()).unwrap(),
        f.policy.projection(),
    )
    .unwrap();
    fs::create_dir_all(source.directory().as_path().join("references")).unwrap();
    fs::write(source.directory().as_path().join("SKILL.md"), "---\nname: selected-task\ndescription: Maintained local instruction selection\nlicense: Apache-2.0\ncompatibility: Graphit modules\nmetadata:\n  owner: fixture-producer\n---\nHub task {{ brand_display }} {{ mcp_tool_task_get }}\n[Execution](references/execution.md)\n").unwrap();
    fs::write(
        source.directory().as_path().join("references/execution.md"),
        "Hub execution {{ mcp_tool_task_get }}\n",
    )
    .unwrap();
    let scope = CapturedPublicationScope::project(
        &f.project,
        identity.id(),
        ScopeLimits::new(16, 65536, 256).unwrap(),
        request.snapshot(),
    )
    .unwrap();
    let store = ArtifactStore::new(scope, f.policy.artifacts());
    let registry = HubRegistry::project(&f.project, &identity, f.policy.hub()).unwrap();
    let coordinator = ScopedCoordinator::new(
        registry,
        store,
        NativePath::new(f.project.root().to_owned()).unwrap(),
    )
    .unwrap();
    let display =
        ArtifactDisplayMetadata::new("Selected task instructions", None, &[], f.policy.hub())
            .unwrap();
    let mut writer = coordinator.writer(&request).unwrap();
    let result = writer
        .publish(
            ProjectionRequest {
                binding: NativeBinding::Codex,
                source: &source,
                alias: Some("hub-task-source"),
                entry_name: None,
            },
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
            Some(&display),
        )
        .unwrap();
    assert!(result.cleanup_pending.is_none());
    assert!(
        f.project
            .root()
            .join(".agents/skills/hub-task-source/SKILL.md")
            .is_file()
    );
    source.directory().as_path().to_owned()
}
fn native_skill(f: &Fixture, name: &str) -> std::path::PathBuf {
    f.project.root().join(".agents/skills").join(name)
}
fn assert_five_sets(f: &Fixture, result: &InstallOutcome, task_name: &str, selected_hub: bool) {
    assert_eq!(result.skills.len(), 5);
    let mut companions = 0;
    for effect in &result.skills {
        assert!(!effect.observation_failed);
        assert_eq!(effect.pending, Some(false));
        assert!(effect.publication.is_some());
        let root = f.project.root().join(&effect.path);
        let bytes = fs::read(root.join("SKILL.md")).unwrap();
        let cancel = Cancellation::default();
        let mut renderer = RenderContext::new(
            f.policy.content(),
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let descriptor = SkillDescriptor::parse(&bytes, &mut renderer).unwrap();
        assert_eq!(descriptor.name(), effect.name);
        assert!(!descriptor.body().contains("{{"));
        let module = [
            graphit_core::module_settings::Module::Task,
            graphit_core::module_settings::Module::Memory,
            graphit_core::module_settings::Module::Ast,
            graphit_core::module_settings::Module::Knowledge,
            graphit_core::module_settings::Module::Hub,
        ]
        .into_iter()
        .find(|module| module.name() == effect.module)
        .unwrap();
        let templates = graphit_runtime::skill_instructions::skill_templates(module).unwrap();
        for reference in templates.references() {
            let text = fs::read_to_string(root.join(reference.relative_path)).unwrap();
            assert!(!text.is_empty());
            assert!(!text.contains("{{"));
            companions += 1;
        }
        if effect.module == "task" {
            assert_eq!(effect.name, task_name);
            if selected_hub {
                assert_eq!(descriptor.license(), Some("Apache-2.0"));
                assert_eq!(descriptor.compatibility(), Some("Graphit modules"));
                assert_eq!(
                    descriptor
                        .metadata()
                        .and_then(|items| items.get("owner"))
                        .map(String::as_str),
                    Some("fixture-producer")
                );
                assert!(descriptor.body().contains(&format!(
                    "Hub task {} {}",
                    f.project.brand().key(),
                    f.project.brand().mcp_tool_name(&["task", "get"]).unwrap()
                )));
                assert_eq!(
                    fs::read_to_string(root.join("references/execution.md")).unwrap(),
                    format!(
                        "Hub execution {}\n",
                        f.project.brand().mcp_tool_name(&["task", "get"]).unwrap()
                    )
                );
                assert_eq!(effect.origins["skills/task/entry.md"], "Hub");
            } else {
                assert_eq!(effect.origins["skills/task/entry.md"], "CompiledDefault");
            }
        } else if effect.module == "knowledge" {
            assert!(descriptor.body().contains("Project knowledge handbook"));
            assert_eq!(effect.origins["skills/knowledge/entry.md"], "Project");
        } else if effect.module == "memory" {
            assert!(
                descriptor
                    .body()
                    .contains(&format!("Global memory {}", f.project.brand().key()))
            );
            assert_eq!(effect.origins["skills/memory/entry.md"], "Global");
        } else {
            assert_eq!(
                effect.origins[&format!("skills/{}/entry.md", effect.module)],
                "CompiledDefault"
            );
        }
    }
    assert_eq!(companions, 10);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_sync_reports_unowned_skill_destination_and_preserves_user_files() {
    let f = Fixture::new();
    let name = f.project.brand().skill_name("task").unwrap();
    let foreign = native_skill(&f, &name);
    fs::create_dir_all(foreign.join("references")).unwrap();
    let files = [
        (
            "SKILL.md",
            "Existing skill maintained outside this writer\n",
        ),
        ("references/session.md", "Existing session instructions\n"),
        ("notes.txt", "User notes must survive publication refusal\n"),
    ];
    for (relative, text) in files {
        fs::write(foreign.join(relative), text).unwrap();
    }
    let raw = f
        .project
        .brand_directory()
        .join("rules/skills/task/entry.md");
    fs::create_dir_all(raw.parent().unwrap()).unwrap();
    let override_text = "Selected project override {{ brand_display }}\n";
    fs::write(&raw, override_text).unwrap();

    let (stop, job) = f.start();
    let cancel = Cancellation::default();
    let request = f.policy.request_context(Instant::now(), &cancel).unwrap();
    let identity = ProjectIdentityStore::at(&f.project)
        .load()
        .unwrap()
        .unwrap();
    let scope = CapturedPublicationScope::project(
        &f.project,
        identity.id(),
        ScopeLimits::new(16, 65536, 256).unwrap(),
        request.snapshot(),
    )
    .unwrap();
    let store = ArtifactStore::new(scope, f.policy.artifacts());
    let key = fs::read_to_string(f.installation.root().join("daemon/mcp.key")).unwrap();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let input = f.input(NativeBinding::Codex, vec![ManagedRoleName::Scribe]);
    for _ in 0..2 {
        let response = client
            .post(f.endpoint())
            .header("x-graphit-auth-origin", "runtime")
            .bearer_auth(&key)
            .json(&input)
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let result: InstallOutcome = response.json().await.unwrap();
        let failure = result
            .failure
            .as_deref()
            .expect("foreign destination refused");
        assert!(failure.contains(&format!(
            "skill task publication at {} failed:",
            std::path::Path::new(".agents/skills").join(&name).display()
        )));
        assert!(failure.contains("Conflict: AlreadyExists"));
        assert_eq!(result.roles.len(), 1);
        assert!(result.roles[0].definition_configured);
        assert_eq!(result.skills.len(), 1);
        let task = &result.skills[0];
        assert_eq!(task.module, "task");
        assert_eq!(task.origins["skills/task/entry.md"], "Project");
        assert!(task.publication.is_none());
        assert_eq!(task.pending, Some(false));
        assert!(!task.observation_failed);

        let snapshot = store.load().unwrap().unwrap();
        assert!(snapshot.pending().is_none());
        assert!(
            snapshot
                .entries()
                .iter()
                .all(|entry| entry.destination() != foreign)
        );
        for (relative, text) in files {
            assert_eq!(fs::read_to_string(foreign.join(relative)).unwrap(), text);
        }
        assert_eq!(fs::read_dir(&foreign).unwrap().count(), 3);
        assert_eq!(fs::read_dir(foreign.join("references")).unwrap().count(), 1);
        assert_eq!(fs::read_to_string(&raw).unwrap(), override_text);
    }
    f.stop(stop, job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_install_sync_materializes_skills_companions_overrides_and_shared_targets() {
    let f = Fixture::new();
    let (stop, job) = f.start();
    let cancel = Cancellation::default();
    ConfigStore::project(&f.project)
        .unwrap()
        .set(
            ConfigKey::new("knowledge.docs_dir").unwrap(),
            serde_json::json!("handbook"),
            Duration::from_secs(5),
            &cancel,
        )
        .unwrap();
    let project = f
        .project
        .brand_directory()
        .join("rules/skills/knowledge/entry.md");
    fs::create_dir_all(project.parent().unwrap()).unwrap();
    fs::write(&project,"Project knowledge {{ docs_dir }}\n[Design](references/documentation-design.md)\n[Domain](references/worked-domain.md)\n").unwrap();
    let global = f.installation.root().join("rules/skills/memory/entry.md");
    fs::create_dir_all(global.parent().unwrap()).unwrap();
    fs::write(
        &global,
        "Global memory {{ brand_display }}\n[Context](references/durable-context.md)\n",
    )
    .unwrap();
    let unrelated = native_skill(&f, "unrelated").join("SKILL.md");
    fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
    fs::write(&unrelated, "Unrelated skill remains foreign\n").unwrap();
    local_hub_skill(&f);
    let args = [
        "roles",
        "install",
        "--agent",
        "codex",
        "--role",
        "scribe",
        "--skill-alias",
        "task=delivered-task",
    ];
    let first = outcome(&f.executable(&args).await);
    assert_five_sets(&f, &first, "delivered-task", false);
    let shared = outcome(
        &f.executable(&[
            "roles",
            "sync",
            "--agent",
            "antigravity",
            "--role",
            "scribe",
            "--skill-alias",
            "task=delivered-task",
        ])
        .await,
    );
    assert!(shared.failure.is_none());
    assert!(
        shared
            .skills
            .iter()
            .all(|effect| effect.publication.as_deref() == Some("Unchanged"))
    );
    assert_five_sets(&f, &shared, "delivered-task", false);
    let retargeted = outcome(
        &f.executable(&[
            "roles",
            "sync",
            "--agent",
            "codex",
            "--role",
            "scribe",
            "--skill-alias",
            "task=retargeted-task",
            "--skill-bundle",
            "task=fixtures/task-instructions@one",
        ])
        .await,
    );
    assert_five_sets(&f, &retargeted, "retargeted-task", true);
    assert!(!native_skill(&f, "delivered-task").exists());
    let foreign = native_skill(&f, "blocked-task");
    fs::create_dir_all(&foreign).unwrap();
    let current = fs::read(native_skill(&f, "retargeted-task").join("SKILL.md")).unwrap();
    let mut renderer = RenderContext::new(
        f.policy.content(),
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let descriptor = SkillDescriptor::parse(&current, &mut renderer).unwrap();
    let equal_candidate = render_skill(
        NativeBinding::Codex,
        &descriptor,
        Some("blocked-task"),
        &mut renderer,
    )
    .unwrap();
    fs::write(foreign.join("SKILL.md"), &equal_candidate.bytes).unwrap();
    let task = graphit_runtime::skill_instructions::skill_templates(
        graphit_core::module_settings::Module::Task,
    )
    .unwrap();
    fs::create_dir(foreign.join("references")).unwrap();
    for reference in task.references() {
        fs::copy(
            native_skill(&f, "retargeted-task").join(reference.relative_path),
            foreign.join(reference.relative_path),
        )
        .unwrap();
    }
    let refused = f
        .executable(&[
            "roles",
            "sync",
            "--agent",
            "codex",
            "--role",
            "scribe",
            "--skill-alias",
            "task=blocked-task",
            "--skill-bundle",
            "task=fixtures/task-instructions@one",
        ])
        .await;
    assert!(!refused.status.success());
    let failure: InstallOutcome = serde_json::from_slice(&refused.stdout).unwrap();
    assert!(failure.failure.is_some());
    assert!(
        failure
            .skills
            .iter()
            .any(|effect| effect.module == "task" && effect.publication.is_none())
    );
    assert_eq!(
        fs::read(foreign.join("SKILL.md")).unwrap(),
        equal_candidate.bytes
    );
    assert_eq!(
        fs::read(native_skill(&f, "retargeted-task").join("SKILL.md")).unwrap(),
        current
    );
    assert_eq!(
        fs::read_to_string(&unrelated).unwrap(),
        "Unrelated skill remains foreign\n"
    );
    f.stop(stop, job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn installed_local_bundle_uses_current_brand_precedence_and_rejects_invalid_companion() {
    let f = Fixture::with_brand(Brand::new("orbit").unwrap());
    let (stop, job) = f.start();
    let cancel = Cancellation::default();
    ConfigStore::project(&f.project)
        .unwrap()
        .set(
            ConfigKey::new("knowledge.docs_dir").unwrap(),
            serde_json::json!("guidebook"),
            Duration::from_secs(5),
            &cancel,
        )
        .unwrap();
    let global = f.installation.root().join("rules/skills/task/entry.md");
    fs::create_dir_all(global.parent().unwrap()).unwrap();
    fs::write(&global, "GLOBAL {{ brand_display }} {{ mcp_tool_task_get }} {{ mcp_tool_module_skill }}\n[Execution](references/execution.md)\n").unwrap();
    let project = f
        .project
        .brand_directory()
        .join("rules/skills/task/entry.md");
    fs::create_dir_all(project.parent().unwrap()).unwrap();
    fs::write(&project, "PROJECT {{ brand_display }} {{ mcp_tool_task_get }} {{ mcp_tool_module_skill }}\n[Execution](references/execution.md)\n").unwrap();
    let source = local_hub_skill(&f);
    let key = fs::read_to_string(f.installation.root().join("daemon/mcp.key")).unwrap();
    let mut input = f.input(NativeBinding::Codex, vec![ManagedRoleName::Scribe]);
    input
        .skill_aliases
        .insert("task".into(), "orbit-instructions".into());
    input.module_skill = Some("orbit_current_skill".into());
    input.skill_bundles.insert(
        "task".into(),
        graphit_app::compiled_skills::HubSelection {
            id: "fixtures/task-instructions".into(),
            version: "one".into(),
            identity_provider: None,
        },
    );
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for (expected, origin) in [
        ("PROJECT", "Project"),
        ("GLOBAL", "Global"),
        ("Hub task", "Hub"),
    ] {
        let response = client
            .post(f.endpoint())
            .header("x-graphit-auth-origin", "runtime")
            .bearer_auth(&key)
            .json(&input)
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let outcome: InstallOutcome = response.json().await.unwrap();
        assert!(outcome.failure.is_none());
        assert_eq!(outcome.skills.len(), 5);
        let task = outcome
            .skills
            .iter()
            .find(|effect| effect.module == "task")
            .unwrap();
        assert_eq!(task.origins["skills/task/entry.md"], origin);
        let task_root = f.project.root().join(&task.path);
        let bytes = fs::read_to_string(task_root.join("SKILL.md")).unwrap();
        assert!(bytes.contains(&format!("{expected} orbit orbit_task_get")));
        let mut renderer = RenderContext::new(
            f.policy.content(),
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let descriptor = SkillDescriptor::parse(bytes.as_bytes(), &mut renderer).unwrap();
        assert_eq!(descriptor.license(), Some("Apache-2.0"));
        assert_eq!(descriptor.compatibility(), Some("Graphit modules"));
        assert_eq!(
            descriptor
                .metadata()
                .and_then(|items| items.get("owner"))
                .map(String::as_str),
            Some("fixture-producer")
        );
        if origin != "Hub" {
            assert!(bytes.contains("orbit_current_skill"));
        }
        assert_eq!(
            fs::read_to_string(task_root.join("references/execution.md")).unwrap(),
            "Hub execution orbit_task_get\n"
        );
        for effect in &outcome.skills {
            let root = f.project.root().join(&effect.path);
            let body = fs::read_to_string(root.join("SKILL.md")).unwrap();
            assert!(!body.contains("{{"));
            assert!(!body.contains("graphit_"));
            if effect.module == "knowledge" {
                assert!(body.contains("guidebook"));
            }
        }
        match origin {
            "Project" => fs::remove_file(&project).unwrap(),
            "Global" => fs::remove_file(&global).unwrap(),
            _ => {}
        }
    }
    let owned = native_skill(&f, "orbit-instructions").join("SKILL.md");
    let before = fs::read(&owned).unwrap();
    fs::write(
        source.join("references/execution.md"),
        "{{ missing_selected_value }}",
    )
    .unwrap();
    let response = client
        .post(f.endpoint())
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(&key)
        .json(&input)
        .send()
        .await
        .unwrap();
    let invalid: InstallOutcome = response.json().await.unwrap();
    assert!(invalid.failure.is_some());
    assert!(invalid.skills.is_empty());
    assert_eq!(fs::read(&owned).unwrap(), before);
    f.stop(stop, job).await;
}
