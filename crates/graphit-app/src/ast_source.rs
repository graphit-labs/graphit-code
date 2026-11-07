//! Native CLI arguments and the authenticated client for the live Source service.
use clap::Args;
use futures_util::StreamExt;
use graphit_core::{
    file_io::SnapshotContext,
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
    project_paths::InstallationPaths,
};
use graphit_hub::policy::RequestContext;
use reqwest::header::{AUTHORIZATION, HeaderName, HeaderValue};
use rmcp::{
    model::{CallToolRequestParams, CallToolResult, ClientJsonRpcMessage, ProtocolVersion},
    service::{ClientLifecycleMode, ClientServiceExt},
    transport::{
        StreamableHttpClientTransport,
        common::client_side_sse::BoxedSseResponse,
        streamable_http_client::{
            StreamableHttpClient, StreamableHttpClientTransportConfig, StreamableHttpError,
            StreamableHttpPostResponse,
        },
    },
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::HashMap,
    ffi::OsStr,
    io::{self, Write},
    net::SocketAddr,
    sync::Arc,
};
use zeroize::Zeroizing;

/// Bind SDK HTTP I/O, including startup and response streams, to the caller's Original.
#[derive(Clone)]
struct SourceHttpClient {
    client: reqwest::Client,
    original: Arc<SnapshotContext>,
    deadline: tokio::time::Instant,
}
impl SourceHttpClient {
    async fn request<T>(
        &self,
        request: impl Future<Output = Result<T, StreamableHttpError<reqwest::Error>>>,
    ) -> Result<T, StreamableHttpError<reqwest::Error>> {
        self.original.check()?;
        let result = tokio::select! {
            result = request => result,
            () = self.original.cancellation().cancelled() => Err(io::Error::from(io::ErrorKind::Interrupted).into()),
            () = tokio::time::sleep_until(self.deadline) => Err(io::Error::from(io::ErrorKind::TimedOut).into()),
        };
        self.original.check()?;
        result
    }
    fn stream(&self, stream: BoxedSseResponse) -> BoxedSseResponse {
        let original = Arc::clone(&self.original);
        let deadline = self.deadline;
        Box::pin(stream.take_until(async move {
            tokio::select! {
                () = original.cancellation().cancelled() => (),
                () = tokio::time::sleep_until(deadline) => (),
            }
        }))
    }
    fn response(&self, response: StreamableHttpPostResponse) -> StreamableHttpPostResponse {
        match response {
            StreamableHttpPostResponse::Sse(stream, session) => {
                StreamableHttpPostResponse::Sse(self.stream(stream), session)
            }
            other => other,
        }
    }
}
impl StreamableHttpClient for SourceHttpClient {
    type Error = reqwest::Error;
    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        self.request(
            self.client
                .post_message(uri, message, session_id, auth_header, headers),
        )
        .await
        .map(|response| self.response(response))
    }
    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
        cap: usize,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        self.request(self.client.post_message_with_max_sse_event_size(
            uri,
            message,
            session_id,
            auth_header,
            headers,
            cap,
        ))
        .await
        .map(|response| self.response(response))
    }
    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxedSseResponse, StreamableHttpError<Self::Error>> {
        self.request(
            self.client
                .get_stream(uri, session_id, last_event_id, auth_header, headers),
        )
        .await
        .map(|stream| self.stream(stream))
    }
    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
        cap: usize,
    ) -> Result<BoxedSseResponse, StreamableHttpError<Self::Error>> {
        self.request(self.client.get_stream_with_max_sse_event_size(
            uri,
            session_id,
            last_event_id,
            auth_header,
            headers,
            cap,
        ))
        .await
        .map(|stream| self.stream(stream))
    }
    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        self.request(
            self.client
                .delete_session(uri, session_id, auth_header, headers),
        )
        .await
    }
}

/// Wire data only; the service selects its own retained project and authorizes the call.
#[derive(Clone, Debug, Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceParameters {
    pub path: String,
    #[arg(long)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub entity: Option<String>,
    /// Exact primary-key value, requiring entity_type and excluding entity name.
    #[arg(long, conflicts_with = "entity", requires = "entity_type", allow_hyphen_values = true, value_parser = cli_entity_key)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub entity_key: Option<crate::graph_server::GraphKey>,
    /// Type of --entity-key (default: string); int64 must be explicit.
    #[arg(long, value_enum, requires = "entity_key")]
    #[serde(skip)]
    pub entity_key_type: Option<crate::graph_entry::KeyType>,
    #[arg(long)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub entity_type: Option<String>,
    #[arg(long, allow_hyphen_values = true)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub head: Option<i64>,
    #[arg(long, allow_hyphen_values = true)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tail: Option<i64>,
    #[arg(long = "start", allow_hyphen_values = true)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub start_line: Option<i64>,
    #[arg(long = "end", allow_hyphen_values = true)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub end_line: Option<i64>,
    #[arg(long)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub pattern: Option<String>,
    #[arg(long)]
    #[serde(default)]
    pub regex: bool,
    #[arg(long, allow_hyphen_values = true)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub before: Option<i64>,
    #[arg(long, allow_hyphen_values = true)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub after: Option<i64>,
    #[arg(long)]
    #[serde(default)]
    pub line_numbers: bool,
}

/// Native CLI/MCP target selection plus the shared Source selectors.
#[derive(Clone, Debug, Args, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceArguments {
    #[command(flatten)]
    #[serde(flatten)]
    pub source: SourceParameters,
    #[arg(long)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub context: Option<String>,
    #[arg(long)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub project_dir: Option<String>,
    #[arg(skip)]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub project_version: Option<String>,
}

fn cli_entity_key(value: &str) -> Result<crate::graph_server::GraphKey, String> {
    Ok(crate::graph_server::GraphKey::String(value.to_owned()))
}

#[cfg(test)]
mod key_tests {
    use super::*;
    use clap::Parser;
    use serde_json::json;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        source: SourceParameters,
    }

    #[test]
    fn source_cli_keys_use_literal_values_and_explicit_types() {
        for (flags, expected) in [
            (
                vec!["--entity-key", "001"],
                json!({"type":"string","value":"001"}),
            ),
            (
                vec![
                    "--entity-key",
                    "9007199254740993",
                    "--entity-key-type",
                    "int64",
                ],
                json!({"type":"int64","value":"9007199254740993"}),
            ),
            (
                vec![
                    "--entity-key",
                    "-9223372036854775808",
                    "--entity-key-type",
                    "int64",
                ],
                json!({"type":"int64","value":"-9223372036854775808"}),
            ),
            (
                vec!["--entity-key", "{\"type\":\"int64\",\"value\":\"1\"}"],
                json!({"type":"string","value":"{\"type\":\"int64\",\"value\":\"1\"}"}),
            ),
        ] {
            let cli = Cli::try_parse_from(
                ["source", "main.rs", "--entity-type", "Function"]
                    .into_iter()
                    .chain(flags),
            )
            .unwrap();
            cli.source.validate().unwrap();
            assert_eq!(
                serde_json::to_value(cli.source.wire_key().unwrap()).unwrap(),
                expected
            );
        }
        assert!(Cli::try_parse_from(["source", "main.rs", "--entity-key", "x"]).is_err());
        assert!(
            Cli::try_parse_from([
                "source",
                "main.rs",
                "--entity-type",
                "Function",
                "--entity",
                "name",
                "--entity-key",
                "x"
            ])
            .is_err()
        );
    }

    #[test]
    fn source_wire_rejects_invalid_key_selection_without_coercion() {
        for value in [
            json!({"path":"main.rs","entity_key":{"type":"string","value":"x"}}),
            json!({"path":"main.rs","entity_type":"Function","entity":"name","entity_key":{"type":"string","value":"x"}}),
            json!({"path":"main.rs","entity_type":"Function","entity_key":{"type":"int64","value":"9223372036854775808"}}),
        ] {
            let input: SourceParameters = serde_json::from_value(value).unwrap();
            assert!(input.validate().is_err());
        }
        for key in [
            json!(null),
            json!({"type":"int64","value":9007199254740993i64}),
            json!({"type":"string","value":"x","offset":0}),
        ] {
            assert!(
                serde_json::from_value::<SourceParameters>(
                    json!({"path":"main.rs","entity_type":"Function","entity_key":key})
                )
                .is_err()
            );
        }
    }
}

impl SourceParameters {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let path = std::path::Path::new(&self.path);
        if self.path.is_empty()
            || self.path.contains('\0')
            || path.is_absolute()
            || path
                .components()
                .any(|p| !matches!(p, std::path::Component::Normal(_)))
        {
            return Err("Source path must be a nonempty relative file path");
        }
        if self
            .entity
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.contains('\0'))
            || self
                .entity_type
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.contains('\0'))
            || self.entity_type.is_some() && !self.has_entity()
        {
            return Err("Source entity name or type invalid");
        }
        if self.entity.is_some() && self.entity_key.is_some() {
            return Err("Source entity name and primary key are incompatible");
        }
        if self.entity_key_type.is_some() && self.entity_key.is_none() {
            return Err("Source entity key type requires a primary key");
        }
        if self.entity_key.is_some() {
            if self.entity_type.is_none() {
                return Err("Source primary key requires entity_type");
            }
            if self.native_key().is_err() {
                return Err("Source INT64 primary key is invalid");
            }
        }
        Ok(())
    }
    pub(crate) fn native_key(
        &self,
    ) -> Result<Option<graphit_data::GraphAnchorKey<'_>>, &'static str> {
        use crate::{graph_entry::KeyType, graph_server::GraphKey};
        let Some(key) = &self.entity_key else {
            return Ok(None);
        };
        let value = match key {
            GraphKey::String(value) | GraphKey::Int64(value) => value,
        };
        let int64 = match self.entity_key_type {
            Some(KeyType::String) => false,
            Some(KeyType::Int64) => true,
            None => matches!(key, GraphKey::Int64(_)),
        };
        if int64 {
            value
                .parse()
                .map(graphit_data::GraphAnchorKey::Int64)
                .map(Some)
                .map_err(|_| "Source INT64 primary key is invalid")
        } else {
            Ok(Some(graphit_data::GraphAnchorKey::String(value)))
        }
    }
    fn wire_key(&self) -> Result<Option<crate::graph_server::GraphKey>, &'static str> {
        Ok(self.native_key()?.map(|key| match key {
            graphit_data::GraphAnchorKey::String(value) => {
                crate::graph_server::GraphKey::String(value.to_owned())
            }
            graphit_data::GraphAnchorKey::Int64(value) => {
                crate::graph_server::GraphKey::Int64(value.to_string())
            }
        }))
    }
    pub(crate) fn resolve_live_entity(
        &self,
        graph: &graphit_ast::live_graph::LiveGraph,
        original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
        options: graphit_ast::icebug_query::QueryOptions,
    ) -> Result<
        graphit_ast::project_entity_source::ProjectEntitySource,
        graphit_ast::imported_graph::EntitySourceError,
    > {
        use graphit_ast::imported_graph::EntitySourceError as E;
        self.validate().map_err(|_| E::InvalidRequest)?;
        if let Some(key) = self.native_key().map_err(|_| E::InvalidRequest)? {
            graphit_ast::project_entity_source::resolve_live_project_entity_source_by_key(
                graph,
                original,
                &self.path,
                self.entity_type.as_deref().ok_or(E::InvalidRequest)?,
                key,
                options.max_rows.min(256),
            )
        } else {
            graphit_ast::project_entity_source::resolve_live_project_entity_source(
                graph,
                original,
                &self.path,
                self.entity.as_deref().ok_or(E::InvalidRequest)?,
                self.entity_type.as_deref(),
                options,
            )
        }
    }
    pub(crate) fn resolve_project_entity(
        &self,
        graph: &std::sync::Arc<graphit_data::CapturedLocalGraph>,
        original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
        runtime: &tokio::runtime::Handle,
        options: graphit_ast::icebug_query::QueryOptions,
    ) -> Result<
        graphit_ast::project_entity_source::ProjectEntitySource,
        graphit_ast::imported_graph::EntitySourceError,
    > {
        use graphit_ast::imported_graph::EntitySourceError as E;
        self.validate().map_err(|_| E::InvalidRequest)?;
        if let Some(key) = self.native_key().map_err(|_| E::InvalidRequest)? {
            graphit_ast::project_entity_source::resolve_project_entity_source_by_key(
                graph,
                original,
                &self.path,
                self.entity_type.as_deref().ok_or(E::InvalidRequest)?,
                key,
                options.max_rows.min(256),
            )
        } else {
            graphit_ast::project_entity_source::resolve_project_entity_source(
                graph,
                original,
                runtime,
                &self.path,
                self.entity.as_deref().ok_or(E::InvalidRequest)?,
                self.entity_type.as_deref(),
                options,
            )
        }
    }
    pub(crate) fn resolve_imported_window(
        &self,
        graph: &std::sync::Arc<graphit_data::CapturedLocalGraph>,
        original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
        runtime: &tokio::runtime::Handle,
        options: graphit_ast::icebug_query::QueryOptions,
    ) -> Result<(usize, usize), graphit_ast::imported_graph::EntitySourceError> {
        use graphit_ast::imported_graph::EntitySourceError as E;
        self.validate().map_err(|_| E::InvalidRequest)?;
        if let Some(key) = self.native_key().map_err(|_| E::InvalidRequest)? {
            graphit_ast::imported_graph::resolve_imported_entity_window_by_key(
                graph,
                original,
                &self.path,
                self.entity_type.as_deref().ok_or(E::InvalidRequest)?,
                key,
                options.max_rows.min(256),
            )
        } else {
            graphit_ast::imported_graph::resolve_imported_entity_window(
                graph,
                original,
                runtime,
                &self.path,
                self.entity.as_deref().ok_or(E::InvalidRequest)?,
                self.entity_type.as_deref(),
                options,
            )
        }
    }
    pub(crate) fn resolve_remote_window(
        &self,
        graph: &std::sync::Arc<graphit_data::remote_graph::RemoteIcebugGraph>,
        original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
        runtime: &tokio::runtime::Handle,
        options: graphit_ast::icebug_query::QueryOptions,
    ) -> Result<(usize, usize), graphit_ast::imported_graph::EntitySourceError> {
        use graphit_ast::imported_graph::EntitySourceError as E;
        self.validate().map_err(|_| E::InvalidRequest)?;
        if let Some(key) = self.native_key().map_err(|_| E::InvalidRequest)? {
            graphit_ast::imported_graph::resolve_remote_entity_window_by_key(
                graph,
                original,
                runtime,
                &self.path,
                self.entity_type.as_deref().ok_or(E::InvalidRequest)?,
                key,
                options.max_rows.min(256),
            )
        } else {
            graphit_ast::imported_graph::resolve_remote_entity_window(
                graph,
                original,
                runtime,
                &self.path,
                self.entity.as_deref().ok_or(E::InvalidRequest)?,
                self.entity_type.as_deref(),
                options,
            )
        }
    }
    pub(crate) fn has_entity(&self) -> bool {
        self.entity.is_some() || self.entity_key.is_some()
    }
    pub(crate) fn selection(&self) -> graphit_core::text_slice::Request {
        graphit_core::text_slice::Request {
            head: self.head.unwrap_or_default(),
            tail: self.tail.unwrap_or_default(),
            start_line: self.start_line.unwrap_or_default(),
            end_line: self.end_line.unwrap_or_default(),
            pattern: self.pattern.clone().unwrap_or_default(),
            is_regex: self.regex,
            before: self.before.unwrap_or_default(),
            after: self.after.unwrap_or_default(),
            line_numbers: self.line_numbers,
        }
    }
}

fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    value: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(value).map(Some)
}

/// Read the live private key without creating directories, keys or issuer owners.
pub async fn runtime_headers(
    installation: &InstallationPaths,
    limits: NativeDocumentFileLimits,
    original: &RequestContext<'_>,
) -> io::Result<HashMap<HeaderName, HeaderValue>> {
    original.snapshot().check()?;
    let directory = installation.global_path(&["daemon"])?;
    let snapshot = original.snapshot().clone();
    let headers = tokio::task::spawn_blocking(move || {
        let captured = CapturedDocumentDirectory::capture(&directory, limits, &snapshot)?;
        let observation = captured.read(OsStr::new("mcp.key"), &snapshot)?;
        let bytes = observation
            .bytes()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        if bytes.len() != 64
            || !bytes
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid private runtime key",
            ));
        }
        let mut bearer = Zeroizing::new(Vec::with_capacity(71));
        bearer.extend_from_slice(b"Bearer ");
        bearer.extend_from_slice(bytes);
        let mut authorization = HeaderValue::from_bytes(&bearer).map_err(io::Error::other)?;
        authorization.set_sensitive(true);
        captured.revalidate_observation(&observation, &snapshot)?;
        snapshot.check()?;
        Ok(HashMap::from([
            (AUTHORIZATION, authorization),
            (
                HeaderName::from_static("x-graphit-auth-origin"),
                HeaderValue::from_static("runtime"),
            ),
        ]))
    })
    .await
    .map_err(io::Error::other)??;
    original.snapshot().check()?;
    Ok(headers)
}

/// A private SDK connection per operation; the reqwest transport pool is shared.
pub(crate) async fn connect_source<H: rmcp::ClientHandler>(
    installation: &InstallationPaths,
    address: SocketAddr,
    client: &reqwest::Client,
    limits: NativeDocumentFileLimits,
    original: &RequestContext<'_>,
    handler: H,
) -> io::Result<rmcp::service::RunningService<rmcp::RoleClient, H>> {
    if !address.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source service must be loopback",
        ));
    }
    let deadline = tokio::time::Instant::from_std(original.snapshot().required_deadline()?);
    let headers = runtime_headers(installation, limits, original).await?;
    let config = StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"))
        .custom_headers(headers);
    let transport = StreamableHttpClientTransport::with_client(
        SourceHttpClient {
            client: client.clone(),
            original: Arc::clone(original.snapshot()),
            deadline,
        },
        config,
    );

    let connected = tokio::select! {
        result=handler.serve_with_lifecycle(transport,ClientLifecycleMode::Discover{preferred_versions:vec![ProtocolVersion::LATEST]})=>result.map_err(io::Error::other),
        ()=original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
        ()=tokio::time::sleep_until(deadline)=>Err(io::ErrorKind::TimedOut.into()),
    };
    match connected {
        Ok(connection) => {
            if let Err(error) = original.check() {
                let _ = connection.cancel().await;
                Err(error)
            } else {
                Ok(connection)
            }
        }
        Err(error) => {
            original.check()?;
            Err(error)
        }
    }
}

pub async fn call_source_result(
    installation: &InstallationPaths,
    address: SocketAddr,
    client: &reqwest::Client,
    arguments: &SourceArguments,
    limits: NativeDocumentFileLimits,
    original: &RequestContext<'_>,
) -> io::Result<CallToolResult> {
    arguments
        .source
        .validate()
        .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
    let deadline = tokio::time::Instant::from_std(original.snapshot().required_deadline()?);
    let connection = connect_source(installation, address, client, limits, original, ()).await?;

    let result=async {
        original.snapshot().check()?;
        let serde_json::Value::Object(mut wire)=serde_json::to_value(arguments).map_err(io::Error::other)? else {unreachable!("Source struct serializes as an object")};
        if let Some(key) = arguments.source.wire_key().map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))? {
            wire.insert("entity_key".into(), serde_json::to_value(key).map_err(io::Error::other)?);
        }
        tokio::select! {
            result=connection.peer().call_tool(CallToolRequestParams::new("graphit_ast_source").with_arguments(wire))=>result.map_err(io::Error::other),
            ()=original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
            ()=tokio::time::sleep_until(deadline)=>Err(io::ErrorKind::TimedOut.into()),
        }
    }.await;
    connection.cancel().await.map_err(io::Error::other)?;
    original.snapshot().check()?;
    result
}

pub async fn run_source(
    installation: &InstallationPaths,
    address: SocketAddr,
    client: &reqwest::Client,
    arguments: &SourceArguments,
    limits: NativeDocumentFileLimits,
    original: &RequestContext<'_>,
    output: &mut impl Write,
) -> io::Result<()> {
    let result =
        call_source_result(installation, address, client, arguments, limits, original).await?;
    if result.is_error == Some(true) {
        return Err(io::Error::other(RemoteSourceFailure(result)));
    }
    original.snapshot().check()?;
    for content in result.content {
        let text = content.as_text().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Source returned non-text content",
            )
        })?;
        original.snapshot().check()?;
        output.write_all(text.text.as_bytes())?;
    }
    Ok(())
}

#[derive(Debug)]
struct RemoteSourceFailure(CallToolResult);
impl std::fmt::Display for RemoteSourceFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for content in &self.0.content {
            if let Some(text) = content.as_text() {
                formatter.write_str(&text.text)?;
            }
        }
        Ok(())
    }
}
impl std::error::Error for RemoteSourceFailure {}
pub fn source_exit_code(error: &io::Error) -> i32 {
    if error.kind() == io::ErrorKind::Interrupted {
        130
    } else {
        1
    }
}
