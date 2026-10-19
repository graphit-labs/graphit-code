use super::content::{fingerprint, validate_name};
use super::{AgentDescriptor, ContentError, NativeBinding, RenderContext, SkillDescriptor};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeSet,
    fmt::{self, Write as _},
    io,
};

/// Trusted caller catalog data, not artifact metadata or server authorization.
/// The caller must qualify native identities and access under its server policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolAccess {
    Read,
    Write,
    Shell,
}
#[derive(Clone, Debug)]
pub struct NativeToolGrant {
    pub name: String,
    pub access: ToolAccess,
}
/// Profile selected by the trusted caller. Native declarations do not prove
/// enforcement; PF/NI qualify dispatch and the actual host separately.
#[derive(Clone, Debug)]
pub struct ToolGrants {
    pub binding: NativeBinding,
    pub read_only: bool,
    pub allows_shell: bool,
    pub tools: Vec<NativeToolGrant>,
}
impl ToolGrants {
    /// Native host tools admitted by the selected capabilities; grants no Graphit authority.
    pub fn native(binding: NativeBinding, read_only: bool, allows_shell: bool) -> Self {
        const NAMES: &[&str] = &[
            "Read",
            "Grep",
            "Glob",
            "Write",
            "Edit",
            "Bash",
            "read",
            "web",
            "write",
            "shell",
            "edit",
            "patch",
            "grep",
            "glob",
            "read_file",
            "grep_search",
            "view_file",
            "write_file",
            "run_shell_command",
            "replace_file_content",
            "run_command",
        ];
        let tools = NAMES
            .iter()
            .filter_map(|name| {
                let access = native_access(binding, name)?;
                if (read_only && access != ToolAccess::Read)
                    || (!allows_shell && access == ToolAccess::Shell)
                {
                    return None;
                }
                Some(NativeToolGrant {
                    name: (*name).into(),
                    access,
                })
            })
            .collect();
        Self {
            binding,
            read_only,
            allows_shell,
            tools,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentProvenance {
    pub renderer_schema: &'static str,
    pub binding: NativeBinding,
    pub effective_name: String,
    pub input_fingerprint: [u8; 32],
    pub output_fingerprint: [u8; 32],
}
/// Publication draft only. It carries no captured root, ownership or payload lease.
#[derive(Clone, Debug)]
pub struct RenderedContent {
    pub relative_entry: String,
    pub bytes: Vec<u8>,
    pub provenance: ContentProvenance,
}

pub fn render_skill(
    binding: NativeBinding,
    descriptor: &SkillDescriptor<'_>,
    alias: Option<&str>,
    context: &mut RenderContext<'_>,
) -> Result<RenderedContent, ContentError> {
    context.check()?;
    let name = alias.unwrap_or(descriptor.name());
    validate_name(name)?;
    if let Some(alias) = alias {
        context.input(alias.len())?;
    }
    #[derive(Serialize)]
    struct SkillHeader<'a> {
        name: &'a str,
        #[serde(flatten)]
        fields: SkillFields<'a>,
    }
    #[derive(Serialize)]
    struct SkillFields<'a> {
        description: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        license: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        compatibility: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<&'a std::collections::BTreeMap<String, String>>,
    }
    let source = &descriptor.metadata;
    let mut keys = vec!["name", "description"];
    if source.license.is_some() {
        keys.push("license");
    }
    if source.compatibility.is_some() {
        keys.push("compatibility");
    }
    if source.metadata.is_some() {
        keys.push("metadata");
    }
    context.object(&keys, 1)?;
    for value in [
        Some(name),
        Some(source.description.as_str()),
        source.license.as_deref(),
        source.compatibility.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        context.string(value, 2)?;
    }
    if let Some(metadata) = &source.metadata {
        context.node(2)?;
        context.collection(metadata.len())?;
        for (key, value) in metadata {
            context.scalar(key)?;
            context.string(value, 3)?;
        }
    }
    let metadata = SkillHeader {
        name,
        fields: SkillFields {
            description: &source.description,
            license: source.license.as_deref(),
            compatibility: source.compatibility.as_deref(),
            metadata: source.metadata.as_ref(),
        },
    };
    let bytes = markdown(&metadata, descriptor.body, context)?;
    finish(
        binding,
        name,
        binding.skill_path(name),
        descriptor.input_fingerprint,
        bytes,
        context,
    )
}

pub fn render_agent(
    binding: NativeBinding,
    descriptor: &AgentDescriptor<'_>,
    alias: Option<&str>,
    grants: &ToolGrants,
    context: &mut RenderContext<'_>,
) -> Result<RenderedContent, ContentError> {
    render_agent_document(binding, descriptor, alias, grants, None, None, context)
}

/// Runtime data for a native profile. Hooks are embedded only by the Kiro schema.
pub struct NativeAgentRuntime<'a> {
    pub mcp: &'a super::mcp_config::NativeMcpPlan,
    pub kiro_hooks: &'a [super::kiro_hooks::KiroHookPlan],
}

/// Compose the Root profile with the selected host's MCP representation.
/// Native declarations preserve host approval policy and grant no Graphit access.
pub fn render_agent_with_native_mcp(
    binding: NativeBinding,
    descriptor: &AgentDescriptor<'_>,
    alias: Option<&str>,
    grants: &ToolGrants,
    runtime: NativeAgentRuntime<'_>,
    context: &mut RenderContext<'_>,
) -> Result<RenderedContent, ContentError> {
    let mcp = runtime.mcp;
    // A transport plan alone does not identify a readonly MCP catalog.
    // Kiro Live supplies its qualified exact tool grants through the existing path.
    if binding == NativeBinding::Antigravity
        || (grants.read_only && binding != NativeBinding::KiroV3)
    {
        return Err(ContentError::UnsupportedCapability);
    }
    if mcp.binding != binding
        || mcp.entry.len()
            != if binding == NativeBinding::OpenCode {
                3
            } else {
                2
            }
        || (binding == NativeBinding::OpenCode && mcp.entry[..2] != ["mcp", "servers"])
        || (binding != NativeBinding::KiroV3 && !runtime.kiro_hooks.is_empty())
    {
        return Err(ContentError::UnsupportedCapability);
    }
    let server = &mcp.entry[mcp.entry.len() - 1];
    if server.is_empty()
        || !server
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || mcp.fields.iter().any(|field| {
            field.path.len() != mcp.entry.len() + 1
                || field.path[..mcp.entry.len()] != mcp.entry
                || !(matches!(
                    field.path[mcp.entry.len()].as_str(),
                    "command" | "args" | "type"
                ) || (binding == NativeBinding::Codex
                    && field.path[mcp.entry.len()] == "env_vars"))
        })
    {
        return Err(ContentError::InvalidPolicy);
    }
    let field = |key| {
        mcp.fields
            .iter()
            .find(|field| field.path[mcp.entry.len()] == key)
            .map(|field| &field.value)
    };
    let command = field("command");
    let args = if binding == NativeBinding::OpenCode {
        if field("type").and_then(Value::as_str) != Some("local") {
            return Err(ContentError::InvalidPolicy);
        }
        command.and_then(Value::as_array).and_then(|argv| {
            argv.first()
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .map(|_| &argv[1..])
        })
    } else {
        if command.and_then(Value::as_str).is_none_or(str::is_empty) {
            return Err(ContentError::InvalidPolicy);
        }
        if matches!(binding, NativeBinding::Claude | NativeBinding::Cursor) {
            if field("type").and_then(Value::as_str) != Some("stdio") {
                return Err(ContentError::InvalidPolicy);
            }
        } else if field("type").is_some() {
            return Err(ContentError::InvalidPolicy);
        }
        field("args").and_then(Value::as_array).map(Vec::as_slice)
    };
    if args.is_none_or(|args| {
        !args
            .iter()
            .all(|arg| arg.is_string() && arg.as_str() != Some("--role"))
            || !args
                .windows(2)
                .any(|pair| pair[0].as_str() == Some("mcp") && pair[1].as_str() == Some("--stdio"))
    }) {
        return Err(ContentError::InvalidPolicy);
    }
    render_agent_document(
        binding,
        descriptor,
        alias,
        grants,
        Some(runtime),
        None,
        context,
    )
}

/// Compose a Role with the configured Graphit stdio namespace.
/// Native declarations do not grant access; the Role MCP server authenticates each call.
pub fn render_role_agent_with_mcp(
    binding: NativeBinding,
    role: super::Role,
    descriptor: &AgentDescriptor<'_>,
    alias: Option<&str>,
    grants: &ToolGrants,
    runtime: NativeAgentRuntime<'_>,
    context: &mut RenderContext<'_>,
) -> Result<RenderedContent, ContentError> {
    let mcp = runtime.mcp;
    let opencode = binding == NativeBinding::OpenCode;
    let prefix: &[&str] = if opencode {
        &["mcp", "servers"]
    } else {
        &["mcpServers"]
    };
    if !matches!(
        binding,
        NativeBinding::KiroV3
            | NativeBinding::OpenCode
            | NativeBinding::Claude
            | NativeBinding::Kimi
    ) || mcp.binding != binding
        || mcp.entry.len() != prefix.len() + 1
        || mcp.entry[..prefix.len()] != *prefix
        || (binding != NativeBinding::KiroV3 && !runtime.kiro_hooks.is_empty())
    {
        return Err(ContentError::UnsupportedCapability);
    }
    validate_name(&mcp.entry[prefix.len()])?;
    // No persisted environment or headers: credentials belong to the run's SDK child.
    if mcp.fields.iter().any(|field| {
        field.path.len() != mcp.entry.len() + 1
            || field.path[..mcp.entry.len()] != mcp.entry
            || !matches!(
                field.path[mcp.entry.len()].as_str(),
                "command" | "args" | "type"
            )
    }) {
        return Err(ContentError::InvalidPolicy);
    }
    let field = |name| {
        mcp.fields
            .iter()
            .find(|field| field.path[mcp.entry.len()] == name)
            .map(|field| &field.value)
    };
    let args = if opencode {
        if field("type").and_then(Value::as_str) != Some("local") || field("args").is_some() {
            return Err(ContentError::InvalidPolicy);
        }
        field("command").and_then(Value::as_array).and_then(|argv| {
            argv.first()
                .and_then(Value::as_str)
                .filter(|command| !command.is_empty())
                .map(|_| &argv[1..])
        })
    } else {
        if (binding == NativeBinding::Claude
            && field("type").and_then(Value::as_str) != Some("stdio"))
            || (binding != NativeBinding::Claude && field("type").is_some())
            || field("command")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
        {
            return Err(ContentError::InvalidPolicy);
        }
        field("args").and_then(Value::as_array).map(Vec::as_slice)
    };
    if args.is_none_or(|args| {
        !args.iter().all(Value::is_string)
            || !args
                .windows(2)
                .any(|pair| pair[0].as_str() == Some("mcp") && pair[1].as_str() == Some("--stdio"))
            || (binding == NativeBinding::KiroV3
                && !args.iter().any(|arg| arg.as_str() == Some("--role")))
    }) {
        return Err(ContentError::InvalidPolicy);
    }
    render_agent_document(
        binding,
        descriptor,
        alias,
        grants,
        Some(runtime),
        Some(role),
        context,
    )
}

fn render_agent_document(
    binding: NativeBinding,
    descriptor: &AgentDescriptor<'_>,
    alias: Option<&str>,
    grants: &ToolGrants,
    runtime: Option<NativeAgentRuntime<'_>>,
    role: Option<super::Role>,
    context: &mut RenderContext<'_>,
) -> Result<RenderedContent, ContentError> {
    context.check()?;
    let hooks = runtime
        .as_ref()
        .map_or(&[][..], |runtime| runtime.kiro_hooks);
    let mcp = runtime.map(|runtime| runtime.mcp);
    if hooks.iter().any(|hook| hook.binding() != binding) {
        return Err(ContentError::InvalidPolicy);
    }
    let name = alias.unwrap_or(descriptor.name());
    validate_name(name)?;
    if let Some(alias) = alias {
        context.input(alias.len())?;
    }
    validate_grants(
        binding,
        descriptor,
        grants,
        mcp.is_some() && role == Some(super::Role::Tracker),
        mcp.map(|plan| plan.entry[plan.entry.len() - 1].as_str()),
        context,
    )?;
    // Validate borrowed fields before any native DOM or string copies. Bodies in
    // Markdown stay borrowed; JSON/TOML bodies are native scalar fields.
    context.scalar(name)?;
    context.scalar(descriptor.description())?;
    if let Some(plan) = mcp {
        context.object(&[plan.entry[plan.entry.len() - 1].as_str()], 1)?;
        let fields: Vec<_> = plan
            .fields
            .iter()
            .map(|field| field.path[plan.entry.len()].as_str())
            .collect();
        context.object(&fields, 2)?;
        for field in &plan.fields {
            context.tree(&field.value, 3)?;
        }
    }
    let mcp_selector = mcp.and_then(|plan| {
        let server = &plan.entry[plan.entry.len() - 1];
        match binding {
            NativeBinding::KiroV3 => Some(format!("@{server}")),
            NativeBinding::Claude | NativeBinding::Qwen | NativeBinding::Kimi => {
                Some(format!("mcp__{server}__*"))
            }
            NativeBinding::Codex
            | NativeBinding::Cursor
            | NativeBinding::OpenCode
            | NativeBinding::Antigravity => None,
        }
    });
    let mcp_servers = mcp
        .filter(|_| {
            matches!(
                binding,
                NativeBinding::KiroV3 | NativeBinding::Claude | NativeBinding::Qwen
            ) && !(role.is_some() && binding == NativeBinding::Claude)
        })
        .map(|plan| {
            let server: Map<String, Value> = plan
                .fields
                .iter()
                .map(|field| (field.path[plan.entry.len()].clone(), field.value.clone()))
                .collect();
            let mut servers = Map::new();
            servers.insert(
                plan.entry[plan.entry.len() - 1].clone(),
                Value::Object(server),
            );
            Value::Object(servers)
        });
    let bytes = match binding {
        NativeBinding::Codex => {
            context.object(
                &["name", "description", "developer_instructions", "features"],
                1,
            )?;
            for value in [name, descriptor.description(), descriptor.body()] {
                context.string(value, 2)?;
            }
            context.object(&["shell_tool"], 2)?;
            context.node(3)?;
            let mut document = toml_edit::DocumentMut::new();
            document["name"] = toml_edit::value(name);
            document["description"] = toml_edit::value(descriptor.description());
            document["developer_instructions"] = toml_edit::value(descriptor.body());
            document["features"]["shell_tool"] = toml_edit::value(grants.allows_shell);
            let mut writer = LimitedWriter::new(context);
            write!(&mut writer, "{document}").map_err(|_| writer.error())?;
            writer.finish()?
        }
        _ => {
            let mut metadata = Map::new();
            metadata.insert("name".into(), json!(name));
            metadata.insert("description".into(), json!(descriptor.description()));
            if !matches!(binding, NativeBinding::Cursor | NativeBinding::OpenCode) {
                metadata.insert(
                    "tools".into(),
                    json!(
                        grants
                            .tools
                            .iter()
                            .map(|g| g.name.as_str())
                            .chain(mcp_selector.as_deref())
                            .collect::<Vec<_>>()
                    ),
                );
            }
            match binding {
                NativeBinding::Cursor => {
                    metadata.insert("model".into(), json!("inherit"));
                    metadata.insert("readonly".into(), json!(grants.read_only));
                }
                NativeBinding::Claude => {
                    if role.is_some()
                        && let Some(plan) = mcp
                    {
                        metadata.insert(
                            "mcpServers".into(),
                            json!([plan.entry[plan.entry.len() - 1]]),
                        );
                    } else if let Some(servers) = &mcp_servers {
                        metadata.insert("mcpServers".into(), json!([servers]));
                    }
                }
                NativeBinding::OpenCode => {
                    metadata.insert("mode".into(), json!("all"));
                    let mut permissions =
                        vec![json!({"action":"*","resource":"*","effect":"deny"})];
                    for grant in &grants.tools {
                        let action = if let Some(selector) = grant.name.strip_prefix('@') {
                            selector
                                .chars()
                                .map(|c| {
                                    if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                                        c
                                    } else {
                                        '_'
                                    }
                                })
                                .collect::<String>()
                        } else if grant.access == ToolAccess::Write {
                            "edit".into()
                        } else {
                            grant.name.clone()
                        };
                        permissions.push(json!({"action":action,"resource":"*","effect":"allow"}));
                    }
                    if mcp.is_some() {
                        permissions.push(json!({"action":"execute","resource":"*","effect":if role.is_some() { "allow" } else { "ask" }}));
                    }
                    if (role.is_some() || !grants.read_only)
                        && let Some(plan) = mcp
                    {
                        let server = plan.entry[plan.entry.len() - 1]
                            .chars()
                            .map(|c| {
                                if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                                    c
                                } else {
                                    '_'
                                }
                            })
                            .collect::<String>();
                        permissions.push(
                            json!({"action":format!("{server}_*"),"resource":"*","effect":if role.is_some() { "allow" } else { "ask" }}),
                        );
                    }
                    metadata.insert("permissions".into(), Value::Array(permissions));
                }
                NativeBinding::Qwen => {
                    if grants.read_only {
                        metadata.insert("approvalMode".into(), json!("plan"));
                    }
                    if let Some(servers) = &mcp_servers {
                        metadata.insert("mcpServers".into(), servers.clone());
                    }
                }
                NativeBinding::Antigravity => {
                    metadata.insert("subagent".into(), json!(true));
                    metadata.insert(
                        "commandExecutionPolicy".into(),
                        json!(if grants.allows_shell {
                            "sandbox"
                        } else {
                            "off"
                        }),
                    );
                }
                NativeBinding::KiroV3 => {
                    if !hooks.is_empty() {
                        metadata.insert(
                            "hooks".into(),
                            Value::Array(
                                hooks
                                    .iter()
                                    .map(super::kiro_hooks::KiroHookPlan::profile_value)
                                    .collect(),
                            ),
                        );
                    }
                    // Project inclusion must be composed with the qualified MC catalog.
                    // This pure role has no imported MCP/Powers configuration.
                    metadata.insert("includeMcpJson".into(), json!(false));
                    metadata.insert("includePowers".into(), json!(false));
                    if let Some(servers) = &mcp_servers {
                        metadata.insert("mcpServers".into(), servers.clone());
                    }
                    {
                        let mut rules = Vec::new();
                        if role.is_some() {
                            if let Some(plan) = mcp {
                                rules.push(json!({"capability":"mcp","match":[format!("{}/*",plan.entry[plan.entry.len() - 1])],"effect":"allow"}));
                            }
                        } else {
                            for grant in &grants.tools {
                                if let Some(selector) = grant.name.strip_prefix('@') {
                                    rules.push(json!({"capability":"mcp","match":[selector],"effect":"allow"}));
                                }
                            }
                        }
                        if grants.read_only {
                            rules.push(json!({"capability":"fs_write","effect":"deny"}));
                        }
                        if !grants.allows_shell || role == Some(super::Role::Tracker) {
                            rules.push(json!({"capability":"shell","effect":"deny"}));
                        }
                        metadata.insert("permissions".into(), json!({"rules":rules}));
                    }
                }
                _ => {}
            }
            let metadata = Value::Object(metadata);
            context.tree(&metadata, 1)?;
            markdown(&metadata, descriptor.body(), context)?
        }
    };
    finish(
        binding,
        name,
        binding.agent_path(name),
        {
            let mut input_fingerprint = descriptor.input_fingerprint;
            if let Some(plan) = mcp {
                let plan_bytes =
                    serde_json::to_vec(&plan.fields).map_err(|_| ContentError::InvalidContent)?;
                let plan_hash = fingerprint(&plan_bytes, context)?;
                let mut input = [0; 64];
                input[..32].copy_from_slice(&input_fingerprint);
                input[32..].copy_from_slice(&plan_hash);
                input_fingerprint = fingerprint(&input, context)?;
            }
            if !hooks.is_empty() {
                let values: Vec<_> = hooks
                    .iter()
                    .map(super::kiro_hooks::KiroHookPlan::profile_value)
                    .collect();
                let hook_bytes =
                    serde_json::to_vec(&values).map_err(|_| ContentError::InvalidContent)?;
                let hook_hash = fingerprint(&hook_bytes, context)?;
                let mut input = [0; 64];
                input[..32].copy_from_slice(&input_fingerprint);
                input[32..].copy_from_slice(&hook_hash);
                input_fingerprint = fingerprint(&input, context)?;
            }
            input_fingerprint
        },
        bytes,
        context,
    )
}

fn validate_grants(
    binding: NativeBinding,
    descriptor: &AgentDescriptor<'_>,
    grants: &ToolGrants,
    tracker_mcp: bool,
    mcp_server: Option<&str>,
    context: &mut RenderContext<'_>,
) -> Result<(), ContentError> {
    if binding != grants.binding {
        return Err(ContentError::InvalidPolicy);
    }
    if tracker_mcp
        && !(grants.read_only
            && grants.allows_shell
            && descriptor.read_only()
            && descriptor.allows_shell())
    {
        return Err(ContentError::UnsupportedCapability);
    }
    // These schemas carry inspection shell alongside readonly intent; neither field grants Graphit authority.
    if grants.read_only
        && grants.allows_shell
        && !matches!(binding, NativeBinding::Codex | NativeBinding::Cursor)
        && !(tracker_mcp && matches!(binding, NativeBinding::KiroV3))
    {
        return Err(ContentError::UnsupportedCapability);
    }
    if (descriptor.read_only() && !grants.read_only)
        || (descriptor.allows_shell() && !grants.allows_shell)
        || (!descriptor.allows_shell() && grants.allows_shell)
    {
        return Err(ContentError::UnsupportedCapability);
    }
    if grants.tools.len() > context.limits.max_collection_entries {
        return Err(ContentError::LimitExceeded);
    }
    let mut seen = BTreeSet::new();
    for grant in &grants.tools {
        context.input(grant.name.len())?;
        let mcp_read = grant.name.starts_with('@');
        let access = if mcp_read {
            if !exact_mcp_read(&grant.name, mcp_server) {
                return Err(ContentError::InvalidPolicy);
            }
            ToolAccess::Read
        } else {
            native_access(binding, &grant.name).ok_or(ContentError::UnsupportedCapability)?
        };
        if access != grant.access {
            return Err(ContentError::InvalidPolicy);
        }
        if grant.name.is_empty()
            || grant.name.len() > context.limits.max_scalar_bytes
            || grant
                .name
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '*' | '?' | '[' | ']'))
            || !seen.insert(&grant.name)
            || (grant.access == ToolAccess::Read && !mcp_read && !read_tool(binding, &grant.name))
            || (grants.read_only && grant.access != ToolAccess::Read)
            || (!grants.allows_shell && grant.access == ToolAccess::Shell)
        {
            return Err(ContentError::InvalidPolicy);
        }
    }
    // OpenCode's edit permission gates the complete mutation family. A subset
    // cannot be represented without granting tools absent from caller policy.
    if binding == NativeBinding::OpenCode {
        let edits =
            ["edit", "write", "patch"].map(|name| seen.iter().any(|grant| grant.as_str() == name));
        if edits.iter().any(|present| *present) && !edits.iter().all(|present| *present) {
            return Err(ContentError::UnsupportedCapability);
        }
    }
    // These schemas do not support a role-local native tool allowlist.
    if matches!(binding, NativeBinding::Codex | NativeBinding::Cursor) && !grants.tools.is_empty() {
        return Err(ContentError::UnsupportedCapability);
    }
    // Cursor readonly still permits non-mutating shell commands. Strict no-shell
    // selection has no qualified field; do not silently promise it.
    if binding == NativeBinding::Cursor && !grants.allows_shell {
        return Err(ContentError::UnsupportedCapability);
    }
    // MCP catalog inclusion is MC/PF work, not a generic tool-string escape hatch.
    if binding == NativeBinding::KiroV3
        && grants.tools.iter().any(|g| {
            !g.name.starts_with('@')
                && !matches!(g.name.as_str(), "read" | "write" | "shell" | "web")
        })
    {
        return Err(ContentError::UnsupportedCapability);
    }
    Ok(())
}

// Read-only MCP selectors require the exact server plan carried by this render.
// The caller supplies its trusted tool catalog; host approval is not Graphit admission.
fn exact_mcp_read(selector: &str, server: Option<&str>) -> bool {
    let Some((namespace, tool)) = selector.strip_prefix('@').and_then(|s| s.split_once('/')) else {
        return false;
    };
    Some(namespace) == server
        && !tool.is_empty()
        && tool
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

// Only identities qualified in the native contract are admitted. Additional
// catalogs (including MCP) require the MC/PF qualification before composition.
fn native_access(binding: NativeBinding, name: &str) -> Option<ToolAccess> {
    if read_tool(binding, name) {
        return Some(ToolAccess::Read);
    }
    match (binding, name) {
        (NativeBinding::Claude | NativeBinding::Kimi, "Write" | "Edit")
        | (NativeBinding::Qwen, "write_file" | "edit")
        | (NativeBinding::Antigravity, "replace_file_content")
        | (NativeBinding::KiroV3, "write")
        | (NativeBinding::OpenCode, "edit" | "write" | "patch") => Some(ToolAccess::Write),
        (NativeBinding::Claude | NativeBinding::Kimi, "Bash")
        | (NativeBinding::Antigravity, "run_command")
        | (NativeBinding::KiroV3, "shell")
        | (NativeBinding::Qwen, "run_shell_command")
        | (NativeBinding::OpenCode, "shell") => Some(ToolAccess::Shell),
        _ => None,
    }
}

fn read_tool(binding: NativeBinding, name: &str) -> bool {
    match binding {
        NativeBinding::Claude | NativeBinding::Kimi => matches!(name, "Read" | "Grep" | "Glob"),
        NativeBinding::Qwen => matches!(name, "read_file" | "grep_search" | "glob"),
        NativeBinding::KiroV3 => matches!(name, "read" | "web"),
        NativeBinding::OpenCode => matches!(name, "read" | "grep" | "glob"),
        NativeBinding::Antigravity => matches!(name, "view_file" | "grep_search"),
        NativeBinding::Codex | NativeBinding::Cursor => false,
    }
}

pub(super) fn json(
    value: &impl Serialize,
    context: &mut RenderContext<'_>,
) -> Result<Vec<u8>, ContentError> {
    let mut writer = LimitedWriter::new(context);
    serde_json::to_writer(&mut writer, value).map_err(|_| writer.error())?;
    writer.finish()
}

pub(super) fn markdown(
    value: &impl Serialize,
    body: &str,
    context: &mut RenderContext<'_>,
) -> Result<Vec<u8>, ContentError> {
    let mut writer = LimitedWriter::new(context);
    writer.write_str("---\n").map_err(|_| writer.error())?;
    serde_saphyr::to_fmt_writer(&mut writer, value).map_err(|_| writer.error())?;
    if !writer.bytes.ends_with(b"\n") {
        writer.write_str("\n").map_err(|_| writer.error())?;
    }
    writer.write_str("---\n").map_err(|_| writer.error())?;
    writer.write_str(body).map_err(|_| writer.error())?;
    writer.finish()
}
fn finish(
    binding: NativeBinding,
    name: &str,
    relative_entry: String,
    input_fingerprint: [u8; 32],
    bytes: Vec<u8>,
    context: &mut RenderContext<'_>,
) -> Result<RenderedContent, ContentError> {
    context.check()?;
    let output_fingerprint = fingerprint(&bytes, context)?;
    Ok(RenderedContent {
        relative_entry,
        bytes,
        provenance: ContentProvenance {
            renderer_schema: "graphit-native-content-v1",
            binding,
            effective_name: name.into(),
            input_fingerprint,
            output_fingerprint,
        },
    })
}
struct LimitedWriter<'a, 'b> {
    context: &'a mut RenderContext<'b>,
    bytes: Vec<u8>,
    failure: Option<ContentError>,
    capacity_limit: usize,
}
impl<'a, 'b> LimitedWriter<'a, 'b> {
    fn new(context: &'a mut RenderContext<'b>) -> Self {
        let capacity_limit = context.limits.max_output_bytes - context.output_bytes();
        Self {
            context,
            bytes: Vec::new(),
            failure: None,
            capacity_limit,
        }
    }
    fn push(&mut self, bytes: &[u8]) -> Result<(), ContentError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let result = self.context.output(bytes.len()).and_then(|()| {
            let required = self
                .bytes
                .len()
                .checked_add(bytes.len())
                .ok_or(ContentError::LimitExceeded)?;
            if required > self.bytes.capacity() {
                let growth = self
                    .bytes
                    .capacity()
                    .saturating_mul(2)
                    .max(1024)
                    .min(self.capacity_limit);
                let desired = required.max(growth);
                self.bytes
                    .try_reserve_exact(desired - self.bytes.len())
                    .map_err(|_| ContentError::LimitExceeded)?;
            }
            Ok(())
        });
        if let Err(error) = result {
            self.failure = Some(error);
            return Err(error);
        }
        for chunk in bytes.chunks(64 * 1024) {
            if let Err(error) = self.context.check() {
                self.failure = Some(error);
                return Err(error);
            }
            self.bytes.extend_from_slice(chunk);
        }
        Ok(())
    }
    fn error(&self) -> ContentError {
        self.failure.unwrap_or(ContentError::InvalidContent)
    }
    fn finish(self) -> Result<Vec<u8>, ContentError> {
        self.context.check()?;
        Ok(self.bytes)
    }
}
impl fmt::Write for LimitedWriter<'_, '_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.push(text.as_bytes()).map_err(|_| fmt::Error)
    }
}
impl io::Write for LimitedWriter<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.push(bytes).map_err(|_| io::ErrorKind::Other)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
