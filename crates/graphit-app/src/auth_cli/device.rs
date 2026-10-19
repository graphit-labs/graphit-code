//! CLI Device authorization shares the Native profile publication producer.
use super::{
    LoginArguments,
    broker::{Error, validate},
    revision,
};
use graphit_auth::{
    broker::oauth::{NativeOAuth, device::DeviceInstructions},
    profile_manager::{BrokerLoginSelection, NativeProfileManager, ProfileManagerAuthority},
};
use graphit_core::auth_document::publication::AuthDocumentPublication;
use std::io;

pub async fn login<'a>(
    input: &'a LoginArguments,
    manager: &'a NativeProfileManager<'a>,
    authority: ProfileManagerAuthority<'a>,
    oauth: &'a NativeOAuth<'a>,
    announce: impl FnOnce(&DeviceInstructions) -> io::Result<()>,
    now: impl FnOnce() -> io::Result<i64>,
) -> Result<AuthDocumentPublication, Error> {
    validate(input)?;
    let snapshot = manager
        .begin(authority)
        .map_err(|e| Error::Command(e.into()))?;
    revision(snapshot.revision(), input.expected_revision).map_err(Error::Command)?;
    let mut pending = manager
        .begin_device_broker_login(
            snapshot,
            oauth,
            BrokerLoginSelection {
                profile: &input.profile,
                provider: &input.provider,
                s3_disabled: input.s3_disabled,
            },
        )
        .await
        .map_err(Error::Login)?;
    announce(pending.instructions().map_err(Error::Login)?).map_err(Error::Announce)?;
    let received = pending.receive().await.map_err(Error::Login)?;
    received
        .publish(now().map_err(Error::Clock)?)
        .await
        .map_err(Error::Login)
}
