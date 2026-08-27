package daemon

import (
	"path/filepath"
	"reflect"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
)

// Routing a watcher batch to the right indexers is the step between "the OS told
// us a file changed" and "the index reflects it". Both of its rules are easy to
// get subtly wrong:
//
//   - knowledge.docs_dir can be set to ".", and then "is this path inside the
//     docs directory?" answers yes for every file in the project. Location alone
//     cannot decide what is documentation.
//   - the docs directory is not the whole of the wiki's scope: the root README is
//     indexed from outside it, so a path can be documentation without being
//     under docs/.
//   - the two destinations are not exclusive. The AST pipeline has parsers for
//     .yaml, .json, .xml and .proto, and a full scan indexes those files even
//     when they live under docs/. An incremental update has to agree with a full
//     scan. Markdown is deliberately not in that overlap: no shipped query file
//     claims .md, so a document routes to the wiki alone — unless the project opts
//     markdown back in, which is why the guard below checks the live table.
//
// requireParsers skips when the extension table does not match the one these
// tests route against.
//
// ast.HasParserForExtension answers from a table that initTsExtMap builds at
// package init out of the installed runtime query files alone. On a machine where
// the launcher has not unpacked a runtime — a fresh checkout, or a home directory
// that was cleared — that table is empty, every extension looks unparseable, and
// these tests fail with an empty astChanged that says nothing about the routing
// logic they exist to cover.
//
// The .md check is the same problem from the other side: no query file claims .md
// any more, and the cases below expect a document to route to the wiki alone. A
// runtime unpacked before markdown.yaml was dropped still registers .md, which
// would fail those cases for a reason that has nothing to do with routing.
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
			// The regression: with the default docs dir every path looked like
			// documentation, so nothing was ever handed to the AST pipeline and
			// the daemon stopped reindexing code entirely.
			name:      "code at the root, docs dir defaulted to the project root",
			docsDir:   ".",
			changed:   []string{"criada.sql"},
			wantAstCh: []string{abs("criada.sql")},
		},
		{
			// The overlap: a structured document has a grammar as well as a wiki
			// extension, and a full scan indexes it under docs/ too.
			name:       "a structured document under docs reaches both indexers",
			docsDir:    "docs",
			changed:    []string{"docs/openapi.yaml"},
			wantAstCh:  []string{abs("docs/openapi.yaml")},
			wantKnowdg: true,
		},
		{
			// .txt has no parser, so this one is documentation and nothing else.
			name:       "a docs file with no parser reaches knowledge only",
			docsDir:    "docs",
			changed:    []string{"docs/notas.txt"},
			wantKnowdg: true,
		},
		{
			// Same extension, outside the docs dir and not named by the scope. It is
			// not documentation, and no query file claims .md for it to be code
			// either, so nobody is woken up.
			name:    "markdown outside the docs dir reaches neither indexer",
			docsDir: "docs",
			changed: []string{"CONTRIBUTING.md"},
		},
		{
			// The root README is in the wiki's scope without being under docs/, so an
			// edit to it has to rebuild the wiki. Before extraDocs existed, editing
			// the README changed nothing the reader could see.
			name:       "the root README is documentation even from outside the docs dir",
			docsDir:    "docs",
			extraDocs:  []string{"README.md"},
			changed:    []string{"README.md"},
			wantKnowdg: true,
		},
		{
			// Only the exact paths the scope names — a README one directory down is
			// somebody else's file, and it is not code either.
			name:      "a nested README is not the project README",
			docsDir:   "docs",
			extraDocs: []string{"README.md"},
			changed:   []string{"internal/x/README.md"},
		},
		{
			// The parse cache is keyed by repo-relative slash paths.
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
			// The daemon writes its shards inside the tree it is watching, and a
			// shard is a .json file, which has a parser. Feeding one back into the
			// pipeline makes it emit a shard for the shard — a.sql.nodes.json
			// becomes a.sql.nodes.json.nodes.json — and the batch grows without
			// bound. A full scan never sees these files because discovery skips
			// dot-directories; the scoped path has to skip them too.
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
			// Only directory components count: discovery skips dot-dirs, not dotfiles.
			name:      "a dotfile that is not inside a dot directory is still source",
			docsDir:   ".",
			changed:   []string{".hidden.sql"},
			wantAstCh: []string{abs(".hidden.sql")},
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
