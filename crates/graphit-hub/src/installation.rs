//! Closed installation identities. Membership and source versions do not own paths.
use crate::{AdapterId, ArtifactType, native::NativeBinding};
use graphit_core::artifact_store::ArtifactKey;
use serde::{Deserialize, Serialize};
use std::io;

/// Identity of a global native corpus, qualified engine, or shared host surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InstallationTarget {
    GlobalNative,
    GlobalCache,
    NativeBinding { binding: NativeBinding },
    SharedAdapter { adapter: AdapterId },
}

impl<'de> Deserialize<'de> for InstallationTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // An empty struct variant enforces unknown-field rejection; a serde unit
        // variant accepts extra fields even with the enum's deny_unknown_fields.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Target {
            GlobalNative {},
            GlobalCache {},
            NativeBinding { binding: NativeBinding },
            SharedAdapter { adapter: AdapterId },
        }
        Ok(match Target::deserialize(deserializer)? {
            Target::GlobalNative {} => Self::GlobalNative,
            Target::GlobalCache {} => Self::GlobalCache,
            Target::NativeBinding { binding } => Self::NativeBinding { binding },
            Target::SharedAdapter { adapter } => Self::SharedAdapter { adapter },
        })
    }
}
impl InstallationTarget {
    pub const fn for_artifact(binding: NativeBinding, kind: ArtifactType) -> Self {
        if Self::engine_specific(binding.adapter(), kind) {
            Self::NativeBinding { binding }
        } else {
            Self::SharedAdapter {
                adapter: binding.adapter(),
            }
        }
    }
    pub const fn namespace(self) -> &'static str {
        match self {
            Self::GlobalNative => "global_native",
            Self::GlobalCache => "global_cache",
            Self::NativeBinding { binding } => binding.as_str(),
            Self::SharedAdapter { adapter } => adapter.as_str(),
        }
    }
    pub const fn adapter(self) -> Option<AdapterId> {
        match self {
            Self::GlobalNative | Self::GlobalCache => None,
            Self::NativeBinding { binding } => Some(binding.adapter()),
            Self::SharedAdapter { adapter } => Some(adapter),
        }
    }
    pub fn includes(self, binding: NativeBinding) -> bool {
        match self {
            Self::GlobalNative => true,
            Self::GlobalCache => false,
            Self::NativeBinding { binding: selected } => selected == binding,
            Self::SharedAdapter { adapter } => adapter == binding.adapter(),
        }
    }
    pub fn key(self, kind: ArtifactType, id: &str) -> io::Result<ArtifactKey> {
        self.validate_type(kind)?;
        ArtifactKey::new(self.namespace(), kind.as_str(), id)
    }
    pub fn from_key(key: &ArtifactKey) -> io::Result<Self> {
        let kind = ArtifactType::ALL
            .into_iter()
            .find(|v| v.as_str() == key.artifact_type())
            .ok_or(io::ErrorKind::InvalidData)?;
        let target = if key.adapter() == "global_native" {
            Self::GlobalNative
        } else if key.adapter() == "global_cache" {
            Self::GlobalCache
        } else {
            match key.adapter().parse::<NativeBinding>() {
                Ok(binding) if Self::engine_specific(binding.adapter(), kind) => {
                    Self::NativeBinding { binding }
                }
                _ => Self::SharedAdapter {
                    adapter: key
                        .adapter()
                        .parse()
                        .map_err(|_| io::ErrorKind::InvalidData)?,
                },
            }
        };
        target.validate_type(kind)?;
        if target.namespace() != key.adapter() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(target)
    }
    pub(crate) fn validate_type(self, kind: ArtifactType) -> io::Result<()> {
        let valid = match self {
            Self::GlobalNative => matches!(
                kind,
                ArtifactType::Ast | ArtifactType::Knowledge | ArtifactType::Language
            ),
            Self::GlobalCache => !matches!(kind, ArtifactType::Ast | ArtifactType::Knowledge),
            Self::NativeBinding { binding } => Self::engine_specific(binding.adapter(), kind),
            Self::SharedAdapter { adapter } => !Self::engine_specific(adapter, kind),
        };
        if !valid {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
    const fn engine_specific(adapter: AdapterId, kind: ArtifactType) -> bool {
        matches!(kind, ArtifactType::Agent)
            || (matches!(adapter, AdapterId::Kiro)
                && matches!(
                    kind,
                    ArtifactType::Command
                        | ArtifactType::Workflow
                        | ArtifactType::Mcp
                        | ArtifactType::Power
                ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_cache_identity_is_strict_and_never_selects_a_native_binding() {
        let target = InstallationTarget::GlobalCache;
        assert_eq!(target.adapter(), None);
        assert_eq!(
            serde_json::to_string(&target).unwrap(),
            r#"{"kind":"global_cache"}"#
        );
        assert_eq!(
            serde_json::from_str::<InstallationTarget>(r#"{"kind":"global_cache"}"#).unwrap(),
            target
        );
        for kind in ArtifactType::ALL {
            let key = ArtifactKey::new("global_cache", kind.as_str(), "producer/module").unwrap();
            if matches!(kind, ArtifactType::Ast | ArtifactType::Knowledge) {
                assert!(target.key(kind, "producer/module").is_err());
                assert!(InstallationTarget::from_key(&key).is_err());
            } else {
                assert_eq!(target.key(kind, "producer/module").unwrap(), key);
                assert_eq!(InstallationTarget::from_key(&key).unwrap(), target);
            }
            for binding in NativeBinding::ALL {
                assert!(!target.includes(binding));
                assert_ne!(InstallationTarget::for_artifact(binding, kind), target);
            }
        }
        for bad in [
            r#"{"kind":"global_cache","binding":"codex"}"#,
            r#"{"kind":"global_cache","adapter":"codex"}"#,
            r#"{"kind":"GlobalCache"}"#,
        ] {
            assert!(serde_json::from_str::<InstallationTarget>(bad).is_err());
        }
    }

    #[test]
    fn global_native_key_and_serde_are_host_independent_and_type_checked() {
        let target = InstallationTarget::GlobalNative;
        assert_eq!(target.adapter(), None);
        assert_eq!(
            serde_json::to_string(&target).unwrap(),
            r#"{"kind":"global_native"}"#
        );
        assert_eq!(
            serde_json::from_str::<InstallationTarget>(r#"{"kind":"global_native"}"#).unwrap(),
            target
        );
        for kind in ArtifactType::ALL {
            let key = ArtifactKey::new("global_native", kind.as_str(), "producer/module").unwrap();
            if matches!(
                kind,
                ArtifactType::Ast | ArtifactType::Knowledge | ArtifactType::Language
            ) {
                assert_eq!(target.key(kind, "producer/module").unwrap(), key);
                assert_eq!(InstallationTarget::from_key(&key).unwrap(), target);
            } else {
                assert!(target.key(kind, "producer/module").is_err());
                assert!(InstallationTarget::from_key(&key).is_err());
            }
            for binding in NativeBinding::ALL {
                let host = InstallationTarget::for_artifact(binding, kind);
                assert_ne!(host, target);
                assert_eq!(host.adapter(), Some(binding.adapter()));
                assert!(target.includes(binding));
            }
        }
        for bad in [
            r#"{"kind":"global_native","adapter":"codex"}"#,
            r#"{"kind":"global_native","binding":"kiro-v2"}"#,
            r#"{"kind":"GlobalNative"}"#,
        ] {
            assert!(serde_json::from_str::<InstallationTarget>(bad).is_err());
        }
    }
}
