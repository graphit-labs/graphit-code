//! ANTLR codec consumes only projections from the retained genuine model.

use super::*;
use crate::antlr_engine::model::AntlrProjection;

pub(super) fn encode(
    inputs: &[NativeNnAntlrInput<'_, '_, '_, '_, '_, '_>],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    let schema = native_nn_projection_schema();
    let mut columns = NnColumns::new();
    let mut seen = HashSet::new();
    for input in inputs {
        check(original)?;
        let parsed = input.document.parsed();
        if !std::ptr::eq(parsed.original_context(), original.as_ref()) {
            return Err(NativeNnError::RequestMismatch);
        }
        let identity = parsed.identity_document();
        let (path, hash) = associated_source(&input.source, identity, parsed.source(), original)?;
        let raw = match &input.source {
            NativeNnSource::Indexed(raw) | NativeNnSource::Physical { raw, .. } => *raw,
        };
        let projections = input
            .document
            .code_projections()
            .map_err(|e| NativeNnError::Antlr(Box::new(e)))?;
        let available: HashMap<_, _> = projections
            .iter()
            .filter_map(|p| match p {
                AntlrProjection::Available(p) => Some((p.entity_index, p)),
                AntlrProjection::Unavailable(_) => None,
            })
            .collect();
        let observation = raw.branch_observation();
        for &index in input.entity_indices {
            check(original)?;
            let p = available.get(&index).ok_or(NativeNnError::InvalidInput)?;
            let domain = match (p.language, p.grammar) {
                ("plsql", "antlr-plsql") => NameDomain::Oracle,
                ("db2", "antlr-db2") => NameDomain::Db2,
                _ => return Err(NativeNnError::InvalidInput),
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
            if !domain.matches(
                NameKind::Identifier,
                entity.name_raw().text(),
                entity.name(),
            ) {
                return Err(NativeNnError::InvalidInput);
            }
            push_text(&mut columns.name_kinds, NameKind::Identifier.wire())?;
            push_range(&mut columns.names, entity.name_raw())?;
            push_extent(&mut columns.extents, p.extent)?;
            let owner_end = columns
                .owners
                .values()
                .len()
                .checked_add(entity.namespace().len())
                .ok_or(NativeNnError::Representation)?;
            i32::try_from(owner_end).map_err(|_| NativeNnError::Representation)?;
            for owner in entity.namespace() {
                check(original)?;
                let ExtentObservation::Exact(extent) = owner.extent() else {
                    return Err(NativeNnError::InvalidInput);
                };
                if !domain.matches(NameKind::Identifier, owner.anchor().text(), owner.name()) {
                    return Err(NativeNnError::InvalidInput);
                }
                let b = columns.owners.values();
                push_text(builder::<StringBuilder>(b, 0)?, owner.identity().label())?;
                push_text(builder::<StringBuilder>(b, 1)?, owner.name())?;
                push_range(builder::<StructBuilder>(b, 2)?, owner.anchor())?;
                push_extent(builder::<StructBuilder>(b, 3)?, extent)?;
                push_owner_name(
                    b,
                    domain,
                    NameKind::Identifier,
                    owner.anchor(),
                    owner.name(),
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
            columns.push_projection(p.text, original)?;
        }
    }
    columns.finish(schema)
}
