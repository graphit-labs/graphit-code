use super::{AuthenticatedCaller, RoleIssuer, VerifiedRoleGrant, source::Origin};
use super::{
    imported_ast::ImportedAstReadAdmission, imported_knowledge::ImportedKnowledgeReadAdmission,
    installed_hub::InstalledHubReadAdmission, records::ProjectReadAdmission,
    web_hub_mutation::WebHubPublicationAdmission,
};
use crate::broker::oauth::web::AuthenticatedWebCaller;
use crate::{
    broker::{
        AuthenticatedBrokerCaller, BrokerError, BrokerRequestContext, BrokerVerifier,
        VerifiedNativeLogin,
    },
    policy::{AuthError, ExternalEffect, IssuerLimits, Obligation, Permission},
    profile::{
        AiService, DirectProtocol, LocalCredentials, LocalExecution, LocalS3, ProfileCredentials,
        ProfileError, ProviderKind, S3Location, S3Source,
    },
    store::{AuthSnapshot, AuthSnapshotRecheck, AuthStoreError},
};
use graphit_hub::policy::RequestContext;
use std::{
    fmt,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
mod web_hub_catalog_write;
use web_hub_catalog_write::WebHubCatalogWrite;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServicePurpose {
    Embedding,
    Rerank,
    RecordStorage,
    InventoryRead,
    InventoryPublish,
    HubRead,
    HubWrite,
    PresenceRead,
    PresencePublish,
}
impl ServicePurpose {
    fn inventory(self) -> bool {
        matches!(self, Self::InventoryRead | Self::InventoryPublish)
    }
    fn presence(self) -> bool {
        matches!(self, Self::PresenceRead | Self::PresencePublish)
    }
    fn hub(self) -> bool {
        matches!(self, Self::HubRead | Self::HubWrite)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceOrigin {
    Runtime,
    Local,
    Web,
    BrokerIncoming,
}
#[derive(Clone, Copy)]
pub enum SubjectSelector<'a> {
    Exact(&'a str),
    AnyVerified,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialSource {
    DirectKey,
    NativeAccess,
    LocalBrokerKey,
    Anonymous,
    IncomingBearer,
}
#[derive(Clone, Copy)]
pub struct IncomingPrincipalInput<'a> {
    pub issuer: &'a str,
    pub subject: SubjectSelector<'a>,
    pub incoming_resource: &'a str,
}
#[derive(Clone, Copy)]
pub enum RemoteServiceBindingInput<'a> {
    BrokerInventory {
        provider: &'a str,
        purpose: ServicePurpose,
        endpoint: &'a str,
    },
    Direct {
        provider: &'a str,
        purpose: ServicePurpose,
        protocol: DirectProtocol,
        endpoint: &'a str,
        model: &'a str,
        model_revision: &'a str,
        recipe: &'a str,
        dimensions: u32,
        request_dimensions: Option<u32>,
        batch_endpoint: Option<&'a str>,
    },
    Broker {
        provider: &'a str,
        purpose: ServicePurpose,
        endpoint: &'a str,
        resource: Option<&'a str>,
    },
}
#[derive(Clone, Copy)]
pub enum ServiceRuleInput<'a> {
    StandaloneS3 {
        purpose: ServicePurpose,
        origin: ServiceOrigin,
        provider: &'a str,
        profile: Option<&'a str>,
    },
    Local {
        origin: ServiceOrigin,
        provider: &'a str,
        purpose: ServicePurpose,
        execution: LocalExecution,
    },
    Remote {
        origin: ServiceOrigin,
        binding: RemoteServiceBindingInput<'a>,
        credential: CredentialSource,
        incoming: Option<IncomingPrincipalInput<'a>>,
    },
}
#[derive(Clone, Copy, Debug)]
pub struct ServiceAdmissionLimits {
    pub max_rules: usize,
    pub max_binding_bytes: usize,
}
/// Borrowed startup data. Only the existing issuer-policy builder retains it.
#[derive(Clone, Copy)]
pub struct ServiceAuthorityInput<'a> {
    pub rules: &'a [ServiceRuleInput<'a>],
    pub limits: ServiceAdmissionLimits,
    pub native_refresh_margin: Duration,
}

enum SubjectPolicy {
    Exact(String),
    AnyVerified,
}
struct IncomingPolicy {
    issuer: String,
    subject: SubjectPolicy,
    resource: String,
}
enum Binding {
    StandaloneS3 {
        purpose: ServicePurpose,
        provider: String,
        profile: Option<String>,
    },
    BrokerInventory {
        provider: String,
        purpose: ServicePurpose,
        endpoint: String,
    },
    Local {
        provider: String,
        purpose: ServicePurpose,
        execution: LocalExecution,
    },
    Direct {
        provider: String,
        purpose: ServicePurpose,
        protocol: DirectProtocol,
        endpoint: String,
        model: String,
        model_revision: String,
        recipe: String,
        dimensions: u32,
        request_dimensions: Option<u32>,
        batch_endpoint: Option<String>,
    },
    Broker {
        provider: String,
        purpose: ServicePurpose,
        endpoint: String,
        resource: Option<String>,
    },
}
struct Rule {
    origin: ServiceOrigin,
    binding: Binding,
    credential: Option<CredentialSource>,
    incoming: Option<IncomingPolicy>,
}
pub(crate) struct ServicePolicy {
    rules: Vec<Rule>,
    margin: Duration,
}

impl ServiceRuleInput<'_> {
    fn hub_pair(&self, other: &Self) -> bool {
        match (*self, *other) {
            (
                Self::Remote {
                    binding:
                        RemoteServiceBindingInput::Broker {
                            purpose: ServicePurpose::HubRead,
                            endpoint: left,
                            resource: left_resource,
                            ..
                        },
                    credential: first,
                    incoming: None,
                    ..
                },
                Self::Remote {
                    binding:
                        RemoteServiceBindingInput::Broker {
                            purpose: ServicePurpose::HubRead,
                            endpoint: right,
                            resource: right_resource,
                            ..
                        },
                    credential: second,
                    incoming: None,
                    ..
                },
            ) => {
                left == right
                    && left_resource == right_resource
                    && matches!(
                        (first, second),
                        (CredentialSource::NativeAccess, CredentialSource::Anonymous)
                            | (CredentialSource::Anonymous, CredentialSource::NativeAccess)
                    )
            }
            _ => false,
        }
    }
    fn key(&self) -> (ServiceOrigin, &str, ServicePurpose, Option<&str>) {
        match *self {
            Self::StandaloneS3 {
                origin,
                provider,
                profile,
                purpose,
            } => (origin, provider, purpose, profile),
            Self::Local {
                origin,
                provider,
                purpose,
                ..
            } => (origin, provider, purpose, None),
            Self::Remote {
                origin, binding, ..
            } => match binding {
                RemoteServiceBindingInput::Direct {
                    provider, purpose, ..
                }
                | RemoteServiceBindingInput::Broker {
                    provider, purpose, ..
                }
                | RemoteServiceBindingInput::BrokerInventory {
                    provider, purpose, ..
                } => (origin, provider, purpose, None),
            },
        }
    }
}

/// Checks all borrowed strings/counts before any policy allocation or copy.
pub(crate) fn service_preflight(
    input: ServiceAuthorityInput<'_>,
    issuer: IssuerLimits,
) -> Result<(), AuthError> {
    let l = input.limits;
    if [l.max_rules, l.max_binding_bytes]
        .into_iter()
        .any(|n| n == 0 || n > isize::MAX as usize)
        || Instant::now()
            .checked_add(input.native_refresh_margin)
            .is_none()
        || SystemTime::now()
            .checked_add(input.native_refresh_margin)
            .is_none()
    {
        return Err(AuthError::Invalid);
    }
    if input.rules.len() > l.max_rules {
        return Err(AuthError::Limit);
    }
    let mut bytes = 0usize;
    let mut raw_excess = 0usize;
    let mut string = |s: &str| -> Result<(), AuthError> {
        if s.is_empty() {
            return Err(AuthError::Invalid);
        }
        if s.len() > issuer.max_scope_bytes {
            return Err(AuthError::Limit);
        }
        bytes = bytes
            .checked_add(s.len())
            .filter(|n| *n <= l.max_binding_bytes)
            .ok_or(AuthError::Limit)?;
        Ok(())
    };
    for (i, rule) in input.rules.iter().enumerate() {
        match rule {
            ServiceRuleInput::Remote {
                origin: ServiceOrigin::Web,
                binding:
                    RemoteServiceBindingInput::Broker {
                        purpose:
                            ServicePurpose::HubRead
                            | ServicePurpose::HubWrite
                            | ServicePurpose::Embedding
                            | ServicePurpose::Rerank
                            | ServicePurpose::RecordStorage,
                        ..
                    },
                credential: CredentialSource::IncomingBearer,
                incoming: None,
            } => {}
            ServiceRuleInput::Remote {
                origin: ServiceOrigin::Web,
                ..
            } => {
                return Err(AuthError::Invalid);
            }
            ServiceRuleInput::Local {
                origin: ServiceOrigin::Web,
                purpose,
                ..
            } if !matches!(purpose, ServicePurpose::Embedding | ServicePurpose::Rerank) => {
                return Err(AuthError::Invalid);
            }
            _ => {}
        }
        if input.rules[..i]
            .iter()
            .any(|old| old.key() == rule.key() && !old.hub_pair(rule))
        {
            return Err(AuthError::Invalid);
        }
        match *rule {
            ServiceRuleInput::StandaloneS3 {
                origin,
                provider,
                profile,
                purpose,
            } => {
                if !matches!(
                    purpose,
                    ServicePurpose::HubRead | ServicePurpose::RecordStorage
                ) {
                    return Err(AuthError::Invalid);
                }
                if !matches!(
                    origin,
                    ServiceOrigin::Runtime | ServiceOrigin::Local | ServiceOrigin::Web
                ) {
                    return Err(AuthError::Invalid);
                }
                string(provider)?;
                if let Some(profile) = profile {
                    string(profile)?;
                }
            }
            ServiceRuleInput::Local {
                origin,
                provider,
                purpose,
                ..
            } => {
                if origin == ServiceOrigin::BrokerIncoming
                    || purpose.inventory()
                    || purpose.hub()
                    || purpose.presence()
                {
                    return Err(AuthError::Invalid);
                }
                string(provider)?;
            }
            ServiceRuleInput::Remote {
                origin,
                binding,
                credential,
                incoming,
            } => match binding {
                RemoteServiceBindingInput::BrokerInventory {
                    provider,
                    purpose,
                    endpoint,
                } => {
                    if !purpose.inventory()
                        || origin == ServiceOrigin::BrokerIncoming
                        || !matches!(
                            credential,
                            CredentialSource::LocalBrokerKey | CredentialSource::NativeAccess
                        )
                        || incoming.is_some()
                    {
                        return Err(AuthError::Invalid);
                    }
                    string(provider)?;
                    let normalized = inventory_endpoint(endpoint, issuer.max_scope_bytes)?;
                    raw_excess = raw_excess
                        .checked_add(endpoint.len().saturating_sub(normalized.as_str().len()))
                        .ok_or(AuthError::Limit)?;
                    string(normalized.as_str())?;
                }
                RemoteServiceBindingInput::Direct {
                    provider,
                    endpoint,
                    model,
                    model_revision,
                    recipe,
                    purpose,
                    protocol,
                    dimensions,
                    request_dimensions,
                    batch_endpoint,
                } => {
                    if purpose.inventory()
                        || purpose.hub()
                        || purpose.presence()
                        || credential != CredentialSource::DirectKey
                        || incoming.is_some()
                        || origin == ServiceOrigin::BrokerIncoming
                    {
                        return Err(AuthError::Invalid);
                    }
                    string(provider)?;
                    string(endpoint)?;
                    string(model)?;
                    string(model_revision)?;
                    string(recipe)?;
                    crate::profile::validate_direct_identity(model_revision, recipe)
                        .map_err(|_| AuthError::Invalid)?;
                    if let Some(endpoint) = batch_endpoint {
                        string(endpoint)?;
                    }
                    crate::profile::validate_direct(
                        protocol,
                        endpoint,
                        model,
                        dimensions,
                        request_dimensions,
                        batch_endpoint,
                    )
                    .map_err(|_| AuthError::Invalid)?;
                }
                RemoteServiceBindingInput::Broker {
                    provider,
                    endpoint,
                    resource,
                    purpose,
                    ..
                } => {
                    if purpose.inventory() {
                        return Err(AuthError::Invalid);
                    }
                    if purpose == ServicePurpose::RecordStorage
                        && !matches!(
                            credential,
                            CredentialSource::NativeAccess | CredentialSource::IncomingBearer
                        )
                    {
                        return Err(AuthError::Invalid);
                    }
                    if purpose.presence()
                        && (origin == ServiceOrigin::BrokerIncoming
                            || incoming.is_some()
                            || !matches!(
                                credential,
                                CredentialSource::NativeAccess | CredentialSource::LocalBrokerKey
                            ))
                    {
                        return Err(AuthError::Invalid);
                    }
                    if purpose == ServicePurpose::HubRead
                        && (origin == ServiceOrigin::BrokerIncoming
                            || !(matches!(
                                credential,
                                CredentialSource::NativeAccess | CredentialSource::Anonymous
                            ) || (origin == ServiceOrigin::Web
                                && credential == CredentialSource::IncomingBearer)))
                    {
                        return Err(AuthError::Invalid);
                    }
                    if purpose == ServicePurpose::HubWrite
                        && (origin == ServiceOrigin::BrokerIncoming
                            || incoming.is_some()
                            || !(matches!(
                                credential,
                                CredentialSource::NativeAccess | CredentialSource::LocalBrokerKey
                            ) || (origin == ServiceOrigin::Web
                                && credential == CredentialSource::IncomingBearer)))
                    {
                        return Err(AuthError::Invalid);
                    }
                    string(provider)?;
                    string(endpoint)?;
                    if let Some(resource) = resource {
                        string(resource)?;
                    }
                    if origin == ServiceOrigin::BrokerIncoming {
                        let principal = incoming.ok_or(AuthError::Invalid)?;
                        if credential != CredentialSource::IncomingBearer
                            || principal.issuer != endpoint
                        {
                            return Err(AuthError::Invalid);
                        }
                        string(principal.issuer)?;
                        string(principal.incoming_resource)?;
                        if let SubjectSelector::Exact(subject) = principal.subject {
                            string(subject)?;
                        }
                    } else if incoming.is_some()
                        || !(matches!(
                            credential,
                            CredentialSource::NativeAccess
                                | CredentialSource::LocalBrokerKey
                                | CredentialSource::Anonymous
                        ) || (origin == ServiceOrigin::Web
                            && matches!(
                                purpose,
                                ServicePurpose::HubRead
                                    | ServicePurpose::HubWrite
                                    | ServicePurpose::Embedding
                                    | ServicePurpose::Rerank
                                    | ServicePurpose::RecordStorage
                            )
                            && credential == CredentialSource::IncomingBearer))
                    {
                        return Err(AuthError::Invalid);
                    }
                }
            },
        }
    }
    bytes
        .checked_add(raw_excess)
        .filter(|n| *n <= l.max_binding_bytes)
        .ok_or(AuthError::Limit)?;
    Ok(())
}

pub(crate) fn copy_string(raw: &str) -> Result<String, AuthError> {
    let mut value = String::new();
    value
        .try_reserve_exact(raw.len())
        .map_err(|_| AuthError::Limit)?;
    value.push_str(raw);
    Ok(value)
}

fn inventory_endpoint(raw: &str, max_bytes: usize) -> Result<url::Url, AuthError> {
    if raw.len() > max_bytes {
        return Err(AuthError::Limit);
    }
    if raw.is_empty() || raw.trim() != raw || raw.chars().any(char::is_control) {
        return Err(AuthError::Invalid);
    }
    let embedded_credentials = std::cell::Cell::new(false);
    let diagnostic = |violation| {
        if violation == url::SyntaxViolation::EmbeddedCredentials {
            embedded_credentials.set(true);
        }
    };
    let endpoint = url::Url::options()
        .syntax_violation_callback(Some(&diagnostic))
        .parse(raw)
        .map_err(|_| AuthError::Invalid)?;
    if embedded_credentials.get()
        || !matches!(endpoint.scheme(), "http" | "https")
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(AuthError::Invalid);
    }
    if endpoint.as_str().len() > max_bytes {
        return Err(AuthError::Limit);
    }
    Ok(endpoint)
}
/// Captures the validated rules for the existing issuer-policy constructor.
pub(crate) fn capture_services(
    input: ServiceAuthorityInput<'_>,
    limits: IssuerLimits,
) -> Result<ServicePolicy, AuthError> {
    let mut rules = Vec::new();
    rules
        .try_reserve_exact(input.rules.len())
        .map_err(|_| AuthError::Limit)?;
    for rule in input.rules {
        let (origin, binding, credential, incoming) = match *rule {
            ServiceRuleInput::StandaloneS3 {
                origin,
                provider,
                profile,
                purpose,
            } => (
                origin,
                Binding::StandaloneS3 {
                    purpose,
                    provider: copy_string(provider)?,
                    profile: profile.map(copy_string).transpose()?,
                },
                None,
                None,
            ),
            ServiceRuleInput::Local {
                origin,
                provider,
                purpose,
                execution,
            } => (
                origin,
                Binding::Local {
                    provider: copy_string(provider)?,
                    purpose,
                    execution,
                },
                None,
                None,
            ),
            ServiceRuleInput::Remote {
                origin,
                binding,
                credential,
                incoming,
            } => {
                let binding = match binding {
                    RemoteServiceBindingInput::BrokerInventory {
                        provider,
                        purpose,
                        endpoint,
                    } => {
                        let endpoint = inventory_endpoint(endpoint, limits.max_scope_bytes)?;
                        Binding::BrokerInventory {
                            provider: copy_string(provider)?,
                            purpose,
                            endpoint: copy_string(endpoint.as_str())?,
                        }
                    }
                    RemoteServiceBindingInput::Direct {
                        provider,
                        purpose,
                        protocol,
                        endpoint,
                        model,
                        model_revision,
                        recipe,
                        dimensions,
                        request_dimensions,
                        batch_endpoint,
                    } => Binding::Direct {
                        provider: copy_string(provider)?,
                        purpose,
                        protocol,
                        endpoint: copy_string(endpoint)?,
                        model: copy_string(model)?,
                        model_revision: copy_string(model_revision)?,
                        recipe: copy_string(recipe)?,
                        dimensions,
                        request_dimensions,
                        batch_endpoint: batch_endpoint.map(copy_string).transpose()?,
                    },
                    RemoteServiceBindingInput::Broker {
                        provider,
                        purpose,
                        endpoint,
                        resource,
                    } => Binding::Broker {
                        provider: copy_string(provider)?,
                        purpose,
                        endpoint: copy_string(endpoint)?,
                        resource: resource.map(copy_string).transpose()?,
                    },
                };
                let incoming = incoming
                    .map(|p| {
                        Ok::<_, AuthError>(IncomingPolicy {
                            issuer: copy_string(p.issuer)?,
                            resource: copy_string(p.incoming_resource)?,
                            subject: match p.subject {
                                SubjectSelector::Exact(s) => SubjectPolicy::Exact(copy_string(s)?),
                                SubjectSelector::AnyVerified => SubjectPolicy::AnyVerified,
                            },
                        })
                    })
                    .transpose()?;
                (origin, binding, Some(credential), incoming)
            }
        };
        rules.push(Rule {
            origin,
            binding,
            credential,
            incoming,
        });
    }
    Ok(ServicePolicy {
        rules,
        margin: input.native_refresh_margin,
    })
}

#[derive(Debug)]
pub enum ServiceUseError {
    Auth(AuthError),
    Store(AuthStoreError),
    Broker(BrokerError),
    Profile(ProfileError),
    Disabled,
    MissingCredential,
    NeedsRefresh,
    LoginRequired,
    WrongMode,
}
impl fmt::Display for ServiceUseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
            Self::Broker(e) => e.fmt(f),
            Self::Profile(e) => e.fmt(f),
            Self::Disabled => f.write_str("auth service disabled"),
            Self::MissingCredential => f.write_str("auth service credential missing"),
            Self::NeedsRefresh => f.write_str("auth service requires administrative refresh"),
            Self::LoginRequired => f.write_str("native Broker Device login required"),
            Self::WrongMode => f.write_str("wrong auth service mode"),
        }
    }
}
impl std::error::Error for ServiceUseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Auth(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Broker(error) => Some(error),
            Self::Profile(error) => Some(error),
            _ => None,
        }
    }
}
impl From<AuthError> for ServiceUseError {
    fn from(e: AuthError) -> Self {
        Self::Auth(e)
    }
}
impl From<AuthStoreError> for ServiceUseError {
    fn from(e: AuthStoreError) -> Self {
        Self::Store(e)
    }
}
impl From<BrokerError> for ServiceUseError {
    fn from(e: BrokerError) -> Self {
        Self::Broker(e)
    }
}
impl From<ProfileError> for ServiceUseError {
    fn from(e: ProfileError) -> Self {
        Self::Profile(e)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceMode {
    StandaloneS3,
    LocalAuthorized,
    Direct,
    BrokerNative,
    BrokerKey,
    BrokerIncoming,
    BrokerAnonymous,
}
pub enum ServiceBindingView<'a> {
    StandaloneS3 {
        provider: &'a str,
        profile: Option<&'a str>,
        provider_revision: u64,
    },
    BrokerInventory {
        provider: &'a str,
        purpose: ServicePurpose,
        endpoint: &'a str,
        provider_revision: u64,
    },
    Local {
        provider: &'a str,
        purpose: ServicePurpose,
        execution: LocalExecution,
    },
    Direct {
        provider: &'a str,
        purpose: ServicePurpose,
        protocol: DirectProtocol,
        endpoint: &'a str,
        model: &'a str,
        model_revision: &'a str,
        recipe: &'a str,
        dimensions: u32,
        request_dimensions: Option<u32>,
        batch_endpoint: Option<&'a str>,
    },
    Broker {
        provider: &'a str,
        purpose: ServicePurpose,
        endpoint: &'a str,
        resource: Option<&'a str>,
        provider_revision: u64,
    },
}
pub(crate) fn original_check(original: &RequestContext<'_>) -> Result<(), ServiceUseError> {
    original.snapshot().check().map_err(|e| match e.kind() {
        std::io::ErrorKind::Interrupted => ServiceUseError::Profile(ProfileError::Cancelled),
        std::io::ErrorKind::TimedOut => ServiceUseError::Profile(ProfileError::Deadline),
        _ => ServiceUseError::Auth(AuthError::Request),
    })
}

#[derive(Clone, Copy)]
enum ServiceAuthorization {
    Local,
    WebHubRead,
    Remote {
        permission: Permission,
        obligation: Obligation,
    },
}
impl ServiceAuthorization {
    fn require_remote(self) -> Result<(), ServiceUseError> {
        match self {
            Self::Remote {
                permission: Permission::External(ExternalEffect::RemoteRequest),
                obligation: Obligation::Unconditional,
            } => Ok(()),
            _ => Err(AuthError::Denied.into()),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Authority<'a> {
    Records(super::record_storage::RecordStorageAdmission<'a>),
    Caller(&'a AuthenticatedCaller<'a>),
    Role(&'a VerifiedRoleGrant<'a>),
    Web {
        caller: &'a AuthenticatedWebCaller<'a>,
        admission: &'a ProjectReadAdmission<'a>,
        original: &'a RequestContext<'a>,
    },
    ImportedAst(&'a ImportedAstReadAdmission<'a>),
    ImportedKnowledge(&'a ImportedKnowledgeReadAdmission<'a>),
    InstalledHub(&'a InstalledHubReadAdmission<'a>),
    WebHubPublication(&'a WebHubPublicationAdmission<'a>),
    WebHubCatalogWrite(WebHubCatalogWrite<'a>),
    Incoming {
        caller: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
    },
}
impl<'a> Authority<'a> {
    fn original(self) -> &'a RequestContext<'a> {
        match self {
            Self::Records(admission) => admission.original(),
            Self::Caller(p) => p.request,
            Self::Role(p) => p.request,
            Self::Web { original, .. } => original,
            Self::ImportedAst(admission) => admission.request(),
            Self::ImportedKnowledge(admission) => admission.request(),
            Self::InstalledHub(admission) => admission.request(),
            Self::WebHubPublication(admission) => admission.request(),
            Self::WebHubCatalogWrite(write) => write.original,
            Self::Incoming { original, .. } => original,
        }
    }
    pub(super) fn origin(self) -> ServiceOrigin {
        match self {
            Self::Records(admission) => admission.principal().origin(),
            Self::Caller(p) => match p.source.origin() {
                Origin::Runtime => ServiceOrigin::Runtime,
                Origin::Local => ServiceOrigin::Local,
            },
            Self::Role(p) => match p.source.origin() {
                Origin::Runtime => ServiceOrigin::Runtime,
                Origin::Local => ServiceOrigin::Local,
            },
            Self::Incoming { .. } => ServiceOrigin::BrokerIncoming,
            Self::Web { .. } => ServiceOrigin::Web,
            Self::ImportedAst(admission) => admission.origin(),
            Self::ImportedKnowledge(admission) => admission.origin(),
            Self::InstalledHub(_) | Self::WebHubPublication(_) | Self::WebHubCatalogWrite(_) => {
                ServiceOrigin::Web
            }
        }
    }
    fn check(self, issuer: &RoleIssuer) -> Result<(), ServiceUseError> {
        original_check(self.original())?;
        match self {
            Self::Records(admission) => {
                if !std::ptr::eq(admission.issuer(), issuer) {
                    return Err(AuthError::Denied.into());
                }
                admission.recheck()?;
            }
            Self::Caller(p) => {
                if !std::ptr::eq(p.issuer, issuer) {
                    return Err(AuthError::Denied.into());
                }
                let checked = p.check();
                // Physical caller guards can collapse interruption into Auth::Request.
                // Preserve the same Original's typed cancellation or expiry first.
                original_check(self.original())?;
                checked?;
            }
            Self::Role(p) => {
                if !std::ptr::eq(p.issuer, issuer) {
                    return Err(AuthError::Denied.into());
                }
                drop(p.check()?);
            }
            Self::Web {
                caller,
                admission,
                original,
            } => {
                admission.check_web_embedding(issuer, caller, original)?;
            }
            Self::ImportedAst(admission) => admission.check_embedding_service(issuer)?,
            Self::ImportedKnowledge(admission) => admission.check_embedding_service(issuer)?,
            Self::InstalledHub(admission) => admission.check_service(issuer)?,
            Self::WebHubPublication(admission) => admission.check_service(issuer)?,
            Self::WebHubCatalogWrite(write) => write.check(issuer)?,
            Self::Incoming {
                caller,
                verifier,
                original,
            } => caller.check(verifier, BrokerRequestContext::new(original.snapshot()))?,
        }
        Ok(())
    }
    fn hub_web_caller(
        self,
    ) -> Option<(&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str)> {
        match self {
            Self::Records(admission) => admission.principal().hub_web_caller(),
            Self::Web { admission, .. } => admission.web_service_caller(),
            Self::ImportedAst(read) => read.web_service_caller(),
            Self::ImportedKnowledge(read) => read.web_service_caller(),
            Self::InstalledHub(read) => Some(read.web_service_caller()),
            Self::WebHubPublication(publication) => Some(publication.web_service_caller()),
            Self::WebHubCatalogWrite(write) => Some((write.caller, write.verifier, write.provider)),
            _ => None,
        }
    }
    fn remote(self, rule: &Rule) -> Result<ServiceAuthorization, ServiceUseError> {
        if let Self::Records(admission) = self {
            admission.recheck()?;
            if rule.binding.purpose() != ServicePurpose::RecordStorage {
                return Err(AuthError::Denied.into());
            }
            if let Some((caller, verifier, provider)) = admission.principal().hub_web_caller() {
                let Binding::Broker {
                    provider: bound,
                    endpoint,
                    resource,
                    ..
                } = &rule.binding
                else {
                    return Err(AuthError::Denied.into());
                };
                if rule.origin != ServiceOrigin::Web
                    || rule.credential != Some(CredentialSource::IncomingBearer)
                    || rule.incoming.is_some()
                    || provider != bound
                    || !verifier.matches_service_binding(
                        endpoint,
                        resource.as_deref(),
                        caller.provider_revision(),
                    )
                {
                    return Err(AuthError::Denied.into());
                }
                return Ok(ServiceAuthorization::Remote {
                    permission: Permission::External(ExternalEffect::RemoteRequest),
                    obligation: Obligation::Unconditional,
                });
            }
            return admission
                .principal()
                .authority()
                .ok_or(AuthError::Denied)?
                .remote(rule);
        }
        if matches!(
            self,
            Self::WebHubPublication(_) | Self::WebHubCatalogWrite(_)
        ) {
            let (caller, verifier, provider) = self.hub_web_caller().ok_or(AuthError::Denied)?;
            let Binding::Broker {
                provider: bound_provider,
                purpose: ServicePurpose::HubWrite,
                endpoint,
                resource,
            } = &rule.binding
            else {
                return Err(AuthError::Denied.into());
            };
            if rule.origin != ServiceOrigin::Web
                || rule.credential != Some(CredentialSource::IncomingBearer)
                || rule.incoming.is_some()
                || provider != bound_provider
                || !verifier.matches_service_binding(
                    endpoint,
                    resource.as_deref(),
                    caller.provider_revision(),
                )
            {
                return Err(AuthError::Denied.into());
            }
            match self {
                Self::WebHubPublication(publication) => publication.recheck()?,
                Self::WebHubCatalogWrite(write) => write.check(write.issuer)?,
                _ => return Err(AuthError::Denied.into()),
            }
            return Ok(ServiceAuthorization::Remote {
                permission: Permission::External(ExternalEffect::RemoteRequest),
                obligation: Obligation::Unconditional,
            });
        }
        if let Self::InstalledHub(read) = self
            && matches!(rule.binding, Binding::StandaloneS3 { .. })
        {
            if rule.origin != ServiceOrigin::Web
                || rule.credential.is_some()
                || rule.incoming.is_some()
            {
                return Err(AuthError::Denied.into());
            }
            read.recheck()?;
            return Ok(ServiceAuthorization::WebHubRead);
        }
        if let Some((caller, verifier, provider)) = self.hub_web_caller() {
            let Binding::Broker {
                provider: bound_provider,
                purpose,
                endpoint,
                resource,
            } = &rule.binding
            else {
                return Err(AuthError::Denied.into());
            };
            let imported_inference =
                matches!(
                    self,
                    Self::Web { .. } | Self::ImportedAst(_) | Self::ImportedKnowledge(_)
                ) && matches!(purpose, ServicePurpose::Embedding | ServicePurpose::Rerank);
            if (*purpose != ServicePurpose::HubRead && !imported_inference)
                || rule.origin != ServiceOrigin::Web
                || rule.credential != Some(CredentialSource::IncomingBearer)
                || rule.incoming.is_some()
                || provider != bound_provider
                || !verifier.matches_service_binding(
                    endpoint,
                    resource.as_deref(),
                    caller.provider_revision(),
                )
            {
                return Err(AuthError::Denied.into());
            }
            caller.check(
                verifier,
                BrokerRequestContext::new(self.original().snapshot()),
            )?;
            return Ok(if imported_inference {
                ServiceAuthorization::Remote {
                    permission: Permission::External(ExternalEffect::RemoteRequest),
                    obligation: Obligation::Unconditional,
                }
            } else {
                ServiceAuthorization::WebHubRead
            });
        }
        let permission = Permission::External(ExternalEffect::RemoteRequest);
        let obligation = match self {
            Self::Caller(p) => p.authorize(permission)?,
            Self::Role(p) => p.authorize(permission)?,
            Self::ImportedAst(read)
                if matches!(
                    rule.binding.purpose(),
                    ServicePurpose::HubRead | ServicePurpose::Embedding | ServicePurpose::Rerank
                ) =>
            {
                read.native_remote_permission()?
            }
            Self::ImportedKnowledge(read)
                if matches!(
                    rule.binding.purpose(),
                    ServicePurpose::HubRead | ServicePurpose::Embedding | ServicePurpose::Rerank
                ) =>
            {
                read.native_remote_permission()?
            }
            Self::Records(_)
            | Self::Web { .. }
            | Self::ImportedAst(_)
            | Self::ImportedKnowledge(_)
            | Self::InstalledHub(_)
            | Self::WebHubPublication(_)
            | Self::WebHubCatalogWrite(_) => {
                return Err(AuthError::Denied.into());
            }
            Self::Incoming {
                caller, verifier, ..
            } => {
                let principal = rule.incoming.as_ref().ok_or(AuthError::Denied)?;
                if rule.origin != ServiceOrigin::BrokerIncoming
                    || rule.credential != Some(CredentialSource::IncomingBearer)
                    || caller.issuer() != principal.issuer
                    || matches!(&principal.subject, SubjectPolicy::Exact(s) if s != caller.subject())
                    || !verifier.matches_service_binding(
                        &principal.issuer,
                        Some(&principal.resource),
                        caller.provider_revision(),
                    )
                {
                    return Err(AuthError::Denied.into());
                }
                super::obligation(permission)
            }
        };
        if obligation != Obligation::Unconditional {
            return Err(AuthError::Denied.into());
        }
        Ok(ServiceAuthorization::Remote {
            permission,
            obligation,
        })
    }
}

impl Binding {
    fn provider(&self) -> &str {
        match self {
            Self::StandaloneS3 { provider, .. }
            | Self::Local { provider, .. }
            | Self::Direct { provider, .. }
            | Self::BrokerInventory { provider, .. }
            | Self::Broker { provider, .. } => provider,
        }
    }
    fn purpose(&self) -> ServicePurpose {
        match self {
            Self::StandaloneS3 { purpose, .. } => *purpose,
            Self::Local { purpose, .. }
            | Self::Direct { purpose, .. }
            | Self::BrokerInventory { purpose, .. }
            | Self::Broker { purpose, .. } => *purpose,
        }
    }
    fn view(&self, revision: u64) -> ServiceBindingView<'_> {
        match self {
            Self::StandaloneS3 {
                provider, profile, ..
            } => ServiceBindingView::StandaloneS3 {
                provider,
                profile: profile.as_deref(),
                provider_revision: revision,
            },
            Self::BrokerInventory {
                provider,
                purpose,
                endpoint,
            } => ServiceBindingView::BrokerInventory {
                provider,
                purpose: *purpose,
                endpoint,
                provider_revision: revision,
            },
            Self::Local {
                provider,
                purpose,
                execution,
            } => ServiceBindingView::Local {
                provider,
                purpose: *purpose,
                execution: *execution,
            },
            Self::Direct {
                provider,
                purpose,
                protocol,
                endpoint,
                model,
                model_revision,
                recipe,
                dimensions,
                request_dimensions,
                batch_endpoint,
            } => ServiceBindingView::Direct {
                provider,
                purpose: *purpose,
                protocol: *protocol,
                endpoint,
                model,
                model_revision,
                recipe,
                dimensions: *dimensions,
                request_dimensions: *request_dimensions,
                batch_endpoint: batch_endpoint.as_deref(),
            },
            Self::Broker {
                provider,
                purpose,
                endpoint,
                resource,
            } => ServiceBindingView::Broker {
                provider,
                purpose: *purpose,
                endpoint,
                resource: resource.as_deref(),
                provider_revision: revision,
            },
        }
    }
}

/// One original physical phase, not an owned credential or a USER storage grant.
/// ```compile_fail
/// use graphit_auth::ServiceSelection;
/// fn debug<T: std::fmt::Debug>() {} debug::<ServiceSelection<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ServiceSelection;
/// fn clone<T: Clone>() {} clone::<ServiceSelection<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ServiceSelection;
/// fn serialize<T: serde::Serialize>() {} serialize::<ServiceSelection<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ServiceSelection;
/// fn restore<T: serde::de::DeserializeOwned>() {} restore::<ServiceSelection<'static>>();
/// ```
pub struct ServiceSelection<'a> {
    issuer: &'a RoleIssuer,
    authority: Authority<'a>,
    snapshot: AuthSnapshot,
    rule: usize,
    generation: u64,
    authorization: ServiceAuthorization,
    provider: usize,
    profile: Option<usize>,
    mode: ServiceMode,
}
struct Gate<'a> {
    issuer: &'a RoleIssuer,
    authority: Authority<'a>,
    observation: Mutex<AuthSnapshotRecheck<'a>>,
    rule: usize,
    generation: u64,
    authorization: ServiceAuthorization,
    runtime_selection: Option<(
        &'a crate::profile::ProviderRecord,
        &'a crate::profile::ProfileRecord,
        bool,
    )>,
}
impl Gate<'_> {
    fn check_runtime_selection(
        &self,
        expected: &crate::profile::ProviderRecord,
        expected_profile: &crate::profile::ProfileRecord,
        uses_credentials: bool,
    ) -> Result<(), ServiceUseError> {
        let original = self.authority.original();
        self.issuer
            .local
            .recheck_installation(&self.issuer.realm, original.snapshot())?;
        let (guard, mut current) = self.issuer.local.load_locked(original)?;
        let (provider, profile) = current.document().active(original)?;
        let matches = profile.name() == expected_profile.name()
            && provider == expected
            && (!uses_credentials
                || (profile.revision() == expected_profile.revision()
                    && profile.token_revision() == expected_profile.token_revision()
                    && profile.provider_revision() == expected_profile.provider_revision()));
        self.issuer.local.revalidate(&mut current, original)?;
        if let Some(guard) = &guard {
            guard
                .revalidate()
                .map_err(|error| AuthStoreError::Core(error.into()))?;
        }
        self.authority.check(self.issuer)?;
        if !matches {
            return Err(AuthError::Denied.into());
        }
        Ok(())
    }

    fn check(&self) -> Result<(), ServiceUseError> {
        self.authority.check(self.issuer)?;
        let rule = &self.issuer.policy.services.rules[self.rule];
        match self.authorization {
            ServiceAuthorization::Local if matches!(rule.binding, Binding::Local { .. }) => (),
            ServiceAuthorization::WebHubRead => {
                if !matches!(
                    self.authority.remote(rule)?,
                    ServiceAuthorization::WebHubRead
                ) {
                    return Err(AuthError::Denied.into());
                }
            }
            remote => {
                remote.require_remote()?;
                self.authority.remote(rule)?.require_remote()?;
            }
        }
        #[cfg(test)]
        BEFORE_SERVICE_OBSERVATION_LOCK.with(|slot| {
            if let Some(hook) = slot.borrow_mut().take() {
                hook();
            }
        });
        let mut observation = self.observation.lock().map_err(|_| AuthError::State)?;
        original_check(self.authority.original())?;
        if let Some((provider, profile, uses_credentials)) = self.runtime_selection {
            self.check_runtime_selection(provider, profile, uses_credentials)?;
        } else {
            observation.recheck(self.authority.original().snapshot())?;
        }
        drop(observation);
        self.authority.check(self.issuer)?;
        let state = self.issuer.lock()?;
        if !state.service_enabled || state.service_generation != self.generation {
            return Err(AuthError::Denied.into());
        }
        original_check(self.authority.original())
    }
}
enum CredentialProof<'a> {
    StandaloneS3 {
        location: &'a S3Location,
        keys: Option<&'a LocalCredentials>,
        expires: Option<(SystemTime, Instant)>,
        user: Option<super::user_subject::VerifiedUserSubject<'a>>,
    },
    Key(&'a str),
    Anonymous,
    Incoming {
        caller: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
    },
    Web {
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
    },
    Native {
        raw: &'a str,
        proof: VerifiedNativeLogin<'a>,
        verifier: &'a BrokerVerifier,
        device: bool,
    },
}
/// A finite borrow with a fresh authority/physical gate, never a restored token.
/// ```compile_fail
/// use graphit_auth::ServiceCredentialLease;
/// fn debug<T: std::fmt::Debug>() {} debug::<ServiceCredentialLease<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ServiceCredentialLease;
/// fn clone<T: Clone>() {} clone::<ServiceCredentialLease<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ServiceCredentialLease;
/// fn serialize<T: serde::Serialize>() {} serialize::<ServiceCredentialLease<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ServiceCredentialLease;
/// fn inspect(lease: &ServiceCredentialLease<'_>) { let _ = &lease.gate; }
/// ```
/// ```compile_fail
/// use graphit_auth::ServiceCredentialLease;
/// fn restore<T: serde::de::DeserializeOwned>() {} restore::<ServiceCredentialLease<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::{ServiceCredentialLease, ProjectRecordAuthority};
/// fn grant<'a>(lease: ServiceCredentialLease<'a>) -> ProjectRecordAuthority<'a> { lease.into() }
/// ```
/// ```compile_fail
/// use graphit_auth::{ServiceCredentialLease, VerifiedRoleGrant};
/// fn grant<'a>(lease: ServiceCredentialLease<'a>) -> VerifiedRoleGrant<'a> { lease.into() }
/// ```
/// ```compile_fail
/// use graphit_auth::ServiceCredentialLease;
/// fn escape<'a>(lease: &'a ServiceCredentialLease<'a>) -> &'static str {
///     lease.credential().unwrap().unwrap()
/// }
/// ```
/// ```compile_fail
/// use graphit_auth::{ServiceSelection, ServiceCredentialLease};
/// fn escape<'a>(mut phase: ServiceSelection<'a>) -> ServiceCredentialLease<'a> {
///     phase.credential().unwrap()
/// }
/// ```
/// ```compile_fail
/// use graphit_auth::ServiceCredentialLease;
/// fn replace<'a>(lease: &mut ServiceCredentialLease<'a>, context: &'a graphit_hub::policy::RequestContext<'a>) {
///     lease.set_original(context);
/// }
/// ```
pub struct ServiceCredentialLease<'a> {
    gate: Gate<'a>,
    proof: CredentialProof<'a>,
    revision: u64,
}
/// Checked non-secret selection metadata; it confers no refresh or record authority.
#[derive(Clone, Copy, Debug)]
pub struct NativeProfileRevision<'a> {
    pub provider: &'a str,
    pub profile: &'a str,
    pub issuer: &'a str,
    pub subject: &'a str,
    pub revisions: crate::profile::TokenExpectation,
}
pub struct LocalServiceLease<'a> {
    gate: Gate<'a>,
    revision: u64,
}

/// Borrowed transport data from the explicitly selected profile, never identity or a grant.
pub enum S3CredentialView<'a> {
    Local(&'a LocalS3),
    AwsChain { profile: Option<&'a str> },
    Anonymous,
}
pub struct StandaloneS3View<'a> {
    pub location: &'a S3Location,
    pub credential: S3CredentialView<'a>,
}

#[derive(Clone, Copy)]
pub(super) enum ProfileSelection<'a> {
    Active,
    Standalone(Option<&'a str>),
}

impl RoleIssuer {
    /// Capture the exact standalone transport pair without changing the active login.
    pub fn capture_standalone_s3_service<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        provider: &str,
        profile: Option<&str>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority_selected(
            Authority::Caller(caller),
            ServicePurpose::HubRead,
            Some(provider),
            ProfileSelection::Standalone(profile),
        )
    }
    /// Capture the exact standalone Hub transport for the current Role authority.
    pub fn capture_role_standalone_s3_service<'a>(
        &'a self,
        role: &'a VerifiedRoleGrant<'a>,
        provider: &str,
        profile: Option<&str>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority_selected(
            Authority::Role(role),
            ServicePurpose::HubRead,
            Some(provider),
            ProfileSelection::Standalone(profile),
        )
    }
    /// Capture embeddings with the imported Knowledge read's own authority.
    pub fn capture_imported_knowledge_embedding_service<'a>(
        &'a self,
        admission: &'a ImportedKnowledgeReadAdmission<'a>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::ImportedKnowledge(admission),
            ServicePurpose::Embedding,
            None,
        )
    }

    /// Capture embeddings with the imported AST read's own authority.
    pub fn capture_imported_ast_embedding_service<'a>(
        &'a self,
        admission: &'a ImportedAstReadAdmission<'a>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::ImportedAst(admission),
            ServicePurpose::Embedding,
            None,
        )
    }

    /// Capture optional reranking with the imported Knowledge read's own authority.
    pub fn capture_imported_knowledge_rerank_service<'a>(
        &'a self,
        admission: &'a ImportedKnowledgeReadAdmission<'a>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::ImportedKnowledge(admission),
            ServicePurpose::Rerank,
            None,
        )
    }

    /// Capture optional reranking with the imported AST read's own authority.
    pub fn capture_imported_ast_rerank_service<'a>(
        &'a self,
        admission: &'a ImportedAstReadAdmission<'a>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::ImportedAst(admission),
            ServicePurpose::Rerank,
            None,
        )
    }

    /// Capture HubRead from the installed Hub admission's current Web authority.
    pub fn capture_installed_hub_service<'a>(
        &'a self,
        read: &'a InstalledHubReadAdmission<'a>,
        provider: &str,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::InstalledHub(read),
            ServicePurpose::HubRead,
            Some(provider),
        )
    }

    /// Select HubWrite only for this Web publication's source and principal.
    pub fn capture_web_hub_write_service<'a>(
        &'a self,
        publication: &'a WebHubPublicationAdmission<'a>,
        provider: &str,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::WebHubPublication(publication),
            ServicePurpose::HubWrite,
            Some(provider),
        )
    }

    /// Select only an explicitly admitted Standalone S3 provider/profile.
    /// The retained Web identity comes from the installed Hub read, not that profile.
    pub fn capture_installed_hub_standalone_s3_service<'a>(
        &'a self,
        read: &'a InstalledHubReadAdmission<'a>,
        provider: &str,
        profile: Option<&str>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority_selected(
            Authority::InstalledHub(read),
            ServicePurpose::HubRead,
            Some(provider),
            ProfileSelection::Standalone(profile),
        )
    }

    /// Capture HubRead using the imported AST admission's own caller and Original.
    pub fn capture_imported_ast_hub_service<'a>(
        &'a self,
        admission: &'a ImportedAstReadAdmission<'a>,
        provider: &str,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::ImportedAst(admission),
            ServicePurpose::HubRead,
            Some(provider),
        )
    }

    /// Capture HubRead using the imported Knowledge admission's own caller and Original.
    pub fn capture_imported_knowledge_hub_service<'a>(
        &'a self,
        admission: &'a ImportedKnowledgeReadAdmission<'a>,
        provider: &str,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::ImportedKnowledge(admission),
            ServicePurpose::HubRead,
            Some(provider),
        )
    }

    /// Capture an embedding service for one admitted Web read.
    /// The admission remains bound to its caller, module, project and Original.
    pub fn capture_web_embedding_service<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        admission: &'a ProjectReadAdmission<'a>,
        original: &'a RequestContext<'a>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::Web {
                caller,
                admission,
                original,
            },
            ServicePurpose::Embedding,
            None,
        )
    }

    /// Capture optional reranking for this exact admitted Web read.
    pub fn capture_web_rerank_service<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        admission: &'a ProjectReadAdmission<'a>,
        original: &'a RequestContext<'a>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::Web {
                caller,
                admission,
                original,
            },
            ServicePurpose::Rerank,
            None,
        )
    }

    pub fn capture_service<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        purpose: ServicePurpose,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(Authority::Caller(caller), purpose, None)
    }
    pub fn capture_role_service<'a>(
        &'a self,
        role: &'a VerifiedRoleGrant<'a>,
        purpose: ServicePurpose,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(Authority::Role(role), purpose, None)
    }
    /// Capture Hub read transport for one explicitly selected configured provider.
    pub fn capture_hub_service<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        provider_name: &str,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::Caller(caller),
            ServicePurpose::HubRead,
            Some(provider_name),
        )
    }
    pub fn capture_role_hub_service<'a>(
        &'a self,
        role: &'a VerifiedRoleGrant<'a>,
        provider_name: &str,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::Role(role),
            ServicePurpose::HubRead,
            Some(provider_name),
        )
    }
    pub fn capture_hub_write_service<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        provider_name: &str,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::Caller(caller),
            ServicePurpose::HubWrite,
            Some(provider_name),
        )
    }
    pub fn capture_role_hub_write_service<'a>(
        &'a self,
        role: &'a VerifiedRoleGrant<'a>,
        provider_name: &str,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::Role(role),
            ServicePurpose::HubWrite,
            Some(provider_name),
        )
    }
    /// Capture a finite Presence read from the caller's active provider.
    pub fn capture_presence_read_service<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        provider_name: &str,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::Caller(caller),
            ServicePurpose::PresenceRead,
            Some(provider_name),
        )
    }
    /// Capture Presence read using a current Role grant with RemoteRequest.
    pub fn capture_role_presence_read_service<'a>(
        &'a self,
        role: &'a VerifiedRoleGrant<'a>,
        provider_name: &str,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::Role(role),
            ServicePurpose::PresenceRead,
            Some(provider_name),
        )
    }
    /// Capture a finite publisher from an actual Runtime or Local caller.
    pub fn capture_presence_publish_service<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        provider_name: &str,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::Caller(caller),
            ServicePurpose::PresencePublish,
            Some(provider_name),
        )
    }
    pub fn capture_incoming_service<'a>(
        &'a self,
        caller: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        purpose: ServicePurpose,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::Incoming {
                caller,
                verifier,
                original,
            },
            purpose,
            None,
        )
    }
    fn capture_service_authority<'a>(
        &'a self,
        authority: Authority<'a>,
        purpose: ServicePurpose,
        provider_name: Option<&str>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority_selected(
            authority,
            purpose,
            provider_name,
            ProfileSelection::Active,
        )
    }
    pub(super) fn capture_service_authority_selected<'a>(
        &'a self,
        authority: Authority<'a>,
        purpose: ServicePurpose,
        provider_name: Option<&str>,
        profile_selection: ProfileSelection<'_>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        let profile_name = match profile_selection {
            ProfileSelection::Active => None,
            ProfileSelection::Standalone(name) => name,
        };
        authority.check(self)?;
        if matches!(authority, Authority::InstalledHub(_)) && purpose != ServicePurpose::HubRead {
            return Err(AuthError::Denied.into());
        }
        if matches!(
            authority,
            Authority::WebHubPublication(_) | Authority::WebHubCatalogWrite(_)
        ) && purpose != ServicePurpose::HubWrite
        {
            return Err(AuthError::Denied.into());
        }
        if (purpose.hub() || purpose.presence() || purpose == ServicePurpose::RecordStorage)
            && provider_name.is_none()
        {
            return Err(AuthError::Denied.into());
        }
        if let Some(name) = provider_name {
            if name.is_empty() {
                return Err(AuthError::Invalid.into());
            }
            if name.len() > self.limits.max_scope_bytes {
                return Err(AuthError::Limit.into());
            }
        }
        if let Some(name) = profile_name
            && (name.is_empty() || name.len() > self.limits.max_scope_bytes)
        {
            return Err(AuthError::Invalid.into());
        }
        if purpose.inventory() && !matches!(authority, Authority::Caller(_)) {
            return Err(AuthError::Denied.into());
        }
        if purpose == ServicePurpose::PresencePublish && !matches!(authority, Authority::Caller(_))
        {
            return Err(AuthError::Denied.into());
        }
        let generation = {
            let state = self.lock()?;
            if !state.service_enabled {
                return Err(AuthError::Denied.into());
            }
            state.service_generation
        };
        // Runtime/Local/Role have one effective RemoteRequest ceiling for this
        // capture, independent of the service rule. Read that real proof once;
        // the returned lease still repeats the live check at every barrier.
        let remote_allowed =
            if matches!(
                authority,
                Authority::Incoming { .. } | Authority::Records(_)
            ) || (matches!(
                authority,
                Authority::Web { .. } | Authority::ImportedAst(_) | Authority::ImportedKnowledge(_)
            ) && matches!(purpose, ServicePurpose::Embedding | ServicePurpose::Rerank)
                && authority.hub_web_caller().is_some())
            {
                // Imported Web inference must validate each exact provider/resource rule.
                None
            } else if let Some(rule) = self.policy.services.rules.iter().find(|rule| {
                rule.origin == authority.origin()
                    && rule.binding.purpose() == purpose
                    && provider_name.is_none_or(|name| rule.binding.provider() == name)
                    && !matches!(rule.binding, Binding::Local { .. })
            }) {
                Some(match authority.remote(rule) {
                    Ok(_) => true,
                    Err(ServiceUseError::Auth(AuthError::Denied)) => {
                        authority.check(self)?;
                        false
                    }
                    Err(error) => return Err(error),
                })
            } else {
                Some(false)
            };
        let candidate = |rule: &Rule| -> Result<bool, ServiceUseError> {
            if rule.origin != authority.origin()
                || rule.binding.purpose() != purpose
                || provider_name.is_some_and(|name| rule.binding.provider() != name)
            {
                return Ok(false);
            }
            match (&rule.binding, profile_selection) {
                (Binding::StandaloneS3 { profile, .. }, ProfileSelection::Standalone(name))
                    if profile.as_deref() == name => {}
                (Binding::StandaloneS3 { .. }, _) | (_, ProfileSelection::Standalone(_)) => {
                    return Ok(false);
                }
                _ => (),
            }
            if matches!(rule.binding, Binding::Local { .. }) {
                return Ok(true);
            }
            if let Some(allowed) = remote_allowed {
                return Ok(allowed);
            }
            match authority.remote(rule) {
                Ok(_) => Ok(true),
                Err(ServiceUseError::Auth(AuthError::Denied)) => {
                    authority.check(self)?;
                    Ok(false)
                }
                Err(error) => Err(error),
            }
        };
        let mut admitted = false;
        for rule in &self.policy.services.rules {
            admitted |= candidate(rule)?;
        }
        if !admitted {
            return Err(AuthError::Denied.into());
        }
        let mut selection = {
            let (guard, mut snapshot) = self.local.load_locked(authority.original())?;
            authority.check(self)?;
            let document = snapshot.document();
            let (provider, profile) = match authority {
                _ if matches!(profile_selection, ProfileSelection::Standalone(None)) => (
                    document
                        .providers()
                        .iter()
                        .position(|p| Some(p.name()) == provider_name)
                        .ok_or(ProfileError::Missing)?,
                    None,
                ),
                _ if profile_name.is_some() => {
                    let name = profile_name.ok_or(AuthError::Invalid)?;
                    let profile = document
                        .profiles()
                        .iter()
                        .position(|p| p.name() == name)
                        .ok_or(ProfileError::Missing)?;
                    let selected = &document.profiles()[profile];
                    if Some(selected.provider()) != provider_name {
                        return Err(AuthError::Denied.into());
                    }
                    let provider = document
                        .providers()
                        .iter()
                        .position(|p| Some(p.name()) == provider_name)
                        .ok_or(ProfileError::Missing)?;
                    if document.providers()[provider].revision() != selected.provider_revision() {
                        return Err(ProfileError::Stale.into());
                    }
                    (provider, Some(profile))
                }
                authority
                    if matches!(
                        authority,
                        Authority::Web { .. }
                            | Authority::ImportedAst(_)
                            | Authority::ImportedKnowledge(_)
                    ) && matches!(
                        purpose,
                        ServicePurpose::Embedding | ServicePurpose::Rerank
                    ) && authority.hub_web_caller().is_some() =>
                {
                    // A configured local profile remains local. Remote inference always
                    // retains the imported read's own Broker principal and provider.
                    let local = match document.active(authority.original()) {
                        Ok((provider, profile)) => {
                            let service = match purpose {
                                ServicePurpose::Embedding => &provider.config().embedding,
                                ServicePurpose::Rerank => &provider.config().rerank,
                                _ => unreachable!("inference purpose"),
                            };
                            (matches!(service, AiService::Local(_))
                                && self.policy.services.rules.iter().any(|rule| {
                                    rule.origin == authority.origin()
                                        && rule.binding.purpose() == purpose
                                        && matches!(rule.binding, Binding::Local { .. })
                                        && rule.matches(provider, service)
                                }))
                            .then_some((provider, profile))
                        }
                        Err(ProfileError::NoActive) => None,
                        Err(error) => return Err(error.into()),
                    };
                    if let Some((provider, profile)) = local {
                        (
                            document
                                .providers()
                                .iter()
                                .position(|entry| entry.name() == provider.name())
                                .ok_or(ProfileError::Missing)?,
                            Some(
                                document
                                    .profiles()
                                    .iter()
                                    .position(|entry| entry.name() == profile.name())
                                    .ok_or(ProfileError::Missing)?,
                            ),
                        )
                    } else {
                        let mut remote_rule = false;
                        for rule in &self.policy.services.rules {
                            if !matches!(rule.binding, Binding::Local { .. }) && candidate(rule)? {
                                remote_rule = true;
                                break;
                            }
                        }
                        if !remote_rule {
                            return Err(AuthError::Denied.into());
                        }
                        let (caller, _, bound_provider) =
                            authority.hub_web_caller().ok_or(AuthError::Denied)?;
                        if provider_name.is_some_and(|name| name != bound_provider) {
                            return Err(AuthError::Denied.into());
                        }
                        let provider = document
                            .providers()
                            .iter()
                            .position(|entry| entry.name() == bound_provider)
                            .ok_or(ProfileError::Missing)?;
                        if document.providers()[provider].revision() != caller.provider_revision() {
                            return Err(ProfileError::Stale.into());
                        }
                        (provider, None)
                    }
                }
                authority
                    if (matches!(
                        purpose,
                        ServicePurpose::HubRead | ServicePurpose::RecordStorage
                    ) || matches!(
                        authority,
                        Authority::WebHubPublication(_) | Authority::WebHubCatalogWrite(_)
                    )) && authority.hub_web_caller().is_some() =>
                {
                    let (caller, _, bound_provider) =
                        authority.hub_web_caller().ok_or(AuthError::Denied)?;
                    if Some(bound_provider) != provider_name {
                        return Err(AuthError::Denied.into());
                    }
                    let provider = document
                        .providers()
                        .iter()
                        .position(|p| p.name() == bound_provider)
                        .ok_or(ProfileError::Missing)?;
                    if document.providers()[provider].revision() != caller.provider_revision() {
                        return Err(ProfileError::Stale.into());
                    }
                    if let Authority::WebHubCatalogWrite(write) = authority {
                        write.check_provider(&document.providers()[provider])?;
                    }
                    (provider, None)
                }
                Authority::Records(admission) if admission.principal().provider().is_some() => {
                    let name = admission.principal().provider().ok_or(AuthError::Denied)?;
                    let provider = document
                        .providers()
                        .iter()
                        .position(|entry| entry.name() == name)
                        .ok_or(ProfileError::Missing)?;
                    (provider, None)
                }
                Authority::Incoming { caller, .. } => {
                    let mut selected = None;
                    for (index, rule) in self.policy.services.rules.iter().enumerate() {
                        if candidate(rule)? {
                            selected = Some(index);
                            break;
                        }
                    }
                    let index = selected.ok_or(AuthError::Denied)?;
                    let provider = document
                        .providers()
                        .iter()
                        .position(|p| {
                            p.name() == self.policy.services.rules[index].binding.provider()
                        })
                        .ok_or(ProfileError::Missing)?;
                    if document.providers()[provider].revision() != caller.provider_revision() {
                        return Err(ProfileError::Stale.into());
                    }
                    (provider, None)
                }
                _ => match document.active(authority.original()) {
                    Ok((p, a)) => (
                        document
                            .providers()
                            .iter()
                            .position(|v| v.name() == p.name())
                            .ok_or(ProfileError::Missing)?,
                        Some(
                            document
                                .profiles()
                                .iter()
                                .position(|v| v.name() == a.name())
                                .ok_or(ProfileError::Missing)?,
                        ),
                    ),
                    Err(ProfileError::NoActive) if purpose == ServicePurpose::HubRead => (
                        document
                            .providers()
                            .iter()
                            .position(|p| Some(p.name()) == provider_name)
                            .ok_or(ProfileError::Missing)?,
                        None,
                    ),
                    Err(ProfileError::NoActive) => {
                        let local = self.policy.services.rules.iter().any(|r| {
                            r.origin == authority.origin()
                                && r.binding.purpose() == purpose
                                && matches!(r.binding, Binding::Local { .. })
                        });
                        if !local {
                            return Err(AuthError::Denied.into());
                        }
                        let (mut physical, _) = self.local.snapshot_parts(&mut snapshot);
                        physical.recheck(authority.original().snapshot())?;
                        authority.check(self)?;
                        self.check_service_generation(generation)?;
                        original_check(authority.original())?;
                        return Err(ProfileError::NoActive.into());
                    }
                    Err(e) => return Err(e.into()),
                },
            };
            let selected = &document.providers()[provider];
            if provider_name.is_some_and(|name| selected.name() != name) {
                return Err(AuthError::Denied.into());
            }
            let service = match purpose {
                ServicePurpose::Embedding => Some(&selected.config().embedding),
                ServicePurpose::Rerank => Some(&selected.config().rerank),
                ServicePurpose::InventoryRead
                | ServicePurpose::InventoryPublish
                | ServicePurpose::HubRead
                | ServicePurpose::RecordStorage
                | ServicePurpose::HubWrite
                | ServicePurpose::PresenceRead
                | ServicePurpose::PresencePublish => None,
            };
            if matches!(service, Some(AiService::Disabled)) {
                return Err(ServiceUseError::Disabled);
            }
            let mut chosen = None;
            for (i, rule) in self.policy.services.rules.iter().enumerate() {
                if !candidate(rule)? {
                    continue;
                }
                let matches = if let Binding::StandaloneS3 { .. } = rule.binding {
                    (matches!(
                        selected.config().kind,
                        ProviderKind::Local { broker: None, .. }
                    ) || (purpose == ServicePurpose::RecordStorage
                        && matches!(selected.config().kind, ProviderKind::Local { .. })))
                        && profile.is_none_or(|index| {
                            matches!(
                                document.profiles()[index]
                                    .data()
                                    .map(|data| &data.credentials),
                                Some(ProfileCredentials::Local(_))
                            )
                        })
                        && selected.config().s3.source != S3Source::BrokerScope
                } else if purpose.hub()
                    || purpose.presence()
                    || purpose == ServicePurpose::RecordStorage
                {
                    rule.matches_broker(selected, profile.map(|i| &document.profiles()[i]))
                } else if purpose.inventory() {
                    rule.matches_inventory(selected, self.limits.max_scope_bytes)?
                } else {
                    service.is_some_and(|service| rule.matches(selected, service))
                };
                if matches {
                    chosen = Some(i);
                    break;
                }
            }
            let rule = chosen.ok_or(AuthError::Denied)?;
            let mode = if matches!(
                self.policy.services.rules[rule].binding,
                Binding::StandaloneS3 { .. }
            ) {
                ServiceMode::StandaloneS3
            } else {
                match self.policy.services.rules[rule].credential {
                    None => ServiceMode::LocalAuthorized,
                    Some(CredentialSource::DirectKey) => ServiceMode::Direct,
                    Some(CredentialSource::NativeAccess) => ServiceMode::BrokerNative,
                    Some(CredentialSource::LocalBrokerKey) => ServiceMode::BrokerKey,
                    Some(CredentialSource::IncomingBearer) => ServiceMode::BrokerIncoming,
                    Some(CredentialSource::Anonymous) => ServiceMode::BrokerAnonymous,
                }
            };
            let authorization = if mode == ServiceMode::LocalAuthorized {
                ServiceAuthorization::Local
            } else {
                authority.remote(&self.policy.services.rules[rule])?
            };
            self.local.revalidate(&mut snapshot, authority.original())?;
            if let Some(guard) = &guard {
                guard
                    .revalidate()
                    .map_err(|error| AuthStoreError::Core(error.into()))?;
            }
            authority.check(self)?;
            self.check_service_generation(generation)?;
            original_check(authority.original())?;
            ServiceSelection {
                issuer: self,
                authority,
                snapshot,
                rule,
                generation,
                authorization,
                provider,
                profile,
                mode,
            }
        };
        selection.mode()?;
        Ok(selection)
    }
    fn check_service_generation(&self, generation: u64) -> Result<(), AuthError> {
        let state = self.lock()?;
        if !state.service_enabled || state.service_generation != generation {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
    pub fn revoke_service_policy(&self, parent: &super::AdminParent<'_>) -> Result<(), AuthError> {
        parent.check()?;
        parent.caller.source.check_physical(self, parent.request)?;
        let mut state = self.lock()?;
        self.parent_locked(parent, &state)?;
        if !self
            .origin_policy(parent.caller.source.origin())
            .administrative
            .contains(&Permission::ManageAccessGrants)
        {
            return Err(AuthError::Denied);
        }
        revoke(&mut state)
    }
    pub fn revoke_service_policy_with_role(
        &self,
        role: &VerifiedRoleGrant<'_>,
    ) -> Result<(), AuthError> {
        if !std::ptr::eq(self, role.issuer)
            || role.authorize(Permission::ManageAccessGrants)? != Obligation::Unconditional
        {
            return Err(AuthError::Denied);
        }
        role.source.check_physical(self, role.request)?;
        let mut state = self.lock()?;
        role.check_locked(&state)?;
        revoke(&mut state)
    }
}
fn revoke(state: &mut super::State) -> Result<(), AuthError> {
    if state.service_enabled {
        let generation = state
            .service_generation
            .checked_add(1)
            .ok_or(AuthError::Limit)?;
        state.service_generation = generation;
        state.service_enabled = false;
    }
    Ok(())
}
impl Rule {
    fn matches_broker(
        &self,
        provider: &crate::profile::ProviderRecord,
        profile: Option<&crate::profile::ProfileRecord>,
    ) -> bool {
        let Binding::Broker {
            provider: name,
            purpose,
            endpoint,
            resource,
        } = &self.binding
        else {
            return false;
        };
        if provider.name() != name {
            return false;
        }
        if !purpose.hub() && !purpose.presence() && *purpose != ServicePurpose::RecordStorage {
            return false;
        }
        let (broker, native) = match &provider.config().kind {
            ProviderKind::Broker(broker) => (broker, true),
            ProviderKind::Local {
                broker: Some(broker),
                ..
            } => (broker, false),
            _ => return false,
        };
        broker.endpoint == *endpoint
            && broker.mcp_resource == *resource
            && match self.credential {
                Some(CredentialSource::IncomingBearer) => {
                    ((self.origin == ServiceOrigin::Web && purpose.hub())
                        || (*purpose == ServicePurpose::RecordStorage
                            && matches!(
                                self.origin,
                                ServiceOrigin::Web | ServiceOrigin::BrokerIncoming
                            )))
                        && native
                        && profile.is_none()
                }
                Some(CredentialSource::NativeAccess) => {
                    native && profile.is_some_and(|profile| profile.broker_credentials().is_some())
                }
                Some(CredentialSource::Anonymous) => {
                    *purpose == ServicePurpose::HubRead
                        && !native
                        && broker.allow_anonymous
                        && (profile.is_none()
                            || matches!(profile.and_then(|profile| profile.data()).map(|data| &data.credentials), Some(ProfileCredentials::Local(keys)) if keys.broker_key.is_none()))
                }
                Some(CredentialSource::LocalBrokerKey) => {
                    (*purpose == ServicePurpose::HubWrite || purpose.presence())
                        && !native
                        && matches!(profile.and_then(|profile| profile.data()).map(|data| &data.credentials), Some(ProfileCredentials::Local(keys)) if keys.broker_key.is_some())
                }
                _ => false,
            }
    }
    fn matches_inventory(
        &self,
        provider: &crate::profile::ProviderRecord,
        max_bytes: usize,
    ) -> Result<bool, AuthError> {
        let Binding::BrokerInventory {
            provider: name,
            endpoint,
            ..
        } = &self.binding
        else {
            return Ok(false);
        };
        if provider.name() != name {
            return Ok(false);
        }
        let broker = match (&provider.config().kind, self.credential) {
            (
                ProviderKind::Local {
                    broker: Some(broker),
                    ..
                },
                Some(CredentialSource::LocalBrokerKey),
            )
            | (ProviderKind::Broker(broker), Some(CredentialSource::NativeAccess)) => broker,
            _ => return Ok(false),
        };
        Ok(inventory_endpoint(&broker.endpoint, max_bytes)?.as_str() == endpoint)
    }
    fn matches(&self, provider: &crate::profile::ProviderRecord, service: &AiService) -> bool {
        if provider.name() != self.binding.provider() {
            return false;
        }
        match (&self.binding, service) {
            (Binding::Local { execution, .. }, AiService::Local(actual)) => {
                matches!(provider.config().kind, ProviderKind::Local { .. }) && execution == actual
            }
            (
                Binding::Direct {
                    protocol,
                    endpoint,
                    model,
                    model_revision,
                    recipe,
                    dimensions,
                    request_dimensions,
                    batch_endpoint,
                    ..
                },
                AiService::Direct {
                    protocol: p,
                    endpoint: e,
                    model: m,
                    model_revision: mr,
                    recipe: re,
                    dimensions: d,
                    request_dimensions: r,
                    batch_endpoint: b,
                },
            ) => {
                protocol == p
                    && endpoint == e
                    && model == m
                    && model_revision == mr
                    && recipe == re
                    && dimensions == d
                    && request_dimensions == r
                    && batch_endpoint == b
                    && matches!(provider.config().kind, ProviderKind::Local { .. })
            }
            (
                Binding::Broker {
                    endpoint, resource, ..
                },
                AiService::Broker,
            ) => {
                let (broker, native) = match &provider.config().kind {
                    ProviderKind::Broker(b) => (b, true),
                    ProviderKind::Local {
                        broker: Some(b), ..
                    } => (b, false),
                    _ => return false,
                };
                broker.endpoint == *endpoint
                    && broker.mcp_resource == *resource
                    && match self.credential {
                        Some(CredentialSource::NativeAccess) => native,
                        Some(CredentialSource::LocalBrokerKey) => !native,
                        Some(CredentialSource::Anonymous) => !native && broker.allow_anonymous,
                        Some(CredentialSource::IncomingBearer) => true,
                        _ => false,
                    }
            }
            _ => false,
        }
    }
}
impl ServiceSelection<'_> {
    fn gate<'s>(&'s mut self) -> (Gate<'s>, &'s crate::profile::AuthDocument) {
        let (observation, document) = self.issuer.local.snapshot_parts(&mut self.snapshot);
        // Runtime authority is bound to its own key. Revalidate the selected
        // records, not the inode of the shared auth document: another profile's
        // refresh must not invalidate this operation. A credential-bearing
        // selection additionally keeps its exact profile/token revisions.
        let runtime_selection = match (self.mode, self.authority, self.profile) {
            (
                mode @ (ServiceMode::LocalAuthorized
                | ServiceMode::Direct
                | ServiceMode::BrokerNative
                | ServiceMode::BrokerKey
                | ServiceMode::BrokerAnonymous),
                Authority::Caller(caller),
                Some(profile),
            ) if caller.source.origin() == Origin::Runtime => Some((
                &document.providers()[self.provider],
                &document.profiles()[profile],
                !matches!(mode, ServiceMode::LocalAuthorized),
            )),
            _ => None,
        };
        (
            Gate {
                issuer: self.issuer,
                authority: self.authority,
                observation: Mutex::new(observation),
                rule: self.rule,
                generation: self.generation,
                authorization: self.authorization,
                runtime_selection,
            },
            document,
        )
    }
    pub fn mode(&mut self) -> Result<ServiceMode, ServiceUseError> {
        let mode = self.mode;
        self.gate().0.check()?;
        Ok(mode)
    }
    /// The captured request after checking current authority and profile state.
    ///
    /// Consumers can compare its snapshot identity before credential verification.
    /// This metadata does not validate a JWT or authorize access to a project;
    /// retain and check the resulting credential lease before effects.
    ///
    /// ```no_run
    /// # fn same_original(
    /// #     selection: &mut graphit_auth::ServiceSelection<'_>,
    /// #     request: &graphit_hub::policy::RequestContext<'_>,
    /// # ) -> Result<bool, graphit_auth::ServiceUseError> {
    /// Ok(std::sync::Arc::ptr_eq(
    ///     selection.original()?.snapshot(),
    ///     request.snapshot(),
    /// ))
    /// # }
    /// ```
    pub fn original(&mut self) -> Result<&RequestContext<'_>, ServiceUseError> {
        self.mode()?;
        Ok(self.authority.original())
    }
    /// Checked configuration data; the consumer must retain and check its lease.
    pub fn provider_config(&mut self) -> Result<&crate::profile::ProviderConfig, ServiceUseError> {
        self.mode()?;
        Ok(self.snapshot.document().providers()[self.provider].config())
    }
    /// Checked binding data from the same captured provider revision.
    /// Consumers may construct their verifier before borrowing a Native credential.
    pub fn binding(&mut self) -> Result<ServiceBindingView<'_>, ServiceUseError> {
        self.mode()?;
        let revision = self.snapshot.document().providers()[self.provider].revision();
        Ok(self.issuer.policy.services.rules[self.rule]
            .binding
            .view(revision))
    }
    pub fn local(&mut self) -> Result<LocalServiceLease<'_>, ServiceUseError> {
        if self.mode()? != ServiceMode::LocalAuthorized {
            return Err(ServiceUseError::WrongMode);
        }
        let revision = self.snapshot.document().providers()[self.provider].revision();
        let gate = self.gate().0;
        gate.check()?;
        Ok(LocalServiceLease { gate, revision })
    }
    pub fn credential(&mut self) -> Result<ServiceCredentialLease<'_>, ServiceUseError> {
        let mode = self.mode()?;
        if matches!(
            mode,
            ServiceMode::LocalAuthorized | ServiceMode::BrokerNative
        ) {
            return Err(ServiceUseError::WrongMode);
        }
        let provider = self.provider;
        let profile = self.profile;
        let (gate, document) = self.gate();
        gate.check()?;
        let proof = match mode {
            ServiceMode::StandaloneS3 => {
                let keys = profile
                    .map(|index| {
                        match document.profiles()[index]
                            .data()
                            .map(|data| &data.credentials)
                        {
                            Some(ProfileCredentials::Local(keys)) => Ok(keys),
                            _ => Err(AuthError::Denied),
                        }
                    })
                    .transpose()?;
                let location = &document.providers()[provider].config().s3;
                let expires = keys
                    .and_then(|keys| keys.s3.as_ref())
                    .and_then(|s3| s3.expires_at)
                    .map(|seconds| {
                        let utc = u64::try_from(seconds)
                            .ok()
                            .and_then(|seconds| {
                                UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
                            })
                            .ok_or(ProfileError::InvalidDocument)?;
                        let remaining = utc
                            .duration_since(SystemTime::now())
                            .map_err(|_| ServiceUseError::NeedsRefresh)?;
                        let mono = Instant::now()
                            .checked_add(remaining)
                            .ok_or(ProfileError::InvalidDocument)?;
                        Ok::<_, ServiceUseError>((utc, mono))
                    })
                    .transpose()?;
                let user = if let Authority::InstalledHub(read) = gate.authority {
                    let (caller, verifier, _) = read.web_service_caller();
                    Some(
                        gate.issuer
                            .user_from_web(caller, verifier, gate.authority.original())?,
                    )
                } else {
                    None
                };
                CredentialProof::StandaloneS3 {
                    location,
                    keys,
                    expires,
                    user,
                }
            }
            ServiceMode::BrokerAnonymous
                if profile.is_none()
                    && gate.issuer.policy.services.rules[gate.rule]
                        .binding
                        .purpose()
                        == ServicePurpose::HubRead =>
            {
                CredentialProof::Anonymous
            }
            ServiceMode::BrokerIncoming => match gate.authority {
                Authority::Records(admission) => match admission.principal() {
                    super::record_storage::RecordStoragePrincipal::Incoming {
                        caller,
                        verifier,
                        ..
                    } => CredentialProof::Incoming { caller, verifier },
                    super::record_storage::RecordStoragePrincipal::Web {
                        caller, verifier, ..
                    } => CredentialProof::Web { caller, verifier },
                    _ => return Err(AuthError::Denied.into()),
                },
                Authority::Incoming {
                    caller, verifier, ..
                } => CredentialProof::Incoming { caller, verifier },
                authority => {
                    let (caller, verifier, _) =
                        authority.hub_web_caller().ok_or(AuthError::Denied)?;
                    let purpose = gate.issuer.policy.services.rules[gate.rule]
                        .binding
                        .purpose();
                    let web_inference =
                        matches!(
                            authority,
                            Authority::Web { .. }
                                | Authority::ImportedAst(_)
                                | Authority::ImportedKnowledge(_)
                        ) && matches!(purpose, ServicePurpose::Embedding | ServicePurpose::Rerank);
                    if purpose != ServicePurpose::HubRead
                        && !web_inference
                        && !(matches!(
                            authority,
                            Authority::WebHubPublication(_) | Authority::WebHubCatalogWrite(_)
                        ) && purpose == ServicePurpose::HubWrite)
                    {
                        return Err(AuthError::Denied.into());
                    }
                    CredentialProof::Web { caller, verifier }
                }
            },
            _ => {
                let ProfileCredentials::Local(keys) = &document.profiles()
                    [profile.ok_or(AuthError::Denied)?]
                .data()
                .ok_or(AuthError::Denied)?
                .credentials
                else {
                    return Err(AuthError::Denied.into());
                };
                let raw = match mode {
                    ServiceMode::Direct => match gate.issuer.policy.services.rules[gate.rule]
                        .binding
                        .purpose()
                    {
                        ServicePurpose::Embedding => keys.embedding_api_key.as_ref(),
                        ServicePurpose::Rerank => keys.rerank_api_key.as_ref(),
                        ServicePurpose::InventoryRead
                        | ServicePurpose::InventoryPublish
                        | ServicePurpose::HubRead
                        | ServicePurpose::RecordStorage
                        | ServicePurpose::HubWrite
                        | ServicePurpose::PresenceRead
                        | ServicePurpose::PresencePublish => {
                            return Err(ServiceUseError::WrongMode);
                        }
                    },
                    ServiceMode::BrokerKey => keys.broker_key.as_ref(),
                    ServiceMode::BrokerAnonymous => {
                        if keys.broker_key.is_some() {
                            return Err(AuthError::Denied.into());
                        }
                        return Ok(ServiceCredentialLease {
                            gate,
                            proof: CredentialProof::Anonymous,
                            revision: document.providers()[provider].revision(),
                        });
                    }
                    _ => return Err(ServiceUseError::WrongMode),
                }
                .ok_or(ServiceUseError::MissingCredential)?
                .expose();
                if raw.is_empty() {
                    return Err(ServiceUseError::MissingCredential);
                }
                CredentialProof::Key(raw)
            }
        };
        let lease = ServiceCredentialLease {
            gate,
            proof,
            revision: document.providers()[provider].revision(),
        };
        lease.check()?;
        if mode == ServiceMode::StandaloneS3 {
            lease.standalone_s3()?;
        }
        Ok(lease)
    }
    /// Identify the exact Native profile before asking an administrative owner to rotate it.
    pub fn native_profile_revision(
        &mut self,
    ) -> Result<NativeProfileRevision<'_>, ServiceUseError> {
        if self.mode()? != ServiceMode::BrokerNative {
            return Err(ServiceUseError::WrongMode);
        }
        let document = self.snapshot.document();
        let provider = &document.providers()[self.provider];
        let profile = &document.profiles()[self.profile.ok_or(AuthError::Denied)?];
        let credentials = profile.broker_credentials().ok_or(AuthError::Denied)?;
        Ok(NativeProfileRevision {
            provider: provider.name(),
            profile: profile.name(),
            issuer: credentials.issuer,
            subject: credentials.subject,
            revisions: crate::profile::TokenExpectation {
                document_revision: document.revision(),
                provider_revision: provider.revision(),
                profile_revision: profile.revision(),
                token_revision: profile.token_revision(),
            },
        })
    }
    pub async fn native_credential<'s>(
        &'s mut self,
        verifier: &'s BrokerVerifier,
    ) -> Result<ServiceCredentialLease<'s>, ServiceUseError> {
        if self.mode()? != ServiceMode::BrokerNative {
            return Err(ServiceUseError::WrongMode);
        }
        let provider = self.provider;
        let profile = self.profile.ok_or(AuthError::Denied)?;
        let (gate, document) = self.gate();
        gate.check()?;
        let record = &document.providers()[provider];
        let credentials = document.profiles()[profile]
            .broker_credentials()
            .ok_or(AuthError::Denied)?;
        let issuer = credentials.issuer;
        let subject = credentials.subject;
        let margin = if credentials.device {
            Duration::ZERO
        } else {
            gate.issuer.policy.services.margin
        };
        if credentials.s3_disabled
            && gate.issuer.policy.services.rules[gate.rule]
                .binding
                .purpose()
                .hub()
        {
            return Err(ServiceUseError::Disabled);
        }
        let (endpoint, resource) = match &gate.issuer.policy.services.rules[gate.rule].binding {
            Binding::Broker {
                endpoint, resource, ..
            } => (endpoint.as_str(), resource.as_deref()),
            Binding::BrokerInventory { .. } => {
                let ProviderKind::Broker(broker) = &record.config().kind else {
                    return Err(AuthError::Denied.into());
                };
                (broker.endpoint.as_str(), broker.mcp_resource.as_deref())
            }
            _ => return Err(ServiceUseError::WrongMode),
        };
        if issuer != endpoint
            || !verifier.matches_service_binding(endpoint, resource, record.revision())
        {
            return Err(AuthError::Denied.into());
        }
        let expiration = u64::try_from(credentials.expires_at)
            .ok()
            .and_then(|n| {
                // Renewable OAuth profiles store UTC microseconds; Device is access-only seconds.
                let elapsed = if credentials.device {
                    Duration::from_secs(n)
                } else {
                    Duration::from_micros(n)
                };
                UNIX_EPOCH.checked_add(elapsed)
            })
            .ok_or(ProfileError::InvalidDocument)?;
        if !expiration
            .duration_since(SystemTime::now())
            .is_ok_and(|n| n > margin)
        {
            return Err(if credentials.device {
                ServiceUseError::LoginRequired
            } else {
                ServiceUseError::NeedsRefresh
            });
        }
        gate.check()?;
        let raw = credentials.access_token.expose();
        let proof = verifier
            .verify_refresh(
                raw,
                None,
                subject,
                BrokerRequestContext::new(gate.authority.original().snapshot()),
            )
            .await?;
        gate.check()?;
        if proof.issuer() != issuer || proof.subject() != subject {
            return Err(AuthError::Denied.into());
        }
        if proof.needs_service_refresh(
            verifier,
            BrokerRequestContext::new(gate.authority.original().snapshot()),
            margin,
        )? {
            return Err(if credentials.device {
                ServiceUseError::LoginRequired
            } else {
                ServiceUseError::NeedsRefresh
            });
        }
        let lease = ServiceCredentialLease {
            gate,
            proof: CredentialProof::Native {
                raw,
                proof,
                verifier,
                device: credentials.device,
            },
            revision: record.revision(),
        };
        lease.check()?;
        Ok(lease)
    }
}
impl ServiceCredentialLease<'_> {
    /// Existing issuer margin, reusable by finite temporary credential sessions.
    pub fn refresh_margin(&self) -> Result<Duration, ServiceUseError> {
        self.check()?;
        Ok(self.gate.issuer.policy.services.margin)
    }
    pub fn check(&self) -> Result<(), ServiceUseError> {
        self.gate.check()?;
        let context = BrokerRequestContext::new(self.gate.authority.original().snapshot());
        match &self.proof {
            CredentialProof::StandaloneS3 { expires, .. } => {
                if expires
                    .is_some_and(|(utc, mono)| utc <= SystemTime::now() || mono <= Instant::now())
                {
                    return Err(ServiceUseError::NeedsRefresh);
                }
            }
            CredentialProof::Incoming { caller, verifier } => caller.check(verifier, context)?,
            CredentialProof::Web { caller, verifier } => caller.check(verifier, context)?,
            CredentialProof::Native {
                proof,
                verifier,
                device,
                ..
            } => {
                let margin = if *device {
                    Duration::ZERO
                } else {
                    self.gate.issuer.policy.services.margin
                };
                if proof.needs_service_refresh(verifier, context, margin)? {
                    return Err(if *device {
                        ServiceUseError::LoginRequired
                    } else {
                        ServiceUseError::NeedsRefresh
                    });
                }
            }
            CredentialProof::Key(_) | CredentialProof::Anonymous => (),
        }
        original_check(self.gate.authority.original())
    }
    pub fn original(&self) -> Result<&RequestContext<'_>, ServiceUseError> {
        self.check()?;
        Ok(self.gate.authority.original())
    }
    pub fn binding(&self) -> Result<ServiceBindingView<'_>, ServiceUseError> {
        self.check()?;
        Ok(self.gate.issuer.policy.services.rules[self.gate.rule]
            .binding
            .view(self.revision))
    }
    pub fn credential(&self) -> Result<Option<&str>, ServiceUseError> {
        self.check()?;
        Ok(match &self.proof {
            CredentialProof::StandaloneS3 { .. } => return Err(ServiceUseError::WrongMode),
            CredentialProof::Key(raw) | CredentialProof::Native { raw, .. } => Some(raw),
            CredentialProof::Incoming { caller, .. } => Some(caller.bearer_for_broker()),
            CredentialProof::Web { caller, .. } => Some(caller.bearer_for_broker()),
            CredentialProof::Anonymous => None,
        })
    }
    /// Read only the explicitly captured transport. Identity remains a separate proof.
    pub fn standalone_s3(&self) -> Result<StandaloneS3View<'_>, ServiceUseError> {
        self.check()?;
        let CredentialProof::StandaloneS3 { location, keys, .. } = &self.proof else {
            return Err(ServiceUseError::WrongMode);
        };
        let credential = if let Some(keys) = keys {
            if keys.anonymous {
                return Err(AuthError::Denied.into());
            }
            match location.source {
                S3Source::LocalCredentials => {
                    let s3 = keys.s3.as_ref().ok_or(ServiceUseError::MissingCredential)?;
                    if s3
                        .access_key_id
                        .as_ref()
                        .is_none_or(|key| key.expose().is_empty())
                        || s3
                            .secret_access_key
                            .as_ref()
                            .is_none_or(|key| key.expose().is_empty())
                    {
                        return Err(ServiceUseError::MissingCredential);
                    }
                    S3CredentialView::Local(s3)
                }
                S3Source::AwsChain => S3CredentialView::AwsChain {
                    profile: keys.s3.as_ref().and_then(|s3| s3.aws_profile.as_deref()),
                },
                S3Source::BrokerScope => return Err(ServiceUseError::WrongMode),
            }
        } else {
            S3CredentialView::Anonymous
        };
        Ok(StandaloneS3View {
            location,
            credential,
        })
    }
}
#[cfg(test)]
thread_local! {
    pub(super) static BEFORE_SERVICE_OBSERVATION_LOCK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
impl<'a> LocalServiceLease<'a> {
    #[cfg(test)]
    pub(super) fn test_observation_lock(
        &self,
    ) -> std::sync::MutexGuard<'_, AuthSnapshotRecheck<'a>> {
        self.gate.observation.lock().unwrap()
    }
    pub fn check(&self) -> Result<(), ServiceUseError> {
        self.gate.check()
    }
    pub fn original(&self) -> Result<&RequestContext<'_>, ServiceUseError> {
        self.check()?;
        Ok(self.gate.authority.original())
    }
    pub fn binding(&self) -> Result<ServiceBindingView<'_>, ServiceUseError> {
        self.check()?;
        Ok(self.gate.issuer.policy.services.rules[self.gate.rule]
            .binding
            .view(self.revision))
    }
    pub fn execution(&self) -> Result<LocalExecution, ServiceUseError> {
        self.check()?;
        match self.gate.issuer.policy.services.rules[self.gate.rule].binding {
            Binding::Local { execution, .. } => Ok(execution),
            _ => Err(ServiceUseError::WrongMode),
        }
    }
}

impl super::user_subject::ServiceSubjectProof for ServiceCredentialLease<'_> {
    fn subject_identity(
        &self,
    ) -> Result<Option<super::user_subject::SubjectIdentity<'_>>, ServiceUseError> {
        self.check()?;
        Ok(match &self.proof {
            CredentialProof::Incoming { caller, .. } => {
                Some(super::user_subject::SubjectIdentity {
                    issuer: caller.issuer(),
                    subject: caller.subject(),
                    username: caller.username(),
                    teams: caller.teams(),
                    origin: super::user_subject::VerifiedUserOrigin::Incoming,
                })
            }
            CredentialProof::Web { caller, .. } => Some(super::user_subject::SubjectIdentity {
                issuer: caller.issuer(),
                subject: caller.subject(),
                username: caller.username(),
                teams: caller.teams(),
                origin: super::user_subject::VerifiedUserOrigin::Web,
            }),
            CredentialProof::Native { proof, .. } => Some(super::user_subject::SubjectIdentity {
                issuer: proof.issuer(),
                subject: proof.subject(),
                username: proof.username(),
                teams: proof.teams(),
                origin: super::user_subject::VerifiedUserOrigin::Native,
            }),
            CredentialProof::StandaloneS3 {
                user: Some(user), ..
            } => Some(super::user_subject::SubjectIdentity {
                issuer: user.issuer()?,
                subject: user.subject()?,
                username: user.username()?,
                teams: user.teams()?,
                origin: user.origin()?,
            }),
            CredentialProof::StandaloneS3 { user: None, .. }
            | CredentialProof::Key(_)
            | CredentialProof::Anonymous => None,
        })
    }
}
