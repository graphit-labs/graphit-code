//! Readonly Live tools. Selectors and renderer declarations are never admissions.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]

mod ast;
pub(crate) mod stdio;
pub(crate) use crate::browser_auth::live_mcp::LiveMcpLaunch;
pub(crate) use ast::is_tool as is_ast_tool;

use crate::{
    foreground_knowledge,
    knowledge_live::{
        Access, ArtifactSelection, LiveError, LiveSessionService, ReadContexts,
        ReadContextsSnapshot, Session, SessionState,
    },
    live_agent_read::{self, Command, Output, ReadContext},
    reference_explorer::{self, Identity, Resources, SourceKey},
};
use graphit_hub::policy::RequestContext;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io;

pub(crate) const CONTEXT: &str = "graphit_live_context";
pub(crate) const HEADS: &str = "graphit_knowledge_heads";
pub(crate) const PAGE: &str = "graphit_knowledge_page";
pub(crate) const SEARCH: &str = "graphit_knowledge_search";

/// The Source resolver supplies this association from the selected installation.
/// Both fields are data; the caller retains its captures and every call admits anew.
pub(crate) struct SelectedContext<'a> {
    pub selection: &'a ArtifactSelection,
    pub source: &'a SourceKey,
}

pub(crate) struct CallContext<'a> {
    pub resources: &'a Resources<'a>,
    pub backend: &'a graphit_auth::broker::project_access::BrokerProjectAccess,
    pub identity: Identity<'a>,
    pub original: &'a RequestContext<'a>,
    pub access: &'a Access<'a>,
    pub live: &'a LiveSessionService,
    pub session: &'a Session,
    pub read_contexts: &'a ReadContexts,
    pub turn: &'a crate::knowledge_live::SendRequest,
    pub selected: &'a [SelectedContext<'a>],
    pub source: Option<&'a crate::source_server::SourceServer>,
    pub ast: &'a [crate::live_agent_ast::AstSelection<'a>],
    pub incoming: &'a tokio_util::sync::CancellationToken,
    pub model: Option<&'a std::sync::Arc<crate::foreground_memory_jobs::MemoryJobResources>>,
    pub allowed: &'a (dyn Fn() -> bool + Sync),
}

#[derive(Debug)]
pub(crate) enum Error {
    Read(reference_explorer::Failure),
    Knowledge(foreground_knowledge::Failure),
    Live(LiveError),
    Web(graphit_auth::broker::BrokerError),
    ContextChanged,
}
impl Error {
    pub(crate) fn response(&self) -> CallToolResult {
        let code = match self {
            Self::ContextChanged => "context_changed",
            Self::Read(error) => error.response().1,
            Self::Web(error) => match error {
                graphit_auth::broker::BrokerError::Cancelled => "cancelled",
                graphit_auth::broker::BrokerError::Deadline => "deadline",
                _ => "authentication_or_permission_denied",
            },
            Self::Knowledge(error) => match error {
                foreground_knowledge::Failure::Semantic(error) => {
                    foreground_knowledge::search::semantic_response(error).1
                }
                foreground_knowledge::Failure::Search(error) => {
                    foreground_knowledge::search::response(error).1
                }
                foreground_knowledge::Failure::Rerank(error) => error.code(),
                foreground_knowledge::Failure::Native(error) => {
                    reference_explorer::Failure::knowledge_response(error).1
                }
                foreground_knowledge::Failure::References(_) => "knowledge_reference_failure",
                foreground_knowledge::Failure::Encoding(_) => "response_encoding_failed",
            },
            Self::Live(error) => match error.kind() {
                io::ErrorKind::Interrupted => "cancelled",
                io::ErrorKind::TimedOut => "deadline",
                io::ErrorKind::PermissionDenied => "permission_denied",
                io::ErrorKind::NotFound => "not_found",
                _ => "live_context_unavailable",
            },
        };
        crate::mcp_output::typed_error(json!({"error":{"code":code}}))
    }
}
impl From<reference_explorer::Failure> for Error {
    fn from(error: reference_explorer::Failure) -> Self {
        Self::Read(error)
    }
}
impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Read(reference_explorer::Failure::Physical(error))
    }
}
impl From<graphit_auth::AuthError> for Error {
    fn from(error: graphit_auth::AuthError) -> Self {
        Self::Read(error.into())
    }
}
impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Read(error.into())
    }
}
fn invalid(code: &'static str) -> Error {
    reference_explorer::Failure::Invalid(code).into()
}

fn context_schema() -> Value {
    json!({"type":"object","properties":{
        "source":{"type":"string"},"project_id":{"type":["string","null"]},
        "instance_id":{"type":["string","null"]},"project_kind":{"type":["string","null"]},
        "type":{"type":"string"},"id":{"type":"string"},"version":{"type":"string"}
    },"required":["source","type","id","version"],"additionalProperties":false})
}
pub(crate) fn catalog() -> Vec<Tool> {
    let context = context_schema();
    [
        (CONTEXT, "Read the authorized Live project and selected contexts; no global catalogue.", json!({"type":"object","additionalProperties":false})),
        (HEADS, "Read Knowledge heads from the Live project or an exact selected context.", json!({"type":"object","properties":{"context":context},"additionalProperties":false})),
        (PAGE, "Read one Knowledge page with its real generation and reference binding.", json!({"type":"object","properties":{"context":context,"slug":{"type":"string"}},"required":["slug"],"additionalProperties":false})),
        (SEARCH, "Search the Live project or one exact selected Knowledge context using the existing fts/semantic/hybrid reader.", json!({"type":"object","properties":{"context":context,"search":{"type":"object","properties":{"query":{"type":"string"},"mode":{"enum":["fts","semantic","hybrid"]},"metric":{"enum":["cosine","l2","dot"],"default":"cosine"},"top_k":{"type":"integer","minimum":0,"default":0},"page_size":{"type":"integer","minimum":1,"maximum":100,"default":20},"preview":{"type":"boolean","default":false},"cursor":{"type":"object"}},"required":["query","mode"],"additionalProperties":false}},"required":["search"],"additionalProperties":false})),
    ].into_iter().map(|(name, description, schema)| Tool::new(name, description, schema.as_object().expect("object schema").clone())).collect()
}

/// Potential names are closed even before a particular run has selected AST.
pub(crate) fn potential_catalog() -> Vec<Tool> {
    let mut tools = catalog();
    let context = context_schema();
    let source = json!({"context":context,"path":{"type":"string"},
        "entity":{"type":"string"},"entity_type":{"type":"string"},"entity_key":crate::graph_server::GraphKey::schema(),
        "head":{"type":"integer"},"tail":{"type":"integer"},
        "start_line":{"type":"integer"},"end_line":{"type":"integer"},
        "pattern":{"type":"string"},"regex":{"type":"boolean","default":false},
        "before":{"type":"integer"},"after":{"type":"integer"},
        "line_numbers":{"type":"boolean","default":false}});
    for (name, description, properties, required) in [
        (
            ast::SOURCE,
            "Read Source only from an exact prepared AST context.",
            source,
            vec!["context", "path"],
        ),
        (
            ast::SCHEMA,
            "Read the schema of an exact prepared AST context.",
            json!({"context":context}),
            vec!["context"],
        ),
        (
            ast::QUERY,
            "Run existing readonly Cypher on an exact prepared AST context.",
            json!({"context":context,"query":{"type":"string"},"parameters":{"type":"object"}}),
            vec!["context", "query"],
        ),
        (
            ast::SEARCH,
            "Search an exact prepared AST context with existing lexical and CodeQuery readers.",
            json!({"context":context,"query":{"type":"string"},"mode":{"enum":["fts","semantic","vector","hybrid"],"default":"semantic"},"k":{"type":"integer","minimum":1},"metric":{"enum":["cosine","l2","dot"],"default":"cosine"}}),
            vec!["context", "query"],
        ),
    ] {
        let schema = json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
        tools.push(Tool::new(
            name,
            description,
            schema.as_object().expect("object schema").clone(),
        ));
    }
    tools
}

/// Accept only the exact four Knowledge names or those four plus all four AST names.
pub(crate) fn catalog_from_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Option<Vec<Tool>> {
    let mut actual: Vec<_> = names.into_iter().collect();
    actual.sort_unstable();
    for tools in [catalog(), potential_catalog()] {
        let mut expected: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        expected.sort_unstable();
        if actual == expected {
            return Some(tools);
        }
    }
    None
}

fn catalog_with_ast_contexts(contexts: &[ArtifactSelection]) -> Vec<Tool> {
    let mut tools = potential_catalog();
    let values = serde_json::to_value(contexts).expect("context metadata");
    for tool in &mut tools {
        if is_ast_tool(&tool.name) {
            std::sync::Arc::make_mut(&mut tool.input_schema)
                .get_mut("properties")
                .expect("canonical properties")
                .get_mut("context")
                .expect("canonical context")
                .as_object_mut()
                .expect("context schema")
                .insert("enum".into(), values.clone());
        }
    }
    tools
}

/// Normalize schemas, retaining only the same typed AST context enum on all four tools.
pub(crate) fn catalog_from_tools(tools: &[Tool]) -> Option<Vec<Tool>> {
    let canonical = catalog_from_names(tools.iter().map(|tool| tool.name.as_ref()))?;
    if canonical.len() == 4 {
        return Some(canonical);
    }
    let mut contexts: Option<Vec<ArtifactSelection>> = None;
    for tool in tools.iter().filter(|tool| is_ast_tool(&tool.name)) {
        let values = tool
            .input_schema
            .get("properties")?
            .get("context")?
            .get("enum")?;
        let selected: Vec<ArtifactSelection> = serde_json::from_value(values.clone()).ok()?;
        if selected.is_empty() {
            return None;
        }
        for (index, selection) in selected.iter().enumerate() {
            if selection.artifact_type != "ast"
                || selection.id.is_empty()
                || selection.version.is_empty()
                || !matches!(selection.source.as_str(), "project" | "hub")
                || selected[..index].contains(selection)
                || selection.source == "project"
                    && (selection.project_id.as_deref() != Some(selection.id.as_str())
                        || selection.instance_id.is_some()
                        || selection.project_kind.is_some())
            {
                return None;
            }
        }
        if contexts
            .as_ref()
            .is_some_and(|expected| expected != &selected)
        {
            return None;
        }
        contexts = Some(selected);
    }
    Some(catalog_with_ast_contexts(contexts.as_ref()?))
}

pub(crate) fn list(context: CallContext<'_>) -> Result<CallToolResult, Error> {
    let stored = context.check()?;
    let tools = if context.has_ast(&stored)? {
        let selected = context
            .ast
            .iter()
            .map(|entry| entry.selection.clone())
            .collect::<Vec<_>>();
        catalog_with_ast_contexts(&selected)
    } else {
        catalog()
    };
    let bytes = crate::task_server::browser::codec::encode_response(
        &json!({"tools":tools}),
        context.resources.policy.content.max_output_bytes,
    )?;
    Ok(crate::mcp_output::typed(serde_json::from_slice::<Value>(
        &bytes,
    )?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Heads {
    #[serde(default, deserialize_with = "reference_explorer::present")]
    context: Option<ArtifactSelection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default, deserialize_with = "reference_explorer::present")]
    context: Option<ArtifactSelection>,
    slug: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    #[serde(default, deserialize_with = "reference_explorer::present")]
    context: Option<ArtifactSelection>,
    search: foreground_knowledge::search::Input,
}

impl CallContext<'_> {
    fn check_io(&self) -> Result<(), Error> {
        if self.incoming.is_cancelled() {
            return Err(io::Error::from(io::ErrorKind::Interrupted).into());
        }
        self.resources.check(self.original)?;
        if !std::sync::Arc::ptr_eq(self.original.snapshot(), self.access.original().snapshot()) {
            return Err(invalid("different_live_original"));
        }
        self.access.read()?;
        let (caller, verifier, _) = self.identity;
        caller
            .check(
                verifier,
                graphit_auth::broker::BrokerRequestContext::new(self.original.snapshot()),
            )
            .map_err(Error::Web)?;
        if !(self.allowed)() {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied).into());
        }
        Ok(())
    }
    fn check(&self) -> Result<ReadContextsSnapshot, Error> {
        self.check_io()?;
        let stored = self
            .live
            .read_contexts(
                self.access,
                &self.session.id,
                &self.session.cursor.generation,
            )
            .map_err(Error::Live)?;
        let actual = &stored.session;
        if stored.last_prompt.as_deref() != Some(self.turn.turn_id.as_str())
            || actual.cursor.generation != self.session.cursor.generation
            || actual.state != SessionState::Running
            || actual.project != self.session.project
            || actual.binding != self.session.binding
            || actual.agent != self.session.agent
            || actual.artifacts != self.session.artifacts
            || actual.model != self.session.model
            || &stored.contexts != self.read_contexts
        {
            return Err(invalid("live_run_context_changed"));
        }
        Ok(stored)
    }
    fn has_ast(&self, stored: &ReadContextsSnapshot) -> Result<bool, Error> {
        if self.ast.is_empty() {
            return Ok(false);
        }
        if self.source.is_none() {
            return Err(invalid("selected_context_unavailable"));
        }
        let mut seen = Vec::with_capacity(self.ast.len());
        for entry in self.ast {
            let selection = entry.selection;
            if selection.artifact_type != "ast"
                || !self.session.artifacts.contains(selection)
                || seen.contains(&selection)
            {
                return Err(invalid("selected_context_changed"));
            }
            let mut stamps = stored
                .contexts
                .selected
                .iter()
                .filter(|v| &v.selection == selection);
            stamps.next().ok_or(Error::ContextChanged)?;
            if stamps.next().is_some() {
                return Err(Error::ContextChanged);
            }
            seen.push(selection);
        }
        if self
            .session
            .artifacts
            .iter()
            .filter(|v| v.artifact_type == "ast")
            .count()
            != seen.len()
        {
            return Err(Error::ContextChanged);
        }
        Ok(true)
    }
    fn selected_source(&self, wanted: &ArtifactSelection) -> Result<&SourceKey, Error> {
        if !self.session.artifacts.contains(wanted) {
            return Err(invalid("context_not_selected"));
        }
        let mut matches = self
            .selected
            .iter()
            .filter(|entry| entry.selection == wanted);
        let selected = matches
            .next()
            .ok_or_else(|| invalid("selected_context_unavailable"))?;
        if matches.next().is_some() {
            return Err(invalid("selected_context_ambiguous"));
        }
        if wanted.artifact_type != "knowledge" {
            return Err(invalid("selected_context_changed"));
        }
        match selected.source {
            SourceKey::ImportedKnowledge { artifact }
                if wanted.id == artifact.id && wanted.version == artifact.version => {}
            SourceKey::ProjectKnowledge
                if wanted.source == "project"
                    && wanted.project_id.as_deref() == Some(wanted.id.as_str())
                    && wanted.instance_id.is_none()
                    && wanted.project_kind.is_none()
                    && !wanted.version.is_empty() => {}
            SourceKey::ImportedKnowledge { .. } | SourceKey::ProjectKnowledge => {
                return Err(invalid("selected_context_changed"));
            }
            _ => return Err(invalid("unsupported_live_context")),
        }
        Ok(selected.source)
    }
}

pub(crate) async fn call(
    context: CallContext<'_>,
    call: CallToolRequestParams,
) -> Result<CallToolResult, Error> {
    let stored = context.check()?;
    let input = Value::Object(call.arguments.unwrap_or_default());
    if serde_json::to_vec(&input)?.len() > context.resources.policy.content.max_input_bytes {
        return Err(invalid("invalid_arguments"));
    }
    if is_ast_tool(call.name.as_ref()) {
        let value = Box::pin(ast::read(&context, &stored, call.name.as_ref(), input)).await?;
        let current = context.check()?;
        if current.contexts != stored.contexts {
            return Err(Error::ContextChanged);
        }
        let bytes = crate::task_server::browser::codec::encode_response(
            &value,
            context.resources.policy.content.max_output_bytes,
        )?;
        return Ok(crate::mcp_output::typed(serde_json::from_slice::<Value>(
            &bytes,
        )?));
    }
    let (wanted, command) = match call.name.as_ref() {
        CONTEXT => {
            let _: Empty =
                serde_json::from_value(input).map_err(|_| invalid("invalid_arguments"))?;
            (None, Command::Viewer)
        }
        HEADS => {
            let input: Heads =
                serde_json::from_value(input).map_err(|_| invalid("invalid_arguments"))?;
            (input.context, Command::Heads)
        }
        PAGE => {
            let input: Page =
                serde_json::from_value(input).map_err(|_| invalid("invalid_arguments"))?;
            if input.slug.len() > context.resources.policy.content.max_scalar_bytes {
                return Err(invalid("invalid_arguments"));
            }
            graphit_data::knowledge_index::validate_slug(&input.slug)
                .map_err(|_| invalid("invalid_arguments"))?;
            (input.context, Command::Page(input.slug))
        }
        SEARCH => {
            let input: Search =
                serde_json::from_value(input).map_err(|_| invalid("invalid_arguments"))?;
            input
                .search
                .validate(context.resources.policy.content)
                .map_err(invalid)?;
            (input.context, Command::Search(input.search))
        }
        _ => return Err(invalid("unknown_live_tool")),
    };
    let reader = ReadContext {
        resources: context.resources,
        backend: context.backend,
        identity: context.identity,
        original: context.original,
        access: context.access,
        live: context.live,
        stored: &stored,
        model: context.model,
    };
    let default = crate::live_agent_contexts::project_selection(&context.session.project);
    let bytes = if matches!(command, Command::Viewer) {
        let project = Box::pin(live_agent_read::read(&reader, &default, Command::Viewer)).await?;
        let Output::Bytes(project) = project else {
            return Err(invalid("invalid_live_read"));
        };
        let admitted_project: Value = serde_json::from_slice(&project)?;
        let mut contexts = Vec::with_capacity(context.selected.len());
        for selected in context.selected {
            let source = context.selected_source(selected.selection)?;
            Box::pin(live_agent_read::read(
                &reader,
                selected.selection,
                Command::Viewer,
            ))
            .await?;
            contexts.push(json!({"context":selected.selection,"source":source}));
        }
        crate::task_server::browser::codec::encode_response(
            &json!({"project":admitted_project,"contexts":contexts}),
            context.resources.policy.content.max_output_bytes,
        )?
    } else {
        let selection = wanted.as_ref().unwrap_or(&default);
        if selection != &default {
            context.selected_source(selection)?;
        }
        let output = Box::pin(live_agent_read::read(&reader, selection, command)).await?;
        let Output::Bytes(bytes) = output else {
            return Err(invalid("invalid_live_read"));
        };
        bytes
    };
    let current = context.check()?;
    if current.contexts != stored.contexts {
        return Err(Error::ContextChanged);
    }
    Ok(crate::mcp_output::typed(serde_json::from_slice::<Value>(
        &bytes,
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_catalog_requires_exact_known_sets_without_duplicate_or_mixed_names() {
        let baseline = catalog();
        let complete = potential_catalog();
        assert_eq!(
            catalog_from_names(baseline.iter().map(|t| t.name.as_ref()))
                .unwrap()
                .len(),
            4
        );
        assert_eq!(
            catalog_from_names(complete.iter().rev().map(|t| t.name.as_ref()))
                .unwrap()
                .len(),
            8
        );
        let mut names = baseline.iter().map(|t| t.name.as_ref()).collect::<Vec<_>>();
        names[3] = names[0];
        assert!(catalog_from_names(names).is_none());
        let mut names = complete.iter().map(|t| t.name.as_ref()).collect::<Vec<_>>();
        names[7] = names[0];
        assert!(catalog_from_names(names).is_none());
        assert!(catalog_from_names([CONTEXT, HEADS, PAGE, ast::SOURCE]).is_none());
        assert!(catalog_from_names([CONTEXT, HEADS, PAGE, "graphit_ast_write"]).is_none());
        assert!(catalog_from_names(complete.iter().take(7).map(|t| t.name.as_ref())).is_none());
    }
    #[test]
    fn live_catalog_preserves_only_consistent_prepared_ast_context_metadata() {
        let selected: ArtifactSelection = serde_json::from_value(json!({
            "source":"project","project_id":"fixture-project","type":"ast",
            "id":"fixture-project","version":"main"
        }))
        .unwrap();
        let listed = catalog_with_ast_contexts(std::slice::from_ref(&selected));
        let normalized = catalog_from_tools(&listed).unwrap();
        assert_eq!(normalized.len(), 8);
        let decorated = listed
            .iter()
            .cloned()
            .map(crate::mcp_output::decorate_tool)
            .collect::<Vec<_>>();
        let external = catalog_from_tools(&decorated)
            .unwrap()
            .into_iter()
            .map(crate::mcp_output::decorate_tool)
            .collect::<Vec<_>>();
        for tool in &external {
            assert_eq!(
                tool.input_schema["properties"]["ai_optimized"]["default"],
                true
            );
            assert!(tool.output_schema.is_none());
        }
        for tool in external.iter().filter(|tool| is_ast_tool(&tool.name)) {
            assert_eq!(
                tool.input_schema["properties"]["context"]["enum"],
                json!([selected])
            );
        }
        for tool in normalized.iter().filter(|tool| is_ast_tool(&tool.name)) {
            assert_eq!(
                tool.input_schema["properties"]["context"]["enum"],
                json!([selected])
            );
        }
        assert!(catalog_from_tools(&potential_catalog()).is_none());
        let mut inconsistent = listed.clone();
        let tool = inconsistent
            .iter_mut()
            .find(|tool| is_ast_tool(&tool.name))
            .unwrap();
        std::sync::Arc::make_mut(&mut tool.input_schema)
            .get_mut("properties")
            .unwrap()["context"]["enum"][0]["version"] = json!("other");
        assert!(catalog_from_tools(&inconsistent).is_none());
        assert!(
            catalog_from_tools(&catalog_with_ast_contexts(&[
                selected.clone(),
                selected.clone()
            ]))
            .is_none()
        );
        let mut wrong = selected;
        wrong.artifact_type = "knowledge".into();
        assert!(catalog_from_tools(&catalog_with_ast_contexts(&[wrong])).is_none());
        assert_eq!(catalog_from_tools(&catalog()).unwrap().len(), 4);
    }
}
