//! Portable identity of a configured remote embedding space.
//!
//! A declared deployment/model revision and query/document recipe identify the
//! vectors. Credentials, endpoint location, devices and service rollout revisions
//! do not. An incompatible change therefore follows ordinary reindexing.
use crate::embedding_service::{EmbeddingMetadata, EmbeddingServiceError};
use graphit_auth::{broker::BrokerEmbeddingService, profile::DirectProtocol};
use sha2::{Digest, Sha256};

pub(crate) fn broker(
    service: &BrokerEmbeddingService,
) -> Result<EmbeddingMetadata, EmbeddingServiceError> {
    remote(
        "graphit-embeddings-v1",
        &service.identity.model,
        &service.identity.model_revision,
        &service.identity.recipe,
        service.dimensions,
    )
}

pub(crate) fn direct(
    protocol: DirectProtocol,
    model: &str,
    model_revision: &str,
    recipe: &str,
    dimensions: u32,
) -> Result<EmbeddingMetadata, EmbeddingServiceError> {
    let protocol = match protocol {
        DirectProtocol::OpenAi => "openai",
        DirectProtocol::OpenAiCompatible => "openai-compatible",
        DirectProtocol::Cohere => "cohere",
        DirectProtocol::CohereV2 => "cohere-v2",
        DirectProtocol::Voyage => "voyage",
        DirectProtocol::VoyageV1 => "voyage-v1",
        DirectProtocol::GoogleGemini1 => "google-gemini-1",
        DirectProtocol::GoogleGemini2 => "google-gemini-2",
        DirectProtocol::Jina => "jina",
        DirectProtocol::JinaV1 => "jina-v1",
    };
    remote(protocol, model, model_revision, recipe, dimensions)
}

fn remote(
    protocol: &str,
    model: &str,
    model_revision: &str,
    recipe: &str,
    dimensions: u32,
) -> Result<EmbeddingMetadata, EmbeddingServiceError> {
    for value in [model, model_revision, recipe] {
        if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
            return Err(EmbeddingServiceError::Metadata);
        }
    }
    let mut hash = Sha256::new();
    hash.update(b"graphit.embedding.vector-space.v1\0");
    for value in [protocol, model, model_revision, recipe] {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    hash.update(dimensions.to_le_bytes());
    EmbeddingMetadata::remote(
        hash.finalize().into(),
        u64::from(dimensions),
        model.to_owned(),
        model_revision.to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_auth::broker::BrokerModelIdentity;

    fn service() -> BrokerEmbeddingService {
        BrokerEmbeddingService {
            endpoint: "https://broker.invalid/v1/embeddings".into(),
            identity: BrokerModelIdentity {
                model: "embedding-model".into(),
                model_revision: "weights-r2".into(),
                recipe: "retrieval-r1".into(),
                service_revision: "service-r1".into(),
            },
            dimensions: 768,
            max_batch: 16,
            max_input_bytes: 8192,
        }
    }

    #[test]
    fn incompatible_remote_spaces_never_share_an_identity() {
        let original = service();
        let expected = broker(&original).unwrap().vector_id;
        for change in 0..4 {
            let mut changed = service();
            match change {
                0 => changed.identity.model.push_str("-other"),
                1 => changed.identity.model_revision.push_str("-other"),
                2 => changed.identity.recipe.push_str("-other"),
                _ => changed.dimensions += 1,
            }
            assert_ne!(broker(&changed).unwrap().vector_id, expected);
        }
    }

    #[test]
    fn service_rollout_location_and_batching_do_not_change_the_vector_space() {
        let original = broker(&service()).unwrap();
        let mut moved = service();
        moved.endpoint = "https://another.invalid/v1/embeddings".into();
        moved.identity.service_revision = "service-r2".into();
        moved.max_batch = 2;
        moved.max_input_bytes = 4096;
        assert!(broker(&moved).unwrap() == original);
        assert!(original.local_generation.is_none());
    }

    #[test]
    fn direct_requires_a_real_declared_revision_recipe_and_positive_dimensions() {
        assert!(direct(DirectProtocol::OpenAi, "model", "", "recipe", 768).is_err());
        assert!(direct(DirectProtocol::OpenAi, "model", "r1", "", 768).is_err());
        assert!(direct(DirectProtocol::OpenAi, "model", "r1", "recipe", 0).is_err());
    }
}
