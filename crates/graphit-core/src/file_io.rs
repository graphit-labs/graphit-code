//! Shared original-request cancellation and deadlines.
//! Synchronous native I/O checks the request between bounded chunks.
use crate::file_lock::Cancellation;
#[cfg(any(test, feature = "test-util"))]
use std::sync::{Mutex, OnceLock};
use std::{io, sync::Arc, time::Instant};

/// Retains the original request identity, deadline and cancellation signal.
pub struct SnapshotContext {
    identity: RequestIdentity,
    deadline: Option<Instant>,
    cancellation: Cancellation,
    #[cfg(any(test, feature = "test-util"))]
    test_clock: OnceLock<Arc<Mutex<Instant>>>,
}
impl SnapshotContext {
    pub fn new(deadline: Instant, cancellation: Cancellation) -> Self {
        Self {
            identity: RequestIdentity(Arc::new(())),
            deadline: Some(deadline),
            cancellation,
            #[cfg(any(test, feature = "test-util"))]
            test_clock: OnceLock::new(),
        }
    }
    /// A continuous service has cancellation but no artificial expiry.
    pub fn service(cancellation: Cancellation) -> Self {
        Self {
            identity: RequestIdentity(Arc::new(())),
            deadline: None,
            cancellation,
            #[cfg(any(test, feature = "test-util"))]
            test_clock: OnceLock::new(),
        }
    }
    /// Installs deadline-check time once for tests, before Auth admission or capture.
    /// Identity, deadline and cancellation stay unchanged; SDK timers remain real.
    #[cfg(any(test, feature = "test-util"))]
    pub fn set_test_clock(&self, now: Arc<Mutex<Instant>>) -> Result<(), Arc<Mutex<Instant>>> {
        self.test_clock.set(now)
    }
    #[cfg(any(test, feature = "test-util"))]
    pub(crate) fn test_clock(&self) -> Option<&Arc<Mutex<Instant>>> {
        self.test_clock.get()
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    /// Finite transports must reject a service context rather than invent a deadline.
    pub fn required_deadline(&self) -> io::Result<Instant> {
        self.check()?;
        self.deadline.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "operation requires a finite deadline",
            )
        })
    }
    pub fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }
    /// Equal deadlines and signals do not establish request identity or authority.
    pub fn same_request(&self, other: &Self) -> bool {
        self.identity.matches(other)
    }
    pub fn check(&self) -> io::Result<()> {
        crate::local_files::check_cancelled(&self.cancellation)?;
        if let Some(deadline) = self.deadline {
            #[cfg(any(test, feature = "test-util"))]
            let now = test_now(self.test_clock())?;
            #[cfg(not(any(test, feature = "test-util")))]
            let now = Instant::now();
            if now >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
        }
        Ok(())
    }
    pub(crate) fn request_identity(&self) -> RequestIdentity {
        self.identity.clone()
    }
}
#[cfg(any(test, feature = "test-util"))]
pub(crate) fn test_now(clock: Option<&Arc<Mutex<Instant>>>) -> io::Result<Instant> {
    match clock {
        Some(clock) => clock
            .lock()
            .map(|now| *now)
            .map_err(|_| io::Error::other("test clock poisoned")),
        None => Ok(Instant::now()),
    }
}

/// An allocation identity avoids address reuse when a request context is moved.
/// It is internal data, never a serialized request or filesystem capability.
#[derive(Clone)]
pub(crate) struct RequestIdentity(Arc<()>);
impl RequestIdentity {
    pub(crate) fn matches(&self, context: &SnapshotContext) -> bool {
        Arc::ptr_eq(&self.0, &context.identity.0)
    }
}

pub(crate) fn native_name_bytes(name: &std::ffi::OsStr) -> io::Result<usize> {
    #[cfg(unix)]
    {
        Ok(name.as_encoded_bytes().len())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        name.encode_wide()
            .count()
            .checked_mul(2)
            .ok_or_else(|| io::ErrorKind::FileTooLarge.into())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = name;
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn context() -> Arc<SnapshotContext> {
        Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ))
    }
    #[test]
    fn clones_keep_request_identity_without_accepting_an_equal_fresh_context() {
        let original = context();
        let clone = Arc::clone(&original);
        let fresh = SnapshotContext::new(
            original.required_deadline().unwrap(),
            original.cancellation().clone(),
        );
        assert!(original.same_request(&clone));
        assert!(!original.same_request(&fresh));
    }
    #[test]
    fn test_clock_advances_shared_request_without_changing_identity_or_deadline() {
        let deadline = Instant::now();
        let now = Arc::new(Mutex::new(deadline - Duration::from_secs(1)));
        let original = Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
        let identity = original.request_identity();
        original.set_test_clock(Arc::clone(&now)).unwrap();
        let replacement = Arc::new(Mutex::new(deadline + Duration::from_secs(1)));
        let rejected = original
            .set_test_clock(Arc::clone(&replacement))
            .unwrap_err();
        assert!(Arc::ptr_eq(&rejected, &replacement));
        let captured = Arc::clone(&original);
        assert_eq!(original.required_deadline().unwrap(), deadline);
        captured.check().unwrap();
        assert!(identity.matches(&captured));
        *now.lock().unwrap() = deadline;
        assert_eq!(
            captured.check().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        *now.lock().unwrap() = deadline + Duration::from_secs(1);
        assert_eq!(
            original.check().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(original.deadline(), Some(deadline));
        assert!(original.same_request(&captured));
        assert!(identity.matches(&captured));
    }
    #[test]
    fn test_clock_expiry_keeps_cancellation_cause_first() {
        let deadline = Instant::now();
        let now = Arc::new(Mutex::new(deadline));
        let original = SnapshotContext::new(deadline, Cancellation::default());
        original.set_test_clock(Arc::clone(&now)).unwrap();
        assert_eq!(
            original.check().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(
            std::panic::catch_unwind(|| {
                let _guard = now.lock().unwrap();
                panic!("poison the request-local test clock");
            })
            .is_err()
        );
        assert_eq!(original.check().unwrap_err().kind(), io::ErrorKind::Other);
        original.cancellation().cancel();
        assert_eq!(
            original.check().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(
            original.required_deadline().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
    }
    #[test]
    fn service_context_does_not_consult_test_clock() {
        let now = Arc::new(Mutex::new(Instant::now()));
        assert!(
            std::panic::catch_unwind(|| {
                let _guard = now.lock().unwrap();
                panic!("poison the request-local test clock");
            })
            .is_err()
        );
        let service = SnapshotContext::service(Cancellation::default());
        service.set_test_clock(now).unwrap();
        service.check().unwrap();
        assert_eq!(service.deadline(), None);
        assert_eq!(
            service.required_deadline().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        service.cancellation().cancel();
        assert_eq!(
            service.check().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
    }
    #[test]
    fn test_clocks_are_local_to_each_request() {
        let deadline = Instant::now();
        let before = deadline - Duration::from_secs(1);
        let first_now = Arc::new(Mutex::new(before));
        let second_now = Arc::new(Mutex::new(before));
        let first = SnapshotContext::new(deadline, Cancellation::default());
        first.set_test_clock(Arc::clone(&first_now)).unwrap();
        let second = SnapshotContext::new(deadline, Cancellation::default());
        second.set_test_clock(Arc::clone(&second_now)).unwrap();
        first.check().unwrap();
        second.check().unwrap();
        assert!(!first.same_request(&second));
        *first_now.lock().unwrap() = deadline;
        assert_eq!(first.check().unwrap_err().kind(), io::ErrorKind::TimedOut);
        second.check().unwrap();
        let real_time = SnapshotContext::new(deadline, Cancellation::default());
        assert_eq!(
            real_time.check().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        *second_now.lock().unwrap() = deadline;
        assert_eq!(second.check().unwrap_err().kind(), io::ErrorKind::TimedOut);
    }
    #[test]
    fn cancellation_and_deadline_reject_normal_native_work() {
        let context = context();
        context.cancellation().cancel();
        assert_eq!(
            context.check().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
        assert_eq!(expired.check().unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert_eq!(
            expired.required_deadline().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }
    #[test]
    fn service_context_retains_identity_and_cancels_without_a_deadline() {
        let original = Arc::new(SnapshotContext::service(Cancellation::default()));
        assert_eq!(original.deadline(), None);
        original.check().unwrap();
        assert_eq!(
            original.required_deadline().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(original.same_request(&Arc::clone(&original)));
        assert!(!original.same_request(&SnapshotContext::service(original.cancellation().clone())));
        original.cancellation().cancel();
        assert_eq!(
            original.check().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(
            original.required_deadline().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
    }
}
