//! Shared discovery and JWKS owners for the daemon's admitted Broker services.

use graphit_auth::{
    ServiceBindingView, ServiceMode, ServiceSelection, ServiceUseError,
    broker::{BrokerError, BrokerPolicy, BrokerVerifier, VerifierLimits},
    profile::{BrokerService, ProviderKind},
};
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    sync::{Arc, Mutex},
};

/// A verifier is cached data, never authentication or project authorization.
#[derive(Debug)]
pub enum BrokerVerifierCacheError {
    Service(ServiceUseError),
    Broker(BrokerError),
    Poisoned,
}

impl fmt::Display for BrokerVerifierCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Service(_) => "Broker service selection rejected",
            Self::Broker(_) => "Broker verifier configuration rejected",
            Self::Poisoned => "Broker verifier cache poisoned",
        })
    }
}
impl Error for BrokerVerifierCacheError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Service(error) => Some(error),
            Self::Broker(error) => Some(error),
            Self::Poisoned => None,
        }
    }
}
impl From<ServiceUseError> for BrokerVerifierCacheError {
    fn from(error: ServiceUseError) -> Self {
        Self::Service(error)
    }
}

struct Entry {
    revision: u64,
    endpoint: String,
    resource: Option<String>,
    verifier: Arc<BrokerVerifier>,
}

#[derive(Eq, PartialEq, Ord, PartialOrd)]
enum CacheKey {
    Provider(String),
    Native,
}
impl Entry {
    fn matches(&self, revision: u64, endpoint: &str, resource: Option<&str>) -> bool {
        self.revision == revision
            && self.endpoint == endpoint
            && self.resource.as_deref() == resource
    }
}

/// Retain one owner alongside the daemon's configured issuer and service consumers.
///
/// Selections must come from that issuer. Each provider retains only its current
/// verifier; replacement leaves an old `Arc` alive only while a caller uses it.
/// Callers still obtain and check `selection.native_credential(&verifier)` before
/// sending a request. This owner does not refresh tokens or grant PROJECT access.
pub struct BrokerVerifierCache {
    network: NetworkLimits,
    limits: VerifierLimits,
    allow_loopback_http: bool,
    proxy: Option<String>,
    additional_roots: Vec<Vec<u8>>,
    entries: Mutex<BTreeMap<CacheKey, Arc<Entry>>>,
}
impl BrokerVerifierCache {
    /// Capture explicit network settings once; environment variables are not read.
    pub fn new(
        network: NetworkLimits,
        limits: VerifierLimits,
        allow_loopback_http: bool,
        proxy: Option<&str>,
        additional_roots: &[&[u8]],
    ) -> Self {
        Self {
            network,
            limits,
            allow_loopback_http,
            proxy: proxy.map(str::to_owned),
            additional_roots: additional_roots.iter().map(|root| root.to_vec()).collect(),
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    /// Obtain a warm verifier only after checking the current admitted selection.
    ///
    /// ```no_run
    /// use graphit_auth::ServiceSelection;
    /// use graphit_daemon::broker_verifiers::{BrokerVerifierCache, BrokerVerifierCacheError};
    ///
    /// async fn check_native(
    ///     cache: &BrokerVerifierCache,
    ///     selection: &mut ServiceSelection<'_>,
    /// ) -> Result<(), BrokerVerifierCacheError> {
    ///     let verifier = cache.get(selection)?;
    ///     let credential = selection.native_credential(&verifier).await?;
    ///     credential.check()?;
    ///     Ok(())
    /// }
    /// ```
    pub fn get(
        &self,
        selection: &mut ServiceSelection<'_>,
    ) -> Result<Arc<BrokerVerifier>, BrokerVerifierCacheError> {
        if selection.mode()? != ServiceMode::BrokerNative {
            return Err(ServiceUseError::WrongMode.into());
        }
        if let Some(verifier) = self.cached(selection)? {
            selection.mode()?;
            return Ok(verifier);
        }
        // Strings are copied only on a miss, not on each warm request.
        let (provider, revision, mut endpoint, mut resource, inventory) =
            match selection.binding()? {
                ServiceBindingView::Broker {
                    provider,
                    endpoint,
                    resource,
                    provider_revision,
                    ..
                } => (
                    provider.to_owned(),
                    provider_revision,
                    endpoint.to_owned(),
                    resource.map(str::to_owned),
                    false,
                ),
                ServiceBindingView::BrokerInventory {
                    provider,
                    provider_revision,
                    ..
                } => (
                    provider.to_owned(),
                    provider_revision,
                    String::new(),
                    None,
                    true,
                ),
                _ => return Err(ServiceUseError::WrongMode.into()),
            };
        let ProviderKind::Broker(service) = &selection.provider_config()?.kind else {
            return Err(ServiceUseError::WrongMode.into());
        };
        if inventory {
            // Match the native_credential SDK's actual Inventory verifier policy.
            endpoint = service.endpoint.clone();
            resource = service.mcp_resource.clone();
        }
        selection.mode()?;
        let result = self.resolve(
            CacheKey::Provider(provider),
            revision,
            &endpoint,
            resource.as_deref(),
        )?;
        selection.mode()?;
        Ok(result)
    }

    /// Reuse discovery for an admitted Broker AI service in any credential mode.
    /// Public metadata grants no authority; the selection's real credential lease
    /// still authenticates and authorizes every service use.
    pub fn get_for_service(
        &self,
        selection: &mut ServiceSelection<'_>,
    ) -> Result<Arc<BrokerVerifier>, BrokerVerifierCacheError> {
        if selection.mode()? == ServiceMode::BrokerNative {
            return self.get(selection);
        }
        if !matches!(
            selection.mode()?,
            ServiceMode::BrokerKey | ServiceMode::BrokerIncoming | ServiceMode::BrokerAnonymous
        ) {
            return Err(ServiceUseError::WrongMode.into());
        }
        let (provider, revision, endpoint, resource) = match selection.binding()? {
            ServiceBindingView::Broker {
                provider,
                provider_revision,
                endpoint,
                resource,
                ..
            } => (
                provider.to_owned(),
                provider_revision,
                endpoint.to_owned(),
                resource.map(str::to_owned),
            ),
            _ => return Err(ServiceUseError::WrongMode.into()),
        };
        let configured = match &selection.provider_config()?.kind {
            ProviderKind::Broker(service)
            | ProviderKind::Local {
                broker: Some(service),
                ..
            } => service,
            _ => return Err(ServiceUseError::WrongMode.into()),
        };
        if configured.endpoint != endpoint || configured.mcp_resource != resource {
            return Err(ServiceUseError::WrongMode.into());
        }
        selection.mode()?;
        let result = self.resolve(
            CacheKey::Provider(provider),
            revision,
            &endpoint,
            resource.as_deref(),
        )?;
        selection.mode()?;
        Ok(result)
    }

    /// Resolve cached verifier data for a Runtime caller's current Native binding.
    /// This does not verify a token or grant access: the caller must still use
    /// `authenticate_native`, which rechecks the active profile and binding.
    pub fn get_native(
        &self,
        binding: &BrokerService,
        revision: u64,
    ) -> Result<Arc<BrokerVerifier>, BrokerVerifierCacheError> {
        self.resolve(
            CacheKey::Native,
            revision,
            &binding.endpoint,
            binding.mcp_resource.as_deref(),
        )
    }

    fn resolve(
        &self,
        key: CacheKey,
        revision: u64,
        endpoint: &str,
        resource: Option<&str>,
    ) -> Result<Arc<BrokerVerifier>, BrokerVerifierCacheError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| BrokerVerifierCacheError::Poisoned)?;
        if let Some(entry) = entries
            .get(&key)
            .filter(|entry| entry.matches(revision, endpoint, resource))
        {
            return Ok(Arc::clone(&entry.verifier));
        }
        let entry = if let Some(entry) = entries
            .values()
            .find(|entry| entry.matches(revision, endpoint, resource))
        {
            Arc::clone(entry)
        } else {
            let roots: Vec<&[u8]> = self.additional_roots.iter().map(Vec::as_slice).collect();
            let verifier = Arc::new(
                BrokerVerifier::new(
                    BrokerPolicy {
                        endpoint,
                        mcp_resource: resource,
                        provider_revision: revision,
                        network: ClientPolicy {
                            origin: endpoint,
                            allow_loopback_http: self.allow_loopback_http,
                            proxy: self.proxy.as_deref(),
                            additional_roots: &roots,
                        },
                    },
                    self.network,
                    self.limits,
                )
                .map_err(BrokerVerifierCacheError::Broker)?,
            );
            Arc::new(Entry {
                revision,
                endpoint: endpoint.to_owned(),
                resource: resource.map(str::to_owned),
                verifier,
            })
        };
        let verifier = Arc::clone(&entry.verifier);
        // Each provider and the active Native binding retain only their current
        // entry, sharing the same verifier and warm discovery/JWKS data.
        entries.insert(key, entry);
        Ok(verifier)
    }

    fn cached(
        &self,
        selection: &mut ServiceSelection<'_>,
    ) -> Result<Option<Arc<BrokerVerifier>>, BrokerVerifierCacheError> {
        let (entry, inventory) = {
            let binding = selection.binding()?;
            let entries = self
                .entries
                .lock()
                .map_err(|_| BrokerVerifierCacheError::Poisoned)?;
            match binding {
                ServiceBindingView::Broker {
                    provider,
                    endpoint,
                    resource,
                    provider_revision,
                    ..
                } => (
                    entries
                        .iter()
                        .find_map(|(key, entry)| match key {
                            CacheKey::Provider(name) if name == provider => Some(entry),
                            _ => None,
                        })
                        .filter(|entry| entry.matches(provider_revision, endpoint, resource))
                        .cloned(),
                    false,
                ),
                ServiceBindingView::BrokerInventory {
                    provider,
                    provider_revision,
                    ..
                } => (
                    entries
                        .iter()
                        .find_map(|(key, entry)| match key {
                            CacheKey::Provider(name) if name == provider => Some(entry),
                            _ => None,
                        })
                        .filter(|entry| entry.revision == provider_revision)
                        .cloned(),
                    true,
                ),
                _ => return Err(ServiceUseError::WrongMode.into()),
            }
        };
        let ProviderKind::Broker(service) = &selection.provider_config()?.kind else {
            return Err(ServiceUseError::WrongMode.into());
        };
        Ok(entry
            .filter(|entry| {
                !inventory
                    || entry.matches(
                        entry.revision,
                        &service.endpoint,
                        service.mcp_resource.as_deref(),
                    )
            })
            .map(|entry| Arc::clone(&entry.verifier)))
    }
}
