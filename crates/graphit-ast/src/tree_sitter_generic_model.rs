//! Profile-driven semantics over retained native Tree-sitter observations.

use crate::{
    RangeError, SourceRange, SourceText,
    catalog::EffectiveProfile,
    entity_extent::EntityExtentError,
    prepared_input::{OriginError, OriginalProjection, PreparedInput},
    profiles::ProfileParser,
    symbols::{IdentityDocument, IdentityDocumentInput, IdentityError, IdentityLimits},
    tree_sitter_context::{ContextError, ContextualTreeSitterDocument, PreparedTreeSitterContexts},
    tree_sitter_profile::{CaptureBinding, TreeSitterProfileMatches},
};
use graphit_core::file_io::SnapshotContext;
use std::{collections::HashMap, error::Error, fmt, io, sync::Arc};

mod exports;
mod relations;
pub use exports::GenericExport;
pub use relations::{GenericRelation, GenericRelationTarget};
mod projections;
pub use projections::{GenericCodeProjection, GenericProjectionIssue};
mod embedded;
pub use embedded::{
    GenericEmbeddedBlock, GenericEmbeddedBody, GenericEmbeddedInput, GenericEmbeddedIssue,
    GenericEmbeddedLanguage, GenericEmbeddedObservation,
};
mod entities;
pub use entities::{
    GenericContribution, GenericEntity, GenericEntityDiagnostic, GenericEntityIssue,
    GenericOccurrence, GenericOwnerFrame, GenericSemanticDocument, GenericSyntaxRecovery,
};
mod contexts;
mod declarations;
mod extents;
mod names;
pub use contexts::GenericOwnedContext;
pub use declarations::GenericDeclaration;
pub use extents::GenericExtent;
mod policies;
pub use names::{GenericName, GenericNameIssue, GenericNameRepresentation};
pub use policies::{GenericComplexity, GenericComplexityDecision, GenericDecisionRule};

#[derive(Debug)]
pub enum GenericModelError {
    Operation(io::Error),
    Context(ContextError),
    Origin(OriginError),
    Identity(IdentityError),
    Extent(EntityExtentError),
    Range(RangeError),
    Regex(regex::Error),
    Engine(crate::tree_sitter_engine::EngineError),
    Profile(crate::tree_sitter_profile::TreeSitterProfileError),
    InvalidEmbeddedRole {
        embedded_index: usize,
        role: &'static str,
        capture: String,
    },
    ReservedProfile,
    InvalidRole {
        query_index: usize,
        role: &'static str,
        capture: String,
    },
    InvalidPolicy {
        field: &'static str,
        reason: &'static str,
    },
    NativeContract(&'static str),
}

impl fmt::Display for GenericModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(e) => e.fmt(f),
            Self::Context(e) => e.fmt(f),
            Self::Origin(e) => e.fmt(f),
            Self::Identity(e) => e.fmt(f),
            Self::Extent(e) => e.fmt(f),
            Self::Range(e) => e.fmt(f),
            Self::Regex(e) => e.fmt(f),
            Self::Engine(e) => e.fmt(f),
            Self::Profile(e) => e.fmt(f),
            Self::InvalidEmbeddedRole {
                embedded_index,
                role,
                capture,
            } => write!(
                f,
                "embedded {embedded_index} {role} capture @{capture} is not declared"
            ),
            Self::ReservedProfile => f.write_str("profile requires its specialized semantic model"),
            Self::InvalidRole {
                query_index,
                role,
                capture,
            } => write!(
                f,
                "query {query_index} {role} capture @{capture} is not declared"
            ),
            Self::InvalidPolicy { field, reason } => write!(f, "invalid {field}: {reason}"),
            Self::NativeContract(reason) => f.write_str(reason),
        }
    }
}

impl Error for GenericModelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Context(e) => Some(e),
            Self::Origin(e) => Some(e),
            Self::Identity(e) => Some(e),
            Self::Extent(e) => Some(e),
            Self::Range(e) => Some(e),
            Self::Regex(e) => Some(e),
            Self::Engine(e) => Some(e),
            Self::Profile(e) => Some(e),
            _ => None,
        }
    }
}

macro_rules! from_error {
    ($ty:ty, $variant:ident) => {
        impl From<$ty> for GenericModelError {
            fn from(error: $ty) -> Self {
                Self::$variant(error)
            }
        }
    };
}
from_error!(io::Error, Operation);
from_error!(ContextError, Context);
from_error!(OriginError, Origin);
from_error!(IdentityError, Identity);
from_error!(EntityExtentError, Extent);
from_error!(RangeError, Range);
from_error!(regex::Error, Regex);
from_error!(crate::tree_sitter_engine::EngineError, Engine);
from_error!(crate::tree_sitter_profile::TreeSitterProfileError, Profile);

fn generic_profile(profile: &EffectiveProfile) -> Result<(), GenericModelError> {
    if *profile.parser().value() != ProfileParser::TreeSitter
        || [
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
        .any(|language| profile.language().value().eq_ignore_ascii_case(language))
    {
        return Err(GenericModelError::ReservedProfile);
    }
    Ok(())
}

/// Reusable compiled queries and predicates; preparation retains no request context.
pub struct PreparedGenericTreeSitterModel<'profile> {
    profile: &'profile EffectiveProfile,
    contexts: PreparedTreeSitterContexts<'profile>,
    name_reject: Arc<[Option<regex::Regex>]>,
    upper_name: Option<Arc<regex::Regex>>,
    embedded: Arc<[embedded::PreparedEmbedded<'profile>]>,
}

impl<'profile> PreparedGenericTreeSitterModel<'profile> {
    pub fn prepare(
        profile: &'profile EffectiveProfile,
        contexts: PreparedTreeSitterContexts<'profile>,
        original: &SnapshotContext,
    ) -> Result<Self, GenericModelError> {
        original.check()?;
        let result = (|| {
            generic_profile(profile)?;
            for kind in profile.context_requires_entity().value().iter() {
                original.check()?;
                if !profile
                    .context_types()
                    .value()
                    .iter()
                    .any(|(key, _)| key.value() == kind.value())
                {
                    return Err(GenericModelError::InvalidPolicy {
                        field: "context_requires_entity",
                        reason: "required native kind has no context_types entry",
                    });
                }
            }
            for (query_index, roles) in contexts.prepared_profile().query_roles() {
                original.check()?;
                for (role, capture) in [
                    ("name", Some(roles.name)),
                    ("value", roles.value),
                    ("span", roles.span),
                    ("declaration", roles.declaration),
                    ("context", roles.context),
                    ("parent", roles.parent),
                    ("qualifier", roles.qualifier),
                ] {
                    if let Some(capture) = capture
                        && capture.capture_index.is_none()
                    {
                        return Err(GenericModelError::InvalidRole {
                            query_index,
                            role,
                            capture: capture.name.to_owned(),
                        });
                    }
                }
            }
            if *profile.exports().value().strategy().value()
                == crate::profiles::ExportStrategy::Modifier
                && !profile
                    .exports()
                    .value()
                    .config()
                    .value()
                    .iter()
                    .any(|(key, value)| key.value() == "keyword" && !value.value().is_empty())
            {
                return Err(GenericModelError::InvalidPolicy {
                    field: "exports.config.keyword",
                    reason: "modifier requires a nonempty native keyword",
                });
            }
            let mut name_reject = Vec::new();
            for query in profile.queries().value().iter() {
                original.check()?;
                let pattern = query.value().name_reject().value();
                name_reject.push(if pattern.is_empty() {
                    None
                } else {
                    Some(regex::Regex::new(pattern)?)
                });
            }
            let upper_name = (*profile.exports().value().strategy().value()
                == crate::profiles::ExportStrategy::CapitalizedName)
                .then(|| regex::Regex::new(r"^\p{Lu}").map(Arc::new))
                .transpose()?;
            let embedded = embedded::prepare(profile, contexts.prepared_profile(), original)?;
            Ok(Self {
                embedded,
                upper_name,
                profile,
                contexts,
                name_reject: name_reject.into(),
            })
        })();
        original.check()?;
        result
    }

    pub fn collect<'a, 'o, 'v, 'p, 'request>(
        &self,
        input: &'a PreparedInput<'o, 'v, 'p, 'request>,
        relative_components: &'o [&'o str],
        limits: IdentityLimits,
    ) -> Result<
        ParsedGenericTreeSitterDocument<'a, 'profile, 'o, 'v, 'p, 'request>,
        GenericModelError,
    >
    where
        'profile: 'o,
    {
        let original = input.original_context();
        original.check()?;
        let result =
            (|| {
                let context = self.contexts.collect(input)?;
                let observations = context.observations()?;
                if !std::ptr::eq(observations.profile, self.profile)
                    || observations.groups.len() != self.name_reject.len()
                {
                    return Err(GenericModelError::NativeContract(
                        "generic factory profile mismatch",
                    ));
                }
                let source =
                    SourceText::new(input.original_text().as_bytes(), limits.source_bytes())?;
                let identity = IdentityDocument::new(
                    IdentityDocumentInput {
                        relative_components,
                        language: self.profile.language().value(),
                        grammar: self.profile.grammar().value(),
                        original_bytes: input.original_text().as_bytes(),
                    },
                    limits,
                    original,
                )?;
                let mut node_indices = HashMap::new();
                let mut children: Vec<Vec<usize>> = vec![Vec::new(); observations.nodes.len()];
                for (index, node) in observations.nodes.iter().enumerate() {
                    original.check()?;
                    if node_indices.insert(node.node_id, index).is_some() {
                        return Err(GenericModelError::NativeContract(
                            "duplicate native node identity",
                        ));
                    }
                    if let Some(parent) = node.parent {
                        let ancestor = observations.nodes.get(parent).ok_or(
                            GenericModelError::NativeContract("native parent is outside the tree"),
                        )?;
                        if parent >= index
                            || ancestor.range.start > node.range.start
                            || ancestor.range.end < node.range.end
                        {
                            return Err(GenericModelError::NativeContract(
                                "native ancestry is inconsistent",
                            ));
                        }
                        children[parent].push(index);
                    }
                }
                let embedded = embedded::collect(&self.embedded, observations, input)?;
                Ok(ParsedGenericTreeSitterDocument {
                    embedded: embedded.blocks,
                    embedded_observations: embedded.observations,
                    context,
                    identity,
                    source,
                    input,
                    node_indices,
                    children,
                    name_reject: Arc::clone(&self.name_reject),
                    upper_name: self.upper_name.clone(),
                })
            })();
        original.check()?;
        result
    }
}

/// The native tree and input mapping stay alive for all semantic consumers.
pub struct ParsedGenericTreeSitterDocument<'a, 'profile, 'o, 'v, 'p, 'request> {
    context: ContextualTreeSitterDocument<'profile, 'o, 'v, 'request>,
    identity: IdentityDocument<'o, 'request>,
    source: SourceText<'o>,
    input: &'a PreparedInput<'o, 'v, 'p, 'request>,
    node_indices: HashMap<usize, usize>,
    children: Vec<Vec<usize>>,
    name_reject: Arc<[Option<regex::Regex>]>,
    upper_name: Option<Arc<regex::Regex>>,
    embedded: Vec<GenericEmbeddedBlock<'profile, 'o, 'v>>,
    embedded_observations: Vec<GenericEmbeddedObservation<'profile>>,
}

impl<'profile, 'o, 'v, 'request>
    ParsedGenericTreeSitterDocument<'_, 'profile, 'o, 'v, '_, 'request>
{
    pub fn observations(
        &self,
    ) -> Result<&TreeSitterProfileMatches<'profile, 'o, 'v>, GenericModelError> {
        Ok(self.context.observations()?)
    }

    pub fn identity_document(&self) -> Result<&IdentityDocument<'o, 'request>, GenericModelError> {
        self.original_context()?;
        Ok(&self.identity)
    }

    pub fn source(&self) -> Result<SourceText<'o>, GenericModelError> {
        self.original_context()?;
        Ok(self.source.clone())
    }

    pub fn original_context(&self) -> Result<&'request SnapshotContext, GenericModelError> {
        let original = self.input.original_context();
        original.check()?;
        Ok(original)
    }

    pub fn node_index(&self, native_id: usize) -> Result<usize, GenericModelError> {
        self.original_context()?;
        self.node_indices
            .get(&native_id)
            .copied()
            .ok_or(GenericModelError::NativeContract(
                "capture is outside the retained tree",
            ))
    }

    pub fn node_origin(&self, index: usize) -> Result<OriginalProjection<'o>, GenericModelError> {
        let node =
            self.observations()?
                .nodes
                .get(index)
                .ok_or(GenericModelError::NativeContract(
                    "native node index is outside the tree",
                ))?;
        self.input
            .project_prepared(
                u64::try_from(node.range.start)
                    .map_err(|_| GenericModelError::NativeContract("node offset overflow"))?,
                u64::try_from(node.range.end)
                    .map_err(|_| GenericModelError::NativeContract("node offset overflow"))?,
            )
            .map_err(Into::into)
    }

    /// A semantic name can only come from a slot bound to the configured name role.
    pub fn capture_name(
        &self,
        query_index: usize,
        match_index: usize,
        capture_index: usize,
    ) -> Result<Result<GenericName<'o>, GenericNameIssue>, GenericModelError> {
        let group = self
            .observations()?
            .groups
            .get(query_index)
            .ok_or(GenericModelError::NativeContract("foreign query index"))?;
        let matched = group
            .matches
            .get(match_index)
            .ok_or(GenericModelError::NativeContract("foreign query match"))?;
        let CaptureBinding::Present(slots) = &matched.bindings.name else {
            return Err(GenericModelError::NativeContract(
                "match has no bound name role",
            ));
        };
        if !slots.contains(&capture_index) {
            return Err(GenericModelError::NativeContract(
                "capture does not bind the name role",
            ));
        }
        let capture = matched
            .captures
            .get(capture_index)
            .ok_or(GenericModelError::NativeContract("foreign capture slot"))?;
        let OriginalProjection::Exact(raw) = &capture.original else {
            return Ok(Err(GenericNameIssue::NonExact));
        };
        let node_index = self.node_index(capture.native.node_id)?;
        let mut stack = vec![node_index];
        while let Some(index) = stack.pop() {
            self.original_context()?;
            let node = &self.observations()?.nodes[index];
            if node.error || node.missing {
                return Ok(Err(GenericNameIssue::Recovery));
            }
            stack.extend(self.children[index].iter().copied());
        }
        if self
            .name_reject
            .get(query_index)
            .ok_or(GenericModelError::NativeContract("missing query predicate"))?
            .as_ref()
            .is_some_and(|predicate| predicate.is_match(raw.text()))
        {
            return Ok(Err(GenericNameIssue::Rejected));
        }
        let query = group.query.value();
        let data = *query.name_is_data().value()
            || !query.value_capture().value().is_empty()
            || !query.parent_capture().value().is_empty();
        names::name_view(&self.source, *raw, data)
    }
}
