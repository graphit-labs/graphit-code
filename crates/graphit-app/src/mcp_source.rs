//! SDK stdio proxy for the authenticated Source service catalog.
mod failure;
mod knowledge_ask;
mod stdio;
use crate::ast_source::{SourceArguments, call_source_result, connect_source};
use failure::ForwardFailure;
use graphit_core::{
    file_lock::Cancellation, native_document_file::NativeDocumentFileLimits,
    project_paths::InstallationPaths,
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorCode,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext as Incoming,
};
use std::{
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
pub use stdio::stdout as stdio_stdout;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Mutex, oneshot},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

struct State {
    installation: InstallationPaths,
    client: reqwest::Client,
    key_limits: NativeDocumentFileLimits,
    content: ContentLimits,
    timeout: Duration,
    current: Option<(PathBuf, Option<String>)>,
    task_version: Option<String>,
    tool: Tool,
    catalog: Vec<Tool>,
    stop: CancellationToken,
    jobs: Mutex<JoinSet<()>>,
}
/// Transport pools and schema are shared; credentials and cancellation belong to each call.
#[derive(Clone)]
pub struct SourceProxy(Arc<State>);
impl SourceProxy {
    pub async fn new(
        installation: InstallationPaths,
        address: SocketAddr,
        client: reqwest::Client,
        policy: (NativeDocumentFileLimits, ContentLimits, Duration),
        current: Option<(&Path, Option<&str>)>,
        original: &RequestContext<'_>,
    ) -> io::Result<Self> {
        // These launcher inputs only deny Root access; RoleProxy authenticates Roles.
        if [
            "GRAPHIT_ROLE_BEARER",
            "GRAPHIT_ROLE_REFERENCE",
            "GRAPHIT_ROLE_SELECTION",
        ]
        .iter()
        .any(|key| std::env::var_os(key).is_some())
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Root MCP is unavailable in a managed-role transport context",
            ));
        }
        let (key_limits, content, timeout) = policy;
        let deadline = original.snapshot().required_deadline()?;
        if timeout.is_zero() || Instant::now().checked_add(timeout).is_none() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let current = current
            .map(|(root, version)| {
                root.to_str().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Source project root is not UTF-8",
                    )
                })?;
                Ok::<_, io::Error>((root.to_path_buf(), version.map(str::to_owned)))
            })
            .transpose()?;
        let remote =
            connect_source(&installation, address, &client, key_limits, original, ()).await?;
        let tools = tokio::select! {
            result=remote.peer().list_all_tools()=>result.map_err(io::Error::other),
            ()=original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
            ()=tokio::time::sleep_until(deadline.into())=>Err(io::ErrorKind::TimedOut.into()),
        };
        remote.cancel().await.map_err(io::Error::other)?;
        original.snapshot().check()?;
        let tools = tools?;
        let task_version = if tools
            .iter()
            .any(|tool| tool.name.starts_with("graphit_task_"))
        {
            match &current {
                Some((root, None)) => Some(
                    crate::task_entry::captured_project_version(&installation, root, original)
                        .await?,
                ),
                Some((_, Some(version))) => Some(version.clone()),
                None => None,
            }
        } else {
            None
        };
        let tool = tools
            .iter()
            .find(|tool| tool.name == "graphit_ast_source")
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "Source tool unavailable"))?;
        Ok(Self(Arc::new(State {
            installation,
            client,
            key_limits,
            content,
            timeout,
            current,
            task_version,
            tool,
            catalog: tools,
            stop: CancellationToken::new(),
            jobs: Mutex::new(JoinSet::new()),
        })))
    }
    /// The SDK owns framing and negotiation; EOF cancels outgoing calls and joins owners.
    pub async fn serve_stream<R, W>(&self, read: R, write: W) -> io::Result<()>
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let service = self
            .clone()
            .serve_with_ct((read, write), self.0.stop.child_token())
            .await
            .map_err(io::Error::other)?;
        let result = service.waiting().await.map_err(io::Error::other);
        self.shutdown().await?;
        result.map(|_| ())
    }
    pub async fn shutdown(&self) -> io::Result<()> {
        self.0.stop.cancel();
        let mut jobs = self.0.jobs.lock().await;
        let mut failed = false;
        while let Some(result) = jobs.join_next().await {
            failed |= result.is_err();
        }
        if failed {
            Err(io::Error::other("Source proxy worker failed"))
        } else {
            Ok(())
        }
    }

    // Selection comes from the actual advertised schema, never an authority claim.
    fn select_catalog_call(
        &self,
        mut call: CallToolRequestParams,
    ) -> Result<CallToolRequestParams, ErrorData> {
        let Some((root, version)) = &self.0.current else {
            return Ok(call);
        };
        let Some(tool) = self.0.catalog.iter().find(|tool| tool.name == call.name) else {
            return Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                "unknown tool",
                None,
            ));
        };
        // This guidance endpoint selects Global by omission, independent of launcher cwd.
        if call.name
            == self
                .0
                .installation
                .brand()
                .mcp_tool_name(&["module", "skill"])
                .map_err(|_| {
                    ErrorData::internal_error("invalid captured module skill namespace", None)
                })?
        {
            return Ok(call);
        }
        if call
            .arguments
            .as_ref()
            .and_then(|arguments| arguments.get("scope"))
            .and_then(serde_json::Value::as_str)
            == Some("user")
        {
            return Ok(call);
        }
        // Global management must not acquire a PROJECT merely from the launch directory.
        if call
            .arguments
            .as_ref()
            .and_then(|arguments| arguments.get("global"))
            == Some(&serde_json::Value::Bool(true))
        {
            return Ok(call);
        }
        let Some(properties) = tool
            .input_schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
        else {
            return Ok(call);
        };
        if let Some(selection_properties) = properties
            .get("selection")
            .and_then(|schema| schema.get("properties"))
            .and_then(serde_json::Value::as_object)
            && selection_properties.contains_key("project_dir")
        {
            let arguments = call.arguments.get_or_insert_with(Default::default);
            let selection = arguments
                .entry("selection")
                .or_insert_with(|| serde_json::json!({}));
            let Some(selection) = selection.as_object_mut() else {
                return Ok(call);
            };
            let same_project = match (selection.get("project_dir"), selection.get("project_id")) {
                (None, None) => {
                    let directory = graphit_core::native_path::NativePath::new(root.clone())
                        .map_err(|_| {
                            ErrorData::internal_error("invalid captured project locator", None)
                        })?;
                    selection.insert(
                        "project_dir".into(),
                        serde_json::to_value(directory).map_err(|_| {
                            ErrorData::internal_error("project locator encoding failed", None)
                        })?,
                    );
                    true
                }
                (Some(directory), _) => serde_json::from_value::<
                    graphit_core::native_path::NativePath,
                >(directory.clone())
                .is_ok_and(|directory| directory.as_path() == root),
                _ => false,
            };
            if same_project
                && selection_properties.contains_key("project_version")
                && !selection.contains_key("project_version")
                && let Some(version) = &self.0.task_version
            {
                selection.insert(
                    "project_version".into(),
                    serde_json::Value::String(version.clone()),
                );
            }
            return Ok(call);
        }
        let Some(directory_schema) = properties.get("project_dir") else {
            return Ok(call);
        };
        let arguments = call.arguments.get_or_insert_with(Default::default);
        if arguments.contains_key("context") {
            return Ok(call);
        }
        let path_type = directory_schema.get("type");
        let accepts_string = path_type.and_then(serde_json::Value::as_str) == Some("string")
            || path_type
                .and_then(serde_json::Value::as_array)
                .is_some_and(|types| types.iter().any(|kind| kind.as_str() == Some("string")));
        if !accepts_string {
            return Ok(call);
        }
        let same_project = match arguments.get("project_dir") {
            None | Some(serde_json::Value::Null) => {
                arguments.insert(
                    "project_dir".into(),
                    serde_json::Value::String(
                        root.to_str().expect("validated Unicode root").into(),
                    ),
                );
                true
            }
            Some(serde_json::Value::String(directory)) => Path::new(directory) == root,
            _ => false,
        };
        if same_project
            && properties.contains_key("project_version")
            && arguments
                .get("project_version")
                .is_none_or(serde_json::Value::is_null)
            && let Some(version) = version
        {
            arguments.insert(
                "project_version".into(),
                serde_json::Value::String(version.clone()),
            );
        }
        Ok(call)
    }

    async fn call_remote(
        &self,
        call: CallToolRequestParams,
        context: Incoming<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let deadline = Instant::now()
            .checked_add(self.0.timeout)
            .ok_or_else(|| ErrorData::internal_error("invalid forwarding deadline", None))?;
        let cancel = Cancellation::default();
        let _guard = CancelOnDrop(cancel.clone());
        let (send, receive) = oneshot::channel();
        let mut jobs = tokio::select! {
            jobs = self.0.jobs.lock() => jobs,
            () = context.ct.cancelled() => return Ok(CallToolResult::error(vec![ContentBlock::text("Source forwarding cancelled")]).into()),
            () = self.0.stop.cancelled() => return Ok(CallToolResult::error(vec![ContentBlock::text("Source proxy stopped")]).into()),
            () = tokio::time::sleep_until(deadline.into()) => return Ok(CallToolResult::error(vec![ContentBlock::text("Source forwarding deadline")]).into()),
        };
        while jobs.try_join_next().is_some() {}
        if self.0.stop.is_cancelled() || context.ct.is_cancelled() {
            return Ok(
                CallToolResult::error(vec![ContentBlock::text("Source proxy stopped")]).into(),
            );
        }
        let owner = Arc::clone(&self.0);
        let worker_cancel = cancel.clone();
        let incoming = context.ct.clone();
        jobs.spawn(async move {
            let result = async {
                let original = RequestContext::new(owner.content, &worker_cancel, deadline)
                    .map_err(|error| ForwardFailure::new("original", io::Error::other(error)))?;
                let address = crate::source_entry::address(&owner.installation, owner.key_limits, &original)
                    .await.map_err(|error| ForwardFailure::new("discovery", error))?;
                let remote = connect_source(&owner.installation, address, &owner.client, owner.key_limits, &original, ())
                    .await.map_err(|error| ForwardFailure::new("connect", error))?;
                let result = tokio::select! {
                    value = remote.peer().call_tool(call) => value.map_err(io::Error::other),
                    () = incoming.cancelled() => { worker_cancel.cancel(); Err(io::ErrorKind::Interrupted.into()) },
                    () = worker_cancel.cancelled() => Err(io::ErrorKind::Interrupted.into()),
                    () = owner.stop.cancelled() => { worker_cancel.cancel(); Err(io::ErrorKind::Interrupted.into()) },
                    () = tokio::time::sleep_until(deadline.into()) => Err(io::ErrorKind::TimedOut.into()),
                };
                // Always join SDK cleanup, retaining a call failure if cleanup also fails.
                let cleanup = remote.cancel().await.map(|_| ()).map_err(io::Error::other);
                let valid = original.check();
                ForwardFailure::finish(result, cleanup, valid)
            }.await;
            let _ = send.send(result);
        });
        drop(jobs);
        let result = tokio::select! {
            result = receive => result.map_err(|_| ErrorData::internal_error("Source proxy worker failed", None))?,
            () = context.ct.cancelled() => Err(ForwardFailure::new("forward", io::ErrorKind::Interrupted.into())),
            () = self.0.stop.cancelled() => Err(ForwardFailure::new("forward", io::ErrorKind::Interrupted.into())),
            () = tokio::time::sleep_until(deadline.into()) => Err(ForwardFailure::new("forward", io::ErrorKind::TimedOut.into())),
        };
        let current = !context.ct.is_cancelled()
            && !self.0.stop.is_cancelled()
            && !cancel.is_cancelled()
            && Instant::now() < deadline;
        Ok(match result {
            Ok(value) if current => value,
            Ok(_) => CallToolResult::error(vec![ContentBlock::text("Source forwarding cancelled")]),
            Err(error) => {
                // Receiving a finished job can race the same terminal signals as a success.
                // An RPC rejection is deliverable only while this transport is still current.
                let error = if !current && error.protocol_error().is_some() {
                    error.terminal(if Instant::now() >= deadline {
                        io::ErrorKind::TimedOut
                    } else {
                        io::ErrorKind::Interrupted
                    })
                } else {
                    error
                };
                if let Some(protocol) = error.protocol_error() {
                    return Err(protocol);
                }
                CallToolResult::error(vec![ContentBlock::text(error.message())])
            }
        }
        .into())
    }
}
struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl ServerHandler for SourceProxy {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.0
            .catalog
            .iter()
            .find(|tool| tool.name == name)
            .cloned()
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: Incoming<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.0.catalog.clone()))
    }
    async fn call_tool(
        &self,
        call: CallToolRequestParams,
        context: Incoming<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if !self.0.catalog.iter().any(|tool| tool.name == call.name) {
            return Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                "unknown tool",
                None,
            ));
        }
        if call.name == knowledge_ask::TOOL {
            return knowledge_ask::call(self, call, context).await;
        }
        if call.name != self.0.tool.name {
            let now_names = [
                self.0
                    .installation
                    .brand()
                    .mcp_tool_name(&["now"])
                    .map_err(|_| {
                        ErrorData::internal_error("invalid captured Now namespace", None)
                    })?,
                self.0
                    .installation
                    .brand()
                    .mcp_tool_name(&["instances"])
                    .map_err(|_| {
                        ErrorData::internal_error("invalid captured instances namespace", None)
                    })?,
            ];
            let call = if now_names.iter().any(|name| name.as_str() == call.name) {
                call
            } else {
                self.select_catalog_call(call)?
            };
            return self.call_remote(call, context).await;
        }
        let mut input: SourceArguments = serde_json::from_value(serde_json::Value::Object(
            call.arguments.unwrap_or_default(),
        ))
        .map_err(|_| ErrorData::new(ErrorCode::INVALID_PARAMS, "Source arguments invalid", None))?;
        if input.context.is_none()
            && let Some((root, version)) = &self.0.current
            && input
                .project_dir
                .as_deref()
                .is_none_or(|directory| Path::new(directory) == root)
        {
            input
                .project_dir
                .get_or_insert_with(|| root.to_str().expect("validated UTF-8 root").to_owned());
            if let Some(version) = version {
                input.project_version.get_or_insert_with(|| version.clone());
            }
        }
        let deadline = Instant::now().checked_add(self.0.timeout).ok_or_else(|| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, "invalid Source deadline", None)
        })?;
        let cancel = Cancellation::default();
        let _guard = CancelOnDrop(cancel.clone());
        let (send, receive) = oneshot::channel();
        {
            let mut jobs = self.0.jobs.lock().await;
            while jobs.try_join_next().is_some() {}
            if self.0.stop.is_cancelled() || context.ct.is_cancelled() {
                return Ok(
                    CallToolResult::error(vec![ContentBlock::text("Source proxy stopped")]).into(),
                );
            }
            let state = Arc::clone(&self.0);
            let cancellation = cancel.clone();
            let incoming = context.ct.clone();
            jobs.spawn(async move {
                let result = async {
                    let original = RequestContext::new(state.content, &cancellation, deadline)
                        .map_err(io::Error::other)?;
                    let operation = async {
                        let address = crate::source_entry::address(
                            &state.installation,
                            state.key_limits,
                            &original,
                        )
                        .await?;
                        call_source_result(
                            &state.installation,
                            address,
                            &state.client,
                            &input,
                            state.key_limits,
                            &original,
                        )
                        .await
                    };
                    tokio::pin!(operation);
                    tokio::select! {
                        result=&mut operation=>result,
                        ()=incoming.cancelled()=>{cancellation.cancel();operation.await},
                        ()=state.stop.cancelled()=>{cancellation.cancel();operation.await},
                    }
                }
                .await;
                let _ = send.send(result);
            });
        }
        let result = receive.await.map_err(|_| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Source proxy worker failed",
                None,
            )
        })?;
        let result = match result {
            Ok(result)
                if !cancel.is_cancelled()
                    && !context.ct.is_cancelled()
                    && !self.0.stop.is_cancelled()
                    && Instant::now() < deadline =>
            {
                result
            }
            Ok(_) => CallToolResult::error(vec![ContentBlock::text("Source request cancelled")]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(match error.kind() {
                io::ErrorKind::Interrupted => "Source request cancelled",
                io::ErrorKind::TimedOut => "Source request deadline expired",
                _ => "Source transport or credential failed",
            })]),
        };
        if let Ok(mut jobs) = self.0.jobs.try_lock() {
            while jobs.try_join_next().is_some() {}
        }
        Ok(result.into())
    }
}
/// Run the native child's Web-bound MCP transport without Runtime credentials.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub async fn serve_live_user_stdio(
    limits: graphit_hub::native::ContentLimits,
    timeout: std::time::Duration,
    cancellation: &graphit_core::file_lock::Cancellation,
) -> std::io::Result<()> {
    let proxy = crate::live_agent_tools::stdio::LiveMcpProxy::from_environment(limits, timeout)?;
    let serve = proxy
        .clone()
        .serve_stream(tokio::io::stdin(), stdio_stdout());
    tokio::pin!(serve);
    tokio::select! {
        result = &mut serve => result,
        () = cancellation.cancelled() => {
            proxy.request_stop();
            serve.await
        }
    }
}
