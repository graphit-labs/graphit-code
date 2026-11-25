//! Explicit operational hints and observations of the daemon's existing supervisor.
use crate::task_entry::CallArguments;
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::io;
#[derive(Args)]
#[command(after_help = "Examples:
  graphit index status
  graphit index run --module knowledge --wait-ms 5000

A request may be coalesced with existing project work. Ctrl+C stops waiting only;
shared indexing continues. Use status to inspect its outcome.")]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Command,
}
#[derive(Args)]
#[command(
    after_help = "Ctrl+C stops waiting only; shared indexing continues. A timeout exits 3.
Use graphit index status to inspect the later outcome."
)]
pub struct Run {
    #[command(flatten)]
    selection: CallArguments,
    /// Wait up to this many milliseconds; 0 returns after acceptance. Timeout exits 3.
    #[arg(long, value_name = "MILLISECONDS", default_value = "0")]
    wait_ms: u64,
    /// Request this module; the coalesced supervisor processes the whole project.
    #[arg(long, value_enum)]
    module: Option<Module>,
}
#[derive(Clone, Copy, clap::ValueEnum, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Module {
    Ast,
    Knowledge,
}
#[derive(Subcommand)]
pub enum Command {
    /// Show index availability, processing phases, last completion, and errors.
    Status(CallArguments),
    /// Request a project rescan; acceptance does not mean indexing is complete.
    Run(Run),
}
impl Command {
    pub fn selection_mut(&mut self) -> &mut CallArguments {
        match self {
            Self::Status(v) => v,
            Self::Run(v) => &mut v.selection,
        }
    }
    pub fn prepare(&mut self) -> io::Result<(&'static str, Value)> {
        let (operation, input) = match self {
            Self::Status(_) => ("index_status", json!({})),
            Self::Run(value) => (
                "index_run",
                json!({"wait_ms":value.wait_ms,"module":value.module}),
            ),
        };
        let selection = self.selection_mut();
        let mut selected = json!({});
        if let Some(dir) = &selection.project_dir {
            selected["project_dir"] =
                serde_json::to_value(graphit_core::native_path::NativePath::new(dir.clone())?)?;
        }
        if let Some(id) = &selection.project_id {
            selected["project_id"] = json!(id);
        }
        if let Some(version) = &selection.project_version {
            selected["project_version"] = json!(version);
        }
        Ok((operation, json!({"selection":selected,"input":input})))
    }
}
