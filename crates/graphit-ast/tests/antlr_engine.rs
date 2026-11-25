use graphit_ast::antlr_engine::{AntlrEngineError, CaptureStep, Dialect, ParsedSyntax};
fn operation_context() -> graphit_core::file_io::SnapshotContext {
    graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(60),
        graphit_core::file_lock::Cancellation::default(),
    )
}
fn parse(dialect: Dialect, source: &str) -> Result<ParsedSyntax<'_>, AntlrEngineError> {
    graphit_ast::antlr_engine::parse_strict(dialect, source, &operation_context())
}

#[test]
fn five_real_recognizers_accept_statements_and_reject_broken_syntax() {
    let cases = [
        (Dialect::Db2, "SELECT x FROM t;", "SELECT FROM ;"),
        (Dialect::TSql, "select N'café' as label;", "SELECT FROM ;"),
        (
            Dialect::PostgreSql,
            "select 'café' as label;",
            "SELECT FROM ;",
        ),
        (Dialect::PlSql, "BEGIN NULL; END;", "BEGIN IF THEN END;"),
        (
            Dialect::Cobol85,
            "IDENTIFICATION DIVISION. PROGRAM-ID. HELLO. PROCEDURE DIVISION. STOP RUN.",
            "IDENTIFICATION DIVISION. PROGRAM-ID.",
        ),
    ];
    for (dialect, valid, invalid) in cases {
        let syntax = parse(dialect, valid).unwrap_or_else(|error| panic!("{dialect:?}: {error}"));
        assert!(!syntax.nodes.is_empty(), "{dialect:?}");
        assert!(syntax.tokens.iter().any(|token| token.eof));
        for token in &syntax.tokens {
            if let Some(span) = &token.span {
                assert!(valid.get(span.clone()).is_some());
            }
        }
        assert!(
            parse(dialect, invalid).is_err(),
            "{dialect:?} accepted malformed syntax"
        );
    }
}

#[test]
fn comments_original_case_and_utf8_positions_survive() {
    let source = "-- café\r\nSeLeCt N'Árvore' AS label;";
    let syntax = parse(Dialect::TSql, source).unwrap();
    assert!(syntax.tokens.iter().any(|token| {
        token.channel != 0
            && token
                .text
                .as_deref()
                .is_some_and(|text| text.contains("café"))
    }));
    let token = syntax
        .tokens
        .iter()
        .find(|token| token.text.as_deref() == Some("SeLeCt"))
        .unwrap();
    assert_eq!(&source[token.span.clone().unwrap()], "SeLeCt");
    let literal = syntax
        .tokens
        .iter()
        .find(|token| token.text.as_deref() == Some("N'Árvore'"))
        .unwrap();
    assert_eq!(&source[literal.span.clone().unwrap()], "N'Árvore'");
}

#[test]
fn db2_procedure_contains_control_flow_tree() {
    let source = "CREATE PROCEDURE f (IN x INT) LANGUAGE SQL BEGIN DECLARE y INT; IF x > 0 THEN SET y = x; ELSE SET y = 0; END IF; WHILE x > 0 DO SET x = x - 1; END WHILE; END";
    let syntax = parse(Dialect::Db2, source).unwrap();
    assert!(syntax.nodes.iter().any(|node| node.kind.contains("if")));
    assert!(syntax.nodes.iter().any(|node| node.kind.contains("while")));
}

#[test]
fn db2_designator_types_native() {
    use graphit_ast::antlr_engine::NodeClass;

    for source in [
        "CREATE VARIABLE V DECFLOAT(16) DEFAULT 1;",
        "CREATE VARIABLE V DECFLOAT DEFAULT 1;",
    ] {
        let syntax = parse(Dialect::Db2, source).unwrap();
        let builtin = syntax
            .nodes
            .iter()
            .find(|node| node.kind == "built_in_type")
            .unwrap();
        assert_eq!(builtin.class, NodeClass::Rule);
        assert_eq!(
            &source[builtin.span.clone().unwrap()],
            if source.contains("(16)") {
                "DECFLOAT(16)"
            } else {
                "DECFLOAT"
            }
        );
        assert!(syntax.diagnostics.is_empty());
    }

    let inferred = [
        "DEC()",
        "DECIMAL()",
        "NUMERIC()",
        "NUM()",
        "DECFLOAT()",
        "CHAR()",
        "CHARACTER VARYING()",
        "VARCHAR() FOR BIT DATA",
        "CLOB()",
        "CHAR LARGE OBJECT()",
        "GRAPHIC()",
        "VARGRAPHIC()",
        "DBCLOB()",
        "NCHAR()",
        "NATIONAL CHARACTER()",
        "NVARCHAR()",
        "NCHAR VARYING()",
        "NATIONAL CHAR VARYING()",
        "NCLOB()",
        "NCHAR LARGE OBJECT()",
        "NATIONAL CHARACTER LARGE OBJECT()",
        "BINARY()",
        "VARBINARY()",
        "BINARY VARYING()",
        "BLOB()",
        "BINARY LARGE OBJECT()",
        "TIMESTAMP()",
    ];
    let source = format!("-- café\r\nDROP FUNCTION F({});", inferred.join(","));
    let syntax = parse(Dialect::Db2, &source).unwrap();
    let actual: Vec<_> = syntax
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == "inferred_builtin_type")
        .collect();
    assert_eq!(actual.len(), inferred.len());
    for ((index, node), expected) in actual.iter().zip(inferred) {
        assert_eq!(node.class, NodeClass::Rule);
        assert_eq!(&source[node.span.clone().unwrap()], expected);
        let parent = node.parent.unwrap();
        assert!(parent < *index);
        assert_eq!(syntax.nodes[parent].kind, "designator_data_type");
        let list = syntax.nodes[parent].parent.unwrap();
        assert_eq!(syntax.nodes[list].kind, "designator_data_type_list");
        let paren = syntax.nodes[list].parent.unwrap();
        assert_eq!(syntax.nodes[paren].kind, "designator_data_type_list_paren");
        assert_eq!(
            syntax.nodes[syntax.nodes[paren].parent.unwrap()].kind,
            "function_designator"
        );
    }
    assert!(syntax.diagnostics.is_empty());
    assert!(syntax.nodes.iter().all(|node| !node.error && !node.missing));
    for node in &syntax.nodes {
        if let Some(span) = &node.span {
            assert!(source.get(span.clone()).is_some());
        }
    }

    for (source, designator, expected) in [
        (
            "DROP PROCEDURE PROC_F(VARCHAR());",
            "procedure_designator",
            "VARCHAR()",
        ),
        (
            "DROP METHOD METHOD_F(DECIMAL()) FOR TYPE_F;",
            "method_designator",
            "DECIMAL()",
        ),
    ] {
        let syntax = parse(Dialect::Db2, source).unwrap();
        let node = syntax
            .nodes
            .iter()
            .find(|node| node.kind == "inferred_builtin_type")
            .unwrap();
        assert_eq!(&source[node.span.clone().unwrap()], expected);
        let paren = syntax
            .nodes
            .iter()
            .find(|node| node.kind == "designator_data_type_list_paren")
            .unwrap();
        assert_eq!(syntax.nodes[paren.parent.unwrap()].kind, designator);
    }
    for source in [
        "DROP FUNCTION F(VARCHAR(10),FLOAT(24),INTEGER,DECFLOAT(34));",
        "DROP FUNCTION F();",
        "CREATE TYPE ARRAY_F AS INTEGER ARRAY[INTEGER];",
        "CREATE TYPE ARRAY_F AS INTEGER ARRAY[10];",
    ] {
        let syntax = parse(Dialect::Db2, source).unwrap();
        assert!(syntax.diagnostics.is_empty(), "{source}");
        assert!(
            !syntax
                .nodes
                .iter()
                .any(|node| node.kind == "inferred_builtin_type"),
            "{source}"
        );
    }
    for source in [
        "DROP FUNCTION F(FLOAT());",
        "DROP FUNCTION F(INTEGER());",
        "DROP FUNCTION F(DATE());",
        "DROP FUNCTION F(TIME());",
        "DROP FUNCTION F(BOOLEAN());",
        "DROP FUNCTION F(XML());",
        "DROP FUNCTION F(CURSOR());",
        "DROP FUNCTION F(USER_TYPE());",
        "DROP FUNCTION F(DECIMAL(,2));",
        "DROP FUNCTION F(VARCHAR(,));",
        "CREATE VARIABLE V VARCHAR() DEFAULT 1;",
        "CREATE TYPE ARRAY_F AS INTEGER ARRAY[DOUBLE];",
    ] {
        let recovered =
            graphit_ast::antlr_engine::parse(Dialect::Db2, source, &operation_context()).unwrap();
        assert!(
            !recovered.diagnostics.is_empty(),
            "accepted malformed syntax: {source}"
        );
        assert!(
            matches!(
                parse(Dialect::Db2, source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(matches!(
        graphit_ast::antlr_engine::parse(
            Dialect::Db2,
            "DROP FUNCTION F(VARCHAR());",
            &cancelled
        ),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted
    ));
}

#[test]
fn db2_external_c_keyword_native() {
    use graphit_ast::antlr_engine::NodeClass;

    for (source, header, option) in [
        (
            "CREATE FUNCTION FUNC_C_F(P_C INTEGER) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'lib!entry' NO SQL;",
            "create_function_external_scalar_statement",
            "ext_scalar_option_list_item",
        ),
        (
            "CREATE FUNCTION FUNC_TABLE_C_F(P_TABLE INTEGER) RETURNS TABLE(COLUMN_F INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'lib!table' NO SQL;",
            "create_function_external_table_statement",
            "ext_table_option_list_item",
        ),
        (
            "CREATE PROCEDURE PROC_C_F(IN P_IN INTEGER, OUT P_OUT INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'lib!proc';",
            "create_procedure_external_statement",
            "option_list_2_item",
        ),
        (
            "CREATE TYPE TYPE_C_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_C_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL;",
            "method_specification",
            "external_routine_characteristics",
        ),
        (
            "-- β C C_\r\nCREATE FUNCTION FUNC_C_F(P_C INTEGER) RETURNS INTEGER LANGUAGE c PARAMETER STYLE SQL EXTERNAL NAME 'C_' NO SQL;",
            "create_function_external_scalar_statement",
            "ext_scalar_option_list_item",
        ),
    ] {
        let syntax =
            parse(Dialect::Db2, source).unwrap_or_else(|error| panic!("{source}: {error}"));
        assert!(syntax.diagnostics.is_empty());
        assert!(syntax.nodes.iter().any(|node| node.kind == header));
        let keyword = syntax
            .nodes
            .iter()
            .find(|node| node.class == NodeClass::Terminal && node.kind == "C_")
            .unwrap();
        assert_eq!(syntax.nodes[keyword.parent.unwrap()].kind, option);
        let raw = &source[keyword.span.clone().unwrap()];
        assert!(raw == "C" || raw == "c");
        let token = syntax
            .tokens
            .iter()
            .find(|token| token.kind == "C_")
            .unwrap();
        assert_eq!(token.channel, 0);
        assert_eq!(token.text.as_deref(), Some(raw));
        assert_eq!(token.span, keyword.span);
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
        }
        if source.starts_with("--") {
            for expected in ["-- β C C_", "'C_'"] {
                let token = syntax
                    .tokens
                    .iter()
                    .find(|token| token.text.as_deref() == Some(expected))
                    .unwrap();
                assert_eq!(&source[token.span.clone().unwrap()], expected);
                assert_eq!(token.channel != 0, expected.starts_with("--"));
            }
        }
    }
    for source in [
        "CREATE VARIABLE C INTEGER DEFAULT 1;",
        "CREATE VARIABLE c INTEGER DEFAULT 1;",
        "SELECT C FROM TABLE_F;",
        "CREATE FUNCTION C(C INTEGER) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'lib!entry' NO SQL;",
    ] {
        let syntax = parse(Dialect::Db2, source).unwrap();
        let identifiers = syntax
            .nodes
            .iter()
            .filter(|node| {
                node.class == NodeClass::Terminal
                    && node.kind == "C_"
                    && syntax.nodes[node.parent.unwrap()].kind == "id_"
            })
            .count();
        assert_eq!(
            identifiers,
            if source.starts_with("CREATE FUNCTION") {
                2
            } else {
                1
            }
        );
        assert!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "id_")
                .any(|node| { matches!(source.get(node.span.clone().unwrap()), Some("C" | "c")) })
        );
    }
    let java = "CREATE FUNCTION FUNC_JAVA_F(P_JAVA INTEGER) RETURNS INTEGER LANGUAGE JAVA PARAMETER STYLE JAVA EXTERNAL NAME 'pkg.method' NO SQL;";
    let syntax = parse(Dialect::Db2, java).unwrap();
    assert!(syntax.tokens.iter().any(|token| token.kind == "JAVA"));
    assert!(!syntax.tokens.iter().any(|token| token.kind == "C_"));
    for language in ["C_", "C_BOGUS"] {
        let source = format!(
            "CREATE FUNCTION FUNC_C_F(P_C INTEGER) RETURNS INTEGER LANGUAGE {language} PARAMETER STYLE SQL EXTERNAL NAME 'lib!entry' NO SQL;"
        );
        let recovered =
            graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &operation_context()).unwrap();
        assert!(!recovered.diagnostics.is_empty());
        assert!(matches!(
            parse(Dialect::Db2, &source),
            Err(AntlrEngineError::Syntax { .. })
        ));
    }
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, java, &cancelled),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
}

#[test]
fn db2_module_names_native() {
    use graphit_ast::antlr_engine::NodeClass;

    for (source, kind, raw) in [
        ("CREATE MODULE MODULE_F;", "module_name", "MODULE_F"),
        (
            "CREATE MODULE SCHEMA_F.MODULE_F;",
            "module_name",
            "SCHEMA_F.MODULE_F",
        ),
        (
            "DROP FUNCTION SCHEMA_F.FUNC_F(VARCHAR());",
            "function_name",
            "SCHEMA_F.FUNC_F",
        ),
        (
            "DROP PROCEDURE SCHEMA_F.PROC_F(INTEGER);",
            "procedure_name",
            "SCHEMA_F.PROC_F",
        ),
        (
            "DROP SPECIFIC FUNCTION SCHEMA_F.SPEC_F;",
            "specific_name",
            "SCHEMA_F.SPEC_F",
        ),
        (
            "DROP SPECIFIC PROCEDURE SCHEMA_F.SPEC_F;",
            "specific_name",
            "SCHEMA_F.SPEC_F",
        ),
        (
            "DROP FUNCTION \"S\".\"F\"\"n\"(VARCHAR());",
            "function_name",
            "\"S\".\"F\"\"n\"",
        ),
        (
            "CREATE VARIABLE SCHEMA_F.VAR_F INTEGER DEFAULT 1;",
            "schema_qualified_variable_declaration_name",
            "SCHEMA_F.VAR_F",
        ),
        (
            "-- β\r\nCREATE VARIABLE \"Esquema\".\"Va\"\"r\" VARCHAR(30) DEFAULT '%MODULE';",
            "schema_qualified_variable_declaration_name",
            "\"Esquema\".\"Va\"\"r\"",
        ),
    ] {
        let syntax =
            parse(Dialect::Db2, source).unwrap_or_else(|error| panic!("{source}: {error}"));
        let node = syntax.nodes.iter().find(|node| node.kind == kind).unwrap();
        assert_eq!(node.class, NodeClass::Rule);
        assert_eq!(&source[node.span.clone().unwrap()], raw);
        assert!(syntax.diagnostics.is_empty());
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        for token in &syntax.tokens {
            if token.eof {
                assert_eq!(token.status, graphit_ast::antlr_engine::SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
                continue;
            }
            if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
        if source.starts_with("--") {
            for (raw, kind, hidden) in [
                ("-- β", "LINE_COMMENT", true),
                ("'\u{25}MODULE'", "STRING_LITERAL", false),
                ("\"Va\"\"r\"", "DOUBLE_QUOTE_ID", false),
            ] {
                let token = syntax
                    .tokens
                    .iter()
                    .find(|token| token.text.as_deref() == Some(raw))
                    .unwrap();
                assert_eq!(token.kind, kind);
                assert_eq!(token.channel != 0, hidden);
            }
        }
    }
    for source in [
        "CREATE VARIABLE MODULE INTEGER DEFAULT 1;",
        "CREATE VARIABLE module INTEGER DEFAULT 1;",
        "SELECT MODULE FROM TABLE_F;",
        "CREATE MODULE MODULE;",
    ] {
        let syntax = parse(Dialect::Db2, source).unwrap();
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "MODULE"
                    && node.class == NodeClass::Terminal
                    && syntax.nodes[node.parent.unwrap()].kind == "id_")
                .count(),
            1
        );
        if source.starts_with("CREATE MODULE") {
            assert_eq!(
                syntax
                    .nodes
                    .iter()
                    .filter(|node| node.kind == "MODULE"
                        && node.class == NodeClass::Terminal
                        && syntax.nodes[node.parent.unwrap()].kind == "create_module_statement")
                    .count(),
                1
            );
        }
    }
    let modulo = "CREATE VARIABLE MODULO_F INTEGER DEFAULT (7 % 3);";
    let syntax = parse(Dialect::Db2, modulo).unwrap();
    let node = syntax
        .nodes
        .iter()
        .find(|node| node.kind == "MODULO" && node.class == NodeClass::Terminal)
        .unwrap();
    assert_eq!(&modulo[node.span.clone().unwrap()], "%");
    assert_eq!(syntax.nodes[node.parent.unwrap()].kind, "expression");
    for (source, kind, raw) in [
        (
            "ALTER MODULE MODULE_F DROP FUNCTION F(VARCHAR());",
            "module_function_designator",
            "FUNCTION F(VARCHAR())",
        ),
        (
            "ALTER MODULE MODULE_F DROP PROCEDURE PROC_F(TIMESTAMP());",
            "module_procedure_designator",
            "PROCEDURE PROC_F(TIMESTAMP())",
        ),
        (
            "ALTER MODULE SCHEMA_F.MODULE_F ADD FUNCTION F(INTEGER);",
            "module_function_definition",
            "FUNCTION F(INTEGER)",
        ),
        (
            "ALTER MODULE MODULE_F ADD SPECIFIC FUNCTION SPEC_F;",
            "module_function_definition",
            "SPECIFIC FUNCTION SPEC_F",
        ),
        (
            "ALTER MODULE MODULE_F ADD PROCEDURE PROC_F(INTEGER);",
            "module_procedure_definition",
            "PROCEDURE PROC_F(INTEGER)",
        ),
        (
            "ALTER MODULE MODULE_F ADD SPECIFIC PROCEDURE SPEC_F;",
            "module_procedure_definition",
            "SPECIFIC PROCEDURE SPEC_F",
        ),
    ] {
        let syntax =
            parse(Dialect::Db2, source).unwrap_or_else(|error| panic!("{source}: {error}"));
        let node = syntax.nodes.iter().find(|node| node.kind == kind).unwrap();
        assert_eq!(&source[node.span.clone().unwrap()], raw);
        assert!(
            syntax
                .nodes
                .iter()
                .any(|node| node.kind.starts_with("unqualified_")
                    && syntax.nodes[node.parent.unwrap()].kind == kind)
        );
        if source.contains("VARCHAR()") || source.contains("TIMESTAMP()") {
            let inferred = syntax
                .nodes
                .iter()
                .find(|node| node.kind == "inferred_builtin_type")
                .unwrap();
            assert!(matches!(
                source.get(inferred.span.clone().unwrap()),
                Some("VARCHAR()" | "TIMESTAMP()")
            ));
        }
    }
    for (declaration, value) in [
        ("V", "1"),
        ("SCHEMA_F.VAR_F", "1"),
        ("SCHEMA_F.VAR_F", "OTHER_V"),
        ("SCHEMA_F.VAR_F", "SCHEMA_F.OTHER_V"),
        ("SCHEMA_F.VAR_F", "SCHEMA_F.MODULE_F.OTHER_V"),
    ] {
        let source = format!("CREATE VARIABLE {declaration} INTEGER DEFAULT {value};");
        let syntax = parse(Dialect::Db2, &source).unwrap();
        if value.contains('.') {
            let node = syntax
                .nodes
                .iter()
                .find(|node| node.kind == "qualified_global_variable_reference")
                .unwrap();
            assert_eq!(&source[node.span.clone().unwrap()], value);
            assert_eq!(syntax.nodes[node.parent.unwrap()].kind, "global_variable");
        } else if value == "OTHER_V" {
            let node = syntax
                .nodes
                .iter()
                .find(|node| node.kind == "special_register")
                .unwrap();
            assert_eq!(&source[node.span.clone().unwrap()], value);
        }
        let declaration_node = syntax
            .nodes
            .iter()
            .find(|node| {
                node.kind
                    == if declaration.contains('.') {
                        "schema_qualified_variable_declaration_name"
                    } else {
                        "variable_name"
                    }
            })
            .unwrap();
        assert_eq!(&source[declaration_node.span.clone().unwrap()], declaration);
        assert_eq!(
            syntax.nodes[declaration_node.parent.unwrap()].kind,
            "create_variable_statement"
        );
    }
    for source in [
        "CREATE VARIABLE SCHEMA_F.MODULE_F.VAR_F INTEGER DEFAULT 1;",
        "CREATE VARIABLE V INTEGER DEFAULT SCHEMA_F.MODULE_F.NAMESPACE_F.VAR_F;",
        "DROP FUNCTION SCHEMA_F.MODULE_F.FUNC_F(INTEGER);",
        "ALTER MODULE MODULE_F DROP FUNCTION SCHEMA_F.FUNC_F(VARCHAR());",
        "ALTER MODULE MODULE_F ADD FUNCTION SCHEMA_F.FUNC_F(INTEGER);",
        "ALTER MODULE MODULE_F ADD PROCEDURE SCHEMA_F.PROC_F(INTEGER);",
        "ALTER MODULE MODULE_F ADD SPECIFIC FUNCTION SCHEMA_F.SPEC_F;",
        "ALTER MODULE MODULE_F ADD SPECIFIC PROCEDURE SCHEMA_F.SPEC_F;",
    ] {
        let recovered =
            graphit_ast::antlr_engine::parse(Dialect::Db2, source, &operation_context()).unwrap();
        assert!(!recovered.diagnostics.is_empty(), "{source}");
        assert!(
            matches!(
                parse(Dialect::Db2, source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
    let malformed =
        "CREATE VARIABLE SCHEMA_F.MODULE_F.VAR_F INTEGER DEFAULT 1; CREATE MODULE AFTER_F;";
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, malformed, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    assert!(
        recovered
            .nodes
            .iter()
            .any(|node| node.kind == "create_module_statement"
                && malformed.get(node.span.clone().unwrap()) == Some("CREATE MODULE AFTER_F"))
    );
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, modulo, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, modulo, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn db2_special_registers_native() {
    use graphit_ast::antlr_engine::{NodeClass, ProfileQuery, parse_profile_queries};

    for register in [
        "CURRENT CLIENT_ACCTNG",
        "CLIENT ACCTNG",
        "CURRENT CLIENT_APPLNAME",
        "CLIENT APPLNAME",
        "CURRENT CLIENT_USERID",
        "CLIENT USERID",
        "CURRENT CLIENT_WRKSTNNAME",
        "CLIENT WRKSTNNAME",
        "CURRENT DBPARTITIONNUM",
        "CURRENT DECFLOAT ROUNDING MODE",
        "CURRENT DEFAULT TRANSFORM GROUP",
        "CURRENT DEGREE",
        "CURRENT EXPLAIN MODE",
        "CURRENT EXPLAIN SNAPSHOT",
        "CURRENT FEDERATED ASYNCHRONY",
        "CURRENT IMPLICIT XMLPARSE OPTION",
        "CURRENT ISOLATION",
        "CURRENT LOCALE LC_MESSAGES",
        "CURRENT LOCALE LC_TIME",
        "CURRENT LOCK TIMEOUT",
        "CURRENT MAINTAINED TABLE TYPES FOR OPTIMIZATION",
        "CURRENT MDC ROLLOUT MODE",
        "CURRENT OPTIMIZATION PROFILE",
        "CURRENT PACKAGE PATH",
        "CURRENT PATH",
        "CURRENT_PATH",
        "CURRENT FUNCTION PATH",
        "CURRENT QUERY OPTIMIZATION",
        "CURRENT REFRESH AGE",
        "CURRENT SERVER",
        "CURRENT_SERVER",
        "CURRENT SQL_CCFLAGS",
        "CURRENT TEMPORAL BUSINESS_TIME",
        "CURRENT TEMPORAL SYSTEM_TIME",
        "CURRENT TIMEZONE",
        "CURRENT_TIMEZONE",
        "CURRENT DATE",
        "CURRENT TIME",
        "CURRENT TIMESTAMP",
        "CURRENT TIMESTAMP(3)",
        "CURRENT TIMESTAMP()",
        "CURRENT USER",
        "USER",
        "SESSION_USER",
        "SYSTEM_USER",
        "CURRENT SCHEMA",
        "CURRENT MEMBER",
        "CURRENT SQLID",
        "CURRENT_DATE",
        "CURRENT_TIME",
        "CURRENT_TIMESTAMP",
        "CURRENT_USER",
        "CURRENT_SCHEMA",
        "CURRENT_SQLID",
        "SYSDATE",
        "LOCALTIMESTAMP",
        "LOCALTIMESTAMP(3)",
        "LOCALTIMESTAMP()",
        "CURRENT_TIMESTAMP(3)",
        "CURRENT_TIMESTAMP()",
    ] {
        let source = format!("CREATE VARIABLE REG_F VARCHAR(200) DEFAULT {register};");
        let syntax =
            parse(Dialect::Db2, &source).unwrap_or_else(|error| panic!("{source}: {error}"));
        let special = syntax
            .nodes
            .iter()
            .find(|node| node.kind == "special_register")
            .unwrap();
        assert_eq!(&source[special.span.clone().unwrap()], register);
        assert_eq!(
            syntax.nodes[special.parent.unwrap()].kind,
            "create_variable_statement"
        );
        let native = register.contains(' ')
            || register.contains('(')
            || [
                "USER",
                "SESSION_USER",
                "SYSTEM_USER",
                "CURRENT_USER",
                "CURRENT_PATH",
            ]
            .contains(&register);
        assert_eq!(
            syntax
                .nodes
                .iter()
                .any(|node| node.kind == "native_special_register"),
            native,
            "{source}"
        );
        if !native {
            assert!(syntax.nodes.iter().any(|node| {
                node.kind == "id_"
                    && node.parent
                        == syntax
                            .nodes
                            .iter()
                            .position(|node| std::ptr::eq(node, special))
            }));
        }
        assert!(syntax.diagnostics.is_empty());
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
    }
    for name in [
        "ACCTNG",
        "USERID",
        "WRKSTNNAME",
        "TIMEZONE",
        "SQLID",
        "CURRENT_DATE",
        "CURRENT_TIME",
        "CURRENT_TIMESTAMP",
        "CURRENT_SCHEMA",
        "CURRENT_SQLID",
        "CURRENT_SERVER",
        "CURRENT_TIMEZONE",
        "SYSDATE",
        "LOCALTIMESTAMP",
        "current_timezone",
    ] {
        let source = format!("CREATE VARIABLE {name} INTEGER DEFAULT 1;");
        let syntax = parse(Dialect::Db2, &source).unwrap();
        let node = syntax
            .nodes
            .iter()
            .find(|node| node.kind == "variable_name")
            .unwrap();
        assert_eq!(&source[node.span.clone().unwrap()], name);
        assert!(
            syntax
                .nodes
                .iter()
                .any(|node| node.class == NodeClass::Terminal
                    && syntax.nodes[node.parent.unwrap()].kind == "id_"
                    && source.get(node.span.clone().unwrap()) == Some(name))
        );
    }
    for source in [
        "SELECT SYSDATE FROM TABLE_F;",
        "CREATE FUNCTION ACCTNG(P_IN INTEGER) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'lib!entry' NO SQL;",
        "CREATE VARIABLE \"SYSDATE\" INTEGER DEFAULT SCHEMA_F.CURRENT_TIMEZONE;",
    ] {
        let syntax = parse(Dialect::Db2, source).unwrap();
        assert!(syntax.diagnostics.is_empty());
    }
    for (value, calls, narrow, general) in [
        ("LOCALTIMESTAMP(3)", 0, true, false),
        ("CURRENT_TIMESTAMP(3)", 0, true, false),
        ("CURRENT_TIMESTAMP()", 0, true, false),
        ("LOCALTIMESTAMP()", 0, true, false),
        ("CURRENT TIMESTAMP(3)", 0, false, true),
        ("CURRENT TIMESTAMP()", 0, false, true),
        ("SCHEMA_F.LOCALTIMESTAMP(3)", 1, false, false),
        ("LOCALTIMESTAMP(1,2)", 1, false, false),
        ("LOCALTIMESTAMP(1+2)", 1, false, false),
    ] {
        let source = format!("CREATE VARIABLE REG_F TIMESTAMP DEFAULT ({value});");
        let parsed = parse_profile_queries(
            Dialect::Db2,
            &source,
            &[ProfileQuery {
                pattern: "//function_invocation/function_name".into(),
                captures: vec![Vec::new()],
                ..ProfileQuery::default()
            }],
            &operation_context(),
        )
        .unwrap();
        assert!(parsed.syntax.diagnostics.is_empty(), "{source}");
        assert_eq!(parsed.matches.len(), calls, "{source}");
        assert_eq!(
            parsed
                .syntax
                .nodes
                .iter()
                .any(|node| node.kind == "timestamp_register_invocation"),
            narrow,
            "{source}"
        );
        assert_eq!(
            parsed
                .syntax
                .nodes
                .iter()
                .any(|node| node.kind == "native_special_register"),
            general,
            "{source}"
        );
        if narrow {
            let node = parsed
                .syntax
                .nodes
                .iter()
                .find(|node| node.kind == "timestamp_register_invocation")
                .unwrap();
            assert_eq!(parsed.syntax.nodes[node.parent.unwrap()].kind, "expression");
            assert_eq!(&source[node.span.clone().unwrap()], value);
        }
    }
    for value in [
        "CLIENT ACCTNG",
        "CURRENT LOCALE LC_TIME",
        "CURRENT MAINTAINED TABLE TYPES FOR OPTIMIZATION",
        "CURRENT FUNCTION PATH",
        "CURRENT_SERVER",
        "CURRENT_TIMEZONE",
        "CURRENT TEMPORAL BUSINESS_TIME",
        "CURRENT TEMPORAL SYSTEM_TIME",
        "CURRENT TIMESTAMP - CURRENT TIMEZONE",
        "CURRENT TIMESTAMP(13)",
    ] {
        let source = format!("CREATE VARIABLE REG_F VARCHAR(200) DEFAULT ({value});");
        let syntax = parse(Dialect::Db2, &source).unwrap();
        assert!(syntax.diagnostics.is_empty());
    }
    let source = "-- β CURRENT_TIMESTAMP\r\nCREATE VARIABLE REG_F DATE DEFAULT current date;";
    let syntax = parse(Dialect::Db2, source).unwrap();
    for token in &syntax.tokens {
        if token.eof {
            assert_eq!(token.status, graphit_ast::antlr_engine::SpanStatus::Eof);
            assert_eq!(token.span, Some(source.len()..source.len()));
        } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
            assert_eq!(source.get(span.clone()), Some(text.as_str()));
        }
    }
    let comment = syntax
        .tokens
        .iter()
        .find(|token| token.kind == "LINE_COMMENT")
        .unwrap();
    assert_ne!(comment.channel, 0);
    assert_eq!(comment.text.as_deref(), Some("-- β CURRENT_TIMESTAMP"));
    let string = parse(
        Dialect::Db2,
        "CREATE VARIABLE STR_F VARCHAR(100) DEFAULT 'CURRENT_TIMESTAMP LOCALTIMESTAMP';",
    )
    .unwrap();
    assert!(
        !string
            .tokens
            .iter()
            .any(|token| token.kind == "CURRENT_TIMESTAMP" || token.kind == "LOCALTIMESTAMP")
    );
    for value in [
        "CURRENT LOCALE",
        "CURRENT DECFLOAT ROUNDING",
        "CURRENT CLIENT_ACCTNG(1)",
        "CURRENT TIMESTAMP(1,2)",
    ] {
        let source = format!("CREATE VARIABLE REG_F VARCHAR(200) DEFAULT ({value});");
        let recovered =
            graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &operation_context()).unwrap();
        assert!(!recovered.diagnostics.is_empty(), "{source}");
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
    let malformed =
        "CREATE VARIABLE REG_F VARCHAR(200) DEFAULT (CURRENT LOCALE); CREATE MODULE AFTER_F;";
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, malformed, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    assert!(
        recovered
            .nodes
            .iter()
            .any(|node| node.kind == "create_module_statement"
                && malformed.get(node.span.clone().unwrap()) == Some("CREATE MODULE AFTER_F"))
    );
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn db2_nontext_constants_native() {
    use graphit_ast::antlr_engine::{NodeClass, SpanStatus};

    for (value, anchor, child) in [
        ("-15", "signed_numeric_constant", "integer_constant"),
        ("+100", "signed_numeric_constant", "integer_constant"),
        ("+1", "signed_numeric_constant", "integer_constant"),
        ("-1", "signed_numeric_constant", "integer_constant"),
        ("+0", "signed_numeric_constant", "integer_constant"),
        ("-0", "signed_numeric_constant", "integer_constant"),
        ("1000.", "constant_", "FLOAT_LITERAL"),
        (".5", "constant_", "FLOAT_LITERAL"),
        ("15.2", "constant_", "FLOAT_LITERAL"),
        ("+5.E+2", "signed_numeric_constant", "REAL_LITERAL"),
        ("-15.2", "signed_numeric_constant", "FLOAT_LITERAL"),
        ("1E-3", "constant_", "REAL_LITERAL"),
        ("INF", "decfloat_special_constant", "INF"),
        ("inf", "decfloat_special_constant", "INF"),
        ("NaN", "decfloat_special_constant", "NAN"),
        ("SNAN", "decfloat_special_constant", "SNAN"),
        ("INFINITY", "decfloat_special_constant", "INFINITY"),
        (
            "+INFINITY",
            "signed_decfloat_special_constant",
            "decfloat_special_constant",
        ),
        (
            "-SNAN",
            "signed_decfloat_special_constant",
            "decfloat_special_constant",
        ),
        ("TRUE", "boolean_constant", "TRUE"),
        ("false", "boolean_constant", "FALSE"),
        ("DATE '2026-10-11'", "typed_datetime_constant", "string"),
        ("TIME '14:15:16'", "typed_datetime_constant", "string"),
        (
            "TIMESTAMP '2026-10-11-14.15.16.123456789012'",
            "typed_datetime_constant",
            "string",
        ),
        (
            "DATE 'not-a-calendar-value'",
            "typed_datetime_constant",
            "string",
        ),
    ] {
        let source = format!("CREATE VARIABLE CONST_F VARCHAR(200) DEFAULT {value};");
        let syntax =
            parse(Dialect::Db2, &source).unwrap_or_else(|error| panic!("{source}: {error}"));
        let (index, node) = syntax
            .nodes
            .iter()
            .enumerate()
            .find(|(_, node)| node.kind == anchor)
            .unwrap();
        assert_eq!(&source[node.span.clone().unwrap()], value, "{source}");
        assert!(
            syntax
                .nodes
                .iter()
                .any(|node| node.kind == child && node.parent == Some(index)),
            "missing {child}: {source}"
        );
        if matches!(value, "+1" | "-1" | "+0" | "-0") {
            let (integer_index, integer) = syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, child)| child.parent == Some(index) && child.kind == "integer_constant")
                .unwrap();
            let expected = if value.ends_with('1') {
                "L_ONE"
            } else {
                "L_ZERO"
            };
            let leaf = syntax
                .nodes
                .iter()
                .find(|child| child.parent == Some(integer_index) && child.kind == expected)
                .unwrap();
            assert_eq!(integer.span, leaf.span);
            assert_eq!(source.get(leaf.span.clone().unwrap()), Some(&value[1..]));
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == expected && token.span == leaf.span)
                .unwrap();
            assert_eq!(token.text.as_deref(), Some(&value[1..]));
            assert_eq!(token.status, SpanStatus::Regular);
            let sign_kind = if value.starts_with('+') {
                "PLUS"
            } else {
                "MINUS"
            };
            let sign = syntax
                .nodes
                .iter()
                .find(|child| child.parent == Some(index) && child.kind == sign_kind)
                .unwrap();
            assert_eq!(source.get(sign.span.clone().unwrap()), Some(&value[..1]));
        }
        let constant = syntax
            .nodes
            .iter()
            .find(|node| node.kind == "constant_")
            .unwrap();
        assert_eq!(&source[constant.span.clone().unwrap()], value);
        assert_eq!(
            syntax.nodes[constant.parent.unwrap()].kind,
            "create_variable_statement"
        );
        if anchor != "constant_" {
            assert_eq!(syntax.nodes[node.parent.unwrap()].kind, "constant_");
        }
        assert!(syntax.diagnostics.is_empty());
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        for token in &syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
        let expression = format!("CREATE VARIABLE EXPR_F VARCHAR(200) DEFAULT ({value});");
        let syntax = parse(Dialect::Db2, &expression).unwrap();
        assert!(syntax.nodes.iter().any(|node| node.kind == "constant_"
            && expression.get(node.span.clone().unwrap()) == Some(value)
            && syntax.nodes[node.parent.unwrap()].kind == "expression"));
    }
    for name in ["INF", "INFINITY", "NAN", "SNAN", "inf", "Nan", "UESCAPE"] {
        let source = format!("CREATE VARIABLE {name} INTEGER DEFAULT 1;");
        let syntax = parse(Dialect::Db2, &source).unwrap();
        let node = syntax
            .nodes
            .iter()
            .find(|node| node.kind == "variable_name")
            .unwrap();
        assert_eq!(&source[node.span.clone().unwrap()], name);
        assert!(
            syntax
                .nodes
                .iter()
                .any(|node| node.class == NodeClass::Terminal
                    && syntax.nodes[node.parent.unwrap()].kind == "id_"
                    && source.get(node.span.clone().unwrap()) == Some(name))
        );
    }
    let source = "CREATE VARIABLE \"NAN\" INTEGER DEFAULT SCHEMA_F.NAN;";
    let syntax = parse(Dialect::Db2, source).unwrap();
    let node = syntax
        .nodes
        .iter()
        .find(|node| node.kind == "qualified_global_variable_reference")
        .unwrap();
    assert_eq!(&source[node.span.clone().unwrap()], "SCHEMA_F.NAN");
    assert!(
        !syntax
            .nodes
            .iter()
            .any(|node| node.kind == "decfloat_special_constant")
    );
    let unknown = "CREATE VARIABLE UNKNOWN_F INTEGER DEFAULT \"UNKNOWN\";";
    let syntax = parse(Dialect::Db2, unknown).unwrap();
    let special = syntax
        .nodes
        .iter()
        .find(|node| node.kind == "special_register")
        .unwrap();
    assert_eq!(&unknown[special.span.clone().unwrap()], "\"UNKNOWN\"");
    assert!(
        !syntax
            .nodes
            .iter()
            .any(|node| node.kind == "boolean_constant")
    );
    let unquoted = "CREATE VARIABLE UNKNOWN_F INTEGER DEFAULT UNKNOWN;";
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, unquoted, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    let unknown_token = recovered
        .tokens
        .iter()
        .find(|token| token.kind == "UNKNOWN")
        .unwrap();
    assert_eq!(
        unquoted.get(unknown_token.span.clone().unwrap()),
        Some("UNKNOWN")
    );
    assert!(matches!(
        parse(Dialect::Db2, unquoted),
        Err(AntlrEngineError::Syntax { .. })
    ));
    for source in [
        "CREATE VARIABLE UNKNOWN_F INTEGER DEFAULT SCHEMA_F.\"UNKNOWN\";",
        "SELECT SNAN FROM TABLE_F;",
        "CREATE VARIABLE OLD_F INTEGER DEFAULT 0;",
        "CREATE VARIABLE OLD_F INTEGER DEFAULT 1;",
        "CREATE VARIABLE OLD_F INTEGER DEFAULT NULL;",
        "CREATE VARIABLE OLD_F VARCHAR(100) DEFAULT 'β ''quoted''';",
        "CREATE TYPE ARRAY_F AS INTEGER ARRAY[INTEGER];",
        "CREATE TYPE ARRAY_F AS INTEGER ARRAY[10];",
        "DROP FUNCTION F(DECIMAL(12,2),VARCHAR(10));",
    ] {
        let syntax = parse(Dialect::Db2, source).unwrap();
        assert!(syntax.diagnostics.is_empty());
    }
    let source = "-- β INF\r\nCREATE VARIABLE CONST_F INTEGER DEFAULT - /* β */\r\n15;";
    let syntax = parse(Dialect::Db2, source).unwrap();
    let node = syntax
        .nodes
        .iter()
        .find(|node| node.kind == "signed_numeric_constant")
        .unwrap();
    assert_eq!(&source[node.span.clone().unwrap()], "- /* β */\r\n15");
    for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
        let token = syntax
            .tokens
            .iter()
            .find(|token| token.kind == kind)
            .unwrap();
        assert_ne!(token.channel, 0);
        assert_eq!(
            source.get(token.span.clone().unwrap()),
            token.text.as_deref()
        );
    }
    for token in &syntax.tokens {
        if token.eof {
            assert_eq!(token.status, SpanStatus::Eof);
            assert_eq!(token.span, Some(source.len()..source.len()));
        } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
            assert_eq!(source.get(span.clone()), Some(text.as_str()));
        }
    }
    for source in [
        "CREATE VARIABLE BAD_F INTEGER DEFAULT + +1;",
        "CREATE VARIABLE BAD_F INTEGER DEFAULT - -1;",
        "CREATE VARIABLE BAD_F INTEGER DEFAULT +TRUE;",
        "CREATE VARIABLE BAD_F INTEGER DEFAULT +5.E+;",
        "CREATE VARIABLE BAD_F DATE DEFAULT DATE;",
        "CREATE VARIABLE BAD_F DATE DEFAULT DATE 'unterminated;",
        "DROP FUNCTION F(VARCHAR(-1));",
    ] {
        let recovered =
            graphit_ast::antlr_engine::parse(Dialect::Db2, source, &operation_context()).unwrap();
        assert!(!recovered.diagnostics.is_empty(), "{source}");
        assert!(
            matches!(
                parse(Dialect::Db2, source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
    let malformed = "CREATE VARIABLE BAD_F INTEGER DEFAULT +TRUE; CREATE MODULE AFTER_F;";
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, malformed, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    assert!(
        recovered
            .nodes
            .iter()
            .any(|node| node.kind == "create_module_statement"
                && malformed.get(node.span.clone().unwrap()) == Some("CREATE MODULE AFTER_F"))
    );
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, source, &cancelled),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, source, &expired),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn db2_prefixed_strings_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    for (value, family, token_kind) in [
        ("G'α''β'", "graphic_string_constant", "GRAPHIC_G_LITERAL"),
        ("g'β\r\nγ'", "graphic_string_constant", "GRAPHIC_G_LITERAL"),
        ("N'α'", "graphic_string_constant", "GRAPHIC_N_LITERAL"),
        ("n'a''b'", "graphic_string_constant", "GRAPHIC_N_LITERAL"),
        (
            "GX'FFFF'",
            "hex_graphic_string_constant",
            "HEX_GRAPHIC_LITERAL",
        ),
        (
            "gx'0041abcd'",
            "hex_graphic_string_constant",
            "HEX_GRAPHIC_LITERAL",
        ),
        (
            "BX'0000'",
            "binary_string_constant",
            "BINARY_STRING_LITERAL",
        ),
        (
            "bx'41aB'",
            "binary_string_constant",
            "BINARY_STRING_LITERAL",
        ),
        (
            "UX'0041'",
            "utf16be_graphic_string_constant",
            "UTF16BE_GRAPHIC_LITERAL",
        ),
        (
            "ux'D800'",
            "utf16be_graphic_string_constant",
            "UTF16BE_GRAPHIC_LITERAL",
        ),
        ("G''", "graphic_string_constant", "GRAPHIC_G_LITERAL"),
        ("N''", "graphic_string_constant", "GRAPHIC_N_LITERAL"),
        ("GX''", "hex_graphic_string_constant", "HEX_GRAPHIC_LITERAL"),
        ("BX''", "binary_string_constant", "BINARY_STRING_LITERAL"),
        (
            "UX''",
            "utf16be_graphic_string_constant",
            "UTF16BE_GRAPHIC_LITERAL",
        ),
    ] {
        let source = format!(
            "-- β\r\nCREATE VARIABLE STR_F VARCHAR(200) DEFAULT /* α /* β */ γ */ {value};"
        );
        let syntax =
            parse(Dialect::Db2, &source).unwrap_or_else(|error| panic!("{source}: {error}"));
        let (index, node) = syntax
            .nodes
            .iter()
            .enumerate()
            .find(|(_, node)| node.kind == family)
            .unwrap();
        assert_eq!(source.get(node.span.clone().unwrap()), Some(value));
        let prefix = &syntax.nodes[node.parent.unwrap()];
        assert_eq!(prefix.kind, "prefixed_string_constant");
        let constant = &syntax.nodes[prefix.parent.unwrap()];
        assert_eq!(constant.kind, "constant_");
        assert_eq!(
            syntax.nodes[constant.parent.unwrap()].kind,
            "create_variable_statement"
        );
        let terminal = syntax
            .nodes
            .iter()
            .find(|node| node.parent == Some(index) && node.kind == token_kind)
            .unwrap();
        assert_eq!(terminal.span, node.span);
        let token = syntax
            .tokens
            .iter()
            .find(|token| token.kind == token_kind)
            .unwrap();
        assert_eq!(token.text.as_deref(), Some(value));
        assert_eq!(token.span, node.span);
        assert_eq!(token.status, SpanStatus::Regular);
        assert!(syntax.diagnostics.is_empty());
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == kind)
                .unwrap();
            assert_ne!(token.channel, 0);
            assert_eq!(
                source.get(token.span.clone().unwrap()),
                token.text.as_deref()
            );
        }
        for token in &syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
        let expression = format!("CREATE VARIABLE EXPR_F VARCHAR(200) DEFAULT ({value});");
        let syntax = parse(Dialect::Db2, &expression).unwrap();
        let constant = syntax
            .nodes
            .iter()
            .find(|node| node.kind == "constant_")
            .unwrap();
        assert_eq!(expression.get(constant.span.clone().unwrap()), Some(value));
        assert_eq!(syntax.nodes[constant.parent.unwrap()].kind, "expression");
        assert!(syntax.nodes.iter().any(|node| node.kind == family));
    }
    for name in ["N", "n", "X", "BX", "GX", "UX", "\"G\""] {
        let source = format!("CREATE VARIABLE {name} INTEGER DEFAULT 1;");
        let syntax = parse(Dialect::Db2, &source).unwrap();
        let node = syntax
            .nodes
            .iter()
            .find(|node| node.kind == "variable_name")
            .unwrap();
        assert_eq!(source.get(node.span.clone().unwrap()), Some(name));
        if name != "\"G\"" {
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.text.as_deref() == Some(name))
                .unwrap();
            assert_eq!(token.kind, "ID");
        }
    }
    let bare_g = graphit_ast::antlr_engine::parse(Dialect::Db2, "G", &operation_context()).unwrap();
    assert_eq!(
        bare_g.tokens.iter().find(|token| !token.eof).unwrap().kind,
        "G"
    );
    for value in ["''", "'a''b'", "'β\r\nγ'", "X"] {
        let source = format!("CREATE VARIABLE OLD_F VARCHAR(200) DEFAULT {value};");
        let syntax = parse(Dialect::Db2, &source).unwrap();
        assert!(
            !syntax
                .nodes
                .iter()
                .any(|node| node.kind == "prefixed_string_constant")
        );
    }
    let query_source = "CREATE VARIABLE STR_F VARCHAR(200) DEFAULT (UX'0041');";
    let queries = parse_profile_queries(
        Dialect::Db2,
        query_source,
        &[ProfileQuery {
            pattern: "//function_invocation/function_name".into(),
            captures: vec![Vec::new()],
            ..ProfileQuery::default()
        }],
        &operation_context(),
    )
    .unwrap();
    assert!(queries.syntax.diagnostics.is_empty());
    assert!(queries.matches.is_empty());
    for value in [
        "BX'ABC'",
        "GX'004'",
        "UX'004100'",
        "BX'0G'",
        "GX'00 41'",
        "UX'0041 0042'",
        "G'unterminated",
        "N'α' 'extra'",
    ] {
        let source = format!("CREATE VARIABLE BAD_F VARCHAR(200) DEFAULT {value};");
        let recovered =
            graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &operation_context()).unwrap();
        assert!(!recovered.diagnostics.is_empty(), "{source}");
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
    let malformed = "CREATE VARIABLE BAD_F VARCHAR(200) DEFAULT BX'ABC'; CREATE MODULE AFTER_F;";
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, malformed, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    let tail = malformed.find("'ABC'").unwrap();
    assert!(
        recovered
            .diagnostics
            .iter()
            .any(
                |diagnostic| diagnostic.span == Some(tail..tail + "'ABC'".len())
                    && diagnostic.status == SpanStatus::Regular
            )
    );
    let prefix = recovered
        .tokens
        .iter()
        .find(|token| token.text.as_deref() == Some("BX"))
        .unwrap();
    assert_eq!(prefix.kind, "ID");
    let body = recovered
        .tokens
        .iter()
        .find(|token| token.text.as_deref() == Some("'ABC'"))
        .unwrap();
    assert_eq!(body.kind, "STRING_LITERAL");
    assert_eq!(body.span, Some(tail..tail + "'ABC'".len()));
    assert!(
        recovered
            .nodes
            .iter()
            .any(|node| node.kind == "create_module_statement"
                && malformed.get(node.span.clone().unwrap()) == Some("CREATE MODULE AFTER_F"))
    );
    assert!(matches!(
        parse(Dialect::Db2, malformed),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, query_source, &cancelled),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, query_source, &expired),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn db2_hex_character_strings_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    for value in [
        "X'4142'",
        "x'46 72 61 6E 6B'",
        "X' 4142 '",
        "x'4 1  4 2'",
        "X'  4  1   a  B  '",
        "X''",
        "X' '",
    ] {
        for expression in [false, true] {
            let default = if expression {
                format!("({value})")
            } else {
                value.into()
            };
            let source = format!(
                "-- β\r\nCREATE VARIABLE HEX_F VARCHAR(200) DEFAULT /* α /* β */ γ */ {default};"
            );
            let syntax =
                parse(Dialect::Db2, &source).unwrap_or_else(|error| panic!("{source}: {error}"));
            let (index, node) = syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, node)| node.kind == "hex_character_string_constant")
                .unwrap();
            assert_eq!(source.get(node.span.clone().unwrap()), Some(value));
            let prefix = &syntax.nodes[node.parent.unwrap()];
            assert_eq!(prefix.kind, "prefixed_string_constant");
            let constant = &syntax.nodes[prefix.parent.unwrap()];
            assert_eq!(constant.kind, "constant_");
            assert_eq!(
                syntax.nodes[constant.parent.unwrap()].kind,
                if expression {
                    "expression"
                } else {
                    "create_variable_statement"
                }
            );
            let terminal = syntax
                .nodes
                .iter()
                .find(|node| node.parent == Some(index) && node.kind == "HEX_CHARACTER_LITERAL")
                .unwrap();
            assert_eq!(terminal.span, node.span);
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == "HEX_CHARACTER_LITERAL")
                .unwrap();
            let start = source.find(value).unwrap();
            assert_eq!(token.text.as_deref(), Some(value));
            assert_eq!(token.span, Some(start..start + value.len()));
            assert_eq!(token.span, node.span);
            assert_eq!(token.status, SpanStatus::Regular);
            assert!(syntax.diagnostics.is_empty());
            for (index, node) in syntax.nodes.iter().enumerate() {
                assert!(!node.error && !node.missing);
                assert!(node.parent.is_none_or(|parent| parent < index));
                if let Some(span) = &node.span {
                    assert!(source.get(span.clone()).is_some());
                }
            }
            for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
                let token = syntax
                    .tokens
                    .iter()
                    .find(|token| token.kind == kind)
                    .unwrap();
                assert_ne!(token.channel, 0);
                assert_eq!(
                    source.get(token.span.clone().unwrap()),
                    token.text.as_deref()
                );
            }
            for token in &syntax.tokens {
                if token.eof {
                    assert_eq!(token.status, SpanStatus::Eof);
                    assert_eq!(token.span, Some(source.len()..source.len()));
                } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                    assert_eq!(source.get(span.clone()), Some(text.as_str()));
                }
            }
        }
    }
    for name in ["X", "x", "\"X\""] {
        let source = format!("CREATE VARIABLE {name} INTEGER DEFAULT 1;");
        let syntax = parse(Dialect::Db2, &source).unwrap();
        let name_node = syntax
            .nodes
            .iter()
            .find(|node| node.kind == "variable_name")
            .unwrap();
        assert_eq!(source.get(name_node.span.clone().unwrap()), Some(name));
        let token = syntax
            .tokens
            .iter()
            .find(|token| token.text.as_deref() == Some(name))
            .unwrap();
        assert_eq!(
            token.kind,
            if name == "\"X\"" {
                "DOUBLE_QUOTE_ID"
            } else {
                "ID"
            }
        );
        assert!(
            !syntax
                .nodes
                .iter()
                .any(|node| node.kind == "hex_character_string_constant")
        );
    }
    let query_source = "CREATE VARIABLE HEX_F VARCHAR(200) DEFAULT (x'4 1');";
    let queries = parse_profile_queries(
        Dialect::Db2,
        query_source,
        &[
            ProfileQuery {
                pattern: "//function_invocation/function_name".into(),
                captures: vec![Vec::new()],
                ..ProfileQuery::default()
            },
            ProfileQuery {
                pattern: "//hex_character_string_constant".into(),
                captures: vec![Vec::new()],
                ..ProfileQuery::default()
            },
        ],
        &operation_context(),
    )
    .unwrap();
    assert!(queries.syntax.diagnostics.is_empty());
    assert_eq!(queries.matches.len(), 1);
    assert_eq!(queries.matches[0].query_index, 1);
    let selected = &queries.syntax.nodes[queries.matches[0].node_index];
    assert_eq!(selected.kind, "hex_character_string_constant");
    assert_eq!(
        query_source.get(selected.span.clone().unwrap()),
        Some("x'4 1'")
    );

    let routine = "CREATE FUNCTION HEX_ROUTINE_F(IN \"Pé\" VARCHAR(20) DEFAULT X'4 1') RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.hex' NO SQL;";
    let syntax = parse(Dialect::Db2, routine).unwrap();
    let hex = syntax
        .nodes
        .iter()
        .find(|node| node.kind == "hex_character_string_constant")
        .unwrap();
    let prefixed = &syntax.nodes[hex.parent.unwrap()];
    let constant = &syntax.nodes[prefixed.parent.unwrap()];
    assert_eq!(constant.kind, "constant_");
    assert_eq!(
        syntax.nodes[constant.parent.unwrap()].kind,
        "routine_default_value"
    );
    assert_eq!(routine.get(hex.span.clone().unwrap()), Some("X'4 1'"));

    for value in [
        "X'ABC'",
        "X' A B C '",
        "X'0G'",
        "X'unterminated",
        "X'41' 'extra'",
    ] {
        let source = format!("CREATE VARIABLE BAD_F VARCHAR(200) DEFAULT {value};");
        let recovered =
            graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &operation_context()).unwrap();
        assert!(!recovered.diagnostics.is_empty(), "{source}");
        for diagnostic in &recovered.diagnostics {
            assert!(!diagnostic.message.is_empty());
            if let Some(span) = &diagnostic.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
    let malformed = "CREATE VARIABLE BAD_F VARCHAR(200) DEFAULT X'ABC'; CREATE MODULE AFTER_F;";
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, malformed, &operation_context()).unwrap();
    let tail = malformed.find("'ABC'").unwrap();
    assert!(
        recovered
            .diagnostics
            .iter()
            .any(
                |diagnostic| diagnostic.span == Some(tail..tail + "'ABC'".len())
                    && diagnostic.status == SpanStatus::Regular
            )
    );
    for (text, kind) in [("X", "ID"), ("'ABC'", "STRING_LITERAL")] {
        let token = recovered
            .tokens
            .iter()
            .find(|token| token.text.as_deref() == Some(text))
            .unwrap();
        assert_eq!(token.kind, kind);
        assert_eq!(malformed.get(token.span.clone().unwrap()), Some(text));
    }
    eprintln!(
        "DB2 X native recovery: diagnostics={}, following_module={}",
        recovered.diagnostics.len(),
        recovered
            .nodes
            .iter()
            .any(|node| node.kind == "create_module_statement")
    );
    assert!(matches!(
        parse(Dialect::Db2, malformed),
        Err(AntlrEngineError::Syntax { .. })
    ));

    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, query_source, &cancelled),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, query_source, &expired),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn db2_unicode_string_constants_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let cases = [
        ("U&''", "U&''"),
        ("u&'α''β'", "u&'α''β'"),
        (r"U&'c:\\temp'", r"U&'c:\\temp'"),
        (r"U&'\0041β'", r"U&'\0041β'"),
        (r"U&'\+01D11Eβ'", r"U&'\+01D11Eβ'"),
        (r"U&'\0041F'", r"U&'\0041F'"),
        (r"U&'\+01D11EF'", r"U&'\+01D11EF'"),
        (r"U&'\D800'", r"U&'\D800'"),
        ("U&'@+01D11E' UESCAPE '@'", "U&'@+01D11E'"),
        ("U&'@@@0041' UESCAPE '@'", "U&'@@@0041'"),
        (r"U&'\041' UESCAPE '@'", r"U&'\041'"),
        ("U&'🙂0041' UESCAPE '🙂'", "U&'🙂0041'"),
        ("U&'Z0041z' UESCAPE 'Z'", "U&'Z0041z'"),
        ("U&'\u{feff}β\r\nγ'", "U&'\u{feff}β\r\nγ'"),
        (
            "U&'@0041' /* α /* β */ γ */ UESCAPE -- 🙂\r\n '@'",
            "U&'@0041'",
        ),
    ];
    for (value, raw) in cases {
        for expression in [false, true] {
            let default = if expression {
                format!("({value})")
            } else {
                value.into()
            };
            let source = format!("-- β\r\nCREATE VARIABLE U_F VARCHAR(200) DEFAULT {default};");
            let syntax =
                parse(Dialect::Db2, &source).unwrap_or_else(|error| panic!("{source}: {error}"));
            let (index, node) = syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, node)| node.kind == "unicode_string_constant")
                .unwrap();
            assert_eq!(source.get(node.span.clone().unwrap()), Some(value));
            let constant = &syntax.nodes[node.parent.unwrap()];
            assert_eq!(constant.kind, "constant_");
            assert_eq!(
                syntax.nodes[constant.parent.unwrap()].kind,
                if expression {
                    "expression"
                } else {
                    "create_variable_statement"
                }
            );
            let terminal = syntax
                .nodes
                .iter()
                .find(|node| node.parent == Some(index) && node.kind == "UNICODE_STRING_LITERAL")
                .unwrap();
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == "UNICODE_STRING_LITERAL")
                .unwrap();
            assert_eq!(token.text.as_deref(), Some(raw));
            assert_eq!(source.get(token.span.clone().unwrap()), Some(raw));
            assert_eq!(token.span, terminal.span);
            assert_eq!(token.status, SpanStatus::Regular);
            assert_eq!(
                syntax
                    .nodes
                    .iter()
                    .filter(|n| n.kind == "unicode_escape_clause")
                    .count(),
                usize::from(value.contains("UESCAPE"))
            );
            assert!(syntax.diagnostics.is_empty());
            for (position, node) in syntax.nodes.iter().enumerate() {
                assert!(!node.error && !node.missing);
                assert!(node.parent.is_none_or(|parent| parent < position));
                if let Some(span) = &node.span {
                    assert!(source.get(span.clone()).is_some());
                }
            }
            for token in &syntax.tokens {
                if token.eof {
                    assert_eq!(token.status, SpanStatus::Eof);
                    assert_eq!(token.span, Some(source.len()..source.len()));
                } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                    assert_eq!(source.get(span.clone()), Some(text.as_str()));
                }
            }
            if value.contains("/*") {
                for kind in ["SQL_COMMENT", "LINE_COMMENT"] {
                    assert!(
                        syntax
                            .tokens
                            .iter()
                            .any(|token| token.kind == kind && token.channel != 0)
                    );
                }
            }
        }
    }
    for name in ["UESCAPE", "uescape", "\"UESCAPE\"", "APP.UESCAPE", "U"] {
        let source = format!("CREATE VARIABLE {name} INTEGER DEFAULT 1;");
        let syntax = parse(Dialect::Db2, &source).unwrap();
        let node = syntax
            .nodes
            .iter()
            .find(|node| {
                node.kind
                    == if name.contains('.') {
                        "schema_qualified_variable_declaration_name"
                    } else {
                        "variable_name"
                    }
            })
            .unwrap();
        assert_eq!(source.get(node.span.clone().unwrap()), Some(name));
    }
    let query_source = "CREATE VARIABLE U_F VARCHAR(200) DEFAULT (U&'@0041' UESCAPE '@');";
    let query = parse_profile_queries(
        Dialect::Db2,
        query_source,
        &[ProfileQuery {
            pattern: "//function_invocation/function_name".into(),
            captures: vec![Vec::new()],
            ..ProfileQuery::default()
        }],
        &operation_context(),
    )
    .unwrap();
    assert!(query.syntax.diagnostics.is_empty());
    assert!(query.matches.is_empty());
    for value in [
        r"U&'\041'",
        r"U&'\+01D11'",
        r"U&'\00G1'",
        r"U&'\+01D11G'",
        "U&'@041' UESCAPE '@'",
        "U&'ok' UESCAPE ''",
        "U&'ok' UESCAPE 'xy'",
        "U&'ok' UESCAPE '+'",
        "U&'ok' UESCAPE ''''",
        "U&'ok' UESCAPE '\"'",
        "U&'ok' UESCAPE ' '",
        "U&'ok' UESCAPE '1'",
        "U&'ok' UESCAPE 'F'",
        "U&'ok' UESCAPE 'f'",
    ] {
        let source = format!("-- 🙂\r\nCREATE VARIABLE BAD_U VARCHAR(200) DEFAULT {value};");
        let syntax =
            graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &operation_context()).unwrap();
        assert!(!syntax.diagnostics.is_empty(), "{source}");
        let whole_start = source.find(value).unwrap();
        for diagnostic in &syntax.diagnostics {
            assert!(
                diagnostic.grammar.starts_with("Db2Unicode"),
                "{diagnostic:?}"
            );
            assert_eq!(diagnostic.status, SpanStatus::Regular);
            let span = diagnostic.span.clone().unwrap();
            assert!(
                span.start >= whole_start && span.end <= whole_start + value.len(),
                "{diagnostic:?}"
            );
            assert!(source.get(span).is_some());
            assert_eq!(diagnostic.line, 2);
            assert!(!diagnostic.message.is_empty());
        }
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
    let invalid_argument = r"CREATE VARIABLE BAD_U VARCHAR(200) DEFAULT U&'\041' UESCAPE '+';";
    let syntax =
        graphit_ast::antlr_engine::parse(Dialect::Db2, invalid_argument, &operation_context())
            .unwrap();
    assert!(!syntax.diagnostics.is_empty());
    let arg_start = invalid_argument.find("'+'").unwrap();
    assert!(syntax.diagnostics.iter().all(|diagnostic| {
        diagnostic
            .span
            .as_ref()
            .is_some_and(|span| span.start >= arg_start)
    }));
    let hidden_bad = "-- α\r\nCREATE VARIABLE BAD_U VARCHAR(200) DEFAULT U&'ok' /* β\r\n γ */ UESCAPE -- 🙂\r\n '+';";
    let syntax =
        graphit_ast::antlr_engine::parse(Dialect::Db2, hidden_bad, &operation_context()).unwrap();
    let argument = hidden_bad.find("'+'").unwrap();
    assert!(!syntax.diagnostics.is_empty());
    assert!(
        syntax
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.grammar == "Db2UnicodeParser.g4"
                && diagnostic.line == 4
                && diagnostic.column == 2
                && diagnostic.span == Some(argument + 1..argument + 2))
    );
    assert!(syntax.diagnostics.iter().all(|diagnostic| {
        diagnostic
            .span
            .as_ref()
            .is_some_and(|span| span.start >= argument && span.end <= argument + 3)
    }));
    assert!(matches!(
        parse(Dialect::Db2, hidden_bad),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let recovered_source =
        r"CREATE VARIABLE BAD_U VARCHAR(200) DEFAULT U&'\041'; CREATE MODULE AFTER_U;";
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, recovered_source, &operation_context())
            .unwrap();
    assert!(!recovered.diagnostics.is_empty());
    assert!(
        recovered
            .nodes
            .iter()
            .any(|node| node.kind == "create_module_statement")
    );
    assert!(matches!(
        parse(Dialect::Db2, recovered_source),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let multiple = r"CREATE VARIABLE A_U VARCHAR(200) DEFAULT U&'\041'; CREATE VARIABLE B_U VARCHAR(200) DEFAULT U&'@041' UESCAPE '@';";
    let syntax =
        graphit_ast::antlr_engine::parse(Dialect::Db2, multiple, &operation_context()).unwrap();
    for raw in [r"U&'\041'", "U&'@041'"] {
        let start = multiple.find(raw).unwrap();
        assert!(syntax.diagnostics.iter().any(|d| {
            d.span
                .as_ref()
                .is_some_and(|span| span.start >= start && span.end <= start + raw.len())
        }));
    }
    let mixed = r"CREATE VARIABLE BAD_U VARCHAR(200) DEFAULT U&'\041'; @;";
    let syntax =
        graphit_ast::antlr_engine::parse(Dialect::Db2, mixed, &operation_context()).unwrap();
    let marker = mixed.find('@').unwrap();
    assert!(
        syntax
            .diagnostics
            .iter()
            .any(|d| d.grammar == "Db2Lexer.g4" && d.span == Some(marker..marker + 1))
    );
    assert!(
        syntax
            .diagnostics
            .iter()
            .any(|d| d.grammar == "Db2UnicodeParser.g4"
                && d.span.as_ref().is_some_and(|span| span.end < marker))
    );
    let multiline = "-- α\r\nCREATE VARIABLE BAD_U VARCHAR(200) DEFAULT U&'β\r\n\\041';";
    let syntax =
        graphit_ast::antlr_engine::parse(Dialect::Db2, multiline, &operation_context()).unwrap();
    assert!(syntax.diagnostics.iter().any(|d| {
        d.line == 3
            && d.column == 0
            && d.span
                .as_ref()
                .is_some_and(|span| multiline.get(span.clone()) == Some("\\"))
    }));
    std::thread::scope(|scope| {
        let handles: Vec<_> = ["@", "!", "🙂", "Z"].into_iter().map(|escape| scope.spawn(move || {
            let source = format!("CREATE VARIABLE U_F VARCHAR(200) DEFAULT U&'{escape}0041' UESCAPE '{escape}';");
            for _ in 0..8 { assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty()); }
        })).collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, query_source, &cancelled),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, query_source, &expired),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn plsql_metadata_token_does_not_collide_with_generated_metadata() {
    let source = "CREATE SEQUENCE s SHARING = METADATA;";
    let syntax = parse(Dialect::PlSql, source).unwrap();
    assert!(
        syntax
            .tokens
            .iter()
            .any(|token| token.kind == "METADATA" && token.text.as_deref() == Some("METADATA"))
    );
}

#[test]
fn incomplete_source_retains_recovery_and_diagnostics() {
    let source = "SELECT x FROM ;";
    let syntax =
        graphit_ast::antlr_engine::parse(Dialect::TSql, source, &operation_context()).unwrap();
    assert!(!syntax.diagnostics.is_empty());
    assert!(!syntax.nodes.is_empty());
    assert!(parse(Dialect::TSql, source).is_err());

    let source = "IDENTIFICATION . PROGRAM-ID. HELLO. PROCEDURE DIVISION. STOP RUN.";
    let syntax =
        graphit_ast::antlr_engine::parse(Dialect::Cobol85, source, &operation_context()).unwrap();
    let inserted: Vec<_> = syntax.nodes.iter().filter(|node| node.missing).collect();
    assert!(
        inserted.iter().any(|node| node.kind == "DIVISION"),
        "the SDK must insert the missing DIVISION"
    );
    for node in inserted {
        assert!(node.error);
        assert_eq!(node.span, None);
        assert_eq!(
            node.status,
            graphit_ast::antlr_engine::SpanStatus::Unindexed(
                graphit_ast::antlr_engine::UnindexedReason::InsertedRecoveryToken
            )
        );
    }
    assert!(parse(Dialect::Cobol85, source).is_err());
}

#[test]
fn postgres_dollar_tags_and_operator_actions_preserve_tokens() {
    let source = "SELECT $Tag$á $tag$ x$Tag$, 1..2, a << 2;";
    assert!(parse(Dialect::PostgreSql, source).is_err());
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::PostgreSql, source, &operation_context())
            .unwrap();
    let number = recovered
        .tokens
        .iter()
        .position(|token| token.text.as_deref() == Some("1"))
        .unwrap();
    assert_eq!(recovered.tokens[number].kind, "Integral");
    assert_eq!(recovered.tokens[number + 1].text.as_deref(), Some(".."));
    assert_eq!(recovered.tokens[number + 2].kind, "Integral");
    let source = "SELECT $Tag$á $tag$ x$Tag$, a << 2;";
    let syntax = parse(Dialect::PostgreSql, source).unwrap();
    assert!(syntax.tokens.iter().any(|token| token.kind == "LESS_LESS"));
    assert!(
        syntax
            .tokens
            .iter()
            .any(|token| token.text.as_deref() == Some("$Tag$"))
    );
}

#[test]
fn postgres_routine_hook_exposes_real_dollar_body_span() {
    let source = "CREATE FUNCTION f() RETURNS void AS $body$BEGIN IF true THEN NULL; END IF; END$body$ LANGUAGE plpgsql;";
    let syntax = parse(Dialect::PostgreSql, source).unwrap();
    assert_eq!(syntax.embedded_bodies.len(), 1);
    assert_eq!(
        &source[syntax.embedded_bodies[0].clone()],
        "BEGIN IF true THEN NULL; END IF; END"
    );
    let ordinary = "CREATE FUNCTION f() RETURNS integer AS 'SELECT 1' LANGUAGE sql;";
    assert!(
        parse(Dialect::PostgreSql, ordinary)
            .unwrap()
            .embedded_bodies
            .is_empty()
    );
}

#[test]
fn one_initial_bom_preserves_original_sdk_byte_positions() {
    for (dialect, statement) in [
        (Dialect::Db2, "SELECT x FROM t;"),
        (Dialect::TSql, "SELECT N'á' AS label;"),
        (Dialect::PostgreSql, "SELECT 'á' AS label;"),
        (Dialect::PlSql, "BEGIN NULL; END;"),
        (
            Dialect::Cobol85,
            "IDENTIFICATION DIVISION. PROGRAM-ID. HELLO. PROCEDURE DIVISION. STOP RUN.",
        ),
    ] {
        let source = format!("\u{feff}{statement}");
        let syntax = parse(dialect, &source).unwrap();
        let first = syntax
            .tokens
            .iter()
            .find(|token| token.channel == 0 && !token.eof)
            .unwrap();
        assert_eq!(first.span.as_ref().unwrap().start, 3, "{dialect:?}");
        for token in syntax.tokens.iter().filter(|token| !token.eof) {
            if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(&source[span.clone()], text);
            }
        }
    }
    assert!(parse(Dialect::TSql, "\u{feff}\u{feff}SELECT 1;").is_err());
}

fn plpgsql_fixture() -> graphit_ast::tree_sitter_engine::GrammarLanguage {
    graphit_ast::tree_sitter_engine::builtin("tree-sitter-plpgsql").unwrap()
}

#[test]
fn postgres_splice_uses_real_plpgsql_nodes_queries_and_original_points() {
    use graphit_ast::{
        antlr_engine::parse_postgres_with_plpgsql, tree_sitter_engine::ParseControl,
    };
    for tag in ["$$", "$MiXeD$"] {
        let body_text = "DECLARE\r\n  value text := 'á';\r\nBEGIN\r\n  RAISE NOTICE '%', value;\r\n  RETURN;\r\nEND;";
        let source = format!(
            "-- cabeçalho α\r\nCREATE FUNCTION f() RETURNS void AS {tag}{body_text}{tag} LANGUAGE plpgsql;"
        );
        let language = plpgsql_fixture();
        let query = language
            .compile_query("((identifier) @name (#eq? @name \"value\"))")
            .unwrap();
        let syntax = parse_postgres_with_plpgsql(
            &source,
            &language,
            ParseControl::default(),
            &operation_context(),
        )
        .unwrap();
        assert!(syntax.host.diagnostics.is_empty());
        assert_eq!(syntax.bodies.len(), 1);
        drop(language);
        let body = &syntax.bodies[0];
        assert_eq!(body.tree().source(), body_text);
        assert!(!body.tree().has_syntax_errors());
        assert_eq!(
            syntax.host.nodes[body.host_parent].kind,
            "createfunc_opt_list"
        );
        assert_eq!(&source[body.span.clone()], body_text);
        let context = operation_context();
        let matches = body
            .matches_with_context(&query, ParseControl::default(), &context)
            .unwrap();
        assert_eq!(
            matches,
            body.matches(&query, ParseControl::default()).unwrap()
        );
        let local_matches = body
            .tree()
            .matches_with_context(&query, ParseControl::default(), &context)
            .unwrap();
        assert!(!matches.is_empty());
        for (projected, local) in matches.iter().zip(&local_matches) {
            assert_eq!(projected.pattern_index, local.pattern_index);
            assert_eq!(projected.properties, local.properties);
            for (projected, local) in projected.captures.iter().zip(&local.captures) {
                assert_eq!(projected.node_id, local.node_id);
                assert_eq!(
                    projected.range,
                    (body.span.start + local.range.start)..(body.span.start + local.range.end)
                );
            }
        }
        let original = graphit_ast::SourceText::new(source.as_bytes(), u32::MAX as usize).unwrap();
        for capture in matches.iter().flat_map(|found| &found.captures) {
            assert_eq!(capture.text, "value");
            assert_eq!(&source[capture.range.clone()], capture.text);
            assert_eq!(
                capture.start,
                original.point_at(capture.range.start).unwrap()
            );
            assert_eq!(capture.end, original.point_at(capture.range.end).unwrap());
        }
        drop(query);
        let nodes = body
            .nodes_with_context(ParseControl::default(), &context)
            .unwrap();
        assert_eq!(nodes, body.nodes(ParseControl::default()).unwrap());
        let local_nodes = body
            .tree()
            .nodes_with_context(ParseControl::default(), &context)
            .unwrap();
        assert_eq!(nodes.len(), local_nodes.len());
        for (projected, local) in nodes.iter().zip(&local_nodes) {
            assert_eq!(projected.node_id, local.node_id);
            assert_eq!(projected.parent, local.parent);
            assert_eq!(projected.kind, local.kind);
            assert_eq!(projected.field, local.field);
            assert_eq!(
                (projected.named, projected.error, projected.missing),
                (local.named, local.error, local.missing)
            );
        }
        assert!(nodes.iter().any(|node| node.kind == "pl_block"));
        assert_eq!(nodes[0].parent, None);
        for node in &nodes {
            assert!(node.range.start >= body.span.start && node.range.end <= body.span.end);
            assert_eq!(node.start, original.point_at(node.range.start).unwrap());
            assert_eq!(node.end, original.point_at(node.range.end).unwrap());
        }
    }
}

#[test]
fn postgres_splice_preserves_host_literals_and_routes_only_plpgsql_dollar_as() {
    use graphit_ast::{
        antlr_engine::parse_postgres_with_plpgsql, tree_sitter_engine::ParseControl,
    };
    let language = plpgsql_fixture();
    for literal in [
        "'BEGIN RAISE NOTICE ''á''; END;'",
        r"E'BEGIN RAISE NOTICE \'á\'; END;'",
        "U&'BEGIN RETURN; END;'",
    ] {
        let source = format!("CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS {literal};");
        let host = parse(Dialect::PostgreSql, &source).unwrap();
        assert!(host.embedded_bodies.is_empty());
        let token = host
            .tokens
            .iter()
            .find(|token| token.text.as_deref() == Some(literal))
            .unwrap();
        assert_eq!(&source[token.span.clone().unwrap()], literal);
        let syntax = parse_postgres_with_plpgsql(
            &source,
            &language,
            ParseControl::default(),
            &operation_context(),
        )
        .unwrap();
        assert!(syntax.bodies.is_empty());
    }
    let source = "CREATE FUNCTION f() RETURNS int LANGUAGE sql AS $$SELECT 1;$$;";
    let syntax = parse_postgres_with_plpgsql(
        source,
        &language,
        ParseControl::default(),
        &operation_context(),
    )
    .unwrap();
    assert!(syntax.host.diagnostics.is_empty());
    assert!(syntax.bodies.is_empty());
    let source = "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$BEGIN RETURN; END;$$ SET search_path TO $x$public$x$;";
    let syntax = parse_postgres_with_plpgsql(
        source,
        &language,
        ParseControl::default(),
        &operation_context(),
    )
    .unwrap();
    assert!(syntax.host.diagnostics.is_empty());
    assert_eq!(syntax.bodies.len(), 1);
    assert_eq!(syntax.host.embedded_bodies.len(), 1);
    assert_eq!(syntax.bodies[0].tree().source(), "BEGIN RETURN; END;");
}

#[test]
fn postgres_splice_retains_recovery_and_honors_parse_control() {
    use graphit_ast::{
        antlr_engine::{EmbeddedError, parse_postgres_with_plpgsql},
        tree_sitter_engine::{EngineError, ParseControl},
    };
    let language = plpgsql_fixture();
    let source = "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$BEGIN IF THEN; END;$$;";
    let syntax = parse_postgres_with_plpgsql(
        source,
        &language,
        ParseControl::default(),
        &operation_context(),
    )
    .unwrap();
    assert!(syntax.host.diagnostics.is_empty());
    assert!(syntax.bodies[0].tree().has_syntax_errors());
    assert!(
        syntax.bodies[0]
            .nodes(ParseControl::default())
            .unwrap()
            .iter()
            .any(|node| node.error || node.missing)
    );
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    let control = ParseControl {
        cancellation: Some(&cancelled),
        deadline: None,
    };
    assert!(matches!(
        parse_postgres_with_plpgsql(source, &language, control, &operation_context()),
        Err(EmbeddedError::TreeSitter(EngineError::Cancelled))
    ));
    assert!(matches!(
        syntax.bodies[0].nodes(control),
        Err(EmbeddedError::TreeSitter(EngineError::Cancelled))
    ));
    let expired = ParseControl {
        cancellation: None,
        deadline: Some(std::time::Instant::now()),
    };
    assert!(matches!(
        parse_postgres_with_plpgsql(source, &language, expired, &operation_context()),
        Err(EmbeddedError::TreeSitter(EngineError::TimedOut))
    ));
}

#[test]
fn plsql_metadata_preprocessing_uses_lexer_boundaries_and_real_origins() {
    use graphit_ast::{
        antlr_engine::preprocessing::prepare_plsql,
        prepared_input::{OriginalProjection, PreparedInput},
    };
    let source = "CREATE MATERIALIZED VIEW mv (x) USING (nested (x), 'á ) quoted')\r\nREFRESH COMPLETE AS SELECT x FROM t;";
    let prepared = prepare_plsql(source, &operation_context()).unwrap();
    assert_eq!(prepared.text().len(), source.len());
    assert!(!prepared.text().contains("nested"));
    assert!(prepared.text().contains("\r\nREFRESH"));
    let syntax = parse(Dialect::PlSql, prepared.text()).unwrap();
    let select = syntax
        .tokens
        .iter()
        .find(|token| token.text.as_deref() == Some("SELECT"))
        .unwrap()
        .span
        .clone()
        .unwrap();
    let context = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(60),
        graphit_core::file_lock::Cancellation::default(),
    );
    prepared.with_origin_plan(|plan| {
        assert!(!plan.removed_original.is_empty());
        let input = PreparedInput::new(
            source.as_bytes(),
            prepared.text().as_bytes(),
            plan,
            &context,
        )
        .unwrap();
        match input
            .project_prepared(select.start as u64, select.end as u64)
            .unwrap()
        {
            OriginalProjection::Exact(range) => {
                assert_eq!(range.text(), "SELECT");
                assert_eq!(range.start(), source.find("SELECT").unwrap());
            }
            _ => panic!("copied SELECT must retain exact origin"),
        }
    });
    let malformed =
        "CREATE MATERIALIZED VIEW mv (x) USING (nested (x) REFRESH COMPLETE AS SELECT x FROM t;";
    let prepared = prepare_plsql(malformed, &operation_context()).unwrap();
    assert_eq!(prepared.text(), malformed);
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::PlSql, prepared.text(), &operation_context())
            .unwrap();
    assert!(!recovered.diagnostics.is_empty());
    let unfinished = "CREATE MATERIALIZED VIEW mv (x) USING ('unfinished";
    let prepared = prepare_plsql(unfinished, &operation_context()).unwrap();
    assert_eq!(prepared.text(), unfinished);
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::PlSql, prepared.text(), &operation_context())
            .unwrap();
    assert!(!recovered.diagnostics.is_empty());
    assert!(parse(Dialect::PlSql, prepared.text()).is_err());
}

#[test]
fn cobol_formats_continuations_comments_and_compound_origins() {
    use graphit_ast::{
        antlr_engine::preprocessing::{CobolFormat, prepare_cobol},
        prepared_input::{OriginPiece, OriginalProjection, PreparedInput},
    };
    let fixed = "000100 IDENTIFICATION DIVISION.\r\n000200 PROGRAM-ID. HELLO.\r\n000300 AUTHOR. by café\r\n000400     documentation text\r\n000500 PROCEDURE DIVISION.\r\n000600*comment café\r\n000700     DISPLAY 'HELLOÁ\r\n000800-    'WORLD'.\r\n000900     STOP RUN.\r\n";
    let tandem = " IDENTIFICATION DIVISION.\n PROGRAM-ID. HELLO.\n AUTHOR. by café\n PROCEDURE DIVISION.\n*comment café\n     DISPLAY 'HELLOÁ\n-    'WORLD'.\n     STOP RUN.\n";
    for (format, source) in [
        (CobolFormat::Fixed, fixed),
        (CobolFormat::Variable, fixed),
        (CobolFormat::Tandem, tandem),
    ] {
        let prepared = prepare_cobol(source, format, &operation_context()).unwrap();
        assert!(prepared.text().contains("'HELLOÁWORLD'"));
        assert!(prepared.text().contains("*>CE"));
        let syntax = parse(Dialect::Cobol85, prepared.text()).unwrap();
        assert!(syntax.tokens.iter().any(|token| {
            token.channel != 0
                && token
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("comment café"))
        }));
        let literal = syntax
            .tokens
            .iter()
            .find(|token| token.text.as_deref() == Some("'HELLOÁWORLD'"))
            .unwrap()
            .span
            .clone()
            .unwrap();
        let context = graphit_core::file_io::SnapshotContext::new(
            std::time::Instant::now() + std::time::Duration::from_secs(60),
            graphit_core::file_lock::Cancellation::default(),
        );
        prepared.with_origin_plan(|plan| {
            let input = PreparedInput::new(
                source.as_bytes(),
                prepared.text().as_bytes(),
                plan,
                &context,
            )
            .unwrap();
            match input
                .project_prepared(literal.start as u64, literal.end as u64)
                .unwrap()
            {
                OriginalProjection::Compound(pieces) => {
                    let text: String = pieces
                        .into_iter()
                        .map(|piece| match piece {
                            OriginPiece::Original(range) => range.text().to_owned(),
                            _ => panic!("literal pieces are original"),
                        })
                        .collect();
                    assert_eq!(text, "'HELLOÁWORLD'");
                }
                _ => panic!("continuation cannot be represented by an exact original hull"),
            }
        });
    }
    let with_bom = format!("\u{feff}{fixed}");
    assert!(
        parse(
            Dialect::Cobol85,
            prepare_cobol(&with_bom, CobolFormat::Fixed, &operation_context())
                .unwrap()
                .text()
        )
        .is_ok()
    );
    assert!(prepare_cobol("000100?broken", CobolFormat::Fixed, &operation_context()).is_err());
}

#[test]
fn native_eof_unicode_and_lexer_diagnostics_have_real_boundaries() {
    use graphit_ast::antlr_engine::{self, DiagnosticPhase, SpanStatus};
    for source in ["", "\u{feff}"] {
        let syntax = antlr_engine::parse(Dialect::TSql, source, &operation_context()).unwrap();
        let eof = syntax.tokens.iter().find(|token| token.eof).unwrap();
        assert_eq!(eof.span, Some(source.len()..source.len()));
        assert_eq!(eof.status, SpanStatus::Eof);
    }
    let source = "\u{feff}SELECT '🙂α' AS value;\r\n🙂";
    let syntax = antlr_engine::parse(Dialect::TSql, source, &operation_context()).unwrap();
    let diagnostic = syntax
        .diagnostics
        .iter()
        .find(|item| item.phase == DiagnosticPhase::Lexer)
        .unwrap();
    let span = diagnostic.span.clone().unwrap();
    assert_eq!(&source[span], "🙂");
    assert_eq!(diagnostic.line, 2);
    assert_eq!(diagnostic.column, 0);
    assert!(antlr_engine::parse_strict(Dialect::TSql, source, &operation_context()).is_err());
}

#[test]
fn native_operations_preserve_original_core_cancellation_and_deadline() {
    use graphit_ast::antlr_engine::{self, AntlrEngineError};
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(antlr_engine::parse(Dialect::PostgreSql, "SELECT 1;", &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(antlr_engine::parse(Dialect::PostgreSql, "SELECT 1;", &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    let running = operation_context();
    let source = "SELECT 1;\n".repeat(100_000);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(5));
            running.cancellation().cancel();
        });
        assert!(
            matches!(antlr_engine::parse(Dialect::PostgreSql, &source, &running), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
        );
    });
}

#[test]
fn parsed_projection_preserves_compound_pieces_and_input_binding() {
    use graphit_ast::{
        antlr_engine::preprocessing::{CobolFormat, prepare_cobol},
        prepared_input::{OriginError, OriginalProjection, PreparedInput},
    };
    let source = "000100 IDENTIFICATION DIVISION.\n000200 PROGRAM-ID. HELLO.\n000300 PROCEDURE DIVISION.\n000400     DISPLAY 'HEL\n000500-    'LO'.\n000600     STOP RUN.\n";
    let operation = operation_context();
    let prepared = prepare_cobol(source, CobolFormat::Fixed, &operation).unwrap();
    let syntax =
        graphit_ast::antlr_engine::parse_strict(Dialect::Cobol85, prepared.text(), &operation)
            .unwrap();
    prepared.with_origin_plan(|plan| {
        let input = PreparedInput::new(source.as_bytes(), prepared.text().as_bytes(), plan, &operation).unwrap();
        let projected = syntax.project(&input).unwrap();
        let literal = syntax.tokens.iter().position(|token| token.text.as_deref() == Some("'HELLO'")).unwrap();
        assert!(matches!(&projected.tokens[literal], Some(OriginalProjection::Compound(pieces)) if pieces.len() >= 2));
        let other = prepared.text().to_owned();
        let unrelated = graphit_ast::antlr_engine::parse_strict(Dialect::Cobol85, &other, &operation).unwrap();
        assert!(matches!(unrelated.project(&input), Err(OriginError::InvalidPlan)));
    });
}

#[test]
fn plpgsql_sdk_progress_observes_original_core_operation() {
    use graphit_ast::tree_sitter_engine::{EngineError, ParseControl};
    let language = plpgsql_fixture();
    let source = format!("BEGIN {} END;", "PERFORM 1;\n".repeat(100_000));
    let operation = operation_context();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(5));
            operation.cancellation().cancel();
        });
        assert!(
            matches!(language.parse_with_context(&source, ParseControl::default(), &operation), Err(EngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
        );
    });
}

#[test]
fn postgres_local_unicode_letter_predicate_and_tags_use_real_scalars() {
    for identifier in ["é", "α", "中", "𐐀"] {
        let source = format!("SELECT {identifier} FROM t; -- comentário 🙂\n");
        let syntax = parse(Dialect::PostgreSql, &source).unwrap();
        let token = syntax
            .tokens
            .iter()
            .find(|token| token.text.as_deref() == Some(identifier))
            .unwrap();
        assert_eq!(&source[token.span.clone().unwrap()], identifier);
    }
    let source = "CREATE FUNCTION f() RETURNS void AS $𐐀$BEGIN RETURN; END;$𐐀$ LANGUAGE plpgsql;";
    let syntax = parse(Dialect::PostgreSql, source).unwrap();
    assert_eq!(
        &source[syntax.embedded_bodies[0].clone()],
        "BEGIN RETURN; END;"
    );
    let source = "SELECT 🙂 FROM t;";
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::PostgreSql, source, &operation_context())
            .unwrap();
    assert!(!recovered.diagnostics.is_empty());
    assert!(parse(Dialect::PostgreSql, source).is_err());
}

#[test]
fn postgres_splice_context_preserves_original_cause_before_optional_control() {
    use graphit_ast::{
        antlr_engine::{AntlrEngineError, EmbeddedError, parse_postgres_with_plpgsql},
        tree_sitter_engine::{EngineError, ParseControl},
    };
    let language = plpgsql_fixture();
    let source = "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$BEGIN RETURN; END;$$;";
    let context = operation_context();
    let syntax =
        parse_postgres_with_plpgsql(source, &language, ParseControl::default(), &context).unwrap();
    let query = language.compile_query("(identifier) @name").unwrap();
    assert!(
        syntax.bodies[0]
            .matches_with_context(&query, ParseControl::default(), &context)
            .unwrap()
            .is_empty()
    );
    context.cancellation().cancel();
    let optional_expired = ParseControl {
        cancellation: None,
        deadline: Some(std::time::Instant::now()),
    };
    assert!(
        matches!(parse_postgres_with_plpgsql(source, &language, optional_expired, &context), Err(EmbeddedError::Host(AntlrEngineError::Operation(error))) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    for query_projection in [true, false] {
        let error = if query_projection {
            syntax.bodies[0]
                .matches_with_context(&query, optional_expired, &context)
                .map(|_| ())
        } else {
            syntax.bodies[0]
                .nodes_with_context(optional_expired, &context)
                .map(|_| ())
        }
        .unwrap_err();
        assert!(
            matches!(&error, EmbeddedError::TreeSitter(EngineError::Operation(cause)) if cause.kind() == std::io::ErrorKind::Interrupted)
        );
        assert!(
            std::error::Error::source(&error)
                .unwrap()
                .is::<EngineError>()
        );
    }
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(syntax.bodies[0].nodes_with_context(ParseControl::default(), &expired), Err(EmbeddedError::TreeSitter(EngineError::Operation(error))) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    assert!(
        matches!(syntax.bodies[0].matches_with_context(&query, ParseControl::default(), &expired), Err(EmbeddedError::TreeSitter(EngineError::Operation(error))) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn db2_method_parameters_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let queries: Vec<_> = [
        "//method_signature/method_param_list/method_signature_parameter_decl/parameter_name",
        "//method_specification/method_spec_parameter_list_paren/method_spec_parameter_list/method_spec_parameter_decl/parameter_name",
    ].into_iter().map(|pattern| ProfileQuery {
        pattern: pattern.into(), captures: vec![Vec::new()],
        ..ProfileQuery::default()
    }).collect();
    let signature = |params: &str| {
        format!(
            "CREATE METHOD METHOD_F({params}) RETURNS INTEGER FOR TYPE_F EXTERNAL NAME 'pkg.method';"
        )
    };
    let specification = |params: &str| {
        format!(
            "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F({params}) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL NOT FENCED;"
        )
    };
    for (source, expected, query_index) in [
        (signature("P_BOOL BOOLEAN,P_TEXT VARCHAR(40),P_REF REF(APP.TYPE_F),CLOB AS LOCATOR"), vec!["P_BOOL", "P_TEXT", "P_REF"], 0),
        ("CREATE METHOD METHOD_SQL_F(P_METHOD INTEGER,P_TEXT VARCHAR(40)) RETURNS INTEGER FOR TYPE_F INHERIT ISOLATION LEVEL WITHOUT LOCK REQUEST BEGIN ATOMIC RETURN P_METHOD; END;".into(), vec!["P_METHOD", "P_TEXT"], 0),
        ("CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(P_SPEC BOOLEAN,P_SPEC_2 VARCHAR(40),P_REF REF(APP.OTHER_TYPE_F)) RETURNS INTEGER LANGUAGE SQL;".into(), vec!["P_SPEC", "P_SPEC_2", "P_REF"], 1),
        (specification("P_F,INTEGER,CLOB AS LOCATOR"), vec![], 1),
        (signature(""), vec![], 0),
        (specification(""), vec![], 1),
        (signature("p_raw APP.\"Té\""), vec!["p_raw"], 0),
        (specification("\"Pé\" REF(\"Schéma\".\"Té\"),\"XML\""), vec!["\"Pé\""], 1),
        (signature("P_F,ARRAY_F,CURSOR_F"), vec![], 0),
        ("CREATE METHOD METHOD_F() FOR TYPE_F EXTERNAL NAME 'pkg.method';".into(), vec![], 0),
    ] {
        let source = format!("-- α\r\n/* β /* γ */ δ */ {source}");
        let result = parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        assert!(result.syntax.diagnostics.is_empty(), "{source}: {:?}", result.syntax.diagnostics);
        let names: Vec<_> = result.matches.iter().map(|matched| {
            assert_eq!(matched.query_index, query_index);
            assert_eq!(matched.captures, [Some(matched.node_index)]);
            let node = &result.syntax.nodes[matched.node_index];
            assert_eq!(node.kind, "parameter_name");
            assert_eq!(result.syntax.nodes[node.parent.unwrap()].kind, if query_index == 0 { "method_signature_parameter_decl" } else { "method_spec_parameter_decl" });
            source.get(node.span.clone().unwrap()).unwrap()
        }).collect();
        assert_eq!(names, expected, "{source}");
        assert_eq!(result.syntax.nodes.iter().filter(|node| node.kind == if query_index == 0 { "method_signature" } else { "method_specification" }).count(), 1);
        assert!(!result.syntax.nodes.iter().any(|node| node.kind == "param_decl_list_paren"));
        for (index, node) in result.syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span { assert!(source.get(span.clone()).is_some()); }
        }
        for token in &result.syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
        for kind in ["SQL_COMMENT", "LINE_COMMENT"] {
            assert!(result.syntax.tokens.iter().any(|token| token.kind == kind && token.channel != 0));
        }
    }
    let types = [
        "SMALLINT",
        "INT",
        "INTEGER",
        "BIGINT",
        "DEC",
        "DEC(8,0)",
        "DECIMAL(1,0)",
        "NUM(8)",
        "NUMERIC",
        "DECFLOAT",
        "DECFLOAT(34)",
        "FLOAT",
        "FLOAT(1)",
        "REAL",
        "DOUBLE",
        "DOUBLE PRECISION",
        "CHAR",
        "CHAR(1 OCTETS) FOR BIT DATA",
        "CHARACTER(8 CODEUNITS32)",
        "VARCHAR(1)",
        "CHAR VARYING(40 OCTETS) FOR BIT DATA",
        "CHARACTER VARYING(40 CODEUNITS32)",
        "CLOB",
        "CLOB(1 K OCTETS)",
        "CHAR LARGE OBJECT(1 M)",
        "CHARACTER LARGE OBJECT(1 G CODEUNITS32)",
        "GRAPHIC",
        "GRAPHIC(1 CODEUNITS16)",
        "VARGRAPHIC(1 CODEUNITS32)",
        "DBCLOB",
        "DBCLOB(1 K CODEUNITS16)",
        "NCHAR",
        "NCHAR(1)",
        "NATIONAL CHAR",
        "NATIONAL CHARACTER(1)",
        "NVARCHAR(1)",
        "NCHAR VARYING(1)",
        "NATIONAL CHAR VARYING(1)",
        "NATIONAL CHARACTER VARYING(1)",
        "NCLOB",
        "NCLOB(1 K)",
        "NCHAR LARGE OBJECT(1 M)",
        "NATIONAL CHARACTER LARGE OBJECT(1 G)",
        "BINARY",
        "BINARY(1)",
        "VARBINARY(1)",
        "BINARY VARYING(1)",
        "BLOB",
        "BLOB(1 K)",
        "BINARY LARGE OBJECT(1 M)",
        "DATE",
        "TIME",
        "TIMESTAMP",
        "TIMESTAMP(0)",
        "BOOLEAN",
        "DB2SECURITYLABEL",
        "SYSPROC.DB2SECURITYLABEL",
        "APP.TYPE_F",
        "REF(TYPE_F)",
        "REF(APP.TYPE_F)",
    ];
    for data_type in types {
        for source in [
            signature(&format!("P_F {data_type}")),
            specification(&format!("P_F {data_type}")),
        ] {
            let syntax =
                parse(Dialect::Db2, &source).unwrap_or_else(|error| panic!("{source}: {error}"));
            let kind = if source.starts_with("CREATE METHOD") {
                "method_parameter_type"
            } else {
                "method_spec_parameter_type"
            };
            let node = syntax.nodes.iter().find(|node| node.kind == kind).unwrap();
            assert_eq!(source.get(node.span.clone().unwrap()), Some(data_type));
        }
    }
    assert!(parse(Dialect::Db2, &signature("P_XML XML")).is_ok());
    for name in [
        "DB2SECURITYLABEL",
        "db2securitylabel",
        "SYSPROC",
        "sysproc",
        "\"SYSPROC\"",
        "APP.SYSPROC",
    ] {
        let source = format!("CREATE VARIABLE {name} INTEGER DEFAULT 1;");
        let syntax = parse(Dialect::Db2, &source).unwrap();
        let kind = if name.contains('.') {
            "schema_qualified_variable_declaration_name"
        } else {
            "variable_name"
        };
        assert!(
            syntax
                .nodes
                .iter()
                .any(|node| node.kind == kind
                    && source.get(node.span.clone().unwrap()) == Some(name))
        );
    }
    let cursor = "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);";
    let result =
        parse_profile_queries(Dialect::Db2, cursor, &queries, &operation_context()).unwrap();
    assert!(result.syntax.diagnostics.is_empty());
    assert!(result.matches.is_empty());
    assert!(
        result
            .syntax
            .nodes
            .iter()
            .any(|node| node.kind == "param_decl"
                && cursor.get(node.span.clone().unwrap()) == Some("P_CURSOR INTEGER"))
    );
    assert!(
        !result
            .syntax
            .nodes
            .iter()
            .any(|node| node.kind.starts_with("method_spec_parameter"))
    );
    for params in [
        "IN P_F INTEGER",
        "OUT P_F INTEGER",
        "INOUT P_F INTEGER",
        "P_F INTEGER DEFAULT 1",
        "P_F INTEGER AS LOCATOR AS LOCATOR",
        "AS LOCATOR P_F INTEGER",
        "P_F REF()",
        "P_F VARCHAR",
        "P_F VARCHAR()",
        "P_F FLOAT()",
        "P_F ANCHOR DATA TYPE TO V_F",
        "P_F ROW OF TABLE_F",
        "P_F CURSOR",
        "P_F VARGRAPHIC",
        "P_F NVARCHAR",
    ] {
        for source in [signature(params), specification(params)] {
            assert!(
                matches!(
                    parse(Dialect::Db2, &source),
                    Err(AntlrEngineError::Syntax { .. })
                ),
                "accepted {source}"
            );
        }
    }
    assert!(matches!(
        parse(Dialect::Db2, &specification("P_XML XML")),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let broken = format!(
        "-- α\r\n{} CREATE MODULE AFTER_METHOD;",
        signature("P_F INTEGER DEFAULT 1")
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    std::thread::scope(|scope| {
        let handles: Vec<_> = ["BOOLEAN", "VARCHAR(1)", "REF(APP.TYPE_F)", "XML"].into_iter().map(|data_type| scope.spawn(move || {
            let source = format!("CREATE METHOD METHOD_F(P_F {data_type}) RETURNS INTEGER FOR TYPE_F EXTERNAL NAME 'pkg.method';");
            for _ in 0..4 { assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty()); }
        })).collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
    let source = signature("P_F BOOLEAN");
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn db2_routine_defaults_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    let scalar = |value: &str| {
        format!(
            "-- α\r\nCREATE FUNCTION FUNC_SCALAR_F(IN \"Pé\" INTEGER DEFAULT /* β /* γ */ δ */ {value}) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.scalar' NO SQL;"
        )
    };
    let queries: Vec<_> = [
        "//ext_scalar_param_decl/routine_default_clause/routine_default_value",
        "//ext_scalar_param_decl/parameter_name",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    for (value, child) in [
        ("NULL", "NULL_"),
        ("1", "constant_"),
        ("0", "constant_"),
        ("-1", "constant_"),
        ("+100", "constant_"),
        ("15.2", "constant_"),
        ("+5.E+2", "constant_"),
        ("INF", "constant_"),
        ("-INFINITY", "constant_"),
        ("NAN", "constant_"),
        ("SNAN", "constant_"),
        ("TRUE", "constant_"),
        ("FALSE", "constant_"),
        ("DATE '2026-10-11'", "constant_"),
        ("TIME '12:34:56'", "constant_"),
        ("TIMESTAMP '2026-10-11 12:34:56'", "constant_"),
        ("'a''β'", "constant_"),
        ("G'α''β'", "constant_"),
        ("N'α'", "constant_"),
        ("GX'0041abcd'", "constant_"),
        ("BX'41aB'", "constant_"),
        ("UX'0041'", "constant_"),
        ("U&'a\\0041'", "constant_"),
        ("U&'a!0041' UESCAPE '!'", "constant_"),
        ("CURRENT DATE", "routine_default_special_register"),
        ("CURRENT TIME", "routine_default_special_register"),
        ("CURRENT USER", "routine_default_special_register"),
        ("CURRENT SCHEMA", "routine_default_special_register"),
        ("CURRENT MEMBER", "routine_default_special_register"),
        ("CURRENT SQLID", "routine_default_special_register"),
        ("USER", "routine_default_special_register"),
        ("SESSION_USER", "routine_default_special_register"),
        ("SYSTEM_USER", "routine_default_special_register"),
        ("CURRENT_DATE", "routine_default_special_register"),
        ("CURRENT_TIME", "routine_default_special_register"),
        ("CURRENT_USER", "routine_default_special_register"),
        ("CURRENT_SCHEMA", "routine_default_special_register"),
        ("CURRENT_SQLID", "routine_default_special_register"),
        ("SYSDATE", "routine_default_special_register"),
        ("CURRENT TIMESTAMP", "routine_default_special_register"),
        ("CURRENT TIMESTAMP()", "routine_default_special_register"),
        ("CURRENT TIMESTAMP(0)", "routine_default_special_register"),
        ("CURRENT TIMESTAMP(1)", "routine_default_special_register"),
        ("CURRENT_TIMESTAMP(6)", "routine_default_special_register"),
        ("CURRENT_TIMESTAMP()", "routine_default_special_register"),
        ("LOCALTIMESTAMP", "routine_default_special_register"),
        ("LOCALTIMESTAMP()", "routine_default_special_register"),
        ("LOCALTIMESTAMP(13)", "routine_default_special_register"),
        ("(1+2)", "expression"),
        ("(CURRENT DEGREE)", "expression"),
        ("(CURRENT TIMESTAMP(2))", "expression"),
        ("V_F", "global_variable"),
        ("APP.V_F", "global_variable"),
        ("SERVER_F.APP.V_F", "global_variable"),
        ("\"Schéma\".\"Vé\"", "global_variable"),
        ("CURRENT_MEMBER", "global_variable"),
        ("\"UNKNOWN\"", "global_variable"),
    ] {
        let source = scalar(value);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        let values: Vec<_> = result
            .matches
            .iter()
            .filter(|matched| matched.query_index == 0)
            .collect();
        assert_eq!(values.len(), 1, "{source}");
        let selected = values[0];
        assert_eq!(selected.captures, [Some(selected.node_index)]);
        let node = &syntax.nodes[selected.node_index];
        assert_eq!(node.kind, "routine_default_value");
        assert_eq!(source.get(node.span.clone().unwrap()), Some(value));
        let clause_index = node.parent.unwrap();
        let clause = &syntax.nodes[clause_index];
        assert_eq!(clause.kind, "routine_default_clause");
        assert_eq!(
            syntax.nodes[clause.parent.unwrap()].kind,
            "ext_scalar_param_decl"
        );
        assert!(
            source
                .get(clause.span.clone().unwrap())
                .unwrap()
                .starts_with("DEFAULT /* β")
        );
        assert!(
            syntax
                .nodes
                .iter()
                .any(|candidate| candidate.parent == Some(selected.node_index)
                    && candidate.kind == child),
            "{value}: missing {child}"
        );
        let names: Vec<_> = result
            .matches
            .iter()
            .filter(|matched| matched.query_index == 1)
            .collect();
        assert_eq!(names.len(), 1);
        assert_eq!(
            source.get(syntax.nodes[names[0].node_index].span.clone().unwrap()),
            Some("\"Pé\"")
        );
        assert!(
            !syntax
                .nodes
                .iter()
                .any(|node| node.kind == "function_invocation")
        );
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == kind)
                .unwrap();
            assert_ne!(token.channel, 0);
            assert_eq!(
                source.get(token.span.clone().unwrap()),
                token.text.as_deref()
            );
        }
        for token in &syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
    }
    let unnamed = "CREATE FUNCTION FUNC_SCALAR_F(INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.scalar' NO SQL;";
    let result =
        parse_profile_queries(Dialect::Db2, unnamed, &queries, &operation_context()).unwrap();
    assert!(result.syntax.diagnostics.is_empty());
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].query_index, 0);
    for source in [
        "CREATE VARIABLE V_F INTEGER DEFAULT 1;",
        "ALTER TABLE TABLE_F ADD COLUMN COL_F INTEGER WITH DEFAULT;",
        "CREATE PROCEDURE PROC_F(IN P_F INTEGER) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F = P_F; END;",
        "CREATE FUNCTION FUNC_TABLE_F(P_F INTEGER DEFAULT 1) RETURNS TABLE(COL_F INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.table' NO SQL;",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let syntax =
            parse(Dialect::Db2, source).unwrap_or_else(|error| panic!("{source}: {error}"));
        if source.starts_with("CREATE FUNCTION FUNC_TABLE_F") {
            assert_eq!(
                syntax
                    .nodes
                    .iter()
                    .filter(|node| node.kind == "routine_default_clause")
                    .count(),
                1
            );
            assert!(
                !syntax
                    .nodes
                    .iter()
                    .any(|node| node.kind == "default_clause")
            );
        } else {
            assert!(
                !syntax
                    .nodes
                    .iter()
                    .any(|node| node.kind.starts_with("routine_default")),
                "{source}"
            );
            if source.starts_with("CREATE PROCEDURE") {
                let (local_index, local) = syntax
                    .nodes
                    .iter()
                    .enumerate()
                    .find(|(_, node)| node.kind == "declare_variable_default_clause")
                    .unwrap();
                assert_eq!(
                    syntax.nodes[local.parent.unwrap()].kind,
                    "declare_variable_statement"
                );
                assert_eq!(source.get(local.span.clone().unwrap()), Some("DEFAULT 1"));
                let value = syntax
                    .nodes
                    .iter()
                    .find(|node| node.parent == Some(local_index) && node.kind == "default_values")
                    .unwrap();
                assert_eq!(source.get(value.span.clone().unwrap()), Some("1"));
                let token = syntax
                    .tokens
                    .iter()
                    .find(|token| token.kind == "L_ONE" && token.span == value.span)
                    .unwrap();
                assert_eq!(token.text.as_deref(), Some("1"));
                assert_eq!(token.status, SpanStatus::Regular);
            } else if source.starts_with("ALTER TABLE") || source.starts_with("CREATE FUNCTION") {
                assert!(
                    syntax
                        .nodes
                        .iter()
                        .any(|node| node.kind == "default_clause")
                );
            }
        }
    }
    for value in [
        "",
        "1+2",
        "+TRUE",
        "CURRENT DEGREE",
        "UNKNOWN",
        "CAST(1)",
        "EMPTY_CLOB()",
        "1 DEFAULT 2",
        "(1+2",
        "CURRENT TIMESTAMP(1,2)",
        "CURRENT_TIMESTAMP(-1)",
    ] {
        let source = scalar(value);
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {source}"
        );
    }
    let with_default = scalar("1").replace("INTEGER DEFAULT", "INTEGER WITH DEFAULT");
    assert!(matches!(
        parse(Dialect::Db2, &with_default),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let broken = format!("{} CREATE MODULE AFTER_DEFAULT;", scalar(""));
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    std::thread::scope(|scope| {
        let handles: Vec<_> = ["CURRENT TIMESTAMP(0)", "LOCALTIMESTAMP()", "APP.V_F", "U&'a!0041' UESCAPE '!'"]
            .into_iter().map(|value| scope.spawn(move || {
                let source = format!("CREATE FUNCTION F(P_F INTEGER DEFAULT {value}) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.f' NO SQL;");
                for _ in 0..4 { assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty()); }
            })).collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
    let source = scalar("1");
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn db2_procedure_parameter_lists_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    let external = |list: &str| {
        format!(
            "CREATE PROCEDURE \"Schéma\".PROC_F{list} LANGUAGE C PARAMETER STYLE SQL NO SQL EXTERNAL NAME 'pkg.proc';"
        )
    };
    let sql = |list: &str| {
        format!(
            "CREATE PROCEDURE SCHEMA_F.PROC_F{list} LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER; SET LOCAL_F=1; END;"
        )
    };
    let queries: Vec<_> = [
        "//proc_ext_param/parameter_name",
        "//proc_parameter_list_item/parameter_name",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    for (statement, expected, query_index, declaration_count, list_present) in [
        (
            external("(P_IN INTEGER,OUT P_OUT VARCHAR(20),INOUT P_IO INTEGER)"),
            vec!["P_IN", "P_OUT", "P_IO"],
            0,
            3,
            true,
        ),
        (
            external("(IN \"Pé\" INTEGER,INTEGER,OUT VARCHAR(20),INOUT INTEGER)"),
            vec!["\"Pé\""],
            0,
            4,
            true,
        ),
        (external("(INTEGER,OUT VARCHAR(20))"), vec![], 0, 2, true),
        (external("(P_F)"), vec![], 0, 1, true),
        (external("()"), vec![], 0, 0, true),
        (external(""), vec![], 0, 0, false),
        (external("(P_F INTEGER DEFAULT 1)"), vec!["P_F"], 0, 1, true),
        (
            sql("(P_IN INTEGER,OUT P_OUT INTEGER,INOUT P_IO INTEGER)"),
            vec!["P_IN", "P_OUT", "P_IO"],
            1,
            3,
            true,
        ),
        (sql("(IN \"Pé\" INTEGER)"), vec!["\"Pé\""], 1, 1, true),
        (sql("()"), vec![], 1, 0, false),
        (sql(""), vec![], 1, 0, false),
    ] {
        let source = format!("-- α\r\n/* β /* γ */ δ */ {statement}");
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        let names: Vec<_> = result
            .matches
            .iter()
            .map(|matched| {
                assert_eq!(matched.query_index, query_index);
                assert_eq!(matched.captures, [Some(matched.node_index)]);
                let node = &syntax.nodes[matched.node_index];
                assert_eq!(node.kind, "parameter_name");
                assert_eq!(
                    syntax.nodes[node.parent.unwrap()].kind,
                    if query_index == 0 {
                        "proc_ext_param"
                    } else {
                        "proc_parameter_list_item"
                    }
                );
                source.get(node.span.clone().unwrap()).unwrap()
            })
            .collect();
        assert_eq!(names, expected, "{source}");
        let family = if query_index == 0 {
            "create_procedure_external_statement"
        } else {
            "create_procedure_sql_statement"
        };
        let declarations = if query_index == 0 {
            "proc_ext_param"
        } else {
            "proc_parameter_list_item"
        };
        let list_kind = if query_index == 0 {
            "proc_ext_param_list"
        } else {
            "proc_parameter_list"
        };
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == declarations)
                .count(),
            declaration_count
        );
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == family)
                .count(),
            1
        );
        let wrappers: Vec<_> = syntax
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == "create_procedure_statement")
            .collect();
        assert_eq!(wrappers.len(), 1);
        assert!(
            syntax
                .nodes
                .iter()
                .any(|node| node.kind == family && node.parent == Some(wrappers[0].0))
        );
        let lists: Vec<_> = syntax
            .nodes
            .iter()
            .filter(|node| node.kind == list_kind)
            .collect();
        assert_eq!(lists.len(), usize::from(list_present));
        if query_index == 0 && declaration_count == 0 && list_present {
            assert_eq!(source.get(lists[0].span.clone().unwrap()), Some("()"));
        }
        if source.contains("DEFAULT 1") {
            for kind in ["routine_default_clause", "routine_default_value"] {
                assert_eq!(
                    syntax.nodes.iter().filter(|node| node.kind == kind).count(),
                    1
                );
            }
            assert!(
                !syntax
                    .nodes
                    .iter()
                    .any(|node| node.kind == "default_clause")
            );
        } else {
            assert!(
                !syntax
                    .nodes
                    .iter()
                    .any(|node| node.kind.starts_with("routine_default"))
            );
        }
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == kind)
                .unwrap();
            assert_ne!(token.channel, 0);
            assert_eq!(
                source.get(token.span.clone().unwrap()),
                token.text.as_deref()
            );
        }
        for token in &syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
    }
    for source in [
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
        "CREATE METHOD METHOD_F(P_METHOD INTEGER) RETURNS INTEGER FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CREATE FUNCTION F(P_FUNCTION INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.f' NO SQL;",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
    }
    for statement in [
        sql("(INTEGER)"),
        external("(IN)"),
        external("(P_F INTEGER,)"),
        external("(,P_F INTEGER)"),
        external("(IN OUT P_F INTEGER)"),
        external("(P_F INTEGER OUT)"),
        external("(P_F INTEGER"),
    ] {
        assert!(
            matches!(
                parse(Dialect::Db2, &statement),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {statement}"
        );
    }
    let broken = format!(
        "-- α\r\n{} CREATE MODULE AFTER_PROC;",
        external("(P_F INTEGER,)")
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    std::thread::scope(|scope| {
        let handles: Vec<_> = ["()", "", "(P_F INTEGER)", "(INTEGER,OUT VARCHAR(20))"]
            .into_iter().map(|list| scope.spawn(move || {
                let source = format!("CREATE PROCEDURE PROC_F{list} LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.proc';");
                for _ in 0..4 { assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty()); }
            })).collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
    let source = external("()");
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn db2_procedure_defaults_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    let procedure = |value: &str, sql_family: bool| {
        let tail = if sql_family {
            "LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER; SET LOCAL_F=1; END;"
        } else {
            "LANGUAGE C PARAMETER STYLE SQL NO SQL EXTERNAL NAME 'pkg.proc';"
        };
        format!(
            "-- α\r\nCREATE PROCEDURE SCHEMA_F.PROC_F(IN \"Pé\" INTEGER DEFAULT /* β /* γ */ δ */ {value}) {tail}"
        )
    };
    let queries: Vec<_> = [
        "//proc_ext_param/routine_default_clause/routine_default_value",
        "//proc_parameter_list_item/routine_default_clause/routine_default_value",
        "//proc_ext_param/parameter_name",
        "//proc_parameter_list_item/parameter_name",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    for sql_family in [false, true] {
        for (value, child) in [
            ("NULL", "NULL_"),
            ("1", "constant_"),
            ("0", "constant_"),
            ("-1", "constant_"),
            ("+100", "constant_"),
            ("15.2", "constant_"),
            ("+5.E+2", "constant_"),
            ("INF", "constant_"),
            ("-INFINITY", "constant_"),
            ("NAN", "constant_"),
            ("SNAN", "constant_"),
            ("TRUE", "constant_"),
            ("FALSE", "constant_"),
            ("DATE '2026-10-11'", "constant_"),
            ("TIME '12:34:56'", "constant_"),
            ("TIMESTAMP '2026-10-11 12:34:56'", "constant_"),
            ("'a''β'", "constant_"),
            ("G'α''β'", "constant_"),
            ("N'α'", "constant_"),
            ("GX'0041abcd'", "constant_"),
            ("BX'41aB'", "constant_"),
            ("UX'0041'", "constant_"),
            ("U&'a\\0041'", "constant_"),
            ("U&'a!0041' UESCAPE '!'", "constant_"),
            ("CURRENT DATE", "routine_default_special_register"),
            ("CURRENT TIME", "routine_default_special_register"),
            ("CURRENT USER", "routine_default_special_register"),
            ("CURRENT SCHEMA", "routine_default_special_register"),
            ("CURRENT MEMBER", "routine_default_special_register"),
            ("CURRENT SQLID", "routine_default_special_register"),
            ("USER", "routine_default_special_register"),
            ("SESSION_USER", "routine_default_special_register"),
            ("SYSTEM_USER", "routine_default_special_register"),
            ("CURRENT_DATE", "routine_default_special_register"),
            ("CURRENT_TIME", "routine_default_special_register"),
            ("CURRENT_USER", "routine_default_special_register"),
            ("CURRENT_SCHEMA", "routine_default_special_register"),
            ("CURRENT_SQLID", "routine_default_special_register"),
            ("SYSDATE", "routine_default_special_register"),
            ("CURRENT TIMESTAMP", "routine_default_special_register"),
            ("CURRENT TIMESTAMP()", "routine_default_special_register"),
            ("CURRENT TIMESTAMP(0)", "routine_default_special_register"),
            ("CURRENT TIMESTAMP(1)", "routine_default_special_register"),
            ("CURRENT_TIMESTAMP(6)", "routine_default_special_register"),
            ("CURRENT_TIMESTAMP()", "routine_default_special_register"),
            ("LOCALTIMESTAMP", "routine_default_special_register"),
            ("LOCALTIMESTAMP()", "routine_default_special_register"),
            ("LOCALTIMESTAMP(13)", "routine_default_special_register"),
            ("(1+2)", "expression"),
            ("(CURRENT DEGREE)", "expression"),
            ("(CURRENT TIMESTAMP(2))", "expression"),
            ("V_F", "global_variable"),
            ("APP.V_F", "global_variable"),
            ("SERVER_F.APP.V_F", "global_variable"),
            ("\"Schéma\".\"Vé\"", "global_variable"),
            ("CURRENT_MEMBER", "global_variable"),
            ("\"UNKNOWN\"", "global_variable"),
        ] {
            let source = procedure(value, sql_family);
            let result =
                parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                    .unwrap();
            let syntax = &result.syntax;
            assert!(
                syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                syntax.diagnostics
            );
            let values: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == usize::from(sql_family))
                .collect();
            assert_eq!(values.len(), 1, "{source}");
            let selected = values[0];
            assert_eq!(selected.captures, [Some(selected.node_index)]);
            let node = &syntax.nodes[selected.node_index];
            assert_eq!(node.kind, "routine_default_value");
            assert_eq!(source.get(node.span.clone().unwrap()), Some(value));
            let clause_index = node.parent.unwrap();
            let clause = &syntax.nodes[clause_index];
            assert_eq!(clause.kind, "routine_default_clause");
            assert_eq!(
                syntax.nodes[clause.parent.unwrap()].kind,
                if sql_family {
                    "proc_parameter_list_item"
                } else {
                    "proc_ext_param"
                }
            );
            assert!(
                source
                    .get(clause.span.clone().unwrap())
                    .unwrap()
                    .starts_with("DEFAULT /* β")
            );
            assert!(
                syntax
                    .nodes
                    .iter()
                    .any(|candidate| candidate.parent == Some(selected.node_index)
                        && candidate.kind == child),
                "{value}: missing {child}"
            );
            let names: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == usize::from(sql_family) + 2)
                .collect();
            assert_eq!(names.len(), 1);
            assert_eq!(
                source.get(syntax.nodes[names[0].node_index].span.clone().unwrap()),
                Some("\"Pé\"")
            );
            assert!(
                !syntax
                    .nodes
                    .iter()
                    .any(|node| node.kind == "function_invocation")
            );
            assert_eq!(
                syntax
                    .nodes
                    .iter()
                    .filter(|node| node.kind == "create_procedure_statement")
                    .count(),
                1
            );
            assert!(
                !syntax
                    .nodes
                    .iter()
                    .any(|node| node.kind == "default_clause")
            );
            assert_eq!(result.matches.len(), 2);
            for (index, node) in syntax.nodes.iter().enumerate() {
                assert!(!node.error && !node.missing);
                assert!(node.parent.is_none_or(|parent| parent < index));
                if let Some(span) = &node.span {
                    assert!(source.get(span.clone()).is_some());
                }
            }
            for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
                let token = syntax
                    .tokens
                    .iter()
                    .find(|token| token.kind == kind)
                    .unwrap();
                assert_ne!(token.channel, 0);
                assert_eq!(
                    source.get(token.span.clone().unwrap()),
                    token.text.as_deref()
                );
            }
            for token in &syntax.tokens {
                if token.eof {
                    assert_eq!(token.status, SpanStatus::Eof);
                    assert_eq!(token.span, Some(source.len()..source.len()));
                } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                    assert_eq!(source.get(span.clone()), Some(text.as_str()));
                }
            }
        }
    }
    let unnamed = "CREATE PROCEDURE PROC_F(INTEGER DEFAULT 1) LANGUAGE C PARAMETER STYLE SQL NO SQL EXTERNAL NAME 'pkg.proc';";
    let result =
        parse_profile_queries(Dialect::Db2, unnamed, &queries, &operation_context()).unwrap();
    assert!(result.syntax.diagnostics.is_empty());
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].query_index, 0);
    for sql_family in [false, true] {
        let tail = if sql_family {
            "LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER; SET LOCAL_F=1; END;"
        } else {
            "LANGUAGE C PARAMETER STYLE SQL NO SQL EXTERNAL NAME 'pkg.proc';"
        };
        let source = format!(
            "CREATE PROCEDURE PROC_F(P_A INTEGER DEFAULT -1, IN P_B DATE DEFAULT CURRENT DATE, P_C INTEGER DEFAULT (1+2)) {tail}"
        );
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        let raw_matches = |query_index| {
            result
                .matches
                .iter()
                .filter(|matched| matched.query_index == query_index)
                .map(|matched| {
                    assert_eq!(matched.captures, [Some(matched.node_index)]);
                    source
                        .get(
                            result.syntax.nodes[matched.node_index]
                                .span
                                .clone()
                                .unwrap(),
                        )
                        .unwrap()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            raw_matches(usize::from(sql_family)),
            ["-1", "CURRENT DATE", "(1+2)"]
        );
        assert_eq!(
            raw_matches(usize::from(sql_family) + 2),
            ["P_A", "P_B", "P_C"]
        );
        assert_eq!(result.matches.len(), 6);
        assert_eq!(
            result
                .syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "routine_default_clause")
                .count(),
            3
        );
    }
    for source in [
        "ALTER TABLE TABLE_F ADD COLUMN COL_F INTEGER WITH DEFAULT;",
        "CREATE PROCEDURE PROC_F(IN P_F INTEGER) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F = P_F; END;",
        "CREATE FUNCTION FUNC_TABLE_F(P_F INTEGER DEFAULT 1) RETURNS TABLE(COL_F INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.table' NO SQL;",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
        "CREATE METHOD METHOD_F(P_METHOD INTEGER) RETURNS INTEGER FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CREATE FUNCTION F(P_FUNCTION INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.f' NO SQL;",
        "CREATE FUNCTION F_SOURCE(P_F INTEGER DEFAULT 1) RETURNS INTEGER SOURCE APP.F_TARGET;",
        "CREATE FUNCTION F_TEMPLATE(P_F INTEGER DEFAULT 1) RETURNS INTEGER AS TEMPLATE;",
        "CREATE FUNCTION F_SQL(P_F INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN P_F; END;",
        "CREATE PROCEDURE PROC_SOURCE SOURCE APP.PROC_TARGET FOR SERVER SERVER_F;",
        "CALL PROC_F(P_IN => 1);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(
            !result.matches.iter().any(|matched| matched.query_index < 2),
            "{source}"
        );
        if source.starts_with("CREATE PROCEDURE PROC_F(") {
            assert_eq!(result.matches.len(), 1);
            assert_eq!(result.matches[0].query_index, 3);
            assert!(
                !result
                    .syntax
                    .nodes
                    .iter()
                    .any(|node| node.kind.starts_with("routine_default"))
            );
            let (local_index, local) = result
                .syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, node)| node.kind == "declare_variable_default_clause")
                .unwrap();
            assert_eq!(
                result.syntax.nodes[local.parent.unwrap()].kind,
                "declare_variable_statement"
            );
            assert_eq!(source.get(local.span.clone().unwrap()), Some("DEFAULT 1"));
            let value = result
                .syntax
                .nodes
                .iter()
                .find(|node| node.parent == Some(local_index) && node.kind == "default_values")
                .unwrap();
            assert_eq!(source.get(value.span.clone().unwrap()), Some("1"));
            let token = result
                .syntax
                .tokens
                .iter()
                .find(|token| token.kind == "L_ONE" && token.span == value.span)
                .unwrap();
            assert_eq!(token.text.as_deref(), Some("1"));
            assert_eq!(token.status, SpanStatus::Regular);
        } else {
            assert!(result.matches.is_empty(), "{source}");
            if source.starts_with("CREATE VARIABLE CURSOR_F") {
                assert!(
                    !result
                        .syntax
                        .nodes
                        .iter()
                        .any(|node| node.kind.starts_with("routine_default"))
                );
                assert!(
                    result
                        .syntax
                        .nodes
                        .iter()
                        .any(|node| node.kind == "cursor_value_constructor")
                );
            }
            if source.starts_with("CREATE FUNCTION FUNC_TABLE_F")
                || source.starts_with("CREATE FUNCTION F_SOURCE")
                || source.starts_with("CREATE FUNCTION F_TEMPLATE")
                || source.starts_with("CREATE FUNCTION F_SQL")
            {
                assert_eq!(
                    result
                        .syntax
                        .nodes
                        .iter()
                        .filter(|node| node.kind == "routine_default_clause")
                        .count(),
                    1
                );
                assert!(
                    !result
                        .syntax
                        .nodes
                        .iter()
                        .any(|node| node.kind == "default_clause")
                );
            }
            if source.starts_with("ALTER TABLE") {
                assert!(
                    !result
                        .syntax
                        .nodes
                        .iter()
                        .any(|node| node.kind.starts_with("routine_default")),
                    "{source}"
                );
                assert!(
                    result
                        .syntax
                        .nodes
                        .iter()
                        .any(|node| node.kind == "default_clause"),
                    "{source}"
                );
            }
        }
    }
    for sql_family in [false, true] {
        for value in [
            "",
            "1+2",
            "+TRUE",
            "CURRENT DEGREE",
            "UNKNOWN",
            "CAST(1)",
            "EMPTY_CLOB()",
            "1 DEFAULT 2",
            "(1+2",
            "CURRENT TIMESTAMP(1,2)",
            "CURRENT_TIMESTAMP(-1)",
        ] {
            let source = procedure(value, sql_family);
            assert!(
                matches!(
                    parse(Dialect::Db2, &source),
                    Err(AntlrEngineError::Syntax { .. })
                ),
                "accepted {source}"
            );
        }
        let with_default =
            procedure("1", sql_family).replace("INTEGER DEFAULT", "INTEGER WITH DEFAULT");
        assert!(matches!(
            parse(Dialect::Db2, &with_default),
            Err(AntlrEngineError::Syntax { .. })
        ));
        let broken = format!(
            "{} CREATE MODULE AFTER_PROC_DEFAULT;",
            procedure("", sql_family)
        );
        let recovered =
            graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
        assert!(!recovered.diagnostics.is_empty());
        for diagnostic in &recovered.diagnostics {
            assert_eq!(diagnostic.grammar, "Db2Parser.g4");
            assert!(!diagnostic.message.is_empty());
            assert!(
                diagnostic
                    .span
                    .as_ref()
                    .is_some_and(|span| broken.get(span.clone()).is_some())
            );
            assert!(diagnostic.line >= 2);
        }
        assert!(matches!(
            parse(Dialect::Db2, &broken),
            Err(AntlrEngineError::Syntax { .. })
        ));
        let source = procedure("1", sql_family);
        let cancelled = operation_context();
        cancelled.cancellation().cancel();
        assert!(
            matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
        );
        let expired = graphit_core::file_io::SnapshotContext::new(
            std::time::Instant::now(),
            graphit_core::file_lock::Cancellation::default(),
        );
        assert!(
            matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
        );
    }
    std::thread::scope(|scope| {
        let procedure = &procedure;
        let handles: Vec<_> = [
            ("CURRENT TIMESTAMP(0)", false),
            ("LOCALTIMESTAMP()", true),
            ("APP.V_F", false),
            ("U&'a!0041' UESCAPE '!'", true),
        ]
        .into_iter()
        .map(|(value, sql_family)| {
            scope.spawn(move || {
                let source = procedure(value, sql_family);
                for _ in 0..4 {
                    assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                }
            })
        })
        .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_alter_type_method_wrapper_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    let alter = |method: &str| format!("-- α\r\n/* β /* γ */ δ */ ALTER TYPE TYPE_F ADD {method};");
    let queries: Vec<_> = [
        "//alter_type_opts/method_specification/method_name",
        "//alter_type_opts/method_specification/method_spec_parameter_list_paren/method_spec_parameter_list/method_spec_parameter_decl/parameter_name",
        "//alter_type_opts/method_specification",
    ].into_iter().map(|pattern| ProfileQuery { pattern: pattern.into(), captures: vec![Vec::new()],
        ..ProfileQuery::default() }).collect();
    for (method, expected_name, expected_params) in [
        (
            "METHOD METHOD_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE SQL CONTAINS SQL NO EXTERNAL ACTION DETERMINISTIC",
            "METHOD_F",
            vec!["P_SPEC"],
        ),
        (
            "METHOD METHOD_F() RETURNS INTEGER LANGUAGE SQL",
            "METHOD_F",
            vec![],
        ),
        (
            "METHOD METHOD_F(INTEGER,VARCHAR(20)) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL NOT FENCED",
            "METHOD_F",
            vec![],
        ),
        (
            "METHOD \"Mé\"(\"Pé\" INTEGER,INTEGER,P_TEXT VARCHAR(20)) LANGUAGE C PARAMETER STYLE SQL NOT FENCED RETURNS INTEGER",
            "\"Mé\"",
            vec!["\"Pé\"", "P_TEXT"],
        ),
        (
            "OVERRIDING METHOD METHOD_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE SQL CONTAINS SQL",
            "METHOD_F",
            vec!["P_SPEC"],
        ),
        (
            "OVERRIDING METHOD METHOD_F() RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL NOT FENCED",
            "METHOD_F",
            vec![],
        ),
    ] {
        let source = alter(method);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        let raw_matches = |query_index| {
            result
                .matches
                .iter()
                .filter(|matched| matched.query_index == query_index)
                .map(|matched| {
                    assert_eq!(matched.captures, [Some(matched.node_index)]);
                    let node = &syntax.nodes[matched.node_index];
                    assert_eq!(
                        node.kind,
                        ["method_name", "parameter_name", "method_specification"][query_index]
                    );
                    assert_eq!(
                        syntax.nodes[node.parent.unwrap()].kind,
                        [
                            "method_specification",
                            "method_spec_parameter_decl",
                            "alter_type_opts"
                        ][query_index]
                    );
                    source.get(node.span.clone().unwrap()).unwrap()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(raw_matches(0), [expected_name]);
        assert_eq!(raw_matches(1), expected_params);
        assert_eq!(raw_matches(2), [method]);
        assert_eq!(result.matches.len(), expected_params.len() + 2);
        for kind in [
            "alter_type_statement",
            "alter_type_opts",
            "method_specification",
        ] {
            assert_eq!(
                syntax.nodes.iter().filter(|node| node.kind == kind).count(),
                1
            );
        }
        assert_eq!(
            syntax
                .tokens
                .iter()
                .filter(|token| token.kind == "METHOD")
                .count(),
            1
        );
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == kind)
                .unwrap();
            assert_ne!(token.channel, 0);
            assert_eq!(
                source.get(token.span.clone().unwrap()),
                token.text.as_deref()
            );
        }
        for token in &syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
    }
    let source = format!(
        "{} {}",
        alter("METHOD METHOD_A(P_A INTEGER) RETURNS INTEGER LANGUAGE SQL"),
        alter("OVERRIDING METHOD METHOD_B(P_B INTEGER) RETURNS INTEGER LANGUAGE SQL")
    );
    let result =
        parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
    assert!(result.syntax.diagnostics.is_empty());
    let raw_matches = |query_index| {
        result
            .matches
            .iter()
            .filter(|matched| matched.query_index == query_index)
            .map(|matched| {
                source
                    .get(
                        result.syntax.nodes[matched.node_index]
                            .span
                            .clone()
                            .unwrap(),
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(raw_matches(0), ["METHOD_A", "METHOD_B"]);
    assert_eq!(raw_matches(1), ["P_A", "P_B"]);
    assert_eq!(raw_matches(2).len(), 2);
    for source in [
        "ALTER TYPE TYPE_F ADD ATTRIBUTE ATTRIBUTE_F INTEGER;",
        "ALTER TYPE TYPE_F DROP ATTRIBUTE ATTRIBUTE_F RESTRICT;",
        "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE SQL;",
        "CREATE METHOD METHOD_F() FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER; SET LOCAL_F=P_F; END;",
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
    }
    for method in [
        "METHOD METHOD METHOD_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE SQL",
        "METHOD_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE SQL",
        "METHOD OVERRIDING METHOD_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE SQL",
        "OVERRIDING METHOD METHOD METHOD_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE SQL",
        "METHOD () RETURNS INTEGER LANGUAGE SQL",
        "METHOD METHOD_F(P_SPEC INTEGER RETURNS INTEGER LANGUAGE SQL",
    ] {
        let source = alter(method);
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {source}"
        );
    }
    let broken = format!(
        "{} CREATE MODULE AFTER_ALTER_METHOD;",
        alter("METHOD METHOD METHOD_F() RETURNS INTEGER LANGUAGE SQL")
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = alter("METHOD METHOD_F() RETURNS INTEGER LANGUAGE SQL");
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let alter = &alter;
        let handles: Vec<_> = [
            "METHOD M_A() RETURNS INTEGER LANGUAGE SQL",
            "OVERRIDING METHOD M_B(P_B INTEGER) RETURNS INTEGER LANGUAGE SQL",
            "METHOD M_C(\"Pé\" INTEGER) LANGUAGE SQL RETURNS INTEGER",
            "METHOD M_D(INTEGER) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL",
        ]
        .into_iter()
        .map(|method| {
            scope.spawn(move || {
                let source = alter(method);
                for _ in 0..4 {
                    assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                }
            })
        })
        .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_method_spec_returns_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    let create = |methods: &str| {
        format!(
            "-- α\r\n/* β /* γ */ δ */ CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL {methods};"
        )
    };
    let alter = |method: &str| format!("-- α\r\n/* β /* γ */ δ */ ALTER TYPE TYPE_F ADD {method};");
    let queries: Vec<_> = [
        "//method_specification/method_specification_return_clause",
        "//method_specification/method_name",
        "//method_specification/method_spec_parameter_list_paren/method_spec_parameter_list/method_spec_parameter_decl/parameter_name",
        "//method_specification/method_specification_seq/data_type",
    ].into_iter().map(|pattern| ProfileQuery { pattern: pattern.into(), captures: vec![Vec::new()],
        ..ProfileQuery::default() }).collect();
    for (method, expected_name, expected_params) in [
        (
            "METHOD METHOD_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE SQL",
            "METHOD_F",
            vec!["P_SPEC"],
        ),
        (
            "METHOD METHOD_F() LANGUAGE SQL RETURNS INTEGER",
            "METHOD_F",
            vec![],
        ),
        (
            "METHOD METHOD_F(INTEGER,VARCHAR(20)) LANGUAGE C PARAMETER STYLE SQL RETURNS INTEGER NOT FENCED",
            "METHOD_F",
            vec![],
        ),
        (
            "METHOD \"Mé\"(\"Pé\" INTEGER,INTEGER,P_TEXT VARCHAR(20)) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL",
            "\"Mé\"",
            vec!["\"Pé\"", "P_TEXT"],
        ),
        (
            "METHOD METHOD_F(P_SPEC INTEGER) SPECIFIC APP.SPEC_F LANGUAGE SQL RETURNS INTEGER CONTAINS SQL NO EXTERNAL ACTION DETERMINISTIC",
            "METHOD_F",
            vec!["P_SPEC"],
        ),
        (
            "METHOD METHOD_F(P_SPEC INTEGER) LANGUAGE SQL CONTAINS SQL RETURNS INTEGER SPECIFIC APP.SPEC_F NOT DETERMINISTIC",
            "METHOD_F",
            vec!["P_SPEC"],
        ),
        // SELF placement is a parser contract; catalog/result-type compatibility is separate.
        (
            "METHOD METHOD_F() SELF AS RESULT LANGUAGE SQL RETURNS INTEGER",
            "METHOD_F",
            vec![],
        ),
        (
            "METHOD METHOD_F() RETURNS INTEGER LANGUAGE SQL SELF AS RESULT",
            "METHOD_F",
            vec![],
        ),
        (
            "METHOD METHOD_F(P_SPEC INTEGER) LANGUAGE C RETURNS NULL ON NULL INPUT RETURNS INTEGER PARAMETER STYLE SQL",
            "METHOD_F",
            vec!["P_SPEC"],
        ),
        (
            "METHOD METHOD_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL RETURNS NULL ON NULL INPUT",
            "METHOD_F",
            vec!["P_SPEC"],
        ),
        (
            "OVERRIDING METHOD METHOD_F(P_SPEC INTEGER) LANGUAGE SQL RETURNS INTEGER CONTAINS SQL",
            "METHOD_F",
            vec!["P_SPEC"],
        ),
        (
            "OVERRIDING METHOD METHOD_F() RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL",
            "METHOD_F",
            vec![],
        ),
    ] {
        for source in [create(method), alter(method)] {
            let result =
                parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                    .unwrap();
            let syntax = &result.syntax;
            assert!(
                syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                syntax.diagnostics
            );
            let raw_matches = |query_index| {
                result
                    .matches
                    .iter()
                    .filter(|matched| matched.query_index == query_index)
                    .map(|matched| {
                        assert_eq!(matched.captures, [Some(matched.node_index)]);
                        let node = &syntax.nodes[matched.node_index];
                        assert_eq!(
                            node.kind,
                            [
                                "method_specification_return_clause",
                                "method_name",
                                "parameter_name"
                            ][query_index]
                        );
                        assert_eq!(
                            syntax.nodes[node.parent.unwrap()].kind,
                            [
                                "method_specification",
                                "method_specification",
                                "method_spec_parameter_decl"
                            ][query_index]
                        );
                        source.get(node.span.clone().unwrap()).unwrap()
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(raw_matches(0), ["RETURNS INTEGER"]);
            assert_eq!(raw_matches(1), [expected_name]);
            assert_eq!(raw_matches(2), expected_params);
            assert!(
                !result
                    .matches
                    .iter()
                    .any(|matched| matched.query_index == 3)
            );
            assert_eq!(result.matches.len(), expected_params.len() + 2);
            let returns: Vec<_> = syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| node.kind == "method_specification_return_clause")
                .collect();
            assert_eq!(returns.len(), 1);
            let (return_index, _) = returns[0];
            let data_type = syntax
                .nodes
                .iter()
                .find(|node| {
                    node.kind == "method_ordinary_return_type" && node.parent == Some(return_index)
                })
                .unwrap();
            assert_eq!(source.get(data_type.span.clone().unwrap()), Some("INTEGER"));
            assert_eq!(
                syntax
                    .nodes
                    .iter()
                    .filter(|node| node.kind == "method_specification")
                    .count(),
                1
            );
            if source.contains("RETURNS NULL ON NULL INPUT") {
                assert!(
                    syntax
                        .nodes
                        .iter()
                        .any(|node| node.kind == "external_routine_characteristics"
                            && source.get(node.span.clone().unwrap())
                                == Some("RETURNS NULL ON NULL INPUT"))
                );
            }
            for (index, node) in syntax.nodes.iter().enumerate() {
                assert!(!node.error && !node.missing);
                assert!(node.parent.is_none_or(|parent| parent < index));
                if let Some(span) = &node.span {
                    assert!(source.get(span.clone()).is_some());
                }
            }
            for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
                let token = syntax
                    .tokens
                    .iter()
                    .find(|token| token.kind == kind)
                    .unwrap();
                assert_ne!(token.channel, 0);
                assert_eq!(
                    source.get(token.span.clone().unwrap()),
                    token.text.as_deref()
                );
            }
            for token in &syntax.tokens {
                if token.eof {
                    assert_eq!(token.status, SpanStatus::Eof);
                    assert_eq!(token.span, Some(source.len()..source.len()));
                } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                    assert_eq!(source.get(span.clone()), Some(text.as_str()));
                }
            }
        }
    }
    let first = "METHOD METHOD_A(P_A INTEGER) LANGUAGE SQL RETURNS INTEGER";
    let second = "OVERRIDING METHOD METHOD_B(P_B INTEGER) RETURNS INTEGER LANGUAGE SQL";
    let source = create(&format!("{first},{second}"));
    let result =
        parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
    assert!(
        result.syntax.diagnostics.is_empty(),
        "{:?}",
        result.syntax.diagnostics
    );
    for (query_index, expected) in [
        (0, vec!["RETURNS INTEGER", "RETURNS INTEGER"]),
        (1, vec!["METHOD_A", "METHOD_B"]),
        (2, vec!["P_A", "P_B"]),
    ] {
        let selected: Vec<_> = result
            .matches
            .iter()
            .filter(|matched| matched.query_index == query_index)
            .map(|matched| {
                source
                    .get(
                        result.syntax.nodes[matched.node_index]
                            .span
                            .clone()
                            .unwrap(),
                    )
                    .unwrap()
            })
            .collect();
        assert_eq!(selected, expected);
    }
    let methods: Vec<_> = result
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == "method_specification")
        .collect();
    assert_eq!(methods.len(), 2);
    for (index, _) in methods {
        assert_eq!(
            result
                .syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "method_specification_return_clause"
                    && node.parent == Some(index))
                .count(),
            1
        );
    }
    for methods in [
        format!("{},{}", first.replace(" RETURNS INTEGER", ""), second),
        format!("{},{}", first, second.replace("RETURNS INTEGER ", "")),
    ] {
        let source = create(&methods);
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {source}"
        );
    }
    for source in [
        "CREATE METHOD METHOD_F() FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CREATE METHOD METHOD_F(P_METHOD INTEGER) RETURNS INTEGER FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "ALTER TYPE TYPE_F ADD ATTRIBUTE ATTRIBUTE_F INTEGER;",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER; SET LOCAL_F=P_F; END;",
        "CREATE FUNCTION FUNC_F(P_F INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.scalar' NO SQL;",
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
    }
    for method in [
        "METHOD METHOD_F(P_SPEC INTEGER) LANGUAGE SQL",
        "OVERRIDING METHOD METHOD_F(P_SPEC INTEGER) LANGUAGE SQL",
        "METHOD METHOD_F(P_SPEC INTEGER) LANGUAGE C PARAMETER STYLE SQL RETURNS NULL ON NULL INPUT",
        "METHOD METHOD_F() RETURNS INTEGER RETURNS INTEGER LANGUAGE SQL",
        "METHOD METHOD_F() RETURNS INTEGER LANGUAGE SQL RETURNS INTEGER",
        "OVERRIDING METHOD METHOD_F() LANGUAGE SQL RETURNS INTEGER RETURNS INTEGER",
        "METHOD METHOD_F() LANGUAGE SQL RETURNS",
        "METHOD METHOD_F() RETURNS LANGUAGE SQL",
    ] {
        for source in [create(method), alter(method)] {
            assert!(
                matches!(
                    parse(Dialect::Db2, &source),
                    Err(AntlrEngineError::Syntax { .. })
                ),
                "accepted {source}"
            );
        }
    }
    let broken = format!(
        "{} CREATE MODULE AFTER_METHOD_RETURNS;",
        create("METHOD METHOD_F() LANGUAGE SQL")
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = create("METHOD METHOD_F() RETURNS INTEGER LANGUAGE SQL");
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let create = &create;
        let handles: Vec<_> = ["METHOD M_A() RETURNS INTEGER LANGUAGE SQL", "OVERRIDING METHOD M_B(P_B INTEGER) LANGUAGE SQL RETURNS INTEGER", "METHOD M_C(\"Pé\" INTEGER) LANGUAGE SQL RETURNS INTEGER", "METHOD M_D(INTEGER) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL RETURNS NULL ON NULL INPUT"].into_iter().map(|method| scope.spawn(move || {
            let source = create(method);
            for _ in 0..4 { assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty()); }
        })).collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_function_defaults_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    let function = |params: &str, family: usize| {
        let (name, tail) = [
            (
                "FUNC_TABLE_F",
                "RETURNS TABLE(COL_F INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.table' NO SQL;",
            ),
            (
                "FUNC_OLE_F",
                "RETURNS TABLE(COL_F INTEGER) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
            ),
            ("F_SOURCE", "RETURNS INTEGER SOURCE APP.F_TARGET;"),
            ("F_TEMPLATE", "RETURNS INTEGER AS TEMPLATE;"),
            (
                "F_SQL",
                "RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN 1; END;",
            ),
        ][family];
        format!("-- α\r\nCREATE FUNCTION {name}({params}) {tail}")
    };
    let families = [
        (
            0,
            "ext_table_param_decl",
            "create_function_external_table_statement",
        ),
        (
            1,
            "param_decl_3",
            "create_function_old_db_external_function_statement",
        ),
        (
            2,
            "sourced_function_param_decl",
            "create_function_sourced_or_template_statement",
        ),
        (
            2,
            "sourced_function_param_decl",
            "create_function_sourced_or_template_statement",
        ),
        (
            3,
            "param_decl_2",
            "create_function_sql_scalar_table_or_row_statement",
        ),
    ];
    let queries: Vec<_> = [
        "//ext_table_param_decl/routine_default_clause/routine_default_value",
        "//param_decl_3/routine_default_clause/routine_default_value",
        "//sourced_function_param_decl/routine_default_clause/routine_default_value",
        "//param_decl_2/routine_default_clause/routine_default_value",
        "//ext_table_param_decl/parameter_name",
        "//param_decl_3/parameter_name",
        "//sourced_function_param_decl/parameter_name",
        "//param_decl_2/parameter_name",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    for (family, (query_index, parameter_kind, wrapper_kind)) in families.into_iter().enumerate() {
        let data_type = if family == 1 {
            "VARCHAR(40)"
        } else {
            "INTEGER"
        };
        for (value, child) in [
            ("NULL", "NULL_"),
            ("1", "constant_"),
            ("0", "constant_"),
            ("-1", "constant_"),
            ("+100", "constant_"),
            ("15.2", "constant_"),
            ("+5.E+2", "constant_"),
            ("INF", "constant_"),
            ("-INFINITY", "constant_"),
            ("NAN", "constant_"),
            ("SNAN", "constant_"),
            ("TRUE", "constant_"),
            ("FALSE", "constant_"),
            ("DATE '2026-10-11'", "constant_"),
            ("TIME '12:34:56'", "constant_"),
            ("TIMESTAMP '2026-10-11 12:34:56'", "constant_"),
            ("'a''β'", "constant_"),
            ("G'α''β'", "constant_"),
            ("N'α'", "constant_"),
            ("GX'0041abcd'", "constant_"),
            ("BX'41aB'", "constant_"),
            ("UX'0041'", "constant_"),
            ("U&'a\\0041'", "constant_"),
            ("U&'a!0041' UESCAPE '!'", "constant_"),
            ("CURRENT DATE", "routine_default_special_register"),
            ("CURRENT TIME", "routine_default_special_register"),
            ("CURRENT USER", "routine_default_special_register"),
            ("CURRENT SCHEMA", "routine_default_special_register"),
            ("CURRENT MEMBER", "routine_default_special_register"),
            ("CURRENT SQLID", "routine_default_special_register"),
            ("USER", "routine_default_special_register"),
            ("SESSION_USER", "routine_default_special_register"),
            ("SYSTEM_USER", "routine_default_special_register"),
            ("CURRENT_DATE", "routine_default_special_register"),
            ("CURRENT_TIME", "routine_default_special_register"),
            ("CURRENT_USER", "routine_default_special_register"),
            ("CURRENT_SCHEMA", "routine_default_special_register"),
            ("CURRENT_SQLID", "routine_default_special_register"),
            ("SYSDATE", "routine_default_special_register"),
            ("CURRENT TIMESTAMP", "routine_default_special_register"),
            ("CURRENT TIMESTAMP()", "routine_default_special_register"),
            ("CURRENT TIMESTAMP(0)", "routine_default_special_register"),
            ("CURRENT TIMESTAMP(1)", "routine_default_special_register"),
            ("CURRENT_TIMESTAMP(6)", "routine_default_special_register"),
            ("CURRENT_TIMESTAMP()", "routine_default_special_register"),
            ("LOCALTIMESTAMP", "routine_default_special_register"),
            ("LOCALTIMESTAMP()", "routine_default_special_register"),
            ("LOCALTIMESTAMP(13)", "routine_default_special_register"),
            ("(1+2)", "expression"),
            ("(CURRENT DEGREE)", "expression"),
            ("(CURRENT TIMESTAMP(2))", "expression"),
            ("V_F", "global_variable"),
            ("APP.V_F", "global_variable"),
            ("SERVER_F.APP.V_F", "global_variable"),
            ("\"Schéma\".\"Vé\"", "global_variable"),
            ("CURRENT_MEMBER", "global_variable"),
            ("\"UNKNOWN\"", "global_variable"),
        ] {
            let source = function(
                &format!("\"Pé\" {data_type} DEFAULT /* β /* γ */ δ */ {value}"),
                family,
            );
            let result =
                parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                    .unwrap();
            let syntax = &result.syntax;
            assert!(
                syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                syntax.diagnostics
            );
            assert_eq!(
                syntax
                    .nodes
                    .iter()
                    .filter(|node| node.kind == wrapper_kind)
                    .count(),
                1,
                "{source}"
            );
            assert_eq!(
                syntax
                    .nodes
                    .iter()
                    .filter(|node| node.kind == "routine_default_clause")
                    .count(),
                1
            );
            assert!(
                !syntax
                    .nodes
                    .iter()
                    .any(|node| node.kind == "default_clause")
            );
            let selected: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == query_index)
                .collect();
            assert_eq!(selected.len(), 1);
            let selected = selected[0];
            assert_eq!(selected.captures, [Some(selected.node_index)]);
            let node = &syntax.nodes[selected.node_index];
            assert_eq!(node.kind, "routine_default_value");
            assert_eq!(source.get(node.span.clone().unwrap()), Some(value));
            let clause = &syntax.nodes[node.parent.unwrap()];
            assert_eq!(clause.kind, "routine_default_clause");
            assert_eq!(syntax.nodes[clause.parent.unwrap()].kind, parameter_kind);
            assert!(
                source
                    .get(clause.span.clone().unwrap())
                    .unwrap()
                    .starts_with("DEFAULT /* β")
            );
            assert!(
                syntax
                    .nodes
                    .iter()
                    .any(|node| node.parent == Some(selected.node_index) && node.kind == child),
                "{source}: missing {child}"
            );
            let names: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == query_index + 4)
                .collect();
            assert_eq!(names.len(), 1);
            assert_eq!(names[0].captures, [Some(names[0].node_index)]);
            let name = &syntax.nodes[names[0].node_index];
            assert_eq!(syntax.nodes[name.parent.unwrap()].kind, parameter_kind);
            assert_eq!(source.get(name.span.clone().unwrap()), Some("\"Pé\""));
            assert_eq!(result.matches.len(), 2);
            assert!(
                !syntax
                    .nodes
                    .iter()
                    .any(|node| node.kind == "function_invocation")
            );
            for (index, node) in syntax.nodes.iter().enumerate() {
                assert!(!node.error && !node.missing);
                assert!(node.parent.is_none_or(|parent| parent < index));
                if let Some(span) = &node.span {
                    assert!(source.get(span.clone()).is_some());
                }
            }
            for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
                let token = syntax
                    .tokens
                    .iter()
                    .find(|token| token.kind == kind)
                    .unwrap();
                assert_ne!(token.channel, 0);
                assert_eq!(
                    source.get(token.span.clone().unwrap()),
                    token.text.as_deref()
                );
            }
            for token in &syntax.tokens {
                if token.eof {
                    assert_eq!(token.status, SpanStatus::Eof);
                    assert_eq!(token.span, Some(source.len()..source.len()));
                } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                    assert_eq!(source.get(span.clone()), Some(text.as_str()));
                }
            }
        }
        let params = if family == 1 {
            "P_A VARCHAR(40) DEFAULT -1,P_B VARCHAR(40) DEFAULT CURRENT DATE,P_C VARCHAR(40) DEFAULT (1+2)"
        } else {
            "P_A INTEGER DEFAULT -1,P_B DATE DEFAULT CURRENT DATE,P_C INTEGER DEFAULT (1+2)"
        };
        let source = function(params, family);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        let raw_matches = |index| {
            result
                .matches
                .iter()
                .filter(|matched| matched.query_index == index)
                .map(|matched| {
                    source
                        .get(
                            result.syntax.nodes[matched.node_index]
                                .span
                                .clone()
                                .unwrap(),
                        )
                        .unwrap()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(raw_matches(query_index), ["-1", "CURRENT DATE", "(1+2)"]);
        assert_eq!(raw_matches(query_index + 4), ["P_A", "P_B", "P_C"]);
        assert_eq!(result.matches.len(), 6);
        let source = function(&format!("P_F {data_type}"), family);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].query_index, query_index + 4);
        assert!(
            !result
                .syntax
                .nodes
                .iter()
                .any(|node| node.kind.starts_with("routine_default"))
        );
        for value in [
            "",
            "1+2",
            "+TRUE",
            "CURRENT DEGREE",
            "UNKNOWN",
            "CAST(1)",
            "EMPTY_CLOB()",
            "1 DEFAULT 2",
            "(1+2",
            "CURRENT TIMESTAMP(1,2)",
            "CURRENT_TIMESTAMP(-1)",
        ] {
            let source = function(&format!("P_F {data_type} DEFAULT {value}"), family);
            assert!(
                matches!(
                    parse(Dialect::Db2, &source),
                    Err(AntlrEngineError::Syntax { .. })
                ),
                "accepted {source}"
            );
        }
        let source = function(&format!("P_F {data_type} WITH DEFAULT 1"), family);
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {source}"
        );
    }
    let unnamed = function("INTEGER DEFAULT 1", 0);
    let result =
        parse_profile_queries(Dialect::Db2, &unnamed, &queries, &operation_context()).unwrap();
    assert!(result.syntax.diagnostics.is_empty());
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].query_index, 0);
    let ole_character = function("P_F VARCHAR(40) DEFAULT 'cmd'", 1);
    assert!(
        parse(Dialect::Db2, &ole_character)
            .unwrap()
            .diagnostics
            .is_empty()
    );
    for source in [
        "ALTER TABLE TABLE_F ADD COLUMN COL_F INTEGER WITH DEFAULT;",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F=P_F; END;",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE C PARAMETER STYLE SQL NO SQL EXTERNAL NAME 'pkg.proc';",
        "CREATE FUNCTION FUNC_F(P_F INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.scalar' NO SQL;",
        "CREATE METHOD METHOD_F(P_METHOD INTEGER) RETURNS INTEGER FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE SQL;",
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
        if source.starts_with("ALTER TABLE") {
            assert!(
                result
                    .syntax
                    .nodes
                    .iter()
                    .any(|node| node.kind == "default_clause")
            );
        }
        if source.starts_with("CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE SQL") {
            let (local_index, local) = result
                .syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, node)| node.kind == "declare_variable_default_clause")
                .unwrap();
            assert_eq!(
                result.syntax.nodes[local.parent.unwrap()].kind,
                "declare_variable_statement"
            );
            assert_eq!(source.get(local.span.clone().unwrap()), Some("DEFAULT 1"));
            let value = result
                .syntax
                .nodes
                .iter()
                .find(|node| node.parent == Some(local_index) && node.kind == "default_values")
                .unwrap();
            assert_eq!(source.get(value.span.clone().unwrap()), Some("1"));
            let token = result
                .syntax
                .tokens
                .iter()
                .find(|token| token.kind == "L_ONE" && token.span == value.span)
                .unwrap();
            assert_eq!(token.text.as_deref(), Some("1"));
            assert_eq!(token.status, SpanStatus::Regular);
        }
    }
    let broken = format!(
        "{} CREATE MODULE AFTER_FUNCTION_DEFAULT;",
        function("P_F INTEGER DEFAULT", 4)
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = function("P_F INTEGER DEFAULT 1", 4);
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let function = &function;
        let handles: Vec<_> = [0, 1, 2, 4]
            .into_iter()
            .map(|family| {
                scope.spawn(move || {
                    let data_type = if family == 1 {
                        "VARCHAR(40)"
                    } else {
                        "INTEGER"
                    };
                    let source = function(&format!("P_F {data_type} DEFAULT (1+2)"), family);
                    for _ in 0..4 {
                        assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_shared_function_parameter_names_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    let function = |params: &str, family: usize| {
        let (name, tail) = [
            (
                "FUNC_OLE_F",
                "RETURNS TABLE(COL_F INTEGER) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
            ),
            ("F_SOURCE", "RETURNS INTEGER SOURCE APP.F_TARGET;"),
            ("F_TEMPLATE", "RETURNS INTEGER AS TEMPLATE;"),
        ][family];
        format!("-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION {name}({params}) {tail}")
    };
    let queries: Vec<_> = [
        "//param_decl_3/parameter_name",
        "//param_decl_3/data_type",
        "//param_decl_3/routine_default_clause/routine_default_value",
        "//sourced_function_param_decl/parameter_name",
        "//sourced_function_param_decl/sourced_function_parameter_type",
        "//sourced_function_param_decl/routine_default_clause/routine_default_value",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    for family in 0..3 {
        let parameter_kind = if family == 0 {
            "param_decl_3"
        } else {
            "sourced_function_param_decl"
        };
        let type_kind = if family == 0 {
            "data_type"
        } else {
            "sourced_function_parameter_type"
        };
        let queries: Vec<_> = [
            format!("//{parameter_kind}/parameter_name"),
            format!("//{parameter_kind}/{type_kind}"),
            format!("//{parameter_kind}/routine_default_clause/routine_default_value"),
        ]
        .into_iter()
        .map(|pattern| ProfileQuery {
            pattern,
            captures: vec![Vec::new()],
            ..ProfileQuery::default()
        })
        .collect();
        // Nominals exercise native syntax, not catalog classification or assignment.
        // OLE DB parameters use only its character/graphic context.
        let types = if family == 0 {
            vec!["CHAR(10)", "VARCHAR(40)", "GRAPHIC(10)"]
        } else {
            vec!["INTEGER", "TYPE_F", "\"Typé\""]
        };
        let value = if family == 0 { "'cmd'" } else { "1" };
        for data_type in types {
            for (name, default) in [
                (None, false),
                (None, true),
                (Some("P_F"), false),
                (Some("P_F"), true),
                (Some("\"Pé\""), true),
            ] {
                let mut params = name.map_or_else(
                    || data_type.to_owned(),
                    |name| format!("{name} {data_type}"),
                );
                if default {
                    params.push_str(&format!(" DEFAULT /* ε */ {value}"));
                }
                let source = function(&params, family);
                let result =
                    parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                        .unwrap();
                let syntax = &result.syntax;
                assert!(
                    syntax.diagnostics.is_empty(),
                    "{source}: {:?}",
                    syntax.diagnostics
                );
                let wrapper = if family == 0 {
                    "create_function_old_db_external_function_statement"
                } else {
                    "create_function_sourced_or_template_statement"
                };
                assert_eq!(
                    syntax
                        .nodes
                        .iter()
                        .filter(|node| node.kind == wrapper)
                        .count(),
                    1,
                    "{source}"
                );
                assert_eq!(
                    syntax
                        .nodes
                        .iter()
                        .filter(|node| node.kind == parameter_kind)
                        .count(),
                    1
                );
                for (query_index, expected) in [
                    (0, name),
                    (1, Some(data_type)),
                    (2, default.then_some(value)),
                ] {
                    let selected: Vec<_> = result
                        .matches
                        .iter()
                        .filter(|matched| matched.query_index == query_index)
                        .collect();
                    assert_eq!(
                        selected.len(),
                        usize::from(expected.is_some()),
                        "{source}, query {query_index}"
                    );
                    if let Some(expected) = expected {
                        let selected = selected[0];
                        assert_eq!(selected.captures, [Some(selected.node_index)]);
                        let node = &syntax.nodes[selected.node_index];
                        assert_eq!(source.get(node.span.clone().unwrap()), Some(expected));
                        let parent = &syntax.nodes[node.parent.unwrap()];
                        if query_index == 2 {
                            assert_eq!(node.kind, "routine_default_value");
                            assert_eq!(parent.kind, "routine_default_clause");
                            assert_eq!(syntax.nodes[parent.parent.unwrap()].kind, parameter_kind);
                        } else {
                            assert_eq!(
                                node.kind,
                                if query_index == 0 {
                                    "parameter_name"
                                } else {
                                    type_kind
                                }
                            );
                            assert_eq!(parent.kind, parameter_kind);
                        }
                    }
                }
                assert_eq!(
                    result.matches.len(),
                    1 + usize::from(name.is_some()) + usize::from(default)
                );
                assert_eq!(
                    syntax
                        .nodes
                        .iter()
                        .filter(|node| node.kind == "parameter_name")
                        .count(),
                    usize::from(name.is_some())
                );
                assert!(
                    !syntax
                        .nodes
                        .iter()
                        .any(|node| node.kind == "default_clause")
                );
                for (index, node) in syntax.nodes.iter().enumerate() {
                    assert!(!node.error && !node.missing);
                    assert!(node.parent.is_none_or(|parent| parent < index));
                    if let Some(span) = &node.span {
                        assert!(source.get(span.clone()).is_some());
                    }
                }
                for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
                    let token = syntax
                        .tokens
                        .iter()
                        .find(|token| token.kind == kind)
                        .unwrap();
                    assert_ne!(token.channel, 0);
                    assert_eq!(
                        source.get(token.span.clone().unwrap()),
                        token.text.as_deref()
                    );
                }
                for token in &syntax.tokens {
                    if token.eof {
                        assert_eq!(token.status, SpanStatus::Eof);
                        assert_eq!(token.span, Some(source.len()..source.len()));
                    } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                        assert_eq!(source.get(span.clone()), Some(text.as_str()));
                    }
                }
            }
        }
        let params = if family == 0 {
            "P_A VARCHAR(40) DEFAULT 'a',GRAPHIC(10) DEFAULT G'b',\"Pé\" CHAR(10) DEFAULT 'c'"
        } else {
            "P_A INTEGER DEFAULT 1,TYPE_F DEFAULT 2,\"Pé\" \"Typé\" DEFAULT (1+2)"
        };
        let source = function(params, family);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert_eq!(
            result
                .syntax
                .nodes
                .iter()
                .filter(|node| node.kind == parameter_kind)
                .count(),
            3
        );
        let raw_matches = |index| {
            result
                .matches
                .iter()
                .filter(|matched| matched.query_index == index)
                .map(|matched| {
                    assert_eq!(matched.captures, [Some(matched.node_index)]);
                    source
                        .get(
                            result.syntax.nodes[matched.node_index]
                                .span
                                .clone()
                                .unwrap(),
                        )
                        .unwrap()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(raw_matches(0), ["P_A", "\"Pé\""]);
        assert_eq!(
            raw_matches(1),
            if family == 0 {
                ["VARCHAR(40)", "GRAPHIC(10)", "CHAR(10)"]
            } else {
                ["INTEGER", "TYPE_F", "\"Typé\""]
            }
        );
        assert_eq!(
            raw_matches(2),
            if family == 0 {
                ["'a'", "G'b'", "'c'"]
            } else {
                ["1", "2", "(1+2)"]
            }
        );
        assert_eq!(result.matches.len(), 8);
        let data_type = if family == 0 {
            "VARCHAR(40)"
        } else {
            "INTEGER"
        };
        for params in [
            "DEFAULT 1".to_owned(),
            format!("{data_type},"),
            format!("IN P_F {data_type}"),
            format!("P_F {data_type} DEFAULT"),
            format!("P_F {data_type} DEFAULT 1 DEFAULT 2"),
        ] {
            let source = function(&params, family);
            assert!(
                matches!(
                    parse(Dialect::Db2, &source),
                    Err(AntlrEngineError::Syntax { .. })
                ),
                "accepted {source}"
            );
        }
        let source = function(&format!("{data_type} DEFAULT {value}"), family);
        let unclosed = source.replacen(") RETURNS", " RETURNS", 1);
        assert!(
            matches!(
                parse(Dialect::Db2, &unclosed),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {unclosed}"
        );
    }
    let sql_unnamed = "CREATE FUNCTION F_SQL(INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN 1; END;";
    assert!(matches!(
        parse(Dialect::Db2, sql_unnamed),
        Err(AntlrEngineError::Syntax { .. })
    ));
    for source in [
        "CREATE FUNCTION F_SQL(P_F INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN 1; END;",
        "CREATE FUNCTION FUNC_TABLE_F(INTEGER DEFAULT 1) RETURNS TABLE(COL_F INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.table' NO SQL;",
        "CREATE FUNCTION FUNC_F(P_F INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.scalar' NO SQL;",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F=P_F; END;",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE C PARAMETER STYLE SQL NO SQL EXTERNAL NAME 'pkg.proc';",
        "CREATE METHOD METHOD_F(P_METHOD INTEGER) RETURNS INTEGER FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(P_SPEC INTEGER) RETURNS INTEGER LANGUAGE SQL;",
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
    }
    let broken = format!(
        "{} CREATE MODULE AFTER_PARAMETER_NAME;",
        function("INTEGER DEFAULT", 1)
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = function("INTEGER DEFAULT 1", 2);
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let function = &function;
        let handles: Vec<_> = [0, 1, 2, 0]
            .into_iter()
            .map(|family| {
                scope.spawn(move || {
                    let params = if family == 0 {
                        "VARCHAR(40) DEFAULT 'cmd'"
                    } else {
                        "INTEGER DEFAULT 1"
                    };
                    let source = function(params, family);
                    for _ in 0..4 {
                        assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_method_ordinary_return_types_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    let statement = |returns: &str, params: &str, family: usize| {
        let header = format!("METHOD_F({params}) {returns}");
        let body = match family {
            0 => format!("CREATE METHOD {header} FOR TYPE_F EXTERNAL NAME 'pkg.method';"),
            1 => format!(
                "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD {header} LANGUAGE SQL;"
            ),
            2 => format!("ALTER TYPE TYPE_F ADD METHOD {header} LANGUAGE SQL;"),
            _ => unreachable!(),
        };
        format!("-- α\r\n/* β /* γ */ δ */ {body}")
    };
    let queries: Vec<_> = [
        "//method_signature/method_ordinary_return_type",
        "//method_specification/method_specification_return_clause/method_ordinary_return_type",
        "//method_ordinary_return_type/method_spec_parameter_type",
        "//method_signature/method_param_list/method_signature_parameter_decl/parameter_name",
        "//method_specification/method_spec_parameter_list_paren/method_spec_parameter_list/method_spec_parameter_decl/parameter_name",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    // The shared mature type domain describes syntax. Catalog classes and
    // correspondence between definitions are server conditions.
    let types = [
        "SMALLINT",
        "INT",
        "INTEGER",
        "BIGINT",
        "DEC",
        "DEC(1,0)",
        "DECIMAL(12,2)",
        "NUMERIC(10)",
        "NUM(10,1)",
        "DECFLOAT",
        "DECFLOAT(16)",
        "FLOAT",
        "FLOAT(53)",
        "REAL",
        "DOUBLE",
        "DOUBLE PRECISION",
        "CHAR",
        "CHAR(1)",
        "CHARACTER(20 OCTETS)",
        "CHAR(10) FOR BIT DATA",
        "VARCHAR(40)",
        "CHARACTER VARYING(40 CODEUNITS32)",
        "VARCHAR(40) FOR BIT DATA",
        "CLOB",
        "CLOB(1K OCTETS)",
        "CHARACTER LARGE OBJECT(2M CODEUNITS32)",
        "GRAPHIC",
        "GRAPHIC(1)",
        "VARGRAPHIC(40 CODEUNITS16)",
        "DBCLOB",
        "DBCLOB(2G CODEUNITS16)",
        "NCHAR",
        "NCHAR(10)",
        "NATIONAL CHARACTER(10)",
        "NVARCHAR(20)",
        "NCHAR VARYING(20)",
        "NATIONAL CHAR VARYING(20)",
        "NCLOB",
        "NCLOB(1K)",
        "NCHAR LARGE OBJECT(2M)",
        "NATIONAL CHARACTER LARGE OBJECT(2G)",
        "BINARY",
        "BINARY(10)",
        "VARBINARY(40)",
        "BINARY VARYING(40)",
        "BLOB",
        "BLOB(1K)",
        "BINARY LARGE OBJECT(2G)",
        "DATE",
        "TIME",
        "TIMESTAMP",
        "TIMESTAMP(6)",
        "BOOLEAN",
        "DB2SECURITYLABEL",
        "SYSPROC.DB2SECURITYLABEL",
        "TYPE_F",
        "APP.TYPE_F",
        "\"Typé\"",
        "\"Schéma\".\"Typé\"",
        "\"XML\"",
        "REF(TYPE_F)",
        "REF(APP.TYPE_F)",
        "REF(\"Schéma\".\"Typé\")",
    ];
    for family in 0..3 {
        for data_type in types {
            for params in ["", "P_F INTEGER, \"Pé\" REF(APP.OTHER_TYPE_F)"] {
                let source = statement(&format!("RETURNS {data_type}"), params, family);
                let result =
                    parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                        .unwrap();
                let syntax = &result.syntax;
                assert!(
                    syntax.diagnostics.is_empty(),
                    "{source}: {:?}",
                    syntax.diagnostics
                );
                let type_query = usize::from(family != 0);
                for query_index in 0..3 {
                    let matches: Vec<_> = result
                        .matches
                        .iter()
                        .filter(|matched| matched.query_index == query_index)
                        .collect();
                    assert_eq!(
                        matches.len(),
                        usize::from(query_index == type_query || query_index == 2),
                        "{source}"
                    );
                    for matched in matches {
                        assert_eq!(matched.captures, [Some(matched.node_index)]);
                        let node = &syntax.nodes[matched.node_index];
                        assert_eq!(source.get(node.span.clone().unwrap()), Some(data_type));
                        assert_eq!(
                            node.kind,
                            if query_index == 2 {
                                "method_spec_parameter_type"
                            } else {
                                "method_ordinary_return_type"
                            }
                        );
                        assert_eq!(
                            syntax.nodes[node.parent.unwrap()].kind,
                            match query_index {
                                0 => "method_signature",
                                1 => "method_specification_return_clause",
                                2 => "method_ordinary_return_type",
                                _ => unreachable!(),
                            }
                        );
                    }
                }
                let names: Vec<_> = result
                    .matches
                    .iter()
                    .filter(|matched| matched.query_index >= 3)
                    .map(|matched| {
                        assert_eq!(matched.query_index, if family == 0 { 3 } else { 4 });
                        let node = &syntax.nodes[matched.node_index];
                        assert_eq!(node.kind, "parameter_name");
                        source.get(node.span.clone().unwrap()).unwrap()
                    })
                    .collect();
                assert_eq!(
                    names,
                    if params.is_empty() {
                        vec![]
                    } else {
                        vec!["P_F", "\"Pé\""]
                    }
                );
                assert_eq!(
                    syntax
                        .nodes
                        .iter()
                        .filter(|node| node.kind == "method_ordinary_return_type")
                        .count(),
                    1
                );
                assert_eq!(
                    syntax
                        .nodes
                        .iter()
                        .filter(|node| node.kind == "parameter_name")
                        .count(),
                    names.len()
                );
                for (index, node) in syntax.nodes.iter().enumerate() {
                    assert!(!node.error && !node.missing);
                    assert!(node.parent.is_none_or(|parent| parent < index));
                    if let Some(span) = &node.span {
                        assert!(source.get(span.clone()).is_some());
                    }
                }
                for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
                    let token = syntax
                        .tokens
                        .iter()
                        .find(|token| token.kind == kind)
                        .unwrap();
                    assert_ne!(token.channel, 0);
                    assert_eq!(
                        source.get(token.span.clone().unwrap()),
                        token.text.as_deref()
                    );
                }
                for token in &syntax.tokens {
                    if token.eof {
                        assert_eq!(token.status, SpanStatus::Eof);
                        assert_eq!(token.span, Some(source.len()..source.len()));
                    } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                        assert_eq!(source.get(span.clone()), Some(text.as_str()));
                    }
                }
            }
        }
        for returns in [
            "RETURNS XML",
            "RETURNS REF()",
            "RETURNS REF(APP.)",
            "RETURNS CHAR()",
            "RETURNS FLOAT()",
            "RETURNS DECIMAL()",
            "RETURNS VARCHAR()",
            "RETURNS ARRAY[INTEGER]",
            "RETURNS ANCHOR DATA TYPE TO APP.COL_F",
            "RETURNS ROW(INTEGER)",
            "RETURNS",
            "RETURNS INTEGER RETURNS INTEGER",
            "RETURNS CLOB AS",
            "RETURNS CLOB AS LOCATOR LOCATOR",
        ] {
            let source = statement(returns, "", family);
            assert!(
                matches!(
                    parse(Dialect::Db2, &source),
                    Err(AntlrEngineError::Syntax { .. })
                ),
                "{source}"
            );
        }
        let source = statement("RETURNS INTEGER CAST FROM INTEGER", "", family);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty());
        assert!(
            !result
                .syntax
                .nodes
                .iter()
                .any(|node| node.kind == "method_ordinary_return_type")
        );
        if family == 0 {
            for kind in ["method_cast_target_type", "method_cast_source_type"] {
                let node = result
                    .syntax
                    .nodes
                    .iter()
                    .find(|node| node.kind == kind)
                    .unwrap();
                assert_eq!(
                    result.syntax.nodes[node.parent.unwrap()].kind,
                    "method_signature"
                );
                assert_eq!(source.get(node.span.clone().unwrap()), Some("INTEGER"));
            }
        } else {
            let return_index = result
                .syntax
                .nodes
                .iter()
                .position(|node| node.kind == "method_specification_return_clause")
                .unwrap();
            assert_eq!(
                result
                    .syntax
                    .nodes
                    .iter()
                    .filter(|node| node.kind == "method_cast_source_type"
                        && node.parent == Some(return_index))
                    .count(),
                1
            );
            let target_index = result
                .syntax
                .nodes
                .iter()
                .position(|node| node.kind == "method_cast_target_type")
                .unwrap();
            assert_eq!(result.syntax.nodes[target_index].parent, Some(return_index));
            let target_type = result
                .syntax
                .nodes
                .iter()
                .find(|node| {
                    node.kind == "method_cast_non_xml_type" && node.parent == Some(target_index)
                })
                .unwrap();
            assert_eq!(
                source.get(target_type.span.clone().unwrap()),
                Some("INTEGER")
            );
            let missing = statement("", "", family);
            assert!(matches!(
                parse(Dialect::Db2, &missing),
                Err(AntlrEngineError::Syntax { .. })
            ));
        }
    }
    let source = statement("", "P_XML XML", 0);
    let result =
        parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
    assert!(result.syntax.diagnostics.is_empty());
    assert!(!result.matches.iter().any(|matched| matched.query_index < 3));
    assert_eq!(result.matches.len(), 1);
    for family in [1, 2] {
        assert!(matches!(
            parse(
                Dialect::Db2,
                &statement("RETURNS INTEGER", "P_XML XML", family)
            ),
            Err(AntlrEngineError::Syntax { .. })
        ));
    }
    for method in [
        "METHOD METHOD_F() LANGUAGE SQL RETURNS BOOLEAN CONTAINS SQL",
        "OVERRIDING METHOD METHOD_F() RETURNS REF(APP.TYPE_F) LANGUAGE SQL",
        "METHOD METHOD_F() RETURNS CLOB AS LOCATOR LANGUAGE C NOT FENCED",
    ] {
        for source in [
            format!("CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL {method};"),
            format!("ALTER TYPE TYPE_F ADD {method};"),
        ] {
            let result =
                parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                    .unwrap();
            assert!(
                result.syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                result.syntax.diagnostics
            );
            let selected: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == 1)
                .collect();
            assert_eq!(selected.len(), 1);
            let raw = source
                .get(
                    result.syntax.nodes[selected[0].node_index]
                        .span
                        .clone()
                        .unwrap(),
                )
                .unwrap();
            assert_eq!(
                raw,
                if method.contains("CLOB") {
                    "CLOB"
                } else if method.contains("REF(") {
                    "REF(APP.TYPE_F)"
                } else {
                    "BOOLEAN"
                }
            );
            assert_eq!(
                result
                    .syntax
                    .nodes
                    .iter()
                    .filter(|node| node.kind == "as_locator")
                    .count(),
                usize::from(method.contains("AS LOCATOR"))
            );
        }
    }
    let source = "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_A() RETURNS BOOLEAN LANGUAGE SQL, METHOD METHOD_B() RETURNS REF(APP.TYPE_F) LANGUAGE SQL;";
    let result =
        parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
    assert!(
        result.syntax.diagnostics.is_empty(),
        "{:?}",
        result.syntax.diagnostics
    );
    for query_index in [1, 2] {
        assert_eq!(
            result
                .matches
                .iter()
                .filter(|matched| matched.query_index == query_index)
                .map(|matched| source
                    .get(
                        result.syntax.nodes[matched.node_index]
                            .span
                            .clone()
                            .unwrap()
                    )
                    .unwrap())
                .collect::<Vec<_>>(),
            ["BOOLEAN", "REF(APP.TYPE_F)"]
        );
    }
    for source in [
        "ALTER TABLE TABLE_F ADD COLUMN COL_F INTEGER WITH DEFAULT;",
        "CREATE PROCEDURE PROC_F(IN P_F INTEGER) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F = P_F; END;",
        "CREATE FUNCTION F_SOURCE(P_F INTEGER DEFAULT 1) RETURNS INTEGER SOURCE APP.F_TARGET;",
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
    }
    let broken = statement("RETURNS REF()", "", 1);
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = statement("RETURNS REF(APP.TYPE_F)", "", 0);
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let statement = &statement;
        let handles: Vec<_> = [0, 1, 2, 0]
            .into_iter()
            .map(|family| {
                scope.spawn(move || {
                    let source = statement("RETURNS REF(APP.TYPE_F)", "P_F INTEGER", family);
                    for _ in 0..4 {
                        assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_method_cast_target_inferred_attributes_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    let statement = |returns: &str, params: &str, family: usize| {
        let header = format!("METHOD_F({params}) {returns}");
        let body = match family {
            0 => format!("CREATE METHOD {header} FOR TYPE_F EXTERNAL NAME 'pkg.method';"),
            1 => format!(
                "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD {header} LANGUAGE SQL;"
            ),
            2 => format!("ALTER TYPE TYPE_F ADD METHOD {header} LANGUAGE SQL;"),
            _ => unreachable!(),
        };
        format!("-- α\r\n/* β /* γ */ δ */ {body}")
    };
    let queries: Vec<_> = [
        "//method_signature/method_cast_target_type",
        "//method_specification/method_specification_return_clause/method_cast_target_type",
        "//method_cast_target_type/inferred_builtin_type",
        "//method_signature/method_param_list/method_signature_parameter_decl/parameter_name",
        "//method_specification/method_spec_parameter_list_paren/method_spec_parameter_list/method_spec_parameter_decl/parameter_name",
        "//method_signature/method_ordinary_return_type",
        "//method_specification/method_specification_return_clause/method_ordinary_return_type",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery { pattern: pattern.into(), captures: vec![Vec::new()],
        ..ProfileQuery::default() })
    .collect();
    // VARCHAR() is IBM's literal example. Other alternatives derive from its
    // quantified parameterizable-type rule, not executed server examples.
    let types = [
        ("DEC()", "DEC(12,2)"),
        ("DECIMAL()", "DECIMAL(12,2)"),
        ("NUMERIC()", "NUMERIC(12,2)"),
        ("NUM()", "NUM(12,2)"),
        ("DECFLOAT()", "DECFLOAT(16)"),
        ("CHAR()", "CHAR(40)"),
        ("CHARACTER()", "CHARACTER(40)"),
        ("CHAR VARYING()", "CHAR VARYING(40)"),
        ("CHARACTER VARYING()", "CHARACTER VARYING(40)"),
        ("VARCHAR()", "VARCHAR(40)"),
        ("CHAR() FOR BIT DATA", "CHAR(40) FOR BIT DATA"),
        ("CHARACTER() FOR BIT DATA", "CHARACTER(40) FOR BIT DATA"),
        (
            "CHAR VARYING() FOR BIT DATA",
            "CHAR VARYING(40) FOR BIT DATA",
        ),
        (
            "CHARACTER VARYING() FOR BIT DATA",
            "CHARACTER VARYING(40) FOR BIT DATA",
        ),
        ("VARCHAR() FOR BIT DATA", "VARCHAR(40) FOR BIT DATA"),
        ("CLOB()", "CLOB(2K)"),
        ("CHAR LARGE OBJECT()", "CHAR LARGE OBJECT(2K)"),
        ("CHARACTER LARGE OBJECT()", "CHARACTER LARGE OBJECT(2K)"),
        ("GRAPHIC()", "GRAPHIC(40)"),
        ("VARGRAPHIC()", "VARGRAPHIC(40)"),
        ("DBCLOB()", "DBCLOB(2K)"),
        ("NCHAR()", "NCHAR(40)"),
        ("NATIONAL CHAR()", "NATIONAL CHAR(40)"),
        ("NATIONAL CHARACTER()", "NATIONAL CHARACTER(40)"),
        ("NVARCHAR()", "NVARCHAR(40)"),
        ("NCHAR VARYING()", "NCHAR VARYING(40)"),
        ("NATIONAL CHAR VARYING()", "NATIONAL CHAR VARYING(40)"),
        (
            "NATIONAL CHARACTER VARYING()",
            "NATIONAL CHARACTER VARYING(40)",
        ),
        ("NCLOB()", "NCLOB(2K)"),
        ("NCHAR LARGE OBJECT()", "NCHAR LARGE OBJECT(2K)"),
        (
            "NATIONAL CHARACTER LARGE OBJECT()",
            "NATIONAL CHARACTER LARGE OBJECT(2K)",
        ),
        ("BINARY()", "BINARY(40)"),
        ("VARBINARY()", "VARBINARY(40)"),
        ("BINARY VARYING()", "BINARY VARYING(40)"),
        ("BLOB()", "BLOB(2K)"),
        ("BINARY LARGE OBJECT()", "BINARY LARGE OBJECT(2K)"),
        ("TIMESTAMP()", "TIMESTAMP(6)"),
    ];
    for family in 0..3 {
        for (inferred, nonempty) in types {
            for target in [inferred, nonempty] {
                for params in ["", "P_F INTEGER, \"Pé\" REF(APP.OTHER_TYPE_F)"] {
                    let source = statement(
                        &format!("RETURNS {target} CAST FROM {nonempty}"),
                        params,
                        family,
                    );
                    let result = parse_profile_queries(
                        Dialect::Db2,
                        &source,
                        &queries,
                        &operation_context(),
                    )
                    .unwrap();
                    let syntax = &result.syntax;
                    assert!(
                        syntax.diagnostics.is_empty(),
                        "{source}: {:?}",
                        syntax.diagnostics
                    );
                    for query_index in 0..3 {
                        let selected: Vec<_> = result
                            .matches
                            .iter()
                            .filter(|matched| matched.query_index == query_index)
                            .collect();
                        let expected = if query_index == 2 {
                            usize::from(target == inferred)
                        } else {
                            usize::from(query_index == usize::from(family != 0))
                        };
                        assert_eq!(selected.len(), expected, "{source}");
                        for matched in selected {
                            assert_eq!(matched.captures, [Some(matched.node_index)]);
                            let node = &syntax.nodes[matched.node_index];
                            assert_eq!(source.get(node.span.clone().unwrap()), Some(target));
                            assert_eq!(
                                node.kind,
                                if query_index == 2 {
                                    "inferred_builtin_type"
                                } else {
                                    "method_cast_target_type"
                                }
                            );
                            assert_eq!(
                                syntax.nodes[node.parent.unwrap()].kind,
                                match query_index {
                                    0 => "method_signature",
                                    1 => "method_specification_return_clause",
                                    2 => "method_cast_target_type",
                                    _ => unreachable!(),
                                }
                            );
                        }
                    }
                    let target_index = syntax
                        .nodes
                        .iter()
                        .position(|node| node.kind == "method_cast_target_type")
                        .unwrap();
                    let child_kind = if target == inferred {
                        "inferred_builtin_type"
                    } else {
                        "method_cast_non_xml_type"
                    };
                    let child = syntax
                        .nodes
                        .iter()
                        .find(|node| node.parent == Some(target_index) && node.kind == child_kind)
                        .unwrap();
                    assert_eq!(source.get(child.span.clone().unwrap()), Some(target));
                    let source_parent = syntax.nodes[target_index].parent;
                    let source_node = syntax
                        .nodes
                        .iter()
                        .find(|node| {
                            node.parent == source_parent && node.kind == "method_cast_source_type"
                        })
                        .unwrap();
                    assert_eq!(
                        source.get(source_node.span.clone().unwrap()),
                        Some(nonempty)
                    );
                    assert!(
                        !syntax
                            .nodes
                            .iter()
                            .any(|node| node.kind == "method_ordinary_return_type"
                                || node.kind == "data_type_3")
                    );
                    assert!(
                        !result
                            .matches
                            .iter()
                            .any(|matched| matched.query_index >= 5)
                    );
                    let names: Vec<_> = result
                        .matches
                        .iter()
                        .filter(|matched| (3..5).contains(&matched.query_index))
                        .map(|matched| {
                            assert_eq!(matched.query_index, if family == 0 { 3 } else { 4 });
                            let node = &syntax.nodes[matched.node_index];
                            assert_eq!(node.kind, "parameter_name");
                            source.get(node.span.clone().unwrap()).unwrap()
                        })
                        .collect();
                    assert_eq!(
                        names,
                        if params.is_empty() {
                            vec![]
                        } else {
                            vec!["P_F", "\"Pé\""]
                        }
                    );
                    for (index, node) in syntax.nodes.iter().enumerate() {
                        assert!(!node.error && !node.missing);
                        assert!(node.parent.is_none_or(|parent| parent < index));
                        if let Some(span) = &node.span {
                            assert!(source.get(span.clone()).is_some());
                        }
                    }
                    for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
                        let token = syntax
                            .tokens
                            .iter()
                            .find(|token| token.kind == kind)
                            .unwrap();
                        assert_ne!(token.channel, 0);
                        assert_eq!(
                            source.get(token.span.clone().unwrap()),
                            token.text.as_deref()
                        );
                    }
                    for token in &syntax.tokens {
                        if token.eof {
                            assert_eq!(token.status, SpanStatus::Eof);
                            assert_eq!(token.span, Some(source.len()..source.len()));
                        } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                            assert_eq!(source.get(span.clone()), Some(text.as_str()));
                        }
                    }
                }
            }
        }
        for target in [
            "FLOAT()",
            "INTEGER()",
            "DATE()",
            "TIME()",
            "BOOLEAN()",
            "XML()",
            "CURSOR()",
            "REF()",
            "TYPE_F()",
            "DECIMAL(,2)",
            "VARCHAR(,)",
            "CHAR(,)",
            "VARCHAR()()",
        ] {
            let source = statement(&format!("RETURNS {target} CAST FROM INTEGER"), "", family);
            assert!(
                matches!(
                    parse(Dialect::Db2, &source),
                    Err(AntlrEngineError::Syntax { .. })
                ),
                "{source}"
            );
        }
        for returns in [
            "RETURNS VARCHAR() CAST FROM VARCHAR()",
            "RETURNS DECFLOAT() CAST FROM DECFLOAT()",
            "RETURNS TIMESTAMP() CAST FROM TIMESTAMP()",
            "RETURNS CAST FROM INTEGER",
            "RETURNS VARCHAR() CAST FROM",
            "RETURNS VARCHAR() FROM CAST INTEGER",
            "RETURNS VARCHAR() CAST INTEGER",
            "RETURNS VARCHAR() CAST FROM INTEGER RETURNS INTEGER",
        ] {
            let source = statement(returns, "", family);
            assert!(
                matches!(
                    parse(Dialect::Db2, &source),
                    Err(AntlrEngineError::Syntax { .. })
                ),
                "{source}"
            );
        }
        for ordinary in ["INTEGER", "BOOLEAN", "REF(APP.TYPE_F)"] {
            let source = statement(&format!("RETURNS {ordinary}"), "P_F INTEGER", family);
            let result =
                parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                    .unwrap();
            assert!(
                result.syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                result.syntax.diagnostics
            );
            assert!(!result.matches.iter().any(|matched| matched.query_index < 3));
            assert_eq!(
                result
                    .matches
                    .iter()
                    .filter(|matched| matched.query_index >= 5)
                    .count(),
                1
            );
            assert_eq!(
                result
                    .matches
                    .iter()
                    .filter(|matched| (3..5).contains(&matched.query_index))
                    .count(),
                1
            );
        }
        // Hidden nested comments stay inside the raw target, not normalized away.
        let target = "VARCHAR(/* β /* γ */ δ */)";
        let source = statement(
            &format!("RETURNS /* ρ */ {target} CAST /* κ */ FROM VARCHAR(40)"),
            "P_F INTEGER",
            family,
        );
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        for matched in result
            .matches
            .iter()
            .filter(|matched| matched.query_index < 3)
        {
            assert_eq!(
                source.get(
                    result.syntax.nodes[matched.node_index]
                        .span
                        .clone()
                        .unwrap()
                ),
                Some(target)
            );
        }
    }
    for method in [
        "METHOD METHOD_F() LANGUAGE SQL RETURNS VARCHAR() CAST FROM VARCHAR(40) CONTAINS SQL",
        "OVERRIDING METHOD METHOD_F() RETURNS VARCHAR() CAST FROM VARCHAR(40) LANGUAGE SQL",
        "METHOD METHOD_F() RETURNS CLOB() CAST FROM CLOB AS LOCATOR LANGUAGE C NOT FENCED",
    ] {
        for source in [
            format!("CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL {method};"),
            format!("ALTER TYPE TYPE_F ADD {method};"),
        ] {
            let result =
                parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                    .unwrap();
            assert!(
                result.syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                result.syntax.diagnostics
            );
            let targets: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == 1)
                .map(|matched| {
                    source
                        .get(
                            result.syntax.nodes[matched.node_index]
                                .span
                                .clone()
                                .unwrap(),
                        )
                        .unwrap()
                })
                .collect();
            assert_eq!(
                targets,
                [if method.contains("CLOB") {
                    "CLOB()"
                } else {
                    "VARCHAR()"
                }]
            );
            let locator = result
                .syntax
                .nodes
                .iter()
                .find(|node| node.kind == "as_locator");
            assert_eq!(locator.is_some(), method.contains("AS LOCATOR"));
            if let Some(locator) = locator {
                assert_eq!(
                    result.syntax.nodes[locator.parent.unwrap()].kind,
                    "method_specification_return_clause"
                );
            }
        }
    }
    let source = "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_A() RETURNS VARCHAR() CAST FROM VARCHAR(40) LANGUAGE SQL, METHOD METHOD_B() RETURNS TIMESTAMP() CAST FROM TIMESTAMP(6) LANGUAGE SQL;";
    let result =
        parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
    assert!(
        result.syntax.diagnostics.is_empty(),
        "{:?}",
        result.syntax.diagnostics
    );
    for query_index in [1, 2] {
        assert_eq!(
            result
                .matches
                .iter()
                .filter(|matched| matched.query_index == query_index)
                .map(|matched| source
                    .get(
                        result.syntax.nodes[matched.node_index]
                            .span
                            .clone()
                            .unwrap()
                    )
                    .unwrap())
                .collect::<Vec<_>>(),
            ["VARCHAR()", "TIMESTAMP()"]
        );
    }
    let source = statement("", "P_XML XML", 0);
    let result =
        parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
    assert!(result.syntax.diagnostics.is_empty());
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].query_index, 3);
    for family in [1, 2] {
        for (returns, params) in [("", ""), ("RETURNS INTEGER", "P_XML XML")] {
            assert!(matches!(
                parse(Dialect::Db2, &statement(returns, params, family)),
                Err(AntlrEngineError::Syntax { .. })
            ));
        }
    }
    for source in [
        "ALTER TABLE TABLE_F ADD COLUMN COL_F INTEGER WITH DEFAULT;",
        "CREATE PROCEDURE PROC_F(IN P_F INTEGER) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F = P_F; END;",
        "CREATE FUNCTION F_SOURCE(P_F INTEGER DEFAULT 1) RETURNS INTEGER SOURCE APP.F_TARGET;",
        "CREATE FUNCTION FUNC_C_F(P_C INTEGER) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'lib!entry' NO SQL;",
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries[..3], &operation_context())
                .unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
    }
    let broken = statement("RETURNS VARCHAR() CAST FROM VARCHAR()", "", 1);
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = statement("RETURNS VARCHAR() CAST FROM VARCHAR(40)", "", 0);
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let statement = &statement;
        let handles: Vec<_> = [0, 1, 2, 0]
            .into_iter()
            .map(|family| {
                scope.spawn(move || {
                    let source = statement(
                        "RETURNS VARCHAR() CAST FROM VARCHAR(40)",
                        "P_F INTEGER",
                        family,
                    );
                    for _ in 0..4 {
                        assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_shared_function_zero_arguments_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let function = |params: &str, family: usize| {
        let (name, tail) = [
            (
                "FUNC_OLE_F",
                "RETURNS TABLE(COL_F INTEGER) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
            ),
            ("F_SOURCE", "RETURNS INTEGER SOURCE APP.F_TARGET;"),
            ("F_TEMPLATE", "RETURNS INTEGER AS TEMPLATE;"),
        ][family];
        format!("-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION {name}({params}) {tail}")
    };
    let queries: Vec<_> = [
        "//param_decl_3/parameter_name",
        "//param_decl_3/data_type",
        "//param_decl_3/routine_default_clause/routine_default_value",
        "//sourced_function_param_decl/parameter_name",
        "//sourced_function_param_decl/sourced_function_parameter_type",
        "//sourced_function_param_decl/routine_default_clause/routine_default_value",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    for family in 0..3 {
        let parameter_kind = if family == 0 {
            "param_decl_3"
        } else {
            "sourced_function_param_decl"
        };
        let list_kind = if family == 0 {
            "param_decl_list_3"
        } else {
            "sourced_function_param_decl_list"
        };
        let type_kind = if family == 0 {
            "data_type"
        } else {
            "sourced_function_parameter_type"
        };
        let queries: Vec<_> = [
            format!("//{parameter_kind}/parameter_name"),
            format!("//{parameter_kind}/{type_kind}"),
            format!("//{parameter_kind}/routine_default_clause/routine_default_value"),
        ]
        .into_iter()
        .map(|pattern| ProfileQuery {
            pattern,
            captures: vec![Vec::new()],
            ..ProfileQuery::default()
        })
        .collect();
        let wrapper = if family == 0 {
            "create_function_old_db_external_function_statement"
        } else {
            "create_function_sourced_or_template_statement"
        };
        for params in ["", "/* π /* ρ */ σ */", " \r\n-- λ\r\n "] {
            let source = function(params, family);
            let result =
                parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                    .unwrap();
            let syntax = &result.syntax;
            assert!(
                syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                syntax.diagnostics
            );
            assert!(result.matches.is_empty(), "{source}");
            assert_eq!(
                syntax
                    .nodes
                    .iter()
                    .filter(|node| node.kind == wrapper)
                    .count(),
                1
            );
            for kind in [
                list_kind,
                parameter_kind,
                "parameter_name",
                "routine_default_clause",
            ] {
                assert!(
                    !syntax.nodes.iter().any(|node| node.kind == kind),
                    "{source}: {kind}"
                );
            }
            for (index, node) in syntax.nodes.iter().enumerate() {
                assert!(!node.error && !node.missing);
                assert!(node.parent.is_none_or(|parent| parent < index));
                if let Some(span) = &node.span {
                    assert!(source.get(span.clone()).is_some());
                }
            }
            for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
                let token = syntax
                    .tokens
                    .iter()
                    .find(|token| token.kind == kind)
                    .unwrap();
                assert_ne!(token.channel, 0);
                assert_eq!(
                    source.get(token.span.clone().unwrap()),
                    token.text.as_deref()
                );
            }
            assert!(syntax.tokens.iter().any(|token| token.eof));
            for token in &syntax.tokens {
                if token.eof {
                    assert_eq!(token.status, SpanStatus::Eof);
                    assert_eq!(token.span, Some(source.len()..source.len()));
                } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                    assert_eq!(source.get(span.clone()), Some(text.as_str()));
                }
            }
        }
        // Existing types are syntax controls, not provider/catalog or assignment proof.
        let (left_type, right_type, values) = if family == 0 {
            (
                "VARCHAR(40)",
                "GRAPHIC(10)",
                [("'a'", "G'b'"), ("NULL", "NULL")],
            )
        } else {
            ("INTEGER", "\"Typé\"", [("1", "(1+2)"), ("NULL", "NULL")])
        };
        for (left_value, right_value) in values {
            let source = function(
                &format!(
                    "\"Pé\" {left_type} DEFAULT /* ε */ {left_value},{right_type} DEFAULT {right_value}"
                ),
                family,
            );
            let result =
                parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                    .unwrap();
            let syntax = &result.syntax;
            assert!(
                syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                syntax.diagnostics
            );
            let lists: Vec<_> = syntax
                .nodes
                .iter()
                .filter(|node| node.kind == list_kind)
                .collect();
            assert_eq!(lists.len(), 1);
            assert_eq!(syntax.nodes[lists[0].parent.unwrap()].kind, wrapper);
            assert_eq!(
                syntax
                    .nodes
                    .iter()
                    .filter(|node| node.kind == parameter_kind)
                    .count(),
                2
            );
            for (index, expected) in [
                vec!["\"Pé\""],
                vec![left_type, right_type],
                vec![left_value, right_value],
            ]
            .into_iter()
            .enumerate()
            {
                let selected: Vec<_> = result
                    .matches
                    .iter()
                    .filter(|matched| matched.query_index == index)
                    .collect();
                assert_eq!(selected.len(), expected.len());
                for (matched, expected) in selected.into_iter().zip(expected) {
                    assert_eq!(matched.captures, [Some(matched.node_index)]);
                    let node = &syntax.nodes[matched.node_index];
                    assert_eq!(source.get(node.span.clone().unwrap()), Some(expected));
                    let parent = &syntax.nodes[node.parent.unwrap()];
                    if index == 2 {
                        assert_eq!(parent.kind, "routine_default_clause");
                        assert_eq!(syntax.nodes[parent.parent.unwrap()].kind, parameter_kind);
                    } else {
                        assert_eq!(parent.kind, parameter_kind);
                    }
                }
            }
            assert_eq!(result.matches.len(), 5);
        }
        for params in [
            ",",
            &format!(",{left_type}"),
            &format!("{left_type},"),
            &format!("{left_type},,{right_type}"),
            "DEFAULT 1",
            &format!("IN P_F {left_type}"),
            &format!("P_F {left_type} DEFAULT"),
            &format!("P_F {left_type} DEFAULT 1 DEFAULT 2"),
        ] {
            let source = function(params, family);
            assert!(
                matches!(
                    parse(Dialect::Db2, &source),
                    Err(AntlrEngineError::Syntax { .. })
                ),
                "accepted {source}"
            );
        }
        let source = function("", family);
        for broken in [source.replacen("()", "", 1), source.replacen("()", "(", 1)] {
            assert!(
                matches!(
                    parse(Dialect::Db2, &broken),
                    Err(AntlrEngineError::Syntax { .. })
                ),
                "accepted {broken}"
            );
        }
    }
    for source in [
        "CREATE FUNCTION F_SQL(P_F INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN 1; END;",
        "CREATE FUNCTION FUNC_TABLE_F(INTEGER DEFAULT 1) RETURNS TABLE(COL_F INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.table' NO SQL;",
        "CREATE FUNCTION FUNC_F(P_F INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.scalar' NO SQL;",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F=P_F; END;",
        "CREATE METHOD METHOD_F(P_METHOD INTEGER) RETURNS VARCHAR() CAST FROM VARCHAR(40) FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CALL APP.P_F(1, 'raw');",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
    }
    let sql_unnamed = "CREATE FUNCTION F_SQL(INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN 1; END;";
    assert!(matches!(
        parse(Dialect::Db2, sql_unnamed),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let broken = format!(
        "{} CREATE MODULE AFTER_ZERO_ARGUMENT;",
        function("INTEGER DEFAULT", 1)
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = function("", 2);
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2, &source, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let function = &function;
        let handles: Vec<_> = [0, 1, 2, 0]
            .into_iter()
            .map(|family| {
                scope.spawn(move || {
                    let source = function("/* σ */", family);
                    for _ in 0..4 {
                        assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_external_table_function_zero_arguments_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let function = |params: &str| {
        format!(
            "-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION FUNC_TABLE_F({params}) RETURNS TABLE(COL_F INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.table' NO SQL;"
        )
    };
    let queries: Vec<_> = [
        "//create_function_external_table_statement/ext_table_param_decl_list/ext_table_param_decl/parameter_name",
        "//create_function_external_table_statement/ext_table_param_decl_list/ext_table_param_decl/data_type",
        "//create_function_external_table_statement/ext_table_param_decl_list/ext_table_param_decl/routine_default_clause/routine_default_value",
    ].into_iter().map(|pattern| ProfileQuery { pattern: pattern.into(), captures: vec![Vec::new()],
        ..ProfileQuery::default() }).collect();
    for params in ["", "/* π /* ρ */ σ */", " \r\n-- λ\r\n "] {
        let source = function(params);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "create_function_external_table_statement")
                .count(),
            1
        );
        for kind in [
            "ext_table_param_decl_list",
            "ext_table_param_decl",
            "parameter_name",
            "routine_default_clause",
        ] {
            assert!(
                !syntax.nodes.iter().any(|node| node.kind == kind),
                "{source}: {kind}"
            );
        }
        assert!(syntax.nodes.iter().any(|node| node.kind == "column_name"));
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == kind)
                .unwrap();
            assert_ne!(token.channel, 0);
            assert_eq!(
                source.get(token.span.clone().unwrap()),
                token.text.as_deref()
            );
        }
        assert!(syntax.tokens.iter().any(|token| token.eof));
        for token in &syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
    }
    for (params, expected_names, expected_types, expected_values) in [
        (
            "P_F INTEGER DEFAULT 1",
            vec!["P_F"],
            vec!["INTEGER"],
            vec!["1"],
        ),
        ("INTEGER DEFAULT 1", vec![], vec!["INTEGER"], vec!["1"]),
        (
            "\"Pé\" INTEGER DEFAULT /* ε */ NULL,VARCHAR(40) DEFAULT 'a'",
            vec!["\"Pé\""],
            vec!["INTEGER", "VARCHAR(40)"],
            vec!["NULL", "'a'"],
        ),
        (
            "P_A INTEGER DEFAULT (1+2),\"Pé\" INTEGER DEFAULT NULL",
            vec!["P_A", "\"Pé\""],
            vec!["INTEGER", "INTEGER"],
            vec!["(1+2)", "NULL"],
        ),
    ] {
        let source = function(params);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        let lists: Vec<_> = syntax
            .nodes
            .iter()
            .filter(|node| node.kind == "ext_table_param_decl_list")
            .collect();
        assert_eq!(lists.len(), 1);
        assert_eq!(
            syntax.nodes[lists[0].parent.unwrap()].kind,
            "create_function_external_table_statement"
        );
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "ext_table_param_decl")
                .count(),
            expected_types.len()
        );
        for (index, expected) in [expected_names, expected_types, expected_values]
            .into_iter()
            .enumerate()
        {
            let selected: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == index)
                .collect();
            assert_eq!(selected.len(), expected.len());
            for (matched, expected) in selected.into_iter().zip(expected) {
                assert_eq!(matched.captures, [Some(matched.node_index)]);
                let node = &syntax.nodes[matched.node_index];
                assert_eq!(source.get(node.span.clone().unwrap()), Some(expected));
                let parent = &syntax.nodes[node.parent.unwrap()];
                if index == 2 {
                    assert_eq!(node.kind, "routine_default_value");
                    assert_eq!(parent.kind, "routine_default_clause");
                    assert_eq!(
                        syntax.nodes[parent.parent.unwrap()].kind,
                        "ext_table_param_decl"
                    );
                } else {
                    assert_eq!(parent.kind, "ext_table_param_decl");
                }
            }
        }
    }
    for params in [
        ",",
        ",INTEGER",
        "INTEGER,",
        "INTEGER,,INTEGER",
        "DEFAULT 1",
        "P_F INTEGER DEFAULT",
        "P_F INTEGER DEFAULT 1+2",
        "P_F INTEGER DEFAULT 1 DEFAULT 2",
        "P_F INTEGER WITH DEFAULT 1",
    ] {
        let source = function(params);
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {source}"
        );
    }
    let source = function("");
    for broken in [source.replacen("()", "", 1), source.replacen("()", "(", 1)] {
        assert!(
            matches!(
                parse(Dialect::Db2, &broken),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {broken}"
        );
    }
    for source in [
        "CREATE FUNCTION FUNC_OLE_F() RETURNS TABLE(COL_F INTEGER) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
        "CREATE FUNCTION F_SOURCE() RETURNS INTEGER SOURCE APP.F_TARGET;",
        "CREATE FUNCTION F_TEMPLATE() RETURNS INTEGER AS TEMPLATE;",
        "CREATE FUNCTION F_SQL(P_F INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN 1; END;",
        "CREATE FUNCTION FUNC_F(P_F INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.scalar' NO SQL;",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F=P_F; END;",
        "CREATE METHOD METHOD_F(P_METHOD INTEGER) RETURNS VARCHAR() CAST FROM VARCHAR(40) FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
    }
    let sql_unnamed = "CREATE FUNCTION F_SQL(INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN 1; END;";
    assert!(matches!(
        parse(Dialect::Db2, sql_unnamed),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let broken = format!(
        "{} CREATE MODULE AFTER_EXTERNAL_TABLE_ZERO;",
        function("P_F INTEGER DEFAULT")
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = function("");
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2,&source,&cancelled),Err(AntlrEngineError::Operation(error))if error.kind()==std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2,&source,&expired),Err(AntlrEngineError::Operation(error))if error.kind()==std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let function = &function;
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(move || {
                    let source = function("/* σ */");
                    for _ in 0..4 {
                        assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_sql_function_zero_arguments_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let function = |params: &str| {
        format!(
            "-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION F_SQL({params}) RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN 1; END;"
        )
    };
    let queries: Vec<_> = [
        "//create_function_sql_scalar_table_or_row_statement/param_decl_list_2/param_decl_2/parameter_name",
        "//create_function_sql_scalar_table_or_row_statement/param_decl_list_2/param_decl_2/data_type_1",
        "//create_function_sql_scalar_table_or_row_statement/param_decl_list_2/param_decl_2/routine_default_clause/routine_default_value",
    ].into_iter().map(|pattern| ProfileQuery { pattern: pattern.into(), captures: vec![Vec::new()],
        ..ProfileQuery::default() }).collect();
    for params in ["", "/* π /* ρ */ σ */", " \r\n-- λ\r\n "] {
        let source = function(params);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "create_function_sql_scalar_table_or_row_statement")
                .count(),
            1
        );
        for kind in [
            "param_decl_list_2",
            "param_decl_2",
            "parameter_name",
            "routine_default_clause",
        ] {
            assert!(
                !syntax.nodes.iter().any(|node| node.kind == kind),
                "{source}: {kind}"
            );
        }
        assert!(
            syntax
                .nodes
                .iter()
                .any(|node| node.kind == "sql_function_body")
        );
        assert!(syntax.nodes.iter().any(|node| node.kind == "data_type_1"
            && node.span.as_ref().and_then(|span| source.get(span.clone())) == Some("INTEGER")));
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == kind)
                .unwrap();
            assert_ne!(token.channel, 0);
            assert_eq!(
                source.get(token.span.clone().unwrap()),
                token.text.as_deref()
            );
        }
        assert!(syntax.tokens.iter().any(|token| token.eof));
        for token in &syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
    }
    for (params, expected_names, expected_types, expected_values) in [
        (
            "P_F INTEGER DEFAULT 1",
            vec!["P_F"],
            vec!["INTEGER"],
            vec!["1"],
        ),
        (
            "IN P_F INTEGER DEFAULT 1",
            vec!["P_F"],
            vec!["INTEGER"],
            vec!["1"],
        ),
        (
            "\"Pé\" INTEGER DEFAULT /* ε */ NULL,P_B VARCHAR(40) DEFAULT 'a'",
            vec!["\"Pé\"", "P_B"],
            vec!["INTEGER", "VARCHAR(40)"],
            vec!["NULL", "'a'"],
        ),
        (
            "P_A INTEGER DEFAULT (1+2),\"Pé\" INTEGER DEFAULT NULL",
            vec!["P_A", "\"Pé\""],
            vec!["INTEGER", "INTEGER"],
            vec!["(1+2)", "NULL"],
        ),
    ] {
        let source = function(params);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        let lists: Vec<_> = syntax
            .nodes
            .iter()
            .filter(|node| node.kind == "param_decl_list_2")
            .collect();
        assert_eq!(lists.len(), 1);
        assert_eq!(
            syntax.nodes[lists[0].parent.unwrap()].kind,
            "create_function_sql_scalar_table_or_row_statement"
        );
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "param_decl_2")
                .count(),
            expected_types.len()
        );
        for (index, expected) in [expected_names, expected_types, expected_values]
            .into_iter()
            .enumerate()
        {
            let selected: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == index)
                .collect();
            assert_eq!(selected.len(), expected.len());
            for (matched, expected) in selected.into_iter().zip(expected) {
                assert_eq!(matched.captures, [Some(matched.node_index)]);
                let node = &syntax.nodes[matched.node_index];
                assert_eq!(source.get(node.span.clone().unwrap()), Some(expected));
                let parent = &syntax.nodes[node.parent.unwrap()];
                if index == 2 {
                    assert_eq!(node.kind, "routine_default_value");
                    assert_eq!(parent.kind, "routine_default_clause");
                    assert_eq!(syntax.nodes[parent.parent.unwrap()].kind, "param_decl_2");
                } else {
                    assert_eq!(parent.kind, "param_decl_2");
                }
            }
        }
    }
    for params in [
        ",",
        ",P_F INTEGER",
        "P_F INTEGER,",
        "P_A INTEGER,,P_B INTEGER",
        "INTEGER DEFAULT 1",
        "P_F DEFAULT 1",
        "DEFAULT 1",
        "P_F INTEGER DEFAULT",
        "P_F INTEGER DEFAULT 1+2",
        "P_F INTEGER DEFAULT 1 DEFAULT 2",
        "P_F INTEGER WITH DEFAULT 1",
    ] {
        let source = function(params);
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {source}"
        );
    }
    let source = function("");
    for broken in [source.replacen("()", "", 1), source.replacen("()", "(", 1)] {
        assert!(
            matches!(
                parse(Dialect::Db2, &broken),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {broken}"
        );
    }
    for source in [
        "CREATE FUNCTION FUNC_OLE_F() RETURNS TABLE(COL_F INTEGER) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
        "CREATE FUNCTION F_SOURCE() RETURNS INTEGER SOURCE APP.F_TARGET;",
        "CREATE FUNCTION F_TEMPLATE() RETURNS INTEGER AS TEMPLATE;",
        "CREATE FUNCTION FUNC_TABLE_F(INTEGER DEFAULT 1) RETURNS TABLE(COL_F INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.table' NO SQL;",
        "CREATE FUNCTION FUNC_F(P_F INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.scalar' NO SQL;",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F=P_F; END;",
        "CREATE METHOD METHOD_F(P_METHOD INTEGER) RETURNS VARCHAR() CAST FROM VARCHAR(40) FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
    }
    let broken = format!(
        "{} CREATE MODULE AFTER_SQL_ZERO;",
        function("P_F INTEGER DEFAULT")
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = function("");
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2,&source,&cancelled),Err(AntlrEngineError::Operation(error))if error.kind()==std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2,&source,&expired),Err(AntlrEngineError::Operation(error))if error.kind()==std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let function = &function;
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(move || {
                    let source = function("/* σ */");
                    for _ in 0..4 {
                        assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_external_scalar_function_zero_arguments_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let function = |params: &str| {
        format!(
            "-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION FUNC_SCALAR_F({params}) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.scalar' NO SQL;"
        )
    };
    let queries: Vec<_> = [
        "//ext_scalar_param_decl/routine_default_clause/routine_default_value",
        "//ext_scalar_param_decl/parameter_name",
        "//create_function_external_scalar_statement/ext_scalar_param_decl/data_type",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    for params in ["", "/* π /* ρ */ σ */", " \r\n-- λ\r\n "] {
        let source = function(params);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "create_function_external_scalar_statement")
                .count(),
            1
        );
        for kind in [
            "ext_scalar_param_decl",
            "parameter_name",
            "routine_default_clause",
        ] {
            assert!(
                !syntax.nodes.iter().any(|node| node.kind == kind),
                "{source}: {kind}"
            );
        }
        assert!(
            syntax
                .nodes
                .iter()
                .any(|node| node.kind == "ext_scalar_option_list")
        );
        let (return_index, return_node) = syntax
            .nodes
            .iter()
            .enumerate()
            .find(|(_, node)| node.kind == "external_scalar_ordinary_return_type")
            .unwrap();
        assert_eq!(
            syntax.nodes[return_node.parent.unwrap()].kind,
            "create_function_external_scalar_statement"
        );
        assert_eq!(
            return_node
                .span
                .as_ref()
                .and_then(|span| source.get(span.clone())),
            Some("INTEGER")
        );
        assert!(syntax.nodes.iter().any(|node| node.kind == "data_type"
            && node.parent == Some(return_index)
            && node.span.as_ref().and_then(|span| source.get(span.clone())) == Some("INTEGER")));
        assert!(
            !syntax
                .nodes
                .iter()
                .any(|node| node.kind == "data_type_2" && node.parent == return_node.parent)
        );
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == kind)
                .unwrap();
            assert_ne!(token.channel, 0);
            assert_eq!(
                source.get(token.span.clone().unwrap()),
                token.text.as_deref()
            );
        }
        assert!(syntax.tokens.iter().any(|token| token.eof));
        for token in &syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
    }
    for (params, expected_names, expected_types, expected_values) in [
        (
            "P_F INTEGER DEFAULT 1",
            vec!["P_F"],
            vec!["INTEGER"],
            vec!["1"],
        ),
        ("INTEGER DEFAULT 1", vec![], vec!["INTEGER"], vec!["1"]),
        (
            "\"Pé\" INTEGER DEFAULT /* ε */ NULL,VARCHAR(40) DEFAULT 'a'",
            vec!["\"Pé\""],
            vec!["INTEGER", "VARCHAR(40)"],
            vec!["NULL", "'a'"],
        ),
        (
            "P_A INTEGER DEFAULT (1+2),\"Pé\" INTEGER DEFAULT NULL",
            vec!["P_A", "\"Pé\""],
            vec!["INTEGER", "INTEGER"],
            vec!["(1+2)", "NULL"],
        ),
    ] {
        let source = function(params);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        for node in syntax
            .nodes
            .iter()
            .filter(|node| node.kind == "ext_scalar_param_decl")
        {
            assert_eq!(
                syntax.nodes[node.parent.unwrap()].kind,
                "create_function_external_scalar_statement"
            );
        }
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "ext_scalar_param_decl")
                .count(),
            expected_types.len()
        );
        for (index, expected) in [expected_values, expected_names, expected_types]
            .into_iter()
            .enumerate()
        {
            let selected: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == index)
                .collect();
            assert_eq!(selected.len(), expected.len());
            for (matched, expected) in selected.into_iter().zip(expected) {
                assert_eq!(matched.captures, [Some(matched.node_index)]);
                let node = &syntax.nodes[matched.node_index];
                assert_eq!(source.get(node.span.clone().unwrap()), Some(expected));
                let parent = &syntax.nodes[node.parent.unwrap()];
                if index == 0 {
                    assert_eq!(node.kind, "routine_default_value");
                    assert_eq!(parent.kind, "routine_default_clause");
                    assert_eq!(
                        syntax.nodes[parent.parent.unwrap()].kind,
                        "ext_scalar_param_decl"
                    );
                } else {
                    assert_eq!(parent.kind, "ext_scalar_param_decl");
                }
            }
        }
    }
    for params in [
        ",",
        ",P_F INTEGER",
        "P_F INTEGER,",
        "P_A INTEGER,,P_B INTEGER",
        "DEFAULT 1",
        "P_F INTEGER DEFAULT",
        "P_F INTEGER DEFAULT 1+2",
        "P_F INTEGER DEFAULT 1 DEFAULT 2",
        "P_F INTEGER WITH DEFAULT 1",
    ] {
        let source = function(params);
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {source}"
        );
    }
    let source = function("");
    for broken in [source.replacen("()", "", 1), source.replacen("()", "(", 1)] {
        assert!(
            matches!(
                parse(Dialect::Db2, &broken),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {broken}"
        );
    }
    for source in [
        "CREATE FUNCTION FUNC_OLE_F() RETURNS TABLE(COL_F INTEGER) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
        "CREATE FUNCTION F_SOURCE() RETURNS INTEGER SOURCE APP.F_TARGET;",
        "CREATE FUNCTION F_TEMPLATE() RETURNS INTEGER AS TEMPLATE;",
        "CREATE FUNCTION FUNC_TABLE_F(INTEGER DEFAULT 1) RETURNS TABLE(COL_F INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.table' NO SQL;",
        "CREATE FUNCTION F_SQL() RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN 1; END;",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F=P_F; END;",
        "CREATE METHOD METHOD_F(P_METHOD INTEGER) RETURNS VARCHAR() CAST FROM VARCHAR(40) FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
    }
    let broken = format!(
        "{} CREATE MODULE AFTER_EXTERNAL_SCALAR_ZERO;",
        function("P_F INTEGER DEFAULT")
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = function("");
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2,&source,&cancelled),Err(AntlrEngineError::Operation(error))if error.kind()==std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2,&source,&expired),Err(AntlrEngineError::Operation(error))if error.kind()==std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let function = &function;
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(move || {
                    for params in ["/* σ */", "IN \"Pé\" INTEGER DEFAULT 1"] {
                        let source = function(params);
                        for _ in 0..4 {
                            assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                        }
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_external_scalar_parameter_locator_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let function = |params: &str| {
        format!(
            "-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION FUNC_LOC_F({params}) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.scalar' NO SQL;"
        )
    };
    let queries: Vec<_> = [
        "//ext_scalar_param_decl/routine_default_clause/routine_default_value",
        "//ext_scalar_param_decl/parameter_name",
        "//create_function_external_scalar_statement/ext_scalar_param_decl/data_type",
        "//ext_scalar_param_decl/as_locator",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    for params in ["", "/* π /* ρ */ σ */", " \r\n-- λ\r\n "] {
        let source = function(params);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "create_function_external_scalar_statement")
                .count(),
            1
        );
        for kind in [
            "ext_scalar_param_decl",
            "parameter_name",
            "routine_default_clause",
            "as_locator",
        ] {
            assert!(
                !syntax.nodes.iter().any(|node| node.kind == kind),
                "{source}: {kind}"
            );
        }
        assert!(
            syntax
                .nodes
                .iter()
                .any(|node| node.kind == "ext_scalar_option_list")
        );
        let (return_index, return_node) = syntax
            .nodes
            .iter()
            .enumerate()
            .find(|(_, node)| node.kind == "external_scalar_ordinary_return_type")
            .unwrap();
        assert_eq!(
            syntax.nodes[return_node.parent.unwrap()].kind,
            "create_function_external_scalar_statement"
        );
        assert_eq!(
            return_node
                .span
                .as_ref()
                .and_then(|span| source.get(span.clone())),
            Some("INTEGER")
        );
        assert!(syntax.nodes.iter().any(|node| node.kind == "data_type"
            && node.parent == Some(return_index)
            && node.span.as_ref().and_then(|span| source.get(span.clone())) == Some("INTEGER")));
        assert!(
            !syntax
                .nodes
                .iter()
                .any(|node| node.kind == "data_type_2" && node.parent == return_node.parent)
        );
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == kind)
                .unwrap();
            assert_ne!(token.channel, 0);
            assert_eq!(
                source.get(token.span.clone().unwrap()),
                token.text.as_deref()
            );
        }
        assert!(syntax.tokens.iter().any(|token| token.eof));
        for token in &syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
    }
    for (params, expected_names, expected_types, expected_values, expected_locators) in [
        (
            "IN P_LOC CLOB DEFAULT NULL AS LOCATOR",
            vec!["P_LOC"],
            vec!["CLOB"],
            vec!["NULL"],
            vec!["AS LOCATOR"],
        ),
        (
            "BLOB AS LOCATOR",
            vec![],
            vec!["BLOB"],
            vec![],
            vec!["AS LOCATOR"],
        ),
        (
            "\"Pé\" DBCLOB(2K) DEFAULT /* ε */ NULL AS /* π */ LOCATOR",
            vec!["\"Pé\""],
            vec!["DBCLOB(2K)"],
            vec!["NULL"],
            vec!["AS /* π */ LOCATOR"],
        ),
        (
            "P_A CLOB AS LOCATOR,P_B BLOB DEFAULT NULL AS LOCATOR,P_C INTEGER DEFAULT 1",
            vec!["P_A", "P_B", "P_C"],
            vec!["CLOB", "BLOB", "INTEGER"],
            vec!["NULL", "1"],
            vec!["AS LOCATOR", "AS LOCATOR"],
        ),
        (
            "P_N CLOB DEFAULT NULL",
            vec!["P_N"],
            vec!["CLOB"],
            vec!["NULL"],
            vec![],
        ),
        (
            "INTEGER DEFAULT 1",
            vec![],
            vec!["INTEGER"],
            vec!["1"],
            vec![],
        ),
    ] {
        let source = function(params);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        for node in syntax
            .nodes
            .iter()
            .filter(|node| node.kind == "ext_scalar_param_decl")
        {
            assert_eq!(
                syntax.nodes[node.parent.unwrap()].kind,
                "create_function_external_scalar_statement"
            );
        }
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "ext_scalar_param_decl")
                .count(),
            expected_types.len()
        );
        for (index, expected) in [
            expected_values,
            expected_names,
            expected_types,
            expected_locators,
        ]
        .into_iter()
        .enumerate()
        {
            let selected: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == index)
                .collect();
            assert_eq!(selected.len(), expected.len());
            for (matched, expected) in selected.into_iter().zip(expected) {
                assert_eq!(matched.captures, [Some(matched.node_index)]);
                let node = &syntax.nodes[matched.node_index];
                assert_eq!(source.get(node.span.clone().unwrap()), Some(expected));
                let parent = &syntax.nodes[node.parent.unwrap()];
                if index == 0 {
                    assert_eq!(node.kind, "routine_default_value");
                    assert_eq!(parent.kind, "routine_default_clause");
                    assert_eq!(
                        syntax.nodes[parent.parent.unwrap()].kind,
                        "ext_scalar_param_decl"
                    );
                } else {
                    assert_eq!(parent.kind, "ext_scalar_param_decl");
                    if index == 3 {
                        assert_eq!(node.kind, "as_locator");
                        let slot = parent.span.as_ref().unwrap();
                        let locator = node.span.as_ref().unwrap();
                        assert!(slot.start <= locator.start);
                        assert_eq!(slot.end, locator.end);
                    }
                }
            }
        }
    }
    for params in [
        "AS LOCATOR CLOB",
        "P_LOC AS LOCATOR CLOB",
        "P_LOC CLOB AS LOCATOR DEFAULT NULL",
        "P_LOC CLOB AS LOCATOR AS LOCATOR",
        "P_LOC CLOB DEFAULT NULL AS",
        "P_LOC CLOB DEFAULT NULL LOCATOR",
        "P_LOC CLOB AS LOCATOR LOCATOR",
    ] {
        let source = function(params);
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {source}"
        );
    }
    let source = function("");
    for broken in [source.replacen("()", "", 1), source.replacen("()", "(", 1)] {
        assert!(
            matches!(
                parse(Dialect::Db2, &broken),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {broken}"
        );
    }
    for source in [
        "CREATE FUNCTION FUNC_OLE_F() RETURNS TABLE(COL_F INTEGER) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
        "CREATE FUNCTION F_SOURCE() RETURNS INTEGER SOURCE APP.F_TARGET;",
        "CREATE FUNCTION F_TEMPLATE() RETURNS INTEGER AS TEMPLATE;",
        "CREATE FUNCTION FUNC_TABLE_F(INTEGER DEFAULT 1) RETURNS TABLE(COL_F INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.table' NO SQL;",
        "CREATE FUNCTION F_SQL() RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN 1; END;",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F=P_F; END;",
        "CREATE METHOD METHOD_F(P_METHOD INTEGER) RETURNS VARCHAR() CAST FROM VARCHAR(40) FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F() RETURNS CLOB AS LOCATOR LANGUAGE C NOT FENCED;",
        "ALTER TYPE TYPE_F ADD METHOD METHOD_F() RETURNS CLOB AS LOCATOR LANGUAGE C NOT FENCED;",
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(result.matches.is_empty(), "{source}");
    }
    let broken = format!(
        "{} CREATE MODULE AFTER_EXTERNAL_SCALAR_LOCATOR;",
        function("P_LOC CLOB DEFAULT NULL AS")
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = function("");
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2,&source,&cancelled),Err(AntlrEngineError::Operation(error))if error.kind()==std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2,&source,&expired),Err(AntlrEngineError::Operation(error))if error.kind()==std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let function = &function;
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(move || {
                    for params in ["/* σ */", "IN \"Pé\" CLOB DEFAULT NULL AS /* π */ LOCATOR"] {
                        let source = function(params);
                        for _ in 0..4 {
                            assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                        }
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_sql_function_typed_returns_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let function = |params: &str, returns: &str, select: &str, access: &str| {
        format!(
            "-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION F_SQL_RET({params}) RETURNS {returns} LANGUAGE SQL {access} BEGIN ATOMIC RETURN {select}; END;"
        )
    };
    let queries: Vec<_> = [
        "//sql_function_return_column/column_name",
        "//sql_function_return_column/sql_function_return_data_type",
        "//create_function_sql_scalar_table_or_row_statement/param_decl_list_2/param_decl_2/parameter_name",
    ].into_iter().map(|pattern| ProfileQuery { pattern: pattern.into(), captures: vec![Vec::new()],
        ..ProfileQuery::default() }).collect();
    let verify = |source: &str, wrapper: &str, names: &[&str], types: &[&str], parameter: &str| {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == wrapper)
                .count(),
            1
        );
        assert_eq!(
            syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "sql_function_return_column")
                .count(),
            names.len()
        );
        let expected_parameters = [parameter];
        for (index, expected) in [names, types, &expected_parameters[..]]
            .into_iter()
            .enumerate()
        {
            let selected: Vec<_> = result
                .matches
                .iter()
                .filter(|m| m.query_index == index)
                .collect();
            assert_eq!(selected.len(), expected.len(), "{source}");
            for (matched, expected) in selected.into_iter().zip(expected) {
                assert_eq!(matched.captures, [Some(matched.node_index)]);
                let node = &syntax.nodes[matched.node_index];
                assert_eq!(source.get(node.span.clone().unwrap()), Some(*expected));
                let parent = &syntax.nodes[node.parent.unwrap()];
                if index == 2 {
                    assert_eq!(parent.kind, "param_decl_2");
                } else {
                    assert_eq!(parent.kind, "sql_function_return_column");
                    let list = &syntax.nodes[parent.parent.unwrap()];
                    assert_eq!(list.kind, wrapper);
                    assert_eq!(
                        syntax.nodes[list.parent.unwrap()].kind,
                        "create_function_sql_scalar_table_or_row_statement"
                    );
                }
            }
        }
        assert!(
            syntax
                .nodes
                .iter()
                .any(|node| node.kind == "sql_function_body")
        );
        assert!(syntax.nodes.iter().any(|node| node.kind == "fullselect"));
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing, "{source}: {}", node.kind);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
            let token = syntax
                .tokens
                .iter()
                .find(|token| token.kind == kind)
                .unwrap();
            assert_ne!(token.channel, 0);
            assert_eq!(
                source.get(token.span.clone().unwrap()),
                token.text.as_deref()
            );
        }
        assert!(syntax.tokens.iter().any(|token| token.eof));
        for token in &syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
    };
    for (params, returns, select, access, wrapper, names, types, parameter) in [
        (
            "P_F INTEGER",
            "TABLE(C_A INTEGER)",
            "SELECT P_F FROM SRC_DUMMY_F",
            "READS SQL DATA",
            "sql_function_return_table_columns",
            vec!["C_A"],
            vec!["INTEGER"],
            "P_F",
        ),
        (
            "P_F INTEGER",
            "TABLE(C_A INTEGER,C_B VARCHAR(40))",
            "SELECT P_F,'ok' FROM SRC_DUMMY_F",
            "READS SQL DATA",
            "sql_function_return_table_columns",
            vec!["C_A", "C_B"],
            vec!["INTEGER", "VARCHAR(40)"],
            "P_F",
        ),
        (
            "P_F INTEGER",
            "TABLE(C_D APP.DIST_F,C_S APP.STRUCT_F,C_R REF(APP.STRUCT_F))",
            "SELECT C_D,C_S,C_R FROM SRC_TYPES_F",
            "READS SQL DATA",
            "sql_function_return_table_columns",
            vec!["C_D", "C_S", "C_R"],
            vec!["APP.DIST_F", "APP.STRUCT_F", "REF(APP.STRUCT_F)"],
            "P_F",
        ),
        (
            "P_OBJ TYPE_F",
            "ROW(C_A INTEGER,C_B INTEGER)",
            "SELECT 1,2 FROM SRC_DUMMY_F",
            "READS SQL DATA",
            "sql_function_return_row_columns",
            vec!["C_A", "C_B"],
            vec!["INTEGER", "INTEGER"],
            "P_OBJ",
        ),
        (
            "P_OBJ TYPE_F",
            "ROW(C_A INTEGER,C_B INTEGER,C_C INTEGER)",
            "SELECT 1,2,3 FROM SRC_DUMMY_F",
            "READS SQL DATA",
            "sql_function_return_row_columns",
            vec!["C_A", "C_B", "C_C"],
            vec!["INTEGER", "INTEGER", "INTEGER"],
            "P_OBJ",
        ),
        (
            "P_F INTEGER",
            "TABLE(\"Cé\" /* ε */ REF(/* π */\"Á\".\"β\"))",
            "SELECT \"Cé\" FROM SRC_TYPES_F",
            "READS SQL DATA",
            "sql_function_return_table_columns",
            vec!["\"Cé\""],
            vec!["REF(/* π */\"Á\".\"β\")"],
            "P_F",
        ),
    ] {
        let source = function(params, returns, select, access);
        verify(&source, wrapper, &names, &types, parameter);
    }
    let types = [
        "SMALLINT",
        "INT",
        "INTEGER",
        "BIGINT",
        "DEC",
        "DEC(8,0)",
        "DECIMAL(1,0)",
        "NUM(8)",
        "NUMERIC",
        "DECFLOAT",
        "DECFLOAT(34)",
        "FLOAT",
        "FLOAT(1)",
        "REAL",
        "DOUBLE",
        "DOUBLE PRECISION",
        "CHAR",
        "CHAR(1 OCTETS) FOR BIT DATA",
        "CHARACTER(8 CODEUNITS32)",
        "VARCHAR(1)",
        "CHAR VARYING(40 OCTETS) FOR BIT DATA",
        "CHARACTER VARYING(40 CODEUNITS32)",
        "CLOB",
        "CLOB(1 K OCTETS)",
        "CHAR LARGE OBJECT(1 M)",
        "CHARACTER LARGE OBJECT(1 G CODEUNITS32)",
        "GRAPHIC",
        "GRAPHIC(1 CODEUNITS16)",
        "VARGRAPHIC(1 CODEUNITS32)",
        "DBCLOB",
        "DBCLOB(1 K CODEUNITS16)",
        "NCHAR",
        "NCHAR(1)",
        "NATIONAL CHAR",
        "NATIONAL CHARACTER(1)",
        "NVARCHAR(1)",
        "NCHAR VARYING(1)",
        "NATIONAL CHAR VARYING(1)",
        "NATIONAL CHARACTER VARYING(1)",
        "NCLOB",
        "NCLOB(1 K)",
        "NCHAR LARGE OBJECT(1 M)",
        "NATIONAL CHARACTER LARGE OBJECT(1 G)",
        "BINARY",
        "BINARY(1)",
        "VARBINARY(1)",
        "BINARY VARYING(1)",
        "BLOB",
        "BLOB(1 K)",
        "BINARY LARGE OBJECT(1 M)",
        "DATE",
        "TIME",
        "TIMESTAMP",
        "TIMESTAMP(0)",
        "BOOLEAN",
        "DB2SECURITYLABEL",
        "SYSPROC.DB2SECURITYLABEL",
        "APP.TYPE_F",
        "REF(TYPE_F)",
        "REF(APP.TYPE_F)",
    ];
    for data_type in types.into_iter().chain([
        "XML",
        "CURSOR",
        "\"TYPE_F\"",
        "\"Á\".\"β\"",
        "REF(\"Á\".\"β\")",
    ]) {
        let returns = format!("TABLE(C_T {data_type})");
        let source = function(
            "P_F INTEGER",
            &returns,
            "SELECT C_T FROM SRC_TYPES_F",
            "READS SQL DATA",
        );
        verify(
            &source,
            "sql_function_return_table_columns",
            &["C_T"],
            &[data_type],
            "P_F",
        );
        let syntax = parse(Dialect::Db2, &source).unwrap();
        let kind = if data_type.starts_with("REF(")
            || data_type.starts_with('"')
            || data_type == "APP.TYPE_F"
        {
            "sql_function_return_named_type"
        } else {
            "sql_function_return_builtin_type"
        };
        assert_eq!(
            syntax.nodes.iter().filter(|node| node.kind == kind).count(),
            1,
            "{source}"
        );
        if data_type == "CURSOR" {
            assert!(
                syntax
                    .tokens
                    .iter()
                    .any(|token| token.kind == "CURSOR" && token.text.as_deref() == Some("CURSOR"))
            );
        }
    }
    for returns in [
        "TABLE()",
        "ROW()",
        "TABLE(C_A)",
        "ROW(C_A INTEGER)",
        "TABLE(,C_A INTEGER)",
        "TABLE(C_A INTEGER,)",
        "TABLE(C_A INTEGER,,C_B INTEGER)",
        "TABLE(APP.C_A INTEGER)",
        "TABLE C_A INTEGER",
        "ROW C_A INTEGER,C_B INTEGER",
        "TABLE(C_A REF())",
        "TABLE(C_A VARCHAR())",
        "TABLE(C_A FLOAT())",
        "TABLE(C_A ANCHOR DATA TYPE TO APP.COL_F)",
        "TABLE(C_A ROW OF APP.TABLE_F)",
        "TABLE(C_A CLOB AS LOCATOR)",
        "TABLE(C_A INTEGER DEFAULT 1)",
        "TABLE(C_A INTEGER",
    ] {
        let source = function(
            "P_F INTEGER",
            returns,
            "SELECT C_A FROM SRC_TYPES_F",
            "READS SQL DATA",
        );
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {source}"
        );
    }
    for source in [
        "CREATE FUNCTION F_SQL(P_F INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC RETURN 1; END;",
        "CREATE FUNCTION FUNC_TABLE_F(INTEGER DEFAULT 1) RETURNS TABLE(COL_F INTEGER) LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.table' NO SQL;",
        "CREATE FUNCTION FUNC_OLE_F() RETURNS TABLE(COL_F INTEGER) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
        "CREATE FUNCTION F_SOURCE() RETURNS INTEGER SOURCE APP.F_TARGET;",
        "CREATE FUNCTION F_TEMPLATE() RETURNS INTEGER AS TEMPLATE;",
        "CREATE FUNCTION FUNC_LOC_F(IN P_LOC CLOB DEFAULT NULL AS LOCATOR) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.scalar' NO SQL;",
        "CREATE PROCEDURE PROC_F(P_F INTEGER DEFAULT 1) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER DEFAULT 1; SET LOCAL_F=P_F; END;",
        "CREATE METHOD METHOD_F(P_METHOD INTEGER) RETURNS VARCHAR() CAST FROM VARCHAR(40) FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CALL PROC_F(P_IN => 1);",
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        assert!(
            !result.matches.iter().any(|matched| matched.query_index < 2),
            "{source}"
        );
        assert!(
            !result
                .syntax
                .nodes
                .iter()
                .any(|node| node.kind.starts_with("sql_function_return_")),
            "{source}"
        );
    }
    let broken = format!(
        "{} CREATE MODULE AFTER_SQL_TYPED_RETURN;",
        function(
            "P_F INTEGER",
            "TABLE(C_A INTEGER,)",
            "SELECT C_A FROM SRC_TYPES_F",
            "READS SQL DATA"
        )
    );
    let recovered =
        graphit_ast::antlr_engine::parse(Dialect::Db2, &broken, &operation_context()).unwrap();
    assert!(!recovered.diagnostics.is_empty());
    for diagnostic in &recovered.diagnostics {
        assert_eq!(diagnostic.grammar, "Db2Parser.g4");
        assert!(!diagnostic.message.is_empty());
        assert!(
            diagnostic
                .span
                .as_ref()
                .is_some_and(|span| broken.get(span.clone()).is_some())
        );
        assert!(diagnostic.line >= 2);
    }
    assert!(matches!(
        parse(Dialect::Db2, &broken),
        Err(AntlrEngineError::Syntax { .. })
    ));
    let source = function(
        "P_F INTEGER",
        "TABLE(C_A INTEGER)",
        "SELECT P_F FROM SRC_DUMMY_F",
        "READS SQL DATA",
    );
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2,&source,&cancelled),Err(AntlrEngineError::Operation(error)) if error.kind()==std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse(Dialect::Db2,&source,&expired),Err(AntlrEngineError::Operation(error)) if error.kind()==std::io::ErrorKind::TimedOut)
    );
    std::thread::scope(|scope| {
        let function = &function;
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(move || {
                    for (params, returns, select, access) in [
                        (
                            "P_F INTEGER",
                            "TABLE(C_A INTEGER)",
                            "SELECT P_F FROM SRC_DUMMY_F",
                            "READS SQL DATA",
                        ),
                        (
                            "P_OBJ TYPE_F",
                            "ROW(C_A INTEGER,C_B INTEGER)",
                            "SELECT 1,2 FROM SRC_DUMMY_F",
                            "READS SQL DATA",
                        ),
                    ] {
                        let source = function(params, returns, select, access);
                        for _ in 0..4 {
                            assert!(parse(Dialect::Db2, &source).unwrap().diagnostics.is_empty());
                        }
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

#[test]
fn db2_fullselect_table_qnames_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};

    let queries: Vec<_> = [
        "//from_clause//singles_table_reference/table_reference_name",
        "//from_clause//singles_table_reference/table_reference_name/table_name",
        "//from_clause//singles_table_reference/table_reference_name/table_reference_schema",
        "//from_clause//singles_table_reference/table_reference_name/data_source_name",
        "//from_clause//table_reference//table_name",
        "//sql_function_return_column/column_name",
        "//create_function_sql_scalar_table_or_row_statement/param_decl_list_2/param_decl_2/parameter_name",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, pattern)| ProfileQuery {
        pattern: pattern.into(),
        captures: if index == 0 {
            vec![
                Vec::new(),
                vec![CaptureStep::NearestAncestor("singles_table_reference".into())],
            ]
        } else {
            vec![Vec::new()]
        },
        ..ProfileQuery::default()
    })
    .collect();
    let verify = |source: &str,
                  references: &[(&str, Option<&str>, &str, Option<&str>)],
                  typed_function: bool| {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        let syntax = &result.syntax;
        assert!(
            syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            syntax.diagnostics
        );
        for query_index in 0..queries.len() {
            let expected: Vec<_> = match query_index {
                0 => references.iter().map(|reference| reference.0).collect(),
                1 | 4 => references.iter().map(|reference| reference.2).collect(),
                2 => references
                    .iter()
                    .filter_map(|reference| reference.1)
                    .collect(),
                3 => references
                    .iter()
                    .filter_map(|reference| reference.3)
                    .collect(),
                5 if typed_function => vec!["C_A"],
                6 if typed_function => vec!["P_F"],
                _ => vec![],
            };
            let matches: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == query_index)
                .collect();
            assert_eq!(
                matches.len(),
                expected.len(),
                "{source}: query {query_index}"
            );
            for (matched, expected) in matches.into_iter().zip(expected) {
                assert_eq!(matched.captures[0], Some(matched.node_index));
                let node = &syntax.nodes[matched.node_index];
                assert_eq!(node.status, SpanStatus::Regular);
                let span = node.span.clone().expect("regular selected span");
                assert_eq!(source.get(span.clone()), Some(expected), "{source}");
                let parent_index = node.parent.expect("selected parent");
                let parent = &syntax.nodes[parent_index];
                let parent_span = parent.span.as_ref().expect("regular parent span");
                assert!(parent_span.start <= span.start && span.end <= parent_span.end);
                match query_index {
                    0 => {
                        assert_eq!(parent.kind, "singles_table_reference");
                        assert_eq!(matched.captures[1], Some(parent_index));
                    }
                    1..=4 => assert_eq!(parent.kind, "table_reference_name"),
                    5 => assert_eq!(parent.kind, "sql_function_return_column"),
                    6 => assert_eq!(parent.kind, "param_decl_2"),
                    _ => unreachable!(),
                }
            }
        }
        for (index, node) in syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing, "{source}: {}", node.kind);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(
                    source.get(span.clone()).is_some(),
                    "{source}: {}",
                    node.kind
                );
            }
        }
        if typed_function {
            for kind in [
                "create_function_sql_scalar_table_or_row_statement",
                "sql_function_return_table_columns",
                "compound_sql_inlined",
                "return_statement",
                "fullselect",
            ] {
                assert!(syntax.nodes.iter().any(|node| node.kind == kind), "{kind}");
            }
        }
    };
    for (source, raw, schema, table, datasource) in [
        ("SELECT C_A FROM TABLE_F;", "TABLE_F", None, "TABLE_F", None),
        (
            "SELECT C_A FROM APP.TABLE_F;",
            "APP.TABLE_F",
            Some("APP"),
            "TABLE_F",
            None,
        ),
        (
            "SELECT 1 FROM SYSIBM.SYSDUMMY1;",
            "SYSIBM.SYSDUMMY1",
            Some("SYSIBM"),
            "SYSDUMMY1",
            None,
        ),
        (
            "SELECT C_A FROM SESSION.TABLE_F;",
            "SESSION.TABLE_F",
            Some("SESSION"),
            "TABLE_F",
            None,
        ),
        (
            "SELECT C_A FROM \"App\".\"Tab.le\";",
            "\"App\".\"Tab.le\"",
            Some("\"App\""),
            "\"Tab.le\"",
            None,
        ),
        (
            "SELECT C_A FROM \"App.Table\";",
            "\"App.Table\"",
            None,
            "\"App.Table\"",
            None,
        ),
        (
            "SELECT C_A FROM \"A\"\"B\".\"T\"\"Q\";",
            "\"A\"\"B\".\"T\"\"Q\"",
            Some("\"A\"\"B\""),
            "\"T\"\"Q\"",
            None,
        ),
        (
            "-- α\r\n/* β /* γ */ δ */ SELECT C_A FROM \"Á\" /* ε */ . /* π */ \"β.γ\" AS Q_F;",
            "\"Á\" /* ε */ . /* π */ \"β.γ\"",
            Some("\"Á\""),
            "\"β.γ\"",
            None,
        ),
        (
            "SELECT C_A FROM DATA_F.REMOTE_SCHEMA_F.REMOTE_TABLE_F;",
            "DATA_F.REMOTE_SCHEMA_F.REMOTE_TABLE_F",
            Some("REMOTE_SCHEMA_F"),
            "REMOTE_TABLE_F",
            Some("DATA_F"),
        ),
        (
            "SELECT C_A FROM \"Data.Source\".\"Remote.Schema\".\"Remote.Table\";",
            "\"Data.Source\".\"Remote.Schema\".\"Remote.Table\"",
            Some("\"Remote.Schema\""),
            "\"Remote.Table\"",
            Some("\"Data.Source\""),
        ),
    ] {
        verify(source, &[(raw, schema, table, datasource)], false);
    }
    verify(
        "SELECT C_A FROM APP.TABLE_F AS Q_F, OTHER_F.OTHER_TABLE_F;",
        &[
            ("APP.TABLE_F", Some("APP"), "TABLE_F", None),
            (
                "OTHER_F.OTHER_TABLE_F",
                Some("OTHER_F"),
                "OTHER_TABLE_F",
                None,
            ),
        ],
        false,
    );
    verify(
        "CREATE FUNCTION F_SQL_QNAME(P_F INTEGER) RETURNS TABLE(C_A INTEGER) LANGUAGE SQL READS SQL DATA BEGIN ATOMIC RETURN SELECT P_F FROM SYSIBM.SYSDUMMY1; END;",
        &[("SYSIBM.SYSDUMMY1", Some("SYSIBM"), "SYSDUMMY1", None)],
        true,
    );
    for source in [
        "SELECT C_A FROM APP.;",
        "SELECT C_A FROM .TABLE_F;",
        "SELECT C_A FROM APP..TABLE_F;",
        "SELECT C_A FROM DATA_F.REMOTE_SCHEMA_F.;",
        "SELECT C_A FROM DATA_F..REMOTE_TABLE_F;",
        "SELECT C_A FROM A_F.B_F.C_F.D_F;",
        "SELECT C_A FROM \"unterminated;",
    ] {
        let recovered =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(!recovered.syntax.diagnostics.is_empty(), "{source}");
        for node in &recovered.syntax.nodes {
            if let Some(span) = &node.span {
                assert!(
                    source.get(span.clone()).is_some(),
                    "{source}: {}",
                    node.kind
                );
            }
        }
        for diagnostic in &recovered.syntax.diagnostics {
            if let Some(span) = &diagnostic.span {
                assert!(
                    source.get(span.clone()).is_some(),
                    "{source}: {diagnostic:?}"
                );
            }
        }
    }
}

#[test]
fn db2_sql_function_direct_return_values_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    #[derive(Default)]
    struct Expected<'a> {
        returned: Option<&'a str>,
        direct_values: Option<&'a str>,
        rows: &'a [&'a str],
        expression: Option<&'a str>,
        parameters: &'a [&'a str],
        columns: &'a [(&'a str, &'a str)],
    }
    let queries: Vec<_> = [
        "//sql_function_body/return_statement",
        "//sql_function_body/return_statement/fullselect/values_clause",
        "//fullselect/values_clause/values_row",
        "//sql_function_body/return_statement/expression",
        "//create_function_sql_scalar_table_or_row_statement/param_decl_list_2/param_decl_2/parameter_name",
        "//sql_function_return_column/column_name",
        "//sql_function_return_column/sql_function_return_data_type",
    ].into_iter().map(|pattern| ProfileQuery {
        pattern: pattern.into(), captures: vec![Vec::new()],
        ..ProfileQuery::default()
    }).collect();
    let original = operation_context();
    let verify = |source: &str, expected: Expected<'_>| {
        let result = parse_profile_queries(Dialect::Db2, source, &queries, &original).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        for query_index in 0..queries.len() {
            let wanted: Vec<_> = match query_index {
                0 => expected.returned.into_iter().collect(),
                1 => expected.direct_values.into_iter().collect(),
                2 => expected.rows.to_vec(),
                3 => expected.expression.into_iter().collect(),
                4 => expected.parameters.to_vec(),
                5 => expected.columns.iter().map(|column| column.0).collect(),
                6 => expected.columns.iter().map(|column| column.1).collect(),
                _ => unreachable!(),
            };
            let matches: Vec<_> = result
                .matches
                .iter()
                .filter(|matched| matched.query_index == query_index)
                .collect();
            assert_eq!(matches.len(), wanted.len(), "{source}: query {query_index}");
            for (matched, wanted) in matches.into_iter().zip(wanted) {
                assert_eq!(matched.captures, [Some(matched.node_index)]);
                let node = &result.syntax.nodes[matched.node_index];
                assert_eq!(node.status, SpanStatus::Regular);
                let span = node.span.clone().expect("regular capture span");
                assert_eq!(
                    source.get(span.clone()),
                    Some(wanted),
                    "{source}: query {query_index}"
                );
                let parent = &result.syntax.nodes[node.parent.expect("capture parent")];
                let parent_span = parent.span.as_ref().expect("regular parent span");
                assert!(parent_span.start <= span.start && span.end <= parent_span.end);
                match query_index {
                    0 => assert_eq!(parent.kind, "sql_function_body"),
                    1 => {
                        assert_eq!(parent.kind, "fullselect");
                        assert_eq!(
                            result.syntax.nodes[parent.parent.unwrap()].kind,
                            "return_statement"
                        );
                    }
                    2 => assert_eq!(parent.kind, "values_clause"),
                    3 => assert_eq!(parent.kind, "return_statement"),
                    4 => assert_eq!(parent.kind, "param_decl_2"),
                    5 | 6 => assert_eq!(parent.kind, "sql_function_return_column"),
                    _ => unreachable!(),
                }
            }
        }
        for (index, node) in result.syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing, "{source}: {}", node.kind);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        assert!(result.syntax.tokens.iter().any(|token| token.eof));
        for token in &result.syntax.tokens {
            if let Some(span) = &token.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        (result.syntax.nodes, result.syntax.tokens)
    };
    verify(
        "CREATE FUNCTION F_DIRECT(P_F INTEGER) RETURNS INTEGER LANGUAGE SQL CONTAINS SQL RETURN P_F+1;",
        Expected {
            returned: Some("RETURN P_F+1"),
            expression: Some("P_F+1"),
            parameters: &["P_F"],
            ..Expected::default()
        },
    );
    verify(
        "CREATE FUNCTION F_NULL() RETURNS INTEGER LANGUAGE SQL CONTAINS SQL RETURN NULL;",
        Expected {
            returned: Some("RETURN NULL"),
            expression: Some("NULL"),
            ..Expected::default()
        },
    );
    verify(
        "CREATE FUNCTION F_ROW(P_OBJ TYPE_F) RETURNS ROW(NAME_F VARCHAR(10),FIRST_F VARCHAR(10)) LANGUAGE SQL CONTAINS SQL RETURN VALUES(P_OBJ..NAME_F,P_OBJ..FIRST_F);",
        Expected {
            returned: Some("RETURN VALUES(P_OBJ..NAME_F,P_OBJ..FIRST_F)"),
            direct_values: Some("VALUES(P_OBJ..NAME_F,P_OBJ..FIRST_F)"),
            rows: &["(P_OBJ..NAME_F,P_OBJ..FIRST_F)"],
            parameters: &["P_OBJ"],
            columns: &[("NAME_F", "VARCHAR(10)"), ("FIRST_F", "VARCHAR(10)")],
            ..Expected::default()
        },
    );
    verify(
        "CREATE FUNCTION F_TABLE() RETURNS TABLE(C_A INTEGER,C_B INTEGER) LANGUAGE SQL CONTAINS SQL RETURN VALUES(11,22),(33,44);",
        Expected {
            returned: Some("RETURN VALUES(11,22),(33,44)"),
            direct_values: Some("VALUES(11,22),(33,44)"),
            rows: &["(11,22)", "(33,44)"],
            columns: &[("C_A", "INTEGER"), ("C_B", "INTEGER")],
            ..Expected::default()
        },
    );
    verify(
        "CREATE FUNCTION F_READ() RETURNS TABLE(C_A INTEGER,C_B INTEGER) LANGUAGE SQL READS SQL DATA RETURN SELECT 11,22 FROM SRC_DUMMY_F;",
        Expected {
            returned: Some("RETURN SELECT 11,22 FROM SRC_DUMMY_F"),
            columns: &[("C_A", "INTEGER"), ("C_B", "INTEGER")],
            ..Expected::default()
        },
    );
    let compound = verify(
        "CREATE FUNCTION F_COMPOUND() RETURNS TABLE(C_A INTEGER,C_B INTEGER) LANGUAGE SQL CONTAINS SQL BEGIN ATOMIC RETURN VALUES(11,22); END;",
        Expected {
            rows: &["(11,22)"],
            columns: &[("C_A", "INTEGER"), ("C_B", "INTEGER")],
            ..Expected::default()
        },
    );
    assert!(
        compound
            .0
            .iter()
            .any(|node| node.kind == "compound_sql_inlined")
    );
    verify(
        "CREATE FUNCTION F_CTE() RETURNS TABLE(C_A INTEGER) LANGUAGE SQL READS SQL DATA RETURN WITH C_F(C_A) AS (VALUES 11) SELECT C_A FROM C_F;",
        Expected {
            returned: Some("RETURN WITH C_F(C_A) AS (VALUES 11) SELECT C_A FROM C_F"),
            rows: &["11"],
            columns: &[("C_A", "INTEGER")],
            ..Expected::default()
        },
    );
    verify(
        "CREATE FUNCTION F_CTE_NESTED() RETURNS TABLE(C_A INTEGER) LANGUAGE SQL READS SQL DATA RETURN WITH C_F(C_A) AS (WITH C_N(C_A) AS (VALUES 11),C_M(C_A) AS (VALUES 22) SELECT C_A FROM C_N) SELECT C_A FROM C_F;",
        Expected {
            returned: Some(
                "RETURN WITH C_F(C_A) AS (WITH C_N(C_A) AS (VALUES 11),C_M(C_A) AS (VALUES 22) SELECT C_A FROM C_N) SELECT C_A FROM C_F",
            ),
            rows: &["11", "22"],
            columns: &[("C_A", "INTEGER")],
            ..Expected::default()
        },
    );
    let scalar = verify(
        "CREATE FUNCTION F_SCALAR_QUERY() RETURNS INTEGER LANGUAGE SQL CONTAINS SQL RETURN(VALUES 11);",
        Expected {
            returned: Some("RETURN(VALUES 11)"),
            expression: Some("(VALUES 11)"),
            rows: &["11"],
            ..Expected::default()
        },
    );
    assert!(scalar.0.iter().any(|node| node.kind == "scalar_fullselect"));
    let raw = verify(
        "-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION F_RAW(P_F INTEGER) RETURNS TABLE(\"Cé\" INTEGER,\"β\" INTEGER) LANGUAGE SQL CONTAINS SQL RETURN /* θ */ VALUES /* ε */ (P_F /* π */,22);",
        Expected {
            returned: Some("RETURN /* θ */ VALUES /* ε */ (P_F /* π */,22)"),
            direct_values: Some("VALUES /* ε */ (P_F /* π */,22)"),
            rows: &["(P_F /* π */,22)"],
            parameters: &["P_F"],
            columns: &[("\"Cé\"", "INTEGER"), ("\"β\"", "INTEGER")],
            ..Expected::default()
        },
    );
    for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
        assert!(
            raw.1
                .iter()
                .any(|token| token.kind == kind && token.channel != 0)
        );
    }
    for (source, rows) in [
        ("VALUES 11;", vec!["11"]),
        ("VALUES 11,22,33;", vec!["11", "22", "33"]),
        ("VALUES(11,22),(33,44);", vec!["(11,22)", "(33,44)"]),
        ("(VALUES 11);", vec!["11"]),
        ("(SELECT 11 FROM SRC_DUMMY_F);", vec![]),
        ("VALUES 11 UNION ALL VALUES 22;", vec!["11", "22"]),
        (
            "SELECT 11 FROM SRC_DUMMY_F UNION ALL VALUES 22;",
            vec!["22"],
        ),
        ("(VALUES 11) INTERSECT (VALUES 11);", vec!["11", "11"]),
        ("VALUES 11 EXCEPT ALL VALUES 22;", vec!["11", "22"]),
        ("CREATE VIEW V_VALUES_F(C_A) AS VALUES 11;", vec!["11"]),
        (
            "CREATE VIEW V_SELECT_F(C_A) AS SELECT 11 FROM SRC_DUMMY_F;",
            vec![],
        ),
        (
            "WITH C_F(C_A) AS (VALUES 11) SELECT C_A FROM C_F;",
            vec!["11"],
        ),
        ("SELECT C_A FROM SRC_DUMMY_F ORDER BY C_A DESC;", vec![]),
        (
            "DELETE FROM TABLE_F ORDER BY C_A OFFSET 2 ROWS FETCH NEXT 3 ROWS ONLY;",
            vec![],
        ),
    ] {
        verify(
            source,
            Expected {
                rows: &rows,
                ..Expected::default()
            },
        );
    }
    let grouped = verify(
        "(VALUES 11);",
        Expected {
            rows: &["11"],
            ..Expected::default()
        },
    );
    let grouped_node = grouped
        .0
        .iter()
        .position(|node| node.kind == "fullselect_in_parentheses")
        .unwrap();
    assert!(
        grouped
            .0
            .iter()
            .any(|node| node.kind == "fullselect" && node.parent == Some(grouped_node))
    );
    for (source, expected) in [
        (
            "VALUES(11,22),(33,44) ORDER BY 2 OFFSET 2 ROWS FETCH NEXT 3 ROWS ONLY WITH UR;",
            Expected {
                rows: &["(11,22)", "(33,44)"],
                ..Expected::default()
            },
        ),
        (
            "CREATE FUNCTION F_ORDER() RETURNS TABLE(C_A INTEGER,C_B INTEGER) LANGUAGE SQL CONTAINS SQL RETURN VALUES(11,22),(33,44) ORDER BY 2 OFFSET 2 ROWS FETCH NEXT 3 ROWS ONLY WITH UR;",
            Expected {
                returned: Some(
                    "RETURN VALUES(11,22),(33,44) ORDER BY 2 OFFSET 2 ROWS FETCH NEXT 3 ROWS ONLY WITH UR",
                ),
                direct_values: Some("VALUES(11,22),(33,44)"),
                rows: &["(11,22)", "(33,44)"],
                columns: &[("C_A", "INTEGER"), ("C_B", "INTEGER")],
                ..Expected::default()
            },
        ),
    ] {
        let syntax = verify(source, expected);
        for kind in [
            "order_by_clause",
            "offset_clause",
            "fetch_clause",
            "isolation_clause",
        ] {
            assert!(
                syntax.0.iter().any(|node| node.kind == kind),
                "{source}: {kind}"
            );
        }
    }
    for source in [
        "VALUES;",
        "VALUES();",
        "VALUES(11,);",
        "VALUES(11;",
        "VALUES 11 UNION ALL;",
        "(VALUES 11 VALUES 22);",
        "CREATE FUNCTION F_BAD() RETURNS INTEGER LANGUAGE SQL CONTAINS SQL RETURN VALUES;",
        "CREATE FUNCTION F_BAD() RETURNS INTEGER LANGUAGE SQL CONTAINS SQL RETURN VALUES(11,);",
        "CREATE FUNCTION F_BAD() RETURNS TABLE(C_A INTEGER) LANGUAGE SQL READS SQL DATA RETURN WITH C_F(C_A) AS () SELECT C_A FROM C_F;",
        "CREATE VIEW V_BAD_F(C_A) VALUES 11;",
        "VALUES 11 ORDER 2;",
        "VALUES 11 BY 2;",
        "VALUES 11 ORDER BY;",
    ] {
        let recovered = parse_profile_queries(Dialect::Db2, source, &queries, &original).unwrap();
        assert!(!recovered.syntax.diagnostics.is_empty(), "{source}");
        assert!(
            recovered
                .syntax
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.phase
                    == graphit_ast::antlr_engine::DiagnosticPhase::Parser),
            "{source}"
        );
        for node in &recovered.syntax.nodes {
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        assert!(
            matches!(
                graphit_ast::antlr_engine::parse_strict(Dialect::Db2, source, &original),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(parse_profile_queries(Dialect::Db2,"VALUES 11;",&queries,&cancelled),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(graphit_ast::antlr_engine::parse_strict(Dialect::Db2,"VALUES 11;",&expired),
        Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn db2_aggregate_interface_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let original = operation_context();
    let queries: Vec<_> = [
        "//create_function_aggregate_interface_statement/function_name",
        "//agg_fn_param_decl/parameter_name",
        "//state_variable_declaration/variable_name",
        "//aggregate_return_clause/aggregate_builtin_type",
        "//aggregate_return_clause/aggregate_cast_target_type",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    let implementation = "AGGREGATE WITH (S_TOTAL DOUBLE,S_COUNT INTEGER) USING INITIALIZE PROCEDURE INIT_F ACCUMULATE PROCEDURE ACC_F MERGE PROCEDURE MERGE_F FINALIZE FUNCTION FINAL_F";
    let anonymous = "AGGREGATE WITH (DOUBLE) USING INITIALIZE PROCEDURE INIT_F ACCUMULATE PROCEDURE ACC_F MERGE PROCEDURE MERGE_F FINALIZE FUNCTION FINAL_F";
    let verify = |source: &str, expected: [&[&str]; 5]| {
        let parsed = parse_profile_queries(Dialect::Db2, source, &queries, &original).unwrap();
        assert!(
            parsed.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            parsed.syntax.diagnostics
        );
        for (query_index, expected) in expected.into_iter().enumerate() {
            let matches: Vec<_> = parsed
                .matches
                .iter()
                .filter(|m| m.query_index == query_index)
                .collect();
            assert_eq!(
                matches.len(),
                expected.len(),
                "{source}: query {query_index}"
            );
            for (matched, expected) in matches.into_iter().zip(expected) {
                assert_eq!(matched.captures, [Some(matched.node_index)]);
                let node = &parsed.syntax.nodes[matched.node_index];
                assert_eq!(node.status, SpanStatus::Regular);
                let span = node.span.as_ref().unwrap();
                assert_eq!(
                    source.get(span.clone()),
                    Some(*expected),
                    "{source}: query {query_index}"
                );
                let parent = &parsed.syntax.nodes[node.parent.unwrap()];
                assert_eq!(
                    parent.kind,
                    match query_index {
                        0 => "create_function_aggregate_interface_statement",
                        1 => "agg_fn_param_decl",
                        2 => "state_variable_declaration",
                        3 | 4 => "aggregate_return_clause",
                        _ => unreachable!(),
                    }
                );
            }
        }
        for (index, node) in parsed.syntax.nodes.iter().enumerate() {
            assert!(!node.error && !node.missing, "{source}: {}", node.kind);
            assert!(node.parent.is_none_or(|parent| parent < index));
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        assert!(parsed.syntax.tokens.iter().any(|token| token.eof));
        for token in &parsed.syntax.tokens {
            if let Some(span) = &token.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
    };
    let named = [
        &["AGG_F"][..],
        &["P_F"][..],
        &["S_TOTAL", "S_COUNT"][..],
        &["DOUBLE"][..],
        &[][..],
    ];
    verify(
        &format!("CREATE FUNCTION AGG_F(IN P_F DOUBLE DEFAULT 1) RETURNS DOUBLE {implementation};"),
        named,
    );
    verify(
        &format!("CREATE FUNCTION AGG_F(IN P_F DOUBLE DEFAULT 1) {implementation} RETURNS DOUBLE;"),
        named,
    );
    verify(
        &format!("CREATE FUNCTION AGG_F() RETURNS DOUBLE {anonymous};"),
        [&["AGG_F"], &[], &[], &["DOUBLE"], &[]],
    );
    verify(
        &format!("CREATE FUNCTION AGG_F(DOUBLE) RETURNS DOUBLE {anonymous};"),
        [&["AGG_F"], &[], &[], &["DOUBLE"], &[]],
    );
    let options = [
        "SPECIFIC SPEC_F",
        "NOT DETERMINISTIC",
        "CALLED ON NULL INPUT",
        "NO EXTERNAL ACTION",
        "NO SCRATCHPAD",
        "NO FINAL CALL",
        "ALLOW PARALLEL",
        "NOT SECURED",
    ];
    verify(
        &format!(
            "CREATE FUNCTION AGG_F(IN P_F DOUBLE DEFAULT 1) RETURNS DOUBLE {} {implementation};",
            options.join(" ")
        ),
        named,
    );
    verify(
        &format!(
            "CREATE FUNCTION AGG_F(IN P_F DOUBLE DEFAULT 1) {implementation} RETURNS DOUBLE {};",
            options.into_iter().rev().collect::<Vec<_>>().join(" ")
        ),
        named,
    );
    verify(
        &format!(
            "CREATE FUNCTION AGG_F(IN P_F DOUBLE) RETURNS DOUBLE DETERMINISTIC EXTERNAL ACTION SECURED {implementation};"
        ),
        named,
    );
    verify(
        &format!(
            "CREATE FUNCTION AGG_F() RETURNS DOUBLE DETERMINISTIC {anonymous}; CREATE FUNCTION AGG_SECOND_F() RETURNS DOUBLE DETERMINISTIC {anonymous};"
        ),
        [
            &["AGG_F", "AGG_SECOND_F"],
            &[],
            &[],
            &["DOUBLE", "DOUBLE"],
            &[],
        ],
    );
    let designators = "AGGREGATE WITH (S_TOTAL DOUBLE,S_COUNT INTEGER) USING IN MODULE MODULE_F INITIALIZE SPECIFIC PROCEDURE APP.INIT_SPEC ACCUMULATE PROCEDURE APP.ACC_F(DOUBLE) MERGE PROCEDURE APP.MERGE_F(DOUBLE) FINALIZE SPECIFIC FUNCTION APP.FINAL_SPEC";
    verify(
        &format!("CREATE FUNCTION AGG_F(IN P_F DOUBLE DEFAULT 1) RETURNS DOUBLE {designators};"),
        named,
    );
    for default in [
        "CURRENT_SCHEMA",
        "-15",
        "NULL",
        "'café'",
        "APP.VAR_F",
        "(1+2)",
    ] {
        verify(
            &format!(
                "CREATE FUNCTION AGG_F(P_F DOUBLE DEFAULT {default}) RETURNS DOUBLE {implementation};"
            ),
            named,
        );
    }
    let types = [
        "SMALLINT",
        "INT",
        "INTEGER",
        "BIGINT",
        "DEC",
        "DECIMAL(1,0)",
        "NUMERIC(5,2)",
        "NUM",
        "DECFLOAT",
        "DECFLOAT(17)",
        "FLOAT",
        "FLOAT(1)",
        "REAL",
        "DOUBLE",
        "DOUBLE PRECISION",
        "CHAR",
        "CHARACTER(1 OCTETS) FOR BIT DATA",
        "VARCHAR(1) FOR BIT DATA",
        "CHAR VARYING(5)",
        "CHARACTER VARYING(5)",
        "CLOB",
        "CHAR LARGE OBJECT(1K OCTETS)",
        "CHARACTER LARGE OBJECT",
        "GRAPHIC",
        "GRAPHIC(1 CODEUNITS16)",
        "VARGRAPHIC(1)",
        "DBCLOB",
        "DBCLOB(1K CODEUNITS16)",
        "NCHAR",
        "NATIONAL CHAR(1)",
        "NATIONAL CHARACTER(1)",
        "NVARCHAR(1)",
        "NCHAR VARYING(1)",
        "NATIONAL CHAR VARYING(1)",
        "NATIONAL CHARACTER VARYING(1)",
        "NCLOB",
        "NCHAR LARGE OBJECT(1K)",
        "NATIONAL CHARACTER LARGE OBJECT",
        "BINARY",
        "BINARY(1)",
        "VARBINARY(1)",
        "BINARY VARYING(1)",
        "BLOB",
        "BINARY LARGE OBJECT(1K)",
        "DATE",
        "TIME",
        "TIMESTAMP",
        "TIMESTAMP(0)",
        "XML",
        "DB2SECURITYLABEL",
        "SYSPROC.DB2SECURITYLABEL",
    ];
    for ty in types {
        let state = format!(
            "AGGREGATE WITH (S_F {ty}) USING INITIALIZE PROCEDURE INIT_F ACCUMULATE PROCEDURE ACC_F MERGE PROCEDURE MERGE_F FINALIZE FUNCTION FINAL_F"
        );
        verify(
            &format!("CREATE FUNCTION AGG_F(P_F {ty}) RETURNS {ty} {state};"),
            [&["AGG_F"], &["P_F"], &["S_F"], &[ty], &[]],
        );
    }
    for target in [
        "DEC()",
        "DECIMAL()",
        "NUMERIC()",
        "NUM()",
        "DECFLOAT()",
        "CHAR()",
        "CHARACTER() FOR BIT DATA",
        "VARCHAR()",
        "CHAR VARYING()",
        "CHARACTER VARYING()",
        "CLOB()",
        "CHAR LARGE OBJECT()",
        "CHARACTER LARGE OBJECT()",
        "GRAPHIC()",
        "VARGRAPHIC()",
        "DBCLOB()",
        "NCHAR()",
        "NATIONAL CHAR()",
        "NATIONAL CHARACTER()",
        "NVARCHAR()",
        "NCHAR VARYING()",
        "NATIONAL CHAR VARYING()",
        "NATIONAL CHARACTER VARYING()",
        "NCLOB()",
        "NCHAR LARGE OBJECT()",
        "NATIONAL CHARACTER LARGE OBJECT()",
        "BINARY()",
        "VARBINARY()",
        "BINARY VARYING()",
        "BLOB()",
        "BINARY LARGE OBJECT()",
        "TIMESTAMP()",
        "CHAR(10)",
    ] {
        verify(
            &format!("CREATE FUNCTION AGG_F() RETURNS {target} CAST FROM CHAR(10) {anonymous};"),
            [&["AGG_F"], &[], &[], &["CHAR(10)"], &[target]],
        );
    }
    verify(
        &format!(
            "-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION \"APP\".\"AGGé\"(IN \"Pé\" DOUBLE DEFAULT 1) RETURNS DOUBLE {implementation};"
        ),
        [
            &["\"APP\".\"AGGé\""],
            &["\"Pé\""],
            &["S_TOTAL", "S_COUNT"],
            &["DOUBLE"],
            &[],
        ],
    );
    let mut invalid = vec![
        format!("CREATE FUNCTION AGG_F() {anonymous};"),
        "CREATE FUNCTION AGG_F() RETURNS DOUBLE;".into(),
        format!("CREATE FUNCTION AGG_F RETURNS DOUBLE {anonymous};"),
        format!("CREATE FUNCTION AGG_F(IN DOUBLE) RETURNS DOUBLE {anonymous};"),
        format!("CREATE FUNCTION AGG_F(OUT P_F DOUBLE) RETURNS DOUBLE {anonymous};"),
        format!("CREATE FUNCTION AGG_F(P_F DOUBLE DEFAULT) RETURNS DOUBLE {anonymous};"),
        format!("CREATE FUNCTION AGG_F(P_F DOUBLE WITH DEFAULT 1) RETURNS DOUBLE {anonymous};"),
        format!("CREATE FUNCTION AGG_F(P_F DOUBLE DEFAULT 1+2) RETURNS DOUBLE {anonymous};"),
        "CREATE FUNCTION AGG_F() RETURNS DOUBLE AGGREGATE WITH () USING INITIALIZE PROCEDURE INIT_F ACCUMULATE PROCEDURE ACC_F MERGE PROCEDURE MERGE_F FINALIZE FUNCTION FINAL_F;".into(),
        "CREATE FUNCTION AGG_F() RETURNS DOUBLE AGGREGATE WITH (S_F DOUBLE DEFAULT 1) USING INITIALIZE PROCEDURE INIT_F ACCUMULATE PROCEDURE ACC_F MERGE PROCEDURE MERGE_F FINALIZE FUNCTION FINAL_F;".into(),
        "CREATE FUNCTION AGG_F() RETURNS DOUBLE AGGREGATE WITH (DOUBLE) USING ACCUMULATE PROCEDURE ACC_F INITIALIZE PROCEDURE INIT_F MERGE PROCEDURE MERGE_F FINALIZE FUNCTION FINAL_F;".into(),
        "CREATE FUNCTION AGG_F() RETURNS DOUBLE AGGREGATE WITH (DOUBLE) USING INITIALIZE PROCEDURE INIT_F ACCUMULATE PROCEDURE ACC_F FINALIZE FUNCTION FINAL_F;".into(),
        format!("CREATE FUNCTION AGG_F() RETURNS DOUBLE LANGUAGE C {anonymous};"),
        format!("CREATE FUNCTION AGG_F() RETURNS DOUBLE FENCED {anonymous};"),
        format!("CREATE FUNCTION AGG_F() RETURNS DOUBLE {anonymous} EXTRA_F;"),
        format!("CREATE FUNCTION AGG_F() RETURNS FLOAT() CAST FROM CHAR(10) {anonymous};"),
        format!("CREATE FUNCTION AGG_F() RETURNS VARCHAR() {anonymous};"),
        format!("CREATE FUNCTION AGG_F() RETURNS CHAR(10) CAST FROM VARCHAR() {anonymous};"),
    ];
    for option in options {
        invalid.push(format!(
            "CREATE FUNCTION AGG_F() RETURNS DOUBLE {option} {option} {anonymous};"
        ));
    }
    for pair in [
        "DETERMINISTIC NOT DETERMINISTIC",
        "SECURED NOT SECURED",
        "EXTERNAL ACTION NO EXTERNAL ACTION",
    ] {
        invalid.push(format!(
            "CREATE FUNCTION AGG_F() RETURNS DOUBLE {pair} {anonymous};"
        ));
    }
    for ty in [
        "BOOLEAN",
        "CURSOR",
        "ANCHOR TO V_F",
        "REF(TYPE_F)",
        "TYPE_F",
        "VARCHAR",
        "VARGRAPHIC",
        "VARCHAR()",
        "FLOAT()",
    ] {
        invalid.push(format!(
            "CREATE FUNCTION AGG_F(P_F {ty}) RETURNS DOUBLE {anonymous};"
        ));
    }
    for source in invalid {
        let recovered = parse_profile_queries(Dialect::Db2, &source, &queries, &original).unwrap();
        assert!(!recovered.syntax.diagnostics.is_empty(), "{source}");
        assert!(
            recovered
                .syntax
                .diagnostics
                .iter()
                .any(|d| d.phase == graphit_ast::antlr_engine::DiagnosticPhase::Parser),
            "{source}"
        );
        for node in &recovered.syntax.nodes {
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some());
            }
        }
        assert!(
            matches!(
                graphit_ast::antlr_engine::parse_strict(Dialect::Db2, &source, &original),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(parse_profile_queries(Dialect::Db2,"CREATE FUNCTION AGG_F() RETURNS DOUBLE;",&queries,&cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(parse_profile_queries(Dialect::Db2,"CREATE FUNCTION AGG_F() RETURNS DOUBLE;",&queries,&expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn tsql_revoke_permissions_native() {
    use graphit_ast::{
        antlr_engine::{
            DiagnosticPhase, NodeClass, ProfileQuery, parse_profile_queries,
            semantic::{CaptureBinding, SemanticError, parse_semantic_occurrences},
        },
        catalog::{
            CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind,
        },
        prepared_input::{ByteSpan, OriginPlan, OriginSegment, OriginalProjection, PreparedInput},
        profiles::{
            ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput, QueryKind,
        },
        scalar_policy::{RegexLimits, ScalarPolicy},
    };
    fn catalog(bytes: &[u8], original: &graphit_core::file_io::SnapshotContext) -> CatalogDraft {
        let id = ProfileId::new("tsql-revoke-fixture", 256).unwrap();
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
                original,
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
            original,
        )
        .unwrap()
    }
    let yaml = include_bytes!("fixtures/catalog/shipped/tsql.yaml");
    assert_eq!(yaml, include_bytes!("fixtures/profiles/shipped/tsql.yaml"));
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/tsql.yaml")
    );
    let mut fixture: serde_json::Value = serde_saphyr::from_str_with_options(
        std::str::from_utf8(yaml).unwrap(),
        serde_saphyr::options! {
            duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
            merge_keys: serde_saphyr::MergeKeyPolicy::Error,
            strict_booleans: true,
        },
    )
    .unwrap();
    let revokes: Vec<_> = fixture["queries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|query| query["data_key"] == "revokes")
        .cloned()
        .collect();
    assert_eq!(revokes.len(), 1);
    assert_eq!(revokes[0]["pattern"], "//revoke_permission//table_name");
    fixture["queries"] = serde_json::Value::Array(revokes);
    let original = operation_context();
    let catalog = catalog(&serde_json::to_vec(&fixture).unwrap(), &original);
    let profile = &catalog.profiles()[0];
    let query = profile.queries().value().as_slice()[0].value();
    assert_eq!(*query.r#type().value(), QueryKind::Relation);
    assert_eq!(query.data_key().value(), "revokes");
    assert_eq!(query.graph_label().value(), "");
    assert_eq!(query.relation_type().value(), "REFERENCES");
    // Structured capture steps below select the relation target; no legacy name_capture.
    let raw_queries = [ProfileQuery {
        pattern: query.pattern().value().clone(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    }];
    for (source, expected) in [
        (
            "REVOKE SELECT ON OBJECT::dbo.pedido FROM leitor;",
            vec!["dbo.pedido"],
        ),
        (
            "REVOKE GRANT OPTION FOR SELECT, UPDATE (nome, id) ON OBJECT::[dbo].[Árvore] FROM [leitor], PUBLIC CASCADE AS [dono];",
            vec!["[dbo].[Árvore]"],
        ),
        (
            "REVOKE REFERENCES ON dbo.pedido(id, nome) TO leitor;",
            vec!["dbo.pedido"],
        ),
        (
            "REVOKE ALL PRIVILEGES ON OBJECT::dbo.pedido FROM leitor;",
            vec!["dbo.pedido"],
        ),
        ("REVOKE CONNECT FROM leitor;", vec![]),
        (
            "REVOKE CONTROL SERVER FROM operador CASCADE AS dbo;",
            vec![],
        ),
        ("REVOKE SELECT ON SCHEMA::Sales TO Vendors;", vec!["Sales"]),
        ("REVOKE ALTER ON DATABASE::db1 FROM leitor;", vec!["db1"]),
        (
            "-- café\r\nReVoKe SELECT ON OBJECT::\"dbo\".\"Árvore\" FROM \"Leitór\";",
            vec!["\"dbo\".\"Árvore\""],
        ),
        (
            "GRANT SELECT ON OBJECT::dbo.concedida TO leitor; REVOKE SELECT ON OBJECT::dbo.retirada FROM leitor; REVERT; SELECT N'REVOKE SELECT ON dbo.falsa FROM leitor';",
            vec!["dbo.retirada"],
        ),
        ("GRANT CONNECT TO leitor; REVERT; SELECT 'REVOKE';", vec![]),
    ] {
        let bound = ByteSpan {
            start: 0,
            end: u64::try_from(source.len()).unwrap(),
        };
        let segments = [OriginSegment::Direct {
            prepared: bound,
            original: bound,
        }];
        let input = PreparedInput::new(
            source.as_bytes(),
            source.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &[],
                anchors: &[],
            },
            &original,
        )
        .unwrap();
        let document = parse_semantic_occurrences(profile, &input).unwrap();
        assert!(
            document.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            document.syntax.diagnostics
        );
        let actual: Vec<_> = document
            .occurrences
            .iter()
            .map(|occurrence| {
                assert!(std::ptr::eq(occurrence.query, query));
                let CaptureBinding::Present(name) = &occurrence.name else {
                    panic!("missing target")
                };
                assert_eq!(name.node_index, occurrence.matched.node_index);
                let node = &document.syntax.nodes[name.node_index];
                assert_eq!(node.class, NodeClass::Rule);
                assert_eq!(node.kind, "table_name");
                let text = name.prepared.as_ref().unwrap().text();
                assert_eq!(&source[node.span.clone().unwrap()], text);
                let OriginalProjection::Exact(range) = name.original.as_ref().unwrap() else {
                    panic!("expected exact original target")
                };
                assert_eq!(range.text(), text);
                let parent = &document.syntax.nodes[node.parent.unwrap()];
                assert_eq!(parent.kind, "revoke_permission");
                text
            })
            .collect();
        assert_eq!(actual, expected, "{source}");
        for node in &document.syntax.nodes {
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some(), "{source}");
            }
        }
        assert!(
            graphit_ast::antlr_engine::parse_strict(Dialect::TSql, source, &original).is_ok(),
            "{source}"
        );
    }
    for source in [
        "REVOKE;",
        "REVOKE FROM leitor;",
        "REVOKE GRANT FOR SELECT ON dbo.pedido FROM leitor;",
        "REVOKE SELECT, ON dbo.pedido FROM leitor;",
        "REVOKE SELECT ON FROM leitor;",
        "REVOKE SELECT ON dbo.pedido FROM;",
        "REVOKE SELECT ON dbo.pedido FROM leitor,;",
        "REVOKE SELECT() ON dbo.pedido FROM leitor;",
        "REVOKE UPDATE(id,) ON dbo.pedido FROM leitor;",
        "REVOKE SELECT ON dbo.pedido() FROM leitor;",
        "REVOKE SELECT ON OBJECT dbo.pedido FROM leitor;",
        "REVOKE SELECT ON dbo.pedido FROM leitor AS dono CASCADE;",
        "REVOKE FROBNICATE ON dbo.pedido FROM leitor;",
    ] {
        let recovered =
            parse_profile_queries(Dialect::TSql, source, &raw_queries, &original).unwrap();
        assert!(
            recovered
                .syntax
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.phase == DiagnosticPhase::Parser),
            "{source}"
        );
        for node in &recovered.syntax.nodes {
            if let Some(span) = &node.span {
                assert!(source.get(span.clone()).is_some(), "{source}");
            }
        }
        assert!(
            matches!(
                graphit_ast::antlr_engine::parse_strict(Dialect::TSql, source, &original),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
    let cancelled = operation_context();
    cancelled.cancellation().cancel();
    let valid = "REVOKE SELECT ON dbo.pedido FROM leitor;";
    assert!(
        matches!(parse_profile_queries(Dialect::TSql, valid, &raw_queries, &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert!(
        matches!(parse_profile_queries(Dialect::TSql, valid, &raw_queries, &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    let context = operation_context();
    let bound = ByteSpan {
        start: 0,
        end: u64::try_from(valid.len()).unwrap(),
    };
    let segments = [OriginSegment::Direct {
        prepared: bound,
        original: bound,
    }];
    let input = PreparedInput::new(
        valid.as_bytes(),
        valid.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &context,
    )
    .unwrap();
    context.cancellation().cancel();
    assert!(
        matches!(parse_semantic_occurrences(profile, &input), Err(SemanticError::Operation(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
}

#[test]
fn db2_method_cast_explicit_xml_rejected_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let statement = |returns: &str, family: usize| {
        let body = match family {
            0 => {
                format!("CREATE METHOD METHOD_F() {returns} FOR TYPE_F EXTERNAL NAME 'pkg.method';")
            }
            1 => format!(
                "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F() {returns} LANGUAGE SQL;"
            ),
            2 => format!("ALTER TYPE TYPE_F ADD METHOD METHOD_F() {returns} LANGUAGE SQL;"),
            _ => unreachable!(),
        };
        format!("-- α\r\n/* β /* γ */ δ */ {body}")
    };

    for family in 0..3 {
        for returns in [
            "RETURNS INTEGER CAST FROM SMALLINT",
            "RETURNS VARCHAR() CAST FROM VARCHAR(40)",
            "RETURNS DECIMAL(12,2) CAST FROM DOUBLE",
            "RETURNS \"XML\" CAST FROM INTEGER",
            "RETURNS INTEGER CAST FROM \"XML\"",
            "RETURNS \"XML\" CAST FROM \"XML\"",
        ] {
            let source = statement(returns, family);
            let syntax =
                parse(Dialect::Db2, &source).unwrap_or_else(|e| panic!("preserved {source}: {e}"));
            assert!(syntax.diagnostics.is_empty());
            assert!(syntax.tokens.iter().any(|t| t.eof));
        }
    }
    {
        let source = "ALTER TABLE TYPE_F ADD COLUMN ATTRIBUTE_F XML;";
        let syntax = parse(Dialect::Db2, source).unwrap_or_else(|e| panic!("global {source}: {e}"));
        assert!(syntax.nodes.iter().any(|n| {
            n.kind == "built_in_type"
                && n.span
                    .as_ref()
                    .is_some_and(|s| source.get(s.clone()) == Some("XML"))
        }));
    }
    println!("baseline and preserved builtin/inferred/quoted-nominal/global XML controls passed");

    let queries: Vec<_> = [
        "//method_signature/method_cast_target_type",
        "//method_specification/method_specification_return_clause/method_cast_target_type",
        "//method_signature/method_cast_source_type",
        "//method_specification/method_specification_return_clause/method_cast_source_type",
        "//method_cast_non_xml_type/method_cast_non_xml_builtin_type",
        "//method_cast_target_type/inferred_builtin_type",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    for family in 0..3 {
        for (target, from) in [
            ("INTEGER", "SMALLINT"),
            ("VARCHAR()", "VARCHAR(40)"),
            ("DECIMAL(12,2)", "DOUBLE"),
            ("\"XML\"", "INTEGER"),
            ("INTEGER", "\"XML\""),
            ("\"XML\"", "\"XML\""),
            ("VARCHAR(/* β /* γ */ δ */)", "VARCHAR(40)"),
        ] {
            let source = statement(&format!("RETURNS {target} CAST FROM {from}"), family);
            let result =
                parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                    .unwrap();
            assert!(
                result.syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                result.syntax.diagnostics
            );
            for (query_index, expected, kind) in [
                (usize::from(family != 0), target, "method_cast_target_type"),
                (
                    2 + usize::from(family != 0),
                    from,
                    "method_cast_source_type",
                ),
            ] {
                let matches = result
                    .matches
                    .iter()
                    .filter(|m| m.query_index == query_index)
                    .collect::<Vec<_>>();
                assert_eq!(matches.len(), 1, "{source}");
                let matched = matches[0];
                assert_eq!(matched.captures, [Some(matched.node_index)]);
                let node = &result.syntax.nodes[matched.node_index];
                assert_eq!(node.kind, kind);
                assert_eq!(source.get(node.span.clone().unwrap()), Some(expected));
                assert_eq!(
                    result.syntax.nodes[node.parent.unwrap()].kind,
                    if family == 0 {
                        "method_signature"
                    } else {
                        "method_specification_return_clause"
                    }
                );
                let child = result
                    .syntax
                    .nodes
                    .iter()
                    .find(|n| n.parent == Some(matched.node_index))
                    .unwrap();
                assert_eq!(
                    child.kind,
                    if target.contains('(')
                        && kind == "method_cast_target_type"
                        && target.starts_with("VARCHAR(")
                    {
                        "inferred_builtin_type"
                    } else {
                        "method_cast_non_xml_type"
                    }
                );
                assert_eq!(source.get(child.span.clone().unwrap()), Some(expected));
            }
            for token in &result.syntax.tokens {
                if token.eof {
                    assert_eq!(token.status, SpanStatus::Eof);
                    assert_eq!(token.span, Some(source.len()..source.len()));
                } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                    assert_eq!(source.get(span.clone()), Some(text.as_str()));
                }
            }
            assert!(!result.syntax.nodes.iter().any(|n| n.kind == "data_type_4"));
        }
    }
    let mut accepted = Vec::new();
    for family in 0..3 {
        for returns in [
            "RETURNS XML CAST FROM INTEGER",
            "RETURNS INTEGER CAST FROM XML",
            "RETURNS XML CAST FROM XML",
            "RETURNS xml CAST FROM INTEGER",
            "RETURNS INTEGER CAST FROM xml",
        ] {
            let source = statement(returns, family);
            let raw =
                parse_profile_queries(Dialect::Db2, &source, &[], &operation_context()).unwrap();
            if raw.syntax.diagnostics.is_empty() || parse(Dialect::Db2, &source).is_ok() {
                accepted.push(source);
            } else {
                assert!(matches!(
                    parse(Dialect::Db2, &source),
                    Err(AntlrEngineError::Syntax { .. })
                ));
                assert!(raw.syntax.tokens.iter().any(|t| t.kind == "XML"));
                for diagnostic in &raw.syntax.diagnostics {
                    assert_eq!(diagnostic.grammar, "Db2Parser.g4");
                    assert!(!diagnostic.message.is_empty());
                    assert!(
                        diagnostic
                            .span
                            .as_ref()
                            .is_some_and(|span| source.get(span.clone()).is_some())
                    );
                    assert!(diagnostic.line >= 2);
                }
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "native parser accepted Method CAST XML: {accepted:?}"
    );
}

#[test]
fn db2_method_signature_parameter_declarations_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let queries: Vec<_> = [
        "//method_signature/method_param_list/method_signature_parameter_decl",
        "//method_signature/method_param_list/method_signature_parameter_decl/parameter_name",
        "//method_specification/method_spec_parameter_list_paren/method_spec_parameter_list/method_spec_parameter_decl",
    ].into_iter().map(|pattern| ProfileQuery {
        pattern: pattern.into(), captures: vec![Vec::new()],
        ..ProfileQuery::default()
    }).collect();
    let signature = |params: &str| {
        format!(
            "-- α\r\n/* β /* γ */ δ */ CREATE METHOD METHOD_F({params}) RETURNS INTEGER FOR TYPE_F EXTERNAL NAME 'pkg.method';"
        )
    };
    for (params, declarations, names) in [
        (
            "P_F INTEGER,P_LOC CLOB AS LOCATOR",
            vec!["P_F INTEGER", "P_LOC CLOB AS LOCATOR"],
            vec!["P_F", "P_LOC"],
        ),
        (
            "INTEGER,P_F REF(TYPE_F),CLOB AS LOCATOR",
            vec!["INTEGER", "P_F REF(TYPE_F)", "CLOB AS LOCATOR"],
            vec!["P_F"],
        ),
        ("", vec![], vec![]),
        (
            "\"Pé\" /* ε /* ζ */ η */ INTEGER,\r\nP_F VARCHAR(40)",
            vec!["\"Pé\" /* ε /* ζ */ η */ INTEGER", "P_F VARCHAR(40)"],
            vec!["\"Pé\"", "P_F"],
        ),
        (
            "P_F INTEGER,P_F CLOB",
            vec!["P_F INTEGER", "P_F CLOB"],
            vec!["P_F", "P_F"],
        ),
    ] {
        let source = signature(params);
        let result =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            result.syntax.diagnostics
        );
        let slots = result
            .matches
            .iter()
            .filter(|m| m.query_index == 0)
            .collect::<Vec<_>>();
        let observed = slots
            .iter()
            .map(|m| {
                assert_eq!(m.captures, [Some(m.node_index)]);
                let node = &result.syntax.nodes[m.node_index];
                assert_eq!(node.kind, "method_signature_parameter_decl");
                assert_eq!(node.status, SpanStatus::Regular);
                assert_eq!(
                    result.syntax.nodes[node.parent.unwrap()].kind,
                    "method_param_list"
                );
                let children = result
                    .syntax
                    .nodes
                    .iter()
                    .filter(|n| n.parent == Some(m.node_index))
                    .collect::<Vec<_>>();
                assert_eq!(
                    children
                        .iter()
                        .filter(|n| n.kind == "method_parameter_type")
                        .count(),
                    1
                );
                assert_eq!(
                    children.iter().filter(|n| n.kind == "as_locator").count(),
                    usize::from(source[node.span.clone().unwrap()].ends_with("AS LOCATOR"))
                );
                source.get(node.span.clone().unwrap()).unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(observed, declarations, "{source}");
        for pair in slots.windows(2) {
            assert_ne!(pair[0].node_index, pair[1].node_index);
            let a = result.syntax.nodes[pair[0].node_index]
                .span
                .as_ref()
                .unwrap();
            let b = result.syntax.nodes[pair[1].node_index]
                .span
                .as_ref()
                .unwrap();
            assert!(a.end < b.start);
        }
        let observed_names = result
            .matches
            .iter()
            .filter(|m| m.query_index == 1)
            .map(|m| {
                let node = &result.syntax.nodes[m.node_index];
                assert_eq!(node.kind, "parameter_name");
                let parent = node.parent.unwrap();
                assert!(slots.iter().any(|slot| slot.node_index == parent));
                source.get(node.span.clone().unwrap()).unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(observed_names, names, "{source}");
        assert_eq!(
            result
                .syntax
                .nodes
                .iter()
                .filter(|n| n.kind == "method_param_list")
                .count(),
            usize::from(!declarations.is_empty())
        );
        assert!(!result.matches.iter().any(|m| m.query_index == 2));
        for token in &result.syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
        for kind in ["SQL_COMMENT", "LINE_COMMENT"] {
            assert!(
                result
                    .syntax
                    .tokens
                    .iter()
                    .any(|t| t.kind == kind && t.channel != 0)
            );
        }
    }
    for source in [
        "CREATE TYPE TYPE_F AS(ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(P_F INTEGER) RETURNS INTEGER LANGUAGE SQL;",
        "ALTER TYPE TYPE_F ADD METHOD METHOD_F(P_F INTEGER) RETURNS INTEGER LANGUAGE SQL;",
    ] {
        let result =
            parse_profile_queries(Dialect::Db2, source, &queries, &operation_context()).unwrap();
        assert!(result.syntax.diagnostics.is_empty());
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].query_index, 2);
        let node = &result.syntax.nodes[result.matches[0].node_index];
        assert_eq!(source.get(node.span.clone().unwrap()), Some("P_F INTEGER"));
        assert!(
            !result
                .syntax
                .nodes
                .iter()
                .any(|n| n.kind == "method_signature_parameter_decl")
        );
    }
    for params in [",INTEGER", "P_F INTEGER,", "P_F REF()", "P_F CLOB AS"] {
        let source = signature(params);
        let raw = parse_profile_queries(Dialect::Db2, &source, &[], &operation_context()).unwrap();
        assert!(!raw.syntax.diagnostics.is_empty(), "{source}");
        assert!(
            matches!(
                parse(Dialect::Db2, &source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
        for diagnostic in &raw.syntax.diagnostics {
            assert_eq!(diagnostic.grammar, "Db2Parser.g4");
            assert!(!diagnostic.message.is_empty());
            assert!(
                diagnostic
                    .span
                    .as_ref()
                    .is_some_and(|s| source.get(s.clone()).is_some())
            );
        }
    }
}

#[test]
fn db2_optional_array_cardinality_and_table_options_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let queries: Vec<_> = [
        "//create_type_array_statement/type_name",
        "//create_table_statement/table_name",
        "//create_table_statement//column_definition/column_name",
        "//create_type_array_statement/integer_constant",
        "//create_type_array_statement/data_type_2",
        "//create_table_statement/create_table_opts",
    ]
    .into_iter()
    .map(|pattern| ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    })
    .collect();
    let mut gaps = Vec::new();
    for (body, arrays, tables, columns, cardinalities, indices, options) in [
        (
            "CREATE TYPE ARRAY_EMPTY_F AS INTEGER ARRAY[];",
            vec!["ARRAY_EMPTY_F"],
            vec![],
            vec![],
            0,
            0,
            0,
        ),
        (
            "CREATE TYPE ARRAY_BOUND_F AS INTEGER ARRAY[100];",
            vec!["ARRAY_BOUND_F"],
            vec![],
            vec![],
            1,
            0,
            0,
        ),
        (
            "CREATE TYPE ARRAY_ASSOC_F AS INTEGER ARRAY[INTEGER];",
            vec!["ARRAY_ASSOC_F"],
            vec![],
            vec![],
            0,
            1,
            0,
        ),
        (
            "CREATE TYPE ARRAY_CHAR_F AS INTEGER ARRAY[VARCHAR(20)];",
            vec!["ARRAY_CHAR_F"],
            vec![],
            vec![],
            0,
            1,
            0,
        ),
        (
            "CREATE TABLE TABLE_F(COLUMN_F INTEGER);",
            vec![],
            vec!["TABLE_F"],
            vec!["COLUMN_F"],
            0,
            0,
            0,
        ),
        (
            "CREATE TABLE \"Té\"(\"Cé\" INTEGER DEFAULT 1);",
            vec![],
            vec!["\"Té\""],
            vec!["\"Cé\""],
            0,
            0,
            0,
        ),
        (
            "CREATE TABLE TABLE_OPTS_F(COLUMN_F INTEGER) DATA CAPTURE NONE;",
            vec![],
            vec!["TABLE_OPTS_F"],
            vec!["COLUMN_F"],
            0,
            0,
            1,
        ),
        (
            "CREATE TABLE TABLE_MULTI_F(COLUMN_F INTEGER) DATA CAPTURE NONE COMPRESS NO;",
            vec![],
            vec!["TABLE_MULTI_F"],
            vec!["COLUMN_F"],
            0,
            0,
            2,
        ),
        (
            "CREATE TABLE TABLE_LIKE_F LIKE TABLE_F;",
            vec![],
            vec!["TABLE_LIKE_F"],
            vec![],
            0,
            0,
            0,
        ),
        (
            "CREATE TYPE ARRAY_F AS INTEGER ARRAY[]; CREATE TABLE TABLE_F(COLUMN_F INTEGER);",
            vec!["ARRAY_F"],
            vec!["TABLE_F"],
            vec!["COLUMN_F"],
            0,
            0,
            0,
        ),
    ] {
        let source = format!("-- declaração α\r\n/* β /* γ */ δ */ {body}");
        let parsed =
            parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context()).unwrap();
        if !parsed.syntax.diagnostics.is_empty() {
            gaps.push(format!("{body}: {:?}", parsed.syntax.diagnostics));
            continue;
        }
        for (query_index, expected, kind, parent_kind) in [
            (0, arrays, "type_name", "create_type_array_statement"),
            (1, tables, "table_name", "create_table_statement"),
            (2, columns, "column_name", "column_definition"),
        ] {
            let names = parsed
                .matches
                .iter()
                .filter(|m| m.query_index == query_index)
                .map(|m| {
                    assert_eq!(m.captures, [Some(m.node_index)]);
                    let node = &parsed.syntax.nodes[m.node_index];
                    assert_eq!(node.kind, kind);
                    assert_eq!(node.status, SpanStatus::Regular);
                    assert_eq!(parsed.syntax.nodes[node.parent.unwrap()].kind, parent_kind);
                    source.get(node.span.clone().unwrap()).unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(names, expected, "{source}");
        }
        for (query_index, expected) in [(3, cardinalities), (4, indices), (5, options)] {
            assert_eq!(
                parsed
                    .matches
                    .iter()
                    .filter(|m| m.query_index == query_index)
                    .count(),
                expected,
                "{source}"
            );
        }
        for token in &parsed.syntax.tokens {
            if token.eof {
                assert_eq!(token.status, SpanStatus::Eof);
                assert_eq!(token.span, Some(source.len()..source.len()));
            } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
        for kind in ["SQL_COMMENT", "LINE_COMMENT"] {
            assert!(
                parsed
                    .syntax
                    .tokens
                    .iter()
                    .any(|t| t.kind == kind && t.channel != 0)
            );
        }
    }
    assert!(
        gaps.is_empty(),
        "documented array/table syntax gaps:\n{}",
        gaps.join("\n")
    );
    for source in [
        "CREATE TYPE ARRAY_F AS INTEGER ARRAY;",
        "CREATE TYPE ARRAY_F AS INTEGER ARRAY[,];",
        "CREATE TYPE ARRAY_F AS INTEGER ARRAY[10,20];",
        "CREATE TABLE TABLE_F();",
        "CREATE TABLE TABLE_F(COLUMN_F INTEGER,);",
        "CREATE TABLE TABLE_F(COLUMN_F INTEGER) DATA CAPTURE;",
        "CREATE TABLE TABLE_F COLUMN_F INTEGER DATA CAPTURE NONE;",
    ] {
        let parsed =
            parse_profile_queries(Dialect::Db2, source, &[], &operation_context()).unwrap();
        assert!(!parsed.syntax.diagnostics.is_empty(), "{source}");
        assert!(
            matches!(
                parse(Dialect::Db2, source),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
        for diagnostic in &parsed.syntax.diagnostics {
            assert_eq!(diagnostic.grammar, "Db2Parser.g4");
            assert!(!diagnostic.message.is_empty());
            assert!(
                diagnostic
                    .span
                    .as_ref()
                    .is_some_and(|s| source.get(s.clone()).is_some())
            );
        }
    }
}

#[test]
fn db2_method_cast_reference_constructor_native() {
    use graphit_ast::antlr_engine::{ProfileQuery, SpanStatus, parse_profile_queries};
    let statement = |target: &str, from: &str, family: usize| {
        let returns = format!("RETURNS {target} CAST FROM {from}");
        let body = match family {
            0 => {
                format!("CREATE METHOD METHOD_F() {returns} FOR TYPE_F EXTERNAL NAME 'pkg.method';")
            }
            1 => format!(
                "CREATE TYPE TYPE_F AS(ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F() {returns} LANGUAGE SQL;"
            ),
            2 => format!("ALTER TYPE TYPE_F ADD METHOD METHOD_F() {returns} LANGUAGE SQL;"),
            _ => unreachable!(),
        };
        format!("-- α\r\n{body}")
    };
    let mut rejected = Vec::new();
    for family in 0..3 {
        for (target, from, expected) in [
            (
                "REF(APP.TYPE_REF_F)",
                "INTEGER",
                vec![(
                    "method_cast_target_type",
                    "REF(APP.TYPE_REF_F)",
                    "APP.TYPE_REF_F",
                )],
            ),
            (
                "INTEGER",
                "REF(APP.TYPE_REF_F)",
                vec![(
                    "method_cast_source_type",
                    "REF(APP.TYPE_REF_F)",
                    "APP.TYPE_REF_F",
                )],
            ),
            (
                "REF(APP.TYPE_REF_F)",
                "REF(APP.TYPE_OTHER_F)",
                vec![
                    (
                        "method_cast_target_type",
                        "REF(APP.TYPE_REF_F)",
                        "APP.TYPE_REF_F",
                    ),
                    (
                        "method_cast_source_type",
                        "REF(APP.TYPE_OTHER_F)",
                        "APP.TYPE_OTHER_F",
                    ),
                ],
            ),
            (
                "REF(/* α /* β */ γ */ \"Sché\".\"Réf\")",
                "INTEGER",
                vec![(
                    "method_cast_target_type",
                    "REF(/* α /* β */ γ */ \"Sché\".\"Réf\")",
                    "\"Sché\".\"Réf\"",
                )],
            ),
        ] {
            let source = statement(target, from, family);
            let raw =
                parse_profile_queries(Dialect::Db2, &source, &[], &operation_context()).unwrap();
            if !raw.syntax.diagnostics.is_empty() {
                rejected.push((
                    family,
                    target,
                    from,
                    format!("{:?}", raw.syntax.diagnostics),
                ));
                continue;
            }
            // The baseline reaches this new selector only when the genuine parser accepts REF.
            let queries: Vec<_> = [
                "//method_reference_type",
                "//method_reference_type/method_named_type",
                "//method_signature/method_cast_target_type",
                "//method_signature/method_cast_source_type",
                "//method_specification/method_specification_return_clause/method_cast_target_type",
                "//method_specification/method_specification_return_clause/method_cast_source_type",
            ]
            .into_iter()
            .map(|pattern| ProfileQuery {
                pattern: pattern.into(),
                captures: vec![Vec::new()],
                ..ProfileQuery::default()
            })
            .collect();
            let result =
                parse_profile_queries(Dialect::Db2, &source, &queries, &operation_context())
                    .unwrap();
            assert!(
                result.syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                result.syntax.diagnostics
            );
            let refs: Vec<_> = result
                .matches
                .iter()
                .filter(|m| m.query_index == 0)
                .collect();
            let names: Vec<_> = result
                .matches
                .iter()
                .filter(|m| m.query_index == 1)
                .collect();
            assert_eq!(refs.len(), expected.len(), "{source}");
            assert_eq!(names.len(), expected.len(), "{source}");
            for ((matched, name), (role, literal, nominal)) in
                refs.into_iter().zip(names).zip(expected)
            {
                assert_eq!(matched.captures, [Some(matched.node_index)]);
                let node = &result.syntax.nodes[matched.node_index];
                assert_eq!(node.kind, "method_reference_type");
                assert_eq!(source.get(node.span.clone().unwrap()), Some(literal));
                let parent = &result.syntax.nodes[node.parent.unwrap()];
                assert_eq!(parent.kind, "method_cast_non_xml_type");
                let role_node = &result.syntax.nodes[parent.parent.unwrap()];
                assert_eq!(role_node.kind, role);
                assert_eq!(source.get(role_node.span.clone().unwrap()), Some(literal));
                let nominal_node = &result.syntax.nodes[name.node_index];
                assert_eq!(nominal_node.kind, "method_named_type");
                assert_eq!(nominal_node.parent, Some(matched.node_index));
                assert_eq!(
                    source.get(nominal_node.span.clone().unwrap()),
                    Some(nominal)
                );
            }
            for (index, literal) in [
                (if family == 0 { 2 } else { 4 }, target),
                (if family == 0 { 3 } else { 5 }, from),
            ] {
                let wrappers: Vec<_> = result
                    .matches
                    .iter()
                    .filter(|m| m.query_index == index)
                    .collect();
                assert_eq!(wrappers.len(), 1);
                assert_eq!(
                    source.get(
                        result.syntax.nodes[wrappers[0].node_index]
                            .span
                            .clone()
                            .unwrap()
                    ),
                    Some(literal)
                );
            }
            for token in &result.syntax.tokens {
                if token.eof {
                    assert_eq!(token.status, SpanStatus::Eof);
                    assert_eq!(token.span, Some(source.len()..source.len()));
                } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                    assert_eq!(source.get(span.clone()), Some(text.as_str()));
                }
            }
            assert!(parse(Dialect::Db2, &source).is_ok());
        }
    }
    assert!(
        rejected.is_empty(),
        "native Method CAST REF forms rejected: {rejected:?}"
    );
    for family in 0..3 {
        for (target, from) in [
            ("REF()", "INTEGER"),
            ("INTEGER", "REF()"),
            ("REF(APP.TYPE_REF_F", "INTEGER"),
            ("INTEGER", "REF(APP.TYPE_REF_F"),
        ] {
            let source = statement(target, from, family);
            let raw =
                parse_profile_queries(Dialect::Db2, &source, &[], &operation_context()).unwrap();
            assert!(!raw.syntax.diagnostics.is_empty(), "{source}");
            assert!(
                matches!(
                    parse(Dialect::Db2, &source),
                    Err(AntlrEngineError::Syntax { .. })
                ),
                "{source}"
            );
        }
    }
}

#[test]
fn db2_module_type_variable_native() {
    for action in ["ADD", "PUBLISH"] {
        for definition in [
            "TYPE T_F AS INTEGER ARRAY[10]",
            "TYPE T_F AS ANCHOR DATA TYPE TO APP.M_A.REF_F ARRAY[]",
            "TYPE T_F AS INTEGER ARRAY[ANCHOR DATA TYPE TO APP.TABLE_F.C]",
            "TYPE T_F AS ANCHOR DATA TYPE TO ROW OF SERVER_F.APP.TABLE_F CURSOR",
            "TYPE T_F AS ROW ANCHOR ROW APP.TABLE_F",
            "TYPE T_F AS ROW ANCHOR DATA TYPE TO ROW OF SERVER_F.APP.TABLE_F",
            "TYPE T_F AS ROW ANCHOR ROW OF \"café\".\"tabela.pão\"",
            "TYPE T_F AS ANCHOR DATA TYPE TO APP.TABLE_F.C",
            "VARIABLE V_F ANCHOR DATA TYPE TO APP.M_A.REF_F",
            "VARIABLE V_F ANCHOR DATA TYPE TO APP.TABLE_F.C",
            "VARIABLE V_F ANCHOR ROW OF SERVER_F.APP.TABLE_F",
            "TYPE T_F AS INTEGER WITH WEAK TYPE RULES",
            "TYPE T_F AS INTEGER WITH WEAK TYPE RULES CHECK(VALUE >= 0 AND VALUE <= 100)",
            "TYPE T_F AS INTEGER WITH WEAK TYPE RULES CHECK(VALUE + 1 > 0)",
            "TYPE T_F AS INTEGER ARRAY[]",
            "TYPE T_F AS APP.ELEMENT_F ARRAY[INTEGER]",
            "TYPE T_F AS INTEGER ARRAY[VARCHAR(20)]",
            "OR REPLACE TYPE T_F AS INTEGER ARRAY[]",
            "TYPE T_F AS CURSOR",
            "TYPE T_F AS APP.ROW_F CURSOR",
            "TYPE T_F AS ANCHOR ROW OF APP.TABLE_F CURSOR",
            "OR REPLACE TYPE T_F AS CURSOR",
            "TYPE T_F AS INTEGER",
            "TYPE T_F AS INTEGER WITH STRONG TYPE RULES",
            "TYPE T_F AS INTEGER WITH WEAK TYPE RULES NOT NULL CHECK(VALUE > 0)",
            "TYPE T_F AS ROW(FIELD_F INTEGER,OTHER_F APP.TYPE_REF_F)",
            "TYPE T_F AS ROW ANCHOR ROW OF APP.TABLE_F",
            "OR REPLACE TYPE T_F AS ROW(FIELD_F INTEGER)",
            "VARIABLE V_F INTEGER",
            "OR REPLACE VARIABLE V_F APP.TYPE_REF_F",
            "VARIABLE V_F INTEGER DEFAULT NULL",
            "VARIABLE V_F INTEGER CONSTANT 1",
            "VARIABLE V_F DATE DEFAULT CURRENT DATE",
            "VARIABLE V_F INTEGER DEFAULT APP.GLOBAL_F",
            "VARIABLE V_F INTEGER DEFAULT (1 + 2)",
            "VARIABLE V_F CURSOR DEFAULT (CURSOR FOR SELECT C FROM APP.TABLE_F)",
            "VARIABLE V_F CURSOR CONSTANT (INSENSITIVE CURSOR WITH HOLD FOR SELECT C FROM APP.TABLE_F)",
            "VARIABLE V_F CURSOR DEFAULT (ASENSITIVE CURSOR WITHOUT HOLD FOR SELECT C FROM APP.TABLE_F)",
            "TYPE \"tipo.pão\" AS INTEGER ARRAY[]",
            "VARIABLE \"variável.pão\" INTEGER",
        ] {
            let source =
                format!("-- α\r\n/* β */ ALTER MODULE \"café\".\"MODULE\" {action} {definition};");
            let syntax =
                parse(Dialect::Db2, &source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
            assert!(syntax.diagnostics.is_empty());
            assert!(syntax.tokens.iter().any(|token| token.eof));
            assert!(syntax.nodes.iter().all(|node| !node.error && !node.missing));
            let helper = if definition.contains(" VARIABLE ") || definition.starts_with("VARIABLE ")
            {
                "module_variable_definition"
            } else {
                "module_type_definition"
            };
            let name_kind = if helper == "module_variable_definition" {
                "variable_name"
            } else {
                "unqualified_type_name"
            };
            let (index, declaration) = syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, node)| node.kind == helper)
                .unwrap();
            assert_eq!(&source[declaration.span.clone().unwrap()], definition);
            let (_, name) = syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, node)| node.kind == name_kind && node.parent == Some(index))
                .unwrap();
            assert!(name.span.as_ref().unwrap().start > 0);
            let opts = declaration.parent.unwrap();
            assert_eq!(syntax.nodes[opts].kind, "alter_module_opts");
            assert_eq!(
                syntax.nodes[syntax.nodes[opts].parent.unwrap()].kind,
                "alter_module_statement"
            );
            for node in &syntax.nodes {
                if let Some(span) = &node.span {
                    assert!(source.get(span.clone()).is_some(), "{source}: {node:?}");
                }
            }
            if definition == "VARIABLE V_F INTEGER" {
                assert!(
                    !syntax
                        .nodes
                        .iter()
                        .any(|node| matches!(node.kind.as_str(), "default_values" | "NULL_"))
                );
            }
        }
    }
    for source in [
        "SET CURSOR_F = CURSOR FOR SELECT C FROM APP.TABLE_F;",
        "SET CURSOR_F = INSENSITIVE CURSOR WITH HOLD FOR SELECT C FROM APP.TABLE_F;",
        "SET CURSOR_F = ASENSITIVE CURSOR WITHOUT HOLD FOR SELECT C FROM APP.TABLE_F;",
    ] {
        let syntax =
            parse(Dialect::Db2, source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        let constructor = syntax
            .nodes
            .iter()
            .find(|node| node.kind == "cursor_value_constructor")
            .unwrap();
        assert_eq!(
            syntax.nodes[constructor.parent.unwrap()].kind,
            "set_statement"
        );
        assert!(syntax.diagnostics.is_empty());
    }
    for source in [
        "CREATE VARIABLE V_F INTEGER;",
        "ALTER MODULE M ADD TYPE K AS INTEGER ARRAY[];",
        "ALTER MODULE K ADD VARIABLE G INTEGER;",
        "ALTER MODULE G PUBLISH TYPE M AS INTEGER;",
        "CREATE VARIABLE V_F CLOB(1K);",
        "CREATE VARIABLE V_F VARCHAR(1);",
        "CREATE VARIABLE V_F TIMESTAMP(0);",
        "CREATE TYPE T_F AS INTEGER ARRAY[1];",
        "CREATE VARIABLE V_F CLOB(1M);",
        "CREATE VARIABLE V_F BLOB(1G);",
        "CREATE TYPE BONUS AS INTEGER WITH WEAK TYPE RULES CHECK(VALUE >= 0 AND VALUE <= 100);",
        "CREATE TABLE TABLE_F(C INTEGER, CHECK(C > 0));",
        "CREATE TABLE TABLE_F(FLAG BOOLEAN CHECK(FLAG));",
        "CREATE TABLE TABLE_F(FLAG BOOLEAN CHECK(TRUE));",
        "CREATE TABLE TABLE_F(C INTEGER CHECK(C >= 0 AND C <= 100));",
        "SELECT NEXT VALUE FOR SEQ_F FROM TABLE_F;",
        "CREATE TYPE T_F AS ROW ANCHOR ROW OF APP.TABLE_F;",
        "CREATE TYPE T_F AS ANCHOR DATA TYPE TO APP.M_A.REF_F ARRAY[];",
        "CREATE VARIABLE V_F ANCHOR ROW OF SERVER_F.APP.TABLE_F;",
        "CREATE VARIABLE V_F ANCHOR DATA TYPE TO APP.TABLE_F.C;",
        "CREATE OR REPLACE VARIABLE V_F INTEGER;",
        "CREATE VARIABLE V_F INTEGER DEFAULT NULL;",
        "CREATE VARIABLE V_F CURSOR DEFAULT (CURSOR FOR SELECT C FROM APP.TABLE_F);",
        "CREATE VARIABLE V_F CURSOR DEFAULT (INSENSITIVE CURSOR WITH HOLD FOR SELECT C FROM APP.TABLE_F);",
    ] {
        let syntax =
            parse(Dialect::Db2, source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(
            syntax
                .nodes
                .iter()
                .any(|node| node.kind == "create_variable_statement"
                    || node.kind == "create_type_statement"
                    || node.kind == "create_table_statement"
                    || node.kind == "select_statement"
                    || node.kind == "alter_module_statement")
        );
        if !source.starts_with("ALTER MODULE") {
            assert!(
                !syntax
                    .nodes
                    .iter()
                    .any(|node| node.kind == "module_variable_definition")
            );
        }
    }
    for source in [
        "ALTER MODULE M ADD TYPE APP.T_F AS INTEGER;",
        "ALTER MODULE M ADD TYPE T_F AS INTEGER WITH WEAK TYPE RULES CHECK(VALUE >);",
        "ALTER MODULE M ADD TYPE T_F AS INTEGER WITH WEAK TYPE RULES CHECK(AND VALUE > 0);",
        "CREATE TABLE TABLE_F(C INTEGER CHECK(C >));",
        "ALTER MODULE M ADD TYPE T_F AS ROW ANCHOR ROW OF A.B.C.D;",
        "ALTER MODULE M ADD TYPE T_F AS ANCHOR DATA TYPE TO A.B.C.D;",
        "ALTER MODULE M ADD TYPE T_F AS INTEGER ARRAY[ANCHOR DATA TYPE TO A.B.C.D];",
        "ALTER MODULE M ADD VARIABLE V_F ANCHOR ROW OF A.B.C.D;",
        "ALTER MODULE M ADD TYPE T_F AS ROW ANCHOR ROW OF A..B;",
        "ALTER MODULE M ADD TYPE T_F AS ROW ANCHOR ROW OF APP.;",
        "ALTER MODULE M ADD VARIABLE V_F ANCHOR DATA TYPE TO APP.;",
        "ALTER MODULE M ADD VARIABLE APP.V_F INTEGER;",
        "ALTER MODULE M ADD TYPE T_F AS(FIELD_F INTEGER) NOT FINAL MODE DB2SQL;",
        "ALTER MODULE M ADD OR REPLACE TYPE T_F AS INTEGER;",
        "ALTER MODULE M ADD TYPE T_F;",
        "ALTER MODULE M ADD TYPE T_F AS;",
        "ALTER MODULE M ADD TYPE T_F AS INTEGER ARRAY[;",
        "ALTER MODULE M ADD TYPE T_F AS ROW();",
        "ALTER MODULE M PUBLISH VARIABLE V_F;",
        "ALTER MODULE M ADD VARIABLE V_F INTEGER DEFAULT;",
        "ALTER MODULE M ADD VARIABLE V_F INTEGER CONSTANT;",
        "ALTER MODULE M ADD VARIABLE V_F INTEGER NULL;",
        "ALTER MODULE M ADD VARIABLE V_F CURSOR DEFAULT (CURSOR SELECT C FROM T);",
        "CREATE VARIABLE V_F INTEGER NULL;",
        "CREATE VARIABLE V_F INTEGER DEFAULT;",
    ] {
        assert!(
            parse(Dialect::Db2, source).is_err(),
            "accepted malformed MODULE/global declaration: {source}"
        );
    }
}

#[test]
fn oracle_collection_members_raw_stock() -> Result<(), Box<dyn std::error::Error>> {
    use graphit_ast::{
        antlr_engine::{
            NodeClass,
            semantic::{CaptureBinding, parse_semantic_occurrences},
        },
        catalog::{
            CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind,
        },
        prepared_input::{ByteSpan, OriginPlan, OriginSegment, OriginalProjection, PreparedInput},
        profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
        scalar_policy::{RegexLimits, ScalarPolicy},
    };
    fn catalog(bytes: &[u8]) -> CatalogDraft {
        let original = operation_context();
        let id = ProfileId::new("semantic-json-fixture", 256).unwrap();
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

    let yaml = include_bytes!("fixtures/catalog/shipped/plsql.yaml");
    assert_eq!(yaml, include_bytes!("fixtures/profiles/shipped/plsql.yaml"));
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/plsql.yaml")
    );
    let fixture: serde_json::Value = serde_saphyr::from_str_with_options(
        std::str::from_utf8(yaml)?,
        serde_saphyr::options! {
            duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
            merge_keys: serde_saphyr::MergeKeyPolicy::Error,
            strict_booleans: true,
        },
    )?;
    assert_eq!(fixture["queries"].as_array().unwrap().len(), 177);
    let draft = catalog(yaml);
    let profile = &draft.profiles()[0];
    let source = "DECLARE TYPE values_t IS TABLE OF NUMBER; xs values_t; n NUMBER; b BOOLEAN; BEGIN n:=xs.COUNT; n:=xs.FIRST; n:=xs.LAST; n:=xs.LIMIT; n:=xs.NEXT(1); b:=xs.EXISTS(1); n:=xs.COUNT(); END; /";
    let original = operation_context();
    let span = ByteSpan {
        start: 0,
        end: source.len() as u64,
    };
    let segments = [OriginSegment::Direct {
        prepared: span,
        original: span,
    }];
    let input = PreparedInput::new(
        source.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let document = parse_semantic_occurrences(profile, &input)?;
    assert!(
        document.syntax.diagnostics.is_empty(),
        "{:?}",
        document.syntax.diagnostics
    );
    let new_occurrences: Vec<_> = document
        .occurrences
        .iter()
        .filter(|occurrence| occurrence.query_index >= 172)
        .collect();
    assert_eq!(
        new_occurrences.len(),
        6,
        "five stock selectors retain COUNT() as another native shape"
    );
    let mut kinds = std::collections::BTreeMap::new();
    for occurrence in new_occurrences {
        let node = &document.syntax.nodes[occurrence.matched.node_index];
        assert_eq!(node.class, NodeClass::Terminal);
        assert!(matches!(
            node.kind.as_str(),
            "COUNT" | "FIRST" | "LAST" | "LIMIT" | "NEXT"
        ));
        *kinds.entry(node.kind.as_str()).or_insert(0) += 1;
        assert_eq!(occurrence.query.data_key().value(), "calls");
        let CaptureBinding::Present(name) = &occurrence.name else {
            panic!("native name capture")
        };
        let Some(OriginalProjection::Exact(name)) = &name.original else {
            panic!("direct Original")
        };
        assert_eq!(name.text(), node.kind);
        assert!(matches!(&occurrence.qualifier, CaptureBinding::Present(_)));
    }
    assert_eq!(
        kinds,
        std::collections::BTreeMap::from([
            ("COUNT", 2),
            ("FIRST", 1),
            ("LAST", 1),
            ("LIMIT", 1),
            ("NEXT", 1)
        ])
    );
    for member in ["NEXT(1)", "COUNT()"] {
        let start = source.find(&format!("xs.{member}")).unwrap() + "xs.".len();
        let end = start + member.len();
        let sites: Vec<_> = document
            .occurrences
            .iter()
            .filter(|occurrence| {
                occurrence.query.relation_type().value() == "CALLS"
                    && occurrence
                        .matched
                        .prepared
                        .as_ref()
                        .is_some_and(|range| range.start() == start && range.end() <= end)
            })
            .collect();
        assert_eq!(
            sites.len(),
            2,
            "{member}: keep ordinary and terminal provenance"
        );
        assert_ne!(sites[0].query_index, sites[1].query_index);
    }
    Ok(())
}
