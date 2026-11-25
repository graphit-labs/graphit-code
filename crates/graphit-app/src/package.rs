//! Native package CLI; receives the caller's actual configuration and cancellation.
use clap::{Args, Subcommand};
use graphit_core::{
    artifact_store::{
        CapturedSourceReader, CopyPolicy, RetainedFileSet, SnapshotPolicy, SourceReadLimits,
    },
    config::ConfigSnapshot,
    file_lock::Cancellation,
};
use graphit_hub::{
    native_package::{self, PackageKind},
    policy::HubResourcePolicy,
};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Instant,
};

#[derive(Args)]
pub(super) struct Arguments {
    #[command(subcommand)]
    command: PackageCommand,
}
#[derive(Subcommand)]
enum PackageCommand {
    /// Install a native corpus in the installation's global cache.
    Install(InstallArguments),
    /// Package a completed native payload directory.
    Export {
        #[arg(long)]
        kind: PackageKind,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Validate physical package structure and every payload body.
    Inspect {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        kind: Option<PackageKind>,
    },
    /// Publish a validated native tree into a missing directory.
    Import {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        kind: Option<PackageKind>,
    },
}

#[derive(Args)]
pub(super) struct InstallArguments {
    #[arg(long)]
    pub input: PathBuf,
    #[arg(long)]
    pub producer: graphit_core::project_identity::ProjectId,
    #[arg(long)]
    pub id: String,
    #[arg(long)]
    pub version: String,
    #[arg(long)]
    pub kind: Option<PackageKind>,
}
pub(super) enum Operation {
    Install(InstallArguments),
    Physical(PhysicalCommand),
}
pub(super) enum PhysicalCommand {
    Export {
        kind: PackageKind,
        source: PathBuf,
        output: PathBuf,
    },
    Inspect {
        input: PathBuf,
        kind: Option<PackageKind>,
    },
    Import {
        input: PathBuf,
        output: PathBuf,
        kind: Option<PackageKind>,
    },
}
impl Arguments {
    pub(super) fn is_install(&self) -> bool {
        matches!(self.command, PackageCommand::Install(_))
    }
    pub(super) fn classify(self) -> Operation {
        match self.command {
            PackageCommand::Install(args) => Operation::Install(args),
            PackageCommand::Export {
                kind,
                source,
                output,
            } => Operation::Physical(PhysicalCommand::Export {
                kind,
                source,
                output,
            }),
            PackageCommand::Inspect { input, kind } => {
                Operation::Physical(PhysicalCommand::Inspect { input, kind })
            }
            PackageCommand::Import {
                input,
                output,
                kind,
            } => Operation::Physical(PhysicalCommand::Import {
                input,
                output,
                kind,
            }),
        }
    }
}

/// Run the real package commands with the shared caller config and cancellation.
pub(super) fn run(
    command: PhysicalCommand,
    config: &ConfigSnapshot,
    cancellation: &Cancellation,
    output: &mut impl Write,
) -> ExitCode {
    match execute(command, config, cancellation) {
        Ok(kind) => match serde_json::to_writer(
            &mut *output,
            &serde_json::json!({"status":"completed","kind":kind.to_string()}),
        )
        .map_err(io::Error::other)
        .and_then(|()| writeln!(output))
        {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {} package: {error}", config.brand().binary_name());
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("error: {} package: {error}", config.brand().binary_name());
            ExitCode::FAILURE
        }
    }
}
fn output_path(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let name = absolute.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let parent = absolute.parent().ok_or(io::ErrorKind::InvalidInput)?;
    Ok(fs::canonicalize(parent)?.join(name))
}
fn source_path(path: &Path, directory: bool) -> io::Result<PathBuf> {
    let metadata = fs::metadata(path)?;
    if if directory {
        !metadata.is_dir()
    } else {
        !metadata.is_file()
    } {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    fs::canonicalize(path)
}
fn execute(
    command: PhysicalCommand,
    config: &ConfigSnapshot,
    cancellation: &Cancellation,
) -> Result<PackageKind, Box<dyn std::error::Error>> {
    let policy = HubResourcePolicy::resolve(config)?;
    let request = policy.request_context(Instant::now(), cancellation)?;
    let limits = policy.copy();
    let snapshot = SnapshotPolicy::new(
        CopyPolicy::FollowLinks,
        limits,
        RetainedFileSet::new([], policy.retained())?,
        policy.retained(),
    )?;
    let read_limits = SourceReadLimits::new(64 * 1024, 64 * 1024, limits.total_file_bytes_limit())?;
    match command {
        PhysicalCommand::Export {
            kind,
            source,
            output,
        } => {
            let source = source_path(&source, true)?;
            let output = output_path(&output)?;
            if output.starts_with(&source) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "package output is inside the source",
                )
                .into());
            }
            let captured = CapturedSourceReader::capture(
                &source,
                graphit_fs::NodeKind::Directory,
                &snapshot,
                read_limits,
                Arc::clone(request.snapshot()),
            )?;
            Ok(native_package::export(
                &captured,
                kind,
                &output,
                request.snapshot(),
            )?)
        }
        PhysicalCommand::Inspect { input, kind } => {
            let input = source_path(&input, false)?;
            let captured = CapturedSourceReader::capture(
                &input,
                graphit_fs::NodeKind::File,
                &snapshot,
                read_limits,
                Arc::clone(request.snapshot()),
            )?;
            Ok(native_package::inspect(
                &captured,
                kind,
                limits,
                request.snapshot(),
            )?)
        }
        PhysicalCommand::Import {
            input,
            output,
            kind,
        } => {
            let input = source_path(&input, false)?;
            let output = output_path(&output)?;
            let captured = CapturedSourceReader::capture(
                &input,
                graphit_fs::NodeKind::File,
                &snapshot,
                read_limits,
                Arc::clone(request.snapshot()),
            )?;
            Ok(native_package::import(
                &captured,
                kind,
                &output,
                limits,
                request.snapshot(),
            )?)
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn configured_package_paths_follow_links_and_keep_kind_validation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("payload");
        fs::create_dir(&root).unwrap();
        let archive = temp.path().join("payload.zip");
        fs::write(&archive, b"ordinary configured file").unwrap();
        let directory_link = temp.path().join("source-link");
        let file_link = temp.path().join("archive-link");
        std::os::unix::fs::symlink(&root, &directory_link).unwrap();
        std::os::unix::fs::symlink(&archive, &file_link).unwrap();
        assert_eq!(
            source_path(&directory_link, true).unwrap(),
            fs::canonicalize(&root).unwrap()
        );
        assert_eq!(
            source_path(&file_link, false).unwrap(),
            fs::canonicalize(&archive).unwrap()
        );
        assert!(source_path(&file_link, true).is_err());
        assert!(source_path(&directory_link, false).is_err());
    }
}
