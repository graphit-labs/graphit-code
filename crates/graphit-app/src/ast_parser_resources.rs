//! Shared builtin grammar owners for the effective fallback profile factories.
use graphit_ast::{
    catalog::EffectiveProfile,
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::{BuiltinGrammarError, GrammarLanguage, builtin},
    tree_sitter_profile::PreparedTreeSitterProfile,
};
use graphit_core::file_io::SnapshotContext;
use std::{collections::HashMap, io, sync::Mutex};

/// Context and query factories share the same immutable SDK language owner.
/// Request state and prepared queries remain local to their operation.
#[derive(Default)]
pub(crate) struct AstParserResources {
    languages: Mutex<HashMap<String, GrammarLanguage>>,
}

impl AstParserResources {
    fn language(
        &self,
        profile: &EffectiveProfile,
        original: &SnapshotContext,
    ) -> io::Result<GrammarLanguage> {
        original.check()?;
        let id = profile.grammar().value();
        let mut languages = self.languages.lock().unwrap_or_else(|e| e.into_inner());
        original.check()?;
        if let Some(language) = languages.get(id) {
            return Ok(language.clone());
        }
        let language = builtin(id).map_err(|cause| {
            let kind = match &cause {
                BuiltinGrammarError::Unsupported => io::ErrorKind::Unsupported,
                BuiltinGrammarError::Native(_) => io::ErrorKind::Other,
            };
            io::Error::new(kind, cause)
        })?;
        original.check()?;
        languages.insert(id.clone(), language.clone());
        Ok(language)
    }

    /// Prepare once per effective profile/operation and reuse across its FILEs.
    pub fn contexts<'profile>(
        &self,
        profile: &'profile EffectiveProfile,
        original: &SnapshotContext,
    ) -> io::Result<PreparedTreeSitterContexts<'profile>> {
        let language = self.language(profile, original)?;
        let factory = PreparedTreeSitterContexts::prepare(profile, language, original)
            .map_err(io::Error::other)?;
        original.check()?;
        Ok(factory)
    }

    pub fn profile<'profile>(
        &self,
        profile: &'profile EffectiveProfile,
        original: &SnapshotContext,
    ) -> io::Result<PreparedTreeSitterProfile<'profile>> {
        let language = self.language(profile, original)?;
        let factory = PreparedTreeSitterProfile::prepare(profile, language, original)
            .map_err(io::Error::other)?;
        original.check()?;
        Ok(factory)
    }
}

#[cfg(test)]
#[path = "ast_parser_resources_tests.rs"]
mod tests;
