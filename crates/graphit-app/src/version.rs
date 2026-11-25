//! The public build-version journey. No project, installation or credential startup.

use std::io::Write;

use graphit_core::terminal::{Limits, Mode, Options, Printer, TerminalError};

use crate::build_metadata::{MAX_BRAND_BYTES, MAX_VERSION_BYTES, Metadata, MetadataError};

const OUTPUT_BYTES: usize = MAX_BRAND_BYTES + 1 + MAX_VERSION_BYTES + 1;
const DIAGNOSTIC_BYTES: usize = 64;

#[derive(Debug)]
pub(crate) enum CliError {
    Metadata(MetadataError),
    Allocation,
    Output(TerminalError),
}

impl CliError {
    fn diagnostic(&self) -> &'static str {
        match self {
            Self::Metadata(MetadataError::InvalidBrand) => "error: invalid build brand\n",
            Self::Metadata(MetadataError::InvalidVersion) => "error: invalid build version\n",
            Self::Allocation => "error: output allocation failed\n",
            Self::Output(TerminalError::Io { .. }) => "error: output write failed\n",
            Self::Output(_) => "error: output publication failed\n",
        }
    }
}

fn options(bytes: usize) -> Options {
    Options {
        mode: Mode::Plain,
        limits: Limits {
            max_input_bytes: bytes,
            max_output_bytes: bytes,
            max_table_rows: 0,
            max_tasks: 1,
        },
    }
}

pub(crate) fn render<W: Write + Send>(metadata: &Metadata<'_>, writer: W) -> Result<(), CliError> {
    let brand = metadata.brand().binary_name();
    let version = metadata.version();
    let bytes = brand
        .len()
        .checked_add(version.len())
        .and_then(|n| n.checked_add(2))
        .filter(|n| *n <= OUTPUT_BYTES)
        .ok_or(CliError::Allocation)?;
    let mut line = String::new();
    line.try_reserve_exact(bytes)
        .map_err(|_| CliError::Allocation)?;
    line.push_str(brand);
    line.push(' ');
    line.push_str(version);
    line.push('\n');
    Printer::new(writer, "", options(OUTPUT_BYTES))
        .map_err(CliError::Output)?
        .write_data(&line)
        .map_err(CliError::Output)
}

pub(crate) fn write_diagnostic<W: Write + Send>(
    error: &CliError,
    writer: W,
) -> Result<(), TerminalError> {
    Printer::new(writer, "", options(DIAGNOSTIC_BYTES))?.write_data(error.diagnostic())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{self, ErrorKind},
        sync::{Arc, Mutex},
    };

    #[derive(Default)]
    struct Observation {
        bytes: Vec<u8>,
        writes: usize,
        flushes: usize,
    }

    struct Sink {
        observation: Arc<Mutex<Observation>>,
        fail_after: Option<usize>,
        fail_flush: bool,
    }

    impl Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let mut observed = self.observation.lock().unwrap();
            observed.writes += 1;
            let accepted = match self.fail_after {
                Some(limit) => {
                    let remaining = limit.saturating_sub(observed.bytes.len());
                    if remaining == 0 {
                        return Err(ErrorKind::BrokenPipe.into());
                    }
                    bytes.len().min(remaining)
                }
                None => bytes.len(),
            };
            observed.bytes.extend_from_slice(&bytes[..accepted]);
            Ok(accepted)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.observation.lock().unwrap().flushes += 1;
            if self.fail_flush {
                Err(ErrorKind::BrokenPipe.into())
            } else {
                Ok(())
            }
        }
    }

    fn sink(fail_after: Option<usize>, fail_flush: bool) -> (Sink, Arc<Mutex<Observation>>) {
        let observation = Arc::new(Mutex::new(Observation::default()));
        (
            Sink {
                observation: observation.clone(),
                fail_after,
                fail_flush,
            },
            observation,
        )
    }

    #[test]
    fn complete_maximum_line_uses_the_real_core_writer_and_flush() {
        let brand = "a".repeat(MAX_BRAND_BYTES);
        let version = "v".repeat(MAX_VERSION_BYTES);
        let metadata = Metadata::new(&brand, &version).unwrap();
        let (writer, observation) = sink(None, false);
        render(&metadata, writer).unwrap();
        let observed = observation.lock().unwrap();
        assert_eq!(observed.bytes, format!("{brand} {version}\n").as_bytes());
        assert_eq!(observed.bytes.len(), OUTPUT_BYTES);
        assert_eq!(observed.flushes, 1);
    }

    #[test]
    fn write_and_flush_failures_preserve_partial_bytes_without_retry() {
        let metadata = Metadata::new("graphit", "dev").unwrap();
        for (limit, flush, expected, writes, flushes) in [
            (Some(0), false, &b""[..], 1, 0),
            (Some(4), false, &b"grap"[..], 2, 0),
            (None, true, &b"graphit dev\n"[..], 1, 1),
        ] {
            let (writer, observation) = sink(limit, flush);
            let error = render(&metadata, writer).unwrap_err();
            assert!(matches!(
                error,
                CliError::Output(TerminalError::Io {
                    kind: ErrorKind::BrokenPipe,
                    ..
                })
            ));
            let observed = observation.lock().unwrap();
            assert_eq!(observed.bytes, expected);
            assert_eq!(observed.writes, writes);
            assert_eq!(observed.flushes, flushes);
        }
    }

    #[test]
    fn diagnostic_is_bounded_and_its_own_failure_is_observable() {
        let (writer, observation) = sink(None, false);
        write_diagnostic(&CliError::Metadata(MetadataError::InvalidBrand), writer).unwrap();
        assert_eq!(
            observation.lock().unwrap().bytes,
            b"error: invalid build brand\n"
        );
        let (writer, observation) = sink(Some(0), false);
        assert!(
            write_diagnostic(&CliError::Metadata(MetadataError::InvalidBrand), writer).is_err()
        );
        assert!(observation.lock().unwrap().bytes.is_empty());
    }
}
