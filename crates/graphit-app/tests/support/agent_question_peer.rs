//! KnowledgeQuestion's native Codex fixture reads the admitted stdin in the child.
use super::{emit, emit_value};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, Read};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Mode {
    Answer,
    Malformed,
    Provider,
    Missing,
    Empty,
    Nonzero,
    Cancel,
    Timeout,
}

pub(super) async fn run(mode: Mode, expected_body: String) -> io::Result<i32> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let prompt: Value = serde_json::from_str(&input)?;
    let pages = prompt["pages"]
        .as_array()
        .ok_or(io::ErrorKind::InvalidData)?;
    for page in pages {
        if page["slug"] == "Paris" {
            assert!(
                page["body"] == expected_body,
                "Question literal body mismatch"
            );
        }
    }
    if matches!(mode, Mode::Malformed) {
        emit("{broken")?;
        return Ok(0);
    }
    emit_value(json!({"type":"turn.started"}))?;
    if matches!(mode, Mode::Cancel | Mode::Timeout) {
        std::future::pending::<()>().await;
    }
    if matches!(mode, Mode::Provider) {
        emit_value(json!({"type":"turn.failed","error":{"message":"fixture failure"}}))?;
        return Ok(0);
    }
    if !matches!(mode, Mode::Missing) {
        let text = if matches!(mode, Mode::Empty) {
            " \n\t".into()
        } else {
            let first = pages.first().ok_or(io::ErrorKind::InvalidData)?;
            format!(
                "{}\n{}|{}\n[[outside-selection]]",
                prompt["question"]
                    .as_str()
                    .ok_or(io::ErrorKind::InvalidData)?,
                first["title"].as_str().ok_or(io::ErrorKind::InvalidData)?,
                first["body"].as_str().ok_or(io::ErrorKind::InvalidData)?
            )
        };
        emit_value(
            json!({"type":"item.completed","item":{"id":"fixture","type":"agent_message","text":text}}),
        )?;
    }
    emit_value(json!({"type":"turn.completed","usage":{
        "input_tokens":8,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1
    }}))?;
    Ok(if matches!(mode, Mode::Nonzero) { 7 } else { 0 })
}
