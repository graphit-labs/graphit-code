package daemon

import (
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
)

// The three incremental paths do NOT share one path convention, and conflating them is how
// the absolute-path duplicate File node got in. This file pins each one's contract so a
// later change cannot quietly swap them:
//
//   - AST: both lists are REPO-RELATIVE, because the parse cache and the graph are keyed
//     that way. An absolute path here becomes a second File node for a file already in the
//     graph, and no error is raised.
//   - Knowledge: no path list at all — a batch only decides WHETHER to rebuild, and the
//     wiki derives every source path from the project root itself.
//   - Memory: the batch paths stay ABSOLUTE, because they are only ever a predicate over
//     scope directories, never something that gets stored.
func TestClassifyBatchEmitsRepoRelativeASTPaths(t *testing.T) {
	t.Parallel()
	requireParsers(t)

	const projectDir = "/proj"
	abs := func(rel string) string { return filepath.Join(projectDir, filepath.FromSlash(rel)) }
	knowledgeExts := config.ResolveKnowledgeExtensions(nil, nil)

	batch := fswatch.Batch{
		Changed: []string{abs("internal/a.sql"), abs("cmd/b.sql")},
		Removed: []string{abs("internal/gone.sql")},
	}

	got := classifyBatch(batch, projectDir, filepath.Join(projectDir, "docs"),
		knowledgeExts, nil, nil, nil)

	for _, p := range append(append([]string{}, got.astChanged...), got.astRemoved...) {
		if filepath.IsAbs(p) {
			t.Errorf("classifyBatch handed the pipeline an absolute path %q; the graph is keyed by repo-relative paths, "+
				"and an absolute one silently creates a duplicate File node", p)
		}
	}
	if len(got.astChanged) != 2 || len(got.astRemoved) != 1 {
		t.Fatalf("unexpected classification: changed=%v removed=%v", got.astChanged, got.astRemoved)
	}
	if got.astChanged[0] != "internal/a.sql" || got.astRemoved[0] != "internal/gone.sql" {
		t.Errorf("paths are not repo-relative slash form: changed=%v removed=%v", got.astChanged, got.astRemoved)
	}
}

// Memory's contract is the opposite one, on purpose: anyUnder answers "does this batch touch
// this scope directory", which only works on absolute paths. Nothing from the batch is ever
// persisted, so there is no relative form to convert to — and converting would break the
// scope selection instead of fixing anything.
func TestMemoryScopeSelectionUsesAbsoluteBatchPaths(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	scope := filepath.Join(root, "project-01ABC")

	if !anyUnder([]string{filepath.Join(scope, "memories", "m.md")}, scope) {
		t.Error("an absolute path inside the scope was not detected, so the scope would never recompile")
	}
	if anyUnder([]string{filepath.Join(root, "other-scope", "m.md")}, scope) {
		t.Error("a path in a sibling scope was treated as inside this one")
	}
}
