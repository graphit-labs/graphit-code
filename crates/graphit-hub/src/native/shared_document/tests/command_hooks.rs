use super::*;
use crate::native::hooks::{CommandHookInput, NativeHookPlan, plan_command_hook};
use graphit_core::{installation_identity::InstallationId, project_identity::ProjectId};

fn native(binding: NativeBinding, command: &str, request: &RequestContext<'_>) -> NativeHookPlan {
    plan_command_hook(
        binding,
        CommandHookInput {
            identifier: "graphit-context",
            native_event: match binding {
                NativeBinding::Cursor => "sessionStart",
                NativeBinding::Antigravity => "PreInvocation",
                NativeBinding::Kimi => "UserPromptSubmit",
                _ => "SessionStart",
            },
            command,
            matcher: None,
            timeout: Some(Duration::from_secs(10)),
        },
        limits(),
        request,
    )
    .unwrap()
}
fn installation_core(f: &Fixture, request: &RequestContext<'_>) -> ArtifactStore {
    let installation = InstallationPaths::resolve(
        Brand::default(),
        &f.root,
        None,
        Some(&f.root.join("global")),
    )
    .unwrap();
    let scope = CapturedPublicationScope::installation(
        &installation,
        InstallationId::new("hook-test-installation").unwrap(),
        [f.root.join("provider")],
        ScopeLimits::new(16, 16 * 1024, 1024).unwrap(),
        request.snapshot(),
    )
    .unwrap();
    ArtifactStore::new(scope, f.core.limits())
}
fn project(n: u8) -> ProjectId {
    format!("01ARZ3NDEKTSV4RRFFQ69G5FA{n}").parse().unwrap()
}

#[test]
fn five_project_schemas_publish_native_commands_and_retain_foreign_values() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    for binding in [
        NativeBinding::Codex,
        NativeBinding::Claude,
        NativeBinding::Cursor,
        NativeBinding::Qwen,
        NativeBinding::Antigravity,
    ] {
        let f = Fixture::new(&request);
        fs::write(
            f.path("hooks.json"),
            b"{\n// foreign configuration\n\"huge\":1e1000,\"foreign\":null,\"disabled\":true}\n",
        )
        .unwrap();
        let plan = native(binding, "graphit hook first", &request);
        let mut writer = f.writer("hooks.json", &request);
        assert_eq!(
            writer.sync_command_hook(&plan, None).unwrap(),
            SharedDocumentChange::Created
        );
        let first = fs::read(f.path("hooks.json")).unwrap();
        assert_eq!(
            writer.sync_command_hook(&plan, None).unwrap(),
            SharedDocumentChange::Unchanged
        );
        assert_eq!(fs::read(f.path("hooks.json")).unwrap(), first);
        let text = String::from_utf8(first).unwrap();
        assert!(text.contains("// foreign configuration"));
        assert!(text.contains("\"huge\":1e1000"));
        assert!(text.contains("\"foreign\":null"));
        assert!(text.contains("graphit hook first"));
        if binding == NativeBinding::Cursor {
            assert!(text.contains("\"version\""));
        }
        let changed = native(binding, "graphit hook second", &request);
        assert_eq!(
            writer.sync_command_hook(&changed, None).unwrap(),
            SharedDocumentChange::Updated
        );
        assert_eq!(
            writer.remove_command_hook(&changed, None).unwrap(),
            SharedDocumentChange::Removed
        );
        let text = fs::read_to_string(f.path("hooks.json")).unwrap();
        assert!(!text.contains("graphit hook second"));
        assert!(text.contains("\"huge\":1e1000"));
        assert!(text.contains("// foreign configuration"));
    }
}

#[test]
fn grouped_commands_preserve_nested_foreign_handlers_timeout_and_wrapper_overrides() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    fs::write(f.path("settings.json"), br#"{"hooks":{"SessionStart":[{"matcher":"resume","hooks":[{"type":"command","command":"foreign"}]}]},"disableAllHooks":true}"#).unwrap();
    let first = native(NativeBinding::Claude, "graphit first", &request);
    let second = native(NativeBinding::Claude, "graphit second", &request);
    let mut writer = f.writer("settings.json", &request);
    writer.sync_command_hook(&first, None).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(f.path("settings.json")).unwrap()).unwrap();
    let groups = value["hooks"]["SessionStart"].as_array_mut().unwrap();
    let own = groups.last_mut().unwrap();
    own["sequential"] = true.into();
    own["hooks"][0]["timeout"] = 177.into();
    own["hooks"][0]["future"] = serde_json::Value::Null;
    own["hooks"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"type":"command","command":"foreign sibling"}));
    groups.reverse();
    fs::write(f.path("settings.json"), serde_json::to_vec(&value).unwrap()).unwrap();
    writer.sync_command_hook(&second, None).unwrap();
    let updated: serde_json::Value =
        serde_json::from_slice(&fs::read(f.path("settings.json")).unwrap()).unwrap();
    assert_eq!(
        updated["hooks"]["SessionStart"][0]["hooks"][0]["timeout"],
        177
    );
    assert!(
        updated["hooks"]["SessionStart"][0]["sequential"]
            .as_bool()
            .unwrap()
    );
    assert!(updated["disableAllHooks"].as_bool().unwrap());
    writer.remove_command_hook(&second, None).unwrap();
    let removed: serde_json::Value =
        serde_json::from_slice(&fs::read(f.path("settings.json")).unwrap()).unwrap();
    assert_eq!(
        removed["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "foreign sibling"
    );
    assert_eq!(
        removed["hooks"]["SessionStart"][1]["hooks"][0]["command"],
        "foreign"
    );
    // A comment outside the owned handler is foreign even without extra properties.
    let f = Fixture::new(&request);
    let mut writer = f.writer("commented.json", &request);
    writer.sync_command_hook(&first, None).unwrap();
    fs::write(
        f.path("commented.json"),
        br#"{
        "hooks":{"SessionStart":[{
            // foreign wrapper comment
            "hooks":[
                /* foreign array comment */
                {"type":"command","command":"graphit first","timeout":10}
            ]
        }]}
    }"#,
    )
    .unwrap();
    writer.remove_command_hook(&first, None).unwrap();
    let retained = fs::read_to_string(f.path("commented.json")).unwrap();
    assert!(retained.contains("// foreign wrapper comment"));
    assert!(retained.contains("/* foreign array comment */"));
    assert!(!retained.contains("graphit first"));
}

#[test]
fn foreign_equal_commands_critical_edits_and_ambiguous_copies_are_never_adopted() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    for mode in ["foreign", "edited", "duplicate"] {
        let f = Fixture::new(&request);
        let plan = native(NativeBinding::Claude, "graphit command", &request);
        let mut writer = f.writer("settings.json", &request);
        if mode != "foreign" {
            writer.sync_command_hook(&plan, None).unwrap();
        }
        let value = match mode {
            "foreign" => {
                serde_json::json!({"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"graphit command","timeout":10}]}]}})
            }
            "edited" => {
                serde_json::json!({"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"changed by user"}]}]}})
            }
            _ => {
                serde_json::json!({"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"graphit command"},{"type":"command","command":"graphit command"}]}]}})
            }
        };
        let bytes = serde_json::to_vec(&value).unwrap();
        fs::write(f.path("settings.json"), &bytes).unwrap();
        assert!(writer.sync_command_hook(&plan, None).is_err());
        if mode != "foreign" {
            assert!(writer.remove_command_hook(&plan, None).is_err());
        }
        assert_eq!(fs::read(f.path("settings.json")).unwrap(), bytes);
    }
}

#[test]
fn native_timeout_units_and_unavailable_boundaries_are_explicit() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    for (binding, timeout, expected) in [
        (NativeBinding::Qwen, Duration::from_secs(1500), 1_500_000),
        (NativeBinding::Claude, Duration::from_secs(1500), 1500),
    ] {
        let plan = plan_command_hook(
            binding,
            CommandHookInput {
                identifier: "graphit-context",
                native_event: "SessionStart",
                command: "graphit hook",
                matcher: None,
                timeout: Some(timeout),
            },
            limits(),
            &request,
        )
        .unwrap();
        let FieldValue::Object(tuple) = &plan.tuple else {
            unreachable!()
        };
        assert!(tuple.get("timeout") == Some(&FieldValue::Integer(expected)));
    }
    for (binding, event, timeout) in [
        (NativeBinding::Codex, "SessionEnd", Duration::from_secs(4)),
        (
            NativeBinding::Kimi,
            "UserPromptSubmit",
            Duration::from_secs(601),
        ),
        (
            NativeBinding::Cursor,
            "beforeSubmitPrompt",
            Duration::from_secs(5),
        ),
        (
            NativeBinding::OpenCode,
            "SessionStart",
            Duration::from_secs(5),
        ),
    ] {
        assert!(
            plan_command_hook(
                binding,
                CommandHookInput {
                    identifier: "graphit-context",
                    native_event: event,
                    command: "graphit hook",
                    matcher: None,
                    timeout: Some(timeout)
                },
                limits(),
                &request
            )
            .is_err()
        );
    }
}

#[test]
fn kimi_two_project_concurrency_uses_one_global_tuple_and_last_reference_cleanup() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let core = installation_core(&f, &request);
    fs::write(
        f.path("config.toml"),
        "# foreign theme\ntheme='dark'\n[[hooks]]\nevent='Notification'\ncommand='foreign'\n",
    )
    .unwrap();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|threads| {
        for n in [1, 2] {
            let core = &core;
            let f = &f;
            let barrier = &barrier;
            threads.spawn(move || {
                let cancel = Cancellation::default();
                let original = super::request(&cancel);
                let plan = native(NativeBinding::Kimi, "graphit dispatcher", &original);
                barrier.wait();
                let mut writer = f
                    .shared
                    .writer(&f.parent, "config.toml", core, limits(), &original)
                    .unwrap();
                writer.sync_command_hook(&plan, Some(&project(n))).unwrap();
                assert_eq!(
                    writer.sync_command_hook(&plan, Some(&project(n))).unwrap(),
                    SharedDocumentChange::Unchanged
                );
            });
        }
    });
    let plan = native(NativeBinding::Kimi, "graphit dispatcher", &request);
    let mut writer = f
        .shared
        .writer(&f.parent, "config.toml", &core, limits(), &request)
        .unwrap();
    let text = fs::read_to_string(f.path("config.toml")).unwrap();
    assert_eq!(text.matches("graphit dispatcher").count(), 1);
    // Consume the native array-of-tables bytes the writer actually published.
    let document =
        NativeDocument::parse(DocumentFormat::Toml, text.as_bytes(), limits(), &request).unwrap();
    let Some(FieldValue::Array(hooks)) = document.get(&["hooks"]).unwrap() else {
        panic!("Published Kimi hooks must decode as a native array");
    };
    assert_eq!(hooks.len(), 2);
    let FieldValue::Object(foreign) = &hooks[0] else {
        panic!("Foreign hook must remain an object");
    };
    let FieldValue::Object(owned) = &hooks[1] else {
        panic!("Owned hook must remain an object");
    };
    assert!(foreign.get("command") == Some(&FieldValue::String("foreign".into())));
    assert!(owned.get("command") == Some(&FieldValue::String("graphit dispatcher".into())));
    assert!(owned.get("event") == Some(&FieldValue::String("UserPromptSubmit".into())));
    assert_eq!(document.render().unwrap(), text);
    assert!(writer.command_hook_has_references(&plan).unwrap());
    let changed = native(NativeBinding::Kimi, "different dispatcher", &request);
    assert!(
        writer
            .sync_command_hook(&changed, Some(&project(1)))
            .is_err()
    );
    assert_eq!(fs::read_to_string(f.path("config.toml")).unwrap(), text);
    writer
        .remove_command_hook(&plan, Some(&project(1)))
        .unwrap();
    assert!(writer.command_hook_has_references(&plan).unwrap());
    assert!(
        fs::read_to_string(f.path("config.toml"))
            .unwrap()
            .contains("graphit dispatcher")
    );
    writer
        .remove_command_hook(&plan, Some(&project(2)))
        .unwrap();
    assert!(!writer.command_hook_has_references(&plan).unwrap());
    let text = fs::read_to_string(f.path("config.toml")).unwrap();
    assert!(!text.contains("graphit dispatcher"));
    assert!(text.contains("# foreign theme"));
    assert!(text.contains("command='foreign'"));
}

#[test]
fn kimi_recovery_restores_project_references_from_the_real_core_operation() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let core = installation_core(&f, &request);
    let plan = native(NativeBinding::Kimi, "graphit dispatcher", &request);
    let mut writer = f
        .shared
        .writer(&f.parent, "config.toml", &core, limits(), &request)
        .unwrap();
    writer.sync_command_hook(&plan, Some(&project(1))).unwrap();
    let after = writer.persistence.state.entries.clone();
    let desired = fs::read(f.path("config.toml")).unwrap();
    fs::remove_file(f.path("config.toml")).unwrap();
    writer.persistence.state.entries.clear();
    let observed = f
        .parent
        .read(OsStr::new("config.toml"), request.snapshot())
        .unwrap();
    let operation = NativeDocumentOperationId::generate();
    let key = ArtifactKey::new("native-shared", "document", "kimi-recovery").unwrap();
    writer.persistence.state.pending = Some(Pending {
        operation,
        key: key.clone(),
        scope: serde_json::value::to_raw_value(&writer.scope).unwrap(),
        previous: observed.receipt().cloned(),
        owner: writer.owner.clone(),
        after,
        recorded: false,
        released: false,
    });
    writer.persistence.save().unwrap();
    let resolution = writer
        .core
        .prepare_native_document(operation, &key, &f.parent, &observed, &desired)
        .unwrap_or_else(|_| panic!("Core preparation failed"))
        .publish()
        .unwrap_or_else(|_| panic!("Core publication failed"));
    assert_eq!(resolution.outcome(), NativeDocumentOutcome::Applied);
    drop(resolution);
    drop(writer);
    let mut writer = f
        .shared
        .writer(&f.parent, "config.toml", &core, limits(), &request)
        .unwrap();
    assert!(writer.persistence.state.pending.is_none());
    assert!(writer.command_hook_has_references(&plan).unwrap());
    writer.sync_command_hook(&plan, Some(&project(2))).unwrap();
    writer
        .remove_command_hook(&plan, Some(&project(1)))
        .unwrap();
    assert!(
        fs::read_to_string(f.path("config.toml"))
            .unwrap()
            .contains("graphit dispatcher")
    );
    writer
        .remove_command_hook(&plan, Some(&project(2)))
        .unwrap();
    assert!(!writer.command_hook_has_references(&plan).unwrap());
}

#[test]
fn command_hooks_keep_original_cancellation_and_foreign_kimi_registration() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let core = installation_core(&f, &request);
    let plan = native(NativeBinding::Kimi, "graphit dispatcher", &request);
    let bytes = "[[hooks]]\nevent='UserPromptSubmit'\ncommand='graphit dispatcher'\n";
    fs::write(f.path("config.toml"), bytes).unwrap();
    let mut writer = f
        .shared
        .writer(&f.parent, "config.toml", &core, limits(), &request)
        .unwrap();
    assert_eq!(
        writer
            .sync_command_hook(&plan, Some(&project(1)))
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read_to_string(f.path("config.toml")).unwrap(), bytes);
    cancel.cancel();
    assert_eq!(
        writer
            .sync_command_hook(&plan, Some(&project(1)))
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(fs::read_to_string(f.path("config.toml")).unwrap(), bytes);
}

#[test]
fn command_hook_batch_restores_deleted_document_and_rejects_existing_edits() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    for binding in [
        NativeBinding::Codex,
        NativeBinding::Claude,
        NativeBinding::Cursor,
        NativeBinding::Qwen,
        NativeBinding::Antigravity,
    ] {
        let f = Fixture::new(&request);
        let plans: Vec<_> = ["first", "second"]
            .into_iter()
            .map(|name| {
                plan_command_hook(
                    binding,
                    CommandHookInput {
                        identifier: name,
                        native_event: match binding {
                            NativeBinding::Cursor => "sessionStart",
                            NativeBinding::Antigravity => "PreInvocation",
                            _ => "SessionStart",
                        },
                        command: name,
                        matcher: None,
                        timeout: Some(Duration::from_secs(17)),
                    },
                    limits(),
                    &request,
                )
                .unwrap()
            })
            .collect();
        let mut writer = f.writer("hooks.json", &request);
        writer.sync_command_hooks(&plans, None).unwrap();
        let expected = fs::read(f.path("hooks.json")).unwrap();
        drop(writer);
        fs::remove_file(f.path("hooks.json")).unwrap();
        let mut writer = f.writer("hooks.json", &request);
        // A subset must not discard another recorded event from the deleted file.
        assert!(writer.sync_command_hooks(&plans[..1], None).is_err());
        assert!(!f.path("hooks.json").exists());
        assert_eq!(
            writer.sync_command_hooks(&plans, None).unwrap(),
            vec![SharedDocumentChange::Updated; 2]
        );
        assert_eq!(fs::read(f.path("hooks.json")).unwrap(), expected);
        assert_eq!(
            writer.sync_command_hooks(&plans, None).unwrap(),
            vec![SharedDocumentChange::Unchanged; 2]
        );
        // A present document is user content: removing or editing its owned tuple
        // is a conflict, even when it becomes an otherwise empty valid document.
        for changed in [
            String::from_utf8(expected.clone())
                .unwrap()
                .replace("first", "edited"),
            "{}".into(),
        ] {
            fs::write(f.path("hooks.json"), &changed).unwrap();
            assert!(writer.sync_command_hooks(&plans, None).is_err());
            assert_eq!(fs::read_to_string(f.path("hooks.json")).unwrap(), changed);
        }
    }
}

#[test]
fn command_hook_batch_restore_keeps_kimi_project_references() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let core = installation_core(&f, &request);
    let plans = [native(NativeBinding::Kimi, "graphit dispatcher", &request)];
    let mut writer = f
        .shared
        .writer(&f.parent, "config.toml", &core, limits(), &request)
        .unwrap();
    writer
        .sync_command_hooks(&plans, Some(&project(1)))
        .unwrap();
    writer
        .sync_command_hooks(&plans, Some(&project(2)))
        .unwrap();
    let expected = fs::read(f.path("config.toml")).unwrap();
    drop(writer);
    fs::remove_file(f.path("config.toml")).unwrap();
    let mut writer = f
        .shared
        .writer(&f.parent, "config.toml", &core, limits(), &request)
        .unwrap();
    writer
        .sync_command_hooks(&plans, Some(&project(1)))
        .unwrap();
    assert_eq!(fs::read(f.path("config.toml")).unwrap(), expected);
    writer
        .remove_command_hook(&plans[0], Some(&project(1)))
        .unwrap();
    assert!(writer.command_hook_has_references(&plans[0]).unwrap());
    assert!(
        fs::read_to_string(f.path("config.toml"))
            .unwrap()
            .contains("graphit dispatcher")
    );
    writer
        .remove_command_hook(&plans[0], Some(&project(2)))
        .unwrap();
    assert!(!writer.command_hook_has_references(&plans[0]).unwrap());
    assert!(
        !fs::read_to_string(f.path("config.toml"))
            .unwrap()
            .contains("graphit dispatcher")
    );
}
