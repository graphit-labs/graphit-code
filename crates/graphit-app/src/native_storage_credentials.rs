//! Broker STSv3 transport. A returned store is data; callers retain their service lease.
use graphit_auth::{ServiceBindingView, ServiceCredentialLease, ServicePurpose, ServiceUseError};
use graphit_core::project_identity::ProjectId;
use object_store::aws::{AmazonS3, AmazonS3Builder, S3CopyIfNotExists};
use serde::Deserialize;
use std::{
    fmt, io,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone)]
pub(crate) struct BrokerStorage {
    pub(crate) store: Arc<AmazonS3>,
    pub(crate) bucket: String,
    pub(crate) region: String,
    pub(crate) endpoint: Option<String>,
    pub(crate) prefix: String,
    pub(crate) authorization_revision: String,
    pub(crate) copy_if_not_exists: S3CopyIfNotExists,
    conditional_delete: bool,
    expires_at: Instant,
}
impl BrokerStorage {
    pub(crate) fn supports_conditional_delete(&self) -> bool {
        self.conditional_delete
    }

    pub(crate) fn expires_at(&self) -> Instant {
        self.expires_at
    }

    pub(crate) fn check(&self) -> Result<(), StorageCredentialsError> {
        if Instant::now() >= self.expires_at {
            Err(StorageCredentialsError::Expired)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
pub(crate) enum StorageCredentialsError {
    Authorization(ServiceUseError),
    Request(io::Error),
    Transport(reqwest::Error),
    Http(reqwest::StatusCode),
    InvalidResponse(&'static str),
    Storage(object_store::Error),
    Expired,
}
impl fmt::Display for StorageCredentialsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authorization(cause) => write!(f, "storage authorization: {cause}"),
            Self::Request(cause) => write!(f, "storage credentials request: {cause}"),
            Self::Transport(cause) => write!(f, "storage credentials transport: {cause}"),
            Self::Http(status) => write!(f, "Broker storage credentials returned HTTP {status}"),
            Self::InvalidResponse(cause) => {
                write!(f, "invalid Broker storage credentials: {cause}")
            }
            Self::Storage(cause) => write!(f, "storage client: {cause}"),
            Self::Expired => f.write_str("Broker storage credentials expired"),
        }
    }
}
impl std::error::Error for StorageCredentialsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Authorization(e) => Some(e),
            Self::Request(e) => Some(e),
            Self::Transport(e) => Some(e),
            Self::Storage(e) => Some(e),
            _ => None,
        }
    }
}
impl From<ServiceUseError> for StorageCredentialsError {
    fn from(value: ServiceUseError) -> Self {
        Self::Authorization(value)
    }
}

impl From<StorageCredentialsError> for io::Error {
    fn from(error: StorageCredentialsError) -> Self {
        let kind = match &error {
            StorageCredentialsError::Authorization(_) | StorageCredentialsError::Expired => {
                io::ErrorKind::PermissionDenied
            }
            StorageCredentialsError::Request(cause) => cause.kind(),
            StorageCredentialsError::Http(status)
                if matches!(
                    *status,
                    reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
                ) =>
            {
                io::ErrorKind::PermissionDenied
            }
            StorageCredentialsError::InvalidResponse(_) => io::ErrorKind::InvalidData,
            StorageCredentialsError::Transport(cause) if cause.is_timeout() => {
                io::ErrorKind::TimedOut
            }
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, error)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum CopyIfNotExists {
    Multipart,
    IfNoneMatch,
}
impl CopyIfNotExists {
    fn from_string<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            String::deserialize(deserializer)?,
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialsResponse {
    access_key_id: String,
    secret_access_key: String,
    session_token: String,
    expires_at: String,
    bucket: String,
    region: String,
    endpoint: String,
    conditional_delete: bool,
    #[serde(deserialize_with = "CopyIfNotExists::from_string")]
    copy_if_not_exists: CopyIfNotExists,
    prefixes: [String; 1],
    authorization_revision: String,
    scope: String,
    project_id: String,
    module: String,
    #[serde(default, deserialize_with = "installation_id")]
    installation_id: Option<String>,
}
fn installation_id<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}
#[derive(Clone, Copy)]
pub(crate) enum StorageScope<'a> {
    Project {
        project_id: ProjectId,
        module: &'a str,
    },
    User {
        installation_id: &'a str,
    },
}
impl StorageScope<'_> {
    fn matches(self, response: &CredentialsResponse) -> bool {
        match self {
            Self::Project { project_id, module } => {
                response.scope == "project"
                    && response.project_id == project_id.to_string()
                    && response.module == module
                    && response.installation_id.is_none()
            }
            Self::User { installation_id } => {
                response.scope == "user"
                    && response.project_id.is_empty()
                    && response.module == "memory"
                    && response.installation_id.as_deref() == Some(installation_id)
            }
        }
    }
    fn request(self) -> serde_json::Value {
        match self {
            Self::Project { project_id, module } => {
                serde_json::json!({"scope":"project","project_id":project_id,"module":module})
            }
            Self::User { installation_id } => {
                serde_json::json!({"scope":"user","module":"memory","installation_id":installation_id})
            }
        }
    }
}
impl Drop for CredentialsResponse {
    fn drop(&mut self) {
        self.access_key_id.zeroize();
        self.secret_access_key.zeroize();
        self.session_token.zeroize();
    }
}

fn invalid(message: &'static str) -> StorageCredentialsError {
    StorageCredentialsError::InvalidResponse(message)
}
fn origin(value: &str, allow_loopback_http: bool) -> Result<url::Url, StorageCredentialsError> {
    let url = url::Url::parse(value).map_err(|_| invalid("invalid endpoint"))?;
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        None => false,
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.host().is_none()
        || !(url.scheme() == "https" || (url.scheme() == "http" && allow_loopback_http && loopback))
    {
        return Err(invalid("endpoint is not an allowed Broker/storage origin"));
    }
    Ok(url)
}
fn prefix(value: &str) -> Result<&str, StorageCredentialsError> {
    let prefix = value.strip_suffix('/').unwrap_or(value);
    if prefix.is_empty()
        || prefix.chars().any(char::is_control)
        || prefix.contains(['\\', ':'])
        || prefix.split('/').any(|s| matches!(s, "" | "." | ".."))
    {
        return Err(invalid("invalid storage prefix"));
    }
    Ok(prefix)
}
#[cfg(test)]
fn parse_response(
    bytes: &[u8],
    producer: ProjectId,
    allow_loopback_http: bool,
) -> Result<BrokerStorage, StorageCredentialsError> {
    parse_scoped_response(
        bytes,
        StorageScope::Project {
            project_id: producer,
            module: "hub",
        },
        allow_loopback_http,
        Duration::from_secs(30),
    )
}
fn parse_scoped_response(
    bytes: &[u8],
    scope: StorageScope<'_>,
    allow_loopback_http: bool,
    request_timeout: Duration,
) -> Result<BrokerStorage, StorageCredentialsError> {
    let response: CredentialsResponse =
        serde_json::from_slice(bytes).map_err(|_| invalid("malformed response"))?;
    if !scope.matches(&response) {
        return Err(invalid("response scope or producer differs"));
    }
    if response
        .authorization_revision
        .parse::<u64>()
        .ok()
        .is_none_or(|r| r == 0)
        || [
            &response.access_key_id,
            &response.secret_access_key,
            &response.session_token,
            &response.bucket,
            &response.region,
        ]
        .into_iter()
        .any(|s| s.is_empty() || s.chars().any(char::is_control))
    {
        return Err(invalid("missing credentials, topology or revision"));
    }
    let expires = chrono::DateTime::parse_from_rfc3339(&response.expires_at)
        .map_err(|_| invalid("invalid expiry"))?;
    let remaining = SystemTime::from(expires)
        .duration_since(SystemTime::now())
        .map_err(|_| StorageCredentialsError::Expired)?;
    let expires_at = Instant::now()
        .checked_add(remaining)
        .ok_or_else(|| invalid("expiry is out of range"))?;
    let prefix = prefix(&response.prefixes[0])?.to_owned();
    let endpoint = if response.endpoint.is_empty() {
        None
    } else {
        origin(&response.endpoint, allow_loopback_http)?;
        Some(response.endpoint.clone())
    };
    let copy_if_not_exists = match &response.copy_if_not_exists {
        CopyIfNotExists::Multipart => S3CopyIfNotExists::Multipart,
        CopyIfNotExists::IfNoneMatch => {
            S3CopyIfNotExists::Header("if-none-match".into(), "*".into())
        }
    };
    let mut builder = AmazonS3Builder::new()
        .with_client_options(object_store::ClientOptions::new().with_timeout(request_timeout))
        .with_retry(object_store::RetryConfig {
            retry_timeout: request_timeout,
            ..Default::default()
        })
        .with_copy_if_not_exists(copy_if_not_exists.clone())
        .with_bucket_name(&response.bucket)
        .with_region(&response.region)
        .with_access_key_id(&response.access_key_id)
        .with_secret_access_key(&response.secret_access_key)
        .with_token(&response.session_token);
    if let Some(endpoint) = &endpoint {
        builder = builder
            .with_endpoint(endpoint)
            .with_allow_http(endpoint.starts_with("http://"));
    }
    let store = builder.build().map_err(StorageCredentialsError::Storage)?;
    let storage = BrokerStorage {
        store: Arc::new(store),
        bucket: response.bucket.clone(),
        region: response.region.clone(),
        endpoint,
        prefix,
        authorization_revision: response.authorization_revision.clone(),
        copy_if_not_exists,
        conditional_delete: response.conditional_delete,
        expires_at,
    };
    storage.check()?;
    Ok(storage)
}

pub(crate) async fn broker_credentials(
    lease: &ServiceCredentialLease<'_>,
    producer: ProjectId,
    client: &reqwest::Client,
    allow_loopback_http: bool,
) -> Result<BrokerStorage, StorageCredentialsError> {
    let deadline = lease
        .original()?
        .snapshot()
        .required_deadline()
        .map_err(StorageCredentialsError::Request)?;
    broker_credentials_for(
        lease,
        StorageScope::Project {
            project_id: producer,
            module: "hub",
        },
        client,
        allow_loopback_http,
        deadline,
        deadline.saturating_duration_since(Instant::now()),
    )
    .await
}

pub(crate) async fn broker_credentials_for(
    lease: &ServiceCredentialLease<'_>,
    scope: StorageScope<'_>,
    client: &reqwest::Client,
    allow_loopback_http: bool,
    deadline: Instant,
    request_timeout: Duration,
) -> Result<BrokerStorage, StorageCredentialsError> {
    lease.check()?;
    let ServiceBindingView::Broker {
        purpose, endpoint, ..
    } = lease.binding()?
    else {
        return Err(invalid("service binding is not HubRead or HubWrite"));
    };
    if !matches!(
        (purpose, scope),
        (
            ServicePurpose::HubRead | ServicePurpose::HubWrite,
            StorageScope::Project { module: "hub", .. }
        ) | (
            ServicePurpose::RecordStorage,
            StorageScope::Project {
                module: "memory" | "task",
                ..
            } | StorageScope::User { .. }
        )
    ) {
        return Err(invalid("storage scope differs from service purpose"));
    }
    let mut endpoint = origin(endpoint, allow_loopback_http)?;
    endpoint.set_path("/v1/s3/credentials");
    let credential = lease
        .credential()?
        .ok_or_else(|| invalid("authenticated Broker credential required"))?;
    let original = lease.original()?;
    let operation = async {
        lease.check()?;
        let mut response = client
            .post(endpoint)
            .bearer_auth(credential)
            .json(&scope.request())
            .send()
            .await
            .map_err(|e| StorageCredentialsError::Transport(e.without_url()))?;
        lease.check()?;
        if !response.status().is_success() {
            return Err(StorageCredentialsError::Http(response.status()));
        }
        let limit = original.content_limits().max_input_bytes;
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(invalid("response exceeds request input limit"));
        }
        let mut bytes = Zeroizing::new(Vec::new());
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| StorageCredentialsError::Transport(e.without_url()))?
        {
            lease.check()?;
            if bytes
                .len()
                .checked_add(chunk.len())
                .is_none_or(|n| n > limit)
            {
                return Err(invalid("response exceeds request input limit"));
            }
            bytes
                .try_reserve(chunk.len())
                .map_err(|_| invalid("response allocation failed"))?;
            bytes.extend_from_slice(&chunk);
        }
        lease.check()?;
        let storage = parse_scoped_response(&bytes, scope, allow_loopback_http, request_timeout)?;
        lease.check()?;
        storage.check()?;
        Ok(storage)
    };
    tokio::select! {
        biased;
        _ = original.cancellation().cancelled() => Err(StorageCredentialsError::Request(io::Error::new(io::ErrorKind::Interrupted, "storage credentials request cancelled"))),
        _ = tokio::time::sleep_until(deadline.into()) => Err(StorageCredentialsError::Request(io::Error::new(io::ErrorKind::TimedOut, "storage credentials request deadline expired"))),
        result = operation => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::time::Duration;

    fn producer() -> ProjectId {
        "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()
    }
    fn response() -> Value {
        json!({
            "access_key_id":"issued-key", "secret_access_key":"issued-secret", "session_token":"issued-session",
            "expires_at": chrono::DateTime::<chrono::Utc>::from(SystemTime::now()+Duration::from_secs(600)).to_rfc3339(),
            "bucket":"native-bucket", "region":"us-east-1", "endpoint":"http://127.0.0.1:9000",
            "conditional_delete":true, "copy_if_not_exists":"multipart",
            "prefixes":["tenant/ação%2F/hub/"], "authorization_revision":"7", "scope":"project", "project_id":producer(), "module":"hub"
        })
    }
    fn parse(value: &Value, allow_http: bool) -> Result<BrokerStorage, StorageCredentialsError> {
        parse_response(&serde_json::to_vec(value).unwrap(), producer(), allow_http)
    }
    #[test]
    fn record_scopes_use_exact_project_and_user_contracts() {
        for module in ["memory", "task"] {
            let scope = StorageScope::Project {
                project_id: producer(),
                module,
            };
            assert_eq!(
                scope.request(),
                json!({"scope":"project","project_id":producer(),"module":module})
            );
            let mut value = response();
            value["module"] = json!(module);
            parse_scoped_response(
                &serde_json::to_vec(&value).unwrap(),
                scope,
                true,
                Duration::from_secs(30),
            )
            .unwrap();
            assert!(parse(&value, true).is_err());
        }
        let scope = StorageScope::User {
            installation_id: "installation-A",
        };
        assert_eq!(
            scope.request(),
            json!({"scope":"user","module":"memory","installation_id":"installation-A"})
        );
        let mut value = response();
        value["scope"] = json!("user");
        value["project_id"] = json!("");
        value["module"] = json!("memory");
        value["installation_id"] = json!("installation-A");
        parse_scoped_response(
            &serde_json::to_vec(&value).unwrap(),
            scope,
            true,
            Duration::from_secs(30),
        )
        .unwrap();
        for (field, replacement) in [
            ("installation_id", json!("installation-B")),
            ("installation_id", Value::Null),
            ("project_id", json!(producer())),
            ("module", json!("task")),
        ] {
            let mut bad = value.clone();
            bad[field] = replacement;
            assert!(
                parse_scoped_response(
                    &serde_json::to_vec(&bad).unwrap(),
                    scope,
                    true,
                    Duration::from_secs(30)
                )
                .is_err()
            );
        }
    }
    #[test]
    fn project_response_preserves_topology_and_issued_expiry_without_network() {
        let storage = parse(&response(), true).unwrap();
        assert_eq!(storage.bucket, "native-bucket");
        assert_eq!(storage.region, "us-east-1");
        assert_eq!(storage.endpoint.as_deref(), Some("http://127.0.0.1:9000"));
        assert_eq!(storage.prefix, "tenant/ação%2F/hub");
        assert_eq!(storage.authorization_revision, "7");
        storage.check().unwrap();
        let _store = Arc::clone(&storage.store);
        let mut default_aws = response();
        default_aws["endpoint"] = json!("");
        assert!(parse(&default_aws, false).unwrap().endpoint.is_none());
        // Building the maintained client is not a claim of IAM or successful object IO.
    }
    #[test]
    fn project_response_preserves_issued_conditional_delete_capability() {
        for supported in [false, true] {
            let mut value = response();
            value["conditional_delete"] = json!(supported);
            let storage = parse(&value, true).unwrap();
            assert_eq!(storage.supports_conditional_delete(), supported);
            storage.check().unwrap();
        }
    }
    #[test]
    fn project_response_requires_unambiguous_boolean_conditional_delete() {
        let mut missing = response();
        missing
            .as_object_mut()
            .unwrap()
            .remove("conditional_delete");
        assert!(matches!(
            parse(&missing, true),
            Err(StorageCredentialsError::InvalidResponse(
                "malformed response"
            ))
        ));
        for invalid in [Value::Null, json!("true"), json!(0), json!([]), json!({})] {
            let mut value = response();
            value["conditional_delete"] = invalid;
            assert!(matches!(
                parse(&value, true),
                Err(StorageCredentialsError::InvalidResponse(
                    "malformed response"
                ))
            ));
        }
        let mut duplicate = serde_json::to_string(&response()).unwrap();
        duplicate.insert_str(1, "\"conditional_delete\":false,");
        assert!(matches!(
            parse_response(duplicate.as_bytes(), producer(), true),
            Err(StorageCredentialsError::InvalidResponse(
                "malformed response"
            ))
        ));
    }
    #[test]
    fn project_response_rejects_ambiguous_scope_topology_secrets_and_expiry() {
        let value = response();
        for (field, replacement) in [
            ("scope", json!("user")),
            ("module", json!("memory")),
            ("project_id", json!("01ARZ3NDEKTSV4RRFFQ69G5FAV")),
            ("authorization_revision", json!("0")),
            ("authorization_revision", json!("not-a-revision")),
            ("access_key_id", json!("")),
            ("secret_access_key", json!("")),
            ("session_token", json!("")),
            ("bucket", json!("")),
            ("region", json!("")),
            ("endpoint", json!("http://remote.example")),
            ("endpoint", json!("https://storage.example/path")),
            ("endpoint", json!("https://user:password@storage.example")),
            ("endpoint", json!("https://storage.example?secret=x")),
            ("prefixes", json!([])),
            ("prefixes", json!(["one", "two"])),
            ("prefixes", json!(["/foreign"])),
            ("prefixes", json!(["a//b"])),
            ("prefixes", json!(["a/../b"])),
            ("prefixes", json!(["a//"])),
            ("expires_at", json!("not-a-date")),
            ("expires_at", json!("2000-01-01T00:00:00Z")),
            ("installation_id", Value::Null),
            ("unknown", json!(true)),
        ] {
            let mut invalid = value.clone();
            invalid[field] = replacement;
            assert!(parse(&invalid, true).is_err(), "field {field} must fail");
        }
        assert!(parse(&value, false).is_err());
        let mut expired = value.clone();
        expired["expires_at"] = json!("2000-01-01T00:00:00Z");
        assert!(matches!(
            parse(&expired, true),
            Err(StorageCredentialsError::Expired)
        ));
        let mut text = serde_json::to_string(&value).unwrap();
        text.insert_str(1, "\"scope\":\"project\",");
        assert!(parse_response(text.as_bytes(), producer(), true).is_err());
    }

    #[test]
    fn project_response_accepts_only_the_two_issued_copy_strategies() {
        for strategy in ["multipart", "if_none_match"] {
            let mut value = response();
            value["copy_if_not_exists"] = json!(strategy);
            let decoded: CredentialsResponse = serde_json::from_value(value.clone()).unwrap();
            assert!(matches!(
                (strategy, &decoded.copy_if_not_exists),
                ("multipart", CopyIfNotExists::Multipart)
                    | ("if_none_match", CopyIfNotExists::IfNoneMatch)
            ));
            parse(&value, true).unwrap().check().unwrap();
        }
    }
    #[test]
    fn project_response_requires_an_unambiguous_copy_strategy_string() {
        let mut missing = response();
        missing
            .as_object_mut()
            .unwrap()
            .remove("copy_if_not_exists");
        assert!(matches!(
            parse(&missing, true),
            Err(StorageCredentialsError::InvalidResponse(
                "malformed response"
            ))
        ));
        for invalid in [
            Value::Null,
            json!(true),
            json!(0),
            json!([]),
            json!({}),
            json!({"multipart":null}),
            json!("unknown"),
            json!("Multipart"),
            json!("if-none-match"),
        ] {
            let mut value = response();
            value["copy_if_not_exists"] = invalid;
            assert!(matches!(
                parse(&value, true),
                Err(StorageCredentialsError::InvalidResponse(
                    "malformed response"
                ))
            ));
        }
        let mut duplicate = serde_json::to_string(&response()).unwrap();
        duplicate.insert_str(1, "\"copy_if_not_exists\":\"if_none_match\",");
        assert!(matches!(
            parse_response(duplicate.as_bytes(), producer(), true),
            Err(StorageCredentialsError::InvalidResponse(
                "malformed response"
            ))
        ));
    }
}
