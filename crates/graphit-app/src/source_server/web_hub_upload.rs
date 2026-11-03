//! Received packages keep one owned private spool and the actual Web publisher.
use super::SourceServer;
use crate::{file_publication, native_publication};
use axum::extract::{Multipart, multipart::Field};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
    WebHubPublicationInput,
    broker::{
        BrokerVerifier,
        oauth::web::AuthenticatedWebCaller,
        project_access::{
            BrokerProjectAccess, BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead,
        },
    },
};
use graphit_core::artifact_store::{
    CapturedPublicationScope, CapturedSourceReader, CopyPolicy, SourceReadLimits, StoreOwner,
};
use graphit_core::project_identity::ProjectId;
use graphit_hub::policy::{HubResourcePolicy, RequestContext};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, Write};
#[cfg(all(test, target_os = "linux"))]
pub(crate) mod stream_observer;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Query {
    pub(crate) project_id: ProjectId,
    pub(crate) project_version: String,
    pub(crate) publication: Publication,
}
#[derive(Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub(crate) enum Publication {
    File(file_publication::Metadata),
    Native(native_publication::PublicationMetadata),
}
impl Publication {
    fn provider(&self) -> &str {
        match self {
            Self::File(value) => &value.provider,
            Self::Native(value) => &value.provider,
        }
    }
}
impl Query {
    fn validate(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        if self.project_version.is_empty()
            || self.project_version.contains('\0')
            || self.project_version.len() > policy.content().max_scalar_bytes
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        match &self.publication {
            Publication::File(value) => {
                value.validate(policy)?;
            }
            Publication::Native(value) => {
                value.validate(native_publication::metadata_limits(
                    policy.content(),
                    policy.projection(),
                ))?;
            }
        }
        Ok(())
    }
}
fn multipart_error(error: axum::extract::multipart::MultipartError) -> io::Error {
    io::Error::new(
        if error.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
            io::ErrorKind::FileTooLarge
        } else {
            io::ErrorKind::InvalidInput
        },
        error,
    )
}
async fn guarded<T>(
    original: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
    future: impl std::future::Future<Output = io::Result<T>>,
) -> io::Result<T> {
    check()?;
    let cancellation = original.cancellation();
    let deadline = original.snapshot().required_deadline()?;
    let result = tokio::select! {
        value = future => value,
        () = cancellation.cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(deadline.into()) => Err(io::ErrorKind::TimedOut.into()),
    }?;
    check()?;
    Ok(result)
}
async fn metadata(
    multipart: &mut Multipart,
    maximum: usize,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<Query> {
    let check = || original.check();
    let mut field = guarded(original, &check, async {
        multipart
            .next_field()
            .await
            .map_err(multipart_error)?
            .ok_or(io::ErrorKind::InvalidInput.into())
    })
    .await?;
    if field.name() != Some("metadata")
        || field.file_name().is_some()
        || field
            .content_type()
            .is_some_and(|value| value != "application/json" && value != "text/plain")
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = guarded(original, &check, async {
        field.chunk().await.map_err(multipart_error)
    })
    .await?
    {
        if chunk.len() > maximum.saturating_sub(bytes.len()) {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let query: Query = serde_json::from_slice(&bytes).map_err(|_| io::ErrorKind::InvalidInput)?;
    query.validate(policy)?;
    Ok(query)
}
async fn spool(
    multipart: &mut Multipart,
    maximum: u64,
    original: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
    #[cfg(test)] _identity: Option<&Query>,
) -> io::Result<tempfile::TempDir> {
    let mut field: Field<'_> = guarded(original, check, async {
        multipart
            .next_field()
            .await
            .map_err(multipart_error)?
            .ok_or(io::ErrorKind::InvalidInput.into())
    })
    .await?;
    if field.name() != Some("file") || field.file_name().is_none_or(str::is_empty) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let owner = tempfile::Builder::new()
        .prefix("hub-received-package-")
        .tempdir()?;
    let mut file = graphit_fs::create_private_file(&owner.path().join("payload.zip"))?;
    #[cfg(all(test, target_os = "linux"))]
    if let Some(identity) = _identity {
        stream_observer::observe(identity, owner.path());
    }
    let mut length = 0u64;
    while let Some(chunk) = guarded(original, check, async {
        field.chunk().await.map_err(multipart_error)
    })
    .await?
    {
        length = length
            .checked_add(chunk.len() as u64)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if length > maximum {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        for block in chunk.chunks(64 * 1024) {
            check()?;
            file.write_all(block)?;
        }
    }
    drop(field);
    drop(file);
    if guarded(original, check, async {
        multipart.next_field().await.map_err(multipart_error)
    })
    .await?
    .is_some()
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    check()?;
    Ok(owner)
}

pub(crate) struct Input {
    pub(crate) multipart: Multipart,
    pub(crate) metadata_limit: usize,
}
fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}
fn invalid_package(error: io::Error) -> io::Error {
    if error.kind() == io::ErrorKind::InvalidData {
        io::Error::new(io::ErrorKind::InvalidInput, error)
    } else {
        error
    }
}
impl SourceServer {
    pub(crate) async fn web_hub_upload<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
        backend: &'a BrokerProjectAccess,
        original: &'a RequestContext<'a>,
        upload: Input,
    ) -> io::Result<Value> {
        let state = &self.0;
        let policy = &state.policy.native_install.hub;
        let Input {
            mut multipart,
            metadata_limit,
        } = upload;
        original.check()?;
        let input = metadata(
            &mut multipart,
            metadata_limit.min(policy.content().max_input_bytes),
            policy,
            original,
        )
        .await?;
        if input.publication.provider() != provider {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let projects = state.live_projects.snapshot()?;
        let selection = crate::task_server::request::Selection {
            project_dir: None,
            project_id: Some(input.project_id),
            project_version: Some(input.project_version.clone()),
        };
        let index = selection.select(&projects.projects, 0).map_err(|code| {
            io::Error::from(if code == "selection_ambiguous" {
                io::ErrorKind::AlreadyExists
            } else {
                io::ErrorKind::NotFound
            })
        })?;
        let (paths, id, version) = projects
            .projects
            .get(index)
            .ok_or(io::ErrorKind::NotFound)?;
        let scope =
            CapturedPublicationScope::project(paths, *id, state.policy.scope, original.snapshot())?;
        let binding = scope.binding()?;
        let issuer = state.auth.issuer();
        let authority = ProjectRecordAuthority::new(issuer);
        let source_read = authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(caller),
                verifier,
                backend,
                original,
                provider,
                scope: BrokerProjectScope {
                    installation: state.auth.installation(),
                    project: ProjectCapture {
                        scope: ScopeCapture {
                            live: &scope,
                            record: &binding,
                        },
                        version,
                    },
                    providers: &[],
                },
                module: ProjectReadModule::Hub,
            })
            .await
            .map_err(denied)?;
        let admission = issuer
            .admit_web_hub_publication(WebHubPublicationInput {
                caller,
                verifier,
                original,
                provider,
                installation: state.auth.installation(),
                source: &source_read,
            })
            .map_err(denied)?;
        let mut selection = issuer
            .capture_web_hub_write_service(&admission, provider)
            .map_err(denied)?;
        let lease = selection.credential().map_err(denied)?;
        let before = || -> io::Result<()> {
            original.check()?;
            if state.stop.is_cancelled() {
                return Err(io::ErrorKind::NotConnected.into());
            }
            admission.recheck().map_err(denied)?;
            lease.check().map_err(denied)?;
            let installation = admission.installation().live;
            if !matches!(installation.owner(), StoreOwner::Installation { .. })
                || installation.owner_root() != state.policy.installation.root()
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            if !state.live_projects.is_active(paths, *id, version)? {
                return Err(io::ErrorKind::NotFound.into());
            }
            Ok(())
        };
        before()?;

        let owner = spool(
            &mut multipart,
            policy.copy().total_file_bytes_limit(),
            original,
            &before,
            #[cfg(test)]
            Some(&input),
        )
        .await?;
        before()?;
        match &input.publication {
            Publication::File(metadata) => {
                let prepared = file_publication::prepare_received(
                    metadata, *id, owner, policy, original, &before,
                )
                .map_err(invalid_package)?;
                before()?;
                let storage = crate::native_storage_credentials::broker_credentials(
                    &lease,
                    *id,
                    &state.native_http,
                    state.native_allow_loopback_http,
                )
                .await
                .map_err(io::Error::from)?;
                let current = || -> io::Result<()> {
                    before()?;
                    storage.check().map_err(io::Error::from)
                };
                let project_metadata = crate::hub_project_metadata::publish(
                    storage.store.as_ref(),
                    &storage.prefix,
                    (paths, *id),
                    policy.content().max_input_bytes,
                    original,
                    &current,
                )
                .await?;
                let publication = file_publication::publish(
                    file_publication::Input {
                        store: storage.store.as_ref(),
                        storage_prefix: &storage.prefix,
                        prepared,
                    },
                    original,
                    current,
                )
                .await
                .map_err(|cause| {
                    crate::hub_project_metadata::artifact_failure(project_metadata, cause)
                })?;
                Ok(
                    json!({"status":"published","publication":publication,"project_metadata":project_metadata}),
                )
            }
            Publication::Native(metadata) => {
                let reader = CapturedSourceReader::capture(
                    &owner.path().join("payload.zip"),
                    graphit_fs::NodeKind::File,
                    &policy.snapshot_policy(CopyPolicy::RejectFileLinks, [])?,
                    SourceReadLimits::new(
                        64 * 1024,
                        64 * 1024,
                        policy.copy().total_file_bytes_limit(),
                    )?,
                    std::sync::Arc::clone(original.snapshot()),
                )?;
                let prepared = crate::received_native::prepare(
                    &reader,
                    owner.path(),
                    metadata.artifact_type,
                    &state.policy,
                    original,
                    &before,
                )
                .await
                .map_err(invalid_package)?;
                before()?;
                prepared.recheck(original, &before)?;
                native_publication::validate_input(
                    *id,
                    metadata,
                    prepared.components(),
                    native_publication::metadata_limits(policy.content(), policy.projection()),
                    original,
                )
                .map_err(invalid_package)?;
                let publication_reader = prepared.reader(original)?;
                let storage = crate::native_storage_credentials::broker_credentials(
                    &lease,
                    *id,
                    &state.native_http,
                    state.native_allow_loopback_http,
                )
                .await
                .map_err(io::Error::from)?;
                let current = || -> io::Result<()> {
                    before()?;
                    prepared.recheck(original, &before)?;
                    storage.check().map_err(io::Error::from)
                };
                let project_metadata = crate::hub_project_metadata::publish(
                    storage.store.as_ref(),
                    &storage.prefix,
                    (paths, *id),
                    policy.content().max_input_bytes,
                    original,
                    &current,
                )
                .await?;
                let publication = native_publication::publish(
                    native_publication::PublishInput {
                        store: storage.store.as_ref(),
                        storage_prefix: &storage.prefix,
                        producer: *id,
                        metadata,
                        reader: publication_reader,
                        components: prepared.components(),
                        limits: native_publication::metadata_limits(
                            policy.content(),
                            policy.projection(),
                        ),
                    },
                    original,
                    current,
                )
                .await
                .map_err(|cause| {
                    crate::hub_project_metadata::artifact_failure(project_metadata, cause)
                })?;
                Ok(
                    json!({"status":"published","publication":publication,"project_metadata":project_metadata}),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, extract::FromRequest, http::Request};
    use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};
    use serde_json::json;
    use std::time::{Duration, Instant};
    fn policy() -> HubResourcePolicy {
        let brand = Brand::new("graphit").unwrap();
        HubResourcePolicy::resolve(
            &ConfigSnapshot::new(
                &brand,
                [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
            )
            .unwrap(),
        )
        .unwrap()
    }
    fn query() -> Value {
        json!({"project_id":"01KSH1CRFFG8Z74B5ZS78WW808","project_version":"main", "publication":{
            "format":"file","provider":"broker","artifact_type":"skill","id":"fixture/upload","version":"release/one","name":"Received"}})
    }
    fn part(
        bytes: &mut Vec<u8>,
        name: &str,
        filename: Option<&str>,
        content_type: Option<&str>,
        value: &[u8],
    ) {
        write!(
            bytes,
            "--boundary\r\nContent-Disposition: form-data; name=\"{name}\""
        )
        .unwrap();
        if let Some(filename) = filename {
            write!(bytes, "; filename=\"{filename}\"").unwrap();
        }
        write!(bytes, "\r\n").unwrap();
        if let Some(content_type) = content_type {
            write!(bytes, "Content-Type: {content_type}\r\n").unwrap();
        }
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(value);
        bytes.extend_from_slice(b"\r\n");
    }
    async fn decode_multipart(mut bytes: Vec<u8>) -> Multipart {
        bytes.extend_from_slice(b"--boundary--\r\n");
        Multipart::from_request(
            Request::builder()
                .header("content-type", "multipart/form-data; boundary=boundary")
                .body(Body::from(bytes))
                .unwrap(),
            &(),
        )
        .await
        .unwrap()
    }
    #[tokio::test]
    async fn strict_metadata_keeps_closed_formats_and_rejects_forged_selection() {
        let policy = policy();
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel).unwrap();
        for (format, kind) in [
            ("file", "agent"),
            ("file", "skill"),
            ("file", "command"),
            ("file", "workflow"),
            ("file", "rule"),
            ("file", "mcp"),
            ("file", "language"),
            ("file", "power"),
            ("native", "ast"),
            ("native", "knowledge"),
        ] {
            let mut value = query();
            value["publication"]["format"] = json!(format);
            value["publication"]["artifact_type"] = json!(kind);
            let mut bytes = Vec::new();
            part(
                &mut bytes,
                "metadata",
                None,
                None,
                &serde_json::to_vec(&value).unwrap(),
            );
            assert!(
                metadata(&mut decode_multipart(bytes).await, 8192, &policy, &original)
                    .await
                    .is_ok()
            );
        }
        let mut invalid = Vec::new();
        for (field, value) in [
            ("project_dir", json!("/host/path")),
            ("producer", json!("spoof")),
            ("project_version", json!(null)),
            ("project_id", json!("invalid")),
        ] {
            let mut value_query = query();
            value_query[field] = value;
            invalid.push(value_query.to_string());
        }
        for (format, kind) in [
            ("file", "framework"),
            ("file", "ast"),
            ("native", "skill"),
            ("other", "knowledge"),
        ] {
            let mut value = query();
            value["publication"]["format"] = json!(format);
            value["publication"]["artifact_type"] = json!(kind);
            invalid.push(value.to_string());
        }
        let mut exclusive = query();
        exclusive["publication"]["format"] = json!("native");
        exclusive["publication"]["artifact_type"] = json!("ast");
        exclusive["publication"]["set_latest"] = json!(true);
        invalid.push(exclusive.to_string());
        invalid.push(query().to_string().replacen(
            "\"project_version\":\"main\"",
            "\"project_version\":\"main\",\"project_version\":\"other\"",
            1,
        ));
        for value in invalid {
            let mut bytes = Vec::new();
            part(&mut bytes, "metadata", None, None, value.as_bytes());
            assert!(
                metadata(&mut decode_multipart(bytes).await, 8192, &policy, &original)
                    .await
                    .is_err()
            );
        }
    }
    #[tokio::test]
    async fn streaming_requires_metadata_then_one_file_and_ignores_filename_as_a_path() {
        let policy = policy();
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel).unwrap();
        let mut bytes = Vec::new();
        part(
            &mut bytes,
            "metadata",
            None,
            Some("application/json"),
            query().to_string().as_bytes(),
        );
        part(
            &mut bytes,
            "file",
            Some("../../host-path"),
            None,
            b"exact bytes",
        );
        let mut multipart = decode_multipart(bytes).await;
        metadata(&mut multipart, 8192, &policy, &original)
            .await
            .unwrap();
        let owner = spool(&mut multipart, 11, &original, &|| original.check(), None)
            .await
            .unwrap();
        let path = owner.path().to_owned();
        assert_eq!(
            std::fs::read(path.join("payload.zip")).unwrap(),
            b"exact bytes"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(path.join("payload.zip"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        drop(owner);
        assert!(!path.exists());
        for extra in [false, true] {
            let mut bytes = Vec::new();
            part(
                &mut bytes,
                "metadata",
                None,
                None,
                query().to_string().as_bytes(),
            );
            part(&mut bytes, "file", Some("file.zip"), None, b"too large");
            if extra {
                part(&mut bytes, "file", Some("second.zip"), None, b"second");
            }
            let mut multipart = decode_multipart(bytes).await;
            metadata(&mut multipart, 8192, &policy, &original)
                .await
                .unwrap();
            let error = spool(
                &mut multipart,
                if extra { 8192 } else { 2 },
                &original,
                &|| original.check(),
                None,
            )
            .await
            .unwrap_err();
            assert_eq!(
                error.kind(),
                if extra {
                    io::ErrorKind::InvalidInput
                } else {
                    io::ErrorKind::FileTooLarge
                }
            );
        }
        let mut bytes = Vec::new();
        part(
            &mut bytes,
            "metadata",
            None,
            None,
            query().to_string().as_bytes(),
        );
        let mut multipart = decode_multipart(bytes).await;
        metadata(&mut multipart, 8192, &policy, &original)
            .await
            .unwrap();
        assert!(
            spool(&mut multipart, 8192, &original, &|| original.check(), None)
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn incomplete_stream_cancels_or_expires_without_waiting_for_the_body() {
        use futures_util::StreamExt;
        for deadline in [false, true] {
            let policy = policy();
            let cancel = Cancellation::default();
            let original = RequestContext::new(
                policy.content(),
                &cancel,
                Instant::now() + Duration::from_millis(if deadline { 250 } else { 5000 }),
            )
            .unwrap();
            let mut bytes = Vec::new();
            part(
                &mut bytes,
                "metadata",
                None,
                None,
                query().to_string().as_bytes(),
            );
            bytes.extend_from_slice(b"--boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"file.zip\"\r\n\r\npartial");
            let body = futures_util::stream::once(async {
                Ok::<_, io::Error>(axum::body::Bytes::from(bytes))
            })
            .chain(futures_util::stream::pending());
            let mut multipart = Multipart::from_request(
                Request::builder()
                    .header("content-type", "multipart/form-data; boundary=boundary")
                    .body(Body::from_stream(body))
                    .unwrap(),
                &(),
            )
            .await
            .unwrap();
            metadata(&mut multipart, 8192, &policy, &original)
                .await
                .unwrap();
            let cancellation = cancel.clone();
            let signal = tokio::spawn(async move {
                if !deadline {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    cancellation.cancel();
                }
            });
            let error = tokio::time::timeout(
                Duration::from_secs(5),
                spool(&mut multipart, 8192, &original, &|| original.check(), None),
            )
            .await
            .unwrap()
            .unwrap_err();
            assert_eq!(
                error.kind(),
                if deadline {
                    io::ErrorKind::TimedOut
                } else {
                    io::ErrorKind::Interrupted
                }
            );
            signal.await.unwrap();
        }
    }
}
