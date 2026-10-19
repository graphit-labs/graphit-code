//! Native Hub publication through the existing authenticated daemon connection.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use graphit_core::{
    native_document_file::NativeDocumentFileLimits, native_path::NativePath,
    project_paths::InstallationPaths,
};
use graphit_hub::{
    ArtifactType,
    policy::{HubResourcePolicy, RequestContext},
};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use serde_json::json;
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Kind {
    Ast,
    Knowledge,
}
#[derive(clap::Args)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Command,
}
#[derive(clap::Subcommand)]
pub enum Command {
    Authored(crate::authored_resources::client::Arguments),
    Install(crate::remote_file_install::client::InstallArguments),
    InstallLocal(crate::local_file_install::Arguments),
    Update(crate::remote_file_install::client::UpdateArguments),
    /// Reapply all installed artifacts from their recorded sources and versions.
    Sync(crate::remote_file_install::client::UpdateAllArguments),
    Remove(crate::remote_file_install::lifecycle::Arguments),
    Recover(crate::remote_file_install::lifecycle::Arguments),
    Forget(crate::remote_file_install::lifecycle::Arguments),
    Content(crate::remote_file_install::client::ContentArguments),
    List(crate::remote_file_install::client::ListArguments),
    Projects(crate::remote_hub_catalog::client::ProjectsArguments),
    Search(crate::remote_hub_catalog::client::SearchArguments),
    Publish(crate::file_publication::client::Arguments),
    PublishNative(PublishArguments),
    /// Retract remote catalog references while preserving payloads and local installations.
    Unpublish(UnpublishArguments),
    InstallNative(crate::native_hub_install::InstallArguments),
}
impl Command {
    pub fn global(&self) -> bool {
        use crate::remote_file_install::client::ContentCommand;
        match self {
            Self::InstallNative(_) | Self::Projects(_) | Self::Search(_) | Self::Unpublish(_) => {
                true
            }
            Self::Install(input) => input.global,
            Self::Update(input) => input.target.global,
            Self::Sync(input) => input.global,
            Self::List(input) => input.global,
            Self::Content(input) => match &input.command {
                ContentCommand::Remote(_) => true,
                ContentCommand::Installed(input) => input.global,
            },
            Self::Remove(input) | Self::Recover(input) | Self::Forget(input) => {
                input.scope != crate::remote_file_install::lifecycle::Scope::Project
            }
            Self::Publish(_)
            | Self::PublishNative(_)
            | Self::InstallLocal(_)
            | Self::Authored(_) => false,
        }
    }
    pub fn project_dir(&self) -> Option<&Path> {
        use crate::remote_file_install::client::ContentCommand;
        match self {
            Self::Authored(input) => input.project_dir(),
            Self::Install(input) => input.project_dir.as_deref(),
            Self::Update(input) => input.target.project_dir.as_deref(),
            Self::Sync(input) => input.project_dir.as_deref(),
            Self::List(input) => input.project_dir.as_deref(),
            Self::InstallLocal(input) => Some(&input.project_dir),
            Self::Remove(input) | Self::Recover(input) | Self::Forget(input) => {
                input.project_dir.as_deref()
            }
            Self::Content(input) => match &input.command {
                ContentCommand::Remote(_) => None,
                ContentCommand::Installed(input) => input.project_dir.as_deref(),
            },
            Self::Publish(input) => input.project_dir.as_deref(),
            Self::PublishNative(input) => input.project_dir.as_deref(),
            Self::InstallNative(_) | Self::Projects(_) | Self::Search(_) | Self::Unpublish(_) => {
                None
            }
        }
    }
    pub fn prepare(
        self,
        project: Option<(&Path, Option<&str>)>,
        policy: &HubResourcePolicy,
        original: &RequestContext<'_>,
    ) -> io::Result<CallToolRequestParams> {
        use crate::remote_file_install::client::{self, ContentCommand};
        let version = project.and_then(|(_, version)| version);
        match self {
            Self::Authored(input) => input.prepare(project, policy, original),
            Self::Install(mut input) => {
                if let Some((root, _)) = project {
                    input.project_dir = Some(root.to_owned());
                }
                client::prepare_install(&input, false, version, policy, original)
            }
            Self::Sync(mut input) => {
                if let Some((root, _)) = project {
                    input.project_dir = Some(root.to_owned());
                }
                client::prepare_sync(&input, version, policy, original)
            }
            Self::Update(mut input) => {
                if let Some((root, _)) = project {
                    input.target.project_dir = Some(root.to_owned());
                }
                client::prepare_update(&input, version, policy, original)
            }
            Self::InstallLocal(mut input) => {
                if let Some((root, _)) = project {
                    input.project_dir = root.to_owned();
                }
                crate::local_file_install::prepare(&input, version, policy, original)
            }
            Self::Remove(mut input) => {
                if let Some((root, _)) = project {
                    input.project_dir = Some(root.to_owned());
                }
                crate::remote_file_install::lifecycle::prepare(
                    &input,
                    crate::remote_file_install::lifecycle::Operation::Remove,
                    version,
                    policy,
                    original,
                )
            }
            Self::Recover(mut input) => {
                if let Some((root, _)) = project {
                    input.project_dir = Some(root.to_owned());
                }
                crate::remote_file_install::lifecycle::prepare(
                    &input,
                    crate::remote_file_install::lifecycle::Operation::Recover,
                    version,
                    policy,
                    original,
                )
            }
            Self::Forget(mut input) => {
                if let Some((root, _)) = project {
                    input.project_dir = Some(root.to_owned());
                }
                crate::remote_file_install::lifecycle::prepare(
                    &input,
                    crate::remote_file_install::lifecycle::Operation::Forget,
                    version,
                    policy,
                    original,
                )
            }
            Self::List(mut input) => {
                if let Some((root, _)) = project {
                    input.project_dir = Some(root.to_owned());
                }
                client::prepare_list(&input, version, policy, original)
            }
            Self::Content(mut input) => {
                if let ContentCommand::Installed(input) = &mut input.command
                    && let Some((root, _)) = project
                {
                    input.project_dir = Some(root.to_owned());
                }
                client::prepare_content(&input, version, policy, original)
            }
            Self::Publish(mut input) => {
                if let Some((root, _)) = project {
                    input.project_dir = Some(root.to_owned());
                }
                crate::file_publication::client::prepare(&input, version, policy, original)
            }
            Self::PublishNative(mut input) => {
                if let Some((root, _)) = project {
                    input.project_dir = Some(root.to_owned());
                }
                prepare(&input, version, policy, original)
            }
            Self::Unpublish(input) => {
                if project.is_some() {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                prepare_unpublish(input, policy, original)
            }
            Self::Projects(input) => {
                if project.is_some() {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                crate::remote_hub_catalog::client::prepare_projects(&input, policy, original)
            }
            Self::Search(input) => {
                if project.is_some() {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                crate::remote_hub_catalog::client::prepare_search(&input, policy, original)
            }
            Self::InstallNative(input) => {
                if project.is_some() {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                crate::native_hub_install::prepare(&input, policy, original)
            }
        }
    }
}

#[derive(clap::Args)]
pub struct UnpublishArguments {
    #[arg(long)]
    pub provider: String,
    #[arg(long)]
    pub producer: String,
    #[arg(long, value_parser = ["agent", "ast", "command", "knowledge", "language", "mcp", "power", "rule", "skill", "workflow"])]
    pub kind: String,
    #[arg(long)]
    pub id: String,
    /// Literal artifact version; latest and release-prefix selection are not accepted.
    #[arg(
        long,
        required_unless_present = "all_versions",
        conflicts_with = "all_versions"
    )]
    pub version: Option<String>,
    /// Retract every version observed in the current artifact catalog.
    #[arg(long, conflicts_with = "version")]
    pub all_versions: bool,
    /// Require this current version hash before changing the catalog.
    #[arg(long, requires = "version", conflicts_with = "all_versions")]
    pub expected_sha256: Option<String>,
}
fn prepare_unpublish(
    input: UnpublishArguments,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    use crate::hub_unpublish::{Arguments, Target};
    original.check()?;
    let target = match (input.version, input.all_versions, input.expected_sha256) {
        (Some(version), false, expected_sha256) => Target::Version {
            version,
            expected_sha256,
        },
        (None, true, None) => Target::Artifact {},
        _ => return Err(io::ErrorKind::InvalidInput.into()),
    };
    let arguments = Arguments {
        provider: input.provider,
        producer: input
            .producer
            .parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
        artifact_type: serde_json::from_value(json!(input.kind))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
        id: input.id,
        target,
    };
    arguments.validate(policy)?;
    let arguments = serde_json::to_value(arguments).map_err(io::Error::other)?;
    if serde_json::to_vec(&arguments)
        .map_err(io::Error::other)?
        .len()
        > original.content_limits().max_input_bytes
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    original.check()?;
    Ok(CallToolRequestParams::new("graphit_hub_unpublish")
        .with_arguments(arguments.as_object().expect("serialized arguments").clone()))
}

#[derive(clap::Args)]
pub struct PublishArguments {
    #[arg(long, value_enum)]
    pub kind: Kind,
    #[arg(long)]
    pub provider: String,
    #[arg(long)]
    pub id: String,
    #[arg(long)]
    pub version: String,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub tag: Vec<String>,
    #[arg(long)]
    pub author: Option<String>,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
}
pub fn prepare(
    arguments: &PublishArguments,
    version: Option<&str>,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    let metadata = crate::native_publication::PublicationMetadata {
        provider: arguments.provider.clone(),
        artifact_type: match arguments.kind {
            Kind::Ast => ArtifactType::Ast,
            Kind::Knowledge => ArtifactType::Knowledge,
        },
        id: arguments.id.clone(),
        version: arguments.version.clone(),
        name: arguments.name.clone(),
        description: arguments.description.clone(),
        tags: arguments.tag.clone(),
        author: arguments.author.clone(),
        dependencies: None,
    };
    metadata.validate(crate::native_publication::metadata_limits(
        policy.content(),
        policy.projection(),
    ))?;
    let project = arguments
        .project_dir
        .as_ref()
        .ok_or(io::ErrorKind::InvalidInput)?;
    let path = NativePath::new(std::fs::canonicalize(project)?)?;
    let mut args = json!({"publication":metadata,"project_dir":path});
    if let Some(version) = version {
        args["project_version"] = json!(version);
    }
    if serde_json::to_vec(&args).map_err(io::Error::other)?.len()
        > original.content_limits().max_input_bytes
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    original.check()?;
    Ok(CallToolRequestParams::new("graphit_hub_publish_native")
        .with_arguments(args.as_object().unwrap().clone()))
}
pub async fn call(
    installation: &InstallationPaths,
    mut prepared: CallToolRequestParams,
    native: NativeDocumentFileLimits,
    original: &RequestContext<'_>,
    output: &mut impl Write,
) -> io::Result<()> {
    original.check()?;
    let address = crate::source_entry::address(installation, native, original).await?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(io::Error::other)?;
    let connection =
        crate::ast_source::connect_source(installation, address, &client, native, original, ())
            .await?;
    let catalog_request =
        prepared.name == "graphit_hub_projects" || prepared.name == "graphit_hub_search";
    let lifecycle_request =
        crate::remote_file_install::lifecycle::Operation::from_tool(prepared.name.as_ref())
            .is_some();
    crate::mcp_output::json_call(&mut prepared);
    let result = tokio::select! {
        result=connection.peer().call_tool(prepared)=>result.map_err(io::Error::other),
        ()=original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
        ()=tokio::time::sleep_until(original.snapshot().required_deadline()?.into())=>Err(io::ErrorKind::TimedOut.into()),
    };
    let joined = connection.cancel().await.map_err(io::Error::other);
    let result = result?;
    // Preserve the known Hub operation result even when delivery cleanup or the Original fails.
    let current = original.check();
    if result.is_error == Some(true) || joined.is_err() || current.is_err() {
        let catalog_failure = catalog_request.then(|| {
            crate::remote_hub_catalog::client::delivery_failure(&result, current.as_ref().err())
        });
        let lifecycle_failure = lifecycle_request.then(|| {
            crate::remote_file_install::lifecycle::delivery_failure(&result, current.as_ref().err())
        });
        return Err(io::Error::other(PublicationDeliveryFailure {
            catalog_failure,
            lifecycle_failure,
            result,
            cleanup: joined.err(),
            guard: current.err(),
        }));
    }
    serde_json::to_writer(
        &mut *output,
        &result
            .structured_content
            .ok_or(io::ErrorKind::InvalidData)?,
    )
    .map_err(io::Error::other)?;
    output.write_all(b"\n")?;
    output.flush()
}
#[derive(Debug)]
pub(crate) struct PublicationDeliveryFailure {
    catalog_failure: Option<crate::remote_hub_catalog::client::Failure>,
    lifecycle_failure: Option<crate::remote_file_install::lifecycle::DeliveryFailure>,
    pub(crate) result: CallToolResult,
    cleanup: Option<io::Error>,
    guard: Option<io::Error>,
}
impl std::fmt::Display for PublicationDeliveryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.catalog_failure.is_some() {
            return f.write_str("Hub catalog operation failed");
        }
        write!(
            f,
            "Hub operation delivery failed: {}",
            serde_json::to_string(&self.result).map_err(|_| std::fmt::Error)?
        )?;
        if let Some(cause) = &self.guard {
            write!(f, "; request: {cause}")?;
        }
        if let Some(cause) = &self.cleanup {
            write!(f, "; connection cleanup: {cause}")?;
        }
        Ok(())
    }
}
impl std::error::Error for PublicationDeliveryFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Some(cause) = &self.catalog_failure {
            return Some(cause);
        }
        if let Some(cause) = &self.lifecycle_failure {
            return Some(cause);
        }
        self.guard
            .as_ref()
            .or(self.cleanup.as_ref())
            .map(|e| e as _)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};
    use std::time::Instant;
    #[test]
    fn publication_preflight_keeps_literal_artifact_version_and_native_project_selection() {
        let brand = Brand::default();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel).unwrap();
        let project = tempfile::tempdir().unwrap();
        let mut args = PublishArguments {
            kind: Kind::Knowledge,
            provider: "non-default".into(),
            id: "namespace/ação".into(),
            version: "release/版本".into(),
            name: "Knowledge".into(),
            description: None,
            tag: vec![],
            author: None,
            project_dir: Some(project.path().to_owned()),
        };
        let request = prepare(&args, Some("feature/project"), &policy, &original).unwrap();
        let request = request.arguments.unwrap();
        assert_eq!(request["project_version"], "feature/project");
        assert_eq!(request["publication"]["version"], "release/版本");
        assert_eq!(request["publication"]["provider"], "non-default");
        assert_eq!(
            serde_json::from_value::<NativePath>(request["project_dir"].clone())
                .unwrap()
                .as_path(),
            std::fs::canonicalize(project.path()).unwrap()
        );
        args.version = "latest".into();
        assert!(prepare(&args, None, &policy, &original).is_err());
        args.version = "release/1".into();
        args.name = "bad/name".into();
        assert!(prepare(&args, None, &policy, &original).is_err());
    }

    #[derive(clap::Parser)]
    struct HubCli {
        #[command(subcommand)]
        command: Command,
    }
    #[test]
    fn unpublish_cli_requires_an_explicit_exclusive_remote_target() {
        use clap::Parser;
        let base = [
            "hub",
            "unpublish",
            "--provider",
            "broker",
            "--producer",
            "01KSH1CRFFG8Z74B5ZS78WW808",
            "--kind",
            "skill",
            "--id",
            "team/tool",
        ];
        assert!(HubCli::try_parse_from(base).is_err());
        let mut args = base.to_vec();
        args.extend(["--version", "release/1", "--all-versions"]);
        assert!(HubCli::try_parse_from(args).is_err());
        let mut args = base.to_vec();
        args.extend(["--all-versions", "--expected-sha256", "bad"]);
        assert!(HubCli::try_parse_from(args).is_err());
        let mut args = base.to_vec();
        args.extend(["--all-versions", "--project-dir", "/unused"]);
        assert!(HubCli::try_parse_from(args).is_err());
    }
    #[test]
    fn unpublish_preflight_is_global_and_preserves_literal_version_and_hash() {
        use clap::Parser;
        let brand = Brand::default();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel).unwrap();
        let digest = "ab".repeat(32);
        let parsed = HubCli::try_parse_from([
            "hub",
            "unpublish",
            "--provider",
            "non-default",
            "--producer",
            "01KSH1CRFFG8Z74B5ZS78WW808",
            "--kind",
            "ast",
            "--id",
            "team/ação",
            "--version",
            "release/版本",
            "--expected-sha256",
            &digest,
        ])
        .unwrap();
        assert!(parsed.command.global());
        assert!(parsed.command.project_dir().is_none());
        let prepared = parsed.command.prepare(None, &policy, &original).unwrap();
        assert_eq!(prepared.name, "graphit_hub_unpublish");
        let body = prepared.arguments.unwrap();
        assert_eq!(body["target"]["version"], "release/版本");
        assert_eq!(body["target"]["expected_sha256"], digest);
        assert_eq!(body["provider"], "non-default");
        assert_eq!(body["artifact_type"], "ast");
        assert!(!body.contains_key("project_dir"));
        assert!(!body.contains_key("project_version"));
        let parsed = HubCli::try_parse_from([
            "hub",
            "unpublish",
            "--provider",
            "broker",
            "--producer",
            "01KSH1CRFFG8Z74B5ZS78WW808",
            "--kind",
            "skill",
            "--id",
            "team/tool",
            "--all-versions",
        ])
        .unwrap();
        let body = parsed
            .command
            .prepare(None, &policy, &original)
            .unwrap()
            .arguments
            .unwrap();
        assert_eq!(body["target"], json!({"kind":"artifact"}));
    }
    #[test]
    fn unpublish_preflight_rejects_latest_and_local_project_context() {
        let brand = Brand::default();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel).unwrap();
        let arguments = || UnpublishArguments {
            provider: "broker".into(),
            producer: "01KSH1CRFFG8Z74B5ZS78WW808".into(),
            kind: "skill".into(),
            id: "team/tool".into(),
            version: Some("latest".into()),
            all_versions: false,
            expected_sha256: None,
        };
        assert!(
            Command::Unpublish(arguments())
                .prepare(None, &policy, &original)
                .is_err()
        );
        assert!(
            Command::Unpublish(arguments())
                .prepare(Some((Path::new("/unused"), None)), &policy, &original)
                .is_err()
        );
    }
}
