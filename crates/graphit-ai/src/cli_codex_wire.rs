//! Pinned Codex 0.160.1 wire data. No launcher, terminal, authority or progress projection.
use graphit_core::file_io::SnapshotContext;
use serde::de::{self, Deserialize, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Deserializer, value::RawValue};
use std::{
    collections::TryReserveError, error::Error, fmt, io, num::ParseIntError, str::Utf8Error,
};

#[derive(Clone, Copy)]
pub struct CodexWireLimits {
    pub frame_bytes: usize,
    pub depth: usize,
    pub nodes: usize,
    pub fields: usize,
    pub array_items: usize,
    pub string_bytes: usize,
    pub decoded_string_bytes: usize,
    pub id_bytes: usize,
}
impl CodexWireLimits {
    pub(crate) fn validate(&self) -> Result<(), CodexWireError> {
        policy_queue(0, self)
            .map(|_| ())
            .map_err(|cause| wire_error(cause, WireStage::Policy))
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CodexWireStats {
    pub nodes: usize,
    pub fields: usize,
    pub array_items: usize,
    pub decoded_string_bytes: usize,
    pub max_depth: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexEventKind {
    ThreadStarted,
    TurnStarted,
    TurnCompleted,
    TurnFailed,
    ItemStarted,
    ItemUpdated,
    ItemCompleted,
    Error,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexItemPhase {
    Started,
    Updated,
    Completed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexChangeKind {
    Add,
    Delete,
    Update,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexCollabTool {
    SpawnAgent,
    SendInput,
    Wait,
    CloseAgent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexAgentStatus {
    PendingInit,
    Running,
    Interrupted,
    Completed,
    Errored,
    Shutdown,
    NotFound,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodexUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub cache_write_input_tokens: i64,
}
pub enum JsonView<'a> {
    DefaultNull,
    Raw(&'a str),
}
impl<'a> JsonView<'a> {
    pub fn as_str(&self) -> &'a str {
        match self {
            Self::DefaultNull => "null",
            Self::Raw(raw) => raw,
        }
    }
}
pub struct CodexChangeView<'a> {
    pub path: &'a str,
    pub kind: CodexChangeKind,
}
pub struct CodexTodoView<'a> {
    pub text: &'a str,
    pub completed: bool,
}
pub struct CodexAgentStateView<'a> {
    pub id: &'a str,
    pub status: CodexAgentStatus,
    pub message: Option<&'a str>,
}
pub struct CodexListView<'a, T> {
    bytes: &'a [u8],
    entries: &'a [Entry],
    project: fn(&'a [u8], &'a Entry) -> T,
}
impl<'a, T: 'a> CodexListView<'a, T> {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<T> {
        self.entries
            .get(index)
            .map(|entry| (self.project)(self.bytes, entry))
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = T> + 'a {
        let bytes = self.bytes;
        let project = self.project;
        self.entries.iter().map(move |entry| project(bytes, entry))
    }
}
pub type CodexStringList<'a> = CodexListView<'a, &'a str>;
pub type CodexRawJsonList<'a> = CodexListView<'a, JsonView<'a>>;
pub type CodexChangeList<'a> = CodexListView<'a, CodexChangeView<'a>>;
pub type CodexTodoList<'a> = CodexListView<'a, CodexTodoView<'a>>;
pub type CodexAgentStateList<'a> = CodexListView<'a, CodexAgentStateView<'a>>;
pub enum CodexWebActionView<'a> {
    Search {
        query: Option<&'a str>,
        queries: Option<CodexStringList<'a>>,
    },
    OpenPage {
        url: Option<&'a str>,
    },
    FindInPage {
        url: Option<&'a str>,
        pattern: Option<&'a str>,
    },
    Other,
}
pub struct CodexMcpResultView<'a> {
    pub content: CodexRawJsonList<'a>,
    pub meta: Option<JsonView<'a>>,
    pub structured_content: Option<JsonView<'a>>,
}
pub enum CodexItemBodyView<'a> {
    AgentMessage {
        text: &'a str,
    },
    Reasoning {
        text: &'a str,
    },
    CommandExecution {
        command: &'a str,
        aggregated_output: &'a str,
        exit_code: Option<i32>,
        status: CodexStatus,
    },
    FileChange {
        changes: CodexChangeList<'a>,
        status: CodexStatus,
    },
    McpToolCall {
        server: &'a str,
        tool: &'a str,
        arguments: JsonView<'a>,
        result: Option<CodexMcpResultView<'a>>,
        error: Option<&'a str>,
        status: CodexStatus,
    },
    CollabToolCall {
        tool: CodexCollabTool,
        sender_thread_id: &'a str,
        receiver_thread_ids: CodexStringList<'a>,
        prompt: Option<&'a str>,
        agents_states: CodexAgentStateList<'a>,
        status: CodexStatus,
    },
    WebSearch {
        inner_id: &'a str,
        query: &'a str,
        action: CodexWebActionView<'a>,
        results: Option<CodexRawJsonList<'a>>,
    },
    TodoList {
        items: CodexTodoList<'a>,
    },
    Error {
        message: &'a str,
    },
}
pub struct CodexItemView<'a> {
    pub outer_id: &'a str,
    pub body: CodexItemBodyView<'a>,
}
impl<'a> CodexItemView<'a> {
    pub fn sdk_observed_id(&self) -> &'a str {
        match self.body {
            CodexItemBodyView::WebSearch { inner_id, .. } => inner_id,
            _ => self.outer_id,
        }
    }
}
pub enum CodexEventView<'a> {
    ThreadStarted {
        thread_id: &'a str,
    },
    TurnStarted,
    TurnCompleted {
        usage: CodexUsage,
    },
    TurnFailed {
        message: &'a str,
    },
    Item {
        phase: CodexItemPhase,
        item: CodexItemView<'a>,
    },
    Error {
        message: &'a str,
    },
}
#[expect(
    clippy::large_enum_variant,
    reason = "The known owner stays inline; boxing would add an allocation outside the two admitted arenas."
)]
pub enum CodexDecodedFrame {
    Ignored(CodexWireStats),
    Known(OwnedCodexEvent),
}

#[derive(Clone, Copy, Default)]
struct Span {
    start: usize,
    len: usize,
}
#[derive(Clone, Copy, Default)]
struct Range {
    start: usize,
    len: usize,
}
#[derive(Clone, Copy)]
enum Entry {
    String(Span),
    Raw(Span),
    Change(Span, CodexChangeKind),
    Todo(Span, bool),
    Agent(Span, CodexAgentStatus, Option<Span>),
}
struct Arena {
    bytes: Vec<u8>,
    entries: Vec<Entry>,
    byte_limit: usize,
    entry_limit: usize,
}
fn text(bytes: &[u8], span: Span) -> &str {
    std::str::from_utf8(&bytes[span.start..span.start + span.len]).expect("validated arena UTF8")
}
impl Arena {
    fn copy(&mut self, bytes: &[u8]) -> Result<Span, Cause> {
        let end = add(self.bytes.len(), bytes.len())?;
        if end > self.byte_limit {
            return Err(Cause::Policy("byte arena"));
        }
        let span = Span {
            start: self.bytes.len(),
            len: bytes.len(),
        };
        self.bytes.extend_from_slice(bytes);
        Ok(span)
    }
    fn push(&mut self, entry: Entry) -> Result<(), Cause> {
        if self.entries.len() >= self.entry_limit {
            return Err(Cause::Policy("entry arena"));
        }
        self.entries.push(entry);
        Ok(())
    }
    fn list<'a, T>(
        &'a self,
        range: Range,
        project: fn(&'a [u8], &'a Entry) -> T,
    ) -> CodexListView<'a, T> {
        CodexListView {
            bytes: &self.bytes,
            entries: &self.entries[range.start..range.start + range.len],
            project,
        }
    }
}
fn string_entry<'a>(bytes: &'a [u8], entry: &Entry) -> &'a str {
    match entry {
        Entry::String(span) => text(bytes, *span),
        _ => unreachable!("typed string entry"),
    }
}
fn raw_entry<'a>(bytes: &'a [u8], entry: &Entry) -> JsonView<'a> {
    match entry {
        Entry::Raw(span) => JsonView::Raw(text(bytes, *span)),
        _ => unreachable!("typed raw entry"),
    }
}
fn change_entry<'a>(bytes: &'a [u8], entry: &Entry) -> CodexChangeView<'a> {
    match entry {
        Entry::Change(span, kind) => CodexChangeView {
            path: text(bytes, *span),
            kind: *kind,
        },
        _ => unreachable!("typed change entry"),
    }
}
fn todo_entry<'a>(bytes: &'a [u8], entry: &Entry) -> CodexTodoView<'a> {
    match entry {
        Entry::Todo(span, completed) => CodexTodoView {
            text: text(bytes, *span),
            completed: *completed,
        },
        _ => unreachable!("typed todo entry"),
    }
}
fn agent_entry<'a>(bytes: &'a [u8], entry: &Entry) -> CodexAgentStateView<'a> {
    match entry {
        Entry::Agent(id, status, message) => CodexAgentStateView {
            id: text(bytes, *id),
            status: *status,
            message: message.map(|span| text(bytes, span)),
        },
        _ => unreachable!("typed agent entry"),
    }
}
enum WebAction {
    Search(Option<Span>, Option<Range>),
    OpenPage(Option<Span>),
    FindInPage(Option<Span>, Option<Span>),
    Other,
}
struct McpResult {
    content: Range,
    meta: Option<Span>,
    structured_content: Option<Span>,
}
enum Body {
    AgentMessage(Span),
    Reasoning(Span),
    Command(Span, Span, Option<i32>, CodexStatus),
    File(Range, CodexStatus),
    Mcp(
        Span,
        Span,
        Option<Span>,
        Option<McpResult>,
        Option<Span>,
        CodexStatus,
    ),
    Collab(
        CodexCollabTool,
        Span,
        Range,
        Option<Span>,
        Range,
        CodexStatus,
    ),
    Web(Span, Span, WebAction, Option<Range>),
    Todo(Range),
    Error(Span),
}
enum Event {
    Thread(Span),
    Turn,
    Usage(CodexUsage),
    Failed(Span),
    Item(CodexItemPhase, Span, Body),
    Error(Span),
}
pub struct OwnedCodexEvent {
    event: Event,
    arena: Arena,
    stats: CodexWireStats,
}
impl OwnedCodexEvent {
    pub fn kind(&self) -> CodexEventKind {
        match self.event {
            Event::Thread(_) => CodexEventKind::ThreadStarted,
            Event::Turn => CodexEventKind::TurnStarted,
            Event::Usage(_) => CodexEventKind::TurnCompleted,
            Event::Failed(_) => CodexEventKind::TurnFailed,
            Event::Item(CodexItemPhase::Started, ..) => CodexEventKind::ItemStarted,
            Event::Item(CodexItemPhase::Updated, ..) => CodexEventKind::ItemUpdated,
            Event::Item(CodexItemPhase::Completed, ..) => CodexEventKind::ItemCompleted,
            Event::Error(_) => CodexEventKind::Error,
        }
    }
    pub fn stats(&self) -> CodexWireStats {
        self.stats
    }
    pub fn view(&self) -> CodexEventView<'_> {
        let arena = &self.arena;
        let s = |span| text(&arena.bytes, span);
        let optional = |span: Option<Span>| span.map(s);
        match &self.event {
            Event::Thread(id) => CodexEventView::ThreadStarted { thread_id: s(*id) },
            Event::Turn => CodexEventView::TurnStarted,
            Event::Usage(usage) => CodexEventView::TurnCompleted { usage: *usage },
            Event::Failed(message) => CodexEventView::TurnFailed {
                message: s(*message),
            },
            Event::Error(message) => CodexEventView::Error {
                message: s(*message),
            },
            Event::Item(phase, id, body) => {
                let body = match body {
                    Body::AgentMessage(v) => CodexItemBodyView::AgentMessage { text: s(*v) },
                    Body::Reasoning(v) => CodexItemBodyView::Reasoning { text: s(*v) },
                    Body::Command(command, output, exit_code, status) => {
                        CodexItemBodyView::CommandExecution {
                            command: s(*command),
                            aggregated_output: s(*output),
                            exit_code: *exit_code,
                            status: *status,
                        }
                    }
                    Body::File(changes, status) => CodexItemBodyView::FileChange {
                        changes: arena.list(*changes, change_entry),
                        status: *status,
                    },
                    Body::Mcp(server, tool, arguments, result, error, status) => {
                        CodexItemBodyView::McpToolCall {
                            server: s(*server),
                            tool: s(*tool),
                            arguments: arguments
                                .map_or(JsonView::DefaultNull, |span| JsonView::Raw(s(span))),
                            result: result.as_ref().map(|r| CodexMcpResultView {
                                content: arena.list(r.content, raw_entry),
                                meta: r.meta.map(|span| JsonView::Raw(s(span))),
                                structured_content: r
                                    .structured_content
                                    .map(|span| JsonView::Raw(s(span))),
                            }),
                            error: optional(*error),
                            status: *status,
                        }
                    }
                    Body::Collab(tool, sender, receivers, prompt, states, status) => {
                        CodexItemBodyView::CollabToolCall {
                            tool: *tool,
                            sender_thread_id: s(*sender),
                            receiver_thread_ids: arena.list(*receivers, string_entry),
                            prompt: optional(*prompt),
                            agents_states: arena.list(*states, agent_entry),
                            status: *status,
                        }
                    }
                    Body::Web(id, query, action, results) => {
                        let action = match action {
                            WebAction::Search(query, queries) => CodexWebActionView::Search {
                                query: optional(*query),
                                queries: queries.map(|r| arena.list(r, string_entry)),
                            },
                            WebAction::OpenPage(url) => CodexWebActionView::OpenPage {
                                url: optional(*url),
                            },
                            WebAction::FindInPage(url, pattern) => CodexWebActionView::FindInPage {
                                url: optional(*url),
                                pattern: optional(*pattern),
                            },
                            WebAction::Other => CodexWebActionView::Other,
                        };
                        CodexItemBodyView::WebSearch {
                            inner_id: s(*id),
                            query: s(*query),
                            action,
                            results: results.map(|r| arena.list(r, raw_entry)),
                        }
                    }
                    Body::Todo(items) => CodexItemBodyView::TodoList {
                        items: arena.list(*items, todo_entry),
                    },
                    Body::Error(message) => CodexItemBodyView::Error {
                        message: s(*message),
                    },
                };
                CodexEventView::Item {
                    phase: *phase,
                    item: CodexItemView {
                        outer_id: s(*id),
                        body,
                    },
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireStage {
    Policy,
    Depth,
    Grammar,
    Audit,
    Schema,
    Build,
    Publish,
}
enum Cause {
    Io(io::Error),
    Sdk(serde_json::Error),
    Integer(ParseIntError),
    Allocation { _cause: TryReserveError },
    Utf8(Utf8Error),
    Policy(&'static str),
}
impl Cause {
    fn label(&self) -> &'static str {
        match self {
            Self::Io(_) => "native request",
            Self::Sdk(_) => "JSON decoder",
            Self::Integer(error) => {
                if matches!(
                    error.kind(),
                    std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow
                ) {
                    "integer range"
                } else {
                    "typed integer"
                }
            }
            Self::Allocation { .. } => "allocation",
            Self::Utf8(error) => {
                if error.error_len().is_none() {
                    "incomplete UTF8"
                } else {
                    "invalid UTF8"
                }
            }
            Self::Policy(label) => label,
        }
    }
}
struct SafeCause(Cause);
impl fmt::Display for SafeCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.label())
    }
}
impl fmt::Debug for SafeCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl Error for SafeCause {}
pub struct CodexWireError {
    stage: WireStage,
    cause: SafeCause,
}
impl CodexWireError {
    pub fn stage(&self) -> WireStage {
        self.stage
    }
    pub fn io_kind(&self) -> Option<io::ErrorKind> {
        match &self.cause.0 {
            Cause::Io(error) => Some(error.kind()),
            _ => None,
        }
    }
    pub fn sdk_position(&self) -> Option<(serde_json::error::Category, usize, usize)> {
        match &self.cause.0 {
            Cause::Sdk(error) => Some((error.classify(), error.line(), error.column())),
            _ => None,
        }
    }
}
impl fmt::Display for CodexWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Codex wire {:?}: {}", self.stage, self.cause)
    }
}
impl fmt::Debug for CodexWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl Error for CodexWireError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}
fn add(a: usize, b: usize) -> Result<usize, Cause> {
    a.checked_add(b)
        .filter(|v| *v <= isize::MAX as usize)
        .ok_or(Cause::Policy("layout overflow"))
}
fn limited(value: usize, limit: usize, label: &'static str) -> Result<usize, Cause> {
    if value > limit {
        Err(Cause::Policy(label))
    } else {
        Ok(value)
    }
}
#[derive(Clone, Copy)]
struct Pending<'de> {
    raw: &'de RawValue,
    depth: usize,
    key: Option<Span>,
}
struct TransientState<'n, 'de> {
    original: &'n SnapshotContext,
    limits: &'n CodexWireLimits,
    queue: Vec<Pending<'de>>,
    queue_limit: usize,
    stats: CodexWireStats,
    first: Option<Cause>,
    stage: WireStage,
}
impl<'n, 'de> TransientState<'n, 'de> {
    fn check(&self) -> Result<(), Cause> {
        self.original.check().map_err(Cause::Io)
    }
    fn bridge<E: de::Error>(&mut self, cause: Cause) -> E {
        if self.first.is_none() {
            self.first = Some(cause);
        }
        E::custom("wire")
    }
    fn finish<T>(&mut self, result: Result<T, serde_json::Error>) -> Result<T, Cause> {
        if self.first.is_none()
            && let Err(cause) = self.check()
        {
            self.first = Some(cause);
        }
        if let Some(cause) = self.first.take() {
            Err(cause)
        } else {
            result.map_err(Cause::Sdk)
        }
    }
    fn push(
        &mut self,
        raw: &'de RawValue,
        depth: usize,
        key: Option<Span>,
        audit: bool,
    ) -> Result<(), Cause> {
        self.check()?;
        if self.queue.len() >= self.queue_limit {
            return Err(Cause::Policy("pending nodes"));
        }
        if audit {
            self.stats.nodes = limited(add(self.stats.nodes, 1)?, self.limits.nodes, "nodes")?;
            if raw.get().starts_with(['{', '[']) {
                self.stats.max_depth = self.stats.max_depth.max(depth);
            }
        }
        self.queue.push(Pending { raw, depth, key });
        Ok(())
    }
}
struct StringSeed<'s, 'n, 'de, F> {
    state: &'s mut TransientState<'n, 'de>,
    callback: F,
}
impl<'de, F, T> DeserializeSeed<'de> for StringSeed<'_, '_, 'de, F>
where
    F: FnMut(&str) -> Result<T, Cause>,
{
    type Value = T;
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<T, D::Error> {
        deserializer.deserialize_str(self)
    }
}
impl<'de, F, T> Visitor<'de> for StringSeed<'_, '_, 'de, F>
where
    F: FnMut(&str) -> Result<T, Cause>,
{
    type Value = T;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("string")
    }
    fn visit_str<E: de::Error>(mut self, value: &str) -> Result<T, E> {
        self.state
            .check()
            .and_then(|()| (self.callback)(value))
            .map_err(|cause| self.state.bridge(cause))
    }
}
fn string_value<'de, T>(
    raw: &'de RawValue,
    state: &mut TransientState<'_, 'de>,
    callback: impl FnMut(&str) -> Result<T, Cause>,
) -> Result<T, Cause> {
    state.check()?;
    if !raw.get().starts_with('"') {
        return Err(Cause::Policy("string shape"));
    }
    let result = {
        let mut deserializer = Deserializer::from_str(raw.get());
        let value = StringSeed { state, callback }.deserialize(&mut deserializer);
        match value {
            Ok(value) => deserializer.end().map(|()| value),
            Err(error) => Err(error),
        }
    };
    state.finish(result)
}
struct Audit<'s, 'n, 'de> {
    state: &'s mut TransientState<'n, 'de>,
    depth: usize,
    map: bool,
}
impl<'de> DeserializeSeed<'de> for Audit<'_, '_, 'de> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        if self.map {
            deserializer.deserialize_map(self)
        } else {
            deserializer.deserialize_seq(self)
        }
    }
}
impl<'de> Visitor<'de> for Audit<'_, '_, 'de> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("container")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        loop {
            let key = map.next_key_seed(StringSeed {
                state: self.state,
                callback: |value: &str| Ok(value.len()),
            })?;
            let Some(key_bytes) = key else {
                break;
            };
            let result = (|| {
                self.state.check()?;
                limited(key_bytes, self.state.limits.string_bytes, "string bytes")?;
                self.state.stats.decoded_string_bytes = limited(
                    add(self.state.stats.decoded_string_bytes, key_bytes)?,
                    self.state.limits.decoded_string_bytes,
                    "decoded string bytes",
                )?;
                self.state.stats.fields = limited(
                    add(self.state.stats.fields, 1)?,
                    self.state.limits.fields,
                    "fields",
                )?;
                Ok(())
            })();
            result.map_err(|cause| self.state.bridge(cause))?;
            let raw = map.next_value::<&RawValue>()?;
            self.state
                .push(raw, self.depth + 1, None, true)
                .map_err(|cause| self.state.bridge(cause))?;
        }
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        while let Some(raw) = sequence.next_element::<&RawValue>()? {
            let result = (|| {
                self.state.stats.array_items = limited(
                    add(self.state.stats.array_items, 1)?,
                    self.state.limits.array_items,
                    "array items",
                )?;
                self.state.push(raw, self.depth + 1, None, true)
            })();
            result.map_err(|cause| self.state.bridge(cause))?;
        }
        Ok(())
    }
}
fn depth_gate(frame: &[u8], state: &mut TransientState<'_, '_>) -> Result<(), Cause> {
    let mut quoted = false;
    let mut escape = false;
    let mut depth = 0;
    for byte in frame {
        state.check()?;
        if quoted {
            if escape {
                escape = false;
            } else if *byte == b'\\' {
                escape = true;
            } else if *byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    depth = limited(add(depth, 1)?, state.limits.depth, "depth")?;
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(())
}
fn audit<'de>(root: &'de RawValue, state: &mut TransientState<'_, 'de>) -> Result<(), Cause> {
    state.push(root, 1, None, true)?;
    let mut index = 0;
    while index < state.queue.len() {
        let pending = state.queue[index];
        state.check()?;
        match pending.raw.get().as_bytes()[0] {
            b'{' | b'[' => {
                let result = {
                    let mut deserializer = Deserializer::from_str(pending.raw.get());
                    let value = Audit {
                        state,
                        depth: pending.depth,
                        map: pending.raw.get().starts_with('{'),
                    }
                    .deserialize(&mut deserializer);
                    value.and_then(|()| deserializer.end())
                };
                state.finish(result)?;
            }
            b'"' => {
                let bytes = string_value(pending.raw, state, |value| Ok(value.len()))?;
                limited(bytes, state.limits.string_bytes, "string bytes")?;
                state.stats.decoded_string_bytes = limited(
                    add(state.stats.decoded_string_bytes, bytes)?,
                    state.limits.decoded_string_bytes,
                    "decoded string bytes",
                )?;
            }
            _ => {}
        }
        index += 1;
    }
    state.queue.clear();
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum Field {
    Type,
    Id,
    ThreadId,
    Usage,
    Error,
    Item,
    Message,
    InputTokens,
    CachedTokens,
    OutputTokens,
    ReasoningTokens,
    CacheWriteTokens,
    Text,
    Command,
    Output,
    ExitCode,
    Status,
    Changes,
    Path,
    Kind,
    Server,
    Tool,
    Arguments,
    Result,
    Content,
    Meta,
    StructuredContent,
    Sender,
    Receivers,
    Prompt,
    States,
    Query,
    Action,
    Results,
    Items,
    Completed,
    Queries,
    Url,
    Pattern,
    Extra,
}
const FIELDS: usize = Field::Extra as usize;
fn field(value: &str) -> Field {
    match value {
        "type" => Field::Type,
        "id" => Field::Id,
        "thread_id" => Field::ThreadId,
        "usage" => Field::Usage,
        "error" => Field::Error,
        "item" => Field::Item,
        "message" => Field::Message,
        "input_tokens" => Field::InputTokens,
        "cached_input_tokens" => Field::CachedTokens,
        "output_tokens" => Field::OutputTokens,
        "reasoning_output_tokens" => Field::ReasoningTokens,
        "cache_write_input_tokens" => Field::CacheWriteTokens,
        "text" => Field::Text,
        "command" => Field::Command,
        "aggregated_output" => Field::Output,
        "exit_code" => Field::ExitCode,
        "status" => Field::Status,
        "changes" => Field::Changes,
        "path" => Field::Path,
        "kind" => Field::Kind,
        "server" => Field::Server,
        "tool" => Field::Tool,
        "arguments" => Field::Arguments,
        "result" => Field::Result,
        "content" => Field::Content,
        "_meta" => Field::Meta,
        "structured_content" => Field::StructuredContent,
        "sender_thread_id" => Field::Sender,
        "receiver_thread_ids" => Field::Receivers,
        "prompt" => Field::Prompt,
        "agents_states" => Field::States,
        "query" => Field::Query,
        "action" => Field::Action,
        "results" => Field::Results,
        "items" => Field::Items,
        "completed" => Field::Completed,
        "queries" => Field::Queries,
        "url" => Field::Url,
        "pattern" => Field::Pattern,
        _ => Field::Extra,
    }
}
#[derive(Clone, Copy)]
struct Slots<'de> {
    values: [Option<&'de RawValue>; FIELDS],
    counts: [u8; FIELDS],
    ids: [Option<&'de RawValue>; 2],
    id_positions: [usize; 2],
    type_position: usize,
}
impl Slots<'_> {
    fn empty() -> Self {
        Self {
            values: [None; FIELDS],
            counts: [0; FIELDS],
            ids: [None; 2],
            id_positions: [0; 2],
            type_position: 0,
        }
    }
}
impl<'de> Slots<'de> {
    fn required(&self, field: Field) -> Result<&'de RawValue, Cause> {
        self.values[field as usize].ok_or(Cause::Policy("required field"))
    }
    fn optional(&self, field: Field) -> Option<&'de RawValue> {
        self.values[field as usize].filter(|raw| raw.get() != "null")
    }
}
struct Record<'s, 'n, 'de>(&'s mut TransientState<'n, 'de>);
impl<'de> DeserializeSeed<'de> for Record<'_, '_, 'de> {
    type Value = Slots<'de>;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Record<'_, '_, 'de> {
    type Value = Slots<'de>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut slots = Slots::empty();
        let mut position = 0;
        while let Some(key) = map.next_key_seed(StringSeed {
            state: self.0,
            callback: |value: &str| Ok(field(value)),
        })? {
            let raw = map.next_value::<&RawValue>()?;
            self.0.check().map_err(|cause| self.0.bridge(cause))?;
            if key != Field::Extra {
                let index = key as usize;
                let count = slots.counts[index];
                slots.counts[index] = count.saturating_add(1).min(3);
                if count == 0 {
                    slots.values[index] = Some(raw);
                }
                if key == Field::Id && count < 2 {
                    slots.ids[count as usize] = Some(raw);
                    slots.id_positions[count as usize] = position;
                }
                if key == Field::Type && count == 0 {
                    slots.type_position = position;
                }
            }
            position += 1;
        }
        Ok(slots)
    }
}
fn record<'de>(
    raw: &'de RawValue,
    state: &mut TransientState<'_, 'de>,
) -> Result<Slots<'de>, Cause> {
    state.check()?;
    if !raw.get().starts_with('{') {
        return Err(Cause::Policy("object shape"));
    }
    let result = {
        let mut deserializer = Deserializer::from_str(raw.get());
        let value = Record(state).deserialize(&mut deserializer);
        value.and_then(|slots| deserializer.end().map(|()| slots))
    };
    state.finish(result)
}
struct Array<'s, 'n, 'de>(&'s mut TransientState<'n, 'de>);
impl<'de> DeserializeSeed<'de> for Array<'_, '_, 'de> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for Array<'_, '_, 'de> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("array")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut array: A) -> Result<(), A::Error> {
        while let Some(raw) = array.next_element::<&RawValue>()? {
            self.0
                .push(raw, 0, None, false)
                .map_err(|cause| self.0.bridge(cause))?;
        }
        Ok(())
    }
}
fn array<'de>(raw: &'de RawValue, state: &mut TransientState<'_, 'de>) -> Result<Range, Cause> {
    state.check()?;
    if !raw.get().starts_with('[') {
        return Err(Cause::Policy("array shape"));
    }
    let start = state.queue.len();
    let result = {
        let mut deserializer = Deserializer::from_str(raw.get());
        let value = Array(state).deserialize(&mut deserializer);
        value.and_then(|()| deserializer.end())
    };
    state.finish(result)?;
    Ok(Range {
        start,
        len: state.queue.len() - start,
    })
}
struct Agents<'s, 'n, 'de, 'a> {
    state: &'s mut TransientState<'n, 'de>,
    arena: Option<&'a mut Arena>,
}
impl<'de> DeserializeSeed<'de> for Agents<'_, '_, 'de, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Agents<'_, '_, 'de, '_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("agents map")
    }
    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<(), A::Error> {
        let limit = self.state.limits.id_bytes;
        while let Some(key) = map.next_key_seed(StringSeed {
            state: self.state,
            callback: |value: &str| {
                limited(value.len(), limit, "id bytes")?;
                self.arena
                    .as_mut()
                    .map(|arena| arena.copy(value.as_bytes()))
                    .transpose()
            },
        })? {
            let raw = map.next_value::<&RawValue>()?;
            self.state
                .push(raw, 0, key, false)
                .map_err(|cause| self.state.bridge(cause))?;
        }
        Ok(())
    }
}
fn agents<'de>(
    raw: &'de RawValue,
    state: &mut TransientState<'_, 'de>,
    arena: Option<&mut Arena>,
) -> Result<Range, Cause> {
    state.check()?;
    if !raw.get().starts_with('{') {
        return Err(Cause::Policy("agents map shape"));
    }
    let start = state.queue.len();
    let result = {
        let mut deserializer = Deserializer::from_str(raw.get());
        let value = Agents { state, arena }.deserialize(&mut deserializer);
        value.and_then(|()| deserializer.end())
    };
    state.finish(result)?;
    Ok(Range {
        start,
        len: state.queue.len() - start,
    })
}
fn integer64<'de>(raw: &'de RawValue, state: &mut TransientState<'_, 'de>) -> Result<i64, Cause> {
    state.check()?;
    raw.get().parse::<i64>().map_err(Cause::Integer)?;
    let result = {
        let mut deserializer = Deserializer::from_str(raw.get());
        let value = i64::deserialize(&mut deserializer);
        value.and_then(|value| deserializer.end().map(|()| value))
    };
    state.finish(result)
}
fn integer32<'de>(raw: &'de RawValue, state: &mut TransientState<'_, 'de>) -> Result<i32, Cause> {
    state.check()?;
    raw.get().parse::<i32>().map_err(Cause::Integer)?;
    let result = {
        let mut deserializer = Deserializer::from_str(raw.get());
        let value = i32::deserialize(&mut deserializer);
        value.and_then(|value| deserializer.end().map(|()| value))
    };
    state.finish(result)
}
fn boolean(raw: &RawValue) -> Result<bool, Cause> {
    match raw.get() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(Cause::Policy("boolean shape")),
    }
}
#[derive(Clone, Copy)]
enum Rule {
    String(bool),
    I64,
    I32,
    Bool,
    Raw,
    Enum(&'static [&'static str]),
    Nullable(&'static Rule),
    Array(&'static Rule),
    Record(&'static [FieldRule]),
    Agents,
    Action,
}
#[derive(Clone, Copy)]
struct FieldRule {
    field: Field,
    required: bool,
    rule: Rule,
}
macro_rules! rules { ($($field:ident $required:literal => $rule:expr),* $(,)?) => { &[$(FieldRule { field: Field::$field, required: $required, rule: $rule }),*] }; }
const STRING: Rule = Rule::String(false);
const ID: Rule = Rule::String(true);
const RAW: Rule = Rule::Raw;
const NULL_STRING: Rule = Rule::Nullable(&STRING);
const NULL_RAW: Rule = Rule::Nullable(&RAW);
const I32_RULE: Rule = Rule::I32;
const NULL_I32: Rule = Rule::Nullable(&I32_RULE);
const STRINGS: Rule = Rule::Array(&STRING);
const NULL_STRINGS: Rule = Rule::Nullable(&STRINGS);
const RAWS: Rule = Rule::Array(&RAW);
const NULL_RAWS: Rule = Rule::Nullable(&RAWS);
const STATUS: &[&str] = &["in_progress", "completed", "failed"];
const COMMAND_STATUS: &[&str] = &["in_progress", "completed", "failed", "declined"];
const AGENT_STATUS: &[&str] = &[
    "pending_init",
    "running",
    "interrupted",
    "completed",
    "errored",
    "shutdown",
    "not_found",
];
const COLLAB_TOOLS: &[&str] = &["spawn_agent", "send_input", "wait", "close_agent"];
const MESSAGE: &[FieldRule] = rules!(Message true => STRING);
const MESSAGE_RULE: Rule = Rule::Record(MESSAGE);
const NULL_MESSAGE: Rule = Rule::Nullable(&MESSAGE_RULE);
const USAGE: &[FieldRule] = rules!(InputTokens true => Rule::I64, CachedTokens true => Rule::I64, OutputTokens true => Rule::I64, ReasoningTokens true => Rule::I64, CacheWriteTokens false => Rule::I64);
const CHANGE: &[FieldRule] =
    rules!(Path true => STRING, Kind true => Rule::Enum(&["add", "delete", "update"]));
const CHANGE_RULE: Rule = Rule::Record(CHANGE);
const TODO: &[FieldRule] = rules!(Text true => STRING, Completed true => Rule::Bool);
const TODO_RULE: Rule = Rule::Record(TODO);
const AGENT: &[FieldRule] =
    rules!(Status true => Rule::Enum(AGENT_STATUS), Message false => NULL_STRING);
const MCP_RESULT: &[FieldRule] =
    rules!(Content true => RAWS, Meta false => NULL_RAW, StructuredContent false => NULL_RAW);
const MCP_RESULT_RULE: Rule = Rule::Record(MCP_RESULT);
const NULL_MCP_RESULT: Rule = Rule::Nullable(&MCP_RESULT_RULE);
#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Agent,
    Reasoning,
    Command,
    File,
    Mcp,
    Collab,
    Web,
    Todo,
    Error,
}
fn item_kind<'de>(
    slots: &Slots<'de>,
    state: &mut TransientState<'_, 'de>,
) -> Result<Option<ItemKind>, Cause> {
    discriminator(slots, state, |value| match value {
        "agent_message" => Some(ItemKind::Agent),
        "reasoning" => Some(ItemKind::Reasoning),
        "command_execution" => Some(ItemKind::Command),
        "file_change" => Some(ItemKind::File),
        "mcp_tool_call" => Some(ItemKind::Mcp),
        "collab_tool_call" => Some(ItemKind::Collab),
        "web_search" => Some(ItemKind::Web),
        "todo_list" => Some(ItemKind::Todo),
        "error" => Some(ItemKind::Error),
        _ => None,
    })
}
fn discriminator<'de, T>(
    slots: &Slots<'de>,
    state: &mut TransientState<'_, 'de>,
    classify: impl Fn(&str) -> T,
) -> Result<T, Cause> {
    if slots.counts[Field::Type as usize] != 1 {
        return Err(Cause::Policy("type field"));
    }
    string_value(slots.required(Field::Type)?, state, |value| {
        Ok(classify(value))
    })
}
fn event_kind<'de>(
    slots: &Slots<'de>,
    state: &mut TransientState<'_, 'de>,
) -> Result<Option<CodexEventKind>, Cause> {
    discriminator(slots, state, |value| match value {
        "thread.started" => Some(CodexEventKind::ThreadStarted),
        "turn.started" => Some(CodexEventKind::TurnStarted),
        "turn.completed" => Some(CodexEventKind::TurnCompleted),
        "turn.failed" => Some(CodexEventKind::TurnFailed),
        "item.started" => Some(CodexEventKind::ItemStarted),
        "item.updated" => Some(CodexEventKind::ItemUpdated),
        "item.completed" => Some(CodexEventKind::ItemCompleted),
        "error" => Some(CodexEventKind::Error),
        _ => None,
    })
}
fn item_rules(kind: ItemKind) -> &'static [FieldRule] {
    match kind {
        ItemKind::Agent | ItemKind::Reasoning => {
            rules!(Type true => STRING, Id true => ID, Text true => STRING)
        }
        ItemKind::Command => {
            rules!(Type true => STRING, Id true => ID, Command true => STRING, Output true => STRING, ExitCode false => NULL_I32, Status true => Rule::Enum(COMMAND_STATUS))
        }
        ItemKind::File => {
            rules!(Type true => STRING, Id true => ID, Changes true => Rule::Array(&CHANGE_RULE), Status true => Rule::Enum(STATUS))
        }
        ItemKind::Mcp => {
            rules!(Type true => STRING, Id true => ID, Server true => STRING, Tool true => STRING, Arguments false => RAW, Result false => NULL_MCP_RESULT, Error false => NULL_MESSAGE, Status true => Rule::Enum(STATUS))
        }
        ItemKind::Collab => {
            rules!(Type true => STRING, Id true => ID, Tool true => Rule::Enum(COLLAB_TOOLS), Sender true => ID, Receivers true => Rule::Array(&ID), Prompt false => NULL_STRING, States true => Rule::Agents, Status true => Rule::Enum(STATUS))
        }
        ItemKind::Web => {
            rules!(Type true => STRING, Id true => ID, Query true => STRING, Action true => Rule::Action, Results false => NULL_RAWS)
        }
        ItemKind::Todo => {
            rules!(Type true => STRING, Id true => ID, Items true => Rule::Array(&TODO_RULE))
        }
        ItemKind::Error => rules!(Type true => STRING, Id true => ID, Message true => STRING),
    }
}
fn validate_record<'de>(
    slots: &Slots<'de>,
    schema: &[FieldRule],
    state: &mut TransientState<'_, 'de>,
    web: bool,
) -> Result<(), Cause> {
    for spec in schema {
        let count = slots.counts[spec.field as usize];
        if spec.field == Field::Id && web {
            if count != 2
                || !(slots.id_positions[0] < slots.type_position
                    && slots.type_position < slots.id_positions[1])
            {
                return Err(Cause::Policy("web id ordering"));
            }
            validate_value(
                slots.ids[1].ok_or(Cause::Policy("web inner id"))?,
                ID,
                state,
            )?;
        } else if count > 1 {
            return Err(Cause::Policy("known duplicate field"));
        }
        if let Some(raw) = slots.values[spec.field as usize] {
            validate_value(raw, spec.rule, state)?;
        } else if spec.required {
            return Err(Cause::Policy("required field"));
        }
    }
    Ok(())
}
fn validate_value<'de>(
    raw: &'de RawValue,
    rule: Rule,
    state: &mut TransientState<'_, 'de>,
) -> Result<(), Cause> {
    state.check()?;
    match rule {
        Rule::String(id) => {
            let limit = state.limits.id_bytes;
            string_value(raw, state, |value| {
                if id {
                    limited(value.len(), limit, "id bytes")?;
                }
                Ok(())
            })?;
        }
        Rule::I64 => {
            integer64(raw, state)?;
        }
        Rule::I32 => {
            integer32(raw, state)?;
        }
        Rule::Bool => {
            boolean(raw)?;
        }
        Rule::Raw => {}
        Rule::Enum(allowed) => {
            string_value(raw, state, |value| {
                if allowed.contains(&value) {
                    Ok(())
                } else {
                    Err(Cause::Policy("enum value"))
                }
            })?;
        }
        Rule::Nullable(inner) => {
            if raw.get() != "null" {
                validate_value(raw, *inner, state)?;
            }
        }
        Rule::Array(inner) => {
            let range = array(raw, state)?;
            for i in range.start..range.start + range.len {
                let raw = state.queue[i].raw;
                validate_value(raw, *inner, state)?;
            }
        }
        Rule::Record(schema) => {
            let slots = record(raw, state)?;
            validate_record(&slots, schema, state, false)?;
        }
        Rule::Agents => {
            let range = agents(raw, state, None)?;
            for i in range.start..range.start + range.len {
                let raw = state.queue[i].raw;
                let slots = record(raw, state)?;
                validate_record(&slots, AGENT, state, false)?;
            }
        }
        Rule::Action => {
            let slots = record(raw, state)?;
            let kind = discriminator(&slots, state, action_kind)?;
            validate_record(&slots, action_rules(kind), state, false)?;
        }
    }
    Ok(())
}
#[derive(Clone, Copy)]
enum ActionKind {
    Search,
    Open,
    Find,
    Other,
}
fn action_kind(value: &str) -> ActionKind {
    match value {
        "search" => ActionKind::Search,
        "open_page" => ActionKind::Open,
        "find_in_page" => ActionKind::Find,
        _ => ActionKind::Other,
    }
}
fn action_rules(kind: ActionKind) -> &'static [FieldRule] {
    match kind {
        ActionKind::Search => {
            rules!(Type true => STRING, Query false => NULL_STRING, Queries false => NULL_STRINGS)
        }
        ActionKind::Open => rules!(Type true => STRING, Url false => NULL_STRING),
        ActionKind::Find => {
            rules!(Type true => STRING, Url false => NULL_STRING, Pattern false => NULL_STRING)
        }
        ActionKind::Other => rules!(Type true => STRING),
    }
}
fn validate_event<'de>(
    kind: CodexEventKind,
    slots: &Slots<'de>,
    state: &mut TransientState<'_, 'de>,
) -> Result<Option<ItemKind>, Cause> {
    let schema: &[FieldRule] = match kind {
        CodexEventKind::ThreadStarted => rules!(Type true => STRING, ThreadId true => ID),
        CodexEventKind::TurnStarted => rules!(Type true => STRING),
        CodexEventKind::TurnCompleted => {
            rules!(Type true => STRING, Usage true => Rule::Record(USAGE))
        }
        CodexEventKind::TurnFailed => rules!(Type true => STRING, Error true => MESSAGE_RULE),
        CodexEventKind::Error => rules!(Type true => STRING, Message true => STRING),
        _ => rules!(Type true => STRING, Item true => RAW),
    };
    validate_record(slots, schema, state, false)?;
    match kind {
        CodexEventKind::ItemStarted
        | CodexEventKind::ItemUpdated
        | CodexEventKind::ItemCompleted => {
            let item = record(slots.required(Field::Item)?, state)?;
            let kind = item_kind(&item, state)?;
            if let Some(kind) = kind {
                validate_record(&item, item_rules(kind), state, kind == ItemKind::Web)?;
            }
            Ok(kind)
        }
        _ => Ok(None),
    }
}

fn copy_string<'de>(
    raw: &'de RawValue,
    state: &mut TransientState<'_, 'de>,
    arena: &mut Arena,
    id: bool,
) -> Result<Span, Cause> {
    let limit = state.limits.id_bytes;
    string_value(raw, state, |value| {
        if id {
            limited(value.len(), limit, "id bytes")?;
        }
        arena.copy(value.as_bytes())
    })
}
fn copy_field<'de>(
    slots: &Slots<'de>,
    field: Field,
    state: &mut TransientState<'_, 'de>,
    arena: &mut Arena,
    id: bool,
) -> Result<Span, Cause> {
    copy_string(slots.required(field)?, state, arena, id)
}
fn copy_optional<'de>(
    slots: &Slots<'de>,
    field: Field,
    state: &mut TransientState<'_, 'de>,
    arena: &mut Arena,
) -> Result<Option<Span>, Cause> {
    slots
        .optional(field)
        .map(|raw| copy_string(raw, state, arena, false))
        .transpose()
}
fn copy_raw(raw: &RawValue, arena: &mut Arena) -> Result<Span, Cause> {
    arena.copy(raw.get().as_bytes())
}
fn copy_raw_optional(
    slots: &Slots<'_>,
    field: Field,
    arena: &mut Arena,
) -> Result<Option<Span>, Cause> {
    slots
        .optional(field)
        .map(|raw| copy_raw(raw, arena))
        .transpose()
}
fn strings<'de>(
    raw: &'de RawValue,
    state: &mut TransientState<'_, 'de>,
    arena: &mut Arena,
    id: bool,
) -> Result<Range, Cause> {
    let pending = array(raw, state)?;
    let start = arena.entries.len();
    for index in pending.start..pending.start + pending.len {
        let raw = state.queue[index].raw;
        let span = copy_string(raw, state, arena, id)?;
        arena.push(Entry::String(span))?;
    }
    Ok(Range {
        start,
        len: pending.len,
    })
}
fn raw_list<'de>(
    raw: &'de RawValue,
    state: &mut TransientState<'_, 'de>,
    arena: &mut Arena,
) -> Result<Range, Cause> {
    let pending = array(raw, state)?;
    let start = arena.entries.len();
    for index in pending.start..pending.start + pending.len {
        state.check()?;
        let span = copy_raw(state.queue[index].raw, arena)?;
        arena.push(Entry::Raw(span))?;
    }
    Ok(Range {
        start,
        len: pending.len,
    })
}
fn status<'de>(
    slots: &Slots<'de>,
    state: &mut TransientState<'_, 'de>,
) -> Result<CodexStatus, Cause> {
    string_value(slots.required(Field::Status)?, state, |value| match value {
        "in_progress" => Ok(CodexStatus::InProgress),
        "completed" => Ok(CodexStatus::Completed),
        "failed" => Ok(CodexStatus::Failed),
        "declined" => Ok(CodexStatus::Declined),
        _ => Err(Cause::Policy("status")),
    })
}
fn agent_status<'de>(
    slots: &Slots<'de>,
    state: &mut TransientState<'_, 'de>,
) -> Result<CodexAgentStatus, Cause> {
    string_value(slots.required(Field::Status)?, state, |value| match value {
        "pending_init" => Ok(CodexAgentStatus::PendingInit),
        "running" => Ok(CodexAgentStatus::Running),
        "interrupted" => Ok(CodexAgentStatus::Interrupted),
        "completed" => Ok(CodexAgentStatus::Completed),
        "errored" => Ok(CodexAgentStatus::Errored),
        "shutdown" => Ok(CodexAgentStatus::Shutdown),
        "not_found" => Ok(CodexAgentStatus::NotFound),
        _ => Err(Cause::Policy("agent status")),
    })
}
fn collab_tool<'de>(
    slots: &Slots<'de>,
    state: &mut TransientState<'_, 'de>,
) -> Result<CodexCollabTool, Cause> {
    string_value(slots.required(Field::Tool)?, state, |value| match value {
        "spawn_agent" => Ok(CodexCollabTool::SpawnAgent),
        "send_input" => Ok(CodexCollabTool::SendInput),
        "wait" => Ok(CodexCollabTool::Wait),
        "close_agent" => Ok(CodexCollabTool::CloseAgent),
        _ => Err(Cause::Policy("collab tool")),
    })
}
fn build_action<'de>(
    raw: &'de RawValue,
    state: &mut TransientState<'_, 'de>,
    arena: &mut Arena,
) -> Result<WebAction, Cause> {
    let slots = record(raw, state)?;
    let kind = discriminator(&slots, state, action_kind)?;
    match kind {
        ActionKind::Search => Ok(WebAction::Search(
            copy_optional(&slots, Field::Query, state, arena)?,
            slots
                .optional(Field::Queries)
                .map(|raw| strings(raw, state, arena, false))
                .transpose()?,
        )),
        ActionKind::Open => Ok(WebAction::OpenPage(copy_optional(
            &slots,
            Field::Url,
            state,
            arena,
        )?)),
        ActionKind::Find => Ok(WebAction::FindInPage(
            copy_optional(&slots, Field::Url, state, arena)?,
            copy_optional(&slots, Field::Pattern, state, arena)?,
        )),
        ActionKind::Other => Ok(WebAction::Other),
    }
}
fn build_states<'de>(
    raw: &'de RawValue,
    state: &mut TransientState<'_, 'de>,
    arena: &mut Arena,
) -> Result<Range, Cause> {
    let pending = agents(raw, state, Some(arena))?;
    let start = arena.entries.len();
    for index in pending.start..pending.start + pending.len {
        let entry = state.queue[index];
        let key = entry.key.ok_or(Cause::Policy("agent key span"))?;
        let slots = record(entry.raw, state)?;
        let status = agent_status(&slots, state)?;
        let message = copy_optional(&slots, Field::Message, state, arena)?;
        let mut previous = None;
        for position in start..arena.entries.len() {
            state.check()?;
            if let Entry::Agent(old_key, ..) = arena.entries[position]
                && text(&arena.bytes, old_key) == text(&arena.bytes, key)
            {
                previous = Some(position);
                break;
            }
        }
        let entry = Entry::Agent(key, status, message);
        if let Some(position) = previous {
            arena.entries[position] = entry;
        } else {
            arena.push(entry)?;
        }
    }
    Ok(Range {
        start,
        len: arena.entries.len() - start,
    })
}
fn build_body<'de>(
    kind: ItemKind,
    slots: &Slots<'de>,
    state: &mut TransientState<'_, 'de>,
    arena: &mut Arena,
) -> Result<Body, Cause> {
    match kind {
        ItemKind::Agent => Ok(Body::AgentMessage(copy_field(
            slots,
            Field::Text,
            state,
            arena,
            false,
        )?)),
        ItemKind::Reasoning => Ok(Body::Reasoning(copy_field(
            slots,
            Field::Text,
            state,
            arena,
            false,
        )?)),
        ItemKind::Command => Ok(Body::Command(
            copy_field(slots, Field::Command, state, arena, false)?,
            copy_field(slots, Field::Output, state, arena, false)?,
            slots
                .optional(Field::ExitCode)
                .map(|raw| integer32(raw, state))
                .transpose()?,
            status(slots, state)?,
        )),
        ItemKind::File => {
            let pending = array(slots.required(Field::Changes)?, state)?;
            let start = arena.entries.len();
            for index in pending.start..pending.start + pending.len {
                let raw = state.queue[index].raw;
                let change = record(raw, state)?;
                let path = copy_field(&change, Field::Path, state, arena, false)?;
                let kind =
                    string_value(change.required(Field::Kind)?, state, |value| match value {
                        "add" => Ok(CodexChangeKind::Add),
                        "delete" => Ok(CodexChangeKind::Delete),
                        "update" => Ok(CodexChangeKind::Update),
                        _ => Err(Cause::Policy("change kind")),
                    })?;
                arena.push(Entry::Change(path, kind))?;
            }
            Ok(Body::File(
                Range {
                    start,
                    len: pending.len,
                },
                status(slots, state)?,
            ))
        }
        ItemKind::Mcp => {
            let server = copy_field(slots, Field::Server, state, arena, false)?;
            let tool = copy_field(slots, Field::Tool, state, arena, false)?;
            let arguments = slots.values[Field::Arguments as usize]
                .map(|raw| copy_raw(raw, arena))
                .transpose()?;
            let result = if let Some(raw) = slots.optional(Field::Result) {
                let result = record(raw, state)?;
                Some(McpResult {
                    content: raw_list(result.required(Field::Content)?, state, arena)?,
                    meta: copy_raw_optional(&result, Field::Meta, arena)?,
                    structured_content: copy_raw_optional(
                        &result,
                        Field::StructuredContent,
                        arena,
                    )?,
                })
            } else {
                None
            };
            let error = if let Some(raw) = slots.optional(Field::Error) {
                let error = record(raw, state)?;
                Some(copy_field(&error, Field::Message, state, arena, false)?)
            } else {
                None
            };
            Ok(Body::Mcp(
                server,
                tool,
                arguments,
                result,
                error,
                status(slots, state)?,
            ))
        }
        ItemKind::Collab => {
            let tool = collab_tool(slots, state)?;
            let sender = copy_field(slots, Field::Sender, state, arena, true)?;
            let receivers = strings(slots.required(Field::Receivers)?, state, arena, true)?;
            let prompt = copy_optional(slots, Field::Prompt, state, arena)?;
            let agents = build_states(slots.required(Field::States)?, state, arena)?;
            Ok(Body::Collab(
                tool,
                sender,
                receivers,
                prompt,
                agents,
                status(slots, state)?,
            ))
        }
        ItemKind::Web => {
            let id = copy_string(
                slots.ids[1].ok_or(Cause::Policy("web inner id"))?,
                state,
                arena,
                true,
            )?;
            let query = copy_field(slots, Field::Query, state, arena, false)?;
            let action = build_action(slots.required(Field::Action)?, state, arena)?;
            let results = slots
                .optional(Field::Results)
                .map(|raw| raw_list(raw, state, arena))
                .transpose()?;
            Ok(Body::Web(id, query, action, results))
        }
        ItemKind::Todo => {
            let pending = array(slots.required(Field::Items)?, state)?;
            let start = arena.entries.len();
            for index in pending.start..pending.start + pending.len {
                let raw = state.queue[index].raw;
                let todo = record(raw, state)?;
                let text = copy_field(&todo, Field::Text, state, arena, false)?;
                let completed = boolean(todo.required(Field::Completed)?)?;
                arena.push(Entry::Todo(text, completed))?;
            }
            Ok(Body::Todo(Range {
                start,
                len: pending.len,
            }))
        }
        ItemKind::Error => Ok(Body::Error(copy_field(
            slots,
            Field::Message,
            state,
            arena,
            false,
        )?)),
    }
}
fn build_event<'de>(
    kind: CodexEventKind,
    item_kind: Option<ItemKind>,
    slots: &Slots<'de>,
    state: &mut TransientState<'_, 'de>,
    arena: &mut Arena,
) -> Result<Event, Cause> {
    match kind {
        CodexEventKind::ThreadStarted => Ok(Event::Thread(copy_field(
            slots,
            Field::ThreadId,
            state,
            arena,
            true,
        )?)),
        CodexEventKind::TurnStarted => Ok(Event::Turn),
        CodexEventKind::TurnCompleted => {
            let usage = record(slots.required(Field::Usage)?, state)?;
            Ok(Event::Usage(CodexUsage {
                input_tokens: integer64(usage.required(Field::InputTokens)?, state)?,
                cached_input_tokens: integer64(usage.required(Field::CachedTokens)?, state)?,
                output_tokens: integer64(usage.required(Field::OutputTokens)?, state)?,
                reasoning_output_tokens: integer64(usage.required(Field::ReasoningTokens)?, state)?,
                cache_write_input_tokens: usage.values[Field::CacheWriteTokens as usize]
                    .map(|raw| integer64(raw, state))
                    .transpose()?
                    .unwrap_or(0),
            }))
        }
        CodexEventKind::TurnFailed => {
            let error = record(slots.required(Field::Error)?, state)?;
            Ok(Event::Failed(copy_field(
                &error,
                Field::Message,
                state,
                arena,
                false,
            )?))
        }
        CodexEventKind::Error => Ok(Event::Error(copy_field(
            slots,
            Field::Message,
            state,
            arena,
            false,
        )?)),
        CodexEventKind::ItemStarted
        | CodexEventKind::ItemUpdated
        | CodexEventKind::ItemCompleted => {
            let item = record(slots.required(Field::Item)?, state)?;
            let id = copy_string(
                item.ids[0].ok_or(Cause::Policy("outer item id"))?,
                state,
                arena,
                true,
            )?;
            let body = build_body(
                item_kind.ok_or(Cause::Policy("known item kind"))?,
                &item,
                state,
                arena,
            )?;
            let phase = match kind {
                CodexEventKind::ItemStarted => CodexItemPhase::Started,
                CodexEventKind::ItemUpdated => CodexItemPhase::Updated,
                _ => CodexItemPhase::Completed,
            };
            Ok(Event::Item(phase, id, body))
        }
    }
}

fn policy_queue(frame_len: usize, limits: &CodexWireLimits) -> Result<usize, Cause> {
    let positive = [
        limits.frame_bytes,
        limits.depth,
        limits.nodes,
        limits.fields,
        limits.array_items,
        limits.string_bytes,
        limits.decoded_string_bytes,
        limits.id_bytes,
    ];
    if positive.contains(&0) || limits.frame_bytes > 8 * 1024 * 1024 || limits.depth > 128 {
        return Err(Cause::Policy("positive bounded policy"));
    }
    limited(frame_len, limits.frame_bytes, "frame bytes")?;
    Ok(limits.nodes.min(add(frame_len, 1)?))
}
fn arena_bounds(frame_len: usize, stats: CodexWireStats) -> Result<(usize, usize), Cause> {
    Ok((
        add(stats.decoded_string_bytes, frame_len)?,
        add(stats.array_items, stats.fields)?,
    ))
}
fn wire_error(cause: Cause, stage: WireStage) -> CodexWireError {
    CodexWireError {
        stage,
        cause: SafeCause(cause),
    }
}
fn audited_frame<'n, 'de>(
    frame: &'de [u8],
    limits: &'n CodexWireLimits,
    original: &'n SnapshotContext,
) -> Result<(&'de RawValue, TransientState<'n, 'de>), CodexWireError> {
    original
        .check()
        .map_err(|error| wire_error(Cause::Io(error), WireStage::Policy))?;
    let queue_limit =
        policy_queue(frame.len(), limits).map_err(|cause| wire_error(cause, WireStage::Policy))?;
    let mut state = TransientState {
        original,
        limits,
        queue: Vec::new(),
        queue_limit,
        stats: CodexWireStats::default(),
        first: None,
        stage: WireStage::Depth,
    };
    let result = (|| {
        depth_gate(frame, &mut state)?;
        std::str::from_utf8(frame).map_err(Cause::Utf8)?;
        state.stage = WireStage::Grammar;
        state.check()?;
        let result = {
            let mut deserializer = Deserializer::from_slice(frame);
            let root = <&RawValue>::deserialize(&mut deserializer);
            root.and_then(|root| deserializer.end().map(|()| root))
        };
        let root = state.finish(result)?;
        state
            .queue
            .try_reserve_exact(queue_limit)
            .map_err(|_cause| Cause::Allocation { _cause })?;
        state.stage = WireStage::Audit;
        audit(root, &mut state)?;
        state.check()?;
        Ok(root)
    })();
    let stage = state.stage;
    let root = result.map_err(|cause| wire_error(cause, stage))?;
    Ok((root, state))
}

pub(crate) fn audit_json_frame(
    frame: &[u8],
    limits: &CodexWireLimits,
    original: &SnapshotContext,
) -> Result<CodexWireStats, CodexWireError> {
    let (_, state) = audited_frame(frame, limits, original)?;
    Ok(state.stats)
}

pub fn decode_codex_frame(
    frame: &[u8],
    limits: &CodexWireLimits,
    original: &SnapshotContext,
) -> Result<CodexDecodedFrame, CodexWireError> {
    let (root, mut state) = audited_frame(frame, limits, original)?;
    let result = (|| {
        state.stage = WireStage::Schema;
        let slots = record(root, &mut state)?;
        let Some(kind) = event_kind(&slots, &mut state)? else {
            state.stage = WireStage::Publish;
            state.check()?;
            return Ok(CodexDecodedFrame::Ignored(state.stats));
        };
        let item_kind = validate_event(kind, &slots, &mut state)?;
        if matches!(
            kind,
            CodexEventKind::ItemStarted
                | CodexEventKind::ItemUpdated
                | CodexEventKind::ItemCompleted
        ) && item_kind.is_none()
        {
            state.stage = WireStage::Publish;
            state.check()?;
            return Ok(CodexDecodedFrame::Ignored(state.stats));
        }
        state.queue.clear();
        state.stage = WireStage::Build;
        state.check()?;
        let (byte_limit, entry_limit) = arena_bounds(frame.len(), state.stats)?;
        let mut arena = Arena {
            bytes: Vec::new(),
            entries: Vec::new(),
            byte_limit,
            entry_limit,
        };
        state.check()?;
        arena
            .bytes
            .try_reserve_exact(byte_limit)
            .map_err(|_cause| Cause::Allocation { _cause })?;
        state.check()?;
        arena
            .entries
            .try_reserve_exact(entry_limit)
            .map_err(|_cause| Cause::Allocation { _cause })?;
        state.check()?;
        let event = build_event(kind, item_kind, &slots, &mut state, &mut arena)?;
        state.stage = WireStage::Publish;
        state.check()?;
        Ok(CodexDecodedFrame::Known(OwnedCodexEvent {
            event,
            arena,
            stats: state.stats,
        }))
    })();
    let stage = state.stage;
    result.map_err(|cause| wire_error(cause, stage))
}

#[cfg(test)]
#[path = "cli_codex_wire_tests.rs"]
mod tests;
