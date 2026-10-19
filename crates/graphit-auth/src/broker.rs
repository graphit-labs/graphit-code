//! Broker identity verification. Identity does not confer a role or permission.
mod json;
pub mod oauth;
pub mod project_access;
mod purpose;
mod services;
pub use services::{BrokerEmbeddingService, BrokerModelIdentity, BrokerRerankService};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_core::file_io::SnapshotContext;
use graphit_http::client::{
    BoundedClient, ClientPolicy, Limits as NetworkLimits, NetworkError, NetworkRequest,
    NetworkRequestContext,
};
use http::{HeaderMap, Method, StatusCode};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, errors::ErrorKind};
pub use purpose::VerifiedNativeProfileClaims;
use purpose::{Claims, TokenPurpose};
use secrecy::zeroize::Zeroizing;
use serde_json::Value;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex as AsyncMutex;
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrokerError {
    InvalidPolicy,
    InvalidMetadata,
    InvalidDocument,
    InvalidKeys,
    InvalidToken,
    Limit,
    Busy,
    Cancelled,
    Deadline,
    Connection,
    Transport,
}
impl std::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "broker authentication: {self:?}")
    }
}
impl std::error::Error for BrokerError {}
impl From<NetworkError> for BrokerError {
    fn from(error: NetworkError) -> Self {
        match error {
            NetworkError::Cancelled => Self::Cancelled,
            NetworkError::Deadline => Self::Deadline,
            NetworkError::Busy => Self::Busy,
            NetworkError::Limit => Self::Limit,
            NetworkError::Connection => Self::Connection,
            _ => Self::Transport,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct JsonLimits {
    pub bytes: usize,
    pub nodes: usize,
    pub depth: usize,
    pub scalar_bytes: usize,
    pub collection_entries: usize,
}
impl JsonLimits {
    fn validate(self) -> Result<(), BrokerError> {
        if [
            self.bytes,
            self.nodes,
            self.depth,
            self.scalar_bytes,
            self.collection_entries,
        ]
        .contains(&0)
            || self.depth > 128
            || self.bytes.checked_add(1).is_none()
            || self.bytes > isize::MAX as usize
        {
            return Err(BrokerError::InvalidPolicy);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub struct VerifierLimits {
    pub token_bytes: usize,
    pub json: JsonLimits,
    pub max_keys: usize,
    pub metadata_ttl: Duration,
    pub key_ttl: Duration,
    pub unknown_key_cooldown: Duration,
}
impl VerifierLimits {
    fn validate(self) -> Result<(), BrokerError> {
        self.json.validate()?;
        if self.token_bytes == 0
            || self.token_bytes > isize::MAX as usize
            || self.max_keys == 0
            || self.max_keys > self.json.collection_entries
        {
            return Err(BrokerError::InvalidPolicy);
        }
        for ttl in [self.metadata_ttl, self.key_ttl, self.unknown_key_cooldown] {
            if ttl.is_zero() || Instant::now().checked_add(ttl).is_none() {
                return Err(BrokerError::InvalidPolicy);
            }
        }
        Ok(())
    }
}
pub struct BrokerPolicy<'a> {
    pub endpoint: &'a str,
    pub mcp_resource: Option<&'a str>,
    pub provider_revision: u64,
    pub network: ClientPolicy<'a>,
}
#[derive(Clone, Copy)]
pub struct BrokerRequestContext<'a> {
    original: &'a SnapshotContext,
}
impl<'a> BrokerRequestContext<'a> {
    pub fn new(original: &'a SnapshotContext) -> Self {
        Self { original }
    }
    fn check(self) -> Result<(), BrokerError> {
        self.original.check().map_err(|error| match error.kind() {
            std::io::ErrorKind::Interrupted => BrokerError::Cancelled,
            std::io::ErrorKind::TimedOut => BrokerError::Deadline,
            _ => BrokerError::InvalidPolicy,
        })
    }
    fn network(self) -> NetworkRequestContext<'a> {
        NetworkRequestContext::with_deadline(self.original.deadline(), self.original.cancellation())
    }
}

/// Public metadata is data; it never restores an authenticated caller.
pub struct BrokerMetadata {
    issuer: String,
    client_id: String,
    access_token_audience: String,
    scopes: Vec<String>,
    redirect_path: String,
    authorization_endpoint: String,
    token_endpoint: String,
    revocation_endpoint: Option<String>,
    registration_endpoint: Option<String>,
    device_authorization_endpoint: Option<String>,
    jwks_uri: String,
    services: services::BrokerServices,
    url_bytes: usize,
}
impl BrokerMetadata {
    pub fn issuer(&self) -> &str {
        &self.issuer
    }
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
    pub fn access_token_audience(&self) -> &str {
        &self.access_token_audience
    }
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }
    pub fn redirect_path(&self) -> &str {
        &self.redirect_path
    }
    pub fn authorization_endpoint(&self) -> &str {
        &self.authorization_endpoint
    }
    pub fn token_endpoint(&self) -> &str {
        &self.token_endpoint
    }
    pub fn revocation_endpoint(&self) -> Option<&str> {
        self.revocation_endpoint.as_deref()
    }
    pub fn device_authorization_endpoint(&self) -> Option<&str> {
        self.device_authorization_endpoint.as_deref()
    }
    pub fn jwks_uri(&self) -> &str {
        &self.jwks_uri
    }
    /// Public discovery selects STSv3 only; the authenticated credentials response
    /// supplies the current topology and authorization revision.
    pub fn s3_enabled(&self) -> Result<bool, BrokerError> {
        self.services.s3_enabled(|path| self.service_endpoint(path))
    }
    /// Resolve only the selected embedding descriptor from cached discovery.
    /// Discovery observations do not confer authentication or PROJECT access.
    pub fn embedding_service(&self) -> Result<Option<BrokerEmbeddingService>, BrokerError> {
        self.services.embedding(|path| self.service_endpoint(path))
    }
    /// Call only for explicitly enabled reranking; no network request is made.
    pub fn rerank_service(&self) -> Result<Option<BrokerRerankService>, BrokerError> {
        self.services.rerank(|path| self.service_endpoint(path))
    }
    fn service_endpoint(&self, path: &str) -> Result<String, BrokerError> {
        let endpoint = format!("{}{path}", self.issuer.trim_end_matches('/'));
        let url =
            checked_url(&endpoint, self.url_bytes).map_err(|_| BrokerError::InvalidMetadata)?;
        let issuer = Url::parse(&self.issuer).map_err(|_| BrokerError::InvalidMetadata)?;
        if url.origin() != issuer.origin() {
            return Err(BrokerError::InvalidMetadata);
        }
        Ok(endpoint)
    }
}
/// Only a locally verified signed access token constructs this identity proof.
/// ```compile_fail
/// use graphit_auth::broker::AuthenticatedBrokerCaller;
/// fn debug<T: std::fmt::Debug>() {} debug::<AuthenticatedBrokerCaller<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::broker::AuthenticatedBrokerCaller;
/// fn clone<T: Clone>() {} clone::<AuthenticatedBrokerCaller<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::broker::AuthenticatedBrokerCaller;
/// fn serialize<T: serde::Serialize>() {} serialize::<AuthenticatedBrokerCaller<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::broker::AuthenticatedBrokerCaller;
/// let caller = AuthenticatedBrokerCaller { bearer: "body-selected", subject: "chosen".into() };
/// ```
pub struct AuthenticatedBrokerCaller<'a> {
    bearer: &'a str,
    issuer: &'a str,
    provider_revision: u64,
    subject: String,
    username: Option<String>,
    organization: Option<String>,
    teams: Vec<String>,
    expires_at: SystemTime,
    expires_mono: Instant,
    original: &'a SnapshotContext,
    source: &'a BrokerVerifier,
}
impl AuthenticatedBrokerCaller<'_> {
    pub fn subject(&self) -> &str {
        &self.subject
    }
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }
    pub fn organization(&self) -> Option<&str> {
        self.organization.as_deref()
    }
    pub fn teams(&self) -> &[String] {
        &self.teams
    }
    pub fn issuer(&self) -> &str {
        self.issuer
    }
    pub fn provider_revision(&self) -> u64 {
        self.provider_revision
    }
    pub fn expires_at(&self) -> SystemTime {
        self.expires_at
    }
    pub fn bearer_for_broker(&self) -> &str {
        self.bearer
    }
    pub fn check(
        &self,
        verifier: &BrokerVerifier,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), BrokerError> {
        verifier.check_proof(
            self.source,
            self.original,
            self.provider_revision,
            self.expires_at,
            self.expires_mono,
            context,
        )
    }
}

#[derive(Clone, Copy)]
struct Moment {
    utc: SystemTime,
    mono: Instant,
}
trait Clock: Send + Sync {
    fn now(&self) -> Moment;
}
struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Moment {
        Moment {
            utc: SystemTime::now(),
            mono: Instant::now(),
        }
    }
}
struct PublicKey {
    kid: String,
    key: DecodingKey,
}
struct Cached {
    metadata: Arc<BrokerMetadata>,
    keys: Vec<PublicKey>,
    metadata_until: Instant,
    keys_until: Instant,
}
#[derive(Default)]
struct Cache {
    entry: Option<Arc<Cached>>,
    next_forced: Option<Instant>,
}
pub struct BrokerVerifier {
    client: BoundedClient,
    endpoint: String,
    resource: Option<String>,
    revision: u64,
    origin: url::Origin,
    url_bytes: usize,
    limits: VerifierLimits,
    cache: Mutex<Cache>,
    fetch_gate: AsyncMutex<()>,
    clock: Arc<dyn Clock>,
}
impl BrokerVerifier {
    pub fn new(
        policy: BrokerPolicy<'_>,
        network_limits: NetworkLimits,
        limits: VerifierLimits,
    ) -> Result<Self, BrokerError> {
        limits.validate()?;
        if policy.provider_revision == 0 {
            return Err(BrokerError::InvalidPolicy);
        }
        let endpoint = checked_url(policy.endpoint, network_limits.url_bytes)
            .map_err(|_| BrokerError::InvalidPolicy)?;
        let origin = checked_url(policy.network.origin, network_limits.url_bytes)
            .map_err(|_| BrokerError::InvalidPolicy)?;
        let resource = policy
            .mcp_resource
            .map(|text| {
                checked_url(text, network_limits.url_bytes).map_err(|_| BrokerError::InvalidPolicy)
            })
            .transpose()?;
        if endpoint.origin() != origin.origin() || endpoint.query().is_some() {
            return Err(BrokerError::InvalidPolicy);
        }
        if resource.as_ref().is_some_and(|resource| {
            resource.scheme() != "https"
                && !(policy.network.allow_loopback_http
                    && resource.scheme() == "http"
                    && literal_loopback(resource))
        }) {
            return Err(BrokerError::InvalidPolicy);
        }
        let client = BoundedClient::new(policy.network, network_limits)
            .map_err(|_| BrokerError::InvalidPolicy)?;
        Ok(Self {
            client,
            endpoint: policy.endpoint.into(),
            resource: policy.mcp_resource.map(str::to_owned),
            revision: policy.provider_revision,
            origin: origin.origin(),
            url_bytes: network_limits.url_bytes,
            limits,
            cache: Mutex::new(Cache::default()),
            fetch_gate: AsyncMutex::new(()),
            clock: Arc::new(SystemClock),
        })
    }
    /// Trusted configured API endpoint; discovery and claims cannot select it.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub(crate) fn matches_service_binding(
        &self,
        endpoint: &str,
        resource: Option<&str>,
        revision: u64,
    ) -> bool {
        self.endpoint == endpoint
            && self.resource.as_deref() == resource
            && self.revision == revision
    }
    pub async fn metadata(
        &self,
        context: BrokerRequestContext<'_>,
    ) -> Result<Arc<BrokerMetadata>, BrokerError> {
        context.check()?;
        let entry = self.cached(context).await?;
        context.check()?;
        Ok(entry.metadata.clone())
    }
    pub async fn verify<'a>(
        &'a self,
        raw: &'a str,
        context: BrokerRequestContext<'a>,
    ) -> Result<AuthenticatedBrokerCaller<'a>, BrokerError> {
        if self.resource.is_none() {
            return Err(BrokerError::InvalidPolicy);
        }
        self.verify_access(raw, TokenPurpose::IncomingMcp, context)
            .await
    }
    async fn verify_access<'a>(
        &'a self,
        raw: &'a str,
        purpose: TokenPurpose<'_>,
        context: BrokerRequestContext<'a>,
    ) -> Result<AuthenticatedBrokerCaller<'a>, BrokerError> {
        context.check()?;
        let anchor = self.clock.now();
        let kid = self.preflight(raw, purpose, context)?;
        let mut entry = self.cached(context).await?;
        let first = self.signed(raw, &kid, &entry, anchor, purpose, context);
        let claims = match first {
            Ok(claims) => claims,
            Err(SignedError::Reload) => {
                entry = self.forced(&entry, context).await?;
                self.signed(raw, &kid, &entry, anchor, purpose, context)
                    .map_err(|error| match error {
                        SignedError::Reload => BrokerError::InvalidToken,
                        SignedError::Denied(error) => error,
                    })?
            }
            Err(SignedError::Denied(error)) => return Err(error),
        };
        let expires_at = UNIX_EPOCH
            .checked_add(Duration::from_secs(claims.exp))
            .ok_or(BrokerError::InvalidToken)?;
        let remaining = expires_at
            .duration_since(anchor.utc)
            .map_err(|_| BrokerError::InvalidToken)?;
        let expires_mono = anchor
            .mono
            .checked_add(remaining)
            .ok_or(BrokerError::InvalidToken)?;
        self.current(expires_at, expires_mono, context)?;
        Ok(AuthenticatedBrokerCaller {
            bearer: raw,
            issuer: &self.endpoint,
            provider_revision: self.revision,
            subject: claims.sub,
            username: claims.preferred_username.filter(|name| !name.is_empty()),
            organization: claims.organization,
            teams: claims.groups.unwrap_or_default(),
            expires_at,
            expires_mono,
            original: context.original,
            source: self,
        })
    }
    fn current(
        &self,
        utc: SystemTime,
        mono: Instant,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), BrokerError> {
        context.check()?;
        let now = self.clock.now();
        if now.utc >= utc || now.mono >= mono {
            Err(BrokerError::InvalidToken)
        } else {
            Ok(())
        }
    }
    fn check_proof(
        &self,
        source: &Self,
        original: &SnapshotContext,
        revision: u64,
        utc: SystemTime,
        mono: Instant,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), BrokerError> {
        context.check()?;
        if !std::ptr::eq(self, source)
            || revision != self.revision
            || !original.same_request(context.original)
        {
            return Err(BrokerError::InvalidToken);
        }
        self.current(utc, mono, context)
    }

    /// Verifies a native ID/access pair, without MCP UserInfo or permission grants.
    pub async fn verify_login<'a>(
        &'a self,
        id_token: &str,
        access_token: &str,
        expected_nonce: &str,
        context: BrokerRequestContext<'a>,
    ) -> Result<VerifiedNativeLogin<'a>, BrokerError> {
        self.expected_input(expected_nonce)?;
        self.verify_pair(
            NativeInput {
                id: Some(id_token),
                access: access_token,
                nonce: Some(expected_nonce),
                subject: None,
            },
            (TokenPurpose::NativeId, TokenPurpose::NativeAccess),
            context,
        )
        .await
    }
    /// Verifies refreshed credentials against expected subject data. When the ID
    /// token is absent, profile_claims is None; the consumer preserves old data.
    pub async fn verify_refresh<'a>(
        &'a self,
        access_token: &str,
        id_token: Option<&str>,
        expected_subject: &str,
        context: BrokerRequestContext<'a>,
    ) -> Result<VerifiedNativeLogin<'a>, BrokerError> {
        self.expected_input(expected_subject)?;
        self.verify_pair(
            NativeInput {
                id: id_token,
                access: access_token,
                nonce: None,
                subject: Some(expected_subject),
            },
            (TokenPurpose::NativeId, TokenPurpose::NativeAccess),
            context,
        )
        .await
    }
    /// Verify an initial Device access token without an invented subject or ID token.
    pub async fn verify_device_login<'a>(
        &'a self,
        access_token: &str,
        context: BrokerRequestContext<'a>,
    ) -> Result<VerifiedNativeLogin<'a>, BrokerError> {
        if self.resource.is_some() {
            return Err(BrokerError::InvalidPolicy);
        }
        self.verify_pair(
            NativeInput {
                id: None,
                access: access_token,
                nonce: None,
                subject: None,
            },
            (TokenPurpose::NativeId, TokenPurpose::NativeAccess),
            context,
        )
        .await
    }
    fn expected_input(&self, input: &str) -> Result<(), BrokerError> {
        if input.is_empty() {
            Err(BrokerError::InvalidToken)
        } else if input.len() > self.limits.json.scalar_bytes {
            Err(BrokerError::Limit)
        } else {
            Ok(())
        }
    }
    async fn verify_pair<'a>(
        &'a self,
        input: NativeInput<'_>,
        purposes: (TokenPurpose<'_>, TokenPurpose<'_>),
        context: BrokerRequestContext<'a>,
    ) -> Result<VerifiedNativeLogin<'a>, BrokerError> {
        context.check()?;
        let anchor = self.clock.now();
        let access_kid = self.preflight(input.access, purposes.1, context)?;
        let id_kid = input
            .id
            .map(|raw| self.preflight(raw, purposes.0, context))
            .transpose()?;
        let mut entry = self.cached(context).await?;
        let keys = TokenKeys {
            access: &access_kid,
            id: id_kid.as_deref(),
        };
        let first = self.login_pair(&input, keys, &entry, anchor, purposes, context);
        let (access, id) = match first {
            Ok(pair) => pair,
            Err(SignedError::Reload) => {
                entry = self.forced(&entry, context).await?;
                self.login_pair(&input, keys, &entry, anchor, purposes, context)
                    .map_err(|error| match error {
                        SignedError::Reload => BrokerError::InvalidToken,
                        SignedError::Denied(error) => error,
                    })?
            }
            Err(SignedError::Denied(error)) => return Err(error),
        };
        let expiration = id.as_ref().map_or(access.exp, |id| id.exp.min(access.exp));
        let expires_at = UNIX_EPOCH
            .checked_add(Duration::from_secs(expiration))
            .ok_or(BrokerError::InvalidToken)?;
        let remaining = expires_at
            .duration_since(anchor.utc)
            .map_err(|_| BrokerError::InvalidToken)?;
        let expires_mono = anchor
            .mono
            .checked_add(remaining)
            .ok_or(BrokerError::InvalidToken)?;
        self.current(expires_at, expires_mono, context)?;
        Ok(VerifiedNativeLogin {
            issuer: &self.endpoint,
            provider_revision: self.revision,
            subject: access.sub,
            username: access.preferred_username,
            teams: access.groups,
            organization: access.organization,
            profile_claims: id
                .filter(|claims| {
                    !matches!(purposes.0, TokenPurpose::WebId(_))
                        || claims
                            .preferred_username
                            .as_ref()
                            .is_some_and(|name| !name.is_empty())
                })
                .map(Claims::into_profile)
                .transpose()?,
            expires_at,
            expires_mono,
            source: self,
            original: context.original,
        })
    }
    fn login_pair(
        &self,
        input: &NativeInput<'_>,
        keys: TokenKeys<'_>,
        entry: &Cached,
        anchor: Moment,
        purposes: (TokenPurpose<'_>, TokenPurpose<'_>),
        context: BrokerRequestContext<'_>,
    ) -> Result<(Claims, Option<Claims>), SignedError> {
        let id = input
            .id
            .zip(keys.id)
            .map(|(raw, kid)| {
                let claims = self.signed(raw, kid, entry, anchor, purposes.0, context)?;
                if let Some(nonce) = input.nonce {
                    claims.check_nonce(nonce).map_err(SignedError::Denied)?;
                }
                Ok(claims)
            })
            .transpose()?;
        let access = self.signed(
            input.access,
            keys.access,
            entry,
            anchor,
            purposes.1,
            context,
        )?;
        if id.as_ref().is_some_and(|id| id.sub != access.sub)
            || input.subject.is_some_and(|subject| access.sub != subject)
        {
            return Err(SignedError::Denied(BrokerError::InvalidToken));
        }
        Ok((access, id))
    }
    fn preflight(
        &self,
        raw: &str,
        purpose: TokenPurpose,
        context: BrokerRequestContext<'_>,
    ) -> Result<String, BrokerError> {
        if raw.len() > self.limits.token_bytes {
            return Err(BrokerError::Limit);
        }
        let parts: Vec<_> = raw.split('.').take(4).collect();
        if parts.len() != 3
            || parts.iter().any(|s| s.is_empty())
            || parts[2].len() != 86
            || URL_SAFE_NO_PAD
                .decode(parts[2])
                .map_err(|_| BrokerError::InvalidToken)?
                .len()
                != 64
        {
            return Err(BrokerError::InvalidToken);
        }
        let decoded = |part: &str| -> Result<Zeroizing<Vec<u8>>, BrokerError> {
            let tail = match part.len() % 4 {
                0 => 0,
                2 => 1,
                3 => 2,
                _ => return Err(BrokerError::InvalidToken),
            };
            let length = (part.len() / 4)
                .checked_mul(3)
                .and_then(|n| n.checked_add(tail))
                .ok_or(BrokerError::Limit)?;
            if length > self.limits.json.bytes {
                return Err(BrokerError::Limit);
            }
            let value = Zeroizing::new(
                URL_SAFE_NO_PAD
                    .decode(part)
                    .map_err(|_| BrokerError::InvalidToken)?,
            );
            if value.len() > self.limits.json.bytes {
                return Err(BrokerError::Limit);
            }
            Ok(value)
        };
        let header = json::parse(&decoded(parts[0])?, self.limits.json, context)?;
        if required_string(&header, "alg", BrokerError::InvalidToken)? != "EdDSA"
            || ["crit", "b64", "jku", "x5u", "jwk", "x5c"]
                .iter()
                .any(|key| header.get(key).is_some())
        {
            return Err(BrokerError::InvalidToken);
        }
        let kid = required_string(&header, "kid", BrokerError::InvalidToken)?.to_owned();
        let claims = json::parse(&decoded(parts[1])?, self.limits.json, context)?;
        if claims.get("nbf").is_some_and(|v| v.as_u64().is_none()) {
            return Err(BrokerError::InvalidToken);
        }
        if matches!(purpose, TokenPurpose::NativeId | TokenPurpose::WebId(_))
            && ["azp", "nonce"].iter().any(|key| {
                claims
                    .get(key)
                    .is_some_and(|value| value.as_str().is_none_or(str::is_empty))
            })
        {
            return Err(BrokerError::InvalidToken);
        }
        match claims.get("aud") {
            Some(Value::String(value)) if !value.is_empty() => {}
            Some(Value::Array(values)) if !values.is_empty() => {
                let mut seen = std::collections::BTreeSet::new();
                for value in values {
                    let text = value
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or(BrokerError::InvalidToken)?;
                    if !seen.insert(text) {
                        return Err(BrokerError::InvalidToken);
                    }
                }
            }
            _ => return Err(BrokerError::InvalidToken),
        }
        context.check()?;
        Ok(kid)
    }
    fn signed(
        &self,
        raw: &str,
        kid: &str,
        entry: &Cached,
        anchor: Moment,
        purpose: TokenPurpose,
        context: BrokerRequestContext<'_>,
    ) -> Result<Claims, SignedError> {
        context.check().map_err(SignedError::Denied)?;
        let key = entry
            .keys
            .iter()
            .find(|key| key.kid == kid)
            .ok_or(SignedError::Reload)?;
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.leeway = 0;
        validation.validate_nbf = true;
        validation.set_required_spec_claims(&["iss", "sub", "aud", "exp"]);
        let audience = match purpose {
            TokenPurpose::IncomingMcp => self
                .resource
                .as_deref()
                .ok_or(SignedError::Denied(BrokerError::InvalidPolicy))?,
            TokenPurpose::NativeId => &entry.metadata.client_id,
            TokenPurpose::NativeAccess | TokenPurpose::WebAccess(_) => {
                &entry.metadata.access_token_audience
            }
            TokenPurpose::WebId(client) => client.client_id(),
        };
        validation.set_audience(&[audience]);
        validation.set_issuer(&[&self.endpoint]);
        let claims = decode::<Claims>(raw, &key.key, &validation)
            .map_err(|error| {
                if matches!(error.kind(), ErrorKind::InvalidSignature) {
                    SignedError::Reload
                } else {
                    SignedError::Denied(BrokerError::InvalidToken)
                }
            })?
            .claims;
        claims
            .validate(purpose, &entry.metadata, self.resource.as_deref(), anchor)
            .map_err(SignedError::Denied)?;
        context.check().map_err(SignedError::Denied)?;
        Ok(claims)
    }
    async fn get(
        &self,
        url: &str,
        headers: &HeaderMap,
        context: BrokerRequestContext<'_>,
    ) -> Result<Value, BrokerError> {
        let response = self
            .client
            .execute(
                NetworkRequest {
                    method: Method::GET,
                    url,
                    headers,
                    body: &[],
                },
                context.network(),
            )
            .await?;
        if response.status() != StatusCode::OK {
            return Err(BrokerError::InvalidMetadata);
        }
        json::parse(response.body(), self.limits.json, context)
    }
    fn endpoint_url(&self, text: &str) -> Result<String, BrokerError> {
        let url = checked_url(text, self.url_bytes).map_err(|_| BrokerError::InvalidMetadata)?;
        if url.origin() != self.origin {
            return Err(BrokerError::InvalidMetadata);
        }
        Ok(text.to_owned())
    }
    async fn discover(
        &self,
        context: BrokerRequestContext<'_>,
    ) -> Result<BrokerMetadata, BrokerError> {
        let broker_url = self.discovery_url("/.well-known/graphit-broker")?;
        let empty = HeaderMap::new();
        let broker = self.get(&broker_url, &empty, context).await?;
        let error = BrokerError::InvalidMetadata;
        if required_string(&broker, "version", error)? != "1"
            || required_string(&broker, "issuer", error)? != self.endpoint
        {
            return Err(error);
        }
        let auth = broker.get("authentication").ok_or(error)?;
        if required_string(auth, "type", error)? != "oidc"
            || required_string(auth, "issuer", error)? != self.endpoint
        {
            return Err(error);
        }
        let client_id = required_string(auth, "client_id", error)?.to_owned();
        let scopes = string_list(auth, "scopes", error)?;
        let audiences = string_list(auth, "audiences", error)?;
        let resources = if auth.get("mcp_resources").is_some() {
            string_list(auth, "mcp_resources", error)?
        } else {
            Vec::new()
        };
        let audience = required_string(auth, "access_token_audience", error)?;
        let redirect_path = required_string(auth, "redirect_uri_path", error)?;
        if !scopes.iter().any(|v| v == "openid")
            || !audiences.iter().any(|v| v == audience)
            || self
                .resource
                .as_ref()
                .is_some_and(|resource| !resources.iter().any(|v| v == resource))
            || !redirect_path.starts_with('/')
            || redirect_path.contains(['?', '#'])
        {
            return Err(error);
        }
        let redirect_path = redirect_path.to_owned();
        let access_token_audience = audience.to_owned();
        let standard_url = self.discovery_url("/.well-known/openid-configuration")?;
        let standard = self.get(&standard_url, &empty, context).await?;
        if required_string(&standard, "issuer", error)? != self.endpoint {
            return Err(error);
        }
        for (field, required) in [
            ("grant_types_supported", "authorization_code"),
            ("grant_types_supported", "refresh_token"),
            ("code_challenge_methods_supported", "S256"),
            ("token_endpoint_auth_methods_supported", "none"),
            ("id_token_signing_alg_values_supported", "EdDSA"),
        ] {
            if !string_list(&standard, field, error)?
                .iter()
                .any(|v| v == required)
            {
                return Err(error);
            }
        }
        Ok(BrokerMetadata {
            services: services::BrokerServices::read(&broker)?,
            url_bytes: self.url_bytes,
            issuer: self.endpoint.clone(),
            client_id,
            access_token_audience,
            scopes,
            redirect_path,
            authorization_endpoint: self.endpoint_url(required_string(
                &standard,
                "authorization_endpoint",
                error,
            )?)?,
            token_endpoint: self.endpoint_url(required_string(
                &standard,
                "token_endpoint",
                error,
            )?)?,
            jwks_uri: self.endpoint_url(required_string(&standard, "jwks_uri", error)?)?,
            revocation_endpoint: standard
                .get("revocation_endpoint")
                .map(|_| {
                    self.endpoint_url(required_string(&standard, "revocation_endpoint", error)?)
                })
                .transpose()?,
            device_authorization_endpoint: if standard
                .get("device_authorization_endpoint")
                .is_some()
            {
                if !string_list(&standard, "grant_types_supported", error)?
                    .iter()
                    .any(|grant| grant == "urn:ietf:params:oauth:grant-type:device_code")
                {
                    return Err(error);
                }
                Some(self.endpoint_url(required_string(
                    &standard,
                    "device_authorization_endpoint",
                    error,
                )?)?)
            } else {
                None
            },
            registration_endpoint: standard
                .get("registration_endpoint")
                .map(|_| {
                    self.endpoint_url(required_string(&standard, "registration_endpoint", error)?)
                })
                .transpose()?,
        })
    }
    async fn keys(
        &self,
        metadata: &BrokerMetadata,
        context: BrokerRequestContext<'_>,
    ) -> Result<Vec<PublicKey>, BrokerError> {
        let document = self
            .get(&metadata.jwks_uri, &HeaderMap::new(), context)
            .await?;
        let values = document
            .get("keys")
            .and_then(Value::as_array)
            .ok_or(BrokerError::InvalidKeys)?;
        if values.len() > self.limits.max_keys {
            return Err(BrokerError::Limit);
        }
        let mut keys = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for value in values {
            context.check()?;
            let kid = required_string(value, "kid", BrokerError::InvalidKeys)?;
            if !seen.insert(kid)
                || ["d", "p", "q", "dp", "dq", "qi", "oth", "k"]
                    .iter()
                    .any(|field| value.get(field).is_some())
            {
                return Err(BrokerError::InvalidKeys);
            }
            let kty = required_string(value, "kty", BrokerError::InvalidKeys)?;
            if kty != "OKP" {
                continue;
            }
            if required_string(value, "crv", BrokerError::InvalidKeys)? != "Ed25519" {
                continue;
            }
            for (field, allowed) in [("alg", "EdDSA"), ("use", "sig")] {
                if value.get(field).is_some()
                    && required_string(value, field, BrokerError::InvalidKeys)? != allowed
                {
                    return Err(BrokerError::InvalidKeys);
                }
            }
            if value.get("key_ops").is_some()
                && string_list(value, "key_ops", BrokerError::InvalidKeys)? != ["verify"]
            {
                return Err(BrokerError::InvalidKeys);
            }
            let x = required_string(value, "x", BrokerError::InvalidKeys)?;
            if x.len() != 43
                || URL_SAFE_NO_PAD
                    .decode(x)
                    .map_err(|_| BrokerError::InvalidKeys)?
                    .len()
                    != 32
            {
                return Err(BrokerError::InvalidKeys);
            }
            keys.push(PublicKey {
                kid: kid.into(),
                key: DecodingKey::from_ed_components(x).map_err(|_| BrokerError::InvalidKeys)?,
            });
        }
        if keys.is_empty() {
            return Err(BrokerError::InvalidKeys);
        }
        Ok(keys)
    }
    fn cached_now(&self) -> Result<Option<Arc<Cached>>, BrokerError> {
        let now = Instant::now();
        Ok(self
            .cache
            .lock()
            .map_err(|_| BrokerError::InvalidPolicy)?
            .entry
            .as_ref()
            .filter(|entry| now < entry.metadata_until && now < entry.keys_until)
            .cloned())
    }
    fn discovery_url(&self, suffix: &str) -> Result<String, BrokerError> {
        let base = self.endpoint.trim_end_matches('/');
        if base
            .len()
            .checked_add(suffix.len())
            .ok_or(BrokerError::Limit)?
            > self.url_bytes
        {
            return Err(BrokerError::Limit);
        }
        self.endpoint_url(&format!("{base}{suffix}"))
    }
    async fn cached(&self, context: BrokerRequestContext<'_>) -> Result<Arc<Cached>, BrokerError> {
        context.check()?;
        if let Some(entry) = self.cached_now()? {
            return Ok(entry);
        }
        let _gate = tokio::select! {
            biased;
            _ = context.original.cancellation().cancelled() => return Err(BrokerError::Cancelled),
            _ = deadline_reached(context.original.deadline()) => return Err(BrokerError::Deadline),
            guard = self.fetch_gate.lock() => guard,
        };
        context.check()?;
        if let Some(entry) = self.cached_now()? {
            return Ok(entry);
        }
        let previous = self
            .cache
            .lock()
            .map_err(|_| BrokerError::InvalidPolicy)?
            .entry
            .clone();
        let (metadata, metadata_until) =
            if let Some(entry) = previous.filter(|e| Instant::now() < e.metadata_until) {
                (entry.metadata.clone(), entry.metadata_until)
            } else {
                let metadata = Arc::new(self.discover(context).await?);
                (
                    metadata,
                    Instant::now()
                        .checked_add(self.limits.metadata_ttl)
                        .ok_or(BrokerError::Limit)?,
                )
            };
        let keys = self.keys(&metadata, context).await?;
        let entry = Arc::new(Cached {
            metadata,
            keys,
            metadata_until,
            keys_until: Instant::now()
                .checked_add(self.limits.key_ttl)
                .ok_or(BrokerError::Limit)?,
        });
        context.check()?;
        self.cache
            .lock()
            .map_err(|_| BrokerError::InvalidPolicy)?
            .entry = Some(entry.clone());
        Ok(entry)
    }
    async fn forced(
        &self,
        old: &Cached,
        context: BrokerRequestContext<'_>,
    ) -> Result<Arc<Cached>, BrokerError> {
        context.check()?;
        let _gate = tokio::select! {
            biased;
            _ = context.original.cancellation().cancelled() => return Err(BrokerError::Cancelled),
            _ = deadline_reached(context.original.deadline()) => return Err(BrokerError::Deadline),
            guard = self.fetch_gate.lock() => guard,
        };
        let now = Instant::now();
        {
            let mut cache = self.cache.lock().map_err(|_| BrokerError::InvalidPolicy)?;
            if let Some(entry) = cache.entry.as_ref().filter(|entry| {
                !std::ptr::eq(entry.as_ref(), old)
                    && now < entry.metadata_until
                    && now < entry.keys_until
            }) {
                return Ok(Arc::clone(entry));
            }
            if cache.next_forced.is_some_and(|next| now < next) {
                return Err(BrokerError::InvalidToken);
            }
            cache.next_forced = Some(
                now.checked_add(self.limits.unknown_key_cooldown)
                    .ok_or(BrokerError::Limit)?,
            );
        }
        let keys = self.keys(&old.metadata, context).await?;
        let entry = Arc::new(Cached {
            metadata: old.metadata.clone(),
            keys,
            metadata_until: old.metadata_until,
            keys_until: Instant::now()
                .checked_add(self.limits.key_ttl)
                .ok_or(BrokerError::Limit)?,
        });
        context.check()?;
        self.cache
            .lock()
            .map_err(|_| BrokerError::InvalidPolicy)?
            .entry = Some(entry.clone());
        Ok(entry)
    }
}
enum SignedError {
    Reload,
    Denied(BrokerError),
}
struct NativeInput<'a> {
    id: Option<&'a str>,
    access: &'a str,
    nonce: Option<&'a str>,
    subject: Option<&'a str>,
}
#[derive(Clone, Copy)]
struct TokenKeys<'a> {
    access: &'a str,
    id: Option<&'a str>,
}

/// A verified native credential result. It is never an MCP caller or role proof.
/// ```compile_fail
/// use graphit_auth::broker::VerifiedNativeLogin;
/// fn copy<T: Clone>() {} copy::<VerifiedNativeLogin<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::broker::VerifiedNativeLogin;
/// fn print<T: std::fmt::Debug>() {} print::<VerifiedNativeLogin<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::broker::VerifiedNativeLogin;
/// let _: VerifiedNativeLogin<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_auth::broker::{AuthenticatedBrokerCaller, VerifiedNativeLogin};
/// fn caller<'a>(value: VerifiedNativeLogin<'a>) -> AuthenticatedBrokerCaller<'a> { value.into() }
/// ```
/// ```compile_fail
/// use graphit_auth::broker::VerifiedNativeLogin;
/// let proof = VerifiedNativeLogin { subject: "selected from body".into() };
/// ```
/// ```compile_fail
/// use graphit_auth::broker::VerifiedNativeLogin;
/// fn serialize<T: serde::Serialize>() {} serialize::<VerifiedNativeLogin<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::{LocalParent, broker::VerifiedNativeLogin};
/// fn parent<'a>(value: VerifiedNativeLogin<'a>) -> LocalParent<'a> { value.into() }
/// ```
/// ```compile_fail
/// use graphit_auth::{Permission, broker::VerifiedNativeLogin};
/// fn permission(value: VerifiedNativeLogin<'_>) -> Permission { value.into() }
/// ```
pub struct VerifiedNativeLogin<'a> {
    issuer: &'a str,
    provider_revision: u64,
    subject: String,
    username: Option<String>,
    teams: Option<Vec<String>>,
    organization: Option<String>,
    profile_claims: Option<VerifiedNativeProfileClaims>,
    expires_at: SystemTime,
    expires_mono: Instant,
    source: &'a BrokerVerifier,
    original: &'a SnapshotContext,
}
impl VerifiedNativeLogin<'_> {
    pub(crate) fn needs_service_refresh(
        &self,
        verifier: &BrokerVerifier,
        context: BrokerRequestContext<'_>,
        margin: Duration,
    ) -> Result<bool, BrokerError> {
        context.check()?;
        // This opaque proof has already passed the existing native signature
        // producer. Preserve its real binding before classifying its actual
        // lifetime; never reinterpret a signature/issuer/audience failure.
        if !std::ptr::eq(self.source, verifier)
            || self.provider_revision != verifier.revision
            || !self.original.same_request(context.original)
        {
            return Err(BrokerError::InvalidToken);
        }
        let now = verifier.clock.now();
        let Some(utc) = self.expires_at.duration_since(now.utc).ok() else {
            return Ok(true);
        };
        let Some(mono) = self.expires_mono.checked_duration_since(now.mono) else {
            return Ok(true);
        };
        Ok(utc.min(mono) <= margin)
    }
    pub fn issuer(&self) -> &str {
        self.issuer
    }
    pub fn provider_revision(&self) -> u64 {
        self.provider_revision
    }
    pub fn subject(&self) -> &str {
        &self.subject
    }
    /// Identity attributes from the verified access JWT, never a permission.
    /// Consumers must retain and check this proof's original lifetime.
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }
    pub fn teams(&self) -> &[String] {
        self.teams.as_deref().unwrap_or_default()
    }
    /// Optional attributes actually present in the signed access token.
    pub fn access_teams(&self) -> Option<&[String]> {
        self.teams.as_deref()
    }
    pub fn organization(&self) -> Option<&str> {
        self.organization.as_deref()
    }
    pub fn profile_claims(&self) -> Option<&VerifiedNativeProfileClaims> {
        self.profile_claims.as_ref()
    }
    pub fn expires_at(&self) -> SystemTime {
        self.expires_at
    }
    pub fn check(
        &self,
        verifier: &BrokerVerifier,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), BrokerError> {
        verifier.check_proof(
            self.source,
            self.original,
            self.provider_revision,
            self.expires_at,
            self.expires_mono,
            context,
        )
    }
}
fn checked_url(text: &str, max: usize) -> Result<Url, BrokerError> {
    if text.is_empty() || text.len() > max || text.bytes().any(|b| b <= b' ' || b == 127) {
        return Err(BrokerError::Limit);
    }
    let authority = text
        .split_once("://")
        .ok_or(BrokerError::InvalidPolicy)?
        .1
        .split(['/', '?', '#'])
        .next()
        .ok_or(BrokerError::InvalidPolicy)?;
    let url = Url::parse(text).map_err(|_| BrokerError::InvalidPolicy)?;
    if authority.contains('@')
        || !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(BrokerError::InvalidPolicy);
    }
    if url.as_str().len() > max {
        return Err(BrokerError::Limit);
    }
    Ok(url)
}
fn literal_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    }
}
fn required_string<'a>(
    value: &'a Value,
    key: &str,
    error: BrokerError,
) -> Result<&'a str, BrokerError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .ok_or(error)
}
fn string_list(value: &Value, key: &str, error: BrokerError) -> Result<Vec<String>, BrokerError> {
    let array = value.get(key).and_then(Value::as_array).ok_or(error)?;
    let mut seen = std::collections::BTreeSet::new();
    array
        .iter()
        .map(|v| {
            let s = v.as_str().filter(|s| !s.is_empty()).ok_or(error)?;
            if !seen.insert(s) {
                return Err(error);
            }
            Ok(s.into())
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests;

async fn deadline_reached(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
        None => std::future::pending().await,
    }
}
