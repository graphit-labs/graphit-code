//! Explicit textual export profiles. These values describe output, never authority.
use crate::markdown_export::MarkdownExportError;
use graphit_core::{brand::Brand, file_io::SnapshotContext};
use graphit_data::knowledge_index::KnowledgeDocument;
use serde::Serialize;
use serde_json::{Map, Value};

/// The owner supplies its captured installation brand for generated provenance.
#[derive(Clone, Debug)]
pub enum KnowledgeTextExportFormat {
    Markdown,
    Okf { brand: Brand },
    Obsidian { brand: Brand },
}
impl KnowledgeTextExportFormat {
    pub(crate) fn is_okf(&self) -> bool {
        !matches!(self, Self::Markdown)
    }
    pub(crate) fn validate_document(
        &self,
        document: &KnowledgeDocument,
    ) -> Result<(), MarkdownExportError> {
        if self.is_okf() {
            if document.kind.trim().is_empty() {
                return Err(MarkdownExportError::InvalidOkfMetadata("type"));
            }
            if document.kind == "Attested Computation" {
                return Err(MarkdownExportError::InvalidOkfMetadata(
                    "Attested Computation requires runtime and execution metadata",
                ));
            }
        }
        Ok(())
    }
}

pub(crate) fn metadata(
    original: &SnapshotContext,
    brand: &Brand,
    document: &KnowledgeDocument,
    native: &impl Serialize,
) -> Result<Value, MarkdownExportError> {
    original.check().map_err(MarkdownExportError::Request)?;
    // Reuse the native codec, including its temporal precision and None semantics.
    // This transient value contains one page's metadata and never its body.
    let native = serde_json::to_value(native).map_err(MarkdownExportError::Metadata)?;
    let mut generated = Map::new();
    generated.insert(
        "by".into(),
        Value::String(format!("process:{}-knowledge-wiki", brand.key())),
    );
    match native.get("updated_at") {
        Some(value @ Value::String(_)) => {
            generated.insert("at".into(), value.clone());
        }
        Some(Value::Null) => {}
        _ => return Err(MarkdownExportError::InvalidOkfMetadata("updated_at")),
    }
    let mut header = Map::new();
    header.insert("type".into(), Value::String(document.kind.clone()));
    header.insert("id".into(), Value::String(document.slug.clone()));
    header.insert("title".into(), Value::String(document.title.clone()));
    if let Some(description) = &document.description {
        header.insert("description".into(), Value::String(description.clone()));
    }
    header.insert("generated".into(), Value::Object(generated));
    if !document.source_path.is_empty() {
        let source = Map::from_iter([(
            "resource".into(),
            Value::String(document.source_path.clone()),
        )]);
        header.insert("sources".into(), Value::Array(vec![Value::Object(source)]));
    }
    // Native stale_after_ns is a duration; OKF stale_after would be an absolute instant.
    header.insert("graphit".into(), native);
    original.check().map_err(MarkdownExportError::Request)?;
    Ok(Value::Object(header))
}
