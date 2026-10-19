mod device_login;
pub use device_login::{ManagedDeviceBrokerLogin, ReceivedDeviceBrokerLogin};
mod interactive_login;
pub use interactive_login::{
    InteractiveLoginConfig, InteractiveLoginError, InteractiveLoginLimits,
    InteractiveTransportError, ManagedInteractiveBrokerLogin, ReceivedInteractiveBrokerLogin,
};

use super::*;
use crate::{
    broker::{
        BrokerMetadata, BrokerRequestContext,
        oauth::{
            FinalNativeTokens, GuardedNativeError, NativeError, NativeErrorKind, NativeFlowGate,
            NativeOAuth, PendingLogin, RemoteEffect,
        },
    },
    profile::{OidcTokens, ProfileLimits, ProviderKind, Secret, valid_name},
    store::AuthSnapshotRecheck,
};
use secrecy::SecretString;

/// Explicit borrowed selectors. Stored data is never an authorization grant.
pub struct BrokerLoginSelection<'a> {
    pub profile: &'a str,
    pub provider: &'a str,
    pub s3_disabled: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrokerLoginErrorKind {
    Manager(ProfileManagerError),
    OAuth(NativeErrorKind),
    ServiceMismatch,
    IdentityMismatch,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrokerLoginError {
    kind: BrokerLoginErrorKind,
    remote: RemoteEffect,
}
impl BrokerLoginError {
    pub fn kind(self) -> BrokerLoginErrorKind {
        self.kind
    }
    pub fn remote_effect(self) -> RemoteEffect {
        self.remote
    }
    fn local(kind: BrokerLoginErrorKind, remote: RemoteEffect) -> Self {
        Self { kind, remote }
    }
    fn native(error: NativeError) -> Self {
        Self {
            kind: BrokerLoginErrorKind::OAuth(error.kind()),
            remote: error.effect(),
        }
    }
}
impl fmt::Display for BrokerLoginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            BrokerLoginErrorKind::Manager(error) => error.fmt(f),
            BrokerLoginErrorKind::OAuth(kind) => write!(f, "native Broker login failed: {kind:?}"),
            BrokerLoginErrorKind::ServiceMismatch => f.write_str("Broker login service mismatch"),
            BrokerLoginErrorKind::IdentityMismatch => f.write_str("Broker login identity mismatch"),
        }
    }
}
impl std::error::Error for BrokerLoginError {}
fn manager(error: impl Into<ProfileManagerError>) -> BrokerLoginErrorKind {
    BrokerLoginErrorKind::Manager(error.into())
}
fn profile(error: ProfileError) -> BrokerLoginErrorKind {
    manager(AuthStoreError::Profile(error))
}
fn before(kind: BrokerLoginErrorKind) -> BrokerLoginError {
    BrokerLoginError::local(kind, RemoteEffect::NotSent)
}
fn after(kind: BrokerLoginErrorKind) -> BrokerLoginError {
    BrokerLoginError::local(kind, RemoteEffect::MayHaveApplied)
}

/// One actual native pending and its original managed snapshot; neither is transferable authority.
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedBrokerLogin;
/// fn clone(value: ManagedBrokerLogin<'_>) { let _ = value.clone(); }
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedBrokerLogin;
/// fn debug(value: ManagedBrokerLogin<'_>) { println!("{value:?}"); }
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedBrokerLogin;
/// fn serialize<T: serde::Serialize>() {} serialize::<ManagedBrokerLogin<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedBrokerLogin;
/// let _: ManagedBrokerLogin<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedBrokerLogin;
/// fn inspect(p: &ManagedBrokerLogin<'_>) { let _ = &p.pending; }
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedBrokerLogin;
/// fn repeat(value: ManagedBrokerLogin<'_>) { drop(value); drop(value); }
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedBrokerLogin;
/// fn detached<'a>(mut pending: ManagedBrokerLogin<'a>) -> &'a str { pending.authorization_url().unwrap() }
/// ```
pub struct ManagedBrokerLogin<'a> {
    // Field order closes native owned secrets/permit before the captured snapshot.
    pending: PendingLogin<'a>,
    managed: ManagedAuthSnapshot<'a>,
    service: &'a NativeOAuth<'a>,
    selection: BrokerLoginSelection<'a>,
    expected: ProfileExpectation,
    prior_updated_at: Option<i64>,
}
impl ManagedBrokerLogin<'_> {
    /// Sensitive borrowed URL, exposed only after fresh original physical/authority checks.
    pub fn authorization_url(&mut self) -> Result<&str, BrokerLoginError> {
        check_managed(&mut self.managed, self.service, &self.selection).map_err(before)?;
        self.pending
            .authorization_url(BrokerRequestContext::new(self.managed.request.snapshot()))
            .map_err(BrokerLoginError::native)
    }
}
struct ManagedLoginGate<'s, 'a> {
    manager: &'s NativeProfileManager<'a>,
    authority: &'s ProfileManagerAuthority<'a>,
    request: &'a RequestContext<'a>,
    service: &'a NativeOAuth<'a>,
    endpoint: &'s str,
    resource: Option<&'s str>,
    provider_revision: u64,
    observation: AuthSnapshotRecheck<'s>,
}
impl ManagedLoginGate<'_, '_> {
    fn check(
        &mut self,
        service: &NativeOAuth<'_>,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), BrokerLoginErrorKind> {
        self.request
            .snapshot()
            .check()
            .map_err(|_| manager(AuthError::Request))?;
        if !std::ptr::eq(self.service, service)
            || !service.matches_provider(self.endpoint, self.resource, self.provider_revision)
        {
            return Err(BrokerLoginErrorKind::ServiceMismatch);
        }
        if !service.matches_request(self.request.snapshot(), context) {
            return Err(manager(AuthError::Request));
        }
        self.observation
            .recheck(self.request.snapshot())
            .map_err(manager)?;
        self.authority
            .authorize(self.manager.issuer)
            .map_err(manager)?;
        self.request
            .snapshot()
            .check()
            .map_err(|_| manager(AuthError::Request))?;
        Ok(())
    }
}
impl NativeFlowGate for ManagedLoginGate<'_, '_> {
    type Error = BrokerLoginErrorKind;
    fn before_metadata(
        &mut self,
        service: &NativeOAuth<'_>,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), Self::Error> {
        self.check(service, context)
    }
    fn before_post(
        &mut self,
        service: &NativeOAuth<'_>,
        _metadata: &BrokerMetadata,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), Self::Error> {
        self.check(service, context)
    }
}
fn check_managed<'a>(
    managed: &mut ManagedAuthSnapshot<'a>,
    service: &'a NativeOAuth<'a>,
    selection: &BrokerLoginSelection<'_>,
) -> Result<(), BrokerLoginErrorKind> {
    let store = managed.manager.issuer.profile_store();
    let (observation, document) = store.snapshot_parts(&mut managed.snapshot);
    let provider = document
        .provider(selection.provider)
        .ok_or_else(|| profile(ProfileError::Missing))?;
    let ProviderKind::Broker(config) = &provider.config().kind else {
        return Err(profile(ProfileError::InvalidDocument));
    };
    ManagedLoginGate {
        manager: managed.manager,
        authority: &managed.authority,
        request: managed.request,
        service,
        endpoint: &config.endpoint,
        resource: config.mcp_resource.as_deref(),
        provider_revision: provider.revision(),
        observation,
    }
    .check(
        service,
        BrokerRequestContext::new(managed.request.snapshot()),
    )
}
fn preflight(
    managed: &ManagedAuthSnapshot<'_>,
    selection: &BrokerLoginSelection<'_>,
) -> Result<(ProfileExpectation, Option<i64>), BrokerLoginErrorKind> {
    let limits = managed.manager.issuer.profile_store().profile_limits();
    for name in [selection.profile, selection.provider] {
        if name.len() > limits.scalar_bytes {
            return Err(profile(ProfileError::Limit));
        }
        valid_name(name).map_err(profile)?;
    }
    let document = managed.snapshot.document();
    let provider = document
        .provider(selection.provider)
        .ok_or_else(|| profile(ProfileError::Missing))?;
    if !matches!(&provider.config().kind, ProviderKind::Broker(_)) {
        return Err(profile(ProfileError::InvalidDocument));
    }
    document
        .revision()
        .checked_add(2)
        .ok_or_else(|| profile(ProfileError::Limit))?;
    let selected = document.profile(selection.profile);
    if let Some(prior) = selected {
        prior
            .revision()
            .checked_add(1)
            .and_then(|n| {
                n.checked_add(u64::from(document.active_name() != Some(selection.profile)))
            })
            .ok_or_else(|| profile(ProfileError::Limit))?;
        prior
            .token_revision()
            .checked_add(1)
            .ok_or_else(|| profile(ProfileError::Limit))?;
    } else if document.profiles().len() >= limits.profiles.min(limits.collection_entries) {
        return Err(profile(ProfileError::Limit));
    }
    Ok((
        ProfileExpectation {
            document_revision: document.revision(),
            provider_revision: provider.revision(),
            profile_revision: selected.map(ProfileRecord::revision),
        },
        selected.map(ProfileRecord::updated_at),
    ))
}
impl NativeProfileManager<'_> {
    /// Begin one initial login using the captured administrative snapshot.
    /// ```compile_fail
    /// use graphit_auth::{AuthenticatedCaller, broker::oauth::NativeOAuth};
    /// use graphit_auth::profile_manager::{NativeProfileManager, BrokerLoginSelection};
    /// async fn ordinary<'a>(m: &'a NativeProfileManager<'a>, c: AuthenticatedCaller<'a>, oauth: &'a NativeOAuth<'a>, s: BrokerLoginSelection<'a>) {
    ///     m.begin_broker_login(c, oauth, s, "http://127.0.0.1/callback").await;
    /// }
    /// ```
    pub async fn begin_broker_login<'a>(
        &'a self,
        mut managed: ManagedAuthSnapshot<'a>,
        oauth: &'a NativeOAuth<'a>,
        selection: BrokerLoginSelection<'a>,
        redirect: &str,
    ) -> Result<ManagedBrokerLogin<'a>, BrokerLoginError> {
        let (expected, prior_updated_at) = admit_login(self, &mut managed, oauth, &selection)?;
        let context = BrokerRequestContext::new(managed.request.snapshot());
        let pending = oauth
            .begin(redirect, context)
            .await
            .map_err(BrokerLoginError::native)?;
        check_managed(&mut managed, oauth, &selection).map_err(before)?;
        pending
            .check(oauth, context)
            .map_err(BrokerLoginError::native)?;
        Ok(ManagedBrokerLogin {
            pending,
            managed,
            service: oauth,
            selection,
            expected,
            prior_updated_at,
        })
    }
    /// Consume the exact login attempt once and return the exact Core publication.
    /// ```compile_fail
    /// use graphit_auth::profile_manager::{NativeProfileManager, ManagedBrokerLogin};
    /// use graphit_auth::broker::oauth::FinalNativeTokens;
    /// async fn raw<'a>(m: &'a NativeProfileManager<'a>, t: FinalNativeTokens<'a>) { m.finish_broker_login(t, "callback", 0).await; }
    /// ```
    /// ```compile_fail
    /// use graphit_auth::profile_manager::{NativeProfileManager, ManagedBrokerLogin};
    /// use graphit_auth::profile::ProfileExpectation;
    /// async fn cas<'a>(m: &'a NativeProfileManager<'a>, p: ManagedBrokerLogin<'a>, e: ProfileExpectation) {
    ///     m.finish_broker_login(p, "callback", e, 0).await;
    /// }
    /// ```
    /// ```compile_fail
    /// use graphit_auth::profile_manager::{NativeProfileManager, ManagedBrokerLogin};
    /// use graphit_hub::policy::RequestContext;
    /// async fn rebase<'a>(m: &'a NativeProfileManager<'a>, p: ManagedBrokerLogin<'a>, c: &'a RequestContext<'a>) {
    ///     m.finish_broker_login(p, "callback", c, 0).await;
    /// }
    /// ```
    /// ```compile_fail
    /// use graphit_auth::profile_manager::NativeProfileManager;
    /// use graphit_auth::broker::oauth::PendingLogin;
    /// async fn protocol<'a>(m: &'a NativeProfileManager<'a>, p: PendingLogin<'a>) { m.finish_broker_login(p, "callback", 0).await; }
    /// ```
    /// ```compile_fail
    /// use graphit_auth::profile_manager::NativeProfileManager;
    /// use graphit_auth::profile::ProfileData;
    /// async fn dto(m: &NativeProfileManager<'_>, p: ProfileData) { m.finish_broker_login(p, "callback", 0).await; }
    /// ```
    /// ```compile_fail
    /// use graphit_auth::profile_manager::{NativeProfileManager, ManagedBrokerLogin};
    /// use graphit_auth::broker::oauth::NativeOAuth;
    /// async fn service<'a>(m: &'a NativeProfileManager<'a>, p: ManagedBrokerLogin<'a>, replacement: &'a NativeOAuth<'a>) { m.finish_broker_login(p, "callback", replacement, 0).await; }
    /// ```
    /// ```no_run
    /// use graphit_auth::{AdminParent, broker::oauth::NativeOAuth};
    /// use graphit_auth::profile_manager::{NativeProfileManager, ProfileManagerAuthority, BrokerLoginSelection, BrokerLoginError};
    /// use graphit_core::auth_document::publication::AuthDocumentPublication;
    /// async fn login<'a>(manager: &'a NativeProfileManager<'a>, parent: &'a AdminParent<'a>, service: &'a NativeOAuth<'a>, selection: BrokerLoginSelection<'a>, callback: &str) -> Result<AuthDocumentPublication, BrokerLoginError> {
    ///     let snapshot = manager.begin(ProfileManagerAuthority::Admin(parent)).expect("fresh administration");
    ///     let mut pending = manager.begin_broker_login(snapshot, service, selection, "http://127.0.0.1:43891/callback").await?;
    ///     let _authorization_url = pending.authorization_url()?;
    ///     manager.finish_broker_login(pending, callback, 0).await
    /// }
    /// ```
    pub async fn finish_broker_login<'a>(
        &'a self,
        pending: ManagedBrokerLogin<'a>,
        callback: &str,
        now: i64,
    ) -> Result<AuthDocumentPublication, BrokerLoginError> {
        if !std::ptr::eq(self, pending.managed.manager) {
            return Err(before(manager(ProfileManagerError::WrongManager)));
        }
        if now < 0 {
            return Err(before(profile(ProfileError::InvalidDocument)));
        }
        if pending.prior_updated_at.is_some_and(|prior| now < prior) {
            return Err(before(profile(ProfileError::Conflict)));
        }
        let ManagedBrokerLogin {
            pending,
            managed,
            service,
            selection,
            expected,
            ..
        } = pending;
        let ManagedAuthSnapshot {
            authority,
            request,
            mut snapshot,
            ..
        } = managed;
        authority
            .authorize(self.issuer)
            .map_err(|e| before(manager(e)))?;
        let context = BrokerRequestContext::new(request.snapshot());
        let store = self.issuer.profile_store();
        let tokens = {
            let (observation, document) = store.snapshot_parts(&mut snapshot);
            let provider = document
                .provider(selection.provider)
                .ok_or_else(|| before(profile(ProfileError::Missing)))?;
            let ProviderKind::Broker(config) = &provider.config().kind else {
                return Err(before(profile(ProfileError::InvalidDocument)));
            };
            let mut gate = ManagedLoginGate {
                manager: self,
                authority: &authority,
                request,
                service,
                endpoint: &config.endpoint,
                resource: config.mcp_resource.as_deref(),
                provider_revision: provider.revision(),
                observation,
            };
            service
                .exchange_guarded(pending, callback, context, &mut gate)
                .await
                .map_err(|error| match error {
                    GuardedNativeError::Native(error) => BrokerLoginError::native(error),
                    GuardedNativeError::Admission(kind) => before(kind),
                })?
        };
        let data = verified_profile(
            &tokens,
            &selection,
            expected.provider_revision,
            context,
            store.profile_limits(),
            request,
        )
        .map_err(after)?;
        let prepared = store
            .prepare(snapshot, request, |doc, ctx| {
                doc.replace_profile_data(data, expected, now, ctx)?;
                doc.activate(selection.profile, doc.revision(), ctx)
            })
            .map_err(|e| after(manager(e)))?;
        #[cfg(test)]
        crate::issuer::tests::profile_manager::after_prepare();
        tokens
            .check(service, context)
            .map_err(BrokerLoginError::native)?;
        authority
            .authorize(self.issuer)
            .map_err(|e| after(manager(e)))?;
        tokens
            .check(service, context)
            .map_err(BrokerLoginError::native)?;
        store.commit(prepared).map_err(|e| after(manager(e)))
    }
}
fn verified_profile(
    tokens: &FinalNativeTokens<'_>,
    selection: &BrokerLoginSelection<'_>,
    provider_revision: u64,
    context: BrokerRequestContext<'_>,
    limits: ProfileLimits,
    request: &RequestContext<'_>,
) -> Result<ProfileData, BrokerLoginErrorKind> {
    let native = |e: NativeError| BrokerLoginErrorKind::OAuth(e.kind());
    let identity = tokens.identity(context).map_err(native)?;
    if identity.provider_revision() != provider_revision {
        return Err(BrokerLoginErrorKind::IdentityMismatch);
    }
    let claims = identity
        .profile_claims()
        .ok_or(BrokerLoginErrorKind::IdentityMismatch)?;
    let access = tokens.access_token(context).map_err(native)?;
    let refresh = tokens.refresh_token(context).map_err(native)?;
    let id = tokens
        .id_token(context)
        .map_err(native)?
        .ok_or(BrokerLoginErrorKind::IdentityMismatch)?;
    let expiry = tokens.expires_at_micros(context).map_err(native)?;
    // Admit every scalar and the entire aggregate before the first owned copy.
    let mut bytes = claims
        .teams()
        .len()
        .checked_mul(std::mem::size_of::<String>())
        .ok_or_else(|| profile(ProfileError::Limit))?;
    if claims.teams().len() > limits.collection_entries {
        return Err(profile(ProfileError::Limit));
    }
    for text in [
        Some(selection.profile),
        Some(selection.profile),
        Some(selection.provider),
        Some(identity.issuer()),
        Some(identity.subject()),
        Some(access),
        refresh,
        Some(id),
        Some(claims.username()),
        claims.organization(),
    ]
    .into_iter()
    .flatten()
    .chain(claims.teams().iter().map(String::as_str))
    {
        request.check().map_err(|_| manager(AuthError::Request))?;
        if text.len() > limits.scalar_bytes {
            return Err(profile(ProfileError::Limit));
        }
        bytes = bytes
            .checked_add(text.len())
            .ok_or_else(|| profile(ProfileError::Limit))?;
        if bytes > limits.bytes {
            return Err(profile(ProfileError::Limit));
        }
    }
    let text = |value: &str| -> Result<String, BrokerLoginErrorKind> {
        request.check().map_err(|_| manager(AuthError::Request))?;
        let mut owned = String::new();
        owned
            .try_reserve_exact(value.len())
            .map_err(|_| profile(ProfileError::Limit))?;
        owned.push_str(value);
        Ok(owned)
    };
    let secret = |value: &str| text(value).map(|s| Secret::new(SecretString::from(s)));
    let mut teams = Vec::new();
    teams
        .try_reserve_exact(claims.teams().len())
        .map_err(|_| profile(ProfileError::Limit))?;
    for team in claims.teams() {
        teams.push(text(team)?);
    }
    Ok(ProfileData {
        name: text(selection.profile)?,
        provider: text(selection.provider)?,
        username: text(claims.username())?,
        organization: claims.organization().map(text).transpose()?,
        teams,
        credentials: ProfileCredentials::Broker {
            issuer: text(identity.issuer())?,
            subject: text(identity.subject())?,
            tokens: OidcTokens {
                access_token: secret(access)?,
                refresh_token: refresh.map(secret).transpose()?,
                id_token: secret(id)?,
                expires_at: expiry,
            },
            s3_disabled: selection.s3_disabled,
        },
    })
}

fn admit_login<'a>(
    owner: &NativeProfileManager<'a>,
    managed: &mut ManagedAuthSnapshot<'a>,
    oauth: &'a NativeOAuth<'a>,
    selection: &BrokerLoginSelection<'_>,
) -> Result<(ProfileExpectation, Option<i64>), BrokerLoginError> {
    if !std::ptr::eq(owner, managed.manager) {
        return Err(before(manager(ProfileManagerError::WrongManager)));
    }
    managed
        .authority
        .authorize(owner.issuer)
        .map_err(|e| before(manager(e)))?;
    managed
        .request
        .snapshot()
        .check()
        .map_err(|_| before(manager(AuthError::Request)))?;
    let (expected, prior_updated_at) = preflight(managed, selection).map_err(before)?;
    check_managed(managed, oauth, selection).map_err(before)?;
    Ok((expected, prior_updated_at))
}
