//! Explicit test-only cancellation of the real publisher before replacement.
//! This default-off feature grants no authority and never constructs cleanup proofs.
use super::{AuthDocumentStore, RequestIdentity, SnapshotContext};
use crate::file_lock::Cancellation;
use std::{cell::Cell, fmt, marker::PhantomData, rc::Rc, sync::Arc};

/// Closed arming failures, without paths, bodies or retained native errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmError {
    Busy,
    InvalidContext,
}
impl fmt::Display for ArmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Busy => "publication cancellation fixture already armed",
            Self::InvalidContext => "publication cancellation fixture context is not live",
        })
    }
}
impl std::error::Error for ArmError {}

#[derive(Default)]
struct State {
    sequence: u64,
    lease: Option<Lease>,
}
struct Lease {
    id: u64,
    store: Arc<()>,
    request: RequestIdentity,
    cancellation: Option<Cancellation>,
}
thread_local! {
    static STATE: Cell<State> = const { Cell::new(State { sequence: 0, lease: None }) };
}

/// One opaque, thread-bound lease. Drop only disarms the fixture, without I/O.
/// A consumed trigger retains the lease until this guard drops.
///
/// ```compile_fail
/// use graphit_core::auth_document::test_support::PublicationCancellation;
/// fn require<T: Send>() {}
/// require::<PublicationCancellation>();
/// ```
/// ```compile_fail
/// use graphit_core::auth_document::test_support::PublicationCancellation;
/// fn require<T: Sync>() {}
/// require::<PublicationCancellation>();
/// ```
/// ```compile_fail
/// use graphit_core::auth_document::test_support::PublicationCancellation;
/// fn copy(value: &PublicationCancellation) { let _ = Clone::clone(value); }
/// ```
/// ```compile_fail
/// use graphit_core::auth_document::test_support::PublicationCancellation;
/// fn expose(value: &PublicationCancellation) { println!("{value:?}"); }
/// ```
/// ```compile_fail
/// use graphit_core::auth_document::test_support::PublicationCancellation;
/// fn require<T: serde::Serialize>() {}
/// require::<PublicationCancellation>();
/// ```
/// ```compile_fail
/// use graphit_core::auth_document::test_support::PublicationCancellation;
/// fn require<T: serde::de::DeserializeOwned>() {}
/// require::<PublicationCancellation>();
/// ```
#[must_use]
pub struct PublicationCancellation {
    id: u64,
    _thread: PhantomData<Rc<()>>,
}
impl PublicationCancellation {
    /// Arm one real store and the original live request, without filesystem I/O.
    /// A shared original request observes the same cancellation.
    pub fn before_replace(
        store: &AuthDocumentStore,
        context: &SnapshotContext,
    ) -> Result<Self, ArmError> {
        context.check().map_err(|_| ArmError::InvalidContext)?;
        STATE
            .try_with(|slot| {
                let mut state = slot.take();
                if state.lease.is_some() {
                    slot.set(state);
                    return Err(ArmError::Busy);
                }
                let Some(id) = state.sequence.checked_add(1) else {
                    slot.set(state);
                    return Err(ArmError::Busy);
                };
                state.sequence = id;
                state.lease = Some(Lease {
                    id,
                    store: Arc::clone(&store.identity),
                    request: context.request_identity(),
                    cancellation: Some(context.cancellation().clone()),
                });
                slot.set(state);
                Ok(Self {
                    id,
                    _thread: PhantomData,
                })
            })
            .unwrap_or(Err(ArmError::Busy))
    }
}
impl Drop for PublicationCancellation {
    fn drop(&mut self) {
        let _ = STATE.try_with(|slot| {
            let mut state = slot.take();
            if state
                .lease
                .as_ref()
                .is_some_and(|lease| lease.id == self.id)
            {
                state.lease = None;
            }
            slot.set(state);
        });
    }
}

pub(super) fn trigger_before_replace(store: &AuthDocumentStore, context: &SnapshotContext) {
    let signal = STATE
        .try_with(|slot| {
            let mut state = slot.take();
            let signal = state.lease.as_mut().and_then(|lease| {
                if Arc::ptr_eq(&lease.store, &store.identity) && lease.request.matches(context) {
                    lease.cancellation.take()
                } else {
                    None
                }
            });
            slot.set(state);
            signal
        })
        .ok()
        .flatten();
    // Cancellation may notify waiters; it must occur after restoring the TLS state.
    if let Some(signal) = signal {
        signal.cancel();
    }
}
