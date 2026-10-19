//! Fixed private logical state; no Stage proof, restored grant or public pathname writer.
use super::{
    NativeDocumentDirectoryIdentity, NativeDocumentNamespace, NativeDocumentNamespaceGuard,
};
use crate::{
    artifact_store::ScopeContext,
    file_io::RequestIdentity,
    native_file_identity::PhysicalId,
    private_document::{self, Access, CapturedFile, CreatedScratch, Observed},
};
use secrecy::zeroize::Zeroizing;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Seek, Write},
    path::Path,
};
const JOURNAL: &str = "documents.json";

/// Validated envelope capacities, not a resource reservation or a writing grant.
#[derive(Clone, Copy)]
pub struct NativeDocumentJournalWriteBounds {
    ceiling: usize,
    previous: usize,
    desired: usize,
}
impl NativeDocumentJournalWriteBounds {
    /// Zero previous bytes means that consumption requires an absent journal.
    /// A positive previous bound also allows absence. All sizes include headers.
    pub fn new(
        journal_bytes: usize,
        previous_bytes: usize,
        desired_bytes: usize,
    ) -> io::Result<Self> {
        if desired_bytes == 0 || previous_bytes > journal_bytes || desired_bytes > journal_bytes {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        capacity(journal_bytes)?;
        Ok(Self {
            ceiling: journal_bytes,
            previous: previous_bytes,
            desired: desired_bytes,
        })
    }
}
fn capacity(bytes: usize) -> io::Result<u64> {
    if bytes == 0 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    bytes
        .checked_add(1)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| io::ErrorKind::FileTooLarge.into())
}

/// Observed data without a retained descriptor, lock or restoration constructor.
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentJournalSnapshot;
/// let _: NativeDocumentJournalSnapshot = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentJournalSnapshot;
/// fn copy(value: &NativeDocumentJournalSnapshot) { let _ = value.clone(); let _: NativeDocumentJournalSnapshot = value.clone(); }
/// ```
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentJournalSnapshot;
/// fn expose(value: &NativeDocumentJournalSnapshot) { println!("{value:?}"); }
/// ```
pub struct NativeDocumentJournalSnapshot {
    request: RequestIdentity,
    parent: NativeDocumentDirectoryIdentity,
    namespace: NativeDocumentDirectoryIdentity,
    limit: usize,
    value: Option<Observed>,
}
impl NativeDocumentJournalSnapshot {
    /// Borrow the previously validated body. Its application schema belongs to Hub.
    pub fn body(&self) -> io::Result<Option<&RawValue>> {
        self.value
            .as_ref()
            .map(|value| decode(&value.bytes).map(|envelope| envelope.body))
            .transpose()
    }
}
/// Qualified metadata-publication observation, independent of the primary error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeDocumentJournalEffect {
    Applied,
    Unchanged,
    Unknown,
}
/// Static categories only: no body, native path or vendor diagnostic excerpts.
#[derive(Debug)]
pub struct NativeDocumentJournalFailure {
    primary: io::ErrorKind,
    secondary: Option<io::ErrorKind>,
    effect: NativeDocumentJournalEffect,
}
impl NativeDocumentJournalFailure {
    pub fn kind(&self) -> io::ErrorKind {
        self.primary
    }
    pub fn secondary_kind(&self) -> Option<io::ErrorKind> {
        self.secondary
    }
    pub fn effect(&self) -> NativeDocumentJournalEffect {
        self.effect
    }
    fn unknown(error: io::Error) -> Self {
        Self {
            primary: error.kind(),
            secondary: None,
            effect: NativeDocumentJournalEffect::Unknown,
        }
    }
}
impl std::fmt::Display for NativeDocumentJournalFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "private journal {:?} ({:?})",
            self.primary, self.effect
        )
    }
}
impl std::error::Error for NativeDocumentJournalFailure {}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<'a> {
    parent_identity: NativeDocumentDirectoryIdentity,
    namespace_identity: NativeDocumentDirectoryIdentity,
    physical_id: PhysicalId,
    #[serde(borrow)]
    body: &'a RawValue,
}
fn decode(bytes: &[u8]) -> io::Result<Envelope<'_>> {
    let envelope: Envelope<'_> =
        serde_json::from_slice(bytes).map_err(|_| private_document::conflict())?;
    envelope.physical_id.validate()?;
    object(envelope.body)?;
    Ok(envelope)
}
fn object(body: &RawValue) -> io::Result<()> {
    if !body.get().trim_start().starts_with('{') {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
struct BoundedWriter {
    bytes: Option<Zeroizing<Vec<u8>>>,
    count: usize,
    limit: usize,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .count
            .checked_add(bytes.len())
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if next > self.limit {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        if let Some(output) = &mut self.bytes {
            output
                .try_reserve_exact(bytes.len())
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            output.extend_from_slice(bytes);
        }
        self.count = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode(envelope: &Envelope<'_>, limit: usize, retain: bool) -> io::Result<BoundedWriter> {
    let mut bytes = retain.then(|| Zeroizing::new(Vec::new()));
    if let Some(bytes) = &mut bytes {
        let size = encode(envelope, limit, false)?.count;
        bytes
            .try_reserve_exact(size)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
    }
    let mut writer = BoundedWriter {
        bytes,
        count: 0,
        limit,
    };
    serde_json::to_writer(&mut writer, envelope).map_err(|error| {
        io::Error::from(error.io_error_kind().unwrap_or(io::ErrorKind::InvalidData))
    })?;
    Ok(writer)
}
fn worst(body: &RawValue, limit: usize) -> io::Result<usize> {
    private_document::validate_limit(limit)?;
    object(body)?;
    let envelope = Envelope {
        parent_identity: NativeDocumentDirectoryIdentity {
            identity: PhysicalId::largest(),
        },
        namespace_identity: NativeDocumentDirectoryIdentity {
            identity: PhysicalId::largest(),
        },
        physical_id: PhysicalId::largest(),
        body,
    };
    Ok(encode(&envelope, limit, false)?.count)
}
impl NativeDocumentNamespace {
    /// Validate the maximum serialized journal envelope before creating its namespace.
    pub fn admit_journal_body(body: &RawValue, max_bytes: usize) -> io::Result<usize> {
        worst(body, max_bytes)
    }
}
impl NativeDocumentNamespaceGuard<'_> {
    fn snapshot(
        &self,
        value: Option<Observed>,
        limit: usize,
    ) -> io::Result<NativeDocumentJournalSnapshot> {
        if let Some(value) = &value {
            let envelope = decode(&value.bytes)?;
            if envelope.parent_identity != self.namespace.directory_identity()
                || envelope.namespace_identity != self.namespace.namespace.directory_identity()
                || envelope.physical_id != value.metadata.identity
            {
                return Err(private_document::conflict());
            }
        }
        self.revalidate()?;
        Ok(NativeDocumentJournalSnapshot {
            request: self.context.request_identity(),
            parent: self.namespace.directory_identity(),
            namespace: self.namespace.namespace.directory_identity(),
            limit,
            value,
        })
    }
    /// Read the fixed journal and close its FD before returning immutable data.
    pub fn read_journal(&self, max_bytes: usize) -> io::Result<NativeDocumentJournalSnapshot> {
        self.read_checked(max_bytes)
    }
    fn read_checked(&self, max_bytes: usize) -> io::Result<NativeDocumentJournalSnapshot> {
        private_document::validate_limit(max_bytes)?;
        self.revalidate()?;
        let destination = self.namespace.namespace.path(OsStr::new(JOURNAL))?;
        let check = || self.revalidate();
        let access = Access {
            limit: max_bytes,
            access_limit: self.namespace.namespace.limits.access_bytes,
            context: Some(self.context),
            check: &check,
        };
        let value = CapturedFile::open(&destination, &access, || {
            #[cfg(test)]
            test_hooks::run(&test_hooks::AFTER_READ);
        })?
        .map(|captured| captured.value);
        self.snapshot(value, max_bytes)
    }
    /// Replace only the fixed journal under the real kernel guard and original request.
    pub fn replace_journal(
        &self,
        expected: &NativeDocumentJournalSnapshot,
        body: &RawValue,
        bounds: NativeDocumentJournalWriteBounds,
    ) -> Result<NativeDocumentJournalSnapshot, NativeDocumentJournalFailure> {
        let max_bytes = bounds.ceiling;
        let setup = (|| {
            if !expected.request.matches(self.context)
                || expected.parent != self.namespace.directory_identity()
                || expected.namespace != self.namespace.namespace.directory_identity()
                || expected.limit != max_bytes
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            if expected
                .value
                .as_ref()
                .is_some_and(|value| value.bytes.len() > bounds.previous)
                || worst(body, max_bytes)? > bounds.desired
            {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            self.revalidate()?;
            let destination = self.namespace.namespace.path(OsStr::new(JOURNAL))?;
            let scratch = private_document::random_path(
                self.namespace.namespace.canonical_path(),
                ".documents-",
            )?;
            self.namespace
                .namespace
                .path(scratch.file_name().ok_or(io::ErrorKind::InvalidInput)?)?;
            self.namespace.scope.admit_native_document_layout(
                self.namespace.namespace.canonical_path(),
                &[destination.clone(), scratch.clone()],
                &ScopeContext::from_snapshot(self.context),
            )?;
            Ok((destination, scratch))
        })()
        .map_err(NativeDocumentJournalFailure::unknown)?;
        let (destination, scratch_path) = setup;
        let check = || self.revalidate();
        let access = Access {
            limit: max_bytes,
            access_limit: self.namespace.namespace.limits.access_bytes,
            context: Some(self.context),
            check: &check,
        };
        // No initial body read: two comparisons use the closed expected snapshot.
        let old = (|| {
            let captured = private_document::capture(&destination, &access)?;
            match (&expected.value, captured) {
                (None, None) => Ok(None),
                (Some(expected), Some((file, metadata))) if expected.metadata == metadata => {
                    Ok(Some(file))
                }
                _ => Err(private_document::conflict()),
            }
        })()
        .map_err(NativeDocumentJournalFailure::unknown)?;
        let (scratch, mut file) =
            CreatedScratch::create(scratch_path, &check, private_document::create_file)
                .map_err(NativeDocumentJournalFailure::unknown)?;
        let prepared = (|| {
            #[cfg(test)]
            test_hooks::run(&test_hooks::AFTER_BIRTH);
            scratch.verify(&file, &check)?;
            let identity = private_document::replacement_identity(
                scratch.identity(),
                expected
                    .value
                    .as_ref()
                    .map(|value| &value.metadata.identity),
            )?;
            let envelope = Envelope {
                parent_identity: self.namespace.directory_identity(),
                namespace_identity: self.namespace.namespace.directory_identity(),
                physical_id: identity,
                body,
            };
            let bytes = encode(&envelope, max_bytes, true)?
                .bytes
                .ok_or(io::ErrorKind::InvalidData)?;
            write_bytes(&mut file, &bytes, self, &scratch)?;
            file.sync_all()?;
            scratch.verify(&file, &check)?;
            file.rewind()?;
            private_document::compare_bytes(&mut file, &bytes, &access)?;
            scratch.verify(&file, &check)?;
            if let Some(mut old) = old {
                let expected = expected.value.as_ref().ok_or(io::ErrorKind::InvalidData)?;
                private_document::compare_bytes(&mut old, &expected.bytes, &access)?;
                if private_document::observe(&destination, &old, &access)? != expected.metadata {
                    return Err(private_document::conflict());
                }
            }
            Ok((envelope.physical_id, bytes))
        })();
        drop(file);
        let prepared = prepared.and_then(|(identity, bytes)| {
            #[cfg(test)]
            test_hooks::run(&test_hooks::BEFORE_COMPARE);
            private_document::verify_closed(&destination, expected.value.as_ref(), &access)?;
            let mut stage = super::open_file(scratch.path())?;
            scratch.verify(&stage, &check)?;
            private_document::compare_bytes(&mut stage, &bytes, &access)?;
            scratch.verify(&stage, &check)?;
            drop(stage);
            self.revalidate()?;
            Ok((identity, bytes))
        });
        let (identity, bytes) = match prepared {
            Ok(value) => value,
            Err(error) => {
                return Err(failure(
                    error,
                    None,
                    scratch.cleanup(&check).err(),
                    NativeDocumentJournalEffect::Unknown,
                ));
            }
        };
        #[cfg(test)]
        test_hooks::run(&test_hooks::BEFORE_NATIVE);
        if let Err(error) = self.revalidate() {
            return Err(failure(
                error,
                None,
                scratch.cleanup(&check).err(),
                NativeDocumentJournalEffect::Unknown,
            ));
        }
        let native = replace(scratch.path(), &destination, expected.value.is_some());
        #[cfg(test)]
        test_hooks::run(&test_hooks::AFTER_NATIVE);
        let observed = private_document::observe_effect(
            &destination,
            private_document::ExpectedEffect {
                identity: &identity,
                bytes: &bytes,
                old: expected.value.as_ref(),
                native_failed: native.is_err(),
                retain: true,
            },
            &access,
        )
        .and_then(|value| self.snapshot(value, max_bytes));
        let effect = match &observed {
            Ok(snapshot)
                if snapshot.value.as_ref().is_some_and(|value| {
                    value.metadata.identity == identity
                        && value.bytes.as_slice() == bytes.as_slice()
                }) =>
            {
                NativeDocumentJournalEffect::Applied
            }
            Ok(_) => NativeDocumentJournalEffect::Unchanged,
            Err(_) => NativeDocumentJournalEffect::Unknown,
        };
        let cleanup = scratch.cleanup(&check);
        match (native, observed, cleanup) {
            (Ok(()), Ok(snapshot), Ok(())) => Ok(snapshot),
            (Err(primary), observed, cleanup) => {
                Err(failure(primary, observed.err(), cleanup.err(), effect))
            }
            (Ok(()), Err(primary), cleanup) => Err(failure(primary, None, cleanup.err(), effect)),
            (Ok(()), Ok(_), Err(primary)) => Err(failure(primary, None, None, effect)),
        }
    }
}
fn write_bytes(
    file: &mut File,
    bytes: &[u8],
    guard: &NativeDocumentNamespaceGuard<'_>,
    scratch: &CreatedScratch,
) -> io::Result<()> {
    for chunk in bytes.chunks(64 * 1024) {
        let mut left = chunk;
        while !left.is_empty() {
            scratch.verify(file, &|| guard.revalidate())?;
            let written = write_chunk(file, left)?;
            if written == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            left = &left[written..];
            #[cfg(test)]
            test_hooks::run(&test_hooks::AFTER_WRITE);
        }
    }
    Ok(())
}
fn write_chunk(file: &mut File, bytes: &[u8]) -> io::Result<usize> {
    #[cfg(test)]
    if let Some(write) = test_hooks::WRITE.with(|hook| hook.borrow_mut().take()) {
        return write(file, bytes);
    }
    file.write(bytes)
}
fn failure(
    primary: io::Error,
    secondary: Option<io::Error>,
    cleanup: Option<io::Error>,
    effect: NativeDocumentJournalEffect,
) -> NativeDocumentJournalFailure {
    NativeDocumentJournalFailure {
        primary: primary.kind(),
        secondary: secondary.or(cleanup).map(|error| error.kind()),
        effect,
    }
}
fn replace(stage: &Path, destination: &Path, exists: bool) -> io::Result<()> {
    #[cfg(test)]
    if let Some(replace) = test_hooks::NATIVE.with(|hook| hook.borrow_mut().take()) {
        return replace(stage, destination, exists);
    }
    private_document::replace(stage, destination, exists)
}
#[cfg(test)]
mod test_hooks {
    use super::*;
    use std::{cell::RefCell, thread::LocalKey};
    type Hook = RefCell<Option<Box<dyn FnOnce()>>>;
    type WriteHook = RefCell<Option<Box<dyn FnOnce(&mut File, &[u8]) -> io::Result<usize>>>>;
    type Native = RefCell<Option<Box<dyn FnOnce(&Path, &Path, bool) -> io::Result<()>>>>;
    thread_local! {
        pub(super) static WRITE: WriteHook = const { RefCell::new(None) };
        pub(super) static AFTER_WRITE: Hook = const { RefCell::new(None) };
        pub(super) static AFTER_READ: Hook = const { RefCell::new(None) };
        pub(super) static AFTER_BIRTH: Hook = const { RefCell::new(None) };
        pub(super) static BEFORE_COMPARE: Hook = const { RefCell::new(None) };
        pub(super) static BEFORE_NATIVE: Hook = const { RefCell::new(None) };
        pub(super) static AFTER_NATIVE: Hook = const { RefCell::new(None) };
        pub(super) static NATIVE: Native = const { RefCell::new(None) };
    }
    pub(super) fn run(hook: &'static LocalKey<Hook>) {
        if let Some(action) = hook.with(|hook| hook.borrow_mut().take()) {
            action();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{
        artifact_store::{CapturedPublicationScope, ScopeLimits},
        brand::Brand,
        file_io::SnapshotContext,
        file_lock::Cancellation,
        installation_identity::InstallationId,
        native_document_file::NativeDocumentFileLimits,
        project_paths::InstallationPaths,
    };
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        time::{Duration, Instant},
    };
    fn scope(root: &Path) -> CapturedPublicationScope {
        if !root.join("owner").exists() {
            fs::create_dir(root.join("owner")).unwrap();
        }
        let installation =
            InstallationPaths::resolve(Brand::default(), root, None, Some(&root.join("owner")))
                .unwrap();
        CapturedPublicationScope::installation(
            &installation,
            InstallationId::new("journal-unit").unwrap(),
            [root.to_owned()],
            ScopeLimits::new(4, 65536, 65536).unwrap(),
            &native_capture_request(),
        )
        .unwrap()
    }
    fn fixture() -> (
        tempfile::TempDir,
        PathBuf,
        CapturedPublicationScope,
        SnapshotContext,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(tmp.path()).unwrap();
        let scope = scope(&root);
        (tmp, root, scope, context(Cancellation::default()))
    }
    fn context(cancel: Cancellation) -> SnapshotContext {
        SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel)
    }
    fn capture(
        scope: &CapturedPublicationScope,
        root: &Path,
        ctx: &SnapshotContext,
    ) -> NativeDocumentNamespace {
        NativeDocumentNamespace::capture(
            scope,
            root,
            NativeDocumentFileLimits::new(1, 65536, 256, 65536).unwrap(),
            ctx,
        )
        .unwrap()
    }
    fn raw(text: &str) -> &RawValue {
        serde_json::from_str(text).unwrap()
    }
    fn directory(root: &Path) -> PathBuf {
        root.join(".graphit-provider-documents.state")
    }
    fn write_bounds(cap: usize) -> NativeDocumentJournalWriteBounds {
        NativeDocumentJournalWriteBounds::new(cap, cap, cap).unwrap()
    }
    fn scratch(root: &Path) -> PathBuf {
        fs::read_dir(directory(root))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .as_encoded_bytes()
                    .starts_with(b".documents-")
            })
            .unwrap()
    }
    #[test]
    fn journal_create_and_replace_return_current_body() {
        let (_tmp, root, scope, ctx) = fixture();
        let ns = capture(&scope, &root, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        let absent = guard.read_journal(4096).unwrap();
        let body = raw("{\"revision\":1}");
        let desired = worst(body, 4096).unwrap();
        let permit = NativeDocumentJournalWriteBounds::new(4096, 0, desired).unwrap();

        let first = guard.replace_journal(&absent, body, permit).unwrap();

        guard
            .replace_journal(&first, raw("{\"revision\":22}"), write_bounds(4096))
            .unwrap();
        assert_eq!(
            guard
                .read_journal(4096)
                .unwrap()
                .body()
                .unwrap()
                .unwrap()
                .get(),
            "{\"revision\":22}"
        );
    }
    #[test]
    fn native_error_after_actual_effect_keeps_applied_proof_and_static_failure() {
        let (_tmp, root, scope, ctx) = fixture();
        let ns = capture(&scope, &root, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        let absent = guard.read_journal(4096).unwrap();

        let permit = write_bounds(4096);

        test_hooks::NATIVE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(|stage, destination, exists| {
                private_document::replace(stage, destination, exists)?;
                Err(io::ErrorKind::PermissionDenied.into())
            }))
        });
        let error = guard
            .replace_journal(&absent, raw("{\"launch\":\"sensitive-test-only\"}"), permit)
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.effect(), NativeDocumentJournalEffect::Applied);
        assert_eq!(error.secondary_kind(), None);
        assert!(!format!("{error:?}").contains("sensitive-test-only"));
        assert!(
            guard
                .read_journal(4096)
                .unwrap()
                .body()
                .unwrap()
                .unwrap()
                .get()
                .contains("sensitive-test-only")
        );

        assert_eq!(fs::read_dir(directory(&root)).unwrap().count(), 2);
    }
    #[test]
    fn native_error_without_effect_observes_unchanged_and_only_cleans_owned_scratch() {
        let (_tmp, root, scope, ctx) = fixture();
        let ns = capture(&scope, &root, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        let absent = guard.read_journal(4096).unwrap();
        let old = guard
            .replace_journal(&absent, raw("{\"revision\":1}"), write_bounds(4096))
            .unwrap();
        let before = fs::read(directory(&root).join(JOURNAL)).unwrap();
        test_hooks::NATIVE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(|_, _, _| Err(io::ErrorKind::Other.into())))
        });
        let error = guard
            .replace_journal(&old, raw("{\"revision\":2}"), write_bounds(4096))
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.effect(), NativeDocumentJournalEffect::Unchanged);
        assert_eq!(error.secondary_kind(), None);
        assert_eq!(fs::read(directory(&root).join(JOURNAL)).unwrap(), before);
        assert_eq!(fs::read_dir(directory(&root)).unwrap().count(), 2);
    }
    #[test]
    fn cancellation_after_native_effect_is_unknown_and_fresh_request_reads_actual_state() {
        let (_tmp, root, scope, _) = fixture();
        let cancel = Cancellation::default();
        let ctx = context(cancel.clone());
        let ns = capture(&scope, &root, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        let absent = guard.read_journal(4096).unwrap();
        test_hooks::AFTER_NATIVE
            .with(|hook| *hook.borrow_mut() = Some(Box::new(move || cancel.cancel())));
        let error = guard
            .replace_journal(&absent, raw("{\"revision\":1}"), write_bounds(4096))
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(error.effect(), NativeDocumentJournalEffect::Unknown);
        drop(guard);
        let fresh = context(Cancellation::default());
        let ns = capture(&scope, &root, &fresh);
        let guard = ns.lock(&fresh).unwrap();
        assert_eq!(
            guard
                .read_journal(4096)
                .unwrap()
                .body()
                .unwrap()
                .unwrap()
                .get(),
            "{\"revision\":1}"
        );
        assert_eq!(fs::read_dir(directory(&root)).unwrap().count(), 2);
    }
    #[test]
    fn cancellation_after_birth_preserves_unknown_private_residue_without_drop_cleanup() {
        let (_tmp, root, scope, _) = fixture();
        let cancel = Cancellation::default();
        let ctx = context(cancel.clone());
        let ns = capture(&scope, &root, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        let absent = guard.read_journal(4096).unwrap();
        test_hooks::AFTER_BIRTH
            .with(|hook| *hook.borrow_mut() = Some(Box::new(move || cancel.cancel())));
        assert_eq!(
            guard
                .replace_journal(&absent, raw("{}"), write_bounds(4096))
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        let path = scratch(&root);
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);

        drop(guard);
        let fresh = context(Cancellation::default());
        let ns = capture(&scope, &root, &fresh);
        let guard = ns.lock(&fresh).unwrap();
        assert!(guard.read_journal(4096).unwrap().body().unwrap().is_none());
        assert!(path.exists());
        assert_eq!(fs::read_dir(directory(&root)).unwrap().count(), 2);
    }
    #[test]
    fn external_edit_before_final_comparison_preserves_exact_bytes_and_no_publication() {
        let (_tmp, root, scope, ctx) = fixture();
        let ns = capture(&scope, &root, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        let absent = guard.read_journal(4096).unwrap();
        let old = guard
            .replace_journal(&absent, raw("{\"revision\":1}"), write_bounds(4096))
            .unwrap();
        let destination = directory(&root).join(JOURNAL);
        let external = String::from_utf8(fs::read(&destination).unwrap())
            .unwrap()
            .replace("\"revision\":1", "\"revision\":3");
        let changed = external.clone();
        test_hooks::BEFORE_COMPARE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || fs::write(destination, changed).unwrap()))
        });
        let error = guard
            .replace_journal(&old, raw("{\"revision\":2}"), write_bounds(4096))
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            fs::read(directory(&root).join(JOURNAL)).unwrap(),
            external.as_bytes()
        );
        assert_eq!(fs::read_dir(directory(&root)).unwrap().count(), 2);
    }
    #[test]
    fn every_short_write_checks_scratch_privacy_before_more_plaintext() {
        let (_tmp, root, scope, ctx) = fixture();
        let ns = capture(&scope, &root, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        let absent = guard.read_journal(128 * 1024).unwrap();
        let changed_root = root.clone();
        test_hooks::WRITE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(|file, bytes| file.write(&bytes[..1])))
        });
        test_hooks::AFTER_WRITE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::set_permissions(scratch(&changed_root), fs::Permissions::from_mode(0o644))
                    .unwrap()
            }))
        });
        let body = format!("{{\"value\":\"{}\"}}", "x".repeat(70 * 1024));
        let error = guard
            .replace_journal(&absent, raw(&body), write_bounds(128 * 1024))
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);

        assert_eq!(fs::metadata(scratch(&root)).unwrap().len(), 1);
        assert_eq!(
            fs::metadata(scratch(&root)).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert!(!directory(&root).join(JOURNAL).exists());
    }
    #[test]
    fn replacing_namespace_before_native_effect_preserves_foreign_state_and_old_scratch() {
        let (_tmp, root, scope, ctx) = fixture();
        let ns = capture(&scope, &root, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        let absent = guard.read_journal(4096).unwrap();
        let changed_root = root.clone();
        test_hooks::BEFORE_NATIVE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::rename(
                    directory(&changed_root),
                    changed_root.join("original-state"),
                )
                .unwrap();
                drop(graphit_fs::create_private_directory(&directory(&changed_root)).unwrap());
                fs::write(directory(&changed_root).join(JOURNAL), b"foreign sentinel").unwrap();
            }))
        });
        let error = guard
            .replace_journal(&absent, raw("{}"), write_bounds(4096))
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            fs::read(directory(&root).join(JOURNAL)).unwrap(),
            b"foreign sentinel"
        );
        assert_eq!(
            fs::read_dir(root.join("original-state")).unwrap().count(),
            2
        );
    }
    #[test]
    fn scratch_path_bound_precedes_filesystem_creation() {
        let (_tmp, root, scope, ctx) = fixture();
        let cap = directory(&root).as_os_str().as_encoded_bytes().len() + 1 + JOURNAL.len();
        let ns = NativeDocumentNamespace::capture(
            &scope,
            &root,
            NativeDocumentFileLimits::new(1, cap, 256, 65536).unwrap(),
            &ctx,
        )
        .unwrap();
        let guard = ns.lock(&ctx).unwrap();
        let absent = guard.read_journal(4096).unwrap();
        assert_eq!(
            guard
                .replace_journal(&absent, raw("{}"), write_bounds(4096))
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );

        assert_eq!(fs::read_dir(directory(&root)).unwrap().count(), 1);
    }
    #[test]
    fn journal_crash_child() {
        let Some(path) = std::env::var_os("GRAPHIT_JOURNAL_CRASH_ROOT") else {
            return;
        };
        let root = PathBuf::from(path);
        let scope = scope(&root);
        let ctx = context(Cancellation::default());
        let ns = capture(&scope, &root, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        let absent = guard.read_journal(4096).unwrap();
        test_hooks::AFTER_BIRTH
            .with(|hook| *hook.borrow_mut() = Some(Box::new(|| std::process::exit(71))));
        let _ = guard.replace_journal(&absent, raw("{}"), write_bounds(4096));
        panic!("child did not exit at actual SDK birth");
    }
    #[test]
    fn actual_process_death_preserves_unknown_scratch_and_releases_kernel_lock() {
        let (_tmp, root, scope, ctx) = fixture();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_document_file::journal::tests::journal_crash_child",
                "--nocapture",
            ])
            .env("GRAPHIT_JOURNAL_CRASH_ROOT", &root)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(71));
        let path = scratch(&root);
        let ns = capture(&scope, &root, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        assert!(guard.read_journal(4096).unwrap().body().unwrap().is_none());
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        assert_eq!(fs::read_dir(directory(&root)).unwrap().count(), 2);
    }
    #[test]
    fn full_snapshot_bindings_reject_before_body_io_or_sdk_birth() {
        let (_tmp, root, scope, ctx) = fixture();
        let ns = capture(&scope, &root, &ctx);
        let other_path = root.join("other");
        fs::create_dir(&other_path).unwrap();
        let other = capture(&scope, &other_path, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        let foreign = context(Cancellation::default());
        for field in 0..3 {
            let mut expected = guard.read_journal(4096).unwrap();
            match field {
                0 => expected.parent = other.directory_identity(),
                1 => expected.namespace = other.namespace.directory_identity(),
                _ => expected.request = foreign.request_identity(),
            }
            let permit = write_bounds(4096);

            assert_eq!(
                guard
                    .replace_journal(&expected, raw("{}"), permit)
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::InvalidInput
            );

            assert!(!directory(&root).join(JOURNAL).exists());
            assert_eq!(fs::read_dir(directory(&root)).unwrap().count(), 1);
        }
    }
    #[test]
    fn journal_reader_rejects_replacement_during_read() {
        let (_tmp, root, scope, ctx) = fixture();
        let ns = capture(&scope, &root, &ctx);
        let guard = ns.lock(&ctx).unwrap();
        let missing = guard.read_journal(4096).unwrap();
        guard
            .replace_journal(&missing, raw("{}"), write_bounds(4096))
            .unwrap();
        let destination = directory(&root).join(JOURNAL);
        let retained = root.join("original");
        let bytes = fs::read(&destination).unwrap();
        let foreign_bytes = bytes.clone();
        let changed = destination.clone();
        test_hooks::AFTER_READ.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::rename(&changed, retained).unwrap();
                let mut replacement = private_document::create_file(&changed).unwrap();
                replacement.write_all(&foreign_bytes).unwrap();
            }))
        });

        assert_eq!(
            guard.read_journal(4096).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );

        assert_eq!(fs::read(destination).unwrap(), bytes);
    }
    fn native_capture_request() -> std::sync::Arc<crate::file_io::SnapshotContext> {
        std::sync::Arc::new(crate::file_io::SnapshotContext::new(
            std::time::Instant::now() + std::time::Duration::from_secs(30),
            crate::file_lock::Cancellation::default(),
        ))
    }
}
