package commands

import (
	"bytes"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/output"
)

// `ast index` printed the grammar overrides and then nothing until it finished.
// The pipeline had been emitting progress the whole time; no caller consumed it.
// On a 36k-file repository that was 16 minutes of silence, which is how a real
// hang went unnoticed for a day.
func TestProgressThrottlePrintsTheFirstCallOfAPhase(t *testing.T) {
	th := progressThrottle{every: 10 * time.Second}
	now := time.Unix(0, 0)

	if !th.allow("parsing", now) {
		t.Error("the first line of a phase was suppressed")
	}
}

func TestProgressThrottleSuppressesWithinTheInterval(t *testing.T) {
	th := progressThrottle{every: 10 * time.Second}
	now := time.Unix(0, 0)
	th.allow("parsing", now)

	for i := 1; i < 5000; i++ {
		if th.allow("parsing", now.Add(time.Duration(i)*time.Millisecond)) {
			t.Fatalf("printed again after %dms, want silence until 10s", i)
		}
	}
	if !th.allow("parsing", now.Add(10*time.Second)) {
		t.Error("still silent once the interval elapsed")
	}
}

// On a terminal the progress line is rewritten in place, so the 10s interval
// that keeps a log readable only makes a live counter look frozen. Off a
// terminal every refresh really is another line, and the coarse interval is
// the only thing keeping 36k files from becoming 36k lines.
func TestProgressIntervalIsFastOnlyWhenThereIsACursorToMove(t *testing.T) {
	if got := progressInterval(true); got >= time.Second {
		t.Errorf("on a TTY the in-place line refreshes every %v, too slow to read as progress", got)
	}
	if got := progressInterval(false); got != 10*time.Second {
		t.Errorf("off a TTY got %v, want the coarse 10s that keeps the log readable", got)
	}
}

// File cost here spans four orders of magnitude — a 200-line PL/SQL package
// against a 1.3M-line XML — so the interval has to be wall-clock, not a file
// count. One slow file must not buy silence beyond the interval.
func TestProgressThrottleMeasuresWallClockNotCalls(t *testing.T) {
	th := progressThrottle{every: 10 * time.Second}
	now := time.Unix(0, 0)
	th.allow("parsing", now)

	if !th.allow("parsing", now.Add(11*time.Second)) {
		t.Error("a single slow file kept the reporter quiet past the interval")
	}
}

// The write phase was 424s of the 970s and had no callback at all. When it
// finally got one, it had to be able to speak immediately — otherwise the
// transition from parsing to writing is precisely the moment that stays silent.
func TestProgressThrottleAlwaysPrintsAPhaseChange(t *testing.T) {
	th := progressThrottle{every: 10 * time.Second}
	now := time.Unix(0, 0)
	th.allow("parsing", now)

	if !th.allow("writing", now.Add(time.Millisecond)) {
		t.Error("the phase change was throttled away")
	}
	if th.allow("writing", now.Add(2*time.Millisecond)) {
		t.Error("the new phase did not start throttling")
	}
}

func TestIndexProgressReporterNamesEveryPostParsePhase(t *testing.T) {
	var buf bytes.Buffer
	p := output.NewPrinterTo("", &buf)
	report := indexProgressReporter(p)

	report("saving-cache", 0, 848, 0)
	report("writing", 0, 848, 0)
	report("searching", 0, 848, 0)
	report("search-maintenance", 0, 0, 0)
	p.EndProgress()

	got := buf.String()
	for _, want := range []string{
		"Saving parse cache: 848 file(s)",
		"Writing graph: 848 file(s)",
		"Building search index: 848 file(s)",
		"Maintaining search index",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("progress output %q does not contain %q", got, want)
		}
	}
}
