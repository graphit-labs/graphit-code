//! Typed native MCP fields; publication and ownership belong to the shared writer.
use super::{DocumentFormat, FieldValue, NativeBinding};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};
use url::Url;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeServerSpec {
    pub name: String,
    pub transport: NativeServerTransport,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeServerTransport {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default)]
        env_vars: Vec<String>,
        cwd: Option<String>,
    },
    Http {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        bearer_token_env: Option<String>,
    },
    Sse {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum McpConfigError {
    InvalidName,
    InvalidTransport,
    UnsupportedTransport,
    UnsupportedOption,
}
impl fmt::Display for McpConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidName => "invalid MCP server name",
            Self::InvalidTransport => "invalid MCP transport",
            Self::UnsupportedTransport => "MCP transport is unsupported by this binding",
            Self::UnsupportedOption => "MCP option is unsupported by this binding",
        })
    }
}
impl std::error::Error for McpConfigError {}

/// Exact managed leaf paths. Foreign values are never converted through JSON.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedField {
    pub(crate) path: Vec<String>,
    pub(crate) value: serde_json::Value,
}
impl ManagedField {
    pub(crate) fn native_value(&self) -> Result<FieldValue, McpConfigError> {
        fn value(input: &serde_json::Value) -> Result<FieldValue, McpConfigError> {
            use serde_json::Value;
            Ok(match input {
                Value::String(v) => FieldValue::String(v.clone()),
                Value::Bool(v) => FieldValue::Boolean(*v),
                Value::Number(v) => {
                    FieldValue::Integer(v.as_i64().ok_or(McpConfigError::InvalidTransport)?)
                }
                Value::Array(v) => {
                    FieldValue::Array(v.iter().map(value).collect::<Result<_, _>>()?)
                }
                Value::Object(v) => FieldValue::Object(
                    v.iter()
                        .map(|(k, v)| Ok((k.clone(), value(v)?)))
                        .collect::<Result<_, McpConfigError>>()?,
                ),
                Value::Null => return Err(McpConfigError::InvalidTransport),
            })
        }
        value(&self.value)
    }
}
#[derive(Clone)]
pub struct NativeMcpPlan {
    pub(crate) binding: NativeBinding,
    pub(crate) format: DocumentFormat,
    pub(crate) entry: Vec<String>,
    pub(crate) fields: Vec<ManagedField>,
}
impl NativeServerSpec {
    pub fn graphit(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: NativeServerTransport::Stdio {
                command: "graphit".into(),
                args: vec!["mcp".into(), "--stdio".into()],
                env: BTreeMap::new(),
                env_vars: Vec::new(),
                cwd: None,
            },
        }
    }
    pub fn plan(&self, binding: NativeBinding) -> Result<NativeMcpPlan, McpConfigError> {
        if self.name.is_empty()
            || self
                .name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\')
        {
            return Err(McpConfigError::InvalidName);
        }
        let root = match binding {
            NativeBinding::Codex => "mcp_servers",
            NativeBinding::OpenCode => "mcp",
            _ => "mcpServers",
        };
        let mut plan = NativeMcpPlan {
            binding,
            format: if binding == NativeBinding::Codex {
                DocumentFormat::Toml
            } else {
                DocumentFormat::Json
            },
            entry: if binding == NativeBinding::OpenCode {
                vec![root.into(), "servers".into(), self.name.clone()]
            } else {
                vec![root.into(), self.name.clone()]
            },
            fields: Vec::new(),
        };
        let mut add = |suffix: &[&str], value: serde_json::Value| {
            let mut path = plan.entry.clone();
            path.extend(suffix.iter().map(|v| (*v).into()));
            plan.fields.push(ManagedField { path, value });
        };
        use NativeBinding as B;
        match &self.transport {
            NativeServerTransport::Stdio {
                command,
                args,
                env,
                env_vars,
                cwd,
            } => {
                if command.is_empty()
                    || command.contains('\0')
                    || args.iter().any(|v| v.contains('\0'))
                    || env
                        .iter()
                        .any(|(k, v)| !environment_name(k) || v.contains('\0'))
                    || env_vars.iter().any(|name| !environment_name(name))
                    || cwd
                        .as_ref()
                        .is_some_and(|v| v.is_empty() || v.contains('\0'))
                {
                    return Err(McpConfigError::InvalidTransport);
                }
                if cwd.is_some() && !matches!(binding, B::Codex | B::Qwen | B::OpenCode | B::Kimi) {
                    return Err(McpConfigError::UnsupportedOption);
                }
                if !env_vars.is_empty() && binding != B::Codex {
                    return Err(McpConfigError::UnsupportedOption);
                }
                match binding {
                    B::OpenCode => {
                        add(&["type"], "local".into());
                        let argv = std::iter::once(command.clone())
                            .chain(args.iter().cloned())
                            .collect::<Vec<_>>();
                        add(&["command"], serde_json::json!(argv));
                    }
                    _ => {
                        if matches!(binding, B::Claude | B::Cursor) {
                            add(&["type"], "stdio".into());
                        }
                        add(&["command"], command.clone().into());
                        add(&["args"], serde_json::json!(args));
                    }
                }
                let env_key = if binding == B::OpenCode {
                    "environment"
                } else {
                    "env"
                };
                for (key, value) in env {
                    add(&[env_key, key], value.clone().into());
                }
                if !env_vars.is_empty() {
                    add(&["env_vars"], serde_json::json!(env_vars));
                }
                if let Some(cwd) = cwd {
                    add(&["cwd"], cwd.clone().into());
                }
            }
            NativeServerTransport::Http {
                url,
                headers,
                bearer_token_env,
            } => {
                endpoint(url)?;
                validate_headers(headers)?;
                if bearer_token_env
                    .as_ref()
                    .is_some_and(|v| !environment_name(v))
                {
                    return Err(McpConfigError::InvalidTransport);
                }
                if bearer_token_env.is_some() && binding != B::Codex {
                    return Err(McpConfigError::UnsupportedOption);
                }
                match binding {
                    B::Claude => add(&["type"], "http".into()),
                    B::OpenCode => add(&["type"], "remote".into()),
                    _ => {}
                }
                let key = match binding {
                    B::Qwen => "httpUrl",
                    B::Antigravity => "serverUrl",
                    _ => "url",
                };
                add(&[key], url.clone().into());
                let key = if binding == B::Codex {
                    "http_headers"
                } else {
                    "headers"
                };
                for (name, value) in headers {
                    add(&[key, name], value.clone().into());
                }
                if let Some(name) = bearer_token_env {
                    add(&["bearer_token_env_var"], name.clone().into());
                }
            }
            NativeServerTransport::Sse { url, headers } => {
                endpoint(url)?;
                validate_headers(headers)?;
                match binding {
                    B::Claude => add(&["type"], "sse".into()),
                    B::Kimi => add(&["transport"], "sse".into()),
                    B::Qwen => {}
                    _ => return Err(McpConfigError::UnsupportedTransport),
                }
                add(&["url"], url.clone().into());
                for (name, value) in headers {
                    add(&["headers", name], value.clone().into());
                }
            }
        }
        Ok(plan)
    }
}
fn environment_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
fn endpoint(value: &str) -> Result<(), McpConfigError> {
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(McpConfigError::InvalidTransport);
    }
    let url = Url::parse(value).map_err(|_| McpConfigError::InvalidTransport)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.has_host()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(McpConfigError::InvalidTransport);
    }
    Ok(())
}
fn validate_headers(headers: &BTreeMap<String, String>) -> Result<(), McpConfigError> {
    if headers.iter().any(|(name, value)| {
        name.is_empty()
            || !name.bytes().all(|v| v.is_ascii_alphanumeric() || v == b'-')
            || value.contains(['\r', '\n', '\0'])
    }) {
        return Err(McpConfigError::InvalidTransport);
    }
    Ok(())
}
#[cfg(test)]
mod tests;

impl NativeMcpPlan {
    pub fn binding(&self) -> NativeBinding {
        self.binding
    }
    /// Selected native PROJECT placement. Kimi uses the explicitly captured global root.
    pub fn project_config_path(&self) -> Option<&'static str> {
        Some(match self.binding {
            NativeBinding::Codex => ".codex/config.toml",
            NativeBinding::Claude => ".mcp.json",
            NativeBinding::Cursor => ".cursor/mcp.json",
            NativeBinding::Qwen => ".qwen/settings.json",
            NativeBinding::OpenCode if self.format == DocumentFormat::Jsonc => "opencode.jsonc",
            NativeBinding::OpenCode => "opencode.json",
            NativeBinding::KiroV3 => ".kiro/settings/mcp.json",
            NativeBinding::Antigravity => ".agents/mcp_config.json",
            NativeBinding::Kimi => return None,
        })
    }
    pub fn entry(&self) -> &[String] {
        &self.entry
    }
    pub fn fields(&self) -> &[ManagedField] {
        &self.fields
    }
    pub fn format(&self) -> DocumentFormat {
        self.format
    }
    pub fn jsonc(mut self) -> Result<Self, McpConfigError> {
        if self.binding != NativeBinding::OpenCode {
            return Err(McpConfigError::UnsupportedOption);
        }
        self.format = DocumentFormat::Jsonc;
        Ok(self)
    }
}
impl ManagedField {
    pub fn path(&self) -> &[String] {
        &self.path
    }
    pub fn value(&self) -> &serde_json::Value {
        &self.value
    }
}
