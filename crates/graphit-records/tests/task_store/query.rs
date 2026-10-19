use super::*;
use graphit_records::{
    gates::TaskStatus,
    task_protocol::{ReadRequest, ReadResponse, execute_read},
};
use serde_json::{Value, json};

fn query(selection: TaskQuerySelection, page_size: usize, top_k: usize) -> TaskQuery {
    TaskQuery {
        selection,
        columns: vec![TaskQueryColumn::Id, TaskQueryColumn::Status],
        page_size: NonZeroUsize::new(page_size).unwrap(),
        top_k: NonZeroUsize::new(top_k).unwrap(),
        cursor: None,
    }
}
fn tasks(ids: Option<Vec<TaskId>>) -> TaskQuerySelection {
    TaskQuerySelection::Tasks {
        ids,
        status: Some(TaskStatus::Open),
        session_id: None,
    }
}
fn page(read: TaskQueryRead) -> (Vec<Vec<Value>>, Option<TaskQueryCursor>) {
    let TaskQueryRead::Published {
        columns,
        rows,
        next_cursor,
        ..
    } = read
    else {
        panic!("published query")
    };
    assert_eq!(columns, vec![TaskQueryColumn::Id, TaskQueryColumn::Status]);
    (rows, next_cursor)
}
#[test]
fn query_wire_rejects_unknown_fields_private_columns_and_invalid_ids_or_limits() {
    let valid = json!({"operation":"query", "input":{"selection":{"table":"tasks","ids":["01KSH1CRFFG8Z74B5ZS78WW808"],"status":null},"columns":["id","status"],"page_size":1,"top_k":2}});
    assert!(matches!(
        serde_json::from_value::<ReadRequest>(valid.clone()).unwrap(),
        ReadRequest::Query(_)
    ));
    for (pointer, bad) in [
        ("/input/page_size", json!(0)),
        ("/input/top_k", json!(0)),
        ("/input/columns", json!(["claim_token"])),
        ("/input/columns", json!(["payload"])),
        ("/input/selection/ids", json!(["tsk-legacy"])),
        (
            "/input/selection/ids",
            json!(["00000000000000000000000000"]),
        ),
        ("/input/selection/status", json!("unknown")),
    ] {
        let mut value = valid.clone();
        *value
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("pointer {pointer}")) = bad;
        assert!(
            serde_json::from_value::<ReadRequest>(value).is_err(),
            "{pointer}"
        );
    }
    let schema = serde_json::to_value(schemars::schema_for!(TaskQuery)).unwrap();
    assert_eq!(
        schema["properties"]["page_size"]["minimum"].as_f64(),
        Some(1.0)
    );
    assert_eq!(schema["properties"]["top_k"]["minimum"].as_f64(), Some(1.0));
    assert_eq!(schema["$defs"]["TaskQueryCursor"]["type"], json!("string"));
    assert_eq!(schema["additionalProperties"], json!(false));
    let mut object_cursor = valid.clone();
    object_cursor["input"]["cursor"] = json!({});
    assert!(serde_json::from_value::<ReadRequest>(object_cursor).is_err());
    for pointer in ["/input", "/input/selection"] {
        let mut value = valid.clone();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("actor".into(), json!("forged"));
        assert!(serde_json::from_value::<ReadRequest>(value).is_err());
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finite_query_projection_pages_schema_and_public_empty_state() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let schema = execute_read(
        ReadRequest::Schema,
        ctx.snapshot(),
        &read,
        read_limits(),
        NonZeroUsize::new(1024).unwrap(),
    )
    .await
    .unwrap();
    let ReadResponse::Schema(schema) = schema else {
        panic!("schema response")
    };
    assert_eq!(schema.tables.len(), 2);
    assert_eq!(schema.tables[0].table, TaskQueryTable::Tasks);
    assert_eq!(schema.tables[1].table, TaskQueryTable::Sessions);
    assert!(
        schema.tables[1]
            .columns
            .iter()
            .all(|c| c.name != TaskQueryColumn::ParentId)
    );
    assert!(
        schema.tables[0]
            .columns
            .iter()
            .any(|c| c.name == TaskQueryColumn::ParentId
                && c.nullable
                && c.data_type == TaskQueryType::Ulid)
    );
    let initial = query(tasks(None), 1, 2);
    assert_eq!(
        NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), initial.clone())
            .await
            .unwrap(),
        TaskQueryRead::NoCorpus
    );
    assert!(!f.project.runtime_root().join("task").exists());
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("query"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let (empty, cursor) = page(
        NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), initial)
            .await
            .unwrap(),
    );
    assert!(empty.is_empty());
    assert!(cursor.is_none());
    let mut ids = Vec::new();
    for key in ["q1", "q2", "q3"] {
        ids.push(
            NativeTaskStore::create_task(
                ctx.snapshot(),
                &write,
                task_input(session.id, key),
                graphit_records::reference_projection::AuthoredReferencesField::Omitted,
                write_limits(),
            )
            .await
            .unwrap()
            .specification
            .id,
        );
    }
    ids.sort_by_key(|id| id.get());
    let other_session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("other-query-session"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let other_task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(other_session.id, "other-query-task"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    NativeTaskStore::cancel_task(
        ctx.snapshot(),
        &write,
        other_task.specification.id,
        TaskCancellation {
            claim: None,
            reason: "Query status fixture",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let subset = query(tasks(Some(vec![ids[1]])), 1, 2);
    assert_eq!(
        page(
            NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), subset)
                .await
                .unwrap()
        )
        .0,
        vec![vec![json!(ids[1]), json!("open")]]
    );
    let status_filter = query(
        TaskQuerySelection::Tasks {
            ids: None,
            status: Some(TaskStatus::Cancelled),
            session_id: None,
        },
        1,
        2,
    );
    assert_eq!(
        page(
            NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), status_filter)
                .await
                .unwrap()
        )
        .0,
        vec![vec![json!(other_task.specification.id), json!("cancelled")]]
    );
    let session_filter = query(
        TaskQuerySelection::Tasks {
            ids: None,
            status: None,
            session_id: Some(other_session.id),
        },
        1,
        2,
    );
    assert_eq!(
        page(
            NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), session_filter)
                .await
                .unwrap()
        )
        .0,
        vec![vec![json!(other_task.specification.id), json!("cancelled")]]
    );
    let mut scalars = query(tasks(Some(vec![ids[0]])), 1, 2);
    scalars.columns = schema.tables[0].columns.iter().map(|c| c.name).collect();
    let TaskQueryRead::Published {
        columns: scalar_columns,
        rows: scalar_rows,
        ..
    } = NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), scalars)
        .await
        .unwrap()
    else {
        panic!("scalar query")
    };
    assert_eq!(scalar_rows.len(), 1);
    let scalar_values: std::collections::HashMap<_, _> = scalar_columns
        .into_iter()
        .zip(scalar_rows.into_iter().next().unwrap())
        .collect();
    assert_eq!(scalar_values[&TaskQueryColumn::ParentId], Value::Null);
    assert_eq!(
        scalar_values[&TaskQueryColumn::SessionId],
        json!(session.id)
    );
    assert_eq!(scalar_values[&TaskQueryColumn::Priority], json!(2));
    assert_eq!(scalar_values[&TaskQueryColumn::Flagged], json!(false));
    assert_eq!(scalar_values[&TaskQueryColumn::TaskType], json!("feature"));
    assert_eq!(scalar_values[&TaskQueryColumn::Revision], json!(1));

    let query_input = query(tasks(Some(ids.clone())), 1, 2);
    let response = execute_read(
        ReadRequest::Query(query_input.clone()),
        ctx.snapshot(),
        &read,
        read_limits(),
        NonZeroUsize::new(1024).unwrap(),
    )
    .await
    .unwrap();
    let ReadResponse::Query(response) = response else {
        panic!("query response")
    };
    let (first, cursor) = page(response);
    assert_eq!(first, vec![vec![json!(ids[0]), json!("open")]]);
    let mut next = query_input.clone();
    next.cursor =
        Some(serde_json::from_value(serde_json::to_value(cursor.unwrap()).unwrap()).unwrap());
    let (second, end) = page(
        NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), next.clone())
            .await
            .unwrap(),
    );
    assert_eq!(second, vec![vec![json!(ids[1]), json!("open")]]);
    assert!(end.is_none());
    let mut all = query(tasks(Some(ids.clone())), 2, 10);
    let (first, cursor) = page(
        NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), all.clone())
            .await
            .unwrap(),
    );
    assert_eq!(first.len(), 2);
    all.cursor = cursor;
    let (last, end) = page(
        NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), all)
            .await
            .unwrap(),
    );
    assert_eq!(last, vec![vec![json!(ids[2]), json!("open")]]);
    assert!(end.is_none());
    for mut changed in [
        next.clone(),
        next.clone(),
        next.clone(),
        next.clone(),
        next.clone(),
    ]
    .into_iter()
    .enumerate()
    {
        match changed.0 {
            0 => changed.1.columns.reverse(),
            1 => changed.1.page_size = NonZeroUsize::new(2).unwrap(),
            2 => changed.1.top_k = NonZeroUsize::new(3).unwrap(),
            3 => changed.1.selection = tasks(Some(vec![ids[2]])),
            _ => {
                changed.1.selection = TaskQuerySelection::Sessions {
                    ids: None,
                    status: None,
                }
            }
        }
        assert!(matches!(
            NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), changed.1).await,
            Err(TaskStoreError::InvalidCursor)
        ));
    }
    let mut no_ids = query_input.clone();
    no_ids.selection = tasks(Some(vec![]));
    assert!(
        page(
            NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), no_ids)
                .await
                .unwrap()
        )
        .0
        .is_empty()
    );
    let sessions = query(
        TaskQuerySelection::Sessions {
            ids: Some(vec![session.id]),
            status: Some(TaskStatus::Open),
        },
        1,
        2,
    );
    assert_eq!(
        page(
            NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), sessions.clone())
                .await
                .unwrap()
        )
        .0,
        vec![vec![json!(session.id), json!("open")]]
    );
    let mut invalid = sessions;
    invalid.columns.push(TaskQueryColumn::SessionId);
    assert!(matches!(
        NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), invalid).await,
        Err(TaskStoreError::Invalid(_))
    ));
    for columns in [vec![], vec![TaskQueryColumn::Id, TaskQueryColumn::Id]] {
        let mut invalid = query_input.clone();
        invalid.columns = columns;
        assert!(matches!(
            NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), invalid).await,
            Err(TaskStoreError::Invalid(_))
        ));
    }
    let another = Fixture::new(&ctx, "main");
    admissions!(another, ctx, issuer2, authority2, grant2, read2, write2);
    NativeTaskStore::create_session(
        ctx.snapshot(),
        &write2,
        session_input("other-published-root"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeTaskStore::query(ctx.snapshot(), &read2, read_limits(), next.clone()).await,
        Err(TaskStoreError::InvalidCursor)
    ));
    let other_cancel = Cancellation::default();
    let other_ctx = request(&other_cancel);
    assert!(matches!(
        NativeTaskStore::query(
            other_ctx.snapshot(),
            &read,
            read_limits(),
            query_input.clone()
        )
        .await,
        Err(TaskStoreError::DifferentRequest)
    ));
    NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "q4"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), next).await,
        Err(TaskStoreError::InvalidCursor)
    ));
    assert!(matches!(
        NativeTaskStore::query_schema(other_ctx.snapshot(), &read),
        Err(TaskStoreError::DifferentRequest)
    ));
    cancel.cancel();
    assert!(
        NativeTaskStore::query(ctx.snapshot(), &read, read_limits(), query_input)
            .await
            .is_err()
    );
    assert!(NativeTaskStore::query_schema(ctx.snapshot(), &read).is_err());
}
