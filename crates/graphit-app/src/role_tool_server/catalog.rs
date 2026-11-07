use graphit_auth::{Permission, VerifiedRoleGrant};
use graphit_core::{brand::Brand, module_settings::Module};
use rmcp::model::Tool;
use serde_json::{Value, json};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Operation {
    Skill,
    SessionHook,
    TaskSearch,
    TaskReferences,
    KnowledgeReferences,
    HubList,
    HubShow,
    HubContent,
    HubProjects,
    HubSearch,
    HubTypePath,
    MemoryGet,
    MemorySource,
    MemoryList,
    MemoryMandatory,
    MemoryImportant,
    MemoryHistory,
    MemorySearch,
    MemoryInsert,
    MemoryUpdate,
    MemoryDelete,
    MemoryMarkMandatory,
    MemoryUnmarkMandatory,
    MemoryPromote,
    MemoryDemote,
    KnowledgeList,
    KnowledgeSource,
    KnowledgeSchema,
    KnowledgeId,
    KnowledgeSearch,
    KnowledgeLog,
    Source,
    AstSchema,
    AstQuery,
    AstSearch,
    GitStatus,
    GitDiff,
}
impl Operation {
    pub fn hub(self) -> bool {
        matches!(
            self,
            Self::HubList
                | Self::HubShow
                | Self::HubContent
                | Self::HubProjects
                | Self::HubSearch
                | Self::HubTypePath
        )
    }
    pub fn references(self) -> bool {
        matches!(self, Self::TaskReferences | Self::KnowledgeReferences)
    }
    pub fn ast(self) -> bool {
        matches!(self, Self::AstSchema | Self::AstQuery | Self::AstSearch)
    }
    pub fn git(self) -> bool {
        matches!(self, Self::GitStatus | Self::GitDiff)
    }
    pub fn memory(self) -> bool {
        matches!(
            self,
            Self::MemoryGet
                | Self::MemorySource
                | Self::MemoryList
                | Self::MemoryMandatory
                | Self::MemoryImportant
                | Self::MemoryHistory
                | Self::MemorySearch
                | Self::MemoryInsert
                | Self::MemoryUpdate
                | Self::MemoryDelete
                | Self::MemoryMarkMandatory
                | Self::MemoryUnmarkMandatory
                | Self::MemoryPromote
                | Self::MemoryDemote
        )
    }
    pub fn knowledge(self) -> bool {
        matches!(
            self,
            Self::KnowledgeList
                | Self::KnowledgeSource
                | Self::KnowledgeSchema
                | Self::KnowledgeId
                | Self::KnowledgeSearch
                | Self::KnowledgeLog
        )
    }
    pub fn write(self) -> bool {
        matches!(
            self,
            Self::MemoryInsert
                | Self::MemoryUpdate
                | Self::MemoryDelete
                | Self::MemoryMarkMandatory
                | Self::MemoryUnmarkMandatory
                | Self::MemoryPromote
                | Self::MemoryDemote
        )
    }
    pub fn permission(self) -> Option<Permission> {
        if self.write() {
            Some(Permission::WriteMemory)
        } else if self.memory() {
            Some(Permission::Read(Module::Memory))
        } else if self.knowledge() {
            Some(Permission::Read(Module::Knowledge))
        } else {
            match self {
                Self::TaskSearch => Some(Permission::Read(Module::Task)),
                Self::TaskReferences => Some(Permission::Read(Module::Task)),
                Self::KnowledgeReferences => Some(Permission::Read(Module::Knowledge)),
                Self::HubList
                | Self::HubShow
                | Self::HubContent
                | Self::HubProjects
                | Self::HubSearch
                | Self::HubTypePath => Some(Permission::Read(Module::Hub)),
                Self::Source | Self::AstSchema | Self::AstQuery | Self::AstSearch => {
                    Some(Permission::Read(Module::Ast))
                }
                Self::GitStatus | Self::GitDiff => Some(Permission::InspectGit),
                _ => None,
            }
        }
    }
}
const OPERATIONS: &[(&str, Operation, &str)] = &[
    (
        "hub_type_path",
        Operation::HubTypePath,
        "Describe an artifact's PROJECT-relative destination, node kind and Agent schema identifier for the admitted native binding.",
    ),
    (
        "hub_projects",
        Operation::HubProjects,
        "Discover currently authorized remote Hub projects; discovery grants no artifact or PROJECT access.",
    ),
    (
        "hub_search",
        Operation::HubSearch,
        "Read current artifact manifests of an explicit remote producer using exact filters and independent origin/PROJECT authority; no payload reads.",
    ),
    (
        "hub_list",
        Operation::HubList,
        "List installed Hub metadata in the admitted PROJECT or installation; FILE entries require their current origin authority.",
    ),
    (
        "hub_show",
        Operation::HubShow,
        "Inspect one exact installed artifact version without hydrating its payload.",
    ),
    (
        "hub_content",
        Operation::HubContent,
        "Read one installed artifact text file through admitted PROJECT Core capture, or a currently authorized remote FILE cache; AST and Knowledge use their separate admitted readers.",
    ),
    (
        "wiki_log",
        Operation::KnowledgeLog,
        "Read committed document synchronization history in the admitted PROJECT; semantic-only transactions are excluded.",
    ),
    (
        "task_references",
        Operation::TaskReferences,
        "Read native Task/Session reference pages in the admitted source PROJECT; target identifiers grant no target access.",
    ),
    (
        "knowledge_references",
        Operation::KnowledgeReferences,
        "Read native Knowledge reference pages in the admitted source PROJECT; no foreign or USER corpus is opened.",
    ),
    (
        "session_hook",
        Operation::SessionHook,
        "Deliver mandatory PROJECT Memory for the authenticated Role at an agent hook boundary.",
    ),
    (
        "git_status",
        Operation::GitStatus,
        "Inspect readonly Git status for the authenticated physical PROJECT.",
    ),
    (
        "git_diff",
        Operation::GitDiff,
        "Inspect tracked worktree or staged Git changes; no commands or repository writes.",
    ),
    (
        "module_skill",
        Operation::Skill,
        "Render the selected module skill or exact companion with captured overrides.",
    ),
    (
        "task_search",
        Operation::TaskSearch,
        "Search admitted Task specifications and comments using fts (default), semantic or hybrid retrieval.",
    ),
    (
        "memory_get",
        Operation::MemoryGet,
        "Read a current PROJECT Memory.",
    ),
    (
        "memory_source",
        Operation::MemorySource,
        "Select the authored Memory body with original line coordinates.",
    ),
    (
        "memory_list",
        Operation::MemoryList,
        "List the complete current PROJECT Memory set with literal filters.",
    ),
    (
        "memory_mandatory",
        Operation::MemoryMandatory,
        "Read mandatory current PROJECT Memories.",
    ),
    (
        "memory_important",
        Operation::MemoryImportant,
        "Read important current PROJECT Memories.",
    ),
    (
        "memory_history",
        Operation::MemoryHistory,
        "Read the immutable Memory history.",
    ),
    (
        "memory_search",
        Operation::MemorySearch,
        "Search native Memory FTS; no semantic fallback.",
    ),
    (
        "memory_insert",
        Operation::MemoryInsert,
        "Create a PROJECT Memory with an actual write admission.",
    ),
    (
        "memory_update",
        Operation::MemoryUpdate,
        "Update the expected Memory revision.",
    ),
    (
        "memory_delete",
        Operation::MemoryDelete,
        "Delete the expected Memory revision.",
    ),
    (
        "memory_mark_mandatory",
        Operation::MemoryMarkMandatory,
        "Mark the expected Memory revision mandatory.",
    ),
    (
        "memory_unmark_mandatory",
        Operation::MemoryUnmarkMandatory,
        "Unmark the expected Memory revision mandatory.",
    ),
    (
        "memory_promote",
        Operation::MemoryPromote,
        "Promote the expected Memory revision.",
    ),
    (
        "memory_demote",
        Operation::MemoryDemote,
        "Demote the expected Memory revision.",
    ),
    (
        "knowledge_search",
        Operation::KnowledgeSearch,
        "Search admitted PROJECT Knowledge lexically on its captured SDK pin.",
    ),
    (
        "knowledge_list",
        Operation::KnowledgeList,
        "Read complete native Knowledge heads.",
    ),
    (
        "wiki_source",
        Operation::KnowledgeSource,
        "Read one native Knowledge page after EOF validation.",
    ),
    (
        "knowledge_id",
        Operation::KnowledgeId,
        crate::knowledge_id::DESCRIPTION,
    ),
    (
        "knowledge_schema",
        Operation::KnowledgeSchema,
        "Read the producer physical Knowledge schema under its actual admission.",
    ),
    (
        "ast_source",
        Operation::Source,
        "Inspect local indexed or physical Source through the Role composer.",
    ),
    (
        "ast_schema",
        Operation::AstSchema,
        "Read the admitted PROJECT graph schema from its retained publication.",
    ),
    (
        "ast_query",
        Operation::AstQuery,
        "Execute readonly Cypher on the admitted PROJECT graph with native typed results.",
    ),
    (
        "ast_search",
        Operation::AstSearch,
        "Search the admitted PROJECT entity index using FTS or the shared embedding service.",
    ),
];
const TASK_WRITE: &[&str] = &["add_task_comment", "progress_task"];
pub(super) fn task_operation(brand: &Brand, name: &str) -> Option<(&'static str, bool)> {
    crate::task_server::catalog::OPERATIONS
        .iter()
        .find_map(|(base, operation, write, _)| {
            // This Role operation keeps its existing direct native read result.
            if *operation == "search" {
                return None;
            }
            let suffix = base.strip_prefix("graphit_")?;
            if *write
                && (!cfg!(any(
                    target_os = "linux",
                    target_os = "windows",
                    target_os = "macos"
                )) || !TASK_WRITE.contains(operation))
            {
                return None;
            }
            (brand
                .mcp_tool_name(&suffix.split('_').collect::<Vec<_>>())
                .ok()?
                .as_str()
                == name)
                .then_some((*operation, *write))
        })
}
pub(super) fn operation(brand: &Brand, name: &str) -> Option<Operation> {
    OPERATIONS.iter().find_map(|(suffix, operation, _)| {
        (brand
            .mcp_tool_name(&suffix.split('_').collect::<Vec<_>>())
            .ok()?
            .as_str()
            == name)
            .then_some(*operation)
    })
}
pub(super) fn allowed(brand: &Brand, name: &str, grant: &VerifiedRoleGrant<'_>) -> bool {
    if let Some((_, write)) = task_operation(brand, name) {
        return grant
            .authorize(if write {
                Permission::WriteOwnTask
            } else {
                Permission::Read(Module::Task)
            })
            .is_ok();
    }
    match operation(brand, name) {
        Some(Operation::SessionHook) => grant.actor().is_ok(),
        Some(Operation::Skill) => [
            Module::Task,
            Module::Memory,
            Module::Knowledge,
            Module::Ast,
            Module::Hub,
        ]
        .iter()
        .any(|m| grant.authorize(Permission::Read(*m)).is_ok()),
        Some(op) => op
            .permission()
            .is_some_and(|permission| grant.authorize(permission).is_ok()),
        None => false,
    }
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn text() -> Value {
    json!({"type":"string"})
}
fn selector() -> Value {
    object(
        json!({"head":{"type":"integer","minimum":0},"tail":{"type":"integer","minimum":0},"start_line":{"type":"integer","minimum":0},"end_line":{"type":"integer","minimum":0},"pattern":text(),"is_regex":{"type":"boolean"},"before":{"type":"integer","minimum":0},"after":{"type":"integer","minimum":0},"line_numbers":{"type":"boolean"}}),
        &[],
    )
}
fn hub_kind() -> Value {
    json!({"enum":graphit_hub::ArtifactType::ALL.iter().map(|kind|kind.as_str()).collect::<Vec<_>>()})
}
pub(super) fn tools(brand: &Brand) -> Vec<Tool> {
    let mut tools = Vec::new();
    for tool in crate::task_server::catalog::tools() {
        let suffix = tool
            .name
            .strip_prefix("graphit_")
            .expect("native Task prefix");
        let name = brand
            .mcp_tool_name(&suffix.split('_').collect::<Vec<_>>())
            .expect("closed native tool name");
        if task_operation(brand, &name).is_some() {
            let mut tool = tool.clone();
            tool.name = name.into();
            tools.push(tool);
        }
    }
    for (suffix, operation, description) in OPERATIONS {
        if (operation.write()
            || operation.ast()
            || operation.references()
            || operation.hub()
            || *operation == Operation::KnowledgeLog)
            && !cfg!(any(
                target_os = "linux",
                target_os = "windows",
                target_os = "macos"
            ))
        {
            continue;
        }
        let input = match operation {
            Operation::SessionHook => object(
                json!({"engine":{"enum":["kiro"]},"event":{"enum":["SessionStart","UserPromptSubmit"]}}),
                &["engine", "event"],
            ),
            Operation::Skill => object(
                json!({"module":{"enum":["ast","hub","task","memory","knowledge"]},"reference":text(),"hub":{"type":"object","properties":{"id":text(),"version":text()},"required":["id","version"],"additionalProperties":false}}),
                &["module"],
            ),
            Operation::GitStatus => object(json!({}), &[]),
            Operation::GitDiff => object(
                json!({"comparison":{"enum":["worktree","staged"],"default":"worktree"}}),
                &[],
            ),
            Operation::Source => object(json!({"path":text(),"selectors":selector()}), &["path"]),
            Operation::HubTypePath => object(
                json!({"artifact_type":hub_kind(),"name":{"type":"string","minLength":1}}),
                &["artifact_type", "name"],
            ),
            Operation::HubList => object(
                json!({"scope":{"enum":["project","global_installation"]},"artifact_type":hub_kind(),"id":text(),"version":text(),"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1}}),
                &["scope", "limit"],
            ),
            Operation::HubShow => object(
                json!({"scope":{"enum":["project","global_installation"]},"artifact_type":hub_kind(),"id":text(),"version":text()}),
                &["scope", "artifact_type", "id", "version"],
            ),
            Operation::HubContent => object(
                json!({"scope":{"enum":["project","global_installation"]},"artifact_type":hub_kind(),"id":text(),"version":text(),"path":text()}),
                &["scope", "artifact_type", "id", "version", "path"],
            ),
            Operation::HubProjects => {
                let mut schema = object(
                    json!({"transport":{"enum":["broker","standalone"]},"provider":text(),"profile":text(),"identity_provider":text(),"id":text(),"name_prefix":text(),"limit":{"type":"integer","minimum":1,"maximum":1000},"cursor":text()}),
                    &["transport", "provider", "limit"],
                );
                schema["not"] = json!({"required":["id","name_prefix"]});
                schema
            }
            Operation::HubSearch => object(
                json!({"transport":{"enum":["broker","standalone"]},"provider":text(),"profile":text(),"identity_provider":text(),"producer":text(),"artifact_type":hub_kind(),"id":text(),"version":text(),"limit":{"type":"integer","minimum":1,"maximum":1000},"cursor":text()}),
                &["transport", "provider", "producer", "limit"],
            ),
            Operation::AstSchema => object(json!({}), &[]),
            Operation::AstQuery => object(
                json!({"query":text(),"parameters":{"type":"object"}}),
                &["query"],
            ),
            Operation::AstSearch => object(
                json!({"query":text(),"mode":{"enum":["fts","semantic","vector","hybrid"],"default":"semantic"},"k":{"type":"integer","minimum":1,"default":10},"metric":{"enum":["cosine","l2","dot"],"default":"cosine"},"rerank":{"type":"boolean","description":"Explicit override; omission uses search.rerank (default false)."}}),
                &["query"],
            ),
            Operation::KnowledgeSource => {
                object(json!({"slug":text(),"selectors":selector()}), &["slug"])
            }
            Operation::KnowledgeId => crate::knowledge_id::schema(),
            Operation::KnowledgeList | Operation::KnowledgeSchema => object(json!({}), &[]),
            Operation::KnowledgeLog => object(
                json!({"limit":{"type":"integer","minimum":1},"cursor":text()}),
                &["limit"],
            ),
            Operation::KnowledgeSearch => object(
                json!({"query":text(),"top_k":{"type":"integer","minimum":0,"default":0},"page_size":{"type":"integer","minimum":1,"maximum":100,"default":20},"preview":{"type":"boolean","default":false},"cursor":{"type":"object"},"rerank":{"type":"boolean","description":"Explicit override; omission uses search.rerank (default false)."}}),
                &["query"],
            ),
            Operation::MemoryGet | Operation::MemoryHistory => {
                object(json!({"id":text()}), &["id"])
            }
            Operation::MemorySource => object(json!({"id":text(),"selectors":selector()}), &["id"]),
            Operation::MemoryList | Operation::MemoryMandatory | Operation::MemoryImportant => {
                memory::filter_schema()
            }
            Operation::MemorySearch => object(
                json!({"text":text(),"filter":memory::filter_schema(),"include_deleted_history":{"type":"boolean"},"top_k":{"type":"integer","minimum":1}}),
                &["text", "top_k"],
            ),
            Operation::TaskSearch => crate::task_server::catalog::search_input(),
            Operation::TaskReferences => object(
                json!({
                    "focus":{"type":"object","properties":{"task":text(),"session":text()},"oneOf":[{"required":["task"]},{"required":["session"]}],"additionalProperties":false},
                    "page_size":{"type":"integer","minimum":1,"maximum":100},
                    "outgoing_cursor":{"type":["object","null"]},
                    "incoming_cursor":{"type":["object","null"]}
                }),
                &["focus", "page_size"],
            ),
            Operation::KnowledgeReferences => object(
                json!({
                    "outgoing":{"type":["string","null"]},
                    "incoming":{"type":["object","null"],"properties":{
                        "target_kind":{"enum":["task","session","memory","knowledge"]},
                        "scope":{"type":"object","properties":{"project":text(),"user":text()},"oneOf":[{"required":["project"]},{"required":["user"]}],"additionalProperties":false},
                        "id":text(),"context":{"type":["string","null"]}
                    },"required":["target_kind","scope","id"],"additionalProperties":false},
                    "page_size":{"type":"integer","minimum":1,"maximum":100},
                    "outgoing_cursor":{"type":["object","null"]},
                    "incoming_cursor":{"type":["object","null"]}
                }),
                &["page_size"],
            ),
            _ => memory::write_schema(*operation),
        };
        let schema = object(
            json!({"selection":crate::task_server::catalog::selection(),"input":input}),
            &["input"],
        );
        let tool = Tool::new(
            brand
                .mcp_tool_name(&suffix.split('_').collect::<Vec<_>>())
                .expect("closed tool name"),
            *description,
            schema.as_object().expect("object").clone(),
        );
        tools.push(if *operation == Operation::KnowledgeId {
            tool.with_annotations(rmcp::model::ToolAnnotations::new().read_only(true))
        } else {
            tool
        });
    }
    tools
}
use super::memory;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_search_keeps_one_role_tool_and_native_closed_query() {
        let brand = Brand::default();
        let name = brand.mcp_tool_name(&["task", "search"]).unwrap();
        let tools = tools(&brand);
        let matching: Vec<_> = tools.iter().filter(|tool| tool.name == name).collect();
        assert_eq!(matching.len(), 1);
        assert!(task_operation(&brand, &name).is_none());
        let schema = Value::Object(matching[0].input_schema.as_ref().clone());
        assert_eq!(
            schema["properties"]["input"],
            crate::task_server::catalog::search_input()
        );
        assert!(
            schema["properties"]["input"]["properties"]["filter"]["properties"]
                .get("query")
                .is_none()
        );
        let input = json!({"text":"needle","filter":{},"page_size":1,"top_k":10,"cursor":null});
        serde_json::from_value::<graphit_records::task_store::TaskLexicalQuery>(input).unwrap();
    }
}
