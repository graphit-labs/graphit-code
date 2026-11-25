use graphit_ast::{
    antlr_engine::{
        AntlrEngineError, NodeClass, SpanStatus,
        cobol_copy::{CopyProgramScope, parse_cobol_semantic_occurrences},
        preprocessing::{CobolFormat, prepare_cobol},
        semantic::{
            CaptureBinding, CapturedNode, SemanticError, SemanticQueryOrigin,
            parse_semantic_occurrences,
        },
    },
    catalog::{
        CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind,
        ProvenanceView,
    },
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, OriginalProjection, PreparedInput},
    profiles::{
        Presence, ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput,
        QuerySource,
    },
    scalar_policy::{RegexLimits, ScalarPolicy},
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{
    error::Error,
    io::ErrorKind,
    time::{Duration, Instant},
};

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}
fn catalog(bytes: &[u8]) -> CatalogDraft {
    let operation = context();
    let id = ProfileId::new("semantic-fixture", 256).unwrap();
    let decoder = ProfileDecoder::new(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })
        .unwrap(),
    );
    let decoded = [decoder
        .decode(
            ProfileInput { id: &id, bytes },
            &ProfileLimits::new(ProfileLimitsInput {
                input_bytes: 1 << 20,
                nodes: 32768,
                depth: 64,
                text_bytes: 1 << 20,
                scalar_bytes: 32768,
                collection_entries: 4096,
            })
            .unwrap(),
            &operation,
        )
        .unwrap()];
    CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles: &decoded,
        }],
        &CatalogLimits::new(CatalogLimitsInput {
            policy_entries: 4096,
            selector_bytes: 32768,
        })
        .unwrap(),
        &operation,
    )
    .unwrap()
}
fn profile(grammar: &str, body: &str) -> CatalogDraft {
    catalog(format!("language: fixture\nparser: antlr4\ngrammar: {grammar}\n{body}").as_bytes())
}
fn with_identity(
    source: &str,
    operation: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>),
) {
    let bound = ByteSpan {
        start: 0,
        end: u64::try_from(source.len()).unwrap(),
    };
    let segments = if source.is_empty() {
        vec![]
    } else {
        vec![OriginSegment::Direct {
            prepared: bound,
            original: bound,
        }]
    };
    let input = PreparedInput::new(
        source.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        operation,
    )
    .unwrap();
    action(&input);
}
fn present<'a, 'o, 'v>(binding: &'a CaptureBinding<'o, 'v>) -> &'a CapturedNode<'o, 'v> {
    match binding {
        CaptureBinding::Present(node) => node,
        _ => panic!("expected present capture"),
    }
}
fn text<'v>(binding: &CaptureBinding<'_, 'v>) -> &'v str {
    present(binding).prepared.as_ref().unwrap().text()
}

const COPY: &str = "queries: [{data_key: custom-copy, graph_label: Include, pattern: '//copyStatement/copySource', source: cobol-preprocessor, type: entity}]\nantlr_contexts: [{pattern: '//programUnit', label: Program, name: [{xpath: '/identificationDivision/programIdParagraph/programName'}]}]\n";
const HELLO: &str =
    "IDENTIFICATION DIVISION.\nPROGRAM-ID. HELLO.\nDATA DIVISION.\nWORKING-STORAGE SECTION.\n";
fn program(copy: &str) -> String {
    format!("{HELLO}{copy}\nPROCEDURE DIVISION.\nSTOP RUN.\nEND PROGRAM HELLO.")
}
fn assert_linked(
    document: &graphit_ast::antlr_engine::cobol_copy::CobolSemanticDocument<'_, '_, '_>,
    index: usize,
) -> usize {
    let item = &document.preprocessor.occurrences[index];
    let CopyProgramScope::Linked {
        main_anchor_node_index,
        program_node_index,
        context_indices,
    } = &item.scope
    else {
        panic!(
            "scope {:?}; diagnostics {:?}",
            item.scope, document.main.syntax.diagnostics
        )
    };
    let keyword = &document.main.syntax.nodes[*main_anchor_node_index];
    assert_eq!(keyword.kind, "COPY");
    assert!(!keyword.missing);
    assert_eq!(keyword.status, SpanStatus::Regular);
    let mut parent = keyword.parent;
    while parent != Some(*program_node_index) {
        parent = document.main.syntax.nodes[parent.unwrap()].parent;
    }
    assert_eq!(
        document.main.syntax.nodes[*program_node_index].kind,
        "programUnit"
    );
    assert_eq!(
        document.main.syntax.nodes[*program_node_index].class,
        NodeClass::Rule
    );
    assert!(!context_indices.is_empty());
    assert!(
        context_indices
            .iter()
            .any(|index| document.main.contexts[*index].matched.node_index == *program_node_index)
    );
    let mut expected = Vec::new();
    let mut parent = keyword.parent;
    while let Some(index) = parent {
        if let Some(context) = document
            .main
            .contexts
            .iter()
            .position(|context| context.matched.node_index == index)
        {
            expected.push(context);
        }
        parent = document.main.syntax.nodes[index].parent;
    }
    assert_eq!(*context_indices, expected);
    assert!(item.occurrence.contexts.is_empty());
    *program_node_index
}

#[test]
fn stock_copy_uses_preprocessor_and_actual_main_program_parent() {
    let yaml = include_bytes!("fixtures/catalog/shipped/cobol85.yaml");
    assert_eq!(
        yaml,
        include_bytes!("fixtures/profiles/shipped/cobol85.yaml")
    );
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/cobol85.yaml")
    );
    let catalog = catalog(yaml);
    let profile = &catalog.profiles()[0];
    let physical = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HELLO.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n           COPY BOOK.\n       PROCEDURE DIVISION.\n           STOP RUN.\n       END PROGRAM HELLO.\n";
    let operation = context();
    let prepared = prepare_cobol(physical, CobolFormat::Fixed, &operation).unwrap();
    prepared.with_origin_plan(|plan| {
        let input = PreparedInput::new(
            physical.as_bytes(),
            prepared.text().as_bytes(),
            plan,
            &operation,
        )
        .unwrap();
        let result = parse_cobol_semantic_occurrences(profile, &input).unwrap();
        assert_eq!(result.preprocessor.grammar(), "Cobol85Preprocessor.g4");
        assert!(
            result.preprocessor.syntax.diagnostics.is_empty(),
            "{:?}",
            result.preprocessor.syntax.diagnostics
        );
        assert_eq!(result.preprocessor.occurrences.len(), 1);
        let item = &result.preprocessor.occurrences[0].occurrence;
        assert_eq!(text(&item.name), "BOOK");
        assert_eq!(item.query.data_key().value(), "imports");
        assert_eq!(item.query.relation_type().value(), "REFERENCES");
        assert_eq!(*item.query.source().value(), QuerySource::CobolPreprocessor);
        assert!(matches!(
            item.query.source().provenance(),
            ProvenanceView::Declared(_)
        ));
        assert!(std::ptr::eq(
            item.query,
            profile.queries().value().as_slice()[item.query_index].value()
        ));
        assert_eq!(
            result.preprocessor.syntax.nodes[item.matched.node_index].kind,
            "copySource"
        );
        assert_linked(&result, 0);
        assert!(
            result.main.syntax.diagnostics.is_empty(),
            "{:?}",
            result.main.syntax.diagnostics
        );
    });
}

#[test]
fn raw_library_replacing_and_custom_capture_multiplicity() {
    let catalog = profile(
        "antlr-cobol85",
        r#"
queries:
  - {data_key: raw, graph_label: Include, source: cobol-preprocessor, pattern: '//copyStatement/copySource', value_label: Library, type: entity, captures: {value: [{xpath: '/copyLibrary'}], qualifier: [{nearest_ancestor: 'copyStatement'}, {xpath: '/COPY'}]}}
  - {data_key: literal, graph_label: Include, source: cobol-preprocessor, pattern: '//copyStatement/copySource', type: entity, captures: {name: [{xpath: '/literal'}]}}
antlr_contexts: [{pattern: '//programUnit', label: Program}]
"#,
    );
    let source = program(
        "COPY \"book.cpy\" OF \"lib\" SUPPRESS.\nCOPY BOOK IN LIB.\nCOPY BOOK REPLACING ==OLD== BY ==NEW==.",
    );
    with_identity(&source, &context(), |input| {
        let result = parse_cobol_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
        assert!(
            result.preprocessor.syntax.diagnostics.is_empty(),
            "{:?}",
            result.preprocessor.syntax.diagnostics
        );
        assert_eq!(result.preprocessor.occurrences.len(), 6);
        assert!(
            result.main.syntax.diagnostics.is_empty(),
            "{:?}",
            result.main.syntax.diagnostics
        );
        for index in 0..6 {
            assert_linked(&result, index);
        }
        let raw: Vec<_> = result
            .preprocessor
            .occurrences
            .iter()
            .filter(|o| o.occurrence.query_index == 0)
            .map(|o| &o.occurrence)
            .collect();
        assert_eq!(
            raw.iter().map(|o| text(&o.name)).collect::<Vec<_>>(),
            ["\"book.cpy\" OF \"lib\"", "BOOK IN LIB", "BOOK"]
        );
        assert_eq!(text(&raw[0].value), "\"lib\"");
        assert_eq!(text(&raw[1].value), "LIB");
        assert!(matches!(raw[2].value, CaptureBinding::Missing));
        let literal: Vec<_> = result
            .preprocessor
            .occurrences
            .iter()
            .filter(|o| o.occurrence.query_index == 1)
            .collect();
        assert_eq!(text(&literal[0].occurrence.name), "\"book.cpy\"");
        assert!(matches!(
            literal[1].occurrence.name,
            CaptureBinding::Missing
        ));
        for (a, b) in raw.iter().zip(literal) {
            assert_eq!(a.matched.node_index, b.occurrence.matched.node_index);
        }
    });
}

#[test]
fn strings_comments_and_exec_sql_do_not_become_copy_observations() {
    let catalog = profile("antlr-cobol85", COPY);
    for source in [program(""), program("01 MSG PIC X(16) VALUE \"COPY BOOK.\"."), "IDENTIFICATION DIVISION. PROGRAM-ID. HELLO. PROCEDURE DIVISION. DISPLAY \"COPY BOOK.\". EXEC SQL COPY BOOK. END-EXEC. STOP RUN.".into()] {
        with_identity(&source, &context(), |input| {
            let result = parse_cobol_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
            assert!(result.preprocessor.syntax.diagnostics.is_empty(), "{:?}", result.preprocessor.syntax.diagnostics);
            assert!(result.preprocessor.occurrences.is_empty());
        });
    }
    let physical = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HELLO.\n      * COPY HIDDEN.\n       PROCEDURE DIVISION.\n           STOP RUN.\n";
    let operation = context();
    let prepared = prepare_cobol(physical, CobolFormat::Fixed, &operation).unwrap();
    prepared.with_origin_plan(|plan| {
        let input = PreparedInput::new(
            physical.as_bytes(),
            prepared.text().as_bytes(),
            plan,
            &operation,
        )
        .unwrap();
        assert!(
            parse_cobol_semantic_occurrences(&catalog.profiles()[0], &input)
                .unwrap()
                .preprocessor
                .occurrences
                .is_empty()
        );
    });
}

#[test]
fn physical_formats_and_continued_utf8_keep_original_projection() {
    let catalog = profile("antlr-cobol85", COPY);
    for (format, physical) in [
        (
            CobolFormat::Fixed,
            "\u{feff}       IDENTIFICATION DIVISION.\r\n       PROGRAM-ID. HELLO.\r\n       DATA DIVISION.\r\n       WORKING-STORAGE SECTION.\r\n           COPY \"café\r\n      -    \"book\".\r\n       PROCEDURE DIVISION.\r\n           STOP RUN.\r\n",
        ),
        (
            CobolFormat::Variable,
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HELLO.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n           COPY \"cafébook\".\n       PROCEDURE DIVISION.\n           STOP RUN.\n",
        ),
        (
            CobolFormat::Tandem,
            " IDENTIFICATION DIVISION.\n PROGRAM-ID. HELLO.\n DATA DIVISION.\n WORKING-STORAGE SECTION.\n COPY \"cafébook\".\n PROCEDURE DIVISION.\n STOP RUN.\n",
        ),
    ] {
        let operation = context();
        let prepared = prepare_cobol(physical, format, &operation).unwrap();
        prepared.with_origin_plan(|plan| {
            let input = PreparedInput::new(
                physical.as_bytes(),
                prepared.text().as_bytes(),
                plan,
                &operation,
            )
            .unwrap();
            let result = parse_cobol_semantic_occurrences(&catalog.profiles()[0], &input).unwrap();
            assert!(
                result.preprocessor.syntax.diagnostics.is_empty(),
                "{format:?} {:?}",
                result.preprocessor.syntax.diagnostics
            );
            assert_eq!(result.preprocessor.occurrences.len(), 1);
            let item = &result.preprocessor.occurrences[0].occurrence;
            assert_eq!(text(&item.name), "\"cafébook\"");
            if format == CobolFormat::Fixed {
                assert!(matches!(
                    present(&item.name).original,
                    Some(OriginalProjection::Compound(_))
                ));
            }
            assert_linked(&result, 0);
        });
    }
}

#[test]
fn equal_named_and_nested_programs_link_distinct_native_parents() {
    let catalog = profile("antlr-cobol85", COPY);
    let multi = format!("{}\n{}", program("COPY FIRST."), program("COPY SECOND."));
    let nested = format!(
        "{HELLO}COPY OUTER.\nPROCEDURE DIVISION.\nDISPLAY 'OUTER'.\n{}\nEND PROGRAM HELLO.",
        program("COPY INNER.")
    );
    for source in [multi, nested] {
        with_identity(&source, &context(), |input| {
            let result = parse_cobol_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
            assert_eq!(result.preprocessor.occurrences.len(), 2);
            assert_ne!(assert_linked(&result, 0), assert_linked(&result, 1));
        });
    }
    with_identity("COPY OUTSIDE.", &context(), |input| {
        let result = parse_cobol_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
        assert_eq!(result.preprocessor.occurrences.len(), 1);
        assert!(matches!(
            result.preprocessor.occurrences[0].scope,
            CopyProgramScope::NoAnchor | CopyProgramScope::OutsideProgram
        ));
    });
}

#[test]
fn explicit_source_and_invalid_selectors_preserve_causal_query_origin() {
    let normal = profile("antlr-cobol85", COPY);
    let omitted = profile(
        "antlr-cobol85",
        &COPY.replace(", source: cobol-preprocessor", ""),
    );
    assert_eq!(
        *omitted.profiles()[0].queries().value().as_slice()[0]
            .value()
            .source()
            .value(),
        QuerySource::Primary
    );
    assert!(matches!(
        omitted.profiles()[0].queries().value().as_slice()[0]
            .value()
            .source()
            .provenance(),
        ProvenanceView::Default
    ));
    with_identity(&program("COPY BOOK."), &context(), |input| {
        assert!(matches!(
            parse_semantic_occurrences(&normal.profiles()[0], input),
            Err(SemanticError::UnsupportedQuerySource { query_index: 0, .. })
        ));
        assert!(matches!(
            parse_cobol_semantic_occurrences(&omitted.profiles()[0], input),
            Err(SemanticError::Engine {
                query_origin: Some(SemanticQueryOrigin::Profile { query_index: 0 }),
                cause: AntlrEngineError::QueryFailure { .. }
            })
        ));
        for pattern in ["//unknownRule", "//copyStatement/copySource"] {
            let body = format!(
                "queries:\n  - {{data_key: first, graph_label: Main, pattern: '//programUnit', type: entity}}\n  - {{data_key: second, graph_label: Include, source: cobol-preprocessor, pattern: '{pattern}', captures: {{name: [{{xpath: '/unknownCapture'}}]}}, type: entity}}\n"
            );
            let catalog = profile("antlr-cobol85", &body);
            let error = parse_cobol_semantic_occurrences(&catalog.profiles()[0], input)
                .err()
                .unwrap();
            assert!(error.source().is_some());
            assert!(matches!(
                error,
                SemanticError::Engine {
                    query_origin: Some(SemanticQueryOrigin::Profile { query_index: 1 }),
                    cause: AntlrEngineError::QueryFailure { .. }
                }
            ));
        }
        for grammar in ["antlr-plsql", "antlr-tsql", "tree-sitter-json"] {
            let catalog = profile(grammar, COPY);
            assert!(matches!(
                parse_cobol_semantic_occurrences(&catalog.profiles()[0], input),
                Err(SemanticError::UnsupportedQuerySource { query_index: 0, .. })
            ));
        }
    });
}

#[test]
fn original_cancellation_and_expiration_return_no_partial_document() {
    let catalog = profile("antlr-cobol85", COPY);
    let source = program("COPY BOOK.");
    let cancellation = Cancellation::default();
    let operation = SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        cancellation.clone(),
    );
    with_identity(&source, &operation, |input| {
        cancellation.cancel();
        assert!(
            matches!(parse_cobol_semantic_occurrences(&catalog.profiles()[0], input), Err(SemanticError::Operation(e)) if e.kind() == ErrorKind::Interrupted)
        );
    });
    let operation = SnapshotContext::new(
        Instant::now() + Duration::from_millis(100),
        Cancellation::default(),
    );
    with_identity(&source, &operation, |input| {
        std::thread::sleep(Duration::from_millis(120));
        assert!(
            matches!(parse_cobol_semantic_occurrences(&catalog.profiles()[0], input), Err(SemanticError::Operation(e)) if e.kind() == ErrorKind::TimedOut)
        );
    });
}

#[test]
fn unknown_source_is_rejected_at_its_original_field_span() {
    let operation = context();
    let id = ProfileId::new("bad-source", 128).unwrap();
    let decoder = ProfileDecoder::new(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })
        .unwrap(),
    );
    for scalar in ["other-parser", "[primary]", "null"] {
        let bytes = format!(
            "language: cobol\nqueries: [{{data_key: x, graph_label: X, pattern: '//programUnit', source: {scalar}, type: entity}}]\n"
        );
        let error = decoder
            .decode(
                ProfileInput {
                    id: &id,
                    bytes: bytes.as_bytes(),
                },
                &ProfileLimits::new(ProfileLimitsInput {
                    input_bytes: 1 << 20,
                    nodes: 32768,
                    depth: 64,
                    text_bytes: 1 << 20,
                    scalar_bytes: 32768,
                    collection_entries: 4096,
                })
                .unwrap(),
                &operation,
            )
            .err()
            .unwrap();
        assert_eq!(error.schema_path(), Some("queries.source"));
        let span = error.span().unwrap();
        assert_eq!(span.start(), bytes.find(scalar).unwrap());
        // Container sites point to their opening token; scalar sites cover the value.
        if !scalar.starts_with('[') {
            assert!(span.end() >= span.start() + scalar.len());
        }
    }
    let bytes = "language: cobol\nqueries: [{data_key: x, graph_label: X, pattern: '//programUnit', source: primary, type: entity}]\n";
    let decoded = decoder
        .decode(
            ProfileInput {
                id: &id,
                bytes: bytes.as_bytes(),
            },
            &ProfileLimits::new(ProfileLimitsInput {
                input_bytes: 1 << 20,
                nodes: 32768,
                depth: 64,
                text_bytes: 1 << 20,
                scalar_bytes: 32768,
                collection_entries: 4096,
            })
            .unwrap(),
            &operation,
        )
        .unwrap();
    let Presence::Set(queries) = decoded.patch().queries() else {
        panic!("queries")
    };
    let Presence::Set(source) = queries.value().as_slice()[0].value().source() else {
        panic!("source")
    };
    assert_eq!(*source.value(), QuerySource::Primary);
}

#[test]
fn main_copy_directives_have_real_parents_and_strict_structural_errors() {
    use graphit_ast::antlr_engine::{Dialect, parse_strict};
    let operation = context();
    let positives = [
        program("COPY BOOK."),
        program("COPY \"book.cpy\" OF \"lib\" SUPPRESS."),
        program("COPY BOOK IN LIB."),
        program("COPY book.cpy."),
        program("COPY 12.CPY."),
        program("COPY BOOK.12."),
        program("COPY BOOK REPLACING ==OLD== BY ==NEW==."),
        program("COPY BOOK REPLACING ==== BY ==A = B==."),
        program("COPY BOOK REPLACING ==A = == BY ==B==."),
        format!("{HELLO}01 F PIC 9.\nPROCEDURE DIVISION.\nCOPY ACTIONS.\nDISPLAY F.\nP1.\nCOPY PARAGRAPH-CONTENT.\nSTOP RUN.\nEND PROGRAM HELLO."),
        "IDENTIFICATION DIVISION. PROGRAM-ID. HELLO. DATA DIVISION. FILE SECTION. COPY FILE-ENTRIES. WORKING-STORAGE SECTION. COPY DATA-ENTRIES. PROCEDURE DIVISION. STOP RUN. END PROGRAM HELLO.".into(),
    ];
    for source in positives {
        let syntax = parse_strict(Dialect::Cobol85, &source, &operation).unwrap();
        let copy: Vec<_> = syntax
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == "COPY")
            .collect();
        assert!(!copy.is_empty());
        for (_, node) in copy {
            assert_eq!(node.class, NodeClass::Terminal);
            let mut parent = node.parent;
            while let Some(index) = parent {
                if syntax.nodes[index].kind == "programUnit" {
                    break;
                }
                parent = syntax.nodes[index].parent;
            }
            assert!(parent.is_some());
        }
    }
    for directive in [
        "COPY .",
        "COPY BOOK",
        "COPY BOOK OF .",
        "COPY BOOK REPLACING .",
        "COPY BOOK REPLACING ==OLD== ==NEW==.",
        "COPY BOOK REPLACING ==OLD== BY .",
        "COPY BOOK REPLACING ==OLD BY ==NEW==.",
        "COPY BOOK REPLACING ==OLD== BY ==NEW.",
        "COPY -1.",
        "COPY +1.",
        "COPY -12.CPY.",
        "COPY BOOK.-12.",
        "COPY N\"BOOK\".",
        "COPY BOOK REPLACING = =OLD= = BY ==NEW==.",
    ] {
        let source = program(directive);
        assert!(
            matches!(
                parse_strict(Dialect::Cobol85, &source, &operation),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{directive}"
        );
    }
}

#[test]
fn actual_json_consumer_checks_query_source_before_sdk_compilation() {
    use graphit_ast::tree_sitter_engine::GrammarLanguage;
    use graphit_ast::tree_sitter_profile::{PreparedTreeSitterProfile, TreeSitterProfileError};
    let operation = context();
    for source in ["", "source: primary,"] {
        let catalog = catalog(format!("language: json\ngrammar: tree-sitter-json\nqueries: [{{{source} data_key: value, graph_label: Value, pattern: '(document) @name', type: entity}}]\n").as_bytes());
        let language = GrammarLanguage::from_language(tree_sitter_json::LANGUAGE.into()).unwrap();
        assert!(
            PreparedTreeSitterProfile::prepare(&catalog.profiles()[0], language, &operation)
                .is_ok()
        );
    }
    for prefix in [
        "",
        "  - {data_key: bad-native, graph_label: Value, pattern: '(unregistered_native_rule) @name', type: entity}\n",
    ] {
        let catalog = catalog(format!("language: json\ngrammar: tree-sitter-json\nqueries:\n{prefix}  - {{source: cobol-preprocessor, data_key: value, graph_label: Value, pattern: '(document) @name', type: entity}}\n").as_bytes());
        let language = GrammarLanguage::from_language(tree_sitter_json::LANGUAGE.into()).unwrap();
        let expected = usize::from(!prefix.is_empty());
        assert!(
            matches!(PreparedTreeSitterProfile::prepare(&catalog.profiles()[0], language, &operation), Err(TreeSitterProfileError::UnsupportedQuerySource {query_index, source:QuerySource::CobolPreprocessor}) if query_index == expected)
        );
    }
}
