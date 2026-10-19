//! CLI selection for authored PROJECT Resources; publication is a separate command.
use graphit_core::native_path::NativePath;
use graphit_hub::policy::{HubResourcePolicy, RequestContext};
use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(clap::Args)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Command,
}
#[derive(clap::Subcommand)]
pub enum Command {
    /// Enumerate authored resource identities in the selected PROJECT/version.
    List(List),
    /// Review files in one explicitly selected authored resource tree.
    Review(Review),
    /// Read a text file relative to one explicitly selected authored resource tree.
    Content(Content),
}
#[derive(clap::Args)]
pub struct List {
    #[arg(long, value_parser = ["agent", "ast", "command", "knowledge", "language", "mcp", "power", "rule", "skill", "workflow"])]
    pub kind: Option<String>,
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long)]
    pub version: Option<String>,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long)]
    pub cursor: Option<String>,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
}
#[derive(clap::Args)]
pub struct Review {
    #[arg(long, value_parser = ["agent", "ast", "command", "knowledge", "language", "mcp", "power", "rule", "skill", "workflow"])]
    pub kind: String,
    #[arg(long)]
    pub id: String,
    #[arg(long)]
    pub version: String,
    #[arg(long, default_value_t = 0)]
    pub offset: usize,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
}
#[derive(clap::Args)]
pub struct Content {
    #[arg(long, value_parser = ["agent", "ast", "command", "knowledge", "language", "mcp", "power", "rule", "skill", "workflow"])]
    pub kind: String,
    #[arg(long)]
    pub id: String,
    #[arg(long)]
    pub version: String,
    #[arg(long)]
    pub path: String,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
}
impl Arguments {
    pub fn project_dir(&self) -> Option<&Path> {
        match &self.command {
            Command::List(input) => input.project_dir.as_deref(),
            Command::Review(input) => input.project_dir.as_deref(),
            Command::Content(input) => input.project_dir.as_deref(),
        }
    }
    pub fn prepare(
        &self,
        project: Option<(&Path, Option<&str>)>,
        policy: &HubResourcePolicy,
        original: &RequestContext<'_>,
    ) -> io::Result<CallToolRequestParams> {
        original.check()?;
        let (root, version) = project.ok_or(io::ErrorKind::InvalidInput)?;
        let (tool, query) = match &self.command {
            Command::List(input) => {
                if input.limit == 0 || input.limit > policy.content().max_collection_entries {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                let mut query = json!({"limit":input.limit});
                for (field, value) in [
                    ("artifact_type", &input.kind),
                    ("id", &input.id),
                    ("version", &input.version),
                    ("cursor", &input.cursor),
                ] {
                    if let Some(value) = value {
                        query[field] = json!(value);
                    }
                }
                (super::LIST, query)
            }
            Command::Review(input) => {
                if input.limit == 0
                    || input.limit > policy.content().max_collection_entries
                    || input.offset.checked_add(input.limit).is_none()
                {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                (
                    super::REVIEW,
                    json!({"artifact_type":input.kind,"id":input.id,
                    "version":input.version,"offset":input.offset,"limit":input.limit}),
                )
            }
            Command::Content(input) => (
                super::CONTENT,
                json!({"artifact_type":input.kind,
                "id":input.id,"version":input.version,"path":input.path}),
            ),
        };
        let mut arguments =
            json!({"project_dir":NativePath::new(std::fs::canonicalize(root)?)?,"query":query});
        if let Some(version) = version {
            arguments["project_version"] = json!(version);
        }
        if serde_json::to_vec(&arguments)
            .map_err(io::Error::other)?
            .len()
            > original.content_limits().max_input_bytes
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        original.check()?;
        Ok(
            CallToolRequestParams::new(tool).with_arguments(match arguments {
                Value::Object(arguments) => arguments,
                _ => unreachable!("object literal"),
            }),
        )
    }
}
