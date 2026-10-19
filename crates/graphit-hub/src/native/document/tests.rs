use super::*;
use crate::native::ContentLimits;
use graphit_core::file_lock::Cancellation;
use std::time::{Duration, Instant};

fn limits() -> SharedWriteLimits {
    SharedWriteLimits {
        input_bytes: 8192,
        output_bytes: 8192,
        journal_bytes: 8192,
        nodes: 128,
        depth: 16,
        scalar_bytes: 256,
        entries: 64,
        hook_tuples: 1,
        references: 1,
        documents: 1,
    }
}
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 8192,
            max_output_bytes: 8192,
            max_depth: 16,
            max_nodes: 128,
            max_scalar_bytes: 256,
            max_collection_entries: 64,
        },
        cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}

#[test]
fn jsonc_patch_preserves_foreign_literals_comments_and_overrides() {
    let input = "{\r\n  // user's theme\r\n  \"theme\": \"dark\",\r\n  \"huge\": 1e1000,\r\n  \"mcpServers\": {\r\n    \"graphit\": {\r\n      \"command\": \"before\", // launch note\r\n      \"disabled\": true,\r\n      \"timeout\": 250,\r\n      \"extra\": \"\\u00e9\",\r\n    },\r\n    \"foreign\": {\"command\": \"foreign\"},\r\n  },\r\n}\r\n";
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let mut doc =
        NativeDocument::parse(DocumentFormat::Jsonc, input.as_bytes(), limits(), &req).unwrap();
    assert_eq!(doc.render().unwrap(), input);
    doc.set(
        &["mcpServers", "graphit", "command"],
        FieldValue::String("graphit".into()),
    )
    .unwrap();
    doc.set(
        &["mcpServers", "graphit", "args"],
        FieldValue::Array(vec![
            FieldValue::String("mcp".into()),
            FieldValue::String("--stdio".into()),
        ]),
    )
    .unwrap();
    let output = doc.render().unwrap();
    for preserved in [
        "// user's theme",
        "// launch note",
        "\"huge\": 1e1000",
        "\"disabled\": true",
        "\"timeout\": 250",
        "\"extra\": \"\\u00e9\"",
        "\"foreign\": {\"command\": \"foreign\"}",
    ] {
        assert!(output.contains(preserved), "missing untouched region");
    }
    assert!(
        matches!(doc.get(&["mcpServers","graphit","command"]).unwrap(),Some(FieldValue::String(v)) if v=="graphit")
    );
    assert_eq!(doc.get(&["huge"]).err(), Some(DocumentError::WrongType));
    doc.remove(&["mcpServers", "graphit", "command"]).unwrap();
    assert!(
        doc.get(&["mcpServers", "graphit", "command"])
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        doc.get(&["mcpServers", "graphit", "disabled"]).unwrap(),
        Some(FieldValue::Boolean(true))
    ));
    assert!(doc.render().unwrap().contains("// user's theme"));
}

#[test]
fn toml_patch_preserves_native_types_and_attached_comments() {
    let input = "# user header\r\ndate = 1979-05-27T07:32:00Z\r\nfloat = -0.0\r\n[[users]]\r\nname = 'foreign'\r\n[mcp_servers.graphit]\r\n# executable note\r\ncommand = 'before' # keep launch note\r\nstartup_timeout_sec = 250 # user timeout\r\nenabled = false\r\nenv = { KEEP = 'user', MANAGED = 'old' }\r\n";
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let mut doc =
        NativeDocument::parse(DocumentFormat::Toml, input.as_bytes(), limits(), &req).unwrap();
    doc.set(
        &["mcp_servers", "graphit", "command"],
        FieldValue::String("graphit".into()),
    )
    .unwrap();
    doc.set(
        &["mcp_servers", "graphit", "env", "MANAGED"],
        FieldValue::String("new".into()),
    )
    .unwrap();
    let output = doc.render().unwrap();
    for preserved in [
        "# user header",
        "# executable note",
        "# keep launch note",
        "startup_timeout_sec = 250 # user timeout",
        "enabled = false",
        "KEEP = 'user'",
        "[[users]]",
        "name = 'foreign'",
    ] {
        assert!(
            output.contains(preserved),
            "missing untouched TOML region: {preserved}"
        );
    }
    let native: toml_edit::DocumentMut = output.parse().unwrap();
    assert_eq!(
        native["date"].as_datetime().unwrap().to_string(),
        "1979-05-27T07:32:00Z"
    );
    assert_eq!(
        native["float"].as_float().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    assert!(
        matches!(doc.get(&["mcp_servers","graphit","env","MANAGED"]).unwrap(),Some(FieldValue::String(v)) if v=="new")
    );
    doc.remove(&["mcp_servers", "graphit", "env", "MANAGED"])
        .unwrap();
    assert!(doc.render().unwrap().contains("KEEP = 'user'"));
}

#[test]
fn strict_syntax_and_decoded_duplicates_fail_with_redacted_errors() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    for input in [
        "{\"secret_marker\":1,\"secret_marker\":2}",
        "{\"a\":1,\"\\u0061\":2}",
        "{\"x\":{\"a\":1,\"a\":2}}",
    ] {
        assert_eq!(
            NativeDocument::parse(DocumentFormat::Jsonc, input.as_bytes(), limits(), &req).err(),
            Some(DocumentError::DuplicateKey)
        );
    }
    for input in [
        "{a:1}",
        "{'a':1}",
        "{\"a\":0x10}",
        "{\"a\":+2}",
        "{\"a\":.5}",
        "{\"a\":NaN}",
        "{\"a\":1 \"b\":2}",
        "{\"a\":\"\\x20\"}",
        "{\"a\":",
    ] {
        let e = NativeDocument::parse(DocumentFormat::Jsonc, input.as_bytes(), limits(), &req)
            .err()
            .unwrap();
        assert!(!e.to_string().contains(input));
    }
    for input in ["{\"a\":1,}", "{/* comment */\"a\":1}"] {
        assert!(
            NativeDocument::parse(DocumentFormat::Jsonc, input.as_bytes(), limits(), &req).is_ok()
        );
        assert_eq!(
            NativeDocument::parse(DocumentFormat::Json, input.as_bytes(), limits(), &req).err(),
            Some(DocumentError::InvalidDocument)
        );
    }
    for input in ["a=1\na=2\n", "a = { x=1, x=2 }", "a='unterminated"] {
        assert_eq!(
            NativeDocument::parse(DocumentFormat::Toml, input.as_bytes(), limits(), &req).err(),
            Some(DocumentError::InvalidDocument)
        );
    }
    for input in ["null", "[]", "1", "\"text\""] {
        assert_eq!(
            NativeDocument::parse(DocumentFormat::Json, input.as_bytes(), limits(), &req).err(),
            Some(DocumentError::WrongType)
        );
    }
    assert_eq!(
        NativeDocument::parse(DocumentFormat::Json, &[0xff], limits(), &req).err(),
        Some(DocumentError::InvalidEncoding)
    );
}

#[test]
fn independent_input_output_limits_and_atomic_failed_edit() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let mut l = limits();
    l.input_bytes = 2;
    l.scalar_bytes = 2;
    let mut doc = NativeDocument::parse(DocumentFormat::Json, b"{}", l, &req).unwrap();
    doc.set(&["x"], FieldValue::String("ok".into())).unwrap();
    assert!(doc.render().unwrap().len() > l.input_bytes);
    let input = b"{\"x\":\"old\"}";
    let mut l = limits();
    l.output_bytes = input.len();
    l.scalar_bytes = 3;
    let mut doc = NativeDocument::parse(DocumentFormat::Json, input, l, &req).unwrap();
    let before = doc.render().unwrap();
    assert_eq!(
        doc.set(&["extra"], FieldValue::String("x".into())).err(),
        Some(DocumentError::InvalidPath)
    );
    assert_eq!(
        doc.set(&["y"], FieldValue::String("new".into())).err(),
        Some(DocumentError::LimitExceeded)
    );
    assert_eq!(doc.render().unwrap(), before);
    for (format, input) in [
        (DocumentFormat::Json, "{}"),
        (DocumentFormat::Jsonc, "{}"),
        (DocumentFormat::Toml, ""),
    ] {
        let value = FieldValue::String("\n\t".into());
        let mut reference =
            NativeDocument::parse(format, input.as_bytes(), limits(), &req).unwrap();
        reference.set(&["x"], value.clone()).unwrap();
        let exact = reference.render().unwrap().len();
        let mut l = limits();
        l.output_bytes = exact;
        l.scalar_bytes = 2;
        let mut doc = NativeDocument::parse(format, input.as_bytes(), l, &req).unwrap();
        doc.set(&["x"], value.clone()).unwrap();
        assert_eq!(doc.render().unwrap().len(), exact);
        l.output_bytes -= 1;
        let mut doc = NativeDocument::parse(format, input.as_bytes(), l, &req).unwrap();
        assert_eq!(
            doc.set(&["x"], value).err(),
            Some(DocumentError::LimitExceeded)
        );
        assert!(doc.get(&["x"]).unwrap().is_none());
    }
    assert_eq!(
        doc.set(&["x", "n"], FieldValue::String("new".into())).err(),
        Some(DocumentError::WrongType)
    );
    assert_eq!(
        doc.set(&["x", "nested"], FieldValue::String("new".into()))
            .err(),
        Some(DocumentError::InvalidPath)
    );
    assert_eq!(doc.remove(&[]).err(), Some(DocumentError::InvalidPath));
    assert_eq!(doc.render().unwrap(), before);
}

#[test]
fn admitted_large_source_can_be_inspected_and_shrunk_to_output_budget() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    for (format, input, path) in [
        (DocumentFormat::Json, "{\"x\":12345,\"y\":1}", &["x"][..]),
        (DocumentFormat::Jsonc, "{\"x\":12345,\"y\":1}", &["x"][..]),
        (DocumentFormat::Toml, "x=12345\ny=1\n", &["x"][..]),
    ] {
        let mut l = limits();
        l.output_bytes = input.len() - 2;
        l.scalar_bytes = 5;
        let mut doc = NativeDocument::parse(format, input.as_bytes(), l, &req).unwrap();
        assert_eq!(doc.render().err(), Some(DocumentError::LimitExceeded));
        assert!(matches!(
            doc.get(path).unwrap(),
            Some(FieldValue::Integer(12345))
        ));
        doc.set(path, FieldValue::Integer(1)).unwrap();
        assert!(doc.render().unwrap().len() <= l.output_bytes);
        let mut doc = NativeDocument::parse(format, input.as_bytes(), l, &req).unwrap();
        doc.remove(path).unwrap();
        assert!(doc.get(path).unwrap().is_none());
        assert!(doc.render().unwrap().len() <= l.output_bytes);
    }
}

#[test]
fn removals_preserve_attached_and_embedded_comments_at_all_positions() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    for (format, input, paths) in [
        (
            DocumentFormat::Jsonc,
            "{\r\n // before\r\n \"a\": /* between */ [1, // inside\r\n 2], // after\r\n \"b\":2, /* last */ \"c\":3 /* end */\r\n}\r\n",
            vec![vec!["a"], vec!["c"], vec!["b"]],
        ),
        (
            DocumentFormat::Toml,
            "# before\r\na = [1, # inside\r\n 2] # after\r\n# between\r\nb=2\r\n# last\r\nc=3 # end\r\n",
            vec![vec!["a"], vec!["c"], vec!["b"]],
        ),
        (
            DocumentFormat::Toml,
            "x = {\r\n # before\r\n a=1, # after\r\n b=2, # between\r\n c=3 # last\r\n} # end\r\n",
            vec![vec!["x", "a"], vec!["x", "c"], vec!["x", "b"]],
        ),
    ] {
        let mut doc = NativeDocument::parse(format, input.as_bytes(), limits(), &req).unwrap();
        for path in paths {
            doc.remove(&path).unwrap();
            assert!(doc.get(&path).unwrap().is_none());
            let output = doc.render().unwrap();
            for marker in ["before", "after", "between", "last", "end"] {
                assert!(output.contains(marker), "lost {marker}");
            }
            if input.contains("inside") {
                assert!(output.contains("inside"));
            }
        }
    }
    for (input, path) in [
        (
            "# before\r\na.b.command = 'x' # after\r\ny=1\r\n",
            vec!["a", "b", "command"],
        ),
        (
            "x = { a.b.command = 'x', keep=1 } # after\r\n",
            vec!["x", "a", "b", "command"],
        ),
        ("x = { a=1, } # after\r\n", vec!["x", "a"]),
    ] {
        let mut doc =
            NativeDocument::parse(DocumentFormat::Toml, input.as_bytes(), limits(), &req).unwrap();
        doc.remove(&path).unwrap();
        assert!(doc.get(&path).unwrap().is_none());
        assert!(doc.render().unwrap().contains("# after"));
    }
}

#[test]
fn object_replacement_and_creation_use_native_table_shape() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let value = FieldValue::Object(BTreeMap::from([(
        "MANAGED".into(),
        FieldValue::String("new".into()),
    )]));
    for input in [
        "env={ OLD='old' }\nother=1\n",
        "other=1\n# table note\n[env] # header note\nOLD='old'\n",
    ] {
        let mut doc =
            NativeDocument::parse(DocumentFormat::Toml, input.as_bytes(), limits(), &req).unwrap();
        doc.set(&["env"], value.clone()).unwrap();
        assert!(doc.get(&["env", "OLD"]).unwrap().is_none());
        assert!(
            matches!(doc.get(&["env","MANAGED"]).unwrap(),Some(FieldValue::String(v)) if v=="new")
        );
        assert!(matches!(
            doc.get(&["other"]).unwrap(),
            Some(FieldValue::Integer(1))
        ));
        if input.contains("table note") {
            assert!(doc.render().unwrap().contains("# table note"));
            assert!(doc.render().unwrap().contains("# header note"));
            assert_eq!(doc.remove(&["env"]).err(), Some(DocumentError::WrongType));
        }
    }
    let mut doc = NativeDocument::parse(
        DocumentFormat::Toml,
        b"env={ KEEP='user' }\n",
        limits(),
        &req,
    )
    .unwrap();
    doc.set(
        &["env", "nested", "MANAGED"],
        FieldValue::String("new".into()),
    )
    .unwrap();
    assert!(doc.render().unwrap().contains("KEEP='user'"));
    assert!(
        matches!(doc.get(&["env","nested","MANAGED"]).unwrap(),Some(FieldValue::String(v)) if v=="new")
    );
}

#[test]
fn source_scalar_collection_node_and_depth_boundaries() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let text = b"{\"a\":\"1234\"}";
    let mut l = limits();
    l.input_bytes = text.len();
    l.scalar_bytes = 4;
    assert!(NativeDocument::parse(DocumentFormat::Json, text, l, &req).is_ok());
    l.input_bytes -= 1;
    assert_eq!(
        NativeDocument::parse(DocumentFormat::Json, text, l, &req).err(),
        Some(DocumentError::LimitExceeded)
    );
    l.input_bytes = text.len();
    l.scalar_bytes = 3;
    assert_eq!(
        NativeDocument::parse(DocumentFormat::Json, text, l, &req).err(),
        Some(DocumentError::LimitExceeded)
    );
    let text = b"{\"a\":[1,2,3]}";
    let mut l = limits();
    l.entries = 3;
    l.nodes = 5;
    l.depth = 3;
    assert!(NativeDocument::parse(DocumentFormat::Json, text, l, &req).is_ok());
    l.entries = 2;
    assert_eq!(
        NativeDocument::parse(DocumentFormat::Json, text, l, &req).err(),
        Some(DocumentError::LimitExceeded)
    );
    l.entries = 3;
    l.nodes = 4;
    assert_eq!(
        NativeDocument::parse(DocumentFormat::Json, text, l, &req).err(),
        Some(DocumentError::LimitExceeded)
    );
    l.nodes = 5;
    l.depth = 2;
    assert_eq!(
        NativeDocument::parse(DocumentFormat::Json, text, l, &req).err(),
        Some(DocumentError::LimitExceeded)
    );
    let text = b"[[a]]\nx=1\n[[a]]\nx=2\n";
    let mut l = limits();
    l.depth = 4;
    l.nodes = 6;
    l.entries = 2;
    assert!(NativeDocument::parse(DocumentFormat::Toml, text, l, &req).is_ok());
    l.nodes = 5;
    assert_eq!(
        NativeDocument::parse(DocumentFormat::Toml, text, l, &req).err(),
        Some(DocumentError::LimitExceeded)
    );
    let mut l = limits();
    l.nodes = 1;
    l.entries = 1;
    // Two delimiters plus two comments reach the four-token cap while the
    // semantic tree is only one node. A third comment tests lexer work directly.
    assert!(NativeDocument::parse(DocumentFormat::Jsonc, b"{/**//**/}", l, &req).is_ok());
    assert_eq!(
        NativeDocument::parse(DocumentFormat::Jsonc, b"{/**//**//**/}", l, &req).err(),
        Some(DocumentError::LimitExceeded)
    );
    // The TOML lexer also emits EOF: comment/newline/comment/EOF is four.
    assert!(NativeDocument::parse(DocumentFormat::Toml, b"#a\n#b", l, &req).is_ok());
    assert_eq!(
        NativeDocument::parse(DocumentFormat::Toml, b"#a\n#b\n", l, &req).err(),
        Some(DocumentError::LimitExceeded)
    );
}

#[test]
fn public_packet_zero_overflow_and_ceiling_are_revalidated() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let mut bad = Vec::new();
    for field in 0..10 {
        let mut l = limits();
        let fields = [
            &mut l.input_bytes,
            &mut l.output_bytes,
            &mut l.journal_bytes,
            &mut l.nodes,
            &mut l.depth,
            &mut l.scalar_bytes,
            &mut l.entries,
            &mut l.hook_tuples,
            &mut l.references,
            &mut l.documents,
        ];
        *fields.into_iter().nth(field).unwrap() = 0;
        bad.push(l);
    }
    let mut l = limits();
    l.input_bytes = usize::MAX;
    bad.push(l);
    let mut l = limits();
    l.nodes = usize::MAX;
    bad.push(l);
    let mut l = limits();
    l.depth = 81;
    bad.push(l);
    let mut l = limits();
    l.entries = l.nodes + 1;
    bad.push(l);
    for l in bad {
        assert_eq!(
            NativeDocument::parse(DocumentFormat::Json, b"{}", l, &req).err(),
            Some(DocumentError::InvalidPolicy)
        );
    }
}

#[test]
fn missing_container_and_literal_dotted_key_are_exact() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    for (format, input) in [
        (DocumentFormat::Json, "{}"),
        (DocumentFormat::Jsonc, "{}"),
        (DocumentFormat::Toml, ""),
    ] {
        let mut doc = NativeDocument::parse(format, input.as_bytes(), limits(), &req).unwrap();
        doc.set(
            &["mcp_servers", "with.dot", "command"],
            FieldValue::String("graphit".into()),
        )
        .unwrap();
        assert!(
            matches!(doc.get(&["mcp_servers","with.dot","command"]).unwrap(),Some(FieldValue::String(v)) if v=="graphit")
        );
        assert!(doc.get(&["missing", "command"]).unwrap().is_none());
        doc.remove(&["missing", "command"]).unwrap();
        doc.remove(&["mcp_servers", "with.dot", "command"]).unwrap();
        assert!(
            doc.get(&["mcp_servers", "with.dot", "command"])
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn captured_cancellation_and_deadline_are_not_restarted_between_edits() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let mut doc = NativeDocument::parse(DocumentFormat::Json, b"{}", limits(), &req).unwrap();
    cancel.cancel();
    assert_eq!(
        doc.set(&["a"], FieldValue::Boolean(true)).err(),
        Some(DocumentError::Interrupted)
    );
    assert_eq!(doc.render().err(), Some(DocumentError::Interrupted));
    assert_eq!(doc.get(&["a"]).err(), Some(DocumentError::Interrupted));
    assert_eq!(doc.remove(&["a"]).err(), Some(DocumentError::Interrupted));
    assert_eq!(
        NativeDocument::parse(DocumentFormat::Json, b"{}", limits(), &req).err(),
        Some(DocumentError::Interrupted)
    );
    let cancel = Cancellation::default();
    let req = RequestContext::new(
        ContentLimits {
            max_input_bytes: 8192,
            max_output_bytes: 8192,
            max_depth: 16,
            max_nodes: 128,
            max_scalar_bytes: 256,
            max_collection_entries: 64,
        },
        &cancel,
        Instant::now() + Duration::from_millis(20),
    )
    .unwrap();
    let mut doc = NativeDocument::parse(DocumentFormat::Json, b"{}", limits(), &req).unwrap();
    std::thread::sleep(Duration::from_millis(25));
    assert_eq!(
        doc.set(&["a"], FieldValue::Boolean(true)).err(),
        Some(DocumentError::TimedOut)
    );
    assert_eq!(doc.render().err(), Some(DocumentError::TimedOut));
}

#[test]
fn empty_toml_table_removal_preserves_comments_and_foreign_tables() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let input = "# personal header\r\n[mcp_servers.foreign]\r\ncommand='foreign'\r\n[mcp_servers.graphit] # keep launch note\r\n# keep body note\r\n";
    let mut document =
        NativeDocument::parse(DocumentFormat::Toml, input.as_bytes(), limits(), &request).unwrap();
    document.remove(&["mcp_servers", "graphit"]).unwrap();
    let output = document.render().unwrap();
    assert!(!output.contains("[mcp_servers.graphit]"));
    for keep in [
        "# personal header",
        "# keep launch note",
        "# keep body note",
        "[mcp_servers.foreign]",
        "command='foreign'",
    ] {
        assert!(output.contains(keep), "missing untouched TOML region");
    }
    assert!(document.get(&["mcp_servers", "graphit"]).unwrap().is_none());
    assert!(
        matches!(document.get(&["mcp_servers","foreign","command"]).unwrap(),Some(FieldValue::String(value)) if value=="foreign")
    );
    // Non-empty tables retain their native fields until the owner removes them.
    assert_eq!(
        document.remove(&["mcp_servers", "foreign"]).err(),
        Some(DocumentError::WrongType)
    );
}

#[test]
fn tuple_edits_preserve_foreign_native_values_and_personal_fields() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let input = r#"{
      "hooks":{"preToolUse":[
        // foreign hook note
        {"command":"foreign","huge":1e1000,"metadata":null},
        {"command":"owned","timeout":250,"disabled":true,"extra":"user"}
      ]}
    }"#;
    let mut doc =
        NativeDocument::parse(DocumentFormat::Jsonc, input.as_bytes(), limits(), &req).unwrap();
    let mut fields = TupleFields::from([
        ("command".into(), Some(FieldValue::String("owned".into()))),
        ("matcher".into(), None),
    ]);
    assert!(matches!(
        doc.tuple_match(&["hooks", "preToolUse"], &fields).unwrap(),
        TupleMatch::Unique(1)
    ));
    fields.insert("command".into(), Some(FieldValue::String("updated".into())));
    fields.insert(
        "matcher".into(),
        Some(FieldValue::String("@custom/tool".into())),
    );
    doc.edit_tuple(&["hooks", "preToolUse"], TupleEdit::Update(1, &fields))
        .unwrap();
    let output = doc.render().unwrap();
    for literal in [
        "// foreign hook note",
        r#"{"command":"foreign","huge":1e1000,"metadata":null}"#,
        r#""timeout":250"#,
        r#""disabled":true"#,
        r#""extra":"user""#,
    ] {
        assert!(output.contains(literal), "foreign/personal content changed");
    }
    assert!(matches!(
        doc.tuple_match(&["hooks", "preToolUse"], &fields).unwrap(),
        TupleMatch::Unique(1)
    ));
    doc.edit_tuple(&["hooks", "preToolUse"], TupleEdit::Remove(1))
        .unwrap();
    assert!(matches!(
        doc.tuple_match(&["hooks", "preToolUse"], &fields).unwrap(),
        TupleMatch::Missing
    ));
    let output = doc.render().unwrap();
    assert!(output.contains(r#"{"command":"foreign","huge":1e1000,"metadata":null}"#));
    assert!(output.contains("// foreign hook note"));
    assert!(!output.contains("updated") && !output.contains("timeout"));
    doc.edit_tuple(
        &["hooks", "preToolUse"],
        TupleEdit::Append(&FieldValue::Object(BTreeMap::from([(
            "command".into(),
            FieldValue::String("new".into()),
        )]))),
    )
    .unwrap();
    let fields = TupleFields::from([("command".into(), Some(FieldValue::String("new".into())))]);
    assert!(matches!(
        doc.tuple_match(&["hooks", "preToolUse"], &fields).unwrap(),
        TupleMatch::Unique(1)
    ));
}

#[test]
fn tuple_matching_distinguishes_absence_ambiguity_and_wrong_native_types() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let fields = TupleFields::from([
        ("command".into(), Some(FieldValue::String("wanted".into()))),
        ("matcher".into(), None),
    ]);
    let doc = NativeDocument::parse(DocumentFormat::Json, br#"{"hooks":[{"command":"wanted"},{"command":"wanted","matcher":null},{"command":1e1000},{"command":"wanted"}]}"#, limits(), &req).unwrap();
    assert!(matches!(
        doc.tuple_match(&["hooks"], &fields).unwrap(),
        TupleMatch::Ambiguous
    ));
    let fields = TupleFields::from([("command".into(), Some(FieldValue::String("absent".into())))]);
    assert!(matches!(
        doc.tuple_match(&["hooks"], &fields).unwrap(),
        TupleMatch::Missing
    ));
    for input in [
        br#"{"hooks":{}}"#.as_slice(),
        br#"{"hooks":[null]}"#.as_slice(),
    ] {
        let doc = NativeDocument::parse(DocumentFormat::Json, input, limits(), &req).unwrap();
        assert!(matches!(
            doc.tuple_match(&["hooks"], &fields),
            Err(DocumentError::WrongType)
        ));
    }
}

#[test]
fn tuple_edit_failure_keeps_the_original_document() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let mut small = limits();
    small.output_bytes = 256;
    let input = br#"{"hooks":[]}"#;
    let mut doc = NativeDocument::parse(DocumentFormat::Json, input, small, &req).unwrap();
    let value = FieldValue::Object(BTreeMap::from([(
        "command".into(),
        FieldValue::String("\"".repeat(150)),
    )]));
    assert!(matches!(
        doc.edit_tuple(&["hooks"], TupleEdit::Append(&value)),
        Err(DocumentError::LimitExceeded)
    ));
    assert_eq!(doc.render().unwrap().as_bytes(), input);
    cancel.cancel();
    assert!(matches!(
        doc.edit_tuple(&["hooks"], TupleEdit::Remove(0)),
        Err(DocumentError::Interrupted)
    ));
}
