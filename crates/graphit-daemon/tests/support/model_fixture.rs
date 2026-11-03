use super::model_api::*;
use graphit_ai::{
    context::InferenceContext, embedding::EmbeddingLimits, model_manifest::ManifestLimits,
    native_rerank::NativeRerankLimits,
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
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

pub fn cpu_selection() -> LocalExecutionSelection {
    use graphit_auth::profile::{AiService, LocalDevice, LocalExecution};
    let cpu = AiService::Local(LocalExecution {
        device: LocalDevice::Cpu,
        device_id: 0,
    });
    LocalExecutionSelection::from_services(&cpu, &cpu, true).unwrap()
}
pub fn context(cancel: &Cancellation) -> InferenceContext<'_> {
    InferenceContext::new(cancel, Instant::now() + Duration::from_secs(30))
}
pub fn snapshot_parent() -> (
    tempfile::TempDir,
    Arc<graphit_core::native_document_file::CapturedDocumentDirectory>,
) {
    let builder = tempfile::Builder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::PermissionsExt;
        let mut builder = builder;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
        builder
    };
    let root = builder.tempdir().unwrap();
    let original = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    let limits =
        graphit_core::native_document_file::NativeDocumentFileLimits::new(1, 4096, 64, 65536)
            .unwrap();
    let parent = graphit_core::native_document_file::CapturedDocumentDirectory::capture(
        root.path(),
        limits,
        &original,
    )
    .unwrap();
    // Establish the fixture's actual SDK privacy policy before testing artifact outcomes.
    parent.private_handle(&original).unwrap();
    (root, Arc::new(parent))
}
pub fn service(
    snapshot_parent: Arc<graphit_core::native_document_file::CapturedDocumentDirectory>,
) -> ModelService {
    ModelService::new(
        snapshot_parent,
        ServiceLimits {
            max_active_workers: NonZeroUsize::new(4).unwrap(),
            embedding: EmbeddingLimits::new(32, 65536, 768, 32 * 768, 8, 131072, 131072).unwrap(),
            rerank: NativeRerankLimits::new(32, 8, 4096, 8192, 8192).unwrap(),
        },
    )
    .unwrap()
}
pub fn rpc_limits() -> RpcLimits {
    RpcLimits::new(1024 * 1024, 32, 65536, 32 * 768).unwrap()
}
pub fn manifest_limits() -> ManifestLimits {
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
pub fn capture_policy() -> SnapshotPolicy {
    let retention = RetainedBytesLimits::new(1, 1, 256).unwrap();
    SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(4 * 1024 * 1024 * 1024, 32, 4096, 8).unwrap(),
        RetainedFileSet::new([], retention).unwrap(),
        retention,
    )
    .unwrap()
}
pub fn capture(path: &Path, original: &Arc<SnapshotContext>) -> CapturedSourceReader {
    CapturedSourceReader::capture(
        path,
        NodeKind::Directory,
        &capture_policy(),
        SourceReadLimits::new(65536, 512, 65536).unwrap(),
        original.clone(),
    )
    .unwrap()
}
pub struct Fixture {
    pub directory: tempfile::TempDir,
    pub body: Value,
}
impl Fixture {
    /// Captured metadata/integrity only; these bytes are never an inference fixture.
    #[allow(
        dead_code,
        reason = "shared model fixture; metadata validation is exercised by model_service"
    )]
    pub fn metadata(task: TaskKind) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        std::fs::write(
            directory.path().join("model.gguf"),
            b"GGUF integrity-only fixture, not native model weights",
        )
        .unwrap();
        Self::from_bundle(directory, task, None, original)
    }
    /// Real pinned artifact, required explicitly by native integration cases.
    pub fn actual(task: TaskKind) -> Self {
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        Self::actual_with_original(task, &original)
    }
    /// Keep physical fixture preparation under its caller's existing Original.
    pub fn actual_with_original(task: TaskKind, original: &Arc<SnapshotContext>) -> Self {
        original.check().unwrap();
        let environment = match task {
            TaskKind::Embedding => "GRAPHIT_EMBEDDINGGEMMA2_MODEL",
            TaskKind::Rerank => "GRAPHIT_BGE_RERANKER_V2_M3_MODEL",
        };
        let source = PathBuf::from(
            std::env::var_os(environment).expect("required real pinned GGUF fixture"),
        );
        let directory = tempfile::tempdir().unwrap();
        original.check().unwrap();
        let target = directory.path().join("model.gguf");
        // A private inode keeps other fixtures' link/unlink operations from
        // changing this captured source's observed ctime.
        std::fs::copy(&source, &target).expect("copy pinned GGUF into isolated fixture");
        original.check().unwrap();
        Self::from_bundle(
            directory,
            task,
            Some(match task {
                TaskKind::Embedding => [
                    0x68, 0xba, 0xe2, 0x9d, 0x62, 0xfb, 0x8c, 0x7d, 0x23, 0xe9, 0x8d, 0x21, 0xfd,
                    0x46, 0x62, 0x75, 0x3d, 0xdd, 0x63, 0x6e, 0x6b, 0x62, 0xb8, 0xa7, 0x0d, 0xfe,
                    0x05, 0x9a, 0x98, 0x44, 0xf2, 0x16,
                ],
                TaskKind::Rerank => [
                    0x3c, 0x2d, 0xe4, 0x08, 0xd2, 0xc0, 0xa8, 0x5a, 0x94, 0x72, 0xdc, 0x09, 0xf9,
                    0xd5, 0xa2, 0x2c, 0x9b, 0x73, 0x74, 0x3c, 0x63, 0x43, 0x95, 0x2c, 0x15, 0x05,
                    0x32, 0x99, 0xc7, 0x77, 0xc2, 0x98,
                ],
            }),
            original.clone(),
        )
    }
    fn from_bundle(
        directory: tempfile::TempDir,
        task: TaskKind,
        expected: Option<[u8; 32]>,
        original: Arc<SnapshotContext>,
    ) -> Self {
        use graphit_ai::model_manifest::{BgeRerankRecipe, GemmaEmbeddingRecipe};
        let seed = capture(directory.path(), &original);
        let digest = seed
            .nodes()
            .find(|n| n.relative() == Path::new("model.gguf"))
            .unwrap()
            .digest()
            .unwrap();
        if let Some(expected) = expected {
            assert_eq!(
                digest, expected,
                "real GGUF fixture must match the selected upstream artifact pin"
            );
        }
        let (id, revision, runtime, recipe) = match task {
            TaskKind::Embedding => (
                "embeddinggemma2",
                "bfcd298762cc34d0357ece5ebdd31791a3a374d8",
                "gguf-embeddinggemma2",
                json!({
                "architecture":"gemma-embedding2","tokenizer":"embedded","dimensions":768,"max_tokens":8192,"precision":"bf16",
                "attention":"noncausal","pooling":"mean","normalization":"f32_l2","mrl":"truncate_then_f32_l2",
                "query_prefix":GemmaEmbeddingRecipe::QUERY_PREFIX,"document_prefix":GemmaEmbeddingRecipe::DOCUMENT_PREFIX,
                "code_query_prefix":GemmaEmbeddingRecipe::CODE_QUERY_PREFIX,"code_document_prefix":GemmaEmbeddingRecipe::DOCUMENT_PREFIX}),
            ),
            TaskKind::Rerank => (
                BgeRerankRecipe::MODEL_ID,
                BgeRerankRecipe::UPSTREAM_REVISION,
                "gguf-bge-reranker-v2-m3",
                json!({
                "architecture":"bert","tokenizer":"embedded","attention":"noncausal","pooling":"rank","pair_template":"xlm_roberta",
                "truncation":"longest_first_right","outputs":1,"precision":"f16","max_tokens":8192,"score":"raw_logit"}),
            ),
        };
        let body = json!({"schema":"graphit-model/v1","id":id,"revision":revision,"task":task,"runtime":runtime,
            "artifacts":[{"role":"model","path":"model.gguf","digest":digest,"minimum_bytes":1,"required":true,"format":"gguf"}],
            "model_role":"model","recipe":recipe});
        drop(seed);
        // Prepare the default manifest once. A caller that only needs settings
        // must not perform and discard another full artifact capture.
        original.check().unwrap();
        std::fs::write(
            directory.path().join("model.json"),
            serde_json::to_vec(&body).unwrap(),
        )
        .unwrap();
        original.check().unwrap();
        Self { directory, body }
    }
    pub fn settings(&self, slots: &[&str]) -> ModelSettings {
        ModelSettings {
            slots: slots
                .iter()
                .map(|name| SlotSettings {
                    name: (*name).into(),
                    selection: SelectionSettings {
                        id: self.body["id"].as_str().unwrap().into(),
                        revision: self.body["revision"].as_str().unwrap().into(),
                        task: if self.body["task"] == "embedding" {
                            TaskKind::Embedding
                        } else {
                            TaskKind::Rerank
                        },
                    },
                    bundle: self.directory.path().into(),
                    manifest_relative: "model.json".into(),
                    runtime: if self.body["task"] == "embedding" {
                        RuntimeSettings::EmbeddingGemma2 {
                            threads: std::num::NonZeroU32::new(2).unwrap(),
                            queue_capacity: NonZeroUsize::new(8).unwrap(),
                        }
                    } else {
                        RuntimeSettings::BgeRerankerV2M3 {
                            threads: std::num::NonZeroU32::new(2).unwrap(),
                            queue_capacity: NonZeroUsize::new(8).unwrap(),
                        }
                    },
                })
                .collect(),
        }
    }
    pub fn config(&self, slots: &[&str]) -> ResolvedConfig {
        self.resolve(self.settings(slots)).unwrap()
    }
    pub fn resolve(&self, settings: ModelSettings) -> ModelResult<ResolvedConfig> {
        self.resolve_with_selection(settings, cpu_selection())
    }
    pub fn resolve_with_selection(
        &self,
        settings: ModelSettings,
        selection: LocalExecutionSelection,
    ) -> ModelResult<ResolvedConfig> {
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        self.resolve_with_original(settings, selection, &original)
    }
    #[allow(
        dead_code,
        reason = "shared model fixture; original request behavior is exercised by model_service"
    )]
    pub fn config_with_original(
        &self,
        slots: &[&str],
        original: &Arc<SnapshotContext>,
    ) -> ResolvedConfig {
        self.resolve_with_original(self.settings(slots), cpu_selection(), original)
            .unwrap()
    }
    fn resolve_with_original(
        &self,
        settings: ModelSettings,
        selection: LocalExecutionSelection,
        original: &Arc<SnapshotContext>,
    ) -> ModelResult<ResolvedConfig> {
        std::fs::write(
            self.directory.path().join("model.json"),
            serde_json::to_vec(&self.body).unwrap(),
        )
        .unwrap();
        let names: Vec<_> = settings
            .local_slots(selection)
            .map(|s| s.name.clone())
            .collect();
        let inputs = names
            .iter()
            .map(|name| {
                CapturedModelInput::capture(
                    name,
                    self.directory.path(),
                    &capture_policy(),
                    SourceReadLimits::new(65536, 512, 65536).unwrap(),
                    Arc::clone(original),
                )
            })
            .collect::<ModelResult<Vec<_>>>()?;
        ResolvedConfig::resolve(settings, selection, inputs, original, &manifest_limits())
    }
}
