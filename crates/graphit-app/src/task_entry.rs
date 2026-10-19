//! Task CLI operations through the live authenticated Source MCP service.
use clap::{Args, Subcommand};
use graphit_core::{
    native_document_file::NativeDocumentFileLimits, native_path::NativePath,
    project_paths::InstallationPaths,
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use serde_json::{Map, Value};
use std::{io, path::PathBuf, sync::Arc, time::Instant};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Args, Default)]
pub struct CallArguments {
    /// Exact native PROJECT locator; selection grants no authority.
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    #[arg(long)]
    pub project_id: Option<String>,
    #[arg(skip)]
    pub project_version: Option<String>,
}

mod fields;
pub use fields::*;

#[derive(Subcommand)]
pub enum Command {
    /// Read current Task state and explicitly requested snapshot-bound history/comment pages.
    GetTask(GetTaskArguments),
    /// Read a cursor-pinned Task catalogue, including explicit dates and dependencies.
    Catalogue(CatalogueArguments),
    /// Search current Task specifications and comments using textual, semantic or hybrid retrieval.
    #[cfg(target_os = "linux")]
    Search(SearchArguments),
    /// Read a finite projected Task or Session page from one native pin.
    Query(QueryArguments),
    /// Read the native Task query tables and permitted column types.
    Schema(CallArguments),
    /// Read a Session and its immutable native history.
    GetSession(GetSessionArguments),
    /// Read native Session heads.
    SessionHeads(CallArguments),
    /// Create a native Task with acceptance and test checks, and explicit optional calendar dates.
    #[cfg(target_os = "linux")]
    CreateTask(CreateTaskArguments),
    /// Create a native Session.
    #[cfg(target_os = "linux")]
    CreateSession(CreateSessionArguments),
    /// Claim a Task; return the opaque participant fence only to this caller.
    #[cfg(target_os = "linux")]
    ClaimTask(ClaimArguments),
    /// Claim a Session; return its opaque participant fence.
    #[cfg(target_os = "linux")]
    ClaimSession(ClaimArguments),
    /// Renew a Task owner's actual lease and revision.
    #[cfg(target_os = "linux")]
    HeartbeatTask(HeartbeatArguments),
    /// Renew a Session owner's actual lease and revision.
    #[cfg(target_os = "linux")]
    HeartbeatSession(HeartbeatArguments),
    /// Add a literal Markdown comment with the current participant fence and an idempotency key.
    #[cfg(target_os = "linux")]
    AddTaskComment(CommentArguments),
    /// Record the current Task owner's progress.
    #[cfg(target_os = "linux")]
    ProgressTask(ProgressArguments),
    /// Record the current Session owner's checkpoint.
    #[cfg(target_os = "linux")]
    CheckpointSession(ProgressArguments),
    /// Release a Task using its current participant fence.
    #[cfg(target_os = "linux")]
    ReleaseTask(ReleaseArguments),
    /// Release a Session using its current participant fence.
    #[cfg(target_os = "linux")]
    ReleaseSession(ReleaseArguments),
    /// Revise Task specification; omitted fields stay, --clear-schedule removes dates.
    #[cfg(target_os = "linux")]
    ReviseTask(ReviseTaskArguments),
    /// Revise the coordinated Session specification; omitted fields stay.
    #[cfg(target_os = "linux")]
    ReviseSession(ReviseSessionArguments),
    /// Verify a current Task check with evidence.
    #[cfg(target_os = "linux")]
    VerifyTaskCheck(CheckArguments),
    /// Supersede a Task check, optionally creating a replacement.
    #[cfg(target_os = "linux")]
    SupersedeTaskCheck(SupersedeArguments),
    /// Replace Task parent or dependency links with recorded reason.
    #[cfg(target_os = "linux")]
    UpdateTaskLinks(LinksArguments),
    /// Set or clear the native Task flag with recorded reason.
    #[cfg(target_os = "linux")]
    SetTaskFlag(FlagArguments),
    /// Associate or clear the full Git commit ID without changing Task status/checks.
    #[cfg(target_os = "linux")]
    SetTaskCommit(CommitArguments),
    /// Complete a Task after its actual gates pass.
    #[cfg(target_os = "linux")]
    CompleteTask(CompleteArguments),
    /// Cancel an eligible Task with recorded reason and live fence when owned.
    #[cfg(target_os = "linux")]
    CancelTask(CancelArguments),
    /// Complete a Session after its native Task gates pass.
    #[cfg(target_os = "linux")]
    CompleteSession(CompleteArguments),
}
impl Command {
    pub fn selection_mut(&mut self) -> &mut CallArguments {
        match self {
            Self::GetTask(arguments) => &mut arguments.selection,
            Self::Catalogue(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::Search(arguments) => &mut arguments.selection,
            Self::Query(arguments) => &mut arguments.selection,
            Self::Schema(arguments) => arguments,
            Self::GetSession(arguments) => &mut arguments.selection,
            Self::SessionHeads(arguments) => arguments,
            #[cfg(target_os = "linux")]
            Self::CreateTask(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::CreateSession(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::ClaimTask(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::ClaimSession(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::HeartbeatTask(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::HeartbeatSession(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::AddTaskComment(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::ProgressTask(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::CheckpointSession(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::ReleaseTask(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::ReleaseSession(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::ReviseTask(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::ReviseSession(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::VerifyTaskCheck(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::SupersedeTaskCheck(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::UpdateTaskLinks(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::SetTaskFlag(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::SetTaskCommit(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::CompleteTask(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::CancelTask(arguments) => &mut arguments.selection,
            #[cfg(target_os = "linux")]
            Self::CompleteSession(arguments) => &mut arguments.selection,
        }
    }
    fn operation(&self) -> (&'static str, bool, &CallArguments) {
        match self {
            Self::GetTask(arguments) => ("get_task", false, &arguments.selection),
            Self::Catalogue(arguments) => ("catalogue", false, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::Search(arguments) => ("search", false, &arguments.selection),
            Self::Query(arguments) => ("query", false, &arguments.selection),
            Self::Schema(arguments) => ("schema", false, arguments),
            Self::GetSession(arguments) => ("get_session", false, &arguments.selection),
            Self::SessionHeads(arguments) => ("session_heads", false, arguments),
            #[cfg(target_os = "linux")]
            Self::CreateTask(arguments) => ("create_task", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::CreateSession(arguments) => ("create_session", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::ClaimTask(arguments) => ("claim_task", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::ClaimSession(arguments) => ("claim_session", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::HeartbeatTask(arguments) => ("heartbeat_task", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::HeartbeatSession(arguments) => ("heartbeat_session", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::AddTaskComment(arguments) => ("add_task_comment", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::ProgressTask(arguments) => ("progress_task", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::CheckpointSession(arguments) => {
                ("checkpoint_session", true, &arguments.selection)
            }
            #[cfg(target_os = "linux")]
            Self::ReleaseTask(arguments) => ("release_task", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::ReleaseSession(arguments) => ("release_session", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::ReviseTask(arguments) => ("revise_task", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::ReviseSession(arguments) => ("revise_session", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::VerifyTaskCheck(arguments) => ("verify_task_check", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::SupersedeTaskCheck(arguments) => {
                ("supersede_task_check", true, &arguments.selection)
            }
            #[cfg(target_os = "linux")]
            Self::UpdateTaskLinks(arguments) => ("update_task_links", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::SetTaskFlag(arguments) => ("set_task_flag", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::SetTaskCommit(arguments) => ("set_task_commit", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::CompleteTask(arguments) => ("complete_task", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::CancelTask(arguments) => ("cancel_task", true, &arguments.selection),
            #[cfg(target_os = "linux")]
            Self::CompleteSession(arguments) => ("complete_session", true, &arguments.selection),
        }
    }
    fn input(&self, max_bytes: usize) -> io::Result<Option<Value>> {
        let value = match self {
            Self::Schema(_) | Self::SessionHeads(_) => return Ok(None),
            Self::GetTask(arguments) => arguments.input(max_bytes)?,
            Self::Catalogue(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::Search(arguments) => arguments.input(max_bytes)?,
            Self::Query(arguments) => arguments.input(max_bytes)?,
            Self::GetSession(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::CreateTask(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::CreateSession(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::ClaimTask(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::ClaimSession(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::HeartbeatTask(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::HeartbeatSession(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::AddTaskComment(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::ProgressTask(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::CheckpointSession(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::ReleaseTask(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::ReleaseSession(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::ReviseTask(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::ReviseSession(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::VerifyTaskCheck(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::SupersedeTaskCheck(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::UpdateTaskLinks(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::SetTaskFlag(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::SetTaskCommit(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::CompleteTask(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::CancelTask(arguments) => arguments.input(max_bytes)?,
            #[cfg(target_os = "linux")]
            Self::CompleteSession(arguments) => arguments.input(max_bytes)?,
        };
        Ok(Some(value))
    }
    fn wire(&self, limits: ContentLimits) -> io::Result<(&'static str, Map<String, Value>)> {
        let (operation, write, arguments) = self.operation();
        let mut selection = Map::new();
        if let Some(path) = &arguments.project_dir {
            selection.insert(
                "project_dir".into(),
                serde_json::to_value(NativePath::new(path.clone())?).map_err(io::Error::other)?,
            );
        }
        if let Some(id) = &arguments.project_id {
            selection.insert("project_id".into(), Value::String(id.clone()));
        }
        if let Some(version) = &arguments.project_version {
            selection.insert("project_version".into(), Value::String(version.clone()));
        }
        let mut wire = Map::from_iter([("selection".into(), Value::Object(selection))]);
        if let Some(input) = self.input(limits.max_input_bytes)? {
            wire.insert("input".into(), input);
        }
        if serde_json::to_vec(&wire).map_err(io::Error::other)?.len() > limits.max_input_bytes {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let decoded: crate::task_server::request::Arguments =
            serde_json::from_value(Value::Object(wire.clone()))
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        crate::task_server::request::Operation::parse(operation, write, decoded.input)
            .map_err(|code| io::Error::new(io::ErrorKind::InvalidInput, code))?;
        let (name, _, _, _) = crate::task_server::catalog::OPERATIONS
            .iter()
            .find(|(_, candidate, active_write, _)| {
                *candidate == operation && (!*active_write || cfg!(target_os = "linux"))
            })
            .ok_or_else(|| io::Error::from(io::ErrorKind::Unsupported))?;
        Ok((name, wire))
    }
}

pub async fn captured_project_version(
    installation: &InstallationPaths,
    root: &std::path::Path,
    original: &RequestContext<'_>,
) -> io::Result<String> {
    original.check()?;
    let project = graphit_core::project_paths::ProjectPaths::at(root, installation)?;
    let snapshot = Arc::clone(original.snapshot());
    let version = tokio::task::spawn_blocking(move || {
        snapshot.check()?;
        let remaining = snapshot
            .required_deadline()?
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))?;
        let checkout = graphit_core::git_checkout::GitCheckout::inspect(
            project.root(),
            remaining,
            snapshot.cancellation(),
        )
        .map_err(io::Error::other)?;
        snapshot.check()?;
        let version = graphit_core::version_context::VersionContext::select(
            &project,
            checkout.as_checkout(),
            "",
            None,
            "",
        )?;
        Ok::<_, io::Error>(version.version().to_owned())
    })
    .await
    .map_err(io::Error::other)??;
    original.check()?;
    Ok(version)
}

pub async fn prepare(
    installation: &InstallationPaths,
    command: &Command,
    content: ContentLimits,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
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
            "Root Task is unavailable in a managed-role transport context",
        ));
    }
    original.check()?;
    let (name, mut arguments) = command.wire(content)?;
    let (_, _, selection) = command.operation();
    if let Some(root) = &selection.project_dir
        && selection.project_version.is_none()
    {
        let version = captured_project_version(installation, root, original).await?;
        arguments
            .get_mut("selection")
            .and_then(Value::as_object_mut)
            .expect("wire selection is an object")
            .insert("project_version".into(), Value::String(version));
        if serde_json::to_vec(&arguments)
            .map_err(io::Error::other)?
            .len()
            > content.max_input_bytes
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    Ok(CallToolRequestParams::new(name).with_arguments(arguments))
}

pub async fn run(
    installation: &InstallationPaths,
    client: &reqwest::Client,
    mut prepared: CallToolRequestParams,
    policy: (NativeDocumentFileLimits, ContentLimits),
    original: &RequestContext<'_>,
    output: &mut impl io::Write,
) -> io::Result<()> {
    original.check()?;
    let address = crate::source_entry::address(installation, policy.0, original).await?;
    let connection =
        crate::ast_source::connect_source(installation, address, client, policy.0, original, ())
            .await?;
    let deadline = tokio::time::Instant::from_std(original.snapshot().required_deadline()?);
    crate::mcp_output::json_call(&mut prepared);
    let result = tokio::select! {
        result = connection.peer().call_tool(prepared) => result.map_err(io::Error::other),
        () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(deadline) => Err(io::ErrorKind::TimedOut.into()),
    };
    let closed = connection
        .cancel()
        .await
        .map(|_| ())
        .map_err(io::Error::other);
    let result = result?;
    check_delivery(&result, closed, original)?;
    for content in &result.content {
        let text = content.as_text().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Task returned non-text content")
        })?;
        if text.text.len() > policy.1.max_output_bytes {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if result.is_error == Some(true) {
            return Err(io::Error::other(text.text.clone()));
        }
        check_delivery(&result, Ok(()), original)?;
        output.write_all(text.text.as_bytes())?;
        output.write_all(b"\n")?;
    }
    check_delivery(&result, Ok(()), original)?;
    Ok(())
}

fn check_delivery(
    result: &CallToolResult,
    closed: io::Result<()>,
    original: &RequestContext<'_>,
) -> io::Result<()> {
    let connection = closed.err();
    let request = original.check().err();
    if let Some(cause) = connection.as_ref().or(request.as_ref()) {
        return Err(io::Error::new(
            cause.kind(),
            TaskDeliveryFailure {
                result: result.clone(),
                connection,
                request,
            },
        ));
    }
    Ok(())
}

struct TaskDeliveryFailure {
    result: CallToolResult,
    connection: Option<io::Error>,
    request: Option<io::Error>,
}
impl std::fmt::Debug for TaskDeliveryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Task response delivery failed")
    }
}
impl std::fmt::Display for TaskDeliveryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = serde_json::json!({"code":"delivery_failed","result":&self.result,
            "connection_error":self.connection.as_ref().map(ToString::to_string),
            "request_error":self.request.as_ref().map(ToString::to_string)});
        write!(f, "{value}")
    }
}
impl std::error::Error for TaskDeliveryFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.connection
            .as_ref()
            .or(self.request.as_ref())
            .map(|cause| cause as &(dyn std::error::Error + 'static))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};
    use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};
    use graphit_hub::policy::HubResourcePolicy;
    use std::time::Duration;

    #[derive(Parser)]
    struct TaskCli {
        #[command(subcommand)]
        command: Command,
    }
    fn parse(flags: &[&str]) -> Command {
        TaskCli::try_parse_from(std::iter::once("task").chain(flags.iter().copied()))
            .unwrap()
            .command
    }
    fn wire(flags: &[&str]) -> Map<String, Value> {
        let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        parse(flags).wire(policy.content()).unwrap().1
    }
    const ID: &str = "00000000000000000000000003";
    const TOKEN: &str = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE";

    #[test]
    fn task_cli_subcommands_match_native_catalog() {
        TaskCli::command().debug_assert();
        let cli = TaskCli::command();
        let actual: std::collections::BTreeSet<_> = cli
            .get_subcommands()
            .filter(|c| c.get_name() != "help")
            .map(|c| c.get_name().replace('-', "_"))
            .collect();
        let expected: std::collections::BTreeSet<_> = crate::task_server::catalog::OPERATIONS
            .iter()
            .filter(|(_, _, write, _)| !*write || cfg!(target_os = "linux"))
            .map(|(_, operation, _, _)| operation.to_string())
            .collect();
        assert_eq!(actual, expected);
        #[cfg(target_os = "linux")]
        assert_eq!(actual.len(), 28);
        for subcommand in cli.get_subcommands().filter(|c| c.get_name() != "help") {
            let mut help = subcommand.clone();
            let help = help.render_long_help().to_string();
            assert!(!help.contains("--input"));
            assert!(help.contains("--project-dir"));
        }
    }

    #[test]
    fn query_and_schema_cli_use_the_closed_native_inputs() {
        let arguments = wire(&[
            "query",
            "--table",
            "tasks",
            "--id",
            ID,
            "--column",
            "id,status",
            "--page-size",
            "1",
            "--top-k",
            "1",
        ]);
        assert_eq!(
            arguments["input"],
            serde_json::json!({
                "selection":{"table":"tasks","ids":[ID]},"columns":["id","status"],"page_size":1,"top_k":1
            })
        );
        assert!(!wire(&["schema"]).contains_key("input"));
        for invalid in [
            vec!["query"],
            vec!["schema", "--input", "{}"],
            vec!["get-task", "--id", "00000000000000000000000000"],
            vec!["get-task", "--id", ID, "--actor", "forged"],
            vec!["catalogue", "--page-size", "0"],
            vec!["get-task", "--id", ID, "--history-cursor", "opaque"],
        ] {
            assert!(TaskCli::try_parse_from(std::iter::once("task").chain(invalid)).is_err());
        }
        let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        assert!(
            parse(&[
                "query",
                "--table",
                "sessions",
                "--session-id",
                ID,
                "--column",
                "id",
                "--page-size",
                "1",
                "--top-k",
                "1"
            ])
            .wire(policy.content())
            .is_err()
        );
    }

    #[test]
    fn query_distinguishes_an_empty_id_set_from_an_omitted_filter() {
        let flags = [
            "query",
            "--table",
            "tasks",
            "--column",
            "id",
            "--page-size",
            "1",
            "--top-k",
            "1",
        ];
        assert!(wire(&flags)["input"]["selection"].get("ids").is_none());
        let mut empty = flags.to_vec();
        empty.push("--empty-ids");
        assert_eq!(
            wire(&empty)["input"]["selection"]["ids"],
            serde_json::json!([])
        );
        let mut continuation = empty.clone();
        continuation.extend(["--cursor", "opaque-continuation"]);
        let input = parse(&continuation).input(4096).unwrap().unwrap();
        assert_eq!(input["selection"]["ids"], serde_json::json!([]));
        assert_eq!(input["cursor"], "opaque-continuation");
        empty.extend(["--id", ID]);
        assert!(TaskCli::try_parse_from(std::iter::once("task").chain(empty)).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn every_task_operation_builds_a_closed_native_request() {
        let owned = [
            "--id",
            ID,
            "--claim-token",
            TOKEN,
            "--expected-revision",
            "1",
        ];
        let mut cases: Vec<Vec<&str>> = vec![
            vec!["get-task", "--id", ID],
            vec!["get-session", "--id", ID],
            vec!["catalogue", "--page-size", "10"],
            vec!["schema"],
            vec!["session-heads"],
            vec![
                "query",
                "--table",
                "tasks",
                "--column",
                "id",
                "--page-size",
                "1",
                "--top-k",
                "1",
            ],
            vec![
                "search",
                "--text",
                "needle",
                "--page-size",
                "1",
                "--top-k",
                "1",
            ],
            vec![
                "create-session",
                "--idempotency-key",
                "session",
                "--title",
                "Title",
                "--description",
                "Description",
                "--strategy",
                "Strategy",
            ],
            vec![
                "create-task",
                "--idempotency-key",
                "task",
                "--session-id",
                ID,
                "--title",
                "Title",
                "--description",
                "Description",
                "--task-type",
                "TASK",
                "--priority",
                "1",
                "--check-kind",
                "acceptance",
                "--check-text",
                "Acceptance",
                "--check-kind",
                "test",
                "--check-text",
                "Test",
                "--start-date",
                "2026-10-29",
                "--end-date",
                "2026-10-30",
                "--due-date",
                "2026-10-31",
            ],
            vec!["claim-task", "--id", ID, "--lease-ms", "1000"],
            vec!["claim-session", "--id", ID, "--lease-ms", "1000"],
            vec![
                "set-task-commit",
                "--id",
                ID,
                "--expected-revision",
                "1",
                "--commit",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "--reason",
                "Associate commit",
            ],
            vec!["cancel-task", "--id", ID, "--reason", "Obsolete work"],
        ];
        for flags in [
            vec!["heartbeat-task", "--lease-ms", "1000"],
            vec!["heartbeat-session", "--lease-ms", "1000"],
            vec![
                "add-task-comment",
                "--lease-ms",
                "1000",
                "--idempotency-key",
                "comment",
                "--kind",
                "note",
                "--body",
                "Literal Markdown",
            ],
            vec![
                "progress-task",
                "--lease-ms",
                "1000",
                "--summary",
                "Progress",
                "--next-step",
                "Continue",
            ],
            vec![
                "checkpoint-session",
                "--lease-ms",
                "1000",
                "--summary",
                "Progress",
                "--next-step",
                "Continue",
            ],
            vec![
                "release-task",
                "--summary",
                "Released",
                "--next-step",
                "Continue",
            ],
            vec![
                "release-session",
                "--summary",
                "Released",
                "--next-step",
                "Continue",
            ],
            vec![
                "revise-task",
                "--lease-ms",
                "1000",
                "--reason",
                "Revise",
                "--description",
                "New description",
            ],
            vec![
                "revise-session",
                "--lease-ms",
                "1000",
                "--reason",
                "Revise",
                "--strategy",
                "New strategy",
            ],
            vec![
                "verify-task-check",
                "--lease-ms",
                "1000",
                "--check-id",
                ID,
                "--passed",
                "false",
                "--evidence",
                "Failure recorded",
            ],
            vec![
                "supersede-task-check",
                "--lease-ms",
                "1000",
                "--check-id",
                ID,
                "--reason",
                "Supersede",
                "--replacement-kind",
                "test",
                "--replacement-text",
                "Replacement",
            ],
            vec![
                "update-task-links",
                "--lease-ms",
                "1000",
                "--reason",
                "Dependencies",
                "--depends-on",
                ID,
            ],
            vec![
                "set-task-flag",
                "--lease-ms",
                "1000",
                "--flagged",
                "false",
                "--reason",
                "Clear flag",
            ],
            vec!["complete-task"],
            vec!["complete-session"],
        ] {
            let mut flags = flags;
            flags.extend(owned);
            cases.push(flags);
        }
        assert_eq!(cases.len(), 28);
        let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for flags in cases {
            let command = parse(&flags);
            let (operation, write, _) = command.operation();
            let (name, _) = command
                .wire(policy.content())
                .unwrap_or_else(|e| panic!("{}: {e}", flags[0]));
            assert!(
                crate::task_server::catalog::OPERATIONS.iter().any(
                    |(expected, op, w, _)| *expected == name && *op == operation && *w == write
                )
            );
            assert!(seen.insert(operation));
        }
    }

    #[test]
    fn opaque_cursor_files_preserve_native_shapes_and_input_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let object = dir.path().join("cursor.json");
        std::fs::write(&object, r#"{"binding":{"opaque":[1,2]},"offset":3}"#).unwrap();
        let input = parse(&[
            "catalogue",
            "--page-size",
            "1",
            "--cursor-file",
            object.to_str().unwrap(),
        ])
        .input(4096)
        .unwrap()
        .unwrap();
        assert_eq!(
            input["cursor"],
            serde_json::json!({"binding":{"opaque":[1,2]},"offset":3})
        );
        let token = dir.path().join("cursor.txt");
        std::fs::write(&token, "  native_opaque-token\n").unwrap();
        let command = parse(&[
            "get-task",
            "--id",
            ID,
            "--history-page-size",
            "1",
            "--history-cursor-file",
            token.to_str().unwrap(),
            "--comments-page-size",
            "1",
            "--comments-cursor",
            "comments-token",
        ]);
        let input = command.input(4096).unwrap().unwrap();
        assert_eq!(input["history"]["cursor"], "native_opaque-token");
        assert_eq!(input["comments"]["cursor"], "comments-token");
        assert!(command.input(4).is_err());
        assert!(
            parse(&["catalogue", "--page-size", "1", "--cursor", "[]"])
                .input(4096)
                .is_err()
        );
        assert!(
            TaskCli::try_parse_from([
                "task",
                "catalogue",
                "--page-size",
                "1",
                "--cursor",
                "{}",
                "--cursor-file",
                object.to_str().unwrap()
            ])
            .is_err()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn mutations_preserve_fences_dates_checks_and_explicit_clears() {
        let owned = [
            "--id",
            ID,
            "--claim-token",
            TOKEN,
            "--expected-revision",
            "7",
        ];
        let mut flags = vec![
            "revise-session",
            "--lease-ms",
            "30000",
            "--reason",
            "Updated strategy",
            "--title",
            "Updated",
        ];
        flags.extend(owned);
        let mut command = parse(&flags);
        command.selection_mut().project_version = Some("main".into());
        let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let (name, arguments) = command.wire(policy.content()).unwrap();
        assert_eq!(name, "graphit_task_session_revise");
        assert_eq!(arguments["selection"]["project_version"], "main");
        assert_eq!(
            arguments["input"],
            serde_json::json!({"target":{"id":ID,"claim":{"token":TOKEN,"expected_revision":7}},"lease_ms":30000,"reason":"Updated strategy","title":"Updated"})
        );
        let mut flags = vec![
            "revise-task",
            "--lease-ms",
            "1",
            "--reason",
            "Remove schedule",
            "--clear-schedule",
            "--clear-references",
            "--check-kind",
            "test",
            "--check-text",
            "First",
            "--check-kind",
            "acceptance",
            "--check-text",
            "Second",
        ];
        flags.extend(owned);
        let input = &wire(&flags)["input"];
        assert!(input["schedule"].is_null());
        assert_eq!(input["references"], serde_json::json!([]));
        assert_eq!(
            input["add_checks"],
            serde_json::json!([{"kind":"test","text":"First"},{"kind":"acceptance","text":"Second"}])
        );
        assert!(input.get("title").is_none());
        let mut links = vec![
            "update-task-links",
            "--lease-ms",
            "1",
            "--reason",
            "Clear links",
            "--clear-parent",
            "--clear-dependencies",
        ];
        links.extend(owned);
        let links = wire(&links);
        assert!(links["input"]["parent_id"].is_null());
        assert_eq!(links["input"]["depends_on"], serde_json::json!([]));
        let commit = wire(&[
            "set-task-commit",
            "--id",
            ID,
            "--expected-revision",
            "8",
            "--claim-token",
            TOKEN,
            "--claim-revision",
            "7",
            "--clear-commit",
            "--reason",
            "Clear association",
        ]);
        assert!(commit["input"]["commit"].is_null());
        assert_eq!(commit["input"]["expected_revision"], 8);
        assert_eq!(commit["input"]["claim"]["expected_revision"], 7);
        assert!(
            TaskCli::try_parse_from([
                "task",
                "revise-task",
                "--clear-schedule",
                "--due-date",
                "2026-10-29"
            ])
            .is_err()
        );
        assert!(
            TaskCli::try_parse_from(["task", "create-task", "--due-date", "2026-02-30"]).is_err()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reference_fields_are_paired_without_discarding_optional_fields() {
        let flags = [
            "create-session",
            "--idempotency-key",
            "refs",
            "--title",
            "Refs",
            "--description",
            "Records",
            "--strategy",
            "Use references",
            "--reference-type",
            "task",
            "--reference-id",
            ID,
            "--reference-relation",
            "relates_to",
            "--reference-type",
            "knowledge",
            "--reference-id",
            "design",
            "--reference-relation",
            "supports",
            "--reference-scope",
            "",
            "--reference-scope",
            "user",
            "--reference-scope-id",
            "",
            "--reference-scope-id",
            "owner",
            "--reference-context",
            "",
            "--reference-context",
            "notes",
            "--reference-field",
            "description",
            "--reference-field",
            "",
            "--reference-field-absent",
            "2",
        ];
        let references = &wire(&flags)["input"]["references"];
        assert_eq!(
            references,
            &serde_json::json!([
                {"target":{"type":"task","id":ID},"relation":"relates_to","field":"description"},
                {"target":{"type":"knowledge","id":"design","scope":"user","scope_id":"owner","context":"notes"},"relation":"supports"}
            ])
        );
        let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let literal = wire(&flags[..flags.len() - 2]);
        assert_eq!(literal["input"]["references"][1]["field"], "");
        assert!(
            parse(&flags[..flags.len() - 4])
                .wire(policy.content())
                .is_err()
        );
        for invalid_index in ["1", "3"] {
            let mut invalid = flags[..flags.len() - 2].to_vec();
            invalid.extend(["--reference-field-absent", invalid_index]);
            assert!(parse(&invalid).wire(policy.content()).is_err());
        }
        let mut duplicate = flags.to_vec();
        duplicate.extend(["--reference-field-absent", "2"]);
        assert!(parse(&duplicate).wire(policy.content()).is_err());
        let mut zero = flags[..flags.len() - 2].to_vec();
        zero.extend(["--reference-field-absent", "0"]);
        assert!(TaskCli::try_parse_from(std::iter::once("task").chain(zero)).is_err());
    }

    // Reuses the actual authenticated Task publication in the Role consumer fixture.
    pub(crate) fn assert_known_result_delivery(result: &CallToolResult) {
        let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel).unwrap();
        check_delivery(result, Ok(()), &original).unwrap();
        cancel.cancel();
        let error = check_delivery(result, Ok(()), &original).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        let value: Value = serde_json::from_str(&error.to_string()).unwrap();
        assert_eq!(value["result"], serde_json::to_value(result).unwrap());
        let error = check_delivery(
            result,
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Source cleanup failed",
            )),
            &original,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        let failure = error
            .get_ref()
            .unwrap()
            .downcast_ref::<TaskDeliveryFailure>()
            .unwrap();
        assert_eq!(
            failure.request.as_ref().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(
            failure.connection.as_ref().unwrap().kind(),
            io::ErrorKind::BrokenPipe
        );
        let value: Value = serde_json::from_str(&error.to_string()).unwrap();
        assert_eq!(value["result"], serde_json::to_value(result).unwrap());
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_millis(25);
        let original = RequestContext::new(policy.content(), &cancel, deadline).unwrap();
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        let error = check_delivery(result, Ok(()), &original).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        let value: Value = serde_json::from_str(&error.to_string()).unwrap();
        assert_eq!(value["result"], serde_json::to_value(result).unwrap());
    }
}
