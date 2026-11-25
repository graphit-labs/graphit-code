//! Keep the native fixture's expected wire values while exercising field-based CLI arguments.
use super::*;

pub(super) fn task_flags(operation: &str, input: &Value) -> Vec<String> {
    fn scalar(flags: &mut Vec<String>, key: &str, value: &Value) {
        if value.is_null() {
            return;
        }
        flags.push(format!("--{}", key.replace('_', "-")));
        let mut text = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        if matches!(key, "status" | "column") {
            text = text.replace('_', "-");
        }
        flags.push(text);
    }
    let mut flags = Vec::new();
    for (key, value) in input.as_object().expect("Task fixture input object") {
        match key.as_str() {
            "target" => {
                scalar(&mut flags, "id", &value["id"]);
                scalar(&mut flags, "claim_token", &value["claim"]["token"]);
                scalar(
                    &mut flags,
                    "expected_revision",
                    &value["claim"]["expected_revision"],
                );
            }
            "claim" if !value.is_null() => {
                scalar(&mut flags, "claim_token", &value["token"]);
                scalar(
                    &mut flags,
                    if matches!(operation, "set-task-commit" | "cancel-task") {
                        "claim_revision"
                    } else {
                        "expected_revision"
                    },
                    &value["expected_revision"],
                );
            }
            "claim" => {}
            "filter" | "selection" => {
                for (field, v) in value.as_object().expect("Task fixture selection/filter") {
                    if field == "ids" {
                        if v.as_array().unwrap().is_empty() {
                            flags.push("--empty-ids".into());
                        }
                        for id in v.as_array().unwrap() {
                            scalar(&mut flags, "id", id);
                        }
                    } else {
                        scalar(&mut flags, field, v);
                    }
                }
            }
            "checks" | "add_checks" => {
                for check in value.as_array().unwrap() {
                    scalar(&mut flags, "check_kind", &check["kind"]);
                    scalar(&mut flags, "check_text", &check["text"]);
                }
            }
            "columns" => {
                for column in value.as_array().unwrap() {
                    scalar(&mut flags, "column", column);
                }
            }
            "depends_on" => {
                if value.is_null() || value.as_array().unwrap().is_empty() {
                    if operation == "update-task-links" {
                        flags.push("--clear-dependencies".into());
                    }
                } else {
                    for id in value.as_array().unwrap() {
                        scalar(&mut flags, "depends_on", id);
                    }
                }
            }
            "parent_id" if value.is_null() => {
                if operation == "update-task-links" {
                    flags.push("--clear-parent".into());
                }
            }
            "commit" if value.is_null() => {
                if operation == "set-task-commit" {
                    flags.push("--clear-commit".into());
                }
            }
            "schedule" => {
                if value.is_null() {
                    if operation == "revise-task" {
                        flags.push("--clear-schedule".into());
                    }
                } else {
                    for (field, v) in value.as_object().unwrap() {
                        scalar(&mut flags, field, v);
                    }
                }
            }
            "history" | "comments" => {
                if !value.is_null() {
                    for (field, v) in value.as_object().unwrap() {
                        scalar(&mut flags, &format!("{key}_{field}"), v);
                    }
                }
            }
            "cursor" => {
                scalar(&mut flags, "cursor", value);
            }
            "replacement" => {
                if !value.is_null() {
                    scalar(&mut flags, "replacement_text", &value["text"]);
                    scalar(&mut flags, "replacement_kind", &value["kind"]);
                }
            }
            "references" => {
                let entries = value.as_array().expect("fixture reference array");
                if entries.is_empty() {
                    flags.push("--clear-references".into());
                }
                for (index, entry) in entries.iter().enumerate() {
                    for field in ["type", "id", "scope", "scope_id", "context"] {
                        scalar(
                            &mut flags,
                            &format!("reference_{field}"),
                            entry["target"]
                                .get(field)
                                .unwrap_or(&Value::String(String::new())),
                        );
                    }
                    scalar(&mut flags, "reference_relation", &entry["relation"]);
                    scalar(
                        &mut flags,
                        "reference_field",
                        entry.get("field").unwrap_or(&Value::String(String::new())),
                    );
                    if entry.get("field").is_none() {
                        flags.extend(["--reference-field-absent".into(), (index + 1).to_string()]);
                    }
                }
            }
            "id" | "lease_ms" | "expected_revision" | "idempotency_key" | "title"
            | "description" | "strategy" | "task_type" | "priority" | "session_id"
            | "parent_id" | "commit" | "reason" | "kind" | "body" | "summary" | "next_step"
            | "check_id" | "passed" | "evidence" | "flagged" | "mode" | "text" | "page_size"
            | "top_k" => scalar(&mut flags, key, value),
            field => panic!("Task fixture field not mapped to CLI flags: {field}"),
        }
    }
    flags
}
