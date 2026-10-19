use super::*;
use crate::{
    catalog::Embedded,
    prepared_input::{ByteSpan, OriginAnchor, OriginPlan, OriginSegment, SyntheticReason},
    tree_sitter_engine::{CompiledQuery, ParseControl, QueryMatch},
    tree_sitter_profile::PreparedTreeSitterProfile,
};
use std::collections::HashSet;

pub(super) struct PreparedEmbedded<'profile> {
    descriptor: &'profile Embedded,
    compiled: CompiledQuery,
    numeric_references: Option<Arc<regex::Regex>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericEmbeddedLanguage {
    Selected(String),
    Missing,
    Unmapped(String),
    Ambiguous(Vec<String>),
}

pub struct GenericEmbeddedObservation<'profile> {
    pub embedded_index: usize,
    pub match_index: usize,
    pub descriptor: &'profile Embedded,
    pub matched: Arc<QueryMatch>,
}

pub(super) struct CollectedEmbedded<'profile, 'o, 'v> {
    pub blocks: Vec<GenericEmbeddedBlock<'profile, 'o, 'v>>,
    pub observations: Vec<GenericEmbeddedObservation<'profile>>,
}

/// Metadata for the selected SDK capture; its owned text remains in the retained raw match.
pub struct GenericEmbeddedBody<'o, 'v> {
    pub node_id: usize,
    pub prepared: SourceRange<'v>,
    pub original: OriginalProjection<'o>,
}

pub struct GenericEmbeddedBlock<'profile, 'o, 'v> {
    pub embedded_index: usize,
    pub match_index: usize,
    pub body_slot: usize,
    pub descriptor: &'profile Embedded,
    pub body: GenericEmbeddedBody<'o, 'v>,
    pub language: GenericEmbeddedLanguage,
    numeric_references: Option<Arc<regex::Regex>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericEmbeddedIssue {
    LanguageUnavailable,
    MissingNormalizer(String),
    EmptyReplacementKey,
    PartialMappedAtom,
    MixedSyntheticReplacement,
    RecoveredBody,
    InvalidNumericReference { prepared_start: usize },
}

pub(super) fn prepare<'profile>(
    profile: &'profile EffectiveProfile,
    factory: &PreparedTreeSitterProfile<'profile>,
    original: &SnapshotContext,
) -> Result<Arc<[PreparedEmbedded<'profile>]>, GenericModelError> {
    let mut output = Vec::new();
    let numeric_reference_pattern = profile
        .text_normalizers()
        .value()
        .iter()
        .any(|(_, normalizer)| *normalizer.value().numeric_char_refs().value())
        .then(|| regex::Regex::new(r"&#[^;&\s]*;?").map(Arc::new))
        .transpose()?;
    for (index, descriptor) in profile.embedded().value().iter().enumerate() {
        original.check()?;
        let descriptor = descriptor.value();
        let compiled = factory.compile_query(descriptor.pattern().value(), original)?;
        let text = descriptor.text_capture().value();
        if text.is_empty() || compiled.capture_index_for_name(text).is_none() {
            return Err(GenericModelError::InvalidEmbeddedRole {
                embedded_index: index,
                role: "text",
                capture: text.clone(),
            });
        }
        let lang = descriptor.lang_capture().value();
        if !lang.is_empty() && compiled.capture_index_for_name(lang).is_none() {
            return Err(GenericModelError::InvalidEmbeddedRole {
                embedded_index: index,
                role: "lang",
                capture: lang.clone(),
            });
        }
        let numeric_references = profile
            .text_normalizers()
            .value()
            .iter()
            .find(|(name, _)| name.value() == descriptor.normalize().value())
            .filter(|(_, normalizer)| *normalizer.value().numeric_char_refs().value())
            .and(numeric_reference_pattern.clone());
        output.push(PreparedEmbedded {
            descriptor,
            compiled,
            numeric_references,
        });
    }
    Ok(output.into())
}

pub(super) fn collect<'profile, 'o, 'v>(
    prepared: &[PreparedEmbedded<'profile>],
    observations: &TreeSitterProfileMatches<'profile, 'o, 'v>,
    input: &PreparedInput<'o, 'v, '_, '_>,
) -> Result<CollectedEmbedded<'profile, 'o, 'v>, GenericModelError> {
    let original = input.original_context();
    let mut claimed = HashSet::new();
    let mut output = Vec::new();
    let mut raw_observations = Vec::new();
    for (embedded_index, prepared) in prepared.iter().enumerate() {
        for (match_index, matched) in observations
            .tree
            .matches_with_context(&prepared.compiled, ParseControl::default(), original)?
            .into_iter()
            .enumerate()
        {
            original.check()?;
            let descriptor = prepared.descriptor;
            let matched = Arc::new(matched);
            raw_observations.push(GenericEmbeddedObservation {
                embedded_index,
                match_index,
                descriptor,
                matched: Arc::clone(&matched),
            });
            let mut languages = Vec::new();
            for capture in &matched.captures {
                if !descriptor.lang_capture().value().is_empty()
                    && capture.name == *descriptor.lang_capture().value()
                    && !languages.contains(&capture.text)
                {
                    languages.push(capture.text.clone());
                }
            }
            let language = match languages.as_slice() {
                [] => {
                    if descriptor.r#default().value().is_empty() {
                        GenericEmbeddedLanguage::Missing
                    } else {
                        GenericEmbeddedLanguage::Selected(descriptor.r#default().value().clone())
                    }
                }
                [language] if language.is_empty() => {
                    if descriptor.r#default().value().is_empty() {
                        GenericEmbeddedLanguage::Missing
                    } else {
                        GenericEmbeddedLanguage::Selected(descriptor.r#default().value().clone())
                    }
                }
                [language] => descriptor
                    .languages()
                    .value()
                    .iter()
                    .find(|(key, _)| key.value() == language)
                    .map(|(_, target)| GenericEmbeddedLanguage::Selected(target.value().clone()))
                    .unwrap_or_else(|| GenericEmbeddedLanguage::Unmapped(language.clone())),
                _ => GenericEmbeddedLanguage::Ambiguous(languages),
            };
            for (body_slot, native) in matched.captures.iter().enumerate() {
                if native.name != *descriptor.text_capture().value()
                    || !claimed.insert(native.node_id)
                {
                    continue;
                }
                let start = u64::try_from(native.range.start)
                    .map_err(|_| OriginError::ArithmeticOverflow)?;
                let end =
                    u64::try_from(native.range.end).map_err(|_| OriginError::ArithmeticOverflow)?;
                output.push(GenericEmbeddedBlock {
                    embedded_index,
                    match_index,
                    body_slot,
                    descriptor,
                    language: language.clone(),
                    numeric_references: prepared.numeric_references.clone(),
                    body: GenericEmbeddedBody {
                        prepared: input.prepared_span(start, end)?,
                        original: input.project_prepared(start, end)?,
                        node_id: native.node_id,
                    },
                });
            }
        }
    }
    Ok(CollectedEmbedded {
        blocks: output,
        observations: raw_observations,
    })
}

#[derive(Clone)]
enum Mapping {
    Direct(ByteSpan),
    Atom(Vec<ByteSpan>),
    Synthetic(SyntheticReason),
}
#[derive(Clone)]
struct Piece {
    prepared: ByteSpan,
    mapping: Mapping,
}

/// Owns transformed text and its mapping storage; the child is consumed inside `with_input`.
pub struct GenericEmbeddedInput<'o, 'request> {
    original_text: &'o str,
    original: &'request SnapshotContext,
    text: String,
    pieces: Vec<Piece>,
    anchors: Vec<(u64, Vec<u64>)>,
    pub diagnostics: Vec<GenericEmbeddedIssue>,
}

fn segment_span(segment: OriginSegment<'_>) -> ByteSpan {
    match segment {
        OriginSegment::Direct { prepared, .. }
        | OriginSegment::MappedAtom { prepared, .. }
        | OriginSegment::Synthetic { prepared, .. } => prepared,
    }
}

fn crop(piece: &Piece, start: u64, end: u64) -> Result<Option<Piece>, GenericEmbeddedIssue> {
    let clipped = ByteSpan {
        start: start.max(piece.prepared.start),
        end: end.min(piece.prepared.end),
    };
    if clipped.start >= clipped.end {
        return Ok(None);
    }
    let mapping = match &piece.mapping {
        Mapping::Direct(original) => Mapping::Direct(ByteSpan {
            start: original.start + clipped.start - piece.prepared.start,
            end: original.start + clipped.end - piece.prepared.start,
        }),
        Mapping::Atom(fragments) => {
            if clipped != piece.prepared {
                return Err(GenericEmbeddedIssue::PartialMappedAtom);
            }
            Mapping::Atom(fragments.clone())
        }
        Mapping::Synthetic(reason) => Mapping::Synthetic(*reason),
    };
    Ok(Some(Piece {
        prepared: clipped,
        mapping,
    }))
}

struct EmbeddedOrigin<'a> {
    segments: Vec<OriginSegment<'a>>,
    removed: Vec<ByteSpan>,
    anchors: Vec<(u64, Vec<u64>)>,
}

impl<'o, 'request> GenericEmbeddedInput<'o, 'request> {
    pub fn prepared_text(&self) -> &str {
        &self.text
    }

    pub fn with_input<R, E>(
        &self,
        consume: impl FnOnce(&PreparedInput<'o, '_, '_, 'request>) -> Result<R, E>,
    ) -> Result<R, E>
    where
        E: From<GenericModelError>,
    {
        let origin = self.origin()?;
        let anchors: Vec<_> = origin
            .anchors
            .iter()
            .map(|(prepared, candidates)| OriginAnchor {
                prepared: *prepared,
                original_candidates: candidates,
            })
            .collect();
        let input = PreparedInput::new(
            self.original_text.as_bytes(),
            self.text.as_bytes(),
            OriginPlan {
                segments: &origin.segments,
                removed_original: &origin.removed,
                anchors: &anchors,
            },
            self.original,
        )
        .map_err(GenericModelError::from)?;
        consume(&input)
    }

    fn origin(&self) -> Result<EmbeddedOrigin<'_>, GenericModelError> {
        self.original.check().map_err(GenericModelError::from)?;
        let mut segments = Vec::new();
        let mut used = Vec::new();
        for piece in &self.pieces {
            let segment = match &piece.mapping {
                Mapping::Direct(original) => {
                    used.push(*original);
                    OriginSegment::Direct {
                        prepared: piece.prepared,
                        original: *original,
                    }
                }
                Mapping::Atom(fragments) => {
                    used.extend(fragments.iter().copied());
                    OriginSegment::MappedAtom {
                        prepared: piece.prepared,
                        original_fragments: fragments,
                    }
                }
                Mapping::Synthetic(reason) => OriginSegment::Synthetic {
                    prepared: piece.prepared,
                    reason: *reason,
                },
            };
            segments.push(segment);
        }
        used.sort_by_key(|span| (span.start, span.end));
        let mut removed = Vec::new();
        let mut cursor = 0;
        for span in used {
            if cursor < span.start {
                removed.push(ByteSpan {
                    start: cursor,
                    end: span.start,
                });
            }
            cursor = cursor.max(span.end);
        }
        let length = u64::try_from(self.original_text.len())
            .map_err(|_| GenericModelError::Origin(OriginError::ArithmeticOverflow))?;
        if cursor < length {
            removed.push(ByteSpan {
                start: cursor,
                end: length,
            });
        }
        let mut normalized_anchors =
            std::collections::BTreeMap::<u64, std::collections::BTreeSet<u64>>::new();
        for (position, candidates) in &self.anchors {
            normalized_anchors
                .entry(*position)
                .or_default()
                .extend(candidates.iter().copied());
        }
        let normalized_anchors: Vec<_> = normalized_anchors
            .into_iter()
            .map(|(position, candidates)| (position, candidates.into_iter().collect::<Vec<_>>()))
            .collect();
        Ok(EmbeddedOrigin {
            segments,
            removed,
            anchors: normalized_anchors,
        })
    }

    fn append_slice(
        &self,
        start: usize,
        end: usize,
        text: &mut String,
        pieces: &mut Vec<Piece>,
    ) -> Result<(), GenericEmbeddedIssue> {
        let offset = text.len() as u64;
        text.push_str(&self.text[start..end]);
        for piece in &self.pieces {
            if let Some(mut clipped) = crop(piece, start as u64, end as u64)? {
                clipped.prepared = ByteSpan {
                    start: offset + clipped.prepared.start - start as u64,
                    end: offset + clipped.prepared.end - start as u64,
                };
                pieces.push(clipped);
            }
        }
        Ok(())
    }

    fn replace_ranges(
        &mut self,
        replacements: &[(usize, usize, String)],
    ) -> Result<(), GenericEmbeddedIssue> {
        let mut text = String::new();
        let mut pieces = Vec::new();
        let mut cursor = 0;
        let mut anchors = Vec::new();
        for (start, end, replacement) in replacements {
            self.append_slice(cursor, *start, &mut text, &mut pieces)?;
            let output_start = text.len() as u64;
            let mut fragments = Vec::new();
            let mut synthetic = Vec::new();
            for piece in &self.pieces {
                if let Some(clipped) = crop(piece, *start as u64, *end as u64)? {
                    match clipped.mapping {
                        Mapping::Direct(original) => fragments.push(original),
                        Mapping::Atom(original) => fragments.extend(original),
                        Mapping::Synthetic(reason) => synthetic.push(reason),
                    }
                }
            }
            if !synthetic.is_empty()
                && (!fragments.is_empty() || synthetic.iter().any(|r| *r != synthetic[0]))
            {
                return Err(GenericEmbeddedIssue::MixedSyntheticReplacement);
            }
            text.push_str(replacement);
            if !replacement.is_empty() {
                let mapping = if fragments.is_empty() {
                    Mapping::Synthetic(
                        synthetic
                            .first()
                            .copied()
                            .unwrap_or(SyntheticReason::InsertedByTransform),
                    )
                } else {
                    Mapping::Atom(fragments)
                };
                pieces.push(Piece {
                    prepared: ByteSpan {
                        start: output_start,
                        end: text.len() as u64,
                    },
                    mapping,
                });
            }
            cursor = *end;
        }
        self.append_slice(cursor, self.text.len(), &mut text, &mut pieces)?;
        // Only carry anchors at unchanged boundaries; replacement interiors have no invented offsets.
        for (position, candidates) in &self.anchors {
            let mut shift = 0i128;
            let mut retained = true;
            for (start, end, replacement) in replacements {
                if *position > *start as u64 && *position < *end as u64 {
                    retained = false;
                    break;
                }
                if *position >= *end as u64 {
                    shift += replacement.len() as i128 - (*end - *start) as i128;
                }
            }
            if retained {
                let position = u64::try_from(*position as i128 + shift)
                    .map_err(|_| GenericEmbeddedIssue::PartialMappedAtom)?;
                anchors.push((position, candidates.clone()));
            }
        }
        self.text = text;
        self.pieces = pieces;
        self.anchors = anchors;
        Ok(())
    }
}

impl<'o, 'v, 'request> ParsedGenericTreeSitterDocument<'_, '_, 'o, 'v, '_, 'request> {
    pub fn embedded_observations(
        &self,
    ) -> Result<&[GenericEmbeddedObservation<'_>], GenericModelError> {
        self.original_context()?;
        Ok(&self.embedded_observations)
    }

    pub fn embedded_blocks(
        &self,
    ) -> Result<&[GenericEmbeddedBlock<'_, 'o, 'v>], GenericModelError> {
        self.original_context()?;
        Ok(&self.embedded)
    }

    pub fn prepare_embedded(
        &self,
        block_index: usize,
    ) -> Result<Result<GenericEmbeddedInput<'o, 'request>, GenericEmbeddedIssue>, GenericModelError>
    {
        let original = self.original_context()?;
        let block = self
            .embedded
            .get(block_index)
            .ok_or(GenericModelError::NativeContract("foreign embedded block"))?;
        if !matches!(block.language, GenericEmbeddedLanguage::Selected(_)) {
            return Ok(Err(GenericEmbeddedIssue::LanguageUnavailable));
        }
        if self.subtree_recovered(self.node_index(block.body.node_id)?)? {
            return Ok(Err(GenericEmbeddedIssue::RecoveredBody));
        }
        let start = block.body.prepared.start() as u64;
        let end = block.body.prepared.end() as u64;
        let plan = self.input.origin_plan();
        let mut pieces = Vec::new();
        for segment in plan.segments {
            original.check()?;
            let piece = Piece {
                prepared: segment_span(*segment),
                mapping: match segment {
                    OriginSegment::Direct { original, .. } => Mapping::Direct(*original),
                    OriginSegment::MappedAtom {
                        original_fragments, ..
                    } => Mapping::Atom(original_fragments.to_vec()),
                    OriginSegment::Synthetic { reason, .. } => Mapping::Synthetic(*reason),
                },
            };
            match crop(&piece, start, end) {
                Ok(Some(mut clipped)) => {
                    clipped.prepared.start -= start;
                    clipped.prepared.end -= start;
                    pieces.push(clipped);
                }
                Ok(None) => (),
                Err(issue) => return Ok(Err(issue)),
            }
        }
        let mut output = GenericEmbeddedInput {
            original_text: self.input.original_text(),
            original,
            text: block.body.prepared.text().to_owned(),
            pieces,
            anchors: plan
                .anchors
                .iter()
                .filter(|anchor| anchor.prepared >= start && anchor.prepared <= end)
                .map(|anchor| (anchor.prepared - start, anchor.original_candidates.to_vec()))
                .collect(),
            diagnostics: Vec::new(),
        };
        for (prepared, parent_offset) in [(0, start), (output.text.len() as u64, end)] {
            let projection = self.input.project_prepared(parent_offset, parent_offset)?;
            let candidates = match projection {
                OriginalProjection::Exact(range) => vec![range.start() as u64],
                OriginalProjection::Ambiguous(candidates) => candidates
                    .iter()
                    .map(|range| range.start() as u64)
                    .collect(),
                _ => Vec::new(),
            };
            if !candidates.is_empty()
                && !output
                    .anchors
                    .iter()
                    .any(|(position, _)| *position == prepared)
            {
                output.anchors.push((prepared, candidates));
            }
        }
        let normalizer_name = block.descriptor.normalize().value();
        if !normalizer_name.is_empty() {
            let Some((_, normalizer)) = self
                .observations()?
                .profile
                .text_normalizers()
                .value()
                .iter()
                .find(|(name, _)| name.value() == normalizer_name)
            else {
                return Ok(Err(GenericEmbeddedIssue::MissingNormalizer(
                    normalizer_name.clone(),
                )));
            };
            let normalizer = normalizer.value();
            let mut replace: Vec<_> = normalizer
                .replace()
                .value()
                .iter()
                .map(|(key, value)| (key.value().as_str(), value.value().as_str()))
                .collect();
            if replace.iter().any(|(key, _)| key.is_empty()) {
                return Ok(Err(GenericEmbeddedIssue::EmptyReplacementKey));
            }
            replace.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(b.0)));
            let mut replacements = Vec::new();
            let mut cursor = 0;
            while cursor < output.text.len() {
                original.check()?;
                if let Some((key, value)) = replace
                    .iter()
                    .find(|(key, _)| output.text[cursor..].starts_with(key))
                {
                    if key != value {
                        replacements.push((cursor, cursor + key.len(), (*value).to_owned()));
                    }
                    cursor += key.len();
                } else {
                    cursor += output.text[cursor..]
                        .chars()
                        .next()
                        .ok_or(GenericModelError::NativeContract("transform cursor"))?
                        .len_utf8();
                }
            }
            if let Err(issue) = output.replace_ranges(&replacements) {
                return Ok(Err(issue));
            }
            if *normalizer.numeric_char_refs().value() {
                let numeric =
                    block
                        .numeric_references
                        .as_ref()
                        .ok_or(GenericModelError::NativeContract(
                            "numeric reference pattern was not prepared",
                        ))?;
                let mut replacements = Vec::new();
                for matched in numeric.find_iter(&output.text) {
                    original.check()?;
                    let value = matched
                        .as_str()
                        .strip_prefix("&#")
                        .and_then(|s| s.strip_suffix(';'))
                        .and_then(|digits| {
                            let (digits, radix) = digits
                                .strip_prefix('x')
                                .or_else(|| digits.strip_prefix('X'))
                                .map_or((digits, 10), |digits| (digits, 16));
                            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                                return None;
                            }
                            u32::from_str_radix(digits, radix)
                                .ok()
                                .and_then(char::from_u32)
                                .filter(|c| !matches!(c, '\r' | '\n'))
                        });
                    match value {
                        Some(value) => {
                            replacements.push((matched.start(), matched.end(), value.to_string()))
                        }
                        None => {
                            output
                                .diagnostics
                                .push(GenericEmbeddedIssue::InvalidNumericReference {
                                    prepared_start: matched.start(),
                                })
                        }
                    }
                }
                if let Err(issue) = output.replace_ranges(&replacements) {
                    return Ok(Err(issue));
                }
            }
        }
        let prefix = block.descriptor.wrap_prefix().value();
        let suffix = block.descriptor.wrap_suffix().value();
        let prefix_length = prefix.len() as u64;
        if !prefix.is_empty() || !suffix.is_empty() {
            for piece in &mut output.pieces {
                piece.prepared.start += prefix_length;
                piece.prepared.end += prefix_length;
            }
            for (position, _) in &mut output.anchors {
                *position += prefix_length;
            }
            if !prefix.is_empty() {
                output.pieces.insert(
                    0,
                    Piece {
                        prepared: ByteSpan {
                            start: 0,
                            end: prefix_length,
                        },
                        mapping: Mapping::Synthetic(SyntheticReason::Wrapper),
                    },
                );
            }
            if !suffix.is_empty() {
                output.pieces.push(Piece {
                    prepared: ByteSpan {
                        start: prefix_length + output.text.len() as u64,
                        end: prefix_length + output.text.len() as u64 + suffix.len() as u64,
                    },
                    mapping: Mapping::Synthetic(SyntheticReason::Wrapper),
                });
            }
            output.text = format!("{prefix}{}{suffix}", output.text);
        }
        original.check()?;
        Ok(Ok(output))
    }
}
