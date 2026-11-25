//! Canonical Knowledge source identities are ordinary data, never capture authority.
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fmt::{self, Write},
    num::NonZeroUsize,
    path::{Component, Path},
};

const PREFIX: &str = "knowledge-source";
const READABLE_PREFIX: &str = "knowledge-source/path/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeSourceIdentity {
    slug: String,
    source_path: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnowledgeSourcePathComponents {
    Portable(Vec<String>),
}
#[derive(Debug)]
pub enum KnowledgeSourceIdentityError {
    InvalidRelativePath,
    NonUnicodePath,
    InvalidAuthoredId,
    InvalidManagedSourcePath,
    LimitExceeded(&'static str),
}
impl fmt::Display for KnowledgeSourceIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRelativePath => f.write_str("invalid relative Knowledge source path"),
            Self::NonUnicodePath => f.write_str("Knowledge source path must be Unicode"),
            Self::InvalidAuthoredId => f.write_str("invalid literal Knowledge source id"),
            Self::InvalidManagedSourcePath => {
                f.write_str("invalid canonical Knowledge source origin")
            }
            Self::LimitExceeded(field) => write!(f, "Knowledge source identity exceeds {field}"),
        }
    }
}
impl Error for KnowledgeSourceIdentityError {}
type Result<T> = std::result::Result<T, KnowledgeSourceIdentityError>;
fn bounded(bytes: usize, cap: NonZeroUsize, field: &'static str) -> Result<()> {
    if bytes > cap.get() {
        return Err(KnowledgeSourceIdentityError::LimitExceeded(field));
    }
    Ok(())
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with("..")
        && !id.chars().any(|c| c == '/' || c == '\\' || c.is_control())
}
fn valid_bytes(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes != b"."
        && bytes != b".."
        && !bytes.iter().any(|b| *b == 0 || *b == b'/')
}
fn default_slug(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"graphit.knowledge.default-slug.v1\0");
    hash.update(legacy_origin(parts.iter().copied()).as_bytes());
    let digest = hash.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    ulid::Ulid::from_bytes(bytes).to_string()
}

fn legacy_origin<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut origin = String::from("knowledge-source/t");
    for part in parts {
        origin.push('/');
        for byte in part.bytes() {
            write!(origin, "{byte:02x}").expect("formatting into String cannot fail");
        }
    }
    origin
}

fn readable_origin<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut origin = String::from(READABLE_PREFIX);
    for part in parts {
        origin.push_str(part);
        origin.push('/');
    }
    origin.pop();
    origin
}

pub(crate) fn is_readable_managed_origin(value: &str) -> bool {
    value
        .strip_prefix(READABLE_PREFIX)
        .is_some_and(|body| body.split('/').all(|part| valid_bytes(part.as_bytes())))
}

impl KnowledgeSourceIdentity {
    /// Store a readable relative origin and keep the historical default ULID.
    /// An authored id remains supplied literal data; the default encodes no creation time.
    /// This does not inspect a filesystem or authorize reading this path.
    pub fn from_relative_path(
        relative: &Path,
        authored_id: Option<&str>,
        max_field_bytes: NonZeroUsize,
    ) -> Result<Self> {
        if let Some(id) = authored_id {
            bounded(id.len(), max_field_bytes, "slug")?;
            if !valid_id(id) {
                return Err(KnowledgeSourceIdentityError::InvalidAuthoredId);
            }
        }
        let mut parts = Vec::new();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(KnowledgeSourceIdentityError::InvalidRelativePath);
            };
            let part = part
                .to_str()
                .ok_or(KnowledgeSourceIdentityError::NonUnicodePath)?;
            if !valid_bytes(part.as_bytes()) {
                return Err(KnowledgeSourceIdentityError::InvalidRelativePath);
            }
            parts.push(part);
        }
        if parts.is_empty() {
            return Err(KnowledgeSourceIdentityError::InvalidRelativePath);
        }
        let source_path = readable_origin(parts.iter().copied());
        bounded(source_path.len(), max_field_bytes, "source_path")?;
        let slug = match authored_id {
            Some(id) => id.to_owned(),
            None => {
                bounded(ulid::ULID_LEN, max_field_bytes, "slug")?;
                default_slug(&parts)
            }
        };
        Ok(Self { slug, source_path })
    }
    pub fn slug(&self) -> &str {
        &self.slug
    }
    pub fn source_path(&self) -> &str {
        &self.source_path
    }
}
fn unhex(value: &str) -> Result<Vec<u8>> {
    if value.is_empty()
        || !value.len().is_multiple_of(2)
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(KnowledgeSourceIdentityError::InvalidManagedSourcePath);
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().as_chunks::<2>().0.iter() {
        let text = std::str::from_utf8(pair)
            .map_err(|_| KnowledgeSourceIdentityError::InvalidManagedSourcePath)?;
        output.push(
            u8::from_str_radix(text, 16)
                .map_err(|_| KnowledgeSourceIdentityError::InvalidManagedSourcePath)?,
        );
    }
    Ok(output)
}
/// Decode the exact reserved data class, without restoring a native capture.
/// Unrelated origins return None; malformed reserved values are errors.
pub fn decode_managed_knowledge_source_path(
    value: &str,
    max_field_bytes: NonZeroUsize,
) -> Result<Option<KnowledgeSourcePathComponents>> {
    bounded(value.len(), max_field_bytes, "source_path")?;
    parse_managed_knowledge_source_path(value)
}
fn parse_managed_knowledge_source_path(
    value: &str,
) -> Result<Option<KnowledgeSourcePathComponents>> {
    if value == PREFIX {
        return Err(KnowledgeSourceIdentityError::InvalidManagedSourcePath);
    }
    let Some(rest) = value.strip_prefix("knowledge-source/") else {
        return Ok(None);
    };
    let (mode, body) = rest
        .split_once('/')
        .ok_or(KnowledgeSourceIdentityError::InvalidManagedSourcePath)?;
    if mode != "t" && mode != "path" {
        return Err(KnowledgeSourceIdentityError::InvalidManagedSourcePath);
    }
    let mut output = Vec::new();
    for part in body.split('/') {
        let part = if mode == "t" {
            String::from_utf8(unhex(part)?)
                .map_err(|_| KnowledgeSourceIdentityError::InvalidManagedSourcePath)?
        } else {
            part.to_owned()
        };
        if !valid_bytes(part.as_bytes()) {
            return Err(KnowledgeSourceIdentityError::InvalidManagedSourcePath);
        }
        output.push(part);
    }
    Ok(Some(KnowledgeSourcePathComponents::Portable(output)))
}

fn normalized_origin(value: &str) -> Result<Option<String>> {
    Ok(parse_managed_knowledge_source_path(value)?.map(
        |KnowledgeSourcePathComponents::Portable(parts)| {
            readable_origin(parts.iter().map(String::as_str))
        },
    ))
}

mod reconciliation;
pub use reconciliation::{
    KnowledgeDocumentLifecycleError, KnowledgeDocumentLifecycleLimits, KnowledgeSourceHead,
    KnowledgeSourceOrigins, knowledge_source_relative_path, read_native_knowledge_source_origins,
    reconcile_native_knowledge_source_file, remove_native_knowledge_source_origins,
};

mod source_first_publication;
pub use source_first_publication::{
    KnowledgeSourceFirstPublicationError, KnowledgeSourcePublicationEffect,
    KnowledgeSourcePublicationPhase, publish_native_knowledge_source_file,
    publish_native_knowledge_source_stream,
};
