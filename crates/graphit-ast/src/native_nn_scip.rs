//! Additional compiler definitions use the official SCIP/Source association.
use super::*;
use crate::semantic_graph::{
    AstGraphValue,
    scip_document::{AstScipGraphDocument, apply_scip_document, assemble_scip_source},
};

/// Prepare only definitions introduced by SCIP, preserving the graph's real
/// identities and grammar. Parser entities retain their existing projections.
pub fn prepare_native_nn_scip(
    graph: &AstScipGraphDocument,
    document: &::scip::types::Document,
    source: NativeNnSource<'_>,
    limits: crate::symbols::IdentityLimits,
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, NativeNnError> {
    check(original)?;
    let result = (|| {
        let raw = match &source {
            NativeNnSource::Indexed(raw) | NativeNnSource::Physical { raw, .. } => *raw,
        };
        let text = match &source {
            NativeNnSource::Indexed(raw) => {
                match raw.source(original).map_err(NativeNnError::Source)? {
                    IndexedSource::Available(text) => text,
                    IndexedSource::Missing => return Err(NativeNnError::InvalidInput),
                }
            }
            NativeNnSource::Physical { capture, .. } => capture
                .source(original)
                .map_err(|error| NativeNnError::PhysicalSource(Box::new(error)))?,
        };
        let source_text =
            SourceText::new(text.as_bytes(), text.len().max(1)).map_err(NativeNnError::Range)?;
        // Reuse the official producer to verify occurrence coordinates, symbols,
        // UIDs and extents against this same retained Raw/physical Source.
        let associated = assemble_scip_source(
            document,
            match &source {
                NativeNnSource::Indexed(raw) => NativeNnSource::Indexed(raw),
                NativeNnSource::Physical { capture, raw } => {
                    NativeNnSource::Physical { capture, raw }
                }
            },
            limits,
            original,
        )
        .map_err(graph_error)?;
        let captured = associated.association();
        let file = &graph.parts.association;
        if captured.path() != file.path()
            || captured.relative_components() != file.relative_components()
            || captured.source_sha256() != file.source_sha256()
            || captured.source_observation() != file.source_observation()
        {
            return Err(NativeNnError::InvalidInput);
        }
        // The official producer validated this captured Source's text and ranges.
        // Canonical identities still belong to the original FILE publication;
        // an unrelated Source commit cannot replace that provenance.
        let mut canonical_parts = associated.into_parts();
        canonical_parts.association = file.clone();
        canonical_parts.file_observation = graph.parts.file_observation.clone();
        let canonical =
            apply_scip_document(canonical_parts, document, text, original).map_err(graph_error)?;
        let canonical = canonical
            .parts
            .nodes
            .iter()
            .map(|node| (node.key(), node))
            .collect::<HashMap<_, _>>();
        let language = node_text(&graph.parts.file_observation, "lang")?;
        let grammar = node_text(&graph.parts.file_observation, "grammar")?;
        let observation = raw.branch_observation();
        let mut columns = NnColumns::new();
        let mut seen = HashSet::new();
        for node in &graph.parts.nodes {
            check(original)?;
            if !matches!(node.properties().get("data_key"),Some(AstGraphValue::String(key)) if key=="scip_definitions")
            {
                continue;
            }
            let expected = canonical
                .get(node.key())
                .ok_or(NativeNnError::InvalidInput)?;
            if node.label() != expected.label() {
                return Err(NativeNnError::InvalidInput);
            }
            for key in [
                "uid",
                "data_key",
                "name_start_byte",
                "name_end_byte",
                "start_byte",
                "end_byte",
                "source_sha256",
                "raw_version",
            ] {
                if node.properties().get(key) != expected.properties().get(key) {
                    return Err(NativeNnError::InvalidInput);
                }
            }
            let uid = node_text(expected, "uid")?;
            if !seen.insert(uid) {
                return Err(NativeNnError::DuplicateIdentity);
            }
            let range = |prefix: &str| -> Result<SourceRange<'_>, NativeNnError> {
                let position = |suffix: &str| -> Result<usize, NativeNnError> {
                    let key = format!("{prefix}{suffix}");
                    let Some(AstGraphValue::Int64(value)) = expected.properties().get(&key) else {
                        return Err(NativeNnError::InvalidInput);
                    };
                    usize::try_from(*value).map_err(|_| NativeNnError::Representation)
                };
                source_text
                    .range(position("start_byte")?, position("end_byte")?)
                    .map_err(NativeNnError::Range)
            };
            let name = range("name_")?;
            let extent = EntityExtent::new(&source_text, range("")?, original)
                .map_err(|error| NativeNnError::Extent(Box::new(error)))?;
            for (index, value) in [
                uid,
                name.text(),
                node.label(),
                raw.path(),
                language,
                grammar,
                if observation.is_some() {
                    "branch"
                } else {
                    "module"
                },
                extent.range().text(),
            ]
            .into_iter()
            .enumerate()
            {
                push_text(&mut columns.strings[index], value)?;
            }
            push_optional(
                &mut columns.strings[8],
                observation.and_then(|value| value.physical_branch),
            )?;
            push_optional(
                &mut columns.strings[9],
                observation.and_then(|value| value.inherited_target),
            )?;
            for component in graph.parts.association.relative_components() {
                push_text(columns.paths.values(), component)?;
            }
            columns.paths.append(true);
            push_range(&mut columns.names, name)?;
            push_extent(&mut columns.extents, &extent)?;
            columns.owners.append(true);
            columns.raw_versions.append_value(file.raw_version());
            columns
                .source_sha
                .append_value(graph.parts.association.source_sha256())
                .map_err(NativeNnError::Arrow)?;
            columns.push_projection(extent.range().text(), original)?;
            push_text(&mut columns.data_keys, "scip_definitions")?;
            push_text(&mut columns.name_kinds, NameKind::ScipSource.wire())?;
        }
        let batch = columns.finish(native_nn_projection_schema())?;
        validate_native_nn_projection(&batch, original)?;
        Ok(batch)
    })();
    if let NativeNnSource::Physical { capture, .. } = &source {
        capture
            .revalidate(original)
            .map_err(|error| NativeNnError::PhysicalSource(Box::new(error)))?;
    }
    check(original)?;
    result
}

fn node_text<'a>(
    node: &'a crate::semantic_graph::AstGraphNode,
    key: &str,
) -> Result<&'a str, NativeNnError> {
    match node.properties().get(key) {
        Some(AstGraphValue::String(value)) if text_valid(value) => Ok(value),
        _ => Err(NativeNnError::InvalidInput),
    }
}
fn graph_error(error: crate::semantic_graph::AstGraphError) -> NativeNnError {
    NativeNnError::Operation(io::Error::other(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::scip::types::{Document, Occurrence, PositionEncoding};
    use graphit_core::file_lock::Cancellation;
    use graphit_data::lance_module_store::LanceModuleStore;

    #[tokio::test]
    async fn compiler_only_projections_bind_official_ranges_and_raw_source_without_parser()
    -> Result<(), Box<dyn Error>> {
        let original = Arc::new(SnapshotContext::service(Cancellation::default()));
        let directory = tempfile::tempdir()?;
        let schema = crate::indexed_source::source_schema();
        let text = "type Alias = i32;\n";
        let store = LanceModuleStore::create(
            directory.path().join("raw.lance").to_str().unwrap(),
            schema.clone(),
            vec![RecordBatch::try_new(
                schema,
                vec![
                    Arc::new(StringArray::from(vec!["source.custom"])),
                    Arc::new(StringArray::from(vec![text])),
                ],
            )?],
        )
        .await?;
        let raw = crate::indexed_source::read_indexed_source(
            &store.snapshot(),
            "source.custom",
            &original,
        )
        .await?;
        let document = Document {
            relative_path: "source.custom".into(),
            language: "custom-compiler-language".into(),
            text: text.into(),
            position_encoding: PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
            occurrences: vec![Occurrence {
                range: vec![0, 5, 10],
                enclosing_range: vec![0, 0, 0, 17],
                symbol: "scip-test cargo fixture 1 Alias#".into(),
                symbol_roles: 1,
                ..Default::default()
            }],
            ..Default::default()
        };
        let limits = crate::symbols::IdentityLimits::new(crate::symbols::IdentityLimitsInput {
            source_bytes: 1 << 20,
            path_components: 32,
            path_component_bytes: 4096,
            identity_text_bytes: 4096,
            owner_depth: 64,
            uid_bytes: 32768,
        })?;
        let batch =
            assemble_scip_source(&document, NativeNnSource::Indexed(&raw), limits, &original)?;
        let applied = apply_scip_document(batch.into_parts(), &document, text, &original)?;
        let prepared = prepare_native_nn_scip(
            &applied,
            &document,
            NativeNnSource::Indexed(&raw),
            limits,
            &original,
        )?;
        assert_eq!(prepared.num_rows(), 1);
        assert_eq!(
            column::<StringArray>(&prepared, "data_key")?.value(0),
            "scip_definitions"
        );
        assert_eq!(
            column::<StringArray>(&prepared, "grammar")?.value(0),
            "scip-compiler"
        );
        assert_eq!(
            column::<StringArray>(&prepared, "projection")?.value(0),
            "type Alias = i32;"
        );
        let model = NativeNnModel {
            vector_id: [51; 32],
            dimensions: 2,
            model_id: "compiler/fixture",
            revision: "one",
        };
        let completed =
            complete_native_nn(&prepared, &model, Some(8), &[vec![0.25, 0.75]], &original)?;
        validate_native_nn(&completed, &model, &original)?;
        assert_eq!(
            node_text(&applied.parts.nodes[0], "uid")?,
            column::<StringArray>(&completed, "uid")?.value(0)
        );
        let mut current = graphit_data::lance_branch_store::BranchSnapshot::capture_observed(
            directory.path().join("raw.lance").to_str().unwrap(),
            None,
            None,
        )
        .await?;
        current = current
            .prepare_merge(
                vec![RecordBatch::try_new(
                    crate::indexed_source::source_schema(),
                    vec![
                        Arc::new(StringArray::from(vec!["unrelated.custom"])),
                        Arc::new(StringArray::from(vec!["unrelated FILE"])),
                    ],
                )?],
                vec!["path".into()],
            )
            .await?
            .commit()
            .await?;
        let current_source =
            crate::indexed_source::read_branch_source(&current, "source.custom", &original).await?;
        assert!(current_source.version() > raw.version());
        let replayed = prepare_native_nn_scip(
            &applied,
            &document,
            NativeNnSource::Indexed(&current_source),
            limits,
            &original,
        )?;
        assert_eq!(
            column::<StringArray>(&prepared, "uid")?,
            column::<StringArray>(&replayed, "uid")?
        );
        assert_eq!(
            column::<StringArray>(&prepared, "projection")?,
            column::<StringArray>(&replayed, "projection")?
        );
        assert_eq!(
            column::<UInt64Array>(&prepared, "raw_version")?,
            column::<UInt64Array>(&replayed, "raw_version")?
        );
        let lexical = crate::entity_search::encode_file(&applied.parts, text, &original)?;
        let replayed_corpus = crate::corpus::encode_file(
            crate::corpus::schema(),
            lexical,
            replayed,
            None,
            &original,
        )?;
        assert_eq!(
            replayed_corpus.num_rows(),
            1,
            "replay must preserve one consistent FILE provenance through corpus encoding"
        );
        let mut invalid = document.clone();
        invalid.text = "different".into();
        assert!(
            prepare_native_nn_scip(
                &applied,
                &invalid,
                NativeNnSource::Indexed(&raw),
                limits,
                &original
            )
            .is_err()
        );
        Ok(())
    }
}
