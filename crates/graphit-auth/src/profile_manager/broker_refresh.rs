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
    store::AuthSnapshotRecheck,
};
use secrecy::SecretString;
use sha2::{Digest, Sha256};

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
            BrokerRefreshErrorKind::OAuth(_) => f.write_str("native Broker refresh failed"),
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

#[derive(Eq, PartialEq)]
struct RefreshEpoch([u8; 32]);
/// One issuer's rotating-token gate, shared by every Native profile manager.
#[derive(Default)]
pub(crate) struct NativeRefreshState {
    uncertain: Option<(RefreshEpoch, BrokerRefreshError)>,
}

struct ManagedRefreshGate<'s, 'a> {
    manager: &'s NativeProfileManager<'a>,
    authority: &'s ProfileManagerAuthority<'a>,
    request: &'a RequestContext<'a>,
    service: &'a NativeOAuth<'a>,
    endpoint: &'s str,
    resource: Option<&'s str>,
    provider_revision: u64,
    issuer: &'s str,
    observation: AuthSnapshotRecheck<'s>,
}
impl ManagedRefreshGate<'_, '_> {
    fn check(
        &mut self,
        service: &NativeOAuth<'_>,
        context: BrokerRequestContext<'_>,
    ) -> Result<(), BrokerRefreshErrorKind> {
        // The protocol supplies this context; the manager never substitutes it.
        self.request
            .snapshot()
            .check()
            .map_err(|_| manager(AuthError::Request))?;
        if !std::ptr::eq(self.service, service)
            || !service.matches_provider(self.endpoint, self.resource, self.provider_revision)
        {
            return Err(BrokerRefreshErrorKind::ServiceMismatch);
        }
        // Check the actual original request, not equivalent deadline/cancel values.
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
impl NativeFlowGate for ManagedRefreshGate<'_, '_> {
    type Error = BrokerRefreshErrorKind;
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
        if metadata.issuer() != self.issuer {
            return Err(BrokerRefreshErrorKind::IdentityMismatch);
        }
        self.check(service, context)
    }
}

impl NativeProfileManager<'_> {
    /// Refreshes only the captured active Broker profile under its original authority.
    /// Remote rotation and the returned local publication are separate effects.
    /// There is no retry, reload or postpublication authority check.
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
        mut managed: ManagedAuthSnapshot<'a>,
        oauth: &'a NativeOAuth<'a>,
        now: i64,
    ) -> Result<AuthDocumentPublication, BrokerRefreshError> {
        let before = |kind| BrokerRefreshError::local(kind, RemoteEffect::NotSent);
        if !std::ptr::eq(self, managed.manager) {
            return Err(before(manager(ProfileManagerError::WrongManager)));
        }
        managed
            .authority
            .authorize(self.issuer)
            .map_err(|e| before(manager(e)))?;
        let request = managed.request;
        let epoch = {
            let (provider, selected) = managed
                .snapshot
                .document()
                .active(request)
                .map_err(|e| before(profile(e)))?;
            let credentials = selected
                .broker_credentials()
                .ok_or_else(|| before(profile(ProfileError::InvalidDocument)))?;
            let mut digest = Sha256::new();
            for value in [
                provider.name(),
                selected.name(),
                credentials.issuer,
                credentials.subject,
            ] {
                let length =
                    u64::try_from(value.len()).map_err(|_| before(profile(ProfileError::Limit)))?;
                digest.update(length.to_le_bytes());
                digest.update(value.as_bytes());
            }
            digest.update(provider.revision().to_le_bytes());
            digest.update(selected.token_revision().to_le_bytes());
            RefreshEpoch(digest.finalize().into())
        };
        let deadline = async {
            match request.snapshot().deadline() {
                Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
                None => std::future::pending::<()>().await,
            }
        };
        let mut state = tokio::select! {
            biased;
            () = request.cancellation().cancelled() => return Err(before(profile(ProfileError::Cancelled))),
            () = deadline => return Err(before(profile(ProfileError::Deadline))),
            state = self.issuer.native_profile_refresh().lock() => state,
        };
        // A competing owner may have rotated while this snapshot waited. Never
        // send the old rotating token; return a typed, definitely-unsent conflict.
        managed
            .authority
            .authorize(self.issuer)
            .map_err(|e| before(manager(e)))?;
        self.issuer
            .profile_store()
            .revalidate(&mut managed.snapshot, request)
            .map_err(|e| before(manager(e)))?;
        if let Some((previous, error)) = &state.uncertain
            && previous == &epoch
        {
            return Err(*error);
        }
        // A future dropped after remote POST has no returned effect proof. The
        // conservative marker belongs only to this exact credential epoch.
        state.uncertain = Some((
            epoch,
            BrokerRefreshError::local(
                BrokerRefreshErrorKind::OAuth(NativeErrorKind::InvalidRotation),
                RemoteEffect::MayHaveApplied,
            ),
        ));
        let result = self.refresh_broker_locked(managed, oauth, now).await;
        match &result {
            Err(error) if error.remote_effect() == RemoteEffect::MayHaveApplied => {
                if let Some((_, previous)) = &mut state.uncertain { *previous = *error; }
            }
            Err(_) => state.uncertain = None,
            Ok(publication) if publication.effect() == graphit_core::auth_document::publication::AuthPublicationEffect::Applied
                || publication.effect() == graphit_core::auth_document::publication::AuthPublicationEffect::Unchanged => {
                state.uncertain = None;
            }
            Ok(publication) => {
                let failure = publication.failure().unwrap_or(graphit_core::auth_document::AuthDocumentError::Conflict);
                if let Some((_, previous)) = &mut state.uncertain {
                    *previous = BrokerRefreshError::local(manager(AuthStoreError::Core(failure)), RemoteEffect::MayHaveApplied);
                }
            }
        }
        result
    }

    async fn refresh_broker_locked<'a>(
        &'a self,
        managed: ManagedAuthSnapshot<'a>,
        oauth: &'a NativeOAuth<'a>,
        now: i64,
    ) -> Result<AuthDocumentPublication, BrokerRefreshError> {
        let before = |kind| BrokerRefreshError::local(kind, RemoteEffect::NotSent);
        let after = |kind| BrokerRefreshError::local(kind, RemoteEffect::MayHaveApplied);
        if !std::ptr::eq(self, managed.manager) {
            return Err(before(manager(ProfileManagerError::WrongManager)));
        }
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
        let (name, expected, tokens, update) = {
            let (observation, document) = store.snapshot_parts(&mut snapshot);
            let (provider, selected) = document.active(request).map_err(|e| before(profile(e)))?;
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
            let expected = TokenExpectation {
                document_revision: document.revision(),
                provider_revision: provider.revision(),
                profile_revision: selected.revision(),
                token_revision: selected.token_revision(),
            };
            if [
                expected.document_revision,
                expected.profile_revision,
                expected.token_revision,
            ]
            .contains(&u64::MAX)
            {
                return Err(before(profile(ProfileError::Limit)));
            }
            let mut copies = CopyBudget {
                limits: store.profile_limits(),
                used: 0,
                request,
            };
            let name = copies.text(selected.name()).map_err(before)?;
            let mut gate = ManagedRefreshGate {
                manager: self,
                authority: &authority,
                request,
                service: oauth,
                endpoint: &config.endpoint,
                resource: config.mcp_resource.as_deref(),
                provider_revision: expected.provider_revision,
                issuer,
                observation,
            };
            let tokens = oauth
                .refresh_guarded(old_refresh, subject, context, &mut gate)
                .await
                .map_err(|error| match error {
                    GuardedNativeError::Native(error) => BrokerRefreshError::native(error),
                    GuardedNativeError::Admission(kind) => before(kind),
                })?;
            let identity = tokens
                .identity(context)
                .map_err(BrokerRefreshError::native)?;
            if identity.issuer() != issuer
                || identity.subject() != subject
                || identity.provider_revision() != expected.provider_revision
            {
                return Err(after(BrokerRefreshErrorKind::IdentityMismatch));
            }
            let update = token_update(&tokens, context, &mut copies).map_err(after)?;
            (name, expected, tokens, update)
        };
        let prepared = store
            .prepare(snapshot, request, |doc, ctx| {
                doc.rotate_tokens(&name, update, expected, now, ctx)
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
        // This final retained-token/context check has no file I/O or allocation.
        tokens
            .check(oauth, context)
            .map_err(BrokerRefreshError::native)?;
        store.commit(prepared).map_err(|e| after(manager(e)))
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
