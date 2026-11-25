//! Rust Tree-sitter codec consumes only projections from the retained genuine model.

use super::*;
use crate::tree_sitter_model::{TsOwnerSelection, TsProjection};

pub(super) fn encode(
    inputs: &[NativeNnTsInput<'_, '_, '_, '_, '_, '_>],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    let schema = native_nn_projection_schema();
    let mut columns = NnColumns::new();
    let mut seen = HashSet::new();
    for input in inputs {
        check(original)?;
        if !std::ptr::eq(
            input.document.original_context().map_err(model_error)?,
            original.as_ref(),
        ) {
            return Err(NativeNnError::RequestMismatch);
        }
        let identity = input.document.identity_document().map_err(model_error)?;
        let source = input.document.source().map_err(model_error)?;
        let (path, hash) = associated_source(&input.source, identity, source.as_str(), original)?;
        let raw = match &input.source {
            NativeNnSource::Indexed(raw) | NativeNnSource::Physical { raw, .. } => *raw,
        };
        let projections = input.document.code_projections().map_err(model_error)?;
        let observation = raw.branch_observation();
        for &index in input.entity_indices {
            check(original)?;
            let position = projections
                .binary_search_by_key(&index, |projection| match projection {
                    TsProjection::Available(p) => p.entity_index,
                    TsProjection::Unavailable(p) => p.entity_index,
                })
                .map_err(|_| NativeNnError::InvalidInput)?;
            let TsProjection::Available(p) = &projections[position] else {
                return Err(NativeNnError::InvalidInput);
            };
            let entity = p.entity;
            if !seen.insert((entity.identity().uid().as_str(), entity.data_key())) {
                return Err(NativeNnError::DuplicateIdentity);
            }
            for (i, value) in [
                entity.identity().uid().as_str(),
                entity.name(),
                entity.identity().label(),
                &path,
                p.language,
                p.grammar,
                if observation.is_some() {
                    "branch"
                } else {
                    "module"
                },
                p.text,
            ]
            .into_iter()
            .enumerate()
            {
                push_text(&mut columns.strings[i], value)?;
            }
            push_optional(
                &mut columns.strings[8],
                observation.and_then(|o| o.physical_branch),
            )?;
            push_optional(
                &mut columns.strings[9],
                observation.and_then(|o| o.inherited_target),
            )?;
            let path_end = columns
                .paths
                .values()
                .len()
                .checked_add(identity.relative_components().len())
                .ok_or(NativeNnError::Representation)?;
            i32::try_from(path_end).map_err(|_| NativeNnError::Representation)?;
            for component in identity.relative_components() {
                push_text(columns.paths.values(), component)?;
            }
            columns.paths.append(true);
            let name_domain = match (p.language, p.grammar) {
                ("rust", "tree-sitter-rust") => NameDomain::Rust,
                ("go", "tree-sitter-go") => NameDomain::Go,
                ("python", "tree-sitter-python") => NameDomain::Python,
                ("javascript", "tree-sitter-javascript") => {
                    NameDomain::Ecma(crate::ecma_names::EcmaNameDialect::JavaScript)
                }
                ("typescript", "tree-sitter-typescript") | ("tsx", "tree-sitter-tsx") => {
                    NameDomain::Ecma(crate::ecma_names::EcmaNameDialect::TypeScript)
                }
                _ => return Err(NativeNnError::InvalidInput),
            };
            let name_kind = match entity.name_representation() {
                crate::tree_sitter_model::TsNameRepresentation::Identifier => NameKind::Identifier,
                crate::tree_sitter_model::TsNameRepresentation::LiteralSource { .. } => {
                    NameKind::LiteralSource
                }
            };
            if !name_domain.matches(name_kind, entity.name_raw().text(), entity.name()) {
                return Err(NativeNnError::InvalidInput);
            }
            push_text(&mut columns.name_kinds, name_kind.wire())?;
            push_range(&mut columns.names, entity.name_raw())?;
            push_extent(&mut columns.extents, p.extent)?;
            let owner_end = columns
                .owners
                .values()
                .len()
                .checked_add(entity.namespaces().len())
                .ok_or(NativeNnError::Representation)?;
            i32::try_from(owner_end).map_err(|_| NativeNnError::Representation)?;
            for owner in entity.namespaces() {
                check(original)?;
                let ExtentObservation::Exact(extent) = &owner.extent else {
                    return Err(NativeNnError::InvalidInput);
                };
                let owner_kind = match owner.selection {
                    TsOwnerSelection::ConfiguredIdentifier
                    | TsOwnerSelection::DerivedDeclarationIdentifier => NameKind::Identifier,
                    TsOwnerSelection::ConfiguredRustImplType => NameKind::RustType,
                    TsOwnerSelection::ConfiguredLiteralSource
                    | TsOwnerSelection::DerivedDeclarationLiteralSource => NameKind::LiteralSource,
                };
                let b = columns.owners.values();
                push_text(builder::<StringBuilder>(b, 0)?, &owner.label)?;
                push_text(builder::<StringBuilder>(b, 1)?, &owner.name)?;
                push_range(builder::<StructBuilder>(b, 2)?, owner.anchor)?;
                push_extent(builder::<StructBuilder>(b, 3)?, extent)?;
                push_owner_name(b, name_domain, owner_kind, owner.anchor, &owner.name)?;
                b.append(true);
            }
            columns.owners.append(true);
            push_text(&mut columns.data_keys, entity.data_key())?;
            columns.raw_versions.append_value(raw.version());
            columns
                .source_sha
                .append_value(hash)
                .map_err(NativeNnError::Arrow)?;
            columns.push_projection(p.text, original)?;
        }
    }
    columns.finish(schema)
}

fn model_error(error: TreeSitterModelError) -> NativeNnError {
    NativeNnError::TreeSitter(Box::new(error))
}
