//! Retract catalog references without changing payloads, sources or installations.
use crate::native_publication::{self, Catalog, CatalogIdentity};
use crate::native_storage_credentials::BrokerStorage;
use graphit_core::project_identity::ProjectId;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion,
    policy::{HubResourcePolicy, RequestContext},
    remote_metadata::{
        AuthorInput, DependencyInput, ManifestInput, MetadataLimits, RemoteArtifactManifest,
        VersionInput,
    },
};
use object_store::{
    aws::{AwsAuthorizer, AwsCredential},
    client::{HttpRequest, HttpRequestBody},
    path::Path as ObjectPath,
};
use serde::{Deserialize, Serialize};
use std::{io, time::Instant};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Arguments {
    pub(crate) provider: String,
    pub(crate) producer: ProjectId,
    pub(crate) artifact_type: ArtifactType,
    pub(crate) id: String,
    pub(crate) target: Target,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Target {
    Version {
        version: String,
        #[serde(
            default,
            deserialize_with = "present_digest",
            skip_serializing_if = "Option::is_none"
        )]
        expected_sha256: Option<String>,
    },
    Artifact {},
}

fn present_digest<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

impl Arguments {
    pub(crate) fn validate(&self, policy: &HubResourcePolicy) -> io::Result<ArtifactId> {
        if self.provider.is_empty()
            || self.provider.trim() != self.provider
            || self.provider.chars().any(char::is_control)
            || self.provider.len() > policy.content().max_scalar_bytes
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let id = ArtifactId::new(&self.id, policy.projection())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        if let Target::Version {
            version,
            expected_sha256,
        } = &self.target
        {
            if version.eq_ignore_ascii_case("latest") {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            ConcreteVersion::new(version, policy.projection())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
            if expected_sha256.as_ref().is_some_and(|hash| {
                hash.len() != 64
                    || !hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        Ok(id)
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct Receipt {
    pub(crate) producer: ProjectId,
    pub(crate) artifact_type: ArtifactType,
    pub(crate) id: String,
    pub(crate) targeted_versions: Vec<String>,
    pub(crate) desired_latest: Option<String>,
    pub(crate) catalog: &'static str,
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) receipt: Receipt,
    pub(crate) stage: &'static str,
    pub(crate) cause: io::Error,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Hub retraction failed at {} (catalog {})",
            self.stage, self.receipt.catalog
        )
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Caller retains the actual HubWrite lease and current Broker authorization.
pub(crate) struct Input<'a> {
    pub(crate) arguments: &'a Arguments,
    pub(crate) storage: &'a BrokerStorage,
    pub(crate) client: &'a reqwest::Client,
    pub(crate) policy: &'a HubResourcePolicy,
}

#[derive(Debug)]
struct RemainingCatalog {
    targeted_versions: Vec<String>,
    desired_latest: Option<String>,
    encoded: Option<Vec<u8>>,
}

fn remaining_catalog(
    current: &RemoteArtifactManifest,
    target: &Target,
    limits: MetadataLimits,
    original: &RequestContext<'_>,
) -> io::Result<RemainingCatalog> {
    let Target::Version {
        version,
        expected_sha256,
    } = target
    else {
        let targeted = current
            .versions()
            .iter()
            .map(|item| item.version().as_str().to_owned())
            .collect();
        return Ok(RemainingCatalog {
            targeted_versions: targeted,
            desired_latest: None,
            encoded: None,
        });
    };
    let found = current
        .versions()
        .iter()
        .find(|item| item.version().as_str() == version)
        .ok_or(io::ErrorKind::NotFound)?;
    if expected_sha256
        .as_ref()
        .is_some_and(|hash| *hash != crate::file_publication::hex(found.hash()))
    {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    let targeted = vec![version.clone()];
    let retained: Vec<_> = current
        .versions()
        .iter()
        .filter(|item| item.version().as_str() != version)
        .collect();
    let latest = current
        .latest()
        .map(ConcreteVersion::as_str)
        .filter(|latest| retained.iter().any(|v| v.version().as_str() == *latest))
        .map(str::to_owned);
    if retained.is_empty() {
        return Ok(RemainingCatalog {
            targeted_versions: targeted,
            desired_latest: latest,
            encoded: None,
        });
    }
    let hashes: Vec<_> = retained
        .iter()
        .map(|v| crate::file_publication::hex(v.hash()))
        .collect();
    let versions: Vec<_> = retained
        .iter()
        .zip(&hashes)
        .map(|(version, hash)| VersionInput {
            version: version.version().as_str(),
            hash,
        })
        .collect();
    let tags: Vec<_> = current.tags().iter().map(String::as_str).collect();
    let dependencies: Vec<_> = current
        .dependencies()
        .iter()
        .map(|d| DependencyInput {
            artifact_type: d.artifact_type(),
            id: d.id().as_str(),
            selector: d.selector(),
        })
        .collect();
    let encoded = RemoteArtifactManifest::encode(
        &ManifestInput {
            producer: current.producer(),
            artifact_type: current.artifact_type(),
            id: current.id().as_str(),
            name: current.name(),
            description: current.description(),
            tags: &tags,
            author: current.author().map(|username| AuthorInput { username }),
            latest: latest.as_deref(),
            versions: &versions,
            dependencies: &dependencies,
        },
        limits,
        original,
    )
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(RemainingCatalog {
        targeted_versions: targeted,
        desired_latest: latest,
        encoded: Some(encoded),
    })
}

// Match the SDK's path-style topology: the STS client never enables virtual-hosted requests.
fn delete_url(
    endpoint: Option<&str>,
    bucket: &str,
    region: &str,
    path: &ObjectPath,
) -> io::Result<url::Url> {
    const PATH_ENCODE: percent_encoding::AsciiSet = percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~')
        .remove(b'/');
    let endpoint = endpoint
        .map(str::to_owned)
        .unwrap_or_else(|| format!("https://s3.{region}.amazonaws.com"));
    let encoded = percent_encoding::utf8_percent_encode(path.as_ref(), &PATH_ENCODE);
    let url = url::Url::parse(&format!(
        "{}/{bucket}/{encoded}",
        endpoint.trim_end_matches('/')
    ))
    .map_err(|_| io::ErrorKind::InvalidInput)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(url)
}

fn delete_request(
    url: url::Url,
    etag: &str,
    credential: &AwsCredential,
    region: &str,
) -> io::Result<reqwest::Request> {
    let mut signed = HttpRequest::new(HttpRequestBody::empty());
    *signed.method_mut() = reqwest::Method::DELETE;
    *signed.uri_mut() = url
        .as_str()
        .parse()
        .map_err(|_| io::ErrorKind::InvalidInput)?;
    signed.headers_mut().insert(
        reqwest::header::IF_MATCH,
        reqwest::header::HeaderValue::from_str(etag).map_err(|_| io::ErrorKind::InvalidInput)?,
    );
    AwsAuthorizer::new(credential, "s3", region)
        .try_authorize(&mut signed, None)
        .map_err(io::Error::other)?;
    let mut request = reqwest::Request::new(reqwest::Method::DELETE, url);
    *request.headers_mut() = signed.into_parts().0.headers;
    Ok(request)
}

async fn delete_catalog(
    input: &Input<'_>,
    catalog: &Catalog,
    effect: &mut &'static str,
    original: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<()> {
    check()?;
    if !input.storage.supports_conditional_delete() {
        return Err(io::ErrorKind::Unsupported.into());
    }
    let (path, etag) = catalog.conditional_delete_target()?;
    let deadline = original
        .snapshot()
        .required_deadline()?
        .min(input.storage.expires_at());
    if deadline <= Instant::now() {
        return Err(io::ErrorKind::TimedOut.into());
    }
    let url = delete_url(
        input.storage.endpoint.as_deref(),
        &input.storage.bucket,
        &input.storage.region,
        path,
    )?;
    let credential = native_publication::guarded(
        original,
        check,
        input.storage.store.credentials().get_credential(),
    )
    .await?;
    let request = delete_request(url, etag, &credential, &input.storage.region)?;
    check()?;
    *effect = "uncertain";
    let response = tokio::select! {
        biased;
        () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
        result = input.client.execute(request) => result.map_err(|error| io::Error::other(error.without_url()))?,
    };
    use reqwest::StatusCode;
    let status = response.status();
    if status == StatusCode::NO_CONTENT {
        *effect = "removed";
        return Ok(());
    }
    let kind = match status {
        StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT => io::ErrorKind::AlreadyExists,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => io::ErrorKind::PermissionDenied,
        StatusCode::NOT_FOUND => io::ErrorKind::NotFound,
        StatusCode::BAD_REQUEST => io::ErrorKind::InvalidInput,
        StatusCode::NOT_IMPLEMENTED | StatusCode::METHOD_NOT_ALLOWED => io::ErrorKind::Unsupported,
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => io::ErrorKind::TimedOut,
        _ => io::ErrorKind::Other,
    };
    if matches!(
        kind,
        io::ErrorKind::AlreadyExists
            | io::ErrorKind::PermissionDenied
            | io::ErrorKind::NotFound
            | io::ErrorKind::InvalidInput
            | io::ErrorKind::Unsupported
    ) {
        *effect = "rejected";
    }
    Err(io::Error::new(
        kind,
        format!("catalog DELETE returned HTTP {}", status.as_u16()),
    ))
}

pub(crate) async fn execute(
    input: Input<'_>,
    original: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<Receipt> {
    let id = input.arguments.validate(input.policy)?;
    check()?;
    let limits =
        native_publication::metadata_limits(input.policy.content(), input.policy.projection());
    let mut catalog = native_publication::read_catalog(
        input.storage.store.as_ref(),
        CatalogIdentity {
            storage_prefix: &input.storage.prefix,
            producer: input.arguments.producer,
            artifact_type: input.arguments.artifact_type,
            id: &id,
            limits,
        },
        original,
        check,
    )
    .await?;
    let current = catalog.current.take().ok_or(io::ErrorKind::NotFound)?;
    let RemainingCatalog {
        targeted_versions,
        desired_latest,
        encoded,
    } = remaining_catalog(&current, &input.arguments.target, limits, original)?;
    let mut receipt = Receipt {
        producer: input.arguments.producer,
        artifact_type: input.arguments.artifact_type,
        id: id.as_str().to_owned(),
        targeted_versions,
        desired_latest,
        catalog: "not_attempted",
    };
    let (stage, result) = match encoded {
        Some(encoded) => (
            "catalog_update",
            native_publication::write_catalog(
                input.storage.store.as_ref(),
                catalog,
                encoded,
                &mut receipt.catalog,
                original,
                check,
            )
            .await,
        ),
        None => (
            "catalog_delete",
            delete_catalog(&input, &catalog, &mut receipt.catalog, original, check).await,
        ),
    };
    if let Err(cause) = result {
        return Err(io::Error::new(
            cause.kind(),
            Failure {
                receipt,
                stage,
                cause,
            },
        ));
    }
    if receipt.catalog == "published" {
        receipt.catalog = "updated";
    }
    if let Err(cause) = check() {
        return Err(io::Error::new(
            cause.kind(),
            Failure {
                receipt,
                stage: "delivery",
                cause,
            },
        ));
    }
    Ok(receipt)
}

#[cfg(test)]
mod tests;
