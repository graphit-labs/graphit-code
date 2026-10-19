//! One finite installation request to the existing owner; absence alone permits offline startup.
use crate::{
    ast_source::runtime_headers,
    native_install::{
        InstallFailure, InstallOutcome, InstallRequest, PayloadEffect, RegistryEffect,
    },
    native_install_http::ROUTE,
};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use graphit_core::{
    native_document_file::NativeDocumentFileLimits, project_paths::InstallationPaths,
};
use graphit_daemon::source_discovery::{DiscoveryError, read_discovery};
use graphit_http::client::{
    BoundedClient, ClientPolicy, GuardedNetworkError, Limits, NetworkEffect, NetworkRequest,
    NetworkRequestContext,
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, SourceRef,
    native_package::PackageKind,
    policy::{HubResourcePolicy, RequestContext, ResourceSetting},
};
use serde::Deserialize;
use std::{fmt, io, path::PathBuf, time::Duration};

pub enum InstallClientError {
    Discovery(DiscoveryError),
    Local(io::Error),
    Network(GuardedNetworkError<io::Error>),
    Terminal {
        cause: io::Error,
        effect: NetworkEffect,
    },
    Observed {
        cause: io::Error,
        outcome: Box<InstallOutcome>,
    },
}
impl fmt::Debug for InstallClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl fmt::Display for InstallClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Discovery(_) => f.write_str("native installation discovery failed"),
            Self::Local(_) => f.write_str("native installation client preparation failed"),
            Self::Network(error) => write!(f, "native installation {error}"),
            Self::Terminal { effect, .. } => write!(
                f,
                "native installation terminal response unavailable ({effect:?})"
            ),
            Self::Observed { outcome, .. } => write!(
                f,
                "native installation delivery rejected (payload={:?}, registry={:?})",
                outcome.payload, outcome.registry
            ),
        }
    }
}
impl std::error::Error for InstallClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Discovery(e) => e,
            Self::Local(e) => e,
            Self::Network(e) => e,
            Self::Terminal { cause, .. } | Self::Observed { cause, .. } => cause,
        })
    }
}
impl From<io::Error> for InstallClientError {
    fn from(e: io::Error) -> Self {
        Self::Local(e)
    }
}

fn required_nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Terminal {
    producer: graphit_core::project_identity::ProjectId,
    id: String,
    version: String,
    #[serde(deserialize_with = "required_nullable")]
    kind: Option<PackageKind>,
    #[serde(deserialize_with = "required_nullable")]
    destination: Option<PathBuf>,
    payload: PayloadEffect,
    registry: RegistryEffect,
    #[serde(deserialize_with = "required_nullable")]
    failure: Option<TerminalFailure>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalFailure {
    category: crate::native_install::FailureCategory,
    phase: crate::native_install::InstallPhase,
    message: String,
}
fn decode(
    bytes: &[u8],
    installation: &InstallationPaths,
    request: &InstallRequest,
    policy: &HubResourcePolicy,
) -> io::Result<InstallOutcome> {
    let value: Terminal = serde_json::from_slice(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid native installation terminal schema",
        )
    })?;
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "native installation terminal identity or effects mismatch",
        )
    };
    if value.producer != request.producer
        || value.id != request.id
        || value.version != request.version
        || request.kind.is_some_and(|k| value.kind != Some(k))
        || (value.payload == PayloadEffect::NotPublished
            && value.registry != RegistryEffect::NotAttempted)
        || (value.failure.is_none()
            && !(value.payload == PayloadEffect::Published
                && value.registry == RegistryEffect::Registered))
        || (value.payload == PayloadEffect::Published
            && (value.kind.is_none() || value.destination.is_none()))
    {
        return Err(invalid());
    }
    if let Some(destination) = &value.destination {
        let kind = value.kind.ok_or_else(invalid)?;
        let source = SourceRef::hub_cache(
            installation,
            request.producer,
            match kind {
                PackageKind::Ast => ArtifactType::Ast,
                PackageKind::Knowledge => ArtifactType::Knowledge,
            },
            ArtifactId::new(&request.id, policy.projection()).map_err(io::Error::other)?,
            None,
            ConcreteVersion::new(&request.version, policy.projection())
                .map_err(io::Error::other)?,
            policy.projection(),
        )
        .map_err(io::Error::other)?;
        if destination != source.directory().as_path() {
            return Err(invalid());
        }
    }
    if value
        .failure
        .as_ref()
        .is_some_and(|e| e.message.len() > policy.content().max_scalar_bytes)
    {
        return Err(invalid());
    }
    Ok(InstallOutcome {
        producer: value.producer,
        id: value.id,
        version: value.version,
        kind: value.kind,
        destination: value.destination,
        payload: value.payload,
        registry: value.registry,
        failure: value.failure.map(|e| InstallFailure {
            category: e.category,
            phase: e.phase,
            message: e.message,
        }),
    })
}

/// `None` means a genuine absent discovery document; errors never trigger another owner.
pub async fn execute_if_active(
    installation: &InstallationPaths,
    request: &InstallRequest,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> Result<Option<InstallOutcome>, InstallClientError> {
    let deadline = original.snapshot().required_deadline()?;
    let number = |setting| usize::try_from(policy.limit(setting).value).map_err(io::Error::other);
    let documents = NativeDocumentFileLimits::new(
        policy.content().max_input_bytes,
        graphit_core::native_path::MAX_PATH_BYTES,
        number(ResourceSetting::SnapshotDepth)?,
        policy.content().max_scalar_bytes,
    )?;
    let Some(discovery) = read_discovery(installation, documents, original.snapshot())
        .map_err(InstallClientError::Discovery)?
    else {
        original.snapshot().check()?;
        return Ok(None);
    };
    let mut headers: HeaderMap = runtime_headers(installation, documents, original)
        .await?
        .into_iter()
        .collect();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let origin = format!("http://{}", discovery.address());
    let url = format!("{origin}{ROUTE}");
    let content = policy.content();
    let client = BoundedClient::new(
        ClientPolicy {
            origin: &origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        Limits {
            request_body_bytes: content.max_input_bytes,
            response_body_bytes: content.max_output_bytes,
            url_bytes: content.max_scalar_bytes,
            header_bytes: content.max_input_bytes,
            header_count: content.max_collection_entries,
            max_in_flight: 1,
            max_idle_per_host: 1,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 1,
            max_certificate_bytes: content.max_input_bytes,
        },
    )
    .map_err(|e| InstallClientError::Local(io::Error::other(e)))?;
    let bytes = serde_json::to_vec(request).map_err(io::Error::other)?;
    if bytes.len() > client.request_body_bytes() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native installation request exceeds captured limit",
        )
        .into());
    }
    let mut gate = || original.snapshot().check();
    let response = client
        .execute_guarded(
            NetworkRequest {
                method: Method::POST,
                url: &url,
                headers: &headers,
                body: &bytes,
            },
            NetworkRequestContext::new(deadline, original.cancellation()),
            &mut gate,
        )
        .await
        .map_err(InstallClientError::Network)?;
    let terminal_error = |cause| InstallClientError::Terminal {
        cause,
        effect: NetworkEffect::MayHaveApplied,
    };
    if response.status() != StatusCode::OK
        || response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_none_or(|s| s != "application/json")
    {
        return Err(terminal_error(io::Error::new(
            io::ErrorKind::InvalidData,
            "native installation HTTP response has no terminal outcome",
        )));
    }
    original.snapshot().check().map_err(terminal_error)?;
    let outcome = decode(response.body(), installation, request, policy).map_err(terminal_error)?;
    if let Err(cause) = original.snapshot().check() {
        return Err(InstallClientError::Observed {
            cause,
            outcome: Box::new(outcome),
        });
    }
    Ok(Some(outcome))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_terminal_rejects_ambiguous_identity_and_effects() {
        let directory = tempfile::tempdir().unwrap();
        let brand = graphit_core::brand::Brand::default();
        let installation = InstallationPaths::resolve(
            brand.clone(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let config = graphit_core::config::ConfigSnapshot::new(&brand, []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let request = InstallRequest {
            input: directory.path().join("input.zip"),
            producer: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            id: "vendor/corpus".into(),
            version: "v1".into(),
            kind: None,
        };
        let outcome = InstallOutcome {
            producer: request.producer,
            id: request.id.clone(),
            version: request.version.clone(),
            kind: None,
            destination: None,
            payload: PayloadEffect::NotPublished,
            registry: RegistryEffect::NotAttempted,
            failure: Some(InstallFailure {
                category: crate::native_install::FailureCategory::InvalidInput,
                phase: crate::native_install::InstallPhase::Input,
                message: "native installation failed".into(),
            }),
        };
        let valid = serde_json::to_value(&outcome).unwrap();
        assert!(
            decode(
                &serde_json::to_vec(&valid).unwrap(),
                &installation,
                &request,
                &policy
            )
            .is_ok()
        );
        for malformed in [
            {
                let mut v = valid.clone();
                v["unknown"] = serde_json::json!(1);
                v
            },
            {
                let mut v = valid.clone();
                v["failure"]["unknown"] = serde_json::json!(1);
                v
            },
            {
                let mut v = valid.clone();
                v.as_object_mut().unwrap().remove("failure");
                v
            },
            {
                let mut v = valid.clone();
                v.as_object_mut().unwrap().remove("kind");
                v
            },
            {
                let mut v = valid.clone();
                v["id"] = serde_json::json!("vendor/other");
                v
            },
            {
                let mut v = valid.clone();
                v["registry"] = serde_json::to_value(RegistryEffect::Registered).unwrap();
                v
            },
            {
                let mut v = valid.clone();
                v["failure"] = serde_json::Value::Null;
                v
            },
            {
                let mut v = valid.clone();
                v["payload"] = serde_json::to_value(PayloadEffect::Published).unwrap();
                v
            },
        ] {
            assert!(
                decode(
                    &serde_json::to_vec(&malformed).unwrap(),
                    &installation,
                    &request,
                    &policy
                )
                .is_err()
            );
        }
        let wire = serde_json::to_string(&valid).unwrap();
        let duplicate = format!("{},\"version\":\"v1\"}}", wire.strip_suffix('}').unwrap());
        assert!(decode(duplicate.as_bytes(), &installation, &request, &policy).is_err());
        assert!(!installation.root().exists());
    }
}
