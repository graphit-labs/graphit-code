use super::*;

#[derive(Serialize)]
struct Identity {
    provider: String,
    subject: String,
    username: Option<String>,
    expires_at: u64,
}
struct RefreshInFlight<'a> {
    entry: &'a Session,
    completed: bool,
}
impl Drop for RefreshInFlight<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.entry.cancelled.cancel();
        }
    }
}
impl Shared {
    pub(super) async fn credentials<'a>(
        &self,
        entry: &'a Session,
        original: &SnapshotContext,
    ) -> Result<tokio::sync::MutexGuard<'a, WebCredentials>, Failure> {
        let deadline = original
            .required_deadline()
            .map_err(|_| Failure::Unavailable)?;
        let mut credentials = tokio::select! {
            guard = entry.credentials.lock() => guard,
            () = self.stop.cancelled() => return Err(Failure::Unavailable),
            () = original.cancellation().cancelled() => return Err(Failure::Unavailable),
            () = tokio::time::sleep_until(deadline.into()) => return Err(Failure::Unavailable),
        };
        original.check().map_err(|_| Failure::Unavailable)?;
        if entry.cancelled.is_cancelled() || entry.expires <= Instant::now() {
            return Err(Failure::Unauthorized);
        }
        let flow = self.flow(entry.provider)?;
        let ctx = BrokerRequestContext::new(original);
        let expires_soon = credentials
            .expires_at()
            .duration_since(SystemTime::now())
            .map_or(true, |remaining| remaining <= self.policy.refresh_margin);
        if expires_soon {
            let refresh = credentials.refresh_token().ok_or(Failure::Unauthorized)?;
            let mut rotation = RefreshInFlight {
                entry,
                completed: false,
            };
            let refreshed = flow
                .refresh(&entry.registered, refresh, credentials.subject(), ctx)
                .await;
            rotation.completed = true;
            *credentials = refreshed?;
        }
        credentials.check(&flow, &entry.registered, ctx)?;
        Ok(credentials)
    }
    pub(super) async fn session(&self, jar: &CookieJar) -> Result<(String, Arc<Session>), Failure> {
        if self.stop.is_cancelled() {
            return Err(Failure::Unavailable);
        }
        let id = self.handle(jar, false)?;
        let mut sessions = self.sessions.lock().await;
        let entry = sessions.get(&id).cloned().ok_or(Failure::Unauthorized)?;
        if entry.cancelled.is_cancelled() || entry.expires <= Instant::now() {
            entry.cancelled.cancel();
            sessions.remove(&id);
            return Err(Failure::Unauthorized);
        }
        Ok((id, entry))
    }
    pub(super) async fn invalidate(&self, id: &str, entry: &Session) {
        entry.cancelled.cancel();
        self.sessions.lock().await.remove(id);
    }
}
pub(super) async fn identity(State(service): State<BrowserAuth>, jar: CookieJar) -> Response {
    let shared = &service.0;
    let (id, entry) = match shared.session(&jar).await {
        Ok(entry) => entry,
        Err(error) => return private_response((shared.clear(jar, false), error).into_response()),
    };
    let original = match shared.context() {
        Ok(ctx) => ctx,
        Err(error) => return error.into_response(),
    };
    let work = async {
        let credentials = shared.credentials(&entry, &original).await?;
        let flow = shared.flow(entry.provider)?;
        let ctx = BrokerRequestContext::new(&original);
        let caller = flow
            .verify_access(&entry.registered, credentials.access_token(), ctx)
            .await?;
        caller.check(&shared.providers[entry.provider].1, ctx)?;
        if entry.cancelled.is_cancelled()
            || entry.expires <= Instant::now()
            || shared.stop.is_cancelled()
        {
            return Err(Failure::Unauthorized);
        }
        Ok(Identity {
            provider: shared.providers[entry.provider].0.clone(),
            subject: caller.subject().to_owned(),
            username: caller.username().map(str::to_owned),
            expires_at: credentials
                .expires_at()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Failure::Invalid)?
                .as_secs(),
        })
    };
    let result = tokio::select! { _ = shared.stop.cancelled() => { original.cancellation().cancel(); Err(Failure::Unavailable) }, result = work => result };
    match result {
        Ok(identity) => private_response(Json(identity).into_response()),
        Err(error) => {
            let invalidate = invalidates(&error);
            if invalidate {
                shared.invalidate(&id, &entry).await;
                private_response((shared.clear(jar, false), error).into_response())
            } else {
                error.into_response()
            }
        }
    }
}
pub(super) async fn logout(
    State(service): State<BrowserAuth>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Response {
    let shared = &service.0;
    if let Err(error) = shared.origin(&headers) {
        return error.into_response();
    }
    let jar = shared.clear(jar, false);
    let id = match shared.handle(&CookieJar::from_headers(&headers), false) {
        Ok(id) => id,
        Err(_) => return private_response((jar, StatusCode::NO_CONTENT).into_response()),
    };
    let entry = shared.sessions.lock().await.remove(&id);
    let Some(entry) = entry else {
        return private_response((jar, StatusCode::NO_CONTENT).into_response());
    };
    entry.cancelled.cancel();
    let original = match shared.context() {
        Ok(ctx) => ctx,
        Err(error) => return private_response((jar, error).into_response()),
    };
    let work = async {
        let credentials = entry.credentials.lock().await;
        let Some(refresh) = credentials.refresh_token() else {
            return Ok(());
        };
        let flow = shared.flow(entry.provider)?;
        match flow
            .revoke(
                &entry.registered,
                refresh,
                BrokerRequestContext::new(&original),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == NativeErrorKind::Unsupported => Ok(()),
            Err(error) => Err(Failure::from(error)),
        }
    };
    let result = tokio::select! { _ = shared.stop.cancelled() => { original.cancellation().cancel(); Err(Failure::Unavailable) }, result = work => result };
    private_response(match result {
        Ok(()) => (jar, StatusCode::NO_CONTENT).into_response(),
        Err(error) => (jar, error).into_response(),
    })
}

pub(super) fn invalidates(error: &Failure) -> bool {
    match error {
        Failure::Unauthorized => true,
        Failure::Broker(BrokerError::InvalidToken | BrokerError::InvalidPolicy) => true,
        Failure::Native(error) => {
            error.effect() == RemoteEffect::MayHaveApplied
                || matches!(
                    error.kind(),
                    NativeErrorKind::InvalidRotation
                        | NativeErrorKind::Broker(
                            BrokerError::InvalidToken | BrokerError::InvalidPolicy
                        )
                )
        }
        _ => false,
    }
}
