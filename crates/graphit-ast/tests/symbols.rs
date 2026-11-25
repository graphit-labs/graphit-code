use graphit_ast::catalog::*;
use graphit_ast::prepared_input::*;
use graphit_ast::profiles::*;
use graphit_ast::scalar_policy::{RegexLimits, ScalarPolicy};
use graphit_ast::symbols::*;
use graphit_ast::{BytePoint, RangeError, SourceText};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const GO: &[u8] = include_bytes!("fixtures/symbols/go.txt");
const JS: &[u8] = include_bytes!("fixtures/symbols/same-line.js");
const UNICODE: &[u8] = include_bytes!("fixtures/symbols/unicode-crlf.js");
const JSON: &[u8] = include_bytes!("fixtures/symbols/values.json");

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}

fn policy() -> IdentityLimitsInput {
    IdentityLimitsInput {
        source_bytes: 4096,
        path_components: 16,
        path_component_bytes: 256,
        identity_text_bytes: 256,
        owner_depth: 16,
        uid_bytes: 4096,
    }
}
fn limits() -> IdentityLimits {
    IdentityLimits::new(policy()).unwrap()
}
fn document<'o, 'r>(
    paths: &'o [&'o str],
    bytes: &'o [u8],
    ctx: &'r SnapshotContext,
) -> IdentityDocument<'o, 'r> {
    IdentityDocument::new(
        IdentityDocumentInput {
            relative_components: paths,
            language: "go",
            grammar: "tree-sitter-go",
            original_bytes: bytes,
        },
        limits(),
        ctx,
    )
    .unwrap()
}
fn error<T>(
    result: Result<T, IdentityError>,
    code: IdentityErrorCode,
    field: Option<IdentityField>,
    index: Option<u32>,
) -> IdentityError {
    let actual = result.err().expect("expected identity rejection");
    assert_eq!(
        (actual.code(), actual.field(), actual.index()),
        (code, field, index)
    );
    actual
}
fn same_tuple(a: &SyntaxIdentity<'_, '_, '_>, b: &SyntaxIdentity<'_, '_, '_>) -> bool {
    a.document().relative_components() == b.document().relative_components()
        && a.document().language() == b.document().language()
        && a.document().grammar() == b.document().grammar()
        && a.label() == b.label()
        && a.subject().start() == b.subject().start()
        && a.subject().end() == b.subject().end()
        && a.context().len() == b.context().len()
        && a.context()
            .iter()
            .zip(b.context())
            .all(|(x, y)| x.label() == y.label() && x.start() == y.start() && x.end() == y.end())
}

#[test]
fn inserting_unrelated_lines_preserves_logical_identity_and_scope() {
    fn uid(bytes: &[u8]) -> String {
        let ctx = context();
        let doc = IdentityDocument::new(
            IdentityDocumentInput {
                relative_components: &["src", "stable.js"],
                language: "javascript",
                grammar: "tree-sitter-javascript",
                original_bytes: bytes,
            },
            limits(),
            &ctx,
        )
        .unwrap();
        let source = SourceText::new(bytes, 4096).unwrap();
        let text = std::str::from_utf8(bytes).unwrap();
        let owner_start = text.find("Box").unwrap();
        let name_start = text.find("same").unwrap();
        let owner = doc
            .symbol(
                "Class",
                source.range(owner_start, owner_start + 3).unwrap(),
                &[],
                LogicalDeclaration::named(
                    (source.range(owner_start, owner_start + 3).unwrap()).text(),
                ),
            )
            .unwrap();
        doc.symbol(
            "Method",
            source.range(name_start, name_start + 4).unwrap(),
            &[&owner],
            LogicalDeclaration::named((source.range(name_start, name_start + 4).unwrap()).text()),
        )
        .unwrap()
        .uid()
        .as_str()
        .to_owned()
    }

    let original = b"class Box { same() {} }\n";
    let shifted = b"// unrelated observation\n\nclass Box { same() {} }\n";
    assert_eq!(uid(original), uid(shifted));
}

fn assert_manual_oracle(key: &SyntaxIdentity<'_, '_, '_>, row: &serde_json::Value) {
    let tuple = &row["typed_tuple"];
    let paths = tuple[1].as_array().unwrap();
    assert_eq!(key.document().relative_components().len(), paths.len());
    for (actual, expected) in key.document().relative_components().iter().zip(paths) {
        assert_eq!(*actual, expected.as_str().unwrap());
    }
    assert_eq!(key.document().language(), tuple[2].as_str().unwrap());
    assert_eq!(key.document().grammar(), tuple[3].as_str().unwrap());
    assert_eq!(key.label(), tuple[4].as_str().unwrap());
    let start = tuple[5][0].as_u64().unwrap() as usize;
    let end = tuple[5][1].as_u64().unwrap() as usize;
    assert_eq!((key.subject().start(), key.subject().end()), (start, end));
    let bytes = match row["original_fixture"].as_str().unwrap() {
        "go.txt" => GO,
        "same-line.js" => JS,
        "unicode-crlf.js" => UNICODE,
        "values.json" => JSON,
        _ => panic!("unknown manual fixture"),
    };
    assert_eq!(key.subject().text().as_bytes(), &bytes[start..end]);
    let frames = tuple[6].as_array().unwrap();
    assert_eq!(key.context().len(), frames.len());
    for (actual, expected) in key.context().iter().zip(frames) {
        assert_eq!(actual.label(), expected[0].as_str().unwrap());
        assert_eq!(actual.start(), expected[1].as_u64().unwrap() as usize);
        assert_eq!(actual.end(), expected[2].as_u64().unwrap() as usize);
    }
    assert_eq!(key.uid().as_str(), row["expected_uid"].as_str().unwrap());
    assert_eq!(
        key.uid().as_str().len(),
        row["expected_uid_utf8_bytes"].as_u64().unwrap() as usize
    );
    if row["id"] == "F02-unicode" {
        assert_eq!(key.subject().start_point(), BytePoint { row: 1, column: 9 });
        assert_eq!(key.subject().end_point(), BytePoint { row: 1, column: 12 });
    }
}

#[test]
fn all_fifteen_manual_oracles_compare_logical_uid_and_source_observations() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/symbols/expected-identity.json")).unwrap();
    let rows = expected["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 15);
    assert_eq!(
        (GO.len(), JS.len(), UNICODE.len(), JSON.len()),
        (27, 36, 26, 14)
    );
    let ctx = context();
    let mut seen = BTreeMap::new();
    for row in rows {
        let tuple = &row["typed_tuple"];
        let paths: Vec<&str> = tuple[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let bytes = match row["original_fixture"].as_str().unwrap() {
            "go.txt" => GO,
            "same-line.js" => JS,
            "unicode-crlf.js" => UNICODE,
            "values.json" => JSON,
            _ => panic!("unknown manual fixture"),
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
        for prior in row["prior_subjects"].as_array().unwrap() {
            let subject = source
                .range(
                    prior[1].as_u64().unwrap() as usize,
                    prior[2].as_u64().unwrap() as usize,
                )
                .unwrap();
            doc.symbol(
                prior[0].as_str().unwrap(),
                subject,
                &[],
                LogicalDeclaration::named(subject.text()),
            )
            .unwrap();
        }
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
                LogicalDeclaration::named(
                    (source
                        .range(
                            o[1].as_u64().unwrap() as usize,
                            o[2].as_u64().unwrap() as usize,
                        )
                        .unwrap())
                    .text(),
                ),
            )
            .unwrap()
        });
        let owners: Vec<_> = owner.iter().collect();
        let start = tuple[5][0].as_u64().unwrap() as usize;
        let end = tuple[5][1].as_u64().unwrap() as usize;
        let key = doc
            .symbol(
                tuple[4].as_str().unwrap(),
                source.range(start, end).unwrap(),
                &owners,
                LogicalDeclaration::named((source.range(start, end).unwrap()).text()),
            )
            .unwrap();
        assert!(std::ptr::eq(key.document(), &doc));
        assert_manual_oracle(&key, row);
        seen.insert(
            row["id"].as_str().unwrap(),
            row["expected_uid"].as_str().unwrap(),
        );
    }
    assert_eq!(seen["F01-a"], seen["F01-repeat"]);
    assert_ne!(seen["F01-a"], seen["F01-b"]);
    assert_ne!(seen["F02-first"], seen["F02-second"]);
    assert_ne!(seen["F03-components-a"], seen["F03-components-b"]);
    assert_ne!(seen["F03-label-case"], seen["F01-a"]);
    assert_ne!(seen["F04-value-a"], seen["F04-value-b"]);
    assert_eq!(seen["F04-value-a"], seen["F04-value-repeat"]);
}

#[test]
fn six_positive_limits_and_count_representability_have_exact_closed_fields() {
    let fields = [
        IdentityField::SourceBytes,
        IdentityField::PathComponents,
        IdentityField::PathComponentBytes,
        IdentityField::IdentityTextBytes,
        IdentityField::OwnerDepth,
        IdentityField::UidBytes,
    ];
    for (i, field) in fields.into_iter().enumerate() {
        let mut input = policy();
        match i {
            0 => input.source_bytes = 0,
            1 => input.path_components = 0,
            2 => input.path_component_bytes = 0,
            3 => input.identity_text_bytes = 0,
            4 => input.owner_depth = 0,
            5 => input.uid_bytes = 0,
            _ => unreachable!(),
        }
        error(
            IdentityLimits::new(input),
            IdentityErrorCode::InvalidLimits,
            Some(field),
            None,
        );
    }
    let mut input = policy();
    input.path_components = u32::MAX as usize;
    input.owner_depth = u32::MAX as usize;
    IdentityLimits::new(input).unwrap();
    if let Some(over) = (u32::MAX as usize).checked_add(1) {
        let mut input = policy();
        input.path_components = over;
        error(
            IdentityLimits::new(input),
            IdentityErrorCode::InvalidLimits,
            Some(IdentityField::PathComponents),
            None,
        );
        let mut input = policy();
        input.owner_depth = over;
        error(
            IdentityLimits::new(input),
            IdentityErrorCode::InvalidLimits,
            Some(IdentityField::OwnerDepth),
            None,
        );
    }
    let p = limits();
    assert_eq!(
        (
            p.source_bytes(),
            p.path_components(),
            p.path_component_bytes(),
            p.identity_text_bytes(),
            p.owner_depth(),
            p.uid_bytes()
        ),
        (4096, 16, 256, 256, 16, 4096)
    );
}

#[test]
fn document_and_text_predicates_reject_without_normalizing() {
    let ctx = context();
    error(
        IdentityDocument::new(
            IdentityDocumentInput {
                relative_components: &[],
                language: "go",
                grammar: "g",
                original_bytes: GO,
            },
            limits(),
            &ctx,
        ),
        IdentityErrorCode::InvalidDocument,
        Some(IdentityField::PathComponents),
        None,
    );
    for component in ["", ".", "..", "a/b", "x\0y"] {
        error(
            IdentityDocument::new(
                IdentityDocumentInput {
                    relative_components: &[component],
                    language: "go",
                    grammar: "g",
                    original_bytes: GO,
                },
                limits(),
                &ctx,
            ),
            IdentityErrorCode::InvalidDocument,
            Some(IdentityField::PathComponentBytes),
            Some(0),
        );
    }
    for invalid in ["", "\0", " go", "go ", "\tgo", "go\n"] {
        error(
            IdentityDocument::new(
                IdentityDocumentInput {
                    relative_components: &["a"],
                    language: invalid,
                    grammar: "g",
                    original_bytes: GO,
                },
                limits(),
                &ctx,
            ),
            IdentityErrorCode::InvalidDocument,
            Some(IdentityField::Language),
            None,
        );
        error(
            IdentityDocument::new(
                IdentityDocumentInput {
                    relative_components: &["a"],
                    language: "go",
                    grammar: invalid,
                    original_bytes: GO,
                },
                limits(),
                &ctx,
            ),
            IdentityErrorCode::InvalidGrammar,
            Some(IdentityField::Grammar),
            None,
        );
    }
    let paths = [" spaced ", "é\\x::file.go"];
    let doc = document(&paths, GO, &ctx);
    assert_eq!(doc.relative_components(), paths);
    let subject = SourceText::new(GO, 4096).unwrap().range(18, 21).unwrap();
    for label in [
        "",
        "\0",
        " Function",
        "Function ",
        "\tFunction",
        "Function\n",
    ] {
        error(
            doc.symbol(
                label,
                subject,
                &[],
                LogicalDeclaration::named((subject).text()),
            ),
            IdentityErrorCode::InvalidLabel,
            Some(IdentityField::Label),
            None,
        );
    }
    let key = doc
        .symbol(
            "Custom\"Λ",
            subject,
            &[],
            LogicalDeclaration::named((subject).text()),
        )
        .unwrap();
    assert_eq!(key.label(), "Custom\"Λ");
    let cause = error(
        IdentityDocument::new(
            IdentityDocumentInput {
                relative_components: &["a"],
                language: "go",
                grammar: "g",
                original_bytes: &[0xff],
            },
            limits(),
            &ctx,
        ),
        IdentityErrorCode::InvalidUtf8,
        Some(IdentityField::SourceBytes),
        None,
    );
    assert_eq!(
        cause.range_error(),
        Some(RangeError::InvalidUtf8 {
            valid_up_to: 0,
            error_len: Some(1)
        })
    );
    let empty = document(&["empty.go"], b"", &ctx);
    error(
        empty.symbol(
            "Function",
            SourceText::new(b"", 1).unwrap().range(0, 0).unwrap(),
            &[],
            LogicalDeclaration::named(
                (SourceText::new(b"", 1).unwrap().range(0, 0).unwrap()).text(),
            ),
        ),
        IdentityErrorCode::InvalidAnchor,
        None,
        None,
    );
}

#[test]
fn actual_ranges_foreign_binding_and_relative_points_are_rejected() {
    let go = SourceText::new(GO, 4096).unwrap();
    assert_eq!(go.range(5, 2).err(), Some(RangeError::ReversedRange));
    assert_eq!(go.range(0, 28).err(), Some(RangeError::OutOfBounds));
    let unicode = SourceText::new(UNICODE, 4096).unwrap();
    for end in [1, 4] {
        assert_eq!(
            unicode.range(0, end).err(),
            Some(RangeError::NotCharBoundary)
        );
    }
    let ctx = context();
    let doc = document(&["a.go"], GO, &ctx);
    error(
        doc.symbol(
            "Function",
            go.range(27, 27).unwrap(),
            &[],
            LogicalDeclaration::named((go.range(27, 27).unwrap()).text()),
        ),
        IdentityErrorCode::InvalidAnchor,
        None,
        None,
    );
    let foreign = GO.to_vec();
    assert_ne!(foreign.as_ptr(), GO.as_ptr());
    error(
        doc.symbol(
            "Function",
            SourceText::new(&foreign, 4096)
                .unwrap()
                .range(18, 21)
                .unwrap(),
            &[],
            LogicalDeclaration::named(
                (SourceText::new(&foreign, 4096)
                    .unwrap()
                    .range(18, 21)
                    .unwrap())
                .text(),
            ),
        ),
        IdentityErrorCode::InvalidAnchor,
        None,
        None,
    );

    let original = "α".as_bytes();
    let alpha = document(&["a.go"], original, &ctx);
    let ascii = SourceText::new(b"a", 1).unwrap().range(0, 1).unwrap();
    let failure = error(
        alpha.symbol(
            "Function",
            ascii,
            &[],
            LogicalDeclaration::named((ascii).text()),
        ),
        IdentityErrorCode::InvalidAnchor,
        None,
        None,
    );
    assert_eq!(failure.range_error(), None);
    // A SourceText on a suffix reports relative offsets and points, even
    // when its text is borrowed from the same allocation.
    let raw = b"a\nb";
    let full = document(&["x"], raw, &ctx);
    let relative = SourceText::new(&raw[2..], 3).unwrap().range(0, 1).unwrap();
    let whole = SourceText::new(raw, 3).unwrap().range(2, 3).unwrap();
    assert_eq!(relative.text().as_ptr(), whole.text().as_ptr());
    assert_ne!(relative.start_point(), whole.start_point());
    error(
        full.symbol(
            "Function",
            relative,
            &[],
            LogicalDeclaration::named((relative).text()),
        ),
        IdentityErrorCode::InvalidAnchor,
        None,
        None,
    );
}

#[test]
fn owner_document_prefix_repetition_self_and_independent_lifetimes() {
    let ctx = context();
    let paths = ["values.json"];
    let doc = document(&paths, JSON, &ctx);
    let other = document(&paths, JSON, &ctx);
    let source = SourceText::new(JSON, 4096).unwrap();
    let a = doc
        .symbol(
            "Pair",
            source.range(2, 3).unwrap(),
            &[],
            LogicalDeclaration::named((source.range(2, 3).unwrap()).text()),
        )
        .unwrap();
    let b = doc
        .symbol(
            "Pair",
            source.range(8, 9).unwrap(),
            &[],
            LogicalDeclaration::named((source.range(8, 9).unwrap()).text()),
        )
        .unwrap();
    let nested = doc
        .symbol(
            "Value",
            source.range(5, 6).unwrap(),
            &[&a],
            LogicalDeclaration::named((source.range(5, 6).unwrap()).text()),
        )
        .unwrap();
    let foreign = other
        .symbol(
            "Pair",
            source.range(2, 3).unwrap(),
            &[],
            LogicalDeclaration::named((source.range(2, 3).unwrap()).text()),
        )
        .unwrap();
    let subject = source.range(11, 12).unwrap();
    error(
        doc.symbol(
            "Value",
            subject,
            &[&foreign],
            LogicalDeclaration::named((subject).text()),
        ),
        IdentityErrorCode::InvalidContext,
        Some(IdentityField::Owners),
        Some(0),
    );
    error(
        doc.symbol(
            "Value",
            subject,
            &[&b, &nested],
            LogicalDeclaration::named((subject).text()),
        ),
        IdentityErrorCode::InvalidContext,
        Some(IdentityField::Owners),
        Some(1),
    );
    error(
        doc.symbol(
            "Value",
            subject,
            &[&nested],
            LogicalDeclaration::named((subject).text()),
        ),
        IdentityErrorCode::InvalidContext,
        Some(IdentityField::Owners),
        Some(0),
    );
    error(
        doc.symbol(
            "Value",
            subject,
            &[&a, &a],
            LogicalDeclaration::named((subject).text()),
        ),
        IdentityErrorCode::InvalidContext,
        Some(IdentityField::Owners),
        Some(1),
    );
    error(
        doc.symbol(
            "Pair",
            source.range(2, 3).unwrap(),
            &[&a],
            LogicalDeclaration::named((source.range(2, 3).unwrap()).text()),
        ),
        IdentityErrorCode::InvalidContext,
        Some(IdentityField::Owners),
        Some(0),
    );
    let child = {
        let temporary = doc
            .symbol(
                "Pair",
                source.range(8, 9).unwrap(),
                &[],
                LogicalDeclaration::named((source.range(8, 9).unwrap()).text()),
            )
            .unwrap();
        doc.symbol(
            "Value",
            subject,
            &[&temporary],
            LogicalDeclaration::named((subject).text()),
        )
        .unwrap()
    };
    assert_eq!(child.context()[0].label(), "Pair");
    assert_eq!(
        (child.context()[0].start(), child.context()[0].end()),
        (8, 9)
    );
}

#[test]
fn structural_caps_exact_and_one_below_cover_every_field() {
    let ctx = context();
    for (field, exact) in [
        (IdentityField::SourceBytes, 27),
        (IdentityField::PathComponents, 2),
        (IdentityField::PathComponentBytes, 4),
        (IdentityField::Language, 14),
        (IdentityField::Grammar, 14),
    ] {
        for accepted in [true, false] {
            let mut p = policy();
            let cap = if accepted { exact } else { exact - 1 };
            let mut paths = &["a.go"][..];
            let mut language = "go";
            let mut grammar = "tree-sitter-go";
            match field {
                IdentityField::SourceBytes => p.source_bytes = cap,
                IdentityField::PathComponents => {
                    p.path_components = cap;
                    paths = &["a", "b"];
                }
                IdentityField::PathComponentBytes => p.path_component_bytes = cap,
                IdentityField::Language => {
                    p.identity_text_bytes = cap;
                    language = "abcdefghijklmn";
                    grammar = "g";
                }
                IdentityField::Grammar => p.identity_text_bytes = cap,
                _ => unreachable!(),
            }
            let result = IdentityDocument::new(
                IdentityDocumentInput {
                    relative_components: paths,
                    language,
                    grammar,
                    original_bytes: GO,
                },
                IdentityLimits::new(p).unwrap(),
                &ctx,
            );
            if accepted {
                result.ok().unwrap();
            } else {
                error(
                    result,
                    IdentityErrorCode::LimitExceeded,
                    Some(field),
                    if field == IdentityField::PathComponentBytes {
                        Some(0)
                    } else {
                        None
                    },
                );
            }
        }
    }
    for (maximum, accepted) in [(14, true), (13, false)] {
        let mut p = policy();
        p.identity_text_bytes = maximum;
        let doc = IdentityDocument::new(
            IdentityDocumentInput {
                relative_components: &["a"],
                language: "go",
                grammar: "g",
                original_bytes: GO,
            },
            IdentityLimits::new(p).unwrap(),
            &ctx,
        )
        .unwrap();
        let result = doc.symbol(
            "abcdefghijklmn",
            SourceText::new(GO, 4096).unwrap().range(18, 21).unwrap(),
            &[],
            LogicalDeclaration::named(
                (SourceText::new(GO, 4096).unwrap().range(18, 21).unwrap()).text(),
            ),
        );
        if accepted {
            result.ok().unwrap();
        } else {
            error(
                result,
                IdentityErrorCode::LimitExceeded,
                Some(IdentityField::Label),
                None,
            );
        }
    }
    for (maximum, accepted) in [(67, true), (66, false)] {
        let mut p = policy();
        p.uid_bytes = maximum;
        let doc = IdentityDocument::new(
            IdentityDocumentInput {
                relative_components: &["a.go"],
                language: "go",
                grammar: "tree-sitter-go",
                original_bytes: GO,
            },
            IdentityLimits::new(p).unwrap(),
            &ctx,
        )
        .unwrap();
        let result = doc.symbol(
            "Function",
            SourceText::new(GO, 4096).unwrap().range(18, 21).unwrap(),
            &[],
            LogicalDeclaration::named(
                (SourceText::new(GO, 4096).unwrap().range(18, 21).unwrap()).text(),
            ),
        );
        if accepted {
            assert_eq!(result.ok().unwrap().uid().as_str().len(), 67);
        } else {
            error(
                result,
                IdentityErrorCode::LimitExceeded,
                Some(IdentityField::UidBytes),
                None,
            );
        }
    }
    let mut p = policy();
    p.owner_depth = 1;
    let doc = IdentityDocument::new(
        IdentityDocumentInput {
            relative_components: &["x"],
            language: "json",
            grammar: "g",
            original_bytes: JSON,
        },
        IdentityLimits::new(p).unwrap(),
        &ctx,
    )
    .unwrap();
    let source = SourceText::new(JSON, 4096).unwrap();
    let outer = doc
        .symbol(
            "Pair",
            source.range(2, 3).unwrap(),
            &[],
            LogicalDeclaration::named((source.range(2, 3).unwrap()).text()),
        )
        .unwrap();
    let inner = doc
        .symbol(
            "Value",
            source.range(5, 6).unwrap(),
            &[&outer],
            LogicalDeclaration::named((source.range(5, 6).unwrap()).text()),
        )
        .unwrap();
    error(
        doc.symbol(
            "Value",
            source.range(11, 12).unwrap(),
            &[&outer, &inner],
            LogicalDeclaration::named((source.range(11, 12).unwrap()).text()),
        ),
        IdentityErrorCode::LimitExceeded,
        Some(IdentityField::OwnerDepth),
        None,
    );
}

#[test]
fn retained_context_cancel_and_expiry_precede_input_validation() {
    let signal = Cancellation::default();
    let cancelled = SnapshotContext::new(Instant::now() + Duration::from_secs(60), signal.clone());
    signal.cancel();
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    for (ctx, code) in [
        (&cancelled, IdentityErrorCode::Cancelled),
        (&expired, IdentityErrorCode::DeadlineExceeded),
    ] {
        error(
            IdentityDocument::new(
                IdentityDocumentInput {
                    relative_components: &[],
                    language: "",
                    grammar: "",
                    original_bytes: b"",
                },
                limits(),
                ctx,
            ),
            code,
            None,
            None,
        );
    }
    let signal = Cancellation::default();
    let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(60), signal.clone());
    let doc = document(&["a"], GO, &ctx);
    signal.cancel();
    error(
        doc.symbol(
            "",
            SourceText::new(GO, 4096).unwrap().range(18, 21).unwrap(),
            &[],
            LogicalDeclaration::named(
                (SourceText::new(GO, 4096).unwrap().range(18, 21).unwrap()).text(),
            ),
        ),
        IdentityErrorCode::Cancelled,
        None,
        None,
    );
}

#[derive(Debug, Eq, PartialEq)]
enum LookupInputError {
    MissingUid,
    EmptyUid,
}
fn lookup<'a, 'doc, 'o, 'r>(
    index: &BTreeMap<&str, &'a SyntaxIdentity<'doc, 'o, 'r>>,
    uid: Option<&str>,
) -> Result<Option<&'a SyntaxIdentity<'doc, 'o, 'r>>, LookupInputError> {
    match uid {
        None => Err(LookupInputError::MissingUid),
        Some("") => Err(LookupInputError::EmptyUid),
        Some(uid) => Ok(index.get(uid).copied()),
    }
}

fn function_label(profile: &EffectiveProfile) -> &str {
    profile
        .queries()
        .value()
        .iter()
        .find(|query| query.value().graph_label().value().as_str() == "Function")
        .unwrap()
        .value()
        .graph_label()
        .value()
        .as_str()
}
fn profile_document<'o, 'r>(
    paths: &'o [&'o str],
    bytes: &'o [u8],
    profile: &'o EffectiveProfile,
    ctx: &'r SnapshotContext,
) -> IdentityDocument<'o, 'r> {
    IdentityDocument::new(
        IdentityDocumentInput {
            relative_components: paths,
            language: profile.language().value().as_str(),
            grammar: profile.grammar().value().as_str(),
            original_bytes: bytes,
        },
        limits(),
        ctx,
    )
    .unwrap()
}

#[test]
fn used_decoder_catalog_identity_and_uid_only_borrowed_index() {
    let ctx = context();
    let decoder = ProfileDecoder::new(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1_048_576,
            dfa_cache_policy_bytes: 1_048_576,
        })
        .unwrap(),
    );
    let profile_limits = ProfileLimits::new(ProfileLimitsInput {
        input_bytes: 32768,
        nodes: 2048,
        depth: 16,
        text_bytes: 32768,
        scalar_bytes: 4096,
        collection_entries: 256,
    })
    .unwrap();
    let inputs = [
        (
            "go",
            include_bytes!("fixtures/symbols/profiles/go.yaml").as_slice(),
        ),
        (
            "javascript",
            include_bytes!("fixtures/symbols/profiles/javascript.yaml").as_slice(),
        ),
        (
            "json",
            include_bytes!("fixtures/symbols/profiles/json.yaml").as_slice(),
        ),
    ];
    let decoded: Vec<_> = inputs
        .iter()
        .map(|(name, bytes)| {
            let id = ProfileId::new(name, 256).unwrap();
            decoder
                .decode(ProfileInput { id: &id, bytes }, &profile_limits, &ctx)
                .unwrap()
        })
        .collect();
    let catalog_limits = CatalogLimits::new(CatalogLimitsInput {
        policy_entries: 256,
        selector_bytes: 4096,
    })
    .unwrap();
    let catalog = CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles: &decoded,
        }],
        &catalog_limits,
        &ctx,
    )
    .unwrap();

    let effective = |language| {
        catalog
            .profiles()
            .iter()
            .find(|p| p.language().value().as_str() == language)
            .unwrap()
    };
    let go = effective("go");
    let js = effective("javascript");
    let json = effective("json");
    let go_label = function_label(go);
    let js_label = function_label(js);
    let pair_query = json
        .queries()
        .value()
        .iter()
        .find(|q| {
            q.value().graph_label().value().as_str() == "Pair"
                && q.value().value_label().value().as_str() == "Value"
        })
        .unwrap()
        .value();
    let pair_label = pair_query.graph_label().value().as_str();
    let value_label = pair_query.value_label().value().as_str();
    assert_eq!(
        (go_label, js_label, pair_label, value_label),
        ("Function", "Function", "Pair", "Value")
    );
    let a_paths = ["a.go"];
    let b_paths = ["b.go"];
    let js_paths = ["same-line.js"];
    let json_paths = ["values.json"];
    let a = profile_document(&a_paths, GO, go, &ctx);
    let b = profile_document(&b_paths, GO, go, &ctx);
    let jsdoc = profile_document(&js_paths, JS, js, &ctx);
    let jsondoc = profile_document(&json_paths, JSON, json, &ctx);
    let gosource = SourceText::new(GO, 4096).unwrap();
    let jssource = SourceText::new(JS, 4096).unwrap();
    let jsonsource = SourceText::new(JSON, 4096).unwrap();
    let ka = a
        .symbol(
            go_label,
            gosource.range(18, 21).unwrap(),
            &[],
            LogicalDeclaration::named((gosource.range(18, 21).unwrap()).text()),
        )
        .unwrap();
    let kb = b
        .symbol(
            go_label,
            gosource.range(18, 21).unwrap(),
            &[],
            LogicalDeclaration::named((gosource.range(18, 21).unwrap()).text()),
        )
        .unwrap();
    let js1 = jsdoc
        .symbol(
            js_label,
            jssource.range(9, 12).unwrap(),
            &[],
            LogicalDeclaration::named((jssource.range(9, 12).unwrap()).text()),
        )
        .unwrap();
    let js2 = jsdoc
        .symbol(
            js_label,
            jssource.range(27, 30).unwrap(),
            &[],
            LogicalDeclaration::named((jssource.range(27, 30).unwrap()).text()),
        )
        .unwrap();
    let pair1 = jsondoc
        .symbol(
            pair_label,
            jsonsource.range(2, 3).unwrap(),
            &[],
            LogicalDeclaration::named((jsonsource.range(2, 3).unwrap()).text()),
        )
        .unwrap();
    let pair2 = jsondoc
        .symbol(
            pair_label,
            jsonsource.range(8, 9).unwrap(),
            &[],
            LogicalDeclaration::named((jsonsource.range(8, 9).unwrap()).text()),
        )
        .unwrap();
    let value1 = jsondoc
        .symbol(
            value_label,
            jsonsource.range(5, 6).unwrap(),
            &[&pair1],
            LogicalDeclaration::named((jsonsource.range(5, 6).unwrap()).text()),
        )
        .unwrap();
    let value2 = jsondoc
        .symbol(
            value_label,
            jsonsource.range(11, 12).unwrap(),
            &[&pair2],
            LogicalDeclaration::named((jsonsource.range(11, 12).unwrap()).text()),
        )
        .unwrap();
    let replay = jsondoc
        .symbol(
            value_label,
            jsonsource.range(5, 6).unwrap(),
            &[&pair1],
            LogicalDeclaration::named((jsonsource.range(5, 6).unwrap()).text()),
        )
        .unwrap();
    // All storage is immutable before any reference enters the consumer index.
    let identities = [ka, kb, js1, js2, pair1, pair2, value1, value2, replay];
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/symbols/expected-identity.json")).unwrap();
    let rows = expected["rows"].as_array().unwrap();
    for (key, id) in identities.iter().zip([
        "F01-a",
        "F01-b",
        "F02-first",
        "F02-second",
        "F04-pair-a",
        "F04-pair-b",
        "F04-value-a",
        "F04-value-b",
        "F04-value-repeat",
    ]) {
        let row = rows.iter().find(|row| row["id"] == id).unwrap();
        assert_manual_oracle(key, row);
    }
    let mut index: BTreeMap<&str, &SyntaxIdentity<'_, '_, '_>> = BTreeMap::new();
    for key in &identities {
        if let Some(previous) = index.get(key.uid().as_str()) {
            assert!(same_tuple(previous, key));
        } else {
            index.insert(key.uid().as_str(), key);
        }
    }
    assert_eq!(index.len(), 8);
    for key in &identities {
        let found = lookup(&index, Some(key.uid().as_str())).unwrap().unwrap();
        assert!(same_tuple(found, key));
        assert_eq!(found.subject().text(), key.subject().text());
    }
    assert!(std::ptr::eq(
        lookup(&index, Some(identities[8].uid().as_str()))
            .unwrap()
            .unwrap(),
        &identities[6]
    ));
    assert_eq!(
        lookup(&index, None).err(),
        Some(LookupInputError::MissingUid)
    );
    assert_eq!(
        lookup(&index, Some("")).err(),
        Some(LookupInputError::EmptyUid)
    );
    for unknown in ["Run", "Function", "18:21", " ", "unknown"] {
        assert!(lookup(&index, Some(unknown)).unwrap().is_none());
    }
}

#[test]
fn actual_root_exact_projection_composes_without_coercing_other_origins() {
    let ctx = context();
    let doc = document(&["a.go"], GO, &ctx);
    let direct = [OriginSegment::Direct {
        prepared: ByteSpan { start: 0, end: 27 },
        original: ByteSpan { start: 0, end: 27 },
    }];
    let prepared = PreparedInput::new(
        GO,
        GO,
        OriginPlan {
            segments: &direct,
            removed_original: &[],
            anchors: &[],
        },
        &ctx,
    )
    .unwrap();
    let exact = match prepared.project_prepared(18, 21).unwrap() {
        OriginalProjection::Exact(range) => range,
        _ => panic!("declared direct root must be exact"),
    };
    let projected = doc
        .symbol(
            "Function",
            exact,
            &[],
            LogicalDeclaration::named((exact).text()),
        )
        .unwrap();
    let direct = doc
        .symbol(
            "Function",
            SourceText::new(GO, 4096).unwrap().range(18, 21).unwrap(),
            &[],
            LogicalDeclaration::named(
                (SourceText::new(GO, 4096).unwrap().range(18, 21).unwrap()).text(),
            ),
        )
        .unwrap();
    assert!(same_tuple(&projected, &direct));
    assert_eq!(projected.uid().as_str(), direct.uid().as_str());
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan { start: 0, end: 1 },
            original: ByteSpan { start: 0, end: 1 },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan { start: 1, end: 2 },
            reason: SyntheticReason::Wrapper,
        },
        OriginSegment::Direct {
            prepared: ByteSpan { start: 2, end: 3 },
            original: ByteSpan { start: 1, end: 2 },
        },
    ];
    let prepared = PreparedInput::new(
        b"ab",
        b"a!b",
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &ctx,
    )
    .unwrap();
    assert!(matches!(
        prepared.project_prepared(1, 2).unwrap(),
        OriginalProjection::Synthetic(SyntheticReason::Wrapper)
    ));
    assert!(
        matches!(prepared.project_prepared(0,3).unwrap(),OriginalProjection::Compound(pieces) if pieces.len()==3)
    );
}
