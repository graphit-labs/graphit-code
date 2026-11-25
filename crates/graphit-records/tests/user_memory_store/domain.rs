use super::*;
use crate::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{
        ChangeMemoryImportance, CreateMemory, DeleteMemory, Memory, MemoryId, MemoryMutation,
        MemoryReference, MemoryReferenceScope, MemoryStoreError, UpdateMemory, revision, rows,
        write,
    },
    reference_projection::{LogicalScope, ReferenceTargetKind},
};
use graphit_core::{
    artifact_store::UserIdentity, file_lock::Cancellation, installation_identity::InstallationId,
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use std::{
    num::NonZeroU128,
    time::{Duration, Instant},
};

fn request(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 1048576,
            max_output_bytes: 1048576,
            max_depth: 64,
            max_nodes: 65536,
            max_scalar_bytes: 65536,
            max_collection_entries: 65536,
        },
        cancel,
        Instant::now() + Duration::from_secs(120),
    )
    .unwrap()
}
fn owner() -> UserMemoryOwner {
    UserMemoryOwner {
        installation_id: InstallationId::new("unit-1").unwrap(),
        identity: UserIdentity::new("Literal Issuer".into(), " Subject ".into()).unwrap(),
    }
}
fn input(scope: &UserMemoryScopeId) -> CreateMemory {
    CreateMemory {
        idempotency_key: "create".into(),
        title: " Title ".into(),
        body: " Body ".into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Decision,
            mandatory: true,
            important: false,
        },
        tags: vec![" Tag ".into()],
        references: Some(vec![
            MemoryReference {
                target_kind: ReferenceTargetKind::Memory,
                scope: MemoryReferenceScope::User(scope.as_str().into()),
                id: " opaque memory ".into(),
                context: None,
                relation: " literal ".into(),
                field: None,
            };
            2
        ]),
    }
}
fn memory(input: &CreateMemory) -> Memory {
    Memory {
        id: MemoryId::new(NonZeroU128::new(1).unwrap()),
        revision: 1,
        title: input.title.clone(),
        body: input.body.clone(),
        attributes: input.attributes,
        tags: input.tags.clone(),
        references: input.references.clone().unwrap(),
        reference_generation: 1,
        created_by: ulid::Ulid::from(10u128).to_string(),
        updated_by: ulid::Ulid::from(10u128).to_string(),
        created_at: 11,
        updated_at: 11,
    }
}
fn update(body: Option<&str>, refs: Option<Vec<MemoryReference>>) -> MemoryMutation {
    MemoryMutation::Update(UpdateMemory {
        expected_revision: 1,
        idempotency_key: "update".into(),
        reason: " Literal reason ".into(),
        title: None,
        body: body.map(str::to_owned),
        classification: None,
        mandatory: None,
        important: None,
        tags: None,
        references: refs,
    })
}

#[test]
fn pure_user_creation_and_snapshot_preserve_literal_references_and_duplicates() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let scope_id = UserMemoryScopeId::from_owner(&owner()).unwrap();
    let scope = LogicalScope::User(scope_id.as_str());
    let input = input(&scope_id);
    let refs = write::validate_input_in_scope(request.snapshot(), scope, &input).unwrap();
    assert_eq!(refs, input.references.clone().unwrap());
    assert_eq!(refs[0], refs[1]);
    assert_eq!(refs[0].id, " opaque memory ");
    assert_eq!(refs[0].relation, " literal ");
    rows::valid_memory_in_scope(request.snapshot(), scope, &memory(&input)).unwrap();
    let mut absent = input.clone();
    absent.references = None;
    assert!(
        write::validate_input_in_scope(request.snapshot(), scope, &absent)
            .unwrap()
            .is_empty()
    );
    absent.references = Some(vec![]);
    assert!(
        write::validate_input_in_scope(request.snapshot(), scope, &absent)
            .unwrap()
            .is_empty()
    );
    assert_eq!(input.references.unwrap().len(), 2);
}

#[test]
fn pure_user_mutations_preserve_domain_revision_and_independent_reference_generation() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let scope_id = UserMemoryScopeId::from_owner(&owner()).unwrap();
    let scope = LogicalScope::User(scope_id.as_str());
    let before = memory(&input(&scope_id));
    let body_change = update(Some("Changed body"), None);
    revision::validate_in_scope(request.snapshot(), scope, &body_change, 1024).unwrap();
    let after = revision::derive_in_scope(
        request.snapshot(),
        scope,
        &before,
        &body_change,
        &before.updated_by,
        12,
    )
    .unwrap();
    assert_eq!((after.revision, after.reference_generation), (2, 1));
    assert_eq!(after.references, before.references);
    assert_eq!(after.attributes, before.attributes);
    assert_eq!(before.body, " Body ");
    let cleared = revision::derive_in_scope(
        request.snapshot(),
        scope,
        &before,
        &update(None, Some(vec![])),
        &before.updated_by,
        12,
    )
    .unwrap();
    assert!(cleared.references.is_empty());
    assert_eq!((cleared.revision, cleared.reference_generation), (2, 2));
    let importance = ChangeMemoryImportance {
        expected_revision: 1,
        idempotency_key: "promote".into(),
        reason: "promote".into(),
    };
    let promoted = revision::derive_in_scope(
        request.snapshot(),
        scope,
        &before,
        &MemoryMutation::Promote(importance.clone()),
        &before.updated_by,
        12,
    )
    .unwrap();
    assert!(promoted.attributes.important && promoted.attributes.mandatory);
    assert_eq!(promoted.reference_generation, 1);
    assert!(matches!(
        revision::derive_in_scope(
            request.snapshot(),
            scope,
            &promoted,
            &MemoryMutation::Promote(importance),
            &before.updated_by,
            13,
        ),
        Err(MemoryStoreError::NoChange)
    ));
    let deleted = revision::derive_in_scope(
        request.snapshot(),
        scope,
        &before,
        &MemoryMutation::Delete(DeleteMemory {
            expected_revision: 1,
            idempotency_key: "delete".into(),
            reason: "delete".into(),
        }),
        &before.updated_by,
        12,
    )
    .unwrap();
    assert!(deleted.references.is_empty());
    assert_eq!(deleted.reference_generation, 2);
    assert_eq!(deleted.attributes, before.attributes);
    assert!(matches!(
        revision::derive_in_scope(
            request.snapshot(),
            scope,
            &before,
            &body_change,
            &before.updated_by,
            10,
        ),
        Err(MemoryStoreError::Invalid("backward Memory clock"))
    ));
}

#[test]
fn pure_user_scope_preserves_original_errors_and_rejects_invalid_input_before_derivation() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let scope_id = UserMemoryScopeId::from_owner(&owner()).unwrap();
    let scope = LogicalScope::User(scope_id.as_str());
    let mut input = input(&scope_id);
    input.references.as_mut().unwrap()[0].context = Some("invalid memory context".into());
    assert!(write::validate_input_in_scope(request.snapshot(), scope, &input).is_err());
    assert!(matches!(
        revision::validate_in_scope(request.snapshot(), scope, &update(Some("changed"), None), 1,),
        Err(MemoryStoreError::Limit("mutation input bytes"))
    ));
    cancel.cancel();
    assert!(matches!(
        write::validate_input_in_scope(request.snapshot(), scope, &input,),
        Err(MemoryStoreError::Request(_))
    ));
    assert!(matches!(
        revision::validate_in_scope(
            request.snapshot(),
            scope,
            &update(Some("changed"), None),
            1024,
        ),
        Err(MemoryStoreError::Request(_))
    ));
}
