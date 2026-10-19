//! Local profile administration against one owned private installation.
use crate::auth_cli::{self, Command, CommandResult};
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ServiceAdmissionLimits,
    ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::ProfileLimits,
    profile_manager::{NativeProfileManager, ProfileManagerAuthority},
};
use graphit_core::{
    artifact_store::ScopeLimits,
    auth_document::{AuthDocumentLimits, publication::PendingCleanup},
    file_lock::{Cancellation, FileLock},
    installation_bootstrap::BootstrapLimits,
    native_path::MAX_PATH_BYTES,
    project_paths::InstallationPaths,
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner, RuntimeKeyRecovery},
};
use graphit_daemon::auth_startup::{DaemonAuth, StartupPolicy, StartupResources};
use graphit_hub::policy::{HubResourcePolicy, RequestContext, ResourceSetting};
use std::{
    fmt,
    io::{self, Write},
    process::ExitCode,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

enum Resources {
    Auth(Box<(DaemonAuth, Option<PendingCleanup>)>),
    Runtime(Arc<RuntimeKeyOwner>, FileLock),
    Recovery(Box<(RuntimeKeyRecovery, FileLock)>),
}

/// Failed cleanup retains the native owner and singleton for an explicit continuation.
#[must_use]
pub struct AuthEntryFailure {
    cause: io::Error,
    cleanup: Option<io::Error>,
    resources: Option<Resources>,
}
impl AuthEntryFailure {
    pub fn cause(&self) -> &io::Error {
        &self.cause
    }
    pub fn cleanup_failure(&self) -> Option<&io::Error> {
        self.cleanup.as_ref()
    }
    pub fn has_resources(&self) -> bool {
        self.resources.is_some()
    }
    /// Reattempt only owned cleanup, never replay the command or a document CAS.
    pub fn cleanup(mut self, policy: &HubResourcePolicy) -> Result<(), Self> {
        let Some(resources) = self.resources.take() else {
            return Ok(());
        };
        match cleanup(resources, policy) {
            Ok(()) => Ok(()),
            Err((resources, error)) => {
                self.cleanup = Some(error);
                self.resources = resources;
                Err(self)
            }
        }
    }
}
impl fmt::Debug for AuthEntryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthEntryFailure")
            .field("cause", &self.cause)
            .field("cleanup", &self.cleanup)
            .field("has_resources", &self.has_resources())
            .finish()
    }
}
impl fmt::Display for AuthEntryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cause.fmt(f)?;
        if let Some(error) = &self.cleanup {
            write!(f, "; cleanup: {error}")?;
        }
        Ok(())
    }
}
impl std::error::Error for AuthEntryFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
impl From<io::Error> for AuthEntryFailure {
    fn from(cause: io::Error) -> Self {
        Self {
            cause,
            cleanup: None,
            resources: None,
        }
    }
}

fn number(policy: &HubResourcePolicy, setting: ResourceSetting) -> io::Result<usize> {
    usize::try_from(policy.limit(setting).value).map_err(io::Error::other)
}
fn startup(policy: &HubResourcePolicy) -> io::Result<StartupPolicy> {
    let depth = number(policy, ResourceSetting::SnapshotDepth)?;
    let paths = number(policy, ResourceSetting::SnapshotPathBytes)?;
    let bytes = number(policy, ResourceSetting::ContentInputBytes)?;
    let scalar = number(policy, ResourceSetting::ContentScalarBytes)?;
    let entries = number(policy, ResourceSetting::ContentCollectionEntries)?;
    let admin = [Permission::ManageAuthProfiles];
    let origin = |administrative| SourceAuthorityPolicy {
        ordinary: &[],
        administrative,
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(15 * 60),
            maximum: Duration::from_secs(60 * 60),
        },
    };
    Ok(StartupPolicy {
        bootstrap: BootstrapLimits::new(MAX_PATH_BYTES, depth).map_err(io::Error::other)?,
        runtime: RuntimeKeyLimits::new(MAX_PATH_BYTES, depth, scalar).map_err(io::Error::other)?,
        scope: ScopeLimits::new(1, MAX_PATH_BYTES, depth)?,
        document: AuthDocumentLimits::new(bytes, MAX_PATH_BYTES, depth, scalar)
            .map_err(io::Error::other)?,
        profiles: ProfileLimits {
            bytes,
            nodes: number(policy, ResourceSetting::ContentNodes)?,
            scalar_bytes: scalar,
            collection_entries: entries,
            providers: entries,
            profiles: entries,
        },
        authority: IssuerAuthorityPolicy::new(
            origin(&admin),
            origin(&[]),
            IssuerLimits {
                max_permissions: entries,
                max_scope_bytes: paths,
                max_parent_bearer_bytes: scalar,
            },
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: entries,
                    max_binding_bytes: paths,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .map_err(io::Error::other)?,
        ttl: TtlPolicy {
            ttl: Duration::from_secs(30 * 60),
            maximum: Duration::from_secs(24 * 60 * 60),
            refresh_margin: Duration::from_secs(60),
        },
    })
}

fn cleanup(
    resources: Resources,
    policy: &HubResourcePolicy,
) -> Result<(), (Option<Resources>, io::Error)> {
    let cancellation = Cancellation::default();
    let original = match policy.request_context(Instant::now(), &cancellation) {
        Ok(original) => original,
        Err(error) => return Err((Some(resources), io::Error::other(error))),
    };
    match resources {
        Resources::Auth(owned) => {
            let (auth, pending) = *owned;
            if pending.is_some() {
                let mut pending = pending;
                let cleaned = (|| -> io::Result<()> {
                    let key = auth
                        .runtime()
                        .check(original.snapshot())
                        .map_err(io::Error::other)?;
                    let raw = std::str::from_utf8(
                        key.secret(original.snapshot()).map_err(io::Error::other)?,
                    )
                    .map_err(io::Error::other)?;
                    let caller = auth
                        .issuer()
                        .authenticate_runtime(raw, &original)
                        .map_err(io::Error::other)?;
                    let session = auth
                        .issuer()
                        .bootstrap_admin(&caller)
                        .map_err(io::Error::other)?;
                    let parent = auth
                        .issuer()
                        .admin_parent(&caller, &session)
                        .map_err(io::Error::other)?;
                    let manager = NativeProfileManager::new(auth.issuer());
                    let proof = pending
                        .take()
                        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
                    let recovered = match manager
                        .cleanup_pending(ProfileManagerAuthority::Admin(&parent), proof)
                    {
                        Ok(recovered) => recovered,
                        Err(error) => {
                            let cause = io::Error::other(error.error());
                            pending = Some(error.into_pending_cleanup());
                            return Err(cause);
                        }
                    };
                    if let Some(error) = recovered.failure() {
                        pending = recovered.into_pending_cleanup();
                        return Err(io::Error::other(error));
                    }
                    Ok(())
                })();
                if let Err(error) = cleaned {
                    return Err((Some(Resources::Auth(Box::new((auth, pending)))), error));
                }
            }
            let removed = auth.remove_runtime(original.snapshot());
            if let Some(error) = removed.failure().or(removed.cleanup_failure()) {
                return Err((
                    Some(Resources::Auth(Box::new((auth, None)))),
                    io::Error::other(format!(
                        "runtime removal: {error}; effects={:?}",
                        removed.effects()
                    )),
                ));
            }
            Ok(())
        }
        Resources::Runtime(owner, singleton) => {
            let discarded = owner.discard_pending(original.snapshot());
            if let Some(error) = discarded.failure().or(discarded.cleanup_failure()) {
                return Err((
                    Some(Resources::Runtime(owner, singleton)),
                    io::Error::other(error),
                ));
            }
            let removed = owner.remove(original.snapshot());
            if let Some(error) = removed.failure().or(removed.cleanup_failure()) {
                return Err((
                    Some(Resources::Runtime(owner, singleton)),
                    io::Error::other(error),
                ));
            }
            Ok(())
        }
        Resources::Recovery(owned) => {
            let (recovery, singleton) = *owned;
            let discarded = recovery.discard(original.snapshot());
            if let Some(error) = discarded.failure() {
                let resources = discarded
                    .into_recovery()
                    .map(|recovery| Resources::Recovery(Box::new((recovery, singleton))));
                return Err((resources, io::Error::other(error)));
            }
            Ok(())
        }
    }
}

/// Executes installation-owned profile commands, including Native Broker login.
/// A live daemon owning this installation is a causal singleton error; no key is adopted.
pub fn run(
    command: Command,
    paths: &InstallationPaths,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
    output: &mut dyn Write,
    diagnostic: &mut dyn Write,
    mut announce: impl FnMut(&str) -> io::Result<()>,
) -> Result<ExitCode, AuthEntryFailure> {
    original.snapshot().check()?;
    let auth = match DaemonAuth::start(paths, &[], startup(policy)?, original) {
        Ok(auth) => auth,
        Err(failure) => {
            let cause = io::Error::other(format!("authentication startup: {:?}", failure.cause()));
            let resources = failure
                .into_resources()
                .and_then(|resources| match resources {
                    StartupResources::RuntimeOwner { owner, singleton } => {
                        Some(Resources::Runtime(owner, singleton))
                    }
                    StartupResources::RuntimeStart { outcome, singleton } => outcome
                        .into_recovery()
                        .map(|recovery| Resources::Recovery(Box::new((recovery, singleton)))),
                });
            let mut failure = AuthEntryFailure::from(cause);
            if let Some(resources) = resources
                && let Err((resources, error)) = cleanup(resources, policy)
            {
                failure.resources = resources;
                failure.cleanup = Some(error);
            }
            return Err(failure);
        }
    };
    let mut pending = None;
    let result = (|| -> io::Result<ExitCode> {
        let key = auth
            .runtime()
            .check(original.snapshot())
            .map_err(io::Error::other)?;
        let raw = std::str::from_utf8(key.secret(original.snapshot()).map_err(io::Error::other)?)
            .map_err(io::Error::other)?;
        let caller = auth
            .issuer()
            .authenticate_runtime(raw, original)
            .map_err(io::Error::other)?;
        let session = auth
            .issuer()
            .bootstrap_admin(&caller)
            .map_err(io::Error::other)?;
        let parent = auth
            .issuer()
            .admin_parent(&caller, &session)
            .map_err(io::Error::other)?;
        let manager = NativeProfileManager::new(auth.issuer());
        let recovered = manager
            .recover_stage(ProfileManagerAuthority::Admin(&parent))
            .map_err(io::Error::other)?;
        if let Some(error) = recovered.failure() {
            pending = recovered.into_pending_cleanup();
            return Err(io::Error::other(error));
        }
        let clock = || {
            i64::try_from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(io::Error::other)?
                    .as_secs(),
            )
            .map_err(io::Error::other)
        };
        let selected = match &command {
            Command::Login(input) => Some(input.provider.clone()),
            Command::Refresh(_) => {
                let snapshot = manager
                    .begin(ProfileManagerAuthority::Admin(&parent))
                    .map_err(io::Error::other)?;
                let active = snapshot
                    .active_name()
                    .ok_or_else(|| io::Error::other("no active profile"))?;
                Some(
                    snapshot
                        .profiles()
                        .find(|profile| profile.name() == active)
                        .ok_or_else(|| io::Error::other("active profile not found"))?
                        .provider()
                        .to_owned(),
                )
            }
            _ => None,
        };
        let broker = if let Some(name) = selected {
            let snapshot = manager
                .begin(ProfileManagerAuthority::Admin(&parent))
                .map_err(io::Error::other)?;
            let provider = snapshot
                .providers()
                .find(|p| p.name() == name)
                .ok_or_else(|| io::Error::other("provider not found"))?;
            match &provider.config().kind {
                graphit_auth::profile::ProviderKind::Broker(service) => {
                    Some((service.clone(), provider.revision()))
                }
                _ if matches!(&command, Command::Login(input) if input.device) => {
                    return Err(io::Error::other("--device requires a Broker provider"));
                }
                _ => None,
            }
        } else {
            None
        };
        let execution = if let Some(selected) = broker {
            let verifier = crate::admin_auth::verifier(policy.content(), &selected.0, selected.1)?;
            let oauth = graphit_auth::broker::oauth::NativeOAuth::new(
                &verifier,
                crate::admin_auth::oauth_limits(policy.content()),
            )
            .map_err(io::Error::other)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async {
                let result = match &command {
                    Command::Login(input) => {
                        if input.device {
                            auth_cli::device::login(
                                input,
                                &manager,
                                ProfileManagerAuthority::Admin(&parent),
                                &oauth,
                                |prompt| {
                                    writeln!(
                                        diagnostic,
                                        "Open {} and enter code {}",
                                        prompt.verification_uri(),
                                        prompt.user_code()
                                    )?;
                                    announce(
                                        prompt
                                            .verification_uri_complete()
                                            .unwrap_or(prompt.verification_uri()),
                                    )
                                },
                                clock,
                            )
                            .await
                        } else {
                            let metadata = verifier
                                .metadata(graphit_auth::broker::BrokerRequestContext::new(
                                    original.snapshot(),
                                ))
                                .await
                                .map_err(io::Error::other)?;
                            let content = policy.content();
                            auth_cli::broker::login(
                                input,
                                &manager,
                                ProfileManagerAuthority::Admin(&parent),
                                &oauth,
                                graphit_auth::profile_manager::InteractiveLoginConfig {
                                    bind: "127.0.0.1:0".parse().map_err(io::Error::other)?,
                                    callback_path: metadata.redirect_path(),
                                    limits: graphit_auth::profile_manager::InteractiveLoginLimits {
                                        head_bytes: content.max_input_bytes,
                                        header_entries: content.max_collection_entries,
                                        target_bytes: content.max_scalar_bytes,
                                        callback_bytes: content.max_input_bytes,
                                    },
                                },
                                &mut announce,
                                clock,
                            )
                            .await
                        }
                    }
                    Command::Refresh(input) => {
                        auth_cli::broker::refresh(
                            input,
                            &manager,
                            ProfileManagerAuthority::Admin(&parent),
                            &oauth,
                            clock()?,
                        )
                        .await
                    }
                    _ => unreachable!("only login and refresh select a Broker"),
                };
                result.map(CommandResult::Publication).map_err(|error| {
                    io::Error::other(format!(
                        "{error}; remote_effect={:?}",
                        error.remote_effect()
                    ))
                })
            })
        } else {
            auth_cli::execute(
                command,
                &manager,
                ProfileManagerAuthority::Admin(&parent),
                clock()?,
            )
            .map_err(io::Error::other)
        };
        match execution {
            Ok(CommandResult::Read(value)) => {
                original.snapshot().check()?;
                serde_json::to_writer(&mut *output, &value)?;
                output.write_all(b"\n")?;
                original.snapshot().check()?;
                Ok(ExitCode::SUCCESS)
            }
            Ok(CommandResult::Publication(publication)) => {
                let effect = publication.effect();
                let failure = publication.failure();
                let cleanup_failure = publication.cleanup_failure();
                let cleanup_effect = publication.cleanup_effect();
                pending = publication.into_pending_cleanup();
                writeln!(output, "{effect:?}")?;
                if failure.is_some() || cleanup_failure.is_some() || pending.is_some() {
                    writeln!(
                        diagnostic,
                        "authentication publication: effect={effect:?}; failure={failure:?}; cleanup={cleanup_effect:?}; cleanup_failure={cleanup_failure:?}; pending={}",
                        pending.is_some()
                    )?;
                    Ok(ExitCode::FAILURE)
                } else {
                    Ok(ExitCode::SUCCESS)
                }
            }
            Err(error) => {
                writeln!(diagnostic, "authentication: {error}")?;
                Ok(ExitCode::FAILURE)
            }
        }
    })();
    let cleanup = cleanup(Resources::Auth(Box::new((auth, pending))), policy);
    match (result, cleanup) {
        (Ok(exit), Ok(())) => Ok(exit),
        (Err(error), Ok(())) => Err(error.into()),
        (result, Err((resources, cleanup))) => Err(AuthEntryFailure {
            cause: result
                .err()
                .unwrap_or_else(|| io::Error::other("authentication cleanup failed")),
            cleanup: Some(cleanup),
            resources,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{brand::Brand, config::ConfigSnapshot};

    #[test]
    fn existing_daemon_rejects_administration_without_touching_its_private_files() {
        let directory = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let config = ConfigSnapshot::new(
            paths.brand(),
            [HubResourcePolicy::distribution_defaults(paths.brand()).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancellation = Cancellation::default();
        let original = policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        let auth = DaemonAuth::start(&paths, &[], startup(&policy).unwrap(), &original).unwrap();
        let key = std::fs::read(paths.root().join("daemon/mcp.key")).unwrap();
        let auth_bytes = std::fs::read(paths.auth_file()).ok();
        let error = run(
            Command::Provider(Box::new(auth_cli::ProviderArguments {
                command: auth_cli::ProviderCommand::List,
            })),
            &paths,
            &policy,
            &original,
            &mut Vec::new(),
            &mut Vec::new(),
            |_| panic!("unexpected Broker authorization URL"),
        )
        .unwrap_err();
        assert!(!error.has_resources());
        assert_eq!(
            std::fs::read(paths.root().join("daemon/mcp.key")).unwrap(),
            key
        );
        assert_eq!(std::fs::read(paths.auth_file()).ok(), auth_bytes);
        cleanup(Resources::Auth(Box::new((auth, None))), &policy)
            .unwrap_or_else(|(_, error)| panic!("{error}"));
        let original = policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        assert_eq!(
            run(
                Command::Provider(Box::new(auth_cli::ProviderArguments {
                    command: auth_cli::ProviderCommand::List
                })),
                &paths,
                &policy,
                &original,
                &mut Vec::new(),
                &mut Vec::new(),
                |_| panic!("unexpected Broker authorization URL"),
            )
            .unwrap(),
            ExitCode::SUCCESS
        );
    }
}
