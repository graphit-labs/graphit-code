//! Canonical publication choices, without rendered payloads or checkout paths.
use super::*;
use graphit_core::config::{ConfigKey, ConfigMutation, ConfigStore};

#[derive(Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Choices {
    rules: Vec<RuleSelection>,
    skill_aliases: BTreeMap<String, String>,
    skill_bundles: BTreeMap<String, crate::compiled_skills::HubSelection>,
    module_skill: Option<String>,
}
pub(super) struct SavedSelection {
    store: ConfigStore,
    key: ConfigKey,
    previous: serde_json::Value,
}
pub(super) fn resolve(
    input: &InstallRequest,
    source: Option<&ProjectPaths>,
    policy: &SourcePolicy,
) -> io::Result<(InstallRequest, SavedSelection)> {
    let store = match input.destination {
        InstallDestination::Global => ConfigStore::global(&policy.installation)?,
        InstallDestination::Project => {
            ConfigStore::project(source.ok_or(io::ErrorKind::InvalidInput)?)?
        }
    };
    let binding = input
        .binding
        .unwrap_or(policy.binding)
        .as_str()
        .replace('-', "_");
    let key = ConfigKey::new(&format!("agents.{binding}.instructions"))?;
    let previous = store
        .load()?
        .get::<serde_json::Value>(&key)?
        .unwrap_or(serde_json::Value::Null);
    let mut choices: Choices = if previous.is_null() {
        Choices::default()
    } else {
        serde_json::from_value(previous.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
    };
    for rule in &input.rules {
        choices
            .rules
            .retain(|previous| previous.role.role() != rule.role.role());
        choices.rules.push(rule.clone());
    }
    choices.skill_aliases.extend(input.skill_aliases.clone());
    choices.skill_bundles.extend(input.skill_bundles.clone());
    if input.module_skill.is_some() {
        choices.module_skill = input.module_skill.clone();
    }
    let mut selected = input.clone();
    selected.rules = choices.rules;
    selected.skill_aliases = choices.skill_aliases;
    selected.skill_bundles = choices.skill_bundles;
    selected.module_skill = choices.module_skill;
    Ok((
        selected,
        SavedSelection {
            store,
            key,
            previous,
        },
    ))
}
impl SavedSelection {
    pub(super) fn persist(
        self,
        input: &InstallRequest,
        original: &RequestContext<'_>,
    ) -> io::Result<()> {
        let empty = input.rules.is_empty()
            && input.skill_aliases.is_empty()
            && input.skill_bundles.is_empty()
            && input.module_skill.is_none();
        if empty && self.previous.is_null() {
            return Ok(());
        }
        let choices = Choices {
            rules: input.rules.clone(),
            skill_aliases: input.skill_aliases.clone(),
            skill_bundles: input.skill_bundles.clone(),
            module_skill: input.module_skill.clone(),
        };
        let value = serde_json::to_value(choices).map_err(io::Error::other)?;
        if value != self.previous {
            let mut writer = self.store.writer(
                original.remaining()?.ok_or(io::ErrorKind::InvalidInput)?,
                original.cancellation(),
            )?;
            if writer
                .load()?
                .get::<serde_json::Value>(&self.key)?
                .unwrap_or(serde_json::Value::Null)
                != self.previous
            {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "publication choices changed during sync",
                ));
            }
            writer.apply([ConfigMutation::Set {
                key: self.key,
                value,
            }])?;
        }
        Ok(())
    }
}
