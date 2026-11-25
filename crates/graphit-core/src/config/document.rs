use super::value::SettingValue;
use super::{ConfigInspection, ConfigKey, ConfigSource};
use crate::brand::Brand;
use crate::local_files::{invalid_data, invalid_input};
use crate::project_identity::ProjectIdentity;
use secrecy::{ExposeSecret, SecretString, zeroize::Zeroizing};
use serde::de::{self, MapAccess, Visitor};
use serde::ser::{SerializeMap, SerializeStruct};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Write};
use std::sync::Arc;

pub(crate) const FILE_LIMIT: usize = 256 * 1024;
pub(crate) const EMPTY_ENCODED_LEN: usize = b"{\"settings\":{}}".len();
const SETTING_LIMIT: usize = 1024;

#[derive(Clone, Debug)]
/// Raw configuration has no public wire serialization; use `inspect` for adapters.
///
/// ```compile_fail
/// use graphit_core::brand::Brand;
/// use graphit_core::config::{ConfigDocument, ConfigSource};
/// let document = ConfigDocument::new(&Brand::default(), ConfigSource::Global, []).unwrap();
/// serde_json::to_string(&document).unwrap();
/// ```
pub struct ConfigDocument {
    pub(crate) brand: Brand,
    pub(crate) source: ConfigSource,
    pub(crate) settings: Arc<BTreeMap<ConfigKey, Arc<SettingValue>>>,
}

impl ConfigDocument {
    pub fn new(
        brand: &Brand,
        source: ConfigSource,
        entries: impl IntoIterator<Item = (ConfigKey, Value)>,
    ) -> io::Result<Self> {
        Self::try_new(brand, source, entries.into_iter().map(Ok))
    }

    pub(crate) fn try_new(
        brand: &Brand,
        source: ConfigSource,
        entries: impl IntoIterator<Item = io::Result<(ConfigKey, Value)>>,
    ) -> io::Result<Self> {
        let mut settings = BTreeMap::new();
        let mut encoded_len = EMPTY_ENCODED_LEN;
        for entry in entries {
            let (key, value) = entry?;
            source.validate(&key)?;
            validate_value(&value)?;
            if key.as_str() == "githooks" {
                let parsed;
                let hooks =
                    if let (ConfigSource::Environment, Value::String(text)) = (source, &value) {
                        parsed = serde_json::from_str(text)
                            .map_err(|_| invalid_input("githooks requires a JSON object"))?;
                        &parsed
                    } else {
                        &value
                    };
                let commands = serde_json::from_value(hooks.clone())
                    .map_err(|_| invalid_input("githooks requires an object of command arrays"))?;
                crate::git_hooks::commands(commands)?;
            }
            encoded_len += key.as_str().len() + 3 + usize::from(!settings.is_empty());
            let remaining = FILE_LIMIT
                .checked_sub(encoded_len)
                .ok_or_else(|| invalid_input("configuration exceeds its byte budget"))?;
            encoded_len += count_serialized(&value, remaining)?;
            let value = Arc::new(SettingValue::new(&key, value)?);
            if settings.insert(key, value).is_some() {
                return Err(invalid_input("duplicate configuration key"));
            }
            if settings.len() > SETTING_LIMIT {
                return Err(invalid_input("too many configuration settings"));
            }
        }
        Ok(Self {
            brand: brand.clone(),
            source,
            settings: Arc::new(settings),
        })
    }

    pub fn source(&self) -> ConfigSource {
        self.source
    }
    pub fn contains(&self, key: &ConfigKey) -> bool {
        self.settings.contains_key(key)
    }

    pub fn get<'a, T: Deserialize<'a>>(&'a self, key: &ConfigKey) -> io::Result<Option<T>> {
        self.settings
            .get(key)
            .map(|value| value.get(self.source))
            .transpose()
    }

    pub fn expose_secret(&self, key: &ConfigKey) -> io::Result<Option<&SecretString>> {
        if !key.is_secret() {
            return Err(invalid_input("key does not name a configuration secret"));
        }
        Ok(self
            .settings
            .get(key)
            .and_then(|value| match value.as_ref() {
                SettingValue::Secret(value) => Some(value),
                _ => None,
            }))
    }

    pub fn inspect(&self) -> Vec<ConfigInspection> {
        self.settings
            .iter()
            .map(|(key, value)| ConfigInspection {
                key: key.as_str().to_owned(),
                source: self.source,
                value: value.inspection(),
            })
            .collect()
    }

    /// Decode the current configuration document from caller-supplied bytes.
    ///
    /// Applies the 256 KiB document bound, canonical keys, source policy and secret validation.
    /// Malformed or invalid settings return sanitized `InvalidData` errors. This performs no I/O
    /// and grants no filesystem authority; callers retain their original capture/request checks.
    ///
    /// ```
    /// use graphit_core::brand::Brand;
    /// use graphit_core::config::{ConfigDocument, ConfigKey, ConfigSource};
    /// let document = ConfigDocument::decode(
    ///     &Brand::default(), ConfigSource::Global,
    ///     br#"{"settings":{"custom.enabled":true}}"#,
    /// ).unwrap();
    /// assert_eq!(document.get::<bool>(&ConfigKey::new("custom.enabled").unwrap()).unwrap(), Some(true));
    /// ```
    pub fn decode(brand: &Brand, source: ConfigSource, bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > FILE_LIMIT {
            return Err(invalid_data("configuration exceeds its byte budget"));
        }
        if source == ConfigSource::Project {
            return crate::project_identity::decode_project_document(brand, bytes)
                .map(|(_, config)| config);
        }
        let raw: RawDocument = serde_json::from_slice(bytes)
            .map_err(|_| invalid_data("invalid configuration JSON"))?;
        Self::from_sections(brand, source, raw.settings, raw.githooks)
    }

    pub(crate) fn from_sections(
        brand: &Brand,
        source: ConfigSource,
        settings: RawSettings,
        githooks: Option<Value>,
    ) -> io::Result<Self> {
        let mut entries = Vec::with_capacity(settings.0.len() + usize::from(githooks.is_some()));
        for (text, value) in settings.0 {
            if text == "githooks" {
                return Err(invalid_data(
                    "githooks belongs at the document root, outside settings",
                ));
            }
            let key = ConfigKey::new(&text)
                .map_err(|_| invalid_data("invalid stored configuration key"))?;
            if key.as_str() != text {
                return Err(invalid_data("stored configuration key is not canonical"));
            }
            entries.push((key, value));
        }
        if let Some(value) = githooks {
            entries.push((ConfigKey::new("githooks")?, value));
        }
        Self::new(brand, source, entries)
            .map_err(|_| invalid_data("invalid stored configuration settings"))
    }

    pub(crate) fn encode_with_identity(
        &self,
        identity: Option<&ProjectIdentity>,
    ) -> io::Result<Zeroizing<Vec<u8>>> {
        let mut bytes = LimitedBuffer(Zeroizing::new(Vec::new()));
        serde_json::to_writer(
            &mut bytes,
            &WireDocument {
                config: self,
                identity,
            },
        )
        .map_err(|_| invalid_input("configuration exceeds its encoding budget"))?;
        Ok(bytes.0)
    }

    pub(crate) fn encoded_len(&self) -> io::Result<usize> {
        count_serialized(
            &WireDocument {
                config: self,
                identity: None,
            },
            FILE_LIMIT,
        )
    }
}

pub(crate) fn count_serialized(value: &impl Serialize, limit: usize) -> io::Result<usize> {
    struct Counter {
        used: usize,
        limit: usize,
    }
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.used) {
                return Err(invalid_input("configuration exceeds its byte budget"));
            }
            self.used += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { used: 0, limit };
    serde_json::to_writer(&mut counter, value)
        .map_err(|_| invalid_input("configuration exceeds its byte budget"))?;
    Ok(counter.used)
}

fn validate_value(value: &Value) -> io::Result<()> {
    let mut stack = vec![(value, 0usize)];
    let mut count = 0usize;
    while let Some((value, depth)) = stack.pop() {
        count += 1;
        if depth > 64 || count > 8192 {
            return Err(invalid_input(
                "configuration value exceeds its structural budget",
            ));
        }
        match value {
            Value::String(text) if text.len() > FILE_LIMIT => {
                return Err(invalid_input("configuration value exceeds its byte budget"));
            }
            Value::Array(array) => {
                if array.len() + stack.len() + count > 8192 {
                    return Err(invalid_input(
                        "configuration value exceeds its structural budget",
                    ));
                }
                stack.extend(array.iter().map(|value| (value, depth + 1)));
            }
            Value::Object(object) => {
                if object.len() + stack.len() + count > 8192
                    || object.keys().any(|key| key.len() > FILE_LIMIT)
                {
                    return Err(invalid_input(
                        "configuration value exceeds its structural budget",
                    ));
                }
                stack.extend(object.values().map(|value| (value, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDocument {
    #[serde(default)]
    settings: RawSettings,
    #[serde(default, deserialize_with = "present_value")]
    githooks: Option<Value>,
}
#[derive(Default)]
pub(crate) struct RawSettings(BTreeMap<String, Value>);

pub(crate) fn present_value<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

impl<'de> Deserialize<'de> for RawSettings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueSettings;
        impl<'de> Visitor<'de> for UniqueSettings {
            type Value = RawSettings;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a unique settings object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut settings = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    if settings.len() >= SETTING_LIMIT || settings.insert(key, value).is_some() {
                        return Err(de::Error::custom(
                            "duplicate or excessive configuration settings",
                        ));
                    }
                }
                Ok(RawSettings(settings))
            }
        }
        deserializer.deserialize_map(UniqueSettings)
    }
}

struct WireDocument<'a> {
    config: &'a ConfigDocument,
    identity: Option<&'a ProjectIdentity>,
}
impl Serialize for WireDocument<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let hooks = self
            .config
            .settings
            .iter()
            .find(|(key, _)| key.as_str() == "githooks")
            .map(|(_, value)| value);
        let mut fields = 1 + usize::from(hooks.is_some());
        if let Some(identity) = self.identity {
            fields += 3
                + usize::from(!identity.metadata().description.is_empty())
                + usize::from(!identity.metadata().cluster.is_empty());
        }
        let mut object = serializer.serialize_struct("Configuration", fields)?;
        if let Some(identity) = self.identity {
            let metadata = identity.metadata();
            object.serialize_field("id", &identity.id())?;
            object.serialize_field("name", &metadata.name)?;
            if !metadata.description.is_empty() {
                object.serialize_field("description", &metadata.description)?;
            }
            if !metadata.cluster.is_empty() {
                object.serialize_field("cluster", &metadata.cluster)?;
            }
            object.serialize_field("ephemeral", &metadata.ephemeral)?;
        }
        if let Some(hooks) = hooks {
            match hooks.as_ref() {
                SettingValue::Public(value) => object.serialize_field("githooks", value)?,
                SettingValue::Secret(_) => unreachable!("githooks is public configuration"),
            }
        }
        object.serialize_field("settings", &WireSettings(self.config))?;
        object.end()
    }
}
struct WireSettings<'a>(&'a ConfigDocument);
impl Serialize for WireSettings<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        for (key, value) in self.0.settings.iter() {
            if key.as_str() == "githooks" {
                continue;
            }
            match value.as_ref() {
                SettingValue::Public(value) => map.serialize_entry(key.as_str(), value)?,
                SettingValue::Secret(value) => {
                    map.serialize_entry(key.as_str(), value.expose_secret())?
                }
            }
        }
        map.end()
    }
}

struct LimitedBuffer(Zeroizing<Vec<u8>>);
impl Write for LimitedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > FILE_LIMIT.saturating_sub(self.0.len()) {
            return Err(invalid_input("configuration exceeds its byte budget"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
