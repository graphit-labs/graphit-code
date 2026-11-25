//! NN projections from the same generic ANTLR model, Source and Original.
use super::*;
use crate::antlr_engine::generic_model::ParsedGenericAntlrDocument;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeNnAntlrGenericProjectionIssue {
    LabelNotEmbedded,
    ExtentUnavailable,
    Conflict,
    OwnerExtentUnavailable,
}

pub enum NativeNnAntlrGenericProjection<'source> {
    Available {
        entity_index: usize,
        extent: EntityExtent<'source>,
    },
    Unavailable {
        entity_index: usize,
        reasons: Vec<NativeNnAntlrGenericProjectionIssue>,
    },
}

pub(super) fn language_matches(language: &str, grammar: &str) -> bool {
    matches!(
        (language, grammar),
        ("postgresql", "antlr-postgresql") | ("tsql", "antlr-tsql") | ("cobol85", "antlr-cobol85")
    )
}

fn extent<'profile: 'source, 'source>(
    document: &ParsedGenericAntlrDocument<'profile, 'source, '_, '_>,
    source: &SourceText<'source>,
    index: usize,
    original: &SnapshotContext,
) -> Result<Option<EntityExtent<'source>>, NativeNnError> {
    original.check().map_err(NativeNnError::Operation)?;
    let entity = document
        .entities()
        .get(index)
        .ok_or(NativeNnError::InvalidInput)?;
    if entity.extent_conflict() {
        return Ok(None);
    }
    let Some(range) = entity.extent() else {
        return Ok(None);
    };
    if entity.name().start() < range.start() || entity.name().end() > range.end() {
        return Ok(None);
    }
    EntityExtent::new(source, range, original)
        .map(Some)
        .map_err(|e| NativeNnError::Extent(Box::new(e)))
}

// Only a unique, strictly containing native ancestry can supply namespace frames.
fn owners<'profile: 'source, 'source>(
    document: &ParsedGenericAntlrDocument<'profile, 'source, '_, '_>,
    source: &SourceText<'source>,
    index: usize,
    original: &SnapshotContext,
) -> Result<Option<Vec<usize>>, NativeNnError> {
    let entities = document.entities();
    let mut current = index;
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    loop {
        original.check().map_err(NativeNnError::Operation)?;
        if !seen.insert(current) {
            return Err(NativeNnError::InvalidInput);
        }
        let child = entities.get(current).ok_or(NativeNnError::InvalidInput)?;
        let Some(child_extent) = extent(document, source, current, original)? else {
            return Ok(None);
        };
        let parent = match child.owner_indices() {
            [] => break,
            [parent] => *parent,
            _ => return Ok(None),
        };
        let Some(parent_extent) = extent(document, source, parent, original)? else {
            return Ok(None);
        };
        let child_range = child_extent.range();
        let parent_range = parent_extent.range();
        if parent_range.start() > child_range.start()
            || parent_range.end() < child_range.end()
            || (parent_range.start() == child_range.start()
                && parent_range.end() == child_range.end())
        {
            return Ok(None);
        }
        result.try_reserve(1).map_err(NativeNnError::Allocation)?;
        result.push(parent);
        current = parent;
    }
    result.reverse();
    Ok(Some(result))
}

/// Observe projection availability from the genuine profile and exact native ranges.
/// This reads the retained acquisition only; it does not parse or infer source again.
pub fn projections<'profile: 'source, 'source>(
    document: &ParsedGenericAntlrDocument<'profile, 'source, '_, '_>,
    original: &SnapshotContext,
) -> Result<Vec<NativeNnAntlrGenericProjection<'source>>, NativeNnError> {
    original.check().map_err(NativeNnError::Operation)?;
    let source = SourceText::new(document.source().as_bytes(), document.source().len().max(1))
        .map_err(NativeNnError::Range)?;
    projections_from(document, &source, original)
}

fn projections_from<'profile: 'source, 'source>(
    document: &ParsedGenericAntlrDocument<'profile, 'source, '_, '_>,
    source: &SourceText<'source>,
    original: &SnapshotContext,
) -> Result<Vec<NativeNnAntlrGenericProjection<'source>>, NativeNnError> {
    original.check().map_err(NativeNnError::Operation)?;
    if !std::ptr::eq(document.original_context(), original) {
        return Err(NativeNnError::RequestMismatch);
    }
    let profile = document.observations().main().profile;
    if !language_matches(profile.language().value(), profile.grammar().value()) {
        return Err(NativeNnError::InvalidInput);
    }
    let mut output = Vec::new();
    output
        .try_reserve(document.entities().len())
        .map_err(NativeNnError::Allocation)?;
    for (index, entity) in document.entities().iter().enumerate() {
        original.check().map_err(NativeNnError::Operation)?;
        let mut reasons = Vec::new();
        if !profile
            .embed_labels()
            .value()
            .iter()
            .any(|label| label.value() == entity.label())
        {
            reasons.push(NativeNnAntlrGenericProjectionIssue::LabelNotEmbedded);
        }
        if entity.extent_conflict() {
            reasons.push(NativeNnAntlrGenericProjectionIssue::Conflict);
        }
        let extent = extent(document, source, index, original)?;
        if extent.is_none() {
            reasons.push(NativeNnAntlrGenericProjectionIssue::ExtentUnavailable);
        }
        if owners(document, source, index, original)?.is_none() {
            reasons.push(NativeNnAntlrGenericProjectionIssue::OwnerExtentUnavailable);
        }
        if let Some(extent) = extent
            && reasons.is_empty()
        {
            output.push(NativeNnAntlrGenericProjection::Available {
                entity_index: index,
                extent,
            });
        } else {
            output.push(NativeNnAntlrGenericProjection::Unavailable {
                entity_index: index,
                reasons,
            });
        }
    }
    original.check().map_err(NativeNnError::Operation)?;
    Ok(output)
}

pub(super) fn encode(
    inputs: &[NativeNnAntlrGenericInput<'_, '_, '_, '_, '_>],
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    let schema = native_nn_projection_schema();
    let mut columns = NnColumns::new();
    let mut seen = HashSet::new();
    for input in inputs {
        check(original)?;
        let document = input.document;
        if !std::ptr::eq(document.original_context(), original.as_ref()) {
            return Err(NativeNnError::RequestMismatch);
        }
        let identity = document.identity_document();
        let (path, hash) = associated_source(&input.source, identity, document.source(), original)?;
        let source = SourceText::new(document.source().as_bytes(), document.source().len().max(1))
            .map_err(NativeNnError::Range)?;
        let available = projections_from(document, &source, original)?;
        let profile = document.observations().main().profile;
        let raw = match &input.source {
            NativeNnSource::Indexed(raw) | NativeNnSource::Physical { raw, .. } => *raw,
        };
        let observation = raw.branch_observation();
        for &index in input.entity_indices {
            check(original)?;
            let Some(NativeNnAntlrGenericProjection::Available { extent, .. }) =
                available.get(index)
            else {
                return Err(NativeNnError::InvalidInput);
            };
            let entity = &document.entities()[index];
            if !seen.insert((entity.uid(), entity.data_key())) {
                return Err(NativeNnError::DuplicateIdentity);
            }
            let text = extent.range().text();
            for (i, value) in [
                entity.uid(),
                entity.name().text(),
                entity.label(),
                &path,
                profile.language().value(),
                profile.grammar().value(),
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
            push_range(&mut columns.names, entity.name())?;
            push_extent(&mut columns.extents, extent)?;
            for owner in
                owners(document, &source, index, original)?.ok_or(NativeNnError::InvalidInput)?
            {
                check(original)?;
                let owner_end = columns
                    .owners
                    .values()
                    .len()
                    .checked_add(1)
                    .ok_or(NativeNnError::Representation)?;
                i32::try_from(owner_end).map_err(|_| NativeNnError::Representation)?;
                let owner = &document.entities()[owner];
                let owner_extent = EntityExtent::new(
                    &source,
                    owner.extent().ok_or(NativeNnError::InvalidInput)?,
                    original,
                )
                .map_err(|e| NativeNnError::Extent(Box::new(e)))?;
                let b = columns.owners.values();
                push_text(builder::<StringBuilder>(b, 0)?, owner.label())?;
                push_text(builder::<StringBuilder>(b, 1)?, owner.name().text())?;
                push_range(builder::<StructBuilder>(b, 2)?, owner.name())?;
                push_extent(builder::<StructBuilder>(b, 3)?, &owner_extent)?;
                push_owner_name(
                    b,
                    NameDomain::Generic,
                    NameKind::NativeSource,
                    owner.name(),
                    owner.name().text(),
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
