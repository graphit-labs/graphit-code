use std::sync::Arc;

use tree_sitter::{Language, Parser};

use super::EngineError;

/// A retained language supplied by a statically linked grammar SDK.
#[derive(Clone)]
pub struct GrammarLanguage(pub(super) Arc<LanguageOwner>);

pub(super) struct LanguageOwner {
    pub(super) language: Language,
}

impl GrammarLanguage {
    /// Retain a language supplied by a statically linked grammar SDK.
    pub fn from_language(language: Language) -> Result<Self, EngineError> {
        let owner = Self(Arc::new(LanguageOwner { language }));
        let mut parser = Parser::new();
        parser
            .set_language(&owner.0.language)
            .map_err(EngineError::Language)?;
        Ok(owner)
    }

    pub(super) fn same_language(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
