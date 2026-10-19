//! Bounded same-origin transport. Identity and authorization belong to its callers.

use graphit_core::file_lock::Cancellation;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use reqwest::{Client, Url};
use secrecy::zeroize::Zeroizing;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkError {
    InvalidPolicy,
    InvalidRequest,
    Origin,
    Busy,
    Cancelled,
    Deadline,
    /// A timeout, SDK-reported closed/incomplete message, or concrete connection I/O failure.
    /// TLS verification and HTTP parsing failures remain Transport.
    Connection,
    Transport,
    Limit,
    Encoding,
}
impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for NetworkError {}

pub(crate) fn request_error(error: &reqwest::Error) -> NetworkError {
    let mut connection = error.is_timeout();
    let mut cause = std::error::Error::source(error);
    while let Some(current) = cause {
        if let Some(error) = current.downcast_ref::<hyper::Error>() {
            // HTTP parsing failures remain terminal even if another wrapper
            // carries a connection category. EOF before headers complete is a
            // distinct SDK condition, not necessarily a nested std::io::Error.
            if error.is_parse() {
                return NetworkError::Transport;
            }
            connection |= error.is_incomplete_message() || error.is_closed() || error.is_timeout();
        }
        if let Some(error) = current.downcast_ref::<std::io::Error>() {
            connection |= connection_failure(error.kind());
        }
        cause = current.source();
    }
    if connection {
        NetworkError::Connection
    } else {
        NetworkError::Transport
    }
}
pub(crate) fn connection_failure(kind: std::io::ErrorKind) -> bool {
    use std::io::ErrorKind;
    matches!(
        kind,
        ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
            | ErrorKind::NotConnected
            | ErrorKind::UnexpectedEof
            | ErrorKind::TimedOut
            | ErrorKind::NetworkUnreachable
            | ErrorKind::HostUnreachable
    )
}

/// Whether this request reached the native send boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkEffect {
    NotSent,
    /// The remote effect is possible, not known to have occurred.
    MayHaveApplied,
}

/// Typed failures with diagnostics that do not format caller admission errors.
pub enum GuardedNetworkError<E> {
    Network {
        source: NetworkError,
        effect: NetworkEffect,
    },
    Admission {
        source: E,
        effect: NetworkEffect,
    },
}
impl<E> GuardedNetworkError<E> {
    pub fn effect(&self) -> NetworkEffect {
        match self {
            Self::Network { effect, .. } | Self::Admission { effect, .. } => *effect,
        }
    }
}
impl<E> std::fmt::Debug for GuardedNetworkError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network { source, effect } => f
                .debug_struct("Network")
                .field("source", source)
                .field("effect", effect)
                .finish(),
            Self::Admission { effect, .. } => f
                .debug_struct("Admission")
                .field("effect", effect)
                .finish_non_exhaustive(),
        }
    }
}
impl<E> std::fmt::Display for GuardedNetworkError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network { source, effect } => write!(f, "HTTP {source} ({effect:?})"),
            Self::Admission { effect, .. } => write!(f, "HTTP admission rejected ({effect:?})"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for GuardedNetworkError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Network { source, .. } => Some(source),
            Self::Admission { source, .. } => Some(source),
        }
    }
}

/// A borrowed destination for one finite response. The caller owns any staging
/// bytes and must discard them when the operation fails or its future is dropped.
pub trait NetworkResponseSink {
    type Error;

    /// Decide status and bounded metadata before the first body callback.
    fn head(&mut self, status: StatusCode, headers: &HeaderMap) -> Result<(), Self::Error>;
    fn chunk(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}

/// Completion metadata; this value does not publish the caller's destination.
pub struct NetworkStreamResponse {
    status: StatusCode,
    accepted_bytes: usize,
}
impl NetworkStreamResponse {
    pub fn status(&self) -> StatusCode {
        self.status
    }
    pub fn accepted_bytes(&self) -> usize {
        self.accepted_bytes
    }
}

/// Accepted bytes count only chunk callbacks that returned `Ok`. A failed
/// callback may already have written part of its chunk; this is not a commit count.
pub enum NetworkStreamError<E, S> {
    Network {
        source: NetworkError,
        effect: NetworkEffect,
        accepted_bytes: usize,
    },
    Admission {
        source: E,
        effect: NetworkEffect,
        accepted_bytes: usize,
    },
    Sink {
        source: S,
        effect: NetworkEffect,
        accepted_bytes: usize,
    },
}
impl<E, S> NetworkStreamError<E, S> {
    pub fn effect(&self) -> NetworkEffect {
        match self {
            Self::Network { effect, .. }
            | Self::Admission { effect, .. }
            | Self::Sink { effect, .. } => *effect,
        }
    }
    pub fn accepted_bytes(&self) -> usize {
        match self {
            Self::Network { accepted_bytes, .. }
            | Self::Admission { accepted_bytes, .. }
            | Self::Sink { accepted_bytes, .. } => *accepted_bytes,
        }
    }
}
impl<E, S> std::fmt::Debug for NetworkStreamError<E, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Network { .. } => "Network",
            Self::Admission { .. } => "Admission",
            Self::Sink { .. } => "Sink",
        };
        let mut record = f.debug_struct(name);
        if let Self::Network { source, .. } = self {
            record.field("source", source);
        }
        record
            .field("effect", &self.effect())
            .field("accepted_bytes", &self.accepted_bytes())
            .finish_non_exhaustive()
    }
}
impl<E, S> std::fmt::Display for NetworkStreamError<E, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network { source, .. } => write!(f, "HTTP {source}"),
            Self::Admission { .. } => f.write_str("HTTP admission rejected"),
            Self::Sink { .. } => f.write_str("HTTP response sink rejected"),
        }?;
        write!(
            f,
            " ({:?}, accepted={})",
            self.effect(),
            self.accepted_bytes()
        )
    }
}
impl<E: std::error::Error + 'static, S: std::error::Error + 'static> std::error::Error
    for NetworkStreamError<E, S>
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Network { source, .. } => Some(source),
            Self::Admission { source, .. } => Some(source),
            Self::Sink { source, .. } => Some(source),
        }
    }
}

// Effects are attached once the finite operation has finished or been cancelled.
enum RequestFailure<E, S> {
    Network(NetworkError),
    Admission(E),
    Sink(S),
}
impl<E, S> From<NetworkError> for RequestFailure<E, S> {
    fn from(source: NetworkError) -> Self {
        Self::Network(source)
    }
}

/// Logical limits for Graphit's retained data, independent of vendor parser buffers.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub request_body_bytes: usize,
    pub response_body_bytes: usize,
    pub url_bytes: usize,
    pub header_bytes: usize,
    pub header_count: usize,
    pub max_in_flight: usize,
    pub max_idle_per_host: usize,
    pub idle_timeout: Duration,
    pub max_certificates: usize,
    pub max_certificate_bytes: usize,
}
impl Limits {
    fn validate(self) -> Result<Self, NetworkError> {
        for limit in [
            self.request_body_bytes,
            self.response_body_bytes,
            self.url_bytes,
            self.header_bytes,
            self.header_count,
            self.max_in_flight,
            self.max_idle_per_host,
            self.max_certificates,
            self.max_certificate_bytes,
        ] {
            if limit == 0 || limit.checked_add(1).is_none() || limit > isize::MAX as usize {
                return Err(NetworkError::InvalidPolicy);
            }
        }
        if self.max_in_flight > Semaphore::MAX_PERMITS
            || u32::try_from(self.header_count).is_err()
            || u32::try_from(self.header_bytes).is_err()
            || self.idle_timeout.is_zero()
            || Instant::now().checked_add(self.idle_timeout).is_none()
        {
            return Err(NetworkError::InvalidPolicy);
        }
        Ok(self)
    }
}

/// Construction consumes only explicitly captured policy; it reads no proxy environment.
pub struct ClientPolicy<'a> {
    pub origin: &'a str,
    pub allow_loopback_http: bool,
    pub proxy: Option<&'a str>,
    pub additional_roots: &'a [&'a [u8]],
}

/// Supply the original call's absolute deadline and shared cancellation signal.
#[derive(Clone, Copy)]
pub struct NetworkRequestContext<'a> {
    pub(crate) deadline: Option<Instant>,
    pub(crate) cancellation: &'a Cancellation,
}
impl<'a> NetworkRequestContext<'a> {
    pub fn new(deadline: Instant, cancellation: &'a Cancellation) -> Self {
        Self::with_deadline(Some(deadline), cancellation)
    }
    pub fn service(cancellation: &'a Cancellation) -> Self {
        Self::with_deadline(None, cancellation)
    }
    pub fn with_deadline(deadline: Option<Instant>, cancellation: &'a Cancellation) -> Self {
        Self {
            deadline,
            cancellation,
        }
    }
    pub(crate) async fn deadline_reached(self) {
        match self.deadline {
            Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
            None => std::future::pending().await,
        }
    }
    pub(crate) fn check(self) -> Result<(), NetworkError> {
        if self.cancellation.is_cancelled() {
            Err(NetworkError::Cancelled)
        } else if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Err(NetworkError::Deadline)
        } else {
            Ok(())
        }
    }
}

/// Borrowed request inputs have no diagnostic or serialization representation.
pub struct NetworkRequest<'a> {
    pub method: Method,
    pub url: &'a str,
    pub headers: &'a HeaderMap,
    pub body: &'a [u8],
}

/// Response bytes are erased on drop; status and headers require explicit access.
pub struct NetworkResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Zeroizing<Vec<u8>>,
}
impl NetworkResponse {
    pub fn status(&self) -> StatusCode {
        self.status
    }
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    pub fn into_body(self) -> Zeroizing<Vec<u8>> {
        self.body
    }
}

struct BufferedSink {
    headers: HeaderMap,
    body: Zeroizing<Vec<u8>>,
    limit: usize,
}
impl NetworkResponseSink for BufferedSink {
    type Error = NetworkError;

    fn head(&mut self, _: StatusCode, headers: &HeaderMap) -> Result<(), NetworkError> {
        self.headers = headers.clone();
        Ok(())
    }
    fn chunk(&mut self, bytes: &[u8]) -> Result<(), NetworkError> {
        let length = self
            .body
            .len()
            .checked_add(bytes.len())
            .ok_or(NetworkError::Limit)?;
        if length > self.limit {
            return Err(NetworkError::Limit);
        }
        if length > self.body.capacity() {
            let capacity = length
                .max(self.body.capacity().saturating_mul(2))
                .min(self.limit);
            let additional = capacity - self.body.len();
            self.body
                .try_reserve_exact(additional)
                .map_err(|_| NetworkError::Limit)?;
        }
        self.body.extend_from_slice(bytes);
        Ok(())
    }
}

pub struct BoundedClient {
    client: Client,
    origin: Url,
    limits: Limits,
    permits: Arc<Semaphore>,
    #[cfg(test)]
    before_send: Option<Box<dyn Fn() + Send + Sync>>,
}
pub(crate) fn build_client(
    policy: ClientPolicy<'_>,
    limits: Limits,
    http1_only: bool,
) -> Result<(Client, Url, Limits), NetworkError> {
    let limits = limits.validate()?;
    let origin =
        parse_url(policy.origin, limits.url_bytes).map_err(|_| NetworkError::InvalidPolicy)?;
    if !transport_allowed(&origin, policy.allow_loopback_http) {
        return Err(NetworkError::InvalidPolicy);
    }
    if policy.additional_roots.len() > limits.max_certificates {
        return Err(NetworkError::InvalidPolicy);
    }
    let mut root_bytes = 0usize;
    for der in policy.additional_roots {
        root_bytes = root_bytes
            .checked_add(der.len())
            .ok_or(NetworkError::InvalidPolicy)?;
        if der.is_empty() || root_bytes > limits.max_certificate_bytes {
            return Err(NetworkError::InvalidPolicy);
        }
    }
    let mut builder = Client::builder()
        .tls_backend_rustls()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .referer(false)
        .no_gzip()
        .no_brotli()
        .no_zstd()
        .no_deflate()
        .http1_max_headers(limits.header_count)
        .http2_max_header_list_size(limits.header_bytes as u32)
        .pool_max_idle_per_host(limits.max_idle_per_host)
        .pool_idle_timeout(limits.idle_timeout);
    if let Some(proxy) = policy.proxy {
        if proxy.len() > limits.url_bytes {
            return Err(NetworkError::InvalidPolicy);
        }
        let parsed = Url::parse(proxy).map_err(|_| NetworkError::InvalidPolicy)?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || parsed.fragment().is_some()
            || parsed.as_str().len() > limits.url_bytes
        {
            return Err(NetworkError::InvalidPolicy);
        }
        builder =
            builder.proxy(reqwest::Proxy::all(parsed).map_err(|_| NetworkError::InvalidPolicy)?);
    }
    let certificates = policy
        .additional_roots
        .iter()
        .map(|der| reqwest::Certificate::from_der(der).map_err(|_| NetworkError::InvalidPolicy))
        .collect::<Result<Vec<_>, _>>()?;
    builder = builder.tls_certs_merge(certificates);
    if http1_only {
        builder = builder.http1_only();
    }
    let client = builder.build().map_err(|_| NetworkError::InvalidPolicy)?;
    Ok((client, origin, limits))
}

impl BoundedClient {
    pub fn new(policy: ClientPolicy<'_>, limits: Limits) -> Result<Self, NetworkError> {
        let (client, origin, limits) = build_client(policy, limits, false)?;
        Ok(Self {
            client,
            origin,
            limits,
            permits: Arc::new(Semaphore::new(limits.max_in_flight)),
            #[cfg(test)]
            before_send: None,
        })
    }

    /// The validated request body byte limit captured when this client was constructed.
    /// Consumers can bound encoding using the same limit that guards the native send.
    pub fn request_body_bytes(&self) -> usize {
        self.limits.request_body_bytes
    }

    pub async fn execute(
        &self,
        request: NetworkRequest<'_>,
        context: NetworkRequestContext<'_>,
    ) -> Result<NetworkResponse, NetworkError> {
        let mut gate = || Ok::<(), std::convert::Infallible>(());
        self.execute_with_gate(request, context, &mut gate)
            .await
            .map_err(|error| match error {
                GuardedNetworkError::Network { source, .. } => source,
                GuardedNetworkError::Admission { source, .. } => match source {},
            })
    }

    /// Recheck the caller's admission at send, every response await and output.
    /// The gate and original request context are borrowed only for this future.
    pub async fn execute_guarded<G, E>(
        &self,
        request: NetworkRequest<'_>,
        context: NetworkRequestContext<'_>,
        gate: &mut G,
    ) -> Result<NetworkResponse, GuardedNetworkError<E>>
    where
        G: FnMut() -> Result<(), E>,
    {
        self.execute_with_gate(request, context, gate).await
    }

    async fn execute_with_gate<G, E>(
        &self,
        request: NetworkRequest<'_>,
        context: NetworkRequestContext<'_>,
        gate: &mut G,
    ) -> Result<NetworkResponse, GuardedNetworkError<E>>
    where
        G: FnMut() -> Result<(), E>,
    {
        let mut sink = BufferedSink {
            headers: HeaderMap::new(),
            body: Zeroizing::new(Vec::new()),
            limit: self.limits.response_body_bytes,
        };
        let response = self
            .execute_stream_guarded(request, context, gate, &mut sink)
            .await
            .map_err(|error| match error {
                NetworkStreamError::Network { source, effect, .. }
                | NetworkStreamError::Sink { source, effect, .. } => {
                    GuardedNetworkError::Network { source, effect }
                }
                NetworkStreamError::Admission { source, effect, .. } => {
                    GuardedNetworkError::Admission { source, effect }
                }
            })?;
        Ok(NetworkResponse {
            status: response.status(),
            headers: sink.headers,
            body: sink.body,
        })
    }

    /// Deliver bounded response chunks without retaining the whole body. The sink
    /// and gate belong to this finite future; neither is retained by the client.
    /// Only a successful return permits the caller to consider publishing its
    /// staged bytes. Failure or future drop does not roll back sink effects.
    pub async fn execute_stream_guarded<G, E, S>(
        &self,
        request: NetworkRequest<'_>,
        context: NetworkRequestContext<'_>,
        gate: &mut G,
        sink: &mut S,
    ) -> Result<NetworkStreamResponse, NetworkStreamError<E, S::Error>>
    where
        G: FnMut() -> Result<(), E>,
        S: NetworkResponseSink,
    {
        let mut effect = NetworkEffect::NotSent;
        let mut accepted_bytes = 0usize;
        let result: Result<NetworkStreamResponse, RequestFailure<E, S::Error>> = async {
            context.check()?;
            let admission = gate().map_err(RequestFailure::Admission);
            context.check()?;
            admission?;
            if request.body.len() > self.limits.request_body_bytes {
                return Err(NetworkError::Limit.into());
            }
            let url = parse_url(request.url, self.limits.url_bytes)?;
            if url.origin() != self.origin.origin() {
                return Err(NetworkError::Origin.into());
            }
            check_headers(request.headers, self.limits)?;
            for name in request.headers.keys() {
                if matches!(
                    name.as_str(),
                    "host"
                        | "content-length"
                        | "transfer-encoding"
                        | "connection"
                        | "proxy-authorization"
                        | "proxy-connection"
                        | "keep-alive"
                        | "te"
                        | "trailer"
                        | "upgrade"
                        | "accept-encoding"
                ) {
                    return Err(NetworkError::InvalidRequest.into());
                }
            }
            let uri = http::Uri::try_from(url.as_str()).map_err(|_| NetworkError::InvalidRequest)?;
            let authority = uri.authority().ok_or(NetworkError::InvalidRequest)?;
            let content_length = request.body.len().to_string();
            let (mut count, mut bytes) = header_usage(request.headers)?;
            let accept = !request.headers.contains_key(header::ACCEPT);
            for (name, value) in [
                ("host", authority.as_str()),
                ("accept-encoding", "identity"),
                ("content-length", content_length.as_str()),
            ]
            .into_iter()
            .chain(accept.then_some(("accept", "*/*")))
            {
                count = count.checked_add(1).ok_or(NetworkError::Limit)?;
                bytes = bytes
                    .checked_add(name.len())
                    .and_then(|n| n.checked_add(value.len()))
                    .ok_or(NetworkError::Limit)?;
            }
            if count > self.limits.header_count || bytes > self.limits.header_bytes {
                return Err(NetworkError::Limit.into());
            }
            let mut headers = request.headers.clone();
            headers.insert(
                header::HOST,
                HeaderValue::from_str(authority.as_str()).map_err(|_| NetworkError::InvalidRequest)?,
            );
            headers.insert(
                header::ACCEPT_ENCODING,
                HeaderValue::from_static("identity"),
            );
            if !headers.contains_key(header::ACCEPT) {
                headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
            }
            headers.insert(
                header::CONTENT_LENGTH,
                HeaderValue::from_str(&content_length).map_err(|_| NetworkError::InvalidRequest)?,
            );
            for value in headers.values_mut() {
                value.set_sensitive(true);
            }
            check_headers(&headers, self.limits)?;
            let permit = self
                .permits
                .clone()
                .try_acquire_owned()
                .map_err(|_| NetworkError::Busy)?;
            context.check()?;
            let operation = async {
                let prepared = self
                    .client
                    .request(request.method, url)
                    .headers(headers)
                    .body(request.body.to_vec())
                    .build()
                    .map_err(|_| NetworkError::InvalidRequest)?;
                #[cfg(test)]
                if let Some(hook) = &self.before_send {
                    hook();
                }
                context.check()?;
                let admission = gate().map_err(RequestFailure::Admission);
                context.check()?;
                admission?;
                effect = NetworkEffect::MayHaveApplied;
                let response_result = self
                    .client
                    .execute(prepared)
                    .await
                    .map_err(|error| request_error(&error));
                context.check()?;
                let admission = gate().map_err(RequestFailure::Admission);
                context.check()?;
                let mut response = response_result?;
                admission?;
                check_headers(response.headers(), self.limits)?;
                for value in response.headers().get_all(header::CONTENT_ENCODING) {
                    if !value.as_bytes().eq_ignore_ascii_case(b"identity") {
                        return Err(NetworkError::Encoding.into());
                    }
                }
                if response
                    .content_length()
                    .is_some_and(|length| length > self.limits.response_body_bytes as u64)
                {
                    return Err(NetworkError::Limit.into());
                }
                let status = response.status();
                let mut headers = response.headers().clone();
                for value in headers.values_mut() {
                    value.set_sensitive(true);
                }
                context.check()?;
                let admission = gate().map_err(RequestFailure::Admission);
                context.check()?;
                admission?;
                let head = sink.head(status, &headers).map_err(RequestFailure::Sink);
                context.check()?;
                let admission = gate().map_err(RequestFailure::Admission);
                context.check()?;
                head?;
                admission?;
                drop(headers);
                loop {
                    let chunk_result = response.chunk().await.map_err(|error| request_error(&error));
                    context.check()?;
                    let admission = gate().map_err(RequestFailure::Admission);
                    context.check()?;
                    let chunk = chunk_result?;
                    admission?;
                    let Some(chunk) = chunk else { break; };
                    let length = accepted_bytes
                        .checked_add(chunk.len())
                        .ok_or(NetworkError::Limit)?;
                    if length > self.limits.response_body_bytes {
                        return Err(NetworkError::Limit.into());
                    }
                    context.check()?;
                    let admission = gate().map_err(RequestFailure::Admission);
                    context.check()?;
                    admission?;
                    let delivered = sink.chunk(&chunk).map_err(RequestFailure::Sink);
                    if delivered.is_ok() {
                        accepted_bytes = length;
                    }
                    context.check()?;
                    let admission = gate().map_err(RequestFailure::Admission);
                    context.check()?;
                    delivered?;
                    admission?;
                }
                context.check()?;
                let admission = gate().map_err(RequestFailure::Admission);
                context.check()?;
                admission?;
                Ok(NetworkStreamResponse {
                    status,
                    accepted_bytes,
                })
            };
            let result = tokio::select! {
                biased;
                _ = context.cancellation.cancelled() => Err(RequestFailure::Network(NetworkError::Cancelled)),
                _ = context.deadline_reached() => Err(RequestFailure::Network(NetworkError::Deadline)),
                result = operation => result,
            };
            drop(permit);
            context.check()?;
            let admission = gate().map_err(RequestFailure::Admission);
            context.check()?;
            match result {
                Ok(response) => { admission?; Ok(response) }
                Err(error) => Err(error),
            }
        }.await;
        result.map_err(|error| match error {
            RequestFailure::Network(source) => NetworkStreamError::Network {
                source,
                effect,
                accepted_bytes,
            },
            RequestFailure::Admission(source) => NetworkStreamError::Admission {
                source,
                effect,
                accepted_bytes,
            },
            RequestFailure::Sink(source) => NetworkStreamError::Sink {
                source,
                effect,
                accepted_bytes,
            },
        })
    }
}

pub(crate) fn parse_url(raw: &str, limit: usize) -> Result<Url, NetworkError> {
    if raw.len() > limit {
        return Err(NetworkError::Limit);
    }
    if raw
        .bytes()
        .any(|byte| byte.is_ascii_control() || byte == b' ')
        || raw.split_once("://").is_some_and(|(_, tail)| {
            tail.split(['/', '?', '#'])
                .next()
                .is_some_and(|authority| authority.contains('@'))
        })
    {
        return Err(NetworkError::InvalidRequest);
    }
    let url = Url::parse(raw).map_err(|_| NetworkError::InvalidRequest)?;
    if url.as_str().len() > limit {
        return Err(NetworkError::Limit);
    }
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(NetworkError::InvalidRequest);
    }
    Ok(url)
}
fn transport_allowed(url: &Url, allow_loopback_http: bool) -> bool {
    url.scheme() == "https"
        || (allow_loopback_http
            && url.scheme() == "http"
            && url
                .host_str()
                .and_then(|host| {
                    host.trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .ok()
                })
                .is_some_and(|address| address.is_loopback()))
}
pub(crate) fn check_headers(headers: &HeaderMap, limits: Limits) -> Result<(), NetworkError> {
    let (count, bytes) = header_usage(headers)?;
    if count > limits.header_count || bytes > limits.header_bytes {
        return Err(NetworkError::Limit);
    }
    Ok(())
}
fn header_usage(headers: &HeaderMap) -> Result<(usize, usize), NetworkError> {
    let mut bytes = 0usize;
    for (name, value) in headers {
        bytes = bytes
            .checked_add(name.as_str().len())
            .and_then(|n| n.checked_add(value.as_bytes().len()))
            .ok_or(NetworkError::Limit)?;
    }
    Ok((headers.len(), bytes))
}

#[cfg(test)]
mod tests;
