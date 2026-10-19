//! CLI ingress for the canonical authenticated Knowledge source reader.
use graphit_core::{native_path::NativePath, text_slice};
use graphit_hub::policy::RequestContext;
use rmcp::model::CallToolRequestParams;
use serde_json::{Map, json};
use std::{io, path::PathBuf};

pub use crate::knowledge_search_entry::call;

#[derive(clap::Args)]
pub struct Arguments {
    /// Exact Knowledge page slug; this is not a filesystem path.
    pub slug: String,
    #[arg(long)]
    pub head: Option<i64>,
    #[arg(long)]
    pub tail: Option<i64>,
    #[arg(long = "start")]
    pub start_line: Option<i64>,
    #[arg(long = "end")]
    pub end_line: Option<i64>,
    #[arg(long)]
    pub pattern: Option<String>,
    #[arg(long)]
    pub regex: bool,
    #[arg(long)]
    pub before: Option<i64>,
    #[arg(long)]
    pub after: Option<i64>,
    #[arg(long)]
    pub line_numbers: bool,
    #[arg(long, conflicts_with_all = ["artifact_id", "artifact_version", "context_alias"])]
    pub project_dir: Option<PathBuf>,
    #[arg(long, requires = "artifact_version", conflicts_with = "project_dir")]
    pub artifact_id: Option<String>,
    #[arg(long, requires = "artifact_id", conflicts_with = "project_dir")]
    pub artifact_version: Option<String>,
    #[arg(long, requires = "artifact_id", conflicts_with = "project_dir")]
    pub context_alias: Option<String>,
}

pub fn prepare(
    arguments: &Arguments,
    version: Option<&str>,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    graphit_data::knowledge_index::validate_slug(&arguments.slug).map_err(io::Error::other)?;
    let selectors = text_slice::Request {
        head: arguments.head.unwrap_or_default(),
        tail: arguments.tail.unwrap_or_default(),
        start_line: arguments.start_line.unwrap_or_default(),
        end_line: arguments.end_line.unwrap_or_default(),
        pattern: arguments.pattern.clone().unwrap_or_default(),
        is_regex: arguments.regex,
        before: arguments.before.unwrap_or_default(),
        after: arguments.after.unwrap_or_default(),
        line_numbers: arguments.line_numbers,
    };
    text_slice::apply("", &selectors)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let mut input = Map::from_iter([
        ("slug".into(), arguments.slug.clone().into()),
        (
            "selectors".into(),
            serde_json::to_value(selectors).map_err(io::Error::other)?,
        ),
    ]);
    if let Some(id) = &arguments.artifact_id {
        if arguments.project_dir.is_some() || version.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let artifact_version = arguments
            .artifact_version
            .as_ref()
            .ok_or(io::ErrorKind::InvalidInput)?;
        let mut artifact = json!({"id":id,"version":artifact_version});
        if let Some(alias) = &arguments.context_alias {
            artifact["context_alias"] = alias.clone().into();
        }
        input.insert(
            "source".into(),
            json!({"kind":"imported_knowledge","artifact":artifact}),
        );
    } else {
        if arguments.artifact_version.is_some() || arguments.context_alias.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if let Some(root) = &arguments.project_dir {
            input.insert(
                "project_dir".into(),
                serde_json::to_value(NativePath::new(std::fs::canonicalize(root)?)?)
                    .map_err(io::Error::other)?,
            );
        }
        if let Some(version) = version {
            input.insert("project_version".into(), version.into());
        }
    }
    Ok(CallToolRequestParams::new("graphit_wiki_source").with_arguments(input))
}
