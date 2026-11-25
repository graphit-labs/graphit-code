//! Local attribution identity and event salt; neither grants remote authority.
use crate::config::{
    ConfigDocument, ConfigKey, ConfigMutation, ConfigSnapshot, ConfigSource, ConfigStore,
};
use crate::file_lock::Cancellation;
use crate::local_files::{invalid_data, invalid_input};
use crate::project_paths::InstallationPaths;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use secrecy::{ExposeSecret, SecretString, zeroize::Zeroizing};
use std::io;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallationId(Arc<str>);

impl InstallationId {
    pub fn new(value: &str) -> io::Result<Self> {
        let value = value.trim();
        if value.is_empty()
            || value.len() > 128
            || !value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            || !value
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(invalid_input("invalid local installation identifier"));
        }
        Ok(Self(Arc::from(value)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Resolved state has no implicit wire representation of its optional salt.
///
/// ```compile_fail
/// use graphit_core::installation_identity::InstallationSettings;
/// fn serializable<T: serde::Serialize>() {}
/// serializable::<InstallationSettings>();
/// ```
#[derive(Clone, Debug, Default)]
pub struct InstallationSettings {
    unit_id: Option<InstallationId>,
    client_secret: Option<SecretString>,
}

impl InstallationSettings {
    /// Resolve captured settings without reading, generating or persisting anything.
    pub fn resolve(snapshot: &ConfigSnapshot) -> io::Result<Self> {
        let unit = ConfigKey::new("unit.id")?;
        let secret = ConfigKey::new("client.secret")?;
        Self::decode(
            snapshot.get::<&str>(&unit),
            snapshot.source(&unit),
            snapshot.expose_secret(&secret),
            snapshot.source(&secret),
        )
    }
    pub fn unit_id(&self) -> Option<&InstallationId> {
        self.unit_id.as_ref()
    }
    pub fn client_secret(&self) -> Option<&SecretString> {
        self.client_secret.as_ref()
    }
    pub fn is_provisioned(&self) -> bool {
        self.unit_id.is_some() && self.client_secret.is_some()
    }

    fn from_document(document: &ConfigDocument) -> io::Result<Self> {
        Self::decode(
            document.get::<&str>(&ConfigKey::new("unit.id")?),
            Some(document.source()),
            document.expose_secret(&ConfigKey::new("client.secret")?),
            Some(document.source()),
        )
    }
    fn decode(
        unit: io::Result<Option<&str>>,
        unit_source: Option<ConfigSource>,
        secret: io::Result<Option<&SecretString>>,
        secret_source: Option<ConfigSource>,
    ) -> io::Result<Self> {
        let unit_id = unit
            .and_then(|unit| unit.map(InstallationId::new).transpose())
            .map_err(|_| semantic_error(unit_source, "invalid installation identifier setting"))?;
        let client_secret = secret
            .and_then(|secret| {
                secret
                    .map(|secret| {
                        validate_secret(secret)?;
                        Ok(secret.clone())
                    })
                    .transpose()
            })
            .map_err(|_| semantic_error(secret_source, "invalid installation salt setting"))?;
        Ok(Self {
            unit_id,
            client_secret,
        })
    }
}

/// Secrets have no implicit wire representation.
///
/// ```compile_fail
/// use graphit_core::installation_identity::InstallationIdentity;
/// fn serializable<T: serde::Serialize>() {}
/// serializable::<InstallationIdentity>();
/// ```
#[derive(Clone, Debug)]
pub struct InstallationIdentity {
    unit_id: InstallationId,
    client_secret: SecretString,
}
impl InstallationIdentity {
    pub fn unit_id(&self) -> &InstallationId {
        &self.unit_id
    }
    pub fn client_secret(&self) -> &SecretString {
        &self.client_secret
    }
}

#[derive(Clone, Debug)]
pub struct InstallationIdentityStore {
    config: ConfigStore,
}

impl InstallationIdentityStore {
    pub fn new(installation: &InstallationPaths) -> io::Result<Self> {
        Ok(Self {
            config: ConfigStore::global(installation)?,
        })
    }
    pub fn load(&self) -> io::Result<InstallationSettings> {
        InstallationSettings::from_document(&self.config.load()?)
    }
    /// Provision only persisted global settings. Captured overrides belong to `resolve`.
    pub fn ensure(
        &self,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<InstallationIdentity> {
        self.ensure_with_entropy(timeout, cancellation, SystemTime::now(), |bytes| {
            getrandom::fill(bytes).map_err(|_| io::Error::other("installation entropy unavailable"))
        })
    }
    fn ensure_with_entropy(
        &self,
        timeout: Duration,
        cancellation: &Cancellation,
        now: SystemTime,
        mut entropy: impl FnMut(&mut [u8]) -> io::Result<()>,
    ) -> io::Result<InstallationIdentity> {
        let mut writer = self.config.writer(timeout, cancellation)?;
        let state = InstallationSettings::from_document(&writer.load()?)?;
        let mut changes = Vec::new();
        let unit_id = match state.unit_id {
            Some(unit) => unit,
            None => {
                let timestamp = now
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map_err(|_| invalid_input("installation clock is outside the ULID range"))?
                    .as_millis();
                if timestamp > (1u128 << ulid::Ulid::TIME_BITS) - 1 {
                    return Err(invalid_input(
                        "installation clock is outside the ULID range",
                    ));
                }
                let mut random = Zeroizing::new([0u8; 16]);
                entropy(&mut random[6..])
                    .map_err(|_| io::Error::other("installation entropy unavailable"))?;
                let unit = InstallationId::new(
                    &ulid::Ulid::from_parts(timestamp as u64, u128::from_be_bytes(*random))
                        .to_string(),
                )?;
                changes.push(ConfigMutation::Set {
                    key: ConfigKey::new("unit.id")?,
                    value: unit.as_str().into(),
                });
                unit
            }
        };
        let client_secret = match state.client_secret {
            Some(secret) => secret,
            None => {
                let mut random = Zeroizing::new([0u8; 32]);
                entropy(&mut *random)
                    .map_err(|_| io::Error::other("installation entropy unavailable"))?;
                let secret = SecretString::from(URL_SAFE_NO_PAD.encode(random.as_slice()));
                changes.push(ConfigMutation::Set {
                    key: ConfigKey::new("client.secret")?,
                    value: secret.expose_secret().into(),
                });
                secret
            }
        };
        if !changes.is_empty() {
            writer.apply(changes)?;
        }
        Ok(InstallationIdentity {
            unit_id,
            client_secret,
        })
    }
}

fn validate_secret(secret: &SecretString) -> io::Result<()> {
    let text = secret.expose_secret();
    if !(32..=4096).contains(&text.len()) || text.trim().is_empty() {
        Err(invalid_input("invalid installation salt"))
    } else {
        Ok(())
    }
}
fn semantic_error(source: Option<ConfigSource>, message: &str) -> io::Error {
    if matches!(source, Some(ConfigSource::Global | ConfigSource::Project)) {
        invalid_data(message)
    } else {
        invalid_input(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::Brand;
    use serde_json::json;
    use std::fs;
    use std::path::Path;

    fn store(root: &Path) -> InstallationIdentityStore {
        let paths =
            InstallationPaths::resolve(Brand::default(), root, None, Some(Path::new("global")))
                .unwrap();
        InstallationIdentityStore::new(&paths).unwrap()
    }
    fn set(store: &InstallationIdentityStore, key: &str, value: serde_json::Value) {
        store
            .config
            .set(
                ConfigKey::new(key).unwrap(),
                value,
                Duration::ZERO,
                &Cancellation::default(),
            )
            .unwrap();
    }

    #[test]
    fn entropy_failure_on_either_field_never_publishes_a_partial_pair() {
        for failure in [1, 2] {
            let root = tempfile::tempdir().unwrap();
            let store = store(root.path());
            set(&store, "custom.value", json!("preserved"));
            let before = fs::read(store.config.path()).unwrap();
            let mut calls = 0;
            let error = store
                .ensure_with_entropy(
                    Duration::ZERO,
                    &Cancellation::default(),
                    SystemTime::now(),
                    |bytes| {
                        calls += 1;
                        bytes.fill(7);
                        if calls == failure {
                            Err(io::Error::other("artificial-sensitive-entropy-canary"))
                        } else {
                            Ok(())
                        }
                    },
                )
                .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Other);
            assert!(!error.to_string().contains("canary"));
            assert_eq!(calls, failure);
            assert_eq!(fs::read(store.config.path()).unwrap(), before);
            assert!(!store.load().unwrap().is_provisioned());
        }
    }

    #[test]
    fn cancellation_after_generation_keeps_configuration_absent() {
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path());
        let cancellation = Cancellation::default();
        let mut calls = 0;
        let error = store
            .ensure_with_entropy(Duration::ZERO, &cancellation, SystemTime::now(), |bytes| {
                bytes.fill(3);
                calls += 1;
                if calls == 2 {
                    cancellation.cancel();
                }
                Ok(())
            })
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(!store.config.path().exists());
        assert!(!store.load().unwrap().is_provisioned());
    }

    #[test]
    fn complete_and_partial_states_use_entropy_only_for_missing_fields() {
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path());
        set(&store, "unit.id", json!("custom-Owner"));
        let identity = store
            .ensure_with_entropy(
                Duration::ZERO,
                &Cancellation::default(),
                SystemTime::UNIX_EPOCH - Duration::from_secs(1),
                |bytes| {
                    assert_eq!(bytes.len(), 32);
                    bytes.fill(8);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(identity.unit_id().as_str(), "custom-Owner");
        assert_eq!(
            URL_SAFE_NO_PAD
                .decode(identity.client_secret().expose_secret())
                .unwrap(),
            vec![8; 32]
        );
        let before = fs::read(store.config.path()).unwrap();
        let modified = fs::metadata(store.config.path())
            .unwrap()
            .modified()
            .unwrap();
        let again = store
            .ensure_with_entropy(
                Duration::ZERO,
                &Cancellation::default(),
                SystemTime::UNIX_EPOCH - Duration::from_secs(1),
                |_| panic!("complete identity must not use entropy"),
            )
            .unwrap();
        assert_eq!(again.unit_id(), identity.unit_id());
        assert_eq!(
            again.client_secret().expose_secret(),
            identity.client_secret().expose_secret()
        );
        assert_eq!(fs::read(store.config.path()).unwrap(), before);
        assert_eq!(
            fs::metadata(store.config.path())
                .unwrap()
                .modified()
                .unwrap(),
            modified
        );
    }

    #[test]
    fn generated_ulid_uses_validated_clock_and_exact_entropy_bits() {
        for now in [
            SystemTime::UNIX_EPOCH - Duration::from_millis(1),
            SystemTime::UNIX_EPOCH + Duration::from_millis(1u64 << 48),
        ] {
            let root = tempfile::tempdir().unwrap();
            let store = store(root.path());
            assert_eq!(
                store
                    .ensure_with_entropy(Duration::ZERO, &Cancellation::default(), now, |_| panic!(
                        "invalid clock must fail before entropy"
                    ))
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
            assert!(!store.config.path().exists());
        }
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path());
        set(&store, "client.secret", json!("s".repeat(32)));
        let identity = store
            .ensure_with_entropy(
                Duration::ZERO,
                &Cancellation::default(),
                SystemTime::UNIX_EPOCH + Duration::from_millis(42),
                |bytes| {
                    assert_eq!(bytes.len(), 10);
                    bytes.fill(5);
                    Ok(())
                },
            )
            .unwrap();
        let id = ulid::Ulid::from_string(identity.unit_id().as_str()).unwrap();
        assert_eq!(id.timestamp_ms(), 42);
        assert_eq!(&id.to_bytes()[6..], &[5; 10]);
        assert_eq!(id.to_string(), identity.unit_id().as_str());
    }

    #[test]
    fn aggregate_publication_budget_keeps_the_last_document() {
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path());
        let padding = 256 * 1024 - b"{\"settings\":{\"custom.padding\":\"\"}}".len() - 10;
        set(&store, "custom.padding", json!("x".repeat(padding)));
        let before = fs::read(store.config.path()).unwrap();
        let error = store
            .ensure_with_entropy(
                Duration::ZERO,
                &Cancellation::default(),
                SystemTime::now(),
                |bytes| {
                    bytes.fill(2);
                    Ok(())
                },
            )
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(fs::read(store.config.path()).unwrap(), before);
        assert!(!store.load().unwrap().is_provisioned());
    }
}
