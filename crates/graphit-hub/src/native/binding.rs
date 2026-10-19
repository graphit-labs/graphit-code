use crate::AdapterId;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// A selected native schema, never inferred from a launcher version.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeBinding {
    Antigravity,
    Claude,
    Codex,
    Cursor,
    KiroV3,
    #[serde(rename = "opencode")]
    OpenCode,
    Qwen,
    Kimi,
}
impl NativeBinding {
    pub const ALL: [Self; 8] = [
        Self::Antigravity,
        Self::Claude,
        Self::Codex,
        Self::Cursor,
        Self::KiroV3,
        Self::OpenCode,
        Self::Qwen,
        Self::Kimi,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Antigravity => "antigravity",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::KiroV3 => "kiro-v3",
            Self::OpenCode => "opencode",
            Self::Qwen => "qwen",
            Self::Kimi => "kimi",
        }
    }
    pub(crate) const fn agent_extension(self) -> &'static str {
        match self {
            Self::Codex => "toml",
            _ => "md",
        }
    }
    pub const fn adapter(self) -> AdapterId {
        match self {
            Self::Antigravity => AdapterId::Antigravity,
            Self::Claude => AdapterId::Claude,
            Self::Codex => AdapterId::Codex,
            Self::Cursor => AdapterId::Cursor,
            Self::KiroV3 => AdapterId::Kiro,
            Self::OpenCode => AdapterId::OpenCode,
            Self::Qwen => AdapterId::Qwen,
            Self::Kimi => AdapterId::Kimi,
        }
    }
    pub const fn schema(self) -> &'static str {
        match self {
            Self::Antigravity => "antigravity-agent-md",
            Self::Claude => "claude-agent-md",
            Self::Codex => "codex-agent-toml",
            Self::Cursor => "cursor-agent-md",
            Self::KiroV3 => "kiro-v3-agent-md",
            Self::OpenCode => "opencode-agent-md",
            Self::Qwen => "qwen-agent-md",
            Self::Kimi => "kimi-agent-md",
        }
    }
    pub(crate) fn agent_path(self, name: &str) -> String {
        let extension = self.agent_extension();
        format!("{}/agents/{name}.{extension}", self.adapter().layout().root)
    }
    pub(crate) fn skill_path(self, name: &str) -> String {
        let root = if self == Self::Codex {
            ".agents"
        } else {
            self.adapter().layout().root
        };
        format!("{root}/skills/{name}/SKILL.md")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BindingParseError;
impl fmt::Display for BindingParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown or empty native binding")
    }
}
impl std::error::Error for BindingParseError {}
impl fmt::Display for NativeBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl FromStr for NativeBinding {
    type Err = BindingParseError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.eq_ignore_ascii_case("claude-code") {
            return Ok(Self::Claude);
        }
        Self::ALL
            .into_iter()
            .find(|id| value.eq_ignore_ascii_case(id.as_str()))
            .ok_or(BindingParseError)
    }
}
