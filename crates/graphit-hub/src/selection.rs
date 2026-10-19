use crate::native::{BindingParseError, NativeBinding};
use graphit_core::config::{ConfigKey, ConfigSnapshot, ConfigSource};
use std::{fmt, sync::LazyLock};

static AGENT_KEY: LazyLock<ConfigKey> =
    LazyLock::new(|| ConfigKey::new("agent").expect("static agent key is valid"));

/// Ordered, canonical membership. Membership grants no physical ownership or permission.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentMembership(Vec<NativeBinding>);
impl AgentMembership {
    pub fn new(ids: impl IntoIterator<Item = NativeBinding>) -> Self {
        let mut members = Vec::with_capacity(NativeBinding::ALL.len());
        for id in ids {
            if !members.contains(&id) {
                members.push(id);
            }
        }
        Self(members)
    }
    /// Fail the entire captured input if any name is invalid; never silently filter it.
    pub fn parse<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<Self, BindingParseError> {
        let mut membership = Self::default();
        for name in names {
            let id = name.parse()?;
            if !membership.0.contains(&id) {
                membership.0.push(id);
            }
        }
        Ok(membership)
    }
    pub fn as_slice(&self) -> &[NativeBinding] {
        &self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionOrigin {
    Flag,
    Configuration(ConfigSource),
    Membership,
    Default,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub binding: NativeBinding,
    pub origin: SelectionOrigin,
}
/// Winning input failed validation. Its contents and shadowed layer values are not exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionError {
    origin: SelectionOrigin,
}
impl SelectionError {
    pub fn origin(&self) -> SelectionOrigin {
        self.origin
    }
}
impl fmt::Display for SelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid agent selection from {:?}", self.origin)
    }
}
impl std::error::Error for SelectionError {}
fn parse(value: &str, origin: SelectionOrigin) -> Result<Selection, SelectionError> {
    value
        .parse()
        .map(|binding| Selection { binding, origin })
        .map_err(|_| SelectionError { origin })
}
fn configured(
    snapshot: &ConfigSnapshot,
    sources: &[ConfigSource],
) -> Result<Option<Selection>, SelectionError> {
    let key = &*AGENT_KEY;
    for &source in sources {
        if let Some(layer) = snapshot.layer(source) {
            let origin = SelectionOrigin::Configuration(source);
            if let Some(value) = layer
                .get::<&str>(key)
                .map_err(|_| SelectionError { origin })?
            {
                return parse(value, origin).map(Some);
            }
        }
    }
    Ok(None)
}
/// Specialized project-agent precedence over already captured configuration.
pub fn select_agent(
    snapshot: &ConfigSnapshot,
    flag: Option<&str>,
    membership: &AgentMembership,
) -> Result<Selection, SelectionError> {
    if let Some(value) = flag {
        return parse(value, SelectionOrigin::Flag);
    }
    if let Some(selection) = configured(snapshot, &[ConfigSource::Command, ConfigSource::Project])?
    {
        return Ok(selection);
    }
    let ambient = configured(
        snapshot,
        &[
            ConfigSource::Environment,
            ConfigSource::Global,
            ConfigSource::Distribution,
        ],
    )?
    .unwrap_or(Selection {
        binding: NativeBinding::OpenCode,
        origin: SelectionOrigin::Default,
    });
    if membership.0.is_empty() || membership.0.contains(&ambient.binding) {
        Ok(ambient)
    } else {
        Ok(Selection {
            binding: membership.0[0],
            origin: SelectionOrigin::Membership,
        })
    }
}
/// Batch selection keeps membership order. A flag selects exactly one binding.
pub fn select_agents(
    snapshot: &ConfigSnapshot,
    flag: Option<&str>,
    membership: &AgentMembership,
) -> Result<Vec<NativeBinding>, SelectionError> {
    if flag.is_some() || membership.0.is_empty() {
        Ok(vec![select_agent(snapshot, flag, membership)?.binding])
    } else {
        Ok(membership.0.clone())
    }
}
