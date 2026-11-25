//! Configure a native installation and initialize an explicitly selected project.
use clap::Args;
use graphit_core::{
    config::{ConfigKey, ConfigStore},
    file_io::SnapshotContext,
    project_identity::{ProjectId, ProjectIdentity, ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::NativeBinding;
use std::{fs, io, path::PathBuf, time::Instant};

pub(crate) struct Arguments {
    pub agent: NativeBinding,
    pub project_dir: PathBuf,
    pub project_id: Option<ProjectId>,
    pub name: Option<String>,
}

#[derive(Args)]
pub(crate) struct InitArguments {
    /// Agent binding. Uses the configured agent or defaults to Codex.
    #[arg(long)]
    pub agent: Option<NativeBinding>,
    /// Project root to initialize. A dot discovers the current project.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
    /// Initial project ULID. An existing project must have this ID.
    #[arg(long)]
    pub project_id: Option<ProjectId>,
    /// Initial project name. Existing metadata is preserved.
    #[arg(long)]
    pub name: Option<String>,
    /// Use explicit Coordinator sources when running from a development build.
    #[arg(long)]
    pub coordinator_source: Option<PathBuf>,
}

pub(crate) struct Configuration {
    pub agent: NativeBinding,
    pub project: (ProjectPaths, ProjectIdentity),
}

pub(crate) fn initialize(
    arguments: Arguments,
    installation: &InstallationPaths,
    original: &SnapshotContext,
) -> io::Result<Configuration> {
    original.check()?;
    let root = fs::canonicalize(installation.start_directory().join(arguments.project_dir))?;
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "project directory required",
        ));
    }
    match fs::canonicalize(installation.root()) {
        Ok(global) if global == root => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "installation directory is not a project",
            ));
        }
        Ok(_) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    let metadata = match &arguments.name {
        Some(name) => ProjectMetadata::new(name)?,
        None => ProjectMetadata::for_directory(&root)?,
    };
    let paths = ProjectPaths::at(root, installation)?;
    original.check()?;
    let directory = paths.brand_directory();
    match fs::create_dir(&directory) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(&directory)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "project configuration must be a directory, not a link or file",
                ));
            }
        }
        Err(error) => return Err(error),
    }
    let mut writer =
        ProjectIdentityStore::at(&paths).writer(remaining(original)?, original.cancellation())?;
    let identity = match arguments.project_id {
        Some(id) => writer.ensure_with_id(&metadata, id)?,
        None => writer.ensure(&metadata)?,
    };
    Ok(Configuration {
        agent: arguments.agent,
        project: (paths, identity),
    })
}

pub(crate) fn persist_agent(
    configuration: &Configuration,
    installation: &InstallationPaths,
    original: &SnapshotContext,
) -> io::Result<()> {
    ConfigStore::global(installation)?.set(
        ConfigKey::new("agent")?,
        serde_json::Value::String(configuration.agent.as_str().into()),
        remaining(original)?,
        original.cancellation(),
    )?;
    Ok(())
}

fn remaining(original: &SnapshotContext) -> io::Result<std::time::Duration> {
    original.check()?;
    original
        .required_deadline()?
        .checked_duration_since(Instant::now())
        .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use graphit_core::{brand::Brand, file_lock::Cancellation};
    use std::time::Duration;

    fn context() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(10),
            Cancellation::default(),
        )
    }

    fn configure(
        arguments: Arguments,
        installation: &InstallationPaths,
        original: &SnapshotContext,
    ) -> io::Result<Configuration> {
        let configuration = initialize(arguments, installation, original)?;
        persist_agent(&configuration, installation, original)?;
        Ok(configuration)
    }

    #[derive(Parser)]
    struct InitParser {
        #[command(flatten)]
        arguments: InitArguments,
    }

    #[test]
    fn init_parser_defaults_to_the_current_directory_without_a_required_agent() {
        let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let parsed = InitParser::try_parse_from(["init", "--project-id", id]).unwrap();
        assert_eq!(parsed.arguments.project_id, Some(id.parse().unwrap()));
        assert_eq!(parsed.arguments.project_dir, PathBuf::from("."));
        assert!(parsed.arguments.agent.is_none());
        for invalid in [
            "invalid",
            "00000000000000000000000000",
            "01arz3ndektsv4rrffq69g5fav",
        ] {
            assert!(
                InitParser::try_parse_from([
                    "init",
                    "--agent",
                    "codex",
                    "--project-dir",
                    ".",
                    "--project-id",
                    invalid,
                ])
                .is_err()
            );
        }
    }

    #[test]
    fn explicit_project_id_reuses_identity_and_conflict_preserves_agent_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            &project,
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        let make = |id, agent, name: &str| Arguments {
            agent,
            project_dir: project.clone(),
            project_id: Some(id),
            name: Some(name.into()),
        };
        let first = configure(
            make(id, NativeBinding::Codex, "first"),
            &installation,
            &context(),
        )
        .unwrap();
        let (paths, first) = first.project;
        assert_eq!(first.id(), id);
        let same = configure(
            make(id, NativeBinding::Codex, "other"),
            &installation,
            &context(),
        )
        .unwrap();
        assert_eq!(same.project.1, first);
        let identity_file = paths
            .root()
            .join(installation.brand().directory())
            .join("project.json");
        let identity_before = fs::read(&identity_file).unwrap();
        let config_before = fs::read(installation.config_file()).unwrap();
        let conflict = "01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap();
        let result = configure(
            make(conflict, NativeBinding::KiroV3, "conflict"),
            &installation,
            &context(),
        );
        assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::InvalidInput));
        assert_eq!(fs::read(identity_file).unwrap(), identity_before);
        assert_eq!(fs::read(installation.config_file()).unwrap(), config_before);
    }

    #[test]
    fn new_project_and_repeat_use_real_identity_without_replacing_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            &project,
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let make = |name: &str| Arguments {
            agent: NativeBinding::KiroV3,
            project_dir: project.clone(),
            project_id: None,
            name: Some(name.into()),
        };
        let first = configure(make("first-name"), &installation, &context()).unwrap();
        let (_, first) = first.project;
        let second = configure(make("other-name"), &installation, &context()).unwrap();
        let (_, second) = second.project;
        assert_eq!(first, second);
        assert_eq!(
            ConfigStore::global(&installation)
                .unwrap()
                .load()
                .unwrap()
                .get::<String>(&ConfigKey::new("agent").unwrap())
                .unwrap()
                .as_deref(),
            Some("kiro-v3")
        );
    }

    #[test]
    fn installation_as_project_is_rejected_before_configuration_changes() {
        let temp = tempfile::tempdir().unwrap();
        let installation =
            InstallationPaths::resolve(Brand::default(), temp.path(), None, Some(temp.path()))
                .unwrap();
        let result = configure(
            Arguments {
                agent: NativeBinding::KiroV3,
                project_dir: temp.path().into(),
                project_id: None,
                name: None,
            },
            &installation,
            &context(),
        );
        assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::InvalidInput));
        assert!(!installation.config_file().exists());
        assert!(!temp.path().join(".graphit").exists());
    }

    #[test]
    fn cancelled_setup_creates_no_installation_or_project_state() {
        let temp = tempfile::tempdir().unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let original = context();
        original.cancellation().cancel();
        let result = configure(
            Arguments {
                agent: NativeBinding::Codex,
                project_dir: temp.path().into(),
                project_id: None,
                name: None,
            },
            &installation,
            &original,
        );
        assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::Interrupted));
        assert!(!installation.root().exists());
        assert!(!temp.path().join(".graphit").exists());
    }
}
