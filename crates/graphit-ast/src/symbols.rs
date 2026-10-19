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
//!     doc.symbol("Value", subject, &[]).unwrap()
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
//!     let key = doc.symbol("Value", subject, &[]).unwrap();
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
//!     let owner = doc.symbol("Pair", parent, &[])?;
//!     let child = doc.symbol("Value", subject, &[&owner])?;
//!     drop(owner);
//!     Ok(child)
//! }
//! ```

use crate::{RangeError, SourceRange, SourceText};
use graphit_core::file_io::SnapshotContext;
use serde::{Serialize, Serializer, ser::SerializeSeq};
use std::{
    error::Error,
    fmt,
    io::{self, Write},
};

const TAG: &str = "ast-syntax-root-1";

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

/// Borrows unchanged logical metadata, original bytes and original context.
pub struct IdentityDocument<'o, 'request> {
    input: IdentityDocumentInput<'o>,
    source: SourceText<'o>,
    limits: IdentityLimits,
    original: &'request SnapshotContext,
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
        Ok(Self {
            input,
            source,
            limits,
            original,
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
        let wire = (
            TAG,
            self.relative_components(),
            self.language(),
            self.grammar(),
            label,
            [as_u64(subject.start())?, as_u64(subject.end())?],
            WireOwners(owners),
        );
        let length = serialize_wire(&wire, self.limits.uid_bytes(), None, original)?;
        if length == 0 {
            return Err(invariant());
        }
        let mut output = Vec::new();
        reserve(&mut output, length, original)?;
        let written = serialize_wire(&wire, self.limits.uid_bytes(), Some(&mut output), original)?;
        if written != length || output.len() != length {
            return Err(invariant());
        }
        check_context(original)?;
        let uid = SyntaxUid {
            value: String::from_utf8(output).map_err(|_| invariant())?,
        };
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
}
impl SyntaxUid {
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

// Owner offsets are checked in symbol validation before either SDK pass. The wrapper
// borrows existing keys; it allocates no temporary tuple or JSON value vector.
struct WireOwners<'a, 'doc, 'o, 'request>(&'a [&'a SyntaxIdentity<'doc, 'o, 'request>]);
impl Serialize for WireOwners<'_, '_, '_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for owner in self.0 {
            sequence.serialize_element(&(
                owner.label(),
                owner.subject.start() as u64,
                owner.subject.end() as u64,
            ))?;
        }
        sequence.end()
    }
}
struct BoundedWriter<'a> {
    original: &'a SnapshotContext,
    output: Option<&'a mut Vec<u8>>,
    cap: usize,
    count: usize,
    first_error: Option<IdentityError>,
}
impl BoundedWriter<'_> {
    fn fail(&mut self, error: IdentityError) -> io::Error {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
        io::Error::from(io::ErrorKind::Other)
    }
    fn chunk(&mut self, bytes: &[u8]) -> Result<usize, IdentityError> {
        if bytes.is_empty() {
            return Ok(0);
        }
        check_context(self.original)?;

        let next = self.count.checked_add(bytes.len()).ok_or_else(overflow)?;
        cap(next, self.cap, IdentityField::UidBytes, None)?;
        if let Some(output) = self.output.as_mut() {
            let available = output
                .capacity()
                .checked_sub(output.len())
                .ok_or_else(invariant)?;
            if bytes.len() > available {
                return Err(invariant());
            }
            output.extend_from_slice(bytes);
        }
        self.count = next;
        Ok(bytes.len())
    }
}
impl Write for BoundedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.first_error.is_some() {
            return Err(io::Error::from(io::ErrorKind::Other));
        }
        match self.chunk(bytes) {
            Ok(n) => Ok(n),
            Err(error) => Err(self.fail(error)),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.first_error.is_some() {
            return Err(io::Error::from(io::ErrorKind::Other));
        }
        match check_context(self.original) {
            Ok(()) => Ok(()),
            Err(error) => Err(self.fail(error)),
        }
    }
}
fn serialize_wire(
    wire: &impl Serialize,
    maximum: usize,
    output: Option<&mut Vec<u8>>,
    original: &SnapshotContext,
) -> Result<usize, IdentityError> {
    check_context(original)?;
    let mut writer = BoundedWriter {
        original,
        output,
        cap: maximum,
        count: 0,
        first_error: None,
    };
    let sdk = serde_json::to_writer(&mut writer, wire);
    if let Some(error) = writer.first_error {
        return Err(error);
    }
    sdk.map_err(|_| IdentityError::new(IdentityErrorCode::Serialization, None, None))?;
    cap(writer.count, maximum, IdentityField::UidBytes, None)?;
    Ok(writer.count)
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

    fn limits() -> IdentityLimits {
        IdentityLimits::new(IdentityLimitsInput {
            source_bytes: 4096,
            path_components: 16,
            path_component_bytes: 256,
            identity_text_bytes: 256,
            owner_depth: 16,
            uid_bytes: 4096,
        })
        .unwrap()
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

    #[test]
    fn first_other_error_latches_without_retry_or_further_effect() {
        let ctx = context();
        let mut output = Vec::new();
        reserve(&mut output, 1, &ctx).unwrap();
        let mut writer = BoundedWriter {
            original: &ctx,
            output: Some(&mut output),
            cap: 1,
            count: 0,
            first_error: None,
        };
        writer.write_all(b"a").unwrap();
        assert_eq!(
            writer.write_all(b"bc").unwrap_err().kind(),
            io::ErrorKind::Other
        );
        let error = writer.first_error.unwrap();
        assert_eq!(error.code(), IdentityErrorCode::LimitExceeded);
        assert_eq!(error.field(), Some(IdentityField::UidBytes));
        for _ in 0..3 {
            assert_eq!(
                writer.write_all(b"z").unwrap_err().kind(),
                io::ErrorKind::Other
            );
            assert_eq!(writer.write(b"").unwrap_err().kind(), io::ErrorKind::Other);
            assert_eq!(writer.flush().unwrap_err().kind(), io::ErrorKind::Other);
        }
        assert_eq!(writer.first_error, Some(error));
        assert_eq!(writer.count, 1);
        assert_eq!(writer.output.as_deref().unwrap().as_slice(), b"a");
    }

    #[test]
    fn writer_empty_flush_retains_original_context_error() {
        let signal = Cancellation::default();
        let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(60), signal.clone());
        let mut writer = BoundedWriter {
            original: &ctx,
            output: None,
            cap: 100,
            count: 0,
            first_error: None,
        };
        assert_eq!(writer.write(b"").unwrap(), 0);
        writer.flush().unwrap();
        signal.cancel();
        assert_eq!(writer.write(b"").unwrap(), 0);
        assert_eq!(writer.flush().unwrap_err().kind(), io::ErrorKind::Other);
        assert_eq!(
            writer.first_error.unwrap().code(),
            IdentityErrorCode::Cancelled
        );
        assert_eq!(writer.count, 0);
    }

    #[test]
    fn every_manual_wire_oracle_has_exact_two_pass_output_and_uid_cap() {
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/symbols/expected-identity.json"
        ))
        .unwrap();
        let rows = expected["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 15);
        let ctx = context();
        for row in rows {
            let tuple = &row["typed_tuple"];
            let paths: Vec<&str> = tuple[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            let bytes = match row["original_fixture"].as_str().unwrap() {
                "go.txt" => &include_bytes!("../tests/fixtures/symbols/go.txt")[..],
                "same-line.js" => &include_bytes!("../tests/fixtures/symbols/same-line.js")[..],
                "unicode-crlf.js" => {
                    &include_bytes!("../tests/fixtures/symbols/unicode-crlf.js")[..]
                }
                "values.json" => &include_bytes!("../tests/fixtures/symbols/values.json")[..],
                _ => panic!("unknown oracle fixture"),
            };
            let doc = IdentityDocument::new(
                IdentityDocumentInput {
                    relative_components: &paths,
                    language: tuple[2].as_str().unwrap(),
                    grammar: tuple[3].as_str().unwrap(),
                    original_bytes: bytes,
                },
                limits(),
                &ctx,
            )
            .unwrap();
            let source = SourceText::new(bytes, 4096).unwrap();
            let owner_tuple = tuple[6].as_array().unwrap().first();
            let owner = owner_tuple.map(|o| {
                doc.symbol(
                    o[0].as_str().unwrap(),
                    source
                        .range(
                            o[1].as_u64().unwrap() as usize,
                            o[2].as_u64().unwrap() as usize,
                        )
                        .unwrap(),
                    &[],
                )
                .unwrap()
            });
            let owners: Vec<_> = owner.iter().collect();
            let wire = (
                TAG,
                &paths,
                doc.language(),
                doc.grammar(),
                tuple[4].as_str().unwrap(),
                [tuple[5][0].as_u64().unwrap(), tuple[5][1].as_u64().unwrap()],
                WireOwners(&owners),
            );
            let expected_uid = row["expected_uid"].as_str().unwrap().as_bytes();
            let count = serialize_wire(&wire, 4096, None, &ctx).unwrap();
            assert_eq!(count, expected_uid.len());
            let mut output = Vec::new();
            reserve(&mut output, count, &ctx).unwrap();
            assert_eq!(
                serialize_wire(&wire, 4096, Some(&mut output), &ctx).unwrap(),
                count
            );
            assert_eq!(output.as_slice(), expected_uid);
            let error = serialize_wire(&wire, count - 1, None, &ctx).unwrap_err();
            assert_eq!(error.code(), IdentityErrorCode::LimitExceeded);
            assert_eq!(error.field(), Some(IdentityField::UidBytes));
        }
    }
}
