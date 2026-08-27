package ast

import (
	"strings"
	"testing"
)

// The File table carries every file's full source, so a single-shot COPY meant one
// JSON document holding the whole repository — 2.4 GB on a 36k-file Oracle export,
// with individual files of 133 MB. That COPY failed and the File table came out
// empty. batchRows is what bounds it, and it has to bound by BYTES: row sizes here
// span six orders of magnitude, so a row count cannot express the limit.
func TestBatchRowsSplitsByPayloadSizeNotRowCount(t *testing.T) {
	rows := []map[string]any{
		{"path": "a", "source": strings.Repeat("x", 900)},
		{"path": "b", "source": strings.Repeat("x", 900)},
		{"path": "c", "source": strings.Repeat("x", 900)},
	}

	batches := batchRows(rows, 2000)
	if len(batches) < 2 {
		t.Fatalf("expected the rows to be split, got %d batch(es)", len(batches))
	}

	var seen int
	for _, b := range batches {
		if len(b) == 0 {
			t.Error("a batch must never be empty")
		}
		seen += len(b)
	}
	if seen != len(rows) {
		t.Errorf("batching lost rows: %d in, %d out", len(rows), seen)
	}
}

// A row bigger than the budget still has to be copied — the point is to bound the
// document, not to silently drop content that does not fit.
func TestBatchRowsKeepsAnOversizedRow(t *testing.T) {
	huge := map[string]any{"path": "big", "source": strings.Repeat("x", 10_000)}
	rows := []map[string]any{
		{"path": "small", "source": "x"},
		huge,
		{"path": "small2", "source": "x"},
	}

	batches := batchRows(rows, 100)

	var seen int
	var foundHuge bool
	for _, b := range batches {
		for _, r := range b {
			seen++
			if r["path"] == "big" {
				foundHuge = true
			}
		}
	}
	if seen != len(rows) {
		t.Errorf("batching lost rows: %d in, %d out", len(rows), seen)
	}
	if !foundHuge {
		t.Error("the oversized row was dropped instead of getting its own batch")
	}
}

func TestBatchRowsEdgeCases(t *testing.T) {
	if got := batchRows(nil, copyBatchBytes); got != nil {
		t.Errorf("nil input must yield no batches, got %d", len(got))
	}
	if got := batchRows([]map[string]any{}, copyBatchBytes); got != nil {
		t.Errorf("empty input must yield no batches, got %d", len(got))
	}

	rows := []map[string]any{{"a": 1}, {"b": 2}}
	// A non-positive budget means "do not split", not "split into nothing".
	if got := batchRows(rows, 0); len(got) != 1 || len(got[0]) != 2 {
		t.Errorf("a zero budget must pass the rows through in one batch, got %#v", got)
	}

	// Well under the real budget: one batch, so the common case pays nothing.
	if got := batchRows(rows, copyBatchBytes); len(got) != 1 {
		t.Errorf("small input must stay in a single batch, got %d", len(got))
	}
}

// estimateRowBytes must track the strings, since they are the only values that can
// be arbitrarily large and therefore the only ones that can break a COPY.
func TestEstimateRowBytesTracksStringSize(t *testing.T) {
	small := estimateRowBytes(map[string]any{"source": "x"})
	big := estimateRowBytes(map[string]any{"source": strings.Repeat("x", 10_000)})

	if big-small < 9_000 {
		t.Errorf("string length is not being measured: small=%d big=%d", small, big)
	}
	if small <= 0 {
		t.Errorf("a row must have a non-zero estimate, got %d", small)
	}
}
