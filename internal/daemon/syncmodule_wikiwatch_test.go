package daemon

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
)

// The AST honours .astignore and the wiki honours .wikiignore, each inside its
// own pipeline. But the daemon runs one watcher, and it was built from the AST
// checker alone, so .astignore decided whether the wiki heard about anything at
// all. Excluding docs/ from AST parsing left the directory unwatched and the wiki
// never rebuilt.
//
// These tests stage their own project query files, so the extensions they route
// on are the ones staged here rather than whatever the installed runtime holds —
// which is why .md still has a parser below even though the framework ships no
// markdown query file.
//
// The watch is now the union of what the two want, and each applies its own file
// to what arrives.

// stageProjectParsers writes query files declaring the extensions these tests
// route on, into the project's own .graphit/ast/queries. Project query files
// register their extensions, so this makes the tests independent of whatever the
// launcher has unpacked into the developer's home — no runtime, no skip.
func stageProjectParsers(t *testing.T, dir string) {
	t.Helper()
	qdir := filepath.Join(dir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		t.Fatal(err)
	}
	for _, l := range []struct{ name, ext string }{
		{"sqlish", ".sql"}, {"goish", ".go"}, {"mdish", ".md"},
	} {
		yaml := "language: " + l.name + "\n" +
			"grammar: tree-sitter-go\n" +
			"extensions: [\"" + l.ext + "\"]\n" +
			"queries:\n" +
			"  - data_key: functions\n" +
			"    graph_label: Function\n" +
			"    pattern: '(function_declaration name: (identifier) @name)'\n"
		if err := os.WriteFile(filepath.Join(qdir, l.name+".yaml"), []byte(yaml), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	ast.InvalidateQueryCaches()
	t.Cleanup(ast.InvalidateQueryCaches)
}

func stageIgnores(t *testing.T, astIgnore, wikiIgnore string) string {
	t.Helper()
	dir := t.TempDir()
	stageProjectParsers(t, dir)
	if astIgnore != "" {
		if err := os.WriteFile(filepath.Join(dir, ast.AstIgnoreFile), []byte(astIgnore), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if wikiIgnore != "" {
		if err := os.WriteFile(filepath.Join(dir, knowledge.KnowledgeIgnoreFile),
			[]byte(wikiIgnore), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

func classifyIn(t *testing.T, projectDir string, changed ...string) batchTargets {
	t.Helper()
	abs := make([]string, 0, len(changed))
	for _, c := range changed {
		abs = append(abs, filepath.Join(projectDir, filepath.FromSlash(c)))
	}
	scope := knowledge.ScopeFor(projectDir, nil, nil)
	return classifyBatch(
		fswatch.Batch{Changed: abs},
		projectDir,
		filepath.Join(projectDir, scope.Subdir),
		config.ResolveKnowledgeExtensions(nil, nil),
		ast.NewAstIgnoreChecker(projectDir),
		knowledge.NewKnowledgeIgnoreChecker(projectDir),
		scope.ExtraFiles,
	)
}

// The case that was broken: docs excluded from AST parsing must still rebuild
// the wiki.
func TestDocsExcludedFromAstStillReachTheWiki(t *testing.T) {
	projectDir := stageIgnores(t, "docs/\n", "")

	got := classifyIn(t, projectDir, "docs/guia.md")

	if !got.knowledge {
		t.Error("a document under a directory excluded by .astignore did not schedule a " +
			"wiki rebuild — .astignore is still deciding for the wiki")
	}
	if len(got.astChanged) != 0 {
		t.Errorf("the AST was handed %v despite .astignore excluding docs/", got.astChanged)
	}
}

// And the mirror image: what .wikiignore excludes must still reach the AST.
func TestFilesExcludedFromWikiStillReachTheAst(t *testing.T) {
	projectDir := stageIgnores(t, "", "gerado/\n")

	got := classifyIn(t, projectDir, "gerado/consulta.sql")

	if got.knowledge {
		t.Error(".wikiignore excluded the directory but a wiki rebuild was scheduled anyway")
	}
	if len(got.astChanged) != 1 {
		t.Errorf("the AST was handed %v, want the one .sql file — .wikiignore is deciding "+
			"for the AST", got.astChanged)
	}
}

// A path both files exclude reaches neither.
func TestPathExcludedByBothReachesNobody(t *testing.T) {
	projectDir := stageIgnores(t, "vendor/\n", "vendor/\n")

	got := classifyIn(t, projectDir, "vendor/lib.md")

	if got.knowledge || len(got.astChanged) != 0 {
		t.Errorf("a path excluded by both files was still routed: knowledge=%v ast=%v",
			got.knowledge, got.astChanged)
	}
}

// ignoreUnion decides what is watched at all. It must skip a path only when
// every member skips it, and must still skip the brand directory, which both
// exclude by default and which the daemon writes into.
func TestIgnoreUnionWatchesWhatEitherWants(t *testing.T) {
	projectDir := stageIgnores(t, "docs/\n", "gerado/\n")
	u := ignoreUnion{
		ast.NewAstIgnoreChecker(projectDir),
		knowledge.NewKnowledgeIgnoreChecker(projectDir),
	}

	cases := []struct {
		path     string
		isDir    bool
		wantSkip bool
		reason   string
	}{
		{"docs", true, false, "only .astignore excludes it; the wiki still wants it"},
		{"gerado", true, false, "only .wikiignore excludes it; the AST still wants it"},
		{"src", true, false, "neither excludes it"},
		{".graphit", true, true, "both exclude it, and the daemon writes into it"},
	}
	for _, tc := range cases {
		if got := u.IsIgnored(tc.path, tc.isDir); got != tc.wantSkip {
			t.Errorf("IsIgnored(%q) = %v, want %v — %s", tc.path, got, tc.wantSkip, tc.reason)
		}
	}
}
