use crate::{
    Cleanup, IoFailure,
    supervisor::{HostRecord, RecordPhase, RecordSnapshot},
};
use graphit_core::file_lock::Cancellation;
use std::{
    fmt,
    future::Future,
    io,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    thread::{self, JoinHandle},
    time::Instant,
};
use tokio::sync::{Notify, mpsc, oneshot};

const THREAD_NAME: &str = "graphit-process-host";

/// Explicit logical capacity; thread stacks use the standard library and SDK defaults.
#[derive(Clone, Copy, Debug)]
pub struct HostLimits {
    slots: usize,
    blocking_threads: usize,
}
impl HostLimits {
    pub fn new(slots: usize, blocking_threads: usize) -> io::Result<Self> {
        if [slots, blocking_threads].contains(&0) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let value = Self {
            slots,
            blocking_threads,
        };
        Ok(value)
    }
}
#[derive(Clone, Copy, Debug)]
pub enum HostPhase {
    Installation,
    Pump,
}
#[derive(Clone, Copy, Debug)]
pub enum HostFailure {
    Io(IoFailure),
    Panic(HostPhase),
}
#[derive(Clone, Copy, Debug)]
pub enum HostReadiness {
    Ready,
    Pending,
    Failed(HostFailure),
    Sealed,
}
#[derive(Clone, Copy, Debug)]
pub struct HostReport {
    failure: Option<HostFailure>,
}
impl HostReport {
    pub fn failure(&self) -> Option<HostFailure> {
        self.failure
    }
}
pub enum HostShutdown {
    Joined(HostReport),
    Pending(PendingHost),
}
#[derive(Clone, Copy, Debug)]
pub enum HostShutdownObservation {
    Joined(HostReport),
    Pending,
}

#[derive(Clone, Copy)]
enum Readiness {
    Starting,
    Ready,
    Failed(HostFailure),
    Sealed,
}
pub(crate) struct Shared {
    service: Cancellation,
    sealed: AtomicBool,
    readiness: Mutex<Readiness>,
    slots: Mutex<Box<[bool]>>,
    pump_waker: Mutex<Option<Waker>>,
    changed: Notify,
}
impl Shared {
    fn readiness(&self) -> HostReadiness {
        if self.service.is_cancelled() {
            self.seal();
        }
        match *self.readiness.lock().unwrap_or_else(|e| e.into_inner()) {
            Readiness::Starting if !self.sealed.load(Ordering::Acquire) => HostReadiness::Pending,
            Readiness::Ready if !self.sealed.load(Ordering::Acquire) => HostReadiness::Ready,
            Readiness::Failed(error) => HostReadiness::Failed(error),
            _ => HostReadiness::Sealed,
        }
    }
    fn signal(&self) {
        let waker = self
            .pump_waker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(waker) = waker {
            waker.wake();
        }
        self.changed.notify_waiters();
    }
    fn seal(&self) {
        if !self.sealed.swap(true, Ordering::AcqRel) {
            self.signal();
        }
    }
}
impl Wake for Shared {
    fn wake(self: Arc<Self>) {
        Shared::signal(&self);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        Shared::signal(self);
    }
}
pub struct ProcessHost {
    sender: Option<mpsc::Sender<Box<HostRecord>>>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    finished: Option<oneshot::Receiver<ExitPayload>>,
}
pub struct PendingHost {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    finished: Option<oneshot::Receiver<ExitPayload>>,
    joined: Option<HostReport>,
}
struct ExitPayload {
    report: HostReport,
}

impl ProcessHost {
    pub fn install(service: &Cancellation, limits: HostLimits) -> io::Result<Self> {
        #[cfg(test)]
        {
            Self::install_inner(service, limits, None)
        }
        #[cfg(not(test))]
        {
            Self::install_inner(service, limits)
        }
    }
    #[cfg(test)]
    pub(crate) fn install_scheduled(
        service: &Cancellation,
        limits: HostLimits,
        schedule: FixtureSchedule,
    ) -> io::Result<Self> {
        Self::install_inner(service, limits, Some(schedule))
    }
    fn install_inner(
        service: &Cancellation,
        limits: HostLimits,
        #[cfg(test)] schedule: Option<FixtureSchedule>,
    ) -> io::Result<Self> {
        if service.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let shared = Arc::new(Shared {
            service: service.clone(),
            sealed: AtomicBool::new(false),
            readiness: Mutex::new(Readiness::Starting),
            slots: Mutex::new(vec![false; limits.slots].into_boxed_slice()),
            pump_waker: Mutex::new(None),
            changed: Notify::new(),
        });
        let (sender, receiver) = mpsc::channel(limits.slots);
        let state = HostState {
            receiver,
            records: std::iter::repeat_with(|| None).take(limits.slots).collect(),
            shared: Arc::clone(&shared),
            disconnected: false,
        };
        if service.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let (completed, finished) = oneshot::channel();
        let thread = thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                #[cfg(test)]
                let payload = thread_main(state, limits, schedule);
                #[cfg(not(test))]
                let payload = thread_main(state, limits);
                let _ = completed.send(payload);
            })?;
        Ok(Self {
            sender: Some(sender),
            shared,
            thread: Some(thread),
            finished: Some(finished),
        })
    }
    pub async fn ready(&self, deadline: Instant) -> HostReadiness {
        loop {
            let changed = self.shared.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let observation = self.shared.readiness();
            if !matches!(observation, HostReadiness::Pending) || Instant::now() >= deadline {
                return observation;
            }
            if tokio::time::timeout_at(deadline.into(), changed)
                .await
                .is_err()
            {
                return self.shared.readiness();
            }
        }
    }
    pub(crate) fn reserve(&self) -> Result<Arc<HostSlot>, crate::OperationAdmissionFailure> {
        match self.shared.readiness() {
            HostReadiness::Ready => (),
            HostReadiness::Pending => return Err(crate::OperationAdmissionFailure::HostNotReady),
            HostReadiness::Failed(_) | HostReadiness::Sealed => {
                return Err(crate::OperationAdmissionFailure::HostSealed);
            }
        }
        let mut slots = self.shared.slots.lock().unwrap_or_else(|e| e.into_inner());
        if self.shared.service.is_cancelled() {
            self.shared.seal();
        }
        if self.shared.sealed.load(Ordering::Acquire) {
            return Err(crate::OperationAdmissionFailure::HostSealed);
        }
        let index = slots
            .iter()
            .position(|occupied| !occupied)
            .ok_or(crate::OperationAdmissionFailure::CapacityFull)?;
        slots[index] = true;
        Ok(Arc::new(HostSlot {
            shared: Arc::clone(&self.shared),
            index,
            reservation_released: AtomicBool::new(false),
            snapshot: Mutex::new(RecordSnapshot::reserved()),
            changed: Notify::new(),
        }))
    }
    pub(crate) fn service_signal(&self) -> Cancellation {
        self.shared.service.clone()
    }
    pub(crate) fn sender(&self) -> Option<mpsc::Sender<Box<HostRecord>>> {
        self.sender.clone()
    }
    pub async fn shutdown(mut self, deadline: Instant) -> HostShutdown {
        self.shared.seal();
        drop(self.sender.take());
        let mut pending = PendingHost {
            shared: Arc::clone(&self.shared),
            thread: self.thread.take(),
            finished: self.finished.take(),
            joined: None,
        };
        match pending.repair(deadline).await {
            HostShutdownObservation::Joined(report) => HostShutdown::Joined(report),
            HostShutdownObservation::Pending => HostShutdown::Pending(pending),
        }
    }
}
impl Drop for ProcessHost {
    fn drop(&mut self) {
        self.shared.seal();
    }
}
impl PendingHost {
    pub async fn repair(&mut self, deadline: Instant) -> HostShutdownObservation {
        if let Some(report) = self.joined {
            return HostShutdownObservation::Joined(report);
        }
        loop {
            let changed = self.shared.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if Instant::now() >= deadline {
                return HostShutdownObservation::Pending;
            }
            let result = tokio::select! {
                result = self.finished.as_mut().expect("host completion receiver") => result,
                _ = changed => continue,
                _ = tokio::time::sleep_until(deadline.into()) => return HostShutdownObservation::Pending,
            };
            self.finished.take();
            let failure = HostReport {
                failure: Some(HostFailure::Panic(HostPhase::Pump)),
            };
            let report = match self.thread.take().expect("host thread").join() {
                Ok(()) => result.map_or(failure, |payload| payload.report),
                Err(_) => failure,
            };
            self.joined = Some(report);
            return HostShutdownObservation::Joined(report);
        }
    }
}
impl Drop for PendingHost {
    fn drop(&mut self) {
        self.shared.seal();
    }
}
impl fmt::Debug for ProcessHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProcessHost")
    }
}
impl fmt::Debug for PendingHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PendingHost")
    }
}
impl fmt::Debug for HostShutdown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Joined(report) => f.debug_tuple("Joined").field(report).finish(),
            Self::Pending(_) => f.write_str("PendingHost"),
        }
    }
}

pub(crate) struct HostSlot {
    shared: Arc<Shared>,
    pub(crate) index: usize,
    reservation_released: AtomicBool,
    snapshot: Mutex<RecordSnapshot>,
    changed: Notify,
}
impl HostSlot {
    pub(crate) fn observe(&self) -> RecordSnapshot {
        *self.snapshot.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub(crate) fn publish(&self, observation: RecordSnapshot) {
        let mut snapshot = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        if *snapshot == observation {
            return;
        }
        *snapshot = observation;
        if observation.phase == RecordPhase::Closed
            && matches!(
                observation.supervision.cleanup,
                Cleanup::Complete | Cleanup::NotSpawned
            )
        {
            // The driver has dropped its native owner before publishing this
            // outcome. Release capacity before observers can see completion;
            // retaining an old observation or stop handle is not a live process.
            self.release_reservation();
        }
        drop(snapshot);
        self.changed.notify_waiters();
    }
    fn release_reservation(&self) {
        if !self.reservation_released.swap(true, Ordering::AcqRel) {
            self.shared.slots.lock().unwrap_or_else(|e| e.into_inner())[self.index] = false;
            self.shared.signal();
        }
    }
    pub(crate) fn wake(&self) {
        self.shared.signal();
    }
    pub(crate) fn sealed(&self) -> bool {
        self.shared.sealed.load(Ordering::Acquire)
    }
    pub(crate) async fn host_closed(&self) {
        loop {
            let changed = self.shared.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.sealed() {
                return;
            }
            changed.await;
        }
    }
    pub(crate) async fn completed(&self) -> RecordSnapshot {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let snapshot = self.observe();
            if snapshot.finished() {
                return snapshot;
            }
            changed.await;
        }
    }
}
impl Drop for HostSlot {
    fn drop(&mut self) {
        // A late observer drop must not release a newer use of the same index.
        self.release_reservation();
    }
}

struct HostState {
    receiver: mpsc::Receiver<Box<HostRecord>>,
    records: Vec<Option<Box<HostRecord>>>,
    shared: Arc<Shared>,
    disconnected: bool,
}
impl HostState {
    fn publish_infrastructure_failure(&mut self) {
        // Reject admissions first, then copy every already accepted record into
        // the fixed owner table without polling its F or moving native custody.
        *self
            .shared
            .readiness
            .lock()
            .unwrap_or_else(|e| e.into_inner()) =
            Readiness::Failed(HostFailure::Panic(HostPhase::Pump));
        self.receiver.close();
        for _ in 0..self.records.len() {
            let Ok(record) = self.receiver.try_recv() else {
                break;
            };
            let index = record.slot_index();
            // One factory slot has exactly one submitted record. Existing
            // records retain that slot, so an accepted duplicate is impossible.
            self.records[index] = Some(record);
        }
        for record in self.records.iter().flatten() {
            record.infrastructure_unknown();
        }
        self.shared
            .pump_waker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        self.shared.seal();
        self.shared.signal();
    }
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        *self
            .shared
            .pump_waker
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(cx.waker().clone());
        // One pre-admitted Shared allocation provides the actual wake adapter.
        // There is no per-poll allocation or strong backreference to a record.
        let waker = Waker::from(Arc::clone(&self.shared));
        let mut driver_context = Context::from_waker(&waker);
        if self.shared.sealed.load(Ordering::Acquire) {
            self.receiver.close();
        }
        // One bounded pass can admit no more records than the installed table.
        for _ in 0..self.records.len() {
            if self.disconnected {
                break;
            }
            match self.receiver.poll_recv(&mut driver_context) {
                Poll::Ready(Some(record)) => {
                    let index = record.slot_index();
                    if self.records[index].is_some() {
                        self.shared.seal();
                        // Only the host polls admitted records. This received record
                        // has not been polled and therefore has no native child.
                        record.reject_unpolled();
                        continue;
                    }
                    self.records[index] = Some(record);
                }
                Poll::Ready(None) => {
                    self.disconnected = true;
                    self.shared.seal();
                    break;
                }
                Poll::Pending => break,
            }
        }
        let sealed = self.shared.sealed.load(Ordering::Acquire);
        for record in &mut self.records {
            if let Some(active) = record {
                if sealed {
                    active.close();
                }
                if active.poll_once(&mut driver_context).is_ready() {
                    drop(record.take());
                }
            }
        }
        if self.disconnected && self.records.iter().all(Option::is_none) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
struct Pump<'a> {
    state: &'a mut HostState,
}
impl Future for Pump<'_> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.get_mut().state.poll(cx)
    }
}
// Physical harness scheduling only. The actual installed thread/runtime/state
// remain the sole owner; no fabricated readiness, queue, record or authority.
#[cfg(test)]
pub(crate) struct FixtureSchedule {
    pub(crate) start: std::sync::mpsc::Receiver<()>,
    pub(crate) before_ready: Option<tokio::sync::oneshot::Sender<()>>,
    pub(crate) close_queue: std::sync::mpsc::Receiver<()>,
    pub(crate) queue_closed: tokio::sync::oneshot::Sender<()>,
    pub(crate) pump: std::sync::mpsc::Receiver<()>,
}
fn thread_main(
    mut state: HostState,
    limits: HostLimits,
    #[cfg(test)] mut schedule: Option<FixtureSchedule>,
) -> ExitPayload {
    let runtime = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(limits.blocking_threads)
            .build()
    }));
    let runtime = match runtime {
        Ok(Ok(runtime)) => runtime,
        Ok(Err(error)) => {
            let failure = HostFailure::Io(error.into());
            *state
                .shared
                .readiness
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Readiness::Failed(failure);
            state.shared.seal();
            return ExitPayload {
                report: HostReport {
                    failure: Some(failure),
                },
            };
        }
        Err(_) => {
            let failure = HostFailure::Panic(HostPhase::Installation);
            *state
                .shared
                .readiness
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Readiness::Failed(failure);
            state.shared.seal();
            return ExitPayload {
                report: HostReport {
                    failure: Some(failure),
                },
            };
        }
    };
    #[cfg(test)]
    if let Some(schedule) = &mut schedule {
        // Real thread/runtime reached the pre-Ready gate; no synthetic state.
        if let Some(arrived) = schedule.before_ready.take() {
            let _ = arrived.send(());
        }
        let _ = schedule.start.recv();
    }
    if state.shared.service.is_cancelled() {
        let failure = HostFailure::Io(io::Error::from(io::ErrorKind::Interrupted).into());
        *state
            .shared
            .readiness
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Readiness::Failed(failure);
        state.shared.seal();
        drop(runtime);
        return ExitPayload {
            report: HostReport {
                failure: Some(failure),
            },
        };
    }
    *state
        .shared
        .readiness
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Readiness::Ready;
    state.shared.signal();
    #[cfg(test)]
    if let Some(schedule) = schedule {
        if schedule.close_queue.recv().is_ok() {
            state.receiver.close();
            let _ = schedule.queue_closed.send(());
        }
        // Keep the actual receiver closed before the first pump so submission
        // can observe Closed without a fabricated sealed/readiness transition.
        let _ = schedule.pump.recv();
    }
    let service = state.shared.service.clone();
    let shared = Arc::clone(&state.shared);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.block_on(async {
            let pump = Pump { state: &mut state };
            tokio::pin!(pump);
            tokio::select! {
                _ = service.cancelled() => { shared.seal(); pump.await; }
                _ = &mut pump => (),
            }
        });
    }));
    let failure = if result.is_err() {
        state.publish_infrastructure_failure();
        Some(HostFailure::Panic(HostPhase::Pump))
    } else {
        *state
            .shared
            .readiness
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Readiness::Sealed;
        None
    };
    state
        .shared
        .pump_waker
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    state.shared.signal();
    drop(state);
    drop(runtime);
    ExitPayload {
        report: HostReport { failure },
    }
}
