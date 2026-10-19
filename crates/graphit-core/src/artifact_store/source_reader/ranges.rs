//! One engine for captured single ranges and ordered aggregate reads.
use super::{CapturedSourceFile, Shared, positional_read};
use std::{fs::File, io, sync::Arc};

/// A borrowed range of an actual captured file, never an adopted descriptor.
pub struct SourceRangeRequest<'a> {
    file: &'a CapturedSourceFile,
    start: u64,
    length: usize,
}
impl<'a> SourceRangeRequest<'a> {
    pub fn new(file: &'a CapturedSourceFile, start: u64, length: usize) -> io::Result<Self> {
        file.file.shared.context.check()?;
        validate_offset(file, start, length)?;
        Ok(Self {
            file,
            start,
            length,
        })
    }
}

/// Positive bounds for the number of ranges and their returned payload.
#[derive(Clone, Copy)]
pub struct AggregateReadLimits {
    max_ranges: usize,
    max_payload_bytes: usize,
}
impl AggregateReadLimits {
    pub fn new(max_ranges: usize, max_payload_bytes: usize) -> io::Result<Self> {
        if max_ranges == 0 || max_payload_bytes == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            max_ranges,
            max_payload_bytes,
        })
    }
}
/// Owned bytes from a successfully validated range.
pub struct RetainedSourceRange {
    bytes: Vec<u8>,
}
impl RetainedSourceRange {
    pub fn len(&self) -> usize {
        self.bytes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}
impl AsRef<[u8]> for RetainedSourceRange {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}
fn validate_offset(file: &CapturedSourceFile, start: u64, length: usize) -> io::Result<()> {
    let end = start
        .checked_add(u64::try_from(length).map_err(|_| io::ErrorKind::InvalidInput)?)
        .ok_or(io::ErrorKind::InvalidInput)?;
    if end > file.len() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
pub(super) fn read_ranges(
    shared: &Arc<Shared>,
    requests: &[SourceRangeRequest<'_>],
    limits: AggregateReadLimits,
) -> io::Result<Vec<RetainedSourceRange>> {
    shared.context.check()?;
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    if requests.len() > limits.max_ranges {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut total = 0usize;
    for request in requests {
        shared.context.check()?;
        if !Arc::ptr_eq(shared, &request.file.file.shared) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        validate_offset(request.file, request.start, request.length)?;
        if request.length > shared.limits.range_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        total = total
            .checked_add(request.length)
            .ok_or(io::ErrorKind::InvalidInput)?;
    }
    if total > limits.max_payload_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    // Positional reads share a descriptor; only a native exclusive writer makes it busy.
    // Keep each shared guard throughout its range, without changing the OS cursor.
    let first_fd = requests[0].file.file.read_handle()?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(requests.len())
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    let descriptors = std::iter::once(Ok(first_fd)).chain(
        requests[1..]
            .iter()
            .map(|request| request.file.file.read_handle()),
    );
    for (request, fd) in requests.iter().zip(descriptors) {
        shared.context.check()?;
        let file = &request.file.file;
        let fd = fd?;
        file.check(&fd)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(request.length)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        bytes.resize(request.length, 0);
        let mut read = 0;
        while read != request.length {
            shared.context.check()?;
            let block = (request.length - read).min(shared.limits.block_bytes);
            let offset = request
                .start
                .checked_add(u64::try_from(read).map_err(|_| io::ErrorKind::InvalidInput)?)
                .ok_or(io::ErrorKind::InvalidInput)?;
            let count = file.read(&fd, offset, &mut bytes[read..read + block])?;
            if count == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            read += count;
        }
        file.check(&fd)?;
        output.push(RetainedSourceRange { bytes });
    }
    // Earlier files must still match after the final payload was consumed.
    for request in requests {
        shared.context.check()?;
        let fd = request.file.file.read_handle()?;
        request.file.file.check(&fd)?;
    }
    shared.context.check()?;
    Ok(output)
}
pub(super) fn read_chunk(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    #[cfg(test)]
    if let Some(result) = tests::read_chunk(file, buffer, offset) {
        return result;
    }
    positional_read(file, buffer, offset)
}
#[cfg(test)]
mod tests;
