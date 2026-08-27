package output

import (
	"fmt"
	"io"
	"os"
	"strings"
	"sync"
	"time"
	"unicode/utf8"

	"github.com/fatih/color"
	"golang.org/x/term"
)

const (
	SymbolOK      = "✓"
	SymbolError   = "✗"
	SymbolWarn    = "!"
	SymbolStep    = "›"
	SymbolRunning = "◦"
	SymbolDivider = "─"
	SymbolBullet  = "•"
)

var (
	green   = color.New(color.FgGreen, color.Bold)
	red     = color.New(color.FgRed, color.Bold)
	yellow  = color.New(color.FgYellow, color.Bold)
	cyan    = color.New(color.FgCyan)
	magenta = color.New(color.FgMagenta)
	bold    = color.New(color.Bold)
	dim     = color.New(color.FgHiBlack)
)

var isTTY bool

var muted bool

func init() {
	isTTY = term.IsTerminal(int(os.Stdout.Fd()))
	if !isTTY {
		color.NoColor = true
	}
}

// IsTTY reports whether stdout is a terminal — that is, whether there is a
// cursor to move. Callers that emit progress use it to choose how often to
// speak: a line rewritten in place can refresh many times a second, while a
// line appended to a log file cannot.
func IsTTY() bool { return isTTY }

func Mute() {
	muted = true
	color.NoColor = true
}

type Printer struct {
	prefix string
	w      io.Writer

	// mu guards progress, which says the cursor is parked at the end of a
	// transient line that no newline has terminated yet.
	mu       sync.Mutex
	progress bool
}

func NewPrinter(prefix string) *Printer {
	w := io.Writer(os.Stdout)
	if muted {
		w = io.Discard
	}
	return &Printer{prefix: prefix, w: w}
}

// NewPrinterTo builds a printer that writes to w instead of standard output.
//
// It exists for callers that interleave printed messages with output they write
// themselves — the live search prints an answer as it streams and reports tool calls
// around it, and the two have to reach the same place in the right order or the
// answer arrives cut in half. Naming the destination once makes that orderable, and
// makes it observable in a test without reassigning os.Stdout underneath a process
// that has other things to say.
func NewPrinterTo(prefix string, w io.Writer) *Printer {
	if muted {
		w = io.Discard
	}
	return &Printer{prefix: prefix, w: w}
}

func (p *Printer) tag() string {
	if p.prefix == "" {
		return ""
	}
	return dim.Sprintf("[%s]", p.prefix)
}

func (p *Printer) tagLine(content string) string {
	if p.prefix == "" {
		return content
	}
	return p.tag() + " " + content
}

// println writes one finished line. Every printing method goes through here
// so that a transient progress line — which has no newline of its own — is
// erased first; otherwise the next line lands on top of half of it.
func (p *Printer) println(line string) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.erase()
	_, _ = fmt.Fprintln(p.w, line)
}

func (p *Printer) printf(format string, args ...any) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.erase()
	_, _ = fmt.Fprintf(p.w, format, args...)
}

// erase clears the progress line if one is on screen. Caller holds mu.
func (p *Printer) erase() {
	if !p.progress {
		return
	}
	p.progress = false
	_, _ = fmt.Fprint(p.w, "\r\033[K")
}

func (p *Printer) Info(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	p.println(p.tagLine(line))
}

func (p *Printer) Success(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	p.println(p.tagLine(green.Sprint(SymbolOK + " " + line)))
}

func (p *Printer) Error(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	p.println(p.tagLine(red.Sprint(SymbolError + " " + line)))
}

func (p *Printer) Warn(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	p.println(p.tagLine(yellow.Sprint(SymbolWarn + " " + line)))
}

func (p *Printer) Running(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	p.println(p.tagLine(cyan.Sprint(SymbolRunning + " " + line)))
}

func (p *Printer) Step(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	p.println("  " + dim.Sprint(SymbolStep) + " " + dim.Sprint(line))
}

// StepProgress says the same thing Step says, except that consecutive calls
// overwrite each other instead of stacking up. Indexing a large repository
// reports for twenty minutes; as ordinary lines that is a screenful of one
// number counting up, and everything printed before it is scrolled away.
//
// Without a terminal there is no cursor to move, so this degrades to Step —
// and the caller is expected to throttle far harder in that case, because
// every call really is another line in the log.
func (p *Printer) StepProgress(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	if !isTTY {
		p.Step("%s", line)
		return
	}

	// A line longer than the terminal wraps, and a wrapped line survives the
	// next \r\033[K from the second row down — leaving debris on screen.
	line = truncate(line, termWidth()-len("  "+SymbolStep+" "))
	p.overwrite("  " + dim.Sprint(SymbolStep) + " " + dim.Sprint(line))
}

// overwrite replaces the transient line with content and leaves the cursor
// sitting on it. Both StepProgress and the task spinner are made of this.
func (p *Printer) overwrite(line string) {
	p.mu.Lock()
	defer p.mu.Unlock()
	_, _ = fmt.Fprint(p.w, "\r\033[K"+line)
	p.progress = true
}

// EndProgress erases the progress line when nothing else is going to be
// printed after it. Any ordinary print erases it on its own.
func (p *Printer) EndProgress() {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.erase()
}

func termWidth() int {
	w, _, err := term.GetSize(int(os.Stdout.Fd()))
	if err != nil || w <= 0 {
		return 80
	}
	return w
}

func truncate(s string, max int) string {
	if max <= 1 || utf8.RuneCountInString(s) <= max {
		return s
	}
	return string([]rune(s)[:max-1]) + "…"
}

func (p *Printer) StepOK(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	p.println("  " + dim.Sprint(SymbolStep) + " " + green.Sprint(SymbolOK+" "+line))
}

func (p *Printer) StepWarn(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	p.println("  " + dim.Sprint(SymbolStep) + " " + yellow.Sprint(SymbolWarn+" "+line))
}

func (p *Printer) Detail(key, value string) {
	p.println("  " + dim.Sprint(SymbolStep) + " " + dim.Sprint(key+":") +
		" " + magenta.Sprint(value))
}

func (p *Printer) KeyValue(key string, value string) {
	padded := fmt.Sprintf("%-14s", key+":")
	p.println("  " + dim.Sprint(padded) + " " + value)
}

func (p *Printer) Data(content string) {
	p.println(content)
}

func (p *Printer) Count(label string, n int) {
	plural := "s"
	if n == 1 {
		plural = ""
	}
	p.println("  " + dim.Sprint(SymbolStep) + " " +
		magenta.Sprintf("%d", n) + " " + dim.Sprintf("%s%s", label, plural))
}

func (p *Printer) ListItem(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	p.println("  " + dim.Sprint(SymbolBullet) + " " + line)
}

func (p *Printer) Header(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	p.println("")
	p.println(bold.Sprint(line))
}

func (p *Printer) Blank() {
	p.println("")
}

func (p *Printer) Table(headers [2]string, rows [][2]string) {

	lw := utf8.RuneCountInString(headers[0])
	for _, r := range rows {
		rl := utf8.RuneCountInString(r[0])
		if rl > lw {
			lw = rl
		}
	}
	lw += 2

	sep := dim.Sprint(strings.Repeat(SymbolDivider, lw+30))
	p.println(sep)
	p.printf("%-*s  %s\n", lw, bold.Sprint(headers[0]), bold.Sprint(headers[1]))
	p.println(sep)
	for _, r := range rows {
		p.printf("%-*s  %s\n", lw, r[0], r[1])
	}
	p.println(sep)
}

var spinnerFrames = []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}

type Task struct {
	p         *Printer
	msg       string
	mu        sync.Mutex
	done      bool
	stopCh    chan struct{}
	startedAt time.Time
}

func (p *Printer) StartTask(msg string, args ...any) *Task {
	line := fmt.Sprintf(msg, args...)
	t := &Task{
		p:         p,
		msg:       line,
		stopCh:    make(chan struct{}),
		startedAt: time.Now(),
	}

	if !isTTY {

		p.Running("%s", line)
		return t
	}

	go func() {
		frame := 0
		ticker := time.NewTicker(80 * time.Millisecond)
		defer ticker.Stop()

		for {
			select {
			case <-t.stopCh:
				return
			case <-ticker.C:
				t.mu.Lock()
				if t.done {
					t.mu.Unlock()
					return
				}

				spinner := cyan.Sprint(spinnerFrames[frame%len(spinnerFrames)])
				text := cyan.Sprint(SymbolRunning + " " + t.msg)
				line := p.tagLine(text) + " " + spinner
				p.overwrite(line)
				frame++
				t.mu.Unlock()
			}
		}
	}()

	return t
}

func (t *Task) Update(msg string, args ...any) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.msg = fmt.Sprintf(msg, args...)
}

func (t *Task) Done(msg string, args ...any) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.done {
		return
	}
	t.done = true
	close(t.stopCh)

	elapsed := time.Since(t.startedAt)
	line := fmt.Sprintf(msg, args...) + dim.Sprintf(" (%.1fs)", elapsed.Seconds())
	t.p.println(t.p.tagLine(green.Sprint(SymbolOK + " " + line)))
}

func (t *Task) Fail(msg string, args ...any) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.done {
		return
	}
	t.done = true
	close(t.stopCh)

	elapsed := time.Since(t.startedAt)
	line := fmt.Sprintf(msg, args...) + dim.Sprintf(" (%.1fs)", elapsed.Seconds())
	t.p.println(t.p.tagLine(red.Sprint(SymbolError + " " + line)))
}

func Fatal(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	_, _ = fmt.Fprintln(os.Stderr, red.Sprint(SymbolError+" "+line))
	os.Exit(1)
}

func Interrupted() {
	if muted {
		os.Exit(130)
	}
	_, _ = fmt.Fprintln(os.Stdout, "")
	_, _ = fmt.Fprintln(os.Stdout, yellow.Sprint(SymbolWarn+" Interrupted."))
	os.Exit(130)
}
