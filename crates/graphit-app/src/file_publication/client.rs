//! CLI metadata for the same authenticated FILE publisher exposed by Source.
use super::Metadata;
use graphit_core::native_path::NativePath;
use graphit_hub::policy::{HubResourcePolicy, RequestContext};
use rmcp::model::CallToolRequestParams;
use serde_json::json;
use std::{io, path::PathBuf};

#[derive(clap::Args)]
pub struct Arguments {
    #[arg(long, value_enum)]
    pub kind: crate::remote_file_install::client::Kind,
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
    /// Declare one dependency; repeat the three values for each dependency.
    #[arg(long, num_args = 3, value_names = ["KIND", "ID", "SELECTOR"], action = clap::ArgAction::Append, conflicts_with = "clear_dependencies")]
    pub dependency: Vec<String>,
    /// Publish an explicitly empty dependency list; omission preserves declarations.
    #[arg(long, conflicts_with = "dependency")]
    pub clear_dependencies: bool,
    /// Current archive SHA256 required to replace content at this concrete version.
    #[arg(long)]
    pub replace_sha256: Option<String>,
    /// Make this concrete version the catalog's latest pointer.
    #[arg(long)]
    pub set_latest: bool,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
}

impl Arguments {
    fn declared_dependencies(&self) -> io::Result<Option<Vec<super::Dependency>>> {
        if self.clear_dependencies && !self.dependency.is_empty()
            || !self.dependency.len().is_multiple_of(3)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "each --dependency requires KIND ID SELECTOR; --clear-dependencies is exclusive",
            ));
        }
        if self.clear_dependencies {
            return Ok(Some(Vec::new()));
        }
        if self.dependency.is_empty() {
            return Ok(None);
        }
        self.dependency
            .as_chunks::<3>()
            .0
            .iter()
            .map(|parts| {
                let artifact_type = graphit_hub::ArtifactType::ALL
                    .into_iter()
                    .find(|kind| kind.as_str() == parts[0])
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("unknown dependency kind: {}", parts[0]),
                        )
                    })?;
                Ok(super::Dependency {
                    artifact_type,
                    id: parts[1].clone(),
                    selector: parts[2].clone(),
                })
            })
            .collect::<io::Result<Vec<_>>>()
            .map(Some)
    }
}
pub fn prepare(
    input: &Arguments,
    project_version: Option<&str>,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    let dependencies = input.declared_dependencies()?;
    let publication = Metadata {
        provider: input.provider.clone(),
        artifact_type: input.kind,
        id: input.id.clone(),
        version: input.version.clone(),
        name: input.name.clone(),
        description: input.description.clone(),
        tags: input.tag.clone(),
        author: input.author.clone(),
        dependencies,
        expected_sha256: input.replace_sha256.clone(),
        set_latest: input.set_latest,
    };
    publication.validate(policy)?;
    let path = input
        .project_dir
        .as_ref()
        .ok_or(io::ErrorKind::InvalidInput)?;
    let path = NativePath::new(std::fs::canonicalize(path)?)?;
    let mut args = json!({"publication":publication,"project_dir":path});
    if let Some(version) = project_version {
        args["project_version"] = json!(version);
    }
    if serde_json::to_vec(&args).map_err(io::Error::other)?.len()
        > original.content_limits().max_input_bytes
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    original.check()?;
    Ok(CallToolRequestParams::new("graphit_hub_publish_file")
        .with_arguments(args.as_object().unwrap().clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        arguments: Arguments,
    }
    fn parse(extra: &[&str]) -> Arguments {
        Cli::try_parse_from(
            [
                "graphit",
                "--kind",
                "skill",
                "--provider",
                "dev",
                "--id",
                "vendor/example",
                "--version",
                "1",
                "--name",
                "Example",
            ]
            .into_iter()
            .chain(extra.iter().copied()),
        )
        .unwrap()
        .arguments
    }
    #[test]
    fn dependencies_are_repeated_typed_triples_and_clear_is_explicit() {
        assert!(parse(&[]).declared_dependencies().unwrap().is_none());
        assert!(
            parse(&["--clear-dependencies"])
                .declared_dependencies()
                .unwrap()
                .unwrap()
                .is_empty()
        );
        let args = parse(&[
            "--dependency",
            "skill",
            "vendor/a",
            "latest",
            "--dependency",
            "knowledge",
            "vendor/b",
            "release-1",
        ]);
        let deps = args.declared_dependencies().unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(deps).unwrap(),
            json!([
                {"artifact_type":"skill","id":"vendor/a","selector":"latest"},
                {"artifact_type":"knowledge","id":"vendor/b","selector":"release-1"}
            ])
        );
        assert!(
            parse(&["--dependency", "unknown", "x", "latest"])
                .declared_dependencies()
                .is_err()
        );
        let base = [
            "graphit",
            "--kind",
            "skill",
            "--provider",
            "dev",
            "--id",
            "v/a",
            "--version",
            "1",
            "--name",
            "A",
        ];
        for extra in [
            vec!["--dependency", "skill", "v/a"],
            vec!["--dependencies", "[]"],
            vec![
                "--dependency",
                "skill",
                "v/a",
                "latest",
                "--clear-dependencies",
            ],
        ] {
            assert!(Cli::try_parse_from(base.into_iter().chain(extra)).is_err());
        }
    }
}
