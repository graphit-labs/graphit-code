//! Canonical Knowledge source identities are ordinary data, never capture authority.
use std::{
    collections::TryReserveError,
    error::Error,
    fmt::{self, Write},
    num::NonZeroUsize,
    path::{Component, Path},
};

const PREFIX: &str = "knowledge-source";

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
    Overflow,
    Allocation(TryReserveError),
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
            Self::Overflow => f.write_str("Knowledge source identity arithmetic overflow"),
            Self::Allocation(_) => f.write_str("Knowledge source identity allocation failed"),
        }
    }
}
impl Error for KnowledgeSourceIdentityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Allocation(e) => Some(e),
            _ => None,
        }
    }
}
type Result<T> = std::result::Result<T, KnowledgeSourceIdentityError>;
fn bounded(bytes: usize, cap: NonZeroUsize, field: &'static str) -> Result<()> {
    if bytes > cap.get() {
        return Err(KnowledgeSourceIdentityError::LimitExceeded(field));
    }
    Ok(())
}
fn reserve<T>(items: &mut Vec<T>, count: usize) -> Result<()> {
    items
        .try_reserve_exact(count)
        .map_err(KnowledgeSourceIdentityError::Allocation)
}
fn add(left: usize, right: usize) -> Result<usize> {
    left.checked_add(right)
        .ok_or(KnowledgeSourceIdentityError::Overflow)
}
fn copy(value: &str) -> Result<String> {
    let mut output = String::new();
    output
        .try_reserve_exact(value.len())
        .map_err(KnowledgeSourceIdentityError::Allocation)?;
    output.push_str(value);
    Ok(output)
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
impl KnowledgeSourceIdentity {
    /// Encode relative components; an authored id is supplied literal data.
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
        let count = relative.components().count();
        if count == 0 {
            return Err(KnowledgeSourceIdentityError::InvalidRelativePath);
        }
        relative
            .to_str()
            .ok_or(KnowledgeSourceIdentityError::NonUnicodePath)?;
        let mut encoded_bytes = 0usize;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(KnowledgeSourceIdentityError::InvalidRelativePath);
            };
            let bytes = name
                .to_str()
                .ok_or(KnowledgeSourceIdentityError::NonUnicodePath)?
                .len();
            encoded_bytes = add(
                encoded_bytes,
                bytes
                    .checked_mul(2)
                    .ok_or(KnowledgeSourceIdentityError::Overflow)?,
            )?;
        }
        let suffix_bytes = add(encoded_bytes, count - 1)?;
        let origin_bytes = add(PREFIX.len() + 3, suffix_bytes)?;
        bounded(origin_bytes, max_field_bytes, "source_path")?;
        let default_bytes = add("source-".len() + 2, suffix_bytes)?;
        if authored_id.is_none() {
            bounded(default_bytes, max_field_bytes, "slug")?;
        }
        let mut source_path = String::new();
        source_path
            .try_reserve_exact(origin_bytes)
            .map_err(KnowledgeSourceIdentityError::Allocation)?;
        source_path.push_str(PREFIX);
        source_path.push('/');
        source_path.push('t');
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(KnowledgeSourceIdentityError::InvalidRelativePath);
            };
            let bytes = name
                .to_str()
                .ok_or(KnowledgeSourceIdentityError::NonUnicodePath)?
                .as_bytes();
            if !valid_bytes(bytes) {
                return Err(KnowledgeSourceIdentityError::InvalidRelativePath);
            }
            source_path.push('/');
            for byte in bytes.iter() {
                write!(source_path, "{byte:02x}").expect("formatting into String cannot fail");
            }
        }
        let slug = if let Some(id) = authored_id {
            copy(id)?
        } else {
            let mut output = String::new();
            output
                .try_reserve_exact(default_bytes)
                .map_err(KnowledgeSourceIdentityError::Allocation)?;
            output.push_str("source-");
            output.push('t');
            for part in source_path[(PREFIX.len() + 3)..].split('/') {
                output.push('-');
                output.push_str(part);
            }
            output
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
    let mut output = Vec::new();
    reserve(&mut output, value.len() / 2)?;
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
    if mode != "t" {
        return Err(KnowledgeSourceIdentityError::InvalidManagedSourcePath);
    }
    let mut output = Vec::new();
    reserve(&mut output, body.split('/').count())?;
    for part in body.split('/') {
        let bytes = unhex(part)?;
        if !valid_bytes(&bytes) {
            return Err(KnowledgeSourceIdentityError::InvalidManagedSourcePath);
        }
        output.push(
            String::from_utf8(bytes)
                .map_err(|_| KnowledgeSourceIdentityError::InvalidManagedSourcePath)?,
        );
    }
    Ok(Some(KnowledgeSourcePathComponents::Portable(output)))
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
