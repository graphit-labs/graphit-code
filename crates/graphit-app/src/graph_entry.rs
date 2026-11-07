//! Graph CLI operations through the authenticated Source MCP connection.
use clap::{Args, Subcommand, ValueEnum};
use graphit_core::{
    native_document_file::NativeDocumentFileLimits, native_path::NativePath,
    project_paths::InstallationPaths,
};
use graphit_hub::policy::RequestContext;
use rmcp::model::CallToolRequestParams;
use serde_json::{Map, Value, json};
use std::{io, path::PathBuf};

#[derive(Args, Default)]
pub struct Selection {
    #[arg(long, conflicts_with = "project_dir")]
    pub context: Option<String>,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    #[arg(skip)]
    pub project_version: Option<String>,
}

#[derive(Args)]
pub struct Query {
    #[command(flatten)]
    pub selection: Selection,
    pub query: String,
    /// Named typed Cypher value; repeat for each name (VALUE is a JSON value).
    #[arg(long = "param", value_name = "NAME=VALUE", value_parser = parse_parameter)]
    pub parameters: Vec<(String, Value)>,
    /// Named literal Cypher string; repeat for each name, without JSON quoting.
    #[arg(long = "param-string", value_name = "NAME=TEXT", value_parser = parse_string_parameter)]
    pub string_parameters: Vec<(String, Value)>,
}

fn parameter_parts(text: &str) -> Result<(&str, &str), String> {
    let (name, value) = text.split_once('=').ok_or("expected NAME=VALUE")?;
    if name.is_empty() || name.trim() != name || name.chars().any(char::is_control) {
        return Err(
            "parameter name must be nonempty and contain no whitespace or control characters"
                .into(),
        );
    }
    Ok((name, value))
}
fn parse_parameter(text: &str) -> Result<(String, Value), String> {
    let (name, value) = parameter_parts(text)?;
    Ok((
        name.into(),
        serde_json::from_str(value).map_err(|e| e.to_string())?,
    ))
}
fn parse_string_parameter(text: &str) -> Result<(String, Value), String> {
    let (name, value) = parameter_parts(text)?;
    Ok((name.into(), Value::String(value.into())))
}

#[derive(Args)]
pub struct Search {
    #[command(flatten)]
    pub selection: Selection,
    pub query: String,
    #[arg(long, default_value = "semantic", value_parser = ["fts", "hybrid", "semantic", "vector"])]
    pub mode: String,
    #[arg(long)]
    pub k: Option<usize>,
    #[arg(long, default_value = "cosine", value_parser = ["l2", "cosine", "dot"])]
    pub metric: String,
    /// Override search.rerank for this query; omitted uses configuration (default false).
    #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true")]
    pub rerank: Option<bool>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum KeyType {
    String,
    Int64,
}
#[derive(Clone, Copy, ValueEnum)]
pub enum Direction {
    Outgoing,
    Incoming,
}

#[derive(Args)]
pub struct Neighborhood {
    #[command(flatten)]
    pub selection: Selection,
    #[arg(long)]
    pub anchor_label: String,
    #[arg(long)]
    pub anchor_key: String,
    #[arg(long, value_enum, default_value = "string")]
    pub key_type: KeyType,
    #[arg(long)]
    pub relationship: Option<String>,
    #[arg(long)]
    pub other_label: Option<String>,
    #[arg(long, value_enum)]
    pub direction: Option<Direction>,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    /// Exact opaque JSON cursor returned by the preceding neighborhood page.
    #[arg(long, conflicts_with = "cursor_file", value_parser = crate::cli_cursor::parse)]
    pub cursor: Option<Value>,
    /// Read that unchanged cursor object from a UTF-8 JSON file.
    #[arg(long, conflicts_with = "cursor")]
    pub cursor_file: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Command {
    /// List exact installed Native AST contexts.
    Imports,
    /// Search the current PROJECT or an exact installed AST context.
    Search(Search),
    /// Query the current PROJECT or an explicitly selected installed graph.
    Query(Query),
    /// Show graph labels, columns and relationships.
    Schema(Selection),
    /// Show a bounded sample of graph nodes and edges.
    Sample(Selection),
    /// Page over the relationships of one node.
    Neighborhood(Neighborhood),
}

impl Command {
    pub fn selection_mut(&mut self) -> Option<&mut Selection> {
        match self {
            Self::Imports => None,
            Self::Query(a) => Some(&mut a.selection),
            Self::Search(a) => Some(&mut a.selection),
            Self::Schema(a) | Self::Sample(a) => Some(a),
            Self::Neighborhood(a) => Some(&mut a.selection),
        }
    }
    fn wire(&self, max_input_bytes: usize) -> io::Result<(&'static str, Map<String, Value>)> {
        let (name, selection, mut args) = match self {
            Self::Imports => return Ok(("graphit_ast_imports", Map::new())),
            Self::Search(a) => {
                let mut args = json!({"query":a.query,"mode":a.mode,"metric":a.metric})
                    .as_object()
                    .unwrap()
                    .clone();
                if let Some(k) = a.k {
                    args.insert("k".into(), json!(k));
                }
                if let Some(rerank) = a.rerank {
                    args.insert("rerank".into(), json!(rerank));
                }
                ("graphit_ast_search", &a.selection, args)
            }
            Self::Query(a) => {
                let mut args = Map::from_iter([("query".into(), Value::String(a.query.clone()))]);
                let mut parameters = Map::new();
                for (name, value) in a.parameters.iter().chain(&a.string_parameters) {
                    if parameters.insert(name.clone(), value.clone()).is_some() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("duplicate Cypher parameter: {name}"),
                        ));
                    }
                }
                if !parameters.is_empty() {
                    args.insert("parameters".into(), Value::Object(parameters));
                }
                ("graphit_ast_query", &a.selection, args)
            }
            Self::Schema(a) => ("graphit_ast_schema", a, Map::new()),
            Self::Sample(a) => ("graphit_ast_sample", a, Map::new()),
            Self::Neighborhood(a) => {
                let mut args = json!({"anchor_label":a.anchor_label,"anchor_key":{"type":match a.key_type{KeyType::String=>"string",KeyType::Int64=>"int64"},"value":a.anchor_key},"limit":a.limit}).as_object().unwrap().clone();
                if let Some(relationship) = &a.relationship {
                    args.insert("relationship".into(), relationship.clone().into());
                }
                if let Some(label) = &a.other_label {
                    args.insert("other_label".into(), label.clone().into());
                }
                if let Some(direction) = a.direction {
                    args.insert(
                        "direction".into(),
                        match direction {
                            Direction::Outgoing => "outgoing",
                            Direction::Incoming => "incoming",
                        }
                        .into(),
                    );
                }
                if a.cursor.is_some() && a.cursor_file.is_some() {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                let cursor = match &a.cursor_file {
                    Some(path) => Some(crate::cli_cursor::read_file(path, max_input_bytes)?),
                    None => a.cursor.clone(),
                };
                if let Some(cursor) = cursor {
                    args.insert("cursor".into(), cursor);
                }
                ("graphit_ast_neighborhood", &a.selection, args)
            }
        };
        if let Some(context) = &selection.context {
            if selection.project_dir.is_some() || selection.project_version.is_some() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            args.insert("context".into(), Value::String(context.clone()));
        } else {
            if let Some(path) = &selection.project_dir {
                args.insert(
                    "project_dir".into(),
                    serde_json::to_value(NativePath::new(path.clone())?)
                        .map_err(io::Error::other)?,
                );
            }
            if let Some(version) = &selection.project_version {
                args.insert("project_version".into(), Value::String(version.clone()));
            }
        }
        Ok((name, args))
    }
}

impl Command {
    pub fn prepare(
        &self,
        content: graphit_hub::native::ContentLimits,
    ) -> io::Result<CallToolRequestParams> {
        let (name, arguments) = self.wire(content.max_input_bytes)?;
        if serde_json::to_vec(&arguments)
            .map_err(io::Error::other)?
            .len()
            > content.max_input_bytes
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(CallToolRequestParams::new(name).with_arguments(arguments))
    }
}

pub async fn run(
    installation: &InstallationPaths,
    client: &reqwest::Client,
    mut prepared: CallToolRequestParams,
    policy: (NativeDocumentFileLimits, graphit_hub::native::ContentLimits),
    original: &RequestContext<'_>,
    output: &mut impl io::Write,
) -> io::Result<()> {
    original.check()?;
    let address = crate::source_entry::address(installation, policy.0, original).await?;
    let connection =
        crate::ast_source::connect_source(installation, address, client, policy.0, original, ())
            .await?;
    let deadline = tokio::time::Instant::from_std(original.snapshot().required_deadline()?);
    crate::mcp_output::json_call(&mut prepared);
    let result = tokio::select! {
        result=connection.peer().call_tool(prepared)=>result.map_err(io::Error::other),
        ()=original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
        ()=tokio::time::sleep_until(deadline)=>Err(io::ErrorKind::TimedOut.into()),
    };
    connection.cancel().await.map_err(io::Error::other)?;
    original.check()?;
    let result = result?;
    if result.is_error == Some(true) {
        return Err(io::Error::other(
            serde_json::to_string(&result).map_err(io::Error::other)?,
        ));
    }
    let value = result.structured_content.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "graph response has no structured content",
        )
    })?;
    let encoded = serde_json::to_vec(&value).map_err(io::Error::other)?;
    if encoded.len() > policy.1.max_output_bytes {
        return Err(io::ErrorKind::InvalidData.into());
    }
    original.check()?;
    output.write_all(&encoded)?;
    output.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        command: Command,
    }

    #[test]
    fn repeated_named_parameters_preserve_types_and_literal_strings() {
        let parsed = Cli::try_parse_from([
            "graphit",
            "query",
            "RETURN $name, $count, $enabled, $options",
            "--param-string",
            "name=ação=teste",
            "--param",
            "count=3",
            "--param",
            "enabled=true",
            "--param",
            "options={\"optional\":null,\"ids\":[1,2]}",
        ])
        .unwrap();
        let (_, wire) = parsed.command.wire(4096).unwrap();
        assert_eq!(
            wire["parameters"],
            json!({"name":"ação=teste", "count":3,
            "enabled":true,"options":{"optional":null,"ids":[1,2]}})
        );
        for args in [
            vec!["graphit", "query", "RETURN 1", "--parameters", "{}"],
            vec!["graphit", "query", "RETURN 1", "--param", "invalid"],
            vec!["graphit", "query", "RETURN 1", "--param", "value=not-json"],
        ] {
            assert!(Cli::try_parse_from(args).is_err());
        }
        let duplicate = Cli::try_parse_from([
            "graphit",
            "query",
            "RETURN $x",
            "--param",
            "x=1",
            "--param-string",
            "x=one",
        ])
        .unwrap();
        assert_eq!(
            duplicate.command.wire(4096).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn neighborhood_without_filters_requests_one_page_of_all_relationships() {
        let parsed = Cli::try_parse_from([
            "graphit",
            "neighborhood",
            "--anchor-label",
            "Function",
            "--anchor-key",
            "selected",
        ])
        .unwrap();
        let (tool, wire) = parsed.command.wire(4096).unwrap();
        assert_eq!(tool, "graphit_ast_neighborhood");
        assert_eq!(
            wire["anchor_key"],
            json!({"type":"string","value":"selected"})
        );
        for optional in ["relationship", "other_label", "direction"] {
            assert!(!wire.contains_key(optional));
        }
    }

    #[test]
    fn neighborhood_opaque_cursor_can_be_read_from_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cursor.json");
        let cursor = json!({"fingerprint":vec![5;32],"position":{
            "uid":"ação","relationship":"CALLS","from_label":"Function",
            "from_key":{"type":"int64","value":"9007199254740993"},
            "to_label":"Function","to_key":{"type":"string","value":"target"},
            "occurrence":"9007199254740993"
        }});
        std::fs::write(&path, serde_json::to_vec(&cursor).unwrap()).unwrap();
        let mut args = vec![
            "graphit",
            "neighborhood",
            "--anchor-label",
            "Function",
            "--anchor-key",
            "a",
            "--relationship",
            "CALLS",
            "--other-label",
            "Function",
            "--cursor-file",
            path.to_str().unwrap(),
        ];
        let parsed = Cli::try_parse_from(&args).unwrap();
        assert_eq!(parsed.command.wire(4096).unwrap().1["cursor"], cursor);
        args.extend(["--cursor", "{}"]);
        assert!(Cli::try_parse_from(args).is_err());
    }
    #[test]
    fn graph_search_preserves_omission_and_explicit_rerank_overrides() {
        for rerank in [None, Some(false), Some(true)] {
            let command = Command::Search(Search {
                selection: Selection::default(),
                query: "needle".into(),
                mode: "fts".into(),
                k: Some(10),
                metric: "cosine".into(),
                rerank,
            });
            let (name, args) = command.wire(4096).unwrap();
            assert_eq!(name, "graphit_ast_search");
            assert_eq!(args.get("rerank").and_then(Value::as_bool), rerank);
            assert_eq!(args.contains_key("rerank"), rerank.is_some());
        }
    }

    #[test]
    fn graph_wire_keeps_project_native_paths_context_and_closed_parameter_shapes() {
        let project = tempfile::tempdir().unwrap();
        let mut command = Command::Query(Query {
            selection: Selection {
                project_dir: Some(project.path().to_path_buf()),
                project_version: Some("feature/graph".into()),
                context: None,
            },
            query: "MATCH (f:File) RETURN f.path".into(),
            parameters: vec![("path".into(), json!("main.rs"))],
            string_parameters: Vec::new(),
        });
        let (name, args) = command.wire(4096).unwrap();
        assert_eq!(name, "graphit_ast_query");
        assert_eq!(
            serde_json::from_value::<NativePath>(args["project_dir"].clone())
                .unwrap()
                .as_path(),
            project.path()
        );
        assert_eq!(args["project_version"], "feature/graph");
        assert_eq!(args["parameters"]["path"], "main.rs");
        command.selection_mut().unwrap().context = Some("artifact@v1".into());
        assert_eq!(
            command.wire(4096).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        let installed = Command::Schema(Selection {
            context: Some("artifact@v1".into()),
            ..Selection::default()
        });
        assert_eq!(installed.wire(4096).unwrap().1.len(), 1);
        let invalid = Command::Query(Query {
            selection: Selection::default(),
            query: "RETURN 1".into(),
            parameters: vec![("path".into(), json!(1))],
            string_parameters: vec![("path".into(), json!("duplicate"))],
        });
        assert_eq!(
            invalid.wire(4096).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
