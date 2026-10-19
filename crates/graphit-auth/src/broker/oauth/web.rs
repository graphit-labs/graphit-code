#![doc = include_str!("../../../../../docs/specs/rust_web_oauth.md")]
use super::super::{AuthenticatedBrokerCaller, NativeInput, TokenPurpose};
use super::*;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};

const APPLICATION_TYPE_NATIVE: &str = "native";
const APPLICATION_TYPE_WEB: &str = "web";

/// A client registered at the captured issuer for one exact configured callback.
/// Its private fields are protocol data, not a permission grant.
pub struct RegisteredWebClient {
    issuer: String,
    revision: u64,
    client_id: String,
    redirect: String,
    resource: String,
}
impl RegisteredWebClient {
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
    pub fn callback_uri(&self) -> &str {
        &self.redirect
    }
    pub fn resource(&self) -> &str {
        &self.resource
    }
    fn check(
        &self,
        verifier: &BrokerVerifier,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), BrokerError> {
        context.check()?;
        if self.issuer != verifier.endpoint || self.revision != verifier.revision {
            return Err(BrokerError::InvalidPolicy);
        }
        Ok(())
    }
}
/// Public Web Authorization Code flow. Shares the native OAuth SDK/transport;
/// it does not activate a personal profile or construct Records authority.
pub struct WebOAuth<'a> {
    client: OAuthClient<'a>,
    callback: String,
    application_type: &'static str,
}
impl<'a> WebOAuth<'a> {
    pub fn new(
        verifier: &'a BrokerVerifier,
        limits: OAuthLimits,
        callback: &str,
        allow_loopback_http: bool,
    ) -> Result<Self, NativeError> {
        let limits = limits.validate()?;
        let url = checked_url(callback, limits.callback_bytes)?;
        if url.query().is_some()
            || url.path() == "/"
            || (url.scheme() != "https"
                && !(allow_loopback_http && url.scheme() == "http" && literal_loopback(&url)))
        {
            return Err(NativeError::local(NativeErrorKind::InvalidRedirect));
        }
        Ok(Self {
            client: OAuthClient { verifier, limits },
            callback: callback.to_owned(),
            application_type: if url.scheme() == "http" {
                APPLICATION_TYPE_NATIVE
            } else {
                APPLICATION_TYPE_WEB
            },
        })
    }
    pub async fn register(
        &self,
        context: BrokerRequestContext<'_>,
    ) -> Result<RegisteredWebClient, NativeError> {
        context.check()?;
        let verifier = self.client.verifier;
        let metadata = verifier.metadata(context).await?;
        let endpoint = metadata
            .registration_endpoint
            .as_deref()
            .ok_or_else(|| NativeError::local(NativeErrorKind::Unsupported))?;
        self.client.endpoint(endpoint)?;
        let body = serde_json::to_vec(&serde_json::json!({
            "redirect_uris": [&self.callback],
            "application_type": self.application_type,
            "token_endpoint_auth_method": "none",
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "client_name": "Graphit Code Web UI",
            "scope": metadata.scopes().join(" "),
        }))
        .map_err(|_| NativeError::local(NativeErrorKind::InvalidPolicy))?;
        self.client
            .cap(body.len(), self.client.limits.form_body_bytes)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let response = verifier
            .client
            .execute(
                NetworkRequest {
                    method: Method::POST,
                    url: endpoint,
                    headers: &headers,
                    body: &body,
                },
                context.network(),
            )
            .await
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e.into())))?;
        if response.status() != StatusCode::CREATED {
            return Err(NativeError::remote(NativeErrorKind::HttpRejected));
        }
        let value = super::super::json::parse(response.body(), verifier.limits.json, context)
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        let client_id =
            super::super::required_string(&value, "client_id", BrokerError::InvalidDocument)
                .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        // OIDC defaults an omitted application_type to web, never native.
        if value
            .get("application_type")
            .map_or(Some(APPLICATION_TYPE_WEB), |v| v.as_str())
            != Some(self.application_type)
            || value
                .get("token_endpoint_auth_method")
                .is_some_and(|v| v.as_str() != Some("none"))
            || value.get("redirect_uris").is_some_and(|v| {
                v.as_array().is_none_or(|uris| {
                    uris.len() != 1 || uris[0].as_str() != Some(self.callback.as_str())
                })
            })
        {
            return Err(NativeError::remote(NativeErrorKind::InvalidResponse));
        }
        self.client
            .scalar(client_id)
            .map_err(NativeError::after_send)?;
        let resource = verifier
            .discovery_url("/v1")
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        context
            .check()
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        Ok(RegisteredWebClient {
            issuer: verifier.endpoint.clone(),
            revision: verifier.revision,
            client_id: client_id.to_owned(),
            redirect: self.callback.clone(),
            resource,
        })
    }
    fn binding(
        &self,
        registered: &RegisteredWebClient,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), NativeError> {
        registered.check(self.client.verifier, context)?;
        if registered.redirect != self.callback {
            return Err(NativeError::local(NativeErrorKind::InvalidPolicy));
        }
        Ok(())
    }
    pub async fn begin<'r>(
        &'r self,
        registered: &'r RegisteredWebClient,
        context: BrokerRequestContext<'r>,
    ) -> Result<PendingWebLogin<'r>, NativeError> {
        self.binding(registered, context)?;
        let metadata = self.client.verifier.metadata(context).await?;
        self.client.endpoint(metadata.authorization_endpoint())?;
        self.client.endpoint(metadata.token_endpoint())?;
        let authorization = self.client.authorization(
            &metadata,
            registered.client_id(),
            &registered.redirect,
            Some(registered.resource()),
            context,
        )?;
        Ok(PendingWebLogin {
            source: self,
            original: context,
            registered,
            metadata,
            authorization,
        })
    }
    pub async fn exchange(
        &self,
        pending: PendingWebLogin<'_>,
        callback: &str,
        context: BrokerRequestContext<'_>,
    ) -> Result<WebCredentials, NativeError> {
        pending.check(self, context)?;
        let code = self.client.callback(
            &pending.registered.redirect,
            &pending.authorization.state,
            callback,
            context,
        )?;
        let form = self.client.form(
            &[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("client_id", pending.registered.client_id()),
                ("redirect_uri", &pending.registered.redirect),
                ("code_verifier", &pending.authorization.verifier),
                ("resource", pending.registered.resource()),
            ],
            context,
        )?;
        let anchor = self.client.verifier.clock.now();
        let response = self
            .client
            .post(pending.metadata.token_endpoint(), &form, context)
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
            .verify_pair(
                NativeInput {
                    id: Some(id),
                    access: &tokens.access,
                    nonce: Some(&pending.authorization.nonce),
                    subject: None,
                },
                (
                    TokenPurpose::WebId(pending.registered),
                    TokenPurpose::WebAccess(pending.registered),
                ),
                context,
            )
            .await
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        self.finish(tokens, identity, pending.registered, anchor, context)
            .map_err(NativeError::after_send)
    }
    pub async fn refresh(
        &self,
        registered: &RegisteredWebClient,
        old_refresh: &str,
        subject: &str,
        context: BrokerRequestContext<'_>,
    ) -> Result<WebCredentials, NativeError> {
        self.binding(registered, context)?;
        self.client.token_input(old_refresh)?;
        self.client.verifier.expected_input(subject)?;
        let metadata = self.client.verifier.metadata(context).await?;
        let scopes = metadata.scopes().join(" ");
        let form = self.client.form(
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", old_refresh),
                ("client_id", registered.client_id()),
                ("scope", &scopes),
                ("resource", registered.resource()),
            ],
            context,
        )?;
        let anchor = self.client.verifier.clock.now();
        let response = self
            .client
            .post(metadata.token_endpoint(), &form, context)
            .await?;
        let tokens = codec::parse(response.body(), self.client.limits, context)
            .map_err(NativeError::after_send)?;
        if tokens
            .refresh
            .as_deref()
            .is_none_or(|fresh| bool::from(fresh.as_bytes().ct_eq(old_refresh.as_bytes())))
        {
            return Err(NativeError::remote(NativeErrorKind::InvalidRotation));
        }
        let identity = self
            .client
            .verifier
            .verify_pair(
                NativeInput {
                    id: tokens.id.as_deref().map(String::as_str),
                    access: &tokens.access,
                    nonce: None,
                    subject: Some(subject),
                },
                (
                    TokenPurpose::WebId(registered),
                    TokenPurpose::WebAccess(registered),
                ),
                context,
            )
            .await
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        self.finish(tokens, identity, registered, anchor, context)
            .map_err(NativeError::after_send)
    }
    pub async fn revoke(
        &self,
        registered: &RegisteredWebClient,
        refresh: &str,
        context: BrokerRequestContext<'_>,
    ) -> Result<RevocationReceipt, NativeError> {
        self.binding(registered, context)?;
        self.client.token_input(refresh)?;
        let metadata = self.client.verifier.metadata(context).await?;
        let endpoint = metadata
            .revocation_endpoint()
            .ok_or_else(|| NativeError::local(NativeErrorKind::Unsupported))?;
        let form = self.client.form(
            &[
                ("token", refresh),
                ("token_type_hint", "refresh_token"),
                ("client_id", registered.client_id()),
            ],
            context,
        )?;
        self.client.post(endpoint, &form, context).await?;
        Ok(RevocationReceipt { _private: () })
    }
    pub async fn verify_access<'r>(
        &'r self,
        registered: &'r RegisteredWebClient,
        access: &'r str,
        context: BrokerRequestContext<'r>,
    ) -> Result<AuthenticatedWebCaller<'r>, BrokerError> {
        registered.check(self.client.verifier, context)?;
        if registered.redirect != self.callback {
            return Err(BrokerError::InvalidPolicy);
        }
        let caller = self
            .client
            .verifier
            .verify_access(access, TokenPurpose::WebAccess(registered), context)
            .await?;
        Ok(AuthenticatedWebCaller { caller, registered })
    }
    fn finish(
        &self,
        mut tokens: codec::TokenResponse,
        identity: VerifiedNativeLogin<'_>,
        registered: &RegisteredWebClient,
        anchor: Moment,
        context: BrokerRequestContext<'_>,
    ) -> Result<WebCredentials, NativeError> {
        identity.check(self.client.verifier, context)?;
        let (expires_at, expires_mono, _) = token_expiry(tokens.expires_in, &identity, anchor)?;
        tokens.id = None;
        Ok(WebCredentials {
            tokens,
            subject: identity.subject().to_owned(),
            profile: identity.profile_claims,
            issuer: registered.issuer.clone(),
            revision: registered.revision,
            client_id: registered.client_id.clone(),
            expires_at,
            expires_mono,
        })
    }
}
/// Consumed once by exchange; borrowed proofs never leave this flow's original.
pub struct PendingWebLogin<'a> {
    source: &'a WebOAuth<'a>,
    original: BrokerRequestContext<'a>,
    registered: &'a RegisteredWebClient,
    metadata: Arc<BrokerMetadata>,
    authorization: client::LoginAuthorization,
}
impl PendingWebLogin<'_> {
    pub fn authorization_url(
        &self,
        context: BrokerRequestContext<'_>,
    ) -> Result<&str, NativeError> {
        self.check(self.source, context)?;
        Ok(&self.authorization.url)
    }
    fn check(
        &self,
        source: &WebOAuth<'_>,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), NativeError> {
        source.binding(self.registered, context)?;
        if !std::ptr::eq(source, self.source)
            || !self.original.original.same_request(context.original)
        {
            return Err(NativeError::local(NativeErrorKind::InvalidCallback));
        }
        Ok(())
    }
}
/// Owned token data for a server session, not request authority. No ID token retained.
pub struct WebCredentials {
    tokens: codec::TokenResponse,
    subject: String,
    profile: Option<super::super::VerifiedNativeProfileClaims>,
    issuer: String,
    revision: u64,
    client_id: String,
    expires_at: SystemTime,
    expires_mono: Instant,
}
impl WebCredentials {
    pub fn subject(&self) -> &str {
        &self.subject
    }
    pub fn profile_claims(&self) -> Option<&super::super::VerifiedNativeProfileClaims> {
        self.profile.as_ref()
    }
    pub fn access_token(&self) -> &str {
        &self.tokens.access
    }
    pub fn refresh_token(&self) -> Option<&str> {
        self.tokens.refresh.as_deref().map(String::as_str)
    }
    pub fn expires_at(&self) -> SystemTime {
        self.expires_at
    }
    /// Monotonic deadline captured when these OAuth credentials were verified.
    pub fn expires_mono(&self) -> Instant {
        self.expires_mono
    }
    pub fn check(
        &self,
        flow: &WebOAuth<'_>,
        registered: &RegisteredWebClient,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), BrokerError> {
        registered.check(flow.client.verifier, context)?;
        if self.issuer != registered.issuer
            || self.revision != registered.revision
            || self.client_id != registered.client_id
            || registered.redirect != flow.callback
        {
            return Err(BrokerError::InvalidToken);
        }
        flow.client
            .verifier
            .current(self.expires_at, self.expires_mono, context)
    }
}
/// Locally verified Web access identity, distinct from MCP admission.
pub struct AuthenticatedWebCaller<'a> {
    caller: AuthenticatedBrokerCaller<'a>,
    registered: &'a RegisteredWebClient,
}
impl AuthenticatedWebCaller<'_> {
    pub fn subject(&self) -> &str {
        self.caller.subject()
    }
    pub fn username(&self) -> Option<&str> {
        self.caller.username()
    }
    pub fn teams(&self) -> &[String] {
        self.caller.teams()
    }
    pub fn issuer(&self) -> &str {
        self.caller.issuer()
    }
    pub fn provider_revision(&self) -> u64 {
        self.caller.provider_revision()
    }
    pub fn bearer_for_broker(&self) -> &str {
        self.caller.bearer_for_broker()
    }
    pub fn check(
        &self,
        verifier: &BrokerVerifier,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), BrokerError> {
        self.registered.check(verifier, context)?;
        self.caller.check(verifier, context)
    }
}
