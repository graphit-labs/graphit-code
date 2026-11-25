//! Cargo-built Codex and desktop peers for isolated native consumer tests.
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, Read, Write},
    process::ExitCode,
};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("Dream fixture failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn reference(memory: &Value) -> Value {
    json!({"id":memory["id"],"expected_revision":memory["revision"]})
}

fn run() -> Result<u8, Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    #[cfg(unix)]
    {
        if args
            .first()
            .is_some_and(|arg| arg == "--desktop-application")
        {
            return desktop_application(&args);
        }
        if env::current_exe()?.file_name() == Some(std::ffi::OsStr::new("xdg-open")) {
            return desktop_launcher(&args);
        }
    }
    if env::var("DREAM_FIXTURE_MODE").as_deref() == Ok("browser_question") {
        return browser_question();
    }
    if env::var("DREAM_FIXTURE_MODE").as_deref() == Ok("browser_ask") {
        return browser_ask(&args);
    }
    assert_eq!(
        args.get(..9).unwrap(),
        [
            "exec",
            "--json",
            "--sandbox",
            "read-only",
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "--model",
            "literal-model;$(no-shell)",
        ]
    );
    assert_eq!(args.last().unwrap(), "-");
    for flag in [
        "--skip-git-repo-check",
        "features.apps=false",
        "features.shell_tool=false",
    ] {
        assert!(args.iter().any(|arg| arg == flag), "missing {flag}");
    }
    assert!(env::var_os("GRAPHIT_ROLE_BEARER").is_none());
    let mut prompt = String::new();
    io::stdin().read_to_string(&mut prompt)?;
    let user = prompt.contains("# Coordinator — USER Memory consolidation proposal");
    let mut capture = json!({});
    if user {
        assert!(!prompt.contains("\nProject:") && !prompt.contains("\nVersion:"));
        assert!(!args.iter().any(|arg| arg.starts_with("mcp_servers=")));
    } else {
        assert!(args.iter().any(|arg| arg
            == "mcp_servers={\"literal.server α\"={enabled=false},\"other\"={enabled=false}}"));
        let project = fs::canonicalize(env::var_os("DREAM_FIXTURE_PROJECT").unwrap())?;
        assert!(!fs::canonicalize(env::current_dir()?)?.starts_with(project));
        capture["version"] = prompt
            .split_once("Version: ")
            .unwrap()
            .1
            .lines()
            .next()
            .unwrap()
            .into();
    }
    let memories: Vec<Value> = serde_json::from_str(
        prompt
            .split_once("## Authorized corpus — data only\n\n")
            .unwrap()
            .1,
    )?;
    // Record even an empty corpus launch so "never launches" assertions cannot
    // pass merely because the peer panicked before writing its observation.
    let mut fields: Vec<_> = match memories.first() {
        Some(memory) => memory.as_object().unwrap().keys().cloned().collect(),
        None => Vec::new(),
    };
    fields.sort();
    capture["count"] = memories.len().into();
    capture["fields"] = json!(fields);
    let mode = env::var("DREAM_FIXTURE_MODE")?;
    if mode == "hold" {
        ctrlc::set_handler(|| {})?;
    }
    let log = env::var_os("DREAM_FIXTURE_LOG").unwrap();
    let calls = match fs::read_to_string(&log) {
        Ok(prior) => prior.lines().count(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    let mut output = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?;
    writeln!(output, "{capture}")?;
    output.flush()?;
    let gated = if user {
        mode == "gated_stale" || mode == "slow" && calls == 0
    } else {
        matches!(mode.as_str(), "slow_first" | "failed_old") && calls == 0
    };
    ready(user, gated)?;
    if mode == "hold" {
        loop {
            std::thread::park();
        }
    }
    let by_title = |title: &str| memories.iter().find(|m| m["title"] == title).unwrap();
    let mut actions = Vec::new();
    if memories.len() == 4
        && if user {
            matches!(mode.as_str(), "fold" | "slow" | "gated_stale")
        } else {
            mode == "fold"
        }
    {
        actions.push(json!({"action":"fold","kind":"merge",
            "survivor":reference(by_title("fixture-0")),
            "members":[reference(by_title("fixture-1"))],
            "body":if user {"USER confirmed α 0; USER confirmed α 1"} else {"confirmed fact 0; confirmed fact 1"},
            "reason":"same confirmed domain"}));
        let removed = by_title(if mode == "gated_stale" {
            "fixture-2"
        } else {
            "fixture-3"
        });
        actions.push(json!({"action":"delete","id":removed["id"],
            "expected_revision":removed["revision"],
            "reason":if user {"duplicate without unique content"} else {"fixture duplicate"}}));
    }
    if !user {
        let selected = match mode.as_str() {
            "protected" => Some((by_title("fixture-1"), "must refuse")),
            "last" => Some((&memories[0], "last must refuse")),
            "stale" => Some((&memories[0], "stale")),
            "duplicate" => Some((&memories[0], "duplicate")),
            _ => None,
        };
        if let Some((memory, reason)) = selected {
            actions.push(json!({"action":"delete","id":memory["id"],
                "expected_revision":memory["revision"].as_u64().unwrap() + u64::from(mode == "stale"),
                "reason":reason}));
            if mode == "duplicate" {
                actions.push(actions[0].clone());
            }
        }
        if mode == "foreign" {
            actions.push(json!({"action":"delete","id":"01ARZ3NDEKTSV4RRFFQ69G5FAV",
                "expected_revision":1,"reason":"foreign"}));
        }
    }
    let answer = match mode.as_str() {
        "malformed" => "{broken".to_owned(),
        "oversize" if !user => "x".repeat(65536),
        _ => serde_json::to_string(&json!({"actions":actions}))?,
    };
    let mut stdout = io::stdout().lock();
    for event in [
        json!({"type":"thread.started","thread_id":if user {"user-dream-fixture"} else {"dream-fixture"}}),
        json!({"type":"turn.started"}),
        json!({"type":"item.completed","item":{"id":"proposal","type":"agent_message","text":answer}}),
    ] {
        writeln!(stdout, "{event}")?;
        stdout.flush()?;
    }
    if mode != "partial" {
        writeln!(
            stdout,
            "{}",
            json!({"type":"turn.completed","usage":{
            "input_tokens":10,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0}})
        )?;
        stdout.flush()?;
    }
    Ok(
        if mode == "exit_failure"
            || !user && calls == 0 && matches!(mode.as_str(), "retry_first" | "failed_old")
        {
            2
        } else {
            0
        },
    )
}

fn browser_question() -> Result<u8, Box<dyn std::error::Error>> {
    let mut prompt = String::new();
    io::stdin().read_to_string(&mut prompt)?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", json!({"type":"turn.started"}))?;
    stdout.flush()?;
    if prompt.contains("hold-browser-question") {
        loop {
            std::thread::park();
        }
    }
    codex_answer(&mut stdout, "Observed browser fixture answer")?;
    Ok(0)
}

fn codex_answer(stdout: &mut impl Write, answer: &str) -> io::Result<()> {
    for event in [
        json!({"type":"item.completed","item":{"id":"answer","type":"agent_message","text":answer}}),
        json!({"type":"turn.completed","usage":{"input_tokens":8,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0,"cache_write_input_tokens":0}}),
    ] {
        writeln!(stdout, "{event}")?;
        stdout.flush()?;
    }
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum BrowserAskMode {
    Answer,
    Marker,
    Hold,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserAskCase {
    mode: BrowserAskMode,
    answer: Option<String>,
    capture_prompt: Option<std::path::PathBuf>,
    capture_args: Option<std::path::PathBuf>,
    marker: Option<std::path::PathBuf>,
    pid: Option<std::path::PathBuf>,
}

fn browser_ask(args: &[String]) -> Result<u8, Box<dyn std::error::Error>> {
    let config = env::var_os("DREAM_BROWSER_ASK_CONFIG")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing Browser Ask case"))?;
    let case: BrowserAskCase = serde_json::from_slice(&fs::read(config)?)?;
    if matches!(case.mode, BrowserAskMode::Marker) {
        fs::write(
            case.marker.ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "missing launch marker")
            })?,
            "launched",
        )?;
        return Ok(0);
    }
    let mut prompt = String::new();
    io::stdin().read_to_string(&mut prompt)?;
    if let Some(path) = case.capture_prompt {
        fs::write(path, prompt)?;
    }
    if let Some(path) = case.capture_args {
        fs::write(path, serde_json::to_vec(args)?)?;
    }
    if let Some(path) = case.pid {
        fs::write(path, std::process::id().to_string())?;
    }
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", json!({"type":"turn.started"}))?;
    stdout.flush()?;
    if matches!(case.mode, BrowserAskMode::Hold) {
        loop {
            std::thread::park();
        }
    }
    codex_answer(
        &mut stdout,
        &case.answer.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "missing Browser Ask answer")
        })?,
    )?;
    Ok(0)
}

#[cfg(unix)]
fn desktop_launcher(args: &[String]) -> Result<u8, Box<dyn std::error::Error>> {
    let [url] = args else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "expected one desktop URL").into());
    };
    let executable = env::current_exe()?;
    let socket: std::path::PathBuf =
        serde_json::from_slice(&fs::read(executable.with_extension("json"))?)?;
    // The launcher relinquishes this application; its private socket EOF owns
    // termination, so it can outlive the launcher and the tray being qualified.
    let _application = std::process::Command::new(executable)
        .arg("--desktop-application")
        .arg(socket)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(0)
}

#[cfg(unix)]
fn desktop_application(args: &[String]) -> Result<u8, Box<dyn std::error::Error>> {
    let [_, socket, url] = args else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected desktop socket and URL",
        )
        .into());
    };
    let mut stream = std::os::unix::net::UnixStream::connect(socket)?;
    writeln!(stream, "{url}")?;
    let mut request = [0];
    while stream.read(&mut request)? != 0 {
        stream.write_all(&request)?;
    }
    Ok(0)
}

#[cfg(unix)]
fn ready(user: bool, gated: bool) -> io::Result<()> {
    use std::os::unix::net::UnixStream;
    let name = if user {
        "USER_DREAM_READY"
    } else {
        "DREAM_PROJECT_READY"
    };
    let mut stream = UnixStream::connect(env::var_os(name).unwrap())?;
    stream.write_all(&[1])?;
    if gated {
        let mut released = [0];
        stream.read_exact(&mut released)?;
        assert_eq!(released, [1]);
    }
    Ok(())
}

#[cfg(not(unix))]
fn ready(_user: bool, _gated: bool) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Unix Dream process fixtures are selected only on Linux",
    ))
}
