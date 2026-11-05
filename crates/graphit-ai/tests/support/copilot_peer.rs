//! Controlled prompt-mode CLI. Both AI and App run this actual native child.
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    net::TcpListener,
};
pub const ANSWER: &str = "Native Copilot documentary answer 🦀\n";
fn journal(value: Value) {
    if let Some(path) = std::env::var_os("COPILOT_CONVERSATION_JOURNAL") {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(f, "{value}").unwrap();
    }
}
fn event(kind: &str, data: Value, child: bool) -> Value {
    json!({"type":kind,"id":format!("event-{kind}"),"timestamp":"2026-10-18T00:00:00Z","parentId":null,"agentId":if child {Some("child")} else {None},"data":data})
}
fn emit(value: Value, fragmented: bool) {
    let mut text = serde_json::to_vec(&value).unwrap();
    text.push(b'\n');
    let mut stdout = std::io::stdout().lock();
    if fragmented {
        for bytes in text.chunks(7) {
            stdout.write_all(bytes).unwrap();
            stdout.flush().unwrap();
        }
    } else {
        stdout.write_all(&text).unwrap();
        stdout.flush().unwrap();
    }
}
fn gate() {
    let Some(path) = std::env::var_os("COPILOT_GATE_PORT") else {
        // Deadline tests wait for process cancellation and have no peer to
        // release them. Keep the child alive without opening a socket.
        loop {
            std::thread::park();
        }
    };
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    std::fs::write(path, listener.local_addr().unwrap().port().to_string()).unwrap();
    let (mut connection, _) = listener.accept().unwrap();
    connection
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let mut request = [0; 2048];
    let read = connection.read(&mut request).unwrap();
    assert!(request[..read].starts_with(b"POST /fixture/release HTTP/1.1\r\n"));
    connection
        .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .unwrap();
}
pub fn run() {
    let mode = std::env::var_os("COPILOT_CONTROL_MODE")
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_else(|| std::env::var("COPILOT_MODE").unwrap_or_else(|_| "answer".into()));
    let args: Vec<_> = std::env::args().skip(1).collect();
    if let Some(path) = std::env::var_os("COPILOT_NATIVE_ARGS") {
        std::fs::write(path, format!("{}\n", args.join("\n"))).unwrap();
    }
    assert!(args.iter().any(|a| a == "--output-format=json"));
    assert!(args.iter().any(|a| a == "--available-tools="));
    assert!(args.iter().any(|a| a == "--no-ask-user"));
    assert!(!args.iter().any(|a| a.starts_with("--server")
        || a.starts_with("--allow-all")
        || a.starts_with("--session-id")
        || a == "--continue"));
    let model = args
        .iter()
        .find_map(|a| a.strip_prefix("--model="))
        .unwrap();
    let resume = args.iter().find_map(|a| a.strip_prefix("--resume="));
    journal(
        json!({"op":"start","model":model,"environment":std::env::var("COPILOT_CAPTURED_ENV").ok()}),
    );
    if mode == "start_hold" {
        gate();
    }
    if mode == "startup_error" {
        eprintln!("private-fixture-startup-failure");
        std::process::exit(7);
    }
    let store = std::env::var_os("COPILOT_SESSION_STORE");
    let session = if let Some(id) = resume {
        if mode == "resume_error"
            || store
                .as_ref()
                .is_some_and(|p| std::fs::read_to_string(p).ok().as_deref() != Some(id))
        {
            eprintln!("private-fixture-missing-session");
            std::process::exit(9);
        }
        id.to_owned()
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        if let Some(path) = store.as_ref() {
            std::fs::write(path, &id).unwrap();
        }
        id
    };
    journal(
        json!({"op":if resume.is_some(){"resume"}else{"create"},"session":session,"model":model,"cwd":std::env::current_dir().unwrap()}),
    );
    let mut prompt = String::new();
    std::io::stdin()
        .take(65537)
        .read_to_string(&mut prompt)
        .unwrap();
    assert!(!prompt.is_empty() && prompt.len() <= 65536);
    journal(json!({"op":"prompt","session":session,"prompt_bytes":prompt.len(),"prompt":prompt}));
    if mode == "malformed" {
        println!("{{\"type\":\"assistant.message\",bad}}");
        return;
    }
    if mode == "duplicate_keys" {
        println!(
            "{{\"type\":\"user.message\",\"type\":\"assistant.message\",\"id\":\"ambiguous\",\"data\":{{}}}}"
        );
        return;
    }
    if mode == "truncated" {
        print!("{{\"type\":\"result\",\"sessionId\":\"");
        std::io::stdout().flush().unwrap();
        return;
    }
    if mode == "eof_progress" {
        print!(
            "{}",
            json!({"type":"user.message","id":"native-prompt","data":{"content":prompt}})
        );
        std::io::stdout().flush().unwrap();
        return;
    }
    let fragmented = mode == "fragmented";
    emit(
        json!({"type":"user.message","id":"native-prompt","data":{"content":prompt}}),
        fragmented,
    );
    if mode == "native_error" || mode == "transient" {
        emit(
            event(
                "session.error",
                json!({"errorType":if mode=="transient"{"model_call"}else{"fatal"},"message":"private-native-error-canary"}),
                false,
            ),
            fragmented,
        );
        if mode == "native_error" {
            std::process::exit(1);
        }
    }
    emit(
        event("assistant.turn_start", json!({"turnId":"main-turn"}), false),
        fragmented,
    );
    emit(
        event(
            "assistant.message_delta",
            json!({"messageId":"delta","deltaContent":"never authoritative"}),
            false,
        ),
        fragmented,
    );
    emit(
        event(
            "tool.execution_start",
            json!({"toolCallId":"tool-private-id","toolName":"fixture-native-tool","arguments":{"private":"tool-canary"}}),
            false,
        ),
        fragmented,
    );
    emit(
        event(
            "tool.execution_complete",
            json!({"toolCallId":"tool-private-id","success":true,"result":{"content":"tool is not the answer"}}),
            false,
        ),
        fragmented,
    );
    emit(
        event(
            "assistant.message",
            json!({"messageId":"foreign","originatingMessageId":"foreign-prompt","turnId":"main-turn","content":"foreign is not the answer"}),
            false,
        ),
        fragmented,
    );
    emit(
        event(
            "assistant.message",
            json!({"messageId":"child","originatingMessageId":"native-prompt","turnId":"main-turn","content":"child is not the answer"}),
            true,
        ),
        fragmented,
    );
    emit(
        event(
            "assistant.message",
            json!({"messageId":"reasoning","originatingMessageId":"native-prompt","turnId":"main-turn","chunkIndex":0,"chunkCount":2,"content":"reasoning is not the answer"}),
            false,
        ),
        fragmented,
    );
    emit(
        event(
            "assistant.message",
            json!({"messageId":"main-final","originatingMessageId":"native-prompt","turnId":"main-turn","chunkIndex":1,"chunkCount":2,"content":if mode=="empty_text"{" "}else{ANSWER}}),
            false,
        ),
        fragmented,
    );
    emit(
        event("assistant.turn_end", json!({"turnId":"main-turn"}), false),
        fragmented,
    );
    if mode == "hold" {
        gate();
    }
    if mode == "no_result" {
        return;
    }
    let id = if mode == "resume_mismatch" {
        uuid::Uuid::new_v4().to_string()
    } else {
        session
    };
    let result = json!({"type":"result","timestamp":"2026-10-18T00:00:00Z","sessionId":id,"exitCode":if mode=="result_error"{1}else{0},"usage":{"premiumRequests":0,"totalApiDurationMs":0,"sessionDurationMs":0,"codeChanges":{"linesAdded":0,"linesRemoved":0,"filesModified":[]}}});
    if mode == "final_fragment" {
        print!("{result}");
        std::io::stdout().flush().unwrap();
    } else {
        emit(result.clone(), fragmented);
    }
    if mode == "duplicate_result" {
        emit(result, false);
    }
    if mode == "nonzero_after_result" {
        eprintln!("private-fixture-save-failure");
        std::process::exit(8);
    }
}
