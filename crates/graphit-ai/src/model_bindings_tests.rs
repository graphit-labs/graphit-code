//! Real captured-source integrity fixtures; bytes are never submitted to an SDK.
use crate::{
    model_bindings::{ModelBindings, bind_captured_model},
    model_manifest::*,
};
use graphit_core::{
    artifact_store::{
        CapturedSourceReader, CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet,
        SnapshotContext, SnapshotPolicy, SourceReadLimits,
    },
    file_lock::Cancellation,
};
use graphit_fs::NodeKind;
use serde_json::{Value, json};
use std::{
    fs, io,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn limits() -> ManifestLimits {
    ManifestLimits {
        max_manifest_bytes: 32768,
        max_artifacts: 8,
        max_role_bytes: 128,
        max_relative_path_bytes: 256,
        max_id_bytes: 128,
        max_revision_bytes: 128,
        max_dimension: 8192,
        max_tokens: 8192,
    }
}
fn policy() -> SnapshotPolicy {
    let retention = RetainedBytesLimits::new(1, 1, 256).unwrap();
    SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(1024 * 1024, 32, 4096, 8).unwrap(),
        RetainedFileSet::new([], retention).unwrap(),
        retention,
    )
    .unwrap()
}
fn capture(path: &Path, original: &Arc<SnapshotContext>) -> CapturedSourceReader {
    CapturedSourceReader::capture(
        path,
        NodeKind::Directory,
        &policy(),
        SourceReadLimits::new(65536, 512, 65536).unwrap(),
        original.clone(),
    )
    .unwrap()
}
fn gemma_recipe() -> Value {
    json!({"architecture":"gemma-embedding2","tokenizer":"embedded","dimensions":768,"max_tokens":8192,"precision":"bf16","attention":"noncausal","pooling":"mean","normalization":"f32_l2","mrl":"truncate_then_f32_l2","query_prefix":GemmaEmbeddingRecipe::QUERY_PREFIX,"document_prefix":GemmaEmbeddingRecipe::DOCUMENT_PREFIX,"code_query_prefix":GemmaEmbeddingRecipe::CODE_QUERY_PREFIX,"code_document_prefix":GemmaEmbeddingRecipe::DOCUMENT_PREFIX})
}
fn rerank_recipe() -> Value {
    json!({"architecture":"bert","tokenizer":"embedded","attention":"noncausal","pooling":"rank","pair_template":"xlm_roberta","truncation":"longest_first_right","max_tokens":8192,"precision":"f16","score":"raw_logit","outputs":1})
}
struct Fixture {
    directory: tempfile::TempDir,
    original: Arc<SnapshotContext>,
    body: Value,
    reader: CapturedSourceReader,
}
impl Fixture {
    fn new(task: ModelTask, mutate: impl FnOnce(&mut Value)) -> Self {
        Self::with_deadline(task, Duration::from_secs(30), mutate)
    }
    fn with_deadline(task: ModelTask, duration: Duration, mutate: impl FnOnce(&mut Value)) -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("weights.gguf"),
            b"GGUF integrity-only fixture; never native input",
        )
        .unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + duration,
            Cancellation::default(),
        ));
        let seed = capture(directory.path(), &original);
        let digest = seed
            .nodes()
            .find(|n| n.relative() == Path::new("weights.gguf"))
            .unwrap()
            .digest()
            .unwrap();
        let (runtime, task_name, recipe) = match task {
            ModelTask::Embedding => ("gguf-embeddinggemma2", "embedding", gemma_recipe()),
            ModelTask::Rerank => ("gguf-bge-reranker-v2-m3", "rerank", rerank_recipe()),
        };
        let mut body = json!({"schema":"graphit-model/v1","id":"fixture-model","revision":"fixture-r1","task":task_name,"runtime":runtime,"artifacts":[{"role":"weights-custom","path":"weights.gguf","digest":digest,"minimum_bytes":1,"required":true,"format":"gguf"}],"model_role":"weights-custom","recipe":recipe});
        drop(seed);
        mutate(&mut body);
        fs::write(
            directory.path().join("model.json"),
            serde_json::to_vec(&body).unwrap(),
        )
        .unwrap();
        let reader = capture(directory.path(), &original);
        Self {
            directory,
            original,
            body,
            reader,
        }
    }
    fn bind(
        &self,
        task: ModelTask,
        bounds: &ManifestLimits,
    ) -> Result<ModelBindings, ModelBindingError> {
        bind_captured_model(
            &self.reader,
            &self.original,
            Path::new("model.json"),
            &ModelSelection {
                id: "fixture-model",
                revision: "fixture-r1",
                task,
            },
            bounds,
        )
    }
}
fn rejected(result: Result<ModelBindings, ModelBindingError>) -> ModelBindingError {
    match result {
        Err(e) => e,
        Ok(_) => panic!("expected binding rejection"),
    }
}
fn decode_error(body: &Value) -> ModelBindingError {
    match decode_manifest(
        &serde_json::to_vec(body).unwrap(),
        &limits(),
        &mut || Ok(()),
    ) {
        Err(e) => e,
        Ok(_) => panic!("expected strict schema rejection"),
    }
}

#[test]
fn distinct_embedding_and_paired_rerank_bind_actual_digests_and_recipes() {
    for task in [ModelTask::Embedding, ModelTask::Rerank] {
        let fixture = Fixture::new(task, |_| {});
        let bound = fixture.bind(task, &limits()).unwrap();
        assert_eq!(bound.snapshot_fingerprint, fixture.reader.fingerprint());
        assert_eq!(
            bound.manifest.artifacts[0].digest,
            fixture
                .reader
                .nodes()
                .find(|n| n.relative() == Path::new("weights.gguf"))
                .unwrap()
                .digest()
                .unwrap()
        );
        match bound.manifest.backend {
            ModelBackend::EmbeddingGemma2 { recipe, .. } => {
                assert_eq!(recipe.dimensions, 768);
                assert_eq!(recipe.precision, GemmaPrecision::Bf16);
            }
            ModelBackend::BgeRerankerV2M3 { recipe, .. } => {
                assert_eq!(recipe.max_tokens, 8192);
                assert_eq!(recipe.precision, RerankPrecision::F16);
                assert_eq!(recipe.score, RerankScoreTransform::RawLogit);
            }
        }
    }
}
#[test]
fn all_embedding_mrl_and_explicit_precision_selections_bind() {
    for dimensions in [128, 256, 512, 768] {
        for precision in ["bf16", "f32"] {
            let fixture = Fixture::new(ModelTask::Embedding, |b| {
                b["recipe"]["dimensions"] = json!(dimensions);
                b["recipe"]["precision"] = json!(precision);
            });
            fixture.bind(ModelTask::Embedding, &limits()).unwrap();
        }
    }
    let fixture = Fixture::new(ModelTask::Rerank, |b| {
        b["recipe"]["score"] = json!("sigmoid")
    });
    let ModelBackend::BgeRerankerV2M3 { recipe, .. } = fixture
        .bind(ModelTask::Rerank, &limits())
        .unwrap()
        .manifest
        .backend
    else {
        panic!("rerank recipe")
    };
    assert_eq!(recipe.score, RerankScoreTransform::Sigmoid);
}
#[test]
fn model_families_reject_cross_task_fields_and_unsupported_recipes() {
    type Mutation = fn(&mut Value);
    let gemma: &[Mutation] = &[
        |b| b["task"] = json!("rerank"),
        |b| b["recipe"]["pooling"] = json!("rank"),
        |b| b["recipe"]["dimensions"] = json!(1),
        |b| b["recipe"]["precision"] = json!("f16"),
        |b| b["recipe"]["max_tokens"] = json!(8193),
        |b| b["recipe"]["query_prefix"] = json!("wrong"),
        |b| b["recipe"]["score"] = json!("raw_logit"),
    ];
    let rerank: &[Mutation] = &[
        |b| b["task"] = json!("embedding"),
        |b| b["recipe"]["pooling"] = json!("mean"),
        |b| b["recipe"]["outputs"] = json!(1024),
        |b| b["recipe"]["precision"] = json!("f32"),
        |b| b["recipe"]["precision"] = json!("q8_0"),
        |b| b["recipe"]["max_tokens"] = json!(8193),
        |b| b["recipe"]["max_tokens"] = json!(3),
        |b| b["recipe"]["score"] = json!("auto"),
        |b| b["recipe"]["pair_template"] = json!("cosine"),
        |b| b["recipe"]["truncation"] = json!("silent"),
        |b| b["recipe"]["dimensions"] = json!(768),
    ];
    for (task, cases) in [(ModelTask::Embedding, gemma), (ModelTask::Rerank, rerank)] {
        for mutate in cases {
            let fixture = Fixture::new(task, *mutate);
            assert!(fixture.bind(task, &limits()).is_err());
        }
    }
}
#[test]
fn removed_runtimes_and_external_tensor_fields_are_rejected() {
    let fixture = Fixture::new(ModelTask::Embedding, |_| {});
    for runtime in ["onnx", "gguf-bge-reranker-base", "unknown"] {
        let mut b = fixture.body.clone();
        b["runtime"] = json!(runtime);
        decode_error(&b);
    }
    for key in [
        "tokenizer_role",
        "inputs",
        "output",
        "text",
        "inference",
        "external_data_roles",
    ] {
        let mut b = fixture.body.clone();
        b[key] = json!("removed");
        assert_eq!(decode_error(&b).cause, BindingCause::UnknownField);
    }
}
#[test]
fn required_paths_roles_formats_and_digest_bind_to_physical_capture() {
    type Mutation = fn(&mut Value);
    let cases: &[Mutation] = &[
        |b| b["artifacts"][0]["path"] = json!("../escape.gguf"),
        |b| b["artifacts"][0]["path"] = json!("a\\b.gguf"),
        |b| b["artifacts"][0]["path"] = json!("/absolute.gguf"),
        |b| b["artifacts"][0]["digest"] = json!([1, 2]),
        |b| b["model_role"] = json!("absent"),
        |b| b["artifacts"][0]["required"] = json!(false),
        |b| b["artifacts"][0]["format"] = json!("onnx"),
    ];
    for mutate in cases {
        let fixture = Fixture::new(ModelTask::Embedding, *mutate);
        assert!(fixture.bind(ModelTask::Embedding, &limits()).is_err());
    }
    let fixture = Fixture::new(ModelTask::Rerank, |b| {
        b["artifacts"][0]["digest"] = json!(vec![0; 32])
    });
    assert_eq!(
        rejected(fixture.bind(ModelTask::Rerank, &limits())).stage,
        BindingStage::Artifact
    );
    let fixture = Fixture::new(ModelTask::Rerank, |_| {});
    assert_eq!(
        rejected(fixture.bind(ModelTask::Embedding, &limits())).stage,
        BindingStage::Selection
    );
}
#[test]
fn duplicate_artifact_roles_and_recipe_keys_fail_without_exposing_input() {
    let fixture = Fixture::new(ModelTask::Embedding, |b| {
        let a = b["artifacts"][0].clone();
        b["artifacts"].as_array_mut().unwrap().push(a);
    });
    assert_eq!(
        rejected(fixture.bind(ModelTask::Embedding, &limits())).cause,
        BindingCause::Duplicate
    );
    let fixture = Fixture::new(ModelTask::Embedding, |_| {});
    let wire = serde_json::to_string(&fixture.body).unwrap().replacen(
        "\"precision\":\"bf16\"",
        "\"precision\":\"bf16\",\"precision\":\"private-sensitive-model\"",
        1,
    );
    let error = match decode_manifest(wire.as_bytes(), &limits(), &mut || Ok(())) {
        Err(e) => e,
        Ok(_) => panic!("duplicate recipe accepted"),
    };
    assert_eq!(error.cause, BindingCause::Duplicate);
    assert!(!format!("{error:?} {error}").contains("private-sensitive-model"));
}
#[test]
fn optional_absence_is_allowed_but_present_wrong_digest_is_not() {
    let optional = |b: &mut Value| {
        b["artifacts"].as_array_mut().unwrap().push(json!({"role":"optional","path":"absent.gguf","digest":vec![0;32],"minimum_bytes":1,"required":false,"format":"gguf"}))
    };
    let fixture = Fixture::new(ModelTask::Embedding, optional);
    fixture.bind(ModelTask::Embedding, &limits()).unwrap();
    fs::write(
        fixture.directory.path().join("absent.gguf"),
        b"actual peer artifact",
    )
    .unwrap();
    let reader = capture(fixture.directory.path(), &fixture.original);
    assert!(
        bind_captured_model(
            &reader,
            &fixture.original,
            Path::new("model.json"),
            &ModelSelection {
                id: "fixture-model",
                revision: "fixture-r1",
                task: ModelTask::Embedding
            },
            &limits()
        )
        .is_err()
    );
}
#[test]
fn manifest_and_recipe_limits_accept_exact_caps_and_reject_one_above() {
    let fixture = Fixture::new(ModelTask::Rerank, |_| {});
    let mut exact = limits();
    exact.max_manifest_bytes = serde_json::to_vec(&fixture.body).unwrap().len();
    fixture.bind(ModelTask::Rerank, &exact).unwrap();
    exact.max_manifest_bytes -= 1;
    assert_eq!(
        rejected(fixture.bind(ModelTask::Rerank, &exact)).cause,
        BindingCause::Capacity
    );
    let mut cap = limits();
    cap.max_tokens = 8191;
    assert_eq!(
        rejected(fixture.bind(ModelTask::Rerank, &cap)).cause,
        BindingCause::Capacity
    );
    let mut cap = limits();
    cap.max_id_bytes = "fixture-model".len() - 1;
    assert_eq!(
        rejected(fixture.bind(ModelTask::Rerank, &cap)).stage,
        BindingStage::Selection
    );
}
#[test]
fn original_identity_cancel_deadline_and_peer_mutation_are_terminal() {
    let fixture = Fixture::new(ModelTask::Embedding, |_| {});
    let foreign = SnapshotContext::new(
        fixture.original.required_deadline().unwrap(),
        fixture.original.cancellation().clone(),
    );
    assert!(
        bind_captured_model(
            &fixture.reader,
            &foreign,
            Path::new("model.json"),
            &ModelSelection {
                id: "fixture-model",
                revision: "fixture-r1",
                task: ModelTask::Embedding
            },
            &limits()
        )
        .is_err()
    );
    let file = fixture.directory.path().join("weights.gguf");
    crate::model_bindings::fences::before_revalidate(move || {
        fs::write(file, b"actual peer mutation").unwrap()
    });
    assert_eq!(
        rejected(fixture.bind(ModelTask::Embedding, &limits())).stage,
        BindingStage::Revalidate
    );
    let fixture = Fixture::new(ModelTask::Embedding, |_| {});
    let cancel = fixture.original.cancellation().clone();
    crate::model_bindings::fences::before_revalidate(move || cancel.cancel());
    assert_eq!(
        rejected(fixture.bind(ModelTask::Embedding, &limits())).kind,
        io::ErrorKind::Interrupted
    );
    let fixture = Fixture::with_deadline(ModelTask::Embedding, Duration::from_millis(50), |_| {});
    std::thread::sleep(
        fixture
            .original
            .required_deadline()
            .unwrap()
            .saturating_duration_since(Instant::now())
            + Duration::from_millis(2),
    );
    assert_eq!(
        rejected(fixture.bind(ModelTask::Embedding, &limits())).kind,
        io::ErrorKind::TimedOut
    );
}
#[test]
fn decoder_checks_preserve_the_original_cancellation() {
    let fixture = Fixture::new(ModelTask::Rerank, |_| {});
    let bytes = serde_json::to_vec(&fixture.body).unwrap();
    let mut checks = 0;
    let error = match decode_manifest(&bytes, &limits(), &mut || {
        checks += 1;
        if checks == 2 {
            fixture.original.cancellation().cancel();
        }
        fixture.reader.check_request(&fixture.original)
    }) {
        Err(e) => e,
        Ok(_) => panic!("cancelled parse accepted"),
    };
    assert_eq!(error.kind, io::ErrorKind::Interrupted);
    assert_eq!(error.stage, BindingStage::Request);
    assert!(checks >= 2);
}
