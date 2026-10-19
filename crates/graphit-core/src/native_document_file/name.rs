//! Native directory-entry names are observed data, never restored authority.
use super::*;
use crate::native_path::MAX_PATH_BYTES;
use serde::{Deserializer, Serializer};

/// A closed native filename record. Decoding does not qualify a live file.
///
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentName;
/// fn inspect(name: NativeDocumentName) { let _ = format!("{name:?}"); }
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::{NativeDocumentName, NativeDocumentObservation};
/// fn restore(name: NativeDocumentName) -> NativeDocumentObservation { name.into() }
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct NativeDocumentName(OsString);
impl NativeDocumentName {
    pub(super) fn copy_raw(name: &OsStr) -> io::Result<Self> {
        validate_name(name)?;
        let mut owned = OsString::new();
        owned
            .try_reserve_exact(name.len())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        owned.push(name);
        Ok(Self(owned))
    }

    pub fn as_os_str(&self) -> &OsStr {
        &self.0
    }
}
impl Serialize for NativeDocumentName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        validate_name(&self.0).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(
            self.0
                .to_str()
                .ok_or_else(|| serde::ser::Error::custom("document filename must be Unicode"))?,
        )
    }
}
impl<'de> Deserialize<'de> for NativeDocumentName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        validate_name(OsStr::new(&name)).map_err(serde::de::Error::custom)?;
        Ok(Self(OsString::from(name)))
    }
}

/// Finite bounds for one name scan, including unrelated entries.
/// Native bytes are Unix bytes or twice the Windows UTF-16 unit count.
/// OS/library internal allocation and blocking calls are not preemptible.
#[derive(Clone, Copy)]
pub struct NativeDocumentNameLimits {
    entries: usize,
    total_name_bytes: usize,
}
impl NativeDocumentNameLimits {
    pub fn new(entries: usize, total_name_bytes: usize) -> io::Result<Self> {
        if entries == 0
            || total_name_bytes == 0
            || entries.checked_add(1).is_none()
            || total_name_bytes.checked_add(1).is_none()
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            entries,
            total_name_bytes,
        })
    }
}

/// A request-bound file observation and its observed directory-entry spelling.
/// Missing has no name. Stored filename data cannot restore this observation.
///
/// ```compile_fail
/// use graphit_core::native_document_file::NamedNativeDocumentObservation;
/// fn copy(value: NamedNativeDocumentObservation) { let _ = value.clone(); }
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NamedNativeDocumentObservation;
/// fn inspect(value: NamedNativeDocumentObservation) { let _ = format!("{value:?}"); }
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NamedNativeDocumentObservation;
/// let _: NamedNativeDocumentObservation = serde_json::from_str("{}").unwrap();
/// ```
pub struct NamedNativeDocumentObservation {
    content_cap: usize,
    observation: NativeDocumentObservation,
    name: Option<NativeDocumentName>,
    limits: NativeDocumentNameLimits,
}
impl NamedNativeDocumentObservation {
    pub(super) fn validate_binding(
        &self,
        parent: &CapturedDocumentDirectory,
        name: &OsStr,
        context: &SnapshotContext,
    ) -> io::Result<()> {
        if self.observation.directory != *parent.identity()
            || self.observation.name != name
            || !self.observation.request.matches(context)
        {
            return Err(conflict());
        }
        Ok(())
    }

    pub(super) fn route_metadata(
        &self,
        parent: &CapturedDocumentDirectory,
        name: &OsStr,
        context: &SnapshotContext,
    ) -> io::Result<&FileObservation> {
        self.validate_binding(parent, name, context)?;
        self.observation
            .value
            .as_ref()
            .map(|value| &value.metadata)
            .ok_or_else(|| io::ErrorKind::InvalidInput.into())
    }

    pub fn observation(&self) -> &NativeDocumentObservation {
        &self.observation
    }
    pub fn native_name(&self) -> Option<&NativeDocumentName> {
        self.name.as_ref()
    }
}

impl CapturedDocumentDirectory {
    /// Read through the original destination and qualify its actual entry name.
    /// Content is read once; ordinary `read` does not enumerate the directory.
    pub fn read_named(
        &self,
        name: &OsStr,
        limits: NativeDocumentNameLimits,
        context: &SnapshotContext,
    ) -> io::Result<NamedNativeDocumentObservation> {
        self.read_named_bounded(name, self.limits.bytes, limits, context)
    }

    /// Read once with a content bound no larger than the captured directory limit.
    pub fn read_named_bounded(
        &self,
        name: &OsStr,
        content_cap: usize,
        limits: NativeDocumentNameLimits,
        context: &SnapshotContext,
    ) -> io::Result<NamedNativeDocumentObservation> {
        if content_cap > self.limits.bytes {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let (observation, name) = self
            .read_inner(name, Some(limits), context, content_cap)
            .map_err(|error| io::Error::from(error.kind()))?;
        Ok(NamedNativeDocumentObservation {
            content_cap,
            observation,
            name,
            limits,
        })
    }

    /// Reobserve the original route, exact bytes, full metadata and entry name.
    pub fn revalidate_named_observation(
        &self,
        expected: &NamedNativeDocumentObservation,
        context: &SnapshotContext,
    ) -> io::Result<()> {
        let expected_observation = &expected.observation;
        if expected_observation.directory != *self.identity()
            || !expected_observation.request.matches(context)
        {
            return Err(conflict());
        }
        let fresh = self.read_named_bounded(
            &expected_observation.name,
            expected.content_cap,
            expected.limits,
            context,
        )?;
        if fresh.name != expected.name || fresh.observation.value != expected_observation.value {
            return Err(conflict());
        }
        Ok(())
    }

    pub(super) fn observe_name(
        &self,
        original: &Path,
        file: &File,
        before: &FileObservation,
        limits: NativeDocumentNameLimits,
        context: &SnapshotContext,
    ) -> io::Result<NativeDocumentName> {
        self.revalidate(context)?;
        if observe_handle(file, self.limits.access_bytes)? != *before {
            return Err(conflict());
        }
        context.check()?;
        let mut entries = fs::read_dir(&self.canonical)?;
        let mut remaining = limits.entries;
        let mut used = 0usize;
        loop {
            let Some(entry) = next_entry(&mut entries, &mut remaining, context)? else {
                return Err(conflict());
            };
            #[cfg(test)]
            hooks::run(&hooks::AFTER_ENTRY);
            context.check()?;
            let name = entry.file_name();
            used = used
                .checked_add(native_bytes(&name)?)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            if used > limits.total_name_bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirEntryExt;
                let PhysicalId::Unix { inode, .. } = before.identity else {
                    return Err(conflict());
                };
                if entry.ino() != inode {
                    continue;
                }
            }
            context.check()?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = self.path(&name)?;
            context.check()?;
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(conflict());
            }
            if physical_id(&path, &metadata)? != before.identity {
                continue;
            }
            #[cfg(test)]
            hooks::run(&hooks::BEFORE_OPEN);
            context.check()?;
            let matched = open_file(&path)?;
            if handle_id(&matched)? != before.identity || !matched.metadata()?.is_file() {
                return Err(conflict());
            }
            #[cfg(test)]
            hooks::run(&hooks::AFTER_MATCH);
            context.check()?;
            self.revalidate(context)?;
            for path in [original, path.as_path()] {
                context.check()?;
                if physical_id(path, &regular_metadata(path)?.ok_or_else(conflict)?)?
                    != before.identity
                {
                    return Err(conflict());
                }
            }
            if observe_handle(file, self.limits.access_bytes)? != *before
                || handle_id(&matched)? != before.identity
            {
                return Err(conflict());
            }
            context.check()?;
            return NativeDocumentName::copy_raw(&name);
        }
    }
}

fn next_entry<T>(
    entries: &mut impl Iterator<Item = io::Result<T>>,
    remaining: &mut usize,
    context: &SnapshotContext,
) -> io::Result<Option<T>> {
    context.check()?;
    let Some(entry) = entries.next() else {
        context.check()?;
        return Ok(None);
    };
    let entry = entry?;
    context.check()?;
    // One overflow/EOF probe is allowed, without filename or metadata access.
    if *remaining == 0 {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    *remaining -= 1;
    Ok(Some(entry))
}

pub(super) fn validate_name(name: &OsStr) -> io::Result<()> {
    let text = name.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "document filename must be Unicode",
        )
    })?;
    if text.len() > MAX_PATH_BYTES {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut components = Path::new(name).components();
    if name.is_empty()
        || components.next() != Some(Component::Normal(name))
        || components.next().is_some()
        || name.as_encoded_bytes().contains(&0)
        || name.as_encoded_bytes().contains(&b'/')
        || (cfg!(windows)
            && name
                .as_encoded_bytes()
                .iter()
                .any(|b| matches!(b, b':' | b'\\')))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

#[cfg(test)]
mod hooks {
    use std::{cell::RefCell, thread::LocalKey};
    type Hook = RefCell<Option<Box<dyn FnOnce()>>>;
    thread_local! {
        pub(super) static AFTER_ENTRY: Hook = const { RefCell::new(None) };
        pub(super) static BEFORE_OPEN: Hook = const { RefCell::new(None) };
        pub(super) static AFTER_MATCH: Hook = const { RefCell::new(None) };
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
    use std::time::{Duration, Instant};

    fn fixture() -> (
        tempfile::TempDir,
        SnapshotContext,
        CapturedDocumentDirectory,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("config"), b"same").unwrap();
        let context = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let directory = CapturedDocumentDirectory::capture(
            tmp.path(),
            NativeDocumentFileLimits::new(64, 65536, 256, 65536).unwrap(),
            &context,
        )
        .unwrap();
        (tmp, context, directory)
    }
    fn limits() -> NativeDocumentNameLimits {
        NativeDocumentNameLimits::new(32, 1024).unwrap()
    }

    #[test]
    fn named_revalidation_rejects_cancelled_or_expired_context() {
        for deadline in [false, true] {
            let (_tmp, _ordinary, directory) = fixture();
            let cancel = Cancellation::default();
            let context = SnapshotContext::new(
                Instant::now()
                    + if deadline {
                        Duration::from_millis(250)
                    } else {
                        Duration::from_secs(30)
                    },
                cancel.clone(),
            );

            let expected = directory
                .read_named_bounded(OsStr::new("config"), 4, limits(), &context)
                .unwrap();
            if deadline {
                std::thread::sleep(Duration::from_millis(260));
            } else {
                cancel.cancel();
            }
            assert_eq!(
                directory
                    .revalidate_named_observation(&expected, &context)
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
    fn named_revalidation_compares_full_native_name() {
        let (_tmp, context, directory) = fixture();

        let mut expected = directory
            .read_named_bounded(OsStr::new("config"), 4, limits(), &context)
            .unwrap();
        // Private corruption fault: public callers cannot forge this observation.
        expected.name = Some(NativeDocumentName::copy_raw(OsStr::new("other")).unwrap());
        assert_eq!(
            directory
                .revalidate_named_observation(&expected, &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn named_read_rejects_growth_at_eof_probe() {
        use std::io::Write;
        let (tmp, context, directory) = fixture();

        let path = tmp.path().join("config");
        test_hooks::BEFORE_PROBE.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                OpenOptions::new()
                    .append(true)
                    .open(path)
                    .unwrap()
                    .write_all(b"z")
                    .unwrap();
            }));
        });
        assert_eq!(
            directory
                .read_named_bounded(OsStr::new("config"), 4, limits(), &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );

        assert_eq!(fs::read(tmp.path().join("config")).unwrap(), b"samez");
    }

    #[test]
    fn named_read_rejects_growth_at_open() {
        let (tmp, context, directory) = fixture();

        let path = tmp.path().join("config");
        test_hooks::BEFORE_OPEN.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || fs::write(path, b"samez").unwrap()))
        });
        assert_eq!(
            directory
                .read_named_bounded(OsStr::new("config"), 4, limits(), &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );

        assert_eq!(fs::read(tmp.path().join("config")).unwrap(), b"samez");
    }

    #[test]
    fn named_read_rejects_expired_context() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("config"), b"same").unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let context = SnapshotContext::new(deadline, Cancellation::default());
        let directory = CapturedDocumentDirectory::capture(
            tmp.path(),
            NativeDocumentFileLimits::new(64, 65536, 256, 65536).unwrap(),
            &context,
        )
        .unwrap();

        std::thread::sleep(
            deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
        );
        assert_eq!(
            directory
                .read_named_bounded(OsStr::new("config"), 4, limits(), &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::TimedOut
        );

        assert_eq!(fs::read(tmp.path().join("config")).unwrap(), b"same");
    }

    #[test]
    fn named_read_checks_cancel_between_blocks() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = vec![b'x'; 128 * 1024];
        fs::write(tmp.path().join("config"), &bytes).unwrap();
        let cancel = Cancellation::default();
        let context =
            SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
        let directory = CapturedDocumentDirectory::capture(
            tmp.path(),
            NativeDocumentFileLimits::new(bytes.len(), 65536, 256, 65536).unwrap(),
            &context,
        )
        .unwrap();

        test_hooks::AFTER_CHUNK
            .with(|slot| *slot.borrow_mut() = Some(Box::new(move || cancel.cancel())));
        assert_eq!(
            directory
                .read_named_bounded(OsStr::new("config"), bytes.len(), limits(), &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );

        assert_eq!(fs::read(tmp.path().join("config")).unwrap(), bytes);
    }

    #[test]
    fn named_read_rejects_foreign_same_bytes_and_stale_names() {
        for after_match in [false, true] {
            let (tmp, context, directory) = fixture();

            let root = tmp.path().to_owned();
            let action: Box<dyn FnOnce()> = Box::new(move || {
                fs::rename(root.join("config"), root.join("old")).unwrap();
                if !after_match {
                    fs::write(root.join("config"), b"same").unwrap();
                }
            });
            let hook = if after_match {
                &hooks::AFTER_MATCH
            } else {
                &hooks::BEFORE_OPEN
            };
            hook.with(|slot| *slot.borrow_mut() = Some(action));
            assert_eq!(
                directory
                    .read_named_bounded(OsStr::new("config"), 8, limits(), &context)
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::InvalidData
            );

            assert_eq!(fs::read(tmp.path().join("old")).unwrap(), b"same");
            if !after_match {
                assert_eq!(fs::read(tmp.path().join("config")).unwrap(), b"same");
            }
        }
    }

    #[test]
    fn named_read_preserves_scan_limits_and_cancel() {
        for cancel_scan in [false, true] {
            let (tmp, context, directory) = fixture();
            let scan_limits = if cancel_scan {
                limits()
            } else {
                NativeDocumentNameLimits::new(32, 1).unwrap()
            };

            if cancel_scan {
                let cancel = context.cancellation().clone();
                hooks::AFTER_ENTRY
                    .with(|slot| *slot.borrow_mut() = Some(Box::new(move || cancel.cancel())));
            }
            let expected = if cancel_scan {
                io::ErrorKind::Interrupted
            } else {
                io::ErrorKind::FileTooLarge
            };
            assert_eq!(
                directory
                    .read_named_bounded(OsStr::new("config"), 8, scan_limits, &context)
                    .err()
                    .unwrap()
                    .kind(),
                expected
            );

            assert_eq!(fs::read(tmp.path().join("config")).unwrap(), b"same");
        }
    }

    #[test]
    fn named_read_rejects_parent_retarget() {
        let outer = tempfile::tempdir().unwrap();
        let path = outer.path().join("parent");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("config"), b"same").unwrap();
        let context = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let directory = CapturedDocumentDirectory::capture(
            &path,
            NativeDocumentFileLimits::new(64, 65536, 256, 65536).unwrap(),
            &context,
        )
        .unwrap();

        fs::rename(&path, outer.path().join("old")).unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("config"), b"foreign").unwrap();
        assert_eq!(
            directory
                .read_named_bounded(OsStr::new("config"), 8, limits(), &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );

        assert_eq!(fs::read(path.join("config")).unwrap(), b"foreign");
        assert_eq!(fs::read(outer.path().join("old/config")).unwrap(), b"same");
    }

    #[test]
    fn named_read_missing_does_not_enumerate() {
        let (_tmp, context, directory) = fixture();

        hooks::AFTER_ENTRY
            .with(|slot| *slot.borrow_mut() = Some(Box::new(|| panic!("unexpected name scan"))));
        assert!(
            directory
                .read_named_bounded(OsStr::new("missing"), 8, limits(), &context)
                .unwrap()
                .native_name()
                .is_none()
        );
        hooks::AFTER_ENTRY.with(|slot| {
            slot.borrow_mut().take();
        });
    }

    #[test]
    fn scan_admission_permits_one_overflow_or_eof_probe_and_propagates_iterator_error() {
        use std::cell::Cell;
        let (_, context, _) = fixture();
        let calls = Cell::new(0);
        let mut entries = [Ok(1), Ok(2), Ok(3)]
            .into_iter()
            .inspect(|_| calls.set(calls.get() + 1));
        let mut remaining = 1;
        assert_eq!(
            next_entry(&mut entries, &mut remaining, &context).unwrap(),
            Some(1)
        );
        assert_eq!(
            next_entry(&mut entries, &mut remaining, &context)
                .unwrap_err()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
        assert_eq!(calls.get(), 2);
        let mut exact = [Ok(1)].into_iter();
        let mut remaining = 1;
        assert_eq!(
            next_entry(&mut exact, &mut remaining, &context).unwrap(),
            Some(1)
        );
        assert_eq!(
            next_entry(&mut exact, &mut remaining, &context).unwrap(),
            None
        );
        let mut failure = [Err::<(), _>(io::Error::from(
            io::ErrorKind::PermissionDenied,
        ))]
        .into_iter();
        assert_eq!(
            next_entry(&mut failure, &mut remaining, &context)
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        context.cancellation().cancel();
        let mut never = std::iter::from_fn(|| -> Option<io::Result<()>> {
            panic!("cancelled scan must not advance")
        });
        assert_eq!(
            next_entry(&mut never, &mut remaining, &context)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn cancelled_after_first_entry_never_opens_or_adopts_candidate() {
        let (tmp, context, directory) = fixture();
        let cancel = context.cancellation().clone();
        hooks::AFTER_ENTRY
            .with(|slot| *slot.borrow_mut() = Some(Box::new(move || cancel.cancel())));
        assert_eq!(
            directory
                .read_named(OsStr::new("config"), limits(), &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(fs::read(tmp.path().join("config")).unwrap(), b"same");
    }

    #[test]
    fn same_bytes_foreign_replacement_between_entry_metadata_and_open_is_rejected() {
        let (tmp, context, directory) = fixture();
        let root = tmp.path().to_owned();
        hooks::BEFORE_OPEN.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                fs::rename(root.join("config"), root.join("old")).unwrap();
                fs::write(root.join("config"), b"same").unwrap();
            }))
        });
        assert_eq!(
            directory
                .read_named(OsStr::new("config"), limits(), &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(tmp.path().join("config")).unwrap(), b"same");
        assert_eq!(fs::read(tmp.path().join("old")).unwrap(), b"same");
    }

    #[test]
    fn same_id_rename_after_entry_match_is_not_returned_as_a_stale_name() {
        let (tmp, context, directory) = fixture();
        let root = tmp.path().to_owned();
        hooks::AFTER_MATCH.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                fs::rename(root.join("config"), root.join("renamed")).unwrap();
            }))
        });
        assert!(
            directory
                .read_named(OsStr::new("config"), limits(), &context)
                .is_err()
        );
        assert_eq!(fs::read(tmp.path().join("renamed")).unwrap(), b"same");
        let actual = directory
            .read_named(OsStr::new("renamed"), limits(), &context)
            .unwrap();
        assert_eq!(
            actual.native_name().unwrap().as_os_str(),
            OsStr::new("renamed")
        );
    }

    #[test]
    fn cancellation_after_full_id_match_does_not_return_a_qualified_name() {
        let (tmp, context, directory) = fixture();
        let cancel = context.cancellation().clone();
        hooks::AFTER_MATCH
            .with(|slot| *slot.borrow_mut() = Some(Box::new(move || cancel.cancel())));
        assert_eq!(
            directory
                .read_named(OsStr::new("config"), limits(), &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(fs::read(tmp.path().join("config")).unwrap(), b"same");
    }

    #[test]
    fn parent_replacement_after_match_fails_and_preserves_both_directories() {
        let outer = tempfile::tempdir().unwrap();
        let original = outer.path().join("parent");
        fs::create_dir(&original).unwrap();
        fs::write(original.join("config"), b"same").unwrap();
        let context = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let directory = CapturedDocumentDirectory::capture(
            &original,
            NativeDocumentFileLimits::new(64, 65536, 256, 65536).unwrap(),
            &context,
        )
        .unwrap();
        let root = outer.path().to_owned();
        hooks::AFTER_MATCH.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                fs::rename(root.join("parent"), root.join("old")).unwrap();
                fs::create_dir(root.join("parent")).unwrap();
                fs::write(root.join("parent/config"), b"foreign").unwrap();
            }))
        });
        assert!(
            directory
                .read_named(OsStr::new("config"), limits(), &context)
                .is_err()
        );
        assert_eq!(
            fs::read(outer.path().join("parent/config")).unwrap(),
            b"foreign"
        );
        assert_eq!(fs::read(outer.path().join("old/config")).unwrap(), b"same");
    }

    #[test]
    fn missing_and_regular_read_never_enter_name_scan() {
        let (tmp, context, directory) = fixture();
        hooks::AFTER_ENTRY
            .with(|slot| *slot.borrow_mut() = Some(Box::new(|| panic!("unexpected name scan"))));
        let missing = directory
            .read_named(OsStr::new("missing"), limits(), &context)
            .unwrap();
        assert!(missing.native_name().is_none());
        directory.read(OsStr::new("config"), &context).unwrap();
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
        hooks::AFTER_ENTRY.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
