//! Reserve TCP ports by keeping the selected listener open for its consumer.
//! The caller chooses addresses, port policy and the whole-operation budget.

use std::{
    fmt, io,
    net::{IpAddr, SocketAddr, TcpListener},
    num::NonZeroU16,
    time::{Duration, Instant},
};

use crate::file_lock::Cancellation;

#[derive(Clone, Copy)]
pub enum PortSelection {
    Ephemeral,
    Range { first: NonZeroU16, last: NonZeroU16 },
}

impl fmt::Debug for PortSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ephemeral => "Ephemeral",
            Self::Range { .. } => "Range { .. }",
        })
    }
}

#[derive(Clone, Copy)]
pub struct Options {
    pub address: IpAddr,
    pub ports: PortSelection,
    pub timeout: Duration,
}

impl fmt::Debug for Options {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Options")
            .field("ports", &self.ports)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// Owns the live reservation. The standard listener initially uses blocking I/O.
/// An async server must configure nonblocking I/O before transferring it to its
/// runtime. Dropping this owner closes its handle; no hidden clone is retained.
#[must_use = "dropping this owner releases the listener"]
pub struct BoundListener {
    listener: TcpListener,
    address: SocketAddr,
    attempts: u32,
}

impl BoundListener {
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    pub fn into_listener(self) -> TcpListener {
        self.listener
    }
}

impl fmt::Debug for BoundListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoundListener")
            .field("attempts", &self.attempts)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Bind,
    Address,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    InvalidRange,
    InvalidTimeout,
    Cancelled,
    TimedOut,
    Exhausted,
    Io {
        stage: Stage,
        kind: io::ErrorKind,
        os_code: Option<i32>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BindError {
    pub failure: Failure,
    pub attempts: u32,
}

impl fmt::Display for BindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TCP reservation {:?}; attempts {}",
            self.failure, self.attempts
        )
    }
}

impl std::error::Error for BindError {}

/// Does not perform DNS resolution, read configuration, or wait for clients.
/// Range selection retries only address collisions, in inclusive ascending
/// order. Ephemeral selection makes one request for a port chosen by the OS.
///
/// Cancellation/deadline are checked around each synchronous bind and before
/// publishing success. A kernel syscall itself cannot be interrupted by this
/// budget. A listener obtained after cancellation/expiry is dropped, not returned.
pub fn reserve(options: Options, cancellation: &Cancellation) -> Result<BoundListener, BindError> {
    reserve_with(options, cancellation, TcpListener::bind)
}

fn reserve_with(
    options: Options,
    cancellation: &Cancellation,
    mut bind: impl FnMut(SocketAddr) -> io::Result<TcpListener>,
) -> Result<BoundListener, BindError> {
    let (first, last) = match options.ports {
        PortSelection::Ephemeral => (0, 0),
        PortSelection::Range { first, last } if first <= last => (first.get(), last.get()),
        PortSelection::Range { .. } => return Err(error(Failure::InvalidRange, 0)),
    };
    let deadline = Instant::now()
        .checked_add(options.timeout)
        .filter(|_| !options.timeout.is_zero())
        .ok_or_else(|| error(Failure::InvalidTimeout, 0))?;
    let mut attempts = 0;
    for port in first..=last {
        check_abort(cancellation, deadline, attempts)?;
        attempts += 1;
        let result = bind(SocketAddr::new(options.address, port));
        check_abort(cancellation, deadline, attempts)?;
        let listener = match result {
            Ok(listener) => listener,
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
            Err(cause) => return Err(io_error(Stage::Bind, cause, attempts)),
        };
        let address = listener
            .local_addr()
            .map_err(|cause| io_error(Stage::Address, cause, attempts))?;
        check_abort(cancellation, deadline, attempts)?;
        return Ok(BoundListener {
            listener,
            address,
            attempts,
        });
    }
    Err(error(Failure::Exhausted, attempts))
}

fn error(failure: Failure, attempts: u32) -> BindError {
    BindError { failure, attempts }
}

fn io_error(stage: Stage, cause: io::Error, attempts: u32) -> BindError {
    error(
        Failure::Io {
            stage,
            kind: cause.kind(),
            os_code: cause.raw_os_error(),
        },
        attempts,
    )
}

fn check_abort(
    cancellation: &Cancellation,
    deadline: Instant,
    attempts: u32,
) -> Result<(), BindError> {
    if cancellation.is_cancelled() {
        Err(error(Failure::Cancelled, attempts))
    } else if Instant::now() >= deadline {
        Err(error(Failure::TimedOut, attempts))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(ports: PortSelection) -> Options {
        Options {
            address: IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            ports,
            timeout: Duration::from_secs(2),
        }
    }

    fn range(first: u16, last: u16) -> PortSelection {
        PortSelection::Range {
            first: NonZeroU16::new(first).unwrap(),
            last: NonZeroU16::new(last).unwrap(),
        }
    }

    #[test]
    fn inclusive_top_boundary_and_no_retry_on_other_io() {
        let mut observed = Vec::new();
        let error = reserve_with(
            options(range(65534, 65535)),
            &Cancellation::default(),
            |addr| {
                observed.push(addr.port());
                Err(io::ErrorKind::AddrInUse.into())
            },
        )
        .unwrap_err();
        assert_eq!(observed, [65534, 65535]);
        assert_eq!(error, super::error(Failure::Exhausted, 2));

        let error = reserve_with(options(range(1, 65535)), &Cancellation::default(), |_| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private marker",
            ))
        })
        .unwrap_err();
        assert_eq!(error.attempts, 1);
        assert_eq!(
            error.failure,
            Failure::Io {
                stage: Stage::Bind,
                kind: io::ErrorKind::PermissionDenied,
                os_code: None,
            }
        );
        assert!(!format!("{error:?} {error}").contains("private marker"));
    }

    #[test]
    fn invalid_and_pre_cancelled_requests_never_bind() {
        let cancel = Cancellation::default();
        for request in [
            options(range(2, 1)),
            Options {
                timeout: Duration::ZERO,
                ..options(PortSelection::Ephemeral)
            },
            Options {
                timeout: Duration::MAX,
                ..options(PortSelection::Ephemeral)
            },
        ] {
            let error = reserve_with(request, &cancel, |_| panic!("must not bind")).unwrap_err();
            assert_eq!(error.attempts, 0);
            assert!(matches!(
                error.failure,
                Failure::InvalidRange | Failure::InvalidTimeout
            ));
        }
        cancel.cancel();
        let error = reserve_with(options(PortSelection::Ephemeral), &cancel, |_| {
            panic!("must not bind")
        })
        .unwrap_err();
        assert_eq!(error, super::error(Failure::Cancelled, 0));
    }

    #[test]
    fn cancellation_between_collisions_stops_selection() {
        let cancel = Cancellation::default();
        let error = reserve_with(options(range(1, 65535)), &cancel, |_| {
            cancel.cancel();
            Err(io::ErrorKind::AddrInUse.into())
        })
        .unwrap_err();
        assert_eq!(error, super::error(Failure::Cancelled, 1));
    }

    #[test]
    fn abort_after_success_drops_real_listener() {
        for cancelled in [false, true] {
            let cancel = Cancellation::default();
            let mut request = options(PortSelection::Ephemeral);
            request.timeout = Duration::from_millis(100);
            let mut selected = None;
            let error = reserve_with(request, &cancel, |address| {
                let listener = TcpListener::bind(address)?;
                selected = Some(listener.local_addr()?);
                if cancelled {
                    cancel.cancel();
                } else {
                    std::thread::sleep(Duration::from_millis(120));
                }
                Ok(listener)
            })
            .unwrap_err();
            assert_eq!(error.attempts, 1);
            assert_eq!(
                error.failure,
                if cancelled {
                    Failure::Cancelled
                } else {
                    Failure::TimedOut
                }
            );
            let rebound = TcpListener::bind(selected.unwrap()).expect("aborted socket was closed");
            drop(rebound);
        }
    }
}
