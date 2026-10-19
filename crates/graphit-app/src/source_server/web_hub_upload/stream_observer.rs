//! Test-only phase observation; no pause, grant, payload or production API.
use super::{Publication, Query};
use graphit_core::project_identity::ProjectId;
use graphit_hub::ArtifactType;
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use tokio::sync::oneshot;
type Key = (String, String, String, String);
static OBSERVATIONS: OnceLock<Mutex<HashMap<Key, oneshot::Sender<PathBuf>>>> = OnceLock::new();
fn observations() -> &'static Mutex<HashMap<Key, oneshot::Sender<PathBuf>>> {
    OBSERVATIONS.get_or_init(Mutex::default)
}
pub(crate) struct Guard {
    key: Key,
    receiver: Option<oneshot::Receiver<PathBuf>>,
}
pub(crate) fn arm(
    project: ProjectId,
    kind: ArtifactType,
    id: &str,
    version: &str,
) -> io::Result<Guard> {
    let key = (
        project.to_string(),
        format!("{kind:?}"),
        id.to_owned(),
        version.to_owned(),
    );
    let (send, receiver) = oneshot::channel();
    let mut entries = observations().lock().map_err(|_| io::ErrorKind::Other)?;
    if entries.contains_key(&key) {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    entries.insert(key.clone(), send);
    Ok(Guard {
        key,
        receiver: Some(receiver),
    })
}
impl Guard {
    pub(crate) async fn entered(&mut self) -> io::Result<PathBuf> {
        self.receiver
            .take()
            .ok_or(io::ErrorKind::InvalidInput)?
            .await
            .map_err(|_| io::ErrorKind::BrokenPipe.into())
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if let Ok(mut entries) = observations().lock() {
            entries.remove(&self.key);
        }
    }
}
pub(super) fn observe(query: &Query, path: &Path) {
    let (kind, id, version) = match &query.publication {
        Publication::File(value) => (
            value.artifact_type.artifact_type(),
            &value.id,
            &value.version,
        ),
        Publication::Native(value) => (value.artifact_type, &value.id, &value.version),
    };
    let key = (
        query.project_id.to_string(),
        format!("{kind:?}"),
        id.clone(),
        version.clone(),
    );
    let send = observations()
        .lock()
        .ok()
        .and_then(|mut entries| entries.remove(&key));
    if let Some(send) = send {
        let _ = send.send(path.to_owned());
    }
}
