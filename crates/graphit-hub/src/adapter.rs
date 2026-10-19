use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterId {
    Antigravity,
    Claude,
    Codex,
    Cursor,
    Kiro,
    OpenCode,
    Qwen,
    Kimi,
}
/// Rules travel in the host's dynamic context, not a physical rules folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RulePlacement {
    DynamicContext,
}

/// Relative host folders, not a renderer or proof of installed native capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub root: &'static str,
    pub commands: Option<&'static str>,
    pub skills: &'static str,
    pub agents: &'static str,
    pub rules: RulePlacement,
}
impl AdapterId {
    pub const ALL: [Self; 8] = [
        Self::Antigravity,
        Self::Claude,
        Self::Codex,
        Self::Cursor,
        Self::Kiro,
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
            Self::Kiro => "kiro",
            Self::OpenCode => "opencode",
            Self::Qwen => "qwen",
            Self::Kimi => "kimi",
        }
    }
    pub const fn layout(self) -> Layout {
        let root = match self {
            Self::Antigravity => ".agents",
            Self::Claude => ".claude",
            Self::Codex => ".codex",
            Self::Cursor => ".cursor",
            Self::Kiro => ".kiro",
            Self::OpenCode => ".opencode",
            Self::Qwen => ".qwen",
            Self::Kimi => ".kimi-code",
        };
        let commands = match self {
            Self::Antigravity => Some("workflows"),
            Self::Kiro => Some("hooks"),
            Self::Kimi => None,
            _ => Some("commands"),
        };
        Layout {
            root,
            commands,
            skills: "skills",
            agents: "agents",
            rules: RulePlacement::DynamicContext,
        }
    }
}
impl fmt::Display for AdapterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdapterParseError;
impl fmt::Display for AdapterParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown or empty adapter name")
    }
}
impl std::error::Error for AdapterParseError {}
impl FromStr for AdapterId {
    type Err = AdapterParseError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.eq_ignore_ascii_case("claude-code") {
            return Ok(Self::Claude);
        }
        Self::ALL
            .into_iter()
            .find(|id| value.eq_ignore_ascii_case(id.as_str()))
            .ok_or(AdapterParseError)
    }
}
