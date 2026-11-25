//! Typed catalogue and lint commands; native data stays JSON on the wire.
use crate::task_entry::CallArguments;
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    num::{NonZeroU64, NonZeroUsize},
    process::ExitCode,
};
#[derive(Args)]
pub struct List {
    #[command(flatten)]
    selection: CallArguments,
    /// Keep documents with this metadata kind.
    #[arg(long)]
    kind: Option<String>,
    /// Keep documents whose source path starts with this prefix.
    #[arg(long)]
    source_prefix: Option<String>,
    /// Filter stale status; use --stale=false for current documents.
    #[arg(long,num_args=0..=1,default_missing_value="true",require_equals=true,action=clap::ArgAction::Set)]
    stale: Option<bool>,
    /// Filter missing required metadata; use --metadata-errors=false to exclude it.
    #[arg(long,num_args=0..=1,default_missing_value="true",require_equals=true,action=clap::ArgAction::Set)]
    metadata_errors: Option<bool>,
    /// Maximum documents in this page.
    #[arg(long, default_value = "25")]
    page_size: NonZeroUsize,
    /// Exact JSON continuation object from the previous page; quote it as one argument.
    #[arg(long,value_parser=crate::cli_cursor::parse)]
    cursor: Option<Value>,
}
#[derive(Args)]
pub struct Lint {
    #[command(flatten)]
    selection: CallArguments,
    /// Report persisted stale markers plus pages older than this explicit age.
    #[arg(long, value_name = "DAYS")]
    max_age_days: Option<NonZeroU64>,
}
#[derive(Subcommand)]
pub enum Command {
    /// List published documents with metadata filters and pagination.
    List(List),
    /// Validate the complete published corpus; exit 1 for issues, 3 for no corpus.
    Lint(Lint),
}
impl Command {
    pub fn selection_mut(&mut self) -> &mut CallArguments {
        match self {
            Self::List(v) => &mut v.selection,
            Self::Lint(v) => &mut v.selection,
        }
    }
    pub fn prepare(&mut self) -> io::Result<(&'static str, Value)> {
        let (op, input) = match self {
            Self::List(v) => {
                let mut filter = json!({});
                for (key, value) in [
                    ("kind", v.kind.as_ref().map(|s| json!(s))),
                    ("source_prefix", v.source_prefix.as_ref().map(|s| json!(s))),
                    ("stale", v.stale.map(|b| json!(b))),
                    ("metadata_errors", v.metadata_errors.map(|b| json!(b))),
                ] {
                    if let Some(value) = value {
                        filter[key] = value;
                    }
                }
                let mut input = json!({"filter":filter,"page_size":v.page_size});
                if let Some(cursor) = &v.cursor {
                    input["cursor"] = cursor.clone();
                }
                ("knowledge_list", input)
            }
            Self::Lint(v) => {
                let mut input = json!({});
                if let Some(days) = v.max_age_days {
                    input["max_age_days"] = json!(days);
                }
                ("knowledge_lint", input)
            }
        };
        let selection = self.selection_mut();
        let mut selected = json!({});
        if let Some(path) = &selection.project_dir {
            selected["project_dir"] =
                serde_json::to_value(graphit_core::native_path::NativePath::new(path.clone())?)?;
        }
        if let Some(id) = &selection.project_id {
            selected["project_id"] = json!(id);
        }
        if let Some(version) = &selection.project_version {
            selected["project_version"] = json!(version);
        }
        Ok((op, json!({"selection":selected,"input":input})))
    }
}
pub async fn run(
    installation: &graphit_core::project_paths::InstallationPaths,
    operation: &str,
    value: Value,
    original: &graphit_hub::policy::RequestContext<'_>,
    output: &mut impl Write,
) -> io::Result<ExitCode> {
    let mut bytes = Vec::new();
    crate::memory_entry::call_native(installation, operation, value, original, &mut bytes).await?;
    let value: Value = serde_json::from_slice(&bytes)?;
    output.write_all(&bytes)?;
    Ok(if operation != "knowledge_lint" {
        ExitCode::SUCCESS
    } else if value["state"] == "no_corpus" {
        ExitCode::from(3)
    } else if value["issue_count"].as_u64().unwrap_or(0) > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}
