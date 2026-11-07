//! Logical identities with borrowed original ranges and bounded UID representation.
//! Owners are declared frames; keys retain their document and request context.
//!
//! Catalog metadata cannot escape a local catalog:
//! ```compile_fail,E0515
//! use graphit_ast::{catalog::*, profiles::*, symbols::*};
//! use graphit_core::file_io::SnapshotContext;
//! fn escape<'r>(profiles: &[DecodedProfile], ctx: &'r SnapshotContext,
//!     catalog_limits: &CatalogLimits, limits: IdentityLimits) -> IdentityDocument<'static, 'r> {
//!     let catalog = CatalogDraft::build(&[ProfileLayer {
//!         kind: ProfileLayerKind::Runtime, profiles,
//!     }], catalog_limits, ctx).unwrap();
//!     let profile = catalog.profiles().iter().next().unwrap();
//!     IdentityDocument::new(IdentityDocumentInput {
//!         relative_components: &["a.go"], original_bytes: b"x",
//!         language: profile.language().value().as_str(), grammar: profile.grammar().value().as_str(),
//!     }, limits, ctx).unwrap()
//! }
//! ```
//!
//! A key cannot escape its document:
//! ```compile_fail,E0515
//! use graphit_ast::{SourceText, symbols::*};
//! use graphit_core::file_io::SnapshotContext;
//! fn escape<'o, 'r>(input: IdentityDocumentInput<'o>, limits: IdentityLimits,
//!     ctx: &'r SnapshotContext) -> SyntaxIdentity<'o, 'o, 'r> {
//!     let subject = SourceText::new(input.original_bytes, limits.source_bytes()).unwrap().range(0, 1).unwrap();
//!     let doc = IdentityDocument::new(input, limits, ctx).unwrap();
//!     doc.symbol("Value", subject, &[], LogicalDeclaration::named(subject.text())).unwrap()
//! }
//! ```
//!
//! A UID view cannot escape its key:
//! ```compile_fail,E0515
//! use graphit_ast::symbols::SyntaxIdentity;
//! fn escape<'a>(key: SyntaxIdentity<'a, 'a, 'a>) -> &'a str { key.uid().as_str() }
//! ```
//!
//! Original bytes stay borrowed while the key is used:
//! ```compile_fail,E0502
//! use graphit_ast::{SourceText, symbols::*};
//! use graphit_core::file_io::SnapshotContext;
//! fn mutate<'r>(raw: &mut String, ctx: &'r SnapshotContext, limits: IdentityLimits) {
//!     let doc = IdentityDocument::new(IdentityDocumentInput {
//!         relative_components: &["x"], language: "json", grammar: "g", original_bytes: raw.as_bytes(),
//!     }, limits, ctx).unwrap();
//!     let subject = SourceText::new(raw.as_bytes(), limits.source_bytes()).unwrap().range(0, 1).unwrap();
//!     let key = doc.symbol("Value", subject, &[], LogicalDeclaration::named(subject.text())).unwrap();
//!     raw.push('!');
//!     assert_eq!(key.subject().text(), "x");
//! }
//! ```
//!
//! The document retains its original context:
//! ```compile_fail,E0505
//! use graphit_ast::symbols::*;
//! use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
//! use std::time::{Duration, Instant};
//! fn drop_context(limits: IdentityLimits) {
//!     let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(60), Cancellation::default());
//!     let doc = IdentityDocument::new(IdentityDocumentInput {
//!         relative_components: &["x"], language: "json", grammar: "g", original_bytes: b"x",
//!     }, limits, &ctx).unwrap();
//!     drop(ctx);
//!     assert_eq!(doc.language(), "json");
//! }
//! ```
//!
//! Owner keys may be dropped after their frames are copied:
//! ```
//! use graphit_ast::{SourceRange, symbols::*};
//! fn child<'doc, 'o, 'r>(doc: &'doc IdentityDocument<'o, 'r>,
//!     parent: SourceRange<'o>, subject: SourceRange<'o>) -> Result<SyntaxIdentity<'doc, 'o, 'r>, IdentityError> {
//!     let owner = doc.symbol("Pair", parent, &[], LogicalDeclaration::named(parent.text()))?;
//!     let child = doc.symbol("Value", subject, &[&owner], LogicalDeclaration::named(subject.text()))?;
//!     drop(owner);
//!     Ok(child)
//! }
//! ```

use crate::{RangeError, SourceRange, SourceText};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{CODE_ID_TEXT_BYTES, CodeDeclaration, CodeFileId, CodeScope, CodeUid};
use std::{collections::HashMap, error::Error, fmt, fmt::Write as _, io, sync::Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityErrorCode {
    InvalidLimits,
    InvalidDocument,
    InvalidGrammar,
    InvalidLabel,
    InvalidAnchor,
    InvalidContext,
    InvalidUtf8,
    LimitExceeded,
    ArithmeticOverflow,
    Cancelled,
    DeadlineExceeded,
    ContextDenied,
    Allocation,
    Serialization,
    InvariantViolation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityField {
    SourceBytes,
    PathComponents,
    PathComponentBytes,
    IdentityTextBytes,
    OwnerDepth,
    UidBytes,
    Language,
    Grammar,
    Label,
    Owners,
    Uid,
}

/// Closed diagnostics retain no source text, path, label or SDK error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdentityError {
    code: IdentityErrorCode,
    field: Option<IdentityField>,
    index: Option<u32>,
    range_error: Option<RangeError>,
}
impl IdentityError {
    pub fn code(&self) -> IdentityErrorCode {
        self.code
    }
    pub fn field(&self) -> Option<IdentityField> {
        self.field
    }
    pub fn index(&self) -> Option<u32> {
        self.index
    }
    pub fn range_error(&self) -> Option<RangeError> {
        self.range_error
    }
    fn new(code: IdentityErrorCode, field: Option<IdentityField>, index: Option<u32>) -> Self {
        Self {
            code,
            field,
            index,
            range_error: None,
        }
    }
}
impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.code {
            IdentityErrorCode::InvalidLimits => "identity limits are invalid",
            IdentityErrorCode::InvalidDocument => "identity document is invalid",
            IdentityErrorCode::InvalidGrammar => "identity grammar is invalid",
            IdentityErrorCode::InvalidLabel => "identity label is invalid",
            IdentityErrorCode::InvalidAnchor => "identity anchor is invalid",
            IdentityErrorCode::InvalidContext => "identity owners are invalid",
            IdentityErrorCode::InvalidUtf8 => "identity source is not UTF-8",
            IdentityErrorCode::LimitExceeded => "identity exceeds its structural limit",
            IdentityErrorCode::ArithmeticOverflow => "identity arithmetic overflow",
            IdentityErrorCode::Cancelled => "identity operation cancelled",
            IdentityErrorCode::DeadlineExceeded => "identity deadline exceeded",
            IdentityErrorCode::ContextDenied => "identity original context denied",
            IdentityErrorCode::Allocation => "identity reservation failed",
            IdentityErrorCode::Serialization => "identity serialization failed",
            IdentityErrorCode::InvariantViolation => "identity invariant violated",
        })
    }
}
impl Error for IdentityError {}

#[derive(Clone, Copy)]
pub struct IdentityLimitsInput {
    pub source_bytes: usize,
    pub path_components: usize,
    pub path_component_bytes: usize,
    pub identity_text_bytes: usize,
    pub owner_depth: usize,
    pub uid_bytes: usize,
}
#[derive(Clone, Copy)]
pub struct IdentityLimits {
    input: IdentityLimitsInput,
}
impl IdentityLimits {
    pub fn new(input: IdentityLimitsInput) -> Result<Self, IdentityError> {
        for (value, field, count) in [
            (input.source_bytes, IdentityField::SourceBytes, false),
            (input.path_components, IdentityField::PathComponents, true),
            (
                input.path_component_bytes,
                IdentityField::PathComponentBytes,
                false,
            ),
            (
                input.identity_text_bytes,
                IdentityField::IdentityTextBytes,
                false,
            ),
            (input.owner_depth, IdentityField::OwnerDepth, true),
            (input.uid_bytes, IdentityField::UidBytes, false),
        ] {
            if value == 0 || (count && u32::try_from(value).is_err()) {
                return Err(IdentityError::new(
                    IdentityErrorCode::InvalidLimits,
                    Some(field),
                    None,
                ));
            }
        }
        Ok(Self { input })
    }
    pub fn source_bytes(&self) -> usize {
        self.input.source_bytes
    }
    pub fn path_components(&self) -> usize {
        self.input.path_components
    }
    pub fn path_component_bytes(&self) -> usize {
        self.input.path_component_bytes
    }
    pub fn identity_text_bytes(&self) -> usize {
        self.input.identity_text_bytes
    }
    pub fn owner_depth(&self) -> usize {
        self.input.owner_depth
    }
    pub fn uid_bytes(&self) -> usize {
        self.input.uid_bytes
    }
}

pub struct IdentityDocumentInput<'o> {
    pub relative_components: &'o [&'o str],
    pub language: &'o str,
    pub grammar: &'o str,
    pub original_bytes: &'o [u8],
}

/// Semantic fields observed by the native producer before identity admission.
/// Empty names are valid observations, for example a decoded JSON object key.
#[derive(Clone, Copy)]
pub struct LogicalDeclaration<'a> {
    pub name: &'a str,
    pub declared_type: Option<&'a str>,
    pub anonymous: bool,
}
impl<'a> LogicalDeclaration<'a> {
    pub fn named(name: &'a str) -> Self {
        Self {
            name,
            declared_type: None,
            anonymous: false,
        }
    }
}

type DeclarationGroups = HashMap<CodeUid, HashMap<(usize, usize), u64>>;

/// Borrows unchanged logical metadata, original bytes and original context.
pub struct IdentityDocument<'o, 'request> {
    input: IdentityDocumentInput<'o>,
    source: SourceText<'o>,
    limits: IdentityLimits,
    original: &'request SnapshotContext,
    file: CodeFileId,
    // Admission state for this parse only. Anchors bind repeated observations;
    // they never enter a digest or survive the document.
    declarations: Mutex<DeclarationGroups>,
}
impl<'o, 'request> IdentityDocument<'o, 'request> {
    pub fn new(
        input: IdentityDocumentInput<'o>,
        limits: IdentityLimits,
        original: &'request SnapshotContext,
    ) -> Result<Self, IdentityError> {
        check_context(original)?;
        cap(
            input.relative_components.len(),
            limits.path_components(),
            IdentityField::PathComponents,
            None,
        )?;
        if input.relative_components.is_empty() {
            return Err(IdentityError::new(
                IdentityErrorCode::InvalidDocument,
                Some(IdentityField::PathComponents),
                None,
            ));
        }
        // All inexpensive length admissions precede source UTF-8 scanning.
        check_context(original)?;
        cap(
            input.original_bytes.len(),
            limits.source_bytes(),
            IdentityField::SourceBytes,
            None,
        )?;
        as_u64(input.original_bytes.len())?;
        for (i, component) in input.relative_components.iter().enumerate() {
            check_context(original)?;
            let index = Some(as_u32(i)?);
            cap(
                component.len(),
                limits.path_component_bytes(),
                IdentityField::PathComponentBytes,
                index,
            )?;
            check_context(original)?;
            if component.is_empty()
                || *component == "."
                || *component == ".."
                || component
                    .as_bytes()
                    .iter()
                    .any(|byte| *byte == 0 || *byte == b'/')
            {
                return Err(IdentityError::new(
                    IdentityErrorCode::InvalidDocument,
                    Some(IdentityField::PathComponentBytes),
                    index,
                ));
            }
        }
        text(
            input.language,
            limits.identity_text_bytes(),
            IdentityField::Language,
            IdentityErrorCode::InvalidDocument,
            original,
        )?;
        text(
            input.grammar,
            limits.identity_text_bytes(),
            IdentityField::Grammar,
            IdentityErrorCode::InvalidGrammar,
            original,
        )?;
        check_context(original)?;
        let source =
            SourceText::new(input.original_bytes, limits.source_bytes()).map_err(|cause| {
                IdentityError {
                    code: if matches!(cause, RangeError::InvalidUtf8 { .. }) {
                        IdentityErrorCode::InvalidUtf8
                    } else {
                        IdentityErrorCode::InvariantViolation
                    },
                    field: Some(IdentityField::SourceBytes),
                    index: None,
                    range_error: Some(cause),
                }
            })?;
        let file =
            CodeFileId::from_components(input.relative_components).map_err(|_| invariant())?;
        Ok(Self {
            input,
            source,
            limits,
            original,
            file,
            declarations: Mutex::new(HashMap::new()),
        })
    }
    pub fn relative_components(&self) -> &'o [&'o str] {
        self.input.relative_components
    }
    pub fn language(&self) -> &'o str {
        self.input.language
    }
    pub fn grammar(&self) -> &'o str {
        self.input.grammar
    }
    pub fn limits(&self) -> IdentityLimits {
        self.limits
    }

    pub fn symbol<'doc>(
        &'doc self,
        label: &str,
        subject: SourceRange<'o>,
        owners: &[&SyntaxIdentity<'doc, 'o, 'request>],
        declaration: LogicalDeclaration<'_>,
    ) -> Result<SyntaxIdentity<'doc, 'o, 'request>, IdentityError> {
        let original = self.original;
        check_context(original)?;
        cap(
            owners.len(),
            self.limits.owner_depth(),
            IdentityField::OwnerDepth,
            None,
        )?;
        text(
            label,
            self.limits.identity_text_bytes(),
            IdentityField::Label,
            IdentityErrorCode::InvalidLabel,
            original,
        )?;
        self.anchor(subject, original)?;
        for (i, owner) in owners.iter().enumerate() {
            check_context(original)?;
            let invalid = || {
                IdentityError::new(
                    IdentityErrorCode::InvalidContext,
                    Some(IdentityField::Owners),
                    Some(i as u32),
                )
            };
            as_u64(owner.subject.start())?;
            as_u64(owner.subject.end())?;
            if !std::ptr::eq(owner.document, self) || owner.context.len() != i {
                return Err(invalid());
            }
            let self_label = equal_text(owner.label(), label, original)?;
            check_context(original)?;
            if self_label
                && owner.subject.start() == subject.start()
                && owner.subject.end() == subject.end()
            {
                return Err(invalid());
            }
            for (frame, prefix) in owner.context.iter().zip(&owners[..i]) {
                check_context(original)?;
                let same_label = equal_text(frame.label(), prefix.label(), original)?;
                check_context(original)?;
                if !same_label
                    || frame.start != prefix.subject.start()
                    || frame.end != prefix.subject.end()
                {
                    return Err(invalid());
                }
            }
        }
        cap(
            CODE_ID_TEXT_BYTES,
            self.limits.uid_bytes(),
            IdentityField::UidBytes,
            None,
        )?;
        cap(
            declaration.name.len(),
            self.limits.source_bytes(),
            IdentityField::SourceBytes,
            None,
        )?;
        if let Some(declared_type) = declaration.declared_type {
            cap(
                declared_type.len(),
                self.limits.source_bytes(),
                IdentityField::SourceBytes,
                None,
            )?;
        }
        let mut scopes = Vec::new();
        reserve(&mut scopes, owners.len(), original)?;
        for owner in owners {
            check_context(original)?;
            scopes.push(CodeScope::Declaration(owner.uid().code()));
        }
        let derive = |ordinal| {
            CodeUid::derive(
                self.file,
                CodeDeclaration::Fallback {
                    language: self.language(),
                    kind: label,
                    owners: &scopes,
                    name: declaration.name,
                    declared_type: declaration.declared_type,
                    anonymous: declaration.anonymous,
                    ordinal,
                },
            )
            .map_err(|_| invariant())
        };
        let group = derive(0)?;
        let mut value = String::new();
        value
            .try_reserve_exact(CODE_ID_TEXT_BYTES)
            .map_err(|_| IdentityError::new(IdentityErrorCode::Allocation, None, None))?;
        let owned_label = copy_text(label, original)?;
        let mut context = Vec::new();
        reserve(&mut context, owners.len(), original)?;
        for owner in owners {
            check_context(original)?;
            let frame = ContextFrame {
                label: copy_text(owner.label(), original)?,
                start: owner.subject.start(),
                end: owner.subject.end(),
            };
            check_context(original)?;
            context.push(frame);
        }
        let mut declarations = self.declarations.lock().map_err(|_| invariant())?;
        check_context(original)?;
        declarations
            .try_reserve(1)
            .map_err(|_| IdentityError::new(IdentityErrorCode::Allocation, None, None))?;
        let subjects = declarations.entry(group).or_default();
        let anchor = (subject.start(), subject.end());
        let admitted = subjects.get(&anchor).copied();
        let ordinal = admitted.unwrap_or(as_u64(subjects.len())?);
        let code = derive(ordinal)?;
        write!(&mut value, "{code}")
            .map_err(|_| IdentityError::new(IdentityErrorCode::Serialization, None, None))?;
        check_context(original)?;
        if admitted.is_none() {
            subjects
                .try_reserve(1)
                .map_err(|_| IdentityError::new(IdentityErrorCode::Allocation, None, None))?;
            subjects.insert(anchor, ordinal);
        }
        let uid = SyntaxUid { value, code };
        Ok(SyntaxIdentity {
            document: self,
            label: owned_label,
            subject,
            context,
            uid,
        })
    }

    fn anchor(
        &self,
        subject: SourceRange<'o>,
        original: &SnapshotContext,
    ) -> Result<(), IdentityError> {
        check_context(original)?;
        let invalid = || IdentityError::new(IdentityErrorCode::InvalidAnchor, None, None);
        if subject.start() >= subject.end() {
            return Err(invalid());
        }
        let length = subject
            .end()
            .checked_sub(subject.start())
            .ok_or_else(overflow)?;
        let bytes = self
            .input
            .original_bytes
            .get(subject.start()..subject.end())
            .ok_or_else(invalid)?;
        let actual = subject.text().as_bytes();
        // This binding MUST precede UTF-8 projection: a foreign ASCII end can
        // fall inside an original multibyte character.
        if bytes.len() != length || bytes.len() != actual.len() || bytes.as_ptr() != actual.as_ptr()
        {
            return Err(invalid());
        }
        check_context(original)?;
        let checked = self
            .source
            .range(subject.start(), subject.end())
            .map_err(|cause| IdentityError {
                code: IdentityErrorCode::InvariantViolation,
                field: None,
                index: None,
                range_error: Some(cause),
            })?;
        check_context(original)?;
        if checked.start() != subject.start()
            || checked.end() != subject.end()
            || checked.text().len() != actual.len()
            || checked.text().as_ptr() != actual.as_ptr()
            || checked.start_point() != subject.start_point()
            || checked.end_point() != subject.end_point()
        {
            return Err(invalid());
        }
        Ok(())
    }
}

pub struct SyntaxIdentity<'doc, 'o, 'request> {
    document: &'doc IdentityDocument<'o, 'request>,
    label: String,
    subject: SourceRange<'o>,
    context: Vec<ContextFrame>,
    uid: SyntaxUid,
}
impl<'doc, 'o, 'request> SyntaxIdentity<'doc, 'o, 'request> {
    pub fn document(&self) -> &'doc IdentityDocument<'o, 'request> {
        self.document
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn subject(&self) -> &SourceRange<'o> {
        &self.subject
    }
    pub fn context(&self) -> &[ContextFrame] {
        &self.context
    }
    pub fn uid(&self) -> &SyntaxUid {
        &self.uid
    }
}
pub struct ContextFrame {
    label: String,
    start: usize,
    end: usize,
}
impl ContextFrame {
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn start(&self) -> usize {
        self.start
    }
    pub fn end(&self) -> usize {
        self.end
    }
}
pub struct SyntaxUid {
    value: String,
    code: CodeUid,
}
impl SyntaxUid {
    pub fn code(&self) -> CodeUid {
        self.code
    }
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

fn invariant() -> IdentityError {
    IdentityError::new(IdentityErrorCode::InvariantViolation, None, None)
}
fn overflow() -> IdentityError {
    IdentityError::new(IdentityErrorCode::ArithmeticOverflow, None, None)
}
fn as_u64(value: usize) -> Result<u64, IdentityError> {
    u64::try_from(value).map_err(|_| overflow())
}
fn as_u32(value: usize) -> Result<u32, IdentityError> {
    u32::try_from(value).map_err(|_| overflow())
}
fn check_context(original: &SnapshotContext) -> Result<(), IdentityError> {
    original.check().map_err(|cause| {
        IdentityError::new(
            match cause.kind() {
                io::ErrorKind::Interrupted => IdentityErrorCode::Cancelled,
                io::ErrorKind::TimedOut => IdentityErrorCode::DeadlineExceeded,
                _ => IdentityErrorCode::ContextDenied,
            },
            None,
            None,
        )
    })
}
fn cap(
    length: usize,
    maximum: usize,
    field: IdentityField,
    index: Option<u32>,
) -> Result<(), IdentityError> {
    if length > maximum {
        Err(IdentityError::new(
            IdentityErrorCode::LimitExceeded,
            Some(field),
            index,
        ))
    } else {
        Ok(())
    }
}
fn text(
    value: &str,
    maximum: usize,
    field: IdentityField,
    code: IdentityErrorCode,
    original: &SnapshotContext,
) -> Result<(), IdentityError> {
    check_context(original)?;
    cap(value.len(), maximum, field, None)?;
    check_context(original)?;
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes.contains(&0)
        || bytes.first().is_some_and(u8::is_ascii_whitespace)
        || bytes.last().is_some_and(u8::is_ascii_whitespace)
    {
        Err(IdentityError::new(code, Some(field), None))
    } else {
        Ok(())
    }
}
fn equal_text(a: &str, b: &str, original: &SnapshotContext) -> Result<bool, IdentityError> {
    check_context(original)?;
    Ok(a == b)
}
fn reserve<T>(
    values: &mut Vec<T>,
    additional: usize,
    original: &SnapshotContext,
) -> Result<(), IdentityError> {
    check_context(original)?;
    values
        .try_reserve_exact(additional)
        .map_err(|_| IdentityError::new(IdentityErrorCode::Allocation, None, None))
}
fn copy_text(value: &str, original: &SnapshotContext) -> Result<String, IdentityError> {
    let mut bytes = Vec::new();
    reserve(&mut bytes, value.len(), original)?;
    check_context(original)?;
    bytes.extend_from_slice(value.as_bytes());
    check_context(original)?;
    String::from_utf8(bytes).map_err(|_| invariant())
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};

    fn context() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(60),
            Cancellation::default(),
        )
    }

    #[test]
    fn actual_vector_capacity_overflow_is_typed() {
        let ctx = context();
        let mut output: Vec<u8> = Vec::new();
        assert_eq!(
            reserve(&mut output, usize::MAX, &ctx).unwrap_err().code(),
            IdentityErrorCode::Allocation
        );
        assert!(output.is_empty());
        assert_eq!(output.capacity(), 0);
        let mut frames: Vec<ContextFrame> = Vec::new();
        assert_eq!(
            reserve(&mut frames, usize::MAX, &ctx).unwrap_err().code(),
            IdentityErrorCode::Allocation
        );
        assert!(frames.is_empty());
    }

    #[test]
    fn actual_reservation_and_copy_preserve_bytes() {
        let ctx = context();
        let mut output: Vec<u8> = Vec::new();
        reserve(&mut output, 74, &ctx).unwrap();
        assert!(output.capacity() >= 74);
        assert_eq!(copy_text("tree-sitter-x", &ctx).unwrap(), "tree-sitter-x");
    }
}
