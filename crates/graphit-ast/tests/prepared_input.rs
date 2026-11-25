//! Public-output tests for the exported origin-map API after Root Stage activation.
use graphit_ast::prepared_input::*;
use graphit_ast::{RangeError, SourceRange};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}
fn span(start: u64, end: u64) -> ByteSpan {
    ByteSpan { start, end }
}
fn direct(prepared: ByteSpan, original: ByteSpan) -> OriginSegment<'static> {
    OriginSegment::Direct { prepared, original }
}
fn plan<'p>(
    segments: &'p [OriginSegment<'p>],
    removed: &'p [ByteSpan],
    anchors: &'p [OriginAnchor<'p>],
) -> OriginPlan<'p> {
    OriginPlan {
        segments,
        removed_original: removed,
        anchors,
    }
}
fn bytes(value: &Value) -> Vec<u8> {
    let hex = value["hex"].as_str().expect("hex");
    assert!(hex.len().is_multiple_of(2));
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII hex"), 16).expect("hex byte")
        })
        .collect()
}
fn fixture_span(value: &Value) -> ByteSpan {
    let a = value.as_array().expect("span");
    assert_eq!(a.len(), 2);
    span(a[0].as_u64().expect("start"), a[1].as_u64().expect("end"))
}
fn synthetic(value: &str) -> SyntheticReason {
    match value {
        "InsertedByTransform" => SyntheticReason::InsertedByTransform,
        "RecoveryToken" => SyntheticReason::RecoveryToken,
        "Wrapper" => SyntheticReason::Wrapper,
        _ => panic!("unknown fixture synthetic reason"),
    }
}
fn reason(value: UnmappableReason) -> &'static str {
    match value {
        UnmappableReason::PartialMappedAtom => "PartialMappedAtom",
        UnmappableReason::MissingAnchor => "MissingAnchor",
        UnmappableReason::RemovedPortion => "RemovedPortion",
    }
}
fn synthetic_name(value: SyntheticReason) -> &'static str {
    match value {
        SyntheticReason::InsertedByTransform => "InsertedByTransform",
        SyntheticReason::RecoveryToken => "RecoveryToken",
        SyntheticReason::Wrapper => "Wrapper",
    }
}
fn checked_range(range: SourceRange<'_>, original: &str) -> Value {
    let expected = original
        .get(range.start()..range.end())
        .expect("original byte range");
    assert_eq!(range.text(), expected);
    assert_eq!(
        range.text().as_ptr(),
        expected.as_ptr(),
        "range must borrow original bytes"
    );
    json!([range.start(), range.end()])
}
fn error_json(error: OriginError) -> Value {
    let name = match error {
        OriginError::Range(RangeError::NotCharBoundary) => "NotCharBoundary",
        OriginError::Range(RangeError::ReversedRange) => "ReversedRange",
        OriginError::Range(RangeError::OutOfBounds) => "OutOfBounds",
        OriginError::Range(RangeError::InvalidUtf8 { .. }) => "InvalidUtf8",
        OriginError::Cancelled => "Cancelled",
        OriginError::DeadlineExceeded => "DeadlineExceeded",
        OriginError::ContextDenied => "ContextDenied",
        OriginError::ArithmeticOverflow => "ArithmeticOverflow",
        OriginError::InvalidPlan => "InvalidPlan",
        OriginError::Allocation => "Allocation",
        OriginError::Range(_) => panic!("unexpected range error"),
    };
    json!({"error":name})
}
fn forward_json(result: Result<OriginalProjection<'_>, OriginError>, original: &str) -> Value {
    match result {
        Err(e) => error_json(e),
        Ok(OriginalProjection::Exact(r)) => {
            json!({"kind":"Exact","original":checked_range(r,original)})
        }
        Ok(OriginalProjection::Synthetic(r)) => {
            json!({"kind":"Synthetic","reason":synthetic_name(r)})
        }
        Ok(OriginalProjection::Unmappable(r)) => json!({"kind":"Unmappable","reason":reason(r)}),
        Ok(OriginalProjection::Compound(pieces)) => {
            let pieces: Vec<_> = pieces
                .into_iter()
                .map(|piece| match piece {
                    OriginPiece::Original(r) => {
                        json!({"kind":"Original","original":checked_range(r,original)})
                    }
                    OriginPiece::Synthetic(r) => {
                        json!({"kind":"Synthetic","reason":synthetic_name(r)})
                    }
                    OriginPiece::Unmappable(r) => json!({"kind":"Unmappable","reason":reason(r)}),
                })
                .collect();
            json!({"kind":"Compound","pieces":pieces})
        }
        Ok(OriginalProjection::Ambiguous(candidates)) => {
            let points: Vec<_> = candidates
                .into_iter()
                .map(|r| {
                    checked_range(r, original);
                    assert_eq!(r.start(), r.end());
                    r.start()
                })
                .collect();
            json!({"kind":"Ambiguous","original_point_candidates":points})
        }
    }
}
fn spans_json(spans: Vec<ByteSpan>) -> Vec<Value> {
    spans.into_iter().map(|s| json!([s.start, s.end])).collect()
}
fn inverse_json(result: Result<PreparedProjection, OriginError>) -> Value {
    match result {
        Err(e) => error_json(e),
        Ok(PreparedProjection::Unique(p)) => json!({"kind":"Unique","prepared":spans_json(p)}),
        Ok(PreparedProjection::Ambiguous(candidates)) => {
            let candidates: Vec<_> = candidates.into_iter().map(spans_json).collect();
            json!({"kind":"Ambiguous","prepared_candidates":candidates})
        }
        Ok(PreparedProjection::Removed) => json!({"kind":"Removed"}),
        Ok(PreparedProjection::Unmappable(r)) => json!({"kind":"Unmappable","reason":reason(r)}),
    }
}
fn assert_error<T>(result: Result<T, OriginError>, expected: OriginError) {
    match result {
        Ok(_) => panic!("expected bounded error {expected}"),
        Err(actual) => assert_eq!(actual, expected),
    }
}

#[test]
fn all_coordinate_fixtures_preserve_public_ranges_pieces_and_candidates() {
    let packet: Value =
        serde_json::from_str(include_str!("fixtures/antlr_coordinates/coordinates.json"))
            .expect("fixture JSON");
    let fixtures = packet["fixtures"].as_array().expect("fixtures");
    assert_eq!(fixtures.len(), 25);
    let mut queries = 0;
    for fixture in fixtures {
        let original = bytes(&fixture["original"]);
        let prepared = bytes(&fixture["prepared"]);
        let raw_segments = fixture["segments"].as_array().expect("segments");
        let fragments: Vec<Vec<ByteSpan>> = raw_segments
            .iter()
            .map(|s| {
                s["original_fragments"]
                    .as_array()
                    .map(|a| a.iter().map(fixture_span).collect())
                    .unwrap_or_default()
            })
            .collect();
        let segments: Vec<_> = raw_segments
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let prepared = fixture_span(&s["prepared"]);
                match s["kind"].as_str().expect("segment kind") {
                    "Direct" => OriginSegment::Direct {
                        prepared,
                        original: fixture_span(&s["original"]),
                    },
                    "MappedAtom" => OriginSegment::MappedAtom {
                        prepared,
                        original_fragments: &fragments[i],
                    },
                    "Synthetic" => OriginSegment::Synthetic {
                        prepared,
                        reason: synthetic(s["reason"].as_str().expect("reason")),
                    },
                    _ => panic!("unknown segment"),
                }
            })
            .collect();
        let removed: Vec<_> = fixture["removed_original"]
            .as_array()
            .expect("removed")
            .iter()
            .map(fixture_span)
            .collect();
        let raw_anchors = fixture["anchors"].as_array().expect("anchors");
        let points: Vec<Vec<u64>> = raw_anchors
            .iter()
            .map(|a| {
                a["original_candidates"]
                    .as_array()
                    .expect("candidates")
                    .iter()
                    .map(|p| p.as_u64().expect("point"))
                    .collect()
            })
            .collect();
        let anchors: Vec<_> = raw_anchors
            .iter()
            .enumerate()
            .map(|(i, a)| OriginAnchor {
                prepared: a["prepared"].as_u64().expect("anchor"),
                original_candidates: &points[i],
            })
            .collect();
        let ctx = context();
        let result = PreparedInput::new(
            &original,
            &prepared,
            plan(&segments, &removed, &anchors),
            &ctx,
        );
        let input = match fixture["expected_constructor"]
            .as_str()
            .expect("constructor")
        {
            "Valid" => result.unwrap_or_else(|e| panic!("{} constructor: {e}", fixture["id"])),
            "InvalidUtf8" => {
                assert!(matches!(
                    result,
                    Err(OriginError::Range(RangeError::InvalidUtf8 {
                        valid_up_to: 0,
                        error_len: Some(1)
                    }))
                ));
                continue;
            }
            "InvalidPlanByteMismatch" | "InvalidPlanPreparedCoverage" => {
                assert_error(result, OriginError::InvalidPlan);
                continue;
            }
            _ => panic!("unknown constructor expectation"),
        };
        let original_text = std::str::from_utf8(&original).expect("original");
        let prepared_text = std::str::from_utf8(&prepared).expect("prepared");
        assert_eq!(input.original_text(), original_text);
        assert_eq!(input.original_text().as_ptr(), original.as_ptr());
        assert_eq!(input.prepared_text(), prepared_text);
        assert_eq!(input.prepared_text().as_ptr(), prepared.as_ptr());
        // The 22 C++ scalar/BOM token queries are retired, not reinterpreted.
        // Keep the 6 explicit byte/inverse queries and all 25 constructor cases.
        for query in fixture["queries"]
            .as_array()
            .expect("queries")
            .iter()
            .filter(|query| {
                matches!(
                    query["operation"].as_str(),
                    Some("project_prepared" | "inverse_original")
                )
            })
        {
            queries += 1;
            let actual = match query["operation"].as_str().expect("operation") {
                "project_prepared" => {
                    let s = fixture_span(&query["args"]);
                    forward_json(input.project_prepared(s.start, s.end), original_text)
                }
                "inverse_original" => {
                    let s = fixture_span(&query["args"]);
                    inverse_json(input.inverse_original(s.start, s.end))
                }
                _ => panic!("unknown operation"),
            };
            assert_eq!(
                actual, query["expected_projection"],
                "fixture {}",
                fixture["id"]
            );
            if !query["expected_prepared"].is_null() {
                let s = fixture_span(&query["expected_prepared"]);
                let r = input.prepared_span(s.start, s.end).expect("prepared span");
                assert_eq!(
                    r.text(),
                    prepared_text
                        .get(r.start()..r.end())
                        .expect("prepared slice")
                );
            }
        }
    }
    assert_eq!(queries, 6);
}

#[test]
fn inverse_exact_atomic_cross_segment_and_removed_relations() {
    let ctx = context();
    let fragments = [span(0, 1), span(1, 2)];
    let segments = [
        OriginSegment::MappedAtom {
            prepared: span(0, 1),
            original_fragments: &fragments,
        },
        direct(span(1, 2), span(2, 3)),
    ];
    let input = PreparedInput::new(b"abc", b"Xc", plan(&segments, &[], &[]), &ctx).expect("atom");
    assert_eq!(
        input.inverse_original(0, 3),
        Ok(PreparedProjection::Unique(vec![span(0, 1), span(1, 2)]))
    );
    assert_eq!(
        input.inverse_original(0, 1),
        Ok(PreparedProjection::Unmappable(
            UnmappableReason::PartialMappedAtom
        ))
    );
    let disjoint = [span(0, 1), span(2, 3)];
    let atom = [OriginSegment::MappedAtom {
        prepared: span(0, 1),
        original_fragments: &disjoint,
    }];
    let removed = [span(1, 2)];
    let input =
        PreparedInput::new(b"a+b", b"X", plan(&atom, &removed, &[]), &ctx).expect("disjoint");
    assert_eq!(
        input.inverse_original(1, 2),
        Ok(PreparedProjection::Removed)
    );
    assert_eq!(
        input.inverse_original(0, 3),
        Ok(PreparedProjection::Unmappable(
            UnmappableReason::RemovedPortion
        ))
    );
    assert_eq!(
        input.inverse_original(0, 1),
        Ok(PreparedProjection::Unmappable(
            UnmappableReason::PartialMappedAtom
        ))
    );
}

#[test]
fn inverse_enumerates_all_complete_paths_without_truncation() {
    let ctx = context();
    let segments = [
        direct(span(0, 1), span(0, 1)),
        direct(span(1, 3), span(0, 2)),
    ];
    let input = PreparedInput::new(b"ab", b"aab", plan(&segments, &[], &[]), &ctx).expect("copies");
    assert_eq!(
        input.inverse_original(0, 2),
        Ok(PreparedProjection::Ambiguous(vec![
            vec![span(0, 1), span(2, 3)],
            vec![span(1, 2), span(2, 3)],
        ]))
    );
}

#[test]
fn empty_inverse_uses_strict_interiors_or_explicit_anchors() {
    let ctx = context();
    let segments = [
        direct(span(0, 2), span(0, 2)),
        direct(span(2, 4), span(0, 2)),
    ];
    let input =
        PreparedInput::new(b"ab", b"abab", plan(&segments, &[], &[]), &ctx).expect("copies");
    assert_eq!(
        input.inverse_original(1, 1),
        Ok(PreparedProjection::Ambiguous(vec![
            vec![span(1, 1)],
            vec![span(3, 3)]
        ]))
    );
    assert_eq!(
        input.inverse_original(0, 0),
        Ok(PreparedProjection::Unmappable(
            UnmappableReason::MissingAnchor
        ))
    );
    let anchors = [
        OriginAnchor {
            prepared: 1,
            original_candidates: &[1],
        },
        OriginAnchor {
            prepared: 2,
            original_candidates: &[0, 2],
        },
    ];
    let input =
        PreparedInput::new(b"ab", b"abab", plan(&segments, &[], &anchors), &ctx).expect("anchors");
    assert_eq!(
        input.inverse_original(1, 1),
        Ok(PreparedProjection::Ambiguous(vec![
            vec![span(1, 1)],
            vec![span(3, 3)]
        ]))
    );
    assert_eq!(
        input.inverse_original(0, 0),
        Ok(PreparedProjection::Unique(vec![span(2, 2)]))
    );
}

#[test]
fn malformed_plans_reject_gaps_overlaps_empty_atoms_and_contradictory_anchors() {
    let ctx = context();
    let whole = [direct(span(0, 2), span(0, 2))];
    let gap = [direct(span(1, 2), span(1, 2))];
    let overlap = [
        direct(span(0, 2), span(0, 2)),
        direct(span(1, 2), span(1, 2)),
    ];
    for segments in [&gap[..], &overlap[..]] {
        assert_error(
            PreparedInput::new(b"ab", b"ab", plan(segments, &[], &[]), &ctx),
            OriginError::InvalidPlan,
        );
    }
    let removed = [span(0, 1)];
    assert_error(
        PreparedInput::new(b"ab", b"ab", plan(&whole, &removed, &[]), &ctx),
        OriginError::InvalidPlan,
    );
    let atom = [OriginSegment::MappedAtom {
        prepared: span(0, 2),
        original_fragments: &[],
    }];
    assert_error(
        PreparedInput::new(b"ab", b"ab", plan(&atom, &[], &[]), &ctx),
        OriginError::InvalidPlan,
    );
    let bad_points = [
        vec![OriginAnchor {
            prepared: 1,
            original_candidates: &[0],
        }],
        vec![OriginAnchor {
            prepared: 0,
            original_candidates: &[0, 0],
        }],
        vec![
            OriginAnchor {
                prepared: 0,
                original_candidates: &[0],
            },
            OriginAnchor {
                prepared: 0,
                original_candidates: &[0],
            },
        ],
        vec![OriginAnchor {
            prepared: 0,
            original_candidates: &[],
        }],
    ];
    for anchors in bad_points {
        assert_error(
            PreparedInput::new(b"ab", b"ab", plan(&whole, &[], &anchors), &ctx),
            OriginError::InvalidPlan,
        );
    }
    let only_synthetic = [OriginSegment::Synthetic {
        prepared: span(0, 2),
        reason: SyntheticReason::Wrapper,
    }];
    assert_error(
        PreparedInput::new(b"ab", b"ab", plan(&only_synthetic, &[], &[]), &ctx),
        OriginError::InvalidPlan,
    );
    let removed_overlap = [span(0, 2), span(1, 2)];
    assert_error(
        PreparedInput::new(b"ab", b"", plan(&[], &removed_overlap, &[]), &ctx),
        OriginError::InvalidPlan,
    );
    let original_gap = [direct(span(0, 1), span(0, 1))];
    assert_error(
        PreparedInput::new(b"ab", b"a", plan(&original_gap, &[], &[]), &ctx),
        OriginError::InvalidPlan,
    );
}

#[test]
fn invalid_projection_does_not_prevent_a_later_valid_projection() {
    let ctx = context();
    let segments = [direct(span(0, 3), span(0, 3))];
    let input =
        PreparedInput::new(b"abc", b"abc", plan(&segments, &[], &[]), &ctx).expect("source");
    assert_error(
        input.project_prepared(0, 4),
        OriginError::Range(RangeError::OutOfBounds),
    );
    assert_eq!(
        forward_json(input.project_prepared(0, 1), "abc"),
        json!({"kind":"Exact","original":[0,1]})
    );
}

#[test]
fn original_context_cancels_constructor_and_every_projection_entrypoint() {
    let signal = Cancellation::default();
    let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(60), signal.clone());
    let segments = [direct(span(0, 1), span(0, 1))];
    let input = PreparedInput::new(b"a", b"a", plan(&segments, &[], &[]), &ctx).expect("source");
    signal.cancel();
    assert_error(input.prepared_span(0, 1), OriginError::Cancelled);
    assert_error(input.project_prepared(0, 1), OriginError::Cancelled);
    assert_error(input.inverse_original(0, 1), OriginError::Cancelled);
    assert_error(
        PreparedInput::new(&[0xff], b"", plan(&[], &[], &[]), &ctx),
        OriginError::Cancelled,
    );
}

#[test]
fn original_expired_deadline_precedes_source_validation() {
    let ctx = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_error(
        PreparedInput::new(&[0xff], &[], plan(&[], &[], &[]), &ctx),
        OriginError::DeadlineExceeded,
    );
    assert_error(
        PreparedInput::new(&[], &[], plan(&[], &[], &[]), &ctx),
        OriginError::DeadlineExceeded,
    );
}

#[test]
fn original_borrows_byte_points_boundaries_and_closed_reasons() {
    let ctx = context();
    let text = "aé\r\n🦀e\u{301}\n";
    let segments = [direct(span(0, 13), span(0, 13))];
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        plan(&segments, &[], &[]),
        &ctx,
    )
    .expect("unicode");
    assert_error(
        input.prepared_span(1, 2),
        OriginError::Range(RangeError::NotCharBoundary),
    );
    assert_error(
        input.prepared_span(3, 1),
        OriginError::Range(RangeError::ReversedRange),
    );
    let r = input.prepared_span(5, 12).expect("exact span");
    assert_eq!((r.start_point().row, r.start_point().column), (1, 0));
    assert_eq!((r.end_point().row, r.end_point().column), (1, 7));
    assert_eq!(r.text(), "🦀e\u{301}");
    for r in [SyntheticReason::Wrapper, SyntheticReason::RecoveryToken] {
        let segments = [OriginSegment::Synthetic {
            prepared: span(0, 1),
            reason: r,
        }];
        let input =
            PreparedInput::new(b"", b"x", plan(&segments, &[], &[]), &ctx).expect("synthetic");
        assert_eq!(
            forward_json(input.project_prepared(0, 1), ""),
            json!({"kind":"Synthetic","reason":synthetic_name(r)})
        );
    }
    let errors = [
        OriginError::Cancelled,
        OriginError::DeadlineExceeded,
        OriginError::ContextDenied,
        OriginError::Range(RangeError::InvalidUtf8 {
            valid_up_to: 0,
            error_len: Some(1),
        }),
        OriginError::ArithmeticOverflow,
        OriginError::InvalidPlan,
        OriginError::Allocation,
    ];
    for error in errors {
        let message = error.to_string();
        assert!(message.len() < 80);
        assert!(!message.contains(text) && !message.contains("eda080") && !message.contains('/'));
    }
}

#[test]
fn empty_input_and_rust_bom_byte_coordinates_preserve_original_boundaries() {
    let ctx = context();
    let anchors = [OriginAnchor {
        prepared: 0,
        original_candidates: &[0],
    }];
    let empty = PreparedInput::new(b"", b"", plan(&[], &[], &anchors), &ctx).unwrap();
    assert_eq!(
        forward_json(empty.project_prepared(0, 0), ""),
        json!({"kind":"Exact","original":[0,0]})
    );
    let text = "\u{feff}é\r\n🦀";
    let segments = [direct(span(0, 11), span(0, 11))];
    let anchors = [OriginAnchor {
        prepared: 11,
        original_candidates: &[11],
    }];
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        plan(&segments, &[], &anchors),
        &ctx,
    )
    .unwrap();
    for (start, end, expected) in [(0, 3, "\u{feff}"), (3, 5, "é"), (7, 11, "🦀")] {
        match input.project_prepared(start, end).unwrap() {
            OriginalProjection::Exact(range) => {
                assert_eq!(range.text(), expected);
                assert_eq!(range.text().as_ptr(), text[start as usize..].as_ptr());
            }
            _ => panic!("direct byte projection"),
        }
    }
    assert_eq!(
        forward_json(input.project_prepared(11, 11), text),
        json!({"kind":"Exact","original":[11,11]})
    );
    let crab = input.prepared_span(7, 11).unwrap();
    assert_eq!((crab.start_point().row, crab.start_point().column), (1, 0));
    assert_eq!((crab.end_point().row, crab.end_point().column), (1, 4));
}
