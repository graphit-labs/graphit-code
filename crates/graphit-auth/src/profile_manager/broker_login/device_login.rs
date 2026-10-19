use super::*;
use crate::{
    broker::oauth::device::{DeviceInstructions, FinalDeviceTokens, PendingDeviceLogin},
    profile::{BrokerDeviceMetadata, BrokerDeviceProfileData, BrokerDeviceTokens},
};

/// Owns the actual Device grant and the original administrative snapshot.
pub struct ManagedDeviceBrokerLogin<'a> {
    pending: PendingDeviceLogin<'a>,
    managed: ManagedAuthSnapshot<'a>,
    service: &'a NativeOAuth<'a>,
    selection: BrokerLoginSelection<'a>,
    expected: ProfileExpectation,
    prior_updated_at: Option<i64>,
}
/// Owns one verified, consumed Device response until its original publication.
pub struct ReceivedDeviceBrokerLogin<'a> {
    tokens: FinalDeviceTokens<'a>,
    managed: ManagedAuthSnapshot<'a>,
    service: &'a NativeOAuth<'a>,
    selection: BrokerLoginSelection<'a>,
    expected: ProfileExpectation,
    prior_updated_at: Option<i64>,
}
impl NativeProfileManager<'_> {
    pub async fn begin_device_broker_login<'a>(
        &'a self,
        mut managed: ManagedAuthSnapshot<'a>,
        oauth: &'a NativeOAuth<'a>,
        selection: BrokerLoginSelection<'a>,
    ) -> Result<ManagedDeviceBrokerLogin<'a>, BrokerLoginError> {
        let (expected, prior_updated_at) = admit_login(self, &mut managed, oauth, &selection)?;
        let context = BrokerRequestContext::new(managed.request.snapshot());
        let pending = {
            let (observation, document) = self
                .issuer
                .profile_store()
                .snapshot_parts(&mut managed.snapshot);
            let provider = document
                .provider(selection.provider)
                .ok_or_else(|| before(profile(ProfileError::Missing)))?;
            let ProviderKind::Broker(config) = &provider.config().kind else {
                return Err(before(profile(ProfileError::InvalidDocument)));
            };
            if config.mcp_resource.is_some() {
                return Err(before(profile(ProfileError::InvalidDocument)));
            }
            let mut gate = ManagedLoginGate {
                manager: self,
                authority: &managed.authority,
                request: managed.request,
                service: oauth,
                endpoint: &config.endpoint,
                resource: None,
                provider_revision: provider.revision(),
                observation,
            };
            oauth
                .begin_device_guarded(context, &mut gate)
                .await
                .map_err(|error| match error {
                    GuardedNativeError::Native(e) => BrokerLoginError::native(e),
                    GuardedNativeError::Admission(e) => before(e),
                })?
        };
        check_managed(&mut managed, oauth, &selection).map_err(after)?;
        pending
            .check(oauth, context)
            .map_err(BrokerLoginError::native)?;
        Ok(ManagedDeviceBrokerLogin {
            pending,
            managed,
            service: oauth,
            selection,
            expected,
            prior_updated_at,
        })
    }
}
impl ManagedDeviceBrokerLogin<'_> {
    /// Only the public browser URI and user code leave this owned flow.
    pub fn instructions(&mut self) -> Result<&DeviceInstructions, BrokerLoginError> {
        check_managed(&mut self.managed, self.service, &self.selection).map_err(before)?;
        self.pending
            .instructions(BrokerRequestContext::new(self.managed.request.snapshot()))
            .map_err(BrokerLoginError::native)
    }
}
impl<'a> ManagedDeviceBrokerLogin<'a> {
    pub async fn receive(self) -> Result<ReceivedDeviceBrokerLogin<'a>, BrokerLoginError> {
        let Self {
            pending,
            mut managed,
            service,
            selection,
            expected,
            prior_updated_at,
        } = self;
        check_managed(&mut managed, service, &selection).map_err(before)?;
        let context = BrokerRequestContext::new(managed.request.snapshot());
        let tokens = {
            let (observation, document) = managed
                .manager
                .issuer
                .profile_store()
                .snapshot_parts(&mut managed.snapshot);
            let provider = document
                .provider(selection.provider)
                .ok_or_else(|| before(profile(ProfileError::Missing)))?;
            let ProviderKind::Broker(config) = &provider.config().kind else {
                return Err(before(profile(ProfileError::InvalidDocument)));
            };
            let mut gate = ManagedLoginGate {
                manager: managed.manager,
                authority: &managed.authority,
                request: managed.request,
                service,
                endpoint: &config.endpoint,
                resource: config.mcp_resource.as_deref(),
                provider_revision: provider.revision(),
                observation,
            };
            service
                .receive_device_guarded(pending, context, &mut gate)
                .await
                .map_err(|error| match error {
                    GuardedNativeError::Native(e) => BrokerLoginError::native(e),
                    GuardedNativeError::Admission(e) => before(e),
                })?
        };
        check_managed(&mut managed, service, &selection).map_err(after)?;
        tokens
            .check(service, context)
            .map_err(BrokerLoginError::native)?;
        Ok(ReceivedDeviceBrokerLogin {
            tokens,
            managed,
            service,
            selection,
            expected,
            prior_updated_at,
        })
    }
}
impl ReceivedDeviceBrokerLogin<'_> {
    pub async fn publish(self, now: i64) -> Result<AuthDocumentPublication, BrokerLoginError> {
        if now < 0 {
            return Err(after(profile(ProfileError::InvalidDocument)));
        }
        if self.prior_updated_at.is_some_and(|prior| now < prior) {
            return Err(after(profile(ProfileError::Conflict)));
        }
        let Self {
            tokens,
            mut managed,
            service,
            selection,
            expected,
            ..
        } = self;
        check_managed(&mut managed, service, &selection).map_err(after)?;
        let ManagedAuthSnapshot {
            manager: owner,
            authority,
            request,
            snapshot,
        } = managed;
        let context = BrokerRequestContext::new(request.snapshot());
        let store = owner.issuer.profile_store();
        let data = verified_device_profile(
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
                doc.replace_broker_device_profile(data, expected, now, ctx)?;
                doc.activate(selection.profile, doc.revision(), ctx)
            })
            .map_err(|e| after(manager(e)))?;
        #[cfg(test)]
        crate::issuer::tests::profile_manager::after_prepare();
        tokens
            .check(service, context)
            .map_err(BrokerLoginError::native)?;
        authority
            .authorize(owner.issuer)
            .map_err(|e| after(manager(e)))?;
        tokens
            .check(service, context)
            .map_err(BrokerLoginError::native)?;
        store.commit(prepared).map_err(|e| after(manager(e)))
    }
}
fn verified_device_profile(
    tokens: &FinalDeviceTokens<'_>,
    selection: &BrokerLoginSelection<'_>,
    revision: u64,
    context: BrokerRequestContext<'_>,
    limits: ProfileLimits,
    request: &RequestContext<'_>,
) -> Result<BrokerDeviceProfileData, BrokerLoginErrorKind> {
    let native = |e: NativeError| BrokerLoginErrorKind::OAuth(e.kind());
    let identity = tokens.identity(context).map_err(native)?;
    if identity.provider_revision() != revision {
        return Err(BrokerLoginErrorKind::IdentityMismatch);
    }
    let access = tokens.access_token(context).map_err(native)?;
    let expires_at = tokens.expires_at(context).map_err(native)?;
    let teams = identity.access_teams();
    let count = teams.map_or(0, <[String]>::len);
    if count > limits.collection_entries {
        return Err(profile(ProfileError::Limit));
    }
    let mut bytes = count
        .checked_mul(std::mem::size_of::<String>())
        .ok_or_else(|| profile(ProfileError::Limit))?;
    for text in [
        Some(selection.profile),
        Some(selection.profile),
        Some(selection.provider),
        Some(identity.issuer()),
        Some(identity.subject()),
        Some(access),
        identity.username(),
        identity.organization(),
    ]
    .into_iter()
    .flatten()
    .chain(teams.into_iter().flatten().map(String::as_str))
    {
        request.check().map_err(|_| manager(AuthError::Request))?;
        bytes = bytes
            .checked_add(text.len())
            .ok_or_else(|| profile(ProfileError::Limit))?;
        if text.len() > limits.scalar_bytes || bytes > limits.bytes {
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
    let teams = teams
        .map(|values| {
            let mut owned = Vec::new();
            owned
                .try_reserve_exact(values.len())
                .map_err(|_| profile(ProfileError::Limit))?;
            for value in values {
                owned.push(text(value)?);
            }
            Ok::<_, BrokerLoginErrorKind>(owned)
        })
        .transpose()?;
    Ok(BrokerDeviceProfileData {
        name: text(selection.profile)?,
        provider: text(selection.provider)?,
        issuer: text(identity.issuer())?,
        subject: text(identity.subject())?,
        tokens: BrokerDeviceTokens {
            access_token: Secret::new(SecretString::from(text(access)?)),
            expires_at,
        },
        s3_disabled: selection.s3_disabled,
        metadata: BrokerDeviceMetadata {
            username: identity.username().map(text).transpose()?,
            organization: identity.organization().map(text).transpose()?,
            teams,
        },
    })
}
