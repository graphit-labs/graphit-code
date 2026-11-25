use crate::{IoFailure, ProcessHost, host::HostSlot, supervisor::HostRecord};
use graphit_core::file_io::SnapshotContext;
use std::{
    fmt, io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{Notify, mpsc, oneshot};

#[derive(Clone, Copy, Debug)]
pub enum OperationAdmissionFailure {
    HostNotReady,
    HostSealed,
    CapacityFull,
    Core(IoFailure),
}
impl fmt::Display for OperationAdmissionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for OperationAdmissionFailure {}
impl From<io::Error> for OperationAdmissionFailure {
    fn from(error: io::Error) -> Self {
        Self::Core(error.into())
    }
}
pub(crate) struct ControlLease {
    pub(crate) original: Arc<SnapshotContext>,
    pub(crate) deadline: std::time::Instant,
    pub(crate) slot: Arc<HostSlot>,
    closed: AtomicBool,
    changed: Notify,
}
impl ControlLease {
    pub(crate) fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.changed.notify_waiters();
            self.slot.wake();
        }
    }
    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire) || self.slot.sealed()
    }
    pub(crate) async fn closed(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.is_closed() {
                return;
            }
            tokio::select! {
                _ = changed => (),
                _ = self.slot.host_closed() => (),
            }
        }
    }
}

/// Consuming local stop. Its Drop closes A without cancelling the original.
pub struct OperationStop {
    sender: Option<oneshot::Sender<()>>,
    lease: Arc<ControlLease>,
}
impl OperationStop {
    pub fn stop(mut self) {
        self.lease.close();
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(());
        }
    }
}
impl Drop for OperationStop {
    fn drop(&mut self) {
        self.lease.close();
    }
}
impl fmt::Debug for OperationStop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OperationStop")
    }
}

/// Keeps the native borrow outside the host; Drop closes only this caller.
/// ```compile_fail
/// use graphit_process::OperationReceiver;
/// fn detach(value: OperationReceiver<'_>) -> OperationReceiver<'static> { value }
/// ```
pub struct OperationReceiver<'native> {
    pub(crate) receiver: Option<oneshot::Receiver<()>>,
    pub(crate) sender: Option<mpsc::Sender<Box<HostRecord>>>,
    pub(crate) lease: Arc<ControlLease>,
    pub(crate) original: &'native Arc<SnapshotContext>,
}
impl Drop for OperationReceiver<'_> {
    fn drop(&mut self) {
        self.lease.close();
    }
}
impl fmt::Debug for OperationReceiver<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OperationReceiver")
    }
}
pub fn operation<'native>(
    original: &'native Arc<SnapshotContext>,
    host: &ProcessHost,
) -> Result<(OperationStop, OperationReceiver<'native>), OperationAdmissionFailure> {
    let deadline = original.required_deadline()?;
    let slot = host.reserve()?;
    let sender = host.sender().ok_or(OperationAdmissionFailure::HostSealed)?;
    let (stop_sender, receiver) = oneshot::channel();
    let lease = Arc::new(ControlLease {
        original: Arc::clone(original),
        deadline,
        slot,
        closed: AtomicBool::new(false),
        changed: Notify::new(),
    });
    Ok((
        OperationStop {
            sender: Some(stop_sender),
            lease: Arc::clone(&lease),
        },
        OperationReceiver {
            receiver: Some(receiver),
            sender: Some(sender),
            lease,
            original,
        },
    ))
}
