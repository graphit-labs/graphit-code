//! One retained administrative rotation for the publisher owner's selected provider.
use super::Plan;
use graphit_auth::{
    broker::oauth::{NativeOAuth, RemoteEffect},
    profile_manager::{NativeProfileManager, ProfileManagerAuthority, ProfileManagerError},
};
use graphit_core::{
    auth_document::{
        AuthDocumentError,
        publication::{AuthDocumentPublication, AuthPublicationEffect, PendingCleanup},
        recovery::{AuthCleanupEffect, AuthStageRecovery},
    },
    file_lock::Cancellation,
};
use std::{
    fmt, io,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) struct Completion {
    pub result: io::Result<()>,
    pub pending: Option<PendingCleanup>,
}

pub(super) struct NativeRefreshJob {
    stop: Cancellation,
    job: tokio::task::JoinSet<Completion>,
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
            match result {
                Ok(completion) => completion,
                Err(error) => Completion {
                    result: Err(if operation_stop.is_cancelled() {
                        io::Error::new(io::ErrorKind::Interrupted, error)
                    } else {
                        error
                    }),
                    pending: None,
                },
            }
        });
        Self { stop, job }
    }

    pub fn cancel(&self) {
        self.stop.cancel();
    }

    pub async fn join(&mut self) -> io::Result<Completion> {
        self.job
            .join_next()
            .await
            .ok_or_else(|| io::Error::other("native refresh operation already joined"))?
            .map_err(io::Error::other)
    }
}

#[derive(Debug)]
struct PublicationFailure {
    effect: AuthPublicationEffect,
    failure: Option<AuthDocumentError>,
    cleanup: Option<AuthDocumentError>,
    cleanup_effect: AuthCleanupEffect,
    remote: RemoteEffect,
}
impl fmt::Display for PublicationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native refresh publication: effect={:?}; failure={:?}; cleanup={:?}; cleanup_effect={:?}; remote={:?}",
            self.effect, self.failure, self.cleanup, self.cleanup_effect, self.remote
        )
    }
}
impl std::error::Error for PublicationFailure {}

#[derive(Debug)]
struct RecoveryFailure {
    effect: AuthCleanupEffect,
    failure: Option<AuthDocumentError>,
}
impl fmt::Display for RecoveryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native refresh cleanup: effect={:?}; failure={:?}",
            self.effect, self.failure
        )
    }
}
impl std::error::Error for RecoveryFailure {}

fn recovery(outcome: AuthStageRecovery) -> Completion {
    let failure = RecoveryFailure {
        effect: outcome.effect(),
        failure: outcome.failure(),
    };
    let pending = outcome.into_pending_cleanup();
    let failed = failure.failure.is_some()
        || matches!(
            failure.effect,
            AuthCleanupEffect::NotApplied | AuthCleanupEffect::Unknown
        )
        || pending.is_some();
    Completion {
        result: if failed {
            Err(io::Error::other(failure))
        } else {
            Ok(())
        },
        pending,
    }
}

fn publication(
    manager: &NativeProfileManager<'_>,
    authority: ProfileManagerAuthority<'_>,
    outcome: AuthDocumentPublication,
) -> Completion {
    let failure = PublicationFailure {
        effect: outcome.effect(),
        failure: outcome.failure(),
        cleanup: outcome.cleanup_failure(),
        cleanup_effect: outcome.cleanup_effect(),
        // The manager returns publication only after verified OAuth rotation.
        remote: RemoteEffect::MayHaveApplied,
    };
    let mut first = (failure.failure.is_some()
        || failure.cleanup.is_some()
        || failure.effect != AuthPublicationEffect::Applied)
        .then(|| io::Error::other(failure));
    let mut pending = None;
    if let Some(proof) = outcome.into_pending_cleanup() {
        match manager.cleanup_pending(authority, proof) {
            Ok(cleaned) => {
                let cleaned = recovery(cleaned);
                if let Err(error) = cleaned.result {
                    first.get_or_insert(error);
                }
                pending = cleaned.pending;
            }
            Err(error) => {
                first.get_or_insert_with(|| io::Error::other(error.error()));
                pending = Some(error.into_pending_cleanup());
            }
        }
    }
    if pending.is_some() {
        first.get_or_insert_with(|| io::Error::other("native refresh cleanup remains pending"));
    }
    Completion {
        result: first.map_or(Ok(()), Err),
        pending,
    }
}

async fn rotate(plan: &Plan, stop: &Cancellation) -> io::Result<Completion> {
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
    match manager.recover_stage(authority()) {
        Ok(outcome) => {
            let recovered = recovery(outcome);
            if recovered.result.is_err() || recovered.pending.is_some() {
                return Ok(recovered);
            }
        }
        // Another owner is rotating under the common issuer gate. Its stage is
        // active; the manager below will wait and recheck this fresh snapshot.
        Err(ProfileManagerError::RefreshBusy) => (),
        Err(error) => return Err(io::Error::other(error)),
    }
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
            return Ok(Completion {
                result: Ok(()),
                pending: None,
            });
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
    let refreshed = match manager.refresh_broker(snapshot, &oauth, now).await {
        Ok(publication) => publication,
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
                    return Ok(Completion {
                        result: Ok(()),
                        pending: None,
                    });
                }
            }
            return Err(io::Error::other(error));
        }
    };
    Ok(publication(&manager, authority(), refreshed))
}

pub(super) fn cleanup(
    plan: &Plan,
    proof: PendingCleanup,
) -> Result<(), (io::Error, Option<PendingCleanup>)> {
    let stop = Cancellation::default();
    let original = match plan
        .policy
        .request_context(std::time::Instant::now(), &stop)
    {
        Ok(original) => original,
        Err(error) => return Err((io::Error::other(error), Some(proof))),
    };
    let authority = (|| {
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
        Ok::<_, io::Error>((caller, session))
    })();
    let (caller, session) = match authority {
        Ok(authority) => authority,
        Err(error) => return Err((io::Error::other(error), Some(proof))),
    };
    let parent = match plan.auth.issuer().admin_parent(&caller, &session) {
        Ok(parent) => parent,
        Err(error) => return Err((io::Error::other(error), Some(proof))),
    };
    let manager = NativeProfileManager::new(plan.auth.issuer());
    match manager.cleanup_pending(ProfileManagerAuthority::Admin(&parent), proof) {
        Ok(outcome) => {
            let Completion { result, pending } = recovery(outcome);
            result.map_err(|error| (error, pending))
        }
        Err(error) => Err((
            io::Error::other(error.error()),
            Some(error.into_pending_cleanup()),
        )),
    }
}
