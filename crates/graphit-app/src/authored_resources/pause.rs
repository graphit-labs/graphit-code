//! Test-only scheduling witness after a real Core capture; it creates no proof.
use graphit_core::project_identity::ProjectId;
use graphit_hub::policy::RequestContext;
use std::{
    io,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

struct Signals {
    entered: AtomicBool,
    released: AtomicBool,
}
static PAUSES: Mutex<Vec<(ProjectId, Weak<Signals>)>> = Mutex::new(Vec::new());
pub(crate) struct CapturePause {
    signals: Arc<Signals>,
}
impl CapturePause {
    pub(crate) fn entered(&self) -> bool {
        self.signals.entered.load(Ordering::Acquire)
    }
    pub(crate) fn release(&self) {
        self.signals.released.store(true, Ordering::Release);
    }
}
impl Drop for CapturePause {
    fn drop(&mut self) {
        self.release();
    }
}
pub(crate) fn after_capture(project: ProjectId) -> CapturePause {
    let signals = Arc::new(Signals {
        entered: AtomicBool::new(false),
        released: AtomicBool::new(false),
    });
    let mut pauses = PAUSES.lock().expect("private capture scheduling lock");
    pauses.retain(|(_, value)| value.strong_count() != 0);
    assert!(
        !pauses.iter().any(|(id, _)| *id == project),
        "one capture pause per fixture PROJECT"
    );
    pauses.push((project, Arc::downgrade(&signals)));
    CapturePause { signals }
}
pub(super) fn observed(project: ProjectId, original: &RequestContext<'_>) -> io::Result<()> {
    let signals = PAUSES
        .lock()
        .map_err(|_| io::ErrorKind::Other)?
        .iter()
        .find(|(id, _)| *id == project)
        .and_then(|(_, value)| value.upgrade());
    let Some(signals) = signals else {
        return Ok(());
    };
    signals.entered.store(true, Ordering::Release);
    while !signals.released.load(Ordering::Acquire) {
        original.check()?;
        std::thread::sleep(Duration::from_millis(5));
    }
    original.check()
}
