//! Bounded render provenance. These records describe data, never filesystem authority.
use crate::{ArtifactType, SourceRef, projection::validate_name};
use graphit_core::artifact_store::{
    CopyLimits, PayloadSelection, SnapshotBindingRecord, ValidatedPayloadPlan,
};
use std::io;

/// Explicit caller policy, independent of general JSON string limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineageLimits {
    schema_bytes: usize,
    selection_bytes: usize,
    total_bytes: usize,
}

impl LineageLimits {
    pub fn new(schema: usize, selection: usize, total: usize) -> io::Result<Self> {
        if [schema, selection, total].contains(&0)
            || [schema, selection, total]
                .into_iter()
                .any(|n| n > 16 * 1024 * 1024)
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            schema_bytes: schema,
            selection_bytes: selection,
            total_bytes: total,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MaterializationLimits {
    pub output: CopyLimits,
    pub snapshots: CopyLimits,
    pub lineage: LineageLimits,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderLineage {
    host_schema: String,
    selected_name: String,
    input_fingerprint: String,
}
impl RenderLineage {
    pub fn new(
        schema: &str,
        selected_name: &str,
        input: [u8; 32],
        limits: LineageLimits,
    ) -> io::Result<Self> {
        // Admit all UTF-8 strings before allocating the owned record.
        validate_text(schema, limits.schema_bytes)?;
        validate_text(selected_name, limits.selection_bytes)?;
        validate_name(selected_name).map_err(|_| invalid())?;
        check_total([schema.len(), selected_name.len(), 64], limits)?;
        Ok(Self {
            host_schema: schema.into(),
            selected_name: selected_name.into(),
            input_fingerprint: hex(input),
        })
    }
    pub fn host_schema(&self) -> &str {
        &self.host_schema
    }
    pub fn selected_name(&self) -> &str {
        &self.selected_name
    }
    pub fn input_fingerprint(&self) -> &str {
        &self.input_fingerprint
    }
    pub(crate) fn validate(&self, limits: LineageLimits) -> io::Result<()> {
        validate_text(&self.host_schema, limits.schema_bytes)?;
        validate_text(&self.selected_name, limits.selection_bytes)?;
        validate_name(&self.selected_name).map_err(|_| invalid())?;
        if self.input_fingerprint.len() != 64
            || !self
                .input_fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        check_total(
            [self.host_schema.len(), self.selected_name.len(), 64],
            limits,
        )
    }
}
fn invalid() -> io::Error {
    io::ErrorKind::InvalidData.into()
}
fn validate_text(value: &str, cap: usize) -> io::Result<()> {
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        return Err(invalid());
    }
    if value.len() > cap {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(())
}
fn check_total(lengths: [usize; 3], limits: LineageLimits) -> io::Result<()> {
    lengths.into_iter().try_fold(0usize, |sum, n| {
        sum.checked_add(n)
            .filter(|n| *n <= limits.total_bytes)
            .ok_or(io::ErrorKind::FileTooLarge)
    })?;
    Ok(())
}
fn hex(bytes: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(64);
    for byte in bytes {
        result.push(HEX[usize::from(byte >> 4)] as char);
        result.push(HEX[usize::from(byte & 15)] as char);
    }
    result
}

/// A caller's real origin-backed render plan. No persistent generated copy exists.
pub struct DerivedArtifact {
    pub(crate) original_source: SourceRef,
    pub(crate) original_binding: SnapshotBindingRecord,
    pub(crate) original_selection: PayloadSelection,
    pub(crate) plan: ValidatedPayloadPlan,
    pub(crate) lineage: RenderLineage,
    pub(crate) selection: PayloadSelection,
}
impl DerivedArtifact {
    /// The selection pair is (original input, rendered output).
    pub fn new(
        original_source: SourceRef,
        plan: ValidatedPayloadPlan,
        schema: &str,
        selected_name: &str,
        (original_selection, selection): (PayloadSelection, PayloadSelection),
        limits: LineageLimits,
    ) -> io::Result<Self> {
        let snapshot = plan.source_snapshot().ok_or(io::ErrorKind::InvalidInput)?;
        let original_binding = snapshot.binding()?;
        validate_original_selection(&original_source, &original_binding, &original_selection)?;
        if original_binding.source() != original_source.directory().as_path()
            || !plan.nodes().any(|node| {
                node.relative() == selection.relative() && node.kind() == selection.kind()
            })
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let lineage = RenderLineage::new(schema, selected_name, snapshot.fingerprint(), limits)?;
        Ok(Self {
            original_source,
            original_binding,
            original_selection,
            plan,
            lineage,
            selection,
        })
    }
    pub fn original_source(&self) -> &SourceRef {
        &self.original_source
    }
    pub fn lineage(&self) -> &RenderLineage {
        &self.lineage
    }
    pub fn selection(&self) -> &PayloadSelection {
        &self.selection
    }
    pub fn plan(&self) -> &ValidatedPayloadPlan {
        &self.plan
    }
    pub fn original_selection(&self) -> &PayloadSelection {
        &self.original_selection
    }
    pub fn original_binding(&self) -> &SnapshotBindingRecord {
        &self.original_binding
    }
}

/// Validate recorded input selection, never granting access to the original source.
pub(crate) fn validate_original_selection(
    source: &SourceRef,
    binding: &SnapshotBindingRecord,
    selection: &PayloadSelection,
) -> io::Result<()> {
    use graphit_fs::NodeKind;
    if !binding.contains_node(selection.relative(), selection.kind())
        || match source.artifact_type() {
            ArtifactType::Agent => selection.kind() != NodeKind::File,
            ArtifactType::Skill => {
                selection.kind() != NodeKind::Directory
                    || !selection.relative().as_os_str().is_empty()
                    || !binding.contains_node(std::path::Path::new("SKILL.md"), NodeKind::File)
            }
            _ => false,
        }
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn limits() -> LineageLimits {
        LineageLimits::new(4, 4, 72).unwrap()
    }
    #[test]
    fn native_render_admission_keeps_unicode_names_and_exact_source_fingerprint() {
        let value = RenderLineage::new("v001", "éé", [0; 32], limits()).unwrap();
        assert_eq!(value.input_fingerprint(), "0".repeat(64));
        assert_eq!(value.selected_name(), "éé");
    }
    #[test]
    fn concrete_render_fields_and_total_are_validated_before_publication() {
        for (schema, name) in [("v001x", "éé"), ("v001", "ééx")] {
            assert!(RenderLineage::new(schema, name, [0; 32], limits()).is_err());
        }
        assert!(
            RenderLineage::new("v001", "éé", [0; 32], LineageLimits::new(4, 4, 71).unwrap())
                .is_err()
        );
        for name in ["", " a", "a ", "..", "a/b", "a\\b", "a\0"] {
            assert!(RenderLineage::new("v1", name, [0; 32], limits()).is_err());
        }
        assert!(RenderLineage::new("\nv1", "a", [0; 32], limits()).is_err());
    }
}
