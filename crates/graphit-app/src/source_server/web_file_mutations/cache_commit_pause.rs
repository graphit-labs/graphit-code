//! Test-only witness after real cache cleanup, before registry admission.
use graphit_core::project_identity::ProjectId;
use graphit_hub::{ArtifactType, SourceRef, policy::RequestContext};
use std::{
    io,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

type Key = (ProjectId, ArtifactType, String, String);
fn key(source: &SourceRef) -> Key {
    (
        source.producer(),
        source.artifact_type(),
        source.id().as_str().to_owned(),
        source.version().as_str().to_owned(),
    )
}
struct Signals {
    entered: AtomicBool,
    released: AtomicBool,
}
static PAUSES: Mutex<Vec<(Key, Weak<Signals>)>> = Mutex::new(Vec::new());
pub(crate) struct CacheCommitPause {
    signals: Arc<Signals>,
}
impl CacheCommitPause {
    pub(crate) fn entered(&self) -> bool {
        self.signals.entered.load(Ordering::Acquire)
    }
    pub(crate) fn release(&self) {
        self.signals.released.store(true, Ordering::Release);
    }
}
impl Drop for CacheCommitPause {
    fn drop(&mut self) {
        self.release();
    }
}
pub(crate) fn after_cache(source: &SourceRef) -> CacheCommitPause {
    let signals = Arc::new(Signals {
        entered: AtomicBool::new(false),
        released: AtomicBool::new(false),
    });
    let key = key(source);
    let mut pauses = PAUSES.lock().expect("private cache scheduling lock");
    pauses.retain(|(_, value)| value.strong_count() != 0);
    assert!(
        !pauses.iter().any(|(existing, _)| existing == &key),
        "one cache pause per actual source tuple"
    );
    pauses.push((key, Arc::downgrade(&signals)));
    CacheCommitPause { signals }
}
pub(super) fn observed(source: &SourceRef, original: &RequestContext<'_>) -> io::Result<()> {
    let key = key(source);
    let signals = PAUSES
        .lock()
        .map_err(|_| io::ErrorKind::Other)?
        .iter()
        .find(|(existing, _)| existing == &key)
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
