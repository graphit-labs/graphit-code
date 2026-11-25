//! Typed AST profile patches with original source sites and request cancellation.

use crate::scalar_policy::{
    CodecSpan, OriginalByteSpan, ScalarAudit, ScalarLimits, ScalarPolicy, ScalarPolicyError,
    ScalarPolicyErrorCode,
};
use graphit_core::file_io::SnapshotContext;
use serde::{
    Deserialize, Deserializer,
    de::{self, DeserializeOwned, MapAccess, SeqAccess, Visitor},
};
use serde_saphyr::{DuplicateKeyPolicy, MergeKeyPolicy, Spanned};
use std::{fmt, io};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileIdError {
    InvalidLimits,
    InvalidId,
    InputTooLarge,
    AllocationDenied,
}
impl fmt::Display for ProfileIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ProfileIdError {}

pub struct ProfileId {
    text: String,
}
impl ProfileId {
    pub fn new(value: &str, maximum_bytes: usize) -> Result<Self, ProfileIdError> {
        if maximum_bytes == 0 || maximum_bytes > u32::MAX as usize {
            return Err(ProfileIdError::InvalidLimits);
        }
        if value.is_empty() || value.as_bytes().contains(&0) {
            return Err(ProfileIdError::InvalidId);
        }
        if value.len() > maximum_bytes {
            return Err(ProfileIdError::InputTooLarge);
        }
        let mut text = String::new();
        text.try_reserve_exact(value.len())
            .map_err(|_| ProfileIdError::AllocationDenied)?;
        text.push_str(value);
        Ok(Self { text })
    }
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

pub struct ProfileInput<'a> {
    pub id: &'a ProfileId,
    pub bytes: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileLimitError {
    InvalidLimits,
    ArithmeticOverflow,
}
impl fmt::Display for ProfileLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ProfileLimitError {}

pub struct ProfileLimitsInput {
    pub input_bytes: usize,
    pub nodes: usize,
    pub depth: usize,
    pub text_bytes: usize,
    pub scalar_bytes: usize,
    pub collection_entries: usize,
}
pub struct ProfileLimits {
    input: ProfileLimitsInput,
    native_nodes: usize,
    native_events: usize,
}
impl ProfileLimits {
    pub fn new(input: ProfileLimitsInput) -> Result<Self, ProfileLimitError> {
        if [
            input.input_bytes,
            input.nodes,
            input.depth,
            input.text_bytes,
            input.scalar_bytes,
            input.collection_entries,
        ]
        .contains(&0)
            || input.input_bytes > u32::MAX as usize
            || input.collection_entries > u32::MAX as usize
        {
            return Err(ProfileLimitError::InvalidLimits);
        }
        input
            .input_bytes
            .checked_add(1)
            .ok_or(ProfileLimitError::ArithmeticOverflow)?;
        let native_nodes = input
            .nodes
            .checked_mul(2)
            .ok_or(ProfileLimitError::ArithmeticOverflow)?;
        let native_events = input
            .nodes
            .checked_mul(4)
            .ok_or(ProfileLimitError::ArithmeticOverflow)?;
        Ok(Self {
            input,
            native_nodes,
            native_events,
        })
    }
    pub fn input_bytes(&self) -> usize {
        self.input.input_bytes
    }
    pub fn nodes(&self) -> usize {
        self.input.nodes
    }
    pub fn depth(&self) -> usize {
        self.input.depth
    }
    pub fn text_bytes(&self) -> usize {
        self.input.text_bytes
    }
    pub fn scalar_bytes(&self) -> usize {
        self.input.scalar_bytes
    }
    pub fn collection_entries(&self) -> usize {
        self.input.collection_entries
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileErrorCode {
    InvalidLimits,
    InputTooLarge,
    InvalidUtf8,
    Decode,
    QuotaExceeded,
    ArithmeticOverflow,
    AllocationDenied,
    InvalidField,
    InvalidSpan,
    Cancelled,
    DeadlineExceeded,
    ContextDenied,
}
pub struct ProfileError<'a> {
    profile_id: Option<&'a ProfileId>,
    code: ProfileErrorCode,
    span: Option<OriginalByteSpan>,
    schema_path: Option<&'static str>,
    indices: [Option<u32>; 4],
}
impl<'a> ProfileError<'a> {
    pub fn profile_id(&self) -> Option<&'a ProfileId> {
        self.profile_id
    }
    pub fn code(&self) -> ProfileErrorCode {
        self.code
    }
    pub fn span(&self) -> Option<OriginalByteSpan> {
        self.span
    }
    pub fn schema_path(&self) -> Option<&'static str> {
        self.schema_path
    }
    pub fn indices(&self) -> [Option<u32>; 4] {
        self.indices
    }
    fn new(id: &'a ProfileId, code: ProfileErrorCode) -> Self {
        Self {
            profile_id: Some(id),
            code,
            span: None,
            schema_path: None,
            indices: [None; 4],
        }
    }
    fn site(
        mut self,
        span: OriginalByteSpan,
        path: &'static str,
        indices: [Option<u32>; 4],
    ) -> Self {
        self.span = Some(span);
        self.schema_path = Some(path);
        self.indices = indices;
        self
    }
}
impl fmt::Debug for ProfileError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProfileError")
            .field("code", &self.code)
            .field("span", &self.span)
            .field("schema_path", &self.schema_path)
            .field("indices", &self.indices)
            .finish()
    }
}
impl fmt::Display for ProfileError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.code)
    }
}
impl std::error::Error for ProfileError<'_> {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationKind {
    ScalarExact,
    NodeStart,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLocation {
    kind: LocationKind,
    span: OriginalByteSpan,
}
impl SourceLocation {
    pub fn kind(self) -> LocationKind {
        self.kind
    }
    pub fn span(self) -> OriginalByteSpan {
        self.span
    }
}
pub struct Located<T> {
    value: T,
    location: SourceLocation,
}
impl<T> Located<T> {
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn location(&self) -> SourceLocation {
        self.location
    }
}
pub type LocatedText = Located<String>;
pub type LocatedBool = Located<bool>;
impl Located<String> {
    pub fn span(&self) -> OriginalByteSpan {
        self.location.span
    }
}
impl Located<bool> {
    pub fn span(&self) -> OriginalByteSpan {
        self.location.span
    }
}

pub enum Presence<'a, T> {
    Missing,
    Set(&'a Located<T>),
    Clear { span: OriginalByteSpan },
}
enum Field<T> {
    Missing,
    Set(Located<T>),
    Clear(OriginalByteSpan),
}
impl<T> Field<T> {
    fn view(&self) -> Presence<'_, T> {
        match self {
            Self::Missing => Presence::Missing,
            Self::Set(value) => Presence::Set(value),
            Self::Clear(span) => Presence::Clear { span: *span },
        }
    }
}
pub struct List<T> {
    values: Vec<Located<T>>,
}
impl<T> List<T> {
    pub fn as_slice(&self) -> &[Located<T>] {
        &self.values
    }
    pub fn iter(&self) -> std::slice::Iter<'_, Located<T>> {
        self.values.iter()
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}
pub struct PatchMap<T> {
    entries: Vec<(LocatedText, Field<T>)>,
}
impl<T> PatchMap<T> {
    pub fn iter(&self) -> impl Iterator<Item = (&LocatedText, Presence<'_, T>)> {
        self.entries.iter().map(|(key, value)| (key, value.view()))
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileParser {
    TreeSitter,
    Antlr4,
}
/// Native tree selected by a profile query; omission selects the primary parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuerySource {
    Primary,
    CobolPreprocessor,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryKind {
    Entity,
    Relation,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportStrategy {
    CapitalizedName,
    NoPrefix,
    Modifier,
    ExportStatement,
    NoModifier,
    NoStatic,
    None,
}
macro_rules! patch {
    ($name:ident { $($field:ident : $value:ty),* $(,)? }) => {
        pub struct $name { $($field: Field<$value>),* }
        impl $name { $(pub fn $field(&self) -> Presence<'_, $value> { self.$field.view() })* }
    };
}
patch!(CaptureStepPatch {
    xpath: String,
    nearest_descendant: String,
    nearest_ancestor: String
});
patch!(AntlrContextPatch { pattern: String, keyword: String, child_rule: String, negate: bool, label: String, name: List<CaptureStepPatch> });
patch!(QueryPatch {
    select: String, keyword: String, child_rule: String, negate: bool, captures: PatchMap<List<CaptureStepPatch>>,
    source: QuerySource, data_key: String, pattern: String, graph_label: String, name_capture: String,
    relation_type: String, value_capture: String, value_label: String, name_reject: String,
    span_capture: String, declaration_capture: String, context_capture: String, parent_capture: String, parent_label: String, qualifier_capture: String,
    target_fallback: String, r#type: QueryKind, name_is_data: bool, target_labels: List<String>,
});
patch!(ExportPatch { strategy: ExportStrategy, config: PatchMap<String>, config_list: PatchMap<List<String>> });
patch!(TargetRulePatch { labels: List<String>, fallback: String });
patch!(EmbeddedPatch {
    pattern: String, text_capture: String, lang_capture: String, r#default: String,
    wrap_prefix: String, wrap_suffix: String, normalize: String,
    languages: PatchMap<String>, host_labels: List<String>,
});
patch!(TextNormalizerPatch { replace: PatchMap<String>, numeric_char_refs: bool });
patch!(ComplexityPatch { node_types: List<String>, operators: List<String>, head_calls: HeadCallsPatch });
patch!(HeadCallsPatch {
    node_type: String, names: List<String>, pair_names: List<String>, subject_pair_names: List<String>,
});
pub struct ProfilePatch {
    location: SourceLocation,
    language: Field<String>,
    extensions: Field<List<String>>,
    self_keywords: Field<List<String>>,
    anon_func_types: Field<List<String>>,
    declaration_types: Field<List<String>>,
    comment_types: Field<List<String>>,
    embed_labels: Field<List<String>>,
    parser: Field<ProfileParser>,
    grammar: Field<String>,
    start_rule: Field<String>,
    merge: Field<bool>,
    exclusive: Field<bool>,
    queries: Field<List<QueryPatch>>,
    exports: Field<ExportPatch>,
    context_types: Field<PatchMap<String>>,
    context_requires_entity: Field<List<String>>,
    antlr_contexts: Field<List<AntlrContextPatch>>,
    context_name_queries: Field<PatchMap<String>>,
    target_rules: Field<PatchMap<TargetRulePatch>>,
    embedded: Field<List<EmbeddedPatch>>,
    text_normalizers: Field<PatchMap<TextNormalizerPatch>>,
    complexity: Field<ComplexityPatch>,
}
macro_rules! getters { ($($field:ident : $value:ty),* $(,)?) => {
    $(pub fn $field(&self) -> Presence<'_, $value> { self.$field.view() })*
}; }
impl ProfilePatch {
    pub fn location(&self) -> SourceLocation {
        self.location
    }
    getters! {
        language: String, extensions: List<String>, self_keywords: List<String>,
        anon_func_types: List<String>, declaration_types: List<String>, comment_types: List<String>,
        embed_labels: List<String>, parser: ProfileParser, grammar: String, start_rule: String,
        merge: bool, exclusive: bool, queries: List<QueryPatch>, exports: ExportPatch,
        context_types: PatchMap<String>, context_requires_entity: List<String>, antlr_contexts: List<AntlrContextPatch>, context_name_queries: PatchMap<String>,
        target_rules: PatchMap<TargetRulePatch>, embedded: List<EmbeddedPatch>,
        text_normalizers: PatchMap<TextNormalizerPatch>, complexity: ComplexityPatch,
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WireUsage {
    input_bytes: usize,
    nodes: usize,
    depth: usize,
    text_bytes: usize,
    max_scalar_bytes: usize,
    max_collection_entries: usize,
}
impl WireUsage {
    pub fn input_bytes(self) -> usize {
        self.input_bytes
    }
    pub fn nodes(self) -> usize {
        self.nodes
    }
    pub fn depth(self) -> usize {
        self.depth
    }
    pub fn text_bytes(self) -> usize {
        self.text_bytes
    }
    pub fn max_scalar_bytes(self) -> usize {
        self.max_scalar_bytes
    }
    pub fn max_collection_entries(self) -> usize {
        self.max_collection_entries
    }
}
pub struct DecodedProfile {
    id: ProfileId,
    usage: WireUsage,
    patch: ProfilePatch,
}
impl DecodedProfile {
    pub fn id(&self) -> &ProfileId {
        &self.id
    }
    pub fn usage(&self) -> WireUsage {
        self.usage
    }
    pub fn patch(&self) -> &ProfilePatch {
        &self.patch
    }
}

// These temporary DTOs belong to the bounded native wire phase.
#[derive(Deserialize)]
#[serde(transparent)]
struct WText(Option<String>);
macro_rules! scalar_wire {
    ($($name:ident),* $(,)?) => { $(
        struct $name;
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let _: Option<String> = Option::deserialize(d)?;
                Ok(Self)
            }
        }
    )* };
}
scalar_wire!(WParser, WQueryKind, WExportStrategy);
// Preserve the field site for semantic rejection of non-scalar query sources.
struct WQuerySource;
impl<'de> Deserialize<'de> for WQuerySource {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let _ = serde::de::IgnoredAny::deserialize(d)?;
        Ok(Self)
    }
}
#[derive(Deserialize)]
#[serde(transparent)]
struct WList<T> {
    values: Vec<Spanned<T>>,
}
struct WMap<T> {
    entries: Vec<(Spanned<WText>, Spanned<T>)>,
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for WMap<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = WMap<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = access.next_entry::<Spanned<WText>, Spanned<T>>()? {
                    entries.push(entry);
                }
                Ok(WMap { entries })
            }
        }
        d.deserialize_map(V(std::marker::PhantomData))
    }
}

macro_rules! scalar_markers {
    ($marker:expr) => {
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok($marker)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok($marker)
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok($marker)
        }
        fn visit_i128<E: de::Error>(self, _: i128) -> Result<Self::Value, E> {
            Ok($marker)
        }
        fn visit_u128<E: de::Error>(self, _: u128) -> Result<Self::Value, E> {
            Ok($marker)
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok($marker)
        }
        fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Ok($marker)
        }
        fn visit_string<E: de::Error>(self, _: String) -> Result<Self::Value, E> {
            Ok($marker)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok($marker)
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok($marker)
        }
    };
}
macro_rules! any_scalar_wire {
    ($($name:ident),* $(,)?) => { $(
        struct $name;
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = $name;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a profile scalar marker")
                    }
                    scalar_markers!($name);
                }
                d.deserialize_any(V)
            }
        }
    )* };
}
any_scalar_wire!(WBool, WNullableText);
enum WireNullableObject<T> {
    Map(T),
    Scalar,
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for WireNullableObject<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = WireNullableObject<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a profile object or scalar marker")
            }
            scalar_markers!(WireNullableObject::Scalar);
            fn visit_map<M: MapAccess<'de>>(self, access: M) -> Result<Self::Value, M::Error> {
                T::deserialize(de::value::MapAccessDeserializer::new(access))
                    .map(WireNullableObject::Map)
            }
        }
        d.deserialize_any(V(std::marker::PhantomData))
    }
}
enum WireNullableList<T> {
    Seq(Vec<Spanned<T>>),
    Scalar,
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for WireNullableList<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = WireNullableList<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a profile list or scalar marker")
            }
            scalar_markers!(WireNullableList::Scalar);
            fn visit_seq<S: SeqAccess<'de>>(self, access: S) -> Result<Self::Value, S::Error> {
                Vec::<Spanned<T>>::deserialize(de::value::SeqAccessDeserializer::new(access))
                    .map(WireNullableList::Seq)
            }
        }
        d.deserialize_any(V(std::marker::PhantomData))
    }
}
struct WSlot<T> {
    key: Spanned<WText>,
    value: Spanned<T>,
}

macro_rules! wire_schema {
    ($wire:ident { $($field:ident : $value:ty => $key:literal, $path:literal),* $(,)? }) => {
        struct $wire { $($field: Option<WSlot<$value>>),* }
        impl<'de> Deserialize<'de> for $wire {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = $wire;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a closed profile object")
                    }
                    fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
                        $(let mut $field = None;)*
                        while let Some(key) = access.next_key::<Spanned<WText>>()? {
                            match key.value.0.as_deref() {
                                $(Some($key) => {
                                    if $field.is_some() { return Err(de::Error::duplicate_field($key)); }
                                    $field = Some(WSlot { key, value: access.next_value::<Spanned<$value>>()? });
                                })*
                                Some(unknown) => return Err(de::Error::unknown_field(unknown, &[$($key),*])),
                                None => return Err(de::Error::custom("invalid closed profile key")),
                            }
                        }
                        Ok($wire { $($field),* })
                    }
                }
                d.deserialize_map(V)
            }
        }
    };
}
wire_schema!(WProfile {
    language: WText => "language", "language",
    extensions: WList<WText> => "extensions", "extensions",
    self_keywords: WList<WText> => "self_keywords", "self_keywords",
    anon_func_types: WList<WText> => "anon_func_types", "anon_func_types",
    declaration_types: WList<WText> => "declaration_types", "declaration_types",
    comment_types: WList<WText> => "comment_types", "comment_types",
    embed_labels: WList<WText> => "embed_labels", "embed_labels",
    parser: WParser => "parser", "parser",
    grammar: WText => "grammar", "grammar",
    start_rule: WText => "start_rule", "start_rule",
    merge: WBool => "merge", "merge",
    exclusive: WBool => "exclusive", "exclusive",
    queries: WList<WQuery> => "queries", "queries",
    exports: WireNullableObject<WExports> => "exports", "exports",
    context_types: WMap<WNullableText> => "context_types", "context_types",
    context_requires_entity: WList<WText> => "context_requires_entity", "context_requires_entity",
    antlr_contexts: WList<WAntlrContext> => "antlr_contexts", "antlr_contexts",
    context_name_queries: WMap<WNullableText> => "context_name_queries", "context_name_queries",
    target_rules: WMap<WireNullableObject<WTargetRule>> => "target_rules", "target_rules",
    embedded: WList<WEmbedded> => "embedded", "embedded",
    text_normalizers: WMap<WireNullableObject<WNormalizer>> => "text_normalizers", "text_normalizers",
    complexity: WireNullableObject<WComplexity> => "complexity", "complexity",
});
wire_schema!(WCaptureStep {
    xpath: WText => "xpath", "captures.xpath",
    nearest_descendant: WText => "nearest_descendant", "captures.nearest_descendant",
    nearest_ancestor: WText => "nearest_ancestor", "captures.nearest_ancestor",
});
wire_schema!(WAntlrContext {
    pattern: WText => "pattern", "antlr_contexts.pattern",
    keyword: WText => "keyword", "antlr_contexts.keyword",
    child_rule: WText => "child_rule", "antlr_contexts.child_rule",
    negate: WBool => "negate", "antlr_contexts.negate",
    label: WText => "label", "antlr_contexts.label",
    name: WList<WCaptureStep> => "name", "antlr_contexts.name",
});
wire_schema!(WQuery {
    select: WText => "select", "queries.select",
    keyword: WText => "keyword", "queries.keyword",
    child_rule: WText => "child_rule", "queries.child_rule",
    negate: WBool => "negate", "queries.negate",
    captures: WMap<WireNullableList<WCaptureStep>> => "captures", "queries.captures",
    source: WQuerySource => "source", "queries.source",
    data_key: WText => "data_key", "queries.data_key",
    pattern: WText => "pattern", "queries.pattern",
    graph_label: WText => "graph_label", "queries.graph_label",
    name_capture: WText => "name_capture", "queries.name_capture",
    relation_type: WText => "relation_type", "queries.relation_type",
    value_capture: WText => "value_capture", "queries.value_capture",
    value_label: WText => "value_label", "queries.value_label",
    name_reject: WText => "name_reject", "queries.name_reject",
    span_capture: WText => "span_capture", "queries.span_capture",
    declaration_capture: WText => "declaration_capture", "queries.declaration_capture",
    context_capture: WText => "context_capture", "queries.context_capture",
    parent_capture: WText => "parent_capture", "queries.parent_capture",
    parent_label: WText => "parent_label", "queries.parent_label",
    qualifier_capture: WText => "qualifier_capture", "queries.qualifier_capture",
    target_fallback: WText => "target_fallback", "queries.target_fallback",
    r#type: WQueryKind => "type", "queries.type",
    name_is_data: WBool => "name_is_data", "queries.name_is_data",
    target_labels: WList<WText> => "target_labels", "queries.target_labels",
});
wire_schema!(WExports {
    strategy: WExportStrategy => "strategy", "exports.strategy",
    config: WMap<WNullableText> => "config", "exports.config",
    config_list: WMap<WireNullableList<WText>> => "config_list", "exports.config_list",
});
wire_schema!(WTargetRule {
    labels: WList<WText> => "labels", "target_rules.labels",
    fallback: WText => "fallback", "target_rules.fallback",
});
wire_schema!(WEmbedded {
    pattern: WText => "pattern", "embedded.pattern",
    text_capture: WText => "text_capture", "embedded.text_capture",
    lang_capture: WText => "lang_capture", "embedded.lang_capture",
    r#default: WText => "default", "embedded.default",
    wrap_prefix: WText => "wrap_prefix", "embedded.wrap_prefix",
    wrap_suffix: WText => "wrap_suffix", "embedded.wrap_suffix",
    normalize: WText => "normalize", "embedded.normalize",
    languages: WMap<WNullableText> => "languages", "embedded.languages",
    host_labels: WList<WText> => "host_labels", "embedded.host_labels",
});
wire_schema!(WNormalizer {
    replace: WMap<WNullableText> => "replace", "text_normalizers.replace",
    numeric_char_refs: WBool => "numeric_char_refs", "text_normalizers.numeric_char_refs",
});
wire_schema!(WComplexity {
    node_types: WList<WText> => "node_types", "complexity.node_types",
    operators: WList<WText> => "operators", "complexity.operators",
    head_calls: WireNullableObject<WHeadCalls> => "head_calls", "complexity.head_calls",
});
wire_schema!(WHeadCalls {
    node_type: WText => "node_type", "complexity.head_calls.node_type",
    names: WList<WText> => "names", "complexity.head_calls.names",
    pair_names: WList<WText> => "pair_names", "complexity.head_calls.pair_names",
    subject_pair_names: WList<WText> => "subject_pair_names", "complexity.head_calls.subject_pair_names",
});

fn scalar_code(code: ScalarPolicyErrorCode) -> ProfileErrorCode {
    match code {
        ScalarPolicyErrorCode::InvalidLimits | ScalarPolicyErrorCode::RegexPolicyBuild => {
            ProfileErrorCode::InvalidLimits
        }
        ScalarPolicyErrorCode::InputTooLarge => ProfileErrorCode::InputTooLarge,
        ScalarPolicyErrorCode::InvalidUtf8 => ProfileErrorCode::InvalidUtf8,
        ScalarPolicyErrorCode::QuotaExceeded => ProfileErrorCode::QuotaExceeded,
        ScalarPolicyErrorCode::WrongScalarKind => ProfileErrorCode::InvalidField,
        ScalarPolicyErrorCode::InvalidSpan
        | ScalarPolicyErrorCode::MissingScalarSpan
        | ScalarPolicyErrorCode::AmbiguousScalarSpan => ProfileErrorCode::InvalidSpan,
        ScalarPolicyErrorCode::Parse
        | ScalarPolicyErrorCode::UnsupportedTag
        | ScalarPolicyErrorCode::ForbiddenAnchorAlias
        | ScalarPolicyErrorCode::InvalidScalar => ProfileErrorCode::Decode,
    }
}
fn check_context<'a>(
    id: &'a ProfileId,
    original: &SnapshotContext,
) -> Result<(), ProfileError<'a>> {
    original.check().map_err(|cause| {
        ProfileError::new(
            id,
            match cause.kind() {
                io::ErrorKind::Interrupted => ProfileErrorCode::Cancelled,
                io::ErrorKind::TimedOut => ProfileErrorCode::DeadlineExceeded,
                _ => ProfileErrorCode::ContextDenied,
            },
        )
    })
}
type Indices = [Option<u32>; 4];
struct Walk<'w, 'a> {
    id: &'a ProfileId,
    audit: &'w ScalarAudit<'a>,
    limits: &'w ProfileLimits,
    original: &'w SnapshotContext,
    usage: WireUsage,
}
impl<'w, 'a> Walk<'w, 'a> {
    fn error(
        &self,
        code: ProfileErrorCode,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> ProfileError<'a> {
        ProfileError::new(self.id, code).site(at.span, path, indices)
    }
    fn scalar_error(
        &self,
        cause: ScalarPolicyError,
        path: &'static str,
        indices: Indices,
    ) -> ProfileError<'a> {
        let mut error = ProfileError::new(self.id, scalar_code(cause.code));
        error.span = cause.span;
        error.schema_path = Some(path);
        error.indices = indices;
        error
    }
    fn check(&self) -> Result<(), ProfileError<'a>> {
        check_context(self.id, self.original)
    }
    fn site<T>(
        &mut self,
        wire: &Spanned<T>,
        scalar: bool,
        path: &'static str,
        indices: Indices,
    ) -> Result<SourceLocation, ProfileError<'a>> {
        self.check()?;
        let span = wire.defined.span();
        let codec = CodecSpan::from_parts(span.byte_offset(), span.byte_len())
            .map_err(|cause| self.scalar_error(cause, path, indices))?;
        let span = self
            .audit
            .original_span(codec)
            .map_err(|cause| self.scalar_error(cause, path, indices))?;
        self.check()?;
        Ok(SourceLocation {
            kind: if scalar {
                LocationKind::ScalarExact
            } else {
                LocationKind::NodeStart
            },
            span,
        })
    }
    fn text(
        &mut self,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<&'w str, ProfileError<'a>> {
        self.check()?;
        let audit = self.audit;
        let value = audit.text_at(at.span);
        self.check()?;
        value.map_err(|cause| self.scalar_error(cause, path, indices))
    }
    fn boolean(
        &mut self,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<bool, ProfileError<'a>> {
        self.check()?;
        let value = self.audit.bool_at(at.span);
        self.check()?;
        value.map_err(|cause| self.scalar_error(cause, path, indices))
    }
    fn null(
        &mut self,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        self.check()?;
        let value = self.audit.null_at(at.span);
        self.check()?;
        value.map_err(|cause| self.scalar_error(cause, path, indices))
    }
    fn nullable_text(
        &mut self,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<Option<&'w str>, ProfileError<'a>> {
        match self.text(at, path, indices) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.code == ProfileErrorCode::InvalidField => {
                self.null(at, path, indices)?;
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
    fn node(
        &mut self,
        depth: usize,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        self.check()?;
        let nodes =
            self.usage.nodes.checked_add(1).ok_or_else(|| {
                self.error(ProfileErrorCode::ArithmeticOverflow, at, path, indices)
            })?;
        if nodes > self.limits.nodes() || depth > self.limits.depth() {
            return Err(self.error(ProfileErrorCode::QuotaExceeded, at, path, indices));
        }
        self.usage.nodes = nodes;
        self.usage.depth = self.usage.depth.max(depth);
        Ok(())
    }
    fn text_size(
        &mut self,
        text: &str,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        self.check()?;
        let total = self
            .usage
            .text_bytes
            .checked_add(text.len())
            .ok_or_else(|| self.error(ProfileErrorCode::ArithmeticOverflow, at, path, indices))?;
        if total > self.limits.text_bytes() || text.len() > self.limits.scalar_bytes() {
            return Err(self.error(ProfileErrorCode::QuotaExceeded, at, path, indices));
        }
        self.usage.text_bytes = total;
        self.usage.max_scalar_bytes = self.usage.max_scalar_bytes.max(text.len());
        Ok(())
    }
    fn collection(
        &mut self,
        entries: usize,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        self.check()?;
        if entries > self.limits.collection_entries() {
            return Err(self.error(ProfileErrorCode::QuotaExceeded, at, path, indices));
        }
        self.usage.max_collection_entries = self.usage.max_collection_entries.max(entries);
        Ok(())
    }
    fn child_depth(&mut self, depth: usize) -> Result<usize, ProfileError<'a>> {
        self.check()?;
        depth
            .checked_add(1)
            .ok_or_else(|| ProfileError::new(self.id, ProfileErrorCode::ArithmeticOverflow))
    }
    fn index(&mut self, mut indices: Indices, index: usize) -> Result<Indices, ProfileError<'a>> {
        let index = u32::try_from(index)
            .map_err(|_| ProfileError::new(self.id, ProfileErrorCode::ArithmeticOverflow))?;
        for slot in &mut indices {
            self.check()?;
            if slot.is_none() {
                *slot = Some(index);
                return Ok(indices);
            }
        }
        Err(ProfileError::new(self.id, ProfileErrorCode::InvalidField))
    }
    fn string(&mut self, text: &str) -> Result<String, ProfileError<'a>> {
        self.check()?;
        let mut owned = String::new();
        owned
            .try_reserve_exact(text.len())
            .map_err(|_| ProfileError::new(self.id, ProfileErrorCode::AllocationDenied))?;
        self.check()?;
        owned.push_str(text);
        self.check()?;
        Ok(owned)
    }
    fn vector<T>(&mut self, entries: usize) -> Result<Vec<T>, ProfileError<'a>> {
        self.check()?;
        let mut vector = Vec::new();
        vector
            .try_reserve_exact(entries)
            .map_err(|_| ProfileError::new(self.id, ProfileErrorCode::AllocationDenied))?;
        self.check()?;
        Ok(vector)
    }
    fn key(
        &mut self,
        key: &Spanned<WText>,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        let at = self.site(key, true, path, indices)?;
        let text = self.text(at, path, indices)?;
        self.text_size(text, at, path, indices)
    }
    fn required(
        &mut self,
        slot: &Option<WSlot<WText>>,
        parent: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        self.check()?;
        let Some(slot) = slot else {
            return Err(self.error(ProfileErrorCode::InvalidField, parent, path, indices));
        };
        let at = self.site(&slot.value, true, path, indices)?;
        let text = self.text(at, path, indices)?;
        self.check()?;
        if text.is_empty() {
            return Err(self.error(ProfileErrorCode::InvalidField, at, path, indices));
        }
        Ok(())
    }
    fn routing(
        &mut self,
        slot: &Option<WSlot<WText>>,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        self.check()?;
        if let Some(slot) = slot {
            let at = self.site(&slot.value, true, path, indices)?;
            let text = self.text(at, path, indices)?;
            self.check()?;
            if text.as_bytes().contains(&0)
                || text.as_bytes().first().is_some_and(u8::is_ascii_whitespace)
                || text.as_bytes().last().is_some_and(u8::is_ascii_whitespace)
            {
                return Err(self.error(ProfileErrorCode::InvalidField, at, path, indices));
            }
        }
        Ok(())
    }
}

trait WNode: DeserializeOwned {
    type Output;
    fn scalar(&self) -> bool;
    fn validate<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        depth: usize,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>>;
    fn build<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<Field<Self::Output>, ProfileError<'a>>;
}
impl WNode for WText {
    type Output = String;
    fn scalar(&self) -> bool {
        true
    }
    fn validate<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        depth: usize,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        w.node(depth, at, path, indices)?;
        let text = w.text(at, path, indices)?;
        w.text_size(text, at, path, indices)
    }
    fn build<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<Field<String>, ProfileError<'a>> {
        let text = w.text(at, path, indices)?;
        Ok(Field::Set(Located {
            value: w.string(text)?,
            location: at,
        }))
    }
}
impl WNode for WBool {
    type Output = bool;
    fn scalar(&self) -> bool {
        true
    }
    fn validate<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        depth: usize,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        w.node(depth, at, path, indices)?;
        w.boolean(at, path, indices)?;
        Ok(())
    }
    fn build<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<Field<bool>, ProfileError<'a>> {
        Ok(Field::Set(Located {
            value: w.boolean(at, path, indices)?,
            location: at,
        }))
    }
}
macro_rules! enum_node {
    ($wire:ident, $output:ident, {$($text:literal => $variant:ident),* $(,)?}) => {
        impl WNode for $wire {
            type Output = $output;
            fn scalar(&self) -> bool { true }
            fn validate<'a>(&self, w: &mut Walk<'_, 'a>, at: SourceLocation, depth: usize,
                path: &'static str, indices: Indices) -> Result<(), ProfileError<'a>> {
                w.node(depth, at, path, indices)?;
                let text = w.text(at, path, indices)?; w.text_size(text, at, path, indices)?;
                w.check()?;
                match text { $($text => Ok(()),)* _ => Err(w.error(ProfileErrorCode::InvalidField, at, path, indices)) }
            }
            fn build<'a>(&self, w: &mut Walk<'_, 'a>, at: SourceLocation,
                path: &'static str, indices: Indices) -> Result<Field<$output>, ProfileError<'a>> {
                let text = w.text(at, path, indices)?; w.check()?;
                let value = match text { $($text => $output::$variant,)*
                    _ => return Err(w.error(ProfileErrorCode::InvalidField, at, path, indices)),
                };
                Ok(Field::Set(Located { value, location: at }))
            }
        }
    };
}
enum_node!(WParser, ProfileParser, {"tree-sitter" => TreeSitter, "antlr4" => Antlr4});
enum_node!(WQuerySource, QuerySource, {"primary" => Primary, "cobol-preprocessor" => CobolPreprocessor});
enum_node!(WQueryKind, QueryKind, {"entity" => Entity, "relation" => Relation});
enum_node!(WExportStrategy, ExportStrategy, {
    "capitalized_name" => CapitalizedName, "no_prefix" => NoPrefix, "modifier" => Modifier,
    "export_statement" => ExportStatement, "no_modifier" => NoModifier,
    "no_static" => NoStatic, "none" => None,
});
impl WNode for WNullableText {
    type Output = String;
    fn scalar(&self) -> bool {
        true
    }
    fn validate<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        depth: usize,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        w.node(depth, at, path, indices)?;
        if let Some(text) = w.nullable_text(at, path, indices)? {
            w.text_size(text, at, path, indices)?;
        }
        Ok(())
    }
    fn build<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<Field<String>, ProfileError<'a>> {
        match w.nullable_text(at, path, indices)? {
            Some(text) => Ok(Field::Set(Located {
                value: w.string(text)?,
                location: at,
            })),
            None => Ok(Field::Clear(at.span)),
        }
    }
}

fn validate_list<'a, T: WNode>(
    values: &[Spanned<T>],
    w: &mut Walk<'_, 'a>,
    at: SourceLocation,
    depth: usize,
    path: &'static str,
    indices: Indices,
) -> Result<(), ProfileError<'a>> {
    w.node(depth, at, path, indices)?;
    w.collection(values.len(), at, path, indices)?;
    let child_depth = w.child_depth(depth)?;
    for (index, value) in values.iter().enumerate() {
        w.check()?;
        let indices = w.index(indices, index)?;
        let at = w.site(value, value.value.scalar(), path, indices)?;
        value.value.validate(w, at, child_depth, path, indices)?;
    }
    Ok(())
}
fn build_list<'a, T: WNode>(
    values: &[Spanned<T>],
    w: &mut Walk<'_, 'a>,
    at: SourceLocation,
    path: &'static str,
    indices: Indices,
) -> Result<Field<List<T::Output>>, ProfileError<'a>> {
    let mut output = w.vector(values.len())?;
    for (index, value) in values.iter().enumerate() {
        w.check()?;
        let indices = w.index(indices, index)?;
        let site = w.site(value, value.value.scalar(), path, indices)?;
        match value.value.build(w, site, path, indices)? {
            Field::Set(value) => {
                w.check()?;
                output.push(value);
            }
            _ => return Err(w.error(ProfileErrorCode::InvalidField, site, path, indices)),
        }
    }
    Ok(Field::Set(Located {
        value: List { values: output },
        location: at,
    }))
}
impl<T: WNode> WNode for WList<T> {
    type Output = List<T::Output>;
    fn scalar(&self) -> bool {
        false
    }
    fn validate<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        depth: usize,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        validate_list(&self.values, w, at, depth, path, indices)
    }
    fn build<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<Field<Self::Output>, ProfileError<'a>> {
        build_list(&self.values, w, at, path, indices)
    }
}
impl<T: WNode> WNode for WireNullableList<T> {
    type Output = List<T::Output>;
    fn scalar(&self) -> bool {
        matches!(self, Self::Scalar)
    }
    fn validate<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        depth: usize,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        match self {
            Self::Seq(values) => validate_list(values, w, at, depth, path, indices),
            Self::Scalar => {
                w.node(depth, at, path, indices)?;
                w.null(at, path, indices)
            }
        }
    }
    fn build<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<Field<Self::Output>, ProfileError<'a>> {
        match self {
            Self::Seq(values) => build_list(values, w, at, path, indices),
            Self::Scalar => {
                w.null(at, path, indices)?;
                Ok(Field::Clear(at.span))
            }
        }
    }
}
impl<T: WNode> WNode for WireNullableObject<T> {
    type Output = T::Output;
    fn scalar(&self) -> bool {
        matches!(self, Self::Scalar)
    }
    fn validate<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        depth: usize,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        match self {
            Self::Map(value) => value.validate(w, at, depth, path, indices),
            Self::Scalar => {
                w.node(depth, at, path, indices)?;
                w.null(at, path, indices)
            }
        }
    }
    fn build<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<Field<Self::Output>, ProfileError<'a>> {
        match self {
            Self::Map(value) => value.build(w, at, path, indices),
            Self::Scalar => {
                w.null(at, path, indices)?;
                Ok(Field::Clear(at.span))
            }
        }
    }
}
impl<T: WNode> WNode for WMap<T> {
    type Output = PatchMap<T::Output>;
    fn scalar(&self) -> bool {
        false
    }
    fn validate<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        depth: usize,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        w.node(depth, at, path, indices)?;
        w.collection(self.entries.len(), at, path, indices)?;
        let child_depth = w.child_depth(depth)?;
        for (index, (key, value)) in self.entries.iter().enumerate() {
            w.check()?;
            let indices = w.index(indices, index)?;
            w.key(key, path, indices)?;
            let at = w.site(value, value.value.scalar(), path, indices)?;
            value.value.validate(w, at, child_depth, path, indices)?;
        }
        Ok(())
    }
    fn build<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<Field<Self::Output>, ProfileError<'a>> {
        let mut entries = w.vector(self.entries.len())?;
        for (index, (key, value)) in self.entries.iter().enumerate() {
            w.check()?;
            let indices = w.index(indices, index)?;
            let key_at = w.site(key, true, path, indices)?;
            let text = w.text(key_at, path, indices)?;
            let key = Located {
                value: w.string(text)?,
                location: key_at,
            };
            let value_at = w.site(value, value.value.scalar(), path, indices)?;
            let value = value.value.build(w, value_at, path, indices)?;
            w.check()?;
            entries.push((key, value));
        }
        Ok(Field::Set(Located {
            value: PatchMap { entries },
            location: at,
        }))
    }
}

macro_rules! object_rules {
    (plain, $value:expr, $walk:expr, $at:expr, $indices:expr) => {};
    (rules, $value:expr, $walk:expr, $at:expr, $indices:expr) => {
        $value.rules($walk, $at, $indices)?;
    };
}
macro_rules! object_output {
    (root, $patch:ident, $at:expr, {$($field:ident : $value:expr),*}) => {
        $patch { location: $at, $($field: $value),* }
    };
    (nested, $patch:ident, $at:expr, {$($field:ident : $value:expr),*}) => { $patch { $($field: $value),* } };
}
macro_rules! object_node {
    ($wire:ident, $patch:ident, $root:ident, $rules:ident, {$($field:ident => $path:literal),* $(,)?}) => {
        impl WNode for $wire {
            type Output = $patch;
            fn scalar(&self) -> bool { false }
            fn validate<'a>(&self, w: &mut Walk<'_, 'a>, at: SourceLocation, depth: usize,
                path: &'static str, indices: Indices) -> Result<(), ProfileError<'a>> {
                w.node(depth, at, path, indices)?;
                let count = 0 $(+ usize::from(self.$field.is_some()))*;
                w.collection(count, at, path, indices)?;
                let child_depth = w.child_depth(depth)?;
                $({
                    w.check()?;
                    if let Some(slot) = &self.$field {
                        w.key(&slot.key, $path, indices)?;
                        let site = w.site(&slot.value, slot.value.value.scalar(), $path, indices)?;
                        slot.value.value.validate(w, site, child_depth, $path, indices)?;
                    }
                })*
                object_rules!($rules, self, w, at, indices);
                Ok(())
            }
            fn build<'a>(&self, w: &mut Walk<'_, 'a>, at: SourceLocation,
                _path: &'static str, indices: Indices) -> Result<Field<$patch>, ProfileError<'a>> {
                $(let $field = {
                    w.check()?;
                    match &self.$field {
                        None => Field::Missing,
                        Some(slot) => {
                            let site = w.site(&slot.value, slot.value.value.scalar(), $path, indices)?;
                            slot.value.value.build(w, site, $path, indices)?
                        }
                    }
                };)*
                Ok(Field::Set(Located {
                    value: object_output!($root, $patch, at, {$($field: $field),*}), location: at,
                }))
            }
        }
    };
}
object_node!(WProfile, ProfilePatch, root, rules, {
    language => "language", extensions => "extensions", self_keywords => "self_keywords",
    anon_func_types => "anon_func_types", declaration_types => "declaration_types",
    comment_types => "comment_types", embed_labels => "embed_labels", parser => "parser",
    grammar => "grammar", start_rule => "start_rule", merge => "merge", exclusive => "exclusive",
    queries => "queries", exports => "exports", context_types => "context_types",
    context_requires_entity => "context_requires_entity",
    antlr_contexts => "antlr_contexts", context_name_queries => "context_name_queries", target_rules => "target_rules", embedded => "embedded",
    text_normalizers => "text_normalizers", complexity => "complexity",
});
object_node!(WCaptureStep, CaptureStepPatch, nested, rules, {
    xpath => "captures.xpath", nearest_descendant => "captures.nearest_descendant", nearest_ancestor => "captures.nearest_ancestor",
});
object_node!(WAntlrContext, AntlrContextPatch, nested, rules, {
    pattern => "antlr_contexts.pattern", keyword => "antlr_contexts.keyword", child_rule => "antlr_contexts.child_rule", negate => "antlr_contexts.negate", label => "antlr_contexts.label", name => "antlr_contexts.name",
});
object_node!(WQuery, QueryPatch, nested, rules, {
    select => "queries.select", keyword => "queries.keyword", child_rule => "queries.child_rule", negate => "queries.negate", captures => "queries.captures",
    source => "queries.source",
    data_key => "queries.data_key", pattern => "queries.pattern", graph_label => "queries.graph_label",
    name_capture => "queries.name_capture", relation_type => "queries.relation_type",
    value_capture => "queries.value_capture", value_label => "queries.value_label",
    name_reject => "queries.name_reject", span_capture => "queries.span_capture",
    declaration_capture => "queries.declaration_capture",
    context_capture => "queries.context_capture",
    parent_capture => "queries.parent_capture", parent_label => "queries.parent_label",
    qualifier_capture => "queries.qualifier_capture", target_fallback => "queries.target_fallback",
    r#type => "queries.type", name_is_data => "queries.name_is_data", target_labels => "queries.target_labels",
});
object_node!(WExports, ExportPatch, nested, plain, {
    strategy => "exports.strategy", config => "exports.config", config_list => "exports.config_list",
});
object_node!(WTargetRule, TargetRulePatch, nested, plain, {
    labels => "target_rules.labels", fallback => "target_rules.fallback",
});
object_node!(WEmbedded, EmbeddedPatch, nested, rules, {
    pattern => "embedded.pattern", text_capture => "embedded.text_capture",
    lang_capture => "embedded.lang_capture", r#default => "embedded.default",
    wrap_prefix => "embedded.wrap_prefix", wrap_suffix => "embedded.wrap_suffix",
    normalize => "embedded.normalize", languages => "embedded.languages", host_labels => "embedded.host_labels",
});
object_node!(WNormalizer, TextNormalizerPatch, nested, rules, {
    replace => "text_normalizers.replace", numeric_char_refs => "text_normalizers.numeric_char_refs",
});
object_node!(WComplexity, ComplexityPatch, nested, plain, {
    node_types => "complexity.node_types", operators => "complexity.operators", head_calls => "complexity.head_calls",
});
object_node!(WHeadCalls, HeadCallsPatch, nested, rules, {
    node_type => "complexity.head_calls.node_type", names => "complexity.head_calls.names",
    pair_names => "complexity.head_calls.pair_names", subject_pair_names => "complexity.head_calls.subject_pair_names",
});

impl WProfile {
    fn rules<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        w.required(&self.language, at, "language", indices)?;
        w.routing(&self.language, "language", indices)?;
        w.routing(&self.grammar, "grammar", indices)
    }
}
impl<'w, 'a> Walk<'w, 'a> {
    fn nonempty(
        &mut self,
        slot: &Option<WSlot<WText>>,
        path: &'static str,
        indices: Indices,
    ) -> Result<bool, ProfileError<'a>> {
        self.check()?;
        let Some(slot) = slot else {
            return Ok(false);
        };
        let site = self.site(&slot.value, true, path, indices)?;
        let text = self.text(site, path, indices)?;
        self.check()?;
        Ok(!text.is_empty())
    }
    fn pair(
        &mut self,
        left: &Option<WSlot<WText>>,
        right: &Option<WSlot<WText>>,
        parent: SourceLocation,
        path: &'static str,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        if self.nonempty(left, path, indices)? != self.nonempty(right, path, indices)? {
            let site = match left.as_ref().or(right.as_ref()) {
                Some(slot) => self.site(&slot.value, true, path, indices)?,
                None => parent,
            };
            return Err(self.error(ProfileErrorCode::InvalidField, site, path, indices));
        }
        Ok(())
    }
}
impl WCaptureStep {
    fn rules<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        let count = usize::from(w.nonempty(&self.xpath, "captures.xpath", indices)?)
            + usize::from(w.nonempty(
                &self.nearest_descendant,
                "captures.nearest_descendant",
                indices,
            )?)
            + usize::from(w.nonempty(
                &self.nearest_ancestor,
                "captures.nearest_ancestor",
                indices,
            )?);
        if count != 1 {
            return Err(w.error(ProfileErrorCode::InvalidField, at, "captures", indices));
        }
        Ok(())
    }
}
impl WAntlrContext {
    fn rules<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        w.required(&self.pattern, at, "antlr_contexts.pattern", indices)?;
        if w.nonempty(&self.keyword, "antlr_contexts.keyword", indices)?
            && w.nonempty(&self.child_rule, "antlr_contexts.child_rule", indices)?
        {
            return Err(w.error(
                ProfileErrorCode::InvalidField,
                at,
                "antlr_contexts",
                indices,
            ));
        }
        Ok(())
    }
}
impl WQuery {
    fn rules<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        w.required(&self.data_key, at, "queries.data_key", indices)?;
        w.required(&self.pattern, at, "queries.pattern", indices)?;
        let relation = if let Some(slot) = &self.r#type {
            let site = w.site(&slot.value, true, "queries.type", indices)?;
            let text = w.text(site, "queries.type", indices)?;
            w.check()?;
            text == "relation"
        } else {
            false
        };
        if relation {
            w.required(&self.relation_type, at, "queries.relation_type", indices)?;
        } else {
            w.required(&self.graph_label, at, "queries.graph_label", indices)?;
        }
        if let Some(captures) = &self.captures {
            for (key, _) in &captures.value.value.entries {
                w.check()?;
                if !matches!(
                    key.value.0.as_deref(),
                    Some("name" | "value" | "span" | "parent" | "qualifier")
                ) {
                    let site = w.site(key, true, "queries.captures", indices)?;
                    return Err(w.error(
                        ProfileErrorCode::InvalidField,
                        site,
                        "queries.captures",
                        indices,
                    ));
                }
            }
            for (slot, label, path) in [
                ("value", &self.value_label, "queries.value_label"),
                ("parent", &self.parent_label, "queries.parent_label"),
            ] {
                let configured = captures.value.value.entries.iter().any(|(key, value)| {
                    key.value.0.as_deref() == Some(slot)
                        && matches!(value.value, WireNullableList::Seq(_))
                });
                if configured != w.nonempty(label, path, indices)? {
                    return Err(w.error(ProfileErrorCode::InvalidField, at, path, indices));
                }
            }
            Ok(())
        } else {
            w.pair(
                &self.value_capture,
                &self.value_label,
                at,
                "queries.value_capture",
                indices,
            )?;
            w.pair(
                &self.parent_capture,
                &self.parent_label,
                at,
                "queries.parent_capture",
                indices,
            )
        }
    }
}
impl WEmbedded {
    fn rules<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        w.required(&self.pattern, at, "embedded.pattern", indices)?;
        w.required(&self.text_capture, at, "embedded.text_capture", indices)?;
        if let Some(slot) = &self.languages {
            for (index, (key, _)) in slot.value.value.entries.iter().enumerate() {
                w.check()?;
                let indices_at = w.index(indices, index)?;
                let site = w.site(key, true, "embedded.languages", indices_at)?;
                let text = w.text(site, "embedded.languages", indices_at)?;
                for (other_key, _) in &slot.value.value.entries[..index] {
                    w.check()?;
                    let other_at = w.site(other_key, true, "embedded.languages", indices_at)?;
                    let other = w.text(other_at, "embedded.languages", indices_at)?;
                    w.check()?;
                    if text.eq_ignore_ascii_case(other) {
                        return Err(w.error(
                            ProfileErrorCode::InvalidField,
                            site,
                            "embedded.languages",
                            indices_at,
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
impl WNormalizer {
    fn rules<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        _at: SourceLocation,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        if let Some(slot) = &self.replace {
            for (index, (key, _)) in slot.value.value.entries.iter().enumerate() {
                w.check()?;
                let indices = w.index(indices, index)?;
                let at = w.site(key, true, "text_normalizers.replace", indices)?;
                let text = w.text(at, "text_normalizers.replace", indices)?;
                w.check()?;
                if text.is_empty() {
                    return Err(w.error(
                        ProfileErrorCode::InvalidField,
                        at,
                        "text_normalizers.replace",
                        indices,
                    ));
                }
            }
        }
        Ok(())
    }
}
impl WHeadCalls {
    fn rules<'a>(
        &self,
        w: &mut Walk<'_, 'a>,
        at: SourceLocation,
        indices: Indices,
    ) -> Result<(), ProfileError<'a>> {
        w.required(
            &self.node_type,
            at,
            "complexity.head_calls.node_type",
            indices,
        )
    }
}

fn native_options(limits: &ProfileLimits) -> serde_saphyr::Options {
    let input = limits.input_bytes();
    let depth = limits.depth();
    let budget = serde_saphyr::budget! {
        max_reader_input_bytes: Some(input), max_events: limits.native_events, max_nodes: limits.native_nodes,
        max_depth: depth, flow_nesting_limit: depth, max_documents: 1,
        max_aliases: 0, max_anchors: 0, max_recorded_anchor_events: 0, max_recorded_anchor_bytes: 0,
        max_total_scalar_bytes: input, max_total_comment_bytes: input, max_merge_keys: 0,
        max_inclusion_depth: 0, simple_key_max_lookahead: input,
    };
    let alias_limits = serde_saphyr::alias_limits! {
        max_total_replayed_events: 0, max_replay_stack_depth: 0, max_alias_expansions_per_anchor: 0,
    };
    serde_saphyr::options! {
        budget: budget, alias_limits: alias_limits, merge_keys: MergeKeyPolicy::Error,
        duplicate_keys: DuplicateKeyPolicy::Error, strict_booleans: true, reject_unsupported_tags: true,
        reject_non_finite_typeless_float: true, emit_comments: false, with_snippet: false,
    }
}
fn native_error<'a>(
    id: &'a ProfileId,
    audit: &ScalarAudit<'_>,
    cause: &serde_saphyr::Error,
) -> ProfileError<'a> {
    let cause = cause.without_snippet();
    let code = match cause {
        serde_saphyr::Error::Budget { .. } | serde_saphyr::Error::MultipleDocuments { .. } => {
            ProfileErrorCode::QuotaExceeded
        }
        serde_saphyr::Error::InvalidOptions { .. } => ProfileErrorCode::InvalidLimits,
        serde_saphyr::Error::InvalidUtf8Input => ProfileErrorCode::InvalidUtf8,
        _ => ProfileErrorCode::Decode,
    };
    let metadata = cause
        .location()
        .and_then(|location| {
            let span = location.span();
            native_byte_metadata(span.byte_offset(), span.byte_len())
        })
        .or_else(|| match cause {
            serde_saphyr::Error::ExternalMessage { source, .. } => match source.as_ref() {
                serde_saphyr::ExternalMessageSource::Parser(scan) => scan
                    .marker()
                    .byte_offset()
                    .and_then(|offset| u64::try_from(offset).ok())
                    .map(|offset| Ok(CodecSpan::new(offset, 0))),
                _ => None,
            },
            _ => None,
        });
    let mut error = ProfileError::new(id, code);
    match metadata {
        None => error,
        Some(Ok(codec)) => match audit.original_span(codec) {
            Ok(span) => {
                error.span = Some(span);
                error
            }
            Err(cause) => {
                error.code = ProfileErrorCode::InvalidSpan;
                error.span = cause.span;
                error
            }
        },
        Some(Err(cause)) => {
            error.code = ProfileErrorCode::InvalidSpan;
            error.span = cause.span;
            error
        }
    }
}

fn native_byte_metadata(
    offset: Option<u64>,
    length: Option<u64>,
) -> Option<Result<CodecSpan, ScalarPolicyError>> {
    match (offset, length) {
        (None, None) => None,
        _ => Some(CodecSpan::from_parts(offset, length)),
    }
}

pub struct ProfileDecoder {
    policy: ScalarPolicy,
}
impl ProfileDecoder {
    pub fn new(policy: ScalarPolicy) -> Self {
        Self { policy }
    }
    pub fn decode<'a>(
        &self,
        input: ProfileInput<'a>,
        limits: &ProfileLimits,
        original: &SnapshotContext,
    ) -> Result<DecodedProfile, ProfileError<'a>> {
        let id = input.id;
        check_context(id, original)?;
        if input.bytes.len() > limits.input_bytes() {
            return Err(ProfileError::new(id, ProfileErrorCode::InputTooLarge));
        }
        let text = std::str::from_utf8(input.bytes)
            .map_err(|_| ProfileError::new(id, ProfileErrorCode::InvalidUtf8))?;
        check_context(id, original)?;
        let scalar_limits = ScalarLimits::new(
            limits.input_bytes(),
            limits.depth(),
            limits.native_events,
            limits.native_nodes,
            limits.input_bytes(),
            limits.input_bytes(),
            limits.collection_entries(),
        )
        .map_err(|cause| ProfileError::new(id, scalar_code(cause.code)))?;
        check_context(id, original)?;
        let audited = self.policy.audit(input.bytes, &scalar_limits);
        check_context(id, original)?;
        let audit = audited.map_err(|cause| {
            let mut error = ProfileError::new(id, scalar_code(cause.code));
            error.span = cause.span;
            error
        })?;
        // The typed parser receives the exact same audited input, including BOMs and CRLF.
        if !std::ptr::eq(audit.input(), input.bytes) {
            return Err(ProfileError::new(id, ProfileErrorCode::InvalidSpan));
        }
        check_context(id, original)?;
        let typed =
            serde_saphyr::from_str_with_options::<Spanned<WProfile>>(text, native_options(limits));
        check_context(id, original)?;
        let wire = typed.map_err(|cause| native_error(id, &audit, &cause))?;
        let mut walk = Walk {
            id,
            audit: &audit,
            limits,
            original,
            usage: WireUsage {
                input_bytes: input.bytes.len(),
                nodes: 0,
                depth: 0,
                text_bytes: 0,
                max_scalar_bytes: 0,
                max_collection_entries: 0,
            },
        };
        let at = walk.site(&wire, false, "profile", [None; 4])?;
        wire.value
            .validate(&mut walk, at, 1, "profile", [None; 4])?;
        let usage = walk.usage;
        // All semantic quotas and schema checks have completed before any retained copy.
        let owned_id = ProfileId {
            text: walk.string(id.as_str())?,
        };
        let patch = match wire.value.build(&mut walk, at, "profile", [None; 4])? {
            Field::Set(value) => value.value,
            _ => return Err(ProfileError::new(id, ProfileErrorCode::InvalidField)),
        };
        walk.check()?;
        Ok(DecodedProfile {
            id: owned_id,
            usage,
            patch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar_policy::RegexLimits;
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};

    fn context() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(120),
            Cancellation::default(),
        )
    }

    fn policy() -> ScalarPolicy {
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })
        .unwrap()
    }
    fn limits() -> ProfileLimits {
        ProfileLimits::new(ProfileLimitsInput {
            input_bytes: 1024,
            nodes: 128,
            depth: 8,
            text_bytes: 1024,
            scalar_bytes: 1024,
            collection_entries: 32,
        })
        .unwrap()
    }

    #[test]
    fn original_context_cancel_precedes_deadline() {
        let cancellation = Cancellation::default();
        let original = SnapshotContext::new(
            Instant::now() - Duration::from_secs(1),
            cancellation.clone(),
        );
        let id = ProfileId::new("context", 64).unwrap();
        assert_eq!(
            check_context(&id, &original).unwrap_err().code(),
            ProfileErrorCode::DeadlineExceeded
        );
        cancellation.cancel();
        assert_eq!(
            check_context(&id, &original).unwrap_err().code(),
            ProfileErrorCode::Cancelled
        );
    }

    #[test]
    fn actual_vector_capacity_overflow_is_typed() {
        let original = context();
        let input = b"language: x\n";
        let policy = policy();
        let audit = policy
            .audit(
                input,
                &ScalarLimits::new(1024, 8, 512, 256, 1024, 1024, 32).unwrap(),
            )
            .unwrap();
        let id = ProfileId::new("allocation-boundary", 64).unwrap();
        let limits = limits();
        let mut walk = Walk {
            id: &id,
            audit: &audit,
            limits: &limits,
            original: &original,
            usage: WireUsage {
                input_bytes: input.len(),
                nodes: 0,
                depth: 0,
                text_bytes: 0,
                max_scalar_bytes: 0,
                max_collection_entries: 0,
            },
        };
        // Non-ZST Vec capacity above isize::MAX is rejected before allocator invocation.
        assert_eq!(
            walk.vector::<u8>(usize::MAX).err().unwrap().code(),
            ProfileErrorCode::AllocationDenied
        );
    }

    #[test]
    fn absent_error_metadata_differs_from_partial_and_invalid_byte_ranges() {
        assert!(native_byte_metadata(None, None).is_none());
        for (offset, length) in [(Some(0), None), (None, Some(0))] {
            assert_eq!(
                native_byte_metadata(offset, length)
                    .unwrap()
                    .unwrap_err()
                    .code,
                ScalarPolicyErrorCode::MissingScalarSpan
            );
        }
        let input = "language: α\n".as_bytes();
        let policy = policy();
        let audit = policy
            .audit(
                input,
                &ScalarLimits::new(1024, 8, 512, 256, 1024, 1024, 32).unwrap(),
            )
            .unwrap();
        assert!(
            audit
                .original_span(
                    native_byte_metadata(Some(input.len() as u64), Some(0))
                        .unwrap()
                        .unwrap()
                )
                .is_ok()
        );
        for codec in [
            CodecSpan::new(u64::MAX, 1),
            CodecSpan::new(input.len() as u64 + 1, 0),
            CodecSpan::new(11, 0),
        ] {
            assert_eq!(
                audit.original_span(codec).unwrap_err().code,
                ScalarPolicyErrorCode::InvalidSpan
            );
        }
    }
}
