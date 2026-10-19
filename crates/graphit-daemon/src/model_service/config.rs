use super::local_execution::local_provider;
use super::{LocalExecutionSelection, ModelErrorKind, ModelResult, ModelServiceError};
use graphit_ai::{
    gguf_rerank::{GgufRerankClient, GgufRerankOptions},
    model_bindings::{bind_captured_model, read_captured_model_manifest},
    model_manifest::{
        ArtifactFormat, BgeRerankRecipe, ManifestLimits, ModelBackend, ModelManifest,
        ModelSelection, ModelTask, RerankScoreTransform, portable_path,
    },
    native_embedding::{NativeEmbeddingOptions, NativeWeightPrecision},
};
use graphit_auth::profile::{LocalDevice, LocalExecution};
use graphit_core::{
    artifact_store::{CapturedSourceReader, SnapshotContext, SnapshotPolicy, SourceReadLimits},
    config::{ConfigKey, ConfigSnapshot},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    num::{NonZeroU32, NonZeroUsize},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Equality data only: never an authorization or an artifact publication proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct ConfigId(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Embedding,
    Rerank,
}
impl TaskKind {
    pub(crate) fn native(self) -> ModelTask {
        match self {
            Self::Embedding => ModelTask::Embedding,
            Self::Rerank => ModelTask::Rerank,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionSettings {
    pub id: String,
    pub revision: String,
    pub task: TaskKind,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "backend", deny_unknown_fields)]
pub enum RuntimeSettings {
    #[serde(rename = "gguf-bge-reranker-v2-m3")]
    BgeRerankerV2M3 {
        threads: NonZeroU32,
        queue_capacity: NonZeroUsize,
    },
    #[serde(rename = "gguf-embeddinggemma2")]
    EmbeddingGemma2 {
        threads: NonZeroU32,
        queue_capacity: NonZeroUsize,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotSettings {
    pub name: String,
    pub selection: SelectionSettings,
    pub bundle: PathBuf,
    pub manifest_relative: PathBuf,
    pub runtime: RuntimeSettings,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSettings {
    pub slots: Vec<SlotSettings>,
}
impl ModelSettings {
    /// Global residency policy, independent of model and vector identity.
    pub fn idle_timeout(snapshot: &ConfigSnapshot) -> ModelResult<Duration> {
        let key = ConfigKey::new("models.idle_timeout_seconds").map_err(ModelServiceError::io)?;
        let seconds = snapshot
            .get::<u64>(&key)
            .map_err(ModelServiceError::io)?
            .unwrap_or(DEFAULT_IDLE_TIMEOUT.as_secs());
        validate_idle_timeout(Duration::from_secs(seconds))
    }
    /// Missing is distinct from an explicitly empty configuration.
    pub fn read(snapshot: &ConfigSnapshot) -> ModelResult<Option<Self>> {
        let key = ConfigKey::new("daemon.models").map_err(ModelServiceError::io)?;
        snapshot.get(&key).map_err(ModelServiceError::io)
    }
}
pub struct CapturedModelInput {
    slot: String,
    bundle: PathBuf,
    reader: CapturedSourceReader,
}
impl CapturedModelInput {
    /// Borrow the captured reader for validation under its original request.
    /// This cannot replace the bundle binding or manufacture another capture.
    pub fn reader(&self) -> &CapturedSourceReader {
        &self.reader
    }

    /// Capture this exact bundle under the caller's original request. The private
    /// fields prevent an unrelated reader from being paired with a load path.
    pub fn capture(
        slot: &str,
        bundle: &Path,
        policy: &SnapshotPolicy,
        limits: SourceReadLimits,
        original: Arc<SnapshotContext>,
    ) -> ModelResult<Self> {
        original.check().map_err(ModelServiceError::io)?;
        let bundle = bundle.canonicalize().map_err(ModelServiceError::io)?;
        original.check().map_err(ModelServiceError::io)?;
        let reader = CapturedSourceReader::capture(
            &bundle,
            graphit_fs::NodeKind::Directory,
            policy,
            limits,
            original.clone(),
        )
        .map_err(ModelServiceError::io)?;
        original.check().map_err(ModelServiceError::io)?;
        Ok(Self {
            slot: slot.to_owned(),
            bundle,
            reader,
        })
    }
}
pub(crate) enum ResolvedExecution {
    BgeRerankerV2M3 {
        options: GgufRerankOptions,
        queue_capacity: NonZeroUsize,
    },
    EmbeddingGemma2 {
        options: NativeEmbeddingOptions,
        queue_capacity: NonZeroUsize,
    },
}
pub(crate) struct ResolvedModel {
    pub id: ConfigId,
    pub vector_id: [u8; 32],
    pub manifest: Arc<ModelManifest>,
    pub bundle: PathBuf,
    pub execution: ResolvedExecution,
    pub device: LocalExecution,
}
#[derive(Clone)]
pub struct ResolvedConfig {
    pub(crate) slots: BTreeMap<String, Arc<ResolvedModel>>,
    pub(super) idle_timeout: Duration,
}
impl ResolvedConfig {
    pub fn idle_timeout(&self) -> Duration {
        self.idle_timeout
    }
    pub fn with_idle_timeout(mut self, timeout: Duration) -> ModelResult<Self> {
        self.idle_timeout = validate_idle_timeout(timeout)?;
        Ok(self)
    }
    /// Complete slot identity map; generation remains owned by ModelService.
    pub fn config_ids(&self) -> BTreeMap<String, ConfigId> {
        self.slots
            .iter()
            .map(|(slot, model)| (slot.clone(), model.id))
            .collect()
    }
    pub fn empty() -> Self {
        Self {
            slots: BTreeMap::new(),
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        }
    }
    pub fn resolve(
        settings: ModelSettings,
        selection: LocalExecutionSelection,
        inputs: impl IntoIterator<Item = CapturedModelInput>,
        original: &SnapshotContext,
        limits: &ManifestLimits,
    ) -> ModelResult<Self> {
        original.check().map_err(ModelServiceError::io)?;
        selection.validate()?;
        let mut captures = BTreeMap::new();
        for input in inputs {
            if captures.insert(input.slot.clone(), input).is_some() {
                return Err(ModelServiceError::new(ModelErrorKind::Configuration));
            }
        }
        let resolved = Self::compile(settings, selection, original, |slot| {
            let captured = captures
                .remove(&slot.name)
                .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Configuration))?;
            if slot.bundle != captured.bundle {
                return Err(ModelServiceError::new(ModelErrorKind::Configuration));
            }
            let binding = bind_captured_model(
                &captured.reader,
                original,
                &slot.manifest_relative,
                &ModelSelection {
                    id: &slot.selection.id,
                    revision: &slot.selection.revision,
                    task: slot.selection.task.native(),
                },
                limits,
            )?;
            Ok(binding.manifest)
        })?;
        if !captures.is_empty() {
            return Err(ModelServiceError::new(ModelErrorKind::Configuration));
        }
        Ok(resolved)
    }

    /// Runtime configuration declares identity; physical weights are checked by
    /// the existing native staging producer before the first load and inference.
    pub fn resolve_declared(
        settings: ModelSettings,
        selection: LocalExecutionSelection,
        original: &Arc<SnapshotContext>,
        policy: &SnapshotPolicy,
        source: SourceReadLimits,
        limits: &ManifestLimits,
    ) -> ModelResult<Self> {
        Self::compile(settings, selection, original, |slot| {
            if slot.manifest_relative.to_str().is_none_or(|path| {
                path.len() > limits.max_relative_path_bytes || !portable_path(path)
            }) {
                return Err(ModelServiceError::new(ModelErrorKind::Configuration));
            }
            let reader = CapturedSourceReader::capture(
                &slot.bundle.join(&slot.manifest_relative),
                graphit_fs::NodeKind::File,
                policy,
                source,
                original.clone(),
            )
            .map_err(ModelServiceError::io)?;
            if !reader
                .source_path(original)
                .map_err(ModelServiceError::io)?
                .starts_with(&slot.bundle)
            {
                return Err(ModelServiceError::new(ModelErrorKind::Configuration));
            }
            let manifest = read_captured_model_manifest(
                &reader,
                original,
                Path::new(""),
                &ModelSelection {
                    id: &slot.selection.id,
                    revision: &slot.selection.revision,
                    task: slot.selection.task.native(),
                },
                limits,
            )?;
            // Both runtime backends consume one self-contained GGUF, including
            // its tokenizer. Never accept an auxiliary integrity declaration
            // that the native loader would not actually verify or consume.
            if manifest.artifacts.len() != 1 {
                return Err(ModelServiceError::caused(
                    ModelErrorKind::Configuration,
                    std::io::Error::from(std::io::ErrorKind::Unsupported),
                ));
            }
            reader.revalidate_content().map_err(ModelServiceError::io)?;
            original.check().map_err(ModelServiceError::io)?;
            Ok(manifest)
        })
    }

    fn compile(
        settings: ModelSettings,
        selection: LocalExecutionSelection,
        original: &SnapshotContext,
        mut manifest_for_slot: impl FnMut(&SlotSettings) -> ModelResult<ModelManifest>,
    ) -> ModelResult<Self> {
        original.check().map_err(ModelServiceError::io)?;
        selection.validate()?;
        let mut slots = BTreeMap::new();
        for mut slot in settings.slots {
            let Some(device) = selection.for_task(slot.selection.task) else {
                continue;
            };
            original.check().map_err(ModelServiceError::io)?;
            if slot.name.is_empty()
                || slot.name.len() > 128
                || !slot
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                || slots.contains_key(&slot.name)
            {
                return Err(ModelServiceError::new(ModelErrorKind::Configuration));
            }
            slot.bundle = slot.bundle.canonicalize().map_err(ModelServiceError::io)?;
            original.check().map_err(ModelServiceError::io)?;
            let manifest = manifest_for_slot(&slot)?;
            original.check().map_err(ModelServiceError::io)?;
            let id = identity(&manifest, &slot, device)?;
            let vector_id = vector_identity(&manifest, &slot.runtime)?;
            let execution = match (slot.runtime, &manifest.backend) {
                (
                    RuntimeSettings::BgeRerankerV2M3 {
                        threads,
                        queue_capacity,
                    },
                    ModelBackend::BgeRerankerV2M3 { recipe, .. },
                ) if queue_capacity.get() <= tokio::sync::Semaphore::MAX_PERMITS
                    && manifest.task == ModelTask::Rerank =>
                {
                    let options = GgufRerankOptions {
                        threads,
                        max_tokens: u32::try_from(recipe.max_tokens)
                            .ok()
                            .and_then(NonZeroU32::new)
                            .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Configuration))?,
                        provider: local_provider(device.device),
                        device_id: device.device_id as i32,
                        precision: recipe.precision,
                        score: recipe.score,
                    };
                    GgufRerankClient::validate_options(options)?;
                    ResolvedExecution::BgeRerankerV2M3 {
                        options,
                        queue_capacity,
                    }
                }
                (
                    RuntimeSettings::EmbeddingGemma2 {
                        threads,
                        queue_capacity,
                    },
                    ModelBackend::EmbeddingGemma2 { recipe, .. },
                ) if threads.get() <= i32::MAX as u32
                    && queue_capacity.get() <= tokio::sync::Semaphore::MAX_PERMITS
                    && manifest.task == ModelTask::Embedding =>
                {
                    use graphit_ai::model_manifest::GemmaPrecision;
                    let options = NativeEmbeddingOptions {
                        provider: local_provider(device.device),
                        device_id: device.device_id as i32,
                        threads,
                        max_tokens: u32::try_from(recipe.max_tokens)
                            .ok()
                            .and_then(NonZeroU32::new)
                            .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Configuration))?,
                        dimensions: usize::try_from(recipe.dimensions)
                            .map_err(ModelServiceError::configuration)?,
                        precision: match recipe.precision {
                            GemmaPrecision::Bf16 => NativeWeightPrecision::Bf16,
                            GemmaPrecision::F32 => NativeWeightPrecision::F32,
                        },
                    };
                    ResolvedExecution::EmbeddingGemma2 {
                        options,
                        queue_capacity,
                    }
                }
                _ => return Err(ModelServiceError::new(ModelErrorKind::Configuration)),
            };
            slots.insert(
                slot.name,
                Arc::new(ResolvedModel {
                    id,
                    vector_id,
                    manifest: Arc::new(manifest),
                    bundle: slot.bundle,
                    execution,
                    device,
                }),
            );
        }
        original.check().map_err(ModelServiceError::io)?;
        Ok(Self {
            slots,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        })
    }
    pub fn config_id(&self, slot: &str) -> Option<ConfigId> {
        self.slots.get(slot).map(|s| s.id)
    }
}

fn validate_idle_timeout(timeout: Duration) -> ModelResult<Duration> {
    if timeout.is_zero() || Instant::now().checked_add(timeout).is_none() {
        return Err(ModelServiceError::new(ModelErrorKind::Configuration));
    }
    Ok(timeout)
}

fn hash_bytes(h: &mut Sha256, b: &[u8]) {
    h.update((b.len() as u64).to_le_bytes());
    h.update(b);
}
fn hash_text(h: &mut Sha256, s: &str) {
    hash_bytes(h, s.as_bytes());
}

fn hash_recipe(hash: &mut Sha256, manifest: &ModelManifest) {
    match &manifest.backend {
        ModelBackend::BgeRerankerV2M3 { model_role, recipe } => {
            hash_text(
                hash,
                "gguf-bge-reranker-v2-m3/llama.cpp/4fbc76dec51d0add466f0210855c0596589b60d4",
            );
            hash_text(hash, model_role);
            for value in [
                BgeRerankRecipe::ARCHITECTURE,
                "embedded/noncausal/rank/1/f16",
                BgeRerankRecipe::PAIR_TEMPLATE,
                BgeRerankRecipe::TRUNCATION,
            ] {
                hash_text(hash, value);
            }
            hash.update(recipe.max_tokens.to_le_bytes());
            hash.update([match recipe.score {
                RerankScoreTransform::RawLogit => 0,
                RerankScoreTransform::Sigmoid => 1,
            }]);
        }
        graphit_ai::model_manifest::ModelBackend::EmbeddingGemma2 { model_role, recipe } => {
            use graphit_ai::model_manifest::{GemmaEmbeddingRecipe, GemmaPrecision};
            hash_text(
                hash,
                "gguf-embeddinggemma2/llama.cpp/4fbc76dec51d0add466f0210855c0596589b60d4",
            );
            hash_text(hash, model_role);
            hash_text(hash, GemmaEmbeddingRecipe::ARCHITECTURE);
            hash_text(hash, "embedded/noncausal/mean/f32_l2/truncate_then_f32_l2");
            hash_text(hash, GemmaEmbeddingRecipe::QUERY_PREFIX);
            hash_text(hash, GemmaEmbeddingRecipe::DOCUMENT_PREFIX);
            hash_text(hash, GemmaEmbeddingRecipe::CODE_QUERY_PREFIX);
            hash.update(recipe.dimensions.to_le_bytes());
            hash.update(recipe.max_tokens.to_le_bytes());
            hash.update([match recipe.precision {
                GemmaPrecision::Bf16 => 0,
                GemmaPrecision::F32 => 1,
            }]);
        }
    }
}

fn vector_identity(manifest: &ModelManifest, _runtime: &RuntimeSettings) -> ModelResult<[u8; 32]> {
    let mut hash = Sha256::new();
    hash_text(&mut hash, "graphit-vector-space-v1");
    hash_text(&mut hash, &manifest.id);
    hash_text(&mut hash, &manifest.revision);
    hash.update([match manifest.task {
        ModelTask::Embedding => 0,
        ModelTask::Rerank => 1,
    }]);
    let mut roles: Vec<&str> = match &manifest.backend {
        ModelBackend::EmbeddingGemma2 { model_role, .. }
        | ModelBackend::BgeRerankerV2M3 { model_role, .. } => vec![model_role.as_str()],
    };
    roles.sort_unstable();
    hash.update((roles.len() as u64).to_le_bytes());
    for role in roles {
        let artifact = manifest
            .artifacts
            .iter()
            .find(|artifact| artifact.role == role && artifact.required)
            .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Configuration))?;
        hash_text(&mut hash, role);
        hash_bytes(&mut hash, &artifact.digest);
    }
    hash_recipe(&mut hash, manifest);
    Ok(hash.finalize().into())
}

fn identity(
    manifest: &ModelManifest,
    slot: &SlotSettings,
    device: LocalExecution,
) -> ModelResult<ConfigId> {
    let mut hash = Sha256::new();
    // The pinned SDK/runtime ABI is part of provisioning equality.
    hash_text(&mut hash, "graphit-model-config-v3");
    hash_text(&mut hash, &manifest.id);
    hash_text(&mut hash, &manifest.revision);
    hash.update([match manifest.task {
        ModelTask::Embedding => 0,
        ModelTask::Rerank => 1,
    }]);
    let mut artifacts: Vec<_> = manifest.artifacts.iter().collect();
    artifacts.sort_by(|a, b| (&a.role, &a.path).cmp(&(&b.role, &b.path)));
    hash.update((artifacts.len() as u64).to_le_bytes());
    for a in artifacts {
        hash_text(&mut hash, &a.role);
        hash_text(&mut hash, &a.path);
        hash_bytes(&mut hash, &a.digest);
        hash.update(a.minimum_bytes.to_le_bytes());
        hash.update([u8::from(a.required)]);
        hash.update([match a.format {
            ArtifactFormat::Gguf => 3,
        }]);
    }
    hash_recipe(&mut hash, manifest);
    hash.update([match device.device {
        LocalDevice::Auto => 0,
        LocalDevice::Cpu => 1,
        LocalDevice::Cuda => 2,
        LocalDevice::Metal => 3,
    }]);
    hash.update(device.device_id.to_le_bytes());
    hash_text(
        &mut hash,
        slot.bundle
            .to_str()
            .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Configuration))?,
    );
    hash_text(
        &mut hash,
        slot.manifest_relative
            .to_str()
            .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Configuration))?,
    );
    hash_bytes(
        &mut hash,
        &serde_json::to_vec(&slot.runtime).map_err(ModelServiceError::configuration)?,
    );
    Ok(ConfigId(hash.finalize().into()))
}

#[cfg(test)]
mod backend_identity_tests {
    use super::*;
    use graphit_ai::model_manifest::{
        GemmaEmbeddingRecipe, GemmaPrecision, ModelArtifact, ModelBackend,
    };

    fn manifest(precision: GemmaPrecision, dimensions: u64, max_tokens: u64) -> ModelManifest {
        ModelManifest {
            id: "embeddinggemma2".into(),
            revision: "model-revision".into(),
            task: ModelTask::Embedding,
            artifacts: vec![ModelArtifact {
                role: "model".into(),
                path: "model.gguf".into(),
                digest: [9; 32],
                minimum_bytes: 1,
                required: true,
                format: ArtifactFormat::Gguf,
            }],
            backend: ModelBackend::EmbeddingGemma2 {
                model_role: "model".into(),
                recipe: GemmaEmbeddingRecipe {
                    dimensions,
                    max_tokens,
                    precision,
                },
            },
        }
    }
    #[test]
    fn gemma_config_identity_tracks_precision_recipe_revision_and_artifact() {
        // Identity data only: this fixture never opens a model or starts a runtime.
        let slot = SlotSettings {
            name: "embedding".into(),
            selection: SelectionSettings {
                id: "embeddinggemma2".into(),
                revision: "model-revision".into(),
                task: TaskKind::Embedding,
            },
            bundle: PathBuf::from("models/embeddinggemma2"),
            manifest_relative: PathBuf::from("manifest.json"),
            runtime: RuntimeSettings::EmbeddingGemma2 {
                threads: NonZeroU32::new(4).unwrap(),
                queue_capacity: NonZeroUsize::new(8).unwrap(),
            },
        };
        let device = LocalExecution {
            device: LocalDevice::Cpu,
            device_id: 0,
        };
        let original = manifest(GemmaPrecision::Bf16, 768, 8192);
        let expected = identity(&original, &slot, device).unwrap();
        let vector = vector_identity(&original, &slot.runtime).unwrap();
        assert_eq!(expected, identity(&original, &slot, device).unwrap());
        let gpu = LocalExecution {
            device: LocalDevice::Cuda,
            device_id: 2,
        };
        assert_ne!(expected, identity(&original, &slot, gpu).unwrap());
        assert_ne!(
            identity(&original, &slot, gpu).unwrap(),
            identity(
                &original,
                &slot,
                LocalExecution {
                    device_id: 1,
                    ..gpu
                }
            )
            .unwrap()
        );
        // Hardware changes provisioning equality; the vector recipe has no device input.
        assert_eq!(vector, vector_identity(&original, &slot.runtime).unwrap());
        for changed in [
            manifest(GemmaPrecision::F32, 768, 8192),
            manifest(GemmaPrecision::Bf16, 128, 8192),
            manifest(GemmaPrecision::Bf16, 768, 4096),
        ] {
            assert_ne!(expected, identity(&changed, &slot, device).unwrap());
            assert_ne!(vector, vector_identity(&changed, &slot.runtime).unwrap());
        }
        let mut changed = manifest(GemmaPrecision::Bf16, 768, 8192);
        changed.revision.push_str("-new");
        assert_ne!(expected, identity(&changed, &slot, device).unwrap());
        assert_ne!(vector, vector_identity(&changed, &slot.runtime).unwrap());
        let mut changed = manifest(GemmaPrecision::Bf16, 768, 8192);
        changed.artifacts[0].digest[0] ^= 1;
        assert_ne!(expected, identity(&changed, &slot, device).unwrap());
        assert_ne!(vector, vector_identity(&changed, &slot.runtime).unwrap());
        let mut relocated = slot;
        relocated.bundle = PathBuf::from("another-installation/models");
        relocated.runtime = RuntimeSettings::EmbeddingGemma2 {
            threads: NonZeroU32::new(1).unwrap(),
            queue_capacity: NonZeroUsize::new(2).unwrap(),
        };
        assert_ne!(expected, identity(&original, &relocated, device).unwrap());
        assert_eq!(
            vector,
            vector_identity(&original, &relocated.runtime).unwrap()
        );
    }
    #[test]
    fn strict_runtime_wire_has_no_onnx_fields_for_gemma() {
        let settings: RuntimeSettings = serde_json::from_str(
            r#"{"backend":"gguf-embeddinggemma2","threads":4,"queue_capacity":8}"#,
        )
        .unwrap();
        assert!(
            matches!(settings, RuntimeSettings::EmbeddingGemma2 { threads, queue_capacity } if threads.get() == 4 && queue_capacity.get() == 8)
        );
        let rerank: RuntimeSettings = serde_json::from_str(
            r#"{"backend":"gguf-bge-reranker-v2-m3","threads":2,"queue_capacity":8}"#,
        )
        .unwrap();
        assert!(
            matches!(rerank, RuntimeSettings::BgeRerankerV2M3 { threads, queue_capacity }
            if threads.get() == 2 && queue_capacity.get() == 8)
        );
        for wire in [
            r#"{"backend":"gguf-bge-reranker-v2-m3","threads":2,"queue_capacity":8,"native_library":"ort.so"}"#,
            r#"{"backend":"gguf-bge-reranker-v2-m3","threads":2,"queue_capacity":8,"provider":"cpu"}"#,
            r#"{"backend":"gguf-bge-reranker-v2-m3","threads":0,"queue_capacity":8}"#,
            r#"{"backend":"gguf-bge-reranker-base","threads":2,"queue_capacity":8}"#,
            r#"{"threads":4,"queue_capacity":8}"#,
            r#"{"backend":"unknown","threads":4,"queue_capacity":8}"#,
            r#"{"backend":"gguf-embeddinggemma2","queue_capacity":8}"#,
            r#"{"backend":"gguf-embeddinggemma2","threads":4}"#,
            r#"{"backend":"gguf-embeddinggemma2","threads":0,"queue_capacity":8}"#,
            r#"{"backend":"gguf-embeddinggemma2","threads":4,"queue_capacity":0}"#,
            r#"{"backend":"gguf-embeddinggemma2","threads":4294967296,"queue_capacity":8}"#,
            r#"{"backend":"gguf-embeddinggemma2","threads":4,"threads":2,"queue_capacity":8}"#,
            r#"{"backend":"gguf-embeddinggemma2","threads":4,"queue_capacity":8,"provider":"cpu"}"#,
            r#"{"backend":"gguf-embeddinggemma2","threads":4,"queue_capacity":8,"native_library":"ort.so"}"#,
            r#"{"backend":"onnx","native_library":"ort.so","provider":"cpu"}"#,
            r#"{"backend":"onnx","native_library":"ort.so","provider":"cpu","threads":4}"#,
        ] {
            assert!(
                serde_json::from_str::<RuntimeSettings>(wire).is_err(),
                "{wire}"
            );
        }
    }
}
