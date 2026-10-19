//! CLI/MCP share one authenticated producer for the complete reconciliation.
use graphit_core::{
    config::{ConfigKey, ConfigSnapshot},
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, RequestContext},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

pub const TOOL: &str = "graphit_sync";
#[derive(clap::Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Arguments {
    /// Native agent; defaults to the effective Graphit agent configuration.
    #[arg(long)]
    pub agent: Option<NativeBinding>,
    /// Reconcile global skills/roles and Hub installations without a PROJECT.
    #[arg(long, conflicts_with_all = ["project_dir", "coordinator_source"])]
    #[serde(default)]
    pub global: bool,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    #[arg(skip)]
    pub project_version: Option<String>,
    /// Explicit product Coordinator source for development/package verification.
    #[arg(long)]
    pub coordinator_source: Option<PathBuf>,
}
#[derive(clap::Args)]
pub struct SkillsArguments {
    #[command(subcommand)]
    pub command: SkillsCommand,
}
#[derive(clap::Subcommand)]
pub enum SkillsCommand {
    Sync(crate::managed_roles::client::Selection),
}

pub fn prepare(
    mut input: Arguments,
    config: &ConfigSnapshot,
    _installation: &InstallationPaths,
    project: Option<&ProjectPaths>,
    version: Option<String>,
    _policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<rmcp::model::CallToolRequestParams> {
    original.check()?;
    if input.global != project.is_none() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if input.agent.is_none() {
        input.agent = config
            .get::<String>(&ConfigKey::new("agent")?)?
            .map(|value| {
                value
                    .parse()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
            })
            .transpose()?;
    }
    input.coordinator_source = input
        .coordinator_source
        .map(std::fs::canonicalize)
        .transpose()?;
    input.project_dir = project.map(|project| project.root().to_owned());
    input.project_version = version;
    Ok(
        rmcp::model::CallToolRequestParams::new(TOOL).with_arguments(
            serde_json::to_value(input)?
                .as_object()
                .ok_or(io::ErrorKind::InvalidData)?
                .clone(),
        ),
    )
}
pub async fn run(
    prepared: rmcp::model::CallToolRequestParams,
    installation: &InstallationPaths,
    _policy: &HubResourcePolicy,
    limits: NativeDocumentFileLimits,
    original: &RequestContext<'_>,
    output: &mut impl Write,
) -> io::Result<ExitCode> {
    if let Err(error) =
        crate::hub_publish::call(installation, prepared, limits, original, output).await
    {
        if let Some(failure) = error.get_ref().and_then(|cause| {
            cause.downcast_ref::<crate::hub_publish::PublicationDeliveryFailure>()
        }) && let Some(result) = &failure.result.structured_content
        {
            serde_json::to_writer(&mut *output, result)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
        return Err(error);
    }
    Ok(ExitCode::SUCCESS)
}
