//! Completion providers are independent of native Agent, Skill and MCP hosts.

use std::{fmt, str::FromStr};

/// The selected CLI family; this does not attest an executable or grant authority.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CliCompletionProvider {
    Antigravity,
    Claude,
    Codex,
    Copilot,
    Cursor,
    Grok,
    Kimi,
    KiroV3,
    OpenCode,
    Qwen,
}

impl CliCompletionProvider {
    pub const ALL: [Self; 10] = [
        Self::Antigravity,
        Self::Claude,
        Self::Codex,
        Self::Copilot,
        Self::Cursor,
        Self::Grok,
        Self::Kimi,
        Self::KiroV3,
        Self::OpenCode,
        Self::Qwen,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Antigravity => "antigravity",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Cursor => "cursor",
            Self::Grok => "grok",
            Self::Kimi => "kimi",
            Self::KiroV3 => "kiro-v3",
            Self::OpenCode => "opencode",
            Self::Qwen => "qwen",
        }
    }

    /// The native executable name, never a discovery or fallback instruction.
    pub const fn executable(self) -> &'static str {
        match self {
            Self::Antigravity => "agy",
            Self::Cursor => "cursor-agent",
            Self::KiroV3 => "kiro-cli",
            provider => provider.as_str(),
        }
    }

    /// Classify one explicitly selected executable basename, without resolving it.
    pub fn for_executable(name: &str) -> Result<Self, InvalidCliCompletionProvider> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.executable() == name)
            .ok_or(InvalidCliCompletionProvider)
    }
}

impl fmt::Display for CliCompletionProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CliCompletionProvider {
    type Err = InvalidCliCompletionProvider;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.as_str() == value)
            .ok_or(InvalidCliCompletionProvider)
    }
}

/// An unsupported identity; caller input is deliberately absent from diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidCliCompletionProvider;

impl fmt::Display for InvalidCliCompletionProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unsupported CLI completion provider")
    }
}

impl std::error::Error for InvalidCliCompletionProvider {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn provider_identity_and_explicit_executable_are_distinct_and_unique() {
        let mut identities = HashSet::new();
        let mut executables = HashSet::new();
        for provider in CliCompletionProvider::ALL {
            assert!(identities.insert(provider.as_str()));
            assert!(executables.insert(provider.executable()));
            assert_eq!(provider.as_str().parse(), Ok(provider));
            assert_eq!(
                CliCompletionProvider::for_executable(provider.executable()),
                Ok(provider)
            );
        }
        assert_eq!(CliCompletionProvider::Antigravity.executable(), "agy");
        assert_eq!(CliCompletionProvider::Cursor.executable(), "cursor-agent");
        assert_eq!(CliCompletionProvider::KiroV3.executable(), "kiro-cli");
        assert_eq!(CliCompletionProvider::Copilot.executable(), "copilot");
        assert_eq!(CliCompletionProvider::Grok.executable(), "grok");
    }

    #[test]
    fn unknown_selection_never_becomes_a_provider_or_an_alias() {
        for value in [
            "", "unknown", "gemini", "Copilot", " copilot", "copilot ", "kiro",
        ] {
            assert_eq!(
                value.parse::<CliCompletionProvider>(),
                Err(InvalidCliCompletionProvider)
            );
        }
        for value in [
            "",
            "unknown",
            "gemini",
            "cursor",
            "antigravity",
            "kiro-v3",
            "copilot --help",
        ] {
            assert_eq!(
                CliCompletionProvider::for_executable(value),
                Err(InvalidCliCompletionProvider)
            );
        }
        assert!("agy".parse::<CliCompletionProvider>().is_err());
        assert!("cursor-agent".parse::<CliCompletionProvider>().is_err());
        assert!("kiro-cli".parse::<CliCompletionProvider>().is_err());
        assert_eq!(
            InvalidCliCompletionProvider.to_string(),
            "unsupported CLI completion provider"
        );
    }
}
