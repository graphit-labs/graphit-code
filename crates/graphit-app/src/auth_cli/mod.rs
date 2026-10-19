//! Installation-scoped Auth command component. The caller supplies genuine profile authority.
mod args;
pub mod broker;
pub mod device;
pub use args::*;
use graphit_auth::{
    profile::{
        AiService, LocalCredentials, LocalS3, ProfileExpectation, ProviderConfig, ProviderKind,
        Secret,
    },
    profile_manager::{
        LocalProfileInput, NativeProfileManager, ProfileManagerAuthority, ProfileManagerError,
    },
};
use graphit_core::auth_document::publication::AuthDocumentPublication;
use serde_json::{Value, json};
use std::fmt;

/// Physical publications remain owned by the caller, including pending cleanup and failures.
#[must_use]
pub enum CommandResult {
    Read(Value),
    Publication(AuthDocumentPublication),
}
#[derive(Debug)]
pub enum CommandError {
    Profile(ProfileManagerError),
    InvalidInput(&'static str),
    NotFound,
    Conflict,
}
impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Profile(error) => error.fmt(f),
            Self::InvalidInput(message) => f.write_str(message),
            Self::NotFound => f.write_str("provider or profile not found"),
            Self::Conflict => f.write_str("authentication document revision changed"),
        }
    }
}
impl std::error::Error for CommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Profile(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ProfileManagerError> for CommandError {
    fn from(error: ProfileManagerError) -> Self {
        Self::Profile(error)
    }
}
fn revision(actual: u64, expected: Option<u64>) -> Result<u64, CommandError> {
    if expected.is_some_and(|expected| expected != actual) {
        Err(CommandError::Conflict)
    } else {
        Ok(actual)
    }
}

/// Does not bootstrap an issuer, manufacture a parent or replace the authority's original request.
pub fn execute<'a>(
    command: Command,
    manager: &'a NativeProfileManager<'a>,
    authority: ProfileManagerAuthority<'a>,
    now: i64,
) -> Result<CommandResult, CommandError> {
    let snapshot = manager.begin(authority)?;
    let document_revision = snapshot.revision();
    let publication = match command {
        Command::Provider(arguments) => match arguments.command {
            ProviderCommand::List => {
                return Ok(CommandResult::Read(
                    json!({"revision": document_revision, "providers": snapshot.providers().map(|p| json!({"name": p.name(), "revision": p.revision(), "config": provider(p.config())})).collect::<Vec<_>>()}),
                ));
            }
            ProviderCommand::Show { name } => {
                let p = snapshot
                    .providers()
                    .find(|p| p.name() == name)
                    .ok_or(CommandError::NotFound)?;
                return Ok(CommandResult::Read(
                    json!({"revision": document_revision, "name": p.name(), "provider_revision": p.revision(), "created_at": p.created_at(), "updated_at": p.updated_at(), "config": provider(p.config())}),
                ));
            }
            ProviderCommand::Add {
                name,
                kind,
                fields,
                expected_revision,
            } => {
                let expected = revision(document_revision, expected_revision)?;
                let config = fields
                    .apply(Some(kind), None)
                    .map_err(CommandError::InvalidInput)?;
                manager.add_provider(snapshot, name, config, expected, now)?
            }
            ProviderCommand::Update {
                name,
                kind,
                fields,
                expected_revision,
            } => {
                let expected = revision(document_revision, expected_revision)?;
                let p = snapshot
                    .providers()
                    .find(|p| p.name() == name)
                    .ok_or(CommandError::NotFound)?;
                let provider_revision = p.revision();
                let config = fields
                    .apply(kind, Some(p.config()))
                    .map_err(CommandError::InvalidInput)?;
                manager.update_provider(
                    snapshot,
                    &name,
                    config,
                    expected,
                    provider_revision,
                    now,
                )?
            }
            ProviderCommand::Remove {
                name,
                cascade,
                yes,
                expected_revision,
            } => {
                if !yes {
                    return Err(CommandError::InvalidInput(
                        "provider removal requires --yes",
                    ));
                }
                let expected = revision(document_revision, expected_revision)?;
                manager.remove_provider(snapshot, &name, cascade, expected)?
            }
        },
        Command::Account(arguments) => match arguments.command {
            AccountCommand::List => {
                return Ok(CommandResult::Read(
                    json!({"revision": document_revision, "active": snapshot.active_name(), "profiles": snapshot.profiles().map(|p| profile(&p, snapshot.active_name())).collect::<Vec<_>>()}),
                ));
            }
            AccountCommand::Show { profile: name } => {
                let name = name.as_deref().or(snapshot.active_name());
                let data = match name {
                    Some(name) => profile(
                        &snapshot
                            .profiles()
                            .find(|p| p.name() == name)
                            .ok_or(CommandError::NotFound)?,
                        snapshot.active_name(),
                    ),
                    None => Value::Null,
                };
                return Ok(CommandResult::Read(
                    json!({"revision": document_revision, "profile": data}),
                ));
            }
            AccountCommand::Use {
                profile,
                expected_revision,
            } => {
                let expected = revision(document_revision, expected_revision)?;
                manager.activate(snapshot, &profile, expected)?
            }
        },
        Command::Login(input) => {
            if input.device {
                return Err(CommandError::InvalidInput(
                    "--device requires a Broker provider",
                ));
            }
            if input.s3_disabled {
                return Err(CommandError::InvalidInput(
                    "--s3-disabled requires native Broker login",
                ));
            }
            let expected = ProfileExpectation {
                document_revision: revision(document_revision, input.expected_revision)?,
                provider_revision: snapshot
                    .providers()
                    .find(|p| p.name() == input.provider)
                    .ok_or(CommandError::NotFound)?
                    .revision(),
                profile_revision: snapshot
                    .profiles()
                    .find(|p| p.name() == input.profile)
                    .map(|p| p.revision()),
            };
            if input.anonymous && input.username.is_some() {
                return Err(CommandError::InvalidInput(
                    "anonymous local login cannot supply a username",
                ));
            }
            let s3 = if input.s3_access_key.is_some()
                || input.s3_secret_key.is_some()
                || input.s3_session_token.is_some()
                || input.aws_profile.is_some()
            {
                Some(LocalS3 {
                    access_key_id: input.s3_access_key.map(Secret::new),
                    secret_access_key: input.s3_secret_key.map(Secret::new),
                    session_token: input.s3_session_token.map(Secret::new),
                    expires_at: input.s3_expires_at,
                    aws_profile: input.aws_profile,
                })
            } else {
                None
            };
            let username = if input.anonymous {
                "anonymous".into()
            } else {
                input.username.ok_or(CommandError::InvalidInput(
                    "local login requires --username or --anonymous",
                ))?
            };
            manager.install_local(
                snapshot,
                LocalProfileInput {
                    name: input.profile,
                    provider: input.provider,
                    username,
                    organization: input.organization,
                    teams: input.teams,
                    credentials: LocalCredentials {
                        anonymous: input.anonymous,
                        mcp_key: input.mcp_key.map(Secret::new),
                        broker_key: input.broker_key.map(Secret::new),
                        embedding_api_key: input.embedding_api_key.map(Secret::new),
                        rerank_api_key: input.rerank_api_key.map(Secret::new),
                        s3,
                    },
                },
                expected,
                now,
            )?
        }
        Command::Refresh(_) => {
            return Err(CommandError::InvalidInput(
                "refresh requires the native Broker executor",
            ));
        }
        Command::Logout(input) => {
            let expected = revision(document_revision, input.expected_revision)?;
            manager.logout(snapshot, input.profile.as_deref(), expected)?
        }
    };
    Ok(CommandResult::Publication(publication))
}

fn profile(p: &graphit_auth::profile_manager::ProfileView<'_>, active: Option<&str>) -> Value {
    json!({"name": p.name(), "provider": p.provider(), "username": p.username(), "organization": p.organization(), "teams": p.teams().map(|teams| teams.collect::<Vec<_>>()), "active": active == Some(p.name()), "revision": p.revision(), "provider_revision": p.provider_revision(), "token_revision": p.token_revision(), "created_at": p.created_at(), "updated_at": p.updated_at(), "broker_identity": p.broker_identity().map(|(issuer, subject)| json!({"issuer": endpoint(issuer), "subject": subject})), "anonymous": p.local_anonymous(), "s3_disabled": p.broker_s3_disabled()})
}
fn endpoint(raw: &str) -> Option<String> {
    let mut url = url::Url::parse(raw).ok()?;
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    Some(url.into())
}
fn service(value: &AiService) -> Value {
    match value {
        AiService::Disabled => json!({"mode": "disabled"}),
        AiService::Broker => json!({"mode": "broker"}),
        AiService::Local(execution) => {
            json!({"mode": "local", "device": match execution.device { graphit_auth::profile::LocalDevice::Auto => "auto", graphit_auth::profile::LocalDevice::Cpu => "cpu", graphit_auth::profile::LocalDevice::Cuda => "cuda", graphit_auth::profile::LocalDevice::Metal => "metal" }, "device_id": execution.device_id})
        }
        AiService::Direct {
            protocol,
            endpoint: raw,
            model,
            model_revision,
            recipe,
            dimensions,
            request_dimensions,
            batch_endpoint,
        } => {
            json!({"request_dimensions":request_dimensions,"batch_endpoint":batch_endpoint.as_deref().and_then(endpoint),"mode": "direct", "protocol": protocol_name(*protocol), "endpoint": endpoint(raw), "model": model, "model_revision":model_revision, "recipe":recipe, "dimensions": dimensions})
        }
    }
}
fn protocol_name(value: graphit_auth::profile::DirectProtocol) -> &'static str {
    use graphit_auth::profile::DirectProtocol::*;
    match value {
        OpenAi => "openai",
        OpenAiCompatible => "openai-compatible",
        Cohere => "cohere",
        CohereV2 => "cohere-v2",
        Voyage => "voyage",
        VoyageV1 => "voyage-v1",
        GoogleGemini1 => "google-gemini-1",
        GoogleGemini2 => "google-gemini-2",
        Jina => "jina",
        JinaV1 => "jina-v1",
    }
}
fn provider(config: &ProviderConfig) -> Value {
    let (kind, broker, allow_aws) = match &config.kind {
        ProviderKind::Local {
            allow_aws_credential_chain,
            broker,
        } => ("local", broker.as_ref(), Some(*allow_aws_credential_chain)),
        ProviderKind::Broker(broker) => ("broker", Some(broker), None),
    };
    json!({"type": kind, "allow_aws_credential_chain": allow_aws, "broker": broker.map(|b| json!({"endpoint": endpoint(&b.endpoint), "mcp_resource": b.mcp_resource.as_deref().and_then(endpoint), "allow_anonymous": b.allow_anonymous})), "embedding": service(&config.embedding), "rerank": service(&config.rerank), "s3": {"bucket": config.s3.bucket, "region": config.s3.region, "endpoint": config.s3.endpoint.as_deref().and_then(endpoint), "prefix": config.s3.prefix, "source": match config.s3.source { graphit_auth::profile::S3Source::LocalCredentials => "login", graphit_auth::profile::S3Source::AwsChain => "aws-chain", graphit_auth::profile::S3Source::BrokerScope => "broker" }}})
}

#[cfg(test)]
mod tests;
