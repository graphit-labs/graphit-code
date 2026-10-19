//! Bounded provider/profile data. No document or selector proves authentication.
mod codec;
#[cfg(test)]
mod tests;

use graphit_hub::policy::RequestContext;
use secrecy::{ExposeSecret, SecretString, zeroize::Zeroizing};
use std::{error::Error, fmt, net::IpAddr};
use url::Url;

pub const FORMAT: &str = "rust-auth-v1";
pub const EPOCH: u64 = 1;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileError {
    InvalidPolicy,
    InvalidDocument,
    Limit,
    Cancelled,
    Deadline,
    Conflict,
    Missing,
    Stale,
    NoActive,
}
impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPolicy => "invalid auth profile policy",
            Self::InvalidDocument => "invalid auth profile document",
            Self::Limit => "auth profile limit exceeded",
            Self::Cancelled => "auth profile request cancelled",
            Self::Deadline => "auth profile request deadline exceeded",
            Self::Conflict => "auth profile revision conflict",
            Self::Missing => "auth profile record missing",
            Self::Stale => "auth profile provider changed",
            Self::NoActive => "no active auth profile",
        })
    }
}
impl Error for ProfileError {}
/// Every capacity is explicit and positive; JSON field names count as scalar nodes.
#[derive(Clone, Copy, Debug)]
pub struct ProfileLimits {
    pub bytes: usize,
    pub nodes: usize,
    pub scalar_bytes: usize,
    pub collection_entries: usize,
    pub providers: usize,
    pub profiles: usize,
}
impl ProfileLimits {
    fn validate(self) -> Result<(), ProfileError> {
        if [
            self.bytes,
            self.nodes,
            self.scalar_bytes,
            self.collection_entries,
            self.providers,
            self.profiles,
        ]
        .contains(&0)
            || self.bytes > isize::MAX as usize
            || self.bytes.checked_add(1).is_none()
        {
            Err(ProfileError::InvalidPolicy)
        } else {
            Ok(())
        }
    }
}
/// An explicit secret container, with no automatic cloning, logging or public serde.
/// ```compile_fail
/// use graphit_auth::profile::Secret;
/// fn duplicate(secret: &Secret) { let _: Secret = secret.clone(); }
/// ```
/// ```compile_fail
/// use graphit_auth::profile::Secret;
/// fn log(secret: &Secret) { println!("{secret:?}"); }
/// ```
/// ```compile_fail
/// use graphit_auth::profile::Secret;
/// fn serialize(secret: &Secret) { let _ = serde_json::to_vec(secret); }
/// ```
pub struct Secret(SecretString);
impl Secret {
    pub fn new(value: SecretString) -> Self {
        Self(value)
    }
    /// Deliberate exposure for a future bounded credential consumer; conveys no authority.
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalDevice {
    Auto,
    Cpu,
    Cuda,
    Metal,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalExecution {
    pub device: LocalDevice,
    pub device_id: u32,
}
impl LocalExecution {
    pub fn validate_platform(self, macos: bool) -> Result<(), ProfileError> {
        if (self.device == LocalDevice::Metal && (!macos || self.device_id != 0))
            || (macos && self.device == LocalDevice::Auto && self.device_id != 0)
        {
            Err(ProfileError::InvalidDocument)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectProtocol {
    OpenAi,
    OpenAiCompatible,
    Cohere,
    CohereV2,
    Voyage,
    VoyageV1,
    GoogleGemini1,
    GoogleGemini2,
    Jina,
    JinaV1,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiService {
    Local(LocalExecution),
    Disabled,
    Broker,
    Direct {
        protocol: DirectProtocol,
        endpoint: String,
        model: String,
        model_revision: String,
        recipe: String,
        dimensions: u32,
        request_dimensions: Option<u32>,
        batch_endpoint: Option<String>,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrokerService {
    pub endpoint: String,
    pub mcp_resource: Option<String>,
    pub allow_anonymous: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderKind {
    Local {
        allow_aws_credential_chain: bool,
        broker: Option<BrokerService>,
    },
    Broker(BrokerService),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum S3Source {
    LocalCredentials,
    AwsChain,
    BrokerScope,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct S3Location {
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub prefix: Option<String>,
    pub source: S3Source,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub embedding: AiService,
    pub rerank: AiService,
    pub s3: S3Location,
}
impl ProviderConfig {
    /// Validate service capabilities before publishing a provider configuration.
    /// Broker inference is exclusive; Direct rerank requires a native protocol.
    pub fn validate_ai_services(&self) -> Result<(), ProfileError> {
        if matches!(self.kind, ProviderKind::Broker(_))
            && [&self.embedding, &self.rerank]
                .iter()
                .any(|service| !matches!(service, AiService::Broker | AiService::Disabled))
        {
            return Err(ProfileError::InvalidDocument);
        }
        for (embedding, service) in [(true, &self.embedding), (false, &self.rerank)] {
            if let AiService::Direct {
                protocol,
                endpoint,
                model,
                model_revision,
                recipe,
                dimensions,
                request_dimensions,
                batch_endpoint,
            } = service
            {
                validate_direct_identity(model_revision, recipe)?;
                validate_direct(
                    *protocol,
                    endpoint,
                    model,
                    *dimensions,
                    *request_dimensions,
                    batch_endpoint.as_deref(),
                )?;
                let embed_protocol = matches!(
                    protocol,
                    DirectProtocol::OpenAi
                        | DirectProtocol::OpenAiCompatible
                        | DirectProtocol::Cohere
                        | DirectProtocol::Voyage
                        | DirectProtocol::GoogleGemini1
                        | DirectProtocol::GoogleGemini2
                );
                let native_rerank = matches!(
                    protocol,
                    DirectProtocol::Cohere
                        | DirectProtocol::CohereV2
                        | DirectProtocol::Voyage
                        | DirectProtocol::VoyageV1
                        | DirectProtocol::Jina
                        | DirectProtocol::JinaV1
                );
                if (embedding && (!embed_protocol || *dimensions == 0))
                    || (!embedding && (!native_rerank || *dimensions != 0))
                {
                    return Err(ProfileError::InvalidDocument);
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRecord {
    name: String,
    revision: u64,
    created_at: i64,
    updated_at: i64,
    config: ProviderConfig,
}
impl ProviderRecord {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn created_at(&self) -> i64 {
        self.created_at
    }
    pub fn updated_at(&self) -> i64 {
        self.updated_at
    }
    pub fn config(&self) -> &ProviderConfig {
        &self.config
    }
}
pub struct LocalS3 {
    pub access_key_id: Option<Secret>,
    pub secret_access_key: Option<Secret>,
    pub session_token: Option<Secret>,
    pub expires_at: Option<i64>,
    pub aws_profile: Option<String>,
}
pub struct LocalCredentials {
    /// Explicit local anonymous assertion; never a verified caller or grant.
    pub anonymous: bool,
    pub mcp_key: Option<Secret>,
    pub broker_key: Option<Secret>,
    pub embedding_api_key: Option<Secret>,
    pub rerank_api_key: Option<Secret>,
    pub s3: Option<LocalS3>,
}
pub struct OidcTokens {
    pub access_token: Secret,
    pub refresh_token: Option<Secret>,
    pub id_token: Secret,
    pub expires_at: i64,
}
pub enum ProfileCredentials {
    Local(LocalCredentials),
    Broker {
        issuer: String,
        subject: String,
        tokens: OidcTokens,
        s3_disabled: bool,
    },
}
/// Plain stored data, including untrusted identity assertions. Not a verified identity.
pub struct ProfileData {
    pub name: String,
    pub provider: String,
    pub username: String,
    pub organization: Option<String>,
    pub teams: Vec<String>,
    pub credentials: ProfileCredentials,
}
/// Optional display assertions from a verified Device access JWT, never authority.
#[derive(Default)]
pub struct BrokerDeviceMetadata {
    pub username: Option<String>,
    pub organization: Option<String>,
    pub teams: Option<Vec<String>>,
}
/// Access-only data. A Device grant does not produce an ID or refresh token.
pub struct BrokerDeviceTokens {
    pub access_token: Secret,
    pub expires_at: i64,
}
/// Stored data only; verified identity and permission are separate producers.
pub struct BrokerDeviceProfileData {
    pub name: String,
    pub provider: String,
    pub issuer: String,
    pub subject: String,
    pub tokens: BrokerDeviceTokens,
    pub s3_disabled: bool,
    pub metadata: BrokerDeviceMetadata,
}
enum ProfileBody {
    Named(ProfileData),
    BrokerDevice(BrokerDeviceProfileData),
}
impl ProfileBody {
    fn name(&self) -> &str {
        match self {
            Self::Named(data) => &data.name,
            Self::BrokerDevice(data) => &data.name,
        }
    }
    fn provider(&self) -> &str {
        match self {
            Self::Named(data) => &data.provider,
            Self::BrokerDevice(data) => &data.provider,
        }
    }
}
pub(crate) struct BrokerCredentialsRef<'a> {
    pub(crate) issuer: &'a str,
    pub(crate) subject: &'a str,
    pub(crate) access_token: &'a Secret,
    pub(crate) expires_at: i64,
    pub(crate) s3_disabled: bool,
    pub(crate) device: bool,
}
pub struct ProfileRecord {
    revision: u64,
    token_revision: u64,
    provider_revision: u64,
    created_at: i64,
    updated_at: i64,
    data: ProfileBody,
}
impl ProfileRecord {
    pub fn name(&self) -> &str {
        self.data.name()
    }
    pub fn provider(&self) -> &str {
        self.data.provider()
    }
    pub fn data(&self) -> Option<&ProfileData> {
        match &self.data {
            ProfileBody::Named(data) => Some(data),
            ProfileBody::BrokerDevice(_) => None,
        }
    }
    pub fn broker_device_data(&self) -> Option<&BrokerDeviceProfileData> {
        match &self.data {
            ProfileBody::BrokerDevice(data) => Some(data),
            ProfileBody::Named(_) => None,
        }
    }
    pub(crate) fn broker_credentials(&self) -> Option<BrokerCredentialsRef<'_>> {
        if let Some(data) = self.broker_device_data() {
            return Some(BrokerCredentialsRef {
                issuer: &data.issuer,
                subject: &data.subject,
                access_token: &data.tokens.access_token,
                expires_at: data.tokens.expires_at,
                s3_disabled: data.s3_disabled,
                device: true,
            });
        }
        match &self.data()?.credentials {
            ProfileCredentials::Broker {
                issuer,
                subject,
                tokens,
                s3_disabled,
            } => Some(BrokerCredentialsRef {
                issuer,
                subject,
                access_token: &tokens.access_token,
                expires_at: tokens.expires_at,
                s3_disabled: *s3_disabled,
                device: false,
            }),
            ProfileCredentials::Local(_) => None,
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn token_revision(&self) -> u64 {
        self.token_revision
    }
    pub fn provider_revision(&self) -> u64 {
        self.provider_revision
    }
    pub fn created_at(&self) -> i64 {
        self.created_at
    }
    pub fn updated_at(&self) -> i64 {
        self.updated_at
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ProfileExpectation {
    pub document_revision: u64,
    pub provider_revision: u64,
    pub profile_revision: Option<u64>,
}
#[derive(Clone, Copy, Debug)]
pub struct TokenExpectation {
    pub document_revision: u64,
    pub provider_revision: u64,
    pub profile_revision: u64,
    pub token_revision: u64,
}
/// Profile assertions supplied by the authentication consumer, never a proof.
pub struct ProfileClaimsUpdate {
    pub username: String,
    pub organization: Option<String>,
    pub teams: Vec<String>,
}
/// Keeps a refreshed ID token and its profile assertions inseparable.
/// Authentication and issuer/subject verification belong to the consumer.
///
/// ```compile_fail
/// use graphit_auth::profile::ProfileIdentityUpdate;
/// fn duplicate<T: Clone>() {} duplicate::<ProfileIdentityUpdate>();
/// ```
/// ```compile_fail
/// use graphit_auth::profile::ProfileIdentityUpdate;
/// fn disclose<T: std::fmt::Debug>() {} disclose::<ProfileIdentityUpdate>();
/// ```
/// ```compile_fail
/// use graphit_auth::profile::ProfileIdentityUpdate;
/// fn serialize<T: serde::Serialize>() {} serialize::<ProfileIdentityUpdate>();
/// ```
pub enum ProfileIdentityUpdate {
    Preserve,
    Replace {
        id_token: Secret,
        claims: ProfileClaimsUpdate,
    },
}
pub struct TokenUpdate {
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub identity: ProfileIdentityUpdate,
    pub expires_at: i64,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum WebHubScope {
    Project,
    Global,
}
pub(crate) struct WebHubAdministrator {
    pub(crate) provider: String,
    pub(crate) issuer: String,
    pub(crate) subject: String,
    pub(crate) scopes: Vec<WebHubScope>,
}
/// A fresh-format data document. Physical private storage and authentication are separate.
pub struct AuthDocument {
    limits: ProfileLimits,
    revision: u64,
    active_profile: Option<String>,
    providers: Vec<ProviderRecord>,
    profiles: Vec<ProfileRecord>,
    web_hub_administrators: Vec<WebHubAdministrator>,
}
impl AuthDocument {
    pub fn new(limits: ProfileLimits, context: &RequestContext<'_>) -> Result<Self, ProfileError> {
        codec::check(limits, context)?;
        Ok(Self {
            limits,
            revision: 1,
            active_profile: None,
            providers: Vec::new(),
            profiles: Vec::new(),
            web_hub_administrators: Vec::new(),
        })
    }
    pub fn decode(
        bytes: &[u8],
        limits: ProfileLimits,
        context: &RequestContext<'_>,
    ) -> Result<Self, ProfileError> {
        codec::decode(bytes, limits, context)
    }
    pub fn encode(&self, context: &RequestContext<'_>) -> Result<Zeroizing<Vec<u8>>, ProfileError> {
        codec::encode(self, context)
    }
    pub(crate) fn web_hub_administrators(&self) -> &[WebHubAdministrator] {
        &self.web_hub_administrators
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn active_name(&self) -> Option<&str> {
        self.active_profile.as_deref()
    }
    pub fn providers(&self) -> &[ProviderRecord] {
        &self.providers
    }
    pub fn profiles(&self) -> &[ProfileRecord] {
        &self.profiles
    }
    pub fn provider(&self, name: &str) -> Option<&ProviderRecord> {
        self.provider_index(name).ok().map(|i| &self.providers[i])
    }
    pub fn profile(&self, name: &str) -> Option<&ProfileRecord> {
        self.profile_index(name).ok().map(|i| &self.profiles[i])
    }
    fn provider_index(&self, name: &str) -> Result<usize, usize> {
        self.providers
            .binary_search_by(|p| p.name.as_str().cmp(name))
    }
    fn profile_index(&self, name: &str) -> Result<usize, usize> {
        self.profiles.binary_search_by(|p| p.name().cmp(name))
    }
    fn prepare(&self, expected: u64, context: &RequestContext<'_>) -> Result<u64, ProfileError> {
        codec::check(self.limits, context)?;
        if expected != self.revision {
            return Err(ProfileError::Conflict);
        }
        self.revision.checked_add(1).ok_or(ProfileError::Limit)
    }
    pub fn add_provider(
        &mut self,
        name: String,
        config: ProviderConfig,
        expected: u64,
        now: i64,
        context: &RequestContext<'_>,
    ) -> Result<(), ProfileError> {
        let revision = self.prepare(expected, context)?;
        valid_time(now)?;
        codec::validate_provider_input(&name, &config, self.limits, context)?;
        let i = self
            .provider_index(&name)
            .err()
            .ok_or(ProfileError::Conflict)?;
        let record = ProviderRecord {
            name,
            revision,
            created_at: now,
            updated_at: now,
            config,
        };
        codec::validate_change(self, codec::Change::Provider(&record), context)?;
        reserve(
            &mut self.providers,
            self.limits.providers.min(self.limits.collection_entries),
        )?;
        codec::check(self.limits, context)?;
        self.providers.insert(i, record);
        self.revision = revision;
        Ok(())
    }
    pub fn update_provider(
        &mut self,
        name: &str,
        config: ProviderConfig,
        expected_document: u64,
        expected_provider: u64,
        now: i64,
        context: &RequestContext<'_>,
    ) -> Result<(), ProfileError> {
        let revision = self.prepare(expected_document, context)?;
        let i = self
            .provider_index(name)
            .map_err(|_| ProfileError::Missing)?;
        let old = &self.providers[i];
        if old.revision != expected_provider {
            return Err(ProfileError::Conflict);
        }
        let provider_revision = old.revision.checked_add(1).ok_or(ProfileError::Limit)?;
        next_time(now, old.updated_at)?;
        codec::validate_provider_input(name, &config, self.limits, context)?;
        let record = ProviderRecord {
            name: old.name.clone(),
            revision: provider_revision,
            created_at: old.created_at,
            updated_at: now,
            config,
        };
        codec::validate_change(self, codec::Change::Provider(&record), context)?;
        codec::check(self.limits, context)?;
        self.providers[i] = record;
        self.revision = revision;
        Ok(())
    }
    pub fn remove_provider(
        &mut self,
        name: &str,
        cascade: bool,
        expected: u64,
        context: &RequestContext<'_>,
    ) -> Result<(), ProfileError> {
        let revision = self.prepare(expected, context)?;
        let i = self
            .provider_index(name)
            .map_err(|_| ProfileError::Missing)?;
        if !cascade && self.profiles.iter().any(|p| p.provider() == name) {
            return Err(ProfileError::Conflict);
        }
        codec::validate_change(self, codec::Change::RemoveProvider(name, cascade), context)?;
        codec::check(self.limits, context)?;
        if cascade {
            if self
                .active_profile
                .as_deref()
                .and_then(|n| self.profile(n))
                .is_some_and(|p| p.provider() == name)
            {
                self.active_profile = None;
            }
            self.profiles.retain(|p| p.provider() != name);
        }
        self.providers.remove(i);
        self.revision = revision;
        Ok(())
    }
    /// Materializes only a provider. Never creates a profile, key, session or login.
    pub fn ensure_default_local(
        &mut self,
        expected: u64,
        now: i64,
        context: &RequestContext<'_>,
    ) -> Result<bool, ProfileError> {
        codec::check(self.limits, context)?;
        if expected != self.revision {
            return Err(ProfileError::Conflict);
        }
        if let Some(p) = self.provider("local") {
            return match p.config.kind {
                ProviderKind::Local { .. } => Ok(false),
                ProviderKind::Broker(_) => Err(ProfileError::Conflict),
            };
        }
        let execution = LocalExecution {
            device: LocalDevice::Cpu,
            device_id: 0,
        };
        self.add_provider(
            "local".into(),
            ProviderConfig {
                kind: ProviderKind::Local {
                    allow_aws_credential_chain: false,
                    broker: None,
                },
                embedding: AiService::Local(execution),
                rerank: AiService::Local(execution),
                s3: S3Location {
                    bucket: None,
                    region: None,
                    endpoint: None,
                    prefix: None,
                    source: S3Source::LocalCredentials,
                },
            },
            expected,
            now,
            context,
        )?;
        Ok(true)
    }
    /// Replace validated data by CAS. Authentication must occur in a separate producer.
    pub fn replace_profile_data(
        &mut self,
        data: ProfileData,
        expected: ProfileExpectation,
        now: i64,
        context: &RequestContext<'_>,
    ) -> Result<(), ProfileError> {
        self.replace_profile_body(ProfileBody::Named(data), expected, now, context)
    }
    /// Publish only caller-supplied Device data; this method does not authenticate it.
    pub fn replace_broker_device_profile(
        &mut self,
        data: BrokerDeviceProfileData,
        expected: ProfileExpectation,
        now: i64,
        context: &RequestContext<'_>,
    ) -> Result<(), ProfileError> {
        self.replace_profile_body(ProfileBody::BrokerDevice(data), expected, now, context)
    }
    fn replace_profile_body(
        &mut self,
        data: ProfileBody,
        expected: ProfileExpectation,
        now: i64,
        context: &RequestContext<'_>,
    ) -> Result<(), ProfileError> {
        let document_revision = self.prepare(expected.document_revision, context)?;
        let provider = self
            .provider(data.provider())
            .ok_or(ProfileError::Missing)?;
        if provider.revision != expected.provider_revision {
            return Err(ProfileError::Stale);
        }
        codec::validate_profile_body_input(&data, &provider.config, self.limits, context)?;
        let (index, revision, token_revision, created_at) =
            match (self.profile_index(data.name()), expected.profile_revision) {
                (Ok(i), Some(r)) if self.profiles[i].revision == r => {
                    let old = &self.profiles[i];
                    next_time(now, old.updated_at)?;
                    (
                        i,
                        old.revision.checked_add(1).ok_or(ProfileError::Limit)?,
                        old.token_revision
                            .checked_add(1)
                            .ok_or(ProfileError::Limit)?,
                        old.created_at,
                    )
                }
                (Err(i), None) => {
                    valid_time(now)?;
                    (i, document_revision, document_revision, now)
                }
                _ => return Err(ProfileError::Conflict),
            };
        let replace = expected.profile_revision.is_some();
        let record = ProfileRecord {
            revision,
            token_revision,
            provider_revision: expected.provider_revision,
            created_at,
            updated_at: now,
            data,
        };
        codec::validate_change(self, codec::Change::Profile(&record), context)?;
        if !replace {
            reserve(
                &mut self.profiles,
                self.limits.profiles.min(self.limits.collection_entries),
            )?;
        }
        codec::check(self.limits, context)?;
        if replace {
            self.profiles[index] = record;
        } else {
            self.profiles.insert(index, record);
        }
        self.revision = document_revision;
        Ok(())
    }
    /// Select current stored data. The result is never an authenticated caller or role.
    pub fn active(
        &self,
        context: &RequestContext<'_>,
    ) -> Result<(&ProviderRecord, &ProfileRecord), ProfileError> {
        codec::check(self.limits, context)?;
        let name = self
            .active_profile
            .as_deref()
            .ok_or(ProfileError::NoActive)?;
        let profile = self.profile(name).ok_or(ProfileError::Missing)?;
        let provider = self
            .provider(profile.provider())
            .ok_or(ProfileError::Missing)?;
        if provider.revision != profile.provider_revision {
            return Err(ProfileError::Stale);
        }
        codec::check(self.limits, context)?;
        Ok((provider, profile))
    }
    pub fn activate(
        &mut self,
        name: &str,
        expected: u64,
        context: &RequestContext<'_>,
    ) -> Result<(), ProfileError> {
        let revision = self.prepare(expected, context)?;
        let profile = self.profile(name).ok_or(ProfileError::Missing)?;
        let provider = self
            .provider(profile.provider())
            .ok_or(ProfileError::Missing)?;
        if provider.revision != profile.provider_revision {
            return Err(ProfileError::Stale);
        }
        let selected_revision = profile
            .revision
            .checked_add(u64::from(self.active_profile.as_deref() != Some(name)))
            .ok_or(ProfileError::Limit)?;
        codec::validate_change(self, codec::Change::Active(name), context)?;
        let name = name.to_owned();
        codec::check(self.limits, context)?;
        let index = self
            .profile_index(&name)
            .map_err(|_| ProfileError::Missing)?;
        self.profiles[index].revision = selected_revision;
        self.active_profile = Some(name);
        self.revision = revision;
        Ok(())
    }
    pub fn logout(
        &mut self,
        name: Option<&str>,
        expected: u64,
        context: &RequestContext<'_>,
    ) -> Result<(), ProfileError> {
        let revision = self.prepare(expected, context)?;
        let name = name
            .or(self.active_profile.as_deref())
            .ok_or(ProfileError::NoActive)?;
        let i = self
            .profile_index(name)
            .map_err(|_| ProfileError::Missing)?;
        let active = self.active_profile.as_deref() == Some(name);
        codec::validate_change(self, codec::Change::Logout(name), context)?;
        codec::check(self.limits, context)?;
        self.profiles.remove(i);
        if active {
            self.active_profile = None;
        }
        self.revision = revision;
        Ok(())
    }
    pub fn rotate_tokens(
        &mut self,
        name: &str,
        update: TokenUpdate,
        expected: TokenExpectation,
        now: i64,
        context: &RequestContext<'_>,
    ) -> Result<(), ProfileError> {
        let revision = self.prepare(expected.document_revision, context)?;
        if self.active_profile.as_deref() != Some(name) {
            return Err(ProfileError::Conflict);
        }
        let i = self
            .profile_index(name)
            .map_err(|_| ProfileError::Missing)?;
        let p = &self.profiles[i];
        let provider = self.provider(p.provider()).ok_or(ProfileError::Missing)?;
        if provider.revision != expected.provider_revision
            || p.provider_revision != expected.provider_revision
        {
            return Err(ProfileError::Stale);
        }
        if p.revision != expected.profile_revision || p.token_revision != expected.token_revision {
            return Err(ProfileError::Conflict);
        }
        if !p
            .data()
            .is_some_and(|data| matches!(data.credentials, ProfileCredentials::Broker { .. }))
        {
            return Err(ProfileError::InvalidDocument);
        }
        let profile_revision = p.revision.checked_add(1).ok_or(ProfileError::Limit)?;
        let token_revision = p.token_revision.checked_add(1).ok_or(ProfileError::Limit)?;
        next_time(now, p.updated_at)?;
        codec::validate_token_update(&update, self.limits, context)?;
        codec::validate_change(self, codec::Change::Tokens(name, &update, now), context)?;
        codec::check(self.limits, context)?;
        let profile = &mut self.profiles[i];
        let ProfileBody::Named(data) = &mut profile.data else {
            return Err(ProfileError::InvalidDocument);
        };
        if let ProfileCredentials::Broker { tokens, .. } = &mut data.credentials {
            tokens.access_token = update.access_token;
            tokens.refresh_token = Some(update.refresh_token);
            if let ProfileIdentityUpdate::Replace { id_token, claims } = update.identity {
                tokens.id_token = id_token;
                data.username = claims.username;
                data.organization = claims.organization;
                data.teams = claims.teams;
            }
            tokens.expires_at = update.expires_at;
        }
        profile.revision = profile_revision;
        profile.token_revision = token_revision;
        profile.updated_at = now;
        self.revision = revision;
        Ok(())
    }
}
fn reserve<T>(values: &mut Vec<T>, cap: usize) -> Result<(), ProfileError> {
    if values.len() >= cap {
        return Err(ProfileError::Limit);
    }
    if values.len() == values.capacity() {
        values
            .try_reserve_exact(1)
            .map_err(|_| ProfileError::Limit)?;
    }
    Ok(())
}
fn valid_time(value: i64) -> Result<(), ProfileError> {
    if value < 0 {
        Err(ProfileError::InvalidDocument)
    } else {
        Ok(())
    }
}
fn next_time(value: i64, previous: i64) -> Result<(), ProfileError> {
    valid_time(value)?;
    if value < previous {
        Err(ProfileError::Conflict)
    } else {
        Ok(())
    }
}
pub(crate) fn valid_name(value: &str) -> Result<(), ProfileError> {
    if value.is_empty()
        || value.trim() != value
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
    {
        Err(ProfileError::InvalidDocument)
    } else {
        Ok(())
    }
}
fn valid_text(value: &str) -> Result<(), ProfileError> {
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        Err(ProfileError::InvalidDocument)
    } else {
        Ok(())
    }
}
fn valid_url(value: &str, resource: bool) -> Result<(), ProfileError> {
    valid_text(value)?;
    let u = Url::parse(value).map_err(|_| ProfileError::InvalidDocument)?;
    if u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || !matches!(u.scheme(), "http" | "https")
        || u.fragment().is_some()
        || (resource && u.query().is_some())
    {
        return Err(ProfileError::InvalidDocument);
    }
    if u.scheme() == "http"
        && !matches!(u.host(), Some(url::Host::Domain("localhost")))
        && !u.host_str().is_some_and(|h| {
            h.trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        })
    {
        return Err(ProfileError::InvalidDocument);
    }
    Ok(())
}
pub(crate) fn validate_direct_identity(
    model_revision: &str,
    recipe: &str,
) -> Result<(), ProfileError> {
    valid_text(model_revision)?;
    valid_text(recipe)
}
pub(crate) fn validate_direct(
    protocol: DirectProtocol,
    endpoint: &str,
    model: &str,
    dimensions: u32,
    request_dimensions: Option<u32>,
    batch_endpoint: Option<&str>,
) -> Result<(), ProfileError> {
    valid_url(endpoint, false)?;
    valid_text(model)?;
    if let Some(width) = request_dimensions {
        let native = match (protocol, model) {
            (DirectProtocol::OpenAi, "text-embedding-3-small") => 1536,
            (DirectProtocol::OpenAi, "text-embedding-3-large") => 3072,
            _ => return Err(ProfileError::InvalidDocument),
        };
        if width == 0 || width >= native || width != dimensions {
            return Err(ProfileError::InvalidDocument);
        }
    }
    if matches!(
        protocol,
        DirectProtocol::GoogleGemini1 | DirectProtocol::GoogleGemini2
    ) {
        let batch = batch_endpoint.ok_or(ProfileError::InvalidDocument)?;
        valid_url(endpoint, true)?;
        valid_url(batch, true)?;
        let name = model
            .strip_prefix("models/")
            .ok_or(ProfileError::InvalidDocument)?;
        if dimensions == 0
            || name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(ProfileError::InvalidDocument);
        }
        let single = Url::parse(endpoint).map_err(|_| ProfileError::InvalidDocument)?;
        let batch = Url::parse(batch).map_err(|_| ProfileError::InvalidDocument)?;
        let coherent = |url: &Url, operation: &str| {
            url.path()
                .strip_suffix(operation)
                .and_then(|p| p.strip_suffix(model))
                .is_some_and(|p| p.ends_with('/'))
        };
        if single.origin() != batch.origin()
            || !coherent(&single, ":embedContent")
            || !coherent(&batch, ":batchEmbedContents")
        {
            return Err(ProfileError::InvalidDocument);
        }
    } else if batch_endpoint.is_some() {
        return Err(ProfileError::InvalidDocument);
    }
    Ok(())
}
fn validate_config(config: &ProviderConfig) -> Result<(), ProfileError> {
    config.validate_ai_services()?;
    let broker = match &config.kind {
        ProviderKind::Local { broker, .. } => broker.as_ref(),
        ProviderKind::Broker(b) => Some(b),
    };
    if let Some(b) = broker {
        valid_url(&b.endpoint, false)?;
        if let Some(resource) = &b.mcp_resource {
            valid_url(resource, true)?;
        }
    }
    match &config.kind {
        ProviderKind::Local {
            allow_aws_credential_chain,
            broker,
        } => {
            if config.s3.source == S3Source::BrokerScope
                || (config.s3.source == S3Source::AwsChain && !allow_aws_credential_chain)
                || broker.as_ref().is_some_and(|b| b.mcp_resource.is_some())
            {
                return Err(ProfileError::InvalidDocument);
            }
        }
        ProviderKind::Broker(b) => {
            if b.allow_anonymous || config.s3.source != S3Source::BrokerScope {
                return Err(ProfileError::InvalidDocument);
            }
        }
    }
    if (matches!(config.embedding, AiService::Broker) || matches!(config.rerank, AiService::Broker))
        && broker.is_none()
    {
        return Err(ProfileError::InvalidDocument);
    }
    for service in [&config.embedding, &config.rerank] {
        if let AiService::Local(execution) = service {
            execution.validate_platform(cfg!(target_os = "macos"))?;
        }
    }
    for v in [&config.s3.bucket, &config.s3.region, &config.s3.prefix]
        .into_iter()
        .flatten()
    {
        valid_text(v)?;
    }
    if let Some(e) = &config.s3.endpoint {
        valid_url(e, false)?;
    }
    Ok(())
}
fn validate_data(data: &ProfileData, config: Option<&ProviderConfig>) -> Result<(), ProfileError> {
    valid_name(&data.name)?;
    valid_name(&data.provider)?;
    valid_profile_claims(&data.username, data.organization.as_deref(), &data.teams)?;
    match &data.credentials {
        ProfileCredentials::Local(c) => {
            if c.anonymous
                && (data.username != "anonymous"
                    || data.organization.is_some()
                    || !data.teams.is_empty()
                    || c.broker_key.is_some())
            {
                return Err(ProfileError::InvalidDocument);
            }
            for secret in [
                &c.mcp_key,
                &c.broker_key,
                &c.embedding_api_key,
                &c.rerank_api_key,
            ]
            .into_iter()
            .flatten()
            {
                valid_text(secret.expose())?;
            }
            if let Some(s3) = &c.s3 {
                if s3.access_key_id.is_some() != s3.secret_access_key.is_some()
                    || (s3.session_token.is_some() && s3.access_key_id.is_none())
                {
                    return Err(ProfileError::InvalidDocument);
                }
                for secret in [&s3.access_key_id, &s3.secret_access_key, &s3.session_token]
                    .into_iter()
                    .flatten()
                {
                    valid_text(secret.expose())?;
                }
                if let Some(t) = s3.expires_at {
                    valid_time(t)?;
                }
                if let Some(v) = &s3.aws_profile {
                    valid_text(v)?;
                }
            }
            if let Some(config) = config {
                let ProviderKind::Local {
                    allow_aws_credential_chain,
                    broker,
                } = &config.kind
                else {
                    return Err(ProfileError::InvalidDocument);
                };
                if c.anonymous && !broker.as_ref().is_some_and(|b| b.allow_anonymous) {
                    return Err(ProfileError::InvalidDocument);
                }
                if (matches!(config.embedding, AiService::Direct { .. })
                    && c.embedding_api_key.is_none())
                    || (matches!(config.rerank, AiService::Direct { .. })
                        && c.rerank_api_key.is_none())
                {
                    return Err(ProfileError::InvalidDocument);
                }
                if broker.is_some()
                    && c.broker_key.is_none()
                    && !broker.as_ref().is_some_and(|b| b.allow_anonymous)
                {
                    return Err(ProfileError::InvalidDocument);
                }
                if config.s3.bucket.is_some()
                    && !allow_aws_credential_chain
                    && !c
                        .s3
                        .as_ref()
                        .is_some_and(|s| s.access_key_id.is_some() || s.aws_profile.is_some())
                {
                    return Err(ProfileError::InvalidDocument);
                }
            }
        }
        ProfileCredentials::Broker {
            issuer,
            subject,
            tokens,
            ..
        } => {
            valid_url(issuer, false)?;
            valid_text(subject)?;
            valid_text(tokens.access_token.expose())?;
            valid_text(tokens.id_token.expose())?;
            if let Some(t) = &tokens.refresh_token {
                valid_text(t.expose())?;
            }
            valid_time(tokens.expires_at)?;
            if config.is_some_and(|c| !matches!(c.kind, ProviderKind::Broker(_))) {
                return Err(ProfileError::InvalidDocument);
            }
        }
    }
    Ok(())
}

fn validate_device_data(
    data: &BrokerDeviceProfileData,
    config: Option<&ProviderConfig>,
) -> Result<(), ProfileError> {
    valid_name(&data.name)?;
    valid_name(&data.provider)?;
    valid_url(&data.issuer, false)?;
    valid_text(&data.subject)?;
    valid_text(data.tokens.access_token.expose())?;
    valid_time(data.tokens.expires_at)?;
    if let Some(username) = &data.metadata.username {
        valid_text(username)?;
    }
    if let Some(organization) = &data.metadata.organization {
        valid_text(organization)?;
    }
    if let Some(teams) = &data.metadata.teams {
        for team in teams {
            valid_text(team)?;
        }
    }
    if let Some(config) = config {
        match &config.kind {
            ProviderKind::Broker(broker) if broker.mcp_resource.is_none() => {}
            _ => return Err(ProfileError::InvalidDocument),
        }
    }
    Ok(())
}

fn valid_profile_claims(
    username: &str,
    organization: Option<&str>,
    teams: &[String],
) -> Result<(), ProfileError> {
    valid_text(username)?;
    if let Some(organization) = organization {
        valid_text(organization)?;
    }
    for team in teams {
        valid_text(team)?;
    }
    Ok(())
}
