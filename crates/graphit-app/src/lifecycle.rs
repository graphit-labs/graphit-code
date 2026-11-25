//! Local ownership-aware project withdrawal and user installation removal.
use graphit_app::{
    foreground_project_control::{ProjectCommand, client as project_control},
    git_records,
    project_coordinator_install::client as coordinator,
    service_entry::ServiceCommand,
};
use graphit_core::{
    artifact_store::{ArtifactStore, PublicationScope},
    config::ConfigSnapshot,
    git_record_event::GitRecordFacts,
    installation_identity::InstallationIdentityStore,
    native_path::NativePath,
    project_identity::{ProjectIdentity, ProjectIdentityStore},
    project_inventory::{GlobalProjectInventory, ProjectInventorySnapshot},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::{
    AdapterId, NativeBinding,
    policy::{HubResourcePolicy, RequestContext},
    publication::ScopedCoordinator,
    registry::HubRegistry,
};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) mod installed_package;

type Failure = Box<dyn std::error::Error>;

const DATA_DIRECTORIES: &[&str] = &[
    "ast", "cache", "daemon", "models", "projects", "runtime", "services", "users",
];
const DATA_OWNERSHIP: &str = "installation-data-ownership.json";
const DIRECTORY_OWNER: &str = ".graphit-installation-owner";

pub(crate) fn record_data_ownership(
    installation: &InstallationPaths,
    original: &RequestContext<'_>,
) -> io::Result<()> {
    let marker = installation.root().join(DATA_OWNERSHIP);
    if marker.try_exists()?
        || InstallationIdentityStore::new(installation)?
            .load()?
            .is_provisioned()
    {
        return Ok(());
    }
    graphit_fs::verify_private_directory_path(installation.root())?;
    let mut owned = std::collections::BTreeMap::new();
    let result = (|| -> io::Result<()> {
        for name in DATA_DIRECTORIES {
            original.check()?;
            let path = installation.root().join(name);
            match graphit_fs::create_private_directory(&path) {
                Ok(directory) => {
                    drop(directory);
                    let owner = uuid::Uuid::new_v4().to_string();
                    let mut proof = graphit_fs::create_private_file(&path.join(DIRECTORY_OWNER))?;
                    proof.write_all(owner.as_bytes())?;
                    proof.sync_all()?;
                    owned.insert(*name, owner);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    })();
    let mut staged = tempfile::Builder::new()
        .prefix(".installation-data-")
        .make_in(installation.root(), graphit_fs::create_private_file)?;
    serde_json::to_writer(&mut staged, &owned).map_err(io::Error::other)?;
    staged.flush()?;
    staged.as_file().sync_all()?;
    staged
        .persist_noclobber(marker)
        .map_err(|error| error.error)?;
    result
}

#[derive(clap::Args)]
pub(crate) struct RemoveArguments {
    /// Project root to remove. A dot discovers the current project.
    #[arg(long, default_value = ".")]
    project_dir: PathBuf,
    /// Withdraw only the integrations owned for this agent.
    #[arg(long)]
    agent: Option<NativeBinding>,
}

pub(crate) struct Context<'a> {
    pub installation: &'a InstallationPaths,
    pub variables: &'a [(OsString, OsString)],
    pub config: &'a ConfigSnapshot,
    pub policy: &'a HubResourcePolicy,
    pub original: &'a RequestContext<'a>,
}

impl Context<'_> {
    fn remaining(&self) -> io::Result<std::time::Duration> {
        self.original
            .remaining()?
            .ok_or_else(|| io::ErrorKind::InvalidInput.into())
    }

    fn inventory(&self) -> io::Result<ProjectInventorySnapshot> {
        GlobalProjectInventory::at(self.installation, self.policy.content().max_input_bytes)?
            .load(self.original.snapshot())
    }

    pub async fn remove(
        &self,
        arguments: RemoveArguments,
        outcome: &mut Value,
    ) -> Result<(), Failure> {
        outcome["failed_step"] = "project_selection".into();
        let selected = self
            .installation
            .start_directory()
            .join(&arguments.project_dir);
        let project = if arguments.project_dir == Path::new(".") {
            ProjectPaths::discover(selected, self.installation)?
        } else {
            ProjectPaths::at(selected, self.installation)?
        };
        self.remove_checkout(&project, arguments.agent, true, outcome)
            .await
    }

    async fn remove_checkout(
        &self,
        project: &ProjectPaths,
        agent: Option<NativeBinding>,
        remove_identity: bool,
        outcome: &mut Value,
    ) -> Result<(), Failure> {
        self.original.check()?;
        outcome["project"] = json!(project.root());
        outcome["agent"] = json!(agent);
        let identity_store = ProjectIdentityStore::at(project);
        let Some(identity) = identity_store.load()? else {
            outcome["state"] = "absent".into();
            return Ok(());
        };
        let installed = InstallationIdentityStore::new(self.installation)?
            .load()?
            .unit_id()
            .is_some();
        if installed {
            outcome["failed_step"] = "daemon".into();
            crate::cli::ensure_source_with_package(
                self.installation,
                self.variables,
                self.policy.content(),
                self.original,
                Some(project.root()),
                remove_identity,
            )
            .await?;
            let version = graphit_app::task_entry::captured_project_version(
                self.installation,
                project.root(),
                self.original,
            )
            .await?;
            outcome["failed_step"] = "agent_integrations".into();
            outcome["agent_integrations"] = json!([]);
            for binding in NativeBinding::ALL
                .into_iter()
                .filter(|binding| agent.is_none_or(|agent| agent == *binding))
            {
                let body = coordinator::prepare(
                    coordinator::Arguments {
                        command: coordinator::Command::Remove { binding },
                    },
                    project,
                    Some(version.clone()),
                    self.policy,
                    self.original,
                )?;
                let mut receipt = Vec::new();
                let status = coordinator::run(
                    body,
                    self.installation,
                    self.policy,
                    self.original,
                    &mut receipt,
                )
                .await?;
                outcome["agent_integrations"].as_array_mut().expect("integration receipts")
                    .push(json!({"agent":binding,"result":serde_json::from_slice::<Value>(&receipt)?}));
                if status != std::process::ExitCode::SUCCESS {
                    return Err(io::Error::other("agent integration removal failed").into());
                }
            }
            outcome["failed_step"] = "owned_files".into();
            self.remove_owned_files(project, &identity, agent, outcome)?;
        }
        if agent.is_some() {
            outcome["state"] = if installed { "agent_removed" } else { "absent" }.into();
            return Ok(());
        }

        let inventory = self.inventory()?;
        outcome["failed_step"] = "git_hooks".into();
        if self.git_hooks_shared(project, &inventory)? {
            outcome["git_hooks"] = "shared".into();
        } else {
            let mut receipt = Vec::new();
            let status = git_records::run(
                git_records::Arguments {
                    command: git_records::Command::Remove,
                },
                project,
                self.installation,
                self.policy,
                self.original,
                self.config,
                &mut io::empty(),
                &mut receipt,
            )
            .await?;
            outcome["git_hooks"] = serde_json::from_slice(&receipt)?;
            if status != std::process::ExitCode::SUCCESS {
                return Err(io::Error::other("Git hook removal failed").into());
            }
        }
        if inventory.get(identity.id()).is_some_and(|entry| {
            entry
                .checkouts()
                .any(|checkout| checkout.path() == project.root())
        }) {
            outcome["failed_step"] = "checkout_retirement".into();
            let mut receipt = Vec::new();
            let status = project_control::run(
                ProjectCommand::RetireCheckout {
                    project_id: identity.id(),
                    project_dir: NativePath::new(project.root().to_owned())?,
                },
                self.installation,
                self.policy,
                self.original,
                &mut receipt,
            )
            .await?;
            outcome["checkout"] = serde_json::from_slice(&receipt)?;
            if status != std::process::ExitCode::SUCCESS {
                return Err(io::Error::other("checkout retirement failed").into());
            }
        }
        if remove_identity {
            outcome["failed_step"] = "project_document".into();
            self.original.check()?;
            outcome["project_document_removed"] = identity_store
                .writer(self.remaining()?, self.original.cancellation())?
                .remove(identity.id())?
                .into();
        }
        outcome["state"] = "removed".into();
        Ok(())
    }

    fn git_hooks_shared(
        &self,
        project: &ProjectPaths,
        inventory: &ProjectInventorySnapshot,
    ) -> io::Result<bool> {
        let inspect = |root: &Path| {
            GitRecordFacts::inspect(root, self.remaining()?, self.original.cancellation())
        };
        let Some(facts) = inspect(project.root())? else {
            return Ok(false);
        };
        for sibling in inventory.projects().flat_map(|entry| entry.checkouts()) {
            let path = sibling.path();
            if path != project.root()
                && path.try_exists()?
                && inspect(path)?.is_some_and(|other| other.common_dir == facts.common_dir)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn remove_owned_files(
        &self,
        project: &ProjectPaths,
        identity: &ProjectIdentity,
        agent: Option<NativeBinding>,
        outcome: &mut Value,
    ) -> Result<(), Failure> {
        let scope = PublicationScope::project(project, identity.id())?;
        let slot = scope.publication_slot();
        let core = ArtifactStore::new(scope, self.policy.artifacts());
        let registry = HubRegistry::project(project, identity, self.policy.hub())?;
        let entries = registry.load()?;
        let owned = core.load()?;
        outcome["owned_files_removed"] = json!(0);
        let mut count = 0usize;
        let scoped = ScopedCoordinator::new(
            registry,
            core.clone(),
            NativePath::new(project.root().to_owned())?,
        )?;
        let mut writer = scoped.writer(self.original)?;
        for (kind, id, entry) in entries.entries_for_consumer(&slot) {
            for install in &entry.installations {
                if agent.is_none_or(|agent| install.target.includes(agent)) {
                    self.original.check()?;
                    writer.remove(&install.target.key(kind, id)?)?;
                    count += 1;
                    outcome["owned_files_removed"] = count.into();
                }
            }
        }
        drop(writer);
        let Some(owned) = owned else {
            return Ok(());
        };
        let mut writer = core.writer_with_context(self.original.snapshot())?;
        for entry in owned
            .entries()
            .iter()
            .filter(|entry| entry.publication_slot() == slot)
        {
            let key = entry.key();
            let selected = match key.artifact_type() {
                "managed-role" => agent.is_none_or(|agent| key.adapter() == agent.as_str()),
                "module-skill" => agent.is_none_or(|agent| {
                    let adapter = skill_adapter(agent);
                    let shared = NativeBinding::ALL.into_iter().any(|other| {
                        other != agent
                            && skill_adapter(other) == adapter
                            && owned.entries().iter().any(|entry| {
                                entry.publication_slot() == slot
                                    && entry.key().artifact_type() == "managed-role"
                                    && entry.key().adapter() == other.as_str()
                            })
                    });
                    key.adapter() == adapter.as_str() && !shared
                }),
                _ => false,
            };
            if selected {
                self.original.check()?;
                match writer.remove(key) {
                    Ok(_) => count += 1,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                    Err(error) => return Err(error.into()),
                }
                outcome["owned_files_removed"] = count.into();
            }
        }
        Ok(())
    }

    pub async fn uninstall(&self, outcome: &mut Value) -> Result<(), Failure> {
        outcome["installation"] = json!(self.installation.root());
        outcome["failed_step"] = "package_inventory".into();
        let settings = InstallationIdentityStore::new(self.installation)?.load()?;
        let mut usage = installed_package::usage(self.original)?;
        let package = match usage.as_ref() {
            Some(usage) => {
                usage.plan_uninstall(self.installation, settings.unit_id(), self.original)?
            }
            None => installed_package::Removal::Unmanaged,
        };
        outcome["projects"] = json!([]);
        if let Some(unit) = settings.unit_id() {
            outcome["failed_step"] = "project_integrations".into();
            let inventory = self.inventory()?;
            for entry in inventory.projects() {
                for checkout in entry.checkouts() {
                    let mut project_outcome = json!({"project":checkout.path()});
                    let result = if checkout.path().try_exists()? {
                        let project = ProjectPaths::at(checkout.path(), self.installation)?;
                        if ProjectIdentityStore::at(&project)
                            .load()?
                            .is_some_and(|identity| identity.id() == entry.identity().id())
                        {
                            self.remove_checkout(&project, None, false, &mut project_outcome)
                                .await
                        } else {
                            project_outcome["state"] = "identity_changed".into();
                            Ok(())
                        }
                    } else {
                        project_outcome["state"] = "unavailable".into();
                        Ok(())
                    };
                    outcome["projects"]
                        .as_array_mut()
                        .expect("project receipts")
                        .push(project_outcome);
                    result?;
                }
            }
            outcome["failed_step"] = "global_integrations".into();
            let home = self
                .installation
                .home_directory()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
            let native = graphit_app::managed_roles::global_paths::NativeGlobalPaths::capture(
                home,
                self.installation.start_directory(),
                self.variables,
            );
            let store = ArtifactStore::new(
                PublicationScope::installation_instructions(
                    self.installation,
                    unit.clone(),
                    native.allowed_roots()?,
                )?,
                self.policy.artifacts(),
            );
            if let Some(owned) = store.load()? {
                let mut writer = store.writer_with_context(self.original.snapshot())?;
                for entry in owned.entries() {
                    self.original.check()?;
                    writer.remove(entry.key())?;
                }
            }
        }
        let guard = if self.installation.root().try_exists()? {
            Some(
                graphit_app::daemon_launcher::lifecycle_lock(self.installation, self.original)
                    .await?,
            )
        } else {
            None
        };
        if let Some(guard) = &guard {
            outcome["failed_step"] = "daemon".into();
            for operation in [ServiceCommand::Stop, ServiceCommand::Uninstall] {
                crate::cli::daemon_service_request_with_guard(
                    self.installation,
                    self.variables,
                    self.original,
                    &operation,
                    Some(guard),
                )
                .await?;
            }
            outcome["daemon"] = "removed".into();
        }
        if settings.is_provisioned() {
            outcome["failed_step"] = "local_data".into();
            purge_installation(self.installation, self.original, &mut outcome["local_data"])?;
        }
        outcome["failed_step"] = "package".into();
        installed_package::remove(
            package,
            usage.as_mut(),
            self.original,
            &mut outcome["package"],
        )?;
        Ok(())
    }
}

fn skill_adapter(binding: NativeBinding) -> AdapterId {
    if binding == NativeBinding::Codex {
        AdapterId::Antigravity
    } else {
        binding.adapter()
    }
}

fn purge_installation(
    installation: &InstallationPaths,
    original: &RequestContext<'_>,
    outcome: &mut Value,
) -> io::Result<()> {
    let root = installation.root();
    graphit_fs::verify_private_directory_path(root)?;
    let ownership = root.join(DATA_OWNERSHIP);
    let owned: std::collections::BTreeMap<String, String> =
        match graphit_core::local_files::read_regular(&ownership, 4096)? {
            Some((bytes, _)) => {
                graphit_fs::verify_private_file_path(&ownership)?;
                serde_json::from_slice(&bytes).map_err(io::Error::other)?
            }
            None => Default::default(),
        };
    if owned
        .keys()
        .any(|path| !DATA_DIRECTORIES.contains(&path.as_str()))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid installation data receipt",
        ));
    }
    *outcome = json!({"removed":[],"preserved":[]});
    for name in DATA_DIRECTORIES {
        original.check()?;
        let path = root.join(name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let proof = path.join(DIRECTORY_OWNER);
        let proven = if metadata.is_dir()
            && let Some(owner) = owned.get(*name)
            && graphit_fs::verify_private_file_path(&proof).is_ok()
        {
            graphit_core::local_files::read_regular(&proof, 128)?
                .is_some_and(|(bytes, _)| bytes.as_slice() == owner.as_bytes())
        } else {
            false
        };
        let result = if proven {
            fs::remove_dir_all(path)?;
            "removed"
        } else {
            "preserved"
        };
        outcome[result]
            .as_array_mut()
            .expect("installation data paths")
            .push(json!(name));
    }
    let config = graphit_core::config::ConfigStore::global(installation)?;
    let timeout = original.remaining()?.ok_or(io::ErrorKind::InvalidInput)?;
    let _config_lock = config.writer(timeout, original.cancellation())?;
    for name in [
        "auth.json",
        "project-inventory.json",
        "broker-agent-instructions.json",
        "daemon.pid",
        DATA_OWNERSHIP,
        "config.json",
    ] {
        original.check()?;
        let path = root.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                fs::remove_file(path)?;
                outcome["removed"]
                    .as_array_mut()
                    .expect("removed paths")
                    .push(json!(name));
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "owned installation file was replaced",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    // Keep the lock inodes available to concurrent callers after releasing the guards.
    Ok(())
}
