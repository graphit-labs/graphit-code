//! CLI inputs for the same authenticated Hub FILE tools exposed by Source.
use graphit_core::project_identity::ProjectId;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, NativeBinding,
    policy::{HubResourcePolicy, RequestContext},
};
use rmcp::model::CallToolRequestParams;
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};

#[derive(Clone, Copy, clap::ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Skill,
    Command,
    Workflow,
    Agent,
    Rule,
    Mcp,
    Language,
    Power,
}
impl Kind {
    pub(crate) fn artifact_type(self) -> ArtifactType {
        match self {
            Self::Skill => ArtifactType::Skill,
            Self::Command => ArtifactType::Command,
            Self::Workflow => ArtifactType::Workflow,
            Self::Agent => ArtifactType::Agent,
            Self::Rule => ArtifactType::Rule,
            Self::Mcp => ArtifactType::Mcp,
            Self::Language => ArtifactType::Language,
            Self::Power => ArtifactType::Power,
        }
    }
}
#[derive(Clone, Copy, clap::ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Broker,
    Standalone,
}

#[derive(clap::Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteArguments {
    #[arg(long, value_enum, default_value = "broker")]
    pub transport: Transport,
    #[arg(long)]
    pub provider: String,
    /// Explicit standalone transport profile. Omit only for unsigned standalone S3.
    #[arg(long)]
    pub profile: Option<String>,
    /// Explicit Native identity for standalone ACLs; does not select S3 credentials.
    #[arg(long)]
    pub identity_provider: Option<String>,
    #[arg(long)]
    pub producer: String,
    #[arg(long, value_enum)]
    #[serde(rename = "artifact_type")]
    pub kind: Kind,
    #[arg(long)]
    pub id: String,
    #[arg(long, default_value = "latest")]
    pub version: String,
    /// Treat a numeric version as a literal rather than a stable release prefix.
    #[arg(long)]
    pub exact: bool,
}
impl RemoteArguments {
    pub(crate) fn validate(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        let limit = policy.content().max_scalar_bytes;
        for text in std::iter::once(self.provider.as_str())
            .chain(self.profile.as_deref())
            .chain(self.identity_provider.as_deref())
        {
            if text.is_empty()
                || text.trim() != text
                || text.len() > limit
                || text.chars().any(char::is_control)
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        if matches!(self.transport, Transport::Broker)
            && (self.profile.is_some() || self.identity_provider.is_some())
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.producer
            .parse::<ProjectId>()
            .map_err(io::Error::other)?;
        ArtifactId::new(&self.id, policy.projection()).map_err(io::Error::other)?;
        ConcreteVersion::new(&self.version, policy.projection()).map_err(io::Error::other)?;
        Ok(())
    }
}

/// Physical PROJECT publication mode; this does not select a NativeRoot installer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PhysicalMode {
    Copy,
}
fn present_mode<'de, D: serde::Deserializer<'de>>(
    value: D,
) -> Result<Option<PhysicalMode>, D::Error> {
    PhysicalMode::deserialize(value).map(Some)
}

#[derive(clap::Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallArguments {
    #[command(flatten)]
    pub selection: RemoteArguments,
    #[arg(long)]
    pub global: bool,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    #[arg(skip)]
    pub project_version: Option<String>,
    #[arg(long)]
    pub binding: Option<NativeBinding>,
    #[arg(long)]
    pub alias: Option<String>,
    #[arg(long)]
    pub entry_name: Option<String>,
    /// PROJECT installation mode. The checkout receives a copy.
    #[arg(long, value_enum)]
    #[serde(
        default,
        deserialize_with = "present_mode",
        skip_serializing_if = "Option::is_none"
    )]
    pub mode: Option<PhysicalMode>,
}

/// A captured installation target. An omitted selector updates its remote entries.
#[derive(clap::Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAllArguments {
    #[arg(long)]
    pub global: bool,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    #[arg(skip)]
    pub project_version: Option<String>,
    #[arg(long)]
    pub binding: Option<NativeBinding>,
    #[arg(long)]
    pub identity_provider: Option<String>,
}

#[derive(clap::Args)]
pub struct UpdateArguments {
    #[command(flatten)]
    pub target: UpdateAllArguments,
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long, value_enum)]
    pub transport: Option<Transport>,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(long)]
    pub producer: Option<String>,
    #[arg(long, value_enum)]
    pub kind: Option<Kind>,
    #[arg(long)]
    pub version: Option<String>,
    #[arg(long)]
    pub exact: bool,
    #[arg(long)]
    pub alias: Option<String>,
    #[arg(long)]
    pub entry_name: Option<String>,
    #[arg(long, value_enum)]
    pub mode: Option<PhysicalMode>,
}

/// Refresh all recorded installations without choosing a new release.
pub fn prepare_sync(
    arguments: &UpdateAllArguments,
    project_version: Option<&str>,
    _policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    if arguments.global
        && (arguments.project_dir.is_some()
            || arguments.project_version.is_some()
            || project_version.is_some()
            || arguments.binding.is_some())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut value = serde_json::to_value(arguments).map_err(io::Error::other)?;
    if let Some(version) = project_version {
        value["project_version"] = version.into();
    }
    prepare("graphit_hub_sync", value, original)
}

pub fn prepare_update(
    arguments: &UpdateArguments,
    project_version: Option<&str>,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    if let Some(id) = &arguments.id {
        let input = InstallArguments {
            selection: RemoteArguments {
                transport: arguments.transport.unwrap_or(Transport::Broker),
                provider: arguments
                    .provider
                    .clone()
                    .ok_or(io::ErrorKind::InvalidInput)?,
                profile: arguments.profile.clone(),
                identity_provider: arguments.target.identity_provider.clone(),
                producer: arguments
                    .producer
                    .clone()
                    .ok_or(io::ErrorKind::InvalidInput)?,
                kind: arguments.kind.ok_or(io::ErrorKind::InvalidInput)?,
                id: id.clone(),
                version: arguments.version.clone().unwrap_or_else(|| "latest".into()),
                exact: arguments.exact,
            },
            global: arguments.target.global,
            project_dir: arguments.target.project_dir.clone(),
            project_version: arguments.target.project_version.clone(),
            binding: arguments.target.binding,
            alias: arguments.alias.clone(),
            entry_name: arguments.entry_name.clone(),
            mode: arguments.mode,
        };
        return prepare_install(&input, true, project_version, policy, original);
    }
    if arguments.transport.is_some()
        || arguments.provider.is_some()
        || arguments.profile.is_some()
        || arguments.producer.is_some()
        || arguments.kind.is_some()
        || arguments.version.is_some()
        || arguments.exact
        || arguments.alias.is_some()
        || arguments.entry_name.is_some()
        || arguments.mode.is_some()
        || arguments.target.global
            && (arguments.target.project_dir.is_some()
                || arguments.target.project_version.is_some()
                || project_version.is_some()
                || arguments.target.binding.is_some())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut value = serde_json::to_value(&arguments.target).map_err(io::Error::other)?;
    if let Some(version) = project_version {
        value["project_version"] = version.into();
    }
    prepare("graphit_hub_update", value, original)
}

#[derive(clap::Args)]
pub struct ContentArguments {
    #[command(subcommand)]
    pub command: ContentCommand,
}
#[derive(clap::Subcommand)]
pub enum ContentCommand {
    Installed(InstalledContentArguments),
    Remote(RemoteContentArguments),
}
#[derive(clap::Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledContentArguments {
    #[arg(long, value_enum)]
    #[serde(rename = "artifact_type")]
    pub kind: Kind,
    #[arg(long)]
    pub id: String,
    /// Exact installed artifact version; required when a global ID has multiple versions.
    #[arg(long)]
    pub version: Option<String>,
    /// Explicit verified Native identity for a stored standalone origin.
    #[arg(long)]
    pub identity_provider: Option<String>,
    #[arg(long)]
    pub global: bool,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    #[arg(skip)]
    pub project_version: Option<String>,
    #[arg(long)]
    pub binding: Option<NativeBinding>,
    /// Relative file in a directory artifact; omit for a projected single file.
    #[arg(long)]
    pub path: Option<String>,
}
#[derive(clap::Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteContentArguments {
    #[command(flatten)]
    pub selection: RemoteArguments,
    #[arg(long)]
    pub path: String,
}

#[derive(clap::Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ListArguments {
    #[arg(long)]
    pub global: bool,
    /// Explicit Native identity used only for stored standalone origins.
    #[arg(long)]
    pub identity_provider: Option<String>,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    #[arg(skip)]
    pub project_version: Option<String>,
    #[arg(long, value_enum)]
    #[serde(rename = "artifact_type")]
    pub kind: Option<Kind>,
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long)]
    pub version: Option<String>,
    #[arg(long)]
    pub binding: Option<NativeBinding>,
    #[arg(long, default_value_t = 0)]
    #[serde(default)]
    pub offset: usize,
    #[arg(long, default_value_t = 50)]
    #[serde(default = "default_page_limit")]
    pub limit: usize,
}
fn default_page_limit() -> usize {
    50
}

pub fn prepare_list(
    arguments: &ListArguments,
    project_version: Option<&str>,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    if arguments.global
        && (arguments.project_dir.is_some()
            || arguments.project_version.is_some()
            || project_version.is_some()
            || arguments.binding.is_some())
        || arguments.limit == 0
        || arguments.limit > policy.content().max_collection_entries
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if let Some(id) = &arguments.id {
        ArtifactId::new(id, policy.projection()).map_err(io::Error::other)?;
    }
    if let Some(version) = &arguments.version {
        ConcreteVersion::new(version, policy.projection()).map_err(io::Error::other)?;
    }
    let mut value = serde_json::to_value(arguments).map_err(io::Error::other)?;
    if let Some(version) = project_version {
        value["project_version"] = version.into();
    }
    prepare("graphit_hub_list", value, original)
}

pub fn prepare_install(
    arguments: &InstallArguments,
    update: bool,
    project_version: Option<&str>,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    arguments.selection.validate(policy)?;
    if arguments.global
        && (arguments.project_dir.is_some()
            || arguments.project_version.is_some()
            || project_version.is_some()
            || arguments.binding.is_some()
            || arguments.alias.is_some()
            || arguments.entry_name.is_some()
            || arguments.mode.is_some())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut value = serde_json::to_value(arguments).map_err(io::Error::other)?;
    if let Some(version) = project_version {
        value["project_version"] = version.into();
    }
    prepare(
        if update {
            "graphit_hub_update"
        } else {
            "graphit_hub_install"
        },
        value,
        original,
    )
}
pub fn prepare_content(
    arguments: &ContentArguments,
    project_version: Option<&str>,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    let value = match &arguments.command {
        ContentCommand::Remote(arguments) => {
            arguments.selection.validate(policy)?;
            serde_json::json!({"remote":arguments})
        }
        ContentCommand::Installed(arguments) => {
            ArtifactId::new(&arguments.id, policy.projection()).map_err(io::Error::other)?;
            if arguments.global
                && (arguments.project_dir.is_some()
                    || arguments.project_version.is_some()
                    || project_version.is_some()
                    || arguments.binding.is_some())
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let mut value = serde_json::to_value(arguments).map_err(io::Error::other)?;
            if let Some(version) = project_version {
                value["project_version"] = version.into();
            }
            serde_json::json!({"installed":value})
        }
    };
    prepare("graphit_hub_content", value, original)
}
fn prepare(
    name: &str,
    value: serde_json::Value,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    if serde_json::to_vec(&value).map_err(io::Error::other)?.len()
        > original.content_limits().max_input_bytes
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(CallToolRequestParams::new(name.to_owned()).with_arguments(
        value
            .as_object()
            .ok_or(io::ErrorKind::InvalidInput)?
            .clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};
    use std::time::Instant;

    #[test]
    fn update_without_id_selects_installed_target_and_rejects_ambiguous_overrides() {
        use clap::Parser;
        #[derive(Parser)]
        struct UpdateCli {
            #[command(flatten)]
            input: UpdateArguments,
        }
        let brand = Brand::default();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancellation = Cancellation::default();
        let request = policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        let mut input = UpdateCli::try_parse_from(["update", "--binding", "codex"])
            .unwrap()
            .input;
        let wire = prepare_update(&input, Some("feature/update"), &policy, &request).unwrap();
        let arguments = wire.arguments.unwrap();
        assert!(!arguments.contains_key("selection"));
        assert_eq!(arguments["binding"], "codex");
        assert_eq!(arguments["project_version"], "feature/update");
        input.provider = Some("must-not-override-registry".into());
        assert!(prepare_update(&input, None, &policy, &request).is_err());
        let input = UpdateCli::try_parse_from(["update", "--global"])
            .unwrap()
            .input;
        assert!(prepare_update(&input, None, &policy, &request).is_ok());
        assert!(prepare_update(&input, Some("main"), &policy, &request).is_err());
        let input = UpdateCli::try_parse_from([
            "update",
            "--id",
            "fixture/update",
            "--provider",
            "fixture",
            "--producer",
            "01J00000000000000000000001",
            "--kind",
            "skill",
        ])
        .unwrap()
        .input;
        let wire = prepare_update(&input, None, &policy, &request).unwrap();
        let arguments = wire.arguments.unwrap();
        assert_eq!(arguments["selection"]["id"], "fixture/update");
        assert_eq!(arguments["selection"]["version"], "latest");
    }

    #[test]
    fn project_modes_are_typed_and_global_cache_cannot_select_a_physical_mode() {
        let brand = Brand::default();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancellation = Cancellation::default();
        let request = policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        let value = serde_json::json!({
            "selection": {"transport":"broker","provider":"fixture","producer":"01J00000000000000000000001","artifact_type":"skill","id":"fixture/project-mode","version":"release-one","exact":true},
            "global":false
        });
        let mut input: InstallArguments = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(input.mode, None);
        for update in [false, true] {
            let wire = prepare_install(&input, update, None, &policy, &request).unwrap();
            assert!(!wire.arguments.unwrap().contains_key("mode"));
        }
        {
            input.mode = Some(PhysicalMode::Copy);
            for update in [false, true] {
                assert_eq!(
                    prepare_install(&input, update, None, &policy, &request)
                        .unwrap()
                        .arguments
                        .unwrap()["mode"],
                    "copy"
                );
                input.global = true;
                assert!(prepare_install(&input, update, None, &policy, &request).is_err());
                input.global = false;
            }
        }
        for invalid in [
            serde_json::Value::Null,
            serde_json::json!("Copy"),
            serde_json::json!("native"),
            serde_json::json!("link"),
            serde_json::json!(false),
        ] {
            let mut bad = value.clone();
            bad["mode"] = invalid;
            assert!(serde_json::from_value::<InstallArguments>(bad).is_err());
        }
    }
}
