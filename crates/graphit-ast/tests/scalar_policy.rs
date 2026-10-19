use graphit_ast::scalar_policy::{
    CodecSpan, OriginalByteSpan, RegexLimits, ScalarAudit, ScalarKind, ScalarLimits, ScalarPolicy,
    ScalarPolicyError, ScalarPolicyErrorCode as Code,
};
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_saphyr::{DuplicateKeyPolicy, MergeKeyPolicy, Spanned};
use std::fmt;

const INPUT: usize = 32_768;
const DEPTH: usize = 16;
const NODES: usize = 2048;
const EVENTS: usize = NODES * 4;
const RECORDS: usize = NODES * 2;

fn policy() -> ScalarPolicy {
    ScalarPolicy::new(RegexLimits {
        compiled_size_policy_bytes: 1 << 20,
        dfa_cache_policy_bytes: 1 << 20,
    })
    .unwrap()
}

fn limits() -> ScalarLimits {
    ScalarLimits::new(INPUT, DEPTH, EVENTS, RECORDS, INPUT, 4096, 64).unwrap()
}

macro_rules! profiles {
    ($($name:literal),+ $(,)?) => {
        [$(($name, include_bytes!(concat!("../../../crates/graphit-ast/profiles/queries/", $name, ".yaml")).as_slice())),+]
    };
}

const PROFILES: [(&str, &[u8]); 45] = profiles!(
    "bash",
    "c",
    "clojure",
    "cobol85",
    "cpp",
    "csharp",
    "css",
    "dart",
    "db2",
    "dockerfile",
    "elixir",
    "go",
    "graphql",
    "groovy",
    "haskell",
    "hcl",
    "html",
    "java",
    "javascript",
    "json",
    "julia",
    "kotlin",
    "lua",
    "objc",
    "php",
    "plpgsql",
    "plsql",
    "postgresql",
    "protobuf",
    "python",
    "r",
    "ruby",
    "rust",
    "scala",
    "sql",
    "svelte",
    "swift",
    "toml",
    "tsql",
    "tsx",
    "typescript",
    "vue",
    "xml",
    "yaml",
    "zig",
);

type Text = Spanned<Option<String>>;

// Retain entries rather than deduplicating Option<String> keys before kind lookup.
struct TextPairs(Vec<(Text, Text)>);

impl<'de> Deserialize<'de> for TextPairs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PairsVisitor;
        impl<'de> Visitor<'de> for PairsVisitor {
            type Value = TextPairs;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map of spanned text entries")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
                let mut entries = Vec::new();
                while let Some(pair) = access.next_entry::<Text, Text>()? {
                    entries.push(pair);
                }
                Ok(TextPairs(entries))
            }
        }
        deserializer.deserialize_map(PairsVisitor)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JointWire {
    text: Text,
    list: Vec<Text>,
    map: TextPairs,
    flag: Text,
    cleared: Text,
}

struct JointResult {
    text: String,
    list: Vec<String>,
    map: Vec<(String, String)>,
    flag: bool,
}

fn original(audit: &ScalarAudit<'_>, wire: &Text) -> Result<OriginalByteSpan, ScalarPolicyError> {
    let span = wire.defined.span();
    audit.original_span(CodecSpan::from_parts(span.byte_offset(), span.byte_len())?)
}

fn native_options() -> serde_saphyr::Options {
    let budget = serde_saphyr::budget! {
        max_reader_input_bytes: Some(INPUT), max_events: EVENTS, max_nodes: RECORDS,
        max_depth: DEPTH, flow_nesting_limit: DEPTH, max_documents: 1,
        max_aliases: 0, max_anchors: 0, max_recorded_anchor_events: 0,
        max_recorded_anchor_bytes: 0, max_total_scalar_bytes: INPUT,
        max_total_comment_bytes: INPUT, max_merge_keys: 0, max_inclusion_depth: 0,
        simple_key_max_lookahead: INPUT,
    };
    let alias_limits = serde_saphyr::alias_limits! {
        max_total_replayed_events: 0, max_replay_stack_depth: 0,
        max_alias_expansions_per_anchor: 0,
    };
    serde_saphyr::options! {
        budget: budget, alias_limits: alias_limits, merge_keys: MergeKeyPolicy::Error,
        duplicate_keys: DuplicateKeyPolicy::Error, strict_booleans: true,
        reject_unsupported_tags: true, reject_non_finite_typeless_float: true,
        emit_comments: false, with_snippet: false,
    }
}

fn codec_error(audit: &ScalarAudit<'_>, cause: &serde_saphyr::Error) -> ScalarPolicyError {
    let cause = cause.without_snippet();
    let code = match cause {
        serde_saphyr::Error::Budget { .. } | serde_saphyr::Error::MultipleDocuments { .. } => {
            Code::QuotaExceeded
        }
        serde_saphyr::Error::UnsupportedTag { .. } => Code::UnsupportedTag,
        serde_saphyr::Error::InvalidOptions { .. } => Code::InvalidLimits,
        serde_saphyr::Error::InvalidUtf8Input => Code::InvalidUtf8,
        _ => Code::Parse,
    };
    let metadata = cause
        .location()
        .and_then(|location| {
            let span = location.span();
            span.byte_offset()
                .zip(span.byte_len())
                .map(|(offset, len)| CodecSpan::new(offset, len))
        })
        .or_else(|| match cause {
            serde_saphyr::Error::ExternalMessage { source, .. } => match source.as_ref() {
                serde_saphyr::ExternalMessageSource::Parser(scan) => scan
                    .marker()
                    .byte_offset()
                    .and_then(|offset| u64::try_from(offset).ok())
                    .map(|offset| CodecSpan::new(offset, 0)),
                _ => None,
            },
            _ => None,
        });
    match metadata.map(|span| audit.original_span(span)).transpose() {
        Ok(span) => ScalarPolicyError { code, span },
        Err(error) => error,
    }
}

// The helper admits only the audit, so wire metadata cannot be joined to another input.
// Option<String> is transport only: final values always come from native kind lookup.
fn joint(audit: &ScalarAudit<'_>) -> Result<JointResult, ScalarPolicyError> {
    let text = std::str::from_utf8(audit.input()).map_err(|_| ScalarPolicyError {
        code: Code::InvalidUtf8,
        span: None,
    })?;
    let wire: JointWire = serde_saphyr::from_str_with_options(text, native_options())
        .map_err(|error| codec_error(audit, &error))?;
    let get_text = |value: &Text| -> Result<String, ScalarPolicyError> {
        Ok(audit.text_at(original(audit, value)?)?.to_owned())
    };
    let result = JointResult {
        text: get_text(&wire.text)?,
        list: wire.list.iter().map(get_text).collect::<Result<_, _>>()?,
        map: wire
            .map
            .0
            .iter()
            .map(|(key, value)| Ok((get_text(key)?, get_text(value)?)))
            .collect::<Result<_, ScalarPolicyError>>()?,
        flag: audit.bool_at(original(audit, &wire.flag)?)?,
    };
    audit.null_at(original(audit, &wire.cleared)?)?;
    Ok(result)
}

fn one(input: &[u8]) -> (ScalarKind, String, OriginalByteSpan) {
    let audit = policy().audit(input, &limits()).unwrap();
    assert_eq!(audit.records().len(), 1);
    let record = &audit.records()[0];
    (record.kind(), record.text().to_owned(), record.span())
}

#[test]
fn s01_audits_every_shipped_profile_without_numeric_conversion() {
    let policy = policy();
    let (mut bytes, mut booleans) = (0, 0);
    for (name, input) in PROFILES {
        let audit = policy
            .audit(input, &limits())
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(audit.input().as_ptr(), input.as_ptr(), "{name}");
        bytes += input.len();
        for record in audit.records() {
            match record.kind() {
                ScalarKind::String => {
                    audit.text_at(record.span()).unwrap();
                }
                ScalarKind::Boolean => {
                    booleans += 1;
                    assert!(audit.bool_at(record.span()).unwrap(), "{name}");
                }
                kind => panic!("{name}: unexpected shipped kind {kind:?}"),
            }
        }
    }
    assert_eq!(bytes, 198_844);
    assert_eq!(booleans, 5);
}

#[test]
fn s02_joint_text_list_keys_boolean_and_null_keep_native_kinds() {
    let audit = policy()
        .audit(
            include_bytes!("fixtures/scalar_policy/joint.yaml"),
            &limits(),
        )
        .unwrap();
    let decoded = joint(&audit).unwrap();
    assert_eq!(decoded.text, "α😀");
    assert_eq!(decoded.list, ["123", "true", "null", "1_0"]);
    assert_eq!(
        decoded.map,
        [
            ("nUlL".into(), "NuLl".into()),
            ("other".into(), "yes".into())
        ]
    );
    assert!(decoded.flag);
    for (field, value) in [
        ("text", "123"),
        ("text", "true"),
        ("text", "null"),
        ("list", "[123]"),
        ("map", "{true: value}"),
        ("map", "{key: false}"),
        ("flag", "\"true\""),
        ("cleared", "\"null\""),
    ] {
        let mut yaml = String::new();
        for (key, good) in [
            ("text", "okay"),
            ("list", "[]"),
            ("map", "{}"),
            ("flag", "true"),
            ("cleared", "null"),
        ] {
            yaml.push_str(&format!(
                "{key}: {}\n",
                if key == field { value } else { good }
            ));
        }
        let audit = policy().audit(yaml.as_bytes(), &limits()).unwrap();
        let error = joint(&audit).err().expect("wrong scalar kind must fail");
        assert_eq!(error.code, Code::WrongScalarKind, "{field}: {value}");
        assert!(error.span.is_some());
    }
}

#[test]
fn s03_core_numeric_magnitudes_and_string_controls() {
    for value in include_str!("fixtures/scalar_policy/numeric.txt").lines() {
        let audit = policy().audit(value.as_bytes(), &limits()).unwrap();
        let record = &audit.records()[0];
        assert_eq!(record.kind(), ScalarKind::Numeric, "{value}");
        assert_eq!(record.text(), value);
        assert_eq!(
            audit.text_at(record.span()).err().unwrap().code,
            Code::WrongScalarKind
        );
    }
    for value in [
        "+0x1", "-0o7", "0X1", "1_0", "tRuE", "nUlL", "NuLl", "yes", "on",
    ] {
        assert_eq!(one(value.as_bytes()).0, ScalarKind::String, "{value}");
    }
    for value in ["true", "True", "TRUE", "false", "False", "FALSE"] {
        let audit = policy().audit(value.as_bytes(), &limits()).unwrap();
        assert_eq!(
            audit.bool_at(audit.records()[0].span()).unwrap(),
            value.starts_with(['t', 'T'])
        );
    }
    for value in ["~", "null", "Null", "NULL"] {
        assert_eq!(one(value.as_bytes()).0, ScalarKind::Null);
    }
}

#[test]
fn s04_styles_and_resolved_tags_are_owned_by_the_native_parser() {
    for (yaml, expected) in [
        ("'123'", "123"),
        ("\"true\"", "true"),
        ("!!str null", "null"),
        ("! 123", "123"),
        ("|\n  α\n", "α\n"),
        (">\n  α\n  β\n", "α β\n"),
        ("%TAG !s! tag:yaml.org,2002:\n---\n!s!str 123", "123"),
    ] {
        let (kind, text, span) = one(yaml.as_bytes());
        assert_eq!(kind, ScalarKind::String);
        assert_eq!(text, expected);
        assert!(yaml.get(span.start()..span.end()).is_some());
    }
    for (yaml, code) in [
        ("!!bool tRuE", Code::InvalidScalar),
        ("!!null nope", Code::InvalidScalar),
        ("!!int nope", Code::InvalidScalar),
        ("!!float nope", Code::InvalidScalar),
        ("!custom abc", Code::UnsupportedTag),
        ("!!seq {}", Code::UnsupportedTag),
        ("!!map []", Code::UnsupportedTag),
    ] {
        assert_eq!(
            policy()
                .audit(yaml.as_bytes(), &limits())
                .err()
                .unwrap()
                .code,
            code,
            "{yaml}"
        );
    }
    assert_eq!(one(b"!!bool 'TRUE'").0, ScalarKind::Boolean);
    assert_eq!(one(b"!!null 'null'").0, ScalarKind::Null);
    assert_eq!(one(b"!!int 0xff").0, ScalarKind::Numeric);
    assert_eq!(one(b"!!float 1e999").0, ScalarKind::Numeric);
    policy()
        .audit(b"!!map {x: !!seq [! 1]}", &limits())
        .unwrap();
}

#[derive(Deserialize)]
struct OriginWire {
    x: Text,
}

fn origin_wire(audit: &ScalarAudit<'_>) -> Text {
    let input = std::str::from_utf8(audit.input()).unwrap();
    let wire: OriginWire = serde_saphyr::from_str_with_options(input, native_options()).unwrap();
    wire.x
}

#[test]
fn s05_codec_origin_bom_unicode_tags_and_original_diagnostics() {
    let policy = policy();
    for count in 0..=4 {
        let yaml = format!("{}x: abc", "\u{feff}".repeat(count));
        let audit = policy.audit(yaml.as_bytes(), &limits()).unwrap();
        let wire = origin_wire(&audit);
        let codec = wire.defined.span();
        let delta = count.min(2) * 3;
        assert_eq!(codec.byte_offset(), Some((count * 3 + 3 - delta) as u64));
        assert_eq!(codec.byte_len(), Some(3));
        let span = original(&audit, &wire).unwrap();
        assert_eq!((span.start(), span.end()), (count * 3 + 3, count * 3 + 6));
        assert_eq!(audit.text_at(span).unwrap(), "abc");
        assert_eq!(&yaml.as_bytes()[span.start()..span.end()], b"abc");
        let eof = audit
            .original_span(CodecSpan::new((yaml.len() - delta) as u64, 0))
            .unwrap();
        assert_eq!((eof.start(), eof.end()), (yaml.len(), yaml.len()));
        assert_eq!(
            audit.text_at(eof).err().unwrap().code,
            Code::MissingScalarSpan
        );
        let yaml = format!("{}x: 123", "\u{feff}".repeat(count));
        let audit = policy.audit(yaml.as_bytes(), &limits()).unwrap();
        let span = original(&audit, &origin_wire(&audit)).unwrap();
        let error = audit.text_at(span).err().unwrap();
        assert_eq!(error.code, Code::WrongScalarKind);
        assert_eq!(error.span, Some(span));
        let empty = format!("{}x:", "\u{feff}".repeat(count));
        let audit = policy.audit(empty.as_bytes(), &limits()).unwrap();
        let wire = origin_wire(&audit);
        let span = original(&audit, &wire).unwrap();
        // Granit anchors an implicit empty value at ':'; EOF is a separate coordinate.
        assert_eq!((span.start(), span.end()), (count * 3 + 1, count * 3 + 1));
        assert_eq!(span, audit.records().last().unwrap().span());
        audit.null_at(span).unwrap();
    }
    for yaml in [
        "x: α😀",
        "x: 'α😀'",
        "x: !!str α😀",
        "%TAG !s! tag:yaml.org,2002:\n---\nx: !s!str α😀",
        "x: 'a\u{feff}b'",
        "x: a\u{feff}b",
    ] {
        for count in 0..=3 {
            let yaml = format!("{}{yaml}", "\u{feff}".repeat(count));
            let audit = policy.audit(yaml.as_bytes(), &limits()).unwrap();
            let span = original(&audit, &origin_wire(&audit)).unwrap();
            let expected = audit.records().last().unwrap();
            assert_eq!(span, expected.span());
            assert_eq!(audit.text_at(span).unwrap(), expected.text());
        }
    }
    let left = policy.audit(b"x: aaa", &limits()).unwrap();
    let right = policy.audit(b"x: bbb", &limits()).unwrap();
    let span = original(&left, &origin_wire(&left)).unwrap();
    assert_eq!(span, original(&right, &origin_wire(&right)).unwrap());
    assert_eq!(left.text_at(span).unwrap(), "aaa");
    assert_eq!(right.text_at(span).unwrap(), "bbb");
    assert_eq!(
        CodecSpan::from_parts(None, Some(3)).err().unwrap().code,
        Code::MissingScalarSpan
    );
    assert_eq!(
        CodecSpan::from_parts(Some(3), None).err().unwrap().code,
        Code::MissingScalarSpan
    );
    assert_eq!(
        CodecSpan::from_parts(None, None).err().unwrap().code,
        Code::MissingScalarSpan
    );
    for span in [
        CodecSpan::new(u64::MAX, 1),
        CodecSpan::new(3, u64::MAX),
        CodecSpan::new(7, 0),
    ] {
        assert_eq!(
            left.original_span(span).err().unwrap().code,
            Code::InvalidSpan
        );
    }
    let unicode = policy.audit("x: α".as_bytes(), &limits()).unwrap();
    assert_eq!(
        unicode
            .original_span(CodecSpan::new(4, 0))
            .err()
            .unwrap()
            .code,
        Code::InvalidSpan
    );
    let eof = unicode.original_span(CodecSpan::new(5, 0)).unwrap();
    assert_eq!((eof.start(), eof.end()), (5, 5));
    assert_eq!(
        unicode.text_at(eof).err().unwrap().code,
        Code::MissingScalarSpan
    );
    for count in 0..=3 {
        let yaml = format!("{}x: [", "\u{feff}".repeat(count));
        let error = policy.audit(yaml.as_bytes(), &limits()).err().unwrap();
        assert_eq!(error.code, Code::Parse);
        let span = error
            .span
            .expect("native unclosed-flow error must retain its opening byte marker");
        assert_eq!((span.start(), span.end()), (count * 3 + 3, count * 3 + 3));
    }
    // A typed native error with byte metadata uses the same origin conversion.
    for count in 0..=3 {
        let yaml = format!("{}x: [abc]", "\u{feff}".repeat(count));
        let audit = policy.audit(yaml.as_bytes(), &limits()).unwrap();
        let cause = serde_saphyr::from_str_with_options::<OriginWire>(&yaml, native_options())
            .err()
            .unwrap();
        let error = codec_error(&audit, &cause);
        assert_eq!(error.code, Code::Parse);
        let span = error
            .span
            .expect("typed sequence error must retain byte metadata");
        assert_eq!((span.start(), span.end()), (count * 3 + 3, count * 3 + 4));
        assert!(!error.to_string().contains("abc"));
    }
    let missing = codec_error(&left, &serde_saphyr::Error::InvalidUtf8Input);
    assert_eq!((missing.code, missing.span), (Code::InvalidUtf8, None));
}

#[test]
fn s06_native_syntax_duplicate_policy_and_forbidden_constructs() {
    let policy = policy();
    for yaml in ["x: [", "{x: y", "x: \"bad\\q\""] {
        assert_eq!(
            policy.audit(yaml.as_bytes(), &limits()).err().unwrap().code,
            Code::Parse
        );
    }
    for yaml in ["x: &id value\ny: *id", "&id [value]", "&id {x: value}"] {
        assert_eq!(
            policy.audit(yaml.as_bytes(), &limits()).err().unwrap().code,
            Code::ForbiddenAnchorAlias
        );
    }
    assert_eq!(
        policy
            .audit(b"---\nx\n---\ny", &limits())
            .err()
            .unwrap()
            .code,
        Code::QuotaExceeded
    );
    for map in ["{same: a, same: b}", "{same: a, 'same': b}", "{<<: value}"] {
        let yaml = format!("text: okay\nlist: []\nmap: {map}\nflag: true\ncleared: null\n");
        let audit = policy.audit(yaml.as_bytes(), &limits()).unwrap();
        assert_eq!(joint(&audit).err().unwrap().code, Code::Parse);
    }
    // Core-distinct string keys must not collapse under the codec's null resolver.
    let audit = policy
        .audit(
            include_bytes!("fixtures/scalar_policy/distinct_keys.yaml"),
            &limits(),
        )
        .unwrap();
    assert_eq!(
        joint(&audit).unwrap().map,
        [("nUlL".into(), "a".into()), ("NuLl".into(), "b".into())]
    );
}

#[test]
fn s06_guaranteed_ledger_limits_and_regex_policies() {
    let policy = policy();
    let yaml = b"language: x\n";
    let observed = policy.audit(yaml, &limits()).unwrap().usage();
    assert_eq!(
        (
            observed.input_bytes,
            observed.scalar_records,
            observed.decoded_text_bytes,
            observed.max_depth
        ),
        (12, 2, 9, 1)
    );
    let exact = ScalarLimits::new(12, 1, observed.events, 2, 9, 4096, 64).unwrap();
    assert_eq!(policy.audit(yaml, &exact).unwrap().usage(), observed);
    for (bytes, events, records, text, code) in [
        (11, observed.events, 2, 9, Code::InputTooLarge),
        (12, observed.events - 1, 2, 9, Code::QuotaExceeded),
        (12, observed.events, 1, 9, Code::QuotaExceeded),
        (12, observed.events, 2, 8, Code::QuotaExceeded),
    ] {
        let limits = ScalarLimits::new(bytes, 1, events, records, text, 4096, 64).unwrap();
        assert_eq!(policy.audit(yaml, &limits).err().unwrap().code, code);
    }
    assert_eq!(
        policy.audit(b"x: {y: z}", &exact).err().unwrap().code,
        Code::QuotaExceeded
    );
    assert_eq!(
        policy
            .audit(
                b"%TAG !s! tag:yaml.org,2002:\n---\nx: !s!str y",
                &ScalarLimits::new(INPUT, DEPTH, EVENTS, RECORDS, INPUT, 1, 64).unwrap()
            )
            .err()
            .unwrap()
            .code,
        Code::QuotaExceeded
    );
    assert_eq!(
        policy
            .audit(
                b"%FUTURE one two\n---\nx",
                &ScalarLimits::new(INPUT, DEPTH, EVENTS, RECORDS, INPUT, 4096, 1).unwrap()
            )
            .err()
            .unwrap()
            .code,
        Code::QuotaExceeded
    );
    assert_eq!(
        ScalarLimits::new(0, 1, 1, 1, 1, 1, 1).err().unwrap().code,
        Code::InvalidLimits
    );
    if usize::BITS > 32 {
        assert!(ScalarLimits::new(usize::MAX, 1, 1, 1, 1, 1, 1).is_err());
    }
    assert_eq!(
        policy.audit(&[0xff; 13], &exact).err().unwrap().code,
        Code::InputTooLarge
    );
    assert_eq!(
        policy.audit(&[0xff], &exact).err().unwrap().code,
        Code::InvalidUtf8
    );
    assert_eq!(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1,
            dfa_cache_policy_bytes: 1
        })
        .err()
        .unwrap()
        .code,
        Code::RegexPolicyBuild
    );
    let small_cache = ScalarPolicy::new(RegexLimits {
        compiled_size_policy_bytes: 1 << 20,
        dfa_cache_policy_bytes: 1,
    })
    .unwrap();
    assert_eq!(
        small_cache.audit(b"1e999", &limits()).unwrap().records()[0].kind(),
        ScalarKind::Numeric
    );
    assert!(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 0,
            dfa_cache_policy_bytes: 1
        })
        .is_err()
    );
}

#[test]
fn s07_numeric_mutants_of_every_profile_fail_text_lookup() {
    let policy = policy();
    for (name, input) in PROFILES {
        for numeric in ["123", "1e999", "0x10000000000000000"] {
            let mut mutant = input.to_vec();
            mutant.extend_from_slice(format!("\nscalar_policy_probe: {numeric}\n").as_bytes());
            let audit = policy
                .audit(&mutant, &limits())
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let numeric = audit
                .records()
                .iter()
                .find(|record| record.kind() == ScalarKind::Numeric)
                .expect("mutant must be numeric");
            assert_eq!(
                audit.text_at(numeric.span()).err().unwrap().code,
                Code::WrongScalarKind
            );
        }
    }
}

#[test]
fn s08_redacted_errors_and_complete_native_streams() {
    let policy = policy();
    for yaml in ["secret_value: [", "secret_value: !private hidden"] {
        let error = policy.audit(yaml.as_bytes(), &limits()).err().unwrap();
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains("secret_value"));
            assert!(!rendered.contains("hidden"));
            assert!(!rendered.contains("private"));
        }
    }
    for yaml in ["", "# comment\n", "---\n", "x: # empty\n", "x: !!str ''"] {
        policy.audit(yaml.as_bytes(), &limits()).unwrap();
    }
    let audit = policy.audit(b"x: okay", &limits()).unwrap();
    assert_eq!(
        OriginalByteSpan::new(2, 1).err().unwrap().code,
        Code::InvalidSpan
    );
    assert_eq!(
        audit
            .text_at(OriginalByteSpan::new(0, 0).unwrap())
            .err()
            .unwrap()
            .code,
        Code::MissingScalarSpan
    );
    assert_eq!(
        audit
            .text_at(OriginalByteSpan::new(usize::MAX, usize::MAX).unwrap())
            .err()
            .unwrap()
            .code,
        Code::InvalidSpan
    );
}
