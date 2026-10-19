//! Bounded NDJSON client for the authenticated daemon Now route.
//! Framing belongs to the HTTP and Tokio codecs; snapshot validation belongs to Daemon.
use crate::{ast_source::runtime_headers, source_server::BrokerRequest};
use axum::body::Bytes;
use graphit_core::{
    native_document_file::NativeDocumentFileLimits, project_paths::InstallationPaths,
};
use graphit_daemon::now_subscription::{
    NowSubscriptionError, RemoteNowSnapshot, decode_now_snapshot,
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use std::{
    error::Error,
    fmt,
    io::{self, Write},
    net::SocketAddr,
    num::NonZeroUsize,
    pin::Pin,
};
use tokio_stream::{Stream, StreamExt};
use tokio_util::{
    codec::{FramedRead, LinesCodec, LinesCodecError},
    io::StreamReader,
};

type BodyStream = Pin<Box<dyn Stream<Item = io::Result<Bytes>> + Send>>;
type Lines = FramedRead<StreamReader<BodyStream, Bytes>, LinesCodec>;

/// Actual causes remain inspectable; diagnostics exclude headers and source paths.
pub enum NowTransportError {
    Input,
    Http(u16),
    Protocol,
    Request(io::Error),
    Network(reqwest::Error),
    Frame(LinesCodecError),
    Snapshot(NowSubscriptionError),
    Encode(serde_json::Error),
}
impl fmt::Debug for NowTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(status) => write!(f, "Now HTTP {status}"),
            Self::Input => f.write_str("Now invalid input"),
            Self::Protocol => f.write_str("Now invalid HTTP protocol"),
            Self::Request(error) => write!(f, "Now request {:?}", error.kind()),
            Self::Network(_) => f.write_str("Now HTTP transport failed"),
            Self::Frame(_) => f.write_str("Now invalid or oversized NDJSON frame"),
            Self::Snapshot(_) => f.write_str("Now invalid full snapshot"),
            Self::Encode(_) => f.write_str("Now request encoding failed"),
        }
    }
}
impl fmt::Display for NowTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for NowTransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Network(e) => Some(e),
            Self::Frame(e) => Some(e),
            Self::Snapshot(e) => Some(e),
            Self::Encode(e) => Some(e),
            _ => None,
        }
    }
}
/// One borrowed Original and HTTP body. A failure retires the body immediately.
/// No previous view or background task is retained by this client.
pub struct NowStream<'a> {
    lines: Option<Lines>,
    original: &'a RequestContext<'a>,
    request: &'a BrokerRequest,
    line_bytes: NonZeroUsize,
}
impl NowStream<'_> {
    pub async fn next(&mut self) -> Result<Option<RemoteNowSnapshot>, NowTransportError> {
        if let Err(error) = self.original.check() {
            self.lines.take();
            return Err(NowTransportError::Request(error));
        }
        let Some(lines) = self.lines.as_mut() else {
            return Ok(None);
        };
        let item = tokio::select! {
            item = lines.next()=>item,
            () = self.original.cancellation().cancelled()=>{
                self.lines.take(); return Err(NowTransportError::Request(io::ErrorKind::Interrupted.into()));
            },
            () = deadline(self.original)=>{
                self.lines.take(); return Err(NowTransportError::Request(io::ErrorKind::TimedOut.into()));
            },
        };
        let result = match item {
            Some(Ok(line)) => decode_now_snapshot(
                line.as_bytes(),
                self.request.project_id,
                self.request.instance_id.as_deref(),
                self.request.version.as_deref(),
                self.line_bytes,
            )
            .map(Some)
            .map_err(NowTransportError::Snapshot),
            Some(Err(error)) => Err(NowTransportError::Frame(error)),
            None => Ok(None),
        };
        if result.is_err() || matches!(result, Ok(None)) {
            self.lines.take();
        }
        if let Err(error) = self.original.check() {
            self.lines.take();
            return Err(NowTransportError::Request(error));
        }
        result
    }
}
async fn deadline(original: &RequestContext<'_>) {
    match original.deadline() {
        Some(end) => tokio::time::sleep_until(end.into()).await,
        None => std::future::pending::<()>().await,
    }
}
struct RequestBytes {
    value: Vec<u8>,
    cap: usize,
}
impl Write for RequestBytes {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if input.len() > self.cap.saturating_sub(self.value.len()) {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        self.value.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Open only the resolved loopback daemon; authenticate with its live Runtime key.
/// `content` is the captured caller's existing input/output policy, not a new TTL.
pub async fn connect_now<'a>(
    installation: &InstallationPaths,
    address: SocketAddr,
    client: &reqwest::Client,
    limits: (NativeDocumentFileLimits, ContentLimits),
    request: &'a BrokerRequest,
    original: &'a RequestContext<'a>,
) -> Result<NowStream<'a>, NowTransportError> {
    if !address.ip().is_loopback() {
        return Err(NowTransportError::Input);
    }
    original.check().map_err(NowTransportError::Request)?;
    let (native, content) = limits;
    let line_bytes = NonZeroUsize::new(content.max_output_bytes).ok_or(NowTransportError::Input)?;
    let mut body = RequestBytes {
        value: Vec::new(),
        cap: content.max_input_bytes,
    };
    serde_json::to_writer(&mut body, request).map_err(NowTransportError::Encode)?;
    let headers = runtime_headers(installation, native, original)
        .await
        .map_err(NowTransportError::Request)?;
    let response = tokio::select! {
        result=client.post(format!("http://{address}/api/now/subscribe"))
            .headers(headers.into_iter().collect()).header(reqwest::header::CONTENT_TYPE,"application/json")
            .body(body.value).send()=>result.map_err(NowTransportError::Network)?,
        ()=original.cancellation().cancelled()=>return Err(NowTransportError::Request(io::ErrorKind::Interrupted.into())),
        ()=deadline(original)=>return Err(NowTransportError::Request(io::ErrorKind::TimedOut.into())),
    };
    original.check().map_err(NowTransportError::Request)?;
    if !response.status().is_success() {
        return Err(NowTransportError::Http(response.status().as_u16()));
    }
    if response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        != Some("application/x-ndjson")
    {
        return Err(NowTransportError::Protocol);
    }
    let bytes: BodyStream = Box::pin(
        response
            .bytes_stream()
            .map(|value| value.map_err(io::Error::other)),
    );
    let lines = FramedRead::new(
        StreamReader::new(bytes),
        LinesCodec::new_with_max_length(line_bytes.get()),
    );
    Ok(NowStream {
        lines: Some(lines),
        original,
        request,
        line_bytes,
    })
}
