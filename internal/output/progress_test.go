package output

import (
	"bytes"
	"strings"
	"testing"
	"unicode/utf8"
)

// `ast index` on a 36k-file repository reported for eleven minutes with Step,
// one line per refresh, and buried everything printed before it under a column
// of the same sentence with a different number in it.
func TestStepProgressRewritesOneLineOnATTY(t *testing.T) {
	isTTYOrig := isTTY
	defer func() { isTTY = isTTYOrig }()
	isTTY = true

	var buf bytes.Buffer
	p := NewPrinter("").WithWriter(&buf)

	p.StepProgress("Parsing: %d/%d file(s)", 1, 3)
	p.StepProgress("Parsing: %d/%d file(s)", 2, 3)
	p.StepProgress("Parsing: %d/%d file(s)", 3, 3)

	out := buf.String()
	if n := strings.Count(out, "\n"); n != 0 {
		t.Errorf("progress emitted %d newline(s); each one is a line that scrolls: %q", n, out)
	}
	if n := strings.Count(out, "\r\033[K"); n != 3 {
		t.Errorf("got %d erase sequence(s), want one per refresh: %q", n, out)
	}
	if !strings.Contains(out, "Parsing: 3/3 file(s)") {
		t.Errorf("the last refresh is missing: %q", out)
	}
}

// The cursor is left parked on the progress line, so whatever prints next has
// to erase it — otherwise the summary lands on top of half a counter.
func TestAPrintAfterProgressErasesIt(t *testing.T) {
	isTTYOrig := isTTY
	defer func() { isTTY = isTTYOrig }()
	isTTY = true

	var buf bytes.Buffer
	p := NewPrinter("").WithWriter(&buf)

	p.StepProgress("Parsing: 900/900 file(s)")
	buf.Reset()
	p.Success("900 files indexed")

	out := buf.String()
	if !strings.HasPrefix(out, "\r\033[K") {
		t.Errorf("the final line did not erase the progress line first: %q", out)
	}
	if !strings.Contains(out, "900 files indexed") {
		t.Errorf("missing the line itself: %q", out)
	}
}

func TestProgressIsErasedOnceOnly(t *testing.T) {
	isTTYOrig := isTTY
	defer func() { isTTY = isTTYOrig }()
	isTTY = true

	var buf bytes.Buffer
	p := NewPrinter("").WithWriter(&buf)

	p.StepProgress("working")
	p.EndProgress()
	buf.Reset()
	p.EndProgress()
	p.Step("done")

	if out := buf.String(); strings.Contains(out, "\r\033[K") {
		t.Errorf("erased a progress line that was already gone: %q", out)
	}
}

// A pipe or a log file has no cursor to move. Rewriting in place there writes
// escape codes into the log and loses every intermediate report, so the
// fallback is the plain appended line it always was.
func TestStepProgressFallsBackToPlainLinesWithoutATTY(t *testing.T) {
	isTTYOrig := isTTY
	defer func() { isTTY = isTTYOrig }()
	isTTY = false

	var buf bytes.Buffer
	p := NewPrinter("").WithWriter(&buf)

	p.StepProgress("Parsing: %d/%d file(s)", 1, 2)
	p.StepProgress("Parsing: %d/%d file(s)", 2, 2)

	out := buf.String()
	if strings.Contains(out, "\033[K") {
		t.Errorf("wrote a cursor escape into a non-terminal stream: %q", out)
	}
	if n := strings.Count(out, "\n"); n != 2 {
		t.Errorf("got %d line(s), want one per call so the log keeps the history: %q", n, out)
	}
}

// A line wider than the terminal wraps, and \r returns to the start of the
// last screen row only — the rows above it stay on screen as debris.
func TestProgressLineIsTruncatedToTheTerminal(t *testing.T) {
	isTTYOrig := isTTY
	defer func() { isTTY = isTTYOrig }()
	isTTY = true

	var buf bytes.Buffer
	p := NewPrinter("").WithWriter(&buf)

	p.StepProgress("%s", strings.Repeat("x", 500))

	// Stdout is not a terminal under `go test`, so termWidth falls back to 80.
	out := strings.TrimPrefix(buf.String(), "\r\033[K")
	if len([]rune(out)) > 80 {
		t.Errorf("progress line is %d columns wide, wider than the terminal: %q", len([]rune(out)), out)
	}
	if !strings.HasSuffix(out, "…") {
		t.Errorf("truncated without saying so: %q", out)
	}
}

func TestTruncateLeavesShortLinesAlone(t *testing.T) {
	if got := truncate("Parsing: 1/2 file(s)", 80); got != "Parsing: 1/2 file(s)" {
		t.Errorf("truncate mangled a line that fits: %q", got)
	}
	if got := truncate("abcdef", 4); got != "abc…" {
		t.Errorf("truncate(%q, 4) = %q, want %q", "abcdef", got, "abc…")
	}
	// Counted in runes, not bytes — a multi-byte line must not be cut mid-glyph.
	if got := truncate("ãéîõü", 3); got != "ãé…" {
		t.Errorf("truncate cut a multi-byte line by bytes: %q", got)
	}
}

func TestHumanBytesPicksTheUnit(t *testing.T) {
	tests := []struct {
		n    int64
		want string
	}{
		{0, "0 B"},
		{512, "512 B"},
		{1023, "1023 B"},
		{1024, "1.0 KB"},
		{1536, "1.5 KB"},
		{1024*1024 - 1, "1024.0 KB"},
		{1 << 20, "1.0 MB"},
		{138619279, "132.2 MB"},
	}
	for _, tt := range tests {
		if got := HumanBytes(tt.n); got != tt.want {
			t.Errorf("HumanBytes(%d) = %q; want %q", tt.n, got, tt.want)
		}
	}
}

func TestProgressBarIsExactlyWidthCellsWide(t *testing.T) {
	const width = 10
	for _, fraction := range []float64{0, 0.01, 0.5, 0.99, 1} {
		bar := ProgressBar(fraction, width)
		cells := utf8.RuneCountInString(strings.Trim(bar, "[]"))
		if cells != width {
			t.Errorf("ProgressBar(%v, %d) has %d cells; want %d (%q)", fraction, width, cells, width, bar)
		}
	}
}

func TestProgressBarClampsOutOfRangeFractions(t *testing.T) {
	empty := ProgressBar(0, 4)
	if got := ProgressBar(-0.5, 4); got != empty {
		t.Errorf("negative fraction = %q; want it clamped to %q", got, empty)
	}

	full := ProgressBar(1, 4)
	if got := ProgressBar(1.5, 4); got != full {
		t.Errorf("fraction above one = %q; want it clamped to %q", got, full)
	}
}

func TestProgressBarZeroWidthIsEmpty(t *testing.T) {
	if got := ProgressBar(0.5, 0); got != "" {
		t.Errorf("ProgressBar with zero width = %q; want empty", got)
	}
}

// An undeclared Content-Length is a real case, and the line has to stay honest
// about it rather than draw a bar it would have to invent a position for.
func TestDownloadLineWithoutATotalShowsOnlyBytesReceived(t *testing.T) {
	got := DownloadLine("Downloading model.onnx", 2<<20, 0, 24)

	if strings.Contains(got, "%") {
		t.Errorf("line claims a percentage with no total: %q", got)
	}
	if strings.Contains(got, barEmpty) || strings.Contains(got, barFilled) {
		t.Errorf("line draws a bar with no total: %q", got)
	}
	if !strings.Contains(got, "2.0 MB") {
		t.Errorf("line does not report what arrived: %q", got)
	}
}

func TestDownloadLineWithATotalShowsBarPercentAndBoth(t *testing.T) {
	got := DownloadLine("Downloading model.onnx", 1<<20, 4<<20, 8)

	for _, want := range []string{"Downloading model.onnx", "25%", "1.0 MB / 4.0 MB"} {
		if !strings.Contains(got, want) {
			t.Errorf("line %q is missing %q", got, want)
		}
	}
	if !strings.Contains(got, barFilled) {
		t.Errorf("line %q has no bar", got)
	}
}

// A log file gets the percentage but not the bar: the bar only means anything
// when it is rewritten in place.
func TestDownloadLineOmitsTheBarAtZeroWidth(t *testing.T) {
	got := DownloadLine("Downloading model.onnx", 1<<20, 4<<20, 0)

	if strings.Contains(got, barEmpty) || strings.Contains(got, barFilled) {
		t.Errorf("line drew a bar at zero width: %q", got)
	}
	if !strings.Contains(got, "25%") {
		t.Errorf("line dropped the percentage along with the bar: %q", got)
	}
}
