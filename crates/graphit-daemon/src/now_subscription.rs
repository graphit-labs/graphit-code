//! Admitted Now subscription over the real captured WebSocket transport.
//! Remote snapshots are DATA; they never grant local or PROJECT authority.

use broker_now_wire::NowSnapshotData;
use graphit_auth::{
    ServiceBindingView, ServiceCredentialLease, ServicePurpose, ServiceUseError,
    broker::{
        BrokerError, BrokerRequestContext, BrokerVerifier, oauth::web::AuthenticatedWebCaller,
    },
};
use graphit_core::project_identity::ProjectId;
use graphit_http::{
    client::NetworkRequestContext,
    websocket::{
        BoundedWebSocket, CloseCode, Message, WebSocketClient, WebSocketError, WebSocketRead,
    },
};
use graphit_hub::policy::RequestContext;
use http::{HeaderMap, HeaderValue, header};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Visitor},
    ser::SerializeStruct,
};
use std::{
    error::Error,
    fmt,
    io::{self, Write},
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

#[derive(Clone, Copy)]
pub struct NowSubscriptionConfig<'a> {
    pub project_id: ProjectId,
    pub instance_id: Option<&'a str>,
    pub version: Option<&'a str>,
    pub command_bytes: NonZeroUsize,
    pub snapshot_bytes: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NowRemoteCode {
    InvalidData,
    TooLarge,
    Unauthenticated,
    Forbidden,
    Deadline,
    AuthorizationUnavailable,
    Closed,
    Superseded,
    Backpressure,
    OutcomeUncertain,
    NotRegistered,
    OwnerConflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NowRecoveryCause {
    Notice(NowRemoteCode),
    Close(u16),
}

/// Actual admission errors remain private behind a finite diagnostic boundary.
pub struct NowAdmissionCause(ServiceUseError);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NowAdmissionKind {
    Auth,
    Store,
    Broker,
    Profile,
    Disabled,
    MissingCredential,
    NeedsRefresh,
    LoginRequired,
    WrongMode,
}
impl NowAdmissionCause {
    pub fn kind(&self) -> NowAdmissionKind {
        match &self.0 {
            ServiceUseError::Auth(_) => NowAdmissionKind::Auth,
            ServiceUseError::Store(_) => NowAdmissionKind::Store,
            ServiceUseError::Broker(_) => NowAdmissionKind::Broker,
            ServiceUseError::Profile(_) => NowAdmissionKind::Profile,
            ServiceUseError::Disabled => NowAdmissionKind::Disabled,
            ServiceUseError::MissingCredential => NowAdmissionKind::MissingCredential,
            ServiceUseError::LoginRequired => NowAdmissionKind::LoginRequired,
            ServiceUseError::NeedsRefresh => NowAdmissionKind::NeedsRefresh,
            ServiceUseError::WrongMode => NowAdmissionKind::WrongMode,
        }
    }
}
impl fmt::Debug for NowAdmissionCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind().fmt(f)
    }
}
impl fmt::Display for NowAdmissionCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for NowAdmissionCause {}

pub struct NowRequestCause(io::Error);
impl fmt::Debug for NowRequestCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.kind().fmt(f)
    }
}
impl fmt::Display for NowRequestCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for NowRequestCause {}

pub struct NowProtocolCause(serde_json::Error);
impl fmt::Debug for NowProtocolCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.classify().fmt(f)
    }
}
impl fmt::Display for NowProtocolCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for NowProtocolCause {}

#[derive(Debug)]
pub enum NowSubscriptionError {
    Admission(NowAdmissionCause),
    WebAuthentication(BrokerError),
    Request(NowRequestCause),
    Transport(WebSocketError),
    Http { status: u16, source: WebSocketError },
    Protocol(NowProtocolCause),
    InvalidInput,
    InvalidProtocol,
    Limit,
    Allocation,
    DifferentOriginal,
    WrongPurpose,
    Remote(NowRemoteCode),
    RecoveryRequired(NowRecoveryCause),
    Closed(u16),
}
impl fmt::Display for NowSubscriptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Now {self:?}")
    }
}
impl Error for NowSubscriptionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Admission(error) => Some(error),
            Self::WebAuthentication(error) => Some(error),
            Self::Request(error) => Some(error),
            Self::Transport(error) | Self::Http { source: error, .. } => Some(error),
            Self::Protocol(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ServiceUseError> for NowSubscriptionError {
    fn from(error: ServiceUseError) -> Self {
        Self::Admission(NowAdmissionCause(error))
    }
}
impl From<io::Error> for NowSubscriptionError {
    fn from(error: io::Error) -> Self {
        Self::Request(NowRequestCause(error))
    }
}
impl From<serde_json::Error> for NowSubscriptionError {
    fn from(error: serde_json::Error) -> Self {
        Self::Protocol(NowProtocolCause(error))
    }
}
impl From<WebSocketError> for NowSubscriptionError {
    fn from(error: WebSocketError) -> Self {
        if let WebSocketError::Sdk(cause) = &error
            && let Some(status) = cause.http_status()
        {
            return Self::Http {
                status,
                source: error,
            };
        }
        Self::Transport(error)
    }
}

/// Owned, validated remote DATA with no public constructor or Debug/Clone.
pub struct RemoteNowSnapshot {
    project_id: ProjectId,
    instances: Vec<RemoteNowInstance>,
}
impl RemoteNowSnapshot {
    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }
    pub fn instances(&self) -> &[RemoteNowInstance] {
        &self.instances
    }
}
impl Serialize for RemoteNowSnapshot {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut fields = serializer.serialize_struct("RemoteNowSnapshot", 3)?;
        fields.serialize_field("type", "snapshot")?;
        fields.serialize_field("project_id", &self.project_id)?;
        fields.serialize_field("instances", &self.instances)?;
        fields.end()
    }
}

#[derive(Serialize)]
pub struct RemoteNowInstance {
    instance_id: String,
    version: String,
    now: NowSnapshotData,
}
impl RemoteNowInstance {
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn now(&self) -> &NowSnapshotData {
        &self.now
    }
}
impl<'de> Deserialize<'de> for RemoteNowInstance {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            instance_id: String,
            version: String,
            now: NowSnapshotData,
        }
        struct Object;
        impl<'de> Visitor<'de> for Object {
            type Value = RemoteNowInstance;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a Now instance object")
            }
            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                let fields = Fields::deserialize(de::value::MapAccessDeserializer::new(map))?;
                if !valid_instance(&fields.instance_id) || !valid_version(&fields.version) {
                    return Err(de::Error::custom("invalid Now channel data"));
                }
                Ok(RemoteNowInstance {
                    instance_id: fields.instance_id,
                    version: fields.version,
                    now: fields.now,
                })
            }
        }
        deserializer.deserialize_map(Object)
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Incoming {
    Welcome {
        heartbeat_timeout_ns: String,
    },
    Snapshot {
        project_id: ProjectId,
        instances: Vec<RemoteNowInstance>,
    },
    Error {
        code: NowRemoteCode,
    },
}

#[derive(Serialize)]
struct Subscribe<'a> {
    op: &'static str,
    project_id: ProjectId,
    #[serde(skip_serializing_if = "Option::is_none")]
    instance_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'a str>,
}

#[derive(Serialize)]
struct Heartbeat {
    op: &'static str,
}

/// Borrows the caller-owned real lease. It spawns no task and caches no old view.
pub struct NowSubscription<'op, 'auth> {
    authority: SubscriptionAuthority<'op, 'auth>,
    original: &'op RequestContext<'auth>,
    config: NowSubscriptionConfig<'op>,
    socket: Option<BoundedWebSocket<'op>>,
    heartbeat: Duration,
    next_heartbeat: Instant,
}

#[derive(Clone, Copy)]
enum SubscriptionAuthority<'op, 'auth> {
    Service(&'op ServiceCredentialLease<'auth>),
    Web {
        caller: &'op AuthenticatedWebCaller<'auth>,
        verifier: &'op BrokerVerifier,
    },
}
impl SubscriptionAuthority<'_, '_> {
    fn check(self, original: &RequestContext<'_>) -> Result<(), NowSubscriptionError> {
        original.snapshot().check()?;
        match self {
            Self::Service(lease) => {
                lease.check()?;
                if !Arc::ptr_eq(lease.original()?.snapshot(), original.snapshot()) {
                    return Err(NowSubscriptionError::DifferentOriginal);
                }
            }
            Self::Web { caller, verifier } => caller
                .check(verifier, BrokerRequestContext::new(original.snapshot()))
                .map_err(NowSubscriptionError::WebAuthentication)?,
        }
        original.snapshot().check()?;
        Ok(())
    }
}

/// Native callers hold cache.get + native_credential; Local callers explicitly
/// hold credential(). Both must capture PresenceRead through the owning issuer.
pub async fn start_now_subscription<'op, 'auth>(
    client: &WebSocketClient,
    lease: &'op ServiceCredentialLease<'auth>,
    original: &'op RequestContext<'auth>,
    config: NowSubscriptionConfig<'op>,
) -> Result<NowSubscription<'op, 'auth>, NowSubscriptionError> {
    let authority = SubscriptionAuthority::Service(lease);
    authority.check(original)?;
    let ServiceBindingView::Broker {
        purpose: ServicePurpose::PresenceRead,
        endpoint,
        ..
    } = lease.binding()?
    else {
        return Err(NowSubscriptionError::WrongPurpose);
    };
    let raw = lease
        .credential()?
        .ok_or(NowSubscriptionError::WrongPurpose)?;
    start_authorized(client, authority, endpoint, raw, original, config).await
}

/// A cookie-authenticated Web transport retains its verified caller and Original.
/// The Broker independently admits PROJECT PresenceRead and masks module data.
pub async fn start_web_now_subscription<'op, 'auth>(
    client: &WebSocketClient,
    caller: &'op AuthenticatedWebCaller<'auth>,
    verifier: &'op BrokerVerifier,
    original: &'op RequestContext<'auth>,
    config: NowSubscriptionConfig<'op>,
) -> Result<NowSubscription<'op, 'auth>, NowSubscriptionError> {
    start_authorized(
        client,
        SubscriptionAuthority::Web { caller, verifier },
        verifier.endpoint(),
        caller.bearer_for_broker(),
        original,
        config,
    )
    .await
}

async fn start_authorized<'op, 'auth>(
    client: &WebSocketClient,
    authority: SubscriptionAuthority<'op, 'auth>,
    endpoint: &str,
    raw: &str,
    original: &'op RequestContext<'auth>,
    config: NowSubscriptionConfig<'op>,
) -> Result<NowSubscription<'op, 'auth>, NowSubscriptionError> {
    authority.check(original)?;
    if config
        .instance_id
        .is_some_and(|value| !valid_instance(value))
        || config.version.is_some_and(|value| !valid_version(value))
    {
        return Err(NowSubscriptionError::InvalidInput);
    }
    let mut destination =
        url::Url::parse(endpoint).map_err(|_| NowSubscriptionError::InvalidInput)?;
    if destination.query().is_some() || destination.fragment().is_some() {
        return Err(NowSubscriptionError::InvalidInput);
    }
    destination
        .path_segments_mut()
        .map_err(|_| NowSubscriptionError::InvalidInput)?
        .pop_if_empty()
        .extend(["v1", "realtime"]);
    let bearer = Zeroizing::new(format!("Bearer {raw}"));
    let mut value =
        HeaderValue::from_str(&bearer).map_err(|_| NowSubscriptionError::InvalidInput)?;
    value.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, value);
    let subscribe = encode_command(
        &Subscribe {
            op: "subscribe",
            project_id: config.project_id,
            instance_id: config.instance_id,
            version: config.version,
        },
        original,
        config.command_bytes,
    )?;
    authority.check(original)?;
    let socket = client
        .connect(
            destination.as_str(),
            headers,
            NetworkRequestContext::with_deadline(original.deadline(), original.cancellation()),
        )
        .await;
    authority.check(original)?;
    let mut socket = socket?;
    let sent = socket.send(Message::Text(subscribe)).await;
    authority.check(original)?;
    sent?;
    // The server cannot send welcome until the first subscribe was admitted.
    let heartbeat = loop {
        authority.check(original)?;
        let message = socket.receive().await;
        authority.check(original)?;
        match message? {
            WebSocketRead::Message(Message::Text(text)) => {
                match decode(&text, config.snapshot_bytes)? {
                    Incoming::Welcome {
                        heartbeat_timeout_ns,
                    } => break cadence(&heartbeat_timeout_ns)?,
                    Incoming::Error { code } => return Err(notice(code)),
                    _ => return Err(NowSubscriptionError::InvalidProtocol),
                }
            }
            WebSocketRead::Message(Message::Ping(_) | Message::Pong(_)) => (),
            WebSocketRead::Message(Message::Close { code, .. }) => return Err(closed(code)),
            WebSocketRead::Closed => return Err(NowSubscriptionError::Closed(1000)),
            _ => return Err(NowSubscriptionError::InvalidProtocol),
        }
    };
    let next_heartbeat = Instant::now()
        .checked_add(heartbeat)
        .ok_or(NowSubscriptionError::InvalidProtocol)?;
    authority.check(original)?;
    Ok(NowSubscription {
        authority,
        original,
        config,
        socket: Some(socket),
        heartbeat,
        next_heartbeat,
    })
}

/// Decode one complete remote DATA snapshot for an HTTP/NDJSON consumer.
/// This does not authenticate the transport or grant local PROJECT authority.
pub fn decode_now_snapshot(
    bytes: &[u8],
    expected_project: ProjectId,
    instance_filter: Option<&str>,
    version_filter: Option<&str>,
    snapshot_bytes: NonZeroUsize,
) -> Result<RemoteNowSnapshot, NowSubscriptionError> {
    if instance_filter.is_some_and(|value| !valid_instance(value))
        || version_filter.is_some_and(|value| !valid_version(value))
    {
        return Err(NowSubscriptionError::InvalidInput);
    }
    let Incoming::Snapshot {
        project_id,
        instances,
    } = decode_bytes(bytes, snapshot_bytes)?
    else {
        return Err(NowSubscriptionError::InvalidProtocol);
    };
    checked_snapshot(
        project_id,
        instances,
        expected_project,
        instance_filter,
        version_filter,
    )
}

impl NowSubscription<'_, '_> {
    /// Each result replaces the whole view. Every error withdraws the old view and
    /// ends this owner; callers must remove their previous displayed snapshot.
    pub async fn next(&mut self) -> Result<RemoteNowSnapshot, NowSubscriptionError> {
        let mut socket = self
            .socket
            .take()
            .ok_or(NowSubscriptionError::Closed(1000))?;
        loop {
            self.authority.check(self.original)?;
            let received = socket.receive_until(self.next_heartbeat).await;
            self.authority.check(self.original)?;
            match received? {
                WebSocketRead::Wakeup => {
                    let heartbeat = encode_command(
                        &Heartbeat { op: "heartbeat" },
                        self.original,
                        self.config.command_bytes,
                    )?;
                    self.authority.check(self.original)?;
                    let sent = socket.send(Message::Text(heartbeat)).await;
                    self.authority.check(self.original)?;
                    sent?;
                    self.next_heartbeat = Instant::now()
                        .checked_add(self.heartbeat)
                        .ok_or(NowSubscriptionError::InvalidProtocol)?;
                }
                WebSocketRead::Message(Message::Text(text)) => {
                    match decode(&text, self.config.snapshot_bytes)? {
                        Incoming::Snapshot {
                            project_id,
                            instances,
                        } => {
                            let snapshot = checked_snapshot(
                                project_id,
                                instances,
                                self.config.project_id,
                                self.config.instance_id,
                                self.config.version,
                            )?;
                            self.authority.check(self.original)?;
                            self.socket = Some(socket);
                            return Ok(snapshot);
                        }
                        Incoming::Error { code } => return Err(notice(code)),
                        _ => return Err(NowSubscriptionError::InvalidProtocol),
                    }
                }
                WebSocketRead::Message(Message::Ping(_) | Message::Pong(_)) => (),
                WebSocketRead::Message(Message::Close { code, .. }) => return Err(closed(code)),
                WebSocketRead::Closed => return Err(NowSubscriptionError::Closed(1000)),
                _ => return Err(NowSubscriptionError::InvalidProtocol),
            }
        }
    }

    /// Consuming shutdown closes only this native socket; no Graphit jobs exist.
    pub async fn shutdown(mut self) -> Result<(), NowSubscriptionError> {
        let Some(socket) = self.socket.take() else {
            return Ok(());
        };
        self.authority.check(self.original)?;
        let result = socket.close(CloseCode::Normal).await;
        self.authority.check(self.original)?;
        result.map_err(Into::into)
    }
}

fn decode(text: &str, limit: NonZeroUsize) -> Result<Incoming, NowSubscriptionError> {
    decode_bytes(text.as_bytes(), limit)
}
fn decode_bytes(bytes: &[u8], limit: NonZeroUsize) -> Result<Incoming, NowSubscriptionError> {
    if bytes.len() > limit.get() {
        return Err(NowSubscriptionError::Limit);
    }
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct ObjectIncoming(#[serde(deserialize_with = "object")] Incoming);
    serde_json::from_slice::<ObjectIncoming>(bytes)
        .map(|value| value.0)
        .map_err(Into::into)
}

fn object<'de, D: Deserializer<'de>, T: Deserialize<'de>>(deserializer: D) -> Result<T, D::Error> {
    struct MapOnly<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for MapOnly<T> {
        type Value = T;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a Now protocol object")
        }
        fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
            T::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(MapOnly(std::marker::PhantomData))
}
fn notice(code: NowRemoteCode) -> NowSubscriptionError {
    if code == NowRemoteCode::Backpressure {
        NowSubscriptionError::RecoveryRequired(NowRecoveryCause::Notice(code))
    } else {
        NowSubscriptionError::Remote(code)
    }
}
fn closed(code: CloseCode) -> NowSubscriptionError {
    let code = u16::from(code);
    if code == 1013 {
        NowSubscriptionError::RecoveryRequired(NowRecoveryCause::Close(code))
    } else {
        NowSubscriptionError::Closed(code)
    }
}
fn valid_instance(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && text.as_bytes()[0].is_ascii_alphanumeric()
        && text.as_bytes()[text.len() - 1].is_ascii_alphanumeric()
}
fn valid_version(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().all(|c| !c.is_control())
}
fn cadence(text: &str) -> Result<Duration, NowSubscriptionError> {
    if text.is_empty() || text.starts_with('0') || !text.bytes().all(|c| c.is_ascii_digit()) {
        return Err(NowSubscriptionError::InvalidProtocol);
    }
    let timeout = text
        .parse::<u128>()
        .map_err(|_| NowSubscriptionError::InvalidProtocol)?;
    let half = timeout / 2;
    if half == 0 || half >= timeout {
        return Err(NowSubscriptionError::InvalidProtocol);
    }
    let seconds =
        u64::try_from(half / 1_000_000_000).map_err(|_| NowSubscriptionError::InvalidProtocol)?;
    // Verify that the whole advertised duration is representable as well.
    u64::try_from(timeout / 1_000_000_000).map_err(|_| NowSubscriptionError::InvalidProtocol)?;
    Ok(Duration::new(seconds, (half % 1_000_000_000) as u32))
}
fn checked_snapshot(
    project_id: ProjectId,
    instances: Vec<RemoteNowInstance>,
    expected_project: ProjectId,
    instance_filter: Option<&str>,
    version_filter: Option<&str>,
) -> Result<RemoteNowSnapshot, NowSubscriptionError> {
    if project_id != expected_project {
        return Err(NowSubscriptionError::InvalidProtocol);
    }
    let project_text = project_id.to_string();
    let mut previous = None;
    for instance in &instances {
        let key = (instance.instance_id.as_str(), instance.version.as_str());
        if previous.is_some_and(|previous| previous >= key)
            || instance.now.project_id() != project_text
            || instance_filter.is_some_and(|filter| filter != instance.instance_id)
            || version_filter.is_some_and(|filter| filter != instance.version)
        {
            return Err(NowSubscriptionError::InvalidProtocol);
        }
        previous = Some(key);
    }
    Ok(RemoteNowSnapshot {
        project_id,
        instances,
    })
}

struct CommandWriter<'a, 'r> {
    original: &'a RequestContext<'r>,
    limit: usize,
    bytes: Vec<u8>,
    error: Option<NowSubscriptionError>,
}
impl Write for CommandWriter<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = (|| {
            if self.error.is_some() {
                return Err(NowSubscriptionError::Limit);
            }
            self.original.snapshot().check()?;
            self.bytes
                .len()
                .checked_add(bytes.len())
                .filter(|length| *length <= self.limit)
                .ok_or(NowSubscriptionError::Limit)?;
            self.bytes
                .try_reserve_exact(bytes.len())
                .map_err(|_| NowSubscriptionError::Allocation)?;
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        })();
        match result {
            Ok(length) => Ok(length),
            Err(error) => {
                if self.error.is_none() {
                    self.error = Some(error);
                }
                Err(io::Error::other("Now command rejected"))
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
fn encode_command(
    value: &impl Serialize,
    original: &RequestContext<'_>,
    limit: NonZeroUsize,
) -> Result<String, NowSubscriptionError> {
    original.snapshot().check()?;
    let mut writer = CommandWriter {
        original,
        limit: limit.get(),
        bytes: Vec::new(),
        error: None,
    };
    if let Err(error) = serde_json::to_writer(&mut writer, value) {
        return Err(writer.error.unwrap_or_else(|| error.into()));
    }
    original.snapshot().check()?;
    String::from_utf8(writer.bytes).map_err(|_| NowSubscriptionError::InvalidProtocol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use graphit_hub::native::ContentLimits;

    fn n(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).unwrap()
    }
    fn original(signal: &Cancellation) -> RequestContext<'_> {
        RequestContext::new(
            ContentLimits {
                max_input_bytes: 65536,
                max_output_bytes: 65536,
                max_depth: 32,
                max_nodes: 4096,
                max_scalar_bytes: 4096,
                max_collection_entries: 64,
            },
            signal,
            Instant::now() + Duration::from_secs(3),
        )
        .unwrap()
    }
    const PROJECT: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    #[test]
    fn subscribe_omits_each_filter_independently_and_writer_bounds_escaped_bytes() {
        let signal = Cancellation::default();
        let original = original(&signal);
        let project = PROJECT.parse().unwrap();
        for (instance_id, version) in [
            (None, None),
            (Some("native"), None),
            (None, Some(" branch/ação\"\\ ")),
        ] {
            let value = Subscribe {
                op: "subscribe",
                project_id: project,
                instance_id,
                version,
            };
            let expected = serde_json::to_string(&value).unwrap();
            let actual = encode_command(&value, &original, n(expected.len())).unwrap();
            assert_eq!(actual, expected);
            assert!(!actual.contains("null"));
            assert_eq!(actual.contains("instance_id"), instance_id.is_some());
            assert_eq!(actual.contains("version"), version.is_some());
            assert!(matches!(
                encode_command(&value, &original, n(expected.len() - 1)),
                Err(NowSubscriptionError::Limit)
            ));
        }
        let mut writer = CommandWriter {
            original: &original,
            limit: 4,
            bytes: Vec::new(),
            error: None,
        };
        writer.write_all(b"1234").unwrap();
        let capacity = writer.bytes.capacity();
        assert!(writer.write_all(b"5").is_err());
        signal.cancel();
        assert!(writer.write_all(&[]).is_err());
        assert_eq!(writer.bytes, b"1234");
        assert_eq!(writer.bytes.capacity(), capacity);
        assert!(matches!(writer.error, Some(NowSubscriptionError::Limit)));
    }

    #[test]
    fn welcome_cadence_is_exact_checked_half_without_a_new_interval_or_ttl() {
        assert_eq!(cadence("3").unwrap(), Duration::from_nanos(1));
        assert_eq!(
            cadence("600000001").unwrap(),
            Duration::from_nanos(300000000)
        );
        for invalid in [
            "",
            "0",
            "1",
            "01",
            "-1",
            "+2",
            "2.0",
            "2e9",
            " 2",
            "340282366920938463463374607431768211456",
        ] {
            assert!(cadence(invalid).is_err(), "{invalid}");
        }
        assert!(cadence(&format!("{}000000000", u128::from(u64::MAX) + 1)).is_err());
    }

    #[test]
    fn private_envelopes_reject_nonobjects_duplicates_unknowns_and_invented_codes() {
        for text in [
            r#"["welcome","600"]"#,
            r#"{"type":"welcome","heartbeat_timeout_ns":"600","type":"welcome"}"#,
            r#"{"type":"welcome","heartbeat_timeout_ns":"600","extra":1}"#,
            r#"{"type":"error","code":"unavailable"}"#,
            r#"{"type":"error","code":"closed","code":"closed"}"#,
            r#"{"type":"snapshot","project_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","instances":[["native","main",{}]]}"#,
        ] {
            assert!(decode(text, n(65536)).is_err());
        }
        for (wire, expected) in [
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
            let text = format!(r#"{{"type":"error","code":"{wire}"}}"#);
            let Incoming::Error { code } = decode(&text, n(text.len())).unwrap() else {
                panic!("expected actual Broker notice");
            };
            assert_eq!(code, expected);
            match notice(code) {
                NowSubscriptionError::RecoveryRequired(NowRecoveryCause::Notice(actual)) => {
                    assert_eq!(actual, NowRemoteCode::Backpressure);
                    assert_eq!(expected, actual);
                }
                NowSubscriptionError::Remote(actual) => {
                    assert_eq!(actual, expected);
                    assert_ne!(actual, NowRemoteCode::Backpressure);
                }
                other => panic!("incorrect actual Broker cause {other:?}"),
            }
        }
        assert!(matches!(
            decode(r#"{"type":"error","code":"backpressure"}"#, n(65536)).unwrap(),
            Incoming::Error {
                code: NowRemoteCode::Backpressure
            }
        ));
        assert!(matches!(
            notice(NowRemoteCode::Backpressure),
            NowSubscriptionError::RecoveryRequired(NowRecoveryCause::Notice(
                NowRemoteCode::Backpressure
            ))
        ));
        assert!(matches!(
            closed(CloseCode::Again),
            NowSubscriptionError::RecoveryRequired(NowRecoveryCause::Close(1013))
        ));
        let config = NowSubscriptionConfig {
            project_id: PROJECT.parse().unwrap(),
            instance_id: None,
            version: Some("branch/ação"),
            command_bytes: n(65536),
            snapshot_bytes: n(65536),
        };
        let snapshot = checked_snapshot(
            config.project_id,
            Vec::new(),
            config.project_id,
            config.instance_id,
            config.version,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&snapshot).unwrap(),
            format!(r#"{{"type":"snapshot","project_id":"{PROJECT}","instances":[]}}"#)
        );
        assert!(matches!(
            decode("{}", n(1)),
            Err(NowSubscriptionError::Limit)
        ));
    }

    fn snapshot_data(pairs: &[(&str, &str)]) -> serde_json::Value {
        let section = serde_json::json!({
            "availability": "no_corpus", "items": [], "total": 0,
            "has_more": false, "error": null
        });
        let instances: Vec<_> = pairs
            .iter()
            .map(|(instance, version)| {
                serde_json::json!({
                    "instance_id": instance, "version": version,
                    "now": {
                        "project_id": PROJECT, "generated_at": "9007199254740993001",
                        "sections": {
                            "tasks": section, "sessions": section,
                            "memories": section, "knowledge": section
                        }
                    }
                })
            })
            .collect();
        serde_json::json!({"type":"snapshot", "project_id":PROJECT, "instances":instances})
    }

    #[test]
    fn byte_snapshot_decoder_caps_before_parse_and_preserves_exact_data() {
        let value = snapshot_data(&[("native", "branch/ação")]);
        let bytes = serde_json::to_vec(&value).unwrap();
        let project = PROJECT.parse().unwrap();
        let actual = decode_now_snapshot(&bytes, project, None, None, n(bytes.len())).unwrap();
        assert_eq!(actual.project_id(), project);
        assert_eq!(actual.instances()[0].instance_id(), "native");
        assert_eq!(actual.instances()[0].version(), "branch/ação");
        assert_eq!(
            actual.instances()[0].now().generated_at(),
            "9007199254740993001"
        );
        assert_eq!(serde_json::to_value(&actual).unwrap(), value);
        assert!(matches!(
            decode_now_snapshot(&bytes, project, None, None, n(bytes.len() - 1)),
            Err(NowSubscriptionError::Limit)
        ));
        assert!(matches!(
            decode_now_snapshot(&[0xff; 5], project, None, None, n(4)),
            Err(NowSubscriptionError::Limit)
        ));
        assert!(matches!(
            decode_now_snapshot(&[0xff], project, None, None, n(1)),
            Err(NowSubscriptionError::Protocol(_))
        ));
    }

    #[test]
    fn byte_snapshot_decoder_rejects_non_snapshot_envelopes_and_invalid_nested_data() {
        let project = PROJECT.parse().unwrap();
        for bytes in [
            &b"[]"[..],
            &b"PRIVATE_MALFORMED_JSON"[..],
            &br#"{"type":"welcome","heartbeat_timeout_ns":"600"}"#[..],
            &br#"{"type":"error","code":"forbidden"}"#[..],
            &br#"{"type":"snapshot","type":"snapshot","project_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","instances":[]}"#[..],
            &br#"{"type":"snapshot","project_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","instances":[],"extra":"PRIVATE_DATA"}"#[..],
        ] {
            let error = decode_now_snapshot(bytes, project, None, None, n(65536)).err().unwrap();
            assert!(!format!("{error:?} {error}").contains("PRIVATE"));
        }
        for change in 0..8 {
            let mut value = snapshot_data(&[("native", "main")]);
            match change {
                0 => {
                    value["instances"][0]["now"]["project_id"] =
                        serde_json::json!("01ARZ3NDEKTSV4RRFFQ69G5FAW")
                }
                1 => value["instances"][0]["now"]["extra"] = serde_json::json!("PRIVATE_DATA"),
                2 => {
                    value["instances"][0]["now"]["sections"]["tasks"]["total"] =
                        serde_json::json!(9007199254740992_u64)
                }
                3 => {
                    value["instances"][0]["now"]["sections"]["tasks"]["has_more"] =
                        serde_json::json!(true)
                }
                4 => value["instances"][0]["now"]["sections"]
                    .as_object_mut()
                    .unwrap()
                    .remove("knowledge")
                    .map(|_| ())
                    .unwrap(),
                5 => value["instances"][0] = serde_json::json!(["native", "main", {}]),
                6 => value["instances"][0]["instance_id"] = serde_json::json!("bad/id"),
                7 => value["instances"][0]["version"] = serde_json::json!("main\n"),
                _ => unreachable!(),
            }
            let bytes = serde_json::to_vec(&value).unwrap();
            assert!(decode_now_snapshot(&bytes, project, None, None, n(65536)).is_err());
        }
        let valid = serde_json::to_string(&snapshot_data(&[("native", "main")])).unwrap();
        let duplicate = valid.replace(
            "\"instance_id\":\"native\"",
            "\"instance_id\":\"native\",\"instance_id\":\"native\"",
        );
        assert_ne!(duplicate, valid);
        assert!(decode_now_snapshot(duplicate.as_bytes(), project, None, None, n(65536)).is_err());
    }

    #[test]
    fn byte_snapshot_decoder_checks_project_strict_pairs_and_independent_literal_filters() {
        let project = PROJECT.parse().unwrap();
        let foreign = "01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap();
        let valid = serde_json::to_vec(&snapshot_data(&[
            ("a", "branch/ação"),
            ("a", "main"),
            ("b", "main"),
        ]))
        .unwrap();
        assert!(decode_now_snapshot(&valid, foreign, None, None, n(65536)).is_err());
        assert!(decode_now_snapshot(&valid, project, None, None, n(65536)).is_ok());
        for pairs in [
            vec![("b", "main"), ("a", "main")],
            vec![("a", "main"), ("a", "branch/ação")],
            vec![("a", "main"), ("a", "main")],
        ] {
            let bytes = serde_json::to_vec(&snapshot_data(&pairs)).unwrap();
            assert!(decode_now_snapshot(&bytes, project, None, None, n(65536)).is_err());
        }
        for (instance, version, pairs) in [
            (Some("a"), None, vec![("a", "branch/ação"), ("a", "main")]),
            (None, Some("main"), vec![("a", "main"), ("b", "main")]),
            (Some("a"), Some("branch/ação"), vec![("a", "branch/ação")]),
            (
                None,
                Some(" branch/ação\"\\ "),
                vec![("a", " branch/ação\"\\ ")],
            ),
        ] {
            let bytes = serde_json::to_vec(&snapshot_data(&pairs)).unwrap();
            assert!(decode_now_snapshot(&bytes, project, instance, version, n(65536)).is_ok());
        }
        assert!(decode_now_snapshot(&valid, project, Some("a"), None, n(65536)).is_err());
        assert!(decode_now_snapshot(&valid, project, None, Some("main"), n(65536)).is_err());
        for filter in ["", "*", ".a", "a.", "bad/id", "ação"] {
            assert!(matches!(
                decode_now_snapshot(&valid, project, Some(filter), None, n(65536)),
                Err(NowSubscriptionError::InvalidInput)
            ));
        }
        for filter in ["", " \t", "main\n", "\0"] {
            assert!(matches!(
                decode_now_snapshot(&valid, project, None, Some(filter), n(65536)),
                Err(NowSubscriptionError::InvalidInput)
            ));
        }
        assert!(decode_now_snapshot(&valid, project, None, Some("*"), n(65536)).is_err());
    }
}
