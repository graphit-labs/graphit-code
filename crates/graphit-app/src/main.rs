mod build_metadata;
mod cli;
mod package;
mod setup;
mod version;

use build_metadata::Metadata;
use std::process::ExitCode;
use tracing_subscriber::{filter::LevelFilter, fmt::format::FmtSpan, prelude::*};

fn main() -> ExitCode {
    // A supervised copy process keeps finite hook stdin cancellable on every OS.
    // Run before configuration, logging, runtimes and signal handlers are initialized.
    if std::env::args_os()
        .skip(1)
        .eq([graphit_app::git_hook_commands::INPUT_COPY_COMMAND])
    {
        use std::io::Write;
        let mut output = std::io::stdout().lock();
        return match std::io::copy(&mut std::io::stdin().lock(), &mut output)
            .and_then(|_| output.flush())
        {
            Ok(_) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Git hook stdin: {error}");
                ExitCode::FAILURE
            }
        };
    }

    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(LevelFilter::OFF.into())
        .try_from_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off"));
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(false)
                .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE),
        )
        .init();
    let metadata = match Metadata::new(
        env!("GRAPHIT_COMPILED_BUILD_BRAND"),
        env!("GRAPHIT_COMPILED_BUILD_VERSION"),
    ) {
        Ok(metadata) => metadata,
        Err(error) => {
            let error = version::CliError::Metadata(error);
            let _ = version::write_diagnostic(&error, std::io::stderr());
            return ExitCode::FAILURE;
        }
    };
    cli::run(std::env::args_os().skip(1), &metadata)
}
