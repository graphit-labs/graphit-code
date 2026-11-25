use super::*;
use crate::{
    profile::{ProfileCredentials, ProviderKind},
    store::AuthStoreError,
};

#[cfg(test)]
thread_local! {
    pub(super) static BEFORE_LOCAL_OBSERVATION_LOCK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
    pub(super) static BEFORE_SOURCE_STATE_LOCK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
    pub(super) static AFTER_LOCAL_ADMISSION_INSPECTION: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Origin {
    Runtime,
    Local,
}
impl Origin {
    pub(super) fn tag(self) -> u8 {
        match self {
            Self::Runtime => 0,
            Self::Local => 1,
        }
    }
}
pub(super) struct LocalState {
    pub generation: u64,
    pub binding: Option<Zeroizing<Id>>,
}
pub(super) struct LocalProof {
    original: Arc<graphit_core::file_io::SnapshotContext>,
    binding: Zeroizing<Id>,
    generation: u64,
    task_principal: Id,
}
pub(super) enum SourceProof<'a> {
    Runtime {
        key: RuntimeKeyBorrow<'a>,
        digest: Zeroizing<Id>,
    },
    Local(Arc<LocalProof>),
}
impl SourceProof<'_> {
    pub(super) fn origin(&self) -> Origin {
        match self {
            Self::Runtime { .. } => Origin::Runtime,
            Self::Local(_) => Origin::Local,
        }
    }
    pub(super) fn generation(&self) -> u64 {
        match self {
            Self::Runtime { key, .. } => key.generation(),
            Self::Local(p) => p.generation,
        }
    }
    pub(super) fn check_credentials(
        &self,
        issuer: &RoleIssuer,
        request: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        request.check().map_err(|_| AuthError::Request)?;
        match self {
            Self::Runtime { key, digest } => {
                let current = Zeroizing::<Id>::new(
                    Sha256::digest(key.secret(request.snapshot()).map_err(runtime_error)?).into(),
                );
                if !bool::from(digest.as_slice().ct_eq(current.as_slice())) {
                    return Err(AuthError::Denied);
                }
            }
            Self::Local(proof) => {
                if !proof.original.same_request(request.snapshot()) {
                    return Err(AuthError::Request);
                }
                #[cfg(test)]
                BEFORE_LOCAL_OBSERVATION_LOCK.with(|slot| {
                    if let Some(hook) = slot.borrow_mut().take() {
                        hook();
                    }
                });
                issuer.check_local_binding(&proof.binding, request)?;
            }
        }
        request.check().map(|_| ()).map_err(|_| AuthError::Request)
    }
    pub(super) fn check_locked(&self, state: &State) -> Result<(), AuthError> {
        if self.generation() != source_generation(state, self.origin()) {
            return Err(AuthError::Denied);
        }
        if let Self::Local(proof) = self
            && state.local.binding.as_ref().is_none_or(|binding| {
                !bool::from(binding.as_slice().ct_eq(proof.binding.as_slice()))
            })
        {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
    pub(super) fn update_actor(&self, hash: &mut Sha256) {
        hash.update([self.origin().tag()]);
        hash.update(self.generation().to_be_bytes());
        if let Self::Local(p) = self {
            hash.update(p.binding.as_slice());
        }
    }
    pub(super) fn update_task_principal(&self, hash: &mut Sha256) {
        hash.update([self.origin().tag()]);
        if let Self::Local(proof) = self {
            hash.update(proof.task_principal);
        }
    }
}
pub(super) fn source_generation(state: &State, origin: Origin) -> u64 {
    match origin {
        Origin::Runtime => state.runtime_generation,
        Origin::Local => state.local.generation,
    }
}
impl RoleIssuer {
    pub(super) fn origin_policy(&self, origin: Origin) -> &crate::policy::OriginPolicy {
        match origin {
            Origin::Runtime => &self.policy.runtime,
            Origin::Local => &self.policy.local,
        }
    }
    pub(super) fn capture_source<'a>(
        &'a self,
        origin: Origin,
        raw: Option<&str>,
        request: &'a RequestContext<'a>,
    ) -> Result<SourceProof<'a>, AuthError> {
        request.check().map_err(|_| AuthError::Request)?;
        if raw.is_some_and(|r| r.len() > self.limits.max_parent_bearer_bytes) {
            return Err(AuthError::Limit);
        }
        match origin {
            Origin::Runtime => {
                let key = self
                    .runtime
                    .check(request.snapshot())
                    .map_err(runtime_error)?;

                let digest: Zeroizing<Id> = Zeroizing::new(
                    Sha256::digest(key.secret(request.snapshot()).map_err(runtime_error)?).into(),
                );
                if let Some(raw) = raw {
                    let submitted = Zeroizing::<Id>::new(Sha256::digest(raw.as_bytes()).into());
                    if !bool::from(submitted.as_slice().ct_eq(digest.as_slice())) {
                        return Err(AuthError::Denied);
                    }
                }
                let generation = key.generation();
                #[cfg(test)]
                BEFORE_SOURCE_STATE_LOCK.with(|slot| {
                    if let Some(hook) = slot.borrow_mut().take() {
                        hook();
                    }
                });
                let mut state = self.lock()?;
                request.check().map_err(|_| AuthError::Request)?;
                if raw.is_some() {
                    self.observe_generation(&mut state, generation)?;
                } else {
                    if generation < state.runtime_generation {
                        return Err(AuthError::Denied);
                    }
                    state.runtime_generation = generation;
                }
                drop(state);
                Ok(SourceProof::Runtime { key, digest })
            }
            Origin::Local => {
                let (_guard, snapshot) = self.local.load_locked(request).map_err(store_error)?;
                let document = snapshot.document();
                let qualified = local_qualification(document, self.limits, request);
                drop(snapshot);
                drop(_guard);

                #[cfg(test)]
                AFTER_LOCAL_ADMISSION_INSPECTION.with(|slot| {
                    if let Some(hook) = slot.borrow_mut().take() {
                        hook();
                    }
                });
                if let Some((binding, _, _)) = &qualified {
                    self.check_local_binding(binding, request)?;
                }
                request.check().map_err(|_| AuthError::Request)?;
                #[cfg(test)]
                BEFORE_SOURCE_STATE_LOCK.with(|slot| {
                    if let Some(hook) = slot.borrow_mut().take() {
                        hook();
                    }
                });
                let mut state = self.lock()?;
                request.check().map_err(|_| AuthError::Request)?;
                let changed = match (&qualified, &state.local.binding) {
                    (Some((binding, _, _)), Some(previous)) => {
                        !bool::from(binding.as_slice().ct_eq(previous.as_slice()))
                    }
                    (None, None) => false,
                    _ => true,
                };
                if changed {
                    state.local.generation = state
                        .local
                        .generation
                        .checked_add(1)
                        .ok_or(AuthError::State)?;
                    state.local.binding = qualified.as_ref().map(|(b, _, _)| Zeroizing::new(**b));
                }
                let generation = state.local.generation;
                if raw.is_some() {
                    prune(&mut state, self.clock.now());
                }
                drop(state);
                let (binding, key_digest, task_principal) = qualified.ok_or(AuthError::Denied)?;
                if let Some(raw) = raw {
                    if raw.is_empty() {
                        return Err(AuthError::Denied);
                    }
                    let submitted = Zeroizing::<Id>::new(Sha256::digest(raw.as_bytes()).into());
                    if !bool::from(submitted.as_slice().ct_eq(key_digest.as_slice())) {
                        return Err(AuthError::Denied);
                    }
                }
                self.check_local_binding(&binding, request)?;

                request.check().map_err(|_| AuthError::Request)?;
                Ok(SourceProof::Local(Arc::new(LocalProof {
                    original: Arc::clone(request.snapshot()),
                    binding,
                    generation,
                    task_principal,
                })))
            }
        }
    }
}
fn local_qualification(
    document: &crate::profile::AuthDocument,
    limits: IssuerLimits,
    request: &RequestContext<'_>,
) -> Option<(Zeroizing<Id>, Zeroizing<Id>, Id)> {
    document
        .active(request)
        .ok()
        .and_then(|(provider, profile)| {
            if !matches!(provider.config().kind, ProviderKind::Local { .. }) {
                return None;
            }
            let data = profile.data()?;
            let ProfileCredentials::Local(credentials) = &data.credentials else {
                return None;
            };
            let key = credentials.mcp_key.as_ref()?.expose();
            if key.is_empty() || key.len() > limits.max_parent_bearer_bytes {
                return None;
            }
            let key_digest = Zeroizing::<Id>::new(Sha256::digest(key.as_bytes()).into());
            let mut hash = Sha256::new();
            hash.update(b"graphit-local-source-v1\0");
            for value in [
                provider.name(),
                data.name.as_str(),
                data.provider.as_str(),
                data.username.as_str(),
            ] {
                hash_text(&mut hash, value);
            }
            match &data.organization {
                None => hash.update([0]),
                Some(v) => {
                    hash.update([1]);
                    hash_text(&mut hash, v);
                }
            }
            hash.update((data.teams.len() as u64).to_be_bytes());
            for team in &data.teams {
                hash_text(&mut hash, team);
            }
            hash.update([u8::from(credentials.anonymous)]);
            for v in [
                provider.revision(),
                profile.revision(),
                profile.token_revision(),
                profile.provider_revision(),
            ] {
                hash.update(v.to_be_bytes());
            }
            hash_text(&mut hash, document.active_name()?);
            hash.update(key_digest.as_slice());
            // Canonical identities came from the same active profile
            // whose current MCP key authenticates this caller. Keep
            // credential revisions/secrets out of durable attribution.
            let mut principal = Sha256::new();
            principal.update(b"graphit-local-task-principal-v1\0");
            hash_text(&mut principal, provider.name());
            hash_text(&mut principal, data.name.as_str());
            Some((
                Zeroizing::<Id>::new(hash.finalize().into()),
                key_digest,
                principal.finalize().into(),
            ))
        })
}
impl RoleIssuer {
    pub(super) fn check_broker_provider(
        &self,
        name: &str,
        caller: crate::broker::project_access::BrokerProjectCaller<'_>,
        verifier: &crate::broker::BrokerVerifier,
        original: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        original.check().map_err(|_| AuthError::Request)?;
        let current = self.local.load(original).map_err(store_error)?;
        let provider = current.document().provider(name).ok_or(AuthError::Denied)?;
        if !caller.matches_provider(verifier, provider) {
            return Err(AuthError::Denied);
        }
        original.check().map_err(|_| AuthError::Request)
    }
    fn check_local_binding(
        &self,
        expected: &Id,
        request: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        request.check().map_err(|_| AuthError::Request)?;
        let current = self.local.load(request).map_err(store_error)?;
        let (binding, _, _) = local_qualification(current.document(), self.limits, request)
            .ok_or(AuthError::Denied)?;
        if !bool::from(binding.as_slice().ct_eq(expected.as_slice())) {
            return Err(AuthError::Denied);
        }
        request.check().map_err(|_| AuthError::Request)
    }
}
fn hash_text(hash: &mut Sha256, value: &str) {
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value.as_bytes());
}
pub(super) fn store_error(error: AuthStoreError) -> AuthError {
    use graphit_core::auth_document::AuthDocumentError;
    match error {
        AuthStoreError::Core(AuthDocumentError::Limit | AuthDocumentError::Resource) => {
            AuthError::Limit
        }
        AuthStoreError::Core(AuthDocumentError::Cancelled | AuthDocumentError::Deadline) => {
            AuthError::Request
        }
        AuthStoreError::Core(AuthDocumentError::Io) => AuthError::Io,
        _ => AuthError::Denied,
    }
}
