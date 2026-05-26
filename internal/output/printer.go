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

func IsTTY() bool { return isTTY }

func Mute() { muted = true }

func Unmute() { muted = false }

func IsMuted() bool { return muted }

type Printer struct {
	prefix string
	w      io.Writer
}

func NewPrinter(prefix string) *Printer {
	w := io.Writer(os.Stdout)
	if muted {
		w = io.Discard
	}
	return &Printer{prefix: prefix, w: w}
}

func (p *Printer) WithWriter(w io.Writer) *Printer {
	return &Printer{prefix: p.prefix, w: w}
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

func (p *Printer) Info(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	_, _ = fmt.Fprintln(p.w, p.tagLine(line))
}

func (p *Printer) Success(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	_, _ = fmt.Fprintln(p.w, p.tagLine(green.Sprint(SymbolOK+" "+line)))
}

func (p *Printer) Error(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	_, _ = fmt.Fprintln(p.w, p.tagLine(red.Sprint(SymbolError+" "+line)))
}

func (p *Printer) Warn(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	fmt.Fprintln(p.w, p.tagLine(yellow.Sprint(SymbolWarn+" "+line)))
}

func (p *Printer) Running(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	fmt.Fprintln(p.w, p.tagLine(cyan.Sprint(SymbolRunning+" "+line)))
}

func (p *Printer) Step(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	fmt.Fprintln(p.w, "  "+dim.Sprint(SymbolStep)+" "+dim.Sprint(line))
}

func (p *Printer) StepOK(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	fmt.Fprintln(p.w, "  "+dim.Sprint(SymbolStep)+" "+green.Sprint(SymbolOK+" "+line))
}

func (p *Printer) StepError(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	fmt.Fprintln(p.w, "  "+dim.Sprint(SymbolStep)+" "+red.Sprint(SymbolError+" "+line))
}

func (p *Printer) StepWarn(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	fmt.Fprintln(p.w, "  "+dim.Sprint(SymbolStep)+" "+yellow.Sprint(SymbolWarn+" "+line))
}

func (p *Printer) Detail(key, value string) {
	fmt.Fprintln(p.w, "  "+dim.Sprint(SymbolStep)+" "+dim.Sprint(key+":")+
		" "+magenta.Sprint(value))
}

func (p *Printer) KeyValue(key string, value string) {
	padded := fmt.Sprintf("%-14s", key+":")
	fmt.Fprintln(p.w, "  "+dim.Sprint(padded)+" "+value)
}

func (p *Printer) Data(content string) {
	fmt.Fprintln(p.w, content)
}

func (p *Printer) Count(label string, n int) {
	plural := "s"
	if n == 1 {
		plural = ""
	}
	fmt.Fprintln(p.w, "  "+dim.Sprint(SymbolStep)+" "+
		magenta.Sprintf("%d", n)+" "+dim.Sprintf("%s%s", label, plural))
}

func (p *Printer) ListItem(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	fmt.Fprintln(p.w, "  "+dim.Sprint(SymbolBullet)+" "+line)
}

func (p *Printer) Header(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	fmt.Fprintln(p.w, "")
	fmt.Fprintln(p.w, bold.Sprint(line))
}

func (p *Printer) Divider() {
	fmt.Fprintln(p.w, dim.Sprint(strings.Repeat(SymbolDivider, 48)))
}

func (p *Printer) Blank() {
	fmt.Fprintln(p.w)
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
	fmt.Fprintln(p.w, sep)
	_, _ = fmt.Fprintf(p.w, "%-*s  %s\n", lw, bold.Sprint(headers[0]), bold.Sprint(headers[1]))
	fmt.Fprintln(p.w, sep)
	for _, r := range rows {
		_, _ = fmt.Fprintf(p.w, "%-*s  %s\n", lw, r[0], r[1])
	}
	fmt.Fprintln(p.w, sep)
}

var spinnerFrames = []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}

type Task struct {
	p      *Printer
	msg    string
	mu     sync.Mutex
	done   bool
	stopCh chan struct{}
}

func (p *Printer) StartTask(msg string, args ...any) *Task {
	line := fmt.Sprintf(msg, args...)
	t := &Task{
		p:      p,
		msg:    line,
		stopCh: make(chan struct{}),
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
				_, _ = fmt.Fprintf(p.w, "\r\033[K%s", line)
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

	line := fmt.Sprintf(msg, args...)
	if isTTY {
		fmt.Fprintf(t.p.w, "\r\033[K")
	}
	fmt.Fprintln(t.p.w, t.p.tagLine(green.Sprint(SymbolOK+" "+line)))
}

func (t *Task) Fail(msg string, args ...any) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.done {
		return
	}
	t.done = true
	close(t.stopCh)

	line := fmt.Sprintf(msg, args...)
	if isTTY {
		fmt.Fprintf(t.p.w, "\r\033[K")
	}
	fmt.Fprintln(t.p.w, t.p.tagLine(red.Sprint(SymbolError+" "+line)))
}

func Fatal(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	fmt.Fprintln(os.Stderr, red.Sprint(SymbolError+" "+line))
	os.Exit(1)
}

func Interrupted() {
	fmt.Fprintln(os.Stdout, "")
	fmt.Fprintln(os.Stdout, yellow.Sprint(SymbolWarn+" Interrupted."))
	os.Exit(130)
}
