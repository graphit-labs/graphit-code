//! Explicit Memory index maintenance through the authenticated Source service.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]

use graphit_core::{
    native_document_file::NativeDocumentFileLimits, native_path::NativePath,
    project_paths::InstallationPaths,
};
use graphit_hub::policy::RequestContext;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use serde_json::{Map, Value};
use std::{
    io::{self, Write},
    path::PathBuf,
};

#[derive(
    Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    #[default]
    Project,
    User,
}

#[derive(clap::Args)]
pub struct Arguments {
    /// Select the authorized PROJECT or the caller's own verified USER corpus.
    #[arg(long, value_enum, default_value = "project")]
    pub scope: Scope,
    /// Replace the existing lexical index explicitly; never triggered by a read.
    #[arg(long)]
    pub refresh: bool,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
}

pub fn prepare(
    arguments: &Arguments,
    version: Option<&str>,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    if arguments.scope == Scope::User && (arguments.project_dir.is_some() || version.is_some()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "USER Memory does not accept PROJECT selectors",
        ));
    }
    if version.is_some_and(|value| value.trim().is_empty() || value.contains('\0')) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut input = Map::from_iter([
        (
            "scope".into(),
            serde_json::to_value(arguments.scope).map_err(io::Error::other)?,
        ),
        ("refresh".into(), Value::Bool(arguments.refresh)),
    ]);
    if let Some(root) = &arguments.project_dir {
        input.insert(
            "project_dir".into(),
            serde_json::to_value(NativePath::new(std::fs::canonicalize(root)?)?)
                .map_err(io::Error::other)?,
        );
    }
    if let Some(version) = version {
        input.insert("project_version".into(), version.into());
    }
    original.check()?;
    Ok(CallToolRequestParams::new("graphit_memory_index").with_arguments(input))
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
    let joined = connection
        .cancel()
        .await
        .map(|_| ())
        .map_err(io::Error::other);
    deliver(result, joined, original, output)
}

fn deliver(
    result: io::Result<CallToolResult>,
    joined: io::Result<()>,
    original: &RequestContext<'_>,
    output: &mut impl Write,
) -> io::Result<()> {
    let result = result?;
    let completion = joined.err();
    let request = original.check().err();
    if let Some(cause) = completion.as_ref().or(request.as_ref()) {
        return Err(io::Error::new(
            cause.kind(),
            IndexFailure {
                result,
                completion,
                request,
            },
        ));
    }
    if result.is_error == Some(true) {
        return Err(io::Error::other(IndexFailure {
            result,
            completion: None,
            request: None,
        }));
    }
    let payload = result.structured_content.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Memory index response has no result",
        )
    })?;
    serde_json::to_writer(&mut *output, &payload).map_err(io::Error::other)?;
    output.write_all(b"\n")?;
    output.flush()
}

#[derive(Debug)]
struct IndexFailure {
    result: CallToolResult,
    completion: Option<io::Error>,
    request: Option<io::Error>,
}

impl std::fmt::Display for IndexFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = if self.completion.is_none() && self.request.is_none() {
            serde_json::to_string(&self.result)
        } else {
            #[derive(serde::Serialize)]
            struct Delivery<'a> {
                code: &'static str,
                result: &'a CallToolResult,
                #[serde(skip_serializing_if = "Option::is_none")]
                connection_error: Option<String>,
                #[serde(skip_serializing_if = "Option::is_none")]
                request_error: Option<String>,
            }
            serde_json::to_string(&Delivery {
                code: "delivery_failed",
                result: &self.result,
                connection_error: self.completion.as_ref().map(ToString::to_string),
                request_error: self.request.as_ref().map(ToString::to_string),
            })
        };
        f.write_str(&value.map_err(|_| std::fmt::Error)?)
    }
}

impl std::error::Error for IndexFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.completion
            .as_ref()
            .or(self.request.as_ref())
            .map(|cause| cause as &(dyn std::error::Error + 'static))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};
    use graphit_hub::policy::HubResourcePolicy;
    use std::{
        error::Error,
        time::{Duration, Instant},
    };

    // Called by the existing PROJECT consumer after its actual SDK Refresh.
    pub(crate) fn assert_publication_delivery(result: &CallToolResult) {
        let expected = serde_json::to_value(result).unwrap();
        let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap().content();
        let cancel = Cancellation::default();
        let request =
            RequestContext::new(policy, &cancel, Instant::now() + Duration::from_secs(10)).unwrap();
        let mut output = Vec::new();
        deliver(Ok(result.clone()), Ok(()), &request, &mut output).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&output).unwrap(),
            *result.structured_content.as_ref().unwrap()
        );

        cancel.cancel();
        output.clear();
        let error = deliver(Ok(result.clone()), Ok(()), &request, &mut output).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(output.is_empty());
        let payload: Value = serde_json::from_str(&error.to_string()).unwrap();
        assert_eq!(payload["result"], expected);
        assert!(payload["request_error"].is_string());
        let failure = error
            .get_ref()
            .unwrap()
            .downcast_ref::<IndexFailure>()
            .unwrap();
        assert_eq!(
            failure.request.as_ref().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(
            failure
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .is_some()
        );

        // Keep both causes, while retaining the existing cleanup-error priority.
        let error = deliver(
            Ok(result.clone()),
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "connection cleanup failed",
            )),
            &request,
            &mut output,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        let payload: Value = serde_json::from_str(&error.to_string()).unwrap();
        assert_eq!(payload["result"], expected);
        assert_eq!(payload["connection_error"], "connection cleanup failed");
        assert!(payload["request_error"].is_string());
        let failure = error
            .get_ref()
            .unwrap()
            .downcast_ref::<IndexFailure>()
            .unwrap();
        assert_eq!(
            failure
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
        assert!(output.is_empty());

        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_millis(25);
        let request = RequestContext::new(policy, &cancel, deadline).unwrap();
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        let error = deliver(Ok(result.clone()), Ok(()), &request, &mut output).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        let payload: Value = serde_json::from_str(&error.to_string()).unwrap();
        assert_eq!(payload["result"], expected);
        assert!(output.is_empty());

        // No observed response means there is no result or receipt to report.
        let error = deliver(
            Err(io::ErrorKind::Interrupted.into()),
            Ok(()),
            &request,
            &mut output,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(error.get_ref().is_none());
        assert!(output.is_empty());
    }
}
