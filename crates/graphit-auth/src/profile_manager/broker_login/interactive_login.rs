use super::*;
use graphit_core::file_io::SnapshotContext;
use secrecy::zeroize::Zeroizing;
use std::{future::Future, io, net::SocketAddr, time::Instant};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const RECEIVED: &[u8] =
    b"HTTP/1.1 204 No Content\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
const REJECTED: &[u8] =
    b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";

/// Input limits for the local callback listener.
#[derive(Clone, Copy)]
pub struct InteractiveLoginLimits {
    pub head_bytes: usize,
    pub header_entries: usize,
    pub target_bytes: usize,
    pub callback_bytes: usize,
}

pub struct InteractiveLoginConfig<'a> {
    pub bind: SocketAddr,
    pub callback_path: &'a str,
    pub limits: InteractiveLoginLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractiveTransportError {
    InvalidPolicy,
    Limit,
    Allocation,
    Io,
    Cancelled,
    Deadline,
    InvalidRequest,
}

/// Closed transport errors or the exact original managed-login error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractiveLoginError {
    Transport(InteractiveTransportError),
    Broker(BrokerLoginError),
}
impl fmt::Display for InteractiveLoginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Broker(error) => error.fmt(f),
            Self::Transport(_) => f.write_str("native Broker callback transport failed"),
        }
    }
}
impl std::error::Error for InteractiveLoginError {}
impl From<InteractiveTransportError> for InteractiveLoginError {
    fn from(value: InteractiveTransportError) -> Self {
        Self::Transport(value)
    }
}
impl From<BrokerLoginError> for InteractiveLoginError {
    fn from(value: BrokerLoginError) -> Self {
        Self::Broker(value)
    }
}

struct Transport<'a> {
    // Physical descriptors and protected buffers drop before the managed pending owner.
    stream: Option<TcpStream>,
    listener: Option<TcpListener>,
    callback: Option<Zeroizing<String>>,
    head: Zeroizing<Vec<u8>>,
    authority: String,
    origin: String,
    path: &'a str,
    limits: InteractiveLoginLimits,
}
impl Transport<'_> {
    fn close(&mut self) {
        drop(self.stream.take());
        drop(self.listener.take());
    }
}

/// One owned listener and the exact native pending login. No browser is spawned.
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedInteractiveBrokerLogin;
/// fn duplicate(value: ManagedInteractiveBrokerLogin<'_>) { drop(value); drop(value); }
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedInteractiveBrokerLogin;
/// fn fields(value: ManagedInteractiveBrokerLogin<'_>) { let _ = value.pending; }
/// ```
pub struct ManagedInteractiveBrokerLogin<'a> {
    transport: Transport<'a>,
    pending: ManagedBrokerLogin<'a>,
}

/// A structurally received callback, not an authenticated identity.
/// ```compile_fail
/// use graphit_auth::profile_manager::ReceivedInteractiveBrokerLogin;
/// fn duplicate(value: ReceivedInteractiveBrokerLogin<'_>) { drop(value); drop(value); }
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ReceivedInteractiveBrokerLogin;
/// fn fields(value: ReceivedInteractiveBrokerLogin<'_>) { let _ = value.callback; }
/// ```
/// ```no_run
/// use graphit_auth::profile_manager::{ManagedInteractiveBrokerLogin, ReceivedInteractiveBrokerLogin};
/// use graphit_core::auth_document::publication::AuthDocumentPublication;
/// async fn finish(value: ManagedInteractiveBrokerLogin<'_>) -> Result<AuthDocumentPublication, Box<dyn std::error::Error>> {
///     let received: ReceivedInteractiveBrokerLogin<'_> = value.receive().await?;
///     let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
///     let now = i64::try_from(seconds)?;
///     Ok(received.publish(now).await?)
/// }
/// ```
pub struct ReceivedInteractiveBrokerLogin<'a> {
    callback: Zeroizing<String>,
    pending: ManagedBrokerLogin<'a>,
}
impl ReceivedInteractiveBrokerLogin<'_> {
    /// Obtain `now` after receiving the callback. Forward the exact existing publication/error.
    pub async fn publish(self, now: i64) -> Result<AuthDocumentPublication, BrokerLoginError> {
        let manager = self.pending.managed.manager;
        manager
            .finish_broker_login(self.pending, &self.callback, now)
            .await
    }
}

fn finite_deadline(context: &SnapshotContext) -> Result<Instant, InteractiveTransportError> {
    context
        .required_deadline()
        .map_err(|error| match error.kind() {
            io::ErrorKind::Interrupted => InteractiveTransportError::Cancelled,
            io::ErrorKind::TimedOut => InteractiveTransportError::Deadline,
            _ => InteractiveTransportError::InvalidPolicy,
        })
}
fn check(context: &SnapshotContext) -> Result<(), InteractiveTransportError> {
    finite_deadline(context).map(|_| ())
}
async fn original_io<T>(
    context: &SnapshotContext,
    future: impl Future<Output = io::Result<T>>,
) -> Result<T, InteractiveTransportError> {
    let deadline = finite_deadline(context)?;
    tokio::select! {
        biased;
        _ = context.cancellation().cancelled() => Err(InteractiveTransportError::Cancelled),
        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Err(InteractiveTransportError::Deadline),
        result = future => result.map_err(|_| InteractiveTransportError::Io),
    }
}

fn validate(config: &InteractiveLoginConfig<'_>) -> Result<usize, InteractiveTransportError> {
    let limits = config.limits;
    for value in [
        limits.head_bytes,
        limits.header_entries,
        limits.target_bytes,
        limits.callback_bytes,
    ] {
        if value == 0 || value > isize::MAX as usize {
            return Err(InteractiveTransportError::InvalidPolicy);
        }
    }
    let head_capacity = limits
        .head_bytes
        .checked_add(1)
        .ok_or(InteractiveTransportError::InvalidPolicy)?;
    let header_capacity = limits
        .header_entries
        .checked_mul(std::mem::size_of::<httparse::Header<'static>>())
        .ok_or(InteractiveTransportError::InvalidPolicy)?;
    if head_capacity > isize::MAX as usize
        || header_capacity > isize::MAX as usize
        || !config.bind.ip().is_loopback()
        || config.callback_path.len() > limits.target_bytes
    {
        return Err(InteractiveTransportError::InvalidPolicy);
    }
    let path: http::Uri = config
        .callback_path
        .parse()
        .map_err(|_| InteractiveTransportError::InvalidPolicy)?;
    if !config.callback_path.starts_with('/')
        || config.callback_path.starts_with("//")
        || path.scheme().is_some()
        || path.authority().is_some()
        || path.query().is_some()
        || config.callback_path.contains('#')
        || path.path() != config.callback_path
    {
        return Err(InteractiveTransportError::InvalidPolicy);
    }
    // Literal IP, optional IPv6 brackets, colon and maximal decimal port; no DNS.
    let origin_bound = 7usize
        .checked_add(config.bind.ip().to_string().len())
        .and_then(|v| v.checked_add(if config.bind.is_ipv6() { 8 } else { 6 }))
        .ok_or(InteractiveTransportError::InvalidPolicy)?;
    if origin_bound
        .checked_add(limits.target_bytes)
        .is_none_or(|v| v > limits.callback_bytes)
    {
        return Err(InteractiveTransportError::InvalidPolicy);
    }
    Ok(head_capacity)
}

impl NativeProfileManager<'_> {
    /// Compose the actual managed OAuth flow with one explicitly bounded literal-loopback transport.
    pub async fn begin_interactive_broker_login<'a>(
        &'a self,
        mut managed: ManagedAuthSnapshot<'a>,
        oauth: &'a NativeOAuth<'a>,
        selection: BrokerLoginSelection<'a>,
        config: InteractiveLoginConfig<'a>,
    ) -> Result<ManagedInteractiveBrokerLogin<'a>, InteractiveLoginError> {
        let head_capacity = validate(&config)?;
        // Public bounded config validation precedes admission. The shared actual gate
        // runs before owned head-buffer allocation/bind/discovery; begin rechecks after bind.
        admit_login(self, &mut managed, oauth, &selection)?;
        let original = managed.request.snapshot();
        let mut head = Zeroizing::new(Vec::new());
        head.try_reserve_exact(head_capacity)
            .map_err(|_| InteractiveTransportError::Allocation)?;
        head.resize(head_capacity, 0);
        let listener = original_io(original, TcpListener::bind(config.bind)).await?;
        let address = listener
            .local_addr()
            .map_err(|_| InteractiveTransportError::Io)?;
        let authority = if address.port() == 80 {
            if address.is_ipv6() {
                format!("[{}]", address.ip())
            } else {
                address.ip().to_string()
            }
        } else {
            address.to_string()
        };
        let origin = format!("http://{address}");
        let mut redirect = String::new();
        let length = origin
            .len()
            .checked_add(config.callback_path.len())
            .ok_or(InteractiveTransportError::Limit)?;
        if length > config.limits.callback_bytes {
            return Err(InteractiveTransportError::Limit.into());
        }
        redirect
            .try_reserve_exact(length)
            .map_err(|_| InteractiveTransportError::Allocation)?;
        redirect.push_str(&origin);
        redirect.push_str(config.callback_path);
        let transport = Transport {
            stream: None,
            listener: Some(listener),
            callback: None,
            head,
            authority,
            origin,
            path: config.callback_path,
            limits: config.limits,
        };
        // The existing delegate retains the pending native login during discovery.
        let pending = self
            .begin_broker_login(managed, oauth, selection, &redirect)
            .await?;
        Ok(ManagedInteractiveBrokerLogin { transport, pending })
    }
}

enum Parsed {
    Partial,
    Rejected(InteractiveTransportError),
    Callback(Zeroizing<String>),
}
fn parse_head(
    transport: &mut Transport<'_>,
    length: usize,
    original: &SnapshotContext,
) -> Result<Parsed, InteractiveTransportError> {
    check(original)?;
    let mut headers = Vec::new();
    headers
        .try_reserve_exact(transport.limits.header_entries)
        .map_err(|_| InteractiveTransportError::Allocation)?;
    headers.resize(transport.limits.header_entries, httparse::EMPTY_HEADER);
    let mut request = httparse::Request::new(&mut headers);
    let complete = match request.parse(&transport.head[..length]) {
        Ok(httparse::Status::Partial) => return Ok(Parsed::Partial),
        Ok(httparse::Status::Complete(end)) => end,
        Err(httparse::Error::TooManyHeaders) => {
            return Ok(Parsed::Rejected(InteractiveTransportError::Limit));
        }
        Err(_) => return Ok(Parsed::Rejected(InteractiveTransportError::InvalidRequest)),
    };
    if complete > transport.limits.head_bytes || complete != length {
        return Ok(Parsed::Rejected(InteractiveTransportError::InvalidRequest));
    }
    let Some(target) = request.path else {
        return Ok(Parsed::Rejected(InteractiveTransportError::InvalidRequest));
    };
    if target.len() > transport.limits.target_bytes {
        return Ok(Parsed::Rejected(InteractiveTransportError::Limit));
    }
    if request.method != Some("GET")
        || request.version != Some(1)
        || !target.starts_with('/')
        || target.starts_with("//")
        || target.contains('#')
        || target.split('?').next() != Some(transport.path)
    {
        return Ok(Parsed::Rejected(InteractiveTransportError::InvalidRequest));
    }
    let mut host = false;
    let mut content_length = false;
    for header in request.headers.iter() {
        if header.name.eq_ignore_ascii_case("host") {
            if host || header.value != transport.authority.as_bytes() {
                return Ok(Parsed::Rejected(InteractiveTransportError::InvalidRequest));
            }
            host = true;
        } else if header.name.eq_ignore_ascii_case("transfer-encoding")
            || header.name.eq_ignore_ascii_case("expect")
        {
            return Ok(Parsed::Rejected(InteractiveTransportError::InvalidRequest));
        } else if header.name.eq_ignore_ascii_case("content-length") {
            if content_length
                || header.value.is_empty()
                || !header.value.iter().all(|value| *value == b'0')
            {
                return Ok(Parsed::Rejected(InteractiveTransportError::InvalidRequest));
            }
            content_length = true;
        }
    }
    if !host {
        return Ok(Parsed::Rejected(InteractiveTransportError::InvalidRequest));
    }
    let total = transport
        .origin
        .len()
        .checked_add(target.len())
        .ok_or(InteractiveTransportError::Limit)?;
    if total > transport.limits.callback_bytes {
        return Ok(Parsed::Rejected(InteractiveTransportError::Limit));
    }
    check(original)?;
    let mut callback = Zeroizing::new(String::new());
    callback
        .try_reserve_exact(total)
        .map_err(|_| InteractiveTransportError::Allocation)?;
    callback.push_str(&transport.origin);
    callback.push_str(target);
    Ok(Parsed::Callback(callback))
}

async fn write_response(
    transport: &mut Transport<'_>,
    original: &SnapshotContext,
    bytes: &[u8],
) -> Result<(), InteractiveTransportError> {
    let stream = transport
        .stream
        .as_mut()
        .ok_or(InteractiveTransportError::Io)?;
    original_io(original, stream.write_all(bytes)).await
}
async fn receive_one(
    transport: &mut Transport<'_>,
    original: &SnapshotContext,
) -> Result<Option<InteractiveTransportError>, InteractiveTransportError> {
    let mut length = 0;
    loop {
        check(original)?;
        if length == transport.head.len() {
            return Err(InteractiveTransportError::Limit);
        }
        let stream = transport
            .stream
            .as_mut()
            .ok_or(InteractiveTransportError::Io)?;
        let read = original_io(original, stream.read(&mut transport.head[length..])).await?;
        if read == 0 {
            return Ok(Some(InteractiveTransportError::InvalidRequest));
        }
        length += read;
        if length > transport.limits.head_bytes {
            return Ok(Some(InteractiveTransportError::Limit));
        }
        match parse_head(transport, length, original)? {
            Parsed::Partial => continue,
            Parsed::Rejected(error) => return Ok(Some(error)),
            Parsed::Callback(callback) => {
                transport.callback = Some(callback);
                return Ok(None);
            }
        }
    }
}
impl<'a> ManagedInteractiveBrokerLogin<'a> {
    pub fn authorization_url(&mut self) -> Result<&str, BrokerLoginError> {
        self.pending.authorization_url()
    }
    pub async fn receive(
        mut self,
    ) -> Result<ReceivedInteractiveBrokerLogin<'a>, InteractiveLoginError> {
        let original = self.pending.managed.request.snapshot();
        loop {
            check(original)?;
            let listener = self
                .transport
                .listener
                .as_ref()
                .ok_or(InteractiveTransportError::Io)?;
            let (stream, _) = original_io(original, listener.accept()).await?;
            self.transport.stream = Some(stream);
            let rejected = receive_one(&mut self.transport, original).await?;
            write_response(
                &mut self.transport,
                original,
                if rejected.is_some() {
                    REJECTED
                } else {
                    RECEIVED
                },
            )
            .await?;
            drop(self.transport.stream.take());
            self.transport.head.fill(0);
            if rejected.is_some() {
                continue;
            }
            self.transport.close();
            let callback = self
                .transport
                .callback
                .take()
                .ok_or(InteractiveTransportError::Io)?;
            return Ok(ReceivedInteractiveBrokerLogin {
                callback,
                pending: self.pending,
            });
        }
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::{
        task::Poll,
        time::{Duration, Instant},
    };

    #[test]
    fn interactive_login_rejects_service_context_and_preserves_cancel_cause() {
        let original = SnapshotContext::service(Cancellation::default());
        assert_eq!(
            check(&original),
            Err(InteractiveTransportError::InvalidPolicy)
        );
        original.cancellation().cancel();
        assert_eq!(check(&original), Err(InteractiveTransportError::Cancelled));
    }
    fn limits() -> InteractiveLoginLimits {
        InteractiveLoginLimits {
            head_bytes: 1024,
            header_entries: 8,
            target_bytes: 512,
            callback_bytes: 1024,
        }
    }
    fn transport(listener: Option<TcpListener>, stream: Option<TcpStream>) -> Transport<'static> {
        let limits = limits();
        Transport {
            stream,
            listener,
            callback: None,
            head: Zeroizing::new(vec![0; limits.head_bytes + 1]),
            authority: "127.0.0.1:12345".into(),
            origin: "http://127.0.0.1:12345".into(),
            path: "/callback",
            limits,
        }
    }
    async fn pair() -> (TcpListener, TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (accepted, peer) = tokio::join!(
            listener.accept(),
            TcpStream::connect(listener.local_addr().unwrap())
        );
        (listener, accepted.unwrap().0, peer.unwrap())
    }
    #[tokio::test]
    async fn same_original_prewrite_denies_cancel_deadline_and_positive_real_response() {
        for variant in 0..3 {
            let (listener, stream, mut peer) = pair().await;
            let cancel = Cancellation::default();
            let deadline = if variant == 1 {
                Instant::now() - Duration::from_secs(1)
            } else {
                Instant::now() + Duration::from_secs(5)
            };
            let original = SnapshotContext::new(deadline, cancel.clone());
            if variant == 0 {
                cancel.cancel();
            }
            let mut transport = transport(Some(listener), Some(stream));
            let result = write_response(&mut transport, &original, RECEIVED).await;
            if variant == 2 {
                result.unwrap();
            } else {
                assert_eq!(
                    result,
                    Err(if variant == 0 {
                        InteractiveTransportError::Cancelled
                    } else {
                        InteractiveTransportError::Deadline
                    })
                );
            }
            transport.close();
            let mut actual = Vec::new();
            tokio::time::timeout(Duration::from_secs(5), peer.read_to_end(&mut actual))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(actual, if variant == 2 { RECEIVED } else { &[] });
        }
    }
    #[tokio::test]
    async fn genuinely_pending_accept_and_read_share_original_cancel_and_deadline() {
        for reading in [false, true] {
            for expiry in [false, true] {
                let cancel = Cancellation::default();
                let original = SnapshotContext::new(
                    Instant::now() + Duration::from_millis(if expiry { 100 } else { 5000 }),
                    cancel.clone(),
                );
                let independent = SnapshotContext::new(
                    Instant::now() + Duration::from_secs(5),
                    Cancellation::default(),
                );
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let (stream, mut peer) = if reading {
                    let (accepted, peer) =
                        tokio::join!(listener.accept(), TcpStream::connect(address));
                    (Some(accepted.unwrap().0), Some(peer.unwrap()))
                } else {
                    (None, None)
                };
                let mut transport = transport(Some(listener), stream);
                {
                    let operation = async {
                        if reading {
                            receive_one(&mut transport, &original).await.map(|_| ())
                        } else {
                            original_io(&original, transport.listener.as_ref().unwrap().accept())
                                .await
                                .map(|_| ())
                        }
                    };
                    let mut pending = Box::pin(operation);
                    std::future::poll_fn(|cx| {
                        assert!(matches!(pending.as_mut().poll(cx), Poll::Pending));
                        Poll::Ready(())
                    })
                    .await;
                    if !expiry {
                        cancel.cancel();
                    }
                    assert_eq!(
                        pending.await,
                        Err(if expiry {
                            InteractiveTransportError::Deadline
                        } else {
                            InteractiveTransportError::Cancelled
                        })
                    );
                }
                independent.check().unwrap();
                transport.close();
                let replacement = TcpListener::bind(address).await.unwrap();
                drop(replacement);
                if let Some(peer) = &mut peer {
                    let mut byte = [0];
                    assert_eq!(
                        tokio::time::timeout(Duration::from_secs(5), peer.read(&mut byte))
                            .await
                            .unwrap()
                            .unwrap(),
                        0
                    );
                }
            }
        }
    }
    #[test]
    fn explicit_policy_rejects_zero_overflow_nonloopback_and_nonpath_without_bind() {
        let valid = || InteractiveLoginConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            callback_path: "/callback",
            limits: limits(),
        };
        assert_eq!(validate(&valid()).unwrap(), 1025);
        let mut exact = valid();
        exact.limits.callback_bytes = 7 + "127.0.0.1".len() + 6 + exact.limits.target_bytes;
        validate(&exact).unwrap();
        exact.limits.callback_bytes -= 1;
        assert_eq!(
            validate(&exact),
            Err(InteractiveTransportError::InvalidPolicy)
        );
        for variant in 0..5 {
            let mut config = valid();
            match variant {
                0 => config.limits.head_bytes = 0,
                1 => config.limits.header_entries = usize::MAX,
                2 => config.bind = "0.0.0.0:0".parse().unwrap(),
                3 => config.callback_path = "/callback?x=1",
                _ => config.limits.callback_bytes = 1,
            }
            assert_eq!(
                validate(&config),
                Err(InteractiveTransportError::InvalidPolicy)
            );
        }
    }
    fn parse(
        transport: &mut Transport<'_>,
        bytes: &[u8],
    ) -> Result<Parsed, InteractiveTransportError> {
        transport.head[..bytes.len()].copy_from_slice(bytes);
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            Cancellation::default(),
        );
        parse_head(transport, bytes.len(), &original)
    }
    #[test]
    fn mature_parser_route_host_and_framing_controls_preserve_raw_callback() {
        let requests = [
            "POST /callback?code=c&state=s HTTP/1.1\r\nHost: 127.0.0.1:12345\r\n\r\n",
            "GET /wrong?code=c&state=s HTTP/1.1\r\nHost: 127.0.0.1:12345\r\n\r\n",
            "GET http://127.0.0.1:12345/callback?code=c HTTP/1.1\r\nHost: 127.0.0.1:12345\r\n\r\n",
            "GET /callback#fragment HTTP/1.1\r\nHost: 127.0.0.1:12345\r\n\r\n",
            "GET /callback HTTP/1.1\r\nHost: other\r\n\r\n",
            "GET /callback HTTP/1.1\r\nHost: 127.0.0.1:12345\r\nHost: 127.0.0.1:12345\r\n\r\n",
            "GET /callback HTTP/1.1\r\nHost: 127.0.0.1:12345\r\nTransfer-Encoding: chunked\r\n\r\n",
            "GET /callback HTTP/1.1\r\nHost: 127.0.0.1:12345\r\nExpect: 100-continue\r\n\r\n",
            "GET /callback HTTP/1.1\r\nHost: 127.0.0.1:12345\r\nContent-Length: 1\r\n\r\n",
            "GET /callback HTTP/1.1\r\nHost: 127.0.0.1:12345\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n",
        ];
        for request in requests {
            let mut transport = transport(None, None);
            assert!(matches!(
                parse(&mut transport, request.as_bytes()).unwrap(),
                Parsed::Rejected(_)
            ));
        }
        let mut transport = transport(None, None);
        let raw = "GET /callback?code=c%2Fraw&state=s HTTP/1.1\r\nHost: 127.0.0.1:12345\r\nContent-Length: 0\r\n\r\n";
        let Parsed::Callback(callback) = parse(&mut transport, raw.as_bytes()).unwrap() else {
            panic!("positive request rejected")
        };
        assert_eq!(
            callback.as_str(),
            "http://127.0.0.1:12345/callback?code=c%2Fraw&state=s"
        );
    }
    #[test]
    fn exact_target_and_header_caps_accept_then_plus_one_rejects() {
        let target = "/callback?code=c&state=s";
        let request = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:12345\r\n\r\n");
        let mut transport = transport(None, None);
        transport.limits.target_bytes = target.len();
        transport.limits.header_entries = 1;
        assert!(matches!(
            parse(&mut transport, request.as_bytes()).unwrap(),
            Parsed::Callback(_)
        ));
        transport.limits.target_bytes -= 1;
        assert!(matches!(
            parse(&mut transport, request.as_bytes()).unwrap(),
            Parsed::Rejected(InteractiveTransportError::Limit)
        ));
        transport.limits.target_bytes += 1;
        let second =
            format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:12345\r\nX-Extra: x\r\n\r\n");
        assert!(matches!(
            parse(&mut transport, second.as_bytes()).unwrap(),
            Parsed::Rejected(InteractiveTransportError::Limit)
        ));
    }
}
