use super::super::web_file_reads::inputs::Scope;
use crate::remote_file_install::{
    client::{Kind, PhysicalMode, RemoteArguments},
    lifecycle::Operation as LifecycleOperation,
};
use graphit_hub::{ArtifactId, ConcreteVersion, NativeBinding, policy::HubResourcePolicy};
use serde::Deserialize;
use std::io;

#[derive(Clone, Copy)]
pub(crate) enum Operation {
    Install,
    InstallLocal,
    Update,
    Remove,
    Recover,
    Forget,
}
impl Operation {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::InstallLocal => "install_local",
            Self::Update => "update",
            Self::Remove => "remove",
            Self::Recover => "recover",
            Self::Forget => "forget",
        }
    }
    pub(super) fn lifecycle(self) -> io::Result<LifecycleOperation> {
        match self {
            Self::Remove => Ok(LifecycleOperation::Remove),
            Self::Recover => Ok(LifecycleOperation::Recover),
            Self::Forget => Ok(LifecycleOperation::Forget),
            _ => Err(io::ErrorKind::InvalidInput.into()),
        }
    }
}
fn non_null<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Install {
    pub selection: RemoteArguments,
    pub scope: Scope,
    #[serde(default, deserialize_with = "non_null")]
    pub binding: Option<NativeBinding>,
    #[serde(default, deserialize_with = "non_null")]
    pub alias: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    pub entry_name: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    pub mode: Option<PhysicalMode>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lifecycle {
    pub scope: Scope,
    #[serde(rename = "artifact_type")]
    pub kind: Kind,
    pub id: String,
    #[serde(default, deserialize_with = "non_null")]
    pub version: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    pub binding: Option<NativeBinding>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateAll {
    pub scope: Scope,
    #[serde(default, deserialize_with = "non_null")]
    pub binding: Option<NativeBinding>,
}

pub(crate) enum Query {
    UpdateAll(UpdateAll),
    Local(super::local_input::LocalInstall),
    Install(Operation, Install),
    Lifecycle(Operation, Lifecycle),
}
impl Query {
    pub(crate) fn decode(bytes: &[u8], operation: Operation) -> io::Result<Self> {
        match operation {
            Operation::InstallLocal => {
                return super::local_input::LocalInstall::decode(bytes).map(Self::Local);
            }
            Operation::Install => {
                serde_json::from_slice(bytes).map(|input| Self::Install(operation, input))
            }
            Operation::Update => {
                #[derive(Deserialize)]
                #[serde(untagged)]
                enum Update {
                    One(Install),
                    All(UpdateAll),
                }
                serde_json::from_slice(bytes).map(|input| match input {
                    Update::One(input) => Self::Install(operation, input),
                    Update::All(input) => Self::UpdateAll(input),
                })
            }
            _ => serde_json::from_slice(bytes).map(|input| Self::Lifecycle(operation, input)),
        }
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
    }
    pub(super) fn scope(&self) -> &Scope {
        match self {
            Self::UpdateAll(input) => &input.scope,
            Self::Local(input) => &input.scope,
            Self::Install(_, input) => &input.scope,
            Self::Lifecycle(_, input) => &input.scope,
        }
    }
    pub(super) fn operation(&self) -> Operation {
        match self {
            Self::UpdateAll(_) => Operation::Update,
            Self::Local(_) => Operation::InstallLocal,
            Self::Install(op, _) | Self::Lifecycle(op, _) => *op,
        }
    }
    pub(crate) fn validate(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        let invalid = || io::Error::from(io::ErrorKind::InvalidInput);
        let binding = match self {
            Self::UpdateAll(input) => input.binding,
            Self::Local(input) => return input.validate(policy),
            Self::Install(_, input) => {
                input.selection.validate(policy).map_err(|_| invalid())?;
                if input.selection.identity_provider.is_some() {
                    return Err(invalid());
                }
                for value in input.alias.iter().chain(input.entry_name.iter()) {
                    if value.contains('/') {
                        return Err(invalid());
                    }
                    ArtifactId::new(value, policy.projection()).map_err(|_| invalid())?;
                }
                if matches!(input.scope, Scope::GlobalCache)
                    && (input.alias.is_some() || input.entry_name.is_some() || input.mode.is_some())
                {
                    return Err(invalid());
                }
                input.binding
            }
            Self::Lifecycle(op, input) => {
                ArtifactId::new(&input.id, policy.projection()).map_err(|_| invalid())?;
                if let Some(version) = &input.version {
                    ConcreteVersion::new(version, policy.projection()).map_err(|_| invalid())?;
                }
                if matches!(input.scope, Scope::GlobalCache)
                    && (!matches!(op, Operation::Remove) || input.version.is_none())
                {
                    return Err(invalid());
                }
                input.binding
            }
        };
        match self.scope() {
            Scope::GlobalCache if binding.is_some() => Err(invalid()),
            Scope::Project {
                project_version, ..
            } if binding.is_none()
                || project_version.is_empty()
                || project_version.contains('\0') =>
            {
                Err(invalid())
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{brand::Brand, config::ConfigSnapshot};
    use serde_json::{Value, json};

    fn policy() -> HubResourcePolicy {
        let brand = Brand::default();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        HubResourcePolicy::resolve(&config).unwrap()
    }

    fn input() -> Value {
        json!({
            "selection": {
                "transport": "broker", "provider": "broker",
                "producer": "01KSH1CRFFG8Z74B5ZS78WW808",
                "artifact_type": "skill", "id": "fixture/web-file-reader",
                "version": "release/one", "exact": true
            },
            "scope": {
                "kind": "project", "project_id": "01KSH1CRFFG8Z74B5ZS78WW808",
                "project_version": "main"
            },
            "binding": NativeBinding::KiroV3
        })
    }

    #[test]
    fn collective_update_uses_strict_captured_scope_and_preserves_single_selection() {
        let policy = policy();
        let value = json!({"scope":{"kind":"project","project_id":"01KSH1CRFFG8Z74B5ZS78WW808","project_version":"main"},"binding":NativeBinding::KiroV3});
        let query = Query::decode(&serde_json::to_vec(&value).unwrap(), Operation::Update).unwrap();
        query.validate(&policy).unwrap();
        assert!(matches!(query, Query::UpdateAll(_)));
        for (key, extra) in [
            ("selection", Value::Null),
            ("project_dir", json!("/arbitrary")),
            ("alias", json!("not-a-collective-override")),
            ("binding", Value::Null),
        ] {
            let mut invalid = value.clone();
            invalid[key] = extra;
            assert!(
                Query::decode(&serde_json::to_vec(&invalid).unwrap(), Operation::Update).is_err()
            );
        }
        let global = json!({"scope":{"kind":"global_cache"}});
        Query::decode(&serde_json::to_vec(&global).unwrap(), Operation::Update)
            .unwrap()
            .validate(&policy)
            .unwrap();
        let query =
            Query::decode(&serde_json::to_vec(&input()).unwrap(), Operation::Update).unwrap();
        assert!(matches!(query, Query::Install(Operation::Update, _)));
    }

    #[test]
    fn project_mode_uses_the_native_enum_and_distinguishes_omission() {
        let policy = policy();
        for operation in [Operation::Install, Operation::Update] {
            for (wire, expected) in [(None, None), (Some("copy"), Some(PhysicalMode::Copy))] {
                let mut value = input();
                if let Some(wire) = wire {
                    value["mode"] = json!(wire);
                }
                let query = Query::decode(&serde_json::to_vec(&value).unwrap(), operation).unwrap();
                query.validate(&policy).unwrap();
                let Query::Install(_, input) = query else {
                    panic!("install and update share their strict body");
                };
                assert_eq!(input.mode, expected);
            }
        }
    }

    #[test]
    fn present_mode_rejects_null_unknown_types_and_duplicate_fields() {
        for operation in [Operation::Install, Operation::Update] {
            for mode in [
                Value::Null,
                json!("unknown"),
                json!("link"),
                json!("Copy"),
                json!(true),
                json!(1),
                json!([]),
                json!({}),
            ] {
                let mut value = input();
                value["mode"] = mode;
                assert_eq!(
                    Query::decode(&serde_json::to_vec(&value).unwrap(), operation)
                        .err()
                        .unwrap()
                        .kind(),
                    io::ErrorKind::InvalidInput
                );
            }
            let mut bytes = serde_json::to_string(&input()).unwrap();
            bytes.pop();
            bytes.push_str(r#","mode":"copy","mode":"link"}"#);
            assert_eq!(
                Query::decode(bytes.as_bytes(), operation)
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn global_cache_rejects_all_project_projection_options_before_effects() {
        let policy = policy();
        let mut global = input();
        global["scope"] = json!({"kind": "global_cache"});
        global.as_object_mut().unwrap().remove("binding");
        for operation in [Operation::Install, Operation::Update] {
            Query::decode(&serde_json::to_vec(&global).unwrap(), operation)
                .unwrap()
                .validate(&policy)
                .unwrap();
            let mut link = global.clone();
            link["mode"] = json!("link");
            assert!(Query::decode(&serde_json::to_vec(&link).unwrap(), operation).is_err());
            for (field, value) in [
                ("binding", json!(NativeBinding::KiroV3)),
                ("alias", json!("alias")),
                ("entry_name", json!("SKILL.md")),
                ("mode", json!("copy")),
            ] {
                let mut value_with_option = global.clone();
                value_with_option[field] = value;
                let query =
                    Query::decode(&serde_json::to_vec(&value_with_option).unwrap(), operation)
                        .unwrap();
                assert_eq!(
                    query.validate(&policy).err().unwrap().kind(),
                    io::ErrorKind::InvalidInput
                );
            }
        }
    }
}
