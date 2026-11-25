//! Human Native record administration; no managed-role credentials are synthesized.
use clap::{Args, Subcommand, ValueEnum};
use graphit_core::{native_path::NativePath, project_paths::InstallationPaths};
use graphit_hub::policy::RequestContext;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    io::{self, Read, Write},
    num::{NonZeroU64, NonZeroUsize},
    path::PathBuf,
};

#[derive(Args, Default)]
pub struct Selection {
    /// Select a project directory (project scope only).
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    /// Select an exact project ID (project scope only).
    #[arg(long)]
    pub project_id: Option<String>,
    #[arg(skip)]
    pub project_version: Option<String>,
    /// Select project records or personal records for the local installation or signed-in account.
    #[arg(long, value_enum, default_value = "project")]
    pub scope: crate::memory_index_entry::Scope,
}
#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    Fact,
    Skill,
    Tension,
    Decision,
    Convention,
    Correction,
}
#[derive(Args, Default, Serialize)]
pub struct Filter {
    /// Keep records with this classification.
    #[arg(long, value_enum)]
    classification: Option<Class>,
    /// Keep records with this exact tag.
    #[arg(long)]
    tag: Option<String>,
    /// Filter mandatory status; use --mandatory=false for unmarked records.
    #[arg(long,num_args=0..=1,default_missing_value="true",require_equals=true,action=clap::ArgAction::Set)]
    mandatory: Option<bool>,
    /// Filter important status; use --important=false for unmarked records.
    #[arg(long,num_args=0..=1,default_missing_value="true",require_equals=true,action=clap::ArgAction::Set)]
    important: Option<bool>,
    /// Exclude mandatory records from the results.
    #[arg(long)]
    exclude_mandatory: bool,
}
#[derive(Args)]
pub struct List {
    #[command(flatten)]
    selection: Selection,
    #[command(flatten)]
    filter: Filter,
}
#[derive(Args)]
pub struct Record {
    #[command(flatten)]
    selection: Selection,
    #[arg(long)]
    id: graphit_records::memory_store::MemoryId,
}
#[derive(Args)]
pub struct Search {
    #[command(flatten)]
    selection: Selection,
    /// Words to search for in Memory.
    #[arg(value_name = "QUERY")]
    pub text: String,
    #[command(flatten)]
    filter: Filter,
    /// Maximum number of search results.
    #[arg(long, default_value = "20")]
    top_k: NonZeroUsize,
    /// Search mode; semantic and hybrid require configured embeddings.
    #[arg(long, value_enum, default_value = "fts")]
    mode: crate::knowledge_search_entry::Mode,
    /// Also search retained history of deleted records.
    #[arg(long)]
    include_deleted_history: bool,
}
#[derive(Args, Default)]
pub struct Body {
    /// Literal Markdown body.
    #[arg(long, conflicts_with = "body_file")]
    body: Option<String>,
    /// UTF-8 file, or - to read standard input.
    #[arg(long, conflicts_with = "body")]
    body_file: Option<PathBuf>,
}
impl Body {
    fn read(&self, maximum: usize) -> io::Result<Option<String>> {
        if let Some(body) = &self.body {
            return if body.len() <= maximum {
                Ok(Some(body.clone()))
            } else {
                Err(io::ErrorKind::InvalidInput.into())
            };
        }
        let Some(path) = &self.body_file else {
            return Ok(None);
        };
        let reader: Box<dyn Read> = if path.as_os_str() == "-" {
            Box::new(io::stdin().lock())
        } else {
            Box::new(std::fs::File::open(path)?)
        };
        let mut bytes = Vec::new();
        reader
            .take(maximum.saturating_add(1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > maximum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Memory body exceeds input limit",
            ));
        }
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Memory body must be UTF-8"))
    }
}
#[derive(Args)]
#[command(group(clap::ArgGroup::new("memory_create_body").args(["body", "body_file"]).required(true)))]
pub struct Create {
    #[command(flatten)]
    selection: Selection,
    /// Stable key for retrying the same logical change.
    #[arg(long)]
    idempotency_key: String,
    #[arg(long)]
    title: String,
    #[command(flatten)]
    body: Body,
    #[arg(long, value_enum, default_value = "fact")]
    classification: Class,
    #[arg(long)]
    mandatory: bool,
    #[arg(long)]
    important: bool,
    /// Add a tag; repeat --tag for each tag.
    #[arg(long = "tag", value_name = "TAG")]
    tags: Vec<String>,
}
#[derive(Args)]
pub struct Change {
    #[command(flatten)]
    selection: Selection,
    #[arg(long)]
    id: graphit_records::memory_store::MemoryId,
    /// Current record revision; a mismatch rejects the change.
    #[arg(long)]
    expected_revision: NonZeroU64,
    /// Stable key for retrying the same logical change.
    #[arg(long)]
    idempotency_key: String,
    /// Explain why this record is being changed.
    #[arg(long)]
    reason: String,
}
impl Change {
    fn value(&self) -> Value {
        json!({"id":self.id,"expected_revision":self.expected_revision,"idempotency_key":self.idempotency_key,"reason":self.reason})
    }
}
#[derive(Args)]
#[command(group(clap::ArgGroup::new("memory_update_fields").args(["title", "body", "body_file", "classification", "tags", "clear_tags"]).required(true).multiple(true)))]
pub struct Update {
    #[command(flatten)]
    change: Change,
    #[arg(long)]
    title: Option<String>,
    #[command(flatten)]
    body: Body,
    #[arg(long, value_enum)]
    classification: Option<Class>,
    /// Replace all tags with these values; repeat --tag for each tag.
    #[arg(long = "tag", value_name = "TAG", conflicts_with = "clear_tags")]
    tags: Vec<String>,
    /// Remove all tags.
    #[arg(long)]
    clear_tags: bool,
}
#[derive(Args)]
pub struct Mark {
    #[command(flatten)]
    change: Change,
    /// true sets the flag; false removes it through the native attribute update.
    #[arg(long, action=clap::ArgAction::Set)]
    value: bool,
}
#[derive(Subcommand)]
pub enum Command {
    /// List Memory using supported metadata filters.
    List(List),
    /// Read one current record.
    Get(Record),
    /// Search current records and optionally deleted history.
    Search(Search),
    /// Read creation and immutable revision history.
    History(Record),
    /// Create a Memory record from text, a file, or standard input.
    Create(Create),
    /// Change record fields when its current revision matches.
    Update(Update),
    /// Tombstone a record and preserve its history.
    Delete(Change),
    /// Set or remove mandatory status using native Memory update.
    Mandatory(Mark),
    /// Set or remove important status using native Memory update.
    Important(Mark),
}
impl Command {
    pub fn selection_mut(&mut self) -> &mut Selection {
        match self {
            Self::List(v) => &mut v.selection,
            Self::Get(v) | Self::History(v) => &mut v.selection,
            Self::Search(v) => &mut v.selection,
            Self::Create(v) => &mut v.selection,
            Self::Update(v) => &mut v.change.selection,
            Self::Delete(v) => &mut v.selection,
            Self::Mandatory(v) | Self::Important(v) => &mut v.change.selection,
        }
    }
    pub fn prepare(&mut self, maximum: usize) -> io::Result<(&'static str, Value)> {
        let (operation, input) = match self {
            Self::List(v) => ("memory_list", json!(v.filter)),
            Self::Get(v) => ("memory_get", json!({"id":v.id})),
            Self::History(v) => ("memory_history", json!({"id":v.id})),
            Self::Search(v) => (
                "memory_search",
                json!({"text":v.text,"filter":v.filter,"mode":v.mode,"top_k":v.top_k,"include_deleted_history":v.include_deleted_history}),
            ),
            Self::Create(v) => (
                "memory_create",
                json!({"idempotency_key":v.idempotency_key,"title":v.title,"body":v.body.read(maximum)?.ok_or_else(||io::Error::new(io::ErrorKind::InvalidInput,"create requires --body or --body-file"))?,"attributes":{"classification":v.classification,"mandatory":v.mandatory,"important":v.important},"tags":v.tags}),
            ),
            Self::Delete(v) => ("memory_delete", v.value()),
            Self::Update(v) => {
                let mut value = v.change.value();
                if let Some(body) = v.body.read(maximum)? {
                    value["body"] = json!(body);
                }
                if let Some(title) = &v.title {
                    value["title"] = json!(title);
                }
                if let Some(class) = v.classification {
                    value["classification"] = json!(class);
                }
                if v.clear_tags || !v.tags.is_empty() {
                    value["tags"] = json!(v.tags);
                }
                ("memory_update", value)
            }
            Self::Mandatory(v) => {
                let mut value = v.change.value();
                value["mandatory"] = json!(v.value);
                ("memory_update", value)
            }
            Self::Important(v) => {
                let mut value = v.change.value();
                value["important"] = json!(v.value);
                ("memory_update", value)
            }
        };
        let selection = self.selection_mut();
        if selection.scope == crate::memory_index_entry::Scope::User
            && (selection.project_dir.is_some()
                || selection.project_id.is_some()
                || selection.project_version.is_some())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "USER Memory rejects PROJECT selectors",
            ));
        }
        let mut selected = json!({});
        if let Some(dir) = &selection.project_dir {
            selected["project_dir"] = serde_json::to_value(NativePath::new(dir.clone())?)?;
        }
        if let Some(id) = &selection.project_id {
            selected["project_id"] = json!(id);
        }
        if let Some(version) = &selection.project_version {
            selected["project_version"] = json!(version);
        }
        let value = json!({"scope":selection.scope,"selection":selected,"input":input});
        if serde_json::to_vec(&value)?.len() > maximum {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok((operation, value))
    }
}

/// The existing producer supplies a bounded effect receipt even if JSON delivery
/// fails. Only its declared metadata headers belong in this fallback.
fn header_receipt(headers: &reqwest::header::HeaderMap) -> Option<Value> {
    let state = headers.get("x-graphit-memory-outcome")?.to_str().ok()?;
    if !matches!(
        state,
        "completed" | "confirmed" | "uncertain" | "not_applied"
    ) {
        return None;
    }
    let mut native = json!({"state":state,"receipt_source":"response_headers"});
    for (header, field) in [
        ("x-graphit-memory-id", "id"),
        ("x-graphit-memory-completion-error", "completion_error"),
        ("x-graphit-memory-delivery-error", "delivery_error"),
        ("x-graphit-memory-response-error", "response_error"),
    ] {
        if let Some(value) = headers.get(header).and_then(|v| v.to_str().ok()) {
            native[field] = json!(value);
        }
    }
    for (header, field) in [
        ("x-graphit-memory-revision", "revision"),
        ("x-graphit-memory-sdk-version", "sdk_version"),
        ("x-graphit-memory-index-sdk-version", "index_sdk_version"),
    ] {
        if let Some(value) = headers
            .get(header)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
        {
            native[field] = json!(value);
        }
    }
    Some(json!({"native":native,"response_complete":false}))
}

pub async fn call_native(
    installation: &InstallationPaths,
    operation: &str,
    mut body: Value,
    original: &RequestContext<'_>,
    output: &mut impl Write,
) -> io::Result<()> {
    if [
        "GRAPHIT_ROLE_BEARER",
        "GRAPHIT_ROLE_REFERENCE",
        "GRAPHIT_ROLE_SELECTION",
    ]
    .iter()
    .any(|key| std::env::var_os(key).is_some())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Native record administration is unavailable in managed-role context",
        ));
    }
    let address = crate::source_entry::address(installation, original).await?;
    let headers = crate::ast_source::runtime_headers(installation, original).await?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(io::Error::other)?;
    if body["scope"] != "user"
        && body["selection"]["project_version"].is_null()
        && let Some(root) = body["selection"]["project_dir"].as_str()
    {
        let version = crate::task_entry::captured_project_version(
            installation,
            std::path::Path::new(root),
            original,
        )
        .await?;
        body["selection"]["project_version"] = json!(version);
    }
    if serde_json::to_vec(&body)?.len() > original.content_limits().max_input_bytes {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let operation = async {
        let mut response = client
            .post(format!("http://{address}/api/records/native/{operation}"))
            .headers(headers.into_iter().collect())
            .json(&body)
            .send()
            .await
            .map_err(io::Error::other)?;
        let status = response.status();
        if !status.is_success()
            && matches!(
                operation,
                "memory_create" | "memory_update" | "memory_delete"
            )
            && let Some(receipt) = header_receipt(response.headers())
        {
            let bytes = serde_json::to_vec(&receipt)?;
            if bytes.len() > original.content_limits().max_output_bytes {
                return Err(io::Error::other(format!(
                    "Memory effect {} acknowledged; response and receipt exceed the output limit. Read current state before retrying.",
                    receipt["native"]["state"].as_str().unwrap_or("unknown")
                )));
            }
            output.write_all(&bytes)?;
            writeln!(output)?;
            return Err(io::Error::other(
                "Memory effect receipt returned from native headers; response delivery was incomplete. Read current state before retrying.",
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(io::Error::other)? {
            if chunk.len()
                > original
                    .content_limits()
                    .max_output_bytes
                    .saturating_sub(bytes.len())
            {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Native records HTTP {}: invalid response", status.as_u16()),
            )
        })?;
        if !status.is_success() {
            return Err(io::Error::other(value.to_string()));
        }
        serde_json::to_writer(&mut *output, &value)?;
        writeln!(output)?;
        if value["native"].get("idempotency_key").is_some()
            && (value["native"]["state"] != "completed"
                || !value["native"]["completion_error"].is_null()
                || !value["native"]["delivery_error"].is_null())
        {
            return Err(io::Error::other(
                "Memory publication has a partial or uncertain effect; the native receipt was returned",
            ));
        }
        original.check()?;
        Ok(())
    };
    tokio::select! {result=operation=>result,()=original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),()=tokio::time::sleep_until(original.snapshot().required_deadline()?.into())=>Err(io::ErrorKind::TimedOut.into())}
}
