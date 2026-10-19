//! Typed final-only CLI consumer of the existing authenticated Source connection.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use graphit_core::{
    native_document_file::NativeDocumentFileLimits, project_paths::InstallationPaths,
};
use graphit_hub::policy::RequestContext;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use serde::Serialize;
use std::{
    io::{self, Write},
    num::NonZeroUsize,
};

#[derive(clap::Args)]
pub struct Arguments {
    /// Literal question, without trimming or shell interpretation.
    #[arg(long)]
    pub query: String,
    /// Explicit Knowledge page slug; repeat to select more pages.
    #[arg(long = "slug", required_unless_present = "retrieval", conflicts_with_all = ["retrieval", "top_k", "metric"])]
    pub selected_slugs: Vec<String>,
    /// Retrieve pages from the question instead of selecting explicit slugs.
    #[arg(
        long,
        value_enum,
        requires = "top_k",
        conflicts_with = "selected_slugs"
    )]
    pub retrieval: Option<RetrievalMode>,
    /// Number of ranked pages to provide; requires automatic retrieval.
    #[arg(long, requires = "retrieval")]
    pub top_k: Option<NonZeroUsize>,
    /// Vector metric for semantic or hybrid retrieval; defaults to cosine.
    #[arg(long, value_enum, requires = "retrieval")]
    pub metric: Option<RetrievalMetric>,
    /// Requested model identifier passed literally to the captured completion binding.
    #[arg(long)]
    pub model: String,
    /// Select a retained local project rather than the current project.
    #[arg(long)]
    pub project_dir: Option<String>,
}
#[derive(Clone, Copy, clap::ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RetrievalMode {
    Fts,
    Semantic,
    Hybrid,
}
#[derive(Clone, Copy, clap::ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RetrievalMetric {
    Cosine,
    L2,
    Dot,
}
#[derive(Serialize)]
struct RetrievalInput {
    mode: RetrievalMode,
    top_k: NonZeroUsize,
    #[serde(skip_serializing_if = "Option::is_none")]
    metric: Option<RetrievalMetric>,
}
#[derive(Serialize)]
struct Input<'a> {
    query: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_slugs: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retrieval: Option<RetrievalInput>,
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_dir: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_version: Option<&'a str>,
}

pub fn prepare(
    arguments: &Arguments,
    version: Option<&str>,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    let retrieval = match arguments.retrieval {
        Some(mode) => {
            if !arguments.selected_slugs.is_empty()
                || matches!(mode, RetrievalMode::Fts) && arguments.metric.is_some()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "retrieval excludes --slug; --metric requires semantic or hybrid",
                ));
            }
            Some(RetrievalInput {
                mode,
                top_k: arguments.top_k.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "retrieval requires --top-k")
                })?,
                metric: arguments.metric,
            })
        }
        None => {
            if arguments.top_k.is_some() || arguments.metric.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--top-k and --metric require --retrieval",
                ));
            }
            None
        }
    };
    let serde_json::Value::Object(input) = serde_json::to_value(Input {
        query: &arguments.query,
        selected_slugs: retrieval
            .is_none()
            .then_some(arguments.selected_slugs.as_slice()),
        retrieval,
        model: &arguments.model,
        project_dir: arguments.project_dir.as_deref(),
        project_version: version,
    })
    .map_err(io::Error::other)?
    else {
        unreachable!("typed input object")
    };
    Ok(CallToolRequestParams::new("graphit_knowledge_ask").with_arguments(input))
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
    // Preserve the operation's first error if shutdown also fails.
    let result = result?;
    joined?;
    original.snapshot().check()?;
    if result.is_error == Some(true) {
        return Err(io::Error::other(RemoteAskFailure(result)));
    }
    let payload = result.structured_content.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Knowledge Ask response has no structured final result",
        )
    })?;
    if let Some(answer) = payload
        .pointer("/receipt/completion/answer")
        .and_then(serde_json::Value::as_str)
    {
        output.write_all(answer.as_bytes())?;
    } else {
        serde_json::to_writer(&mut *output, &payload).map_err(io::Error::other)?;
    }
    output.write_all(b"\n")?;
    output.flush()
}
struct RemoteAskFailure(CallToolResult);
impl std::fmt::Debug for RemoteAskFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Knowledge Ask service failure")
    }
}
impl std::fmt::Display for RemoteAskFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for content in &self.0.content {
            if let Some(text) = content.as_text() {
                formatter.write_str(&text.text)?;
            }
        }
        Ok(())
    }
}
impl std::error::Error for RemoteAskFailure {}
