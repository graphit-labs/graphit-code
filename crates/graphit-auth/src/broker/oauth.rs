//! Native OAuth protocol. Its outputs are verified data, never permission grants.
mod client;
mod codec;
pub mod device;
pub mod web;
use client::OAuthClient;

use super::{
    BrokerError, BrokerMetadata, BrokerRequestContext, BrokerVerifier, Moment, VerifiedNativeLogin,
    checked_url, literal_loopback,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_http::client::{NetworkError, NetworkRequest};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use oauth2::{
    AuthUrl, ClientId, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
};
use secrecy::zeroize::Zeroizing;
use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use url::{Url, form_urlencoded};

#[derive(Clone, Copy, Debug)]
pub struct OAuthLimits {
    pub authorization_url_bytes: usize,
    pub callback_bytes: usize,
    pub callback_query_entries: usize,
    pub form_body_bytes: usize,
    pub token_bytes: usize,
    pub response_nodes: usize,
    pub response_scalar_bytes: usize,
}
impl OAuthLimits {
    fn validate(self) -> Result<Self, NativeError> {
        if [
            self.authorization_url_bytes,
            self.callback_bytes,
            self.callback_query_entries,
            self.form_body_bytes,
            self.token_bytes,
            self.response_nodes,
            self.response_scalar_bytes,
        ]
        .into_iter()
        .any(|n| n == 0 || n > isize::MAX as usize)
        {
            return Err(NativeError::local(NativeErrorKind::InvalidPolicy));
        }
        Ok(self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteEffect {
    NotSent,
    MayHaveApplied,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeErrorKind {
    InvalidPolicy,
    InvalidRedirect,
    InvalidCallback,
    AuthorizationRejected,
    InvalidResponse,
    InvalidRotation,
    DeviceExpired,
    Unsupported,
    Entropy,
    Limit,
    Busy,
    HttpRejected,
    Broker(BrokerError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeError {
    kind: NativeErrorKind,
    effect: RemoteEffect,
}
impl NativeError {
    fn local(kind: NativeErrorKind) -> Self {
        Self {
            kind,
            effect: RemoteEffect::NotSent,
        }
    }
    fn remote(kind: NativeErrorKind) -> Self {
        Self {
            kind,
            effect: RemoteEffect::MayHaveApplied,
        }
    }
    pub fn kind(self) -> NativeErrorKind {
        self.kind
    }
    pub fn effect(self) -> RemoteEffect {
        self.effect
    }
    fn after_send(mut self) -> Self {
        self.effect = RemoteEffect::MayHaveApplied;
        self
    }
}
impl std::fmt::Display for NativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native OAuth: {:?} ({:?})", self.kind, self.effect)
    }
}
impl std::error::Error for NativeError {}
impl From<BrokerError> for NativeError {
    fn from(error: BrokerError) -> Self {
        Self::local(NativeErrorKind::Broker(error))
    }
}

fn post_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    let mut content = HeaderValue::from_static("application/x-www-form-urlencoded");
    content.set_sensitive(true);
    headers.insert(header::CONTENT_TYPE, content);
    headers
}

/// A mandatory internal gate. It conveys no caller or publication authority.
pub(crate) trait NativeFlowGate {
    type Error;
    fn before_metadata(
        &mut self,
        service: &NativeOAuth<'_>,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), Self::Error>;
    fn before_post(
        &mut self,
        service: &NativeOAuth<'_>,
        metadata: &BrokerMetadata,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), Self::Error>;
}
pub(crate) enum GuardedNativeError<E> {
    Native(NativeError),
    Admission(E),
}
impl<E> From<NativeError> for GuardedNativeError<E> {
    fn from(error: NativeError) -> Self {
        Self::Native(error)
    }
}
impl<E> From<BrokerError> for GuardedNativeError<E> {
    fn from(error: BrokerError) -> Self {
        Self::Native(error.into())
    }
}
struct ProtocolNativeGate<'a> {
    source: &'a NativeOAuth<'a>,
    original: &'a graphit_core::file_io::SnapshotContext,
}
impl NativeFlowGate for ProtocolNativeGate<'_> {
    type Error = NativeError;
    fn before_metadata(
        &mut self,
        service: &NativeOAuth<'_>,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), Self::Error> {
        context.check()?;
        if !std::ptr::eq(self.source, service) || !self.original.same_request(context.original) {
            return Err(NativeError::local(NativeErrorKind::InvalidPolicy));
        }
        Ok(())
    }
    fn before_post(
        &mut self,
        service: &NativeOAuth<'_>,
        _metadata: &BrokerMetadata,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), Self::Error> {
        self.before_metadata(service, context)
    }
}

/// Native loopback OAuth, sharing the Web protocol client and functional limits.
pub struct NativeOAuth<'a> {
    client: OAuthClient<'a>,
}
impl<'a> NativeOAuth<'a> {
    pub fn new(verifier: &'a BrokerVerifier, limits: OAuthLimits) -> Result<Self, NativeError> {
        let limits = limits.validate()?;
        Ok(Self {
            client: OAuthClient { verifier, limits },
        })
    }
    pub async fn begin(
        &'a self,
        redirect: &str,
        context: BrokerRequestContext<'a>,
    ) -> Result<PendingLogin<'a>, NativeError> {
        context.check()?;
        let parsed = self.client.redirect(redirect)?;
        let metadata = self.client.verifier.metadata(context).await?;
        self.client.endpoint(metadata.authorization_endpoint())?;
        self.client.endpoint(metadata.token_endpoint())?;
        if let Some(endpoint) = metadata.revocation_endpoint() {
            self.client.endpoint(endpoint)?;
        }
        let uri: http::Uri = redirect
            .parse()
            .map_err(|_| NativeError::local(NativeErrorKind::InvalidRedirect))?;
        if uri.path() != metadata.redirect_path() || parsed.path() != metadata.redirect_path() {
            return Err(NativeError::local(NativeErrorKind::InvalidRedirect));
        }
        let authorization = self.client.authorization(
            &metadata,
            metadata.client_id(),
            redirect,
            self.client.verifier.resource.as_deref(),
            context,
        )?;
        Ok(PendingLogin {
            source: self,
            original: context,
            revision: self.client.verifier.revision,
            metadata,
            redirect: redirect.into(),
            authorization: authorization.url,
            state: authorization.state,
            nonce: authorization.nonce,
            verifier: authorization.verifier,
        })
    }
    pub async fn exchange(
        &'a self,
        pending: PendingLogin<'a>,
        callback: &str,
        context: BrokerRequestContext<'a>,
    ) -> Result<FinalNativeTokens<'a>, NativeError> {
        let mut gate = ProtocolNativeGate {
            source: self,
            original: context.original,
        };
        self.exchange_guarded(pending, callback, context, &mut gate)
            .await
            .map_err(|error| match error {
                GuardedNativeError::Native(error) | GuardedNativeError::Admission(error) => error,
            })
    }
    pub(crate) async fn exchange_guarded<G: NativeFlowGate>(
        &'a self,
        pending: PendingLogin<'a>,
        callback: &str,
        context: BrokerRequestContext<'a>,
        gate: &mut G,
    ) -> Result<FinalNativeTokens<'a>, GuardedNativeError<G::Error>> {
        pending.check(self, context)?;
        let code = self
            .client
            .callback(&pending.redirect, &pending.state, callback, context)?;
        self.client.endpoint(pending.metadata.token_endpoint())?;
        let mut fields = vec![
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("client_id", pending.metadata.client_id()),
            ("redirect_uri", pending.redirect.as_str()),
            ("code_verifier", pending.verifier.as_str()),
        ];
        if let Some(resource) = &self.client.verifier.resource {
            fields.push(("resource", resource));
        }
        let form = self.client.form(&fields, context)?;
        let anchor = self.client.verifier.clock.now();
        let headers = post_headers();
        gate.before_post(self, &pending.metadata, context)
            .map_err(GuardedNativeError::Admission)?;
        let response = self
            .client
            .execute_post(
                pending.metadata.token_endpoint(),
                &form,
                &headers,
                StatusCode::OK,
                context,
            )
            .await?;
        let tokens = codec::parse(response.body(), self.client.limits, context)
            .map_err(NativeError::after_send)?;
        let id = tokens
            .id
            .as_deref()
            .ok_or_else(|| NativeError::remote(NativeErrorKind::InvalidResponse))?;
        let identity = self
            .client
            .verifier
            .verify_login(id, &tokens.access, &pending.nonce, context)
            .await
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        self.finish(tokens, identity, anchor, context)
            .map_err(NativeError::after_send)
            .map_err(GuardedNativeError::Native)
    }
    pub async fn refresh(
        &'a self,
        old_refresh: &str,
        expected_subject: &str,
        context: BrokerRequestContext<'a>,
    ) -> Result<FinalNativeTokens<'a>, NativeError> {
        let mut gate = ProtocolNativeGate {
            source: self,
            original: context.original,
        };
        self.refresh_guarded(old_refresh, expected_subject, context, &mut gate)
            .await
            .map_err(|error| match error {
                GuardedNativeError::Native(error) | GuardedNativeError::Admission(error) => error,
            })
    }
    pub(crate) fn matches_request(
        &self,
        original: &graphit_core::file_io::SnapshotContext,
        context: BrokerRequestContext<'_>,
    ) -> bool {
        original.same_request(context.original)
    }
    pub(crate) fn matches_provider(
        &self,
        endpoint: &str,
        resource: Option<&str>,
        revision: u64,
    ) -> bool {
        self.client.verifier.endpoint == endpoint
            && self.client.verifier.resource.as_deref() == resource
            && self.client.verifier.revision == revision
    }
    pub(crate) async fn refresh_guarded<G: NativeFlowGate>(
        &'a self,
        old_refresh: &str,
        expected_subject: &str,
        context: BrokerRequestContext<'a>,
        gate: &mut G,
    ) -> Result<FinalNativeTokens<'a>, GuardedNativeError<G::Error>> {
        context.check()?;
        self.client.token_input(old_refresh)?;
        self.client.scalar(expected_subject)?;
        self.client.verifier.expected_input(expected_subject)?;
        if expected_subject.is_empty() {
            return Err(NativeError::local(NativeErrorKind::InvalidPolicy).into());
        }
        gate.before_metadata(self, context)
            .map_err(GuardedNativeError::Admission)?;
        let metadata = self.client.verifier.metadata(context).await?;
        self.client.endpoint(metadata.token_endpoint())?;
        let scope_len = self.client.scope_length(&metadata)?;
        // Admit the eventual joined scope before its allocation.
        self.client.scalar_len(scope_len)?;
        let scope = metadata.scopes().join(" ");
        let mut fields = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", old_refresh),
            ("client_id", metadata.client_id()),
            ("scope", scope.as_str()),
        ];
        if let Some(resource) = &self.client.verifier.resource {
            fields.push(("resource", resource));
        }
        let form = self.client.form(&fields, context)?;
        let anchor = self.client.verifier.clock.now();
        let headers = post_headers();
        gate.before_post(self, &metadata, context)
            .map_err(GuardedNativeError::Admission)?;
        let response = self
            .client
            .execute_post(
                metadata.token_endpoint(),
                &form,
                &headers,
                StatusCode::OK,
                context,
            )
            .await?;
        let tokens = codec::parse(response.body(), self.client.limits, context)
            .map_err(NativeError::after_send)?;
        let fresh = tokens
            .refresh
            .as_deref()
            .ok_or_else(|| NativeError::remote(NativeErrorKind::InvalidRotation))?;
        if bool::from(fresh.as_bytes().ct_eq(old_refresh.as_bytes())) {
            return Err(NativeError::remote(NativeErrorKind::InvalidRotation).into());
        }
        let identity = self
            .client
            .verifier
            .verify_refresh(
                &tokens.access,
                tokens.id.as_deref().map(String::as_str),
                expected_subject,
                context,
            )
            .await
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        self.finish(tokens, identity, anchor, context)
            .map_err(NativeError::after_send)
            .map_err(GuardedNativeError::Native)
    }
    pub async fn revoke(
        &self,
        refresh: &str,
        context: BrokerRequestContext<'_>,
    ) -> Result<RevocationReceipt, NativeError> {
        context.check()?;
        self.client.token_input(refresh)?;
        let metadata = self.client.verifier.metadata(context).await?;
        let endpoint = metadata
            .revocation_endpoint()
            .ok_or_else(|| NativeError::local(NativeErrorKind::Unsupported))?;
        self.client.endpoint(endpoint)?;
        let form = self.client.form(
            &[
                ("token", refresh),
                ("token_type_hint", "refresh_token"),
                ("client_id", metadata.client_id()),
            ],
            context,
        )?;
        self.client.post(endpoint, &form, context).await?;
        context
            .check()
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        Ok(RevocationReceipt { _private: () })
    }
    fn finish(
        &'a self,
        tokens: codec::TokenResponse,
        identity: VerifiedNativeLogin<'a>,
        anchor: Moment,
        context: BrokerRequestContext<'a>,
    ) -> Result<FinalNativeTokens<'a>, NativeError> {
        let (utc, mono, micros) = token_expiry(tokens.expires_in, &identity, anchor)?;
        let result = FinalNativeTokens {
            source: self,
            tokens,
            identity,
            utc,
            mono,
            micros,
        };
        result.check(self, context)?;
        Ok(result)
    }
}
fn token_expiry(
    expires_in: u64,
    identity: &VerifiedNativeLogin<'_>,
    anchor: Moment,
) -> Result<(SystemTime, Instant, i64), NativeError> {
    let lifetime = Duration::from_secs(expires_in);
    let response_utc = anchor
        .utc
        .checked_add(lifetime)
        .ok_or_else(|| NativeError::local(NativeErrorKind::Limit))?;
    let response_mono = anchor
        .mono
        .checked_add(lifetime)
        .ok_or_else(|| NativeError::local(NativeErrorKind::Limit))?;
    let jwt_remaining = identity
        .expires_at
        .duration_since(anchor.utc)
        .map_err(|_| NativeError::local(NativeErrorKind::InvalidResponse))?;
    let anchored_jwt_mono = anchor
        .mono
        .checked_add(jwt_remaining)
        .ok_or_else(|| NativeError::local(NativeErrorKind::Limit))?;
    let utc = response_utc.min(identity.expires_at);
    let mono = response_mono
        .min(identity.expires_mono)
        .min(anchored_jwt_mono);
    let micros = i64::try_from(
        utc.duration_since(UNIX_EPOCH)
            .map_err(|_| NativeError::local(NativeErrorKind::Limit))?
            .as_micros(),
    )
    .map_err(|_| NativeError::local(NativeErrorKind::Limit))?;
    Ok((utc, mono, micros))
}
fn encoded_bound<'a>(
    fields: impl Iterator<Item = (&'a str, usize)>,
    cap: usize,
) -> Result<usize, NativeError> {
    let mut total = 0usize;
    for (index, (key, value_len)) in fields.enumerate() {
        total = value_len
            .checked_mul(3)
            .and_then(|n| n.checked_add(key.len()))
            .and_then(|n| n.checked_add(1 + usize::from(index != 0)))
            .and_then(|n| total.checked_add(n))
            .ok_or_else(|| NativeError::local(NativeErrorKind::Limit))?;
        if total > cap {
            return Err(NativeError::local(NativeErrorKind::Limit));
        }
    }
    Ok(total)
}
fn random_secret() -> Result<Zeroizing<String>, NativeError> {
    let mut bytes = Zeroizing::new([0u8; 32]);
    getrandom::fill(bytes.as_mut()).map_err(|_| NativeError::local(NativeErrorKind::Entropy))?;
    Ok(Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes.as_ref())))
}

/// Single-use login attempt. Drop erases owned secrets.
/// ```compile_fail
/// use graphit_auth::broker::oauth::PendingLogin;
/// fn clone(value: PendingLogin<'_>) { let _ = value.clone(); }
/// ```
/// ```compile_fail
/// use graphit_auth::broker::oauth::PendingLogin;
/// fn debug(value: PendingLogin<'_>) { let _ = format!("{value:?}"); }
/// ```
/// ```compile_fail
/// use graphit_auth::broker::oauth::PendingLogin;
/// let _: PendingLogin<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_auth::broker::oauth::PendingLogin;
/// let pending = PendingLogin { state: "forged".into() };
/// ```
/// ```compile_fail
/// use graphit_auth::broker::oauth::PendingLogin;
/// fn serialize<T: serde::Serialize>() {} serialize::<PendingLogin<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::broker::oauth::PendingLogin;
/// fn twice(value: PendingLogin<'_>) { drop(value); drop(value); }
/// ```
pub struct PendingLogin<'a> {
    source: &'a NativeOAuth<'a>,
    original: BrokerRequestContext<'a>,
    revision: u64,
    metadata: Arc<BrokerMetadata>,
    redirect: String,
    authorization: Zeroizing<String>,
    state: Zeroizing<String>,
    nonce: Zeroizing<String>,
    verifier: Zeroizing<String>,
}
impl PendingLogin<'_> {
    pub fn check(
        &self,
        source: &NativeOAuth<'_>,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), NativeError> {
        context.check()?;
        if !std::ptr::eq(self.source, source)
            || self.revision != source.client.verifier.revision
            || !self.original.original.same_request(context.original)
        {
            return Err(NativeError::local(NativeErrorKind::InvalidCallback));
        }
        Ok(())
    }
    pub fn authorization_url(
        &self,
        context: BrokerRequestContext<'_>,
    ) -> Result<&str, NativeError> {
        self.check(self.source, context)?;
        Ok(&self.authorization)
    }
}

/// Verified credential data. It carries no caller, permission, grant or role.
/// ```compile_fail
/// use graphit_auth::broker::oauth::FinalNativeTokens;
/// fn clone(value: FinalNativeTokens<'_>) { let _ = value.clone(); }
/// ```
/// ```compile_fail
/// use graphit_auth::broker::oauth::FinalNativeTokens;
/// fn debug(value: FinalNativeTokens<'_>) { let _ = format!("{value:?}"); }
/// ```
/// ```compile_fail
/// use graphit_auth::broker::oauth::FinalNativeTokens;
/// let _: FinalNativeTokens<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_auth::broker::oauth::FinalNativeTokens;
/// fn serialize<T: serde::Serialize>() {} serialize::<FinalNativeTokens<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::{Permission, broker::oauth::FinalNativeTokens};
/// fn permission(value: FinalNativeTokens<'_>) -> Permission { value.into() }
/// ```
/// ```compile_fail
/// use graphit_auth::{LocalParent, broker::oauth::FinalNativeTokens};
/// fn parent<'a>(value: FinalNativeTokens<'a>) -> LocalParent<'a> { value.into() }
/// ```
/// ```compile_fail
/// use graphit_auth::broker::oauth::FinalNativeTokens;
/// let tokens = FinalNativeTokens { utc: std::time::SystemTime::now() };
/// ```
/// ```compile_fail
/// use graphit_auth::broker::{AuthenticatedBrokerCaller, oauth::FinalNativeTokens};
/// fn caller<'a>(value: FinalNativeTokens<'a>) -> AuthenticatedBrokerCaller<'a> { value.into() }
/// ```
pub struct FinalNativeTokens<'a> {
    source: &'a NativeOAuth<'a>,
    tokens: codec::TokenResponse,
    identity: VerifiedNativeLogin<'a>,
    utc: SystemTime,
    mono: Instant,
    micros: i64,
}
impl FinalNativeTokens<'_> {
    pub fn check(
        &self,
        source: &NativeOAuth<'_>,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), NativeError> {
        if !std::ptr::eq(self.source, source) {
            return Err(NativeError::remote(NativeErrorKind::InvalidPolicy));
        }
        self.identity
            .check(source.client.verifier, context)
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        source
            .client
            .verifier
            .current(self.utc, self.mono, context)
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        Ok(())
    }
    pub fn identity(
        &self,
        context: BrokerRequestContext<'_>,
    ) -> Result<&VerifiedNativeLogin<'_>, NativeError> {
        self.check(self.source, context)?;
        Ok(&self.identity)
    }
    pub fn access_token(&self, context: BrokerRequestContext<'_>) -> Result<&str, NativeError> {
        self.check(self.source, context)?;
        Ok(&self.tokens.access)
    }
    pub fn refresh_token(
        &self,
        context: BrokerRequestContext<'_>,
    ) -> Result<Option<&str>, NativeError> {
        self.check(self.source, context)?;
        Ok(self.tokens.refresh.as_deref().map(String::as_str))
    }
    pub fn id_token(&self, context: BrokerRequestContext<'_>) -> Result<Option<&str>, NativeError> {
        self.check(self.source, context)?;
        Ok(self.tokens.id.as_deref().map(String::as_str))
    }
    pub fn scope(&self, context: BrokerRequestContext<'_>) -> Result<Option<&str>, NativeError> {
        self.check(self.source, context)?;
        Ok(self.tokens.scope.as_deref().map(String::as_str))
    }
    pub fn expires_at_micros(&self, context: BrokerRequestContext<'_>) -> Result<i64, NativeError> {
        self.check(self.source, context)?;
        Ok(self.micros)
    }
}
/// Only receipt of HTTP 200 for this call; no assertion about other token copies.
pub struct RevocationReceipt {
    _private: (),
}

#[cfg(test)]
pub(crate) mod tests;
