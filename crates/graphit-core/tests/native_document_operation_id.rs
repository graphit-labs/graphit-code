use graphit_core::artifact_store::NativeDocumentOperationId;
use std::{collections::BTreeSet, collections::HashSet, io};

const CANONICAL: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

#[test]
fn canonical_string_roundtrip_is_compact_data() {
    let id: NativeDocumentOperationId = CANONICAL.parse().unwrap();
    let encoded = serde_json::to_string(&id).unwrap();
    assert_eq!(encoded, format!("\"{CANONICAL}\""));
    let restored: NativeDocumentOperationId = serde_json::from_str(&encoded).unwrap();
    assert!(id == restored);
    assert_eq!(restored.to_string(), CANONICAL);
    assert_eq!(std::mem::size_of::<NativeDocumentOperationId>(), 16);
    assert_eq!(HashSet::from([id, restored]).len(), 1);
    assert_eq!(BTreeSet::from([id, restored]).len(), 1);
}

#[test]
fn generated_ids_have_canonical_wire_and_total_order() {
    let ids: Vec<_> = (0..128)
        .map(|_| NativeDocumentOperationId::generate())
        .collect();
    assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), ids.len());
    for id in &ids {
        let text = id.to_string();
        assert_eq!(text.len(), 26);
        assert!(text.parse::<NativeDocumentOperationId>().unwrap() == *id);
    }
    let ordered: Vec<_> = ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn parser_rejects_noncanonical_alphabet_overflow_and_lengths() {
    for text in [
        "",
        "01ARZ3NDEKTSV4RRFFQ69G5FA",
        "01ARZ3NDEKTSV4RRFFQ69G5FAV0",
        "01arz3ndektsv4rrffq69g5fav",
        "O1ARZ3NDEKTSV4RRFFQ69G5FAV",
        "I1ARZ3NDEKTSV4RRFFQ69G5FAV",
        "L1ARZ3NDEKTSV4RRFFQ69G5FAV",
        "U1ARZ3NDEKTSV4RRFFQ69G5FAV",
        "81ARZ3NDEKTSV4RRFFQ69G5FAV",
        "01ARZ3NDEKTSV4RRFFQ69G5FA ",
        "é1ARZ3NDEKTSV4RRFFQ69G5FA",
    ] {
        assert_eq!(
            text.parse::<NativeDocumentOperationId>()
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(
            serde_json::from_str::<NativeDocumentOperationId>(
                &serde_json::to_string(text).unwrap()
            )
            .is_err()
        );
    }
    for text in ["00000000000000000000000000", "7ZZZZZZZZZZZZZZZZZZZZZZZZZ"] {
        assert_eq!(
            text.parse::<NativeDocumentOperationId>()
                .unwrap()
                .to_string(),
            text
        );
    }
}

#[test]
fn decoder_accepts_only_one_string_value() {
    for input in [
        "null",
        "true",
        "1",
        "[]",
        "{}",
        "{\"id\":\"01ARZ3NDEKTSV4RRFFQ69G5FAV\"}",
        "[\"01ARZ3NDEKTSV4RRFFQ69G5FAV\"]",
        "\"01ARZ3NDEKTSV4RRFFQ69G5FAV\" null",
    ] {
        assert!(serde_json::from_str::<NativeDocumentOperationId>(input).is_err());
    }
}

#[test]
fn oversized_values_fail_with_static_parser_diagnostics() {
    let oversized = "A".repeat(1024 * 1024);
    let error = oversized
        .parse::<NativeDocumentOperationId>()
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(!error.to_string().contains(&oversized));
    let json = serde_json::to_string(&oversized).unwrap();
    let error = serde_json::from_str::<NativeDocumentOperationId>(&json)
        .err()
        .unwrap();
    assert!(!error.to_string().contains(&oversized));
}
