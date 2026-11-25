//! Captured project/environment selection and the currently supported native layouts.
use graphit_core::{
    brand::Brand,
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
};
use graphit_hub::{AgentMembership, NativeBinding, select_agent, select_agents};
use std::{error::Error, io};

fn main() -> Result<(), Box<dyn Error>> {
    let brand = Brand::default();
    let config = ConfigSnapshot::new(
        &brand,
        [
            ConfigDocument::new(
                &brand,
                ConfigSource::Project,
                [(ConfigKey::new("agent")?, serde_json::json!("codex"))],
            )?,
            ConfigDocument::new(
                &brand,
                ConfigSource::Environment,
                [(ConfigKey::new("agent")?, serde_json::json!("antigravity"))],
            )?,
        ],
    )?;
    let ambient = ConfigSnapshot::new(
        &brand,
        [ConfigDocument::new(
            &brand,
            ConfigSource::Environment,
            [(ConfigKey::new("agent")?, serde_json::json!("antigravity"))],
        )?],
    )?;
    let members = AgentMembership::new(NativeBinding::ALL);
    let project = select_agent(&config, None, &members)?;
    let ambient = select_agent(&ambient, None, &members)?;
    let flag = select_agent(&config, Some("claude-code"), &members)?;
    let bindings = select_agents(&config, None, &members)?;
    if project.binding != NativeBinding::Codex
        || ambient.binding != NativeBinding::Antigravity
        || flag.binding != NativeBinding::Claude
        || bindings.as_slice() != members.as_slice()
    {
        return Err(io::Error::other("captured native selection differs").into());
    }
    let layouts: Vec<_> = bindings
        .iter()
        .map(|binding| {
            let layout = binding.adapter().layout();
            serde_json::json!({"binding":binding,"root":layout.root,"commands":layout.commands})
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({"project":project.binding,"environment":ambient.binding,
        "flag":flag.binding,"bindings":bindings,"layouts":layouts})
    );
    Ok(())
}
