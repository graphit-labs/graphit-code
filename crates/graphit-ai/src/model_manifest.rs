//! Closed portable model schema decoded with Serde and validated before use.
use serde::Deserialize;
use std::{fmt, io};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingStage {
    Policy,
    Request,
    Manifest,
    Decode,
    Selection,
    Artifact,
    Revalidate,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingCause {
    Io,
    Policy,
    Capacity,
    Allocation,
    Schema,
    Duplicate,
    UnknownField,
    MissingField,
    Selection,
    Path,
    Role,
    Artifact,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JsonCategory {
    Io,
    Syntax,
    Data,
    Eof,
}
#[derive(Debug)]
pub struct ModelBindingError {
    pub stage: BindingStage,
    pub cause: BindingCause,
    pub kind: io::ErrorKind,
    pub native_code: Option<i32>,
    pub json: Option<(JsonCategory, usize, usize)>,
}
impl ModelBindingError {
    pub(crate) fn new(stage: BindingStage, cause: BindingCause, kind: io::ErrorKind) -> Self {
        Self {
            stage,
            cause,
            kind,
            native_code: None,
            json: None,
        }
    }
    pub(crate) fn io(stage: BindingStage, error: io::Error) -> Self {
        Self {
            stage,
            cause: BindingCause::Io,
            kind: error.kind(),
            native_code: error.raw_os_error(),
            json: None,
        }
    }
}
impl fmt::Display for ModelBindingError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "model binding {:?}/{:?}/{:?}",
            self.stage, self.cause, self.kind
        )?;
        if let Some(code) = self.native_code {
            write!(out, " native={code}")?;
        }
        if let Some((category, line, column)) = self.json {
            write!(out, " json={category:?}@{line}:{column}")?;
        }
        Ok(())
    }
}
impl std::error::Error for ModelBindingError {}

#[derive(Clone, Copy)]
pub struct ManifestLimits {
    pub max_manifest_bytes: usize,
    pub max_artifacts: usize,
    pub max_role_bytes: usize,
    pub max_relative_path_bytes: usize,
    pub max_id_bytes: usize,
    pub max_revision_bytes: usize,
    pub max_dimension: u64,
    pub max_tokens: u64,
}
impl ManifestLimits {
    pub(crate) fn validate(&self) -> Result<(), ModelBindingError> {
        if [
            self.max_manifest_bytes,
            self.max_artifacts,
            self.max_role_bytes,
            self.max_relative_path_bytes,
            self.max_id_bytes,
            self.max_revision_bytes,
        ]
        .contains(&0)
            || [self.max_dimension, self.max_tokens].contains(&0)
        {
            return Err(ModelBindingError::new(
                BindingStage::Policy,
                BindingCause::Policy,
                io::ErrorKind::InvalidInput,
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ModelTask {
    Embedding,
    Rerank,
}
pub struct ModelSelection<'a> {
    pub id: &'a str,
    pub revision: &'a str,
    pub task: ModelTask,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ArtifactFormat {
    Gguf,
}
pub struct ModelArtifact {
    pub role: String,
    pub path: String,
    pub digest: [u8; 32],
    pub minimum_bytes: u64,
    pub required: bool,
    pub format: ArtifactFormat,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GemmaPrecision {
    Bf16,
    F32,
}
pub struct GemmaEmbeddingRecipe {
    pub dimensions: u64,
    pub max_tokens: u64,
    pub precision: GemmaPrecision,
}
impl GemmaEmbeddingRecipe {
    pub const ARCHITECTURE: &'static str = "gemma-embedding2";
    pub const QUERY_PREFIX: &'static str = "task: search result | query: ";
    pub const DOCUMENT_PREFIX: &'static str = "title: none | text: ";
    pub const CODE_QUERY_PREFIX: &'static str = "task: code retrieval | query: ";
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RerankPrecision {
    F16,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RerankScoreTransform {
    RawLogit,
    Sigmoid,
}
pub struct BgeRerankRecipe {
    pub max_tokens: u64,
    pub precision: RerankPrecision,
    pub score: RerankScoreTransform,
}
impl BgeRerankRecipe {
    pub const MODEL_ID: &'static str = "bge-reranker-v2-m3";
    pub const UPSTREAM_MODEL: &'static str = "BAAI/bge-reranker-v2-m3";
    pub const UPSTREAM_REVISION: &'static str = "953dc6f6f85a1b2dbfca4c34a2796e7dde08d41e";
    pub const ARCHITECTURE: &'static str = "bert";
    pub const MAX_TOKENS: u64 = 8192;
    pub const PAIR_TEMPLATE: &'static str = "xlm_roberta";
    pub const TRUNCATION: &'static str = "longest_first_right";
}
pub enum ModelBackend {
    BgeRerankerV2M3 {
        model_role: String,
        recipe: BgeRerankRecipe,
    },
    EmbeddingGemma2 {
        model_role: String,
        recipe: GemmaEmbeddingRecipe,
    },
}
pub struct ModelManifest {
    pub id: String,
    pub revision: String,
    pub task: ModelTask,
    pub artifacts: Vec<ModelArtifact>,
    pub backend: ModelBackend,
}
#[derive(Deserialize)]
#[serde(tag = "runtime", deny_unknown_fields)]
enum WireManifest {
    #[serde(rename = "gguf-bge-reranker-v2-m3")]
    BgeRerankerV2M3 {
        schema: String,
        id: String,
        revision: String,
        task: String,
        artifacts: Vec<WireArtifact>,
        model_role: String,
        recipe: WireBgeRecipe,
    },
    #[serde(rename = "gguf-embeddinggemma2")]
    EmbeddingGemma2 {
        schema: String,
        id: String,
        revision: String,
        task: String,
        artifacts: Vec<WireArtifact>,
        model_role: String,
        recipe: WireGemmaRecipe,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireGemmaRecipe {
    architecture: String,
    tokenizer: String,
    dimensions: u64,
    max_tokens: u64,
    precision: GemmaPrecision,
    attention: String,
    pooling: String,
    normalization: String,
    mrl: String,
    query_prefix: String,
    document_prefix: String,
    code_query_prefix: String,
    code_document_prefix: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireBgeRecipe {
    architecture: String,
    tokenizer: String,
    attention: String,
    pooling: String,
    pair_template: String,
    truncation: String,
    max_tokens: u64,
    precision: RerankPrecision,
    score: RerankScoreTransform,
    outputs: u64,
}
enum WireBackend {
    BgeRerankerV2M3 {
        model_role: String,
        recipe: WireBgeRecipe,
    },
    EmbeddingGemma2 {
        model_role: String,
        recipe: WireGemmaRecipe,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireArtifact {
    role: String,
    path: String,
    digest: Vec<u8>,
    minimum_bytes: u64,
    required: bool,
    format: String,
}
fn invalid(cause: BindingCause) -> ModelBindingError {
    ModelBindingError::new(BindingStage::Decode, cause, io::ErrorKind::InvalidData)
}
fn bounded(length: usize, maximum: usize) -> Result<(), ModelBindingError> {
    if length > maximum {
        Err(ModelBindingError::new(
            BindingStage::Decode,
            BindingCause::Capacity,
            io::ErrorKind::FileTooLarge,
        ))
    } else {
        Ok(())
    }
}
fn positive(value: u64, maximum: u64) -> Result<u64, ModelBindingError> {
    if value == 0 {
        Err(invalid(BindingCause::Schema))
    } else if value > maximum {
        Err(ModelBindingError::new(
            BindingStage::Decode,
            BindingCause::Capacity,
            io::ErrorKind::FileTooLarge,
        ))
    } else {
        Ok(value)
    }
}
pub(crate) fn exact_name(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}
fn role(value: &str) -> bool {
    exact_name(value)
        && !value.starts_with(char::is_whitespace)
        && !value.ends_with(char::is_whitespace)
}
/// Whether a value is a portable relative model path; this grants no file access.
pub fn portable_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains(['\\', ':'])
        && !value.chars().any(char::is_control)
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
fn json_error(error: serde_json::Error) -> ModelBindingError {
    // Classify mature Serde diagnostics locally; never retain or display their input text.
    let message = error.to_string();
    let cause = if message.starts_with("duplicate field") {
        BindingCause::Duplicate
    } else if message.starts_with("unknown field") {
        BindingCause::UnknownField
    } else if message.starts_with("missing field") {
        BindingCause::MissingField
    } else {
        BindingCause::Schema
    };
    let category = match error.classify() {
        serde_json::error::Category::Io => JsonCategory::Io,
        serde_json::error::Category::Syntax => JsonCategory::Syntax,
        serde_json::error::Category::Data => JsonCategory::Data,
        serde_json::error::Category::Eof => JsonCategory::Eof,
    };
    let mut safe = invalid(cause);
    safe.json = Some((category, error.line(), error.column()));
    safe
}
/// Decode strict, bounded manifest metadata while checking the caller's original request.
///
/// The caller owns source reading and the original request. This parser validates
/// metadata, not artifact contents or inference authority. Captured-source consumers
/// retain their snapshot and use `bind_captured_model`; explicit configuration consumers
/// must verify the consumed artifacts through the local runtime before inference.
pub fn decode_manifest(
    bytes: &[u8],
    limits: &ManifestLimits,
    check: &mut dyn FnMut() -> io::Result<()>,
) -> Result<ModelManifest, ModelBindingError> {
    limits.validate()?;
    bounded(bytes.len(), limits.max_manifest_bytes)?;
    check().map_err(|error| ModelBindingError::io(BindingStage::Request, error))?;
    let wire: WireManifest = serde_json::from_slice(bytes).map_err(json_error)?;
    let (schema, id, revision, task_name, raw_artifacts, backend) = match wire {
        WireManifest::BgeRerankerV2M3 {
            schema,
            id,
            revision,
            task,
            artifacts,
            model_role,
            recipe,
        } => (
            schema,
            id,
            revision,
            task,
            artifacts,
            WireBackend::BgeRerankerV2M3 { model_role, recipe },
        ),
        WireManifest::EmbeddingGemma2 {
            schema,
            id,
            revision,
            task,
            artifacts,
            model_role,
            recipe,
        } => (
            schema,
            id,
            revision,
            task,
            artifacts,
            WireBackend::EmbeddingGemma2 { model_role, recipe },
        ),
    };
    check().map_err(|error| ModelBindingError::io(BindingStage::Request, error))?;
    if schema != "graphit-model/v1" {
        return Err(invalid(BindingCause::Schema));
    }
    bounded(id.len(), limits.max_id_bytes)?;
    bounded(revision.len(), limits.max_revision_bytes)?;
    if !exact_name(&id) || !exact_name(&revision) {
        return Err(invalid(BindingCause::Selection));
    }
    let task = match task_name.as_str() {
        "embedding" => ModelTask::Embedding,
        "rerank" => ModelTask::Rerank,
        _ => return Err(invalid(BindingCause::Schema)),
    };
    bounded(raw_artifacts.len(), limits.max_artifacts)?;
    if raw_artifacts.is_empty() {
        return Err(invalid(BindingCause::Artifact));
    }
    let mut artifacts: Vec<ModelArtifact> = Vec::with_capacity(raw_artifacts.len());
    for item in raw_artifacts {
        bounded(item.role.len(), limits.max_role_bytes)?;
        bounded(item.path.len(), limits.max_relative_path_bytes)?;
        if !role(&item.role) {
            return Err(invalid(BindingCause::Role));
        }
        if !portable_path(&item.path) {
            return Err(invalid(BindingCause::Path));
        }
        if artifacts
            .iter()
            .any(|old| old.role == item.role || old.path == item.path)
        {
            return Err(invalid(BindingCause::Duplicate));
        }
        let digest = item
            .digest
            .try_into()
            .map_err(|_| invalid(BindingCause::Artifact))?;
        let format = match item.format.as_str() {
            "gguf" => ArtifactFormat::Gguf,
            _ => return Err(invalid(BindingCause::Artifact)),
        };
        artifacts.push(ModelArtifact {
            role: item.role,
            path: item.path,
            digest,
            minimum_bytes: item.minimum_bytes,
            required: item.required,
            format,
        });
    }
    let backend = match backend {
        WireBackend::BgeRerankerV2M3 { model_role, recipe } => {
            if task != ModelTask::Rerank {
                return Err(invalid(BindingCause::Schema));
            }
            bounded(model_role.len(), limits.max_role_bytes)?;
            if !role(&model_role)
                || !artifacts
                    .iter()
                    .any(|a| a.role == model_role && a.required && a.format == ArtifactFormat::Gguf)
            {
                return Err(invalid(BindingCause::Role));
            }
            if artifacts.iter().any(|a| a.format != ArtifactFormat::Gguf) {
                return Err(invalid(BindingCause::Artifact));
            }
            let max_tokens = positive(recipe.max_tokens, limits.max_tokens)?;
            if !(4..=BgeRerankRecipe::MAX_TOKENS).contains(&max_tokens)
                || recipe.architecture != BgeRerankRecipe::ARCHITECTURE
                || recipe.tokenizer != "embedded"
                || recipe.attention != "noncausal"
                || recipe.pooling != "rank"
                || recipe.pair_template != BgeRerankRecipe::PAIR_TEMPLATE
                || recipe.truncation != BgeRerankRecipe::TRUNCATION
                || recipe.outputs != 1
            {
                return Err(invalid(BindingCause::Schema));
            }
            ModelBackend::BgeRerankerV2M3 {
                model_role,
                recipe: BgeRerankRecipe {
                    max_tokens,
                    precision: recipe.precision,
                    score: recipe.score,
                },
            }
        }
        WireBackend::EmbeddingGemma2 { model_role, recipe } => {
            if task != ModelTask::Embedding {
                return Err(invalid(BindingCause::Schema));
            }
            bounded(model_role.len(), limits.max_role_bytes)?;
            if !role(&model_role)
                || !artifacts
                    .iter()
                    .any(|a| a.role == model_role && a.required && a.format == ArtifactFormat::Gguf)
            {
                return Err(invalid(BindingCause::Role));
            }
            if artifacts.iter().any(|a| a.format != ArtifactFormat::Gguf) {
                return Err(invalid(BindingCause::Artifact));
            }
            let dimensions = positive(recipe.dimensions, limits.max_dimension)?;
            let max_tokens = positive(recipe.max_tokens, limits.max_tokens)?;
            if ![128, 256, 512, 768].contains(&dimensions)
                || max_tokens > 8192
                || recipe.architecture != GemmaEmbeddingRecipe::ARCHITECTURE
                || recipe.tokenizer != "embedded"
                || recipe.attention != "noncausal"
                || recipe.pooling != "mean"
                || recipe.normalization != "f32_l2"
                || recipe.mrl != "truncate_then_f32_l2"
                || recipe.query_prefix != GemmaEmbeddingRecipe::QUERY_PREFIX
                || recipe.document_prefix != GemmaEmbeddingRecipe::DOCUMENT_PREFIX
                || recipe.code_query_prefix != GemmaEmbeddingRecipe::CODE_QUERY_PREFIX
                || recipe.code_document_prefix != GemmaEmbeddingRecipe::DOCUMENT_PREFIX
            {
                return Err(invalid(BindingCause::Schema));
            }
            ModelBackend::EmbeddingGemma2 {
                model_role,
                recipe: GemmaEmbeddingRecipe {
                    dimensions,
                    max_tokens,
                    precision: recipe.precision,
                },
            }
        }
    };
    check().map_err(|error| ModelBindingError::io(BindingStage::Request, error))?;
    Ok(ModelManifest {
        id,
        revision,
        task,
        artifacts,
        backend,
    })
}
