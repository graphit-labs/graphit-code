//! Knowledge export through the authenticated Source connection.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
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

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    serde::Deserialize,
    serde::Serialize,
    clap::ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    #[default]
    Native,
    Markdown,
    Okf,
    Obsidian,
}

#[derive(clap::Args)]
pub struct Arguments {
    /// Export an installable native package or a Markdown, OKF or Obsidian directory.
    #[arg(long, value_enum, default_value = "native")]
    pub format: Format,
    /// Destination package file or text directory, relative to this working directory.
    #[arg(long)]
    pub output: PathBuf,
    /// Select a retained local project; otherwise use the current selection.
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

/// Validate and capture the existing export payload before startup side effects.
pub fn prepare(
    arguments: &Arguments,
    version: Option<&str>,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.snapshot().check()?;
    let destination = if arguments.output.is_absolute() {
        arguments.output.clone()
    } else {
        std::env::current_dir()?.join(&arguments.output)
    };
    let mut input = serde_json::Map::from_iter([
        (
            "output".into(),
            serde_json::to_value(NativePath::new(destination)?).map_err(io::Error::other)?,
        ),
        (
            "format".into(),
            serde_json::to_value(arguments.format).map_err(io::Error::other)?,
        ),
    ]);
    if let Some(id) = &arguments.artifact_id {
        if arguments.project_dir.is_some() || version.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let artifact_version = arguments
            .artifact_version
            .as_ref()
            .ok_or(io::ErrorKind::InvalidInput)?;
        if std::iter::once(id)
            .chain(std::iter::once(artifact_version))
            .chain(arguments.context_alias.iter())
            .any(|value| value.is_empty() || value.contains('\0'))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut artifact = serde_json::json!({"id":id,"version":artifact_version});
        if let Some(alias) = &arguments.context_alias {
            artifact["context_alias"] = alias.clone().into();
        }
        input.insert(
            "source".into(),
            serde_json::json!({"kind":"imported_knowledge","artifact":artifact}),
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
            input.insert(
                "project_version".into(),
                serde_json::Value::String(version.to_owned()),
            );
        }
    }
    original.snapshot().check()?;
    Ok(CallToolRequestParams::new("graphit_knowledge_export").with_arguments(input))
}

/// Consume the exact prepared payload through the authenticated Source connection.
pub async fn call(
    installation: &InstallationPaths,
    mut prepared: CallToolRequestParams,
    native: NativeDocumentFileLimits,
    original: &RequestContext<'_>,
    output: &mut impl Write,
) -> io::Result<()> {
    original.snapshot().check()?;
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
    if let Err(cause) = joined.and_then(|_| original.snapshot().check()) {
        return Err(io::Error::new(
            cause.kind(),
            ExportFailure {
                result,
                completion: Some(cause),
            },
        ));
    }
    if result.is_error == Some(true) {
        return Err(io::Error::other(ExportFailure {
            result,
            completion: None,
        }));
    }
    let payload = result.structured_content.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Knowledge export response has no result",
        )
    })?;
    serde_json::to_writer(&mut *output, &payload).map_err(io::Error::other)?;
    output.write_all(b"\n")?;
    output.flush()
}

struct ExportFailure {
    result: CallToolResult,
    completion: Option<io::Error>,
}
impl std::fmt::Debug for ExportFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Knowledge export failed")
    }
}
impl std::fmt::Display for ExportFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for content in &self.result.content {
            if let Some(text) = content.as_text() {
                f.write_str(&text.text)?;
            }
        }
        if let Some(cause) = &self.completion {
            write!(f, "; Source connection completion failed: {cause}")?;
        }
        Ok(())
    }
}
impl std::error::Error for ExportFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.completion
            .as_ref()
            .map(|cause| cause as &(dyn std::error::Error + 'static))
    }
}
