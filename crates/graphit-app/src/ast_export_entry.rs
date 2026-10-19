//! Publish a native AST package through the authenticated Source MCP connection.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use graphit_core::{
    native_document_file::NativeDocumentFileLimits, native_path::NativePath,
    project_paths::InstallationPaths,
};
use graphit_hub::policy::RequestContext;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use std::{
    io::{self, Write},
    path::PathBuf,
};

#[derive(clap::Args)]
pub struct Arguments {
    /// Destination package file, resolved from this invocation's working directory.
    #[arg(long)]
    pub output: PathBuf,
    /// Select a retained local project; otherwise use the current selection.
    #[arg(long, conflicts_with = "context")]
    pub project_dir: Option<PathBuf>,
    /// Repackage an exact installed AST, independently of PROJECT selection.
    #[arg(long, conflicts_with = "project_dir")]
    pub context: Option<String>,
}
pub fn prepare(
    arguments: &Arguments,
    version: Option<&str>,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    if arguments.context.is_some() && (arguments.project_dir.is_some() || version.is_some()) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let destination = if arguments.output.is_absolute() {
        arguments.output.clone()
    } else {
        std::env::current_dir()?.join(&arguments.output)
    };
    let mut input = serde_json::Map::from_iter([(
        "output".into(),
        serde_json::to_value(NativePath::new(destination)?).map_err(io::Error::other)?,
    )]);
    if let Some(context) = &arguments.context {
        input.insert("context".into(), context.clone().into());
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
    Ok(CallToolRequestParams::new("graphit_ast_export").with_arguments(input))
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
        result=connection.peer().call_tool(prepared)=>result.map_err(io::Error::other),
        ()=original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
        ()=tokio::time::sleep_until(deadline.into())=>Err(io::ErrorKind::TimedOut.into()),
    };
    let joined = connection.cancel().await.map_err(io::Error::other);
    let result = result?;
    joined?;
    if result.is_error == Some(true) {
        return Err(io::Error::other(ExportFailure(result)));
    }
    original.check()?;
    let payload = result.structured_content.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "AST export response has no result",
        )
    })?;
    serde_json::to_writer(&mut *output, &payload).map_err(io::Error::other)?;
    output.write_all(b"\n")?;
    output.flush()
}
struct ExportFailure(CallToolResult);
impl std::fmt::Debug for ExportFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AST export failed")
    }
}
impl std::fmt::Display for ExportFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for content in &self.0.content {
            if let Some(text) = content.as_text() {
                f.write_str(&text.text)?;
            }
        }
        Ok(())
    }
}
impl std::error::Error for ExportFailure {}
