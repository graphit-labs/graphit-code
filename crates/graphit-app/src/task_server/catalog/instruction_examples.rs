use super::{OPERATIONS, tools};
use graphit_core::{
    file_io::SnapshotContext, file_lock::Cancellation, module_settings::Module,
    native_path::NativePath,
};
use graphit_hub::native::{
    ContentLimits, NativeBinding, RenderContext, SkillDescriptor, render_skill,
};
use graphit_records::task_protocol::WriteRequest;
use graphit_runtime::{
    instructions::{InstructionLimits, InstructionValue, InstructionVariable, render_instruction},
    skill_instructions::skill_templates,
};
use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};
use std::{
    num::{NonZeroU64, NonZeroUsize},
    time::{Duration, Instant},
};

fn accepts_top(schema: &Value, arguments: &Value) -> bool {
    let object = arguments.as_object().unwrap();
    let properties = schema["properties"].as_object().unwrap();
    object.keys().all(|key| properties.contains_key(key))
        && schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .all(|key| object.contains_key(key.as_str().unwrap()))
}

#[test]
fn compiled_task_examples_follow_advertised_native_contract_and_hub_rendering() {
    let fixtures: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../graphit-runtime/tests/fixtures/skill-instructions/variables.json"
    )))
    .unwrap();
    let set = skill_templates(Module::Task).unwrap();
    let cancellation = Cancellation::default();
    let original = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    );
    let mut validated = 0;
    for brand in ["graphit", "orbit"] {
        let values = fixtures[brand].as_object().unwrap();
        let variables: Vec<_> = values
            .iter()
            .map(|(name, value)| InstructionVariable {
                name,
                value: InstructionValue::Text(value.as_str().unwrap()),
            })
            .collect();
        for entry in
            std::iter::once(set.body_entry()).chain(set.references().iter().map(|r| r.entry))
        {
            let body = render_instruction(
                entry,
                set.candidates(),
                &variables,
                InstructionLimits {
                    max_templates: NonZeroUsize::new(32).unwrap(),
                    max_template_bytes: NonZeroUsize::new(262144).unwrap(),
                    max_values: NonZeroUsize::new(256).unwrap(),
                    max_variable_bytes: NonZeroUsize::new(65536).unwrap(),
                    max_output_bytes: NonZeroUsize::new(65536).unwrap(),
                    fuel: NonZeroU64::new(100000).unwrap(),
                    recursion: NonZeroUsize::new(32).unwrap(),
                },
                &original,
            )
            .unwrap()
            .into_body();
            for code in body.split("~~~json\n").skip(1) {
                let code = code.split_once("\n~~~").expect("closed native example").0;
                let call: CallToolRequestParams = serde_json::from_str(code).unwrap();
                let (name, operation, write, _) = OPERATIONS
                    .iter()
                    .find(|(name, _, _, _)| {
                        let variable =
                            format!("mcp_tool_{}", name.strip_prefix("graphit_").unwrap());
                        values.get(&variable).and_then(Value::as_str) == Some(call.name.as_ref())
                    })
                    .expect("example calls an advertised native operation");
                assert!(write);
                let tool = tools().iter().find(|tool| tool.name == *name).unwrap();
                let schema = Value::Object(tool.input_schema.as_ref().clone());
                let arguments = Value::Object(call.arguments.unwrap());
                assert!(accepts_top(&schema, &arguments));
                assert!(accepts_top(
                    &schema["properties"]["input"],
                    &arguments["input"]
                ));
                serde_json::from_value::<graphit_core::project_identity::ProjectId>(
                    arguments["selection"]["project_id"].clone(),
                )
                .unwrap();
                let request = json!({"operation":operation,"input":arguments["input"]});
                serde_json::from_value::<WriteRequest>(request.clone()).unwrap();
                let mut spoofed = request.clone();
                spoofed["input"]["agent_id"] = json!("invented-actor");
                assert!(serde_json::from_value::<WriteRequest>(spoofed).is_err());
                let mut old_format = arguments.clone();
                old_format["ai_optimized"] = json!(true);
                assert!(!accepts_top(&schema, &old_format));
                assert!(!accepts_top(
                    &schema,
                    &json!({"project_dir":"/example","title":"flat"})
                ));
                let native =
                    NativePath::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))).unwrap();
                let encoded = serde_json::to_value(&native).unwrap();
                assert_eq!(
                    schema["properties"]["selection"]["properties"]["project_dir"]["anyOf"][0]["type"],
                    "string"
                );
                assert!(encoded.is_string());
                assert_eq!(
                    serde_json::from_value::<NativePath>(encoded).unwrap(),
                    native
                );
                assert!(
                    serde_json::from_value::<NativePath>(json!({"os":"Unix","units":[47,112]}))
                        .is_err()
                );
                validated += 1;
            }
            if entry == set.body_entry() {
                let mut context = RenderContext::new(
                    ContentLimits {
                        max_input_bytes: 524288,
                        max_output_bytes: 524288,
                        max_depth: 16,
                        max_nodes: 2048,
                        max_scalar_bytes: 65536,
                        max_collection_entries: 64,
                    },
                    &cancellation,
                    Instant::now() + Duration::from_secs(30),
                )
                .unwrap();
                let input = format!(
                    "---\nname: native-task\ndescription: {}\n---\n{body}",
                    serde_json::to_string(set.description()).unwrap()
                );
                let descriptor = SkillDescriptor::parse(input.as_bytes(), &mut context).unwrap();
                let binding = NativeBinding::KiroV3;
                let output = render_skill(binding, &descriptor, None, &mut context).unwrap();
                let parsed = SkillDescriptor::parse(&output.bytes, &mut context).unwrap();
                assert_eq!(parsed.body(), body);
                assert_eq!(output.provenance.binding, binding);
            }
        }
    }
    assert_eq!(
        validated, 8,
        "four real native examples across two captured namespaces"
    );
}
