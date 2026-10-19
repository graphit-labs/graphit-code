use graphit_core::{
    file_lock::Cancellation,
    startup_priority::{PriorityOutcome, lower_background_priority},
};
use graphit_daemon::{
    model_child::{ChildOptions, serve_child},
    model_service::ModelServiceError,
};
use std::{io, path::PathBuf};

fn main() -> std::process::ExitCode {
    let priority = lower_background_priority();
    for outcome in [priority.cpu(), priority.io()] {
        if let PriorityOutcome::Unavailable(error) = outcome {
            eprintln!("model child startup priority: {error}");
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
            if let Some(model) = error
                .get_ref()
                .and_then(|cause| cause.downcast_ref::<ModelServiceError>())
            {
                // ModelErrorKind contains only unit variants; no source text or credential is logged.
                eprintln!(
                    "model child startup/service failed: {:?}; model=\"{:?}\"",
                    error.kind(),
                    model.kind()
                );
            } else {
                eprintln!("model child startup/service failed: {:?}", error.kind());
            }
            std::process::ExitCode::FAILURE
        }
    }
}
