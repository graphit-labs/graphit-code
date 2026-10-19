//! Local model CLI and selection producer; inferencing Auth is owned by the daemon.
use crate::model_provision::{
    InstallSource, MODEL_BYTES, MODEL_FILE, MODEL_ID, MODEL_REVISION, MODEL_SHA256,
    ModelProvisioner, PinnedModel, ProvisionError, ProvisionLimits, RERANK_MODEL_BYTES,
    RERANK_MODEL_FILE, RERANK_MODEL_ID, RERANK_MODEL_REVISION, RERANK_MODEL_SHA256, remaining,
};
use clap::{Args, Subcommand};
use graphit_ai::model_manifest::{
    BgeRerankRecipe, GemmaEmbeddingRecipe, ModelBackend, ModelTask, decode_manifest,
};
use graphit_core::{
    artifact_store::SnapshotContext,
    config::{ConfigKey, ConfigMutation},
    project_paths::InstallationPaths,
};
use graphit_daemon::model_service::{
    CapturedModelInput, LocalExecutionSelection, ModelSettings, ResolvedConfig, RuntimeSettings,
    SelectionSettings, SlotSettings, TaskKind,
};
use serde_json::json;
use std::{
    io::{self, Read, Write},
    num::{NonZeroU32, NonZeroUsize},
    path::Path,
    process::ExitCode,
    sync::Arc,
};

pub const DEFAULT_SLOT: &str = "embedding";
pub const DEFAULT_RERANK_SLOT: &str = "rerank";
/// Model subcommands parsed by the shared CLI before installation setup.
#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: ModelCommand,
}
/// Typed installation or selection operation for the existing model producer.
#[derive(Subcommand)]
pub enum ModelCommand {
    /// Install a pinned GGUF model, preserving its existing task selection.
    Install {
        #[arg(value_parser=[MODEL_ID, RERANK_MODEL_ID])]
        model: String,
        /// Install a previously prepared, digest-verified GGUF instead of downloading it.
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
    /// Select a captured installed embedding or rerank manifest.
    Select {
        manifest_relative: std::path::PathBuf,
    },
}

/// Paths and original cancellation/deadline are captured by the shared CLI owner.
pub fn run(
    arguments: Arguments,
    paths: &InstallationPaths,
    original: Arc<SnapshotContext>,
) -> ExitCode {
    let installing = matches!(&arguments.command, ModelCommand::Install { .. });
    let result = (|| -> Result<bool, ProvisionError<io::Error>> {
        original.check()?;
        let producer =
            ModelProvisioner::capture(paths, original.clone(), ProvisionLimits::product()?)?;
        let mut gate = || original.check();
        match arguments.command {
            ModelCommand::Install { model, file } => {
                let model = match model.as_str() {
                    MODEL_ID => PinnedModel::EmbeddingGemma2,
                    RERANK_MODEL_ID => PinnedModel::BgeRerankerV2M3,
                    _ => return Err(ProvisionError::Configuration),
                };
                let captured = file
                    .as_ref()
                    .map(|path| {
                        let path = std::path::absolute(path)?;
                        graphit_core::artifact_store::CapturedSourceReader::capture(
                            &path,
                            graphit_fs::NodeKind::File,
                            &producer.limits().snapshot,
                            producer.limits().read,
                            original.clone(),
                        )
                    })
                    .transpose()?;
                let relative = captured
                    .as_ref()
                    .map(|reader| {
                        reader
                            .nodes()
                            .next()
                            .ok_or(ProvisionError::Configuration)
                            .map(|node| node.relative().to_path_buf())
                    })
                    .transpose()?;
                let source = match (&captured, &relative) {
                    (Some(reader), Some(relative)) => InstallSource::Captured { reader, relative },
                    (None, None) => InstallSource::Remote,
                    _ => return Err(ProvisionError::Configuration),
                };
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                Ok(runtime
                    .block_on(producer.install(model, source, &mut gate))?
                    .selected)
            }
            ModelCommand::Select { manifest_relative } => {
                producer.select(&manifest_relative, None, &mut gate)
            }
        }
    })();
    match result {
        Ok(selected) => {
            if writeln!(
                io::stdout(),
                "model {}",
                if !installing || selected {
                    "selected"
                } else {
                    "provisioned; selection preserved"
                }
            )
            .is_ok()
            {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            let _ = writeln!(io::stderr(), "models: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Explicit product defaults, generated once by App rather than rediscovered in Runtime.
pub fn default_runtime() -> io::Result<RuntimeSettings> {
    let (threads, queue_capacity) = default_runtime_limits()?;
    Ok(RuntimeSettings::EmbeddingGemma2 {
        threads,
        queue_capacity,
    })
}
pub fn default_rerank_runtime() -> io::Result<RuntimeSettings> {
    let (threads, queue_capacity) = default_runtime_limits()?;
    Ok(RuntimeSettings::BgeRerankerV2M3 {
        threads,
        queue_capacity,
    })
}
fn default_runtime_limits() -> io::Result<(NonZeroU32, NonZeroUsize)> {
    let threads = u32::try_from(std::thread::available_parallelism()?.get().div_ceil(2))
        .map_err(|_| io::ErrorKind::InvalidInput)?;
    Ok((
        NonZeroU32::new(threads).ok_or(io::ErrorKind::InvalidInput)?,
        NonZeroUsize::new(8).ok_or(io::ErrorKind::InvalidInput)?,
    ))
}
pub fn pinned_manifest(dimensions: u64) -> io::Result<Vec<u8>> {
    if ![128, 256, 512, 768].contains(&dimensions) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut manifest = json!({
        "schema":"graphit-model/v1","runtime":"gguf-embeddinggemma2","id":MODEL_ID,"revision":MODEL_REVISION,"task":"embedding",
        "artifacts":[{"role":"model","path":MODEL_FILE,"digest":MODEL_SHA256,"minimum_bytes":MODEL_BYTES,"required":true,"format":"gguf"}],"model_role":"model",
        "recipe":{"architecture":GemmaEmbeddingRecipe::ARCHITECTURE,"tokenizer":"embedded","dimensions":dimensions,"max_tokens":8192,"precision":"bf16","attention":"noncausal","pooling":"mean","normalization":"f32_l2","mrl":"truncate_then_f32_l2","query_prefix":GemmaEmbeddingRecipe::QUERY_PREFIX,"document_prefix":GemmaEmbeddingRecipe::DOCUMENT_PREFIX,"code_query_prefix":GemmaEmbeddingRecipe::CODE_QUERY_PREFIX,"code_document_prefix":GemmaEmbeddingRecipe::DOCUMENT_PREFIX}
    });
    manifest.sort_all_objects();
    serde_json::to_vec(&manifest).map_err(|_| io::ErrorKind::InvalidData.into())
}

/// The default reranker returns the classifier's single raw logit, without sigmoid.
pub fn pinned_rerank_manifest() -> io::Result<Vec<u8>> {
    let mut manifest = json!({
        "schema":"graphit-model/v1", "runtime":"gguf-bge-reranker-v2-m3",
        "id":RERANK_MODEL_ID, "revision":RERANK_MODEL_REVISION, "task":"rerank",
        "artifacts":[{"role":"model", "path":RERANK_MODEL_FILE, "digest":RERANK_MODEL_SHA256,
            "minimum_bytes":RERANK_MODEL_BYTES, "required":true, "format":"gguf"}],
        "model_role":"model",
        "recipe":{"architecture":BgeRerankRecipe::ARCHITECTURE, "tokenizer":"embedded",
            "attention":"noncausal", "pooling":"rank", "pair_template":BgeRerankRecipe::PAIR_TEMPLATE,
            "truncation":BgeRerankRecipe::TRUNCATION, "max_tokens":BgeRerankRecipe::MAX_TOKENS,
            "precision":"f16", "score":"raw_logit", "outputs":1}
    });
    manifest.sort_all_objects();
    serde_json::to_vec(&manifest).map_err(|_| io::ErrorKind::InvalidData.into())
}

pub(crate) fn select_bundle<E>(
    producer: &ModelProvisioner,
    bundle: &Path,
    relative: &Path,
    explicit: bool,
    runtime: Option<RuntimeSettings>,
    gate: &mut impl FnMut() -> Result<(), E>,
) -> Result<bool, ProvisionError<E>> {
    producer.check(gate)?;
    let key = ConfigKey::new("daemon.models")?;
    let mut writer = producer.config().writer(
        remaining(producer.original())?,
        producer.original().cancellation(),
    )?;
    producer.check(gate)?;
    let document = writer.load()?;
    let current: Option<ModelSettings> = document.get(&key)?;
    producer.check(gate)?;
    // Read only the small manifest before capturing the model bundle. The task
    // determines its canonical slot, without hashing the model twice.
    let manifest_reader = graphit_core::artifact_store::CapturedSourceReader::capture(
        &bundle.join(relative),
        graphit_fs::NodeKind::File,
        &producer.limits().snapshot,
        producer.limits().read,
        producer.original().clone(),
    )?;
    let manifest_relative = manifest_reader
        .nodes()
        .next()
        .ok_or(ProvisionError::Configuration)?
        .relative()
        .to_path_buf();
    let file = manifest_reader.open_file(&manifest_relative)?;
    let length =
        usize::try_from(file.len()).map_err(|_| io::Error::from(io::ErrorKind::FileTooLarge))?;
    if length == 0 || length > producer.limits().manifest.max_manifest_bytes {
        return Err(ProvisionError::Configuration);
    }
    let mut body = Vec::new();
    body.try_reserve_exact(length)
        .map_err(|_| io::Error::from(io::ErrorKind::OutOfMemory))?;
    body.resize(length, 0);
    file.stream(0)?.read_exact(&mut body)?;
    producer.check(gate)?;
    let manifest = decode_manifest(&body, &producer.limits().manifest, &mut || {
        producer.original().check()
    })?;
    producer.check(gate)?;
    let (slot_name, task) = match (&manifest.backend, manifest.task) {
        (ModelBackend::EmbeddingGemma2 { .. }, ModelTask::Embedding) => {
            (DEFAULT_SLOT, TaskKind::Embedding)
        }
        (ModelBackend::BgeRerankerV2M3 { .. }, ModelTask::Rerank) => {
            (DEFAULT_RERANK_SLOT, TaskKind::Rerank)
        }
        _ => return Err(ProvisionError::Configuration),
    };
    if !explicit
        && current.as_ref().is_some_and(|settings| {
            settings.slots.is_empty()
                || settings
                    .slots
                    .iter()
                    .any(|slot| slot.selection.task == task)
        })
    {
        return Ok(false);
    }
    let captured = CapturedModelInput::capture(
        slot_name,
        bundle,
        &producer.limits().snapshot,
        producer.limits().read,
        producer.original().clone(),
    )
    .map_err(ProvisionError::Model)?;
    manifest_reader.revalidate_content()?;
    let mut settings = current.unwrap_or(ModelSettings { slots: Vec::new() });
    let index = settings
        .slots
        .iter()
        .position(|slot| slot.name == slot_name);
    if index.is_some_and(|index| settings.slots[index].selection.task != task) {
        return Err(ProvisionError::Configuration);
    }
    let old = index.map(|index| settings.slots.remove(index));
    let runtime = match runtime {
        Some(runtime) => runtime,
        None => match (task, old.map(|slot| slot.runtime)) {
            (TaskKind::Embedding, Some(runtime @ RuntimeSettings::EmbeddingGemma2 { .. })) => {
                runtime
            }
            (TaskKind::Rerank, Some(runtime @ RuntimeSettings::BgeRerankerV2M3 { .. })) => runtime,
            (TaskKind::Embedding, _) => default_runtime()?,
            (TaskKind::Rerank, _) => default_rerank_runtime()?,
        },
    };
    let slot = SlotSettings {
        name: slot_name.into(),
        selection: SelectionSettings {
            id: manifest.id,
            revision: manifest.revision,
            task,
        },
        bundle: bundle.to_path_buf(),
        manifest_relative: relative.to_path_buf(),
        runtime,
    };
    let value = serde_json::to_value(&slot).map_err(|_| ProvisionError::Configuration)?;
    let validation =
        serde_json::from_value(value.clone()).map_err(|_| ProvisionError::Configuration)?;
    // Installation validates the explicitly selected slot's static binding. This
    // selector neither changes an Auth profile nor enables search reranking.
    let cpu = graphit_auth::profile::AiService::Local(graphit_auth::profile::LocalExecution {
        device: graphit_auth::profile::LocalDevice::Cpu,
        device_id: 0,
    });
    let disabled = graphit_auth::profile::AiService::Disabled;
    let selection = match task {
        TaskKind::Embedding => LocalExecutionSelection::from_services(&cpu, &disabled, false),
        TaskKind::Rerank => LocalExecutionSelection::from_services(&disabled, &cpu, true),
    }
    .map_err(ProvisionError::Model)?;
    ResolvedConfig::resolve(
        ModelSettings {
            slots: vec![validation],
        },
        selection,
        [captured],
        producer.original(),
        &producer.limits().manifest,
    )
    .map_err(ProvisionError::Model)?;
    producer.check(gate)?;
    let slot = serde_json::from_value(value).map_err(|_| ProvisionError::Configuration)?;
    if let Some(index) = index {
        settings.slots.insert(index, slot);
    } else {
        settings.slots.push(slot);
    }
    let value = serde_json::to_value(settings).map_err(|_| ProvisionError::Configuration)?;
    producer.check(gate)?;
    let changed = writer.apply([ConfigMutation::Set { key, value }])?;
    producer.check(gate)?;
    Ok(changed)
}
