use rmcp::model::Tool;
use serde_json::{Value, json};
use std::sync::OnceLock;

#[cfg(test)]
mod instruction_examples;

/// Public tools advertise only operations implemented by the native producer.
pub(crate) const OPERATIONS: &[(&str, &str, bool, &str)] = &[
    (
        "graphit_task_get",
        "get_task",
        false,
        "Read current Task state and explicitly requested snapshot-bound history/comment pages.",
    ),
    (
        "graphit_task_list",
        "catalogue",
        false,
        "Read a cursor-pinned Task catalogue, including explicit dates and dependencies.",
    ),
    (
        "graphit_task_query",
        "query",
        false,
        "Read a finite projected Task or Session page from one native pin.",
    ),
    (
        "graphit_task_schema",
        "schema",
        false,
        "Read the native Task query tables and permitted column types.",
    ),
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    (
        "graphit_task_search",
        "search",
        false,
        "Search current Task specifications and comments using fts (default), semantic or hybrid retrieval; continue with the returned native cursor.",
    ),
    (
        "graphit_task_session_get",
        "get_session",
        false,
        "Read current Session state and explicitly requested snapshot-bound history pages.",
    ),
    (
        "graphit_task_session_list",
        "session_heads",
        false,
        "Read native Session heads.",
    ),
    (
        "graphit_task_create",
        "create_task",
        true,
        "Create a native Task with acceptance and test checks, and explicit optional calendar dates.",
    ),
    (
        "graphit_task_session_create",
        "create_session",
        true,
        "Create a native Session.",
    ),
    (
        "graphit_task_claim",
        "claim_task",
        true,
        "Claim a Task; return the opaque participant fence only to this caller.",
    ),
    (
        "graphit_task_session_claim",
        "claim_session",
        true,
        "Claim a Session; return its opaque participant fence.",
    ),
    (
        "graphit_task_heartbeat",
        "heartbeat_task",
        true,
        "Renew a Task owner's actual lease and revision.",
    ),
    (
        "graphit_task_session_heartbeat",
        "heartbeat_session",
        true,
        "Renew a Session owner's actual lease and revision.",
    ),
    (
        "graphit_task_comment_add",
        "add_task_comment",
        true,
        "Add a literal Markdown comment with the current participant fence and an idempotency key.",
    ),
    (
        "graphit_task_progress",
        "progress_task",
        true,
        "Record the current Task owner's progress.",
    ),
    (
        "graphit_task_session_checkpoint",
        "checkpoint_session",
        true,
        "Record the current Session owner's checkpoint.",
    ),
    (
        "graphit_task_release",
        "release_task",
        true,
        "Release a Task using its current participant fence.",
    ),
    (
        "graphit_task_session_release",
        "release_session",
        true,
        "Release a Session using its current participant fence.",
    ),
    (
        "graphit_task_commit_set",
        "set_task_commit",
        true,
        "Associate or clear a full Git commit ID using revision CAS; preserve status and checks, including completed tasks. Active claims require their fence.",
    ),
    (
        "graphit_task_revise",
        "revise_task",
        true,
        "Revise Task specification; omitted fields stay, schedule null clears.",
    ),
    (
        "graphit_task_session_revise",
        "revise_session",
        true,
        "Revise the coordinated Session specification; omitted fields stay and null is invalid.",
    ),
    (
        "graphit_task_check",
        "verify_task_check",
        true,
        "Verify a current Task check with evidence.",
    ),
    (
        "graphit_task_check_supersede",
        "supersede_task_check",
        true,
        "Supersede a Task check, optionally creating a replacement.",
    ),
    (
        "graphit_task_update_links",
        "update_task_links",
        true,
        "Replace Task parent or dependency links with recorded reason.",
    ),
    (
        "graphit_task_flag",
        "set_task_flag",
        true,
        "Set or clear the native Task flag with recorded reason.",
    ),
    (
        "graphit_task_complete",
        "complete_task",
        true,
        "Complete a Task after its actual gates pass.",
    ),
    (
        "graphit_task_cancel",
        "cancel_task",
        true,
        "Cancel an eligible Task with recorded reason and live fence when owned.",
    ),
    (
        "graphit_task_session_complete",
        "complete_session",
        true,
        "Complete a Session after its native Task gates pass.",
    ),
];
pub(crate) fn tools() -> &'static [Tool] {
    static TOOLS: OnceLock<Vec<Tool>> = OnceLock::new();
    TOOLS.get_or_init(|| {
        OPERATIONS
            .iter()
            .map(|(name, operation, _, description)| {
                let mut properties = serde_json::Map::new();
                properties.insert("selection".into(), selection());
                let input = input_schema(operation);
                let required = if let Some(input) = input {
                    properties.insert("input".into(), input);
                    vec!["input"]
                } else {
                    vec![]
                };
                Tool::new(*name, *description, json!({
            "type":"object","properties":properties,"required":required,"additionalProperties":false
        }).as_object().expect("object schema").clone())
            })
            .collect()
    })
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn text() -> Value {
    json!({"type":"string"})
}
fn id() -> Value {
    json!({"type":"string","pattern":"^[0-7][0-9A-HJKMNP-TV-Z]{25}$"})
}
fn integer() -> Value {
    json!({"type":"integer","minimum":0})
}
fn lease() -> Value {
    json!({"type":"integer","minimum":1})
}
fn nullable(value: Value) -> Value {
    json!({"anyOf":[value,{"type":"null"}]})
}
fn array(value: Value) -> Value {
    json!({"type":"array","items":value})
}
pub(crate) fn selection() -> Value {
    object(
        json!({
            "project_id":nullable(id()),"project_version":nullable(text()),
            "project_dir":nullable(text())
        }),
        &[],
    )
}
fn fence() -> Value {
    object(
        json!({
            "token":{"type":"string","pattern":"^[A-Za-z0-9_-]{43}$"},
            "expected_revision":integer()
        }),
        &["token", "expected_revision"],
    )
}
fn target() -> Value {
    object(json!({"id":id(),"claim":fence()}), &["id", "claim"])
}
pub(crate) fn filter() -> Value {
    let schema = rmcp::schemars::generate::SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator()
        .into_root_schema_for::<graphit_records::task_store::CatalogueFilter>();
    serde_json::to_value(schema).expect("serializable native Task catalogue filter schema")
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub(crate) fn search_input() -> Value {
    let filter = object(
        json!({"status":nullable(json!({"enum":["open","in_progress","completed","cancelled"]})),"session_id":nullable(id()),"flagged":nullable(json!({"type":"boolean"})),"blocked":nullable(json!({"type":"boolean"}))}),
        &[],
    );
    let generation = object(
        json!({"version":integer(),"physical_branch":nullable(text()),"inherited_target":nullable(text())}),
        &["version"],
    );
    let binding = object(
        json!({
            "mode":{"enum":["fts","semantic","hybrid"]},"semantic":{"type":["object","null"]},
            "corpus_key":text(),"project_id":id(),"root":selection()["properties"]["project_dir"]["anyOf"][0].clone(),
            "project_version":text(),"generation":generation,"text":text(),
            "filter":filter,"page_size":lease(),"top_k":lease()
        }),
        &[
            "corpus_key",
            "project_id",
            "root",
            "project_version",
            "generation",
            "text",
            "filter",
            "page_size",
            "top_k",
        ],
    );
    object(
        json!({"mode":{"enum":["fts","semantic","hybrid"],"default":"fts"},"text":text(),"filter":filter,"page_size":lease(),"top_k":lease(),
            "cursor":nullable(object(json!({"binding":binding,"offset":lease()}), &["binding","offset"]))}),
        &["text", "filter", "page_size", "top_k"],
    )
}
fn schedule() -> Value {
    let day = nullable(json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"}));
    let mut schema = object(json!({"start_date":day,"end_date":day,"due_date":day}), &[]);
    schema["anyOf"] = json!([
        {"required":["start_date"],"properties":{"start_date":{"type":"string"}}},
        {"required":["end_date"],"properties":{"end_date":{"type":"string"}}},
        {"required":["due_date"],"properties":{"due_date":{"type":"string"}}}
    ]);
    schema
}
fn check() -> Value {
    object(
        json!({"kind":{"enum":["acceptance","test"]},"text":text()}),
        &["kind", "text"],
    )
}
fn references() -> Value {
    array(object(
        json!({
            "target":object(json!({
                "type":text(),"id":text(),"scope":text(),"scope_id":text(),"context":text()
            }), &["type", "id"]),
            "relation":text(),"field":text()
        }),
        &["target", "relation"],
    ))
}
fn input_schema(operation: &str) -> Option<Value> {
    let mut schema = match operation {
        "schema" | "session_heads" => return None,
        "query" => {
            let schema = rmcp::schemars::generate::SchemaSettings::draft2020_12()
                .with(|settings| settings.inline_subschemas = true)
                .into_generator()
                .into_root_schema_for::<graphit_records::task_store::TaskQuery>();
            serde_json::to_value(schema).expect("serializable native Task query schema")
        }
        "get_task" => {
            let schema = rmcp::schemars::generate::SchemaSettings::draft2020_12()
                .with(|settings| settings.inline_subschemas = true)
                .into_generator()
                .into_root_schema_for::<graphit_records::task_store::TaskGetInput>();
            serde_json::to_value(schema).expect("serializable native Task Get schema")
        }
        "get_session" => {
            let schema = rmcp::schemars::generate::SchemaSettings::draft2020_12()
                .with(|settings| settings.inline_subschemas = true)
                .into_generator()
                .into_root_schema_for::<graphit_records::task_store::SessionGetInput>();
            serde_json::to_value(schema).expect("serializable native Session Get schema")
        }
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        "search" => search_input(),
        "catalogue" => {
            let schema = rmcp::schemars::generate::SchemaSettings::draft2020_12()
                .with(|settings| settings.inline_subschemas = true)
                .into_generator()
                .into_root_schema_for::<graphit_records::task_store::CatalogueCursor>();
            let cursor = serde_json::to_value(schema)
                .expect("serializable native Task catalogue cursor schema");
            object(
                json!({"filter":filter(),"page_size":lease(),"cursor":nullable(cursor)}),
                &["filter", "page_size"],
            )
        }
        "create_session" => object(
            json!({"idempotency_key":text(),"title":text(),"description":text(),"strategy":text()}),
            &["idempotency_key", "title", "description", "strategy"],
        ),
        "create_task" => object(
            json!({
                "idempotency_key":text(),"session_id":id(),"parent_id":nullable(id()),"depends_on":array(id()),
                "title":text(),"description":text(),"task_type":text(),"priority":{"type":"integer","minimum":0,"maximum":4},
                "commit":nullable(commit()),"schedule":nullable(schedule()),"checks":json!({"type":"array","items":check(),"minItems":2,"allOf":[{"contains":{"type":"object","properties":{"kind":{"const":"acceptance"}},"required":["kind"]}},{"contains":{"type":"object","properties":{"kind":{"const":"test"}},"required":["kind"]}}]})
            }),
            &[
                "idempotency_key",
                "session_id",
                "title",
                "description",
                "task_type",
                "priority",
                "checks",
            ],
        ),
        "claim_task" | "claim_session" => {
            object(json!({"id":id(),"lease_ms":lease()}), &["id", "lease_ms"])
        }
        "heartbeat_task" | "heartbeat_session" => object(
            json!({"target":target(),"lease_ms":lease()}),
            &["target", "lease_ms"],
        ),
        "add_task_comment" => object(
            json!({
                "target":target(),"lease_ms":lease(),"idempotency_key":text(),
                "kind":{"enum":["note","decision","problem","lesson","knowledge"]},"body":text()
            }),
            &["target", "lease_ms", "idempotency_key", "kind", "body"],
        ),
        "progress_task" | "checkpoint_session" => object(
            json!({"target":target(),"lease_ms":lease(),"summary":text(),"next_step":text()}),
            &["target", "lease_ms", "summary", "next_step"],
        ),
        "release_task" | "release_session" => object(
            json!({"target":target(),"summary":text(),"next_step":text()}),
            &["target", "summary", "next_step"],
        ),
        "revise_task" => object(
            json!({
                "target":target(),"lease_ms":lease(),"reason":text(),"title":text(),"description":text(),"task_type":text(),
                "priority":nullable(json!({"type":"integer","minimum":0,"maximum":4})),"schedule":nullable(schedule()),"add_checks":array(check())
            }),
            &["target", "lease_ms", "reason"],
        ),
        "revise_session" => object(
            json!({"target":target(),"lease_ms":lease(),"reason":text(),"title":text(),"description":text(),"strategy":text()}),
            &["target", "lease_ms", "reason"],
        ),
        "verify_task_check" => object(
            json!({"target":target(),"lease_ms":lease(),"check_id":id(),"passed":{"type":"boolean"},"evidence":text()}),
            &["target", "lease_ms", "check_id", "passed", "evidence"],
        ),
        "supersede_task_check" => object(
            json!({
                "target":target(),"lease_ms":lease(),"check_id":id(),"reason":text(),
                "replacement":nullable(object(json!({"text":text(),"kind":nullable(json!({"enum":["acceptance","test"]}))}), &["text"]))
            }),
            &["target", "lease_ms", "check_id", "reason"],
        ),
        "update_task_links" => object(
            json!({"target":target(),"lease_ms":lease(),"reason":text(),"parent_id":nullable(id()),"depends_on":nullable(array(id()))}),
            &["target", "lease_ms", "reason"],
        ),
        "set_task_commit" => object(
            json!({"id":id(),"expected_revision":lease(),"claim":nullable(fence()),"commit":nullable(commit()),"reason":text()}),
            &["id", "expected_revision", "commit", "reason"],
        ),
        "set_task_flag" => object(
            json!({"target":target(),"lease_ms":lease(),"flagged":{"type":"boolean"},"reason":text()}),
            &["target", "lease_ms", "flagged", "reason"],
        ),
        "complete_task" | "complete_session" => target(),
        "cancel_task" => object(
            json!({"id":id(),"claim":nullable(fence()),"reason":text()}),
            &["id", "reason"],
        ),
        _ => unreachable!("catalogue contains only native operations"),
    };
    if matches!(
        operation,
        "create_task"
            | "create_session"
            | "add_task_comment"
            | "progress_task"
            | "checkpoint_session"
            | "release_task"
            | "release_session"
            | "revise_task"
            | "revise_session"
            | "verify_task_check"
            | "supersede_task_check"
            | "update_task_links"
            | "set_task_flag"
            | "complete_task"
            | "cancel_task"
            | "complete_session"
    ) {
        schema["properties"]["references"] = references();
    }
    Some(schema)
}

fn commit() -> Value {
    json!({"type":"string","pattern":"^(?:[0-9a-f]{40}|[0-9a-f]{64})$","description":"Full lowercase Git object ID, as returned by git rev-parse HEAD."})
}
