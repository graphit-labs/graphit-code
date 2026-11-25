//! Discovered AI configuration is data, never authentication or PROJECT authority.
use super::BrokerError;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrokerModelIdentity {
    pub model: String,
    pub model_revision: String,
    pub recipe: String,
    pub service_revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrokerEmbeddingService {
    pub endpoint: String,
    pub identity: BrokerModelIdentity,
    pub dimensions: u32,
    pub max_batch: usize,
    pub max_input_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrokerRerankService {
    pub endpoint: String,
    pub identity: BrokerModelIdentity,
    pub max_documents: usize,
}

#[derive(Default)]
pub(super) struct BrokerServices {
    embedding: Option<Value>,
    rerank: Option<Value>,
    s3_credentials: Option<Value>,
}

impl BrokerServices {
    // The caller already bounded the discovery JSON. Retain observations without
    // resolving an unused AI service during login or an unrelated search.
    pub(super) fn read(document: &Value) -> Result<Self, BrokerError> {
        let Some(services) = document.get("services") else {
            return Ok(Self::default());
        };
        let services = services.as_object().ok_or(BrokerError::InvalidMetadata)?;
        Ok(Self {
            embedding: services.get("embeddings").cloned(),
            rerank: services.get("rerank").cloned(),
            s3_credentials: services.get("s3_credentials").cloned(),
        })
    }

    pub(super) fn s3_enabled(
        &self,
        mut endpoint: impl FnMut(&str) -> Result<String, BrokerError>,
    ) -> Result<bool, BrokerError> {
        let Some(value) = &self.s3_credentials else {
            return Ok(false);
        };
        if text(value, "protocol")? != "graphit-s3-credentials-v3"
            || path(value)? != "/v1/s3/credentials"
        {
            return Err(BrokerError::InvalidMetadata);
        }
        endpoint(path(value)?)?;
        Ok(true)
    }

    // Each getter parses only the explicitly selected service. Its caller uses
    // the existing trusted-origin URL validator; descriptors are not authority.
    pub(super) fn embedding(
        &self,
        mut endpoint: impl FnMut(&str) -> Result<String, BrokerError>,
    ) -> Result<Option<BrokerEmbeddingService>, BrokerError> {
        self.embedding
            .as_ref()
            .map(|value| {
                if text(value, "protocol")? != "graphit-embeddings-v1" {
                    return Err(BrokerError::InvalidMetadata);
                }
                Ok(BrokerEmbeddingService {
                    endpoint: endpoint(path(value)?)?,
                    identity: identity(value)?,
                    dimensions: u32::try_from(positive(value, "dimensions")?)
                        .map_err(|_| BrokerError::InvalidMetadata)?,
                    max_batch: positive(value, "max_batch")?,
                    max_input_bytes: positive(value, "max_input_bytes")?,
                })
            })
            .transpose()
    }

    pub(super) fn rerank(
        &self,
        mut endpoint: impl FnMut(&str) -> Result<String, BrokerError>,
    ) -> Result<Option<BrokerRerankService>, BrokerError> {
        self.rerank
            .as_ref()
            .map(|value| {
                if text(value, "protocol")? != "graphit-rerank-v1" {
                    return Err(BrokerError::InvalidMetadata);
                }
                Ok(BrokerRerankService {
                    endpoint: endpoint(path(value)?)?,
                    identity: identity(value)?,
                    max_documents: positive(value, "max_documents")?,
                })
            })
            .transpose()
    }
}

fn identity(value: &Value) -> Result<BrokerModelIdentity, BrokerError> {
    Ok(BrokerModelIdentity {
        model: text(value, "model")?.into(),
        model_revision: text(value, "model_revision")?.into(),
        recipe: text(value, "recipe")?.into(),
        service_revision: text(value, "revision")?.into(),
    })
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, BrokerError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.trim() == *s && !s.chars().any(char::is_control))
        .ok_or(BrokerError::InvalidMetadata)
}

fn path(value: &Value) -> Result<&str, BrokerError> {
    let path = text(value, "path")?;
    if !path.starts_with('/') || path.starts_with("//") || path.contains(['?', '#', '\\']) {
        return Err(BrokerError::InvalidMetadata);
    }
    Ok(path)
}

fn positive(value: &Value, key: &str) -> Result<usize, BrokerError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .filter(|n| *n != 0)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or(BrokerError::InvalidMetadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn document() -> Value {
        json!({"services": {
            "embeddings": {"protocol":"graphit-embeddings-v1", "path":"/v1/embeddings",
                "revision":"service-r7", "model":"embedding-model", "model_revision":"weights-r2",
                "recipe":"retrieval-r3", "dimensions":768, "max_batch":16, "max_input_bytes":8192},
            "rerank": {"protocol":"graphit-rerank-v1", "path":"/v1/rerank",
                "revision":"service-r8", "model":"reranker", "model_revision":"weights-r4",
                "recipe":"cross-encoder-r1", "max_documents":32}
        }})
    }

    fn parse(document: &Value) -> Result<Option<BrokerEmbeddingService>, BrokerError> {
        BrokerServices::read(document)?.embedding(endpoint)
    }

    fn endpoint(path: &str) -> Result<String, BrokerError> {
        Ok(format!("https://broker.invalid{path}"))
    }

    #[test]
    fn s3_uses_discovered_stsv3_and_distinguishes_disabled_from_invalid() {
        let mut document = document();
        assert!(
            !BrokerServices::read(&document)
                .unwrap()
                .s3_enabled(endpoint)
                .unwrap()
        );
        document["services"]["s3_credentials"] = json!({"protocol":"graphit-s3-credentials-v3","path":"/v1/s3/credentials","authorization_revision":"7"});
        assert!(
            BrokerServices::read(&document)
                .unwrap()
                .s3_enabled(endpoint)
                .unwrap()
        );
        for invalid in [
            Value::Null,
            json!({}),
            json!({"protocol":"other","path":"/v1/s3/credentials"}),
            json!({"protocol":"graphit-s3-credentials-v3","path":"/foreign"}),
        ] {
            document["services"]["s3_credentials"] = invalid;
            assert!(
                BrokerServices::read(&document)
                    .unwrap()
                    .s3_enabled(endpoint)
                    .is_err()
            );
        }
    }

    #[test]
    fn retains_distinct_service_model_and_recipe_revisions() {
        let parsed = BrokerServices::read(&document()).unwrap();
        let embedding = parsed.embedding(endpoint).unwrap().unwrap();
        assert_eq!(embedding.endpoint, "https://broker.invalid/v1/embeddings");
        assert_eq!(embedding.identity.service_revision, "service-r7");
        assert_eq!(embedding.identity.model_revision, "weights-r2");
        assert_eq!(embedding.identity.recipe, "retrieval-r3");
        assert_eq!(embedding.dimensions, 768);
        assert_eq!(parsed.rerank(endpoint).unwrap().unwrap().max_documents, 32);
    }

    #[test]
    fn absent_services_are_not_an_inferred_local_service() {
        let services = BrokerServices::read(&json!({})).unwrap();
        assert!(services.embedding(endpoint).unwrap().is_none());
        assert!(services.rerank(endpoint).unwrap().is_none());
        assert!(parse(&json!({"services":null})).is_err());
    }

    #[test]
    fn rejects_incomplete_identity_protocols_and_nonpositive_capabilities() {
        for field in ["model", "model_revision", "recipe", "revision"] {
            let mut value = document();
            value["services"]["embeddings"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(parse(&value).is_err(), "missing {field}");
        }
        for field in ["dimensions", "max_batch", "max_input_bytes"] {
            let mut value = document();
            value["services"]["embeddings"][field] = json!(0);
            assert!(parse(&value).is_err(), "zero {field}");
        }
        let mut value = document();
        value["services"]["embeddings"]["protocol"] = json!("openai-embeddings-v1");
        assert!(parse(&value).is_err());
    }

    #[test]
    fn rejects_absolute_or_ambiguous_service_destinations_and_retains_validator_errors() {
        for path in [
            "https://other.invalid/v1/embeddings",
            "//other.invalid/path",
            "/path?q=x",
            "/path#x",
            "/\\host/path",
        ] {
            let mut value = document();
            value["services"]["embeddings"]["path"] = json!(path);
            assert!(parse(&value).is_err(), "{path}");
        }
        assert!(matches!(
            BrokerServices::read(&document())
                .unwrap()
                .embedding(|_| Err(BrokerError::Cancelled)),
            Err(BrokerError::Cancelled)
        ));
    }

    #[test]
    fn an_unused_reranker_cannot_block_embedding_or_resolve_its_endpoint() {
        let mut document = document();
        document["services"]["rerank"] = json!({"protocol":"invalid-unused-reranker"});
        let services = BrokerServices::read(&document).unwrap();
        let mut endpoints = Vec::new();
        let embedding = services
            .embedding(|path| {
                endpoints.push(path.to_owned());
                endpoint(path)
            })
            .unwrap()
            .unwrap();
        assert_eq!(embedding.dimensions, 768);
        assert_eq!(endpoints, ["/v1/embeddings"]);
        assert!(services.rerank(endpoint).is_err());
    }
}
