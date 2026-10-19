//! Read-only, bounded observations of one opened log file. No lock or mutation.
use crate::{
    file_lock::Cancellation,
    local_files::{directory_exists, regular_metadata},
    log_file::{LogError, LogPaths, Stage},
};
use std::{
    fmt,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_read_bytes: usize,
    pub max_lines: usize,
}
impl Limits {
    fn validate(self) -> Result<(), LogError> {
        let row_bytes = self
            .max_lines
            .min(self.max_read_bytes)
            .checked_mul(size_of::<String>());
        if self
            .max_read_bytes
            .checked_mul(4)
            .and_then(|n| n.checked_add(row_bytes?))
            .is_none()
            || u64::try_from(self.max_read_bytes).is_err()
        {
            return Err(LogError::InvalidOptions);
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct Tail {
    pub lines: Vec<String>,
    pub read_bytes: usize,
    pub byte_windowed: bool,
    pub final_unterminated: bool,
    pub replaced_utf8: bool,
}
impl fmt::Debug for Tail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tail")
            .field("lines", &self.lines.len())
            .field("read_bytes", &self.read_bytes)
            .field("byte_windowed", &self.byte_windowed)
            .field("final_unterminated", &self.final_unterminated)
            .field("replaced_utf8", &self.replaced_utf8)
            .finish()
    }
}

pub fn read(
    paths: &LogPaths,
    limits: Limits,
    cancellation: &Cancellation,
) -> Result<Tail, LogError> {
    limits.validate()?;
    if limits.max_read_bytes == 0 || limits.max_lines == 0 {
        return Ok(Tail::default());
    }
    check_cancel(cancellation)?;
    let path = paths.path();
    if !directory_exists(path.parent().ok_or(LogError::InvalidPath)?)
        .map_err(|e| LogError::from_io(Stage::Directories, e))?
    {
        return Err(LogError::from_io(
            Stage::Directories,
            io::ErrorKind::NotFound.into(),
        ));
    }
    if regular_metadata(&path)
        .map_err(|e| LogError::from_io(Stage::Open, e))?
        .is_none()
    {
        return Err(LogError::from_io(
            Stage::Open,
            io::ErrorKind::NotFound.into(),
        ));
    }
    let mut file = File::open(path).map_err(|e| LogError::from_io(Stage::Open, e))?;
    let metadata = file
        .metadata()
        .map_err(|e| LogError::from_io(Stage::Read, e))?;
    if !metadata.is_file() {
        return Err(LogError::InvalidPath);
    }
    let (bytes, byte_windowed) = window(
        &mut file,
        metadata.len(),
        limits.max_read_bytes,
        cancellation,
    )?;
    let mut tail = parse(&bytes, byte_windowed, limits.max_lines)?;
    check_cancel(cancellation)?;
    tail.read_bytes = bytes.len();
    Ok(tail)
}
fn check_cancel(cancellation: &Cancellation) -> Result<(), LogError> {
    if cancellation.is_cancelled() {
        Err(LogError::from_io(
            Stage::Read,
            io::ErrorKind::Interrupted.into(),
        ))
    } else {
        Ok(())
    }
}
fn window(
    source: &mut (impl Read + Seek),
    observed_len: u64,
    budget: usize,
    cancellation: &Cancellation,
) -> Result<(Vec<u8>, bool), LogError> {
    check_cancel(cancellation)?;
    let size = observed_len.min(budget as u64) as usize;
    let start = observed_len - size as u64;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| LogError::Allocation)?;
    bytes.resize(size, 0);
    check_cancel(cancellation)?;
    source
        .seek(SeekFrom::Start(start))
        .map_err(|e| LogError::from_io(Stage::Read, e))?;
    let mut filled = 0;
    while filled < size {
        check_cancel(cancellation)?;
        match source.read(&mut bytes[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(LogError::from_io(Stage::Read, e)),
        }
    }
    check_cancel(cancellation)?;
    bytes.truncate(filled);
    Ok((bytes, start > 0))
}
fn parse(mut bytes: &[u8], byte_windowed: bool, max_lines: usize) -> Result<Tail, LogError> {
    let mut tail = Tail {
        byte_windowed,
        ..Tail::default()
    };
    if byte_windowed && let Some((&peek, rest)) = bytes.split_first() {
        bytes = if peek == b'\n' {
            rest
        } else {
            rest.iter()
                .position(|b| *b == b'\n')
                .map_or(&[], |n| &rest[n + 1..])
        };
    }
    if bytes.is_empty() || max_lines == 0 {
        return Ok(tail);
    }
    tail.final_unterminated = !bytes.ends_with(b"\n");
    let body = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let selected = body.rsplit(|b| *b == b'\n').take(max_lines);
    tail.lines
        .try_reserve_exact(selected.clone().count())
        .map_err(|_| LogError::Allocation)?;
    for line in selected {
        let (text, replaced) = text(line.strip_suffix(b"\r").unwrap_or(line))?;
        tail.replaced_utf8 |= replaced;
        tail.lines.push(text);
    }
    tail.lines.reverse();
    Ok(tail)
}
fn text(bytes: &[u8]) -> Result<(String, bool), LogError> {
    let mut size = 0_usize;
    let mut replaced = false;
    for chunk in bytes.utf8_chunks() {
        let invalid = !chunk.invalid().is_empty();
        replaced |= invalid;
        size = size
            .checked_add(chunk.valid().len())
            .and_then(|n| n.checked_add(if invalid { 3 } else { 0 }))
            .ok_or(LogError::Allocation)?;
    }
    let mut result = String::new();
    result
        .try_reserve_exact(size)
        .map_err(|_| LogError::Allocation)?;
    for chunk in bytes.utf8_chunks() {
        result.push_str(chunk.valid());
        if !chunk.invalid().is_empty() {
            result.push('\u{fffd}');
        }
    }
    Ok((result, replaced))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Cursor};

    struct Observed {
        source: Cursor<Vec<u8>>,
        chunk: usize,
        largest: usize,
        total: usize,
        reads: usize,
        seeks: usize,
        interrupt: bool,
        fail_read: bool,
        fail_seek: bool,
        cancel: Option<Cancellation>,
    }
    impl Observed {
        fn new(bytes: &[u8]) -> Self {
            Self {
                source: Cursor::new(bytes.to_vec()),
                chunk: 2,
                largest: 0,
                total: 0,
                reads: 0,
                seeks: 0,
                interrupt: false,
                fail_read: false,
                fail_seek: false,
                cancel: None,
            }
        }
    }
    impl Read for Observed {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            self.largest = self.largest.max(bytes.len());
            if let Some(cancel) = &self.cancel {
                cancel.cancel();
            }
            if std::mem::take(&mut self.interrupt) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            if self.fail_read {
                return Err(io::Error::other("private-path-and-payload"));
            }
            let limit = bytes.len().min(self.chunk);
            let read = self.source.read(&mut bytes[..limit])?;
            self.total += read;
            Ok(read)
        }
    }
    impl Seek for Observed {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            self.seeks += 1;
            if self.fail_seek {
                return Err(io::Error::other("private-path-and-payload"));
            }
            self.source.seek(position)
        }
    }
    fn is_interrupted(error: LogError) {
        assert!(matches!(
            error,
            LogError::Io {
                stage: Stage::Read,
                kind: io::ErrorKind::Interrupted,
                rollback_failed: false,
                ..
            }
        ));
    }

    #[test]
    fn bounded_window_retries_interrupted_reads_and_uses_an_in_budget_peek() {
        let mut source = Observed::new(b"aaa\nbbb\nccc\n");
        source.interrupt = true;
        let (bytes, clipped) = window(&mut source, 12, 9, &Cancellation::default()).unwrap();
        assert_eq!(bytes, b"\nbbb\nccc\n");
        assert!(clipped);
        assert_eq!(source.total, 9);
        assert_eq!(source.largest, 9);
        assert_eq!(source.seeks, 1);
        assert_eq!(parse(&bytes, clipped, 50).unwrap().lines, ["bbb", "ccc"]);
        assert_eq!(source.source.position(), 12);
    }

    #[test]
    fn short_eof_is_actual_bytes_without_padding_or_a_second_observation() {
        let mut source = Observed::new(b"abc\n");
        let (bytes, clipped) = window(&mut source, 10, 10, &Cancellation::default()).unwrap();
        assert_eq!(bytes, b"abc\n");
        assert!(!clipped);
        assert_eq!(source.total, 4);
        assert_eq!(source.largest, 10);
        assert_eq!(source.seeks, 1);
        assert_eq!(source.reads, 3);
        let (bytes, clipped) = window(&mut source, 100, 10, &Cancellation::default()).unwrap();
        assert!(bytes.is_empty());
        assert!(clipped);
    }

    #[test]
    fn mid_read_and_interrupted_cancellation_stop_before_retry_or_publication() {
        for interrupt in [false, true] {
            let cancellation = Cancellation::default();
            let mut source = Observed::new(b"first\nsecond\n");
            source.interrupt = interrupt;
            source.cancel = Some(cancellation.clone());
            is_interrupted(window(&mut source, 13, 13, &cancellation).unwrap_err());
            assert_eq!(source.reads, 1);
            assert_eq!(source.seeks, 1);
            assert_eq!(source.total, if interrupt { 0 } else { 2 });
        }
    }

    #[test]
    fn seek_read_errors_are_sanitized_and_capacity_overflow_precedes_io() {
        for fail_seek in [false, true] {
            let mut source = Observed::new(b"private\n");
            source.fail_seek = fail_seek;
            source.fail_read = !fail_seek;
            let error = window(&mut source, 8, 8, &Cancellation::default()).unwrap_err();
            assert!(matches!(
                error,
                LogError::Io {
                    stage: Stage::Read,
                    kind: io::ErrorKind::Other,
                    rollback_failed: false,
                    ..
                }
            ));
            assert!(!format!("{error} {error:?}").contains("private"));
            assert_eq!(source.seeks, 1);
            assert_eq!(source.reads, usize::from(!fail_seek));
        }
        let mut source = Observed::new(b"unused");
        assert!(matches!(
            window(&mut source, u64::MAX, usize::MAX, &Cancellation::default()),
            Err(LogError::Allocation)
        ));
        assert_eq!((source.seeks, source.reads), (0, 0));
    }

    #[test]
    fn selected_utf8_matches_the_standard_lossy_policy_with_bounded_capacity() {
        let all_bytes: Vec<u8> = (0..=255).collect();
        for bytes in [
            &all_bytes[..],
            "olá 🌍 漢字".as_bytes(),
            b"\xf0\x9f",
            b"\xff\xfe\xc0\xaf",
            b"",
            b"valid",
        ] {
            let (actual, replaced) = text(bytes).unwrap();
            assert_eq!(actual, String::from_utf8_lossy(bytes));
            assert_eq!(replaced, std::str::from_utf8(bytes).is_err());
            assert!(actual.len() <= 3 * bytes.len());
            // Allocators may provide more than the requested exact reservation.
            assert!(actual.capacity() >= actual.len());
        }
    }

    #[test]
    fn real_fd_truncate_and_grow_observe_only_the_captured_length() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("read.log");
        fs::write(&path, b"old\nend\n").unwrap();
        let mut file = File::open(&path).unwrap();
        let observed = file.metadata().unwrap().len();
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(4)
            .unwrap();
        let (bytes, _) = window(&mut file, observed, 64, &Cancellation::default()).unwrap();
        assert_eq!(bytes, b"old\n");
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(0)
            .unwrap();
        assert!(
            window(&mut file, observed, 64, &Cancellation::default())
                .unwrap()
                .0
                .is_empty()
        );
        fs::write(&path, b"one\n").unwrap();
        let observed = file.metadata().unwrap().len();
        fs::write(&path, b"one\ntwo\n").unwrap();
        assert_eq!(
            window(&mut file, observed, 64, &Cancellation::default())
                .unwrap()
                .0,
            b"one\n"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn old_reader_fd_observes_the_old_file_after_atomic_compaction() {
        use crate::log_file::{Durability, Limits as FileLimits, LogFile, Options};
        use std::time::Duration;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("read.log");
        let log = LogFile::open(
            LogPaths::explicit(&path).unwrap(),
            Options {
                limits: FileLimits {
                    max_file_bytes: 12,
                    max_record_bytes: 4,
                    max_batch_bytes: 4,
                    retain_bytes: 4,
                },
                durability: Durability::Buffered,
                lock_timeout: Duration::from_secs(1),
            },
            Cancellation::default(),
        )
        .unwrap();
        for bytes in [b"one\n", b"two\n", b"tri\n"] {
            log.append(bytes).unwrap();
        }
        let mut old = File::open(&path).unwrap();
        let observed = old.metadata().unwrap().len();
        assert!(log.append(b"end\n").unwrap().compacted);
        assert_eq!(
            window(&mut old, observed, 64, &Cancellation::default())
                .unwrap()
                .0,
            b"one\ntwo\ntri\n"
        );
        assert_eq!(
            read(
                log.paths(),
                Limits {
                    max_read_bytes: 64,
                    max_lines: 50
                },
                &Cancellation::default()
            )
            .unwrap()
            .lines,
            ["tri", "end"]
        );
    }
}
