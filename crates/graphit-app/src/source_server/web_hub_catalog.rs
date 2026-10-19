//! Web HubRead catalog access retains its admitted identity and request.
use super::SourceServer;
use crate::{
    remote_file_install::client::Transport,
    remote_hub_catalog::{
        self as catalog,
        client::{ProjectsArguments, SearchArguments},
    },
};
use graphit_auth::broker::{BrokerVerifier, oauth::web::AuthenticatedWebCaller};
use graphit_hub::policy::{HubResourcePolicy, RequestContext};
use std::io;

pub(crate) enum Query {
    Projects(ProjectsArguments),
    Search(SearchArguments),
}
impl Query {
    pub(crate) fn decode(bytes: &[u8], search: bool) -> io::Result<Self> {
        if search {
            serde_json::from_slice(bytes).map(Self::Search)
        } else {
            serde_json::from_slice(bytes).map(Self::Projects)
        }
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
    }
    fn validate(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        let identity_provider = match self {
            Self::Projects(input) => {
                input.validate(policy)?;
                &input.identity_provider
            }
            Self::Search(input) => {
                input.validate(policy)?;
                &input.identity_provider
            }
        };
        if identity_provider.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }
    fn transport(&self) -> (Transport, &str, Option<&str>) {
        match self {
            Self::Projects(input) => (input.transport, &input.provider, input.profile.as_deref()),
            Self::Search(input) => (input.transport, &input.provider, input.profile.as_deref()),
        }
    }
}
fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}
impl SourceServer {
    pub(crate) async fn web_remote_hub_catalog<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        web_provider: &'a str,
        original: &'a RequestContext<'a>,
        query: Query,
    ) -> io::Result<Vec<u8>> {
        let policy = &self.0.policy.native_install.hub;
        query.validate(policy)?;
        original.check()?;
        let issuer = self.0.auth.issuer();
        let read = issuer
            .admit_web_installed_hub_read(
                caller,
                verifier,
                original,
                web_provider,
                self.0.auth.installation(),
            )
            .map_err(denied)?;
        let (transport, provider, profile) = query.transport();
        let mut selection = match transport {
            Transport::Broker => issuer.capture_installed_hub_service(&read, provider),
            Transport::Standalone => {
                issuer.capture_installed_hub_standalone_s3_service(&read, provider, profile)
            }
        }
        .map_err(denied)?;
        let lease = selection.credential().map_err(denied)?;
        let subject = lease
            .user_subject()
            .map_err(denied)?
            .ok_or(io::ErrorKind::PermissionDenied)?;
        let before = || -> io::Result<()> {
            original.check()?;
            if self.0.stop.is_cancelled() {
                return Err(io::ErrorKind::NotConnected.into());
            }
            read.recheck().map_err(denied)?;
            lease.check().map_err(denied)?;
            Ok(())
        };
        before()?;
        let capture = catalog::Catalog {
            lease: &lease,
            subject: Some(subject),
            policy,
            http: &self.0.native_http,
            allow_loopback_http: self.0.native_allow_loopback_http,
            before: &before,
        };
        let mut result = match query {
            Query::Projects(input) => {
                serde_json::to_value(catalog::projects(&capture, &input).await?)
            }
            Query::Search(input) => {
                serde_json::to_value(catalog::artifacts::search(capture, &input).await?)
            }
        }
        .map_err(io::Error::other)?;
        before()?;
        result["status"] = "ok".into();
        let bytes = serde_json::to_vec(&result).map_err(io::Error::other)?;
        if bytes.len() > policy.content().max_output_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        before()?;
        Ok(bytes)
    }
}
