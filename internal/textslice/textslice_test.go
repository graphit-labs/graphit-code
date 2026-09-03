package textslice

import (
	"strings"
	"testing"
)

const probeText = `alfa granada
bravo xenolito
charlie granada
delta micaxisto
echo anfibolio
foxtrot granada`

func TestApplyWholeTextByDefault(t *testing.T) {
	t.Parallel()

	got, err := Apply(probeText, Request{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.Source != probeText {
		t.Errorf("a zero Request must return the text unchanged, got:\n%s", got.Source)
	}
	if got.TotalLines != 6 {
		t.Errorf("TotalLines = %d, want 6", got.TotalLines)
	}
	if got.StartLine != 1 || got.EndLine != 6 {
		t.Errorf("range = %d..%d, want 1..6", got.StartLine, got.EndLine)
	}
}

// The operations compose in a fixed order, and this is the case that pins it:
// head takes the first lines of the selected window, not of the whole text.
func TestApplyHeadAppliesToTheSelectedWindow(t *testing.T) {
	t.Parallel()

	got, err := Apply(probeText, Request{StartLine: 3, EndLine: 6, Head: 2})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	want := "charlie granada\ndelta micaxisto"
	if got.Source != want {
		t.Errorf("Source = %q, want %q", got.Source, want)
	}
	if got.StartLine != 3 || got.EndLine != 4 {
		t.Errorf("range = %d..%d, want 3..4", got.StartLine, got.EndLine)
	}
}

func TestApplyTailKeepsAbsoluteLineNumbers(t *testing.T) {
	t.Parallel()

	got, err := Apply(probeText, Request{Tail: 2, LineNumbers: true})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(got.Source, "   5: echo anfibolio") {
		t.Errorf("tail lost its absolute numbering, got:\n%s", got.Source)
	}
	if got.StartLine != 5 || got.EndLine != 6 {
		t.Errorf("range = %d..%d, want 5..6", got.StartLine, got.EndLine)
	}
}

// A range beyond the end is clamped rather than panicking: callers pass line
// numbers that came from an index which may be a revision behind the text.
func TestApplyClampsOutOfRangeRequests(t *testing.T) {
	t.Parallel()

	got, err := Apply(probeText, Request{StartLine: 5, EndLine: 900})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.EndLine != 6 {
		t.Errorf("EndLine = %d, want it clamped to 6", got.EndLine)
	}

	if _, err := Apply(probeText, Request{StartLine: 900}); err != nil {
		t.Errorf("a start beyond the end must clamp, not fail: %v", err)
	}
}

// Hits sit on lines 1, 3 and 6. With after=1 the windows are 1-2, 3-4 and 6, so
// line 5 is skipped: five lines out, and a separator where the gap is.
func TestSearchEmitsHitsWithContextAndMarksGaps(t *testing.T) {
	t.Parallel()

	got, err := Apply(probeText, Request{Pattern: "granada", After: 1})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(got.Matches) != 5 {
		t.Fatalf("got %d lines, want 5 (3 hits + 2 context, line 5 skipped)", len(got.Matches))
	}
	hits := 0
	for _, m := range got.Matches {
		if m.IsMatch {
			hits++
		}
	}
	if hits != 3 {
		t.Errorf("marked %d lines as hits, want 3", hits)
	}
	if !strings.Contains(got.Source, "---") {
		t.Errorf("line 5 was skipped, so a separator is required:\n%s", got.Source)
	}
}

// Widen the context until the windows join up: every line is covered, so there is
// no gap left to separate. Overlapping windows must merge, not repeat lines.
func TestSearchMergesOverlappingContextWindows(t *testing.T) {
	t.Parallel()

	got, err := Apply(probeText, Request{Pattern: "granada", After: 2})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(got.Matches) != 6 {
		t.Fatalf("got %d lines, want all 6 with no repeats", len(got.Matches))
	}
	seen := make(map[int]bool, len(got.Matches))
	for _, m := range got.Matches {
		if seen[m.LineNumber] {
			t.Errorf("line %d emitted twice — windows were not merged", m.LineNumber)
		}
		seen[m.LineNumber] = true
	}
	if strings.Contains(got.Source, "---") {
		t.Errorf("no lines were skipped, so no separator should appear:\n%s", got.Source)
	}
}

func TestSearchSeparatesNonAdjacentGroups(t *testing.T) {
	t.Parallel()

	got, err := Apply(probeText, Request{Pattern: "granada"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(got.Source, "---") {
		t.Errorf("non-adjacent hits must be separated, got:\n%s", got.Source)
	}
}

func TestSearchIsCaseInsensitiveUnlessRegex(t *testing.T) {
	t.Parallel()

	got, err := Apply(probeText, Request{Pattern: "GRANADA"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(got.Matches) != 3 {
		t.Errorf("plain pattern must match case-insensitively, got %d matches", len(got.Matches))
	}

	got, err = Apply(probeText, Request{Pattern: "^GRANADA", IsRegex: true})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(got.Matches) != 0 {
		t.Errorf("regex must not be silently case-folded, got %d matches", len(got.Matches))
	}
}

func TestSearchRejectsAnInvalidRegex(t *testing.T) {
	t.Parallel()

	if _, err := Apply(probeText, Request{Pattern: "granada(", IsRegex: true}); err == nil {
		t.Error("an unparsable regex must be reported, not silently treated as text")
	}
}

func TestApplyReportsNoMatchesWithEmptySource(t *testing.T) {
	t.Parallel()

	got, err := Apply(probeText, Request{Pattern: "wollastonita"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.Source != "" || len(got.Matches) != 0 {
		t.Errorf("a pattern with no hits must return nothing, got %q / %d matches", got.Source, len(got.Matches))
	}
}
