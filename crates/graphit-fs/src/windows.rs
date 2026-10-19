use super::*;
use std::{
    fs::OpenOptions,
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
};
use windows_sys::Win32::{
    Foundation::GENERIC_WRITE,
    Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT},
    System::{IO::DeviceIoControl, Ioctl::FSCTL_SET_REPARSE_POINT},
};

pub(super) fn create(plan: &LinkPlan) -> Result<LinkCreation, LinkFailure> {
    match plan.kind {
        NodeKind::File => {
            std::os::windows::fs::symlink_file(&plan.source, &plan.destination)
                .map_err(|e| failure(LinkStage::Symlink, e))?;
            Ok(LinkCreation::FileSymlink)
        }
        NodeKind::Directory => directory_link_using(
            || std::os::windows::fs::symlink_dir(&plan.source, &plan.destination),
            || create_junction(plan),
        ),
    }
}

fn create_junction(plan: &LinkPlan) -> Result<(), LinkFailure> {
    let target: Vec<_> = plan.source.as_os_str().encode_wide().collect();
    let buffer = super::junction_buffer::encode(&target)
        .map_err(|e| failure(LinkStage::JunctionBuffer, e))?;
    junction_using(
        || fs::create_dir(&plan.destination),
        || {
            // Exclusive access: the staging directory belongs to the coordinated caller.
            // Never enable privileges or reopen with a more privileged access mode.
            let file = OpenOptions::new()
                .access_mode(GENERIC_WRITE)
                .share_mode(0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
                .open(&plan.destination)
                .map_err(|e| failure(LinkStage::JunctionOpen, e))?;
            set_junction(&file, &buffer).map_err(|e| failure(LinkStage::JunctionSet, e))
        },
        || fs::remove_dir(&plan.destination),
    )
}

// xtask permits exactly this SET call; other SDK sites have their own exact allowlist.
#[allow(unsafe_code)]
fn set_junction(file: &fs::File, buffer: &[u8]) -> io::Result<()> {
    let mut returned = 0u32;
    // SAFETY: File owns an open synchronous, non-overlapped directory handle.
    // The initialized buffer remains alive and immutable for the call, its exact
    // length is at most 16 KiB, and the API consumes bytes without Rust struct
    // dereferences. No output buffer is supplied; returned is valid/writable even
    // for SET with zero output. DeviceIoControl does not retain these pointers.
    let result = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            buffer.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forced_junction_roundtrip_without_privilege_adjustment() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source 🚀");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("keep"), b"untouched").unwrap();
        let plan =
            LinkPlan::capture(&source, &root.path().join("junction"), NodeKind::Directory).unwrap();
        create_junction(&plan).unwrap();
        assert_eq!(fs::canonicalize(plan.destination()).unwrap(), plan.source());
        assert_eq!(
            fs::canonicalize(fs::read_link(plan.destination()).unwrap()).unwrap(),
            plan.source()
        );
        assert_eq!(
            fs::read(plan.destination().join("keep")).unwrap(),
            b"untouched"
        );
        assert_eq!(
            plan.create().unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        fs::remove_dir(plan.destination()).unwrap();
        assert_eq!(fs::read(source.join("keep")).unwrap(), b"untouched");
    }
}
