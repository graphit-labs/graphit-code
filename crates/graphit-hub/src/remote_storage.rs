//! Current remote identity keys and incremental payload hashes. Neither grants access.
use crate::{ArtifactId, ArtifactType, ConcreteVersion};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    io::{self, Read},
};

const S3_KEY_BYTES: usize = 1024;

fn checked_key(key: String) -> io::Result<String> {
    if key.len() > S3_KEY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "remote key exceeds S3's 1024-byte limit",
        ));
    }
    Ok(key)
}

/// Key for the complete current manifest of one producer/type/full artifact ID.
/// The consumer must also validate its configured provider prefix against S3's key limit.
pub fn registry_key(
    producer: ProjectId,
    kind: ArtifactType,
    id: &ArtifactId,
) -> io::Result<String> {
    let mut key = format!("v1/projects/{producer}/registry/{}/", kind.as_str());
    URL_SAFE_NO_PAD.encode_string(id.as_str().as_bytes(), &mut key);
    key.push_str(".json");
    checked_key(key)
}

/// Immutable release prefix with its trailing separator. Validate the complete object key
/// after adding provider prefix and an object name. Native release digests use their own contract.
pub fn release_prefix(
    producer: ProjectId,
    kind: ArtifactType,
    id: &ArtifactId,
    version: &ConcreteVersion,
    digest: &[u8; 32],
) -> io::Result<String> {
    let mut key = format!("v1/projects/{producer}/artifacts/{}/", kind.as_str());
    URL_SAFE_NO_PAD.encode_string(id.as_str().as_bytes(), &mut key);
    key.push('/');
    URL_SAFE_NO_PAD.encode_string(version.as_str().as_bytes(), &mut key);
    key.push('/');
    for byte in digest {
        write!(key, "{byte:02x}").expect("writing to a String is infallible");
    }
    key.push('/');
    checked_key(key)
}

/// Hash bytes from the reader's current position through EOF with the original I/O context.
/// For publication, finish the ZIP and rewind its file first: ZIP headers can be rewritten.
/// Cancellation is cooperative between reads; this does not interrupt an already blocked Read.
pub fn hash_reader(reader: &mut impl Read, original: &SnapshotContext) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        original.check()?;
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(length) => hash.update(&buffer[..length]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    original.check()?;
    Ok(hash.finalize().into())
}
