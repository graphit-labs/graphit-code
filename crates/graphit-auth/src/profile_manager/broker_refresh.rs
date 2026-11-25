use super::*;
use crate::{
    broker::{
        BrokerMetadata, BrokerRequestContext,
        oauth::{
            FinalNativeTokens, GuardedNativeError, NativeError, NativeErrorKind, NativeFlowGate,
            NativeOAuth, RemoteEffect,
        },
    },
    profile::{
        ProfileClaimsUpdate, ProfileIdentityUpdate, ProfileLimits, ProviderKind, Secret,
        TokenExpectation, TokenUpdate,
    },
};
use secrecy::SecretString;

/// Closed diagnostics. Neither these categories nor stored profile data grant authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrokerRefreshErrorKind {
    Manager(ProfileManagerError),
    OAuth(NativeErrorKind),
    ServiceMismatch,
    IdentityMismatch,
}

/// A local failure after verified remote rotation still means `MayHaveApplied`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrokerRefreshError {
    kind: BrokerRefreshErrorKind,
    remote: RemoteEffect,
}
impl BrokerRefreshError {
    pub fn kind(self) -> BrokerRefreshErrorKind {
        self.kind
    }
    pub fn remote_effect(self) -> RemoteEffect {
        self.remote
    }
    fn native(error: NativeError) -> Self {
        Self {
            kind: BrokerRefreshErrorKind::OAuth(error.kind()),
            remote: error.effect(),
        }
    }
    fn local(kind: BrokerRefreshErrorKind, remote: RemoteEffect) -> Self {
        Self { kind, remote }
    }
}
impl fmt::Display for BrokerRefreshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            BrokerRefreshErrorKind::Manager(error) => error.fmt(f),
            BrokerRefreshErrorKind::OAuth(kind) => {
                write!(
                    f,
                    "native Broker refresh failed: {kind:?} ({:?})",
                    self.remote
                )
            }
            BrokerRefreshErrorKind::ServiceMismatch => {
                f.write_str("Broker refresh service mismatch")
            }
            BrokerRefreshErrorKind::IdentityMismatch => {
                f.write_str("Broker refresh identity mismatch")
            }
        }
    }
}
impl std::error::Error for BrokerRefreshError {}

fn manager(error: impl Into<ProfileManagerError>) -> BrokerRefreshErrorKind {
    BrokerRefreshErrorKind::Manager(error.into())
}
fn profile(error: ProfileError) -> BrokerRefreshErrorKind {
    manager(AuthStoreError::Profile(error))
}

fn before(kind: BrokerRefreshErrorKind) -> BrokerRefreshError {
    BrokerRefreshError::local(kind, RemoteEffect::NotSent)
}
fn after(kind: BrokerRefreshErrorKind) -> BrokerRefreshError {
    BrokerRefreshError::local(kind, RemoteEffect::MayHaveApplied)
}

struct RefreshTarget {
    profile: String,
    provider: String,
    issuer: String,
    subject: String,
    provider_revision: u64,
    token_revision: u64,
}
impl RefreshTarget {
    fn check<'d>(
        &self,
        document: &'d AuthDocument,
        request: &RequestContext<'_>,
        require_active: bool,
    ) -> Result<&'d ProfileRecord, ProfileError> {
        request.check().map_err(|_| {
            if request.cancellation().is_cancelled() {
                ProfileError::Cancelled
            } else {
                ProfileError::Deadline
            }
        })?;
        if require_active && document.active_name() != Some(self.profile.as_str()) {
            return Err(ProfileError::Conflict);
        }
        let selected = document
            .profile(&self.profile)
            .ok_or(ProfileError::Missing)?;
        let provider = document
            .provider(&self.provider)
            .ok_or(ProfileError::Missing)?;
        let credentials = selected
            .broker_credentials()
            .ok_or(ProfileError::InvalidDocument)?;
        if selected.provider() != self.provider
            || provider.revision() != self.provider_revision
            || selected.provider_revision() != self.provider_revision
            || selected.token_revision() != self.token_revision
            || credentials.issuer != self.issuer
            || credentials.subject != self.subject
        {
            return Err(ProfileError::Conflict);
        }
        Ok(selected)
    }
}

struct ManagedRefreshGate<'s, 'a> {
    manager: &'s NativeProfileManager<'a>,
    authority: &'s ProfileManagerAuthority<'a>,
    request: &'a RequestContext<'a>,
    service: &'a NativeOAuth<'a>,
    endpoint: &'s str,
    resource: Option<&'s str>,
    target: &'s RefreshTarget,
    marked: &'s mut bool,
}
impl ManagedRefreshGate<'_, '_> {
    fn check(
        &mut self,
        service: &NativeOAuth<'_>,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), BrokerRefreshError> {
        self.request
            .snapshot()
            .check()
            .map_err(|_| before(manager(AuthError::Request)))?;
        if !std::ptr::eq(self.service, service)
            || !service.matches_provider(
                self.endpoint,
                self.resource,
                self.target.provider_revision,
            )
        {
            return Err(before(BrokerRefreshErrorKind::ServiceMismatch));
        }
        if !service.matches_request(self.request.snapshot(), context) {
            return Err(before(manager(AuthError::Request)));
        }
        self.authority
            .authorize(self.manager.issuer)
            .map_err(|e| before(manager(e)))?;
        let current = self
            .manager
            .issuer
            .profile_store()
            .load(self.request)
            .map_err(|e| before(manager(e)))?;
        let selected = self
            .target
            .check(current.document(), self.request, true)
            .map_err(|e| before(profile(e)))?;
        if selected.refresh_uncertain() && !*self.marked {
            return Err(after(BrokerRefreshErrorKind::OAuth(
                NativeErrorKind::InvalidRotation,
            )));
        }
        Ok(())
    }
}
impl NativeFlowGate for ManagedRefreshGate<'_, '_> {
    type Error = BrokerRefreshError;
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
        metadata: &BrokerMetadata,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), Self::Error> {
        if metadata.issuer() != self.target.issuer {
            return Err(before(BrokerRefreshErrorKind::IdentityMismatch));
        }
        self.check(service, context)?;
        let store = self.manager.issuer.profile_store();
        let prepared = store
            .prepare_latest(self.request, |document, request| {
                let selected = self.target.check(document, request, true)?;
                if selected.refresh_uncertain() {
                    return Err(ProfileError::Conflict);
                }
                document.set_refresh_uncertain(&self.target.profile, true, request)
            })
            .map_err(|e| before(manager(e)))?;
        self.authority
            .authorize(self.manager.issuer)
            .map_err(|e| before(manager(e)))?;
        store
            .commit_latest(prepared)
            .map_err(|e| before(manager(e)))?;
        *self.marked = true;
        Ok(())
    }
}

impl NativeProfileManager<'_> {
    /// Refreshes only the captured active Broker profile under its original authority.
    /// Remote rotation and the returned local publication are separate effects.
    /// The latest document is merged under its short publisher lock; HTTP is never retried.
    ///
    /// ```compile_fail
    /// use graphit_auth::{AuthenticatedCaller, profile_manager::NativeProfileManager};
    /// use graphit_auth::broker::oauth::NativeOAuth;
    /// async fn ordinary<'a>(m: &'a NativeProfileManager<'a>, caller: AuthenticatedCaller<'a>, oauth: &'a NativeOAuth<'a>) {
    ///     m.refresh_broker(caller, oauth, 0).await;
    /// }
    /// ```
    /// ```compile_fail
    /// use graphit_auth::profile_manager::{ManagedAuthSnapshot, NativeProfileManager};
    /// use graphit_auth::broker::oauth::NativeOAuth;
    /// async fn tokens<'a>(m: &'a NativeProfileManager<'a>, s: ManagedAuthSnapshot<'a>, oauth: &'a NativeOAuth<'a>) {
    ///     m.refresh_broker(s, oauth, "caller-refresh-token", 0).await;
    /// }
    /// ```
    /// ```compile_fail
    /// use graphit_auth::profile_manager::{ManagedAuthSnapshot, NativeProfileManager};
    /// use graphit_auth::{broker::oauth::NativeOAuth, profile::TokenExpectation};
    /// async fn cas<'a>(m: &'a NativeProfileManager<'a>, s: ManagedAuthSnapshot<'a>, oauth: &'a NativeOAuth<'a>, expected: TokenExpectation) {
    ///     m.refresh_broker(s, oauth, expected, 0).await;
    /// }
    /// ```
    /// ```compile_fail
    /// use graphit_auth::profile_manager::{ManagedAuthSnapshot, NativeProfileManager};
    /// use graphit_auth::broker::oauth::NativeOAuth;
    /// use graphit_hub::policy::RequestContext;
    /// async fn rebase<'a>(m: &'a NativeProfileManager<'a>, s: ManagedAuthSnapshot<'a>, oauth: &'a NativeOAuth<'a>, ctx: &'a RequestContext<'a>) {
    ///     m.refresh_broker(s, oauth, ctx, 0).await;
    /// }
    /// ```
    /// ```compile_fail
    /// use graphit_auth::broker::oauth::NativeFlowGate;
    /// ```
    /// ```compile_fail
    /// use graphit_auth::store::AuthSnapshotRecheck;
    /// ```
    pub async fn refresh_broker<'a>(
        &'a self,
        managed: ManagedAuthSnapshot<'a>,
        oauth: &'a NativeOAuth<'a>,
        now: i64,
    ) -> Result<graphit_core::auth_document::publication::AuthPublicationEffect, BrokerRefreshError>
    {
        if !std::ptr::eq(self, managed.manager) {
            return Err(before(manager(ProfileManagerError::WrongManager)));
        }
        managed
            .authority
            .authorize(self.issuer)
            .map_err(|e| before(manager(e)))?;
        let request = managed.request;
        let selected = managed
            .snapshot
            .document()
            .active(request)
            .map_err(|e| before(profile(e)))?
            .1;
        let store = self.issuer.profile_store();
        let _profile_lock = loop {
            if let Some(lock) = store
                .try_profile_refresh_lock(selected.name(), request)
                .map_err(|e| before(manager(e)))?
            {
                break lock;
            }
            let deadline = async {
                match request.snapshot().deadline() {
                    Some(at) => tokio::time::sleep_until(at.into()).await,
                    None => std::future::pending::<()>().await,
                }
            };
            tokio::select! {
                biased;
                () = request.cancellation().cancelled() => return Err(before(profile(ProfileError::Cancelled))),
                () = deadline => return Err(before(profile(ProfileError::Deadline))),
                () = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
            }
        };
        self.refresh_broker_locked(managed, oauth, now).await
    }

    async fn refresh_broker_locked<'a>(
        &'a self,
        managed: ManagedAuthSnapshot<'a>,
        oauth: &'a NativeOAuth<'a>,
        now: i64,
    ) -> Result<graphit_core::auth_document::publication::AuthPublicationEffect, BrokerRefreshError>
    {
        if !std::ptr::eq(self, managed.manager) {
            return Err(before(manager(ProfileManagerError::WrongManager)));
        }
        let ManagedAuthSnapshot {
            authority,
            request,
            snapshot,
            ..
        } = managed;
        authority
            .authorize(self.issuer)
            .map_err(|e| before(manager(e)))?;
        let context = BrokerRequestContext::new(request.snapshot());
        let store = self.issuer.profile_store();
        let (provider, selected) = snapshot
            .document()
            .active(request)
            .map_err(|e| before(profile(e)))?;
        let ProviderKind::Broker(config) = &provider.config().kind else {
            return Err(before(profile(ProfileError::InvalidDocument)));
        };
        let ProfileCredentials::Broker {
            issuer,
            subject,
            tokens: prior,
            ..
        } = &selected
            .data()
            .ok_or_else(|| before(profile(ProfileError::InvalidDocument)))?
            .credentials
        else {
            return Err(before(profile(ProfileError::InvalidDocument)));
        };
        let old_refresh = prior
            .refresh_token
            .as_ref()
            .map(Secret::expose)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| before(profile(ProfileError::InvalidDocument)))?;
        if now < 0 {
            return Err(before(profile(ProfileError::InvalidDocument)));
        }
        if now < selected.updated_at() {
            return Err(before(profile(ProfileError::Conflict)));
        }
        if snapshot.document().revision().checked_add(2).is_none()
            || selected.revision().checked_add(2).is_none()
            || selected.token_revision().checked_add(1).is_none()
        {
            return Err(before(profile(ProfileError::Limit)));
        }
        let mut copies = CopyBudget {
            limits: store.profile_limits(),
            used: 0,
            request,
        };
        let target = RefreshTarget {
            profile: copies.text(selected.name()).map_err(before)?,
            provider: copies.text(provider.name()).map_err(before)?,
            issuer: copies.text(issuer).map_err(before)?,
            subject: copies.text(subject).map_err(before)?,
            provider_revision: provider.revision(),
            token_revision: selected.token_revision(),
        };
        let mut marked = false;
        let result = {
            let mut gate = ManagedRefreshGate {
                manager: self,
                authority: &authority,
                request,
                service: oauth,
                endpoint: &config.endpoint,
                resource: config.mcp_resource.as_deref(),
                target: &target,
                marked: &mut marked,
            };
            oauth
                .refresh_guarded(old_refresh, subject, context, &mut gate)
                .await
                .map_err(|error| match error {
                    GuardedNativeError::Native(error) => BrokerRefreshError::native(error),
                    GuardedNativeError::Admission(error) => error,
                })
        };
        let tokens = match result {
            Ok(tokens) => tokens,
            Err(error) => {
                if marked && error.remote_effect() == RemoteEffect::NotSent {
                    authority
                        .authorize(self.issuer)
                        .map_err(|e| before(manager(e)))?;
                    let prepared = store
                        .prepare_latest(request, |document, request| {
                            target.check(document, request, false)?;
                            document.set_refresh_uncertain(&target.profile, false, request)
                        })
                        .map_err(|e| before(manager(e)))?;
                    authority
                        .authorize(self.issuer)
                        .map_err(|e| before(manager(e)))?;
                    store
                        .commit_latest(prepared)
                        .map_err(|e| before(manager(e)))?;
                }
                return Err(error);
            }
        };
        let identity = tokens
            .identity(context)
            .map_err(BrokerRefreshError::native)?;
        if identity.issuer() != issuer
            || identity.subject() != subject
            || identity.provider_revision() != target.provider_revision
        {
            return Err(after(BrokerRefreshErrorKind::IdentityMismatch));
        }
        let update = token_update(&tokens, context, &mut copies).map_err(after)?;
        let prepared = store
            .prepare_latest(request, |document, request| {
                let current = target.check(document, request, false)?;
                if !current.refresh_uncertain() {
                    return Err(ProfileError::Conflict);
                }
                let expected = TokenExpectation {
                    document_revision: document.revision(),
                    provider_revision: target.provider_revision,
                    profile_revision: current.revision(),
                    token_revision: target.token_revision,
                };
                document.rotate_tokens(&target.profile, update, expected, now, request)
            })
            .map_err(|e| after(manager(e)))?;
        #[cfg(test)]
        crate::issuer::tests::profile_manager::after_prepare();
        tokens
            .check(oauth, context)
            .map_err(BrokerRefreshError::native)?;
        authority
            .authorize(self.issuer)
            .map_err(|e| after(manager(e)))?;
        tokens
            .check(oauth, context)
            .map_err(BrokerRefreshError::native)?;
        store.commit_latest(prepared).map_err(|e| after(manager(e)))
    }
}

struct CopyBudget<'a> {
    limits: ProfileLimits,
    used: usize,
    request: &'a RequestContext<'a>,
}
impl CopyBudget<'_> {
    fn text(&mut self, text: &str) -> Result<String, BrokerRefreshErrorKind> {
        self.request
            .check()
            .map_err(|_| manager(AuthError::Request))?;
        let total = self
            .used
            .checked_add(text.len())
            .ok_or_else(|| profile(ProfileError::Limit))?;
        if text.len() > self.limits.scalar_bytes || total > self.limits.bytes {
            return Err(profile(ProfileError::Limit));
        }
        let mut result = String::new();
        result
            .try_reserve_exact(text.len())
            .map_err(|_| profile(ProfileError::Limit))?;
        result.push_str(text);
        self.used = total;
        Ok(result)
    }
    fn secret(&mut self, value: &str) -> Result<Secret, BrokerRefreshErrorKind> {
        Ok(Secret::new(SecretString::from(self.text(value)?)))
    }
}
fn token_update(
    tokens: &FinalNativeTokens<'_>,
    context: BrokerRequestContext<'_>,
    copies: &mut CopyBudget<'_>,
) -> Result<TokenUpdate, BrokerRefreshErrorKind> {
    let native = |error: NativeError| BrokerRefreshErrorKind::OAuth(error.kind());
    let identity = tokens.identity(context).map_err(native)?;
    let access_token = copies.secret(tokens.access_token(context).map_err(native)?)?;
    let refresh_token = copies.secret(
        tokens
            .refresh_token(context)
            .map_err(native)?
            .ok_or(BrokerRefreshErrorKind::IdentityMismatch)?,
    )?;
    let identity = match (
        tokens.id_token(context).map_err(native)?,
        identity.profile_claims(),
    ) {
        (None, None) => ProfileIdentityUpdate::Preserve,
        (Some(id), Some(claims)) => {
            if claims.teams().len() > copies.limits.collection_entries {
                return Err(profile(ProfileError::Limit));
            }
            let id_token = copies.secret(id)?;
            let username = copies.text(claims.username())?;
            let organization = claims
                .organization()
                .map(|text| copies.text(text))
                .transpose()?;
            let mut teams = Vec::new();
            teams
                .try_reserve_exact(claims.teams().len())
                .map_err(|_| profile(ProfileError::Limit))?;
            for team in claims.teams() {
                teams.push(copies.text(team)?);
            }
            ProfileIdentityUpdate::Replace {
                id_token,
                claims: ProfileClaimsUpdate {
                    username,
                    organization,
                    teams,
                },
            }
        }
        _ => return Err(BrokerRefreshErrorKind::IdentityMismatch),
    };
    Ok(TokenUpdate {
        access_token,
        refresh_token,
        identity,
        expires_at: tokens.expires_at_micros(context).map_err(native)?,
    })
}
