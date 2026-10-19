use graphit_ast::{
    antlr_engine::{
        self,
        semantic::{
            CaptureBinding as HostBinding,
            postgresql::{PostgreSqlSemanticError, parse_postgresql_semantic_occurrences},
        },
    },
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{
        ByteSpan, OriginPlan, OriginSegment, OriginalProjection, PreparedInput, SyntheticReason,
    },
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    tree_sitter_engine::{EngineError, GrammarLanguage, ParseControl},
    tree_sitter_profile::{CaptureBinding, PreparedTreeSitterProfile, TreeSitterProfileError},
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{error::Error, sync::atomic::AtomicBool, time::Instant};
type TestResult = Result<(), Box<dyn Error>>;
fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}

fn catalog(bytes: &[u8]) -> CatalogDraft {
    let original = context();
    let id = ProfileId::new("tree-profile-fixture", 256).unwrap();
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
            &original,
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
        &original,
    )
    .unwrap()
}

fn identity(
    text: &str,
    original: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    let bound = ByteSpan {
        start: 0,
        end: u64::try_from(text.len())?,
    };
    let segments = if text.is_empty() {
        vec![]
    } else {
        vec![OriginSegment::Direct {
            prepared: bound,
            original: bound,
        }]
    };
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        original,
    )?;
    action(&input)
}

fn plpgsql_fixture() -> graphit_ast::tree_sitter_engine::GrammarLanguage {
    graphit_ast::tree_sitter_engine::builtin("tree-sitter-plpgsql").unwrap()
}

const HOST_PROFILE: &[u8] = br#"language: postgresql
parser: antlr4
grammar: antlr-postgresql
queries:
  - {data_key: functions, graph_label: Function, pattern: '//createfunctionstmt', keyword: 'FUNCTION', select: '/func_name', type: entity}
  - {data_key: procedures, graph_label: Procedure, pattern: '//createfunctionstmt', keyword: 'PROCEDURE', select: '/func_name', type: entity}
antlr_contexts:
  - {pattern: '//createfunctionstmt', keyword: FUNCTION, label: Function, name: [{xpath: '/func_name'}]}
  - {pattern: '//createfunctionstmt', keyword: PROCEDURE, label: Procedure, name: [{xpath: '/func_name'}]}
"#;

#[test]
fn stock_body_captures_decisions_and_homonymous_scopes_keep_native_owners() -> TestResult {
    let host_catalog = catalog(include_bytes!("fixtures/catalog/shipped/postgresql.yaml"));
    let body_catalog = catalog(include_bytes!("fixtures/catalog/shipped/plpgsql.yaml"));
    let language = plpgsql_fixture();
    let original = context();
    let factory = PreparedTreeSitterProfile::prepare(
        &body_catalog.profiles()[0],
        language.clone(),
        &original,
    )?;
    let source = "-- Ω\r\nCREATE FUNCTION same() RETURNS void LANGUAGE plpgsql AS $$DECLARE v text := 'á'; BEGIN IF true THEN NULL; END IF; END;$$;\r\nCREATE PROCEDURE same() LANGUAGE plpgsql AS $MiXeD$DECLARE v text := 'β'; BEGIN IF true THEN NULL; END IF; END;$MiXeD$;";
    identity(source, &original, |input| {
        let result = parse_postgresql_semantic_occurrences(
            &host_catalog.profiles()[0],
            &factory,
            input,
            ParseControl::default(),
        )?;
        drop(factory);
        drop(language);
        assert_eq!(result.host.contexts.len(), 2);
        assert_eq!(result.bodies.len(), 2);
        assert_eq!(result.host.occurrences.len(), 2);
        for (index, body) in result.bodies.iter().enumerate() {
            assert_eq!(body.host_contexts.len(), 1);
            let scope = &result.host.contexts[body.host_contexts[0]];
            assert_eq!(
                scope.label,
                Some(if index == 0 { "Function" } else { "Procedure" })
            );
            let HostBinding::Present(name) = &scope.name else {
                panic!("native routine name")
            };
            assert_eq!(name.prepared.as_ref().unwrap().text(), "same");
            assert_eq!(body.observations.groups.len(), 1);
            let group = &body.observations.groups[0];
            assert!(std::ptr::eq(
                group.query,
                &body_catalog.profiles()[0].queries().value().as_slice()[0]
            ));
            assert_eq!(group.query.value().graph_label().value(), "Variable");
            assert_eq!(group.matches.len(), 1);
            let found = &group.matches[0];
            let CaptureBinding::Present(slots) = &found.bindings.name else {
                panic!("variable")
            };
            assert_eq!(slots.len(), 1);
            let capture = &found.captures[slots[0]];
            assert_eq!(capture.prepared.text(), "v");
            assert!(matches!(capture.original, OriginalProjection::Exact(_)));
            let address = &body.matches[0];
            assert_eq!((address.query_index, address.match_index), (0, 0));
            let mut node_index = address.capture_nodes[slots[0]];
            assert_eq!(
                body.observations.nodes[node_index].node_id,
                capture.native.node_id
            );
            while let Some(parent) = body.observations.nodes[node_index].parent {
                node_index = parent;
            }
            assert_eq!(node_index, 0);
            assert_eq!(body.decisions.len(), 1);
            assert_eq!(
                body.observations.nodes[body.decisions[0].node_index].kind,
                "stmt_if"
            );
            assert!(body.decisions[0].prepared.text().contains("IF true"));
            let retained = body
                .observations
                .body
                .tree()
                .nodes_with_context(ParseControl::default(), &original)?;
            assert_eq!(retained.len(), body.observations.nodes.len());
            assert_eq!(
                retained[address.capture_nodes[slots[0]]].node_id,
                capture.native.node_id
            );
        }
        Ok(())
    })?;
    Ok(())
}

#[test]
fn only_nonempty_dollar_plpgsql_bodies_are_composed() -> TestResult {
    let host_catalog = catalog(HOST_PROFILE);
    let body_catalog = catalog(include_bytes!("fixtures/catalog/shipped/plpgsql.yaml"));
    let language = plpgsql_fixture();
    let original = context();
    let factory =
        PreparedTreeSitterProfile::prepare(&body_catalog.profiles()[0], language, &original)?;
    for source in [
        "CREATE FUNCTION f() RETURNS void LANGUAGE sql AS $$SELECT 1;$$;",
        "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS 'BEGIN RETURN; END;';",
        "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS E'BEGIN RETURN; END;';",
        "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS U&'BEGIN RETURN; END;';",
        "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$$$;",
        "SELECT $MiXeD$BEGIN RETURN; END;$MiXeD$;",
    ] {
        identity(source, &original, |input| {
            let result = parse_postgresql_semantic_occurrences(
                &host_catalog.profiles()[0],
                &factory,
                input,
                ParseControl::default(),
            )?;
            assert!(result.bodies.is_empty(), "{source}");
            Ok(())
        })?;
    }
    identity(
        "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$BEGIN RETURN; END;$$ SET search_path TO $x$public$x$;",
        &original,
        |input| {
            let result = parse_postgresql_semantic_occurrences(
                &host_catalog.profiles()[0],
                &factory,
                input,
                ParseControl::default(),
            )?;
            assert_eq!(result.bodies.len(), 1);
            assert!(
                result.bodies[0]
                    .observations
                    .body
                    .tree()
                    .source()
                    .starts_with("BEGIN")
            );
            Ok(())
        },
    )
}

#[test]
fn origins_remain_distinct_from_prepared_host_coordinates() -> TestResult {
    let host_catalog = catalog(HOST_PROFILE);
    let body_catalog = catalog(include_bytes!("fixtures/catalog/shipped/plpgsql.yaml"));
    let language = plpgsql_fixture();
    let original = context();
    let factory =
        PreparedTreeSitterProfile::prepare(&body_catalog.profiles()[0], language, &original)?;
    let prepared = "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$DECLARE v int; BEGIN IF true THEN NULL; END IF; END;$$;";
    let bound = ByteSpan {
        start: 0,
        end: prepared.len() as u64,
    };
    for segment in [
        OriginSegment::Direct {
            prepared: bound,
            original: ByteSpan {
                start: 4,
                end: 4 + prepared.len() as u64,
            },
        },
        OriginSegment::Synthetic {
            prepared: bound,
            reason: SyntheticReason::Wrapper,
        },
    ] {
        let source = format!("xxxx{prepared}");
        let segments = [segment];
        let removed = [ByteSpan {
            start: 0,
            end: if matches!(segment, OriginSegment::Synthetic { .. }) {
                source.len() as u64
            } else {
                4
            },
        }];
        let input = PreparedInput::new(
            source.as_bytes(),
            prepared.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &removed,
                anchors: &[],
            },
            &original,
        )?;
        let result = parse_postgresql_semantic_occurrences(
            &host_catalog.profiles()[0],
            &factory,
            &input,
            ParseControl::default(),
        )?;
        let body = &result.bodies[0];
        let capture = &body.observations.groups[0].matches[0].captures[0];
        assert_eq!(capture.prepared.text(), "v");
        match (&segments[0], &capture.original) {
            (OriginSegment::Direct { .. }, OriginalProjection::Exact(span)) => {
                assert_eq!(span.text(), "v");
                assert_eq!(span.start(), capture.prepared.start() + 4);
            }
            (
                OriginSegment::Synthetic { .. },
                OriginalProjection::Synthetic(SyntheticReason::Wrapper),
            ) => {}
            _ => panic!("projection retained"),
        }
        assert_eq!(body.decisions.len(), 1);
    }
    Ok(())
}

#[test]
fn controls_binding_and_native_generation_errors_preserve_causes() -> TestResult {
    let host_catalog = catalog(HOST_PROFILE);
    let body_catalog = catalog(include_bytes!("fixtures/catalog/shipped/plpgsql.yaml"));
    let language = plpgsql_fixture();
    let original = context();
    let factory =
        PreparedTreeSitterProfile::prepare(&body_catalog.profiles()[0], language, &original)?;
    let source = "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$DECLARE v int; BEGIN RETURN; END;$$;";
    identity(source, &original, |input| {
        let cancelled = AtomicBool::new(true);
        let error = parse_postgresql_semantic_occurrences(
            &host_catalog.profiles()[0],
            &factory,
            input,
            ParseControl {
                cancellation: Some(&cancelled),
                deadline: None,
            },
        )
        .err()
        .unwrap();
        assert!(matches!(
            error,
            PostgreSqlSemanticError::Body(TreeSitterProfileError::Engine(EngineError::Cancelled))
        ));
        let expired = ParseControl {
            cancellation: None,
            deadline: Some(Instant::now()),
        };
        assert!(matches!(
            parse_postgresql_semantic_occurrences(
                &host_catalog.profiles()[0],
                &factory,
                input,
                expired
            ),
            Err(PostgreSqlSemanticError::Body(
                TreeSitterProfileError::Engine(EngineError::TimedOut)
            ))
        ));
        let result = parse_postgresql_semantic_occurrences(
            &host_catalog.profiles()[0],
            &factory,
            input,
            ParseControl::default(),
        )?;
        let foreign = GrammarLanguage::from_language(tree_sitter_json::LANGUAGE.into())?;
        let query = foreign.compile_query("(object) @name")?;
        assert!(matches!(
            result.bodies[0].observations.body.matches_with_context(
                &query,
                ParseControl::default(),
                &original
            ),
            Err(antlr_engine::EmbeddedError::TreeSitter(
                EngineError::GrammarMismatch
            ))
        ));
        let other_catalog = catalog(
            b"language: json\nqueries:\n  - {data_key: objects, pattern: '(object) @name', graph_label: Object}\n",
        );
        let other_factory =
            PreparedTreeSitterProfile::prepare(&other_catalog.profiles()[0], foreign, &original)?;
        assert!(matches!(
            other_factory.collect_plpgsql_bodies(
                &result.host.syntax,
                input,
                ParseControl::default()
            ),
            Err(TreeSitterProfileError::EmbeddedProfileMismatch)
        ));
        let copied = String::from(source);
        let alien_host =
            antlr_engine::parse(antlr_engine::Dialect::PostgreSql, &copied, &original)?;
        assert!(matches!(
            factory.collect_plpgsql_bodies(&alien_host, input, ParseControl::default()),
            Err(TreeSitterProfileError::Origin(_))
        ));
        original.cancellation().cancel();
        let error = parse_postgresql_semantic_occurrences(
            &host_catalog.profiles()[0],
            &factory,
            input,
            expired,
        )
        .err()
        .unwrap();
        assert!(
            matches!(error, PostgreSqlSemanticError::Host(graphit_ast::antlr_engine::semantic::SemanticError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
        );
        Ok(())
    })
}

#[test]
fn body_query_patterns_properties_duplicates_and_missing_bindings_are_observable() -> TestResult {
    let host_catalog = catalog(HOST_PROFILE);
    let body_catalog = catalog(br#"language: plpgsql
queries:
  - data_key: variable_or_decision
    graph_label: Variable
    pattern: '((decl_stmt (decl_statement (decl_varname) @name)) (#set! origin "body")) (stmt_if) @decision'
    name_capture: name
    value_capture: payload
    value_label: Payload
  - data_key: repeated_variable
    graph_label: Variable
    pattern: '(decl_stmt (decl_statement (decl_varname) @name))'
    name_capture: name
"#);
    let language = plpgsql_fixture();
    let original = context();
    let factory =
        PreparedTreeSitterProfile::prepare(&body_catalog.profiles()[0], language, &original)?;
    identity(
        "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$DECLARE v int; BEGIN IF true THEN NULL; END IF; END;$$;",
        &original,
        |input| {
            let result = parse_postgresql_semantic_occurrences(
                &host_catalog.profiles()[0],
                &factory,
                input,
                ParseControl::default(),
            )?;
            let body = &result.bodies[0];
            assert_eq!(body.observations.groups.len(), 2);
            let first = &body.observations.groups[0];
            assert_eq!(first.matches.len(), 2);
            assert!(first.matches.iter().any(|found| matches!(
                found.bindings.name,
                CaptureBinding::Missing { declared: true }
            )));
            assert!(first.matches.iter().all(|found| matches!(
                found.bindings.value,
                CaptureBinding::Missing { declared: false }
            )));
            assert!(
                first
                    .matches
                    .iter()
                    .all(|found| matches!(found.bindings.span, CaptureBinding::NotConfigured))
            );
            assert!(
                first
                    .matches
                    .iter()
                    .any(|found| !found.properties.is_empty())
            );
            assert_eq!(body.observations.groups[1].matches.len(), 1);
            assert_eq!(body.matches.len(), 3);
            assert_eq!(
                body.matches
                    .iter()
                    .map(|found| found.query_index)
                    .collect::<Vec<_>>(),
                [0, 0, 1]
            );
            for (index, group) in body.observations.groups.iter().enumerate() {
                assert!(std::ptr::eq(
                    group.query,
                    &body_catalog.profiles()[0].queries().value().as_slice()[index]
                ));
            }
            Ok(())
        },
    )
}
