use super::*;
use arrow_array::{
    Array, RecordBatch, UInt32Array,
    builder::{Float32Builder, ListBuilder},
    new_null_array,
};
use std::num::NonZeroU32;

fn identity(config: u8, dimensions: u32) -> MemoryEmbeddingIdentity {
    MemoryEmbeddingIdentity::new(
        [config; 32],
        NonZeroU32::new(dimensions).unwrap(),
        MEMORY_DOCUMENT_RECIPE.into(),
    )
    .unwrap()
}
fn source() -> Memory {
    Memory {
        id: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
        revision: 1,
        title: " título \n".into(),
        body: " Body β \n".into(),
        tags: vec![" tag ".into(), "z".into()],
        attributes: input("a").attributes,
        references: vec![],
        reference_generation: 1,
        created_by: ulid::Ulid::from(1u128).to_string(),
        updated_by: ulid::Ulid::from(1u128).to_string(),
        created_at: 1,
        updated_at: 1,
    }
}
fn replace_column(batch: &RecordBatch, col: usize, value: Arc<dyn Array>) -> RecordBatch {
    let mut columns = batch.columns().to_vec();
    columns[col] = value;
    RecordBatch::try_new(batch.schema(), columns).unwrap()
}
fn row(project: graphit_core::project_identity::ProjectId, memory: &Memory) -> RecordBatch {
    let document = prepare_memory_embedding_document(memory, &identity(0, 2)).unwrap();
    encode_memory_embedding_row(project, &document, &[1., 0.], write_limits()).unwrap()
}
fn project(f: &Fixture) -> graphit_core::project_identity::ProjectId {
    match f.project.owner() {
        graphit_core::artifact_store::StoreOwner::Project { project_id } => *project_id,
        _ => panic!("project fixture"),
    }
}

#[test]
fn literal_projection_identity_and_irrelevant_metadata_are_distinct() {
    let memory = source();
    let identity = identity(0, 2);
    let document = prepare_memory_embedding_document(&memory, &identity).unwrap();
    assert_eq!(
        document.text(),
        "[\" título \\n\",\" Body β \\n\",[\" tag \",\"z\"]]"
    );
    assert_eq!(document.identity(), &identity);
    assert_eq!(document.memory_id(), memory.id);
    assert_eq!(document.revision(), 1);
    let hash = document
        .fingerprint()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        hash,
        "93629a5895a9190d30f188eea795234d1b91054f0fbbf5b0be139040ec5adb54"
    );
    let mut irrelevant = memory.clone();
    irrelevant.id = "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap();
    irrelevant.revision = 3;
    irrelevant.attributes.important = true;
    irrelevant.updated_at = 50;
    irrelevant.updated_by = ulid::Ulid::from(2u128).to_string();
    irrelevant.references.push(MemoryReference {
        target_kind: ReferenceTargetKind::Knowledge,
        scope: MemoryReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap()),
        id: "doc".into(),
        context: None,
        relation: "supports".into(),
        field: None,
    });
    let unchanged = prepare_memory_embedding_document(&irrelevant, &identity).unwrap();
    assert_eq!(unchanged.text(), document.text());
    assert_eq!(unchanged.fingerprint(), document.fingerprint());
    assert_ne!(unchanged.memory_id(), document.memory_id());
    assert_eq!(unchanged.revision(), 3);
    for field in 0..3 {
        let mut changed = memory.clone();
        match field {
            0 => changed.title.push(' '),
            1 => changed.body.push('β'),
            _ => changed.tags.reverse(),
        }
        assert_ne!(
            prepare_memory_embedding_document(&changed, &identity)
                .unwrap()
                .fingerprint(),
            document.fingerprint()
        );
    }
    for changed_identity in [self::identity(1, 2), self::identity(0, 3)] {
        assert_ne!(
            prepare_memory_embedding_document(&memory, &changed_identity)
                .unwrap()
                .fingerprint(),
            document.fingerprint()
        );
    }
    assert!(
        MemoryEmbeddingIdentity::new([0; 32], NonZeroU32::new(2).unwrap(), "other-recipe".into())
            .is_err()
    );
    irrelevant.revision = 0;
    assert!(prepare_memory_embedding_document(&irrelevant, &identity).is_err());
}

#[test]
fn encoder_requires_actual_width_finite_values_and_text_budget() {
    let document = prepare_memory_embedding_document(&source(), &identity(0, 2)).unwrap();
    let project = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
    let batch = encode_memory_embedding_row(project, &document, &[0., 0.], write_limits()).unwrap();
    assert_eq!(batch.schema(), sdk::schema());
    assert_eq!(batch.num_columns(), 24);
    assert_eq!(
        sdk::string(&batch, 0, 0),
        format!("embedding/{}/1", document.memory_id())
    );
    assert_eq!(
        sdk::string(&batch, 17, 0),
        format!(
            "{{\"memory_id\":\"{}\",\"memory_revision\":1}}",
            document.memory_id()
        )
    );
    assert!(batch.columns().iter().all(|array| array.null_count() == 0));
    for invalid in [
        &[][..],
        &[1.][..],
        &[1., 0., 0.][..],
        &[f32::NAN, 0.][..],
        &[f32::INFINITY, 0.][..],
    ] {
        assert!(encode_memory_embedding_row(project, &document, invalid, write_limits()).is_err());
    }
    let mut limits = write_limits();
    limits.max_row_bytes = NonZeroUsize::new(1).unwrap();
    assert!(matches!(
        encode_memory_embedding_row(project, &document, &[1., 0.], limits),
        Err(MemoryStoreError::Limit(_))
    ));
    limits = write_limits();
    limits.read.max_query_bytes = NonZeroUsize::MIN;
    assert!(encode_memory_embedding_row(project, &document, &[1., 0.], limits).is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sdk_embedding_roundtrip_is_invisible_to_history_lexical_and_catalogue() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let memory = NativeMemoryStore::create(ctx.snapshot(), &write, input("embed"), write_limits())
        .await
        .unwrap();
    let old = sdk::pin(&f).await;
    let logical = sdk::all(&old).await;
    assert_eq!(logical.num_columns(), 24);
    for col in 18..24 {
        assert_eq!(logical.column(col).null_count(), logical.num_rows());
    }
    sdk::replace(&old, row(project(&f), &memory)).await;
    let fresh = sdk::pin(&f).await;
    assert!(fresh.version() > old.version());
    assert_eq!(sdk::all(&old).await.num_rows(), 3);
    assert_eq!(sdk::all(&fresh).await.num_rows(), 4);
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read, memory.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        memory
    );
    let (_, heads) = published(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
    );
    assert_eq!(heads.len(), 1);
    let list = NativeMemoryStore::list(
        ctx.snapshot(),
        &read,
        MemoryListFilter::default(),
        read_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(list, MemoryListRead::Published { ref memories, .. } if memories.len() == 1));
    let query = MemoryLexicalQuery {
        text: "Literal".into(),
        filter: MemoryListFilter::default(),
        include_deleted_history: false,
        top_k: NonZeroUsize::new(10).unwrap(),
    };
    let hits = NativeMemoryStore::search_lexical(ctx.snapshot(), &read, query, read_limits())
        .await
        .unwrap();
    assert!(
        matches!(hits, MemoryLexicalSearchRead::Published { total_hits: 1, ref hits, .. } if hits.len() == 1)
    );
    let mut query_only = read_limits();
    query_only.max_query_bytes = NonZeroUsize::MIN;
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read, memory.id, query_only)
            .await
            .unwrap()
            .unwrap()
            .memory,
        memory
    );
    assert_eq!(sdk::pin(&f).await.version(), fresh.version());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sdk_embedding_malformed_headers_payload_shape_and_foreign_project_fail_before_skip() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let memory =
        NativeMemoryStore::create(ctx.snapshot(), &write, input("bad-embed"), write_limits())
            .await
            .unwrap();
    let base = row(project(&f), &memory);
    let mut bad = vec![];
    for col in [4, 5, 6, 7, 11, 12, 16, 20, 21] {
        bad.push(sdk::text_column(&base, col, vec!["forged".into()]));
    }
    for col in [9, 10, 15] {
        bad.push(sdk::numbers(&base, col, vec![1]));
    }
    for col in [13, 14] {
        bad.push(sdk::flags(&base, col, vec![true]));
    }
    bad.push(sdk::text_column(
        &base,
        3,
        vec!["01KSH1CRFFG8Z74B5ZS78WW809".into()],
    ));
    bad.push(sdk::text_column(
        &base,
        17,
        vec![format!(
            "{{\"memory_id\":\"{}\",\"memory_revision\":2}}",
            memory.id
        )],
    ));
    bad.push(sdk::text_column(
        &base,
        17,
        vec![format!(
            "{{\"memory_id\":\"{}\",\"memory_revision\":1,\"unknown\":true}}",
            memory.id
        )],
    ));
    for col in 18..24 {
        bad.push(replace_column(
            &base,
            col,
            new_null_array(base.schema().field(col).data_type(), 1),
        ));
    }
    for dimension in [0, 1, 3] {
        bad.push(replace_column(
            &base,
            22,
            Arc::new(UInt32Array::from(vec![dimension])),
        ));
    }
    for values in [
        vec![Some(f32::NAN), Some(0.)],
        vec![Some(f32::INFINITY), Some(0.)],
        vec![None, Some(0.)],
    ] {
        let mut builder = ListBuilder::new(Float32Builder::new());
        for value in values {
            builder.values().append_option(value);
        }
        builder.append(true);
        bad.push(replace_column(&base, 23, Arc::new(builder.finish())));
    }
    let mut limits = read_limits();
    limits.batch_rows = NonZeroU32::new(1).unwrap();
    for (index, invalid) in bad.into_iter().enumerate() {
        sdk::replace(&sdk::pin(&f).await, invalid).await;
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &read, memory.id, limits)
                .await
                .is_err(),
            "case {index}"
        );
    }
    sdk::replace(&sdk::pin(&f).await, base.clone()).await;
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read, memory.id, limits)
            .await
            .unwrap()
            .is_some()
    );
    let pin = sdk::pin(&f).await;
    let logical = sdk::select(&sdk::all(&pin).await, "head");
    let forged = replace_column(&logical, 20, Arc::clone(base.column(20)));
    sdk::replace(&pin, forged).await;
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read, memory.id, limits)
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sdk_empty_old_schema_is_rejected_without_conversion_or_publication() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    sdk::empty_with_schema(&f, &ctx, sdk::legacy_schema()).await;
    let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    let store =
        graphit_data::lance_branch_store::LanceBranchStore::open(&uri, sdk::legacy_schema(), None)
            .await
            .unwrap();
    let before = store.capture_target(None).await.unwrap();
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read, source().id, read_limits())
            .await
            .is_err()
    );
    assert!(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .is_err()
    );
    assert!(
        NativeMemoryStore::list(
            ctx.snapshot(),
            &read,
            MemoryListFilter::default(),
            read_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeMemoryStore::create(ctx.snapshot(), &write, input("old"), write_limits())
            .await
            .is_err()
    );
    assert!(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Refresh,
            write_limits()
        )
        .await
        .is_err()
    );
    let reopened =
        graphit_data::lance_branch_store::LanceBranchStore::open(&uri, sdk::legacy_schema(), None)
            .await
            .unwrap()
            .capture_target(None)
            .await
            .unwrap();
    assert_eq!(reopened.version(), before.version());
}
