//! A qualified private slot shared by all document writers of one physical parent.
use super::*;
use crate::{
    artifact_store::{CapturedPublicationScope, ScopeContext},
    native_document_lock::NativeDocumentLock,
};

const NAMESPACE: &str = ".graphit-provider-documents.state";
const LOCK: &str = "lock";

/// Live provider/scope captures, without document ownership or cleanup authority.
/// Missing admitted directories may remain after failure; Drop never removes them.
/// Bootstrap precedes provider locks, then Hub and Core locks. Callers deduplicate
/// acquisitions by full parent identity while retaining and revalidating every route.
///
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentNamespace;
/// let _: NativeDocumentNamespace = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentNamespace;
/// fn duplicate(slot: &NativeDocumentNamespace) { let _ = Clone::clone(slot); }
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentNamespace;
/// fn expose(slot: &NativeDocumentNamespace) { println!("{slot:?}"); }
/// ```
/// ```compile_fail
/// use graphit_core::{artifact_store::ScopeBindingRecord, native_document_file::NativeDocumentNamespace};
/// fn restore(data: ScopeBindingRecord) -> NativeDocumentNamespace { data.into() }
/// ```
pub struct NativeDocumentNamespace {
    pub(super) scope: CapturedPublicationScope,
    pub(super) parent: CapturedDocumentDirectory,
    pub(super) namespace: CapturedDocumentDirectory,
}

impl NativeDocumentNamespace {
    /// Capture a real authorized parent and qualify its fixed private namespace.
    /// Layout admission precedes SDK birth. Existing public provider parents are
    /// allowed; the namespace and every newly created directory must be private.
    pub fn capture(
        scope: &CapturedPublicationScope,
        requested_parent: &Path,
        limits: NativeDocumentFileLimits,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        Self::capture_checked(scope, requested_parent, limits, context)
            .map_err(|error| io::Error::from(error.kind()))
    }

    fn capture_checked(
        scope: &CapturedPublicationScope,
        input: &Path,
        limits: NativeDocumentFileLimits,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        context.check()?;
        limits.path(input)?;
        if input.ancestors().count() > limits.ancestors {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let scope_context = scope_context(context);
        scope.revalidate(&scope_context)?;
        let mut anchor_input = None;
        for path in input.ancestors() {
            context.check()?;
            let result = fs::symlink_metadata(path);
            #[cfg(test)]
            ANCESTOR_PROBE.with(|hook| {
                if let Some(observer) = hook.borrow_mut().as_mut() {
                    observer(path);
                }
            });
            match result {
                Ok(_) => {
                    anchor_input = Some(path);
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
        }
        let anchor_input = anchor_input.ok_or(io::ErrorKind::NotFound)?;
        context.check()?;
        let anchor_path = fs::canonicalize(anchor_input)?;
        check_capture_layout(limits, anchor_input, &anchor_path)?;
        let mut births = Vec::new();
        let mut planned_parent = anchor_path.clone();
        for component in input
            .strip_prefix(anchor_input)
            .map_err(|_| conflict())?
            .components()
        {
            let Component::Normal(name) = component else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            planned_parent.push(name);
            limits.check_path(&planned_parent)?;
            births
                .try_reserve(1)
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            births.push(planned_parent.clone());
        }
        let namespace_path = planned_parent.join(NAMESPACE);
        check_capture_layout(limits, input, &planned_parent)?;
        check_capture_layout(limits, &namespace_path, &namespace_path)?;
        limits.check_path(&namespace_path.join(LOCK))?;
        births
            .try_reserve(1)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        births.push(namespace_path.clone());
        scope.admit_native_document_layout(&planned_parent, &births, &scope_context)?;
        let anchor = CapturedDocumentDirectory::capture(anchor_input, limits, context)?;
        if anchor.canonical != anchor_path {
            return Err(conflict());
        }
        let mut created = Vec::new();
        created
            .try_reserve_exact(births.len())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        for path in &births {
            check_bootstrap(scope, &scope_context, &anchor, &created, context)?;
            #[cfg(test)]
            run_hook(&BEFORE_BIRTH, path);
            check_bootstrap(scope, &scope_context, &anchor, &created, context)?;
            let file = match graphit_fs::create_private_directory(path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    directory_metadata(path)?;
                    open_access_directory(path)?
                }
                Err(error) => return Err(error),
            };
            #[cfg(test)]
            run_hook(&AFTER_BIRTH, path);
            context.check()?;
            let identity = handle_id(&file)?;
            identity.validate()?;
            let directory = DirectoryHandle {
                path: path.clone(),
                file,
                identity,
            };
            verify_private(&directory)?;
            created.push(directory);
            check_bootstrap(scope, &scope_context, &anchor, &created, context)?;
        }
        let parent = CapturedDocumentDirectory::capture(input, limits, context)?;
        let namespace = CapturedDocumentDirectory::capture(&namespace_path, limits, context)?;
        check_bootstrap(scope, &scope_context, &anchor, &created, context)?;
        let expected_parent = created[..created.len() - 1]
            .last()
            .map_or_else(|| anchor.identity(), |directory| &directory.identity);
        if parent.canonical != planned_parent
            || parent.identity() != expected_parent
            || namespace.identity() != &created.last().ok_or_else(conflict)?.identity
        {
            return Err(conflict());
        }
        let captured = Self {
            scope: scope.clone(),
            parent,
            namespace,
        };
        captured.revalidate_checked(context)?;
        Ok(captured)
    }

    pub fn parent(&self) -> &CapturedDocumentDirectory {
        &self.parent
    }

    /// Historical full parent identity data; revalidation remains mandatory.
    pub fn directory_identity(&self) -> NativeDocumentDirectoryIdentity {
        self.parent.directory_identity()
    }

    /// Revalidate this route even when another alias supplied the deduplicated lock.
    pub fn revalidate(&self, context: &SnapshotContext) -> io::Result<()> {
        self.revalidate_checked(context)
            .map_err(|error| io::Error::from(error.kind()))
    }

    fn revalidate_checked(&self, context: &SnapshotContext) -> io::Result<()> {
        self.scope.revalidate(&scope_context(context))?;
        self.parent.revalidate(context)?;
        self.namespace.revalidate(context)?;
        drop(self.namespace.private_handle(context)?);
        self.scope.revalidate(&scope_context(context))?;
        self.parent.revalidate(context)
    }

    pub fn lock<'a>(
        &'a self,
        context: &'a SnapshotContext,
    ) -> io::Result<NativeDocumentNamespaceGuard<'a>> {
        self.revalidate(context)?;
        let lock = NativeDocumentLock::acquire(&self.namespace, OsStr::new(LOCK), context)?;
        let guard = NativeDocumentNamespaceGuard {
            namespace: self,
            context,
            lock,
        };
        guard.revalidate()?;
        Ok(guard)
    }
}

/// Exact kernel lock, borrowing the real captures and original request context.
/// It grants no Stage, arbitrary file access or capability from a restored record.
///
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentNamespaceGuard;
/// let _: NativeDocumentNamespaceGuard<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentNamespaceGuard;
/// fn duplicate(guard: &NativeDocumentNamespaceGuard<'_>) { let _ = Clone::clone(guard); }
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentNamespaceGuard;
/// fn expose(guard: &NativeDocumentNamespaceGuard<'_>) { println!("{guard:?}"); }
/// ```
/// ```compile_fail
/// use graphit_core::{native_document_file::NativeDocumentNamespaceGuard, native_document_lock::NativeDocumentLock};
/// fn restore(data: NativeDocumentLock<'static>) -> NativeDocumentNamespaceGuard<'static> { data.into() }
/// ```
#[must_use = "dropping the guard releases the kernel lock"]
pub struct NativeDocumentNamespaceGuard<'a> {
    pub(super) namespace: &'a NativeDocumentNamespace,
    pub(super) context: &'a SnapshotContext,
    lock: NativeDocumentLock<'a>,
}
impl NativeDocumentNamespaceGuard<'_> {
    pub fn revalidate(&self) -> io::Result<()> {
        self.namespace.revalidate(self.context)?;
        self.lock.revalidate()?;
        self.namespace.revalidate(self.context)
    }
}

fn scope_context(context: &SnapshotContext) -> ScopeContext {
    ScopeContext::from_snapshot(context)
}

fn verify_private(directory: &DirectoryHandle) -> io::Result<()> {
    graphit_fs::verify_private_directory(&directory.file)?;
    if handle_id(&directory.file)? != directory.identity
        || !directory.file.metadata()?.is_dir()
        || physical_id(&directory.path, &directory_metadata(&directory.path)?)?
            != directory.identity
    {
        return Err(conflict());
    }
    graphit_fs::verify_private_directory(&directory.file)
}

fn check_bootstrap(
    scope: &CapturedPublicationScope,
    scope_context: &ScopeContext,
    anchor: &CapturedDocumentDirectory,
    created: &[DirectoryHandle],
    context: &SnapshotContext,
) -> io::Result<()> {
    scope.revalidate(scope_context)?;
    anchor.revalidate(context)?;
    for directory in created {
        context.check()?;
        verify_private(directory)?;
    }
    context.check()
}

fn check_capture_layout(
    limits: NativeDocumentFileLimits,
    input: &Path,
    canonical: &Path,
) -> io::Result<()> {
    if canonical.ancestors().count() > limits.ancestors {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    limits.check_path(input)?;
    limits.check_path(canonical)
}

#[cfg(test)]
type BirthHook = std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>>;
#[cfg(test)]
type ProbeHook = std::cell::RefCell<Option<Box<dyn FnMut(&Path)>>>;
#[cfg(test)]
thread_local! {
    static BEFORE_BIRTH: BirthHook = const { std::cell::RefCell::new(None) };
    static AFTER_BIRTH: BirthHook = const { std::cell::RefCell::new(None) };
    static ANCESTOR_PROBE: ProbeHook = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn run_hook(hook: &'static std::thread::LocalKey<BirthHook>, path: &Path) {
    hook.with(|hook| {
        if let Some(callback) = hook.borrow_mut().take() {
            callback(path);
        }
    });
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{
        artifact_store::ScopeLimits, brand::Brand, file_lock::Cancellation,
        installation_identity::InstallationId, project_paths::InstallationPaths,
    };
    use std::{
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    fn fixture() -> (
        tempfile::TempDir,
        PathBuf,
        CapturedPublicationScope,
        std::sync::Arc<SnapshotContext>,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let owner = root.join("owner");
        fs::create_dir(&owner).unwrap();
        let installation =
            InstallationPaths::resolve(Brand::default(), &root, None, Some(&owner)).unwrap();
        let context = std::sync::Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        let scope = CapturedPublicationScope::installation(
            &installation,
            InstallationId::new("namespace-unit").unwrap(),
            [root.clone()],
            ScopeLimits::new(1, 65536, 1024).unwrap(),
            &std::sync::Arc::clone(&context),
        )
        .unwrap();
        (temp, root, scope, context)
    }
    fn limits() -> NativeDocumentFileLimits {
        NativeDocumentFileLimits::new(1, 65536, 256, 65536).unwrap()
    }

    #[test]
    fn actual_sdk_race_winner_is_qualified_without_repair() {
        use std::os::unix::fs::PermissionsExt;
        for private in [true, false] {
            let (_tmp, root, scope, context) = fixture();
            BEFORE_BIRTH.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move |path| {
                    if private {
                        drop(graphit_fs::create_private_directory(path).unwrap());
                    } else {
                        fs::create_dir(path).unwrap();
                        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
                        fs::write(path.join("sentinel"), b"foreign").unwrap();
                    }
                }))
            });
            let result = NativeDocumentNamespace::capture(&scope, &root, limits(), &context);
            if private {
                result.unwrap().revalidate(&context).unwrap();
            } else {
                assert!(result.is_err());
                assert_eq!(
                    fs::metadata(root.join(NAMESPACE))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o7777,
                    0o755
                );
                assert_eq!(
                    fs::read(root.join(NAMESPACE).join("sentinel")).unwrap(),
                    b"foreign"
                );
            }
            assert!(!root.join(NAMESPACE).join(LOCK).exists());
        }
    }

    #[test]
    fn replacement_after_sdk_birth_cannot_substitute_another_full_identity() {
        let (_tmp, root, scope, context) = fixture();
        let original = root.join("original");
        let backup = original.clone();
        AFTER_BIRTH.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |path| {
                fs::rename(path, backup).unwrap();
                drop(graphit_fs::create_private_directory(path).unwrap());
                fs::write(path.join("sentinel"), b"foreign").unwrap();
            }))
        });
        assert_eq!(
            NativeDocumentNamespace::capture(&scope, &root, limits(), &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert!(original.exists());
        assert_eq!(
            fs::read(root.join(NAMESPACE).join("sentinel")).unwrap(),
            b"foreign"
        );
        assert!(!root.join(NAMESPACE).join(LOCK).exists());
    }

    #[test]
    fn cancellation_after_birth_preserves_private_residue_without_a_capability() {
        let (_tmp, root, scope, _) = fixture();
        let cancellation = Cancellation::default();
        let context = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancellation.clone(),
        );
        AFTER_BIRTH
            .with(|hook| *hook.borrow_mut() = Some(Box::new(move |_| cancellation.cancel())));
        assert_eq!(
            NativeDocumentNamespace::capture(&scope, &root, limits(), &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        graphit_fs::verify_private_directory(&fs::File::open(root.join(NAMESPACE)).unwrap())
            .unwrap();
        assert_eq!(fs::read_dir(root.join(NAMESPACE)).unwrap().count(), 0);
    }

    #[test]
    fn alias_retarget_before_sdk_birth_is_zero_layout_mutation() {
        use std::os::unix::fs::symlink;
        let (_tmp, root, scope, context) = fixture();
        let provider = root.join("provider");
        fs::create_dir(&provider).unwrap();
        let alias = root.join("alias");
        symlink(&provider, &alias).unwrap();
        let route = alias.clone();
        let other = root.clone();
        BEFORE_BIRTH.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |_| {
                fs::remove_file(&route).unwrap();
                symlink(other, route).unwrap();
            }))
        });
        assert_eq!(
            NativeDocumentNamespace::capture(&scope, &alias, limits(), &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert!(!provider.join(NAMESPACE).exists());
        assert!(!root.join(NAMESPACE).exists());
    }

    #[test]
    fn live_scope_replacement_during_actual_wait_is_rejected_after_kernel_acquisition() {
        let (_tmp, root, scope, context) = fixture();
        let ns = NativeDocumentNamespace::capture(&scope, &root, limits(), &context).unwrap();
        let held = ns.lock(&context).unwrap();
        let (send, receive) = mpsc::channel();
        thread::scope(|threads| {
            let waiting = threads.spawn(|| {
                crate::file_lock::BEFORE_WAIT.with(|hook| {
                    *hook.borrow_mut() = Some(Box::new(move || send.send(()).unwrap()))
                });
                ns.lock(&context).err().unwrap().kind()
            });
            receive.recv_timeout(Duration::from_secs(10)).unwrap();
            fs::rename(root.join("owner"), root.join("original-owner")).unwrap();
            drop(graphit_fs::create_private_directory(&root.join("owner")).unwrap());
            drop(held);
            assert_eq!(waiting.join().unwrap(), io::ErrorKind::InvalidData);
        });
        assert!(root.join("original-owner").exists());
        assert_eq!(
            fs::metadata(root.join(NAMESPACE).join(LOCK)).unwrap().len(),
            0
        );
    }

    #[test]
    fn cancellation_during_actual_wait_keeps_the_holder_and_permanent_entry() {
        let (_tmp, root, scope, context) = fixture();
        let ns = NativeDocumentNamespace::capture(&scope, &root, limits(), &context).unwrap();
        let held = ns.lock(&context).unwrap();
        let cancellation = Cancellation::default();
        let wait = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancellation.clone(),
        );
        let (send, receive) = mpsc::channel();
        thread::scope(|threads| {
            let waiting = threads.spawn(|| {
                crate::file_lock::BEFORE_WAIT.with(|hook| {
                    *hook.borrow_mut() = Some(Box::new(move || send.send(()).unwrap()))
                });
                ns.lock(&wait).err().unwrap().kind()
            });
            receive.recv_timeout(Duration::from_secs(10)).unwrap();
            cancellation.cancel();
            assert_eq!(waiting.join().unwrap(), io::ErrorKind::Interrupted);
        });
        held.revalidate().unwrap();
        assert!(root.join(NAMESPACE).join(LOCK).exists());
    }

    #[test]
    fn cancellation_during_ancestor_discovery_stops_further_metadata_probes() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let (_tmp, root, scope, _) = fixture();
        let cancellation = Cancellation::default();
        let context = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancellation.clone(),
        );
        let probes = Arc::new(AtomicUsize::new(0));
        let observed = probes.clone();
        ANCESTOR_PROBE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
                cancellation.cancel();
            }))
        });
        let result = NativeDocumentNamespace::capture(
            &scope,
            &root.join("absent/deeper"),
            limits(),
            &context,
        );
        ANCESTOR_PROBE.with(|hook| hook.borrow_mut().take());
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::Interrupted);
        assert_eq!(probes.load(Ordering::SeqCst), 1);
        assert!(!root.join("absent").exists());
        assert!(!root.join(NAMESPACE).exists());
    }
}
