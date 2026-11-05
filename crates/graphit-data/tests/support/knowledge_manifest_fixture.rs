//! Corrupt the payload of a real Lance publication without breaking framing.
use std::{fs, path::Path};

pub(super) fn corrupt_published_manifest(corpus: &Path) {
    let mut count = 0;
    for entry in fs::read_dir(corpus.join("_versions")).unwrap() {
        let manifest = entry.unwrap().path();
        if manifest.extension().is_some_and(|ext| ext == "manifest") {
            let mut bytes = fs::read(&manifest).unwrap();
            let footer = bytes.len().checked_sub(16).unwrap();
            let offset = usize::try_from(u64::from_le_bytes(
                bytes[footer..footer + 8].try_into().unwrap(),
            ))
            .unwrap();
            assert!(offset + 4 < footer);
            // Invalid protobuf tag, with the real length and footer intact.
            bytes[offset + 4] = 0;
            fs::write(manifest, bytes).unwrap();
            count += 1;
        }
    }
    assert!(count > 0, "expected at least one actual Lance manifest");
}
