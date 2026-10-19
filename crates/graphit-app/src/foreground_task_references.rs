//! Finite browser references: native query data and navigation from the same admitted pin.
use crate::task_server::{
    TaskPolicy,
    request::{Arguments, Selection},
};
use graphit_auth::ProjectReadAdmission;
use graphit_core::{
    file_io::SnapshotContext, native_path::NativePath, project_identity::ProjectId,
};
use graphit_hub::native::ContentLimits;
use graphit_records::task_store::{
    NativeTaskStore, TaskReferenceRecord, TaskReferenceResolution, TaskReferencesCursor,
    TaskReferencesQuery, TaskReferencesRead, TaskStoreError,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{num::NonZeroUsize, sync::Arc};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactSelection {
    project_dir: NativePath,
    project_id: ProjectId,
    project_version: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Task,
    Session,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    kind: Kind,
    id: String,
    #[serde(default = "page_size")]
    page_size: NonZeroUsize,
    outgoing_cursor: Option<TaskReferencesCursor>,
    incoming_cursor: Option<TaskReferencesCursor>,
}
fn page_size() -> NonZeroUsize {
    NonZeroUsize::new(20).expect("positive default page size")
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    selection: ExactSelection,
    input: Input,
}

fn check_tree(
    value: &Value,
    limits: ContentLimits,
    nodes: &mut usize,
    depth: usize,
) -> Result<(), &'static str> {
    *nodes = nodes
        .checked_add(1)
        .filter(|n| *n <= limits.max_nodes)
        .ok_or("invalid_arguments")?;
    if depth > limits.max_depth {
        return Err("invalid_arguments");
    }
    match value {
        Value::Array(values) => {
            if values.len() > limits.max_collection_entries {
                return Err("invalid_arguments");
            }
            for value in values {
                check_tree(value, limits, nodes, depth + 1)?;
            }
        }
        Value::Object(values) => {
            if values.len() > limits.max_collection_entries {
                return Err("invalid_arguments");
            }
            for (key, value) in values {
                if key.len() > limits.max_scalar_bytes {
                    return Err("invalid_arguments");
                }
                check_tree(value, limits, nodes, depth + 1)?;
            }
        }
        Value::String(text) if text.len() > limits.max_scalar_bytes => {
            return Err("invalid_arguments");
        }
        Value::Number(number) if number.to_string().len() > limits.max_scalar_bytes => {
            return Err("invalid_arguments");
        }
        _ => {}
    }
    Ok(())
}

/// The byte cap is also applied by the route. Typed serde rejects duplicates before I/O.
pub(crate) fn parse(bytes: &[u8], limits: ContentLimits) -> Result<Arguments, &'static str> {
    if bytes.len() > limits.max_input_bytes {
        return Err("invalid_arguments");
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "invalid_arguments")?;
    check_tree(&value, limits, &mut 0, 1)?;
    let body: Body = serde_json::from_slice(bytes).map_err(|_| "invalid_arguments")?;
    if body.input.page_size.get() > 100
        || body.selection.project_version.is_empty()
        || body.selection.project_version.contains('\0')
    {
        return Err("invalid_arguments");
    }
    let id = Value::String(body.input.id);
    let focus = match body.input.kind {
        Kind::Task => {
            TaskReferenceRecord::Task(serde_json::from_value(id).map_err(|_| "invalid_arguments")?)
        }
        Kind::Session => TaskReferenceRecord::Session(
            serde_json::from_value(id).map_err(|_| "invalid_arguments")?,
        ),
    };
    let query = TaskReferencesQuery {
        focus,
        page_size: body.input.page_size,
        outgoing_cursor: body.input.outgoing_cursor,
        incoming_cursor: body.input.incoming_cursor,
    };
    Ok(Arguments {
        selection: Selection {
            project_dir: Some(body.selection.project_dir),
            project_id: Some(body.selection.project_id),
            project_version: Some(body.selection.project_version),
        },
        input: Some(serde_json::to_value(query).map_err(|_| "invalid_arguments")?),
    })
}

#[derive(Serialize)]
struct Navigation {
    source_href: Option<String>,
    target_href: Option<String>,
}
fn href(observation: &TaskReferenceResolution) -> Result<Option<String>, TaskStoreError> {
    let (mode, id) = match observation {
        TaskReferenceResolution::Task { id, .. } => ("explorer", serde_json::to_value(id)?),
        TaskReferenceResolution::Session { id, .. } => ("sessions", serde_json::to_value(id)?),
        TaskReferenceResolution::Unresolved => return Ok(None),
    };
    let id = id.as_str().ok_or(TaskStoreError::Invalid(
        "invalid observed reference identifier",
    ))?;
    let mut url = url::Url::parse("http://references.invalid/").expect("constant base URL");
    url.path_segments_mut()
        .map_err(|_| TaskStoreError::Invalid("invalid reference navigation base"))?
        .extend(["task", mode, id]);
    Ok(Some(url.path().to_owned()))
}

pub(crate) async fn read(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    policy: &TaskPolicy,
    query: TaskReferencesQuery,
) -> Result<Value, TaskStoreError> {
    let native = NativeTaskStore::references(original, admission, query, policy.read).await?;
    let (outgoing, incoming) = match &native {
        TaskReferencesRead::NoCorpus => (Vec::new(), Vec::new()),
        TaskReferencesRead::Published {
            outgoing, incoming, ..
        } => {
            let navigation = |page: &graphit_records::task_store::TaskReferencePage| {
                page.results
                    .iter()
                    .map(|edge| {
                        Ok(Navigation {
                            source_href: href(&edge.source_observation)?,
                            target_href: href(&edge.target_resolution)?,
                        })
                    })
                    .collect::<Result<Vec<_>, TaskStoreError>>()
            };
            (navigation(outgoing)?, navigation(incoming)?)
        }
    };
    let project = admission.project()?;
    let root = NativePath::new(project.scope.live.owner_root().to_owned())
        .map_err(|_| TaskStoreError::Invalid("invalid reference selection path"))?;
    let project_id = match project.scope.live.owner() {
        graphit_core::artifact_store::StoreOwner::Project { project_id } => *project_id,
        _ => return Err(TaskStoreError::WrongAdmission),
    };
    let selection = json!({"project_dir":root,"project_id":project_id,"project_version":project.version.version()});
    Ok(
        json!({"result":{"selection":selection,"native":native,"navigation":{"outgoing":outgoing,"incoming":incoming}}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_records::task_spec::TaskId;
    use std::num::NonZeroU128;

    fn content() -> ContentLimits {
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 256,
        }
    }
    fn body() -> Value {
        json!({"selection":{"project_dir":NativePath::new(std::env::temp_dir().join("references-test")).unwrap(),"project_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","project_version":"main"},"input":{"kind":"task","id":TaskId::new(NonZeroU128::new(1).unwrap())}})
    }
    #[test]
    fn strict_input_keeps_native_focus_and_default_twenty_with_closed_boundaries() {
        let data = body();
        let parsed = parse(&serde_json::to_vec(&data).unwrap(), content()).unwrap();
        let query: TaskReferencesQuery = serde_json::from_value(parsed.input.unwrap()).unwrap();
        assert_eq!(
            query.focus,
            TaskReferenceRecord::Task(TaskId::new(NonZeroU128::new(1).unwrap()))
        );
        assert_eq!(query.page_size.get(), 20);
        for size in [
            json!(null),
            json!(0),
            json!(101),
            json!(-1),
            json!(20.0),
            json!("20"),
        ] {
            let mut data = body();
            data["input"]["page_size"] = size;
            assert!(parse(&serde_json::to_vec(&data).unwrap(), content()).is_err());
        }
        for size in [1, 100] {
            let mut data = body();
            data["input"]["page_size"] = json!(size);
            let parsed = parse(&serde_json::to_vec(&data).unwrap(), content()).unwrap();
            let query: TaskReferencesQuery = serde_json::from_value(parsed.input.unwrap()).unwrap();
            assert_eq!(query.page_size.get(), size);
        }
        for field in ["project_dir", "project_id", "project_version"] {
            let mut data = body();
            data["selection"].as_object_mut().unwrap().remove(field);
            assert!(parse(&serde_json::to_vec(&data).unwrap(), content()).is_err());
        }
        for patch in [
            json!({"kind":"memory"}),
            json!({"id":"not-a-native-id"}),
            json!({"unknown":true}),
            json!({"outgoing_cursor":{"foreign":true}}),
        ] {
            let mut data = body();
            data["input"]
                .as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            assert!(parse(&serde_json::to_vec(&data).unwrap(), content()).is_err());
        }
    }
    #[test]
    fn duplicate_fields_and_actual_tree_byte_limits_are_rejected() {
        let data = body();
        let duplicate = format!(
            r#"{{"selection":{},"input":{{"kind":"task","kind":"session","id":{}}}}}"#,
            data["selection"], data["input"]["id"]
        );
        assert!(parse(duplicate.as_bytes(), content()).is_err());
        let bytes = serde_json::to_vec(&data).unwrap();
        let mut limits = content();
        limits.max_input_bytes = bytes.len() - 1;
        assert!(parse(&bytes, limits).is_err());
        let mut limits = content();
        limits.max_depth = 1;
        assert!(parse(&bytes, limits).is_err());
        let mut limits = content();
        limits.max_nodes = 1;
        assert!(parse(&bytes, limits).is_err());
        let mut limits = content();
        limits.max_scalar_bytes = 1;
        assert!(parse(&bytes, limits).is_err());
        let mut limits = content();
        limits.max_collection_entries = 1;
        assert!(parse(&bytes, limits).is_err());
    }
    #[test]
    fn unresolved_never_creates_navigation_and_native_observations_use_local_routes() {
        assert_eq!(href(&TaskReferenceResolution::Unresolved).unwrap(), None);
        let id = TaskId::new(NonZeroU128::new(7).unwrap());
        let observed = TaskReferenceResolution::Task {
            id,
            title: "Observed".into(),
            status: graphit_records::gates::TaskStatus::Open,
        };
        assert_eq!(
            href(&observed).unwrap(),
            Some(format!(
                "/task/explorer/{}",
                serde_json::to_value(id).unwrap().as_str().unwrap()
            ))
        );
    }
}
