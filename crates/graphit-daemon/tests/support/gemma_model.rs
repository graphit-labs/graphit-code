use super::model_api::*;
use graphit_core::{
    artifact_store::{
        CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet, SnapshotContext,
        SnapshotPolicy, SourceReadLimits,
    },
    file_lock::Cancellation,
};
use serde_json::json;
use std::{
    num::{NonZeroU32, NonZeroUsize},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

pub struct GemmaFixture {
    pub directory: tempfile::TempDir,
}
impl GemmaFixture {
    pub fn new(model: &Path) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("model.gguf");
        // Core observations include inode change time. A private inode avoids
        // invalidating concurrent snapshots when another fixture creates a link.
        std::fs::copy(model, &destination).unwrap();
        let digest = [
            0x68, 0xba, 0xe2, 0x9d, 0x62, 0xfb, 0x8c, 0x7d, 0x23, 0xe9, 0x8d, 0x21, 0xfd, 0x46,
            0x62, 0x75, 0x3d, 0xdd, 0x63, 0x6e, 0x6b, 0x62, 0xb8, 0xa7, 0x0d, 0xfe, 0x05, 0x9a,
            0x98, 0x44, 0xf2, 0x16,
        ];
        for dimensions in [128, 768] {
            let manifest = json!({"schema":"graphit-model/v1","id":"embeddinggemma2","revision":"bfcd298762cc34d0357ece5ebdd31791a3a374d8",
                "task":"embedding","runtime":"gguf-embeddinggemma2","model_role":"model",
                "artifacts":[{"role":"model","path":"model.gguf","digest":digest,"minimum_bytes":557950176u64,"required":true,"format":"gguf"}],
                "recipe":{"architecture":"gemma-embedding2","tokenizer":"embedded","dimensions":dimensions,"max_tokens":8192,"precision":"bf16",
                    "attention":"noncausal","pooling":"mean","normalization":"f32_l2","mrl":"truncate_then_f32_l2",
                    "query_prefix":"task: search result | query: ","document_prefix":"title: none | text: ",
                    "code_query_prefix":"task: code retrieval | query: ","code_document_prefix":"title: none | text: "}});
            std::fs::write(
                directory.path().join(format!("{dimensions}.json")),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
        }
        Self { directory }
    }
    pub fn config(&self, slots: &[(&str, u64)], signal: &Cancellation) -> ResolvedConfig {
        self.try_config(slots, signal).unwrap()
    }
    pub fn try_config(
        &self,
        slots: &[(&str, u64)],
        signal: &Cancellation,
    ) -> ModelResult<ResolvedConfig> {
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(180),
            signal.clone(),
        ));
        let settings = ModelSettings {
            slots: slots
                .iter()
                .map(|(name, dim)| SlotSettings {
                    name: (*name).into(),
                    selection: SelectionSettings {
                        id: "embeddinggemma2".into(),
                        revision: "bfcd298762cc34d0357ece5ebdd31791a3a374d8".into(),
                        task: TaskKind::Embedding,
                    },
                    bundle: self.directory.path().into(),
                    manifest_relative: format!("{dim}.json").into(),
                    runtime: RuntimeSettings::EmbeddingGemma2 {
                        threads: NonZeroU32::new(2).unwrap(),
                        queue_capacity: NonZeroUsize::new(8).unwrap(),
                    },
                })
                .collect(),
        };
        let retention = RetainedBytesLimits::new(1, 1, 256).unwrap();
        let policy = SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            CopyLimits::new(600_000_000, 32, 4096, 8).unwrap(),
            RetainedFileSet::new([], retention).unwrap(),
            retention,
        )
        .unwrap();
        let inputs = settings
            .slots
            .iter()
            .map(|slot| {
                CapturedModelInput::capture(
                    &slot.name,
                    &slot.bundle,
                    &policy,
                    SourceReadLimits::new(65536, 4096, 600_000_000).unwrap(),
                    original.clone(),
                )
            })
            .collect::<ModelResult<Vec<_>>>()?;
        let mut limits = super::fixture::manifest_limits();
        limits.max_tokens = 8192;
        ResolvedConfig::resolve(
            settings,
            super::fixture::cpu_selection(),
            inputs,
            &original,
            &limits,
        )
    }
}
