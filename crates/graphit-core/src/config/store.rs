use super::document::{EMPTY_ENCODED_LEN, FILE_LIMIT};
use super::value::SettingValue;
use super::{ConfigDocument, ConfigKey, ConfigSource};
use crate::brand::Brand;
use crate::file_lock::{Cancellation, FileLock};
use crate::local_files::{
    atomic_replace, canonical_directory, check_cancelled, directory_exists, invalid_input,
    read_regular, regular_metadata,
};
use crate::project_identity::{ProjectIdentity, project_document_lock};
use crate::project_paths::{InstallationPaths, ProjectPaths};
use secrecy::ExposeSecret;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, Permissions};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ConfigStore {
    parent: PathBuf,
    brand: Brand,
    source: ConfigSource,
}

impl ConfigStore {
    pub fn global(installation: &InstallationPaths) -> io::Result<Self> {
        let parent = match canonical_directory(installation.root()) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => installation.root().to_owned(),
            Err(error) => return Err(error),
        };
        Ok(Self {
            parent,
            brand: installation.brand().clone(),
            source: ConfigSource::Global,
        })
    }
    pub fn project(project: &ProjectPaths) -> io::Result<Self> {
        let parent = project.brand_directory();
        directory_exists(&parent)?;
        Ok(Self {
            parent,
            brand: project.brand().clone(),
            source: ConfigSource::Project,
        })
    }
    pub fn path(&self) -> PathBuf {
        self.parent.join(if self.source == ConfigSource::Project {
            "project.json"
        } else {
            "config.json"
        })
    }
    pub fn lock_path(&self) -> PathBuf {
        self.parent.join(if self.source == ConfigSource::Project {
            ".project.json.lock"
        } else {
            ".config.json.lock"
        })
    }
    pub fn load(&self) -> io::Result<ConfigDocument> {
        self.read().map(|(document, _, _)| document)
    }

    pub fn writer(
        &self,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<ConfigWriter> {
        check_cancelled(cancellation)?;
        if self.source == ConfigSource::Project {
            directory_exists(&self.parent)?;
        }
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&self.parent)?;
        if self.source == ConfigSource::Project {
            directory_exists(&self.parent)?;
        }
        let (lock, parent) = if self.source == ConfigSource::Project {
            let (lock, parent) = project_document_lock(&self.parent, timeout, cancellation)?;
            (lock, Some(parent))
        } else {
            regular_metadata(&self.lock_path())?;
            (
                FileLock::acquire(self.lock_path(), timeout, cancellation)?,
                None,
            )
        };
        Ok(ConfigWriter {
            store: self.clone(),
            cancellation: cancellation.clone(),
            _lock: lock,
            _parent: parent,
        })
    }

    pub fn set(
        &self,
        key: ConfigKey,
        value: Value,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<bool> {
        let patch = self.validate([ConfigMutation::Set { key, value }])?;
        self.writer(timeout, cancellation)?.publish(patch)
    }
    pub fn unset(
        &self,
        key: ConfigKey,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<bool> {
        let patch = self.validate([ConfigMutation::Unset { key }])?;
        self.writer(timeout, cancellation)?.publish(patch)
    }

    fn read(&self) -> io::Result<(ConfigDocument, Option<Permissions>, Option<ProjectIdentity>)> {
        if self.source == ConfigSource::Project {
            directory_exists(&self.parent)?;
        }
        let Some((bytes, permissions)) = read_regular(&self.path(), FILE_LIMIT)? else {
            return Ok((
                ConfigDocument::new(&self.brand, self.source, [])?,
                None,
                None,
            ));
        };
        if self.source == ConfigSource::Project {
            let (identity, config) =
                crate::project_identity::decode_project_document(&self.brand, &bytes)?;
            return Ok((config, Some(permissions), Some(identity)));
        }
        ConfigDocument::decode(&self.brand, self.source, &bytes)
            .map(|document| (document, Some(permissions), None))
    }

    fn validate(&self, changes: impl IntoIterator<Item = ConfigMutation>) -> io::Result<Patch> {
        let mut patch = BTreeMap::new();
        let mut encoded_len = EMPTY_ENCODED_LEN;
        let mut set_count = 0usize;
        for change in changes {
            let (key, value) = match change {
                ConfigMutation::Set { key, value } => {
                    let document =
                        ConfigDocument::new(&self.brand, self.source, [(key.clone(), value)])?;
                    encoded_len +=
                        document.encoded_len()? - EMPTY_ENCODED_LEN + usize::from(set_count != 0);
                    if encoded_len > FILE_LIMIT {
                        return Err(invalid_input(
                            "configuration mutations exceed their byte budget",
                        ));
                    }
                    set_count += 1;
                    (key.clone(), Some(document.settings[&key].clone()))
                }
                ConfigMutation::Unset { key } => {
                    self.source.validate(&key)?;
                    (key, None)
                }
            };
            if patch.insert(key, value).is_some() {
                return Err(invalid_input("duplicate configuration mutation"));
            }
            if patch.len() > 1024 {
                return Err(invalid_input("too many configuration mutations"));
            }
        }
        Ok(patch)
    }
}

type Patch = BTreeMap<ConfigKey, Option<Arc<SettingValue>>>;

#[derive(Debug)]
pub struct ConfigWriter {
    store: ConfigStore,
    cancellation: Cancellation,
    _lock: FileLock,
    _parent: Option<File>,
}
impl ConfigWriter {
    pub fn load(&self) -> io::Result<ConfigDocument> {
        check_cancelled(&self.cancellation)?;
        self.store.load()
    }
    pub fn apply(&mut self, changes: impl IntoIterator<Item = ConfigMutation>) -> io::Result<bool> {
        let patch = self.store.validate(changes)?;
        self.publish(patch)
    }
    fn publish(&mut self, patch: Patch) -> io::Result<bool> {
        check_cancelled(&self.cancellation)?;
        let (current, permissions, identity) = self.store.read()?;
        if self.store.source == ConfigSource::Project && identity.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "project identity is absent; initialize the project before configuring it",
            ));
        }
        let mut desired = (*current.settings).clone();
        let mut changed = false;
        for (key, value) in patch {
            match value {
                Some(value) if desired.get(&key).is_some_and(|old| equivalent(old, &value)) => {}
                Some(value) => {
                    desired.insert(key, value);
                    changed = true;
                }
                None => {
                    changed |= desired.remove(&key).is_some();
                }
            }
        }
        if !changed {
            return Ok(false);
        }
        if desired.len() > 1024 {
            return Err(invalid_input("too many configuration settings"));
        }
        let desired = ConfigDocument {
            settings: Arc::new(desired),
            ..current
        };
        let bytes = desired.encode_with_identity(identity.as_ref())?;
        if permissions.as_ref().is_some_and(Permissions::readonly) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "configuration is read-only",
            ));
        }
        let permissions = if self.store.source == ConfigSource::Global {
            private_permissions(permissions)
        } else {
            permissions
        };
        atomic_replace(&self.store.path(), &bytes, permissions, &self.cancellation)?;
        Ok(true)
    }
}

fn equivalent(a: &SettingValue, b: &SettingValue) -> bool {
    match (a, b) {
        (SettingValue::Public(a), SettingValue::Public(b)) => a == b,
        (SettingValue::Secret(a), SettingValue::Secret(b)) => {
            a.expose_secret() == b.expose_secret()
        }
        _ => false,
    }
}
fn private_permissions(_original: Option<Permissions>) -> Option<Permissions> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Some(Permissions::from_mode(
            _original.map_or(0o600, |permissions| permissions.mode() & 0o600),
        ))
    }
    #[cfg(not(unix))]
    {
        _original
    }
}

pub enum ConfigMutation {
    Set { key: ConfigKey, value: Value },
    Unset { key: ConfigKey },
}
impl fmt::Debug for ConfigMutation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Set { key, value } if key.is_secret() => formatter
                .debug_struct("Set")
                .field("key", key)
                .field("value", &"[REDACTED]")
                .finish(),
            Self::Set { key, value } => formatter
                .debug_struct("Set")
                .field("key", key)
                .field("value", value)
                .finish(),
            Self::Unset { key } => formatter.debug_struct("Unset").field("key", key).finish(),
        }
    }
}
