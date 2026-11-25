use super::key::{ConfigKey, Policy};
use crate::local_files::{invalid_data, invalid_input};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;

impl ConfigKey {
    /// Parse a command value while keeping secrets and textual list items literal.
    /// Structured settings retain their JSON types; commas never infer a list.
    pub fn parse_value(&self, text: &str) -> io::Result<Value> {
        if self.is_secret() {
            return Ok(Value::String(text.to_owned()));
        }
        let value = serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()));
        if value.is_null() {
            return Err(invalid_input("configuration null is not a setting"));
        }
        if matches!(
            self.as_str(),
            "ast.grammar"
                | "ast.grammars_whitelist"
                | "ast.grammars_blacklist"
                | "ast.scip.parsers"
                | "knowledge.extensions"
        ) && matches!(value, Value::Bool(_) | Value::Number(_))
        {
            Ok(Value::String(text.to_owned()))
        } else {
            Ok(value)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    Command,
    Environment,
    Project,
    Global,
    Distribution,
}

impl ConfigSource {
    pub(crate) fn validate(self, key: &ConfigKey) -> io::Result<()> {
        match key.policy() {
            Policy::Environment if self != Self::Environment => Err(invalid_input(
                "setting belongs to the environment layer only",
            )),
            Policy::Installation if self == Self::Project => Err(invalid_input(
                "installation setting cannot be stored in a project",
            )),
            _ => Ok(()),
        }
    }
}

#[derive(Debug)]
pub(crate) enum SettingValue {
    Public(Value),
    Secret(SecretString),
}

impl SettingValue {
    pub(crate) fn new(key: &ConfigKey, value: Value) -> io::Result<Self> {
        if value.is_null() {
            return Err(invalid_input("configuration null is not a setting"));
        }
        if key.is_secret() {
            match value {
                Value::String(value) => Ok(Self::Secret(SecretString::from(value))),
                _ => Err(invalid_input("secret configuration requires a string")),
            }
        } else {
            Ok(Self::Public(value))
        }
    }

    pub(crate) fn get<'a, T: Deserialize<'a>>(&'a self, source: ConfigSource) -> io::Result<T> {
        let Self::Public(value) = self else {
            return Err(invalid_input("secret access must be explicit"));
        };
        T::deserialize(value)
            .or_else(|_| {
                if let (ConfigSource::Environment, Value::String(text)) = (source, value) {
                    serde_json::from_str(text)
                } else {
                    Err(serde::de::Error::custom("incompatible setting type"))
                }
            })
            .map_err(|_| invalid_data("configuration value has incompatible type"))
    }

    pub(crate) fn inspection(&self) -> InspectionValue {
        match self {
            Self::Public(value) => InspectionValue::Public {
                value: value.clone(),
            },
            Self::Secret(value) => InspectionValue::Secret {
                configured: !value.expose_secret().is_empty(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InspectionValue {
    Public { value: Value },
    Secret { configured: bool },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConfigInspection {
    pub key: String,
    pub source: ConfigSource,
    pub value: InspectionValue,
}
