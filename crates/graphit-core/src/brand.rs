//! Validated build namespace; no mutable process-wide branding.

use crate::local_files::invalid_input;
use std::io;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Brand(Arc<Namespace>);

#[derive(Debug, Eq, PartialEq)]
struct Namespace {
    key: String,
    directory: String,
    environment: String,
}

impl Default for Brand {
    fn default() -> Self {
        Self::new("graphit").expect("the default brand is valid")
    }
}

impl Brand {
    pub fn new(key: &str) -> io::Result<Self> {
        let key = key.trim();
        if key.len() > 64 {
            return Err(invalid_input("brand exceeds its byte budget"));
        }
        let key = key.to_ascii_lowercase();
        if matches!(
            key.as_str(),
            "git" | "gitignore" | "gitattributes" | "gitmodules"
        ) {
            return Err(invalid_input("brand conflicts with a Git control path"));
        }
        portable_segment(&key)?;
        if !key
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
            || !key.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(invalid_input(
                "brand must be a 1–64 byte ASCII identifier with alphanumeric edges and optional hyphens",
            ));
        }
        let directory = format!(".{key}");
        let environment = key.replace('-', "_").to_ascii_uppercase();
        Ok(Self(Arc::new(Namespace {
            key,
            directory,
            environment,
        })))
    }

    pub fn key(&self) -> &str {
        &self.0.key
    }
    pub fn directory(&self) -> &str {
        &self.0.directory
    }
    pub fn environment_prefix(&self) -> &str {
        &self.0.environment
    }
    pub fn binary_name(&self) -> &str {
        self.key()
    }
    pub fn windows_binary_name(&self) -> String {
        format!("{}.exe", self.key())
    }

    pub fn environment_variable(&self, suffix: &str) -> io::Result<String> {
        if suffix.is_empty()
            || suffix.len() > 128
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(invalid_input("environment suffix must be uppercase ASCII"));
        }
        Ok(format!("{}_{suffix}", self.environment_prefix()))
    }

    pub fn mcp_server_name(&self, module: &str) -> io::Result<String> {
        module_name(module)?;
        Ok(format!("{}-{module}-mcp", self.key()))
    }

    pub fn skill_name(&self, module: &str) -> io::Result<String> {
        module_name(module)?;
        Ok(format!("{}-{module}", self.key()))
    }

    pub fn mcp_tool_name(&self, parts: &[&str]) -> io::Result<String> {
        if parts.is_empty() || parts.len() > 64 {
            return Err(invalid_input("tool name requires at least one part"));
        }
        for part in parts {
            module_name(part)?;
        }
        if parts.iter().map(|part| part.len() + 1).sum::<usize>() > 128 {
            return Err(invalid_input("tool name exceeds its byte budget"));
        }
        Ok(format!("{}_{}", self.key(), parts.join("_")))
    }
}

fn module_name(name: &str) -> io::Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        Err(invalid_input(
            "module/tool parts must be lowercase ASCII identifiers",
        ))
    } else {
        Ok(())
    }
}

/// Reject filename components that are unsafe or ambiguous on any supported OS.
pub(crate) fn portable_segment(segment: &str) -> io::Result<()> {
    if segment.is_empty() || segment.len() > 128 {
        return Err(invalid_input("filename component exceeds its byte budget"));
    }
    let stem = segment
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            });
    if matches!(segment, "." | "..")
        || segment.ends_with(['.', ' '])
        || device
        || segment.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        })
    {
        Err(invalid_input("expected a portable filename component"))
    } else {
        Ok(())
    }
}
