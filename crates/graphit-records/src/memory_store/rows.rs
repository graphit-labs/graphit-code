use super::{
    model::{StoredCreation, StoredRevision},
    *,
};
use arrow_array::{Array, BooleanArray, RecordBatch, StringArray, UInt64Array, new_null_array};
use arrow_schema::{DataType, Field, Schema};
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use serde::Serialize;
use std::{collections::HashMap, io, sync::Arc};

/// Exact physical layouts; neither variant carries admission or owner authority.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum MemoryLayout {
    Project,
    User,
}
pub(super) fn schema() -> Arc<Schema> {
    schema_for(MemoryLayout::Project)
}
pub(crate) fn schema_for(layout: MemoryLayout) -> Arc<Schema> {
    let mut fields = vec![
        field("row_key", DataType::Utf8),
        field("kind", DataType::Utf8),
        field("record_id", DataType::Utf8),
    ];
    let label = match layout {
        MemoryLayout::Project => {
            fields.extend([
                field("project_id", DataType::Utf8),
                field("operation_version", DataType::Utf8),
            ]);
            "project-v2-embedding-list"
        }
        MemoryLayout::User => {
            fields.extend([
                field("installation_id", DataType::Utf8),
                field("user_issuer", DataType::Utf8),
                field("user_subject", DataType::Utf8),
            ]);
            "user-v1-embedding-list"
        }
    };
    fields.extend([
        field("idempotency_key", DataType::Utf8),
        field("idempotency_actor", DataType::Utf8),
        field("title", DataType::Utf8),
        field("revision", DataType::UInt64),
        field("created_at", DataType::UInt64),
        field("updated_at", DataType::UInt64),
        field("created_by", DataType::Utf8),
        field("updated_by", DataType::Utf8),
        field("superseded", DataType::Boolean),
        field("tombstone", DataType::Boolean),
        field("reference_generation", DataType::UInt64),
        field("references", DataType::Utf8),
        field("payload", DataType::Utf8),
        Field::new("embedding_vector_id", DataType::FixedSizeBinary(32), true),
        Field::new("embedding_fingerprint", DataType::FixedSizeBinary(32), true),
        Field::new("embedding_recipe", DataType::Utf8, true),
        Field::new("embedding_role", DataType::Utf8, true),
        Field::new("embedding_dimensions", DataType::UInt32, true),
        Field::new(
            "embedding_vector",
            DataType::List(Arc::new(Field::new("item", DataType::Float32, true))),
            true,
        ),
    ]);
    Arc::new(Schema::new_with_metadata(fields, layout_metadata(label)))
}
fn field(name: &str, ty: DataType) -> Field {
    Field::new(name, ty, false)
}
fn metadata() -> HashMap<String, String> {
    layout_metadata("project-v2-embedding-list")
}
fn layout_metadata(label: &str) -> HashMap<String, String> {
    HashMap::from([("graphit.memory.layout".into(), label.into())])
}
pub(super) fn head_schema() -> Schema {
    Schema::new_with_metadata(
        vec![
            field("record_id", DataType::Utf8),
            field("title", DataType::Utf8),
            field("updated_by", DataType::Utf8),
            field("updated_at", DataType::UInt64),
        ],
        metadata(),
    )
}
pub(crate) fn validate_schema(
    batch: &RecordBatch,
    expected: &Schema,
) -> Result<(), MemoryStoreError> {
    if batch.schema().as_ref() != expected {
        return Err(MemoryStoreError::Invalid("Memory batch schema"));
    }
    if batch
        .columns()
        .iter()
        .zip(expected.fields())
        .any(|(a, field)| !field.is_nullable() && a.null_count() != 0)
    {
        return Err(MemoryStoreError::Invalid("Memory null field"));
    }
    Ok(())
}
pub(crate) fn text(batch: &RecordBatch, col: usize, row: usize) -> Result<&str, MemoryStoreError> {
    batch
        .column(col)
        .as_any()
        .downcast_ref::<StringArray>()
        .map(|a| a.value(row))
        .ok_or(MemoryStoreError::Invalid("Memory text type"))
}
pub(crate) fn number(batch: &RecordBatch, col: usize, row: usize) -> Result<u64, MemoryStoreError> {
    batch
        .column(col)
        .as_any()
        .downcast_ref::<UInt64Array>()
        .map(|a| a.value(row))
        .ok_or(MemoryStoreError::Invalid("Memory number type"))
}
pub(crate) fn flag(batch: &RecordBatch, col: usize, row: usize) -> Result<bool, MemoryStoreError> {
    batch
        .column(col)
        .as_any()
        .downcast_ref::<BooleanArray>()
        .map(|a| a.value(row))
        .ok_or(MemoryStoreError::Invalid("Memory flag type"))
}
pub(super) struct Row {
    pub kind: String,
    pub project_id: ProjectId,
    pub operation_version: String,
    pub key: String,
    pub actor: String,
    pub memory: Memory,
    pub creation: Option<CreationMemory>,
    pub revision: Option<MemoryRevision>,
    pub tombstone: bool,
}
pub(super) fn valid_memory(
    original: &SnapshotContext,
    project_id: ProjectId,
    memory: &Memory,
) -> Result<(), MemoryStoreError> {
    valid_memory_in_scope(
        original,
        crate::reference_projection::LogicalScope::Project(project_id),
        memory,
    )
}
pub(crate) fn valid_memory_in_scope(
    original: &SnapshotContext,
    scope: crate::reference_projection::LogicalScope<'_>,
    memory: &Memory,
) -> Result<(), MemoryStoreError> {
    original.check()?;
    valid_actor(&memory.created_by)?;
    valid_actor(&memory.updated_by)?;
    nonblank(&memory.title)?;
    nonblank(&memory.body)?;
    for tag in &memory.tags {
        nonblank(tag)?;
    }
    super::write::validate_references_in_scope(original, scope, &memory.references)?;
    if memory.revision == 0
        || memory.reference_generation == 0
        || memory.reference_generation > memory.revision
        || memory.updated_at < memory.created_at
    {
        return Err(MemoryStoreError::Invalid("Memory snapshot metadata"));
    }
    if memory.revision == 1
        && (memory.reference_generation != 1
            || memory.created_by != memory.updated_by
            || memory.created_at != memory.updated_at)
    {
        return Err(MemoryStoreError::Invalid("initial Memory header disagrees"));
    }
    original.check()?;
    Ok(())
}
pub(super) fn decode(
    batch: &RecordBatch,
    row: usize,
    original: &SnapshotContext,
    expected_project: ProjectId,
) -> Result<Option<Row>, MemoryStoreError> {
    original.check()?;
    let kind = text(batch, 1, row)?;
    let project_id: ProjectId = text(batch, 3, row)?.parse()?;
    if project_id != expected_project {
        return Err(MemoryStoreError::Invalid("foreign Memory project"));
    }
    if kind == "embedding" {
        super::embeddings::validate_row(batch, row)?;
        original.check()?;
        return Ok(None);
    }
    if batch
        .columns()
        .iter()
        .skip(18)
        .any(|column| !column.is_null(row))
    {
        return Err(MemoryStoreError::Invalid("logical Memory embedding fields"));
    }
    if kind == "references" {
        super::references::decode(batch, row, original, expected_project)?;
        return Ok(None);
    }
    let (memory, creation, revision) = match kind {
        "head" => (
            serde_json::from_str::<Memory>(text(batch, 17, row)?)?,
            None,
            None,
        ),
        "creation" => {
            let event: CreationMemory =
                serde_json::from_str::<StoredCreation>(text(batch, 17, row)?)?.into();
            let refs = super::write::validate_input(original, event.project_id, &event.input)?;
            if event.memory.title != event.input.title
                || event.memory.body != event.input.body
                || event.memory.attributes != event.input.attributes
                || event.memory.tags != event.input.tags
                || event.memory.references != refs
                || event.memory.created_by != event.actor
                || event.memory.revision != 1
            {
                return Err(MemoryStoreError::Invalid("creation input disagrees"));
            }
            (event.memory.clone(), Some(event), None)
        }
        "revision" => {
            let event: MemoryRevision =
                serde_json::from_str::<StoredRevision>(text(batch, 17, row)?)?.into();
            super::revision::validate_event(original, &event)?;
            (event.after.clone(), None, Some(event))
        }
        _ => return Err(MemoryStoreError::Invalid("unknown Memory row kind")),
    };
    let actor = text(batch, 6, row)?;
    valid_actor(actor)?;
    valid_memory(original, project_id, &memory)?;
    let row_key = if kind == "revision" {
        format!("revision/{}/{}", memory.id, memory.revision)
    } else {
        format!("{kind}/{}", memory.id)
    };
    let tombstone = flag(batch, 14, row)?;
    if text(batch, 0, row)? != row_key
        || text(batch, 2, row)? != memory.id.to_string()
        || text(batch, 7, row)? != memory.title
        || number(batch, 8, row)? != memory.revision
        || number(batch, 9, row)? != memory.created_at
        || number(batch, 10, row)? != memory.updated_at
        || text(batch, 11, row)? != memory.created_by
        || text(batch, 12, row)? != memory.updated_by
        || flag(batch, 13, row)? != (kind != "head")
        || number(batch, 15, row)? != memory.reference_generation
        || text(batch, 16, row)? != serde_json::to_string(&memory.references)?
        || (kind != "revision" && actor != memory.created_by)
        || (kind == "creation" && tombstone)
    {
        return Err(MemoryStoreError::Invalid("Memory header disagrees"));
    }
    nonblank(text(batch, 5, row)?)?;
    if let Some(event) = &creation
        && (event.project_id != project_id
            || event.operation_version != text(batch, 4, row)?
            || event.input.idempotency_key != text(batch, 5, row)?
            || event.actor != actor)
    {
        return Err(MemoryStoreError::Invalid("creation binding disagrees"));
    }
    if let Some(event) = &revision
        && (event.project_id != project_id
            || event.operation_version != text(batch, 4, row)?
            || event.operation.key() != text(batch, 5, row)?
            || event.actor != actor
            || actor != memory.updated_by
            || event.operation.deleted() != tombstone)
    {
        return Err(MemoryStoreError::Invalid("revision binding disagrees"));
    }
    original.check()?;
    Ok(Some(Row {
        kind: kind.to_owned(),
        project_id,
        operation_version: text(batch, 4, row)?.to_owned(),
        key: text(batch, 5, row)?.to_owned(),
        actor: actor.to_owned(),
        memory,
        creation,
        revision,
        tombstone,
    }))
}
#[derive(Clone, Copy)]
pub(crate) enum RowOwner<'a> {
    Project {
        project_id: ProjectId,
        version: &'a str,
    },
    User(&'a crate::user_memory_store::UserMemoryOwner),
}
impl RowOwner<'_> {
    fn layout(self) -> MemoryLayout {
        match self {
            Self::Project { .. } => MemoryLayout::Project,
            Self::User(_) => MemoryLayout::User,
        }
    }
    fn parts(self) -> Vec<String> {
        match self {
            Self::Project {
                project_id,
                version,
            } => vec![project_id.to_string(), version.into()],
            Self::User(o) => vec![
                o.installation_id.as_str().into(),
                o.identity.issuer().into(),
                o.identity.subject().into(),
            ],
        }
    }
}
pub(crate) struct EncodedRow<'a> {
    pub kind: &'static str,
    pub owner: RowOwner<'a>,
    pub key: &'a str,
    pub actor: &'a str,
    pub memory: &'a Memory,
    pub payload: String,
    pub tombstone: bool,
}
pub(super) fn batch(
    event: &CreationMemory,
    limits: MemoryStoreWriteLimits,
) -> Result<RecordBatch, MemoryStoreError> {
    encode(
        [
            EncodedRow {
                kind: "head",
                owner: RowOwner::Project {
                    project_id: event.project_id,
                    version: &event.operation_version,
                },
                key: &event.input.idempotency_key,
                actor: &event.actor,
                memory: &event.memory,
                payload: serde_json::to_string(&event.memory)?,
                tombstone: false,
            },
            EncodedRow {
                kind: "creation",
                owner: RowOwner::Project {
                    project_id: event.project_id,
                    version: &event.operation_version,
                },
                key: &event.input.idempotency_key,
                actor: &event.actor,
                memory: &event.memory,
                payload: serde_json::to_string(&StoredCreation::from(event.clone()))?,
                tombstone: false,
            },
            EncodedRow {
                kind: "references",
                owner: RowOwner::Project {
                    project_id: event.project_id,
                    version: &event.operation_version,
                },
                key: "",
                actor: "",
                memory: &event.memory,
                payload: super::references::payload(
                    event.project_id,
                    &event.operation_version,
                    &event.memory,
                    false,
                    limits,
                )?,
                tombstone: false,
            },
        ],
        limits,
    )
}
pub(super) fn mutation_batch(
    creation: &CreationMemory,
    event: &MemoryRevision,
    limits: MemoryStoreWriteLimits,
) -> Result<RecordBatch, MemoryStoreError> {
    encode(
        [
            EncodedRow {
                kind: "head",
                owner: RowOwner::Project {
                    project_id: creation.project_id,
                    version: &creation.operation_version,
                },
                key: &creation.input.idempotency_key,
                actor: &creation.actor,
                memory: &event.after,
                payload: serde_json::to_string(&event.after)?,
                tombstone: event.operation.deleted(),
            },
            EncodedRow {
                kind: "revision",
                owner: RowOwner::Project {
                    project_id: event.project_id,
                    version: &event.operation_version,
                },
                key: event.operation.key(),
                actor: &event.actor,
                memory: &event.after,
                payload: serde_json::to_string(&StoredRevision::from(event.clone()))?,
                tombstone: event.operation.deleted(),
            },
            EncodedRow {
                kind: "references",
                owner: RowOwner::Project {
                    project_id: creation.project_id,
                    version: &creation.operation_version,
                },
                key: "",
                actor: "",
                memory: &event.after,
                payload: super::references::payload(
                    creation.project_id,
                    &creation.operation_version,
                    &event.after,
                    event.operation.deleted(),
                    limits,
                )?,
                tombstone: event.operation.deleted(),
            },
        ],
        limits,
    )
}
pub(crate) fn encode(
    rows: [EncodedRow<'_>; 3],
    limits: MemoryStoreWriteLimits,
) -> Result<RecordBatch, MemoryStoreError> {
    let keys: Vec<_> = rows
        .iter()
        .map(|r| {
            if r.kind == "revision" || r.kind == "references" {
                format!("{}/{}/{}", r.kind, r.memory.id, r.memory.revision)
            } else {
                format!("{}/{}", r.kind, r.memory.id)
            }
        })
        .collect();
    let ids: Vec<_> = rows.iter().map(|r| r.memory.id.to_string()).collect();
    let layout = rows[0].owner.layout();
    if rows.iter().any(|r| r.owner.layout() != layout) {
        return Err(MemoryStoreError::Invalid("mixed Memory row layouts"));
    }
    let offset = usize::from(layout == MemoryLayout::User);
    let owners: Vec<_> = rows.iter().map(|r| r.owner.parts()).collect();
    let refs: Vec<_> = rows
        .iter()
        .map(|r| bounded_json(&r.memory.references, limits.max_row_bytes.get()))
        .collect::<Result<_, _>>()?;
    let mut text_columns: Vec<Vec<String>> = (0..18 + offset).map(|_| Vec::new()).collect();
    for (i, r) in rows.iter().enumerate() {
        let mut values = vec![
            (0, keys[i].as_str()),
            (1, r.kind),
            (2, ids[i].as_str()),
            (5, r.key),
            (6, r.actor),
            (
                7,
                if r.kind == "references" {
                    ""
                } else {
                    r.memory.title.as_str()
                },
            ),
            (
                11,
                if r.kind == "references" {
                    ""
                } else {
                    r.memory.created_by.as_str()
                },
            ),
            (
                12,
                if r.kind == "references" {
                    ""
                } else {
                    r.memory.updated_by.as_str()
                },
            ),
            (16, refs[i].as_str()),
            (17, r.payload.as_str()),
        ];
        for (c, _) in &mut values {
            if *c >= 5 {
                *c += offset;
            }
        }
        values.extend(
            owners[i]
                .iter()
                .enumerate()
                .map(|(c, v)| (3 + c, v.as_str())),
        );
        let mut bytes = 0;
        for (_, v) in &values {
            charge(
                &mut bytes,
                v.len(),
                limits.max_row_bytes.get(),
                "encoded row bytes",
            )?;
        }

        for (c, v) in values {
            text_columns[c].push(v.to_owned());
        }
    }
    let mut columns: Vec<Arc<dyn Array>> = Vec::new();
    for (c, values) in text_columns.into_iter().enumerate() {
        let a: Arc<dyn Array> = match c.checked_sub(offset).filter(|_| c >= 5) {
            Some(8) => Arc::new(UInt64Array::from(
                rows.iter().map(|r| r.memory.revision).collect::<Vec<_>>(),
            )),
            Some(9) => Arc::new(UInt64Array::from(
                rows.iter()
                    .map(|r| {
                        if r.kind == "references" {
                            0
                        } else {
                            r.memory.created_at
                        }
                    })
                    .collect::<Vec<_>>(),
            )),
            Some(10) => Arc::new(UInt64Array::from(
                rows.iter()
                    .map(|r| {
                        if r.kind == "references" {
                            0
                        } else {
                            r.memory.updated_at
                        }
                    })
                    .collect::<Vec<_>>(),
            )),
            Some(13) => Arc::new(BooleanArray::from(
                rows.iter().map(|r| r.kind != "head").collect::<Vec<_>>(),
            )),
            Some(14) => Arc::new(BooleanArray::from(
                rows.iter().map(|r| r.tombstone).collect::<Vec<_>>(),
            )),
            Some(15) => Arc::new(UInt64Array::from(
                rows.iter()
                    .map(|r| r.memory.reference_generation)
                    .collect::<Vec<_>>(),
            )),
            _ => Arc::new(StringArray::from(values)),
        };
        columns.push(a);
    }
    let schema = schema_for(layout);
    columns.extend(
        schema
            .fields()
            .iter()
            .skip(18 + offset)
            .map(|field| new_null_array(field.data_type(), rows.len())),
    );
    Ok(RecordBatch::try_new(schema, columns)?)
}
pub(super) fn head(
    batch: &RecordBatch,
    row: usize,
) -> Result<MemoryActivityHead, MemoryStoreError> {
    let id = text(batch, 0, row)?.parse()?;
    let title = text(batch, 1, row)?;
    let actor = text(batch, 2, row)?;
    nonblank(title)?;
    valid_actor(actor)?;
    Ok(MemoryActivityHead {
        id,
        title: title.to_owned(),
        updated_by: actor.to_owned(),
        updated_at: number(batch, 3, row)?,
    })
}

pub(crate) fn bounded_json(
    value: &impl Serialize,
    maximum: usize,
) -> Result<String, MemoryStoreError> {
    struct Buffer {
        bytes: Vec<u8>,
        maximum: usize,
        exceeded: bool,
    }
    impl io::Write for Buffer {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            if self
                .bytes
                .len()
                .checked_add(data.len())
                .is_none_or(|n| n > self.maximum)
            {
                self.exceeded = true;
                return Err(io::ErrorKind::InvalidInput.into());
            }
            self.bytes
                .try_reserve(data.len())
                .map_err(io::Error::other)?;
            self.bytes.extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer {
        bytes: vec![],
        maximum,
        exceeded: false,
    };
    if let Err(error) = serde_json::to_writer(&mut buffer, value) {
        return if buffer.exceeded {
            Err(MemoryStoreError::Limit("Memory encoded payload bytes"))
        } else {
            Err(error.into())
        };
    }
    String::from_utf8(buffer.bytes).map_err(|_| MemoryStoreError::Invalid("Memory JSON UTF-8"))
}
