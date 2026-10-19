use super::{Memory, MemoryId, MemoryStoreError, MemoryStoreWriteLimits, charge, rows};
use arrow_array::{
    Array, BooleanArray, FixedSizeBinaryArray, Float32Array, ListArray, RecordBatch, StringArray,
    UInt32Array, UInt64Array,
    builder::{Float32Builder, ListBuilder},
};
use graphit_core::project_identity::ProjectId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{num::NonZeroU32, sync::Arc};

pub(crate) mod capture;
pub(crate) mod publish;
pub(crate) mod query;
pub use capture::{MemoryEmbeddingCorpus, MemoryEmbeddingCorpusRead};
pub use publish::{
    MemoryEmbeddingAttachOutcome, MemoryEmbeddingAttachment, MemoryEmbeddingPublication,
};
pub use query::{
    MemoryEmbeddingUnavailable, MemorySemanticHit, MemorySemanticSearchOptions,
    MemorySemanticSearchRead,
};

pub(crate) struct EmbeddingMetadata {
    pub(crate) revision: u64,
    pub(crate) identity: MemoryEmbeddingIdentity,
    pub(crate) fingerprint: [u8; 32],
}
#[derive(Clone, Copy)]
pub(crate) enum SourceOwner<'a> {
    Project(ProjectId),
    User(&'a crate::user_memory_store::UserMemoryOwner),
}
impl SourceOwner<'_> {
    pub(crate) fn layout(self) -> rows::MemoryLayout {
        match self {
            Self::Project(_) => rows::MemoryLayout::Project,
            Self::User(_) => rows::MemoryLayout::User,
        }
    }
    pub(crate) fn column(self, project: usize) -> usize {
        column(self.layout(), project)
    }
    pub(crate) fn schema(self) -> Arc<arrow_schema::Schema> {
        rows::schema_for(self.layout())
    }
    pub(crate) fn decode(
        self,
        batch: &RecordBatch,
        row: usize,
        original: &graphit_core::file_io::SnapshotContext,
    ) -> Result<bool, MemoryStoreError> {
        match self {
            Self::Project(id) => Ok(rows::decode(batch, row, original, id)?.is_some()),
            Self::User(owner) => {
                Ok(crate::user_memory_store::rows::decode(batch, row, original, owner)?.is_some())
            }
        }
    }
}
fn column(layout: rows::MemoryLayout, project: usize) -> usize {
    match layout {
        rows::MemoryLayout::Project => project,
        rows::MemoryLayout::User => {
            if project > 4 {
                project + 1
            } else {
                project
            }
        }
    }
}
pub(crate) fn metadata(
    batch: &RecordBatch,
    row: usize,
    layout: rows::MemoryLayout,
) -> Result<EmbeddingMetadata, MemoryStoreError> {
    let binary = |col| -> Result<[u8; 32], MemoryStoreError> {
        batch
            .column(column(layout, col))
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .ok_or(MemoryStoreError::Invalid("Memory embedding binary type"))?
            .value(row)
            .try_into()
            .map_err(|_| MemoryStoreError::Invalid("Memory embedding binary width"))
    };
    let dimensions = batch
        .column(column(layout, 22))
        .as_any()
        .downcast_ref::<UInt32Array>()
        .ok_or(MemoryStoreError::Invalid(
            "Memory embedding dimensions type",
        ))?
        .value(row);
    Ok(EmbeddingMetadata {
        revision: rows::number(batch, column(layout, 8), row)?,
        identity: MemoryEmbeddingIdentity::new(
            binary(18)?,
            NonZeroU32::new(dimensions)
                .ok_or(MemoryStoreError::Invalid("Memory embedding dimensions"))?,
            rows::text(batch, column(layout, 20), row)?.to_owned(),
        )?,
        fingerprint: binary(19)?,
    })
}
pub(crate) fn source_at(
    chain: super::lexical_search::HistoryView<'_>,
    revision: u64,
) -> Option<&Memory> {
    chain.sources().find(|m| m.revision == revision)
}
pub(crate) fn validate_source(
    metadata: &EmbeddingMetadata,
    chain: super::lexical_search::HistoryView<'_>,
) -> Result<(), MemoryStoreError> {
    let source = source_at(chain, metadata.revision)
        .ok_or(MemoryStoreError::Invalid("orphan Memory embedding"))?;
    let fingerprint = memory_embedding_fingerprint(source, &metadata.identity)?;
    if fingerprint != metadata.fingerprint {
        return Err(MemoryStoreError::Invalid(
            "Memory embedding source fingerprint",
        ));
    }
    Ok(())
}

// Hash the JSON string interior while serde streams the canonical source tuple.
// The inner JSON encoder has already escaped control characters; only quotes and
// backslashes need a second escape for the outer fingerprint tuple's text string.
struct DocumentFingerprintWriter<'a>(&'a mut Sha256);
impl std::io::Write for DocumentFingerprintWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut start = 0;
        for (index, byte) in bytes.iter().enumerate() {
            let escaped: &[u8] = match byte {
                b'"' => br#"\""#,
                b'\\' => br#"\\"#,
                _ => continue,
            };
            self.0.update(&bytes[start..index]);
            self.0.update(escaped);
            start = index + 1;
        }
        self.0.update(&bytes[start..]);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn memory_embedding_fingerprint(
    memory: &Memory,
    identity: &MemoryEmbeddingIdentity,
) -> Result<[u8; 32], MemoryStoreError> {
    if memory.revision == 0 {
        return Err(MemoryStoreError::Invalid(
            "Memory embedding source revision",
        ));
    }
    let mut header = serde_json::to_vec(&(
        &identity.recipe,
        "Document",
        &identity.vector_id,
        identity.dimensions.get(),
    ))?;
    // Replace the fixed tuple's closing bracket with the text string element.
    header.truncate(header.len() - 1);
    let mut digest = Sha256::new();
    digest.update(&header);
    digest.update(b",\"");
    serde_json::to_writer(
        DocumentFingerprintWriter(&mut digest),
        &(&memory.title, &memory.body, &memory.tags),
    )?;
    digest.update(b"\"]");
    Ok(digest.finalize().into())
}

pub const MEMORY_DOCUMENT_RECIPE: &str = "memory-document-json-v1";

/// Portable vector-space identity paired with the exact Document projection recipe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryEmbeddingIdentity {
    vector_id: [u8; 32],
    dimensions: NonZeroU32,
    recipe: String,
}
impl MemoryEmbeddingIdentity {
    pub fn new(
        vector_id: [u8; 32],
        dimensions: NonZeroU32,
        recipe: String,
    ) -> Result<Self, MemoryStoreError> {
        if recipe != MEMORY_DOCUMENT_RECIPE {
            return Err(MemoryStoreError::Invalid("Memory embedding recipe"));
        }
        Ok(Self {
            vector_id,
            dimensions,
            recipe,
        })
    }
    pub fn vector_id(&self) -> &[u8; 32] {
        &self.vector_id
    }
    pub fn dimensions(&self) -> NonZeroU32 {
        self.dimensions
    }
    pub fn recipe(&self) -> &str {
        &self.recipe
    }
}

/// Pure projection of one source revision; capture associates current/archive state later.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryEmbeddingDocument {
    memory_id: MemoryId,
    revision: u64,
    text: String,
    pub(crate) reusable_vector: Option<Vec<f32>>,
    fingerprint: [u8; 32],
    identity: MemoryEmbeddingIdentity,
}
impl MemoryEmbeddingDocument {
    pub fn reusable_vector(&self) -> Option<&[f32]> {
        self.reusable_vector.as_deref()
    }
    /// A validated vector from the same Memory and vector space with identical input.
    pub fn take_reusable_vector(&mut self) -> Option<Vec<f32>> {
        self.reusable_vector.take()
    }
    pub fn memory_id(&self) -> MemoryId {
        self.memory_id
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Consume the projected text without copying or applying the recipe again.
    pub fn into_text(self) -> String {
        self.text
    }
    pub fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    pub fn identity(&self) -> &MemoryEmbeddingIdentity {
        &self.identity
    }
}

pub fn prepare_memory_embedding_document(
    memory: &Memory,
    identity: &MemoryEmbeddingIdentity,
) -> Result<MemoryEmbeddingDocument, MemoryStoreError> {
    if memory.revision == 0 {
        return Err(MemoryStoreError::Invalid(
            "Memory embedding source revision",
        ));
    }
    let text = serde_json::to_string(&(&memory.title, &memory.body, &memory.tags))?;
    let bytes = serde_json::to_vec(&(
        &identity.recipe,
        "Document",
        &identity.vector_id,
        identity.dimensions.get(),
        &text,
    ))?;
    Ok(MemoryEmbeddingDocument {
        memory_id: memory.id,
        revision: memory.revision,
        text,
        reusable_vector: None,
        fingerprint: Sha256::digest(bytes).into(),
        identity: identity.clone(),
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmbeddingPayload {
    memory_id: MemoryId,
    memory_revision: u64,
}

/// Encode one derived physical row without publishing or performing model inference.
pub fn encode_memory_embedding_row(
    project_id: ProjectId,
    document: &MemoryEmbeddingDocument,
    vector: &[f32],
    limits: MemoryStoreWriteLimits,
) -> Result<RecordBatch, MemoryStoreError> {
    encode_row(
        rows::RowOwner::Project {
            project_id,
            version: "",
        },
        document,
        vector,
        limits,
    )
}
pub(crate) fn encode_row(
    owner: rows::RowOwner<'_>,
    document: &MemoryEmbeddingDocument,
    vector: &[f32],
    limits: MemoryStoreWriteLimits,
) -> Result<RecordBatch, MemoryStoreError> {
    if vector.len() != document.identity.dimensions.get() as usize
        || vector.iter().any(|value| !value.is_finite())
    {
        return Err(MemoryStoreError::Invalid("Memory embedding vector"));
    }
    let payload = serde_json::to_string(&EmbeddingPayload {
        memory_id: document.memory_id,
        memory_revision: document.revision,
    })?;
    let key = format!("embedding/{}/{}", document.memory_id, document.revision);
    let id = document.memory_id.to_string();
    let layout = match owner {
        rows::RowOwner::Project { .. } => rows::MemoryLayout::Project,
        rows::RowOwner::User(_) => rows::MemoryLayout::User,
    };
    let project = match owner {
        rows::RowOwner::Project { project_id, .. } => Some(project_id.to_string()),
        rows::RowOwner::User(_) => None,
    };
    let prefix = match layout {
        rows::MemoryLayout::Project => 18,
        rows::MemoryLayout::User => 19,
    };
    let texts = |col: usize| -> &str {
        match (owner, col) {
            (_, 0) => key.as_str(),
            (_, 1) => "embedding",
            (_, 2) => id.as_str(),
            (rows::RowOwner::Project { .. }, 3) => project.as_deref().expect("PROJECT owner text"),
            (rows::RowOwner::User(u), 3) => u.installation_id.as_str(),
            (rows::RowOwner::User(u), 4) => u.identity.issuer(),
            (rows::RowOwner::User(u), 5) => u.identity.subject(),
            (_, c) if c == column(layout, 16) => "[]",
            (_, c) if c == column(layout, 17) => payload.as_str(),
            _ => "",
        }
    };
    let mut bytes = 0;
    for col in 0..prefix {
        charge(
            &mut bytes,
            texts(col).len(),
            usize::MAX,
            "encoded row bytes",
        )?;
    }
    for text in [document.identity.recipe.as_str(), "Document"] {
        charge(&mut bytes, text.len(), usize::MAX, "encoded row bytes")?;
    }
    let mut row_bytes = 0;
    charge(
        &mut row_bytes,
        bytes,
        limits.max_row_bytes.get(),
        "encoded row bytes",
    )?;

    let mut columns: Vec<Arc<dyn Array>> = (0..prefix)
        .map(|col| -> Arc<dyn Array> {
            if [8, 9, 10, 15].map(|c| column(layout, c)).contains(&col) {
                Arc::new(UInt64Array::from(vec![if col == column(layout, 8) {
                    document.revision
                } else {
                    0
                }]))
            } else if [13, 14].map(|c| column(layout, c)).contains(&col) {
                Arc::new(BooleanArray::from(vec![false]))
            } else {
                Arc::new(StringArray::from(vec![texts(col)]))
            }
        })
        .collect();
    let mut values = ListBuilder::new(Float32Builder::new());
    values.values().append_slice(vector);
    values.append(true);
    columns.extend([
        Arc::new(FixedSizeBinaryArray::try_from_iter(
            [document.identity.vector_id.as_slice()].into_iter(),
        )?) as Arc<dyn Array>,
        Arc::new(FixedSizeBinaryArray::try_from_iter(
            [document.fingerprint.as_slice()].into_iter(),
        )?),
        Arc::new(StringArray::from(vec![document.identity.recipe.as_str()])),
        Arc::new(StringArray::from(vec!["Document"])),
        Arc::new(UInt32Array::from(vec![document.identity.dimensions.get()])),
        Arc::new(values.finish()),
    ]);
    Ok(RecordBatch::try_new(rows::schema_for(layout), columns)?)
}

/// Validate shape and canonical headers while borrowing vector storage.
pub(super) fn validate_row(batch: &RecordBatch, row: usize) -> Result<(), MemoryStoreError> {
    validate_row_for_layout(batch, row, rows::MemoryLayout::Project)
}
pub(crate) fn validate_row_for_layout(
    batch: &RecordBatch,
    row: usize,
    layout: rows::MemoryLayout,
) -> Result<(), MemoryStoreError> {
    if batch
        .columns()
        .iter()
        .skip(column(layout, 18))
        .any(|column| column.is_null(row))
    {
        return Err(MemoryStoreError::Invalid("Memory embedding null field"));
    }
    let id: MemoryId = rows::text(batch, 2, row)?.parse()?;
    let revision = rows::number(batch, column(layout, 8), row)?;
    let payload_text = rows::text(batch, column(layout, 17), row)?;
    let payload: EmbeddingPayload = serde_json::from_str(payload_text)?;
    for col in [4, 5, 6, 7, 11, 12]
        .into_iter()
        .filter(|c| !matches!(layout, rows::MemoryLayout::User) || *c != 4)
    {
        if !rows::text(batch, column(layout, col), row)?.is_empty() {
            return Err(MemoryStoreError::Invalid(
                "Memory embedding header disagrees",
            ));
        }
    }
    for col in [9, 10, 15] {
        if rows::number(batch, column(layout, col), row)? != 0 {
            return Err(MemoryStoreError::Invalid(
                "Memory embedding header disagrees",
            ));
        }
    }
    if revision == 0
        || rows::text(batch, 0, row)? != format!("embedding/{id}/{revision}")
        || payload.memory_id != id
        || payload.memory_revision != revision
        || serde_json::to_string(&payload)? != payload_text
        || rows::text(batch, column(layout, 16), row)? != "[]"
        || rows::flag(batch, column(layout, 13), row)?
        || rows::flag(batch, column(layout, 14), row)?
    {
        return Err(MemoryStoreError::Invalid(
            "Memory embedding header disagrees",
        ));
    }
    for col in [18, 19] {
        let binary = batch
            .column(column(layout, col))
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .ok_or(MemoryStoreError::Invalid("Memory embedding binary type"))?;
        if binary.value(row).len() != 32 {
            return Err(MemoryStoreError::Invalid("Memory embedding binary width"));
        }
    }
    if rows::text(batch, column(layout, 20), row)? != MEMORY_DOCUMENT_RECIPE
        || rows::text(batch, column(layout, 21), row)? != "Document"
    {
        return Err(MemoryStoreError::Invalid("Memory embedding identity"));
    }
    let dimensions = batch
        .column(column(layout, 22))
        .as_any()
        .downcast_ref::<UInt32Array>()
        .ok_or(MemoryStoreError::Invalid(
            "Memory embedding dimensions type",
        ))?
        .value(row) as usize;
    let list = batch
        .column(column(layout, 23))
        .as_any()
        .downcast_ref::<ListArray>()
        .ok_or(MemoryStoreError::Invalid("Memory embedding list type"))?;
    let values = list
        .values()
        .as_any()
        .downcast_ref::<Float32Array>()
        .ok_or(MemoryStoreError::Invalid("Memory embedding float type"))?;
    let offsets = list.value_offsets();
    let start = offsets[row] as usize;
    let end = offsets[row + 1] as usize;
    if dimensions == 0
        || dimensions != end - start
        || (start..end).any(|index| values.is_null(index) || !values.value(index).is_finite())
    {
        return Err(MemoryStoreError::Invalid("Memory embedding vector"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_attributes::{MemoryAttributes, MemoryClass};
    use graphit_core::{
        artifact_store::{ArtifactLimits, CopyLimits},
        file_io::SnapshotContext,
        file_lock::Cancellation,
        native_document_file::NativeDocumentFileLimits,
    };
    use graphit_data::{
        lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore,
    };
    use std::{
        num::NonZeroUsize,
        time::{Duration, Instant},
    };

    fn fingerprint_source(body: String) -> Memory {
        Memory {
            id: "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap(),
            revision: 7,
            title: "título β \"\\\n".into(),
            body,
            tags: vec!["memória".into(), "\"\\\t\n🦊".into()],
            references: vec![],
            attributes: MemoryAttributes {
                classification: MemoryClass::Decision,
                mandatory: false,
                important: false,
            },
            reference_generation: 7,
            created_by: "1".repeat(64),
            updated_by: "1".repeat(64),
            created_at: 1,
            updated_at: 2,
        }
    }

    #[test]
    fn memory_embedding_fingerprint_stream_matches_canonical_and_chunked_unicode() {
        use std::io::Write;
        let identity = MemoryEmbeddingIdentity::new(
            [7; 32],
            NonZeroU32::new(2).unwrap(),
            MEMORY_DOCUMENT_RECIPE.into(),
        )
        .unwrap();
        for body in [
            String::new(),
            " literal whitespace \n\tβ 🦊 / \u{2028} \" \\ ".into(),
            (0..32).map(char::from).collect(),
            "UTF8 β \"\\\n".repeat(100),
        ] {
            let mut memory = fingerprint_source(body);
            for tags in [vec![], memory.tags.clone()] {
                memory.tags = tags;
                let canonical = prepare_memory_embedding_document(&memory, &identity).unwrap();
                assert_eq!(
                    memory_embedding_fingerprint(&memory, &identity).unwrap(),
                    *canonical.fingerprint()
                );
                for width in [1, 2, 7, canonical.text().len()] {
                    let mut header = serde_json::to_vec(&(
                        &identity.recipe,
                        "Document",
                        &identity.vector_id,
                        identity.dimensions.get(),
                    ))
                    .unwrap();
                    header.truncate(header.len() - 1);
                    let mut digest = Sha256::new();
                    digest.update(header);
                    digest.update(b",\"");
                    for chunk in canonical.text().as_bytes().chunks(width) {
                        DocumentFingerprintWriter(&mut digest)
                            .write_all(chunk)
                            .unwrap();
                    }
                    digest.update(b"\"]");
                    let actual: [u8; 32] = digest.finalize().into();
                    assert_eq!(actual, *canonical.fingerprint());
                }
            }
        }
    }

    #[test]
    fn memory_embedding_fingerprint_stream_rejects_zero_revision() {
        let mut memory = fingerprint_source("valid source".into());
        memory.revision = 0;
        let identity = MemoryEmbeddingIdentity::new(
            [7; 32],
            NonZeroU32::new(2).unwrap(),
            MEMORY_DOCUMENT_RECIPE.into(),
        )
        .unwrap();
        assert!(matches!(
            memory_embedding_fingerprint(&memory, &identity),
            Err(MemoryStoreError::Invalid(
                "Memory embedding source revision"
            ))
        ));
    }

    #[test]
    fn memory_embedding_document_into_text_preserves_allocation_and_metadata() {
        let memory = Memory {
            id: "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap(),
            revision: 7,
            title: "título β".into(),
            body: "corpo inteiro\nsem prefixo adicional".into(),
            tags: vec!["memória".into(), "tag β".into()],
            references: vec![],
            attributes: MemoryAttributes {
                classification: MemoryClass::Decision,
                mandatory: false,
                important: false,
            },
            reference_generation: 7,
            created_by: "1".repeat(64),
            updated_by: "1".repeat(64),
            created_at: 1,
            updated_at: 2,
        };
        let identity = MemoryEmbeddingIdentity::new(
            [3; 32],
            NonZeroU32::new(2).unwrap(),
            MEMORY_DOCUMENT_RECIPE.into(),
        )
        .unwrap();
        let document = prepare_memory_embedding_document(&memory, &identity).unwrap();
        let metadata = (
            document.memory_id(),
            document.revision(),
            *document.fingerprint(),
            document.identity().clone(),
        );
        let pointer = document.text().as_ptr();
        let length = document.text().len();
        let capacity = document.text.capacity();
        let expected = document.text().to_owned();
        let text = document.into_text();
        assert_eq!(text.as_ptr(), pointer);
        assert_eq!(text.len(), length);
        assert_eq!(text.capacity(), capacity);
        assert_eq!(text, expected);
        assert!(text.contains("título β") && text.contains("memória"));
        assert_eq!(metadata.0, memory.id);
        assert_eq!(metadata.1, memory.revision);
        assert_eq!(metadata.3, identity);
        assert_eq!(
            metadata.2,
            *prepare_memory_embedding_document(&memory, &identity)
                .unwrap()
                .fingerprint()
        );
    }

    #[tokio::test]
    async fn memory_embedding_only_physical_row_reserves_id_and_charges_scan() {
        let temp = tempfile::tempdir().unwrap();
        let uri = url::Url::from_file_path(temp.path().join("index.lance"))
            .unwrap()
            .to_string();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(120),
            Cancellation::default(),
        ));
        let project_id: ProjectId = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
        let id: MemoryId = "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap();
        let memory = Memory {
            id,
            revision: 1,
            title: "title".into(),
            body: "body".into(),
            tags: vec![],
            references: vec![],
            attributes: MemoryAttributes {
                classification: MemoryClass::Decision,
                mandatory: false,
                important: false,
            },
            reference_generation: 1,
            created_by: "1".repeat(64),
            updated_by: "1".repeat(64),
            created_at: 1,
            updated_at: 1,
        };
        let read = super::super::MemoryStoreReadLimits {
            max_query_bytes: NonZeroUsize::new(65536).unwrap(),
            batch_rows: NonZeroU32::new(1).unwrap(),
            native: NativeDocumentFileLimits::new(1048576, 65536, 256, 8388608).unwrap(),
        };
        let write = MemoryStoreWriteLimits {
            read,
            max_input_bytes: NonZeroUsize::new(65536).unwrap(),
            max_row_bytes: NonZeroUsize::new(65536).unwrap(),
            artifacts: ArtifactLimits::new(
                1048576,
                65536,
                CopyLimits::new(1048576, 128, 65536, 16).unwrap(),
            )
            .unwrap(),
        };
        let identity = MemoryEmbeddingIdentity::new(
            [0; 32],
            NonZeroU32::new(2).unwrap(),
            MEMORY_DOCUMENT_RECIPE.into(),
        )
        .unwrap();
        let document = prepare_memory_embedding_document(&memory, &identity).unwrap();
        let batch = encode_memory_embedding_row(project_id, &document, &[1., 0.], write).unwrap();
        LanceModuleStore::create(&uri, rows::schema(), vec![batch])
            .await
            .unwrap();
        let store = LanceBranchStore::open(&uri, rows::schema(), None)
            .await
            .unwrap();
        let pin = store.capture_target(None).await.unwrap();
        let before = || original.check().map_err(MemoryStoreError::from);
        let logical =
            super::super::read::exact_rows(&pin, "true", &original, project_id, read, before)
                .await
                .unwrap();
        assert!(logical.is_empty());
        assert!(matches!(
            super::super::read::ensure_id_available(&pin, id, &original, project_id, read, before)
                .await,
            Err(MemoryStoreError::Invalid("generated Memory ID collision"))
        ));
        super::super::read::ensure_id_available(
            &pin,
            "01KSH1CRFFG8Z74B5ZS78WW80A".parse().unwrap(),
            &original,
            project_id,
            read,
            before,
        )
        .await
        .unwrap();
        let mut tiny_write = write;
        tiny_write.max_row_bytes = NonZeroUsize::MIN;
        assert!(matches!(
            encode_memory_embedding_row(project_id, &document, &[1., 0.], tiny_write),
            Err(MemoryStoreError::Limit("encoded row bytes"))
        ));
        let mut query_only = read;
        query_only.max_query_bytes = NonZeroUsize::MIN;
        assert!(
            super::super::read::exact_rows(&pin, "true", &original, project_id, query_only, before)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            super::super::read::exact_rows(
                &pin,
                "true",
                &original,
                "01KSH1CRFFG8Z74B5ZS78WW80B".parse().unwrap(),
                read,
                before
            )
            .await
            .is_err()
        );
        assert_eq!(
            store.capture_target(None).await.unwrap().version(),
            pin.version()
        );
    }
}
