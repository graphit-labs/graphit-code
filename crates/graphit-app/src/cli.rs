#[cfg(target_os = "linux")]
mod daemon_limits;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use graphit_core::{
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource, ConfigStore},
    file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
    project_inventory::{GlobalProjectInventory, ProjectInventorySnapshot},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::policy::{HubResourcePolicy, RESOURCE_LIMITS, ResourceSetting};
use std::io::Write as _;
use std::{
    collections::BTreeMap, ffi::OsString, io, path::PathBuf, process::ExitCode, time::Instant,
};

use crate::{build_metadata::Metadata, package, version};
use graphit_app::{
    ast_source::SourceArguments, auth_active, auth_cli, auth_entry,
    foreground_source::ForegroundSource, service_entry::ServiceCommand, source_entry,
};

#[derive(Parser)]
struct Arguments {
    /// Print the compiled build version.
    #[arg(long, exclusive = true)]
    version: bool,
    /// Override a configuration value; repeat for different keys (the last value wins).
    #[arg(short = 'c', long = "config", global = true, value_name = "KEY=VALUE", value_parser = parse_inline)]
    config: Vec<(ConfigKey, serde_json::Value)>,
    /// Run without interactive prompts.
    #[arg(long, global = true)]
    non_interactive: bool,
    /// Select the retained version of the current project.
    #[arg(long, global = true)]
    project_version: Option<String>,
    /// Override the installation directory (otherwise BRAND_GLOBAL_DIR or the home directory).
    #[arg(long, global = true)]
    global_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Configure this installation and optionally initialize a maintained project.
    Setup(crate::setup::Arguments),
    /// Reconcile installed Hub artifacts, native skills/agents, Git/agent hooks and MCP.
    Sync(graphit_app::sync_entry::Arguments),
    /// Reconcile maintained module skills independently.
    Skills(graphit_app::sync_entry::SkillsArguments),
    /// Inspect or persist the current Rust configuration.
    Config(graphit_app::config_entry::Arguments),
    /// Install the latest verified release and resume this installation's active daemon.
    SelfUpdate,
    /// Open the configured Browser served by the active daemon.
    Ui(graphit_app::ui_entry::Arguments),
    /// Control this installation from the Linux desktop tray.
    #[cfg(target_os = "linux")]
    Tray,
    /// Update the running daemon's maintained PROJECT inventory.
    Projects(graphit_app::foreground_project_control::client::Arguments),
    /// Follow the live Broker Now view for an explicitly selected project.
    Now(graphit_app::now_entry::Arguments),
    #[command(name = "_session-hook", hide = true)]
    SessionHook(graphit_app::session_hook_entry::Arguments),
    /// Search, query or export Knowledge through the active service.
    Knowledge(KnowledgeArguments),
    /// Maintain the authoritative indexes of an authorized Memory corpus.
    Memory(MemoryArguments),
    /// Manage authentication providers in the private installation.
    Provider(Box<auth_cli::ProviderArguments>),
    /// List, inspect or select local profiles.
    Account(auth_cli::AccountArguments),
    /// Log in through a configured Local or Broker provider.
    Login(Box<auth_cli::LoginArguments>),
    /// Remove a stored profile.
    Logout(auth_cli::LogoutArguments),
    /// Refresh the active native Broker profile.
    Refresh(auth_cli::RefreshArguments),
    /// Export, inspect or import native packages.
    Package(package::Arguments),
    /// Install or synchronize three managed roles on the active daemon.
    Roles(graphit_app::managed_roles::client::Arguments),
    /// Install PROJECT context hooks on the active daemon.
    Hooks(graphit_app::project_hook_install::client::Arguments),
    /// Publish captured native SDK artifacts to an authorized Hub provider.
    Hub(graphit_app::hub_publish::Arguments),
    /// Publish the product Coordinator and PROJECT context hooks.
    Coordinator(graphit_app::project_coordinator_install::client::Arguments),
    /// Install or select local embedding models.
    Models(ModelsArguments),
    /// Read and mutate native Task/Session records through the active Source service.
    Task(Box<graphit_app::task_entry::Arguments>),
    /// Configure Git hooks and bind native Task/Memory state to commits and merges.
    Git(graphit_app::git_records::Arguments),
    /// Inspect source from the authenticated local service.
    Ast(Box<AstArguments>),
    /// Serve the MCP client transport over standard input/output.
    Mcp(McpArguments),
    /// Run or manage the authenticated local Source service.
    Daemon(DaemonArguments),
}
#[derive(clap::Args)]
struct KnowledgeArguments {
    #[command(subcommand)]
    command: KnowledgeCommand,
}
#[derive(Subcommand)]
enum KnowledgeCommand {
    /// Return the final answer from the service's captured provider binding.
    Ask(graphit_app::knowledge_ask_entry::Arguments),
    /// Search an authorized PROJECT or an installed Knowledge artifact.
    Search(graphit_app::knowledge_search_entry::Arguments),
    /// Read complete native Knowledge Markdown, or an explicit line/pattern slice.
    Source(graphit_app::knowledge_source_entry::Arguments),
    /// Read committed document synchronization history in the selected PROJECT.
    Log(graphit_app::knowledge_log_entry::Arguments),
    /// Freeze the current Knowledge snapshots and export an installable native package.
    Export(graphit_app::knowledge_export_entry::Arguments),
}
#[derive(clap::Args)]
struct MemoryArguments {
    #[command(subcommand)]
    command: MemoryCommand,
}
#[derive(Subcommand)]
enum MemoryCommand {
    /// Ensure or explicitly refresh lexical indexes and configured embeddings.
    Index(graphit_app::memory_index_entry::Arguments),
}
#[derive(clap::Args)]
struct AstArguments {
    #[command(subcommand)]
    command: AstCommand,
}
#[derive(Subcommand)]
enum AstCommand {
    /// Read indexed source or the physical file in the retained project version.
    Source(SourceArguments),
    /// Export a published PROJECT graph or an installed AST context.
    Export(graphit_app::ast_export_entry::Arguments),
    #[command(flatten)]
    Graph(graphit_app::graph_entry::Command),
}
#[derive(clap::Args)]
struct McpArguments {
    #[arg(long, required = true)]
    stdio: bool,
    /// Forward only the transient credential of an owned managed-role run.
    #[arg(long)]
    role: bool,
    /// Use only the process-bound Web credentials of an owned Live turn.
    #[arg(long, conflicts_with_all = ["role", "project_dir"])]
    live_user: bool,
    /// Default PROJECT locator for the authenticated Root Source consumer.
    #[arg(long, conflicts_with_all = ["role", "live_user"])]
    project_dir: Option<PathBuf>,
}
#[derive(clap::Args)]
struct DaemonArguments {
    /// Run the registered daemon from the installation's maintained inventory.
    #[arg(long, conflicts_with = "detached")]
    managed: bool,
    /// Run a Linux detached daemon from the maintained inventory.
    #[arg(long, hide = true, conflicts_with = "managed")]
    detached: bool,
    #[command(subcommand)]
    command: Option<DaemonCommand>,
}
#[derive(Subcommand)]
enum DaemonCommand {
    /// Run until SIGINT or SIGTERM, then join jobs and remove owned discovery/key.
    Foreground,
    #[command(flatten)]
    Control(ServiceCommand),
}
#[derive(clap::Args)]
struct ModelsArguments {
    #[command(flatten)]
    operation: graphit_app::models::Arguments,
    /// Maximum operation duration in seconds.
    #[arg(long, global = true, default_value = "3600")]
    timeout: std::num::NonZeroU64,
}

pub(crate) fn run(
    arguments: impl IntoIterator<Item = OsString>,
    metadata: &Metadata<'_>,
) -> ExitCode {
    let raw_arguments: Vec<_> = arguments.into_iter().collect();
    let mut parser = Arguments::command()
        .name(metadata.brand().binary_name().to_owned())
        .about("Graphit Code: A Powerful Agent Harness for Enterprise Software Ecosystems")
        .arg_required_else_help(true);
    if raw_arguments
        .iter()
        .any(|argument| argument.to_str().is_none())
    {
        let error = parser.error(
            clap::error::ErrorKind::InvalidUtf8,
            "command arguments and paths must be valid Unicode",
        );
        return if error.print().is_ok() {
            ExitCode::from(2)
        } else {
            ExitCode::FAILURE
        };
    }
    let arguments = parser
        .try_get_matches_from_mut(
            std::iter::once(OsString::from(metadata.brand().binary_name()))
                .chain(raw_arguments.iter().cloned()),
        )
        .and_then(|matches| Arguments::from_arg_matches(&matches));
    let arguments = match arguments {
        Ok(arguments) => arguments,
        Err(error) => {
            let code = error.exit_code() as u8;
            return if error.print().is_ok() {
                ExitCode::from(code)
            } else {
                ExitCode::FAILURE
            };
        }
    };
    if arguments.version && arguments.command.is_some() {
        let error = parser.error(
            clap::error::ErrorKind::ArgumentConflict,
            "--version cannot be used with a command",
        );
        return if error.print().is_ok() {
            ExitCode::from(2)
        } else {
            ExitCode::FAILURE
        };
    }
    if arguments.version {
        return match version::render(metadata, io::stdout()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                let _ = version::write_diagnostic(&error, io::stderr());
                ExitCode::FAILURE
            }
        };
    }
    let Some(command) = arguments.command else {
        return ExitCode::from(2);
    };
    if arguments.project_version.is_some()
        && matches!(&command, Command::Package(args) if args.is_install())
    {
        let error = parser.error(
            clap::error::ErrorKind::ArgumentConflict,
            "--project-version cannot be used with package install",
        );
        return if error.print().is_ok() {
            ExitCode::from(2)
        } else {
            ExitCode::FAILURE
        };
    }
    if let Command::Daemon(arguments) = &command
        && if arguments.managed || arguments.detached {
            arguments.command.is_some()
        } else {
            arguments.command.is_none()
        }
    {
        let error = parser.error(
            clap::error::ErrorKind::ArgumentConflict,
            "daemon requires one execution mode or a control subcommand",
        );
        return if error.print().is_ok() {
            ExitCode::from(2)
        } else {
            ExitCode::FAILURE
        };
    }
    // Parsed arguments own their data. The full command/help tree is no longer
    // needed while a long-running MCP, daemon or service command is dispatched.
    drop(parser);
    let result = dispatch(
        command,
        arguments.config,
        arguments.project_version,
        arguments.global_dir,
        arguments.non_interactive,
        metadata,
    );
    match result {
        Ok(code) => code,
        Err(error) => {
            diagnostic(metadata.brand().binary_name(), error.as_ref());
            if let Some(code) = graphit_app::remote_hub_catalog::client::cli_exit_code(
                error.as_ref(),
            )
            .or_else(|| graphit_app::remote_file_install::lifecycle::cli_exit_code(error.as_ref()))
            {
                return ExitCode::from(code);
            }
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::Interrupted)
                || error
                    .downcast_ref::<DaemonInvocationFailure>()
                    .is_some_and(|e| e.interrupted)
                || error
                    .downcast_ref::<DesktopInvocationFailure>()
                    .is_some_and(|e| e.interrupted)
                || error
                    .downcast_ref::<SelfUpdateInvocationFailure>()
                    .is_some_and(|e| e.interrupted)
            {
                ExitCode::from(130)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

fn diagnostic(prefix: &str, error: &(dyn std::error::Error + 'static)) {
    use io::Write;
    let mut output = io::stderr().lock();
    let _ = write!(output, "{prefix}: {error}");
    let mut cause = error.source();
    while let Some(error) = cause {
        let _ = write!(output, ": {error}");
        cause = error.source();
    }
    let _ = writeln!(output);
}
fn cancellation() -> Result<Cancellation, ctrlc::Error> {
    let cancellation = Cancellation::default();
    let interrupted = cancellation.clone();
    ctrlc::set_handler(move || interrupted.cancel())?;
    Ok(cancellation)
}
fn parse_inline(argument: &str) -> io::Result<(ConfigKey, serde_json::Value)> {
    let (key, value) = argument.split_once('=').ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "--config requires KEY=VALUE")
    })?;
    let key = ConfigKey::new(key)?;
    let value = key.parse_value(value)?;
    Ok((key, value))
}
fn command_config(
    brand: &graphit_core::brand::Brand,
    arguments: Vec<(ConfigKey, serde_json::Value)>,
) -> io::Result<ConfigDocument> {
    ConfigDocument::new(
        brand,
        ConfigSource::Command,
        arguments.into_iter().collect::<BTreeMap<_, _>>(),
    )
}

fn native_limits(policy: &HubResourcePolicy) -> io::Result<NativeDocumentFileLimits> {
    let number = |setting| usize::try_from(policy.limit(setting).value).map_err(io::Error::other);
    NativeDocumentFileLimits::new(
        number(ResourceSetting::ContentInputBytes)?,
        graphit_core::native_path::MAX_PATH_BYTES,
        number(ResourceSetting::SnapshotDepth)?,
        number(ResourceSetting::ContentScalarBytes)?,
    )
}

fn command_project(
    command: &mut Command,
    cwd: &std::path::Path,
    installation: &InstallationPaths,
) -> io::Result<ProjectPaths> {
    let explicit_project = match &mut *command {
        Command::Roles(arguments) => arguments.command.source_project(),
        Command::Sync(arguments) => arguments.project_dir.as_deref(),
        Command::Skills(graphit_app::sync_entry::SkillsArguments {
            command: graphit_app::sync_entry::SkillsCommand::Sync(selection),
        }) => selection.source_project.as_deref(),
        Command::SessionHook(arguments) => arguments.project_dir.as_deref(),
        Command::Hub(arguments) => arguments.command.project_dir(),
        Command::Memory(MemoryArguments {
            command: MemoryCommand::Index(arguments),
        }) => arguments.project_dir.as_deref(),
        Command::Mcp(arguments) if !arguments.role => arguments.project_dir.as_deref(),
        Command::Knowledge(KnowledgeArguments {
            command: KnowledgeCommand::Ask(arguments),
        }) => arguments.project_dir.as_deref().map(std::path::Path::new),
        Command::Knowledge(KnowledgeArguments {
            command: KnowledgeCommand::Export(arguments),
        }) => arguments.project_dir.as_deref(),
        Command::Knowledge(KnowledgeArguments {
            command: KnowledgeCommand::Search(arguments),
        }) => arguments.project_dir.as_deref(),
        Command::Knowledge(KnowledgeArguments {
            command: KnowledgeCommand::Source(arguments),
        }) => arguments.project_dir.as_deref(),
        Command::Knowledge(KnowledgeArguments {
            command: KnowledgeCommand::Log(arguments),
        }) => arguments.project_dir.as_deref(),
        Command::Task(arguments) => arguments.command.selection_mut().project_dir.as_deref(),
        Command::Ast(ast) => match &mut ast.command {
            AstCommand::Graph(command) => command
                .selection_mut()
                .and_then(|selection| selection.project_dir.as_deref()),
            AstCommand::Source(source) => source.project_dir.as_deref().map(std::path::Path::new),
            AstCommand::Export(arguments) => arguments.project_dir.as_deref(),
        },
        _ => None,
    };
    let directory = explicit_project.map_or_else(|| cwd.to_owned(), |path| cwd.join(path));
    let project = ProjectPaths::discover(directory, installation)?;
    match command {
        Command::Task(arguments) => {
            let selection = arguments.command.selection_mut();
            if selection.project_dir.is_some() {
                selection.project_dir = Some(project.root().to_owned());
            }
        }
        Command::Ast(ast) => match &mut ast.command {
            AstCommand::Graph(command) => {
                if let Some(selection) = command.selection_mut()
                    && selection.project_dir.is_some()
                {
                    selection.project_dir = Some(project.root().to_owned());
                }
            }
            AstCommand::Source(arguments) => {
                if arguments.project_dir.is_some() {
                    arguments.project_dir = Some(
                        project
                            .root()
                            .to_str()
                            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?
                            .to_owned(),
                    );
                }
            }
            AstCommand::Export(arguments) => {
                if arguments.project_dir.is_some() {
                    arguments.project_dir = Some(project.root().to_owned());
                }
            }
        },
        Command::Knowledge(KnowledgeArguments {
            command: KnowledgeCommand::Ask(arguments),
        }) if arguments.project_dir.is_some() => {
            arguments.project_dir = Some(
                project
                    .root()
                    .to_str()
                    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?
                    .to_owned(),
            );
        }
        _ => {}
    }
    Ok(project)
}

fn dispatch(
    command: Command,
    inline: Vec<(ConfigKey, serde_json::Value)>,
    selected_version: Option<String>,
    global_dir: Option<PathBuf>,
    non_interactive: bool,
    metadata: &Metadata<'_>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    if matches!(&command, Command::Daemon(arguments)
        if !matches!(arguments.command, Some(DaemonCommand::Control(_))))
    {
        daemon_limits::raise_nofile_soft_limit()?;
    }
    enum Route {
        Standard(Command),
        Install(package::InstallArguments),
    }
    let command = match command {
        Command::Package(arguments) => match arguments.classify() {
            package::Operation::Install(arguments) => Route::Install(arguments),
            package::Operation::Physical(command) => {
                let config = ConfigSnapshot::new(
                    metadata.brand(),
                    [
                        HubResourcePolicy::distribution_defaults(metadata.brand())?,
                        command_config(metadata.brand(), inline)?,
                    ],
                )?;
                return Ok(package::run(command, &config, &cancellation()?));
            }
        },
        command => Route::Standard(command),
    };
    // Bootstrap inputs are observed once. Config readers do not discover environment or create data.
    let cwd = std::env::current_dir()?;
    let home = std::env::home_dir();
    let variables: Vec<_> = std::env::vars_os().collect();
    let variable = metadata.brand().environment_variable("GLOBAL_DIR")?;
    let global_dir = global_dir.or_else(|| {
        variables
            .iter()
            .find(|(key, _)| key == std::ffi::OsStr::new(&variable))
            .map(|(_, value)| PathBuf::from(value))
    });
    let installation = InstallationPaths::resolve(
        metadata.brand().clone(),
        &cwd,
        home.as_deref(),
        global_dir.as_deref(),
    )?;
    let command = match command {
        Route::Install(arguments) => {
            return install_dispatch(arguments, &installation, &cwd, variables, inline);
        }
        Route::Standard(command) => command,
    };
    let command = match command {
        Command::Config(arguments) => {
            if selected_version.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "config operates on the current checkout, not a retained project version",
                )
                .into());
            }
            graphit_app::config_entry::run(
                arguments,
                &installation,
                &cwd,
                &variables,
                inline,
                environment_keys()?,
                &cancellation()?,
            )?;
            return Ok(ExitCode::SUCCESS);
        }
        #[cfg(target_os = "linux")]
        Command::Tray => {
            if selected_version.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "tray does not select a project version",
                )
                .into());
            }
            return tray_dispatch(&installation, &variables, inline);
        }
        Command::Ui(arguments) => {
            if selected_version.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "ui does not select a project version",
                )
                .into());
            }
            return ui_dispatch(arguments, &installation, &variables, inline);
        }
        Command::SelfUpdate => {
            if selected_version.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "self-update does not select a project version",
                )
                .into());
            }
            return self_update_dispatch(&installation, &variables, inline, metadata.version());
        }
        Command::Setup(arguments) => {
            return setup_dispatch(
                arguments,
                &installation,
                variables,
                inline,
                selected_version,
            );
        }
        Command::Models(arguments) => {
            if selected_version.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "model installation does not select a project version",
                )
                .into());
            }
            let deadline = Instant::now()
                .checked_add(std::time::Duration::from_secs(arguments.timeout.get()))
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
            let original = std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
                deadline,
                cancellation()?,
            ));
            return Ok(graphit_app::models::run(
                arguments.operation,
                &installation,
                original,
            ));
        }
        command => command,
    };
    if matches!(&command, Command::Daemon(arguments) if arguments.managed || arguments.detached) {
        if selected_version.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "managed daemon selects versions from the maintained inventory",
            )
            .into());
        }
        let detached = matches!(&command, Command::Daemon(arguments) if arguments.detached);
        return managed_daemon_dispatch(&installation, variables, inline, detached);
    }
    let mut command = match command {
        Command::Daemon(DaemonArguments {
            command: Some(DaemonCommand::Control(command)),
            ..
        }) => {
            if selected_version.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "daemon control does not select a project version",
                )
                .into());
            }
            return daemon_service_dispatch(command, &installation, variables, inline);
        }
        Command::Provider(arguments) => {
            return auth_dispatch(
                auth_cli::Command::Provider(arguments),
                &installation,
                variables,
                inline,
                non_interactive,
            );
        }
        Command::Projects(arguments) => {
            let keys = environment_keys()?;
            let config = ConfigSnapshot::new(
                metadata.brand(),
                [
                    HubResourcePolicy::distribution_defaults(metadata.brand())?,
                    ConfigStore::global(&installation)?.load()?,
                    ConfigSnapshot::capture_environment(
                        metadata.brand(),
                        &keys,
                        variables.iter().cloned(),
                    )?,
                    command_config(metadata.brand(), inline)?,
                ],
            )?;
            let policy = HubResourcePolicy::resolve(&config)?;
            let cancel = cancellation()?;
            let original = policy.request_context(Instant::now(), &cancel)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let prepared = graphit_app::foreground_project_control::client::prepare(arguments)?;
            if matches!(
                &prepared,
                graphit_app::foreground_project_control::ProjectCommand::MaintainCheckout { .. }
            ) {
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native_limits(&policy)?, policy.content()),
                    &original,
                    None,
                ))?;
            }
            let mut output = io::stdout().lock();
            return runtime
                .block_on(graphit_app::foreground_project_control::client::run(
                    prepared,
                    &installation,
                    &policy,
                    &original,
                    &mut output,
                ))
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
        }
        Command::Now(arguments) => {
            use std::io::IsTerminal as _;
            let execute = || -> Result<ExitCode, Box<dyn std::error::Error>> {
                let keys = environment_keys()?;
                let config = ConfigSnapshot::new(
                    installation.brand(),
                    [
                        HubResourcePolicy::distribution_defaults(installation.brand())?,
                        ConfigStore::global(&installation)?.load()?,
                        ConfigSnapshot::capture_environment(
                            installation.brand(),
                            &keys,
                            variables.iter().cloned(),
                        )?,
                        command_config(installation.brand(), inline)?,
                    ],
                )?;
                let policy = HubResourcePolicy::resolve(&config)?;
                let cancellation = cancellation()?;
                let original =
                    graphit_hub::policy::RequestContext::service(policy.content(), &cancellation)?;
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                let client = reqwest::Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()?;
                let startup = policy.request_context(Instant::now(), &cancellation)?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native_limits(&policy)?, policy.content()),
                    &startup,
                    None,
                ))?;
                let output = io::stdout();
                let terminal = output.is_terminal();
                let result = if terminal {
                    runtime.block_on(graphit_app::now_entry::run_terminal(
                        &installation,
                        &client,
                        arguments,
                        (native_limits(&policy)?, policy.content()),
                        &original,
                        &mut output.lock(),
                    ))
                } else {
                    runtime.block_on(graphit_app::now_entry::run_pipe(
                        &installation,
                        &client,
                        arguments,
                        (native_limits(&policy)?, policy.content()),
                        &original,
                        &output,
                    ))
                };
                if cancellation.is_cancelled() {
                    return Ok(ExitCode::from(130));
                }
                match result {
                    Ok(()) => Ok(ExitCode::SUCCESS),
                    Err(error) if error.broken_pipe() => Ok(ExitCode::SUCCESS),
                    Err(error) => {
                        eprintln!("{error}");
                        Ok(ExitCode::FAILURE)
                    }
                }
            };
            return match execute() {
                Ok(code) => Ok(code),
                Err(_) => {
                    eprintln!("Now configuration or installation unavailable");
                    Ok(ExitCode::FAILURE)
                }
            };
        }
        Command::Account(arguments) => {
            return auth_dispatch(
                auth_cli::Command::Account(arguments),
                &installation,
                variables,
                inline,
                non_interactive,
            );
        }
        Command::Login(arguments) => {
            return auth_dispatch(
                auth_cli::Command::Login(arguments),
                &installation,
                variables,
                inline,
                non_interactive,
            );
        }
        Command::Logout(arguments) => {
            return auth_dispatch(
                auth_cli::Command::Logout(arguments),
                &installation,
                variables,
                inline,
                non_interactive,
            );
        }
        Command::Refresh(arguments) => {
            return auth_dispatch(
                auth_cli::Command::Refresh(arguments),
                &installation,
                variables,
                inline,
                non_interactive,
            );
        }
        command => command,
    };
    if let Command::Mcp(arguments) = &mut command
        && (arguments.role
            || variables
                .iter()
                .any(|(key, _)| key.as_encoded_bytes().starts_with(b"GRAPHIT_ROLE_")))
    {
        if arguments.live_user || arguments.project_dir.is_some() || selected_version.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "managed-role MCP cannot override its captured transport or project",
            )
            .into());
        }
        if !["GRAPHIT_ROLE_BEARER", "GRAPHIT_ROLE_REFERENCE"]
            .into_iter()
            .all(|name| {
                variables
                    .iter()
                    .any(|(key, value)| key == std::ffi::OsStr::new(name) && !value.is_empty())
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "managed-role MCP requires its private credential pair",
            )
            .into());
        }
        arguments.role = true;
    }
    let root_mcp =
        matches!(&command, Command::Mcp(arguments) if !arguments.role && !arguments.live_user);
    let installation_selection = match &mut command {
        Command::Roles(arguments) => arguments.command.global(),
        Command::Sync(arguments) => arguments.global,
        Command::Skills(graphit_app::sync_entry::SkillsArguments {
            command: graphit_app::sync_entry::SkillsCommand::Sync(selection),
        }) => {
            selection.destination == graphit_app::managed_roles::InstallDestination::Global
                && selection.source_project.is_none()
        }
        Command::Hub(arguments) => arguments.command.global(),
        Command::Mcp(arguments) if arguments.live_user => true,
        Command::Ast(ast) => match &mut ast.command {
            AstCommand::Graph(graph) => graph
                .selection_mut()
                .is_none_or(|selection| selection.context.is_some()),
            AstCommand::Source(source) => source.context.is_some() && source.project_dir.is_none(),
            AstCommand::Export(arguments) => arguments.context.is_some(),
        },
        Command::Knowledge(KnowledgeArguments {
            command: KnowledgeCommand::Search(arguments),
        }) => arguments.artifact_id.is_some(),
        Command::Knowledge(KnowledgeArguments {
            command: KnowledgeCommand::Source(arguments),
        }) => arguments.artifact_id.is_some(),
        Command::Knowledge(KnowledgeArguments {
            command: KnowledgeCommand::Export(arguments),
        }) => arguments.artifact_id.is_some(),
        Command::Memory(MemoryArguments {
            command: MemoryCommand::Index(arguments),
        }) => arguments.scope == graphit_app::memory_index_entry::Scope::User,
        _ => false,
    };
    let default_root_mcp = matches!(
        &command,
        Command::Mcp(arguments) if root_mcp && arguments.project_dir.is_none()
    );
    let global_hook =
        matches!(&command, Command::SessionHook(arguments) if arguments.project_dir.is_none());
    let project = if installation_selection
        || global_hook
        || (default_root_mcp && cwd == installation.root())
    {
        None
    } else {
        match command_project(&mut command, &cwd, &installation) {
            Ok(project) => Some(project),
            Err(error) if default_root_mcp && error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        }
    };
    let Some(project) = project else {
        let version_variable = metadata.brand().environment_variable("PROJECT_VERSION")?;
        if (selected_version.is_some() && !global_hook)
            || (root_mcp
                && variables
                    .iter()
                    .any(|(key, _)| key == std::ffi::OsStr::new(&version_variable)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "project version requires a project",
            )
            .into());
        }
        let keys = environment_keys()?;
        let config = ConfigSnapshot::new(
            metadata.brand(),
            [
                HubResourcePolicy::distribution_defaults(metadata.brand())?,
                ConfigStore::global(&installation)?.load()?,
                ConfigSnapshot::capture_environment(
                    metadata.brand(),
                    &keys,
                    variables.iter().cloned(),
                )?,
                command_config(metadata.brand(), inline)?,
            ],
        )?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let cancel = cancellation()?;
        let original = policy.request_context(Instant::now(), &cancel)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let native = native_limits(&policy)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        match command {
            Command::Sync(arguments) => {
                let prepared = graphit_app::sync_entry::prepare(
                    arguments,
                    &config,
                    &installation,
                    None,
                    None,
                    &policy,
                    &original,
                )?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    None,
                ))?;
                return runtime
                    .block_on(graphit_app::sync_entry::run(
                        prepared,
                        &installation,
                        &policy,
                        native,
                        &original,
                        &mut io::stdout().lock(),
                    ))
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
            }
            Command::Skills(graphit_app::sync_entry::SkillsArguments {
                command: graphit_app::sync_entry::SkillsCommand::Sync(selection),
            }) => {
                let prepared = graphit_app::managed_roles::client::prepare_skills(
                    selection, None, None, &policy, &original,
                )?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    None,
                ))?;
                return runtime
                    .block_on(graphit_app::managed_roles::client::run(
                        prepared,
                        &installation,
                        &policy,
                        &original,
                        &mut io::stdout().lock(),
                    ))
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
            }
            Command::Roles(arguments) => {
                let prepared = graphit_app::managed_roles::client::prepare(
                    arguments, None, None, &policy, &original,
                )?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    None,
                ))?;
                return runtime
                    .block_on(graphit_app::managed_roles::client::run(
                        prepared,
                        &installation,
                        &policy,
                        &original,
                        &mut io::stdout().lock(),
                    ))
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
            }
            Command::SessionHook(arguments) => {
                return runtime.block_on(run_session_hook(
                    &installation,
                    &variables,
                    (native, &policy),
                    &original,
                    arguments,
                    selected_version.as_deref(),
                    &cwd,
                ));
            }
            Command::Hub(arguments) => {
                if selected_version.is_some()
                    && !matches!(
                        &arguments.command,
                        graphit_app::hub_publish::Command::InstallNative(_)
                    )
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "global FILE commands do not accept a project version",
                    )
                    .into());
                }
                let prepared = arguments.command.prepare(None, &policy, &original)?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    None,
                ))?;
                runtime.block_on(graphit_app::hub_publish::call(
                    &installation,
                    prepared,
                    native,
                    &original,
                    &mut io::stdout().lock(),
                ))?;
            }
            Command::Mcp(arguments) if arguments.live_user => {
                runtime.block_on(graphit_app::mcp_source::serve_live_user_stdio(
                    policy.content(),
                    std::time::Duration::from_millis(
                        policy.limit(ResourceSetting::RequestDeadlineMs).value,
                    ),
                    &cancel,
                ))?;
            }
            Command::Mcp(_) => {
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    None,
                ))?;
                runtime.block_on(async {
                    source_entry::stdio(
                        &installation,
                        &client,
                        (
                            native,
                            policy.content(),
                            std::time::Duration::from_millis(
                                policy.limit(ResourceSetting::RequestDeadlineMs).value,
                            ),
                        ),
                        None,
                        &original,
                        &cancel,
                        tokio::io::stdin(),
                        graphit_app::mcp_source::stdio_stdout(),
                    )
                    .await
                })?;
            }
            Command::Ast(ast) => {
                let mut output = io::stdout().lock();
                match ast.command {
                    AstCommand::Graph(graph) => {
                        original.check()?;
                        let prepared = graph.prepare(policy.content())?;
                        runtime.block_on(ensure_source(
                            &installation,
                            &variables,
                            (native, policy.content()),
                            &original,
                            None,
                        ))?;
                        runtime.block_on(graphit_app::graph_entry::run(
                            &installation,
                            &client,
                            prepared,
                            (native, policy.content()),
                            &original,
                            &mut output,
                        ))?;
                    }
                    AstCommand::Source(source) => {
                        runtime.block_on(ensure_source(
                            &installation,
                            &variables,
                            (native, policy.content()),
                            &original,
                            None,
                        ))?;
                        runtime.block_on(source_entry::call_source(
                            &installation,
                            &client,
                            &source,
                            native,
                            &original,
                            &mut output,
                        ))?;
                    }
                    AstCommand::Export(arguments) => {
                        let prepared =
                            graphit_app::ast_export_entry::prepare(&arguments, None, &original)?;
                        runtime.block_on(ensure_source(
                            &installation,
                            &variables,
                            (native, policy.content()),
                            &original,
                            None,
                        ))?;
                        runtime.block_on(graphit_app::ast_export_entry::call(
                            &installation,
                            prepared,
                            native,
                            &original,
                            &mut output,
                        ))?
                    }
                }
                output.flush()?;
            }
            Command::Memory(MemoryArguments {
                command: MemoryCommand::Index(arguments),
            }) => {
                let mut output = io::stdout().lock();
                let prepared = graphit_app::memory_index_entry::prepare(
                    &arguments,
                    selected_version.as_deref(),
                    &original,
                )?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    None,
                ))?;
                runtime.block_on(graphit_app::memory_index_entry::call(
                    &installation,
                    prepared,
                    native,
                    &original,
                    &mut output,
                ))?;
            }
            Command::Knowledge(KnowledgeArguments {
                command: KnowledgeCommand::Search(arguments),
            }) => {
                let mut output = io::stdout().lock();
                let prepared =
                    graphit_app::knowledge_search_entry::prepare(&arguments, None, &original)?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    None,
                ))?;
                runtime.block_on(graphit_app::knowledge_search_entry::call(
                    &installation,
                    prepared,
                    native,
                    &original,
                    &mut output,
                ))?;
            }
            Command::Knowledge(KnowledgeArguments {
                command: KnowledgeCommand::Source(arguments),
            }) => {
                let mut output = io::stdout().lock();
                let prepared =
                    graphit_app::knowledge_source_entry::prepare(&arguments, None, &original)?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    None,
                ))?;
                runtime.block_on(graphit_app::knowledge_source_entry::call(
                    &installation,
                    prepared,
                    native,
                    &original,
                    &mut output,
                ))?;
            }
            Command::Knowledge(KnowledgeArguments {
                command: KnowledgeCommand::Export(arguments),
            }) => {
                let mut output = io::stdout().lock();
                let prepared =
                    graphit_app::knowledge_export_entry::prepare(&arguments, None, &original)?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    None,
                ))?;
                runtime.block_on(graphit_app::knowledge_export_entry::call(
                    &installation,
                    prepared,
                    native,
                    &original,
                    &mut output,
                ))?;
            }
            _ => return Err(io::Error::from(io::ErrorKind::InvalidInput).into()),
        }
        return Ok(ExitCode::SUCCESS);
    };
    let keys = environment_keys()?;
    let config = ConfigSnapshot::new(
        metadata.brand(),
        [
            HubResourcePolicy::distribution_defaults(metadata.brand())?,
            ConfigStore::global(&installation)?.load()?,
            ConfigStore::project(&project)?.load()?,
            ConfigSnapshot::capture_environment(
                metadata.brand(),
                &keys,
                variables.iter().cloned(),
            )?,
            command_config(metadata.brand(), inline)?,
        ],
    )?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = cancellation()?;
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let version_variable = metadata.brand().environment_variable("PROJECT_VERSION")?;
    let environment_version = variables
        .iter()
        .find(|(key, _)| key == std::ffi::OsStr::new(&version_variable))
        .map(|(_, value)| {
            value
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))
        })
        .transpose()?;
    let current_version = selected_version.clone().or(environment_version);

    if current_version
        .as_ref()
        .is_some_and(|version| version.is_empty() || version.contains('\0'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "project version must be nonempty",
        )
        .into());
    }
    let mut builder = match &command {
        Command::Daemon(_) => tokio::runtime::Builder::new_multi_thread(),
        _ => tokio::runtime::Builder::new_current_thread(),
    };
    let runtime = builder.enable_all().build()?;
    let native = native_limits(&policy)?;
    match command {
        Command::Sync(arguments) => {
            let prepared = graphit_app::sync_entry::prepare(
                arguments,
                &config,
                &installation,
                Some(&project),
                current_version.clone(),
                &policy,
                &original,
            )?;
            runtime.block_on(ensure_source(
                &installation,
                &variables,
                (native, policy.content()),
                &original,
                Some(project.root()),
            ))?;
            runtime
                .block_on(graphit_app::sync_entry::run(
                    prepared,
                    &installation,
                    &policy,
                    native,
                    &original,
                    &mut io::stdout().lock(),
                ))
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        }
        Command::Skills(graphit_app::sync_entry::SkillsArguments {
            command: graphit_app::sync_entry::SkillsCommand::Sync(selection),
        }) => {
            let prepared = graphit_app::managed_roles::client::prepare_skills(
                selection,
                Some(&project),
                current_version.clone(),
                &policy,
                &original,
            )?;
            runtime.block_on(ensure_source(
                &installation,
                &variables,
                (native, policy.content()),
                &original,
                Some(project.root()),
            ))?;
            runtime
                .block_on(graphit_app::managed_roles::client::run(
                    prepared,
                    &installation,
                    &policy,
                    &original,
                    &mut io::stdout().lock(),
                ))
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        }
        Command::SessionHook(arguments) => runtime.block_on(run_session_hook(
            &installation,
            &variables,
            (native, &policy),
            &original,
            arguments,
            current_version.as_deref(),
            project.root(),
        )),
        Command::Hub(arguments) => {
            let version = current_version.as_deref();
            let prepared =
                arguments
                    .command
                    .prepare(Some((project.root(), version)), &policy, &original)?;
            runtime.block_on(ensure_source(
                &installation,
                &variables,
                (native, policy.content()),
                &original,
                Some(project.root()),
            ))?;
            runtime.block_on(graphit_app::hub_publish::call(
                &installation,
                prepared,
                native,
                &original,
                &mut io::stdout().lock(),
            ))?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Knowledge(arguments) => match arguments.command {
            KnowledgeCommand::Ask(mut arguments) => {
                if arguments.project_dir.is_none() {
                    arguments.project_dir = Some(
                        project
                            .root()
                            .to_str()
                            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?
                            .to_owned(),
                    );
                }
                let version = if arguments
                    .project_dir
                    .as_deref()
                    .is_some_and(|root| std::path::Path::new(root) == project.root())
                {
                    current_version.as_deref()
                } else {
                    selected_version.as_deref()
                };
                let mut output = io::stdout().lock();
                let prepared =
                    graphit_app::knowledge_ask_entry::prepare(&arguments, version, &original)?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    arguments.project_dir.as_deref().map(std::path::Path::new),
                ))?;
                runtime.block_on(graphit_app::knowledge_ask_entry::call(
                    &installation,
                    prepared,
                    native,
                    &original,
                    &mut output,
                ))?;
                Ok(ExitCode::SUCCESS)
            }
            KnowledgeCommand::Log(mut arguments) => {
                arguments.project_dir = Some(project.root().to_owned());
                let mut output = io::stdout().lock();
                let prepared = graphit_app::knowledge_log_entry::prepare(
                    &arguments,
                    current_version.as_deref(),
                    &original,
                )?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    arguments.project_dir.as_deref(),
                ))?;
                runtime.block_on(graphit_app::knowledge_search_entry::call(
                    &installation,
                    prepared,
                    native,
                    &original,
                    &mut output,
                ))?;
                Ok(ExitCode::SUCCESS)
            }
            KnowledgeCommand::Search(mut arguments) => {
                arguments.project_dir = Some(project.root().to_owned());
                let version = current_version.as_deref();
                let mut output = io::stdout().lock();
                let prepared =
                    graphit_app::knowledge_search_entry::prepare(&arguments, version, &original)?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    arguments.project_dir.as_deref(),
                ))?;
                runtime.block_on(graphit_app::knowledge_search_entry::call(
                    &installation,
                    prepared,
                    native,
                    &original,
                    &mut output,
                ))?;
                Ok(ExitCode::SUCCESS)
            }
            KnowledgeCommand::Source(mut arguments) => {
                arguments.project_dir = Some(project.root().to_owned());
                let version = current_version.as_deref();
                let mut output = io::stdout().lock();
                let prepared =
                    graphit_app::knowledge_source_entry::prepare(&arguments, version, &original)?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    arguments.project_dir.as_deref(),
                ))?;
                runtime.block_on(graphit_app::knowledge_source_entry::call(
                    &installation,
                    prepared,
                    native,
                    &original,
                    &mut output,
                ))?;
                Ok(ExitCode::SUCCESS)
            }
            KnowledgeCommand::Export(mut arguments) => {
                arguments.project_dir = Some(project.root().to_owned());
                let mut output = io::stdout().lock();
                let prepared = graphit_app::knowledge_export_entry::prepare(
                    &arguments,
                    current_version.as_deref(),
                    &original,
                )?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    arguments.project_dir.as_deref(),
                ))?;
                runtime.block_on(graphit_app::knowledge_export_entry::call(
                    &installation,
                    prepared,
                    native,
                    &original,
                    &mut output,
                ))?;
                Ok(ExitCode::SUCCESS)
            }
        },
        Command::Memory(MemoryArguments {
            command: MemoryCommand::Index(mut arguments),
        }) => {
            arguments.project_dir = Some(project.root().to_owned());
            let version = current_version.as_deref();
            let mut output = io::stdout().lock();
            let prepared =
                graphit_app::memory_index_entry::prepare(&arguments, version, &original)?;
            runtime.block_on(ensure_source(
                &installation,
                &variables,
                (native, policy.content()),
                &original,
                Some(project.root()),
            ))?;
            runtime.block_on(graphit_app::memory_index_entry::call(
                &installation,
                prepared,
                native,
                &original,
                &mut output,
            ))?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Roles(arguments) => {
            let prepared = graphit_app::managed_roles::client::prepare(
                arguments,
                Some(&project),
                current_version.clone(),
                &policy,
                &original,
            )?;
            runtime.block_on(ensure_source(
                &installation,
                &variables,
                (native, policy.content()),
                &original,
                Some(project.root()),
            ))?;
            let mut output = io::stdout().lock();
            runtime
                .block_on(graphit_app::managed_roles::client::run(
                    prepared,
                    &installation,
                    &policy,
                    &original,
                    &mut output,
                ))
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        }
        Command::Hooks(arguments) => {
            let prepared = graphit_app::project_hook_install::client::prepare(
                arguments,
                &project,
                current_version.clone(),
                &policy,
                &original,
            )?;
            runtime.block_on(ensure_source(
                &installation,
                &variables,
                (native, policy.content()),
                &original,
                Some(project.root()),
            ))?;
            let mut output = io::stdout().lock();
            runtime
                .block_on(graphit_app::project_hook_install::client::run(
                    prepared,
                    &installation,
                    &policy,
                    &original,
                    &mut output,
                ))
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        }
        Command::Coordinator(arguments) => {
            let prepared = graphit_app::project_coordinator_install::client::prepare(
                arguments,
                &project,
                current_version.clone(),
                &policy,
                &original,
            )?;
            runtime.block_on(ensure_source(
                &installation,
                &variables,
                (native, policy.content()),
                &original,
                Some(project.root()),
            ))?;
            let mut output = io::stdout().lock();
            runtime
                .block_on(graphit_app::project_coordinator_install::client::run(
                    prepared,
                    &installation,
                    &policy,
                    &original,
                    &mut output,
                ))
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        }
        Command::Git(arguments) => {
            use std::io::Seek;
            let configured = graphit_app::git_hook_commands::configured(&config)?;
            let hook = match &arguments.command {
                graphit_app::git_records::Command::Hook { event, arguments } => Some((
                    event.parse::<graphit_core::git_hooks::Event>()?,
                    arguments.clone(),
                )),
                _ => None,
            };
            let commands = hook
                .as_ref()
                .and_then(|(event, _)| configured.get(event))
                .map(Vec::as_slice)
                .unwrap_or_default();
            let records = graphit_app::git_records::requires_source(&arguments, &config)?;
            let mut input = match &hook {
                Some((event, _)) if records || !commands.is_empty() => {
                    runtime.block_on(graphit_app::git_hook_commands::capture_input(
                        *event,
                        &cwd,
                        &variables,
                        &policy,
                        &cancellation,
                    ))?
                }
                _ => None,
            };
            if records || hook.is_none() {
                // A local quality command may run longer than an RPC. Native
                // notifications get a fresh finite authenticated request.
                let request = policy.request_context(Instant::now(), &cancellation)?;
                if records {
                    runtime.block_on(ensure_source(
                        &installation,
                        &variables,
                        (native, policy.content()),
                        &request,
                        None,
                    ))?;
                }
                let mut empty = io::empty();
                let reader: &mut dyn io::Read = match input.as_mut() {
                    Some(file) => {
                        file.rewind()?;
                        file
                    }
                    None => &mut empty,
                };
                let mut stdout = io::stdout();
                let mut stderr = io::stderr();
                let output: &mut dyn io::Write = if hook.is_some() {
                    &mut stderr
                } else {
                    &mut stdout
                };
                let status = runtime.block_on(graphit_app::git_records::run(
                    arguments,
                    &project,
                    &installation,
                    &policy,
                    &request,
                    &config,
                    reader,
                    output,
                ))?;
                if status != ExitCode::SUCCESS {
                    return Ok(status);
                }
            }
            match hook {
                Some((event, arguments)) => runtime
                    .block_on(graphit_app::git_hook_commands::execute(
                        commands,
                        event,
                        &arguments,
                        &cwd,
                        &variables,
                        input.as_mut(),
                        &policy,
                        &cancellation,
                    ))
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error>),
                None => Ok(ExitCode::SUCCESS),
            }
        }
        Command::Task(mut arguments) => {
            let selection = arguments.command.selection_mut();
            if selection.project_dir.is_none() && selection.project_id.is_none() {
                selection.project_dir = Some(project.root().to_owned());
            }
            selection.project_version = if selection.project_dir.as_deref() == Some(project.root())
            {
                current_version
            } else {
                selected_version
            };
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()?;
            let mut output = io::stdout().lock();
            let prepared = runtime.block_on(graphit_app::task_entry::prepare(
                &installation,
                &arguments.command,
                policy.content(),
                &original,
            ))?;
            runtime.block_on(ensure_source(
                &installation,
                &variables,
                (native, policy.content()),
                &original,
                None,
            ))?;
            runtime.block_on(graphit_app::task_entry::run(
                &installation,
                &client,
                prepared,
                (native, policy.content()),
                &original,
                &mut output,
            ))?;
            output.flush()?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Ast(ast) => match ast.command {
            AstCommand::Graph(mut command) => {
                if let Some(selection) = command.selection_mut() {
                    if selection.context.is_some() {
                        if selected_version.is_some() {
                            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
                        }
                    } else {
                        if selection.project_dir.is_none() {
                            selection.project_dir = Some(project.root().to_owned());
                        }
                        selection.project_version =
                            if selection.project_dir.as_deref() == Some(project.root()) {
                                current_version
                            } else {
                                selected_version
                            };
                    }
                } else if selected_version.is_some() {
                    return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
                }
                let mut output = io::stdout().lock();
                original.check()?;
                let prepared = command.prepare(policy.content())?;
                let checkout = command
                    .selection_mut()
                    .and_then(|selection| selection.project_dir.as_deref());
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    checkout,
                ))?;
                runtime.block_on(graphit_app::graph_entry::run(
                    &installation,
                    &reqwest::Client::new(),
                    prepared,
                    (native, policy.content()),
                    &original,
                    &mut output,
                ))?;
                output.flush()?;
                Ok(ExitCode::SUCCESS)
            }
            AstCommand::Export(mut arguments) => {
                if arguments.project_dir.is_none() {
                    arguments.project_dir = Some(project.root().to_owned());
                }
                let version = if arguments.project_dir.as_deref() == Some(project.root()) {
                    current_version.as_deref()
                } else {
                    selected_version.as_deref()
                };
                let mut output = io::stdout().lock();
                let prepared =
                    graphit_app::ast_export_entry::prepare(&arguments, version, &original)?;
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    arguments.project_dir.as_deref(),
                ))?;
                runtime.block_on(graphit_app::ast_export_entry::call(
                    &installation,
                    prepared,
                    native,
                    &original,
                    &mut output,
                ))?;
                output.flush()?;
                Ok(ExitCode::SUCCESS)
            }
            AstCommand::Source(mut source) => {
                if source.context.is_some() {
                    source.project_version = selected_version;
                }
                if source.context.is_none() && source.project_dir.is_none() {
                    source.project_dir = Some(
                        project
                            .root()
                            .to_str()
                            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?
                            .into(),
                    );
                }
                if source.context.is_none()
                    && source
                        .project_dir
                        .as_deref()
                        .is_some_and(|p| std::path::Path::new(p) == project.root())
                {
                    source.project_version = current_version;
                }
                let mut output = io::stdout().lock();
                runtime.block_on(ensure_source(
                    &installation,
                    &variables,
                    (native, policy.content()),
                    &original,
                    source.project_dir.as_deref().map(std::path::Path::new),
                ))?;
                runtime.block_on(source_entry::call_source(
                    &installation,
                    &reqwest::Client::new(),
                    &source,
                    native,
                    &original,
                    &mut output,
                ))?;
                output.flush()?;
                Ok(ExitCode::SUCCESS)
            }
        },
        Command::Mcp(arguments) if arguments.role => {
            let captured_paths = installation.clone();
            let snapshot = original.snapshot().clone();
            let discovery = runtime
                .block_on(async move {
                    tokio::task::spawn_blocking(move || {
                        graphit_daemon::source_discovery::read_discovery(
                            &captured_paths,
                            native,
                            &snapshot,
                        )
                    })
                    .await
                })?
                .map_err(io::Error::other)?
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "Role MCP requires the active daemon",
                    )
                })?;
            runtime.block_on(async {
                let proxy = graphit_app::role_proxy::RoleProxy::from_environment(
                    discovery.address(),
                    reqwest::Client::new(),
                    policy.content(),
                    policy.projection(),
                    std::time::Duration::from_millis(
                        policy.limit(ResourceSetting::RequestDeadlineMs).value,
                    ),
                    &original,
                )
                .await?;
                let serve =
                    proxy.serve_stream(tokio::io::stdin(), graphit_app::mcp_source::stdio_stdout());
                tokio::pin!(serve);
                tokio::select! {
                    result = &mut serve => result,
                    () = cancellation.cancelled() => { proxy.shutdown().await?; serve.await }
                }
            })?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Mcp(_) => {
            runtime.block_on(ensure_source(
                &installation,
                &variables,
                (native, policy.content()),
                &original,
                None,
            ))?;
            let current = Some((project.root(), current_version.as_deref()));
            runtime.block_on(async {
                source_entry::stdio(
                    &installation,
                    &reqwest::Client::new(),
                    (
                        native,
                        policy.content(),
                        std::time::Duration::from_millis(
                            policy.limit(ResourceSetting::RequestDeadlineMs).value,
                        ),
                    ),
                    current,
                    &original,
                    &cancellation,
                    tokio::io::stdin(),
                    graphit_app::mcp_source::stdio_stdout(),
                )
                .await
            })?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Daemon(_) => {
            let inventory = GlobalProjectInventory::at(
                &installation,
                usize::try_from(policy.limit(ResourceSetting::ContentInputBytes).value)?,
            )?
            .load(original.snapshot())?;
            let resolved_current = if let Some(version) = current_version.as_deref() {
                version.to_owned()
            } else {
                let checkout = graphit_core::git_checkout::GitCheckout::inspect(
                    project.root(),
                    original
                        .snapshot()
                        .required_deadline()?
                        .saturating_duration_since(std::time::Instant::now()),
                    &cancellation,
                )?;
                original.snapshot().check()?;
                graphit_core::version_context::VersionContext::select(
                    &project,
                    checkout.as_checkout(),
                    "",
                    None,
                    "",
                )?
                .version()
                .to_owned()
            };
            let selections = daemon_selections(
                &installation,
                Some((&project, &resolved_current)),
                &inventory,
            )?;
            run_foreground_daemon(
                &installation,
                &config,
                selections,
                variables,
                &original,
                &cancellation,
                &runtime,
            )
            .map(|exit| {
                if cancellation.is_cancelled() {
                    ExitCode::from(130)
                } else {
                    exit
                }
            })
        }
        _ => unreachable!("package/models dispatched before async commands"),
    }
}

fn environment_keys() -> io::Result<Vec<ConfigKey>> {
    RESOURCE_LIMITS
        .iter()
        .map(|spec| ConfigKey::new(spec.key))
        .chain(graphit_core::module_settings::Module::ALL.map(|module| Ok(module.key())))
        .chain(
            [
                "agent",
                "ai.cli",
                "mcp.host",
                "mcp.port",
                graphit_data::LocalGraphPolicy::CONFIG_KEY,
                "task.browser",
                "ast.scip.parsers",
                "ast.scip.version",
                graphit_app::git_hook_commands::CONFIG_KEY,
            ]
            .map(ConfigKey::new),
        )
        .collect()
}

fn install_dispatch(
    arguments: package::InstallArguments,
    installation: &InstallationPaths,
    cwd: &std::path::Path,
    variables: Vec<(OsString, OsString)>,
    inline: Vec<(ConfigKey, serde_json::Value)>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    use graphit_app::{
        native_install::{FailureCategory, InstallRequest},
        native_install_client, native_install_entry,
        native_install_http::NativeInstallPolicy,
    };
    let keys = RESOURCE_LIMITS
        .iter()
        .map(|spec| ConfigKey::new(spec.key))
        .chain(std::iter::once(ConfigKey::new(
            graphit_data::LocalGraphPolicy::CONFIG_KEY,
        )))
        .collect::<io::Result<Vec<_>>>()?;
    let config = ConfigSnapshot::new(
        installation.brand(),
        [
            HubResourcePolicy::distribution_defaults(installation.brand())?,
            ConfigStore::global(installation)?.load()?,
            ConfigSnapshot::capture_environment(installation.brand(), &keys, variables)?,
            command_config(installation.brand(), inline)?,
        ],
    )?;
    let hub = HubResourcePolicy::resolve(&config)?;
    let invalid = (|| -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        graphit_hub::ArtifactId::new(&arguments.id, hub.projection())?;
        graphit_hub::ConcreteVersion::new(&arguments.version, hub.projection())?;
        let absolute = if arguments.input.is_absolute() {
            arguments.input.clone()
        } else {
            cwd.join(&arguments.input)
        };
        let mut normal = PathBuf::new();
        for component in absolute.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    if !normal.pop() {
                        return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
                    }
                }
                component => normal.push(component.as_os_str()),
            }
        }
        if !normal.is_absolute() || normal.file_name().is_none() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        graphit_core::native_path::NativePath::new(normal.clone())?;
        Ok(normal)
    })();
    let input = match invalid {
        Ok(input) => input,
        Err(error) => {
            diagnostic("package install arguments", error.as_ref());
            return Ok(ExitCode::from(2));
        }
    };
    let request = InstallRequest {
        input,
        producer: arguments.producer,
        id: arguments.id,
        version: arguments.version,
        kind: arguments.kind,
    };
    let cancel = cancellation()?;
    let original = hub.request_context(Instant::now(), &cancel)?;
    let policy = NativeInstallPolicy::capture(&config, original.snapshot())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let active = match runtime.block_on(native_install_client::execute_if_active(
        installation,
        &request,
        &policy.hub,
        &original,
    )) {
        Ok(active) => active,
        Err(error) => {
            diagnostic("package install client", &error);
            return Ok(if cancel.is_cancelled() {
                ExitCode::from(130)
            } else {
                ExitCode::FAILURE
            });
        }
    };
    let outcome = match active {
        Some(outcome) => outcome,
        None => match runtime.block_on(native_install_entry::execute(
            installation,
            &request,
            &policy,
            &original,
        )) {
            Ok(execution) => {
                if let Some(cause) = &execution.cause {
                    diagnostic("package install", cause);
                }
                execution.outcome
            }
            Err(mut failure) => {
                diagnostic("package install offline", &failure);
                if let Some(execution) = &failure.execution
                    && let Err(error) = native_install_entry::write_outcome(
                        &execution.outcome,
                        hub.content().max_output_bytes,
                        &original,
                        &mut io::stdout().lock(),
                    )
                {
                    diagnostic("package install delivery", &error);
                }
                if let Some(error) = failure.cleanup_failure() {
                    diagnostic("package install cleanup", error);
                }
                if failure.has_resources()
                    && let Err(error) = failure.retry_cleanup(&hub)
                {
                    diagnostic("package install cleanup", error);
                }
                return Ok(if cancel.is_cancelled() {
                    ExitCode::from(130)
                } else {
                    ExitCode::FAILURE
                });
            }
        },
    };
    if let Err(error) = native_install_entry::write_outcome(
        &outcome,
        hub.content().max_output_bytes,
        &original,
        &mut io::stdout().lock(),
    ) {
        diagnostic("package install delivery", &error);
        return Ok(if cancel.is_cancelled() {
            ExitCode::from(130)
        } else {
            ExitCode::FAILURE
        });
    }
    Ok(if outcome.succeeded() {
        ExitCode::SUCCESS
    } else if outcome
        .failure
        .as_ref()
        .is_some_and(|e| e.category == FailureCategory::Cancelled)
    {
        ExitCode::from(130)
    } else {
        ExitCode::FAILURE
    })
}

fn auth_dispatch(
    command: auth_cli::Command,
    installation: &InstallationPaths,
    variables: Vec<(OsString, OsString)>,
    inline: Vec<(ConfigKey, serde_json::Value)>,
    non_interactive: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let keys = RESOURCE_LIMITS
        .iter()
        .map(|spec| ConfigKey::new(spec.key))
        .collect::<io::Result<Vec<_>>>()?;
    let config = ConfigSnapshot::new(
        installation.brand(),
        [
            HubResourcePolicy::distribution_defaults(installation.brand())?,
            ConfigStore::global(installation)?.load()?,
            ConfigSnapshot::capture_environment(installation.brand(), &keys, variables)?,
            command_config(installation.brand(), inline)?,
        ],
    )?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = cancellation()?;
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut announce = |url: &str| {
        writeln!(io::stderr().lock(), "Authorize native Broker login: {url}")?;
        if !non_interactive {
            open::that_detached(url)?;
        }
        Ok(())
    };
    let active = runtime.block_on(auth_active::run_if_active(
        &command,
        installation,
        &client,
        native_limits(&policy)?,
        policy.content().max_output_bytes,
        &original,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
        &mut announce,
    ))?;
    if let Some(code) = active {
        return Ok(code);
    }
    match auth_entry::run(
        command,
        installation,
        &policy,
        &original,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
        &mut announce,
    ) {
        Ok(code) => Ok(code),
        Err(failure) => {
            let interrupted = failure.cause().kind() == io::ErrorKind::Interrupted;
            diagnostic("authentication", &failure);
            if failure.has_resources()
                && let Err(cleanup) = failure.cleanup(&policy)
            {
                diagnostic("authentication cleanup", &cleanup);
            }
            Ok(if interrupted {
                ExitCode::from(130)
            } else {
                ExitCode::FAILURE
            })
        }
    }
}

fn setup_dispatch(
    arguments: crate::setup::Arguments,
    installation: &InstallationPaths,
    variables: Vec<(OsString, OsString)>,
    inline: Vec<(ConfigKey, serde_json::Value)>,
    selected_version: Option<String>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if variables
        .iter()
        .any(|(key, _)| key.as_encoded_bytes().starts_with(b"GRAPHIT_ROLE_"))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "installation setup is unavailable in a managed-role transport context",
        )
        .into());
    }
    if selected_version.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "setup maintains the actual checkout; use projects maintain-checkout for an explicit version",
        )
        .into());
    }
    let config = daemon_global_config(installation, &variables, inline)?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancel = cancellation()?;
    let original = policy.request_context(Instant::now(), &cancel)?;
    let configured = crate::setup::configure(arguments, installation, original.snapshot())?;
    let mut git_records = None;
    if let Some((project, _)) = &configured.project {
        {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(ensure_source(
                installation,
                &variables,
                (native_limits(&policy)?, policy.content()),
                &original,
                Some(project.root()),
            ))?;
            let mut result = Vec::new();
            let project_config = ConfigSnapshot::new(
                config.brand(),
                [ConfigStore::project(project)?.load()?].into_iter().chain(
                    [
                        ConfigSource::Command,
                        ConfigSource::Environment,
                        ConfigSource::Global,
                        ConfigSource::Distribution,
                    ]
                    .into_iter()
                    .filter_map(|source| config.layer(source).cloned()),
                ),
            )?;
            let status = runtime.block_on(graphit_app::git_records::run(
                graphit_app::git_records::Arguments {
                    command: graphit_app::git_records::Command::Install,
                },
                project,
                installation,
                &policy,
                &original,
                &project_config,
                &mut io::empty(),
                &mut result,
            ))?;
            if status != ExitCode::SUCCESS {
                io::stdout().lock().write_all(&result)?;
                return Ok(status);
            }
            git_records = Some(serde_json::from_slice::<serde_json::Value>(&result)?);
        }
    }
    let mut output = io::stdout().lock();
    serde_json::to_writer(
        &mut output,
        &serde_json::json!({
            "installation": installation.root(),
            "agent": configured.agent.as_str(),
            "git_records": git_records,
            "project": configured.project.as_ref().map(|(paths, identity)| serde_json::json!({
                "directory": paths.root(), "identity": identity,
            })),
        }),
    )?;
    writeln!(output)?;
    output.flush()?;
    Ok(ExitCode::SUCCESS)
}

fn ui_dispatch(
    arguments: graphit_app::ui_entry::Arguments,
    installation: &InstallationPaths,
    variables: &[(OsString, OsString)],
    inline: Vec<(ConfigKey, serde_json::Value)>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let config = daemon_global_config(installation, variables, inline)?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = cancellation()?;
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let url = runtime.block_on(ui_request(
        installation,
        variables,
        &policy,
        arguments,
        &original,
    ))?;
    writeln!(io::stdout().lock(), "{url}")?;
    Ok(ExitCode::SUCCESS)
}

struct DesktopInvocationFailure {
    result: Result<String, Box<dyn std::error::Error>>,
    shutdown: graphit_process::HostShutdown,
    interrupted: bool,
}
impl std::fmt::Debug for DesktopInvocationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
impl std::fmt::Display for DesktopInvocationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.result {
            Ok(outcome) => write!(f, "{outcome}; ")?,
            Err(error) => write!(f, "{error}; ")?,
        }
        match &self.shutdown {
            graphit_process::HostShutdown::Joined(report) => match report.failure() {
                Some(error) => write!(f, "ProcessHost cleanup failed: {error:?}"),
                None => write!(f, "ProcessHost joined"),
            },
            graphit_process::HostShutdown::Pending(_) => write!(f, "ProcessHost cleanup pending"),
        }
    }
}
impl std::error::Error for DesktopInvocationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.result.as_ref().err().map(|error| error.as_ref())
    }
}

async fn ui_request(
    installation: &InstallationPaths,
    variables: &[(OsString, OsString)],
    policy: &HubResourcePolicy,
    arguments: graphit_app::ui_entry::Arguments,
    original: &graphit_hub::policy::RequestContext<'_>,
) -> Result<String, Box<dyn std::error::Error>> {
    use graphit_app::{daemon_launcher, ui_entry};
    use graphit_process::{
        HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, ProcessHost,
    };
    use std::time::Duration;
    let deadline = original.snapshot().required_deadline()?;
    let executable = std::env::current_exe()?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let termination_grace = Duration::from_secs(1);
    let cleanup = Duration::from_secs(3);
    original.check()?;
    let service_stop = Cancellation::default();
    let host = ProcessHost::install(&service_stop, HostLimits::new(1, 1)?)?;
    let result = async {
        let ready = tokio::select! {
            biased;
            () = original.cancellation().cancelled() => return Err(io::Error::from(io::ErrorKind::Interrupted).into()),
            ready = host.ready(deadline) => ready,
        };
        original.check()?;
        match ready {
            HostReadiness::Ready => (),
            HostReadiness::Pending => return Err(io::Error::from(io::ErrorKind::TimedOut).into()),
            HostReadiness::Sealed => return Err(io::Error::from(io::ErrorKind::BrokenPipe).into()),
            HostReadiness::Failed(error) => return Err(io::Error::other(format!("UI ProcessHost readiness: {error:?}")).into()),
        }
        ui_entry::execute(
            &host,
            daemon_launcher::Inputs {
                installation, executable: &executable, environment: variables, client: &client,
                limits: native_limits(policy)?, termination_grace, cleanup,
            },
            original, arguments,
        ).await.map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
    }.await;
    service_stop.cancel();
    let shutdown = match host.shutdown(Instant::now() + cleanup).await {
        HostShutdown::Pending(mut pending) => {
            match pending.repair(Instant::now() + cleanup).await {
                HostShutdownObservation::Joined(report) => HostShutdown::Joined(report),
                HostShutdownObservation::Pending => HostShutdown::Pending(pending),
            }
        }
        joined => joined,
    };
    match (result, shutdown) {
        (Ok(outcome), HostShutdown::Joined(report)) if report.failure().is_none() => Ok(outcome),
        (result, shutdown) => Err(Box::new(DesktopInvocationFailure {
            result,
            shutdown,
            interrupted: original.cancellation().is_cancelled(),
        })),
    }
}

#[cfg(target_os = "linux")]
fn tray_dispatch(
    installation: &InstallationPaths,
    variables: &[(OsString, OsString)],
    inline: Vec<(ConfigKey, serde_json::Value)>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    use graphit_process::{
        HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, ProcessHost,
    };
    use std::time::Duration;
    let config = daemon_global_config(installation, variables, inline)?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = cancellation()?;
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let executable = std::env::current_exe()?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let cleanup = Duration::from_secs(3);
        let service_stop = Cancellation::default();
        let host = ProcessHost::install(&service_stop, HostLimits::new(1, 1)?)?;
        let result: Result<String, Box<dyn std::error::Error>> = async {
            match host.ready(original.snapshot().required_deadline()?).await {
                HostReadiness::Ready => (),
                HostReadiness::Pending => {
                    return Err(io::Error::from(io::ErrorKind::TimedOut).into());
                }
                HostReadiness::Sealed => {
                    return Err(io::Error::from(io::ErrorKind::BrokenPipe).into());
                }
                HostReadiness::Failed(error) => {
                    return Err(
                        io::Error::other(format!("tray ProcessHost readiness: {error:?}")).into(),
                    );
                }
            }
            original.check()?;
            graphit_app::tray_entry::execute(
                &host,
                graphit_app::daemon_launcher::Inputs {
                    installation,
                    executable: &executable,
                    environment: variables,
                    client: &client,
                    limits: native_limits(&policy)?,
                    termination_grace: Duration::from_secs(1),
                    cleanup,
                },
                &policy,
                &cancellation,
            )
            .await?;
            Ok("tray closed".into())
        }
        .await;
        service_stop.cancel();
        let shutdown = match host.shutdown(Instant::now() + cleanup).await {
            HostShutdown::Pending(mut pending) => {
                match pending.repair(Instant::now() + cleanup).await {
                    HostShutdownObservation::Joined(report) => HostShutdown::Joined(report),
                    HostShutdownObservation::Pending => HostShutdown::Pending(pending),
                }
            }
            joined => joined,
        };
        match (result, shutdown) {
            (Ok(_), HostShutdown::Joined(report)) if report.failure().is_none() => {
                Ok(if cancellation.is_cancelled() {
                    ExitCode::from(130)
                } else {
                    ExitCode::SUCCESS
                })
            }
            (result, shutdown) => Err(Box::new(DesktopInvocationFailure {
                result,
                shutdown,
                interrupted: cancellation.is_cancelled(),
            }) as Box<dyn std::error::Error>),
        }
    })
}

fn self_update_dispatch(
    installation: &InstallationPaths,
    variables: &[(OsString, OsString)],
    inline: Vec<(ConfigKey, serde_json::Value)>,
    current_version: &str,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    use graphit_app::self_update::Outcome;
    let config = daemon_global_config(installation, variables, inline)?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = cancellation()?;
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let outcome = runtime.block_on(self_update_request(
        installation,
        variables,
        &policy,
        current_version,
        &original,
    ))?;
    let mut output = io::stdout().lock();
    match outcome {
        Outcome::Development => writeln!(output, "Development build; self-update is unavailable.")?,
        Outcome::Current => writeln!(output, "The installed release is current.")?,
        Outcome::Installed { tag, daemon } => {
            writeln!(output, "Installed {tag}; daemon={daemon:?}.")?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

struct SelfUpdateInvocationFailure {
    result: Result<graphit_app::self_update::Outcome, Box<dyn std::error::Error>>,
    shutdown: graphit_process::HostShutdown,
    interrupted: bool,
}
impl std::fmt::Debug for SelfUpdateInvocationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
impl std::fmt::Display for SelfUpdateInvocationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.result {
            Ok(outcome) => write!(f, "self-update {outcome:?}; ")?,
            Err(error) => write!(f, "{error}; ")?,
        }
        match &self.shutdown {
            graphit_process::HostShutdown::Joined(report) => match report.failure() {
                Some(error) => write!(f, "ProcessHost cleanup failed: {error:?}"),
                None => write!(f, "ProcessHost joined"),
            },
            graphit_process::HostShutdown::Pending(_) => write!(f, "ProcessHost cleanup pending"),
        }
    }
}
impl std::error::Error for SelfUpdateInvocationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.result.as_ref().err().map(|error| error.as_ref())
    }
}

async fn self_update_request(
    installation: &InstallationPaths,
    variables: &[(OsString, OsString)],
    policy: &HubResourcePolicy,
    current_version: &str,
    original: &graphit_hub::policy::RequestContext<'_>,
) -> Result<graphit_app::self_update::Outcome, Box<dyn std::error::Error>> {
    use graphit_app::{daemon_launcher, self_update, self_update_release};
    use graphit_process::{
        HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, ProcessHost,
    };
    use std::time::Duration;
    let deadline = original.snapshot().required_deadline()?;
    let executable = std::env::current_exe()?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let bytes = policy.copy().total_file_bytes_limit();
    let limits = self_update_release::Limits {
        http: graphit_http::client::Limits {
            request_body_bytes: policy.content().max_input_bytes,
            response_body_bytes: usize::try_from(bytes)?,
            url_bytes: 8192,
            header_bytes: 65536,
            header_count: 32,
            max_in_flight: 1,
            max_idle_per_host: 1,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 8,
            max_certificate_bytes: 65536,
        },
        tree: policy.copy(),
        metadata_bytes: policy.content().max_input_bytes,
        decoded_archive_bytes: bytes,
        redirects: 5,
    };
    let termination_grace = Duration::from_secs(1);
    let cleanup = Duration::from_secs(3);
    original.check()?;
    let service_stop = Cancellation::default();
    let host = ProcessHost::install(&service_stop, HostLimits::new(1, 1)?)?;
    let result = async {
        let ready = tokio::select! {
            biased;
            () = original.cancellation().cancelled() => return Err(io::Error::from(io::ErrorKind::Interrupted).into()),
            ready = host.ready(deadline) => ready,
        };
        original.check()?;
        match ready {
            HostReadiness::Ready => (),
            HostReadiness::Pending => return Err(io::Error::from(io::ErrorKind::TimedOut).into()),
            HostReadiness::Sealed => return Err(io::Error::from(io::ErrorKind::BrokenPipe).into()),
            HostReadiness::Failed(error) => return Err(io::Error::other(format!("self-update ProcessHost readiness: {error:?}")).into()),
        }
        self_update::execute(
            &host,
            self_update::Inputs {
                launcher: daemon_launcher::Inputs {
                    installation, executable: &executable, environment: variables, client: &client,
                    limits: native_limits(policy)?, termination_grace, cleanup,
                },
                current_version,
                limits,
            },
            original,
        ).await.map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
    }.await;
    service_stop.cancel();
    let shutdown = match host.shutdown(Instant::now() + cleanup).await {
        HostShutdown::Pending(mut pending) => {
            match pending.repair(Instant::now() + cleanup).await {
                HostShutdownObservation::Joined(report) => HostShutdown::Joined(report),
                HostShutdownObservation::Pending => HostShutdown::Pending(pending),
            }
        }
        joined => joined,
    };
    match (result, shutdown) {
        (Ok(outcome), HostShutdown::Joined(report)) if report.failure().is_none() => Ok(outcome),
        (result, shutdown) => Err(Box::new(SelfUpdateInvocationFailure {
            result,
            shutdown,
            interrupted: original.cancellation().is_cancelled(),
        })),
    }
}

fn daemon_global_config(
    installation: &InstallationPaths,
    variables: &[(OsString, OsString)],
    inline: Vec<(ConfigKey, serde_json::Value)>,
) -> io::Result<ConfigSnapshot> {
    ConfigSnapshot::new(
        installation.brand(),
        [
            ConfigStore::global(installation)?.load()?,
            ConfigSnapshot::capture_environment(
                installation.brand(),
                &environment_keys()?,
                variables.iter().cloned(),
            )?,
            command_config(installation.brand(), inline)?,
        ],
    )
}

// Only an unsuccessful invocation retains this receipt. It owns actual pending
// SDK cleanup together with the operation's cause or already observed effect.
struct DaemonInvocationFailure {
    result: Result<graphit_app::daemon_launcher::Outcome, Box<dyn std::error::Error>>,
    shutdown: graphit_process::HostShutdown,
    interrupted: bool,
}
impl std::fmt::Debug for DaemonInvocationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
impl std::fmt::Display for DaemonInvocationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use graphit_app::daemon_launcher::Outcome;
        match &self.result {
            Ok(Outcome::Running(reply)) => {
                write!(f, "daemon {:?} pid={}; ", reply.state, reply.pid)?
            }
            Ok(Outcome::NativeStatus(outcome)) => {
                write!(f, "daemon manager status {}; ", outcome.status)?
            }
            Ok(Outcome::Stopped) => write!(f, "daemon stopped; ")?,
            Ok(Outcome::Absent) => write!(f, "daemon absent; ")?,
            Ok(Outcome::Completed) => write!(f, "daemon command completed; ")?,
            Err(_) => write!(f, "daemon command failed; ")?,
        }
        match &self.shutdown {
            graphit_process::HostShutdown::Joined(report) => match report.failure() {
                Some(error) => write!(f, "ProcessHost cleanup failed: {error:?}"),
                None => write!(f, "ProcessHost joined"),
            },
            graphit_process::HostShutdown::Pending(_) => write!(f, "ProcessHost cleanup pending"),
        }
    }
}
impl std::error::Error for DaemonInvocationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.result.as_ref().err().map(|error| error.as_ref())
    }
}

async fn daemon_service_request(
    installation: &InstallationPaths,
    variables: &[(OsString, OsString)],
    limits: NativeDocumentFileLimits,
    original: &graphit_hub::policy::RequestContext<'_>,
    command: &ServiceCommand,
) -> Result<graphit_app::daemon_launcher::Outcome, Box<dyn std::error::Error>> {
    use graphit_app::daemon_launcher::{Inputs, Outcome};
    use graphit_process::{
        HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, ProcessHost,
    };
    use std::time::Duration;
    let deadline = original.snapshot().required_deadline()?;
    let executable = std::env::current_exe()?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let termination_grace = Duration::from_secs(1);
    let cleanup = Duration::from_secs(3);
    original.check()?;
    let service_stop = Cancellation::default();
    let host = ProcessHost::install(&service_stop, HostLimits::new(1, 1)?)?;
    let result: Result<Outcome, Box<dyn std::error::Error>> = async {
        let ready = tokio::select! {
            biased;
            () = original.cancellation().cancelled() => return Err(io::Error::from(io::ErrorKind::Interrupted).into()),
            ready = host.ready(deadline) => ready,
        };
        original.check()?;
        match ready {
            HostReadiness::Ready => (),
            HostReadiness::Pending => return Err(io::Error::from(io::ErrorKind::TimedOut).into()),
            HostReadiness::Sealed => return Err(io::Error::from(io::ErrorKind::BrokenPipe).into()),
            HostReadiness::Failed(error) => return Err(io::Error::other(format!("daemon ProcessHost readiness: {error:?}")).into()),
        }
        graphit_app::daemon_launcher::execute(
            &host, Inputs { installation, executable: &executable, environment: variables, client: &client, limits, termination_grace, cleanup }, original, command,
        ).await.map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
    }.await;
    service_stop.cancel();
    let shutdown = match host.shutdown(Instant::now() + cleanup).await {
        HostShutdown::Pending(mut pending) => {
            match pending.repair(Instant::now() + cleanup).await {
                HostShutdownObservation::Joined(report) => HostShutdown::Joined(report),
                HostShutdownObservation::Pending => HostShutdown::Pending(pending),
            }
        }
        joined => joined,
    };
    match (result, shutdown) {
        (Ok(outcome), HostShutdown::Joined(report)) if report.failure().is_none() => Ok(outcome),
        (result, shutdown) => Err(Box::new(DaemonInvocationFailure {
            result,
            shutdown,
            interrupted: original.cancellation().is_cancelled(),
        })),
    }
}

async fn run_session_hook(
    installation: &InstallationPaths,
    variables: &[(OsString, OsString)],
    (native, policy): (NativeDocumentFileLimits, &HubResourcePolicy),
    original: &graphit_hub::policy::RequestContext<'_>,
    mut arguments: graphit_app::session_hook_entry::Arguments,
    version: Option<&str>,
    cwd: &std::path::Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if let Some(selected) = arguments.project_dir.as_deref()
        && !selected.is_absolute()
    {
        original.check()?;
        arguments.project_dir = Some(
            ProjectPaths::discover(selected, installation)?
                .root()
                .to_owned(),
        );
    }
    graphit_app::session_hook_entry::read_host_stdin(&mut arguments, original).await?;
    let project = ProjectPaths::discover(
        arguments.project_dir.as_deref().unwrap_or(cwd),
        installation,
    )?;
    arguments.project_dir = Some(project.root().to_owned());
    let prepared =
        graphit_app::session_hook_entry::prepare(installation, &arguments, version, original)?;
    if arguments.recipient == graphit_app::session_hook_entry::Recipient::Coordinator {
        ensure_source(
            installation,
            variables,
            (native, policy.content()),
            original,
            Some(project.root()),
        )
        .await?;
    }
    graphit_app::session_hook_entry::call(
        installation,
        arguments.recipient,
        prepared,
        native,
        policy,
        original,
        &mut io::stdout().lock(),
    )
    .await?;
    Ok(ExitCode::SUCCESS)
}

async fn ensure_source(
    installation: &InstallationPaths,
    variables: &[(OsString, OsString)],
    limits: (NativeDocumentFileLimits, graphit_hub::native::ContentLimits),
    original: &graphit_hub::policy::RequestContext<'_>,
    checkout: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Keep the service bootstrap future off every synchronous dispatch frame.
    // The original request still owns its cancellation and absolute deadline.
    Box::pin(async {
        if variables
            .iter()
            .any(|(key, _)| key.as_encoded_bytes().starts_with(b"GRAPHIT_ROLE_"))
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Root Source startup is unavailable in a managed-role transport context",
            )
            .into());
        }
        original.check()?;
        // Resolve the physical root before service effects. Requested read versions
        // belong to the RPC; automatic maintenance observes the actual checkout.
        let checkout = checkout
            .map(|root| ProjectPaths::at(root, installation))
            .transpose()?;
        match daemon_service_request(
            installation,
            variables,
            limits.0,
            original,
            &ServiceCommand::Start,
        )
        .await?
        {
            graphit_app::daemon_launcher::Outcome::Running(_) => (),
            _ => {
                return Err(
                    io::Error::other("daemon Start did not acknowledge a running service").into(),
                );
            }
        }
        if let Some(project) = checkout {
            let address = source_entry::address(installation, limits.0, original).await?;
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()?;
            source_entry::maintain_checkout(
                installation,
                address,
                &client,
                &project,
                limits,
                original,
            )
            .await?;
        }
        Ok(())
    })
    .await
}

fn daemon_service_dispatch(
    command: ServiceCommand,
    installation: &InstallationPaths,
    variables: Vec<(OsString, OsString)>,
    inline: Vec<(ConfigKey, serde_json::Value)>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    use graphit_app::daemon_launcher::Outcome;
    let config = daemon_global_config(installation, &variables, inline)?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = cancellation()?;
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let outcome = runtime.block_on(daemon_service_request(
        installation,
        &variables,
        native_limits(&policy)?,
        &original,
        &command,
    ))?;
    let exit = match outcome {
        Outcome::Completed => ExitCode::SUCCESS,
        Outcome::Running(reply) => {
            let mut output = io::stdout().lock();
            serde_json::to_writer(&mut output, &reply)?;
            writeln!(output)?;
            ExitCode::SUCCESS
        }
        Outcome::Stopped => {
            writeln!(io::stdout().lock(), "{{\"state\":\"stopped\"}}")?;
            ExitCode::SUCCESS
        }
        Outcome::Absent => {
            writeln!(io::stdout().lock(), "{{\"state\":\"absent\"}}")?;
            ExitCode::FAILURE
        }
        Outcome::NativeStatus(outcome) => {
            if let Some(transcript) = &outcome.transcript {
                io::stdout().lock().write_all(transcript.stdout())?;
                io::stderr().lock().write_all(transcript.stderr())?;
            }
            outcome
                .status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .map_or(ExitCode::FAILURE, ExitCode::from)
        }
    };
    Ok(if cancellation.is_cancelled() {
        ExitCode::from(130)
    } else {
        exit
    })
}

fn managed_daemon_dispatch(
    installation: &InstallationPaths,
    variables: Vec<(OsString, OsString)>,
    inline: Vec<(ConfigKey, serde_json::Value)>,
    detached: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if detached && !cfg!(target_os = "linux") {
        return Err(
            io::Error::new(io::ErrorKind::Unsupported, "detached daemon requires Linux").into(),
        );
    }
    let config = daemon_global_config(installation, &variables, inline)?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = cancellation()?;
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let inventory = GlobalProjectInventory::at(
        installation,
        usize::try_from(policy.limit(ResourceSetting::ContentInputBytes).value)?,
    )?
    .load(original.snapshot())?;
    let selections = daemon_selections(installation, None, &inventory)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    run_foreground_daemon(
        installation,
        &config,
        selections,
        variables,
        &original,
        &cancellation,
        &runtime,
    )
}

fn run_foreground_daemon(
    installation: &InstallationPaths,
    config: &ConfigSnapshot,
    selections: Vec<(ProjectPaths, String)>,
    environment: Vec<(OsString, OsString)>,
    original: &graphit_hub::policy::RequestContext<'_>,
    cancellation: &Cancellation,
    runtime: &tokio::runtime::Runtime,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let binding = config
        .get::<&str>(&ConfigKey::new("agent")?)?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "daemon requires configured agent",
            )
        })?
        .parse::<graphit_hub::NativeBinding>()?;
    let inputs: Vec<_> = selections
        .iter()
        .map(
            |(project, version)| graphit_daemon::project_bootstrap::ProjectBootstrapInput {
                project,
                explicit_version: version,
                pin: None,
                detached_base: "",
            },
        )
        .collect();
    let owner = runtime.block_on(ForegroundSource::start_and_publish(
        installation,
        &inputs,
        0,
        config,
        binding,
        environment,
        original,
    ));
    drop(inputs);
    drop(selections);
    match owner {
        Ok(owner) => runtime
            .block_on(owner.run(cancellation))
            .map_err(|failure| Box::new(failure) as Box<dyn std::error::Error>)?,
        Err(failure) => {
            diagnostic("daemon startup", &failure);
            cleanup_startup(failure, &HubResourcePolicy::resolve(config)?, runtime);
            return Ok(if cancellation.is_cancelled() {
                ExitCode::from(130)
            } else {
                ExitCode::FAILURE
            });
        }
    };
    Ok(ExitCode::SUCCESS)
}

fn daemon_selections(
    installation: &InstallationPaths,
    current: Option<(&ProjectPaths, &str)>,
    inventory: &ProjectInventorySnapshot,
) -> io::Result<Vec<(ProjectPaths, String)>> {
    let mut selections = current
        .map(|(project, version)| vec![(project.clone(), version.to_owned())])
        .unwrap_or_default();
    for entry in inventory.projects() {
        for checkout in entry.checkouts() {
            let project = ProjectPaths::at(checkout.path(), installation)?;
            // Retained versions describe history, not additional source trees.
            let selected = checkout.selected_version();
            if !selections
                .iter()
                .any(|(p, v)| p.root() == project.root() && v == selected)
            {
                selections.push((project, selected.to_owned()));
            }
        }
    }
    Ok(selections)
}

// Consume only retained SDK resources from this failed startup; never unlink a key by path.
fn cleanup_startup(
    mut failure: graphit_app::foreground_source::ForegroundFailure,
    policy: &HubResourcePolicy,
    runtime: &tokio::runtime::Runtime,
) {
    if let Some(owner) = failure.owner.take() {
        if let Err(error) = runtime.block_on(owner.shutdown()) {
            diagnostic("daemon cleanup", &error);
        }
        return;
    }
    let Some(error) = failure.error.into_inner() else {
        return;
    };
    let Ok(auth) = error.downcast::<graphit_daemon::auth_startup::AuthStartupFailure>() else {
        return;
    };
    let Some(resources) = auth.into_resources() else {
        return;
    };
    let cleanup = Cancellation::default();
    let original = match policy.request_context(Instant::now(), &cleanup) {
        Ok(original) => original,
        Err(error) => {
            diagnostic("daemon cleanup", &error);
            return;
        }
    };
    match resources {
        graphit_daemon::auth_startup::StartupResources::RuntimeOwner { owner, singleton } => {
            let discarded = owner.discard_pending(original.snapshot());
            if let Some(error) = discarded.failure().or(discarded.cleanup_failure()) {
                diagnostic("daemon cleanup", &error);
                return;
            }
            let removed = owner.remove(original.snapshot());
            if let Some(error) = removed.failure().or(removed.cleanup_failure()) {
                diagnostic("daemon cleanup", &error);
            }
            drop(owner);
            drop(singleton);
        }
        graphit_daemon::auth_startup::StartupResources::RuntimeStart { outcome, singleton } => {
            if let Some(recovery) = outcome.into_recovery() {
                let discarded = recovery.discard(original.snapshot());
                if let Some(error) = discarded.failure() {
                    diagnostic("daemon cleanup", &error);
                }
            }
            drop(singleton);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_parser_images_capture_environment_and_keep_command_precedence() {
        let brand = Brand::default();
        let key = ConfigKey::new("ast.scip.parsers").unwrap();
        let version = ConfigKey::new("ast.scip.version").unwrap();
        let environment = ConfigSnapshot::capture_environment(
            &brand,
            &environment_keys().unwrap(),
            [
                (
                    key.environment_variable(&brand).into(),
                    ".exm=registry.example/parser:1".into(),
                ),
                (version.environment_variable(&brand).into(), "v2".into()),
            ],
        )
        .unwrap();
        let captured = ConfigSnapshot::new(&brand, [environment.clone()]).unwrap();
        assert_eq!(
            captured.get::<&str>(&key).unwrap(),
            Some(".exm=registry.example/parser:1")
        );
        assert_eq!(captured.get::<&str>(&version).unwrap(), Some("v2"));
        let explicit =
            command_config(&brand, vec![parse_inline("ast.scip.parsers=").unwrap()]).unwrap();
        let selected = ConfigSnapshot::new(&brand, [environment, explicit]).unwrap();
        assert_eq!(selected.get::<&str>(&key).unwrap(), Some(""));
        assert_eq!(
            selected.source(&key),
            Some(graphit_core::config::ConfigSource::Command)
        );
    }
    use graphit_core::{
        brand::Brand,
        module_settings::{Module, ModuleSettings},
    };

    fn project_selection_fixture() -> (tempfile::TempDir, InstallationPaths, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let current = directory.path().join("current");
        let sibling = directory.path().join("sibling");
        for project in [&current, &sibling] {
            std::fs::create_dir_all(project.join(".graphit")).unwrap();
            std::fs::create_dir_all(project.join(".git")).unwrap();
            std::fs::create_dir_all(project.join("src")).unwrap();
        }
        let installation = InstallationPaths::resolve(
            Brand::default(),
            &current,
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        (directory, installation, current, sibling)
    }

    #[test]
    fn daemon_selections_use_each_checkout_version_without_expanding_retained_history() {
        use graphit_core::native_path::NativePath;
        let (_directory, installation, current, sibling) = project_selection_fixture();
        let current = ProjectPaths::at(&current, &installation).unwrap();
        let sibling = ProjectPaths::at(&sibling, &installation).unwrap();
        let inventory: ProjectInventorySnapshot = serde_json::from_value(serde_json::json!({
            "schema": 1,
            "brand": installation.brand().key(),
            "projects": [{
                "identity": {
                    "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
                    "metadata": {"name": "example", "ephemeral": false}
                },
                "versions": ["main", "feature", "retained-without-checkout"],
                "checkouts": [{
                    "path": NativePath::new(current.root().to_owned()).unwrap(),
                    "selected_version": "main", "default_version": "main",
                    "source": "Checkout", "checkout": {"kind": "Branch", "name": "main"}
                }, {
                    "path": NativePath::new(sibling.root().to_owned()).unwrap(),
                    "selected_version": "feature", "default_version": "main",
                    "source": "Checkout", "checkout": {"kind": "Branch", "name": "feature"}
                }]
            }]
        }))
        .unwrap();
        let before = inventory.clone();
        for initial in [None, Some((&current, "main"))] {
            let selections = daemon_selections(&installation, initial, &inventory).unwrap();
            let pairs: Vec<_> = selections
                .iter()
                .map(|(p, version)| (p.root(), version.as_str()))
                .collect();
            assert_eq!(
                pairs,
                vec![(current.root(), "main"), (sibling.root(), "feature")]
            );
        }
        assert_eq!(inventory, before);
        assert_eq!(
            inventory
                .projects()
                .next()
                .unwrap()
                .versions()
                .collect::<Vec<_>>(),
            vec!["main", "feature", "retained-without-checkout"]
        );
    }

    #[test]
    fn project_selection_normalizes_relative_and_absolute_explicit_directories() {
        let (_directory, installation, current, sibling) = project_selection_fixture();
        let sibling_absolute = sibling.to_str().unwrap();
        for (selector, expected) in [
            (".", &current),
            ("src", &current),
            ("../sibling", &sibling),
            ("../sibling/src", &sibling),
            (sibling_absolute, &sibling),
        ] {
            for operation in [
                vec!["ast", "schema"],
                vec!["ast", "source", "main.rs"],
                vec!["ast", "export", "--output", "graph.tar"],
                vec!["task", "schema"],
                vec![
                    "knowledge",
                    "ask",
                    "--query",
                    "question",
                    "--slug",
                    "page",
                    "--model",
                    "local",
                ],
            ] {
                let parsed = Arguments::try_parse_from(
                    ["graphit", "--project-version", "selected-version"]
                        .into_iter()
                        .chain(operation)
                        .chain(["--project-dir", selector]),
                )
                .unwrap();
                assert_eq!(parsed.project_version.as_deref(), Some("selected-version"));
                let mut command = parsed.command.unwrap();
                let selected = command_project(&mut command, &current, &installation).unwrap();
                let expected = std::fs::canonicalize(expected).unwrap();
                assert_eq!(selected.root(), expected);
                let normalized = match &mut command {
                    Command::Ast(ast) => match &mut ast.command {
                        AstCommand::Graph(graph) => {
                            graph.selection_mut().unwrap().project_dir.clone()
                        }
                        AstCommand::Source(source) => {
                            source.project_dir.as_deref().map(PathBuf::from)
                        }
                        AstCommand::Export(export) => {
                            assert_eq!(export.output, PathBuf::from("graph.tar"));
                            export.project_dir.clone()
                        }
                    },
                    Command::Task(task) => task.command.selection_mut().project_dir.clone(),
                    Command::Knowledge(KnowledgeArguments {
                        command: KnowledgeCommand::Ask(ask),
                    }) => ask.project_dir.as_deref().map(PathBuf::from),
                    _ => unreachable!(),
                };
                assert_eq!(normalized.as_deref(), Some(expected.as_path()));
                graphit_core::native_path::NativePath::new(normalized.unwrap()).unwrap();
            }
        }
        assert!(!installation.root().exists(), "selection must be read-only");
    }

    #[test]
    fn project_selection_preserves_omitted_locators_ids_and_context_conflicts() {
        let (_directory, installation, current, _) = project_selection_fixture();
        for args in [
            vec!["graphit", "ast", "schema"],
            vec!["graphit", "task", "schema"],
            vec!["graphit", "task", "schema", "--project-id", "selected-id"],
        ] {
            let mut command = Arguments::try_parse_from(args).unwrap().command.unwrap();
            let selected =
                command_project(&mut command, &current.join("src"), &installation).unwrap();
            assert_eq!(selected.root(), std::fs::canonicalize(&current).unwrap());
            match &mut command {
                Command::Ast(ast) => match &mut ast.command {
                    AstCommand::Graph(graph) => {
                        assert!(graph.selection_mut().unwrap().project_dir.is_none())
                    }
                    _ => unreachable!(),
                },
                Command::Task(task) => {
                    let selection = task.command.selection_mut();
                    assert!(selection.project_dir.is_none());
                    if let Some(id) = &selection.project_id {
                        assert_eq!(id, "selected-id");
                    }
                }
                _ => unreachable!(),
            }
        }
        for args in [
            vec![
                "graphit",
                "ast",
                "schema",
                "--context",
                "artifact@v1",
                "--project-dir",
                ".",
            ],
            vec![
                "graphit",
                "ast",
                "export",
                "--output",
                "graph.tar",
                "--context",
                "artifact@v1",
                "--project-dir",
                ".",
            ],
        ] {
            assert!(Arguments::try_parse_from(args).is_err());
        }
        let parsed =
            Arguments::try_parse_from(["graphit", "ast", "schema", "--context", "artifact@v1"])
                .unwrap();
        let Some(Command::Ast(ast)) = parsed.command else {
            panic!("expected AST command");
        };
        assert!(matches!(ast.command,
            AstCommand::Graph(graphit_app::graph_entry::Command::Schema(selection))
                if selection.context.as_deref() == Some("artifact@v1") && selection.project_dir.is_none()
        ));
    }

    #[test]
    fn project_selection_rejects_missing_files_and_installation_directories() {
        let (_directory, installation, current, _) = project_selection_fixture();
        std::fs::write(current.join("file"), b"not a directory").unwrap();
        std::fs::create_dir_all(installation.root()).unwrap();
        assert!(
            Arguments::try_parse_from(["graphit", "ast", "schema", "--project-dir", ""]).is_err()
        );
        for selector in ["missing", "file", "../installation"] {
            let mut command =
                Arguments::try_parse_from(["graphit", "ast", "schema", "--project-dir", selector])
                    .unwrap()
                    .command
                    .unwrap();
            assert!(command_project(&mut command, &current, &installation).is_err());
        }
    }

    #[test]
    fn ui_selects_installation_without_a_project_or_desktop() {
        let parsed = Arguments::try_parse_from([
            "graphit",
            "--global-dir",
            "/private/installation",
            "ui",
            "--no-open",
        ])
        .unwrap();
        assert!(matches!(
            parsed.command,
            Some(Command::Ui(graphit_app::ui_entry::Arguments {
                no_open: true
            }))
        ));
    }

    #[test]
    fn daemon_manager_invocation_selects_installation_without_a_project() {
        let managed = Arguments::try_parse_from([
            "graphit",
            "--global-dir",
            "/private/installation",
            "daemon",
            "--managed",
        ])
        .unwrap();
        assert_eq!(
            managed.global_dir,
            Some(PathBuf::from("/private/installation"))
        );
        assert!(matches!(managed.command, Some(Command::Daemon(arguments))
            if arguments.managed && arguments.command.is_none()));
        let foreground = Arguments::try_parse_from(["graphit", "daemon", "foreground"]).unwrap();
        assert!(
            matches!(foreground.command, Some(Command::Daemon(arguments))
            if !arguments.managed && matches!(arguments.command, Some(DaemonCommand::Foreground)))
        );
    }

    #[test]
    fn daemon_detached_mode_uses_inventory_and_conflicts_with_managed() {
        let parsed = Arguments::try_parse_from(["graphit", "daemon", "--detached"]).unwrap();
        assert!(matches!(parsed.command, Some(Command::Daemon(arguments))
            if arguments.command.is_none() && arguments.detached && !arguments.managed));
        assert!(
            Arguments::try_parse_from(["graphit", "daemon", "--managed", "--detached"]).is_err()
        );
    }

    #[test]
    fn daemon_controls_are_direct_commands() {
        for command in ["start", "stop", "restart", "status", "install", "uninstall"] {
            let parsed = Arguments::try_parse_from([
                "graphit",
                "--global-dir",
                "/private/installation",
                "daemon",
                command,
            ])
            .unwrap();
            assert!(matches!(parsed.command, Some(Command::Daemon(arguments))
                if !arguments.managed && matches!(arguments.command, Some(DaemonCommand::Control(_)))));
        }
        let install =
            Arguments::try_parse_from(["graphit", "daemon", "install", "--login"]).unwrap();
        assert!(matches!(install.command, Some(Command::Daemon(arguments))
            if matches!(arguments.command, Some(DaemonCommand::Control(ServiceCommand::Install { login: true })))));
        for action in ["enable", "disable"] {
            let parsed = Arguments::try_parse_from(["graphit", "daemon", "login", action]).unwrap();
            assert!(matches!(parsed.command, Some(Command::Daemon(arguments))
                if matches!(arguments.command, Some(DaemonCommand::Control(ServiceCommand::Login { .. })))));
        }
        assert!(Arguments::try_parse_from(["graphit", "daemon", "service", "start"]).is_err());
        assert!(Arguments::try_parse_from(["graphit", "daemon", "login"]).is_err());
    }

    #[test]
    fn ast_graph_subcommands_select_current_project_or_exact_context() {
        for operation in ["schema", "sample"] {
            let parsed = Arguments::try_parse_from(["graphit", "ast", operation]).unwrap();
            assert!(
                matches!(parsed.command, Some(Command::Ast(a)) if matches!(a.command, AstCommand::Graph(_)))
            );
        }
        let parsed = Arguments::try_parse_from([
            "graphit",
            "--project-version",
            "feature/graph",
            "ast",
            "query",
            "MATCH (f:File) RETURN f.path",
            "--param-string",
            "path=main.rs",
            "--project-dir",
            "/project",
        ])
        .unwrap();
        assert_eq!(parsed.project_version.as_deref(), Some("feature/graph"));
        assert!(
            matches!(parsed.command, Some(Command::Ast(a)) if matches!(a.command, AstCommand::Graph(graphit_app::graph_entry::Command::Query(_))))
        );
        Arguments::try_parse_from([
            "graphit",
            "ast",
            "neighborhood",
            "--anchor-label",
            "File",
            "--anchor-key",
            "main.rs",
            "--relationship",
            "CONTAINS",
            "--other-label",
            "Function",
        ])
        .unwrap();
        assert!(
            Arguments::try_parse_from([
                "graphit",
                "ast",
                "schema",
                "--context",
                "id@v1",
                "--project-dir",
                "/project"
            ])
            .is_err()
        );
    }

    #[test]
    fn session_hook_parser_and_module_environment_capture() {
        let args = Arguments::try_parse_from([
            "graphit",
            "--project-version",
            "feature",
            "_session-hook",
            "--engine",
            "kiro",
            "--event",
            "SessionStart",
        ])
        .unwrap();
        assert_eq!(args.project_version.as_deref(), Some("feature"));
        assert!(matches!(args.command, Some(Command::SessionHook(ref hook))
            if matches!(hook.event, graphit_app::session_hook_entry::Event::SessionStart)));
        let prompt = Arguments::try_parse_from([
            "graphit",
            "_session-hook",
            "--engine",
            "kiro",
            "--event",
            "UserPromptSubmit",
        ])
        .unwrap();
        assert!(
            matches!(prompt.command, Some(Command::SessionHook(ref hook))
            if matches!(hook.event, graphit_app::session_hook_entry::Event::UserPromptSubmit))
        );
        for legacy in ["agentSpawn", "userPromptSubmit"] {
            assert!(
                Arguments::try_parse_from([
                    "graphit",
                    "_session-hook",
                    "--engine",
                    "kiro",
                    "--event",
                    legacy,
                ])
                .is_err()
            );
        }
        assert!(
            Arguments::try_parse_from([
                "graphit",
                "_session-hook",
                "--engine",
                "kiro",
                "--event",
                "SessionStart",
                "--v3",
            ])
            .is_err()
        );
        assert!(
            Arguments::try_parse_from([
                "graphit",
                "_session-hook",
                "--engine",
                "other",
                "--event",
                "SessionStart",
            ])
            .is_err()
        );
        let brand = Brand::default();
        let environment = ConfigSnapshot::capture_environment(
            &brand,
            &environment_keys().unwrap(),
            [Module::Hooks, Module::Memory]
                .map(|module| {
                    (
                        OsString::from(module.key().environment_variable(&brand)),
                        OsString::from("false"),
                    )
                })
                .into_iter()
                .chain(std::iter::once((
                    OsString::from(
                        ConfigKey::new("ai.cli")
                            .unwrap()
                            .environment_variable(&brand),
                    ),
                    OsString::from("custom/provider/copilot"),
                ))),
        )
        .unwrap();
        let snapshot = ConfigSnapshot::new(&brand, [environment]).unwrap();
        assert_eq!(
            snapshot
                .get::<String>(&ConfigKey::new("ai.cli").unwrap())
                .unwrap()
                .as_deref(),
            Some("custom/provider/copilot")
        );
        let modules = ModuleSettings::resolve(&snapshot).unwrap();
        assert!(!modules.enabled(Module::Hooks));
        assert!(!modules.enabled(Module::Memory));
        assert!(modules.enabled(Module::Ast));
    }

    #[test]
    fn daemon_global_policy_defaults_and_explicit_equal_value_keep_origin() {
        use graphit_core::brand::Brand;
        use graphit_hub::policy::LimitOrigin;
        let directory = tempfile::tempdir().unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(directory.path()),
        )
        .unwrap();
        let config = daemon_global_config(&installation, &[], Vec::new()).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        for spec in RESOURCE_LIMITS {
            let actual = policy.limit(spec.setting);
            assert_eq!(actual.value, spec.default_value);
            assert_eq!(actual.origin, LimitOrigin::DistributionDefault);
        }
        let setting = ResourceSetting::RequestDeadlineMs.spec();
        let explicit = daemon_global_config(
            &installation,
            &[],
            vec![(
                ConfigKey::new(setting.key).unwrap(),
                serde_json::json!(setting.default_value),
            )],
        )
        .unwrap();
        let configured = HubResourcePolicy::resolve(&explicit)
            .unwrap()
            .limit(setting.setting);
        assert_eq!(configured.value, policy.limit(setting.setting).value);
        assert_eq!(
            configured.origin,
            LimitOrigin::Configuration(ConfigSource::Command)
        );
    }
}
