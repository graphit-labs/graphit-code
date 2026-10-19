use super::service::LiveSessionService;
use std::{sync::Arc, time::Instant};

impl LiveSessionService {
    /// First stop new runs and signal the admitted operations; Root then joins its Source jobs.
    pub fn request_stop(&self) {
        self.stop.cancel();
        let Ok(slots) = self.slots.lock() else {
            return;
        };
        for slot in slots.values() {
            if let Ok(state) = slot.try_lock() {
                if let Some(original) = &state.preparing_original {
                    original.cancellation().cancel();
                }
                if let Some(cancel) = &state.active {
                    cancel.cancel();
                }
                if let Some(owner) = &state.process {
                    owner.stop();
                }
            }
        }
    }
    /// Call after the owning Source jobs have joined. A live future, pending IO,
    /// retained child, missing EOF or failed join remains incomplete.
    /// Ready workspaces stay private on disk for native Load's same-cwd contract;
    /// opening them again requires fresh Auth and fresh Core identity captures.
    pub async fn shutdown(&self, deadline: Instant) -> bool {
        self.request_stop();
        let slots = match self.slots.try_lock() {
            Ok(slots) => slots.values().map(Arc::clone).collect::<Vec<_>>(),
            Err(_) => return false,
        };
        let mut complete = true;
        for slot in slots {
            let owner = match slot.try_lock() {
                Ok(state) => {
                    if state.preparing_original.is_some() {
                        complete = false;
                    }
                    // A finished run can retain active/process custody after its first
                    // bounded join timed out. Only a still-live future prevents shutdown
                    // before observing the real retained owner below.
                    if state.active.is_some() && !state.abandoned && state.terminal.is_none() {
                        complete = false;
                    }
                    if state.terminal.as_ref().is_some_and(|terminal| {
                        (!terminal.joined || !terminal.clean) && state.process.is_none()
                    }) {
                        complete = false;
                    }
                    state.process.as_ref().map(Arc::clone)
                }
                Err(_) => {
                    complete = false;
                    continue;
                }
            };
            if let Some(owner) = owner {
                owner.stop();
                if owner.spawned()
                    && (!owner.observe(deadline).await || !owner.clean_joined().await)
                {
                    complete = false;
                }
            }
            if Instant::now() >= deadline {
                return false;
            }
        }
        complete
    }
}
