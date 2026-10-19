//! The retained HTTP job owns authentication, preparation and native completion.
use super::*;
use crate::knowledge_live::Access;
use graphit_auth::ScopeCapture;
use graphit_core::artifact_store::ArtifactStore;
use graphit_hub::policy::RequestContext;

mod prepare;

pub(super) async fn execute(
    routes: &Routes,
    web_id: &str,
    entry: &Arc<Session>,
    cancellation: &Cancellation,
    deadline: Instant,
    command: Command,
    reply: &mut Option<Reply>,
) -> Result<LiveSession, Error> {
    let policy = routes.tasks.browser_policy();
    let original =
        RequestContext::new(policy.0, cancellation, deadline).map_err(|_| Failure::Invalid)?;
    let shared = &routes.sessions.0;
    let work = async {
        let credentials = shared.credentials(entry, original.snapshot()).await?;
        let bearer = zeroize::Zeroizing::new(credentials.access_token().to_owned());
        drop(credentials);
        let flow = shared.flow(entry.provider)?;
        let (provider, verifier) = &shared.providers[entry.provider];
        let context = BrokerRequestContext::new(original.snapshot());
        let caller = flow
            .verify_access(&entry.registered, &bearer, context)
            .await
            .map_err(Failure::from)?;
        caller.check(verifier, context).map_err(Failure::from)?;
        let snapshot = routes
            .tasks
            .record_snapshot()
            .map_err(|error| Error::Live(LiveFailure::Admission(error)))?;
        let resources = routes.tasks.record_resources(&snapshot);
        let issuer = resources.auth.issuer();
        let scope = crate::foreground_user_memory::capture(
            issuer,
            resources.auth.installation(),
            &caller,
            verifier,
            &original,
            resources.policy,
        )
        .map_err(|cause| match cause {
            crate::foreground_user_memory::CaptureError::Physical(cause) => io_failure(cause),
            crate::foreground_user_memory::CaptureError::Auth(cause) => io_failure(
                original
                    .check()
                    .err()
                    .unwrap_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, cause)),
            ),
            crate::foreground_user_memory::CaptureError::Subject(cause) => io_failure(
                original
                    .check()
                    .err()
                    .unwrap_or_else(|| io::Error::other(cause)),
            ),
        })?;
        let binding = scope.binding().map_err(io_failure)?;
        let capture = || ScopeCapture {
            live: &scope,
            record: &binding,
        };
        let read = issuer
            .admit_web_user_knowledge_read(&caller, verifier, &original, provider, capture())
            .map_err(|cause| {
                io_failure(
                    original
                        .check()
                        .err()
                        .unwrap_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, cause)),
                )
            })?;
        let write = issuer
            .admit_web_user_knowledge_write(&caller, verifier, &original, provider, capture())
            .map_err(|cause| {
                io_failure(
                    original
                        .check()
                        .err()
                        .unwrap_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, cause)),
                )
            })?;
        let access = Access::user(&read, Some(&write)).map_err(io_failure)?;
        let live = routes
            .tasks
            .live_sessions()
            .ok_or_else(|| io_failure(io::ErrorKind::Unsupported.into()))?;
        let id = match &command {
            Command::Create(request) => &request.request_id,
            Command::Send { id, .. } => id,
        };
        let preparation = match live.begin_preparation(&access, id) {
            Ok(guard) => guard,
            Err(cause) if cause.kind() == io::ErrorKind::WouldBlock => {
                // A repeated accepted turn is read-only, even while its owner is active.
                // Create still validates the complete request before returning its session.
                let replayed = match command {
                    Command::Create(request) => Some(
                        live.create(&access, *request)
                            .map_err(|cause| Error::Live(cause.into()))?,
                    ),
                    Command::Send { id, input } => {
                        prepare::replayed_turn(live, &access, &id, Some(&input))?
                    }
                };
                return replayed.ok_or_else(|| Error::Live(cause.into()));
            }
            Err(cause) => return Err(Error::Live(cause.into())),
        };
        let (session, turn) = match command {
            Command::Create(request) => {
                access.write().map_err(io_failure)?;
                let store =
                    ArtifactStore::new(access.scope().clone(), resources.policy.write.artifacts);
                let writer = store
                    .writer_with_context(original.snapshot())
                    .map_err(|cause| io_failure(io::Error::new(cause.kind(), cause)))?;
                drop(writer);
                let turn = request.prompt.as_ref().map(|text| SendRequest {
                    turn_id: request.request_id.clone(),
                    text: text.clone(),
                });
                let session = live
                    .create(&access, *request)
                    .map_err(|cause| Error::Live(cause.into()))?;
                (session, turn)
            }
            Command::Send { id, input } => {
                let observed = live
                    .observe(&access, &id, None)
                    .map_err(|cause| Error::Live(cause.into()))?;
                (observed.replay.session, Some(input))
            }
        };
        if preparation.previous_session().is_some()
            && let Some(recovered) = preparation
                .recover_interrupted(&access)
                .map_err(|cause| Error::Live(cause.into()))?
        {
            return Ok(recovered);
        }
        let selected_session = session.clone();
        let result = prepare::execute(
            routes,
            web_id,
            entry,
            &resources,
            (&caller, verifier, provider),
            &original,
            &access,
            live,
            session,
            turn,
            reply,
        )
        .await
        .map_err(|error| match error {
            Error::Live(LiveFailure::Live(cause)) => Error::Live(LiveFailure::Live(
                cause.retain_prior_effect(preparation.recovered_effect()),
            )),
            other => other,
        });
        match result {
            Err(Error::Live(LiveFailure::Live(cause)))
                if reply.is_some()
                    && !matches!(cause, crate::knowledge_live::LiveError::Preparation { .. }) =>
            {
                Err(Error::Live(LiveFailure::Live(live.fail_preparation(
                    &access,
                    &selected_session,
                    cause,
                ))))
            }
            result => result,
        }
    };
    tokio::pin!(work);
    // Cancellation signals the same retained owner; never drop a running native job.
    let result = tokio::select! {
        result = &mut work => result,
        () = shared.stop.cancelled() => { cancellation.cancel(); work.await },
        () = policy.2.cancelled() => { cancellation.cancel(); work.await },
        () = entry.cancelled.cancelled() => { cancellation.cancel(); work.await },
        () = tokio::time::sleep_until(deadline.into()) => { cancellation.cancel(); work.await },
    };
    if let Err(Error::Session(cause)) = &result
        && session::invalidates(cause)
    {
        shared.invalidate(web_id, entry).await;
    }
    result
}
