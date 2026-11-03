//! Explicit local lifecycle targets; removal never reads remote payload storage.
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, NativeBinding,
    policy::{HubResourcePolicy, RequestContext},
};
use rmcp::model::CallToolRequestParams;
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};

#[derive(Clone, Copy)]
pub(crate) enum Operation {
    Remove,
    Recover,
    Forget,
}
impl Operation {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Remove => "remove",
            Self::Recover => "recover",
            Self::Forget => "forget",
        }
    }
    pub(crate) const fn tool(self) -> &'static str {
        match self {
            Self::Remove => "graphit_hub_remove",
            Self::Recover => "graphit_hub_recover",
            Self::Forget => "graphit_hub_forget",
        }
    }
    pub(crate) fn from_tool(name: &str) -> Option<Self> {
        match name {
            "graphit_hub_remove" => Some(Self::Remove),
            "graphit_hub_recover" => Some(Self::Recover),
            "graphit_hub_forget" => Some(Self::Forget),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Project,
    GlobalCache,
    GlobalNative,
}
fn parse_kind(value: &str) -> Result<ArtifactType, String> {
    ArtifactType::ALL
        .into_iter()
        .find(|kind| kind.as_str() == value)
        .ok_or_else(|| "unsupported artifact type".into())
}

#[derive(clap::Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Arguments {
    #[arg(long, value_parser = parse_kind)]
    #[serde(rename = "artifact_type")]
    pub kind: ArtifactType,
    #[arg(long, value_enum, default_value = "project")]
    pub scope: Scope,
    #[arg(long)]
    pub id: String,
    /// Optional exact installed artifact version; never a release selector.
    #[arg(long)]
    pub version: Option<String>,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    #[arg(skip)]
    pub project_version: Option<String>,
    #[arg(long)]
    pub binding: Option<NativeBinding>,
}
impl Arguments {
    pub(crate) fn validate(
        &self,
        policy: &HubResourcePolicy,
        operation: Operation,
    ) -> io::Result<()> {
        match self.scope {
            Scope::Project if matches!(self.kind, ArtifactType::Ast | ArtifactType::Knowledge) => {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            Scope::GlobalCache | Scope::GlobalNative => {
                if !matches!(operation, Operation::Remove)
                    || self.version.is_none()
                    || self.project_dir.is_some()
                    || self.project_version.is_some()
                    || self.binding.is_some()
                    || match self.scope {
                        Scope::GlobalCache => {
                            matches!(self.kind, ArtifactType::Ast | ArtifactType::Knowledge)
                        }
                        Scope::GlobalNative => !matches!(
                            self.kind,
                            ArtifactType::Ast | ArtifactType::Knowledge | ArtifactType::Language
                        ),
                        Scope::Project => false,
                    }
                {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
            }
            Scope::Project => {}
        }
        ArtifactId::new(&self.id, policy.projection())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        if let Some(version) = &self.version {
            ConcreteVersion::new(version, policy.projection())
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        }
        if self
            .project_dir
            .as_ref()
            .is_some_and(|root| !root.is_absolute())
            || self.project_version.as_ref().is_some_and(|version| {
                version.is_empty()
                    || version.trim() != version
                    || version.len() > policy.content().max_scalar_bytes
                    || version.chars().any(char::is_control)
            })
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }
}
pub(crate) fn prepare(
    arguments: &Arguments,
    operation: Operation,
    project_version: Option<&str>,
    policy: &HubResourcePolicy,
    request: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    request.check()?;
    arguments.validate(policy, operation).map_err(|error| {
        io::Error::new(
            error.kind(),
            DeliveryFailure {
                code: "invalid_arguments",
                exit: 2,
            },
        )
    })?;
    let mut value = serde_json::to_value(arguments).map_err(io::Error::other)?;
    if let Some(version) = project_version {
        value["project_version"] = version.into();
    }
    if serde_json::to_vec(&value).map_err(io::Error::other)?.len()
        > request.content_limits().max_input_bytes
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            DeliveryFailure {
                code: "invalid_arguments",
                exit: 2,
            },
        ));
    }
    request.check()?;
    Ok(CallToolRequestParams::new(operation.tool()).with_arguments(
        value
            .as_object()
            .ok_or(io::ErrorKind::InvalidInput)?
            .clone(),
    ))
}

#[derive(Debug)]
pub(crate) struct DeliveryFailure {
    code: &'static str,
    exit: u8,
}
impl std::fmt::Display for DeliveryFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Hub lifecycle failed: {}", self.code)
    }
}
impl std::error::Error for DeliveryFailure {}
pub(crate) fn delivery_failure(
    result: &rmcp::model::CallToolResult,
    guard: Option<&io::Error>,
) -> DeliveryFailure {
    if guard.is_some_and(|error| error.kind() == io::ErrorKind::Interrupted) {
        return DeliveryFailure {
            code: "request_cancelled",
            exit: 130,
        };
    }
    let code = result
        .structured_content
        .as_ref()
        .and_then(|value| value.get("code"))
        .and_then(serde_json::Value::as_str);
    let (code, exit) = match code {
        Some("invalid_arguments") => ("invalid_arguments", 2),
        Some("authentication_or_permission_denied") => ("authentication_or_permission_denied", 1),
        Some("request_cancelled_or_expired") => ("request_cancelled_or_expired", 1),
        Some("installation_conflict") => ("installation_conflict", 1),
        Some("artifact_not_found") => ("artifact_not_found", 1),
        _ => ("file_lifecycle_failed", 1),
    };
    DeliveryFailure { code, exit }
}
pub fn cli_exit_code(error: &(dyn std::error::Error + 'static)) -> Option<u8> {
    let mut error = Some(error);
    while let Some(current) = error {
        if let Some(failure) = current.downcast_ref::<DeliveryFailure>() {
            return Some(failure.exit);
        }
        error = current
            .downcast_ref::<io::Error>()
            .and_then(io::Error::get_ref)
            .map(|error| error as &(dyn std::error::Error + 'static))
            .or_else(|| current.source());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};
    use std::time::Instant;

    #[test]
    fn lifecycle_selection_is_local_strict_and_preserves_exact_versions() {
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
        let input = serde_json::json!({"artifact_type":"skill","scope":"project","id":"org/ação","version":"release/版本","binding":"codex"});
        let mut arguments: Arguments = serde_json::from_value(input.clone()).unwrap();
        let prepared = prepare(
            &arguments,
            Operation::Forget,
            Some("feature/branch"),
            &policy,
            &request,
        )
        .unwrap();
        assert_eq!(prepared.name, "graphit_hub_forget");
        let wire = prepared.arguments.unwrap();
        assert_eq!(wire["project_version"], "feature/branch");
        assert_eq!(wire["version"], "release/版本");
        for field in ["global", "provider", "producer", "alias"] {
            let mut invalid = input.clone();
            invalid[field] = true.into();
            assert!(serde_json::from_value::<Arguments>(invalid).is_err());
        }
        arguments.version = Some("latest".into());
        let literal = prepare(&arguments, Operation::Remove, None, &policy, &request).unwrap();
        assert_eq!(
            literal.arguments.unwrap()["version"],
            "latest",
            "lifecycle never resolves a release selector"
        );
        arguments.version = Some(String::new());
        assert!(arguments.validate(&policy, Operation::Forget).is_err());
        arguments.version = None;
        arguments.project_dir = Some("relative".into());
        assert!(arguments.validate(&policy, Operation::Forget).is_err());
        cancellation.cancel();
        assert!(prepare(&arguments, Operation::Remove, None, &policy, &request).is_err());
    }

    #[test]
    fn global_removal_has_explicit_target_exact_version_and_no_project_projection() {
        use clap::Parser;
        #[derive(Parser)]
        struct Cli {
            #[command(flatten)]
            input: Arguments,
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
        for (scope, kind) in [("global-cache", "skill"), ("global-native", "ast")] {
            let cli = Cli::try_parse_from([
                "hub",
                "--scope",
                scope,
                "--kind",
                kind,
                "--id",
                "fixture/global",
                "--version",
                "release/one",
            ])
            .unwrap();
            let prepared = prepare(&cli.input, Operation::Remove, None, &policy, &request).unwrap();
            let wire = prepared.arguments.unwrap();
            assert_eq!(
                wire["scope"],
                if kind == "ast" {
                    "global_native"
                } else {
                    "global_cache"
                }
            );
            assert_eq!(wire["version"], "release/one");
            for operation in [Operation::Recover, Operation::Forget] {
                assert!(prepare(&cli.input, operation, None, &policy, &request).is_err());
            }
            let mut value = serde_json::to_value(&cli.input).unwrap();
            for (field, invalid) in [
                ("version", serde_json::Value::Null),
                ("project_dir", serde_json::json!("/project")),
                ("project_version", serde_json::json!("main")),
                ("binding", serde_json::json!("codex")),
            ] {
                let mut bad = value.clone();
                bad[field] = invalid;
                assert!(
                    serde_json::from_value::<Arguments>(bad)
                        .unwrap()
                        .validate(&policy, Operation::Remove)
                        .is_err()
                );
            }
            value["artifact_type"] = if kind == "ast" {
                serde_json::json!("skill")
            } else {
                serde_json::json!("ast")
            };
            assert!(
                serde_json::from_value::<Arguments>(value)
                    .unwrap()
                    .validate(&policy, Operation::Remove)
                    .is_err()
            );
        }
        assert!(
            serde_json::from_value::<Arguments>(
                serde_json::json!({"artifact_type":"skill","id":"fixture/global","version":"one"})
            )
            .is_err(),
            "missing scope is never an implicit global"
        );
    }

    #[test]
    fn hub_instruction_examples_match_current_typed_consumers() {
        use crate::remote_file_install::client::{
            ContentArguments, ContentCommand, InstallArguments, InstalledContentArguments,
            prepare_content, prepare_install,
        };
        use crate::remote_hub_catalog::client::{
            ProjectsArguments, SearchArguments, prepare_projects, prepare_search,
        };
        use graphit_core::module_settings::Module;
        use graphit_runtime::instructions::{
            InstructionLimits, InstructionValue, InstructionVariable, render_instruction,
        };
        for key in ["graphit", "orbit"] {
            let brand = Brand::new(key).unwrap();
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
            let templates =
                graphit_runtime::skill_instructions::skill_templates(Module::Hub).unwrap();
            let mut values = std::collections::BTreeMap::from([
                ("tool_transport".to_owned(), "root".to_owned()),
                ("brand".to_owned(), brand.key().to_owned()),
                ("brand_display".to_owned(), brand.key().to_owned()),
                (
                    "brand_marker".to_owned(),
                    brand.environment_prefix().to_owned(),
                ),
            ]);
            for candidate in templates.candidates() {
                for (offset, _) in candidate.markdown.match_indices("mcp_tool_") {
                    let key = candidate.markdown[offset..]
                        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                        .next()
                        .unwrap();
                    let suffix = key.strip_prefix("mcp_tool_").unwrap();
                    values.insert(
                        key.to_owned(),
                        brand
                            .mcp_tool_name(&suffix.split('_').collect::<Vec<_>>())
                            .unwrap(),
                    );
                }
            }
            let variables = values
                .iter()
                .map(|(name, value)| InstructionVariable {
                    name,
                    value: InstructionValue::Text(value),
                })
                .collect::<Vec<_>>();
            let content = policy.content();
            let nz = |value| std::num::NonZeroUsize::new(value).unwrap();
            let limits = InstructionLimits {
                max_templates: nz(content.max_collection_entries),
                max_template_bytes: nz(content.max_input_bytes),
                max_values: nz(content.max_nodes),
                max_variable_bytes: nz(content.max_input_bytes),
                max_output_bytes: nz(content.max_output_bytes),
                fuel: std::num::NonZeroU64::new(
                    u64::try_from(content.max_nodes)
                        .unwrap()
                        .checked_mul(16)
                        .unwrap(),
                )
                .unwrap(),
                recursion: nz(content.max_depth.min(500)),
            };
            let rendered = std::iter::once(templates.body_entry())
                .chain(
                    templates
                        .references()
                        .iter()
                        .map(|reference| reference.entry),
                )
                .map(|entry| {
                    let rendered = render_instruction(
                        entry,
                        templates.candidates(),
                        &variables,
                        limits,
                        request.snapshot(),
                    )
                    .unwrap();
                    (entry, rendered.into_body())
                })
                .collect::<Vec<_>>();
            let markdown = &rendered
                .iter()
                .find(|(entry, _)| *entry == "skills/hub/references/discovery-cases.md")
                .unwrap()
                .1;
            let examples: Vec<serde_json::Value> = markdown
                .split("~~~json\n")
                .skip(1)
                .map(|block| serde_json::from_str(block.split_once("\n~~~").unwrap().0).unwrap())
                .collect();
            assert_eq!(
                examples.len(),
                12,
                "every maintained Hub JSON example needs a typed consumer"
            );
            let projects: ProjectsArguments = serde_json::from_value(examples[0].clone()).unwrap();
            prepare_projects(&projects, &policy, &request).unwrap();
            for example in &examples[1..3] {
                let search: SearchArguments = serde_json::from_value(example.clone()).unwrap();
                prepare_search(&search, &policy, &request).unwrap();
            }
            let native: crate::native_hub_install::Selection =
                serde_json::from_value(examples[3]["installation"].clone()).unwrap();
            native.validate(&policy).unwrap();
            let unpublish: crate::hub_unpublish::Arguments =
                serde_json::from_value(examples[4].clone()).unwrap();
            unpublish.validate(&policy).unwrap();
            let install: InstallArguments = serde_json::from_value(examples[5].clone()).unwrap();
            prepare_install(&install, false, None, &policy, &request).unwrap();
            let installed: InstalledContentArguments =
                serde_json::from_value(examples[6]["installed"].clone()).unwrap();
            prepare_content(
                &ContentArguments {
                    command: ContentCommand::Installed(installed),
                },
                None,
                &policy,
                &request,
            )
            .unwrap();
            for (example, operation) in examples[7..10].iter().zip([
                Operation::Remove,
                Operation::Recover,
                Operation::Forget,
            ]) {
                let input: Arguments = serde_json::from_value(example.clone()).unwrap();
                assert_eq!(
                    prepare(&input, operation, None, &policy, &request)
                        .unwrap()
                        .name,
                    operation.tool()
                );
            }
            for example in &examples[10..] {
                let input: Arguments = serde_json::from_value(example.clone()).unwrap();
                prepare(&input, Operation::Remove, None, &policy, &request).unwrap();
            }
            for (entry, markdown) in &rendered {
                assert!(
                    !markdown.contains("mcp_tool_"),
                    "{key} {entry} has unresolved tool names"
                );
                for obsolete in ["show", "uninstall", "submit", "link", "unlink", "type_path"] {
                    let mut segments = vec!["hub"];
                    segments.extend(obsolete.split('_'));
                    let obsolete = brand.mcp_tool_name(&segments).unwrap();
                    assert!(
                        !markdown.contains(&obsolete),
                        "{key} {entry} still teaches {obsolete}"
                    );
                }
            }
        }
    }
}
