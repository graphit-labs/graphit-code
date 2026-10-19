//! Fresh finite authority over the issuer's actual private authentication store.
mod broker_login;
mod broker_refresh;
use crate::{
    AdminParent, AuthError, RoleIssuer, VerifiedRoleGrant,
    profile::{
        AuthDocument, LocalCredentials, ProfileCredentials, ProfileData, ProfileError,
        ProfileExpectation, ProfileRecord, ProviderConfig, ProviderRecord,
    },
    store::{AuthSnapshot, AuthStoreError},
};
pub use broker_login::{
    BrokerLoginError, BrokerLoginErrorKind, BrokerLoginSelection, InteractiveLoginConfig,
    InteractiveLoginError, InteractiveLoginLimits, InteractiveTransportError, ManagedBrokerLogin,
    ManagedDeviceBrokerLogin, ManagedInteractiveBrokerLogin, ReceivedDeviceBrokerLogin,
    ReceivedInteractiveBrokerLogin,
};
pub(crate) use broker_refresh::NativeRefreshState;
pub use broker_refresh::{BrokerRefreshError, BrokerRefreshErrorKind};
use graphit_core::auth_document::{
    publication::{AuthDocumentPublication, PendingCleanup},
    recovery::AuthStageRecovery,
};
use graphit_hub::policy::RequestContext;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileManagerError {
    Authority(AuthError),
    Store(AuthStoreError),
    WrongManager,
    RefreshBusy,
}
impl fmt::Display for ProfileManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authority(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
            Self::WrongManager => f.write_str("profile snapshot manager mismatch"),
            Self::RefreshBusy => f.write_str("native profile refresh in progress"),
        }
    }
}
impl std::error::Error for ProfileManagerError {}
impl From<AuthError> for ProfileManagerError {
    fn from(e: AuthError) -> Self {
        Self::Authority(e)
    }
}
impl From<AuthStoreError> for ProfileManagerError {
    fn from(e: AuthStoreError) -> Self {
        Self::Store(e)
    }
}

/// Selects an actual proof, never a permission assertion or replacement request.
/// ```compile_fail
/// use graphit_auth::{AuthenticatedCaller, profile_manager::ProfileManagerAuthority};
/// fn ordinary<'a>(caller: &'a AuthenticatedCaller<'a>) {
///     let _ = ProfileManagerAuthority::Admin(caller);
/// }
/// ```
pub enum ProfileManagerAuthority<'a> {
    Admin(&'a AdminParent<'a>),
    Role(&'a VerifiedRoleGrant<'a>),
}
impl<'a> ProfileManagerAuthority<'a> {
    fn authorize(&self, issuer: &RoleIssuer) -> Result<&'a RequestContext<'a>, AuthError> {
        match self {
            Self::Admin(parent) => parent.profile_request(issuer),
            Self::Role(grant) => grant.profile_request(issuer),
        }
    }
}

pub struct NativeProfileManager<'i> {
    issuer: &'i RoleIssuer,
}
impl<'i> NativeProfileManager<'i> {
    /// Bind the existing issuer without reading or creating authentication state.
    pub fn new(issuer: &'i RoleIssuer) -> Self {
        Self { issuer }
    }

    pub fn begin<'a>(
        &'a self,
        authority: ProfileManagerAuthority<'a>,
    ) -> Result<ManagedAuthSnapshot<'a>, ProfileManagerError> {
        let request = authority.authorize(self.issuer)?;
        let snapshot = self.issuer.profile_store().load(request)?;
        authority.authorize(self.issuer)?;
        Ok(ManagedAuthSnapshot {
            manager: self,
            authority,
            request,
            snapshot,
        })
    }

    fn publish(
        &self,
        managed: ManagedAuthSnapshot<'_>,
        mutation: impl FnOnce(&mut AuthDocument, &RequestContext<'_>) -> Result<(), ProfileError>,
    ) -> Result<AuthDocumentPublication, ProfileManagerError> {
        if !std::ptr::eq(self, managed.manager) {
            return Err(ProfileManagerError::WrongManager);
        }
        let ManagedAuthSnapshot {
            authority,
            request,
            snapshot,
            ..
        } = managed;
        let store = self.issuer.profile_store();
        let prepared = store.prepare(snapshot, request, mutation)?;
        #[cfg(test)]
        crate::issuer::tests::profile_manager::after_prepare();
        authority.authorize(self.issuer)?;
        Ok(store.commit(prepared)?)
    }

    pub fn add_provider(
        &self,
        snapshot: ManagedAuthSnapshot<'_>,
        name: String,
        config: ProviderConfig,
        expected: u64,
        now: i64,
    ) -> Result<AuthDocumentPublication, ProfileManagerError> {
        self.publish(snapshot, |doc, ctx| {
            doc.add_provider(name, config, expected, now, ctx)
        })
    }
    pub fn update_provider(
        &self,
        snapshot: ManagedAuthSnapshot<'_>,
        name: &str,
        config: ProviderConfig,
        expected_document: u64,
        expected_provider: u64,
        now: i64,
    ) -> Result<AuthDocumentPublication, ProfileManagerError> {
        self.publish(snapshot, |doc, ctx| {
            doc.update_provider(name, config, expected_document, expected_provider, now, ctx)
        })
    }
    pub fn remove_provider(
        &self,
        snapshot: ManagedAuthSnapshot<'_>,
        name: &str,
        cascade: bool,
        expected: u64,
    ) -> Result<AuthDocumentPublication, ProfileManagerError> {
        self.publish(snapshot, |doc, ctx| {
            doc.remove_provider(name, cascade, expected, ctx)
        })
    }
    pub fn ensure_default_local(
        &self,
        snapshot: ManagedAuthSnapshot<'_>,
        expected: u64,
        now: i64,
    ) -> Result<AuthDocumentPublication, ProfileManagerError> {
        self.publish(snapshot, |doc, ctx| {
            doc.ensure_default_local(expected, now, ctx).map(|_| ())
        })
    }
    pub fn activate(
        &self,
        snapshot: ManagedAuthSnapshot<'_>,
        name: &str,
        expected: u64,
    ) -> Result<AuthDocumentPublication, ProfileManagerError> {
        self.publish(snapshot, |doc, ctx| doc.activate(name, expected, ctx))
    }
    pub fn logout(
        &self,
        snapshot: ManagedAuthSnapshot<'_>,
        name: Option<&str>,
        expected: u64,
    ) -> Result<AuthDocumentPublication, ProfileManagerError> {
        self.publish(snapshot, |doc, ctx| doc.logout(name, expected, ctx))
    }
    /// Stores only supplied Local credentials, then activates in the same publication.
    pub fn install_local(
        &self,
        snapshot: ManagedAuthSnapshot<'_>,
        input: LocalProfileInput,
        expected: ProfileExpectation,
        now: i64,
    ) -> Result<AuthDocumentPublication, ProfileManagerError> {
        let cap = self.issuer.profile_store().maximum_encoded_bytes();
        self.publish(snapshot, |doc, ctx| {
            if input.name.len() > cap {
                return Err(ProfileError::Limit);
            }
            let mut name = String::new();
            name.try_reserve_exact(input.name.len())
                .map_err(|_| ProfileError::Limit)?;
            name.push_str(&input.name);
            doc.replace_profile_data(
                ProfileData {
                    name: input.name,
                    provider: input.provider,
                    username: input.username,
                    organization: input.organization,
                    teams: input.teams,
                    credentials: ProfileCredentials::Local(input.credentials),
                },
                expected,
                now,
                ctx,
            )?;
            doc.activate(&name, doc.revision(), ctx)
        })
    }
    pub fn recover_stage(
        &self,
        authority: ProfileManagerAuthority<'_>,
    ) -> Result<AuthStageRecovery, ProfileManagerError> {
        let request = authority.authorize(self.issuer)?;
        // Recovery must not discard an active refresh publication's stage.
        let _refresh = self
            .issuer
            .native_profile_refresh()
            .try_lock()
            .map_err(|_| ProfileManagerError::RefreshBusy)?;
        Ok(self.issuer.profile_store().recover_stage(request))
    }
    pub fn cleanup_pending(
        &self,
        authority: ProfileManagerAuthority<'_>,
        pending: PendingCleanup,
    ) -> Result<AuthStageRecovery, ProfileCleanupDenied> {
        let request = match authority.authorize(self.issuer) {
            Ok(request) => request,
            Err(error) => {
                return Err(ProfileCleanupDenied {
                    error: error.into(),
                    pending,
                });
            }
        };
        Ok(self
            .issuer
            .profile_store()
            .cleanup_pending(pending, request))
    }
}

/// A rejected admission retains the original cleanup proof for a fresh request.
pub struct ProfileCleanupDenied {
    error: ProfileManagerError,
    pending: PendingCleanup,
}
impl ProfileCleanupDenied {
    pub fn error(&self) -> ProfileManagerError {
        self.error
    }
    pub fn into_pending_cleanup(self) -> PendingCleanup {
        self.pending
    }
}

/// Owned unverified Local data; optional keys are replaced, never automatically minted.
/// ```compile_fail
/// use graphit_auth::{profile::{ProfileCredentials, LocalCredentials}, profile_manager::LocalProfileInput};
/// fn broker(input: &mut LocalProfileInput, broker: ProfileCredentials) { input.credentials = broker; }
/// ```
pub struct LocalProfileInput {
    pub name: String,
    pub provider: String,
    pub username: String,
    pub organization: Option<String>,
    pub teams: Vec<String>,
    pub credentials: LocalCredentials,
}

/// Captured data owns its snapshot and borrows the original authority until consumed or dropped.
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedAuthSnapshot;
/// fn expose(s: &ManagedAuthSnapshot<'_>) { s.document(); }
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedAuthSnapshot;
/// fn clone<T: Clone>() {} clone::<ManagedAuthSnapshot<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedAuthSnapshot;
/// fn debug<T: std::fmt::Debug>() {} debug::<ManagedAuthSnapshot<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedAuthSnapshot;
/// fn serde<T: serde::Serialize>() {} serde::<ManagedAuthSnapshot<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedAuthSnapshot;
/// fn raw(s: &ManagedAuthSnapshot<'_>) { s.profiles().next().unwrap().record; }
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedAuthSnapshot;
/// fn secret(s: &ManagedAuthSnapshot<'_>) { s.profiles().next().unwrap().credentials(); }
/// ```
/// ```compile_fail
/// use graphit_auth::profile_manager::ManagedAuthSnapshot;
/// fn detached<'a>(s: ManagedAuthSnapshot<'a>) -> &'a str {
///     s.profiles().next().unwrap().name()
/// }
/// ```
pub struct ManagedAuthSnapshot<'a> {
    manager: &'a NativeProfileManager<'a>,
    authority: ProfileManagerAuthority<'a>,
    request: &'a RequestContext<'a>,
    snapshot: AuthSnapshot,
}
impl ManagedAuthSnapshot<'_> {
    pub fn revision(&self) -> u64 {
        self.snapshot.document().revision()
    }
    pub fn was_present(&self) -> bool {
        self.snapshot.was_present()
    }
    pub fn active_name(&self) -> Option<&str> {
        self.snapshot.document().active_name()
    }
    pub fn providers(&self) -> impl ExactSizeIterator<Item = ProviderView<'_>> {
        self.snapshot
            .document()
            .providers()
            .iter()
            .map(|record| ProviderView { record })
    }
    pub fn profiles(&self) -> impl ExactSizeIterator<Item = ProfileView<'_>> {
        self.snapshot
            .document()
            .profiles()
            .iter()
            .map(|record| ProfileView { record })
    }
}
pub struct ProviderView<'a> {
    record: &'a ProviderRecord,
}
impl ProviderView<'_> {
    pub fn name(&self) -> &str {
        self.record.name()
    }
    pub fn revision(&self) -> u64 {
        self.record.revision()
    }
    pub fn created_at(&self) -> i64 {
        self.record.created_at()
    }
    pub fn updated_at(&self) -> i64 {
        self.record.updated_at()
    }
    pub fn config(&self) -> &ProviderConfig {
        self.record.config()
    }
}
pub struct ProfileView<'a> {
    record: &'a ProfileRecord,
}
impl ProfileView<'_> {
    pub fn name(&self) -> &str {
        self.record.name()
    }
    pub fn provider(&self) -> &str {
        self.record.provider()
    }
    pub fn username(&self) -> Option<&str> {
        match self.record.data() {
            Some(data) => Some(&data.username),
            None => self
                .record
                .broker_device_data()
                .and_then(|data| data.metadata.username.as_deref()),
        }
    }
    /// Stored identity assertions only, never a verified Broker identity.
    pub fn broker_identity(&self) -> Option<(&str, &str)> {
        if let Some(data) = self.record.broker_device_data() {
            return Some((&data.issuer, &data.subject));
        }
        match &self.record.data()?.credentials {
            ProfileCredentials::Broker {
                issuer, subject, ..
            } => Some((issuer, subject)),
            ProfileCredentials::Local(_) => None,
        }
    }
    /// Stored service choice only; this metadata does not grant S3 access.
    pub fn broker_s3_disabled(&self) -> Option<bool> {
        if let Some(data) = self.record.broker_device_data() {
            return Some(data.s3_disabled);
        }
        match &self.record.data()?.credentials {
            ProfileCredentials::Broker { s3_disabled, .. } => Some(*s3_disabled),
            ProfileCredentials::Local(_) => None,
        }
    }
    pub fn local_anonymous(&self) -> Option<bool> {
        match &self.record.data()?.credentials {
            ProfileCredentials::Local(credentials) => Some(credentials.anonymous),
            ProfileCredentials::Broker { .. } => None,
        }
    }
    pub fn organization(&self) -> Option<&str> {
        match self.record.data() {
            Some(data) => data.organization.as_deref(),
            None => self
                .record
                .broker_device_data()
                .and_then(|data| data.metadata.organization.as_deref()),
        }
    }
    pub fn teams(&self) -> Option<impl ExactSizeIterator<Item = &str>> {
        let teams = match self.record.data() {
            Some(data) => Some(data.teams.as_slice()),
            None => self
                .record
                .broker_device_data()
                .and_then(|data| data.metadata.teams.as_deref()),
        };
        teams.map(|teams| teams.iter().map(String::as_str))
    }
    pub fn revision(&self) -> u64 {
        self.record.revision()
    }
    pub fn token_revision(&self) -> u64 {
        self.record.token_revision()
    }
    pub fn provider_revision(&self) -> u64 {
        self.record.provider_revision()
    }
    pub fn created_at(&self) -> i64 {
        self.record.created_at()
    }
    pub fn updated_at(&self) -> i64 {
        self.record.updated_at()
    }
}
