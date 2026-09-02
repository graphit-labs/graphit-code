//go:build lancedb

package knowledge

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func publishFrom(t *testing.T, docs map[string]string) string {
	t.Helper()

	root := t.TempDir()
	for rel, body := range docs {
		p := filepath.Join(root, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	wikiDir := filepath.Join(t.TempDir(), "compiled")
	if _, err := RunIndexPipeline(context.Background(), root, wikiDir, IndexConfig{}); err != nil {
		t.Fatalf("compiling: %v", err)
	}

	published := filepath.Join(t.TempDir(), "branch-wiki")
	if err := os.MkdirAll(published, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := copyExcept(wikiDir, published, wiki.IsDerivedFile); err != nil {
		t.Fatalf("publishing: %v", err)
	}
	return published
}

func copyExcept(src, dst string, skip func(string) bool) error {
	return filepath.WalkDir(src, func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		rel, relErr := filepath.Rel(src, path)
		if relErr != nil {
			return relErr
		}
		if skip(filepath.ToSlash(rel)) {
			return nil
		}
		data, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		target := filepath.Join(dst, rel)
		if mkErr := os.MkdirAll(filepath.Dir(target), 0o755); mkErr != nil {
			return mkErr
		}
		return os.WriteFile(target, data, 0o644)
	})
}

// The end-to-end property: a producer publishes its compiled wiki, the sources never
// travel, and the consumer ends up with a searchable context anyway.
//
// This is what the whole change is for. Before it, the branch carried the docs tree and
// the consumer threw the compiled half away and re-ran the generator — paying for the
// embedding model a second time over text whose vectors had just been downloaded.
func TestAPublishedWikiInstallsWithoutItsSources(t *testing.T) {
	isolateHome(t)

	published := publishFrom(t, map[string]string{
		"docs/overview.md": "# Overview\n\nAcme ships widgets to three continents.\n",
		"docs/billing.md":  "# Billing\n\nBilling handles invoices and dunning.\n",
	})

	// What travels is the shard tree, and never the index — it is rebuilt from the shards.
	// Named by the constant, because asserting a literal file name that no longer exists is an
	// assertion that passes for the wrong reason.
	if _, err := os.Stat(filepath.Join(published, wiki.WikiIndexDirName)); !os.IsNotExist(err) {
		t.Error("the rebuildable index was published")
	}
	if _, err := os.Stat(filepath.Join(published, "shards")); err != nil {
		t.Fatalf("no shards were published, so the consumer would have to recompile: %v", err)
	}

	// The consumer places it and indexes it. No source tree exists anywhere.
	const name = "acme-docs"
	dir, err := ResetContextWiki(name)
	if err != nil {
		t.Fatal(err)
	}
	if err := copyExcept(published, dir, func(string) bool { return false }); err != nil {
		t.Fatal(err)
	}
	chunks, err := IndexContextWiki(context.Background(), name)
	if err != nil {
		t.Fatalf("IndexContextWiki: %v", err)
	}
	if chunks == 0 {
		t.Fatal("nothing was indexed from the published shards")
	}

	// And it is searchable, through the same resolver every reader uses.
	projectDir := t.TempDir()
	if err := store.AddContext(projectDir, store.KindKnowledge, store.ContextRecord{Name: name}); err != nil {
		t.Fatal(err)
	}
	resolved := WikiDirForContextIn(projectDir, name)
	if resolved != store.KnowledgeContextDir(name) {
		t.Errorf("resolved to %q, want the context directory itself — there is one shape now", resolved)
	}
	results := wiki.BM25Search(context.Background(), resolved, "widgets continents", 5)
	if len(results) == 0 {
		t.Fatal("a term from the published documentation did not match")
	}
	if got := InstalledContextsIn(projectDir); len(got) != 1 || got[0] != name {
		t.Errorf("InstalledContextsIn = %v, want [%s]", got, name)
	}
}

// A re-install must not leave a page the publisher deleted, which is why the context
// directory is cleared rather than merged into.
func TestReinstallDropsAPageThePublisherDeleted(t *testing.T) {
	isolateHome(t)
	const name = "acme-docs"

	first := publishFrom(t, map[string]string{
		"docs/keep.md": "# Keep\n\nThis one stays.\n",
		"docs/gone.md": "# Gone\n\nThis one is removed upstream.\n",
	})
	dir, err := ResetContextWiki(name)
	if err != nil {
		t.Fatal(err)
	}
	if err := copyExcept(first, dir, func(string) bool { return false }); err != nil {
		t.Fatal(err)
	}
	if _, err := IndexContextWiki(context.Background(), name); err != nil {
		t.Fatal(err)
	}
	if !hasPage(t, dir, "Gone") {
		t.Fatal("the fixture did not produce the page this test is about")
	}

	second := publishFrom(t, map[string]string{
		"docs/keep.md": "# Keep\n\nThis one stays.\n",
	})
	dir, err = ResetContextWiki(name)
	if err != nil {
		t.Fatal(err)
	}
	if err := copyExcept(second, dir, func(string) bool { return false }); err != nil {
		t.Fatal(err)
	}
	if _, err := IndexContextWiki(context.Background(), name); err != nil {
		t.Fatal(err)
	}

	if hasPage(t, dir, "Gone") {
		t.Error("a page the publisher deleted survived the re-install")
	}
	if !hasPage(t, dir, "Keep") {
		t.Error("the surviving page was lost")
	}
}

// hasPage asks the INDEX whether a context holds a page whose slug carries the fragment.
//
// It used to look for `<something><fragment><something>.md` in the directory, which is the shape the
// publication no longer has: it carries the tables and nothing else, because a page is read out of
// `chunks.body` rather than opened as a file.
func hasPage(t *testing.T, dir, slugFragment string) bool {
	t.Helper()
	for _, slug := range wiki.ListPagesAt(context.Background(), dir) {
		if strings.Contains(slug, slugFragment) {
			return true
		}
	}
	return false
}

// A context installed from a publication that carried no shards indexes nothing, and
// the caller has to be able to tell — an empty index answers every query with "no
// results" for a reason that has nothing to do with the query.
func TestAnEmptyPublicationIndexesNothing(t *testing.T) {
	isolateHome(t)
	const name = "empty-docs"
	if _, err := ResetContextWiki(name); err != nil {
		t.Fatal(err)
	}
	chunks, err := IndexContextWiki(context.Background(), name)
	if err != nil {
		t.Fatalf("an empty publication is not an error: %v", err)
	}
	if chunks != 0 {
		t.Errorf("indexed %d chunks from nothing, want 0", chunks)
	}
}

// The context wiki is global and keyed by name, so two projects that import the same
// context share one copy — and the registry is what says which of them may query it.
func TestTwoProjectsShareOneContextCopy(t *testing.T) {
	home := isolateHome(t)
	const name = "shared-docs"

	dir, err := ResetContextWiki(name)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(dir, filepath.Join(home, brand.DotDir())) {
		t.Errorf("context wiki at %q is not under the global dir", dir)
	}

	a, b := t.TempDir(), t.TempDir()
	if err := store.AddContext(a, store.KindKnowledge, store.ContextRecord{Name: name}); err != nil {
		t.Fatal(err)
	}
	if WikiDirForContextIn(a, name) != WikiDirForContextIn(b, name) {
		t.Error("two projects resolved the same context to different directories")
	}
	if got := InstalledContextsIn(b); len(got) != 0 {
		t.Errorf("a project that imported nothing reported %v", got)
	}
}
