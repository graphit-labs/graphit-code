use super::*;
use graphit_app::managed_roles::global_paths::NativeGlobalPaths;
use graphit_core::config::{ConfigKey, ConfigStore};
use graphit_hub::native::{RenderContext, SkillDescriptor};
use std::path::Path;

async fn command(f: &Fixture, args: &[&str]) -> std::process::Output {
    let installation = f.installation.root().to_owned();
    let cwd = f.temp.path().to_owned();
    let args = args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_graphit"))
            .env_clear()
            .env("HOME", &cwd)
            .current_dir(cwd)
            .arg("--global-dir")
            .arg(installation)
            .args(args)
            .output()
            .unwrap()
    })
    .await
    .unwrap()
}
fn outcome(output: &std::process::Output) -> InstallOutcome {
    assert!(
        output.status.success(),
        "Global publication failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let result: InstallOutcome = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result.failure.is_none());
    result
}
fn sets(f: &Fixture, result: &InstallOutcome, root: &Path, task_name: &str) {
    assert_eq!(result.skills.len(), 5);
    let mut companions = 0;
    for effect in &result.skills {
        assert!(!effect.observation_failed);
        assert_eq!(effect.pending, Some(false));
        assert!(effect.publication.is_some());
        let directory = root.join(&effect.name);
        assert_eq!(Path::new(&effect.path), directory);
        let bytes = fs::read(directory.join("SKILL.md")).unwrap();
        let cancel = Cancellation::default();
        let mut context = RenderContext::new(
            f.policy.content(),
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let skill = SkillDescriptor::parse(&bytes, &mut context).unwrap();
        assert_eq!(skill.name(), effect.name);
        assert!(!skill.body().contains("{{"));
        assert!(!skill.body().contains("docs-only-for-A"));
        assert!(!skill.body().contains("docs-only-for-B"));
        assert!(!skill.body().contains(f.project.root().to_str().unwrap()));
        if effect.module == "task" {
            assert_eq!(skill.name(), task_name);
        }
        if effect.module == "knowledge" {
            assert!(skill.body().contains("active PROJECT"));
            assert!(
                skill.body().contains(
                    &f.project
                        .brand()
                        .mcp_tool_name(&["module", "skill"])
                        .unwrap()
                )
            );
        }
        let module = [
            graphit_core::module_settings::Module::Task,
            graphit_core::module_settings::Module::Memory,
            graphit_core::module_settings::Module::Ast,
            graphit_core::module_settings::Module::Knowledge,
            graphit_core::module_settings::Module::Hub,
        ]
        .into_iter()
        .find(|m| m.name() == effect.module)
        .unwrap();
        for reference in graphit_runtime::skill_instructions::skill_templates(module)
            .unwrap()
            .references()
        {
            let text = fs::read_to_string(directory.join(reference.relative_path)).unwrap();
            assert!(!text.is_empty());
            assert!(!text.contains("{{"));
            companions += 1;
        }
    }
    assert_eq!(companions, 10);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_global_publication_is_neutral_shared_and_preserves_foreign_files() {
    let f = Fixture::new();
    let home = f.temp.path().join("native-home Δ");
    fs::create_dir(&home).unwrap();
    let b = f.temp.path().join("project-b");
    fs::create_dir(&b).unwrap();
    let project_b = ProjectPaths::at(&b, &f.installation).unwrap();
    ProjectIdentityStore::at(&project_b)
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .ensure(&ProjectMetadata::new("global-source-b").unwrap())
        .unwrap();
    for (project, directory, marker) in [
        (&f.project, "docs-only-for-A", "Explicit origin A"),
        (&project_b, "docs-only-for-B", "Explicit origin B"),
    ] {
        ConfigStore::project(project)
            .unwrap()
            .set(
                ConfigKey::new("knowledge.docs_dir").unwrap(),
                serde_json::json!(directory),
                Duration::from_secs(5),
                &Cancellation::default(),
            )
            .unwrap();
        let raw = project
            .root()
            .join(project.brand().directory())
            .join("rules/skills/task");
        fs::create_dir_all(&raw).unwrap();
        fs::write(raw.join("entry.md"), format!("{marker} {{{{ brand }}}} {{{{ mcp_tool_task_get }}}}\n[Execution](references/execution.md)\n")).unwrap();
    }
    // Genuine local Hub publication, preserving license/metadata through Global compilation.
    super::compiled_skills::local_hub_skill(&f);
    let cancel = Cancellation::default();
    let original = f.policy.request_context(Instant::now(), &cancel).unwrap();
    let environment = vec![(
        std::ffi::OsString::from("QWEN_HOME"),
        std::ffi::OsString::from("~/.agents"),
    )];
    let native_paths = NativeGlobalPaths::capture(&home, f.temp.path(), &environment);
    // Global defaults work with no PROJECT retained by the owner.
    let empty = ForegroundSource::start(
        &f.installation,
        &[],
        0,
        &f.config,
        NativeBinding::Codex,
        &original,
    )
    .unwrap()
    .with_global_native_paths(native_paths.clone())
    .unwrap();
    let empty_stop = Cancellation::default();
    let empty_retained = empty_stop.clone();
    let empty_job = tokio::spawn(async move { empty.run(&empty_retained).await });
    // Bootstrap creates the private Installation before authored raw input is written.
    let global_memory = f.installation.root().join("rules/skills/memory");
    fs::create_dir_all(&global_memory).unwrap();
    fs::write(
        global_memory.join("entry.md"),
        "Global Memory {{ brand }} {{ mcp_tool_memory_list }}\n",
    )
    .unwrap();
    let defaults = command(
        &f,
        &[
            "roles",
            "install",
            "--destination",
            "global",
            "--agent",
            "codex",
            "--role",
            "scribe",
        ],
    )
    .await;
    let defaults = outcome(&defaults);
    let task = f.project.brand().skill_name("task").unwrap();
    let shared = home.join(".agents/skills");
    sets(&f, &defaults, &shared, &task);
    assert!(
        !fs::read_to_string(shared.join(&task).join("SKILL.md"))
            .unwrap()
            .contains("Explicit origin A")
    );
    f.stop(empty_stop, empty_job).await;
    let original = f.policy.request_context(Instant::now(), &cancel).unwrap();
    let owner = ForegroundSource::start(
        &f.installation,
        &[
            ProjectBootstrapInput {
                project: &f.project,
                explicit_version: "main",
                pin: None,
                detached_base: "",
            },
            ProjectBootstrapInput {
                project: &project_b,
                explicit_version: "main",
                pin: None,
                detached_base: "",
            },
        ],
        0,
        &f.config,
        NativeBinding::Codex,
        &original,
    )
    .unwrap()
    .with_global_native_paths(native_paths)
    .unwrap();
    let stop = Cancellation::default();
    let retained = stop.clone();
    let job = tokio::spawn(async move { owner.run(&retained).await });
    let selected = command(
        &f,
        &[
            "roles",
            "sync",
            "--destination",
            "global",
            "--agent",
            "codex",
            "--role",
            "scribe",
            "--source-project",
            f.project.root().to_str().unwrap(),
            "--skill-bundle",
            "task=fixtures/task-instructions@one",
            "--skill-alias",
            "task=global-task-a",
        ],
    )
    .await;
    let selected = outcome(&selected);
    sets(&f, &selected, &shared, "global-task-a");
    let bytes = fs::read(shared.join("global-task-a/SKILL.md")).unwrap();
    let mut rendering = RenderContext::new(
        f.policy.content(),
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let skill = SkillDescriptor::parse(&bytes, &mut rendering).unwrap();
    assert!(skill.body().contains("Explicit origin A"));
    assert_eq!(skill.license(), Some("Apache-2.0"));
    assert_eq!(skill.compatibility(), Some("Graphit modules"));
    let selected_b = command(
        &f,
        &[
            "roles",
            "sync",
            "--destination",
            "global",
            "--agent",
            "qwen",
            "--role",
            "scribe",
            "--source-project",
            project_b.root().to_str().unwrap(),
            "--skill-alias",
            "task=global-task-b",
        ],
    )
    .await;
    let selected_b = outcome(&selected_b);
    sets(&f, &selected_b, &shared, "global-task-b");
    assert!(!shared.join("global-task-a").exists());
    assert!(!shared.join(&task).exists());
    let body_b = fs::read(shared.join("global-task-b/SKILL.md")).unwrap();
    assert!(String::from_utf8_lossy(&body_b).contains("Explicit origin B"));
    let foreign = shared.join("foreign-task");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("SKILL.md"), &body_b).unwrap();
    let refused = command(
        &f,
        &[
            "roles",
            "sync",
            "--destination",
            "global",
            "--agent",
            "qwen",
            "--role",
            "scribe",
            "--skill-alias",
            "task=foreign-task",
        ],
    )
    .await;
    assert!(!refused.status.success());
    assert!(
        serde_json::from_slice::<InstallOutcome>(&refused.stdout)
            .unwrap()
            .failure
            .is_some()
    );
    assert_eq!(fs::read(foreign.join("SKILL.md")).unwrap(), body_b);
    assert_eq!(
        fs::read(shared.join("global-task-b/SKILL.md")).unwrap(),
        body_b
    );
    let kiro = command(
        &f,
        &[
            "roles",
            "install",
            "--destination",
            "global",
            "--agent",
            "kiro-v3",
            "--role",
            "scribe",
        ],
    )
    .await;
    sets(&f, &outcome(&kiro), &home.join(".kiro/skills"), &task);
    let profile = fs::read_to_string(
        home.join(".kiro/agents")
            .join(format!("{}-scribe.md", f.project.brand().key())),
    )
    .unwrap();
    for forbidden in [
        f.project.root().to_str().unwrap(),
        project_b.root().to_str().unwrap(),
        "--project-dir",
        "--project-version",
        "docs-only-for-A",
        "docs-only-for-B",
    ] {
        assert!(
            !profile.contains(forbidden),
            "Global profile retained PROJECT context"
        );
    }
    assert!(profile.contains("--recipient"));
    let run = command(
        &f,
        &[
            "roles",
            "run",
            "--destination",
            "global",
            "--role",
            "scribe",
            "--prompt",
            "must refuse before native launch",
        ],
    )
    .await;
    assert!(!run.status.success());
    assert!(run.stdout.is_empty());
    assert!(!home.join(".graphit").exists());
    f.stop(stop, job).await;
}
