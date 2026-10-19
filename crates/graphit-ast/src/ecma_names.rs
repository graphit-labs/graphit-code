//! Canonical IdentifierName proof for one captured ECMAScript lexeme.
//!
//! This uses SWC's public lexer, never a second file parser. A proof establishes
//! lexical spelling only; native context/recovery still determines its role.

use std::{error::Error, fmt};
use swc_common::{BytePos, Span, Spanned};
use swc_ecma_ast::{EsVersion, Ident};
use swc_ecma_parser::{
    EsSyntax, StringInput, Syntax, TsSyntax,
    input::{Buffer, Tokens},
    lexer::Lexer,
    unstable::Token,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EcmaNameDialect {
    JavaScript,
    TypeScript,
}

/// Actual library coordinates for a lexeme whose input starts at BytePos(1).
/// These are observations, not fabricated Original ranges.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EcmaLexemeSpan {
    pub start: u32,
    pub end: u32,
}
impl From<Span> for EcmaLexemeSpan {
    fn from(span: Span) -> Self {
        Self {
            start: span.lo.0,
            end: span.hi.0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EcmaNameIssue {
    Empty,
    PositionOverflow,
    NotWordToken {
        token: String,
        span: EcmaLexemeSpan,
    },
    NonWholeLexeme {
        span: EcmaLexemeSpan,
        lexeme_end: u32,
    },
    InvalidDecodedIdentifierName {
        span: EcmaLexemeSpan,
    },
    AdditionalToken {
        token: String,
        span: EcmaLexemeSpan,
    },
    Lexical {
        kind: String,
        span: EcmaLexemeSpan,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcmaNameError {
    issues: Vec<EcmaNameIssue>,
}
impl EcmaNameError {
    pub fn issues(&self) -> &[EcmaNameIssue] {
        &self.issues
    }
}
impl fmt::Display for EcmaNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "captured ECMAScript IdentifierName proof failed: {:?}",
            self.issues
        )
    }
}
impl Error for EcmaNameError {}

fn lexical_issue(error: swc_ecma_parser::error::Error) -> EcmaNameIssue {
    EcmaNameIssue::Lexical {
        kind: format!("{:?}", error.kind()),
        span: error.span().into(),
    }
}

/// Decode only a complete captured IdentifierName using the mature lexer.
///
/// Keywords are legal property IdentifierNames. This function does not claim
/// BindingIdentifier permission, native node kind, Source or parser authority.
pub fn decode_ecma_identifier_name(
    lexeme: &str,
    dialect: EcmaNameDialect,
) -> Result<String, EcmaNameError> {
    if lexeme.is_empty() {
        return Err(EcmaNameError {
            issues: vec![EcmaNameIssue::Empty],
        });
    }
    let Some(end) = u32::try_from(lexeme.len())
        .ok()
        .and_then(|bytes| bytes.checked_add(1))
    else {
        return Err(EcmaNameError {
            issues: vec![EcmaNameIssue::PositionOverflow],
        });
    };
    let syntax = match dialect {
        EcmaNameDialect::JavaScript => Syntax::Es(EsSyntax::default()),
        EcmaNameDialect::TypeScript => Syntax::Typescript(TsSyntax::default()),
    };
    let lexer = Lexer::new(
        syntax,
        EsVersion::EsNext,
        StringInput::new(lexeme, BytePos(1), BytePos(end)),
        None,
    );
    let mut buffer = Buffer::new(lexer);
    let first = buffer.iter.first_token();
    buffer.set_cur(first);
    let mut issues = Vec::new();
    let canonical = if first.token.is_word() {
        Some(first.token.take_word(&buffer).to_string())
    } else {
        issues.push(EcmaNameIssue::NotWordToken {
            token: format!("{:?}", first.token),
            span: first.span.into(),
        });
        None
    };
    if let Some(canonical) = &canonical {
        let mut characters = canonical.chars();
        if !characters.next().is_some_and(Ident::is_valid_start)
            || !characters.all(Ident::is_valid_continue)
        {
            issues.push(EcmaNameIssue::InvalidDecodedIdentifierName {
                span: first.span.into(),
            });
        }
    }
    if first.span.lo != BytePos(1) || first.span.hi != BytePos(end) {
        issues.push(EcmaNameIssue::NonWholeLexeme {
            span: first.span.into(),
            lexeme_end: end,
        });
    }
    let mut current = first;
    while current.token != Token::Eof {
        if current.token == Token::Error {
            issues.push(lexical_issue(current.token.take_error(&mut buffer)));
        }
        current = buffer.iter.next_token();
        buffer.set_cur(current);
        if current.token != Token::Eof {
            issues.push(EcmaNameIssue::AdditionalToken {
                token: format!("{:?}", current.token),
                span: current.span.into(),
            });
        }
    }
    issues.extend(buffer.iter.take_errors().into_iter().map(lexical_issue));
    issues.extend(
        buffer
            .iter
            .take_script_module_errors()
            .into_iter()
            .map(lexical_issue),
    );
    if issues.is_empty()
        && let Some(canonical) = canonical
    {
        Ok(canonical)
    } else {
        Err(EcmaNameError { issues })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mature_lexer_preserves_canonical_equivalence_and_identifier_name_keywords() {
        for dialect in [EcmaNameDialect::JavaScript, EcmaNameDialect::TypeScript] {
            for (raw, decoded) in [
                ("a", "a"),
                (r"\u0061", "a"),
                (r"\u{61}", "a"),
                ("$f", "$f"),
                ("ação", "ação"),
                (r"\u{1D49C}", "𝒜"),
                ("a\u{200c}", "a\u{200c}"),
                ("default", "default"),
                (r"d\u0065fault", "default"),
                ("true", "true"),
            ] {
                assert_eq!(decode_ecma_identifier_name(raw, dialect).unwrap(), decoded);
            }
        }
    }

    #[test]
    fn real_lexical_errors_and_non_identifier_shapes_are_not_repaired() {
        for dialect in [EcmaNameDialect::JavaScript, EcmaNameDialect::TypeScript] {
            for raw in [
                "",
                " a",
                "a ",
                "a b",
                "a/*comment*/",
                "1",
                "\"a\"",
                "#a",
                "[a]",
                "a-b",
                r"\u0030",
                r"\u{110000}",
                r"\uD800",
                r"\u00ZZ",
                "a\u{0000}",
            ] {
                let error = decode_ecma_identifier_name(raw, dialect).unwrap_err();
                assert!(!error.issues().is_empty(), "{raw:?}");
            }
            let error = decode_ecma_identifier_name(r"\u00ZZ", dialect).unwrap_err();
            assert!(error.issues().iter().any(|issue| matches!(
                issue,
                EcmaNameIssue::Lexical { kind, span }
                if !kind.is_empty() && span.start <= span.end
            )));
            let error = decode_ecma_identifier_name("a b", dialect).unwrap_err();
            assert!(
                error
                    .issues()
                    .iter()
                    .any(|issue| matches!(issue, EcmaNameIssue::AdditionalToken { .. }))
            );
        }
    }
}
