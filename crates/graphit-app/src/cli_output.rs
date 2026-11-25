//! Terminal presentation only. Native service replies retain their own wire format.
use serde_json::Value;
use std::io::{self, Write};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Text,
    Json,
}

/// A finite command's existing JSON writer feeds one response at a time here.
/// Raw source/protocol streams bypass this adapter. No output is written on drop.
pub struct Output<W> {
    writer: W,
    format: Format,
    command: String,
    pending: Vec<u8>,
}
impl<W: Write> Output<W> {
    pub fn new(writer: W, format: Format, command: impl Into<String>) -> Self {
        Self {
            writer,
            format,
            command: command.into(),
            pending: Vec::new(),
        }
    }
    fn deliver(&mut self) -> io::Result<()> {
        let bytes = std::mem::take(&mut self.pending);
        if bytes.is_empty() {
            return Ok(());
        }
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => render(&mut self.writer, &value, &self.command),
            // A few producers deliberately return literal text (e.g. an Ask answer).
            Err(_) => self.writer.write_all(&bytes),
        }
    }
}
impl<W: Write> Write for Output<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.format == Format::Json {
            return self.writer.write(bytes);
        }
        for chunk in bytes.split_inclusive(|b| *b == b'\n') {
            self.pending.extend_from_slice(chunk);
            if chunk.ends_with(b"\n") {
                self.deliver()?;
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.deliver()?;
        self.writer.flush()
    }
}

pub fn label(text: &str) -> String {
    let mut text = text.replace(['_', '-'], " ");
    if let Some(first) = text.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    text
}
fn safe(text: &str) -> String {
    use std::fmt::Write as _;
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        if (c.is_control() && c != '\n' && c != '\t')
            || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            let _ = write!(result, "\\u{{{:04x}}}", c as u32);
        } else {
            result.push(c);
        }
    }
    result
}
fn scalar(value: &Value) -> Option<String> {
    Some(match value {
        Value::Null => "Not set".into(),
        Value::Bool(v) => if *v { "Yes" } else { "No" }.into(),
        Value::String(v) => {
            if v.is_empty() {
                "(empty)".into()
            } else {
                safe(v)
            }
        }
        Value::Number(v) => v.to_string(),
        _ => return None,
    })
}

/// One renderer for records, lists, empty results and nested operation outcomes.
pub fn render(out: &mut impl Write, value: &Value, command: &str) -> io::Result<()> {
    // Readable file/page contents are useful verbatim, including line slicing.
    if command == "knowledge source"
        && let Some(source) = value.pointer("/selection/source").and_then(Value::as_str)
    {
        return out.write_all(source.as_bytes());
    }
    if command.contains(" content") {
        for pointer in [
            "/source",
            "/content",
            "/result/content",
            "/result/text",
            "/content/content",
            "/content/text",
            "/text",
        ] {
            if let Some(text) = value.pointer(pointer).and_then(Value::as_str) {
                out.write_all(text.as_bytes())?;
                if !text.ends_with('\n') {
                    out.write_all(b"\n")?;
                }
                return Ok(());
            }
        }
    }
    if command == "init"
        && value["status"] == "ok"
        && value.get("installation").is_some()
        && value.get("agent").is_some()
    {
        writeln!(out, "Project initialized.")?;
        field(out, "Installation", &value["installation"], 2)?;
        field(out, "Agent", &value["agent"], 2)?;
        if let Some(project) = value.get("project").filter(|p| !p.is_null()) {
            field(out, "Project", &project["directory"], 2)?;
            if let Some(identity) = project.get("identity") {
                node(out, identity, 4)?;
            }
            writeln!(out, "  Checkout maintained by the daemon.")?;
            if let Some(git) = value.get("git_records").filter(|v| !v.is_null()) {
                if git["state"].as_str() == Some("non_git") {
                    writeln!(out, "  Git hooks: No Git repository")?;
                } else if let Some(hooks) = git.get("hooks") {
                    field(out, "Git hooks", hooks, 2)?;
                }
            }
        }
        if let Some(profile) = value
            .pointer("/sync/steps/native/coordinator_hooks_mcp/profile")
            .filter(|profile| profile.is_string())
        {
            field(out, "Profile", profile, 2)?;
        }
        writeln!(out, "  Agent configuration synchronized.")?;
        writeln!(
            out,
            "  Open a new agent session and approve project hooks if prompted."
        )?;
        return Ok(());
    }
    if command == "remove" && value["status"] == "ok" {
        writeln!(
            out,
            "{}",
            match value["state"].as_str() {
                Some("absent") => "No Graphit project found.",
                Some("agent_removed") => "Agent integration removed.",
                _ => "Graphit removed from the project.",
            }
        )?;
        field(out, "Project", &value["project"], 2)?;
        if !value["agent"].is_null() {
            field(out, "Agent", &value["agent"], 2)?;
        }
        return Ok(());
    }
    if command == "uninstall" {
        writeln!(
            out,
            "{}",
            match value["package"]["state"].as_str() {
                Some("removed") => "Graphit uninstalled.",
                Some("scheduled_after_exit") =>
                    "Local Graphit installation removed. Package removal will run after exit.",
                Some("shared") =>
                    "Local Graphit installation removed. Another installation uses this package.",
                Some("unregistered") =>
                    "Local cleanup finished. Package preserved because this installation has no matching registration.",
                Some("unmanaged") =>
                    "Local cleanup finished. The executable is managed separately.",
                _ => "Graphit uninstall stopped before completing.",
            }
        )?;
        field(out, "Installation", &value["installation"], 2)?;
        if let Some(step) = value.get("failed_step") {
            field(out, "Failed step", step, 2)?;
        }
        if value["package"]["state"] == "partial" {
            field(out, "Package files removed", &value["package"]["files"], 2)?;
        }
        if let Some(log) = value.pointer("/package/failure_log") {
            field(out, "Deferred cleanup error log", log, 2)?;
        }
        if let Some(preserved) = value["local_data"]["preserved"].as_array() {
            for directory in preserved {
                field(
                    out,
                    "Preserved directory without ownership receipt",
                    directory,
                    2,
                )?;
            }
        }
        if let Some(projects) = value["projects"].as_array() {
            for project in projects.iter().filter(|project| {
                matches!(
                    project["state"].as_str(),
                    Some("unavailable" | "identity_changed")
                )
            }) {
                field(out, "Checkout requiring cleanup", &project["project"], 2)?;
            }
        }
        return Ok(());
    }
    if command.starts_with("config ") && value.get("changed").is_some() {
        return writeln!(
            out,
            "{}",
            match (command, value["changed"].as_bool()) {
                ("config unset", Some(true)) => "Configuration value removed.",
                ("config unset", _) => "Configuration value was already absent.",
                (_, Some(true)) => "Configuration saved.",
                _ => "Configuration already up to date.",
            }
        );
    }
    // Interpret only these commands' known response boundary; authored records stay intact.
    let data = if (value.get("operation").is_some() || value.get("kind").is_some())
        && value.get("result").is_some()
    {
        &value["result"]
    } else {
        value
    };
    if matches!(command, "projects label-set" | "projects label-unset") && data["status"] == "ok" {
        let message = match (
            command,
            data["document_changed"].as_bool(),
            data["inventory_changed"].as_bool(),
        ) {
            ("projects label-set", Some(true), _) => "Project label value added.",
            ("projects label-unset", Some(true), _) => "Project label removed.",
            (_, _, Some(true)) => "Project label unchanged. Local inventory refreshed.",
            ("projects label-set", _, _) => "Project label value is already present.",
            _ => "Project label is already absent.",
        };
        writeln!(out, "{message}")?;
        field(out, "Project ID", &data["project_id"], 2)?;
        return Ok(());
    }
    if matches!(command, "projects label-set" | "projects label-unset")
        && data["status"] == "partial"
        && data["code"] == "inventory_refresh_failed"
    {
        let state = if data["document_changed"] == true {
            "The requested label change was saved to the checkout."
        } else {
            "The checkout already had the requested label state."
        };
        writeln!(out, "{state} The local inventory could not be refreshed.")?;
        field(out, "Project ID", &data["project_id"], 2)?;
        return writeln!(
            out,
            "Next: Resolve the inventory write failure and repeat the same command to refresh it."
        );
    }
    if matches!(command, "task export" | "task export-session") && data["state"] == "completed" {
        writeln!(
            out,
            "{} audit exported.",
            if command == "task export-session" {
                "Session"
            } else {
                "Task"
            }
        )?;
        field(out, "File", &data["output"], 2)?;
        field(out, "ID", &data["id"], 2)?;
        return Ok(());
    }
    if matches!(command, "knowledge list" | "knowledge lint") {
        if data["state"] == "no_corpus" {
            writeln!(out, "No published Knowledge corpus.")?;
            writeln!(
                out,
                "Next: Inspect graphit index status or request graphit index run --module knowledge."
            )?;
            return Ok(());
        }
        if command == "knowledge lint" {
            if data["state"] == "published_empty" {
                writeln!(out, "Knowledge corpus is empty; no documents to check.")?;
            } else if let Some(count) = data["issue_count"].as_u64() {
                if count == 0 {
                    writeln!(out, "Knowledge lint passed.")?;
                } else {
                    writeln!(
                        out,
                        "Knowledge lint found {count} {}.",
                        if count == 1 { "issue" } else { "issues" }
                    )?;
                }
            }
        } else if data["documents"].as_array().is_some_and(Vec::is_empty) {
            writeln!(out, "Knowledge documents\n  No documents in this page.")?;
            if let Some(cursor) = data.get("next_cursor").filter(|v| !v.is_null()) {
                field(out, "Next cursor", cursor, 2)?;
            }
            return Ok(());
        }
    }
    if command == "index run" {
        writeln!(
            out,
            "{}",
            if data["timed_out"] == true {
                "Index wait timed out. Shared processing may continue."
            } else if data["outcome"] == "failed" {
                "Index processing failed."
            } else if data["processed"] == true {
                "Index request processed."
            } else if data["accepted"] == true {
                "Index request accepted. Processing has not completed."
            } else {
                "Index request status"
            }
        )?;
        response(out, value, 2)?;
        if data["processed"] != true {
            writeln!(out, "Next: Run graphit index status to inspect progress.")?;
        }
        return Ok(());
    }
    let heading = match command {
        "projects list" => "Local projects".into(),
        "projects show" => "Local project".into(),
        "knowledge list" => "Knowledge documents".into(),
        "memory list" => "Memory records".into(),
        "memory get" => "Memory record".into(),
        "memory history" => "Memory history".into(),
        "memory search" => "Memory search results".into(),
        "references task" => "Task references".into(),
        "references task-backlinks" => "Task backlinks".into(),
        "references knowledge" => "Knowledge references".into(),
        "provider list" => "Providers".into(),
        "account list" => "Accounts".into(),
        "task get-task" => "Task".into(),
        "task get-session" => "Session".into(),
        "task session-heads" => "Sessions".into(),
        "task catalogue" => "Tasks".into(),
        "ast query" | "task query" => "Query results".into(),
        "ast search" | "knowledge search" | "task search" => "Search results".into(),
        "ast imports" => "Installed AST contexts".into(),
        _ => label(command),
    };
    if command != "knowledge lint" {
        writeln!(out, "{heading}")?;
    }
    if matches!(command, "config get" | "config list") {
        configuration(out, value)
    } else if matches!(command, "projects list" | "projects show") {
        project_inventory(out, data)
    } else {
        response(out, value, 2)
    }
}

fn project_inventory(out: &mut impl Write, data: &Value) -> io::Result<()> {
    let Some(fields) = data.as_object() else {
        return response(out, data, 2);
    };
    fn project(out: &mut impl Write, value: &Value, indent: usize) -> io::Result<()> {
        let Some(fields) = value.as_object() else {
            return node(out, value, indent);
        };
        for (key, value) in fields {
            if key == "cluster"
                && let Some(labels) = value.as_object().filter(|v| !v.is_empty())
            {
                writeln!(out, "{:indent$}Cluster:", "")?;
                for (key, value) in labels {
                    let literal = safe(key).replace('\n', "\\n").replace('\t', "\\t");
                    field(out, &literal, value, indent + 2)?;
                }
            } else {
                field(out, &label(key), value, indent)?;
            }
        }
        Ok(())
    }
    for (key, value) in fields {
        match (key.as_str(), value) {
            ("project", Value::Object(_)) => {
                writeln!(out, "  Project:")?;
                project(out, value, 4)?;
            }
            ("projects", Value::Array(projects)) if !projects.is_empty() => {
                writeln!(out, "  Projects:")?;
                for row in projects {
                    project(out, row, 4)?;
                    writeln!(out)?;
                }
            }
            _ => field(out, &label(key), value, 2)?,
        }
    }
    Ok(())
}
fn field(out: &mut impl Write, name: &str, value: &Value, indent: usize) -> io::Result<()> {
    if value.is_null() && matches!(name, "History" | "Comments") {
        return writeln!(out, "{:indent$}{name}: Not loaded", "");
    }
    if let Some(text) = scalar(value) {
        let mut lines = text.lines();
        writeln!(out, "{:indent$}{name}: {}", "", lines.next().unwrap_or(""))?;
        for line in lines {
            writeln!(out, "{:width$}{line}", "", width = indent + 2)?;
        }
    } else if value.as_array().is_some_and(Vec::is_empty)
        || value.as_object().is_some_and(|m| m.is_empty())
    {
        writeln!(out, "{:indent$}{name}: None", "")?;
    } else {
        writeln!(out, "{:indent$}{name}:", "")?;
        node(out, value, indent + 2)?;
    }
    Ok(())
}
fn response(out: &mut impl Write, value: &Value, indent: usize) -> io::Result<()> {
    if let Value::Object(map) = value {
        // These are transport wrappers, not information architecture.
        if map.contains_key("result") && (map.contains_key("operation") || map.contains_key("kind"))
        {
            response(out, &map["result"], indent)?;
            if let Some(token) = map.get("token") {
                field(out, "Claim token", token, indent)?;
            }
            return Ok(());
        }
        if let Some(Value::Array(content)) = map.get("content")
            && content.iter().all(|v| v.get("text").is_some())
        {
            if content.is_empty() && map.get("structuredContent").is_none() {
                return writeln!(
                    out,
                    "{:indent$}{}",
                    "",
                    if map.get("isError").and_then(Value::as_bool) == Some(true) {
                        "Service returned no error details; inspect the original operation before retrying."
                    } else {
                        "No results."
                    }
                );
            }
            if let Some(value) = map.get("structuredContent") {
                return response(out, value, indent);
            }
            for item in content {
                if let Some(text) = item["text"].as_str() {
                    if let Ok(value) = serde_json::from_str::<Value>(text) {
                        response(out, &value, indent)?;
                    } else {
                        writeln!(out, "{:indent$}{}", "", safe(text))?;
                    }
                }
            }
            return Ok(());
        }
        if let (Some(Value::Array(columns)), Some(Value::Array(rows))) =
            (map.get("columns"), map.get("rows"))
        {
            writeln!(out, "{:indent$}Results: {}", "", rows.len())?;
            for row in rows {
                if let Some(cells) = row.as_array() {
                    for (column, cell) in columns.iter().zip(cells) {
                        let cell = if cell.get("type").is_some() && cell.get("value").is_some() {
                            &cell["value"]
                        } else {
                            cell
                        };
                        field(
                            out,
                            &label(column.as_str().unwrap_or("Value")),
                            cell,
                            indent + 2,
                        )?;
                    }
                    writeln!(out)?;
                }
            }
            for (key, value) in map
                .iter()
                .filter(|(key, _)| !matches!(key.as_str(), "columns" | "rows"))
            {
                field(out, &label(key), value, indent)?;
            }
            return Ok(());
        }
    }
    node(out, value, indent)
}

fn configuration(out: &mut impl Write, value: &Value) -> io::Result<()> {
    if let Some(records) = value.as_array() {
        if records.is_empty() {
            writeln!(out, "  No configured values.")?;
        }
        for record in records {
            configuration(out, record)?;
            writeln!(out)?;
        }
    } else {
        field(out, "Key", &value["key"], 2)?;
        field(out, "Source", &value["source"], 2)?;
        let inspected = &value["value"];
        match inspected["kind"].as_str() {
            Some("public") => field(out, "Value", &inspected["value"], 2)?,
            Some("secret") => {
                writeln!(out, "  Value: [redacted]")?;
                field(out, "Configured", &inspected["configured"], 2)?;
            }
            _ => field(out, "Value", inspected, 2)?,
        }
    }
    Ok(())
}

fn node(out: &mut impl Write, value: &Value, indent: usize) -> io::Result<()> {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                field(out, &label(key), value, indent)?;
            }
            if map.is_empty() {
                writeln!(out, "{:indent$}No results.", "")?;
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                writeln!(out, "{:indent$}No results.", "")?;
            }
            for item in items {
                if let Some(text) = scalar(item) {
                    writeln!(out, "{:indent$}- {text}", "")?;
                } else {
                    node(out, item, indent)?;
                    writeln!(out)?;
                }
            }
        }
        _ => {
            writeln!(out, "{:indent$}{}", "", scalar(value).unwrap())?;
        }
    }
    Ok(())
}

/// Preserve structured partial effects in diagnostics without exposing wire envelopes.
pub fn diagnostic(text: &str) -> String {
    if let Some(start) = text.find('{') {
        let mut stream = serde_json::Deserializer::from_str(&text[start..]).into_iter::<Value>();
        if let Some(Ok(value)) = stream.next() {
            let mut out = Vec::new();
            let _ = response(&mut out, &value, 2);
            let remainder = text[start + stream.byte_offset()..].trim();
            return format!(
                "{}\n{}{}",
                safe(&text[..start]),
                String::from_utf8_lossy(&out).trim_end(),
                if remainder.is_empty() {
                    String::new()
                } else {
                    format!("\n  {}", safe(remainder))
                }
            );
        }
    }
    safe(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn project_inventory_keeps_literal_label_keys() {
        let row = json!({"cluster": {"Team_API": ["one"], "team-api": ["two"], "line\nbreak": ["three"]}});
        for (command, data) in [
            (
                "projects list",
                json!({"projects":[&row], "next_cursor":null}),
            ),
            ("projects show", json!({"project":&row})),
        ] {
            let mut out = Vec::new();
            render(&mut out, &data, command).unwrap();
            let text = String::from_utf8(out).unwrap();
            for key in ["Team_API:", "team-api:", "line\\nbreak:"] {
                assert!(text.contains(key), "{text}");
            }
            assert!(!text.contains("Team api"));
        }
    }

    #[test]
    fn project_partial_text_distinguishes_the_checkout_write() {
        for (changed, state) in [(true, "saved to the checkout"), (false, "already had")] {
            let mut out = Vec::new();
            render(
                &mut out,
                &json!({
                    "status":"partial", "code":"inventory_refresh_failed",
                    "project_id":"project", "document_changed":changed,
                }),
                "projects label-set",
            )
            .unwrap();
            let text = String::from_utf8(out).unwrap();
            assert!(text.contains(state));
            assert!(text.contains("repeat the same command to refresh it"));
        }
    }
    #[test]
    fn functional_outcomes_distinguish_absence_acceptance_timeout_and_export() {
        let cases = [
            (
                "knowledge list",
                json!({"state":"no_corpus"}),
                "No published Knowledge corpus.",
            ),
            (
                "knowledge list",
                json!({"state":"published","documents":[],"next_cursor":null}),
                "No documents in this page.",
            ),
            (
                "knowledge lint",
                json!({"state":"published_empty","issue_count":0,"findings":[]}),
                "no documents to check",
            ),
            (
                "knowledge lint",
                json!({"state":"published","issue_count":0,"findings":[]}),
                "Knowledge lint passed.",
            ),
            (
                "knowledge lint",
                json!({"state":"published","issue_count":1,"findings":[{"code":"broken_link","message":"Repair the target"}]}),
                "Knowledge lint found 1 issue.",
            ),
            (
                "index run",
                json!({"accepted":true,"processed":false,"timed_out":false}),
                "Index request accepted. Processing has not completed.",
            ),
            (
                "index run",
                json!({"accepted":true,"processed":false,"timed_out":true}),
                "Index wait timed out. Shared processing may continue.",
            ),
            (
                "index run",
                json!({"accepted":true,"processed":true,"outcome":"failed"}),
                "Index processing failed.",
            ),
            (
                "task export-session",
                json!({"state":"completed","output":"audit.json","id":"session-id","kind":"session"}),
                "Session audit exported.",
            ),
        ];
        for (command, value, expected) in cases {
            let mut text = Vec::new();
            render(&mut text, &value, command).unwrap();
            let text = String::from_utf8(text).unwrap();
            assert!(text.contains(expected), "{command}: {text}");
            assert!(!text.contains('\u{1b}'));
            let mut json_output = Output::new(Vec::new(), Format::Json, command);
            let encoded = serde_json::to_vec(&value).unwrap();
            json_output.write_all(&encoded).unwrap();
            json_output.flush().unwrap();
            assert_eq!(json_output.writer, encoded);
        }
        let mut text = Vec::new();
        render(
            &mut text,
            &json!({"state":"completed","output":"audit.json","id":"task-id"}),
            "task export",
        )
        .unwrap();
        let text = String::from_utf8(text).unwrap();
        assert!(text.contains("File: audit.json") && text.contains("ID: task-id"));
    }

    #[test]
    fn fragmented_records_preserve_json_opt_in_and_human_default() {
        for format in [Format::Text, Format::Json] {
            let mut bytes = Vec::new();
            let mut out = Output::new(&mut bytes, format, "provider list");
            for part in [b"{\"providers\":".as_slice(), b"[]}\n"] {
                out.write_all(part).unwrap();
            }
            out.flush().unwrap();
            if format == Format::Json {
                assert_eq!(bytes, b"{\"providers\":[]}\n");
            } else {
                assert_eq!(
                    String::from_utf8(bytes).unwrap(),
                    "Providers\n  Providers: None\n"
                );
            }
        }
    }
    #[test]
    fn text_preserves_claims_errors_empty_values_and_redaction() {
        let value = json!({"kind":"claim","result":{"record":{"title":"A\nB\u{1b}[31m","references":[],"optional":null},"token":"full-token"}});
        let mut out = Vec::new();
        render(&mut out, &value, "task claim-task").unwrap();
        let text = String::from_utf8(out).unwrap();
        for needle in [
            "Token: full-token",
            "References: None",
            "Optional: Not set",
            "\\u{001b}",
        ] {
            assert!(text.contains(needle), "{text}");
        }
        assert!(!text.contains("kind"));
        assert!(
            diagnostic(r#"operation failed: {"status":"uncertain","id":"complete-id"}"#)
                .contains("Status: uncertain")
        );
    }
    #[test]
    fn errors_with_trailing_cleanup_details_keep_text_and_partial_effects() {
        let text = diagnostic(
            r#"Hub delivery failed: {"status":"uncertain","id":"complete-id"}; cleanup=closed"#,
        );
        assert!(
            text.contains("Status: uncertain") && text.contains("Id: complete-id"),
            "{text}"
        );
        assert!(
            text.contains("cleanup=closed") && !text.contains("{\"status\""),
            "{text}"
        );
    }

    #[test]
    fn authored_config_objects_never_lose_transport_like_keys() {
        let mut out = Vec::new();
        render(&mut out, &json!({"key":"custom.object","source":"global","value":{"kind":"public","value":{
            "type":"example","value":"payload","operation":"authored","result":{"content":"body"}
        }}}), "config get").unwrap();
        let text = String::from_utf8(out).unwrap();
        for expected in [
            "Type: example",
            "Value: payload",
            "Operation: authored",
            "Content: body",
        ] {
            assert!(text.contains(expected), "{text}");
        }
    }
    #[test]
    fn partial_init_failure_is_never_presented_as_completion() {
        let mut out = Vec::new();
        render(
            &mut out,
            &json!({"status":"failed","outcome":{"effects":"partial"}}),
            "init",
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Status: failed"));
        assert!(!text.contains("Project initialized") && !text.contains("Next steps"));
    }
    #[test]
    fn content_is_copiable_and_write_failure_is_propagated() {
        let mut out = Vec::new();
        render(
            &mut out,
            &json!({"selection":{"source":"# Page"}}),
            "knowledge source",
        )
        .unwrap();
        assert_eq!(out, b"# Page");
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            Output::new(Closed, Format::Text, "test")
                .write_all(b"{}\n")
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
