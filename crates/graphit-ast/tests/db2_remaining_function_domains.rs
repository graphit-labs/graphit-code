use graphit_ast::antlr_engine::{
    AntlrEngineError, Dialect, NodeClass, ProfileQuery, SpanStatus, parse, parse_profile_queries,
    parse_strict,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};

fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}

fn check(body: &str, pattern: &str, expected: &[&str]) {
    let source = format!("-- α\r\n/* β /* γ */ δ */ {body}");
    let parsed = parse_profile_queries(
        Dialect::Db2,
        &source,
        &[ProfileQuery {
            pattern: pattern.into(),
            captures: vec![Vec::new()],
            ..ProfileQuery::default()
        }],
        &context(),
    )
    .unwrap();
    let syntax = &parsed.syntax;
    assert!(
        syntax.diagnostics.is_empty(),
        "{body}: {:?}",
        syntax.diagnostics
    );
    assert_eq!(
        parsed
            .matches
            .iter()
            .map(|matched| {
                assert_eq!(matched.captures, [Some(matched.node_index)]);
                let node = &syntax.nodes[matched.node_index];
                assert_eq!(node.class, NodeClass::Rule);
                assert_eq!(node.status, SpanStatus::Regular);
                source.get(node.span.clone().unwrap()).unwrap()
            })
            .collect::<Vec<_>>(),
        expected,
        "{body}: {pattern}"
    );
    assert!(
        parsed
            .matches
            .windows(2)
            .all(|pair| pair[0].node_index < pair[1].node_index)
    );
    for (index, node) in syntax.nodes.iter().enumerate() {
        assert!(!node.error && !node.missing, "{body}: {}", node.kind);
        assert!(node.parent.is_none_or(|parent| parent < index));
        if let Some(span) = &node.span {
            assert!(source.get(span.clone()).is_some());
        }
    }
    for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
        let token = syntax.tokens.iter().find(|t| t.kind == kind).unwrap();
        assert_ne!(token.channel, 0);
        assert_eq!(
            source.get(token.span.clone().unwrap()),
            token.text.as_deref()
        );
    }
    assert!(syntax.tokens.iter().any(|t| t.eof));
    for token in &syntax.tokens {
        if token.eof {
            assert_eq!(token.status, SpanStatus::Eof);
            assert_eq!(token.span, Some(source.len()..source.len()));
        } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
            assert_eq!(source.get(span.clone()), Some(text.as_str()));
        }
    }
}

#[test]
fn cast_source_has_its_own_domain_without_attribute_inference() {
    for data_type in [
        "DATE",
        "DECIMAL(12,2)",
        "CLOB(2M)",
        "APP.TYPE_F",
        "REF(\"Schéma\".\"Typé\")",
    ] {
        check(
            &format!(
                "CREATE FUNCTION F_CAST() RETURNS VARCHAR() CAST FROM {data_type} LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.fn';"
            ),
            "//create_function_external_scalar_statement/external_scalar_cast_source_type",
            &[data_type],
        );
    }
}

#[test]
fn external_table_parameter_and_column_domains_keep_order() {
    let body = "CREATE FUNCTION F_TABLE(P_REF REF(APP.TYPE_F),XML AS CLOB(2M),P_LOB CLOB(2M) DEFAULT NULL AS LOCATOR) RETURNS TABLE(C_DATE DATE,C_LOB CLOB(2M) AS LOCATOR,C_TYPE APP.TYPE_F,C_REF REF(APP.TYPE_F),C_XML XML AS CLOB(2048)) LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.fn' NO SQL;";
    check(
        body,
        "//ext_table_param_decl/method_reference_type",
        &["REF(APP.TYPE_F)"],
    );
    check(
        body,
        "//ext_table_param_decl/procedure_external_xml_type",
        &["XML AS CLOB(2M)"],
    );
    check(
        body,
        "//ext_table_param_decl/parameter_name",
        &["P_REF", "P_LOB"],
    );
    check(
        body,
        "//create_function_external_table_statement/external_table_return_type",
        &[
            "DATE",
            "CLOB(2M)",
            "APP.TYPE_F",
            "REF(APP.TYPE_F)",
            "XML AS CLOB(2048)",
        ],
    );
    check(
        "CREATE FUNCTION F_GENERIC() RETURNS GENERIC TABLE LANGUAGE JAVA PARAMETER STYLE DB2GENERAL EXTERNAL NAME 'pkg.fn';",
        "//create_function_external_table_statement/external_table_return_type",
        &[],
    );
}

#[test]
fn oledb_columns_and_diagram_reference_keep_separate_contexts() {
    check(
        "CREATE FUNCTION F_OLE(P_CMD VARCHAR(40) DEFAULT 'cmd',GRAPHIC(10)) RETURNS TABLE(C_DATE DATE,C_DEC DECIMAL(12,2),C_LOB CLOB(2M),C_TYPE APP.TYPE_F) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
        "//create_function_old_db_external_function_statement/oledb_return_type",
        &["DATE", "DECIMAL(12,2)", "CLOB(2M)", "APP.TYPE_F"],
    );
    // The diagram admits REF. This is syntax coverage, not provider/catalog legality.
    check(
        "CREATE FUNCTION F_OLE(P_REF REF(APP.TYPE_F)) RETURNS TABLE(C_REF REF(APP.TYPE_F)) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
        "//param_decl_3/method_reference_type",
        &["REF(APP.TYPE_F)"],
    );
}

#[test]
fn sourced_and_template_inference_follow_their_positions() {
    for tail in [
        "RETURNS VARCHAR() SOURCE APP.F_TARGET",
        "RETURNS VARCHAR(40) AS TEMPLATE",
    ] {
        check(
            &format!(
                "CREATE FUNCTION F_SOURCE(P_CHAR CHAR(),DECIMAL() DEFAULT 1,P_TS TIMESTAMP()) {tail};"
            ),
            "//sourced_function_param_decl/sourced_function_parameter_type",
            &["CHAR()", "DECIMAL()", "TIMESTAMP()"],
        );
    }
    for tail in [
        "RETURNS VARCHAR() SOURCE APP.F_TARGET",
        "RETURNS VARCHAR() SPECIFIC APP.F_SPEC SOURCE SPECIFIC APP.F_TARGET PARAMETER CCSID UNICODE",
        "RETURNS VARCHAR() SOURCE APP.F_TARGET()",
        "RETURNS VARCHAR() SOURCE APP.F_TARGET(DECIMAL(),VARCHAR(),TIMESTAMP())",
    ] {
        check(
            &format!("CREATE FUNCTION F_SOURCE() {tail};"),
            "//fn_return_opts_item/sourced_function_return_type",
            &["VARCHAR()"],
        );
    }
    check(
        "CREATE FUNCTION F_TEMPLATE() RETURNS VARCHAR(40) SPECIFIC APP.F_SPEC AS TEMPLATE DETERMINISTIC NO EXTERNAL ACTION;",
        "//fn_return_opts_item/template_function_return_type",
        &["VARCHAR(40)"],
    );
}

#[test]
fn invalid_family_forms_keep_native_diagnostics_and_strict_errors() {
    for body in [
        "CREATE FUNCTION F() RETURNS VARCHAR(20) CAST FROM VARCHAR() LANGUAGE C EXTERNAL NAME 'pkg.fn';",
        "CREATE FUNCTION F() RETURNS INTEGER CAST FROM FLOAT() LANGUAGE C EXTERNAL NAME 'pkg.fn';",
        "CREATE FUNCTION F() RETURNS INTEGER CAST FROM REF() LANGUAGE C EXTERNAL NAME 'pkg.fn';",
        "CREATE FUNCTION F() RETURNS INTEGER CAST FROM XML AS CLOB(2M) LANGUAGE C EXTERNAL NAME 'pkg.fn';",
        "CREATE FUNCTION F(IN P INTEGER) RETURNS TABLE(C DATE) LANGUAGE C EXTERNAL NAME 'pkg.fn';",
        "CREATE FUNCTION F(P XML AS CLOB()) RETURNS TABLE(C DATE) LANGUAGE C EXTERNAL NAME 'pkg.fn';",
        "CREATE FUNCTION F(P VARCHAR(40) AS LOCATOR) RETURNS TABLE(C DATE) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
        "CREATE FUNCTION F(P CHAR()) RETURNS TABLE(C DATE) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
        "CREATE FUNCTION F(P INTEGER DEFAULT) RETURNS INTEGER SOURCE APP.F_TARGET;",
        "CREATE FUNCTION F(P REF(APP.TYPE_F)) RETURNS INTEGER SOURCE APP.F_TARGET;",
        "CREATE FUNCTION F() SOURCE APP.F_TARGET;",
        "CREATE FUNCTION F() RETURNS INTEGER;",
        "CREATE FUNCTION F() RETURNS INTEGER RETURNS INTEGER SOURCE APP.F_TARGET;",
        "CREATE FUNCTION F() RETURNS INTEGER SOURCE APP.F_TARGET AS TEMPLATE;",
        "CREATE FUNCTION F() RETURNS VARCHAR() AS TEMPLATE;",
        "CREATE FUNCTION F() RETURNS INTEGER SOURCE APP.F_TARGET(FLOAT());",
    ] {
        let source = format!("-- α\r\n{body}");
        let syntax = parse(Dialect::Db2, &source, &context()).unwrap();
        assert!(!syntax.diagnostics.is_empty(), "accepted {body}");
        assert!(
            matches!(
                parse_strict(Dialect::Db2, &source, &context()),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {body}"
        );
        for diagnostic in &syntax.diagnostics {
            assert_eq!(diagnostic.grammar, "Db2Parser.g4");
            assert!(!diagnostic.message.is_empty());
            assert!(
                diagnostic
                    .span
                    .as_ref()
                    .is_some_and(|span| source.get(span.clone()).is_some())
            );
        }
        for token in syntax.tokens.iter().filter(|t| !t.eof) {
            if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
    }
}
