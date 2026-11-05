//! Native instruction synchronization uses only the active Broker's fixed route.
use super::{AuthenticatedCaller, Origin, RoleIssuer};
use crate::profile::{ProfileError, ProviderKind};
use graphit_http::client::{
    BoundedClient, ClientPolicy, Limits, NetworkRequest, NetworkRequestContext,
};
use graphit_hub::policy::RequestContext;
use std::{io, time::Duration};

const PATH: &str = "/v1/agent-instructions";

impl RoleIssuer {
    /// Public configuration only. Reading the active origin grants no permissions.
    pub fn instruction_broker_origin(
        &self,
        original: &RequestContext<'_>,
    ) -> io::Result<Option<String>> {
        let snapshot = self
            .profile_store()
            .load(original)
            .map_err(io::Error::other)?;
        match snapshot.document().active(original) {
            Ok((provider, _)) => match &provider.config().kind {
                ProviderKind::Broker(service) => Ok(Some(service.endpoint.clone())),
                ProviderKind::Local { .. } => Ok(None),
            },
            Err(ProfileError::NoActive) => Ok(None),
            Err(error) => Err(io::Error::other(error)),
        }
    }
}

impl AuthenticatedCaller<'_> {
    /// Fetch the active deployment's published templates without exposing its bearer.
    /// The caller supplies the origin captured before the sync, never an alternate URL.
    pub async fn fetch_broker_instructions(
        &self,
        origin: &str,
        max_bytes: usize,
        allow_loopback_http: bool,
    ) -> io::Result<Option<secrecy::zeroize::Zeroizing<Vec<u8>>>> {
        use http::{HeaderMap, HeaderValue, Method, StatusCode, header};

        self.check().map_err(io::Error::other)?;
        if self.source.origin() != Origin::Runtime {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let mut snapshot = self
            .issuer
            .profile_store()
            .load(self.request)
            .map_err(io::Error::other)?;
        let (provider, profile) = snapshot
            .document()
            .active(self.request)
            .map_err(io::Error::other)?;
        let ProviderKind::Broker(service) = &provider.config().kind else {
            return Err(io::ErrorKind::PermissionDenied.into());
        };
        let credential = profile
            .broker_credentials()
            .ok_or(io::ErrorKind::PermissionDenied)?;
        if service.endpoint != origin || credential.issuer != origin {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let encoded = secrecy::zeroize::Zeroizing::new(format!(
            "Bearer {}",
            credential.access_token.expose()
        ));
        let mut authorization = HeaderValue::from_str(&encoded).map_err(io::Error::other)?;
        authorization.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, authorization);
        headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
        let client = BoundedClient::new(
            ClientPolicy {
                origin,
                allow_loopback_http,
                proxy: None,
                additional_roots: &[],
            },
            Limits {
                request_body_bytes: 1,
                response_body_bytes: max_bytes,
                url_bytes: 4096,
                header_bytes: 16 * 1024,
                header_count: 64,
                max_in_flight: 1,
                max_idle_per_host: 1,
                idle_timeout: Duration::from_secs(30),
                max_certificates: 1,
                max_certificate_bytes: 64 * 1024,
            },
        )
        .map_err(io::Error::other)?;
        let url = format!("{}{PATH}", origin.trim_end_matches('/'));
        let response = client
            .execute(
                NetworkRequest {
                    method: Method::GET,
                    url: &url,
                    headers: &headers,
                    body: &[],
                },
                NetworkRequestContext::new(
                    self.request.snapshot().required_deadline()?,
                    self.request.cancellation(),
                ),
            )
            .await
            .map_err(io::Error::other)?;
        self.issuer
            .profile_store()
            .revalidate(&mut snapshot, self.request)
            .map_err(io::Error::other)?;
        self.check().map_err(io::Error::other)?;
        match response.status() {
            StatusCode::OK => Ok(Some(response.into_body())),
            StatusCode::NOT_FOUND => Ok(None),
            status => Err(io::Error::new(
                if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                    io::ErrorKind::PermissionDenied
                } else {
                    io::ErrorKind::Other
                },
                format!("Broker instruction sync returned HTTP {status}"),
            )),
        }
    }
}
