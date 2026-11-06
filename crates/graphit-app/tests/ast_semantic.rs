//! Captured/resolved model metadata → actual NativeNN schema and empty codec.
use graphit_app::ast_semantic::expected_native_nn_model;
use graphit_ast::native_nn::{
    NativeNnError, encode_native_nn_ts, native_nn_schema, validate_native_nn,
};
use graphit_core::{artifact_store::SnapshotContext, file_lock::Cancellation};
use graphit_daemon::model_service as model_api;
use model_api::{ExpectedModel, ModelErrorKind};
use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) use crate::model_fixture as fixture;

fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}

#[test]
fn actual_resolved_owner_drives_complete_empty_identity_after_config_drop() {
    let fixture = fixture::Fixture::metadata(model_api::TaskKind::Embedding);
    let config = fixture.config(&["a"]);
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    let id = expected.vector_id();
    drop(config);
    let model = expected_native_nn_model(&expected).unwrap();
    assert_eq!(model.vector_id, id);
    assert_eq!(model.dimensions, 768);
    assert_eq!(
        (model.model_id, model.revision),
        (
            "embeddinggemma2",
            "bfcd298762cc34d0357ece5ebdd31791a3a374d8"
        )
    );
    assert_eq!(model.model_id.as_ptr(), expected.model().as_ptr());
    assert_eq!(model.revision.as_ptr(), expected.revision().as_ptr());
    let context = original();
    let batch = encode_native_nn_ts(&model, &[], &context).unwrap();
    validate_native_nn(&batch, &model, &context).unwrap();
    assert_eq!(batch.num_rows(), 0);
    assert_eq!(
        batch.schema().as_ref(),
        native_nn_schema(&model).unwrap().as_ref()
    );
    let data_key = batch.schema().field_with_name("data_key").unwrap().clone();
    assert!(
        batch
            .column_by_name("data_key")
            .unwrap()
            .as_any()
            .is::<arrow_array::StringArray>()
    );
    assert!(!data_key.is_nullable());
    assert!(
        batch
            .schema()
            .field_with_name("model_generation")
            .unwrap()
            .is_nullable()
    );
    let schema = batch.schema();
    let metadata = schema.metadata();
    assert_eq!(metadata.len(), 8);
    assert_eq!(metadata["graphit.module"], "ast");
    assert_eq!(
        metadata["graphit.ast.nn.format"],
        graphit_ast::native_nn::NATIVE_NN_FORMAT
    );
    assert_eq!(
        metadata["graphit.ast.nn.vector_id"],
        id.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    assert_eq!(metadata["graphit.ast.nn.dimensions"], "768");
    assert_eq!(metadata["graphit.ast.nn.model_id"], "embeddinggemma2");
    assert_eq!(
        metadata["graphit.ast.nn.revision"],
        "bfcd298762cc34d0357ece5ebdd31791a3a374d8"
    );
    assert_eq!(metadata["graphit.ast.nn.role"], "CodeDocument");
    assert_eq!(
        metadata["graphit.ast.nn.projection_recipe"],
        "ast-parser-entity-original-1"
    );
}

#[test]
fn same_width_changed_validated_revision_requires_distinct_schema() {
    let mut fixture = fixture::Fixture::metadata(model_api::TaskKind::Embedding);
    let old_config = fixture.config(&["a"]);
    let old_expected = ExpectedModel::new(&old_config, "a", "service").unwrap();
    drop(old_config);
    let old_model = expected_native_nn_model(&old_expected).unwrap();
    let context = original();
    let old_batch = encode_native_nn_ts(&old_model, &[], &context).unwrap();
    fixture.body["revision"] = serde_json::json!("fixture-2");
    let mut settings = fixture.settings(&["a"]);
    settings.slots[0].selection.revision = "fixture-2".into();
    let new_config = fixture.resolve(settings).unwrap();
    let new_expected = ExpectedModel::new(&new_config, "a", "service").unwrap();
    drop(new_config);
    let new_model = expected_native_nn_model(&new_expected).unwrap();
    assert_eq!(old_model.dimensions, new_model.dimensions);
    assert_ne!(old_model.vector_id, new_model.vector_id);
    assert_eq!(
        (old_model.revision, new_model.revision),
        ("bfcd298762cc34d0357ece5ebdd31791a3a374d8", "fixture-2")
    );
    assert!(matches!(
        validate_native_nn(&old_batch, &new_model, &context),
        Err(NativeNnError::SchemaMismatch)
    ));
    validate_native_nn(&old_batch, &old_model, &context).unwrap();
    let new_batch = encode_native_nn_ts(&new_model, &[], &context).unwrap();
    validate_native_nn(&new_batch, &new_model, &context).unwrap();
    assert_ne!(old_batch.schema(), new_batch.schema());
}

#[test]
fn actual_rerank_selection_and_original_controls_return_typed_errors() {
    let fixture = fixture::Fixture::metadata(model_api::TaskKind::Rerank);
    let config = fixture.config(&["r"]);
    let expected = ExpectedModel::new(&config, "r", "service").unwrap();
    assert_eq!(expected.dimensions(), None);
    assert!(matches!(
        expected_native_nn_model(&expected),
        Err(NativeNnError::InvalidModel)
    ));
    assert_eq!(
        ExpectedModel::new(&config, "missing", "service")
            .err()
            .unwrap()
            .kind(),
        ModelErrorKind::Selection
    );
    assert_eq!(
        ExpectedModel::new(&config, "r", "").err().unwrap().kind(),
        ModelErrorKind::Selection
    );
    let embed = fixture::Fixture::metadata(model_api::TaskKind::Embedding);
    let config = embed.config(&["a"]);
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    let model = expected_native_nn_model(&expected).unwrap();
    let cancelled = original();
    cancelled.cancellation().cancel();
    let expired = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    for (context, kind) in [
        (&cancelled, std::io::ErrorKind::Interrupted),
        (&expired, std::io::ErrorKind::TimedOut),
    ] {
        let error = encode_native_nn_ts(&model, &[], context).err().unwrap();
        let NativeNnError::Operation(cause) = &error else {
            panic!("expected actual Original control cause")
        };
        assert_eq!(cause.kind(), kind);
        assert!(error.source().unwrap().is::<std::io::Error>());
    }
}

#[test]
fn portable_vector_identity_survives_installation_and_runtime_changes() {
    let producer = fixture::Fixture::metadata(model_api::TaskKind::Embedding);
    let consumer = fixture::Fixture::metadata(model_api::TaskKind::Embedding);
    let first = producer.config(&["a"]);
    let first = ExpectedModel::new(&first, "a", "service").unwrap();
    let mut settings = consumer.settings(&["a"]);
    let model_api::RuntimeSettings::EmbeddingGemma2 {
        threads,
        queue_capacity,
    } = &mut settings.slots[0].runtime
    else {
        panic!("captured GGUF metadata fixture")
    };
    *threads = std::num::NonZeroU32::new(3).unwrap();
    *queue_capacity = std::num::NonZeroUsize::new(9).unwrap();
    let second = consumer.resolve(settings).unwrap();
    let second = ExpectedModel::new(&second, "a", "service").unwrap();
    assert_ne!(first.config_id(), second.config_id());
    assert_eq!(first.vector_id(), second.vector_id());
    let first_model = expected_native_nn_model(&first).unwrap();
    let second_model = expected_native_nn_model(&second).unwrap();
    let context = original();
    let batch = encode_native_nn_ts(&first_model, &[], &context).unwrap();
    validate_native_nn(&batch, &second_model, &context).unwrap();
}

#[test]
fn verified_vector_identity_tracks_recipe_and_embedded_gguf_integrity() {
    let mut fixture = fixture::Fixture::metadata(model_api::TaskKind::Embedding);
    let first = fixture.config(&["a"]);
    let first = ExpectedModel::new(&first, "a", "service").unwrap();
    fixture.body["recipe"]["max_tokens"] = serde_json::json!(4096);
    let recipe = fixture.config(&["a"]);
    let recipe = ExpectedModel::new(&recipe, "a", "service").unwrap();
    assert_eq!(first.dimensions(), recipe.dimensions());
    assert_ne!(first.vector_id(), recipe.vector_id());
    let context = original();
    let batch =
        encode_native_nn_ts(&expected_native_nn_model(&first).unwrap(), &[], &context).unwrap();
    assert!(matches!(
        validate_native_nn(
            &batch,
            &expected_native_nn_model(&recipe).unwrap(),
            &context
        ),
        Err(NativeNnError::SchemaMismatch)
    ));
    let path = fixture.directory.path().join("model.gguf");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.push(b'\n');
    std::fs::write(&path, bytes).unwrap();
    assert!(
        fixture.resolve(fixture.settings(&["a"])).is_err(),
        "unverified GGUF artifact bytes must be rejected"
    );
    let captured = fixture::capture(fixture.directory.path(), &context);
    let digest = captured
        .nodes()
        .find(|node| node.relative() == std::path::Path::new("model.gguf"))
        .unwrap()
        .digest()
        .unwrap();
    fixture.body["artifacts"][0]["digest"] = serde_json::json!(digest);
    drop(captured);
    let artifact = fixture.config(&["a"]);
    let artifact = ExpectedModel::new(&artifact, "a", "service").unwrap();
    assert_eq!(recipe.dimensions(), artifact.dimensions());
    assert_ne!(recipe.vector_id(), artifact.vector_id());
}
