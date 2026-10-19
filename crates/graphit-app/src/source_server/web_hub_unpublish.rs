//! Remote catalog retraction uses its own Web HubWrite lease and Broker authority.
use super::SourceServer;
use crate::hub_unpublish::{self, Arguments, Failure};
use graphit_auth::broker::{BrokerVerifier, oauth::web::AuthenticatedWebCaller};
use graphit_core::artifact_store::StoreOwner;
use graphit_hub::policy::RequestContext;
use serde_json::{Value, json};
use std::io;

fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}

impl SourceServer {
    pub(crate) async fn web_hub_unpublish<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
        original: &'a RequestContext<'a>,
        input: Arguments,
    ) -> io::Result<Value> {
        let state = &self.0;
        let policy = &state.policy.native_install.hub;
        input.validate(policy)?;
        original.check()?;
        if input.provider != provider {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let issuer = state.auth.issuer();
        let installation = state.auth.installation();
        let mut selection = issuer
            .capture_web_hub_catalog_write_service(
                caller,
                verifier,
                original,
                provider,
                installation,
            )
            .map_err(denied)?;
        let lease = selection.credential().map_err(denied)?;
        let before = || -> io::Result<()> {
            original.check()?;
            if state.stop.is_cancelled() {
                return Err(io::ErrorKind::NotConnected.into());
            }
            lease.check().map_err(denied)?;
            if !matches!(installation.live.owner(), StoreOwner::Installation { .. })
                || installation.live.owner_root() != state.policy.installation.root()
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            Ok(())
        };
        before()?;
        let storage = crate::native_storage_credentials::broker_credentials(
            &lease,
            input.producer,
            &state.native_http,
            state.native_allow_loopback_http,
        )
        .await
        .map_err(io::Error::from)?;
        let current = || -> io::Result<()> {
            before()?;
            storage.check().map_err(io::Error::from)
        };
        current()?;
        let receipt = hub_unpublish::execute(
            hub_unpublish::Input {
                arguments: &input,
                storage: &storage,
                client: &state.native_http,
                policy,
            },
            original,
            &current,
        )
        .await?;
        let response = json!({"status":"unpublished","retraction":&receipt});
        if let Err(cause) = current() {
            return Err(io::Error::new(
                cause.kind(),
                Failure {
                    receipt,
                    stage: "delivery",
                    cause,
                },
            ));
        }
        Ok(response)
    }
}

/// Only the typed kernel receipt and finite cause enter the public envelope.
pub(crate) fn failure_body(error: &io::Error) -> Value {
    if let Some(failure) = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<Failure>())
    {
        json!({"status":"error","stage":failure.stage,"retraction":&failure.receipt,
            "cause":crate::native_publication::cause_code(&failure.cause)})
    } else {
        json!({"status":"error","retraction":{"catalog":"not_attempted"},
            "cause":crate::native_publication::cause_code(error)})
    }
}
