//! Standalone S3 transport from the captured Auth lease; no project authorization.
use aws_credential_types::provider::ProvideCredentials;
use graphit_auth::{S3CredentialView, ServiceCredentialLease};
use object_store::aws::{AmazonS3, AmazonS3Builder, S3CopyIfNotExists};
use std::{
    io,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

pub(crate) struct StandaloneStorage {
    pub store: Arc<AmazonS3>,
    pub prefix: String,
    pub region: String,
    pub expires_at: Option<Instant>,
}

pub(crate) async fn open(
    lease: &ServiceCredentialLease<'_>,
    allow_loopback_http: bool,
    deadline: Instant,
    request_timeout: Duration,
) -> io::Result<StandaloneStorage> {
    let view = lease.standalone_s3().map_err(io::Error::other)?;
    let location = view.location;
    let bucket = location.bucket.as_deref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "standalone S3 bucket is required",
        )
    })?;
    let explicit_region = || {
        location.region.as_deref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "standalone S3 region is required",
            )
        })
    };
    let mut builder = AmazonS3Builder::new()
        .with_client_options(object_store::ClientOptions::new().with_timeout(request_timeout))
        .with_retry(object_store::RetryConfig {
            retry_timeout: request_timeout,
            ..Default::default()
        })
        .with_bucket_name(bucket)
        .with_copy_if_not_exists(S3CopyIfNotExists::Multipart);
    if let Some(endpoint) = location.endpoint.as_deref() {
        super::storage_origin(endpoint, allow_loopback_http)?;
        builder = builder
            .with_endpoint(endpoint)
            .with_allow_http(endpoint.starts_with("http://"));
    }
    let mut expires_at = None;
    let mut resolved_region = location.region.clone();
    builder = match view.credential {
        S3CredentialView::Local(credentials) => {
            let access = credentials
                .access_key_id
                .as_ref()
                .ok_or(io::ErrorKind::PermissionDenied)?;
            let secret = credentials
                .secret_access_key
                .as_ref()
                .ok_or(io::ErrorKind::PermissionDenied)?;
            builder = builder
                .with_region(explicit_region()?)
                .with_access_key_id(access.expose())
                .with_secret_access_key(secret.expose());
            if let Some(token) = &credentials.session_token {
                builder = builder.with_token(token.expose());
            }
            builder
        }
        S3CredentialView::Anonymous => builder
            .with_region(explicit_region()?)
            .with_skip_signature(true),
        S3CredentialView::AwsChain { profile } => {
            let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
            if let Some(profile) = profile {
                loader = loader.profile_name(profile);
            }
            let original = lease.original().map_err(io::Error::other)?;
            let config = tokio::select! {
                biased;
                () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
                () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
                config = loader.load() => config,
            };
            lease.check().map_err(io::Error::other)?;
            let region = location
                .region
                .as_deref()
                .or_else(|| config.region().map(|region| region.as_ref()))
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "selected AWS chain has no S3 region",
                    )
                })?;
            let provider = config.credentials_provider().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "selected AWS chain has no credential provider",
                )
            })?;
            resolved_region = Some(region.to_owned());
            // A request owns one resolution. Object GETs use the mature static S3 provider.
            let credentials = tokio::select! {
                biased;
                () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
                () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
                credentials = provider.provide_credentials() => credentials.map_err(io::Error::other)?,
            };
            expires_at = credentials
                .expiry()
                .map(|expiry| -> io::Result<Instant> {
                    let remaining = expiry
                        .duration_since(SystemTime::now())
                        .map_err(|_| io::ErrorKind::PermissionDenied)?;
                    Instant::now()
                        .checked_add(remaining)
                        .ok_or_else(|| io::ErrorKind::InvalidData.into())
                })
                .transpose()?;
            builder = builder
                .with_region(region)
                .with_access_key_id(credentials.access_key_id())
                .with_secret_access_key(credentials.secret_access_key());
            if let Some(token) = credentials.session_token() {
                builder = builder.with_token(token);
            }
            builder
        }
    };
    let store = builder.build().map_err(io::Error::other)?;
    lease.check().map_err(io::Error::other)?;
    Ok(StandaloneStorage {
        store: Arc::new(store),
        prefix: super::storage_prefix(location.prefix.as_deref().unwrap_or_default())?.to_owned(),
        region: resolved_region.ok_or(io::ErrorKind::InvalidInput)?,
        expires_at,
    })
}
