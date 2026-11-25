//! Reference edges are read in their authorized origin; targets confer no access.
use crate::task_entry::CallArguments;
use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{Value, json};
use std::{io, num::NonZeroUsize};
#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Command,
}
#[derive(Args)]
pub struct Page {
    /// Maximum references per page.
    #[arg(long, default_value = "20")]
    page_size: NonZeroUsize,
    /// Exact JSON cursor for the next outgoing page; quote it as one argument.
    #[arg(long,value_parser=crate::cli_cursor::parse)]
    outgoing_cursor: Option<Value>,
    /// Exact JSON cursor for the next incoming page; quote it as one argument.
    #[arg(long,value_parser=crate::cli_cursor::parse)]
    incoming_cursor: Option<Value>,
}
impl Page {
    fn apply(&self, value: &mut Value) {
        value["page_size"] = json!(self.page_size);
        if let Some(cursor) = &self.outgoing_cursor {
            value["outgoing_cursor"] = cursor.clone();
        }
        if let Some(cursor) = &self.incoming_cursor {
            value["incoming_cursor"] = cursor.clone();
        }
    }
}
#[derive(Args)]
pub struct Task {
    #[command(flatten)]
    selection: CallArguments,
    /// Exact Task ID; choose either --task or --session.
    #[arg(long, value_parser=crate::task_entry::identifier, required_unless_present = "session", conflicts_with = "session")]
    task: Option<String>,
    /// Exact Session ID; choose either --task or --session.
    #[arg(long, value_parser=crate::task_entry::identifier, required_unless_present = "task", conflicts_with = "task")]
    session: Option<String>,
    #[command(flatten)]
    page: Page,
}
#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Task,
    Session,
    Memory,
    Knowledge,
}
#[derive(Args)]
#[command(group(clap::ArgGroup::new("knowledge_target_scope").args(["target_project_id", "target_user_id"])))]
pub struct Knowledge {
    #[command(flatten)]
    selection: CallArguments,
    /// Source document slug for outgoing edges.
    #[arg(long, required_unless_present = "target_id")]
    outgoing: Option<String>,
    /// Logical target for incoming edges; it does not authorize target content.
    #[arg(long, requires_all = ["target_kind", "knowledge_target_scope"])]
    target_id: Option<String>,
    /// Kind of the logical target.
    #[arg(long, value_enum, requires = "target_id")]
    target_kind: Option<Kind>,
    /// Project that owns the target; mutually exclusive with --target-user-id.
    #[arg(long, requires = "target_id", conflicts_with = "target_user_id")]
    target_project_id: Option<String>,
    /// User that owns the target; mutually exclusive with --target-project-id.
    #[arg(long, requires = "target_id", conflicts_with = "target_project_id")]
    target_user_id: Option<String>,
    /// Installed context of the logical target, when applicable.
    #[arg(long, requires = "target_id")]
    target_context: Option<String>,
    #[command(flatten)]
    page: Page,
}
#[derive(Args)]
pub struct Backlinks {
    #[command(flatten)]
    selection: CallArguments,
    /// Exact identifier of the logical target.
    #[arg(long)]
    target_id: String,
    /// Kind of the logical target.
    #[arg(long, value_enum)]
    target_kind: Kind,
    /// Project that owns the target; mutually exclusive with --target-user-id.
    #[arg(
        long,
        required_unless_present = "target_user_id",
        conflicts_with = "target_user_id"
    )]
    target_project_id: Option<String>,
    /// User that owns the target; mutually exclusive with --target-project-id.
    #[arg(
        long,
        required_unless_present = "target_project_id",
        conflicts_with = "target_project_id"
    )]
    target_user_id: Option<String>,
    /// Installed context of the logical target, when applicable.
    #[arg(long)]
    target_context: Option<String>,
    /// Maximum references per page.
    #[arg(long, default_value = "20")]
    page_size: NonZeroUsize,
    /// Exact JSON continuation object returned by the previous page.
    #[arg(long,value_parser=crate::cli_cursor::parse)]
    cursor: Option<Value>,
}
#[derive(Subcommand)]
pub enum Command {
    /// Query outgoing and incoming edges of a Task or Session from the Task corpus.
    Task(Task),
    /// Find Task/Session sources that reference any logical target; never open that target.
    TaskBacklinks(Backlinks),
    /// Query outgoing document edges and incoming logical-target edges in Knowledge.
    Knowledge(Knowledge),
}
impl Command {
    pub fn selection_mut(&mut self) -> &mut CallArguments {
        match self {
            Self::Task(v) => &mut v.selection,
            Self::TaskBacklinks(v) => &mut v.selection,
            Self::Knowledge(v) => &mut v.selection,
        }
    }
    pub fn prepare(&mut self) -> io::Result<(&'static str, Value)> {
        let (operation, input) = match self {
            Self::Task(value) => {
                let focus = if let Some(id) = &value.task {
                    json!({"task":id})
                } else {
                    json!({"session":value.session})
                };
                let mut input = json!({"focus":focus});
                value.page.apply(&mut input);
                ("task_references", input)
            }
            Self::TaskBacklinks(value) => {
                let scope = match (&value.target_project_id, &value.target_user_id) {
                    (Some(id), None) => json!({"project":id}),
                    (None, Some(id)) => json!({"user":id}),
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Exactly one target scope is required",
                        ));
                    }
                };
                let mut input = json!({"target":{"target_kind":value.target_kind,"scope":scope,"id":value.target_id,"context":value.target_context},"page_size":value.page_size});
                if let Some(cursor) = &value.cursor {
                    input["cursor"] = cursor.clone();
                }
                ("task_references", input)
            }
            Self::Knowledge(value) => {
                let mut input = json!({});
                if let Some(slug) = &value.outgoing {
                    input["outgoing"] = json!(slug);
                }
                if let Some(id) = &value.target_id {
                    let scope = match (&value.target_project_id, &value.target_user_id) {
                        (Some(id), None) => json!({"project":id}),
                        (None, Some(id)) => json!({"user":id}),
                        _ => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "incoming Knowledge references require --target-project-id or --target-user-id",
                            ));
                        }
                    };
                    input["incoming"] =
                        json!({"id":id,"target_kind":value.target_kind,"scope":scope});
                    if let Some(context) = &value.target_context {
                        input["incoming"]["context"] = json!(context);
                    }
                }
                value.page.apply(&mut input);
                ("knowledge_references", input)
            }
        };
        let selection = self.selection_mut();
        let mut selected = json!({});
        if let Some(dir) = &selection.project_dir {
            selected["project_dir"] =
                serde_json::to_value(graphit_core::native_path::NativePath::new(dir.clone())?)?;
        }
        if let Some(id) = &selection.project_id {
            selected["project_id"] = json!(id);
        }
        if let Some(version) = &selection.project_version {
            selected["project_version"] = json!(version);
        }
        Ok((operation, json!({"selection":selected,"input":input})))
    }
}
