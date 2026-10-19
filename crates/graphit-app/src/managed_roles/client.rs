//! Compiled CLI client; absent discovery never starts or rotates another daemon.
use super::{InstallDestination, InstallOutcome, InstallRequest, ManagedRoleName, RuleSelection};
use crate::ast_source::runtime_headers;
use graphit_core::{
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::source_discovery::read_discovery;
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, RequestContext},
};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    process::ExitCode,
};
#[derive(clap::Args)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Command,
}
#[derive(clap::Subcommand)]
pub enum Command {
    Install(Selection),
    Sync(Selection),
    Run(RunArguments),
}
#[derive(clap::Args)]
pub struct RunArguments {
    #[command(flatten)]
    pub selection: Selection,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub prompt: String,
}
impl Command {
    pub fn global(&self) -> bool {
        let selection = match self {
            Self::Install(s) | Self::Sync(s) => s,
            Self::Run(r) => &r.selection,
        };
        selection.destination == InstallDestination::Global && selection.source_project.is_none()
    }
    pub fn source_project(&self) -> Option<&std::path::Path> {
        match self {
            Self::Install(s) | Self::Sync(s) => s.source_project.as_deref(),
            Self::Run(r) => r.selection.source_project.as_deref(),
        }
    }
}
#[derive(clap::Args)]
pub struct Selection {
    /// Physical destination. Global never captures the current directory as a PROJECT.
    #[arg(long, value_enum, default_value = "project")]
    pub destination: InstallDestination,
    /// Explicit PROJECT origin for raw overrides/installed bundles; independently admitted.
    #[arg(long)]
    pub source_project: Option<std::path::PathBuf>,
    #[arg(long)]
    pub agent: Option<NativeBinding>,
    #[arg(long, value_enum)]
    pub role: Vec<ManagedRoleName>,
    /// A qualified raw bundle for one role: ROLE=ID@VERSION.
    #[arg(long)]
    pub rule: Vec<String>,
    /// Effective configured skill name: MODULE=NAME.
    #[arg(long)]
    pub skill_alias: Vec<String>,
    /// Installed instruction bundle: MODULE=ID@VERSION; remote origins require current access.
    #[arg(long)]
    pub skill_bundle: Vec<String>,
    /// Explicit Native identity for a selected standalone bundle: MODULE=PROVIDER.
    #[arg(long)]
    pub skill_identity_provider: Vec<String>,
    #[arg(long)]
    pub module_skill: Option<String>,
}
/// Validate and serialize one request before daemon startup or transport.
pub fn prepare(
    arguments: Arguments,
    project: Option<&ProjectPaths>,
    version: Option<String>,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<(&'static str, Vec<u8>)> {
    original.check()?;
    let _deadline = original.snapshot().required_deadline()?;
    let (selection, run) = match arguments.command {
        Command::Install(selection) | Command::Sync(selection) => (selection, None),
        Command::Run(input) => (input.selection, Some((input.model, input.prompt))),
    };
    if (run.is_some() && selection.destination != InstallDestination::Project)
        || (selection.destination == InstallDestination::Project && project.is_none())
        || (project.is_none() && (version.is_some() || selection.source_project.is_some()))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Role run/PROJECT publication requires an admitted PROJECT; Global run is unavailable",
        ));
    }
    let mut aliases = BTreeMap::new();
    for alias in selection.skill_alias {
        let (module, name) = alias
            .split_once('=')
            .filter(|(a, b)| !a.is_empty() && !b.is_empty())
            .ok_or(io::ErrorKind::InvalidInput)?;
        if aliases.insert(module.to_owned(), name.to_owned()).is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    let mut skill_bundles = BTreeMap::new();
    for bundle in selection.skill_bundle {
        let (module, artifact) = bundle.split_once('=').ok_or(io::ErrorKind::InvalidInput)?;
        if !["task", "memory", "ast", "knowledge", "hub"].contains(&module) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let (id, version) = artifact
            .rsplit_once('@')
            .ok_or(io::ErrorKind::InvalidInput)?;
        graphit_hub::ArtifactId::new(id, policy.projection())
            .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
        graphit_hub::ConcreteVersion::new(version, policy.projection())
            .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
        if skill_bundles
            .insert(
                module.to_owned(),
                crate::compiled_skills::HubSelection {
                    id: id.to_owned(),
                    version: version.to_owned(),
                    identity_provider: None,
                },
            )
            .is_some()
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    for identity in selection.skill_identity_provider {
        let (module, provider) = identity
            .split_once('=')
            .filter(|(module, provider)| !module.is_empty() && !provider.is_empty())
            .ok_or(io::ErrorKind::InvalidInput)?;
        if provider.len() > policy.content().max_scalar_bytes {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let bundle = skill_bundles
            .get_mut(module)
            .ok_or(io::ErrorKind::InvalidInput)?;
        if bundle
            .identity_provider
            .replace(provider.to_owned())
            .is_some()
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    let mut rules = Vec::new();
    for rule in selection.rule {
        let (role, artifact) = rule.split_once('=').ok_or(io::ErrorKind::InvalidInput)?;
        let (id, version) = artifact
            .rsplit_once('@')
            .filter(|(a, b)| !a.is_empty() && !b.is_empty())
            .ok_or(io::ErrorKind::InvalidInput)?;
        let role = match role {
            "scout" => ManagedRoleName::Scout,
            "tracker" => ManagedRoleName::Tracker,
            "scribe" => ManagedRoleName::Scribe,
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        };
        rules.push(RuleSelection {
            role,
            id: id.into(),
            version: version.into(),
        });
    }
    let input = InstallRequest {
        destination: selection.destination,
        project_dir: project.map(|project| project.root().to_owned()),
        project_version: version,
        binding: selection.agent,
        roles: selection.role,
        skills_only: false,
        rules,
        skill_aliases: aliases,
        skill_bundles,
        module_skill: selection.module_skill,
    };
    let (route, body) = match run {
        Some((model, prompt)) => {
            if model.as_ref().is_some_and(|model| {
                model.is_empty() || model.len() > policy.content().max_scalar_bytes
            }) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            if !input.skill_bundles.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "skill bundles require roles install or sync",
                ));
            }
            if input.roles.len() != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "roles run requires exactly one --role",
                ));
            }
            let role = input.roles[0];
            (
                super::http::RUN_ROUTE,
                serde_json::to_value(crate::managed_role_run::RunRequest {
                    install: input,
                    role,
                    model,
                    prompt,
                })?,
            )
        }
        None => (super::http::ROUTE, serde_json::to_value(input)?),
    };
    let body = serde_json::to_vec(&body).map_err(io::Error::other)?;
    if body.len() > policy.content().max_input_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    original.check()?;
    Ok((route, body))
}

/// Publish only maintained module skills, without configuring Role grants/files.
pub fn prepare_skills(
    selection: Selection,
    project: Option<&ProjectPaths>,
    version: Option<String>,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<(&'static str, Vec<u8>)> {
    let (route, body) = prepare(
        Arguments {
            command: Command::Sync(selection),
        },
        project,
        version,
        policy,
        original,
    )?;
    let mut request: InstallRequest = serde_json::from_slice(&body)?;
    request.skills_only = true;
    Ok((route, serde_json::to_vec(&request)?))
}

/// Send the prepared request through the active daemon; this never starts it.
pub async fn run(
    prepared: (&'static str, Vec<u8>),
    installation: &InstallationPaths,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
    output: &mut dyn Write,
) -> io::Result<ExitCode> {
    original.check()?;
    let deadline = original.snapshot().required_deadline()?;
    let (route, body) = prepared;
    let is_run = route == super::http::RUN_ROUTE;
    if body.len() > policy.content().max_input_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let limits = NativeDocumentFileLimits::new(
        policy.content().max_input_bytes,
        graphit_core::native_path::MAX_PATH_BYTES,
        policy.content().max_depth,
        policy.content().max_scalar_bytes,
    )?;
    let paths = installation.clone();
    let snapshot = original.snapshot().clone();
    let discovery = tokio::task::spawn_blocking(move || read_discovery(&paths, limits, &snapshot))
        .await
        .map_err(io::Error::other)?
        .map_err(io::Error::other)?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "managed roles require the active daemon",
            )
        })?;
    let headers = runtime_headers(installation, limits, original).await?;
    let client = reqwest::Client::new();
    let operation = async {
        let mut response = client
            .post(format!("http://{}{}", discovery.address(), route))
            .headers(headers.into_iter().collect())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(io::Error::other)?;
        if !response.status().is_success() {
            return Err(io::Error::new(
                if matches!(response.status().as_u16(), 401 | 403) {
                    io::ErrorKind::PermissionDenied
                } else {
                    io::ErrorKind::Other
                },
                format!("managed roles returned HTTP {}", response.status()),
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(io::Error::other)? {
            original.snapshot().check()?;
            if chunk.len()
                > policy
                    .content()
                    .max_output_bytes
                    .saturating_sub(bytes.len())
            {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if is_run {
            let result: crate::managed_role_run::RunOutcome = serde_json::from_slice(&bytes)?;
            original.snapshot().check()?;
            serde_json::to_writer(&mut *output, &result)?;
            output.write_all(b"\n")?;
            output.flush()?;
            return Ok(if result.failure.is_some() || !result.run_closed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            });
        }
        let result: InstallOutcome = serde_json::from_slice(&bytes)?;
        original.snapshot().check()?;
        serde_json::to_writer(&mut *output, &result)?;
        output.write_all(b"\n")?;
        output.flush()?;
        Ok(if result.failure.is_some() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        })
    };
    tokio::select! {result=operation=>result,()=original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),()=tokio::time::sleep_until(deadline.into())=>Err(io::ErrorKind::TimedOut.into())}
}
