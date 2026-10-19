use super::*;
use crate::native::{ContentLimits, mcp_config::NativeServerSpec};
use graphit_core::{
    artifact_store::{ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeLimits},
    brand::Brand,
    file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
};
use std::{ffi::OsStr, time::Instant};
mod command_hooks;
mod opencode_plugin;

fn limits() -> SharedWriteLimits {
    SharedWriteLimits {
        input_bytes: 64 * 1024,
        output_bytes: 64 * 1024,
        journal_bytes: 256 * 1024,
        nodes: 4096,
        depth: 32,
        scalar_bytes: 8192,
        entries: 1024,
        hook_tuples: 64,
        references: 64,
        documents: 16,
    }
}
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 64 * 1024,
            max_output_bytes: 64 * 1024,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 8192,
            max_collection_entries: 1024,
        },
        cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}
struct Fixture {
    parent: CapturedDocumentDirectory,
    core: ArtifactStore,
    shared: SharedDocumentStore,
    root: PathBuf,
    _temp: tempfile::TempDir,
}
impl Fixture {
    fn new(request: &RequestContext<'_>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::create_dir(root.join("global")).unwrap();
        fs::create_dir(root.join("provider")).unwrap();
        let installation =
            InstallationPaths::resolve(Brand::default(), &root, None, Some(&root.join("global")))
                .unwrap();
        let project = ProjectPaths::at(&root, &installation).unwrap();
        let scope = CapturedPublicationScope::project(
            &project,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            ScopeLimits::new(16, 16 * 1024, 1024).unwrap(),
            request.snapshot(),
        )
        .unwrap();
        let copy = CopyLimits::new(64 * 1024 * 1024, 10_000, 1024 * 1024, 64).unwrap();
        let core = ArtifactStore::new(scope, ArtifactLimits::new(1024 * 1024, 1024, copy).unwrap());
        let parent = CapturedDocumentDirectory::capture(
            &root.join("provider"),
            NativeDocumentFileLimits::new(64 * 1024, 16 * 1024, 128, 64 * 1024).unwrap(),
            request.snapshot(),
        )
        .unwrap();
        let shared = SharedDocumentStore::open(&root.join("shared")).unwrap();
        Self {
            parent,
            core,
            shared,
            root,
            _temp: temp,
        }
    }
    fn writer<'r, 'c>(
        &'r self,
        name: &str,
        request: &'r RequestContext<'c>,
    ) -> SharedDocumentWriter<'r, 'c> {
        self.shared
            .writer(&self.parent, name, &self.core, limits(), request)
            .unwrap()
    }
    fn path(&self, name: &str) -> PathBuf {
        self.root.join("provider").join(name)
    }
}
fn plan(binding: NativeBinding) -> NativeMcpPlan {
    NativeServerSpec::graphit("graphit").plan(binding).unwrap()
}

#[test]
fn real_writer_installs_updates_and_removes_without_changing_foreign_options() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let path = f.path("config.toml");
    fs::write(&path, "# personal theme\ntheme = 'dark'\n").unwrap();
    let plan = plan(NativeBinding::Codex);
    let mut writer = f.writer("config.toml", &request);
    assert_eq!(writer.sync(&plan).unwrap(), SharedDocumentChange::Created);
    let original = fs::read(&path).unwrap();
    assert_eq!(writer.sync(&plan).unwrap(), SharedDocumentChange::Unchanged);
    assert_eq!(fs::read(&path).unwrap(), original);
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("startup_timeout_sec = 250 # personal timeout\nenabled = false\n");
    fs::write(&path, text).unwrap();
    let mut changed = NativeServerSpec::graphit("graphit");
    if let super::super::mcp_config::NativeServerTransport::Stdio { args, .. } =
        &mut changed.transport
    {
        args.push("--verbose".into());
    }
    let updated = changed.plan(NativeBinding::Codex).unwrap();
    assert_eq!(
        writer.sync(&updated).unwrap(),
        SharedDocumentChange::Updated
    );
    let output = fs::read_to_string(&path).unwrap();
    assert!(output.contains("# personal theme"));
    assert!(output.contains("startup_timeout_sec = 250 # personal timeout"));
    assert!(output.contains("enabled = false"));
    assert!(output.contains("--verbose"));
    assert_eq!(
        writer.remove(&updated).unwrap(),
        SharedDocumentChange::Removed
    );
    let output = fs::read_to_string(&path).unwrap();
    assert!(output.contains("startup_timeout_sec = 250 # personal timeout"));
    assert!(output.contains("enabled = false"));
    assert!(!output.contains("command ="));
    assert!(writer.core.load().unwrap().unwrap().pending().is_none());
}
#[test]
fn kiro_and_qwen_share_one_entry_until_last_reference_is_removed() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let kiro = plan(NativeBinding::KiroV3);
    let qwen = plan(NativeBinding::Qwen);
    let mut writer = f.writer("mcp.json", &request);
    assert_eq!(writer.sync(&kiro).unwrap(), SharedDocumentChange::Created);
    let installed = fs::read(f.path("mcp.json")).unwrap();
    assert_eq!(writer.sync(&qwen).unwrap(), SharedDocumentChange::Unchanged);
    assert_eq!(fs::read(f.path("mcp.json")).unwrap(), installed);
    assert_eq!(writer.remove(&kiro).unwrap(), SharedDocumentChange::Removed);
    assert_eq!(fs::read(f.path("mcp.json")).unwrap(), installed);
    assert_eq!(writer.remove(&qwen).unwrap(), SharedDocumentChange::Removed);
    let result: serde_json::Value =
        serde_json::from_slice(&fs::read(f.path("mcp.json")).unwrap()).unwrap();
    assert!(result["mcpServers"].get("graphit").is_none());
    assert!(writer.persistence.state.entries.is_empty());
}
#[test]
fn foreign_entries_and_edited_critical_fields_are_preserved_on_conflict() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let native = plan(NativeBinding::KiroV3);
    let foreign = r#"{"mcpServers":{"graphit":{"command":"graphit","args":["mcp","--stdio"]}}}"#;
    fs::write(f.path("foreign.json"), foreign).unwrap();
    let mut writer = f.writer("foreign.json", &request);
    assert_eq!(
        writer.sync(&native).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read_to_string(f.path("foreign.json")).unwrap(), foreign);
    drop(writer);
    let mut writer = f.writer("owned.json", &request);
    writer.sync(&native).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(f.path("owned.json")).unwrap()).unwrap();
    value["mcpServers"]["graphit"]["command"] = "edited".into();
    let edited = serde_json::to_vec(&value).unwrap();
    fs::write(f.path("owned.json"), &edited).unwrap();
    assert!(writer.remove(&native).is_err());
    assert_eq!(fs::read(f.path("owned.json")).unwrap(), edited);
    assert!(writer.persistence.state.pending.is_none());
}
#[test]
fn forget_drops_only_registration_even_when_native_document_is_invalid() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let native = plan(NativeBinding::KiroV3);
    let mut writer = f.writer("mcp.json", &request);
    writer.sync(&native).unwrap();
    fs::write(f.path("mcp.json"), b"broken personal document").unwrap();
    assert_eq!(
        writer.forget(&native).unwrap(),
        SharedDocumentChange::Forgotten
    );
    assert_eq!(
        fs::read(f.path("mcp.json")).unwrap(),
        b"broken personal document"
    );
    assert!(writer.persistence.state.entries.is_empty());
}
#[test]
fn recovery_of_published_core_operation_finishes_without_replaying_document() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let native = plan(NativeBinding::KiroV3);
    let mut writer = f.writer("mcp.json", &request);
    let observed = f
        .parent
        .read(OsStr::new("mcp.json"), request.snapshot())
        .unwrap();
    let mut doc = NativeDocument::parse(native.format, b"{}", limits(), &request).unwrap();
    for field in &native.fields {
        doc.set(&refs(&field.path), field.native_value().unwrap())
            .unwrap();
    }
    let desired = doc.render().unwrap();
    let operation = NativeDocumentOperationId::generate();
    let key = ArtifactKey::new("native-shared", "document", "recovery").unwrap();
    writer.persistence.state.pending = Some(Pending {
        operation,
        key: key.clone(),
        scope: serde_json::value::to_raw_value(&writer.scope).unwrap(),
        previous: observed.receipt().cloned(),
        owner: writer.owner.clone(),
        after: vec![Entry::Mcp(McpEntry {
            path: native.entry.clone(),
            owner: writer.owner.clone(),
            bindings: vec![native.binding],
            fields: native.fields.clone(),
        })],
        recorded: false,
        released: false,
    });
    writer.persistence.save().unwrap();
    let resolution = writer
        .core
        .prepare_native_document(operation, &key, &f.parent, &observed, desired.as_bytes())
        .unwrap_or_else(|failure| panic!("preparation failed: {:?}", failure.into_parts().0))
        .publish()
        .unwrap_or_else(|failure| panic!("publication failed: {:?}", failure.into_parts().0));
    assert_eq!(resolution.outcome(), NativeDocumentOutcome::Applied);
    drop(resolution);
    drop(writer);
    let installed = fs::read(f.path("mcp.json")).unwrap();
    let writer = f.writer("mcp.json", &request);
    assert_eq!(fs::read(f.path("mcp.json")).unwrap(), installed);
    assert!(writer.persistence.state.pending.is_none());
    assert_eq!(writer.persistence.state.entries.len(), 1);
    assert!(writer.core.load().unwrap().unwrap().pending().is_none());
}

#[test]
fn jsonc_publication_preserves_comments_and_foreign_native_literals() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let native = plan(NativeBinding::OpenCode).jsonc().unwrap();
    let input = "{\n // personal configuration\n \"theme\":\"dark\",\n \"huge\":1e1000,\n \"mcp\":{\"servers\":{\"other\":{\"type\":\"local\",\"command\":[\"other\"]}}},\n}\n";
    fs::write(f.path("opencode.jsonc"), input).unwrap();
    let mut writer = f.writer("opencode.jsonc", &request);
    writer.sync(&native).unwrap();
    let output = fs::read_to_string(f.path("opencode.jsonc")).unwrap();
    assert!(output.contains("// personal configuration"));
    assert!(output.contains("\"huge\":1e1000"));
    assert!(output.contains("\"other\":{\"type\":\"local\",\"command\":[\"other\"]}"));
    writer.remove(&native).unwrap();
    let output = fs::read_to_string(f.path("opencode.jsonc")).unwrap();
    assert!(output.contains("// personal configuration"));
    assert!(output.contains("\"huge\":1e1000"));
    assert!(!output.contains("\"graphit\""));
}
#[test]
fn intent_before_core_preparation_is_discarded_without_publishing() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let mut writer = f.writer("mcp.json", &request);
    writer.persistence.state.pending = Some(Pending {
        operation: NativeDocumentOperationId::generate(),
        key: ArtifactKey::new("native-shared", "document", "before-preparation").unwrap(),
        scope: serde_json::value::to_raw_value(&writer.scope).unwrap(),
        previous: None,
        owner: writer.owner.clone(),
        after: Vec::new(),
        recorded: false,
        released: false,
    });
    writer.persistence.save().unwrap();
    drop(writer);
    let writer = f.writer("mcp.json", &request);
    assert!(writer.persistence.state.pending.is_none());
    assert!(!f.path("mcp.json").exists());
    assert!(writer.core.load().unwrap().is_none());
}

#[test]
fn all_bindings_publish_native_configurations_and_preserve_foreign_servers() {
    for binding in [
        NativeBinding::Antigravity,
        NativeBinding::Claude,
        NativeBinding::Codex,
        NativeBinding::Cursor,
        NativeBinding::KiroV3,
        NativeBinding::OpenCode,
        NativeBinding::Qwen,
        NativeBinding::Kimi,
    ] {
        let cancel = Cancellation::default();
        let request = request(&cancel);
        let f = Fixture::new(&request);
        let native = plan(binding);
        let input = if native.format == DocumentFormat::Toml {
            "# personal config\n[mcp_servers.foreign]\ncommand='foreign'\n"
        } else if binding == NativeBinding::OpenCode {
            r#"{"mcp":{"servers":{"foreign":{"type":"local","command":["foreign"]}}}}"#
        } else {
            r#"{"mcpServers":{"foreign":{"command":"foreign"}}}"#
        };
        fs::write(f.path("config"), input).unwrap();
        let mut writer = f.writer("config", &request);
        assert_eq!(writer.sync(&native).unwrap(), SharedDocumentChange::Created);
        let observed = f
            .parent
            .read(OsStr::new("config"), request.snapshot())
            .unwrap();
        let document =
            NativeDocument::parse(native.format, observed.bytes().unwrap(), limits(), &request)
                .unwrap();
        for field in native.fields() {
            assert!(
                document.get(&refs(field.path())).unwrap() == Some(field.native_value().unwrap())
            );
        }
        assert_eq!(
            writer
                .remove(&native)
                .unwrap_or_else(|error| panic!("{binding:?}: {error}")),
            SharedDocumentChange::Removed
        );
        let output = fs::read_to_string(f.path("config")).unwrap();
        assert!(output.contains("foreign"));
        assert!(!output.contains("graphit"));
    }
}

fn hook(binding: NativeBinding, body: &str, request: &RequestContext<'_>) -> KiroHookPlan {
    use crate::native::kiro_hooks::{
        KiroHookAction, KiroHookInput, KiroHookTrigger, plan_kiro_hook,
    };
    plan_kiro_hook(
        binding,
        KiroHookInput {
            identifier: "graphit-check",
            trigger: KiroHookTrigger::PreToolUse,
            matcher: None,
            action: KiroHookAction::Command(body),
            timeout: Some(Duration::from_secs(7)),
        },
        limits(),
        request,
    )
    .unwrap()
}
fn hook_fields(plan: &KiroHookPlan) -> TupleFields {
    let FieldValue::Object(tuple) = plan.tuple() else {
        panic!("native hook object required")
    };
    plan.critical_fields()
        .iter()
        .map(|key| ((*key).into(), tuple.get(*key).cloned()))
        .collect()
}
fn hook_array(value: &mut serde_json::Value) -> &mut Vec<serde_json::Value> {
    value["hooks"].as_array_mut().unwrap()
}

#[test]
fn hook_lifecycle_preserves_foreign_tuples_comments_and_user_extensions() {
    for binding in [NativeBinding::KiroV3] {
        for format in [DocumentFormat::Json, DocumentFormat::Jsonc] {
            let cancel = Cancellation::default();
            let req = request(&cancel);
            let f = Fixture::new(&req);
            let native = hook(binding, "before", &req);
            let foreign = {
                r#"{"name":"foreign","trigger":"PreToolUse","action":{"type":"command","command":"foreign"},"huge":1e1000,"metadata":null}"#
            };
            let event = { format!("[{foreign}]") };
            let comment = if format == DocumentFormat::Jsonc {
                "// personal configuration\n"
            } else {
                ""
            };
            let input = format!(r#"{{{comment}"theme":"dark","hooks":{event}}}"#);
            fs::write(f.path("hooks.json"), input).unwrap();
            let mut writer = f.writer("hooks.json", &req);
            assert_eq!(
                writer.sync_hook(&native, format).unwrap(),
                SharedDocumentChange::Created
            );
            let installed = fs::read(f.path("hooks.json")).unwrap();
            assert_eq!(
                writer.sync_hook(&native, format).unwrap(),
                SharedDocumentChange::Unchanged
            );
            assert_eq!(fs::read(f.path("hooks.json")).unwrap(), installed);
            // Simulate a user's native edits without decoding foreign floats/null.
            let text = String::from_utf8(installed).unwrap();
            let options = jsonc_parser::ParseOptions {
                allow_comments: format == DocumentFormat::Jsonc,
                allow_trailing_commas: format == DocumentFormat::Jsonc,
                ..Default::default()
            };
            let root = jsonc_parser::cst::CstRootNode::parse(&text, &options).unwrap();
            let object = root.object_value().unwrap();
            let array = { object.array_value("hooks").unwrap() };
            let owned = array.elements().last().unwrap().as_object().unwrap();
            owned
                .get("timeout")
                .unwrap()
                .set_value(jsonc_parser::cst::CstInputValue::Number("250".into()));
            owned.append("disabled", jsonc_parser::cst::CstInputValue::Bool(true));
            owned.append(
                "extension",
                jsonc_parser::cst::CstInputValue::Number("1e1000".into()),
            );
            fs::write(f.path("hooks.json"), root.to_string()).unwrap();
            let updated = hook(binding, "after", &req);
            assert_eq!(
                writer.sync_hook(&updated, format).unwrap(),
                SharedDocumentChange::Updated
            );
            let output = fs::read_to_string(f.path("hooks.json")).unwrap();
            assert!(output.contains(foreign));
            if format == DocumentFormat::Jsonc {
                assert!(output.contains("// personal configuration"));
            }
            let doc = NativeDocument::parse(format, output.as_bytes(), limits(), &req).unwrap();
            let mut fields = hook_fields(&updated);
            fields.insert("timeout".into(), Some(FieldValue::Integer(250)));
            fields.insert("disabled".into(), Some(FieldValue::Boolean(true)));
            assert!(matches!(
                doc.tuple_match(updated.array_path(), &fields).unwrap(),
                TupleMatch::Unique(1)
            ));
            assert!(
                output.contains("250")
                    && output.contains("disabled")
                    && output.contains("extension")
            );
            assert!(!output.contains("before"));
            assert_eq!(
                writer.remove_hook(&updated, format).unwrap(),
                SharedDocumentChange::Removed
            );
            let output = fs::read_to_string(f.path("hooks.json")).unwrap();
            assert!(output.contains(foreign));
            assert!(
                !output.contains("after")
                    && !output.contains("250")
                    && !output.contains("disabled")
            );
            assert!(writer.persistence.state.entries.is_empty());
            assert!(writer.core.load().unwrap().unwrap().pending().is_none());
        }
    }
}

#[test]
fn hook_reordering_updates_the_registered_tuple_and_preserves_mcp_references() {
    {
        let binding = NativeBinding::KiroV3;
        let cancel = Cancellation::default();
        let req = request(&cancel);
        let f = Fixture::new(&req);
        let native = hook(binding, "before", &req);
        let mut writer = f.writer("hooks.json", &req);
        writer.sync_hook(&native, DocumentFormat::Json).unwrap();
        let mut editor: serde_json::Value =
            serde_json::from_slice(&fs::read(f.path("hooks.json")).unwrap()).unwrap();
        let array = hook_array(&mut editor);
        array.insert(0, serde_json::json!({"name":"foreign","trigger":"Stop","action":{"type":"command","command":"foreign"}}));
        fs::write(f.path("hooks.json"), serde_json::to_vec(&editor).unwrap()).unwrap();
        let updated = hook(binding, "after", &req);
        writer.sync_hook(&updated, DocumentFormat::Json).unwrap();
        let output = fs::read(f.path("hooks.json")).unwrap();
        let doc = NativeDocument::parse(DocumentFormat::Json, &output, limits(), &req).unwrap();
        assert!(matches!(
            doc.tuple_match(updated.array_path(), &hook_fields(&updated))
                .unwrap(),
            TupleMatch::Unique(1)
        ));
        let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!({ &result["hooks"][0]["name"] }, "foreign");
    }
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let f = Fixture::new(&req);
    let mut writer = f.writer("agent.json", &req);
    let native = hook(NativeBinding::KiroV3, "owned", &req);
    let kiro = plan(NativeBinding::KiroV3);
    let qwen = plan(NativeBinding::Qwen);
    writer.sync(&kiro).unwrap();
    writer.sync(&qwen).unwrap();
    writer.sync_hook(&native, DocumentFormat::Json).unwrap();
    writer.remove(&kiro).unwrap();
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(f.path("agent.json")).unwrap()).unwrap();
    assert!(value["mcpServers"].get("graphit").is_some());
    writer.remove(&qwen).unwrap();
    assert_eq!(
        writer.sync_hook(&native, DocumentFormat::Json).unwrap(),
        SharedDocumentChange::Unchanged
    );
    writer.remove_hook(&native, DocumentFormat::Json).unwrap();
    assert!(writer.persistence.state.entries.is_empty());
}

#[test]
fn equal_foreign_hook_tuples_are_collisions_without_adoption() {
    {
        let binding = NativeBinding::KiroV3;
        let cancel = Cancellation::default();
        let req = request(&cancel);
        let f = Fixture::new(&req);
        let native = hook(binding, "same", &req);
        let mut doc = NativeDocument::parse(DocumentFormat::Json, b"{}", limits(), &req).unwrap();
        {
            let version = native.version();
            doc.set(&["version"], FieldValue::String(version.into()))
                .unwrap();
        }
        doc.edit_tuple(native.array_path(), TupleEdit::Append(native.tuple()))
            .unwrap();
        let original = doc.render().unwrap();
        fs::write(f.path("foreign.json"), &original).unwrap();
        let mut writer = f.writer("foreign.json", &req);
        assert_eq!(
            writer
                .sync_hook(&native, DocumentFormat::Json)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            fs::read_to_string(f.path("foreign.json")).unwrap(),
            original
        );
        assert!(writer.persistence.state.entries.is_empty());
        assert_eq!(
            writer.remove_hook(&native, DocumentFormat::Json).unwrap(),
            SharedDocumentChange::Unchanged
        );
        assert_eq!(
            writer.forget_hook(&native).unwrap(),
            SharedDocumentChange::Unchanged
        );
    }
}

#[test]
fn edited_missing_or_duplicate_hook_identity_is_never_overwritten() {
    for binding in [NativeBinding::KiroV3] {
        for change in ["edited", "missing", "duplicate", "matcher"] {
            let cancel = Cancellation::default();
            let req = request(&cancel);
            let f = Fixture::new(&req);
            let native = hook(binding, "owned", &req);
            let mut writer = f.writer("hooks.json", &req);
            writer.sync_hook(&native, DocumentFormat::Json).unwrap();
            let mut editor: serde_json::Value =
                serde_json::from_slice(&fs::read(f.path("hooks.json")).unwrap()).unwrap();
            let array = hook_array(&mut editor);
            match change {
                "edited" => {
                    array[0]["action"]["command"] = "user".into();
                }
                "missing" => {
                    array.clear();
                }
                "duplicate" => {
                    array.push(array[0].clone());
                }
                "matcher" => {
                    array[0]["matcher"] = "".into();
                }
                _ => unreachable!(),
            }
            let original = serde_json::to_vec(&editor).unwrap();
            fs::write(f.path("hooks.json"), &original).unwrap();
            assert_eq!(
                writer
                    .sync_hook(&native, DocumentFormat::Json)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::Other
            );
            assert_eq!(
                writer
                    .remove_hook(&native, DocumentFormat::Json)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::Other
            );
            assert_eq!(fs::read(f.path("hooks.json")).unwrap(), original);
            assert_eq!(writer.persistence.state.entries.len(), 1);
            assert_eq!(
                writer.forget_hook(&native).unwrap(),
                SharedDocumentChange::Forgotten
            );
            assert_eq!(fs::read(f.path("hooks.json")).unwrap(), original);
        }
    }
}

#[test]
fn hook_updates_cannot_collide_with_another_native_name() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let f = Fixture::new(&req);
    let native = hook(NativeBinding::KiroV3, "owned", &req);
    let mut writer = f.writer("hooks.json", &req);
    writer.sync_hook(&native, DocumentFormat::Json).unwrap();
    let mut editor: serde_json::Value =
        serde_json::from_slice(&fs::read(f.path("hooks.json")).unwrap()).unwrap();
    hook_array(&mut editor).push(serde_json::json!({"name":"graphit-check","trigger":"Stop","action":{"type":"command","command":"future"}}));
    let original = serde_json::to_vec(&editor).unwrap();
    fs::write(f.path("hooks.json"), &original).unwrap();
    assert_eq!(
        writer
            .sync_hook(
                &hook(NativeBinding::KiroV3, "future", &req),
                DocumentFormat::Json
            )
            .unwrap_err()
            .kind(),
        io::ErrorKind::Other
    );
    assert_eq!(fs::read(f.path("hooks.json")).unwrap(), original);
    // V3's native name collides even when the foreign action differs.
    drop(writer);
    let current = hook(NativeBinding::KiroV3, "requested", &req);
    fs::write(f.path("v3.json"),br#"{"version":"v1","hooks":[{"name":"graphit-check","trigger":"Stop","action":{"type":"agent","prompt":"foreign"}}]}"#).unwrap();
    let original = fs::read(f.path("v3.json")).unwrap();
    let mut v3writer = f.writer("v3.json", &req);
    assert_eq!(
        v3writer
            .sync_hook(&current, DocumentFormat::Json)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(f.path("v3.json")).unwrap(), original);
}

#[test]
fn incompatible_hook_event_format_and_native_shape_fail_before_publication() {
    use crate::native::kiro_hooks::{
        KiroHookAction, KiroHookInput, KiroHookTrigger, plan_kiro_hook,
    };
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let f = Fixture::new(&req);
    let native = hook(NativeBinding::KiroV3, "owned", &req);
    let mut writer = f.writer("hooks.json", &req);
    writer.sync_hook(&native, DocumentFormat::Json).unwrap();
    let original = fs::read(f.path("hooks.json")).unwrap();
    for format in [DocumentFormat::Toml, DocumentFormat::Jsonc] {
        assert_eq!(
            writer.sync_hook(&native, format).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    let stop = plan_kiro_hook(
        NativeBinding::KiroV3,
        KiroHookInput {
            identifier: "graphit-check",
            trigger: KiroHookTrigger::Stop,
            matcher: None,
            action: KiroHookAction::Command("owned"),
            timeout: None,
        },
        limits(),
        &req,
    )
    .unwrap();
    assert_eq!(
        writer
            .remove_hook(&stop, DocumentFormat::Json)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        writer.forget_hook(&stop).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(fs::read(f.path("hooks.json")).unwrap(), original);
    fs::write(f.path("hooks.json"), b"broken personal document").unwrap();
    assert_eq!(
        writer.forget_hook(&native).unwrap(),
        SharedDocumentChange::Forgotten
    );
    assert_eq!(
        fs::read(f.path("hooks.json")).unwrap(),
        b"broken personal document"
    );
    drop(writer);
    for (index, input) in [
        r#"{"version":"v2","hooks":[]}"#,
        r#"{"version":"v1","hooks":{}}"#,
        r#"{"version":"v1","hooks":[null]}"#,
        r#"{"hooks":"user"}"#,
    ]
    .iter()
    .enumerate()
    {
        let name = format!("invalid{index}.json");
        fs::write(f.path(&name), input).unwrap();
        let mut writer = f.writer(&name, &req);
        assert!(writer.sync_hook(&native, DocumentFormat::Json).is_err());
        assert_eq!(fs::read_to_string(f.path(&name)).unwrap(), *input);
        assert!(writer.persistence.state.entries.is_empty());
    }
}

#[test]
fn hook_writer_keeps_original_cancellation_deadline_and_codec_limits() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let f = Fixture::new(&req);
    let native = hook(NativeBinding::KiroV3, "owned", &req);
    let mut writer = f.writer("hooks.json", &req);
    writer.sync_hook(&native, DocumentFormat::Json).unwrap();
    let original = fs::read(f.path("hooks.json")).unwrap();
    cancel.cancel();
    assert_eq!(
        writer
            .sync_hook(&native, DocumentFormat::Json)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(
        writer
            .remove_hook(&native, DocumentFormat::Json)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(
        writer.forget_hook(&native).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(fs::read(f.path("hooks.json")).unwrap(), original);
    let cancel = Cancellation::default();
    let req = RequestContext::new(
        ContentLimits {
            max_input_bytes: 64 * 1024,
            max_output_bytes: 64 * 1024,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 8192,
            max_collection_entries: 1024,
        },
        &cancel,
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    let f = Fixture::new(&req);
    let native = hook(NativeBinding::KiroV3, "owned", &req);
    let mut writer = f.writer("hooks.json", &req);
    writer.sync_hook(&native, DocumentFormat::Json).unwrap();
    let original = fs::read(f.path("hooks.json")).unwrap();
    std::thread::sleep(req.remaining().unwrap().unwrap());
    assert_eq!(
        writer
            .sync_hook(&native, DocumentFormat::Json)
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(fs::read(f.path("hooks.json")).unwrap(), original);
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let f = Fixture::new(&req);
    let native = hook(NativeBinding::KiroV3, &"x".repeat(200), &req);
    let mut small = limits();
    small.scalar_bytes = 128;
    small.output_bytes = 128;
    let mut writer = f
        .shared
        .writer(&f.parent, "small.json", &f.core, small, &req)
        .unwrap();
    assert_eq!(
        writer
            .sync_hook(&native, DocumentFormat::Json)
            .unwrap_err()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    assert!(!f.path("small.json").exists());
    assert!(writer.persistence.state.entries.is_empty());
}

#[test]
fn published_hook_recovery_uses_the_real_core_operation_without_replaying_prompt() {
    use crate::native::kiro_hooks::{
        KiroHookAction, KiroHookInput, KiroHookTrigger, plan_kiro_hook,
    };
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let f = Fixture::new(&req);
    let native = plan_kiro_hook(
        NativeBinding::KiroV3,
        KiroHookInput {
            identifier: "graphit-check",
            trigger: KiroHookTrigger::PreToolUse,
            matcher: None,
            action: KiroHookAction::AgentPrompt("already rendered Unicode ação\n{{literal}} $PATH"),
            timeout: None,
        },
        limits(),
        &req,
    )
    .unwrap();
    let mut writer = f.writer("hooks.json", &req);
    let observed = f
        .parent
        .read(OsStr::new("hooks.json"), req.snapshot())
        .unwrap();
    let mut doc = NativeDocument::parse(DocumentFormat::Json, b"{}", limits(), &req).unwrap();
    doc.set(&["version"], FieldValue::String("v1".into()))
        .unwrap();
    doc.edit_tuple(native.array_path(), TupleEdit::Append(native.tuple()))
        .unwrap();
    let desired = doc.render().unwrap();
    let operation = NativeDocumentOperationId::generate();
    let key = ArtifactKey::new("native-shared", "document", "hook-recovery").unwrap();
    writer.persistence.state.pending = Some(Pending {
        operation,
        key: key.clone(),
        scope: serde_json::value::to_raw_value(&writer.scope).unwrap(),
        previous: observed.receipt().cloned(),
        owner: writer.owner.clone(),
        after: vec![Entry::Hook(HookEntry {
            identifier: native.identifier().into(),
            owner: writer.owner.clone(),
            binding: native.binding(),
            path: native
                .array_path()
                .iter()
                .map(|key| (*key).into())
                .collect(),
            format: HookDocumentFormat::Native(DocumentFormat::Json),
            fields: hook_fields(&native),
        })],
        recorded: false,
        released: false,
    });
    writer.persistence.save().unwrap();
    let resolution = writer
        .core
        .prepare_native_document(operation, &key, &f.parent, &observed, desired.as_bytes())
        .unwrap_or_else(|failure| panic!("preparation failed: {:?}", failure.into_parts().0))
        .publish()
        .unwrap_or_else(|failure| panic!("publication failed: {:?}", failure.into_parts().0));
    assert_eq!(resolution.outcome(), NativeDocumentOutcome::Applied);
    drop(resolution);
    drop(writer);
    let installed = fs::read(f.path("hooks.json")).unwrap();
    let mut writer = f.writer("hooks.json", &req);
    assert_eq!(fs::read(f.path("hooks.json")).unwrap(), installed);
    assert!(writer.persistence.state.pending.is_none());
    assert_eq!(
        writer.sync_hook(&native, DocumentFormat::Json).unwrap(),
        SharedDocumentChange::Unchanged
    );
    assert_eq!(fs::read(f.path("hooks.json")).unwrap(), installed);
    assert!(writer.core.load().unwrap().unwrap().pending().is_none());
    writer.remove_hook(&native, DocumentFormat::Json).unwrap();
    assert!(writer.persistence.state.entries.is_empty());
}

fn kiro_profile_limits() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 64 * 1024,
        max_output_bytes: 64 * 1024,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 8192,
        max_collection_entries: 1024,
    }
}
#[test]
fn kiro_profile_hooks_preserve_body_foreign_metadata_and_both_events_under_real_writer() {
    use crate::native::kiro_hooks::{
        KiroHookAction, KiroHookInput, KiroHookTrigger, plan_kiro_hook,
    };
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let f = Fixture::new(&req);
    let body = "\nPROJECT literal ação $LITERAL {{ authored }}\n---\nbody stays byte-for-byte\r\n";
    let input = format!(
        "---\nname: coordinator\ndescription: Project coordinator\ntools: [read]\nextension:\n  nullable: null\n  decimal: 2.25\nhooks:\n  - name: foreign\n    trigger: Stop\n    action:\n      type: command\n      command: foreign\n---\n{body}"
    );
    fs::write(f.path("coordinator.md"), &input).unwrap();
    let make = |identifier, trigger, command| {
        plan_kiro_hook(
            NativeBinding::KiroV3,
            KiroHookInput {
                identifier,
                trigger,
                matcher: None,
                action: KiroHookAction::Command(command),
                timeout: Some(Duration::from_secs(7)),
            },
            limits(),
            &req,
        )
        .unwrap()
    };
    let session = make(
        "project-session",
        KiroHookTrigger::SessionStart,
        "session command",
    );
    let prompt = make(
        "project-prompt",
        KiroHookTrigger::UserPromptSubmit,
        "prompt command",
    );
    let mut writer = f.writer("coordinator.md", &req);
    for plan in [&session, &prompt] {
        assert_eq!(
            writer
                .sync_kiro_profile_hook(plan, kiro_profile_limits())
                .unwrap(),
            SharedDocumentChange::Created
        );
    }
    let installed = fs::read(f.path("coordinator.md")).unwrap();
    let mut context = RenderContext::new(
        kiro_profile_limits(),
        &cancel,
        req.snapshot().required_deadline().unwrap(),
    )
    .unwrap();
    let (metadata, actual_body) =
        inspect_kiro_coordinator_profile(&installed, "coordinator", &mut context).unwrap();
    assert_eq!(actual_body.as_bytes(), body.as_bytes());
    assert_eq!(metadata["extension"]["nullable"], serde_json::Value::Null);
    assert_eq!(metadata["extension"]["decimal"], 2.25);
    assert_eq!(
        metadata["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["foreign", "project-session", "project-prompt"]
    );
    assert_eq!(metadata["hooks"][1]["trigger"], "SessionStart");
    assert_eq!(metadata["hooks"][2]["trigger"], "UserPromptSubmit");
    assert!(metadata.get("version").is_none());
    for plan in [&session, &prompt] {
        assert_eq!(
            writer
                .sync_kiro_profile_hook(plan, kiro_profile_limits())
                .unwrap(),
            SharedDocumentChange::Unchanged
        );
    }
    assert_eq!(fs::read(f.path("coordinator.md")).unwrap(), installed);
    assert_eq!(
        writer
            .remove_kiro_profile_hook(&session, kiro_profile_limits())
            .unwrap(),
        SharedDocumentChange::Removed
    );
    assert_eq!(
        writer
            .remove_kiro_profile_hook(&prompt, kiro_profile_limits())
            .unwrap(),
        SharedDocumentChange::Removed
    );
    let removed = fs::read(f.path("coordinator.md")).unwrap();
    let mut context = RenderContext::new(
        kiro_profile_limits(),
        &cancel,
        req.snapshot().required_deadline().unwrap(),
    )
    .unwrap();
    let (metadata, actual_body) =
        inspect_kiro_coordinator_profile(&removed, "coordinator", &mut context).unwrap();
    assert_eq!(actual_body.as_bytes(), body.as_bytes());
    assert_eq!(metadata["hooks"].as_array().unwrap().len(), 1);
    assert_eq!(metadata["hooks"][0]["name"], "foreign");
    assert!(writer.persistence.state.entries.is_empty());
    assert!(writer.core.load().unwrap().unwrap().pending().is_none());
}

#[test]
fn kiro_profile_hook_denials_collisions_limits_and_original_preserve_the_actual_file() {
    let cancel = Cancellation::default();
    let req = request(&cancel);
    let f = Fixture::new(&req);
    let native = hook(NativeBinding::KiroV3, "owned", &req);
    for (name, header, kind) in [
        (
            "coordinator",
            "name: coordinator\ndescription: Coordinator\ntools: []\nhooks:\n  - name: graphit-check\n    trigger: Stop\n    action: {type: command, command: foreign}\n",
            io::ErrorKind::AlreadyExists,
        ),
        (
            "graphit-scout",
            "name: graphit-scout\ndescription: Role\ntools: []\n",
            io::ErrorKind::PermissionDenied,
        ),
        (
            "coordinator",
            "name: coordinator\ndescription: Coordinator\ntools: []\nmcpServers:\n  graphit:\n    command: graphit\n    args: [mcp, --stdio, --role]\n",
            io::ErrorKind::PermissionDenied,
        ),
        (
            "coordinator",
            "name: other\ndescription: Coordinator\ntools: []\n",
            io::ErrorKind::InvalidData,
        ),
        (
            "coordinator",
            "name: coordinator\nname: coordinator\ndescription: Coordinator\ntools: []\n",
            io::ErrorKind::InvalidData,
        ),
    ] {
        let file = format!("{name}.md");
        let input = format!("---\n{header}---\nLiteral body.\n");
        fs::write(f.path(&file), &input).unwrap();
        let mut writer = f.writer(&file, &req);
        assert_eq!(
            writer
                .sync_kiro_profile_hook(&native, kiro_profile_limits())
                .unwrap_err()
                .kind(),
            kind
        );
        assert_eq!(fs::read(f.path(&file)).unwrap(), input.as_bytes());
        assert!(writer.persistence.state.entries.is_empty());
    }
    let input = b"---\nname: coordinator\ndescription: Coordinator\ntools: []\n---\nBody\n";
    fs::write(f.path("coordinator.md"), input).unwrap();
    let mut writer = f.writer("coordinator.md", &req);
    for limited in [
        ContentLimits {
            max_input_bytes: input.len() - 1,
            ..kiro_profile_limits()
        },
        ContentLimits {
            max_output_bytes: 8,
            ..kiro_profile_limits()
        },
    ] {
        assert_eq!(
            writer
                .sync_kiro_profile_hook(&native, limited)
                .unwrap_err()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
        assert_eq!(fs::read(f.path("coordinator.md")).unwrap(), input);
        assert!(writer.persistence.state.entries.is_empty());
    }
    cancel.cancel();
    assert_eq!(
        writer
            .sync_kiro_profile_hook(&native, kiro_profile_limits())
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(fs::read(f.path("coordinator.md")).unwrap(), input);
}
