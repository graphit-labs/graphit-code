//! Decode only the native selectors needed by the existing authenticated caller.
use super::{Arguments, Engine, binding_event};
use graphit_hub::policy::RequestContext;
use serde::Deserialize;
use std::{
    io,
    path::{Component, Path, PathBuf},
};
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Default, Deserialize)]
struct CommandInput {
    hook_event_name: Option<String>,
    cwd: Option<PathBuf>,
}
#[derive(Default, Deserialize)]
struct CursorInput {
    hook_event_name: Option<String>,
    #[serde(default)]
    workspace_roots: Vec<PathBuf>,
    cwd: Option<PathBuf>,
}
#[derive(Default, Deserialize)]
struct AntigravityInput {
    #[serde(rename = "workspacePaths", default)]
    workspace_paths: Vec<PathBuf>,
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Native hook input invalid")
}
fn valid_path(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
}
/// Native IDs and unneeded tool/prompt/transcript fields are deliberately ignored.
pub(super) fn decode(arguments: &mut Arguments, bytes: &[u8]) -> io::Result<()> {
    let (binding, event) = binding_event(arguments.engine, arguments.event);
    let native = event.native_name(binding).map_err(|_| invalid())?;
    if bytes.iter().all(u8::is_ascii_whitespace) {
        // An explicit invocation remains usable with closed stdin.
        return Ok(());
    }
    let (name, cwd, roots) = match arguments.engine {
        Engine::Cursor => {
            let input: CursorInput = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            (input.hook_event_name, input.cwd, input.workspace_roots)
        }
        Engine::Antigravity => {
            let input: AntigravityInput = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            (None, None, input.workspace_paths)
        }
        _ => {
            let input: CommandInput = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            (input.hook_event_name, input.cwd, Vec::new())
        }
    };
    if name.as_deref().is_some_and(|name| name != native)
        || cwd.as_deref().is_some_and(|path| !valid_path(path))
        || roots.iter().any(|path| !valid_path(path))
    {
        return Err(invalid());
    }
    if let Some(selected) = &arguments.project_dir {
        if !valid_path(selected)
            || cwd
                .as_deref()
                .is_some_and(|path| !path.starts_with(selected))
            || (!roots.is_empty() && !roots.iter().any(|path| path == selected))
        {
            return Err(invalid());
        }
    } else {
        arguments.project_dir = match (cwd, roots.as_slice()) {
            (Some(cwd), _) => Some(cwd),
            (None, [root]) => Some(root.clone()),
            (None, []) => None,
            _ => return Err(invalid()),
        };
    }
    Ok(())
}
/// Read one bounded official JSON input under the caller's existing deadline.
pub async fn read(
    arguments: &mut Arguments,
    input: impl AsyncRead + Unpin,
    original: &RequestContext<'_>,
) -> io::Result<()> {
    let deadline = original.snapshot().required_deadline()?;
    let (binding, event) = binding_event(arguments.engine, arguments.event);
    event.native_name(binding).map_err(|_| invalid())?;
    original.check()?;
    let maximum = original.content_limits().max_input_bytes;
    let mut bytes = Vec::new();
    let mut input = input
        .take(u64::try_from(maximum.checked_add(1).ok_or_else(invalid)?).map_err(|_| invalid())?);
    tokio::select! {
        result = input.read_to_end(&mut bytes) => { result?; },
        () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
    }
    original.check()?;
    if bytes.len() > maximum {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    decode(arguments, &bytes)?;
    original.check()
}

/// Use Tokio's nonblocking pipe receiver for native host input on Unix.
/// Tokio's stdio reader uses an uncancellable blocking worker for an open pipe.
pub async fn read_stdin(
    arguments: &mut Arguments,
    original: &RequestContext<'_>,
) -> io::Result<()> {
    use std::io::IsTerminal;
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return read(arguments, tokio::io::empty(), original).await;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        let file = std::fs::File::from(rustix::io::dup(&stdin)?);
        if file.metadata()?.file_type().is_fifo() {
            let input = tokio::net::unix::pipe::Receiver::from_file(file)?;
            read(arguments, input, original).await
        } else {
            read(arguments, tokio::fs::File::from_std(file), original).await
        }
    }
    #[cfg(not(unix))]
    read(arguments, tokio::io::stdin(), original).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_hook_entry::{Event, Recipient};
    use graphit_core::file_lock::Cancellation;
    use graphit_hub::native::ContentLimits;
    use serde_json::json;
    use std::time::{Duration, Instant};
    fn arguments(engine: Engine, event: Event, root: Option<PathBuf>) -> Arguments {
        Arguments {
            recipient: Recipient::Coordinator,
            engine,
            event,
            project_dir: root,
            task_session: None,
            task_ids: Vec::new(),
        }
    }
    fn limits() -> ContentLimits {
        ContentLimits {
            max_input_bytes: 4096,
            max_output_bytes: 4096,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 128,
        }
    }
    #[test]
    fn native_fields_select_the_captured_project_without_importing_host_identity() {
        let root = std::env::current_dir().unwrap();
        for engine in Engine::ALL {
            let event = Event::ALL
                .into_iter()
                .find(|event| {
                    event
                        .semantic()
                        .accepts_context(engine.binding())
                        .is_ok_and(|v| v)
                })
                .unwrap();
            let native = event.semantic().native_name(engine.binding()).unwrap();
            let payload = match engine {
                Engine::Cursor => {
                    json!({"workspace_roots":[root],"hook_event_name":native,"conversation_id":"not-a-Graphit-session"})
                }
                Engine::Antigravity => {
                    json!({"workspacePaths":[root],"conversationId":"not-a-Graphit-session","invocationNum":0})
                }
                Engine::OpenCode => {
                    json!({"cwd":root,"hook_event_name":native,"sessionID":"not-a-Graphit-session"})
                }
                _ => {
                    json!({"cwd":root,"hook_event_name":native,"session_id":"not-a-Graphit-session","prompt":"ignored body","future_optional":{"value":true}})
                }
            };
            let mut args = arguments(engine, event, Some(root.clone()));
            decode(&mut args, &serde_json::to_vec(&payload).unwrap()).unwrap();
            assert_eq!(args.project_dir.as_ref(), Some(&root));
            assert!(args.task_session.is_none());
            assert!(args.task_ids.is_empty());
        }
    }
    #[test]
    fn mismatched_event_foreign_project_ambiguous_roots_and_bad_json_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let foreign = temp.path().join("foreign");
        for payload in [
            json!({"hook_event_name":"UserPromptSubmit","cwd":root}),
            json!({"hook_event_name":"SessionStart","cwd":foreign}),
            json!({"cwd":"relative"}),
            json!(null),
        ] {
            let mut args = arguments(Engine::Claude, Event::SessionStart, Some(root.clone()));
            assert_eq!(
                decode(&mut args, &serde_json::to_vec(&payload).unwrap())
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
        }
        let mut cursor = arguments(Engine::Cursor, Event::SessionStart, None);
        assert!(
            decode(
                &mut cursor,
                &serde_json::to_vec(&json!({"workspace_roots":[root,foreign]})).unwrap()
            )
            .is_err()
        );
        let mut unsupported = arguments(Engine::Antigravity, Event::SessionStart, Some(root));
        assert!(decode(&mut unsupported, b"{}").is_err());
        assert!(decode(&mut cursor, b"{invalid").is_err());
    }
    #[tokio::test]
    async fn input_bytes_and_pending_reads_keep_the_original_limit_and_cancellation() {
        let cancel = Cancellation::default();
        let original =
            RequestContext::new(limits(), &cancel, Instant::now() + Duration::from_secs(5))
                .unwrap();
        let mut args = arguments(Engine::Kiro, Event::SessionStart, None);
        let oversized = vec![b' '; 4097];
        assert_eq!(
            read(&mut args, oversized.as_slice(), &original)
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
        let (input, _held) = tokio::io::duplex(16);
        cancel.cancel();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(8), read(&mut args, input, &original))
                .await
                .expect("cancelled hook input remained pending")
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }
}
