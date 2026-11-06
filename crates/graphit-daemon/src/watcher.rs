//! Native checkout observation. SDK debouncing precedes a bounded project mailbox.

use graphit_core::ignore_rules::IgnoreScope;
use notify_debouncer_full::{
    DebounceEventResult, Debouncer, NoCache, new_debouncer_opt,
    notify::{
        self, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
        event::{AccessKind, AccessMode},
    },
};
use std::{
    collections::{BTreeSet, HashMap},
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, mpsc};

const QUEUED_BATCHES: usize = 16;
const PATHS_PER_BATCH: usize = 256;
type NativeWatcher = Debouncer<CheckoutWatcher, NoCache>;
type Scopes = [IgnoreScope; 2];

// One native event source per configuration; each checkout retains its own SDK
// debouncer, directory policy and bounded delivery queue. Weak caching closes the
// native backend when its last logical watcher is dropped.
static NATIVE_BACKENDS: Mutex<Vec<(notify::Config, Weak<SharedNativeWatcher>)>> =
    Mutex::new(Vec::new());

type NativeHandler = Arc<Mutex<Box<dyn notify::EventHandler>>>;

struct NativeSubscription {
    handler: NativeHandler,
    paths: Mutex<HashMap<PathBuf, RecursiveMode>>,
}

struct SharedNativeWatcher {
    // The SDK event thread never acquires this mutex: watch/unwatch may wait for
    // that same thread. Nor do callbacks run while the subscription list is locked.
    native: Mutex<RecommendedWatcher>,
    subscriptions: Arc<Mutex<Vec<Arc<NativeSubscription>>>>,
}

impl SharedNativeWatcher {
    fn acquire(config: notify::Config) -> notify::Result<Arc<Self>> {
        let mut backends = lock(&NATIVE_BACKENDS);
        backends.retain(|(_, backend)| backend.strong_count() != 0);
        if let Some(backend) = backends.iter().find_map(|(candidate, backend)| {
            (*candidate == config).then(|| backend.upgrade()).flatten()
        }) {
            return Ok(backend);
        }
        let subscriptions = Arc::new(Mutex::new(Vec::<Arc<NativeSubscription>>::new()));
        let delivery = Arc::clone(&subscriptions);
        let native = RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| {
                // Read-only access must not feed back into indexing. Close(Write)
                // is a real write boundary: retain it for the SDK and consumers.
                if event.as_ref().is_ok_and(ignorable_native_access) {
                    return;
                }
                let subscribers = lock(&delivery).clone();
                for subscriber in subscribers {
                    let relevant = match &event {
                        Ok(event) => event.need_rescan() || subscriber.matches(&event.paths),
                        Err(error) => subscriber.matches(&error.paths),
                    };
                    if relevant {
                        let event = match &event {
                            Ok(event) => Ok(event.clone()),
                            Err(error) => Err(duplicate_native_error(error)),
                        };
                        lock(&subscriber.handler).handle_event(event);
                    }
                }
            },
            config,
        )?;
        let backend = Arc::new(Self {
            native: Mutex::new(native),
            subscriptions,
        });
        backends.push((config, Arc::downgrade(&backend)));
        Ok(backend)
    }

    // Called only with native held, serializing changes to aggregate ownership.
    fn mode(&self, path: &Path) -> Option<RecursiveMode> {
        let mut result = None;
        for subscriber in lock(&self.subscriptions).iter() {
            match lock(&subscriber.paths).get(path) {
                Some(RecursiveMode::Recursive) => return Some(RecursiveMode::Recursive),
                Some(mode) => result = Some(*mode),
                None => {}
            }
        }
        result
    }

    fn remove_native(
        &self,
        native: &mut RecommendedWatcher,
        path: &Path,
        old: RecursiveMode,
    ) -> notify::Result<()> {
        // A native deletion may already have removed the watch. The SDK still
        // needs to evict its logical root/cache in that case.
        match native.unwatch(path) {
            Err(error) if matches!(error.kind, notify::ErrorKind::WatchNotFound) => {}
            result => result?,
        }
        if let Some(mode) = self.mode(path) {
            native.watch(path, mode)?;
        }
        // Only recursive removal can also remove sibling descendants. Production
        // checkout watches are nonrecursive and never enumerate other paths here.
        if old == RecursiveMode::Recursive {
            let mut descendants = HashMap::new();
            for subscriber in lock(&self.subscriptions).iter() {
                for (remaining, mode) in lock(&subscriber.paths).iter() {
                    if remaining != path && remaining.starts_with(path) {
                        descendants
                            .entry(remaining.clone())
                            .and_modify(|current| {
                                if *mode == RecursiveMode::Recursive {
                                    *current = *mode;
                                }
                            })
                            .or_insert(*mode);
                    }
                }
            }
            for (remaining, mode) in descendants {
                native.watch(&remaining, mode)?;
            }
        }
        Ok(())
    }
}

fn ignorable_native_access(event: &notify::Event) -> bool {
    matches!(event.kind, EventKind::Access(kind) if kind != AccessKind::Close(AccessMode::Write))
        && !event.need_rescan()
}

impl NativeSubscription {
    fn matches(&self, event_paths: &[PathBuf]) -> bool {
        if event_paths.is_empty() {
            return true;
        }
        let paths = lock(&self.paths);
        event_paths.iter().any(|path| {
            paths.contains_key(path)
                || path
                    .parent()
                    .is_some_and(|parent| paths.contains_key(parent))
                || path
                    .ancestors()
                    .any(|ancestor| paths.get(ancestor) == Some(&RecursiveMode::Recursive))
        })
    }
}

fn duplicate_native_error(error: &notify::Error) -> notify::Error {
    let kind = match &error.kind {
        notify::ErrorKind::Generic(message) => notify::ErrorKind::Generic(message.clone()),
        notify::ErrorKind::Io(error) => notify::ErrorKind::Io(match error.raw_os_error() {
            Some(code) => io::Error::from_raw_os_error(code),
            None => io::Error::new(error.kind(), error.to_string()),
        }),
        notify::ErrorKind::PathNotFound => notify::ErrorKind::PathNotFound,
        notify::ErrorKind::WatchNotFound => notify::ErrorKind::WatchNotFound,
        notify::ErrorKind::InvalidConfig(config) => notify::ErrorKind::InvalidConfig(*config),
        notify::ErrorKind::MaxFilesWatch => notify::ErrorKind::MaxFilesWatch,
    };
    notify::Error::new(kind).set_paths(error.paths.clone())
}

struct CheckoutWatcher {
    backend: Arc<SharedNativeWatcher>,
    subscription: Arc<NativeSubscription>,
}

impl Watcher for CheckoutWatcher {
    fn new<F: notify::EventHandler>(handler: F, config: notify::Config) -> notify::Result<Self> {
        let backend = SharedNativeWatcher::acquire(config)?;
        let subscription = Arc::new(NativeSubscription {
            handler: Arc::new(Mutex::new(Box::new(handler))),
            paths: Mutex::new(HashMap::new()),
        });
        lock(&backend.subscriptions).push(Arc::clone(&subscription));
        Ok(Self {
            backend,
            subscription,
        })
    }

    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
        let mut native = lock(&self.backend.native);
        let old = self.backend.mode(path);
        let previous = lock(&self.subscription.paths).insert(path.to_owned(), mode);
        let aggregate = self.backend.mode(path).expect("registered native path");
        // Let the SDK preserve unchanged native registrations and reattach
        // recreated paths according to the backend's actual ownership.
        let result =
            if old == Some(RecursiveMode::Recursive) && aggregate == RecursiveMode::NonRecursive {
                self.backend
                    .remove_native(&mut native, path, RecursiveMode::Recursive)
            } else {
                native
                    .watch(path, aggregate)
                    .map_err(|error| registration_error(path, error))
            };
        if let Err(error) = result {
            let mut paths = lock(&self.subscription.paths);
            match previous {
                Some(mode) => {
                    paths.insert(path.to_owned(), mode);
                }
                None => {
                    paths.remove(path);
                }
            }
            return Err(error);
        }
        Ok(())
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        let mut native = lock(&self.backend.native);
        let before = self.backend.mode(path);
        let Some(previous) = lock(&self.subscription.paths).remove(path) else {
            return Ok(());
        };
        let old = before.expect("registered native path");
        if self.backend.mode(path) == Some(old) {
            return Ok(());
        }
        if let Err(error) = self.backend.remove_native(&mut native, path, old) {
            lock(&self.subscription.paths).insert(path.to_owned(), previous);
            return Err(error);
        }
        Ok(())
    }

    fn configure(&mut self, config: notify::Config) -> notify::Result<bool> {
        // Pinned native notify backends return false for runtime configuration.
        // Preserve that SDK result; creation-time configurations are pooled apart.
        lock(&self.backend.native).configure(config)
    }

    fn kind() -> notify::WatcherKind {
        RecommendedWatcher::kind()
    }
}

impl Drop for CheckoutWatcher {
    fn drop(&mut self) {
        let paths: Vec<_> = lock(&self.subscription.paths).keys().cloned().collect();
        for path in paths {
            if let Err(error) = self.unwatch(&path) {
                // Drop cannot return an error. Deliver the real cleanup failure
                // to remaining subscribers rather than reporting successful IO.
                let subscribers = lock(&self.backend.subscriptions).clone();
                for subscriber in subscribers {
                    if !Arc::ptr_eq(&subscriber, &self.subscription) {
                        lock(&subscriber.handler).handle_event(Err(duplicate_native_error(&error)));
                    }
                }
            }
        }
        lock(&self.backend.subscriptions)
            .retain(|subscriber| !Arc::ptr_eq(subscriber, &self.subscription));
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IgnoreConfig {
    pub filename: Option<String>,
    pub defaults: Vec<String>,
}

impl IgnoreConfig {
    fn load(&self, root: &Path) -> io::Result<IgnoreScope> {
        let defaults: Vec<_> = self.defaults.iter().map(String::as_str).collect();
        IgnoreScope::load(root, "", self.filename.as_deref(), &defaults)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatchConfig {
    pub root: PathBuf,
    pub ast: IgnoreConfig,
    pub wiki: IgnoreConfig,
    pub debounce: Duration,
    pub control_files: Vec<PathBuf>,
    pub control_directories: Vec<PathBuf>,
}

/// Notification locators only; callers must capture and authorize actual reads.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExternalControls {
    pub files: Vec<PathBuf>,
    pub directories: Vec<PathBuf>,
}

/// Paths include removals; producers resolve current contents against their captured checkout.
/// Rescan requires full reconciliation; retained paths still identify changes
/// which can be published before its tail. Overflow can discard those details.
#[derive(Debug, Default)]
pub struct ChangeBatch {
    pub paths: BTreeSet<PathBuf>,
    pub rescan: bool,
    /// Stored Memory/Task projections changed; no source-tree walk is required.
    pub records_changed: bool,
    /// Native configuration/profile/ignore changes, distinct from a rescan hint.
    pub controls_changed: bool,
    pub error: Option<io::Error>,
}

impl ChangeBatch {
    pub(crate) fn merge(&mut self, other: Self) {
        self.rescan |= other.rescan;
        self.records_changed |= other.records_changed;
        self.controls_changed |= other.controls_changed;
        if self.error.is_none() {
            self.error = other.error;
        }
        self.paths.extend(other.paths);
        if self.paths.len() > PATHS_PER_BATCH {
            self.paths.clear();
            self.rescan = true;
        }
    }

    fn failed(&mut self, error: impl Into<io::Error>) {
        self.rescan = true;
        self.paths.clear();
        if self.error.is_none() {
            self.error = Some(error.into());
        }
    }
}

#[derive(Default)]
struct Overflow {
    rescan: AtomicBool,
    error: Mutex<Option<io::Error>>,
    wake: Notify,
}

struct Delivery {
    sender: mpsc::Sender<ChangeBatch>,
    overflow: Arc<Overflow>,
}

impl Delivery {
    fn send(&self, batch: ChangeBatch) {
        if let Err(mpsc::error::TrySendError::Full(batch)) = self.sender.try_send(batch) {
            if let Some(error) = batch.error {
                lock(&self.overflow.error).get_or_insert(error);
            }
            self.overflow.rescan.store(true, Ordering::Release);
            self.overflow.wake.notify_one();
        }
    }
}

struct NativeState {
    config: Option<WatchConfig>,
    watcher: Option<NativeWatcher>,
    directories: HashMap<PathBuf, Scopes>,
    external: ExternalControls,
    external_watches: BTreeSet<PathBuf>,
}

impl NativeState {
    fn project_config(&self) -> io::Result<&WatchConfig> {
        self.config
            .as_ref()
            .ok_or_else(|| io::ErrorKind::InvalidInput.into())
    }

    fn external_control(&self, path: &Path) -> bool {
        self.external
            .files
            .iter()
            .any(|target| target == path || target.starts_with(path))
            || self
                .external
                .directories
                .iter()
                .any(|target| path.starts_with(target) || target.starts_with(path))
    }

    fn refresh_external(&mut self) -> io::Result<()> {
        let mut desired = BTreeSet::new();
        for target in self
            .external
            .files
            .iter()
            .filter_map(|p| p.parent())
            .chain(self.external.directories.iter().map(PathBuf::as_path))
        {
            for directory in target.ancestors() {
                match fs::symlink_metadata(directory) {
                    Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
                        desired.insert(directory.to_owned());
                        if let Some(parent) = directory.parent() {
                            desired.insert(parent.to_owned());
                        }
                        break;
                    }
                    Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
        }
        for directory in self
            .external_watches
            .difference(&desired)
            .cloned()
            .collect::<Vec<_>>()
        {
            if !self.directories.contains_key(&directory) {
                self.watcher()?.unwatch(&directory).map_err(native_error)?;
            }
        }
        for directory in desired
            .difference(&self.external_watches)
            .cloned()
            .collect::<Vec<_>>()
        {
            if !self.directories.contains_key(&directory) {
                self.watcher()?
                    .watch(&directory, RecursiveMode::NonRecursive)
                    .map_err(native_error)?;
            }
        }
        self.external_watches = desired;
        Ok(())
    }
    fn watcher(&mut self) -> io::Result<&mut NativeWatcher> {
        self.watcher
            .as_mut()
            .ok_or_else(|| io::Error::other("watcher has stopped"))
    }

    fn child_scopes(&self, scopes: &Scopes, name: &std::ffi::OsStr) -> io::Result<Scopes> {
        let ast = scopes[0].child(name)?;
        let config = self.project_config()?;
        let wiki = if config.ast == config.wiki {
            ast.clone()
        } else {
            scopes[1].child(name)?
        };
        Ok([ast, wiki])
    }

    fn add_tree(&mut self, path: PathBuf, scopes: Scopes) -> io::Result<ChangeBatch> {
        let root = self.project_config()?.root.clone();
        let mut batch = ChangeBatch::default();
        let mut pending = vec![(path, scopes)];
        while let Some((directory, scopes)) = pending.pop() {
            if self.directories.contains_key(&directory) {
                continue;
            }
            let Some(entries) =
                readable_subtree(&root, &directory, fs::read_dir(&directory), &mut batch)?
            else {
                continue;
            };
            let watched = self
                .watcher()?
                .watch(&directory, RecursiveMode::NonRecursive)
                .map_err(native_error);
            if readable_subtree(&root, &directory, watched, &mut batch)?.is_none() {
                continue;
            }
            self.directories.insert(directory.clone(), scopes.clone());
            for entry in entries {
                let Some(entry) = readable_subtree(&root, &directory, entry, &mut batch)? else {
                    continue;
                };
                let path = entry.path();
                let Some(kind) = readable_subtree(&root, &path, entry.file_type(), &mut batch)?
                else {
                    continue;
                };
                if !kind.is_dir() {
                    continue;
                }
                let relative = path.strip_prefix(&root).map_err(io::Error::other)?;
                if self.control_directory(&path)
                    || scopes[0].should_descend(relative)?
                    || scopes[1].should_descend(relative)?
                {
                    let name = entry.file_name();
                    if let Some(child) = readable_subtree(
                        &root,
                        &path,
                        self.child_scopes(&scopes, &name),
                        &mut batch,
                    )? {
                        pending.push((path, child));
                    }
                }
            }
        }
        Ok(batch)
    }

    fn remove_tree(&mut self, path: &Path) -> io::Result<()> {
        let removed: Vec<_> = self
            .directories
            .keys()
            .filter(|p| p.starts_with(path))
            .cloned()
            .collect();
        let mut failure = None;
        for directory in removed {
            self.directories.remove(&directory);
            if self.external_watches.contains(&directory) {
                continue;
            }
            // Backends may already have removed watches after directory deletion.
            if let Some(watcher) = self.watcher.as_mut()
                && let Err(error) = watcher.unwatch(&directory)
            {
                failure.get_or_insert(io::Error::other(error));
            }
        }
        failure.map_or(Ok(()), Err)
    }

    fn rebuild(&mut self) -> io::Result<ChangeBatch> {
        let config = self.project_config()?.clone();
        let root = config.root.clone();
        self.remove_tree(&root)?;
        let ast = config.ast.load(&root)?;
        let wiki = if config.ast == config.wiki {
            ast.clone()
        } else {
            config.wiki.load(&root)?
        };
        let scopes = [ast, wiki];
        self.add_tree(root, scopes)
    }

    fn control_file(&self, path: &Path) -> bool {
        let Some(config) = &self.config else {
            return false;
        };
        let Ok(relative) = path.strip_prefix(&config.root) else {
            return false;
        };
        config.control_files.iter().any(|p| p == relative)
            || config
                .control_directories
                .iter()
                .any(|p| relative.starts_with(p))
    }
    fn control_directory(&self, path: &Path) -> bool {
        let Some(config) = &self.config else {
            return false;
        };
        let Ok(relative) = path.strip_prefix(&config.root) else {
            return false;
        };
        config.control_files.iter().any(|p| p.starts_with(relative))
            || config
                .control_directories
                .iter()
                .any(|p| p.starts_with(relative) || relative.starts_with(p))
    }
    fn ignore_file(&self, path: &Path) -> bool {
        let Some(config) = &self.config else {
            return false;
        };
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        name == ".gitignore"
            || config.ast.filename.as_deref() == Some(name)
            || config.wiki.filename.as_deref() == Some(name)
    }

    fn process(&mut self, result: DebounceEventResult) -> ChangeBatch {
        let mut batch = ChangeBatch::default();
        let events = match result {
            Ok(events) => events,
            Err(errors) => {
                if let Some(error) = errors.into_iter().next() {
                    batch.failed(native_error(error));
                }
                return batch;
            }
        };
        if events
            .iter()
            .filter(|event| !ignorable_native_access(&event.event))
            .flat_map(|event| &event.paths)
            .any(|path| self.external_control(path))
        {
            batch.rescan = true;
            batch.controls_changed = true;
            if let Err(error) = self.refresh_external() {
                batch.failed(error);
            }
        }
        let Some(config) = &self.config else {
            return batch;
        };
        let root = config.root.clone();
        let reload = events
            .iter()
            .filter(|event| !ignorable_native_access(&event.event))
            .flat_map(|event| &event.paths)
            .any(|path| {
                path.starts_with(&root) && (self.ignore_file(path) || self.control_file(path))
            });
        if reload {
            batch.rescan = true;
            batch.controls_changed = true;
            match self.rebuild() {
                Ok(rebuilt) => batch.merge(rebuilt),
                Err(error) => batch.failed(error),
            }
        }
        for event in events {
            if ignorable_native_access(&event.event) {
                continue;
            }
            batch.rescan |= event.need_rescan();
            for path in event.event.paths {
                if !path.starts_with(&root) {
                    continue;
                }
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) => Some(metadata),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                    Err(error) => {
                        batch.failed(error);
                        continue;
                    }
                };
                if metadata.is_none() && self.directories.contains_key(&path) {
                    if let Err(error) = self.remove_tree(&path) {
                        batch.failed(error);
                    }
                    batch.rescan = true;
                }
                if let Some(scopes) = path
                    .parent()
                    .and_then(|parent| self.directories.get(parent))
                    .cloned()
                {
                    let is_dir = metadata.as_ref().is_some_and(|metadata| metadata.is_dir());
                    let relative = path
                        .strip_prefix(&root)
                        .expect("path is inside captured root");
                    let visible =
                        if self.control_file(&path) || (is_dir && self.control_directory(&path)) {
                            Ok(true)
                        } else if is_dir {
                            scopes[0]
                                .should_descend(relative)
                                .and_then(|a| scopes[1].should_descend(relative).map(|b| a || b))
                        } else {
                            scopes[0].is_ignored(relative, false).and_then(|a| {
                                scopes[1].is_ignored(relative, false).map(|b| !a || !b)
                            })
                        };
                    match visible {
                        Ok(false) => continue,
                        Err(error) => {
                            batch.failed(error);
                            continue;
                        }
                        Ok(true) => {}
                    }
                    if is_dir && !self.directories.contains_key(&path) {
                        batch.rescan = true;
                        let name = path.file_name().expect("a child directory has a name");
                        let child = readable_subtree(
                            &root,
                            &path,
                            self.child_scopes(&scopes, name),
                            &mut batch,
                        );
                        match child {
                            Ok(Some(child)) => match self.add_tree(path.clone(), child) {
                                Ok(added) => batch.merge(added),
                                Err(error) => batch.failed(error),
                            },
                            Ok(None) => {}
                            Err(error) => batch.failed(error),
                        }
                    }
                    batch.paths.insert(path);
                    if batch.paths.len() > PATHS_PER_BATCH {
                        batch.rescan = true;
                        batch.paths.clear();
                    }
                }
            }
        }
        batch
    }
}

/// Start/stop perform blocking filesystem and SDK work; use spawn_blocking in async callers.
pub struct ProjectWatcher {
    state: Arc<Mutex<NativeState>>,
    receiver: mpsc::Receiver<ChangeBatch>,
    overflow: Arc<Overflow>,
}

/// A live watcher configuration hint, with no read or publication authority.
#[derive(Clone)]
pub struct ProjectWatchControls(std::sync::Weak<Mutex<NativeState>>);

impl ProjectWatchControls {
    pub fn reconfigure(&self, mut config: WatchConfig) -> io::Result<()> {
        validate_controls(&config)?;
        config.root = fs::canonicalize(&config.root)?;
        let state = self.0.upgrade().ok_or(io::ErrorKind::NotConnected)?;
        let mut state = lock(&state);
        if state.watcher.is_none() {
            return Err(io::ErrorKind::NotConnected.into());
        }
        if config.root != state.project_config()?.root {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if &config == state.project_config()? {
            return Ok(());
        }
        state.config = Some(config);
        state.rebuild()?.error.map_or(Ok(()), Err)
    }
}

impl ProjectWatcher {
    #[cfg(test)]
    pub(crate) fn channel_fixture() -> (Self, mpsc::Sender<ChangeBatch>) {
        let (sender, receiver) = mpsc::channel(QUEUED_BATCHES);
        (
            Self {
                state: Arc::new(Mutex::new(NativeState {
                    config: None,
                    watcher: None,
                    directories: HashMap::new(),
                    external: ExternalControls::default(),
                    external_watches: BTreeSet::new(),
                })),
                receiver,
                overflow: Arc::new(Overflow::default()),
            },
            sender,
        )
    }

    pub fn start(mut config: WatchConfig) -> io::Result<Self> {
        validate_controls(&config)?;
        config.root = fs::canonicalize(&config.root)?;
        let debounce = config.debounce;
        Self::start_native(Some(config), debounce)
    }

    /// Observe actual external controls without a checkout or a source-tree scan.
    pub fn start_external(external: ExternalControls, debounce: Duration) -> io::Result<Self> {
        let mut watcher = Self::start_native(None, debounce)?;
        if let Err(error) = watcher.watch_external(external) {
            watcher.stop();
            return Err(error);
        }
        Ok(watcher)
    }

    fn start_native(config: Option<WatchConfig>, debounce: Duration) -> io::Result<Self> {
        if debounce.is_zero() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let (sender, receiver) = mpsc::channel(QUEUED_BATCHES);
        let overflow = Arc::new(Overflow::default());
        let delivery = Delivery {
            sender,
            overflow: overflow.clone(),
        };
        let state = Arc::new(Mutex::new(NativeState {
            config,
            watcher: None,
            directories: HashMap::new(),
            external: ExternalControls::default(),
            external_watches: BTreeSet::new(),
        }));
        let weak = Arc::downgrade(&state);
        let watcher = new_debouncer_opt::<_, CheckoutWatcher, NoCache>(
            debounce,
            None,
            move |result| {
                if let Some(state) = weak.upgrade() {
                    let mut state = lock(&state);
                    if state.watcher.is_some() {
                        let batch = state.process(result);
                        if batch.rescan || batch.error.is_some() || !batch.paths.is_empty() {
                            delivery.send(batch);
                        }
                    }
                }
            },
            NoCache::new(),
            notify::Config::default(),
        )
        .map_err(io::Error::other)?;
        let mut owner = Self {
            state,
            receiver,
            overflow,
        };
        {
            let mut state = lock(&owner.state);
            state.watcher = Some(watcher);
            if state.config.is_some() {
                match state.rebuild() {
                    Ok(batch) => {
                        if let Some(error) = batch.error {
                            lock(&owner.overflow.error).get_or_insert(error);
                        }
                        owner.overflow.rescan.store(batch.rescan, Ordering::Release);
                    }
                    Err(error) => {
                        drop(state);
                        owner.stop();
                        return Err(error);
                    }
                }
            }
        }
        Ok(owner)
    }

    pub fn controls(&self) -> ProjectWatchControls {
        ProjectWatchControls(Arc::downgrade(&self.state))
    }

    pub fn watch_external(&mut self, external: ExternalControls) -> io::Result<()> {
        for path in external.files.iter().chain(&external.directories) {
            if !path.is_absolute()
                || path.components().any(|p| {
                    matches!(
                        p,
                        std::path::Component::ParentDir | std::path::Component::CurDir
                    )
                })
                || path.as_os_str().as_encoded_bytes().contains(&0)
                || path.components().count() > 256
                || path.as_os_str().as_encoded_bytes().len()
                    > graphit_core::native_path::MAX_PATH_BYTES
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        let mut state = lock(&self.state);
        state.external = external;
        state.refresh_external()
    }

    pub async fn next(&mut self) -> Option<ChangeBatch> {
        loop {
            let notified = self.overflow.wake.notified();
            let mut batch = ChangeBatch::default();
            let mut received = false;
            while let Ok(next) = self.receiver.try_recv() {
                received = true;
                batch.merge(next);
            }
            if self.overflow.rescan.swap(false, Ordering::AcqRel) {
                received = true;
                batch.rescan = true;
                // Lost native events may include a profile/configuration update.
                batch.controls_changed = true;
                batch.paths.clear();
                batch.error = batch.error.or_else(|| lock(&self.overflow.error).take());
            }
            if received {
                return Some(batch);
            }
            tokio::select! {
                next = self.receiver.recv() => return next,
                () = notified => {}
            }
        }
    }

    pub fn stop(&mut self) {
        let watcher = {
            let mut state = lock(&self.state);
            state.directories.clear();
            state.external_watches.clear();
            state.watcher.take()
        };
        // SDK stop joins its callback thread; never hold our callback mutex while joining.
        if let Some(watcher) = watcher {
            watcher.stop();
        }
        self.receiver.close();
    }
}

fn validate_controls(config: &WatchConfig) -> io::Result<()> {
    if config.debounce.is_zero() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    for path in config
        .control_files
        .iter()
        .chain(&config.control_directories)
    {
        if path.as_os_str().is_empty()
            || !path
                .components()
                .all(|p| matches!(p, std::path::Component::Normal(_)))
            || path.as_os_str().as_encoded_bytes().contains(&0)
            || path.components().count() > 256
            || path.as_os_str().as_encoded_bytes().len() > graphit_core::native_path::MAX_PATH_BYTES
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    Ok(())
}

impl Drop for ProjectWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

fn registration_error(path: &Path, error: notify::Error) -> notify::Error {
    // This is the pinned Windows backend's metadata-registration error.
    // Confirm disappearance and leave every other Generic/typed cause intact.
    if matches!(&error.kind, notify::ErrorKind::Generic(message)
        if message == "Input watch path is neither a file nor a directory.")
        && matches!(fs::metadata(path), Err(ref cause) if cause.kind() == io::ErrorKind::NotFound)
    {
        notify::Error {
            paths: error.paths.clone(),
            kind: notify::ErrorKind::Io(io::Error::new(io::ErrorKind::NotFound, error)),
        }
    } else {
        error
    }
}

fn native_error(error: notify::Error) -> io::Error {
    let kind = match &error.kind {
        notify::ErrorKind::Io(error) => error.kind(),
        notify::ErrorKind::PathNotFound => io::ErrorKind::NotFound,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, error)
}

fn readable_subtree<T>(
    root: &Path,
    path: &Path,
    result: io::Result<T>,
    batch: &mut ChangeBatch,
) -> io::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if path != root && error.kind() == io::ErrorKind::PermissionDenied => {
            batch.failed(error);
            Ok(None)
        }
        Err(error) if path != root && error.kind() == io::ErrorKind::NotFound => {
            match fs::symlink_metadata(path) {
                Err(missing) if missing.kind() == io::ErrorKind::NotFound => {
                    let metadata = fs::symlink_metadata(root)?;
                    if !metadata.is_dir() || metadata.file_type().is_symlink() {
                        return Err(error);
                    }
                    fs::read_dir(root)?;
                    batch.rescan = true;
                    Ok(None)
                }
                Ok(_) => Err(error),
                Err(cause) => Err(cause),
            }
        }
        Err(error) => Err(error),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::time::timeout;

    // ProjectWatcher and captured contexts expose canonical checkout paths.
    // Keep low-level SDK representation tests on their original raw paths.
    fn canonical_tempdir() -> TempDir {
        TempDir::new_in(fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap()
    }

    #[test]
    fn missing_registration_classification_preserves_native_cause_and_other_failures() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("removed-before-registration");
        let native = notify::Error::generic("Input watch path is neither a file nor a directory.")
            .add_path(missing.clone());
        let classified = registration_error(&missing, native);
        assert_eq!(classified.paths.as_slice(), std::slice::from_ref(&missing));
        let notify::ErrorKind::Io(error) = classified.kind else {
            panic!("missing registration must retain its IO category");
        };
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        let original = error
            .get_ref()
            .unwrap()
            .downcast_ref::<notify::Error>()
            .unwrap();
        assert!(
            matches!(&original.kind, notify::ErrorKind::Generic(message) if message == "Input watch path is neither a file nor a directory.")
        );
        assert_eq!(original.paths.as_slice(), std::slice::from_ref(&missing));

        for path in [dir.path(), missing.as_path()] {
            assert!(matches!(
                registration_error(path, notify::Error::generic("unrelated native failure")).kind,
                notify::ErrorKind::Generic(message) if message == "unrelated native failure"
            ));
        }
        assert!(matches!(
            registration_error(
                dir.path(),
                notify::Error::generic("Input watch path is neither a file nor a directory.")
            )
            .kind,
            notify::ErrorKind::Generic(_)
        ));
        assert!(matches!(
            registration_error(
                &missing,
                notify::Error::new(notify::ErrorKind::MaxFilesWatch)
            )
            .kind,
            notify::ErrorKind::MaxFilesWatch
        ));
        for kind in [io::ErrorKind::StorageFull, io::ErrorKind::PermissionDenied] {
            let native = notify::Error::io(io::Error::from(kind));
            assert!(matches!(registration_error(&missing, native).kind,
                notify::ErrorKind::Io(error) if error.kind() == kind));
        }
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

    async fn changed(watcher: &mut ProjectWatcher, path: &Path) -> ChangeBatch {
        timeout(Duration::from_secs(8), async {
            loop {
                let batch = watcher.next().await.expect("watcher open");
                assert!(batch.error.is_none(), "{:?}", batch.error);
                if batch.rescan || batch.paths.contains(path) {
                    return batch;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out observing native change at {path:?}"))
    }

    #[test]
    fn native_backend_is_shared_by_forty_owners_and_released_after_last_drop() {
        let dir = TempDir::new().unwrap();
        let roots: Vec<_> = (0..40)
            .map(|index| {
                let root = dir.path().join(format!("checkout-{index}"));
                fs::create_dir(&root).unwrap();
                root
            })
            .collect();
        let cfg = notify::Config::default().with_poll_interval(Duration::from_secs(123_457));
        let mut watchers = Vec::new();
        let mut receivers = Vec::new();
        for root in &roots {
            let (send, receive) = std::sync::mpsc::channel();
            let mut watcher = CheckoutWatcher::new(
                move |event| {
                    let _ = send.send(event);
                },
                cfg,
            )
            .unwrap();
            watcher.watch(root, RecursiveMode::NonRecursive).unwrap();
            watchers.push(watcher);
            receivers.push(receive);
        }
        let backend = Arc::downgrade(&watchers[0].backend);
        for watcher in &watchers {
            assert!(Arc::ptr_eq(&watchers[0].backend, &watcher.backend));
        }
        let shared = dir.path().join("checkout-0");
        watchers[1]
            .watch(&shared, RecursiveMode::NonRecursive)
            .unwrap();
        watchers[0].unwatch(&shared).unwrap();
        drop(watchers.remove(0));
        let file = shared.join("after-first-owner-stopped.rs");
        fs::write(&file, "fn still_observed() {}").unwrap();
        loop {
            let event = receivers[1]
                .recv_timeout(Duration::from_secs(8))
                .unwrap()
                .unwrap();
            if event.paths.contains(&file) {
                break;
            }
        }
        // Each probe acknowledges delivery after the shared FILE event. FSEvents
        // may also report startup/root events and global rescans; only ordinary
        // events outside this subscription would violate checkout isolation.
        let probe_deadline = std::time::Instant::now() + Duration::from_secs(8);
        for root in &roots[2..] {
            fs::write(root.join("own-probe.rs"), "fn own_checkout() {}").unwrap();
        }
        for (receiver, root) in receivers[2..].iter().zip(&roots[2..]) {
            loop {
                let event = receiver
                    .recv_timeout(
                        probe_deadline
                            .checked_duration_since(std::time::Instant::now())
                            .expect("checkout probes exceeded their delivery deadline"),
                    )
                    .unwrap()
                    .unwrap();
                assert!(
                    event.need_rescan()
                        || event
                            .paths
                            .iter()
                            .all(|path| path == root || path.parent() == Some(root.as_path())),
                    "unrelated checkout received an ordinary event: {event:?}"
                );
                if event.paths.contains(&root.join("own-probe.rs")) {
                    break;
                }
            }
        }
        // Queued pre-drop events are valid, but no callback retains this owner.
        let drop_deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            match receivers[0].recv_timeout(
                drop_deadline
                    .checked_duration_since(std::time::Instant::now())
                    .expect("dropped subscription exceeded its disconnect deadline"),
            ) {
                Ok(_) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(error) => panic!("dropped subscription remains alive: {error}"),
            }
        }
        drop(watchers);
        assert!(backend.upgrade().is_none());
        let (send, receive) = std::sync::mpsc::channel();
        let mut restarted = CheckoutWatcher::new(
            move |event| {
                let _ = send.send(event);
            },
            cfg,
        )
        .unwrap();
        restarted
            .watch(&shared, RecursiveMode::NonRecursive)
            .unwrap();
        let file = shared.join("after-restart.rs");
        fs::write(&file, "fn restarted() {}").unwrap();
        loop {
            let event = receive
                .recv_timeout(Duration::from_secs(8))
                .unwrap()
                .unwrap();
            if event.paths.contains(&file) {
                break;
            }
        }
    }

    #[tokio::test]
    async fn shared_checkout_reconfigure_and_stop_preserve_sibling_scopes_and_events() {
        let dir = canonical_tempdir();
        let private = dir.path().join("private");
        fs::create_dir(&private).unwrap();
        let mut first = ProjectWatcher::start(config(dir.path())).unwrap();
        let mut sibling = ProjectWatcher::start(config(dir.path())).unwrap();
        let mut cfg = config(dir.path());
        cfg.ast.defaults = vec!["private/".into()];
        cfg.wiki.defaults = cfg.ast.defaults.clone();
        first.controls().reconfigure(cfg).unwrap();
        assert!(!lock(&first.state).directories.contains_key(&private));
        assert!(lock(&sibling.state).directories.contains_key(&private));
        let file = private.join("after-reconfigure.rs");
        fs::write(&file, "fn sibling() {}").unwrap();
        assert!(changed(&mut sibling, &file).await.paths.contains(&file));
        assert!(
            timeout(Duration::from_millis(120), first.next())
                .await
                .is_err()
        );
        first.stop();
        let file = private.join("after-stop.rs");
        fs::write(&file, "fn sibling_still_alive() {}").unwrap();
        assert!(changed(&mut sibling, &file).await.paths.contains(&file));
    }

    #[test]
    fn recursive_owner_removal_reinstalls_sibling_descendant_watch() {
        let dir = TempDir::new().unwrap();
        let child = dir.path().join("nested");
        fs::create_dir(&child).unwrap();
        let cfg = notify::Config::default().with_poll_interval(Duration::from_secs(123_458));
        let mut parent = CheckoutWatcher::new(|_| {}, cfg).unwrap();
        parent.watch(dir.path(), RecursiveMode::Recursive).unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let mut sibling = CheckoutWatcher::new(
            move |event| {
                let _ = send.send(event);
            },
            cfg,
        )
        .unwrap();
        sibling.watch(&child, RecursiveMode::NonRecursive).unwrap();
        parent.unwatch(dir.path()).unwrap();
        let file = child.join("surviving.rs");
        fs::write(&file, "fn survives() {}").unwrap();
        loop {
            let event = receive
                .recv_timeout(Duration::from_secs(8))
                .unwrap()
                .unwrap();
            if event.paths.contains(&file) {
                break;
            }
        }
    }

    #[test]
    fn shared_native_error_fanout_preserves_category_paths_and_os_code() {
        let error =
            notify::Error::io(io::Error::from_raw_os_error(24)).add_path(PathBuf::from("observed"));
        let copied = duplicate_native_error(&error);
        assert_eq!(copied.paths, error.paths);
        match copied.kind {
            notify::ErrorKind::Io(error) => assert_eq!(error.raw_os_error(), Some(24)),
            other => panic!("IO category changed: {other:?}"),
        }
        assert!(matches!(
            duplicate_native_error(&notify::Error::new(notify::ErrorKind::MaxFilesWatch)).kind,
            notify::ErrorKind::MaxFilesWatch
        ));
    }

    #[test]
    fn native_access_filter_keeps_rescan_and_mutation_events() {
        use notify::{
            Event,
            event::{AccessKind, AccessMode, DataChange, Flag, MetadataKind, ModifyKind},
        };
        for kind in [
            AccessKind::Read,
            AccessKind::Open(AccessMode::Read),
            AccessKind::Open(AccessMode::Write),
            AccessKind::Close(AccessMode::Read),
        ] {
            let event = Event::new(EventKind::Access(kind));
            assert!(ignorable_native_access(&event));
            assert!(!ignorable_native_access(&event.set_flag(Flag::Rescan)));
        }
        for kind in [
            EventKind::Access(AccessKind::Close(AccessMode::Write)),
            EventKind::Create(notify::event::CreateKind::File),
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime)),
            EventKind::Remove(notify::event::RemoveKind::File),
            EventKind::Other,
        ] {
            assert!(!ignorable_native_access(&Event::new(kind)));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn native_read_storm_skips_subscribers_and_allows_watch_registration_and_writes() {
        use std::io::Read;

        let dir = TempDir::new().unwrap();
        let child = dir.path().join("child");
        fs::create_dir(&child).unwrap();
        let source = dir.path().join("source.rs");
        fs::write(&source, b"fn observed() {}\n").unwrap();
        let cfg = notify::Config::default().with_poll_interval(Duration::from_secs(123_460));
        let (send, receive) = std::sync::mpsc::channel();
        let access_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = Arc::clone(&access_calls);
        let mut first = CheckoutWatcher::new(
            move |event: notify::Result<notify::Event>| {
                if event.as_ref().is_ok_and(ignorable_native_access) {
                    observed.fetch_add(1, Ordering::Relaxed);
                }
                let _ = send.send(event);
            },
            cfg,
        )
        .unwrap();
        first
            .watch(dir.path(), RecursiveMode::NonRecursive)
            .unwrap();
        let ready = Arc::new(std::sync::Barrier::new(2));
        let producer_ready = Arc::clone(&ready);
        let parent = dir.path().to_owned();
        let reader = std::thread::spawn(move || {
            for index in 0..10_000 {
                if index == 100 {
                    producer_ready.wait();
                }
                drop(fs::File::open(&parent).unwrap());
                let mut bytes = Vec::new();
                fs::File::open(&source)
                    .unwrap()
                    .read_to_end(&mut bytes)
                    .unwrap();
            }
        });
        ready.wait();
        let (registered, registration) = std::sync::mpsc::channel();
        let child_watch = child.clone();
        let register = std::thread::spawn(move || {
            let (send, receive) = std::sync::mpsc::channel();
            let mut second = CheckoutWatcher::new(
                move |event| {
                    let _ = send.send(event);
                },
                cfg,
            )
            .unwrap();
            second
                .watch(&child_watch, RecursiveMode::NonRecursive)
                .unwrap();
            let _ = registered.send((second, receive));
        });
        let second = registration.recv_timeout(Duration::from_secs(8));
        reader.join().unwrap();
        let (_second, child_events) = second.expect("watch registration completed during reads");
        register.join().unwrap();
        let changed = child.join("changed.rs");
        fs::write(&changed, b"fn actual_change() {}\n").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            let event = child_events
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap()
                .unwrap();
            if event.paths.contains(&changed) && matches!(event.kind, EventKind::Modify(_)) {
                break;
            }
        }
        let marker = dir.path().join("after-reads.rs");
        fs::write(&marker, b"fn after_reads() {}\n").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            let event = receive
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap()
                .unwrap();
            if event.paths.contains(&marker) && matches!(event.kind, EventKind::Modify(_)) {
                break;
            }
        }
        assert_eq!(access_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn native_new_directory_removal_and_context_cleanup() {
        let dir = canonical_tempdir();
        let mut watcher = ProjectWatcher::start(config(dir.path())).unwrap();
        let child = dir.path().join("nested");
        fs::create_dir(&child).unwrap();
        changed(&mut watcher, &child).await;
        let file = child.join("source.rs");
        fs::write(&file, "fn first() {}").unwrap();
        changed(&mut watcher, &file).await;
        fs::remove_dir_all(&child).unwrap();
        assert!(changed(&mut watcher, &child).await.rescan);
        assert!(!lock(&watcher.state).directories.contains_key(&child));
        watcher.stop();
        assert!(lock(&watcher.state).directories.is_empty());
        assert!(lock(&watcher.state).watcher.is_none());
    }

    #[tokio::test]
    async fn native_rename_moves_directory_scope_and_watches_new_children() {
        let dir = canonical_tempdir();
        let old = dir.path().join("old");
        let new = dir.path().join("new");
        fs::create_dir(&old).unwrap();
        let mut watcher = ProjectWatcher::start(config(dir.path())).unwrap();
        fs::rename(&old, &new).unwrap();
        // A removal or an unrelated SDK rescan can arrive before the destination.
        // Consume native batches until the renamed directory actually owns its scope.
        timeout(Duration::from_secs(8), async {
            loop {
                let batch = watcher.next().await.expect("watcher open");
                assert!(batch.error.is_none(), "{:?}", batch.error);
                let state = lock(&watcher.state);
                if !state.directories.contains_key(&old) && state.directories.contains_key(&new) {
                    break;
                }
            }
        })
        .await
        .expect("native rename did not replace the old directory scope with the destination");
        assert!(!lock(&watcher.state).directories.contains_key(&old));
        assert!(lock(&watcher.state).directories.contains_key(&new));
        let file = new.join("source.rs");
        fs::write(&file, "fn moved() {}").unwrap();
        changed(&mut watcher, &file).await;
    }

    #[tokio::test]
    async fn union_scopes_keep_wiki_only_directories_and_skip_shared_exclusions() {
        let dir = canonical_tempdir();
        fs::create_dir(dir.path().join("wiki")).unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();
        let mut cfg = config(dir.path());
        cfg.ast.defaults = vec!["wiki/".into(), "target/".into()];
        cfg.wiki.defaults = vec!["target/".into()];
        let mut watcher = ProjectWatcher::start(cfg).unwrap();
        assert!(
            lock(&watcher.state)
                .directories
                .contains_key(&dir.path().join("wiki"))
        );
        assert!(
            !lock(&watcher.state)
                .directories
                .contains_key(&dir.path().join("target"))
        );
        let file = dir.path().join("wiki/page.md");
        fs::write(&file, "# Page").unwrap();
        changed(&mut watcher, &file).await;
    }

    #[tokio::test]
    async fn edited_subdirectory_ignore_reloads_both_scopes() {
        let dir = canonical_tempdir();
        let child = dir.path().join("sub");
        fs::create_dir(&child).unwrap();
        fs::write(child.join(".gitignore"), "*.rs\n").unwrap();
        let mut watcher = ProjectWatcher::start(config(dir.path())).unwrap();
        fs::write(child.join(".gitignore"), "").unwrap();
        assert!(
            changed(&mut watcher, &child.join(".gitignore"))
                .await
                .rescan
        );
        let file = child.join("accepted.rs");
        fs::write(&file, "fn accepted() {}").unwrap();
        changed(&mut watcher, &file).await;
    }

    #[tokio::test]
    async fn native_file_deletion_still_reaches_producer() {
        let dir = canonical_tempdir();
        let file = dir.path().join("source.rs");
        fs::write(&file, "fn removed() {}").unwrap();
        let mut watcher = ProjectWatcher::start(config(dir.path())).unwrap();
        fs::remove_file(&file).unwrap();
        let batch = changed(&mut watcher, &file).await;
        assert!(batch.paths.contains(&file));
    }

    #[test]
    fn native_write_close_remains_a_source_change_after_sdk_debouncing() {
        use notify_debouncer_full::{DebouncedEvent, notify::Event};
        let dir = canonical_tempdir();
        let file = dir.path().join("source.rs");
        fs::write(&file, "written").unwrap();
        let watcher = ProjectWatcher::start(config(dir.path())).unwrap();
        let batch = lock(&watcher.state).process(Ok(vec![DebouncedEvent::new(
            Event::new(EventKind::Access(AccessKind::Close(AccessMode::Write)))
                .add_path(file.clone()),
            std::time::Instant::now(),
        )]));
        assert!(batch.error.is_none());
        assert!(!batch.rescan);
        assert!(!batch.controls_changed);
        assert_eq!(batch.paths, BTreeSet::from([file]));
    }

    #[tokio::test]
    async fn continuous_native_changes_make_progress_before_writer_stops() {
        let dir = canonical_tempdir();
        let file = dir.path().join("source.rs");
        fs::write(&file, "first").unwrap();
        let mut watcher = ProjectWatcher::start(config(dir.path())).unwrap();
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let writer_file = file.clone();
        let writer = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(5));
            loop {
                tokio::select! {
                    _ = &mut stopped => break,
                    _ = tick.tick() => fs::write(&writer_file, "ongoing").unwrap(),
                }
            }
        });
        changed(&mut watcher, &file).await;
        stop.send(()).unwrap();
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn full_queue_requests_rescan_and_preserves_first_error() {
        let (sender, receiver) = mpsc::channel(QUEUED_BATCHES);
        let overflow = Arc::new(Overflow::default());
        let delivery = Delivery {
            sender,
            overflow: overflow.clone(),
        };
        let mut watcher = ProjectWatcher {
            state: Arc::new(Mutex::new(NativeState {
                config: Some(config(Path::new("."))),
                watcher: None,
                directories: HashMap::new(),
                external: ExternalControls::default(),
                external_watches: BTreeSet::new(),
            })),
            receiver,
            overflow,
        };
        for i in 0..QUEUED_BATCHES {
            delivery.send(ChangeBatch {
                paths: [PathBuf::from(format!("file-{i}"))].into(),
                ..Default::default()
            });
        }
        delivery.send(ChangeBatch {
            error: Some(io::Error::other("native error")),
            ..Default::default()
        });
        let batch = watcher.next().await.unwrap();
        assert!(batch.rescan);
        assert!(batch.controls_changed);
        assert!(batch.paths.is_empty());
        assert_eq!(batch.error.unwrap().to_string(), "native error");
    }

    #[test]
    fn rescan_and_explicit_changes_keep_priority_in_either_merge_order() {
        let paths = BTreeSet::from([
            PathBuf::from("removed/subtree"),
            PathBuf::from("z_probe.lua"),
        ]);
        for rescan_first in [false, true] {
            let rescan = ChangeBatch {
                rescan: true,
                ..Default::default()
            };
            let delta = ChangeBatch {
                paths: paths.clone(),
                ..Default::default()
            };
            let (mut first, second) = if rescan_first {
                (rescan, delta)
            } else {
                (delta, rescan)
            };
            first.merge(second);
            assert!(first.rescan);
            assert_eq!(first.paths, paths);
            assert!(!first.controls_changed);
            assert!(first.error.is_none());
        }
    }

    #[test]
    fn directory_removal_keeps_its_tombstone_and_same_batch_file_change() {
        use notify_debouncer_full::{
            DebouncedEvent,
            notify::{
                Event,
                event::{CreateKind, RemoveKind},
            },
        };
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let removed = root.join("removed");
        let changed = root.join("z_probe.lua");
        fs::create_dir(&removed).unwrap();
        let mut watcher = ProjectWatcher::start(config(&root)).unwrap();
        let batch = {
            let mut state = lock(&watcher.state);
            fs::remove_dir(&removed).unwrap();
            fs::write(&changed, "function probe() end\n").unwrap();
            state.process(Ok(vec![
                DebouncedEvent::new(
                    Event::new(EventKind::Remove(RemoveKind::Folder)).add_path(removed.clone()),
                    std::time::Instant::now(),
                ),
                DebouncedEvent::new(
                    Event::new(EventKind::Create(CreateKind::File)).add_path(changed.clone()),
                    std::time::Instant::now(),
                ),
            ]))
        };
        assert!(batch.rescan);
        assert_eq!(batch.paths, BTreeSet::from([removed, changed]));
        assert!(batch.error.is_none());
        watcher.stop();
    }

    #[test]
    fn merging_many_paths_compacts_into_one_rescan() {
        let mut batch = ChangeBatch {
            ..Default::default()
        };
        for i in 0..=PATHS_PER_BATCH {
            batch.merge(ChangeBatch {
                paths: [PathBuf::from(format!("file-{i}"))].into(),
                ..Default::default()
            });
        }
        assert!(batch.rescan);
        assert!(batch.paths.is_empty());
    }

    #[test]
    fn zero_debounce_is_rejected_before_starting_sdk() {
        let dir = TempDir::new().unwrap();
        let mut cfg = config(dir.path());
        cfg.debounce = Duration::ZERO;
        assert_eq!(
            ProjectWatcher::start(cfg).err().unwrap().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn dropping_native_watcher_releases_owner_and_directory_scopes() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        let watcher = ProjectWatcher::start(config(dir.path())).unwrap();
        let weak = Arc::downgrade(&watcher.state);
        drop(watcher);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn native_already_removed_watch_is_idempotent_and_other_errors_survive() {
        let dir = TempDir::new().unwrap();
        let (events, observed) = std::sync::mpsc::channel();
        let mut native = CheckoutWatcher::new(
            move |event| {
                let _ = events.send(event);
            },
            notify::Config::default(),
        )
        .unwrap();
        // Root removal is observed through its surviving parent on FSEvents.
        native
            .watch(dir.path(), RecursiveMode::NonRecursive)
            .unwrap();
        for index in 0..4 {
            let child = dir.path().join(format!("retired-{index}"));
            fs::create_dir(&child).unwrap();
            native.watch(&child, RecursiveMode::NonRecursive).unwrap();
            fs::remove_dir(&child).unwrap();
            loop {
                let event = observed
                    .recv_timeout(Duration::from_secs(8))
                    .unwrap()
                    .unwrap();
                if matches!(event.kind, EventKind::Remove(_)) {
                    break;
                }
            }
            native.unwatch(&child).unwrap();
            native.unwatch(&child).unwrap();
        }
        let absent = dir.path().join("absent");
        assert!(native.watch(&absent, RecursiveMode::NonRecursive).is_err());
    }

    #[tokio::test]
    async fn native_file_rename_reports_both_participants_without_file_id_inventory() {
        let dir = canonical_tempdir();
        let old = dir.path().join("old.rs");
        let new = dir.path().join("new.rs");
        fs::write(&old, "fn renamed() {}").unwrap();
        let mut watcher = ProjectWatcher::start(config(dir.path())).unwrap();
        fs::rename(&old, &new).unwrap();
        let batch = timeout(Duration::from_secs(8), async {
            let mut paths = BTreeSet::new();
            loop {
                let batch = watcher.next().await.unwrap();
                assert!(batch.error.is_none());
                if batch.rescan {
                    return ChangeBatch {
                        rescan: true,
                        ..Default::default()
                    };
                }
                paths.extend(batch.paths);
                if paths.contains(&old) && paths.contains(&new) {
                    return ChangeBatch {
                        paths,
                        ..Default::default()
                    };
                }
            }
        })
        .await
        .unwrap();
        assert!(batch.rescan || (batch.paths.contains(&old) && batch.paths.contains(&new)));
    }

    #[cfg(unix)]
    struct RestorePermissions {
        path: PathBuf,
        permissions: fs::Permissions,
    }

    #[cfg(unix)]
    impl RestorePermissions {
        fn deny(path: &Path) -> Self {
            use std::os::unix::fs::PermissionsExt;
            let restore = Self {
                path: path.to_owned(),
                permissions: fs::metadata(path).unwrap().permissions(),
            };
            fs::set_permissions(path, fs::Permissions::from_mode(0o0)).unwrap();
            match fs::read_dir(path) {
                Err(error) => assert_eq!(error.kind(), io::ErrorKind::PermissionDenied),
                Ok(_) => panic!("fixture must really deny directory reads"),
            }
            restore
        }
        fn restore(&self) {
            fs::set_permissions(&self.path, self.permissions.clone()).unwrap();
        }
    }

    #[cfg(unix)]
    impl Drop for RestorePermissions {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.path, self.permissions.clone());
        }
    }

    #[cfg(unix)]
    async fn denied_batch(watcher: &mut ProjectWatcher) -> ChangeBatch {
        timeout(Duration::from_secs(8), async {
            loop {
                let batch = watcher.next().await.expect("watcher remains open");
                if let Some(error) = &batch.error {
                    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
                    assert!(batch.rescan);
                    return batch;
                }
            }
        })
        .await
        .expect("subtree denial reported")
    }

    #[cfg(unix)]
    async fn accessible_change_while_denied(watcher: &mut ProjectWatcher, path: &Path) {
        timeout(Duration::from_secs(8), async {
            loop {
                let batch = watcher.next().await.expect("watcher remains open");
                // A delayed directory event can rediscover the denied subtree.
                // Every denial must remain causal and request reconciliation.
                if let Some(error) = &batch.error {
                    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
                    assert!(batch.rescan);
                }
                if batch.paths.contains(path) {
                    break;
                }
            }
        })
        .await
        .expect("accessible sibling must deliver its concrete FILE change");
    }

    #[test]
    fn debounced_native_errors_preserve_first_cause_and_request_rescan() {
        let (watcher, _sender) = ProjectWatcher::channel_fixture();
        for (native, expected) in [
            (
                notify::Error::io(io::Error::from(io::ErrorKind::PermissionDenied)),
                io::ErrorKind::PermissionDenied,
            ),
            (
                notify::Error::new(notify::ErrorKind::PathNotFound),
                io::ErrorKind::NotFound,
            ),
            (
                notify::Error::new(notify::ErrorKind::MaxFilesWatch),
                io::ErrorKind::Other,
            ),
        ] {
            let path = PathBuf::from("denied-or-missing");
            let batch = lock(&watcher.state).process(Err(vec![
                native.add_path(path.clone()),
                notify::Error::generic("later error must not replace the first cause"),
            ]));
            assert!(batch.rescan);
            assert!(batch.paths.is_empty());
            let error = batch.error.unwrap();
            assert_eq!(error.kind(), expected);
            let cause = error
                .get_ref()
                .unwrap()
                .downcast_ref::<notify::Error>()
                .unwrap();
            assert_eq!(cause.paths, [path]);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_unreadable_subtree_reports_and_preserves_accessible_watch() {
        let dir = TempDir::new().unwrap();
        let blocked = dir.path().join("blocked");
        let accessible = dir.path().join("accessible");
        fs::create_dir(&blocked).unwrap();
        fs::create_dir(&accessible).unwrap();
        let permissions = RestorePermissions::deny(&blocked);
        let mut watcher = ProjectWatcher::start(config(dir.path())).unwrap();
        denied_batch(&mut watcher).await;
        assert!(!lock(&watcher.state).directories.contains_key(&blocked));
        assert!(lock(&watcher.state).directories.contains_key(&accessible));
        let file = accessible.join("source.rs");
        fs::write(&file, "fn accessible() {}").unwrap();
        accessible_change_while_denied(&mut watcher, &file).await;
        watcher.stop();
        assert!(lock(&watcher.state).directories.is_empty());
        permissions.restore();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_new_unreadable_directory_recovers_after_ignore_rebuild() {
        let dir = TempDir::new().unwrap();
        let mut watcher = ProjectWatcher::start(config(dir.path())).unwrap();
        let blocked = dir.path().join("blocked");
        fs::create_dir(&blocked).unwrap();
        let permissions = RestorePermissions::deny(&blocked);
        denied_batch(&mut watcher).await;
        assert!(!lock(&watcher.state).directories.contains_key(&blocked));
        let accessible = dir.path().join("source.rs");
        fs::write(&accessible, "fn accessible() {}").unwrap();
        accessible_change_while_denied(&mut watcher, &accessible).await;
        // Rebuild must also skip a subtree that remains unreadable.
        fs::write(
            dir.path().join(".gitignore"),
            "# reload
",
        )
        .unwrap();
        denied_batch(&mut watcher).await;
        assert!(!lock(&watcher.state).directories.contains_key(&blocked));
        permissions.restore();
        fs::write(
            dir.path().join(".gitignore"),
            "# restored
",
        )
        .unwrap();
        changed(&mut watcher, &blocked).await;
        assert!(lock(&watcher.state).directories.contains_key(&blocked));
        let restored = blocked.join("source.rs");
        fs::write(&restored, "fn restored() {}").unwrap();
        changed(&mut watcher, &restored).await;
    }

    #[cfg(unix)]
    #[test]
    fn native_unreadable_checkout_remains_an_error() {
        let dir = TempDir::new().unwrap();
        let permissions = RestorePermissions::deny(dir.path());
        match ProjectWatcher::start(config(dir.path())) {
            Err(error) => assert_eq!(error.kind(), io::ErrorKind::PermissionDenied),
            Ok(_) => panic!("an unreadable checkout cannot be watched"),
        }
        permissions.restore();
    }

    #[test]
    fn subtree_denial_policy_preserves_other_native_errors() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        let mut batch = ChangeBatch::default();
        let permission = native_error(notify::Error::io(io::Error::from(
            io::ErrorKind::PermissionDenied,
        )));
        assert!(
            readable_subtree(root, &child, Err::<(), _>(permission), &mut batch)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            batch.error.take().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
        let root_denial = native_error(notify::Error::io(io::Error::from(
            io::ErrorKind::PermissionDenied,
        )));
        assert_eq!(
            readable_subtree(root, root, Err::<(), _>(root_denial), &mut batch)
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        for error in [
            notify::Error::io(io::Error::from_raw_os_error(28)),
            notify::Error::new(notify::ErrorKind::MaxFilesWatch),
            notify::Error::new(notify::ErrorKind::PathNotFound),
        ] {
            let error =
                readable_subtree(root, &child, Err::<(), _>(native_error(error)), &mut batch)
                    .unwrap_err();
            assert!(
                error
                    .get_ref()
                    .unwrap()
                    .downcast_ref::<notify::Error>()
                    .is_some()
            );
            assert!(batch.error.is_none());
        }
    }
    #[tokio::test]
    async fn removed_child_during_discovery_requests_rescan_and_keeps_checkout_watch() {
        let temp = TempDir::new().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let mut watcher = ProjectWatcher::start(config(&root)).unwrap();
        let child = root.join("temporary-output");
        {
            let mut state = lock(&watcher.state);
            let scopes = state.directories[&root].clone();
            fs::create_dir(&child).unwrap();
            let child_scopes = state
                .child_scopes(&scopes, child.file_name().unwrap())
                .unwrap();
            fs::remove_dir(&child).unwrap();

            let batch = state.add_tree(child.clone(), child_scopes).unwrap();
            assert!(batch.rescan);
            assert!(batch.error.is_none());
            assert!(!state.directories.contains_key(&child));

            let mut batch = ChangeBatch::default();
            let scope = state.child_scopes(&scopes, child.file_name().unwrap());
            assert!(
                readable_subtree(&root, &child, scope, &mut batch)
                    .unwrap()
                    .is_none()
            );
            let watched = state
                .watcher()
                .unwrap()
                .watch(&child, RecursiveMode::NonRecursive)
                .map_err(native_error);
            assert_eq!(
                watched.as_ref().unwrap_err().kind(),
                io::ErrorKind::NotFound
            );
            assert!(
                readable_subtree(&root, &child, watched, &mut batch)
                    .unwrap()
                    .is_none()
            );
            assert!(batch.rescan);
            assert!(batch.error.is_none());
            assert!(state.directories.contains_key(&root));
        }
        let file = root.join("source.rs");
        fs::write(&file, "fn retained() {}").unwrap();
        changed(&mut watcher, &file).await;
        watcher.stop();
    }

    #[test]
    fn removed_child_does_not_hide_missing_root_or_control() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("checkout");
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();
        let mut batch = ChangeBatch::default();
        let missing_control = fs::read(child.join(".astignore"));
        assert_eq!(
            readable_subtree(&root, &child, missing_control, &mut batch)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!batch.rescan);
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            readable_subtree(&root, &child, fs::read_dir(&child), &mut batch)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            readable_subtree(&root, &root, fs::read_dir(&root), &mut batch)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!batch.rescan);
        assert!(batch.error.is_none());
    }

    #[tokio::test]
    async fn explicit_project_controls_cross_protected_ignore_and_trigger_rescan() {
        let dir = canonical_tempdir();
        std::fs::create_dir_all(dir.path().join(".graphit/ast/queries")).unwrap();
        let mut cfg = config(dir.path());
        cfg.ast.defaults = vec![".graphit/".into()];
        cfg.wiki.defaults = cfg.ast.defaults.clone();
        cfg.control_files = vec![PathBuf::from(".graphit/project.json")];
        cfg.control_directories = vec![PathBuf::from(".graphit/ast/queries")];
        let mut watcher = ProjectWatcher::start(cfg).unwrap();
        std::fs::write(dir.path().join(".graphit/project.json"), b"{}").unwrap();
        let changed = timeout(Duration::from_secs(5), watcher.next())
            .await
            .unwrap()
            .unwrap();
        assert!(changed.rescan);
        assert!(changed.controls_changed);
        assert!(changed.error.is_none());
        std::fs::write(
            dir.path().join(".graphit/ast/queries/custom.yaml"),
            b"language: fixture",
        )
        .unwrap();
        let changed = timeout(Duration::from_secs(5), watcher.next())
            .await
            .unwrap()
            .unwrap();
        assert!(changed.rescan);
        assert!(changed.error.is_none());
        watcher.stop();
        let mut bad = config(dir.path());
        bad.control_directories = vec![PathBuf::from("../outside")];
        assert_eq!(
            ProjectWatcher::start(bad).err().unwrap().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    #[test]
    fn reading_project_controls_does_not_trigger_a_rescan() {
        use notify_debouncer_full::{
            DebouncedEvent,
            notify::{
                Event,
                event::{AccessKind, AccessMode},
            },
        };
        let dir = TempDir::new().unwrap();
        let mut cfg = config(dir.path());
        cfg.control_files = vec![PathBuf::from(".graphit/project.json")];
        cfg.control_directories = vec![PathBuf::from(".graphit/ast/queries")];
        let mut state = NativeState {
            config: Some(cfg),
            watcher: None,
            external: ExternalControls::default(),
            external_watches: BTreeSet::new(),
            directories: HashMap::new(),
        };
        let reads = [
            ".astignore",
            ".gitignore",
            ".graphit/project.json",
            ".graphit/ast/queries/custom.yaml",
        ]
        .into_iter()
        .map(|path| {
            DebouncedEvent::new(
                Event::new(EventKind::Access(AccessKind::Close(AccessMode::Read)))
                    .add_path(dir.path().join(path)),
                std::time::Instant::now(),
            )
        })
        .collect();
        let batch = state.process(Ok(reads));
        assert!(!batch.rescan);
        assert!(!batch.controls_changed);
        assert!(batch.paths.is_empty());
        assert!(batch.error.is_none());
    }
    #[tokio::test]
    async fn external_controls_follow_missing_births_and_ignore_unrelated_runtime() {
        let project = TempDir::new().unwrap();
        let installation = TempDir::new().unwrap();
        let current = installation.path().join("runtime/current/ast/queries");
        let user = installation.path().join("ast/queries");
        let mut watcher = ProjectWatcher::start(config(project.path())).unwrap();
        watcher
            .watch_external(ExternalControls {
                files: vec![installation.path().join("config.json")],
                directories: vec![current.clone(), user.clone()],
            })
            .unwrap();
        fs::write(installation.path().join("config.json"), "{}").unwrap();
        let batch = timeout(Duration::from_secs(5), watcher.next())
            .await
            .unwrap()
            .unwrap();
        assert!(batch.rescan && batch.error.is_none() && batch.paths.is_empty());
        assert!(batch.controls_changed);
        fs::create_dir_all(&current).unwrap();
        let batch = timeout(Duration::from_secs(5), watcher.next())
            .await
            .unwrap()
            .unwrap();
        assert!(batch.rescan && batch.error.is_none());
        fs::write(current.join("rust.yaml"), "language: rust").unwrap();
        let batch = timeout(Duration::from_secs(5), watcher.next())
            .await
            .unwrap()
            .unwrap();
        assert!(batch.rescan && batch.error.is_none());
        fs::create_dir_all(&user).unwrap();
        let batch = timeout(Duration::from_secs(5), watcher.next())
            .await
            .unwrap()
            .unwrap();
        assert!(batch.rescan && batch.error.is_none());
        fs::write(user.join("rust.yaml"), "language: rust").unwrap();
        let batch = timeout(Duration::from_secs(5), watcher.next())
            .await
            .unwrap()
            .unwrap();
        assert!(batch.rescan && batch.error.is_none());
        // Inspect only the causal native events: sibling version paths cannot trigger rescan.
        let mut state = lock(&watcher.state);
        use notify_debouncer_full::{
            DebouncedEvent,
            notify::{
                Event,
                event::{AccessKind, AccessMode, ModifyKind},
            },
        };
        let unrelated = DebouncedEvent::new(
            Event::new(EventKind::Modify(ModifyKind::Any)).add_path(
                installation
                    .path()
                    .join("runtime/other/ast/queries/rust.yaml"),
            ),
            std::time::Instant::now(),
        );
        let batch = state.process(Ok(vec![unrelated]));
        assert!(!batch.rescan && batch.error.is_none() && batch.paths.is_empty());
        let read = DebouncedEvent::new(
            Event::new(EventKind::Access(AccessKind::Close(AccessMode::Read)))
                .add_path(user.join("rust.yaml")),
            std::time::Instant::now(),
        );
        let batch = state.process(Ok(vec![read]));
        assert!(!batch.rescan && batch.error.is_none() && batch.paths.is_empty());
        drop(state);
        watcher.stop();
    }

    #[tokio::test]
    async fn controlled_retarget_rebuilds_ignored_ancestors_and_rejects_foreign_root() {
        let dir = canonical_tempdir();
        let old = PathBuf::from(".graphit/old");
        let new = PathBuf::from("target/private/queries");
        fs::create_dir_all(dir.path().join(&old)).unwrap();
        fs::create_dir_all(dir.path().join(&new)).unwrap();
        let mut cfg = config(dir.path());
        cfg.ast.defaults = vec![".graphit/".into(), "target/".into()];
        cfg.wiki.defaults = cfg.ast.defaults.clone();
        cfg.control_directories = vec![old.clone()];
        let mut watcher = ProjectWatcher::start(cfg.clone()).unwrap();
        let handle = watcher.controls();
        cfg.control_directories = vec![new.clone()];
        handle.reconfigure(cfg.clone()).unwrap();
        assert!(
            lock(&watcher.state)
                .directories
                .contains_key(&dir.path().join(&new))
        );
        assert!(
            !lock(&watcher.state)
                .directories
                .contains_key(&dir.path().join(&old))
        );
        fs::write(
            dir.path().join(&new).join("profile.yaml"),
            "language: fixture",
        )
        .unwrap();
        let batch = timeout(Duration::from_secs(5), watcher.next())
            .await
            .unwrap()
            .unwrap();
        assert!(batch.rescan && batch.error.is_none());
        let foreign = canonical_tempdir();
        let mut invalid = cfg.clone();
        invalid.root = foreign.path().to_owned();
        assert_eq!(
            handle.reconfigure(invalid).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        let mut invalid = cfg.clone();
        invalid.control_directories = vec![PathBuf::from("../foreign")];
        assert_eq!(
            handle.reconfigure(invalid).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(lock(&watcher.state).config.as_ref(), Some(&cfg));
        watcher.stop();
        assert_eq!(
            handle.reconfigure(cfg).unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
    }
    #[tokio::test]
    async fn external_controls_without_checkout_report_configuration_changes() {
        let directory = TempDir::new().unwrap();
        let config = directory.path().join("config.json");
        fs::write(&config, "{}").unwrap();
        let mut watcher = ProjectWatcher::start_external(
            ExternalControls {
                files: vec![config.clone()],
                ..Default::default()
            },
            Duration::from_millis(40),
        )
        .unwrap();
        fs::write(&config, "{\"daemon\":{\"models\":{\"slots\":[]}}}").unwrap();
        let batch = timeout(Duration::from_secs(8), watcher.next())
            .await
            .unwrap()
            .unwrap();
        assert!(batch.controls_changed && batch.rescan && batch.error.is_none());
        watcher.stop();
    }
}
