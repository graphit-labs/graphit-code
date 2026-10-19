//! One bounded body/hash/probe loop for both real acquisition domains.
use super::*;

pub(super) struct Body {
    pub(super) bytes: Zeroizing<Vec<u8>>,
    pub(super) digest: [u8; 32],
}
pub(super) fn body(
    file: &mut File,
    length: u64,
    limits: NativeDocumentFileLimits,
    content_cap: usize,
    context: &SnapshotContext,
) -> io::Result<Body> {
    let length_usize = usize::try_from(length).map_err(|_| io::ErrorKind::FileTooLarge)?;
    if length_usize > content_cap.min(limits.bytes) {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    context.check()?;

    let mut bytes = Zeroizing::new(Vec::new());
    bytes
        .try_reserve_exact(length_usize)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    bytes.resize(length_usize, 0);

    let mut left = length_usize;
    let mut digest = Sha256::new();
    while left != 0 {
        context.check()?;
        let offset = length_usize - left;
        let wanted = left.min(64 * 1024);
        let read = file.read(&mut bytes[offset..offset + wanted])?;
        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        digest.update(&bytes[offset..offset + read]);
        left -= read;
        #[cfg(test)]
        test_hooks::run(&test_hooks::AFTER_CHUNK);
    }
    #[cfg(test)]
    test_hooks::run(&test_hooks::BEFORE_PROBE);
    context.check()?;
    let mut probe = Zeroizing::new([0_u8; 1]);
    let read = file.read(&mut probe[..])?;
    if read != 0 {
        return Err(conflict());
    }
    let digest = digest.finalize().into();
    Ok(Body { bytes, digest })
}
