//! Rerank one already selected page through its caller's captured AI service.
//!
//! This module neither selects candidates nor captures service authority. Callers
//! enter it only for an explicit `search.rerank = true`, and retain their source
//! pin, generation, cursor, total and original retrieval scores.
use axum::http::{HeaderMap, HeaderValue, header};
use graphit_ai::{
    context::InferenceContext,
    direct_native_rerank::{self, DirectNativeRerankError},
    graphit_rerank::GraphitReranker,
    native_rerank::{GuardedRerankError, NativeRerankError, NativeRerankLimits},
    ranking::{self, RankingError, RankingLimits},
};
use graphit_auth::{
    ServiceBindingView, ServiceCredentialLease, ServiceMode, ServicePurpose, ServiceSelection,
    ServiceUseError, broker::BrokerRerankService,
};
use graphit_core::{file_io::SnapshotContext, runtime_key::RuntimeKeyOwner};
use graphit_daemon::{
    broker_verifiers::{BrokerVerifierCache, BrokerVerifierCacheError},
    model_service::{ExpectedModel, ModelServiceClient, ModelServiceError},
};
use graphit_http::client::{BoundedClient, ClientPolicy, Limits, NetworkEffect};
use graphit_hub::policy::{HubResourcePolicy, ResourceSetting};
use std::{error::Error, fmt, io, net::SocketAddr, sync::Arc};
use zeroize::Zeroizing;

pub(crate) struct LocalRerankResources<'a> {
    pub(crate) client: &'a ModelServiceClient,
    pub(crate) runtime: &'a RuntimeKeyOwner,
    pub(crate) expected: &'a ExpectedModel,
    pub(crate) address: SocketAddr,
}

pub(crate) fn configured(snapshot: &graphit_core::config::ConfigSnapshot) -> io::Result<bool> {
    let key = graphit_core::config::ConfigKey::new("search.rerank")?;
    Ok(snapshot.get::<bool>(&key)?.unwrap_or(false))
}

/// Use the selected service's actual origin. Embedding and reranking may select
/// different endpoints; a pool pinned to one is not a pool for the other.
pub(crate) fn http_client(
    selection: &mut ServiceSelection<'_>,
    config: &graphit_core::config::ConfigSnapshot,
    policy: &HubResourcePolicy,
) -> io::Result<Option<BoundedClient>> {
    let endpoint = match selection.binding().map_err(io::Error::other)? {
        ServiceBindingView::Local {
            purpose: ServicePurpose::Embedding | ServicePurpose::Rerank,
            ..
        } => return Ok(None),
        ServiceBindingView::Direct {
            purpose: ServicePurpose::Embedding | ServicePurpose::Rerank,
            endpoint,
            ..
        }
        | ServiceBindingView::Broker {
            purpose: ServicePurpose::Embedding | ServicePurpose::Rerank,
            endpoint,
            ..
        } => endpoint,
        _ => return Err(io::ErrorKind::PermissionDenied.into()),
    };
    let number = |setting| usize::try_from(policy.limit(setting).value).map_err(io::Error::other);
    let transport = number(ResourceSetting::TransportBytes)?;
    BoundedClient::new(
        ClientPolicy {
            origin: endpoint,
            allow_loopback_http: config
                .get::<bool>(&graphit_core::config::ConfigKey::new(
                    "broker.allow_loopback_http",
                )?)?
                .unwrap_or(false),
            proxy: None,
            additional_roots: &[],
        },
        Limits {
            request_body_bytes: transport,
            response_body_bytes: transport,
            url_bytes: 8192,
            header_bytes: 65536,
            header_count: 32,
            max_in_flight: number(ResourceSetting::TransportInflight)?,
            max_idle_per_host: 1,
            idle_timeout: std::time::Duration::from_secs(30),
            max_certificates: 8,
            max_certificate_bytes: 65536,
        },
    )
    .map(Some)
    .map_err(io::Error::other)
}

/// Borrowed composition data; constructing these references loads no model.
pub(crate) struct SearchRerankResources<'a> {
    pub(crate) local: Option<LocalRerankResources<'a>>,
    pub(crate) http: Option<&'a BoundedClient>,
    pub(crate) verifiers: Option<&'a BrokerVerifierCache>,
    /// Validated discovery data, borrowed from the existing metadata owner.
    /// The descriptor remains an observation, never service authority.
    pub(crate) broker: Option<&'a BrokerRerankService>,
    pub(crate) limits: NativeRerankLimits,
    pub(crate) ranking: RankingLimits,
}

/// Scores retain input order. `order` is a permutation of those input positions.
#[derive(Debug)]
pub(crate) struct RerankScores {
    pub(crate) scores: Vec<f64>,
    pub(crate) order: Vec<usize>,
}

enum Failure {
    Request(io::Error),
    Service(ServiceUseError),
    Verifier(BrokerVerifierCacheError),
    Discovery(graphit_auth::broker::BrokerError),
    Models(ModelServiceError),
    Direct(DirectNativeRerankError),
    Broker(GuardedRerankError<ServiceUseError>),
    Native(NativeRerankError),
    Ranking(RankingError),
    ForeignRequest,
    WrongPurpose,
    MissingLocalResources,
    MissingHttpResources,
    MissingBrokerDescriptor,
    MissingBrokerVerifier,
    InvalidCredentialHeader,
    Allocation,
}

pub(crate) struct SearchRerankError {
    cause: Failure,
    effect: NetworkEffect,
}
impl SearchRerankError {
    #[cfg(test)]
    pub(crate) fn effect(&self) -> NetworkEffect {
        self.effect
    }
    pub(crate) fn code(&self) -> &'static str {
        fn native(error: &NativeRerankError) -> &'static str {
            match error {
                NativeRerankError::Context(_)
                | NativeRerankError::Network(
                    graphit_http::client::NetworkError::Cancelled
                    | graphit_http::client::NetworkError::Deadline,
                ) => "request_cancelled_or_expired",
                NativeRerankError::Limit
                | NativeRerankError::Network(graphit_http::client::NetworkError::Limit) => {
                    "rerank_limit"
                }
                NativeRerankError::Shape | NativeRerankError::Encoding => "rerank_invalid_output",
                _ => "rerank_service_failure",
            }
        }
        fn service(error: &ServiceUseError) -> &'static str {
            match crate::embedding_service::service_kind(error) {
                graphit_daemon::model_service::ModelErrorKind::Cancelled
                | graphit_daemon::model_service::ModelErrorKind::Deadline => {
                    "request_cancelled_or_expired"
                }
                graphit_daemon::model_service::ModelErrorKind::Authentication => {
                    "authentication_or_permission_denied"
                }
                graphit_daemon::model_service::ModelErrorKind::Limit => "rerank_limit",
                _ => "rerank_service_failure",
            }
        }
        match &self.cause {
            Failure::Request(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
                ) =>
            {
                "request_cancelled_or_expired"
            }
            Failure::Service(error) => service(error),
            Failure::ForeignRequest | Failure::WrongPurpose => {
                "authentication_or_permission_denied"
            }
            Failure::Native(error) => native(error),
            Failure::Broker(GuardedRerankError::Rerank { source, .. }) => native(source),
            Failure::Broker(GuardedRerankError::Admission { source, .. }) => service(source),
            Failure::Direct(DirectNativeRerankError::Rerank { source, .. }) => native(source),
            Failure::Direct(DirectNativeRerankError::Service { source, .. }) => service(source),
            Failure::Direct(DirectNativeRerankError::Context(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
                ) =>
            {
                "request_cancelled_or_expired"
            }
            Failure::Models(error) => match error.kind() {
                graphit_daemon::model_service::ModelErrorKind::Cancelled
                | graphit_daemon::model_service::ModelErrorKind::Deadline => {
                    "request_cancelled_or_expired"
                }
                graphit_daemon::model_service::ModelErrorKind::Limit => "rerank_limit",
                _ => "rerank_service_failure",
            },
            Failure::Ranking(RankingError::Context(_)) => "request_cancelled_or_expired",
            Failure::Ranking(RankingError::Limit) => "rerank_limit",
            Failure::MissingLocalResources
            | Failure::MissingHttpResources
            | Failure::MissingBrokerDescriptor
            | Failure::MissingBrokerVerifier => "rerank_service_unavailable",
            Failure::Discovery(
                graphit_auth::broker::BrokerError::Cancelled
                | graphit_auth::broker::BrokerError::Deadline,
            ) => "request_cancelled_or_expired",
            _ => "rerank_service_failure",
        }
    }
}
impl fmt::Debug for SearchRerankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SearchRerankError")
            .field("effect", &self.effect)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for SearchRerankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("search reranking failed")
    }
}
impl Error for SearchRerankError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.cause {
            Failure::Request(e) => Some(e),
            Failure::Service(e) => Some(e),
            Failure::Verifier(e) => Some(e),
            Failure::Discovery(e) => Some(e),
            Failure::Models(e) => Some(e),
            Failure::Direct(e) => Some(e),
            Failure::Broker(e) => Some(e),
            Failure::Native(e) => Some(e),
            Failure::Ranking(e) => Some(e),
            _ => None,
        }
    }
}

fn check(
    original: &Arc<SnapshotContext>,
    selection: &mut ServiceSelection<'_>,
) -> Result<(), Failure> {
    original.check().map_err(Failure::Request)?;
    if !Arc::ptr_eq(
        original,
        selection.original().map_err(Failure::Service)?.snapshot(),
    ) {
        return Err(Failure::ForeignRequest);
    }
    match selection.binding().map_err(Failure::Service)? {
        ServiceBindingView::Local { purpose, .. }
        | ServiceBindingView::Direct { purpose, .. }
        | ServiceBindingView::Broker { purpose, .. }
            if purpose == ServicePurpose::Rerank =>
        {
            Ok(())
        }
        _ => Err(Failure::WrongPurpose),
    }
}

fn owned(text: &str) -> Result<String, Failure> {
    let mut value = String::new();
    value
        .try_reserve_exact(text.len())
        .map_err(|_| Failure::Allocation)?;
    value.push_str(text);
    Ok(value)
}

async fn broker_scores(
    lease: &ServiceCredentialLease<'_>,
    descriptor: &BrokerRerankService,
    resources: &SearchRerankResources<'_>,
    query: &str,
    documents: &[&str],
) -> Result<Vec<f64>, Failure> {
    lease.check().map_err(Failure::Service)?;
    if !matches!(
        lease.binding().map_err(Failure::Service)?,
        ServiceBindingView::Broker {
            purpose: ServicePurpose::Rerank,
            ..
        }
    ) {
        return Err(Failure::WrongPurpose);
    }
    let original = lease.original().map_err(Failure::Service)?;
    let deadline = original
        .snapshot()
        .required_deadline()
        .map_err(Failure::Request)?;
    let context = InferenceContext::new(original.cancellation(), deadline);
    let mut headers = HeaderMap::new();
    if let Some(key) = lease.credential().map_err(Failure::Service)? {
        let bearer = Zeroizing::new(format!("Bearer {key}"));
        let mut value =
            HeaderValue::from_str(&bearer).map_err(|_| Failure::InvalidCredentialHeader)?;
        value.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, value);
    }
    let http = resources.http.ok_or(Failure::MissingHttpResources)?;
    GraphitReranker::new(http, descriptor, resources.limits)
        .map_err(Failure::Native)?
        .score_guarded(query, documents, &headers, context, &mut || lease.check())
        .await
        .map_err(Failure::Broker)
}

/// Score exactly this page, without adopting another request or source generation.
/// Every error remains terminal; no retrieval-score or similarity substitution.
pub(crate) async fn score(
    original: &Arc<SnapshotContext>,
    selection: &mut ServiceSelection<'_>,
    resources: &SearchRerankResources<'_>,
    query: &str,
    documents: &[&str],
) -> Result<RerankScores, SearchRerankError> {
    let mut effect = NetworkEffect::NotSent;
    let result = async {
        check(original, selection)?;
        let deadline = original.required_deadline().map_err(Failure::Request)?;
        let context = InferenceContext::new(original.cancellation(), deadline);
        // Empty pages need no credential discovery, connection or model load.
        if documents.is_empty() {
            return Ok(RerankScores {
                scores: Vec::new(),
                order: Vec::new(),
            });
        }
        let scores = match selection.mode().map_err(Failure::Service)? {
            ServiceMode::LocalAuthorized => {
                let local = resources
                    .local
                    .as_ref()
                    .ok_or(Failure::MissingLocalResources)?;
                let lease = selection.local().map_err(Failure::Service)?;
                let mut texts = Vec::new();
                texts
                    .try_reserve_exact(documents.len())
                    .map_err(|_| Failure::Allocation)?;
                for document in documents {
                    original.check().map_err(Failure::Request)?;
                    texts.push(owned(document)?);
                }
                let query = owned(query)?;
                // A failed local RPC cannot prove that inference was not started.
                effect = NetworkEffect::MayHaveApplied;
                local
                    .client
                    .rerank(
                        &lease,
                        local.runtime,
                        local.expected,
                        query,
                        texts,
                        tokio::net::TcpStream::connect(local.address),
                    )
                    .await
                    .map_err(Failure::Models)?
                    .scores
            }
            ServiceMode::Direct => {
                let http = resources.http.ok_or(Failure::MissingHttpResources)?;
                let lease = selection.credential().map_err(Failure::Service)?;
                direct_native_rerank::score(&lease, query, documents, http, resources.limits)
                    .await
                    .map_err(Failure::Direct)?
            }
            ServiceMode::BrokerNative => {
                let descriptor = resources.broker.ok_or(Failure::MissingBrokerDescriptor)?;
                let verifier = resources
                    .verifiers
                    .ok_or(Failure::MissingBrokerVerifier)?
                    .get(selection)
                    .map_err(Failure::Verifier)?;
                let lease = selection
                    .native_credential(&verifier)
                    .await
                    .map_err(Failure::Service)?;
                broker_scores(&lease, descriptor, resources, query, documents).await?
            }
            ServiceMode::BrokerKey | ServiceMode::BrokerIncoming | ServiceMode::BrokerAnonymous => {
                let descriptor = resources.broker.ok_or(Failure::MissingBrokerDescriptor)?;
                let lease = selection.credential().map_err(Failure::Service)?;
                broker_scores(&lease, descriptor, resources, query, documents).await?
            }
            ServiceMode::StandaloneS3 => return Err(Failure::WrongPurpose),
        };
        effect = NetworkEffect::MayHaveApplied;
        check(original, selection)?;
        let mut positions = Vec::new();
        positions
            .try_reserve_exact(documents.len())
            .map_err(|_| Failure::Allocation)?;
        positions.extend(0..documents.len());
        let order = ranking::rank_order(&positions, &scores, resources.ranking, context)
            .map_err(Failure::Ranking)?;
        check(original, selection)?;
        Ok(RerankScores { scores, order })
    }
    .await;
    result.map_err(|cause| {
        let effect = match &cause {
            Failure::Direct(error) => error.effect(),
            Failure::Broker(error) => error.effect(),
            _ => effect,
        };
        SearchRerankError { cause, effect }
    })
}

/// Resolve only an explicitly requested rerank operation. Callers keep this
/// entire function behind the effective opt-in guard, including HTTP/model data.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run(
    original: &Arc<SnapshotContext>,
    selection: &mut ServiceSelection<'_>,
    local_owner: Option<(
        &crate::foreground_project_control::ProjectControl,
        &RuntimeKeyOwner,
    )>,
    config: &graphit_core::config::ConfigSnapshot,
    policy: &HubResourcePolicy,
    verifiers: Option<&BrokerVerifierCache>,
    query: &str,
    documents: &[&str],
) -> Result<RerankScores, SearchRerankError> {
    let before = |cause| SearchRerankError {
        cause,
        effect: NetworkEffect::NotSent,
    };
    check(original, selection).map_err(before)?;
    original
        .required_deadline()
        .map_err(|error| before(Failure::Request(error)))?;
    if documents.is_empty() {
        return Ok(RerankScores {
            scores: Vec::new(),
            order: Vec::new(),
        });
    }
    let mode = selection
        .mode()
        .map_err(|error| before(Failure::Service(error)))?;
    let local = if mode == ServiceMode::LocalAuthorized {
        let (control, _) = local_owner.ok_or_else(|| before(Failure::MissingLocalResources))?;
        let lease = selection
            .local()
            .map_err(|error| before(Failure::Service(error)))?;
        Some(
            control
                .ensure_local_model(&lease)
                .await
                .map_err(|error| before(Failure::Request(error)))?,
        )
    } else {
        None
    };
    let http =
        http_client(selection, config, policy).map_err(|error| before(Failure::Request(error)))?;
    let descriptor = if matches!(
        mode,
        ServiceMode::BrokerNative
            | ServiceMode::BrokerKey
            | ServiceMode::BrokerIncoming
            | ServiceMode::BrokerAnonymous
    ) {
        let cache = verifiers.ok_or_else(|| before(Failure::MissingBrokerVerifier))?;
        let verifier = cache
            .get_for_service(selection)
            .map_err(|error| before(Failure::Verifier(error)))?;
        let metadata = verifier
            .metadata(graphit_auth::broker::BrokerRequestContext::new(original))
            .await;
        check(original, selection).map_err(before)?;
        Some(
            metadata
                .map_err(|error| before(Failure::Discovery(error)))?
                .rerank_service()
                .map_err(|error| before(Failure::Discovery(error)))?
                .ok_or_else(|| before(Failure::MissingBrokerDescriptor))?,
        )
    } else {
        None
    };
    let content = policy.content();
    let transport = usize::try_from(policy.limit(ResourceSetting::TransportBytes).value)
        .map_err(|error| before(Failure::Request(io::Error::other(error))))?;
    let limits = NativeRerankLimits::new(
        content.max_collection_entries,
        content.max_collection_entries,
        content.max_input_bytes,
        transport,
        transport,
    )
    .map_err(|error| before(Failure::Native(error)))?;
    let ranking = RankingLimits::new(content.max_collection_entries, 1, 1)
        .map_err(|error| before(Failure::Ranking(error)))?;
    let runtime = local_owner.map(|(_, runtime)| runtime);
    let resources = SearchRerankResources {
        local: local
            .as_ref()
            .zip(runtime)
            .map(
                |((client, expected, address), runtime)| LocalRerankResources {
                    client,
                    runtime,
                    expected,
                    address: *address,
                },
            ),
        http: http.as_ref(),
        verifiers,
        broker: descriptor.as_ref(),
        limits,
        ranking,
    };
    score(original, selection, &resources, query, documents).await
}

#[cfg(test)]
mod tests;
