//! Declarative profile overlays and selection over the real profile decoder.

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};
    fn context() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(120),
            Cancellation::default(),
        )
    }
    fn limits_input() -> CatalogLimitsInput {
        CatalogLimitsInput {
            policy_entries: 8,
            selector_bytes: 128,
        }
    }

    #[test]
    fn domain_arithmetic_overflow_is_typed() {
        assert_eq!(
            plus(usize::MAX, 1).unwrap_err().code(),
            CatalogErrorCode::ArithmeticOverflow
        );
        assert_eq!(
            times(usize::MAX, 2).unwrap_err().code(),
            CatalogErrorCode::ArithmeticOverflow
        );
    }

    #[test]
    fn ordered_removals_preserve_declared_query_and_map_order() {
        let ctx = context();
        let walk = Walk { original: &ctx };
        let mut queries: Vec<Planned<QueryPlan<'_>>> = (0..4)
            .map(|_| Planned::default(QueryPlan::empty()))
            .collect();
        walk.remove_ordered(&mut queries, 1).unwrap();
        walk.remove_ordered(&mut queries, 0).unwrap();
        assert_eq!(queries.len(), 2);
        let mut entries = vec![
            (
                Planned::default(Text::one("a")),
                Planned::default(Text::one("1")),
            ),
            (
                Planned::default(Text::one("b")),
                Planned::default(Text::one("2")),
            ),
            (
                Planned::default(Text::one("c")),
                Planned::default(Text::one("3")),
            ),
        ];
        let removed = walk.remove_ordered(&mut entries, 0).unwrap();
        assert_eq!(removed.0.value.first, "a");
        assert_eq!(
            entries.iter().map(|e| e.0.value.first).collect::<Vec<_>>(),
            ["b", "c"]
        );
    }

    #[test]
    fn invalid_ordered_remove_keeps_vector_unchanged() {
        let ctx = context();
        let walk = Walk { original: &ctx };
        let mut values = vec![10_u64, 20, 30];
        assert_eq!(
            walk.remove_ordered(&mut values, usize::MAX)
                .unwrap_err()
                .code(),
            CatalogErrorCode::ArithmeticOverflow
        );
        assert_eq!(values, [10, 20, 30]);
        assert_eq!(walk.remove_ordered(&mut values, 2).unwrap(), 30);
        assert_eq!(values, [10, 20]);
    }

    #[test]
    fn all_limit_fields_require_positive_values() {
        for field in 0..2 {
            let mut limits = limits_input();
            match field {
                0 => limits.policy_entries = 0,
                1 => limits.selector_bytes = 0,
                _ => unreachable!(),
            }
            assert_eq!(
                CatalogLimits::new(limits).unwrap_err().code(),
                CatalogErrorCode::InvalidLimits
            );
        }
        let mut over = limits_input();
        over.policy_entries = u32::MAX as usize;
        if let Some(value) = over.policy_entries.checked_add(1) {
            over.policy_entries = value;
            assert_eq!(
                CatalogLimits::new(over).unwrap_err().code(),
                CatalogErrorCode::InvalidLimits
            );
        }
    }
}

use crate::profiles::*;
use crate::scalar_policy::OriginalByteSpan;
use graphit_core::file_io::SnapshotContext;
use std::mem::size_of;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogErrorCode {
    InvalidLimits,
    QuotaExceeded,
    ArithmeticOverflow,
    AllocationDenied,
    InvalidLayer,
    DuplicateLanguage,
    DuplicateGrammar,
    AmbiguousSelector,
    InvalidPolicy,
    UnknownGrammar,
    DisabledGrammar,
    InvalidSelector,
    Cancelled,
    DeadlineExceeded,
    ContextDenied,
}
#[derive(Clone, Copy, Debug)]
pub struct CatalogError {
    code: CatalogErrorCode,
    site: Option<SourceSite>,
    conflicting_site: Option<SourceSite>,
    layer_index: Option<u32>,
    profile_index: Option<u32>,
    schema_path: Option<&'static str>,
}
impl CatalogError {
    fn new(code: CatalogErrorCode) -> Self {
        Self {
            code,
            site: None,
            conflicting_site: None,
            layer_index: None,
            profile_index: None,
            schema_path: None,
        }
    }
    pub fn code(self) -> CatalogErrorCode {
        self.code
    }
    pub fn site(self) -> Option<SourceSite> {
        self.site
    }
    pub fn conflicting_site(self) -> Option<SourceSite> {
        self.conflicting_site
    }
    pub fn layer_index(self) -> Option<u32> {
        self.layer_index
    }
    pub fn profile_index(self) -> Option<u32> {
        self.profile_index
    }
    pub fn schema_path(self) -> Option<&'static str> {
        self.schema_path
    }
}
impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.code)
    }
}
impl std::error::Error for CatalogError {}
fn overflow() -> CatalogError {
    CatalogError::new(CatalogErrorCode::ArithmeticOverflow)
}
fn plus(a: usize, b: usize) -> Result<usize, CatalogError> {
    a.checked_add(b).ok_or_else(overflow)
}
fn times(a: usize, b: usize) -> Result<usize, CatalogError> {
    a.checked_mul(b).ok_or_else(overflow)
}
fn bounded(value: usize, cap: usize) -> Result<(), CatalogError> {
    if value > cap {
        Err(CatalogError::new(CatalogErrorCode::QuotaExceeded))
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CatalogLimitsInput {
    pub policy_entries: usize,
    pub selector_bytes: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct CatalogLimits {
    values: CatalogLimitsInput,
}
impl CatalogLimits {
    pub fn new(values: CatalogLimitsInput) -> Result<Self, CatalogError> {
        if values.policy_entries == 0
            || values.selector_bytes == 0
            || u32::try_from(values.policy_entries).is_err()
        {
            return Err(CatalogError::new(CatalogErrorCode::InvalidLimits));
        }
        times(values.policy_entries, size_of::<String>())?;
        Ok(Self { values })
    }

    pub fn policy_entries(self) -> usize {
        self.values.policy_entries
    }
    pub fn selector_bytes(self) -> usize {
        self.values.selector_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceIndex(u32);
impl SourceIndex {
    pub fn as_u32(self) -> u32 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSite {
    source: SourceIndex,
    location: SourceLocation,
}
impl SourceSite {
    pub fn source_index(self) -> SourceIndex {
        self.source
    }
    pub fn location(self) -> SourceLocation {
        self.location
    }
    pub fn span(self) -> OriginalByteSpan {
        self.location.span()
    }
}
fn site(source: SourceIndex, location: SourceLocation) -> SourceSite {
    SourceSite { source, location }
}
enum Provenance {
    Default,
    Declared(SourceSite),
    Derived(Vec<SourceSite>),
}
#[derive(Clone, Copy, Debug)]
pub enum ProvenanceView<'a> {
    Default,
    Declared(SourceSite),
    Derived(&'a [SourceSite]),
}
impl Provenance {
    fn view(&self) -> ProvenanceView<'_> {
        match self {
            Self::Default => ProvenanceView::Default,
            Self::Declared(s) => ProvenanceView::Declared(*s),
            Self::Derived(s) => ProvenanceView::Derived(s),
        }
    }
    fn first(&self) -> Option<SourceSite> {
        match self {
            Self::Default => None,
            Self::Declared(s) => Some(*s),
            Self::Derived(s) => s.first().copied(),
        }
    }
}
pub struct Effective<T> {
    value: T,
    provenance: Provenance,
}
impl<T> Effective<T> {
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn provenance(&self) -> ProvenanceView<'_> {
        self.provenance.view()
    }
}
pub type EffectiveText = Effective<String>;
pub type EffectiveBool = Effective<bool>;
pub struct EffectiveList<T> {
    values: Vec<T>,
}
impl<T> EffectiveList<T> {
    pub fn as_slice(&self) -> &[T] {
        &self.values
    }
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.values.iter()
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}
pub struct EffectiveMap<T> {
    entries: Vec<(EffectiveText, Effective<T>)>,
}
impl<T> EffectiveMap<T> {
    pub fn iter(&self) -> impl Iterator<Item = (&EffectiveText, &Effective<T>)> {
        self.entries.iter().map(|(key, value)| (key, value))
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

struct Walk<'a> {
    original: &'a SnapshotContext,
}
impl Walk<'_> {
    fn check(&self) -> Result<(), CatalogError> {
        self.original.check().map_err(|cause| {
            CatalogError::new(match cause.kind() {
                std::io::ErrorKind::Interrupted => CatalogErrorCode::Cancelled,
                std::io::ErrorKind::TimedOut => CatalogErrorCode::DeadlineExceeded,
                _ => CatalogErrorCode::ContextDenied,
            })
        })
    }
    fn reserve<T>(&self, values: &mut Vec<T>, additional: usize) -> Result<(), CatalogError> {
        self.check()?;
        values
            .try_reserve_exact(additional)
            .map_err(|_| CatalogError::new(CatalogErrorCode::AllocationDenied))
    }
    fn push<T>(&self, values: &mut Vec<T>, value: T) -> Result<(), CatalogError> {
        self.reserve(values, 1)?;
        values.push(value);
        Ok(())
    }
    fn remove_ordered<T>(&self, values: &mut Vec<T>, index: usize) -> Result<T, CatalogError> {
        self.check()?;
        if plus(index, 1)? > values.len() {
            return Err(unexpected());
        }
        let removed = values.remove(index);
        self.check()?;
        Ok(removed)
    }
    fn equal(&self, a: Text<'_>, b: Text<'_>, folded: bool) -> Result<bool, CatalogError> {
        self.check()?;
        let equal = if folded {
            a.bytes()
                .map(|v| v.to_ascii_lowercase())
                .eq(b.bytes().map(|v| v.to_ascii_lowercase()))
        } else {
            a.bytes().eq(b.bytes())
        };
        self.check()?;
        Ok(equal)
    }
    fn copy_text(&self, text: Text<'_>) -> Result<String, CatalogError> {
        let length = text.len()?;
        self.check()?;
        let mut value = String::new();
        value
            .try_reserve_exact(length)
            .map_err(|_| CatalogError::new(CatalogErrorCode::AllocationDenied))?;
        value.push_str(text.first);
        value.push_str(text.second);
        self.check()?;
        Ok(value)
    }
}
#[derive(Clone, Copy)]
struct Text<'a> {
    first: &'a str,
    second: &'a str,
}
impl<'a> Text<'a> {
    fn one(value: &'a str) -> Self {
        Self {
            first: value,
            second: "",
        }
    }
    fn len(self) -> Result<usize, CatalogError> {
        plus(self.first.len(), self.second.len())
    }
    fn bytes(self) -> impl Iterator<Item = u8> + 'a {
        self.first.bytes().chain(self.second.bytes())
    }
    fn bare(self) -> Self {
        if self.second.is_empty() {
            for prefix in ["tree-sitter-", "antlr-"] {
                if self
                    .first
                    .get(..prefix.len())
                    .is_some_and(|v| v.eq_ignore_ascii_case(prefix))
                {
                    return Self::one(&self.first[prefix.len()..]);
                }
            }
            self
        } else if self.first == "tree-sitter-" || self.first == "antlr-" {
            Self::one(self.second)
        } else {
            self
        }
    }
}
struct Planned<T> {
    value: T,
    provenance: Provenance,
}
impl<T> Planned<T> {
    fn default(value: T) -> Self {
        Self {
            value,
            provenance: Provenance::Default,
        }
    }
    fn declared(value: T, source: SourceSite) -> Self {
        Self {
            value,
            provenance: Provenance::Declared(source),
        }
    }
}
struct PlanList<T> {
    values: Vec<T>,
}
impl<T> PlanList<T> {
    fn empty() -> Self {
        Self { values: Vec::new() }
    }
}
// Dynamic keys borrow their actual captured profile. No source text is copied into a plan.
struct BorrowedMap<'a, T> {
    entries: Vec<(Planned<Text<'a>>, Planned<T>)>,
}
impl<T> BorrowedMap<'_, T> {
    fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}
trait Materialize {
    type Output;

    fn materialize(self, walk: &Walk<'_>) -> Result<Self::Output, CatalogError>;
}
impl Materialize for Text<'_> {
    type Output = String;
    fn materialize(self, walk: &Walk<'_>) -> Result<String, CatalogError> {
        walk.copy_text(self)
    }
}
impl<T: Materialize> Materialize for Planned<T> {
    type Output = Effective<T::Output>;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        w.check()?;
        Ok(Effective {
            value: self.value.materialize(w)?,
            provenance: self.provenance,
        })
    }
}
impl<T: Materialize> Materialize for PlanList<T> {
    type Output = EffectiveList<T::Output>;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        let mut values = Vec::new();
        w.reserve(&mut values, self.values.len())?;
        for value in self.values {
            w.check()?;
            values.push(value.materialize(w)?);
        }
        Ok(EffectiveList { values })
    }
}
impl<T: Materialize> Materialize for BorrowedMap<'_, T> {
    type Output = EffectiveMap<T::Output>;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        let mut entries = Vec::new();
        w.reserve(&mut entries, self.entries.len())?;
        for (key, value) in self.entries {
            w.check()?;
            entries.push((key.materialize(w)?, value.materialize(w)?));
        }
        Ok(EffectiveMap { entries })
    }
}
impl<T: Materialize> Materialize for Option<T> {
    type Output = Option<T::Output>;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        self.map(|value| value.materialize(w)).transpose()
    }
}
macro_rules! copied_value {
    ($($ty:ty),+ $(,)?) => {$(
        impl Materialize for $ty {
            type Output = Self;
            fn materialize(self, w: &Walk<'_>) -> Result<Self, CatalogError> {
                w.check()?;
                Ok(self)
            }
        }
    )+};
}
copied_value!(bool, ProfileParser, QuerySource, QueryKind, ExportStrategy);

fn unexpected() -> CatalogError {
    CatalogError::new(CatalogErrorCode::InvalidLayer)
}
fn collect_sites(
    w: &Walk<'_>,
    into: &mut Vec<SourceSite>,
    origin: &Provenance,
) -> Result<(), CatalogError> {
    let mut append = |value| -> Result<(), CatalogError> {
        for existing in into.iter() {
            w.check()?;
            if *existing == value {
                return Ok(());
            }
        }
        w.push(into, value)
    };
    match origin {
        Provenance::Default => Ok(()),
        Provenance::Declared(value) => append(*value),
        Provenance::Derived(values) => {
            for value in values {
                append(*value)?;
            }
            Ok(())
        }
    }
}
fn combined(base: Provenance, upper: SourceSite, w: &Walk<'_>) -> Result<Provenance, CatalogError> {
    if matches!(base, Provenance::Default) {
        w.check()?;
        return Ok(Provenance::Declared(upper));
    }
    let mut sites = Vec::new();
    collect_sites(w, &mut sites, &base)?;
    collect_sites(w, &mut sites, &Provenance::Declared(upper))?;
    Ok(Provenance::Derived(sites))
}
fn set<'a, P, T>(
    target: &mut Planned<T>,
    patch: Presence<'a, P>,
    source: SourceIndex,
    w: &Walk<'_>,
    convert: impl FnOnce(&'a P, &Walk<'_>) -> Result<T, CatalogError>,
) -> Result<(), CatalogError> {
    w.check()?;
    match patch {
        Presence::Missing => Ok(()),
        Presence::Clear { .. } => Err(unexpected()),
        Presence::Set(value) => {
            *target = Planned::declared(convert(value.value(), w)?, site(source, value.location()));
            Ok(())
        }
    }
}
fn set_text<'a>(
    target: &mut Planned<Text<'a>>,
    patch: Presence<'a, String>,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<(), CatalogError> {
    set(target, patch, source, w, |value, w| {
        w.check()?;
        Ok(Text::one(value))
    })
}
fn set_copy<T: Copy>(
    target: &mut Planned<T>,
    patch: Presence<'_, T>,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<(), CatalogError> {
    set(target, patch, source, w, |value, w| {
        w.check()?;
        Ok(*value)
    })
}
fn text_list<'a>(
    input: &'a List<String>,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<PlanList<Planned<Text<'a>>>, CatalogError> {
    let mut values = Vec::new();
    w.reserve(&mut values, input.len())?;
    for item in input.iter() {
        w.check()?;
        values.push(Planned::declared(
            Text::one(item.value()),
            site(source, item.location()),
        ));
    }
    Ok(PlanList { values })
}
fn set_texts<'a>(
    target: &mut Planned<PlanList<Planned<Text<'a>>>>,
    patch: Presence<'a, List<String>>,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<(), CatalogError> {
    set(target, patch, source, w, |value, w| {
        text_list(value, source, w)
    })
}
fn map_patch<'a, P, T>(
    target: &mut Planned<BorrowedMap<'a, T>>,
    patch: Presence<'a, PatchMap<P>>,
    source: SourceIndex,
    w: &Walk<'_>,
    mut convert: impl FnMut(&'a P, &Walk<'_>) -> Result<T, CatalogError>,
) -> Result<(), CatalogError> {
    w.check()?;
    let value = match patch {
        Presence::Missing => return Ok(()),
        Presence::Clear { .. } => return Err(unexpected()),
        Presence::Set(v) => v,
    };
    let container_site = site(source, value.location());
    if value.value().is_empty() {
        *target = Planned::declared(BorrowedMap::empty(), container_site);
        return Ok(());
    }
    let mut entries = std::mem::take(&mut target.value.entries);
    let base_origin = std::mem::replace(&mut target.provenance, Provenance::Default);
    for (key, change) in value.value().iter() {
        w.check()?;
        let mut position = None;
        for (i, (candidate, _)) in entries.iter().enumerate() {
            if w.equal(candidate.value, Text::one(key.value()), false)? {
                position = Some(i);
                break;
            }
        }
        match change {
            Presence::Missing => return Err(unexpected()),
            Presence::Clear { .. } => {
                if let Some(i) = position {
                    w.remove_ordered(&mut entries, i)?;
                }
            }
            Presence::Set(item) => {
                let entry = (
                    Planned::declared(Text::one(key.value()), site(source, key.location())),
                    Planned::declared(convert(item.value(), w)?, site(source, item.location())),
                );
                if let Some(i) = position {
                    entries[i] = entry;
                } else {
                    w.push(&mut entries, entry)?;
                }
            }
        }
    }
    target.value.entries = entries;
    target.provenance = combined(base_origin, container_site, w)?;
    Ok(())
}
fn text_map<'a>(
    target: &mut Planned<BorrowedMap<'a, Text<'a>>>,
    patch: Presence<'a, PatchMap<String>>,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<(), CatalogError> {
    map_patch(target, patch, source, w, |value, w| {
        w.check()?;
        Ok(Text::one(value))
    })
}
fn route_text(value: Text<'_>, code: CatalogErrorCode, w: &Walk<'_>) -> Result<(), CatalogError> {
    let length = value.len()?;
    w.check()?;
    if length == 0
        || value.bytes().any(|v| v == 0)
        || value
            .bytes()
            .next()
            .is_some_and(|v| v.is_ascii_whitespace())
        || value
            .bytes()
            .last()
            .is_some_and(|v| v.is_ascii_whitespace())
    {
        return Err(CatalogError::new(code));
    }
    w.check()
}
pub struct CaptureStep {
    xpath: Effective<String>,
    nearest_descendant: Effective<String>,
    nearest_ancestor: Effective<String>,
}
impl CaptureStep {
    pub fn xpath(&self) -> &Effective<String> {
        &self.xpath
    }
    pub fn nearest_descendant(&self) -> &Effective<String> {
        &self.nearest_descendant
    }
    pub fn nearest_ancestor(&self) -> &Effective<String> {
        &self.nearest_ancestor
    }
}
struct CaptureStepPlan<'a> {
    xpath: Planned<Text<'a>>,
    nearest_descendant: Planned<Text<'a>>,
    nearest_ancestor: Planned<Text<'a>>,
}
impl CaptureStepPlan<'_> {
    fn empty() -> Self {
        Self {
            xpath: Planned::default(Text::one("")),
            nearest_descendant: Planned::default(Text::one("")),
            nearest_ancestor: Planned::default(Text::one("")),
        }
    }
}
impl Materialize for CaptureStepPlan<'_> {
    type Output = CaptureStep;
    fn materialize(self, w: &Walk<'_>) -> Result<CaptureStep, CatalogError> {
        w.check()?;
        Ok(CaptureStep {
            xpath: self.xpath.materialize(w)?,
            nearest_descendant: self.nearest_descendant.materialize(w)?,
            nearest_ancestor: self.nearest_ancestor.materialize(w)?,
        })
    }
}
fn capture_step_plan<'a>(
    patch: &'a CaptureStepPatch,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<CaptureStepPlan<'a>, CatalogError> {
    w.check()?;
    let mut output = CaptureStepPlan::empty();
    set_text(&mut output.xpath, patch.xpath(), source, w)?;
    set_text(
        &mut output.nearest_descendant,
        patch.nearest_descendant(),
        source,
        w,
    )?;
    set_text(
        &mut output.nearest_ancestor,
        patch.nearest_ancestor(),
        source,
        w,
    )?;
    Ok(output)
}
pub struct AntlrContext {
    pattern: Effective<String>,
    keyword: Effective<String>,
    child_rule: Effective<String>,
    negate: Effective<bool>,
    label: Effective<String>,
    name: Effective<EffectiveList<Effective<CaptureStep>>>,
}
impl AntlrContext {
    pub fn pattern(&self) -> &Effective<String> {
        &self.pattern
    }
    pub fn keyword(&self) -> &Effective<String> {
        &self.keyword
    }
    pub fn child_rule(&self) -> &Effective<String> {
        &self.child_rule
    }
    pub fn negate(&self) -> &Effective<bool> {
        &self.negate
    }
    pub fn label(&self) -> &Effective<String> {
        &self.label
    }
    pub fn name(&self) -> &Effective<EffectiveList<Effective<CaptureStep>>> {
        &self.name
    }
}
struct AntlrContextPlan<'a> {
    pattern: Planned<Text<'a>>,
    keyword: Planned<Text<'a>>,
    child_rule: Planned<Text<'a>>,
    negate: Planned<bool>,
    label: Planned<Text<'a>>,
    name: Planned<PlanList<Planned<CaptureStepPlan<'a>>>>,
}
impl AntlrContextPlan<'_> {
    fn empty() -> Self {
        Self {
            pattern: Planned::default(Text::one("")),
            keyword: Planned::default(Text::one("")),
            child_rule: Planned::default(Text::one("")),
            negate: Planned::default(false),
            label: Planned::default(Text::one("")),
            name: Planned::default(PlanList::empty()),
        }
    }
}
impl Materialize for AntlrContextPlan<'_> {
    type Output = AntlrContext;
    fn materialize(self, w: &Walk<'_>) -> Result<AntlrContext, CatalogError> {
        w.check()?;
        Ok(AntlrContext {
            pattern: self.pattern.materialize(w)?,
            keyword: self.keyword.materialize(w)?,
            child_rule: self.child_rule.materialize(w)?,
            negate: self.negate.materialize(w)?,
            label: self.label.materialize(w)?,
            name: self.name.materialize(w)?,
        })
    }
}
fn antlr_context_plan<'a>(
    patch: &'a AntlrContextPatch,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<AntlrContextPlan<'a>, CatalogError> {
    w.check()?;
    let mut output = AntlrContextPlan::empty();
    set_text(&mut output.pattern, patch.pattern(), source, w)?;
    set_text(&mut output.keyword, patch.keyword(), source, w)?;
    set_text(&mut output.child_rule, patch.child_rule(), source, w)?;
    set_copy(&mut output.negate, patch.negate(), source, w)?;
    set_text(&mut output.label, patch.label(), source, w)?;
    set(&mut output.name, patch.name(), source, w, |items, w| {
        capture_steps(items, source, w)
    })?;
    Ok(output)
}
fn capture_steps<'a>(
    items: &'a List<CaptureStepPatch>,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<PlanList<Planned<CaptureStepPlan<'a>>>, CatalogError> {
    let mut values = Vec::new();
    w.reserve(&mut values, items.len())?;
    for item in items.iter() {
        w.check()?;
        values.push(Planned::declared(
            capture_step_plan(item.value(), source, w)?,
            site(source, item.location()),
        ));
    }
    Ok(PlanList { values })
}

pub struct Query {
    select: Effective<String>,
    keyword: Effective<String>,
    child_rule: Effective<String>,
    negate: Effective<bool>,
    captures: Effective<EffectiveMap<EffectiveList<Effective<CaptureStep>>>>,

    source: Effective<QuerySource>,
    data_key: Effective<String>,
    pattern: Effective<String>,
    graph_label: Effective<String>,
    name_capture: Effective<String>,
    relation_type: Effective<String>,
    value_capture: Effective<String>,
    value_label: Effective<String>,
    name_reject: Effective<String>,
    span_capture: Effective<String>,
    declaration_capture: Effective<String>,
    context_capture: Effective<String>,
    parent_capture: Effective<String>,
    parent_label: Effective<String>,
    qualifier_capture: Effective<String>,
    target_fallback: Effective<String>,
    r#type: Effective<QueryKind>,
    name_is_data: Effective<bool>,
    target_labels: Effective<EffectiveList<EffectiveText>>,
}

impl Query {
    pub fn select(&self) -> &Effective<String> {
        &self.select
    }
    pub fn keyword(&self) -> &Effective<String> {
        &self.keyword
    }
    pub fn child_rule(&self) -> &Effective<String> {
        &self.child_rule
    }
    pub fn negate(&self) -> &Effective<bool> {
        &self.negate
    }
    pub fn captures(&self) -> &Effective<EffectiveMap<EffectiveList<Effective<CaptureStep>>>> {
        &self.captures
    }

    pub fn source(&self) -> &Effective<QuerySource> {
        &self.source
    }
    pub fn data_key(&self) -> &Effective<String> {
        &self.data_key
    }
    pub fn pattern(&self) -> &Effective<String> {
        &self.pattern
    }
    pub fn graph_label(&self) -> &Effective<String> {
        &self.graph_label
    }
    pub fn name_capture(&self) -> &Effective<String> {
        &self.name_capture
    }
    pub fn relation_type(&self) -> &Effective<String> {
        &self.relation_type
    }
    pub fn value_capture(&self) -> &Effective<String> {
        &self.value_capture
    }
    pub fn value_label(&self) -> &Effective<String> {
        &self.value_label
    }
    pub fn name_reject(&self) -> &Effective<String> {
        &self.name_reject
    }
    pub fn span_capture(&self) -> &Effective<String> {
        &self.span_capture
    }
    pub fn declaration_capture(&self) -> &Effective<String> {
        &self.declaration_capture
    }
    pub fn context_capture(&self) -> &Effective<String> {
        &self.context_capture
    }
    pub fn parent_capture(&self) -> &Effective<String> {
        &self.parent_capture
    }
    pub fn parent_label(&self) -> &Effective<String> {
        &self.parent_label
    }
    pub fn qualifier_capture(&self) -> &Effective<String> {
        &self.qualifier_capture
    }
    pub fn target_fallback(&self) -> &Effective<String> {
        &self.target_fallback
    }
    pub fn r#type(&self) -> &Effective<QueryKind> {
        &self.r#type
    }
    pub fn name_is_data(&self) -> &Effective<bool> {
        &self.name_is_data
    }
    pub fn target_labels(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.target_labels
    }
}

struct QueryPlan<'a> {
    select: Planned<Text<'a>>,
    keyword: Planned<Text<'a>>,
    child_rule: Planned<Text<'a>>,
    negate: Planned<bool>,
    captures: Planned<BorrowedMap<'a, PlanList<Planned<CaptureStepPlan<'a>>>>>,

    source: Planned<QuerySource>,
    data_key: Planned<Text<'a>>,
    pattern: Planned<Text<'a>>,
    graph_label: Planned<Text<'a>>,
    name_capture: Planned<Text<'a>>,
    relation_type: Planned<Text<'a>>,
    value_capture: Planned<Text<'a>>,
    value_label: Planned<Text<'a>>,
    name_reject: Planned<Text<'a>>,
    span_capture: Planned<Text<'a>>,
    declaration_capture: Planned<Text<'a>>,
    context_capture: Planned<Text<'a>>,
    parent_capture: Planned<Text<'a>>,
    parent_label: Planned<Text<'a>>,
    qualifier_capture: Planned<Text<'a>>,
    target_fallback: Planned<Text<'a>>,
    r#type: Planned<QueryKind>,
    name_is_data: Planned<bool>,
    target_labels: Planned<PlanList<Planned<Text<'a>>>>,
}

impl QueryPlan<'_> {
    fn empty() -> Self {
        Self {
            select: Planned::default(Text::one("")),
            keyword: Planned::default(Text::one("")),
            child_rule: Planned::default(Text::one("")),
            negate: Planned::default(false),
            captures: Planned::default(BorrowedMap::empty()),

            source: Planned::default(QuerySource::Primary),
            data_key: Planned::default(Text::one("")),
            pattern: Planned::default(Text::one("")),
            graph_label: Planned::default(Text::one("")),
            name_capture: Planned::default(Text::one("")),
            relation_type: Planned::default(Text::one("")),
            value_capture: Planned::default(Text::one("")),
            value_label: Planned::default(Text::one("")),
            name_reject: Planned::default(Text::one("")),
            span_capture: Planned::default(Text::one("")),
            declaration_capture: Planned::default(Text::one("")),
            context_capture: Planned::default(Text::one("")),
            parent_capture: Planned::default(Text::one("")),
            parent_label: Planned::default(Text::one("")),
            qualifier_capture: Planned::default(Text::one("")),
            target_fallback: Planned::default(Text::one("")),
            r#type: Planned::default(QueryKind::Entity),
            name_is_data: Planned::default(false),
            target_labels: Planned::default(PlanList::empty()),
        }
    }
}

impl Materialize for QueryPlan<'_> {
    type Output = Query;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        w.check()?;
        Ok(Query {
            select: self.select.materialize(w)?,
            keyword: self.keyword.materialize(w)?,
            child_rule: self.child_rule.materialize(w)?,
            negate: self.negate.materialize(w)?,
            captures: self.captures.materialize(w)?,

            source: self.source.materialize(w)?,
            data_key: self.data_key.materialize(w)?,
            pattern: self.pattern.materialize(w)?,
            graph_label: self.graph_label.materialize(w)?,
            name_capture: self.name_capture.materialize(w)?,
            relation_type: self.relation_type.materialize(w)?,
            value_capture: self.value_capture.materialize(w)?,
            value_label: self.value_label.materialize(w)?,
            name_reject: self.name_reject.materialize(w)?,
            span_capture: self.span_capture.materialize(w)?,
            declaration_capture: self.declaration_capture.materialize(w)?,
            context_capture: self.context_capture.materialize(w)?,
            parent_capture: self.parent_capture.materialize(w)?,
            parent_label: self.parent_label.materialize(w)?,
            qualifier_capture: self.qualifier_capture.materialize(w)?,
            target_fallback: self.target_fallback.materialize(w)?,
            r#type: self.r#type.materialize(w)?,
            name_is_data: self.name_is_data.materialize(w)?,
            target_labels: self.target_labels.materialize(w)?,
        })
    }
}

pub struct Exports {
    strategy: Effective<ExportStrategy>,
    config: Effective<EffectiveMap<String>>,
    config_list: Effective<EffectiveMap<EffectiveList<EffectiveText>>>,
}

impl Exports {
    pub fn strategy(&self) -> &Effective<ExportStrategy> {
        &self.strategy
    }
    pub fn config(&self) -> &Effective<EffectiveMap<String>> {
        &self.config
    }
    pub fn config_list(&self) -> &Effective<EffectiveMap<EffectiveList<EffectiveText>>> {
        &self.config_list
    }
}

struct ExportsPlan<'a> {
    strategy: Planned<ExportStrategy>,
    config: Planned<BorrowedMap<'a, Text<'a>>>,
    config_list: Planned<BorrowedMap<'a, PlanList<Planned<Text<'a>>>>>,
}

impl ExportsPlan<'_> {
    fn empty() -> Self {
        Self {
            strategy: Planned::default(ExportStrategy::None),
            config: Planned::default(BorrowedMap::empty()),
            config_list: Planned::default(BorrowedMap::empty()),
        }
    }
}

impl Materialize for ExportsPlan<'_> {
    type Output = Exports;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        w.check()?;
        Ok(Exports {
            strategy: self.strategy.materialize(w)?,
            config: self.config.materialize(w)?,
            config_list: self.config_list.materialize(w)?,
        })
    }
}

pub struct TargetRule {
    labels: Effective<EffectiveList<EffectiveText>>,
    fallback: Effective<String>,
}

impl TargetRule {
    pub fn labels(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.labels
    }
    pub fn fallback(&self) -> &Effective<String> {
        &self.fallback
    }
}

struct TargetRulePlan<'a> {
    labels: Planned<PlanList<Planned<Text<'a>>>>,
    fallback: Planned<Text<'a>>,
}

impl TargetRulePlan<'_> {
    fn empty() -> Self {
        Self {
            labels: Planned::default(PlanList::empty()),
            fallback: Planned::default(Text::one("")),
        }
    }
}

impl Materialize for TargetRulePlan<'_> {
    type Output = TargetRule;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        w.check()?;
        Ok(TargetRule {
            labels: self.labels.materialize(w)?,
            fallback: self.fallback.materialize(w)?,
        })
    }
}

pub struct Embedded {
    pattern: Effective<String>,
    text_capture: Effective<String>,
    lang_capture: Effective<String>,
    r#default: Effective<String>,
    wrap_prefix: Effective<String>,
    wrap_suffix: Effective<String>,
    normalize: Effective<String>,
    languages: Effective<EffectiveMap<String>>,
    host_labels: Effective<EffectiveList<EffectiveText>>,
}

impl Embedded {
    pub fn pattern(&self) -> &Effective<String> {
        &self.pattern
    }
    pub fn text_capture(&self) -> &Effective<String> {
        &self.text_capture
    }
    pub fn lang_capture(&self) -> &Effective<String> {
        &self.lang_capture
    }
    pub fn r#default(&self) -> &Effective<String> {
        &self.r#default
    }
    pub fn wrap_prefix(&self) -> &Effective<String> {
        &self.wrap_prefix
    }
    pub fn wrap_suffix(&self) -> &Effective<String> {
        &self.wrap_suffix
    }
    pub fn normalize(&self) -> &Effective<String> {
        &self.normalize
    }
    pub fn languages(&self) -> &Effective<EffectiveMap<String>> {
        &self.languages
    }
    pub fn host_labels(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.host_labels
    }
}

struct EmbeddedPlan<'a> {
    pattern: Planned<Text<'a>>,
    text_capture: Planned<Text<'a>>,
    lang_capture: Planned<Text<'a>>,
    r#default: Planned<Text<'a>>,
    wrap_prefix: Planned<Text<'a>>,
    wrap_suffix: Planned<Text<'a>>,
    normalize: Planned<Text<'a>>,
    languages: Planned<BorrowedMap<'a, Text<'a>>>,
    host_labels: Planned<PlanList<Planned<Text<'a>>>>,
}

impl EmbeddedPlan<'_> {
    fn empty() -> Self {
        Self {
            pattern: Planned::default(Text::one("")),
            text_capture: Planned::default(Text::one("")),
            lang_capture: Planned::default(Text::one("")),
            r#default: Planned::default(Text::one("")),
            wrap_prefix: Planned::default(Text::one("")),
            wrap_suffix: Planned::default(Text::one("")),
            normalize: Planned::default(Text::one("")),
            languages: Planned::default(BorrowedMap::empty()),
            host_labels: Planned::default(PlanList::empty()),
        }
    }
}

impl Materialize for EmbeddedPlan<'_> {
    type Output = Embedded;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        w.check()?;
        Ok(Embedded {
            pattern: self.pattern.materialize(w)?,
            text_capture: self.text_capture.materialize(w)?,
            lang_capture: self.lang_capture.materialize(w)?,
            r#default: self.r#default.materialize(w)?,
            wrap_prefix: self.wrap_prefix.materialize(w)?,
            wrap_suffix: self.wrap_suffix.materialize(w)?,
            normalize: self.normalize.materialize(w)?,
            languages: self.languages.materialize(w)?,
            host_labels: self.host_labels.materialize(w)?,
        })
    }
}

pub struct Normalizer {
    replace: Effective<EffectiveMap<String>>,
    numeric_char_refs: Effective<bool>,
}

impl Normalizer {
    pub fn replace(&self) -> &Effective<EffectiveMap<String>> {
        &self.replace
    }
    pub fn numeric_char_refs(&self) -> &Effective<bool> {
        &self.numeric_char_refs
    }
}

struct NormalizerPlan<'a> {
    replace: Planned<BorrowedMap<'a, Text<'a>>>,
    numeric_char_refs: Planned<bool>,
}

impl NormalizerPlan<'_> {
    fn empty() -> Self {
        Self {
            replace: Planned::default(BorrowedMap::empty()),
            numeric_char_refs: Planned::default(false),
        }
    }
}

impl Materialize for NormalizerPlan<'_> {
    type Output = Normalizer;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        w.check()?;
        Ok(Normalizer {
            replace: self.replace.materialize(w)?,
            numeric_char_refs: self.numeric_char_refs.materialize(w)?,
        })
    }
}

pub struct HeadCalls {
    node_type: Effective<String>,
    names: Effective<EffectiveList<EffectiveText>>,
    pair_names: Effective<EffectiveList<EffectiveText>>,
    subject_pair_names: Effective<EffectiveList<EffectiveText>>,
}

impl HeadCalls {
    pub fn node_type(&self) -> &Effective<String> {
        &self.node_type
    }
    pub fn names(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.names
    }
    pub fn pair_names(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.pair_names
    }
    pub fn subject_pair_names(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.subject_pair_names
    }
}

struct HeadCallsPlan<'a> {
    node_type: Planned<Text<'a>>,
    names: Planned<PlanList<Planned<Text<'a>>>>,
    pair_names: Planned<PlanList<Planned<Text<'a>>>>,
    subject_pair_names: Planned<PlanList<Planned<Text<'a>>>>,
}

impl HeadCallsPlan<'_> {
    fn empty() -> Self {
        Self {
            node_type: Planned::default(Text::one("")),
            names: Planned::default(PlanList::empty()),
            pair_names: Planned::default(PlanList::empty()),
            subject_pair_names: Planned::default(PlanList::empty()),
        }
    }
}

impl Materialize for HeadCallsPlan<'_> {
    type Output = HeadCalls;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        w.check()?;
        Ok(HeadCalls {
            node_type: self.node_type.materialize(w)?,
            names: self.names.materialize(w)?,
            pair_names: self.pair_names.materialize(w)?,
            subject_pair_names: self.subject_pair_names.materialize(w)?,
        })
    }
}

pub struct Complexity {
    node_types: Effective<EffectiveList<EffectiveText>>,
    operators: Effective<EffectiveList<EffectiveText>>,
    head_calls: Effective<Option<HeadCalls>>,
}

impl Complexity {
    pub fn node_types(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.node_types
    }
    pub fn operators(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.operators
    }
    pub fn head_calls(&self) -> &Effective<Option<HeadCalls>> {
        &self.head_calls
    }
}

struct ComplexityPlan<'a> {
    node_types: Planned<PlanList<Planned<Text<'a>>>>,
    operators: Planned<PlanList<Planned<Text<'a>>>>,
    head_calls: Planned<Option<HeadCallsPlan<'a>>>,
}

impl ComplexityPlan<'_> {
    fn empty() -> Self {
        Self {
            node_types: Planned::default(PlanList::empty()),
            operators: Planned::default(PlanList::empty()),
            head_calls: Planned::default(None),
        }
    }
}

impl Materialize for ComplexityPlan<'_> {
    type Output = Complexity;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        w.check()?;
        Ok(Complexity {
            node_types: self.node_types.materialize(w)?,
            operators: self.operators.materialize(w)?,
            head_calls: self.head_calls.materialize(w)?,
        })
    }
}

pub struct EffectiveProfile {
    language: Effective<String>,
    extensions: Effective<EffectiveList<EffectiveText>>,
    parser: Effective<ProfileParser>,
    grammar: Effective<String>,
    start_rule: Effective<Option<String>>,
    exclusive: Effective<bool>,
    queries: Effective<EffectiveList<Effective<Query>>>,
    exports: Effective<Exports>,
    context_types: Effective<EffectiveMap<String>>,
    context_requires_entity: Effective<EffectiveList<EffectiveText>>,
    antlr_contexts: Effective<EffectiveList<Effective<AntlrContext>>>,
    antlr_primary_queries: std::sync::OnceLock<crate::antlr_engine::PreparedProfileQueries>,
    antlr_preprocessor_queries: std::sync::OnceLock<crate::antlr_engine::PreparedProfileQueries>,
    context_name_queries: Effective<EffectiveMap<String>>,
    target_rules: Effective<EffectiveMap<TargetRule>>,
    embedded: Effective<EffectiveList<Effective<Embedded>>>,
    text_normalizers: Effective<EffectiveMap<Normalizer>>,
    self_keywords: Effective<EffectiveList<EffectiveText>>,
    anon_func_types: Effective<EffectiveList<EffectiveText>>,
    declaration_types: Effective<EffectiveList<EffectiveText>>,
    comment_types: Effective<EffectiveList<EffectiveText>>,
    embed_labels: Effective<EffectiveList<EffectiveText>>,
    complexity: Effective<Complexity>,
}

impl EffectiveProfile {
    pub fn language(&self) -> &Effective<String> {
        &self.language
    }
    pub fn extensions(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.extensions
    }
    pub fn parser(&self) -> &Effective<ProfileParser> {
        &self.parser
    }
    pub fn grammar(&self) -> &Effective<String> {
        &self.grammar
    }
    pub fn start_rule(&self) -> &Effective<Option<String>> {
        &self.start_rule
    }
    pub fn exclusive(&self) -> &Effective<bool> {
        &self.exclusive
    }
    pub fn queries(&self) -> &Effective<EffectiveList<Effective<Query>>> {
        &self.queries
    }
    pub fn exports(&self) -> &Effective<Exports> {
        &self.exports
    }
    pub fn context_types(&self) -> &Effective<EffectiveMap<String>> {
        &self.context_types
    }
    pub fn context_requires_entity(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.context_requires_entity
    }
    pub fn context_name_queries(&self) -> &Effective<EffectiveMap<String>> {
        &self.context_name_queries
    }
    pub fn antlr_contexts(&self) -> &Effective<EffectiveList<Effective<AntlrContext>>> {
        &self.antlr_contexts
    }
    pub(crate) fn antlr_primary_queries(
        &self,
    ) -> &std::sync::OnceLock<crate::antlr_engine::PreparedProfileQueries> {
        &self.antlr_primary_queries
    }
    pub(crate) fn antlr_preprocessor_queries(
        &self,
    ) -> &std::sync::OnceLock<crate::antlr_engine::PreparedProfileQueries> {
        &self.antlr_preprocessor_queries
    }
    pub fn target_rules(&self) -> &Effective<EffectiveMap<TargetRule>> {
        &self.target_rules
    }
    pub fn embedded(&self) -> &Effective<EffectiveList<Effective<Embedded>>> {
        &self.embedded
    }
    pub fn text_normalizers(&self) -> &Effective<EffectiveMap<Normalizer>> {
        &self.text_normalizers
    }
    pub fn self_keywords(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.self_keywords
    }
    pub fn anon_func_types(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.anon_func_types
    }
    pub fn declaration_types(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.declaration_types
    }
    pub fn comment_types(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.comment_types
    }
    pub fn embed_labels(&self) -> &Effective<EffectiveList<EffectiveText>> {
        &self.embed_labels
    }
    pub fn complexity(&self) -> &Effective<Complexity> {
        &self.complexity
    }
}

struct EffectiveProfilePlan<'a> {
    language: Planned<Text<'a>>,
    extensions: Planned<PlanList<Planned<Text<'a>>>>,
    parser: Planned<ProfileParser>,
    grammar: Planned<Text<'a>>,
    start_rule: Planned<Option<Text<'a>>>,
    exclusive: Planned<bool>,
    queries: Planned<PlanList<Planned<QueryPlan<'a>>>>,
    exports: Planned<ExportsPlan<'a>>,
    context_types: Planned<BorrowedMap<'a, Text<'a>>>,
    context_requires_entity: Planned<PlanList<Planned<Text<'a>>>>,
    antlr_contexts: Planned<PlanList<Planned<AntlrContextPlan<'a>>>>,
    context_name_queries: Planned<BorrowedMap<'a, Text<'a>>>,
    target_rules: Planned<BorrowedMap<'a, TargetRulePlan<'a>>>,
    embedded: Planned<PlanList<Planned<EmbeddedPlan<'a>>>>,
    text_normalizers: Planned<BorrowedMap<'a, NormalizerPlan<'a>>>,
    self_keywords: Planned<PlanList<Planned<Text<'a>>>>,
    anon_func_types: Planned<PlanList<Planned<Text<'a>>>>,
    declaration_types: Planned<PlanList<Planned<Text<'a>>>>,
    comment_types: Planned<PlanList<Planned<Text<'a>>>>,
    embed_labels: Planned<PlanList<Planned<Text<'a>>>>,
    complexity: Planned<ComplexityPlan<'a>>,
}

impl EffectiveProfilePlan<'_> {
    fn empty() -> Self {
        Self {
            language: Planned::default(Text::one("")),
            extensions: Planned::default(PlanList::empty()),
            parser: Planned::default(ProfileParser::TreeSitter),
            grammar: Planned::default(Text::one("")),
            start_rule: Planned::default(None),
            exclusive: Planned::default(false),
            queries: Planned::default(PlanList::empty()),
            exports: Planned::default(ExportsPlan::empty()),
            context_types: Planned::default(BorrowedMap::empty()),
            context_requires_entity: Planned::default(PlanList::empty()),
            antlr_contexts: Planned::default(PlanList::empty()),
            context_name_queries: Planned::default(BorrowedMap::empty()),
            target_rules: Planned::default(BorrowedMap::empty()),
            embedded: Planned::default(PlanList::empty()),
            text_normalizers: Planned::default(BorrowedMap::empty()),
            self_keywords: Planned::default(PlanList::empty()),
            anon_func_types: Planned::default(PlanList::empty()),
            declaration_types: Planned::default(PlanList::empty()),
            comment_types: Planned::default(PlanList::empty()),
            embed_labels: Planned::default(PlanList::empty()),
            complexity: Planned::default(ComplexityPlan::empty()),
        }
    }
}

impl Materialize for EffectiveProfilePlan<'_> {
    type Output = EffectiveProfile;
    fn materialize(self, w: &Walk<'_>) -> Result<Self::Output, CatalogError> {
        w.check()?;
        Ok(EffectiveProfile {
            language: self.language.materialize(w)?,
            extensions: self.extensions.materialize(w)?,
            parser: self.parser.materialize(w)?,
            grammar: self.grammar.materialize(w)?,
            start_rule: self.start_rule.materialize(w)?,
            exclusive: self.exclusive.materialize(w)?,
            queries: self.queries.materialize(w)?,
            exports: self.exports.materialize(w)?,
            context_types: self.context_types.materialize(w)?,
            context_requires_entity: self.context_requires_entity.materialize(w)?,
            antlr_contexts: self.antlr_contexts.materialize(w)?,
            antlr_primary_queries: std::sync::OnceLock::new(),
            antlr_preprocessor_queries: std::sync::OnceLock::new(),
            context_name_queries: self.context_name_queries.materialize(w)?,
            target_rules: self.target_rules.materialize(w)?,
            embedded: self.embedded.materialize(w)?,
            text_normalizers: self.text_normalizers.materialize(w)?,
            self_keywords: self.self_keywords.materialize(w)?,
            anon_func_types: self.anon_func_types.materialize(w)?,
            declaration_types: self.declaration_types.materialize(w)?,
            comment_types: self.comment_types.materialize(w)?,
            embed_labels: self.embed_labels.materialize(w)?,
            complexity: self.complexity.materialize(w)?,
        })
    }
}
fn query_plan<'a>(
    patch: &'a QueryPatch,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<QueryPlan<'a>, CatalogError> {
    w.check()?;
    let mut output = QueryPlan::empty();
    set_text(&mut output.select, patch.select(), source, w)?;
    set_text(&mut output.keyword, patch.keyword(), source, w)?;
    set_text(&mut output.child_rule, patch.child_rule(), source, w)?;
    set_copy(&mut output.negate, patch.negate(), source, w)?;
    map_patch(
        &mut output.captures,
        patch.captures(),
        source,
        w,
        |items, w| capture_steps(items, source, w),
    )?;
    set_copy(&mut output.source, patch.source(), source, w)?;
    set_text(&mut output.data_key, patch.data_key(), source, w)?;
    set_text(&mut output.pattern, patch.pattern(), source, w)?;
    set_text(&mut output.graph_label, patch.graph_label(), source, w)?;
    set_text(&mut output.name_capture, patch.name_capture(), source, w)?;
    set_text(&mut output.relation_type, patch.relation_type(), source, w)?;
    set_text(&mut output.value_capture, patch.value_capture(), source, w)?;
    set_text(&mut output.value_label, patch.value_label(), source, w)?;
    set_text(&mut output.name_reject, patch.name_reject(), source, w)?;
    set_text(&mut output.span_capture, patch.span_capture(), source, w)?;
    set_text(
        &mut output.declaration_capture,
        patch.declaration_capture(),
        source,
        w,
    )?;
    set_text(
        &mut output.context_capture,
        patch.context_capture(),
        source,
        w,
    )?;
    set_text(
        &mut output.parent_capture,
        patch.parent_capture(),
        source,
        w,
    )?;
    set_text(&mut output.parent_label, patch.parent_label(), source, w)?;
    set_text(
        &mut output.qualifier_capture,
        patch.qualifier_capture(),
        source,
        w,
    )?;
    set_text(
        &mut output.target_fallback,
        patch.target_fallback(),
        source,
        w,
    )?;
    set_copy(&mut output.r#type, patch.r#type(), source, w)?;
    set_copy(&mut output.name_is_data, patch.name_is_data(), source, w)?;
    set_texts(&mut output.target_labels, patch.target_labels(), source, w)?;
    Ok(output)
}

fn target_plan<'a>(
    patch: &'a TargetRulePatch,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<TargetRulePlan<'a>, CatalogError> {
    w.check()?;
    let mut output = TargetRulePlan::empty();
    set_texts(&mut output.labels, patch.labels(), source, w)?;
    set_text(&mut output.fallback, patch.fallback(), source, w)?;
    Ok(output)
}

fn embedded_plan<'a>(
    patch: &'a EmbeddedPatch,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<EmbeddedPlan<'a>, CatalogError> {
    w.check()?;
    let mut output = EmbeddedPlan::empty();
    set_text(&mut output.pattern, patch.pattern(), source, w)?;
    set_text(&mut output.text_capture, patch.text_capture(), source, w)?;
    set_text(&mut output.lang_capture, patch.lang_capture(), source, w)?;
    set_text(&mut output.r#default, patch.r#default(), source, w)?;
    set_text(&mut output.wrap_prefix, patch.wrap_prefix(), source, w)?;
    set_text(&mut output.wrap_suffix, patch.wrap_suffix(), source, w)?;
    set_text(&mut output.normalize, patch.normalize(), source, w)?;
    text_map(&mut output.languages, patch.languages(), source, w)?;
    set_texts(&mut output.host_labels, patch.host_labels(), source, w)?;
    Ok(output)
}

fn normalizer_plan<'a>(
    patch: &'a TextNormalizerPatch,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<NormalizerPlan<'a>, CatalogError> {
    w.check()?;
    let mut output = NormalizerPlan::empty();
    text_map(&mut output.replace, patch.replace(), source, w)?;
    set_copy(
        &mut output.numeric_char_refs,
        patch.numeric_char_refs(),
        source,
        w,
    )?;
    Ok(output)
}

fn head_plan<'a>(
    patch: &'a HeadCallsPatch,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<HeadCallsPlan<'a>, CatalogError> {
    w.check()?;
    let mut output = HeadCallsPlan::empty();
    set_text(&mut output.node_type, patch.node_type(), source, w)?;
    set_texts(&mut output.names, patch.names(), source, w)?;
    set_texts(&mut output.pair_names, patch.pair_names(), source, w)?;
    set_texts(
        &mut output.subject_pair_names,
        patch.subject_pair_names(),
        source,
        w,
    )?;
    Ok(output)
}

fn exports_plan<'a>(
    patch: &'a ExportPatch,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<ExportsPlan<'a>, CatalogError> {
    let mut output = ExportsPlan::empty();
    set_copy(&mut output.strategy, patch.strategy(), source, w)?;
    text_map(&mut output.config, patch.config(), source, w)?;
    map_patch(
        &mut output.config_list,
        patch.config_list(),
        source,
        w,
        |value, w| text_list(value, source, w),
    )?;
    Ok(output)
}
fn complexity_patch<'a>(
    target: &mut Planned<ComplexityPlan<'a>>,
    patch: Presence<'a, ComplexityPatch>,
    source: SourceIndex,
    w: &Walk<'_>,
) -> Result<(), CatalogError> {
    w.check()?;
    match patch {
        Presence::Missing => Ok(()),
        Presence::Clear { .. } => {
            *target = Planned::default(ComplexityPlan::empty());
            Ok(())
        }
        Presence::Set(value) => {
            let p = value.value();
            set_texts(&mut target.value.node_types, p.node_types(), source, w)?;
            set_texts(&mut target.value.operators, p.operators(), source, w)?;
            match p.head_calls() {
                Presence::Missing => {}
                Presence::Clear { .. } => target.value.head_calls = Planned::default(None),
                Presence::Set(head) => {
                    target.value.head_calls = Planned::declared(
                        Some(head_plan(head.value(), source, w)?),
                        site(source, head.location()),
                    )
                }
            }
            let old = std::mem::replace(&mut target.provenance, Provenance::Default);
            target.provenance = combined(old, site(source, value.location()), w)?;
            Ok(())
        }
    }
}
fn queries_patch<'a>(
    target: &mut Planned<PlanList<Planned<QueryPlan<'a>>>>,
    patch: Presence<'a, List<QueryPatch>>,
    source: SourceIndex,
    merge: bool,
    w: &Walk<'_>,
) -> Result<(), CatalogError> {
    let upper = match patch {
        Presence::Missing => return w.check(),
        Presence::Clear { .. } => return Err(unexpected()),
        Presence::Set(v) => v,
    };
    if !merge || upper.value().is_empty() {
        *target = Planned::default(PlanList::empty());
    }
    if merge && !upper.value().is_empty() {
        let mut i = 0;
        while i < target.value.values.len() {
            w.check()?;
            let mut replaced = false;
            for item in upper.value().iter() {
                let key = match item.value().data_key() {
                    Presence::Set(v) => v,
                    _ => return Err(unexpected()),
                };
                if w.equal(
                    target.value.values[i].value.data_key.value,
                    Text::one(key.value()),
                    false,
                )? {
                    replaced = true;
                    break;
                }
            }
            if replaced {
                w.remove_ordered(&mut target.value.values, i)?;
            } else {
                i += 1;
            }
        }
    }
    w.reserve(&mut target.value.values, upper.value().len())?;
    for item in upper.value().iter() {
        w.check()?;
        target.value.values.push(Planned::declared(
            query_plan(item.value(), source, w)?,
            site(source, item.location()),
        ));
    }
    let old = std::mem::replace(&mut target.provenance, Provenance::Default);
    target.provenance = combined(old, site(source, upper.location()), w)?;
    Ok(())
}
fn embedded_patch<'a>(
    target: &mut Planned<PlanList<Planned<EmbeddedPlan<'a>>>>,
    patch: Presence<'a, List<EmbeddedPatch>>,
    source: SourceIndex,
    merge: bool,
    w: &Walk<'_>,
) -> Result<(), CatalogError> {
    let upper = match patch {
        Presence::Missing => return w.check(),
        Presence::Clear { .. } => return Err(unexpected()),
        Presence::Set(v) => v,
    };
    let keep = merge && !upper.value().is_empty();
    let base = std::mem::take(&mut target.value.values);
    let mut values = Vec::new();
    w.reserve(
        &mut values,
        plus(upper.value().len(), if keep { base.len() } else { 0 })?,
    )?;
    for item in upper.value().iter() {
        w.check()?;
        values.push(Planned::declared(
            embedded_plan(item.value(), source, w)?,
            site(source, item.location()),
        ));
    }
    if keep {
        for item in base {
            w.check()?;
            values.push(item);
        }
    }
    let old = std::mem::replace(&mut target.provenance, Provenance::Default);
    target.provenance = if keep {
        combined(old, site(source, upper.location()), w)?
    } else {
        Provenance::Declared(site(source, upper.location()))
    };
    target.value.values = values;
    Ok(())
}
fn apply_profile<'a>(
    target: &mut EffectiveProfilePlan<'a>,
    patch: &'a ProfilePatch,
    source: SourceIndex,
    inherited: bool,
    w: &Walk<'_>,
) -> Result<(), CatalogError> {
    set_text(&mut target.language, patch.language(), source, w)?;
    set_copy(&mut target.parser, patch.parser(), source, w)?;
    set_copy(&mut target.exclusive, patch.exclusive(), source, w)?;
    set(
        &mut target.start_rule,
        patch.start_rule(),
        source,
        w,
        |v, w| {
            w.check()?;
            Ok(Some(Text::one(v)))
        },
    )?;
    set_texts(&mut target.extensions, patch.extensions(), source, w)?;
    set_texts(&mut target.self_keywords, patch.self_keywords(), source, w)?;
    set_texts(
        &mut target.anon_func_types,
        patch.anon_func_types(),
        source,
        w,
    )?;
    set_texts(
        &mut target.declaration_types,
        patch.declaration_types(),
        source,
        w,
    )?;
    set_texts(&mut target.comment_types, patch.comment_types(), source, w)?;
    set_texts(&mut target.embed_labels, patch.embed_labels(), source, w)?;
    text_map(&mut target.context_types, patch.context_types(), source, w)?;
    set_texts(
        &mut target.context_requires_entity,
        patch.context_requires_entity(),
        source,
        w,
    )?;
    set(
        &mut target.antlr_contexts,
        patch.antlr_contexts(),
        source,
        w,
        |items, w| {
            let mut values = Vec::new();
            w.reserve(&mut values, items.len())?;
            for item in items.iter() {
                w.check()?;
                values.push(Planned::declared(
                    antlr_context_plan(item.value(), source, w)?,
                    site(source, item.location()),
                ));
            }
            Ok(PlanList { values })
        },
    )?;
    text_map(
        &mut target.context_name_queries,
        patch.context_name_queries(),
        source,
        w,
    )?;
    map_patch(
        &mut target.target_rules,
        patch.target_rules(),
        source,
        w,
        |v, w| target_plan(v, source, w),
    )?;
    map_patch(
        &mut target.text_normalizers,
        patch.text_normalizers(),
        source,
        w,
        |v, w| normalizer_plan(v, source, w),
    )?;
    queries_patch(&mut target.queries, patch.queries(), source, inherited, w)?;
    embedded_patch(&mut target.embedded, patch.embedded(), source, inherited, w)?;
    match patch.exports() {
        Presence::Missing => {}
        Presence::Clear { .. } => target.exports = Planned::default(ExportsPlan::empty()),
        Presence::Set(v) => {
            target.exports = Planned::declared(
                exports_plan(v.value(), source, w)?,
                site(source, v.location()),
            )
        }
    }
    complexity_patch(&mut target.complexity, patch.complexity(), source, w)?;
    let derive = match patch.grammar() {
        Presence::Missing => !inherited,
        Presence::Clear { .. } => return Err(unexpected()),
        Presence::Set(v) if !v.value().is_empty() => {
            set_text(&mut target.grammar, patch.grammar(), source, w)?;
            false
        }
        Presence::Set(_) => true,
    };
    if derive {
        if !target.language.value.second.is_empty() {
            return Err(unexpected());
        }
        let prefix = match target.parser.value {
            ProfileParser::TreeSitter => "tree-sitter-",
            ProfileParser::Antlr4 => "antlr-",
        };
        let mut sources = Vec::new();
        collect_sites(w, &mut sources, &target.language.provenance)?;
        collect_sites(w, &mut sources, &target.parser.provenance)?;
        if let Presence::Set(v) = patch.grammar() {
            collect_sites(
                w,
                &mut sources,
                &Provenance::Declared(site(source, v.location())),
            )?;
        }
        target.grammar = Planned {
            value: Text {
                first: prefix,
                second: target.language.value.first,
            },
            provenance: Provenance::Derived(sources),
        };
    }
    route_text(target.language.value, CatalogErrorCode::InvalidSelector, w)?;
    route_text(target.grammar.value, CatalogErrorCode::InvalidSelector, w)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileLayerKind {
    Runtime,
    User,
    Project,
}
impl ProfileLayerKind {
    fn rank(self) -> usize {
        match self {
            Self::Runtime => 0,
            Self::User => 1,
            Self::Project => 2,
        }
    }
}
pub struct ProfileLayer<'a> {
    pub kind: ProfileLayerKind,
    pub profiles: &'a [DecodedProfile],
}
pub struct CatalogDraft {
    profiles: Vec<EffectiveProfile>,
    sources: Vec<ProfileId>,
    limits: CatalogLimits,
}
fn decoded_language(profile: &DecodedProfile) -> Result<&Located<String>, CatalogError> {
    match profile.patch().language() {
        Presence::Set(v) => Ok(v),
        _ => Err(unexpected()),
    }
}
fn diagnostic(
    code: CatalogErrorCode,
    first: &Provenance,
    second: &Provenance,
    path: &'static str,
) -> CatalogError {
    let mut error = CatalogError::new(code);
    error.site = first.first();
    error.conflicting_site = second.first();
    error.schema_path = Some(path);
    error
}
fn validate_plans(plans: &[EffectiveProfilePlan<'_>], w: &Walk<'_>) -> Result<(), CatalogError> {
    for (index, profile) in plans.iter().enumerate() {
        w.check()?;
        for other in &plans[..index] {
            if w.equal(profile.grammar.value, other.grammar.value, true)? {
                return Err(diagnostic(
                    CatalogErrorCode::DuplicateGrammar,
                    &other.grammar.provenance,
                    &profile.grammar.provenance,
                    "grammar",
                ));
            }
            if profile.exclusive.value
                || other.exclusive.value
                || profile.parser.value != other.parser.value
            {
                continue;
            }
            for extension in &profile.extensions.value.values {
                for prior in &other.extensions.value.values {
                    if w.equal(extension.value, prior.value, true)? {
                        return Err(diagnostic(
                            CatalogErrorCode::AmbiguousSelector,
                            &prior.provenance,
                            &extension.provenance,
                            "extensions",
                        ));
                    }
                }
            }
        }
        for extension in &profile.extensions.value.values {
            route_text(extension.value, CatalogErrorCode::InvalidSelector, w)?;
        }
        for embedded in &profile.embedded.value.values {
            for (position, (key, _)) in embedded.value.languages.value.entries.iter().enumerate() {
                for (other, _) in &embedded.value.languages.value.entries[..position] {
                    if w.equal(key.value, other.value, true)? {
                        return Err(diagnostic(
                            CatalogErrorCode::AmbiguousSelector,
                            &other.provenance,
                            &key.provenance,
                            "embedded.languages",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
impl CatalogDraft {
    pub fn profiles(&self) -> &[EffectiveProfile] {
        &self.profiles
    }
    pub fn source_profile(&self, source: SourceIndex) -> Option<&ProfileId> {
        self.sources.get(source.0 as usize)
    }

    pub fn build(
        layers: &[ProfileLayer<'_>],
        limits: &CatalogLimits,
        original: &SnapshotContext,
    ) -> Result<Self, CatalogError> {
        let w = Walk { original };
        w.check()?;
        let mut source_base = 0usize;
        let mut previous = None;
        for (layer_index, layer) in layers.iter().enumerate() {
            w.check()?;
            if previous.is_some_and(|v| v >= layer.kind.rank()) {
                let mut error = CatalogError::new(CatalogErrorCode::InvalidLayer);
                error.layer_index = u32::try_from(layer_index).ok();
                return Err(error);
            }
            previous = Some(layer.kind.rank());
            for (profile_index, profile) in layer.profiles.iter().enumerate() {
                w.check()?;
                let language = decoded_language(profile)?;
                for (prior_index, prior) in layer.profiles[..profile_index].iter().enumerate() {
                    let first = decoded_language(prior)?;
                    if w.equal(Text::one(language.value()), Text::one(first.value()), true)? {
                        let mut error = CatalogError::new(CatalogErrorCode::DuplicateLanguage);
                        error.site = Some(site(
                            SourceIndex(
                                u32::try_from(plus(source_base, prior_index)?)
                                    .map_err(|_| overflow())?,
                            ),
                            first.location(),
                        ));
                        error.conflicting_site = Some(site(
                            SourceIndex(
                                u32::try_from(plus(source_base, profile_index)?)
                                    .map_err(|_| overflow())?,
                            ),
                            language.location(),
                        ));
                        error.layer_index = u32::try_from(layer_index).ok();
                        error.profile_index = u32::try_from(profile_index).ok();
                        error.schema_path = Some("language");
                        return Err(error);
                    }
                }
            }
            source_base = plus(source_base, layer.profiles.len())?;
        }
        // The temporary typed plan borrows decoded text until materialization.
        let mut plans: Vec<EffectiveProfilePlan<'_>> = Vec::new();
        let mut source_number = 0usize;
        for layer in layers {
            for profile in layer.profiles {
                w.check()?;
                let source = SourceIndex(u32::try_from(source_number).map_err(|_| overflow())?);
                source_number = plus(source_number, 1)?;
                let language = decoded_language(profile)?;
                let mut position = None;
                for (index, prior) in plans.iter().enumerate() {
                    if w.equal(prior.language.value, Text::one(language.value()), true)? {
                        position = Some(index);
                        break;
                    }
                }
                let merge = match profile.patch().merge() {
                    Presence::Missing => false,
                    Presence::Set(v) => *v.value(),
                    Presence::Clear { .. } => return Err(unexpected()),
                };
                if let Some(index) = position {
                    if !merge {
                        plans[index] = EffectiveProfilePlan::empty();
                    }
                    apply_profile(&mut plans[index], profile.patch(), source, merge, &w)?;
                } else {
                    let mut plan = EffectiveProfilePlan::empty();
                    apply_profile(&mut plan, profile.patch(), source, false, &w)?;
                    w.push(&mut plans, plan)?;
                }
            }
        }
        validate_plans(&plans, &w)?;
        let mut sources = Vec::new();
        w.reserve(&mut sources, source_number)?;
        for layer in layers {
            for input in layer.profiles {
                let label = input.id().as_str();
                w.check()?;
                let source = ProfileId::new(label, label.len()).map_err(|error| {
                    CatalogError::new(match error {
                        ProfileIdError::AllocationDenied => CatalogErrorCode::AllocationDenied,
                        ProfileIdError::InputTooLarge => CatalogErrorCode::QuotaExceeded,
                        ProfileIdError::InvalidLimits | ProfileIdError::InvalidId => {
                            CatalogErrorCode::InvalidLayer
                        }
                    })
                })?;
                sources.push(source);
                w.check()?;
            }
        }
        let mut profiles = Vec::new();
        w.reserve(&mut profiles, plans.len())?;
        for plan in plans {
            w.check()?;
            profiles.push(plan.materialize(&w)?);
        }
        w.check()?;
        Ok(Self {
            profiles,
            sources,
            limits: *limits,
        })
    }
}

pub struct SelectionPolicy {
    allow: Vec<String>,
    deny: Vec<String>,
}
impl SelectionPolicy {
    pub fn new(
        allow: Vec<String>,
        deny: Vec<String>,
        limits: &CatalogLimits,
        original: &SnapshotContext,
    ) -> Result<Self, CatalogError> {
        let w = Walk { original };
        w.check()?;
        bounded(plus(allow.len(), deny.len())?, limits.policy_entries())?;
        for group in [&allow, &deny] {
            for (index, value) in group.iter().enumerate() {
                bounded(value.len(), limits.selector_bytes())?;
                route_text(Text::one(value), CatalogErrorCode::InvalidPolicy, &w)?;
                for previous in &group[..index] {
                    if w.equal(Text::one(value), Text::one(previous), true)? {
                        return Err(CatalogError::new(CatalogErrorCode::InvalidPolicy));
                    }
                }
            }
        }
        w.check()?;
        Ok(Self { allow, deny })
    }
    fn permits(&self, profile: &EffectiveProfile, w: &Walk<'_>) -> Result<bool, CatalogError> {
        for value in &self.deny {
            if alias_matches(profile, Text::one(value), w)? {
                return Ok(false);
            }
        }
        if self.allow.is_empty() {
            return w.check().map(|()| true);
        }
        for value in &self.allow {
            if alias_matches(profile, Text::one(value), w)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct FileSelector<'a> {
    pub basename: &'a str,
    pub extension: Option<&'a str>,
    pub explicit_grammar: Option<&'a str>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionSkip {
    NoSelector,
    Filtered,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddedLanguageDecision<'a> {
    Skip,
    Language(&'a str),
}
impl Embedded {
    pub fn mapped_language<'a>(
        &'a self,
        captured: &str,
        limits: &CatalogLimits,
        original: &SnapshotContext,
    ) -> Result<Option<EmbeddedLanguageDecision<'a>>, CatalogError> {
        let w = Walk { original };
        w.check()?;
        bounded(captured.len(), limits.selector_bytes())?;
        w.check()?;
        if captured.as_bytes().contains(&0) {
            return Err(CatalogError::new(CatalogErrorCode::InvalidSelector));
        }
        if self.languages.value().is_empty() {
            w.check()?;
            return Ok(None);
        }
        for (key, value) in self.languages.value().iter() {
            if w.equal(Text::one(key.value()), Text::one(captured), true)? {
                w.check()?;
                return Ok(Some(if value.value().is_empty() {
                    EmbeddedLanguageDecision::Skip
                } else {
                    EmbeddedLanguageDecision::Language(value.value())
                }));
            }
        }
        w.check()?;
        Ok(Some(EmbeddedLanguageDecision::Skip))
    }
}
pub enum Selection<'a> {
    Candidates(Vec<&'a EffectiveProfile>),
    Skip(SelectionSkip),
}
fn alias_matches(
    profile: &EffectiveProfile,
    selector: Text<'_>,
    w: &Walk<'_>,
) -> Result<bool, CatalogError> {
    if w.equal(Text::one(profile.language.value()), selector, true)? {
        return Ok(true);
    }
    let grammar = Text::one(profile.grammar.value());
    if w.equal(grammar, selector, true)? {
        return Ok(true);
    }
    w.equal(grammar.bare(), selector, true)
}
fn automatic_matches(
    profile: &EffectiveProfile,
    selector: FileSelector<'_>,
    w: &Walk<'_>,
) -> Result<bool, CatalogError> {
    if *profile.exclusive.value() {
        return Ok(false);
    }
    for route in profile.extensions.value().iter() {
        let value = route.value();
        if value.starts_with('.') {
            if let Some(extension) = selector.extension
                && w.equal(Text::one(value), Text::one(extension), true)?
            {
                return Ok(true);
            }
        } else if w.equal(Text::one(value), Text::one(selector.basename), true)? {
            return Ok(true);
        }
    }
    Ok(false)
}
impl CatalogDraft {
    pub fn resolve_file<'d>(
        &'d self,
        selector: FileSelector<'_>,
        policy: &SelectionPolicy,
        original: &SnapshotContext,
    ) -> Result<Selection<'d>, CatalogError> {
        let w = Walk { original };
        w.check()?;
        bounded(selector.basename.len(), self.limits.selector_bytes())?;
        w.check()?;
        if selector.basename.as_bytes().contains(&0) {
            return Err(CatalogError::new(CatalogErrorCode::InvalidSelector));
        }
        if let Some(extension) = selector.extension {
            bounded(extension.len(), self.limits.selector_bytes())?;
            route_text(Text::one(extension), CatalogErrorCode::InvalidSelector, &w)?;
            if !extension.starts_with('.') {
                return Err(CatalogError::new(CatalogErrorCode::InvalidSelector));
            }
        }
        if let Some(explicit) = selector.explicit_grammar {
            bounded(explicit.len(), self.limits.selector_bytes())?;
            route_text(Text::one(explicit), CatalogErrorCode::InvalidSelector, &w)?;
            let mut selected: Option<&EffectiveProfile> = None;
            for profile in &self.profiles {
                if w.equal(
                    Text::one(profile.grammar.value()),
                    Text::one(explicit),
                    true,
                )? {
                    selected = Some(profile);
                    break;
                }
            }
            if selected.is_none() {
                for profile in &self.profiles {
                    if alias_matches(profile, Text::one(explicit), &w)? {
                        if let Some(previous) = selected {
                            return Err(diagnostic(
                                CatalogErrorCode::AmbiguousSelector,
                                &previous.grammar.provenance,
                                &profile.grammar.provenance,
                                "grammar",
                            ));
                        }
                        selected = Some(profile);
                    }
                }
            }
            let profile =
                selected.ok_or_else(|| CatalogError::new(CatalogErrorCode::UnknownGrammar))?;
            if !policy.permits(profile, &w)? {
                return Err(CatalogError::new(CatalogErrorCode::DisabledGrammar));
            }
            let mut candidates = Vec::new();
            w.push(&mut candidates, profile)?;
            w.check()?;
            return Ok(Selection::Candidates(candidates));
        }
        let mut matches: [Option<&EffectiveProfile>; 2] = [None, None];
        for profile in &self.profiles {
            w.check()?;
            if automatic_matches(profile, selector, &w)? {
                let slot = match profile.parser.value() {
                    ProfileParser::TreeSitter => 0,
                    ProfileParser::Antlr4 => 1,
                };
                if let Some(previous) = matches[slot] {
                    return Err(diagnostic(
                        CatalogErrorCode::AmbiguousSelector,
                        &previous.extensions.provenance,
                        &profile.extensions.provenance,
                        "extensions",
                    ));
                }
                matches[slot] = Some(profile);
            }
        }
        if matches.iter().all(Option::is_none) {
            w.check()?;
            return Ok(Selection::Skip(SelectionSkip::NoSelector));
        }
        let mut candidates = Vec::new();
        for profile in matches.into_iter().flatten() {
            if policy.permits(profile, &w)? {
                w.push(&mut candidates, profile)?;
            }
        }
        w.check()?;
        if candidates.is_empty() {
            Ok(Selection::Skip(SelectionSkip::Filtered))
        } else {
            Ok(Selection::Candidates(candidates))
        }
    }
}
