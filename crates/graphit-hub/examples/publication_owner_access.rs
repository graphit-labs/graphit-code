//! Actual Hub + Core ownership-restricted access fixture; Linux container only.
use std::io;

#[cfg(target_os = "linux")]
mod linux {
    use graphit_core::{
        artifact_store::{
            ArtifactChange, ArtifactLimits, ArtifactMode, ArtifactStore, CopyLimits, CopyPolicy,
        },
        brand::Brand,
        file_lock::Cancellation,
        native_path::NativePath,
        project_identity::{ProjectIdentityStore, ProjectMetadata},
        project_paths::{InstallationPaths, ProjectPaths},
    };
    use graphit_hub::{
        ArtifactId, ArtifactType, ConcreteVersion, NativeBinding, ProjectionLimits,
        ProjectionRequest, SourceRef,
        publication::{PublicationChange, ScopedCoordinator},
        registry::{DocumentLimits, HubLimits, HubRegistry},
    };
    use std::{
        fs, io,
        os::unix::{
            fs::{MetadataExt, PermissionsExt},
            process::CommandExt,
        },
        path::Path,
        process::Command,
        time::Duration,
    };

    fn require(condition: bool) -> io::Result<()> {
        if condition {
            Ok(())
        } else {
            Err(io::Error::other(
                "public Hub ownership-access assertion failed",
            ))
        }
    }
    pub fn run() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = fs::canonicalize(temp.path())?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755))?;
        fs::create_dir(root.join("project"))?;
        fs::create_dir(root.join("source"))?;
        let source = root.join("source/COMMAND.md");
        fs::write(&source, b"public ownership access")?;
        fs::set_permissions(&source, fs::Permissions::from_mode(0o204))?;
        require(
            Command::new("chown")
                .arg("0:2000")
                .arg(&source)
                .status()?
                .success(),
        )?;
        require(
            Command::new("chown")
                .arg("1000:1000")
                .arg(root.join("project"))
                .status()?
                .success(),
        )?;
        require(
            Command::new(std::env::current_exe()?)
                .arg("child")
                .arg(&root)
                .uid(1000)
                .gid(1000)
                .status()?
                .success(),
        )
    }
    pub fn child(root: &Path) -> io::Result<()> {
        let source_path = root.join("source/COMMAND.md");
        let source_metadata = fs::metadata(&source_path)?;
        require(
            source_metadata.uid() == 0
                && source_metadata.gid() == 2000
                && source_metadata.permissions().mode() & 0o777 == 0o204,
        )?;
        require(fs::read(&source_path)? == b"public ownership access")?;
        let installation = InstallationPaths::resolve(
            Brand::default(),
            root,
            None,
            Some(&root.join("installation")),
        )?;
        let project_root = root.join("project");
        fs::create_dir(project_root.join(installation.brand().directory()))?;
        let project = ProjectPaths::at(&project_root, &installation)?;
        let cancel = Cancellation::default();
        let identity = ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(2), &cancel)?
            .ensure(&ProjectMetadata::new("consumer")?)?;
        let projection =
            ProjectionLimits::new(4096, 32 * 1024, 100, 8192, 100).map_err(io::Error::other)?;
        let artifacts = ArtifactLimits::new(
            1024 * 1024,
            8192,
            CopyLimits::new(64 * 1024 * 1024, 10000, 1024 * 1024, 64)?,
        )?;
        let hub_limits = HubLimits::new(
            DocumentLimits {
                bytes: 1024 * 1024,
                entries: 100,
                key_bytes: 8192,
                json_nodes: 100000,
                json_string_bytes: 32 * 1024,
            },
            projection,
            artifacts,
            graphit_hub::materialization::MaterializationLimits {
                output: graphit_core::artifact_store::CopyLimits::new(
                    512 * 1024 * 1024,
                    100_000,
                    8 * 1024 * 1024,
                    128,
                )
                .unwrap(),
                snapshots: graphit_core::artifact_store::CopyLimits::new(
                    16 * 1024 * 1024,
                    512,
                    32 * 1024,
                    16,
                )
                .unwrap(),
                lineage: graphit_hub::materialization::LineageLimits::new(4096, 8192, 32 * 1024)
                    .unwrap(),
            },
        )?;
        let registry = HubRegistry::project(&project, &identity, hub_limits)?;
        let original = std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
            std::time::Instant::now() + std::time::Duration::from_secs(5),
            graphit_core::file_lock::Cancellation::default(),
        ));
        let store = ArtifactStore::new(
            graphit_core::artifact_store::CapturedPublicationScope::project(
                &project,
                identity.id(),
                graphit_core::artifact_store::ScopeLimits::new(1, 16 * 1024, 1024).unwrap(),
                &original,
            )
            .unwrap(),
            artifacts,
        );
        let coordinator = ScopedCoordinator::new(
            registry.clone(),
            store.clone(),
            NativePath::new(project.root().to_owned()).unwrap(),
        )?;
        let source = SourceRef::local_link(
            NativePath::new(root.join("source"))?,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV"
                .parse()
                .map_err(io::Error::other)?,
            ArtifactType::Command,
            ArtifactId::new("org/repo", projection).map_err(io::Error::other)?,
            ConcreteVersion::new("v1", projection).map_err(io::Error::other)?,
            projection,
        )
        .map_err(io::Error::other)?;
        let request = || ProjectionRequest {
            binding: NativeBinding::Codex,
            source: &source,
            alias: None,
            entry_name: None,
        };
        let mode = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
        let mut writer = coordinator
            .writer(&crate::request_context(Duration::from_secs(2), &cancel))
            .map_err(io::Error::other)?;
        require(
            writer
                .publish(request(), mode, None)
                .map_err(io::Error::other)?
                .change
                == PublicationChange::Physical(ArtifactChange::Created),
        )?;
        let destination = project.root().join(".codex/commands/repo.md");
        require(
            fs::metadata(&destination)?.uid() == 1000
                && fs::metadata(&destination)?.permissions().mode() & 0o777 == 0o204,
        )?;
        require(fs::read(&destination).unwrap_err().kind() == io::ErrorKind::PermissionDenied)?;
        let hub_before = fs::read(registry.path())?;
        let result = writer
            .publish(request(), mode, None)
            .map_err(io::Error::other)?;
        require(result.change == PublicationChange::Physical(ArtifactChange::Unchanged))?;
        require(fs::read(registry.path())? == hub_before)?;
        require(fs::metadata(&destination)?.permissions().mode() & 0o777 == 0o204)?;
        require(fs::read(&destination).unwrap_err().kind() == io::ErrorKind::PermissionDenied)?;
        require(
            writer.snapshot().intent().is_none()
                && writer
                    .physical_snapshot()
                    .map_err(io::Error::other)?
                    .unwrap()
                    .pending()
                    .is_none(),
        )?;
        writer
            .remove(&graphit_core::artifact_store::ArtifactKey::new(
                "codex", "command", "org/repo",
            )?)
            .map_err(io::Error::other)?;
        require(!destination.exists())?;
        println!(
            "{{\"public_hub_flow\":true,\"source_uid\":0,\"source_gid\":2000,\"destination_uid\":1000,\"mode\":\"0204\",\"publication\":\"Unchanged\",\"mode_restored\":true,\"pending_absent\":true,\"public_remove\":true}}"
        );
        Ok(())
    }
}
fn main() -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let mut args = std::env::args_os().skip(1);
        match args.next().as_deref().and_then(|v| v.to_str()) {
            Some("owned-access") if args.next().is_none() => linux::run(),
            Some("child") => {
                let root = args.next().ok_or(io::ErrorKind::InvalidInput)?;
                if args.next().is_some() {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                linux::child(std::path::Path::new(&root))
            }
            _ => Err(io::ErrorKind::InvalidInput.into()),
        }
    }
    #[cfg(not(target_os = "linux"))]
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(target_os = "linux")]
fn request_context<'a>(
    timeout: std::time::Duration,
    cancellation: &'a graphit_core::file_lock::Cancellation,
) -> graphit_hub::policy::RequestContext<'a> {
    graphit_hub::policy::RequestContext::new(
        graphit_hub::native::ContentLimits {
            max_input_bytes: 8 * 1024 * 1024,
            max_output_bytes: 16 * 1024 * 1024,
            max_depth: 32,
            max_nodes: 100_000,
            max_scalar_bytes: 1024 * 1024,
            max_collection_entries: 10_000,
        },
        cancellation,
        std::time::Instant::now().checked_add(timeout).unwrap(),
    )
    .unwrap()
}
