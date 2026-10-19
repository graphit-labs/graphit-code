//! Small JSON boundary shared by the active Memory and Knowledge browser readers.
use super::super::request::{Arguments, Selection};
use graphit_core::{native_path::NativePath, project_identity::ProjectId};
use graphit_hub::native::ContentLimits;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExactSelection {
    pub(crate) project_dir: NativePath,
    pub(crate) project_id: ProjectId,
    pub(crate) project_version: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Body<T> {
    pub(crate) selection: ExactSelection,
    pub(crate) input: T,
}
pub(crate) fn decode<T: DeserializeOwned>(
    bytes: &[u8],
    limits: ContentLimits,
) -> Result<T, &'static str> {
    if bytes.len() > limits.max_input_bytes {
        return Err("invalid_arguments");
    }
    serde_json::from_slice(bytes).map_err(|_| "invalid_arguments")
}
pub(crate) fn parse_body<T: DeserializeOwned>(
    bytes: &[u8],
    limits: ContentLimits,
) -> Result<Body<T>, &'static str> {
    let body: Body<T> = decode(bytes, limits)?;
    let path_bytes = body
        .selection
        .project_dir
        .as_path()
        .to_str()
        .ok_or("invalid_arguments")?
        .len();
    // Native ProjectId and MemoryId accept only canonical 26-character ULIDs.
    if limits.max_scalar_bytes < 26
        || body.selection.project_version.len() > limits.max_scalar_bytes
        || path_bytes > limits.max_scalar_bytes
    {
        return Err("invalid_arguments");
    }
    Ok(body)
}
pub(crate) fn arguments<T: Serialize>(
    selection: ExactSelection,
    query: T,
) -> Result<Arguments, &'static str> {
    if selection.project_version.is_empty() || selection.project_version.contains('\0') {
        return Err("invalid_selection");
    }
    Ok(Arguments {
        selection: Selection {
            project_dir: Some(selection.project_dir),
            project_id: Some(selection.project_id),
            project_version: Some(selection.project_version),
        },
        input: Some(serde_json::to_value(query).map_err(|_| "invalid_arguments")?),
    })
}
#[derive(Serialize)]
pub(crate) struct AdmittedSelection {
    pub(crate) project_dir: NativePath,
    pub(crate) project_id: ProjectId,
    pub(crate) project_version: String,
}
#[derive(Serialize)]
pub(crate) struct Response<'a, T> {
    pub(crate) selection: &'a AdmittedSelection,
    pub(crate) native: &'a T,
}
pub(crate) fn encode<T: Serialize>(
    selection: &AdmittedSelection,
    native: &T,
    maximum: usize,
) -> Result<Vec<u8>, serde_json::Error> {
    encode_response(&Response { selection, native }, maximum)
}
pub(crate) fn encode_response<T: Serialize>(
    response: &T,
    maximum: usize,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut output = Output {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut output, response)?;
    Ok(output.bytes)
}
/// Validate the concrete JSON response bound without retaining a serialized copy.
pub(crate) fn check_response<T: Serialize>(
    response: &T,
    maximum: usize,
) -> Result<(), serde_json::Error> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.checked_sub(bytes.len()).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::FileTooLarge, "Browser response limit")
            })?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Counter(maximum), response)
}
struct Output {
    bytes: Vec<u8>,
    maximum: usize,
}
impl std::io::Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.maximum - self.bytes.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::FileTooLarge,
                "Browser response limit",
            ));
        }
        self.bytes.try_reserve(bytes.len()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "Browser response allocation",
            )
        })?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    fn limits() -> ContentLimits {
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 1,
            max_nodes: 1,
            max_scalar_bytes: 4096,
            max_collection_entries: 1024,
        }
    }
    fn selection() -> Value {
        json!({"project_dir":NativePath::new(std::env::temp_dir()).unwrap(),"project_id":ID,"project_version":"main"})
    }
    fn request(input: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({"selection":selection(),"input":input})).unwrap()
    }
    fn parse(op: &str, bytes: &[u8], limits: ContentLimits) -> Result<Arguments, &'static str> {
        if op.starts_with("memory_") {
            crate::foreground_memory::parse(bytes, limits, op)
        } else {
            crate::foreground_knowledge::parse(bytes, limits, op)
        }
    }
    #[test]
    fn all_closed_operations_accept_native_selection_without_json_node_accounting() {
        for (op, input) in [
            (
                "memory_list",
                json!({"tag":null,"important":true,"classification":"fact"}),
            ),
            ("memory_get", json!({"id":ID})),
            ("memory_history", json!({"id":ID})),
            ("knowledge_heads", json!({})),
            ("knowledge_page", json!({"slug":"literal name"})),
        ] {
            let args = parse(op, &request(input), limits()).unwrap();
            assert_eq!(
                serde_json::to_value(args.selection.project_dir.unwrap()).unwrap(),
                selection()["project_dir"]
            );
            assert_eq!(args.selection.project_version.as_deref(), Some("main"));
            assert_eq!(args.selection.project_id.unwrap().to_string(), ID);
        }
    }
    #[test]
    fn unknown_escaped_duplicates_null_types_and_trailing_data_remain_invalid() {
        let selected = selection();
        for (op, input) in [
            ("memory_list", json!({"unknown":true})),
            ("memory_list", json!({"classification":12})),
            ("memory_list", json!({"exclude_mandatory":null})),
            ("memory_list", json!({"important":"true"})),
            ("memory_get", json!({"id":null})),
            ("memory_history", json!({"id":false})),
            ("knowledge_heads", json!({"unknown":null})),
            ("knowledge_page", json!({"slug":{}})),
        ] {
            assert!(parse(op, &request(input), limits()).is_err());
        }
        for (op, input) in [
            ("memory_list", r#"{"tag":null,"\u0074ag":"literal"}"#),
            (
                "memory_get",
                r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","\u0069d":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}"#,
            ),
            ("knowledge_page", r#"{"slug":"first","\u0073lug":"second"}"#),
        ] {
            let bytes = format!(r#"{{"selection":{selected},"input":{input}}}"#);
            assert!(parse(op, bytes.as_bytes(), limits()).is_err());
        }
        for bytes in [
            format!(r#"{{"selection":{selected},"\u0073election":{selected},"input":{{}}}}"#),
            format!(r#"{{"selection":{selected},"input":{{}},"unknown":{{}}}}"#),
            format!(r#"{{"selection":{selected},"input":{{}}}} false"#),
            String::from(r#"{"selection":null,"input":{}}"#),
        ] {
            assert!(parse("memory_list", bytes.as_bytes(), limits()).is_err());
        }
    }
    #[test]
    fn decoded_utf8_version_tag_and_slug_keep_their_actual_scalar_caps() {
        let mut cap = limits();
        cap.max_scalar_bytes = 26;
        for (op, field) in [("memory_list", "tag"), ("knowledge_page", "slug")] {
            for (count, accepted) in [(13, true), (14, false)] {
                let escaped = "\\u00e9".repeat(count);
                let bytes = format!(
                    r#"{{"selection":{},"input":{{"{field}":"{escaped}"}}}}"#,
                    selection()
                );
                assert_eq!(parse(op, bytes.as_bytes(), cap).is_ok(), accepted);
            }
        }
        for (count, accepted) in [(13, true), (14, false)] {
            let selected = selection()
                .to_string()
                .replace("main", &"\\u00e9".repeat(count));
            let bytes = format!(r#"{{"selection":{selected},"input":{{}}}}"#);
            assert_eq!(
                parse("knowledge_heads", bytes.as_bytes(), cap).is_ok(),
                accepted
            );
        }
        cap.max_scalar_bytes = 25;
        assert!(parse("memory_list", &request(json!({})), cap).is_err());
    }
    #[test]
    fn frame_and_unicode_path_scalar_bounds_accept_exact_boundary_and_reject_extra() {
        let bytes = request(json!({}));
        let mut cap = limits();
        cap.max_input_bytes = bytes.len();
        assert!(parse("memory_list", &bytes, cap).is_ok());
        cap.max_input_bytes -= 1;
        assert!(parse("memory_list", &bytes, cap).is_err());
        let mut selected = selection();
        selected["project_dir"] = json!(format!("/projetos espaço/café Ω/{}", "x".repeat(64)));
        let count = selected["project_dir"].as_str().unwrap().len();
        let bytes = serde_json::to_vec(&json!({"selection":selected,"input":{}})).unwrap();
        cap = limits();
        cap.max_scalar_bytes = count;
        assert!(parse("knowledge_heads", &bytes, cap).is_ok());
        cap.max_scalar_bytes -= 1;
        assert!(parse("knowledge_heads", &bytes, cap).is_err());
        for project_id in [
            json!(null),
            json!(7),
            json!("00000000000000000000000000"),
            json!("01arz3ndektsv4rrffq69g5fav"),
        ] {
            let mut invalid = selected.clone();
            invalid["project_id"] = project_id;
            let bytes = serde_json::to_vec(&json!({"selection":invalid,"input":{}})).unwrap();
            assert!(parse("knowledge_heads", &bytes, limits()).is_err());
        }
        for version in ["", "ma\0in"] {
            let mut invalid = selected.clone();
            invalid["project_version"] = json!(version);
            let bytes = serde_json::to_vec(&json!({"selection":invalid,"input":{}})).unwrap();
            assert!(parse("memory_list", &bytes, limits()).is_err());
        }
    }
    #[test]
    fn unicode_paths_preserve_spaces_accents_escapes_and_literal_separators() {
        for relative in [
            "café Ω//literal/",
            r"projeto com espaço\ação Ω",
            "quote\"and\\slash",
        ] {
            let path = std::env::temp_dir().join(relative);
            let path = path.to_str().unwrap();
            let selected = json!({"project_dir":path,"project_id":ID,"project_version":"main"});
            let bytes = serde_json::to_vec(&json!({"selection":selected,"input":{}})).unwrap();
            let args = parse("knowledge_heads", &bytes, limits()).unwrap();
            assert_eq!(
                args.selection.project_dir.unwrap().as_path().as_os_str(),
                path
            );
        }
        let invalid = format!(
            r#"{{"selection":{{"project_dir":"\ud800","project_id":"{ID}","project_version":"main"}},"input":{{}}}}"#
        );
        assert!(parse("knowledge_heads", invalid.as_bytes(), limits()).is_err());
        let legacy = json!({"selection":{"project_dir":{"os":"Unix","units":[47,112]},"project_id":ID,"project_version":"main"},"input":{}});
        assert!(
            parse(
                "knowledge_heads",
                &serde_json::to_vec(&legacy).unwrap(),
                limits()
            )
            .is_err()
        );
    }
}
