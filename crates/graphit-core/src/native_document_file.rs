//! Native document observations and private parent coordination.
//! Observations and namespace guards grant no document publication ownership.
//!
//! Alias and handle checks detect observed changes under cooperative concurrency;
//! they do not turn pathname I/O into a hostile-filesystem security boundary.
use crate::{
    file_io::{NativeIoPass, RequestIdentity, SnapshotContext},
    native_file_identity::{PhysicalId, handle_id, physical_id},
    native_path::{MAX_PATH_BYTES, valid_path},
};
use secrecy::zeroize::Zeroizing;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read, Seek},
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

mod journal;
mod name;
mod namespace;
mod project;
mod user;
pub use user::CapturedUserDocumentDirectory;
mod read;
pub use project::{CapturedProjectDocumentDirectory, DirectoryPresence};
mod route;
pub use journal::{
    NativeDocumentJournalEffect, NativeDocumentJournalFailure, NativeDocumentJournalSnapshot,
    NativeDocumentJournalWriteBounds,
};
pub use name::{NamedNativeDocumentObservation, NativeDocumentName, NativeDocumentNameLimits};
pub use namespace::{NativeDocumentNamespace, NativeDocumentNamespaceGuard};
pub use route::{
    NativeDocumentRouteProbeBatch, NativeDocumentRouteProbeLimits, NativeDocumentRouteProbes,
    NativeDocumentRouteRelation,
};

/// Finite per-document and captured-directory limits; no product defaults.
#[derive(Clone, Copy)]
pub struct NativeDocumentFileLimits {
    bytes: usize,
    path_bytes: usize,
    ancestors: usize,
    access_bytes: usize,
}
impl NativeDocumentFileLimits {
    pub(crate) fn child_path_cost(&self, parent: &Path, suffix: &str) -> io::Result<usize> {
        let extra = suffix
            .len()
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        let extra = if cfg!(windows) {
            extra.checked_mul(2).ok_or(io::ErrorKind::FileTooLarge)?
        } else {
            extra
        };
        let cost = self
            .path_cost(parent)?
            .checked_add(extra)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if cost > self.path_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(cost)
    }
    pub(crate) fn check_stage(&self, length: u64) -> io::Result<()> {
        self.check_bytes(length)?;
        #[cfg(target_os = "macos")]
        self.access_bytes
            .checked_add(52)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        Ok(())
    }

    pub fn new(
        bytes: usize,
        path_bytes: usize,
        ancestors: usize,
        access_bytes: usize,
    ) -> io::Result<Self> {
        if [bytes, path_bytes, ancestors, access_bytes].contains(&0)
            || path_bytes > MAX_PATH_BYTES
            || bytes
                .checked_add(1)
                .and_then(|n| u64::try_from(n).ok())
                .is_none()
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            bytes,
            path_bytes,
            ancestors,
            access_bytes,
        })
    }
    fn path(&self, path: &Path) -> io::Result<usize> {
        valid_path(path)?;
        platform_path(path)?;
        let bytes = native_bytes(path.as_os_str())?;
        if bytes > self.path_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(bytes)
    }
}

struct DirectoryHandle {
    path: PathBuf,
    file: File,
    identity: PhysicalId,
}

/// Read-only physical directory identity for journal keys and cooperative lock order.
///
/// Deserialization restores data, never a handle, observation or write capability.
/// A captured identity is historical: revalidate its directory before trusting a path.
///
/// ```compile_fail
/// use graphit_core::native_document_file::{
///     CapturedDocumentDirectory, NativeDocumentDirectoryIdentity,
/// };
/// fn restore(data: NativeDocumentDirectoryIdentity) -> CapturedDocumentDirectory {
///     data.into()
/// }
/// ```
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "DirectoryIdentityWire")]
pub struct NativeDocumentDirectoryIdentity {
    identity: PhysicalId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectoryIdentityWire {
    identity: PhysicalId,
}

impl TryFrom<DirectoryIdentityWire> for NativeDocumentDirectoryIdentity {
    type Error = io::Error;
    fn try_from(value: DirectoryIdentityWire) -> io::Result<Self> {
        value.identity.validate()?;
        Ok(Self {
            identity: value.identity,
        })
    }
}

impl NativeDocumentDirectoryIdentity {
    /// Fixed, injective native identity encoding; no path, brand or content hash.
    ///
    /// Tag 1 is Unix device/inode; tag 2 is Windows volume/index/creation time.
    /// Integer fields use big endian; Windows index bytes stay native. Unix leaves sixteen bytes zero.
    /// Byte order matches this record's total order. The key grants no authority.
    pub fn routing_key(&self) -> [u8; 33] {
        let mut key = [0u8; 33];
        match &self.identity {
            PhysicalId::Unix { device, inode } => {
                key[0] = 1;
                key[1..9].copy_from_slice(&device.to_be_bytes());
                key[9..17].copy_from_slice(&inode.to_be_bytes());
            }
            PhysicalId::Windows {
                volume,
                index,
                created,
            } => {
                key[0] = 2;
                key[1..9].copy_from_slice(&volume.to_be_bytes());
                key[9..25].copy_from_slice(index);
                key[25..33].copy_from_slice(&created.to_be_bytes());
            }
        }
        key
    }
}

#[cfg(test)]
mod directory_identity_tests {
    use super::*;

    #[test]
    fn both_os_routing_encodings_preserve_every_field_and_order() {
        // Pure encoding fixture, not native Windows filesystem qualification.
        let mut tail = [1; 16];
        tail[15] = 2;
        let ids = [
            PhysicalId::Unix {
                device: 0,
                inode: 1,
            },
            PhysicalId::Unix {
                device: 0,
                inode: u64::MAX,
            },
            PhysicalId::Unix {
                device: u64::MAX,
                inode: 1,
            },
            PhysicalId::Windows {
                volume: 0,
                index: [1; 16],
                created: 1,
            },
            PhysicalId::Windows {
                volume: 0,
                index: [1; 16],
                created: u64::MAX,
            },
            PhysicalId::Windows {
                volume: 0,
                index: [255; 16],
                created: 1,
            },
            PhysicalId::Windows {
                volume: u64::MAX,
                index: [1; 16],
                created: 1,
            },
            PhysicalId::Windows {
                volume: 0,
                index: tail,
                created: 1,
            },
        ]
        .map(|identity| NativeDocumentDirectoryIdentity { identity });
        for (i, first) in ids.iter().enumerate() {
            for (j, second) in ids.iter().enumerate() {
                assert_eq!(first.routing_key() == second.routing_key(), i == j);
                assert_eq!(
                    first.cmp(second),
                    first.routing_key().cmp(&second.routing_key())
                );
            }
        }
        let mut unix = [0; 33];
        unix[0] = 1;
        unix[16] = 1;
        assert_eq!(ids[0].routing_key(), unix);
        let mut windows = [0; 33];
        windows[0] = 2;
        windows[9..25].fill(1);
        windows[32] = 1;
        assert_eq!(ids[3].routing_key(), windows);
        assert_eq!(&ids[4].routing_key()[25..33], &[255; 8]);
        assert_eq!(&ids[5].routing_key()[9..25], &[255; 16]);
        assert_eq!(&ids[6].routing_key()[1..9], &[255; 8]);
        assert_eq!(&ids[3].routing_key()[9..17], &ids[7].routing_key()[9..17]);
        assert_ne!(ids[3].routing_key(), ids[7].routing_key());
    }
}

/// Captured original alias and canonical directory with native namespace observations.
/// Unix retains the selected directory; Windows also pins no-delete ancestors.
/// Construction is read-only and does not establish ownership of any file.
///
/// ```compile_fail
/// use graphit_core::native_document_file::CapturedDocumentDirectory;
/// let _: CapturedDocumentDirectory = serde_json::from_str("{}").unwrap();
/// ```
pub struct CapturedDocumentDirectory {
    input: PathBuf,
    canonical: PathBuf,
    ancestors: Vec<DirectoryHandle>,
    #[cfg(unix)]
    ancestor_bindings: Vec<(PathBuf, PhysicalId)>,
    limits: NativeDocumentFileLimits,
}
impl CapturedDocumentDirectory {
    pub(crate) fn check_child_path(
        parent: &Path,
        child: &str,
        limits: NativeDocumentFileLimits,
    ) -> io::Result<()> {
        limits.child_path_cost(parent, child)?;
        let count = parent
            .ancestors()
            .count()
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if count > limits.ancestors {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(())
    }

    pub fn capture(
        input: &Path,
        limits: NativeDocumentFileLimits,
        context: &SnapshotContext,
    ) -> io::Result<Self> {
        context.check()?;
        limits.path(input)?;
        let canonical = fs::canonicalize(input)?;
        limits.path(&canonical)?;
        let count = canonical.ancestors().count();
        if count > limits.ancestors {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut ancestors = Vec::new();
        ancestors
            .try_reserve_exact(if cfg!(unix) { 1 } else { count })
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        #[cfg(unix)]
        let mut ancestor_bindings = Vec::new();
        #[cfg(unix)]
        ancestor_bindings
            .try_reserve_exact(count.saturating_sub(1))
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        for path in canonical.ancestors() {
            context.check()?;
            let metadata = directory_metadata(path)?;
            let expected = physical_id(path, &metadata)?;
            let file = open_directory(path)?;
            let identity = handle_id(&file)?;
            identity.validate()?;
            if identity != expected
                || !file.metadata()?.is_dir()
                || physical_id(path, &directory_metadata(path)?)? != identity
            {
                return Err(conflict());
            }
            #[cfg(unix)]
            if !ancestors.is_empty() {
                ancestor_bindings.push((path.to_owned(), identity));
                continue;
            }
            ancestors.push(DirectoryHandle {
                path: path.to_owned(),
                file,
                identity,
            });
        }
        let captured = Self {
            input: input.to_owned(),
            canonical,
            ancestors,
            #[cfg(unix)]
            ancestor_bindings,
            limits,
        };
        captured.revalidate(context)?;
        Ok(captured)
    }

    pub fn canonical_path(&self) -> &Path {
        &self.canonical
    }

    /// Returns captured identity data without I/O or granting a handle/capability.
    /// Revalidate this directory separately to detect subsequent path changes.
    pub fn directory_identity(&self) -> NativeDocumentDirectoryIdentity {
        NativeDocumentDirectoryIdentity {
            identity: self.identity().clone(),
        }
    }

    /// Physical identity comparison only. It grants no mutation capability.
    pub fn same_directory(&self, other: &Self) -> bool {
        self.ancestors[0].identity == other.ancestors[0].identity
    }

    pub fn revalidate(&self, context: &SnapshotContext) -> io::Result<()> {
        self.revalidate_checked(|| context.check())
    }

    /// Opens a handle for read-only qualification of this captured directory's privacy.
    ///
    /// Rechecks the captured path/identity and the supplied request around the existing
    /// OS privacy check. Windows opens with READ_CONTROL; normal directory capture
    /// retains its existing access policy. Does not repair permissions, grant authority,
    /// establish private birth, or provide atomic confinement against path replacement.
    pub fn private_handle(&self, context: &SnapshotContext) -> io::Result<File> {
        self.revalidate(context)?;
        let file = open_access_directory(&self.canonical)?;
        self.verify_private_handle(&file, context)?;
        Ok(file)
    }

    /// Verifies a retained handle against this captured private directory.
    ///
    /// Revalidates the capture and original request around physical identity and
    /// OS privacy checks. It neither adopts a replacement nor repairs permissions;
    /// the supplied handle alone grants no authority or private-birth ownership.
    pub fn verify_private_handle(&self, file: &File, context: &SnapshotContext) -> io::Result<()> {
        self.revalidate(context)?;
        if handle_id(file)? != *self.identity() {
            return Err(conflict());
        }
        graphit_fs::verify_private_directory(file)?;
        self.revalidate(context)?;
        Ok(())
    }

    pub(crate) fn revalidate_io(&self, pass: &NativeIoPass) -> io::Result<()> {
        self.revalidate_checked(|| pass.check())
    }

    fn revalidate_checked(&self, check: impl Fn() -> io::Result<()>) -> io::Result<()> {
        check()?;
        if fs::canonicalize(&self.input)? != self.canonical {
            return Err(conflict());
        }
        for ancestor in &self.ancestors {
            check()?;
            if handle_id(&ancestor.file)? != ancestor.identity
                || !ancestor.file.metadata()?.is_dir()
                || physical_id(&ancestor.path, &directory_metadata(&ancestor.path)?)?
                    != ancestor.identity
            {
                return Err(conflict());
            }
        }
        #[cfg(unix)]
        for (path, identity) in &self.ancestor_bindings {
            check()?;
            let file = open_directory(path)?;
            if handle_id(&file)? != *identity
                || !file.metadata()?.is_dir()
                || physical_id(path, &directory_metadata(path)?)? != *identity
            {
                return Err(conflict());
            }
        }
        check()
    }

    /// Bounded exact-handle read with a one-byte growth probe. Missing creates nothing.
    pub fn read(
        &self,
        name: &OsStr,
        context: &SnapshotContext,
    ) -> io::Result<NativeDocumentObservation> {
        self.read_inner(name, None, context, self.limits.bytes)
            .map(|(observation, _)| observation)
    }

    fn read_inner(
        &self,
        name: &OsStr,
        name_limits: Option<NativeDocumentNameLimits>,
        context: &SnapshotContext,
        content_cap: usize,
    ) -> io::Result<(NativeDocumentObservation, Option<NativeDocumentName>)> {
        let path = self.path(name)?;
        self.revalidate(context)?;
        let mut native_name = None;
        let value = match regular_metadata(&path)? {
            None => None,
            Some(metadata) => {
                let expected = physical_id(&path, &metadata).map_err(existing_change)?;
                #[cfg(test)]
                test_hooks::run(&test_hooks::BEFORE_OPEN);
                context.check()?;
                let mut file = open_file(&path).map_err(existing_change)?;
                let before = observe_handle(&file, self.limits.access_bytes)?;
                if before.identity != expected {
                    return Err(observation_changed(None));
                }
                let body = read::body(&mut file, before.length, self.limits, content_cap, context)?;
                if let Some(limits) = name_limits {
                    native_name = Some(self.observe_name(&path, &file, &before, limits, context)?);
                }
                let after = observe_handle(&file, self.limits.access_bytes)?;
                if after.access != before.access {
                    return Err(conflict());
                }
                if after != before
                    || physical_id(
                        &path,
                        &regular_metadata(&path)?.ok_or_else(|| observation_changed(None))?,
                    )
                    .map_err(existing_change)?
                        != before.identity
                {
                    return Err(observation_changed(None));
                }
                Some(ObservedFile {
                    receipt: NativeDocumentReceipt {
                        file_id: before.identity.clone(),
                        length: before.length,
                        content: body.digest,
                        access: before.access.fingerprint(),
                    },
                    metadata: before,
                    bytes: body.bytes,
                })
            }
        };
        self.revalidate(context)?;
        Ok((
            NativeDocumentObservation {
                request: context.request_identity(),
                directory: self.ancestors[0].identity.clone(),
                name: name.to_owned(),
                value,
                _project: None,
            },
            native_name,
        ))
    }

    /// Fresh full observation, including exact bytes. No cached or hash-only proof.
    pub fn revalidate_observation(
        &self,
        observation: &NativeDocumentObservation,
        context: &SnapshotContext,
    ) -> io::Result<()> {
        if observation._project.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if observation.directory != self.ancestors[0].identity {
            return Err(conflict());
        }
        let fresh = self.read(&observation.name, context)?;
        if let (Some(old), Some(current)) = (&observation.value, &fresh.value)
            && old.metadata.access != current.metadata.access
        {
            return Err(conflict());
        }
        if fresh.value != observation.value {
            return Err(observation_changed(None));
        }
        Ok(())
    }

    pub(crate) fn path(&self, name: &OsStr) -> io::Result<PathBuf> {
        name::validate_name(name)?;
        let size = native_bytes(self.canonical.as_os_str())?
            .checked_add(native_bytes(name)?)
            .and_then(|n| n.checked_add(if cfg!(windows) { 2 } else { 1 }))
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if size > self.limits.path_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let path = self.canonical.join(name);
        self.limits.path(&path)?;
        Ok(path)
    }
}

/// Private fields and no decoder: bytes and observations are data, never ownership.
///
/// ```compile_fail
/// use graphit_core::native_document_file::NativeDocumentObservation;
/// let _: NativeDocumentObservation = serde_json::from_str("{}").unwrap();
/// ```
pub struct NativeDocumentObservation {
    request: RequestIdentity,
    directory: PhysicalId,
    name: OsString,
    value: Option<ObservedFile>,
    _project: Option<std::sync::Arc<crate::artifact_store::native_source::chain::CapturedChain>>,
}
impl NativeDocumentObservation {
    pub fn bytes(&self) -> Option<&[u8]> {
        self.value.as_ref().map(|v| v.bytes.as_slice())
    }

    /// Observed metadata for durable data records; never a mutation capability.
    pub fn receipt(&self) -> Option<&NativeDocumentReceipt> {
        self.value.as_ref().map(|v| &v.receipt)
    }
}

/// Closed file metadata data, without bytes, paths or filesystem authority.
///
/// The parent, name, scope and operation must be bound separately by a controlled
/// producer. Deserializing or comparing this record cannot create an observation,
/// stage, publication or cleanup capability. Same-inode edits and same-user
/// forgery are not authenticated by these fingerprints.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ReceiptWire")]
pub struct NativeDocumentReceipt {
    file_id: PhysicalId,
    length: u64,
    content: [u8; 32],
    access: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptWire {
    file_id: PhysicalId,
    length: u64,
    content: [u8; 32],
    access: [u8; 32],
}

impl TryFrom<ReceiptWire> for NativeDocumentReceipt {
    type Error = io::Error;
    fn try_from(value: ReceiptWire) -> io::Result<Self> {
        value.file_id.validate()?;
        value.length.checked_add(1).ok_or_else(conflict)?;
        Ok(Self {
            file_id: value.file_id,
            length: value.length,
            content: value.content,
            access: value.access,
        })
    }
}

impl NativeDocumentReceipt {
    /// Compare only the full native file identities stored in these data records.
    ///
    /// This performs no I/O or allocation and ignores length, content and access.
    /// It does not prove origin, ownership, current state or permission to mutate.
    /// Unix device/inode values can be reused; Windows includes creation time.
    /// A replacement by the caller's own operation still needs its Applied proof.
    pub fn same_file_as(&self, other: &Self) -> bool {
        self.file_id == other.file_id
    }
}

#[cfg(test)]
mod receipt_identity_tests {
    use super::*;

    fn receipt(file_id: PhysicalId) -> NativeDocumentReceipt {
        NativeDocumentReceipt {
            file_id,
            length: 1,
            content: [1; 32],
            access: [2; 32],
        }
    }

    #[test]
    fn unix_identity_compares_both_fields() {
        let original = receipt(PhysicalId::Unix {
            device: 7,
            inode: 11,
        });
        assert!(original.same_file_as(&original.clone()));
        for (device, inode) in [(8, 11), (7, 12)] {
            assert!(!original.same_file_as(&receipt(PhysicalId::Unix { device, inode })));
        }
    }

    #[test]
    fn windows_identity_compares_all_fields_without_claiming_native_execution() {
        let original = receipt(PhysicalId::Windows {
            volume: 7,
            index: [11; 16],
            created: 13,
        });
        assert!(original.same_file_as(&original.clone()));
        for (volume, index, created) in [(8, 11, 13), (7, 12, 13), (7, 11, 14)] {
            assert!(!original.same_file_as(&receipt(PhysicalId::Windows {
                volume,
                index: [index; 16],
                created,
            })));
        }
        assert!(!original.same_file_as(&receipt(PhysicalId::Unix {
            device: 7,
            inode: 11,
        })));
    }

    #[test]
    fn identity_ignores_other_fields_while_full_equality_preserves_them() {
        let original = receipt(PhysicalId::Unix {
            device: 7,
            inode: 11,
        });
        let mut changed_length = original.clone();
        changed_length.length = 2;
        let mut changed_content = original.clone();
        changed_content.content = [3; 32];
        let mut changed_access = original.clone();
        changed_access.access = [4; 32];
        for changed in [changed_length, changed_content, changed_access] {
            assert!(original.same_file_as(&changed));
            assert!(original != changed);
        }
    }
}

#[derive(PartialEq, Eq)]
struct ObservedFile {
    metadata: FileObservation,
    bytes: Zeroizing<Vec<u8>>,
    receipt: NativeDocumentReceipt,
}
#[derive(PartialEq, Eq)]
pub(crate) struct FileObservation {
    pub(crate) identity: PhysicalId,
    access: Access,
    pub(crate) length: u64,
    modified: SystemTime,
}
#[derive(PartialEq, Eq)]
enum Access {
    #[cfg(target_os = "linux")]
    Linux(graphit_fs::LinuxAccessSnapshot),
    #[cfg(all(unix, not(target_os = "linux")))]
    Unix {
        mode: u32,
        uid: u32,
        gid: u32,
        #[cfg(target_os = "macos")]
        mac_access: graphit_fs::MacAccessSnapshot,
    },
    #[cfg(windows)]
    Windows(graphit_fs::WindowsDocumentAccess),
    #[cfg(windows)]
    WindowsProject(graphit_fs::WindowsDocumentAccess),
}
impl Access {
    fn fingerprint(&self) -> [u8; 32] {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(access) => access.fingerprint(),
            #[cfg(all(unix, not(target_os = "linux")))]
            Self::Unix {
                mode,
                uid,
                gid,
                #[cfg(target_os = "macos")]
                mac_access,
            } => {
                let mut digest = Sha256::new();
                digest.update(b"graphit/native-access/unix\0");
                digest.update(mode.to_le_bytes());
                digest.update(uid.to_le_bytes());
                digest.update(gid.to_le_bytes());
                #[cfg(target_os = "macos")]
                digest.update(mac_access.fingerprint());
                digest.finalize().into()
            }
            #[cfg(windows)]
            Self::Windows(access) | Self::WindowsProject(access) => {
                let mut digest = Sha256::new();
                // Access data is independent of the capture's capability. Both
                // PROJECT reads and publication receipts observe the same bounded
                // owner/group/DACL descriptor, without rewriting or sorting ACLs.
                digest.update(b"graphit/native-access/windows-descriptor\0");
                let mut remaining = access.byte_length();
                for word in access.words() {
                    let bytes = word.to_ne_bytes();
                    let count = remaining.min(4);
                    digest.update(&bytes[..count]);
                    remaining -= count;
                }
                digest.finalize().into()
            }
        }
    }
}
pub(crate) fn observe_handle(file: &File, access_bytes: usize) -> io::Result<FileObservation> {
    observe_handle_checked(file, access_bytes, &mut || Ok(()))
}

/// Observe physical identity and access while checking the original request.
pub(crate) fn observe_handle_checked(
    file: &File,
    access_bytes: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<FileObservation> {
    before()?;
    let m = file.metadata()?;
    if !m.is_file() || m.permissions().readonly() {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    before()?;
    let identity = handle_id(file)?;
    identity.validate()?;
    #[cfg(target_os = "linux")]
    let access = {
        use std::os::unix::fs::MetadataExt;
        if m.nlink() == 0 {
            return Err(observation_changed(None));
        }
        if m.nlink() != 1 {
            return Err(conflict());
        }
        Access::Linux(
            graphit_fs::linux_access_snapshot_checked(file, access_bytes, before).map_err(
                |error| {
                    if error.kind() == io::ErrorKind::InvalidInput {
                        conflict()
                    } else {
                        error
                    }
                },
            )?,
        )
    };
    #[cfg(all(unix, not(target_os = "linux")))]
    let access = {
        use std::os::unix::fs::MetadataExt;
        if m.nlink() == 0 {
            return Err(observation_changed(None));
        }
        if m.nlink() != 1 {
            return Err(conflict());
        }
        if access_bytes < 12 {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Access::Unix {
            mode: m.mode() & 0o7777,
            uid: m.uid(),
            gid: m.gid(),
            #[cfg(target_os = "macos")]
            mac_access: graphit_fs::macos_access_snapshot(file, access_bytes - 12)?,
        }
    };
    #[cfg(windows)]
    let access = {
        let links = winapi_util::file::information(file)?.number_of_links();
        if links == 0 {
            return Err(observation_changed(None));
        }
        if links != 1 {
            return Err(conflict());
        }
        Access::Windows(graphit_fs::windows_document_access_snapshot(
            file,
            access_bytes,
            before,
        )?)
    };
    before()?;
    Ok(FileObservation {
        identity,
        access,
        length: m.len(),
        modified: m.modified()?,
    })
}
fn regular_metadata(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && !m.file_type().is_symlink() => Ok(Some(m)),
        Ok(_) => Err(conflict()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn directory_metadata(path: &Path) -> io::Result<Metadata> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || m.file_type().is_symlink() {
        return Err(conflict());
    }
    Ok(m)
}
fn open_directory(path: &Path) -> io::Result<File> {
    open_directory_with_access(path, false)
}

/// The private control boundary needs READ_CONTROL on its original handle.
/// Provider/ancestor capture keeps its existing zero-access Windows behavior.
pub(crate) fn open_access_directory(path: &Path) -> io::Result<File> {
    open_directory_with_access(path, true)
}

fn open_directory_with_access(path: &Path, inspect_access: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        let _ = inspect_access;
        use std::os::unix::fs::OpenOptionsExt;
        options.read(true).custom_flags(
            (rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::NOFOLLOW).bits() as i32,
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
        options
            .access_mode(if inspect_access { 0x0002_0000 } else { 0 })
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}
pub(crate) fn open_file(path: &Path) -> io::Result<File> {
    open_file_with_sharing(path, false)
}

pub(crate) fn open_file_with_sharing(path: &Path, allow_replacement: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        let _ = allow_replacement;
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
        };
        let share = if allow_replacement {
            FILE_SHARE_READ | FILE_SHARE_DELETE
        } else {
            FILE_SHARE_READ
        };
        options
            .share_mode(share)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}
fn native_bytes(value: &OsStr) -> io::Result<usize> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(value.as_bytes().len())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        value
            .encode_wide()
            .count()
            .checked_mul(2)
            .ok_or_else(|| io::ErrorKind::FileTooLarge.into())
    }
}
fn platform_path(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::{os::windows::ffi::OsStrExt, path::Prefix};
        if !matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_) | Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _)))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if path.components().any(
            |c| matches!(c, Component::Normal(n) if n.encode_wide().any(|v| v == u16::from(b':'))),
        ) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    #[cfg(unix)]
    let _ = path;
    Ok(())
}
/// The captured file observation changed during a read or exact revalidation.
/// This is a transient error value, not authority to read, write, or retry.
/// Access/scope/type violations remain distinct errors.
#[derive(Debug)]
pub struct ObservationChanged {
    cause: Option<io::Error>,
}
impl std::fmt::Display for ObservationChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native document observation changed")
    }
}
impl std::error::Error for ObservationChanged {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|cause| cause as _)
    }
}
fn observation_changed(cause: Option<io::Error>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, ObservationChanged { cause })
}
fn conflict() -> io::Error {
    io::ErrorKind::InvalidData.into()
}
fn existing_change(error: io::Error) -> io::Error {
    if error.kind() == io::ErrorKind::NotFound {
        observation_changed(Some(error))
    } else {
        error
    }
}

#[cfg(test)]
pub(crate) mod test_hooks {
    use std::{cell::RefCell, fs::File, io, thread::LocalKey};
    type Hook = RefCell<Option<Box<dyn FnOnce()>>>;
    type Apply = RefCell<Option<Box<dyn FnOnce(&File) -> io::Result<()>>>>;
    thread_local! {
        pub(crate) static AFTER_APPLY: Apply = const { RefCell::new(None) };
        pub(super) static BEFORE_OPEN: Hook = const { RefCell::new(None) };
        pub(super) static AFTER_CHUNK: Hook = const { RefCell::new(None) };
        pub(super) static BEFORE_PROBE: Hook = const { RefCell::new(None) };
    }
    pub(super) fn run(hook: &'static LocalKey<Hook>) {
        let action = hook.with(|h| h.borrow_mut().take());
        if let Some(action) = action {
            action();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::file_lock::Cancellation;
    use std::{
        io::Write,
        time::{Duration, Instant},
    };

    fn fixture(
        size: usize,
    ) -> (
        tempfile::TempDir,
        CapturedDocumentDirectory,
        SnapshotContext,
        Cancellation,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("config"), vec![b'x'; size]).unwrap();
        let cancel = Cancellation::default();
        let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
        let access_bytes = if cfg!(target_os = "macos") { 65536 } else { 12 };
        let limits = NativeDocumentFileLimits::new(size, 65536, 256, access_bytes).unwrap();
        let directory = CapturedDocumentDirectory::capture(tmp.path(), limits, &ctx).unwrap();
        (tmp, directory, ctx, cancel)
    }

    #[test]
    fn actual_growth_probe_rejects_changed_file() {
        let (tmp, directory, ctx, _) = fixture(4);
        let path = tmp.path().join("config");
        test_hooks::BEFORE_PROBE.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                OpenOptions::new()
                    .append(true)
                    .open(path)
                    .unwrap()
                    .write_all(b"z")
                    .unwrap();
            }))
        });
        assert_eq!(
            directory
                .read(OsStr::new("config"), &ctx)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );

        assert_eq!(fs::read(tmp.path().join("config")).unwrap(), b"xxxxz");
    }

    #[cfg(target_os = "macos")]
    fn mac_acl(path: &Path, add: bool) {
        let mut command = std::process::Command::new("chmod");
        if add {
            command.args(["+a", "everyone allow read"]);
        } else {
            command.arg("-N");
        }
        assert!(command.arg(path).status().unwrap().success());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_acl_only_change_conflicts_with_fresh_observation() {
        use std::os::unix::fs::MetadataExt;
        let (tmp, directory, _, _) = fixture(4);
        let path = tmp.path().join("config");
        mac_acl(&path, false);
        let ctx = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let original = directory.read(OsStr::new("config"), &ctx).unwrap();
        let before = fs::metadata(&path).unwrap();
        mac_acl(&path, true);
        let after = fs::metadata(&path).unwrap();
        assert_eq!(before.mode(), after.mode());
        assert_eq!(before.uid(), after.uid());
        assert_eq!(before.gid(), after.gid());
        assert_eq!(before.ino(), after.ino());
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
        assert_eq!(fs::read(&path).unwrap(), b"xxxx");
        assert_eq!(
            directory
                .revalidate_observation(&original, &ctx)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_acl_change_during_actual_content_read_conflicts() {
        let (tmp, directory, ctx, _) = fixture(128 * 1024);
        let path = tmp.path().join("config");
        mac_acl(&path, false);
        test_hooks::AFTER_CHUNK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || mac_acl(&path, true)));
        });
        assert_eq!(
            directory
                .read(OsStr::new("config"), &ctx)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_absent_acl_quota_counts_posix_metadata_and_actual_guids() {
        let (tmp, _, _, _) = fixture(4);
        mac_acl(&tmp.path().join("config"), false);
        for (access_bytes, expected) in [(44, None), (43, Some(io::ErrorKind::FileTooLarge))] {
            let ctx = SnapshotContext::new(
                Instant::now() + Duration::from_secs(30),
                Cancellation::default(),
            );
            let limits = NativeDocumentFileLimits::new(4, 65536, 256, access_bytes).unwrap();
            let directory = CapturedDocumentDirectory::capture(tmp.path(), limits, &ctx).unwrap();
            let result = directory.read(OsStr::new("config"), &ctx);
            assert_eq!(result.err().map(|error| error.kind()), expected);
        }
    }

    #[test]
    fn existing_entry_disappearance_never_becomes_missing() {
        let (tmp, directory, ctx, _) = fixture(4);
        let path = tmp.path().join("config");
        test_hooks::BEFORE_OPEN
            .with(|h| *h.borrow_mut() = Some(Box::new(move || fs::remove_file(path).unwrap())));
        let error = directory.read(OsStr::new("config"), &ctx).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let changed = error
            .get_ref()
            .unwrap()
            .downcast_ref::<ObservationChanged>()
            .unwrap();
        let cause = std::error::Error::source(changed)
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap();
        assert_eq!(cause.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn cancellation_between_real_blocks_stops_read() {
        let (tmp, directory, ctx, cancel) = fixture(128 * 1024);
        test_hooks::AFTER_CHUNK.with(|h| *h.borrow_mut() = Some(Box::new(move || cancel.cancel())));
        assert_eq!(
            directory
                .read(OsStr::new("config"), &ctx)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );

        assert_eq!(
            fs::metadata(tmp.path().join("config")).unwrap().len(),
            128 * 1024
        );
    }

    #[test]
    fn native_receipt_hashes_actual_bytes_and_roundtrips() {
        let (_tmp, directory, ctx, _) = fixture(4097);
        let observed = directory.read(OsStr::new("config"), &ctx).unwrap();
        let receipt = observed.receipt().unwrap();
        assert_eq!(receipt.length, 4097);
        assert_eq!(
            receipt.content,
            <[u8; 32]>::from(Sha256::digest(observed.bytes().unwrap()))
        );
        let encoded = serde_json::to_vec(receipt).unwrap();
        let restored: NativeDocumentReceipt = serde_json::from_slice(&encoded).unwrap();
        assert!(&restored == receipt);

        let missing = directory.read(OsStr::new("missing"), &ctx).unwrap();
        assert!(missing.receipt().is_none());
        assert!(missing.bytes().is_none());
    }

    #[test]
    fn native_receipt_empty_content_is_observed_without_reading_bytes() {
        let (tmp, directory, ctx, _) = fixture(1);
        fs::write(tmp.path().join("config"), b"").unwrap();
        let observed = directory.read(OsStr::new("config"), &ctx).unwrap();
        let receipt = observed.receipt().unwrap();
        assert_eq!(receipt.length, 0);
        assert_eq!(receipt.content, <[u8; 32]>::from(Sha256::digest(b"")));
    }

    #[test]
    fn native_receipt_wire_is_closed_validated_data_not_observation() {
        let (tmp, directory, ctx, _) = fixture(4);
        let observed = directory.read(OsStr::new("config"), &ctx).unwrap();
        let valid = serde_json::to_value(observed.receipt().unwrap()).unwrap();
        let bytes_before = fs::read(tmp.path().join("config")).unwrap();
        let mut invalid = Vec::new();
        for field in ["file_id", "length", "content", "access"] {
            let mut value = valid.clone();
            value.as_object_mut().unwrap().remove(field);
            invalid.push(value);
        }
        let mut value = valid.clone();
        value["unexpected"] = true.into();
        invalid.push(value);
        let mut value = valid.clone();
        value["length"] = u64::MAX.into();
        invalid.push(value);
        for field in ["content", "access"] {
            for length in [0, 31, 33] {
                let mut value = valid.clone();
                value[field] = serde_json::json!(vec![0; length]);
                invalid.push(value);
            }
        }
        let mut value = valid.clone();
        value["file_id"]["inode"] = 0.into();
        invalid.push(value);
        let mut value = valid.clone();
        value["file_id"] = serde_json::json!({
            "volume": 1, "index": vec![1; 16], "created": 1
        });
        invalid.push(value);
        for value in invalid {
            assert!(serde_json::from_value::<NativeDocumentReceipt>(value).is_err());
        }
        for field in ["file_id", "length", "content", "access"] {
            let field_value = serde_json::to_string(&valid[field]).unwrap();
            let remainder = serde_json::to_string(&valid).unwrap();
            let duplicate = format!("{{\"{field}\":{field_value},{}", &remainder[1..]);
            assert!(serde_json::from_str::<NativeDocumentReceipt>(&duplicate).is_err());
        }
        assert_eq!(fs::read(tmp.path().join("config")).unwrap(), bytes_before);

        // Restored data is freely comparable; it cannot replace the opaque live
        // observation or the parent/name checks in revalidate_observation.
        let restored: NativeDocumentReceipt = serde_json::from_value(valid).unwrap();
        assert!(Some(&restored) == observed.receipt());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn native_receipt_distinguishes_real_file_content_mode_and_acl_changes() {
        use std::os::unix::fs::PermissionsExt;
        let (tmp, directory, ctx) = access_fixture(56);
        let path = tmp.path().join("config");
        let first = directory.read(OsStr::new("config"), &ctx).unwrap();
        let original = first.receipt().unwrap();
        fs::rename(&path, tmp.path().join("original")).unwrap();
        fs::write(&path, b"data").unwrap();
        let copied = directory.read(OsStr::new("config"), &ctx).unwrap();
        let copied_receipt = copied.receipt().unwrap();
        assert!(copied_receipt.file_id != original.file_id);
        assert_eq!(copied_receipt.content, original.content);
        assert_eq!(copied_receipt.access, original.access);
        assert!(copied_receipt != original);
        fs::write(&path, b"next").unwrap();
        let changed = directory.read(OsStr::new("config"), &ctx).unwrap();
        let changed_receipt = changed.receipt().unwrap();
        assert!(changed_receipt.file_id == copied_receipt.file_id);
        assert_ne!(changed_receipt.content, copied_receipt.content);
        assert_eq!(changed_receipt.access, copied_receipt.access);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let restricted = directory.read(OsStr::new("config"), &ctx).unwrap();
        assert!(restricted.receipt().unwrap().file_id == changed_receipt.file_id);
        assert_eq!(
            restricted.receipt().unwrap().content,
            changed_receipt.content
        );
        assert_ne!(restricted.receipt().unwrap().access, changed_receipt.access);
        let file = File::open(&path).unwrap();
        named_acl(&file, 424242);
        let acl = directory.read(OsStr::new("config"), &ctx).unwrap();
        assert!(acl.receipt().unwrap().file_id == restricted.receipt().unwrap().file_id);
        assert_eq!(
            acl.receipt().unwrap().content,
            restricted.receipt().unwrap().content
        );
        assert_ne!(
            acl.receipt().unwrap().access,
            restricted.receipt().unwrap().access
        );
        named_acl(&file, 424243);
        let other_acl = directory.read(OsStr::new("config"), &ctx).unwrap();
        assert_ne!(
            other_acl.receipt().unwrap().access,
            acl.receipt().unwrap().access
        );
    }

    #[cfg(target_os = "linux")]
    fn named_acl(file: &File, principal: u32) {
        named_acls(file, &[principal]);
    }

    #[cfg(target_os = "linux")]
    fn named_acls(file: &File, principals: &[u32]) {
        // Disposable fixture in the kernel's documented POSIX ACL xattr ABI:
        // version 2, little-endian (tag, permissions, numeric principal) entries.
        // Production retains opaque bytes; it never parses this format.
        let mut acl = 2u32.to_le_bytes().to_vec();
        let entries = [(0x01u16, 6u16, u32::MAX)]
            .into_iter()
            .chain(principals.iter().map(|&id| (0x02, 4, id)))
            .chain([
                (0x04, 4, u32::MAX),
                (0x10, 4, u32::MAX),
                (0x20, 0, u32::MAX),
            ]);
        for (tag, permissions, id) in entries {
            acl.extend_from_slice(&tag.to_le_bytes());
            acl.extend_from_slice(&permissions.to_le_bytes());
            acl.extend_from_slice(&id.to_le_bytes());
        }
        rustix::fs::fsetxattr(
            file,
            "system.posix_acl_access",
            &acl,
            rustix::fs::XattrFlags::empty(),
        )
        .unwrap();
    }

    #[cfg(target_os = "linux")]
    fn access_fixture(
        access_bytes: usize,
    ) -> (
        tempfile::TempDir,
        CapturedDocumentDirectory,
        SnapshotContext,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("config"), b"data").unwrap();
        let ctx = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let limits = NativeDocumentFileLimits::new(4, 65536, 256, access_bytes).unwrap();
        let directory = CapturedDocumentDirectory::capture(tmp.path(), limits, &ctx).unwrap();
        (tmp, directory, ctx)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_acl_change_conflicts_without_changing_content_identity_or_mode() {
        use std::os::unix::fs::MetadataExt;
        let (tmp, directory, ctx) = access_fixture(56);
        let path = tmp.path().join("config");
        let file = File::open(&path).unwrap();
        named_acl(&file, 424242);
        let before = directory.read(OsStr::new("config"), &ctx).unwrap();
        let metadata = file.metadata().unwrap();
        let identity = handle_id(&file).unwrap();
        named_acl(&file, 424243);
        let after = file.metadata().unwrap();
        assert!(handle_id(&file).unwrap() == identity);
        assert_eq!(after.mode(), metadata.mode());
        assert_eq!(after.uid(), metadata.uid());
        assert_eq!(after.gid(), metadata.gid());
        assert_eq!(after.modified().unwrap(), metadata.modified().unwrap());
        let error = directory.revalidate_observation(&before, &ctx).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(
            !error
                .get_ref()
                .is_some_and(|cause| cause.is::<ObservationChanged>())
        );

        assert_eq!(fs::read(path).unwrap(), b"data");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_acl_exact_access_limit_passes_and_short_limits_read_no_content() {
        for access_bytes in [56, 55, 12, 11] {
            let (tmp, directory, ctx) = access_fixture(access_bytes);
            let path = tmp.path().join("config");
            named_acl(&File::open(&path).unwrap(), 424242);
            let result = directory.read(OsStr::new("config"), &ctx);
            if access_bytes == 56 {
                assert_eq!(result.unwrap().bytes(), Some(b"data".as_slice()));
            } else {
                assert_eq!(result.err().unwrap().kind(), io::ErrorKind::FileTooLarge);
            }

            assert_eq!(fs::read(path).unwrap(), b"data");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_absent_acl_needs_only_mode_uid_gid_capacity() {
        let (tmp, directory, ctx) = access_fixture(12);
        let file = File::open(tmp.path().join("config")).unwrap();
        assert_eq!(
            rustix::fs::fgetxattr(&file, "system.posix_acl_access", &mut [0u8; 0][..]).unwrap_err(),
            rustix::io::Errno::NODATA
        );
        assert_eq!(
            directory.read(OsStr::new("config"), &ctx).unwrap().bytes(),
            Some(b"data".as_slice())
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_acl_mutation_during_real_read_rejects_the_observation() {
        let (tmp, directory, ctx) = access_fixture(56);
        let path = tmp.path().join("config");
        let file = File::open(&path).unwrap();
        named_acl(&file, 424242);
        test_hooks::AFTER_CHUNK
            .with(|h| *h.borrow_mut() = Some(Box::new(move || named_acl(&file, 424243))));
        assert_eq!(
            directory
                .read(OsStr::new("config"), &ctx)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );

        assert_eq!(fs::read(path).unwrap(), b"data");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_acl_presence_transitions_conflict_with_constant_mode_and_content() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        for initially_present in [false, true] {
            let (tmp, directory, ctx) = access_fixture(56);
            let path = tmp.path().join("config");
            let file = File::open(&path).unwrap();
            file.set_permissions(fs::Permissions::from_mode(0o640))
                .unwrap();
            if initially_present {
                named_acl(&file, 424242);
            }
            let before = directory.read(OsStr::new("config"), &ctx).unwrap();
            let metadata = file.metadata().unwrap();
            let identity = handle_id(&file).unwrap();
            if initially_present {
                rustix::fs::fremovexattr(&file, "system.posix_acl_access").unwrap();
            } else {
                named_acl(&file, 424242);
            }
            assert!(handle_id(&file).unwrap() == identity);
            assert_eq!(file.metadata().unwrap().mode(), metadata.mode());
            assert_eq!(
                file.metadata().unwrap().modified().unwrap(),
                metadata.modified().unwrap()
            );
            assert_eq!(
                directory
                    .revalidate_observation(&before, &ctx)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );

            assert_eq!(fs::read(path).unwrap(), b"data");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_unsupported_acl_filesystem_error_is_preserved() {
        // procfs supplies an actual unsupported ACL route; no content is read.
        let file = File::open("/proc/self/status").unwrap();
        let direct =
            rustix::fs::fgetxattr(&file, "system.posix_acl_access", &mut [0u8; 0][..]).unwrap_err();
        assert_eq!(direct, rustix::io::Errno::OPNOTSUPP);
        let error = graphit_fs::linux_access_snapshot(&file, 65536)
            .err()
            .unwrap();
        assert_eq!(error.raw_os_error(), Some(direct.raw_os_error()));
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }
}

impl CapturedDocumentDirectory {
    pub(crate) fn identity(&self) -> &PhysicalId {
        &self.ancestors[0].identity
    }

    pub(crate) fn limits(&self) -> NativeDocumentFileLimits {
        self.limits
    }

    pub(crate) fn check_expected(
        &self,
        observation: &NativeDocumentObservation,
        context: &SnapshotContext,
    ) -> io::Result<PathBuf> {
        context.check()?;
        if observation._project.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if observation.directory != *self.identity() || !observation.request.matches(context) {
            return Err(conflict());
        }
        self.path(&observation.name)
    }

    pub(crate) fn verify_expected(
        &self,
        observation: &NativeDocumentObservation,
        mut pass: NativeIoPass,
    ) -> io::Result<()> {
        if observation._project.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if observation.directory != *self.identity() || !pass.matches_request(&observation.request)
        {
            return Err(conflict());
        }
        self.revalidate_io(&pass)?;
        let path = self.path(&observation.name)?;
        match (&observation.value, regular_metadata(&path)?) {
            (None, None) => {}
            (Some(expected), Some(metadata)) => {
                if physical_id(&path, &metadata)? != expected.metadata.identity {
                    return Err(conflict());
                }
                let mut file = open_file(&path)?;
                let before = observe_handle(&file, self.limits.access_bytes)?;
                if before != expected.metadata {
                    return Err(conflict());
                }
                let actual = inspect_file_inner(
                    &path,
                    &mut file,
                    self.limits,
                    &mut pass,
                    Some(&expected.bytes),
                )?;
                if actual != expected.receipt
                    || observe_handle(&file, self.limits.access_bytes)? != expected.metadata
                {
                    return Err(conflict());
                }
            }
            _ => return Err(conflict()),
        }
        self.revalidate_io(&pass)
    }
}
impl NativeDocumentFileLimits {
    pub(crate) fn check_bytes(&self, length: u64) -> io::Result<()> {
        if length > self.bytes as u64 {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(())
    }

    pub(crate) fn check_path(&self, path: &Path) -> io::Result<()> {
        self.path(path).map(|_| ())
    }

    pub(crate) fn path_cost(&self, path: &Path) -> io::Result<usize> {
        self.path(path)
    }
}
impl NativeDocumentObservation {
    pub(crate) fn apply_access(
        &self,
        file: &File,
        limits: NativeDocumentFileLimits,
    ) -> io::Result<()> {
        if self._project.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if let Some(previous) = &self.value {
            previous.metadata.access.apply(file, limits.access_bytes)?;
            #[cfg(test)]
            if let Some(apply) = test_hooks::AFTER_APPLY.with(|h| h.borrow_mut().take()) {
                apply(file)?;
            }
        }
        Ok(())
    }

    pub(crate) fn verify_stage_access(&self, receipt: &NativeDocumentReceipt) -> io::Result<()> {
        #[cfg(unix)]
        if self
            .receipt()
            .is_some_and(|old| old.access != receipt.access)
        {
            return Err(conflict());
        }
        #[cfg(windows)]
        let _ = receipt;
        Ok(())
    }
}
impl Access {
    fn apply(&self, file: &File, limit: usize) -> io::Result<()> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(access) => graphit_fs::linux_apply_access(file, access, limit),
            #[cfg(target_os = "macos")]
            Self::Unix {
                mode,
                uid,
                gid,
                mac_access,
            } => {
                // The setter's physical prefix is larger than the read response.
                // This finite upper bound admits that prefix in addition to the
                // already bounded observed access data, without another read.
                let buffer = limit.checked_add(52).ok_or(io::ErrorKind::FileTooLarge)?;
                graphit_fs::macos_apply_access(file, mac_access, *mode, *uid, *gid, buffer)
            }
            #[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
            Self::Unix { mode, uid, gid } => {
                use std::os::unix::fs::PermissionsExt;
                let _ = limit;
                rustix::fs::fchown(
                    file,
                    Some(rustix::process::Uid::from_raw(*uid)),
                    Some(rustix::process::Gid::from_raw(*gid)),
                )?;
                file.set_permissions(std::fs::Permissions::from_mode(*mode))
            }
            #[cfg(windows)]
            Self::WindowsProject(_) => Err(io::ErrorKind::InvalidInput.into()),
            #[cfg(windows)]
            Self::Windows(_) => {
                // ReplaceFileW applies the existing descriptor at publication.
                let _ = (file, limit);
                Ok(())
            }
        }
    }
}
impl NativeDocumentReceipt {
    pub(crate) fn file_id(&self) -> &PhysicalId {
        &self.file_id
    }
    pub(crate) fn length(&self) -> u64 {
        self.length
    }
    #[cfg(test)]
    pub(crate) fn largest(length: u64) -> Self {
        Self {
            file_id: PhysicalId::largest(),
            length,
            content: [255; 32],
            access: [255; 32],
        }
    }
    pub(crate) fn destination_after(&self, previous: Option<&Self>) -> io::Result<Self> {
        let mut expected = self.clone();
        match (&self.file_id, previous.map(|r| &r.file_id)) {
            (
                PhysicalId::Windows { volume, index, .. },
                Some(PhysicalId::Windows {
                    volume: old_volume,
                    created,
                    ..
                }),
            ) if volume == old_volume => {
                expected.file_id = PhysicalId::Windows {
                    volume: *volume,
                    index: *index,
                    created: *created,
                };
                expected.access = previous.ok_or_else(conflict)?.access;
            }
            (PhysicalId::Unix { .. }, Some(PhysicalId::Unix { .. })) | (_, None) => {}
            _ => return Err(conflict()),
        }
        Ok(expected)
    }
}

/// Inspect one finite pass and verify the retained parent after its read.
pub(crate) fn inspect_file(
    path: &Path,
    file: &mut File,
    limits: NativeDocumentFileLimits,
    mut pass: NativeIoPass,
    expected_bytes: Option<&[u8]>,
    revalidate: impl FnOnce(&NativeIoPass) -> io::Result<()>,
) -> io::Result<NativeDocumentReceipt> {
    let result = inspect_file_inner(path, file, limits, &mut pass, expected_bytes);
    let validation = revalidate(&pass);
    match (result, validation) {
        (Err(primary), Err(secondary)) => Err(io::Error::new(
            primary.kind(),
            NativeRevalidationFailure {
                secondary: secondary.kind(),
            },
        )),
        (Err(primary), _) => Err(primary),
        (_, Err(secondary)) => Err(secondary),
        (Ok(receipt), Ok(())) => Ok(receipt),
    }
}

#[derive(Debug)]
struct NativeRevalidationFailure {
    secondary: io::ErrorKind,
}
impl std::fmt::Display for NativeRevalidationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native document revalidation failed after a prior failure")
    }
}
impl std::error::Error for NativeRevalidationFailure {}
pub(crate) fn revalidation_secondary(error: &io::Error) -> Option<io::ErrorKind> {
    error
        .get_ref()?
        .downcast_ref::<NativeRevalidationFailure>()
        .map(|e| e.secondary)
}

fn inspect_file_inner(
    path: &Path,
    file: &mut File,
    limits: NativeDocumentFileLimits,
    pass: &mut NativeIoPass,
    expected_bytes: Option<&[u8]>,
) -> io::Result<NativeDocumentReceipt> {
    pass.check()?;
    limits.check_path(path)?;
    let before = observe_handle(file, limits.access_bytes)?;
    limits.check_bytes(before.length)?;
    pass.begin_inspection(
        before
            .length
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?,
    )?;
    file.rewind()?;
    if let Some(bytes) = expected_bytes
        && bytes.len() as u64 != before.length
    {
        return Err(conflict());
    }
    let mut digest = Sha256::new();
    let mut buffer = Zeroizing::new([0u8; 64 * 1024]);
    let mut offset = 0usize;
    let mut left = before.length;
    while left != 0 {
        let wanted = left.min(buffer.len() as u64) as usize;
        let read = pass.read(file, &mut buffer[..wanted])?;
        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if let Some(bytes) = expected_bytes
            && bytes.get(offset..offset + read) != Some(&buffer[..read])
        {
            return Err(conflict());
        }
        digest.update(&buffer[..read]);
        offset = offset
            .checked_add(read)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        left -= read as u64;
    }
    if pass.read(file, &mut buffer[..1])? != 0
        || observe_handle(file, limits.access_bytes)? != before
        || physical_id(path, &regular_metadata(path)?.ok_or_else(conflict)?)? != before.identity
    {
        return Err(conflict());
    }
    pass.check()?;
    Ok(NativeDocumentReceipt {
        file_id: before.identity,
        length: before.length,
        content: digest.finalize().into(),
        access: before.access.fingerprint(),
    })
}

#[cfg(test)]
mod stage_receipt_tests {
    use super::*;
    #[test]
    fn native_stage_windows_expected_receipt_is_distinct_from_actual_scratch() {
        let stage = NativeDocumentReceipt {
            file_id: PhysicalId::Windows {
                volume: 7,
                index: [22; 16],
                created: 333,
            },
            length: 4,
            content: [1; 32],
            access: [2; 32],
        };
        let old = NativeDocumentReceipt {
            file_id: PhysicalId::Windows {
                volume: 7,
                index: [11; 16],
                created: 444,
            },
            length: 3,
            content: [3; 32],
            access: [4; 32],
        };
        let destination = stage.destination_after(Some(&old)).unwrap();
        assert!(
            destination.file_id
                == PhysicalId::Windows {
                    volume: 7,
                    index: [22; 16],
                    created: 444
                }
        );
        assert_eq!(destination.content, stage.content);
        assert_eq!(destination.length, 4);
        assert_eq!(destination.access, old.access);
        assert!(
            stage.file_id
                == PhysicalId::Windows {
                    volume: 7,
                    index: [22; 16],
                    created: 333
                }
        );
        assert_eq!(stage.access, [2; 32]);
        assert!(stage.destination_after(None).unwrap() == stage);
        let other_volume = NativeDocumentReceipt {
            file_id: PhysicalId::Windows {
                volume: 8,
                index: [11; 16],
                created: 444,
            },
            ..old
        };
        assert!(stage.destination_after(Some(&other_volume)).is_err());
        assert!(
            stage
                .destination_after(Some(&NativeDocumentReceipt::largest(3)))
                .is_err()
        );
    }
}
