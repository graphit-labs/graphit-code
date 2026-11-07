use arrow_array::RecordBatch;
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_core::{artifact_store::UserIdentity, installation_identity::InstallationId};
use graphit_data::{
    lance_branch_store::{BranchError, LanceBranchStore},
    lance_module_store::LanceModuleStore,
};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{CreateMemory, Memory, MemoryId, MemoryMutation, UpdateMemory},
    user_memory_store::*,
};
use serde_json::{Value, json};
use std::{collections::HashMap, num::NonZeroU128, sync::Arc};

fn owner(installation: &str, issuer: &str, subject: &str) -> UserMemoryOwner {
    UserMemoryOwner {
        installation_id: InstallationId::new(installation).unwrap(),
        identity: UserIdentity::new(issuer.into(), subject.into()).unwrap(),
    }
}
fn expected_user_schema() -> Arc<Schema> {
    let names = [
        "row_key",
        "kind",
        "record_id",
        "installation_id",
        "user_issuer",
        "user_subject",
        "idempotency_key",
        "idempotency_actor",
        "title",
        "revision",
        "created_at",
        "updated_at",
        "created_by",
        "updated_by",
        "superseded",
        "tombstone",
        "reference_generation",
        "references",
        "payload",
    ];
    let mut fields: Vec<Field> = names
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            Field::new(
                name,
                match i {
                    9..=11 | 16 => DataType::UInt64,
                    14 | 15 => DataType::Boolean,
                    _ => DataType::Utf8,
                },
                false,
            )
        })
        .collect();
    fields.extend([
        Field::new("embedding_vector_id", DataType::FixedSizeBinary(32), true),
        Field::new("embedding_fingerprint", DataType::FixedSizeBinary(32), true),
        Field::new("embedding_recipe", DataType::Utf8, true),
        Field::new("embedding_role", DataType::Utf8, true),
        Field::new("embedding_dimensions", DataType::UInt32, true),
        Field::new(
            "embedding_vector",
            DataType::List(Arc::new(Field::new("item", DataType::Float32, true))),
            true,
        ),
    ]);
    Arc::new(Schema::new_with_metadata(
        fields,
        HashMap::from([(
            "graphit.memory.layout".into(),
            "user-v1-embedding-list".into(),
        )]),
    ))
}

#[test]
fn logical_scope_is_literal_compact_json_and_has_no_principal_parser() {
    let base = owner("unit-1", "https://Issuer.example/", " Subject ");
    let scope = UserMemoryScopeId::from_owner(&base).unwrap();
    assert_eq!(
        scope.as_str(),
        r#"["unit-1","https://Issuer.example/"," Subject "]"#
    );
    assert_eq!(
        serde_json::to_value(&scope).unwrap(),
        Value::String(scope.as_str().into())
    );
    for different in [
        owner("unit-2", "https://Issuer.example/", " Subject "),
        owner("unit-1", "https://issuer.example/", " Subject "),
        owner("unit-1", "https://Issuer.example", " Subject "),
        owner("unit-1", "https://Issuer.example/", "Subject"),
    ] {
        assert_ne!(scope, UserMemoryScopeId::from_owner(&different).unwrap());
    }
    let escaped = owner("unit-1", "発行者", "quote\"/\\\n\t");
    let encoded = UserMemoryScopeId::from_owner(&escaped).unwrap();
    let decoded: [String; 3] = serde_json::from_str(encoded.as_str()).unwrap();
    assert_eq!(
        decoded,
        ["unit-1", "発行者", "quote\"/\\\n\t"].map(String::from)
    );
    assert!(encoded.as_str().contains("発行者"));
}

#[test]
fn owner_and_generation_decoders_are_closed_and_reject_normalized_installations() {
    let original = owner("unit-1", " ISSUER ", " Subject ");
    let raw = serde_json::to_value(&original).unwrap();
    assert_eq!(
        raw,
        json!({"installation_id":"unit-1",
        "identity":{"issuer":" ISSUER ","subject":" Subject "}})
    );
    assert_eq!(
        serde_json::from_value::<UserMemoryOwner>(raw.clone()).unwrap(),
        original
    );
    for (pointer, bad) in [
        ("/installation_id", json!(" unit-1 ")),
        ("/installation_id", json!("../unit")),
        ("/installation_id", json!("")),
        ("/identity/issuer", json!("")),
        ("/identity/subject", json!("")),
    ] {
        let mut value = raw.clone();
        *value.pointer_mut(pointer).unwrap() = bad;
        assert!(
            serde_json::from_value::<UserMemoryOwner>(value).is_err(),
            "{pointer}"
        );
    }
    for nested in [false, true] {
        let mut value = raw.clone();
        let object = if nested {
            value["identity"].as_object_mut().unwrap()
        } else {
            value.as_object_mut().unwrap()
        };
        object.insert("authority".into(), json!(true));
        assert!(serde_json::from_value::<UserMemoryOwner>(value).is_err());
    }
    let generation = UserMemoryGeneration {
        owner: original,
        sdk_version: 19,
        physical_branch: None,
        inherited_target: None,
    };
    let mut value = serde_json::to_value(&generation).unwrap();
    assert_eq!(
        serde_json::from_value::<UserMemoryGeneration>(value.clone()).unwrap(),
        generation
    );
    assert!(value.get("project_id").is_none());
    assert!(value.get("project_version").is_none());
    value
        .as_object_mut()
        .unwrap()
        .insert("project_version".into(), json!("main"));
    assert!(serde_json::from_value::<UserMemoryGeneration>(value).is_err());
}

fn memory() -> Memory {
    Memory {
        id: MemoryId::new(NonZeroU128::new(1).unwrap()),
        revision: 1,
        title: "Literal title".into(),
        body: "Literal body".into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Decision,
            mandatory: true,
            important: false,
        },
        tags: vec![" tag ".into(), " tag ".into()],
        references: vec![],
        reference_generation: 1,
        created_by: ulid::Ulid::from(10u128).to_string(),
        updated_by: ulid::Ulid::from(10u128).to_string(),
        created_at: 11,
        updated_at: 11,
    }
}
#[test]
fn audit_and_history_serialize_shared_canonical_domain_without_project_fields() {
    let initial = memory();
    let mut after = initial.clone();
    after.revision = 2;
    after.body = "Changed body".into();
    after.updated_at = 12;
    let owner = owner("unit-1", "Issuer", "subject");
    let creation = UserCreationMemory {
        owner: owner.clone(),
        actor: initial.created_by.clone(),
        input: CreateMemory {
            idempotency_key: "create".into(),
            title: initial.title.clone(),
            body: initial.body.clone(),
            attributes: initial.attributes,
            tags: initial.tags.clone(),
            references: None,
        },
        memory: initial.clone(),
    };
    let revision = UserMemoryRevision {
        owner: owner.clone(),
        actor: initial.updated_by.clone(),
        operation: MemoryMutation::Update(UpdateMemory {
            expected_revision: 1,
            idempotency_key: "update".into(),
            reason: "Literal reason".into(),
            title: None,
            body: Some(after.body.clone()),
            classification: None,
            mandatory: None,
            important: None,
            tags: None,
            references: Some(vec![]),
        }),
        before: initial.clone(),
        after: after.clone(),
    };
    let history = UserMemoryHistory {
        creation: creation.clone(),
        current: after.clone(),
        deleted: false,
        revisions: vec![revision.clone()],
        generation: UserMemoryGeneration {
            owner,
            sdk_version: 2,
            physical_branch: None,
            inherited_target: None,
        },
    };
    let value = serde_json::to_value(&history).unwrap();
    assert_eq!(value["creation"], serde_json::to_value(creation).unwrap());
    assert_eq!(
        value["creation"]["memory"],
        serde_json::to_value(initial).unwrap()
    );
    assert_eq!(
        value["revisions"][0],
        serde_json::to_value(revision).unwrap()
    );
    assert_eq!(
        value["revisions"][0]["after"],
        serde_json::to_value(after).unwrap()
    );
    assert_eq!(
        value["creation"]["input"]["attributes"]["classification"],
        "decision"
    );
    assert!(value["creation"]["input"]["references"].is_null());
    assert_eq!(
        value["revisions"][0]["operation"]["Update"]["references"],
        json!([])
    );
    for object in [
        &value["creation"],
        &value["revisions"][0],
        &value["generation"],
    ] {
        assert!(object.get("project_id").is_none());
        assert!(object.get("operation_version").is_none());
    }
}

#[test]
fn exact_user_layout_uses_25_columns_with_required_literal_owner_and_nullable_embeddings() {
    let schema = user_memory_schema();
    assert_eq!(schema.as_ref(), expected_user_schema().as_ref());
    assert_eq!(schema.fields().len(), 25);
    assert_eq!(schema.field(19).name(), "embedding_vector_id");
    assert_eq!(schema.field(24).name(), "embedding_vector");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_sdk_empty_user_layout_default_pin_survives_close_and_reopen() {
    let root = tempfile::tempdir().unwrap();
    let uri = url::Url::from_file_path(root.path().join("memory.lance"))
        .unwrap()
        .to_string();
    let schema = user_memory_schema();
    let created = LanceModuleStore::create(
        &uri,
        schema.clone(),
        vec![RecordBatch::new_empty(schema.clone())],
    )
    .await
    .unwrap();
    drop(created);
    let store = LanceBranchStore::open(&uri, expected_user_schema(), None)
        .await
        .unwrap();
    let pin = store.capture_target(None).await.unwrap();
    assert_eq!(pin.schema(), expected_user_schema().as_ref());
    assert!(pin.physical_branch().is_none());
    assert!(pin.inherited_target().is_none());
    let generation = UserMemoryGeneration {
        owner: owner("unit-1", "Issuer", "subject"),
        sdk_version: pin.version(),
        physical_branch: pin.physical_branch().map(str::to_owned),
        inherited_target: pin.inherited_target().map(str::to_owned),
    };
    let mut stream = pin.scan().try_into_stream().await.unwrap();
    while let Some(batch) = stream.try_next().await.unwrap() {
        assert_eq!(batch.num_rows(), 0);
    }
    drop(stream);
    drop(pin);
    drop(store);
    let reopened = LanceBranchStore::open(&uri, user_memory_schema(), None)
        .await
        .unwrap();
    let pin = reopened.capture_target(None).await.unwrap();
    assert_eq!(pin.version(), generation.sdk_version);
    assert!(pin.physical_branch().is_none());
    assert!(pin.inherited_target().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_sdk_refuses_project_and_malformed_user_layouts_without_conversion() {
    let root = tempfile::tempdir().unwrap();
    let uri = url::Url::from_file_path(root.path().join("memory.lance"))
        .unwrap()
        .to_string();
    let schema = user_memory_schema();
    let created = LanceModuleStore::create(
        &uri,
        schema.clone(),
        vec![RecordBatch::new_empty(schema.clone())],
    )
    .await
    .unwrap();
    drop(created);
    let mut fields: Vec<Field> = schema.fields().iter().map(|f| f.as_ref().clone()).collect();
    fields.splice(
        3..6,
        [
            Field::new("project_id", DataType::Utf8, false),
            Field::new("operation_version", DataType::Utf8, false),
        ],
    );
    let project = Arc::new(Schema::new_with_metadata(
        fields,
        HashMap::from([(
            "graphit.memory.layout".into(),
            "project-v2-embedding-list".into(),
        )]),
    ));
    assert!(matches!(
        LanceBranchStore::open(&uri, project, None).await,
        Err(BranchError::SchemaMismatch)
    ));
    let metadata_changed = Arc::new(Schema::new_with_metadata(
        schema.fields().clone(),
        HashMap::from([("graphit.memory.layout".into(), "user-v2".into())]),
    ));
    assert!(matches!(
        LanceBranchStore::open(&uri, metadata_changed, None).await,
        Err(BranchError::SchemaMismatch)
    ));
    let mut fields: Vec<Field> = schema.fields().iter().map(|f| f.as_ref().clone()).collect();
    fields[4] = Field::new("user_issuer", DataType::Utf8, true);
    let nullable_owner = Arc::new(Schema::new_with_metadata(fields, schema.metadata().clone()));
    assert!(matches!(
        LanceBranchStore::open(&uri, nullable_owner, None).await,
        Err(BranchError::SchemaMismatch)
    ));
    let reopened = LanceBranchStore::open(&uri, user_memory_schema(), None)
        .await
        .unwrap();
    assert_eq!(reopened.capture_target(None).await.unwrap().version(), 1);
}
