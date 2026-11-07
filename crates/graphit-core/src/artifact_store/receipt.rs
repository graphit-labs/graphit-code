use super::{codec::*, *};

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Stamp {
    Unix {
        device: u64,
        inode: u64,
        change_seconds: i64,
        change_nanos: u32,
    },
    Windows {
        volume: u64,
        index: [u8; 16],
        created: u64,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub stamp: Stamp,
    pub target: NativePath,
}
impl Receipt {
    /// Only for a durably recorded rename whose source is now absent. NTFS may
    /// tunnel CreationTime at the destination; the volume, full file ID and
    /// literal link target must still identify the same moved object.
    pub fn same_moved_object(&self, other: &Self) -> bool {
        if !same_target(&self.target.0, &other.target.0) {
            return false;
        }
        match (&self.stamp, &other.stamp) {
            (
                Stamp::Unix {
                    device: a,
                    inode: b,
                    ..
                },
                Stamp::Unix {
                    device: c,
                    inode: d,
                    ..
                },
            ) => a == c && b == d,
            (
                Stamp::Windows {
                    volume: a,
                    index: b,
                    ..
                },
                Stamp::Windows {
                    volume: d,
                    index: e,
                    ..
                },
            ) => a == d && b == e,
            _ => false,
        }
    }

    pub fn validate(&self, candidate: &Candidate) -> io::Result<()> {
        let valid_stamp = match &self.stamp {
            #[cfg(unix)]
            Stamp::Unix {
                inode,
                change_nanos,
                ..
            } => *inode != 0 && *change_nanos < 1_000_000_000,
            #[cfg(windows)]
            Stamp::Windows { index, created, .. } => {
                index.iter().any(|byte| *byte != 0) && *created != 0
            }
            _ => false,
        };
        valid_path(&self.target.0)?;
        if !valid_stamp || !same_target(&self.target.0, &candidate.source.0) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        Ok(())
    }
}

fn same_target(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        fn units(path: &Path) -> Vec<u16> {
            let units: Vec<_> = path.as_os_str().encode_wide().collect();
            let prefix: Vec<_> = "\\\\?\\".encode_utf16().collect();
            if let Some(rest) = units.strip_prefix(prefix.as_slice()) {
                let unc: Vec<_> = "UNC\\".encode_utf16().collect();
                if let Some(rest) = rest.strip_prefix(unc.as_slice()) {
                    "\\\\".encode_utf16().chain(rest.iter().copied()).collect()
                } else {
                    rest.to_vec()
                }
            } else {
                units
            }
        }
        units(a) == units(b)
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

pub(super) fn observe(candidate: &Candidate) -> io::Result<Option<Receipt>> {
    observe_at(candidate, &candidate.destination.0)
}

pub(super) fn observe_at(candidate: &Candidate, path: &Path) -> io::Result<Option<Receipt>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_symlink() {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    // Comparing the captured parent rejects an adapter root redirected later.
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    if fs::canonicalize(parent)? != parent {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    #[cfg(unix)]
    let stamp = {
        use std::os::unix::fs::MetadataExt;
        Stamp::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
            change_seconds: metadata.ctime(),
            change_nanos: metadata
                .ctime_nsec()
                .try_into()
                .map_err(|_| io::ErrorKind::Unsupported)?,
        }
    };
    #[cfg(windows)]
    let stamp = {
        use std::os::windows::fs::{FileTypeExt, OpenOptionsExt};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        if metadata.file_type().is_symlink_dir() != (candidate.kind == Kind::Directory) {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        // std owns the no-follow handle; no raw handle or privilege mutation.
        let file = fs::OpenOptions::new()
            .access_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        let (identity, attributes) =
            crate::native_file_identity::windows_handle_observation(&file)?;
        if attributes & u64::from(FILE_ATTRIBUTE_REPARSE_POINT) == 0 {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        let crate::native_file_identity::PhysicalId::Windows {
            volume,
            index,
            created,
        } = identity
        else {
            return Err(io::ErrorKind::Unsupported.into());
        };
        Stamp::Windows {
            volume,
            index,
            created,
        }
    };
    #[cfg(not(any(unix, windows)))]
    return Err(io::Error::from(io::ErrorKind::Unsupported));
    #[cfg(any(unix, windows))]
    {
        let receipt = Receipt {
            stamp,
            target: NativePath(fs::read_link(path)?),
        };
        receipt.validate(candidate)?;
        Ok(Some(receipt))
    }
}

pub(super) fn remove_at(kind: Kind, path: &Path) -> io::Result<()> {
    let _ = kind;
    #[cfg(windows)]
    if kind == Kind::Directory {
        return fs::remove_dir(path);
    }
    fs::remove_file(path)
}
