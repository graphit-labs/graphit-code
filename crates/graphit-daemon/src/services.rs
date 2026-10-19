//! Per-user service definitions and native manager commands.
//! The caller owns the shared ProcessHost and each operation's original context.
use graphit_core::{file_io::SnapshotContext, file_lock::FileLock};
use graphit_process::{
    Cleanup, Failure, IgnoreChunks, Limits, OperationAdmissionFailure, Outcome, ProcessHost,
    Request, Retention, operation, run,
};
use std::{
    ffi::OsString,
    fmt, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServicePlatform {
    Systemd,
    Launchd,
    TaskScheduler,
}

#[derive(Clone, Debug)]
pub struct ServiceDefinition {
    pub name: String,
    pub executable: PathBuf,
    pub global_dir: PathBuf,
    pub user: String,
    pub uid: u32,
}

fn is_xml_10_text(value: &str) -> bool {
    value.chars().all(|character| {
        matches!(character,
            '\t' | '\n' | '\r'
            | '\u{20}'..='\u{d7ff}'
            | '\u{e000}'..='\u{fffd}'
            | '\u{10000}'..='\u{10ffff}'
        )
    })
}

impl ServiceDefinition {
    pub fn validate(&self) -> io::Result<()> {
        if self.name.is_empty()
            || !self
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            || !self.executable.is_absolute()
            || self
                .executable
                .to_str()
                .is_none_or(|value| value.contains('\0'))
            || !self.global_dir.is_absolute()
            || self
                .global_dir
                .to_str()
                .is_none_or(|value| value.contains('\0'))
            || self.user.contains(['\0', '\n', '\r'])
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid per-user service definition",
            ));
        }
        Ok(())
    }
    pub fn document(&self, platform: ServicePlatform, login: bool) -> io::Result<String> {
        self.validate()?;
        let executable = self.executable.to_str().expect("validated Unicode path");
        let global_dir = self
            .global_dir
            .to_str()
            .expect("validated Unicode global directory");
        if matches!(
            platform,
            ServicePlatform::Launchd | ServicePlatform::TaskScheduler
        ) && (![executable, global_dir].into_iter().all(is_xml_10_text)
            || (matches!(platform, ServicePlatform::TaskScheduler) && !is_xml_10_text(&self.user)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "per-user service values must be representable in XML 1.0",
            ));
        }
        let xml_executable = quick_xml::escape::escape(executable);
        let xml_global_dir = quick_xml::escape::escape(global_dir);
        let name = quick_xml::escape::escape(&self.name);
        let user = quick_xml::escape::escape(&self.user);
        Ok(match platform {
            ServicePlatform::Systemd => {
                // WorkingDirectory is a literal directive, not an argv word.
                // Reject values the unit-file reader would trim or continue.
                if global_dir.contains(['\n', '\r'])
                    || global_dir.trim_end_matches([' ', '\t', '\x0b', '\x0c']) != global_dir
                    || !global_dir
                        .bytes()
                        .rev()
                        .take_while(|byte| *byte == b'\\')
                        .count()
                        .is_multiple_of(2)
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "global directory cannot be preserved by systemd WorkingDirectory",
                    ));
                }
                let working_directory = global_dir.replace('%', "%%");
                let launcher = which::which("sh").map_err(io::Error::other)?;
                let launcher = launcher.to_str().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "systemd shell path must be Unicode",
                    )
                })?;
                let launcher = serde_json::to_string(&format!(":{}", launcher.replace('%', "%%")))
                    .map_err(io::Error::other)?;
                let executable = serde_json::to_string(&executable.replace('%', "%%"))
                    .map_err(io::Error::other)?;
                let global_dir = serde_json::to_string(&global_dir.replace('%', "%%"))
                    .map_err(io::Error::other)?;
                let command = serde_json::to_string("exec \"$@\"").map_err(io::Error::other)?;
                format!(
                    "[Unit]\nDescription=Graphit daemon\nStartLimitIntervalSec=0\n\n[Service]\nType=simple\nWorkingDirectory={working_directory}\nExecStart={launcher} -c {command} graphit-service {executable} --global-dir {global_dir} daemon --managed\nKillMode=mixed\nRestart=on-failure\nRestartSec=2\n\n[Install]\nWantedBy=default.target\n"
                )
            }
            ServicePlatform::Launchd => format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{name}</string><key>WorkingDirectory</key><string>{xml_global_dir}</string><key>ProgramArguments</key><array><string>{xml_executable}</string><string>--global-dir</string><string>{xml_global_dir}</string><string>daemon</string><string>--managed</string></array><key>RunAtLoad</key><true/><key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict></dict></plist>\n"
            ),
            ServicePlatform::TaskScheduler => {
                let trigger = if login {
                    format!(
                        "<LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger>"
                    )
                } else {
                    "<RegistrationTrigger><Enabled>false</Enabled></RegistrationTrigger>".to_owned()
                };
                // Task Scheduler supplies these arguments directly to the executable.
                let global_argument = shell_escape::windows::escape(global_dir.into());
                let arguments = format!(
                    "--global-dir {global_argument} daemon {}",
                    if login { "start" } else { "--managed" }
                );
                let arguments = quick_xml::escape::escape(&arguments);
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Task version=\"1.4\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\"><Triggers>{trigger}</Triggers><Principals><Principal id=\"CurrentUser\"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals><Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><AllowStartOnDemand>true</AllowStartOnDemand><RestartOnFailure><Interval>PT1M</Interval><Count>255</Count></RestartOnFailure></Settings><Actions Context=\"CurrentUser\"><Exec><Command>{xml_executable}</Command><Arguments>{arguments}</Arguments><WorkingDirectory>{xml_global_dir}</WorkingDirectory></Exec></Actions></Task>\n"
                )
            }
        })
    }
}

#[derive(Debug)]
pub enum ServiceFailure {
    Io(io::Error),
    Admission(OperationAdmissionFailure),
    Process(Box<Failure>),
    Manager(Box<Outcome>),
    Recovery {
        operation: Box<ServiceFailure>,
        recovery: Box<ServiceFailure>,
    },
}
impl fmt::Display for ServiceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "service IO: {error}"),
            Self::Admission(error) => write!(formatter, "service process admission: {error:?}"),
            Self::Process(error) => write!(formatter, "service process: {error}"),
            Self::Recovery {
                operation,
                recovery,
            } => write!(formatter, "{operation}; service rollback: {recovery}"),
            Self::Manager(outcome) => write!(
                formatter,
                "service manager exited with {}; cleanup {:?}",
                outcome.status, outcome.cleanup
            ),
        }
    }
}
impl std::error::Error for ServiceFailure {}
impl From<io::Error> for ServiceFailure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Native utility resolved once with explicit environment; original request context is borrowed per call.
pub struct ServiceCommands<'host> {
    host: &'host ProcessHost,
    program: PathBuf,
    cwd: PathBuf,
    environment: Vec<(OsString, OsString)>,
    platform: ServicePlatform,
}
impl<'host> ServiceCommands<'host> {
    pub fn resolve(
        host: &'host ProcessHost,
        platform: ServicePlatform,
        uid: u32,
        cwd: PathBuf,
        mut environment: Vec<(OsString, OsString)>,
    ) -> io::Result<Self> {
        if !cwd.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "service working directory must be absolute",
            ));
        }
        if platform == ServicePlatform::Systemd {
            let runtime = environment
                .iter()
                .find(|(key, value)| key == "XDG_RUNTIME_DIR" && !value.is_empty())
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| OsString::from(format!("/run/user/{uid}")));
            if !environment
                .iter()
                .any(|(key, value)| key == "XDG_RUNTIME_DIR" && !value.is_empty())
            {
                environment.retain(|(key, _)| key != "XDG_RUNTIME_DIR");
                environment.push(("XDG_RUNTIME_DIR".into(), runtime.clone()));
            }
            if !environment
                .iter()
                .any(|(key, value)| key == "DBUS_SESSION_BUS_ADDRESS" && !value.is_empty())
            {
                let mut address = OsString::from("unix:path=");
                address.push(runtime);
                address.push("/bus");
                environment.retain(|(key, _)| key != "DBUS_SESSION_BUS_ADDRESS");
                environment.push(("DBUS_SESSION_BUS_ADDRESS".into(), address));
            }
        }
        let name = match platform {
            ServicePlatform::Systemd => "systemctl",
            ServicePlatform::Launchd => "launchctl",
            ServicePlatform::TaskScheduler => "schtasks.exe",
        };
        let paths = environment
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("PATH"))
            .map(|(_, value)| value);
        let program = which::which_in(name, paths, &cwd).map_err(|error| match error {
            which::Error::CannotFindBinaryPath => io::Error::new(io::ErrorKind::NotFound, error),
            error => io::Error::other(error),
        })?;
        Ok(Self {
            host,
            program,
            cwd,
            environment,
            platform,
        })
    }
    pub fn platform(&self) -> ServicePlatform {
        self.platform
    }
    pub async fn capture(
        &self,
        original: &Arc<SnapshotContext>,
        args: Vec<OsString>,
    ) -> Result<Outcome, ServiceFailure> {
        let (_stop, receiver) =
            operation(original, self.host).map_err(ServiceFailure::Admission)?;
        let request = Request {
            program: self.program.clone(),
            args,
            cwd: self.cwd.clone(),
            environment: self.environment.clone(),
            stdin: None,
            limits: Limits {
                stdin_bytes: 0,
                stdout_bytes: 64 * 1024,
                stderr_bytes: 64 * 1024,
                termination_grace: Duration::from_secs(1),
                cleanup: Duration::from_secs(3),
            },
        };
        let outcome = run(
            request,
            original,
            Retention::Full,
            &mut IgnoreChunks,
            receiver,
        )
        .await
        .map_err(|error| ServiceFailure::Process(Box::new(error)))?;
        if outcome.cleanup != Cleanup::Complete || outcome.remaining.is_some() {
            return Err(ServiceFailure::Manager(Box::new(outcome)));
        }
        Ok(outcome)
    }
    pub async fn execute(
        &self,
        original: &Arc<SnapshotContext>,
        args: Vec<OsString>,
    ) -> Result<(), ServiceFailure> {
        let outcome = self.capture(original, args).await?;
        if outcome.status.success() {
            Ok(())
        } else {
            Err(ServiceFailure::Manager(Box::new(outcome)))
        }
    }
}

/// The stable inode is retained; dropping this guard unlocks without deleting it.
pub fn installation_lock(control_root: &Path) -> io::Result<FileLock> {
    FileLock::try_acquire(control_root.join("service-install.lock"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use graphit_process::{HostLimits, HostReadiness, HostShutdown};
    use std::time::Instant;

    fn definition(executable: PathBuf) -> ServiceDefinition {
        ServiceDefinition {
            name: "graphit-fixture".into(),
            global_dir: executable.parent().unwrap().join("installation"),
            executable,
            user: "example&user".into(),
            uid: 1000,
        }
    }

    #[test]
    fn task_scheduler_definition_is_per_user_and_login_is_explicit() {
        let definition = definition(
            std::env::current_dir()
                .unwrap()
                .join("example space/\"quote\"&<unicode-ê>/graphit"),
        );
        let normal = definition
            .document(ServicePlatform::TaskScheduler, false)
            .unwrap();
        let login = definition
            .document(ServicePlatform::TaskScheduler, true)
            .unwrap();
        assert!(normal.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(normal.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(normal.contains("<RegistrationTrigger><Enabled>false</Enabled>"));
        assert!(!normal.contains("<LogonTrigger>"));
        assert!(login.contains("<LogonTrigger>"));
        assert!(login.contains("daemon start"));
        let mut reader = quick_xml::Reader::from_str(&normal);
        while !matches!(reader.read_event().unwrap(), quick_xml::events::Event::Eof) {}
        assert!(normal.contains("&quot;quote&quot;&amp;&lt;unicode-ê&gt;"));
    }

    #[test]
    fn launch_agent_uses_argument_array_and_gui_identity() {
        let executable = std::env::current_dir()
            .unwrap()
            .join("example \rcarriage\tand\nlinefeed-😀/graphit");
        let definition = definition(executable.clone());
        let plist = definition
            .document(ServicePlatform::Launchd, false)
            .unwrap();
        assert!(plist.contains(&format!(
            "<array><string>{}</string><string>--global-dir</string><string>{}</string><string>daemon</string><string>--managed</string></array>",
            quick_xml::escape::escape(executable.to_str().unwrap()),
            quick_xml::escape::escape(definition.global_dir.to_str().unwrap()),
        )));
        assert!(plist.contains("<key>SuccessfulExit</key><false/>"));
        assert!(!plist.contains("<key>UserName</key>"));
        assert!(
            !plist.contains('\r'),
            "CR must be an XML character reference"
        );
        let encoded = plist
            .split_once("<array>")
            .unwrap()
            .1
            .split_once("</array>")
            .unwrap()
            .0;
        let arguments = encoded
            .split("<string>")
            .skip(1)
            .map(|value| {
                quick_xml::escape::unescape(value.split_once("</string>").unwrap().0)
                    .unwrap()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            arguments,
            vec![
                executable.to_str().unwrap(),
                "--global-dir",
                definition.global_dir.to_str().unwrap(),
                "daemon",
                "--managed"
            ]
        );

        assert!(plist.contains(&format!(
            "<key>WorkingDirectory</key><string>{}</string>",
            quick_xml::escape::escape(definition.global_dir.to_str().unwrap()),
        )));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn systemd_working_directory_preserves_literal_directive_or_rejects_loss() {
        let mut definition = definition(std::env::current_exe().unwrap());
        definition.global_dir = std::env::current_dir()
            .unwrap()
            .join("space \"quote\" $USER %x \\ unicode-ê");
        let literal = definition.global_dir.to_str().unwrap().replace('%', "%%");
        let document = definition
            .document(ServicePlatform::Systemd, false)
            .unwrap();
        assert!(
            document
                .lines()
                .any(|line| line == format!("WorkingDirectory={literal}"))
        );
        assert!(
            document.lines().any(|line| line == "KillMode=mixed"),
            "TERM must reach the parent before it drains its owned model child",
        );
        for component in [
            "line\nbreak",
            "line\rbreak",
            "trailing ",
            "trailing\t",
            "trailing\\",
        ] {
            definition.global_dir = std::env::current_dir().unwrap().join(component);
            assert_eq!(
                definition
                    .document(ServicePlatform::Systemd, false)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn windows_global_directory_roundtrips_through_xml_and_native_argument_grammar() {
        let root = std::env::current_dir().unwrap();
        for component in [
            "space path",
            "quote\"and\\slash",
            "literal%USERPROFILE% & <unicode-ê>",
            "space with trailing\\",
        ] {
            let mut definition = definition(std::env::current_exe().unwrap());
            definition.global_dir = root.join(component);
            for (login, mode) in [(false, "--managed"), (true, "start")] {
                let document = definition
                    .document(ServicePlatform::TaskScheduler, login)
                    .unwrap();
                assert!(
                    !document.contains('\r'),
                    "CR must be an XML character reference"
                );
                let mut reader = quick_xml::Reader::from_str(&document);
                while !matches!(reader.read_event().unwrap(), quick_xml::events::Event::Eof) {}
                let encoded = document
                    .split_once("<Arguments>")
                    .unwrap()
                    .1
                    .split_once("</Arguments>")
                    .unwrap()
                    .0;
                let arguments = quick_xml::escape::unescape(encoded).unwrap();
                assert_eq!(
                    winsplit::split(&format!("graphit.exe {arguments}")),
                    vec![
                        "graphit.exe",
                        "--global-dir",
                        definition.global_dir.to_str().unwrap(),
                        "daemon",
                        mode,
                    ]
                );
                assert!(document.contains(&format!(
                    "<WorkingDirectory>{}</WorkingDirectory>",
                    quick_xml::escape::escape(definition.global_dir.to_str().unwrap())
                )));
            }
        }
    }

    #[test]
    fn service_paths_are_absolute_nul_free_before_registration() {
        for path in [
            PathBuf::from("relative"),
            std::env::current_dir().unwrap().join("nul\0path"),
        ] {
            for executable in [false, true] {
                let mut definition = definition(std::env::current_exe().unwrap());
                if executable {
                    definition.executable = path.clone();
                } else {
                    definition.global_dir = path.clone();
                }
                for platform in [
                    ServicePlatform::Systemd,
                    ServicePlatform::Launchd,
                    ServicePlatform::TaskScheduler,
                ] {
                    assert_eq!(
                        definition.document(platform, false).unwrap_err().kind(),
                        io::ErrorKind::InvalidInput
                    );
                }
            }
        }
    }

    #[test]
    fn xml_documents_reject_unrepresentable_values_before_registration() {
        let root = std::env::current_dir().unwrap();
        for value in [
            "\0", "\u{1}", "\u{8}", "\u{b}", "\u{c}", "\u{e}", "\u{1f}", "\u{fffe}", "\u{ffff}",
        ] {
            for field in 0..3 {
                let mut definition = definition(std::env::current_exe().unwrap());
                match field {
                    0 => definition.executable = root.join(format!("graphit{value}")),
                    1 => definition.global_dir = root.join(format!("installation{value}")),
                    _ => definition.user = format!("user{value}"),
                }
                for platform in [ServicePlatform::Launchd, ServicePlatform::TaskScheduler] {
                    for login in [false, true] {
                        let document = definition.document(platform, login);
                        if field == 2 && platform == ServicePlatform::Launchd && value != "\0" {
                            let document = document.unwrap();
                            assert!(!document.contains(value));
                            assert!(!document.contains("<key>UserName</key>"));
                        } else {
                            assert_eq!(document.unwrap_err().kind(), io::ErrorKind::InvalidInput);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_definition_and_installation_contention_are_errors() {
        let mut invalid = definition(PathBuf::from("relative"));
        assert_eq!(
            invalid.validate().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        invalid.executable = std::env::current_dir().unwrap().join("graphit");
        invalid.name = "unsafe/name".into();
        assert_eq!(
            invalid.validate().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        let root = tempfile::tempdir().unwrap();
        let first = installation_lock(root.path()).unwrap();
        assert_eq!(
            installation_lock(root.path()).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(first);
        drop(installation_lock(root.path()).unwrap());
        assert!(root.path().join("service-install.lock").exists());
    }

    async fn host() -> ProcessHost {
        let service = Cancellation::default();
        let host = ProcessHost::install(&service, HostLimits::new(1, 1).unwrap()).unwrap();
        assert!(matches!(
            host.ready(Instant::now() + Duration::from_secs(5)).await,
            HostReadiness::Ready
        ));
        host
    }
    fn context() -> Arc<SnapshotContext> {
        Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(60),
            Cancellation::default(),
        ))
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires a running systemd user manager; run explicitly in the native service suite"]
    async fn native_systemd_execstart_preserves_literal_path_and_arguments() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("space \"quote\" $USER %x \\ = unicode-ê");
        std::fs::create_dir(&directory).unwrap();
        let executable = directory.join("graphit");
        let record = root.path().join("arguments.bin");
        let cwd_record = root.path().join("working-directory.txt");
        std::fs::write(
            &executable,
            format!(
                "#!/bin/sh\nprintf '%s\\0' \"$0\" \"$@\" > \"{}\"\npwd -P > \"{}\"\nexec /usr/bin/sleep 30\n",
                record.display(), cwd_record.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut definition = definition(executable.clone());
        std::fs::create_dir(&definition.global_dir).unwrap();
        definition.name = format!(
            "graphit-services-{}",
            root.path().file_name().unwrap().to_str().unwrap()
        );
        let uid = std::fs::metadata(root.path()).unwrap().uid();
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(format!("/run/user/{uid}")));
        let unit = runtime
            .join("systemd/user")
            .join(format!("{}.service", definition.name));
        let host = host().await;
        let original = context();
        let commands = ServiceCommands::resolve(
            &host,
            ServicePlatform::Systemd,
            uid,
            root.path().to_path_buf(),
            std::env::vars_os().collect(),
        )
        .unwrap();
        struct CleanupUnit {
            program: PathBuf,
            name: String,
            registration: PathBuf,
            root: Option<tempfile::TempDir>,
            completed: bool,
        }
        impl Drop for CleanupUnit {
            fn drop(&mut self) {
                use std::process::Command;
                if self.completed {
                    return;
                }
                let stop = Command::new(&self.program)
                    .args(["--user", "stop", &self.name])
                    .output();
                let observation = Command::new(&self.program)
                    .args([
                        "--user",
                        "show",
                        "--property=ActiveState",
                        "--property=MainPID",
                        "--property=LoadState",
                        &self.name,
                    ])
                    .output();
                let inactive = matches!(&observation, Ok(output) if output.status.success()
                    && String::from_utf8_lossy(&output.stdout).lines().any(|line| line == "ActiveState=inactive")
                    && String::from_utf8_lossy(&output.stdout).lines().any(|line| line == "MainPID=0")
                    && (matches!(&stop, Ok(stopped) if stopped.status.success())
                        || String::from_utf8_lossy(&output.stdout).lines().any(|line| line == "LoadState=not-found")));
                let disabled = inactive
                    && [
                        vec!["--user", "disable", &self.name],
                        vec!["--user", "--runtime", "disable", &self.name],
                    ]
                    .into_iter()
                    .all(|arguments| {
                        let outcome = Command::new(&self.program).args(arguments).output();
                        let success = matches!(&outcome, Ok(output) if output.status.success());
                        if !success {
                            eprintln!("private unit disable failed: {outcome:?}");
                        }
                        success
                    });
                if !disabled {
                    let root = self.root.take().unwrap().keep();
                    eprintln!(
                        "private unit {} retained with installation {}: stop={stop:?} observed={observation:?}",
                        self.name,
                        root.display()
                    );
                    return;
                }
                let removed = match std::fs::remove_file(&self.registration) {
                    Ok(()) => true,
                    Err(error) => error.kind() == io::ErrorKind::NotFound,
                };
                let reload = Command::new(&self.program)
                    .args(["--user", "daemon-reload"])
                    .output();
                if !removed || !matches!(&reload, Ok(output) if output.status.success()) {
                    let root = self.root.take().unwrap().keep();
                    eprintln!(
                        "private unit {} cleanup incomplete; installation {} retained: reload={reload:?}",
                        self.name,
                        root.display()
                    );
                }
            }
        }
        let control_root = root.path().to_path_buf();
        let mut cleanup = CleanupUnit {
            program: commands.program.clone(),
            name: format!("{}.service", definition.name),
            registration: unit.clone(),
            root: Some(root),
            completed: false,
        };
        let service = crate::control::UserService {
            definition,
            files: crate::control::ServiceFiles {
                control_root,
                registration: unit.clone(),
                login_registration: None,
            },
            commands,
        };
        service.install(&original, false).await.unwrap();
        assert!(unit.is_file());
        if let Err(error) = service.start(&original).await {
            if let ServiceFailure::Manager(outcome) = &error {
                let transcript = outcome.transcript.as_ref().unwrap();
                panic!(
                    "{error}: stdout={} stderr={}",
                    String::from_utf8_lossy(transcript.stdout()),
                    String::from_utf8_lossy(transcript.stderr())
                );
            }
            panic!("{error}");
        }
        let status = service.status(&original).await.unwrap();
        assert!(status.status.success());
        let status_text = String::from_utf8_lossy(status.transcript.as_ref().unwrap().stdout());
        assert!(status_text.contains("ActiveState=active"));
        let before_pid = status_text
            .lines()
            .find_map(|line| line.strip_prefix("MainPID="))
            .unwrap()
            .to_owned();
        assert_ne!(before_pid, "0");
        let expected_cwd = format!("{}\n", service.definition.global_dir.display());
        tokio::time::timeout(Duration::from_secs(8), async {
            while std::fs::read_to_string(&cwd_record).ok().as_deref() != Some(&expected_cwd) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        service.restart(&original).await.unwrap();
        let after_status = service.status(&original).await.unwrap();
        let after_text =
            String::from_utf8_lossy(after_status.transcript.as_ref().unwrap().stdout());
        let after_pid = after_text
            .lines()
            .find_map(|line| line.strip_prefix("MainPID="))
            .unwrap();
        assert_ne!(after_pid, before_pid);
        tokio::time::timeout(Duration::from_secs(8), async {
            while !record.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let bytes = std::fs::read(&record).unwrap();
        let arguments: Vec<_> = bytes
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .collect();
        assert_eq!(
            arguments,
            vec![
                executable.as_os_str().as_encoded_bytes(),
                b"--global-dir",
                service.definition.global_dir.as_os_str().as_encoded_bytes(),
                b"daemon",
                b"--managed"
            ]
        );
        service.set_login(&original, true).await.unwrap();
        let login_status = service.status(&original).await.unwrap();
        assert!(
            String::from_utf8_lossy(login_status.transcript.as_ref().unwrap().stdout())
                .contains("UnitFileState=enabled")
        );
        service.set_login(&original, false).await.unwrap();
        let login_status = service.status(&original).await.unwrap();
        assert!(
            String::from_utf8_lossy(login_status.transcript.as_ref().unwrap().stdout())
                .contains("UnitFileState=disabled")
        );
        service.uninstall(&original).await.unwrap();
        assert!(!unit.exists());
        let uninstalled = service.status(&original).await.unwrap();
        assert!(
            String::from_utf8_lossy(uninstalled.transcript.as_ref().unwrap().stdout())
                .contains("LoadState=not-found")
        );
        cleanup.completed = true;
        drop(service);
        drop(cleanup);
        assert!(matches!(
            host.shutdown(Instant::now() + Duration::from_secs(5)).await,
            HostShutdown::Joined(_)
        ));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn unavailable_manager_restores_registration_and_releases_installation_lock() {
        let root = tempfile::tempdir().unwrap();
        let definition = definition(std::env::current_exe().unwrap());
        let registration = root.path().join(format!("{}.service", definition.name));
        std::fs::write(&registration, b"previous registration").unwrap();
        let host = host().await;
        let original = context();
        let mut environment: Vec<_> = std::env::vars_os().collect();
        environment
            .retain(|(key, _)| key != "DBUS_SESSION_BUS_ADDRESS" && key != "XDG_RUNTIME_DIR");
        environment.push((
            "XDG_RUNTIME_DIR".into(),
            root.path().join("unavailable-runtime").into_os_string(),
        ));
        environment.push((
            "DBUS_SESSION_BUS_ADDRESS".into(),
            format!("unix:path={}/absent-bus", root.path().display()).into(),
        ));
        let commands = ServiceCommands::resolve(
            &host,
            ServicePlatform::Systemd,
            1000,
            root.path().to_path_buf(),
            environment,
        )
        .unwrap();
        let service = crate::control::UserService {
            definition,
            files: crate::control::ServiceFiles {
                control_root: root.path().to_path_buf(),
                registration: registration.clone(),
                login_registration: None,
            },
            commands,
        };
        assert!(matches!(
            service.install(&original, true).await,
            Err(ServiceFailure::Recovery { .. })
        ));
        assert_eq!(
            std::fs::read(&registration).unwrap(),
            b"previous registration"
        );
        drop(installation_lock(root.path()).unwrap());
        drop(service);
        assert!(matches!(
            host.shutdown(Instant::now() + Duration::from_secs(5)).await,
            HostShutdown::Joined(_)
        ));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn cancelled_install_does_not_publish_registration() {
        let root = tempfile::tempdir().unwrap();
        let definition = definition(std::env::current_exe().unwrap());
        let registration = root.path().join(format!("{}.service", definition.name));
        let host = host().await;
        let original = context();
        original.cancellation().cancel();
        let commands = ServiceCommands::resolve(
            &host,
            ServicePlatform::Systemd,
            1000,
            root.path().to_path_buf(),
            std::env::vars_os().collect(),
        )
        .unwrap();
        let service = crate::control::UserService {
            definition,
            files: crate::control::ServiceFiles {
                control_root: root.path().to_path_buf(),
                registration: registration.clone(),
                login_registration: None,
            },
            commands,
        };
        assert!(service.install(&original, false).await.is_err());
        assert!(!registration.exists());
        drop(installation_lock(root.path()).unwrap());
        drop(service);
        assert!(matches!(
            host.shutdown(Instant::now() + Duration::from_secs(5)).await,
            HostShutdown::Joined(_)
        ));
    }
}
