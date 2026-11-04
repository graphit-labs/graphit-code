//! Finite Task CLI fields. The native protocol remains the final input validator.
use super::CallArguments;
use clap::{Args, ValueEnum};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    io,
    num::{NonZeroU64, NonZeroUsize},
    path::PathBuf,
};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn identifier(text: &str) -> Result<String, String> {
    serde_json::from_value::<graphit_records::task_spec::TaskId>(json!(text))
        .map(|_| text.to_owned())
        .map_err(|e| e.to_string())
}
fn claim_token(text: &str) -> Result<String, String> {
    serde_json::from_value::<graphit_records::task_protocol::TransportToken>(json!(text))
        .map(|_| text.to_owned())
        .map_err(|e| e.to_string())
}
fn commit(text: &str) -> Result<String, String> {
    if matches!(text.len(), 40 | 64)
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(text.into())
    } else {
        Err(
            "commit must be a full lowercase hexadecimal Git object ID (40 or 64 characters)"
                .into(),
        )
    }
}
fn put(value: &mut Value, key: &str, field: &Option<impl Serialize>) {
    if let Some(field) = field {
        value[key] = json!(field);
    }
}
#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Open,
    InProgress,
    Completed,
    Cancelled,
}
#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Fts,
    Semantic,
    Hybrid,
}
#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Table {
    Tasks,
    Sessions,
}
#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Column {
    Id,
    Status,
    Title,
    Revision,
    CreatedAt,
    UpdatedAt,
    SessionId,
    ParentId,
    TaskType,
    Priority,
    Flagged,
}
#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    Acceptance,
    Test,
}
#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentKind {
    Note,
    Decision,
    Problem,
    Lesson,
    Knowledge,
}
#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceType {
    Task,
    Session,
    Memory,
    Knowledge,
}

#[derive(Args, Default)]
pub struct CursorArguments {
    /// Opaque cursor returned by this exact operation (JSON object for catalogue/search, text for query).
    #[arg(long, conflicts_with = "cursor_file")]
    pub cursor: Option<String>,
    /// Read the opaque cursor from a regular bounded file; JSON object for catalogue/search, raw text for query.
    #[arg(long, conflicts_with = "cursor")]
    pub cursor_file: Option<PathBuf>,
}
fn cursor_value(
    text: Option<&str>,
    file: Option<&PathBuf>,
    max: usize,
    object: bool,
) -> io::Result<Option<Value>> {
    let file_text;
    let text = if let Some(file) = file {
        file_text = crate::cli_cursor::read_text_file(file, max)?;
        Some(file_text.as_str())
    } else {
        text
    };
    text.map(|text| {
        if text.len() > max {
            return Err(invalid("cursor exceeds input byte limit"));
        }
        if object {
            crate::cli_cursor::parse(text).map_err(invalid)
        } else {
            Ok(Value::String(text.trim().to_owned()))
        }
    })
    .transpose()
}
impl CursorArguments {
    fn value(&self, max: usize, object: bool) -> io::Result<Option<Value>> {
        cursor_value(
            self.cursor.as_deref(),
            self.cursor_file.as_ref(),
            max,
            object,
        )
    }
}
#[derive(Args, Default)]
pub struct HistoryArguments {
    /// Request the history section with this positive page size.
    #[arg(long)]
    pub history_page_size: Option<NonZeroUsize>,
    /// Opaque history continuation returned for this record.
    #[arg(
        long,
        requires = "history_page_size",
        conflicts_with = "history_cursor_file"
    )]
    pub history_cursor: Option<String>,
    /// Read raw history cursor text from a regular bounded file.
    #[arg(
        long,
        requires = "history_page_size",
        conflicts_with = "history_cursor"
    )]
    pub history_cursor_file: Option<PathBuf>,
}
impl HistoryArguments {
    fn apply(&self, v: &mut Value, max: usize) -> io::Result<()> {
        if let Some(size) = self.history_page_size {
            v["history"] = json!({"page_size":size});
            if let Some(cursor) = cursor_value(
                self.history_cursor.as_deref(),
                self.history_cursor_file.as_ref(),
                max,
                false,
            )? {
                v["history"]["cursor"] = cursor;
            }
        }
        Ok(())
    }
}
#[derive(Args)]
pub struct GetTaskArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    /// Exact Task ULID.
    #[arg(long, value_parser=identifier)]
    pub id: String,
    #[command(flatten)]
    pub history: HistoryArguments,
    /// Request the comments section with this positive page size.
    #[arg(long)]
    pub comments_page_size: Option<NonZeroUsize>,
    #[arg(
        long,
        requires = "comments_page_size",
        conflicts_with = "comments_cursor_file"
    )]
    pub comments_cursor: Option<String>,
    /// Read raw comments cursor text from a regular bounded file.
    #[arg(
        long,
        requires = "comments_page_size",
        conflicts_with = "comments_cursor"
    )]
    pub comments_cursor_file: Option<PathBuf>,
}
impl GetTaskArguments {
    pub(super) fn input(&self, max: usize) -> io::Result<Value> {
        let mut v = json!({"id":self.id});
        self.history.apply(&mut v, max)?;
        if let Some(size) = self.comments_page_size {
            v["comments"] = json!({"page_size":size});
            if let Some(c) = cursor_value(
                self.comments_cursor.as_deref(),
                self.comments_cursor_file.as_ref(),
                max,
                false,
            )? {
                v["comments"]["cursor"] = c;
            }
        }
        Ok(v)
    }
}
#[derive(Args)]
pub struct GetSessionArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[arg(long, value_parser=identifier)]
    pub id: String,
    #[command(flatten)]
    pub history: HistoryArguments,
}
impl GetSessionArguments {
    pub(super) fn input(&self, max: usize) -> io::Result<Value> {
        let mut v = json!({"id":self.id});
        self.history.apply(&mut v, max)?;
        Ok(v)
    }
}
#[derive(Args, Default)]
pub struct FilterArguments {
    #[arg(long, value_enum)]
    pub status: Option<Status>,
    #[arg(long, value_parser=identifier)]
    pub session_id: Option<String>,
    /// Filter by the flag's exact boolean value.
    #[arg(long, action=clap::ArgAction::Set)]
    pub flagged: Option<bool>,
    #[arg(long, action=clap::ArgAction::Set)]
    pub blocked: Option<bool>,
}
impl FilterArguments {
    fn value(&self) -> Value {
        let mut v = json!({});
        put(&mut v, "status", &self.status);
        put(&mut v, "session_id", &self.session_id);
        put(&mut v, "flagged", &self.flagged);
        put(&mut v, "blocked", &self.blocked);
        v
    }
}
#[derive(Args)]
pub struct CatalogueArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub filter: FilterArguments,
    /// Literal catalogue filter text.
    #[arg(long)]
    pub query: Option<String>,
    /// Positive number of rows in this page.
    #[arg(long)]
    pub page_size: NonZeroUsize,
    #[command(flatten)]
    pub cursor: CursorArguments,
}
impl CatalogueArguments {
    pub(super) fn input(&self, max: usize) -> io::Result<Value> {
        let mut filter = self.filter.value();
        put(&mut filter, "query", &self.query);
        let mut v = json!({"filter":filter,"page_size":self.page_size});
        put(&mut v, "cursor", &self.cursor.value(max, true)?);
        Ok(v)
    }
}
#[derive(Args)]
pub struct SearchArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub filter: FilterArguments,
    #[arg(long, value_enum, default_value = "fts")]
    pub mode: SearchMode,
    /// Literal search text.
    #[arg(long)]
    pub text: String,
    /// Positive number of rows in this page.
    #[arg(long)]
    pub page_size: NonZeroUsize,
    /// Positive maximum number of results for the complete query.
    #[arg(long)]
    pub top_k: NonZeroUsize,
    #[command(flatten)]
    pub cursor: CursorArguments,
}
impl SearchArguments {
    pub(super) fn input(&self, max: usize) -> io::Result<Value> {
        let mut v = json!({"mode":self.mode,"text":self.text,"filter":self.filter.value(),"page_size":self.page_size,"top_k":self.top_k});
        put(&mut v, "cursor", &self.cursor.value(max, true)?);
        Ok(v)
    }
}
#[derive(Args)]
pub struct QueryArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[arg(long, value_enum)]
    pub table: Table,
    /// Exact IDs; repeat the flag or separate values by commas.
    #[arg(long="id", value_parser=identifier, value_delimiter=',', conflicts_with="empty_ids")]
    pub ids: Vec<String>,
    /// Select an explicitly empty ID set (zero results), rather than omitting the ID filter.
    #[arg(long, conflicts_with = "ids")]
    pub empty_ids: bool,
    #[arg(long, value_enum)]
    pub status: Option<Status>,
    /// Task table only; a Session query cannot filter by session ID.
    #[arg(long, value_parser=identifier)]
    pub session_id: Option<String>,
    /// Projected columns in output order; repeat or use commas.
    #[arg(long = "column", value_enum, value_delimiter = ',', required = true)]
    pub columns: Vec<Column>,
    /// Positive number of rows in this page.
    #[arg(long)]
    pub page_size: NonZeroUsize,
    /// Positive maximum number of results for the complete query.
    #[arg(long)]
    pub top_k: NonZeroUsize,
    #[command(flatten)]
    pub cursor: CursorArguments,
}
impl QueryArguments {
    pub(super) fn input(&self, max: usize) -> io::Result<Value> {
        let mut selection = json!({"table":self.table});
        if !self.ids.is_empty() || self.empty_ids {
            selection["ids"] = json!(self.ids);
        }
        put(&mut selection, "status", &self.status);
        put(&mut selection, "session_id", &self.session_id);
        let mut v = json!({"selection":selection,"columns":self.columns,"page_size":self.page_size,"top_k":self.top_k});
        put(&mut v, "cursor", &self.cursor.value(max, false)?);
        Ok(v)
    }
}

#[derive(Args, Default)]
pub struct ReferenceArguments {
    /// Replace authored references. Pair repeated type/id/relation flags by occurrence.
    #[arg(long="reference-type", value_enum, requires_all=["reference_ids","reference_relations"], conflicts_with="clear_references")]
    pub reference_types: Vec<ReferenceType>,
    #[arg(long="reference-id", requires_all=["reference_types","reference_relations"], conflicts_with="clear_references")]
    pub reference_ids: Vec<String>,
    #[arg(long="reference-relation", requires_all=["reference_types","reference_ids"], conflicts_with="clear_references")]
    pub reference_relations: Vec<String>,
    /// Omit entirely or repeat once per reference; an empty value leaves that entry's scope absent.
    #[arg(long = "reference-scope", requires = "reference_types")]
    pub reference_scopes: Vec<String>,
    /// Omit entirely or repeat once per reference; empty means absent.
    #[arg(long = "reference-scope-id", requires = "reference_types")]
    pub reference_scope_ids: Vec<String>,
    /// Omit entirely or repeat once per reference; empty means absent.
    #[arg(long = "reference-context", requires = "reference_types")]
    pub reference_contexts: Vec<String>,
    /// Omit entirely or repeat once per reference; empty text is a literal empty field.
    #[arg(long = "reference-field", requires = "reference_types")]
    pub reference_fields: Vec<String>,
    /// Omit fields at these 1-based reference positions; repeat or use commas. The corresponding field slot must be empty.
    #[arg(
        long = "reference-field-absent",
        requires = "reference_fields",
        value_delimiter = ','
    )]
    pub reference_fields_absent: Vec<NonZeroUsize>,
    /// Explicitly replace authored references with an empty list.
    #[arg(long, conflicts_with_all=["reference_types","reference_ids","reference_relations"])]
    pub clear_references: bool,
}
impl ReferenceArguments {
    #[cfg(target_os = "linux")]
    fn apply(&self, v: &mut Value) -> io::Result<()> {
        let n = self.reference_types.len();
        if self.reference_ids.len() != n || self.reference_relations.len() != n {
            return Err(invalid("reference type, id and relation counts must match"));
        }
        for (name, fields) in [
            ("scope", &self.reference_scopes),
            ("scope-id", &self.reference_scope_ids),
            ("context", &self.reference_contexts),
            ("field", &self.reference_fields),
        ] {
            if !fields.is_empty() && fields.len() != n {
                return Err(invalid(format!(
                    "reference {name} must be omitted entirely or have one value per reference"
                )));
            }
        }
        let mut absent_fields = std::collections::HashSet::new();
        for index in &self.reference_fields_absent {
            if index.get() > n || !absent_fields.insert(index.get()) {
                return Err(invalid(
                    "absent reference field positions must be unique and within the reference list",
                ));
            }
            if self
                .reference_fields
                .get(index.get() - 1)
                .is_none_or(|field| !field.is_empty())
            {
                return Err(invalid(
                    "an explicitly absent reference field must have an empty field slot",
                ));
            }
        }
        if n > 0 || self.clear_references {
            let mut entries = Vec::with_capacity(n);
            for i in 0..n {
                let mut e = json!({"target":{"type":self.reference_types[i],"id":self.reference_ids[i]},"relation":self.reference_relations[i]});
                for (name, values) in [
                    ("scope", &self.reference_scopes),
                    ("scope_id", &self.reference_scope_ids),
                    ("context", &self.reference_contexts),
                ] {
                    if let Some(s) = values.get(i).filter(|s| !s.is_empty()) {
                        e["target"][name] = json!(s);
                    }
                }
                if let Some(s) = self
                    .reference_fields
                    .get(i)
                    .filter(|_| !absent_fields.contains(&(i + 1)))
                {
                    e["field"] = json!(s);
                }
                entries.push(e);
            }
            v["references"] = json!(entries);
        }
        Ok(())
    }
}
#[derive(Args)]
pub struct OwnedArguments {
    /// Task or Session ULID selected by this command.
    #[arg(long, value_parser=identifier)]
    pub id: String,
    /// Current opaque participant fence; never an actor selector.
    #[arg(long, value_parser=claim_token)]
    pub claim_token: String,
    /// Revision returned with the current claim.
    #[arg(long)]
    pub expected_revision: u64,
}
impl OwnedArguments {
    #[cfg(target_os = "linux")]
    fn value(&self) -> Value {
        json!({"id":self.id,"claim":{"token":self.claim_token,"expected_revision":self.expected_revision}})
    }
}
#[derive(Args, Default)]
pub struct OptionalClaimArguments {
    #[arg(long, value_parser=claim_token, requires="claim_revision")]
    pub claim_token: Option<String>,
    /// Revision of an optional claim (separate from set-task-commit's record CAS).
    #[arg(long, requires = "claim_token")]
    pub claim_revision: Option<u64>,
}
impl OptionalClaimArguments {
    #[cfg(target_os = "linux")]
    fn apply(&self, v: &mut Value) {
        if let Some(token) = &self.claim_token {
            v["claim"] = json!({"token":token,"expected_revision":self.claim_revision});
        }
    }
}
#[derive(Args, Default)]
pub struct ScheduleArguments {
    /// Explicit calendar date in YYYY-MM-DD format.
    #[arg(long)]
    pub start_date: Option<graphit_records::task_schedule::CalendarDate>,
    /// Explicit calendar date in YYYY-MM-DD format.
    #[arg(long)]
    pub end_date: Option<graphit_records::task_schedule::CalendarDate>,
    /// Explicit calendar date in YYYY-MM-DD format.
    #[arg(long)]
    pub due_date: Option<graphit_records::task_schedule::CalendarDate>,
}
impl ScheduleArguments {
    #[cfg(target_os = "linux")]
    fn value(&self) -> Option<Value> {
        if self.start_date.is_none() && self.end_date.is_none() && self.due_date.is_none() {
            None
        } else {
            let mut v = json!({});
            put(&mut v, "start_date", &self.start_date);
            put(&mut v, "end_date", &self.end_date);
            put(&mut v, "due_date", &self.due_date);
            Some(v)
        }
    }
}
#[derive(Args, Default)]
pub struct CheckListArguments {
    /// Check categories paired by occurrence with --check-text (new checks when revising).
    #[arg(long = "check-kind", value_enum, requires = "check_texts")]
    pub check_kinds: Vec<CheckKind>,
    /// Literal check text paired by occurrence with --check-kind.
    #[arg(long = "check-text", requires = "check_kinds")]
    pub check_texts: Vec<String>,
}
impl CheckListArguments {
    #[cfg(target_os = "linux")]
    fn value(&self) -> io::Result<Value> {
        if self.check_kinds.len() != self.check_texts.len() {
            return Err(invalid("check kind and text counts must match"));
        }
        Ok(json!(
            self.check_kinds
                .iter()
                .zip(&self.check_texts)
                .map(|(kind, text)| json!({"kind":kind,"text":text}))
                .collect::<Vec<_>>()
        ))
    }
}

#[derive(Args)]
pub struct CreateSessionArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    /// Stable key identifying this creation or comment request.
    #[arg(long)]
    pub idempotency_key: String,
    /// Literal title; omit during revision to preserve it.
    #[arg(long)]
    pub title: String,
    /// Literal description; omit during revision to preserve it.
    #[arg(long)]
    pub description: String,
    /// Literal Session strategy.
    #[arg(long)]
    pub strategy: String,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl CreateSessionArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v = json!({"idempotency_key":self.idempotency_key,"title":self.title,"description":self.description,"strategy":self.strategy});
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct CreateTaskArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    /// Stable key identifying this creation or comment request.
    #[arg(long)]
    pub idempotency_key: String,
    /// Literal title; omit during revision to preserve it.
    #[arg(long)]
    pub title: String,
    /// Literal description; omit during revision to preserve it.
    #[arg(long)]
    pub description: String,
    /// Task classification.
    #[arg(long)]
    pub task_type: String,
    #[arg(long,value_parser=identifier)]
    pub session_id: String,
    /// Priority from 0 through 4.
    #[arg(long,value_parser=clap::value_parser!(u8).range(0..=4))]
    pub priority: u8,
    /// Parent Task ULID.
    #[arg(long,value_parser=identifier)]
    pub parent_id: Option<String>,
    /// Dependency Task ULIDs; repeat or separate by commas.
    #[arg(long,value_parser=identifier,value_delimiter=',')]
    pub depends_on: Vec<String>,
    /// Full lowercase hexadecimal Git commit ID (40 or 64 characters).
    #[arg(long,value_parser=commit)]
    pub commit: Option<String>,
    #[command(flatten)]
    pub schedule: ScheduleArguments,
    #[command(flatten)]
    pub checks: CheckListArguments,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl CreateTaskArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v = json!({"idempotency_key":self.idempotency_key,"session_id":self.session_id,"title":self.title,"description":self.description,"task_type":self.task_type,"priority":self.priority,"depends_on":self.depends_on,"checks":self.checks.value()?});
        put(&mut v, "parent_id", &self.parent_id);
        put(&mut v, "commit", &self.commit);
        put(&mut v, "schedule", &self.schedule.value());
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct ClaimArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[arg(long,value_parser=identifier)]
    pub id: String,
    /// Positive claim lifetime in milliseconds.
    #[arg(long)]
    pub lease_ms: NonZeroU64,
}
impl ClaimArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        Ok(json!({"id":self.id,"lease_ms":self.lease_ms}))
    }
}

#[derive(Args)]
pub struct HeartbeatArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub owned: OwnedArguments,
    /// Positive claim lifetime in milliseconds.
    #[arg(long)]
    pub lease_ms: NonZeroU64,
}
impl HeartbeatArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        Ok(json!({"target":self.owned.value(),"lease_ms":self.lease_ms}))
    }
}

#[derive(Args)]
pub struct CommentArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub owned: OwnedArguments,
    /// Positive claim lifetime in milliseconds.
    #[arg(long)]
    pub lease_ms: NonZeroU64,
    /// Stable key identifying this creation or comment request.
    #[arg(long)]
    pub idempotency_key: String,
    /// Comment category.
    #[arg(long, value_enum)]
    pub kind: CommentKind,
    /// Literal Markdown comment body.
    #[arg(long)]
    pub body: String,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl CommentArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v = json!({"target":self.owned.value(),"lease_ms":self.lease_ms,"idempotency_key":self.idempotency_key,"kind":self.kind,"body":self.body});
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct ProgressArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub owned: OwnedArguments,
    /// Positive claim lifetime in milliseconds.
    #[arg(long)]
    pub lease_ms: NonZeroU64,
    /// Literal progress or release summary.
    #[arg(long)]
    pub summary: String,
    /// Literal next action.
    #[arg(long)]
    pub next_step: String,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl ProgressArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v = json!({"target":self.owned.value(),"lease_ms":self.lease_ms,"summary":self.summary,"next_step":self.next_step});
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct ReleaseArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub owned: OwnedArguments,
    /// Literal progress or release summary.
    #[arg(long)]
    pub summary: String,
    /// Literal next action.
    #[arg(long)]
    pub next_step: String,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl ReleaseArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v =
            json!({"target":self.owned.value(),"summary":self.summary,"next_step":self.next_step});
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct ReviseTaskArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub owned: OwnedArguments,
    /// Positive claim lifetime in milliseconds.
    #[arg(long)]
    pub lease_ms: NonZeroU64,
    /// Recorded reason for this change.
    #[arg(long)]
    pub reason: String,
    /// Literal title; omit during revision to preserve it.
    #[arg(long)]
    pub title: Option<String>,
    /// Literal description; omit during revision to preserve it.
    #[arg(long)]
    pub description: Option<String>,
    /// Task classification.
    #[arg(long)]
    pub task_type: Option<String>,
    /// Priority from 0 through 4.
    #[arg(long,value_parser=clap::value_parser!(u8).range(0..=4))]
    pub priority: Option<u8>,
    #[command(flatten)]
    pub schedule: ScheduleArguments,
    /// Remove all scheduled dates; omission preserves the current schedule.
    #[arg(long,conflicts_with_all=["start_date","end_date","due_date"])]
    pub clear_schedule: bool,
    #[command(flatten)]
    pub checks: CheckListArguments,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl ReviseTaskArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v = json!({"target":self.owned.value(),"lease_ms":self.lease_ms,"reason":self.reason,"add_checks":self.checks.value()?});
        put(&mut v, "title", &self.title);
        put(&mut v, "description", &self.description);
        put(&mut v, "task_type", &self.task_type);
        put(&mut v, "priority", &self.priority);
        put(&mut v, "schedule", &self.schedule.value());
        if self.clear_schedule {
            v["schedule"] = Value::Null;
        }
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct ReviseSessionArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub owned: OwnedArguments,
    /// Positive claim lifetime in milliseconds.
    #[arg(long)]
    pub lease_ms: NonZeroU64,
    /// Recorded reason for this change.
    #[arg(long)]
    pub reason: String,
    /// Literal title; omit during revision to preserve it.
    #[arg(long)]
    pub title: Option<String>,
    /// Literal description; omit during revision to preserve it.
    #[arg(long)]
    pub description: Option<String>,
    /// Literal Session strategy.
    #[arg(long)]
    pub strategy: Option<String>,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl ReviseSessionArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v =
            json!({"target":self.owned.value(),"lease_ms":self.lease_ms,"reason":self.reason});
        put(&mut v, "title", &self.title);
        put(&mut v, "description", &self.description);
        put(&mut v, "strategy", &self.strategy);
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct CheckArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub owned: OwnedArguments,
    /// Positive claim lifetime in milliseconds.
    #[arg(long)]
    pub lease_ms: NonZeroU64,
    /// Current check ULID.
    #[arg(long,value_parser=identifier)]
    pub check_id: String,
    /// Explicit verification result: true or false.
    #[arg(long, action=clap::ArgAction::Set, required=true)]
    pub passed: bool,
    /// Evidence supporting this check result.
    #[arg(long)]
    pub evidence: String,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl CheckArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v = json!({"target":self.owned.value(),"lease_ms":self.lease_ms,"check_id":self.check_id,"passed":self.passed,"evidence":self.evidence});
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct SupersedeArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub owned: OwnedArguments,
    /// Positive claim lifetime in milliseconds.
    #[arg(long)]
    pub lease_ms: NonZeroU64,
    /// Current check ULID.
    #[arg(long,value_parser=identifier)]
    pub check_id: String,
    /// Recorded reason for this change.
    #[arg(long)]
    pub reason: String,
    /// Create a replacement check with this literal text.
    #[arg(long)]
    pub replacement_text: Option<String>,
    /// Optional replacement category; omission preserves the superseded category.
    #[arg(long, value_enum, requires = "replacement_text")]
    pub replacement_kind: Option<CheckKind>,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl SupersedeArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v = json!({"target":self.owned.value(),"lease_ms":self.lease_ms,"check_id":self.check_id,"reason":self.reason});
        if let Some(text) = &self.replacement_text {
            let mut replacement = json!({"text":text});
            put(&mut replacement, "kind", &self.replacement_kind);
            v["replacement"] = replacement;
        }
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct LinksArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub owned: OwnedArguments,
    /// Positive claim lifetime in milliseconds.
    #[arg(long)]
    pub lease_ms: NonZeroU64,
    /// Recorded reason for this change.
    #[arg(long)]
    pub reason: String,
    /// Parent Task ULID.
    #[arg(long,value_parser=identifier,conflicts_with="clear_parent")]
    pub parent_id: Option<String>,
    /// Remove the parent link.
    #[arg(long, conflicts_with = "parent_id")]
    pub clear_parent: bool,
    /// Dependency Task ULIDs; repeat or separate by commas.
    #[arg(long,value_parser=identifier,value_delimiter=',',conflicts_with="clear_dependencies")]
    pub depends_on: Vec<String>,
    /// Replace dependencies with an empty list.
    #[arg(long, conflicts_with = "depends_on")]
    pub clear_dependencies: bool,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl LinksArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v =
            json!({"target":self.owned.value(),"lease_ms":self.lease_ms,"reason":self.reason});
        put(&mut v, "parent_id", &self.parent_id);
        if self.clear_parent {
            v["parent_id"] = Value::Null;
        }
        if !self.depends_on.is_empty() || self.clear_dependencies {
            v["depends_on"] = json!(self.depends_on);
        }
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct FlagArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub owned: OwnedArguments,
    /// Positive claim lifetime in milliseconds.
    #[arg(long)]
    pub lease_ms: NonZeroU64,
    /// Explicit flag value: true or false.
    #[arg(long, action=clap::ArgAction::Set, required=true)]
    pub flagged: bool,
    /// Recorded reason for this change.
    #[arg(long)]
    pub reason: String,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl FlagArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v = json!({"target":self.owned.value(),"lease_ms":self.lease_ms,"flagged":self.flagged,"reason":self.reason});
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct CommitArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[arg(long,value_parser=identifier)]
    pub id: String,
    #[arg(long)]
    pub expected_revision: u64,
    #[command(flatten)]
    pub claim: OptionalClaimArguments,
    /// Full lowercase hexadecimal Git commit ID (40 or 64 characters).
    #[arg(long,value_parser=commit,required_unless_present="clear_commit",conflicts_with="clear_commit")]
    pub commit: Option<String>,
    /// Explicitly remove the commit association.
    #[arg(long, conflicts_with = "commit")]
    pub clear_commit: bool,
    /// Recorded reason for this change.
    #[arg(long)]
    pub reason: String,
}
impl CommitArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v = json!({"id":self.id,"expected_revision":self.expected_revision,"commit":self.commit,"reason":self.reason});
        self.claim.apply(&mut v);
        Ok(v)
    }
}

#[derive(Args)]
pub struct CompleteArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[command(flatten)]
    pub owned: OwnedArguments,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl CompleteArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v = self.owned.value();
        self.references.apply(&mut v)?;
        Ok(v)
    }
}

#[derive(Args)]
pub struct CancelArguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[arg(long,value_parser=identifier)]
    pub id: String,
    #[command(flatten)]
    pub claim: OptionalClaimArguments,
    /// Recorded reason for this change.
    #[arg(long)]
    pub reason: String,
    #[command(flatten)]
    pub references: ReferenceArguments,
}
impl CancelArguments {
    #[cfg(target_os = "linux")]
    pub(super) fn input(&self, _max: usize) -> io::Result<Value> {
        let mut v = json!({"id":self.id,"reason":self.reason});
        self.claim.apply(&mut v);
        self.references.apply(&mut v)?;
        Ok(v)
    }
}
