package wiki

import (
	"os"
	"path/filepath"
	"testing"
)

// writeFile drops a file into a test directory.
//
// It lived in bm25_test.go until the Go BM25 index over markdown pages was deleted; the
// cross-reference tests still build page fixtures, because a wikilink is parsed out of text
// regardless of whether that text arrived from a file or from a column.
func writeFile(t *testing.T, dir, name, content string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
		t.Fatalf("writeFile(%q): %v", name, err)
	}
}
