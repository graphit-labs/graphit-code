//! One owned administrative rotation for the selected provider's current credential.
use super::Plan;
use graphit_auth::{
    broker::oauth::{NativeOAuth, RemoteEffect},
    profile_manager::{NativeProfileManager, ProfileManagerAuthority},
};
use graphit_core::file_lock::Cancellation;
use std::{
    io,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) struct NativeRefreshJob {
    stop: Cancellation,
    job: tokio::task::JoinSet<io::Result<()>>,
}
impl NativeRefreshJob {
    pub fn start(plan: Arc<Plan>, service: Cancellation) -> Self {
        let stop = Cancellation::default();
        if service.is_cancelled() {
            stop.cancel();
        }
        let operation_stop = stop.clone();
        let mut job = tokio::task::JoinSet::new();
        job.spawn(async move {
            let operation = rotate(&plan, &operation_stop);
            tokio::pin!(operation);
            let result = tokio::select! {
                biased;
                result = &mut operation => result,
                () = service.cancelled() => {
                    operation_stop.cancel();
                    operation.await
                },
                () = operation_stop.cancelled() => operation.await,
            };
            result.map_err(|error| {
                if operation_stop.is_cancelled() {
                    io::Error::new(io::ErrorKind::Interrupted, error)
                } else {
                    error
                }
            })
        });
        Self { stop, job }
    }
    pub fn cancel(&self) {
        self.stop.cancel();
    }
    pub async fn join(&mut self) -> io::Result<()> {
        self.job
            .join_next()
            .await
            .ok_or_else(|| io::Error::other("native refresh operation already joined"))?
            .map_err(io::Error::other)?
    }
}

async fn rotate(plan: &Plan, stop: &Cancellation) -> io::Result<()> {
    let original = plan
        .policy
        .request_context(std::time::Instant::now(), stop)
        .map_err(io::Error::other)?;
    let key = plan
        .auth
        .runtime()
        .check(original.snapshot())
        .map_err(io::Error::other)?;
    let raw = std::str::from_utf8(key.secret(original.snapshot()).map_err(io::Error::other)?)
        .map_err(io::Error::other)?;
    let caller = plan
        .auth
        .issuer()
        .authenticate_runtime(raw, &original)
        .map_err(io::Error::other)?;
    let session = plan
        .auth
        .issuer()
        .bootstrap_admin(&caller)
        .map_err(io::Error::other)?;
    let parent = plan
        .auth
        .issuer()
        .admin_parent(&caller, &session)
        .map_err(io::Error::other)?;
    let authority = || ProfileManagerAuthority::Admin(&parent);
    let manager = NativeProfileManager::new(plan.auth.issuer());
    let snapshot = manager.begin(authority()).map_err(io::Error::other)?;
    let active = snapshot.active_name().ok_or(io::ErrorKind::NotFound)?;
    let selected = snapshot
        .profiles()
        .find(|profile| profile.name() == active)
        .ok_or(io::ErrorKind::NotFound)?;
    if selected.provider() != plan.provider.name() || selected.broker_identity().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "native refresh provider changed",
        ));
    }
    let expected_profile = selected.name().to_owned();
    let expected_provider_revision = selected.provider_revision();
    let expected_token_revision = selected.token_revision();
    let (expected_issuer, expected_subject) = selected
        .broker_identity()
        .map(|(issuer, subject)| (issuer.to_owned(), subject.to_owned()))
        .ok_or(io::ErrorKind::PermissionDenied)?;
    let mut selection = plan
        .auth
        .issuer()
        .capture_presence_publish_service(&caller, plan.provider.name())
        .map_err(io::Error::other)?;
    let verifier = plan
        .verifiers
        .as_ref()
        .ok_or(io::ErrorKind::InvalidInput)?
        .get(&mut selection)
        .map_err(io::Error::other)?;
    match selection.native_credential(&verifier).await {
        // A Records demand may already have rotated the same selected account.
        Ok(lease) => {
            lease.check().map_err(io::Error::other)?;
            return Ok(());
        }
        Err(graphit_auth::ServiceUseError::NeedsRefresh) => (),
        Err(error) => return Err(io::Error::other(error)),
    }
    let oauth = NativeOAuth::new(
        &verifier,
        crate::admin_auth::oauth_limits(plan.policy.content()),
    )
    .map_err(io::Error::other)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs();
    let now = i64::try_from(now).map_err(io::Error::other)?;
    match manager.refresh_broker(snapshot, &oauth, now).await {
        Ok(_) => (),
        Err(error) => {
            if error.remote_effect() == RemoteEffect::NotSent {
                let current = manager.begin(authority()).map_err(io::Error::other)?;
                if current.active_name() == Some(expected_profile.as_str())
                    && current.profiles().any(|profile| {
                        profile.name() == expected_profile
                            && profile.provider() == plan.provider.name()
                            && profile.provider_revision() == expected_provider_revision
                            && profile.broker_identity()
                                == Some((expected_issuer.as_str(), expected_subject.as_str()))
                            && profile.token_revision() != expected_token_revision
                    })
                {
                    let mut current = plan
                        .auth
                        .issuer()
                        .capture_presence_publish_service(&caller, plan.provider.name())
                        .map_err(io::Error::other)?;
                    let verifier = plan
                        .verifiers
                        .as_ref()
                        .ok_or(io::ErrorKind::InvalidInput)?
                        .get(&mut current)
                        .map_err(io::Error::other)?;
                    let lease = current
                        .native_credential(&verifier)
                        .await
                        .map_err(io::Error::other)?;
                    lease.check().map_err(io::Error::other)?;
                    return Ok(());
                }
            }
            return Err(io::Error::other(error));
        }
    };
    Ok(())
}
