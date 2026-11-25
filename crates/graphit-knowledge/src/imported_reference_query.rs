//! Reference pages from an admitted imported Knowledge pin.
//! Cursor bindings observe the selected artifact; they confer no authority.
use crate::{
    imported_knowledge::{ImportedKnowledgeInput, ImportedKnowledgeReadError},
    native_reference_query::{KnowledgeLogicalTarget, target},
};
use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_reference_query::{
    KnowledgeReferenceEdge, KnowledgeReferencePage, KnowledgeReferenceQuery,
    KnowledgeReferenceQueryError, KnowledgeReferenceSnapshotRead, KnowledgeReferenceSource,
    KnowledgeReferencesCoverage, KnowledgeReferencesCursor, KnowledgeReferencesWarning,
};
use graphit_hub::registry::Installation;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{error::Error, fmt, io, io::Write, num::NonZeroUsize};

#[derive(Clone, Copy)]
pub struct ImportedKnowledgeReferencesQuery<'a> {
    pub outgoing: Option<&'a str>,
    pub incoming: Option<KnowledgeLogicalTarget<'a>>,
    pub page_size: NonZeroUsize,
    pub outgoing_cursor: Option<&'a ImportedKnowledgeReferencesCursor>,
    pub incoming_cursor: Option<&'a ImportedKnowledgeReferencesCursor>,
}
/// Ordinary observation data, validated with the current reader on every call.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedKnowledgeReferencesCursor {
    binding: [u8; 32],
    cursor: KnowledgeReferencesCursor,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ImportedKnowledgeReferencePage {
    pub results: Vec<KnowledgeReferenceEdge>,
    pub total: usize,
    pub complete: bool,
    pub next_cursor: Option<ImportedKnowledgeReferencesCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ImportedKnowledgeReferencesRead {
    /// Validated outgoing focus from the retained imported pin, never its mount alias.
    pub focused_source: Option<KnowledgeReferenceSource>,
    pub outgoing: Option<Box<ImportedKnowledgeReferencePage>>,
    pub incoming: Option<Box<ImportedKnowledgeReferencePage>>,
    pub coverage: KnowledgeReferencesCoverage,
    pub warnings: Vec<KnowledgeReferencesWarning>,
}
#[derive(Debug)]
pub enum ImportedKnowledgeReferenceError {
    Reader(ImportedKnowledgeReadError),
    Query(KnowledgeReferenceQueryError),
    InvalidCursor,
    Request(io::Error),
    BindingSerialization(serde_json::Error),
}
impl fmt::Display for ImportedKnowledgeReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reader(e) => write!(f, "imported Knowledge references: {e}"),
            Self::Query(e) => write!(f, "imported Knowledge references: {e}"),
            Self::InvalidCursor => f.write_str("imported Knowledge selection no longer matches"),
            Self::Request(_) => f.write_str("imported Knowledge reference request failed"),
            Self::BindingSerialization(_) => {
                f.write_str("imported Knowledge selection encoding failed")
            }
        }
    }
}
impl Error for ImportedKnowledgeReferenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Reader(e) => Some(e),
            Self::Query(e) => Some(e),
            Self::Request(e) => Some(e),
            Self::BindingSerialization(e) => Some(e),
            Self::InvalidCursor => None,
        }
    }
}
impl<'a> ImportedKnowledgeReferencesQuery<'a> {
    pub(crate) fn data_query(
        self,
        binding: &[u8; 32],
    ) -> Result<KnowledgeReferenceQuery<'a>, ImportedKnowledgeReferenceError> {
        for cursor in [self.outgoing_cursor, self.incoming_cursor]
            .into_iter()
            .flatten()
        {
            if &cursor.binding != binding {
                return Err(ImportedKnowledgeReferenceError::InvalidCursor);
            }
        }
        Ok(KnowledgeReferenceQuery {
            outgoing: self.outgoing,
            incoming: self.incoming.map(target),
            page_size: self.page_size,
            outgoing_cursor: self.outgoing_cursor.map(|c| &c.cursor),
            incoming_cursor: self.incoming_cursor.map(|c| &c.cursor),
        })
    }
}
struct BindingWriter<'a> {
    original: &'a SnapshotContext,
    hash: Sha256,
    first_failure: Option<io::Error>,
}
impl Write for BindingWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(error) = &self.first_failure {
            return Err(io::Error::from(error.kind()));
        }
        if let Err(error) = self.original.check() {
            let kind = error.kind();
            self.first_failure = Some(error);
            return Err(io::Error::from(kind));
        }
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.write(&[]).map(|_| ())
    }
}
pub(crate) fn selection_binding(
    original: &SnapshotContext,
    installation: &Installation,
    input: ImportedKnowledgeInput<'_>,
) -> Result<[u8; 32], ImportedKnowledgeReferenceError> {
    original
        .check()
        .map_err(ImportedKnowledgeReferenceError::Request)?;
    let mut writer = BindingWriter {
        original,
        hash: Sha256::new(),
        first_failure: None,
    };
    writer
        .hash
        .update(b"graphit.knowledge.import-reference-cursor\0");
    let encoded = serde_json::to_writer(
        &mut writer,
        &(
            1u8,
            input.registry.scope(),
            installation,
            input.id.as_str(),
            input.version.as_str(),
            input.context_alias,
        ),
    );
    if let Some(error) = writer.first_failure {
        return Err(ImportedKnowledgeReferenceError::Request(error));
    }
    encoded.map_err(ImportedKnowledgeReferenceError::BindingSerialization)?;
    original
        .check()
        .map_err(ImportedKnowledgeReferenceError::Request)?;
    Ok(writer.hash.finalize().into())
}
pub(crate) fn wrap_read(
    read: KnowledgeReferenceSnapshotRead,
    binding: [u8; 32],
) -> ImportedKnowledgeReferencesRead {
    let page = |page: Box<KnowledgeReferencePage>| {
        let KnowledgeReferencePage {
            results,
            total,
            complete,
            next_cursor,
        } = *page;
        Box::new(ImportedKnowledgeReferencePage {
            results,
            total,
            complete,
            next_cursor: next_cursor
                .map(|cursor| ImportedKnowledgeReferencesCursor { binding, cursor }),
        })
    };
    ImportedKnowledgeReferencesRead {
        focused_source: read.focused_source,
        outgoing: read.outgoing.map(page),
        incoming: read.incoming.map(page),
        coverage: read.coverage,
        warnings: read.warnings,
    }
}
