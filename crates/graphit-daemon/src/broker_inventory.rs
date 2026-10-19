//! Read and publish the captured installation's historical inventory with real Auth and HTTP gates.
//! This SDK does not grant Broker access or describe live presence.
#![cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]

use crate::{
    auth_startup::DaemonAuth,
    broker_verifiers::{BrokerVerifierCache, BrokerVerifierCacheError},
    maintained_inventory::{MaintainedProjectVersions, read_maintained_inventory},
};
use graphit_auth::{
    AuthenticatedCaller, ScopeCapture, ServiceBindingView, ServiceCredentialLease, ServicePurpose,
    ServiceUseError,
};
use graphit_core::{
    artifact_store::{ScopeContext, StoreOwner},
    installation_identity::InstallationId,
    project_identity::ProjectId,
};
use graphit_http::client::{
    BoundedClient, GuardedNetworkError, NetworkEffect, NetworkError, NetworkRequest,
    NetworkRequestContext,
};
use graphit_hub::policy::RequestContext;
use http::{HeaderMap, HeaderValue, Method, header};
use serde::{
    Deserialize, Serialize, Serializer,
    ser::{SerializeSeq, SerializeStruct},
};
use std::{collections::HashSet, error::Error, fmt, io, sync::Arc};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnInventory {
    pub instance_id: InstallationId,
    pub projects: Vec<MaintainedProjectVersions>,
    /// Nonnegative UTC epoch microseconds supplied by the Broker.
    pub created_at: i64,
    pub updated_at: i64,
}

/// The caller selects one credential path; a Native failure never retries with a local key.
#[derive(Clone, Copy)]
pub enum InventoryCredentialMode<'a> {
    LocalBrokerKey,
    BrokerNative(&'a BrokerVerifierCache),
}

/// Real admission causes remain available without appearing in diagnostics.
pub enum InventoryAdmissionFailure {
    Auth(ServiceUseError),
    Verifier(BrokerVerifierCacheError),
    Scope(io::Error),
    DifferentOriginal,
    UnexpectedBinding,
}
impl fmt::Debug for InventoryAdmissionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auth(_) => "Auth",
            Self::Verifier(_) => "Verifier",
            Self::Scope(_) => "Scope",
            Self::DifferentOriginal => "DifferentOriginal",
            Self::UnexpectedBinding => "UnexpectedBinding",
        })
    }
}
impl fmt::Display for InventoryAdmissionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for InventoryAdmissionFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Auth(e) => Some(e),
            Self::Verifier(e) => Some(e),
            Self::Scope(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryRemoteCode {
    Unauthenticated,
    Forbidden,
    InvalidInventory,
    OwnerConflict,
    NotFound,
    AuthenticationUnavailable,
    AuthorizationUnavailable,
    InvalidPolicy,
    InventoryUnavailable,
    Closed,
    OutcomeUncertain,
    Deadline,
    MethodNotAllowed,
}
impl InventoryRemoteCode {
    fn matches_status(self, status: u16) -> bool {
        use InventoryRemoteCode::*;
        matches!(
            (status, self),
            (401, Unauthenticated)
                | (403, Forbidden)
                | (400, InvalidInventory)
                | (409, OwnerConflict)
                | (404, NotFound)
                | (
                    503,
                    AuthenticationUnavailable
                        | AuthorizationUnavailable
                        | InvalidPolicy
                        | InventoryUnavailable
                        | Closed
                        | OutcomeUncertain
                )
                | (504, Deadline)
                | (405, MethodNotAllowed)
        )
    }
}

pub enum OwnInventoryReadFailure {
    Admission {
        source: InventoryAdmissionFailure,
        effect: NetworkEffect,
    },
    Network {
        source: NetworkError,
        effect: NetworkEffect,
    },
    InvalidResponse {
        effect: NetworkEffect,
    },
    /// NotFound also describes a missing route; it never establishes an empty inventory.
    Remote {
        status: u16,
        code: InventoryRemoteCode,
        effect: NetworkEffect,
    },
}
impl OwnInventoryReadFailure {
    pub fn effect(&self) -> NetworkEffect {
        match self {
            Self::Admission { effect, .. }
            | Self::Network { effect, .. }
            | Self::InvalidResponse { effect }
            | Self::Remote { effect, .. } => *effect,
        }
    }
    fn admission(source: InventoryAdmissionFailure, effect: NetworkEffect) -> Self {
        Self::Admission { source, effect }
    }
}
impl fmt::Debug for OwnInventoryReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admission { effect, .. } => {
                f.debug_struct("Admission").field("effect", effect).finish()
            }
            Self::Network { source, effect } => f
                .debug_struct("Network")
                .field("source", source)
                .field("effect", effect)
                .finish(),
            Self::InvalidResponse { effect } => f
                .debug_struct("InvalidResponse")
                .field("effect", effect)
                .finish(),
            Self::Remote {
                status,
                code,
                effect,
            } => f
                .debug_struct("Remote")
                .field("status", status)
                .field("code", code)
                .field("effect", effect)
                .finish(),
        }
    }
}
impl fmt::Display for OwnInventoryReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for OwnInventoryReadFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Admission { source, .. } => Some(source),
            Self::Network { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn recheck_scope(
    capture: ScopeCapture<'_>,
    context: &ScopeContext,
    original: &RequestContext<'_>,
) -> Result<(), InventoryAdmissionFailure> {
    original
        .snapshot()
        .check()
        .map_err(InventoryAdmissionFailure::Scope)?;
    capture
        .live
        .revalidate_installation(context)
        .map_err(InventoryAdmissionFailure::Scope)?;
    if !capture
        .record
        .matches_scope_bounded(capture.live, context)
        .map_err(InventoryAdmissionFailure::Scope)?
    {
        return Err(InventoryAdmissionFailure::Scope(
            io::ErrorKind::InvalidData.into(),
        ));
    }
    original
        .snapshot()
        .check()
        .map_err(InventoryAdmissionFailure::Scope)
}
fn recheck(
    lease: &ServiceCredentialLease<'_>,
    capture: ScopeCapture<'_>,
    context: &ScopeContext,
    original: &RequestContext<'_>,
) -> Result<(), InventoryAdmissionFailure> {
    lease.check().map_err(InventoryAdmissionFailure::Auth)?;
    if !Arc::ptr_eq(
        lease
            .original()
            .map_err(InventoryAdmissionFailure::Auth)?
            .snapshot(),
        original.snapshot(),
    ) {
        return Err(InventoryAdmissionFailure::DifferentOriginal);
    }
    recheck_scope(capture, context, original)
}
fn inventory_url(endpoint: &str, id: &InstallationId) -> Result<Url, ()> {
    let mut url = Url::parse(endpoint).map_err(|_| ())?;
    url.path_segments_mut()
        .map_err(|_| ())?
        .pop_if_empty()
        .extend(["v1", "instances", id.as_str(), "inventory"]);
    Ok(url)
}

/// Admit through this service's issuer and captured installation, preserving one Original.
/// Bounds and origin policy are supplied by the existing BoundedClient. No request is retried.
pub async fn read_own_inventory(
    client: &BoundedClient,
    service: &DaemonAuth,
    caller: &AuthenticatedCaller<'_>,
    mode: InventoryCredentialMode<'_>,
    original: &RequestContext<'_>,
) -> Result<OwnInventory, OwnInventoryReadFailure> {
    let capture = service.installation();
    let context = ScopeContext::from_snapshot(original.snapshot());
    recheck_scope(capture, &context, original)
        .map_err(|e| OwnInventoryReadFailure::admission(e, NetworkEffect::NotSent))?;
    let StoreOwner::Installation { installation_id } = capture.live.owner() else {
        return Err(OwnInventoryReadFailure::admission(
            InventoryAdmissionFailure::UnexpectedBinding,
            NetworkEffect::NotSent,
        ));
    };
    let mut selection = service
        .issuer()
        .capture_service(caller, ServicePurpose::InventoryRead)
        .map_err(|e| {
            OwnInventoryReadFailure::admission(
                InventoryAdmissionFailure::Auth(e),
                NetworkEffect::NotSent,
            )
        })?;
    let captured_original = selection.original().map_err(|e| {
        OwnInventoryReadFailure::admission(
            InventoryAdmissionFailure::Auth(e),
            NetworkEffect::NotSent,
        )
    })?;
    if !Arc::ptr_eq(captured_original.snapshot(), original.snapshot()) {
        return Err(OwnInventoryReadFailure::admission(
            InventoryAdmissionFailure::DifferentOriginal,
            NetworkEffect::NotSent,
        ));
    }
    // Keep this Arc alive through the borrowed lease, gates and HTTP awaits.
    let verifier = match mode {
        InventoryCredentialMode::LocalBrokerKey => None,
        InventoryCredentialMode::BrokerNative(cache) => {
            Some(cache.get(&mut selection).map_err(|e| {
                OwnInventoryReadFailure::admission(
                    InventoryAdmissionFailure::Verifier(e),
                    NetworkEffect::NotSent,
                )
            })?)
        }
    };
    let credential = match &verifier {
        Some(verifier) => selection.native_credential(verifier).await,
        None => selection.credential(),
    };
    let lease = credential.map_err(|e| {
        OwnInventoryReadFailure::admission(
            InventoryAdmissionFailure::Auth(e),
            NetworkEffect::NotSent,
        )
    })?;
    let mut gate = || recheck(&lease, capture, &context, original);
    gate().map_err(|e| OwnInventoryReadFailure::admission(e, NetworkEffect::NotSent))?;
    let ServiceBindingView::BrokerInventory {
        purpose: ServicePurpose::InventoryRead,
        endpoint,
        ..
    } = lease.binding().map_err(|e| {
        OwnInventoryReadFailure::admission(
            InventoryAdmissionFailure::Auth(e),
            NetworkEffect::NotSent,
        )
    })?
    else {
        return Err(OwnInventoryReadFailure::admission(
            InventoryAdmissionFailure::UnexpectedBinding,
            NetworkEffect::NotSent,
        ));
    };
    let url = inventory_url(endpoint, installation_id).map_err(|_| {
        OwnInventoryReadFailure::admission(
            InventoryAdmissionFailure::UnexpectedBinding,
            NetworkEffect::NotSent,
        )
    })?;
    let credential = lease
        .credential()
        .map_err(|e| {
            OwnInventoryReadFailure::admission(
                InventoryAdmissionFailure::Auth(e),
                NetworkEffect::NotSent,
            )
        })?
        .ok_or_else(|| {
            OwnInventoryReadFailure::admission(
                InventoryAdmissionFailure::UnexpectedBinding,
                NetworkEffect::NotSent,
            )
        })?;
    let mut bearer = HeaderValue::from_str(&format!("Bearer {credential}")).map_err(|_| {
        OwnInventoryReadFailure::Network {
            source: NetworkError::Encoding,
            effect: NetworkEffect::NotSent,
        }
    })?;
    bearer.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, bearer);
    let response = client
        .execute_guarded(
            NetworkRequest {
                method: Method::GET,
                url: url.as_str(),
                headers: &headers,
                body: &[],
            },
            NetworkRequestContext::with_deadline(original.deadline(), original.cancellation()),
            &mut gate,
        )
        .await
        .map_err(|e| match e {
            GuardedNetworkError::Network { source, effect } => {
                OwnInventoryReadFailure::Network { source, effect }
            }
            GuardedNetworkError::Admission { source, effect } => {
                OwnInventoryReadFailure::Admission { source, effect }
            }
        })?;
    let effect = NetworkEffect::MayHaveApplied;
    gate().map_err(|e| OwnInventoryReadFailure::admission(e, effect))?;
    let result = if response.status().as_u16() == 200 {
        decode_inventory(response.body(), installation_id)
            .map_err(|_| OwnInventoryReadFailure::InvalidResponse { effect })
    } else {
        let status = response.status().as_u16();
        let challenge_valid = status != 401
            || response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split_whitespace().next())
                .is_some_and(|v| v.eq_ignore_ascii_case("Bearer"));
        match decode_remote(response.body(), status) {
            Ok(code) if challenge_valid => Err(OwnInventoryReadFailure::Remote {
                status,
                code,
                effect,
            }),
            _ => Err(OwnInventoryReadFailure::InvalidResponse { effect }),
        }
    };
    gate().map_err(|e| OwnInventoryReadFailure::admission(e, effect))?;
    result
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireInventory {
    instance_id: String,
    projects: Vec<WireProject>,
    created_at: i64,
    updated_at: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireProject {
    project_id: ProjectId,
    versions: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireError {
    error: WireErrorCode,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireErrorCode {
    code: InventoryRemoteCode,
}
fn decode_inventory(bytes: &[u8], expected: &InstallationId) -> Result<OwnInventory, ()> {
    let wire: WireInventory = serde_json::from_slice(bytes).map_err(|_| ())?;
    let instance_id = InstallationId::new(&wire.instance_id).map_err(|_| ())?;
    if instance_id.as_str() != wire.instance_id
        || &instance_id != expected
        || wire.created_at < 0
        || wire.updated_at < 0
    {
        return Err(());
    }
    let mut projects = Vec::new();
    projects.try_reserve(wire.projects.len()).map_err(|_| ())?;
    let mut ids = HashSet::new();
    ids.try_reserve(wire.projects.len()).map_err(|_| ())?;
    for p in wire.projects {
        if !ids.insert(p.project_id) || p.versions.is_empty() {
            return Err(());
        }
        let mut versions = HashSet::new();
        versions.try_reserve(p.versions.len()).map_err(|_| ())?;
        for version in &p.versions {
            if version.is_empty()
                || version.trim() != version
                || version.chars().any(char::is_control)
                || !versions.insert(version.as_str())
            {
                return Err(());
            }
        }
        projects.push(MaintainedProjectVersions {
            project_id: p.project_id,
            versions: p.versions,
        });
    }
    Ok(OwnInventory {
        instance_id,
        projects,
        created_at: wire.created_at,
        updated_at: wire.updated_at,
    })
}
fn decode_remote(bytes: &[u8], status: u16) -> Result<InventoryRemoteCode, ()> {
    let wire: WireError = serde_json::from_slice(bytes).map_err(|_| ())?;
    wire.error
        .code
        .matches_status(status)
        .then_some(wire.error.code)
        .ok_or(())
}

/// A confirmed change reported by the Broker, not a local prediction of a remote write.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnInventoryChange {
    Created,
    Updated,
    Unchanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnInventoryPublication {
    pub instance_id: InstallationId,
    pub change: OwnInventoryChange,
    /// Nonnegative UTC epoch microseconds supplied by the Broker.
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryPublicationRemoteCode {
    InvalidJson,
    InvalidInventory,
    BodyTooLarge,
    Unauthenticated,
    Forbidden,
    OwnerConflict,
    NotFound,
    AuthenticationUnavailable,
    AuthorizationUnavailable,
    InvalidPolicy,
    RegistrationUnavailable,
    Closed,
    OutcomeUncertain,
    Deadline,
    MethodNotAllowed,
}
impl InventoryPublicationRemoteCode {
    fn matches_status(self, status: u16) -> bool {
        use InventoryPublicationRemoteCode::*;
        matches!(
            (status, self),
            (400, InvalidJson | InvalidInventory | BodyTooLarge)
                | (401, Unauthenticated)
                | (403, Forbidden)
                | (409, OwnerConflict)
                | (404, NotFound)
                | (
                    503,
                    AuthenticationUnavailable
                        | AuthorizationUnavailable
                        | InvalidPolicy
                        | RegistrationUnavailable
                        | Closed
                        | OutcomeUncertain
                )
                | (504, Deadline)
                | (405, MethodNotAllowed)
        )
    }
}

/// Failures preserve real causes and the possibility of a write after native send.
pub enum OwnInventoryPublicationFailure {
    Admission {
        source: InventoryAdmissionFailure,
        effect: NetworkEffect,
    },
    Snapshot {
        source: io::Error,
        effect: NetworkEffect,
    },
    Encoding {
        source: serde_json::Error,
        effect: NetworkEffect,
    },
    Network {
        source: NetworkError,
        effect: NetworkEffect,
    },
    InvalidResponse {
        effect: NetworkEffect,
    },
    Remote {
        status: u16,
        code: InventoryPublicationRemoteCode,
        effect: NetworkEffect,
    },
}
impl OwnInventoryPublicationFailure {
    pub fn effect(&self) -> NetworkEffect {
        match self {
            Self::Admission { effect, .. }
            | Self::Snapshot { effect, .. }
            | Self::Encoding { effect, .. }
            | Self::Network { effect, .. }
            | Self::InvalidResponse { effect }
            | Self::Remote { effect, .. } => *effect,
        }
    }
    fn admission(source: InventoryAdmissionFailure, effect: NetworkEffect) -> Self {
        Self::Admission { source, effect }
    }
}
impl fmt::Debug for OwnInventoryPublicationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admission { effect, .. } => {
                f.debug_struct("Admission").field("effect", effect).finish()
            }
            Self::Snapshot { effect, .. } => {
                f.debug_struct("Snapshot").field("effect", effect).finish()
            }
            Self::Encoding { effect, .. } => {
                f.debug_struct("Encoding").field("effect", effect).finish()
            }
            Self::Network { source, effect } => f
                .debug_struct("Network")
                .field("source", source)
                .field("effect", effect)
                .finish(),
            Self::InvalidResponse { effect } => f
                .debug_struct("InvalidResponse")
                .field("effect", effect)
                .finish(),
            Self::Remote {
                status,
                code,
                effect,
            } => f
                .debug_struct("Remote")
                .field("status", status)
                .field("code", code)
                .field("effect", effect)
                .finish(),
        }
    }
}
impl fmt::Display for OwnInventoryPublicationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for OwnInventoryPublicationFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Admission { source, .. } => Some(source),
            Self::Snapshot { source, .. } => Some(source),
            Self::Encoding { source, .. } => Some(source),
            Self::Network { source, .. } => Some(source),
            _ => None,
        }
    }
}

struct PublishProjects<'a>(&'a [MaintainedProjectVersions]);
impl Serialize for PublishProjects<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for project in self.0 {
            sequence.serialize_element(&PublishProject(project))?;
        }
        sequence.end()
    }
}
struct PublishProject<'a>(&'a MaintainedProjectVersions);
impl Serialize for PublishProject<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut fields = serializer.serialize_struct("Project", 2)?;
        fields.serialize_field("project_id", &self.0.project_id)?;
        fields.serialize_field("versions", &self.0.versions)?;
        fields.end()
    }
}
#[derive(Serialize)]
struct PublishBody<'a> {
    projects: PublishProjects<'a>,
}

struct InventoryBodyWriter<'a, 'b> {
    bytes: Vec<u8>,
    limit: usize,
    original: &'a RequestContext<'b>,
}
impl io::Write for InventoryBodyWriter<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.original.snapshot().check()?;
        let end = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if end > self.limit {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        if end > self.bytes.capacity() {
            let capacity = end.max(self.bytes.capacity().saturating_mul(2).min(self.limit));
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(|source| io::Error::new(io::ErrorKind::OutOfMemory, source))?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        // Every write is complete or fails. Retrying Interrupted would hide cancellation.
        self.write(bytes).map(|_| ())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.original.snapshot().check()
    }
}
fn encode_inventory(
    projects: &[MaintainedProjectVersions],
    client: &BoundedClient,
    original: &RequestContext<'_>,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut writer = InventoryBodyWriter {
        bytes: Vec::new(),
        limit: client.request_body_bytes(),
        original,
    };
    serde_json::to_writer(
        &mut writer,
        &PublishBody {
            projects: PublishProjects(projects),
        },
    )?;
    Ok(writer.bytes)
}

/// Publish all historical Core inventory through this service's actual InventoryPublish admission.
/// `max_document_bytes` is the existing Core reader bound. Encoding uses this client's own
/// request body limit; no request is retried or interpreted as rolled back after native send.
pub async fn publish_maintained_inventory(
    client: &BoundedClient,
    service: &DaemonAuth,
    caller: &AuthenticatedCaller<'_>,
    mode: InventoryCredentialMode<'_>,
    original: &RequestContext<'_>,
    max_document_bytes: usize,
) -> Result<OwnInventoryPublication, OwnInventoryPublicationFailure> {
    let capture = service.installation();
    let context = ScopeContext::from_snapshot(original.snapshot());
    let not_sent = NetworkEffect::NotSent;
    recheck_scope(capture, &context, original)
        .map_err(|e| OwnInventoryPublicationFailure::admission(e, not_sent))?;
    let StoreOwner::Installation { installation_id } = capture.live.owner() else {
        return Err(OwnInventoryPublicationFailure::admission(
            InventoryAdmissionFailure::UnexpectedBinding,
            not_sent,
        ));
    };
    let mut selection = service
        .issuer()
        .capture_service(caller, ServicePurpose::InventoryPublish)
        .map_err(|e| {
            OwnInventoryPublicationFailure::admission(InventoryAdmissionFailure::Auth(e), not_sent)
        })?;
    let captured_original = selection.original().map_err(|e| {
        OwnInventoryPublicationFailure::admission(InventoryAdmissionFailure::Auth(e), not_sent)
    })?;
    if !Arc::ptr_eq(captured_original.snapshot(), original.snapshot()) {
        return Err(OwnInventoryPublicationFailure::admission(
            InventoryAdmissionFailure::DifferentOriginal,
            not_sent,
        ));
    }
    let verifier = match mode {
        InventoryCredentialMode::LocalBrokerKey => None,
        InventoryCredentialMode::BrokerNative(cache) => {
            Some(cache.get(&mut selection).map_err(|e| {
                OwnInventoryPublicationFailure::admission(
                    InventoryAdmissionFailure::Verifier(e),
                    not_sent,
                )
            })?)
        }
    };
    let credential = match &verifier {
        Some(verifier) => selection.native_credential(verifier).await,
        None => selection.credential(),
    };
    let lease = credential.map_err(|e| {
        OwnInventoryPublicationFailure::admission(InventoryAdmissionFailure::Auth(e), not_sent)
    })?;
    let mut gate = || recheck(&lease, capture, &context, original);
    gate().map_err(|e| OwnInventoryPublicationFailure::admission(e, not_sent))?;
    let observed = read_maintained_inventory(service, original.snapshot(), max_document_bytes);
    gate().map_err(|e| OwnInventoryPublicationFailure::admission(e, not_sent))?;
    let snapshot = observed.map_err(|source| OwnInventoryPublicationFailure::Snapshot {
        source,
        effect: not_sent,
    })?;
    if &snapshot.installation_id != installation_id {
        return Err(OwnInventoryPublicationFailure::admission(
            InventoryAdmissionFailure::UnexpectedBinding,
            not_sent,
        ));
    }
    let encoded = encode_inventory(&snapshot.projects, client, original);
    gate().map_err(|e| OwnInventoryPublicationFailure::admission(e, not_sent))?;
    let body = encoded.map_err(|source| OwnInventoryPublicationFailure::Encoding {
        source,
        effect: not_sent,
    })?;
    let ServiceBindingView::BrokerInventory {
        purpose: ServicePurpose::InventoryPublish,
        endpoint,
        ..
    } = lease.binding().map_err(|e| {
        OwnInventoryPublicationFailure::admission(InventoryAdmissionFailure::Auth(e), not_sent)
    })?
    else {
        return Err(OwnInventoryPublicationFailure::admission(
            InventoryAdmissionFailure::UnexpectedBinding,
            not_sent,
        ));
    };
    let url = inventory_url(endpoint, installation_id).map_err(|_| {
        OwnInventoryPublicationFailure::admission(
            InventoryAdmissionFailure::UnexpectedBinding,
            not_sent,
        )
    })?;
    let credential = lease
        .credential()
        .map_err(|e| {
            OwnInventoryPublicationFailure::admission(InventoryAdmissionFailure::Auth(e), not_sent)
        })?
        .ok_or_else(|| {
            OwnInventoryPublicationFailure::admission(
                InventoryAdmissionFailure::UnexpectedBinding,
                not_sent,
            )
        })?;
    let mut bearer = HeaderValue::from_str(&format!("Bearer {credential}")).map_err(|_| {
        OwnInventoryPublicationFailure::Network {
            source: NetworkError::Encoding,
            effect: not_sent,
        }
    })?;
    bearer.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, bearer);
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let response = client
        .execute_guarded(
            NetworkRequest {
                method: Method::PUT,
                url: url.as_str(),
                headers: &headers,
                body: &body,
            },
            NetworkRequestContext::with_deadline(original.deadline(), original.cancellation()),
            &mut gate,
        )
        .await
        .map_err(|error| match error {
            GuardedNetworkError::Network { source, effect } => {
                OwnInventoryPublicationFailure::Network { source, effect }
            }
            GuardedNetworkError::Admission { source, effect } => {
                OwnInventoryPublicationFailure::Admission { source, effect }
            }
        })?;
    let effect = NetworkEffect::MayHaveApplied;
    gate().map_err(|e| OwnInventoryPublicationFailure::admission(e, effect))?;
    let status = response.status().as_u16();
    let result = if matches!(status, 200 | 201) {
        decode_publication(response.body(), installation_id, status)
            .map_err(|_| OwnInventoryPublicationFailure::InvalidResponse { effect })
    } else {
        let challenge_valid = status != 401
            || response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split_whitespace().next())
                .is_some_and(|v| v.eq_ignore_ascii_case("Bearer"));
        match decode_publication_remote(response.body(), status) {
            Ok(code) if challenge_valid => Err(OwnInventoryPublicationFailure::Remote {
                status,
                code,
                effect,
            }),
            _ => Err(OwnInventoryPublicationFailure::InvalidResponse { effect }),
        }
    };
    gate().map_err(|e| OwnInventoryPublicationFailure::admission(e, effect))?;
    result
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePublication {
    instance_id: String,
    change: OwnInventoryChange,
    created_at: i64,
    updated_at: i64,
}
fn decode_publication(
    bytes: &[u8],
    expected: &InstallationId,
    status: u16,
) -> Result<OwnInventoryPublication, ()> {
    let wire: WirePublication = serde_json::from_slice(bytes).map_err(|_| ())?;
    let instance_id = InstallationId::new(&wire.instance_id).map_err(|_| ())?;
    if instance_id.as_str() != wire.instance_id
        || &instance_id != expected
        || wire.created_at < 0
        || wire.updated_at < 0
        || !matches!(
            (status, wire.change),
            (201, OwnInventoryChange::Created)
                | (
                    200,
                    OwnInventoryChange::Updated | OwnInventoryChange::Unchanged
                )
        )
    {
        return Err(());
    }
    Ok(OwnInventoryPublication {
        instance_id,
        change: wire.change,
        created_at: wire.created_at,
        updated_at: wire.updated_at,
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePublicationError {
    error: WirePublicationErrorCode,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePublicationErrorCode {
    code: InventoryPublicationRemoteCode,
}
fn decode_publication_remote(
    bytes: &[u8],
    status: u16,
) -> Result<InventoryPublicationRemoteCode, ()> {
    let wire: WirePublicationError = serde_json::from_slice(bytes).map_err(|_| ())?;
    wire.error
        .code
        .matches_status(status)
        .then_some(wire.error.code)
        .ok_or(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
