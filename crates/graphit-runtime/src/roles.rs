use crate::instructions::{InstructionError, InstructionErrorKind};
use graphit_core::{brand::Brand, file_io::SnapshotContext};
use std::{fmt, io};

/// The three maintained default roles; this is not an authority role catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedRole {
    Scout,
    Tracker,
    Scribe,
}
impl ManagedRole {
    pub fn key(self) -> &'static str {
        match self {
            Self::Scout => "scout",
            Self::Tracker => "tracker",
            Self::Scribe => "scribe",
        }
    }
    pub fn summary(self) -> &'static str {
        let markdown = match self {
            Self::Scout => {
                include_str!("../../../templates/agent-instructions/roles/scout-summary.md")
            }
            Self::Tracker => {
                include_str!("../../../templates/agent-instructions/roles/tracker-summary.md")
            }
            Self::Scribe => {
                include_str!("../../../templates/agent-instructions/roles/scribe-summary.md")
            }
        };
        markdown.trim_end_matches('\n')
    }
    pub fn modules(self) -> &'static [&'static str] {
        match self {
            Self::Scout => &["memory", "task", "ast", "knowledge", "hub"],
            Self::Tracker => &["ast", "knowledge", "task"],
            Self::Scribe => &["task", "memory"],
        }
    }
    /// Native descriptor intent only; it grants no tool or authentication authority.
    pub fn read_only(self) -> bool {
        self != Self::Scribe
    }
    /// Native descriptor intent only; a host policy may reject this combination.
    pub fn allows_shell(self) -> bool {
        self == Self::Tracker
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoleError {
    InvalidName,
    Cancelled,
    DeadlineExceeded,
    Allocation,
    Request(io::ErrorKind),
    Instruction(InstructionErrorKind),
}
impl fmt::Display for RoleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidName => "invalid managed role name",
            Self::Cancelled => "managed role composition cancelled",
            Self::DeadlineExceeded => "managed role composition deadline exceeded",
            Self::Allocation => "managed role allocation failed",
            Self::Request(_) => "managed role request failed",
            Self::Instruction(_) => "managed role instruction rendering failed",
        })
    }
}
impl std::error::Error for RoleError {}

/// Default generic content only. No arbitrary body constructor or mutable fields.
///
/// ```compile_fail
/// use graphit_runtime::StaticRole;
/// fn override_body(role: &mut StaticRole) { role.body = "custom".into(); }
/// ```
/// ```compile_fail
/// use graphit_runtime::StaticRole;
/// fn duplicate(role: &StaticRole) { let _: StaticRole = role.clone(); }
/// ```
/// ```compile_fail
/// use graphit_runtime::StaticRole;
/// fn inspect(role: &StaticRole) { println!("{role:?}"); }
/// ```
/// ```compile_fail
/// use graphit_runtime::StaticRole;
/// fn serialize(role: &StaticRole) { let _ = serde_json::to_string(role); }
/// ```
/// ```compile_fail
/// use graphit_runtime::StaticRole;
/// let _: StaticRole = serde_json::from_str("{}").unwrap();
/// ```
pub struct StaticRole {
    role: ManagedRole,
    name: String,
    body: String,
}
impl StaticRole {
    pub fn role(&self) -> ManagedRole {
        self.role
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn summary(&self) -> &'static str {
        self.role.summary()
    }
    pub fn modules(&self) -> &'static [&'static str] {
        self.role.modules()
    }
    pub fn read_only(&self) -> bool {
        self.role.read_only()
    }
    pub fn allows_shell(&self) -> bool {
        self.role.allows_shell()
    }
}

/// Compose a generic role from a validated brand and the original request.
/// Names must fit Hub's 64-byte ASCII bound without truncation.
///
/// ```compile_fail
/// use graphit_runtime::{compose_role, ManagedRole};
/// use graphit_core::{brand::Brand, file_io::SnapshotContext};
/// fn inject(brand: &Brand, original: &SnapshotContext) {
///     compose_role(ManagedRole::Scout, brand, original, "live instructions");
/// }
/// ```
pub fn compose_role(
    role: ManagedRole,
    brand: &Brand,
    original: &SnapshotContext,
) -> Result<StaticRole, RoleError> {
    check(original)?;
    let brand_value = brand;
    let brand = brand.key();
    let _name_len = brand
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_add(role.key().len()))
        .filter(|n| *n <= 64)
        .ok_or(RoleError::InvalidName)?;

    let mut skill_names = Vec::new();
    skill_names
        .try_reserve_exact(role.modules().len())
        .map_err(|_| RoleError::Allocation)?;
    for module in role.modules() {
        skill_names.push(
            brand_value
                .skill_name(module)
                .map_err(|e| RoleError::Request(e.kind()))?,
        );
    }
    let mut skills = Vec::new();
    skills
        .try_reserve_exact(skill_names.len())
        .map_err(|_| RoleError::Allocation)?;
    for skill in &skill_names {
        skills.push(skill.as_str());
    }
    let module_skill = brand_value
        .mcp_tool_name(&["module", "skill"])
        .map_err(|e| RoleError::Request(e.kind()))?;
    let defaults = crate::role_instructions::role_templates(role);
    let (name, content) = crate::role_instructions::render_role(
        role,
        brand_value,
        defaults.candidates(),
        crate::role_instructions::RoleAliases {
            module_skill: &module_skill,
            skills: &skills,
        },
        crate::role_instructions::DEFAULT_ROLE_LIMITS,
        original,
    )
    .map_err(role_instruction_error)?
    .into_parts();
    check(original)?;
    Ok(StaticRole {
        role,
        name,
        body: content,
    })
}

fn role_instruction_error(error: InstructionError) -> RoleError {
    match error.kind() {
        InstructionErrorKind::Allocation => RoleError::Allocation,
        InstructionErrorKind::Request(io::ErrorKind::Interrupted) => RoleError::Cancelled,
        InstructionErrorKind::Request(io::ErrorKind::TimedOut) => RoleError::DeadlineExceeded,
        InstructionErrorKind::Request(kind) => RoleError::Request(kind),
        kind => RoleError::Instruction(kind),
    }
}

fn check(request: &SnapshotContext) -> Result<(), RoleError> {
    request.check().map_err(|error| match error.kind() {
        io::ErrorKind::Interrupted => RoleError::Cancelled,
        io::ErrorKind::TimedOut => RoleError::DeadlineExceeded,
        kind => RoleError::Request(kind),
    })
}
