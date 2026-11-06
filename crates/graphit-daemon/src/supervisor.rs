//! A checkout supervisor serializes producer work and owns its native watcher.

use crate::watcher::{ChangeBatch, ProjectWatcher, WatchConfig};
use futures::FutureExt;
use graphit_core::{
    file_lock::Cancellation, project_identity::ProjectId, version_context::VersionContext,
};
use std::{
    collections::HashMap,
    future::Future,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Weak},
};
use tokio::{sync::Notify, task::JoinHandle};

/// Artifact identity alone is insufficient: each checkout keeps its own captured version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectContext {
    pub id: ProjectId,
    pub version: VersionContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SupervisionKey {
    pub id: ProjectId,
    pub checkout: PathBuf,
    pub version: String,
}

impl ProjectContext {
    pub fn key(&self) -> SupervisionKey {
        SupervisionKey {
            id: self.id,
            checkout: self.root().to_owned(),
            version: self.version.version().to_owned(),
        }
    }

    pub fn root(&self) -> &Path {
        self.version.scope()
    }
}

/// A coalesced execution hint for one captured selection, without producer ownership or authority.
#[derive(Clone)]
pub struct ProjectRescanHandle {
    key: SupervisionKey,
    wake: Weak<Notify>,
    records: Weak<Notify>,
    cancellation: Cancellation,
}

impl ProjectRescanHandle {
    pub fn key(&self) -> &SupervisionKey {
        &self.key
    }

    /// Coalesced derived-record work without reparsing the checkout.
    pub fn request_records(&self) -> bool {
        let Some(wake) = self.records.upgrade() else {
            return false;
        };
        if self.cancellation.is_cancelled() {
            return false;
        }
        wake.notify_one();
        true
    }

    /// Queues a hint while the owner is live. Cancellation may discard an already queued hint.
    pub fn request_rescan(&self) -> bool {
        let Some(wake) = self.wake.upgrade() else {
            return false;
        };
        if self.cancellation.is_cancelled() {
            return false;
        }
        wake.notify_one();
        true
    }
}

/// Producers own the resources borrowed by their future and must complete cancellation
/// cleanup before returning. Stop signals cancellation and awaits that future, without aborting it.
pub struct ProjectSupervisor {
    context: ProjectContext,
    cancellation: Cancellation,
    rescan: Weak<Notify>,
    records: Weak<Notify>,
    task: Option<JoinHandle<io::Result<()>>>,
}

enum SupervisorEvent {
    Hint(ChangeBatch),
    Native(Option<ChangeBatch>),
}

impl ProjectSupervisor {
    pub async fn start<F, Fut>(
        context: ProjectContext,
        config: WatchConfig,
        mut producer: F,
    ) -> io::Result<Self>
    where
        F: FnMut(ProjectContext, ChangeBatch, Cancellation) -> Fut + Send + 'static,
        Fut: Future<Output = io::Result<()>> + Send + 'static,
    {
        Self::start_controlled(
            context,
            config,
            crate::watcher::ExternalControls::default(),
            move |context, batch, cancellation, _, _, _| producer(context, batch, cancellation),
        )
        .await
    }

    /// Every event cancels the background token (fifth argument). Source events
    /// also cancel source preemption (sixth); record changes leave parsing alive.
    /// The producer owns cleanup until return; lifetime cancellation stops it.
    pub async fn start_controlled<F, Fut>(
        context: ProjectContext,
        config: WatchConfig,
        external: crate::watcher::ExternalControls,
        producer: F,
    ) -> io::Result<Self>
    where
        F: FnMut(
                ProjectContext,
                ChangeBatch,
                Cancellation,
                crate::watcher::ProjectWatchControls,
                Cancellation,
                Cancellation,
            ) -> Fut
            + Send
            + 'static,
        Fut: Future<Output = io::Result<()>> + Send + 'static,
    {
        let expected_root = context.root().to_owned();
        let watcher = tokio::task::spawn_blocking(move || {
            if std::fs::canonicalize(&config.root)? != expected_root {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "watcher root differs from captured checkout",
                ));
            }
            let mut watcher = ProjectWatcher::start(config)?;
            watcher.watch_external(external)?;
            Ok::<_, io::Error>(watcher)
        })
        .await
        .map_err(io::Error::other)??;
        Ok(Self::supervise(context, watcher, producer))
    }

    fn supervise<F, Fut>(
        context: ProjectContext,
        mut watcher: ProjectWatcher,
        mut producer: F,
    ) -> Self
    where
        F: FnMut(
                ProjectContext,
                ChangeBatch,
                Cancellation,
                crate::watcher::ProjectWatchControls,
                Cancellation,
                Cancellation,
            ) -> Fut
            + Send
            + 'static,
        Fut: Future<Output = io::Result<()>> + Send + 'static,
    {
        let cancellation = Cancellation::default();
        let stop = cancellation.clone();
        let wake = Arc::new(Notify::new());
        let rescan = Arc::downgrade(&wake);
        let records_wake = Arc::new(Notify::new());
        let records = Arc::downgrade(&records_wake);
        let captured = context.clone();
        let controls = watcher.controls();
        let task = tokio::spawn(async move {
            let result = async {
                let mut queued = Some(ChangeBatch { rescan: true, ..Default::default() });
                let mut watcher_closed = false;
                loop {
                    if stop.is_cancelled() {
                        break;
                    }
                    let batch = match queued.take() {
                        Some(batch) => batch,
                        None if watcher_closed => break,
                        None => tokio::select! {
                            biased;
                            () = stop.cancelled() => break,
                            () = wake.notified() => ChangeBatch { rescan: true, ..Default::default() },
                            () = records_wake.notified() => ChangeBatch { records_changed: true, ..Default::default() },
                            batch = watcher.next() => match batch { Some(batch) => batch, None => break },
                        },
                    };
                    // A notified waiter and Notify's stored permit may represent
                    // the same burst. Hints already covered by this admission do
                    // not need another complete producer invocation.
                    if batch.rescan { let _ = wake.notified().now_or_never(); }
                    if batch.records_changed { let _ = records_wake.notified().now_or_never(); }
                    let background = Cancellation::default();
                    let source_preemption = Cancellation::default();
                    let mut work = Box::pin(producer(
                        captured.clone(), batch, stop.clone(), controls.clone(), background.clone(), source_preemption.clone(),
                    ));
                    let mut stopping = false;
                    loop {
                        tokio::select! {
                            biased;
                            result = &mut work => {
                                background.cancel();
                                source_preemption.cancel();
                                result?;
                                break;
                            },
                            () = stop.cancelled(), if !stopping => {
                                stopping = true;
                                background.cancel();
                                source_preemption.cancel();
                            },
                            event = async {
                                tokio::select! {
                                    () = wake.notified() => SupervisorEvent::Hint(ChangeBatch { rescan: true, ..Default::default() }),
                                    () = records_wake.notified() => SupervisorEvent::Hint(ChangeBatch { records_changed: true, ..Default::default() }),
                                    batch = watcher.next() => SupervisorEvent::Native(batch),
                                }
                            }, if !stopping && !watcher_closed => {
                                background.cancel();
                                let batch = match event {
                                    // Administrative hints wait for source completion. Native
                                    // rescan/overflow remains preemptive even without FILE paths.
                                    SupervisorEvent::Hint(batch) => Some(batch),
                                    SupervisorEvent::Native(batch) => {
                                        if batch.as_ref().is_some_and(|batch|
                                            batch.rescan || batch.controls_changed
                                                || !batch.paths.is_empty() || batch.error.is_some()) {
                                            source_preemption.cancel();
                                        }
                                        batch
                                    },
                                };
                                match batch {
                                    Some(batch) => {
                                        match &mut queued {
                                            Some(queued) => queued.merge(batch),
                                            None => queued = Some(batch),
                                        }
                                    },
                                    None => watcher_closed = true,
                                }
                            },
                        }
                    }
                }
                Ok(())
            }.await;
            stop.cancel();
            tokio::task::spawn_blocking(move || watcher.stop())
                .await
                .map_err(io::Error::other)?;
            result
        });
        Self {
            context,
            cancellation,
            rescan,
            records,
            task: Some(task),
        }
    }

    pub fn context(&self) -> &ProjectContext {
        &self.context
    }

    pub fn rescan_handle(&self) -> ProjectRescanHandle {
        ProjectRescanHandle {
            key: self.context.key(),
            wake: self.rescan.clone(),
            records: self.records.clone(),
            cancellation: self.cancellation.clone(),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.task.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    // Borrow the handle so interrupting the await leaves it owned for later cleanup.
    async fn join_retained(&mut self) -> io::Result<()> {
        let Some(task) = self.task.as_mut() else {
            return Ok(());
        };
        let result = task.await;
        self.task.take();
        result.map_err(io::Error::other)?
    }

    /// Await the producer, join the native watcher, and propagate producer/worker failures.
    pub async fn stop(mut self) -> io::Result<()> {
        self.cancellation.cancel();
        if let Some(task) = self.task.take() {
            task.await.map_err(io::Error::other)??;
        }
        Ok(())
    }

    /// Parking retains only identity and captured checkout version. Resume calls start
    /// with fresh producer resources and receives an initial rescan.
    pub async fn park(self) -> io::Result<ProjectContext> {
        let context = self.context.clone();
        self.stop().await?;
        Ok(context)
    }
}

impl Drop for ProjectSupervisor {
    fn drop(&mut self) {
        // The task remains alive to await producer cleanup and join the watcher.
        // Call stop/park when the caller needs completion or failure acknowledgement.
        self.cancellation.cancel();
    }
}

/// Activity policy belongs to the daemon's project/settings resolver.
#[derive(Clone, Debug)]
pub struct DesiredProject {
    pub context: ProjectContext,
    pub active: bool,
}

/// Reconciliation owns running supervisors and only lightweight parked contexts.
/// A changed checkout/version stops its predecessor before starting the replacement.
#[derive(Default)]
pub struct ProjectSupervisors {
    active: HashMap<SupervisionKey, ProjectSupervisor>,
    parked: HashMap<SupervisionKey, ProjectContext>,
}

impl ProjectSupervisors {
    /// Returns a signal only for the exact running selection; parked selections have no producer.
    pub fn rescan_handle(&self, key: &SupervisionKey) -> Option<ProjectRescanHandle> {
        self.active
            .get(key)
            .filter(|owner| !owner.is_finished() && !owner.cancellation.is_cancelled())
            .map(ProjectSupervisor::rescan_handle)
    }

    pub fn supervised(&self) -> impl Iterator<Item = &ProjectContext> {
        self.active.values().map(ProjectSupervisor::context)
    }

    pub fn parked(&self) -> impl Iterator<Item = &ProjectContext> {
        self.parked.values()
    }

    pub async fn reconcile<F, Fut>(
        &mut self,
        desired: Vec<DesiredProject>,
        mut start: F,
    ) -> io::Result<()>
    where
        F: FnMut(ProjectContext) -> Fut,
        Fut: Future<Output = io::Result<ProjectSupervisor>>,
    {
        let mut wanted = HashMap::with_capacity(desired.len());
        for project in desired {
            if wanted.insert(project.context.key(), project).is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "duplicate checkout/version in desired projects",
                ));
            }
        }
        let stopping: Vec<_> = self
            .active
            .iter()
            .filter(|(key, owner)| {
                wanted.get(*key).is_none_or(|project| !project.active)
                    || owner.is_finished()
                    || owner.cancellation.is_cancelled()
            })
            .map(|(key, _)| key.clone())
            .collect();
        let mut failure = self.stop_registered(&stopping).await.err();
        let blocked_replacements: Vec<_> = stopping
            .iter()
            .filter(|key| {
                self.active.contains_key(*key)
                    && wanted.get(*key).is_none_or(|project| !project.active)
            })
            .collect();
        self.parked.retain(|key, _| wanted.contains_key(key));
        for (key, project) in wanted {
            // A running owner or a failed predecessor remains owned by this registry.
            if self.active.contains_key(&key) {
                continue;
            }
            if !project.active {
                self.parked.insert(key, project.context);
                continue;
            }
            if blocked_replacements
                .iter()
                .any(|previous| previous.id == key.id && previous.checkout == key.checkout)
            {
                continue;
            }
            let owner = match start(project.context).await {
                Ok(owner) => owner,
                Err(error) => {
                    failure.get_or_insert(error);
                    continue;
                }
            };
            if owner.context().key() != key {
                if let Err(error) = owner.stop().await {
                    failure.get_or_insert(error);
                }
                failure.get_or_insert(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "producer factory changed checkout/version",
                ));
                continue;
            }
            self.parked.remove(&key);
            self.active.insert(key, owner);
        }
        failure.map_or(Ok(()), Err)
    }

    /// Cancel and join only the exact captured checkout/version selection.
    /// Pending or failed cleanup retains its owner; success removes only that selection.
    /// This key is owner identity, not authority, and does not change inventory or data.
    pub async fn stop_selection(&mut self, key: &SupervisionKey) -> io::Result<()> {
        if let Some(owner) = self.active.get_mut(key) {
            owner.cancel();
            owner.join_retained().await?;
        }
        self.active.remove(key);
        self.parked.remove(key);
        Ok(())
    }

    // Keep registered owners available if this await is interrupted, and cancel
    // the whole selected group before waiting for any producer cleanup.
    async fn stop_registered(&mut self, keys: &[SupervisionKey]) -> io::Result<()> {
        for key in keys {
            if let Some(owner) = self.active.get(key) {
                owner.cancel();
            }
        }
        let mut failure = None;
        for key in keys {
            if let Err(error) = self.stop_selection(key).await {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    /// Cancel and join every owner of this exact project checkout, across versions.
    /// Other checkout owners remain live. This does not change maintained inventory.
    /// All matching owners are cancelled before any join; cleanup failures remain errors.
    pub async fn stop_checkout(&mut self, id: ProjectId, checkout: &Path) -> io::Result<()> {
        let keys: Vec<_> = self
            .active
            .keys()
            .filter(|key| key.id == id && key.checkout == checkout)
            .cloned()
            .collect();
        let result = self.stop_registered(&keys).await;
        self.parked
            .retain(|key, _| key.id != id || key.checkout != checkout);
        result
    }

    pub async fn shutdown(&mut self) -> io::Result<()> {
        let keys: Vec<_> = self.active.keys().cloned().collect();
        let result = self.stop_registered(&keys).await;
        self.parked.clear();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::IgnoreConfig;
    use graphit_core::{
        brand::Brand,
        project_paths::{InstallationPaths, ProjectPaths},
        version_context::Checkout,
    };
    use std::{
        fs,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    use tempfile::TempDir;
    use tokio::{
        sync::{mpsc, oneshot},
        time::timeout,
    };

    fn context(root: &Path, version: &str) -> ProjectContext {
        fs::create_dir_all(root.join(".graphit")).unwrap();
        let installation =
            InstallationPaths::resolve(Brand::default(), root, None, Some(&root.join("global")))
                .unwrap();
        let paths = ProjectPaths::at(root, &installation).unwrap();
        ProjectContext {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            version: VersionContext::select(
                &paths,
                Checkout::Branch {
                    name: version,
                    default: "main",
                },
                "",
                None,
                "",
            )
            .unwrap(),
        }
    }

    // ProjectWatcher and captured contexts expose canonical checkout paths.
    // Keep low-level SDK representation tests on their original raw paths.
    fn canonical_tempdir() -> TempDir {
        TempDir::new_in(fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap()
    }

    fn config(root: &Path) -> WatchConfig {
        WatchConfig {
            root: root.to_owned(),
            ast: IgnoreConfig::default(),
            wiki: IgnoreConfig::default(),
            control_files: Vec::new(),
            control_directories: Vec::new(),
            debounce: Duration::from_millis(40),
        }
    }

    #[tokio::test]
    async fn native_delta_cancels_background_without_interrupting_parse_or_overlapping_producers() {
        let dir = canonical_tempdir();
        let changed = dir.path().join("changed.rs");
        let (entered_tx, mut entered) = mpsc::channel(8);
        let (cancelled_tx, cancelled) = oneshot::channel();
        let (release_parse, parse_wait) = oneshot::channel();
        let mut first = Some((cancelled_tx, parse_wait));
        let mut calls = 0;
        // Control the exact FILE event; native SDK coalescing is covered by
        // administrative_rescan_and_native_change_reconcile_after_owned_cleanup.
        let (watcher, native_events) = ProjectWatcher::channel_fixture();
        let owner = ProjectSupervisor::supervise(
            context(dir.path(), "feature"),
            watcher,
            move |_, batch, lifetime, _, background, source_preemption| {
                calls += 1;
                let number = calls;
                let entered = entered_tx.clone();
                let first = first.take();
                async move {
                    entered.send((number, batch)).await.unwrap();
                    if let Some((cancelled, parse_wait)) = first {
                        background.cancelled().await;
                        assert!(
                            source_preemption.is_cancelled(),
                            "a FILE delta preempts the full source pass"
                        );
                        assert!(
                            !lifetime.is_cancelled(),
                            "a newer delta must not cancel parsing"
                        );
                        cancelled.send(()).unwrap();
                        parse_wait.await.unwrap();
                        assert!(!lifetime.is_cancelled());
                    }
                    Ok(())
                }
            },
        );
        let (number, initial) = timeout(Duration::from_secs(8), entered.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(number, 1);
        assert!(initial.rescan);
        std::fs::write(&changed, "fn changed() {}\n").unwrap();
        native_events
            .send(ChangeBatch {
                paths: [changed.clone()].into(),
                ..Default::default()
            })
            .await
            .unwrap();
        timeout(Duration::from_secs(8), cancelled)
            .await
            .unwrap()
            .unwrap();
        assert!(
            entered.try_recv().is_err(),
            "the next producer must await parse/cleanup"
        );
        release_parse.send(()).unwrap();
        let (number, delta) = timeout(Duration::from_secs(8), entered.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(number, 2);
        assert!(!delta.rescan);
        assert!(
            delta.paths.contains(&changed),
            "the queued native FILE delta must be preserved"
        );
        owner.stop().await.unwrap();
    }

    #[tokio::test]
    async fn record_notifications_resume_after_background_cancellation_without_source_rescan() {
        let dir = TempDir::new().unwrap();
        let (tx, mut received) = mpsc::channel(4);
        let (release_initial, initial_wait) = oneshot::channel();
        let (release_next, next_wait) = oneshot::channel();
        let mut waits = [initial_wait, next_wait].into_iter();
        let mut initial = true;
        let owner = ProjectSupervisor::start_controlled(
            context(dir.path(), "feature"),
            config(dir.path()),
            crate::watcher::ExternalControls::default(),
            move |_, batch, _, _, background, source_preemption| {
                let first = std::mem::replace(&mut initial, false);
                let tx = tx.clone();
                let wait = waits.next();
                async move {
                    tx.send((
                        first,
                        batch.rescan,
                        batch.records_changed,
                        batch.paths.is_empty(),
                    ))
                    .await
                    .unwrap();
                    if first {
                        background.cancelled().await;
                        assert!(
                            !source_preemption.is_cancelled(),
                            "record hints must preserve the full source pass"
                        );
                    }
                    if let Some(wait) = wait {
                        wait.await.unwrap();
                    }
                    Ok(())
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(
            timeout(Duration::from_secs(8), received.recv())
                .await
                .unwrap()
                .unwrap(),
            (true, true, false, true)
        );
        let handle = owner.rescan_handle();
        for _ in 0..3 {
            assert!(handle.request_records());
        }
        assert!(
            received.try_recv().is_err(),
            "record processing waits for the current source owner"
        );
        release_initial.send(()).unwrap();
        assert_eq!(
            timeout(Duration::from_secs(8), received.recv())
                .await
                .unwrap()
                .unwrap(),
            (false, false, true, true)
        );
        assert!(
            received.try_recv().is_err(),
            "a burst is one pending record batch"
        );
        assert!(
            handle.request_records(),
            "a new hint during the next job is retained"
        );
        release_next.send(()).unwrap();
        assert_eq!(
            timeout(Duration::from_secs(8), received.recv())
                .await
                .unwrap()
                .unwrap(),
            (false, false, true, true)
        );
        owner.stop().await.unwrap();
        assert!(!handle.request_records());
        assert!(
            received.recv().await.is_none(),
            "no redundant record batch remains after the retained hint"
        );
    }

    #[tokio::test]
    async fn administrative_rescan_preserves_full_until_native_delta() {
        administrative_rescan_fixture(Some(false)).await;
    }

    #[tokio::test]
    async fn native_rescan_without_paths_preempts_full_and_waits_for_owned_cleanup() {
        administrative_rescan_fixture(Some(true)).await;
    }

    #[tokio::test]
    async fn administrative_rescan_and_native_change_reconcile_after_owned_cleanup() {
        administrative_rescan_fixture(None).await;
    }

    // Some(false): exact FILE delta; Some(true): pathless native rescan;
    // None: real SDK, which may report a coalesced/global FSEvents rescan.
    async fn administrative_rescan_fixture(channel_rescan: Option<bool>) {
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let changed = root.join("changed.rs");
        let (entered_tx, mut entered) = mpsc::channel(8);
        let (hint_observed, hint) = oneshot::channel();
        let (delta_observed, delta) = oneshot::channel();
        let (release_full, full_wait) = oneshot::channel();
        let mut first = Some((hint_observed, delta_observed, full_wait));
        let producer = move |ctx: ProjectContext,
                             batch: ChangeBatch,
                             lifetime: Cancellation,
                             _: crate::watcher::ProjectWatchControls,
                             background: Cancellation,
                             source_preemption: Cancellation| {
            let entered = entered_tx.clone();
            let first = first.take();
            async move {
                assert!(!source_preemption.is_cancelled());
                let contents = fs::read(ctx.root().join("changed.rs")).ok();
                entered.send((batch, contents)).await.unwrap();
                if let Some((hint_observed, delta_observed, full_wait)) = first {
                    // The background signal acknowledges consumption of the
                    // administrative hint, without a timer or scheduling guess.
                    background.cancelled().await;
                    hint_observed
                        .send(source_preemption.is_cancelled())
                        .unwrap();
                    source_preemption.cancelled().await;
                    assert!(!lifetime.is_cancelled());
                    delta_observed.send(()).unwrap();
                    full_wait.await.unwrap();
                }
                Ok(())
            }
        };
        let (owner, native_events) = if channel_rescan.is_some() {
            let (watcher, sender) = ProjectWatcher::channel_fixture();
            (
                ProjectSupervisor::supervise(context(&root, "feature"), watcher, producer),
                Some(sender),
            )
        } else {
            (
                ProjectSupervisor::start_controlled(
                    context(&root, "feature"),
                    config(&root),
                    crate::watcher::ExternalControls::default(),
                    producer,
                )
                .await
                .unwrap(),
                None,
            )
        };
        let (initial, contents) = timeout(Duration::from_secs(8), entered.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(initial.rescan);
        assert!(contents.is_none());
        let handle = owner.rescan_handle();
        for _ in 0..1000 {
            assert!(handle.request_rescan());
        }
        let preempted = timeout(Duration::from_secs(8), hint)
            .await
            .unwrap()
            .unwrap();
        if channel_rescan.is_some() {
            assert!(
                !preempted,
                "administrative hints must let the full source pass finish"
            );
        }
        assert!(entered.try_recv().is_err());
        fs::write(&changed, "fn changed() {}\n").unwrap();
        if let Some(sender) = native_events {
            let rescan = channel_rescan.unwrap();
            sender
                .send(ChangeBatch {
                    rescan,
                    paths: if rescan {
                        Default::default()
                    } else {
                        [changed.clone()].into()
                    },
                    ..Default::default()
                })
                .await
                .unwrap();
        }
        timeout(Duration::from_secs(8), delta)
            .await
            .unwrap()
            .unwrap();
        assert!(
            entered.try_recv().is_err(),
            "native delta awaits owned cleanup"
        );
        release_full.send(()).unwrap();
        let (next, contents) = timeout(Duration::from_secs(8), entered.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(next.rescan, "the administrative hint remains queued");
        assert!(next.error.is_none());
        assert_eq!(contents.as_deref(), Some(b"fn changed() {}\n".as_slice()));
        match channel_rescan {
            Some(false) => assert!(
                next.paths.contains(&changed),
                "the native FILE delta is retained"
            ),
            Some(true) => assert!(next.paths.is_empty(), "a native rescan needs no FILE path"),
            None => {} // The real full pass reconciles the file even after a global rescan.
        }
        owner.stop().await.unwrap();
    }

    #[tokio::test]
    async fn explicit_rescan_coalesces_during_jobs_without_parallel_work() {
        let dir = TempDir::new().unwrap();
        let (tx, mut entered) = mpsc::channel(8);
        let (release_initial, initial_wait) = oneshot::channel();
        let (release_next, next_wait) = oneshot::channel();
        let mut waits = [initial_wait, next_wait].into_iter();
        let mut call = 0;
        let owner = ProjectSupervisor::start(
            context(dir.path(), "feature"),
            config(dir.path()),
            move |ctx, batch, _| {
                call += 1;
                let number = call;
                let tx = tx.clone();
                let wait = waits.next();
                async move {
                    assert!(batch.rescan);
                    assert!(batch.paths.is_empty());
                    tx.send((number, ctx.key())).await.unwrap();
                    if let Some(wait) = wait {
                        wait.await.unwrap();
                    }
                    Ok(())
                }
            },
        )
        .await
        .unwrap();
        let handle = owner.rescan_handle();
        let first = timeout(Duration::from_secs(8), entered.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first, (1, handle.key().clone()));
        for _ in 0..1000 {
            assert!(handle.request_rescan());
        }
        assert!(entered.try_recv().is_err(), "producer remains serial");
        release_initial.send(()).unwrap();
        let second = timeout(Duration::from_secs(8), entered.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second, (2, handle.key().clone()));
        assert!(
            entered.try_recv().is_err(),
            "burst creates only one pending rescan"
        );
        assert!(
            handle.request_rescan(),
            "a hint during the next job is retained"
        );
        release_next.send(()).unwrap();
        let third = timeout(Duration::from_secs(8), entered.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(third, (3, handle.key().clone()));
        owner.stop().await.unwrap();
        assert!(!handle.request_rescan());
        assert!(
            handle.wake.upgrade().is_none(),
            "handle does not retain the signal owner"
        );
        assert!(entered.recv().await.is_none());
    }

    async fn rescan_test_owner(
        ctx: ProjectContext,
        tx: mpsc::Sender<SupervisionKey>,
    ) -> io::Result<ProjectSupervisor> {
        let cfg = config(ctx.root());
        ProjectSupervisor::start(ctx, cfg, move |ctx, batch, _| {
            let tx = tx.clone();
            async move {
                if batch.rescan {
                    tx.send(ctx.key()).await.map_err(io::Error::other)?;
                }
                Ok(())
            }
        })
        .await
    }

    #[tokio::test]
    async fn registry_rescan_is_exact_and_stale_handles_cannot_wake_replacements() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        let a_context = context(a.path(), "main");
        let b_context = context(b.path(), "feature");
        let a_key = a_context.key();
        let b_key = b_context.key();
        let (tx, mut batches) = mpsc::channel(8);
        let mut registry = ProjectSupervisors::default();
        registry
            .reconcile(
                vec![
                    DesiredProject {
                        context: a_context.clone(),
                        active: true,
                    },
                    DesiredProject {
                        context: b_context.clone(),
                        active: true,
                    },
                ],
                |ctx| rescan_test_owner(ctx, tx.clone()),
            )
            .await
            .unwrap();
        let first = timeout(Duration::from_secs(8), batches.recv())
            .await
            .unwrap()
            .unwrap();
        let second = timeout(Duration::from_secs(8), batches.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(first != second && [first, second].contains(&a_key));
        let old_a = registry.rescan_handle(&a_key).unwrap();
        let live_b = registry.rescan_handle(&b_key).unwrap();
        assert_eq!(old_a.key(), &a_key);
        let mut foreign = a_key.clone();
        foreign.version = "unselected".to_owned();
        assert!(registry.rescan_handle(&foreign).is_none());
        registry
            .reconcile(
                vec![
                    DesiredProject {
                        context: a_context.clone(),
                        active: false,
                    },
                    DesiredProject {
                        context: b_context.clone(),
                        active: true,
                    },
                ],
                |ctx| rescan_test_owner(ctx, tx.clone()),
            )
            .await
            .unwrap();
        assert!(registry.rescan_handle(&a_key).is_none());
        assert!(!old_a.request_rescan());
        assert!(live_b.request_rescan());
        assert_eq!(
            timeout(Duration::from_secs(8), batches.recv())
                .await
                .unwrap()
                .unwrap(),
            b_key
        );
        registry
            .reconcile(
                vec![
                    DesiredProject {
                        context: a_context,
                        active: true,
                    },
                    DesiredProject {
                        context: b_context,
                        active: true,
                    },
                ],
                |ctx| rescan_test_owner(ctx, tx.clone()),
            )
            .await
            .unwrap();
        assert_eq!(
            timeout(Duration::from_secs(8), batches.recv())
                .await
                .unwrap()
                .unwrap(),
            a_key
        );
        let replacement = registry.rescan_handle(&a_key).unwrap();
        assert!(!old_a.request_rescan());
        assert!(replacement.request_rescan());
        assert_eq!(
            timeout(Duration::from_secs(8), batches.recv())
                .await
                .unwrap()
                .unwrap(),
            a_key
        );
        registry.stop_selection(&a_key).await.unwrap();
        assert!(!replacement.request_rescan());
        assert!(registry.rescan_handle(&a_key).is_none());
        assert!(live_b.request_rescan());
        assert_eq!(
            timeout(Duration::from_secs(8), batches.recv())
                .await
                .unwrap()
                .unwrap(),
            b_key
        );
        registry.shutdown().await.unwrap();
        assert!(!live_b.request_rescan());
    }

    #[tokio::test]
    async fn failed_producer_invalidates_rescan_without_losing_its_error() {
        let dir = TempDir::new().unwrap();
        let (entered, started) = oneshot::channel();
        let (release, wait) = oneshot::channel();
        let mut signals = Some((entered, wait));
        let owner = ProjectSupervisor::start(
            context(dir.path(), "main"),
            config(dir.path()),
            move |_, _, _| {
                let (entered, wait) = signals.take().unwrap();
                async move {
                    entered.send(()).unwrap();
                    wait.await.unwrap();
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "producer denied",
                    ))
                }
            },
        )
        .await
        .unwrap();
        let handle = owner.rescan_handle();
        started.await.unwrap();
        release.send(()).unwrap();
        timeout(Duration::from_secs(8), handle.cancellation.cancelled())
            .await
            .unwrap();
        assert!(
            !handle.request_rescan(),
            "failed owner rejects hints before stop"
        );
        let failure = owner.stop().await.unwrap_err();
        assert_eq!(failure.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(failure.to_string(), "producer denied");
        assert!(!handle.request_rescan());
        assert!(handle.wake.upgrade().is_none());
    }

    #[tokio::test]
    async fn continuous_rescan_hints_preserve_filesystem_delivery() {
        let dir = canonical_tempdir();
        let path = dir.path().join("changed.rs");
        let expected = path.clone();
        let handle = Arc::new(std::sync::OnceLock::<ProjectRescanHandle>::new());
        let callback_handle = handle.clone();
        let (entered, started) = oneshot::channel();
        let (release, wait) = oneshot::channel();
        let mut first = Some((entered, wait));
        let (tx, mut batches) = mpsc::channel(4);
        let owner = ProjectSupervisor::start(
            context(dir.path(), "main"),
            config(dir.path()),
            move |_, batch, stop| {
                let first = first.take();
                let tx = tx.clone();
                let handle = callback_handle.clone();
                let changed = batch.paths.contains(&expected);
                async move {
                    if let Some((entered, wait)) = first {
                        entered.send(()).unwrap();
                        wait.await.unwrap();
                    }
                    handle.get().unwrap().request_rescan();
                    tokio::select! {
                        biased;
                        () = stop.cancelled() => Ok(()),
                        result = tx.send(changed) => result.map_err(io::Error::other),
                    }
                }
            },
        )
        .await
        .unwrap();
        assert!(handle.set(owner.rescan_handle()).is_ok());
        started.await.unwrap();
        fs::write(path, "fn changed() {}\n").unwrap();
        release.send(()).unwrap();
        timeout(Duration::from_secs(8), async {
            while !batches.recv().await.unwrap() {}
        })
        .await
        .expect("continuous hints must not starve native filesystem events");
        owner.stop().await.unwrap();
        assert!(!handle.get().unwrap().request_rescan());
    }

    #[test]
    fn supervisor_identity_distinguishes_checkouts_and_branch_versions() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        assert_ne!(
            context(a.path(), "main").key(),
            context(b.path(), "main").key()
        );
        assert_ne!(
            context(a.path(), "main").key(),
            context(a.path(), "feature").key()
        );
        assert_eq!(
            context(a.path(), "main").key(),
            context(a.path(), "main").key()
        );
    }

    #[tokio::test]
    async fn stop_cancels_but_awaits_producer_cleanup() {
        let dir = TempDir::new().unwrap();
        let (entered, waiting) = oneshot::channel();
        let (cancelled, observed) = oneshot::channel();
        let (release, cleanup) = oneshot::channel();
        let mut signals = Some((entered, cancelled, cleanup));
        let supervisor = ProjectSupervisor::start(
            context(dir.path(), "feature"),
            config(dir.path()),
            move |_, batch, stop| {
                let signals = signals.take().unwrap();
                async move {
                    assert!(batch.rescan);
                    signals.0.send(()).unwrap();
                    stop.cancelled().await;
                    signals.1.send(()).unwrap();
                    signals.2.await.unwrap();
                    Ok(())
                }
            },
        )
        .await
        .unwrap();
        timeout(Duration::from_secs(8), waiting)
            .await
            .unwrap()
            .unwrap();
        let stopped = tokio::spawn(supervisor.stop());
        timeout(Duration::from_secs(8), observed)
            .await
            .unwrap()
            .unwrap();
        assert!(
            !stopped.is_finished(),
            "stop must retain producer borrows until cleanup finishes"
        );
        release.send(()).unwrap();
        stopped.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn parked_checkout_does_not_interrupt_another_checkout_and_resumes_with_rescan() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        let b_root = fs::canonicalize(b.path()).unwrap();
        let b_context = context(&b_root, "feature");
        let b_key = b_context.key();
        let (tx, mut batches) = mpsc::channel(16);
        let mut a_owner = ProjectSupervisor::start(
            context(a.path(), "main"),
            config(a.path()),
            |_, _, _| async { Ok(()) },
        )
        .await
        .unwrap();
        assert!(!a_owner.is_finished());
        let b_owner = ProjectSupervisor::start(b_context, config(&b_root), move |ctx, batch, _| {
            let tx = tx.clone();
            async move {
                let contents = match fs::read(ctx.root().join("live.rs")) {
                    Ok(contents) => Some(contents),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error),
                };
                tx.send((ctx, batch, contents))
                    .await
                    .map_err(io::Error::other)?;
                Ok(())
            }
        })
        .await
        .unwrap();
        let (captured, first, contents) = timeout(Duration::from_secs(8), batches.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(captured.key(), b_key);
        assert!(first.rescan);
        assert!(contents.is_none());
        let parked = a_owner.park().await.unwrap();
        let file = b_root.join("live.rs");
        fs::write(&file, "fn live() {}").unwrap();
        timeout(Duration::from_secs(8), async {
            loop {
                let (captured, batch, contents) = batches.recv().await.unwrap();
                assert_eq!(captured.key(), b_key);
                assert!(batch.error.is_none(), "{:?}", batch.error);
                assert!(batch.paths.iter().all(|path| path.starts_with(&b_root)));
                // Pending setup events need not describe this write. A native
                // rescan is sufficient only after this producer observes it.
                if (batch.rescan || batch.paths.contains(&file))
                    && contents.as_deref() == Some(b"fn live() {}".as_slice())
                {
                    break;
                }
            }
        })
        .await
        .expect("the live checkout must reconcile its own file after its sibling parks");
        let (tx, resumed) = oneshot::channel();
        let mut signal = Some(tx);
        a_owner = ProjectSupervisor::start(parked, config(a.path()), move |_, batch, _| {
            let signal = signal.take();
            async move {
                if let Some(signal) = signal {
                    signal.send(batch.rescan).unwrap();
                }
                Ok(())
            }
        })
        .await
        .unwrap();
        assert!(
            timeout(Duration::from_secs(8), resumed)
                .await
                .unwrap()
                .unwrap()
        );
        a_owner.stop().await.unwrap();
        b_owner.stop().await.unwrap();
    }

    #[tokio::test]
    async fn burst_during_producer_runs_after_initial_job_without_parallel_execution() {
        let dir = TempDir::new().unwrap();
        let (entered, ready) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let (tx, mut completed) = mpsc::channel(16);
        let mut first = Some((entered, released));
        let owner = ProjectSupervisor::start(
            context(dir.path(), "main"),
            config(dir.path()),
            move |_, batch, _| {
                let first = first.take();
                let tx = tx.clone();
                async move {
                    if let Some((entered, released)) = first {
                        entered.send(()).unwrap();
                        released.await.unwrap();
                    }
                    tx.send(batch).await.map_err(io::Error::other)?;
                    Ok(())
                }
            },
        )
        .await
        .unwrap();
        timeout(Duration::from_secs(8), ready)
            .await
            .unwrap()
            .unwrap();
        for index in 0..400 {
            fs::write(
                dir.path().join(format!("source-{index}.rs")),
                "fn burst() {}",
            )
            .unwrap();
        }
        release.send(()).unwrap();
        assert!(
            timeout(Duration::from_secs(8), completed.recv())
                .await
                .unwrap()
                .unwrap()
                .rescan
        );
        let next = timeout(Duration::from_secs(8), completed.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(next.rescan || !next.paths.is_empty());
        owner.stop().await.unwrap();
    }

    #[tokio::test]
    async fn producer_error_stops_watcher_and_propagates_to_owner() {
        let dir = TempDir::new().unwrap();
        let (entered, ready) = oneshot::channel();
        let mut signal = Some(entered);
        let owner = ProjectSupervisor::start(
            context(dir.path(), "main"),
            config(dir.path()),
            move |_, _, _| {
                let entered = signal.take().unwrap();
                async move {
                    entered.send(()).unwrap();
                    Err(io::Error::other("producer failed"))
                }
            },
        )
        .await
        .unwrap();
        timeout(Duration::from_secs(8), ready)
            .await
            .unwrap()
            .unwrap();
        let error = timeout(Duration::from_secs(8), owner.stop())
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.to_string(), "producer failed");
    }

    #[tokio::test]
    async fn watcher_root_must_match_captured_checkout() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        let error = ProjectSupervisor::start(
            context(a.path(), "main"),
            config(b.path()),
            |_, _, _| async { Ok(()) },
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    async fn start_noop(ctx: ProjectContext) -> io::Result<ProjectSupervisor> {
        let cfg = config(ctx.root());
        ProjectSupervisor::start(ctx, cfg, |_, _, _| async { Ok(()) }).await
    }

    #[tokio::test]
    async fn registry_retires_stale_projects_parks_promotes_and_replaces_versions() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        let a_ctx = context(a.path(), "main");
        let b_ctx = context(b.path(), "feature");
        let mut registry = ProjectSupervisors::default();
        registry
            .reconcile(
                vec![
                    DesiredProject {
                        context: a_ctx.clone(),
                        active: true,
                    },
                    DesiredProject {
                        context: b_ctx.clone(),
                        active: true,
                    },
                ],
                start_noop,
            )
            .await
            .unwrap();
        assert_eq!(registry.supervised().count(), 2);
        registry
            .reconcile(
                vec![
                    DesiredProject {
                        context: a_ctx.clone(),
                        active: false,
                    },
                    DesiredProject {
                        context: b_ctx.clone(),
                        active: true,
                    },
                ],
                start_noop,
            )
            .await
            .unwrap();
        assert_eq!(registry.supervised().next().unwrap().key(), b_ctx.key());
        assert_eq!(registry.parked().next().unwrap().key(), a_ctx.key());
        registry
            .reconcile(
                vec![DesiredProject {
                    context: a_ctx.clone(),
                    active: true,
                }],
                start_noop,
            )
            .await
            .unwrap();
        assert_eq!(registry.supervised().count(), 1);
        assert_eq!(registry.parked().count(), 0);
        let switched = context(a.path(), "next");
        registry
            .reconcile(
                vec![DesiredProject {
                    context: switched.clone(),
                    active: true,
                }],
                start_noop,
            )
            .await
            .unwrap();
        assert_eq!(registry.supervised().next().unwrap().key(), switched.key());
        registry.shutdown().await.unwrap();
        assert_eq!(registry.supervised().count(), 0);
        assert_eq!(registry.parked().count(), 0);
    }

    #[tokio::test]
    async fn deletion_reaches_producer_with_captured_version() {
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let captured = context(&root, "captured");
        let key = captured.key();
        let file = root.join("removed.rs");
        fs::write(&file, "fn removed() {}").unwrap();
        let (tx, mut jobs) = mpsc::channel(16);
        let owner = ProjectSupervisor::start(captured, config(&root), move |ctx, batch, _| {
            let tx = tx.clone();
            async move {
                let removed = match fs::symlink_metadata(ctx.root().join("removed.rs")) {
                    Ok(_) => false,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => true,
                    Err(error) => return Err(error),
                };
                tx.send((ctx, batch, removed))
                    .await
                    .map_err(io::Error::other)?;
                Ok(())
            }
        })
        .await
        .unwrap();
        let (initial, first, removed) = timeout(Duration::from_secs(8), jobs.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(initial.key(), key);
        assert!(first.rescan);
        assert!(!removed);
        fs::remove_file(&file).unwrap();
        timeout(Duration::from_secs(8), async {
            loop {
                let (child, batch, removed) = jobs.recv().await.unwrap();
                assert_eq!(child.key(), key);
                assert!(batch.error.is_none(), "{:?}", batch.error);
                assert!(batch.paths.iter().all(|path| path.starts_with(&root)));
                if removed && (batch.rescan || batch.paths.contains(&file)) {
                    break;
                }
            }
        })
        .await
        .expect(
            "the captured producer must reconcile the removal, including after a native rescan",
        );
        owner.stop().await.unwrap();
    }

    #[tokio::test]
    async fn registry_rejects_duplicate_desired_context_before_starting_any_watcher() {
        let dir = TempDir::new().unwrap();
        let ctx = context(dir.path(), "main");
        let mut registry = ProjectSupervisors::default();
        let error = registry
            .reconcile(
                vec![
                    DesiredProject {
                        context: ctx.clone(),
                        active: true,
                    },
                    DesiredProject {
                        context: ctx,
                        active: true,
                    },
                ],
                start_noop,
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(registry.supervised().count(), 0);
    }

    #[tokio::test]
    async fn shutdown_cancels_all_projects_before_waiting_for_their_cleanup() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        let (entered, mut running) = mpsc::channel(2);
        let (cancelled, mut stopping) = mpsc::channel(2);
        let release = Arc::new(tokio::sync::Notify::new());
        let mut registry = ProjectSupervisors::default();
        for dir in [&a, &b] {
            let ready = entered.clone();
            let stopped = cancelled.clone();
            let released = release.clone();
            let mut first = true;
            let ctx = context(dir.path(), "main");
            let owner =
                ProjectSupervisor::start(ctx.clone(), config(dir.path()), move |_, _, stop| {
                    let ready = ready.clone();
                    let stopped = stopped.clone();
                    let released = released.clone();
                    let initial = std::mem::replace(&mut first, false);
                    async move {
                        if initial {
                            let finish = released.notified();
                            ready.send(()).await.unwrap();
                            stop.cancelled().await;
                            stopped.send(()).await.unwrap();
                            finish.await;
                        }
                        Ok(())
                    }
                })
                .await
                .unwrap();
            registry.active.insert(ctx.key(), owner);
        }
        for _ in 0..2 {
            timeout(Duration::from_secs(8), running.recv())
                .await
                .unwrap()
                .unwrap();
        }
        let shutdown = tokio::spawn(async move { registry.shutdown().await });
        for _ in 0..2 {
            timeout(Duration::from_secs(8), stopping.recv())
                .await
                .unwrap()
                .unwrap();
        }
        assert!(!shutdown.is_finished());
        release.notify_waiters();
        shutdown.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn failed_project_does_not_prevent_registering_another_checkout() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        let bad = context(a.path(), "main");
        let healthy = context(b.path(), "main");
        let bad_root = bad.root().to_owned();
        let mut registry = ProjectSupervisors::default();
        let error = registry
            .reconcile(
                vec![
                    DesiredProject {
                        context: bad,
                        active: true,
                    },
                    DesiredProject {
                        context: healthy.clone(),
                        active: true,
                    },
                ],
                move |ctx| {
                    let bad_root = bad_root.clone();
                    async move {
                        if ctx.root() == bad_root {
                            Err(io::Error::other("project initialization failed"))
                        } else {
                            start_noop(ctx).await
                        }
                    }
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "project initialization failed");
        assert_eq!(registry.supervised().next().unwrap().key(), healthy.key());
        registry.shutdown().await.unwrap();
    }
    #[tokio::test]
    async fn stop_selection_retains_pending_owner_and_preserves_other_version_and_checkout() {
        let root = TempDir::new().unwrap();
        let alternate = TempDir::new().unwrap();
        fs::write(root.path().join("native-observed.txt"), b"initial").unwrap();
        let a = context(root.path(), "main");
        let a_key = a.key();
        let b = context(root.path(), "feature");
        let b_key = b.key();
        let (entered, waiting) = oneshot::channel();
        let mut entered = Some(entered);
        let (cancelled, cancellation) = oneshot::channel();
        let mut cancelled = Some(cancelled);
        let (release, cleanup) = oneshot::channel();
        let mut cleanup = Some(cleanup);
        let owner = ProjectSupervisor::start(a, config(root.path()), move |ctx, _, stop| {
            let entered = entered.take().unwrap();
            let cancelled = cancelled.take().unwrap();
            let cleanup = cleanup.take().unwrap();
            async move {
                let resource = fs::File::open(ctx.root().join("native-observed.txt"))?;
                entered.send(()).unwrap();
                stop.cancelled().await;
                cancelled.send(()).unwrap();
                cleanup.await.unwrap();
                drop(resource);
                Ok(())
            }
        })
        .await
        .unwrap();
        timeout(Duration::from_secs(5), waiting)
            .await
            .unwrap()
            .unwrap();
        let mut registry = ProjectSupervisors::default();
        registry.active.insert(a_key.clone(), owner);
        let (events, mut received) = mpsc::unbounded_channel();
        let b_owner = ProjectSupervisor::start(b, config(root.path()), move |ctx, _, stop| {
            let events = events.clone();
            async move {
                assert!(!stop.is_cancelled());
                let bytes = fs::read(ctx.root().join("native-observed.txt"))?;
                events.send(bytes).unwrap();
                Ok(())
            }
        })
        .await
        .unwrap();
        registry.active.insert(b_key.clone(), b_owner);
        assert_eq!(
            timeout(Duration::from_secs(5), received.recv())
                .await
                .unwrap()
                .unwrap(),
            b"initial"
        );
        let sibling = context(alternate.path(), "main");
        let sibling_key = sibling.key();
        registry
            .active
            .insert(sibling_key.clone(), start_noop(sibling).await.unwrap());
        let parked = context(root.path(), "parked");
        let parked_key = parked.key();
        registry.parked.insert(parked_key.clone(), parked);
        {
            let pending = registry.stop_selection(&a_key);
            tokio::pin!(pending);
            tokio::select! {
                result = &mut pending => panic!("cleanup must await its owned resource: {result:?}"),
                signal = timeout(Duration::from_secs(5), cancellation) => signal.unwrap().unwrap(),
            }
        }
        assert!(registry.active.get(&a_key).unwrap().task.is_some());
        assert!(!registry.active.get(&a_key).unwrap().is_finished());
        release.send(()).unwrap();
        registry.stop_selection(&a_key).await.unwrap();
        assert!(!registry.active.contains_key(&a_key));
        assert!(registry.parked.contains_key(&parked_key));
        assert!(
            !registry
                .active
                .get(&b_key)
                .unwrap()
                .cancellation
                .is_cancelled()
        );
        assert!(
            !registry
                .active
                .get(&sibling_key)
                .unwrap()
                .cancellation
                .is_cancelled()
        );
        fs::write(root.path().join("native-observed.txt"), b"after-stop").unwrap();
        timeout(Duration::from_secs(5), async {
            while received.recv().await.unwrap() != b"after-stop" {}
        })
        .await
        .unwrap();
        registry.stop_selection(&a_key).await.unwrap();
        registry.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn stop_selection_removes_only_exact_parked_key_and_absence_is_owner_noop() {
        let root = TempDir::new().unwrap();
        let alternate = TempDir::new().unwrap();
        let a = context(root.path(), "main");
        let a_key = a.key();
        let b = context(root.path(), "feature");
        let b_key = b.key();
        let sibling = context(alternate.path(), "main");
        let sibling_key = sibling.key();
        let mut registry = ProjectSupervisors::default();
        registry.parked.insert(a_key.clone(), a);
        registry.parked.insert(b_key.clone(), b);
        registry.parked.insert(sibling_key.clone(), sibling);
        let marker = root.path().join("data-is-owned-elsewhere");
        fs::write(&marker, b"retained").unwrap();
        let mut absent = a_key.clone();
        absent.version = "absent".into();
        registry.stop_selection(&absent).await.unwrap();
        assert_eq!(registry.parked().count(), 3);
        registry.stop_selection(&a_key).await.unwrap();
        registry.stop_selection(&a_key).await.unwrap();
        assert_eq!(registry.parked().count(), 2);
        assert!(registry.parked.contains_key(&b_key));
        assert!(registry.parked.contains_key(&sibling_key));
        assert_eq!(fs::read(marker).unwrap(), b"retained");
    }

    #[tokio::test]
    async fn stop_selection_preserves_real_cleanup_failure_and_owner_until_explicit_retry() {
        let root = TempDir::new().unwrap();
        let a = context(root.path(), "main");
        let a_key = a.key();
        let b = context(root.path(), "feature");
        let b_key = b.key();
        let (entered, waiting) = oneshot::channel();
        let mut entered = Some(entered);
        let owner = ProjectSupervisor::start(a, config(root.path()), move |ctx, _, stop| {
            let entered = entered.take().unwrap();
            async move {
                entered.send(()).unwrap();
                stop.cancelled().await;
                fs::read(ctx.root().join("missing-cleanup-resource")).map(|_| ())
            }
        })
        .await
        .unwrap();
        timeout(Duration::from_secs(5), waiting)
            .await
            .unwrap()
            .unwrap();
        let mut registry = ProjectSupervisors::default();
        registry.active.insert(a_key.clone(), owner);
        registry
            .active
            .insert(b_key.clone(), start_noop(b).await.unwrap());
        let error = registry.stop_selection(&a_key).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(error.raw_os_error().is_some());
        assert!(registry.active.contains_key(&a_key));
        assert!(registry.active.get(&a_key).unwrap().task.is_none());
        assert!(
            !registry
                .active
                .get(&b_key)
                .unwrap()
                .cancellation
                .is_cancelled()
        );
        registry.stop_selection(&a_key).await.unwrap();
        assert!(!registry.active.contains_key(&a_key));
        assert!(registry.active.contains_key(&b_key));
        registry.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn stop_selection_preserves_worker_panic_and_does_not_repoll_completed_handle() {
        let root = TempDir::new().unwrap();
        let a = context(root.path(), "main");
        let a_key = a.key();
        let b = context(root.path(), "feature");
        let b_key = b.key();
        let (entered, waiting) = oneshot::channel();
        let mut entered = Some(entered);
        let owner = ProjectSupervisor::start(a, config(root.path()), move |_, _, stop| {
            let entered = entered.take().unwrap();
            async move {
                entered.send(()).unwrap();
                stop.cancelled().await;
                panic!("worker cleanup failed");
            }
        })
        .await
        .unwrap();
        timeout(Duration::from_secs(5), waiting)
            .await
            .unwrap()
            .unwrap();
        let mut registry = ProjectSupervisors::default();
        registry.active.insert(a_key.clone(), owner);
        registry
            .active
            .insert(b_key.clone(), start_noop(b).await.unwrap());
        let error = registry.stop_selection(&a_key).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        let worker = error
            .get_ref()
            .unwrap()
            .downcast_ref::<tokio::task::JoinError>()
            .unwrap();
        assert!(worker.is_panic());
        assert!(error.to_string().contains("worker cleanup failed"));
        assert!(registry.active.get(&a_key).unwrap().task.is_none());
        assert!(
            !registry
                .active
                .get(&b_key)
                .unwrap()
                .cancellation
                .is_cancelled()
        );
        registry.stop_selection(&a_key).await.unwrap();
        assert!(!registry.active.contains_key(&a_key));
        assert!(registry.active.contains_key(&b_key));
        registry.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn stop_checkout_cancels_all_versions_before_join_and_preserves_siblings() {
        let a = canonical_tempdir();
        let b = canonical_tempdir();
        let id = context(a.path(), "main").id;
        let (cancelled, mut observed) = mpsc::unbounded_channel();
        let mut releases = Vec::new();
        let mut registry = ProjectSupervisors::default();
        for version in ["main", "feature"] {
            let (entered, waiting) = oneshot::channel();
            let mut entered = Some(entered);
            let ctx = context(a.path(), version);
            let key = ctx.key();
            let (release, cleanup) = oneshot::channel();
            releases.push(release);
            let mut cleanup = Some(cleanup);
            let cancelled = cancelled.clone();
            let owner = ProjectSupervisor::start(ctx, config(a.path()), move |ctx, _, stop| {
                let entered = entered.take().unwrap();
                let cleanup = cleanup.take().unwrap();
                let cancelled = cancelled.clone();
                async move {
                    entered.send(()).unwrap();
                    stop.cancelled().await;
                    cancelled.send(ctx.version.version().to_owned()).unwrap();
                    cleanup.await.unwrap();
                    Ok(())
                }
            })
            .await
            .unwrap();
            waiting.await.unwrap();
            registry.active.insert(key, owner);
        }
        let sibling = context(b.path(), "main");
        let sibling_key = sibling.key();
        let sibling_owner =
            ProjectSupervisor::start(sibling, config(b.path()), |_, _, _| async { Ok(()) })
                .await
                .unwrap();
        registry.active.insert(sibling_key.clone(), sibling_owner);
        registry.parked.insert(
            context(a.path(), "parked").key(),
            context(a.path(), "parked"),
        );
        registry.parked.insert(
            context(b.path(), "parked").key(),
            context(b.path(), "parked"),
        );
        {
            let pending = registry.stop_checkout(id, a.path());
            tokio::pin!(pending);
            for _ in 0..2 {
                tokio::select! {
                    result = &mut pending => panic!("cleanup must await its producers: {result:?}"),
                    signal = observed.recv() => assert!(signal.is_some()),
                }
            }
            for release in releases {
                release.send(()).unwrap();
            }
            pending.await.unwrap();
        }
        assert_eq!(registry.supervised().count(), 1);
        assert_eq!(registry.parked().count(), 1);
        assert!(registry.active.contains_key(&sibling_key));
        registry.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn stop_checkout_preserves_cleanup_cause_and_joins_other_matching_owners() {
        let a = canonical_tempdir();
        let id = context(a.path(), "main").id;
        let mut registry = ProjectSupervisors::default();
        let (done, mut completed) = mpsc::unbounded_channel();
        for version in ["main", "feature"] {
            let (entered, waiting) = oneshot::channel();
            let mut entered = Some(entered);
            let ctx = context(a.path(), version);
            let key = ctx.key();
            let done = done.clone();
            let owner = ProjectSupervisor::start(ctx, config(a.path()), move |ctx, _, stop| {
                let entered = entered.take().unwrap();
                let done = done.clone();
                async move {
                    entered.send(()).unwrap();
                    stop.cancelled().await;
                    done.send(ctx.version.version().to_owned()).unwrap();
                    if ctx.version.version() == "main" {
                        Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "actual producer cleanup",
                        ))
                    } else {
                        Ok(())
                    }
                }
            })
            .await
            .unwrap();
            waiting.await.unwrap();
            registry.active.insert(key, owner);
        }
        let error = registry.stop_checkout(id, a.path()).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "actual producer cleanup");
        assert!(completed.try_recv().is_ok());
        assert!(completed.try_recv().is_ok());
        assert_eq!(registry.supervised().count(), 1);
        assert!(registry.active.values().all(|owner| owner.task.is_none()));
        registry.stop_checkout(id, a.path()).await.unwrap();
        assert_eq!(registry.supervised().count(), 0);
    }

    #[derive(Clone, Copy, Debug)]
    enum GroupStop {
        Checkout,
        Shutdown,
        ReconcileRemoved,
        ReconcileParked,
    }

    #[tokio::test]
    async fn interrupted_group_join_retains_registered_resources_and_handles() {
        for operation in [
            GroupStop::Checkout,
            GroupStop::Shutdown,
            GroupStop::ReconcileRemoved,
            GroupStop::ReconcileParked,
        ] {
            let root = canonical_tempdir();
            let sibling = canonical_tempdir();
            fs::write(root.path().join("owned.txt"), b"owned resource").unwrap();
            fs::write(sibling.path().join("observed.txt"), b"initial").unwrap();
            let mut registry = ProjectSupervisors::default();
            let (cancelled, mut cancellation) = mpsc::unbounded_channel();
            let mut releases = Vec::new();
            let mut selected = Vec::new();
            for version in ["main", "feature"] {
                let ctx = context(root.path(), version);
                selected.push(ctx.clone());
                let (entered, waiting) = oneshot::channel();
                let (release, cleanup) = oneshot::channel();
                releases.push(release);
                let mut signals = Some((entered, cleanup));
                let cancelled = cancelled.clone();
                let owner = ProjectSupervisor::start(
                    ctx.clone(),
                    config(root.path()),
                    move |ctx, _, stop| {
                        let (entered, cleanup) = signals.take().unwrap();
                        let cancelled = cancelled.clone();
                        async move {
                            let resource = fs::File::open(ctx.root().join("owned.txt"))?;
                            entered.send(()).unwrap();
                            stop.cancelled().await;
                            cancelled.send(()).unwrap();
                            cleanup.await.unwrap();
                            drop(resource);
                            Ok(())
                        }
                    },
                )
                .await
                .unwrap();
                timeout(Duration::from_secs(5), waiting)
                    .await
                    .unwrap()
                    .unwrap();
                registry.active.insert(ctx.key(), owner);
            }
            let other = context(sibling.path(), "main");
            let other_key = other.key();
            let (events, mut observed) = mpsc::unbounded_channel();
            let owner = ProjectSupervisor::start(
                other.clone(),
                config(sibling.path()),
                move |ctx, _, _| {
                    let events = events.clone();
                    async move {
                        events
                            .send(fs::read(ctx.root().join("observed.txt"))?)
                            .unwrap();
                        Ok(())
                    }
                },
            )
            .await
            .unwrap();
            registry.active.insert(other_key.clone(), owner);
            assert_eq!(
                timeout(Duration::from_secs(5), observed.recv())
                    .await
                    .unwrap()
                    .unwrap(),
                b"initial"
            );
            let mut desired = vec![DesiredProject {
                context: other,
                active: true,
            }];
            if matches!(operation, GroupStop::ReconcileParked) {
                desired.extend(selected.iter().cloned().map(|context| DesiredProject {
                    context,
                    active: false,
                }));
            }
            {
                let pending = async {
                    match operation {
                        GroupStop::Checkout => {
                            registry.stop_checkout(selected[0].id, root.path()).await
                        }
                        GroupStop::Shutdown => registry.shutdown().await,
                        GroupStop::ReconcileRemoved | GroupStop::ReconcileParked => {
                            registry.reconcile(desired.clone(), start_noop).await
                        }
                    }
                };
                tokio::pin!(pending);
                tokio::select! {
                    result = &mut pending => panic!("owned cleanup must stay pending: {operation:?} {result:?}"),
                    signals = timeout(Duration::from_secs(5), async {
                        cancellation.recv().await.unwrap();
                        cancellation.recv().await.unwrap();
                    }) => signals.unwrap(),
                }
            }
            for ctx in &selected {
                let owner = registry.active.get(&ctx.key()).unwrap();
                assert!(owner.task.is_some());
                assert!(!owner.is_finished());
                assert!(owner.cancellation.is_cancelled());
                assert!(!registry.parked.contains_key(&ctx.key()));
            }
            for release in releases {
                release.send(()).unwrap();
            }
            match operation {
                GroupStop::Checkout => registry
                    .stop_checkout(selected[0].id, root.path())
                    .await
                    .unwrap(),
                GroupStop::Shutdown => registry.shutdown().await.unwrap(),
                GroupStop::ReconcileRemoved | GroupStop::ReconcileParked => {
                    registry.reconcile(desired, start_noop).await.unwrap()
                }
            }
            for ctx in &selected {
                assert!(!registry.active.contains_key(&ctx.key()));
                assert_eq!(
                    registry.parked.contains_key(&ctx.key()),
                    matches!(operation, GroupStop::ReconcileParked)
                );
            }
            if matches!(operation, GroupStop::Shutdown) {
                assert!(registry.active.is_empty());
            } else {
                assert!(
                    !registry
                        .active
                        .get(&other_key)
                        .unwrap()
                        .cancellation
                        .is_cancelled()
                );
                fs::write(sibling.path().join("observed.txt"), b"after-cleanup").unwrap();
                timeout(Duration::from_secs(5), async {
                    while observed.recv().await.unwrap() != b"after-cleanup" {}
                })
                .await
                .unwrap();
            }
            registry.shutdown().await.unwrap();
        }
    }

    #[tokio::test]
    async fn reconcile_failure_retains_predecessor_without_parking_or_replacement() {
        for (version, active) in [("main", false), ("main", true), ("feature", true)] {
            let root = TempDir::new().unwrap();
            let ctx = context(root.path(), "main");
            let key = ctx.key();
            let owner =
                ProjectSupervisor::start(ctx.clone(), config(root.path()), |_, _, _| async {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "actual producer failure",
                    ))
                })
                .await
                .unwrap();
            timeout(Duration::from_secs(5), async {
                while !owner.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let mut registry = ProjectSupervisors::default();
            registry.active.insert(key.clone(), owner);
            let starts = Arc::new(AtomicUsize::new(0));
            let count = starts.clone();
            let error = registry
                .reconcile(
                    vec![DesiredProject {
                        context: context(root.path(), version),
                        active,
                    }],
                    move |ctx| {
                        count.fetch_add(1, Ordering::SeqCst);
                        start_noop(ctx)
                    },
                )
                .await
                .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(error.to_string(), "actual producer failure");
            assert_eq!(starts.load(Ordering::SeqCst), 0);
            assert!(registry.active.get(&key).unwrap().task.is_none());
            assert!(!registry.parked.contains_key(&key));
            registry.shutdown().await.unwrap();
            assert!(registry.active.is_empty());
        }
    }

    #[tokio::test]
    async fn reactivation_joins_cancelled_predecessor_before_starting_replacement() {
        let root = canonical_tempdir();
        fs::write(root.path().join("owned.txt"), b"owned resource").unwrap();
        let ctx = context(root.path(), "main");
        let key = ctx.key();
        let (entered, waiting) = oneshot::channel();
        let (cancelled, cancellation) = oneshot::channel();
        let (release, cleanup) = oneshot::channel();
        let mut signals = Some((entered, cancelled, cleanup));
        let owner =
            ProjectSupervisor::start(ctx.clone(), config(root.path()), move |ctx, _, stop| {
                let (entered, cancelled, cleanup) = signals.take().unwrap();
                async move {
                    let resource = fs::File::open(ctx.root().join("owned.txt"))?;
                    entered.send(()).unwrap();
                    stop.cancelled().await;
                    cancelled.send(()).unwrap();
                    cleanup.await.unwrap();
                    drop(resource);
                    Ok(())
                }
            })
            .await
            .unwrap();
        waiting.await.unwrap();
        let mut registry = ProjectSupervisors::default();
        registry.active.insert(key.clone(), owner);
        {
            let pending = registry.stop_checkout(ctx.id, root.path());
            tokio::pin!(pending);
            tokio::select! {
                result = &mut pending => panic!("cleanup must remain pending: {result:?}"),
                signal = timeout(Duration::from_secs(5), cancellation) => signal.unwrap().unwrap(),
            }
        }
        let starts = Arc::new(AtomicUsize::new(0));
        let count = starts.clone();
        {
            let replacement = registry.reconcile(
                vec![DesiredProject {
                    context: ctx,
                    active: true,
                }],
                move |ctx| {
                    count.fetch_add(1, Ordering::SeqCst);
                    start_noop(ctx)
                },
            );
            tokio::pin!(replacement);
            tokio::select! {
                biased;
                result = &mut replacement => panic!("reactivation must await the cancelled predecessor: {result:?}"),
                () = tokio::task::yield_now() => {}
            }
            assert_eq!(starts.load(Ordering::SeqCst), 0);
            release.send(()).unwrap();
            timeout(Duration::from_secs(5), replacement)
                .await
                .unwrap()
                .unwrap();
        }
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert!(
            !registry
                .active
                .get(&key)
                .unwrap()
                .cancellation
                .is_cancelled()
        );
        registry.shutdown().await.unwrap();
    }
}
