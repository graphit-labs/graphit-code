//! Native per-user service operations. Daemon PID/listener/job lifecycle belongs to the entrypoint.
use crate::services::{
    ServiceCommands, ServiceDefinition, ServiceFailure, ServicePlatform, installation_lock,
};
use graphit_core::file_io::SnapshotContext;
use graphit_process::Outcome;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct ServiceFiles {
    pub control_root: PathBuf,
    pub registration: PathBuf,
    pub login_registration: Option<PathBuf>,
}

pub struct UserService<'host> {
    pub definition: ServiceDefinition,
    pub files: ServiceFiles,
    pub commands: ServiceCommands<'host>,
}
impl UserService<'_> {
    fn validate(&self) -> io::Result<()> {
        self.definition.validate()?;
        if !self.files.control_root.is_absolute()
            || !self.files.registration.is_absolute()
            || self
                .files
                .login_registration
                .as_ref()
                .is_some_and(|path| !path.is_absolute() || path == &self.files.registration)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid per-user service paths",
            ));
        }
        Ok(())
    }
    fn unit(&self) -> String {
        format!("{}.service", self.definition.name)
    }
    fn launch_target(&self) -> String {
        format!("gui/{}/{}", self.definition.uid, self.definition.name)
    }

    pub async fn start(&self, original: &Arc<SnapshotContext>) -> Result<(), ServiceFailure> {
        self.validate()?;
        let arguments = match self.commands.platform() {
            ServicePlatform::Systemd => vec!["--user".into(), "start".into(), self.unit().into()],
            ServicePlatform::Launchd => vec![
                "bootstrap".into(),
                format!("gui/{}", self.definition.uid).into(),
                self.files.registration.clone().into_os_string(),
            ],
            ServicePlatform::TaskScheduler => vec![
                "/run".into(),
                "/tn".into(),
                self.definition.name.clone().into(),
            ],
        };
        self.commands.execute(original, arguments).await
    }
    pub async fn stop(&self, original: &Arc<SnapshotContext>) -> Result<(), ServiceFailure> {
        self.validate()?;
        let arguments = match self.commands.platform() {
            ServicePlatform::Systemd => vec!["--user".into(), "stop".into(), self.unit().into()],
            ServicePlatform::Launchd => vec!["bootout".into(), self.launch_target().into()],
            ServicePlatform::TaskScheduler => vec![
                "/end".into(),
                "/tn".into(),
                self.definition.name.clone().into(),
            ],
        };
        self.commands.execute(original, arguments).await
    }
    pub async fn restart(&self, original: &Arc<SnapshotContext>) -> Result<(), ServiceFailure> {
        self.validate()?;
        match self.commands.platform() {
            ServicePlatform::Systemd => {
                self.commands
                    .execute(
                        original,
                        vec!["--user".into(), "restart".into(), self.unit().into()],
                    )
                    .await
            }
            ServicePlatform::Launchd => {
                self.commands
                    .execute(
                        original,
                        vec!["kickstart".into(), "-k".into(), self.launch_target().into()],
                    )
                    .await
            }
            ServicePlatform::TaskScheduler => {
                self.stop(original).await?;
                self.start(original).await
            }
        }
    }
    /// Raw native result: an unsuccessful status query is not silently classified as absent.
    pub async fn status(&self, original: &Arc<SnapshotContext>) -> Result<Outcome, ServiceFailure> {
        self.validate()?;
        let arguments = match self.commands.platform() {
            ServicePlatform::Systemd => vec![
                "--user".into(),
                "show".into(),
                "--property=LoadState,ActiveState,UnitFileState,MainPID".into(),
                self.unit().into(),
            ],
            ServicePlatform::Launchd => vec!["print".into(), self.launch_target().into()],
            ServicePlatform::TaskScheduler => vec![
                "/query".into(),
                "/tn".into(),
                self.definition.name.clone().into(),
                "/xml".into(),
            ],
        };
        self.commands.capture(original, arguments).await
    }

    pub async fn install(
        &self,
        original: &Arc<SnapshotContext>,
        login: bool,
    ) -> Result<(), ServiceFailure> {
        self.validate()?;
        let _lock = installation_lock(&self.files.control_root)?;
        original.check().map_err(io::Error::other)?;
        let old = read_optional(&self.files.registration)?;
        let document = self.definition.document(self.commands.platform(), false)?;
        atomic_write(&self.files.registration, document.as_bytes())?;
        let result = match self.commands.platform() {
            ServicePlatform::Systemd => {
                match self
                    .commands
                    .execute(original, vec!["--user".into(), "daemon-reload".into()])
                    .await
                {
                    Ok(()) => {
                        self.commands
                            .execute(
                                original,
                                vec![
                                    "--user".into(),
                                    if login { "enable" } else { "disable" }.into(),
                                    self.unit().into(),
                                ],
                            )
                            .await
                    }
                    Err(error) => Err(error),
                }
            }
            ServicePlatform::Launchd => self.set_launch_login(login).map_err(ServiceFailure::Io),
            ServicePlatform::TaskScheduler => {
                match self
                    .commands
                    .execute(
                        original,
                        vec![
                            "/create".into(),
                            "/tn".into(),
                            self.definition.name.clone().into(),
                            "/xml".into(),
                            self.files.registration.clone().into_os_string(),
                            "/f".into(),
                        ],
                    )
                    .await
                {
                    Ok(()) => self.set_task_login(original, login).await,
                    Err(error) => Err(error),
                }
            }
        };
        if let Err(error) = result {
            if let Err(recovery) = restore(&self.files.registration, old.as_deref()) {
                return Err(ServiceFailure::Recovery {
                    operation: Box::new(error),
                    recovery: Box::new(ServiceFailure::Io(recovery)),
                });
            }
            if self.commands.platform() == ServicePlatform::Systemd
                && let Err(recovery) = self
                    .commands
                    .execute(original, vec!["--user".into(), "daemon-reload".into()])
                    .await
            {
                return Err(ServiceFailure::Recovery {
                    operation: Box::new(error),
                    recovery: Box::new(recovery),
                });
            }
            return Err(error);
        }
        Ok(())
    }

    fn set_launch_login(&self, login: bool) -> io::Result<()> {
        let path = self.files.login_registration.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "login registration path is required",
            )
        })?;
        if login {
            atomic_write(
                path,
                self.definition
                    .document(ServicePlatform::Launchd, true)?
                    .as_bytes(),
            )
        } else {
            remove_optional(path)
        }
    }
    async fn set_task_login(
        &self,
        original: &Arc<SnapshotContext>,
        login: bool,
    ) -> Result<(), ServiceFailure> {
        let path = self.files.login_registration.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "login registration path is required",
            )
        })?;
        let name = format!("{}-login", self.definition.name);
        if login {
            atomic_write(
                path,
                self.definition
                    .document(ServicePlatform::TaskScheduler, true)?
                    .as_bytes(),
            )?;
            self.commands
                .execute(
                    original,
                    vec![
                        "/create".into(),
                        "/tn".into(),
                        name.into(),
                        "/xml".into(),
                        path.clone().into_os_string(),
                        "/f".into(),
                    ],
                )
                .await
        } else if read_optional(path)?.is_some() {
            self.commands
                .execute(
                    original,
                    vec!["/delete".into(), "/tn".into(), name.into(), "/f".into()],
                )
                .await?;
            remove_optional(path)?;
            Ok(())
        } else {
            Ok(())
        }
    }
    pub async fn set_login(
        &self,
        original: &Arc<SnapshotContext>,
        login: bool,
    ) -> Result<(), ServiceFailure> {
        self.validate()?;
        let _lock = installation_lock(&self.files.control_root)?;
        original.check().map_err(io::Error::other)?;
        match self.commands.platform() {
            ServicePlatform::Systemd => {
                self.commands
                    .execute(
                        original,
                        vec![
                            "--user".into(),
                            if login { "enable" } else { "disable" }.into(),
                            self.unit().into(),
                        ],
                    )
                    .await
            }
            ServicePlatform::Launchd => self.set_launch_login(login).map_err(ServiceFailure::Io),
            ServicePlatform::TaskScheduler => self.set_task_login(original, login).await,
        }
    }
    pub async fn uninstall(&self, original: &Arc<SnapshotContext>) -> Result<(), ServiceFailure> {
        self.validate()?;
        let _lock = installation_lock(&self.files.control_root)?;
        self.stop(original).await?;
        match self.commands.platform() {
            ServicePlatform::Systemd => {
                self.commands
                    .execute(
                        original,
                        vec!["--user".into(), "disable".into(), self.unit().into()],
                    )
                    .await?
            }
            ServicePlatform::Launchd => {
                if let Some(path) = &self.files.login_registration {
                    remove_optional(path)?;
                }
            }
            ServicePlatform::TaskScheduler => {
                self.set_task_login(original, false).await?;
                self.commands
                    .execute(
                        original,
                        vec![
                            "/delete".into(),
                            "/tn".into(),
                            self.definition.name.clone().into(),
                            "/f".into(),
                        ],
                    )
                    .await?;
            }
        }
        remove_optional(&self.files.registration)?;
        if self.commands.platform() == ServicePlatform::Systemd {
            self.commands
                .execute(original, vec!["--user".into(), "daemon-reload".into()])
                .await?;
        }
        Ok(())
    }
}

fn read_optional(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
fn remove_optional(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
fn restore(path: &Path, old: Option<&[u8]>) -> io::Result<()> {
    if let Some(old) = old {
        atomic_write(path, old)
    } else {
        remove_optional(path)
    }
}
fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "service file parent missing")
    })?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(contents)?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
