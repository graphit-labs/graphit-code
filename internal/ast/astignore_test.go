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

	// No .gitignore and no .astignore: the default has to carry this alone.
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

	// The default must not be so broad that it swallows real source.
	for _, keep := range []string{"a.sql", "src/b.go", "docs/guia.md", ".hidden.sql"} {
		if ic.IsIgnored(keep, false) {
			t.Errorf("%s should not be ignored", keep)
		}
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
