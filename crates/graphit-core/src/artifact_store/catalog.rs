//! Atomic catalogue persistence, serialized only while a writer updates its entries.
use super::*;
use std::io::{Read, Write};
pub(super) fn read(store: &ArtifactStore) -> io::Result<Option<Document>> {
    let file = match fs::File::open(store.manifest_path()) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut bytes = Vec::new();
    file.take(store.limits.max_document_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > store.limits.max_document_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let document: Document = serde_json::from_slice(&bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    document.validate(store)?;
    Ok(Some(document))
}
pub(super) fn save(store: &ArtifactStore, document: &Document) -> io::Result<()> {
    document.validate(store)?;
    let bytes = serde_json::to_vec(document).map_err(io::Error::other)?;
    if bytes.len() > store.limits.max_document_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut temp = tempfile::NamedTempFile::new_in(store.runtime_root())?;
    temp.write_all(&bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(store.manifest_path()).map_err(|e| e.error)?;
    Ok(())
}
