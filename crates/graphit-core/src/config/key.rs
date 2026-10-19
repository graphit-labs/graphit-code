//! Canonical keys, collision-free environment names and shared scope policy.
use crate::brand::Brand;
use crate::local_files::invalid_input;
use std::io;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ConfigKey(Arc<str>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Policy {
    Common,
    Installation,
    Environment,
}

pub const SECRET_KEYS: [&str; 2] = ["client.secret", "ui.auth.cookie_encryption_key"];

impl ConfigKey {
    pub fn new(key: &str) -> io::Result<Self> {
        let key = key.trim();
        if key.is_empty() || key.len() > 128 {
            return Err(invalid_input("configuration key exceeds its byte budget"));
        }
        let key = key.to_ascii_lowercase();
        let mut count = 0;
        for part in key.split('.') {
            count += 1;
            if count > 16
                || !part
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                || !part
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                || part.contains("__")
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                return Err(invalid_input(
                    "configuration key requires ASCII dotted identifiers",
                ));
            }
        }
        if key.len() + count - 1 > 128 {
            return Err(invalid_input(
                "configuration environment suffix exceeds its byte budget",
            ));
        }
        if reserved(&key) {
            return Err(invalid_input(if key == "ast.allow_native_grammars" {
                "ast.allow_native_grammars was removed; use ast.scip.parsers for custom compiler images"
            } else {
                "key is outside runtime configuration"
            }));
        }
        Ok(Self(Arc::from(key)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn environment_variable(&self, brand: &Brand) -> String {
        brand
            .environment_variable(&self.0.replace('.', "__").to_ascii_uppercase())
            .expect("validated key produces a valid environment suffix")
    }

    pub fn is_secret(&self) -> bool {
        SECRET_KEYS.contains(&self.as_str())
    }

    pub(crate) fn policy(&self) -> Policy {
        if self.is_secret()
            || matches!(
                self.as_str(),
                "unit.id" | "ai.cli" | "models.embedding.id" | "models.rerank.id"
            )
        {
            Policy::Installation
        } else if self.as_str() == "modules.daemon_ui" {
            Policy::Environment
        } else {
            Policy::Common
        }
    }
}

fn reserved(key: &str) -> bool {
    let namespace = key.split('.').next().unwrap_or_default();
    matches!(namespace, "auth" | "providers" | "accounts" | "settings")
        || key.starts_with("ai.agent_args.")
        || matches!(
            key,
            "global_dir"
                | "model_cache"
                | "project_version"
                | "id"
                | "name"
                | "description"
                | "cluster"
                | "ephemeral"
                | "metadata"
                | "git.hooks"
                | "ai.agent_args"
                | "ast.cluster"
                | "ast.allow_native_grammars"
                | "hub.bucket"
                | "hub.region"
                | "hub.endpoint"
                | "hub.prefix"
                | "hub.access_key_id"
                | "hub.secret_access_key"
                | "hub.subject.user"
                | "hub.subject.teams"
                | "mcp.api_key"
                | "ai.embedding.provider"
                | "ai.embedding.model"
                | "ai.embedding.base_url"
                | "ai.embedding.api_key"
                | "ai.embedding.dimensions"
                | "ai.rerank.provider"
                | "ai.rerank.model"
                | "ai.rerank.base_url"
                | "ai.rerank.api_key"
        )
}
