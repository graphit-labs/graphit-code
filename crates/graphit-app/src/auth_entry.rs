//! Local profile administration against one owned private installation.
use crate::auth_cli::{self, Command, CommandResult};
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ServiceAdmissionLimits,
    ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::ProfileLimits,
    profile_manager::{NativeProfileManager, ProfileManagerAuthority},
};
use graphit_core::{
    auth_document::AuthDocumentLimits, file_lock::Cancellation, project_paths::InstallationPaths,
};
use graphit_daemon::auth_startup::{DaemonAuth, StartupPolicy};
use graphit_hub::policy::{HubResourcePolicy, RequestContext, ResourceSetting};
use std::{
    io::{self, Write},
    process::ExitCode,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) fn remote_effect_description(effect: Option<&str>) -> &'static str {
    match effect {
        Some("NotSent") => "; remote request was not sent",
        Some("MayHaveApplied") => {
            "; remote changes may have applied; inspect the result before retrying"
        }
        Some(_) => "; remote effect unknown; inspect the result before retrying",
        None => "",
    }
}

fn number(policy: &HubResourcePolicy, setting: ResourceSetting) -> io::Result<usize> {
    usize::try_from(policy.limit(setting).value).map_err(io::Error::other)
}
fn startup(policy: &HubResourcePolicy) -> io::Result<StartupPolicy> {
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
        document: AuthDocumentLimits::new(bytes).map_err(io::Error::other)?,
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
                max_scope_bytes: bytes,
                max_parent_bearer_bytes: scalar,
            },
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: entries,
                    max_binding_bytes: bytes,
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

fn remove_runtime(auth: &DaemonAuth, policy: &HubResourcePolicy) -> io::Result<()> {
    let cancellation = Cancellation::default();
    let original = policy
        .request_context(Instant::now(), &cancellation)
        .map_err(io::Error::other)?;
    auth.remove_runtime(original.snapshot())
        .map_err(io::Error::other)?;
    Ok(())
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
) -> io::Result<ExitCode> {
    original.snapshot().check()?;
    let auth =
        DaemonAuth::start(paths, &[], startup(policy)?, original).map_err(io::Error::other)?;
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
                    let effect = match error.remote_effect() {
                        Some(graphit_auth::broker::oauth::RemoteEffect::NotSent) => Some("NotSent"),
                        Some(graphit_auth::broker::oauth::RemoteEffect::MayHaveApplied) => {
                            Some("MayHaveApplied")
                        }
                        None => None,
                    };
                    io::Error::other(format!("{error}{}", remote_effect_description(effect)))
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
                let status = match publication {
                    graphit_core::auth_document::publication::AuthPublicationEffect::Applied => {
                        "applied"
                    }
                    graphit_core::auth_document::publication::AuthPublicationEffect::Unchanged => {
                        "unchanged"
                    }
                };
                serde_json::to_writer(&mut *output, &serde_json::json!({"status":status}))?;
                writeln!(output)?;
                Ok(ExitCode::SUCCESS)
            }
            Err(error) => {
                writeln!(diagnostic, "error: authentication: {error}")?;
                Ok(ExitCode::FAILURE)
            }
        }
    })();
    match (result, remove_runtime(&auth, policy)) {
        (Ok(exit), Ok(())) => Ok(exit),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(removal)) => Err(io::Error::other(format!(
            "{error}; runtime removal: {removal}"
        ))),
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
        let _error = run(
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
        assert_eq!(
            std::fs::read(paths.root().join("daemon/mcp.key")).unwrap(),
            key
        );
        assert_eq!(std::fs::read(paths.auth_file()).ok(), auth_bytes);
        remove_runtime(&auth, &policy).unwrap();
        drop(auth);
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
