//! MCP-only presentation. Internal calls retain their typed result until delivery.
use graphit_core::structured_output::{self, Limits, OutputFormat};
use rmcp::{
    ErrorData,
    model::{CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Tool},
};
use serde_json::{Value, json};

pub(crate) fn select_format(call: &mut CallToolRequestParams) -> Result<OutputFormat, ErrorData> {
    match call
        .arguments
        .as_mut()
        .and_then(|args| args.remove("ai_optimized"))
    {
        None | Some(Value::Bool(true)) => Ok(OutputFormat::Toon),
        Some(Value::Bool(false)) => Ok(OutputFormat::Json),
        Some(_) => Err(ErrorData::invalid_params(
            "ai_optimized must be a boolean",
            None,
        )),
    }
}

pub(crate) fn json_call(call: &mut CallToolRequestParams) {
    call.arguments
        .get_or_insert_with(Default::default)
        .insert("ai_optimized".into(), Value::Bool(false));
}

pub(crate) fn decorate_tool(mut tool: Tool) -> Tool {
    let schema = std::sync::Arc::make_mut(&mut tool.input_schema);
    schema
        .entry("properties")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("tool input properties are an object")
        .insert(
            "ai_optimized".into(),
            json!({
                "type":"boolean", "default":true,
                "description":"Omitted or true: TOON text without duplicate JSON. False: JSON."
            }),
        );
    // A single tool can return TOON text or JSON, so it cannot promise a JSON output schema.
    tool.output_schema = None;
    tool
}

pub(crate) fn typed(value: Value) -> CallToolResult {
    let mut result = CallToolResult::success(Vec::new());
    result.structured_content = Some(value);
    result
}

pub(crate) fn typed_error(value: Value) -> CallToolResult {
    let mut result = typed(value);
    result.is_error = Some(true);
    result
}

pub(crate) fn format_result(
    mut result: CallToolResult,
    format: OutputFormat,
    max_bytes: usize,
) -> Result<CallToolResult, ErrorData> {
    let Some(value) = result.structured_content.as_ref() else {
        return Ok(result);
    };
    let document = match structured_output::encode(
        value,
        format,
        Limits {
            max_depth: 64,
            max_nodes: max_bytes,
            max_input_bytes: max_bytes,
            max_output_bytes: max_bytes,
        },
    ) {
        Ok(document) => document,
        Err(structured_output::OutputError::InvalidLimits) => {
            return Err(ErrorData::internal_error("Invalid MCP output policy", None));
        }
        Err(error) => {
            // Formatting happens after execution, including confirmed writes. A
            // delivery failure is a tool error, not a failed protocol request or
            // proof that the operation was rolled back. Never retry in another format.
            let outcome = if result.is_error == Some(true) {
                "tool_error"
            } else {
                "completed"
            };
            let reason = match error {
                structured_output::OutputError::InputBytes
                | structured_output::OutputError::OutputBytes => "response_limit",
                _ => "response_encoding_failed",
            };
            let diagnostic = format!("{outcome}; {reason}");
            if diagnostic.len() > max_bytes {
                return Err(ErrorData::internal_error(
                    "MCP output policy cannot fit the delivery diagnostic after tool execution",
                    Some(
                        json!({"tool_completed":true,"tool_reported_error":result.is_error == Some(true),"reason":reason}),
                    ),
                ));
            }
            result.content = vec![ContentBlock::text(diagnostic)];
            result.structured_content = None;
            result.is_error = Some(true);
            return Ok(result);
        }
    };
    result.content = vec![ContentBlock::text(document.into_string())];
    if format == OutputFormat::Toon {
        result.structured_content = None;
    }
    Ok(result)
}

pub(crate) fn format_response(
    response: CallToolResponse,
    format: OutputFormat,
    max_bytes: usize,
) -> Result<CallToolResponse, ErrorData> {
    match response {
        CallToolResponse::Complete(result) => {
            format_result(result, format, max_bytes).map(Into::into)
        }
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_selection_and_no_duplicate() {
        for flag in [
            None,
            Some(json!(true)),
            Some(json!(false)),
            Some(json!(null)),
            Some(json!("true")),
            Some(json!(1)),
        ] {
            let mut call = CallToolRequestParams::new("example");
            if let Some(flag) = flag.clone() {
                call.arguments = Some(serde_json::Map::from_iter([("ai_optimized".into(), flag)]));
            }
            let selected = select_format(&mut call);
            match flag {
                None | Some(Value::Bool(true)) => {
                    assert_eq!(selected.as_ref().unwrap(), &OutputFormat::Toon)
                }
                Some(Value::Bool(false)) => {
                    assert_eq!(selected.as_ref().unwrap(), &OutputFormat::Json)
                }
                _ => {
                    assert!(selected.is_err());
                    continue;
                }
            }
            let value = json!({"rows":[{"id":"á\\\"\n", "v":null},{"id":"二", "v":-0.0}], "cursor":"next", "nested":{"max":u64::MAX}});
            let format = selected.unwrap();
            let result = format_result(typed(value.clone()), format, 65536).unwrap();
            assert_eq!(result.content.len(), 1);
            if format == OutputFormat::Toon {
                assert!(result.structured_content.is_none());
            } else {
                assert_eq!(result.structured_content, Some(value));
            }
        }
    }
    #[test]
    fn text_and_error_are_preserved() {
        for format in [OutputFormat::Json, OutputFormat::Toon] {
            let raw =
                CallToolResult::success(vec![ContentBlock::text("fn á() {\n  // original\n}\n")]);
            assert_eq!(format_result(raw.clone(), format, 65536).unwrap(), raw);
            let error = format_result(
                typed_error(json!({"error":{"code":"denied"}})),
                format,
                65536,
            )
            .unwrap();
            assert_eq!(error.is_error, Some(true));
        }
    }

    #[test]
    fn formatting_limit_is_a_delivery_error_after_execution() {
        let value = json!({"rows":[[1,2],[3,4],[5,6]]});
        let bound = serde_json::to_vec(&value).unwrap().len();
        let result = format_result(typed(value), OutputFormat::Toon, bound).unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.structured_content.is_none());
        let text = &result.content[0].as_text().unwrap().text;
        assert_eq!(text, "completed; response_limit");
        assert!(text.len() <= bound);
    }
}
