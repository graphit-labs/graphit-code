use super::rust_semantic;
use arrow_array::RecordBatch;
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    indexed_source::IndexedSourceSnapshot,
    native_nn::{NativeNnModel, NativeNnSource, NativeNnTsInput, encode_native_nn_ts},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    source_inspection::IndexedSource,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_model::{TreeSitterSemanticDocument, TsProjection},
};
use graphit_core::file_io::SnapshotContext;
use std::{error::Error, sync::Arc};

pub fn catalog(original: &SnapshotContext) -> CatalogDraft {
    let id = ProfileId::new("rust-nn-fixture", 256).unwrap();
    let decoder = ProfileDecoder::new(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })
        .unwrap(),
    );
    let bytes = b"language: rust\ngrammar: tree-sitter-rust\nembed_labels: [Function]\ncontext_types: {mod_item: Namespace, impl_item: Struct, trait_item: Trait, function_item: Function}\ncontext_name_queries: {impl_item: '(impl_item type: (_) @name) @context'}\nqueries:\n  - {data_key: functions, graph_label: Function, pattern: '(function_item name: (identifier) @name)'}\n";
    let profiles = [decoder
        .decode(
            ProfileInput { id: &id, bytes },
            &ProfileLimits::new(ProfileLimitsInput {
                input_bytes: 1 << 20,
                nodes: 32768,
                depth: 64,
                text_bytes: 1 << 20,
                scalar_bytes: 32768,
                collection_entries: 4096,
            })
            .unwrap(),
            original,
        )
        .unwrap()];
    CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles: &profiles,
        }],
        &CatalogLimits::new(CatalogLimitsInput {
            policy_entries: 4096,
            selector_bytes: 32768,
        })
        .unwrap(),
        original,
    )
    .unwrap()
}

pub fn identity_limits() -> IdentityLimits {
    IdentityLimits::new(IdentityLimitsInput {
        source_bytes: 1 << 20,
        path_components: 32,
        path_component_bytes: 4096,
        identity_text_bytes: 4096,
        owner_depth: 64,
        uid_bytes: 32768,
    })
    .unwrap()
}

// Expand into the caller's scope so every borrowed owner outlives the document.
macro_rules! rust_document {
    ($document:ident, $path:expr, $source:expr, $original:expr $(,)?) => {
        let rust_fixture_original: &graphit_core::file_io::SnapshotContext = $original;
        let rust_fixture_path: &[&str] = $path;
        let rust_fixture_source: &str = $source;
        let rust_fixture_catalog = rust_semantic::catalog(rust_fixture_original);
        let rust_fixture_factory =
            graphit_ast::tree_sitter_context::PreparedTreeSitterContexts::prepare(
                &rust_fixture_catalog.profiles()[0],
                graphit_ast::tree_sitter_engine::GrammarLanguage::from_language(
                    tree_sitter_rust::LANGUAGE.into(),
                )?,
                rust_fixture_original,
            )?;
        let rust_fixture_span = graphit_ast::prepared_input::ByteSpan {
            start: 0,
            end: rust_fixture_source.len() as u64,
        };
        let rust_fixture_segments = (!rust_fixture_source.is_empty()).then_some(
            graphit_ast::prepared_input::OriginSegment::Direct {
                prepared: rust_fixture_span,
                original: rust_fixture_span,
            },
        );
        let rust_fixture_input = graphit_ast::prepared_input::PreparedInput::new(
            rust_fixture_source.as_bytes(),
            rust_fixture_source.as_bytes(),
            graphit_ast::prepared_input::OriginPlan {
                segments: rust_fixture_segments.as_slice(),
                removed_original: &[],
                anchors: &[],
            },
            rust_fixture_original,
        )?;
        let rust_fixture_parsed =
            graphit_ast::tree_sitter_model::ParsedTreeSitterDocument::collect(
                &rust_fixture_factory,
                &rust_fixture_input,
                rust_fixture_path,
                rust_semantic::identity_limits(),
            )?;
        let $document = rust_fixture_parsed.assemble()?;
    };
}

pub(crate) use rust_document;

pub fn indices(
    document: &TreeSitterSemanticDocument<'_, '_, '_, '_, '_>,
) -> Result<Vec<usize>, Box<dyn Error>> {
    document
        .code_projections()?
        .iter()
        .map(|projection| match projection {
            TsProjection::Available(projection) => Ok(projection.entity_index),
            TsProjection::Unavailable(projection) => Err(format!(
                "configured Rust fixture projection unavailable for entity {}",
                projection.entity_index
            )
            .into()),
        })
        .collect()
}

#[allow(
    dead_code,
    reason = "shared fixture; embedding encoding is exercised by corpus and live search tests"
)]
pub fn encode(
    raw: &IndexedSourceSnapshot,
    path: &[&str],
    model: &NativeNnModel<'_>,
    generation: u64,
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, Box<dyn Error>> {
    let IndexedSource::Available(text) = raw.source(original)? else {
        return Err("actual indexed Source required".into());
    };
    rust_semantic::rust_document!(document, path, text, original);
    let indices = indices(&document)?;
    let vectors = vec![vec![1.; usize::try_from(model.dimensions)?]; indices.len()];
    Ok(encode_native_nn_ts(
        model,
        &[NativeNnTsInput {
            document: &document,
            source: NativeNnSource::Indexed(raw),
            entity_indices: &indices,
            vectors: &vectors,
            model_generation: Some(generation),
        }],
        original,
    )?)
}
