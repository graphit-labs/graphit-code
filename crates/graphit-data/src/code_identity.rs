//! Compact Code identities are data; they do not restore a capture or grant access.
use crate::GraphAnchorKey;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::{error::Error, fmt, str::FromStr};

pub const CODE_ID_TEXT_BYTES: usize = 67;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeIdentityError {
    InvalidLogicalPath,
    InvalidDeclaration,
    InvalidDataKey,
    InvalidRelation,
    InvalidText,
}
impl fmt::Display for CodeIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLogicalPath => "invalid logical Code path",
            Self::InvalidDeclaration => "invalid Code declaration identity",
            Self::InvalidDataKey => "invalid Code data key",
            Self::InvalidRelation => "invalid Code relation identity",
            Self::InvalidText => "invalid compact Code identity",
        })
    }
}
impl Error for CodeIdentityError {}

/// SHA-256 of logical path components, never file content or a physical path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CodeFileId([u8; 32]);
impl CodeFileId {
    pub fn from_components(components: &[&str]) -> Result<Self, CodeIdentityError> {
        if components.is_empty()
            || components.iter().any(|part| {
                part.is_empty()
                    || matches!(*part, "." | "..")
                    || part.as_bytes().iter().any(|b| matches!(b, 0 | b'/'))
            })
        {
            return Err(CodeIdentityError::InvalidLogicalPath);
        }
        let mut hash = Sha256::new();
        field(&mut hash, b"graphit-code-file-1")?;
        for component in components {
            field(&mut hash, component.as_bytes())?;
        }
        Ok(Self(hash.finalize().into()))
    }
    pub fn hint(self) -> [u8; 16] {
        let mut hint = [0; 16];
        hint.copy_from_slice(&self.0[..16]);
        hint
    }
}

/// Actual logical scope observations; a scope need not create a Graph row.
pub enum CodeScope<'a> {
    Declaration(CodeUid),
    /// The producer validates this original global SCIP symbol with the official parser.
    ScipSymbol(&'a str),
}

/// The caller supplies observed semantic fields, not positions or parser addresses.
pub enum CodeDeclaration<'a> {
    /// A nonempty, sorted, distinct set of validated original global SCIP symbols.
    /// Local symbols use the logical fallback.
    Scip {
        symbols: &'a [&'a str],
        kind: &'a str,
        ordinal: u64,
    },
    Fallback {
        language: &'a str,
        kind: &'a str,
        owners: &'a [CodeScope<'a>],
        name: &'a str,
        declared_type: Option<&'a str>,
        anonymous: bool,
        ordinal: u64,
    },
}

/// FILE hint plus the complete SHA-256 declaration digest. Opaque to UI callers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CodeUid([u8; 48]);
impl CodeUid {
    pub fn derive(
        file: CodeFileId,
        declaration: CodeDeclaration<'_>,
    ) -> Result<Self, CodeIdentityError> {
        let mut hash = Sha256::new();
        field(&mut hash, b"graphit-code-declaration-1")?;
        field(&mut hash, &file.0)?;
        match declaration {
            CodeDeclaration::Scip {
                symbols,
                kind,
                ordinal,
            } => {
                if symbols.is_empty()
                    || symbols.iter().any(|symbol| symbol.is_empty())
                    || symbols.windows(2).any(|pair| pair[0] >= pair[1])
                    || kind.is_empty()
                {
                    return Err(CodeIdentityError::InvalidDeclaration);
                }
                field(&mut hash, b"scip-global")?;
                let count = u64::try_from(symbols.len())
                    .map_err(|_| CodeIdentityError::InvalidDeclaration)?;
                field(&mut hash, &count.to_le_bytes())?;
                for symbol in symbols {
                    field(&mut hash, symbol.as_bytes())?;
                }
                field(&mut hash, kind.as_bytes())?;
                field(&mut hash, &ordinal.to_le_bytes())?;
            }
            CodeDeclaration::Fallback {
                language,
                kind,
                owners,
                name,
                declared_type,
                anonymous,
                ordinal,
            } => {
                if language.is_empty() || kind.is_empty() {
                    return Err(CodeIdentityError::InvalidDeclaration);
                }
                field(&mut hash, b"logical-fallback")?;
                field(&mut hash, language.as_bytes())?;
                field(&mut hash, kind.as_bytes())?;
                let count = u64::try_from(owners.len())
                    .map_err(|_| CodeIdentityError::InvalidDeclaration)?;
                field(&mut hash, &count.to_le_bytes())?;
                for owner in owners {
                    match owner {
                        CodeScope::Declaration(uid) => {
                            field(&mut hash, b"declaration")?;
                            field(&mut hash, &uid.0)?;
                        }
                        CodeScope::ScipSymbol(symbol) => {
                            if symbol.is_empty() {
                                return Err(CodeIdentityError::InvalidDeclaration);
                            }
                            field(&mut hash, b"scip-scope")?;
                            field(&mut hash, symbol.as_bytes())?;
                        }
                    }
                }
                field(&mut hash, &[u8::from(anonymous)])?;
                field(&mut hash, name.as_bytes())?;
                field(&mut hash, &[u8::from(declared_type.is_some())])?;
                if let Some(declared_type) = declared_type {
                    field(&mut hash, declared_type.as_bytes())?;
                }
                field(&mut hash, &ordinal.to_le_bytes())?;
            }
        }
        let mut bytes = [0; 48];
        bytes[..16].copy_from_slice(&file.hint());
        bytes[16..].copy_from_slice(&hash.finalize());
        Ok(Self(bytes))
    }
    pub fn file_hint(self) -> [u8; 16] {
        hint(&self.0)
    }
}
impl fmt::Display for CodeUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display(f, "u1:", &self.0)
    }
}
impl FromStr for CodeUid {
    type Err = CodeIdentityError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse(text, "u1:").map(Self)
    }
}

/// A projection key; the literal data_key remains a separate observed property.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CodePrimaryKey([u8; 48]);
impl CodePrimaryKey {
    pub fn derive(uid: CodeUid, data_key: &str) -> Result<Self, CodeIdentityError> {
        if data_key.is_empty() || data_key.as_bytes().contains(&0) {
            return Err(CodeIdentityError::InvalidDataKey);
        }
        let mut hash = Sha256::new();
        field(&mut hash, b"graphit-code-projection-1")?;
        field(&mut hash, &uid.0)?;
        field(&mut hash, data_key.as_bytes())?;
        let mut bytes = [0; 48];
        bytes[..16].copy_from_slice(&uid.file_hint());
        bytes[16..].copy_from_slice(&hash.finalize());
        Ok(Self(bytes))
    }
    pub fn file_hint(self) -> [u8; 16] {
        hint(&self.0)
    }
}
impl fmt::Display for CodePrimaryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display(f, "k1:", &self.0)
    }
}
impl FromStr for CodePrimaryKey {
    type Err = CodeIdentityError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse(text, "k1:").map(Self)
    }
}

/// Identity assigned to an observed reference before its target is resolved.
/// Endpoints are literal typed SDK keys; their text is never reinterpreted.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CodeRelationOccurrence([u8; 48]);
impl CodeRelationOccurrence {
    pub fn derive(
        file: CodeFileId,
        source: GraphAnchorKey<'_>,
        kind: &str,
        query_data_key: Option<&str>,
        target_identity: &str,
        ordinal: u64,
    ) -> Result<Self, CodeIdentityError> {
        if kind.is_empty() || kind.as_bytes().contains(&0) {
            return Err(CodeIdentityError::InvalidRelation);
        }
        if query_data_key.is_some_and(|key| key.is_empty() || key.as_bytes().contains(&0)) {
            return Err(CodeIdentityError::InvalidDataKey);
        }
        let mut hash = Sha256::new();
        field(&mut hash, b"graphit-code-reference-1")?;
        field(&mut hash, &file.0)?;
        key(&mut hash, source)?;
        field(&mut hash, kind.as_bytes())?;
        field(&mut hash, &[u8::from(query_data_key.is_some())])?;
        if let Some(key) = query_data_key {
            field(&mut hash, key.as_bytes())?;
        }
        field(&mut hash, target_identity.as_bytes())?;
        field(&mut hash, &ordinal.to_le_bytes())?;
        let mut bytes = [0; 48];
        bytes[..16].copy_from_slice(&file.hint());
        bytes[16..].copy_from_slice(&hash.finalize());
        Ok(Self(bytes))
    }
    pub fn file_hint(self) -> [u8; 16] {
        hint(&self.0)
    }
}
impl fmt::Display for CodeRelationOccurrence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display(f, "o1:", &self.0)
    }
}
impl FromStr for CodeRelationOccurrence {
    type Err = CodeIdentityError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse(text, "o1:").map(Self)
    }
}

/// A resolved logical relation, distinct from its physical member/row address.
/// Duplicate UIDs in generic artifacts still represent separate physical edges.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CodeRelationUid([u8; 48]);
impl CodeRelationUid {
    pub fn derive(
        file: CodeFileId,
        source: GraphAnchorKey<'_>,
        target: GraphAnchorKey<'_>,
        kind: &str,
        occurrence: CodeRelationOccurrence,
    ) -> Result<Self, CodeIdentityError> {
        if kind.is_empty() || kind.as_bytes().contains(&0) {
            return Err(CodeIdentityError::InvalidRelation);
        }
        let mut hash = Sha256::new();
        field(&mut hash, b"graphit-code-relation-1")?;
        field(&mut hash, &file.0)?;
        key(&mut hash, source)?;
        key(&mut hash, target)?;
        field(&mut hash, kind.as_bytes())?;
        field(&mut hash, &occurrence.0)?;
        let mut bytes = [0; 48];
        bytes[..16].copy_from_slice(&file.hint());
        bytes[16..].copy_from_slice(&hash.finalize());
        Ok(Self(bytes))
    }
    pub fn file_hint(self) -> [u8; 16] {
        hint(&self.0)
    }
}
impl fmt::Display for CodeRelationUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display(f, "r1:", &self.0)
    }
}
impl FromStr for CodeRelationUid {
    type Err = CodeIdentityError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse(text, "r1:").map(Self)
    }
}

fn key(hash: &mut Sha256, value: GraphAnchorKey<'_>) -> Result<(), CodeIdentityError> {
    match value {
        GraphAnchorKey::String(value) => {
            field(hash, b"string")?;
            field(hash, value.as_bytes())
        }
        GraphAnchorKey::Int64(value) => {
            field(hash, b"int64")?;
            field(hash, &value.to_le_bytes())
        }
    }
}

fn field(hash: &mut Sha256, value: &[u8]) -> Result<(), CodeIdentityError> {
    let length = u64::try_from(value.len()).map_err(|_| CodeIdentityError::InvalidDeclaration)?;
    hash.update(length.to_le_bytes());
    hash.update(value);
    Ok(())
}
fn hint(bytes: &[u8; 48]) -> [u8; 16] {
    let mut hint = [0; 16];
    hint.copy_from_slice(&bytes[..16]);
    hint
}
fn display(f: &mut fmt::Formatter<'_>, prefix: &str, bytes: &[u8; 48]) -> fmt::Result {
    let mut encoded = [0; 64];
    URL_SAFE_NO_PAD
        .encode_slice(bytes, &mut encoded)
        .map_err(|_| fmt::Error)?;
    f.write_str(prefix)?;
    f.write_str(std::str::from_utf8(&encoded).map_err(|_| fmt::Error)?)
}
fn parse(text: &str, prefix: &str) -> Result<[u8; 48], CodeIdentityError> {
    if text.len() != CODE_ID_TEXT_BYTES || !text.starts_with(prefix) {
        return Err(CodeIdentityError::InvalidText);
    }
    let mut bytes = [0; 48];
    let length = URL_SAFE_NO_PAD
        .decode_slice(&text[prefix.len()..], &mut bytes)
        .map_err(|_| CodeIdentityError::InvalidText)?;
    if length != bytes.len() {
        return Err(CodeIdentityError::InvalidText);
    }
    Ok(bytes)
}
