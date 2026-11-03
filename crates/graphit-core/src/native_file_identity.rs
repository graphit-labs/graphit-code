//! Native physical identity observations shared by filesystem consumers.
//!
//! Identity data grants no permission, path confinement, ownership or freshness.
//! Callers retain their own captures and revalidate them around each operation.

use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

/// Native identity observed from a file or handle; never sufficient authority alone.
/// Windows replacement may transfer creation time from the previous destination.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum PhysicalId {
    Unix {
        device: u64,
        inode: u64,
    },
    Windows {
        volume: u64,
        index: [u8; 16],
        created: u64,
    },
}
impl PhysicalId {
    pub(crate) fn validate(&self) -> io::Result<()> {
        let valid = match self {
            #[cfg(unix)]
            Self::Unix { inode, .. } => *inode != 0,
            #[cfg(windows)]
            Self::Windows { index, created, .. } => {
                index.iter().any(|byte| *byte != 0) && *created != 0
            }
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(io::ErrorKind::InvalidData.into())
        }
    }
    pub(crate) fn largest() -> Self {
        #[cfg(windows)]
        {
            Self::Windows {
                volume: u64::MAX,
                index: [255; 16],
                created: u64::MAX,
            }
        }
        #[cfg(not(windows))]
        {
            Self::Unix {
                device: u64::MAX,
                inode: u64::MAX,
            }
        }
    }
}
/// Observes the existing native identity for a path and its captured metadata.
///
/// Unix uses the supplied metadata; Windows opens the path itself for identity.
/// The caller owns traversal, metadata/path association and before/after checks.
/// This observation does not reject every alias or grant access to its target.
pub fn physical_id(path: &Path, metadata: &fs::Metadata) -> io::Result<PhysicalId> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        Ok(PhysicalId::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        let _ = metadata;
        let file = fs::OpenOptions::new()
            .access_mode(0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        windows_handle_observation(&file).map(|(identity, _)| identity)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, metadata);
        Err(io::ErrorKind::Unsupported.into())
    }
}
/// Observes identity from a retained file or directory handle.
///
/// Windows uses the full native FileId128 and rejects reparse-point handles.
/// Regularity, access, link-count and caller authorization are separate checks.
pub fn handle_id(file: &fs::File) -> io::Result<PhysicalId> {
    #[cfg(unix)]
    {
        physical_id(Path::new(""), &file.metadata()?)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        let (identity, attributes) = windows_handle_observation(file)?;
        if attributes & u64::from(FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        Ok(identity)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = file;
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(windows)]
pub(crate) fn windows_handle_observation(file: &fs::File) -> io::Result<(PhysicalId, u64)> {
    let native = query_file_id128(file)?;
    let info = winapi_util::file::information(file)?;
    let id = PhysicalId::Windows {
        volume: native.VolumeSerialNumber,
        index: native.FileId.Identifier,
        created: info.creation_time().ok_or(io::ErrorKind::Unsupported)?,
    };
    id.validate()?;
    Ok((id, info.file_attributes()))
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn query_file_id128(
    file: &fs::File,
) -> io::Result<windows_sys::Win32::Storage::FileSystem::FILE_ID_INFO> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_128, FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };
    let mut information = FILE_ID_INFO {
        VolumeSerialNumber: 0,
        FileId: FILE_ID_128 {
            Identifier: [0; 16],
        },
    };
    let bytes = u32::try_from(std::mem::size_of::<FILE_ID_INFO>())
        .map_err(|_| io::ErrorKind::Unsupported)?;
    // SAFETY: The generated SDK writes exactly the initialized, aligned local struct.
    // The File owns the borrowed HANDLE through this synchronous metadata call.
    let success = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&mut information as *mut FILE_ID_INFO).cast(),
            bytes,
        )
    };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(information)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_identity_distinguishes_the_full_native_identifier() {
        let first = PhysicalId::Windows {
            volume: 7,
            index: [1; 16],
            created: 9,
        };
        let mut second = first.clone();
        if let PhysicalId::Windows { index, .. } = &mut second {
            index[15] = 2;
        }
        assert!(first != second);
        assert!(first < second);
        assert!(
            serde_json::from_slice::<PhysicalId>(&serde_json::to_vec(&first).unwrap()).unwrap()
                == first
        );
    }
    #[cfg(windows)]
    #[test]
    fn zero_native_identifier_and_zero_creation_are_invalid() {
        for (index, created) in [([0; 16], 9), ([1; 16], 0)] {
            assert_eq!(
                PhysicalId::Windows {
                    volume: 7,
                    index,
                    created
                }
                .validate()
                .unwrap_err()
                .kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
}

#[cfg(test)]
mod serialization_tests {
    use super::*;
    #[test]
    fn native_identity_uses_unique_fields_without_an_os_tag() {
        for identity in [
            PhysicalId::Unix {
                device: 1,
                inode: 2,
            },
            PhysicalId::Windows {
                volume: 3,
                index: [1; 16],
                created: 4,
            },
        ] {
            let value = serde_json::to_value(&identity).unwrap();
            assert!(value.get("os").is_none());
            assert!(serde_json::from_value::<PhysicalId>(value).unwrap() == identity);
        }
    }
    #[test]
    fn native_identity_rejects_mixed_unknown_and_duplicate_fields() {
        for encoded in [
            r#"{"device":1,"inode":2,"volume":3,"index":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"created":4}"#,
            r#"{"device":1,"inode":2,"extra":0}"#,
            r#"{"device":1,"inode":2,"inode":3}"#,
        ] {
            assert!(serde_json::from_str::<PhysicalId>(encoded).is_err());
        }
    }
}
