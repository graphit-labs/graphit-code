//! Owned subprocess cases for process-custody tests; no product prompt parser.
use serde::Deserialize;
use std::{
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CHILD: &str = "--graphit-fixture-child";

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Process {
    args: Option<Vec<String>>,
    stdout: Option<String>,
    stderr: Option<String>,
    pid: Option<PathBuf>,
    ready: Option<PathBuf>,
    release: Option<PathBuf>,
    gate_before_output: bool,
    ignore_term: bool,
    term_marker: Option<PathBuf>,
    term_exit: Option<i32>,
    hold: bool,
    wait_stdin_eof: bool,
    stdin_eof_marker: Option<PathBuf>,
    expect_stdin: Option<String>,
    read_stdin_bytes: Option<usize>,
    prompt_file: Option<PromptFile>,
    child: Option<Box<Process>>,
    exit: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptFile {
    flag: String,
    expected: Option<String>,
    contains: Option<String>,
    permissions: Option<u32>,
    path_marker: Option<PathBuf>,
}

async fn wait(path: &Path) {
    while !path.exists() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[cfg(unix)]
fn term(ignore: bool, marker: Option<PathBuf>, exit: Option<i32>) -> io::Result<()> {
    if !(ignore || marker.is_some() || exit.is_some()) {
        return Ok(());
    }
    if !ignore && exit.is_none() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::spawn(async move {
        while signal.recv().await.is_some() {
            if let Some(marker) = &marker {
                std::fs::write(marker, b"TERM").unwrap();
            }
            if let Some(code) = exit {
                std::process::exit(code);
            }
        }
    });
    Ok(())
}

#[cfg(not(unix))]
fn term(ignore: bool, marker: Option<PathBuf>, exit: Option<i32>) -> io::Result<()> {
    if ignore || marker.is_some() || exit.is_some() {
        return Err(io::ErrorKind::Unsupported.into());
    }
    Ok(())
}

fn check_prompt(check: &PromptFile, args: &[String]) -> io::Result<()> {
    let index = args
        .iter()
        .position(|arg| arg == &check.flag)
        .ok_or(io::ErrorKind::InvalidInput)?;
    let path = args.get(index + 1).ok_or(io::ErrorKind::InvalidInput)?;
    let literal = std::fs::read_to_string(path)?;
    if let Some(expected) = &check.expected {
        assert!(literal == *expected, "raw fixture prompt mismatch");
    }
    if let Some(expected) = &check.contains {
        assert!(
            literal.contains(expected),
            "raw fixture prompt missing literal"
        );
    }
    #[cfg(unix)]
    if let Some(expected) = check.permissions {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path)?.permissions().mode() & 0o777,
            expected
        );
    }
    #[cfg(not(unix))]
    if check.permissions.is_some() {
        return Err(io::ErrorKind::Unsupported.into());
    }
    if let Some(marker) = &check.path_marker {
        std::fs::write(marker, path)?;
    }
    Ok(())
}

async fn output(stdout: Option<&str>, stderr: Option<&str>) -> io::Result<()> {
    if let Some(bytes) = stdout {
        let mut stream = tokio::io::stdout();
        stream.write_all(bytes.as_bytes()).await?;
        stream.flush().await?;
    }
    if let Some(bytes) = stderr {
        let mut stream = tokio::io::stderr();
        stream.write_all(bytes.as_bytes()).await?;
        stream.flush().await?;
    }
    Ok(())
}

pub(super) async fn run(mut process: Process) -> io::Result<i32> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let depth = args.iter().take_while(|arg| arg.as_str() == CHILD).count();
    if depth != 0 && depth != args.len() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    for _ in 0..depth {
        process = *process.child.take().ok_or(io::ErrorKind::InvalidInput)?;
    }
    if let Some(expected) = &process.args {
        assert!(args == *expected, "raw fixture arguments mismatch");
    }
    if let Some(check) = &process.prompt_file {
        check_prompt(check, &args)?;
    }
    term(process.ignore_term, process.term_marker, process.term_exit)?;
    if let Some(pid) = &process.pid {
        std::fs::write(pid, std::process::id().to_string())?;
    }
    // A std Child deliberately survives its parent's normal exit. The real
    // process group / Job owns its retirement and inherited pipe handles.
    let mut child = if let Some(config) = &process.child {
        let ready = config
            .ready
            .as_ref()
            .or(config.pid.as_ref())
            .ok_or(io::ErrorKind::InvalidInput)?;
        let child = Command::new(std::env::current_exe()?)
            .args(std::iter::repeat_n(CHILD, depth + 1))
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        wait(ready).await;
        Some(child)
    } else {
        None
    };
    if let Some(child) = &mut child
        && child.try_wait()?.is_some()
    {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    let mut input = tokio::io::stdin();
    if let Some(expected) = &process.expect_stdin {
        let mut bytes = vec![0; expected.len()];
        input.read_exact(&mut bytes).await?;
        assert!(bytes == expected.as_bytes(), "raw fixture stdin mismatch");
    }
    if let Some(maximum) = process.read_stdin_bytes {
        // Either a byte or EOF releases the abandoned-observer fixture.
        let _observed_count = input.read(&mut vec![0; maximum]).await?;
    }
    if process.gate_before_output {
        if let Some(ready) = &process.ready {
            std::fs::write(ready, b"ready")?;
        }
        if let Some(release) = &process.release {
            wait(release).await;
        }
    }
    output(
        process.stdout.as_deref(),
        if process.wait_stdin_eof {
            None
        } else {
            process.stderr.as_deref()
        },
    )
    .await?;
    if !process.gate_before_output {
        if let Some(ready) = &process.ready {
            std::fs::write(ready, b"ready")?;
        }
        if let Some(release) = &process.release {
            wait(release).await;
        }
    }
    if process.wait_stdin_eof {
        tokio::io::copy(&mut input, &mut tokio::io::sink()).await?;
        if let Some(marker) = &process.stdin_eof_marker {
            std::fs::write(marker, b"EOF")?;
        }
        output(None, process.stderr.as_deref()).await?;
    }
    if let Some(check) = &process.prompt_file {
        check_prompt(check, &args)?;
    }
    if process.hold {
        std::future::pending::<()>().await;
    }
    Ok(process.exit)
}
