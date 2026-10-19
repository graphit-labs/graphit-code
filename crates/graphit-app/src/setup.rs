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

#[derive(Args)]
pub(crate) struct Arguments {
    /// Native agent binding; current Kiro uses kiro-v3.
    #[arg(long)]
    pub agent: NativeBinding,
    /// Initialize and maintain this existing project directory.
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    /// Initial immutable project ULID; an existing identity must have the same ID.
    #[arg(long, requires = "project_dir")]
    pub project_id: Option<ProjectId>,
    /// Initial project name; an existing identity and its metadata are preserved.
    #[arg(long, requires = "project_dir")]
    pub name: Option<String>,
}

pub(crate) struct Configuration {
    pub agent: NativeBinding,
    pub project: Option<(ProjectPaths, ProjectIdentity)>,
}

pub(crate) fn configure(
    arguments: Arguments,
    installation: &InstallationPaths,
    original: &SnapshotContext,
) -> io::Result<Configuration> {
    original.check()?;
    let project = arguments
        .project_dir
        .map(|directory| {
            let directory = if directory.is_absolute() {
                directory
            } else {
                installation.start_directory().join(directory)
            };
            let root = fs::canonicalize(directory)?;
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
            let brand_directory = root.join(installation.brand().directory());
            match fs::symlink_metadata(&brand_directory) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => (),
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "project configuration must be a directory, not a link or file",
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
            Ok((root, brand_directory, metadata))
        })
        .transpose()?;
    let remaining = || {
        original.check()?;
        original
            .required_deadline()?
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
    };
    let project = project
        .map(|(root, directory, metadata)| {
            original.check()?;
            match fs::create_dir(&directory) {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error),
            }
            let paths = ProjectPaths::at(root, installation)?;
            let mut writer =
                ProjectIdentityStore::at(&paths).writer(remaining()?, original.cancellation())?;
            let identity = match arguments.project_id {
                Some(id) => writer.ensure_with_id(&metadata, id)?,
                None => writer.ensure(&metadata)?,
            };
            Ok((paths, identity))
        })
        .transpose()?;
    ConfigStore::global(installation)?.set(
        ConfigKey::new("agent")?,
        serde_json::Value::String(arguments.agent.as_str().into()),
        remaining()?,
        original.cancellation(),
    )?;
    Ok(Configuration {
        agent: arguments.agent,
        project,
    })
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

    #[derive(Parser)]
    struct SetupParser {
        #[command(flatten)]
        arguments: Arguments,
    }

    #[test]
    fn project_id_parser_requires_project_directory_and_canonical_id() {
        let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let parsed = SetupParser::try_parse_from([
            "setup",
            "--agent",
            "codex",
            "--project-dir",
            ".",
            "--project-id",
            id,
        ])
        .unwrap();
        assert_eq!(parsed.arguments.project_id, Some(id.parse().unwrap()));
        assert!(
            SetupParser::try_parse_from(["setup", "--agent", "codex", "--project-id", id,])
                .is_err()
        );
        for invalid in [
            "invalid",
            "00000000000000000000000000",
            "01arz3ndektsv4rrffq69g5fav",
        ] {
            assert!(
                SetupParser::try_parse_from([
                    "setup",
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
            project_dir: Some(project.clone()),
            project_id: Some(id),
            name: Some(name.into()),
        };
        let first = configure(
            make(id, NativeBinding::Codex, "first"),
            &installation,
            &context(),
        )
        .unwrap();
        let (paths, first) = first.project.unwrap();
        assert_eq!(first.id(), id);
        let same = configure(
            make(id, NativeBinding::Codex, "other"),
            &installation,
            &context(),
        )
        .unwrap();
        assert_eq!(same.project.unwrap().1, first);
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
            project_dir: Some(project.clone()),
            project_id: None,
            name: Some(name.into()),
        };
        let first = configure(make("first-name"), &installation, &context()).unwrap();
        let (_, first) = first.project.unwrap();
        let second = configure(make("other-name"), &installation, &context()).unwrap();
        let (_, second) = second.project.unwrap();
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
                project_dir: Some(temp.path().into()),
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
                project_dir: Some(temp.path().into()),
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
