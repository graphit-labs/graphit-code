//! Encoding of the Microsoft mount-point reparse layout, without struct casts.
use std::io;
const MAX_BUFFER: usize = 16384;
const NT_PREFIX: [u16; 4] = [92, 63, 63, 92];
const VERBATIM_PREFIX: [u16; 4] = [92, 92, 63, 92];

pub(super) fn encode(target: &[u16]) -> io::Result<Vec<u8>> {
    let target = target.strip_prefix(&VERBATIM_PREFIX).unwrap_or(target);
    if target.len() < 3
        || !matches!(target[0], 65..=90 | 97..=122)
        || target[1..3] != [58, 92]
        || target.contains(&0)
        || target.contains(&47)
    {
        return Err(io::Error::from(io::ErrorKind::Unsupported));
    }
    let total = target
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(28))
        .filter(|n| *n <= MAX_BUFFER)
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let substitute_len = u16::try_from((target.len() + 4) * 2)
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let print_len = u16::try_from(target.len() * 2)
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let mut buffer = Vec::with_capacity(total);
    buffer.extend_from_slice(&0xa000_0003u32.to_le_bytes());
    for field in [
        (total - 8) as u16,
        0,
        0,
        substitute_len,
        substitute_len + 2,
        print_len,
    ] {
        buffer.extend_from_slice(&field.to_le_bytes());
    }
    for unit in NT_PREFIX
        .iter()
        .chain(target)
        .chain(&[0])
        .chain(target)
        .chain(&[0])
    {
        buffer.extend_from_slice(&unit.to_le_bytes());
    }
    debug_assert_eq!(buffer.len(), total);
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_golden_layout_and_native_utf16() {
        let expected = vec![
            3, 0, 0, 160, 40, 0, 0, 0, 0, 0, 18, 0, 20, 0, 10, 0, 92, 0, 63, 0, 63, 0, 92, 0, 67,
            0, 58, 0, 92, 0, 120, 0, 121, 0, 0, 0, 67, 0, 58, 0, 92, 0, 120, 0, 121, 0, 0, 0,
        ];
        let target: Vec<_> = "C:\\xy".encode_utf16().collect();
        assert_eq!(encode(&target).unwrap(), expected);
        let verbatim: Vec<_> = "\\\\?\\C:\\xy".encode_utf16().collect();
        assert_eq!(encode(&verbatim).unwrap(), expected);
        let mut native = target;
        native.extend([0xd83d, 0xde80, 0xd800]); // unpaired native UTF-16 remains lossless
        let buffer = encode(&native).unwrap();
        assert!(
            buffer
                .windows(6)
                .any(|b| b == [0x3d, 0xd8, 0x80, 0xde, 0, 0xd8])
        );
    }
    #[test]
    fn exact_buffer_bound_and_unsupported_paths() {
        let mut target = vec![120u16; (MAX_BUFFER - 28) / 4];
        target[..3].copy_from_slice(&[67, 58, 92]);
        assert_eq!(encode(&target).unwrap().len(), MAX_BUFFER);
        target.push(120);
        assert_eq!(
            encode(&target).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        for invalid in [
            "",
            "C:relative",
            "\\\\server\\share",
            "\\\\?\\UNC\\server\\share",
            "\\\\?\\Volume{xyz}\\",
            "C:/path",
            "C:\\x\0y",
        ] {
            assert_eq!(
                encode(&invalid.encode_utf16().collect::<Vec<_>>())
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::Unsupported
            );
        }
    }
}
