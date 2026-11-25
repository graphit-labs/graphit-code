//! Native access-only Device authorization using the pinned OAuth2 protocol client.
use super::*;
use oauth2::{
    AuthType, DeviceAuthorizationUrl, EndpointNotSet, EndpointSet,
    StandardDeviceAuthorizationResponse, TokenResponse as _, TokenUrl,
    basic::{BasicClient, BasicTokenResponse, BasicTokenType},
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::oneshot;

type DeviceClient =
    BasicClient<EndpointNotSet, EndpointSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

/// Public instructions only. No device code or bearer credential is exposed.
pub struct DeviceInstructions {
    verification_uri: String,
    verification_uri_complete: Option<String>,
    user_code: String,
}
impl DeviceInstructions {
    pub fn verification_uri(&self) -> &str {
        &self.verification_uri
    }
    pub fn verification_uri_complete(&self) -> Option<&str> {
        self.verification_uri_complete.as_deref()
    }
    pub fn user_code(&self) -> &str {
        &self.user_code
    }
}

/// A single owned grant, bound to its service, Original and absolute grant deadline.
pub struct PendingDeviceLogin<'a> {
    source: &'a NativeOAuth<'a>,
    original: BrokerRequestContext<'a>,
    metadata: Arc<BrokerMetadata>,
    response: StandardDeviceAuthorizationResponse,
    instructions: DeviceInstructions,
    deadline: Instant,
}
impl PendingDeviceLogin<'_> {
    pub fn check(
        &self,
        service: &NativeOAuth<'_>,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), NativeError> {
        context.check()?;
        if !std::ptr::eq(self.source, service)
            || !self.original.original.same_request(context.original)
        {
            return Err(NativeError::local(NativeErrorKind::InvalidPolicy));
        }
        if Instant::now() >= self.deadline {
            return Err(NativeError::local(NativeErrorKind::DeviceExpired));
        }
        Ok(())
    }
    pub fn instructions(
        &self,
        context: BrokerRequestContext<'_>,
    ) -> Result<&DeviceInstructions, NativeError> {
        self.check(self.source, context)?;
        Ok(&self.instructions)
    }
}

/// Verified access-only credential data. It is neither serializable authority nor a permission.
pub struct FinalDeviceTokens<'a> {
    source: &'a NativeOAuth<'a>,
    tokens: BasicTokenResponse,
    identity: VerifiedNativeLogin<'a>,
    utc: SystemTime,
    mono: Instant,
    seconds: i64,
}
impl FinalDeviceTokens<'_> {
    pub fn check(
        &self,
        service: &NativeOAuth<'_>,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), NativeError> {
        if !std::ptr::eq(self.source, service) {
            return Err(NativeError::remote(NativeErrorKind::InvalidPolicy));
        }
        self.identity
            .check(service.client.verifier, context)
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        service
            .client
            .verifier
            .current(self.utc, self.mono, context)
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))
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
        Ok(self.tokens.access_token().secret())
    }
    /// auth.json timestamps use Unix seconds for this access-only body.
    pub fn expires_at(&self, context: BrokerRequestContext<'_>) -> Result<i64, NativeError> {
        self.check(self.source, context)?;
        Ok(self.seconds)
    }
}

impl<'a> NativeOAuth<'a> {
    pub async fn begin_device(
        &'a self,
        context: BrokerRequestContext<'a>,
    ) -> Result<PendingDeviceLogin<'a>, NativeError> {
        let mut gate = ProtocolNativeGate {
            source: self,
            original: context.original,
        };
        self.begin_device_guarded(context, &mut gate)
            .await
            .map_err(unguarded)
    }
    pub(crate) async fn begin_device_guarded<G: NativeFlowGate + Send>(
        &'a self,
        context: BrokerRequestContext<'a>,
        gate: &mut G,
    ) -> Result<PendingDeviceLogin<'a>, GuardedNativeError<G::Error>>
    where
        G::Error: Send,
    {
        context.check()?;
        if self.client.verifier.resource.is_some() {
            return Err(NativeError::local(NativeErrorKind::InvalidPolicy).into());
        }
        gate.before_metadata(self, context)
            .map_err(GuardedNativeError::Admission)?;
        let metadata = self.client.verifier.metadata(context).await?;
        let client = device_client(self, &metadata)?;
        let (sender, mut terminal) = oneshot::channel();
        let http = DeviceHttp::new(self, &metadata, context, gate, sender, Phase::Authorization);
        let anchor = Instant::now();
        let response: StandardDeviceAuthorizationResponse = {
            let request = client
                .exchange_device_code()
                .request_async::<_, oauth2::EmptyExtraDeviceAuthorizationFields>(&http);
            tokio::pin!(request);
            tokio::select! {
                biased;
                failure = &mut terminal => return Err(failure.unwrap_or_else(|_| NativeError::local(NativeErrorKind::InvalidResponse).into())),
                _ = context.original.cancellation().cancelled() => return Err(http.interrupted(BrokerError::Cancelled).into()),
                _ = super::super::deadline_reached(context.original.deadline()) => return Err(http.interrupted(BrokerError::Deadline).into()),
                result = &mut request => result.map_err(device_error)?,
            }
        };
        drop(http);
        validate_authorization(self, &response).map_err(NativeError::after_send)?;
        context
            .check()
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        encoded_bound(
            [
                (
                    "grant_type",
                    "urn:ietf:params:oauth:grant-type:device_code".len(),
                ),
                ("client_id", metadata.client_id().len()),
                ("device_code", response.device_code().secret().len()),
            ]
            .into_iter(),
            self.client.limits.form_body_bytes,
        )
        .map_err(NativeError::after_send)?;
        let deadline = anchor
            .checked_add(response.expires_in())
            .ok_or_else(|| NativeError::remote(NativeErrorKind::Limit))?;
        let instructions = DeviceInstructions {
            verification_uri: response.verification_uri().as_str().to_owned(),
            verification_uri_complete: response
                .verification_uri_complete()
                .map(|v| v.secret().to_owned()),
            user_code: response.user_code().secret().to_owned(),
        };
        let pending = PendingDeviceLogin {
            source: self,
            original: context,
            metadata,
            response,
            instructions,
            deadline,
        };
        pending
            .check(self, context)
            .map_err(NativeError::after_send)?;
        Ok(pending)
    }
    pub async fn receive_device(
        &'a self,
        pending: PendingDeviceLogin<'a>,
        context: BrokerRequestContext<'a>,
    ) -> Result<FinalDeviceTokens<'a>, NativeError> {
        let mut gate = ProtocolNativeGate {
            source: self,
            original: context.original,
        };
        self.receive_device_guarded(pending, context, &mut gate)
            .await
            .map_err(unguarded)
    }
    pub(crate) async fn receive_device_guarded<G: NativeFlowGate + Send>(
        &'a self,
        pending: PendingDeviceLogin<'a>,
        context: BrokerRequestContext<'a>,
        gate: &mut G,
    ) -> Result<FinalDeviceTokens<'a>, GuardedNativeError<G::Error>>
    where
        G::Error: Send,
    {
        pending.check(self, context)?;
        let client = device_client(self, &pending.metadata)?;
        let timeout = pending
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| NativeError::local(NativeErrorKind::DeviceExpired))?;
        let (sender, mut terminal) = oneshot::channel();
        let http = DeviceHttp::new(self, &pending.metadata, context, gate, sender, Phase::Token);
        // The SDK owns RFC8628 polling and slow_down. The terminal channel prevents its
        // generic HTTP-error retry from replaying an ambiguous token request.
        let tokens = {
            let request = client
                .exchange_device_access_token(&pending.response)
                .request_async(&http, tokio::time::sleep, Some(timeout));
            tokio::pin!(request);
            tokio::select! {
                biased;
                failure = &mut terminal => return Err(failure.unwrap_or_else(|_| NativeError::local(NativeErrorKind::InvalidResponse).into())),
                _ = context.original.cancellation().cancelled() => return Err(http.interrupted(BrokerError::Cancelled).into()),
                _ = super::super::deadline_reached(context.original.deadline()) => return Err(http.interrupted(BrokerError::Deadline).into()),
                _ = tokio::time::sleep_until(pending.deadline.into()) => return Err(http.interrupted_kind(NativeErrorKind::DeviceExpired).into()),
                result = &mut request => result.map_err(device_poll_error)?,
            }
        };
        if tokens.token_type() != &BasicTokenType::Bearer
            || tokens.refresh_token().is_some()
            || tokens
                .scopes()
                .is_some_and(|scopes| scopes.iter().any(|scope| !scope.as_str().trim().is_empty()))
        {
            return Err(NativeError::remote(NativeErrorKind::InvalidResponse).into());
        }
        self.client
            .token_input(tokens.access_token().secret())
            .map_err(NativeError::after_send)?;
        let lifetime = tokens
            .expires_in()
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| NativeError::remote(NativeErrorKind::InvalidResponse))?;
        let anchor = http
            .anchor
            .lock()
            .map_err(|_| NativeError::remote(NativeErrorKind::InvalidPolicy))?
            .take()
            .ok_or_else(|| NativeError::remote(NativeErrorKind::InvalidResponse))?;
        let identity = self
            .client
            .verifier
            .verify_device_login(tokens.access_token().secret(), context)
            .await
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        let (utc, mono, _) =
            token_expiry(lifetime.as_secs(), &identity, anchor).map_err(NativeError::after_send)?;
        let seconds = i64::try_from(
            utc.duration_since(UNIX_EPOCH)
                .map_err(|_| NativeError::remote(NativeErrorKind::Limit))?
                .as_secs(),
        )
        .map_err(|_| NativeError::remote(NativeErrorKind::Limit))?;
        let result = FinalDeviceTokens {
            source: self,
            tokens,
            identity,
            utc,
            mono,
            seconds,
        };
        result.check(self, context)?;
        Ok(result)
    }
}
fn unguarded(error: GuardedNativeError<NativeError>) -> NativeError {
    match error {
        GuardedNativeError::Native(e) | GuardedNativeError::Admission(e) => e,
    }
}
fn device_client(
    service: &NativeOAuth<'_>,
    metadata: &BrokerMetadata,
) -> Result<DeviceClient, NativeError> {
    let endpoint = metadata
        .device_authorization_endpoint()
        .ok_or_else(|| NativeError::local(NativeErrorKind::Unsupported))?;
    service.client.endpoint(endpoint)?;
    service.client.endpoint(metadata.token_endpoint())?;
    service.client.scalar(metadata.client_id())?;
    // Admit the known SDK form scalars before SDK allocation.
    encoded_bound(
        [("client_id", metadata.client_id().len())].into_iter(),
        service.client.limits.form_body_bytes,
    )?;
    Ok(
        BasicClient::new(ClientId::new(metadata.client_id().to_owned()))
            .set_auth_type(AuthType::RequestBody)
            .set_device_authorization_url(
                DeviceAuthorizationUrl::new(endpoint.to_owned())
                    .map_err(|_| NativeError::local(NativeErrorKind::InvalidPolicy))?,
            )
            .set_token_uri(
                TokenUrl::new(metadata.token_endpoint().to_owned())
                    .map_err(|_| NativeError::local(NativeErrorKind::InvalidPolicy))?,
            ),
    )
}
fn device_error<E: std::error::Error + 'static, T: oauth2::ErrorResponse>(
    error: oauth2::RequestTokenError<E, T>,
) -> NativeError {
    match error {
        oauth2::RequestTokenError::ServerResponse(_) => {
            NativeError::remote(NativeErrorKind::AuthorizationRejected)
        }
        _ => NativeError::remote(NativeErrorKind::InvalidResponse),
    }
}
#[derive(Clone, Copy)]
enum Phase {
    Authorization,
    Token,
}
#[derive(Debug)]
struct DeviceTransportError;
impl std::fmt::Display for DeviceTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native Device transport stopped")
    }
}
impl std::error::Error for DeviceTransportError {}
struct DeviceHttp<'s, 'a, G: NativeFlowGate> {
    service: &'a NativeOAuth<'a>,
    metadata: &'s BrokerMetadata,
    context: BrokerRequestContext<'a>,
    gate: Mutex<&'s mut G>,
    terminal: Mutex<Option<oneshot::Sender<GuardedNativeError<G::Error>>>>,
    anchor: Mutex<Option<Moment>>,
    sent: AtomicBool,
    phase: Phase,
}
impl<'s, 'a, G: NativeFlowGate> DeviceHttp<'s, 'a, G> {
    fn new(
        service: &'a NativeOAuth<'a>,
        metadata: &'s BrokerMetadata,
        context: BrokerRequestContext<'a>,
        gate: &'s mut G,
        terminal: oneshot::Sender<GuardedNativeError<G::Error>>,
        phase: Phase,
    ) -> Self {
        Self {
            service,
            metadata,
            context,
            gate: Mutex::new(gate),
            terminal: Mutex::new(Some(terminal)),
            anchor: Mutex::new(None),
            sent: AtomicBool::new(false),
            phase,
        }
    }
    fn interrupted(&self, error: BrokerError) -> NativeError {
        self.interrupted_kind(NativeErrorKind::Broker(error))
    }
    fn interrupted_kind(&self, kind: NativeErrorKind) -> NativeError {
        if self.sent.load(Ordering::Relaxed) {
            NativeError::remote(kind)
        } else {
            NativeError::local(kind)
        }
    }
    fn fail(&self, error: GuardedNativeError<G::Error>) -> DeviceTransportError {
        if let Ok(mut slot) = self.terminal.lock()
            && let Some(sender) = slot.take()
        {
            let _ = sender.send(error);
        }
        DeviceTransportError
    }
    async fn execute(
        &self,
        request: oauth2::HttpRequest,
    ) -> Result<oauth2::HttpResponse, GuardedNativeError<G::Error>> {
        self.context.check()?;
        let endpoint = match self.phase {
            Phase::Authorization => self
                .metadata
                .device_authorization_endpoint()
                .ok_or_else(|| NativeError::local(NativeErrorKind::Unsupported))?,
            Phase::Token => self.metadata.token_endpoint(),
        };
        if request.method() != Method::POST
            || request.uri() != endpoint
            || request.headers().contains_key(header::AUTHORIZATION)
        {
            return Err(NativeError::local(NativeErrorKind::InvalidPolicy).into());
        }
        let (parts, body) = request.into_parts();
        let body = Zeroizing::new(body);
        self.service
            .client
            .cap(body.len(), self.service.client.limits.form_body_bytes)?;
        self.gate
            .lock()
            .map_err(|_| NativeError::local(NativeErrorKind::InvalidPolicy))?
            .before_post(self.service, self.metadata, self.context)
            .map_err(GuardedNativeError::Admission)?;
        *self
            .anchor
            .lock()
            .map_err(|_| NativeError::local(NativeErrorKind::InvalidPolicy))? =
            Some(self.service.client.verifier.clock.now());
        self.sent.store(true, Ordering::Relaxed);
        let response = self
            .service
            .client
            .verifier
            .client
            .execute(
                NetworkRequest {
                    method: Method::POST,
                    url: endpoint,
                    headers: &parts.headers,
                    body: &body,
                },
                self.context.network(),
            )
            .await
            .map_err(|e| {
                if e == NetworkError::Busy {
                    NativeError::local(NativeErrorKind::Busy)
                } else {
                    NativeError::remote(NativeErrorKind::Broker(e.into()))
                }
            })?;
        self.context
            .check()
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        // Non-OAuth HTTP failures are terminal. The SDK interprets genuine 400
        // error responses (including pending/slow_down) without a synthetic body.
        if !matches!(response.status(), StatusCode::OK | StatusCode::BAD_REQUEST) {
            return Err(NativeError::remote(NativeErrorKind::HttpRejected).into());
        }
        let mut output = http::Response::new(response.body().to_vec());
        *output.status_mut() = response.status();
        *output.headers_mut() = response.headers().clone();
        Ok(output)
    }
}
type DeviceHttpFuture<'h> =
    Pin<Box<dyn Future<Output = Result<oauth2::HttpResponse, DeviceTransportError>> + Send + 'h>>;
impl<'c, 's, 'a, G> oauth2::AsyncHttpClient<'c> for DeviceHttp<'s, 'a, G>
where
    G: NativeFlowGate + Send,
    G::Error: Send,
{
    type Error = DeviceTransportError;
    type Future = DeviceHttpFuture<'c>;
    fn call(&'c self, request: oauth2::HttpRequest) -> Self::Future {
        // Only this call borrows the client for 'c. The implementation is not
        // conditional on the lifetimes of metadata, gate or Original storage.
        Box::pin(async move {
            self.execute(request)
                .await
                .map_err(|error| self.fail(error))
        })
    }
}
fn validate_authorization(
    service: &NativeOAuth<'_>,
    response: &StandardDeviceAuthorizationResponse,
) -> Result<(), NativeError> {
    let invalid = || NativeError::local(NativeErrorKind::InvalidResponse);
    service
        .client
        .token_input(response.device_code().secret())?;
    service.client.token_input(response.user_code().secret())?;
    if response.expires_in().is_zero() || response.interval().is_zero() {
        return Err(invalid());
    }
    Instant::now()
        .checked_add(response.expires_in())
        .ok_or_else(invalid)?;
    let uri = response.verification_uri().as_str();
    service.client.endpoint(uri)?;
    service
        .client
        .cap(uri.len(), service.client.limits.authorization_url_bytes)?;
    let parsed = Url::parse(uri).map_err(|_| invalid())?;
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(invalid());
    }
    if let Some(complete) = response.verification_uri_complete() {
        service.client.endpoint(complete.secret())?;
        service.client.cap(
            complete.secret().len(),
            service.client.limits.authorization_url_bytes,
        )?;
        let complete = Url::parse(complete.secret()).map_err(|_| invalid())?;
        if complete.path() != parsed.path()
            || complete.fragment().is_some()
            || complete.query_pairs().count() != 1
            || !complete
                .query_pairs()
                .any(|(k, v)| k == "user_code" && v == response.user_code().secret().as_str())
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn device_poll_error(
    error: oauth2::RequestTokenError<DeviceTransportError, oauth2::DeviceCodeErrorResponse>,
) -> NativeError {
    if matches!(&error,oauth2::RequestTokenError::ServerResponse(response) if matches!(response.error(),oauth2::DeviceCodeErrorResponseType::ExpiredToken))
    {
        NativeError::remote(NativeErrorKind::DeviceExpired)
    } else {
        device_error(error)
    }
}
