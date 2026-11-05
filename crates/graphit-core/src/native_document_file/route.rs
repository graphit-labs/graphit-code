//! Finite, request-bound physical route comparison without reading document bodies.
use super::*;

/// Bounds for one batch, not a global metadata quota or an OS time/RSS bound.
/// Names include the target and every input, including ordered duplicates.
#[derive(Clone, Copy)]
pub struct NativeDocumentRouteProbeLimits {
    routes: usize,
    total_name_bytes: usize,
    total_access_bytes: usize,
}
impl NativeDocumentRouteProbeLimits {
    pub fn new(
        routes: usize,
        total_name_bytes: usize,
        total_access_bytes: usize,
    ) -> io::Result<Self> {
        if [routes, total_name_bytes, total_access_bytes].contains(&0)
            || [routes, total_name_bytes, total_access_bytes]
                .iter()
                .any(|n| n.checked_add(1).is_none())
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            routes,
            total_name_bytes,
            total_access_bytes,
        })
    }
}

/// Current physical relation only; no ownership or future alias authority.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NativeDocumentRouteRelation {
    Missing,
    Distinct,
    Same,
}

struct Route {
    name: NativeDocumentName,
    path: PathBuf,
}

/// A single-use batch borrowing its original parent and request.
/// Preparation copies all inputs and allocates result slots before any probes.
/// The target name is raw input data, not an observed directory-entry spelling.
///
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentRouteProbeBatch;
/// fn copy(value: NativeDocumentRouteProbeBatch<'_, '_>) { let _ = value.clone(); }
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentRouteProbeBatch;
/// fn inspect(value: NativeDocumentRouteProbeBatch<'_, '_>) { let _ = format!("{value:?}"); }
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentRouteProbeBatch;
/// let _: NativeDocumentRouteProbeBatch<'static, 'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentRouteProbeBatch;
/// let _ = NativeDocumentRouteProbeBatch {};
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::{NativeDocumentRouteProbeBatch, NamedNativeDocumentObservation};
/// fn twice(value: NativeDocumentRouteProbeBatch<'_, '_>, expected: &NamedNativeDocumentObservation) {
///     let _ = value.probe(expected);
///     let _ = value.probe(expected);
/// }
/// ```
pub struct NativeDocumentRouteProbeBatch<'d, 'c> {
    parent: &'d CapturedDocumentDirectory,
    context: &'c SnapshotContext,
    target: Route,
    routes: Vec<Route>,
    results: Vec<(NativeDocumentName, NativeDocumentRouteRelation)>,
}

/// Complete ordered results, returned only after every route and final guard.
/// Names are the raw inputs; missing routes do not prove future distinctness.
///
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentRouteProbes;
/// let _ = NativeDocumentRouteProbes {};
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentRouteProbes;
/// fn copy(value: NativeDocumentRouteProbes<'_, '_>) { let _ = value.clone(); }
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentRouteProbes;
/// fn inspect(value: NativeDocumentRouteProbes<'_, '_>) { let _ = format!("{value:?}"); }
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentRouteProbes;
/// let _: NativeDocumentRouteProbes<'static, 'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::{NativeDocumentRouteProbes, NativeDocumentObservation};
/// fn restore(value: NativeDocumentRouteProbes<'_, '_>) -> NativeDocumentObservation { value.into() }
/// ```
/// ```compile_fail
/// use graphit_core::{native_document_file::NativeDocumentRouteProbes, artifact_store::NativeDocumentStage};
/// fn publish(value: NativeDocumentRouteProbes<'_, '_>) -> NativeDocumentStage<'static> { value.into() }
/// ```
pub struct NativeDocumentRouteProbes<'d, 'c> {
    _parent: &'d CapturedDocumentDirectory,
    _context: &'c SnapshotContext,
    results: Vec<(NativeDocumentName, NativeDocumentRouteRelation)>,
}
impl NativeDocumentRouteProbes<'_, '_> {
    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = (&NativeDocumentName, NativeDocumentRouteRelation)> {
        self.results
            .iter()
            .map(|(name, relation)| (name, *relation))
    }
}

impl CapturedDocumentDirectory {
    /// Admit all raw paths, native name storage, result slots and access snapshots.
    /// No target/candidate opens, scans, content reads or content reservations.
    pub fn prepare_route_probes<'d, 'c>(
        &'d self,
        target: &OsStr,
        routes: &[&OsStr],
        limits: NativeDocumentRouteProbeLimits,
        context: &'c SnapshotContext,
    ) -> io::Result<NativeDocumentRouteProbeBatch<'d, 'c>> {
        prepare(self, target, routes, limits, context)
            .map_err(|error| io::Error::from(error.kind()))
    }
}

fn prepare<'d, 'c>(
    parent: &'d CapturedDocumentDirectory,
    target: &OsStr,
    routes: &[&OsStr],
    limits: NativeDocumentRouteProbeLimits,
    context: &'c SnapshotContext,
) -> io::Result<NativeDocumentRouteProbeBatch<'d, 'c>> {
    parent.revalidate(context)?;
    if routes.len() > limits.routes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let access = routes
        .len()
        .checked_mul(2)
        .and_then(|n| n.checked_add(2))
        .and_then(|n| n.checked_mul(parent.limits.access_bytes))
        .ok_or(io::ErrorKind::FileTooLarge)?;
    if access > limits.total_access_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut used = native_bytes(target)?;
    // Validate every input before copying or native target/candidate effects.
    parent.path(target)?;
    for name in routes {
        context.check()?;
        parent.path(name)?;
        used = used
            .checked_add(native_bytes(name)?)
            .ok_or(io::ErrorKind::FileTooLarge)?;
    }
    if used > limits.total_name_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(routes.len())
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    let mut results = Vec::new();
    results
        .try_reserve_exact(routes.len())
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    let copy = |name: &OsStr| -> io::Result<Route> {
        Ok(Route {
            name: NativeDocumentName::copy_raw(name)?,
            path: parent.path(name)?,
        })
    };
    let target = copy(target)?;
    for name in routes {
        context.check()?;
        owned.push(copy(name)?);
    }
    parent.revalidate(context)?;
    Ok(NativeDocumentRouteProbeBatch {
        parent,
        context,
        target,
        routes: owned,
        results,
    })
}

impl<'d, 'c> NativeDocumentRouteProbeBatch<'d, 'c> {
    pub fn target_name(&self) -> &NativeDocumentName {
        &self.target.name
    }

    /// Compare explicit routes to a real present observation from this request.
    /// At most target + candidate handles are retained. Windows pathname identity
    /// uses one additional transient handle; captured ancestors are separate.
    pub fn probe(
        mut self,
        expected: &NamedNativeDocumentObservation,
    ) -> io::Result<NativeDocumentRouteProbes<'d, 'c>> {
        self.probe_inner(expected)
            .map_err(|error| io::Error::from(error.kind()))?;
        Ok(NativeDocumentRouteProbes {
            _parent: self.parent,
            _context: self.context,
            results: self.results,
        })
    }

    fn probe_inner(&mut self, expected: &NamedNativeDocumentObservation) -> io::Result<()> {
        self.parent.revalidate(self.context)?;
        let before =
            expected.route_metadata(self.parent, self.target.name.as_os_str(), self.context)?;
        let initial = regular_metadata(&self.target.path)?.ok_or_else(conflict)?;
        if physical_id(&self.target.path, &initial)? != before.identity {
            return Err(conflict());
        }
        self.context.check()?;
        let target = open_file(&self.target.path)?;
        #[cfg(test)]
        hooks::run(&hooks::AFTER_TARGET_OPEN);
        self.guard_target(&target, before)?;
        // One target snapshot initially, one after each route, one finally;
        // at most one candidate access snapshot for each Same route: 2*n+2.
        for route in std::mem::take(&mut self.routes) {
            self.parent.revalidate(self.context)?;
            let relation = match regular_metadata(&route.path)? {
                None => {
                    #[cfg(test)]
                    hooks::run(&hooks::AFTER_CANDIDATE_OPEN);
                    self.parent.revalidate(self.context)?;
                    if regular_metadata(&route.path)?.is_some() {
                        return Err(conflict());
                    }
                    NativeDocumentRouteRelation::Missing
                }
                Some(metadata) => {
                    let initial_id = physical_id(&route.path, &metadata)?;
                    #[cfg(test)]
                    hooks::run(&hooks::BEFORE_CANDIDATE_OPEN);
                    self.context.check()?;
                    let candidate = open_file(&route.path)?;
                    let identity = regular_identity(&candidate)?;
                    if identity != initial_id {
                        return Err(conflict());
                    }
                    #[cfg(test)]
                    hooks::run(&hooks::AFTER_CANDIDATE_OPEN);
                    self.context.check()?;
                    let relation = if identity == before.identity {
                        if observe_access(&candidate, self.parent.limits.access_bytes)? != *before {
                            return Err(conflict());
                        }
                        NativeDocumentRouteRelation::Same
                    } else {
                        NativeDocumentRouteRelation::Distinct
                    };
                    self.parent.revalidate(self.context)?;
                    if regular_identity(&candidate)? != identity
                        || physical_id(
                            &route.path,
                            &regular_metadata(&route.path)?.ok_or_else(conflict)?,
                        )? != identity
                    {
                        return Err(conflict());
                    }
                    relation
                }
            };
            self.guard_target(&target, before)?;
            self.results.push((route.name, relation));
            #[cfg(test)]
            hooks::run(&hooks::AFTER_ROUTE);
        }
        #[cfg(test)]
        hooks::run(&hooks::BEFORE_FINAL);
        self.guard_target(&target, before)
    }

    fn guard_target(&self, file: &File, before: &FileObservation) -> io::Result<()> {
        self.parent.revalidate(self.context)?;
        if observe_access(file, self.parent.limits.access_bytes)? != *before
            || physical_id(
                &self.target.path,
                &regular_metadata(&self.target.path)?.ok_or_else(conflict)?,
            )? != before.identity
        {
            return Err(conflict());
        }
        self.parent.revalidate(self.context)
    }
}

fn observe_access(file: &File, cap: usize) -> io::Result<FileObservation> {
    #[cfg(test)]
    hooks::ACCESS_CALLS.with(|calls| calls.set(calls.get() + 1));
    observe_handle(file, cap)
}

// Distinct files require a regular single-link full identity, not write access,
// target content caps or an access-control snapshot. Never read their bodies.
fn regular_identity(file: &File) -> io::Result<PhysicalId> {
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(conflict());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(conflict());
        }
    }
    #[cfg(windows)]
    if winapi_util::file::information(file)?.number_of_links() != 1 {
        return Err(conflict());
    }
    let identity = handle_id(file)?;
    identity.validate()?;
    Ok(identity)
}

#[cfg(test)]
mod hooks {
    use std::{
        cell::{Cell, RefCell},
        thread::LocalKey,
    };
    type Hook = RefCell<Option<Box<dyn FnOnce()>>>;
    thread_local! {
        pub(super) static ACCESS_CALLS: Cell<usize> = const { Cell::new(0) };
        pub(super) static AFTER_TARGET_OPEN: Hook = const { RefCell::new(None) };
        pub(super) static BEFORE_CANDIDATE_OPEN: Hook = const { RefCell::new(None) };
        pub(super) static AFTER_CANDIDATE_OPEN: Hook = const { RefCell::new(None) };
        pub(super) static AFTER_ROUTE: Hook = const { RefCell::new(None) };
        pub(super) static BEFORE_FINAL: Hook = const { RefCell::new(None) };
    }
    pub(super) fn run(hook: &'static LocalKey<Hook>) {
        if let Some(action) = hook.with(|slot| slot.borrow_mut().take()) {
            action();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::file_lock::Cancellation;
    use std::{
        cell::Cell,
        os::unix::fs::{PermissionsExt, symlink},
        time::{Duration, Instant},
    };

    fn fixture(
        deadline: Duration,
        cancel: Cancellation,
    ) -> (
        tempfile::TempDir,
        SnapshotContext,
        CapturedDocumentDirectory,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("target"), b"same").unwrap();
        fs::write(tmp.path().join("other"), b"unrelated").unwrap();
        let context = SnapshotContext::new(Instant::now() + deadline, cancel);
        let parent = CapturedDocumentDirectory::capture(
            tmp.path(),
            NativeDocumentFileLimits::new(64, 65536, 256, 65536).unwrap(),
            &context,
        )
        .unwrap();
        hooks::ACCESS_CALLS.with(|calls| calls.set(0));
        (tmp, context, parent)
    }
    fn names() -> NativeDocumentNameLimits {
        NativeDocumentNameLimits::new(32, 65536).unwrap()
    }
    fn bounds(n: usize) -> NativeDocumentRouteProbeLimits {
        NativeDocumentRouteProbeLimits::new(n.max(1), 65536, (2 * n + 2) * 65536).unwrap()
    }

    #[test]
    fn route_probes_access_snapshot_count_is_admitted_even_on_errors() {
        let (_tmp, ctx, parent) = fixture(Duration::from_secs(30), Cancellation::default());
        let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
        let batch = parent
            .prepare_route_probes("target".as_ref(), &["target".as_ref(); 3], bounds(3), &ctx)
            .unwrap();
        assert_eq!(hooks::ACCESS_CALLS.with(Cell::get), 0);
        batch.probe(&expected).unwrap();
        assert_eq!(hooks::ACCESS_CALLS.with(Cell::get), 8);
        hooks::ACCESS_CALLS.with(|calls| calls.set(0));
        let batch = parent
            .prepare_route_probes(
                "target".as_ref(),
                &["target".as_ref(), "other".as_ref()],
                bounds(2),
                &ctx,
            )
            .unwrap();
        let path = parent.canonical.join("target");
        hooks::AFTER_ROUTE.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || fs::write(path, b"edit").unwrap()))
        });
        assert_eq!(
            batch.probe(&expected).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(hooks::ACCESS_CALLS.with(Cell::get) <= 6);
    }

    #[test]
    fn route_probes_reject_target_replacement_after_open_without_body_io() {
        let (tmp, ctx, parent) = fixture(Duration::from_secs(30), Cancellation::default());
        let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
        let path = tmp.path().join("target");
        let historical = tmp.path().join("historical");
        let changed = path.clone();
        hooks::AFTER_TARGET_OPEN.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                fs::rename(&path, historical).unwrap();
                fs::write(path, b"same").unwrap();
            }))
        });
        assert_eq!(
            parent
                .prepare_route_probes("target".as_ref(), &["other".as_ref()], bounds(1), &ctx)
                .unwrap()
                .probe(&expected)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );

        assert_eq!(fs::read(changed).unwrap(), b"same");
    }

    #[test]
    fn route_probes_reject_candidate_replacement_before_or_after_open() {
        for after_open in [false, true] {
            let (tmp, ctx, parent) = fixture(Duration::from_secs(30), Cancellation::default());
            let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
            let path = tmp.path().join("other");
            let historical = tmp.path().join("historical");
            let action: Box<dyn FnOnce()> = Box::new(move || {
                fs::rename(&path, historical).unwrap();
                fs::write(path, b"unrelated").unwrap();
            });
            if after_open {
                hooks::AFTER_CANDIDATE_OPEN.with(|slot| *slot.borrow_mut() = Some(action));
            } else {
                hooks::BEFORE_CANDIDATE_OPEN.with(|slot| *slot.borrow_mut() = Some(action));
            }
            assert_eq!(
                parent
                    .prepare_route_probes("target".as_ref(), &["other".as_ref()], bounds(1), &ctx)
                    .unwrap()
                    .probe(&expected)
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::InvalidData
            );

            assert_eq!(fs::read(tmp.path().join("target")).unwrap(), b"same");
        }
    }

    #[test]
    fn route_probes_last_invalid_candidate_never_returns_partial_results() {
        for kind in ["symlink", "hardlink", "directory"] {
            let (tmp, ctx, parent) = fixture(Duration::from_secs(30), Cancellation::default());
            let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
            let bad = tmp.path().join("last");
            match kind {
                "symlink" => symlink(tmp.path().join("other"), bad).unwrap(),
                "hardlink" => fs::hard_link(tmp.path().join("other"), bad).unwrap(),
                _ => fs::create_dir(bad).unwrap(),
            }
            let result = parent
                .prepare_route_probes(
                    "target".as_ref(),
                    &["target".as_ref(), "last".as_ref()],
                    bounds(2),
                    &ctx,
                )
                .unwrap()
                .probe(&expected);
            assert_eq!(result.err().unwrap().kind(), io::ErrorKind::InvalidData);

            assert_eq!(fs::read(tmp.path().join("target")).unwrap(), b"same");
            assert!(hooks::ACCESS_CALLS.with(Cell::get) <= 6);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn route_probes_fifo_is_rejected_before_a_blocking_open() {
        let (tmp, ctx, parent) = fixture(Duration::from_secs(30), Cancellation::default());
        let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            tmp.path().join("fifo"),
            rustix::fs::Mode::RUSR,
        )
        .unwrap();
        assert_eq!(
            parent
                .prepare_route_probes(
                    "target".as_ref(),
                    &["target".as_ref(), "fifo".as_ref()],
                    bounds(2),
                    &ctx
                )
                .unwrap()
                .probe(&expected)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );

        assert!(hooks::ACCESS_CALLS.with(Cell::get) <= 6);
    }

    #[test]
    fn route_probes_missing_candidate_birth_is_a_conflict() {
        let (tmp, ctx, parent) = fixture(Duration::from_secs(30), Cancellation::default());
        let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
        let path = tmp.path().join("missing");
        hooks::AFTER_CANDIDATE_OPEN.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || fs::write(path, b"birth").unwrap()))
        });
        assert_eq!(
            parent
                .prepare_route_probes("target".as_ref(), &["missing".as_ref()], bounds(1), &ctx)
                .unwrap()
                .probe(&expected)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn route_probes_cancel_or_deadline_between_routes() {
        for deadline in [false, true] {
            let cancel = Cancellation::default();
            let (_tmp, ctx, parent) = fixture(
                if deadline {
                    Duration::from_millis(250)
                } else {
                    Duration::from_secs(30)
                },
                cancel.clone(),
            );
            let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
            hooks::AFTER_ROUTE.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move || {
                    if deadline {
                        std::thread::sleep(Duration::from_millis(260));
                    } else {
                        cancel.cancel();
                    }
                }))
            });
            assert_eq!(
                parent
                    .prepare_route_probes(
                        "target".as_ref(),
                        &["target".as_ref(), "other".as_ref()],
                        bounds(2),
                        &ctx
                    )
                    .unwrap()
                    .probe(&expected)
                    .err()
                    .unwrap()
                    .kind(),
                if deadline {
                    io::ErrorKind::TimedOut
                } else {
                    io::ErrorKind::Interrupted
                }
            );
        }
    }

    #[test]
    fn route_probes_final_target_metadata_access_or_parent_changes_fail() {
        for change in ["bytes", "access", "parent"] {
            let (tmp, ctx, parent) = fixture(Duration::from_secs(30), Cancellation::default());
            let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
            let target = tmp.path().join("target");
            let directory = tmp.path().to_owned();
            let moved = tmp.path().with_extension("historical");
            let cleanup = moved.clone();
            hooks::BEFORE_FINAL.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move || match change {
                    "bytes" => fs::write(target, b"edit").unwrap(),
                    "access" => {
                        fs::set_permissions(target, fs::Permissions::from_mode(0o400)).unwrap()
                    }
                    _ => {
                        fs::rename(&directory, &moved).unwrap();
                        fs::create_dir(directory).unwrap();
                    }
                }))
            });
            let error = parent
                .prepare_route_probes("target".as_ref(), &["other".as_ref()], bounds(1), &ctx)
                .unwrap()
                .probe(&expected)
                .err()
                .unwrap();
            assert!(matches!(
                error.kind(),
                io::ErrorKind::InvalidData | io::ErrorKind::PermissionDenied
            ));

            if change == "parent" {
                fs::remove_dir_all(cleanup).unwrap();
            }
        }
    }

    #[test]
    fn route_probes_reject_non_unicode_and_preserve_unicode_names() {
        use std::os::unix::ffi::OsStringExt;
        let (tmp, ctx, parent) = fixture(Duration::from_secs(30), Cancellation::default());
        let name = OsString::from_vec(vec![b'n', 0xff]);
        #[cfg(target_os = "linux")]
        fs::write(tmp.path().join(&name), b"other").unwrap();
        assert_eq!(
            parent
                .prepare_route_probes("target".as_ref(), &[&name], bounds(1), &ctx)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        #[cfg(target_os = "linux")]
        assert_eq!(fs::read(tmp.path().join(name)).unwrap(), b"other");
        let name = OsString::from("n-é");
        fs::write(tmp.path().join(&name), b"unicode").unwrap();
        let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
        let result = parent
            .prepare_route_probes("target".as_ref(), &[&name, &name], bounds(2), &ctx)
            .unwrap()
            .probe(&expected)
            .unwrap();
        for (actual, relation) in result.iter() {
            assert_eq!(actual.as_os_str(), name);
            assert!(relation == NativeDocumentRouteRelation::Distinct);
        }
    }
}
