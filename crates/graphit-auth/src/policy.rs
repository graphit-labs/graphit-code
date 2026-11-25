use graphit_core::{brand::Brand, module_settings::Module};
use graphit_hub::{
    ArtifactType, NativeBinding, SourceRef,
    native::{Role, RoleCredentialReference, RoleSource},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fmt,
    io::{self, Write},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthError {
    Invalid,
    InvalidSetting(Setting),
    Denied,
    Expired,
    Request,
    Limit,
    Entropy,
    State,
    /// A physical authority check failed before identity could be established.
    Io,
}
impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid role authority input",
            Self::InvalidSetting(setting) => {
                return write!(f, "invalid role authority setting: {}", setting.as_str());
            }
            Self::Denied => "role authority denied",
            Self::Expired => "role authority expired",
            Self::Request => "role authority request unavailable",
            Self::Limit => "role authority limit",
            Self::Entropy => "role authority entropy unavailable",
            Self::State => "role authority state unavailable",
            Self::Io => "role authority I/O unavailable",
        })
    }
}
impl std::error::Error for AuthError {}

/// Static field names in the explicit issuer policy; values are never echoed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    AdminSessionLifetime,
    MaximumAdminSessionLifetime,
    MaxPermissions,
    MaxScopeBytes,
    MaxParentBearerBytes,
    Ttl,
    MaximumTtl,
    RefreshMargin,
}
impl Setting {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AdminSessionLifetime => "admin_session_lifetime",
            Self::MaximumAdminSessionLifetime => "maximum_admin_session_lifetime",
            Self::MaxPermissions => "max_permissions",
            Self::MaxScopeBytes => "max_scope_bytes",
            Self::MaxParentBearerBytes => "max_parent_bearer_bytes",
            Self::Ttl => "ttl",
            Self::MaximumTtl => "maximum",
            Self::RefreshMargin => "refresh_margin",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalEffect {
    NativeLaunch,
    HookExecution,
    RemoteRequest,
    PublishedFilesystem,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Read(Module),
    WriteMemory,
    WriteOwnTask,
    WriteFencedMemory,
    InspectGit,
    CoordinateTask,
    Delegate,
    WriteTask,
    MutateModule(Module),
    ReadUser,
    MutateUser,
    Configure,
    ManageAuthProfiles,
    ManageAccessGrants,
    ManageCache,
    External(ExternalEffect),
}
impl Permission {
    pub(crate) fn code(self) -> Result<u16, AuthError> {
        Ok(match self {
            Self::Read(m) => m as u16,
            Self::MutateModule(Module::Task | Module::Memory) => return Err(AuthError::Invalid),
            Self::MutateModule(m) => 32 + m as u16,
            Self::WriteMemory => 64,
            Self::WriteOwnTask => 65,
            Self::WriteFencedMemory => 66,
            Self::InspectGit => 67,
            Self::CoordinateTask => 68,
            Self::Delegate => 69,
            Self::WriteTask => 70,
            Self::ReadUser => 71,
            Self::MutateUser => 72,
            Self::Configure => 73,
            Self::ManageAuthProfiles => 74,
            Self::ManageAccessGrants => 75,
            Self::ManageCache => 76,
            Self::External(e) => {
                80 + match e {
                    ExternalEffect::NativeLaunch => 0,
                    ExternalEffect::HookExecution => 1,
                    ExternalEffect::RemoteRequest => 2,
                    ExternalEffect::PublishedFilesystem => 3,
                }
            }
        })
    }
    pub(crate) fn allowed(self, role: Role) -> bool {
        match role {
            Role::Scout => matches!(
                self,
                Self::Read(
                    Module::Memory | Module::Task | Module::Ast | Module::Knowledge | Module::Hub
                )
            ),
            Role::Tracker => matches!(
                self,
                Self::Read(Module::Task | Module::Ast | Module::Knowledge) | Self::InspectGit
            ),
            Role::Scribe => matches!(
                self,
                Self::Read(Module::Task | Module::Memory) | Self::WriteMemory | Self::WriteOwnTask
            ),
            Role::Dream => matches!(self, Self::Read(Module::Memory) | Self::WriteFencedMemory),
            Role::Coordinator => self.code().is_ok(),
        }
    }
}
/// Obligations must be enforced by the classified handler before its effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Obligation {
    Unconditional,
    OwnActor,
    MemoryFenceRequired,
}

/// Bounds on the actual permissions, scope data and authentication key inputs.
#[derive(Clone, Copy, Debug)]
pub struct IssuerLimits {
    pub max_permissions: usize,
    pub max_scope_bytes: usize,
    pub max_parent_bearer_bytes: usize,
}
impl IssuerLimits {
    pub(crate) fn validate(self) -> Result<Self, AuthError> {
        let values = [
            (Setting::MaxPermissions, self.max_permissions),
            (Setting::MaxScopeBytes, self.max_scope_bytes),
            (Setting::MaxParentBearerBytes, self.max_parent_bearer_bytes),
        ];
        for (field, value) in values {
            if value == 0 || value > isize::MAX as usize {
                return Err(AuthError::InvalidSetting(field));
            }
        }
        Ok(self)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TtlPolicy {
    pub ttl: Duration,
    pub maximum: Duration,
    pub refresh_margin: Duration,
}
impl TtlPolicy {
    pub(crate) fn validate(self) -> Result<Self, AuthError> {
        if self.maximum.is_zero() {
            return Err(AuthError::InvalidSetting(Setting::MaximumTtl));
        }
        if self.ttl.is_zero() || self.ttl > self.maximum {
            return Err(AuthError::InvalidSetting(Setting::Ttl));
        }
        if self.refresh_margin.is_zero()
            || self.refresh_margin >= self.ttl
            || self.refresh_margin > self.ttl / 2
        {
            return Err(AuthError::InvalidSetting(Setting::RefreshMargin));
        }
        Ok(self)
    }
}
pub struct RoleDefinition {
    pub(crate) reference: RoleCredentialReference,
    pub(crate) binding: NativeBinding,
    pub(crate) permissions: Vec<Permission>,
}
impl RoleDefinition {
    /// Qualified data. Only an authenticated parent can register it with an issuer.
    pub fn new(
        role: Role,
        source: &SourceRef,
        binding: NativeBinding,
        permissions: &[Permission],
        source_policy_sha256: [u8; 32],
        limits: IssuerLimits,
    ) -> Result<Self, AuthError> {
        if source.artifact_type() != ArtifactType::Agent {
            return Err(AuthError::Invalid);
        }
        Self::from_source(
            role,
            RoleSource::HubArtifact(source.id().clone()),
            source,
            binding,
            permissions,
            source_policy_sha256,
            limits,
        )
    }
    /// Tool-maintained definition data. Registration still requires an actual
    /// AdminParent and ManageAccessGrants; a brand never grants authority.
    pub fn tool_owned(
        role: Role,
        brand: &Brand,
        binding: NativeBinding,
        permissions: &[Permission],
        source_policy_sha256: [u8; 32],
        limits: IssuerLimits,
    ) -> Result<Self, AuthError> {
        if !matches!(role, Role::Scout | Role::Tracker | Role::Scribe) {
            return Err(AuthError::Denied);
        }
        let source = RoleSource::ToolOwned(brand.clone());
        Self::from_source(
            role,
            source.clone(),
            &source,
            binding,
            permissions,
            source_policy_sha256,
            limits,
        )
    }
    #[allow(clippy::too_many_arguments)] // Separate validated identity and source data enter the same hash.
    fn from_source(
        role: Role,
        identity: RoleSource,
        source: &impl Serialize,
        binding: NativeBinding,
        permissions: &[Permission],
        source_policy_sha256: [u8; 32],
        limits: IssuerLimits,
    ) -> Result<Self, AuthError> {
        limits.validate()?;
        if permissions.len() > limits.max_permissions {
            return Err(AuthError::Limit);
        }
        let permissions = canonical_permissions(permissions, limits.max_permissions)?;
        if permissions.iter().any(|p| !p.allowed(role)) {
            return Err(AuthError::Denied);
        }
        // Streaming serializer hashes the qualified source without building a DOM/buffer.
        let mut writer = HashWriter {
            hash: Sha256::new(),
            count: 0,
            max: limits.max_scope_bytes,
        };
        writer
            .write_all(b"graphit-role-policy-v1\0")
            .map_err(|_| AuthError::Limit)?;
        serde_json::to_writer(&mut writer, source).map_err(|_| AuthError::Limit)?;
        writer
            .write_all(role.as_str().as_bytes())
            .map_err(|_| AuthError::Limit)?;
        writer
            .write_all(binding.as_str().as_bytes())
            .map_err(|_| AuthError::Limit)?;
        for permission in &permissions {
            writer
                .write_all(&permission.code()?.to_le_bytes())
                .map_err(|_| AuthError::Limit)?;
        }
        writer
            .write_all(&source_policy_sha256)
            .map_err(|_| AuthError::Limit)?;
        Ok(Self {
            reference: RoleCredentialReference::new(role, identity, writer.hash.finalize().into()),
            binding,
            permissions,
        })
    }
    pub fn reference(&self) -> &RoleCredentialReference {
        &self.reference
    }
    pub fn binding(&self) -> NativeBinding {
        self.binding
    }
    pub fn permissions(&self) -> &[Permission] {
        &self.permissions
    }
}
pub(crate) fn canonical_permissions(
    values: &[Permission],
    max: usize,
) -> Result<Vec<Permission>, AuthError> {
    if values.len() > max {
        return Err(AuthError::Limit);
    }
    for v in values {
        v.code()?;
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(values.len())
        .map_err(|_| AuthError::Limit)?;
    result.extend_from_slice(values);
    result.sort_by_key(|p| p.code().expect("validated permission"));
    if result.windows(2).any(|p| p[0] == p[1]) {
        return Err(AuthError::Invalid);
    }
    Ok(result)
}
pub(crate) fn subset(a: &[Permission], b: &[Permission]) -> bool {
    a.iter().all(|p| b.contains(p))
}
pub(crate) fn count<T: Serialize>(value: &T, max: usize) -> Result<usize, AuthError> {
    let mut writer = HashWriter {
        hash: Sha256::new(),
        count: 0,
        max,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| AuthError::Limit)?;
    Ok(writer.count)
}
struct HashWriter {
    hash: Sha256,
    count: usize,
    max: usize,
}
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let new = self
            .count
            .checked_add(bytes.len())
            .filter(|v| *v <= self.max)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        self.hash.update(bytes);
        self.count = new;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Explicit finite parent policy; these are data, not authentication evidence.
#[derive(Clone, Copy)]
pub struct AdminSessionPolicy {
    pub lifetime: Duration,
    pub maximum: Duration,
}
/// Explicit origin data; these inputs cannot authenticate a caller.
#[derive(Clone, Copy)]
pub struct SourceAuthorityPolicy<'a> {
    pub ordinary: &'a [Permission],
    pub administrative: &'a [Permission],
    pub sessions: AdminSessionPolicy,
}
pub(crate) struct OriginPolicy {
    pub ordinary: Vec<Permission>,
    pub administrative: Vec<Permission>,
    pub sessions: AdminSessionPolicy,
}
/// Immutable policy for both real origins in one issuer state.
pub struct IssuerAuthorityPolicy {
    pub(crate) runtime: OriginPolicy,
    pub(crate) local: OriginPolicy,
    pub(crate) limits: IssuerLimits,
    pub(crate) services: crate::issuer::service_credentials::ServicePolicy,
    pub(crate) native_installation: Vec<Permission>,
    pub(crate) incoming_user: Vec<IncomingUserPolicy>,
    pub(crate) web_imported_knowledge: Vec<String>,
    pub(crate) web_imported_ast: Vec<String>,
    pub(crate) web_installed_hub: Vec<String>,
}
pub(crate) struct IncomingUserPolicy {
    pub(crate) provider: String,
    pub(crate) permissions: Vec<Permission>,
}
impl IssuerAuthorityPolicy {
    /// Enable installed Hub reads for one server-configured Web provider.
    /// USER, PROJECT and imported AST/Knowledge permissions do not imply this grant.
    pub fn with_web_installed_hub_read(mut self, provider: &str) -> Result<Self, AuthError> {
        if provider.is_empty() || self.web_installed_hub.iter().any(|p| p == provider) {
            return Err(AuthError::Invalid);
        }
        if provider.len() > self.limits.max_scope_bytes {
            return Err(AuthError::Limit);
        }
        self.web_installed_hub.push(provider.to_owned());
        Ok(self)
    }

    /// Enable imported AST reads for one server-configured Web provider.
    /// PROJECT and imported Knowledge permissions do not authorize these reads.
    pub fn with_web_imported_ast_read(mut self, provider: &str) -> Result<Self, AuthError> {
        if provider.is_empty() || self.web_imported_ast.iter().any(|p| p == provider) {
            return Err(AuthError::Invalid);
        }
        if provider.len() > self.limits.max_scope_bytes {
            return Err(AuthError::Limit);
        }
        self.web_imported_ast.push(provider.to_owned());
        Ok(self)
    }

    /// Enable imported Knowledge reads for one server-configured Web provider.
    /// This is independent of USER/PROJECT permissions and defaults to deny.
    pub fn with_web_imported_knowledge_read(mut self, provider: &str) -> Result<Self, AuthError> {
        if provider.is_empty() || self.web_imported_knowledge.iter().any(|p| p == provider) {
            return Err(AuthError::Invalid);
        }
        if provider.len() > self.limits.max_scope_bytes {
            return Err(AuthError::Limit);
        }
        self.web_imported_knowledge.push(provider.to_owned());
        Ok(self)
    }

    /// Explicit server-owned USER Memory/Knowledge permissions for one inbound provider.
    /// Registered Web and Incoming MCP principals use the same explicit USER policy.
    /// This data neither verifies a principal nor admits a physical namespace.
    pub fn with_incoming_user_permissions(
        mut self,
        provider: &str,
        permissions: &[Permission],
    ) -> Result<Self, AuthError> {
        if provider.is_empty() || self.incoming_user.iter().any(|r| r.provider == provider) {
            return Err(AuthError::Invalid);
        }
        if provider.len() > self.limits.max_scope_bytes {
            return Err(AuthError::Limit);
        }
        if permissions.iter().any(|p| {
            !matches!(
                p,
                Permission::ReadUser
                    | Permission::MutateUser
                    | Permission::Read(Module::Memory | Module::Knowledge)
                    | Permission::MutateModule(Module::Knowledge)
                    | Permission::WriteMemory
                    | Permission::WriteFencedMemory
            )
        }) {
            return Err(AuthError::Invalid);
        }
        let permissions = canonical_permissions(permissions, self.limits.max_permissions)?;
        self.incoming_user
            .try_reserve(1)
            .map_err(|_| AuthError::Limit)?;
        self.incoming_user.push(IncomingUserPolicy {
            provider: provider.to_owned(),
            permissions,
        });
        Ok(self)
    }
    /// Capture the installation permissions available only after Native profile authentication.
    /// Ordinary Runtime/Local permissions and administrative sessions remain unchanged.
    pub fn with_native_installation_permissions(
        mut self,
        permissions: &[Permission],
    ) -> Result<Self, AuthError> {
        if permissions.iter().any(|permission| {
            !matches!(
                permission,
                Permission::Configure
                    | Permission::ManageCache
                    | Permission::External(ExternalEffect::PublishedFilesystem)
            )
        }) {
            return Err(AuthError::Invalid);
        }
        self.native_installation = canonical_permissions(permissions, self.limits.max_permissions)?;
        Ok(self)
    }

    pub fn new(
        runtime: SourceAuthorityPolicy<'_>,
        local: SourceAuthorityPolicy<'_>,
        limits: IssuerLimits,
        services: crate::ServiceAuthorityInput<'_>,
    ) -> Result<Self, AuthError> {
        let limits = limits.validate()?;
        for source in [runtime, local] {
            if source.sessions.maximum.is_zero() {
                return Err(AuthError::InvalidSetting(
                    Setting::MaximumAdminSessionLifetime,
                ));
            }
            if source.sessions.lifetime.is_zero()
                || source.sessions.lifetime > source.sessions.maximum
            {
                return Err(AuthError::InvalidSetting(Setting::AdminSessionLifetime));
            }
            for permissions in [source.ordinary, source.administrative] {
                if permissions.len() > limits.max_permissions {
                    return Err(AuthError::Limit);
                }
                for permission in permissions {
                    permission.code()?;
                }
            }
        }
        crate::issuer::service_credentials::service_preflight(services, limits)?;
        let capture_permissions = |raw: &[Permission]| -> Result<Vec<Permission>, AuthError> {
            let mut captured = Vec::new();
            captured
                .try_reserve_exact(raw.len())
                .map_err(|_| AuthError::Limit)?;
            captured.extend_from_slice(raw);
            captured
                .sort_unstable_by_key(|p| p.code().expect("permission validated before capture"));
            if captured.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(AuthError::Invalid);
            }
            Ok(captured)
        };
        let runtime = OriginPolicy {
            ordinary: capture_permissions(runtime.ordinary)?,
            administrative: capture_permissions(runtime.administrative)?,
            sessions: runtime.sessions,
        };
        let local = OriginPolicy {
            ordinary: capture_permissions(local.ordinary)?,
            administrative: capture_permissions(local.administrative)?,
            sessions: local.sessions,
        };
        let services = crate::issuer::service_credentials::capture_services(services, limits)?;
        Ok(Self {
            runtime,
            local,
            limits,
            services,
            native_installation: Vec::new(),
            incoming_user: Vec::new(),
            web_imported_knowledge: Vec::new(),
            web_imported_ast: Vec::new(),
            web_installed_hub: Vec::new(),
        })
    }
}
