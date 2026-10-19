//! Python identifier equivalence over one complete authored lexeme.
//! Native model callers separately prove node kind, origin and recovery.
use std::{error::Error, fmt};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythonNameError {
    Empty,
    InvalidIdentifier { byte_offset: usize },
}
impl fmt::Display for PythonNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("empty Python identifier"),
            Self::InvalidIdentifier { byte_offset } => {
                write!(formatter, "invalid Python identifier at byte {byte_offset}")
            }
        }
    }
}
impl Error for PythonNameError {}

/// Apply Python's identifier NFKC equivalence, retaining no parser authority.
pub fn canonical_identifier(lexical: &str) -> Result<String, PythonNameError> {
    let mut characters = lexical.char_indices();
    let Some((_, first)) = characters.next() else {
        return Err(PythonNameError::Empty);
    };
    if first != '_' && !unicode_ident::is_xid_start(first) {
        return Err(PythonNameError::InvalidIdentifier { byte_offset: 0 });
    }
    for (byte_offset, character) in characters {
        if !unicode_ident::is_xid_continue(character) {
            return Err(PythonNameError::InvalidIdentifier { byte_offset });
        }
    }
    Ok(lexical.nfkc().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_equivalence_uses_mature_unicode_tables_without_casefold_or_repair() {
        for (authored, expected) in [
            ("K", "K"),
            ("K", "K"),
            ("Ａ", "A"),
            ("a\u{301}", "á"),
            ("π_1", "π_1"),
            ("_", "_"),
            ("_private", "_private"),
            ("__init__", "__init__"),
        ] {
            assert_eq!(canonical_identifier(authored), Ok(expected.to_owned()));
        }
        assert_ne!(canonical_identifier("Name"), canonical_identifier("name"));
        assert_eq!(canonical_identifier(""), Err(PythonNameError::Empty));
        for authored in ["1name", "a.b", "a b", "'text'", "a\\u0061"] {
            assert!(matches!(
                canonical_identifier(authored),
                Err(PythonNameError::InvalidIdentifier { .. })
            ));
        }
        assert_eq!(
            canonical_identifier("π.α"),
            Err(PythonNameError::InvalidIdentifier { byte_offset: 2 })
        );
    }
}
