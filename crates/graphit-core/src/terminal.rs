//! Bounded presentation to a caller-owned sink. No terminal detection or workers.
//!
//! Semantic text escapes C0/C1 controls; `Data` and `write_data` are explicit raw
//! channels. IO errors may follow a partial write. A caller drives task ticks and
//! chooses its own diagnostic/protocol destinations and terminal policy.
use anstyle::{AnsiColor, Style};
use std::{
    fmt::{self, Write as _},
    io::{self, Write},
    num::NonZeroU16,
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Plain,
    Silent,
    Terminal { columns: NonZeroU16, color: bool },
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_table_rows: usize,
    pub max_tasks: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub mode: Mode,
    pub limits: Limits,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Info,
    Success,
    Error,
    Warning,
    Running,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinishOutcome {
    Success,
    Failure,
    Cancelled,
}
/// Borrowed input. There is deliberately no `Debug` or `Serialize` payload dump.
pub enum Message<'a> {
    Line {
        severity: Severity,
        text: &'a str,
    },
    Step {
        severity: Option<Severity>,
        text: &'a str,
    },
    Detail {
        key: &'a str,
        value: &'a str,
    },
    KeyValue {
        key: &'a str,
        value: &'a str,
    },
    Count {
        amount: u64,
        singular: &'a str,
        plural: &'a str,
    },
    Bullet(&'a str),
    Header(&'a str),
    Blank,
    Data(&'a str),
    Table {
        headers: [&'a str; 2],
        rows: &'a [[&'a str; 2]],
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalError {
    InvalidLimits,
    InputBytes,
    OutputBytes,
    TableRows,
    TaskLimit,
    Allocation,
    Poisoned,
    Io {
        kind: io::ErrorKind,
        os_code: Option<i32>,
    },
}
impl fmt::Display for TerminalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "invalid terminal limits",
            Self::InputBytes => "terminal input budget exceeded",
            Self::OutputBytes => "terminal output budget exceeded",
            Self::TableRows => "terminal row budget exceeded",
            Self::TaskLimit => "terminal task budget exceeded",
            Self::Allocation => "terminal allocation failed",
            Self::Poisoned => "terminal state poisoned",
            Self::Io { .. } => "terminal sink failed",
        })
    }
}
impl std::error::Error for TerminalError {}
impl From<io::Error> for TerminalError {
    fn from(error: io::Error) -> Self {
        Self::Io {
            kind: error.kind(),
            os_code: error.raw_os_error(),
        }
    }
}
struct Buffer {
    bytes: Vec<u8>,
    cap: usize,
    error: Option<TerminalError>,
}
impl Buffer {
    fn new(cap: usize) -> Self {
        Self {
            bytes: Vec::new(),
            cap,
            error: None,
        }
    }
    fn reset(&mut self) {
        self.bytes.clear();
        self.error = None;
    }
    fn add(&mut self, value: &str) -> Result<(), TerminalError> {
        let len = self
            .bytes
            .len()
            .checked_add(value.len())
            .ok_or(TerminalError::OutputBytes)?;
        self.reserve(len)?;
        self.bytes.extend_from_slice(value.as_bytes());
        Ok(())
    }
    fn reserve(&mut self, len: usize) -> Result<(), TerminalError> {
        if len > self.cap {
            return Err(TerminalError::OutputBytes);
        }
        if len > self.bytes.capacity() {
            let capacity = len
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.cap);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(|_| TerminalError::Allocation)?;
        }
        Ok(())
    }
    fn format(&mut self, args: fmt::Arguments<'_>) -> Result<(), TerminalError> {
        self.write_fmt(args)
            .map_err(|_| self.error.take().unwrap_or(TerminalError::OutputBytes))
    }
    fn safe(&mut self, value: &str) -> Result<(), TerminalError> {
        let mut start = 0;
        for (index, ch) in value.char_indices() {
            if ch.is_control() {
                self.add(&value[start..index])?;
                self.format(format_args!("\\u{{{:x}}}", u32::from(ch)))?;
                start = index + ch.len_utf8();
            }
        }
        self.add(&value[start..])
    }
    fn styled(&mut self, style: Style, value: &str, color: bool) -> Result<(), TerminalError> {
        if color {
            self.format(format_args!("{style}"))?;
        }
        self.safe(value)?;
        if color {
            self.format(format_args!("{style:#}"))?;
        }
        Ok(())
    }
    fn spaces(&mut self, count: usize) -> Result<(), TerminalError> {
        for _ in 0..count {
            self.add(" ")?;
        }
        Ok(())
    }
}
impl fmt::Write for Buffer {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.add(value).map_err(|error| {
            self.error = Some(error);
            fmt::Error
        })
    }
}
fn copy_text(value: &str) -> Result<String, TerminalError> {
    let mut text = String::new();
    text.try_reserve_exact(value.len())
        .map_err(|_| TerminalError::Allocation)?;
    text.push_str(value);
    Ok(text)
}
fn input_size<'a>(
    values: impl IntoIterator<Item = &'a str>,
    cap: usize,
) -> Result<(), TerminalError> {
    let mut total = 0usize;
    for value in values {
        total = total
            .checked_add(value.len())
            .ok_or(TerminalError::InputBytes)?;
        if total > cap {
            return Err(TerminalError::InputBytes);
        }
    }
    Ok(())
}
fn safe_width(value: &str) -> usize {
    // Control escapes have 5-6 ASCII cells. Other spans use whole-string width,
    // retaining Unicode's ligatures, combining characters and emoji context.
    let mut width = 0usize;
    let mut start = 0;
    for (index, ch) in value.char_indices() {
        if ch.is_control() {
            width = width.saturating_add(value[start..index].width());
            width = width.saturating_add(if u32::from(ch) < 16 { 5 } else { 6 });
            start = index + ch.len_utf8();
        }
    }
    width.saturating_add(value[start..].width())
}
const DIM: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::BrightBlack)));
const BOLD: Style = Style::new().bold();
const VALUE: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Magenta)));
fn status(severity: Severity) -> (&'static str, Style) {
    match severity {
        Severity::Info => ("", Style::new()),
        Severity::Success => (
            "✓",
            Style::new().fg_color(Some(AnsiColor::Green.into())).bold(),
        ),
        Severity::Error => (
            "✗",
            Style::new().fg_color(Some(AnsiColor::Red.into())).bold(),
        ),
        Severity::Warning => (
            "!",
            Style::new().fg_color(Some(AnsiColor::Yellow.into())).bold(),
        ),
        Severity::Running => ("◦", Style::new().fg_color(Some(AnsiColor::Cyan.into()))),
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Owner {
    Direct,
    Task(u64),
}
struct State<W> {
    writer: W,
    prefix: String,
    options: Options,
    buffer: Buffer,
    frame: Buffer,
    visible: Option<Owner>,
    stream_open: bool,
    tasks: usize,
    next_id: u64,
}
impl<W: Write> State<W> {
    fn color(&self) -> bool {
        matches!(self.options.mode, Mode::Terminal { color: true, .. })
    }
    fn validate_text(&self, text: &str) -> Result<(), TerminalError> {
        input_size(
            [self.prefix.as_str(), text],
            self.options.limits.max_input_bytes,
        )
    }
    fn begin(&mut self) -> Result<(), TerminalError> {
        self.buffer.reset();
        if self.stream_open {
            self.buffer.add("\n")?;
        }
        if self.visible.is_some() {
            self.buffer.add("\r\x1b[2K")?;
        }
        Ok(())
    }
    fn tag(&mut self) -> Result<(), TerminalError> {
        if !self.prefix.is_empty() {
            if self.color() {
                self.buffer.format(format_args!("{DIM}"))?;
            }
            self.buffer.add("[")?;
            self.buffer.safe(&self.prefix)?;
            self.buffer.add("]")?;
            if self.color() {
                self.buffer.format(format_args!("{DIM:#}"))?;
            }
            self.buffer.add(" ")?;
        }
        Ok(())
    }
    fn line(&mut self, severity: Severity, text: &str) -> Result<(), TerminalError> {
        self.tag()?;
        let (symbol, style) = status(severity);
        if !symbol.is_empty() {
            self.buffer.styled(style, symbol, self.color())?;
            self.buffer.add(" ")?;
        }
        self.buffer.styled(style, text, self.color())?;
        self.buffer.add("\n")
    }
    fn commit(&mut self, visible: Option<Owner>, stream_open: bool) -> Result<(), TerminalError> {
        // On a partial frame failure remember its owner so cleanup remains possible.
        if visible.is_some() {
            self.visible = visible;
        }
        if stream_open {
            self.stream_open = true;
        }
        if !self.buffer.bytes.is_empty() {
            if let Err(error) = self.writer.write_all(&self.buffer.bytes) {
                // A terminated line may have failed before its final newline.
                // Open a boundary conservatively on the next operation.
                if visible.is_none() {
                    self.stream_open = true;
                }
                return Err(TerminalError::from(error));
            }
            // A flush failure does not make already-written bytes ambiguous.
            self.visible = visible;
            self.stream_open = stream_open;
            self.writer.flush().map_err(TerminalError::from)?;
        }
        self.visible = visible;
        self.stream_open = stream_open;
        Ok(())
    }
    fn render_progress(&mut self, text: &str, symbol: &str) -> Result<(), TerminalError> {
        self.begin()?;
        match self.options.mode {
            Mode::Plain => self.line(Severity::Running, text),
            Mode::Silent => Ok(()),
            Mode::Terminal { columns, .. } => {
                self.frame.reset();
                if !self.prefix.is_empty() {
                    self.frame.add("[")?;
                    self.frame.safe(&self.prefix)?;
                    self.frame.add("] ")?;
                }
                let prefix_end = self.frame.bytes.len();
                self.frame.add(symbol)?;
                self.frame.add(" ")?;
                self.frame.safe(text)?;
                let frame = std::str::from_utf8(&self.frame.bytes)
                    .map_err(|_| TerminalError::Allocation)?;
                let available = usize::from(columns.get() - 1);
                let clipped = frame.width() > available;
                let cells = available.saturating_sub(usize::from(clipped));
                let mut end = 0;
                if !clipped {
                    end = frame.len();
                } else if available > 0 {
                    let mut used = 0usize;
                    for (index, grapheme) in frame.grapheme_indices(true) {
                        let next = index + grapheme.len();
                        used = used.saturating_add(grapheme.width());
                        if used > cells {
                            break;
                        }
                        end = next;
                    }
                }
                let (_, style) = status(Severity::Running);
                let tag_end = prefix_end.min(end);
                if self.color() && tag_end > 0 {
                    self.buffer.format(format_args!("{DIM}"))?;
                }
                self.buffer.add(&frame[..tag_end])?;
                if self.color() && tag_end > 0 {
                    self.buffer.format(format_args!("{DIM:#}"))?;
                }
                if self.color() {
                    self.buffer.format(format_args!("{style}"))?;
                }
                self.buffer.add(&frame[tag_end..end])?;
                if clipped && available > 0 {
                    self.buffer.add("…")?;
                }
                if self.color() {
                    self.buffer.format(format_args!("{style:#}"))?;
                }
                // A width-one terminal still owns a transient (empty) line.
                Ok(())
            }
        }
    }
    fn progress_owner(&self, owner: Owner) -> Option<Owner> {
        matches!(self.options.mode, Mode::Terminal { .. }).then_some(owner)
    }
    fn render_message(&mut self, message: Message<'_>) -> Result<(), TerminalError> {
        self.begin()?;
        let color = self.color();
        match message {
            Message::Line { severity, text } => self.line(severity, text)?,
            Message::Step { severity, text } => {
                self.tag()?;
                self.buffer.styled(DIM, "› ", color)?;
                self.buffer
                    .styled(severity.map(|s| status(s).1).unwrap_or(DIM), text, color)?;
                self.buffer.add("\n")?;
            }
            Message::Detail { key, value } | Message::KeyValue { key, value } => {
                self.tag()?;
                if matches!(message, Message::Detail { .. }) {
                    self.buffer.add("  ")?;
                }
                self.buffer.styled(DIM, key, color)?;
                self.buffer.add(": ")?;
                self.buffer.styled(VALUE, value, color)?;
                self.buffer.add("\n")?;
            }
            Message::Count {
                amount,
                singular,
                plural,
            } => {
                self.tag()?;
                self.buffer.format(format_args!("{amount} "))?;
                self.buffer
                    .safe(if amount == 1 { singular } else { plural })?;
                self.buffer.add("\n")?;
            }
            Message::Bullet(text) => {
                self.tag()?;
                self.buffer.add("• ")?;
                self.buffer.safe(text)?;
                self.buffer.add("\n")?;
            }
            Message::Header(text) => {
                self.buffer.add("\n")?;
                self.tag()?;
                self.buffer.styled(BOLD, text, color)?;
                self.buffer.add("\n\n")?;
            }
            Message::Blank => self.buffer.add("\n")?,
            Message::Data(text) => {
                self.buffer.add(text)?;
                self.buffer.add("\n")?;
            }
            Message::Table { headers, rows } => {
                let width = rows
                    .iter()
                    .map(|r| safe_width(r[0]))
                    .chain([safe_width(headers[0])])
                    .max()
                    .unwrap_or(0);
                self.table_row(headers, width, true)?;
                for row in rows {
                    self.table_row(*row, width, false)?;
                }
            }
        }
        Ok(())
    }
    fn table_row(
        &mut self,
        row: [&str; 2],
        width: usize,
        header: bool,
    ) -> Result<(), TerminalError> {
        self.tag()?;
        let color = self.color();
        let style = if header { BOLD } else { Style::new() };
        self.buffer.styled(style, row[0], color)?;
        self.buffer
            .spaces(width.saturating_sub(safe_width(row[0])))?;
        self.buffer.add("  ")?;
        self.buffer
            .styled(if header { BOLD } else { VALUE }, row[1], color)?;
        self.buffer.add("\n")
    }
    fn validate_message(&self, message: &Message<'_>) -> Result<(), TerminalError> {
        let cap = self.options.limits.max_input_bytes;
        let prefix = self.prefix.as_str();
        match message {
            Message::Line { text, .. }
            | Message::Step { text, .. }
            | Message::Bullet(text)
            | Message::Header(text) => input_size([prefix, text], cap),
            Message::Detail { key, value } | Message::KeyValue { key, value } => {
                input_size([prefix, key, value], cap)
            }
            Message::Count {
                singular, plural, ..
            } => input_size([prefix, singular, plural], cap),
            Message::Table { headers, rows } => {
                if rows.len() > self.options.limits.max_table_rows {
                    return Err(TerminalError::TableRows);
                }
                input_size(
                    std::iter::once(prefix)
                        .chain(headers.iter().copied())
                        .chain(rows.iter().flat_map(|row| row.iter().copied())),
                    cap,
                )
            }
            Message::Data(text) => input_size([*text], cap),
            Message::Blank => Ok(()),
        }
    }
}
/// Clones share one sink and operation lock. `Debug` never exposes the sink or text.
pub struct Printer<W: Write + Send> {
    state: Arc<Mutex<State<W>>>,
}
impl<W: Write + Send> Clone for Printer<W> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}
impl<W: Write + Send> fmt::Debug for Printer<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Printer { .. }")
    }
}
impl<W: Write + Send> Printer<W> {
    pub fn new(writer: W, prefix: &str, options: Options) -> Result<Self, TerminalError> {
        if options.limits.max_tasks == 0 {
            return Err(TerminalError::InvalidLimits);
        }
        input_size([prefix], options.limits.max_input_bytes)?;
        // Each input byte expands to at most six bytes when escaping controls.
        // Extra space covers tag delimiters, status and a spinner, without an
        // unbounded temporary format allocation. Allocator overhead is separate.
        let frame_cap = options
            .limits
            .max_input_bytes
            .checked_mul(6)
            .and_then(|n| n.checked_add(32))
            .ok_or(TerminalError::InvalidLimits)?;
        Ok(Self {
            state: Arc::new(Mutex::new(State {
                writer,
                prefix: copy_text(prefix)?,
                options,
                buffer: Buffer::new(options.limits.max_output_bytes),
                frame: Buffer::new(frame_cap),
                visible: None,
                stream_open: false,
                tasks: 0,
                next_id: 0,
            })),
        })
    }
    fn lock(&self) -> Result<MutexGuard<'_, State<W>>, TerminalError> {
        self.state.lock().map_err(|_| TerminalError::Poisoned)
    }
    pub fn resize(&self, columns: NonZeroU16) -> Result<(), TerminalError> {
        let mut state = self.lock()?;
        if let Mode::Terminal { color, .. } = state.options.mode {
            state.options.mode = Mode::Terminal { columns, color };
        }
        Ok(())
    }
    pub fn print(&self, message: Message<'_>) -> Result<(), TerminalError> {
        let mut state = self.lock()?;
        state.validate_message(&message)?;
        if state.options.mode == Mode::Silent {
            return Ok(());
        }
        state.render_message(message)?;
        state.commit(None, false)
    }
    pub fn write_data(&self, text: &str) -> Result<(), TerminalError> {
        let mut state = self.lock()?;
        input_size([text], state.options.limits.max_input_bytes)?;
        if state.options.mode == Mode::Silent {
            return Ok(());
        }
        // Raw chunks continue a raw stream; they must not insert a newline.
        state.buffer.reset();
        if state.visible.is_some() {
            state.buffer.add("\r\x1b[2K")?;
        }
        state.buffer.add(text)?;
        let stream_open = if text.is_empty() {
            state.stream_open
        } else {
            !text.ends_with('\n')
        };
        state.commit(None, stream_open)
    }
    pub fn progress(&self, text: &str) -> Result<(), TerminalError> {
        let mut state = self.lock()?;
        state.validate_text(text)?;
        if state.options.mode == Mode::Silent {
            return Ok(());
        }
        state.render_progress(text, "◦")?;
        let owner = state.progress_owner(Owner::Direct);
        state.commit(owner, false)
    }
    pub fn end_progress(&self) -> Result<(), TerminalError> {
        let mut state = self.lock()?;
        if state.visible.is_none() {
            return Ok(());
        }
        state.begin()?;
        state.commit(None, false)
    }
    pub fn start_task(&self, message: &str) -> Result<ProgressTask<W>, TerminalError> {
        let mut state = self.lock()?;
        state.validate_text(message)?;
        if state.tasks >= state.options.limits.max_tasks {
            return Err(TerminalError::TaskLimit);
        }
        let id = state
            .next_id
            .checked_add(1)
            .ok_or(TerminalError::TaskLimit)?;
        let message = copy_text(message)?;
        if state.options.mode != Mode::Silent {
            state.render_progress(&message, "◦")?;
        }
        // If sink IO fails no handle is published and the slot is not charged.
        if state.options.mode != Mode::Silent {
            let owner = state.progress_owner(Owner::Task(id));
            state.commit(owner, false)?;
        }
        state.tasks += 1;
        state.next_id = id;
        Ok(ProgressTask {
            inner: Arc::new(TaskInner {
                printer: self.clone(),
                state: Mutex::new(TaskState {
                    id,
                    message,
                    started: Instant::now(),
                    frame: 0,
                    finished: false,
                }),
            }),
        })
    }
}
struct TaskState {
    id: u64,
    message: String,
    started: Instant,
    frame: usize,
    finished: bool,
}
struct TaskInner<W: Write + Send> {
    printer: Printer<W>,
    state: Mutex<TaskState>,
}
impl<W: Write + Send> Drop for TaskInner<W> {
    fn drop(&mut self) {
        let task = self.state.get_mut().unwrap_or_else(|p| p.into_inner());
        if task.finished {
            return;
        }
        if let Ok(mut state) = self.printer.lock() {
            state.tasks = state.tasks.saturating_sub(1);
            if state.visible == Some(Owner::Task(task.id)) && state.begin().is_ok() {
                let _ = state.commit(None, false);
            }
        }
    }
}
pub struct ProgressTask<W: Write + Send> {
    inner: Arc<TaskInner<W>>,
}
impl<W: Write + Send> Clone for ProgressTask<W> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
impl<W: Write + Send> fmt::Debug for ProgressTask<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProgressTask { .. }")
    }
}
impl<W: Write + Send> ProgressTask<W> {
    pub fn update(&self, message: &str) -> Result<(), TerminalError> {
        let mut task = self
            .inner
            .state
            .lock()
            .map_err(|_| TerminalError::Poisoned)?;
        if task.finished {
            return Ok(());
        }
        self.inner.printer.lock()?.validate_text(message)?;
        task.message = copy_text(message)?;
        Ok(())
    }
    pub fn tick(&self) -> Result<(), TerminalError> {
        let mut task = self
            .inner
            .state
            .lock()
            .map_err(|_| TerminalError::Poisoned)?;
        if task.finished {
            return Ok(());
        }
        let mut state = self.inner.printer.lock()?;
        if !matches!(state.options.mode, Mode::Terminal { .. }) {
            return Ok(());
        }
        const FRAMES: [&str; 4] = ["◦", "○", "◌", "●"];
        let frame = (task.frame + 1) % FRAMES.len();
        state.render_progress(&task.message, FRAMES[frame])?;
        let owner = state.progress_owner(Owner::Task(task.id));
        state.commit(owner, false)?;
        task.frame = frame;
        Ok(())
    }
    pub fn finish(&self, outcome: FinishOutcome, message: &str) -> Result<(), TerminalError> {
        let mut task = self
            .inner
            .state
            .lock()
            .map_err(|_| TerminalError::Poisoned)?;
        if task.finished {
            return Ok(());
        }
        let mut state = self.inner.printer.lock()?;
        state.validate_text(message)?;
        let severity = match outcome {
            FinishOutcome::Success => Severity::Success,
            FinishOutcome::Failure => Severity::Error,
            FinishOutcome::Cancelled => Severity::Warning,
        };
        if state.options.mode != Mode::Silent {
            state.begin()?;
            state.line(severity, message)?;
            // Attach elapsed time to the final line without a temporary String.
            state.buffer.bytes.pop();
            let elapsed = task.started.elapsed();
            state.buffer.format(format_args!(
                " ({}.{:03}s)\n",
                elapsed.as_secs(),
                elapsed.subsec_millis()
            ))?;
        }
        // Once IO can begin, completion is final even if the sink fails. Invalid
        // input/render budgets above leave the task open for a smaller retry.
        task.finished = true;
        // Finished handles may be retained by the caller; release their payload
        // allocation together with the active slot, including IO failures.
        task.message = String::new();
        state.tasks -= 1;
        if state.options.mode == Mode::Silent {
            Ok(())
        } else {
            state.commit(None, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_finished_handles_release_message_allocations() {
        let options = Options {
            mode: Mode::Silent,
            limits: Limits {
                max_input_bytes: 4096,
                max_output_bytes: 0,
                max_table_rows: 0,
                max_tasks: 1,
            },
        };
        let printer = Printer::new(io::sink(), "", options).unwrap();
        let mut retained = Vec::new();
        for _ in 0..100 {
            let task = printer.start_task(&"x".repeat(4000)).unwrap();
            assert!(task.inner.state.lock().unwrap().message.capacity() >= 4000);
            task.finish(FinishOutcome::Success, "").unwrap();
            assert_eq!(task.inner.state.lock().unwrap().message.capacity(), 0);
            retained.push(task);
        }
        assert_eq!(printer.state.lock().unwrap().tasks, 0);
        assert_eq!(retained.len(), 100);
    }
    #[test]
    fn allocation_and_checked_limits_are_observable() {
        let mut buffer = Buffer::new(usize::MAX);
        assert_eq!(buffer.reserve(usize::MAX), Err(TerminalError::Allocation));
        let mut bounded = Buffer::new(3);
        bounded.add("abc").unwrap();
        assert_eq!(bounded.add("d"), Err(TerminalError::OutputBytes));
        assert_eq!(bounded.bytes, b"abc");
        bounded.reset();
        bounded.add("xyz").unwrap();
        assert_eq!(bounded.bytes.capacity(), 3);
        assert_eq!(input_size(["ab", "c"], 2), Err(TerminalError::InputBytes));
        let options = Options {
            mode: Mode::Plain,
            limits: Limits {
                max_input_bytes: usize::MAX,
                max_output_bytes: 0,
                max_table_rows: 0,
                max_tasks: 1,
            },
        };
        assert_eq!(
            Printer::new(io::sink(), "", options).unwrap_err(),
            TerminalError::InvalidLimits
        );
    }
    #[test]
    fn poison_and_id_exhaustion_are_sanitized() {
        let options = Options {
            mode: Mode::Silent,
            limits: Limits {
                max_input_bytes: 100,
                max_output_bytes: 100,
                max_table_rows: 1,
                max_tasks: 1,
            },
        };
        let printer = Printer::new(io::sink(), "canary", options).unwrap();
        printer.state.lock().unwrap().next_id = u64::MAX;
        assert_eq!(
            printer.start_task("canary").unwrap_err(),
            TerminalError::TaskLimit
        );
        let clone = printer.clone();
        assert!(
            std::thread::spawn(move || {
                let _lock = clone.state.lock().unwrap();
                panic!("poison fixture");
            })
            .join()
            .is_err()
        );
        assert_eq!(
            printer.print(Message::Data("canary")),
            Err(TerminalError::Poisoned)
        );
        assert!(!format!("{printer:?}").contains("canary"));
    }
}
