use crate::{
    policy::{
        AuthError, IssuerLimits, Obligation, Permission, RoleDefinition, TtlPolicy,
        canonical_permissions, count, subset,
    },
    scope::{PhysicalScopeInput, ScopeCapture, ScopeFacts, ScopeInput},
};
mod imported_knowledge;
pub use imported_knowledge::ImportedKnowledgeReadAdmission;
mod installed_hub;
pub use installed_hub::InstalledHubReadAdmission;
mod web_hub_mutation;
pub use web_hub_mutation::{
    WebHubMutationAdmission, WebHubProjectMutationInput, WebHubPublicationAdmission,
    WebHubPublicationInput,
};
mod imported_ast;
pub use imported_ast::ImportedAstReadAdmission;
mod native_installation;
mod record_storage;
mod records;
pub use record_storage::{
    RecordStorageAdmission, RecordStorageResolver, RecordStorageScope, RemoteRecordStorage,
};
pub use records::{
    ProjectReadAdmission, ProjectReadModule, ProjectRecordAuthority, ProjectWriteAdmission,
    ProjectWriteOperation,
};
pub(crate) mod service_credentials;
mod user_records;
mod user_subject;
pub use service_credentials::{
    CredentialSource, IncomingPrincipalInput, LocalServiceLease, NativeProfileRevision,
    RemoteServiceBindingInput, S3CredentialView, ServiceAdmissionLimits, ServiceAuthorityInput,
    ServiceBindingView, ServiceCredentialLease, ServiceMode, ServiceOrigin, ServicePurpose,
    ServiceRuleInput, ServiceSelection, ServiceUseError, StandaloneS3View, SubjectSelector,
};
pub use user_records::{
    UserKnowledgeReadAdmission, UserKnowledgeWriteAdmission, UserMemoryWriteOperation,
    UserReadAdmission, UserWriteAdmission,
};
pub use user_subject::{VerifiedUserOrigin, VerifiedUserSubject};
mod source;
use crate::store::AuthStore;
use source::{LocalState, Origin, SourceProof, source_generation};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeBindingRecord, ScopeContext, StoreOwner},
    installation_identity::InstallationId,
    runtime_key::{CapturedRuntimeKeyBorrow, RuntimeKeyError, RuntimeKeyOwner},
};
use graphit_hub::{native::RoleCredentialReference, policy::RequestContext};
use secrecy::{ExposeSecret, SecretString, zeroize::Zeroizing};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, RwLock},
    time::{Duration, Instant, SystemTime},
};
use subtle::ConstantTimeEq;

type Id = [u8; 32];
/// Private nonce transport. Explicit exposure is reserved for runtime delivery.
/// ```compile_fail
/// use graphit_auth::RoleBearer;
/// fn debug<T: std::fmt::Debug>() {} debug::<RoleBearer>();
/// ```
/// ```compile_fail
/// use graphit_auth::RoleBearer;
/// fn clone<T: Clone>() {} clone::<RoleBearer>();
/// ```
/// ```compile_fail
/// use graphit_auth::RoleBearer;
/// fn serializable<T: serde::Serialize>() {} serializable::<RoleBearer>();
/// ```
/// ```compile_fail
/// use graphit_auth::RoleBearer;
/// let bearer = RoleBearer { secret: "chosen nonce".into() };
/// ```
pub struct RoleBearer {
    secret: SecretString,
}
impl RoleBearer {
    pub fn expose_for_transport(&self) -> &str {
        self.secret.expose_secret()
    }
}
/// Server-minted attribution, never an input accepted as authority.
#[derive(Clone, PartialEq, Eq)]
pub struct Actor(Id);
impl Actor {
    pub fn write_identifier(&self, output: &mut [u8; 64]) {
        for (i, b) in self.0.iter().enumerate() {
            output[i * 2] = b"0123456789abcdef"[(b >> 4) as usize];
            output[i * 2 + 1] = b"0123456789abcdef"[(b & 15) as usize];
        }
    }
}
/// ```compile_fail
/// use graphit_auth::RunHandle;
/// fn clone<T: Clone>() {} clone::<RunHandle>();
/// ```
/// ```compile_fail
/// use graphit_auth::RunHandle;
/// let run = RunHandle { epoch: [0; 32], id: [0; 32] };
/// ```
/// ```compile_fail
/// use graphit_auth::RunHandle;
/// fn debug<T: std::fmt::Debug>() {} debug::<RunHandle>();
/// ```
pub struct RunHandle {
    epoch: Id,
    id: Id,
}
pub struct IssuedRole {
    pub run: RunHandle,
    pub bearer: RoleBearer,
}
/// A proof for this original request; never a persistent administrative session.
/// ```compile_fail
/// use graphit_auth::AuthenticatedCaller;
/// fn clone<T: Clone>() {} clone::<AuthenticatedCaller<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::AuthenticatedCaller;
/// fn debug<T: std::fmt::Debug>() {} debug::<AuthenticatedCaller<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::AuthenticatedCaller;
/// fn serial<T: serde::Serialize>() {} serial::<AuthenticatedCaller<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::AuthenticatedCaller;
/// let proof = AuthenticatedCaller { issuer: (), key: (), digest: (), request: () };
/// ```
/// ```compile_fail
/// use graphit_auth::AuthenticatedCaller;
/// let _: AuthenticatedCaller<'static> = serde_json::from_str("{}").unwrap();
/// ```
pub struct AuthenticatedCaller<'a> {
    issuer: &'a RoleIssuer,
    source: SourceProof<'a>,
    native: Option<native_installation::NativePhase<'a>>,
    request: &'a RequestContext<'a>,
}
/// Process-local session selector. Its holder still needs fresh matching-origin authentication.
/// ```compile_fail
/// use graphit_auth::AdminSessionHandle;
/// fn clone<T: Clone>() {} clone::<AdminSessionHandle>();
/// ```
/// ```compile_fail
/// use graphit_auth::AdminSessionHandle;
/// fn debug<T: std::fmt::Debug>() {} debug::<AdminSessionHandle>();
/// ```
/// ```compile_fail
/// use graphit_auth::AdminSessionHandle;
/// fn serial<T: serde::Serialize>() {} serial::<AdminSessionHandle>();
/// ```
/// ```compile_fail
/// use graphit_auth::AdminSessionHandle;
/// let handle = AdminSessionHandle { epoch: [0;32], id: [0;32] };
/// ```
/// ```compile_fail
/// use graphit_auth::AdminSessionHandle;
/// let _: AdminSessionHandle = serde_json::from_str("{}").unwrap();
/// ```
pub struct AdminSessionHandle {
    epoch: Id,
    id: Id,
    origin: Origin,
    generation: u64,
}
/// Finite parent decision tied to a fresh authenticated request.
/// ```compile_fail
/// use graphit_auth::AdminParent;
/// fn clone<T: Clone>() {} clone::<AdminParent<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::AdminParent;
/// fn debug<T: std::fmt::Debug>() {} debug::<AdminParent<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::AdminParent;
/// fn serial<T: serde::Serialize>() {} serial::<AdminParent<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::AdminParent;
/// let proof = AdminParent { issuer: (), caller: (), session: [0;32], request: () };
/// ```
/// ```compile_fail
/// use graphit_auth::AdminParent;
/// let _: AdminParent<'static> = serde_json::from_str("{}").unwrap();
/// ```
pub struct AdminParent<'a> {
    issuer: &'a RoleIssuer,
    caller: &'a AuthenticatedCaller<'a>,
    session: Id,
    request: &'a RequestContext<'a>,
}
/// ```compile_fail
/// use graphit_auth::VerifiedRoleGrant;
/// fn serializable<T: serde::Serialize>() {} serializable::<VerifiedRoleGrant<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::VerifiedRoleGrant;
/// fn clone<T: Clone>() {} clone::<VerifiedRoleGrant<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::VerifiedRoleGrant;
/// fn debug<T: std::fmt::Debug>() {} debug::<VerifiedRoleGrant<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::VerifiedRoleGrant;
/// fn change_scope(grant: &mut VerifiedRoleGrant<'_>) { grant.hash = [0; 32]; }
/// ```
/// ```compile_fail
/// use graphit_auth::VerifiedRoleGrant;
/// use graphit_hub::native::RoleCredentialReference;
/// fn grant(reference: RoleCredentialReference) -> VerifiedRoleGrant<'static> { reference.into() }
/// ```
pub struct VerifiedRoleGrant<'a> {
    issuer: &'a RoleIssuer,
    id: CredentialId,
    hash: Id,
    scope: ScopeInput<'a>,
    request: &'a RequestContext<'a>,
    source: SourceProof<'a>,
}

#[derive(Clone, Copy)]
struct Moment {
    utc: SystemTime,
    mono: Instant,
}
trait Clock: Send + Sync {
    fn now(&self) -> Moment;
}
struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Moment {
        Moment {
            utc: SystemTime::now(),
            mono: Instant::now(),
        }
    }
}
trait Entropy: Send + Sync {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), AuthError>;
}
struct OsEntropy;
impl Entropy for OsEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
        getrandom::fill(bytes).map_err(|_| AuthError::Entropy)
    }
}
#[derive(Clone, Copy)]
struct Expiry {
    created: Moment,
    utc: SystemTime,
    mono: Instant,
}
impl Expiry {
    fn after(duration: Duration, now: Moment) -> Result<Self, AuthError> {
        if duration.is_zero() {
            return Err(AuthError::Expired);
        }
        Ok(Self {
            created: now,
            utc: now.utc.checked_add(duration).ok_or(AuthError::Invalid)?,
            mono: now.mono.checked_add(duration).ok_or(AuthError::Invalid)?,
        })
    }
    fn remaining(self, now: Moment) -> Result<Duration, AuthError> {
        if now.utc < self.created.utc || now.mono < self.created.mono {
            return Err(AuthError::Expired);
        }
        let a = self
            .utc
            .duration_since(now.utc)
            .map_err(|_| AuthError::Expired)?;
        let b = self
            .mono
            .checked_duration_since(now.mono)
            .ok_or(AuthError::Expired)?;
        let remaining = a.min(b);
        if remaining.is_zero() {
            Err(AuthError::Expired)
        } else {
            Ok(remaining)
        }
    }
}
struct AdminSession {
    origin: Origin,
    generation: u64,
    expiry: Expiry,
}
struct Profile {
    definition: RoleDefinition,
    generation: u64,
}
struct Run {
    origin: Origin,
    session: Id,
    scope: Id,
    expiry: Expiry,
    generation: u64,
    next: u64,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CredentialId {
    run: Id,
    sequence: u64,
}
struct Credential {
    hash: Id,
    profile: usize,
    profile_generation: u64,
    parent: Option<CredentialId>,
    permissions: Vec<Permission>,
    expiry: Expiry,
    actor: Actor,
}
struct State {
    service_enabled: bool,
    service_generation: u64,
    runtime_generation: u64,
    local: LocalState,
    sessions: BTreeMap<Id, AdminSession>,
    profiles: Vec<Profile>,
    scopes: BTreeMap<Id, Arc<ScopeFacts>>,
    runs: BTreeMap<Id, Run>,
    credentials: BTreeMap<CredentialId, Credential>,
    tokens: BTreeMap<Id, CredentialId>,
}
pub struct RoleIssuer {
    epoch: Id,
    installation: InstallationId,
    brand: String,
    realm: CapturedPublicationScope,
    realm_record: ScopeBindingRecord,
    trusted_roots: RwLock<BTreeSet<PathBuf>>,
    runtime: Arc<RuntimeKeyOwner>,
    local: Arc<AuthStore>,
    policy: crate::policy::IssuerAuthorityPolicy,
    limits: IssuerLimits,
    ttl: TtlPolicy,
    state: Mutex<State>,
    clock: Arc<dyn Clock>,
    entropy: Arc<dyn Entropy>,
    record_storage: std::sync::OnceLock<Arc<dyn RecordStorageResolver>>,
    native_profile_refresh: tokio::sync::Mutex<crate::profile_manager::NativeRefreshState>,
}
impl RoleIssuer {
    /// Add an exact physical PROJECT root after the service has bootstrapped its identity.
    /// Trust survives retirement of activity and grants no module permission.
    /// Once this returns Ok, the insertion is applied; the consumer observes that effect
    /// before any subsequent Original check.
    pub fn trust_project_root(
        &self,
        caller: &AuthenticatedCaller<'_>,
        original: &RequestContext<'_>,
        project: ScopeCapture<'_>,
    ) -> Result<(), AuthError> {
        if !std::ptr::eq(self, caller.issuer)
            || !std::ptr::eq(original, caller.request)
            || !matches!(&caller.source, SourceProof::Runtime { .. })
            || !matches!(project.live.owner(), StoreOwner::Project { .. })
            || project.live.brand().key() != self.brand
        {
            return Err(AuthError::Denied);
        }
        caller.authorize(Permission::Configure)?;
        let context = ScopeContext::from_snapshot(original.snapshot());
        if !project
            .record
            .matches_scope_bounded(project.live, &context)
            .map_err(|_| AuthError::Denied)?
        {
            return Err(AuthError::Denied);
        }
        project
            .live
            .revalidate_bounded(&context)
            .map_err(|_| AuthError::Denied)?;
        caller.check()?;
        let root = project.live.owner_root().to_owned();
        // Same lock order as admissions: issuer state, then the small root set.
        let state = self.lock()?;
        self.caller_locked(caller, &state)?;
        let mut roots = self.trusted_roots.write().map_err(|_| AuthError::State)?;
        original.check().map_err(|_| AuthError::Request)?;
        roots.insert(root);
        Ok(())
    }

    pub(crate) fn native_profile_refresh(
        &self,
    ) -> &tokio::sync::Mutex<crate::profile_manager::NativeRefreshState> {
        &self.native_profile_refresh
    }
    pub(crate) fn profile_store(&self) -> &AuthStore {
        &self.local
    }
    /// Startup captures one real runtime owner and explicit immutable server policy.
    pub fn new(
        realm: ScopeCapture<'_>,
        runtime: Arc<RuntimeKeyOwner>,
        local: Arc<AuthStore>,
        policy: crate::policy::IssuerAuthorityPolicy,
        ttl: TtlPolicy,
        request: &RequestContext<'_>,
    ) -> Result<Self, AuthError> {
        Self::with_sources(
            realm,
            runtime,
            local,
            policy,
            ttl,
            request,
            Arc::new(SystemClock),
            Arc::new(OsEntropy),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn with_sources(
        realm: ScopeCapture<'_>,
        runtime: Arc<RuntimeKeyOwner>,
        local: Arc<AuthStore>,
        policy: crate::policy::IssuerAuthorityPolicy,
        ttl: TtlPolicy,
        request: &RequestContext<'_>,
        clock: Arc<dyn Clock>,
        entropy: Arc<dyn Entropy>,
    ) -> Result<Self, AuthError> {
        let limits = policy.limits;
        let ttl = ttl.validate()?;
        request.check().map_err(|_| AuthError::Request)?;
        let StoreOwner::Installation { installation_id } = realm.live.owner() else {
            return Err(AuthError::Invalid);
        };
        if !realm
            .record
            .matches_scope_bounded(realm.live, &ScopeContext::from_snapshot(request.snapshot()))
            .map_err(|_| AuthError::Denied)?
        {
            return Err(AuthError::Denied);
        }
        count(realm.record, limits.max_scope_bytes)?;
        let key = runtime.check(request.snapshot()).map_err(runtime_error)?;
        key.recheck_installation(realm.live, request.snapshot())
            .map_err(runtime_error)?;
        let generation = key.generation();
        local
            .recheck_installation(realm.live, request.snapshot())
            .map_err(source::store_error)?;
        let mut epoch = [0; 32];
        random_nonzero(entropy.as_ref(), &mut epoch)?;
        key.recheck_installation(realm.live, request.snapshot())
            .map_err(runtime_error)?;
        local
            .recheck_installation(realm.live, request.snapshot())
            .map_err(source::store_error)?;
        request.check().map_err(|_| AuthError::Request)?;
        Ok(Self {
            epoch,
            installation: installation_id.clone(),
            brand: realm.live.brand().key().into(),
            realm: realm.live.clone(),
            realm_record: realm.record.clone(),
            trusted_roots: RwLock::new(realm.live.allowed_roots().map(ToOwned::to_owned).collect()),
            runtime,
            local,
            policy,
            limits,
            ttl,
            clock,
            entropy,
            record_storage: std::sync::OnceLock::new(),
            native_profile_refresh: tokio::sync::Mutex::new(Default::default()),
            state: Mutex::new(State {
                service_enabled: true,
                service_generation: 0,
                runtime_generation: generation,
                local: LocalState {
                    watermark: 0,
                    generation: 0,
                    binding: None,
                },
                sessions: BTreeMap::new(),
                profiles: Vec::new(),
                scopes: BTreeMap::new(),
                runs: BTreeMap::new(),
                credentials: BTreeMap::new(),
                tokens: BTreeMap::new(),
            }),
        })
    }
    fn lock(&self) -> Result<MutexGuard<'_, State>, AuthError> {
        self.state.lock().map_err(|_| AuthError::State)
    }
    pub fn authenticate_runtime<'a>(
        &'a self,
        raw: &str,
        request: &'a RequestContext<'a>,
    ) -> Result<AuthenticatedCaller<'a>, AuthError> {
        self.authenticate(Origin::Runtime, raw, request)
    }
    pub fn authenticate_local<'a>(
        &'a self,
        raw: &str,
        request: &'a RequestContext<'a>,
    ) -> Result<AuthenticatedCaller<'a>, AuthError> {
        self.authenticate(Origin::Local, raw, request)
    }
    fn authenticate<'a>(
        &'a self,
        origin: Origin,
        raw: &str,
        request: &'a RequestContext<'a>,
    ) -> Result<AuthenticatedCaller<'a>, AuthError> {
        let source = self.capture_source(origin, Some(raw), request)?;
        request.check().map_err(|_| AuthError::Request)?;
        Ok(AuthenticatedCaller {
            issuer: self,
            source,
            native: None,
            request,
        })
    }
    fn observe_generation(&self, state: &mut State, generation: u64) -> Result<(), AuthError> {
        if generation < state.runtime_generation {
            return Err(AuthError::Denied);
        }
        state.runtime_generation = generation;
        prune(state, self.clock.now());
        Ok(())
    }
    pub fn bootstrap_admin(
        &self,
        caller: &AuthenticatedCaller<'_>,
    ) -> Result<AdminSessionHandle, AuthError> {
        caller.check()?;
        if !std::ptr::eq(self, caller.issuer) {
            return Err(AuthError::Denied);
        }
        let expiry = Expiry::after(
            self.origin_policy(caller.source.origin()).sessions.lifetime,
            self.clock.now(),
        )?;
        let id = self.session_id()?;
        caller.source.check_physical(self, caller.request)?;
        let mut state = self.lock()?;
        self.caller_locked(caller, &state)?;
        prune(&mut state, self.clock.now());
        if state.sessions.contains_key(&id) {
            return Err(AuthError::Entropy);
        }
        self.caller_locked(caller, &state)?;
        expiry.remaining(self.clock.now())?;
        state.sessions.insert(
            id,
            AdminSession {
                origin: caller.source.origin(),
                generation: caller.source.generation(),
                expiry,
            },
        );
        Ok(AdminSessionHandle {
            epoch: self.epoch,
            id,
            origin: caller.source.origin(),
            generation: caller.source.generation(),
        })
    }
    pub fn admin_parent<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        session: &'a AdminSessionHandle,
    ) -> Result<AdminParent<'a>, AuthError> {
        caller.check()?;
        if !std::ptr::eq(self, caller.issuer)
            || session.epoch != self.epoch
            || session.origin != caller.source.origin()
            || session.generation != caller.source.generation()
        {
            return Err(AuthError::Denied);
        }
        let parent = AdminParent {
            issuer: self,
            caller,
            session: session.id,
            request: caller.request,
        };
        self.parent_locked(&parent, &*self.lock()?)?;
        Ok(parent)
    }
    fn caller_locked(
        &self,
        caller: &AuthenticatedCaller<'_>,
        state: &State,
    ) -> Result<(), AuthError> {
        caller.request.check().map_err(|_| AuthError::Request)?;
        if !std::ptr::eq(self, caller.issuer) {
            return Err(AuthError::Denied);
        }
        caller.source.check_locked(state)
    }
    fn parent_locked(&self, parent: &AdminParent<'_>, state: &State) -> Result<(), AuthError> {
        if !std::ptr::eq(self, parent.issuer) {
            return Err(AuthError::Denied);
        }
        self.caller_locked(parent.caller, state)?;
        if state.sessions.get(&parent.session).is_none_or(|s| {
            s.origin != parent.caller.source.origin()
                || s.generation != parent.caller.source.generation()
        }) {
            return Err(AuthError::Denied);
        }
        self.session_expiry(state, parent.session)?
            .remaining(self.clock.now())
            .map(|_| ())
    }
    fn session_expiry(&self, state: &State, session: Id) -> Result<Expiry, AuthError> {
        let session = state.sessions.get(&session).ok_or(AuthError::Denied)?;
        if session.generation != source_generation(state, session.origin) {
            return Err(AuthError::Denied);
        }
        Ok(session.expiry)
    }
    fn run_parent(
        &self,
        state: &State,
        run: Id,
        parent: &AdminParent<'_>,
    ) -> Result<(), AuthError> {
        if state.runs.get(&run).ok_or(AuthError::Denied)?.session != parent.session {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
    // Retain the original facts briefly, inspect outside State, and compare the same Arc before effect.
    fn checked_run_scope(
        &self,
        run: Id,
        input: &ScopeInput<'_>,
        request: &RequestContext<'_>,
    ) -> Result<Arc<ScopeFacts>, AuthError> {
        let facts = {
            let state = self.lock()?;
            let run = state.runs.get(&run).ok_or(AuthError::Denied)?;
            Arc::clone(state.scopes.get(&run.scope).ok_or(AuthError::State)?)
        };
        if !facts.matches_checked(input, request)? {
            return Err(AuthError::Denied);
        }
        Ok(facts)
    }
    fn same_run_scope(state: &State, run: Id, facts: &Arc<ScopeFacts>) -> Result<(), AuthError> {
        let run = state.runs.get(&run).ok_or(AuthError::Denied)?;
        if !Arc::ptr_eq(state.scopes.get(&run.scope).ok_or(AuthError::State)?, facts) {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
    /// Configure a managed tool-owned role under the live administrative parent.
    /// Unchanged definitions retain their generation; changed definitions use the
    /// same replacement/revocation rules as explicit role configuration.
    pub fn configure_tool_owned_definition(
        &self,
        parent: &AdminParent<'_>,
        definition: RoleDefinition,
    ) -> Result<RoleCredentialReference, AuthError> {
        parent.check()?;
        if !matches!(
            definition.reference.source(),
            graphit_hub::native::RoleSource::ToolOwned(_)
        ) || !matches!(
            definition.reference.role(),
            graphit_hub::native::Role::Scout
                | graphit_hub::native::Role::Tracker
                | graphit_hub::native::Role::Scribe
        ) {
            return Err(AuthError::Denied);
        }
        let reference = definition.reference.clone();
        let previous = {
            let state = self.lock()?;
            self.parent_locked(parent, &state)?;
            let policy = &self
                .origin_policy(parent.caller.source.origin())
                .administrative;
            if !policy.contains(&Permission::ManageAccessGrants)
                || !subset(&definition.permissions, policy)
            {
                return Err(AuthError::Denied);
            }
            state
                .profiles
                .iter()
                .find(|profile| {
                    profile.definition.reference.role() == reference.role()
                        && profile.definition.reference.source() == reference.source()
                        && profile.definition.binding == definition.binding
                })
                .map(|profile| profile.definition.reference.clone())
        };
        match previous {
            Some(previous) if previous == reference => {
                parent.check()?;
            }
            Some(previous) => self.replace_definition(parent, &previous, definition)?,
            None => self.register_definition(parent, definition)?,
        }
        Ok(reference)
    }

    /// Register a Hub definition, or retain an identical definition atomically.
    /// Reuse preserves its generation and existing grants. A different policy
    /// for the same role, Hub source and binding requires explicit replacement.
    /// Both paths require the live administrative parent and permission ceiling.
    pub fn register_or_reuse_hub_definition(
        &self,
        parent: &AdminParent<'_>,
        definition: RoleDefinition,
    ) -> Result<(), AuthError> {
        parent.check()?;
        if !matches!(
            definition.reference.source(),
            graphit_hub::native::RoleSource::HubArtifact(_)
        ) {
            return Err(AuthError::Denied);
        }
        let mut state = self.lock()?;
        self.parent_locked(parent, &state)?;
        let policy = &self
            .origin_policy(parent.caller.source.origin())
            .administrative;
        if !policy.contains(&Permission::ManageAccessGrants)
            || !subset(&definition.permissions, policy)
        {
            return Err(AuthError::Denied);
        }
        let mut registered = false;
        for profile in &state.profiles {
            let current = &profile.definition;
            if current.reference == definition.reference
                || (current.reference.role() == definition.reference.role()
                    && current.reference.source() == definition.reference.source()
                    && current.binding == definition.binding)
            {
                if current.reference != definition.reference
                    || current.binding != definition.binding
                    || current.permissions != definition.permissions
                {
                    return Err(AuthError::Invalid);
                }
                registered = true;
            }
        }
        if !registered {
            state
                .profiles
                .try_reserve_exact(1)
                .map_err(|_| AuthError::Limit)?;
        }
        self.parent_locked(parent, &state)?;
        if !registered {
            state.profiles.push(Profile {
                definition,
                generation: 1,
            });
        }
        Ok(())
    }

    pub fn register_definition(
        &self,
        parent: &AdminParent<'_>,
        definition: RoleDefinition,
    ) -> Result<(), AuthError> {
        parent.check()?;
        let mut state = self.lock()?;
        self.parent_locked(parent, &state)?;
        if !self
            .origin_policy(parent.caller.source.origin())
            .administrative
            .contains(&Permission::ManageAccessGrants)
            || !subset(
                &definition.permissions,
                &self
                    .origin_policy(parent.caller.source.origin())
                    .administrative,
            )
        {
            return Err(AuthError::Denied);
        }
        if state
            .profiles
            .iter()
            .any(|p| p.definition.reference == definition.reference)
        {
            return Err(AuthError::Invalid);
        }
        state
            .profiles
            .try_reserve_exact(1)
            .map_err(|_| AuthError::Limit)?;
        self.parent_locked(parent, &state)?;
        state.profiles.push(Profile {
            definition,
            generation: 1,
        });
        Ok(())
    }
    pub fn replace_definition(
        &self,
        parent: &AdminParent<'_>,
        previous: &RoleCredentialReference,
        definition: RoleDefinition,
    ) -> Result<(), AuthError> {
        parent.check()?;
        let mut state = self.lock()?;
        self.parent_locked(parent, &state)?;
        if !self
            .origin_policy(parent.caller.source.origin())
            .administrative
            .contains(&Permission::ManageAccessGrants)
            || !subset(
                &definition.permissions,
                &self
                    .origin_policy(parent.caller.source.origin())
                    .administrative,
            )
        {
            return Err(AuthError::Denied);
        }
        let index = profile_index(&state, previous)?;
        if definition.reference.role() != previous.role()
            || definition.reference.source() != previous.source()
            || definition.binding != state.profiles[index].definition.binding
        {
            return Err(AuthError::Denied);
        }
        if state
            .profiles
            .iter()
            .enumerate()
            .any(|(i, p)| i != index && p.definition.reference == definition.reference)
        {
            return Err(AuthError::Invalid);
        }
        let generation = state.profiles[index]
            .generation
            .checked_add(1)
            .ok_or(AuthError::State)?;
        self.parent_locked(parent, &state)?;
        state.profiles[index] = Profile {
            definition,
            generation,
        };
        prune(&mut state, self.clock.now());
        Ok(())
    }
    pub fn start_run(
        &self,
        parent: &AdminParent<'_>,
        scope: &ScopeInput<'_>,
        lifetime: Duration,
    ) -> Result<RunHandle, AuthError> {
        parent.check()?;
        self.validate_scope(scope, parent.request)?;
        let id = self.run_id()?;
        let facts = Arc::new(ScopeFacts::capture(scope)?);
        parent.caller.source.check_physical(self, parent.request)?;
        let mut state = self.lock()?;
        self.parent_locked(parent, &state)?;
        prune(&mut state, self.clock.now());
        let expiry = Expiry::after(
            lifetime.min(
                self.session_expiry(&state, parent.session)?
                    .remaining(self.clock.now())?,
            ),
            self.clock.now(),
        )?;
        if state.runs.contains_key(&id) || state.scopes.contains_key(&id) {
            return Err(AuthError::Entropy);
        }
        self.parent_locked(parent, &state)?;
        expiry.remaining(self.clock.now())?;
        state.scopes.insert(id, facts);
        let generation = parent.caller.source.generation();
        state.runs.insert(
            id,
            Run {
                origin: parent.caller.source.origin(),
                session: parent.session,
                scope: id,
                expiry,
                generation,
                next: 0,
            },
        );
        Ok(RunHandle {
            epoch: self.epoch,
            id,
        })
    }
    fn validate_scope(
        &self,
        input: &ScopeInput<'_>,
        request: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        self.validate_scope_roots(&input.physical(), request)?;
        input.validate(&self.installation, &self.brand, request)
    }
    pub(crate) fn validate_physical_scope(
        &self,
        input: &PhysicalScopeInput<'_>,
        request: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        self.validate_scope_roots(input, request)?;
        input.validate(&self.installation, &self.brand, request)
    }
    fn validate_scope_roots(
        &self,
        input: &PhysicalScopeInput<'_>,
        request: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        request.check().map_err(|_| AuthError::Request)?;
        if !self
            .realm_record
            .matches_scope_bounded(
                input.installation.live,
                &ScopeContext::from_snapshot(request.snapshot()),
            )
            .map_err(|_| AuthError::Denied)?
        {
            return Err(AuthError::Denied);
        }
        // Physical trust is independent of current activity and module permissions.
        let roots = self.trusted_roots.read().map_err(|_| AuthError::State)?;
        if input
            .project
            .is_some_and(|p| !roots.contains(p.scope.live.owner_root()))
        {
            return Err(AuthError::Denied);
        }
        for provider in input.providers {
            request.check().map_err(|_| AuthError::Request)?;
            let owner_root_ok = match provider.live.owner() {
                StoreOwner::User { .. } => false,
                StoreOwner::Installation { .. } => {
                    provider.live.owner_root() == self.realm.owner_root()
                }
                StoreOwner::Project { .. } => input
                    .project
                    .is_some_and(|p| provider.live.owner_root() == p.scope.live.owner_root()),
            };
            if !owner_root_ok
                || provider
                    .live
                    .allowed_roots()
                    .any(|root| !roots.contains(root))
            {
                return Err(AuthError::Denied);
            }
        }
        Ok(())
    }
    fn session_id(&self) -> Result<Id, AuthError> {
        for _ in 0..8 {
            let mut id = [0; 32];
            random_nonzero(self.entropy.as_ref(), &mut id)?;
            if !self.lock()?.sessions.contains_key(&id) {
                return Ok(id);
            }
        }
        Err(AuthError::Entropy)
    }
    fn run_id(&self) -> Result<Id, AuthError> {
        for _ in 0..8 {
            let mut id = [0; 32];
            random_nonzero(self.entropy.as_ref(), &mut id)?;
            let state = self.lock()?;
            if !state.runs.contains_key(&id) && !state.scopes.contains_key(&id) {
                return Ok(id);
            }
        }
        Err(AuthError::Entropy)
    }
    fn nonce(&self) -> Result<(RoleBearer, Id), AuthError> {
        for _ in 0..8 {
            let mut nonce = Zeroizing::new([0u8; 32]);
            self.entropy.fill(nonce.as_mut())?;
            let mut encoded = Zeroizing::new([0u8; 47]);
            encoded[..4].copy_from_slice(b"gr1_");
            URL_SAFE_NO_PAD
                .encode_slice(nonce.as_slice(), &mut encoded[4..])
                .map_err(|_| AuthError::State)?;
            let hash: Id = Sha256::digest(encoded.as_slice()).into();
            if !self.lock()?.tokens.contains_key(&hash) {
                let text = std::str::from_utf8(encoded.as_slice()).map_err(|_| AuthError::State)?;
                return Ok((
                    RoleBearer {
                        secret: SecretString::from(text.to_owned()),
                    },
                    hash,
                ));
            }
        }
        Err(AuthError::Entropy)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn issue(
        &self,
        parent: &AdminParent<'_>,
        run: &RunHandle,
        reference: &RoleCredentialReference,
        permissions: &[Permission],
        scope: &ScopeInput<'_>,
    ) -> Result<RoleBearer, AuthError> {
        parent.check()?;
        if run.epoch != self.epoch {
            return Err(AuthError::Denied);
        }
        self.validate_scope(scope, parent.request)?;
        let facts = self.checked_run_scope(run.id, scope, parent.request)?;
        let nonce = self.nonce()?;
        let permissions = canonical_permissions(permissions, self.limits.max_permissions)?;
        parent.caller.source.check_physical(self, parent.request)?;
        let mut state = self.lock()?;
        self.parent_locked(parent, &state)?;
        self.run_parent(&state, run.id, parent)?;
        prune(&mut state, self.clock.now());
        Self::same_run_scope(&state, run.id, &facts)?;
        let run_data = state.runs.get(&run.id).ok_or(AuthError::Denied)?;
        let profile = profile_index(&state, reference)?;
        if !subset(
            &permissions,
            &self
                .origin_policy(parent.caller.source.origin())
                .administrative,
        ) {
            return Err(AuthError::Denied);
        }
        let expiry = Expiry::after(
            self.ttl
                .ttl
                .min(run_data.expiry.remaining(self.clock.now())?)
                .min(
                    self.session_expiry(&state, parent.session)?
                        .remaining(self.clock.now())?,
                ),
            self.clock.now(),
        )?;
        self.insert_credential(
            &mut state,
            run.id,
            profile,
            permissions,
            expiry,
            None,
            parent.request,
            nonce,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn insert_credential(
        &self,
        state: &mut State,
        run: Id,
        profile: usize,
        permissions: Vec<Permission>,
        expiry: Expiry,
        parent: Option<CredentialId>,
        request: &RequestContext<'_>,
        nonce: (RoleBearer, Id),
    ) -> Result<RoleBearer, AuthError> {
        if permissions.len() > self.limits.max_permissions {
            return Err(AuthError::Limit);
        }
        let definition = &state.profiles[profile].definition;
        let run_data = state.runs.get(&run).ok_or(AuthError::Denied)?;
        if definition.binding
            != state
                .scopes
                .get(&run_data.scope)
                .ok_or(AuthError::State)?
                .binding
            || !subset(&permissions, &definition.permissions)
            || permissions
                .iter()
                .any(|p| !p.allowed(definition.reference.role()))
        {
            return Err(AuthError::Denied);
        }
        let sequence = run_data.next.checked_add(1).ok_or(AuthError::State)?;
        let role = state.profiles[profile].definition.reference.role();
        let profile_generation = state.profiles[profile].generation;
        let mut actor_hash = Sha256::new();
        actor_hash.update(b"graphit-run-actor-v1\0");
        actor_hash.update(self.epoch);
        actor_hash.update(run);
        actor_hash.update(role.as_str().as_bytes());
        let actor = Actor(actor_hash.finalize().into());
        let (bearer, hash) = nonce;
        if state.tokens.contains_key(&hash) {
            return Err(AuthError::Entropy);
        }
        request.check().map_err(|_| AuthError::Request)?;
        self.session_expiry(state, run_data.session)?
            .remaining(self.clock.now())?;
        expiry.remaining(self.clock.now())?;
        let id = CredentialId { run, sequence };
        state.credentials.insert(
            id,
            Credential {
                hash,
                profile,
                profile_generation,
                parent,
                permissions,
                expiry,
                actor,
            },
        );
        state.tokens.insert(hash, id);
        state.runs.get_mut(&run).ok_or(AuthError::State)?.next = sequence;
        Ok(bearer)
    }
    pub fn verify<'a>(
        &'a self,
        raw: &str,
        reference: &RoleCredentialReference,
        scope: ScopeInput<'a>,
        request: &'a RequestContext<'a>,
    ) -> Result<VerifiedRoleGrant<'a>, AuthError> {
        request.check().map_err(|_| AuthError::Request)?;
        let hash = token_hash(raw)?;
        let (id, origin) = {
            let state = self.lock()?;
            let id = *state.tokens.get(&hash).ok_or(AuthError::Denied)?;
            let credential = state.credentials.get(&id).ok_or(AuthError::Denied)?;
            if state.profiles[credential.profile].definition.reference != *reference {
                return Err(AuthError::Denied);
            }
            let run = state.runs.get(&id.run).ok_or(AuthError::Denied)?;
            (id, run.origin)
        };
        let source = self.capture_source(origin, None, request)?;
        let grant = VerifiedRoleGrant {
            issuer: self,
            id,
            hash,
            scope,
            request,
            source,
        };
        drop(grant.check()?);
        Ok(grant)
    }
    fn check_credential(
        &self,
        state: &State,
        id: CredentialId,
        request: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        let now = self.clock.now();

        let mut current = Some(id);
        for _ in 0..=state.credentials.len() {
            request.check().map_err(|_| AuthError::Request)?;
            let Some(id) = current else {
                return Ok(());
            };
            let credential = state.credentials.get(&id).ok_or(AuthError::Denied)?;
            credential.expiry.remaining(now)?;
            let run = state.runs.get(&id.run).ok_or(AuthError::Denied)?;
            run.expiry.remaining(now)?;
            self.session_expiry(state, run.session)?.remaining(now)?;
            if run.generation != source_generation(state, run.origin)
                || state
                    .profiles
                    .get(credential.profile)
                    .is_none_or(|p| p.generation != credential.profile_generation)
            {
                return Err(AuthError::Denied);
            }
            current = credential.parent;
        }
        Err(AuthError::Denied)
    }
    pub fn refresh(
        &self,
        parent: &AdminParent<'_>,
        bearer: &RoleBearer,
        scope: &ScopeInput<'_>,
    ) -> Result<RoleBearer, AuthError> {
        parent.check()?;
        self.validate_scope(scope, parent.request)?;
        let hash = token_hash(bearer.expose_for_transport())?;
        let observed_id = *self.lock()?.tokens.get(&hash).ok_or(AuthError::Denied)?;
        let facts = self.checked_run_scope(observed_id.run, scope, parent.request)?;
        let (new_bearer, new_hash) = self.nonce()?;
        parent.caller.source.check_physical(self, parent.request)?;
        let mut state = self.lock()?;
        self.parent_locked(parent, &state)?;
        let id = *state.tokens.get(&hash).ok_or(AuthError::Denied)?;
        self.check_credential(&state, id, parent.request)?;
        let run = state.runs.get(&id.run).ok_or(AuthError::Denied)?;
        self.run_parent(&state, id.run, parent)?;
        Self::same_run_scope(&state, id.run, &facts)?;
        let credential = state.credentials.get(&id).ok_or(AuthError::Denied)?;
        if !subset(
            &credential.permissions,
            &self
                .origin_policy(parent.caller.source.origin())
                .administrative,
        ) {
            return Err(AuthError::Denied);
        }
        let mut duration = self
            .ttl
            .ttl
            .min(run.expiry.remaining(self.clock.now())?)
            .min(
                self.session_expiry(&state, parent.session)?
                    .remaining(self.clock.now())?,
            );
        if let Some(ancestor) = credential.parent {
            duration = duration.min(
                state
                    .credentials
                    .get(&ancestor)
                    .ok_or(AuthError::Denied)?
                    .expiry
                    .remaining(self.clock.now())?,
            );
        }
        let expiry = Expiry::after(duration, self.clock.now())?;
        if state.tokens.contains_key(&new_hash) {
            return Err(AuthError::Entropy);
        }
        self.parent_locked(parent, &state)?;
        self.check_credential(&state, id, parent.request)?;
        expiry.remaining(self.clock.now())?;
        let old_hash = state.credentials.get(&id).ok_or(AuthError::State)?.hash;
        state.tokens.remove(&old_hash);
        state.tokens.insert(new_hash, id);
        let credential = state.credentials.get_mut(&id).ok_or(AuthError::State)?;
        credential.hash = new_hash;
        credential.expiry = expiry;
        Ok(new_bearer)
    }
    pub fn delegate(
        &self,
        parent: &VerifiedRoleGrant<'_>,
        reference: &RoleCredentialReference,
        permissions: &[Permission],
        lifetime: Duration,
    ) -> Result<IssuedRole, AuthError> {
        parent.authorize(Permission::Delegate)?;
        if !std::ptr::eq(self, parent.issuer) {
            return Err(AuthError::Denied);
        }
        let id = self.run_id()?;
        let (bearer, hash) = self.nonce()?;
        let permissions = canonical_permissions(permissions, self.limits.max_permissions)?;
        let mut state = parent.check()?;
        prune(&mut state, self.clock.now());
        let ancestor = state.credentials.get(&parent.id).ok_or(AuthError::Denied)?;
        if !subset(&permissions, &ancestor.permissions) {
            return Err(AuthError::Denied);
        }
        let profile = profile_index(&state, reference)?;
        let parent_run = state.runs.get(&parent.id.run).ok_or(AuthError::Denied)?;
        let scope = parent_run.scope;
        let parent_session = parent_run.session;
        let duration = lifetime
            .min(ancestor.expiry.remaining(self.clock.now())?)
            .min(parent_run.expiry.remaining(self.clock.now())?)
            .min(self.ttl.ttl);
        let expiry = Expiry::after(duration, self.clock.now())?;
        let definition = &state.profiles[profile].definition;
        if definition.binding != state.scopes.get(&scope).ok_or(AuthError::State)?.binding
            || permissions.len() > self.limits.max_permissions
            || !subset(&permissions, &definition.permissions)
            || permissions
                .iter()
                .any(|p| !p.allowed(definition.reference.role()))
        {
            return Err(AuthError::Denied);
        }
        let origin = parent.source.origin();
        let generation = parent.source.generation();
        if state.runs.contains_key(&id)
            || state.scopes.contains_key(&id)
            || state.tokens.contains_key(&hash)
        {
            return Err(AuthError::Entropy);
        }
        let role = state.profiles[profile].definition.reference.role();
        let profile_generation = state.profiles[profile].generation;
        let mut actor_hash = Sha256::new();
        actor_hash.update(b"graphit-run-actor-v1\0");
        actor_hash.update(self.epoch);
        actor_hash.update(id);
        actor_hash.update(role.as_str().as_bytes());
        let credential_id = CredentialId {
            run: id,
            sequence: 1,
        };
        parent.check_locked(&state)?;
        expiry.remaining(self.clock.now())?;
        state.runs.insert(
            id,
            Run {
                origin,
                session: parent_session,
                scope,
                expiry,
                generation,
                next: 1,
            },
        );
        state.credentials.insert(
            credential_id,
            Credential {
                hash,
                profile,
                profile_generation,
                parent: Some(parent.id),
                permissions,
                expiry,
                actor: Actor(actor_hash.finalize().into()),
            },
        );
        state.tokens.insert(hash, credential_id);
        Ok(IssuedRole {
            run: RunHandle {
                epoch: self.epoch,
                id,
            },
            bearer,
        })
    }
    pub fn revoke(&self, parent: &AdminParent<'_>, bearer: &RoleBearer) -> Result<(), AuthError> {
        parent.check()?;
        let hash = token_hash(bearer.expose_for_transport())?;
        let mut state = self.lock()?;
        self.parent_locked(parent, &state)?;
        let id = *state.tokens.get(&hash).ok_or(AuthError::Denied)?;
        self.run_parent(&state, id.run, parent)?;
        remove_credential(&mut state, id);
        prune(&mut state, self.clock.now());
        Ok(())
    }
    pub fn close_run(&self, parent: &AdminParent<'_>, run: &RunHandle) -> Result<(), AuthError> {
        parent.check()?;
        if run.epoch != self.epoch {
            return Err(AuthError::Denied);
        }
        let mut state = self.lock()?;
        self.parent_locked(parent, &state)?;
        self.run_parent(&state, run.id, parent)?;
        if state.runs.remove(&run.id).is_none() {
            return Err(AuthError::Denied);
        }
        prune(&mut state, self.clock.now());
        Ok(())
    }
}
impl AuthenticatedCaller<'_> {
    /// Borrow the request this authentication proof was issued for.
    pub fn request(&self) -> &RequestContext<'_> {
        self.request
    }

    pub fn check(&self) -> Result<(), AuthError> {
        self.source.check_physical(self.issuer, self.request)?;
        self.issuer.caller_locked(self, &*self.issuer.lock()?)
    }
    pub fn authorize(&self, permission: Permission) -> Result<Obligation, AuthError> {
        self.check()?;
        permission.code()?;
        if !self
            .issuer
            .origin_policy(self.source.origin())
            .ordinary
            .contains(&permission)
        {
            return Err(AuthError::Denied);
        }
        Ok(obligation(permission))
    }
    pub fn actor(&self) -> Result<Actor, AuthError> {
        self.check()?;
        let mut hash = Sha256::new();
        hash.update(b"graphit-source-actor-v1\0");
        hash.update(self.issuer.epoch);
        self.source.update_actor(&mut hash);
        Ok(Actor(hash.finalize().into()))
    }
}
impl<'a> AdminParent<'a> {
    /// Authorize an administrative effect under this live parent and its original request.
    /// It does not add permissions to the caller or mint another grant.
    pub fn authorize(&self, permission: Permission) -> Result<Obligation, AuthError> {
        self.check()?;
        permission.code()?;
        if !self
            .issuer
            .origin_policy(self.caller.source.origin())
            .administrative
            .contains(&permission)
        {
            return Err(AuthError::Denied);
        }
        Ok(obligation(permission))
    }

    pub(crate) fn profile_request(
        &self,
        issuer: &RoleIssuer,
    ) -> Result<&'a RequestContext<'a>, AuthError> {
        if !std::ptr::eq(self.issuer, issuer) {
            return Err(AuthError::Denied);
        }
        self.check()?;
        let state = issuer.lock()?;
        issuer.parent_locked(self, &state)?;
        if !issuer
            .origin_policy(self.caller.source.origin())
            .administrative
            .contains(&Permission::ManageAuthProfiles)
        {
            return Err(AuthError::Denied);
        }
        Ok(self.request)
    }

    pub fn check(&self) -> Result<(), AuthError> {
        self.caller.check()?;
        self.issuer.parent_locked(self, &*self.issuer.lock()?)
    }
}
impl<'a> VerifiedRoleGrant<'a> {
    pub(crate) fn profile_request(
        &self,
        issuer: &RoleIssuer,
    ) -> Result<&'a RequestContext<'a>, AuthError> {
        if !std::ptr::eq(self.issuer, issuer) {
            return Err(AuthError::Denied);
        }
        if self.authorize(Permission::ManageAuthProfiles)? != Obligation::Unconditional {
            return Err(AuthError::Denied);
        }
        Ok(self.request)
    }

    fn check_locked(&self, state: &State) -> Result<(), AuthError> {
        self.source.check_locked(state)?;
        self.issuer.check_credential(state, self.id, self.request)?;
        if state
            .credentials
            .get(&self.id)
            .ok_or(AuthError::Denied)?
            .hash
            != self.hash
        {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
    fn check(&self) -> Result<MutexGuard<'_, State>, AuthError> {
        self.source.check_physical(self.issuer, self.request)?;
        self.issuer.validate_scope(&self.scope, self.request)?;
        let facts = self
            .issuer
            .checked_run_scope(self.id.run, &self.scope, self.request)?;
        #[cfg(test)]
        tests::before_final_source();
        self.source.check_physical(self.issuer, self.request)?;
        let state = self.issuer.lock()?;
        self.check_locked(&state)?;
        RoleIssuer::same_run_scope(&state, self.id.run, &facts)?;
        Ok(state)
    }
    pub fn authorize(&self, permission: Permission) -> Result<Obligation, AuthError> {
        let state = self.check()?;
        permission.code()?;
        if !state
            .credentials
            .get(&self.id)
            .ok_or(AuthError::Denied)?
            .permissions
            .contains(&permission)
        {
            return Err(AuthError::Denied);
        }
        Ok(obligation(permission))
    }
    pub fn actor(&self) -> Result<Actor, AuthError> {
        let state = self.check()?;
        Ok(state
            .credentials
            .get(&self.id)
            .ok_or(AuthError::Denied)?
            .actor
            .clone())
    }
    pub fn refresh_due(&self) -> Result<bool, AuthError> {
        let state = self.check()?;
        Ok(state
            .credentials
            .get(&self.id)
            .ok_or(AuthError::Denied)?
            .expiry
            .remaining(self.issuer.clock.now())?
            <= self.issuer.ttl.refresh_margin)
    }
}
fn profile_index(state: &State, reference: &RoleCredentialReference) -> Result<usize, AuthError> {
    state
        .profiles
        .iter()
        .position(|p| p.definition.reference == *reference)
        .ok_or(AuthError::Denied)
}
fn random_nonzero(entropy: &dyn Entropy, output: &mut Id) -> Result<(), AuthError> {
    for _ in 0..8 {
        entropy.fill(output)?;
        if output.iter().any(|b| *b != 0) {
            return Ok(());
        }
    }
    Err(AuthError::Entropy)
}
fn token_hash(raw: &str) -> Result<Id, AuthError> {
    if raw.len() != 47 || !raw.starts_with("gr1_") {
        return Err(AuthError::Denied);
    }
    let mut nonce = Zeroizing::new([0u8; 32]);
    if URL_SAFE_NO_PAD
        .decode_slice(&raw.as_bytes()[4..], nonce.as_mut())
        .ok()
        != Some(32)
    {
        return Err(AuthError::Denied);
    }
    let mut canonical = Zeroizing::new([0u8; 43]);
    URL_SAFE_NO_PAD
        .encode_slice(nonce.as_slice(), canonical.as_mut())
        .map_err(|_| AuthError::Denied)?;
    if !bool::from(canonical.as_slice().ct_eq(&raw.as_bytes()[4..])) {
        return Err(AuthError::Denied);
    }
    Ok(Sha256::digest(raw.as_bytes()).into())
}
fn remove_credential(state: &mut State, id: CredentialId) {
    if let Some(credential) = state.credentials.remove(&id) {
        state.tokens.remove(&credential.hash);
    }
}
fn prune(state: &mut State, now: Moment) {
    state.sessions.retain(|_, session| {
        session.generation
            == match session.origin {
                Origin::Runtime => state.runtime_generation,
                Origin::Local => state.local.generation,
            }
            && session.expiry.remaining(now).is_ok()
    });
    state.runs.retain(|_, run| {
        run.generation
            == match run.origin {
                Origin::Runtime => state.runtime_generation,
                Origin::Local => state.local.generation,
            }
            && state.sessions.contains_key(&run.session)
            && run.expiry.remaining(now).is_ok()
    });
    for (_, credential) in state.credentials.extract_if(.., |id, c| {
        !state.runs.contains_key(&id.run)
            || c.expiry.remaining(now).is_err()
            || state.profiles[c.profile].generation != c.profile_generation
    }) {
        state.tokens.remove(&credential.hash);
    }
    // Each pass removes an orphan; at most the admitted credential count passes.
    // No auxiliary allocation or active-credential eviction is needed.
    while let Some(id) = state
        .credentials
        .iter()
        .find(|(_, c)| {
            c.parent
                .is_some_and(|p| !state.credentials.contains_key(&p))
        })
        .map(|(id, _)| *id)
    {
        remove_credential(state, id);
    }
    state
        .scopes
        .retain(|id, _| state.runs.values().any(|run| run.scope == *id));
}

#[cfg(test)]
pub(crate) mod tests;

fn obligation(permission: Permission) -> Obligation {
    match permission {
        Permission::WriteOwnTask => Obligation::OwnActor,
        Permission::WriteFencedMemory => Obligation::MemoryFenceRequired,
        _ => Obligation::Unconditional,
    }
}
fn runtime_error(error: RuntimeKeyError) -> AuthError {
    match error {
        RuntimeKeyError::Limit => AuthError::Limit,
        RuntimeKeyError::Cancelled | RuntimeKeyError::Deadline => AuthError::Request,
        RuntimeKeyError::Busy => AuthError::State,
        RuntimeKeyError::Io => AuthError::Io,
        _ => AuthError::Denied,
    }
}
