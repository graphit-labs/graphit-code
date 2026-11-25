//! Builtin languages statically linked into the Graphit executable.
//!
//! Acquisition validates the real SDK language. Static code lives as long as
//! the executable; retained trees and queries still own their language generation.

use std::{error::Error, fmt};

use super::{EngineError, GrammarLanguage};

include!(concat!(env!("OUT_DIR"), "/tree_sitter_builtin.rs"));

#[derive(Debug)]
pub enum BuiltinGrammarError {
    Unsupported,
    Native(EngineError),
}

impl fmt::Display for BuiltinGrammarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => formatter.write_str("unsupported builtin grammar"),
            Self::Native(error) => error.fmt(formatter),
        }
    }
}

impl Error for BuiltinGrammarError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Unsupported => None,
            Self::Native(error) => Some(error),
        }
    }
}

/// Acquire a literal effective grammar key, such as `tree-sitter-rust`.
///
/// The C# keys `tree-sitter-c_sharp` and `tree-sitter-c-sharp` are explicit
/// aliases. Other spellings are unsupported; no filesystem or normalization
/// participates in selection.
pub fn builtin(id: &str) -> Result<GrammarLanguage, BuiltinGrammarError> {
    let language = acquire(id).ok_or(BuiltinGrammarError::Unsupported)?;
    GrammarLanguage::from_language(language).map_err(BuiltinGrammarError::Native)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_native_abis_match_the_closed_source_catalog() {
        assert_eq!(BUILTIN_GRAMMAR_NAMES.len(), 41);
        assert_eq!(EXPECTED_ABIS.len(), 41);
        let mut distribution = [0; 3];
        for (&name, &expected) in BUILTIN_GRAMMAR_NAMES.iter().zip(EXPECTED_ABIS) {
            let language = builtin(name).unwrap_or_else(|error| panic!("{name}: {error}"));
            let actual = language.0.language.abi_version();
            assert_eq!(actual, expected, "{name}");
            distribution[actual - 13] += 1;
        }
        assert_eq!(distribution, [1, 21, 19]);
    }

    #[test]
    fn selection_accepts_only_literal_keys_and_the_explicit_csharp_alias() {
        let canonical = builtin("tree-sitter-c_sharp").unwrap();
        let alias = builtin("tree-sitter-c-sharp").unwrap();
        assert_eq!(canonical.0.language, alias.0.language);
        for unknown in [
            "rust",
            "TREE-SITTER-RUST",
            "tree_sitter_rust",
            "tree-sitter-pl_pgsql",
            "../rust",
            "tree-sitter-unknown",
            "",
        ] {
            assert!(
                matches!(builtin(unknown), Err(BuiltinGrammarError::Unsupported)),
                "{unknown}"
            );
        }
    }
}
