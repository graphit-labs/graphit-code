use graphit_core::{
    auth_document::AuthDocumentError,
    file_lock::Cancellation,
    startup_priority::{PriorityOutcome, lower_background_priority},
};
use graphit_daemon::{
    model_child::{ChildOptions, serve_child},
    model_service::ModelServiceError,
};
use std::{io, path::PathBuf};

fn main() -> std::process::ExitCode {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 1 && matches!(args[0].to_str(), Some("--help" | "-h" | "help")) {
        println!(
            "Graphit model service\n\nUsage: graphit-model-server <INSTALLATION>\n\nServe the daemon's authenticated model protocol on stdin/stdout.\nThis process is normally started by graphit daemon.\n\nOptions:\n  -h, --help  Show this help"
        );
        return std::process::ExitCode::SUCCESS;
    }
    if args.len() != 1
        || args[0]
            .to_str()
            .is_none_or(|s| s.is_empty() || s.starts_with('-'))
    {
        eprintln!(
            "error: model service requires an installation directory\n\nUsage: graphit-model-server <INSTALLATION>\n\nRun graphit-model-server --help for details."
        );
        return std::process::ExitCode::from(2);
    }
    let priority = lower_background_priority();
    for outcome in [priority.cpu(), priority.io()] {
        if let PriorityOutcome::Unavailable(error) = outcome {
            eprintln!("warning: model service startup priority: {error}");
        }
    }
    run()
}

#[tokio::main(flavor = "current_thread")]
async fn run() -> std::process::ExitCode {
    let result = async {
        let mut arguments = std::env::args_os().skip(1);
        let root = PathBuf::from(arguments.next().ok_or(io::ErrorKind::InvalidInput)?);
        if arguments.next().is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let lifetime = Cancellation::default();
        #[cfg(unix)]
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        let signal = async {
            #[cfg(unix)]
            {
                tokio::select! {
                    result = tokio::signal::ctrl_c() => result,
                    _ = terminate.recv() => Ok(()),
                }
            }
            #[cfg(not(unix))]
            tokio::signal::ctrl_c().await
        };
        let serving = serve_child(
            &root,
            ChildOptions::standard()?,
            tokio::io::stdin(),
            tokio::io::stdout(),
            lifetime.clone(),
        );
        tokio::pin!(serving);
        tokio::select! {
            result = &mut serving => result,
            received = signal => {
                lifetime.cancel();
                let result = serving.await;
                received?;
                result
            }
        }
    }
    .await;
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", failure_message(&error));
            std::process::ExitCode::FAILURE
        }
    }
}

fn failure_message(error: &io::Error) -> String {
    let message = format!(
        "error: model service startup/service failed: {:?}",
        error.kind()
    );
    if let Some(model) = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<ModelServiceError>())
    {
        format!("{message}; model=\"{:?}\"", model.kind())
    } else if let Some(auth) = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<AuthDocumentError>())
    {
        // Both diagnostics use existing unit variants, never arbitrary cause text.
        format!("{message}; auth_document=\"{auth:?}\"")
    } else {
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_diagnostics_distinguish_private_bootstrap_errors_without_source_text() {
        for (cause, expected) in [
            (AuthDocumentError::Denied, "Denied"),
            (AuthDocumentError::Conflict, "Conflict"),
            (AuthDocumentError::Io, "Io"),
        ] {
            assert_eq!(
                failure_message(&io::Error::other(cause)),
                format!(
                    "error: model service startup/service failed: Other; auth_document=\"{expected}\""
                )
            );
        }
        let unknown = io::Error::other("private-bootstrap-cause-canary");
        assert_eq!(
            failure_message(&unknown),
            "error: model service startup/service failed: Other"
        );
    }
}
