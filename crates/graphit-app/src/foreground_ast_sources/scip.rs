//! Selected SCIP Docker images, with the Process SDK retaining cleanup owners.
use super::*;
use graphit_ast::{
    scip::AstScipError,
    scip_stream::{IndexRecord, visit_index},
    source_index_locator::LocalSourceIndex,
};
use graphit_core::native_file_identity::{PhysicalId, handle_id};
use graphit_fs::{NodeKind, read_directory};
use graphit_hub::{native::ContentError, policy::RequestError};
use graphit_process::{
    Cleanup, HostLimits, HostReadiness, HostReport, HostShutdown, HostShutdownObservation,
    IgnoreChunks, PendingHost, ProcessHost, RemainingProcess, RepairObservation, Request,
    Retention,
};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::Read,
    num::NonZeroU32,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum ScipFamily {
    Go,
    TypeScript,
    Python,
    Java,
    Clang,
    Dotnet,
    Rust,
    Ruby,
    Dart,
    Php,
}
pub(crate) mod config;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum ScipCompiler {
    Maintained(ScipFamily),
    CustomImage(String),
}
impl From<ScipFamily> for ScipCompiler {
    fn from(family: ScipFamily) -> Self {
        Self::Maintained(family)
    }
}
impl ScipCompiler {
    fn family(&self) -> Option<ScipFamily> {
        match self {
            Self::Maintained(family) => Some(*family),
            Self::CustomImage(_) => None,
        }
    }
    pub fn as_str(&self) -> &str {
        match self {
            Self::Maintained(family) => family.as_str(),
            Self::CustomImage(image) => image,
        }
    }
    fn label(&self) -> &str {
        self.family().map_or("custom", ScipFamily::as_str)
    }
    pub fn image(&self, config: &ConfigSnapshot) -> io::Result<String> {
        if let Self::CustomImage(image) = self {
            return Ok(image.clone());
        }
        let key = ConfigKey::new("ast.scip.version")?;
        let version = config.get::<String>(&key)?.unwrap_or_else(|| "v1".into());
        if !valid_tag(&version) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid ast.scip.version image tag",
            ));
        }
        Ok(format!(
            "ghcr.io/graphit-labs/graphit-scip-{}:{version}",
            self.label()
        ))
    }
    fn cache_directory(
        &self,
        installation: &InstallationPaths,
        image: &str,
    ) -> io::Result<PathBuf> {
        match self {
            Self::Maintained(family) => {
                installation.global_path(&["cache", "scip", family.as_str()])
            }
            Self::CustomImage(_) => {
                let hash: [u8; 32] = Sha256::digest(image.as_bytes()).into();
                let key = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();
                installation.global_path(&["cache", "scip", "images", &key])
            }
        }
    }
}
const NATIVE_EXTENSIONS: &[(ScipFamily, &[&str])] = &[
    (ScipFamily::Go, &[".go"]),
    (
        ScipFamily::TypeScript,
        &[".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx", ".mts", ".cts"],
    ),
    (ScipFamily::Python, &[".py", ".pyi"]),
    (ScipFamily::Java, &[".java", ".kt", ".kts"]),
    (
        ScipFamily::Clang,
        &[
            ".c", ".h", ".cc", ".cpp", ".cxx", ".hpp", ".hh", ".hxx", ".m", ".mm",
        ],
    ),
    (ScipFamily::Dotnet, &[".cs", ".vb"]),
    (ScipFamily::Rust, &[".rs"]),
    (ScipFamily::Ruby, &[".rb", ".rake", ".gemspec"]),
    (ScipFamily::Dart, &[".dart"]),
    (ScipFamily::Php, &[".php"]),
];
impl ScipFamily {
    pub fn native_extensions() -> impl Iterator<Item = &'static str> {
        NATIVE_EXTENSIONS
            .iter()
            .flat_map(|(_, extensions)| extensions.iter().copied())
    }
    /// An explicit profile remains authoritative, including custom extensions.
    pub fn for_path_profile(path: &str, profile_language: Option<&str>) -> Option<Self> {
        if let Some(language) = profile_language {
            return match language {
                "go" => Some(Self::Go),
                "javascript" | "jsx" | "typescript" | "tsx" => Some(Self::TypeScript),
                "python" => Some(Self::Python),
                "java" | "kotlin" => Some(Self::Java),
                "c" | "cpp" | "c++" | "objective-c" | "objective-cpp" => Some(Self::Clang),
                "csharp" | "c_sharp" | "c#" | "vbnet" => Some(Self::Dotnet),
                "rust" => Some(Self::Rust),
                "ruby" => Some(Self::Ruby),
                "dart" => Some(Self::Dart),
                "php" => Some(Self::Php),
                _ => None,
            };
        }
        let extension = Path::new(path).extension()?.to_str()?;
        NATIVE_EXTENSIONS.iter().find_map(|(family, extensions)| {
            extensions
                .iter()
                .any(|route| route[1..].eq_ignore_ascii_case(extension))
                .then_some(*family)
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Go => "go",
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Java => "java",
            Self::Clang => "clang",
            Self::Dotnet => "dotnet",
            Self::Rust => "rust",
            Self::Ruby => "ruby",
            Self::Dart => "dart",
            Self::Php => "php",
        }
    }
    fn manifests(self) -> &'static [&'static str] {
        match self {
            Self::Go => &["go.mod", "go.sum", "go.work", "go.work.sum"],
            Self::TypeScript => &[
                "package.json",
                "package-lock.json",
                "npm-shrinkwrap.json",
                "yarn.lock",
                "pnpm-lock.yaml",
                "pnpm-workspace.yaml",
                "tsconfig.json",
                "jsconfig.json",
                ".npmrc",
                ".yarnrc",
                ".yarnrc.yml",
            ],
            Self::Python => &[
                "pyproject.toml",
                "requirements.txt",
                "setup.py",
                "setup.cfg",
                "Pipfile",
                "Pipfile.lock",
                "poetry.lock",
                "uv.lock",
                ".python-version",
            ],
            Self::Java => &[
                "pom.xml",
                "build.gradle",
                "build.gradle.kts",
                "settings.gradle",
                "settings.gradle.kts",
                "gradle.properties",
                "gradle/libs.versions.toml",
                ".mvn/maven.config",
                ".mvn/jvm.config",
            ],
            Self::Clang => &[
                "compile_commands.json",
                "compile_flags.txt",
                "CMakeLists.txt",
                "Makefile",
                "CMakePresets.json",
                ".clangd",
                ".ccls",
            ],
            Self::Dotnet => &[
                "global.json",
                "NuGet.Config",
                "nuget.config",
                "Directory.Build.props",
                "Directory.Build.targets",
                "Directory.Packages.props",
                "packages.lock.json",
                "project.json",
            ],
            Self::Rust => &[
                "Cargo.toml",
                "Cargo.lock",
                ".cargo/config",
                ".cargo/config.toml",
                "rust-toolchain",
                "rust-toolchain.toml",
            ],
            Self::Ruby => &["Gemfile", "Gemfile.lock", ".ruby-version", ".bundle/config"],
            Self::Dart => &[
                "pubspec.yaml",
                "pubspec.lock",
                "pubspec_overrides.yaml",
                "analysis_options.yaml",
            ],
            Self::Php => &["composer.json", "composer.lock", "scip-php.json"],
        }
    }
    fn is_named_manifest(self, name: &str) -> bool {
        match self {
            Self::TypeScript => name.starts_with("tsconfig.") && name.ends_with(".json"),
            Self::Python => name.starts_with("requirements") && name.ends_with(".txt"),
            Self::Dotnet => matches!(
                Path::new(name).extension().and_then(OsStr::to_str),
                Some("csproj" | "fsproj" | "vbproj" | "sln" | "slnx" | "props" | "targets")
            ),
            Self::Ruby => name.ends_with(".gemspec"),
            _ => false,
        }
    }
}

const CLEANUP: Duration = Duration::from_secs(10);

fn completed_compiler_deadline<'a>(
    error: &'a io::Error,
    original: &SnapshotContext,
    process_pending: bool,
) -> Option<&'a graphit_process::Failure> {
    if process_pending || original.check().err()?.kind() != io::ErrorKind::TimedOut {
        return None;
    }
    let failure = error
        .get_ref()?
        .downcast_ref::<graphit_process::Failure>()?;
    (failure.reason == graphit_process::Reason::TimedOut
        && failure.cleanup == Cleanup::Complete
        && failure.remaining.is_none())
    .then_some(failure)
}

#[derive(Debug)]
struct CleanupFailure {
    compiler: Option<io::Error>,
    cleanup: io::Error,
}
impl fmt::Display for CleanupFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(cause) = &self.compiler {
            write!(f, "AST SCIP compiler: {cause}; ")?;
        }
        write!(f, "container cleanup: {}", self.cleanup)
    }
}
impl Error for CleanupFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.compiler.as_ref().unwrap_or(&self.cleanup))
    }
}

enum HostOwner {
    Live(ProcessHost),
    Joining(tokio::task::JoinHandle<HostShutdown>),
    Pending(PendingHost),
    Joined(HostReport),
    Failed(tokio::task::JoinError),
}
struct Container {
    name: String,
    docker: PathBuf,
    cwd: PathBuf,
    // Keep the output mount alive until its container has been removed.
    output: tempfile::TempDir,
}
#[derive(Default)]
struct State {
    host: Option<HostOwner>,
    remaining: Option<RemainingProcess>,
    container: Option<Container>,
}
/// No host is installed until an eligible compiler family is selected.
/// The mutex serializes compiler execution, not corpus admission or resource accounting.
pub(super) struct ScipService {
    environment: Vec<(OsString, OsString)>,
    pull: &'static str,
    stop: Cancellation,
    state: Mutex<State>,
}
impl ScipService {
    pub fn docker_command(&self, root: &Path) -> Result<PathBuf, which::Error> {
        let path = self
            .environment
            .iter()
            .find(|(key, _)| key == OsStr::new("PATH"))
            .map(|(_, value)| value);
        which::which_in("docker", path, root)
    }
    pub fn new(
        environment: Vec<(OsString, OsString)>,
        brand: &graphit_core::brand::Brand,
    ) -> io::Result<Self> {
        let pull_key = OsString::from(
            brand
                .environment_variable("AST_SCIP_PULL_POLICY")
                .map_err(err)?,
        );
        let pull = match environment
            .iter()
            .find(|(key, _)| key == &pull_key)
            .map(|(_, value)| value.to_str())
        {
            None | Some(Some("always")) => "always",
            Some(Some("missing")) => "missing",
            Some(Some("never")) => "never",
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "AST_SCIP_PULL_POLICY must be always, missing or never",
                ));
            }
        };
        Ok(Self {
            environment,
            pull,
            stop: Cancellation::default(),
            state: Mutex::new(State::default()),
        })
    }
    // Family, project/installation configuration, policy, and request authority stay distinct.
    #[allow(clippy::too_many_arguments)]
    pub async fn index(
        &self,
        compiler: impl Into<ScipCompiler>,
        project: &ProjectContext,
        installation: &InstallationPaths,
        config: &ConfigSnapshot,
        policy: &HubResourcePolicy,
        cancellation: &Cancellation,
        guard: &impl Fn() -> io::Result<()>,
    ) -> io::Result<Option<ScipOutput>> {
        let compiler = compiler.into();
        let image = compiler.image(config)?;
        guard()?;
        let operation = policy
            .request_context(Instant::now(), cancellation)
            .map_err(|cause| {
                let kind = match &cause {
                    RequestError::Content(ContentError::Cancelled) => io::ErrorKind::Interrupted,
                    RequestError::Content(ContentError::TimedOut) => io::ErrorKind::TimedOut,
                    _ => io::ErrorKind::Other,
                };
                io::Error::new(kind, cause)
            })?;
        let original = operation.snapshot();
        let mut state = tokio::select! {
            biased;
            () = cancellation.cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(original.required_deadline()?.into()) => return Err(io::ErrorKind::TimedOut.into()),
            state = self.state.lock() => state,
        };
        operation.check()?;
        guard()?;
        if self.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if state.remaining.is_some() || state.container.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "AST SCIP cleanup remains pending",
            ));
        }
        let docker = match self.docker_command(project.root()) {
            Ok(path) => path,
            Err(error) => {
                operation.check()?;
                guard()?;
                eprintln!("AST SCIP unavailable: {error}");
                return Ok(None);
            }
        };
        if state.host.is_none() {
            state.host = Some(HostOwner::Live(ProcessHost::install(
                &self.stop,
                HostLimits::new(1, 2)?,
            )?));
        }
        let Some(HostOwner::Live(host)) = state.host.as_ref() else {
            return Err(io::ErrorKind::NotConnected.into());
        };
        if !matches!(
            host.ready(original.required_deadline()?).await,
            HostReadiness::Ready
        ) {
            operation.check()?;
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "AST SCIP Process host is unavailable",
            ));
        }
        let directory = compiler.cache_directory(installation, &image)?;
        original.check()?;
        std::fs::create_dir_all(directory.join("cache/target"))?;
        let target = target_cache(&directory, project);
        std::fs::create_dir_all(&target)?;
        let output = tempfile::Builder::new()
            .prefix("output-")
            .tempdir_in(&directory)?;
        let name = format!(
            "graphit-scip-{}-{}",
            compiler.label(),
            output
                .path()
                .file_name()
                .and_then(OsStr::to_str)
                .ok_or(io::ErrorKind::InvalidData)?
        );
        let mut args = vec![
            "run".into(),
            format!("--pull={}", self.pull).into(),
            "--name".into(),
            name.clone().into(),
            "--mount".into(),
            mount(project.root(), "/source", true)?,
            "--tmpfs".into(),
            "/overlay".into(),
            "--cap-add=SYS_ADMIN".into(),
            "--security-opt=apparmor=unconfined".into(),
            "--mount".into(),
            mount(&directory.join("cache"), "/cache", false)?,
            "--mount".into(),
            mount(&target, "/cache/target", false)?,
            "--mount".into(),
            mount(output.path(), "/output", false)?,
            "--env".into(),
            "GRAPHIT_SCIP_OVERLAY=1".into(),
            "--env".into(),
            format!(
                "GRAPHIT_SCIP_TIMEOUT_MS={}",
                original
                    .required_deadline()?
                    .checked_duration_since(Instant::now())
                    .ok_or(io::ErrorKind::TimedOut)?
                    .as_millis()
                    .max(1)
            )
            .into(),
        ];
        args.extend(compiler_ownership(output.path())?);
        args.push(image.clone().into());
        guard()?;
        eprintln!(
            "AST SCIP {}: Docker {} image {image} pull={}",
            compiler.label(),
            docker.display(),
            self.pull,
        );
        state.container = Some(Container {
            name,
            docker: docker.clone(),
            cwd: project.root().to_owned(),
            output,
        });
        let result = self
            .run(&mut state, docker, args, project.root(), original)
            .await;
        // Cancellation/deadline ends compilation, but is not a cancellation of cleanup.
        let cleanup_cancel = Cancellation::default();
        let cleanup = policy
            .request_context(Instant::now(), &cleanup_cancel)
            .map_err(err)?;
        let cleaned = self.remove_container(&mut state, cleanup.snapshot()).await;
        if let Err(cleanup) = cleaned {
            let compiler = result.err();
            let kind = compiler.as_ref().map_or(cleanup.kind(), io::Error::kind);
            return Err(io::Error::new(kind, CleanupFailure { compiler, cleanup }));
        }
        let container = state
            .container
            .take()
            .expect("owned container until removal");
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                // Only the compiler's derived budget may become unavailability.
                // Container removal and the Process join have already completed;
                // the caller's Original and authority must still permit fallback.
                if let Some(failure) =
                    completed_compiler_deadline(&error, original, state.remaining.is_some())
                {
                    guard()?;
                    if cancellation.is_cancelled() || self.stop.is_cancelled() {
                        return Err(io::ErrorKind::Interrupted.into());
                    }
                    eprintln!(
                        "AST SCIP {} compiler unavailable: reason={:?} status={:?} cleanup={:?}",
                        compiler.label(),
                        failure.reason,
                        failure.status,
                        failure.cleanup,
                    );
                    return Ok(None);
                }
                return Err(error);
            }
        };
        operation.check()?;
        guard()?;
        if !outcome.status.success() {
            eprintln!(
                "AST SCIP {} compiler unavailable: exit {:?}: stderr:\n{}\nstdout (first 64 KiB):\n{}",
                compiler.label(),
                outcome.status.code(),
                outcome
                    .transcript
                    .as_ref()
                    .map(|t| String::from_utf8_lossy(t.stderr()))
                    .unwrap_or_default(),
                outcome
                    .transcript
                    .as_ref()
                    .map(|t| {
                        let stdout = t.stdout();
                        String::from_utf8_lossy(&stdout[..stdout.len().min(64 * 1024)])
                    })
                    .unwrap_or_default()
            );
            return Ok(None);
        }
        match open_regular(container.output.path(), Path::new("index.scip"), original) {
            Ok(file) => {
                if file_identity(&file, original)?.1 == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "SCIP compiler produced an empty index",
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                operation.check()?;
                guard()?;
                eprintln!("AST SCIP {} compiler produced no index", compiler.label());
                return Ok(None);
            }
            Err(error) => return Err(error),
        }
        operation.check()?;
        guard()?;
        Ok(Some(ScipOutput {
            directory: container.output,
        }))
    }
    async fn run(
        &self,
        state: &mut State,
        program: PathBuf,
        args: Vec<OsString>,
        cwd: &Path,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<graphit_process::Outcome> {
        if let Some(remaining) = state.remaining.as_mut() {
            match remaining.repair(original.required_deadline()?).await {
                RepairObservation::Observed {
                    cleanup: Cleanup::Complete,
                    ..
                } => state.remaining = None,
                observation => {
                    return Err(io::Error::other(format!(
                        "AST SCIP Process repair: {observation:?}"
                    )));
                }
            }
        }
        let Some(HostOwner::Live(host)) = state.host.as_ref() else {
            return Err(io::ErrorKind::NotConnected.into());
        };
        let (_stop, receiver) = graphit_process::operation(original, host).map_err(err)?;
        let result = graphit_process::run(
            Request {
                program,
                args,
                cwd: cwd.to_owned(),
                environment: self.environment.clone(),
                stdin: None,
                limits: graphit_process::Limits {
                    stdin_bytes: 0,
                    stdout_bytes: 8 * 1024 * 1024,
                    stderr_bytes: 64 * 1024,
                    termination_grace: Duration::from_secs(2),
                    cleanup: CLEANUP,
                },
            },
            original,
            Retention::Full,
            &mut IgnoreChunks,
            receiver,
        )
        .await;
        match result {
            Ok(mut outcome) => {
                state.remaining = outcome.remaining.take();
                if outcome.cleanup != Cleanup::Complete || state.remaining.is_some() {
                    return Err(io::Error::other(format!(
                        "AST SCIP Process cleanup: {:?}",
                        outcome.cleanup
                    )));
                }
                Ok(outcome)
            }
            Err(mut failure) => {
                state.remaining = failure.remaining.take();
                if let Err(error) = original.check() {
                    return Err(io::Error::new(error.kind(), failure));
                }
                Err(err(failure))
            }
        }
    }
    async fn remove_container(
        &self,
        state: &mut State,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<()> {
        let Some(container) = &state.container else {
            return Ok(());
        };
        let docker = container.docker.clone();
        let cwd = container.cwd.clone();
        let name = container.name.clone();
        // Successful empty inventory proves a failed Docker startup created nothing.
        let inventory = self
            .run(
                state,
                docker.clone(),
                vec![
                    "ps".into(),
                    "--all".into(),
                    "--no-trunc".into(),
                    "--filter".into(),
                    format!("name=^/{name}$").into(),
                    "--format".into(),
                    "{{.ID}}".into(),
                ],
                &cwd,
                original,
            )
            .await?;
        if !inventory.status.success() {
            return Err(io::Error::other("cannot inspect owned SCIP container"));
        }
        let ids = inventory
            .transcript
            .as_ref()
            .map(|t| t.stdout())
            .unwrap_or_default();
        let id = std::str::from_utf8(ids).map_err(err)?.trim();
        if id.is_empty() {
            return Ok(());
        }
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let removed = self
            .run(
                state,
                docker,
                vec!["rm".into(), "--force".into(), id.into()],
                &cwd,
                original,
            )
            .await?;
        if !removed.status.success() {
            return Err(io::Error::other("owned SCIP container removal failed"));
        }
        Ok(())
    }
    pub async fn shutdown(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        let mut state = self.state.lock().await;
        let cancel = Cancellation::default();
        let original = policy
            .request_context(Instant::now(), &cancel)
            .map_err(err)?;
        if state.container.is_some() {
            self.remove_container(&mut state, original.snapshot())
                .await?;
            state.container.take();
        }
        self.stop.cancel();
        let deadline = Instant::now()
            .checked_add(CLEANUP)
            .ok_or(io::ErrorKind::InvalidInput)?;
        if matches!(state.host, Some(HostOwner::Live(_))) {
            let Some(HostOwner::Live(host)) = state.host.take() else {
                unreachable!()
            };
            state.host = Some(HostOwner::Joining(tokio::spawn(host.shutdown(deadline))));
        }
        if let Some(HostOwner::Joining(join)) = state.host.as_mut() {
            state.host = Some(match join.await {
                Ok(HostShutdown::Joined(report)) => HostOwner::Joined(report),
                Ok(HostShutdown::Pending(host)) => HostOwner::Pending(host),
                Err(error) => HostOwner::Failed(error),
            });
        }
        if let Some(HostOwner::Pending(host)) = state.host.as_mut()
            && let HostShutdownObservation::Joined(report) = host.repair(deadline).await
        {
            state.host = Some(HostOwner::Joined(report));
        }
        match &state.host {
            None => Ok(()),
            Some(HostOwner::Joined(report)) if report.failure().is_none() => Ok(()),
            Some(HostOwner::Joined(report)) => Err(io::Error::other(format!(
                "AST SCIP host failed: {:?}",
                report.failure()
            ))),
            Some(HostOwner::Failed(error)) => {
                Err(io::Error::other(format!("AST SCIP host join: {error}")))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "AST SCIP host join pending",
            )),
        }
    }
}

fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 128
        && (tag.as_bytes()[0].is_ascii_alphanumeric() || tag.as_bytes()[0] == b'_')
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
fn target_cache(directory: &Path, project: &ProjectContext) -> PathBuf {
    use std::fmt::Write as _;
    let digest = Sha256::digest(project.version.version().as_bytes());
    let mut version_root = String::with_capacity(64);
    for byte in digest {
        write!(version_root, "{byte:02x}").expect("writing to a String cannot fail");
    }
    directory
        .join("targets")
        .join(project.id.to_string())
        .join(version_root)
}
fn mount(path: &Path, destination: &str, readonly: bool) -> io::Result<OsString> {
    let source = path
        .to_str()
        .ok_or(io::ErrorKind::InvalidInput)?
        .replace('"', "\"\"");
    Ok(format!(
        "type=bind,\"source={source}\",target={destination}{}",
        if readonly { ",readonly" } else { "" }
    )
    .into())
}

// The image resolves a Windows bind mount's POSIX owner inside its Linux VM.
// A host SID is never converted to a fabricated UID/GID.
fn compiler_ownership(output: &Path) -> io::Result<Vec<OsString>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let owner = std::fs::metadata(output)?;
        Ok(vec![
            "--env".into(),
            format!("GRAPHIT_SCIP_UID={}", owner.uid()).into(),
            "--env".into(),
            format!("GRAPHIT_SCIP_GID={}", owner.gid()).into(),
        ])
    }
    #[cfg(windows)]
    {
        // The output TempDir remains owned until container removal on this path.
        if !std::fs::symlink_metadata(output)?.is_dir() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(vec![
            "--env".into(),
            "GRAPHIT_SCIP_OWNER=output-mount".into(),
        ])
    }
}

/// The private file can wait for the existing NN job mutex without retaining an index.
pub(super) struct ScipOutput {
    directory: tempfile::TempDir,
}
impl ScipOutput {
    /// Callers stage callback effects until both valid EOF and physical rechecks succeed.
    pub fn visit(
        &self,
        policy: &HubResourcePolicy,
        original: &SnapshotContext,
        visit: impl FnMut(IndexRecord) -> Result<(), AstScipError>,
    ) -> io::Result<::scip::types::Metadata> {
        original.check()?;
        let relative = Path::new("index.scip");
        let mut input = open_regular(self.directory.path(), relative, original)?;
        let before = file_identity(&input, original)?;
        let message_bytes = u32::try_from(policy.limit(ResourceSetting::SnapshotBytes).value)
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or(io::ErrorKind::InvalidInput)?;
        let result = visit_index(&mut input, original, message_bytes, visit);
        original.check()?;
        let current = open_regular(self.directory.path(), relative, original)?;
        if before != file_identity(&input, original)?
            || before != file_identity(&current, original)?
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "SCIP output changed while reading",
            ));
        }
        original.check()?;
        result.map_err(err)
    }
}

fn open_regular(root: &Path, relative: &Path, original: &SnapshotContext) -> io::Result<File> {
    open_node(root, relative, NodeKind::File, original)
}
fn open_node(
    root: &Path,
    relative: &Path,
    kind: NodeKind,
    original: &SnapshotContext,
) -> io::Result<File> {
    original.check()?;
    let mut components = relative.components().peekable();
    if (components.peek().is_none() && kind == NodeKind::File)
        || components
            .clone()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut parent = read_directory::open_root_readonly(root, &mut || original.check())?;
    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        let selected_kind = if components.peek().is_some() {
            NodeKind::Directory
        } else {
            kind
        };
        parent = read_directory::open_relative_readonly(
            &parent,
            name,
            Some(selected_kind),
            graphit_core::native_path::MAX_PATH_BYTES,
            &mut || original.check(),
        )?;
    }
    original.check()?;
    Ok(parent)
}
fn file_identity(
    file: &File,
    original: &SnapshotContext,
) -> io::Result<(PhysicalId, u64, std::time::SystemTime)> {
    let metadata = read_directory::inspect(file, Some(NodeKind::File), &mut || original.check())?;
    original.check()?;
    let identity = handle_id(file)?;
    original.check()?;
    Ok((identity, metadata.len(), metadata.modified()?))
}

#[derive(PartialEq)]
struct PhysicalObservation {
    identity: PhysicalId,
    length: u64,
    modified: std::time::SystemTime,
    sha256: [u8; 32],
}
fn observe(
    root: &Path,
    relative: &str,
    original: &SnapshotContext,
) -> io::Result<Option<PhysicalObservation>> {
    original.check()?;
    let relative = Path::new(relative);
    let mut file = match open_regular(root, relative, original) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        result => result?,
    };
    let before = file_identity(&file, original)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        original.check()?;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    let after = file_identity(&file, original)?;
    let current = open_regular(root, relative, original)?;
    if before != after || after != file_identity(&current, original)? {
        return Err(io::ErrorKind::InvalidData.into());
    }
    original.check()?;
    Ok(Some(PhysicalObservation {
        identity: after.0,
        length: after.1,
        modified: after.2,
        sha256: hash.finalize().into(),
    }))
}
/// Only hashes and filesystem identities survive between compiler execution and FILE parsing.
pub(super) struct CapturedInputs {
    root: PathBuf,
    source_paths: BTreeSet<String>,
    family: Option<ScipFamily>,
    manifest_directories: BTreeMap<String, BTreeSet<String>>,
    observations: BTreeMap<String, Option<PhysicalObservation>>,
}
impl CapturedInputs {
    pub async fn capture(
        raw: &LocalSourceIndex<'_>,
        root: &Path,
        compiler: impl Into<ScipCompiler>,
        paths: &[String],
        original: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let family = compiler.into().family();
        let mut observations = BTreeMap::new();
        let mut manifest_directories = BTreeMap::new();
        for path in paths {
            let physical = observe(root, path, original)?.ok_or(io::ErrorKind::NotFound)?;
            let source = raw.read(path, original).await.map_err(err)?;
            if let IndexedSource::Available(text) = source.source(original).map_err(err)?
                && <[u8; 32]>::from(Sha256::digest(text.as_bytes())) != physical.sha256
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "SCIP physical source differs from selected Raw",
                ));
            }
            observations.insert(path.clone(), Some(physical));
            let Some(family) = family else {
                continue;
            };
            let mut parent = Path::new(path).parent();
            while let Some(directory) = parent {
                for file in family.manifests() {
                    let relative = directory.join(file);
                    let relative = literal_path(&relative)?;
                    if !observations.contains_key(&relative) {
                        observations.insert(relative.clone(), observe(root, &relative, original)?);
                    }
                }
                let relative_directory = if directory.as_os_str().is_empty() {
                    String::new()
                } else {
                    literal_path(directory)?
                };
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    manifest_directories.entry(relative_directory)
                {
                    let names = manifest_names(root, directory, family, original)?;
                    for name in &names {
                        let relative = literal_path(&directory.join(name))?;
                        if !observations.contains_key(&relative) {
                            observations
                                .insert(relative.clone(), observe(root, &relative, original)?);
                        }
                    }
                    entry.insert(names);
                }
                parent = directory.parent();
            }
        }
        Ok(Self {
            root: root.to_owned(),
            source_paths: paths.iter().cloned().collect(),
            family,
            manifest_directories,
            observations,
        })
    }
    pub fn recheck(&self, original: &SnapshotContext) -> io::Result<()> {
        for (directory, names) in &self.manifest_directories {
            if &manifest_names(
                &self.root,
                Path::new(directory),
                self.family.ok_or(io::ErrorKind::InvalidData)?,
                original,
            )? != names
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "SCIP build manifest selection changed",
                ));
            }
        }
        for (path, observation) in &self.observations {
            if &observe(&self.root, path, original)? != observation {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "SCIP compiler input changed",
                ));
            }
        }
        original.check()
    }
    pub fn source_sha256(
        &self,
        path: &str,
        original: &SnapshotContext,
    ) -> io::Result<Option<[u8; 32]>> {
        original.check()?;
        if !self.source_paths.contains(path) {
            return Ok(None);
        }
        let observed = self
            .observations
            .get(path)
            .and_then(Option::as_ref)
            .ok_or(io::ErrorKind::InvalidData)?;
        if observe(&self.root, path, original)?.as_ref() != Some(observed) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "SCIP source changed before graph extraction",
            ));
        }
        original.check()?;
        Ok(Some(observed.sha256))
    }
}

fn manifest_names(
    root: &Path,
    relative: &Path,
    family: ScipFamily,
    original: &SnapshotContext,
) -> io::Result<BTreeSet<String>> {
    original.check()?;
    if !matches!(
        family,
        ScipFamily::TypeScript | ScipFamily::Python | ScipFamily::Dotnet | ScipFamily::Ruby
    ) {
        return Ok(BTreeSet::new());
    }
    let directory = open_node(root, relative, NodeKind::Directory, original)?;
    let mut entries =
        read_directory::DirectoryEntries::new(&directory, 64 * 1024, &mut || original.check())?;
    let mut names = BTreeSet::new();
    while let Some(name) = entries
        .next_name(graphit_core::native_path::MAX_PATH_BYTES, &mut || {
            original.check()
        })?
    {
        if let Some(name) = name.to_str()
            && family.is_named_manifest(name)
        {
            names.insert(name.to_owned());
        }
    }
    original.check()?;
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::brand::Brand;
    pub(super) fn context(project: &ProjectPaths, version: &str) -> ProjectContext {
        use graphit_core::version_context::{Checkout, VersionContext};
        ProjectContext {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            version: VersionContext::select(project, Checkout::NonGit, version, None, "").unwrap(),
        }
    }
    #[test]
    fn compiler_pull_policy_is_explicit_and_defaults_to_always() {
        let brand = Brand::default();
        assert_eq!(ScipService::new(Vec::new(), &brand).unwrap().pull, "always");
        let key = OsString::from(brand.environment_variable("AST_SCIP_PULL_POLICY").unwrap());
        for value in ["always", "missing", "never"] {
            assert_eq!(
                ScipService::new(vec![(key.clone(), value.into())], &brand)
                    .unwrap()
                    .pull,
                value
            );
        }
        for value in ["", "auto", "Never"] {
            let error = ScipService::new(vec![(key.clone(), value.into())], &brand)
                .err()
                .unwrap();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert!(error.to_string().contains("always, missing or never"));
        }
    }
    #[test]
    fn maintained_families_follow_the_selected_profile() {
        for (path, language, expected) in [
            ("main.go", "go", ScipFamily::Go),
            ("view.tsx", "tsx", ScipFamily::TypeScript),
            ("main.py", "python", ScipFamily::Python),
            ("Main.kt", "kotlin", ScipFamily::Java),
            ("main.mm", "objective-cpp", ScipFamily::Clang),
            ("main.cs", "csharp", ScipFamily::Dotnet),
            ("main.vb", "vbnet", ScipFamily::Dotnet),
            ("lib.rs", "rust", ScipFamily::Rust),
            ("main.rb", "ruby", ScipFamily::Ruby),
            ("main.dart", "dart", ScipFamily::Dart),
            ("main.php", "php", ScipFamily::Php),
        ] {
            assert_eq!(ScipFamily::for_path_profile(path, None), Some(expected));
            assert_eq!(
                ScipFamily::for_path_profile("custom.extension", Some(language)),
                Some(expected)
            );
        }
        assert_eq!(
            ScipFamily::for_path_profile("matrix.m", Some("matlab")),
            None
        );
        assert_eq!(
            ScipFamily::for_path_profile("view.tsx", Some("unknown")),
            None
        );
        assert_eq!(ScipFamily::for_path_profile("file.unknown", None), None);
        for extension in ["fs", "fsi", "fsx"] {
            assert_eq!(
                ScipFamily::for_path_profile(&format!("main.{extension}"), None),
                None
            );
        }
        for language in ["fsharp", "f_sharp"] {
            assert_eq!(
                ScipFamily::for_path_profile("main.cs", Some(language)),
                None
            );
        }
    }

    #[test]
    fn streaming_output_preserves_encoding_and_detects_replacement_and_cancellation() {
        use graphit_ast::scip_stream::{write_document, write_metadata};
        let installation = Brand::default();
        let keys = [
            ConfigKey::new("hub.limits.snapshot.bytes").unwrap(),
            ConfigKey::new("hub.limits.retained.bytes").unwrap(),
        ];
        let config = ConfigSnapshot::new(
            &installation,
            [ConfigSnapshot::capture_environment(
                &installation,
                &keys,
                keys.iter()
                    .map(|key| (key.environment_variable(&installation).into(), "256".into())),
            )
            .unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancellation = Cancellation::default();
        let original = SnapshotContext::service(cancellation.clone());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("index.scip");
        let limit = NonZeroU32::new(1_000_000).unwrap();
        let mut metadata = ::scip::types::Metadata::new();
        metadata.project_root = "file:///workspace".into();
        let mut document = ::scip::types::Document::new();
        document.relative_path = "lib.rs".into();
        let mut bytes = Vec::new();
        write_metadata(&mut bytes, &metadata, &original, limit).unwrap();
        write_document(&mut bytes, &document, &original, limit).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let output = ScipOutput { directory };
        let mut seen = 0;
        assert_eq!(
            output
                .visit(&policy, &original, |record| {
                    if let IndexRecord::Document(document) = record {
                        assert_eq!(document.relative_path, "lib.rs");
                        assert_eq!(document.position_encoding.value(), 0);
                        seen += 1;
                    }
                    Ok(())
                })
                .unwrap()
                .project_root,
            metadata.project_root
        );
        assert_eq!(seen, 1);
        let mut many_documents = Vec::new();
        write_metadata(&mut many_documents, &metadata, &original, limit).unwrap();
        for index in 0..40 {
            document.relative_path = format!("source-{index}.rs");
            write_document(&mut many_documents, &document, &original, limit).unwrap();
        }
        assert!(many_documents.len() > policy.limit(ResourceSetting::SnapshotBytes).value as usize);
        std::fs::write(&path, many_documents).unwrap();
        let mut seen = 0;
        output
            .visit(&policy, &original, |_| {
                seen += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            seen, 40,
            "the configured limit bounds each record, not the entire Index"
        );
        document.text = "x".repeat(512);
        let mut oversized = Vec::new();
        write_metadata(&mut oversized, &metadata, &original, limit).unwrap();
        write_document(&mut oversized, &document, &original, limit).unwrap();
        std::fs::write(&path, oversized).unwrap();
        assert!(output.visit(&policy, &original, |_| Ok(())).is_err());
        std::fs::write(&path, &bytes).unwrap();
        let replacement = output.directory.path().join("replacement");
        output
            .visit(&policy, &original, |_| {
                std::fs::write(&replacement, &bytes)?;
                std::fs::rename(&replacement, &path)?;
                Ok(())
            })
            .expect_err("replacing a valid file during staged visits must fail");
        let mut truncated = bytes.clone();
        truncated.extend_from_slice(&[0x12, 0x05, 0x01]);
        std::fs::write(&path, truncated).unwrap();
        assert!(
            output.visit(&policy, &original, |_| Ok(())).is_err(),
            "valid earlier callbacks do not qualify malformed EOF"
        );
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            output
                .visit(&policy, &original, |_| {
                    cancellation.cancel();
                    Ok(())
                })
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn family_build_manifest_selection_detects_additions_and_changes() {
        let temp = tempfile::tempdir().unwrap();
        let original = SnapshotContext::service(Cancellation::default());
        for (family, manifest) in [
            (ScipFamily::Go, "go.mod"),
            (ScipFamily::TypeScript, "package.json"),
            (ScipFamily::Python, "pyproject.toml"),
            (ScipFamily::Java, "pom.xml"),
            (ScipFamily::Clang, "compile_commands.json"),
            (ScipFamily::Dotnet, "global.json"),
            (ScipFamily::Rust, "Cargo.toml"),
            (ScipFamily::Ruby, "Gemfile"),
            (ScipFamily::Dart, "pubspec.yaml"),
            (ScipFamily::Php, "composer.json"),
        ] {
            assert!(family.manifests().contains(&manifest));
        }
        std::fs::write(temp.path().join("existing.csproj"), "<Project />").unwrap();
        let names =
            manifest_names(temp.path(), Path::new(""), ScipFamily::Dotnet, &original).unwrap();
        assert_eq!(names, BTreeSet::from(["existing.csproj".into()]));
        let captured = CapturedInputs {
            root: temp.path().to_owned(),
            source_paths: BTreeSet::new(),
            family: Some(ScipFamily::Dotnet),
            manifest_directories: BTreeMap::from([(String::new(), names)]),
            observations: BTreeMap::from([(
                "existing.csproj".into(),
                observe(temp.path(), "existing.csproj", &original).unwrap(),
            )]),
        };
        assert_eq!(
            captured
                .source_sha256("existing.csproj", &original)
                .unwrap(),
            None
        );
        assert_eq!(
            captured.source_sha256("generated.cs", &original).unwrap(),
            None
        );
        captured.recheck(&original).unwrap();
        std::fs::write(temp.path().join("new.fsproj"), "<Project />").unwrap();
        assert_eq!(
            captured.recheck(&original).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        std::fs::remove_file(temp.path().join("new.fsproj")).unwrap();
        std::fs::write(
            temp.path().join("existing.csproj"),
            "<Project><Changed /></Project>",
        )
        .unwrap();
        assert_eq!(
            captured.recheck(&original).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[cfg(windows)]
    #[test]
    fn windows_compiler_ownership_is_observed_by_the_image_not_fabricated() {
        let output = tempfile::tempdir().unwrap();
        assert_eq!(
            compiler_ownership(output.path()).unwrap(),
            vec![
                OsString::from("--env"),
                OsString::from("GRAPHIT_SCIP_OWNER=output-mount"),
            ]
        );
    }

    #[test]
    fn compiler_targets_use_project_identity_and_version_across_checkouts() {
        let temp = tempfile::tempdir().unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let first_root = temp.path().join("first");
        let other_root = temp.path().join("other");
        std::fs::create_dir(&first_root).unwrap();
        std::fs::create_dir(&other_root).unwrap();
        let first = ProjectPaths::at(first_root, &installation).unwrap();
        let other = ProjectPaths::at(other_root, &installation).unwrap();
        let original = context(&first, "feature/one");
        let target = target_cache(temp.path(), &original);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("compiled-project"), b"first").unwrap();
        let same_project = context(&other, "feature/one");
        assert_eq!(target_cache(temp.path(), &same_project), target);
        let mut other_project = context(&other, "feature/one");
        other_project.id = "01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap();
        for selection in [other_project, context(&first, "feature/two")] {
            let isolated = target_cache(temp.path(), &selection);
            std::fs::create_dir_all(&isolated).unwrap();
            assert!(!isolated.join("compiled-project").exists());
            std::fs::write(isolated.join("compiled-project"), b"other").unwrap();
        }
        assert_eq!(
            std::fs::read(target_cache(temp.path(), &original).join("compiled-project")).unwrap(),
            b"first"
        );
        assert_ne!(
            target_cache(&temp.path().join("rust"), &original),
            target_cache(&temp.path().join("go"), &original),
        );
    }
    #[test]
    fn physical_source_open_preserves_strict_capture_and_original_cancellation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(root.join("lib.rs"), b"fn a() {}\n").unwrap();
        std::fs::write(outside.join("foreign.rs"), b"fn foreign() {}\n").unwrap();
        let original = SnapshotContext::service(Cancellation::default());
        assert!(observe(&root, "lib.rs", &original).unwrap().is_some());
        assert!(observe(&root, "missing.rs", &original).unwrap().is_none());
        for relative in ["", "..", "../outside/foreign.rs", "missing/../foreign.rs"] {
            assert_eq!(
                observe(&root, relative, &original).err().unwrap().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        std::fs::create_dir(root.join("directory.rs")).unwrap();
        assert!(observe(&root, "directory.rs", &original).is_err());
        graphit_fs::LinkPlan::capture(&outside, &root.join("linked"), NodeKind::Directory)
            .unwrap()
            .create()
            .unwrap();
        assert!(observe(&root, "linked/foreign.rs", &original).is_err());
        std::fs::hard_link(root.join("lib.rs"), root.join("alias.rs")).unwrap();
        assert_eq!(
            observe(&root, "alias.rs", &original).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        std::fs::remove_file(root.join("alias.rs")).unwrap();
        let cancellation = Cancellation::default();
        let cancelled = SnapshotContext::service(cancellation.clone());
        cancellation.cancel();
        assert_eq!(
            observe(&root, "lib.rs", &cancelled).err().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn compiler_input_replacement_and_new_manifest_are_not_fresh() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("lib.rs"), "fn a() {}\n").unwrap();
        let cancellation = Cancellation::default();
        let original = SnapshotContext::service(cancellation);
        let observation = observe(temp.path(), "lib.rs", &original).unwrap();
        let captured = CapturedInputs {
            root: temp.path().to_owned(),
            source_paths: BTreeSet::from(["lib.rs".into()]),
            family: Some(ScipFamily::Rust),
            manifest_directories: BTreeMap::new(),
            observations: BTreeMap::from([
                ("lib.rs".into(), observation),
                ("Cargo.toml".into(), None),
            ]),
        };
        captured.recheck(&original).unwrap();
        assert_eq!(
            captured.source_sha256("lib.rs", &original).unwrap(),
            Some(Sha256::digest(b"fn a() {}\n").into())
        );
        assert_eq!(
            captured.source_sha256("Cargo.toml", &original).unwrap(),
            None
        );
        std::fs::write(temp.path().join("Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(
            captured.recheck(&original).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        #[cfg(unix)]
        {
            rustix::fs::mknodat(
                rustix::fs::CWD,
                temp.path().join("pipe.rs"),
                rustix::fs::FileType::Fifo,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                0,
            )
            .unwrap();
            assert_eq!(
                observe(temp.path(), "pipe.rs", &original)
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::InvalidData,
                "a FIFO must be rejected without blocking an indexing worker",
            );
        }
        std::fs::remove_file(temp.path().join("Cargo.toml")).unwrap();
        std::fs::write(temp.path().join("replacement"), "fn a() {}\n").unwrap();
        std::fs::rename(temp.path().join("replacement"), temp.path().join("lib.rs")).unwrap();
        assert_eq!(
            captured.recheck(&original).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[tokio::test]
    async fn denied_job_guard_never_installs_a_host_or_process() {
        let temp = tempfile::tempdir().unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let project = ProjectPaths::at(temp.path(), &installation).unwrap();
        let config = ConfigSnapshot::new(installation.brand(), []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let service = ScipService::new(Vec::new(), installation.brand()).unwrap();
        let cancellation = Cancellation::default();
        let result = service
            .index(
                ScipFamily::Rust,
                &context(&project, "main"),
                &installation,
                &config,
                &policy,
                &cancellation,
                &|| Err(io::ErrorKind::PermissionDenied.into()),
            )
            .await;
        assert_eq!(
            result.err().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(service.state.lock().await.host.is_none());
        service.shutdown(&policy).await.unwrap();
    }
    #[test]
    fn custom_image_is_exact_and_cache_is_isolated_by_image_and_project_version() {
        let temp = tempfile::tempdir().unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let project = ProjectPaths::at(temp.path(), &installation).unwrap();
        let config = ConfigSnapshot::new(
            installation.brand(),
            [graphit_core::config::ConfigDocument::new(
                installation.brand(),
                graphit_core::config::ConfigSource::Project,
                [(
                    ConfigKey::new("ast.scip.version").unwrap(),
                    serde_json::json!("ignored invalid tag for custom image"),
                )],
            )
            .unwrap()],
        )
        .unwrap();
        let first = ScipCompiler::CustomImage("localhost:5000/parser@sha256:abcd".into());
        assert_eq!(
            first.image(&config).unwrap(),
            "localhost:5000/parser@sha256:abcd"
        );
        let first_cache = first
            .cache_directory(&installation, first.as_str())
            .unwrap();
        let second = ScipCompiler::CustomImage("localhost:5000/parser:other".into());
        let second_cache = second
            .cache_directory(&installation, second.as_str())
            .unwrap();
        assert_ne!(first_cache, second_cache);
        assert_ne!(
            target_cache(&first_cache, &context(&project, "main")),
            target_cache(&first_cache, &context(&project, "feature"))
        );
        assert!(
            first_cache.starts_with(
                installation
                    .global_path(&["cache", "scip", "images"])
                    .unwrap()
            )
        );
    }
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compiler_families_use_current_compatibility_alias_with_registry_updates() {
        compiler_image_contract(None, "always").await;
    }
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compiler_families_use_never_pull_with_runtime_overlay_and_isolated_outputs() {
        compiler_image_contract(Some("never"), "never").await;
    }
    #[cfg(unix)]
    async fn compiler_image_contract(pull_override: Option<&str>, expected_pull: &str) {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let docker = temp.path().join("docker");
        std::fs::write(
            &docker,
            r#"#!/bin/sh
case "$1" in
run)
  printf '%s\n' "$@" > "$SCIP_TEST_DIR/argv"
  [ -e "$SCIP_TEST_DIR/image-missing" ] && exit 125
  for arg in "$@"; do
    case "$arg" in
      'type=bind,"source='*',target=/output')
        output=${arg#type=bind,\"source=}; output=${output%\",target=/output}
        printf 'nonempty compiler output' > "$output/index.scip" ;;
    esac
  done ;;
ps) printf '%s\n' aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ;;
rm) : > "$SCIP_TEST_DIR/removed" ;;
*) exit 91 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o700)).unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let project = ProjectPaths::at(temp.path(), &installation).unwrap();
        let config = ConfigSnapshot::new(installation.brand(), []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let mut environment = vec![
            ("PATH".into(), temp.path().as_os_str().to_owned()),
            ("SCIP_TEST_DIR".into(), temp.path().as_os_str().to_owned()),
        ];
        if let Some(pull) = pull_override {
            environment.push(("GRAPHIT_AST_SCIP_PULL_POLICY".into(), pull.into()));
        }
        let service = ScipService::new(environment, installation.brand()).unwrap();
        let cancellation = Cancellation::default();
        let context = context(&project, "main");
        let mut outputs = BTreeSet::new();
        let mut names = BTreeSet::new();
        for family in [
            ScipFamily::Go,
            ScipFamily::TypeScript,
            ScipFamily::Python,
            ScipFamily::Java,
            ScipFamily::Clang,
            ScipFamily::Dotnet,
            ScipFamily::Rust,
            ScipFamily::Ruby,
            ScipFamily::Dart,
            ScipFamily::Php,
        ] {
            let output = service
                .index(
                    family,
                    &context,
                    &installation,
                    &config,
                    &policy,
                    &cancellation,
                    &|| Ok(()),
                )
                .await
                .unwrap()
                .expect("fake compiler output");
            let argv = std::fs::read_to_string(temp.path().join("argv")).unwrap();
            let args: Vec<_> = argv.lines().collect();
            let directory = installation
                .global_path(&["cache", "scip", family.as_str()])
                .unwrap();
            assert!(args.contains(&format!("--pull={expected_pull}").as_str()));
            for required in [
                "--tmpfs",
                "/overlay",
                "--cap-add=SYS_ADMIN",
                "--security-opt=apparmor=unconfined",
                "GRAPHIT_SCIP_OVERLAY=1",
            ] {
                assert!(args.contains(&required), "{family:?}: {required}");
            }
            for required in [
                mount(project.root(), "/source", true).unwrap(),
                mount(&directory.join("cache"), "/cache", false).unwrap(),
                mount(&target_cache(&directory, &context), "/cache/target", false).unwrap(),
                mount(output.directory.path(), "/output", false).unwrap(),
            ] {
                assert!(
                    args.contains(&required.to_str().unwrap()),
                    "{family:?}: {required:?}"
                );
            }
            for required in compiler_ownership(output.directory.path()).unwrap() {
                assert!(args.contains(&required.to_str().unwrap()));
            }
            assert!(!argv.contains("target=/workspace"));
            assert!(!argv.contains("GRAPHIT_SCIP_FAMILY"));
            assert!(!argv.contains("GRAPHIT_SCIP_OWNER="));
            assert!(
                args.iter()
                    .find_map(|arg| arg.strip_prefix("GRAPHIT_SCIP_TIMEOUT_MS="))
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
                    > 0
            );
            assert_eq!(
                args.last().unwrap(),
                &format!("ghcr.io/graphit-labs/graphit-scip-{}:v1", family.as_str())
            );
            let name = args.windows(2).find(|pair| pair[0] == "--name").unwrap()[1];
            assert!(name.starts_with(&format!("graphit-scip-{}-", family.as_str())));
            assert!(names.insert(name.to_owned()));
            assert!(outputs.insert(output.directory.path().to_owned()));
            assert!(temp.path().join("removed").exists());
            assert_eq!(
                std::fs::read(output.directory.path().join("index.scip")).unwrap(),
                b"nonempty compiler output"
            );
            assert!(service.state.lock().await.container.is_none());
        }
        std::fs::write(temp.path().join("image-missing"), "").unwrap();
        assert!(
            service
                .index(
                    ScipFamily::Clang,
                    &context,
                    &installation,
                    &config,
                    &policy,
                    &cancellation,
                    &|| Ok(())
                )
                .await
                .unwrap()
                .is_none()
        );
        assert!(service.state.lock().await.container.is_none());
        assert!(
            std::fs::read_to_string(temp.path().join("argv"))
                .unwrap()
                .lines()
                .any(|arg| arg == format!("--pull={expected_pull}"))
        );
        service.shutdown(&policy).await.unwrap();
    }
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compiler_failure_missing_output_and_cleanup_keep_distinct_outcomes() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let docker = temp.path().join("docker");
        std::fs::write(
            &docker,
            r#"#!/bin/sh
IFS= read -r mode < "$SCIP_TEST_DIR/mode"
case "$1" in
run)
  : > "$SCIP_TEST_DIR/ran"
  [ "$mode" = exit ] && exit 23
  [ "$mode" = missing ] && exit 0
  for arg in "$@"; do
    case "$arg" in
      'type=bind,"source='*',target=/output')
        output=${arg#type=bind,\"source=}; output=${output%\",target=/output}
        case "$mode" in
          empty) : > "$output/index.scip" ;;
          link) /bin/ln -s "$SCIP_TEST_DIR/foreign" "$output/index.scip" ;;
          *) printf x > "$output/index.scip" ;;
        esac ;;
    esac
  done ;;
ps) printf '%s\n' aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ;;
rm)
  [ "$mode" = cleanup ] && exit 24
  : > "$SCIP_TEST_DIR/removed" ;;
*) exit 91 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(temp.path().join("foreign"), "foreign index").unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let project = ProjectPaths::at(temp.path(), &installation).unwrap();
        let config = ConfigSnapshot::new(installation.brand(), []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancellation = Cancellation::default();
        let context = context(&project, "main");
        for mode in ["exit", "missing", "empty", "link", "denied", "cleanup"] {
            std::fs::write(temp.path().join("mode"), format!("{mode}\n")).unwrap();
            let _ = std::fs::remove_file(temp.path().join("ran"));
            let _ = std::fs::remove_file(temp.path().join("removed"));
            let service = ScipService::new(
                vec![
                    ("PATH".into(), temp.path().as_os_str().to_owned()),
                    ("SCIP_TEST_DIR".into(), temp.path().as_os_str().to_owned()),
                ],
                installation.brand(),
            )
            .unwrap();
            let guard = || {
                if mode == "denied" && temp.path().join("ran").exists() {
                    Err(io::ErrorKind::PermissionDenied.into())
                } else {
                    Ok(())
                }
            };
            let result = service
                .index(
                    ScipFamily::Rust,
                    &context,
                    &installation,
                    &config,
                    &policy,
                    &cancellation,
                    &guard,
                )
                .await;
            match mode {
                "exit" | "missing" => assert!(result.unwrap().is_none()),
                "empty" => {
                    assert_eq!(result.err().unwrap().kind(), io::ErrorKind::InvalidData)
                }
                "link" => assert_eq!(
                    result.err().unwrap().raw_os_error(),
                    Some(rustix::io::Errno::LOOP.raw_os_error()),
                ),
                "denied" => assert_eq!(
                    result.err().unwrap().kind(),
                    io::ErrorKind::PermissionDenied
                ),
                "cleanup" => {
                    let error = result.err().unwrap();
                    assert!(error.get_ref().unwrap().is::<CleanupFailure>());
                    assert!(service.state.lock().await.container.is_some());
                    assert!(!temp.path().join("removed").exists());
                    let blocked = service
                        .index(
                            ScipFamily::Go,
                            &context,
                            &installation,
                            &config,
                            &policy,
                            &cancellation,
                            &|| Ok(()),
                        )
                        .await;
                    assert_eq!(blocked.err().unwrap().kind(), io::ErrorKind::WouldBlock);
                }
                _ => unreachable!(),
            }
            if mode != "cleanup" {
                assert!(temp.path().join("removed").exists(), "cleanup for {mode}");
                assert!(service.state.lock().await.container.is_none());
            }
            std::fs::write(temp.path().join("mode"), "repaired\n").unwrap();
            service.shutdown(&policy).await.unwrap();
            assert!(service.state.lock().await.container.is_none());
        }
        let service = ScipService::new(
            vec![(
                "PATH".into(),
                temp.path().join("missing-bin").into_os_string(),
            )],
            installation.brand(),
        )
        .unwrap();
        assert!(
            service
                .index(
                    ScipFamily::Go,
                    &context,
                    &installation,
                    &config,
                    &policy,
                    &cancellation,
                    &|| Ok(())
                )
                .await
                .unwrap()
                .is_none()
        );
        assert!(service.state.lock().await.host.is_none());
        service.shutdown(&policy).await.unwrap();
    }

    #[tokio::test]
    async fn queued_compiler_deadline_and_cancellation_never_become_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let project = ProjectPaths::at(temp.path(), &installation).unwrap();
        let keys = [ConfigKey::new("hub.limits.request.deadline_ms").unwrap()];
        let config = ConfigSnapshot::new(
            installation.brand(),
            [ConfigSnapshot::capture_environment(
                installation.brand(),
                &keys,
                [(
                    keys[0].environment_variable(installation.brand()).into(),
                    "20".into(),
                )],
            )
            .unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let service = ScipService::new(Vec::new(), installation.brand()).unwrap();
        let held = service.state.lock().await;
        let cancellation = Cancellation::default();
        let context = context(&project, "main");
        let result = service
            .index(
                ScipFamily::Rust,
                &context,
                &installation,
                &config,
                &policy,
                &cancellation,
                &|| Ok(()),
            )
            .await;
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::TimedOut);
        cancellation.cancel();
        let result = service
            .index(
                ScipFamily::Rust,
                &context,
                &installation,
                &config,
                &policy,
                &cancellation,
                &|| Ok(()),
            )
            .await;
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::Interrupted);
        assert!(held.host.is_none());
        drop(held);
        service.shutdown(&policy).await.unwrap();
    }
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_compiler_keeps_cleanup_and_joins_the_process_host() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let docker = temp.path().join("docker");
        std::fs::write(
            &docker,
            r#"#!/bin/sh
case "$1" in
run) : > "$SCIP_TEST_DIR/started"; trap 'exit 130' TERM INT; while :; do /bin/sleep 1; done ;;
ps) printf '%s\n' aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ;;
rm) : > "$SCIP_TEST_DIR/removed" ;;
*) exit 91 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o700)).unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let project = ProjectPaths::at(temp.path(), &installation).unwrap();
        let config = ConfigSnapshot::new(installation.brand(), []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let environment = vec![
            ("PATH".into(), temp.path().as_os_str().to_owned()),
            ("SCIP_TEST_DIR".into(), temp.path().as_os_str().to_owned()),
        ];
        let service = ScipService::new(environment, installation.brand()).unwrap();
        let cancellation = Cancellation::default();
        let stop = cancellation.clone();
        let marker = temp.path().join("started");
        let stopper = tokio::spawn(async move {
            let started = tokio::time::timeout(Duration::from_secs(5), async {
                while !marker.exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .is_ok();
            stop.cancel();
            started
        });
        let result = service
            .index(
                ScipFamily::Rust,
                &context(&project, "main"),
                &installation,
                &config,
                &policy,
                &cancellation,
                &|| Ok(()),
            )
            .await;
        assert!(
            stopper.await.unwrap(),
            "actual fixture process must have started"
        );
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::Interrupted);
        assert!(
            temp.path().join("removed").exists(),
            "cleanup must run after job cancellation"
        );
        service.shutdown(&policy).await.unwrap();
    }
}

#[cfg(test)]
#[path = "scip_deadline_tests.rs"]
mod deadline_tests;
