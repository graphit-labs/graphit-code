use crate::source_client as support;
use clap::{Parser, Subcommand};
use graphit_app::ast_source::{SourceArguments, run_source, runtime_headers, source_exit_code};
use graphit_core::file_lock::Cancellation;
use std::path::PathBuf;
use std::{
    ffi::OsString,
    io,
    time::{Duration, Instant},
};
type TestResult = Result<(), Box<dyn std::error::Error>>;
#[derive(Debug, Parser)]
#[command(name = "graphit")]
struct SourceCommand {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    non_interactive: bool,
    #[arg(long, global = true)]
    project_version: Option<String>,
    #[command(subcommand)]
    command: AstCommand,
}
#[derive(Debug, Subcommand)]
enum AstCommand {
    Ast {
        #[command(subcommand)]
        command: SourceSubcommand,
    },
}
#[derive(Debug, Subcommand)]
enum SourceSubcommand {
    Source(SourceArguments),
}
impl SourceCommand {
    fn source(&self) -> &SourceArguments {
        let AstCommand::Ast {
            command: SourceSubcommand::Source(arguments),
        } = &self.command;
        arguments
    }
}
fn parse_source_arguments(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<SourceCommand, clap::Error> {
    let mut parsed = SourceCommand::try_parse_from(arguments)?;
    let AstCommand::Ast {
        command: SourceSubcommand::Source(source),
    } = &mut parsed.command;
    source.project_version.clone_from(&parsed.project_version);
    Ok(parsed)
}

fn parse(args: &[&str]) -> SourceCommand {
    parse_source_arguments(args.iter().map(OsString::from)).unwrap()
}

#[test]
fn native_clap_preserves_flags_and_rejects_invalid_arguments() {
    let input = parse(&[
        "graphit",
        "--config",
        "settings.json",
        "--non-interactive",
        "ast",
        "source",
        "no.extension",
        "--start",
        "-2",
        "--end",
        "4",
        "--pattern",
        "",
        "--regex",
        "--line-numbers",
        "--project-version",
        "feature",
    ]);
    assert_eq!(
        input.config.as_ref().unwrap().to_str(),
        Some("settings.json")
    );
    assert!(input.non_interactive);
    let source = input.source();
    assert_eq!(source.source.start_line, Some(-2));
    assert_eq!(source.source.end_line, Some(4));
    assert_eq!(source.source.pattern.as_deref(), Some(""));
    assert!(source.source.regex && source.source.line_numbers);
    for args in [
        vec!["graphit", "ast", "source"],
        vec!["graphit", "ast", "source", "x", "--head", "1.5"],
        vec!["graphit", "ast", "source", "x", "--json"],
        vec!["graphit", "ast", "source", "x", "y"],
    ] {
        assert_eq!(
            parse_source_arguments(args.into_iter().map(OsString::from))
                .unwrap_err()
                .exit_code(),
            2
        );
    }
    assert_eq!(
        parse_source_arguments(["graphit", "ast", "source", "--help"].map(OsString::from))
            .unwrap_err()
            .exit_code(),
        0
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        assert!(
            parse_source_arguments([
                OsString::from("graphit"),
                OsString::from("ast"),
                OsString::from("source"),
                OsString::from_vec(vec![255])
            ])
            .is_err()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_http_source_exact_text_empty_selectors_and_shared_key() -> TestResult {
    let fixture = support::Fixture::new();
    let endpoint = fixture.start().await;
    fixture.file(0, "raw.unknown", "\u{feff}A\r\nβ\0\r\nC");
    fixture.file(2, "raw.unknown", "second");
    fixture.file(0, "empty", "");
    fixture.file(0, "indexed", "disk");
    fixture
        .publish(0, &[("indexed", Some("")), ("null", None)])
        .await;
    fixture.file(0, "null", "fallback");
    let key = fixture.paths.global_path(&["daemon", "mcp.key"])?;
    let before = std::fs::read(&key)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(Duration::from_secs(30))
        .build()?;
    let cancel = Cancellation::default();
    let original = support::request(&cancel);
    for (flags, expected) in [
        (vec!["raw.unknown"], "\u{feff}A\r\nβ\0\r\nC"),
        (vec!["empty"], ""),
        (vec!["indexed"], ""),
        (vec!["null"], "fallback"),
        (vec!["raw.unknown", "--head", "1"], "\u{feff}A\r"),
        (vec!["raw.unknown", "--pattern", "missing"], ""),
        (
            vec!["raw.unknown", "--project-version", "feature"],
            "\u{feff}A\r\nβ\0\r\nC",
        ),
    ] {
        let mut args = vec!["graphit", "ast", "source"];
        args.extend(flags);
        let parsed = parse(&args);
        let mut output = Vec::new();
        run_source(
            &fixture.paths,
            endpoint.address,
            &client,
            parsed.source(),
            support::key_limits(),
            &original,
            &mut output,
        )
        .await?;
        assert_eq!(String::from_utf8(output)?, expected);
    }
    let dir = fixture.projects[2].0.root().to_str().unwrap();
    let parsed = parse(&[
        "graphit",
        "ast",
        "source",
        "raw.unknown",
        "--project-dir",
        dir,
        "--project-version",
        fixture.projects[2].2.version(),
    ]);
    let mut out = Vec::new();
    run_source(
        &fixture.paths,
        endpoint.address,
        &client,
        parsed.source(),
        support::key_limits(),
        &original,
        &mut out,
    )
    .await?;
    assert_eq!(out, b"second");
    assert_eq!(std::fs::read(key)?, before);
    for flags in [
        vec!["--entity", "MissingEntity"],
        vec!["--context", "vendor/missing@release-1"],
    ] {
        let mut args = vec!["graphit", "ast", "source", "raw.unknown"];
        args.extend(flags);
        let parsed = parse(&args);
        let mut output = Vec::new();
        let error = run_source(
            &fixture.paths,
            endpoint.address,
            &client,
            parsed.source(),
            support::key_limits(),
            &original,
            &mut output,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("unavailable"));
        assert!(output.is_empty());
    }
    assert!(endpoint.stop().await?);
    Ok(())
}

struct FailedWrite;
impl io::Write for FailedWrite {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn credentials_original_and_output_failures_never_deliver_payload() -> TestResult {
    let fixture = support::Fixture::new();
    fixture.file(0, "file", "payload");
    let endpoint = fixture.start().await;
    let parsed = parse(&["graphit", "ast", "source", "file"]);
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(Duration::from_secs(30))
        .build()?;
    let cancel = Cancellation::default();
    let original = support::request(&cancel);
    let headers = runtime_headers(&fixture.paths, support::key_limits(), &original).await?;
    let debug = format!("{:?}", headers);
    let key = fixture.paths.global_path(&["daemon", "mcp.key"])?;
    let before = std::fs::read(&key)?;
    let permissions = std::fs::metadata(&key)?.permissions();
    assert!(!debug.contains(std::str::from_utf8(&before)?));
    let error = run_source(
        &fixture.paths,
        endpoint.address,
        &client,
        parsed.source(),
        support::key_limits(),
        &original,
        &mut FailedWrite,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    assert_eq!(source_exit_code(&error), 1);
    for value in [b"bad".to_vec(), vec![b'0'; 64]] {
        std::fs::write(&key, value)?;
        let mut out = Vec::new();
        assert!(
            run_source(
                &fixture.paths,
                endpoint.address,
                &client,
                parsed.source(),
                support::key_limits(),
                &original,
                &mut out
            )
            .await
            .is_err()
        );
        assert!(out.is_empty());
    }
    std::fs::remove_file(&key)?;
    assert!(
        runtime_headers(&fixture.paths, support::key_limits(), &original)
            .await
            .is_err()
    );
    assert!(!key.exists());
    std::fs::write(&key, &before)?;
    // Preserve private attributes; recreating the file still conflicts with its owner receipt.
    std::fs::set_permissions(&key, permissions)?;
    let expired = support::request_until(&cancel, Instant::now() + Duration::from_millis(1));
    tokio::time::sleep(Duration::from_millis(3)).await;
    let mut out = Vec::new();
    assert_eq!(
        run_source(
            &fixture.paths,
            endpoint.address,
            &client,
            parsed.source(),
            support::key_limits(),
            &expired,
            &mut out
        )
        .await
        .unwrap_err()
        .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(out.is_empty());
    cancel.cancel();
    let error = run_source(
        &fixture.paths,
        endpoint.address,
        &client,
        parsed.source(),
        support::key_limits(),
        &original,
        &mut out,
    )
    .await
    .unwrap_err();
    assert_eq!(source_exit_code(&error), 130);
    assert!(out.is_empty());
    let cleanup = endpoint.stop().await.unwrap_err();
    let graphit_daemon::source_discovery::DiscoveryError::Runtime(runtime) = cleanup else {
        panic!("unexpected cleanup: {cleanup:?}");
    };
    assert_eq!(
        runtime,
        graphit_core::runtime_key::RuntimeKeyError::Conflict
    );
    Ok(())
}
