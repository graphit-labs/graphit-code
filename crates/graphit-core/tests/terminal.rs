use graphit_core::terminal::{
    FinishOutcome, Limits, Message, Mode, Options, Printer, Severity, TerminalError,
};
use std::{
    io::{self, Write},
    num::NonZeroU16,
    sync::{Arc, Barrier, Mutex},
    thread,
};
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
struct Recording {
    bytes: Vec<u8>,
    writes: Vec<Vec<u8>>,
    flushes: usize,
    fail_write: bool,
    fail_flush: bool,
    zero: bool,
    short: bool,
    partial_error: bool,
}
#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Recording>>);
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        thread::yield_now();
        let mut r = self.0.lock().unwrap();
        if r.fail_write {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "canary secret sink",
            ));
        }
        if r.zero {
            return Ok(0);
        }
        let len = if r.short || r.partial_error {
            bytes.len().min(2)
        } else {
            bytes.len()
        };
        r.bytes.extend_from_slice(&bytes[..len]);
        r.writes.push(bytes[..len].to_vec());
        if r.partial_error {
            r.fail_write = true;
        }
        Ok(len)
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut r = self.0.lock().unwrap();
        r.flushes += 1;
        if r.fail_flush {
            Err(io::Error::other("canary flush"))
        } else {
            Ok(())
        }
    }
}
impl Sink {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().bytes.clone()).unwrap()
    }
    fn reset(&self) {
        *self.0.lock().unwrap() = Recording::default();
    }
}
fn options(mode: Mode) -> Options {
    Options {
        mode,
        limits: Limits {
            max_input_bytes: 4096,
            max_output_bytes: 32768,
            max_table_rows: 128,
            max_tasks: 4,
        },
    }
}
fn tty(columns: u16, color: bool) -> Mode {
    Mode::Terminal {
        columns: NonZeroU16::new(columns).unwrap(),
        color,
    }
}
fn printer(mode: Mode) -> (Printer<Sink>, Sink) {
    let sink = Sink::default();
    (
        Printer::new(sink.clone(), "graphit", options(mode)).unwrap(),
        sink,
    )
}
#[test]
fn semantic_variants_preserve_symbols_labels_and_raw_channels() {
    let (p, sink) = printer(Mode::Plain);
    for severity in [
        Severity::Info,
        Severity::Success,
        Severity::Error,
        Severity::Warning,
        Severity::Running,
    ] {
        p.print(Message::Line {
            severity,
            text: "texto",
        })
        .unwrap();
    }
    p.print(Message::Step {
        severity: None,
        text: "etapa",
    })
    .unwrap();
    p.print(Message::Step {
        severity: Some(Severity::Success),
        text: "pronta",
    })
    .unwrap();
    p.print(Message::Detail {
        key: "nome",
        value: "valor",
    })
    .unwrap();
    p.print(Message::KeyValue {
        key: "nome",
        value: "valor",
    })
    .unwrap();
    p.print(Message::Count {
        amount: 1,
        singular: "arquivo",
        plural: "arquivos",
    })
    .unwrap();
    p.print(Message::Count {
        amount: 0,
        singular: "arquivo",
        plural: "arquivos",
    })
    .unwrap();
    p.print(Message::Count {
        amount: u64::MAX,
        singular: "arquivo",
        plural: "arquivos",
    })
    .unwrap();
    p.print(Message::Bullet("item")).unwrap();
    p.print(Message::Header("Título")).unwrap();
    p.print(Message::Blank).unwrap();
    p.print(Message::Table {
        headers: ["Chave", "Valor"],
        rows: &[["界", "um"], ["e\u{301}", "dois"]],
    })
    .unwrap();
    p.print(Message::Data("\x1b[31mraw\n\0")).unwrap();
    assert_eq!(
        sink.text(),
        concat!(
            "[graphit] texto\n[graphit] ✓ texto\n[graphit] ✗ texto\n[graphit] ! texto\n[graphit] ◦ texto\n",
            "[graphit] › etapa\n[graphit] › pronta\n[graphit]   nome: valor\n[graphit] nome: valor\n",
            "[graphit] 1 arquivo\n[graphit] 0 arquivos\n[graphit] 18446744073709551615 arquivos\n",
            "[graphit] • item\n\n[graphit] Título\n\n\n",
            "[graphit] Chave  Valor\n[graphit] 界     um\n[graphit] e\u{301}      dois\n\x1b[31mraw\n\0\n"
        )
    );
    let untagged = Printer::new(sink.clone(), "", options(Mode::Plain)).unwrap();
    sink.reset();
    untagged
        .print(Message::Line {
            severity: Severity::Info,
            text: "ok",
        })
        .unwrap();
    assert_eq!(sink.text(), "ok\n");
}
#[test]
fn instances_clones_silence_and_stream_boundaries() {
    let (p, sink) = printer(Mode::Plain);
    let clone = p.clone();
    p.write_data("par").unwrap();
    clone.write_data("cial").unwrap();
    clone
        .print(Message::Line {
            severity: Severity::Warning,
            text: "aviso",
        })
        .unwrap();
    assert_eq!(sink.text(), "parcial\n[graphit] ! aviso\n");
    sink.reset();
    p.write_data("fim\n").unwrap();
    p.progress("etapa").unwrap();
    p.end_progress().unwrap();
    assert_eq!(sink.text(), "fim\n[graphit] ◦ etapa\n");
    let (silent, muted) = printer(Mode::Silent);
    silent.print(Message::Data("canary")).unwrap();
    silent.write_data("canary").unwrap();
    silent.progress("canary").unwrap();
    let task = silent.start_task("canary").unwrap();
    task.update("canary").unwrap();
    task.tick().unwrap();
    task.finish(FinishOutcome::Success, "canary").unwrap();
    silent.end_progress().unwrap();
    assert!(muted.text().is_empty());
    assert_eq!(muted.0.lock().unwrap().flushes, 0);
    assert!(!format!("{silent:?} {task:?}").contains("canary"));
    p.print(Message::Data("ativo")).unwrap();
    assert!(sink.text().ends_with("ativo\n"));
    let (tty_p, tty_sink) = printer(tty(80, true));
    tty_p
        .print(Message::Line {
            severity: Severity::Success,
            text: "ok",
        })
        .unwrap();
    assert!(tty_sink.text().contains("\x1b["));
    assert!(tty_sink.text().contains("✓"));
}
#[test]
fn sink_failures_partial_io_and_flush_are_observable_without_payloads() {
    let (p, sink) = printer(Mode::Plain);
    sink.0.lock().unwrap().short = true;
    p.print(Message::Data("abcdef")).unwrap();
    assert_eq!(sink.text(), "abcdef\n");
    assert_eq!(sink.0.lock().unwrap().flushes, 1);
    sink.reset();
    sink.0.lock().unwrap().zero = true;
    assert_eq!(
        p.print(Message::Data("canary")),
        Err(TerminalError::Io {
            kind: io::ErrorKind::WriteZero,
            os_code: None
        })
    );
    sink.reset();
    sink.0.lock().unwrap().partial_error = true;
    let error = p.write_data("canary").unwrap_err();
    assert_eq!(sink.text(), "ca");
    assert_eq!(
        error,
        TerminalError::Io {
            kind: io::ErrorKind::BrokenPipe,
            os_code: None
        }
    );
    assert!(!format!("{error:?} {error}").contains("canary"));
    {
        let mut r = sink.0.lock().unwrap();
        r.fail_write = false;
        r.partial_error = false;
    }
    p.print(Message::Data("recovery")).unwrap();
    assert_eq!(sink.text(), "ca\nrecovery\n");
    sink.reset();
    sink.0.lock().unwrap().fail_flush = true;
    assert_eq!(
        p.print(Message::Data("once")),
        Err(TerminalError::Io {
            kind: io::ErrorKind::Other,
            os_code: None
        })
    );
    assert_eq!(sink.text(), "once\n"); // no whole-operation retry
}
#[test]
fn input_output_row_and_task_budgets_reject_before_write() {
    let sink = Sink::default();
    let mut o = options(Mode::Plain);
    o.limits.max_input_bytes = 3;
    o.limits.max_output_bytes = 4;
    o.limits.max_table_rows = 0;
    o.limits.max_tasks = 1;
    let p = Printer::new(sink.clone(), "", o).unwrap();
    p.print(Message::Data("abc")).unwrap();
    assert_eq!(sink.text(), "abc\n");
    sink.reset();
    assert_eq!(
        p.print(Message::Data("abcd")),
        Err(TerminalError::InputBytes)
    );
    assert_eq!(
        p.print(Message::Table {
            headers: ["", ""],
            rows: &[["", ""]]
        }),
        Err(TerminalError::TableRows)
    );
    assert_eq!(
        p.print(Message::Count {
            amount: 1,
            singular: "ab",
            plural: "cd"
        }),
        Err(TerminalError::InputBytes)
    );
    assert!(sink.text().is_empty());
    let mut o = options(Mode::Plain);
    o.limits.max_output_bytes = 3;
    let p = Printer::new(sink.clone(), "", o).unwrap();
    assert_eq!(
        p.print(Message::Data("abc")),
        Err(TerminalError::OutputBytes)
    );
    assert!(sink.text().is_empty());
    // Escaping and header/table overhead, including prior cursor cleanup, are
    // counted in the actual output cap before the sink is called.
    assert_eq!(
        p.print(Message::Header("x")),
        Err(TerminalError::OutputBytes)
    );
    assert_eq!(
        p.print(Message::Line {
            severity: Severity::Info,
            text: "\n"
        }),
        Err(TerminalError::OutputBytes)
    );
    let mut o = options(Mode::Silent);
    o.limits.max_input_bytes = 0;
    o.limits.max_output_bytes = 0;
    o.limits.max_tasks = 1;
    let p = Printer::new(sink.clone(), "", o).unwrap();
    p.print(Message::Data("")).unwrap();
    assert_eq!(p.progress("x"), Err(TerminalError::InputBytes));
    let task = p.start_task("").unwrap();
    assert_eq!(p.start_task("").unwrap_err(), TerminalError::TaskLimit);
    task.finish(FinishOutcome::Success, "").unwrap();
    let next = p.start_task("").unwrap();
    drop(next);
    assert!(p.start_task("").is_ok());
    assert!(sink.text().is_empty());
    o.mode = Mode::Plain;
    let p = Printer::new(sink.clone(), "", o).unwrap();
    assert_eq!(p.print(Message::Data("")), Err(TerminalError::OutputBytes));
    o.limits.max_tasks = 0;
    assert_eq!(
        Printer::new(sink.clone(), "", o).unwrap_err(),
        TerminalError::InvalidLimits
    );
}
#[test]
fn unicode_progress_never_wraps_or_splits_graphemes_and_clears_once() {
    let values = [
        "界界界界",
        "e\u{301}e\u{301}e\u{301}",
        "👩🏽‍💻👩🏽‍💻",
        "🇧🇷🇧🇷",
        "👨‍👩‍👧‍👦a",
        "مرحبا بالعالم",
    ];
    for columns in [1, 2, 8, 80] {
        for text in values {
            let sink = Sink::default();
            let p = Printer::new(sink.clone(), "", options(tty(columns, false))).unwrap();
            p.progress(text).unwrap();
            let frame = sink.text();
            assert!(frame.width() < usize::from(columns), "{columns}: {frame:?}");
            assert!(!frame.contains('\n'));
            assert!(!frame.ends_with('\u{200d}'));
            p.end_progress().unwrap();
            let cleared = sink.text();
            assert!(cleared.ends_with("\r\x1b[2K"));
            p.end_progress().unwrap();
            assert_eq!(sink.text(), cleared);
        }
    }
    let (p, sink) = printer(tty(8, false));
    p.progress("muito longo").unwrap();
    assert_eq!(sink.text(), "[graph…");
    p.resize(NonZeroU16::new(80).unwrap()).unwrap();
    p.progress("curto").unwrap();
    assert!(sink.text().ends_with("\r\x1b[2K[graphit] ◦ curto"));
    p.write_data("answer").unwrap();
    p.progress("nova").unwrap();
    assert!(sink.text().ends_with("answer\n[graphit] ◦ nova"));
    p.print(Message::Line {
        severity: Severity::Error,
        text: "falha",
    })
    .unwrap();
    assert!(sink.text().ends_with("\r\x1b[2K[graphit] ✗ falha\n"));
}
#[test]
fn controls_are_escaped_in_semantic_fields_but_explicit_raw_is_preserved() {
    let sink = Sink::default();
    let p = Printer::new(sink.clone(), "p\n", options(Mode::Plain)).unwrap();
    p.print(Message::Line {
        severity: Severity::Info,
        text: "a\r\t\x1b\u{85}\0b",
    })
    .unwrap();
    assert_eq!(
        sink.text(),
        "[p\\u{a}] a\\u{d}\\u{9}\\u{1b}\\u{85}\\u{0}b\n"
    );
    sink.reset();
    p.print(Message::Table {
        headers: ["a\n", "b"],
        rows: &[["e\u{301}", "\x1b[31m"]],
    })
    .unwrap();
    assert!(!sink.text().contains('\x1b'));
    assert_eq!(sink.text().lines().count(), 2);
    sink.reset();
    p.write_data("\x1b[31m\n\0").unwrap();
    assert_eq!(sink.text(), "\x1b[31m\n\0");
}
#[test]
fn concurrent_clones_preserve_header_table_and_task_operation_blocks() {
    let (p, sink) = printer(Mode::Plain);
    thread::scope(|scope| {
        for _ in 0..8 {
            let clone = p.clone();
            scope.spawn(move || {
                for _ in 0..50 {
                    clone.print(Message::Header("header")).unwrap();
                    clone
                        .print(Message::Table {
                            headers: ["K", "V"],
                            rows: &[["a", "1"], ["b", "2"]],
                        })
                        .unwrap();
                    let task = loop {
                        match clone.start_task("task") {
                            Ok(task) => break task,
                            Err(TerminalError::TaskLimit) => thread::yield_now(),
                            Err(e) => panic!("{e}"),
                        }
                    };
                    task.finish(FinishOutcome::Success, "done").unwrap();
                }
            });
        }
    });
    let r = sink.0.lock().unwrap();
    assert_eq!(r.writes.len(), 1600);
    assert_eq!(r.flushes, 1600);
    for block in &r.writes {
        let text = std::str::from_utf8(block).unwrap();
        assert!(
            text == "\n[graphit] header\n\n"
                || text == "[graphit] K  V\n[graphit] a  1\n[graphit] b  2\n"
                || text == "[graphit] ◦ task\n"
                || (text.starts_with("[graphit] ✓ done (") && text.ends_with("s)\n"))
        );
    }
}
#[test]
fn concurrent_finish_is_unique_and_closed_handles_never_write_again() {
    let (p, sink) = printer(tty(80, false));
    let task = p.start_task("work").unwrap();
    let barrier = Arc::new(Barrier::new(100));
    thread::scope(|scope| {
        for index in 0..100 {
            let task = task.clone();
            let barrier = barrier.clone();
            scope.spawn(move || {
                barrier.wait();
                task.finish(
                    [
                        FinishOutcome::Success,
                        FinishOutcome::Failure,
                        FinishOutcome::Cancelled,
                    ][index % 3],
                    "final",
                )
                .unwrap();
            });
        }
    });
    let text = sink.text();
    assert_eq!(text.matches("final").count(), 1);
    assert_eq!(text.matches("\r\x1b[2K").count(), 1);
    task.update(&"x".repeat(10000)).unwrap();
    task.tick().unwrap();
    task.finish(FinishOutcome::Failure, "ignored").unwrap();
    assert_eq!(sink.text(), text);
    drop(task);
    assert!(p.start_task("new").is_ok());
}
#[test]
fn task_ticks_updates_retry_io_failure_and_drop_release_exactly_one_slot() {
    let sink = Sink::default();
    let mut o = options(tty(80, false));
    o.limits.max_tasks = 2;
    let p = Printer::new(sink.clone(), "", o).unwrap();
    let first = p.start_task("first").unwrap();
    let second = p.start_task("second").unwrap();
    assert_eq!(p.start_task("third").unwrap_err(), TerminalError::TaskLimit);
    let text = sink.text();
    let clone = first.clone();
    drop(first);
    assert_eq!(sink.text(), text);
    drop(clone);
    assert_eq!(sink.text(), text); // second owns the visible line
    let third = p.start_task("third").unwrap();
    third.update("updated").unwrap();
    assert!(!sink.text().contains("updated"));
    third.tick().unwrap();
    assert!(sink.text().ends_with("○ updated"));
    assert_eq!(
        third.finish(FinishOutcome::Success, &"x".repeat(4097)),
        Err(TerminalError::InputBytes)
    );
    third.tick().unwrap();
    assert!(sink.text().ends_with("◌ updated"));
    sink.0.lock().unwrap().fail_write = true;
    assert!(matches!(
        third.finish(FinishOutcome::Failure, "failed"),
        Err(TerminalError::Io { .. })
    ));
    let text = sink.text();
    third.tick().unwrap();
    third.finish(FinishOutcome::Success, "retry").unwrap();
    assert_eq!(sink.text(), text);
    sink.0.lock().unwrap().fail_write = false;
    let replacement = p.start_task("replacement").unwrap();
    drop(replacement);
    assert!(sink.text().ends_with("\r\x1b[2K"));
    second
        .finish(FinishOutcome::Cancelled, "cancelled")
        .unwrap();
    assert!(sink.text().contains("! cancelled"));
    let (plain, out) = printer(Mode::Plain);
    let task = plain.start_task("start").unwrap();
    let text = out.text();
    task.tick().unwrap();
    task.update("next").unwrap();
    assert_eq!(out.text(), text);
    task.finish(FinishOutcome::Success, "done").unwrap();
    assert!(out.text().contains("✓ done"));
    // A render-budget failure leaves lifecycle open; later shorter final works.
    let sink = Sink::default();
    let mut o = options(Mode::Plain);
    o.limits.max_output_bytes = 32;
    o.limits.max_tasks = 1;
    let p = Printer::new(sink.clone(), "", o).unwrap();
    let task = p.start_task("work").unwrap();
    let before = sink.text();
    assert_eq!(
        task.finish(FinishOutcome::Success, &"x".repeat(30)),
        Err(TerminalError::OutputBytes)
    );
    assert_eq!(sink.text(), before);
    assert_eq!(p.start_task("new").unwrap_err(), TerminalError::TaskLimit);
    task.finish(FinishOutcome::Success, "done").unwrap();
    assert!(p.start_task("new").is_ok());
}

#[test]
fn partial_terminated_lines_open_a_boundary_but_completed_flush_failures_do_not() {
    for raw in [false, true] {
        let (p, sink) = printer(Mode::Plain);
        sink.0.lock().unwrap().partial_error = true;
        let error = if raw {
            p.write_data("canary\n")
        } else {
            p.print(Message::Data("canary"))
        };
        assert!(matches!(
            error,
            Err(TerminalError::Io {
                kind: io::ErrorKind::BrokenPipe,
                ..
            })
        ));
        assert_eq!(sink.text(), "ca");
        {
            let mut r = sink.0.lock().unwrap();
            r.fail_write = false;
            r.partial_error = false;
        }
        p.print(Message::Data("recovery")).unwrap();
        assert_eq!(sink.text(), "ca\nrecovery\n");
    }
    let (p, sink) = printer(Mode::Plain);
    sink.0.lock().unwrap().fail_flush = true;
    assert!(p.print(Message::Data("complete")).is_err());
    sink.0.lock().unwrap().fail_flush = false;
    p.print(Message::Data("next")).unwrap();
    assert_eq!(sink.text(), "complete\nnext\n");
}

#[test]
fn clipped_emoji_clusters_and_rejected_render_preserve_the_visible_owner() {
    let sink = Sink::default();
    let p = Printer::new(sink.clone(), "", options(tty(8, false))).unwrap();
    p.progress("👩🏽‍💻👩🏽‍💻👩🏽‍💻").unwrap();
    assert_eq!(sink.text(), "◦ 👩🏽‍💻👩🏽‍💻…");
    p.end_progress().unwrap();
    sink.reset();
    let mut o = options(tty(80, false));
    o.limits.max_output_bytes = 8;
    let p = Printer::new(sink.clone(), "", o).unwrap();
    p.progress("abcd").unwrap();
    let before = sink.text();
    assert_eq!(p.progress("abcd"), Err(TerminalError::OutputBytes));
    assert_eq!(sink.text(), before);
    p.end_progress().unwrap();
    assert_eq!(sink.text(), "◦ abcd\r\x1b[2K");
}
