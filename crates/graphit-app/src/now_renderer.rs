//! Borrowed full-snapshot terminal view; the entry owns cancelable pipe IO.
use crate::source_server::BrokerRequest;
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use graphit_daemon::now_subscription::RemoteNowSnapshot;
use std::{
    fmt,
    io::{self, Write},
};

pub struct Renderer<'a, W: Write> {
    output: &'a mut W,
    terminal: bool,
    request: &'a BrokerRequest,
}
impl<'a, W: Write> Renderer<'a, W> {
    pub fn new(output: &'a mut W, request: &'a BrokerRequest) -> io::Result<Self> {
        let renderer = Self {
            output,
            terminal: true,
            request,
        };
        // Construct the guard first so a partially written enter is still restored.
        execute!(renderer.output, EnterAlternateScreen)?;
        Ok(renderer)
    }
    pub fn render(&mut self, snapshot: &RemoteNowSnapshot) -> io::Result<()> {
        execute!(self.output, MoveTo(0, 0), Clear(ClearType::All))?;
        let (columns, rows) = crossterm::terminal::size()
            .ok()
            .filter(|(w, h)| *w > 0 && *h > 1)
            .unwrap_or((80, 24));
        let mut screen = Screen {
            output: self.output,
            columns: usize::from(columns),
            rows,
            row: 0,
            clipped: false,
        };
        screen.line(format_args!("Now  {}", snapshot.project_id()))?;
        screen.line(format_args!(
            "Provider {}  Version {}  Instance {}",
            Safe(&self.request.provider),
            Safe(self.request.version.as_deref().unwrap_or("all")),
            Safe(self.request.instance_id.as_deref().unwrap_or("all"))
        ))?;
        screen.line(format_args!(""))?;
        if snapshot.instances().is_empty() {
            screen.line(format_args!("No online instances match these filters."))?;
        }
        for instance in snapshot.instances() {
            screen.line(format_args!(
                "{}  {}",
                Safe(instance.instance_id()),
                Safe(instance.version())
            ))?;
            let sections = instance.now().sections();
            macro_rules! section {
                ($label:literal,$section:expr) => {{
                    let section = $section;
                    screen.line(format_args!(
                        "  {}  {:?}  total {}{}{}",
                        $label,
                        section.availability(),
                        section
                            .total()
                            .map(|n| n.to_string())
                            .as_deref()
                            .unwrap_or("unavailable"),
                        if section.has_more() == Some(true) {
                            " (more)"
                        } else {
                            ""
                        },
                        section
                            .error()
                            .map(|e| format!("  {e:?}"))
                            .as_deref()
                            .unwrap_or("")
                    ))?;
                }};
            }
            section!("Tasks", sections.tasks());
            for item in sections.tasks().items() {
                screen.line(format_args!(
                    "    {}  {}  owner {}",
                    Safe(item.id()),
                    Safe(item.title()),
                    Safe(item.owner())
                ))?;
            }
            section!("Sessions", sections.sessions());
            for item in sections.sessions().items() {
                screen.line(format_args!(
                    "    {}  {}  {}/{} completed",
                    Safe(item.id()),
                    Safe(item.title()),
                    item.completed_tasks(),
                    item.total_tasks()
                ))?;
            }
            section!("Memories", sections.memories());
            for item in sections.memories().items() {
                screen.line(format_args!(
                    "    {}  {}",
                    Safe(item.id()),
                    Safe(item.title())
                ))?;
            }
            section!("Knowledge", sections.knowledge());
            for item in sections.knowledge().items() {
                screen.line(format_args!(
                    "    {}  {}",
                    Safe(item.slug()),
                    Safe(item.title())
                ))?;
            }
            screen.line(format_args!(""))?;
        }
        if screen.clipped {
            screen.row = rows - 1;
            screen.clipped_line(format_args!(
                "More items or text; use pipe for the complete snapshot."
            ))?;
        }
        self.output.flush()
    }
    pub fn finish(&mut self) -> io::Result<()> {
        if self.terminal {
            execute!(self.output, LeaveAlternateScreen)?;
            self.terminal = false;
        }
        Ok(())
    }
}
impl<W: Write> Drop for Renderer<'_, W> {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
struct Safe<'a>(&'a str);
impl fmt::Display for Safe<'_> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.chars() {
            if c.is_control()
                || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            {
                write!(out, "\\u{{{:04x}}}", c as u32)?;
            } else {
                write!(out, "{c}")?;
            }
        }
        Ok(())
    }
}

// Cursor coordinates bound only the physical view; the full snapshot remains available in pipe output.
struct Screen<'a, W: Write> {
    output: &'a mut W,
    columns: usize,
    rows: u16,
    row: u16,
    clipped: bool,
}
impl<W: Write> Screen<'_, W> {
    fn line(&mut self, text: fmt::Arguments<'_>) -> io::Result<()> {
        if self.row >= self.rows - 1 {
            self.clipped = true;
            return Ok(());
        }
        self.clipped_line(text)?;
        self.row += 1;
        Ok(())
    }
    fn clipped_line(&mut self, text: fmt::Arguments<'_>) -> io::Result<()> {
        use unicode_width::UnicodeWidthChar as _;
        let line = fmt::format(text);
        let mut end = 0;
        let mut width = 0;
        for (position, c) in line.char_indices() {
            let next = width + c.width().unwrap_or(0);
            if next > self.columns {
                self.clipped = true;
                break;
            }
            width = next;
            end = position + c.len_utf8();
        }
        execute!(
            self.output,
            MoveTo(0, self.row),
            crossterm::style::Print(&line[..end])
        )
    }
}
