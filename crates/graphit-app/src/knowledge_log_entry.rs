//! Prepare the Knowledge log CLI call for the existing authenticated Source transport.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use graphit_core::native_path::NativePath;
use graphit_hub::policy::RequestContext;
use rmcp::model::CallToolRequestParams;
use serde_json::{Map, Value};
use std::{io, num::NonZeroUsize, path::PathBuf};

#[derive(clap::Args)]
pub struct Arguments {
    /// Maximum synchronization records in this page.
    #[arg(long, default_value = "10")]
    pub limit: NonZeroUsize,
    /// Continue with the opaque cursor returned by the service.
    #[arg(long)]
    pub cursor: Option<String>,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
}
pub fn prepare(
    arguments: &Arguments,
    version: Option<&str>,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    let mut history = Map::from_iter([("limit".into(), Value::from(arguments.limit.get()))]);
    if let Some(cursor) = &arguments.cursor {
        history.insert("cursor".into(), cursor.clone().into());
    }
    let mut input = Map::from_iter([("history".into(), Value::Object(history))]);
    if let Some(project) = &arguments.project_dir {
        input.insert(
            "project_dir".into(),
            serde_json::to_value(NativePath::new(std::fs::canonicalize(project)?)?)
                .map_err(io::Error::other)?,
        );
    }
    if let Some(version) = version {
        input.insert("project_version".into(), version.into());
    }
    Ok(CallToolRequestParams::new("graphit_wiki_log").with_arguments(input))
}
