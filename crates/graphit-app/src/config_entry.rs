//! Local configuration commands over the maintained Core stores and snapshots.
use clap::{Args, Subcommand};
use graphit_core::{
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource, ConfigStore},
    file_lock::Cancellation,
    local_files::directory_exists,
    module_settings::{Module, ModuleSettings},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::policy::HubResourcePolicy;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet, ffi::OsString, io, io::Write, num::NonZeroU64, path::Path,
    time::Duration,
};

#[derive(Args)]
pub struct Arguments {
    /// Write or inspect only the installation's global configuration.
    #[arg(long, global = true)]
    global: bool,
    /// Maximum time to wait for a configuration writer, in seconds.
    #[arg(long, global = true, default_value = "30")]
    timeout: NonZeroU64,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Persist JSON or literal text; CSV settings and secrets remain strings.
    #[command(
        after_help = "CSV list settings: ast.grammar, ast.grammars_whitelist, ast.grammars_blacklist, ast.scip.parsers, knowledge.extensions. An empty value clears the selected list, subject to that setting's validation.\n\nExamples:\n  graphit config set knowledge.extensions md,txt\n  graphit config set ast.scip.parsers .zig=registry.example/parser:1,.exm=registry.example/parser:2\n\nStructured settings such as githooks, task.browser, daemon.models and agents.<agent>.instructions require JSON objects. Hook commands are complete strings, never CSV. Project writes preserve identity and metadata in project.json; githooks is at the document root, outside settings. Inside task.browser, providers accepts an array of strings or a comma-separated string."
    )]
    Set {
        #[arg(value_parser = ConfigKey::new)]
        key: ConfigKey,
        #[arg(allow_hyphen_values = true)]
        value: String,
    },
    /// Inspect a configured value and its source, with secret redaction.
    Get {
        #[arg(value_parser = ConfigKey::new)]
        key: ConfigKey,
    },
    /// List configured values and their sources, with secret redaction.
    List,
    /// Remove a value from the selected Project or Global store.
    Unset {
        #[arg(value_parser = ConfigKey::new)]
        key: ConfigKey,
    },
}

pub fn run(
    arguments: Arguments,
    installation: &InstallationPaths,
    cwd: &Path,
    variables: &[(OsString, OsString)],
    inline: Vec<(ConfigKey, Value)>,
    environment_keys: Vec<ConfigKey>,
    cancellation: &Cancellation,
) -> io::Result<()> {
    let Arguments {
        global,
        timeout,
        command,
    } = arguments;
    match command {
        Command::Set { key, value } => {
            let value = key.parse_value(&value)?;
            if Module::ALL
                .iter()
                .any(|module| key.as_str().strip_prefix("modules.") == Some(module.name()))
            {
                ModuleSettings::resolve(&ConfigSnapshot::new(
                    installation.brand(),
                    [ConfigDocument::new(
                        installation.brand(),
                        ConfigSource::Command,
                        [(key.clone(), value.clone())],
                    )?],
                )?)?;
            }
            let store = mutation_store(installation, cwd, global)?;
            let changed =
                store.set(key, value, Duration::from_secs(timeout.get()), cancellation)?;
            emit(&json!({"changed": changed}))
        }
        Command::Unset { key } => {
            let store = mutation_store(installation, cwd, global)?;
            let changed = store.unset(key, Duration::from_secs(timeout.get()), cancellation)?;
            emit(&json!({"changed": changed}))
        }
        command => {
            let requested = match &command {
                Command::Get { key } => Some(key.clone()),
                _ => None,
            };
            let mut keys: BTreeSet<_> = environment_keys.into_iter().collect();
            keys.extend(requested.iter().cloned());
            let snapshot = read_configuration(installation, cwd, variables, inline, keys, global)?;
            let values = snapshot.inspect();
            if let Some(key) = requested {
                let value = values
                    .into_iter()
                    .find(|value| value.key == key.as_str())
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            "configuration key is not configured",
                        )
                    })?;
                emit(&value)
            } else {
                emit(&values)
            }
        }
    }
}

fn mutation_store(
    installation: &InstallationPaths,
    cwd: &Path,
    global: bool,
) -> io::Result<ConfigStore> {
    if global {
        ConfigStore::global(installation)
    } else {
        let project = ProjectPaths::discover(cwd, installation)?;
        if !directory_exists(&project.brand_directory())? {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "project configuration requires an existing Graphit checkout",
            ));
        }
        ConfigStore::project(&project)
    }
}

fn read_configuration(
    installation: &InstallationPaths,
    cwd: &Path,
    variables: &[(OsString, OsString)],
    inline: Vec<(ConfigKey, Value)>,
    mut keys: BTreeSet<ConfigKey>,
    global: bool,
) -> io::Result<ConfigSnapshot> {
    let global_document = ConfigStore::global(installation)?.load()?;
    if global {
        return ConfigSnapshot::new(installation.brand(), [global_document]);
    }
    let mut documents = vec![
        HubResourcePolicy::distribution_defaults(installation.brand())?,
        global_document,
    ];
    match ProjectPaths::discover(cwd, installation) {
        Ok(project) => {
            if directory_exists(&project.brand_directory())? {
                documents.push(ConfigStore::project(&project)?.load()?);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    for document in &documents {
        for entry in document.inspect() {
            keys.insert(ConfigKey::new(&entry.key)?);
        }
    }
    keys.extend(inline.iter().map(|(key, _)| key.clone()));
    documents.push(ConfigSnapshot::capture_environment(
        installation.brand(),
        &keys.into_iter().collect::<Vec<_>>(),
        variables.iter().cloned(),
    )?);
    documents.push(ConfigDocument::new(
        installation.brand(),
        ConfigSource::Command,
        inline
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>(),
    )?);
    ConfigSnapshot::new(installation.brand(), documents)
}

fn emit(value: &impl serde::Serialize) -> io::Result<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, value).map_err(io::Error::other)?;
    writeln!(output)
}
