package daemon

import (
	"path/filepath"
	"reflect"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
)

func requireParsers(t *testing.T) {
	t.Helper()
	for _, ext := range []string{".sql", ".go", ".yaml"} {
		if !ast.HasParserForExtension(ext) {
			t.Skipf("no parser registered for %s — the runtime query files are not "+
				"installed, so extension routing cannot be judged from here", ext)
		}
	}
	if ast.HasParserForExtension(".md") {
		t.Skip("the installed runtime still registers .md — it predates the removal of " +
			"markdown.yaml, so document routing cannot be judged from here")
	}
}

func TestClassifyBatch(t *testing.T) {
	t.Parallel()
	requireParsers(t)

	const projectDir = "/proj"
	knowledgeExts := config.ResolveKnowledgeExtensions(nil, nil)

	abs := func(rel string) string { return filepath.Join(projectDir, filepath.FromSlash(rel)) }

	tests := []struct {
		name       string
		docsDir    string
		extraDocs  []string
		changed    []string
		removed    []string
		wantAstCh  []string
		wantAstRm  []string
		wantKnowdg bool
	}{
		{
			name:      "code at the root, docs dir defaulted to the project root",
			docsDir:   ".",
			changed:   []string{"criada.sql"},
			wantAstCh: []string{"criada.sql"},
		},
		{
			name:       "a structured document under docs reaches both indexers",
			docsDir:    "docs",
			changed:    []string{"docs/openapi.yaml"},
			wantAstCh:  []string{"docs/openapi.yaml"},
			wantKnowdg: true,
		},
		{
			name:       "a docs file with no parser reaches knowledge only",
			docsDir:    "docs",
			changed:    []string{"docs/notas.txt"},
			wantKnowdg: true,
		},
		{
			name:    "markdown outside the docs dir reaches neither indexer",
			docsDir: "docs",
			changed: []string{"CONTRIBUTING.md"},
		},
		{
			name:       "the root README is documentation even from outside the docs dir",
			docsDir:    "docs",
			extraDocs:  []string{"README.md"},
			changed:    []string{"README.md"},
			wantKnowdg: true,
		},
		{
			name:      "a nested README is not the project README",
			docsDir:   "docs",
			extraDocs: []string{"README.md"},
			changed:   []string{"internal/x/README.md"},
		},
		{
			name:      "removals are reported relative to the project, slash separated",
			docsDir:   ".",
			removed:   []string{"pkg/velha.sql"},
			wantAstRm: []string{"pkg/velha.sql"},
		},
		{
			name:       "removing a doc still schedules a knowledge rebuild",
			docsDir:    "docs",
			removed:    []string{"docs/antigo.txt"},
			wantKnowdg: true,
		},
		{
			name:    "a file nothing can parse reaches neither indexer",
			docsDir: ".",
			changed: []string{"notas.xyz"},
		},
		{
			name:    "the tool's own state directory is never treated as source",
			docsDir: ".",
			changed: []string{".graphit/ast/project/shards/a.sql.nodes.json"},
			removed: []string{".graphit/ast/project/shards/b.sql.edges.json"},
		},
		{
			name:    "dot directories in general are skipped, as a full scan skips them",
			docsDir: ".",
			changed: []string{".venv/lib/mod.py", ".cache/x.sql"},
		},
		{
			name:      "a dotfile that is not inside a dot directory is still source",
			docsDir:   ".",
			changed:   []string{".hidden.sql"},
			wantAstCh: []string{".hidden.sql"},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			toAbs := func(rels []string) []string {
				out := make([]string, 0, len(rels))
				for _, r := range rels {
					out = append(out, abs(r))
				}
				return out
			}
			batch := fswatch.Batch{Changed: toAbs(tc.changed), Removed: toAbs(tc.removed)}
			docsPath := filepath.Join(projectDir, tc.docsDir)

			got := classifyBatch(batch, projectDir, docsPath, knowledgeExts, nil, nil, tc.extraDocs)

			if !reflect.DeepEqual(got.astChanged, tc.wantAstCh) {
				t.Errorf("astChanged = %v, want %v", got.astChanged, tc.wantAstCh)
			}
			if !reflect.DeepEqual(got.astRemoved, tc.wantAstRm) {
				t.Errorf("astRemoved = %v, want %v", got.astRemoved, tc.wantAstRm)
			}
			if got.knowledge != tc.wantKnowdg {
				t.Errorf("knowledge = %v, want %v", got.knowledge, tc.wantKnowdg)
			}
		})
	}
}
