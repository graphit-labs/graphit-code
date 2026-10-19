//! Publish genuine PROJECT Now observations through the caller-owned Original.
//! Foreground scheduling and owned jobs remain the service owner's responsibility.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]

use crate::{
    now_subscription::{NowRecoveryCause, NowRemoteCode},
    project_now::{
        NowLocal, ProjectNowError, ProjectNowLimits, read_project_now, reauthorize_project_now,
    },
};
use graphit_auth::{
    AuthenticatedCaller, ProjectRecordAuthority, ScopeInput, ServiceBindingView,
    ServiceCredentialLease, ServicePurpose, ServiceUseError,
};
use graphit_core::{artifact_store::StoreOwner, project_identity::ProjectId};
use graphit_http::{
    client::NetworkRequestContext,
    websocket::{
        BoundedWebSocket, CloseCode, Message, WebSocketClient, WebSocketError, WebSocketRead,
    },
};
use graphit_hub::policy::RequestContext;
use http::{HeaderMap, HeaderValue, header};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor, value::MapAccessDeserializer},
};
use std::{
    error::Error,
    fmt,
    future::Future,
    io::{self, Write},
    num::NonZeroUsize,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

/// All authority and physical captures are real caller-owned producer objects.
#[derive(Clone, Copy)]
pub struct NowPublisherInput<'op, 'issuer> {
    pub authority: &'op ProjectRecordAuthority<'issuer>,
    pub caller: &'op AuthenticatedCaller<'issuer>,
    pub scope: ScopeInput<'op>,
    pub heads: ProjectNowLimits,
}

/// Actual escaped command bytes and whole inbound control bytes, not RSS quotas.
#[derive(Clone, Copy)]
pub struct NowPublisherLimits {
    pub command_bytes: NonZeroUsize,
    pub control_bytes: NonZeroUsize,
}

/// Retain an actual private cause without printing paths, credentials or payloads.
pub struct NowPublisherCause(Box<dyn Error + Send + Sync>);
impl fmt::Debug for NowPublisherCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(error) = self.0.downcast_ref::<ServiceUseError>() {
            // These are static enums; arbitrary I/O, payload and path causes
            // remain private below rather than entering the diagnostic chain.
            fmt::Debug::fmt(error, f)
        } else {
            f.write_str("private cause retained")
        }
    }
}
impl fmt::Display for NowPublisherCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for NowPublisherCause {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0
            .downcast_ref::<ServiceUseError>()
            .map(|error| error as &(dyn Error + 'static))
    }
}

#[derive(Debug)]
pub enum NowPublisherError {
    Admission(NowPublisherCause),
    Request(NowPublisherCause),
    Local(NowPublisherCause),
    Protocol(NowPublisherCause),
    Transport(WebSocketError),
    Http { status: u16, source: WebSocketError },
    InvalidScope,
    InvalidProtocol,
    DifferentOriginal,
    WrongPurpose,
    Limit,
    Allocation,
    Remote(NowRemoteCode),
    RecoveryRequired(NowRecoveryCause),
    Closed(u16),
}
impl fmt::Display for NowPublisherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Now publisher {self:?}")
    }
}
impl Error for NowPublisherError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Admission(e) | Self::Request(e) | Self::Local(e) | Self::Protocol(e) => Some(e),
            Self::Transport(e) | Self::Http { source: e, .. } => Some(e),
            _ => None,
        }
    }
}
impl From<ServiceUseError> for NowPublisherError {
    fn from(e: ServiceUseError) -> Self {
        Self::Admission(NowPublisherCause(Box::new(e)))
    }
}
impl From<io::Error> for NowPublisherError {
    fn from(e: io::Error) -> Self {
        Self::Request(NowPublisherCause(Box::new(e)))
    }
}
impl From<ProjectNowError> for NowPublisherError {
    fn from(e: ProjectNowError) -> Self {
        Self::Local(NowPublisherCause(Box::new(e)))
    }
}
impl From<serde_json::Error> for NowPublisherError {
    fn from(e: serde_json::Error) -> Self {
        Self::Protocol(NowPublisherCause(Box::new(e)))
    }
}
impl From<WebSocketError> for NowPublisherError {
    fn from(e: WebSocketError) -> Self {
        if let WebSocketError::Sdk(cause) = &e
            && let Some(status) = cause.http_status()
        {
            return Self::Http { status, source: e };
        }
        Self::Transport(e)
    }
}

#[derive(Serialize)]
struct Publish<'a> {
    op: &'static str,
    instance_id: &'a str,
    project_id: ProjectId,
    version: &'a str,
    snapshot: &'a NowLocal,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    Welcome { heartbeat_timeout_ns: String },
    Error { code: NowRemoteCode },
}
struct ObjectControl(Control);
impl<'de> Deserialize<'de> for ObjectControl {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Object;
        impl<'de> Visitor<'de> for Object {
            type Value = ObjectControl;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a Now publisher control object")
            }
            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                Control::deserialize(MapAccessDeserializer::new(map)).map(ObjectControl)
            }
        }
        d.deserialize_map(Object)
    }
}

type PendingObservation<'op> =
    Pin<Box<dyn Future<Output = Result<NowLocal, NowPublisherError>> + Send + 'op>>;

/// Immutable channel under one Original, retaining its last genuine observation.
/// At most one borrowed recomposition remains owned by this channel.
pub struct NowPublisher<'op, 'issuer> {
    lease: &'op ServiceCredentialLease<'issuer>,
    original: &'op RequestContext<'issuer>,
    input: NowPublisherInput<'op, 'issuer>,
    limits: NowPublisherLimits,
    instance_id: &'op str,
    project_id: ProjectId,
    version: &'op str,
    socket: Option<BoundedWebSocket<'op>>,
    cadence: Duration,
    next_publication: Instant,
    observed: NowLocal,
    pending: Option<PendingObservation<'op>>,
}

fn admitted(
    lease: &ServiceCredentialLease<'_>,
    original: &RequestContext<'_>,
) -> Result<(), NowPublisherError> {
    original.snapshot().check()?;
    let result = lease.check();
    original.snapshot().check()?;
    result?;
    if !Arc::ptr_eq(lease.original()?.snapshot(), original.snapshot()) {
        return Err(NowPublisherError::DifferentOriginal);
    }
    Ok(())
}

/// Compose with actual module admissions, send the first publish, then await welcome.
/// The caller retains the actual selection/verifier/credential and all scope captures.
pub async fn start_now_publisher<'op, 'issuer>(
    client: &WebSocketClient,
    lease: &'op ServiceCredentialLease<'issuer>,
    original: &'op RequestContext<'issuer>,
    input: NowPublisherInput<'op, 'issuer>,
    limits: NowPublisherLimits,
) -> Result<NowPublisher<'op, 'issuer>, NowPublisherError> {
    admitted(lease, original)?;
    let StoreOwner::Installation { installation_id } = input.scope.installation.live.owner() else {
        return Err(NowPublisherError::InvalidScope);
    };
    let project = input.scope.project.ok_or(NowPublisherError::InvalidScope)?;
    let StoreOwner::Project { project_id } = project.scope.live.owner() else {
        return Err(NowPublisherError::InvalidScope);
    };
    let instance_id = installation_id.as_str();
    let project_id = *project_id;
    let version = project.version.version();
    let ServiceBindingView::Broker {
        purpose: ServicePurpose::PresencePublish,
        endpoint,
        ..
    } = lease.binding()?
    else {
        return Err(NowPublisherError::WrongPurpose);
    };
    let mut destination =
        url::Url::parse(endpoint).map_err(|_| NowPublisherError::InvalidProtocol)?;
    if destination.query().is_some() || destination.fragment().is_some() {
        return Err(NowPublisherError::InvalidProtocol);
    }
    destination
        .path_segments_mut()
        .map_err(|_| NowPublisherError::InvalidProtocol)?
        .pop_if_empty()
        .extend(["v1", "realtime"]);
    let raw = lease.credential()?.ok_or(NowPublisherError::WrongPurpose)?;
    let bearer = Zeroizing::new(format!("Bearer {raw}"));
    let mut value =
        HeaderValue::from_str(&bearer).map_err(|_| NowPublisherError::InvalidProtocol)?;
    value.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, value);
    let observed = composed(input, lease, original, project_id).await?;
    admitted(lease, original)?;
    let connected = client
        .connect(
            destination.as_str(),
            headers,
            NetworkRequestContext::with_deadline(original.deadline(), original.cancellation()),
        )
        .await;
    admitted(lease, original)?;
    let mut socket = connected?;
    let text = publication(
        input,
        lease,
        original,
        limits,
        Publish {
            op: "publish",
            instance_id,
            project_id,
            version,
            snapshot: &observed,
        },
    )?;
    let published_at = Instant::now();
    let sent = socket.send(Message::Text(text)).await;
    admitted(lease, original)?;
    sent?;
    let (cadence, next_publication) = loop {
        admitted(lease, original)?;
        let received = socket.receive().await;
        admitted(lease, original)?;
        match received? {
            WebSocketRead::Message(Message::Text(text)) => {
                match decode(&text, limits.control_bytes)? {
                    Control::Welcome {
                        heartbeat_timeout_ns,
                    } => break negotiated_schedule(&heartbeat_timeout_ns, published_at)?,
                    Control::Error { code } => return Err(notice(code)),
                }
            }
            WebSocketRead::Message(Message::Ping(_) | Message::Pong(_)) => (),
            WebSocketRead::Message(Message::Close { code, .. }) => return Err(closed(code)),
            WebSocketRead::Closed => return Err(NowPublisherError::Closed(1000)),
            _ => return Err(NowPublisherError::InvalidProtocol),
        }
    };
    admitted(lease, original)?;
    Ok(NowPublisher {
        lease,
        original,
        input,
        limits,
        instance_id,
        project_id,
        version,
        socket: Some(socket),
        cadence,
        next_publication,
        observed,
        pending: None,
    })
}

impl<'op, 'issuer> NowPublisher<'op, 'issuer> {
    /// Publish on the negotiated tick while one real observation may be pending.
    /// Errors or abandonment drop the socket and the owned pending read together.
    pub async fn next(&mut self) -> Result<(), NowPublisherError> {
        let mut pending = self.pending.take();
        let mut socket = self.socket.take().ok_or(NowPublisherError::Closed(1000))?;
        if pending.is_none() {
            pending = Some(Box::pin(composed(
                self.input,
                self.lease,
                self.original,
                self.project_id,
            )));
        }
        loop {
            admitted(self.lease, self.original)?;
            let received = receive_observation(
                &mut socket,
                self.next_publication,
                &mut self.observed,
                &mut pending,
            )
            .await;
            admitted(self.lease, self.original)?;
            match received? {
                WebSocketRead::Wakeup => {
                    let text = publication(
                        self.input,
                        self.lease,
                        self.original,
                        self.limits,
                        Publish {
                            op: "publish",
                            instance_id: self.instance_id,
                            project_id: self.project_id,
                            version: self.version,
                            snapshot: &self.observed,
                        },
                    )?;
                    admitted(self.lease, self.original)?;
                    let sent = socket.send(Message::Text(text)).await;
                    admitted(self.lease, self.original)?;
                    sent?;
                    self.next_publication = Instant::now()
                        .checked_add(self.cadence)
                        .ok_or(NowPublisherError::InvalidProtocol)?;
                    self.pending = pending;
                    self.socket = Some(socket);
                    return Ok(());
                }
                WebSocketRead::Message(Message::Text(text)) => {
                    match decode(&text, self.limits.control_bytes)? {
                        Control::Error { code } => return Err(notice(code)),
                        _ => return Err(NowPublisherError::InvalidProtocol),
                    }
                }
                WebSocketRead::Message(Message::Ping(_) | Message::Pong(_)) => (),
                WebSocketRead::Message(Message::Close { code, .. }) => {
                    return Err(closed(code));
                }
                WebSocketRead::Closed => return Err(NowPublisherError::Closed(1000)),
                _ => return Err(NowPublisherError::InvalidProtocol),
            }
        }
    }

    /// Consuming cleanup closes only this socket; this API owns no Graphit jobs.
    pub async fn shutdown(mut self) -> Result<(), NowPublisherError> {
        self.pending = None;
        let Some(socket) = self.socket.take() else {
            return Ok(());
        };
        admitted(self.lease, self.original)?;
        let result = socket.close(CloseCode::Normal).await;
        admitted(self.lease, self.original)?;
        result.map_err(Into::into)
    }
}

async fn receive_observation(
    socket: &mut BoundedWebSocket<'_>,
    wakeup: Instant,
    observed: &mut NowLocal,
    pending: &mut Option<PendingObservation<'_>>,
) -> Result<WebSocketRead, NowPublisherError> {
    // receive_until owns the SDK socket while pending. Keep this same future
    // alive when composition completes: dropping it closes its socket.
    let receive = socket.receive_until(wakeup);
    tokio::pin!(receive);
    loop {
        if Instant::now() >= wakeup {
            return receive.await.map_err(Into::into);
        }
        if let Some(read) = pending.as_mut() {
            tokio::select! {
                biased;
                received = &mut receive => return received.map_err(Into::into),
                next = read => {
                    *observed = next?;
                    *pending = None;
                }
            }
        } else {
            return receive.await.map_err(Into::into);
        }
    }
}

async fn composed(
    input: NowPublisherInput<'_, '_>,
    lease: &ServiceCredentialLease<'_>,
    original: &RequestContext<'_>,
    project_id: ProjectId,
) -> Result<NowLocal, NowPublisherError> {
    admitted(lease, original)?;
    let result = read_project_now(
        input.authority,
        input.caller,
        original,
        input.scope,
        input.heads,
    )
    .await;
    admitted(lease, original)?;
    let snapshot = result?;
    if snapshot.project_id() != project_id {
        return Err(NowPublisherError::InvalidScope);
    }
    Ok(snapshot)
}

fn publication(
    input: NowPublisherInput<'_, '_>,
    lease: &ServiceCredentialLease<'_>,
    original: &RequestContext<'_>,
    limits: NowPublisherLimits,
    observed: Publish<'_>,
) -> Result<String, NowPublisherError> {
    admitted(lease, original)?;
    let snapshot = reauthorize_project_now(
        input.authority,
        input.caller,
        original,
        input.scope,
        observed.snapshot,
    )?;
    if snapshot.project_id() != observed.project_id {
        return Err(NowPublisherError::InvalidScope);
    }
    let result = encode(
        &Publish {
            snapshot: &snapshot,
            ..observed
        },
        original,
        limits.command_bytes,
    );
    admitted(lease, original)?;
    result
}

fn decode(text: &str, cap: NonZeroUsize) -> Result<Control, NowPublisherError> {
    if text.len() > cap.get() {
        return Err(NowPublisherError::Limit);
    }
    serde_json::from_str::<ObjectControl>(text)
        .map(|v| v.0)
        .map_err(Into::into)
}
fn negotiated_schedule(
    text: &str,
    published_at: Instant,
) -> Result<(Duration, Instant), NowPublisherError> {
    if text.is_empty() || text.starts_with('0') || !text.bytes().all(|c| c.is_ascii_digit()) {
        return Err(NowPublisherError::InvalidProtocol);
    }
    let timeout = text
        .parse::<u128>()
        .map_err(|_| NowPublisherError::InvalidProtocol)?;
    u64::try_from(timeout / 1_000_000_000).map_err(|_| NowPublisherError::InvalidProtocol)?;
    let half = timeout / 2;
    if half == 0 || half >= timeout {
        return Err(NowPublisherError::InvalidProtocol);
    }
    let seconds =
        u64::try_from(half / 1_000_000_000).map_err(|_| NowPublisherError::InvalidProtocol)?;
    let cadence = Duration::new(seconds, (half % 1_000_000_000) as u32);
    let next = published_at
        .checked_add(cadence)
        .ok_or(NowPublisherError::InvalidProtocol)?;
    Ok((cadence, next))
}
fn notice(code: NowRemoteCode) -> NowPublisherError {
    if code == NowRemoteCode::Backpressure {
        NowPublisherError::RecoveryRequired(NowRecoveryCause::Notice(code))
    } else {
        NowPublisherError::Remote(code)
    }
}
fn closed(code: CloseCode) -> NowPublisherError {
    let code = u16::from(code);
    if code == 1013 {
        NowPublisherError::RecoveryRequired(NowRecoveryCause::Close(code))
    } else {
        NowPublisherError::Closed(code)
    }
}

struct CommandWriter<'a, 'r> {
    original: &'a RequestContext<'r>,
    cap: usize,
    bytes: Vec<u8>,
    error: Option<NowPublisherError>,
}
impl Write for CommandWriter<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = (|| {
            if self.error.is_some() {
                return Err(NowPublisherError::Limit);
            }
            self.original.snapshot().check()?;
            self.bytes
                .len()
                .checked_add(bytes.len())
                .filter(|n| *n <= self.cap)
                .ok_or(NowPublisherError::Limit)?;
            self.bytes
                .try_reserve_exact(bytes.len())
                .map_err(|_| NowPublisherError::Allocation)?;
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        })();
        match result {
            Ok(n) => Ok(n),
            Err(error) => {
                if self.error.is_none() {
                    self.error = Some(error);
                }
                Err(io::Error::other("Now publisher command rejected"))
            }
        }
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.write(bytes).map(|_| ())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.write(&[]).map(|_| ())
    }
}
fn encode(
    value: &impl Serialize,
    original: &RequestContext<'_>,
    cap: NonZeroUsize,
) -> Result<String, NowPublisherError> {
    original.snapshot().check()?;
    let mut writer = CommandWriter {
        original,
        cap: cap.get(),
        bytes: Vec::new(),
        error: None,
    };
    if let Err(e) = serde_json::to_writer(&mut writer, value) {
        return Err(writer.error.unwrap_or_else(|| e.into()));
    }
    original.snapshot().check()?;
    String::from_utf8(writer.bytes).map_err(|_| NowPublisherError::InvalidProtocol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use graphit_hub::native::ContentLimits;
    use serde::Serializer;
    use std::cell::Cell;

    fn n(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).unwrap()
    }
    fn request(cancel: &Cancellation) -> RequestContext<'_> {
        RequestContext::new(
            ContentLimits {
                max_input_bytes: 65536,
                max_output_bytes: 65536,
                max_depth: 32,
                max_nodes: 4096,
                max_scalar_bytes: 4096,
                max_collection_entries: 64,
            },
            cancel,
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap()
    }
    #[test]
    fn control_is_object_only_exact_and_maps_all_real_notices() {
        let cap = n(65536);
        for raw in [
            r#"["welcome","2000"]"#,
            r#"{"type":"welcome","heartbeat_timeout_ns":"2000","extra":1}"#,
            r#"{"type":"welcome","type":"welcome","heartbeat_timeout_ns":"2000"}"#,
            r#"{"type":"welcome","heartbeat_timeout_ns":2000}"#,
            r#"{"type":"snapshot","project_id":"ignored","instances":[]}"#,
            r#"{"type":"error","code":"invented"}"#,
        ] {
            assert!(decode(raw, cap).is_err(), "{raw}");
        }
        for (name, expected) in [
            ("invalid_data", NowRemoteCode::InvalidData),
            ("too_large", NowRemoteCode::TooLarge),
            ("unauthenticated", NowRemoteCode::Unauthenticated),
            ("forbidden", NowRemoteCode::Forbidden),
            ("deadline", NowRemoteCode::Deadline),
            (
                "authorization_unavailable",
                NowRemoteCode::AuthorizationUnavailable,
            ),
            ("closed", NowRemoteCode::Closed),
            ("superseded", NowRemoteCode::Superseded),
            ("backpressure", NowRemoteCode::Backpressure),
            ("outcome_uncertain", NowRemoteCode::OutcomeUncertain),
            ("not_registered", NowRemoteCode::NotRegistered),
            ("owner_conflict", NowRemoteCode::OwnerConflict),
        ] {
            let raw = format!(r#"{{"type":"error","code":"{name}"}}"#);
            let Control::Error { code } = decode(&raw, cap).unwrap() else {
                panic!()
            };
            assert_eq!(code, expected);
            if expected == NowRemoteCode::Backpressure {
                assert!(matches!(
                    notice(code),
                    NowPublisherError::RecoveryRequired(NowRecoveryCause::Notice(
                        NowRemoteCode::Backpressure
                    ))
                ));
            } else {
                assert!(
                    matches!(notice(code), NowPublisherError::Remote(actual) if actual == expected)
                );
            }
        }
        let raw = r#"{"type":"welcome","heartbeat_timeout_ns":"2000"}"#;
        assert!(decode(raw, n(raw.len())).is_ok());
        assert!(matches!(
            decode(raw, n(raw.len() - 1)),
            Err(NowPublisherError::Limit)
        ));
        assert!(matches!(
            closed(CloseCode::Again),
            NowPublisherError::RecoveryRequired(NowRecoveryCause::Close(1013))
        ));
    }

    #[test]
    fn cadence_preserves_decimal_precision_and_rejects_nonrepresentable_timeouts() {
        assert_eq!(
            negotiated_schedule("9007199254740993001", Instant::now())
                .unwrap()
                .0,
            Duration::new(4_503_599_627, 370_496_500)
        );
        assert_eq!(
            negotiated_schedule("3", Instant::now()).unwrap().0,
            Duration::from_nanos(1)
        );
        for bad in [
            "",
            "0",
            "1",
            "01",
            "-2",
            "+2",
            "2.0",
            "2e3",
            "340282366920938463463374607431768211455",
            "18446744073709551616000000000",
        ] {
            assert!(
                matches!(
                    negotiated_schedule(bad, Instant::now()),
                    Err(NowPublisherError::InvalidProtocol)
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn writer_encodes_once_bounds_escaped_bytes_and_retains_first_failure() {
        struct Once<'a>(&'a Cell<usize>);
        impl Serialize for Once<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.set(self.0.get() + 1);
                serializer.serialize_str("ação\n\"quoted\"")
            }
        }
        let cancel = Cancellation::default();
        let original = request(&cancel);
        let calls = Cell::new(0);
        let actual = encode(&Once(&calls), &original, n(65536)).unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(
            encode(&Once(&calls), &original, n(actual.len())).unwrap(),
            actual
        );
        assert_eq!(calls.get(), 2);
        assert!(matches!(
            encode(&Once(&calls), &original, n(actual.len() - 1)),
            Err(NowPublisherError::Limit)
        ));
        assert_eq!(calls.get(), 3);
        let mut writer = CommandWriter {
            original: &original,
            cap: 1,
            bytes: Vec::new(),
            error: None,
        };
        assert!(writer.write_all(b"too large").is_err());
        cancel.cancel();
        assert!(writer.flush().is_err());
        assert!(matches!(writer.error, Some(NowPublisherError::Limit)));
        let error =
            NowPublisherError::from(io::Error::other("private-path/private-token/private-body"));
        let diagnostics = format!("{error:?} {error}");
        assert!(!diagnostics.contains("private-path") && !diagnostics.contains("private-token"));
        assert!(error.source().is_some());
        assert!(error.source().unwrap().source().is_none());
        let admission = NowPublisherError::from(ServiceUseError::Auth(graphit_auth::AuthError::Io));
        assert_eq!(format!("{admission:?}"), "Admission(Auth(Io))");
        assert!(matches!(
            admission
                .source()
                .unwrap()
                .source()
                .unwrap()
                .downcast_ref::<ServiceUseError>(),
            Some(ServiceUseError::Auth(graphit_auth::AuthError::Io))
        ));
        assert!(matches!(
            encode(&"value", &original, n(65536)),
            Err(NowPublisherError::Request(_))
        ));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod pending_tests;
