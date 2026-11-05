use super::{Protocol, append, emit, emit_value, read_prompt, suffix};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

async fn wait(path: &Path) {
    while !path.exists() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
fn turn_gate(home: &Path, query: &str) -> PathBuf {
    let key = Sha256::digest(query.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    home.join(key)
}
#[cfg(unix)]
fn retain_term(marker: PathBuf, contents: &'static [u8]) -> io::Result<()> {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::spawn(async move {
        while term.recv().await.is_some() {
            std::fs::write(&marker, contents).unwrap();
        }
    });
    Ok(())
}
#[cfg(not(unix))]
fn retain_term(_: PathBuf, _: &'static [u8]) -> io::Result<()> {
    Ok(())
}

pub(super) async fn run(protocol: Protocol) -> io::Result<i32> {
    let (prompt, args, model) = read_prompt(protocol)?;
    let query = prompt["question"].as_str().unwrap();
    let home = std::env::current_exe()?.parent().unwrap().to_owned();
    let prefix = match protocol {
        Protocol::Codex => None,
        Protocol::Claude => Some("CLAUDE"),
        Protocol::Cursor => Some("CURSOR"),
        Protocol::Qwen => Some("QWEN"),
        Protocol::Grok => Some("GROK"),
        Protocol::Antigravity => Some("AGY"),
    };
    let mut session = if protocol == Protocol::Cursor {
        "cursor-native-observed"
    } else {
        "00000000-0000-4000-8000-000000000001"
    };
    let release = prefix.map(|p| PathBuf::from(std::env::var_os(format!("{p}_RELEASE")).unwrap()));
    if let Some(release) = &release {
        retain_term(suffix(release, ".term"), b"term")?;
        assert_eq!(model, "fixture-model");
    }
    let journal_args = if protocol == Protocol::Grok {
        &args[2..]
    } else {
        args.as_slice()
    };
    if protocol == Protocol::Codex && query == "cancel-http" {
        retain_term(home.join("http-cancel-term"), b"actual SIGTERM")?;
    }
    // Enforce child ordering at the peer boundary as well as observing the
    // parent's admission. A second child cannot run before the held turn is
    // explicitly released, regardless of when the parent task is scheduled.
    if protocol == Protocol::Codex && query == "second" {
        assert!(suffix(&turn_gate(&home, "wait first"), ".release").exists());
    }
    if protocol == Protocol::Codex && query == "after cancellation" {
        assert!(home.join("http-cancel-release").exists());
    }
    if let Some(prefix) = prefix {
        let base: &[&str] = match protocol {
            Protocol::Claude => &[
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                "--model",
                "fixture-model",
            ],
            Protocol::Qwen => &[
                "--input-format",
                "text",
                "--output-format",
                "stream-json",
                "--include-partial-messages",
                "--model",
                "fixture-model",
            ],
            Protocol::Cursor => &[
                "--print",
                "--output-format",
                "stream-json",
                "--model",
                "fixture-model",
                "--mode",
                "ask",
            ],
            Protocol::Grok => &[
                "--output-format",
                "streaming-json",
                "--model",
                "fixture-model",
                "--verbatim",
                "--deny",
                "*",
            ],
            Protocol::Antigravity => &[
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--model",
                "fixture-model",
            ],
            Protocol::Codex => unreachable!(),
        };
        let mut resumed = base
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        resumed.extend([
            if protocol == Protocol::Antigravity {
                "--conversation"
            } else {
                "--resume"
            }
            .into(),
            session.into(),
        ]);
        assert!(journal_args == base || journal_args == resumed);
        if let Some(index) = journal_args
            .iter()
            .position(|a| a == "--resume" || a == "--conversation")
        {
            assert_eq!(journal_args[index + 1], session);
        }
        let mut record = json!({"args":journal_args,"query":query,"cwd":std::env::current_dir()?,
            "captured":std::env::var(format!("{prefix}_CAPTURED")).ok(),"pid":std::process::id()});
        if protocol == Protocol::Grok {
            record["prompt_file"] = json!(args[1]);
        }
        append(
            std::env::var_os(format!("{prefix}_JOURNAL")).unwrap(),
            record,
        )?;
    } else {
        append(
            home.join("launches.jsonl"),
            json!({"args":args,"query":query}),
        )?;
    }
    if protocol == Protocol::Codex {
        let native = if query.contains("mismatch") {
            "different-private-native"
        } else {
            "private-fixture-native"
        };
        if !query.contains("missing") {
            emit_value(json!({"type":"thread.started","thread_id":native}))?;
        }
        emit_value(json!({"type":"turn.started"}))?;
        if query.contains("malformed") {
            emit("{")?;
            return Ok(3);
        }
        if query == "cancel-http" {
            wait(&home.join("http-cancel-release")).await;
        }
        if query.contains("wait") {
            let gate = turn_gate(&home, query);
            std::fs::write(suffix(&gate, ".ready"), b"native turn waiting")?;
            wait(&suffix(&gate, ".release")).await;
        }
        let line = json!({"type":"item.completed","item":{"id":"answer","type":"agent_message","text":"Paris β [[Paris]]"}}).to_string()+"\n";
        let mut out = std::io::stdout().lock();
        for byte in line.bytes() {
            out.write_all(&[byte])?;
            out.flush()?;
        }
        drop(out);
        emit_value(
            json!({"type":"turn.completed","usage":{"input_tokens":7,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0}}),
        )?;
        return Ok(if query.contains("nonzero") { 7 } else { 0 });
    }
    if query == "mismatch" {
        session = if protocol == Protocol::Cursor {
            "cursor-native-mismatch"
        } else {
            "00000000-0000-4000-8000-000000000002"
        };
    }
    if protocol == Protocol::Grok {
        if query == "wait for release" {
            emit_value(json!({"type":"text","data":"grok native progress"}))?;
            wait(release.as_ref().unwrap()).await;
            assert!(Path::new(&args[1]).exists());
        } else {
            emit_value(json!({"type":"text","data":"Paris answer"}))?;
        }
        if query == "malformed" {
            emit("{bad native frame")?;
            return Ok(0);
        }
        if query == "missing end" {
            return Ok(0);
        }
        if query == "native error" {
            emit_value(json!({"type":"error","message":"native failure"}))?;
            return Ok(0);
        }
        if query == "invalid session" {
            session = "Session title";
        }
        let mut end =
            json!({"type":"end","stopReason":"end_turn","sessionId":session,"requestId":""});
        if query == "missing session" {
            end.as_object_mut().unwrap().remove("sessionId");
        }
        emit_value(end)?;
        return Ok(if query == "nonzero" { 7 } else { 0 });
    }
    if protocol == Protocol::Antigravity {
        assert_eq!(
            prompt["pages"][0]["body"],
            "---\ntitle: Capital β\n---\nParis é a capital.\n"
        );
        if query != "missing init" {
            emit_value(
                json!({"event":"init","conversation_id":session,"init":{"model":"observed-native","permission_mode":"request-review"}}),
            )?;
        }
    } else if query != "missing init" || protocol == Protocol::Claude {
        let mut init = json!({"type":"system","subtype":"init","session_id":session,"model":"observed-native"});
        if protocol == Protocol::Qwen {
            init["uuid"] = json!("init-native");
        }
        emit_value(init)?;
    }
    if query == "malformed" {
        emit("{bad native frame")?;
        return Ok(0);
    }
    if protocol == Protocol::Antigravity {
        emit_value(
            json!({"event":"step_update","step_update":{"conversation_id":session,"step_index":4,"state":"ACTIVE","step_type":"agent_response","text_delta":"antigravity native progress"}}),
        )?;
    }
    if query == "wait for release" {
        wait(release.as_ref().unwrap()).await;
    }
    if query == "missing result" {
        return Ok(0);
    }
    if protocol == Protocol::Antigravity {
        let mut result = json!({"conversation_id":session,"status":if query=="native error" {"ERROR"}else{"SUCCESS"},
            "response":"Paris answer","num_turns":13,"usage":{"input_tokens":12345,"output_tokens":678,"total_tokens":13023},"duration_seconds":42.125});
        if query == "native error" {
            result["error"] = json!("native failure");
        }
        if query == "result mismatch" {
            result["conversation_id"] = json!("00000000-0000-4000-8000-000000000002");
        }
        emit_value(json!({"event":"result","result":result}))?;
    } else {
        let mut result = json!({"type":"result","subtype":if query=="native error" {"error"}else{"success"},
            "duration_ms":1,"duration_api_ms":1,"is_error":query=="native error","session_id":session,"result":"Paris answer"});
        if protocol != Protocol::Cursor {
            result["num_turns"] = json!(1);
            result["usage"] = json!({"input_tokens":7,"output_tokens":3});
        }
        if protocol == Protocol::Qwen {
            result["uuid"] = json!("terminal-native");
            result["permission_denials"] = json!([]);
            result["error"] = if query == "native error" {
                json!({"message":"native failure"})
            } else {
                Value::Null
            };
        }
        emit_value(result)?;
    }
    Ok(if query == "nonzero" { 7 } else { 0 })
}
