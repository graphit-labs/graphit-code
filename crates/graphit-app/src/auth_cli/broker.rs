//! Native Broker operations reuse the SDK's owned flow and the caller's original authority.
use super::{CommandError, LoginArguments, RefreshArguments, revision};
use graphit_auth::{
    broker::oauth::{NativeOAuth, RemoteEffect},
    profile_manager::{
        BrokerLoginError, BrokerLoginSelection, BrokerRefreshError, InteractiveLoginConfig,
        InteractiveLoginError, NativeProfileManager, ProfileManagerAuthority,
    },
};
use graphit_core::auth_document::publication::AuthDocumentPublication;
use std::{fmt, io};

#[derive(Debug)]
pub enum Error {
    Command(CommandError),
    Interactive(InteractiveLoginError),
    Login(BrokerLoginError),
    Refresh(BrokerRefreshError),
    Announce(io::Error),
    Clock(io::Error),
}
impl Error {
    /// Preserve the SDK's remote-effect diagnostic when an exchange or refresh reports it.
    pub fn remote_effect(&self) -> Option<RemoteEffect> {
        match self {
            Self::Login(error) | Self::Interactive(InteractiveLoginError::Broker(error)) => {
                Some(error.remote_effect())
            }
            Self::Refresh(error) => Some(error.remote_effect()),
            _ => None,
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => error.fmt(f),
            Self::Interactive(error) => error.fmt(f),
            Self::Login(error) => error.fmt(f),
            Self::Refresh(error) => error.fmt(f),
            Self::Announce(_) => f.write_str("could not announce native Broker authorization URL"),
            Self::Clock(_) => f.write_str("could not obtain publication timestamp"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
            Self::Interactive(error) => Some(error),
            Self::Login(error) => Some(error),
            Self::Refresh(error) => Some(error),
            Self::Announce(error) | Self::Clock(error) => Some(error),
        }
    }
}
pub(super) fn validate(input: &LoginArguments) -> Result<(), Error> {
    if input.username.is_some()
        || input.organization.is_some()
        || !input.teams.is_empty()
        || input.anonymous
        || input.mcp_key.is_some()
        || input.broker_key.is_some()
        || input.embedding_api_key.is_some()
        || input.rerank_api_key.is_some()
        || input.s3_access_key.is_some()
        || input.s3_secret_key.is_some()
        || input.s3_session_token.is_some()
        || input.s3_expires_at.is_some()
        || input.aws_profile.is_some()
    {
        return Err(Error::Command(CommandError::InvalidInput(
            "native Broker login does not accept Local identity or credential options",
        )));
    }
    Ok(())
}

/// The caller announces/opens the checked URL; the SDK pending/listener stays owned by this future.
/// Obtain the publication timestamp after the callback, rather than at login start.
pub async fn login<'a>(
    input: &'a LoginArguments,
    manager: &'a NativeProfileManager<'a>,
    authority: ProfileManagerAuthority<'a>,
    oauth: &'a NativeOAuth<'a>,
    config: InteractiveLoginConfig<'a>,
    announce: impl FnOnce(&str) -> io::Result<()>,
    now: impl FnOnce() -> io::Result<i64>,
) -> Result<AuthDocumentPublication, Error> {
    validate(input)?;
    let snapshot = manager
        .begin(authority)
        .map_err(|e| Error::Command(e.into()))?;
    revision(snapshot.revision(), input.expected_revision).map_err(Error::Command)?;
    let mut pending = manager
        .begin_interactive_broker_login(
            snapshot,
            oauth,
            BrokerLoginSelection {
                profile: &input.profile,
                provider: &input.provider,
                s3_disabled: input.s3_disabled,
            },
            config,
        )
        .await
        .map_err(Error::Interactive)?;
    announce(pending.authorization_url().map_err(Error::Login)?).map_err(Error::Announce)?;
    let received = pending.receive().await.map_err(Error::Interactive)?;
    let now = now().map_err(Error::Clock)?;
    received.publish(now).await.map_err(Error::Login)
}

/// Rotate only the actual active Broker profile through the SDK's current provider/token CAS.
pub async fn refresh<'a>(
    input: &RefreshArguments,
    manager: &'a NativeProfileManager<'a>,
    authority: ProfileManagerAuthority<'a>,
    oauth: &'a NativeOAuth<'a>,
    now: i64,
) -> Result<AuthDocumentPublication, Error> {
    let snapshot = manager
        .begin(authority)
        .map_err(|e| Error::Command(e.into()))?;
    revision(snapshot.revision(), input.expected_revision).map_err(Error::Command)?;
    manager
        .refresh_broker(snapshot, oauth, now)
        .await
        .map_err(Error::Refresh)
}

#[cfg(test)]
#[path = "broker_tests.rs"]
mod tests;
