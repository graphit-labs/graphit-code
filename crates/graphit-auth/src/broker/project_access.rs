//! Fresh PROJECT authorization, separate from locally verified Broker identity.
use super::{
    AuthenticatedBrokerCaller, BrokerError, BrokerRequestContext, BrokerVerifier, JsonLimits, json,
    oauth::web::AuthenticatedWebCaller,
};
use crate::{
    AuthError, ProjectCapture, ProjectReadModule, ProjectWriteOperation, ScopeCapture,
    profile::ProviderKind,
};
use graphit_core::{artifact_store::StoreOwner, project_identity::ProjectId};
use graphit_http::client::{
    BoundedClient, ClientPolicy, Limits, NetworkError, NetworkRequest, NetworkRequestContext,
};
use graphit_hub::policy::RequestContext;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use secrecy::zeroize::Zeroizing;
use serde::{Deserialize, Serialize};

/// The endpoint is configured independently of the OAuth issuer and resource.
pub struct ProjectAccessPolicy<'a> {
    pub endpoint: &'a str,
    pub network: ClientPolicy<'a>,
}

pub struct BrokerProjectAccess {
    endpoint: String,
    client: BoundedClient,
    json: JsonLimits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrokerProjectError {
    Authentication(BrokerError),
    Authority(AuthError),
    Store(crate::store::AuthStoreError),
    Network(NetworkError),
    Denied,
    InvalidResponse,
    Http(StatusCode),
}
impl std::fmt::Display for BrokerProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authentication(e) => e.fmt(f),
            Self::Authority(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
            Self::Network(e) => e.fmt(f),
            Self::Denied => f.write_str("Broker PROJECT access denied"),
            Self::InvalidResponse => f.write_str("invalid Broker PROJECT response"),
            Self::Http(status) => write!(f, "Broker PROJECT HTTP status {status}"),
        }
    }
}
impl std::error::Error for BrokerProjectError {}
impl From<BrokerError> for BrokerProjectError {
    fn from(error: BrokerError) -> Self {
        Self::Authentication(error)
    }
}
impl From<AuthError> for BrokerProjectError {
    fn from(error: AuthError) -> Self {
        Self::Authority(error)
    }
}
impl From<crate::store::AuthStoreError> for BrokerProjectError {
    fn from(error: crate::store::AuthStoreError) -> Self {
        Self::Store(error)
    }
}
impl From<NetworkError> for BrokerProjectError {
    fn from(error: NetworkError) -> Self {
        Self::Network(error)
    }
}

/// Borrowed SDK proofs; identity data cannot construct either variant's proof.
#[derive(Clone, Copy)]
pub enum BrokerProjectCaller<'a> {
    Web(&'a AuthenticatedWebCaller<'a>),
    Incoming(&'a AuthenticatedBrokerCaller<'a>),
}
impl<'a> BrokerProjectCaller<'a> {
    pub(crate) fn check(
        self,
        verifier: &BrokerVerifier,
        original: &RequestContext<'_>,
    ) -> Result<(), BrokerError> {
        let context = BrokerRequestContext::new(original.snapshot());
        match self {
            Self::Web(caller) => caller.check(verifier, context),
            Self::Incoming(caller) => caller.check(verifier, context),
        }
    }
    fn bearer(self) -> &'a str {
        match self {
            Self::Web(caller) => caller.bearer_for_broker(),
            Self::Incoming(caller) => caller.bearer_for_broker(),
        }
    }
    fn issuer(self) -> &'a str {
        match self {
            Self::Web(caller) => caller.issuer(),
            Self::Incoming(caller) => caller.issuer(),
        }
    }
    fn subject(self) -> &'a str {
        match self {
            Self::Web(caller) => caller.subject(),
            Self::Incoming(caller) => caller.subject(),
        }
    }
    pub(crate) fn matches_provider(
        self,
        verifier: &BrokerVerifier,
        provider: &crate::profile::ProviderRecord,
    ) -> bool {
        let ProviderKind::Broker(service) = &provider.config().kind else {
            return false;
        };
        let revision = match self {
            Self::Web(caller) => caller.provider_revision(),
            Self::Incoming(caller) => caller.provider_revision(),
        };
        provider.revision() == revision
            && verifier.endpoint == service.endpoint
            && verifier.revision == revision
            && match self {
                Self::Web(_) => true, // The registered Web client carries its own resource.
                Self::Incoming(_) => verifier.resource == service.mcp_resource,
            }
    }
}

/// Actual physical captures; no agent binding or active CLI profile is required.
#[derive(Clone, Copy)]
pub struct BrokerProjectScope<'a> {
    pub installation: ScopeCapture<'a>,
    pub project: ProjectCapture<'a>,
    pub providers: &'a [ScopeCapture<'a>],
}
impl BrokerProjectScope<'_> {
    pub(crate) fn physical(&self) -> crate::scope::PhysicalScopeInput<'_> {
        crate::scope::PhysicalScopeInput {
            installation: self.installation,
            project: Some(self.project),
            providers: self.providers,
        }
    }
    pub(crate) fn project_id(&self) -> Result<ProjectId, AuthError> {
        match self.project.scope.live.owner() {
            StoreOwner::Project { project_id } => Ok(*project_id),
            _ => Err(AuthError::Denied),
        }
    }
}

pub struct BrokerRecordRead<'a> {
    pub caller: BrokerProjectCaller<'a>,
    pub verifier: &'a BrokerVerifier,
    pub backend: &'a BrokerProjectAccess,
    pub original: &'a RequestContext<'a>,
    pub scope: BrokerProjectScope<'a>,
    pub provider: &'a str,
    pub module: ProjectReadModule,
}

/// Memory mutation from the actual registered Web principal, never actor input.
pub struct BrokerRecordWrite<'a> {
    pub caller: &'a AuthenticatedWebCaller<'a>,
    pub verifier: &'a BrokerVerifier,
    pub backend: &'a BrokerProjectAccess,
    pub original: &'a RequestContext<'a>,
    pub scope: BrokerProjectScope<'a>,
    pub provider: &'a str,
    pub operation: ProjectWriteOperation,
}

#[derive(Clone, Copy)]
pub(crate) enum ProjectAccessAction {
    Read,
    Write,
}
impl ProjectAccessAction {
    fn wire(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

#[derive(Serialize)]
struct AccessRequest<'a> {
    project_id: ProjectId,
    module: &'a str,
    action: &'static str,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Subject {
    issuer: String,
    subject: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessResponse {
    subject: Subject,
    authorization_revision: i64,
    project_id: ProjectId,
    module: String,
    action: String,
}
fn module_name(module: ProjectReadModule) -> &'static str {
    match module {
        ProjectReadModule::Task => "task",
        ProjectReadModule::Memory => "memory",
        ProjectReadModule::Knowledge => "knowledge",
        ProjectReadModule::Ast => "ast",
        ProjectReadModule::Hub => "hub",
    }
}
impl BrokerProjectAccess {
    pub fn new(
        policy: ProjectAccessPolicy<'_>,
        limits: Limits,
        json: JsonLimits,
    ) -> Result<Self, BrokerProjectError> {
        json.validate()?;
        let endpoint = super::checked_url(policy.endpoint, limits.url_bytes)?;
        let origin = super::checked_url(policy.network.origin, limits.url_bytes)?;
        if endpoint.origin() != origin.origin()
            || endpoint.path() != "/v1/project/access/authorize"
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(BrokerError::InvalidPolicy.into());
        }
        Ok(Self {
            endpoint: policy.endpoint.into(),
            client: BoundedClient::new(policy.network, limits)?,
            json,
        })
    }

    pub(crate) async fn authorize(
        &self,
        caller: BrokerProjectCaller<'_>,
        verifier: &BrokerVerifier,
        project_id: ProjectId,
        module: ProjectReadModule,
        action: ProjectAccessAction,
        original: &RequestContext<'_>,
    ) -> Result<(), BrokerProjectError> {
        caller.check(verifier, original)?;
        let body = Zeroizing::new(
            serde_json::to_vec(&AccessRequest {
                project_id,
                module: module_name(module),
                action: action.wire(),
            })
            .map_err(|_| BrokerProjectError::InvalidResponse)?,
        );
        let authorization_text = Zeroizing::new(format!("Bearer {}", caller.bearer()));
        let mut authorization =
            HeaderValue::from_str(&authorization_text).map_err(|_| BrokerError::InvalidToken)?;
        authorization.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, authorization);
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let response = self
            .client
            .execute(
                NetworkRequest {
                    method: Method::POST,
                    url: &self.endpoint,
                    headers: &headers,
                    body: &body,
                },
                NetworkRequestContext::with_deadline(original.deadline(), original.cancellation()),
            )
            .await?;
        caller.check(verifier, original)?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::FORBIDDEN => return Err(BrokerProjectError::Denied),
            status => return Err(BrokerProjectError::Http(status)),
        }
        let parsed = json::parse(
            response.body(),
            self.json,
            BrokerRequestContext::new(original.snapshot()),
        )?;
        if !parsed.is_object()
            || !parsed
                .get("subject")
                .is_some_and(|subject| subject.is_object())
        {
            return Err(BrokerProjectError::InvalidResponse);
        }
        let reply: AccessResponse =
            serde_json::from_value(parsed).map_err(|_| BrokerProjectError::InvalidResponse)?;
        if reply.subject.issuer != caller.issuer()
            || reply.subject.subject != caller.subject()
            || reply.project_id != project_id
            || reply.module != module_name(module)
            || reply.action != action.wire()
        {
            return Err(BrokerProjectError::InvalidResponse);
        }
        // This is the current SQL observation, not a permission lease.
        let _ = reply.authorization_revision;
        caller.check(verifier, original)?;
        Ok(())
    }
}
