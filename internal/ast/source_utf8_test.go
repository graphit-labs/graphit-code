package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
	"unicode/utf8"
)

func TestSourceOfReturnsValidUTF8ForAMalformedFile(t *testing.T) {
	root := t.TempDir()
	const rel = "reports/broken.xml"
	if err := os.MkdirAll(filepath.Join(root, "reports"), 0o755); err != nil {
		t.Fatal(err)
	}
	raw := append([]byte("<report><title>Rela"), 0xE7, 0xE3, 'o')
	raw = append(raw, []byte("</title></report>\n")...)
	if utf8.Valid(raw) {
		t.Fatal("the fixture is supposed to be invalid UTF-8")
	}
	full := filepath.Join(root, rel)
	if err := os.WriteFile(full, raw, 0o644); err != nil {
		t.Fatal(err)
	}

	cache, err := NewShardCache(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = cache.Close() })
	cache.SetRoot(root)
	if err := cache.Store(rel, contentHashOf(raw), &parseCacheEntry{RelPath: rel, Language: "xml"}); err != nil {
		t.Fatal(err)
	}

	got := cache.SourceOf(rel)
	if got == "" {
		t.Fatal("the file hashes to what was parsed, so it must be readable")
	}
	if !utf8.ValidString(got) {
		t.Error("SourceOf returned invalid UTF-8; Arrow will reject the batch it lands in")
	}
	if !strings.Contains(got, "<report>") || !strings.Contains(got, "</report>") {
		t.Errorf("sanitising must replace the bad bytes, not truncate the file: %q", got)
	}
}

// The sweep of abandoned working directories must not touch one a CONCURRENT run is writing.
// The daemon indexes the same store the CLI does, and a partially deleted export publishes
// itself by rename: OBSERVED as a bundle dropping from 549 Parquet files to 175.
func TestStaleBundleSweepLeavesALiveWorkingDirectoryAlone(t *testing.T) {
	store := t.TempDir()
	finalDir := filepath.Join(store, "graph.icebug")

	live := finalDir + ".tmp.abc1234"
	if err := os.MkdirAll(live, 0o755); err != nil {
		t.Fatal(err)
	}
	abandoned := finalDir + ".tmp.def5678"
	if err := os.MkdirAll(abandoned, 0o755); err != nil {
		t.Fatal(err)
	}
	old := time.Now().Add(-2 * staleBundleTempAge)
	if err := os.Chtimes(abandoned, old, old); err != nil {
		t.Fatal(err)
	}

	removeStaleBundleTemps(finalDir)

	if _, err := os.Stat(live); err != nil {
		t.Error("the sweep deleted a working directory that was still being written")
	}
	if _, err := os.Stat(abandoned); err == nil {
		t.Error("the sweep left an abandoned working directory behind")
	}
}
