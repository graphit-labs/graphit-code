//! Native embedded composition while every parent/child parse owner remains alive.
use crate::ast_parser_resources::AstParserResources;
use graphit_ast::{
    antlr_engine::model::ParsedAntlrDocument,
    catalog::EffectiveProfile,
    native_nn::NativeNnSource,
    prepared_input::{OriginPiece, OriginalProjection, PreparedInput},
    profiles::ProfileParser,
    semantic_graph::{
        AstGraphAntlrInput, AstGraphEmbeddedComposition, AstGraphError, AstGraphGenericInput,
        AstGraphJsonInput, AstGraphTsInput, AstSemanticGraphBatch, assemble_graph_antlr,
        assemble_graph_generic, assemble_graph_json, assemble_graph_ts,
        bind_graph_generic_embedded_host, merge_graph_embedded, observe_graph_embedded,
    },
    semantic_model::ParsedJsonDocument,
    symbols::IdentityLimits,
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_generic_model::{
        GenericEmbeddedBlock, GenericEmbeddedLanguage, GenericModelError, GenericSemanticDocument,
        ParsedGenericTreeSitterDocument, PreparedGenericTreeSitterModel,
    },
    tree_sitter_model::ParsedTreeSitterDocument,
};
use graphit_core::file_io::SnapshotContext;
use serde_json::json;
use std::{collections::HashMap, io, sync::Arc};

enum Model<'profile> {
    Generic(PreparedGenericTreeSitterModel<'profile>),
    Specialized(PreparedTreeSitterContexts<'profile>),
    Json(PreparedTreeSitterContexts<'profile>),
    Antlr(&'profile EffectiveProfile),
}

#[derive(Clone, Eq, PartialEq)]
struct CycleKey {
    profile_index: usize,
    original_intervals: Vec<(usize, usize)>,
}

pub(in crate::foreground_ast_sources) struct Models<'profile> {
    parsers: &'profile AstParserResources,
    profiles: &'profile [EffectiveProfile],
    prepared: HashMap<usize, Model<'profile>>,
}

pub(super) fn source_copy<'a>(source: &NativeNnSource<'a>) -> NativeNnSource<'a> {
    match source {
        NativeNnSource::Indexed(raw) => NativeNnSource::Indexed(raw),
        NativeNnSource::Physical { capture, raw } => NativeNnSource::Physical { capture, raw },
    }
}

fn intervals(projection: &OriginalProjection<'_>) -> Option<Vec<(usize, usize)>> {
    match projection {
        OriginalProjection::Exact(range) => Some(vec![(range.start(), range.end())]),
        OriginalProjection::Compound(pieces) => {
            let mut ranges = Vec::new();
            for piece in pieces {
                match piece {
                    OriginPiece::Original(range) => ranges.push((range.start(), range.end())),
                    OriginPiece::Synthetic(_) => (),
                    OriginPiece::Unmappable(_) => return None,
                }
            }
            (!ranges.is_empty()).then_some(ranges)
        }
        _ => None,
    }
}

impl<'profile> Models<'profile> {
    pub fn new(
        parsers: &'profile AstParserResources,
        profiles: &'profile [EffectiveProfile],
    ) -> Self {
        Self {
            parsers,
            profiles,
            prepared: HashMap::new(),
        }
    }

    fn prepare(
        &mut self,
        index: usize,
        original: &SnapshotContext,
    ) -> Result<&Model<'profile>, AstGraphError> {
        if !self.prepared.contains_key(&index) {
            original.check()?;
            let profile = self
                .profiles
                .get(index)
                .ok_or(AstGraphError::NativeContract(
                    "foreign embedded profile index",
                ))?;
            let model = match (
                profile.parser().value(),
                profile.language().value().as_str(),
            ) {
                (
                    ProfileParser::TreeSitter,
                    "rust" | "go" | "javascript" | "typescript" | "tsx" | "python",
                ) => Model::Specialized(self.parsers.contexts(profile, original)?),
                (ProfileParser::TreeSitter, "json") => {
                    Model::Json(self.parsers.contexts(profile, original)?)
                }
                (ProfileParser::TreeSitter, _) => {
                    Model::Generic(PreparedGenericTreeSitterModel::prepare(
                        profile,
                        self.parsers.contexts(profile, original)?,
                        original,
                    )?)
                }
                (ProfileParser::Antlr4, "db2" | "plsql") => Model::Antlr(profile),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        format!(
                            "embedded native semantic model unavailable for {}",
                            profile.grammar().value()
                        ),
                    )
                    .into());
                }
            };
            self.prepared.insert(index, model);
        }
        self.prepared
            .get(&index)
            .ok_or(AstGraphError::NativeContract(
                "prepared embedded model disappeared",
            ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn append<'o, 'v, 'request>(
        &mut self,
        parsed: &ParsedGenericTreeSitterDocument<'_, 'profile, 'o, 'v, '_, 'request>,
        document: &GenericSemanticDocument<'_, 'profile, 'o, 'v, 'request>,
        batch: AstSemanticGraphBatch,
        source: &NativeNnSource<'_>,
        components: &'o [&'o str],
        limits: IdentityLimits,
        original: &Arc<SnapshotContext>,
        nn: Option<&mut super::GenericNnFile<'_>>,
    ) -> Result<AstSemanticGraphBatch, AstGraphError>
    where
        'profile: 'o,
    {
        let profile = parsed.observations()?.profile;
        let profile_index = self
            .profiles
            .iter()
            .position(|candidate| std::ptr::eq(candidate, profile))
            .ok_or(AstGraphError::NativeContract(
                "host profile is outside the loaded inventory",
            ))?;
        let source_length = document.original_text()?.len();
        let mut chain = vec![CycleKey {
            profile_index,
            original_intervals: vec![(0, source_length)],
        }];
        let batch = AstGraphEmbeddedComposition::new(batch, original)?;
        self.append_inner(
            parsed, document, batch, source, components, limits, original, &mut chain, nn,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn append_inner<'o, 'v, 'request>(
        &mut self,
        parsed: &ParsedGenericTreeSitterDocument<'_, 'profile, 'o, 'v, '_, 'request>,
        document: &GenericSemanticDocument<'_, 'profile, 'o, 'v, 'request>,
        mut batch: AstGraphEmbeddedComposition,
        source: &NativeNnSource<'_>,
        components: &'o [&'o str],
        limits: IdentityLimits,
        original: &Arc<SnapshotContext>,
        chain: &mut Vec<CycleKey>,
        mut nn: Option<&mut super::GenericNnFile<'_>>,
    ) -> Result<AstSemanticGraphBatch, AstGraphError>
    where
        'profile: 'o,
    {
        for (block_index, block) in parsed.embedded_blocks()?.iter().enumerate() {
            original.check()?;
            let metadata = json!({"embedded_index":block.embedded_index,"match_index":block.match_index,"body_slot":block.body_slot,"language":format!("{:?}",block.language),"body_start":block.body.prepared.start(),"body_end":block.body.prepared.end()});
            let GenericEmbeddedLanguage::Selected(language) = &block.language else {
                observe_graph_embedded(
                    &mut batch,
                    json!({"state":"LanguageUnavailable","selection":metadata}),
                    original,
                )?;
                continue;
            };
            let candidates: Vec<_> = self
                .profiles
                .iter()
                .enumerate()
                .filter(|(_, profile)| {
                    profile.language().value() == language || profile.grammar().value() == language
                })
                .map(|(index, _)| index)
                .collect();
            let [profile_index] = candidates.as_slice() else {
                observe_graph_embedded(
                    &mut batch,
                    json!({"state":if candidates.is_empty(){"ProfileNotFound"}else{"ProfileAmbiguous"},"selection":metadata,"profile_indices":candidates}),
                    original,
                )?;
                continue;
            };
            let Some(original_intervals) = intervals(&block.body.original) else {
                observe_graph_embedded(
                    &mut batch,
                    json!({"state":"NoProvableOriginalProgress","selection":metadata}),
                    original,
                )?;
                continue;
            };
            let key = CycleKey {
                profile_index: *profile_index,
                original_intervals,
            };
            if chain.contains(&key) {
                observe_graph_embedded(
                    &mut batch,
                    json!({"state":"NoProgressCycle","selection":metadata}),
                    original,
                )?;
                continue;
            }
            let prepared = match parsed.prepare_embedded(block_index)? {
                Ok(prepared) => prepared,
                Err(issue) => {
                    observe_graph_embedded(
                        &mut batch,
                        json!({"state":"PreparationUnavailable","reason":format!("{issue:?}"),"selection":metadata}),
                        original,
                    )?;
                    continue;
                }
            };
            chain.push(key);
            let child = prepared.with_input(|input| {
                let result = self.child(
                    *profile_index,
                    input,
                    components,
                    limits,
                    source,
                    original,
                    chain,
                    Some((document, block)),
                    nn.as_deref_mut(),
                );
                input
                    .original_context()
                    .check()
                    .map_err(GenericModelError::from)?;
                result
            });
            chain.pop();
            let child = child?;
            merge_graph_embedded(
                &mut batch,
                child,
                json!({"selection":metadata,"transform_diagnostics":format!("{:?}",prepared.diagnostics)}),
                original,
            )?;
        }
        batch.finish(original)
    }

    #[allow(clippy::too_many_arguments)]
    fn child<'o, 'v, 'p, 'request>(
        &mut self,
        profile_index: usize,
        input: &PreparedInput<'o, 'v, 'p, 'request>,
        components: &'o [&'o str],
        limits: IdentityLimits,
        source: &NativeNnSource<'_>,
        original: &Arc<SnapshotContext>,
        chain: &mut Vec<CycleKey>,
        host: Option<(
            &GenericSemanticDocument<'_, '_, '_, '_, '_>,
            &GenericEmbeddedBlock<'_, '_, '_>,
        )>,
        mut nn: Option<&mut super::GenericNnFile<'_>>,
    ) -> Result<AstSemanticGraphBatch, AstGraphError>
    where
        'profile: 'o,
    {
        match self.prepare(profile_index, original)? {
            Model::Specialized(factory) => {
                let parsed = ParsedTreeSitterDocument::collect(factory, input, components, limits)?;
                let document = parsed.assemble()?;
                if let Some(nn) = nn.as_deref_mut() {
                    nn.ts_document(&document, source)?;
                }
                assemble_graph_ts(
                    AstGraphTsInput {
                        document: &document,
                        source: source_copy(source),
                    },
                    original,
                )
            }
            Model::Json(factory) => {
                let parsed = ParsedJsonDocument::collect(factory, input, components, limits)
                    .map_err(io::Error::other)?;
                let document = parsed.assemble().map_err(io::Error::other)?;
                if let Some(nn) = nn.as_deref_mut() {
                    nn.json_document(&document, source)?;
                }
                assemble_graph_json(
                    AstGraphJsonInput {
                        document: &document,
                        source: source_copy(source),
                    },
                    original,
                )
            }
            Model::Antlr(profile) => {
                let parsed = ParsedAntlrDocument::collect(profile, input, components, limits)?;
                let document = parsed.assemble()?;
                if let Some(nn) = nn.as_deref_mut() {
                    nn.antlr_document(&document, source)?;
                }
                assemble_graph_antlr(
                    AstGraphAntlrInput {
                        document: &document,
                        source: source_copy(source),
                    },
                    original,
                )
            }
            Model::Generic(factory) => {
                let parsed = factory.collect(input, components, limits)?;
                let document = parsed.assemble()?;
                if let Some(nn) = nn.as_deref_mut() {
                    nn.generic_document(&document, source)?;
                }
                let batch = assemble_graph_generic(
                    AstGraphGenericInput {
                        document: &document,
                        source: source_copy(source),
                    },
                    original,
                )?;
                let mut batch = AstGraphEmbeddedComposition::new(batch, original)?;
                if let Some((host, block)) = host {
                    bind_graph_generic_embedded_host(host, block, &document, &mut batch, original)?;
                }
                self.append_inner(
                    &parsed, &document, batch, source, components, limits, original, chain, nn,
                )
            }
        }
    }
}
