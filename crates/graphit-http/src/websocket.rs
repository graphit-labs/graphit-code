//! Native WebSocket transport using the captured HTTP policy and one Original.
//! Authentication and PROJECT admission remain with the borrowing consumer.

use crate::client::{
    ClientPolicy, Limits, NetworkError, NetworkRequestContext, build_client, check_headers,
    connection_failure, parse_url, request_error,
};
use futures_util::{SinkExt, StreamExt};
use http::{HeaderMap, HeaderValue, header};
pub use reqwest_websocket::{CloseCode, Message};
use reqwest_websocket::{Upgrade, WebSocket};
use std::{error::Error, fmt, num::NonZeroUsize, time::Instant};
use tungstenite::protocol::WebSocketConfig;

/// Explicit payload limits; these do not bound vendor TLS/HTTP allocations.
#[derive(Clone, Copy)]
pub struct WebSocketLimits {
    pub frame_bytes: NonZeroUsize,
    pub message_bytes: NonZeroUsize,
    pub send_bytes: NonZeroUsize,
}

/// Finite categories never include response headers, URLs or message contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebSocketFailureKind {
    Handshake,
    Transport,
    Protocol,
    Capacity,
    Closed,
}

/// Retains the actual SDK cause behind a redacted diagnostic boundary.
pub struct WebSocketSdkCause(Box<reqwest_websocket::Error>);
impl WebSocketSdkCause {
    /// Actual pre-upgrade HTTP status, without URL, header or response-body data.
    pub fn http_status(&self) -> Option<u16> {
        match self.0.as_ref() {
            reqwest_websocket::Error::Handshake(
                reqwest_websocket::HandshakeError::UnexpectedStatusCode(status),
            ) => Some(status.as_u16()),
            reqwest_websocket::Error::Reqwest(error) => {
                error.status().map(|status| status.as_u16())
            }
            _ => None,
        }
    }

    pub fn kind(&self) -> WebSocketFailureKind {
        match self.0.as_ref() {
            reqwest_websocket::Error::Handshake(_) => WebSocketFailureKind::Handshake,
            reqwest_websocket::Error::Tungstenite(error) => match error {
                tungstenite::Error::Capacity(_) | tungstenite::Error::WriteBufferFull(_) => {
                    WebSocketFailureKind::Capacity
                }
                tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => {
                    WebSocketFailureKind::Closed
                }
                tungstenite::Error::Protocol(
                    tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                ) => WebSocketFailureKind::Closed,
                tungstenite::Error::Io(error) if connection_failure(error.kind()) => {
                    WebSocketFailureKind::Closed
                }
                tungstenite::Error::Protocol(_) | tungstenite::Error::Utf8(_) => {
                    WebSocketFailureKind::Protocol
                }
                _ => WebSocketFailureKind::Transport,
            },
            reqwest_websocket::Error::Reqwest(error) => {
                if request_error(error) == NetworkError::Connection {
                    WebSocketFailureKind::Closed
                } else {
                    WebSocketFailureKind::Transport
                }
            }
            _ => WebSocketFailureKind::Transport,
        }
    }
}
impl fmt::Debug for WebSocketSdkCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut diagnostic = f.debug_struct("WebSocket");
        diagnostic.field("kind", &self.kind());
        match self.0.as_ref() {
            reqwest_websocket::Error::Handshake(error) => {
                use reqwest_websocket::HandshakeError;
                let reason = match error {
                    HandshakeError::UnsupportedHttpVersion(_) => "unsupported_http_version",
                    HandshakeError::ServerRespondedWithDifferentVersion => "http_version_changed",
                    HandshakeError::MissingHeader { .. } => "missing_header",
                    HandshakeError::UnexpectedHeaderValue { .. } => "unexpected_header_value",
                    HandshakeError::ExpectedAProtocol => "missing_protocol",
                    HandshakeError::UnexpectedProtocol { .. } => "unexpected_protocol",
                    HandshakeError::UnexpectedStatusCode(_) => "http_status",
                };
                diagnostic
                    .field("sdk", &"handshake")
                    .field("reason", &reason);
            }
            reqwest_websocket::Error::Reqwest(error) => {
                let reason = if error.is_timeout() {
                    "timeout"
                } else if error.is_upgrade() {
                    "upgrade"
                } else if error.is_connect() {
                    "connect"
                } else if error.is_body() {
                    "body"
                } else if error.is_decode() {
                    "decode"
                } else if error.is_builder() {
                    "builder"
                } else if error.is_redirect() {
                    "redirect"
                } else if error.is_status() {
                    "http_status"
                } else if error.is_request() {
                    "request"
                } else {
                    "other"
                };
                diagnostic
                    .field("sdk", &"reqwest")
                    .field("network", &request_error(error))
                    .field("reason", &reason);
            }
            reqwest_websocket::Error::Tungstenite(error) => {
                use tungstenite::Error;
                let reason = match error {
                    Error::ConnectionClosed => "peer_closed",
                    Error::AlreadyClosed => "already_closed",
                    Error::Io(error) => {
                        diagnostic.field("io_kind", &error.kind());
                        "io"
                    }
                    Error::Tls(_) => "tls",
                    Error::Capacity(_) => "capacity",
                    Error::Protocol(_) => "protocol",
                    Error::WriteBufferFull(_) => "write_buffer_full",
                    Error::Utf8(_) => "utf8",
                    Error::AttackAttempt => "attack_attempt",
                    Error::Url(_) => "url",
                    Error::Http(_) => "http",
                    Error::HttpFormat(_) => "http_format",
                };
                diagnostic
                    .field("sdk", &"tungstenite")
                    .field("reason", &reason);
            }
            _ => {
                diagnostic.field("sdk", &"other");
            }
        }
        if let Some(status) = self.http_status() {
            diagnostic.field("status", &status);
        }
        diagnostic.finish()
    }
}
impl fmt::Display for WebSocketSdkCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for WebSocketSdkCause {}

#[derive(Debug)]
pub enum WebSocketError {
    Network(NetworkError),
    Sdk(WebSocketSdkCause),
    Closed,
}
impl fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(error) => write!(f, "WebSocket {error}"),
            Self::Sdk(error) => error.fmt(f),
            Self::Closed => f.write_str("WebSocket closed"),
        }
    }
}
impl Error for WebSocketError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Network(error) => Some(error),
            Self::Sdk(error) => Some(error),
            Self::Closed => None,
        }
    }
}
impl From<NetworkError> for WebSocketError {
    fn from(error: NetworkError) -> Self {
        Self::Network(error)
    }
}
impl From<reqwest_websocket::Error> for WebSocketError {
    fn from(error: reqwest_websocket::Error) -> Self {
        Self::Sdk(WebSocketSdkCause(Box::new(error)))
    }
}

/// Connection pool and trust settings are captured once, without ambient proxy.
pub struct WebSocketClient {
    client: reqwest::Client,
    origin: reqwest::Url,
    network: Limits,
    limits: WebSocketLimits,
    config: WebSocketConfig,
}
impl WebSocketClient {
    pub fn new(
        policy: ClientPolicy<'_>,
        network: Limits,
        limits: WebSocketLimits,
    ) -> Result<Self, WebSocketError> {
        for limit in [limits.frame_bytes, limits.message_bytes, limits.send_bytes] {
            if limit.get() > isize::MAX as usize {
                return Err(NetworkError::InvalidPolicy.into());
            }
        }
        // A masked frame has at most 14 bytes of framing. Eager writes do not
        // retain an application queue; the SDK also needs room for control frames.
        let write_bytes = limits
            .send_bytes
            .get()
            .max(125)
            .checked_add(14)
            .ok_or(NetworkError::InvalidPolicy)?;
        let config = WebSocketConfig::default()
            .write_buffer_size(0)
            .max_write_buffer_size(write_bytes)
            .max_frame_size(Some(limits.frame_bytes.get()))
            .max_message_size(Some(limits.message_bytes.get()));
        let (client, origin, network) = build_client(policy, network, true)?;
        Ok(Self {
            client,
            origin,
            network,
            limits,
            config,
        })
    }

    /// The destination uses the policy's HTTP(S) scheme. The SDK performs upgrade.
    /// Authorization headers are marked sensitive before the native request.
    pub async fn connect<'a>(
        &self,
        destination: &str,
        mut headers: HeaderMap,
        original: NetworkRequestContext<'a>,
    ) -> Result<BoundedWebSocket<'a>, WebSocketError> {
        original.check()?;
        let destination = parse_url(destination, self.network.url_bytes)?;
        if destination.origin() != self.origin.origin() {
            return Err(NetworkError::Origin.into());
        }
        for name in headers.keys() {
            if matches!(
                name.as_str(),
                "host"
                    | "connection"
                    | "upgrade"
                    | "proxy-authorization"
                    | "proxy-connection"
                    | "accept-encoding"
                    | "content-length"
                    | "transfer-encoding"
            ) || name.as_str().starts_with("sec-websocket-")
            {
                return Err(NetworkError::InvalidRequest.into());
            }
        }
        if headers.get_all(header::AUTHORIZATION).iter().count() > 1 {
            return Err(NetworkError::InvalidRequest.into());
        }
        for value in headers.values_mut() {
            value.set_sensitive(true);
        }
        let uri =
            http::Uri::try_from(destination.as_str()).map_err(|_| NetworkError::InvalidRequest)?;
        let authority = uri.authority().ok_or(NetworkError::InvalidRequest)?;
        headers.insert(
            header::HOST,
            HeaderValue::from_str(authority.as_str()).map_err(|_| NetworkError::InvalidRequest)?,
        );
        if !headers.contains_key(header::ACCEPT) {
            headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
        }
        headers.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_static("identity"),
        );
        check_headers(&headers, self.network)?;
        // Account for the four HTTP/1 upgrade headers generated by this SDK.
        // Its nonce is a fixed 24-byte Base64 key; no placeholder credential is built.
        let managed_bytes = "connection".len()
            + "upgrade".len()
            + "upgrade".len()
            + "websocket".len()
            + "sec-websocket-key".len()
            + 24
            + "sec-websocket-version".len()
            + "13".len();
        let header_bytes = headers
            .iter()
            .try_fold(managed_bytes, |n, (key, value)| {
                n.checked_add(key.as_str().len())?
                    .checked_add(value.as_bytes().len())
            })
            .ok_or(NetworkError::Limit)?;
        if headers
            .len()
            .checked_add(4)
            .is_none_or(|n| n > self.network.header_count)
            || header_bytes > self.network.header_bytes
        {
            return Err(NetworkError::Limit.into());
        }
        original.check()?;
        let operation = async {
            let response = self
                .client
                .get(destination)
                .headers(headers)
                .upgrade()
                .web_socket_config(self.config)
                .send()
                .await?;
            check_headers(response.headers(), self.network).map_err(WebSocketError::from)?;
            original.check()?;
            response
                .into_websocket()
                .await
                .map_err(WebSocketError::from)
        };
        let socket = bounded(original, operation).await?;
        Ok(BoundedWebSocket {
            socket: Some(socket),
            original,
            send_bytes: self.limits.send_bytes,
        })
    }
}

/// Read data has no Debug implementation; callers choose how to project it.
pub enum WebSocketRead {
    Message(Message),
    Wakeup,
    Closed,
}

/// Owns one native socket and borrows its immutable Original. No jobs are spawned.
pub struct BoundedWebSocket<'a> {
    socket: Option<WebSocket>,
    original: NetworkRequestContext<'a>,
    send_bytes: NonZeroUsize,
}
impl BoundedWebSocket<'_> {
    /// A dropped send future drops the socket too, including any pending SDK DATA.
    pub async fn send(&mut self, message: Message) -> Result<(), WebSocketError> {
        let mut socket = self.socket.take().ok_or(WebSocketError::Closed)?;
        self.original.check()?;
        let bytes = match &message {
            Message::Text(text) => text.len(),
            Message::Binary(bytes) | Message::Ping(bytes) | Message::Pong(bytes) => bytes.len(),
            Message::Close { reason, .. } => {
                reason.len().checked_add(2).ok_or(NetworkError::Limit)?
            }
        };
        if bytes > self.send_bytes.get() {
            return Err(NetworkError::Limit.into());
        }
        bounded(self.original, async {
            socket.send(message).await.map_err(Into::into)
        })
        .await?;
        self.socket = Some(socket);
        Ok(())
    }

    /// A consumer can wake for its negotiated heartbeat without replacing Original.
    /// Pending SDK read state survives Wakeup. Error, EOF, Close or future Drop ends it.
    pub async fn receive_until(
        &mut self,
        wakeup: Instant,
    ) -> Result<WebSocketRead, WebSocketError> {
        self.receive_with_wakeup(Some(wakeup)).await
    }

    /// Reads under Original cancellation and its optional deadline, without a heartbeat timer.
    pub async fn receive(&mut self) -> Result<WebSocketRead, WebSocketError> {
        self.receive_with_wakeup(None).await
    }

    async fn receive_with_wakeup(
        &mut self,
        wakeup: Option<Instant>,
    ) -> Result<WebSocketRead, WebSocketError> {
        let mut socket = self.socket.take().ok_or(WebSocketError::Closed)?;
        let result = bounded(self.original, async {
            tokio::select! {
                biased;
                _ = async { match wakeup { Some(wakeup) => tokio::time::sleep_until(wakeup.into()).await, None => std::future::pending().await } } => Ok(WebSocketRead::Wakeup),
                message = socket.next() => match message {
                    Some(Ok(message)) => Ok(WebSocketRead::Message(message)),
                    Some(Err(error)) => Err(error.into()),
                    None => Ok(WebSocketRead::Closed),
                },
            }
        })
        .await?;
        if !matches!(
            &result,
            WebSocketRead::Closed | WebSocketRead::Message(Message::Close { .. })
        ) {
            self.socket = Some(socket);
        }
        Ok(result)
    }

    /// Close is consuming and bounded by the captured Original; errors drop directly.
    pub async fn close(mut self, code: CloseCode) -> Result<(), WebSocketError> {
        let socket = self.socket.take().ok_or(WebSocketError::Closed)?;
        self.original.check()?;
        if !code.is_allowed() {
            return Err(NetworkError::InvalidRequest.into());
        }
        bounded(self.original, async {
            socket.close(code, None).await.map_err(Into::into)
        })
        .await
    }
}

async fn bounded<T>(
    original: NetworkRequestContext<'_>,
    operation: impl std::future::Future<Output = Result<T, WebSocketError>>,
) -> Result<T, WebSocketError> {
    original.check()?;
    let result = tokio::select! {
        biased;
        _ = original.cancellation.cancelled() => Err(NetworkError::Cancelled.into()),
        _ = original.deadline_reached() => Err(NetworkError::Deadline.into()),
        result = operation => result,
    };
    original.check()?;
    result
}
