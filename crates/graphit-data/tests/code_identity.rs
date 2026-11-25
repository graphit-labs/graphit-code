use graphit_data::{
    CodeDeclaration, CodeFileId, CodePrimaryKey, CodeRelationOccurrence, CodeRelationUid,
    CodeScope, CodeUid, GraphAnchorKey,
};

fn file() -> CodeFileId {
    CodeFileId::from_components(&["src", "scope.rs"]).unwrap()
}

fn named(file: CodeFileId, name: &str, declared_type: Option<&str>, ordinal: u64) -> CodeUid {
    CodeUid::derive(
        file,
        CodeDeclaration::Fallback {
            language: "rust",
            kind: "Function",
            owners: &[],
            name,
            declared_type,
            anonymous: false,
            ordinal,
        },
    )
    .unwrap()
}

#[test]
fn compact_text_roundtrips_and_retains_only_a_logical_file_hint() {
    let uid = named(file(), "parse", Some("fn(&str) -> Value"), 0);
    let text = uid.to_string();
    assert_eq!(text.len(), 67);
    assert!(text.starts_with("u1:"));
    assert_eq!(text.parse::<CodeUid>().unwrap(), uid);
    assert_eq!(uid.file_hint(), file().hint());
    assert_eq!(uid, named(file(), "parse", Some("fn(&str) -> Value"), 0));
}

#[test]
fn persisted_recipe_matches_an_independent_sha256_and_base64_vector() {
    // Independently generated with Python hashlib, struct.pack("<Q", len(field))
    // and urlsafe_b64encode. The literal vector fixes framing and domain versions.
    let uid = named(file(), "parse", Some("fn(&str) -> Value"), 0);
    assert_eq!(
        uid.to_string(),
        "u1:IBmzWtZAAwkwBrpf91RyG8KaGQAd_0vEiHf_JRVL91uHt2B32YKErp0oQbSStZ1J"
    );
    assert_eq!(
        CodePrimaryKey::derive(uid, "first").unwrap().to_string(),
        "k1:IBmzWtZAAwkwBrpf91RyG1hTyU4K8O1Xrm9lQlDXkVI5v-1xz0vRTCkabQsO4YQ5"
    );
}

#[test]
fn enclosing_declarations_and_original_scip_scopes_remain_distinct() {
    let first = named(file(), "First", None, 0);
    let second = named(file(), "Second", None, 0);
    let member = |owners: &[CodeScope<'_>]| {
        CodeUid::derive(
            file(),
            CodeDeclaration::Fallback {
                language: "rust",
                kind: "Method",
                owners,
                name: "same",
                declared_type: None,
                anonymous: false,
                ordinal: 0,
            },
        )
        .unwrap()
    };
    assert_ne!(
        member(&[CodeScope::Declaration(first)]),
        member(&[CodeScope::Declaration(second)])
    );
    assert_ne!(
        member(&[
            CodeScope::Declaration(first),
            CodeScope::Declaration(second)
        ]),
        member(&[
            CodeScope::Declaration(second),
            CodeScope::Declaration(first)
        ])
    );
    assert_ne!(
        member(&[CodeScope::ScipSymbol(
            "rust-analyzer cargo demo 1.0.0 First#"
        )]),
        member(&[CodeScope::ScipSymbol(
            "rust-analyzer cargo demo 1.0.0 Second#"
        )])
    );
    assert_ne!(
        member(&[CodeScope::ScipSymbol(&first.to_string())]),
        member(&[CodeScope::Declaration(first)])
    );
}

#[test]
fn active_data_views_keep_distinct_primary_keys_for_the_same_uid() {
    let uid = named(file(), "answer", Some("number"), 0);
    let first = CodePrimaryKey::derive(uid, "first").unwrap();
    let second = CodePrimaryKey::derive(uid, "second").unwrap();
    assert_ne!(first, second);
    assert_eq!(first, CodePrimaryKey::derive(uid, "first").unwrap());
    assert_eq!(first.file_hint(), uid.file_hint());
    assert_eq!(second.file_hint(), uid.file_hint());
    assert_eq!(first.to_string().len(), 67);
    assert_eq!(first.to_string().parse::<CodePrimaryKey>().unwrap(), first);
}

#[test]
fn parallel_logical_relations_retain_occurrences_and_projection_endpoints() {
    let relation = |source, target, kind: &str, query, ordinal| {
        let occurrence =
            CodeRelationOccurrence::derive(file(), source, kind, query, "callee", ordinal)?;
        CodeRelationUid::derive(file(), source, target, kind, occurrence)
    };
    let source = CodePrimaryKey::derive(named(file(), "caller", None, 0), "functions").unwrap();
    let source_text = source.to_string();
    let source = GraphAnchorKey::String(&source_text);
    let target_uid = named(file(), "callee", None, 0);
    let target = CodePrimaryKey::derive(target_uid, "functions").unwrap();
    let target_text = target.to_string();
    let target = GraphAnchorKey::String(&target_text);
    let edge = relation(source, target, "CALLS", Some("calls"), 0).unwrap();
    assert_eq!(edge.to_string().len(), 67);
    assert_eq!(edge.to_string().parse::<CodeRelationUid>().unwrap(), edge);
    assert_eq!(edge.file_hint(), file().hint());
    assert_eq!(
        edge,
        relation(source, target, "CALLS", Some("calls"), 0).unwrap()
    );
    assert_ne!(
        edge,
        relation(source, target, "CALLS", Some("calls"), 1).unwrap()
    );
    assert_ne!(
        edge,
        relation(target, source, "CALLS", Some("calls"), 0).unwrap()
    );
    assert_ne!(
        edge,
        relation(source, target, "REFERENCES", Some("calls"), 0).unwrap()
    );
    assert_ne!(
        edge,
        relation(source, target, "CALLS", Some("alternate"), 0).unwrap()
    );
    let alternate = CodePrimaryKey::derive(target_uid, "alternate").unwrap();
    let alternate_text = alternate.to_string();
    assert_ne!(
        edge,
        relation(
            source,
            GraphAnchorKey::String(&alternate_text),
            "CALLS",
            Some("calls"),
            0
        )
        .unwrap()
    );
    assert_ne!(
        relation(source, target, "CONTAINS", None, 0).unwrap(),
        relation(source, target, "CONTAINS", Some("contains"), 0).unwrap()
    );
    assert!(relation(source, target, "", None, 0).is_err());
    assert!(relation(source, target, "CALLS", Some(""), 0).is_err());
    assert!(edge.to_string().parse::<CodeUid>().is_err());
}

#[test]
fn deferred_reference_keeps_its_occurrence_before_the_target_join() {
    let source = CodePrimaryKey::derive(named(file(), "caller", None, 0), "functions").unwrap();
    let target = CodePrimaryKey::derive(named(file(), "callee", None, 0), "functions").unwrap();
    let source_text = source.to_string();
    let source = GraphAnchorKey::String(&source_text);
    let target_text = target.to_string();
    let target = GraphAnchorKey::String(&target_text);
    let occurrence =
        CodeRelationOccurrence::derive(file(), source, "CALLS", Some("calls"), "callee", 1)
            .unwrap();
    assert_eq!(
        occurrence.to_string(),
        "o1:IBmzWtZAAwkwBrpf91RyGz0geeq47CCGEnZNIs1Q8wKIYpfsM_eN-17sxtB78HXk"
    );
    assert_eq!(
        occurrence
            .to_string()
            .parse::<CodeRelationOccurrence>()
            .unwrap(),
        occurrence
    );
    assert_eq!(occurrence.file_hint(), file().hint());
    let resolved = CodeRelationUid::derive(file(), source, target, "CALLS", occurrence).unwrap();
    assert_eq!(
        resolved.to_string(),
        "r1:IBmzWtZAAwkwBrpf91RyG_flCQSBaAmb3Vv2_OvYOivZ5LqjlUeYDGm6eCP5Lsqw"
    );
    let alias =
        CodeRelationOccurrence::derive(file(), source, "CALLS", Some("calls"), "alias", 1).unwrap();
    assert_ne!(
        resolved,
        CodeRelationUid::derive(file(), source, target, "CALLS", alias).unwrap()
    );
    let next = CodeRelationOccurrence::derive(file(), source, "CALLS", Some("calls"), "callee", 2)
        .unwrap();
    assert_ne!(
        resolved,
        CodeRelationUid::derive(file(), source, target, "CALLS", next).unwrap()
    );
    let other = CodePrimaryKey::derive(named(file(), "other", None, 0), "functions").unwrap();
    assert_ne!(
        resolved,
        CodeRelationUid::derive(
            file(),
            source,
            GraphAnchorKey::String(&other.to_string()),
            "CALLS",
            occurrence
        )
        .unwrap()
    );
    assert!(occurrence.to_string().parse::<CodeRelationUid>().is_err());
}

#[test]
fn generic_sdk_keys_keep_literal_types_and_the_associated_file() {
    let source = GraphAnchorKey::String("opaque caller");
    let reference =
        CodeRelationOccurrence::derive(file(), source, "CALLS", None, "target", 0).unwrap();
    let numeric =
        CodeRelationUid::derive(file(), source, GraphAnchorKey::Int64(1), "CALLS", reference)
            .unwrap();
    let text = CodeRelationUid::derive(
        file(),
        source,
        GraphAnchorKey::String("1"),
        "CALLS",
        reference,
    )
    .unwrap();
    assert_ne!(numeric, text);
    let integer_source = CodeRelationOccurrence::derive(
        file(),
        GraphAnchorKey::Int64(1),
        "CALLS",
        None,
        "target",
        0,
    )
    .unwrap();
    let string_source = CodeRelationOccurrence::derive(
        file(),
        GraphAnchorKey::String("1"),
        "CALLS",
        None,
        "target",
        0,
    )
    .unwrap();
    assert_ne!(integer_source, string_source);
    let other_file = CodeFileId::from_components(&["other", "scope.rs"]).unwrap();
    let other_reference =
        CodeRelationOccurrence::derive(other_file, source, "CALLS", None, "target", 0).unwrap();
    assert_ne!(reference, other_reference);
    assert_eq!(other_reference.file_hint(), other_file.hint());
}

#[test]
fn semantic_changes_and_duplicate_ordinals_do_not_alias_declarations() {
    let original = named(file(), "parse", Some("fn(&str) -> Value"), 0);
    assert_ne!(
        original,
        named(file(), "other", Some("fn(&str) -> Value"), 0)
    );
    assert_ne!(
        original,
        named(file(), "parse", Some("fn(i64) -> Value"), 0)
    );
    assert_ne!(
        original,
        named(file(), "parse", Some("fn(&str) -> Value"), 1)
    );
    let moved = CodeFileId::from_components(&["other", "scope.rs"]).unwrap();
    assert_ne!(
        original,
        named(moved, "parse", Some("fn(&str) -> Value"), 0)
    );
    assert_ne!(
        CodeFileId::from_components(&["a", "bc"]).unwrap(),
        CodeFileId::from_components(&["ab", "c"]).unwrap()
    );
}

#[test]
fn scip_symbols_keep_declaration_multiplicity_and_their_own_domain() {
    let symbol = "rust-analyzer cargo demo 1.0.0 scope/parse().";
    let uid = CodeUid::derive(
        file(),
        CodeDeclaration::Scip {
            symbols: &[symbol],
            kind: "Function",
            ordinal: 0,
        },
    )
    .unwrap();
    let duplicate = CodeUid::derive(
        file(),
        CodeDeclaration::Scip {
            symbols: &[symbol],
            kind: "Function",
            ordinal: 1,
        },
    )
    .unwrap();
    assert_ne!(uid, duplicate);
    assert_ne!(uid, named(file(), "parse", None, 0));
    let alias = "rust-analyzer cargo demo 1.0.0 scope/parse_alias().";
    let aliases = CodeUid::derive(
        file(),
        CodeDeclaration::Scip {
            symbols: &[symbol, alias],
            kind: "Function",
            ordinal: 0,
        },
    )
    .unwrap();
    assert_ne!(uid, aliases);
    for symbols in [vec![], vec![symbol, symbol], vec![alias, symbol]] {
        assert!(
            CodeUid::derive(
                file(),
                CodeDeclaration::Scip {
                    symbols: &symbols,
                    kind: "Function",
                    ordinal: 0,
                },
            )
            .is_err()
        );
    }
}

#[test]
fn noncanonical_text_and_invalid_logical_paths_are_rejected() {
    for components in [vec![], vec!["."], vec![".."], vec!["a/b"], vec!["a\0b"]] {
        assert!(CodeFileId::from_components(&components).is_err());
    }
    let text = named(file(), "parse", None, 0).to_string();
    assert!(format!("{text}=").parse::<CodeUid>().is_err());
    assert!(text.replacen("u1:", "u2:", 1).parse::<CodeUid>().is_err());
    assert!(text.parse::<CodePrimaryKey>().is_err());
    assert!(CodePrimaryKey::derive(named(file(), "parse", None, 0), "").is_err());
    let malformed = text.replacen(':', ":=", 1);
    assert!(malformed.parse::<CodeUid>().is_err());
    let mut invalid_alphabet = text.clone().into_bytes();
    invalid_alphabet[10] = b'/';
    assert!(
        String::from_utf8(invalid_alphabet)
            .unwrap()
            .parse::<CodeUid>()
            .is_err()
    );
    assert!(CodePrimaryKey::derive(named(file(), "parse", None, 0), "first\0second").is_err());
}
