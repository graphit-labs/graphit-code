use super::{ConfigDocument, ConfigInspection, ConfigKey, ConfigSource};
use crate::brand::Brand;
use crate::local_files::invalid_input;
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io;

#[derive(Clone, Debug)]
pub struct ConfigSnapshot {
    brand: Brand,
    layers: BTreeMap<ConfigSource, ConfigDocument>,
}

impl ConfigSnapshot {
    pub fn new(
        brand: &Brand,
        layers: impl IntoIterator<Item = ConfigDocument>,
    ) -> io::Result<Self> {
        let mut captured = BTreeMap::new();
        for document in layers {
            if &document.brand != brand {
                return Err(invalid_input(
                    "configuration brand does not match the call context",
                ));
            }
            if captured.insert(document.source(), document).is_some() {
                return Err(invalid_input("duplicate configuration source layer"));
            }
        }
        Ok(Self {
            brand: brand.clone(),
            layers: captured,
        })
    }

    pub fn brand(&self) -> &Brand {
        &self.brand
    }
    pub fn layer(&self, source: ConfigSource) -> Option<&ConfigDocument> {
        self.layers.get(&source)
    }
    pub fn source(&self, key: &ConfigKey) -> Option<ConfigSource> {
        self.layers
            .iter()
            .find(|(_, layer)| layer.contains(key))
            .map(|(source, _)| *source)
    }
    pub fn get<'a, T: Deserialize<'a>>(&'a self, key: &ConfigKey) -> io::Result<Option<T>> {
        self.source(key)
            .map(|source| self.layers[&source].get(key))
            .transpose()
            .map(Option::flatten)
    }
    pub fn expose_secret(&self, key: &ConfigKey) -> io::Result<Option<&SecretString>> {
        if !key.is_secret() {
            return Err(invalid_input("key does not name a configuration secret"));
        }
        self.source(key)
            .map(|source| self.layers[&source].expose_secret(key))
            .transpose()
            .map(Option::flatten)
    }
    pub fn inspect(&self) -> Vec<ConfigInspection> {
        let keys: BTreeSet<_> = self
            .layers
            .values()
            .flat_map(|layer| layer.settings.keys())
            .collect();
        keys.into_iter()
            .map(|key| {
                let source = self.source(key).expect("union key has a source");
                ConfigInspection {
                    key: key.as_str().to_owned(),
                    source,
                    value: self.layers[&source].settings[key].inspection(),
                }
            })
            .collect()
    }

    /// Capture only requested variables from an explicit caller-owned OS snapshot.
    pub fn capture_environment(
        brand: &Brand,
        keys: &[ConfigKey],
        variables: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> io::Result<ConfigDocument> {
        if keys.len() > 1024 {
            return Err(invalid_input("too many configuration environment keys"));
        }
        let wanted: BTreeMap<_, _> = keys
            .iter()
            .map(|key| (OsString::from(key.environment_variable(brand)), key))
            .collect();
        let entries = variables.into_iter().filter_map(|(name, value)| {
            let key = wanted.get(&name)?;
            Some(
                value
                    .into_string()
                    .map(|value| ((*key).clone(), Value::String(value)))
                    .map_err(|_| invalid_input("configuration environment value must be UTF-8")),
            )
        });
        ConfigDocument::try_new(brand, ConfigSource::Environment, entries)
    }
}
