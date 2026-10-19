use super::*;
use crate::{native_embedding::NativeSdkRoot, native_embedding::worker::GgufRerankWorker};
use graphit_core::file_lock::Cancellation;
use std::{
    num::NonZeroUsize,
    path::PathBuf,
    time::{Duration, Instant},
};

fn options() -> GgufRerankOptions {
    GgufRerankOptions {
        threads: NonZeroU32::new(2).unwrap(),
        max_tokens: NonZeroU32::new(8192).unwrap(),
        provider: LocalProvider::CPU,
        device_id: 0,
        precision: RerankPrecision::F16,
        score: RerankScoreTransform::RawLogit,
    }
}
fn limits() -> NativeRerankLimits {
    NativeRerankLimits::new(8, 2, 65536, 131072, 65536).unwrap()
}
fn context(signal: &Cancellation) -> InferenceContext<'_> {
    InferenceContext::new(signal, Instant::now() + Duration::from_secs(180))
}
fn source() -> GgufModelSource {
    let path = PathBuf::from(
        std::env::var_os("GRAPHIT_BGE_RERANKER_V2_M3_MODEL").expect("pinned F16 GGUF fixture path"),
    );
    GgufModelSource {
        model: BgeRerankRecipe::MODEL_ID.into(),
        revision: BgeRerankRecipe::UPSTREAM_REVISION.into(),
        root: path.parent().unwrap().into(),
        path: path.file_name().unwrap().to_str().unwrap().into(),
        minimum_bytes: 1_159_774_912,
        maximum_bytes: 1_159_774_912,
        sha256: [
            0x3c, 0x2d, 0xe4, 0x08, 0xd2, 0xc0, 0xa8, 0x5a, 0x94, 0x72, 0xdc, 0x09, 0xf9, 0xd5,
            0xa2, 0x2c, 0x9b, 0x73, 0x74, 0x3c, 0x63, 0x43, 0x95, 0x2c, 0x15, 0x05, 0x32, 0x99,
            0xc7, 0x77, 0xc2, 0x98,
        ],
    }
}
#[test]
fn preflight_preserves_context_limits_and_explicit_execution_without_capture() {
    let signal = Cancellation::default();
    let mut selected = options();
    selected.device_id = 1;
    assert_eq!(
        GgufRerankClient::validate_request(selected, limits(), "a", &["b"], context(&signal))
            .unwrap_err()
            .kind(),
        GgufRerankErrorKind::Configuration
    );
    selected = options();
    selected.max_tokens = NonZeroU32::new(8193).unwrap();
    assert_eq!(
        GgufRerankClient::validate_options(selected)
            .unwrap_err()
            .kind(),
        GgufRerankErrorKind::Configuration
    );
    let small = NativeRerankLimits::new(1, 1, 8, 4096, 4096).unwrap();
    assert_eq!(
        GgufRerankClient::validate_request(
            options(),
            small,
            "long query",
            &["b"],
            context(&signal)
        )
        .unwrap_err()
        .kind(),
        GgufRerankErrorKind::Input(NativeRerankError::Limit)
    );
    signal.cancel();
    assert_eq!(
        GgufRerankClient::validate_request(selected, small, "a", &["b"], context(&signal))
            .unwrap_err()
            .kind(),
        GgufRerankErrorKind::Context(ContextError::Cancelled)
    );
    fn send_sync<T: Send + Sync>() {}
    send_sync::<GgufRerankWorker>();
}
#[test]
#[ignore = "requires the pinned real v2-m3 F16 GGUF; paired CPU smoke including the full token bound, independent of reference qualification"]
fn actual_v2_m3_f16_cpu_pairs() {
    actual_v2_m3_f16_pairs(LocalProvider::CPU);
}
#[test]
#[ignore = "requires the pinned real v2-m3 F16 GGUF; bounded output-allocation regression"]
fn actual_v2_m3_f16_bounded_output_cpu() {
    actual_v2_m3_f16_bounded_output(LocalProvider::CPU);
}
#[test]
#[ignore = "requires the pinned real v2-m3 F16 GGUF and actual CUDA backend; bounded output-allocation regression"]
fn actual_v2_m3_f16_bounded_output_cuda() {
    actual_v2_m3_f16_bounded_output(LocalProvider::CUDA);
}
fn actual_v2_m3_f16_bounded_output(provider: LocalProvider) {
    let (_snapshot_root, snapshot_parent) = crate::model_artifact::test_snapshot_parent();
    let _root = NativeSdkRoot::new().unwrap();
    let signal = Cancellation::default();
    let mut selected = options();
    selected.provider = provider;
    selected.max_tokens = NonZeroU32::new(512).unwrap();
    let mut client = GgufRerankClient::load(
        &snapshot_parent,
        source(),
        selected,
        limits(),
        context(&signal),
    )
    .unwrap();
    assert!(client.registered_device().matches_request(provider, 0));
    let query = "What is the capital of France?";
    let relevant = "Paris is the capital of France. ".repeat(100);
    let distractor = "Bananas grow in tropical climates. ".repeat(100);
    let documents = [relevant.as_str(), distractor.as_str()];
    let mut individual = Vec::new();
    for document in documents {
        let request = RequestState::new(context(&signal));
        let pair = client
            .native
            .pin_mut()
            .tokenize_pair(query, document, &request, 512)
            .unwrap();
        assert_eq!(pair.kind, 0);
        assert!(pair.original_token_count > 512);
        assert_eq!(pair.ids.len(), 512);
        let result = client
            .native
            .pin_mut()
            .rerank_tokens(&pair.ids, &request)
            .unwrap();
        assert_eq!(result.kind, 0);
        assert_eq!(result.native_status, 0);
        assert_eq!(result.token_count, 512);
        assert!(result.score.is_finite());
        individual.push(f64::from(result.score));
    }
    let scores = client.score(query, &documents, context(&signal)).unwrap();
    assert_eq!(scores.len(), documents.len());
    for (actual, expected) in scores.iter().zip(&individual) {
        assert!(actual.is_finite());
        assert!((actual - expected).abs() < 1e-5);
    }
    assert!(scores[0] > scores[1]);
    println!("bounded_output provider={provider:?} pair_tokens=512 scores={scores:?}");
    let cancelled = Cancellation::default();
    let request = RequestState::cancel_after_native_callbacks(context(&cancelled), 1);
    let error = client
        .score_with_state(query, &documents, &request)
        .unwrap_err();
    assert_eq!(
        error.kind(),
        GgufRerankErrorKind::Context(ContextError::Cancelled)
    );
    let reused = client.score(query, &documents, context(&signal)).unwrap();
    assert_eq!(reused.len(), scores.len());
    for (actual, expected) in reused.iter().zip(&scores) {
        assert!((actual - expected).abs() < 1e-5);
    }
    let bundle = client._verified_bundle.path().to_owned();
    drop(client);
    assert!(!bundle.exists());
}
#[test]
#[ignore = "requires the pinned real v2-m3 F16 GGUF and actual CUDA backend; paired GPU smoke including the full token bound, independent of reference qualification"]
fn actual_v2_m3_f16_cuda_pairs() {
    actual_v2_m3_f16_pairs(LocalProvider::CUDA);
}
fn actual_v2_m3_f16_pairs(provider: LocalProvider) {
    let (_snapshot_root, snapshot_parent) = crate::model_artifact::test_snapshot_parent();
    let _root = NativeSdkRoot::new().unwrap();
    let signal = Cancellation::default();
    let mut selected = options();
    selected.provider = provider;
    let mut client = GgufRerankClient::load(
        &snapshot_parent,
        source(),
        selected,
        limits(),
        context(&signal),
    )
    .unwrap();
    assert!(client.registered_device().matches_request(provider, 0));
    if provider == LocalProvider::CUDA {
        assert_eq!(client.registered_device().provider, LocalProvider::CUDA);
    }
    let raw = client
        .score(
            "What is the capital of France?",
            &[
                "Paris is the capital of France.",
                "Bananas grow in tropical climates.",
            ],
            context(&signal),
        )
        .unwrap();
    assert_eq!(raw.len(), 2);
    assert!(raw[0] > raw[1]);
    assert!(raw.iter().all(|x| x.is_finite()));
    println!("v2_m3_raw_scores={raw:?}");
    let portuguese = client
        .score(
            "Qual é a capital da França?",
            &[
                "Paris é a capital da França.",
                "Bananas são frutas tropicais.",
            ],
            context(&signal),
        )
        .unwrap();
    assert!(portuguese[0] > portuguese[1]);
    client.options.score = RerankScoreTransform::Sigmoid;
    let sigmoid = client
        .score(
            "What is the capital of France?",
            &[
                "Paris is the capital of France.",
                "Bananas grow in tropical climates.",
            ],
            context(&signal),
        )
        .unwrap();
    for (raw, actual) in raw.iter().zip(sigmoid) {
        assert!((actual - 1.0 / (1.0 + (-raw).exp())).abs() < 1e-12);
    }
    let query = "query ".repeat(128);
    let document = "document ".repeat(512);
    let request = RequestState::new(context(&signal));
    let pair = client
        .native
        .pin_mut()
        .tokenize_pair(&query, &document, &request, 64)
        .unwrap();
    assert_eq!(pair.kind, 0);
    assert!(pair.original_token_count > 64);
    assert_eq!(pair.ids.len(), 64);
    let bounded = client
        .native
        .pin_mut()
        .rerank_tokens(&pair.ids, &request)
        .unwrap();
    assert_eq!(bounded.kind, 0);
    assert_eq!(bounded.native_status, 0);
    assert_eq!(bounded.token_count, 64);
    assert!(bounded.context_tokens >= bounded.token_count && bounded.context_tokens < 8192);
    println!(
        "v2_m3_bounded_pair_tokens={} context_tokens={}",
        bounded.token_count, bounded.context_tokens
    );
    let document = "Paris. ".repeat(5000);
    let request = RequestState::new(context(&signal));
    let pair = client
        .native
        .pin_mut()
        .tokenize_pair("What is the capital of France?", &document, &request, 8192)
        .unwrap();
    assert_eq!(pair.kind, 0);
    assert!(pair.original_token_count > 8192);
    assert_eq!(pair.ids.len(), 8192);
    let full = client
        .native
        .pin_mut()
        .rerank_tokens(&pair.ids, &request)
        .unwrap();
    assert_eq!(full.kind, 0);
    assert_eq!(full.native_status, 0);
    assert_eq!(full.token_count, 8192);
    assert!(full.context_tokens >= full.token_count);
    assert!(full.score.is_finite());
    println!(
        "v2_m3_full_pair_provider={provider:?} tokens={} context_tokens={} score={}",
        full.token_count, full.context_tokens, full.score
    );
    let cancelled = Cancellation::default();
    let request = RequestState::cancel_after_native_callbacks(context(&cancelled), 1);
    let error = client
        .score_with_state(
            "What is the capital of France?",
            &["Paris is the capital of France."],
            &request,
        )
        .unwrap_err();
    assert_eq!(
        error.kind(),
        GgufRerankErrorKind::Context(ContextError::Cancelled)
    );
    assert!(error.native_status().is_some());
    assert!(error.source().is_some());
    let reused = client
        .score(
            "Qual é a capital da França?",
            &["Paris é a capital da França."],
            context(&signal),
        )
        .unwrap();
    assert_eq!(reused.len(), 1);
    let bundle = client._verified_bundle.path().to_owned();
    drop(client);
    assert!(!bundle.exists());
}
#[tokio::test]
#[ignore = "requires the pinned real v2-m3 F16 GGUF; actual bounded worker and joined shutdown"]
async fn actual_v2_m3_f16_worker() {
    let (_snapshot_root, snapshot_parent) = crate::model_artifact::test_snapshot_parent();
    let _root = NativeSdkRoot::new().unwrap();
    let signal = Cancellation::default();
    let worker = GgufRerankWorker::start(
        snapshot_parent.clone(),
        source(),
        options(),
        limits(),
        NonZeroUsize::new(1).unwrap(),
        context(&signal),
    )
    .await
    .unwrap();
    assert!(
        worker
            .registered_device()
            .unwrap()
            .matches_request(LocalProvider::CPU, 0)
    );
    let output = worker
        .score(
            "What is the capital of France?".into(),
            vec![
                "Paris is the capital of France.".into(),
                "Bananas grow in tropical climates.".into(),
            ],
            context(&signal),
        )
        .await
        .unwrap();
    assert_eq!(output.len(), 2);
    assert!(output[0] > output[1]);
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let error = worker
        .score("a".into(), vec!["b".into()], context(&cancelled))
        .await
        .unwrap_err();
    assert_eq!(
        error.kind(),
        crate::native_embedding::worker::NativeWorkerErrorKind::Context(ContextError::Cancelled)
    );
    worker.shutdown(context(&signal)).await.unwrap();
}

#[test]
#[ignore = "requires pinned v2-m3 F16 GGUF and original upstream F32 EN/PT reference"]
fn actual_v2_m3_f16_matches_f32_reference() {
    compare_f32_reference(options(), "cpu");
}
#[test]
#[ignore = "requires pinned v2-m3 F16 GGUF, upstream F32 reference and actual CUDA backend"]
fn actual_v2_m3_f16_cuda_matches_f32_reference() {
    let mut selected = options();
    selected.provider = LocalProvider::CUDA;
    selected.device_id = 0;
    compare_f32_reference(selected, "cuda");
}
fn load_diagnostic(
    snapshot_parent: &CapturedDocumentDirectory,
    source: GgufModelSource,
    selected: GgufRerankOptions,
    limits: NativeRerankLimits,
    context: InferenceContext<'_>,
) -> GgufRerankClient {
    GgufRerankClient::load(snapshot_parent, source, selected, limits, context).unwrap_or_else(
        |error| {
            let mut cause: &dyn std::error::Error = &error;
            for _ in 0..8 {
                if let Some(io) = cause.downcast_ref::<std::io::Error>() {
                    eprintln!(
                        "model_fixture_io kind={:?} native_code={:?}",
                        io.kind(),
                        io.raw_os_error()
                    );
                }
                let Some(next) = cause.source() else {
                    break;
                };
                cause = next;
            }
            panic!("GGUF fixture load failed: {error:?}");
        },
    )
}
fn compare_f32_reference(selected: GgufRerankOptions, device: &str) {
    let (_snapshot_root, snapshot_parent) = crate::model_artifact::test_snapshot_parent();
    let _root = NativeSdkRoot::new().unwrap();
    let signal = Cancellation::default();
    let reference_path = PathBuf::from(
        std::env::var_os("GRAPHIT_BGE_RERANKER_REFERENCE").expect("upstream F32 reference"),
    );
    let reference: serde_json::Value =
        serde_json::from_slice(&std::fs::read(reference_path).unwrap()).unwrap();
    assert_eq!(reference["revision"], BgeRerankRecipe::UPSTREAM_REVISION);
    assert_eq!(reference["reference_dtype"], "float32");
    assert_eq!(reference["score"], "raw_logit");
    let mut client = load_diagnostic(
        &snapshot_parent,
        source(),
        selected,
        limits(),
        context(&signal),
    );
    assert!(
        client
            .registered_device()
            .matches_request(selected.provider, selected.device_id)
    );
    if selected.provider == LocalProvider::CUDA {
        assert_eq!(client.registered_device().provider, LocalProvider::CUDA);
    }
    // This finite conversion regression bound is declared before measuring these cases.
    // It is neither a zero-loss claim nor a retrieval-quality benchmark.
    let absolute_tolerance = 0.05f64;
    let mut maximum_error = 0.0f64;
    let mut observations = Vec::new();
    for scenario in reference["scenarios"].as_array().unwrap() {
        let query = scenario["query"].as_str().unwrap();
        let pairs = scenario["pairs"].as_array().unwrap();
        let documents: Vec<_> = pairs
            .iter()
            .map(|pair| pair["document"].as_str().unwrap())
            .collect();
        for pair in pairs {
            let request = RequestState::new(context(&signal));
            let prepared = client
                .native
                .pin_mut()
                .tokenize_pair(query, pair["document"].as_str().unwrap(), &request, 8192)
                .unwrap();
            assert_eq!(prepared.kind, 0);
            let expected: Vec<_> = pair["input_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| i32::try_from(id.as_i64().unwrap()).unwrap())
                .collect();
            assert_eq!(
                prepared.ids, expected,
                "embedded SDK tokenizer must match the pinned upstream pair IDs"
            );
        }
        let scores = client.score(query, &documents, context(&signal)).unwrap();
        let mut order: Vec<_> = (0..scores.len()).collect();
        order.sort_by(|a, b| scores[*b].total_cmp(&scores[*a]));
        let expected_order: Vec<_> = scenario["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| usize::try_from(id.as_u64().unwrap()).unwrap())
            .collect();
        assert_eq!(order, expected_order);
        let deltas: Vec<_> = scores
            .iter()
            .zip(pairs)
            .map(|(actual, pair)| (actual - pair["logit"].as_f64().unwrap()).abs())
            .collect();
        for delta in &deltas {
            maximum_error = maximum_error.max(*delta);
        }
        observations.push(serde_json::json!({"language":scenario["language"],"scores":scores,"absolute_deltas":deltas,"order":order,"token_ids_match":true}));
    }
    let result = serde_json::json!({"model":BgeRerankRecipe::UPSTREAM_MODEL,"revision":BgeRerankRecipe::UPSTREAM_REVISION,"precision":"f16","reference_precision":"f32","device":device,"maximum_absolute_error":maximum_error,"absolute_tolerance":absolute_tolerance,"scenarios":observations,"zero_loss":false});
    println!("gguf_reference_comparison={result}");
    if let Some(path) = std::env::var_os("GRAPHIT_BGE_RERANKER_COMPARISON") {
        std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
    assert!(
        maximum_error <= absolute_tolerance,
        "F16 conversion error exceeded the declared focal bound: {maximum_error}"
    );
}
