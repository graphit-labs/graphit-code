//! Public path fields are Unicode strings, independent of the service host OS.
use super::*;
#[test]
fn maintained_tool_path_schemas_advertise_unicode_strings() {
    for definition in [
        memory_search::definition(),
        knowledge_search::definition(),
        memory_index::definition(),
        semantic::definition(),
        knowledge_source::definition(),
        knowledge_log::definition(),
        knowledge_export::definition(),
        native_publication::definition(),
        file_publication::definition(),
        project_hook::definition(),
    ] {
        let schema = serde_json::to_value(&definition.input_schema).unwrap();
        assert_eq!(
            schema["properties"]["project_dir"]["type"], "string",
            "{}",
            definition.name
        );
        if let Some(output) = schema["properties"].get("output") {
            assert_eq!(output["type"], "string", "{} output", definition.name);
        }
    }
}
