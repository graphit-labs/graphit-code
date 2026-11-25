use super::*;
use graphit_core::file_lock::Cancellation;
use std::time::{Duration, Instant};

fn policy() -> CodexWireLimits {
    CodexWireLimits {
        frame_bytes: 1024 * 1024,
        depth: 16,
        nodes: 10_000,
        fields: 10_000,
        array_items: 10_000,
        string_bytes: 65_536,
        decoded_string_bytes: 1024 * 1024,
        id_bytes: 1024,
    }
}
fn generous() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn known(raw: &str) -> OwnedCodexEvent {
    match decode_codex_frame(raw.as_bytes(), &policy(), &generous()).unwrap() {
        CodexDecodedFrame::Known(event) => event,
        CodexDecodedFrame::Ignored(_) => panic!("expected known event"),
    }
}
fn denied(raw: &[u8], limits: &CodexWireLimits, native: &SnapshotContext) -> CodexWireError {
    match decode_codex_frame(raw, limits, native) {
        Err(error) => error,
        Ok(_) => panic!("expected denial"),
    }
}
fn item(body: &str, phase: &str) -> String {
    format!(r#"{{"type":"item.{phase}","item":{body}}}"#)
}
const AGENT: &str = r#"{"type":"item.completed","item":{"id":"abcd","type":"agent_message","text":"longword"},"padding":[true,false]}"#;

#[test]
fn all_eight_events_and_signed_usage_preserve_native_data() {
    let event = known(r#"{"thread_id":"","type":"thread.started"}"#);
    assert_eq!(event.kind(), CodexEventKind::ThreadStarted);
    assert!(matches!(
        event.view(),
        CodexEventView::ThreadStarted { thread_id: "" }
    ));
    assert!(matches!(
        known(r#"{"type":"turn.started"}"#).view(),
        CodexEventView::TurnStarted
    ));
    let event = known(
        r#"{"type":"turn.completed","usage":{"input_tokens":-9223372036854775808,"cached_input_tokens":-1,"output_tokens":9223372036854775807,"reasoning_output_tokens":0}}"#,
    );
    if let CodexEventView::TurnCompleted { usage } = event.view() {
        assert_eq!(usage.input_tokens, i64::MIN);
        assert_eq!(usage.cached_input_tokens, -1);
        assert_eq!(usage.output_tokens, i64::MAX);
        assert_eq!(usage.reasoning_output_tokens, 0);
        assert_eq!(usage.cache_write_input_tokens, 0);
    } else {
        panic!("usage view");
    }
    assert!(matches!(
        known(r#"{"error":{"message":"failure"},"type":"turn.failed"}"#).view(),
        CodexEventView::TurnFailed { message: "failure" }
    ));
    assert!(matches!(
        known(r#"{"type":"error","message":"fatal"}"#).view(),
        CodexEventView::Error { message: "fatal" }
    ));
    for (phase, expected, kind) in [
        (
            "started",
            CodexItemPhase::Started,
            CodexEventKind::ItemStarted,
        ),
        (
            "updated",
            CodexItemPhase::Updated,
            CodexEventKind::ItemUpdated,
        ),
        (
            "completed",
            CodexItemPhase::Completed,
            CodexEventKind::ItemCompleted,
        ),
    ] {
        let event = known(&item(
            r#"{"text":"delta","id":"","type":"agent_message"}"#,
            phase,
        ));
        assert_eq!(event.kind(), kind);
        if let CodexEventView::Item { phase, item } = event.view() {
            assert_eq!(phase, expected);
            assert!(item.outer_id.is_empty());
            assert!(item.sdk_observed_id().is_empty());
            assert!(matches!(
                item.body,
                CodexItemBodyView::AgentMessage { text: "delta" }
            ));
        } else {
            panic!("item view");
        }
    }
}

#[test]
fn every_item_body_and_collection_getter_has_a_real_decode_consumer() {
    assert!(matches!(
        known(&item(
            r#"{"id":"r","type":"reasoning","text":"reason"}"#,
            "completed"
        ))
        .view(),
        CodexEventView::Item {
            item: CodexItemView {
                body: CodexItemBodyView::Reasoning { text: "reason" },
                ..
            },
            ..
        }
    ));
    for (value, expected) in [
        ("in_progress", CodexStatus::InProgress),
        ("completed", CodexStatus::Completed),
        ("failed", CodexStatus::Failed),
        ("declined", CodexStatus::Declined),
    ] {
        let raw = format!(
            r#"{{"id":"c","type":"command_execution","command":"cmd","aggregated_output":"out","exit_code":-2147483648,"status":"{value}"}}"#
        );
        if let CodexEventView::Item {
            item:
                CodexItemView {
                    body:
                        CodexItemBodyView::CommandExecution {
                            command,
                            aggregated_output,
                            exit_code,
                            status,
                        },
                    ..
                },
            ..
        } = known(&item(&raw, "completed")).view()
        {
            assert!(command == "cmd" && aggregated_output == "out");
            assert_eq!(exit_code, Some(i32::MIN));
            assert_eq!(status, expected);
        } else {
            panic!("command view");
        }
    }
    for (value, expected) in [
        ("in_progress", CodexStatus::InProgress),
        ("completed", CodexStatus::Completed),
        ("failed", CodexStatus::Failed),
    ] {
        let raw = format!(
            r#"{{"id":"f","type":"file_change","changes":[{{"path":"a","kind":"add"}},{{"path":"d","kind":"delete"}},{{"path":"u","kind":"update"}}],"status":"{value}"}}"#
        );
        let event = known(&item(&raw, "completed"));
        if let CodexEventView::Item {
            item:
                CodexItemView {
                    body: CodexItemBodyView::FileChange { changes, status },
                    ..
                },
            ..
        } = event.view()
        {
            assert_eq!(changes.len(), 3);
            assert!(!changes.is_empty());
            assert!(
                changes
                    .get(0)
                    .is_some_and(|c| c.path == "a" && c.kind == CodexChangeKind::Add)
            );
            assert!(
                changes
                    .get(1)
                    .is_some_and(|c| c.path == "d" && c.kind == CodexChangeKind::Delete)
            );
            assert!(
                changes
                    .get(2)
                    .is_some_and(|c| c.path == "u" && c.kind == CodexChangeKind::Update)
            );
            assert!(changes.get(3).is_none());
            assert_eq!(changes.iter().count(), 3);
            assert_eq!(changes.iter().len(), 3);
            assert_eq!(status, expected);
        } else {
            panic!("file view");
        }
    }
    let event = known(&item(
        r#"{"id":"t","type":"todo_list","items":[{"text":"todo","completed":false},{"text":"done","completed":true}]}"#,
        "completed",
    ));
    if let CodexEventView::Item {
        item:
            CodexItemView {
                body: CodexItemBodyView::TodoList { items },
                ..
            },
        ..
    } = event.view()
    {
        assert_eq!(items.len(), 2);
        assert!(
            items
                .get(0)
                .is_some_and(|v| v.text == "todo" && !v.completed)
        );
        assert!(
            items
                .get(1)
                .is_some_and(|v| v.text == "done" && v.completed)
        );
        assert!(items.get(2).is_none());
        assert_eq!(items.iter().count(), 2);
    } else {
        panic!("todo view");
    }
    assert!(matches!(
        known(&item(
            r#"{"id":"w","type":"error","message":"warning"}"#,
            "completed"
        ))
        .view(),
        CodexEventView::Item {
            item: CodexItemView {
                body: CodexItemBodyView::Error { message: "warning" },
                ..
            },
            ..
        }
    ));
}

#[test]
fn arbitrary_json_is_preserved_as_raw_with_mcp_null_and_empty_distinctions() {
    let raw = item(
        r#"{"id":"m","type":"mcp_tool_call","server":"s","tool":"t","arguments":{"x":1e400,"x":999999999999999999999999999999},"result":{"content":[1e400,{"a":1,"a":2}],"_meta":{"b":1e400},"structured_content":[99999999999999999999999999999]},"error":{"message":"reported"},"status":"failed"}"#,
        "completed",
    );
    let event = known(&raw);
    if let CodexEventView::Item {
        item:
            CodexItemView {
                body:
                    CodexItemBodyView::McpToolCall {
                        server,
                        tool,
                        arguments,
                        result,
                        error,
                        status,
                    },
                ..
            },
        ..
    } = event.view()
    {
        assert!(server == "s" && tool == "t");
        assert!(arguments.as_str() == r#"{"x":1e400,"x":999999999999999999999999999999}"#);
        let result = result.unwrap();
        assert_eq!(result.content.len(), 2);
        assert!(result.content.get(0).is_some_and(|v| v.as_str() == "1e400"));
        assert!(
            result
                .content
                .get(1)
                .is_some_and(|v| v.as_str() == r#"{"a":1,"a":2}"#)
        );
        assert!(result.content.get(2).is_none());
        assert_eq!(result.content.iter().count(), 2);
        assert!(result.meta.is_some_and(|v| v.as_str() == r#"{"b":1e400}"#));
        assert!(
            result
                .structured_content
                .is_some_and(|v| v.as_str().starts_with('['))
        );
        assert!(error == Some("reported"));
        assert_eq!(status, CodexStatus::Failed);
    } else {
        panic!("mcp view");
    }
    for (status, expected) in [
        ("in_progress", CodexStatus::InProgress),
        ("completed", CodexStatus::Completed),
        ("failed", CodexStatus::Failed),
    ] {
        let raw = format!(
            r#"{{"id":"m","type":"mcp_tool_call","server":"","tool":"","result":null,"error":null,"status":"{status}"}}"#
        );
        if let CodexEventView::Item {
            item:
                CodexItemView {
                    body:
                        CodexItemBodyView::McpToolCall {
                            arguments: JsonView::DefaultNull,
                            result: None,
                            error: None,
                            status,
                            ..
                        },
                    ..
                },
            ..
        } = known(&item(&raw, "completed")).view()
        {
            assert_eq!(status, expected);
        } else {
            panic!("mcp defaults");
        }
    }
    let event = known(&item(
        r#"{"id":"m","type":"mcp_tool_call","server":"","tool":"","arguments":null,"result":{"content":[],"_meta":null,"structured_content":null},"error":{"message":""},"status":"completed"}"#,
        "completed",
    ));
    if let CodexEventView::Item {
        item:
            CodexItemView {
                body:
                    CodexItemBodyView::McpToolCall {
                        arguments: JsonView::Raw("null"),
                        result: Some(result),
                        error: Some(""),
                        ..
                    },
                ..
            },
        ..
    } = event.view()
    {
        assert_eq!(result.content.len(), 0);
        assert!(result.meta.is_none() && result.structured_content.is_none());
    } else {
        panic!("explicit null data");
    }
}

#[test]
fn collab_tools_all_states_and_last_wins_charge_every_declaration() {
    for (tool, expected) in [
        ("spawn_agent", CodexCollabTool::SpawnAgent),
        ("send_input", CodexCollabTool::SendInput),
        ("wait", CodexCollabTool::Wait),
        ("close_agent", CodexCollabTool::CloseAgent),
    ] {
        for (status, expected_status) in [
            ("in_progress", CodexStatus::InProgress),
            ("completed", CodexStatus::Completed),
            ("failed", CodexStatus::Failed),
        ] {
            let raw = format!(
                r#"{{"id":"","type":"collab_tool_call","tool":"{tool}","sender_thread_id":"","receiver_thread_ids":["","b"],"prompt":"","agents_states":{{"a":{{"status":"pending_init"}},"a":{{"status":"completed","message":""}},"b":{{"status":"running"}},"c":{{"status":"interrupted"}},"d":{{"status":"errored"}},"e":{{"status":"shutdown"}},"f":{{"status":"not_found"}}}},"status":"{status}"}}"#
            );
            let event = known(&item(&raw, "completed"));
            assert!(event.stats().fields > 20);
            if let CodexEventView::Item {
                item:
                    CodexItemView {
                        body:
                            CodexItemBodyView::CollabToolCall {
                                tool,
                                sender_thread_id,
                                receiver_thread_ids,
                                prompt,
                                agents_states,
                                status,
                            },
                        ..
                    },
                ..
            } = event.view()
            {
                assert_eq!(tool, expected);
                assert!(sender_thread_id.is_empty() && prompt == Some(""));
                assert_eq!(receiver_thread_ids.len(), 2);
                assert!(receiver_thread_ids.get(0) == Some(""));
                assert!(receiver_thread_ids.get(1) == Some("b"));
                assert!(receiver_thread_ids.get(2).is_none());
                assert_eq!(receiver_thread_ids.iter().count(), 2);
                assert_eq!(agents_states.len(), 6);
                assert!(agents_states.get(0).is_some_and(|v| v.id == "a"
                    && v.status == CodexAgentStatus::Completed
                    && v.message == Some("")));
                assert!(agents_states.get(6).is_none());
                let states: Vec<_> = agents_states.iter().map(|v| v.status).collect();
                assert!(
                    states.contains(&CodexAgentStatus::Running)
                        && states.contains(&CodexAgentStatus::Interrupted)
                        && states.contains(&CodexAgentStatus::Errored)
                        && states.contains(&CodexAgentStatus::Shutdown)
                        && states.contains(&CodexAgentStatus::NotFound)
                );
                assert_eq!(status, expected_status);
            } else {
                panic!("collab view");
            }
        }
    }
    let raw = item(
        r#"{"id":"i","type":"collab_tool_call","tool":"wait","sender_thread_id":"s","receiver_thread_ids":[],"prompt":null,"agents_states":{"x":{"status":"invalid"},"x":{"status":"completed"}},"status":"completed"}"#,
        "completed",
    );
    assert!(
        denied(raw.as_bytes(), &policy(), &generous())
            .sdk_position()
            .is_none()
    );
}

#[test]
fn web_literal_two_ids_and_all_action_variants_preserve_both_ids() {
    let actions = [
        r#"{"type":"search","query":"","queries":[]}"#,
        r#"{"type":"search","query":null,"queries":["a","b"]}"#,
        r#"{"type":"open_page","url":"u"}"#,
        r#"{"type":"find_in_page","url":null,"pattern":"p"}"#,
        r#"{"type":"future","url":42,"url":false}"#,
    ];
    for (index, action) in actions.into_iter().enumerate() {
        let raw = format!(
            r#"{{"id":"outer","type":"web_search","id":"inner","query":"q","action":{action},"results":[1e400,{{"v":1,"v":2}}]}}"#
        );
        let event = known(&item(&raw, "completed"));
        if let CodexEventView::Item { item, .. } = event.view() {
            assert!(item.outer_id == "outer" && item.sdk_observed_id() == "inner");
            if let CodexItemBodyView::WebSearch {
                inner_id,
                query,
                action,
                results,
            } = item.body
            {
                assert!(inner_id == "inner" && query == "q");
                let results = results.unwrap();
                assert_eq!(results.len(), 2);
                assert!(results.get(0).is_some_and(|v| v.as_str() == "1e400"));
                assert_eq!(results.iter().count(), 2);
                match (index, action) {
                    (
                        0,
                        CodexWebActionView::Search {
                            query: Some(""),
                            queries: Some(q),
                        },
                    ) => assert_eq!(q.len(), 0),
                    (
                        1,
                        CodexWebActionView::Search {
                            query: None,
                            queries: Some(q),
                        },
                    ) => {
                        assert!(q.get(0) == Some("a") && q.get(1) == Some("b"));
                        assert_eq!(q.iter().count(), 2);
                    }
                    (2, CodexWebActionView::OpenPage { url: Some("u") }) => {}
                    (
                        3,
                        CodexWebActionView::FindInPage {
                            url: None,
                            pattern: Some("p"),
                        },
                    ) => {}
                    (4, CodexWebActionView::Other) => {}
                    _ => panic!("web action view"),
                }
            } else {
                panic!("web view");
            }
        } else {
            panic!("item view");
        }
    }
    let base = r#"{"id":"","type":"web_search","id":"","query":"","action":{"type":"search"}}"#;
    known(&item(base, "completed"));
    for invalid in [
        base.replacen(r#""id":"",""#, "", 1),
        base.replace(r#""query":"""#, r#""id":"third","query":"""#),
        base.replace(
            r#""id":"","type":"web_search","id":"""#,
            r#""id":"","id":"","type":"web_search""#,
        ),
        base.replacen(r#""id":"""#, r#""id":7"#, 1),
    ] {
        denied(
            item(&invalid, "completed").as_bytes(),
            &policy(),
            &generous(),
        );
    }
}

#[test]
fn required_null_duplicate_and_wrong_type_fields_reject_before_owned_admission() {
    let cases = [
        r#"{"type":"thread.started"}"#,
        r#"{"type":"thread.started","thread_id":null}"#,
        r#"{"type":"thread.started","thread_id":7}"#,
        r#"{"type":"thread.started","thread_id":"a","thread_id":"b"}"#,
        r#"{"type":"turn.started","type":"turn.started"}"#,
        r#"{"type":"turn.failed","error":{}}"#,
        r#"{"type":"error","message":[]}"#,
        r#"{"type":"item.completed","item":null}"#,
        r#"{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0,"cache_write_input_tokens":null}}"#,
    ];
    for raw in cases {
        let limits = policy();
        let error = denied(raw.as_bytes(), &limits, &generous());
        assert_eq!(error.stage(), WireStage::Schema);
        assert!(error.sdk_position().is_none());
    }
    for body in [
        r#"{"id":"i","type":"command_execution","command":"","aggregated_output":"","status":"bad"}"#,
        r#"{"id":"i","type":"file_change","changes":[{"path":"","kind":"add","kind":"update"}],"status":"completed"}"#,
        r#"{"id":"i","type":"todo_list","items":[{"text":"","completed":null}]}"#,
        r#"{"id":"i","type":"mcp_tool_call","server":"s","tool":"t","result":{"content":null},"status":"completed"}"#,
        r#"{"id":"i","type":"web_search","id":"j","query":"q","action":{"type":"search","query":null,"query":"a"}}"#,
    ] {
        denied(item(body, "completed").as_bytes(), &policy(), &generous());
    }
    for missing in [
        r#""id":"abcd","#,
        r#""type":"agent_message","#,
        r#", "text":"longword""#,
    ] {
        let missing = missing.replace(", ", ",");
        let raw = AGENT.replace(&missing, "");
        assert!(raw != AGENT);
        denied(raw.as_bytes(), &policy(), &generous());
    }
}

#[test]
fn independent_required_and_optional_field_corpus_rejects_invalid_records() {
    // Literal SDK contract fixtures: no decoder rules generate these expectations.
    type Field = (&'static str, &'static str, &'static str, bool);
    let cases: &[(&str, &[Field])] = &[
        (
            "@",
            &[
                ("type", "\"thread.started\"", "[]", true),
                ("thread_id", "\"\"", "[]", true),
            ],
        ),
        ("@", &[("type", "\"turn.started\"", "[]", true)]),
        (
            "@",
            &[
                ("type", "\"turn.completed\"", "[]", true),
                (
                    "usage",
                    "{\"input_tokens\":0,\"cached_input_tokens\":0,\"output_tokens\":0,\"reasoning_output_tokens\":0}",
                    "[]",
                    true,
                ),
            ],
        ),
        (
            r#"{"type":"turn.completed","usage":@}"#,
            &[
                ("input_tokens", "0", "[]", true),
                ("cached_input_tokens", "0", "[]", true),
                ("output_tokens", "0", "[]", true),
                ("reasoning_output_tokens", "0", "[]", true),
                ("cache_write_input_tokens", "0", "[]", false),
            ],
        ),
        (
            "@",
            &[
                ("type", "\"turn.failed\"", "[]", true),
                ("error", "{\"message\":\"\"}", "[]", true),
            ],
        ),
        (
            r#"{"type":"turn.failed","error":@}"#,
            &[("message", "\"\"", "[]", true)],
        ),
        (
            "@",
            &[
                ("type", "\"error\"", "[]", true),
                ("message", "\"\"", "[]", true),
            ],
        ),
        (
            "@",
            &[
                ("type", "\"item.started\"", "[]", true),
                (
                    "item",
                    "{\"id\":\"\",\"type\":\"agent_message\",\"text\":\"\"}",
                    "[]",
                    true,
                ),
            ],
        ),
        (
            "@",
            &[
                ("type", "\"item.updated\"", "[]", true),
                (
                    "item",
                    "{\"id\":\"\",\"type\":\"agent_message\",\"text\":\"\"}",
                    "[]",
                    true,
                ),
            ],
        ),
        (
            "@",
            &[
                ("type", "\"item.completed\"", "[]", true),
                (
                    "item",
                    "{\"id\":\"\",\"type\":\"agent_message\",\"text\":\"\"}",
                    "[]",
                    true,
                ),
            ],
        ),
        (
            r#"{"type":"item.completed","item":@}"#,
            &[
                ("id", "\"\"", "[]", true),
                ("type", "\"agent_message\"", "[]", true),
                ("text", "\"\"", "[]", true),
            ],
        ),
        (
            r#"{"type":"item.completed","item":@}"#,
            &[
                ("id", "\"\"", "[]", true),
                ("type", "\"reasoning\"", "[]", true),
                ("text", "\"\"", "[]", true),
            ],
        ),
        (
            r#"{"type":"item.completed","item":@}"#,
            &[
                ("id", "\"\"", "[]", true),
                ("type", "\"command_execution\"", "[]", true),
                ("command", "\"\"", "[]", true),
                ("aggregated_output", "\"\"", "[]", true),
                ("status", "\"completed\"", "[]", true),
                ("exit_code", "0", "[]", false),
            ],
        ),
        (
            r#"{"type":"item.completed","item":@}"#,
            &[
                ("id", "\"\"", "[]", true),
                ("type", "\"file_change\"", "[]", true),
                ("changes", "[]", "{}", true),
                ("status", "\"completed\"", "[]", true),
            ],
        ),
        (
            r#"{"type":"item.completed","item":{"id":"","type":"file_change","changes":[@],"status":"completed"}}"#,
            &[
                ("path", "\"\"", "[]", true),
                ("kind", "\"add\"", "[]", true),
            ],
        ),
        (
            r#"{"type":"item.completed","item":@}"#,
            &[
                ("id", "\"\"", "[]", true),
                ("type", "\"mcp_tool_call\"", "[]", true),
                ("server", "\"\"", "[]", true),
                ("tool", "\"\"", "[]", true),
                ("status", "\"completed\"", "[]", true),
                ("result", "{\"content\":[]}", "[]", false),
                ("error", "{\"message\":\"\"}", "[]", false),
            ],
        ),
        (
            r#"{"type":"item.completed","item":{"id":"","type":"mcp_tool_call","server":"","tool":"","status":"completed","result":@}}"#,
            &[("content", "[]", "{}", true)],
        ),
        (
            r#"{"type":"item.completed","item":{"id":"","type":"mcp_tool_call","server":"","tool":"","status":"completed","error":@}}"#,
            &[("message", "\"\"", "[]", true)],
        ),
        (
            r#"{"type":"item.completed","item":@}"#,
            &[
                ("id", "\"\"", "[]", true),
                ("type", "\"collab_tool_call\"", "[]", true),
                ("tool", "\"wait\"", "[]", true),
                ("sender_thread_id", "\"\"", "[]", true),
                ("receiver_thread_ids", "[]", "{}", true),
                ("agents_states", "{}", "[]", true),
                ("status", "\"completed\"", "[]", true),
                ("prompt", "\"\"", "[]", false),
            ],
        ),
        (
            r#"{"type":"item.completed","item":{"id":"","type":"collab_tool_call","tool":"wait","sender_thread_id":"","receiver_thread_ids":[],"agents_states":{"agent":@},"status":"completed"}}"#,
            &[
                ("status", "\"completed\"", "[]", true),
                ("message", "\"\"", "[]", false),
            ],
        ),
        (
            r#"{"type":"item.completed","item":{"id":"outer","type":"web_search","id":"inner",@}}"#,
            &[
                ("query", "\"\"", "[]", true),
                ("action", "{\"type\":\"search\"}", "[]", true),
                ("results", "[]", "{}", false),
            ],
        ),
        (
            r#"{"type":"item.completed","item":{"id":"outer","type":"web_search","id":"inner","query":"","action":@}}"#,
            &[
                ("type", "\"search\"", "[]", true),
                ("query", "\"\"", "[]", false),
                ("queries", "[]", "{}", false),
            ],
        ),
        (
            r#"{"type":"item.completed","item":{"id":"outer","type":"web_search","id":"inner","query":"","action":@}}"#,
            &[
                ("type", "\"open_page\"", "[]", true),
                ("url", "\"\"", "[]", false),
            ],
        ),
        (
            r#"{"type":"item.completed","item":{"id":"outer","type":"web_search","id":"inner","query":"","action":@}}"#,
            &[
                ("type", "\"find_in_page\"", "[]", true),
                ("url", "\"\"", "[]", false),
                ("pattern", "\"\"", "[]", false),
            ],
        ),
        (
            r#"{"type":"item.completed","item":@}"#,
            &[
                ("id", "\"\"", "[]", true),
                ("type", "\"todo_list\"", "[]", true),
                ("items", "[]", "{}", true),
            ],
        ),
        (
            r#"{"type":"item.completed","item":{"id":"","type":"todo_list","items":[@]}}"#,
            &[
                ("text", "\"\"", "[]", true),
                ("completed", "false", "[]", true),
            ],
        ),
        (
            r#"{"type":"item.completed","item":@}"#,
            &[
                ("id", "\"\"", "[]", true),
                ("type", "\"error\"", "[]", true),
                ("message", "\"\"", "[]", true),
            ],
        ),
    ];
    for (wrapper, fields) in cases {
        let record = |selected: Option<(usize, Option<&str>)>, duplicate: Option<usize>| {
            let mut parts = Vec::new();
            for (index, (key, value, _, _)) in fields.iter().enumerate() {
                let value = match selected {
                    Some((changed, replacement)) if changed == index => replacement,
                    _ => Some(*value),
                };
                if let Some(value) = value {
                    let part = format!("\"{key}\":{value}");
                    parts.push(part.clone());
                    if duplicate == Some(index) {
                        parts.push(part);
                    }
                }
            }
            let joined = parts.join(",");
            let value = if wrapper.contains(",@") {
                joined
            } else {
                format!("{{{joined}}}")
            };
            wrapper.replace('@', &value)
        };
        known(&record(None, None));
        for (index, (_, _, wrong, required)) in fields.iter().enumerate() {
            let mut invalid = vec![
                record(Some((index, Some(wrong))), None),
                record(None, Some(index)),
            ];
            if *required {
                invalid.extend([
                    record(Some((index, None)), None),
                    record(Some((index, Some("null"))), None),
                ]);
            }
            for raw in invalid {
                let limits = policy();
                assert_eq!(
                    denied(raw.as_bytes(), &limits, &generous()).stage(),
                    WireStage::Schema
                );
            }
        }
    }
}

#[test]
fn optional_absent_null_and_empty_values_have_exact_borrowed_views() {
    for (option, expected) in [
        ("", None),
        (",\"exit_code\":null", None),
        (",\"exit_code\":2147483647", Some(i32::MAX)),
    ] {
        let event = known(&item(
            &format!(
                r#"{{"id":"","type":"command_execution","command":"","aggregated_output":"","status":"completed"{option}}}"#
            ),
            "completed",
        ));
        assert!(
            matches!(event.view(), CodexEventView::Item { item: CodexItemView { body: CodexItemBodyView::CommandExecution { exit_code, .. }, .. }, .. } if exit_code == expected)
        );
    }
    for (option, expected) in [
        ("", None),
        (",\"prompt\":null", None),
        (",\"prompt\":\"\"", Some("")),
    ] {
        let event = known(&item(
            &format!(
                r#"{{"id":"","type":"collab_tool_call","tool":"wait","sender_thread_id":"","receiver_thread_ids":[],"agents_states":{{}},"status":"completed"{option}}}"#
            ),
            "completed",
        ));
        match event.view() {
            CodexEventView::Item {
                item:
                    CodexItemView {
                        body:
                            CodexItemBodyView::CollabToolCall {
                                prompt,
                                receiver_thread_ids,
                                agents_states,
                                ..
                            },
                        ..
                    },
                ..
            } => {
                assert_eq!(prompt, expected);
                assert_eq!(receiver_thread_ids.len(), 0);
                assert_eq!(agents_states.len(), 0);
                assert!(agents_states.is_empty());
            }
            _ => panic!("collab optional view"),
        }
    }
    for option in ["", ",\"results\":null", ",\"results\":[]"] {
        for action in [
            r#"{"type":"search"}"#,
            r#"{"type":"search","query":null,"queries":null}"#,
            r#"{"type":"open_page"}"#,
            r#"{"type":"open_page","url":null}"#,
            r#"{"type":"find_in_page"}"#,
            r#"{"type":"find_in_page","url":null,"pattern":null}"#,
        ] {
            let event = known(&item(
                &format!(
                    r#"{{"id":"outer","type":"web_search","id":"inner","query":"","action":{action}{option}}}"#
                ),
                "completed",
            ));
            match event.view() {
                CodexEventView::Item {
                    item:
                        CodexItemView {
                            body:
                                CodexItemBodyView::WebSearch {
                                    action, results, ..
                                },
                            ..
                        },
                    ..
                } => {
                    assert_eq!(
                        results.map(|values| values.len()),
                        if option.ends_with("[]") {
                            Some(0)
                        } else {
                            None
                        }
                    );
                    assert!(matches!(
                        action,
                        CodexWebActionView::Search {
                            query: None,
                            queries: None
                        } | CodexWebActionView::OpenPage { url: None }
                            | CodexWebActionView::FindInPage {
                                url: None,
                                pattern: None
                            }
                    ));
                }
                _ => panic!("web optional view"),
            }
        }
    }
}

#[test]
fn every_typed_integer_boundary_and_negative_zero_retains_actual_cause() {
    for token in [
        "-9223372036854775809",
        "9223372036854775808",
        "1.0",
        "1e0",
        "1e400",
        "null",
        "true",
        "\"payload\"",
    ] {
        let raw = format!(
            r#"{{"type":"turn.completed","usage":{{"input_tokens":{token},"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0}}}}"#
        );
        let error = denied(raw.as_bytes(), &policy(), &generous());
        assert!(matches!(error.cause.0, Cause::Integer(_)));
        assert!(error.sdk_position().is_none());
    }
    let raw = r#"{"type":"turn.completed","usage":{"input_tokens":-0,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0}}"#;
    let error = denied(raw.as_bytes(), &policy(), &generous());
    assert!(matches!(error.cause.0, Cause::Sdk(_)));
    let (category, line, column) = error.sdk_position().unwrap();
    assert_eq!(category, serde_json::error::Category::Data);
    assert!(line <= raw.len() + 1 && column <= raw.len() + 1);
    for token in ["2147483647", "-2147483648", "null"] {
        known(&item(
            &format!(
                r#"{{"id":"i","type":"command_execution","command":"","aggregated_output":"","exit_code":{token},"status":"completed"}}"#
            ),
            "completed",
        ));
    }
    for token in ["2147483648", "-2147483649", "-0", "0.0"] {
        denied(item(&format!(r#"{{"id":"i","type":"command_execution","command":"","aggregated_output":"","exit_code":{token},"status":"completed"}}"#), "completed").as_bytes(), &policy(), &generous());
    }
}

#[test]
fn valid_unknown_is_audited_while_bad_unicode_and_grammar_always_reject() {
    for raw in [
        r#"{"type":"future","x":[1e400,{"v":"\uD83D\uDE00","v":"ok"}]}"#,
        r#"{"type":"item.updated","item":{"type":"future_item","x":1e400}}"#,
    ] {
        match decode_codex_frame(raw.as_bytes(), &policy(), &generous()).unwrap() {
            CodexDecodedFrame::Ignored(stats) => {
                assert!(stats.nodes > 2 && stats.fields > 2 && stats.decoded_string_bytes > 0);
            }
            _ => panic!("unknown classification"),
        }
    }
    for raw in [
        r#"{"type":"future","ignored":"\ud800"}"#,
        r#"{"type":"future","\udfff":0}"#,
        r#"{"type":"turn.started","ignored":"\udc00"}"#,
        r#"{"type":"turn.started"} false"#,
        "",
        " \n\t",
        r#"{"type":"future","x":[1,]}"#,
    ] {
        assert!(
            denied(raw.as_bytes(), &policy(), &generous())
                .sdk_position()
                .is_some()
        );
    }
    let invalid = b"{\"type\":\"future\",\"extra\":\"\xff\"}";
    assert!(matches!(
        denied(invalid, &policy(), &generous()).cause.0,
        Cause::Utf8(_)
    ));
    assert!(
        known(r#"{"type":"thread.started","thread_id":"\u0061\uD83D\uDE00"}"#)
            .stats()
            .decoded_string_bytes
            > 0
    );
}

#[test]
fn all_eight_functional_limits_exact_then_exceeded() {
    let baseline = known(AGENT);
    let stats = baseline.stats();
    let limits = policy();
    let mut cases = Vec::new();
    macro_rules! boundary {
        ($field:ident, $value:expr) => {{
            let mut exact = limits;
            exact.$field = $value;
            let mut over = exact;
            over.$field -= 1;
            cases.push((exact, over));
        }};
    }
    boundary!(frame_bytes, AGENT.len());
    boundary!(depth, 2);
    boundary!(nodes, stats.nodes);
    boundary!(fields, stats.fields);
    boundary!(array_items, stats.array_items);
    boundary!(string_bytes, "item.completed".len());
    boundary!(decoded_string_bytes, stats.decoded_string_bytes);
    boundary!(id_bytes, 4);
    for (exact, over) in cases {
        assert!(decode_codex_frame(AGENT.as_bytes(), &exact, &generous()).is_ok());
        denied(AGENT.as_bytes(), &over, &generous());
    }
    let mut zeros = Vec::new();
    macro_rules! zero {
        ($field:ident) => {{
            let mut value = limits;
            value.$field = 0;
            zeros.push(value);
        }};
    }
    zero!(frame_bytes);
    zero!(depth);
    zero!(nodes);
    zero!(fields);
    zero!(array_items);
    zero!(string_bytes);
    zero!(decoded_string_bytes);
    zero!(id_bytes);
    for zero in zeros {
        assert_eq!(
            denied(AGENT.as_bytes(), &zero, &generous()).stage(),
            WireStage::Policy
        );
    }
    assert!(add(usize::MAX, 1).is_err());
    assert!(add(isize::MAX as usize, 1).is_err());
    assert!(
        arena_bounds(
            1,
            CodexWireStats {
                decoded_string_bytes: usize::MAX,
                ..stats
            },
        )
        .is_err()
    );
    let mut hard = limits;
    hard.frame_bytes = 8 * 1024 * 1024 + 1;
    denied(b"invalid", &hard, &generous());
    hard = limits;
    hard.depth = 129;
    denied(b"invalid", &hard, &generous());
}

#[test]
fn depth_gate_precedes_rawvalue_skip_and_ignores_quoted_braces_and_escape_parity() {
    let mut limits = policy();
    limits.depth = 128;
    let raw = format!(
        r#"{{"type":"future","x":{}0{}}}"#,
        "[".repeat(127),
        "]".repeat(127)
    );
    assert!(matches!(
        decode_codex_frame(raw.as_bytes(), &limits, &generous()).unwrap(),
        CodexDecodedFrame::Ignored(_)
    ));
    let raw = format!(
        r#"{{"type":"future","x":{}0{}}}"#,
        "[".repeat(128),
        "]".repeat(128)
    );
    let error = denied(raw.as_bytes(), &limits, &generous());
    assert_eq!(error.stage(), WireStage::Depth);
    assert!(error.sdk_position().is_none());
    limits.depth = 1;
    assert!(
        decode_codex_frame(
            br#"{"type":"future","text":"[[{{\\\"{{"}"#,
            &limits,
            &generous()
        )
        .is_ok()
    );
}

#[test]
fn first_real_cause_wins_sdk_ok_or_error_and_static_marker_is_not_matched() {
    let native = generous();
    native.cancellation().cancel();
    for sdk_ok in [true, false] {
        let first = native.check().err().unwrap();
        let mut state = TransientState {
            original: &native,
            limits: &policy(),
            queue: Vec::new(),
            queue_limit: 1,
            stats: CodexWireStats::default(),
            first: None,
            stage: WireStage::Audit,
        };
        let marker: serde_json::Error = state.bridge(Cause::Io(first));
        assert!(marker.to_string() == "wire");
        let result = if sdk_ok { Ok(()) } else { Err(marker) };
        assert!(
            matches!(state.finish(result), Err(Cause::Io(error)) if error.kind() == io::ErrorKind::Interrupted)
        );
    }
}

#[test]
fn diagnostics_are_safe_with_actual_sdk_cause_and_pass_local_positions() {
    const CANARY: &str = "wire_fixture_canary_not_a_secret";
    let raw = format!(r#"{{"type":"turn.started","ignored":"{CANARY}","broken":[}}"#);
    let native = generous();
    let error = denied(raw.as_bytes(), &policy(), &native);
    assert!(error.sdk_position().is_some());
    assert!(!format!("{error:?} {error}").contains(CANARY));
    let source = error.source().unwrap();
    assert!(!format!("{source:?} {source}").contains(CANARY));
    assert!(source.source().is_none());
    assert!(matches!(error.cause.0, Cause::Sdk(_)));
    let (_, line, column) = error.sdk_position().unwrap();
    assert!(line <= raw.len() + 1 && column <= raw.len() + 1);
}

#[test]
fn original_cancel_deadline_and_owned_data_do_not_retain_authority() {
    let original = std::sync::Arc::new(generous());
    let weak = std::sync::Arc::downgrade(&original);
    let event = decode_codex_frame(AGENT.as_bytes(), &policy(), &original).unwrap();
    drop(original);
    assert!(weak.upgrade().is_none());
    assert!(matches!(event, CodexDecodedFrame::Known(_)));
    let original = generous();
    original.cancellation().cancel();
    assert_eq!(
        denied(AGENT.as_bytes(), &policy(), &original).io_kind(),
        Some(io::ErrorKind::Interrupted)
    );
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(
        denied(AGENT.as_bytes(), &policy(), &expired).io_kind(),
        Some(io::ErrorKind::TimedOut)
    );
}
