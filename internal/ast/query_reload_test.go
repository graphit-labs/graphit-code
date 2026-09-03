package ast

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func writeQueryFile(t *testing.T, dir, langName, ext string) {
	t.Helper()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	yaml := "language: " + langName + "\n" +
		"grammar: tree-sitter-go\n" +
		"extensions: [\"" + ext + "\"]\n" +
		"queries:\n" +
		"  - data_key: functions\n" +
		"    graph_label: Function\n" +
		"    pattern: '(function_declaration name: (identifier) @name)'\n"
	if err := os.WriteFile(filepath.Join(dir, langName+".yaml"), []byte(yaml), 0o644); err != nil {
		t.Fatal(err)
	}
}

// A language added to a project after the process has already looked at that
// project must become visible without a restart.
func TestProjectQueryAddedAtRuntimeIsPickedUp(t *testing.T) {
	const ext, langName = ".laterlang", "laterlang"

	projectDir := t.TempDir()
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		t.Fatal(err)
	}

	if HasParserForExtensionIn(projectDir, ext) {
		t.Fatalf("%s is somehow already known", ext)
	}

	writeQueryFile(t, qdir, langName, ext)

	deadline := time.Now().Add(queryStaleCheckInterval + 3*time.Second)
	for time.Now().Before(deadline) {
		if HasParserForExtensionIn(projectDir, ext) {
			return
		}
		time.Sleep(200 * time.Millisecond)
	}
	t.Errorf("a query file added at runtime was never picked up — a long-lived daemon "+
		"would keep ignoring %s until restarted", ext)
}

// Editing a query file in place must count as a change. The directory's own
// mtime does not move when a file is rewritten, which is why the signature
// includes each file's size and mtime.
func TestEditedQueryFileIsReloaded(t *testing.T) {
	const langName, extA, extB = "editlang", ".editlanga", ".editlangb"

	projectDir := t.TempDir()
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	writeQueryFile(t, qdir, langName, extA)

	if !HasParserForExtensionIn(projectDir, extA) {
		t.Skip("the staged language did not register; covered by TestProjectQueryFileRegistersItsExtension")
	}

	writeQueryFile(t, qdir, langName, extB)

	deadline := time.Now().Add(queryStaleCheckInterval + 3*time.Second)
	for time.Now().Before(deadline) {
		if HasParserForExtensionIn(projectDir, extB) {
			if HasParserForExtensionIn(projectDir, extA) {
				t.Errorf("%s is still registered after the file stopped claiming it", extA)
			}
			return
		}
		time.Sleep(200 * time.Millisecond)
	}
	t.Errorf("an in-place edit was not noticed — the signature is not sensitive to content")
}

// InvalidateQueryCaches is the hook for code that knows it changed something,
// so a grammar install does not have to wait out the staleness interval.
func TestInvalidateQueryCachesIsImmediate(t *testing.T) {
	const ext, langName = ".instantlang", "instantlang"

	projectDir := t.TempDir()
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		t.Fatal(err)
	}
	if HasParserForExtensionIn(projectDir, ext) {
		t.Fatalf("%s is somehow already known", ext)
	}

	writeQueryFile(t, qdir, langName, ext)
	InvalidateQueryCaches()

	if !HasParserForExtensionIn(projectDir, ext) {
		t.Errorf("InvalidateQueryCaches did not make %s visible straight away", ext)
	}
}
