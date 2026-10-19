//! Knowledge retrieval through the existing authenticated Source service.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use graphit_core::{
    native_document_file::NativeDocumentFileLimits, native_path::NativePath,
    project_paths::InstallationPaths,
};
use graphit_hub::policy::RequestContext;
use rmcp::model::CallToolRequestParams;
use serde_json::{Map, Value, json};
use std::{
    io::{self, Write},
    path::PathBuf,
};

#[derive(Clone, Copy, clap::ValueEnum, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Fts,
    Semantic,
    Hybrid,
}

#[derive(Clone, Copy, clap::ValueEnum, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Metric {
    Cosine,
    L2,
    Dot,
}

#[derive(clap::Args)]
pub struct Arguments {
    /// Query the selected Knowledge corpus.
    pub query: String,
    #[arg(long, value_enum, default_value = "fts")]
    pub mode: Mode,
    #[arg(long, value_enum)]
    pub metric: Option<Metric>,
    /// Limit ranked results; zero retains the service's unbounded-result semantics.
    #[arg(long)]
    pub top_k: Option<usize>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
    pub page_size: Option<u32>,
    #[arg(long)]
    pub preview: bool,
    /// Override configured reranking; --rerank=false always disables it.
    #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true,
        action = clap::ArgAction::Set)]
    pub rerank: Option<bool>,
    /// Continue with the exact opaque JSON cursor returned by the service.
    #[arg(long, conflicts_with = "cursor_file", value_parser = crate::cli_cursor::parse)]
    pub cursor: Option<Value>,
    /// Read the unchanged cursor object from a UTF-8 JSON file.
    #[arg(long, conflicts_with = "cursor")]
    pub cursor_file: Option<PathBuf>,
    #[arg(long, conflicts_with_all = ["artifact_id", "artifact_version", "context_alias"])]
    pub project_dir: Option<PathBuf>,
    /// Select an installed Knowledge artifact independently of PROJECT.
    #[arg(long, requires = "artifact_version", conflicts_with = "project_dir")]
    pub artifact_id: Option<String>,
    #[arg(long, requires = "artifact_id", conflicts_with = "project_dir")]
    pub artifact_version: Option<String>,
    #[arg(long, requires = "artifact_id", conflicts_with = "project_dir")]
    pub context_alias: Option<String>,
}

pub fn prepare(
    arguments: &Arguments,
    version: Option<&str>,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    let mut search = Map::from_iter([
        ("query".into(), arguments.query.clone().into()),
        (
            "mode".into(),
            serde_json::to_value(arguments.mode).map_err(io::Error::other)?,
        ),
        ("preview".into(), arguments.preview.into()),
    ]);
    if let Some(rerank) = arguments.rerank {
        search.insert("rerank".into(), rerank.into());
    }
    if let Some(metric) = arguments.metric {
        search.insert(
            "metric".into(),
            serde_json::to_value(metric).map_err(io::Error::other)?,
        );
    }
    if let Some(top_k) = arguments.top_k {
        search.insert("top_k".into(), top_k.into());
    }
    if let Some(page_size) = arguments.page_size {
        search.insert("page_size".into(), page_size.into());
    }
    if arguments.cursor.is_some() && arguments.cursor_file.is_some() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let cursor = match &arguments.cursor_file {
        Some(path) => Some(crate::cli_cursor::read_file(
            path,
            original.content_limits().max_input_bytes,
        )?),
        None => arguments.cursor.clone(),
    };
    if let Some(cursor) = cursor {
        search.insert("cursor".into(), cursor);
    }
    let mut input = Map::from_iter([("search".into(), Value::Object(search))]);
    if let Some(id) = &arguments.artifact_id {
        if arguments.project_dir.is_some() || version.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let artifact_version = arguments
            .artifact_version
            .as_ref()
            .ok_or(io::ErrorKind::InvalidInput)?;
        let mut artifact = json!({"id":id,"version":artifact_version});
        if let Some(alias) = &arguments.context_alias {
            artifact["context_alias"] = alias.clone().into();
        }
        input.insert(
            "source".into(),
            json!({"kind":"imported_knowledge","artifact":artifact}),
        );
    } else {
        if arguments.artifact_version.is_some() || arguments.context_alias.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
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
    }
    Ok(CallToolRequestParams::new("graphit_knowledge_search").with_arguments(input))
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
    let client = reqwest::Client::new();
    let connection =
        crate::ast_source::connect_source(installation, address, &client, native, original, ())
            .await?;
    let deadline = original.snapshot().required_deadline()?;
    crate::mcp_output::json_call(&mut prepared);
    let result = tokio::select! {
        result = connection.peer().call_tool(prepared) => result.map_err(io::Error::other),
        () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(deadline.into()) => Err(io::ErrorKind::TimedOut.into()),
    };
    let joined = connection.cancel().await.map_err(io::Error::other);
    let result = result?;
    joined?;
    original.check()?;
    if result.is_error == Some(true) {
        return Err(io::Error::other(
            serde_json::to_string(&result).map_err(io::Error::other)?,
        ));
    }
    let payload = result.structured_content.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Knowledge response has no result",
        )
    })?;
    serde_json::to_writer(&mut *output, &payload).map_err(io::Error::other)?;
    output.write_all(b"\n")?;
    output.flush()
}
