package ast

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Everything that indexes a project writes its output into the brand directory
// inside that same project, and every watcher over the project sees those writes.
// Feeding them back into the pipeline does not just waste work, it amplifies: a
// shard is a .json file, .json has a parser, so indexing a shard emits a shard
// for the shard, without bound.
//
// `graphit init` puts the directory in .gitignore and the checker honours that,
// but the injection is best-effort and the file belongs to the user. The knowledge
// side has defended itself by default for a while (DefaultKnowledgeIgnorePatterns);
// the AST side has to as well.
func TestAstIgnoreCheckerExcludesBrandDirByDefault(t *testing.T) {
	t.Parallel()

	root := t.TempDir()

	ic := NewAstIgnoreChecker(root)
	dot := brand.DotDir()

	if !ic.IsIgnored(dot, true) {
		t.Errorf("%s/ is not ignored by default", dot)
	}
	shard := dot + "/ast/project/shards/a.sql.nodes.json"
	if !ic.IsIgnored(shard, false) {
		t.Errorf("%s is not ignored by default — an indexer would index its own output", shard)
	}

	for _, keep := range []string{"a.sql", "src/b.go", ".hidden.sql"} {
		if ic.IsIgnored(keep, false) {
			t.Errorf("%s should not be ignored", keep)
		}
	}
}

// The lockfile is the same output as a shard, written outside the brand directory
// where that default cannot reach it. It is .json, .json has a parser, and it is
// rewritten on every install, sync and config change — so it churned the graph with
// Pair and Value nodes describing the indexer to itself.
//
// The pattern is anchored, and that is the half worth pinning: an unanchored
// gitignore entry matches at any depth, which would have swallowed the lockfile of
// a fixture project or a nested checkout — files that belong to whatever is being
// indexed, not to the framework.
func TestAstIgnoreCheckerExcludesTheFrameworkLockfileByDefault(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	ic := NewAstIgnoreChecker(root)
	lock := brand.LockFileName()

	if !ic.IsIgnored(lock, false) {
		t.Errorf("%s is indexed — the framework's own state churns the graph on every "+
			"sync and config write", lock)
	}
	if ic.IsIgnored("internal/hub/testdata/proj/"+lock, false) {
		t.Errorf("a nested %s was excluded — the pattern is not anchored to the project "+
			"root, so it reaches lockfiles that are not ours", lock)
	}
	for _, keep := range []string{lock + ".bak", "graphit.lock.json.tmpl"} {
		if ic.IsIgnored(keep, false) {
			t.Errorf("%s should not be ignored", keep)
		}
	}
}

// The documentation tree belongs to the knowledge wiki, which chunks, links and
// ranks prose in ways a code graph cannot. Indexing it on both sides bought a
// Heading node per section and noise in every structural query, so the AST side
// leaves knowledge.docs_dir alone unless ast.index_docs asks for it.
func TestAstIgnoreCheckerExcludesTheDocsTreeByDefault(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	ic := NewAstIgnoreChecker(root)

	if !ic.IsIgnored("docs", true) {
		t.Error("docs/ is indexed by the AST pipeline — the wiki already owns it")
	}
	if !ic.IsIgnored("docs/guia.md", false) {
		t.Error("a document under docs/ is indexed by the AST pipeline")
	}

	if ic.IsIgnored("internal/x/docs/nota.md", false) {
		t.Error("a nested docs/ directory was excluded — the pattern is not anchored")
	}
	if ic.IsIgnored("docs", false) {
		t.Error("a file named docs was excluded as if it were the directory")
	}
}

// ast.index_docs is the documented way back in, and it has to be the config key
// rather than a "!docs/" line in .astignore: the defaults are applied last, which
// makes them the highest-priority patterns, so a negation cannot outrank them.
func TestAstIndexDocsPutsTheDocsTreeBack(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	lockfile := `{"config":{"ast":{"index_docs":"true"}}}`
	if err := os.WriteFile(filepath.Join(root, brand.LockFileName()), []byte(lockfile), 0o644); err != nil {
		t.Fatal(err)
	}

	ic := NewAstIgnoreChecker(root)
	if ic.IsIgnored("docs/guia.md", false) {
		t.Error("ast.index_docs=true did not put the docs tree back in the graph")
	}
}

// A docs dir of "." is the whole project. Excluding it would exclude everything,
// so the exclusion has to stand down rather than empty the graph.
func TestDocsDirOfDotExcludesNothing(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	lockfile := `{"config":{"knowledge":{"docs_dir":"."}}}`
	if err := os.WriteFile(filepath.Join(root, brand.LockFileName()), []byte(lockfile), 0o644); err != nil {
		t.Fatal(err)
	}

	if p := DocsIgnorePatternFor(root); p != "" {
		t.Errorf("docs_dir=%q produced the pattern %q — it would exclude the whole project", ".", p)
	}

	ic := NewAstIgnoreChecker(root)
	for _, keep := range []string{"a.sql", "src/b.go", "docs/guia.md"} {
		if ic.IsIgnored(keep, false) {
			t.Errorf("%s should not be ignored when the docs dir is the project root", keep)
		}
	}
}

// A configured docs dir that is not "docs" is the one that gets excluded, nested
// path included — and "docs" then stops being special.
func TestCustomDocsDirIsWhatGetsExcluded(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	lockfile := `{"config":{"knowledge":{"docs_dir":"documentation/wiki"}}}`
	if err := os.WriteFile(filepath.Join(root, brand.LockFileName()), []byte(lockfile), 0o644); err != nil {
		t.Fatal(err)
	}

	ic := NewAstIgnoreChecker(root)
	if !ic.IsIgnored("documentation/wiki/guia.md", false) {
		t.Error("the configured docs dir was not excluded")
	}
	if ic.IsIgnored("documentation/outra.md", false) {
		t.Error("a sibling of the configured docs dir was excluded too")
	}
	if ic.IsIgnored("docs/guia.md", false) {
		t.Error("docs/ was excluded even though the project keeps its documentation elsewhere")
	}
}

// A project's own ignore files still layer on top of the default.
func TestAstIgnoreCheckerStillReadsProjectIgnoreFiles(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, AstIgnoreFile), []byte("gerado/\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	ic := NewAstIgnoreChecker(root)
	if !ic.IsIgnored("gerado/x.sql", false) {
		t.Error(AstIgnoreFile + " patterns stopped being honoured")
	}
	if !ic.IsIgnored(brand.DotDir(), true) {
		t.Error("the default was lost once a project ignore file existed")
	}
}
