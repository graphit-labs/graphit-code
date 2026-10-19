//! Web mutation inputs and safe receipts over the existing native Memory stores.
use super::{Class, Query, limits};
use crate::task_server::{TaskPolicy, browser::codec, request::Arguments};
use graphit_auth::{ProjectWriteAdmission, UserWriteAdmission};
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_branch_store::DeltaRecoveryKey;
use graphit_hub::native::ContentLimits;
use graphit_records::{
    memory_attributes::MemoryAttributes,
    memory_store::{
        CreateMemory, DeleteMemory, Memory, MemoryId, MemoryReference, MemoryStoreError,
        MemoryStoreWriteLimits, NativeMemoryStore, UpdateMemory,
    },
    user_memory_store::{
        NativeUserMemoryStore, UserMemoryGeneration, UserMemoryOwner, UserMemoryScopeId,
    },
};
use serde::{Deserialize, Serialize};
use std::{num::NonZeroUsize, sync::Arc};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Attributes {
    classification: Class,
    mandatory: bool,
    important: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Create {
    idempotency_key: String,
    title: String,
    body: String,
    attributes: Attributes,
    tags: Vec<String>,
    references: Option<Vec<MemoryReference>>,
}
impl From<Create> for CreateMemory {
    fn from(input: Create) -> Self {
        Self {
            idempotency_key: input.idempotency_key,
            title: input.title,
            body: input.body,
            attributes: MemoryAttributes {
                classification: input.attributes.classification.native(),
                mandatory: input.attributes.mandatory,
                important: input.attributes.important,
            },
            tags: input.tags,
            references: input.references,
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Update {
    id: MemoryId,
    expected_revision: u64,
    idempotency_key: String,
    reason: String,
    title: Option<String>,
    body: Option<String>,
    classification: Option<Class>,
    mandatory: Option<bool>,
    important: Option<bool>,
    tags: Option<Vec<String>>,
    references: Option<Vec<MemoryReference>>,
}
impl Update {
    fn into_native(self) -> (MemoryId, UpdateMemory) {
        (
            self.id,
            UpdateMemory {
                expected_revision: self.expected_revision,
                idempotency_key: self.idempotency_key,
                reason: self.reason,
                title: self.title,
                body: self.body,
                classification: self.classification.map(Class::native),
                mandatory: self.mandatory,
                important: self.important,
                tags: self.tags,
                references: self.references,
            },
        )
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Delete {
    id: MemoryId,
    expected_revision: u64,
    idempotency_key: String,
    reason: String,
}
impl Delete {
    fn into_native(self) -> (MemoryId, DeleteMemory) {
        (
            self.id,
            DeleteMemory {
                expected_revision: self.expected_revision,
                idempotency_key: self.idempotency_key,
                reason: self.reason,
            },
        )
    }
}
#[derive(Deserialize, Serialize)]
pub(crate) enum Input {
    Create(Create),
    Update(Update),
    Delete(Delete),
}
impl Input {
    fn key(&self) -> &str {
        match self {
            Self::Create(v) => &v.idempotency_key,
            Self::Update(v) => &v.idempotency_key,
            Self::Delete(v) => &v.idempotency_key,
        }
    }
    fn id(&self) -> Option<MemoryId> {
        match self {
            Self::Create(_) => None,
            Self::Update(v) => Some(v.id),
            Self::Delete(v) => Some(v.id),
        }
    }
}
pub(crate) fn parse_project(
    bytes: &[u8],
    limits: ContentLimits,
    operation: &str,
) -> Result<Arguments, &'static str> {
    let (selection, input) = match operation {
        "memory_create" => {
            let body: codec::Body<Create> = codec::parse_body(bytes, limits)?;
            (body.selection, Input::Create(body.input))
        }
        "memory_update" => {
            let body: codec::Body<Update> = codec::parse_body(bytes, limits)?;
            (body.selection, Input::Update(body.input))
        }
        "memory_delete" => {
            let body: codec::Body<Delete> = codec::parse_body(bytes, limits)?;
            (body.selection, Input::Delete(body.input))
        }
        _ => return Err("invalid_arguments"),
    };
    codec::arguments(selection, Query::Mutation(Box::new(input)))
}
pub(crate) fn parse_user(
    bytes: &[u8],
    limits: ContentLimits,
    operation: &str,
) -> Result<Arguments, &'static str> {
    use crate::foreground_user_memory::Input as Body;
    let input = match operation {
        "user_memory_create" => Input::Create(codec::decode::<Body<Create>>(bytes, limits)?.input),
        "user_memory_update" => Input::Update(codec::decode::<Body<Update>>(bytes, limits)?.input),
        "user_memory_delete" => Input::Delete(codec::decode::<Body<Delete>>(bytes, limits)?.input),
        _ => return Err("invalid_arguments"),
    };
    Ok(Arguments {
        selection: Default::default(),
        input: Some(
            serde_json::to_value(Query::Mutation(Box::new(input)))
                .map_err(|_| "invalid_arguments")?,
        ),
    })
}
fn write_limits(policy: &TaskPolicy) -> Result<MemoryStoreWriteLimits, MemoryStoreError> {
    let positive = |n| NonZeroUsize::new(n).ok_or(MemoryStoreError::Invalid("Memory write policy"));
    Ok(MemoryStoreWriteLimits {
        read: limits(policy)?,
        artifacts: policy.write.artifacts,
        max_input_bytes: positive(policy.content.max_input_bytes)?,
        max_row_bytes: positive(policy.content.max_output_bytes)?,
    })
}

#[derive(Serialize)]
struct Recovery<'a> {
    corpus_uri: &'a str,
    physical_branch: Option<&'a str>,
    base_version: u64,
    transaction_id: &'a str,
}
impl<'a> From<&'a DeltaRecoveryKey> for Recovery<'a> {
    fn from(key: &'a DeltaRecoveryKey) -> Self {
        Self {
            corpus_uri: key.corpus_uri(),
            physical_branch: key.physical_branch(),
            base_version: key.base_version(),
            transaction_id: key.transaction_id(),
        }
    }
}
enum Identity {
    Project(codec::AdmittedSelection),
    User {
        owner: UserMemoryOwner,
        scope_id: UserMemoryScopeId,
    },
}
/// Owns only the safe receipt and actual causal SDK error; no corpus or snapshot.
pub(crate) struct Output {
    identity: Identity,
    id: Option<MemoryId>,
    revision: Option<u64>,
    key: String,
    failure: Option<Box<MemoryStoreError>>,
    delivery_failure: Option<Box<MemoryStoreError>>,
    maximum: usize,
}
#[derive(Serialize)]
struct IndexPublication<'a> {
    sdk_version: u64,
    generation: Option<&'a UserMemoryGeneration>,
    recovery_key: Recovery<'a>,
}
#[derive(Serialize)]
struct Receipt<'a> {
    state: &'static str,
    id: Option<MemoryId>,
    revision: Option<u64>,
    idempotency_key: &'a str,
    sdk_version: Option<u64>,
    generation: Option<&'a UserMemoryGeneration>,
    recovery_key: Option<Recovery<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    index_publication: Option<IndexPublication<'a>>,
    completion_error: Option<&'static str>,
    delivery_error: Option<&'static str>,
}
pub(crate) fn error_code(error: &MemoryStoreError) -> &'static str {
    match error {
        MemoryStoreError::Auth(_) => "authentication_or_permission_denied",
        MemoryStoreError::Request(_) => "request_cancelled_or_expired",
        MemoryStoreError::IdempotencyConflict => "idempotency_conflict",
        MemoryStoreError::RevisionConflict { .. } => "revision_conflict",
        MemoryStoreError::NotFound => "memory_not_found",
        MemoryStoreError::NoChange => "no_change",
        MemoryStoreError::Invalid(_) => "invalid_arguments",
        MemoryStoreError::Json(e)
            if e.io_error_kind() == Some(std::io::ErrorKind::FileTooLarge) =>
        {
            "response_limit"
        }
        MemoryStoreError::Confirmed { cause, .. }
        | MemoryStoreError::UserConfirmed { cause, .. }
        | MemoryStoreError::IndexConfirmed { cause, .. }
        | MemoryStoreError::UserIndexConfirmed { cause, .. } => error_code(cause),
        MemoryStoreError::Commit(_) => "publication_uncertain",
        _ => "memory_store_failure",
    }
}
impl Output {
    fn new(identity: Identity, input: &Input, maximum: usize) -> Self {
        Self {
            identity,
            id: input.id(),
            revision: None,
            key: input.key().to_owned(),
            failure: None,
            delivery_failure: None,
            maximum,
        }
    }
    fn observe(
        &mut self,
        result: Result<Memory, MemoryStoreError>,
    ) -> Result<(), MemoryStoreError> {
        match result {
            Ok(memory) => {
                self.id = Some(memory.id);
                self.revision = Some(memory.revision);
            }
            Err(
                error @ (MemoryStoreError::Confirmed { .. }
                | MemoryStoreError::UserConfirmed { .. }
                | MemoryStoreError::IndexConfirmed { .. }
                | MemoryStoreError::UserIndexConfirmed { .. }
                | MemoryStoreError::Commit(_)),
            ) => self.failure = Some(Box::new(error)),
            Err(error) => return Err(error),
        }
        Ok(())
    }
    pub(crate) fn completion_failure(&mut self, error: MemoryStoreError) {
        if self.delivery_failure.is_none() {
            self.delivery_failure = Some(Box::new(error));
        }
    }
    fn receipt(&self) -> Receipt<'_> {
        let mut failure = self.failure.as_deref();
        let mut state = if failure.is_some() {
            "not_applied"
        } else {
            "completed"
        };
        let (mut sdk_version, mut generation, mut recovery_key, mut index_publication) =
            (None, None, None, None);
        while let Some(error) = failure {
            failure = match error {
                MemoryStoreError::Confirmed { version, cause } => {
                    if state != "confirmed" {
                        state = "confirmed";
                        sdk_version = Some(*version);
                    }
                    Some(cause.as_ref())
                }
                MemoryStoreError::UserConfirmed { publication, cause } => {
                    if state != "confirmed" {
                        state = "confirmed";
                        sdk_version = Some(publication.generation.sdk_version);
                        generation = Some(&publication.generation);
                        recovery_key = Some(Recovery::from(&publication.recovery_key));
                    }
                    Some(cause.as_ref())
                }
                MemoryStoreError::IndexConfirmed { publication, cause } => {
                    index_publication = Some(IndexPublication {
                        sdk_version: publication.generation.sdk_version,
                        generation: None,
                        recovery_key: Recovery::from(&publication.recovery_key),
                    });
                    Some(cause.as_ref())
                }
                MemoryStoreError::UserIndexConfirmed { publication, cause } => {
                    index_publication = Some(IndexPublication {
                        sdk_version: publication.generation.sdk_version,
                        generation: Some(&publication.generation),
                        recovery_key: Recovery::from(&publication.recovery_key),
                    });
                    Some(cause.as_ref())
                }
                MemoryStoreError::Commit(error) => {
                    if state != "confirmed" {
                        state = "uncertain";
                        recovery_key = Some(Recovery::from(error.recovery_key()));
                    }
                    None
                }
                _ => None,
            };
        }
        Receipt {
            state,
            id: self.id,
            revision: self.revision,
            idempotency_key: &self.key,
            sdk_version,
            generation,
            recovery_key,
            index_publication,
            completion_error: self.failure.as_deref().map(error_code),
            delivery_error: self.delivery_failure.as_deref().map(error_code),
        }
    }
    pub(crate) fn encode(&self) -> Result<Vec<u8>, serde_json::Error> {
        let receipt = self.receipt();
        match &self.identity {
            Identity::Project(selection) => codec::encode(selection, &receipt, self.maximum),
            Identity::User { owner, scope_id } => {
                #[derive(Serialize)]
                struct Response<'a> {
                    scope_id: &'a UserMemoryScopeId,
                    #[serde(rename = "currentOwner")]
                    current_owner: &'a UserMemoryOwner,
                    native: Receipt<'a>,
                }
                codec::encode_response(
                    &Response {
                        scope_id,
                        current_owner: owner,
                        native: receipt,
                    },
                    self.maximum,
                )
            }
        }
    }
    pub(crate) fn encoding_failure(self, error: serde_json::Error) -> axum::response::Response {
        use axum::{
            http::{HeaderValue, StatusCode},
            response::IntoResponse,
        };
        let receipt = self.receipt();
        let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
        let headers = response.headers_mut();
        headers.insert(
            "x-graphit-memory-outcome",
            HeaderValue::from_static(receipt.state),
        );
        if let Some(id) = receipt.id {
            headers.insert(
                "x-graphit-memory-id",
                HeaderValue::from_str(&id.to_string()).expect("canonical ASCII ULID"),
            );
        }
        if let Some(revision) = receipt.revision {
            headers.insert(
                "x-graphit-memory-revision",
                HeaderValue::from_str(&revision.to_string()).expect("decimal u64"),
            );
        }
        if let Some(version) = receipt.sdk_version {
            headers.insert(
                "x-graphit-memory-sdk-version",
                HeaderValue::from_str(&version.to_string()).expect("decimal u64"),
            );
        }
        if let Some(publication) = &receipt.index_publication {
            headers.insert(
                "x-graphit-memory-index-sdk-version",
                HeaderValue::from_str(&publication.sdk_version.to_string()).expect("decimal u64"),
            );
        }
        let code = if error.io_error_kind() == Some(std::io::ErrorKind::FileTooLarge) {
            "response_limit"
        } else {
            "response_encoding_failed"
        };
        headers.insert(
            "x-graphit-memory-response-error",
            HeaderValue::from_static(code),
        );
        if let Some(code) = receipt.completion_error {
            headers.insert(
                "x-graphit-memory-completion-error",
                HeaderValue::from_static(code),
            );
        }
        if let Some(code) = receipt.delivery_error {
            headers.insert(
                "x-graphit-memory-delivery-error",
                HeaderValue::from_static(code),
            );
        }
        // Retain the encoder and any SDK failure together until response construction;
        // no second store operation or serialization of record content occurs.
        eprintln!(
            "Memory receipt encoding failed ({}) : {error}",
            receipt.state
        );
        response
    }
}
pub(crate) async fn project(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    policy: &TaskPolicy,
    selection: codec::AdmittedSelection,
    input: Input,
) -> Result<Output, MemoryStoreError> {
    let limits = write_limits(policy)?;
    let mut output = Output::new(
        Identity::Project(selection),
        &input,
        policy.content.max_output_bytes,
    );
    let result = match input {
        Input::Create(input) => {
            NativeMemoryStore::create(original, admission, input.into(), limits).await
        }
        Input::Update(input) => {
            let (id, input) = input.into_native();
            NativeMemoryStore::update(original, admission, id, input, limits).await
        }
        Input::Delete(input) => {
            let (id, input) = input.into_native();
            NativeMemoryStore::delete(original, admission, id, input, limits).await
        }
    };
    output.observe(result)?;
    if let Err(error) = admission
        .recheck()
        .map(|_| ())
        .map_err(MemoryStoreError::Auth)
        .and_then(|()| original.check().map_err(MemoryStoreError::Request))
    {
        output.completion_failure(error);
    }
    Ok(output)
}
pub(crate) async fn user(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    policy: &TaskPolicy,
    input: Input,
) -> Result<Output, MemoryStoreError> {
    use graphit_core::artifact_store::StoreOwner;
    let scope = admission.user()?.live;
    let StoreOwner::User {
        installation_id,
        identity,
    } = scope.owner()
    else {
        return Err(MemoryStoreError::WrongAdmission);
    };
    let owner = UserMemoryOwner {
        installation_id: installation_id.clone(),
        identity: identity.clone(),
    };
    let scope_id = UserMemoryScopeId::from_owner(&owner)?;
    let limits = write_limits(policy)?;
    let mut output = Output::new(
        Identity::User { owner, scope_id },
        &input,
        policy.content.max_output_bytes,
    );
    let result = match input {
        Input::Create(input) => {
            NativeUserMemoryStore::create(original, admission, input.into(), limits).await
        }
        Input::Update(input) => {
            let (id, input) = input.into_native();
            NativeUserMemoryStore::update(original, admission, id, input, limits).await
        }
        Input::Delete(input) => {
            let (id, input) = input.into_native();
            NativeUserMemoryStore::delete(original, admission, id, input, limits).await
        }
    };
    output.observe(result)?;
    if let Err(error) = admission
        .recheck()
        .map(|_| ())
        .map_err(MemoryStoreError::Auth)
        .and_then(|()| original.check().map_err(MemoryStoreError::Request))
    {
        output.completion_failure(error);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn content() -> ContentLimits {
        ContentLimits {
            max_input_bytes: 8192,
            max_output_bytes: 8192,
            max_depth: 1,
            max_nodes: 1,
            max_scalar_bytes: 8192,
            max_collection_entries: 64,
        }
    }
    #[test]
    fn mutation_decoder_preserves_u64_and_rejects_authority_duplicates_and_types() {
        let body=br#"{"input":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","expected_revision":9007199254740993,"idempotency_key":"delete","reason":"Exact CAS"}}"#;
        let parsed = parse_user(body, content(), "user_memory_delete").unwrap();
        let query: Query = serde_json::from_value(parsed.input.unwrap()).unwrap();
        let Query::Mutation(input) = query else {
            panic!("mutation expected")
        };
        let Input::Delete(input) = *input else {
            panic!("delete expected")
        };
        assert_eq!(input.expected_revision, 9007199254740993);
        for invalid in [
            br#"{"input":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","expected_revision":1,"idempotency_key":"x","reason":"r","actor":"free"}}"#.as_slice(),
            br#"{"selection":{},"input":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","expected_revision":1,"idempotency_key":"x","reason":"r"}}"#,
            br#"{"input":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","expected_revision":1,"expected_revision":2,"idempotency_key":"x","reason":"r"}}"#,
            br#"{"input":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","expected_revision":1.5,"idempotency_key":"x","reason":"r"}}"#,
        ] {assert!(parse_user(invalid,content(),"user_memory_delete").is_err());}
    }
    #[test]
    fn confirmed_receipt_preserves_sdk_version_and_cause_without_content() {
        let selection = codec::AdmittedSelection {
            project_dir: graphit_core::native_path::NativePath::new(std::env::temp_dir()).unwrap(),
            project_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            project_version: "main".into(),
        };
        let input = Input::Delete(Delete {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            expected_revision: 3,
            idempotency_key: "known-effect".into(),
            reason: "Delete".into(),
        });
        let mut output = Output::new(Identity::Project(selection), &input, 8192);
        output
            .observe(Err(MemoryStoreError::Confirmed {
                version: 9007199254740993,
                cause: Box::new(MemoryStoreError::Request(
                    std::io::ErrorKind::Interrupted.into(),
                )),
            }))
            .unwrap();
        output.completion_failure(MemoryStoreError::Auth(graphit_auth::AuthError::Denied));
        let bytes = output.encode().unwrap();
        assert!(
            std::str::from_utf8(&bytes)
                .unwrap()
                .contains("9007199254740993")
        );
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["native"]["state"], "confirmed");
        assert_eq!(
            value["native"]["completion_error"],
            "request_cancelled_or_expired"
        );
        assert_eq!(
            value["native"]["delivery_error"],
            "authentication_or_permission_denied"
        );
        assert_eq!(value["native"]["idempotency_key"], "known-effect");
        assert!(value["native"].get("body").is_none());
        assert!(
            matches!(output.failure.as_deref(),Some(MemoryStoreError::Confirmed {cause,..})
            if matches!(cause.as_ref(),MemoryStoreError::Request(error) if error.kind()==std::io::ErrorKind::Interrupted))
        );
    }
    #[tokio::test]
    async fn lexical_receipts_keep_index_and_record_effects_separate() {
        use graphit_core::{artifact_store::UserIdentity, installation_identity::InstallationId};
        use graphit_data::{
            lance_branch_store::{JsonFtsIndexMode, LanceBranchStore},
            lance_module_store::LanceModuleStore,
        };
        use graphit_records::{
            memory_store::{MemoryGeneration, MemoryLexicalIndexPublication},
            user_memory_store::{UserMemoryLexicalIndexPublication, UserMemoryPublication},
        };
        let temp = tempfile::tempdir().unwrap();
        let uri = temp
            .path()
            .join("memory.lance")
            .to_str()
            .unwrap()
            .to_owned();
        let batch = arrow_array::RecordBatch::try_from_iter([
            (
                "id",
                Arc::new(arrow_array::StringArray::from(vec!["receipt"])) as arrow_array::ArrayRef,
            ),
            (
                "payload",
                Arc::new(arrow_array::StringArray::from(vec![
                    r#"{"body":"receipt"}"#,
                ])) as arrow_array::ArrayRef,
            ),
        ])
        .unwrap();
        let schema = batch.schema();
        LanceModuleStore::create(&uri, Arc::clone(&schema), vec![batch])
            .await
            .unwrap();
        let store = LanceBranchStore::open(&uri, schema, None).await.unwrap();
        let before = store.capture_target(None).await.unwrap();
        let pending = before
            .prepare_json_fts_index(
                "payload",
                "memory-json-content-v1",
                JsonFtsIndexMode::Create,
            )
            .await
            .unwrap();
        let index_key = pending.recovery_key().clone();
        let indexed = pending.commit().await.unwrap();
        let next = indexed
            .prepare_json_fts_index(
                "payload",
                "memory-json-content-v1",
                JsonFtsIndexMode::Replace,
            )
            .await
            .unwrap();
        let stale = indexed
            .prepare_json_fts_index(
                "payload",
                "memory-json-content-v1",
                JsonFtsIndexMode::Replace,
            )
            .await
            .unwrap();
        let record_key = next.recovery_key().clone();
        let committed = next.commit().await.unwrap();
        let commit_error = match stale.commit().await {
            Err(error) => error,
            Ok(_) => panic!("stale SDK index unexpectedly committed"),
        };
        let uncertain_key = commit_error.recovery_key().clone();
        assert_ne!(index_key.transaction_id(), uncertain_key.transaction_id());
        // SDK commits supply real versions/keys; typed wrappers test only the HTTP projection.
        // Records integration tests cover CRUD/Auth and phase ordering separately.
        let project_id = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        let selection = || codec::AdmittedSelection {
            project_dir: graphit_core::native_path::NativePath::new(temp.path().to_path_buf())
                .unwrap(),
            project_id,
            project_version: "main".into(),
        };
        let publication = MemoryLexicalIndexPublication {
            generation: MemoryGeneration {
                sdk_version: indexed.version(),
                project_id,
                project_version: "main".into(),
                physical_branch: indexed.physical_branch().map(str::to_owned),
                inherited_target: None,
            },
            recovery_key: index_key.clone(),
        };
        let input = Input::Delete(Delete {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            expected_revision: 3,
            idempotency_key: "phase-receipt".into(),
            reason: "Delete".into(),
        });
        let mut refused = Output::new(Identity::Project(selection()), &input, 8192);
        refused
            .observe(Err(MemoryStoreError::IndexConfirmed {
                publication: Box::new(publication),
                cause: Box::new(MemoryStoreError::RevisionConflict {
                    expected: 3,
                    actual: 4,
                }),
            }))
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&refused.encode().unwrap()).unwrap();
        assert_eq!(json["native"]["state"], "not_applied");
        assert!(json["native"]["sdk_version"].is_null());
        assert!(json["native"]["recovery_key"].is_null());
        assert_eq!(json["native"]["completion_error"], "revision_conflict");
        assert_eq!(
            json["native"]["index_publication"]["sdk_version"],
            indexed.version()
        );
        assert_eq!(
            json["native"]["index_publication"]["recovery_key"]["transaction_id"],
            index_key.transaction_id()
        );
        assert!(json["native"].get("body").is_none());
        refused.maximum = 1;
        let error = refused.encode().unwrap_err();
        let fallback = refused.encoding_failure(error);
        assert_eq!(
            fallback.headers()["x-graphit-memory-outcome"],
            "not_applied"
        );
        assert_eq!(
            fallback.headers()["x-graphit-memory-index-sdk-version"],
            indexed.version().to_string()
        );
        assert!(
            !fallback
                .headers()
                .contains_key("x-graphit-memory-sdk-version")
        );
        let owner = UserMemoryOwner {
            installation_id: InstallationId::new("receipt-fixture").unwrap(),
            identity: UserIdentity::new("https://issuer.example".to_owned(), "subject".to_owned())
                .unwrap(),
        };
        let user_generation = UserMemoryGeneration {
            owner: owner.clone(),
            sdk_version: indexed.version(),
            physical_branch: indexed.physical_branch().map(str::to_owned),
            inherited_target: None,
        };
        let user_publication = UserMemoryLexicalIndexPublication {
            generation: user_generation.clone(),
            recovery_key: index_key.clone(),
        };
        let identity = || Identity::User {
            scope_id: UserMemoryScopeId::from_owner(&owner).unwrap(),
            owner: owner.clone(),
        };
        let mut uncertain = Output::new(identity(), &input, 8192);
        uncertain
            .observe(Err(MemoryStoreError::UserIndexConfirmed {
                publication: Box::new(user_publication.clone()),
                cause: Box::new(MemoryStoreError::Commit(commit_error)),
            }))
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&uncertain.encode().unwrap()).unwrap();
        assert_eq!(json["native"]["state"], "uncertain");
        assert!(json["native"]["sdk_version"].is_null());
        assert_eq!(json["native"]["completion_error"], "publication_uncertain");
        assert_eq!(
            json["native"]["recovery_key"]["transaction_id"],
            uncertain_key.transaction_id()
        );
        assert_eq!(
            json["native"]["index_publication"]["recovery_key"]["transaction_id"],
            index_key.transaction_id()
        );
        assert_eq!(
            json["native"]["index_publication"]["generation"],
            serde_json::to_value(&user_generation).unwrap()
        );
        let mut confirmed = Output::new(identity(), &input, 8192);
        confirmed
            .observe(Err(MemoryStoreError::UserConfirmed {
                publication: Box::new(UserMemoryPublication {
                    generation: UserMemoryGeneration {
                        sdk_version: committed.version(),
                        ..user_generation
                    },
                    recovery_key: record_key.clone(),
                }),
                cause: Box::new(MemoryStoreError::UserIndexConfirmed {
                    publication: Box::new(user_publication),
                    cause: Box::new(MemoryStoreError::Request(
                        std::io::ErrorKind::Interrupted.into(),
                    )),
                }),
            }))
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&confirmed.encode().unwrap()).unwrap();
        assert_eq!(json["native"]["state"], "confirmed");
        assert_eq!(json["native"]["sdk_version"], committed.version());
        assert_eq!(
            json["native"]["recovery_key"]["transaction_id"],
            record_key.transaction_id()
        );
        assert_eq!(
            json["native"]["index_publication"]["sdk_version"],
            indexed.version()
        );
        assert_eq!(
            json["native"]["completion_error"],
            "request_cancelled_or_expired"
        );
    }

    #[tokio::test]
    async fn encoding_limit_keeps_known_effect_in_lossless_headers_without_body() {
        let selection = codec::AdmittedSelection {
            project_dir: graphit_core::native_path::NativePath::new(std::env::temp_dir()).unwrap(),
            project_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            project_version: "main".into(),
        };
        let input = Input::Delete(Delete {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            expected_revision: 3,
            idempotency_key: "known-effect".into(),
            reason: "Delete".into(),
        });
        let mut output = Output::new(Identity::Project(selection), &input, 1);
        output
            .observe(Err(MemoryStoreError::Confirmed {
                version: 9007199254740993,
                cause: Box::new(MemoryStoreError::Request(
                    std::io::ErrorKind::Interrupted.into(),
                )),
            }))
            .unwrap();
        let error = output.encode().unwrap_err();
        let response = output.encoding_failure(error);
        assert_eq!(
            response.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(response.headers()["x-graphit-memory-outcome"], "confirmed");
        assert_eq!(
            response.headers()["x-graphit-memory-sdk-version"],
            "9007199254740993"
        );
        assert_eq!(
            response.headers()["x-graphit-memory-completion-error"],
            "request_cancelled_or_expired"
        );
        assert_eq!(
            response.headers()["x-graphit-memory-response-error"],
            "response_limit"
        );
        assert!(
            axum::body::to_bytes(response.into_body(), 1)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
