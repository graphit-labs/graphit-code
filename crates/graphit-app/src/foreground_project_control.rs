//! Administrative requests enter the existing Foreground owner, which retains joins.
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use graphit_core::{file_lock::Cancellation, native_path::NativePath, project_identity::ProjectId};
use graphit_hub::native::ContentLimits;
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub const ROUTE: &str = "/admin/projects";

#[derive(Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectCommand {
    /// Observe the actual checkout, select its version and activate its existing modules.
    MaintainCheckout {
        project_id: ProjectId,
        project_dir: NativePath,
        version: String,
        detached_base: String,
        select: bool,
    },
    RetireCheckout {
        project_id: ProjectId,
        project_dir: NativePath,
    },
}

pub(super) struct PendingCommand {
    pub command: ProjectCommand,
    pub credential: Zeroizing<String>,
    pub cancellation: Cancellation,
    pub deadline: Instant,
    pub response: oneshot::Sender<(StatusCode, serde_json::Value, Option<std::io::Error>)>,
}

pub(super) type LocalModelResources = (
    Arc<graphit_daemon::model_service::ModelServiceClient>,
    graphit_daemon::model_service::ExpectedModel,
    std::net::SocketAddr,
);

pub(super) struct PendingLocalModel {
    pub original: Arc<graphit_core::file_io::SnapshotContext>,
    pub provider: String,
    pub purpose: graphit_auth::ServicePurpose,
    pub execution: graphit_auth::profile::LocalExecution,
    pub response: oneshot::Sender<std::io::Result<LocalModelResources>>,
}

pub(super) enum OwnerCommand {
    Project(PendingCommand),
    LocalModel(PendingLocalModel),
}

struct Shared {
    send: mpsc::Sender<OwnerCommand>,
    content: ContentLimits,
    timeout: Duration,
    stop: CancellationToken,
}

#[derive(Clone)]
pub(super) struct ProjectControl(Arc<Shared>);
impl ProjectControl {
    pub fn new(
        content: ContentLimits,
        timeout: Duration,
        stop: CancellationToken,
    ) -> (Self, mpsc::Receiver<OwnerCommand>) {
        // Only one command enters this owner at a time; this is backpressure, not a quota.
        let (send, receive) = mpsc::channel(1);
        (
            Self(Arc::new(Shared {
                send,
                content,
                timeout,
                stop,
            })),
            receive,
        )
    }
    /// Demand the already owned local process for this real service lease.
    /// Borrowing the lease across the await retains the original admission;
    /// the owner independently recaptures and compares its current binding.
    pub async fn ensure_local_model(
        &self,
        lease: &graphit_auth::LocalServiceLease<'_>,
    ) -> std::io::Result<LocalModelResources> {
        use graphit_auth::{ServiceBindingView, ServicePurpose};
        let (provider, purpose, execution) = match lease.binding().map_err(std::io::Error::other)? {
            ServiceBindingView::Local {
                provider,
                purpose: purpose @ (ServicePurpose::Embedding | ServicePurpose::Rerank),
                execution,
            } => (provider.to_owned(), purpose, execution),
            _ => return Err(std::io::ErrorKind::PermissionDenied.into()),
        };
        let original = Arc::clone(lease.original().map_err(std::io::Error::other)?.snapshot());
        let deadline = original.required_deadline()?;
        let (response, received) = oneshot::channel();
        let pending = PendingLocalModel {
            original: Arc::clone(&original),
            provider,
            purpose,
            execution,
            response,
        };
        tokio::select! {
            result = self.0.send.send(OwnerCommand::LocalModel(pending)) => {
                result.map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
            },
            () = original.cancellation().cancelled() => return Err(std::io::ErrorKind::Interrupted.into()),
            () = self.0.stop.cancelled() => return Err(std::io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(deadline.into()) => return Err(std::io::ErrorKind::TimedOut.into()),
        }
        let result = tokio::select! {
            result = received => result.map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?,
            () = original.cancellation().cancelled() => return Err(std::io::ErrorKind::Interrupted.into()),
            () = self.0.stop.cancelled() => return Err(std::io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(deadline.into()) => return Err(std::io::ErrorKind::TimedOut.into()),
        };
        lease.check().map_err(std::io::Error::other)?;
        original.check()?;
        result
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route(ROUTE, post(command))
            .layer(DefaultBodyLimit::max(self.0.content.max_input_bytes))
            .with_state(self.clone())
    }
}

struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

async fn command(
    State(control): State<ProjectControl>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if headers.contains_key(header::ORIGIN) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if headers.get_all("x-graphit-auth-origin").iter().count() != 1
        || headers
            .get("x-graphit-auth-origin")
            .and_then(|v| v.to_str().ok())
            != Some("runtime")
        || headers.get_all(header::AUTHORIZATION).iter().count() != 1
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(raw) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| {
            !v.is_empty()
                && v.len() <= control.0.content.max_scalar_bytes
                && !v.bytes().any(|b| b.is_ascii_whitespace())
        })
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let credential = Zeroizing::new(raw.to_owned());
    drop(headers);
    let input = match serde_json::from_slice::<ProjectCommand>(&body) {
        Ok(input) => input,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    drop(body);
    let cancellation = Cancellation::default();
    let _guard = CancelOnDrop(cancellation.clone());
    let Some(deadline) = Instant::now().checked_add(control.0.timeout) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let (response, received) = oneshot::channel();
    let pending = PendingCommand {
        command: input,
        credential,
        cancellation,
        deadline,
        response,
    };
    tokio::select! {
        result = control.0.send.send(OwnerCommand::Project(pending)) => if result.is_err() { return StatusCode::SERVICE_UNAVAILABLE.into_response(); },
        () = control.0.stop.cancelled() => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        () = tokio::time::sleep_until(deadline.into()) => return StatusCode::GATEWAY_TIMEOUT.into_response(),
    }
    tokio::select! {
        result = received => match result {
            Ok((status, body, cause)) => {
                if let Some(cause) = cause { eprintln!("PROJECT administrative operation failed: {cause}"); }
                (status, axum::Json(body)).into_response()
            },
            Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        },
        () = control.0.stop.cancelled() => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        () = tokio::time::sleep_until(deadline.into()) => StatusCode::GATEWAY_TIMEOUT.into_response(),
    }
}

/// The compiled client talks only to the installation's captured live daemon.
pub mod client {
    use super::{ProjectCommand, send};
    use graphit_core::{
        native_path::NativePath,
        project_paths::{InstallationPaths, ProjectPaths},
    };
    use graphit_hub::policy::{HubResourcePolicy, RequestContext};
    use std::{
        io::{self, Write},
        path::PathBuf,
        process::ExitCode,
    };

    #[derive(clap::Args)]
    pub struct Arguments {
        #[command(subcommand)]
        pub command: Command,
    }
    #[derive(clap::Subcommand)]
    pub enum Command {
        /// List authorized local projects, checkouts and retained versions.
        List(DiscoveryArguments),
        /// Locate one local project by its exact ID or unique name.
        Show {
            #[arg(long, required_unless_present = "name", conflicts_with = "name")]
            project_id: Option<String>,
            #[arg(
                long,
                required_unless_present = "project_id",
                conflicts_with = "project_id"
            )]
            name: Option<String>,
            /// Filter retained versions by an exact name.
            #[arg(long)]
            version: Option<String>,
            #[arg(long)]
            no_paths: bool,
        },
        /// Add a label value to an active local checkout.
        LabelSet {
            #[command(flatten)]
            target: LabelTarget,
            #[arg(long, value_name = "VALUE")]
            value: String,
        },
        /// Remove a label value or the entire key when no value is given.
        LabelUnset {
            #[command(flatten)]
            target: LabelTarget,
            #[arg(long, value_name = "VALUE")]
            value: Option<String>,
        },
        /// Observe this checkout/version, keep sibling jobs and publish the whole inventory.
        MaintainCheckout {
            #[arg(long)]
            project_id: String,
            /// Select a checkout directory. Defaults to the current project.
            #[arg(long)]
            project_dir: Option<PathBuf>,
            #[arg(long, default_value = "")]
            version: String,
            #[arg(long, default_value = "")]
            detached_base: String,
            #[arg(long)]
            select: bool,
        },
        /// Stop this checkout's jobs, retain its versions, and publish the inventory.
        RetireCheckout {
            #[arg(long)]
            project_id: String,
            /// Select a checkout directory. Defaults to the current project.
            #[arg(long)]
            project_dir: Option<PathBuf>,
        },
    }
    #[derive(clap::Args)]
    pub struct LabelTarget {
        #[arg(long, value_name = "ID")]
        project_id: graphit_core::project_identity::ProjectId,
        /// Select a checkout directory. Defaults to the current project.
        #[arg(long, value_name = "DIR")]
        project_dir: Option<PathBuf>,
        /// Select an exact active version when the checkout has multiple versions.
        #[arg(long, value_name = "VERSION")]
        version: Option<String>,
        #[arg(long, value_name = "KEY")]
        key: String,
    }
    #[derive(clap::Args)]
    pub struct DiscoveryArguments {
        #[arg(long)]
        pub name: Option<String>,
        #[arg(long)]
        pub project_id: Option<String>,
        #[arg(long)]
        pub version: Option<String>,
        /// Require this exact local label key.
        #[arg(long, value_name = "KEY")]
        pub label_key: Option<String>,
        /// Require this exact value of the selected key.
        #[arg(long, value_name = "VALUE", requires = "label_key")]
        pub label_value: Option<String>,
        #[arg(long, num_args=0..=1, default_missing_value="true", require_equals=true, action=clap::ArgAction::Set)]
        pub online: Option<bool>,
        #[arg(long)]
        pub no_paths: bool,
        #[arg(long, default_value = "25")]
        pub page_size: std::num::NonZeroUsize,
        #[arg(long)]
        pub cursor: Option<String>,
    }
    impl Command {
        pub fn apply_project_version(&mut self, global: &str) -> Result<(), &'static str> {
            fn merge(local: &mut Option<String>, global: &str) -> Result<(), &'static str> {
                if local.as_deref().is_some_and(|value| value != global) {
                    return Err("--version and --project-version must select the same version");
                }
                local.get_or_insert_with(|| global.to_owned());
                Ok(())
            }
            match self {
                Self::List(input) => merge(&mut input.version, global),
                Self::Show { version, .. } => merge(version, global),
                Self::LabelSet { target, .. } | Self::LabelUnset { target, .. } => {
                    merge(&mut target.version, global)
                }
                Self::MaintainCheckout { version, .. } => {
                    if !version.is_empty() && version != global {
                        return Err("--version and --project-version must select the same version");
                    }
                    *version = global.to_owned();
                    Ok(())
                }
                Self::RetireCheckout { .. } => {
                    Err("--project-version cannot be used with projects retire-checkout")
                }
            }
        }
    }
    pub fn mcp_call(
        command: &Command,
        installation: &InstallationPaths,
    ) -> io::Result<Option<rmcp::model::CallToolRequestParams>> {
        use serde_json::{Map, Value, json};
        let mut values = Map::new();
        let name = match command {
            Command::List(input) => {
                for (key, value) in [
                    ("name", &input.name),
                    ("project_id", &input.project_id),
                    ("version", &input.version),
                    ("label_key", &input.label_key),
                    ("label_value", &input.label_value),
                    ("cursor", &input.cursor),
                ] {
                    if let Some(value) = value {
                        values.insert(key.into(), json!(value));
                    }
                }
                if let Some(online) = input.online {
                    values.insert("online".into(), json!(online));
                }
                values.insert("include_paths".into(), json!(!input.no_paths));
                values.insert("page_size".into(), json!(input.page_size));
                "graphit_projects_list"
            }
            Command::Show {
                project_id,
                name,
                version,
                no_paths,
            } => {
                if let Some(value) = project_id {
                    values.insert("project_id".into(), Value::String(value.clone()));
                }
                if let Some(value) = name {
                    values.insert("name".into(), Value::String(value.clone()));
                }
                if let Some(value) = version {
                    values.insert("version".into(), Value::String(value.clone()));
                }
                values.insert("include_paths".into(), json!(!no_paths));
                "graphit_projects_get"
            }
            Command::LabelSet { target, value } => {
                values = label_arguments(target, Some(value), installation)?;
                "graphit_projects_labels_set"
            }
            Command::LabelUnset { target, value } => {
                values = label_arguments(target, value.as_deref(), installation)?;
                "graphit_projects_labels_unset"
            }
            _ => return Ok(None),
        };
        Ok(Some(
            rmcp::model::CallToolRequestParams::new(name).with_arguments(values),
        ))
    }
    fn label_arguments(
        target: &LabelTarget,
        value: Option<&str>,
        installation: &InstallationPaths,
    ) -> io::Result<serde_json::Map<String, serde_json::Value>> {
        use serde_json::json;
        require_native_context()?;
        let project = if let Some(directory) = &target.project_dir {
            ProjectPaths::at(installation.start_directory().join(directory), installation)?
        } else {
            ProjectPaths::discover(installation.start_directory(), installation)?
        };
        let path = NativePath::new(project.root().to_owned())?;
        let mut values = serde_json::Map::from_iter([
            ("project_id".into(), json!(target.project_id)),
            ("project_dir".into(), json!(path)),
            ("key".into(), json!(target.key)),
        ]);
        if let Some(value) = value {
            values.insert("value".into(), json!(value));
        }
        if let Some(version) = &target.version {
            values.insert("project_version".into(), json!(version));
        }
        Ok(values)
    }

    fn require_native_context() -> io::Result<()> {
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
                "native project control is unavailable in a managed-role transport context",
            ));
        }
        Ok(())
    }

    /// Validate the command before the caller starts or contacts a daemon.
    pub fn prepare(
        arguments: Arguments,
        installation: &InstallationPaths,
    ) -> io::Result<ProjectCommand> {
        require_native_context()?;
        let checkout = |directory: Option<PathBuf>| {
            if let Some(directory) = directory {
                let absolute = installation.start_directory().join(directory);
                return match std::fs::canonicalize(&absolute) {
                    Ok(path) => NativePath::new(path),
                    // A recorded checkout can be retired after its directory is gone.
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        NativePath::new(absolute)
                    }
                    Err(error) => Err(error),
                };
            }
            let project = ProjectPaths::discover(installation.start_directory(), installation)?;
            NativePath::new(project.root().to_owned())
        };
        Ok(match arguments.command {
            Command::List(_)
            | Command::Show { .. }
            | Command::LabelSet { .. }
            | Command::LabelUnset { .. } => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "project inventory and labels use the authenticated MCP interface",
                ));
            }
            Command::RetireCheckout {
                project_id,
                project_dir,
            } => ProjectCommand::RetireCheckout {
                project_id: serde_json::from_value(serde_json::Value::String(project_id))?,
                project_dir: checkout(project_dir)?,
            },
            Command::MaintainCheckout {
                project_id,
                project_dir,
                version,
                detached_base,
                select,
            } => ProjectCommand::MaintainCheckout {
                project_id: serde_json::from_value(serde_json::Value::String(project_id))?,
                project_dir: checkout(project_dir)?,
                version,
                detached_base,
                select,
            },
        })
    }

    pub async fn run(
        command: ProjectCommand,
        installation: &InstallationPaths,
        policy: &HubResourcePolicy,
        original: &RequestContext<'_>,
        output: &mut dyn Write,
    ) -> io::Result<ExitCode> {
        let address = crate::source_entry::address(installation, original).await?;
        let result = send(
            installation,
            address,
            &reqwest::Client::new(),
            policy.content(),
            &command,
            original,
        )
        .await;
        let (status, value) = match result {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                return Ok(ExitCode::from(130));
            }
            other => other?,
        };
        serde_json::to_writer(&mut *output, &value)?;
        writeln!(output)?;
        Ok(if status.is_success() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        })
    }
}

/// Shared native caller used by explicit CLI commands and MCP startup.
pub(crate) async fn send(
    installation: &graphit_core::project_paths::InstallationPaths,
    address: std::net::SocketAddr,
    client: &reqwest::Client,
    policy: graphit_hub::native::ContentLimits,
    command: &ProjectCommand,
    original: &graphit_hub::policy::RequestContext<'_>,
) -> std::io::Result<(reqwest::StatusCode, serde_json::Value)> {
    use std::io;
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
            "native project control is unavailable in a managed-role transport context",
        ));
    }
    let headers = crate::ast_source::runtime_headers(installation, original).await?;
    let body = serde_json::to_vec(command)?;
    if body.len() > policy.max_input_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let operation = async {
        let mut response = client
            .post(format!("http://{address}{ROUTE}"))
            .headers(headers.into_iter().collect())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(io::Error::other)?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(io::Error::other)? {
            original.check()?;
            if chunk.len() > policy.max_output_bytes.saturating_sub(bytes.len()) {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value = if status.is_success() {
            serde_json::from_slice(&bytes)?
        } else {
            serde_json::from_slice(&bytes).map_err(|_| {
                let kind = match status.as_u16() {
                    403 => io::ErrorKind::PermissionDenied,
                    404 => io::ErrorKind::NotFound,
                    400 => io::ErrorKind::InvalidInput,
                    504 => io::ErrorKind::TimedOut,
                    _ => io::ErrorKind::Other,
                };
                io::Error::new(
                    kind,
                    format!(
                        "Source project control HTTP {} (body_len={})",
                        status.as_u16(),
                        bytes.len()
                    ),
                )
            })?
        };
        original.check()?;
        Ok((status, value))
    };
    let deadline = original.snapshot().required_deadline()?;
    tokio::select! {
        result = operation => result,
        () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(deadline.into()) => Err(io::ErrorKind::TimedOut.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
    }

    async fn listener(
        timeout: Duration,
    ) -> (
        String,
        mpsc::Receiver<OwnerCommand>,
        CancellationToken,
        tokio::task::JoinHandle<std::io::Result<()>>,
    ) {
        let stop = CancellationToken::new();
        let content = ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        };
        let (control, receiver) = ProjectControl::new(content, timeout, stop.clone());
        let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}{}", socket.local_addr().unwrap(), ROUTE);
        let shutdown = stop.clone();
        let served = tokio::spawn(async move {
            axum::serve(socket, control.router())
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
        });
        (url, receiver, stop, served)
    }

    fn body() -> serde_json::Value {
        serde_json::to_value(ProjectCommand::RetireCheckout {
            project_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            project_dir: NativePath::new(std::env::temp_dir().join("projetos").join("café Ω"))
                .unwrap(),
        })
        .unwrap()
    }

    async fn send_peer_response(
        fixture: &crate::source_server::test_fixture::Fixture,
        status: StatusCode,
        body: &'static str,
    ) -> io::Result<(reqwest::StatusCode, serde_json::Value)> {
        let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let router = Router::new().route(
            ROUTE,
            post(move || async move { (status, body).into_response() }),
        );
        let stop = CancellationToken::new();
        let shutdown = stop.clone();
        let served = tokio::spawn(async move {
            axum::serve(socket, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
        });
        let cancellation = Cancellation::default();
        let policy = fixture.server.policy();
        let original = graphit_hub::policy::RequestContext::new(
            policy.content,
            &cancellation,
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
        let (paths, id, _) = &fixture.projects[0];
        let command = ProjectCommand::RetireCheckout {
            project_id: *id,
            project_dir: NativePath::new(paths.root().to_path_buf()).unwrap(),
        };
        let result = send(
            &fixture.paths,
            address,
            &client(),
            policy.content,
            &command,
            &original,
        )
        .await;
        stop.cancel();
        served.await.unwrap().unwrap();
        result
    }

    #[tokio::test]
    async fn send_preserves_empty_and_non_json_http_error_status_without_body() {
        use graphit_core::brand::Brand;
        use graphit_hub::NativeBinding;

        let fixture = crate::source_server::test_fixture::fixture(
            &[],
            Brand::default(),
            NativeBinding::Codex,
        );
        for (status, expected_kind) in [
            (StatusCode::BAD_REQUEST, io::ErrorKind::InvalidInput),
            (StatusCode::UNAUTHORIZED, io::ErrorKind::Other),
            (StatusCode::FORBIDDEN, io::ErrorKind::PermissionDenied),
            (StatusCode::NOT_FOUND, io::ErrorKind::NotFound),
            (StatusCode::SERVICE_UNAVAILABLE, io::ErrorKind::Other),
            (StatusCode::GATEWAY_TIMEOUT, io::ErrorKind::TimedOut),
        ] {
            for body in ["", "private-response-body"] {
                let error = send_peer_response(&fixture, status, body)
                    .await
                    .unwrap_err();
                assert_eq!(error.kind(), expected_kind);
                assert_eq!(
                    error.to_string(),
                    format!(
                        "Source project control HTTP {} (body_len={})",
                        status.as_u16(),
                        body.len()
                    )
                );
                assert!(!error.to_string().contains(&fixture.raw));
                assert!(!error.to_string().contains("private-response-body"));
            }
        }
        fixture.server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn send_preserves_valid_json_success_and_partial_failure_effects() {
        use graphit_core::brand::Brand;
        use graphit_hub::NativeBinding;

        let fixture = crate::source_server::test_fixture::fixture(
            &[],
            Brand::default(),
            NativeBinding::Codex,
        );
        for status in [StatusCode::OK, StatusCode::SERVICE_UNAVAILABLE] {
            let body =
                r#"{"phase":"broker_publish","checkout_removed":true,"broker_effect":"unknown"}"#;
            let (observed, value) = send_peer_response(&fixture, status, body).await.unwrap();
            assert_eq!(observed, status);
            assert_eq!(
                value,
                serde_json::from_str::<serde_json::Value>(body).unwrap()
            );
        }
        fixture.server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn send_rejects_empty_success_including_no_content() {
        use graphit_core::brand::Brand;
        use graphit_hub::NativeBinding;

        let fixture = crate::source_server::test_fixture::fixture(
            &[],
            Brand::default(),
            NativeBinding::Codex,
        );
        for status in [StatusCode::OK, StatusCode::NO_CONTENT] {
            assert!(send_peer_response(&fixture, status, "").await.is_err());
        }
        fixture.server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn browser_origin_and_duplicate_credentials_never_enter_owner() {
        let (url, mut receiver, stop, served) = listener(Duration::from_secs(5)).await;
        let client = client();
        let origin = client
            .post(&url)
            .header("Origin", "https://example.test")
            .header("x-graphit-auth-origin", "runtime")
            .bearer_auth("not-a-runtime-token")
            .json(&body())
            .send()
            .await
            .unwrap();
        assert_eq!(origin.status(), StatusCode::FORBIDDEN);
        let duplicate = client
            .post(&url)
            .header("x-graphit-auth-origin", "runtime")
            .header(header::AUTHORIZATION, "Bearer first-invalid-token")
            .header(header::AUTHORIZATION, "Bearer second-invalid-token")
            .json(&body())
            .send()
            .await
            .unwrap();
        assert_eq!(duplicate.status(), StatusCode::UNAUTHORIZED);
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        stop.cancel();
        served.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn queued_deadline_cancels_same_owner_original() {
        let (url, mut receiver, stop, served) = listener(Duration::from_millis(100)).await;
        let request = tokio::spawn(async move {
            client()
                .post(url)
                .header("x-graphit-auth-origin", "runtime")
                .bearer_auth("not-a-runtime-token")
                .json(&body())
                .send()
                .await
                .unwrap()
        });
        let OwnerCommand::Project(pending) =
            tokio::time::timeout(Duration::from_secs(5), receiver.recv())
                .await
                .expect("owner command deadline")
                .unwrap()
        else {
            panic!("HTTP command must use the existing PROJECT variant");
        };
        assert!(!pending.cancellation.is_cancelled());
        let response = request.await.unwrap();
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        tokio::time::timeout(Duration::from_secs(5), pending.cancellation.cancelled())
            .await
            .expect("owner cancellation deadline");
        assert!(Instant::now() >= pending.deadline);
        stop.cancel();
        served.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn daemon_shutdown_cancels_waiting_command_without_abandoning_owner() {
        let (url, mut receiver, stop, served) = listener(Duration::from_secs(5)).await;
        let request = tokio::spawn(async move {
            client()
                .post(url)
                .header("x-graphit-auth-origin", "runtime")
                .bearer_auth("not-a-runtime-token")
                .json(&body())
                .send()
                .await
                .unwrap()
        });
        let OwnerCommand::Project(pending) =
            tokio::time::timeout(Duration::from_secs(5), receiver.recv())
                .await
                .expect("owner command deadline")
                .unwrap()
        else {
            panic!("HTTP command must use the existing PROJECT variant");
        };
        stop.cancel();
        let response = request.await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        tokio::time::timeout(Duration::from_secs(5), pending.cancellation.cancelled())
            .await
            .expect("owner cancellation deadline");
        // This object remains with the owner; shutdown signals it, never replaces it.
        assert!(pending.response.is_closed());
        served.await.unwrap().unwrap();
    }
}
