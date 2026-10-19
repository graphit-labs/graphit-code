//! Strict incremental plain text over the caller-owned Process supervisor.
use graphit_core::file_io::SnapshotContext;
use graphit_process::{
    Chunk, Counts, Failure, IncrementalSink, OperationAdmissionFailure, Outcome, PipeEnd,
    ProcessHost, Request, Retention, SinkFailure, Stream,
};
use std::{fmt, future::Future, io, sync::Arc};

const PROCESS_CHUNK_BYTES: usize = 8192;
const SCRATCH_BYTES: usize = PROCESS_CHUNK_BYTES + 3;

/// Captured consumer policy. No field supplies an authority or execution budget.
#[derive(Clone, Copy, Debug)]
pub struct PlainLimits {
    pub stdout_text_bytes: usize,
    pub stderr_text_bytes: usize,
    pub events: u64,
}

impl PlainLimits {
    fn validate(self) -> Result<(), PlainReason> {
        if self.stdout_text_bytes == 0
            || self.stderr_text_bytes == 0
            || self.events == 0
            || self.stdout_text_bytes > isize::MAX as usize
        {
            return Err(PlainReason::Policy);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlainLimit {
    Stdout,
    Stderr,
    Events,
    Chunk,
}

/// Static consumer information alongside, never instead of, the real terminal.
#[derive(Clone, Copy, Debug)]
pub enum PlainReason {
    Policy,
    Limit(PlainLimit),
    InvalidUtf8(Stream),
    IncompleteUtf8(Stream),
    UnfinishedPipe(Stream),
    Allocation,
    Sink(SinkFailure),
    Context(io::ErrorKind),
    Admission(OperationAdmissionFailure),
    Process,
}

impl PartialEq for PlainReason {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::Policy => matches!(other, Self::Policy),
            Self::Limit(value) => matches!(other, Self::Limit(right) if value == right),
            Self::InvalidUtf8(value) => matches!(other, Self::InvalidUtf8(right) if value == right),
            Self::IncompleteUtf8(value) => {
                matches!(other, Self::IncompleteUtf8(right) if value == right)
            }
            Self::UnfinishedPipe(value) => {
                matches!(other, Self::UnfinishedPipe(right) if value == right)
            }
            Self::Allocation => matches!(other, Self::Allocation),
            Self::Sink(value) => matches!(other, Self::Sink(right) if value == right),
            Self::Context(value) => matches!(other, Self::Context(right) if value == right),
            Self::Admission(value) => match value {
                OperationAdmissionFailure::HostNotReady => matches!(
                    other,
                    Self::Admission(OperationAdmissionFailure::HostNotReady)
                ),
                OperationAdmissionFailure::HostSealed => matches!(
                    other,
                    Self::Admission(OperationAdmissionFailure::HostSealed)
                ),
                OperationAdmissionFailure::CapacityFull => matches!(
                    other,
                    Self::Admission(OperationAdmissionFailure::CapacityFull)
                ),
                OperationAdmissionFailure::Core(error) => {
                    matches!(other, Self::Admission(OperationAdmissionFailure::Core(right)) if error == right)
                }
            },
            Self::Process => matches!(other, Self::Process),
        }
    }
}

impl Eq for PlainReason {}

/// The text is an explicit payload, excluded from diagnostics and serialization.
pub struct TextChunk<'a> {
    stream: Stream,
    text: &'a str,
}

impl<'a> TextChunk<'a> {
    pub fn stream(&self) -> Stream {
        self.stream
    }

    pub fn text(&self) -> &'a str {
        self.text
    }
}

impl fmt::Debug for TextChunk<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextChunk")
            .field("stream", &self.stream)
            .field("text_bytes", &self.text.len())
            .finish()
    }
}

/// Cooperatively poll, be cancellation-safe on drop, and leave no detached work.
/// The caller separately limits any copies or persistence performed by its sink.
pub trait TextSink: Send {
    fn push<'a>(
        &'a mut self,
        chunk: TextChunk<'a>,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = Result<(), SinkFailure>> + Send + 'a;
}

/// Accepted text and actual Process observation are deliberately separate domains.
pub struct PlainReceipt {
    text: String,
    stderr_bytes: u64,
    events: u64,
    process: Option<Result<Outcome, Failure>>,
}

impl PlainReceipt {
    fn empty() -> Self {
        Self {
            text: String::new(),
            stderr_bytes: 0,
            events: 0,
            process: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn accepted(&self) -> Counts {
        Counts {
            stdout: self.text.len() as u64,
            stderr: self.stderr_bytes,
        }
    }

    pub fn events(&self) -> u64 {
        self.events
    }

    pub fn process(&self) -> Option<&Result<Outcome, Failure>> {
        self.process.as_ref()
    }

    /// Borrow the actual terminal, including its same retained Remaining owner.
    pub fn process_mut(&mut self) -> Option<&mut Result<Outcome, Failure>> {
        self.process.as_mut()
    }
}

impl fmt::Debug for PlainReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlainReceipt")
            .field("stdout_text_bytes", &self.text.len())
            .field("accepted", &self.accepted())
            .field("events", &self.events)
            .field("process", &self.process)
            .finish()
    }
}

pub struct PlainFailure {
    reason: PlainReason,
    receipt: PlainReceipt,
}

impl PlainFailure {
    pub fn reason(&self) -> PlainReason {
        self.reason
    }

    pub fn receipt(&self) -> &PlainReceipt {
        &self.receipt
    }

    pub fn receipt_mut(&mut self) -> &mut PlainReceipt {
        &mut self.receipt
    }
}

impl fmt::Debug for PlainFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlainFailure")
            .field("reason", &self.reason)
            .field("receipt", &self.receipt)
            .finish()
    }
}

impl fmt::Display for PlainFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "plain subprocess {:?}", self.reason)
    }
}

impl std::error::Error for PlainFailure {}

#[derive(Clone, Copy, Default)]
struct Utf8Tail {
    bytes: [u8; 3],
    len: usize,
}

/// This is the decoder actually used by Bridge, not a separate public kernel.
fn decode<'a>(
    tail: &mut Utf8Tail,
    scratch: &'a mut [u8; SCRATCH_BYTES],
    bytes: &[u8],
    stream: Stream,
) -> Result<&'a str, PlainReason> {
    if bytes.len() > PROCESS_CHUNK_BYTES {
        return Err(PlainReason::Limit(PlainLimit::Chunk));
    }
    let length = tail
        .len
        .checked_add(bytes.len())
        .filter(|&n| n <= SCRATCH_BYTES)
        .ok_or(PlainReason::Limit(PlainLimit::Chunk))?;
    scratch[..tail.len].copy_from_slice(&tail.bytes[..tail.len]);
    scratch[tail.len..length].copy_from_slice(bytes);
    match std::str::from_utf8(&scratch[..length]) {
        Ok(text) => {
            tail.len = 0;
            Ok(text)
        }
        Err(error) if error.error_len().is_some() => Err(PlainReason::InvalidUtf8(stream)),
        Err(error) => {
            let valid = error.valid_up_to();
            let remaining = length - valid;
            if remaining > tail.bytes.len() {
                return Err(PlainReason::InvalidUtf8(stream));
            }
            tail.bytes[..remaining].copy_from_slice(&scratch[valid..length]);
            tail.len = remaining;
            std::str::from_utf8(&scratch[..valid]).map_err(|_| PlainReason::InvalidUtf8(stream))
        }
    }
}

fn check(original: &SnapshotContext) -> Result<(), PlainReason> {
    original
        .check()
        .map_err(|error| PlainReason::Context(error.kind()))
}

struct Bridge<'a, S> {
    sink: &'a mut S,
    limits: PlainLimits,
    scratch: [u8; SCRATCH_BYTES],
    stdout: Utf8Tail,
    stderr: Utf8Tail,
    receipt: PlainReceipt,
    failure: Option<PlainReason>,
}

impl<S: TextSink> Bridge<'_, S> {
    async fn deliver(
        &mut self,
        chunk: Chunk<'_>,
        original: &SnapshotContext,
    ) -> Result<(), PlainReason> {
        check(original)?;
        let stream = chunk.stream();
        let (tail, accepted, cap, limit) = match stream {
            Stream::Stdout => (
                &mut self.stdout,
                self.receipt.text.len() as u64,
                self.limits.stdout_text_bytes,
                PlainLimit::Stdout,
            ),
            Stream::Stderr => (
                &mut self.stderr,
                self.receipt.stderr_bytes,
                self.limits.stderr_text_bytes,
                PlainLimit::Stderr,
            ),
        };
        let text = decode(tail, &mut self.scratch, chunk.bytes(), stream)?;
        check(original)?;
        if text.is_empty() {
            return Ok(());
        }
        let next_bytes = accepted
            .checked_add(text.len() as u64)
            .filter(|&n| n <= cap as u64)
            .ok_or(PlainReason::Limit(limit))?;
        let next_events = self
            .receipt
            .events
            .checked_add(1)
            .filter(|&n| n <= self.limits.events)
            .ok_or(PlainReason::Limit(PlainLimit::Events))?;
        if stream == Stream::Stdout {
            self.receipt
                .text
                .try_reserve(text.len())
                .map_err(|_| PlainReason::Allocation)?;
        }
        check(original)?;
        self.sink
            .push(TextChunk { stream, text }, original)
            .await
            .map_err(PlainReason::Sink)?;
        // Ok is an observed acceptance even if the following context check fails.
        self.receipt.events = next_events;
        match stream {
            Stream::Stdout => self.receipt.text.push_str(text),
            Stream::Stderr => self.receipt.stderr_bytes = next_bytes,
        }
        check(original)
    }
}

impl<S: TextSink> IncrementalSink for Bridge<'_, S> {
    async fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        match self.deliver(chunk, original).await {
            Ok(()) => Ok(()),
            Err(reason) => {
                self.failure = Some(reason);
                Err(match reason {
                    PlainReason::Sink(error) => error,
                    _ => SinkFailure::ConsumerFailed,
                })
            }
        }
    }
}

/// Await the real supervisor, including cleanup; never timeout-drop it.
/// Nonzero child status stays in Outcome, and the caller decides its meaning.
#[expect(
    clippy::result_large_err,
    reason = "the API preserves the real Process terminal and owned receipt inline without an error allocation"
)]
pub async fn run_plain<S: TextSink>(
    original: &Arc<SnapshotContext>,
    host: &ProcessHost,
    request: Request,
    limits: PlainLimits,
    sink: &mut S,
) -> Result<PlainReceipt, PlainFailure> {
    if let Err(reason) = check(original).and_then(|()| limits.validate()) {
        return Err(PlainFailure {
            reason,
            receipt: PlainReceipt::empty(),
        });
    }
    let (_stop, receiver) =
        graphit_process::operation(original, host).map_err(|error| PlainFailure {
            reason: PlainReason::Admission(error),
            receipt: PlainReceipt::empty(),
        })?;
    let mut bridge = Bridge {
        sink,
        limits,
        scratch: [0; SCRATCH_BYTES],
        stdout: Utf8Tail::default(),
        stderr: Utf8Tail::default(),
        receipt: PlainReceipt::empty(),
        failure: None,
    };
    let terminal =
        graphit_process::run(request, original, Retention::None, &mut bridge, receiver).await;
    let failure = bridge.failure;
    let tails = [
        (Stream::Stdout, bridge.stdout),
        (Stream::Stderr, bridge.stderr),
    ];
    let mut receipt = bridge.receipt;
    let reason = match &terminal {
        Err(_) => Some(failure.unwrap_or(PlainReason::Process)),
        Ok(outcome) => {
            let mut reason = check(original).err();
            for (stream, tail) in tails {
                if reason.is_some() {
                    break;
                }
                let end = match stream {
                    Stream::Stdout => outcome.pipes.stdout,
                    Stream::Stderr => outcome.pipes.stderr,
                };
                if end != PipeEnd::Eof {
                    reason = Some(PlainReason::UnfinishedPipe(stream));
                } else if tail.len != 0 {
                    reason = Some(PlainReason::IncompleteUtf8(stream));
                }
            }
            reason
        }
    };
    receipt.process = Some(terminal);
    match reason {
        Some(reason) => Err(PlainFailure { reason, receipt }),
        None => Ok(receipt),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_equality_keeps_real_variants_and_native_error_code() {
        let values = [
            PlainReason::Admission(OperationAdmissionFailure::HostNotReady),
            PlainReason::Admission(OperationAdmissionFailure::HostSealed),
            PlainReason::Admission(OperationAdmissionFailure::CapacityFull),
            PlainReason::Admission(OperationAdmissionFailure::Core(
                io::Error::from(io::ErrorKind::Other).into(),
            )),
            PlainReason::Admission(OperationAdmissionFailure::Core(
                io::Error::from_raw_os_error(0).into(),
            )),
            PlainReason::Admission(OperationAdmissionFailure::Core(
                io::Error::from_raw_os_error(7).into(),
            )),
        ];
        for (left, value) in values.iter().enumerate() {
            for (right, other) in values.iter().enumerate() {
                assert_eq!(value == other, left == right);
            }
            assert_ne!(*value, PlainReason::Process);
        }
        for (reason, code) in [
            (values[3], None),
            (values[4], Some(0)),
            (values[5], Some(7)),
        ] {
            let PlainReason::Admission(OperationAdmissionFailure::Core(error)) = reason else {
                panic!("expected the actual typed Core error");
            };
            assert_eq!(error.os_code(), code);
        }
    }

    #[test]
    fn every_scalar_split_uses_the_bridge_decoder() {
        for scalar in ["¢", "€", "🦀"] {
            for offset in 1..scalar.len() {
                let mut tail = Utf8Tail::default();
                let mut scratch = [0; SCRATCH_BYTES];
                let first = [b"A".as_slice(), &scalar.as_bytes()[..offset]].concat();
                let text = decode(&mut tail, &mut scratch, &first, Stream::Stdout).unwrap();
                assert_eq!(text, "A");
                assert_eq!(tail.len, offset);
                let last = [&scalar.as_bytes()[offset..], b"Z".as_slice()].concat();
                let text = decode(&mut tail, &mut scratch, &last, Stream::Stdout).unwrap();
                assert_eq!(text, format!("{scalar}Z"));
                assert_eq!(tail.len, 0);
            }
        }
    }

    #[test]
    fn invalid_combined_chunk_rejects_its_entire_prefix() {
        let mut tail = Utf8Tail::default();
        let mut scratch = [0; SCRATCH_BYTES];
        assert_eq!(
            decode(&mut tail, &mut scratch, &[b'A', 0xff], Stream::Stdout),
            Err(PlainReason::InvalidUtf8(Stream::Stdout))
        );
        assert_eq!(
            decode(&mut tail, &mut scratch, &[0xe2, 0x82], Stream::Stdout).unwrap(),
            ""
        );
        assert_eq!(tail.len, 2);
        assert_eq!(
            decode(&mut tail, &mut scratch, b"X", Stream::Stdout),
            Err(PlainReason::InvalidUtf8(Stream::Stdout))
        );
    }

    #[test]
    fn separate_pipe_tails_reuse_only_the_serial_scratch() {
        let mut stdout = Utf8Tail::default();
        let mut stderr = Utf8Tail::default();
        let mut scratch = [0; SCRATCH_BYTES];
        assert_eq!(
            decode(&mut stdout, &mut scratch, &[0xe2], Stream::Stdout).unwrap(),
            ""
        );
        assert_eq!(
            decode(&mut stderr, &mut scratch, &[0xc2], Stream::Stderr).unwrap(),
            ""
        );
        assert_eq!(
            decode(&mut stdout, &mut scratch, &[0x82, 0xac], Stream::Stdout).unwrap(),
            "€"
        );
        assert_eq!(
            decode(&mut stderr, &mut scratch, &[0xa2], Stream::Stderr).unwrap(),
            "¢"
        );
        assert_eq!((stdout.len, stderr.len), (0, 0));
    }

    #[test]
    fn chunk_bound_and_maximum_tail_are_exact() {
        let mut tail = Utf8Tail {
            bytes: [0xf0, 0x9f, 0xa6],
            len: 3,
        };
        let mut scratch = [0; SCRATCH_BYTES];
        let mut exact = vec![b'x'; PROCESS_CHUNK_BYTES];
        exact[0] = 0x80;
        assert_eq!(
            decode(&mut tail, &mut scratch, &exact, Stream::Stdout)
                .unwrap()
                .len(),
            SCRATCH_BYTES
        );
        assert_eq!(tail.len, 0);
        assert_eq!(
            decode(
                &mut tail,
                &mut scratch,
                &vec![b'x'; PROCESS_CHUNK_BYTES + 1],
                Stream::Stdout
            ),
            Err(PlainReason::Limit(PlainLimit::Chunk))
        );
    }
}
