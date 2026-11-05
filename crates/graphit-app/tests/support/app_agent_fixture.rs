//! Native provider protocol fixture. No shell, interpreter, vendor executable or account.
mod agent_conversation_peer;
mod agent_question_peer;
mod agent_raw_peer;
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{self, Read, Write},
    net::{Ipv4Addr, TcpStream},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Protocol {
    Codex,
    Claude,
    Cursor,
    Qwen,
    Grok,
    Antigravity,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Checks {
    args: Option<Vec<String>>,
    model: Option<String>,
    captured: Option<String>,
    cwd_basename: Option<String>,
    question: Option<String>,
    slug: Option<String>,
    body: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Gate {
    before: usize,
    #[serde(default)]
    send: String,
    expect: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Scenario {
    Wire {
        protocol: Protocol,
        frames: Vec<String>,
        #[serde(default)]
        checks: Box<Checks>,
        gate: Option<Gate>,
        marker: Option<PathBuf>,
        #[serde(default)]
        exit: i32,
    },
    Conversation {
        protocol: Protocol,
    },
    Kimi {
        program: PathBuf,
        test: String,
    },
    Raw {
        process: Box<agent_raw_peer::Process>,
    },
    Question {
        mode: agent_question_peer::Mode,
        expected_body: String,
    },
}

fn emit(raw: &str) -> io::Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(raw.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}
fn emit_value(value: Value) -> io::Result<()> {
    emit(&value.to_string())
}
fn append(path: impl AsRef<Path>, value: Value) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{value}")
}
fn suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    value.into()
}
fn read_prompt(protocol: Protocol) -> io::Result<(Value, Vec<String>, String)> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let (raw, model) = match protocol {
        Protocol::Cursor => {
            assert!(args.len() == 8 || args.len() == 10);
            let raw = args.pop().unwrap();
            assert_eq!(
                &args[..4],
                ["--print", "--output-format", "stream-json", "--model"]
            );
            assert_eq!(&args[5..7], ["--mode", "ask"]);
            (raw, args[4].clone())
        }
        Protocol::Grok => {
            assert!(args.len() == 9 || args.len() == 11);
            assert_eq!(args[0], "--prompt-file");
            assert_eq!(
                &args[2..5],
                ["--output-format", "streaming-json", "--model"]
            );
            assert_eq!(&args[6..9], ["--verbatim", "--deny", "*"]);
            assert!(input.is_empty());
            let path = Path::new(&args[1]);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(std::fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
            }
            let raw = std::fs::read_to_string(path)?;
            std::fs::write(
                std::env::current_exe()?
                    .parent()
                    .unwrap()
                    .join("prompt-path"),
                args[1].as_bytes(),
            )?;
            (raw, args[5].clone())
        }
        Protocol::Claude => {
            assert!(args.len() == 7 || args.len() == 9);
            assert_eq!(
                &args[..6],
                [
                    "-p",
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    "--include-partial-messages",
                    "--model"
                ]
            );
            (input, args[6].clone())
        }
        Protocol::Qwen => {
            assert!(args.len() == 7 || args.len() == 9);
            assert_eq!(
                &args[..6],
                [
                    "--input-format",
                    "text",
                    "--output-format",
                    "stream-json",
                    "--include-partial-messages",
                    "--model"
                ]
            );
            (input, args[6].clone())
        }
        Protocol::Antigravity => {
            assert!(args.len() == 6 || args.len() == 8);
            assert_eq!(
                &args[..5],
                [
                    "--input-format",
                    "stream-json",
                    "--output-format",
                    "stream-json",
                    "--model"
                ]
            );
            let wire: Value = serde_json::from_str(&input)?;
            assert_eq!(wire["event"], "user");
            (
                wire["message"]["content"].as_str().unwrap().to_owned(),
                args[5].clone(),
            )
        }
        Protocol::Codex => (input, String::new()),
    };
    Ok((serde_json::from_str(&raw)?, args, model))
}
fn gate(gate: &Gate) -> io::Result<()> {
    let port: u16 = std::env::var("GATE_PORT").unwrap().parse().unwrap();
    let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
    socket.write_all(gate.send.as_bytes())?;
    if let Some(expected) = &gate.expect {
        let mut bytes = vec![0; expected.len()];
        socket.read_exact(&mut bytes)?;
        assert_eq!(bytes, expected.as_bytes());
    } else {
        let _ = socket.read(&mut [0])?;
    }
    Ok(())
}
fn wire(
    protocol: Protocol,
    frames: Vec<String>,
    checks: Checks,
    controlled: Option<Gate>,
    marker: Option<PathBuf>,
    exit: i32,
) -> io::Result<i32> {
    if let Some(marker) = marker {
        std::fs::write(marker, b"child")?;
        return Ok(exit);
    }
    if let Some(expected) = checks.args {
        let actual: Vec<_> = std::env::args().skip(1).collect();
        assert!(actual == expected, "wire fixture arguments mismatch");
    }
    let (prompt, _, model) = read_prompt(protocol)?;
    if let Some(expected) = checks.model {
        assert_eq!(model, expected);
    }
    if let Some(expected) = checks.captured {
        assert_eq!(std::env::var("CAPTURED").unwrap(), expected);
    }
    if let Some(expected) = checks.cwd_basename {
        assert_eq!(
            std::env::current_dir()?.file_name().unwrap(),
            expected.as_str()
        );
    }
    if let Some(expected) = checks.question {
        assert_eq!(prompt["question"], expected);
    }
    if let Some(expected) = checks.slug {
        assert_eq!(prompt["pages"][0]["slug"], expected);
    }
    if let Some(expected) = checks.body {
        assert_eq!(prompt["pages"][0]["body"], expected);
    }
    for (index, frame) in frames.iter().enumerate() {
        if let Some(controlled) = controlled.as_ref().filter(|gate| gate.before == index) {
            gate(controlled)?;
        }
        emit(frame)?;
    }
    Ok(exit)
}
fn main() -> std::process::ExitCode {
    let result = (|| -> io::Result<i32> {
        let file = std::env::current_exe()?.with_extension("fixture.json");
        let scenario: Scenario = serde_json::from_slice(&std::fs::read(file)?)?;
        match scenario {
            Scenario::Wire {
                protocol,
                frames,
                checks,
                gate,
                marker,
                exit,
            } => wire(protocol, frames, *checks, gate, marker, exit),
            Scenario::Conversation { protocol } => tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(agent_conversation_peer::run(protocol)),
            Scenario::Raw { process } => tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(agent_raw_peer::run(*process)),
            Scenario::Question {
                mode,
                expected_body,
            } => tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(agent_question_peer::run(mode, expected_body)),
            Scenario::Kimi { program, test } => {
                let args: Vec<_> = std::env::args().skip(1).collect();
                assert_eq!(
                    args,
                    [
                        "web",
                        "--no-open",
                        "--host",
                        "127.0.0.1",
                        "--port",
                        "0",
                        "--log-level",
                        "error"
                    ]
                );
                if let Some(path) = std::env::var_os("KIMI_NATIVE_ARGS") {
                    std::fs::write(path, format!("{}\n", args.join("\n")))?;
                }
                assert!(program.is_absolute() && program.is_file());
                let mut command = std::process::Command::new(program);
                command.args([
                    "--ignored",
                    "--exact",
                    &test,
                    "--nocapture",
                    "--format",
                    "terse",
                ]);
                #[cfg(unix)]
                {
                    use std::os::unix::process::CommandExt;
                    Err(command.exec())
                }
                #[cfg(not(unix))]
                {
                    Ok(command.status()?.code().unwrap_or(1))
                }
            }
        }
    })();
    match result {
        Ok(code) => std::process::ExitCode::from(u8::try_from(code).unwrap()),
        Err(error) => {
            eprintln!("agent fixture failed: {:?}", error.kind());
            std::process::ExitCode::FAILURE
        }
    }
}
