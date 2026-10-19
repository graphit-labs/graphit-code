//! Bounded, formatting-preserving documents. Field edits confer no ownership.
use crate::policy::{RequestContext, SharedWriteLimits};
use jsonc_parser::{
    ast::Value as Ast,
    cst::{CstArray, CstInputValue, CstRootNode},
    tokens::Token,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Write},
    io,
    ops::Range,
};
mod hooks;
pub(crate) use hooks::HookMatch;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentFormat {
    Json,
    Jsonc,
    Toml,
}

/// Values the managed schema can generate. Foreign values stay in native nodes.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldValue {
    String(String),
    Boolean(bool),
    Integer(i64),
    Array(Vec<FieldValue>),
    Object(BTreeMap<String, FieldValue>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentError {
    InvalidPolicy,
    InvalidEncoding,
    InvalidDocument,
    DuplicateKey,
    LimitExceeded,
    InvalidPath,
    WrongType,
    Interrupted,
    TimedOut,
}
impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native document: {self:?}")
    }
}
impl std::error::Error for DocumentError {}
type Result<T> = std::result::Result<T, DocumentError>;

pub(crate) type TupleFields = BTreeMap<String, Option<FieldValue>>;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TupleMatch {
    Missing,
    Unique(usize),
    Ambiguous,
}
pub(crate) enum TupleEdit<'a> {
    Append(&'a FieldValue),
    Update(usize, &'a TupleFields),
    Remove(usize),
}

enum Tree {
    Json(CstRootNode),
    Toml(toml_edit::DocumentMut),
}
/// Holds the captured request; neither policy nor mutable native nodes are exposed.
pub struct NativeDocument<'r, 'c> {
    format: DocumentFormat,
    tree: Tree,
    limits: SharedWriteLimits,
    request: &'r RequestContext<'c>,
}
impl<'r, 'c> NativeDocument<'r, 'c> {
    pub fn parse(
        format: DocumentFormat,
        input: &[u8],
        limits: SharedWriteLimits,
        request: &'r RequestContext<'c>,
    ) -> Result<Self> {
        validate_policy(limits)?;
        check_request(request)?;
        if input.len() > limits.input_bytes {
            return Err(DocumentError::LimitExceeded);
        }
        let input = std::str::from_utf8(input).map_err(|_| DocumentError::InvalidEncoding)?;
        let tree = parse_tree(format, input, limits, request)?;
        Ok(Self {
            format,
            tree,
            limits,
            request,
        })
    }
    pub fn render(&self) -> Result<String> {
        render_tree(&self.tree, self.limits.output_bytes, self.request)
    }
    fn copy_text(&self) -> Result<String> {
        render_tree(
            &self.tree,
            self.limits.input_bytes.max(self.limits.output_bytes),
            self.request,
        )
    }
    pub fn get(&self, path: &[&str]) -> Result<Option<FieldValue>> {
        validate_path(path, self.limits)?;
        check_request(self.request)?;
        match &self.tree {
            Tree::Json(_) => {
                let text = self.copy_text()?;
                let parsed = jsonc_parser::parse_to_ast(
                    &text,
                    &Default::default(),
                    &json_options(self.format),
                )
                .map_err(|_| DocumentError::InvalidDocument)?;
                let mut value = parsed
                    .value
                    .as_ref()
                    .ok_or(DocumentError::InvalidDocument)?;
                for key in path {
                    let object = value.as_object().ok_or(DocumentError::WrongType)?;
                    let Some(prop) = object.get(key) else {
                        return Ok(None);
                    };
                    value = &prop.value;
                }
                json_field(value, self.request).map(Some)
            }
            Tree::Toml(doc) => {
                let mut item = doc.as_item();
                for key in path {
                    let table = item.as_table_like().ok_or(DocumentError::WrongType)?;
                    let Some(next) = table.get(key) else {
                        return Ok(None);
                    };
                    item = next;
                }
                toml_field(item, self.request).map(Some)
            }
        }
    }
    /// Edits commit in memory only after bounded serialization and revalidation.
    /// Missing object parents are created; existing nonobjects are never replaced.
    pub fn set(&mut self, path: &[&str], value: FieldValue) -> Result<()> {
        self.edit(path, Some(value))
    }
    /// Removes the exact field and retains parent containers and other fields.
    pub fn remove(&mut self, path: &[&str]) -> Result<()> {
        self.edit(path, None)
    }
    /// Match selected native fields, including absence, without decoding foreign values.
    pub(crate) fn tuple_match(&self, path: &[&str], fields: &TupleFields) -> Result<TupleMatch> {
        validate_path(path, self.limits)?;
        check_request(self.request)?;
        if !matches!(self.tree, Tree::Json(_)) {
            return Err(DocumentError::WrongType);
        }
        let text = self.copy_text()?;
        let parsed =
            jsonc_parser::parse_to_ast(&text, &Default::default(), &json_options(self.format))
                .map_err(|_| DocumentError::InvalidDocument)?;
        let mut value = parsed
            .value
            .as_ref()
            .ok_or(DocumentError::InvalidDocument)?;
        for key in path {
            let object = value.as_object().ok_or(DocumentError::WrongType)?;
            let Some(prop) = object.get(key) else {
                return Ok(TupleMatch::Missing);
            };
            value = &prop.value;
        }
        let array = value.as_array().ok_or(DocumentError::WrongType)?;
        let mut matched = TupleMatch::Missing;
        for (index, value) in array.elements.iter().enumerate() {
            check_request(self.request)?;
            let object = value.as_object().ok_or(DocumentError::WrongType)?;
            let mut equal = true;
            for (key, expected) in fields {
                let actual = object.get(key).map(|prop| &prop.value);
                equal &= match (actual, expected) {
                    (None, None) => true,
                    (Some(actual), Some(expected)) => json_equal(actual, expected, self.request)?,
                    _ => false,
                };
            }
            if equal {
                matched = match matched {
                    TupleMatch::Missing => TupleMatch::Unique(index),
                    _ => TupleMatch::Ambiguous,
                };
            }
        }
        check_request(self.request)?;
        Ok(matched)
    }
    /// The index is observed under the writer lock and is never stored as identity.
    pub(crate) fn edit_tuple(&mut self, path: &[&str], edit: TupleEdit<'_>) -> Result<()> {
        validate_path(path, self.limits)?;
        let mut budget = Budget::new(self.limits, self.request);
        match &edit {
            TupleEdit::Append(value) => {
                if !matches!(value, FieldValue::Object(_)) {
                    return Err(DocumentError::WrongType);
                }
                validate_field(value, 1, &mut budget)?;
            }
            TupleEdit::Update(_, fields) => {
                for value in fields.values().flatten() {
                    validate_field(value, 1, &mut budget)?;
                }
            }
            TupleEdit::Remove(_) => {}
        }
        let text = self.copy_text()?;
        let candidate = parse_tree(self.format, &text, self.limits, self.request)?;
        let Tree::Json(root) = &candidate else {
            return Err(DocumentError::WrongType);
        };
        let array = json_array(root, path, matches!(edit, TupleEdit::Append(_)))?;
        match edit {
            TupleEdit::Append(value) => {
                array.append(json_input(value));
            }
            TupleEdit::Update(index, fields) => {
                let object = array
                    .elements()
                    .get(index)
                    .and_then(|node| node.as_object())
                    .ok_or(DocumentError::WrongType)?;
                for (key, value) in fields {
                    check_request(self.request)?;
                    match (object.get(key), value) {
                        (Some(prop), Some(value)) => prop.set_value(json_input(value)),
                        (None, Some(value)) => {
                            object.append(key, json_input(value));
                        }
                        (Some(prop), None) => prop.remove(),
                        (None, None) => {}
                    }
                }
            }
            TupleEdit::Remove(index) => {
                array
                    .elements()
                    .get(index)
                    .ok_or(DocumentError::WrongType)?
                    .clone()
                    .remove();
            }
        }
        let output = render_tree(&candidate, self.limits.output_bytes, self.request)?;
        self.commit_output(&output)
    }
    fn edit(&mut self, path: &[&str], value: Option<FieldValue>) -> Result<()> {
        validate_path(path, self.limits)?;
        if let Some(v) = &value {
            validate_field(v, 1, &mut Budget::new(self.limits, self.request))?;
        }
        // CST Clone shares mutable Rc nodes. Reparse a bounded rendering instead.
        // The old document remains intact if mutation/output validation fails.
        let text = self.copy_text()?;
        let output = if let Some(value) = value.as_ref() {
            let mut candidate = parse_tree(self.format, &text, self.limits, self.request)?;
            match &mut candidate {
                Tree::Json(root) => json_edit(root, path, value)?,
                Tree::Toml(doc) => toml_edit_field(doc.as_item_mut(), path, value)?,
            }
            render_tree(&candidate, self.limits.output_bytes, self.request)?
        } else {
            remove_field(&text, self.format, path, self.limits, self.request)?
        };
        self.commit_output(&output)
    }
    fn commit_output(&mut self, output: &str) -> Result<()> {
        let verified = parse_tree(self.format, output, self.limits, self.request)?;
        check_request(self.request)?;
        self.tree = verified;
        Ok(())
    }
}

fn validate_policy(l: SharedWriteLimits) -> Result<()> {
    if [
        l.input_bytes,
        l.output_bytes,
        l.journal_bytes,
        l.nodes,
        l.depth,
        l.scalar_bytes,
        l.entries,
        l.hook_tuples,
        l.references,
        l.documents,
    ]
    .contains(&0)
        || l.depth > 80
        || l.scalar_bytes > l.input_bytes
        || l.scalar_bytes > l.output_bytes
        || l.entries > l.nodes
        || l.hook_tuples > l.nodes
        || l.references > l.nodes
        || l.documents > l.nodes
        || l.input_bytes.checked_add(1).is_none()
        || l.nodes
            .checked_mul(4)
            .and_then(|n| n.checked_add(1))
            .is_none()
    {
        return Err(DocumentError::InvalidPolicy);
    }
    Ok(())
}
fn check_request(request: &RequestContext<'_>) -> Result<()> {
    request.remaining().map(|_| ()).map_err(|e| match e.kind() {
        io::ErrorKind::Interrupted => DocumentError::Interrupted,
        io::ErrorKind::TimedOut => DocumentError::TimedOut,
        _ => DocumentError::InvalidPolicy,
    })
}
fn validate_path(path: &[&str], l: SharedWriteLimits) -> Result<()> {
    if path.is_empty()
        || path.len() >= l.depth
        || path
            .iter()
            .any(|s| s.is_empty() || s.len() > l.scalar_bytes)
    {
        return Err(DocumentError::InvalidPath);
    }
    Ok(())
}
struct Budget<'r, 'c> {
    limits: SharedWriteLimits,
    nodes: usize,
    generated_bytes: usize,
    request: &'r RequestContext<'c>,
}
impl<'r, 'c> Budget<'r, 'c> {
    fn new(limits: SharedWriteLimits, request: &'r RequestContext<'c>) -> Self {
        Self {
            limits,
            nodes: 0,
            generated_bytes: 0,
            request,
        }
    }
    fn node(&mut self, depth: usize) -> Result<()> {
        check_request(self.request)?;
        self.nodes += 1;
        if self.nodes > self.limits.nodes || depth > self.limits.depth {
            return Err(DocumentError::LimitExceeded);
        }
        Ok(())
    }
    fn scalar(&self, bytes: usize) -> Result<()> {
        if bytes > self.limits.scalar_bytes {
            Err(DocumentError::LimitExceeded)
        } else {
            Ok(())
        }
    }
    fn collection(&self, entries: usize) -> Result<()> {
        if entries > self.limits.entries {
            Err(DocumentError::LimitExceeded)
        } else {
            Ok(())
        }
    }
    // A lower bound before cloning generated values into the native DOM. Escaping
    // and formatting receive the exact bounded writer check afterwards.
    fn generated(&mut self, bytes: usize) -> Result<()> {
        self.generated_bytes = self
            .generated_bytes
            .checked_add(bytes)
            .filter(|n| *n <= self.limits.output_bytes)
            .ok_or(DocumentError::LimitExceeded)?;
        Ok(())
    }
}
fn validate_field(value: &FieldValue, depth: usize, b: &mut Budget<'_, '_>) -> Result<()> {
    b.node(depth)?;
    match value {
        FieldValue::String(v) => {
            b.scalar(v.len())?;
            b.generated(v.len())?;
            b.generated(2)?;
        }
        FieldValue::Integer(v) => {
            let length = v.to_string().len();
            b.scalar(length)?;
            b.generated(length)?;
        }
        FieldValue::Boolean(v) => b.generated(if *v { 4 } else { 5 })?,
        FieldValue::Array(v) => {
            b.collection(v.len())?;
            b.generated(2)?;
            b.generated(v.len().saturating_sub(1))?;
            for item in v {
                validate_field(item, depth + 1, b)?;
            }
        }
        FieldValue::Object(v) => {
            b.collection(v.len())?;
            b.generated(2)?;
            b.generated(v.len().saturating_sub(1))?;
            for (key, item) in v {
                b.scalar(key.len())?;
                b.generated(key.len())?;
                b.generated(3)?;
                validate_field(item, depth + 1, b)?;
            }
        }
    }
    Ok(())
}
struct Limited<'r, 'c> {
    text: String,
    limit: usize,
    request: &'r RequestContext<'c>,
    error: Option<DocumentError>,
}
impl fmt::Write for Limited<'_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let result = check_request(self.request).and_then(|_| {
            if self
                .text
                .len()
                .checked_add(s.len())
                .is_none_or(|n| n > self.limit)
            {
                Err(DocumentError::LimitExceeded)
            } else {
                Ok(())
            }
        });
        if let Err(e) = result {
            self.error = Some(e);
            return Err(fmt::Error);
        }
        self.text.push_str(s);
        Ok(())
    }
}
fn render_tree(tree: &Tree, limit: usize, request: &RequestContext<'_>) -> Result<String> {
    check_request(request)?;
    let mut out = Limited {
        text: String::new(),
        limit,
        request,
        error: None,
    };
    let result = match tree {
        Tree::Json(doc) => write!(&mut out, "{doc}"),
        Tree::Toml(doc) => write!(&mut out, "{doc}"),
    };
    result.map_err(|_| out.error.unwrap_or(DocumentError::LimitExceeded))?;
    check_request(request)?;
    Ok(out.text)
}
fn parse_tree(
    format: DocumentFormat,
    text: &str,
    limits: SharedWriteLimits,
    request: &RequestContext<'_>,
) -> Result<Tree> {
    check_request(request)?;
    // The caller bounds source input or generated output before this private entry.
    match format {
        DocumentFormat::Json | DocumentFormat::Jsonc => {
            json_preflight(text, limits, request)?;
            let options = json_options(format);
            let parsed = jsonc_parser::parse_to_ast(text, &Default::default(), &options)
                .map_err(|_| DocumentError::InvalidDocument)?;
            let value = parsed
                .value
                .as_ref()
                .ok_or(DocumentError::InvalidDocument)?;
            if !matches!(value, Ast::Object(_)) {
                return Err(DocumentError::WrongType);
            }
            json_validate(value, 1, &mut Budget::new(limits, request))?;
            let root =
                CstRootNode::parse(text, &options).map_err(|_| DocumentError::InvalidDocument)?;
            check_request(request)?;
            Ok(Tree::Json(root))
        }
        DocumentFormat::Toml => {
            toml_preflight(text, limits, request)?;
            let doc: toml_edit::DocumentMut =
                text.parse().map_err(|_| DocumentError::InvalidDocument)?;
            toml_validate(doc.as_item(), 1, &mut Budget::new(limits, request))?;
            check_request(request)?;
            Ok(Tree::Toml(doc))
        }
    }
}
fn json_options(format: DocumentFormat) -> jsonc_parser::ParseOptions {
    let comments = format == DocumentFormat::Jsonc;
    jsonc_parser::ParseOptions {
        allow_comments: comments,
        allow_trailing_commas: comments,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
        allow_bare_decimal_point_numbers: false,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: false,
    }
}
fn json_preflight(text: &str, l: SharedWriteLimits, request: &RequestContext<'_>) -> Result<()> {
    let options = jsonc_parser::ScannerOptions {
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
        allow_bare_decimal_point_numbers: false,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: false,
    };
    let mut scan = jsonc_parser::Scanner::new(text, &options);
    let (mut depth, mut tokens) = (0usize, 0usize);
    while let Some(token) = scan.scan().map_err(|_| DocumentError::InvalidDocument)? {
        check_request(request)?;
        tokens += 1;
        if tokens > l.nodes * 4 {
            return Err(DocumentError::LimitExceeded);
        }
        match token {
            Token::OpenBrace | Token::OpenBracket => {
                depth += 1;
                if depth > l.depth {
                    return Err(DocumentError::LimitExceeded);
                }
            }
            Token::CloseBrace | Token::CloseBracket => {
                depth = depth.checked_sub(1).ok_or(DocumentError::InvalidDocument)?
            }
            Token::String(s) if s.len() > l.scalar_bytes => {
                return Err(DocumentError::LimitExceeded);
            }
            Token::Number(s) if s.len() > l.scalar_bytes => {
                return Err(DocumentError::LimitExceeded);
            }
            _ => {}
        }
    }
    Ok(())
}
fn json_validate(value: &Ast<'_>, depth: usize, b: &mut Budget<'_, '_>) -> Result<()> {
    b.node(depth)?;
    match value {
        Ast::Object(o) => {
            b.collection(o.properties.len())?;
            let mut seen = BTreeSet::new();
            for p in &o.properties {
                b.scalar(p.name.as_str().len())?;
                if !seen.insert(p.name.as_str()) {
                    return Err(DocumentError::DuplicateKey);
                }
                json_validate(&p.value, depth + 1, b)?;
            }
        }
        Ast::Array(a) => {
            b.collection(a.elements.len())?;
            for v in &a.elements {
                json_validate(v, depth + 1, b)?;
            }
        }
        Ast::StringLit(s) => b.scalar(s.value.len())?,
        Ast::NumberLit(n) => b.scalar(n.value.len())?,
        _ => {}
    }
    Ok(())
}
fn json_field(value: &Ast<'_>, request: &RequestContext<'_>) -> Result<FieldValue> {
    check_request(request)?;
    Ok(match value {
        Ast::StringLit(v) => FieldValue::String(v.value.to_string()),
        Ast::BooleanLit(v) => FieldValue::Boolean(v.value),
        Ast::NumberLit(v) => {
            FieldValue::Integer(v.value.parse().map_err(|_| DocumentError::WrongType)?)
        }
        Ast::Array(v) => FieldValue::Array(
            v.elements
                .iter()
                .map(|v| json_field(v, request))
                .collect::<Result<_>>()?,
        ),
        Ast::Object(v) => FieldValue::Object(
            v.properties
                .iter()
                .map(|p| Ok((p.name.as_str().into(), json_field(&p.value, request)?)))
                .collect::<Result<_>>()?,
        ),
        Ast::NullKeyword(_) => return Err(DocumentError::WrongType),
    })
}
fn json_equal(
    actual: &Ast<'_>,
    expected: &FieldValue,
    request: &RequestContext<'_>,
) -> Result<bool> {
    check_request(request)?;
    Ok(match (actual, expected) {
        (Ast::StringLit(actual), FieldValue::String(expected)) => actual.value == *expected,
        (Ast::BooleanLit(actual), FieldValue::Boolean(expected)) => actual.value == *expected,
        (Ast::NumberLit(actual), FieldValue::Integer(expected)) => {
            actual.value.parse::<i64>() == Ok(*expected)
        }
        (Ast::Array(actual), FieldValue::Array(expected)) => {
            if actual.elements.len() != expected.len() {
                return Ok(false);
            }
            for (actual, expected) in actual.elements.iter().zip(expected) {
                if !json_equal(actual, expected, request)? {
                    return Ok(false);
                }
            }
            true
        }
        (Ast::Object(actual), FieldValue::Object(expected)) => {
            if actual.properties.len() != expected.len() {
                return Ok(false);
            }
            for (key, expected) in expected {
                let Some(actual) = actual.get(key) else {
                    return Ok(false);
                };
                if !json_equal(&actual.value, expected, request)? {
                    return Ok(false);
                }
            }
            true
        }
        _ => false,
    })
}
fn json_array(root: &CstRootNode, path: &[&str], create: bool) -> Result<CstArray> {
    let mut object = root.object_value().ok_or(DocumentError::WrongType)?;
    for key in &path[..path.len() - 1] {
        object = match object.get(key) {
            Some(prop) => prop.object_value().ok_or(DocumentError::WrongType)?,
            None if create => object
                .append(key, CstInputValue::Object(Vec::new()))
                .object_value()
                .ok_or(DocumentError::WrongType)?,
            None => return Err(DocumentError::WrongType),
        };
    }
    let key = path[path.len() - 1];
    match object.get(key) {
        Some(prop) => prop.array_value().ok_or(DocumentError::WrongType),
        None if create => object
            .append(key, CstInputValue::Array(Vec::new()))
            .array_value()
            .ok_or(DocumentError::WrongType),
        None => Err(DocumentError::WrongType),
    }
}
fn json_input(value: &FieldValue) -> CstInputValue {
    match value {
        FieldValue::String(v) => CstInputValue::String(v.clone()),
        FieldValue::Boolean(v) => CstInputValue::Bool(*v),
        FieldValue::Integer(v) => CstInputValue::Number(v.to_string()),
        FieldValue::Array(v) => CstInputValue::Array(v.iter().map(json_input).collect()),
        FieldValue::Object(v) => {
            CstInputValue::Object(v.iter().map(|(k, v)| (k.clone(), json_input(v))).collect())
        }
    }
}
fn json_edit(root: &CstRootNode, path: &[&str], value: &FieldValue) -> Result<()> {
    let mut object = root.object_value().ok_or(DocumentError::WrongType)?;
    for key in &path[..path.len() - 1] {
        object = match object.get(key) {
            Some(prop) => prop.object_value().ok_or(DocumentError::WrongType)?,
            None => object
                .append(key, CstInputValue::Object(Vec::new()))
                .object_value()
                .ok_or(DocumentError::WrongType)?,
        };
    }
    let key = path[path.len() - 1];
    if let Some(prop) = object.get(key) {
        prop.set_value(json_input(value));
    } else {
        object.append(key, json_input(value));
    }
    Ok(())
}
struct DecodedBudget {
    bytes: usize,
    limit: usize,
    exceeded: bool,
}
impl<'s> toml_parser::decoder::StringBuilder<'s> for DecodedBudget {
    fn clear(&mut self) {
        self.bytes = 0;
    }
    fn push_str(&mut self, s: &'s str) -> bool {
        self.bytes = self.bytes.saturating_add(s.len());
        self.exceeded |= self.bytes > self.limit;
        !self.exceeded
    }
    fn push_char(&mut self, c: char) -> bool {
        self.bytes = self.bytes.saturating_add(c.len_utf8());
        self.exceeded |= self.bytes > self.limit;
        !self.exceeded
    }
}
fn toml_preflight(text: &str, l: SharedWriteLimits, request: &RequestContext<'_>) -> Result<()> {
    use toml_parser::parser::{Event, EventKind, RecursionGuard};
    let source = toml_parser::Source::new(text);
    let mut tokens = Vec::new();
    for token in source.lex().take(l.nodes * 4 + 1) {
        check_request(request)?;
        tokens.push(token);
    }
    if tokens.len() > l.nodes * 4 {
        return Err(DocumentError::LimitExceeded);
    }
    let (mut invalid, mut exceeded) = (false, false);
    let mut receive = |event: Event| {
        if matches!(event.kind(), EventKind::SimpleKey | EventKind::Scalar) {
            let mut budget = DecodedBudget {
                bytes: 0,
                limit: l.scalar_bytes,
                exceeded: false,
            };
            let mut error = |_: toml_parser::ParseError| invalid = true;
            if let Some(raw) = source.get(event) {
                if event.kind() == EventKind::SimpleKey {
                    raw.decode_key(&mut budget, &mut error);
                } else {
                    let _ = raw.decode_scalar(&mut budget, &mut error);
                }
            } else {
                invalid = true;
            }
            exceeded |= budget.exceeded;
        }
    };
    let mut failed = false;
    toml_parser::parser::parse_document(
        &tokens,
        &mut RecursionGuard::new(&mut receive, (l.depth - 1) as u32),
        &mut |_: toml_parser::ParseError| failed = true,
    );
    check_request(request)?;
    if exceeded {
        Err(DocumentError::LimitExceeded)
    } else if failed || invalid {
        Err(DocumentError::InvalidDocument)
    } else {
        Ok(())
    }
}
fn toml_validate(item: &toml_edit::Item, depth: usize, b: &mut Budget<'_, '_>) -> Result<()> {
    b.node(depth)?;
    if let Some(table) = item.as_table_like() {
        b.collection(table.len())?;
        for (k, v) in table.iter() {
            b.scalar(k.len())?;
            toml_validate(v, depth + 1, b)?;
        }
    } else if let Some(array) = item.as_array_of_tables() {
        b.collection(array.len())?;
        for table in array {
            b.node(depth + 1)?;
            b.collection(table.len())?;
            for (k, v) in table.iter() {
                b.scalar(k.len())?;
                toml_validate(v, depth + 2, b)?;
            }
        }
    } else if let Some(value) = item.as_value() {
        toml_value_validate(value, depth, b)?;
    }
    Ok(())
}
fn toml_value_validate(
    value: &toml_edit::Value,
    depth: usize,
    b: &mut Budget<'_, '_>,
) -> Result<()> {
    if let Some(s) = value.as_str() {
        b.scalar(s.len())?;
    }
    if let Some(a) = value.as_array() {
        b.collection(a.len())?;
        for v in a {
            b.node(depth + 1)?;
            toml_value_validate(v, depth + 1, b)?;
        }
    }
    if let Some(t) = value.as_inline_table() {
        b.collection(t.len())?;
        for (k, v) in t.iter() {
            b.scalar(k.len())?;
            b.node(depth + 1)?;
            toml_value_validate(v, depth + 1, b)?;
        }
    }
    Ok(())
}
fn toml_field(item: &toml_edit::Item, request: &RequestContext<'_>) -> Result<FieldValue> {
    check_request(request)?;
    if let Some(t) = item.as_table_like() {
        return toml_table_field(t, request);
    }
    if let Some(tables) = item.as_array_of_tables() {
        return Ok(FieldValue::Array(
            tables
                .iter()
                .map(|table| toml_table_field(table, request))
                .collect::<Result<_>>()?,
        ));
    }
    toml_value_field(item.as_value().ok_or(DocumentError::WrongType)?, request)
}
fn toml_table_field(
    table: &dyn toml_edit::TableLike,
    request: &RequestContext<'_>,
) -> Result<FieldValue> {
    check_request(request)?;
    Ok(FieldValue::Object(
        table
            .iter()
            .map(|(key, value)| Ok((key.into(), toml_field(value, request)?)))
            .collect::<Result<_>>()?,
    ))
}
fn toml_value_field(v: &toml_edit::Value, request: &RequestContext<'_>) -> Result<FieldValue> {
    check_request(request)?;
    if let Some(s) = v.as_str() {
        return Ok(FieldValue::String(s.into()));
    }
    if let Some(b) = v.as_bool() {
        return Ok(FieldValue::Boolean(b));
    }
    if let Some(n) = v.as_integer() {
        return Ok(FieldValue::Integer(n));
    }
    if let Some(a) = v.as_array() {
        return Ok(FieldValue::Array(
            a.iter()
                .map(|v| toml_value_field(v, request))
                .collect::<Result<_>>()?,
        ));
    }
    if let Some(t) = v.as_inline_table() {
        return Ok(FieldValue::Object(
            t.iter()
                .map(|(k, v)| Ok((k.into(), toml_value_field(v, request)?)))
                .collect::<Result<_>>()?,
        ));
    }
    Err(DocumentError::WrongType)
}
fn toml_input(value: &FieldValue) -> toml_edit::Value {
    match value {
        FieldValue::String(v) => toml_edit::Value::from(v.clone()),
        FieldValue::Boolean(v) => toml_edit::Value::from(*v),
        FieldValue::Integer(v) => toml_edit::Value::from(*v),
        FieldValue::Array(v) => toml_edit::Value::Array(v.iter().map(toml_input).collect()),
        FieldValue::Object(v) => toml_edit::Value::InlineTable(
            v.iter().map(|(k, v)| (k.clone(), toml_input(v))).collect(),
        ),
    }
}

// Native parsers supply assignment boundaries and lexers identify punctuation
// and comments. Keep trivia when removing a field: CST remove also discards it.
struct Removal {
    field: Range<usize>,
    preceding: Option<Range<usize>>,
    following: Option<Range<usize>>,
}
fn json_removal(text: &str, format: DocumentFormat, path: &[&str]) -> Result<Option<Removal>> {
    let parsed = jsonc_parser::parse_to_ast(text, &Default::default(), &json_options(format))
        .map_err(|_| DocumentError::InvalidDocument)?;
    let mut value = parsed
        .value
        .as_ref()
        .ok_or(DocumentError::InvalidDocument)?;
    for key in &path[..path.len() - 1] {
        let object = value.as_object().ok_or(DocumentError::WrongType)?;
        let Some(prop) = object.get(key) else {
            return Ok(None);
        };
        value = &prop.value;
    }
    let object = value.as_object().ok_or(DocumentError::WrongType)?;
    let Some(index) = object
        .properties
        .iter()
        .position(|p| p.name.as_str() == path[path.len() - 1])
    else {
        return Ok(None);
    };
    let field = &object.properties[index];
    Ok(Some(Removal {
        field: field.range.start..field.range.end,
        preceding: index
            .checked_sub(1)
            .map(|i| object.properties[i].range.end..field.range.start),
        following: Some(
            field.range.end
                ..object
                    .properties
                    .get(index + 1)
                    .map_or(object.range.end - 1, |p| p.range.start),
        ),
    }))
}
fn toml_removal(text: &str, path: &[&str]) -> Result<Option<Removal>> {
    let doc = toml_edit::Document::parse(text).map_err(|_| DocumentError::InvalidDocument)?;
    let mut item = doc.as_item();
    for key in path {
        let table = item.as_table_like().ok_or(DocumentError::WrongType)?;
        let Some(next) = table.get(key) else {
            return Ok(None);
        };
        item = next;
    }
    if let Some(table) = item.as_table().filter(|table| table.is_empty()) {
        return Ok(Some(Removal {
            field: table.span().ok_or(DocumentError::InvalidDocument)?,
            preceding: None,
            following: None,
        }));
    }
    let span = item
        .as_value()
        .ok_or(DocumentError::WrongType)?
        .span()
        .ok_or(DocumentError::InvalidDocument)?;
    toml_assignment(doc.as_item(), &span)
        .map(Some)
        .ok_or(DocumentError::InvalidDocument)
}
fn toml_assignment(item: &toml_edit::Item, span: &Range<usize>) -> Option<Removal> {
    let table = item.as_table_like()?;
    let values = table.get_values();
    for (index, (keys, value)) in values.iter().enumerate() {
        if value.span().as_ref() != Some(span) {
            continue;
        }
        let start = keys.first()?.span()?.start;
        let inline = item.as_inline_table();
        return Some(Removal {
            field: start..span.end,
            preceding: inline
                .and_then(|_| index.checked_sub(1))
                .and_then(|i| values[i].1.span())
                .map(|r| r.end..start),
            following: inline.and_then(|t| t.span()).map(|r| {
                span.end
                    ..values
                        .get(index + 1)
                        .and_then(|(k, _)| k.first()?.span())
                        .map_or(r.end - 1, |r| r.start)
            }),
        });
    }
    table
        .iter()
        .find_map(|(_, child)| toml_assignment(child, span))
}
fn remove_field(
    text: &str,
    format: DocumentFormat,
    path: &[&str],
    limits: SharedWriteLimits,
    request: &RequestContext<'_>,
) -> Result<String> {
    let removal = match format {
        DocumentFormat::Json | DocumentFormat::Jsonc => json_removal(text, format, path)?,
        DocumentFormat::Toml => toml_removal(text, path)?,
    };
    let Some(removal) = removal else {
        let mut out = Limited {
            text: String::new(),
            limit: limits.output_bytes,
            request,
            error: None,
        };
        out.write_str(text)
            .map_err(|_| out.error.unwrap_or(DocumentError::LimitExceeded))?;
        return Ok(out.text);
    };
    let mut comments = Vec::new();
    let (mut preceding, mut following) = (None, None);
    let mut record = |range: Range<usize>, comma: bool, comment: bool, line: bool| {
        if comma {
            if removal
                .preceding
                .as_ref()
                .is_some_and(|r| range.start >= r.start && range.end <= r.end)
            {
                preceding = Some(range.clone());
            }
            if removal
                .following
                .as_ref()
                .is_some_and(|r| range.start >= r.start && range.end <= r.end)
            {
                following = Some(range.clone());
            }
        }
        if comment && range.start >= removal.field.start && range.end <= removal.field.end {
            let mut end = range.end;
            if line {
                while end < removal.field.end && matches!(text.as_bytes()[end], b'\r' | b'\n') {
                    end += 1;
                }
            }
            comments.push(range.start..end);
        }
    };
    match format {
        DocumentFormat::Json | DocumentFormat::Jsonc => {
            let options = jsonc_parser::ScannerOptions::default();
            let mut scan = jsonc_parser::Scanner::new(text, &options);
            while let Some(token) = scan.scan().map_err(|_| DocumentError::InvalidDocument)? {
                check_request(request)?;
                record(
                    scan.token_start()..scan.token_end(),
                    matches!(token, Token::Comma),
                    matches!(token, Token::CommentLine(_) | Token::CommentBlock(_)),
                    matches!(token, Token::CommentLine(_)),
                );
            }
        }
        DocumentFormat::Toml => {
            use toml_parser::lexer::TokenKind;
            for token in toml_parser::Source::new(text).lex() {
                check_request(request)?;
                let span = token.span();
                record(
                    span.start()..span.end(),
                    token.kind() == TokenKind::Comma,
                    token.kind() == TokenKind::Comment,
                    true,
                );
            }
        }
    }
    let mut deleted = vec![removal.field];
    if let Some(comma) = following.or(preceding) {
        deleted.push(comma);
    }
    deleted.sort_by_key(|r| r.start);
    let mut out = Limited {
        text: String::new(),
        limit: limits.output_bytes,
        request,
        error: None,
    };
    let mut start = 0;
    let result = (|| -> fmt::Result {
        for range in &deleted {
            out.write_str(text.get(start..range.start).ok_or(fmt::Error)?)?;
            for comment in &comments {
                if comment.start >= range.start && comment.end <= range.end {
                    out.write_str(text.get(comment.clone()).ok_or(fmt::Error)?)?;
                }
            }
            start = range.end;
        }
        out.write_str(text.get(start..).ok_or(fmt::Error)?)
    })();
    result.map_err(|_| out.error.unwrap_or(DocumentError::InvalidDocument))?;
    Ok(out.text)
}
fn toml_edit_field(item: &mut toml_edit::Item, path: &[&str], value: &FieldValue) -> Result<()> {
    let inline = item.as_inline_table().is_some();
    let table = item.as_table_like_mut().ok_or(DocumentError::WrongType)?;
    let key = path[0];
    if path.len() > 1 {
        if table.get(key).is_none() {
            let next = if inline {
                toml_edit::Item::Value(toml_edit::Value::InlineTable(toml_edit::InlineTable::new()))
            } else {
                let mut next = toml_edit::Table::new();
                next.set_implicit(true);
                toml_edit::Item::Table(next)
            };
            table.insert(key, next);
        }
        return toml_edit_field(
            table.get_mut(key).ok_or(DocumentError::WrongType)?,
            &path[1..],
            value,
        );
    }
    {
        let mut replacement = toml_input(value);
        if let Some(existing) = table.get_mut(key) {
            if let Some(old) = existing.as_value() {
                *replacement.decor_mut() = old.decor().clone();
                *existing = toml_edit::Item::Value(replacement);
            } else if let Some(old) = existing.as_table() {
                let toml_edit::Value::InlineTable(replacement) = replacement else {
                    return Err(DocumentError::WrongType);
                };
                let mut next = replacement.into_table();
                *next.decor_mut() = old.decor().clone();
                next.set_position(old.position());
                next.set_implicit(old.is_implicit());
                *existing = toml_edit::Item::Table(next);
            } else {
                return Err(DocumentError::WrongType);
            }
        } else {
            table.insert(key, toml_edit::Item::Value(replacement));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
