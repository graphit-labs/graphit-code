//! Generic NN consumes original native ranges from the retained model.
use super::*;
use crate::tree_sitter_generic_model::GenericCodeProjection;

pub(super) fn language_is_generic(language: &str, grammar: &str) -> bool {
    text_valid(language)
        && text_valid(grammar)
        && ![
            "rust",
            "go",
            "javascript",
            "typescript",
            "tsx",
            "python",
            "json",
            "plsql",
            "db2",
            "postgresql",
            "tsql",
            "cobol85",
        ]
        .iter()
        .any(|reserved| language.eq_ignore_ascii_case(reserved))
}

pub(super) fn encode(
    inputs: &[NativeNnGenericInput<'_, '_, '_, '_, '_, '_>],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    let schema = native_nn_projection_schema();
    let mut columns = NnColumns::new();
    let mut seen = HashSet::new();
    for input in inputs {
        check(original)?;
        let document = input.document;
        if !std::ptr::eq(
            document.original_context().map_err(model_error)?,
            original.as_ref(),
        ) {
            return Err(NativeNnError::RequestMismatch);
        }
        let identity = document.identity_document().map_err(model_error)?;
        let source = document.source().map_err(model_error)?;
        let (path, hash) = associated_source(&input.source, identity, source.as_str(), original)?;
        let raw = match &input.source {
            NativeNnSource::Indexed(raw) | NativeNnSource::Physical { raw, .. } => *raw,
        };
        let profile = document.observations().map_err(model_error)?.profile;
        let language = profile.language().value();
        let grammar = profile.grammar().value();
        if !language_is_generic(language, grammar) {
            return Err(NativeNnError::InvalidInput);
        }
        let projections = document.code_projections().map_err(model_error)?;
        let entities = document.entities().map_err(model_error)?;
        let observation = raw.branch_observation();
        for &index in input.entity_indices {
            check(original)?;
            let position = projections
                .binary_search_by_key(&index, |projection| match projection {
                    GenericCodeProjection::Available { entity_index, .. }
                    | GenericCodeProjection::Unavailable { entity_index, .. } => *entity_index,
                })
                .map_err(|_| NativeNnError::InvalidInput)?;
            let GenericCodeProjection::Available { extent, .. } = &projections[position] else {
                return Err(NativeNnError::InvalidInput);
            };
            let entity = &entities[index];
            if !seen.insert((entity.identity().uid().as_str(), entity.data_key())) {
                return Err(NativeNnError::DuplicateIdentity);
            }
            let name = entity.name().raw();
            let text = extent.range().text();
            for (i, value) in [
                entity.identity().uid().as_str(),
                name.text(),
                entity.identity().label(),
                &path,
                language,
                grammar,
                if observation.is_some() {
                    "branch"
                } else {
                    "module"
                },
                text,
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
            push_text(&mut columns.name_kinds, NameKind::NativeSource.wire())?;
            push_range(&mut columns.names, name)?;
            push_extent(&mut columns.extents, extent)?;
            for owner in document.owner_frames(index).map_err(model_error)? {
                check(original)?;
                let owner_end = columns
                    .owners
                    .values()
                    .len()
                    .checked_add(1)
                    .ok_or(NativeNnError::Representation)?;
                i32::try_from(owner_end).map_err(|_| NativeNnError::Representation)?;
                let owner_extent = owner.extent.ok_or(NativeNnError::InvalidInput)?;
                if !owner.conflicts.is_empty() {
                    return Err(NativeNnError::InvalidInput);
                }
                let b = columns.owners.values();
                push_text(builder::<StringBuilder>(b, 0)?, owner.label)?;
                push_text(builder::<StringBuilder>(b, 1)?, owner.anchor.text())?;
                push_range(builder::<StructBuilder>(b, 2)?, owner.anchor)?;
                push_extent(builder::<StructBuilder>(b, 3)?, owner_extent)?;
                push_owner_name(
                    b,
                    NameDomain::Generic,
                    NameKind::NativeSource,
                    owner.anchor,
                    owner.anchor.text(),
                )?;
                b.append(true);
            }
            columns.owners.append(true);
            push_text(&mut columns.data_keys, entity.data_key())?;
            columns.raw_versions.append_value(raw.version());
            columns
                .source_sha
                .append_value(hash)
                .map_err(NativeNnError::Arrow)?;
            columns.push_projection(text, original)?;
        }
    }
    columns.finish(schema)
}

fn model_error(error: GenericModelError) -> NativeNnError {
    NativeNnError::Generic(Box::new(error))
}
